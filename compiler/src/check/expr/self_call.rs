//! Calls of the running local lambda (compiler.md §284).

use super::*;
use crate::check::{rejection::RejectionSite, FnCtx, ParamSig};

impl Checker<'_> {
    pub(super) fn check_lambda_self_call(
        &mut self,
        name: &ast::Ident,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: &Pos,
    ) -> Option<hir::Expr> {
        let (id, signature, is_async) = fx
            .scopes
            .iter()
            .rev()
            .find_map(|scope| {
                if scope.owns(name.sym.as_ref()) {
                    return Some(None);
                }
                if let Some((binding, id, signature, is_async)) = &scope.self_call {
                    if binding == name.sym.as_ref() {
                        return Some(Some((*id, signature.clone(), *is_async)));
                    }
                }
                scope.fn_boundary.then_some(None)
            })
            .flatten()?;
        self.mark_user_call(call, false);
        let params = signature
            .params
            .iter()
            .cloned()
            .map(ParamSig::positional)
            .collect::<Vec<_>>();
        let args = self.check_args_with_arguments(
            RejectionSite::FunctionValueArgumentCount,
            &params,
            &call.args,
            fx,
            pos,
            name.sym.as_ref(),
            None,
            false,
        );
        let value = hir::Expr {
            pending_work: None,
            ty: signature.ret.clone(),
            kind: ExprKind::Call {
                callee: Callee::LambdaSelf {
                    id,
                    signature,
                    is_async,
                },
                args,
            },
            pos: pos.clone(),
        };
        Some(self.track_async_call_result(value, fx))
    }
}
