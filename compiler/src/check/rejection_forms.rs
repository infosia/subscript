//! Form selectors preserve the diagnostic message and position (§312 rule 1).
use super::*;

impl FnCtx {
    pub(super) fn parameter_local(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .find(|scope| scope.vars.contains_key(name))
            .is_some_and(|scope| scope.parameter_names.contains(name))
    }

    pub(super) fn parameter_value(&self, value: &hir::Expr) -> bool {
        matches!(&value.kind, hir::ExprKind::Local(name, ..) if self.parameter_local(name))
    }
}

impl Checker<'_> {
    /// A dynamic import has the same site under await (§312 rule 2).
    pub(super) fn is_dynamic_import_call(call: &ast::CallExpr) -> bool {
        matches!(call.callee, ast::Callee::Import(_))
            && call.type_args.is_none()
            && !call.args.is_empty()
            && call.args.len() <= 2
            && !call.args.first().is_some_and(|argument| {
                matches!(&*argument.expr, ast::Expr::Lit(lit) if !matches!(lit, ast::Lit::Str(_)))
            })
    }

    pub(super) fn pending_capture_site(&self, name: &str, fx: &FnCtx) -> RejectionSite {
        let mut scopes = fx
            .scopes
            .iter()
            .rev()
            .skip_while(|scope| !scope.owns(name) && !scope.group_members.contains_key(name));
        scopes.next();
        if self.peek_scope_item(name).is_some()
            || scopes.any(|scope| scope.vars.contains_key(name) || scope.pending.contains(name))
        {
            RejectionSite::BlockPendingReadWithoutProgramShadow
        } else {
            RejectionSite::LaterLocalCaptureRead
        }
    }
}
