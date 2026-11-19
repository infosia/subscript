//! Checks the expression entry points, statement expressions, and `await`.

use crate::check::rejection::RejectionSite;

use oxc_ast::ast;
use oxc_span::GetSpan;

use crate::check::{AsyncOrigins, Checker, FnCtx, ScopeItem};
use crate::diag::Pos;
use crate::hir::{self, AsyncCallee, ExprKind};
use crate::types::{ClassId, Type};

use super::contextual_object_class;

impl<'p> Checker<'p> {
    pub(in crate::check) fn check_truth_expr(
        &mut self,
        expression: &ast::Expression,
        fx: &mut FnCtx<'p>,
    ) -> hir::Expr {
        let saved_operand = std::mem::replace(&mut self.inferring_result_operand, false);
        let checked = match super::unparen_expr(expression) {
            expression if expression.as_member_expression().is_some() => {
                match expression.as_member_expression() {
                    Some(member) => self.check_member_read_inner(member, fx, false, true),
                    None => self.err_expr(self.pos(expression.span())),
                }
            }
            expression => self.check_expr(expression, None, fx),
        };
        self.inferring_result_operand = saved_operand;
        checked
    }

    pub(in crate::check) fn ambient_visible(&self, name: &str, fx: &FnCtx<'p>) -> bool {
        !fx.owns_local_name(name) && self.peek_scope_item(name).is_none()
    }

    pub(in crate::check) fn ambient_namespace(
        &self,
        obj: &ast::Expression,
        fx: &FnCtx<'p>,
    ) -> Option<&'static str> {
        let ast::Expression::Identifier(id) = obj else {
            return None;
        };
        let name = match id.name.as_ref() {
            "Array" => "Array",
            "Context" => "Context",
            "Math" => "Math",
            "Number" => "Number",
            "JSON" => "JSON",
            "Date" => "Date",
            "Map" => "Map",
            "Set" => "Set",
            "RegExp" => "RegExp",
            "Worker" => "Worker",
            "Promise" => "Promise",
            "Object" => "Object",
            _ => return None,
        };
        self.ambient_visible(name, fx).then_some(name)
    }

    /// Returns the must-await origins carried through one checked value.
    pub(crate) fn expr_async_origins(&self, expr: &hir::Expr, fx: &FnCtx<'p>) -> AsyncOrigins {
        use hir::ExprKind as K;
        match &expr.kind {
            K::TaskGroup {
                origin: Some(origin),
                ..
            }
            | K::AsyncAll { origin, .. }
            | K::AsyncHandleCreate { origin, .. } => AsyncOrigins::made(*origin),
            K::AsyncHandleTransfer { origin, .. } => AsyncOrigins::made(*origin),
            K::Local(name, _, _) => fx.local_async_origins(name),
            K::ArrayLit(elements) => {
                elements
                    .iter()
                    .fold(AsyncOrigins::default(), |mut origins, element| {
                        origins.extend(self.expr_async_origins(element, fx));
                        origins
                    })
            }
            K::ArraySpreadLit(elements) => {
                elements
                    .iter()
                    .fold(AsyncOrigins::default(), |mut origins, element| {
                        origins.extend(self.expr_async_origins(&element.expr, fx));
                        origins
                    })
            }
            K::Index { obj, .. } | K::Field { obj, .. } | K::Cast(obj) => {
                self.expr_async_origins(obj, fx)
            }
            K::Cond { then, els, .. } => AsyncOrigins::conditional(
                self.expr_async_origins(then, fx),
                self.expr_async_origins(els, fx),
            ),
            K::Binary {
                op: hir::BinOp::And | hir::BinOp::Or,
                left,
                right,
            } => AsyncOrigins::conditional(
                self.expr_async_origins(left, fx),
                self.expr_async_origins(right, fx),
            ),
            K::Seq { value, .. } => self.expr_async_origins(value, fx),
            K::Call {
                callee: hir::Callee::Arr(_),
                args,
            } if self.apparent_type(&expr.ty).holds_async_handle() => {
                args.first().map_or_else(AsyncOrigins::default, |recv| {
                    self.expr_async_origins(recv, fx)
                })
            }
            K::Call {
                callee: hir::Callee::Method { recv, .. },
                ..
            } if self.apparent_type(&expr.ty).holds_async_handle() => {
                if matches!(
                    self.apparent_type(&recv.ty),
                    Type::Array(_) | Type::Str | Type::Generator(_)
                ) {
                    self.expr_async_origins(recv, fx)
                } else {
                    AsyncOrigins::default()
                }
            }
            _ => AsyncOrigins::default(),
        }
    }

    /// True when a value of the type discharges its origins at an argument
    /// or a `return`: the type holds a handle or is a dynamic array
    /// (`compiler.md` §188.1 rule 3).
    pub(crate) fn transfers_async_origins(&self, ty: &Type) -> bool {
        let ty = self.apparent_type(ty);
        ty.holds_async_handle() || matches!(ty, Type::Array(_))
    }

    /// Makes a value whose type holds a handle an origin
    /// (`compiler.md` §188.1 rule 2).
    pub(crate) fn track_async_call_result(
        &mut self,
        value: hir::Expr,
        fx: &mut FnCtx<'p>,
    ) -> hir::Expr {
        if !self.apparent_type(&value.ty).holds_async_handle() {
            return value;
        }
        let pos = value.pos.clone();
        let ty = value.ty.clone();
        let origin = fx.register_async_origin(pos.clone());
        hir::Expr {
            pending_work: None,
            kind: ExprKind::AsyncHandleTransfer {
                value: Box::new(value),
                origin,
            },
            ty,
            pos,
        }
    }

    /// Emits a checker-owned generated-reference rejection.
    pub(super) fn reject_api_form(
        &mut self,
        group: &str,
        surface: &str,
        actual: &str,
        pos: Pos,
    ) -> bool {
        let Some(rejection) = crate::ambient::form_rejection(group, surface) else {
            return false;
        };
        self.emit_api_rejection(rejection, actual, pos);
        true
    }

    pub(in crate::check) fn emit_api_rejection(
        &mut self,
        rejection: crate::ambient::ApiRejection,
        actual: &str,
        pos: Pos,
    ) {
        let message = crate::ambient::rejection_message(rejection, actual);
        self.reject_subset(rejection.site, message, pos);
    }

    pub(crate) fn err_expr(&self, pos: Pos) -> hir::Expr {
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Null,
            ty: Type::Error,
            pos,
        }
    }

    /// Checks one expression. `ctx` is the contextual type used to type
    /// suffix-less numeric literals (C4); it never coerces non-literals.
    pub(crate) fn check_expr(
        &mut self,
        e: &ast::Expression,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
    ) -> hir::Expr {
        self.check_expr_with_header_receiver(e, ctx, fx, false)
    }

    /// Checks one expression with an expected type of the kind `kind`. A
    /// hint types a literal, and gives no parameter types to a lambda
    /// (compiler.md §249 rule 1).
    pub(crate) fn check_expr_as(
        &mut self,
        e: &ast::Expression,
        ctx: Option<&Type>,
        kind: ContextKind,
        fx: &mut FnCtx<'p>,
    ) -> hir::Expr {
        self.check_expr_with_policy(e, (ctx, kind), fx, false, false, false)
    }

    pub(super) fn check_expr_with_header_receiver(
        &mut self,
        e: &ast::Expression,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
        allow_embedded_header_receiver: bool,
    ) -> hir::Expr {
        self.check_expr_with_policy(
            e,
            (ctx, ContextKind::Contextual),
            fx,
            allow_embedded_header_receiver,
            false,
            false,
        )
    }

    /// compiler.md §290 rule 1: an initializer inherits the dependency of
    /// its callee read, and an awaited call still reads its callee's result.
    pub(in crate::check) fn check_expr_with_result_dependency(
        &mut self,
        e: &ast::Expression,
        fx: &mut FnCtx<'p>,
    ) -> hir::Expr {
        self.check_expr_with_policy(e, (None, ContextKind::Contextual), fx, false, false, true)
    }

    /// Checks the expression body of a lambda with no result type: a body
    /// of type `void` gives the result `void` (compiler.md §238 rule 1), so
    /// the expression keeps its `void` type. A conditional body whose
    /// leaves are each `void` is `void` (§258 rule 3).
    pub(crate) fn check_expr_keeping_void(
        &mut self,
        e: &ast::Expression,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
    ) -> hir::Expr {
        let e = self.enter_void_leaf_place(e, super::VoidLeafPlace::VoidResult);
        let checked = self.check_expr_keeping_void_as(e, ctx, ContextKind::Contextual, fx);
        self.void_leaf_place = None;
        checked
    }

    /// Checks one expression that keeps a `void` type.
    pub(super) fn check_expr_keeping_void_as(
        &mut self,
        e: &ast::Expression,
        ctx: Option<&Type>,
        kind: ContextKind,
        fx: &mut FnCtx<'p>,
    ) -> hir::Expr {
        let result_operand = matches!(
            super::unparen_expr(e),
            ast::Expression::CallExpression(_)
                | ast::Expression::AwaitExpression(_)
                | ast::Expression::ConditionalExpression(_)
        );
        self.check_expr_with_policy(e, (ctx, kind), fx, false, true, result_operand)
    }

    /// Rejects a `void` value at `pos`. The site names a destination of a
    /// type that is not `void`.
    pub(super) fn reject_void_value(&mut self, ctx: Option<&Type>, pos: Pos) {
        self.reject_subset(
            if ctx.is_some_and(|ty| !matches!(self.apparent_type(ty), Type::Void | Type::Error)) {
                RejectionSite::VoidExpressionNonVoidDestination
            } else {
                RejectionSite::VoidExpressionValue
            },
            "a `void` expression is only allowed as an expression statement",
            pos,
        );
    }

    fn check_expr_with_policy(
        &mut self,
        e: &ast::Expression,
        (ctx, kind): (Option<&Type>, ContextKind),
        fx: &mut FnCtx<'p>,
        allow_embedded_header_receiver: bool,
        keep_void: bool,
        preserve_result_operand: bool,
    ) -> hir::Expr {
        let previous_numeric = fx.descriptor_numeric_operand;
        if fx.descriptor_default.is_some() {
            if matches!(e, ast::Expression::BinaryExpression(b) if matches!(b.operator,
                ast::BinaryOperator::Addition | ast::BinaryOperator::Subtraction | ast::BinaryOperator::Multiplication |
                ast::BinaryOperator::Division | ast::BinaryOperator::Remainder | ast::BinaryOperator::Exponential |
                ast::BinaryOperator::ShiftLeft | ast::BinaryOperator::ShiftRight | ast::BinaryOperator::ShiftRightZeroFill |
                ast::BinaryOperator::BitwiseOR | ast::BinaryOperator::BitwiseXOR | ast::BinaryOperator::BitwiseAnd))
                || matches!(e, ast::Expression::UnaryExpression(u) if matches!(u.operator, ast::UnaryOperator::UnaryNegation | ast::UnaryOperator::UnaryPlus | ast::UnaryOperator::BitwiseNot))
            {
                fx.descriptor_numeric_operand = true;
            } else if matches!(
                e,
                ast::Expression::CallExpression(_) | ast::Expression::ArrowFunctionExpression(_)
            ) {
                fx.descriptor_numeric_operand = false;
            }
        }
        // compiler.md §249 rule 1a: only the forms that `reads_hint` names
        // read a hint. Each other form reads a hint as no context.
        let expected = match kind {
            ContextKind::Hint | ContextKind::FunctionListHint if !reads_hint(e) => None,
            _ => ctx,
        };
        // compiler.md §290 rule 1: arguments and other value positions do
        // not read a lambda's result, even inside a return operand.
        let saved_operand = self.inferring_result_operand;
        self.inferring_result_operand &= preserve_result_operand;
        let checked =
            self.check_expr_inner(e, (expected, kind), fx, allow_embedded_header_receiver);
        self.inferring_result_operand = saved_operand;
        fx.descriptor_numeric_operand = previous_numeric;
        self.finish_expr_policy(checked, ctx, fx, allow_embedded_header_receiver, keep_void)
    }

    fn finish_expr_policy(
        &mut self,
        checked: hir::Expr,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
        allow_embedded_header_receiver: bool,
        keep_void: bool,
    ) -> hir::Expr {
        if self.apparent_type(&checked.ty) == Type::TaskGroup
            && fx.frames.iter().any(|frame| frame.is_generator)
        {
            self.reject_subset(
                RejectionSite::TaskGroupGeneratorBody,
                "TaskGroup is not allowed in a generator body",
                checked.pos.clone(),
            );
            self.err_expr(checked.pos)
        } else if self.apparent_type(&checked.ty) == Type::TaskGroup
            && !allow_embedded_header_receiver
            && ctx != Some(&Type::TaskGroup)
            && !matches!(
                checked.kind,
                ExprKind::TaskGroup {
                    operation: hir::TaskGroupOperation::Create,
                    ..
                }
            )
        {
            self.reject_subset(
                RejectionSite::TaskGroupPosition,
                "TaskGroup cannot move or copy",
                checked.pos.clone(),
            );
            self.err_expr(checked.pos)
        } else if self.apparent_type(&checked.ty) == Type::array(Type::Void) {
            self.reject_subset(
                RejectionSite::PromiseAllVoidValue,
                "the language has no void[] value",
                checked.pos.clone(),
            );
            self.err_expr(checked.pos)
        } else if !keep_void && self.apparent_type(&checked.ty) == Type::Void {
            self.reject_void_value(ctx, checked.pos.clone());
            self.err_expr(checked.pos)
        } else {
            checked
        }
    }

    /// Checks the operand `e` of `return` in a function with a `void`
    /// result (compiler.md §237 rule 2). Returns the statements of `e` and
    /// the type of `e` as an expression, with no context: an assignment
    /// has the type of its value, and an optional chain has the type of its
    /// member or its call.
    pub(crate) fn check_void_operand(
        &mut self,
        e: &ast::Expression,
        fx: &mut FnCtx<'p>,
    ) -> (Vec<hir::Stmt>, Type) {
        let e = super::unparen_expr(e);
        if let ast::Expression::ChainExpression(chain) = e {
            return self.check_optional_chain_operand(chain, fx);
        }
        let checked = self.check_expr_inner(e, (None, ContextKind::Contextual), fx, false);
        let ty = checked.ty.clone();
        (vec![hir::Stmt::Expr(checked)], ty)
    }

    fn check_expr_inner(
        &mut self,
        e: &ast::Expression,
        expected: (Option<&Type>, ContextKind),
        fx: &mut FnCtx<'p>,
        allow_embedded_header_receiver: bool,
    ) -> hir::Expr {
        self.expression_work.push(None);
        let mut checked =
            self.check_expr_inner_untracked(e, expected, fx, allow_embedded_header_receiver);
        if let Some(work) = self.expression_work.pop().flatten() {
            checked.pending_work = Some(work);
        }
        checked
    }

    fn check_expr_inner_untracked(
        &mut self,
        e: &ast::Expression,
        (ctx, kind): (Option<&Type>, ContextKind),
        fx: &mut FnCtx<'p>,
        allow_embedded_header_receiver: bool,
    ) -> hir::Expr {
        let pos = self.pos(e.span());
        // The list-selection policy belongs only to this conditional
        // element (through parentheses), never to a nested container.
        let list_candidate = kind == ContextKind::FunctionListHint;
        let kind = if list_candidate {
            ContextKind::Hint
        } else {
            kind
        };
        // A handler can skip the ordinary finish, as a direct return does.
        let mut early_return = false;
        let checked = self.check_expr_form(
            e,
            (ctx, kind, list_candidate),
            fx,
            allow_embedded_header_receiver,
            pos,
            &mut early_return,
        );
        if early_return {
            return checked;
        }
        self.finish_expr_form(e, ctx, checked, fx, allow_embedded_header_receiver)
    }

    fn finish_expr_form(
        &mut self,
        e: &ast::Expression,
        ctx: Option<&Type>,
        mut checked: hir::Expr,
        fx: &mut FnCtx<'p>,
        allow_embedded_header_receiver: bool,
    ) -> hir::Expr {
        // compiler.md §339 rules 1–3: a literal place reads a const's literal.
        if self.apparent_type(&checked.ty) == Type::Str
            && ctx.is_some_and(|ty| matches!(self.apparent_type(ty), Type::StringAlias(_)))
            && matches!(
                super::unparen_expr(e),
                ast::Expression::Identifier(_) | ast::Expression::TemplateLiteral(_)
            )
        {
            if let Some(text) = self
                .literal_operand(e, fx)
                .and_then(|value| value.as_string().map(str::to_owned))
            {
                checked = self.check_lit(
                    &ast::Expression::new_string_literal(
                        e.span(),
                        text.as_str(),
                        None,
                        &oxc_ast::builder::AstBuilder::new(self.prog.allocator),
                    ),
                    ctx,
                    checked.pos.clone(),
                );
            }
        }
        self.end_shared_narrowing(&checked, fx);
        if !allow_embedded_header_receiver {
            self.reject_embedded_header_copy(&mut checked, ctx);
        }
        checked
    }

    fn check_expr_form(
        &mut self,
        e: &ast::Expression,
        (ctx, kind, list_candidate): (Option<&Type>, ContextKind, bool),
        fx: &mut FnCtx<'p>,
        allow_embedded_header_receiver: bool,
        pos: Pos,
        early_return: &mut bool,
    ) -> hir::Expr {
        match e {
            ast::Expression::ParenthesizedExpression(p) => self.check_expr_with_policy(
                &p.expression,
                (
                    ctx,
                    if list_candidate {
                        ContextKind::FunctionListHint
                    } else {
                        kind
                    },
                ),
                fx,
                allow_embedded_header_receiver,
                false,
                true,
            ),
            lit @ (ast::Expression::NumericLiteral(_)
            | ast::Expression::StringLiteral(_)
            | ast::Expression::BooleanLiteral(_)
            | ast::Expression::NullLiteral(_)
            | ast::Expression::BigIntLiteral(_)
            | ast::Expression::RegExpLiteral(_)) => self.check_lit(lit, ctx, pos),
            ast::Expression::TemplateLiteral(tpl) => self.check_template(tpl, fx, pos),
            ast::Expression::Identifier(id) => self.check_ident(id, ctx, fx),
            ast::Expression::ThisExpression(_) => self.check_this_value(fx, pos, early_return),
            ast::Expression::UnaryExpression(u) => self.check_unary(u, ctx, fx, pos),
            ast::Expression::UpdateExpression(u) => self.check_update(u, fx, pos, false, None),
            ast::Expression::BinaryExpression(b) => self.check_bin(b, ctx, fx, pos),
            ast::Expression::LogicalExpression(b) => self.check_logical(b, ctx, fx, pos),
            ast::Expression::AssignmentExpression(a) => self.check_assign(a, fx, pos, false, None),
            expression if expression.as_member_expression().is_some() => {
                match expression.as_member_expression() {
                    Some(m) => self.check_member_read(m, fx),
                    None => self.err_expr(pos),
                }
            }
            ast::Expression::ChainExpression(chain) => {
                self.reject_unbound_optional_chain(chain, fx, pos)
            }
            ast::Expression::ConditionalExpression(c) => {
                self.check_cond(c, (ctx, kind), list_candidate, fx, pos)
            }
            ast::Expression::CallExpression(c) => self.check_call(c, ctx, fx, pos),
            ast::Expression::NewExpression(n) => self.check_new(n, ctx, fx, pos),
            ast::Expression::ArrowFunctionExpression(a) => {
                self.check_lambda(a, (ctx, kind), fx, pos)
            }
            ast::Expression::ArrayExpression(a) => self.check_array_lit(a, (ctx, kind), fx, pos),
            ast::Expression::ObjectExpression(object) => {
                self.check_object_form(object, (ctx, kind), fx, pos)
            }
            ast::Expression::TSAsExpression(a) if a.type_annotation.is_const_type_reference() => {
                self.reject_expr_form(e, pos)
            }
            ast::Expression::TSTypeAssertion(a) if a.type_annotation.is_const_type_reference() => {
                self.reject_expr_form(e, pos)
            }
            ast::Expression::TSAsExpression(a) => self.check_as(a, fx, pos),
            ast::Expression::PrivateInExpression(_) | ast::Expression::ImportExpression(_) => {
                self.reject_expr_form(e, pos)
            }
            // compiler.md §204 rule 3: `x satisfies T` is `x`, checked as
            // the initializer of a `T` local. An operand that holds an
            // empty array literal is checked with no context, as `tsc`
            // does not give `T` to that literal.
            ast::Expression::TSSatisfiesExpression(s) => self.check_satisfies_expression(s, fx),
            ast::Expression::YieldExpression(y) => self.check_yield(y, fx, pos),
            ast::Expression::AwaitExpression(a) => self.check_await_expression(a, ctx, fx, pos),
            other => self.reject_expr_form(other, pos),
        }
    }

    fn check_await_expression(
        &mut self,
        a: &ast::AwaitExpression,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        // §188.1 rule 2: an `await` result that holds a handle is
        // an origin.
        let value = self.check_await(a, ctx, fx, pos);
        self.track_async_call_result(value, fx)
    }

    fn reject_expr_form(&mut self, e: &ast::Expression, pos: Pos) -> hir::Expr {
        match e {
            ast::Expression::TSAsExpression(a) if a.type_annotation.is_const_type_reference() => {
                self.reject_subset(
                    RejectionSite::ConstAssertionExpression,
                    "expression form outside the decided surface",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            ast::Expression::TSTypeAssertion(a) if a.type_annotation.is_const_type_reference() => {
                self.reject_subset(
                    RejectionSite::ConstAssertionExpression,
                    "expression form outside the decided surface",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            ast::Expression::PrivateInExpression(_) => {
                self.reject_subset(
                    RejectionSite::InOperator,
                    "operator outside the decided surface",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            ast::Expression::ImportExpression(import) => {
                let site = if Self::is_dynamic_import_call(import) {
                    RejectionSite::DynamicImportCall
                } else {
                    RejectionSite::NonExpressionCalleeUnavailable
                };
                self.reject_subset(site, "call form outside the decided surface", pos.clone());
                self.err_expr(pos)
            }
            ast::Expression::TSNonNullExpression(t) => {
                let p = self.pos(t.span);
                self.reject_subset(
                    RejectionSite::NonNullAssertionExpression,
                    "the `!` assertion is not in the decided surface; narrow with a null check",
                    p.clone(),
                );
                self.err_expr(p)
            }
            ast::Expression::FunctionExpression(_) => {
                self.reject_subset(
                    RejectionSite::FunctionExpression,
                    "function expressions are not in the decided surface; use an arrow",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            other => {
                let p = self.pos(other.span());
                self.reject_subset(
                    match other {
                        ast::Expression::TSTypeAssertion(_) => {
                            RejectionSite::AngleAssertionExpression
                        }
                        ast::Expression::TSInstantiationExpression(_) => {
                            RejectionSite::InstantiationExpression
                        }
                        ast::Expression::SequenceExpression(_) => RejectionSite::CommaExpression,
                        ast::Expression::TaggedTemplateExpression(_) => {
                            RejectionSite::TaggedTemplateExpression
                        }
                        ast::Expression::ClassExpression(_) => RejectionSite::ClassExpression,
                        ast::Expression::ImportMeta(_) | ast::Expression::NewTarget(_) => {
                            RejectionSite::MetaPropertyExpression
                        }
                        ast::Expression::PrivateInExpression(_) => {
                            RejectionSite::PrivateNameExpression
                        }
                        _ => RejectionSite::UnsupportedExpressionKind,
                    },
                    "expression form outside the decided surface",
                    p.clone(),
                );
                self.err_expr(p)
            }
        }
    }

    fn check_this_value(
        &mut self,
        fx: &mut FnCtx<'p>,
        pos: Pos,
        early_return: &mut bool,
    ) -> hir::Expr {
        if fx
            .frames
            .last()
            .is_some_and(|frame| frame.is_lambda && frame.is_async)
        {
            self.diags
                .push(crate::check::rejection::async_receiver_diagnostic(
                    pos.clone(),
                ));
            *early_return = true;
            return self.err_expr(pos);
        }
        if fx.descriptor_default.is_some() {
            self.reject_subset(
                RejectionSite::DescriptorDefaultThisUse,
                "§147 rule 3a: `this` is forbidden in a descriptor member default",
                pos.clone(),
            );
            *early_return = true;
            return self.err_expr(pos);
        }
        if let Some(initializer) = &fx.field_initializer {
            let reason = if fx.frames.last().is_some_and(|frame| frame.is_lambda) {
                "`this` inside a lambda is forbidden"
            } else if initializer.write {
                "a write through `this` is forbidden"
            } else {
                "`this` as a value is forbidden"
            };
            self.reject_subset(
                RejectionSite::FieldInitializerThisUse,
                format!("§147 rule 2: {reason}"),
                pos.clone(),
            );
            *early_return = true;
            return self.err_expr(pos);
        }
        let is_lambda = fx.frames.last().is_some_and(|frame| frame.is_lambda);
        let this_ty = fx.frames.first().and_then(|frame| frame.this_ty.clone());
        if is_lambda {
            if fx.parameter_default && this_ty.is_some() {
                self.reject_subset(
                    RejectionSite::ThisInParameterDefaultArrow,
                    "a lambda in a parameter default cannot capture `this`",
                    pos.clone(),
                );
                *early_return = true;
                return self.err_expr(pos);
            }
            if let Some(Type::Class(id)) = this_ty.as_ref().map(|ty| self.apparent_type(ty)) {
                if self.classes[id.0].is_value {
                    self.reject_subset(
                        RejectionSite::ThisInValueTypeArrow,
                        "a lambda cannot capture a ValueType receiver; the capture is a copy",
                        pos.clone(),
                    );
                    *early_return = true;
                    return self.err_expr(pos);
                }
            }
            if this_ty.is_some() {
                if let Some(local) = self.lookup_local("this", &pos, fx) {
                    *early_return = true;
                    return hir::Expr {
                        pending_work: None,
                        kind: ExprKind::Local(
                            "this".to_string(),
                            local.ty.clone(),
                            local.annotated,
                        ),
                        ty: local.ty,
                        pos,
                    };
                }
            }
        }
        match this_ty {
            Some(ty) => hir::Expr {
                pending_work: None,
                kind: ExprKind::This,
                ty,
                pos,
            },
            None => {
                let site = if is_lambda
                    && fx
                        .frames
                        .first()
                        .is_some_and(|frame| frame.static_this_class.is_some())
                {
                    RejectionSite::ThisInStaticMethodArrow
                } else {
                    fx.frames
                        .last()
                        .and_then(|frame| frame.missing_this_site)
                        .unwrap_or(RejectionSite::ThisOutsideMethod)
                };
                self.reject_subset(
                site,
                if site == RejectionSite::ThisInStaticMethodArrow {
                    "a static method must name its class instead of `this`; use `ClassName.member`"
                } else {
                    "`this` is only available in constructors and methods"
                },
                pos.clone(),
            );
                self.err_expr(pos)
            }
        }
    }

    fn check_object_form(
        &mut self,
        object: &ast::ObjectExpression,
        (ctx, kind): (Option<&Type>, ContextKind),
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        match contextual_object_class(ctx, |ty| self.apparent_type(ty)) {
            Some(id) if self.classes[id.0].is_descriptor => {
                self.check_descriptor_lit(object, (id, kind), fx, pos)
            }
            Some(_) => {
                self.reject_subset(
                    RejectionSite::NominalObjectLiteral,
                    "object literals do not satisfy nominal class types",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
            _ => {
                // C1: the literal has no standalone type, so only a
                // `@Descriptor` context constructs from one.
                self.reject_subset(
                    RejectionSite::ObjectLiteralWithoutDescriptorContext,
                    "object literals are not in the decided surface",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    fn check_satisfies_expression(
        &mut self,
        s: &ast::TSSatisfiesExpression,
        fx: &mut FnCtx<'p>,
    ) -> hir::Expr {
        let target = self.resolve_type(&s.type_annotation);
        let context = (!holds_empty_array_literal(&s.expression)).then_some(&target);
        let mut value = self.check_expr(&s.expression, context, fx);
        self.require_expr_assignable(&mut value, &target, fx, "the initializer");
        value
    }

    fn embedded_header_projection(&self, expr: &hir::Expr) -> Option<(ClassId, ClassId)> {
        let ExprKind::Field { obj, name } = &expr.kind else {
            return None;
        };
        let Type::Class(extension) = self.apparent_type(&obj.ty) else {
            return None;
        };
        let definition = self.classes.get(extension.0)?;
        let first = definition.fields.first()?;
        let Type::Class(header) = self.apparent_type(&first.ty) else {
            return None;
        };
        let nullable = Type::nullable(Type::Class(header));
        let used_as_link = self.classes.iter().any(|class| {
            class.is_boundary && class.fields.iter().any(|field| field.ty == nullable)
        }) || self.foreign_defs.iter().any(|function| {
            function
                .params
                .iter()
                .any(|parameter| parameter.ty == nullable)
        });
        (definition.is_value
            && definition.is_boundary
            && first.name == *name
            && self
                .classes
                .get(header.0)
                .is_some_and(|class| class.is_value && class.is_boundary)
            && used_as_link)
            .then_some((extension, header))
    }

    fn reject_embedded_header_copy(&mut self, expr: &mut hir::Expr, expected: Option<&Type>) {
        if self.apparent_type(&(expr.ty)) == Type::Error {
            return;
        }
        let Some((extension, header)) = self.embedded_header_projection(expr) else {
            return;
        };
        let nullable_header = Type::nullable(Type::Class(header));
        if expected == Some(&nullable_header) {
            return;
        }
        let extension_name = self.classes[extension.0].name.clone();
        let header_name = self.classes[header.0].name.clone();
        self.reject_subset(RejectionSite::EmbeddedHeaderCopied, format!(
                "embedded header `{extension_name}.{}` cannot be copied as `{header_name}`; store it directly into `{header_name} | null` or read one of its fields",
                match &expr.kind {
                    ExprKind::Field { name, .. } => name.as_str(),
                    _ => unreachable!("embedded header projection is a field"),
                }
            ), expr.pos.clone());
        expr.ty = Type::Error;
    }

    /// Checks an expression statement, admitting the ambient
    /// `unreachable()` only when it is the statement's direct call.
    pub(crate) fn check_expr_stmt(
        &mut self,
        e: &ast::Expression,
        fx: &mut FnCtx<'p>,
    ) -> Vec<hir::Stmt> {
        let mut root = e;
        while let ast::Expression::ParenthesizedExpression(paren) = root {
            root = &paren.expression;
        }
        if let ast::Expression::CallExpression(call) = root {
            {
                let callee = &call.callee;
                let mut callee: &ast::Expression = callee;
                while let ast::Expression::ParenthesizedExpression(paren) = callee {
                    callee = &paren.expression;
                }
                if let ast::Expression::Identifier(ident) = callee {
                    if ident.name.as_ref() == "unreachable" {
                        let pos = self.pos(call.span);
                        let checked = self.checked_call(call, Self::poisoned_call, |checker| {
                            checker.check_named_call(ident, call, fx, pos, true)
                        });
                        self.end_shared_narrowing(&checked, fx);
                        return vec![hir::Stmt::Expr(checked)];
                    }
                }
            }
        }
        if let ast::Expression::ChainExpression(chain) = root {
            return self.check_optional_chain_statement(chain, fx);
        }
        let mut prefix = crate::check::SyntheticPrefix::default();
        if let ast::Expression::AssignmentExpression(assign) = root {
            if let Some(logical) = super::logical_assign::LogicalAssign::of(assign.operator) {
                return self.check_logical_assign_statement(
                    assign,
                    logical,
                    fx,
                    self.pos(root.span()),
                );
            }
            let checked =
                self.check_assign(assign, fx, self.pos(root.span()), true, Some(&mut prefix));
            self.end_shared_narrowing(&checked, fx);
            let mut out = Vec::new();
            out.extend(prefix);
            out.push(hir::Stmt::Expr(checked));
            return out;
        }
        if let ast::Expression::UpdateExpression(update) = root {
            let checked =
                self.check_update(update, fx, self.pos(root.span()), true, Some(&mut prefix));
            self.end_shared_narrowing(&checked, fx);
            let mut out = Vec::new();
            out.extend(prefix);
            out.push(hir::Stmt::Expr(checked));
            return out;
        }
        // compiler.md §300 rule 1: a conditional, `&&`, or `||` takes
        // statement leaves, regardless of their value types.
        let root = self.enter_void_leaf_place(root, super::VoidLeafPlace::Statement);
        let checked = self.check_expr_inner(root, (None, ContextKind::Contextual), fx, false);
        self.void_leaf_place = None;
        let mut statements = prefix.into_statements();
        statements.extend(self.void_leaf_statements(checked));
        statements
    }

    /// Checks the three awaitable forms (§26, §37). The AST call is handled
    /// here instead of through the ordinary call path so an async call can
    /// never materialize a Promise-typed value in HIR. A call operand has
    /// the contextual type `Promise<C>` for the context `C` of the `await`
    /// (§215 rule 3).
    fn check_await(
        &mut self,
        awaited: &ast::AwaitExpression,
        ctx: Option<&Type>,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        if !fx.frames.last().is_some_and(|frame| frame.is_async) {
            self.reject_subset(
                RejectionSite::AwaitOutsideAsync,
                "`await` is only legal inside an async function",
                pos.clone(),
            );
            return self.err_expr(pos);
        }

        let mut operand: &ast::Expression = &awaited.argument;
        while let ast::Expression::ParenthesizedExpression(paren) = operand {
            operand = &paren.expression;
        }
        if let ast::Expression::ImportExpression(import) = operand {
            self.reject_subset(
                if Self::is_dynamic_import_call(import) {
                    RejectionSite::DynamicImportCall
                } else {
                    RejectionSite::AwaitNotDirectCall
                },
                "awaitable expressions must be direct calls",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let ast::Expression::CallExpression(call) = operand else {
            // §232 rule 3: a conditional or an optional call takes
            // `Promise<C>` too.
            let context = ctx.map(|value| Type::AsyncHandle(Box::new(value.clone())));
            let context = self.operand_context(operand, context.as_ref());
            let handle = self.check_expr(operand, context.as_ref(), fx);
            let Type::AsyncHandle(value) = self.apparent_type(&handle.ty.clone()) else {
                if self.apparent_type(&(handle.ty)) != Type::Error {
                    self.reject_subset(RejectionSite::AwaitNonHandle, "`await` requires `Context.suspend()`, an async call, or a held async handle", pos.clone());
                }
                return self.err_expr(pos);
            };
            let origins = self.expr_async_origins(&handle, fx);
            fx.handle_async_origins(&origins);
            return hir::Expr {
                pending_work: None,
                kind: ExprKind::AsyncHandleAwait(Box::new(handle)),
                ty: *value,
                pos,
            };
        };
        let context = ctx.map(|value| Type::AsyncHandle(Box::new(value.clone())));
        let active = self.active_call.replace(call.span);
        let active_context = self.enter_call_context(call, context.as_ref());
        let checked = self.checked_call(call, Self::poisoned_call, |checker| {
            checker.check_await_call(operand, call, fx, pos)
        });
        self.active_call = active;
        self.active_call_context = active_context;
        checked
    }

    /// Checks an awaited call. `active_call_context` holds its contextual
    /// type for inference. A path that checks `operand` has no inference
    /// that reads a context, so it passes none.
    fn check_await_call(
        &mut self,
        operand: &ast::Expression,
        call: &ast::CallExpression,
        fx: &mut FnCtx<'p>,
        pos: Pos,
    ) -> hir::Expr {
        if matches!(call.callee, ast::Expression::Super(_)) {
            self.reject_subset(
                RejectionSite::AwaitNotDirectCall,
                "awaitable expressions must be direct calls",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
        let callee = &call.callee;
        let mut callee: &ast::Expression = callee;
        while let ast::Expression::ParenthesizedExpression(paren) = callee {
            callee = &paren.expression;
        }

        if let Some(member) = callee.as_member_expression() {
            let view = super::member_view(member);
            let member = &view;
            let group_receiver = if let ast::Expression::Identifier(id) = member.object {
                fx.scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.vars.get(id.name.as_ref()))
                    .is_some_and(|local| self.apparent_type(&local.ty) == Type::TaskGroup)
            } else {
                false
            };
            let group_join = group_receiver
                && matches!(&member.prop,
                super::MemberKey::Ident(prop) if prop.name.as_ref() == "join");
            if group_join
                || (self.ambient_namespace(member.object, fx) == Some("Promise")
                    && matches!(&member.prop, super::MemberKey::Ident(prop) if prop.name.as_ref() == "all"))
            {
                let handle = self.check_expr_with_result_dependency(operand, fx);
                let Type::AsyncHandle(value) = self.apparent_type(&handle.ty) else {
                    return self.err_expr(pos);
                };
                let origins = self.expr_async_origins(&handle, fx);
                fx.handle_async_origins(&origins);
                return hir::Expr {
                    pending_work: None,
                    kind: ExprKind::AsyncHandleAwait(Box::new(handle)),
                    ty: *value,
                    pos,
                };
            }
            if self.is_context_namespace(member.object, fx)
                && matches!(&member.prop, super::MemberKey::Ident(prop) if prop.name.as_ref() == "suspend")
            {
                if call.type_arguments.is_some() || !call.arguments.is_empty() {
                    self.reject_subset(
                        RejectionSite::ContextSuspendArguments,
                        "`Context.suspend()` takes no type arguments or value arguments",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                return hir::Expr {
                    pending_work: None,
                    kind: ExprKind::AsyncSuspend,
                    ty: Type::Void,
                    pos,
                };
            }
        }

        let indirect = match callee {
            ast::Expression::Identifier(id) => {
                fx.owns_local_name(id.name.as_ref())
                    || matches!(
                        self.peek_scope_item(id.name.as_ref()),
                        Some(ScopeItem::Global(_))
                    )
            }
            expression if expression.as_member_expression().is_some() => {
                if let ast::Expression::StaticMemberExpression(member) = expression {
                    match &member.object {
                        ast::Expression::Identifier(receiver)
                            if !fx.owns_local_name(receiver.name.as_ref()) =>
                        {
                            matches!(self.peek_scope_item(receiver.name.as_ref()), Some(ScopeItem::Class(class)) if self.class_sigs[class.0].static_fields.contains_key(member.property.name.as_ref()))
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            }
            _ => true,
        };
        let completion = matches!(callee, ast::Expression::Identifier(id) if !fx.owns_local_name(id.name.as_ref())
            && self.foreign_defs.iter().any(|f| f.name == id.name.as_ref() && f.completion_result.is_some()));
        let standard = matches!(callee, ast::Expression::Identifier(id) if !fx.owns_local_name(id.name.as_ref()) && matches!(self.peek_scope_item(id.name.as_ref()), Some(ScopeItem::StandardFile(_))));
        if indirect || completion || standard {
            let handle = self.check_expr_with_result_dependency(operand, fx);
            if let Type::AsyncHandle(value) = self.apparent_type(&handle.ty) {
                let origins = self.expr_async_origins(&handle, fx);
                fx.handle_async_origins(&origins);
                return hir::Expr {
                    pending_work: None,
                    kind: ExprKind::AsyncHandleAwait(Box::new(handle)),
                    ty: *value,
                    pos,
                };
            }
            if self.apparent_type(&handle.ty) != Type::Error {
                let site = if matches!(callee, ast::Expression::Identifier(_)) {
                    RejectionSite::AwaitNonHandle
                } else {
                    RejectionSite::AwaitIndirectCall
                };
                self.reject_subset(
                    site,
                    "await requires a call that returns an async handle",
                    pos.clone(),
                );
            }
            return self.err_expr(pos);
        }

        // §234 rule 1: a synchronous static method is an ordinary call, and
        // `await` reads its handle. The call takes no context (§215.1 rule
        // 3a). A function and an instance method take the same step below,
        // after their instance is known.
        if let Some(handle) = self.await_static_call(callee, operand, fx) {
            return self.await_call_handle(handle, fx, pos);
        }
        // Each path below awaits a user function, a generic function, an
        // instance method, a generic method, an interface method, or a
        // promise reaction that rejects its own type arguments (§231 rule 1).
        self.mark_user_call(call, callee.is_member_expression());
        match callee {
            ast::Expression::Identifier(ident) => {
                let name = ident.name.to_string();
                let callee_pos = self.pos(ident.span);
                let item = self.scope_item(&name, &callee_pos, fx);
                if matches!(item, Some(ScopeItem::Poisoned))
                    || matches!(&item, Some(ScopeItem::Global(g)) if self.global_sigs.get(g).is_some_and(|s| matches!(s.state, crate::check::initializer::TypeState::Rejected)))
                {
                    self.check_poisoned_arguments(&call.arguments, fx);
                    return self.err_expr(pos);
                }
                let mut checked = None;
                let mut instance_result = None;
                let (function, checked_name, rejects_type_args) = match item {
                    Some(ScopeItem::Func(function)) => {
                        (function, name.clone(), call.type_arguments.is_some())
                    }
                    Some(ScopeItem::GenericFunc(key)) => {
                        let arguments = if let Some(type_args) = &call.type_arguments {
                            self.resolve_instance_arguments(type_args)
                        } else {
                            // A synchronous callee is rejected below, so
                            // `Promise<C>` decides no type argument of it.
                            let is_async = self
                                .generic_fns
                                .get(&key)
                                .is_some_and(|template| template.function.r#async);
                            if !is_async {
                                self.active_call_context = None;
                            }
                            let Some((arguments, values)) =
                                self.infer_call_arguments(&key, call, fx, &callee_pos)
                            else {
                                return self.err_expr(pos);
                            };
                            checked = Some(values);
                            arguments
                        };
                        let Some(instance) =
                            self.instantiate_fn(&key, &arguments, self.pos(ident.span))
                        else {
                            return self.err_expr(pos);
                        };
                        instance_result = Some((key, arguments.types));
                        (instance.clone(), instance, false)
                    }
                    _ => {
                        self.reject_subset(
                            RejectionSite::AwaitUndeclaredAsyncFunction,
                            format!("`{name}` is not a directly declared async function"),
                            self.pos(ident.span),
                        );
                        return self.err_expr(pos);
                    }
                };
                let Some(mut sig) = self.fn_sigs.get(&function).cloned() else {
                    return self.err_expr(pos);
                };
                if let Some((key, arguments)) = &instance_result {
                    sig.ret = self.call_instance_result(key, &function, arguments, false);
                }
                // §234 rule 1: a synchronous function whose instantiated
                // result is a handle is an ordinary call.
                if !sig.is_async && matches!(self.apparent_type(&sig.ret), Type::AsyncHandle(_)) {
                    if rejects_type_args {
                        self.reject_subset(
                            RejectionSite::AwaitFunctionTypeArguments,
                            format!("`{name}` is not generic"),
                            self.pos(ident.span),
                        );
                    }
                    let call_pos = self.pos(call.span);
                    let mut handle = self
                        .check_direct_call_with_arguments(&function, call, fx, call_pos, checked);
                    if let Some((key, arguments)) = &instance_result {
                        handle.ty = self.call_instance_result(key, &function, arguments, true);
                    }
                    return self.await_call_handle(handle, fx, pos);
                }
                if !sig.is_async {
                    self.reject_subset(
                        RejectionSite::AwaitSynchronousFunction,
                        format!("`{checked_name}` is synchronous and cannot be awaited"),
                        self.pos(ident.span),
                    );
                    return self.err_expr(pos);
                }
                if rejects_type_args {
                    self.reject_subset(
                        RejectionSite::AwaitFunctionTypeArguments,
                        format!("`{name}` is not generic"),
                        self.pos(ident.span),
                    );
                }
                let args = self.check_args_with_arguments(
                    RejectionSite::AwaitMethodArgumentCount,
                    &sig.params,
                    &call.arguments,
                    fx,
                    &pos,
                    &checked_name,
                    checked,
                    sig.generic,
                );
                hir::Expr {
                    pending_work: None,
                    kind: ExprKind::AsyncCall {
                        callee: AsyncCallee::Function(hir::Symbol::from_full_text(function)),
                        args,
                    },
                    ty: sig.ret,
                    pos,
                }
            }
            expression if expression.as_member_expression().is_some() => {
                let Some(member) = expression.as_member_expression() else {
                    return self.err_expr(pos);
                };
                let view = super::member_view(member);
                let member = &view;
                if let super::MemberKey::Ident(method) = &member.prop {
                    if self.rejected_static_generic_method(member.object, method.name.as_ref(), fx)
                    {
                        return self.err_expr(pos);
                    }
                }
                let receiver = self.check_receiver(member.object, fx);
                let receiver_type = self.apparent_type(&receiver.ty);
                // §186 rule 5: `await h.then(...)` awaits the reaction handle.
                if let (Type::AsyncHandle(_), super::MemberKey::Ident(method)) =
                    (&receiver_type, &member.prop)
                {
                    if matches!(method.name.as_ref(), "then" | "catch" | "finally") {
                        let handle = self.check_promise_reaction(
                            receiver,
                            method,
                            call,
                            fx,
                            self.pos(call.span),
                        );
                        let Type::AsyncHandle(result) = self.apparent_type(&handle.ty) else {
                            return self.err_expr(pos);
                        };
                        let origins = self.expr_async_origins(&handle, fx);
                        fx.handle_async_origins(&origins);
                        return hir::Expr {
                            pending_work: None,
                            kind: ExprKind::AsyncHandleAwait(Box::new(handle)),
                            ty: *result,
                            pos,
                        };
                    }
                }
                let field = match (&receiver_type, &member.prop) {
                    (Type::Class(class), super::MemberKey::Ident(name)) => self.classes[class.0]
                        .fields
                        .iter()
                        .any(|field| field.name == name.name.as_ref()),
                    (Type::Array(_) | Type::FixedArray(_, _), super::MemberKey::Computed(_)) => {
                        true
                    }
                    _ => false,
                };
                if field {
                    let callable = match &member.prop {
                        super::MemberKey::Ident(name) => {
                            let mut field = self.member_on(
                                receiver,
                                name.name.as_ref(),
                                self.pos(name.span),
                                None,
                                fx,
                            );
                            self.apply_narrowing(&mut field, fx);
                            field
                        }
                        super::MemberKey::Computed(index) => {
                            self.mark_operand_calls(&index.expression);
                            let index = self.check_expr(&index.expression, Some(&Type::I32), fx);
                            self.check_index(receiver, index, self.pos(member.span), fx)
                        }
                        super::MemberKey::PrivateName(_) => return self.err_expr(pos),
                    };
                    let handle = self.check_indirect_call(callable, call, fx, self.pos(call.span));
                    if let Type::AsyncHandle(value) = self.apparent_type(&handle.ty) {
                        let origins = self.expr_async_origins(&handle, fx);
                        fx.handle_async_origins(&origins);
                        return hir::Expr {
                            pending_work: None,
                            kind: ExprKind::AsyncHandleAwait(Box::new(handle)),
                            ty: *value,
                            pos,
                        };
                    }
                    if self.apparent_type(&handle.ty) != Type::Error {
                        self.reject_subset(
                            RejectionSite::AwaitNonHandle,
                            "await requires a call that returns an async handle",
                            pos.clone(),
                        );
                    }
                    return self.err_expr(pos);
                }
                let super::MemberKey::Ident(method) = &member.prop else {
                    self.reject_subset(
                        RejectionSite::AwaitComputedMethod,
                        "an awaited async method requires an identifier method name",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                };
                if let Some(value) =
                    self.check_constrained_method_call(&receiver, method, call, fx, &pos, true)
                {
                    return value;
                }
                if let Type::Interface(interface) = self.apparent_type(&receiver.ty) {
                    return self
                        .await_interface_method(receiver, &interface, method, call, fx, pos);
                }
                let name = method.name.to_string();
                let method_pos = self.pos(method.span);
                let Type::Class(class) = self.apparent_type(&receiver.ty.clone()) else {
                    if self.apparent_type(&(receiver.ty)) != Type::Error {
                        let receiver_ty = self.type_name(&receiver.ty);
                        self.reject_subset(
                            RejectionSite::AwaitNonClassMethod,
                            format!("type `{receiver_ty}` has no async method `{name}`"),
                            method_pos,
                        );
                    }
                    return self.err_expr(pos);
                };
                if self.reject_member_access(class, &name, false, false, fx, method_pos.clone()) {
                    return self.err_expr(pos);
                }
                let generic = self.class_sigs[class.0].has_generic_method(&name, false);
                let mut inferred = None;
                let mut result_type = None;
                let name = if generic {
                    // A synchronous method is rejected below, so `Promise<C>`
                    // decides no type argument of it.
                    let is_async = self.class_sigs[class.0]
                        .generic_methods
                        .get(&name)
                        .is_some_and(|template| template.function.r#async);
                    if !is_async {
                        self.active_call_context = None;
                    }
                    let Some(instance) = self.instantiate_generic_method_call(
                        class,
                        &name,
                        call,
                        false,
                        fx,
                        method_pos.clone(),
                    ) else {
                        return self.err_expr(pos);
                    };
                    inferred = instance.arguments;
                    result_type = Some(instance.result);
                    instance.name
                } else {
                    name
                };
                let Some(mut sig) = self.class_sigs[class.0].methods.get(&name).cloned() else {
                    let class_name = self.classes[class.0].name.clone();
                    self.reject_subset(
                        if super::is_object_member(&name) {
                            RejectionSite::AwaitClassObjectMethod
                        } else {
                            RejectionSite::AwaitClassUndeclaredMethod
                        },
                        format!("`{class_name}` has no method `{name}`"),
                        method_pos,
                    );
                    return self.err_expr(pos);
                };
                // §234 rule 1: a synchronous method whose instantiated result
                // is a handle is an ordinary call.
                sig.ret = result_type.unwrap_or(sig.ret);
                sig = self.member_signature(class, sig, fx);
                if !sig.is_async && matches!(self.apparent_type(&sig.ret), Type::AsyncHandle(_)) {
                    if !generic && call.type_arguments.is_some() {
                        self.reject_subset(
                            RejectionSite::AwaitMethodTypeArguments,
                            format!("method `{name}` is not generic"),
                            method_pos,
                        );
                    }
                    let handle =
                        self.synchronous_method_call(receiver, name, &sig, call, inferred, fx);
                    return self.await_call_handle(handle, fx, pos);
                }
                if !sig.is_async {
                    self.reject_subset(
                        RejectionSite::AwaitSynchronousMethod,
                        format!("method `{name}` is synchronous and cannot be awaited"),
                        method_pos,
                    );
                    return self.err_expr(pos);
                }
                if !generic && call.type_arguments.is_some() {
                    self.reject_subset(
                        RejectionSite::AwaitMethodTypeArguments,
                        format!("method `{name}` is not generic"),
                        method_pos,
                    );
                }
                let args = self.check_args_with_arguments(
                    RejectionSite::AwaitFunctionArgumentCount,
                    &sig.params,
                    &call.arguments,
                    fx,
                    &pos,
                    &name,
                    inferred,
                    sig.generic,
                );
                hir::Expr {
                    pending_work: None,
                    kind: ExprKind::AsyncCall {
                        callee: AsyncCallee::Method {
                            class,
                            receiver: Box::new(receiver),
                            name: hir::Symbol::from_full_text(name),
                        },
                        args,
                    },
                    ty: sig.ret,
                    pos,
                }
            }
            _ => {
                self.reject_subset(
                    RejectionSite::AwaitIndirectCall,
                    "await requires a call that returns an async handle",
                    pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }
}

/// True when `e` holds an empty array literal at a position that the
/// context of `e` reaches (compiler.md §204 rule 3).
fn holds_empty_array_literal(e: &ast::Expression) -> bool {
    match e {
        ast::Expression::ArrayExpression(array) => {
            array.elements.is_empty()
                || array
                    .elements
                    .iter()
                    .filter_map(crate::check::array_element_expression)
                    .any(holds_empty_array_literal)
        }
        ast::Expression::ParenthesizedExpression(paren) => holds_empty_array_literal(&paren.expression),
        ast::Expression::ConditionalExpression(cond) => {
            holds_empty_array_literal(&cond.consequent) || holds_empty_array_literal(&cond.alternate)
        }
        ast::Expression::BinaryExpression(binary) => {
            holds_empty_array_literal(&binary.left) || holds_empty_array_literal(&binary.right)
        }
        ast::Expression::ObjectExpression(object) => object.properties.iter().any(|property| matches!(property, ast::ObjectPropertyKind::ObjectProperty(property) if !property.method && property.kind == ast::PropertyKind::Init && !property.shorthand && holds_empty_array_literal(&property.value))),
        ast::Expression::ArrowFunctionExpression(arrow) => arrow.body.as_expression().is_some_and(holds_empty_array_literal),
        ast::Expression::LogicalExpression(binary) => holds_empty_array_literal(&binary.left) || holds_empty_array_literal(&binary.right),
        _ => false,
    }
}

/// Whether the form of `e` reads a hint (compiler.md §249 rule 1a): a
/// literal; a binary or a unary operator, which gives its context to its
/// operands as it does for a contextual type; a descriptor literal; an
/// array literal; a conditional; a parenthesis; and a lambda, for its
/// result only. An operand call takes no contextual candidate (§212 rule
/// 1a).
fn reads_hint(e: &ast::Expression) -> bool {
    matches!(
        e,
        ast::Expression::NumericLiteral(_)
            | ast::Expression::StringLiteral(_)
            | ast::Expression::BooleanLiteral(_)
            | ast::Expression::NullLiteral(_)
            | ast::Expression::BigIntLiteral(_)
            | ast::Expression::RegExpLiteral(_)
            | ast::Expression::LogicalExpression(_)
            | ast::Expression::BinaryExpression(_)
            | ast::Expression::UnaryExpression(_)
            | ast::Expression::ObjectExpression(_)
            | ast::Expression::ArrayExpression(_)
            | ast::Expression::ConditionalExpression(_)
            | ast::Expression::ParenthesizedExpression(_)
            | ast::Expression::ArrowFunctionExpression(_)
    )
}

/// The kind of an expected type (compiler.md §249 rule 1). A contextual
/// type comes from a declaration: an annotation, a parameter, a result,
/// or an assignment target. A hint is the type of an earlier element of
/// an array literal with no contextual type (§212 rule 1a), or a context
/// that a hint gives to an inner expression. A hint types a literal, and
/// gives no parameter types to a lambda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextKind {
    /// A contextual type.
    Contextual,
    /// A hint.
    Hint,
    /// A context-free function-list element hint, whose conditional may
    /// defer placement until the whole list selects its type (§305).
    FunctionListHint,
}
