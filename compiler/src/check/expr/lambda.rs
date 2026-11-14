//! Checks arrow function expressions (C5).

use crate::check::rejection::RejectionSite;
use std::collections::HashSet;

use swc_common::Spanned;
use swc_ecma_ast as ast;

use super::entry::ContextKind;
use crate::check::{Checker, FnCtx, Frame, Local, ParamSig, Scope};
use crate::diag::Pos;
use crate::hir::{self, ExprKind};
use crate::types::Type;

impl<'p> Checker<'p> {
    /// compiler.md §286 rule 3: each operand has type void, without context.
    /// §287 rule 1: an async lambda also accepts a Promise<void> operand.
    pub(in crate::check) fn check_inferred_void_return(
        &mut self,
        arg: &ast::Expr,
        fx: &mut FnCtx,
        pos: &Pos,
        out: &mut Vec<hir::Stmt>,
    ) {
        // compiler.md §286 rule 3b: reuse §237's return leaf split.
        if matches!(super::unparen_expr(arg), ast::Expr::Cond(_)) {
            self.split_return(arg, fx, out);
            return;
        }
        let (mut statements, ty) = self.check_void_operand(arg, fx);
        // compiler.md §287 rule 1: reuse annotated async return adoption.
        if fx.frames.last().is_some_and(|frame| frame.is_async)
            && matches!(self.apparent_type(&ty), Type::AsyncHandle(inner) if self.apparent_type(&inner) == Type::Void)
            && matches!(statements.last(), Some(hir::Stmt::Expr(_)))
        {
            if let Some(hir::Stmt::Expr(checked)) = statements.pop() {
                out.extend(statements);
                let _ = self.adopt_returned_handle(checked, &Type::Void, fx, pos, out);
                return;
            }
        }
        if !matches!(self.apparent_type(&ty), Type::Void | Type::Error) {
            let diagnostic_pos = fx.frames.last_mut().and_then(|frame| {
                let (pos, reported) = frame.inferred_void.as_mut()?;
                if *reported {
                    return None;
                }
                *reported = true;
                Some(pos.clone())
            });
            if let Some(pos) = diagnostic_pos {
                self.reject_subset(
                    RejectionSite::BlockLambdaReturnAnnotationMissing,
                    "a lambda with a block body requires a return type annotation",
                    pos,
                );
            }
            self.read_statement_handles(&statements, fx);
        }
        out.extend(statements);
        out.push(hir::Stmt::Return {
            value: None,
            pos: pos.clone(),
        });
    }

    pub(super) fn check_lambda(
        &mut self,
        a: &ast::ArrowExpr,
        (ctx, kind): (Option<&Type>, ContextKind),
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if ctx.is_some_and(|ty| self.apparent_type(ty) == Type::Error) {
            return self.err_expr(pos);
        }
        let ctx_fn = ctx.and_then(|ty| self.lambda_context(ty));
        let (param_ctx, ret_ctx) = match &ctx_fn {
            Some(ft) => (Some(ft.params.as_slice()), Some(&ft.ret)),
            None => (None, None),
        };
        // compiler.md §249 rule 1: a hint gives no parameter types, and the
        // result context that it gives is a hint too.
        let param_ctx = param_ctx.filter(|_| kind == ContextKind::Contextual);
        self.check_lambda_with(a, param_ctx, (ret_ctx, kind), None, fx, pos)
    }

    /// The function type that the contextual type `ctx` gives a lambda: a
    /// function type, or the function type `F` of `F | null` (compiler.md
    /// §251 rule 1). Another type gives no function type.
    pub(crate) fn lambda_context(&self, ctx: &Type) -> Option<crate::types::FuncType> {
        match self.apparent_type(ctx) {
            Type::Func(function) => Some(*function),
            Type::Nullable(inner, crate::types::Absence::Null) => {
                match self.apparent_type(&inner) {
                    Type::Func(function) => Some(*function),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// [`Self::check_lambda`] with the contextual function type split
    /// into its two halves, so a caller can supply parameter context
    /// while leaving the return type to be inferred from the body (the
    /// `map` callback, stdlib.md §9: `U` comes from the closure).
    pub(super) fn check_lambda_with(
        &mut self,
        a: &ast::ArrowExpr,
        param_ctx: Option<&[Type]>,
        (ret_ctx, ret_kind): (Option<&Type>, ContextKind),
        resolved_first: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let generic_context = self.generic_callback_context;
        let result_hint = self.lambda_result_hint.take();
        let void_context = std::mem::take(&mut self.lambda_void_context);
        let called_at_once = std::mem::take(&mut self.lambda_called_at_once);
        let value = self.with_expression_work(|checker| {
            if a.is_async && a.type_params.is_some() {
                checker.reject_subset(
                    RejectionSite::AsyncArrowFunction,
                    "generic async arrows are not in the decided surface",
                    pos.clone(),
                );
                return checker.err_expr(pos);
            }
            if a.is_generator {
                checker.reject_subset(
                    RejectionSite::GeneratorArrowFunction,
                    "generator arrows are not in the decided surface",
                    pos.clone(),
                );
                return checker.err_expr(pos);
            }
            let saved_group_parameters = checker.task_group_parameters;
            checker.task_group_parameters =
                !a.is_async && !a.is_generator && !fx.frames.iter().any(|frame| frame.is_generator);
            let mut params = Vec::new();
            for (i, pat) in a.params.iter().enumerate() {
                // An un-annotated lambda parameter takes its type from the
                // contextual function type (tsc-style contextual typing);
                // only a parameter with neither annotation nor context is an
                // error. This is how a boundary callback (e.g. a `void*`
                // `object | null` userdata slot) is typed without the program
                // spelling the boundary type itself.
                let unannotated_ident = match pat {
                    ast::Pat::Ident(b) if b.type_ann.is_none() && !b.id.optional => Some(b),
                    _ => None,
                };
                let sig = if let (0, Some(resolved), ast::Pat::Ident(binding)) =
                    (i, resolved_first, pat)
                {
                    ParamSig {
                        annotated: binding
                            .type_ann
                            .as_ref()
                            .is_some_and(|ann| checker.written_type(&ann.type_ann)),
                        name: binding.id.sym.to_string(),
                        state: crate::check::initializer::TypeState::decided(resolved.clone()),
                        initializer: None,
                        has_default: false,
                    }
                } else if let Some(b) = unannotated_ident {
                    if let Some(t) = param_ctx.and_then(|p| p.get(i)) {
                        ParamSig {
                            annotated: false,
                            name: b.id.sym.to_string(),
                            state: crate::check::initializer::TypeState::decided(t.clone()),
                            initializer: None,
                            has_default: false,
                        }
                    } else {
                        checker.resolve_param_pat(pat)
                    }
                } else {
                    checker.resolve_param_pat(pat)
                };
                params.push(sig);
            }
            checker.task_group_parameters = saved_group_parameters;
            let mut ret = a
                .return_type
                .as_ref()
                .map(|ann| checker.resolve_result_type(&ann.type_ann))
                .or_else(|| {
                    ret_ctx
                        .filter(|ty| {
                            !a.is_async
                                || matches!(
                                    checker.apparent_type(ty),
                                    Type::AsyncHandle(_) | Type::Error
                                )
                        })
                        .cloned()
                })
                // compiler.md §238 rule 2: a `void` result of the parameter
                // type of a generic call is the context of the result.
                .or_else(|| (void_context && !a.is_async).then_some(Type::Void));

            if a.is_async
                && ret.as_ref().is_some_and(|ty| {
                    !matches!(
                        checker.apparent_type(ty),
                        Type::AsyncHandle(_) | Type::Error
                    )
                })
            {
                checker.reject_subset(
                    RejectionSite::AsyncArrowResultAnnotation,
                    "an async arrow result annotation must be Promise<T>",
                    pos.clone(),
                );
                return checker.err_expr(pos);
            }
            // compiler.md §286 rules 1 to 3: decide a block result before
            // checking its operands without context.
            let mut void_block =
                ret.is_none() && matches!(&*a.body, ast::BlockStmtOrExpr::BlockStmt(_));
            // compiler.md §286 rule 3a: reject before publishing a result,
            // including when the body would otherwise be deferred.
            if void_block {
                if let ast::BlockStmtOrExpr::BlockStmt(block) = &*a.body {
                    if returns_nonvoid_syntax(&block.stmts) {
                        checker.reject_subset(
                            RejectionSite::BlockLambdaReturnAnnotationMissing,
                            "a lambda with a block body requires a return type annotation",
                            pos.clone(),
                        );
                        ret = Some(Type::Error);
                        void_block = false;
                    }
                }
            }
            // compiler.md §239 rule 1: an annotation and a `void` block each
            // decide the result from the syntax, before the body is checked.
            let decided_from_syntax = a.return_type.is_some() || void_block;
            if void_block {
                ret = Some(if a.is_async {
                    Type::async_handle(Type::Void)
                } else {
                    Type::Void
                });
            }
            checker.generic_callback_context = false;
            checker.decide_lambda_parameters(&mut params, evaluation_of(called_at_once), fx);
            if checker.deciding_type
                && decided_from_syntax
                && ret.as_ref().is_some_and(|t| *t != Type::Error)
            {
                let result = ret.clone().unwrap_or(Type::Error);
                let ty = Type::func(
                    params.iter().map(|p| p.ty().clone()).collect(),
                    result.clone(),
                );
                checker.defer_work(
                    crate::check::initializer::DeferredWork::Lambda {
                        source: a.clone(),
                        inferred_void: void_block,
                        params,
                        result: result.clone(),
                        called_at_once,
                    },
                    fx,
                );
                let id = hir::LambdaId(checker.next_lambda_id);
                checker.next_lambda_id += 1;
                return hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Lambda {
                        id,
                        params: Vec::new(),
                        is_async: a.is_async,
                        owns_environment: a.is_async,
                        ret: if a.is_async {
                            match checker.apparent_type(&result) {
                                Type::AsyncHandle(t) => *t,
                                _ => Type::Error,
                            }
                        } else {
                            result
                        },
                        body: Vec::new(),
                        captures: Vec::new(),
                        can_raise: false,
                    },
                    ty,
                    pos,
                };
            }
            // The body checks with the fact that the lambda is called at
            // once (compiler.md §241 rule 2).
            checker.lambda_called_at_once = called_at_once;
            // The result takes the kind of its context; an annotation or a
            // rule of the body gives a contextual type.
            let result_kind = if a.return_type.is_none() && ret.is_some() && ret.as_ref() == ret_ctx
            {
                ret_kind
            } else {
                ContextKind::Contextual
            };
            checker.check_lambda_body(
                a,
                params,
                (ret, result_kind, void_block),
                result_hint.as_ref(),
                fx,
                pos,
            )
        });
        self.generic_callback_context = generic_context;
        value
    }

    pub(in crate::check) fn check_lambda_body(
        &mut self,
        a: &ast::ArrowExpr,
        params: Vec<ParamSig>,
        (mut ret, result_kind, inferred_void): (Option<Type>, ContextKind, bool),
        result_hint: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if a.is_async {
            ret = ret.map(|ty| match self.apparent_type(&ty) {
                Type::AsyncHandle(value) => *value,
                _ => Type::Error,
            });
        }
        let self_call = fx.lambda_initializer.as_ref().and_then(|(span, name)| {
            let syntax_result =
                a.return_type.is_some() || matches!(&*a.body, ast::BlockStmtOrExpr::BlockStmt(_));
            // compiler.md §285 rule 1: the const annotation also decides
            // the result before the body. A result hint does not.
            let decided_result = syntax_result || result_kind == ContextKind::Contextual;
            (*span == a.span && decided_result && ret.is_some()).then(|| {
                let result = ret.clone().unwrap_or(Type::Error);
                (
                    name.clone(),
                    crate::types::FuncType {
                        params: params.iter().map(|p| p.ty().clone()).collect(),
                        ret: if a.is_async {
                            Type::async_handle(result)
                        } else {
                            result
                        },
                        callback_typedef: None,
                    },
                )
            })
        });
        let origin_start = fx.async_origins.len();
        let id = hir::LambdaId(self.next_lambda_id);
        self.next_lambda_id += 1;
        let frame = Frame {
            ret: ret.clone().unwrap_or(Type::Error),
            result_kind,
            is_generator: false,
            is_async: a.is_async,
            yield_ty: None,
            yield_annotated: false,
            is_lambda: true,
            contextual_void: !a.is_async && a.return_type.is_none() && ret == Some(Type::Void),
            inferred_void: inferred_void.then(|| (pos.clone(), false)),
            rejects_return_operand: false,
            void_split: None,
            deferred: evaluation_of(std::mem::take(&mut self.lambda_called_at_once))
                == crate::check::Evaluation::Deferred,
            lambda_id: Some(id),
            captures: Vec::new().into(),
            this_ty: None,
            missing_this_site: fx.frames.last().and_then(|frame| frame.missing_this_site),
            static_this_class: fx.frames.last().and_then(|frame| frame.static_this_class),
            super_call_available: false,
        };
        fx.frames.push(frame);
        fx.scopes.push(Scope {
            fn_boundary: true,
            ..Default::default()
        });
        let saved_narrowed = std::mem::take(&mut fx.narrowed);
        fx.narrowed = saved_narrowed
            .iter()
            .filter(|fact| {
                fact.narrows_type()
                    && !fact.contains('.')
                    && !fact.starts_with("[[global]]")
                    && fx
                        .scopes
                        .iter()
                        .rev()
                        .find_map(|scope| scope.vars.get(&fact.key))
                        .is_some_and(|local| !local.mutable)
            })
            .cloned()
            .collect();
        let saved_notes = std::mem::take(&mut fx.ended_shared_narrowing);
        let saved_loops = std::mem::take(&mut fx.loop_break_facts);
        let saved_switches = std::mem::take(&mut fx.switch_break_facts);
        let saved_depth = std::mem::replace(&mut fx.loop_depth, 0);
        let saved_adoptions = std::mem::take(&mut fx.adoptions);
        let saved_switch_depth = std::mem::replace(&mut fx.switch_depth, 0);
        let saved_reachable = std::mem::replace(&mut fx.flow_reachable, true);
        let saved_default = std::mem::replace(&mut fx.parameter_default, true);
        let mut hir_params = Vec::new();
        for (p, pattern) in params.iter().zip(&a.params) {
            if let Some(frame) = fx.frames.last_mut() {
                for capture in Self::initializer_captures(&p.initializer) {
                    if !frame.captures.iter().any(|c| c.name == capture.name) {
                        frame.captures.push(capture.clone());
                    }
                }
            }
            let mut initializer = p.initializer.clone();
            let default = if let Some(value) = self.finish_initializer(&mut initializer) {
                Some(value)
            } else if let ast::Pat::Assign(assign) = pattern {
                let mut value = self.check_expr(&assign.right, Some(p.ty()), fx);
                self.require_expr_assignable(&mut value, p.ty(), fx, "the default value");
                self.discharge_parameter_default(&value, p.ty(), fx);
                Some(value)
            } else {
                None
            };
            let param_pos = self.pos(pattern.span());
            let async_origins = if self.apparent_type(p.ty()).holds_async_handle() {
                HashSet::from([fx.register_async_origin(param_pos.clone())])
            } else {
                HashSet::new()
            };
            self.declare_local(
                &p.name,
                Local {
                    annotated: p.annotated,
                    ty: p.ty().clone(),
                    mutable: true,
                    async_origins,
                    caught: false,
                    function_value_required: None,
                },
                param_pos,
                fx,
            );
            hir_params.push(hir::Param {
                escape_kind: crate::hir::EscapeKind::None,
                default_can_raise: false,
                name: p.name.clone(),
                ty: p.ty().clone(),
                default,
                foreign_provenance: None,
                pos: pos.clone(),
            });
        }
        fx.parameter_default = saved_default;
        let parameter_patterns = params
            .iter()
            .cloned()
            .zip(a.params.iter().cloned())
            .collect::<Vec<_>>();
        let entry = self.bind_parameter_patterns(parameter_patterns, fx);
        if let Some((name, signature)) = self_call {
            if let Some(scope) = fx.scopes.last_mut() {
                scope.self_call = Some((name, id, signature, a.is_async));
            }
        }
        let body_pos = self.pos(a.body.span());
        let (body, prefix) = fx.with_synthetic_owner(
            crate::check::SyntheticOwnerKind::ArrowBody(body_pos),
            |fx| match &*a.body {
                ast::BlockStmtOrExpr::Expr(e) => {
                    // compiler.md §236 rule 3: an unannotated body whose
                    // leaves mix a handle and a value takes the value type
                    // of its handle leaf, and splits as a return does.
                    if a.is_async && ret.is_none() {
                        if let Some(value) = self.mixed_body_result(e, fx) {
                            if let Some(frame) = fx.frames.last_mut() {
                                frame.ret = value.clone();
                            }
                            ret = Some(value);
                        }
                    }
                    // compiler.md §237 rules 2 and 4: a body of a `void`
                    // result is a `return` of the body.
                    let void_result = ret
                        .as_ref()
                        .is_some_and(|ty| self.apparent_type(ty) == Type::Void);
                    if (a.is_async && ret.is_some()) || void_result {
                        let mut out = Vec::new();
                        self.check_return(
                            &ast::ReturnStmt {
                                span: e.span(),
                                arg: Some(e.clone()),
                            },
                            fx,
                            &mut out,
                        );
                        return out;
                    }
                    let diagnostics = self.diags.len();
                    let mut checked = match &ret {
                        Some(ret) => self.check_expr_as(e, Some(ret), result_kind, fx),
                        None => self.check_expr_keeping_void(e, result_hint, fx),
                    };
                    // compiler.md §238 rule 1: a body of type `void` gives
                    // the result `void`.
                    if ret.is_none() && self.apparent_type(&checked.ty) == Type::Void {
                        ret = Some(Type::Void);
                        if let Some(frame) = fx.frames.last_mut() {
                            frame.ret = Type::Void;
                        }
                        return self.void_leaf_statements(checked);
                    }
                    if let Some(ret) = &ret {
                        self.require_expr_assignable(
                            &mut checked,
                            &ret.clone(),
                            fx,
                            "the lambda body",
                        );
                    } else {
                        ret = Some(checked.ty.clone());
                        if a.is_async {
                            let expected = match self.apparent_type(&checked.ty) {
                                Type::AsyncHandle(inner) => *inner,
                                _ => checked.ty.clone(),
                            };
                            ret = Some(expected.clone());
                            // compiler.md §236 rule 1: a handle body adopts
                            // the handle.
                            let mut out = Vec::new();
                            let body_pos = checked.pos.clone();
                            match self
                                .adopt_returned_handle(checked, &expected, fx, &body_pos, &mut out)
                            {
                                None => return out,
                                Some(value) => checked = value,
                            }
                            self.require_return_assignable(&mut checked, &expected, fx);
                        }
                    }
                    // A rejected body reads its handles, so it gives no
                    // second diagnostic.
                    if self.transfers_async_origins(&checked.ty) || self.diags.len() != diagnostics
                    {
                        let origins = self.expr_async_origins(&checked, fx);
                        fx.handle_async_origins(&origins);
                    }
                    let value_pos = checked.pos.clone();
                    vec![hir::Stmt::Return {
                        value: Some(checked),
                        pos: value_pos,
                    }]
                }
                ast::BlockStmtOrExpr::BlockStmt(block) => {
                    self.reserve_block_declarations(&block.stmts, fx);
                    let mut out = Vec::new();
                    for s in &block.stmts {
                        self.check_stmt(s, fx, &mut out);
                    }
                    if let Some(ret) = &ret {
                        self.end_tail_enum_switches(ret, &mut out);
                        if !matches!(&self.apparent_type(ret), Type::Void | Type::Error)
                            && crate::check::fallthrough::sequence_can_fall_through(&out)
                        {
                            self.reject_subset(
                                if self.ts_return_coverage(&out) {
                                    RejectionSite::LambdaReturnCoverage
                                } else {
                                    RejectionSite::LambdaReturnPathMissing
                                },
                                "not all paths return a value",
                                pos.clone(),
                            );
                        }
                    }
                    out
                }
            },
        );
        let mut statements = entry;
        statements.extend(prefix.into_statements());
        statements.extend(body);
        let body = statements;
        let body = if crate::check::has_dispose_binding(&body) {
            crate::check::using_scope::structure(body, &mut self.next_using_switch_id)
        } else {
            body
        };
        let adoptions = std::mem::replace(&mut fx.adoptions, saved_adoptions);
        let body = self.adoption_epilogue(body, adoptions, &pos);
        fx.narrowed = saved_narrowed;
        fx.ended_shared_narrowing = saved_notes;
        fx.loop_break_facts = saved_loops;
        fx.switch_break_facts = saved_switches;
        fx.loop_depth = saved_depth;
        fx.switch_depth = saved_switch_depth;
        fx.flow_reachable = saved_reachable;
        fx.scopes.pop();
        fx.shadowed_narrowing_scopes.remove(&fx.scopes.len());
        let frame = fx.frames.pop();
        if frame.as_ref().is_some_and(|frame| {
            frame
                .inferred_void
                .as_ref()
                .is_some_and(|(_, reported)| *reported)
        }) {
            ret = Some(Type::Error);
        }
        let captures = frame.map(|f| f.captures.into_inner()).unwrap_or_default();
        self.report_unhandled_async_origins(fx, origin_start);
        fx.async_origins.truncate(origin_start);
        fx.group_origins.retain(|id| (*id as usize) < origin_start);
        let ret = ret.unwrap_or(Type::Error);
        let result = if a.is_async {
            Type::AsyncHandle(Box::new(ret.clone()))
        } else {
            ret.clone()
        };
        let ty = Type::func(params.iter().map(|p| p.ty().clone()).collect(), result);
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Lambda {
                id,
                params: hir_params,
                is_async: a.is_async,
                owns_environment: a.is_async,
                ret,
                body,
                captures,
                can_raise: false,
            },
            ty,
            pos,
        }
    }
}

/// The evaluation of a lambda body: at once when the lambda is the callee
/// of a call (compiler.md §241 rule 2).
fn evaluation_of(called_at_once: bool) -> crate::check::Evaluation {
    if called_at_once {
        crate::check::Evaluation::AtOnce
    } else {
        crate::check::Evaluation::Deferred
    }
}

/// compiler.md §286 rule 3a: inspect this block's returns only.
fn returns_nonvoid_syntax(statements: &[ast::Stmt]) -> bool {
    statements.iter().any(|statement| match statement {
        ast::Stmt::Return(r) => r.arg.as_deref().is_some_and(nonvoid_return_syntax),
        ast::Stmt::Block(block) => returns_nonvoid_syntax(&block.stmts),
        ast::Stmt::If(i) => {
            returns_nonvoid_syntax(std::slice::from_ref(&i.cons))
                || i.alt
                    .as_deref()
                    .is_some_and(|alt| returns_nonvoid_syntax(std::slice::from_ref(alt)))
        }
        ast::Stmt::While(w) => returns_nonvoid_syntax(std::slice::from_ref(&w.body)),
        ast::Stmt::DoWhile(d) => returns_nonvoid_syntax(std::slice::from_ref(&d.body)),
        ast::Stmt::For(f) => returns_nonvoid_syntax(std::slice::from_ref(&f.body)),
        ast::Stmt::ForIn(f) => returns_nonvoid_syntax(std::slice::from_ref(&f.body)),
        ast::Stmt::ForOf(f) => returns_nonvoid_syntax(std::slice::from_ref(&f.body)),
        ast::Stmt::Labeled(l) => returns_nonvoid_syntax(std::slice::from_ref(&l.body)),
        ast::Stmt::Switch(s) => s
            .cases
            .iter()
            .any(|case| returns_nonvoid_syntax(&case.cons)),
        ast::Stmt::Try(t) => {
            returns_nonvoid_syntax(&t.block.stmts)
                || t.handler
                    .as_ref()
                    .is_some_and(|handler| returns_nonvoid_syntax(&handler.body.stmts))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|finalizer| returns_nonvoid_syntax(&finalizer.stmts))
        }
        _ => false,
    })
}

/// compiler.md §286 rule 3a: parentheses and conditionals expose leaves.
fn nonvoid_return_syntax(expr: &ast::Expr) -> bool {
    match super::unparen_expr(expr) {
        ast::Expr::Cond(c) => nonvoid_return_syntax(&c.cons) || nonvoid_return_syntax(&c.alt),
        ast::Expr::Ident(id) => id.sym == "undefined",
        ast::Expr::Lit(_)
        | ast::Expr::Tpl(_)
        | ast::Expr::Array(_)
        | ast::Expr::Object(_)
        | ast::Expr::Unary(_)
        | ast::Expr::Bin(_)
        | ast::Expr::Update(_)
        | ast::Expr::New(_)
        | ast::Expr::This(_)
        | ast::Expr::Arrow(_)
        | ast::Expr::Fn(_) => true,
        _ => false,
    }
}
