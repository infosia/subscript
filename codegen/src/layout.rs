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
use subscript_compiler::hir;
use subscript_compiler::Type;

use crate::lower::internal;

/// How a language type is represented in generated code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Repr {
    /// No value (`void`).
    None,
    /// One CLIF scalar (`I8` boolean, `I32`, `I64`, `F32`, `F64`, or a
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
    /// True for `@value class`.
    pub is_value: bool,
}

/// Precomputed layouts for every class in the module.
#[derive(Debug)]
pub(crate) struct Layouts {
    classes: Vec<ClassLayout>,
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

/// The C-ABI layout of one `@value class`: total size, alignment, and
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
    /// Alignment in bytes (the maximum field alignment).
    pub align: u32,
    /// Fields in declaration (C layout) order.
    pub fields: Vec<FieldLayout>,
}

/// Computes the C-ABI layout of every `@value class` in a checked
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
/// Returns an internal-error string (never panics) when a layout cannot
/// be computed — a value-class containment cycle or an out-of-range
/// class id.
#[must_use = "the computed layouts are the result to compare against C"]
pub fn value_class_layouts(module: &hir::Module) -> Result<Vec<StructLayout>, String> {
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

fn round_up(v: u32, align: u32) -> u32 {
    debug_assert!(align.is_power_of_two());
    (v + align - 1) & !(align - 1)
}

/// Build-time state: memoized class layouts plus an in-progress set
/// for cycle detection (a value class containing itself, directly or
/// transitively, has no finite layout).
struct Builder<'m> {
    module: &'m hir::Module,
    slots: Vec<Option<ClassLayout>>,
    visiting: Vec<bool>,
}

impl<'m> Builder<'m> {
    fn class_layout(&mut self, id: usize) -> Result<ClassLayout, String> {
        if let Some(l) = self.slots.get(id).and_then(|s| s.clone()) {
            return Ok(l);
        }
        let class = self
            .module
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
            let (fs, fa) = self.size_align(&field.ty)?;
            size = round_up(size, fa);
            field_offsets.push(size);
            size += fs;
            align = align.max(fa);
        }
        size = round_up(size.max(1), align);
        let layout = ClassLayout {
            size,
            align,
            field_offsets,
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
                    .module
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
                (round_up(es, ea) * n, ea)
            }
            Type::IterResult(v) => {
                let (vs, va) = self.size_align(v)?;
                let a = va.max(1);
                (round_up(round_up(1, a) + vs, a), a)
            }
            other => scalar_size_align(other)?,
        })
    }
}

/// Size/alignment of every non-class, non-nested type.
fn scalar_size_align(ty: &Type) -> Result<(u32, u32), String> {
    Ok(match ty {
        Type::Bool => (1, 1),
        Type::I32 | Type::U32 | Type::F32 | Type::Enum(_) => (4, 4),
        Type::I64 | Type::U64 | Type::F64 => (8, 8),
        Type::Str
        | Type::Object
        | Type::Array(_)
        | Type::Generator(_)
        | Type::Nullable(_)
        | Type::Null => (8, 8),
        Type::Func(_) => (16, 8),
        Type::Void | Type::Error => (0, 1),
        other => return Err(internal(format!("unsized type {other:?}"))),
    })
}

impl Layouts {
    /// Computes layouts for all classes. Nesting order is free (a
    /// value class may be declared after the class embedding it);
    /// containment cycles are an error.
    pub fn build(module: &hir::Module) -> Result<Layouts, String> {
        let n = module.classes.len();
        let mut b = Builder {
            module,
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
        Ok(Layouts { classes })
    }

    /// Layout of class `id`.
    pub fn class(&self, id: usize) -> Result<&ClassLayout, String> {
        self.classes
            .get(id)
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
                (round_up(es, ea) * n, ea)
            }
            Type::IterResult(v) => {
                let (vs, va) = self.size_align(v)?;
                let a = va.max(1);
                (round_up(round_up(1, a) + vs, a), a)
            }
            other => scalar_size_align(other)?,
        })
    }

    /// Element stride of an array/FixedArray element type.
    pub fn stride(&self, elem: &Type) -> Result<u32, String> {
        let (s, a) = self.size_align(elem)?;
        Ok(round_up(s, a))
    }

    /// Byte offset of the `value` field inside `IterResult<T>`
    /// (`done` is at offset 0).
    pub fn iter_result_value_offset(&self, value_ty: &Type) -> Result<u32, String> {
        let (_, va) = self.size_align(value_ty)?;
        Ok(round_up(1, va.max(1)))
    }

    /// The runtime representation of a type.
    pub fn repr(&self, ty: &Type) -> Result<Repr, String> {
        Ok(match ty {
            Type::Void => Repr::None,
            Type::Bool => Repr::Scalar(types::I8),
            Type::I32 | Type::U32 | Type::Enum(_) => Repr::Scalar(types::I32),
            Type::I64 | Type::U64 => Repr::Scalar(types::I64),
            Type::F32 => Repr::Scalar(types::F32),
            Type::F64 => Repr::Scalar(types::F64),
            Type::Str
            | Type::Object
            | Type::Array(_)
            | Type::Generator(_)
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

/// True for types whose values are Context allocations (collection
/// roots when held in locals or globals): strings, `object`,
/// reference classes, dynamic arrays, coroutines, and their nullable
/// forms.
pub(crate) fn is_managed(layouts: &Layouts, ty: &Type) -> Result<bool, String> {
    Ok(match ty {
        Type::Str | Type::Object | Type::Array(_) | Type::Generator(_) => true,
        Type::Nullable(inner) => {
            is_managed(layouts, inner)? || matches!(**inner, Type::Func(_))
        }
        Type::Class(id) => !layouts.class(id.0)?.is_value,
        _ => false,
    })
}

/// True when a value of `ty` is or *contains* managed handles: a
/// managed scalar, a `FixedArray` whose elements do, or an
/// `IterResult` whose value does. Such values must be visible to the
/// collector wherever they are stored. (Value classes cannot contain
/// managed fields under the C2 whitelist; if that whitelist widens,
/// this predicate must learn to walk their fields.)
pub(crate) fn has_managed_interior(layouts: &Layouts, ty: &Type) -> Result<bool, String> {
    if is_managed(layouts, ty)? {
        return Ok(true);
    }
    Ok(match ty {
        Type::FixedArray(elem, _) => has_managed_interior(layouts, elem)?,
        Type::IterResult(v) => has_managed_interior(layouts, v)?,
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
        return Ok(round_up(size, 8) / 8);
    }
    Ok(0)
}

/// True when the type's comparisons are unsigned.
pub(crate) fn is_unsigned(ty: &Type) -> bool {
    matches!(ty, Type::U32 | Type::U64)
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
            "@value\nclass Vec3 { x: f32; y: f32; z: f32;\n constructor(x: f32, y: f32, z: f32) { this.x = x; this.y = y; this.z = z; } }\nexport function main(): void { const v: Vec3 = new Vec3(1.0, 2.0, 3.0); print(`${v.x}`); }\n",
        );
        let l = layouts.class(0).expect("class 0");
        assert_eq!((l.size, l.align), (12, 4));
        assert_eq!(l.field_offsets, vec![0, 4, 8]);
    }

    #[test]
    fn padding_follows_c_rules() {
        // struct { bool a; double b; int c; } -> b at 8, c at 16, size 24.
        let layouts = layouts_of(
            "@value\nclass P { a: boolean; b: f64; c: i32;\n constructor() { this.a = true; this.b = 1.0; this.c = 1; } }\nexport function main(): void { const p: P = new P(); print(`${p.c}`); }\n",
        );
        let l = layouts.class(0).expect("class 0");
        assert_eq!(l.field_offsets, vec![0, 8, 16]);
        assert_eq!((l.size, l.align), (24, 8));
    }

    #[test]
    fn fixed_array_is_in_place() {
        let layouts = layouts_of(
            "@value\nclass M { e: FixedArray<f32, 16>;\n constructor(e: FixedArray<f32, 16>) { this.e = e; } }\nexport function main(): void { const m: M = new M([0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0]); print(`${m.e[0]}`); }\n",
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
    fn forward_nested_value_classes_lay_out_correctly() {
        // Outer (id 0) embeds Inner (id 1), declared after it: the
        // layout build must resolve the forward reference instead of
        // falling back to a wrong layout.
        let layouts = layouts_of(
            "@value\nclass Outer { inner: Inner; pad: f32;\n constructor(inner: Inner, pad: f32) { this.inner = inner; this.pad = pad; } }\n@value\nclass Inner { x: f64;\n constructor(x: f64) { this.x = x; } }\nexport function main(): void {\n  const o: Outer = new Outer(new Inner(2.5), 1.0);\n  print(`${o.inner.x}`);\n}\n",
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
            "@value\nclass S { s: S;\n constructor(s: S) { this.s = s; } }\nexport function main(): void {}\n",
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
            "@value\nclass P { a: boolean; b: f64; c: i32;\n constructor() { this.a = true; this.b = 1.0; this.c = 1; } }\nclass R { x: i32; constructor() { this.x = 1; } }\n@value\nclass V { x: f32; y: f32;\n constructor(x: f32, y: f32) { this.x = x; this.y = y; } }\nexport function main(): void { const p: P = new P(); const v: V = new V(1.0, 2.0); const r: R = new R(); print(`${p.c}${v.x}${r.x}`); }\n",
        );
        let layouts = value_class_layouts(&module).expect("layouts");
        // R (reference) is excluded; P and V remain in declaration order.
        assert_eq!(layouts.len(), 2);
        assert_eq!(layouts[0].name, "P");
        assert_eq!((layouts[0].size, layouts[0].align), (24, 8));
        assert_eq!(
            layouts[0].fields,
            vec![
                FieldLayout { name: "a".into(), offset: 0 },
                FieldLayout { name: "b".into(), offset: 8 },
                FieldLayout { name: "c".into(), offset: 16 },
            ]
        );
        assert_eq!(layouts[1].name, "V");
        assert_eq!((layouts[1].size, layouts[1].align), (8, 4));
        assert_eq!(
            layouts[1].fields,
            vec![
                FieldLayout { name: "x".into(), offset: 0 },
                FieldLayout { name: "y".into(), offset: 4 },
            ]
        );
    }

    #[test]
    fn value_class_layouts_reports_cycles_as_errors() {
        let module = module_of(
            "@value\nclass S { s: S;\n constructor(s: S) { this.s = s; } }\nexport function main(): void {}\n",
        );
        assert!(value_class_layouts(&module).is_err());
    }

    #[test]
    fn iter_result_layout_is_bool_then_aligned_value() {
        let layouts = layouts_of("export function main(): void {}\n");
        assert_eq!(layouts.iter_result_value_offset(&Type::I32).expect("off"), 4);
        assert_eq!(
            layouts
                .size_align(&Type::IterResult(Box::new(Type::I32)))
                .expect("size"),
            (8, 4)
        );
        assert_eq!(layouts.iter_result_value_offset(&Type::F64).expect("off"), 8);
        assert_eq!(
            layouts
                .size_align(&Type::IterResult(Box::new(Type::F64)))
                .expect("size"),
            (16, 8)
        );
    }
}
