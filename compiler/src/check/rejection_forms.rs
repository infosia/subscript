//! Form selectors preserve the diagnostic message and position (§312 rule 1).
use super::*;

impl<'p> FnCtx<'p> {
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

impl<'p> Checker<'p> {
    /// A dynamic import has the same site under await (§312 rule 2).
    pub(super) fn is_dynamic_import_call(call: &ast::ImportExpression) -> bool {
        !matches!(
            &call.source,
            ast::Expression::NumericLiteral(_)
                | ast::Expression::BooleanLiteral(_)
                | ast::Expression::NullLiteral(_)
                | ast::Expression::BigIntLiteral(_)
                | ast::Expression::RegExpLiteral(_)
        )
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
