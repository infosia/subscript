//! The arity and the shape of a built-in callback (compiler.md §235 rule
//! 4, §242, §244): the parameter lists that a method accepts, and the
//! check of a checked callback value against them.

use crate::check::rejection::RejectionSite;

use crate::check::{Checker, FnCtx};
use crate::diag::Pos;
use crate::hir::{self, ArrFn};
use crate::types::Type;

use super::method::counted_method_message;
use super::CallbackSpec;

impl Checker<'_> {
    /// Uses §143 rule 1b for parameter types and exact project kinds for concrete callback types.
    fn callback_type_fits(&self, from: &Type, to: &Type) -> bool {
        if self.involves_type_parameter(from) || self.involves_type_parameter(to) {
            self.assignable(from, to)
        } else {
            // compiler.md §235 rule 4: a built-in callback takes rule 2.
            self.type_fits(from, to)
        }
    }

    /// Selects the contextual parameter list for a callback's source
    /// arity and emits the subset diagnostic when no accepted list has
    /// that length. With `fewer`, a lambda literal with fewer parameters
    /// takes the list of the smallest arity (compiler.md §244 rule 1); a
    /// function value with fewer parameters is padded by
    /// `expect_callback_shape`, or keeps this diagnostic.
    pub(super) fn callback_params_for_arity(
        &mut self,
        base: &[Type],
        (actual, fewer): (usize, bool),
        spec: CallbackSpec<'_>,
        pos: Pos,
    ) -> Option<Vec<Type>> {
        let CallbackSpec {
            method,
            q_rule,
            allow_index,
        } = spec;
        // compiler.md §244 rule 1: a lambda literal with fewer parameters
        // takes the place of the smallest arity.
        if actual == base.len() || (fewer && actual < base.len()) {
            return Some(base.to_vec());
        }
        if allow_index && actual == base.len() + 1 {
            let mut indexed = base.to_vec();
            indexed.push(Type::I32);
            return Some(indexed);
        }
        if allow_index && actual == base.len() + 2 {
            let actual = format!("{method} callback with the container parameter");
            let _ = self.reject_api_form("T[]", "callback(value, index, array)", &actual, pos);
            return None;
        }
        self.reject_subset(
            RejectionSite::CallbackParameterCount,
            if allow_index {
                format!(
                    "`{method}` callbacks take {} parameter(s), or {} with a trailing \
                     `i32` index; got {actual} (Q27)",
                    base.len(),
                    base.len() + 1,
                )
            } else if q_rule == "Q24" || q_rule == "Q27" {
                format!(
                    "`{method}` callbacks take exactly {} parameter(s); \
                     extra lib callback parameters are not accepted ({q_rule})",
                    base.len(),
                )
            } else {
                format!(
                    "`{method}` callbacks take exactly {} parameter(s); got {actual} ({q_rule})",
                    base.len(),
                )
            },
            pos,
        );
        None
    }

    /// Rejects a callback value that the place takes only with an adapter
    /// it cannot have, with the cause (compiler.md §244 rule 4a, §246 rule
    /// 5): a function with a default parameter has no adapter, and only a
    /// const local or a module `const` takes one. Returns whether it gave
    /// the diagnostic. A lambda literal and a conditional get the type
    /// mismatch of the place or of their branches.
    fn reject_unadapted_callback(
        &mut self,
        checked: &hir::Expr,
        place: &Type,
        method: &str,
        fx: &FnCtx,
    ) -> bool {
        let defaulted = match &checked.kind {
            hir::ExprKind::FuncRef(symbol) => self
                .fn_sigs
                .get(symbol.full_text())
                .is_some_and(|sig| sig.params.iter().any(|p| p.has_default))
                .then(|| hir::source_name(symbol.full_text())),
            _ => None,
        };
        let fewer = match (self.function_of(&checked.ty), self.function_of(place)) {
            (Some(own), Some(place)) => own.params.len() < place.params.len(),
            _ => false,
        };
        let message = if let Some(name) = defaulted.filter(|_| fewer) {
            format!(
                "`{method}` callback `{name}` has fewer parameters than the place and a default parameter, which has no adapter; use a lambda that calls it"
            )
        } else if self.needs_adapter_name(checked, place, fx) {
            format!(
                "`{method}` {}",
                self.adapter_name_message(checked, "callback")
            )
        } else {
            return false;
        };
        self.reject_subset(
            RejectionSite::CallbackFewerParametersUnadapted,
            message,
            checked.pos.clone(),
        );
        true
    }

    /// Validates a checked callback value against the method's accepted
    /// parameter list or lists (and return type, when it is not inferred);
    /// returns the value unchanged on success and a poisoned expression
    /// after the mismatch diagnostic otherwise.
    pub(super) fn expect_callback_shape(
        &mut self,
        mut checked: hir::Expr,
        (params, ret): (&[Type], Option<&Type>),
        spec: CallbackSpec<'_>,
        fx: &FnCtx,
    ) -> hir::Expr {
        let CallbackSpec {
            method,
            allow_index,
            ..
        } = spec;
        // compiler.md §244 rule 1: a value with fewer parameters takes the
        // place of the smallest arity.
        if let Type::Func(own) = self.apparent_type(&checked.ty) {
            if own.params.len() < params.len() {
                let place = Type::func(params.to_vec(), ret.cloned().unwrap_or(own.ret));
                self.discard_function_result(&mut checked, &place, fx);
            }
        }
        // compiler.md §240 rule 2: a callback place with the result `void`.
        if let (Some(ret), Type::Func(own)) = (ret, self.apparent_type(&checked.ty)) {
            let mut place = params.to_vec();
            if allow_index && own.params.len() == params.len() + 1 {
                place.push(Type::I32);
            }
            self.discard_function_result(&mut checked, &Type::func(place, ret.clone()), fx);
        }
        let counted_callback = matches!(method, "Map.groupBy" | "Map.forEach")
            || ArrFn::ALL.iter().any(|operation| {
                operation.name() == method
                    && operation.counted_class() == hir::CountedArrayMethod::Callback
            });
        if counted_callback {
            if let Type::Func(function) = self.apparent_type(&checked.ty) {
                if !self.instance_restriction(
                    crate::check::opaque::InstanceRestriction::ArrayElementKind,
                    &function.ret,
                ) && self.apparent_type(&function.ret).counted_type().is_some()
                {
                    let mut diagnostic = crate::check::rejection::diagnostic(
                        RejectionSite::ArrayMapResult,
                        counted_method_message(
                            &self.apparent_type(&function.ret),
                            format!("`{method}` cannot carry a counted callback result (§171)"),
                        ),
                        checked.pos.clone(),
                    );
                    if method == "map"
                        && matches!(self.apparent_type(&function.ret), Type::AsyncHandle(_))
                    {
                        diagnostic.example = Some(&crate::check::diagnostic_text::ASYNC_MAP);
                    }
                    self.diags.push(diagnostic);
                    return self.err_expr(checked.pos);
                }
                if let Some(parameter) = function.params.iter().find(|parameter| {
                    !self.instance_restriction(
                        crate::check::opaque::InstanceRestriction::ArrayElementKind,
                        parameter,
                    ) && self.apparent_type(parameter).counted_type().is_some()
                }) {
                    self.reject_subset(
                        RejectionSite::ArrayElementDomain,
                        counted_method_message(
                            &self.apparent_type(parameter),
                            format!("`{method}` cannot carry a counted callback parameter (§171)"),
                        ),
                        checked.pos.clone(),
                    );
                    return self.err_expr(checked.pos);
                }
            }
        }
        let ok = match &self.apparent_type(&checked.ty) {
            Type::Error => true,
            Type::Func(ft) => {
                let indexed = allow_index
                    && ft.params.len() == params.len() + 1
                    && ft.params[..params.len()]
                        .iter()
                        .zip(params)
                        .all(|(actual, expected)| self.callback_type_fits(expected, actual))
                    && ft.params.last() == Some(&Type::I32);
                ((ft.params.len() == params.len()
                    && ft
                        .params
                        .iter()
                        .zip(params)
                        .all(|(actual, expected)| self.callback_type_fits(expected, actual)))
                    || indexed)
                    && ret.is_none_or(|r| self.callback_type_fits(&ft.ret, r))
            }
            _ => false,
        };
        if ok {
            return self.place_callback_value(checked, params, ret, fx);
        }
        if let Type::Func(ft) = &self.apparent_type(&checked.ty) {
            let accepted_arity = ft.params.len() == params.len()
                || (allow_index && ft.params.len() == params.len() + 1);
            // compiler.md §244 rule 4a: a value with fewer parameters that
            // rule 1 does not take names its cause, or gets the type
            // mismatch of the place below. Only a value with more
            // parameters than the place allows reaches the arity diagnostic.
            let mut place = params.to_vec();
            if allow_index && ft.params.len() == params.len() + 1 {
                place.push(Type::I32);
            }
            let place = Type::func(place, ret.cloned().unwrap_or(ft.ret.clone()));
            if (ft.params.len() < params.len() || accepted_arity)
                && self.reject_unadapted_callback(&checked, &place, method, fx)
            {
                return self.err_expr(checked.pos);
            }
            if ft.params.len() > params.len() && !accepted_arity {
                let pos = checked.pos.clone();
                let _ = self.callback_params_for_arity(
                    params,
                    (ft.params.len(), false),
                    spec,
                    pos.clone(),
                );
                return self.err_expr(pos);
            }
        }
        let got = self.type_name(&checked.ty);
        let wanted: Vec<String> = params.iter().map(|t| self.type_name(t)).collect();
        let wanted = if allow_index {
            format!("({}) or ({}, i32)", wanted.join(", "), wanted.join(", "))
        } else {
            format!("({})", wanted.join(", "))
        };
        let ret_n = match ret {
            Some(r) => self.type_name(r),
            None => "…".to_string(),
        };
        self.reject_subset(
            if matches!(self.apparent_type(&checked.ty), Type::Func(_)) {
                RejectionSite::CollectionCallbackTypeMismatch
            } else {
                RejectionSite::CollectionCallbackNotFunction
            },
            format!(
                "type mismatch: the `{}` callback expects `{}` => {}, got `{}`",
                method, wanted, ret_n, got
            ),
            checked.pos.clone(),
        );
        self.err_expr(checked.pos)
    }
}
