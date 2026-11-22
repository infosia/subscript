use super::*;
use crate::check::{rejection::RejectionSite, FnCtx};
use hir::AsyncAggregate;

/// The rejection sites of one aggregate: type arguments, arguments,
/// spread, input, counted result.
fn sites(mode: AsyncAggregate) -> [RejectionSite; 5] {
    match mode {
        AsyncAggregate::Race => [
            RejectionSite::PromiseRaceTypeArguments,
            RejectionSite::PromiseRaceArguments,
            RejectionSite::PromiseRaceSpread,
            RejectionSite::PromiseRaceInput,
            RejectionSite::PromiseRaceCountedResult,
        ],
        _ => [
            RejectionSite::PromiseAllTypeArguments,
            RejectionSite::PromiseAllArguments,
            RejectionSite::PromiseAllSpread,
            RejectionSite::PromiseAllInput,
            RejectionSite::PromiseAllCountedResult,
        ],
    }
}

impl<'p> Checker<'p> {
    /// Checks `Promise.all(inputs)` (§166) and `Promise.race(inputs)`
    /// (§350 rules 1 and 2). The argument rules are the same; the result
    /// of a race is the element handle type.
    pub(super) fn check_promise_all(
        &mut self,
        call: &ast::CallExpression,
        mode: AsyncAggregate,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        let [type_arguments, arguments, spread, input, counted] = sites(mode);
        let name = match mode {
            AsyncAggregate::Race => "Promise.race",
            _ => "Promise.all",
        };
        if call.type_arguments.is_some() {
            self.reject_subset(
                type_arguments,
                format!("{name} takes no explicit type arguments"),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let [argument] = call.arguments.as_slice() else {
            self.reject_subset(
                arguments,
                format!("{name} requires one Promise<T>[] argument"),
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        if matches!(argument, ast::Argument::SpreadElement(_)) {
            let message = match mode {
                AsyncAggregate::Race => {
                    "Promise.race takes one Promise<T>[] argument, not a spread argument"
                }
                _ => {
                    "spread arguments require variadic parameters, which the language does not have"
                }
            };
            self.reject_subset(spread, message, pos.clone());
            return self.err_expr(pos);
        }
        let jobs = self.check_expr(crate::check::argument_expression(argument), None, fx);
        // The call consumes the jobs even when an element has a diagnostic.
        let origins = self.expr_async_origins(&jobs, fx);
        fx.handle_async_origins(&origins);
        let Type::Array(element) = self.apparent_type(&jobs.ty) else {
            if self.apparent_type(&jobs.ty) != Type::Error {
                self.reject_subset(input, format!("{name} requires Promise<T>[]"), pos.clone());
            }
            return self.err_expr(pos);
        };
        let Type::AsyncHandle(result) = self.apparent_type(&element) else {
            if self.apparent_type(&element) != Type::Error {
                self.reject_subset(input, format!("{name} requires Promise<T>[]"), pos.clone());
            }
            return self.err_expr(pos);
        };
        // §166 rule 12 and §350 rule 2: the aggregate copies the bytes of a
        // completion with no count, so each counted result type is rejected.
        if self.apparent_type(&result).counted_type().is_some() {
            self.reject_subset(
                counted,
                format!("{name} cannot store a counted result"),
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let origin = fx.register_async_origin(pos.clone());
        let ty = match mode {
            AsyncAggregate::Race => Type::AsyncHandle(result),
            _ => Type::AsyncHandle(Box::new(Type::array(*result))),
        };
        hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::AsyncAll {
                mode,
                jobs: Box::new(jobs),
                origin,
            },
            ty,
            pos,
        }
    }
}
