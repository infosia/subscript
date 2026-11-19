//! Optional parameter facts for inferred function bindings (§164).
use super::{Checker, FnCtx};
use crate::hir::{self, ExprKind};

impl<'p> Checker<'p> {
    pub(super) fn inferred_function_required(
        &self,
        source: &oxc_ast::ast::Expression,
        value: &hir::Expr,
        fx: &FnCtx<'p>,
    ) -> Option<usize> {
        if let oxc_ast::ast::Expression::ArrowFunctionExpression(arrow) =
            super::expr::unparen_expr(source)
        {
            if arrow.params.items.iter().any(|p| p.initializer.is_some()) {
                if arrow.params.rest.is_some() {
                    return Some(arrow.params.items.len() + 1);
                }
                return Some(
                    arrow
                        .params
                        .items
                        .iter()
                        .rposition(|p| p.initializer.is_none())
                        .map_or(0, |i| i + 1),
                );
            }
        }
        self.function_value_required(value, fx)
    }

    pub(super) fn check_function_binding_assignment(
        &mut self,
        target: &hir::Expr,
        source: &oxc_ast::ast::Expression,
        value: &hir::Expr,
        fx: &FnCtx<'p>,
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
            ExprKind::Global(symbol, _) => self
                .global_sigs
                .get(symbol.full_text())
                .and_then(|sig| sig.function_value_required),
            // A selection retains optional-argument facts, but does not name
            // a known function whose defaults an adapter can call (§320 rule 3).
            ExprKind::Cond { then, els, .. } => Some(
                self.function_value_required(then, fx)?
                    .max(self.function_value_required(els, fx)?),
            ),
            _ => None,
        }
    }
}

impl<'p> Checker<'p> {
    /// The existing call form that evaluates an inferred const's defaults
    /// (compiler.md §320 rule 2). Lambda defaults keep the value's environment.
    pub(super) fn adapter_default_callee(
        &mut self,
        value: &hir::Expr,
        fx: &FnCtx<'p>,
    ) -> Option<hir::Callee> {
        let callee = match &value.kind {
            ExprKind::FuncRef(symbol) => hir::Callee::Func(symbol.clone()),
            ExprKind::Lambda { id, params, .. } => hir::Callee::NamedLambda {
                value: Box::new(value.clone()),
                defaults: hir::LambdaDefaults {
                    id: *id,
                    params: params.clone(),
                },
            },
            ExprKind::Local(name, ..) => fx
                .scopes
                .iter()
                .rev()
                .find(|scope| scope.owns(name))?
                .adapter_defaults
                .get(name)?
                .clone(),
            ExprKind::Global(symbol, _) => {
                if let Some(defaults) = self.named_global_lambda_defaults(symbol) {
                    hir::Callee::NamedLambda {
                        value: Box::new(value.clone()),
                        defaults,
                    }
                } else {
                    let source = self.global_adapter_source(symbol)?;
                    self.adapter_default_callee(&source, fx)?
                }
            }
            _ => return None,
        };
        Some(match callee {
            hir::Callee::NamedLambda { defaults, .. } => hir::Callee::NamedLambda {
                value: Box::new(value.clone()),
                defaults,
            },
            callee => callee,
        })
    }

    /// Preserve defaults across const aliases without changing ordinary calls
    /// through those aliases (compiler.md §320 rules 1–3, §288 rule 4).
    pub(super) fn remember_adapter_defaults(
        &mut self,
        name: &str,
        mutable: bool,
        annotated: bool,
        value: &hir::Expr,
        fx: &mut FnCtx<'p>,
    ) {
        if mutable || annotated || self.function_value_required(value, fx).is_none() {
            return;
        }
        if let Some(callee) = self.adapter_default_callee(value, fx) {
            if let Some(scope) = fx.scopes.last_mut() {
                scope.adapter_defaults.insert(name.to_string(), callee);
            }
        }
    }

    pub(super) fn prepare_named_lambda(
        &self,
        declaration: &oxc_ast::ast::VariableDeclarator,
        mutable: bool,
        name: &str,
        annotation: Option<&crate::types::Type>,
        fx: &mut FnCtx<'p>,
    ) {
        use oxc_ast::ast::Expression as Expr;
        if mutable || name.is_empty() {
            return;
        }
        if let Some(Expr::ArrowFunctionExpression(arrow)) =
            declaration.init.as_ref().map(super::expr::unparen_expr)
        {
            // compiler.md §285 rule 4 and §288 rule 1.
            if annotation.is_none_or(|ty| {
                matches!(self.apparent_type(ty), crate::types::Type::Func(function)
                    if function.params.len() == arrow.params.items.len())
            }) {
                fx.lambda_initializer = Some((arrow.span, name.to_string(), annotation.is_none()));
            }
        }
    }

    pub(super) fn remember_named_lambda(
        &self,
        name: &str,
        declaration: &oxc_ast::ast::VariableDeclarator,
        mutable: bool,
        value: &hir::Expr,
        fx: &mut FnCtx<'p>,
    ) {
        use oxc_ast::ast::{BindingPattern as Pat, Expression as Expr};
        if mutable
            || !matches!(&declaration.id, Pat::BindingIdentifier(binding) if declaration.type_annotation.is_none())
            || !matches!(
                declaration.init.as_ref().map(super::expr::unparen_expr),
                Some(Expr::ArrowFunctionExpression(_))
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
        fx: &FnCtx<'p>,
    ) -> Option<hir::LambdaDefaults> {
        match &value.kind {
            ExprKind::Local(name, ..) => fx
                .scopes
                .iter()
                .rev()
                .find(|scope| scope.owns(name))
                .and_then(|scope| scope.named_lambdas.get(name).cloned()),
            ExprKind::Global(symbol, _) => self.named_global_lambda_defaults(symbol),
            _ => None,
        }
    }

    pub(in crate::check) fn lambda_default_signature(
        defaults: &hir::LambdaDefaults,
    ) -> Vec<super::ParamSig<'p>> {
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
