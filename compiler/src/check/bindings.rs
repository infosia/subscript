use super::*;
use crate::check::rejection::{diagnostic, RejectionSite};

impl<'p> Checker<'p> {
    /// Reserves the names that declarations own in one statement list.
    pub(crate) fn reserve_block_declarations(
        &mut self,
        statements: &[ast::Statement],
        fx: &mut FnCtx<'p>,
    ) {
        let Some(scope) = fx.scopes.last_mut() else {
            return;
        };
        for statement in statements {
            let Some(declaration) = statement.as_declaration() else {
                continue;
            };
            if let ast::Declaration::FunctionDeclaration(function) = declaration {
                // §207.1 rule 2a: a duplicate local function binds its
                // name at its declaration, so the later declaration of
                // the name reports the duplicate. A read before it is a
                // read of a rejected name.
                if matches!(
                    self.prog
                        .local_functions
                        .outcomes
                        .get(&function.declaration_name_span().start),
                    Some(crate::parse::local_function::LocalFunctionOutcome::Duplicate)
                ) {
                    scope
                        .rejected_local_names
                        .insert(function.declaration_name().to_string());
                    continue;
                }
                scope.insert_local(
                    function.declaration_name().to_string(),
                    Local {
                        annotated: false,
                        ty: Type::Error,
                        mutable: false,
                        async_origins: Default::default(),
                        caught: false,
                        function_value_required: None,
                    },
                    true,
                );
                continue;
            }
            let declarators = match declaration {
                ast::Declaration::VariableDeclaration(declaration)
                    if declaration.kind != ast::VariableDeclarationKind::Var =>
                {
                    &declaration.declarations
                }

                _ => continue,
            };
            for declarator in declarators {
                // A binding pattern reserves every name it binds, so a
                // read before it names the order, not the binding (C14).
                for binding in pattern::collect_names(&declarator.id) {
                    let name = binding.name.to_string();
                    scope
                        .declaration_positions
                        .entry(name.clone())
                        .or_insert_with(|| self.prog.pos(binding.span));
                    if !scope.vars.contains_key(&name) {
                        scope.pending.insert(name);
                    }
                }
            }
        }
        self.form_lambda_groups(statements, fx);
    }

    /// Declares one local and reports a duplicate in the current scope.
    pub(crate) fn declare_local(&mut self, name: &str, local: Local, pos: Pos, fx: &mut FnCtx) {
        if !local.mutable {
            if let Some(scope) = fx.scopes.last_mut() {
                scope.const_keys.entry(name.to_owned()).or_insert_with(|| {
                    format!("const:{}:{}:{}:{name}", pos.file, pos.line, pos.col)
                });
            }
        }
        let in_switch = fx.scopes.last().is_some_and(|scope| scope.is_switch);
        if !self.declare_in_context(name, local, fx) {
            if let Some(scope) = fx.scopes.last_mut() {
                scope.duplicate_declarations.insert(name.to_string());
            }
            let message = if in_switch {
                format!("duplicate declaration of `{name}` in one switch body")
            } else {
                format!("duplicate declaration of `{name}` in one scope")
            };
            self.reject_subset(RejectionSite::DuplicateLocalDeclaration, message, pos);
        }
    }

    /// Reports a rejected binding pattern one time, and binds every name
    /// in the pattern with the error type, so no `unknown name` follows
    /// it (§107.4).
    pub(crate) fn reject_pattern(
        &mut self,
        rejection: &pattern::PatternRejection<'_>,
        fx: &mut FnCtx<'p>,
    ) {
        let pos = self.pos(rejection.span);
        self.reject_subset(rejection.site, rejection.message, pos);
        self.bind_error_names(&rejection.names, fx);
    }

    /// Binds each name with the error type. A read of one adds no
    /// diagnostic.
    pub(crate) fn bind_error_names(&mut self, names: &[&ast::BindingIdentifier], fx: &mut FnCtx) {
        for binding in names {
            let name = binding.name.to_string();
            fx.discard_pending(&name);
            self.declare_in_context(
                &name,
                Local {
                    annotated: false,
                    ty: Type::Error,
                    mutable: true,
                    async_origins: Default::default(),
                    caught: false,
                    function_value_required: None,
                },
                fx,
            );
        }
    }

    /// True when the source type carries the pattern's reads: an array
    /// pattern reads by index, a field pattern reads by field name
    /// (§107.1).
    fn pattern_source_fits(&mut self, pattern: &pattern::Pattern<'_>, ty: &Type) -> bool {
        if matches!(&self.apparent_type(ty), Type::Error) {
            return false;
        }
        if matches!(&self.apparent_type(ty), Type::IterResult(_)) {
            let message = "an iterator result cannot supply a binding pattern; use `const r = it.next(); if (r.done) ...`";
            let pos = self.pos(pattern.span());
            // TypeScript rejects array patterns and unknown result fields.
            let tsc_accepts = matches!(pattern, pattern::Pattern::Fields { bindings, .. }
                if bindings.iter().all(|binding| matches!(&binding.source,
                    pattern::BindingSource::Field(name) if name == "done" || name == "value")));
            if tsc_accepts {
                self.reject_subset(
                    RejectionSite::IteratorBindingUnsupportedMembers,
                    message,
                    pos,
                );
            } else {
                self.reject_subset(RejectionSite::IteratorBindingUnknownMembers, message, pos);
            }
            return false;
        }
        let (fits, shape) = match pattern {
            pattern::Pattern::Array { .. } => (
                matches!(
                    &self.apparent_type(ty),
                    Type::Array(_) | Type::FixedArray(_, _)
                ),
                "an array binding pattern reads a `T[]` or a `FixedArray<T, N>`",
            ),
            pattern::Pattern::Fields { .. } => (
                matches!(&self.apparent_type(ty), Type::Class(_)),
                "a field binding pattern reads a class",
            ),
            _ => return true,
        };
        if !fits {
            let name = self.type_name(ty);
            let pos = self.pos(pattern.span());
            self.reject_subset(
                if matches!(pattern, pattern::Pattern::Array { .. })
                    && self.apparent_type(ty) == Type::Str
                {
                    RejectionSite::BindingPatternSourceKind
                } else {
                    RejectionSite::BindingPatternNonIterableSource
                },
                format!("{shape}; the source is `{name}`"),
                pos,
            );
        }
        fits
    }

    /// Binds an accepted pattern's names out of `source`, which the
    /// caller already evaluated into storage. Every read is the ordinary
    /// checked element read or field read (§107.2).
    pub(crate) fn bind_pattern_from(
        &mut self,
        pattern: &pattern::Pattern<'_>,
        source: &hir::Expr,
        mutable: bool,
        annotated: bool,
        fx: &mut FnCtx<'p>,
        out: &mut Vec<hir::Stmt>,
    ) {
        let bindings = match pattern {
            pattern::Pattern::Array { bindings, .. }
            | pattern::Pattern::Fields { bindings, .. } => bindings,
            _ => return,
        };
        if !self.pattern_source_fits(pattern, &source.ty) {
            let names: Vec<&ast::BindingIdentifier> =
                bindings.iter().map(|binding| binding.binding).collect();
            self.bind_error_names(&names, fx);
            return;
        }
        for binding in bindings {
            let pos = self.pos(binding.binding.span);
            let value = match &binding.source {
                pattern::BindingSource::Element(index) => {
                    let index = hir::Expr {
                        pending_work: None,
                        kind: hir::ExprKind::Int(i64::from(*index)),
                        ty: Type::I32,
                        pos: pos.clone(),
                    };
                    self.check_index(source.clone(), index, pos.clone(), fx)
                }
                pattern::BindingSource::Field(field) => {
                    self.member_on(source.clone(), field, pos.clone(), None, fx)
                }
            };
            let name = binding.binding.name.to_string();
            let ty = value.ty.clone();
            let async_origins = self.expr_async_origins(&value, fx);
            self.declare_local(
                &name,
                Local {
                    annotated,
                    ty: ty.clone(),
                    mutable,
                    async_origins,
                    caught: false,
                    function_value_required: None,
                },
                pos.clone(),
                fx,
            );
            let prefix = format!("{name}.");
            fx.narrowed
                .retain(|key| key != &name && !key.starts_with(&prefix));
            out.push(hir::Stmt::Let {
                name,
                ty,
                mutable,
                dispose: false,
                init: value,
                pos,
            });
        }
    }

    /// Evaluates `source` one time into checker-generated storage, then
    /// binds the pattern's names out of it (§107.2).
    pub(crate) fn bind_pattern(
        &mut self,
        pattern: &pattern::Pattern<'_>,
        source: hir::Expr,
        mutable: bool,
        annotated: bool,
        fx: &mut FnCtx<'p>,
        out: &mut Vec<hir::Stmt>,
    ) {
        let id = self.next_pattern_id;
        self.next_pattern_id += 1;
        let name = format!("[[pattern#{id}.source]]");
        let ty = source.ty.clone();
        let pos = source.pos.clone();
        self.declare_pattern_storage(&name, &ty, self.expr_async_origins(&source, fx), fx);
        let place = hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::Local(name.clone(), ty.clone(), false),
            ty: ty.clone(),
            pos: pos.clone(),
        };
        out.push(hir::Stmt::Let {
            name,
            ty,
            mutable: false,
            dispose: false,
            init: source,
            pos,
        });
        self.bind_pattern_from(pattern, &place, mutable, annotated, fx, out);
    }

    /// Gives checker-generated pattern storage the origins of the value
    /// that it holds, so each name bound out of it carries them
    /// (`compiler.md` §188.1 rule 2). Storage with no origin stays
    /// undeclared.
    pub(crate) fn declare_pattern_storage(
        &mut self,
        name: &str,
        ty: &Type,
        async_origins: super::AsyncOrigins,
        fx: &mut FnCtx<'p>,
    ) {
        if async_origins.is_empty() {
            return;
        }
        self.declare_in_context(
            name,
            Local {
                annotated: false,
                ty: ty.clone(),
                mutable: false,
                async_origins,
                caught: false,
                function_value_required: None,
            },
            fx,
        );
    }

    pub(super) fn resolution_error(
        &mut self,
        site: RejectionSite,
        message: impl Into<String>,
        pos: Pos,
    ) {
        let mut diagnostic = diagnostic(site, message, pos);
        diagnostic.resolution = true;
        self.diags.push(diagnostic);
    }

    pub(crate) fn pos(&self, span: oxc_span::Span) -> Pos {
        self.prog.pos(span)
    }

    /// True for the three Q35 Context-affine runtime handle types, including
    /// their nullable local form.
    pub(crate) fn is_context_affine_type(&self, ty: &Type) -> bool {
        match &self.apparent_type(ty) {
            Type::Worker(..) | Type::Inbox(_) | Type::Outbox(_) => true,
            Type::Nullable(inner, _) => self.is_context_affine_type(inner),
            _ => false,
        }
    }
}
