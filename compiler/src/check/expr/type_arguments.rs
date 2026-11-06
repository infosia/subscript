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
use swc_common::Spanned;

impl Checker<'_> {
    /// Checks one call with `check`. Each `ast::CallExpr` enters the checker
    /// through this function, so §230 rule 3 reaches each call: an entry of
    /// its type argument list that `check` did not resolve is reported.
    /// A call that already reported a diagnostic, or whose result `poisoned`
    /// marks, reports nothing more.
    pub(in crate::check) fn checked_call<T>(
        &mut self,
        call: &ast::CallExpr,
        poisoned: impl FnOnce(&Self, &T) -> bool,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        if let Some(arguments) = &call.type_args {
            self.type_argument_watch
                .extend(arguments.params.iter().map(|entry| entry.span()));
        }
        let diagnostics = self.diags.len();
        let checked = check(self);
        let Some(arguments) = &call.type_args else {
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
        call: &ast::CallExpr,
        arguments: &ast::TsTypeParamInstantiation,
        takes: usize,
        user: Option<bool>,
    ) {
        let member = |member: &ast::MemberExpr| match &member.prop {
            ast::MemberProp::Ident(name) => Some(name.sym.to_string()),
            _ => None,
        };
        let name = match &call.callee {
            ast::Callee::Expr(callee) => match unparen_expr(callee) {
                ast::Expr::Member(callee) => member(callee),
                ast::Expr::OptChain(chain) => match &*chain.base {
                    ast::OptChainBase::Member(callee) => member(callee),
                    ast::OptChainBase::Call(_) => None,
                },
                ast::Expr::Ident(name) => Some(name.sym.to_string()),
                _ => None,
            },
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
    pub(in crate::check) fn type_arguments_of<'a>(
        &mut self,
        call: &'a ast::CallExpr,
        count: usize,
    ) -> Option<&'a [Box<ast::TsType>]> {
        let arguments = call.type_args.as_ref()?;
        self.type_argument_takes.insert(call.span, count);
        (arguments.params.len() == count).then_some(arguments.params.as_slice())
    }

    /// Records that `call` reaches a user method (`method`) or a user
    /// function or function value. Its unread list then has the diagnostic
    /// of a named function (§231 rule 1).
    pub(in crate::check) fn mark_user_call(&mut self, call: &ast::CallExpr, method: bool) {
        if call.type_args.is_some() {
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
        call: &ast::CallExpr,
        fx: &mut FnCtx,
    ) -> (Option<Type>, Option<hir::Expr>, Option<Type>) {
        let explicit = self
            .type_arguments_of(call, 1)
            .map(|arguments| self.resolve_type(&arguments[0]))
            .filter(|ty| !matches!(&self.apparent_type(ty), Type::Error));
        let arg = &call.args[0];
        if arg.spread.is_some() {
            return (explicit, None, None); // reported by `check_arr_callback`
        }
        if let ast::Expr::Arrow(a) = &*arg.expr {
            let Some(ast::Pat::Ident(binding)) = a.params.first() else {
                return (explicit, None, None);
            };
            let Some(ann) = binding.type_ann.as_ref() else {
                return (explicit, None, None);
            };
            let ty = self.resolve_type(&ann.type_ann);
            let annotated =
                (!matches!(&self.apparent_type(&ty), Type::Error)).then_some(ty.clone());
            return (explicit.or(annotated), None, Some(ty));
        }
        let checked = self.check_expr(&arg.expr, None, fx);
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
        call: &ast::CallExpr,
        elem: &Type,
        acc: &Type,
    ) -> bool {
        if call.type_args.is_some() {
            return false;
        }
        let Some(callback) = call.args.first().filter(|arg| arg.spread.is_none()) else {
            return false;
        };
        let ast::Expr::Arrow(arrow) = unparen_expr(&callback.expr) else {
            return false;
        };
        // §231 rule 3: the result values of a block body are the
        // expressions of its `return` statements.
        let mut results = Vec::new();
        match &*arrow.body {
            ast::BlockStmtOrExpr::Expr(body) => results.push(Some(&**body)),
            ast::BlockStmtOrExpr::BlockStmt(block) => return_values(&block.stmts, &mut results),
        }
        if arrow.return_type.is_some() || arrow.is_async {
            return false;
        }
        // §230 rule 1a: an annotated accumulator beside a parameter with no
        // annotation makes the arrow context-sensitive, and `tsc` types the
        // body after the inference.
        let annotated = |pattern: &ast::Pat| match pattern {
            ast::Pat::Ident(binding) => binding.type_ann.is_some(),
            ast::Pat::Assign(assign) => {
                matches!(&*assign.left, ast::Pat::Ident(binding) if binding.type_ann.is_some())
            }
            _ => true,
        };
        if arrow.params.first().is_some_and(annotated)
            && arrow
                .params
                .iter()
                .skip(1)
                .any(|pattern| !annotated(pattern))
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
fn string_literal_result(results: &[Option<&ast::Expr>]) -> Option<String> {
    fn leaves(expr: &ast::Expr, out: &mut Vec<String>) -> Option<()> {
        match unparen_expr(expr) {
            ast::Expr::Lit(ast::Lit::Str(text)) => out.push(text.value.to_string()),
            ast::Expr::Tpl(template) if template.exprs.is_empty() => out.push(
                template
                    .quasis
                    .iter()
                    .filter_map(|quasi| quasi.cooked.as_ref())
                    .map(|cooked| cooked.to_string())
                    .collect(),
            ),
            ast::Expr::Cond(conditional) => {
                leaves(&conditional.cons, out)?;
                leaves(&conditional.alt, out)?;
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
pub(super) fn is_string_literal_result(expr: &ast::Expr) -> bool {
    match unparen_expr(expr) {
        ast::Expr::Lit(ast::Lit::Str(_)) => true,
        ast::Expr::Tpl(template) => template.exprs.is_empty(),
        ast::Expr::Cond(conditional) => {
            is_string_literal_result(&conditional.cons)
                && is_string_literal_result(&conditional.alt)
        }
        _ => false,
    }
}

/// Each `return` of a block body, in source order. A nested function has
/// its own returns, so the walk does not enter one.
fn return_values<'a>(statements: &'a [ast::Stmt], out: &mut Vec<Option<&'a ast::Expr>>) {
    for statement in statements {
        return_values_of(statement, out);
    }
}

fn return_values_of<'a>(statement: &'a ast::Stmt, out: &mut Vec<Option<&'a ast::Expr>>) {
    match statement {
        ast::Stmt::Return(ret) => out.push(ret.arg.as_deref()),
        ast::Stmt::Block(block) => return_values(&block.stmts, out),
        ast::Stmt::If(branch) => {
            return_values_of(&branch.cons, out);
            if let Some(alt) = &branch.alt {
                return_values_of(alt, out);
            }
        }
        ast::Stmt::Labeled(labeled) => return_values_of(&labeled.body, out),
        ast::Stmt::While(stmt) => return_values_of(&stmt.body, out),
        ast::Stmt::DoWhile(stmt) => return_values_of(&stmt.body, out),
        ast::Stmt::For(stmt) => return_values_of(&stmt.body, out),
        ast::Stmt::ForOf(stmt) => return_values_of(&stmt.body, out),
        ast::Stmt::ForIn(stmt) => return_values_of(&stmt.body, out),
        ast::Stmt::Switch(switch) => {
            for case in &switch.cases {
                return_values(&case.cons, out);
            }
        }
        ast::Stmt::Try(stmt) => {
            return_values(&stmt.block.stmts, out);
            if let Some(handler) = &stmt.handler {
                return_values(&handler.body.stmts, out);
            }
            if let Some(finalizer) = &stmt.finalizer {
                return_values(&finalizer.stmts, out);
            }
        }
        _ => {}
    }
}
