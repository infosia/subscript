//! Optional parameter facts for inferred function bindings (§164).
use super::{Checker, FnCtx};
use crate::hir::{self, ExprKind};

impl Checker<'_> {
    pub(super) fn inferred_function_required(
        &self,
        source: &swc_ecma_ast::Expr,
        value: &hir::Expr,
        fx: &FnCtx,
    ) -> Option<usize> {
        if let swc_ecma_ast::Expr::Arrow(arrow) = super::expr::unparen_expr(source) {
            if arrow
                .params
                .iter()
                .any(|p| matches!(p, swc_ecma_ast::Pat::Assign(_)))
            {
                return Some(
                    arrow
                        .params
                        .iter()
                        .rposition(|p| !matches!(p, swc_ecma_ast::Pat::Assign(_)))
                        .map_or(0, |i| i + 1),
                );
            }
        }
        self.function_value_required(value, fx)
    }

    pub(super) fn check_function_binding_assignment(
        &mut self,
        target: &hir::Expr,
        source: &swc_ecma_ast::Expr,
        value: &hir::Expr,
        fx: &FnCtx,
    ) {
        let Some(required) = self.function_value_required(target, fx) else {
            return;
        };
        let crate::types::Type::Func(signature) = self.apparent_type(&value.ty) else {
            return;
        };
        let value_required = self
            .inferred_function_required(source, value, fx)
            .unwrap_or(signature.params.len());
        if value_required > required {
            self.reject_subset(
                super::rejection::RejectionSite::AssignmentTypeMismatch,
                format!(
                    "the assignment needs {value_required} required argument(s), but the binding permits {required}"
                ),
                value.pos.clone(),
            );
        }
    }

    pub(super) fn function_value_required(&self, value: &hir::Expr, fx: &FnCtx) -> Option<usize> {
        match &value.kind {
            ExprKind::Lambda { params, .. } if params.iter().any(|p| p.default.is_some()) => Some(
                params
                    .iter()
                    .rposition(|p| p.default.is_none())
                    .map_or(0, |i| i + 1),
            ),
            ExprKind::FuncRef(symbol) => self.fn_sigs.get(symbol.full_text()).and_then(|sig| {
                sig.params.iter().any(|p| p.has_default).then(|| {
                    sig.params
                        .iter()
                        .rposition(|p| !p.has_default)
                        .map_or(0, |i| i + 1)
                })
            }),
            ExprKind::Local(name, ..) => fx
                .scopes
                .iter()
                .rev()
                .find_map(|scope| scope.vars.get(name))
                .and_then(|local| local.function_value_required),
            ExprKind::Global(symbol) => self
                .global_sigs
                .get(symbol.full_text())
                .and_then(|sig| sig.function_value_required),
            _ => None,
        }
    }
}
