use super::*;
use crate::check::{rejection::RejectionSite, FnCtx};

impl<'p> Checker<'p> {
    pub(super) fn check_promise_all(
        &mut self,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if call.type_args.is_some() {
            self.reject_subset(
                RejectionSite::PromiseAllTypeArguments,
                "Promise.all takes no explicit type arguments",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let [argument] = call.args.as_slice() else {
            self.reject_subset(
                RejectionSite::PromiseAllArguments,
                "Promise.all requires one Promise<T>[] argument",
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        if argument.spread.is_some() {
            self.reject_subset(
                RejectionSite::PromiseAllSpread,
                "spread arguments require variadic parameters, which the language does not have",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let jobs = self.check_expr(&argument.expr, None, fx);
        let Type::Array(element) = self.apparent_type(&jobs.ty) else {
            if self.apparent_type(&jobs.ty) != Type::Error {
                self.reject_subset(
                    RejectionSite::PromiseAllInput,
                    "Promise.all requires Promise<T>[]",
                    pos.clone(),
                );
            }
            return self.err_expr(pos);
        };
        let Type::AsyncHandle(result) = self.apparent_type(&element) else {
            if self.apparent_type(&element) != Type::Error {
                self.reject_subset(
                    RejectionSite::PromiseAllInput,
                    "Promise.all requires Promise<T>[]",
                    pos.clone(),
                );
            }
            return self.err_expr(pos);
        };
        if self.apparent_type(&result).carries_async_handle() {
            self.reject_subset(
                RejectionSite::PromiseAllCountedResult,
                "Promise.all cannot store a counted result",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let origins = self.expr_async_origins(&jobs, fx);
        fx.handle_async_origins(&origins);
        let origin = fx.register_async_origin(pos.clone());
        hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::AsyncAll {
                jobs: Box::new(jobs),
                origin,
            },
            ty: Type::AsyncHandle(Box::new(Type::array(*result))),
            pos,
        }
    }
}
