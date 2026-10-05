//! Statement checking: declarations, control flow, and the C7 flow
//! narrowing that admits member access on `Ref | null` values.

use super::FactSet;
use crate::check::rejection::RejectionSite;
use std::collections::HashSet;

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::hir::{self, ExprKind};
use crate::types::Type;

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

pub(super) fn insert_for_step_before_continues(statements: &mut [hir::Stmt], step: &[hir::Stmt]) {
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
        let saved_rejected_names = std::mem::take(&mut self.rejected_local_names);
        self.rejected_local_names = fx
            .scopes
            .iter()
            .map(|scope| scope.rejected_local_names.clone())
            .collect();
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
                        self.reject_subset(
                            RejectionSite::LambdaUsingDeclaration,
                            "nested declarations are not in the decided surface",
                            self.pos(using.span),
                        );
                    } else {
                        self.check_using(using, fx, out);
                    }
                    false
                }
                ast::Stmt::Decl(other) => {
                    let pos = self.pos(other.span());
                    for name in super::exports::declaration_names(other) {
                        self.declare_in_context(
                            name.sym.as_ref(),
                            Local {
                                annotated: false,
                                ty: Type::Error,
                                mutable: false,
                                async_origins: HashSet::new(),
                                caught: false,
                            },
                            fx,
                        );
                    }
                    self.reject_subset(
                        match other {
                            ast::Decl::Class(_) => RejectionSite::LocalClassDeclaration,
                            ast::Decl::Fn(_) => RejectionSite::LocalFunctionDeclaration,
                            ast::Decl::TsEnum(_) => RejectionSite::LocalEnumDeclaration,
                            ast::Decl::TsTypeAlias(_) => RejectionSite::LocalAliasDeclaration,
                            ast::Decl::TsInterface(_) => RejectionSite::LocalInterfaceDeclaration,
                            ast::Decl::TsModule(_) => RejectionSite::LocalNamespaceDeclaration,
                            _ => RejectionSite::LocalRejectedDeclaration,
                        },
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
                        self.reject_subset(
                            RejectionSite::LabeledBreak,
                            "labeled break is not decided",
                            pos.clone(),
                        );
                    }
                    if fx.loop_depth == 0 && fx.switch_depth == 0 {
                        self.reject_subset(
                            RejectionSite::BreakOutsideLoopOrSwitch,
                            "`break` outside a loop or switch",
                            pos.clone(),
                        );
                    }
                    let switch_exit = fx
                        .switch_break_facts
                        .last()
                        .is_some_and(|(depth, _)| *depth == fx.loop_depth);
                    if switch_exit {
                        let mut facts = fx.narrowed.clone();
                        if let Some(depth) = fx.scopes.iter().rposition(|scope| scope.is_switch) {
                            for scope in fx.scopes[depth + 1..].iter().rev() {
                                facts.retain(|fact| {
                                    fact.narrows_type()
                                        || !scope.vars.contains_key(root_of(fact))
                                            && !fact.leaves_const_scope(&scope.const_keys)
                                });
                                facts.extend_facts(
                                    scope
                                        .shadowed_narrowing
                                        .iter()
                                        .filter(|fact| !fact.narrows_type())
                                        .cloned(),
                                );
                            }
                        }
                        if let Some((_, edges)) = fx.switch_break_facts.last_mut() {
                            edges.push(facts);
                        }
                    }
                    if !switch_exit {
                        self.record_loop_edge(fx, false);
                    }
                    out.push(hir::Stmt::Break(pos));
                    true
                }
                ast::Stmt::Continue(c) => {
                    let pos = self.pos(c.span);
                    if c.label.is_some() {
                        self.reject_subset(
                            RejectionSite::LabeledContinue,
                            "labeled continue is not decided",
                            pos.clone(),
                        );
                    }
                    if fx.loop_depth == 0 {
                        self.reject_subset(
                            RejectionSite::ContinueOutsideLoop,
                            "`continue` outside a loop",
                            pos.clone(),
                        );
                    }
                    self.record_loop_edge(fx, true);
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
                    self.end_scope_narrowing(fx);
                    fx.pop_scope();
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
                    self.reject_subset(
                        match other {
                            ast::Stmt::DoWhile(_) => RejectionSite::DoWhileStatement,
                            ast::Stmt::ForIn(_) => RejectionSite::ForInStatement,
                            ast::Stmt::Labeled(_) => RejectionSite::LabeledStatement,
                            ast::Stmt::Debugger(_) => RejectionSite::DebuggerStatement,
                            ast::Stmt::With(_) => RejectionSite::WithStatement,
                            _ => RejectionSite::UnsupportedStatementKind,
                        },
                        "statement form outside the decided surface",
                        pos,
                    );
                    false
                }
            },
        );
        self.rejected_local_names = saved_rejected_names;
        out.splice(start..start, prefix);
        if out[start..]
            .iter()
            .any(|statement| matches!(statement, hir::Stmt::Let { dispose: true, .. }))
        {
            if let Some(scope) = fx.scopes.last_mut() {
                scope.dispose_on_exit = true;
            }
        }
        fx.flow_reachable &= !terminates;
        terminates
    }

    fn check_let(&mut self, v: &ast::VarDecl, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        if v.kind == ast::VarDeclKind::Var {
            let pos = self.pos(v.span);
            self.reject_subset(
                RejectionSite::LocalVarDeclaration,
                "`var` is not in the language; use `let` or `const`",
                pos,
            );
            for declaration in &v.decls {
                let names = super::pattern::collect_names(&declaration.name);
                self.bind_error_names(&names, fx);
            }
            return;
        }
        let mutable = v.kind == ast::VarDeclKind::Let;
        self.check_bindings(&v.decls, mutable, false, fx, out);
    }

    fn check_using(&mut self, using: &ast::UsingDecl, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        if using.is_await {
            self.reject_subset(
                RejectionSite::AwaitUsingDeclaration,
                "`await using` is not in the decided surface",
                self.pos(using.span),
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
                .aggregate_type_site
                .replace(RejectionSite::AggregateAnnotationLimit);
            let ann = pattern_type_ann(&d.name).map(|ann| self.resolve_type(&ann.type_ann));
            self.aggregate_type_site = saved_divergence;
            if d.init.is_none()
                && ann
                    .as_ref()
                    .is_some_and(|ty| self.apparent_type(ty) == Type::Error)
            {
                self.bind_error_names(&super::pattern::collect_names(&d.name), fx);
                continue;
            }
            let init = if let Some(init_ast) = &d.init {
                if declarations.len() > 1 {
                    let (init, prefix) = fx.with_synthetic_owner(
                        super::SyntheticOwnerKind::Declarator(self.pos(d.span)),
                        |fx| self.check_expr(init_ast, ann.as_ref(), fx),
                    );
                    out.extend(prefix);
                    init
                } else {
                    self.check_expr(init_ast, ann.as_ref(), fx)
                }
            } else if mutable && ann.is_some() && !pattern.is_destructuring() {
                hir::Expr {
                    pending_work: None,
                    kind: hir::ExprKind::Unassigned,
                    ty: ann.clone().unwrap_or(Type::Error),
                    pos: pos.clone(),
                }
            } else {
                self.reject_subset(
                    RejectionSite::LocalTypeWithoutInitializer,
                    "a local without an initializer requires a type annotation",
                    pos.clone(),
                );
                self.bind_error_names(&super::pattern::collect_names(&d.name), fx);
                continue;
            };
            let annotated =
                pattern_type_ann(&d.name).is_some_and(|ann| self.written_type(&ann.type_ann));
            let ty = match ann {
                Some(ann) => {
                    if !matches!(init.kind, hir::ExprKind::Unassigned) {
                        self.require_expr_assignable(&init, &ann, fx, "the initializer");
                    }
                    ann
                }
                None => self.inferred_initializer_type(&init, pos.clone()),
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
                    self.reject_subset(
                        if self.apparent_type(&ty) == Type::Null {
                            RejectionSite::UsingBindingResourceType
                        } else {
                            RejectionSite::UsingBindingNotDisposable
                        },
                        message,
                        pos.clone(),
                    );
                }
            }
            if pattern.is_destructuring() {
                let source = hir::Expr {
                    pending_work: None,
                    kind: init.kind,
                    ty,
                    pos: init.pos,
                };
                self.bind_pattern(&pattern, source, mutable, annotated, fx, out);
                continue;
            }
            // Only direct integer literals and their const aliases share a value identity.
            let integer_key = if !mutable {
                match d.init.as_deref() {
                    Some(ast::Expr::Lit(ast::Lit::Num(_))) => match &init.kind {
                        hir::ExprKind::Int(value) => Some(format!("int:{value}")),
                        _ => None,
                    },
                    Some(ast::Expr::Ident(ident)) => fx
                        .scopes
                        .iter()
                        .rev()
                        .find(|scope| scope.vars.contains_key(ident.sym.as_ref()))
                        .and_then(|scope| scope.const_keys.get(ident.sym.as_ref()))
                        .filter(|identity| identity.starts_with("int:"))
                        .cloned(),
                    _ => None,
                }
            } else {
                None
            };
            let async_origins = self.expr_async_origins(&init, fx);
            self.declare_local(
                &name,
                Local {
                    annotated,
                    ty: ty.clone(),
                    mutable,
                    async_origins,
                    caught: false,
                },
                pos.clone(),
                fx,
            );
            if let Some(identity) = integer_key {
                if let Some(scope) = fx.scopes.last_mut() {
                    scope.const_keys.insert(name.clone(), identity);
                }
            }
            if !matches!(init.kind, hir::ExprKind::Unassigned)
                && matches!(self.apparent_type(&ty), Type::Nullable(_))
                && !matches!(
                    self.apparent_type(&init.ty),
                    Type::Nullable(_) | Type::Null | Type::Error
                )
            {
                fx.narrowed.insert_fact(super::NarrowingFact {
                    kind: crate::check::narrowing_fact::FactKind::Narrowing,
                    key: name.clone(),
                    shared: false,
                });
            }
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
        let poisoned_result = self.apparent_type(&ret) == Type::Error
            && fx.frames.last().is_some_and(|frame| frame.is_lambda);
        let value = match &r.arg {
            Some(arg) if poisoned_result => Some(self.err_expr(self.pos(arg.span()))),
            Some(arg) => {
                if is_generator {
                    self.reject_subset(
                        RejectionSite::GeneratorReturnValue,
                        "generator return values are not in the decided surface",
                        pos.clone(),
                    );
                    None
                } else if ret == Type::Void {
                    let checked = self.check_expr(arg, None, fx);
                    if self.apparent_type(&checked.ty) != Type::Error {
                        self.reject_subset(
                            if fx.frames.last().is_some_and(|frame| frame.contextual_void) {
                                RejectionSite::VoidFunctionReturnValue
                            } else {
                                RejectionSite::ExplicitVoidReturnValue
                            },
                            "a `void` function cannot return a value",
                            pos.clone(),
                        );
                    }
                    Some(checked)
                } else {
                    let checked = self.check_expr(arg, Some(&ret), fx);
                    self.require_expr_assignable(&checked, &ret, fx, "the return value");
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
                    self.reject_subset(
                        RejectionSite::ReturnValueMissing,
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
            self.reject_subset(
                if matches!(
                    self.apparent_type(&cond.ty),
                    Type::AsyncHandle(_) | Type::Func(_)
                ) {
                    RejectionSite::StatementAlwaysTruthyCondition
                } else {
                    RejectionSite::StatementNonBooleanCondition
                },
                format!("condition must be boolean, got `{}`", name),
                cond.pos.clone(),
            );
        }
    }

    /// Checks a branch body (a block or a single statement) in its own
    /// scope. Returns the statements and whether the branch terminates.
    fn check_branch(&mut self, s: &ast::Stmt, fx: &mut FnCtx) -> (Vec<hir::Stmt>, bool) {
        let reachable = fx.flow_reachable;
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
        self.end_scope_narrowing(fx);
        fx.pop_scope();
        fx.flow_reachable = reachable;
        (out, terminates)
    }

    fn check_if(&mut self, i: &ast::IfStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) -> bool {
        let pos = self.pos(i.span);
        let cond = self.check_truth_expr(&i.test, fx);
        self.require_bool(&cond);
        let (then_extra, else_extra) = self.narrowing_paths(&cond, fx);

        let base = fx.narrowed.clone();
        let mut note_paths: HashSet<_> = base.union(&fx.ended_shared_narrowing).cloned().collect();

        let initial_notes = fx.ended_shared_narrowing.clone();
        fx.narrowed.retain(|key| !else_extra.contains(key));
        fx.narrowed.extend_facts(then_extra.clone());
        let (then_stmts, then_term) = self.check_branch(&i.cons, fx);
        let then_term = then_term || always_returns(&then_stmts);
        let then_notes = fx.ended_shared_narrowing.clone();
        let then_facts = fx.narrowed.clone();
        fx.ended_shared_narrowing = initial_notes;
        fx.narrowed = base.clone();
        fx.narrowed.retain(|key| !then_extra.contains(key));
        fx.narrowed.extend_facts(else_extra);
        let (els_stmts, else_term) = match &i.alt {
            Some(alt) => {
                let (stmts, term) = self.check_branch(alt, fx);
                (Some(stmts), term)
            }
            None => (None, false),
        };
        let else_term = else_term || els_stmts.as_ref().is_some_and(|body| always_returns(body));
        let else_facts = fx.narrowed.clone();
        let then_possible: HashSet<_> = then_facts.union(&then_notes).cloned().collect();
        let else_possible: HashSet<_> = else_facts
            .union(&fx.ended_shared_narrowing)
            .cloned()
            .collect();
        if then_term {
            note_paths.extend_facts(else_possible);
        } else if else_term {
            note_paths.extend_facts(then_possible);
        } else {
            note_paths.extend_facts(then_possible.intersection(&else_possible).cloned());
        }
        fx.narrowed = if then_term {
            else_facts
        } else if else_term {
            then_facts
        } else {
            then_facts.intersection(&else_facts).cloned().collect()
        };
        fx.ended_shared_narrowing.extend_facts(then_notes);
        fx.finish_narrowing_join(&note_paths);
        out.push(hir::Stmt::If {
            cond,
            then: then_stmts,
            els: els_stmts,
            pos,
        });
        then_term && i.alt.is_some() && else_term
    }

    fn record_loop_edge(&self, fx: &mut FnCtx, is_continue: bool) {
        if !fx.flow_reachable {
            return;
        }
        if !fx.has_narrowing_facts() {
            if let Some((_, breaks, continues)) = fx.loop_break_facts.last_mut() {
                let edge = super::LoopEdge {
                    facts: Default::default(),
                    notes: Default::default(),
                };
                if is_continue {
                    continues.push(edge);
                } else {
                    breaks.push(edge);
                }
            }
            return;
        }
        if let Some((scope_depth, _, _)) = fx.loop_break_facts.last() {
            let mut edge = super::LoopEdge {
                facts: fx.narrowed.clone(),
                notes: fx.ended_shared_narrowing.clone(),
            };
            let mut disposed = false;
            for scope in fx.scopes[*scope_depth..].iter().rev() {
                disposed |= scope.dispose_on_exit;
                if scope.dispose_on_exit
                    && edge
                        .facts
                        .iter()
                        .any(|key| key.shared && key.narrows_type())
                {
                    edge.facts.retain(|key| {
                        if key.shared && key.narrows_type() {
                            edge.notes.insert_fact(key.clone());
                            false
                        } else {
                            true
                        }
                    });
                }
                if !scope.vars.is_empty()
                    && edge.facts.iter().any(|key| {
                        scope.vars.contains_key(root_of(key))
                            || key.leaves_const_scope(&scope.const_keys)
                    })
                {
                    edge.facts.retain(|key| {
                        !scope.vars.contains_key(root_of(key))
                            && !key.leaves_const_scope(&scope.const_keys)
                    });
                }
                if !scope.shadowed_narrowing.is_empty() {
                    let mut facts = scope.shadowed_narrowing.clone();
                    if disposed && facts.iter().any(|key| key.shared && key.narrows_type()) {
                        facts.retain(|key| {
                            if key.shared && key.narrows_type() {
                                edge.notes.insert_fact(key.clone());
                                false
                            } else {
                                true
                            }
                        });
                    }
                    edge.facts.extend_facts(facts);
                }
                if !scope.shadowed_ended_shared.is_empty() {
                    edge.notes.extend_facts(scope.shadowed_ended_shared.clone());
                }
            }
            if let Some((_, breaks, continues)) = fx.loop_break_facts.last_mut() {
                if is_continue {
                    continues.push(edge);
                } else {
                    breaks.push(edge);
                }
            }
        }
    }

    fn join_loop_exits(fx: &mut FnCtx, false_exit: Option<super::LoopEdge>) {
        let mut exits = fx
            .loop_break_facts
            .pop()
            .map_or_else(Vec::new, |(_, edges, _)| edges);
        exits.extend(false_exit);
        Self::join_flow_edges(fx, exits);
    }

    fn join_flow_edges(fx: &mut FnCtx, mut edges: Vec<super::LoopEdge>) {
        if edges.iter().all(|edge| edge.notes.is_empty()) {
            fx.narrowed = edges.pop().map_or_else(Default::default, |edge| edge.facts);
            fx.ended_shared_narrowing = Default::default();
            for edge in edges {
                if !std::ptr::eq(&*fx.narrowed, &*edge.facts) && *fx.narrowed != *edge.facts {
                    fx.narrowed.retain(|key| edge.facts.contains(key));
                }
            }
            return;
        }
        let first = edges.pop();
        let mut eligible: HashSet<_> = first.as_ref().map_or_else(HashSet::new, |edge| {
            edge.facts.union(&edge.notes).cloned().collect()
        });
        fx.narrowed = first
            .as_ref()
            .map_or_else(Default::default, |edge| edge.facts.clone());
        fx.ended_shared_narrowing = first.map_or_else(Default::default, |edge| edge.notes);
        for edge in edges {
            eligible.retain(|key| edge.facts.contains(key) || edge.notes.contains(key));
            fx.narrowed.retain(|key| edge.facts.contains(key));
            fx.ended_shared_narrowing.extend_facts(edge.notes);
        }
        fx.finish_narrowing_join(&eligible);
    }

    fn check_while(&mut self, w: &ast::WhileStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        let pos = self.pos(w.span);
        self.end_loop_narrowing(&pos, fx);
        let kept = fx.narrowed.clone();

        let cond = self.check_truth_expr(&w.test, fx);
        self.require_bool(&cond);
        let (then_extra, else_extra) = self.narrowing_paths(&cond, fx);
        let mut false_exit = fx.narrowed.clone();
        if !then_extra.is_empty() {
            false_exit.retain(|key| !then_extra.contains(key));
        }
        if !else_extra.is_empty() {
            false_exit.extend_facts(else_extra);
        }
        let false_exit = (!is_true_literal(&cond)).then_some(super::LoopEdge {
            facts: false_exit,
            notes: fx.ended_shared_narrowing.clone(),
        });
        if !then_extra.is_empty() {
            fx.narrowed.extend_facts(then_extra.clone());
        }
        fx.loop_depth += 1;
        fx.loop_break_facts
            .push((fx.scopes.len(), Vec::new(), Vec::new()));
        let (body, _) = self.check_branch(&w.body, fx);
        self.check_loop_stores(&body, std::iter::once(&cond), &kept);
        fx.loop_depth -= 1;
        Self::join_loop_exits(fx, false_exit);

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

        self.end_loop_narrowing(&pos, fx);
        let kept = fx.narrowed.clone();

        let (cond, cond_prefix) = match &f.test {
            Some(test) => {
                let (checked, prefix) = fx.with_synthetic_owner(
                    super::SyntheticOwnerKind::ForCond(self.pos(test.span())),
                    |fx| {
                        let checked = self.check_truth_expr(test, fx);
                        self.require_bool(&checked);
                        checked
                    },
                );
                (Some(checked), prefix)
            }
            None => (None, super::SyntheticPrefix::default()),
        };
        let (then_extra, else_extra) = cond
            .as_ref()
            .map(|c| self.narrowing_paths(c, fx))
            .unwrap_or_default();
        let mut false_exit = fx.narrowed.clone();
        if !then_extra.is_empty() {
            false_exit.retain(|key| !then_extra.contains(key));
        }
        if !else_extra.is_empty() {
            false_exit.extend_facts(else_extra);
        }
        let false_exit = cond
            .as_ref()
            .is_some_and(|c| !is_true_literal(c))
            .then_some(super::LoopEdge {
                facts: false_exit,
                notes: fx.ended_shared_narrowing.clone(),
            });
        if !then_extra.is_empty() {
            fx.narrowed.extend_facts(then_extra);
        }
        fx.loop_depth += 1;
        fx.loop_break_facts
            .push((fx.scopes.len(), Vec::new(), Vec::new()));
        let (mut body, terminates) = self.check_branch(&f.body, fx);
        let mut update_edges = fx
            .loop_break_facts
            .last()
            .map_or_else(Vec::new, |(_, _, edges)| edges.clone());
        if !terminates {
            update_edges.push(super::LoopEdge {
                facts: fx.narrowed.clone(),
                notes: fx.ended_shared_narrowing.clone(),
            });
        }
        Self::join_flow_edges(fx, update_edges);
        let step_statements = f.update.as_ref().map(|update| {
            let (statements, prefix) = fx.with_synthetic_owner(
                super::SyntheticOwnerKind::ForUpdate(self.pos(update.span())),
                |fx| self.check_expr_stmt(update, fx),
            );
            let mut step = prefix.into_statements();
            step.extend(statements);
            step
        });
        self.check_loop_stores(&body, cond.iter(), &kept);
        if !cond_prefix.is_empty() {
            self.check_loop_stores(&cond_prefix.0, std::iter::empty(), &kept);
        }
        if let Some(step) = &step_statements {
            self.check_loop_stores(step, std::iter::empty(), &kept);
        }
        fx.loop_depth -= 1;
        Self::join_loop_exits(fx, false_exit);
        fx.pop_scope();

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
            pending_work: None,
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
                    pending_work: None,
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
            self.reject_subset(RejectionSite::AsyncForOf, "`for await…of` requires the Promise object/iterator surface, which is not in the language", pos);
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
            fx.pop_scope();
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

        self.end_loop_narrowing(&pos, fx);

        let base = super::LoopEdge {
            facts: fx.narrowed.clone(),
            notes: fx.ended_shared_narrowing.clone(),
        };
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
                pending_work: None,
                kind: ExprKind::Local(name.clone(), elem_ty.clone(), false),
                ty: elem_ty.clone(),
                pos: binding_pos.clone(),
            };
            self.bind_pattern_from(&pattern, &element, mutable, false, fx, &mut prologue);
        } else {
            self.declare_local(
                &name,
                Local {
                    annotated: false,
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
        let kept = fx.narrowed.clone();
        fx.loop_depth += 1;
        fx.loop_break_facts
            .push((fx.scopes.len() - 1, Vec::new(), Vec::new()));
        let (body, _) = self.check_branch(&f.body, fx);
        self.check_loop_stores(&body, std::iter::empty(), &kept);
        fx.loop_depth -= 1;
        fx.pop_scope();
        Self::join_loop_exits(fx, Some(base));
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
            pending_work: None,
            kind: ExprKind::Local(subject_name.clone(), subject_ty.clone(), false),
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
                pending_work: None,
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
                pending_work: None,
                kind: ExprKind::Local(step_name.clone(), step_ty.clone(), false),
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
                        pending_work: None,
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
                        pending_work: None,
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
                    pending_work: None,
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
                    self.reject_subset(
                        RejectionSite::ForOfVarBinding,
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
                self.reject_subset(
                    RejectionSite::ForOfAwaitUsing,
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
                self.reject_subset(
                    RejectionSite::ForOfBindingKind,
                    "`for…of` requires a `const` or `let` identifier binding",
                    self.pos(head.span()),
                );
                return None;
            }
        };
        if declarations.len() != 1 {
            self.reject_subset(
                RejectionSite::ForOfBindingCount,
                "`for…of` requires exactly one identifier binding",
                declaration_pos,
            );
            return None;
        }
        let binding = &declarations[0];
        if binding.init.is_some() {
            self.reject_subset(
                RejectionSite::ForOfBindingInitializer,
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
                                    RejectionSite::ForOfEntries,
                                    "`entries()` yields a pair, but the language has no tuple type",
                                    prop_pos,
                                );
                                return (recv, None, Type::Error, false);
                            }
                            if !call.args.is_empty() {
                                self.reject_subset(
                                    if call.args.iter().all(|argument| argument.spread.is_some()) {
                                        RejectionSite::IteratorMethodArgumentCount
                                    } else {
                                        RejectionSite::IteratorMethodValueArgumentCount
                                    },
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
                                RejectionSite::ForOfKeys,
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
                RejectionSite::ForOfMap,
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
                RejectionSite::ForOfUserClass,
                format!(
                    "`for…of` cannot make user class `{class}` iterable (invariant 5): \
                     that requires `Symbol.iterator`, and `Symbol` is a permanent non-goal"
                ),
                subject.pos.clone(),
            );
        } else {
            self.reject_subset(
                RejectionSite::ForOfSubject,
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
                        self.require_expr_assignable(&checked, disc_ty, fx, "the case label");
                        if !alias_members_seen.insert(index) {
                            *alias_labels_valid = false;
                            self.reject_subset(RejectionSite::LiteralAliasDuplicateCase, format!(
                                    "duplicate case label {label:?} for string-literal union alias `{alias_name}`"
                                ), checked.pos.clone());
                        }
                    } else {
                        *alias_labels_valid = false;
                        self.reject_subset(RejectionSite::LiteralAliasUnknownCase, format!(
                                "case label {label:?} is not a member of string-literal union alias `{alias_name}`"
                            ), checked.pos.clone());
                    }
                }
                _ => {
                    *alias_labels_valid = false;
                    self.reject_subset(RejectionSite::AliasCaseNonLiteral, format!(
                            "case labels for string-literal union alias `{alias_name}` must be string literals naming a member"
                        ), checked.pos.clone());
                }
            }
        } else if (self.involves_type_parameter(disc_ty)
            || self.involves_type_parameter(&checked.ty))
            && self.generic_overlap(disc_ty, &checked.ty)
        {
            // §143 rule 1a: case labels compare values, rather than assign them.
        } else {
            self.require_expr_assignable(&checked, disc_ty, fx, "the case label");
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
            self.reject_subset(RejectionSite::SwitchDiscriminantKind, format!(
                    "switch discriminants are integers, enums, strings, or string-literal union aliases; got `{}`",
                    name
                ), disc.pos.clone());
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
        let mut fallthrough: Option<super::NarrowingFacts> = None;
        fx.switch_break_facts.push((fx.loop_depth, Vec::new()));
        let reachable = fx.flow_reachable;
        let mut cases = Vec::new();
        for (case_index, case) in sw.cases.iter().enumerate() {
            if let Some(scope) = fx.scopes.last_mut() {
                scope.switch_case = Some(case_index);
                scope.dispose_on_exit = false;
            }
            fx.flow_reachable = reachable;
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
            self.end_scope_narrowing(fx);
            exit_notes.extend_facts(fx.ended_shared_narrowing.iter().cloned());
            fallthrough = (!terminates).then(|| fx.narrowed.clone());
            cases.push(hir::SwitchCase {
                test,
                body,
                pos: case_pos,
            });
        }
        fx.flow_reachable = reachable;
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
        fx.ended_shared_narrowing.extend_facts(exit_notes);
        fx.finish_narrowing_join(&note_paths);
        fx.pop_scope();
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
                self.reject_subset(RejectionSite::LiteralAliasSwitchCoverage, format!(
                        "non-exhaustive switch over string-literal union alias `{alias_name}`; missing case labels: {missing}"
                    ), pos.clone());
            }
        }
        out.push(hir::Stmt::Switch { disc, cases, pos });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Pos;
    use crate::hir::BinOp;

    fn narrow_paths(
        cond: &hir::Expr,
        apparent: impl Fn(&Type) -> Type,
    ) -> (Vec<String>, Vec<String>) {
        let (yes, no) = super::super::narrowing::leaf_paths(cond, apparent, &[]);
        (
            yes.into_iter().map(|fact| fact.key).collect(),
            no.into_iter().map(|fact| fact.key).collect(),
        )
    }

    fn expr(kind: ExprKind, ty: Type) -> hir::Expr {
        hir::Expr {
            pending_work: None,
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
                    ExprKind::Local("p".into(), nullable.clone(), false),
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
                    ExprKind::Local("p".into(), nullable.clone(), false),
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
                        ExprKind::Local(
                            "sampler".into(),
                            Type::Class(crate::types::ClassId(0)),
                            false,
                        ),
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
