//! Task groups have one lexical owner and synchronous borrows (§170).

use super::*;
use crate::check::{rejection::RejectionSite, FnCtx, ParamSig};
use crate::hir::TaskGroupOperation as G;

impl<'p> Checker<'p> {
    pub(in crate::check) fn annotation_has_task_group(ty: &ast::TsType) -> bool {
        match ty {
            ast::TsType::TsTypeRef(reference) => {
                matches!(&reference.type_name,
                ast::TsEntityName::Ident(id) if id.sym.as_ref() == "TaskGroup")
                    || reference.type_params.as_ref().is_some_and(|params| {
                        params
                            .params
                            .iter()
                            .any(|ty| Self::annotation_has_task_group(ty))
                    })
            }
            ast::TsType::TsArrayType(array) => Self::annotation_has_task_group(&array.elem_type),
            ast::TsType::TsParenthesizedType(paren) => {
                Self::annotation_has_task_group(&paren.type_ann)
            }
            ast::TsType::TsUnionOrIntersectionType(union) => match union {
                ast::TsUnionOrIntersectionType::TsUnionType(union) => union
                    .types
                    .iter()
                    .any(|ty| Self::annotation_has_task_group(ty)),
                ast::TsUnionOrIntersectionType::TsIntersectionType(intersection) => intersection
                    .types
                    .iter()
                    .any(|ty| Self::annotation_has_task_group(ty)),
            },
            _ => false,
        }
    }

    pub(super) fn check_task_group_new(
        &mut self,
        new: &ast::NewExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if fx.frames.iter().any(|frame| frame.is_generator) {
            self.reject_subset(
                RejectionSite::TaskGroupGeneratorBody,
                "TaskGroup is not allowed in a generator body",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if !self.task_group_local {
            self.reject_subset(
                RejectionSite::TaskGroupPosition,
                "new TaskGroup() requires one const local",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        if new.type_args.is_some() || new.args.as_ref().is_some_and(|args| !args.is_empty()) {
            self.reject_subset(
                RejectionSite::ReferenceConstructorArgumentCount,
                "TaskGroup requires zero arguments",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let origin = fx.register_async_origin(pos.clone());
        fx.group_origins.insert(origin);
        hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::TaskGroup {
                operation: G::Create,
                args: Vec::new(),
                origin: Some(origin),
            },
            ty: Type::TaskGroup,
            pos,
        }
    }

    pub(super) fn check_task_group_method(
        &mut self,
        receiver: hir::Expr,
        name: &str,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let (operation, params, ty) = match name {
            "add" => (
                G::Add,
                vec![ParamSig::positional(Type::async_handle(Type::Void))],
                Type::Void,
            ),
            "join" => (G::Join, Vec::new(), Type::async_handle(Type::Void)),
            _ => {
                self.reject_subset(
                    RejectionSite::TaskGroupUnsupported,
                    format!("TaskGroup has no method `{name}`"),
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
        };
        if call.type_args.is_some() {
            self.reject_subset(
                RejectionSite::NonGenericStaticMethodTypeArguments,
                "TaskGroup methods take no type arguments",
                pos.clone(),
            );
        }
        let mut args = vec![receiver.clone()];
        args.extend(self.check_args(
            RejectionSite::ContextMethodArgumentCount,
            &params,
            &call.args,
            fx,
            &pos,
            name,
        ));
        let origin = if operation == G::Join {
            fx.handle_async_origins(&self.expr_async_origins(&receiver, fx));
            Some(fx.register_async_origin(pos.clone()))
        } else {
            None
        };
        hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::TaskGroup {
                operation,
                args,
                origin,
            },
            ty,
            pos,
        }
    }
}
