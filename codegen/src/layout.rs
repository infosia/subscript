//! C-ABI data layout (design invariant 1) and the HIR-type to CLIF
//! mapping shared by the whole lowering.
//!
//! Every language-visible aggregate (value class, `FixedArray`, the
//! coroutine step result `{ done, value }`) is laid out exactly as the
//! platform C ABI lays out the equivalent C struct: fields in
//! declaration order, each at the next multiple of its natural
//! alignment, struct alignment = max field alignment, size rounded up
//! to the alignment. All target platforms (x86-64, arm64) share these
//! natural alignments.
//!
//! Every lookup is fallible: an out-of-range class id or a value-class
//! containment cycle is reported as an internal error, never masked by
//! a fallback layout.

use cranelift_codegen::ir::types;
use std::ops::Range;
use subscript_compiler::hir;
use subscript_compiler::lir;
use subscript_compiler::types::{
    scalar_size_align as compiler_scalar_size_align, HandleClass, HandleKind, MAX_AGGREGATE_BYTES,
};
use subscript_compiler::Type;

use crate::lower::internal;

/// How a language type is represented in generated code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Repr {
    /// No value (`void`).
    None,
    /// One CLIF scalar (`I8`, `I16`, `I32`, `I64`, `F32`, `F64`, or a
    /// pointer-sized handle).
    Scalar(types::Type),
    /// A function value: `(code pointer, environment pointer)`.
    Pair,
    /// A by-value aggregate handled through a pointer to storage:
    /// value classes, `FixedArray`, coroutine step results.
    Agg { size: u32, align: u32 },
}

/// Layout of one class.
#[derive(Debug, Clone)]
pub(crate) struct ClassLayout {
    /// Total size in bytes (rounded to alignment).
    pub size: u32,
    /// Alignment in bytes.
    pub align: u32,
    /// Field byte offsets, in declaration order.
    pub field_offsets: Vec<u32>,
    /// Field types in declaration order.
    field_types: Vec<Type>,
    /// True for `@CStruct class`.
    pub is_value: bool,
}

/// Precomputed layouts for every class in the module.
#[derive(Debug)]
pub(crate) struct Layouts {
    classes: Vec<ClassLayout>,
    handle_classes: Vec<HandleClass>,
    /// Whether each value class contains one or more managed handles in its
    /// language-layout storage. Boundary structs may contain `string` fields
    /// even though source `@CStruct` classes keep the narrower whitelist.
    managed_interior: Vec<bool>,
}

/// One field's name and byte offset inside its struct, as surfaced by
/// [`value_class_layouts`] for the external `offsetof` proof
/// (`specs/blocks/compiler.md` §12.3). The offset is what
/// `offsetof(S, field)` yields for the equivalent C struct.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FieldLayout {
    /// Field name (from [`hir::ClassDef::fields`], declaration order).
    pub name: String,
    /// Byte offset from the start of the struct.
    pub offset: u32,
}

/// The C-ABI layout of one `@CStruct class`: total size, alignment, and
/// each field's name and byte offset.
///
/// This joins the positional offsets of [`ClassLayout`] with the field
/// names in [`hir::ClassDef`], giving the name↔offset mapping the
/// `offsetof` layout proof (`specs/blocks/compiler.md` §12.3) compares
/// against the platform C compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StructLayout {
    /// Class name (monomorphized instances use `Name<args>` spelling).
    pub name: String,
    /// Total size in bytes, rounded up to `align`.
    pub size: u32,
    /// Alignment in bytes after the class-level override.
    pub align: u32,
    /// Fields in declaration (C layout) order.
    pub fields: Vec<FieldLayout>,
}

/// Computes the C-ABI layout of every `@CStruct class` in a checked
/// module (design invariant 1): for each such class, its total size,
/// alignment, and every field's name and byte offset.
///
/// Reference classes are skipped — they are heap handles, not
/// value-layout aggregates. The returned vector preserves module
/// declaration order among value classes.
///
/// This is the public entry point for the `offsetof` layout proof
/// (`specs/blocks/compiler.md` §12.3): the caller compares each
/// returned [`StructLayout`] against `sizeof`/`_Alignof`/`offsetof`
/// taken from the equivalent C struct via the platform C compiler.
///
/// # Errors
///
/// Returns an error for a discovery HIR or an invalid value-class layout.
#[must_use = "the computed layouts are the result to compare against C"]
pub fn value_class_layouts(module: &hir::Module) -> Result<Vec<StructLayout>, String> {
    reject_discovery_hir(module)?;
    let layouts = Layouts::build(module)?;
    let mut out = Vec::new();
    for (id, class) in module.classes.iter().enumerate() {
        if !class.is_value {
            continue;
        }
        let layout = layouts.class(id)?;
        let fields = class
            .fields
            .iter()
            .zip(&layout.field_offsets)
            .map(|(field, &offset)| FieldLayout {
                name: field.name.clone(),
                offset,
            })
            .collect();
        out.push(StructLayout {
            name: class.name.clone(),
            size: layout.size,
            align: layout.align,
            fields,
        });
    }
    Ok(out)
}

/// Returns each padding byte range in the stored representation of `ty`.
///
/// A range includes bytes that do not belong to a scalar leaf. Nested value
/// classes and `FixedArray` elements contribute their internal padding.
///
/// # Errors
///
/// Returns an error for a discovery HIR or an invalid type layout.
pub fn padding_ranges(module: &hir::Module, ty: &Type) -> Result<Vec<Range<u32>>, String> {
    reject_discovery_hir(module)?;
    let layouts = Layouts::build(module)?;
    layouts.padding_ranges(ty)
}

fn reject_discovery_hir(module: &hir::Module) -> Result<(), String> {
    if let Some(import) = module.poisoned_imports.first() {
        return Err(format!(
            "cannot lay out discovery HIR: poisoned import `{}`",
            import.module
        ));
    }
    Ok(())
}

pub(crate) fn ensure_supported_size(size: u32, context: &str) -> Result<u32, String> {
    if size <= MAX_AGGREGATE_BYTES {
        Ok(size)
    } else {
        Err(internal(format!(
            "{context} is {size} bytes; maximum supported aggregate size is \
             {MAX_AGGREGATE_BYTES} bytes"
        )))
    }
}

pub(crate) fn checked_add_size(left: u32, right: u32, context: &str) -> Result<u32, String> {
    let size = left
        .checked_add(right)
        .ok_or_else(|| internal(format!("{context} overflows u32")))?;
    ensure_supported_size(size, context)
}

pub(crate) fn checked_mul_size(left: u32, right: u32, context: &str) -> Result<u32, String> {
    let size = left
        .checked_mul(right)
        .ok_or_else(|| internal(format!("{context} overflows u32")))?;
    ensure_supported_size(size, context)
}

pub(crate) fn round_up(value: u32, align: u32) -> Result<u32, String> {
    round_up_layout(value, align, "aligned aggregate layout")
}

pub(crate) fn round_up_layout(value: u32, align: u32, context: &str) -> Result<u32, String> {
    if !align.is_power_of_two() {
        return Err(internal(format!("{context} has invalid alignment {align}")));
    }
    let mask = align
        .checked_sub(1)
        .ok_or_else(|| internal(format!("{context} has zero alignment")))?;
    let sum = value
        .checked_add(mask)
        .ok_or_else(|| internal(format!("{context} overflows u32 during alignment")))?;
    ensure_supported_size(sum & !mask, context)
}

/// Build-time state: memoized class layouts plus an in-progress set
/// for cycle detection (a value class containing itself, directly or
/// transitively, has no finite layout).
struct ClassShape {
    name: String,
    is_value: bool,
    is_boundary: bool,
    alignment: Option<u32>,
    fields: Vec<Type>,
}

struct Builder<'m> {
    classes: &'m [ClassShape],
    slots: Vec<Option<ClassLayout>>,
    visiting: Vec<bool>,
}

impl<'m> Builder<'m> {
    fn class_layout(&mut self, id: usize) -> Result<ClassLayout, String> {
        if let Some(l) = self.slots.get(id).and_then(|s| s.clone()) {
            return Ok(l);
        }
        let class = self
            .classes
            .get(id)
            .ok_or_else(|| internal(format!("class id {id} out of range")))?;
        if self.visiting[id] {
            return Err(internal(format!(
                "value-class containment cycle through `{}`",
                class.name
            )));
        }
        self.visiting[id] = true;
        let mut size = 0u32;
        let mut align = 1u32;
        let mut field_offsets = Vec::with_capacity(class.fields.len());
        for field in &class.fields {
            let (fs, fa) = self.size_align(field)?;
            size = round_up(size, fa)?;
            field_offsets.push(size);
            size = checked_add_size(size, fs, "class field layout")?;
            align = align.max(fa);
        }
        if let Some(override_) = class.alignment {
            align = align.max(override_);
        }
        size = round_up(size.max(1), align)?;
        let layout = ClassLayout {
            size,
            align,
            field_offsets,
            field_types: class.fields.clone(),
            is_value: class.is_value,
        };
        self.visiting[id] = false;
        self.slots[id] = Some(layout.clone());
        Ok(layout)
    }

    fn size_align(&mut self, ty: &Type) -> Result<(u32, u32), String> {
        Ok(match ty {
            Type::Class(id) => {
                let is_value = self
                    .classes
                    .get(id.0)
                    .map(|c| c.is_value)
                    .ok_or_else(|| internal(format!("class id {} out of range", id.0)))?;
                if is_value {
                    let l = self.class_layout(id.0)?;
                    (l.size, l.align)
                } else {
                    (8, 8)
                }
            }
            Type::FixedArray(elem, n) => {
                let (es, ea) = self.size_align(elem)?;
                let stride = round_up(es, ea)?;
                (checked_mul_size(stride, *n, "FixedArray byte size")?, ea)
            }
            Type::IterResult(v) => {
                let (vs, va) = self.size_align(v)?;
                let a = va.max(1);
                let value_offset = round_up(1, a)?;
                let end = checked_add_size(value_offset, vs, "IterResult layout")?;
                (round_up(end, a)?, a)
            }
            other => scalar_size_align(other)?,
        })
    }
}

/// Size/alignment of every non-class, non-nested type.
fn scalar_size_align(ty: &Type) -> Result<(u32, u32), String> {
    compiler_scalar_size_align(ty).ok_or_else(|| internal(format!("unsized type {ty:?}")))
}

impl Layouts {
    /// Computes layouts for all classes. Nesting order is free (a
    /// value class may be declared after the class embedding it);
    /// containment cycles are an error.
    pub fn build(module: &hir::Module) -> Result<Layouts, String> {
        let classes = module
            .classes
            .iter()
            .map(|class| ClassShape {
                name: class.name.clone(),
                is_value: class.is_value,
                is_boundary: class.is_boundary,
                alignment: class
                    .alignment_override
                    .as_ref()
                    .map(|alignment| alignment.value),
                fields: class.fields.iter().map(|field| field.ty.clone()).collect(),
            })
            .collect::<Vec<_>>();
        Self::build_shapes(&classes)
    }

    /// Computes layouts from the checked class table carried by LIR.
    pub fn build_lir(module: &lir::Module) -> Result<Layouts, String> {
        let classes = module
            .classes
            .iter()
            .map(|class| ClassShape {
                name: class.source_name.clone(),
                is_value: class.is_value,
                is_boundary: class.is_boundary,
                alignment: class.alignment,
                fields: class.fields.iter().map(|field| field.ty.clone()).collect(),
            })
            .collect::<Vec<_>>();
        Self::build_shapes(&classes)
    }

    fn build_shapes(shapes: &[ClassShape]) -> Result<Layouts, String> {
        let n = shapes.len();
        let mut b = Builder {
            classes: shapes,
            slots: vec![None; n],
            visiting: vec![false; n],
        };
        for i in 0..n {
            b.class_layout(i)?;
        }
        let mut classes = Vec::with_capacity(n);
        for (i, slot) in b.slots.into_iter().enumerate() {
            classes.push(slot.ok_or_else(|| internal(format!("class {i} not laid out")))?);
        }
        let handle_classes = shapes
            .iter()
            .map(|class| {
                if !class.is_value {
                    HandleClass::Reference
                } else if class.is_boundary {
                    HandleClass::BoundaryValue
                } else {
                    HandleClass::Value
                }
            })
            .collect::<Vec<_>>();
        let mut managed_interior = vec![false; n];
        let mut managed_known = vec![false; n];
        let mut managed_visiting = vec![false; n];
        for id in 0..n {
            managed_interior[id] = class_contains_managed(
                shapes,
                &handle_classes,
                id,
                &mut managed_interior,
                &mut managed_known,
                &mut managed_visiting,
            )?;
        }
        Ok(Layouts {
            classes,
            handle_classes,
            managed_interior,
        })
    }

    /// Layout of class `id`.
    pub fn class(&self, id: usize) -> Result<&ClassLayout, String> {
        self.classes
            .get(id)
            .ok_or_else(|| internal(format!("class id {id} out of range")))
    }

    fn class_has_managed_interior(&self, id: usize) -> Result<bool, String> {
        self.managed_interior
            .get(id)
            .copied()
            .ok_or_else(|| internal(format!("class id {id} out of range")))
    }

    /// Size and alignment of a type as stored in memory (fields, array
    /// elements, locals in coroutine frames).
    pub fn size_align(&self, ty: &Type) -> Result<(u32, u32), String> {
        Ok(match ty {
            Type::Class(id) => {
                let l = self.class(id.0)?;
                if l.is_value {
                    (l.size, l.align)
                } else {
                    (8, 8)
                }
            }
            Type::FixedArray(elem, n) => {
                let (es, ea) = self.size_align(elem)?;
                let stride = round_up(es, ea)?;
                (checked_mul_size(stride, *n, "FixedArray byte size")?, ea)
            }
            Type::IterResult(v) => {
                let (vs, va) = self.size_align(v)?;
                let a = va.max(1);
                let value_offset = round_up(1, a)?;
                let end = checked_add_size(value_offset, vs, "IterResult layout")?;
                (round_up(end, a)?, a)
            }
            other => scalar_size_align(other)?,
        })
    }

    /// Element stride of an array/FixedArray element type.
    pub fn stride(&self, elem: &Type) -> Result<u32, String> {
        let (s, a) = self.size_align(elem)?;
        round_up(s, a)
    }

    /// Returns each padding range for `ty` from these precomputed layouts.
    pub(crate) fn padding_ranges(&self, ty: &Type) -> Result<Vec<Range<u32>>, String> {
        let mut ranges = Vec::new();
        self.collect_padding_ranges(ty, 0, &mut ranges)?;
        Ok(ranges)
    }

    fn collect_padding_ranges(
        &self,
        ty: &Type,
        base: u32,
        ranges: &mut Vec<Range<u32>>,
    ) -> Result<(), String> {
        match ty {
            Type::Class(id) if self.class(id.0)?.is_value => {
                let layout = self.class(id.0)?;
                let mut cursor = 0u32;
                for (field_ty, &offset) in layout.field_types.iter().zip(&layout.field_offsets) {
                    if cursor < offset {
                        ranges.push(
                            checked_add_size(base, cursor, "padding range start")?
                                ..checked_add_size(base, offset, "padding range end")?,
                        );
                    }
                    let field_base = checked_add_size(base, offset, "padding field base")?;
                    self.collect_padding_ranges(field_ty, field_base, ranges)?;
                    let (field_size, _) = self.size_align(field_ty)?;
                    cursor = checked_add_size(offset, field_size, "padding field end")?;
                }
                if cursor < layout.size {
                    ranges.push(
                        checked_add_size(base, cursor, "padding tail start")?
                            ..checked_add_size(base, layout.size, "padding tail end")?,
                    );
                }
            }
            Type::FixedArray(element, length) => {
                let stride = self.stride(element)?;
                for index in 0..*length {
                    let offset = checked_mul_size(index, stride, "padding array offset")?;
                    let element_base = checked_add_size(base, offset, "padding element base")?;
                    self.collect_padding_ranges(element, element_base, ranges)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Byte offset of the `value` field inside `IterResult<T>`
    /// (`done` is at offset 0).
    pub fn iter_result_value_offset(&self, value_ty: &Type) -> Result<u32, String> {
        let (_, va) = self.size_align(value_ty)?;
        round_up(1, va.max(1))
    }

    /// The runtime representation of a type.
    pub fn repr(&self, ty: &Type) -> Result<Repr, String> {
        Ok(match ty {
            Type::Void => Repr::None,
            Type::Bool => Repr::Scalar(types::I8),
            Type::I8 | Type::U8 => Repr::Scalar(types::I8),
            Type::I16 | Type::U16 | Type::F16 => Repr::Scalar(types::I16),
            Type::I32 | Type::U32 | Type::Enum(_) | Type::StringAlias(_) => {
                Repr::Scalar(types::I32)
            }
            // Date erases to i64 epoch milliseconds (stdlib.md §3).
            Type::I64 | Type::U64 | Type::Date => Repr::Scalar(types::I64),
            Type::F32 => Repr::Scalar(types::F32),
            Type::F64 => Repr::Scalar(types::F64),
            Type::Str
            | Type::RegExp
            | Type::Object
            | Type::Array(_)
            | Type::Map(..)
            | Type::Set(_)
            | Type::Worker(..)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Generator(_)
            | Type::AsyncHandle(_)
            | Type::Nullable(_)
            | Type::Null => Repr::Scalar(types::I64),
            Type::Func(_) => Repr::Pair,
            Type::Class(id) => {
                let l = self.class(id.0)?;
                if l.is_value {
                    Repr::Agg {
                        size: l.size,
                        align: l.align,
                    }
                } else {
                    Repr::Scalar(types::I64)
                }
            }
            Type::FixedArray(..) | Type::IterResult(_) => {
                let (s, a) = self.size_align(ty)?;
                Repr::Agg { size: s, align: a }
            }
            Type::Error => Repr::Scalar(types::I64),
            other => return Err(internal(format!("unrepresentable type {other:?}"))),
        })
    }
}

/// Returns the uniform backing layout for closure environments in one LIR
/// module. A uniform slot lets a function value move between SSA values
/// without a target-specific storage decision in either transcriber.
pub(crate) fn closure_environment_layout(
    module: &lir::Module,
    layouts: &Layouts,
) -> Result<Option<(u32, u32)>, String> {
    let mut maximum_size = 0u32;
    let mut maximum_align = 1u32;
    let mut found = false;
    for function in &module.functions {
        if function.kind != lir::FunctionKind::Lambda {
            continue;
        }
        let mut size = 0u32;
        let mut align = 1u32;
        let mut captures = 0usize;
        for parameter in function
            .parameters
            .iter()
            .filter(|parameter| parameter.kind == lir::ParameterKind::Capture)
        {
            let value = function
                .values
                .get(parameter.value.0 as usize)
                .filter(|value| value.id == parameter.value)
                .ok_or_else(|| internal("closure capture value is missing"))?;
            let lir::ValueType::Data(ty) = &value.ty else {
                return Err(internal("closure capture is not a data value"));
            };
            let (field_size, field_align) = layouts.size_align(ty)?;
            size = round_up(size, field_align.max(1))?;
            size = checked_add_size(size, field_size.max(1), "closure environment layout")?;
            align = align.max(field_align.max(1));
            captures += 1;
        }
        if captures == 0 {
            continue;
        }
        found = true;
        maximum_size = maximum_size.max(round_up(size.max(1), align)?);
        maximum_align = maximum_align.max(align);
    }
    if found {
        Ok(Some((
            round_up(maximum_size, maximum_align)?,
            maximum_align,
        )))
    } else {
        Ok(None)
    }
}

/// True for types whose values are Context allocations (collection
/// roots when held in locals or globals): strings, `object`,
/// reference classes, dynamic arrays, coroutines, and their nullable
/// forms.
pub(crate) fn is_managed(layouts: &Layouts, ty: &Type) -> Result<bool, String> {
    validate_handle_class(layouts, ty)?;
    // Does this value point to a Context allocation that the marker can reach?
    Ok(ty
        .handle_kind(&layouts.handle_classes)
        .is_some_and(HandleKind::is_collector_managed))
}

fn validate_handle_class(layouts: &Layouts, ty: &Type) -> Result<(), String> {
    match ty {
        Type::Class(id) => {
            layouts.class(id.0)?;
        }
        Type::Nullable(inner) => validate_handle_class(layouts, inner)?,
        _ => {}
    }
    Ok(())
}

/// True when a value of `ty` is or *contains* managed handles: a managed
/// scalar, a boundary value class with managed fields, a `FixedArray` whose
/// elements do, or an `IterResult` whose value does. Such values must be
/// visible to the collector wherever they are stored.
pub(crate) fn has_managed_interior(layouts: &Layouts, ty: &Type) -> Result<bool, String> {
    type_contains_managed(layouts, ty)
}

/// Computes the managed-interior bit for one class without depending on
/// byte layout. The same containment-cycle discipline as layout applies;
/// ordinary reference classes are managed handles themselves, while value
/// classes recursively expose their stored fields to the collector.
fn class_contains_managed(
    classes: &[ClassShape],
    handle_classes: &[HandleClass],
    id: usize,
    memo: &mut [bool],
    known: &mut [bool],
    visiting: &mut [bool],
) -> Result<bool, String> {
    if *known
        .get(id)
        .ok_or_else(|| internal(format!("class id {id} out of range")))?
    {
        return memo
            .get(id)
            .copied()
            .ok_or_else(|| internal(format!("class id {id} out of range")));
    }
    let class = classes
        .get(id)
        .ok_or_else(|| internal(format!("class id {id} out of range")))?;
    if !class.is_value {
        memo[id] = true;
        known[id] = true;
        return Ok(true);
    }
    if visiting[id] {
        return Err(internal(format!(
            "value-class managed-interior cycle through `{}`",
            class.name
        )));
    }
    visiting[id] = true;
    let mut contains = false;
    for field in &class.fields {
        if shape_type_contains_managed(classes, handle_classes, field, memo, known, visiting)? {
            contains = true;
            break;
        }
    }
    visiting[id] = false;
    memo[id] = contains;
    known[id] = true;
    Ok(contains)
}

fn shape_type_contains_managed(
    classes: &[ClassShape],
    handle_classes: &[HandleClass],
    ty: &Type,
    memo: &mut [bool],
    known: &mut [bool],
    visiting: &mut [bool],
) -> Result<bool, String> {
    validate_shape_handle_class(classes, ty)?;
    if let Some(kind) = ty.handle_kind(handle_classes) {
        return Ok(kind.contains_managed());
    }
    Ok(match ty {
        Type::Class(id) => {
            class_contains_managed(classes, handle_classes, id.0, memo, known, visiting)?
        }
        Type::FixedArray(elem, _) => {
            shape_type_contains_managed(classes, handle_classes, elem, memo, known, visiting)?
        }
        Type::IterResult(value) => {
            shape_type_contains_managed(classes, handle_classes, value, memo, known, visiting)?
        }
        _ => false,
    })
}

fn validate_shape_handle_class(classes: &[ClassShape], ty: &Type) -> Result<(), String> {
    match ty {
        Type::Class(id) => {
            classes
                .get(id.0)
                .ok_or_else(|| internal(format!("class id {} out of range", id.0)))?;
        }
        Type::Nullable(inner) => validate_shape_handle_class(classes, inner)?,
        _ => {}
    }
    Ok(())
}

pub(crate) fn type_contains_managed(layouts: &Layouts, ty: &Type) -> Result<bool, String> {
    validate_handle_class(layouts, ty)?;
    if let Some(kind) = ty.handle_kind(&layouts.handle_classes) {
        return Ok(kind.contains_managed());
    }
    Ok(match ty {
        Type::Class(id) => layouts.class_has_managed_interior(id.0)?,
        Type::FixedArray(elem, _) => type_contains_managed(layouts, elem)?,
        Type::IterResult(value) => type_contains_managed(layouts, value)?,
        _ => false,
    })
}

/// Number of 8-byte shadow/root words a value of `ty` needs so the
/// collector can see every handle in it: 1 for a managed scalar, the
/// word-rounded size for an aggregate with managed interior, 0
/// otherwise.
pub(crate) fn managed_words(layouts: &Layouts, ty: &Type) -> Result<u32, String> {
    if is_managed(layouts, ty)? {
        return Ok(1);
    }
    if has_managed_interior(layouts, ty)? {
        let (size, _) = layouts.size_align(ty)?;
        return Ok(round_up(size, 8)? / 8);
    }
    Ok(0)
}

/// True when the type's comparisons are unsigned.
pub(crate) fn is_unsigned(ty: &Type) -> bool {
    matches!(ty, Type::U8 | Type::U16 | Type::U32 | Type::U64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    fn module_of(src: &str) -> hir::Module {
        check_program(&[SourceFile::new("t.ts", src)]).expect("clean check")
    }

    fn layouts_of(src: &str) -> Layouts {
        Layouts::build(&module_of(src)).expect("layouts")
    }

    #[test]
    fn value_class_layout_matches_the_c_struct() {
        // struct { float x; float y; float z; } -> size 12, align 4.
        let layouts = layouts_of(
            "@CStruct\nclass Vec3 { x: f32; y: f32; z: f32;\n constructor(x: f32, y: f32, z: f32) { this.x = x; this.y = y; this.z = z; } }\nexport function main(): void { const v: Vec3 = new Vec3(1.0, 2.0, 3.0); print(`${v.x}`); }\n",
        );
        let l = layouts.class(0).expect("class 0");
        assert_eq!((l.size, l.align), (12, 4));
        assert_eq!(l.field_offsets, vec![0, 4, 8]);
    }

    #[test]
    fn padding_follows_c_rules() {
        // struct { bool a; double b; int c; } -> b at 8, c at 16, size 24.
        let layouts = layouts_of(
            "@CStruct\nclass P { a: boolean; b: f64; c: i32;\n constructor() { this.a = true; this.b = 1.0; this.c = 1; } }\nexport function main(): void { const p: P = new P(); print(`${p.c}`); }\n",
        );
        let l = layouts.class(0).expect("class 0");
        assert_eq!(l.field_offsets, vec![0, 8, 16]);
        assert_eq!((l.size, l.align), (24, 8));
    }

    #[test]
    fn padding_ranges_cover_aligned_nested_values_and_fixed_arrays() {
        let module = module_of(
            "@CStruct({ align: 16 })\nclass Vec3f { x: f32 = 0.0; y: f32 = 0.0; z: f32 = 0.0; }\n@CStruct\nclass Mixed { a: f32 = 0.0; p: Vec3f = new Vec3f(); }\nexport function main(): void {}\n",
        );
        let vec3 = Type::Class(subscript_compiler::ClassId(0));
        let mixed = Type::Class(subscript_compiler::ClassId(1));
        assert_eq!(
            padding_ranges(&module, &vec3).expect("Vec3f padding"),
            vec![12..16]
        );
        assert_eq!(
            padding_ranges(&module, &mixed).expect("Mixed padding"),
            vec![4..16, 28..32]
        );
        assert_eq!(
            padding_ranges(&module, &Type::FixedArray(Box::new(vec3), 2))
                .expect("FixedArray padding"),
            vec![12..16, 28..32]
        );
    }

    #[test]
    fn fixed_array_is_in_place() {
        let layouts = layouts_of(
            "@CStruct\nclass M { e: FixedArray<f32, 16>;\n constructor(e: FixedArray<f32, 16>) { this.e = e; } }\nexport function main(): void { const m: M = new M([0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0]); print(`${m.e[0]}`); }\n",
        );
        assert_eq!(
            layouts
                .size_align(&Type::FixedArray(Box::new(Type::F32), 16))
                .expect("size"),
            (64, 4)
        );
        assert_eq!(layouts.class(0).expect("class 0").size, 64);
    }

    #[test]
    fn boundary_string_field_marks_the_language_struct_as_managed_interior() {
        let module = check_program(&[
            SourceFile::ambient(
                "record.d.ts",
                "declare class Record { label: string; serial: u64; \
                 constructor(label: string, serial: u64); }\n",
            ),
            SourceFile::new(
                "t.ts",
                "export function main(): void { \
                 const record: Record = new Record(\"rooted\", 7); \
                 print(record.label); }\n",
            ),
        ])
        .expect("clean boundary mirror");
        let layouts = Layouts::build(&module).expect("layouts");
        let ty = Type::Class(subscript_compiler::ClassId(0));
        assert!(has_managed_interior(&layouts, &ty).expect("managed interior"));
        assert_eq!(managed_words(&layouts, &ty).expect("managed words"), 2);
    }

    #[test]
    fn oversized_fixed_array_layout_is_an_error_not_a_panic() {
        let layouts = layouts_of("export function main(): void {}\n");
        let error = layouts
            .size_align(&Type::FixedArray(Box::new(Type::U8), u32::MAX))
            .expect_err("oversized FixedArray must fail layout");
        assert!(
            error.contains("maximum supported aggregate size"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn every_layout_arithmetic_shape_reports_overflow() {
        assert!(round_up(u32::MAX, 1).is_err());
        assert!(round_up(u32::MAX, 8).is_err());
        assert!(checked_add_size(MAX_AGGREGATE_BYTES, 1, "test layout").is_err());
        assert!(checked_mul_size(MAX_AGGREGATE_BYTES, 2, "test layout").is_err());
    }

    #[test]
    fn forward_nested_value_classes_lay_out_correctly() {
        // Outer (id 0) embeds Inner (id 1), declared after it: the
        // layout build must resolve the forward reference instead of
        // falling back to a wrong layout.
        let layouts = layouts_of(
            "@CStruct\nclass Outer { inner: Inner; pad: f32;\n constructor(inner: Inner, pad: f32) { this.inner = inner; this.pad = pad; } }\n@CStruct\nclass Inner { x: f64;\n constructor(x: f64) { this.x = x; } }\nexport function main(): void {\n  const o: Outer = new Outer(new Inner(2.5), 1.0);\n  print(`${o.inner.x}`);\n}\n",
        );
        let outer = layouts.class(0).expect("outer");
        let inner = layouts.class(1).expect("inner");
        assert_eq!((inner.size, inner.align), (8, 8));
        assert_eq!(outer.field_offsets, vec![0, 8]);
        assert_eq!((outer.size, outer.align), (16, 8));
    }

    #[test]
    fn value_class_containment_cycle_is_an_error_not_a_hang() {
        let m = module_of(
            "@CStruct\nclass S { s: S;\n constructor(s: S) { this.s = s; } }\nexport function main(): void {}\n",
        );
        let err = Layouts::build(&m).expect_err("cycle must be rejected");
        assert!(err.contains("cycle"), "unexpected error: {err}");
    }

    #[test]
    fn out_of_range_class_id_is_an_error_not_a_fallback() {
        let layouts = layouts_of("export function main(): void {}\n");
        assert!(layouts.class(0).is_err());
        assert!(layouts
            .size_align(&Type::Class(subscript_compiler::ClassId(7)))
            .is_err());
    }

    #[test]
    fn reprs_are_as_documented() {
        let layouts = layouts_of("export function main(): void {}\n");
        let repr = |t: &Type| layouts.repr(t).expect("repr");
        assert_eq!(repr(&Type::I32), Repr::Scalar(types::I32));
        assert_eq!(repr(&Type::I8), Repr::Scalar(types::I8));
        assert_eq!(repr(&Type::U16), Repr::Scalar(types::I16));
        assert_eq!(repr(&Type::F16), Repr::Scalar(types::I16));
        assert_eq!(repr(&Type::U64), Repr::Scalar(types::I64));
        assert_eq!(repr(&Type::F32), Repr::Scalar(types::F32));
        assert_eq!(repr(&Type::Bool), Repr::Scalar(types::I8));
        assert_eq!(repr(&Type::Str), Repr::Scalar(types::I64));
        assert!(matches!(
            repr(&Type::Func(Box::new(subscript_compiler::FuncType {
                params: vec![Type::I32],
                ret: Type::I32
            }))),
            Repr::Pair
        ));
        assert!(is_managed(&layouts, &Type::Str).expect("managed"));
        assert!(is_managed(&layouts, &Type::Nullable(Box::new(Type::Object))).expect("managed"));
        assert!(!is_managed(&layouts, &Type::I64).expect("managed"));
    }

    #[test]
    fn managed_interior_and_word_counts() {
        let layouts = layouts_of("export function main(): void {}\n");
        let fixed_str = Type::FixedArray(Box::new(Type::Str), 3);
        assert!(has_managed_interior(&layouts, &fixed_str).expect("interior"));
        assert_eq!(managed_words(&layouts, &fixed_str).expect("words"), 3);
        let step_str = Type::IterResult(Box::new(Type::Str));
        assert!(has_managed_interior(&layouts, &step_str).expect("interior"));
        assert_eq!(managed_words(&layouts, &step_str).expect("words"), 2);
        let plain = Type::FixedArray(Box::new(Type::F32), 4);
        assert!(!has_managed_interior(&layouts, &plain).expect("interior"));
        assert_eq!(managed_words(&layouts, &plain).expect("words"), 0);
        assert_eq!(managed_words(&layouts, &Type::Str).expect("words"), 1);
    }

    #[test]
    fn value_class_layouts_surface_names_offsets_size_and_align() {
        // Two value classes plus a reference class: the public API
        // reports only the value classes, with named per-field offsets.
        let module = module_of(
            "@CStruct\nclass P { a: boolean; b: f64; c: i32;\n constructor() { this.a = true; this.b = 1.0; this.c = 1; } }\nclass R { x: i32; constructor() { this.x = 1; } }\n@CStruct\nclass V { x: f32; y: f32;\n constructor(x: f32, y: f32) { this.x = x; this.y = y; } }\nexport function main(): void { const p: P = new P(); const v: V = new V(1.0, 2.0); const r: R = new R(); print(`${p.c}${v.x}${r.x}`); }\n",
        );
        let layouts = value_class_layouts(&module).expect("layouts");
        // R (reference) is excluded; P and V remain in declaration order.
        assert_eq!(layouts.len(), 2);
        assert_eq!(layouts[0].name, "P");
        assert_eq!((layouts[0].size, layouts[0].align), (24, 8));
        assert_eq!(
            layouts[0].fields,
            vec![
                FieldLayout {
                    name: "a".into(),
                    offset: 0
                },
                FieldLayout {
                    name: "b".into(),
                    offset: 8
                },
                FieldLayout {
                    name: "c".into(),
                    offset: 16
                },
            ]
        );
        assert_eq!(layouts[1].name, "V");
        assert_eq!((layouts[1].size, layouts[1].align), (8, 4));
        assert_eq!(
            layouts[1].fields,
            vec![
                FieldLayout {
                    name: "x".into(),
                    offset: 0
                },
                FieldLayout {
                    name: "y".into(),
                    offset: 4
                },
            ]
        );
    }

    #[test]
    fn value_class_layouts_report_alignment_overrides() {
        let module = module_of(
            "@CStruct({ align: 16 })\nclass Vec3f { x: f32; y: f32; z: f32; }\n@CStruct\nclass Mixed { a: f32; p: Vec3f; }\n@CStruct\nclass Mat3x3f { c0: Vec3f; c1: Vec3f; c2: Vec3f; }\nexport function main(): void {}\n",
        );
        let layouts = value_class_layouts(&module).expect("layouts");
        assert_eq!(layouts.len(), 3);
        assert_eq!((layouts[0].size, layouts[0].align), (16, 16));
        assert_eq!((layouts[1].size, layouts[1].align), (32, 16));
        assert_eq!(layouts[1].fields[1].offset, 16);
        assert_eq!((layouts[2].size, layouts[2].align), (48, 16));
        assert_eq!(
            layouts[2]
                .fields
                .iter()
                .map(|field| field.offset)
                .collect::<Vec<_>>(),
            vec![0, 16, 32]
        );
    }

    #[test]
    fn value_class_layouts_reports_cycles_as_errors() {
        let module = module_of(
            "@CStruct\nclass S { s: S;\n constructor(s: S) { this.s = s; } }\nexport function main(): void {}\n",
        );
        assert!(value_class_layouts(&module).is_err());
    }

    #[test]
    fn iter_result_layout_is_bool_then_aligned_value() {
        let layouts = layouts_of("export function main(): void {}\n");
        assert_eq!(
            layouts.iter_result_value_offset(&Type::I32).expect("off"),
            4
        );
        assert_eq!(
            layouts
                .size_align(&Type::IterResult(Box::new(Type::I32)))
                .expect("size"),
            (8, 4)
        );
        assert_eq!(
            layouts.iter_result_value_offset(&Type::F64).expect("off"),
            8
        );
        assert_eq!(
            layouts
                .size_align(&Type::IterResult(Box::new(Type::F64)))
                .expect("size"),
            (16, 8)
        );
    }
}
