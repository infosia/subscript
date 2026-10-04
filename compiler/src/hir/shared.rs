//! Shared path classification (compiler.md §124).
use super::*;

impl Expr {
    /// Whether another name can reach this storage location (compiler.md §124).
    #[must_use]
    pub fn is_shared_location(&self, classes: &[ClassDef]) -> bool {
        use ExprKind as K;
        match &self.kind {
            K::Global(_) => true,
            K::Field { obj, .. } => {
                matches!(obj.kind, K::This)
                    || receiver_is_shared(obj.storage_type(classes), classes, false)
                    || obj.is_shared_location(classes)
            }
            K::Cast(_)
            | K::Local(..)
            | K::This
            | K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Str(_)
            | K::Null
            | K::FuncRef(_)
            | K::EnumMember { .. }
            | K::Unary { .. }
            | K::Binary { .. }
            | K::AbsenceTest { .. }
            | K::Assign { .. }
            | K::Call { .. }
            | K::New { .. }
            | K::DescriptorLit { .. }
            | K::Zero
            | K::Unassigned
            | K::RawNew { .. }
            | K::Length(_)
            | K::Index { .. }
            | K::ArrayLit(_)
            | K::ArraySpreadLit(_)
            | K::Template(_)
            | K::Lambda { .. }
            | K::Yield(_)
            | K::AsyncSuspend
            | K::AsyncCall { .. }
            | K::AsyncHandleCreate { .. }
            | K::AsyncHandleAwait(_)
            | K::AsyncHandleTransfer { .. }
            | K::Cond { .. } => false,
        }
    }

    fn storage_type<'a>(&'a self, classes: &'a [ClassDef]) -> &'a Type {
        use ExprKind as K;
        match &self.kind {
            K::Local(_, declared) => declared,
            K::Field { obj, name } => {
                super::sites::declared_field_type(&obj.ty, name, classes).unwrap_or(&self.ty)
            }
            K::Global(_)
            | K::Cast(_)
            | K::This
            | K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Str(_)
            | K::Null
            | K::FuncRef(_)
            | K::EnumMember { .. }
            | K::Unary { .. }
            | K::Binary { .. }
            | K::AbsenceTest { .. }
            | K::Assign { .. }
            | K::Call { .. }
            | K::New { .. }
            | K::DescriptorLit { .. }
            | K::Zero
            | K::Unassigned
            | K::RawNew { .. }
            | K::Length(_)
            | K::Index { .. }
            | K::ArrayLit(_)
            | K::ArraySpreadLit(_)
            | K::Template(_)
            | K::Lambda { .. }
            | K::Yield(_)
            | K::AsyncSuspend
            | K::AsyncCall { .. }
            | K::AsyncHandleCreate { .. }
            | K::AsyncHandleAwait(_)
            | K::AsyncHandleTransfer { .. }
            | K::Cond { .. } => &self.ty,
        }
    }
}

fn receiver_is_shared(ty: &Type, classes: &[ClassDef], boxed: bool) -> bool {
    match ty {
        Type::Class(id) => classes
            .get(id.0)
            .is_some_and(|class| !class.is_value || boxed && class.is_boundary),
        Type::Nullable(inner) => receiver_is_shared(inner, classes, true),
        Type::IterResult(_) => false,
        Type::I8
        | Type::U8
        | Type::I16
        | Type::U16
        | Type::I32
        | Type::U32
        | Type::I64
        | Type::U64
        | Type::F32
        | Type::F64
        | Type::F16
        | Type::Bool
        | Type::Str
        | Type::Date
        | Type::RegExp
        | Type::Void
        | Type::Null
        | Type::Object
        | Type::Enum(_)
        | Type::StringAlias(_)
        | Type::FixedArray(..)
        | Type::Array(_)
        | Type::Map(..)
        | Type::Set(_)
        | Type::Worker(..)
        | Type::Inbox(_)
        | Type::Outbox(_)
        | Type::Func(_)
        | Type::Generator(_)
        | Type::AsyncHandle(_)
        | Type::Error => false,
        // §143: no type parameter reaches the HIR.
        Type::TypeParameter(_) | Type::GenericNumber | Type::GenericUnion(_) => false,
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn shared_path_predicate_distinguishes_value_receivers() {
        use crate::{
            diag::Pos,
            hir::{Expr, ExprKind, Symbol},
            types::{ClassId, Type},
        };
        let module = crate::check_program(&[crate::SourceFile::new(
            "test.ts",
            "class Ref { } @ValueType class Value { n: i32 = 0; }",
        )])
        .expect("class definitions");
        let class_type = |name: &str| {
            Type::Class(ClassId(
                module
                    .classes
                    .iter()
                    .position(|class| class.name == name)
                    .expect("class"),
            ))
        };
        let expr = |kind, ty| Expr {
            pending_work: None,
            kind,
            ty,
            pos: Pos {
                file: "test.ts".into(),
                line: 1,
                col: 1,
            },
        };
        let field = |obj| {
            expr(
                ExprKind::Field {
                    obj: Box::new(obj),
                    name: "value".into(),
                },
                Type::I32,
            )
        };
        for ty in [class_type("Value"), Type::IterResult(Box::new(Type::I32))] {
            let local = expr(ExprKind::Local("r".into(), ty.clone()), ty.clone());
            assert!(!field(local).is_shared_location(&module.classes));
            let global = expr(ExprKind::Global(Symbol::from_full_text("r")), ty.clone());
            assert!(field(global).is_shared_location(&module.classes));
            let this = expr(ExprKind::This, ty);
            assert!(field(this).is_shared_location(&module.classes));
        }
        let local_ref = expr(
            ExprKind::Local("r".into(), class_type("Ref")),
            class_type("Ref"),
        );
        assert!(!local_ref.is_shared_location(&module.classes));
        assert!(field(local_ref).is_shared_location(&module.classes));
        let global = expr(ExprKind::Global(Symbol::from_full_text("S.opt")), Type::I32);
        assert!(global.is_shared_location(&module.classes));
    }

    #[test]
    fn boxed_local_fields_are_shared_but_value_copies_are_local() {
        use crate::{check_program, SourceFile};
        use crate::{
            hir::{Expr, ExprKind, TrapSite},
            types::Type,
        };
        let files = [SourceFile::ambient(
            "interop.generated.d.ts",
            include_str!("../../../corpus/interop/interop.generated.d.ts"),
        )];
        let module = check_program(&files).expect("boundary declarations");
        let class = |name: &str| {
            Type::Class(crate::types::ClassId(
                module
                    .classes
                    .iter()
                    .position(|c| c.name == name)
                    .expect("boundary class"),
            ))
        };
        let target = class("SGPUProbeColorTargetState");
        let blend = class("SGPUProbeBlendState");
        for boxed in [false, true] {
            let storage = if boxed {
                Type::Nullable(Box::new(target.clone()))
            } else {
                target.clone()
            };
            for receiver in [target.clone(), storage.clone()] {
                let field = Expr {
                    pending_work: None,
                    kind: ExprKind::Field {
                        obj: Box::new(Expr {
                            pending_work: None,
                            kind: ExprKind::Local("t".into(), storage.clone()),
                            ty: receiver,
                            pos: crate::Pos::new("test.ts", 1, 1),
                        }),
                        name: "blend".into(),
                    },
                    ty: blend.clone(),
                    pos: crate::Pos::new("test.ts", 1, 3),
                };
                assert_eq!(field.is_shared_location(&module.classes), boxed);
                let nested = Expr {
                    pending_work: None,
                    kind: ExprKind::Field {
                        obj: Box::new(field.clone()),
                        name: "colorOperation".into(),
                    },
                    ty: Type::I32,
                    pos: field.pos.clone(),
                };
                assert!(nested.is_shared_location(&module.classes));
                assert_eq!(
                    field
                        .trap_sites(&module)
                        .iter()
                        .any(|site| matches!(site, TrapSite::NullNarrowing { .. })),
                    boxed
                );
            }
        }
    }
}
