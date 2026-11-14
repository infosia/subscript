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

impl Checker<'_> {
    pub(super) fn prepare_named_lambda(
        &self,
        declaration: &swc_ecma_ast::VarDeclarator,
        mutable: bool,
        name: &str,
        annotation: Option<&crate::types::Type>,
        fx: &mut FnCtx,
    ) {
        use swc_ecma_ast::Expr;
        if mutable || name.is_empty() {
            return;
        }
        if let Some(Expr::Arrow(arrow)) = declaration.init.as_deref().map(super::expr::unparen_expr)
        {
            // compiler.md §285 rule 4 and §288 rule 1.
            if annotation.is_none_or(|ty| {
                matches!(self.apparent_type(ty), crate::types::Type::Func(function)
                    if function.params.len() == arrow.params.len())
            }) {
                fx.lambda_initializer = Some((arrow.span, name.to_string(), annotation.is_none()));
            }
        }
    }

    pub(super) fn remember_named_lambda(
        &self,
        name: &str,
        declaration: &swc_ecma_ast::VarDeclarator,
        mutable: bool,
        value: &hir::Expr,
        fx: &mut FnCtx,
    ) {
        use swc_ecma_ast::{Expr, Pat};
        if mutable
            || !matches!(&declaration.name, Pat::Ident(binding) if binding.type_ann.is_none())
            || !matches!(
                declaration.init.as_deref().map(super::expr::unparen_expr),
                Some(Expr::Arrow(_))
            )
        {
            return;
        }
        if let (ExprKind::Lambda { id, params, .. }, Some(scope)) =
            (&value.kind, fx.scopes.last_mut())
        {
            scope.named_lambdas.insert(
                name.to_string(),
                hir::LambdaDefaults {
                    id: *id,
                    params: params.clone(),
                },
            );
        }
    }

    pub(in crate::check) fn named_lambda_defaults(
        &mut self,
        value: &hir::Expr,
        fx: &FnCtx,
    ) -> Option<hir::LambdaDefaults> {
        match &value.kind {
            ExprKind::Local(name, ..) => fx
                .scopes
                .iter()
                .rev()
                .find(|scope| scope.owns(name))
                .and_then(|scope| scope.named_lambdas.get(name).cloned()),
            ExprKind::Global(symbol) => self.named_global_lambda_defaults(symbol),
            _ => None,
        }
    }

    pub(in crate::check) fn check_omitted_lambda_defaults(
        &mut self,
        defaults: &hir::LambdaDefaults,
        supplied: usize,
    ) {
        fn captured(value: &hir::Expr, earlier: &[hir::Param]) -> Option<String> {
            if let ExprKind::Local(name, ..) = &value.kind {
                if !earlier.iter().any(|parameter| parameter.name == *name) {
                    return Some(name.clone());
                }
            }
            if let ExprKind::Lambda { captures, .. } = &value.kind {
                return captures
                    .iter()
                    .find(|capture| !earlier.iter().any(|p| p.name == capture.name))
                    .map(|capture| capture.name.clone());
            }
            value.children().into_iter().find_map(|child| match child {
                hir::HirChild::Expr(child) => captured(child, earlier),
                hir::HirChild::Stmt(_) => None,
            })
        }
        for (index, parameter) in defaults.params.iter().enumerate().skip(supplied) {
            let Some(default) = &parameter.default else {
                continue;
            };
            if let Some(name) = captured(default, &defaults.params[..index]) {
                let key = (default.pos.file.clone(), default.pos.line, default.pos.col);
                if self.reported_lambda_defaults.insert(key) {
                    self.reject_subset(
                        super::rejection::RejectionSite::LambdaDefaultCapture,
                        format!("a named lambda default cannot read captured local `{name}`"),
                        default.pos.clone(),
                    );
                }
            }
        }
    }

    pub(in crate::check) fn lambda_default_signature(
        defaults: &hir::LambdaDefaults,
    ) -> Vec<super::ParamSig> {
        defaults
            .params
            .iter()
            .map(|parameter| {
                let mut signature = super::ParamSig::positional(parameter.ty.clone());
                signature.name = parameter.name.clone();
                signature.has_default = parameter.default.is_some();
                signature
            })
            .collect()
    }
}
