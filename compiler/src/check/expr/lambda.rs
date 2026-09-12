//! Checks arrow function expressions (C5).

use std::collections::HashSet;

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{Checker, FnCtx, Frame, Local, ParamSig, Scope};
use crate::diag::{Pos, RuleCode};
use crate::divergence::Divergence;
use crate::hir::{self, ExprKind};
use crate::types::{FuncType, Type};

impl<'p> Checker<'p> {
    pub(super) fn check_lambda(
        &mut self,
        a: &ast::ArrowExpr,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let ctx_fn = match ctx {
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
        if a.is_async {
            self.error_diverging(
                RuleCode::S100,
                "async arrow functions are not in the decided surface; use an async function declaration",
                pos.clone(),
                Divergence::AsyncFunctionShape,
            );
            return self.err_expr(pos);
        }
        if a.is_generator {
            self.error(
                RuleCode::S100,
                "generator arrows are not in the decided surface",
                pos.clone(),
            );
            return self.err_expr(pos);
        }
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
            let sig =
                if let (0, Some(resolved), ast::Pat::Ident(binding)) = (i, resolved_first, pat) {
                    ParamSig {
                        name: binding.id.sym.to_string(),
                        ty: resolved.clone(),
                        has_default: false,
                    }
                } else if let Some(b) = unannotated_ident {
                    if let Some(t) = param_ctx.and_then(|p| p.get(i)) {
                        ParamSig {
                            name: b.id.sym.to_string(),
                            ty: t.clone(),
                            has_default: false,
                        }
                    } else {
                        self.resolve_param_pat(pat)
                    }
                } else {
                    self.resolve_param_pat(pat)
                };
            params.push(sig);
        }
        let mut ret = a
            .return_type
            .as_ref()
            .map(|ann| self.resolve_type(&ann.type_ann))
            .or_else(|| ret_ctx.cloned());

        fx.frames.push(Frame {
            ret: ret.clone().unwrap_or(Type::Error),
            is_generator: false,
            is_async: false,
            yield_ty: None,
            is_lambda: true,
            captures: Vec::new(),
            this_ty: None,
            missing_this_divergence: None,
        });
        fx.scopes.push(Scope {
            fn_boundary: true,
            ..Default::default()
        });
        // Lambda bodies start without the enclosing narrowing facts
        // (conservative: the lambda may run later).
        let saved_narrowed = std::mem::take(&mut fx.narrowed);
        let mut hir_params = Vec::new();
        for (p, pattern) in params.iter().zip(&a.params) {
            let param_pos = self.pos(pattern.span());
            self.declare_local(
                &p.name,
                Local {
                    ty: p.ty.clone(),
                    mutable: true,
                    holds_capturing: false,
                    async_origins: HashSet::new(),
                },
                param_pos,
                fx,
            );
            hir_params.push(hir::Param {
                name: p.name.clone(),
                ty: p.ty.clone(),
                default: None,
                foreign_provenance: None,
                pos: pos.clone(),
            });
        }
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
                    let checked = self.check_expr(e, ret.as_ref(), fx);
                    if let Some(ret) = &ret {
                        self.require_assignable(
                            &checked.ty.clone(),
                            &ret.clone(),
                            checked.pos.clone(),
                            "the lambda body",
                        );
                    } else {
                        ret = Some(checked.ty.clone());
                    }
                    if matches!(checked.ty, Type::AsyncHandle(_) | Type::Array(_)) {
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
                    if ret.is_none() {
                        self.error(
                            RuleCode::S100,
                            "a lambda with a block body requires a return type annotation",
                            pos.clone(),
                        );
                        ret = Some(Type::Error);
                    }
                    if let Some(frame) = fx.frames.last_mut() {
                        frame.ret = ret.clone().unwrap_or(Type::Error);
                    }
                    let mut out = Vec::new();
                    for s in &block.stmts {
                        self.check_stmt(s, fx, &mut out);
                    }
                    if let Some(ret) = &ret {
                        if !matches!(ret, Type::Void | Type::Error)
                            && !crate::check::stmt::always_returns(&out)
                        {
                            self.error(RuleCode::S100, "not all paths return a value", pos.clone());
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
            self.insert_scope_exit_disposals(
                body,
                ret.as_ref().unwrap_or(&Type::Error),
                &mut Vec::new(),
                (None, None),
                (true, &[]),
            )
        } else {
            body
        };
        fx.narrowed = saved_narrowed;
        fx.scopes.pop();
        let frame = fx.frames.pop();
        let captures = frame.map(|f| f.captures).unwrap_or_default();
        let ret = ret.unwrap_or(Type::Error);
        let ty = Type::Func(Box::new(FuncType {
            params: params.iter().map(|p| p.ty.clone()).collect(),
            ret: ret.clone(),
        }));
        hir::Expr {
            kind: ExprKind::Lambda {
                params: hir_params,
                ret,
                body,
                captures,
            },
            ty,
            pos,
        }
    }
}
