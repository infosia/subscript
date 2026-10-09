//! `then`, `catch`, and `finally` on a handle (compiler.md §186).
//!
//! Each call creates the handle of a compiler-supplied async helper
//! (`helper.rs`). The receiver and the callbacks are its arguments.

use swc_common::Spanned;

use super::*;
use crate::check::{rejection::RejectionSite, FnCtx};

mod helper;

/// How a helper turns the result of one callback into its own value
/// (§186 rules 1 and 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Delivery {
    /// The callback result is the value.
    Value,
    /// The callback returns nothing: the helper has no value.
    Void,
    /// The callback returns a handle: the helper awaits it (adoption).
    Adopt,
}

/// The callback position in a reaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// A fulfillment callback: it receives the value.
    Fulfilled,
    /// A rejection callback: it receives the error.
    Rejected,
    /// A `finally` callback: it receives nothing.
    Settled,
}

/// One checked callback argument.
#[derive(Debug, Clone)]
pub(super) struct Callback {
    /// The callback function type.
    pub(super) ty: Type,
    /// The callback result type.
    pub(super) result: Type,
    /// Whether the callback result carries a handle across the call.
    pub(super) transfers: bool,
    /// The number of parameters that the helper passes: zero or one.
    pub(super) arity: usize,
    /// How the helper delivers the callback result.
    pub(super) delivery: Delivery,
    /// The value that the callback delivers: `void` for [`Delivery::Void`].
    pub(super) value: Type,
    /// The callback argument position: the helper calls it there.
    pub(super) pos: Pos,
}

/// The four helper forms (§186 rule 2).
#[derive(Debug, Clone)]
pub(super) enum Reaction {
    /// `h.then(f)`.
    Then(Callback),
    /// `h.then(f, r)`.
    ThenBoth(Callback, Callback),
    /// `h.catch(r)`.
    Catch(Callback),
    /// `h.finally(f)`.
    Finally(Callback),
}

impl<'p> Checker<'p> {
    /// Checks `recv.method(...)` where `recv` is a `Promise<T>` and
    /// `method` is `then`, `catch`, or `finally` (§186 rule 5).
    pub(in crate::check) fn check_promise_reaction(
        &mut self,
        recv: hir::Expr,
        property: &ast::IdentName,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let Type::AsyncHandle(value) = self.apparent_type(&recv.ty) else {
            return self.err_expr(pos);
        };
        let value = *value;
        if self.apparent_type(&value) == Type::Error {
            return self.err_expr(pos);
        }
        let method = property.sym.as_ref();
        let method_pos = self.pos(property.span);
        let receiver_origins = self.expr_async_origins(&recv, fx);
        fx.handle_async_origins(&receiver_origins);
        let Some((reaction, callbacks)) =
            self.check_reaction_arguments(&value, method, call, fx, &method_pos)
        else {
            return self.err_expr(pos);
        };
        let result = match &reaction {
            Reaction::Then(f) | Reaction::ThenBoth(f, _) => f.value.clone(),
            Reaction::Catch(_) | Reaction::Finally(_) => value.clone(),
        };
        let symbol = self.promise_helper(&reaction, &value, &result, &recv.pos, &method_pos, &pos);
        let mut args = vec![recv];
        args.extend(callbacks);
        let origin = fx.register_async_origin(pos.clone());
        hir::Expr {
            pending_work: None,
            kind: ExprKind::AsyncHandleCreate {
                callee: hir::AsyncCallee::Function(hir::Symbol::from_full_text(symbol)),
                args,
                origin,
            },
            ty: Type::async_handle(result),
            pos,
        }
    }

    /// Checks the argument list and each callback. Returns `None` after a
    /// diagnostic.
    fn check_reaction_arguments(
        &mut self,
        value: &Type,
        method: &str,
        call: &ast::CallExpr,
        fx: &mut FnCtx,
        method_pos: &Pos,
    ) -> Option<(Reaction, Vec<hir::Expr>)> {
        if call.type_args.is_some() {
            self.reject_combinator(
                method,
                format!("`.{method}(...)` takes no type arguments; the callback result gives the value type"),
                method_pos.clone(),
            );
            return None;
        }
        if let Some(spread) = call.args.iter().find_map(|argument| argument.spread) {
            self.reject_subset(
                RejectionSite::PromiseReactionSpread,
                "spread arguments require variadic parameters, which the language does not have",
                self.pos(spread),
            );
            return None;
        }
        let most = if method == "then" { 2 } else { 1 };
        if call.args.is_empty() {
            self.reject_combinator(
                method,
                format!("`.{method}(...)` requires a callback"),
                method_pos.clone(),
            );
            return None;
        }
        if call.args.len() > most {
            self.reject_subset(
                RejectionSite::PromiseReactionArguments,
                if method == "then" {
                    "`.then(...)` takes a callback and an optional rejection callback".to_string()
                } else {
                    format!("`.{method}(...)` takes one callback")
                },
                self.pos(call.args[most].expr.span()),
            );
            return None;
        }
        if let Some(argument) = call.args.iter().find(|argument| {
            matches!(
                unparen_expr(&argument.expr),
                ast::Expr::Lit(ast::Lit::Null(_))
            )
        }) {
            self.reject_combinator(
                method,
                format!("`.{method}(...)` takes no `null` callback; `.catch(r)` takes a rejection callback alone"),
                self.pos(argument.expr.span()),
            );
            return None;
        }
        match method {
            "then" => {
                let (f, f_expr) = self.check_reaction_callback(
                    &call.args[0],
                    Role::Fulfilled,
                    value,
                    None,
                    method,
                    fx,
                )?;
                let Some(argument) = call.args.get(1) else {
                    return Some((Reaction::Then(f), vec![f_expr]));
                };
                let (r, r_expr) = self.check_reaction_callback(
                    argument,
                    Role::Rejected,
                    value,
                    Some(&f.value),
                    method,
                    fx,
                )?;
                if self.apparent_type(&r.value) != self.apparent_type(&f.value) {
                    let expected = self.type_name(&f.value);
                    let actual = self.type_name(&r.value);
                    self.reject_subset(
                        RejectionSite::PromiseReactionResult,
                        format!("a rejection callback of `.then(...)` must return `{expected}`, the value of its fulfillment callback; it returns `{actual}`"),
                        self.pos(argument.expr.span()),
                    );
                    return None;
                }
                Some((Reaction::ThenBoth(f, r), vec![f_expr, r_expr]))
            }
            "catch" => {
                let (r, r_expr) = self.check_reaction_callback(
                    &call.args[0],
                    Role::Rejected,
                    value,
                    Some(value),
                    method,
                    fx,
                )?;
                if self.apparent_type(&r.value) != self.apparent_type(value) {
                    let expected = self.type_name(value);
                    let message = if r.delivery == Delivery::Void {
                        format!("a `catch` callback must return `{expected}`; a `void` callback is accepted only on `Promise<void>`")
                    } else {
                        let actual = self.type_name(&r.value);
                        format!("a `catch` callback must return `{expected}`, the value type of the handle; it returns `{actual}`")
                    };
                    self.reject_subset(
                        RejectionSite::PromiseReactionResult,
                        message,
                        self.pos(call.args[0].expr.span()),
                    );
                    return None;
                }
                Some((Reaction::Catch(r), vec![r_expr]))
            }
            _ => {
                let (f, f_expr) = self.check_reaction_callback(
                    &call.args[0],
                    Role::Settled,
                    value,
                    None,
                    method,
                    fx,
                )?;
                if f.delivery != Delivery::Void {
                    let actual = self.type_name(&f.ty);
                    self.reject_subset(
                        RejectionSite::PromiseReactionResult,
                        format!("a `finally` callback must be `() => void`; it is `{actual}`"),
                        self.pos(call.args[0].expr.span()),
                    );
                    return None;
                }
                Some((Reaction::Finally(f), vec![f_expr]))
            }
        }
    }

    /// Checks one callback against the parameters that its helper passes.
    ///
    /// The context parameters are the parameters of the TypeScript
    /// callback type: `T` for a fulfillment callback (also `void`), the
    /// rejection reason for a rejection callback, and none for `finally`.
    /// The helper passes the same parameters, except none for a `void`
    /// value and `Error` for the reason.
    fn check_reaction_callback(
        &mut self,
        argument: &ast::ExprOrSpread,
        role: Role,
        value: &Type,
        hint: Option<&Type>,
        method: &str,
        fx: &mut FnCtx,
    ) -> Option<(Callback, hir::Expr)> {
        let pos = self.pos(argument.expr.span());
        let error = Type::Class(self.error_class);
        let void_value = self.apparent_type(value) == Type::Void;
        let (context, params) = match role {
            Role::Fulfilled if void_value => (vec![Type::Void], Vec::new()),
            Role::Fulfilled => (vec![value.clone()], vec![value.clone()]),
            Role::Rejected => (vec![error.clone()], vec![error]),
            Role::Settled => (Vec::new(), Vec::new()),
        };
        let mut checked = if let ast::Expr::Arrow(arrow) = unparen_expr(&argument.expr) {
            // A parameter with no default and no context is a TypeScript
            // arity error: report it before its missing annotation.
            let required = arrow
                .params
                .iter()
                .rposition(|parameter| match parameter {
                    ast::Pat::Assign(_) => false,
                    ast::Pat::Ident(binding) => !binding.id.optional,
                    _ => true,
                })
                .map_or(0, |index| index + 1);
            if required > context.len() {
                self.reject_reaction_arity(role, method, &context, &params, pos);
                return None;
            }
            self.lambda_result_hint = hint.cloned();
            let checked =
                self.check_lambda_with(arrow, Some(&context), None, None, fx, pos.clone());
            self.lambda_result_hint = None;
            checked
        } else {
            self.check_expr(&argument.expr, None, fx)
        };
        let Type::Func(signature) = self.apparent_type(&checked.ty) else {
            if self.apparent_type(&checked.ty) != Type::Error {
                self.reject_subset(
                    RejectionSite::PromiseReactionArguments,
                    format!("a `.{method}(...)` callback must be a function"),
                    pos,
                );
            }
            return None;
        };
        if self.apparent_type(&signature.ret) == Type::Error
            || signature
                .params
                .iter()
                .any(|parameter| self.apparent_type(parameter) == Type::Error)
        {
            return None;
        }
        let fits = signature.params.len() <= params.len()
            && signature
                .params
                .iter()
                .zip(&params)
                .all(|(actual, expected)| {
                    self.apparent_type(actual) == self.apparent_type(expected)
                });
        if !fits {
            let required = self
                .function_value_required(&checked, fx)
                .unwrap_or(signature.params.len());
            self.reject_reaction_parameters(
                role,
                method,
                (&signature.params, required),
                &context,
                &params,
                pos,
            );
            return None;
        }
        let (delivery, value) = match self.apparent_type(&signature.ret) {
            Type::Void => (Delivery::Void, Type::Void),
            Type::AsyncHandle(value) => (Delivery::Adopt, *value),
            _ => (Delivery::Value, signature.ret.clone()),
        };
        if let ExprKind::Lambda {
            is_async: false,
            captures,
            owns_environment,
            ..
        } = &mut checked.kind
        {
            // §186 rule 4: a direct synchronous callback owns its `const`
            // captures. A capture of `this` stays rejected: the accepted
            // form copies the field into a `const`.
            if captures.iter().any(|capture| capture.name == "this") {
                self.diags
                    .push(crate::check::rejection::reaction_receiver_diagnostic(
                        method, pos,
                    ));
                return None;
            }
            if !captures.is_empty() {
                *owns_environment = true;
            }
        }
        let callback = Callback {
            ty: checked.ty.clone(),
            result: signature.ret.clone(),
            transfers: self.apparent_type(&signature.ret).carries_async_handle(),
            arity: signature.params.len(),
            delivery,
            value,
            pos,
        };
        Some((callback, checked))
    }

    /// Reports a callback whose parameters do not match the parameters
    /// that the helper passes. The site follows the `tsc` result: `tsc`
    /// accepts the callback when it requires at most the context
    /// parameters, and when each context type is assignable to its
    /// parameter type. The rejection reason is `any` in `tsc`. `actual`
    /// holds the callback parameters and the number that `tsc` requires.
    fn reject_reaction_parameters(
        &mut self,
        role: Role,
        method: &str,
        (actual, required): (&[Type], usize),
        context: &[Type],
        params: &[Type],
        pos: Pos,
    ) {
        let tsc_accepts = required <= context.len()
            && actual.iter().zip(context).all(|(actual, delivered)| {
                role == Role::Rejected || self.ts_nominal_assignable(delivered, actual)
            });
        if !tsc_accepts {
            if actual.len() > params.len() {
                self.reject_reaction_arity(role, method, context, params, pos);
            } else {
                let message = self.reaction_parameter_type_message(role, method, actual, params);
                self.reject_subset(RejectionSite::PromiseReactionArguments, message, pos);
            }
            return;
        }
        if params.is_empty() && !context.is_empty() && !actual.is_empty() {
            self.reject_subset(
                RejectionSite::PromiseVoidReactionParameter,
                "a `.then(...)` callback on `Promise<void>` takes no parameter; the language has no `void` parameter type",
                pos,
            );
        } else if actual.len() > params.len() {
            let message = format!(
                "{}; a function type has no optional parameter",
                self.reaction_arity_message(role, method, params)
            );
            self.reject_subset(RejectionSite::PromiseReactionParameter, message, pos);
        } else {
            let message = self.reaction_parameter_type_message(role, method, actual, params);
            self.reject_subset(RejectionSite::PromiseReactionParameter, message, pos);
        }
    }

    /// Reports a callback that requires more parameters than `tsc` passes.
    fn reject_reaction_arity(
        &mut self,
        role: Role,
        method: &str,
        context: &[Type],
        params: &[Type],
        pos: Pos,
    ) {
        let message = if params.is_empty() && !context.is_empty() {
            "a `.then(...)` callback on `Promise<void>` takes no parameter".to_string()
        } else {
            self.reaction_arity_message(role, method, params)
        };
        self.reject_subset(RejectionSite::PromiseReactionArguments, message, pos);
    }

    fn reaction_arity_message(&self, role: Role, method: &str, params: &[Type]) -> String {
        match (role, params.first()) {
            (Role::Rejected, _) => format!(
                "a rejection callback of `.{method}(...)` takes at most one parameter, of type `Error`"
            ),
            (_, Some(ty)) => format!(
                "a `.{method}(...)` callback takes at most one parameter, of type `{}`",
                self.type_name(ty)
            ),
            (Role::Fulfilled, None) => {
                "a `.then(...)` callback on `Promise<void>` takes no parameter".to_string()
            }
            (_, None) => format!("a `.{method}(...)` callback takes no parameter"),
        }
    }

    fn reaction_parameter_type_message(
        &self,
        role: Role,
        method: &str,
        actual: &[Type],
        params: &[Type],
    ) -> String {
        let (actual, expected) = actual
            .iter()
            .zip(params)
            .find(|(actual, expected)| self.apparent_type(actual) != self.apparent_type(expected))
            .map(|(actual, expected)| (self.type_name(actual), self.type_name(expected)))
            .unwrap_or_default();
        if role == Role::Rejected {
            format!("a rejection callback parameter of `.{method}(...)` must be `{expected}`; it is `{actual}`")
        } else {
            format!("a `.{method}(...)` callback parameter must be `{expected}`, the value type of the handle; it is `{actual}`")
        }
    }

    /// Reports a form that TypeScript accepts and the language does not
    /// (site `PromiseCombinatorCall`).
    fn reject_combinator(&mut self, method: &str, message: impl Into<String>, pos: Pos) {
        let mut diagnostic =
            crate::check::rejection::diagnostic(RejectionSite::PromiseCombinatorCall, message, pos);
        diagnostic.example = match method {
            "then" => Some(&crate::check::diagnostic_text::THEN),
            "catch" => Some(&crate::check::diagnostic_text::CATCH),
            _ => Some(&crate::check::diagnostic_text::FINALLY),
        };
        self.diags.push(diagnostic);
    }
}

impl Reaction {
    /// The callbacks, in helper parameter order.
    pub(super) fn callbacks(&self) -> Vec<&Callback> {
        match self {
            Self::Then(f) | Self::Catch(f) | Self::Finally(f) => vec![f],
            Self::ThenBoth(f, r) => vec![f, r],
        }
    }

    /// The helper name: the form, each callback shape, and the types.
    fn describe(&self) -> String {
        let shape = |callback: &Callback, name: &str| {
            let parameter = if callback.arity == 0 {
                "()".to_string()
            } else {
                format!("({name})")
            };
            let result = match callback.delivery {
                Delivery::Value => "value",
                Delivery::Void => "void",
                Delivery::Adopt => "handle",
            };
            format!("{parameter} => {result}")
        };
        match self {
            Self::Then(f) => format!("then {}", shape(f, "v")),
            Self::ThenBoth(f, r) => format!("then {}, {}", shape(f, "v"), shape(r, "e")),
            Self::Catch(r) => format!("catch {}", shape(r, "e")),
            Self::Finally(f) => format!("finally {}", shape(f, "v")),
        }
    }
}
