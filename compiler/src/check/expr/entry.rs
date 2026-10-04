//! Checks the expression entry points, statement expressions, and `await`.

use crate::check::rejection::RejectionSite;
use std::collections::HashSet;

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{Checker, FnCtx, ScopeItem};
use crate::diag::Pos;
use crate::hir::{self, AsyncCallee, ExprKind};
use crate::types::{ClassId, Type};

use super::contextual_object_class;

impl<'p> Checker<'p> {
    pub(in crate::check) fn check_truth_expr(
        &mut self,
        expression: &ast::Expr,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        match super::unparen_expr(expression) {
            ast::Expr::Member(member) => self.check_member_read_inner(member, fx, false, true),
            expression => self.check_expr(expression, None, fx),
        }
    }

    pub(in crate::check) fn ambient_visible(&self, name: &str, fx: &FnCtx) -> bool {
        !fx.owns_local_name(name) && self.peek_scope_item(name).is_none()
    }

    pub(in crate::check) fn ambient_namespace(
        &self,
        obj: &ast::Expr,
        fx: &FnCtx,
    ) -> Option<&'static str> {
        let ast::Expr::Ident(id) = obj else {
            return None;
        };
        let name = match id.sym.as_ref() {
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
    pub(crate) fn expr_async_origins(&self, expr: &hir::Expr, fx: &FnCtx) -> HashSet<u32> {
        use hir::ExprKind as K;
        match &expr.kind {
            K::AsyncHandleCreate { origin, .. } => HashSet::from([*origin]),
            K::AsyncHandleTransfer { origin, .. } => HashSet::from([*origin]),
            K::Local(name, _) => fx.local_async_origins(name),
            K::ArrayLit(elements) => elements
                .iter()
                .flat_map(|element| self.expr_async_origins(element, fx))
                .collect(),
            K::ArraySpreadLit(elements) => elements
                .iter()
                .flat_map(|element| self.expr_async_origins(&element.expr, fx))
                .collect(),
            K::Index { obj, .. } | K::Field { obj, .. } | K::Cast(obj) => {
                self.expr_async_origins(obj, fx)
            }
            K::Cond { then, els, .. } => self
                .expr_async_origins(then, fx)
                .into_iter()
                .chain(self.expr_async_origins(els, fx))
                .collect(),
            _ => HashSet::new(),
        }
    }

    pub(super) fn track_async_call_result(
        &mut self,
        value: hir::Expr,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        if !self.apparent_type(&value.ty).carries_async_handle() {
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
        self.reject_subset(
            RejectionSite::Api(rejection.id, rejection.divergence),
            message,
            pos,
        );
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
        e: &ast::Expr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        self.check_expr_with_header_receiver(e, ctx, fx, false)
    }

    pub(super) fn check_expr_with_header_receiver(
        &mut self,
        e: &ast::Expr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        allow_embedded_header_receiver: bool,
    ) -> hir::Expr {
        let previous_numeric = fx.descriptor_numeric_operand;
        if fx.descriptor_default.is_some() {
            if matches!(e, ast::Expr::Bin(b) if matches!(b.op,
                ast::BinaryOp::Add | ast::BinaryOp::Sub | ast::BinaryOp::Mul |
                ast::BinaryOp::Div | ast::BinaryOp::Mod | ast::BinaryOp::Exp |
                ast::BinaryOp::LShift | ast::BinaryOp::RShift | ast::BinaryOp::ZeroFillRShift |
                ast::BinaryOp::BitOr | ast::BinaryOp::BitXor | ast::BinaryOp::BitAnd))
                || matches!(e, ast::Expr::Unary(u) if matches!(u.op, ast::UnaryOp::Minus | ast::UnaryOp::Plus | ast::UnaryOp::Tilde))
            {
                fx.descriptor_numeric_operand = true;
            } else if matches!(e, ast::Expr::Call(_) | ast::Expr::Arrow(_)) {
                fx.descriptor_numeric_operand = false;
            }
        }
        let checked = self.check_expr_inner(e, ctx, fx, allow_embedded_header_receiver);
        fx.descriptor_numeric_operand = previous_numeric;
        if self.apparent_type(&checked.ty) == Type::Void {
            self.reject_subset(
                RejectionSite::VoidExpressionValue,
                "a `void` expression is only allowed as an expression statement",
                checked.pos.clone(),
            );
            self.err_expr(checked.pos)
        } else {
            checked
        }
    }

    fn check_expr_inner(
        &mut self,
        e: &ast::Expr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        allow_embedded_header_receiver: bool,
    ) -> hir::Expr {
        self.expression_work.push(None);
        let mut checked =
            self.check_expr_inner_untracked(e, ctx, fx, allow_embedded_header_receiver);
        if let Some(work) = self.expression_work.pop().flatten() {
            checked.pending_work = Some(work);
        }
        checked
    }

    fn check_expr_inner_untracked(
        &mut self,
        e: &ast::Expr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        allow_embedded_header_receiver: bool,
    ) -> hir::Expr {
        let pos = self.pos(e.span());
        let mut checked = match e {
            ast::Expr::Paren(p) => self.check_expr_with_header_receiver(
                &p.expr,
                ctx,
                fx,
                allow_embedded_header_receiver,
            ),
            ast::Expr::Lit(lit) => self.check_lit(lit, ctx, pos),
            ast::Expr::Tpl(tpl) => self.check_template(tpl, fx, pos),
            ast::Expr::Ident(id) => self.check_ident(id, ctx, fx),
            ast::Expr::This(_) => {
                if fx.descriptor_default.is_some() {
                    self.reject_subset(
                        RejectionSite::DescriptorDefaultThisUse,
                        "§147 rule 3a: `this` is forbidden in a descriptor member default",
                        pos.clone(),
                    );
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
                        return self.err_expr(pos);
                    }
                    if let Some(Type::Class(id)) = this_ty.as_ref().map(|ty| self.apparent_type(ty))
                    {
                        if self.classes[id.0].is_value {
                            self.reject_subset(
                                    RejectionSite::ThisInValueTypeArrow,
                                    "a lambda cannot capture a ValueType receiver; the capture is a copy",
                                    pos.clone(),
                                );
                            return self.err_expr(pos);
                        }
                    }
                    if this_ty.is_some() {
                        if let Some(local) = self.lookup_local("this", &pos, fx) {
                            return hir::Expr {
                                pending_work: None,
                                kind: ExprKind::Local("this".to_string(), local.ty.clone()),
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
                            "`this` is only available in constructors and methods",
                            pos.clone(),
                        );
                        self.err_expr(pos)
                    }
                }
            }
            ast::Expr::Unary(u) => self.check_unary(u, ctx, fx, pos),
            ast::Expr::Update(u) => self.check_update(u, fx, pos, false, None),
            ast::Expr::Bin(b) => self.check_bin(b, ctx, fx, pos),
            ast::Expr::Assign(a) => self.check_assign(a, fx, pos, false, None),
            ast::Expr::Member(m) => self.check_member_read(m, fx),
            ast::Expr::OptChain(chain) => self.reject_unbound_optional_chain(chain, fx, pos),
            ast::Expr::Cond(c) => self.check_cond(c, ctx, fx, pos),
            ast::Expr::Call(c) => self.check_call(c, ctx, fx, pos),
            ast::Expr::New(n) => self.check_new(n, ctx, fx, pos),
            ast::Expr::Arrow(a) => self.check_lambda(a, ctx, fx, pos),
            ast::Expr::Array(a) => self.check_array_lit(a, ctx, fx, pos),
            ast::Expr::Object(object) => {
                match contextual_object_class(ctx, |ty| self.apparent_type(ty)) {
                    Some(id) if self.classes[id.0].is_descriptor => {
                        self.check_descriptor_lit(object, id, fx, pos)
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
            ast::Expr::TsAs(a) => self.check_as(a, fx, pos),
            ast::Expr::Yield(y) => self.check_yield(y, fx, pos),
            ast::Expr::Await(a) => self.check_await(a, fx, pos),
            ast::Expr::TsNonNull(t) => {
                let p = self.pos(t.span);
                self.reject_subset(
                    RejectionSite::NonNullAssertionExpression,
                    "the `!` assertion is not in the decided surface; narrow with a null check",
                    p.clone(),
                );
                self.err_expr(p)
            }
            ast::Expr::Fn(_) => {
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
                        ast::Expr::TsTypeAssertion(_) => RejectionSite::AngleAssertionExpression,
                        ast::Expr::TsSatisfies(_) => RejectionSite::SatisfiesExpression,
                        ast::Expr::TsInstantiation(_) => RejectionSite::InstantiationExpression,
                        ast::Expr::Seq(_) => RejectionSite::CommaExpression,
                        ast::Expr::TaggedTpl(_) => RejectionSite::TaggedTemplateExpression,
                        ast::Expr::Class(_) => RejectionSite::ClassExpression,
                        ast::Expr::MetaProp(_) => RejectionSite::MetaPropertyExpression,
                        ast::Expr::PrivateName(_) => RejectionSite::PrivateNameExpression,
                        ast::Expr::TsConstAssertion(_) => RejectionSite::ConstAssertionExpression,
                        _ => RejectionSite::UnsupportedExpressionKind,
                    },
                    "expression form outside the decided surface",
                    p.clone(),
                );
                self.err_expr(p)
            }
        };
        self.end_shared_narrowing(&checked, fx);
        if !allow_embedded_header_receiver {
            self.reject_embedded_header_copy(&mut checked, ctx);
        }
        checked
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
    pub(crate) fn check_expr_stmt(&mut self, e: &ast::Expr, fx: &mut FnCtx) -> Vec<hir::Stmt> {
        let mut root = e;
        while let ast::Expr::Paren(paren) = root {
            root = &paren.expr;
        }
        if let ast::Expr::Call(call) = root {
            if let ast::Callee::Expr(callee) = &call.callee {
                let mut callee: &ast::Expr = callee;
                while let ast::Expr::Paren(paren) = callee {
                    callee = &paren.expr;
                }
                if let ast::Expr::Ident(ident) = callee {
                    if ident.sym.as_ref() == "unreachable" {
                        let pos = self.pos(call.span);
                        let checked = self.check_named_call(ident, call, fx, pos, true);
                        self.end_shared_narrowing(&checked, fx);
                        return vec![hir::Stmt::Expr(checked)];
                    }
                }
            }
        }
        if let ast::Expr::OptChain(chain) = root {
            return self.check_optional_chain_statement(chain, fx);
        }
        let mut prefix = crate::check::SyntheticPrefix::default();
        if let ast::Expr::Assign(assign) = root {
            let checked =
                self.check_assign(assign, fx, self.pos(root.span()), true, Some(&mut prefix));
            self.end_shared_narrowing(&checked, fx);
            let mut out = Vec::new();
            out.extend(prefix);
            out.push(hir::Stmt::Expr(checked));
            return out;
        }
        if let ast::Expr::Update(update) = root {
            let checked =
                self.check_update(update, fx, self.pos(root.span()), true, Some(&mut prefix));
            self.end_shared_narrowing(&checked, fx);
            let mut out = Vec::new();
            out.extend(prefix);
            out.push(hir::Stmt::Expr(checked));
            return out;
        }
        let checked = self.check_expr_inner(root, None, fx, false);
        let mut statements = prefix.into_statements();
        statements.push(hir::Stmt::Expr(checked));
        statements
    }

    /// Checks the three awaitable forms (§26, §37). The AST call is handled
    /// here instead of through the ordinary call path so an async call can
    /// never materialize a Promise-typed value in HIR.
    fn check_await(&mut self, awaited: &ast::AwaitExpr, fx: &mut FnCtx, pos: Pos) -> hir::Expr {
        if !fx.frames.last().is_some_and(|frame| frame.is_async) {
            self.reject_subset(
                RejectionSite::AwaitOutsideAsync,
                "`await` is only legal inside an async function",
                pos.clone(),
            );
            return self.err_expr(pos);
        }

        let mut operand: &ast::Expr = &awaited.arg;
        while let ast::Expr::Paren(paren) = operand {
            operand = &paren.expr;
        }
        let ast::Expr::Call(call) = operand else {
            let handle = self.check_expr(operand, None, fx);
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
        let ast::Callee::Expr(callee) = &call.callee else {
            self.reject_subset(
                RejectionSite::AwaitNotDirectCall,
                "awaitable expressions must be direct calls",
                pos.clone(),
            );
            return self.err_expr(pos);
        };
        let mut callee: &ast::Expr = callee;
        while let ast::Expr::Paren(paren) = callee {
            callee = &paren.expr;
        }

        if let ast::Expr::Member(member) = callee {
            if self.is_context_namespace(&member.obj, fx)
                && matches!(&member.prop, ast::MemberProp::Ident(prop) if prop.sym.as_ref() == "suspend")
            {
                if call.type_args.is_some() || !call.args.is_empty() {
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

        match callee {
            ast::Expr::Ident(ident) => {
                let name = ident.sym.to_string();
                if fx.owns_local_name(&name) {
                    let ident_pos = self.pos(ident.span);
                    if self
                        .lookup_local(&name, &ident_pos, fx)
                        .is_some_and(|local| matches!(self.apparent_type(&local.ty), Type::Error))
                    {
                        return self.err_expr(pos);
                    }
                    self.reject_subset(
                        RejectionSite::AwaitLocalCall,
                        "an async awaitable cannot be called through a local value",
                        self.pos(ident.span),
                    );
                    return self.err_expr(pos);
                }
                let callee_pos = self.pos(ident.span);
                let item = self.scope_item(&name, &callee_pos);
                if matches!(item, Some(ScopeItem::Poisoned))
                    || matches!(&item, Some(ScopeItem::Global(g)) if self.global_sigs.get(g).is_some_and(|s| matches!(s.state, crate::check::initializer::TypeState::Rejected)))
                {
                    self.check_poisoned_arguments(&call.args, fx);
                    return self.err_expr(pos);
                }
                let mut checked = None;
                let (function, checked_name, rejects_type_args) = match item {
                    Some(ScopeItem::Func(function)) => {
                        (function, name.clone(), call.type_args.is_some())
                    }
                    Some(ScopeItem::GenericFunc(key)) => {
                        let arguments = if let Some(type_args) = &call.type_args {
                            self.resolve_instance_arguments(type_args)
                        } else {
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
                let Some(sig) = self.fn_sigs.get(&function).cloned() else {
                    return self.err_expr(pos);
                };
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
                    &call.args,
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
            ast::Expr::Member(member) => {
                let ast::MemberProp::Ident(method) = &member.prop else {
                    self.reject_subset(
                        RejectionSite::AwaitComputedMethod,
                        "an awaited async method requires an identifier method name",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                };
                let name = method.sym.to_string();
                let method_pos = self.pos(method.span);
                if self.rejected_static_generic_method(&member.obj, &name, fx) {
                    return self.err_expr(pos);
                }
                let receiver = self.check_receiver(&member.obj, fx);
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
                let name = if generic {
                    let Some(instance) = self.instantiate_generic_method_call(
                        class,
                        &name,
                        call,
                        false,
                        method_pos.clone(),
                    ) else {
                        return self.err_expr(pos);
                    };
                    instance
                } else {
                    name
                };
                let Some(sig) = self.class_sigs[class.0].methods.get(&name).cloned() else {
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
                if !sig.is_async {
                    self.reject_subset(
                        RejectionSite::AwaitSynchronousMethod,
                        format!("method `{name}` is synchronous and cannot be awaited"),
                        method_pos,
                    );
                    return self.err_expr(pos);
                }
                if !generic && call.type_args.is_some() {
                    self.reject_subset(
                        RejectionSite::AwaitMethodTypeArguments,
                        format!("method `{name}` is not generic"),
                        method_pos,
                    );
                }
                let args = self.check_args_with_arguments(
                    RejectionSite::AwaitFunctionArgumentCount,
                    &sig.params,
                    &call.args,
                    fx,
                    &pos,
                    &name,
                    None,
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
                self.reject_subset(RejectionSite::AwaitIndirectCall, "an async awaitable must directly call a named async function or instance method", pos.clone());
                self.err_expr(pos)
            }
        }
    }
}
