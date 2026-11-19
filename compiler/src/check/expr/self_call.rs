//! Direct group calls, including the running local lambda (compiler.md §293, §284).

use super::*;
use crate::check::{rejection::RejectionSite, FnCtx, ParamSig};

impl<'p> Checker<'p> {
    pub(super) fn check_lambda_self_call(
        &mut self,
        name: &ast::IdentifierReference,
        call: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
        pos: &Pos,
    ) -> Option<hir::Expr> {
        if let Some(value) = self.check_group_call(name, call, fx, pos) {
            return Some(value);
        }
        let (id, signature, is_async, defaults) = fx
            .scopes
            .iter()
            .rev()
            .find_map(|scope| {
                if scope.owns(name.name.as_ref()) {
                    return Some(None);
                }
                if let Some((binding, id, signature, is_async, defaults)) = &scope.self_call {
                    if binding == name.name.as_ref() {
                        return Some(Some((*id, signature.clone(), *is_async, defaults.clone())));
                    }
                }
                scope.fn_boundary.then_some(None)
            })
            .flatten()?;
        self.mark_user_call(call, false);
        let defaults = defaults.filter(|_| call.arguments.len() < signature.params.len());
        let params = defaults
            .as_ref()
            .map(Self::lambda_default_signature)
            .unwrap_or_else(|| {
                signature
                    .params
                    .iter()
                    .cloned()
                    .map(ParamSig::positional)
                    .collect::<Vec<_>>()
            });
        let args = self.check_args_with_arguments(
            if defaults.is_some() {
                RejectionSite::SourceFunctionArgumentCount
            } else {
                RejectionSite::FunctionValueArgumentCount
            },
            &params,
            &call.arguments,
            fx,
            pos,
            name.name.as_ref(),
            None,
            false,
        );
        let value = hir::Expr {
            pending_work: None,
            ty: signature.ret.clone(),
            kind: ExprKind::Call {
                callee: Callee::LambdaSelf {
                    name: std::sync::Arc::new(name.name.to_string()),
                    id,
                    signature,
                    is_async,
                    defaults,
                },
                args,
            },
            pos: pos.clone(),
        };
        Some(self.track_async_call_result(value, fx))
    }
}
