//! Statement checking: declarations, control flow, and the C7 flow
//! narrowing that admits member access on `Ref | null` values.

use std::collections::HashSet;

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::diag::RuleCode;
use crate::hir::{self, BinOp, ExprKind};
use crate::types::{StringAliasId, Type};

#[cfg(test)]
use crate::types::ABSENT_STRING_ALIAS_DISCRIMINANT;

use super::expr::path_key;
use super::{Checker, FnCtx, Local};

/// Narrowing facts derived from a checked condition: paths known non-null
/// or known present when the condition is true / false.
pub(crate) fn narrow_paths(
    cond: &hir::Expr,
    alias_absence: &impl Fn(StringAliasId) -> i64,
) -> (Vec<String>, Vec<String>) {
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
                if null_side.is_some() && matches!(other.ty, Type::Nullable(_)) {
                    if let Some(key) = path_key(other) {
                        return match op {
                            // `p === null` → p is non-null when false.
                            BinOp::Eq => (Vec::new(), vec![key]),
                            // `p !== null` → p is non-null when true.
                            _ => (vec![key], Vec::new()),
                        };
                    }
                }
                let is_absent = |expr: &hir::Expr| match (&expr.kind, &expr.ty) {
                    (ExprKind::Int(value), Type::StringAlias(id)) => {
                        *value == alias_absence(*id)
                    }
                    _ => false,
                };
                let (absent_side, other) = if is_absent(left) {
                    (Some(()), right)
                } else if is_absent(right) {
                    (Some(()), left)
                } else {
                    (None, left)
                };
                if absent_side.is_some() && matches!(other.ty, Type::StringAlias(_)) {
                    if let Some(key) = path_key(other) {
                        return match op {
                            // `p === undefined` -> p is present when false.
                            BinOp::Eq => (Vec::new(), vec![key]),
                            // `p !== undefined` -> p is present when true.
                            _ => (vec![key], Vec::new()),
                        };
                    }
                }
                (Vec::new(), Vec::new())
            }
            BinOp::And => {
                let (mut t1, _) = narrow_paths(left, alias_absence);
                let (t2, _) = narrow_paths(right, alias_absence);
                t1.extend(t2);
                (t1, Vec::new())
            }
            _ => (Vec::new(), Vec::new()),
        }
    } else {
        (Vec::new(), Vec::new())
    }
}

fn root_of(key: &str) -> &str {
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
        hir::Stmt::Return { .. } => true,
        hir::Stmt::Expr(hir::Expr {
            kind:
                ExprKind::Call {
                    callee: hir::Callee::Ambient(hir::AmbientFn::Unreachable),
                    ..
                },
            ..
        }) => true,
        hir::Stmt::Block(b) => always_returns(b),
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
        hir::Stmt::While { cond, body, .. } => {
            is_true_literal(cond) && !contains_break(body)
        }
        hir::Stmt::For { cond, body, .. } => {
            cond.as_ref().map_or(true, is_true_literal) && !contains_break(body)
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
        hir::Stmt::Block(b) => contains_break(b),
        hir::Stmt::If { then, els, .. } => {
            contains_break(then) || els.as_ref().is_some_and(|e| contains_break(e))
        }
        _ => false,
    })
}

/// Collects root names assigned anywhere in a statement (used to drop
/// narrowing facts across loop iterations).
fn assigned_roots_stmt(s: &ast::Stmt, out: &mut HashSet<String>) {
    match s {
        ast::Stmt::Block(b) => {
            for s in &b.stmts {
                assigned_roots_stmt(s, out);
            }
        }
        ast::Stmt::If(i) => {
            assigned_roots_expr(&i.test, out);
            assigned_roots_stmt(&i.cons, out);
            if let Some(alt) = &i.alt {
                assigned_roots_stmt(alt, out);
            }
        }
        ast::Stmt::While(w) => {
            assigned_roots_expr(&w.test, out);
            assigned_roots_stmt(&w.body, out);
        }
        ast::Stmt::For(f) => {
            match &f.init {
                Some(ast::VarDeclOrExpr::Expr(e)) => assigned_roots_expr(e, out),
                Some(ast::VarDeclOrExpr::VarDecl(v)) => {
                    for d in &v.decls {
                        if let Some(init) = &d.init {
                            assigned_roots_expr(init, out);
                        }
                    }
                }
                None => {}
            }
            if let Some(test) = &f.test {
                assigned_roots_expr(test, out);
            }
            if let Some(update) = &f.update {
                assigned_roots_expr(update, out);
            }
            assigned_roots_stmt(&f.body, out);
        }
        ast::Stmt::ForOf(f) => {
            assigned_roots_expr(&f.right, out);
            assigned_roots_stmt(&f.body, out);
        }
        ast::Stmt::Switch(sw) => {
            assigned_roots_expr(&sw.discriminant, out);
            for case in &sw.cases {
                for s in &case.cons {
                    assigned_roots_stmt(s, out);
                }
            }
        }
        ast::Stmt::Return(r) => {
            if let Some(arg) = &r.arg {
                assigned_roots_expr(arg, out);
            }
        }
        ast::Stmt::Expr(e) => assigned_roots_expr(&e.expr, out),
        ast::Stmt::Decl(ast::Decl::Var(v)) => {
            for d in &v.decls {
                if let Some(init) = &d.init {
                    assigned_roots_expr(init, out);
                }
            }
        }
        ast::Stmt::Throw(t) => assigned_roots_expr(&t.arg, out),
        _ => {}
    }
}

fn assigned_roots_expr(e: &ast::Expr, out: &mut HashSet<String>) {
    match e {
        ast::Expr::Assign(a) => {
            match &a.left {
                ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(binding)) => {
                    out.insert(binding.id.sym.to_string());
                }
                ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(m)) => {
                    if let Some(root) = member_root(m) {
                        out.insert(root);
                    }
                }
                _ => {}
            }
            assigned_roots_expr(&a.right, out);
        }
        ast::Expr::Update(u) => {
            if let ast::Expr::Ident(id) = &*u.arg {
                out.insert(id.sym.to_string());
            }
            assigned_roots_expr(&u.arg, out);
        }
        ast::Expr::Bin(b) => {
            assigned_roots_expr(&b.left, out);
            assigned_roots_expr(&b.right, out);
        }
        ast::Expr::Unary(u) => assigned_roots_expr(&u.arg, out),
        ast::Expr::Paren(p) => assigned_roots_expr(&p.expr, out),
        ast::Expr::Member(m) => assigned_roots_expr(&m.obj, out),
        ast::Expr::Cond(c) => {
            assigned_roots_expr(&c.test, out);
            assigned_roots_expr(&c.cons, out);
            assigned_roots_expr(&c.alt, out);
        }
        ast::Expr::Call(c) => {
            if let ast::Callee::Expr(callee) = &c.callee {
                assigned_roots_expr(callee, out);
            }
            for arg in &c.args {
                assigned_roots_expr(&arg.expr, out);
            }
        }
        ast::Expr::New(n) => {
            if let Some(args) = &n.args {
                for arg in args {
                    assigned_roots_expr(&arg.expr, out);
                }
            }
        }
        ast::Expr::Tpl(t) => {
            for e in &t.exprs {
                assigned_roots_expr(e, out);
            }
        }
        ast::Expr::Array(a) => {
            for e in a.elems.iter().flatten() {
                assigned_roots_expr(&e.expr, out);
            }
        }
        ast::Expr::TsAs(a) => assigned_roots_expr(&a.expr, out),
        ast::Expr::Yield(y) => {
            if let Some(arg) = &y.arg {
                assigned_roots_expr(arg, out);
            }
        }
        ast::Expr::Arrow(a) => match &*a.body {
            ast::BlockStmtOrExpr::Expr(e) => assigned_roots_expr(e, out),
            ast::BlockStmtOrExpr::BlockStmt(b) => {
                for s in &b.stmts {
                    assigned_roots_stmt(s, out);
                }
            }
        },
        _ => {}
    }
}

fn member_root(m: &ast::MemberExpr) -> Option<String> {
    let mut obj: &ast::Expr = &m.obj;
    loop {
        match obj {
            ast::Expr::Ident(id) => return Some(id.sym.to_string()),
            ast::Expr::Member(inner) => obj = &inner.obj,
            ast::Expr::Paren(p) => obj = &p.expr,
            ast::Expr::This(_) => return Some("this".to_string()),
            _ => return None,
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
        match s {
            ast::Stmt::Decl(ast::Decl::Var(v)) => {
                self.check_let(v, fx, out);
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
                let checked = self.check_expr_stmt(&e.expr, fx);
                out.push(hir::Stmt::Expr(checked));
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
                    self.error(RuleCode::S100, "`break` outside a loop or switch", pos.clone());
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
                let mut inner = Vec::new();
                let mut terminates = false;
                for s in &b.stmts {
                    terminates |= self.check_stmt(s, fx, &mut inner);
                }
                fx.scopes.pop();
                out.push(hir::Stmt::Block(inner));
                terminates
            }
            ast::Stmt::Throw(t) => {
                let pos = self.pos(t.span);
                self.error(
                    RuleCode::S010,
                    "exceptions are not in the language; return a result value",
                    pos,
                );
                true
            }
            ast::Stmt::Try(t) => {
                let pos = self.pos(t.span);
                self.error(
                    RuleCode::S010,
                    "exceptions are not in the language; return a result value",
                    pos,
                );
                false
            }
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
        }
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
        for d in &v.decls {
            let ast::Pat::Ident(binding) = &d.name else {
                let pos = self.pos(d.span);
                self.error(
                    RuleCode::S100,
                    "destructuring is not in the decided surface",
                    pos,
                );
                continue;
            };
            let name = binding.id.sym.to_string();
            let pos = self.pos(binding.id.span);
            let ann = binding
                .type_ann
                .as_ref()
                .map(|ann| self.resolve_type(&ann.type_ann));
            let Some(init_ast) = &d.init else {
                self.error(
                    RuleCode::S100,
                    "local declarations require an initializer",
                    pos.clone(),
                );
                continue;
            };
            let init = self.check_expr(init_ast, ann.as_ref(), fx);
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
                None => match &init.ty {
                    Type::Null => {
                        self.error(
                            RuleCode::S100,
                            "cannot infer a type from `null`; annotate the declaration",
                            pos.clone(),
                        );
                        Type::Error
                    }
                    Type::Void => {
                        self.error(
                            RuleCode::S100,
                            "cannot bind a `void` value",
                            pos.clone(),
                        );
                        Type::Error
                    }
                    t => t.clone(),
                },
            };
            let holds_capturing = self.is_capturing_value(&init, fx);
            fx.declare(
                &name,
                Local {
                    ty: ty.clone(),
                    mutable,
                    holds_capturing,
                },
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
                    self.error(
                        RuleCode::S100,
                        "a `void` function cannot return a value",
                        pos.clone(),
                    );
                    Some(self.check_expr(arg, None, fx))
                } else {
                    let checked = self.check_expr(arg, Some(&ret), fx);
                    self.require_assignable(
                        &checked.ty.clone(),
                        &ret,
                        checked.pos.clone(),
                        "the return value",
                    );
                    if self.is_capturing_value(&checked, fx) {
                        self.error(
                            RuleCode::S009,
                            "capturing lambdas may not escape their defining function",
                            pos.clone(),
                        );
                    }
                    Some(checked)
                }
            }
            None => {
                if ret != Type::Void && !is_generator && !matches!(ret, Type::Error) {
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
        if !matches!(cond.ty, Type::Bool | Type::Error) {
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
                let mut t = false;
                for s in &b.stmts {
                    t |= self.check_stmt(s, fx, &mut out);
                }
                t
            }
            single => self.check_stmt(single, fx, &mut out),
        };
        fx.scopes.pop();
        (out, terminates)
    }

    fn check_if(&mut self, i: &ast::IfStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) -> bool {
        let pos = self.pos(i.span);
        let cond = self.check_expr(&i.test, None, fx);
        self.require_bool(&cond);
        let (then_extra, else_extra) = narrow_paths(&cond, &|id| {
            self.string_aliases[id.0].absence_discriminant()
        });

        let mut base = fx.narrowed.clone();

        fx.narrowed = base.iter().cloned().chain(then_extra.clone()).collect();
        let (then_stmts, then_term) = self.check_branch(&i.cons, fx);
        // Keep kills: facts removed inside the branch stay removed.
        base.retain(|k| fx.narrowed.contains(k) || then_extra.contains(k));

        let (els_stmts, else_term) = match &i.alt {
            Some(alt) => {
                fx.narrowed = base.iter().cloned().chain(else_extra.clone()).collect();
                let (stmts, term) = self.check_branch(alt, fx);
                base.retain(|k| fx.narrowed.contains(k) || else_extra.contains(k));
                (Some(stmts), term)
            }
            None => (None, false),
        };

        fx.narrowed = base;
        // A terminating branch propagates the other side's facts.
        match &i.alt {
            Some(_) => {
                if then_term {
                    fx.narrowed.extend(else_extra);
                }
                if else_term {
                    fx.narrowed.extend(then_extra);
                }
            }
            None => {
                if then_term {
                    fx.narrowed.extend(else_extra);
                }
            }
        }

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
        // Facts about names reassigned in the loop do not survive
        // iteration boundaries.
        let mut roots = HashSet::new();
        assigned_roots_expr(&w.test, &mut roots);
        assigned_roots_stmt(&w.body, &mut roots);
        fx.narrowed.retain(|k| !roots.contains(root_of(k)));

        let cond = self.check_expr(&w.test, None, fx);
        self.require_bool(&cond);
        let (then_extra, _) = narrow_paths(&cond, &|id| {
            self.string_aliases[id.0].absence_discriminant()
        });

        let mut base = fx.narrowed.clone();
        fx.narrowed.extend(then_extra.clone());
        fx.loop_depth += 1;
        let (body, _) = self.check_branch(&w.body, fx);
        fx.loop_depth -= 1;
        base.retain(|k| fx.narrowed.contains(k) || then_extra.contains(k));
        fx.narrowed = base;

        out.push(hir::Stmt::While { cond, body, pos });
    }

    fn check_for(&mut self, f: &ast::ForStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        let pos = self.pos(f.span);
        fx.scopes.push(Default::default());
        let init = match &f.init {
            Some(ast::VarDeclOrExpr::VarDecl(v)) => {
                let mut init_out = Vec::new();
                self.check_let(v, fx, &mut init_out);
                init_out.into_iter().next().map(Box::new)
            }
            Some(ast::VarDeclOrExpr::Expr(e)) => {
                let checked = self.check_expr(e, None, fx);
                Some(Box::new(hir::Stmt::Expr(checked)))
            }
            None => None,
        };

        let mut roots = HashSet::new();
        if let Some(test) = &f.test {
            assigned_roots_expr(test, &mut roots);
        }
        if let Some(update) = &f.update {
            assigned_roots_expr(update, &mut roots);
        }
        assigned_roots_stmt(&f.body, &mut roots);
        fx.narrowed.retain(|k| !roots.contains(root_of(k)));

        let cond = f.test.as_ref().map(|t| {
            let checked = self.check_expr(t, None, fx);
            self.require_bool(&checked);
            checked
        });
        let then_extra = cond
            .as_ref()
            .map(|c| {
                narrow_paths(c, &|id| self.string_aliases[id.0].absence_discriminant()).0
            })
            .unwrap_or_default();

        let mut base = fx.narrowed.clone();
        fx.narrowed.extend(then_extra.clone());
        fx.loop_depth += 1;
        let (body, _) = self.check_branch(&f.body, fx);
        let step = f.update.as_ref().map(|u| self.check_expr(u, None, fx));
        fx.loop_depth -= 1;
        base.retain(|k| fx.narrowed.contains(k) || then_extra.contains(k));
        fx.narrowed = base;
        fx.scopes.pop();

        out.push(hir::Stmt::For {
            init,
            cond,
            step,
            body,
            pos,
        });
    }

    /// Checks and binds P22's closed `for…of` surface. Container views
    /// are recognized here, before ordinary call checking, because
    /// `keys()` / `values()` intentionally have no value type outside
    /// this exact subject position.
    fn check_for_of(
        &mut self,
        f: &ast::ForOfStmt,
        fx: &mut FnCtx,
        out: &mut Vec<hir::Stmt>,
    ) {
        let pos = self.pos(f.span);
        if f.is_await {
            self.error(
                RuleCode::S013,
                "`for await…of` requires the Promise object/iterator surface, which is not in the language",
                pos,
            );
            return;
        }

        let Some((name, mutable, binding_pos, annotation)) =
            self.for_of_binding(&f.left)
        else {
            return;
        };
        let (subject, kind, elem_ty, generator) =
            self.check_for_of_subject(&f.right, fx);
        if matches!(subject.ty, Type::Error) || matches!(elem_ty, Type::Error) {
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

        let mut roots = HashSet::new();
        assigned_roots_expr(&f.right, &mut roots);
        assigned_roots_stmt(&f.body, &mut roots);
        fx.narrowed.retain(|k| !roots.contains(root_of(k)));

        fx.scopes.push(Default::default());
        fx.declare(
            &name,
            Local {
                ty: elem_ty.clone(),
                mutable,
                holds_capturing: false,
            },
        );
        let prefix = format!("{name}.");
        fx.narrowed
            .retain(|key| key != &name && !key.starts_with(&prefix));
        fx.loop_depth += 1;
        let (body, _) = self.check_branch(&f.body, fx);
        fx.loop_depth -= 1;
        fx.scopes.pop();

        let id = self.next_for_of_id;
        self.next_for_of_id += 1;
        let subject_name = format!("[[for.of#{id}.subject]]");
        let subject_ty = subject.ty.clone();
        let subject_local = hir::Expr {
            kind: ExprKind::Local(subject_name.clone()),
            ty: subject_ty.clone(),
            pos: subject.pos.clone(),
        };
        let subject_let = hir::Stmt::Let {
            name: subject_name.clone(),
            ty: subject_ty,
            mutable: false,
            init: subject,
            pos: pos.clone(),
        };

        let loop_stmt = if generator {
            let step_name = format!("[[for.of#{id}.step]]");
            let step_ty = Type::IterResult(Box::new(elem_ty.clone()));
            let next = hir::Expr {
                kind: ExprKind::Call {
                    callee: hir::Callee::Method {
                        recv: Box::new(subject_local),
                        name: "next".to_string(),
                    },
                    args: Vec::new(),
                },
                ty: step_ty.clone(),
                pos: pos.clone(),
            };
            let step_local = || hir::Expr {
                kind: ExprKind::Local(step_name.clone()),
                ty: step_ty.clone(),
                pos: pos.clone(),
            };
            let mut driven_body = vec![
                hir::Stmt::Let {
                    name: step_name.clone(),
                    ty: step_ty.clone(),
                    mutable: false,
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
            driven_body.extend(body);
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
    fn for_of_binding(
        &mut self,
        head: &ast::ForHead,
    ) -> Option<(String, bool, crate::diag::Pos, Option<Type>)> {
        let ast::ForHead::VarDecl(decl) = head else {
            self.error(
                RuleCode::S100,
                "`for…of` requires a `const` or `let` identifier binding",
                self.pos(head.span()),
            );
            return None;
        };
        if decl.kind == ast::VarDeclKind::Var {
            self.error(
                RuleCode::S100,
                "`var` is not in the language; use `let` or `const`",
                self.pos(decl.span),
            );
        }
        if decl.decls.len() != 1 {
            self.error(
                RuleCode::S100,
                "`for…of` requires exactly one identifier binding",
                self.pos(decl.span),
            );
            return None;
        }
        let binding = &decl.decls[0];
        if binding.init.is_some() {
            self.error(
                RuleCode::S100,
                "`for…of` bindings cannot have an initializer",
                self.pos(binding.span),
            );
        }
        let ast::Pat::Ident(ident) = &binding.name else {
            self.error(
                RuleCode::S100,
                "destructuring is not in the decided surface",
                self.pos(binding.name.span()),
            );
            return None;
        };
        let annotation = ident
            .type_ann
            .as_ref()
            .map(|ann| self.resolve_type(&ann.type_ann));
        Some((
            ident.id.sym.to_string(),
            decl.kind == ast::VarDeclKind::Let,
            self.pos(ident.id.span),
            annotation,
        ))
    }

    /// Returns the stabilized receiver expression, fused traversal kind,
    /// bound type, and whether the subject is a C8 generator.
    fn check_for_of_subject(
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
                            let prop_pos = self.pos(prop.span);
                            if name == "entries" {
                                self.error(
                                    RuleCode::S014,
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
                            let selected = match (&recv.ty, name) {
                                (Type::Array(_), "keys") => {
                                    Some((hir::ForOfKind::ArrayKeys, Type::I32))
                                }
                                (Type::Array(elem), "values") => Some((
                                    hir::ForOfKind::ArrayValues,
                                    (**elem).clone(),
                                )),
                                (Type::Map(key, _), "keys") => {
                                    Some((hir::ForOfKind::MapKeys, (**key).clone()))
                                }
                                (Type::Map(_, value), "values") => Some((
                                    hir::ForOfKind::MapValues,
                                    (**value).clone(),
                                )),
                                (Type::Set(key), "keys" | "values") => Some((
                                    hir::ForOfKind::SetValues,
                                    (**key).clone(),
                                )),
                                _ => None,
                            };
                            if let Some((kind, elem)) = selected {
                                return (recv, Some(kind), elem, false);
                            }
                            let actual = self.type_name(&recv.ty);
                            self.error(
                                RuleCode::S014,
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

        let saved_for_of_subject = self.in_for_of_subject;
        self.in_for_of_subject = true;
        let subject = self.check_expr(expression, None, fx);
        self.in_for_of_subject = saved_for_of_subject;
        let selected = match &subject.ty {
            Type::Array(elem) => Some((
                Some(hir::ForOfKind::ArrayValues),
                (**elem).clone(),
                false,
            )),
            Type::FixedArray(elem, _) => Some((
                Some(hir::ForOfKind::FixedArrayValues),
                (**elem).clone(),
                false,
            )),
            Type::Map(key, _) => Some((
                Some(hir::ForOfKind::MapKeys),
                (**key).clone(),
                false,
            )),
            Type::Set(key) => Some((
                Some(hir::ForOfKind::SetValues),
                (**key).clone(),
                false,
            )),
            Type::Str => Some((
                Some(hir::ForOfKind::StringCodePoints),
                Type::Str,
                false,
            )),
            Type::Generator(value) => Some((None, (**value).clone(), true)),
            Type::Error => return (subject, None, Type::Error, false),
            _ => None,
        };
        if let Some((kind, elem, generator)) = selected {
            return (subject, kind, elem, generator);
        }
        let actual = self.type_name(&subject.ty);
        if let Type::Class(id) = subject.ty {
            let class = &self.classes[id.0].name;
            self.error(
                RuleCode::S014,
                format!(
                    "`for…of` cannot make user class `{class}` iterable (invariant 5): \
                     that requires `Symbol.iterator`, and `Symbol` is a permanent non-goal; \
                     stock `tsc` rejects this subject too"
                ),
                subject.pos.clone(),
            );
        } else {
            self.error(
                RuleCode::S014,
                format!(
                    "`for…of` accepts only T[], FixedArray<T, N>, Map, Set, string, \
                     or Generator<T>; got `{actual}`"
                ),
                subject.pos.clone(),
            );
        }
        (subject, None, Type::Error, false)
    }

    fn check_switch(&mut self, sw: &ast::SwitchStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        let pos = self.pos(sw.span);
        let disc = self.check_expr(&sw.discriminant, None, fx);
        if !disc.ty.is_integer()
            && !matches!(
                disc.ty,
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
        let alias_switch = match &disc_ty {
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
        let mut cases = Vec::new();
        for case in &sw.cases {
            let case_pos = self.pos(case.span);
            let test = if let Some(t) = &case.test {
                let checked = self.check_expr(t, Some(&disc_ty), fx);
                if let Some((alias_name, members)) = &alias_switch {
                    match &**t {
                        ast::Expr::Lit(ast::Lit::Str(label)) => {
                            let label = label.value.to_string();
                            if let Some(index) =
                                members.iter().position(|member| member == &label)
                            {
                                self.require_assignable(
                                    &checked.ty.clone(),
                                    &disc_ty,
                                    checked.pos.clone(),
                                    "the case label",
                                );
                                if !alias_members_seen.insert(index) {
                                    alias_labels_valid = false;
                                    self.error(
                                        RuleCode::S100,
                                        format!(
                                            "duplicate case label {label:?} for string-literal union alias `{alias_name}`"
                                        ),
                                        checked.pos.clone(),
                                    );
                                }
                            } else {
                                alias_labels_valid = false;
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
                            alias_labels_valid = false;
                            self.error(
                                RuleCode::S100,
                                format!(
                                    "case labels for string-literal union alias `{alias_name}` must be string literals naming a member"
                                ),
                                checked.pos.clone(),
                            );
                        }
                    }
                } else {
                    self.require_assignable(
                        &checked.ty.clone(),
                        &disc_ty,
                        checked.pos.clone(),
                        "the case label",
                    );
                }
                Some(checked)
            } else {
                has_default = true;
                None
            };
            fx.scopes.push(Default::default());
            let mut body = Vec::new();
            for s in &case.cons {
                self.check_stmt(s, fx, &mut body);
            }
            fx.scopes.pop();
            cases.push(hir::SwitchCase {
                test,
                body,
                pos: case_pos,
            });
        }
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
                self.error(
                    RuleCode::S100,
                    format!(
                        "non-exhaustive switch over string-literal union alias `{alias_name}`; missing case labels: {missing}"
                    ),
                    pos.clone(),
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
        let nullable = Type::Nullable(Box::new(Type::Object));
        let cond = expr(
            ExprKind::Binary {
                op: BinOp::Ne,
                left: Box::new(expr(ExprKind::Local("p".into()), nullable.clone())),
                right: Box::new(expr(ExprKind::Null, Type::Null)),
            },
            Type::Bool,
        );
        let (when_true, when_false) = narrow_paths(&cond, &|_| {
            ABSENT_STRING_ALIAS_DISCRIMINANT
        });
        assert_eq!(when_true, vec!["p".to_string()]);
        assert!(when_false.is_empty());

        let cond_eq = expr(
            ExprKind::Binary {
                op: BinOp::Eq,
                left: Box::new(expr(ExprKind::Local("p".into()), nullable)),
                right: Box::new(expr(ExprKind::Null, Type::Null)),
            },
            Type::Bool,
        );
        let (when_true, when_false) = narrow_paths(&cond_eq, &|_| {
            ABSENT_STRING_ALIAS_DISCRIMINANT
        });
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
                        ExprKind::Local("sampler".into()),
                        Type::Class(crate::types::ClassId(0)),
                    )),
                    name: "compare".into(),
                },
                alias.clone(),
            )
        };
        let absent = || {
            expr(
                ExprKind::Int(ABSENT_STRING_ALIAS_DISCRIMINANT),
                alias.clone(),
            )
        };

        let not_equal = expr(
            ExprKind::Binary {
                op: BinOp::Ne,
                left: Box::new(member()),
                right: Box::new(absent()),
            },
            Type::Bool,
        );
        assert_eq!(
            narrow_paths(&not_equal, &|_| ABSENT_STRING_ALIAS_DISCRIMINANT),
            (vec!["sampler.compare".to_string()], Vec::new())
        );

        let equal = expr(
            ExprKind::Binary {
                op: BinOp::Eq,
                left: Box::new(absent()),
                right: Box::new(member()),
            },
            Type::Bool,
        );
        assert_eq!(
            narrow_paths(&equal, &|_| ABSENT_STRING_ALIAS_DISCRIMINANT),
            (Vec::new(), vec!["sampler.compare".to_string()])
        );
    }

    #[test]
    fn root_of_takes_first_segment() {
        assert_eq!(root_of("node.next"), "node");
        assert_eq!(root_of("node"), "node");
    }
}
