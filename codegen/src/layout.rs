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

use cranelift_codegen::ir::types;
use subscript_compiler::hir;
use subscript_compiler::Type;

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

fn round_up(v: u32, align: u32) -> u32 {
    debug_assert!(align.is_power_of_two());
    (v + align - 1) & !(align - 1)
}

impl Layouts {
    /// Computes layouts for all classes. Value classes may contain
    /// other value classes (the checker rejects cycles by field-type
    /// whitelisting on declared classes only; nesting is resolved
    /// iteratively in id order — a nested value class always has a
    /// smaller id because it must be declared to be referenced).
    pub fn build(module: &hir::Module) -> Layouts {
        let mut layouts = Layouts { classes: Vec::new() };
        for class in &module.classes {
            let mut size = 0u32;
            let mut align = 1u32;
            let mut field_offsets = Vec::with_capacity(class.fields.len());
            for field in &class.fields {
                let (fs, fa) = layouts.size_align(&field.ty);
                size = round_up(size, fa);
                field_offsets.push(size);
                size += fs;
                align = align.max(fa);
            }
            size = round_up(size.max(1), align);
            layouts.classes.push(ClassLayout {
                size,
                align,
                field_offsets,
                is_value: class.is_value,
            });
        }
        layouts
    }

    /// Layout of class `id`.
    pub fn class(&self, id: usize) -> &ClassLayout {
        &self.classes[id.min(self.classes.len().saturating_sub(1))]
    }

    /// Size and alignment of a type as stored in memory (fields, array
    /// elements, locals in coroutine frames).
    pub fn size_align(&self, ty: &Type) -> (u32, u32) {
        match ty {
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
            Type::Class(id) => {
                let l = self.class(id.0);
                if l.is_value {
                    (l.size, l.align)
                } else {
                    (8, 8)
                }
            }
            Type::FixedArray(elem, n) => {
                let (es, ea) = self.size_align(elem);
                (round_up(es, ea) * n, ea)
            }
            Type::IterResult(v) => {
                let (vs, va) = self.size_align(v);
                let off = round_up(1, va);
                (round_up(off + vs, va.max(1)), va.max(1))
            }
            Type::Void | Type::Error => (0, 1),
            // `Type` is non-exhaustive; new variants must be sized
            // here before they can be lowered.
            _ => (0, 1),
        }
    }

    /// Element stride of an array/FixedArray element type.
    pub fn stride(&self, elem: &Type) -> u32 {
        let (s, a) = self.size_align(elem);
        round_up(s, a)
    }

    /// Byte offset of the `value` field inside `IterResult<T>`
    /// (`done` is at offset 0).
    pub fn iter_result_value_offset(&self, value_ty: &Type) -> u32 {
        let (_, va) = self.size_align(value_ty);
        round_up(1, va.max(1))
    }

    /// The runtime representation of a type.
    pub fn repr(&self, ty: &Type) -> Repr {
        match ty {
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
                let l = self.class(id.0);
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
                let (s, a) = self.size_align(ty);
                Repr::Agg { size: s, align: a }
            }
            Type::Error => Repr::Scalar(types::I64),
            // `Type` is non-exhaustive; new variants default to a
            // pointer-sized scalar until given a representation.
            _ => Repr::Scalar(types::I64),
        }
    }
}

/// True for types whose values are Context allocations (collection
/// roots when held in locals or globals): strings, `object`,
/// reference classes, dynamic arrays, coroutines, and their nullable
/// forms.
pub(crate) fn is_managed(layouts: &Layouts, ty: &Type) -> bool {
    match ty {
        Type::Str | Type::Object | Type::Array(_) | Type::Generator(_) => true,
        Type::Nullable(inner) => is_managed(layouts, inner) || matches!(**inner, Type::Func(_)),
        Type::Class(id) => !layouts.class(id.0).is_value,
        _ => false,
    }
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

    #[test]
    fn value_class_layout_matches_the_c_struct() {
        // struct { float x; float y; float z; } -> size 12, align 4.
        let m = module_of(
            "@value\nclass Vec3 { x: f32; y: f32; z: f32;\n constructor(x: f32, y: f32, z: f32) { this.x = x; this.y = y; this.z = z; } }\nexport function main(): void { const v: Vec3 = new Vec3(1.0, 2.0, 3.0); print(`${v.x}`); }\n",
        );
        let layouts = Layouts::build(&m);
        let l = layouts.class(0);
        assert_eq!((l.size, l.align), (12, 4));
        assert_eq!(l.field_offsets, vec![0, 4, 8]);
    }

    #[test]
    fn padding_follows_c_rules() {
        // struct { bool a; double b; int c; } -> b at 8, c at 16, size 24.
        let m = module_of(
            "@value\nclass P { a: boolean; b: f64; c: i32;\n constructor() { this.a = true; this.b = 1.0; this.c = 1; } }\nexport function main(): void { const p: P = new P(); print(`${p.c}`); }\n",
        );
        let layouts = Layouts::build(&m);
        let l = layouts.class(0);
        assert_eq!(l.field_offsets, vec![0, 8, 16]);
        assert_eq!((l.size, l.align), (24, 8));
    }

    #[test]
    fn fixed_array_is_in_place() {
        let m = module_of(
            "@value\nclass M { e: FixedArray<f32, 16>;\n constructor(e: FixedArray<f32, 16>) { this.e = e; } }\nexport function main(): void { const m: M = new M([0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0]); print(`${m.e[0]}`); }\n",
        );
        let layouts = Layouts::build(&m);
        assert_eq!(layouts.size_align(&Type::FixedArray(Box::new(Type::F32), 16)), (64, 4));
        assert_eq!(layouts.class(0).size, 64);
    }

    #[test]
    fn reprs_are_as_documented() {
        let m = module_of("export function main(): void {}\n");
        let layouts = Layouts::build(&m);
        assert_eq!(layouts.repr(&Type::I32), Repr::Scalar(types::I32));
        assert_eq!(layouts.repr(&Type::U64), Repr::Scalar(types::I64));
        assert_eq!(layouts.repr(&Type::F32), Repr::Scalar(types::F32));
        assert_eq!(layouts.repr(&Type::Bool), Repr::Scalar(types::I8));
        assert_eq!(layouts.repr(&Type::Str), Repr::Scalar(types::I64));
        assert!(matches!(
            layouts.repr(&Type::Func(Box::new(subscript_compiler::FuncType {
                params: vec![Type::I32],
                ret: Type::I32
            }))),
            Repr::Pair
        ));
        assert!(is_managed(&layouts, &Type::Str));
        assert!(is_managed(&layouts, &Type::Nullable(Box::new(Type::Object))));
        assert!(!is_managed(&layouts, &Type::I64));
    }

    #[test]
    fn iter_result_layout_is_bool_then_aligned_value() {
        let m = module_of("export function main(): void {}\n");
        let layouts = Layouts::build(&m);
        assert_eq!(layouts.iter_result_value_offset(&Type::I32), 4);
        assert_eq!(layouts.size_align(&Type::IterResult(Box::new(Type::I32))), (8, 4));
        assert_eq!(layouts.iter_result_value_offset(&Type::F64), 8);
        assert_eq!(layouts.size_align(&Type::IterResult(Box::new(Type::F64))), (16, 8));
    }
}
