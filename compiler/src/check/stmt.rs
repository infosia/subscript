//! Statement checking: declarations, control flow, and the C7 flow
//! narrowing that admits member access on `Ref | null` values.

use super::FactSet;
use crate::check::rejection::RejectionSite;
use std::collections::HashSet;

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::hir;
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

impl<'p> Checker<'p> {
    /// compiler.md §207.1 rules 2a and 4: the front end moves each local
    /// function that reads no binding of an enclosing function, and
    /// records why each other one stays in its block. Binds the name and
    /// returns true if the front end has a record for the declaration.
    fn check_local_function_outcome(&mut self, function: &ast::FnDecl, fx: &mut FnCtx) -> bool {
        use crate::parse::local_function::LocalFunctionOutcome;
        let Some(outcome) = self
            .prog
            .local_functions
            .outcomes
            .get(&function.ident.span.lo)
        else {
            return false;
        };
        let name = function.ident.sym.as_ref();
        let local = Local {
            annotated: false,
            ty: Type::Error,
            mutable: false,
            async_origins: Default::default(),
            caught: false,
            function_value_required: None,
        };
        match outcome {
            LocalFunctionOutcome::Duplicate => {
                let pos = self.pos(function.ident.span);
                self.declare_local(name, local, pos, fx);
            }
            LocalFunctionOutcome::Poisoned => {
                self.declare_in_context(name, local, fx);
            }
            LocalFunctionOutcome::Reads(read) => {
                self.declare_in_context(name, local, fx);
                self.reject_subset(
                    RejectionSite::LocalFunctionReadsEnclosingBinding,
                    format!("local function `{}` {}", read.function, read.read),
                    self.prog.pos_at(read.at),
                );
            }
        }
        true
    }

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
        let (_terminates, prefix) = fx.with_synthetic_owner(
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
                    if let ast::Decl::Fn(function) = other {
                        if self.check_local_function_outcome(function, fx) {
                            return false;
                        }
                    }
                    let pos = self.pos(other.span());
                    for name in super::exports::declaration_names(other) {
                        self.declare_in_context(
                            name.sym.as_ref(),
                            Local {
                                annotated: false,
                                ty: Type::Error,
                                mutable: false,
                                async_origins: Default::default(),
                                caught: false,
                                function_value_required: None,
                            },
                            fx,
                        );
                    }
                    self.reject_subset(
                        match other {
                            ast::Decl::Class(_) => RejectionSite::LocalClassDeclaration,
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
                    // §289 rule 1a: only this call can discard a non-void catch result.
                    let statement_call = match super::expr::unparen_expr(&e.expr) {
                        ast::Expr::Await(awaited) => {
                            match super::expr::unparen_expr(&awaited.arg) {
                                ast::Expr::Call(call) => Some(call.span),
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    let previous =
                        std::mem::replace(&mut self.void_catch_statement, statement_call);
                    out.extend(self.check_expr_stmt(&e.expr, fx));
                    self.void_catch_statement = previous;
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
                ast::Stmt::DoWhile(d) => {
                    self.check_do_while(d, fx, out);
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
                        let depth = fx
                            .scopes
                            .iter()
                            .rposition(|scope| scope.is_switch)
                            .map_or(fx.scopes.len(), |depth| depth + 1);
                        let facts = Self::flow_edge_leaving_scopes(fx, depth);
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
                    for s in &b.stmts {
                        self.check_stmt(s, fx, &mut inner);
                    }
                    let terminates = !super::fallthrough::sequence_can_fall_through(&inner);
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
                // compiler.md §204 rule 4: `debugger;` is an empty statement.
                ast::Stmt::Empty(_) | ast::Stmt::Debugger(_) => false,
                other => {
                    let pos = self.pos(other.span());
                    self.reject_subset(
                        match other {
                            ast::Stmt::ForIn(_) => RejectionSite::ForInStatement,
                            ast::Stmt::Labeled(_) => RejectionSite::LabeledStatement,
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
        let terminates = !super::fallthrough::sequence_can_fall_through(&out[start..]);
        fx.flow_reachable &= !terminates;
        terminates
    }

    pub(super) fn check_let(&mut self, v: &ast::VarDecl, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
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
            let group_local = !mutable
                && !dispose
                && matches!(d.init.as_deref(),
                Some(ast::Expr::New(new)) if matches!(&*new.callee,
                    ast::Expr::Ident(id) if id.sym.as_ref() == "TaskGroup"));
            let saved_group_type = self.task_group_type;
            self.task_group_type = group_local
                && pattern_type_ann(&d.name).is_some_and(|ann| {
                    super::type_reference_name(Some(&ann.type_ann)) == Some("TaskGroup")
                });
            let ann = pattern_type_ann(&d.name).map(|ann| self.resolve_type(&ann.type_ann));
            self.task_group_type = saved_group_type;
            let saved_group_local = self.task_group_local;
            self.task_group_local = group_local;
            self.aggregate_type_site = saved_divergence;
            if d.init.is_none()
                && ann
                    .as_ref()
                    .is_some_and(|ty| self.apparent_type(ty) == Type::Error)
            {
                self.bind_error_names(&super::pattern::collect_names(&d.name), fx);
                self.task_group_local = saved_group_local;
                continue;
            }
            let saved_lambda_initializer = fx.lambda_initializer.take();
            self.prepare_named_lambda(d, mutable, &name, ann.as_ref(), fx);
            let mut init = if let Some(init_ast) = &d.init {
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
                fx.lambda_initializer = saved_lambda_initializer;
                self.task_group_local = saved_group_local;
                continue;
            };
            fx.lambda_initializer = saved_lambda_initializer;
            self.task_group_local = saved_group_local;
            let annotated =
                pattern_type_ann(&d.name).is_some_and(|ann| self.written_type(&ann.type_ann));
            let ty = match ann {
                Some(ann) => {
                    if !matches!(init.kind, hir::ExprKind::Unassigned) {
                        self.require_expr_assignable(&mut init, &ann, fx, "the initializer");
                    }
                    ann
                }
                None => self.inferred_initializer_type(&init, pos.clone()),
            };
            if dispose && !matches!(&self.apparent_type(&ty), Type::Error) {
                let shape = self.apparent_type(&ty);
                let resource_type = match &shape {
                    Type::Nullable(inner, _) => inner.as_ref(),
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
            let string_literal = (!mutable && !dispose && pattern_type_ann(&d.name).is_none())
                .then(|| {
                    d.init
                        .as_deref()
                        .and_then(|source| self.checked_string_literal_operand(source, &init, fx))
                })
                .flatten();
            let async_origins = self.expr_async_origins(&init, fx);
            let function_value_required = if pattern_type_ann(&d.name).is_some() {
                None
            } else {
                d.init
                    .as_deref()
                    .and_then(|source| self.inferred_function_required(source, &init, fx))
            };
            self.declare_local(
                &name,
                Local {
                    annotated,
                    ty: ty.clone(),
                    mutable,
                    async_origins,
                    caught: false,
                    function_value_required,
                },
                pos.clone(),
                fx,
            );
            if let (Some(text), Some(scope)) = (string_literal, fx.scopes.last_mut()) {
                scope.string_literals.insert(name.clone(), text);
            }
            if pattern_type_ann(&d.name).is_none() {
                if let Some(source) = &d.init {
                    // An element captured in a callable alias has its decided
                    // signature; calling the alias does not read the list again.
                    let sources =
                        if matches!(super::expr::unparen_expr(source), ast::Expr::Member(_))
                            && matches!(self.apparent_type(&init.ty), Type::Func(_))
                        {
                            None
                        } else {
                            self.lambda_value_sources(source, fx)
                        };
                    if let Some(scope) = fx.scopes.last_mut() {
                        scope.result_sources.insert(name.clone(), sources);
                    }
                }
            }
            self.remember_named_lambda(&name, d, mutable, &init, fx);
            self.remember_adapter_defaults(&name, mutable, annotated, &init, fx);
            if let Some(identity) = integer_key {
                if let Some(scope) = fx.scopes.last_mut() {
                    scope.const_keys.insert(name.clone(), identity);
                }
            }
            if !matches!(init.kind, hir::ExprKind::Unassigned)
                && matches!(self.apparent_type(&ty), Type::Nullable(_, _))
                && !matches!(
                    self.apparent_type(&init.ty),
                    Type::Nullable(_, _) | Type::Null | Type::Error
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

    pub(in crate::check) fn check_return(
        &mut self,
        r: &ast::ReturnStmt,
        fx: &mut FnCtx,
        out: &mut Vec<hir::Stmt>,
    ) {
        let pos = self.pos(r.span);
        let (ret, is_generator) = fx
            .frames
            .last()
            .map(|f| (f.ret.clone(), f.is_generator))
            .unwrap_or((Type::Error, false));
        let poisoned_result = self.apparent_type(&ret) == Type::Error
            && fx.frames.last().is_some_and(|frame| frame.is_lambda);
        if let Some(arg) = &r.arg {
            if fx
                .frames
                .last()
                .is_some_and(|frame| frame.inferred_void.is_some())
            {
                self.check_inferred_void_return(arg, fx, &pos, out);
                return;
            }
        }
        let value = match &r.arg {
            Some(arg) if poisoned_result => Some(self.err_expr(self.pos(arg.span()))),
            Some(arg) if !is_generator && ret != Type::Void && self.is_undefined_token(arg, fx) => {
                Some(self.check_return_undefined(arg, &ret))
            }
            Some(arg) => {
                if is_generator {
                    self.reject_subset(
                        RejectionSite::GeneratorReturnValue,
                        "generator return values are not in the decided surface",
                        pos.clone(),
                    );
                    None
                } else if ret == Type::Void {
                    match self.check_void_return(arg, fx, &pos, out) {
                        Some(checked) => Some(checked),
                        None => return,
                    }
                } else {
                    if self.split_mixed_return(arg, &ret, fx, out) {
                        return;
                    }
                    // compiler.md §236 rule 3: a returned handle takes the
                    // context `Promise<T>`.
                    let context = self.return_context(arg, &ret, fx);
                    let diagnostics = self.diags.len();
                    let kind = fx
                        .frames
                        .last()
                        .map_or(crate::check::expr::ContextKind::Contextual, |frame| {
                            frame.result_kind
                        });
                    let checked = self.check_expr_as(arg, Some(&context), kind, fx);
                    let Some(mut checked) =
                        self.adopt_returned_handle(checked, &ret, fx, &pos, out)
                    else {
                        return;
                    };
                    self.require_return_assignable(&mut checked, &ret, fx);
                    // A rejected return reads its handles, so it gives no
                    // second diagnostic (compiler.md §236.4).
                    if self.transfers_async_origins(&checked.ty) || self.diags.len() != diagnostics
                    {
                        let origins = self.expr_async_origins(&checked, fx);
                        fx.handle_return_async_origins(&origins);
                    }
                    Some(checked)
                }
            }
            None => {
                if self.apparent_type(&ret).absence() == Some(crate::types::Absence::Undefined) {
                    self.reject_subset(
                        RejectionSite::UndefinedResultBareReturn,
                        "a function whose result is `T | undefined` returns with `return undefined;`, not `return;`",
                        pos.clone(),
                    );
                } else if ret != Type::Void
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

    /// `return e` in a function with a `void` result (compiler.md §237).
    /// A constructor and a setter reject each operand (rule 1). Else the
    /// literal `undefined` is `return;` (§250 rules 1 to 3). A
    /// conditional splits into one return per leaf, unless each leaf of
    /// an async function is a handle (rule 2). A `Promise<void>` handle
    /// adopts (rule 3). A `void` operand is `e; return;` (rule 2). Any
    /// operand of a lambda that is not async and whose result is `void`
    /// from its context is `e; return;`, and `e` follows the rules of an
    /// expression statement (rule 4). Another operand is rejected (rule
    /// 5), and the rejected return is returned for the caller to place.
    fn check_void_return(
        &mut self,
        arg: &ast::Expr,
        fx: &mut FnCtx,
        pos: &crate::Pos,
        out: &mut Vec<hir::Stmt>,
    ) -> Option<hir::Expr> {
        if fx
            .frames
            .last()
            .is_some_and(|frame| frame.rejects_return_operand)
        {
            // Rule 1: a constructor or a setter rejects each operand.
            let diagnostics = self.diags.len();
            let checked = self.check_expr(arg, None, fx);
            self.reject_void_return_value(diagnostics, fx, pos);
            let origins = self.expr_async_origins(&checked, fx);
            fx.handle_return_async_origins(&origins);
            return Some(checked);
        }
        // compiler.md §250 rules 1 to 3: the literal `undefined` is
        // `return;`.
        if self.is_undefined_token(arg, fx) {
            out.push(hir::Stmt::Return {
                value: None,
                pos: pos.clone(),
            });
            return None;
        }
        // `(e)` has the type of `e`.
        let arg = super::expr::unparen_expr(arg);
        let ret = Type::Void;
        let context = self.return_context(arg, &ret, fx);
        if context == ret && matches!(arg, ast::Expr::Cond(_)) {
            // The leaves of one return give one diagnostic at the return.
            let outer = fx.frames.last_mut().and_then(|frame| {
                frame
                    .void_split
                    .is_none()
                    .then(|| frame.void_split.replace((pos.clone(), false)))
            });
            self.split_return(arg, fx, out);
            if outer.is_some() {
                if let Some(frame) = fx.frames.last_mut() {
                    frame.void_split = None;
                }
            }
            return None;
        }
        if context != ret {
            let diagnostics = self.diags.len();
            let checked = self.check_expr(arg, Some(&context), fx);
            let checked = self.adopt_returned_handle(checked, &ret, fx, pos, out)?;
            self.reject_void_return_value(diagnostics, fx, pos);
            let origins = self.expr_async_origins(&checked, fx);
            fx.handle_return_async_origins(&origins);
            return Some(checked);
        }
        let bare_return = hir::Stmt::Return {
            value: None,
            pos: pos.clone(),
        };
        if fx.frames.last().is_some_and(|frame| frame.contextual_void) {
            out.extend(self.check_expr_stmt(arg, fx));
            out.push(bare_return);
            return None;
        }
        let diagnostics = self.diags.len();
        let (statements, ty) = self.check_void_operand(arg, fx);
        // compiler.md §318 rule 2: the present path adopts through §236.
        if matches!(arg, ast::Expr::OptChain(_))
            && fx.frames.last().is_some_and(|frame| frame.is_async)
            && matches!(self.apparent_type(&ty), Type::AsyncHandle(inner) if self.apparent_type(&inner) == Type::Void)
        {
            let mut adopted = Vec::new();
            if !self.adopt_void_return_statements(statements, fx, pos, &mut adopted) {
                self.reject_void_return_value(diagnostics, fx, pos);
                self.read_statement_handles(&adopted, fx);
            }
            out.extend(adopted);
            out.push(bare_return);
            return None;
        }
        if !matches!(self.apparent_type(&ty), Type::Void | Type::Error) {
            self.reject_void_return_value(diagnostics, fx, pos);
            // The rejected return reads its handles, so a handle gives no
            // second diagnostic.
            self.read_statement_handles(&statements, fx);
        }
        out.extend(statements);
        out.push(bare_return);
        None
    }

    /// Rejects a value operand of `return` in a function with a declared
    /// `void` result (compiler.md §237 rule 5). An operand that gave a
    /// diagnostic gives no second one. The leaves of a split return give
    /// one diagnostic, at the return.
    fn reject_void_return_value(&mut self, diagnostics: usize, fx: &mut FnCtx, pos: &crate::Pos) {
        if self.diags.len() != diagnostics {
            return;
        }
        let pos = match fx
            .frames
            .last_mut()
            .and_then(|frame| frame.void_split.as_mut())
        {
            Some((_, true)) => return,
            Some((outer, reported)) => {
                *reported = true;
                outer.clone()
            }
            None => pos.clone(),
        };
        self.reject_subset(
            RejectionSite::ExplicitVoidReturnValue,
            "a `void` function cannot return a value",
            pos,
        );
    }

    /// Reads the handles of the expressions of `statements`, and of the
    /// branches of each `if`.
    pub(in crate::check) fn read_statement_handles(
        &mut self,
        statements: &[hir::Stmt],
        fx: &mut FnCtx,
    ) {
        for statement in statements {
            match statement {
                hir::Stmt::Expr(checked) => {
                    let origins = self.expr_async_origins(checked, fx);
                    fx.handle_async_origins(&origins);
                }
                hir::Stmt::If { then, .. } => self.read_statement_handles(then, fx),
                _ => {}
            }
        }
    }

    /// Checks `return undefined;` (compiler.md §195.2 rule 1). In a
    /// function whose result is `T | undefined`, the value is the HIR of
    /// `return null;`.
    fn check_return_undefined(&mut self, arg: &ast::Expr, ret: &Type) -> hir::Expr {
        let pos = self.pos(arg.span());
        match self.apparent_type(ret) {
            Type::Nullable(_, crate::types::Absence::Undefined) => hir::Expr {
                pending_work: None,
                kind: hir::ExprKind::Null,
                ty: Type::Null,
                pos,
            },
            Type::Error => self.err_expr(pos),
            _ => {
                let name = self.type_name(ret);
                self.reject_subset(
                    RejectionSite::UndefinedReturnValue,
                    format!(
                        "{}; this function returns `{name}`, not `T | undefined`",
                        crate::check::diagnostic_text::UNDEFINED_PLACES
                    ),
                    pos.clone(),
                );
                self.err_expr(pos)
            }
        }
    }

    pub(super) fn require_bool(&mut self, cond: &hir::Expr) {
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
    pub(super) fn check_branch(&mut self, s: &ast::Stmt, fx: &mut FnCtx) -> (Vec<hir::Stmt>, bool) {
        let reachable = fx.flow_reachable;
        fx.scopes.push(Default::default());
        let mut out = Vec::new();
        let _terminates = match s {
            ast::Stmt::Block(b) => {
                self.reserve_block_declarations(&b.stmts, fx);
                for s in &b.stmts {
                    self.check_stmt(s, fx, &mut out);
                }
                !super::fallthrough::sequence_can_fall_through(&out)
            }
            single => self.check_stmt(single, fx, &mut out),
        };
        self.end_scope_narrowing(fx);
        fx.pop_scope();
        fx.flow_reachable = reachable;
        let terminates = !super::fallthrough::sequence_can_fall_through(&out);
        (out, terminates)
    }

    fn check_if(&mut self, i: &ast::IfStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) -> bool {
        let pos = self.pos(i.span);
        let cond = self.check_truth_expr(&i.test, fx);
        self.require_bool(&cond);
        let (then_extra, else_extra) = self.narrowing_paths(&cond, fx);

        let base = fx.narrowed.clone();
        let mut note_paths: HashSet<_> = base.union(&fx.ended_shared_narrowing).cloned().collect();

        let initial_finally = fx.finally_ended_narrowing.clone();
        let initial_loop_notes = fx.loop_ended_narrowing.clone();
        let initial_notes = fx.ended_shared_narrowing.clone();
        fx.narrowed.retain(|key| !else_extra.contains(key));
        fx.narrowed.extend_facts(then_extra.clone());
        fx.replace_finally_causes_at_facts();
        let (then_stmts, _) = self.check_branch(&i.cons, fx);
        let then_term = !super::fallthrough::sequence_can_fall_through(&then_stmts);
        let then_finally = fx.finally_ended_narrowing.clone();
        let then_loop_notes = fx.loop_ended_narrowing.clone();
        let then_notes = fx.ended_shared_narrowing.clone();
        let then_facts = fx.narrowed.clone();
        fx.finally_ended_narrowing = initial_finally;
        fx.loop_ended_narrowing = initial_loop_notes;
        fx.ended_shared_narrowing = initial_notes;
        fx.narrowed = base.clone();
        fx.narrowed.retain(|key| !then_extra.contains(key));
        fx.narrowed.extend_facts(else_extra);
        fx.replace_finally_causes_at_facts();
        let (els_stmts, _) = match &i.alt {
            Some(alt) => {
                let (stmts, term) = self.check_branch(alt, fx);
                (Some(stmts), term)
            }
            None => (None, false),
        };
        let else_term = els_stmts
            .as_ref()
            .is_some_and(|body| !super::fallthrough::sequence_can_fall_through(body));
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
        if !then_term && !else_term {
            fx.merge_finally_causes(&then_finally, &then_facts);
        }
        fx.narrowed = if then_term {
            else_facts
        } else if else_term {
            then_facts
        } else {
            then_facts.intersection(&else_facts).cloned().collect()
        };
        fx.ended_shared_narrowing.extend_facts(then_notes);
        if !then_term {
            if else_term {
                fx.finally_ended_narrowing = then_finally.clone();
            }
            if else_term {
                fx.loop_ended_narrowing = then_loop_notes;
            } else {
                fx.merge_loop_notes(&then_loop_notes);
            }
        }
        fx.finish_narrowing_join(&note_paths);
        out.push(hir::Stmt::If {
            cond,
            then: then_stmts,
            els: els_stmts,
            pos,
        });
        !super::fallthrough::sequence_can_fall_through(&out[out.len() - 1..])
    }

    /// Resolves the single declaration accepted on the left of
    /// `for…of`.
    pub(super) fn for_of_binding<'a>(
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
    pub(super) fn check_for_of_subject(
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
    pub(super) fn check_for_of_subject_under_flag(
        &mut self,
        expression: &ast::Expr,
        fx: &mut FnCtx,
    ) -> (hir::Expr, Option<hir::ForOfKind>, Type, bool) {
        if let ast::Expr::Call(call) = expression {
            // A fused view call enters the checker here (compiler.md §230 rule 3).
            type Subject = (hir::Expr, Option<hir::ForOfKind>, Type, bool);
            let poisoned = |checker: &Self, (subject, _, element, _): &Subject| {
                checker.poisoned_call(subject)
                    || matches!(checker.apparent_type(element), Type::Error)
            };
            return self.checked_call(call, poisoned, |checker| {
                checker.check_for_of_call_subject(call, expression, fx)
            });
        }
        let subject = self.check_expr(expression, None, fx);
        self.for_of_subject_from(subject)
    }

    /// [`Self::check_for_of_subject_under_flag`] for a call subject.
    fn check_for_of_call_subject(
        &mut self,
        call: &ast::CallExpr,
        expression: &ast::Expr,
        fx: &mut FnCtx,
    ) -> (hir::Expr, Option<hir::ForOfKind>, Type, bool) {
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
                            let subject = self.check_method_call_on(recv, prop, call, fx, call_pos);
                            return self.for_of_subject_from(subject);
                        }
                        let prop_pos = self.pos(prop.span);
                        if name == "entries" {
                            self.reject_subset(
                                if matches!(self.apparent_type(&recv.ty), Type::Array(_)) { RejectionSite::ForOfArrayEntries } else { RejectionSite::ForOfEntries },
                                if matches!(self.apparent_type(&recv.ty), Type::Array(_)) { "array `entries()` yields an index/value pair; use an index loop" } else { "`entries()` yields a pair, but the language has no tuple type" },
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

    fn check_switch(&mut self, sw: &ast::SwitchStmt, fx: &mut FnCtx, out: &mut Vec<hir::Stmt>) {
        let pos = self.pos(sw.span);
        let mut disc = self.check_expr(&sw.discriminant, None, fx);
        let subject_literal = self.checked_string_literal_operand(&sw.discriminant, &disc, fx);
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
                    "switch discriminants are integers, enums, strings, or string-literal union aliases; got `{}`{}",
                    name,
                    self.null_test_fix(&[&disc.ty])
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
        let dispatch_finally = fx.finally_ended_narrowing.clone();
        let dispatch_notes = fx.ended_shared_narrowing.clone();
        let mut exit_notes = dispatch_notes.clone();
        let mut dispatch = fx.narrowed.clone();
        let mut fallthrough: Option<super::LoopEdge> = None;
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
            fx.finally_ended_narrowing = dispatch_finally.clone();
            let case_pos = self.pos(case.span);
            let test = if let Some(t) = &case.test {
                let (checked, _) = fx.with_synthetic_owner(
                    super::SyntheticOwnerKind::SwitchCase(self.pos(t.span())),
                    |fx| {
                        self.check_switch_case_test(
                            t,
                            (&disc, subject_literal.as_deref()),
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
                fx.merge_finally_causes(&previous.finally_ended, &previous.facts);
                fx.narrowed.retain(|key| previous.facts.contains(key));
            }
            let mut body = Vec::new();
            for s in &case.cons {
                self.check_stmt(s, fx, &mut body);
            }
            let terminates = !super::fallthrough::sequence_can_fall_through(&body);
            self.end_scope_narrowing(fx);
            exit_notes.extend_facts(fx.ended_shared_narrowing.iter().cloned());
            fallthrough = (!terminates).then(|| super::LoopEdge {
                facts: fx.narrowed.clone(),
                notes: fx.ended_shared_narrowing.clone(),
                finally_ended: fx.finally_ended_narrowing.clone(),
            });
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
            exits.push(super::LoopEdge {
                facts: dispatch,
                notes: dispatch_notes,
                finally_ended: dispatch_finally,
            });
        }
        Self::join_flow_edges(fx, exits);
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
        self.finish_literal_string_switch(&mut disc, &mut cases, subject_literal.as_deref());
        out.push(hir::Stmt::Switch { disc, cases, pos });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Pos;
    use crate::hir::BinOp;
    use crate::hir::ExprKind;

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
