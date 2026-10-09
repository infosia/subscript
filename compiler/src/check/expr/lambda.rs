//! Checks arrow function expressions (C5).

use crate::check::rejection::RejectionSite;
use std::collections::HashSet;

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{Checker, FnCtx, Frame, Local, ParamSig, Scope};
use crate::diag::Pos;
use crate::hir::{self, ExprKind};
use crate::types::Type;

impl<'p> Checker<'p> {
    pub(super) fn check_lambda(
        &mut self,
        a: &ast::ArrowExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if ctx.is_some_and(|ty| self.apparent_type(ty) == Type::Error) {
            return self.err_expr(pos);
        }
        let ctx_fn = match ctx.map(|ty| self.apparent_type(ty)).as_ref() {
            Some(Type::Func(ft)) => Some((**ft).clone()),
            _ => None,
        };
        let (param_ctx, ret_ctx) = match &ctx_fn {
            Some(ft) => (Some(ft.params.as_slice()), Some(&ft.ret)),
            None => (None, None),
        };
        self.check_lambda_with(a, param_ctx, ret_ctx, None, fx, pos)
    }

    /// [`Self::check_lambda`] with the contextual function type split
    /// into its two halves, so a caller can supply parameter context
    /// while leaving the return type to be inferred from the body (the
    /// `map` callback, stdlib.md §9: `U` comes from the closure).
    pub(super) fn check_lambda_with(
        &mut self,
        a: &ast::ArrowExpr,
        param_ctx: Option<&[Type]>,
        ret_ctx: Option<&Type>,
        resolved_first: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let generic_context = self.generic_callback_context;
        let result_hint = self.lambda_result_hint.take();
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
                });

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
            if ret.is_none() && matches!(&*a.body, ast::BlockStmtOrExpr::BlockStmt(_)) {
                checker.reject_subset(
                    RejectionSite::BlockLambdaReturnAnnotationMissing,
                    "a lambda with a block body requires a return type annotation",
                    pos.clone(),
                );
                ret = Some(Type::Error);
            }
            checker.generic_callback_context = false;
            checker.decide_lambda_parameters(&mut params, fx);
            if checker.deciding_type
                && a.return_type.is_some()
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
                        params,
                        result: result.clone(),
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
            checker.check_lambda_body(a, params, ret, result_hint.as_ref(), fx, pos)
        });
        self.generic_callback_context = generic_context;
        value
    }

    pub(in crate::check) fn check_lambda_body(
        &mut self,
        a: &ast::ArrowExpr,
        params: Vec<ParamSig>,
        mut ret: Option<Type>,
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
        let origin_start = fx.async_origins.len();
        let id = hir::LambdaId(self.next_lambda_id);
        self.next_lambda_id += 1;
        let frame = Frame {
            ret: ret.clone().unwrap_or(Type::Error),
            is_generator: false,
            is_async: a.is_async,
            yield_ty: None,
            yield_annotated: false,
            is_lambda: true,
            contextual_void: a.return_type.is_none() && ret == Some(Type::Void),
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
                let value = self.check_expr(&assign.right, Some(p.ty()), fx);
                self.require_expr_assignable(&value, p.ty(), fx, "the default value");
                Some(value)
            } else {
                None
            };
            let param_pos = self.pos(pattern.span());
            self.declare_local(
                &p.name,
                Local {
                    annotated: p.annotated,
                    ty: p.ty().clone(),
                    mutable: true,
                    async_origins: HashSet::new(),
                    caught: false,
                    function_value_required: None,
                },
                param_pos,
                fx,
            );
            hir_params.push(hir::Param {
                escapes: false,
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
        let body_pos = self.pos(a.body.span());
        let (body, prefix) = fx.with_synthetic_owner(
            crate::check::SyntheticOwnerKind::ArrowBody(body_pos),
            |fx| match &*a.body {
                ast::BlockStmtOrExpr::Expr(e) => {
                    if a.is_async && ret.is_some() {
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
                    let checked = self.check_expr(e, ret.as_ref().or(result_hint), fx);
                    if let Some(ret) = &ret {
                        self.require_expr_assignable(&checked, &ret.clone(), fx, "the lambda body");
                    } else {
                        ret = Some(checked.ty.clone());
                        if a.is_async {
                            let expected = match self.apparent_type(&checked.ty) {
                                Type::AsyncHandle(inner) => *inner,
                                _ => checked.ty.clone(),
                            };
                            self.require_return_assignable(&checked, &expected, fx);
                            ret = Some(expected);
                        }
                    }
                    if matches!(
                        self.apparent_type(&checked.ty),
                        Type::AsyncHandle(_) | Type::Array(_)
                    ) {
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
        let captures = frame.map(|f| f.captures.into_inner()).unwrap_or_default();
        let unhandled = fx.async_origins[origin_start..]
            .iter()
            .filter(|(_, handled)| !*handled)
            .map(|(pos, _)| pos.clone())
            .collect::<Vec<_>>();
        for origin in unhandled {
            let group = fx.group_origins.iter().any(|id| {
                fx.async_origins
                    .get(*id as usize)
                    .is_some_and(|(pos, _)| *pos == origin)
            });
            self.reject_subset(
                if group {
                    RejectionSite::TaskGroupUnjoined
                } else {
                    RejectionSite::AsyncHandleUnawaited
                },
                if group {
                    "a task group requires a join in its declaring scope"
                } else {
                    "an async handle is dropped without any await of its completion"
                },
                origin,
            );
        }
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
