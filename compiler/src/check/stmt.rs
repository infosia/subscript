//! Statement checking: declarations, control flow, and the C7 flow
//! narrowing that admits member access on `Ref | null` values.

use std::collections::HashSet;

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::RuleCode;
use crate::divergence::Divergence;
use crate::hir::{self, BinOp, ExprKind};
use crate::types::Type;

use super::expr::path_key;
use super::{Checker, FnCtx, Local};

/// The type annotation a declaration writes on its whole pattern.
fn pattern_type_ann(pat: &ast::Pat) -> Option<&ast::TsTypeAnn> {
    match pat {
        ast::Pat::Ident(binding) => binding.type_ann.as_deref(),
        ast::Pat::Array(array) => array.type_ann.as_deref(),
        ast::Pat::Object(object) => object.type_ann.as_deref(),
        _ => None,
    }
}

/// Narrowing facts derived from a checked leaf condition: paths known
/// non-null or known present when the condition is true / false.
/// `Checker::narrowing_paths` splits `&&` and `||` and applies the kills
/// of compiler.md §124 before it reaches a leaf.
pub(crate) fn narrow_paths(
    cond: &hir::Expr,
    apparent_type: impl Fn(&Type) -> Type,
) -> (Vec<String>, Vec<String>) {
    if let Some(key) = super::exception::instanceof_narrowed_path(cond) {
        return (vec![key], Vec::new());
    }
    if let ExprKind::AbsenceTest { value, negated } = &cond.kind {
        if let Some(key) = path_key(value) {
            return if *negated {
                (vec![key], Vec::new())
            } else {
                (Vec::new(), vec![key])
            };
        }
        return (Vec::new(), Vec::new());
    }
    if let ExprKind::Binary { op, left, right } = &cond.kind {
        match op {
            BinOp::Eq | BinOp::Ne => {
                let (null_side, other) = if matches!(left.kind, ExprKind::Null) {
                    (Some(()), right)
                } else if matches!(right.kind, ExprKind::Null) {
                    (Some(()), left)
                } else {
                    (None, left)
                };
                if null_side.is_some() && matches!(apparent_type(&other.ty), Type::Nullable(_)) {
                    if let Some(key) = path_key(other) {
                        return match op {
                            // `p === null` → p is non-null when false.
                            BinOp::Eq => (Vec::new(), vec![key]),
                            // `p !== null` → p is non-null when true.
                            _ => (vec![key], Vec::new()),
                        };
                    }
                }
                (Vec::new(), Vec::new())
            }
            _ => (Vec::new(), Vec::new()),
        }
    } else {
        (Vec::new(), Vec::new())
    }
}

pub(super) fn root_of(key: &str) -> &str {
    key.split('.').next().unwrap_or(key)
}

/// Conservative divergence analysis over checked bodies: true when every
/// control path returns or reaches another diverging statement. Used for
/// functions with a non-void declared return type (generators are exempt).
pub(crate) fn always_returns(stmts: &[hir::Stmt]) -> bool {
    stmts.iter().any(stmt_returns)
}

fn stmt_returns(s: &hir::Stmt) -> bool {
    match s {
        hir::Stmt::Return { .. } | hir::Stmt::Throw { .. } => true,
        hir::Stmt::Try { body, handler, .. } => always_returns(body) && always_returns(handler),
        hir::Stmt::Expr(hir::Expr {
            kind:
                ExprKind::Call {
                    callee: hir::Callee::Ambient(hir::AmbientFn::Unreachable),
                    ..
                },
            ..
        }) => true,
        hir::Stmt::Block(b) | hir::Stmt::Using { body: b, .. } => always_returns(b),
        hir::Stmt::If {
            then,
            els: Some(els),
            ..
        } => always_returns(then) && always_returns(els),
        hir::Stmt::Switch { disc, cases, .. } => {
            // Every case must diverge before fallthrough or break. A default
            // still proves general switches cover the discriminant; Q32's
            // checker-proven closed alias set proves a default-less switch.
            let covers_discriminant = cases.iter().any(|c| c.test.is_none())
                || (matches!(disc.ty, Type::StringAlias(_))
                    && cases.iter().all(|c| c.test.is_some()));
            covers_discriminant && cases.iter().all(|c| always_returns(&c.body))
        }
        hir::Stmt::While { cond, body, .. } => is_true_literal(cond) && !contains_break(body),
        hir::Stmt::For { cond, body, .. } => {
            cond.as_ref().is_none_or(is_true_literal) && !contains_break(body)
        }
        _ => false,
    }
}

fn is_true_literal(e: &hir::Expr) -> bool {
    matches!(e.kind, ExprKind::Bool(true))
}

/// True when the statements contain a `break` binding to the enclosing
/// loop (nested loops and switches consume their own breaks).
fn contains_break(stmts: &[hir::Stmt]) -> bool {
    stmts.iter().any(|s| match s {
        hir::Stmt::Break(_) => true,
        hir::Stmt::Block(b) | hir::Stmt::Using { body: b, .. } => contains_break(b),
        hir::Stmt::If { then, els, .. } => {
            contains_break(then) || els.as_ref().is_some_and(|e| contains_break(e))
        }
        hir::Stmt::Try { body, handler, .. } => contains_break(body) || contains_break(handler),
        _ => false,
    })
}

fn insert_for_step_before_continues(statements: &mut [hir::Stmt], step: &[hir::Stmt]) {
    for statement in statements {
        match statement {
            hir::Stmt::Continue(pos) => {
                let mut replacement = step.to_vec();
                replacement.push(hir::Stmt::Continue(pos.clone()));
                *statement = hir::Stmt::Block(replacement);
            }
            hir::Stmt::If { then, els, .. } => {
                insert_for_step_before_continues(then, step);
                if let Some(els) = els {
                    insert_for_step_before_continues(els, step);
                }
            }
            hir::Stmt::Switch { cases, .. } => {
                for case in cases {
                    insert_for_step_before_continues(&mut case.body, step);
                }
            }
            hir::Stmt::Block(body) => insert_for_step_before_continues(body, step),
            hir::Stmt::Try { body, handler, .. } => {
                insert_for_step_before_continues(body, step);
                insert_for_step_before_continues(handler, step);
            }
            hir::Stmt::While { .. } | hir::Stmt::For { .. } | hir::Stmt::ForOf { .. } => {}
            hir::Stmt::Let { .. }
            | hir::Stmt::Expr(_)
            | hir::Stmt::Return { .. }
            | hir::Stmt::Break(_)
            | hir::Stmt::Throw { .. } => {}
            hir::Stmt::Using { body, .. } => insert_for_step_before_continues(body, step),
        }
    }
}

impl<'p> Checker<'p> {
    /// Checks one statement into `out`. Returns true when the statement
    /// always terminates the enclosing flow (return/break/continue).
    pub(crate) fn check_stmt(
        &mut self,
        s: &ast::Stmt,
        fx: &mut FnCtx,
        out: &mut Vec<hir::Stmt>,
    ) -> bool {
        fx.ended_shared_narrowing
            .retain(|key| !fx.narrowed.contains(key));
        let start = out.len();
        let (terminates, prefix) = fx.with_synthetic_owner(
            super::SyntheticOwnerKind::Statement(self.pos(s.span())),
            |fx| match s {
                ast::Stmt::Decl(ast::Decl::Var(v)) => {
                    self.check_let(v, fx, out);
                    false
                }
                ast::Stmt::Decl(ast::Decl::Using(using)) => {
                    if fx.frames.last().is_some_and(|frame| frame.is_lambda) {
                        self.error_diverging(
                            RuleCode::S100,
                            "nested declarations are not in the decided surface",
                            self.pos(using.span),
                            Divergence::UsingDeclaration,
                        );
                    } else {
                        self.check_using(using, fx, out);
                    }
                    false
                }
                ast::Stmt::Decl(other) => {
                    let pos = self.pos(other.span());
                    self.error(
                        RuleCode::S100,
                        "nested declarations are not in the decided surface",
                        pos,
                    );
                    false
                }
                ast::Stmt::Expr(e) => {
                    out.extend(self.check_expr_stmt(&e.expr, fx));
                    false
                }
                ast::Stmt::Return(r) => {
                    self.check_return(r, fx, out);
                    true
                }
                ast::Stmt::If(i) => self.check_if(i, fx, out),
                ast::Stmt::While(w) => {
                    self.check_while(w, fx, out);
                    false
                }
                ast::Stmt::For(f) => {
                    self.check_for(f, fx, out);
                    false
                }
                ast::Stmt::Switch(sw) => {
                    self.check_switch(sw, fx, out);
                    false
                }
                ast::Stmt::Break(b) => {
                    let pos = self.pos(b.span);
                    if b.label.is_some() {
                        self.error(RuleCode::S100, "labeled break is not decided", pos.clone());
                    }
                    if fx.loop_depth == 0 && fx.switch_depth == 0 {
                        self.error(
                            RuleCode::S100,
                            "`break` outside a loop or switch",
                            pos.clone(),
                        );
                    }
                    if let Some((depth, edges)) = fx.switch_break_facts.last_mut() {
                        if *depth == fx.loop_depth {
                            edges.push(fx.narrowed.clone());
                        }
                    }
                    out.push(hir::Stmt::Break(pos));
                    true
                }
                ast::Stmt::Continue(c) => {
                    let pos = self.pos(c.span);
                    if c.label.is_some() {
                        self.error(
                            RuleCode::S100,
                            "labeled continue is not decided",
                            pos.clone(),
                        );
                    }
                    if fx.loop_depth == 0 {
                        self.error(RuleCode::S100, "`continue` outside a loop", pos.clone());
                    }
                    out.push(hir::Stmt::Continue(pos));
                    true
                }
                ast::Stmt::Block(b) => {
                    fx.scopes.push(Default::default());
                    self.reserve_block_declarations(&b.stmts, fx);
                    let mut inner = Vec::new();
                    let mut terminates = false;
                    for s in &b.stmts {
                        terminates |= self.check_stmt(s, fx, &mut inner);
                    }
                    self.end_scope_narrowing(&inner, fx);
                    fx.scopes.pop();
                    out.push(hir::Stmt::Block(inner));
                    terminates
                }
                ast::Stmt::Throw(t) => self.check_throw(t, fx, out),
                ast::Stmt::Try(t) => self.check_try(t, fx, out),
                ast::Stmt::ForOf(for_of) => {
                    self.check_for_of(for_of, fx, out);
                    false
                }
                ast::Stmt::Empty(_) => false,
                other => {
                    let pos = self.pos(other.span());
                    self.error(
                        RuleCode::S100,
                        "statement form outside the decided surface",
                        pos,
                    );
                    false
                }
            },
        );
        out.splice(start..start, prefix);
        terminates
    }

    fn check_let(&mut self, v: &ast::VarDecl, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        if v.kind == ast::VarDeclKind::Var {
            let pos = self.pos(v.span);
            self.error(
                RuleCode::S100,
                "`var` is not in the language; use `let` or `const`",
                pos,
            );
            return;
        }
        let mutable = v.kind == ast::VarDeclKind::Let;
        self.check_bindings(&v.decls, mutable, false, fx, out);
    }

    fn check_using(&mut self, using: &ast::UsingDecl, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        if using.is_await {
            self.error_diverging(
                RuleCode::S100,
                "`await using` is not in the decided surface",
                self.pos(using.span),
                Divergence::UsingDeclaration,
            );
        }
        self.check_bindings(&using.decls, false, !using.is_await, fx, out);
    }

    fn check_bindings(
        &mut self,
        declarations: &[ast::VarDeclarator],
        mutable: bool,
        dispose: bool,
        fx: &mut FnCtx,
        out: &mut Vec<hir::Stmt>,
    ) {
        for d in declarations {
            let pattern = super::pattern::classify(&d.name);
            if let super::pattern::Pattern::Rejected(rejection) = &pattern {
                self.reject_pattern(rejection, fx);
                continue;
            }
            let name = match &pattern {
                super::pattern::Pattern::Name(binding) => binding.id.sym.to_string(),
                _ => String::new(),
            };
            let pos = self.pos(pattern.span());
            let saved_divergence = self
                .aggregate_type_divergence
                .replace(Divergence::AggregateLayoutLimit);
            let ann = pattern_type_ann(&d.name).map(|ann| self.resolve_type(&ann.type_ann));
            self.aggregate_type_divergence = saved_divergence;
            let Some(init_ast) = &d.init else {
                self.error(
                    RuleCode::S100,
                    "local declarations require an initializer",
                    pos.clone(),
                );
                for binding in super::pattern::collect_names(&d.name) {
                    fx.discard_pending(binding.id.sym.as_ref());
                }
                continue;
            };
            let init = if declarations.len() > 1 {
                let (init, prefix) = fx.with_synthetic_owner(
                    super::SyntheticOwnerKind::Declarator(self.pos(d.span)),
                    |fx| self.check_expr(init_ast, ann.as_ref(), fx),
                );
                out.extend(prefix);
                init
            } else {
                self.check_expr(init_ast, ann.as_ref(), fx)
            };
            let ty = match ann {
                Some(ann) => {
                    self.require_assignable(
                        &init.ty.clone(),
                        &ann,
                        init.pos.clone(),
                        "the initializer",
                    );
                    ann
                }
                None => match &self.apparent_type(&init.ty) {
                    Type::Null => {
                        self.error(
                            RuleCode::S100,
                            "cannot infer a type from `null`; annotate the declaration",
                            pos.clone(),
                        );
                        Type::Error
                    }
                    _ => init.ty.clone(),
                },
            };
            if dispose && !matches!(&self.apparent_type(&ty), Type::Error) {
                let shape = self.apparent_type(&ty);
                let resource_type = match &shape {
                    Type::Nullable(inner) => inner.as_ref(),
                    other => other,
                };
                let valid = match self.apparent_type(resource_type) {
                    Type::Class(id) => {
                        !self.classes[id.0].is_value
                            && !self.classes[id.0].is_descriptor
                            && self.class_sigs[id.0]
                                .methods
                                .contains_key(hir::DISPOSE_METHOD_NAME)
                    }
                    _ => false,
                };
                if !valid {
                    let message = if matches!(&self.apparent_type(resource_type), Type::Class(id)
                        if !self.classes[id.0].is_value && !self.classes[id.0].is_descriptor)
                    {
                        "the class of a `using` binding must declare `[Symbol.dispose](): void`"
                    } else {
                        "a `using` binding must be a reference class with a disposal hook, or that class or null"
                    };
                    self.error(RuleCode::S100, message, pos.clone());
                }
            }
            if pattern.is_destructuring() {
                let source = hir::Expr {
                    kind: init.kind,
                    ty,
                    pos: init.pos,
                };
                self.bind_pattern(&pattern, source, mutable, fx, out);
                continue;
            }
            let async_origins = self.expr_async_origins(&init, fx);
            self.declare_local(
                &name,
                Local {
                    ty: ty.clone(),
                    mutable,
                    async_origins,
                    caught: false,
                },
                pos.clone(),
                fx,
            );
            // A fresh binding invalidates stale narrowing facts rooted
            // at a shadowed name.
            let prefix = format!("{}.", name);
            fx.narrowed
                .retain(|k| k != &name && !k.starts_with(&prefix));
            out.push(hir::Stmt::Let {
                name,
                ty,
                mutable,
                dispose,
                init,
                pos,
            });
        }
    }

    fn check_return(&mut self, r: &ast::ReturnStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        let pos = self.pos(r.span);
        let (ret, is_generator) = fx
            .frames
            .last()
            .map(|f| (f.ret.clone(), f.is_generator))
            .unwrap_or((Type::Error, false));
        let value = match &r.arg {
            Some(arg) => {
                if is_generator {
                    self.error(
                        RuleCode::S100,
                        "generator return values are not in the decided surface",
                        pos.clone(),
                    );
                    None
                } else if ret == Type::Void {
                    let checked = self.check_expr(arg, None, fx);
                    if self.apparent_type(&checked.ty) != Type::Error {
                        self.error_diverging(
                            RuleCode::S100,
                            "a `void` function cannot return a value",
                            pos.clone(),
                            Divergence::VoidValue,
                        );
                    }
                    Some(checked)
                } else {
                    let checked = self.check_expr(arg, Some(&ret), fx);
                    self.require_assignable(
                        &checked.ty.clone(),
                        &ret,
                        checked.pos.clone(),
                        "the return value",
                    );
                    if matches!(
                        self.apparent_type(&checked.ty),
                        Type::AsyncHandle(_) | Type::Array(_)
                    ) {
                        let origins = self.expr_async_origins(&checked, fx);
                        fx.handle_async_origins(&origins);
                    }
                    Some(checked)
                }
            }
            None => {
                if ret != Type::Void
                    && !is_generator
                    && !matches!(&self.apparent_type(&ret), Type::Error)
                {
                    let name = self.type_name(&ret);
                    self.error(
                        RuleCode::S100,
                        format!("missing return value of type `{}`", name),
                        pos.clone(),
                    );
                }
                None
            }
        };
        out.push(hir::Stmt::Return { value, pos });
    }

    fn require_bool(&mut self, cond: &hir::Expr) {
        if !self.instance_restriction(
            crate::check::opaque::InstanceRestriction::BooleanContext,
            &cond.ty,
        ) && !matches!(self.apparent_type(&cond.ty), Type::Bool | Type::Error)
        {
            let name = self.type_name(&cond.ty);
            self.error(
                RuleCode::S100,
                format!("condition must be boolean, got `{}`", name),
                cond.pos.clone(),
            );
        }
    }

    /// Checks a branch body (a block or a single statement) in its own
    /// scope. Returns the statements and whether the branch terminates.
    fn check_branch(&mut self, s: &ast::Stmt, fx: &mut FnCtx) -> (Vec<hir::Stmt>, bool) {
        fx.scopes.push(Default::default());
        let mut out = Vec::new();
        let terminates = match s {
            ast::Stmt::Block(b) => {
                self.reserve_block_declarations(&b.stmts, fx);
                let mut t = false;
                for s in &b.stmts {
                    t |= self.check_stmt(s, fx, &mut out);
                }
                t
            }
            single => self.check_stmt(single, fx, &mut out),
        };
        self.end_scope_narrowing(&out, fx);
        fx.scopes.pop();
        (out, terminates)
    }

    fn check_if(&mut self, i: &ast::IfStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) -> bool {
        let pos = self.pos(i.span);
        let cond = self.check_expr(&i.test, None, fx);
        self.require_bool(&cond);
        let (then_extra, else_extra) = self.narrowing_paths(&cond, fx);

        let mut base = fx.narrowed.clone();
        let note_paths: HashSet<_> = base.union(&fx.ended_shared_narrowing).cloned().collect();

        fx.narrowed = base.iter().cloned().chain(then_extra.clone()).collect();
        let (then_stmts, then_term) = self.check_branch(&i.cons, fx);
        // compiler.md §124: a branch cannot restore a fact that a call ended.
        let then_facts = fx.narrowed.clone();
        base.retain(|k| fx.narrowed.contains(k));

        let mut else_facts = base
            .iter()
            .cloned()
            .chain(else_extra.clone())
            .collect::<HashSet<_>>();
        let (els_stmts, else_term) = match &i.alt {
            Some(alt) => {
                fx.narrowed = base.iter().cloned().chain(else_extra.clone()).collect();
                let (stmts, term) = self.check_branch(alt, fx);
                else_facts = fx.narrowed.clone();
                base.retain(|k| fx.narrowed.contains(k));
                (Some(stmts), term)
            }
            None => (None, false),
        };

        fx.narrowed = base;
        // A terminating branch propagates the other side's facts.
        match &i.alt {
            Some(_) => {
                if then_term {
                    fx.narrowed.extend(
                        else_extra
                            .into_iter()
                            .filter(|key| else_facts.contains(key)),
                    );
                }
                if else_term {
                    fx.narrowed.extend(
                        then_extra
                            .into_iter()
                            .filter(|key| then_facts.contains(key)),
                    );
                }
            }
            None => {
                if then_term {
                    fx.narrowed.extend(
                        else_extra
                            .into_iter()
                            .filter(|key| else_facts.contains(key)),
                    );
                }
            }
        }

        fx.ended_shared_narrowing
            .retain(|key| note_paths.contains(key) && !fx.narrowed.contains(key));
        out.push(hir::Stmt::If {
            cond,
            then: then_stmts,
            els: els_stmts,
            pos,
        });
        then_term && i.alt.is_some() && else_term
    }

    fn check_while(&mut self, w: &ast::WhileStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        let pos = self.pos(w.span);
        let note_paths = fx.narrowing_note_paths();
        self.end_loop_narrowing(&pos, fx);

        let cond = self.check_expr(&w.test, None, fx);
        self.require_bool(&cond);
        let (then_extra, _) = self.narrowing_paths(&cond, fx);

        let mut base = fx.narrowed.clone();
        fx.narrowed.extend(then_extra.clone());
        fx.loop_depth += 1;
        let (body, _) = self.check_branch(&w.body, fx);
        fx.loop_depth -= 1;
        base.retain(|k| fx.narrowed.contains(k));
        fx.narrowed = base;

        fx.finish_narrowing_join(&note_paths);
        out.push(hir::Stmt::While { cond, body, pos });
    }

    fn check_for(&mut self, f: &ast::ForStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        let pos = self.pos(f.span);
        fx.scopes.push(Default::default());
        let init = match &f.init {
            Some(ast::VarDeclOrExpr::VarDecl(v)) => {
                let (init_out, prefix) = fx.with_synthetic_owner(
                    super::SyntheticOwnerKind::ForInit(self.pos(v.span)),
                    |fx| {
                        let mut init_out = Vec::new();
                        self.check_let(v, fx, &mut init_out);
                        init_out
                    },
                );
                let mut statements = prefix.into_statements();
                statements.extend(init_out);
                let mut init_out = statements;
                let init = init_out.pop().map(Box::new);
                out.extend(init_out);
                init
            }
            Some(ast::VarDeclOrExpr::Expr(e)) => {
                let (checked, prefix) = fx.with_synthetic_owner(
                    super::SyntheticOwnerKind::ForInit(self.pos(e.span())),
                    |fx| self.check_expr(e, None, fx),
                );
                out.extend(prefix);
                Some(Box::new(hir::Stmt::Expr(checked)))
            }
            None => None,
        };

        let note_paths = fx.narrowing_note_paths();
        self.end_loop_narrowing(&pos, fx);

        let (cond, cond_prefix) = match &f.test {
            Some(test) => {
                let (checked, prefix) = fx.with_synthetic_owner(
                    super::SyntheticOwnerKind::ForCond(self.pos(test.span())),
                    |fx| {
                        let checked = self.check_expr(test, None, fx);
                        self.require_bool(&checked);
                        checked
                    },
                );
                (Some(checked), prefix)
            }
            None => (None, super::SyntheticPrefix::default()),
        };
        let then_extra = cond
            .as_ref()
            .map(|c| self.narrowing_paths(c, fx).0)
            .unwrap_or_default();

        let mut base = fx.narrowed.clone();
        fx.narrowed.extend(then_extra.clone());
        fx.loop_depth += 1;
        let (mut body, _) = self.check_branch(&f.body, fx);
        let step_statements = f.update.as_ref().map(|update| {
            let (statements, prefix) = fx.with_synthetic_owner(
                super::SyntheticOwnerKind::ForUpdate(self.pos(update.span())),
                |fx| self.check_expr_stmt(update, fx),
            );
            let mut step = prefix.into_statements();
            step.extend(statements);
            step
        });
        fx.loop_depth -= 1;
        base.retain(|k| fx.narrowed.contains(k));
        fx.narrowed = base;
        fx.scopes.pop();

        fx.finish_narrowing_join(&note_paths);
        let step = step_statements.as_deref().and_then(|statements| {
            let [hir::Stmt::Expr(expression)] = statements else {
                return None;
            };
            Some(expression.clone())
        });
        if cond_prefix.is_empty() && (f.update.is_none() || step.is_some()) {
            out.push(hir::Stmt::For {
                init,
                cond,
                step,
                body,
                pos,
            });
            return;
        }

        let step_statements = step_statements.unwrap_or_default();
        insert_for_step_before_continues(&mut body, &step_statements);
        body.extend(step_statements);
        let cond = cond.unwrap_or_else(|| hir::Expr {
            kind: hir::ExprKind::Bool(true),
            ty: Type::Bool,
            pos: pos.clone(),
        });
        let (cond, body) = if cond_prefix.is_empty() {
            (cond, body)
        } else {
            let mut guarded = cond_prefix.into_statements();
            guarded.push(hir::Stmt::If {
                cond,
                then: body,
                els: Some(vec![hir::Stmt::Break(pos.clone())]),
                pos: pos.clone(),
            });
            (
                hir::Expr {
                    kind: hir::ExprKind::Bool(true),
                    ty: Type::Bool,
                    pos: pos.clone(),
                },
                guarded,
            )
        };
        let mut block = Vec::new();
        if let Some(init) = init {
            block.push(*init);
        }
        block.push(hir::Stmt::While { cond, body, pos });
        out.push(hir::Stmt::Block(block));
    }

    /// Checks and binds the closed `for…of` surface (stdlib.md §14).
    /// Container views are recognized here, before ordinary call
    /// checking, because `keys()` / `values()` have no value type outside
    /// this exact subject position.
    fn check_for_of(&mut self, f: &ast::ForOfStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        let pos = self.pos(f.span);
        if f.is_await {
            self.error(
                RuleCode::S013,
                "`for await…of` requires the Promise object/iterator surface, which is not in the language",
                pos,
            );
            return;
        }

        let Some((pattern, mutable, binding_pos, annotation)) = self.for_of_binding(&f.left) else {
            return;
        };
        // §107.1: a rejected subject keeps its own rule, so the subject
        // runs before the pattern reports.
        let (subject, kind, elem_ty, generator) = self.check_for_of_subject(&f.right, fx);
        if let super::pattern::Pattern::Rejected(rejection) = &pattern {
            // The names bind inside the loop, which this rejection skips.
            fx.scopes.push(Default::default());
            self.reject_pattern(rejection, fx);
            fx.scopes.pop();
            return;
        }
        if matches!(self.apparent_type(&subject.ty), Type::Error)
            || matches!(self.apparent_type(&elem_ty), Type::Error)
        {
            return;
        }
        if let Some(annotation) = annotation {
            self.require_assignable(
                &elem_ty,
                &annotation,
                binding_pos.clone(),
                "the `for…of` binding",
            );
            self.require_assignable(
                &annotation,
                &elem_ty,
                binding_pos.clone(),
                "the `for…of` binding",
            );
        }

        let note_paths = fx.narrowing_note_paths();
        self.end_loop_narrowing(&pos, fx);

        let base = fx.narrowed.clone();
        let binding_async_origins = self.expr_async_origins(&subject, fx);
        fx.scopes.push(Default::default());
        // A pattern binds the element into checker-generated storage and
        // reads its names out of that (§107.2).
        let name = match &pattern {
            super::pattern::Pattern::Name(binding) => binding.id.sym.to_string(),
            _ => {
                let id = self.next_pattern_id;
                self.next_pattern_id += 1;
                format!("[[pattern#{id}.element]]")
            }
        };
        let mut prologue = Vec::new();
        if pattern.is_destructuring() {
            let element = hir::Expr {
                kind: ExprKind::Local(name.clone(), elem_ty.clone()),
                ty: elem_ty.clone(),
                pos: binding_pos.clone(),
            };
            self.bind_pattern_from(&pattern, &element, mutable, fx, &mut prologue);
        } else {
            self.declare_local(
                &name,
                Local {
                    ty: elem_ty.clone(),
                    mutable,
                    async_origins: binding_async_origins,
                    caught: false,
                },
                binding_pos.clone(),
                fx,
            );
        }
        let prefix = format!("{name}.");
        fx.narrowed
            .retain(|key| key != &name && !key.starts_with(&prefix));
        fx.loop_depth += 1;
        let (body, _) = self.check_branch(&f.body, fx);
        fx.loop_depth -= 1;
        fx.scopes.pop();
        fx.narrowed.retain(|key| base.contains(key));
        fx.finish_narrowing_join(&note_paths);
        let body = if prologue.is_empty() {
            body
        } else {
            prologue.extend(body);
            prologue
        };

        let id = self.next_for_of_id;
        self.next_for_of_id += 1;
        let subject_name = format!("[[for.of#{id}.subject]]");
        let subject_ty = subject.ty.clone();
        let subject_local = hir::Expr {
            kind: ExprKind::Local(subject_name.clone(), subject_ty.clone()),
            ty: subject_ty.clone(),
            pos: subject.pos.clone(),
        };
        let subject_let = hir::Stmt::Let {
            name: subject_name.clone(),
            ty: subject_ty,
            mutable: false,
            dispose: false,
            init: subject,
            pos: pos.clone(),
        };

        let loop_stmt = if generator {
            let step_name = format!("[[for.of#{id}.step]]");
            let step_ty = Type::iter_result(elem_ty.clone());
            let next = hir::Expr {
                kind: ExprKind::Call {
                    callee: hir::Callee::Method {
                        recv: Box::new(subject_local),
                        name: hir::Symbol::from_full_text("next"),
                    },
                    args: Vec::new(),
                },
                ty: step_ty.clone(),
                pos: pos.clone(),
            };
            let step_local = || hir::Expr {
                kind: ExprKind::Local(step_name.clone(), step_ty.clone()),
                ty: step_ty.clone(),
                pos: pos.clone(),
            };
            let mut driven_body = vec![
                hir::Stmt::Let {
                    name: step_name.clone(),
                    ty: step_ty.clone(),
                    mutable: false,
                    dispose: false,
                    init: next,
                    pos: pos.clone(),
                },
                hir::Stmt::If {
                    cond: hir::Expr {
                        kind: ExprKind::Field {
                            obj: Box::new(step_local()),
                            name: "done".to_string(),
                        },
                        ty: Type::Bool,
                        pos: pos.clone(),
                    },
                    then: vec![hir::Stmt::Break(pos.clone())],
                    els: None,
                    pos: pos.clone(),
                },
                hir::Stmt::Let {
                    name,
                    ty: elem_ty.clone(),
                    mutable,
                    dispose: false,
                    init: hir::Expr {
                        kind: ExprKind::Field {
                            obj: Box::new(step_local()),
                            name: "value".to_string(),
                        },
                        ty: elem_ty,
                        pos: binding_pos,
                    },
                    pos: pos.clone(),
                },
            ];
            driven_body.push(hir::Stmt::Block(body));
            hir::Stmt::While {
                cond: hir::Expr {
                    kind: ExprKind::Bool(true),
                    ty: Type::Bool,
                    pos: pos.clone(),
                },
                body: driven_body,
                pos: pos.clone(),
            }
        } else {
            hir::Stmt::ForOf {
                name,
                ty: elem_ty,
                subject: subject_local,
                kind: kind.expect("non-generator `for…of` has a fused kind"),
                body,
                pos: pos.clone(),
            }
        };
        out.push(hir::Stmt::Block(vec![subject_let, loop_stmt]));
    }

    /// Resolves the single declaration accepted on the left of
    /// `for…of`.
    fn for_of_binding<'a>(
        &mut self,
        head: &'a ast::ForHead,
    ) -> Option<(
        super::pattern::Pattern<'a>,
        bool,
        crate::diag::Pos,
        Option<Type>,
    )> {
        let (declarations, mutable, declaration_pos) = match head {
            ast::ForHead::VarDecl(decl) => {
                if decl.kind == ast::VarDeclKind::Var {
                    self.error(
                        RuleCode::S100,
                        "`var` is not in the language; use `let` or `const`",
                        self.pos(decl.span),
                    );
                }
                (
                    &decl.decls,
                    decl.kind == ast::VarDeclKind::Let,
                    self.pos(decl.span),
                )
            }
            ast::ForHead::UsingDecl(using) => {
                self.error(
                    RuleCode::S100,
                    if using.is_await {
                        "`await using` in a `for` head is not in the decided surface"
                    } else {
                        "`using` in a `for` head is not in the decided surface"
                    },
                    self.pos(using.span),
                );
                (&using.decls, false, self.pos(using.span))
            }
            _ => {
                self.error(
                    RuleCode::S100,
                    "`for…of` requires a `const` or `let` identifier binding",
                    self.pos(head.span()),
                );
                return None;
            }
        };
        if declarations.len() != 1 {
            self.error(
                RuleCode::S100,
                "`for…of` requires exactly one identifier binding",
                declaration_pos,
            );
            return None;
        }
        let binding = &declarations[0];
        if binding.init.is_some() {
            self.error(
                RuleCode::S100,
                "`for…of` bindings cannot have an initializer",
                self.pos(binding.span),
            );
        }
        let pattern = super::pattern::classify(&binding.name);
        let pos = self.pos(pattern.span());
        let annotation =
            pattern_type_ann(&binding.name).map(|ann| self.resolve_type(&ann.type_ann));
        Some((pattern, mutable, pos, annotation))
    }

    /// Returns the stabilized receiver expression, fused traversal kind,
    /// bound type, and whether the subject is a C8 generator.
    ///
    /// The subject environment does not depend on the member name
    /// (compiler.md §103.2 rule 4), so the flag covers every path
    /// through the subject, the fused views included.
    fn check_for_of_subject(
        &mut self,
        expression: &ast::Expr,
        fx: &mut FnCtx,
    ) -> (hir::Expr, Option<hir::ForOfKind>, Type, bool) {
        let saved_for_of_subject = self.in_for_of_subject;
        self.in_for_of_subject = true;
        let subject = self.check_for_of_subject_under_flag(expression, fx);
        self.in_for_of_subject = saved_for_of_subject;
        subject
    }

    /// The body of [`Self::check_for_of_subject`], which runs with
    /// `in_for_of_subject` set on every path.
    fn check_for_of_subject_under_flag(
        &mut self,
        expression: &ast::Expr,
        fx: &mut FnCtx,
    ) -> (hir::Expr, Option<hir::ForOfKind>, Type, bool) {
        if let ast::Expr::Call(call) = expression {
            if let ast::Callee::Expr(callee) = &call.callee {
                if let ast::Expr::Member(member) = &**callee {
                    if let ast::MemberProp::Ident(prop) = &member.prop {
                        let name = prop.sym.as_ref();
                        if matches!(name, "keys" | "values" | "entries") {
                            let recv = self.check_expr(&member.obj, None, fx);
                            // The view rules reach the §14.1 containers
                            // only. On any other receiver the three names
                            // are ordinary members (compiler.md §103.2).
                            if !self.is_fused_view_receiver(&recv.ty) {
                                let call_pos = self.pos(call.span);
                                let subject =
                                    self.check_method_call_on(recv, prop, call, fx, call_pos);
                                return self.for_of_subject_from(subject);
                            }
                            let prop_pos = self.pos(prop.span);
                            if name == "entries" {
                                self.reject_subset(
                                    crate::check::rejection::RejectionSite::ForOfEntries,
                                    "`entries()` yields a pair, but the language has no tuple type",
                                    prop_pos,
                                );
                                return (recv, None, Type::Error, false);
                            }
                            if !call.args.is_empty() {
                                self.error(
                                    RuleCode::S100,
                                    format!("`{name}()` expects no arguments"),
                                    self.pos(call.span),
                                );
                                return (recv, None, Type::Error, false);
                            }
                            let selected = match (&self.apparent_type(&recv.ty), name) {
                                (Type::Array(_), "keys") => {
                                    Some((hir::ForOfKind::ArrayKeys, Type::I32))
                                }
                                (Type::Array(elem), "values") => {
                                    Some((hir::ForOfKind::ArrayValues, (**elem).clone()))
                                }
                                (Type::Map(key, _), "keys") => {
                                    Some((hir::ForOfKind::MapKeys, (**key).clone()))
                                }
                                (Type::Map(_, value), "values") => {
                                    Some((hir::ForOfKind::MapValues, (**value).clone()))
                                }
                                (Type::Set(key), "keys" | "values") => {
                                    Some((hir::ForOfKind::SetValues, (**key).clone()))
                                }
                                _ => None,
                            };
                            if let Some((kind, elem)) = selected {
                                return (recv, Some(kind), elem, false);
                            }
                            let actual = self.type_name(&recv.ty);
                            self.reject_subset(
                                crate::check::rejection::RejectionSite::ForOfKeys,
                                format!(
                                    "`{name}()` is a subject-only fused view on Map, Set, \
                                     or T[]; receiver is `{actual}`"
                                ),
                                prop_pos,
                            );
                            return (recv, None, Type::Error, false);
                        }
                    }
                }
            }
        }

        let subject = self.check_expr(expression, None, fx);
        self.for_of_subject_from(subject)
    }

    /// True when the `keys`/`values`/`entries` view rules of stdlib.md
    /// §14.1 reach this receiver type (compiler.md §103.2 rule 1).
    fn is_fused_view_receiver(&self, ty: &Type) -> bool {
        matches!(
            &self.apparent_type(ty),
            Type::Array(_) | Type::FixedArray(..) | Type::Map(..) | Type::Set(_)
        )
    }

    /// Selects the fused traversal for a checked `for…of` subject that
    /// is not a container view.
    fn for_of_subject_from(
        &mut self,
        subject: hir::Expr,
    ) -> (hir::Expr, Option<hir::ForOfKind>, Type, bool) {
        // compiler.md §104.1 rules 1, 2 and 4: the check reads the
        // resolved subject type, and it does not read how the bound
        // value is used.
        if matches!(self.apparent_type(&subject.ty), Type::Map(..)) {
            self.reject_subset(
                crate::check::rejection::RejectionSite::ForOfMap,
                "a bare `Map` is not a `for…of` subject: Map traversal binds `K`; \
                 a `[K, V]` pair has no tuple representation in the language; iterate `map.keys()` or `map.values()`",
                subject.pos.clone(),
            );
            return (subject, None, Type::Error, false);
        }
        let subject = self.apparent_expr(subject);
        let selected = subject
            .ty
            .iteration_element()
            .map(|(kind, element)| (Some(hir::ForOfKind::from(kind)), element, false))
            .or_else(|| match &self.apparent_type(&subject.ty) {
                Type::Generator(value) => Some((None, (**value).clone(), true)),
                _ => None,
            });
        if matches!(self.apparent_type(&subject.ty), Type::Error) {
            return (subject, None, Type::Error, false);
        }
        if let Some((kind, elem, generator)) = selected {
            return (subject, kind, elem, generator);
        }
        let actual = self.type_name(&subject.ty);
        if let Type::Class(id) = self.apparent_type(&subject.ty) {
            let class = &self.classes[id.0].name;
            self.reject_subset(
                crate::check::rejection::RejectionSite::ForOfUserClass,
                format!(
                    "`for…of` cannot make user class `{class}` iterable (invariant 5): \
                     that requires `Symbol.iterator`, and `Symbol` is a permanent non-goal; \
                     stock `tsc` rejects this subject too"
                ),
                subject.pos.clone(),
            );
        } else {
            self.reject_subset(
                crate::check::rejection::RejectionSite::ForOfSubject,
                format!(
                    "`for…of` accepts only T[], FixedArray<T, N>, Set, string, \
                     or Generator<T>; got `{actual}`"
                ),
                subject.pos.clone(),
            );
        }
        (subject, None, Type::Error, false)
    }

    fn check_switch_case_test(
        &mut self,
        t: &ast::Expr,
        disc_ty: &Type,
        alias_switch: Option<&(String, Vec<String>)>,
        alias_members_seen: &mut HashSet<usize>,
        alias_labels_valid: &mut bool,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        let checked = self.check_expr(t, Some(disc_ty), fx);
        if let Some((alias_name, members)) = alias_switch {
            match t {
                ast::Expr::Lit(ast::Lit::Str(label)) => {
                    let label = label.value.to_string();
                    if let Some(index) = members.iter().position(|member| member == &label) {
                        self.require_assignable(
                            &checked.ty.clone(),
                            disc_ty,
                            checked.pos.clone(),
                            "the case label",
                        );
                        if !alias_members_seen.insert(index) {
                            *alias_labels_valid = false;
                            self.error_diverging(
                                RuleCode::S100,
                                format!(
                                    "duplicate case label {label:?} for string-literal union alias `{alias_name}`"
                                ),
                                checked.pos.clone(),
                                Divergence::SwitchOverAlias,
                            );
                        }
                    } else {
                        *alias_labels_valid = false;
                        self.error(
                            RuleCode::S100,
                            format!(
                                "case label {label:?} is not a member of string-literal union alias `{alias_name}`"
                            ),
                            checked.pos.clone(),
                        );
                    }
                }
                _ => {
                    *alias_labels_valid = false;
                    self.error(
                        RuleCode::S100,
                        format!(
                            "case labels for string-literal union alias `{alias_name}` must be string literals naming a member"
                        ),
                        checked.pos.clone(),
                    );
                }
            }
        } else if (self.involves_type_parameter(disc_ty)
            || self.involves_type_parameter(&checked.ty))
            && self.generic_overlap(disc_ty, &checked.ty)
        {
            // §143 rule 1a: case labels compare values, rather than assign them.
        } else {
            self.require_assignable(
                &checked.ty.clone(),
                disc_ty,
                checked.pos.clone(),
                "the case label",
            );
        }
        checked
    }

    fn check_switch(&mut self, sw: &ast::SwitchStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        let pos = self.pos(sw.span);
        let disc = self.check_expr(&sw.discriminant, None, fx);
        if !self.instance_restriction(
            crate::check::opaque::InstanceRestriction::SwitchKind,
            &disc.ty,
        ) && !self.apparent_type(&disc.ty).is_integer()
            && !matches!(
                self.apparent_type(&disc.ty),
                Type::Enum(_) | Type::Str | Type::StringAlias(_) | Type::Error
            )
        {
            let name = self.type_name(&disc.ty);
            self.error(
                RuleCode::S100,
                format!(
                    "switch discriminants are integers, enums, strings, or string-literal union aliases; got `{}`",
                    name
                ),
                disc.pos.clone(),
            );
        }
        let disc_ty = disc.ty.clone();
        let alias_switch = match &self.apparent_type(&disc_ty) {
            Type::StringAlias(id) => self
                .string_aliases
                .get(id.0)
                .map(|alias| (alias.name.clone(), alias.members.clone())),
            _ => None,
        };
        let mut alias_members_seen = HashSet::new();
        let mut alias_labels_valid = true;
        let mut has_default = false;
        fx.switch_depth += 1;
        fx.scopes.push(super::Scope {
            is_switch: true,
            ..Default::default()
        });
        for (case_index, case) in sw.cases.iter().enumerate() {
            self.reserve_block_declarations(&case.cons, fx);
            if let Some(scope) = fx.scopes.last_mut() {
                for statement in &case.cons {
                    let ast::Stmt::Decl(declaration) = statement else {
                        continue;
                    };
                    let declarators = match declaration {
                        ast::Decl::Var(declaration)
                            if declaration.kind != ast::VarDeclKind::Var =>
                        {
                            &declaration.decls
                        }
                        ast::Decl::Using(declaration) => &declaration.decls,
                        _ => continue,
                    };
                    for declarator in declarators {
                        if let ast::Pat::Ident(binding) = &declarator.name {
                            scope
                                .switch_declarations
                                .entry(binding.id.sym.to_string())
                                .or_insert(case_index);
                        }
                    }
                }
            }
        }
        let note_paths = fx.narrowing_note_paths();
        let dispatch_notes = fx.ended_shared_narrowing.clone();
        let mut exit_notes = dispatch_notes.clone();
        let mut dispatch = fx.narrowed.clone();
        let mut fallthrough: Option<HashSet<String>> = None;
        fx.switch_break_facts.push((fx.loop_depth, Vec::new()));
        let mut cases = Vec::new();
        for (case_index, case) in sw.cases.iter().enumerate() {
            if let Some(scope) = fx.scopes.last_mut() {
                scope.switch_case = Some(case_index);
            }
            fx.narrowed = dispatch.clone();
            let case_pos = self.pos(case.span);
            let test = if let Some(t) = &case.test {
                let (checked, _) = fx.with_synthetic_owner(
                    super::SyntheticOwnerKind::SwitchCase(self.pos(t.span())),
                    |fx| {
                        self.check_switch_case_test(
                            t,
                            &disc_ty,
                            alias_switch.as_ref(),
                            &mut alias_members_seen,
                            &mut alias_labels_valid,
                            fx,
                        )
                    },
                );
                Some(checked)
            } else {
                has_default = true;
                None
            };
            dispatch = fx.narrowed.clone();
            if let Some(previous) = &fallthrough {
                fx.narrowed.retain(|key| previous.contains(key));
            }
            let mut body = Vec::new();
            let mut terminates = false;
            for s in &case.cons {
                terminates |= self.check_stmt(s, fx, &mut body);
            }
            self.end_scope_narrowing(&body, fx);
            exit_notes.extend(fx.ended_shared_narrowing.iter().cloned());
            fallthrough = (!terminates).then(|| fx.narrowed.clone());
            cases.push(hir::SwitchCase {
                test,
                body,
                pos: case_pos,
            });
        }
        let mut exits = fx
            .switch_break_facts
            .pop()
            .map_or_else(Vec::new, |(_, edges)| edges);
        exits.extend(fallthrough);
        if !has_default {
            exits.push(dispatch);
        }
        fx.narrowed = exits.pop().unwrap_or_default();
        for edge in exits {
            fx.narrowed.retain(|key| edge.contains(key));
        }
        for case in &cases {
            self.apply_narrowing_effects(&self.body_narrowing_effects(&case.body), fx);
        }
        fx.ended_shared_narrowing.extend(exit_notes);
        fx.finish_narrowing_join(&note_paths);
        fx.scopes.pop();
        fx.switch_depth -= 1;
        if let Some((alias_name, members)) = &alias_switch {
            if !has_default && alias_labels_valid && alias_members_seen.len() != members.len() {
                let missing = members
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| !alias_members_seen.contains(index))
                    .map(|(_, member)| format!("{member:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.error_diverging(
                    RuleCode::S100,
                    format!(
                        "non-exhaustive switch over string-literal union alias `{alias_name}`; missing case labels: {missing}"
                    ),
                    pos.clone(),
                    Divergence::SwitchOverAlias,
                );
            }
        }
        out.push(hir::Stmt::Switch { disc, cases, pos });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Pos;

    fn expr(kind: ExprKind, ty: Type) -> hir::Expr {
        hir::Expr {
            kind,
            ty,
            pos: Pos::new("t.ts", 1, 1),
        }
    }

    #[test]
    fn narrow_paths_reads_null_comparisons() {
        let nullable = Type::nullable(Type::Object);
        let cond = expr(
            ExprKind::Binary {
                op: BinOp::Ne,
                left: Box::new(expr(
                    ExprKind::Local("p".into(), nullable.clone()),
                    nullable.clone(),
                )),
                right: Box::new(expr(ExprKind::Null, Type::Null)),
            },
            Type::Bool,
        );
        let (when_true, when_false) = narrow_paths(&cond, Clone::clone);
        assert_eq!(when_true, vec!["p".to_string()]);
        assert!(when_false.is_empty());

        let cond_eq = expr(
            ExprKind::Binary {
                op: BinOp::Eq,
                left: Box::new(expr(
                    ExprKind::Local("p".into(), nullable.clone()),
                    nullable,
                )),
                right: Box::new(expr(ExprKind::Null, Type::Null)),
            },
            Type::Bool,
        );
        let (when_true, when_false) = narrow_paths(&cond_eq, Clone::clone);
        assert!(when_true.is_empty());
        assert_eq!(when_false, vec!["p".to_string()]);
    }

    #[test]
    fn narrow_paths_reads_absence_comparisons() {
        let alias = Type::StringAlias(crate::types::StringAliasId(0));
        let member = || {
            expr(
                ExprKind::Field {
                    obj: Box::new(expr(
                        ExprKind::Local("sampler".into(), Type::Class(crate::types::ClassId(0))),
                        Type::Class(crate::types::ClassId(0)),
                    )),
                    name: "compare".into(),
                },
                alias.clone(),
            )
        };
        let not_equal = expr(
            ExprKind::AbsenceTest {
                value: Box::new(member()),
                negated: true,
            },
            Type::Bool,
        );
        assert_eq!(
            narrow_paths(&not_equal, Clone::clone),
            (vec!["sampler.compare".to_string()], Vec::new())
        );

        let equal = expr(
            ExprKind::AbsenceTest {
                value: Box::new(member()),
                negated: false,
            },
            Type::Bool,
        );
        assert_eq!(
            narrow_paths(&equal, Clone::clone),
            (Vec::new(), vec!["sampler.compare".to_string()])
        );
    }

    #[test]
    fn root_of_takes_first_segment() {
        assert_eq!(root_of("node.next"), "node");
        assert_eq!(root_of("node"), "node");
    }
}
