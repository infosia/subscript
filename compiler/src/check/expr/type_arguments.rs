//! The type argument list of a call (compiler.md §230 rule 3) and the
//! result of a `reduce` callback (§230 rule 1).
//!
//! A call accepts a type argument list only if the checker reads each
//! entry of it (§231 rule 1). The checker reads an entry when it resolves
//! it, so `resolve_type` removes each entry from the watch set, and the
//! call reports each entry that stays.

use super::*;
use crate::check::rejection::RejectionSite;
use crate::check::FnCtx;
use crate::hir::ArrFn;
use oxc_span::GetSpan;

impl<'p> Checker<'p> {
    /// Checks one call with `check`. Each `ast::CallExpression` enters the checker
    /// through this function, so §230 rule 3 reaches each call: an entry of
    /// its type argument list that `check` did not resolve is reported.
    /// A call that already reported a diagnostic, or whose result `poisoned`
    /// marks, reports nothing more.
    pub(in crate::check) fn checked_call<T>(
        &mut self,
        call: &ast::CallExpression,
        poisoned: impl FnOnce(&Self, &T) -> bool,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        if let Some(arguments) = &call.type_arguments {
            self.type_argument_watch
                .extend(arguments.params.iter().map(|entry| entry.span()));
        }
        let diagnostics = self.diags.len();
        let checked = check(self);
        let Some(arguments) = &call.type_arguments else {
            return checked;
        };
        let mut unread = false;
        for entry in &arguments.params {
            unread |= self.type_argument_watch.remove(&entry.span());
        }
        let takes = self.type_argument_takes.remove(&call.span).unwrap_or(0);
        let user = self.type_argument_user_calls.remove(&call.span);
        if unread && self.diags.len() == diagnostics && !poisoned(self, &checked) {
            self.reject_unread_type_arguments(call, arguments, takes, user);
        }
        checked
    }

    /// The poison test of [`Self::checked_call`] for a checked expression.
    pub(in crate::check) fn poisoned_call(&self, checked: &hir::Expr) -> bool {
        matches!(self.apparent_type(&checked.ty), Type::Error)
    }

    fn reject_unread_type_arguments(
        &mut self,
        call: &ast::CallExpression,
        arguments: &ast::TSTypeParameterInstantiation,
        takes: usize,
        user: Option<bool>,
    ) {
        let name = match unparen_expr(&call.callee) {
            ast::Expression::StaticMemberExpression(member) => {
                Some(member.property.name.to_string())
            }
            ast::Expression::ChainExpression(chain) => match &chain.expression {
                ast::ChainElement::StaticMemberExpression(member) => {
                    Some(member.property.name.to_string())
                }
                _ => None,
            },
            ast::Expression::Identifier(name) => Some(name.name.to_string()),
            _ => None,
        };
        // A callee with no name is not an identifier, so the text names it
        // with no backticks.
        let name = name.map_or_else(|| "the callee".to_string(), |name| format!("`{name}`"));
        let got = arguments.params.len();
        let pos = self.pos(arguments.span);
        // §231 rule 1: a user call has the diagnostic of a named function.
        // A user function or method reads its list when it is generic, so
        // an unread list is the list of a callee that is not generic.
        if let Some(method) = user {
            let message = if method {
                format!("method {name} is not generic")
            } else {
                format!("{name} is not generic")
            };
            self.reject_subset(RejectionSite::NonGenericFunctionTypeArguments, message, pos);
            return;
        }
        let takes = match takes {
            0 => "no type arguments".to_string(),
            1 => "1 type argument".to_string(),
            count => format!("{count} type arguments"),
        };
        self.reject_subset(
            RejectionSite::BuiltinTypeArgumentCount,
            format!("{name} takes {takes}, got {got}"),
            pos,
        );
    }

    /// The entries of the type argument list of `call` if it has `count`
    /// entries. A list of another count stays unread, and the diagnostic of
    /// §230 rule 3 names `count`.
    pub(in crate::check) fn type_arguments_of<'a, 'b>(
        &mut self,
        call: &'a ast::CallExpression<'b>,
        count: usize,
    ) -> Option<&'a [ast::TSType<'b>]> {
        let arguments = call.type_arguments.as_ref()?;
        self.type_argument_takes.insert(call.span, count);
        (arguments.params.len() == count).then_some(arguments.params.as_slice())
    }

    /// Records that `call` reaches a user method (`method`) or a user
    /// function or function value. Its unread list then has the diagnostic
    /// of a named function (§231 rule 1).
    pub(in crate::check) fn mark_user_call(&mut self, call: &ast::CallExpression, method: bool) {
        if call.type_arguments.is_some() {
            self.type_argument_user_calls.insert(call.span, method);
        }
    }

    /// The accumulator type that the callback or the type argument of
    /// `reduce<U>` spells, the callback when it is a function value, and
    /// the resolved type of an annotated `acc` parameter. A type argument
    /// comes first (§230 rule 3). Only an un-annotated arrow with no type
    /// argument leaves `U` to `init` itself (C4).
    pub(super) fn reduce_acc_context(
        &mut self,
        call: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
    ) -> (Option<Type>, Option<hir::Expr>, Option<Type>) {
        let explicit = self
            .type_arguments_of(call, 1)
            .map(|arguments| self.resolve_type(&arguments[0]))
            .filter(|ty| !matches!(&self.apparent_type(ty), Type::Error));
        let arg = &call.arguments[0];
        if matches!(arg, ast::Argument::SpreadElement(_)) {
            return (explicit, None, None); // reported by `check_arr_callback`
        }
        if let ast::Expression::ArrowFunctionExpression(a) = crate::check::argument_expression(arg)
        {
            let Some(parameter) = a.params.items.first() else {
                return (explicit, None, None);
            };
            let ast::BindingPattern::BindingIdentifier(_) = &parameter.pattern else {
                return (explicit, None, None);
            };
            let Some(ann) = parameter.type_annotation.as_ref() else {
                return (explicit, None, None);
            };
            let ty = self.resolve_type(&ann.type_annotation);
            let annotated =
                (!matches!(&self.apparent_type(&ty), Type::Error)).then_some(ty.clone());
            return (explicit.or(annotated), None, Some(ty));
        }
        let checked = self.check_expr(crate::check::argument_expression(arg), None, fx);
        let acc = match &self.apparent_type(&checked.ty) {
            Type::Func(ft) => ft.params.first().cloned(),
            _ => None,
        };
        (explicit.or(acc), Some(checked), None)
    }

    /// §230 rule 1: with an initial value and no type argument, `tsc` types
    /// the result of a `reduce` callback with no result annotation with the
    /// element type as its context. The result values are the expression
    /// body, or the `return` expressions of a block body (§231 rule 3). A
    /// string literal result then widens to `string` unless the element
    /// type has a string literal type. Rejects the call if `string` is not
    /// assignable to `acc`.
    /// Only a string literal widens here: the language has no other
    /// literal type.
    pub(super) fn reject_widened_reduce_result(
        &mut self,
        method: ArrFn,
        call: &ast::CallExpression,
        elem: &Type,
        acc: &Type,
    ) -> bool {
        if call.type_arguments.is_some() {
            return false;
        }
        let Some(callback) = call
            .arguments
            .first()
            .filter(|arg| !matches!(arg, ast::Argument::SpreadElement(_)))
        else {
            return false;
        };
        let ast::Expression::ArrowFunctionExpression(arrow) =
            unparen_expr(crate::check::argument_expression(callback))
        else {
            return false;
        };
        // §231 rule 3: the result values of a block body are the
        // expressions of its `return` statements.
        let mut results = Vec::new();
        match &arrow.body {
            ast::ArrowFunctionBody::FunctionBody(block) => {
                return_values(&block.statements, &mut results)
            }
            body => {
                if let Some(expression) = body.as_expression() {
                    results.push(Some(expression));
                }
            }
        }
        if arrow.return_type.is_some() || arrow.r#async {
            return false;
        }
        let annotated = |parameter: &ast::FormalParameter<'_>| match &parameter.pattern {
            ast::BindingPattern::BindingIdentifier(_) => parameter.type_annotation.is_some(),
            _ => true,
        };
        if arrow.params.items.first().is_some_and(annotated)
            && (arrow
                .params
                .items
                .iter()
                .skip(1)
                .any(|parameter| !annotated(parameter)))
        {
            return false;
        }
        let Some(literal) = string_literal_result(&results) else {
            return false;
        };
        let Some(Some(first)) = results.first() else {
            return false;
        };
        let result_pos = self.pos(first.span());
        if self.has_string_literal_type(elem) || self.assignable(&Type::Str, acc) {
            return false;
        }
        let name = method.name();
        let acc_name = self.type_name(acc);
        self.reject_subset(
            RejectionSite::ReduceLiteralResultWidens,
            format!(
                "the `{name}` callback result `\"{literal}\"` has the type `string`, \
                 and `string` is not assignable to the accumulator `{acc_name}`; \
                 `tsc` rejects the call (TS2769)\n\
                 note: write `{name}<{acc_name}>(...)`, or annotate the callback result: \
                 `(...): {acc_name} => ...`"
            ),
            result_pos,
        );
        true
    }

    fn has_string_literal_type(&self, ty: &Type) -> bool {
        match self.apparent_type(ty) {
            Type::StringAlias(_) => true,
            Type::Nullable(inner, _) => self.has_string_literal_type(&inner),
            Type::GenericUnion(members) => members
                .iter()
                .any(|member| self.has_string_literal_type(member)),
            _ => false,
        }
    }
}

/// The string literal of a result whose every value is the same string
/// literal, through parentheses and conditional branches. `tsc` gives such
/// a result one fresh literal type, and that type widens. Two different
/// literals give a union, and `tsc` keeps a union of literals. A `return`
/// with no value has no literal.
fn string_literal_result(results: &[Option<&ast::Expression>]) -> Option<String> {
    fn leaves(expr: &ast::Expression, out: &mut Vec<String>) -> Option<()> {
        match unparen_expr(expr) {
            ast::Expression::StringLiteral(text) => out.push(text.value.to_string()),
            ast::Expression::TemplateLiteral(template) if template.expressions.is_empty() => out
                .push(
                    template
                        .quasis
                        .iter()
                        .filter_map(|quasi| quasi.value.cooked.as_ref())
                        .map(|cooked| cooked.to_string())
                        .collect(),
                ),
            ast::Expression::ConditionalExpression(conditional) => {
                leaves(&conditional.consequent, out)?;
                leaves(&conditional.alternate, out)?;
            }
            _ => return None,
        }
        Some(())
    }
    let mut out = Vec::new();
    for result in results {
        leaves((*result)?, &mut out)?;
    }
    let first = out.first()?.clone();
    out.iter().all(|value| *value == first).then_some(first)
}

/// Whether each branch of `expr` is a string literal, through parentheses
/// and conditional branches (§233 rule 1a).
pub(super) fn is_string_literal_result(expr: &ast::Expression) -> bool {
    match unparen_expr(expr) {
        ast::Expression::StringLiteral(_) => true,
        ast::Expression::TemplateLiteral(template) => template.expressions.is_empty(),
        ast::Expression::ConditionalExpression(conditional) => {
            is_string_literal_result(&conditional.consequent)
                && is_string_literal_result(&conditional.alternate)
        }
        _ => false,
    }
}

/// Each `return` of a block body, in source order. A nested function has
/// its own returns, so the walk does not enter one.
fn return_values<'a, 'p>(
    statements: &'a [ast::Statement<'p>],
    out: &mut Vec<Option<&'a ast::Expression<'p>>>,
) {
    for statement in statements {
        return_values_of(statement, out);
    }
}

fn return_values_of<'a, 'p>(
    statement: &'a ast::Statement<'p>,
    out: &mut Vec<Option<&'a ast::Expression<'p>>>,
) {
    match statement {
        ast::Statement::ReturnStatement(ret) => out.push(ret.argument.as_ref()),
        ast::Statement::BlockStatement(block) => return_values(&block.body, out),
        ast::Statement::IfStatement(branch) => {
            return_values_of(&branch.consequent, out);
            if let Some(alt) = &branch.alternate {
                return_values_of(alt, out);
            }
        }
        ast::Statement::LabeledStatement(labeled) => return_values_of(&labeled.body, out),
        ast::Statement::WhileStatement(stmt) => return_values_of(&stmt.body, out),
        ast::Statement::DoWhileStatement(stmt) => return_values_of(&stmt.body, out),
        ast::Statement::ForStatement(stmt) => return_values_of(&stmt.body, out),
        ast::Statement::ForOfStatement(stmt) => return_values_of(&stmt.body, out),
        ast::Statement::ForInStatement(stmt) => return_values_of(&stmt.body, out),
        ast::Statement::SwitchStatement(switch) => {
            for case in &switch.cases {
                return_values(&case.consequent, out);
            }
        }
        ast::Statement::TryStatement(stmt) => {
            return_values(&stmt.block.body, out);
            if let Some(handler) = &stmt.handler {
                return_values(&handler.body.body, out);
            }
            if let Some(finalizer) = &stmt.finalizer {
                return_values(&finalizer.body, out);
            }
        }
        _ => {}
    }
}
