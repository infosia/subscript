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

/// Narrowing facts derived from a checked condition: paths known non-null
/// or known present when the condition is true / false.
pub(crate) fn narrow_paths(cond: &hir::Expr) -> (Vec<String>, Vec<String>) {
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
                (Vec::new(), Vec::new())
            }
            BinOp::And => {
                let (mut t1, _) = narrow_paths(left);
                let (t2, _) = narrow_paths(right);
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
        hir::Stmt::While { cond, body, .. } => is_true_literal(cond) && !contains_break(body),
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
            hir::Stmt::While { .. } | hir::Stmt::For { .. } | hir::Stmt::ForOf { .. } => {}
            hir::Stmt::Let { .. }
            | hir::Stmt::Expr(_)
            | hir::Stmt::Return { .. }
            | hir::Stmt::Break(_) => {}
        }
    }
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

trait AstExprChildren {
    fn children(&self) -> Vec<&ast::Expr>;
}

impl AstExprChildren for ast::Expr {
    fn children(&self) -> Vec<&ast::Expr> {
        match self {
            ast::Expr::Array(array) => array
                .elems
                .iter()
                .flatten()
                .map(|element| element.expr.as_ref())
                .collect(),
            ast::Expr::Object(object) => object
                .props
                .iter()
                .flat_map(|property| match property {
                    ast::PropOrSpread::Spread(spread) => vec![spread.expr.as_ref()],
                    ast::PropOrSpread::Prop(property) => match property.as_ref() {
                        ast::Prop::KeyValue(property) => vec![property.value.as_ref()],
                        ast::Prop::Assign(property) => vec![property.value.as_ref()],
                        ast::Prop::Shorthand(_)
                        | ast::Prop::Getter(_)
                        | ast::Prop::Setter(_)
                        | ast::Prop::Method(_) => Vec::new(),
                    },
                })
                .collect(),
            ast::Expr::Unary(unary) => vec![&unary.arg],
            ast::Expr::Update(update) => vec![&update.arg],
            ast::Expr::Bin(binary) => vec![&binary.left, &binary.right],
            ast::Expr::Assign(assign) => {
                let mut children = Vec::with_capacity(2);
                match &assign.left {
                    ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) => {
                        children.push(member.obj.as_ref());
                        if let ast::MemberProp::Computed(property) = &member.prop {
                            children.push(property.expr.as_ref());
                        }
                    }
                    ast::AssignTarget::Simple(ast::SimpleAssignTarget::Paren(paren)) => {
                        children.push(paren.expr.as_ref());
                    }
                    ast::AssignTarget::Simple(ast::SimpleAssignTarget::TsAs(as_expr)) => {
                        children.push(as_expr.expr.as_ref());
                    }
                    ast::AssignTarget::Simple(ast::SimpleAssignTarget::TsSatisfies(satisfies)) => {
                        children.push(satisfies.expr.as_ref());
                    }
                    ast::AssignTarget::Simple(ast::SimpleAssignTarget::TsNonNull(non_null)) => {
                        children.push(non_null.expr.as_ref());
                    }
                    ast::AssignTarget::Simple(ast::SimpleAssignTarget::TsTypeAssertion(
                        assertion,
                    )) => {
                        children.push(assertion.expr.as_ref());
                    }
                    ast::AssignTarget::Simple(ast::SimpleAssignTarget::TsInstantiation(
                        instance,
                    )) => {
                        children.push(instance.expr.as_ref());
                    }
                    _ => {}
                }
                children.push(assign.right.as_ref());
                children
            }
            ast::Expr::Member(member) => {
                let mut children = vec![member.obj.as_ref()];
                if let ast::MemberProp::Computed(property) = &member.prop {
                    children.push(property.expr.as_ref());
                }
                children
            }
            ast::Expr::SuperProp(property) => match &property.prop {
                ast::SuperProp::Computed(property) => vec![property.expr.as_ref()],
                ast::SuperProp::Ident(_) => Vec::new(),
            },
            ast::Expr::Cond(cond) => vec![&cond.test, &cond.cons, &cond.alt],
            ast::Expr::Call(call) => {
                let mut children = Vec::with_capacity(call.args.len() + 1);
                if let ast::Callee::Expr(callee) = &call.callee {
                    children.push(callee.as_ref());
                }
                children.extend(call.args.iter().map(|argument| argument.expr.as_ref()));
                children
            }
            ast::Expr::New(new) => {
                let mut children = vec![new.callee.as_ref()];
                children.extend(
                    new.args
                        .iter()
                        .flatten()
                        .map(|argument| argument.expr.as_ref()),
                );
                children
            }
            ast::Expr::Seq(sequence) => sequence.exprs.iter().map(Box::as_ref).collect(),
            ast::Expr::Tpl(template) => template.exprs.iter().map(Box::as_ref).collect(),
            ast::Expr::TaggedTpl(template) => std::iter::once(template.tag.as_ref())
                .chain(template.tpl.exprs.iter().map(Box::as_ref))
                .collect(),
            ast::Expr::Arrow(arrow) => match arrow.body.as_ref() {
                ast::BlockStmtOrExpr::Expr(expr) => vec![expr],
                ast::BlockStmtOrExpr::BlockStmt(_) => Vec::new(),
            },
            ast::Expr::Yield(yield_expr) => yield_expr.arg.iter().map(Box::as_ref).collect(),
            ast::Expr::Await(await_expr) => vec![&await_expr.arg],
            ast::Expr::Paren(paren) => vec![&paren.expr],
            ast::Expr::TsTypeAssertion(assertion) => vec![&assertion.expr],
            ast::Expr::TsConstAssertion(assertion) => vec![&assertion.expr],
            ast::Expr::TsNonNull(non_null) => vec![&non_null.expr],
            ast::Expr::TsAs(as_expr) => vec![&as_expr.expr],
            ast::Expr::TsInstantiation(instance) => vec![&instance.expr],
            ast::Expr::TsSatisfies(satisfies) => vec![&satisfies.expr],
            ast::Expr::OptChain(chain) => match chain.base.as_ref() {
                ast::OptChainBase::Member(member) => {
                    let mut children = vec![member.obj.as_ref()];
                    if let ast::MemberProp::Computed(property) = &member.prop {
                        children.push(property.expr.as_ref());
                    }
                    children
                }
                ast::OptChainBase::Call(call) => std::iter::once(call.callee.as_ref())
                    .chain(call.args.iter().map(|argument| argument.expr.as_ref()))
                    .collect(),
            },
            ast::Expr::This(_)
            | ast::Expr::Fn(_)
            | ast::Expr::Ident(_)
            | ast::Expr::Lit(_)
            | ast::Expr::Class(_)
            | ast::Expr::MetaProp(_)
            | ast::Expr::JSXMember(_)
            | ast::Expr::JSXNamespacedName(_)
            | ast::Expr::JSXEmpty(_)
            | ast::Expr::JSXElement(_)
            | ast::Expr::JSXFragment(_)
            | ast::Expr::PrivateName(_)
            | ast::Expr::Invalid(_) => Vec::new(),
        }
    }
}

fn assigned_roots_expr(e: &ast::Expr, out: &mut HashSet<String>) {
    match e {
        ast::Expr::Assign(a) => match &a.left {
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(binding)) => {
                out.insert(binding.id.sym.to_string());
            }
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(m)) => {
                if let Some(root) = member_root(m) {
                    out.insert(root);
                }
            }
            _ => {}
        },
        ast::Expr::Update(u) => {
            if let ast::Expr::Ident(id) = &*u.arg {
                out.insert(id.sym.to_string());
            }
        }
        ast::Expr::Arrow(arrow) => {
            if let ast::BlockStmtOrExpr::BlockStmt(block) = arrow.body.as_ref() {
                for statement in &block.stmts {
                    assigned_roots_stmt(statement, out);
                }
            }
        }
        _ => {}
    }
    for child in e.children() {
        assigned_roots_expr(child, out);
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
        let owner = fx.enter_synthetic_owner();
        let terminates = match s {
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
                fx.scopes.pop();
                out.push(hir::Stmt::Block(inner));
                terminates
            }
            ast::Stmt::Throw(t) => {
                let pos = self.pos(t.span);
                self.error_diverging(
                    RuleCode::S010,
                    "exceptions are not in the language; return a result value",
                    pos,
                    Divergence::Exceptions,
                );
                true
            }
            ast::Stmt::Try(t) => {
                let pos = self.pos(t.span);
                self.error_diverging(
                    RuleCode::S010,
                    "exceptions are not in the language; return a result value",
                    pos,
                    Divergence::Exceptions,
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
        };
        self.finish_synthetic_owner(fx, owner, self.pos(s.span()));
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
            let saved_divergence = self
                .aggregate_type_divergence
                .replace(Divergence::AggregateLayoutLimit);
            let ann = binding
                .type_ann
                .as_ref()
                .map(|ann| self.resolve_type(&ann.type_ann));
            self.aggregate_type_divergence = saved_divergence;
            let Some(init_ast) = &d.init else {
                self.error(
                    RuleCode::S100,
                    "local declarations require an initializer",
                    pos.clone(),
                );
                fx.discard_pending(&name);
                continue;
            };
            let init = self.check_expr(init_ast, ann.as_ref(), fx);
            out.extend(fx.drain_synthetic_prefix());
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
                        self.error(RuleCode::S100, "cannot bind a `void` value", pos.clone());
                        Type::Error
                    }
                    t => t.clone(),
                },
            };
            if dispose && !matches!(ty, Type::Error) {
                let valid = match &ty {
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
                    let message = "a `using` initializer must be a non-null reference class that declares `[Symbol.dispose](): void`; narrow nullable values first";
                    if matches!(init.ty, Type::Nullable(_)) {
                        self.error_diverging(
                            RuleCode::S100,
                            message,
                            pos.clone(),
                            Divergence::UsingDeclaration,
                        );
                    } else {
                        self.error(RuleCode::S100, message, pos.clone());
                    }
                }
            }
            let holds_capturing = self.is_capturing_value(&init, fx);
            let async_origins = self.expr_async_origins(&init, fx);
            self.declare_local(
                &name,
                Local {
                    ty: ty.clone(),
                    mutable,
                    holds_capturing,
                    async_origins,
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
                        self.error_diverging(
                            RuleCode::S009,
                            "capturing lambdas may not escape their defining function",
                            pos.clone(),
                            Divergence::EscapingCapture,
                        );
                    }
                    if matches!(checked.ty, Type::AsyncHandle(_) | Type::Array(_)) {
                        let origins = self.expr_async_origins(&checked, fx);
                        fx.handle_async_origins(&origins);
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
        out.extend(fx.drain_synthetic_prefix());
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
                self.reserve_block_declarations(&b.stmts, fx);
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
        out.extend(fx.drain_synthetic_prefix());
        self.require_bool(&cond);
        let (then_extra, else_extra) = narrow_paths(&cond);

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
        out.extend(fx.drain_synthetic_prefix());
        self.require_bool(&cond);
        let (then_extra, _) = narrow_paths(&cond);

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
                let owner = fx.enter_synthetic_owner();
                let mut init_out = Vec::new();
                self.check_let(v, fx, &mut init_out);
                init_out.extend(fx.drain_synthetic_prefix());
                self.finish_synthetic_owner(fx, owner, self.pos(v.span));
                let init = init_out.pop().map(Box::new);
                out.extend(init_out);
                init
            }
            Some(ast::VarDeclOrExpr::Expr(e)) => {
                let owner = fx.enter_synthetic_owner();
                let checked = self.check_expr(e, None, fx);
                out.extend(fx.drain_synthetic_prefix());
                self.finish_synthetic_owner(fx, owner, self.pos(e.span()));
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

        let (cond, cond_prefix) = match &f.test {
            Some(test) => {
                let owner = fx.enter_synthetic_owner();
                let checked = self.check_expr(test, None, fx);
                self.require_bool(&checked);
                let prefix = fx.drain_synthetic_prefix();
                self.finish_synthetic_owner(fx, owner, self.pos(test.span()));
                (Some(checked), prefix)
            }
            None => (None, Vec::new()),
        };
        let then_extra = cond.as_ref().map(|c| narrow_paths(c).0).unwrap_or_default();

        let mut base = fx.narrowed.clone();
        fx.narrowed.extend(then_extra.clone());
        fx.loop_depth += 1;
        let (mut body, _) = self.check_branch(&f.body, fx);
        let step_statements = f.update.as_ref().map(|update| {
            let owner = fx.enter_synthetic_owner();
            let statements = self.check_expr_stmt(update, fx);
            self.finish_synthetic_owner(fx, owner, self.pos(update.span()));
            statements
        });
        fx.loop_depth -= 1;
        base.retain(|k| fx.narrowed.contains(k) || then_extra.contains(k));
        fx.narrowed = base;
        fx.scopes.pop();

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
            let mut guarded = cond_prefix;
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

    /// Checks and binds P22's closed `for…of` surface. Container views
    /// are recognized here, before ordinary call checking, because
    /// `keys()` / `values()` intentionally have no value type outside
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

        let Some((name, mutable, binding_pos, annotation)) = self.for_of_binding(&f.left) else {
            return;
        };
        let (subject, kind, elem_ty, generator) = self.check_for_of_subject(&f.right, fx);
        out.extend(fx.drain_synthetic_prefix());
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

        let binding_async_origins = self.expr_async_origins(&subject, fx);
        fx.scopes.push(Default::default());
        self.declare_local(
            &name,
            Local {
                ty: elem_ty.clone(),
                mutable,
                holds_capturing: false,
                async_origins: binding_async_origins,
            },
            binding_pos.clone(),
            fx,
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
            dispose: false,
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
    fn for_of_binding(
        &mut self,
        head: &ast::ForHead,
    ) -> Option<(String, bool, crate::diag::Pos, Option<Type>)> {
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
            mutable,
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
                                self.error_diverging(
                                    RuleCode::S014,
                                    "`entries()` yields a pair, but the language has no tuple type",
                                    prop_pos,
                                    Divergence::NoTupleType,
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
        let selected = subject
            .ty
            .iteration_element()
            .map(|(kind, element)| (Some(hir::ForOfKind::from(kind)), element, false))
            .or_else(|| match &subject.ty {
                Type::Generator(value) => Some((None, (**value).clone(), true)),
                _ => None,
            });
        if matches!(subject.ty, Type::Error) {
            return (subject, None, Type::Error, false);
        }
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
        out.extend(fx.drain_synthetic_prefix());
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
        let mut cases = Vec::new();
        for (case_index, case) in sw.cases.iter().enumerate() {
            if let Some(scope) = fx.scopes.last_mut() {
                scope.switch_case = Some(case_index);
            }
            let case_pos = self.pos(case.span);
            let test = if let Some(t) = &case.test {
                let checked = self.check_expr(t, Some(&disc_ty), fx);
                if let Some((alias_name, members)) = &alias_switch {
                    match &**t {
                        ast::Expr::Lit(ast::Lit::Str(label)) => {
                            let label = label.value.to_string();
                            if let Some(index) = members.iter().position(|member| member == &label)
                            {
                                self.require_assignable(
                                    &checked.ty.clone(),
                                    &disc_ty,
                                    checked.pos.clone(),
                                    "the case label",
                                );
                                if !alias_members_seen.insert(index) {
                                    alias_labels_valid = false;
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
            let mut body = Vec::new();
            for s in &case.cons {
                self.check_stmt(s, fx, &mut body);
            }
            cases.push(hir::SwitchCase {
                test,
                body,
                pos: case_pos,
            });
        }
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
        let nullable = Type::Nullable(Box::new(Type::Object));
        let cond = expr(
            ExprKind::Binary {
                op: BinOp::Ne,
                left: Box::new(expr(ExprKind::Local("p".into()), nullable.clone())),
                right: Box::new(expr(ExprKind::Null, Type::Null)),
            },
            Type::Bool,
        );
        let (when_true, when_false) = narrow_paths(&cond);
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
        let (when_true, when_false) = narrow_paths(&cond_eq);
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
        let not_equal = expr(
            ExprKind::AbsenceTest {
                value: Box::new(member()),
                negated: true,
            },
            Type::Bool,
        );
        assert_eq!(
            narrow_paths(&not_equal),
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
            narrow_paths(&equal),
            (Vec::new(), vec!["sampler.compare".to_string()])
        );
    }

    #[test]
    fn root_of_takes_first_segment() {
        assert_eq!(root_of("node.next"), "node");
        assert_eq!(root_of("node"), "node");
    }

    #[test]
    fn assigned_roots_expr_walks_object_literal_values() {
        let source = crate::SourceFile::new(
            "object.ts",
            "export function f(): void { use({ value: (root.field = 1) }); }\n",
        );
        let program = swc_common::GLOBALS.set(&swc_common::Globals::new(), || {
            crate::parse::parse_program(&[source]).expect("object source parses")
        });
        let ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) =
            &program.files[0].module.body[0]
        else {
            panic!("expected an exported declaration");
        };
        let ast::Decl::Fn(function) = &export.decl else {
            panic!("expected a function declaration");
        };
        let body = function.function.body.as_ref().expect("function body");
        let mut assigned = HashSet::new();
        assigned_roots_stmt(&body.stmts[0], &mut assigned);
        assert_eq!(assigned, HashSet::from(["root".to_string()]));
    }
}
