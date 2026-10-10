//! The compiler's view of boundary structs for the pass decision
//! (`specs/blocks/compiler.md` §187 rule 3).
//!
//! [`subscript_boundary::struct_pass`] and the pointer-pass functions
//! decide how a call passes a struct. The checker and both code-generation
//! tiers give them the same view: [`field_shape`] maps a language type to
//! its member shape, and [`Classes`] adapts any class table to
//! [`StructView`].

use crate::hir::{ClassDef, ForeignFn};
use crate::types::{ClassId, Type};
use subscript_boundary::{FieldShape, StructView};

/// The facts of one class that the shape of a member needs.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ClassFacts {
    /// True for a value class.
    pub is_value: bool,
    /// True for a boundary struct.
    pub is_boundary: bool,
    /// True for an embedded header (§33 rule 9).
    pub is_embedded_header: bool,
    /// The field types, in declaration order.
    pub fields: Vec<Type>,
    /// For each field, true when its C pointer is `const` (the
    /// `ConstPointer` provenance, §187 rule 7).
    pub constant: Vec<bool>,
}

impl ClassFacts {
    /// Collects the facts of one class. `fields` pairs each field type with
    /// its `const` fact.
    #[must_use]
    pub fn new(
        is_value: bool,
        is_boundary: bool,
        is_embedded_header: bool,
        fields: Vec<(Type, bool)>,
    ) -> Self {
        let (fields, constant) = fields.into_iter().unzip();
        Self {
            is_value,
            is_boundary,
            is_embedded_header,
            fields,
            constant,
        }
    }
}

/// `shape` with the `const` fact of its pointer: a struct-pointer member
/// whose C pointer is `const` passes a target that copies its bytes as it
/// is (§187 rule 9).
#[must_use]
pub fn with_constant(shape: FieldShape<ClassId>, constant: bool) -> FieldShape<ClassId> {
    match shape {
        FieldShape::Pointer { target, header, .. } => FieldShape::Pointer {
            target,
            header,
            constant,
        },
        shape => shape,
    }
}

/// A class table that the pass decision reads.
pub trait BoundaryClasses {
    /// The facts of class `id`, or `None` for an unknown id.
    fn class(&self, id: ClassId) -> Option<ClassFacts>;

    /// The number of classes: the ids are `0..class_count()`.
    fn class_count(&self) -> usize;

    /// Resolves an alias or a type parameter to the type it stands for.
    fn resolve(&self, ty: &Type) -> Type {
        ty.clone()
    }
}

/// Maps the type of a boundary-struct member to its shape. The mirror
/// spells a collapsed count-first pair as `T[]`, a string view as
/// `string`, a callback as a function type, an embedded struct as `X`, a
/// struct-pointer member as `X | null`, and a userdata slot as
/// `object | null` (Q13).
#[must_use]
pub fn field_shape<C: BoundaryClasses + ?Sized>(classes: &C, ty: &Type) -> FieldShape<ClassId> {
    let ty = classes.resolve(ty);
    if crate::types::boundary_kind(&ty).is_some() {
        return FieldShape::Bytes;
    }
    if ty.function_type().is_some() {
        return FieldShape::Callback;
    }
    match ty {
        Type::Object => FieldShape::Userdata,
        Type::Str => FieldShape::StringView,
        // §187 rule 11: a read validates a wire alias element (§52).
        Type::Array(element) if matches!(classes.resolve(&element), Type::StringAlias(_)) => {
            FieldShape::ValidatedPair
        }
        Type::Array(element) => FieldShape::Pair {
            elements: match classes.resolve(&element) {
                Type::Class(id) if classes.class(id).is_some_and(|class| class.is_value) => {
                    Some(id)
                }
                _ => None,
            },
        },
        Type::FixedArray(element, _) => field_shape(classes, &element),
        Type::Class(id) => match classes.class(id) {
            Some(class) if class.is_value => FieldShape::Embedded(id),
            Some(_) => FieldShape::Bytes,
            None => FieldShape::Unlowered,
        },
        Type::Nullable(inner) => match classes.resolve(&inner) {
            Type::Object => FieldShape::Userdata,
            Type::Class(id) => match classes.class(id) {
                Some(class) if class.is_value => FieldShape::Pointer {
                    target: id,
                    header: class.is_boundary && class.is_embedded_header,
                    constant: false,
                },
                Some(_) => FieldShape::Bytes,
                None => FieldShape::Unlowered,
            },
            _ => FieldShape::Unlowered,
        },
        _ => FieldShape::Unlowered,
    }
}

/// The LIR class table: the view of both code-generation tiers.
impl BoundaryClasses for crate::lir::Module {
    fn class(&self, id: ClassId) -> Option<ClassFacts> {
        self.classes.get(id.0).map(|class| {
            ClassFacts::new(
                class.is_value,
                class.is_boundary,
                class.is_embedded_header,
                class
                    .fields
                    .iter()
                    .map(|field| {
                        (
                            field.ty.clone(),
                            field.foreign_provenance
                                == Some(crate::lir::ForeignTypeProvenance::ConstPointer),
                        )
                    })
                    .collect(),
            )
        })
    }

    fn class_count(&self) -> usize {
        self.classes.len()
    }
}

/// The checked class table. The checker's own scan reads the same classes
/// while it resolves them; this view reads them after the check.
impl BoundaryClasses for crate::hir::Module {
    fn class(&self, id: ClassId) -> Option<ClassFacts> {
        self.classes.get(id.0).map(|class| {
            ClassFacts::new(
                class.is_value,
                class.is_boundary,
                is_embedded_header(&self.classes, &self.foreign_fns, id),
                class
                    .fields
                    .iter()
                    .map(|field| {
                        (
                            field.ty.clone(),
                            field.foreign_provenance
                                == Some(crate::hir::ForeignTypeProvenance::ConstPointer),
                        )
                    })
                    .collect(),
            )
        })
    }

    fn class_count(&self) -> usize {
        self.classes.len()
    }
}

/// Adapts a class table to [`StructView`]. A struct is a boundary value
/// class.
#[derive(Debug, Clone, Copy)]
pub struct Classes<'c, C: ?Sized>(pub &'c C);

impl<C: BoundaryClasses + ?Sized> StructView for Classes<'_, C> {
    type Struct = ClassId;

    fn structs(&self) -> Vec<ClassId> {
        (0..self.0.class_count())
            .map(ClassId)
            .filter(|id| {
                self.0
                    .class(*id)
                    .is_some_and(|class| class.is_value && class.is_boundary)
            })
            .collect()
    }

    fn fields(&self, s: ClassId) -> Option<Vec<FieldShape<ClassId>>> {
        let class = self.0.class(s)?;
        if !(class.is_value && class.is_boundary) {
            return None;
        }
        Some(
            class
                .fields
                .iter()
                .zip(&class.constant)
                .map(|(ty, constant)| with_constant(field_shape(self.0, ty), *constant))
                .collect(),
        )
    }
}

/// True when a class whose first field has type `first` extends class `id`
/// (§33 rule 9): the field is `id` by value. The binder, the checker, and
/// code generation compare the first field the same way, with no alias
/// resolution.
#[must_use]
pub fn extends(first: &Type, id: ClassId) -> bool {
    *first == Type::Class(id)
}

/// True when boundary value class `header` is an embedded header (§33
/// rule 9): it is the first field of another boundary value class, and a
/// boundary field or a foreign parameter is typed `header | null`.
#[must_use]
pub fn is_embedded_header(
    classes: &[ClassDef],
    foreign_fns: &[ForeignFn],
    header: ClassId,
) -> bool {
    if !classes
        .get(header.0)
        .is_some_and(|class| class.is_value && class.is_boundary)
    {
        return false;
    }
    let link = Type::Nullable(Box::new(Type::Class(header)));
    let used_as_link = classes
        .iter()
        .any(|class| class.is_boundary && class.fields.iter().any(|field| field.ty == link))
        || foreign_fns
            .iter()
            .any(|function| function.params.iter().any(|parameter| parameter.ty == link));
    used_as_link
        && classes.iter().any(|class| {
            class.is_value
                && class.is_boundary
                && class
                    .fields
                    .first()
                    .is_some_and(|field| extends(&field.ty, header))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_boundary::{struct_pass, StructPass};

    /// A table of `(is_value, is_boundary, is_embedded_header, fields)`.
    struct Table(Vec<(bool, bool, bool, Vec<Type>)>);

    impl BoundaryClasses for Table {
        fn class(&self, id: ClassId) -> Option<ClassFacts> {
            self.0.get(id.0).map(|(value, boundary, header, fields)| {
                ClassFacts::new(
                    *value,
                    *boundary,
                    *header,
                    fields.iter().map(|ty| (ty.clone(), false)).collect(),
                )
            })
        }

        fn class_count(&self) -> usize {
            self.0.len()
        }
    }

    fn nullable(ty: Type) -> Type {
        Type::Nullable(Box::new(ty))
    }

    #[test]
    fn each_member_type_maps_to_its_shape() {
        let table = Table(vec![
            (true, true, false, vec![Type::I32]),
            (false, false, false, Vec::new()),
            (true, true, true, vec![Type::I32]),
        ]);
        let point = Type::Class(ClassId(0));
        let handle = Type::Class(ClassId(1));
        let header = Type::Class(ClassId(2));
        let unknown = Type::Class(ClassId(9));
        let cases = [
            (Type::I32, FieldShape::Bytes),
            (Type::Bool, FieldShape::Bytes),
            (Type::Object, FieldShape::Userdata),
            (nullable(Type::Object), FieldShape::Userdata),
            (Type::Str, FieldShape::StringView),
            (
                Type::Array(Box::new(Type::U32)),
                FieldShape::Pair { elements: None },
            ),
            (
                Type::Array(Box::new(point.clone())),
                FieldShape::Pair {
                    elements: Some(ClassId(0)),
                },
            ),
            (Type::FixedArray(Box::new(Type::U8), 4), FieldShape::Bytes),
            (
                Type::FixedArray(Box::new(point.clone()), 2),
                FieldShape::Embedded(ClassId(0)),
            ),
            (point.clone(), FieldShape::Embedded(ClassId(0))),
            (handle.clone(), FieldShape::Bytes),
            (nullable(handle), FieldShape::Bytes),
            (
                nullable(point),
                FieldShape::Pointer {
                    target: ClassId(0),
                    header: false,
                    constant: false,
                },
            ),
            (
                nullable(header),
                FieldShape::Pointer {
                    target: ClassId(2),
                    header: true,
                    constant: false,
                },
            ),
            (unknown, FieldShape::Unlowered),
            (nullable(Type::Str), FieldShape::Unlowered),
            (Type::Void, FieldShape::Unlowered),
        ];
        for (ty, shape) in cases {
            assert_eq!(field_shape(&table, &ty), shape, "{ty:?}");
        }
    }

    /// The checked classes of a chain: `Header` is the first field of
    /// `Ext` and a field and a parameter link it, so it is an embedded
    /// header and its link does not make a scratch struct.
    #[test]
    fn a_checked_chain_header_is_an_embedded_header() {
        let mirror = "// @subscript-c-header include=\"chain.h\"\n\
            declare class Header { kind: i32; next: Header | null; \
            constructor(kind: i32, next: Header | null); }\n\
            declare class Ext { header: Header; extra: i32; \
            constructor(header: Header, extra: i32); }\n\
            declare class Holder { k: i32; ext: Ext | null; \
            constructor(k: i32, ext: Ext | null); }\n\
            declare function use(header: Header | null): i32;\n";
        let module = crate::check_program(&[
            crate::SourceFile::ambient("chain.d.ts", mirror),
            crate::SourceFile::new("main.ts", "export function main(): void {}\n"),
        ])
        .expect("the chain checks");
        let id = |name: &str| {
            ClassId(
                module
                    .classes
                    .iter()
                    .position(|class| class.name == name)
                    .expect("class"),
            )
        };
        let headers: Vec<_> = ["Header", "Ext", "Holder"]
            .iter()
            .map(|name| is_embedded_header(&module.classes, &module.foreign_fns, id(name)))
            .collect();
        assert_eq!(headers, [true, false, false]);
        let view = Classes(&module);
        assert_eq!(struct_pass(&view, id("Header")), StructPass::Bytes);
        assert_eq!(struct_pass(&view, id("Ext")), StructPass::Bytes);
        assert_eq!(struct_pass(&view, id("Holder")), StructPass::Scratch);
        assert_eq!(
            view.fields(id("Holder")),
            Some(vec![
                FieldShape::Bytes,
                FieldShape::Pointer {
                    target: id("Ext"),
                    header: false,
                    constant: false,
                },
            ])
        );
    }

    #[test]
    fn the_view_defines_boundary_value_classes_only() {
        let table = Table(vec![
            (true, true, false, vec![Type::I32]),
            (true, false, false, vec![Type::I32]),
            (false, true, false, vec![Type::I32]),
            (true, true, false, vec![Type::Class(ClassId(1))]),
        ]);
        let view = Classes(&table);
        assert_eq!(view.fields(ClassId(0)), Some(vec![FieldShape::Bytes]));
        assert_eq!(view.fields(ClassId(1)), None);
        assert_eq!(view.fields(ClassId(2)), None);
        assert_eq!(struct_pass(&view, ClassId(0)), StructPass::Bytes);
        // An embedded value class that is not a boundary struct has no
        // byte image.
        assert_eq!(struct_pass(&view, ClassId(3)), StructPass::Scratch);
    }
}
