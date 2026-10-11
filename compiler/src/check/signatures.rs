use super::*;
use crate::check::rejection::RejectionSite;

impl<'p> Checker<'p> {
    /// Pass B for a mirror file: resolves type aliases first (so later
    /// declarations may reference them), then boundary-struct shapes,
    /// foreign-function signatures, and ambient-constant values. Runs with
    /// `in_boundary` set so the boundary null forms are legal.
    pub(super) fn resolve_mirror_signatures(&mut self, file: usize) {
        let module = &self.prog.files[file].module;
        // Sub-pass 1: type aliases.
        for item in &module.body {
            let Some(decl) = module_decl(item) else {
                continue;
            };
            if let ast::Decl::TsTypeAlias(t) = decl {
                if string_alias_members(&t.type_ann).is_some()
                    || wire_alias_literal(&t.type_ann).is_some()
                {
                    continue;
                }
                let ty = self.resolve_type(&t.type_ann);
                self.type_aliases.insert(t.id.sym.to_string(), ty);
            }
        }
        // Sub-pass 2: struct shapes, foreign signatures, ambient consts.
        for item in &module.body {
            let Some(decl) = module_decl(item) else {
                continue;
            };
            match decl {
                ast::Decl::Class(c) if c.class.type_params.is_none() => {
                    let name = c.ident.sym.to_string();
                    if let Some(&id) = self.class_ids.get(&self.declaration_symbol(file, &name)) {
                        self.resolve_class_shape(id, &c.class, c.declare);
                    }
                }
                ast::Decl::Fn(f) => {
                    let name = f.ident.sym.to_string();
                    let pos = self.pos(f.ident.span);
                    self.allow_wire_alias_boundary = true;
                    let sig = self.resolve_fn_sig(&f.function, pos.clone());
                    self.allow_wire_alias_boundary = false;
                    for parameter in &sig.params {
                        if Self::contains_string_alias(parameter.ty())
                            && !Self::supported_wire_alias_boundary_type(parameter.ty())
                        {
                            self.reject_subset(RejectionSite::WireAliasNestedForeignParameter, format!(
                                    "wire-mapped aliases are supported only as direct foreign-function parameters or array-descriptor elements; `{}` nests one inside another boundary type",
                                    parameter.name
                                ), pos.clone());
                        }
                    }
                    if Self::contains_string_alias(&sig.ret)
                        && !self.prog.files[file]
                            .provenance
                            .completions
                            .contains_key(&name)
                        && !matches!(sig.ret, Type::StringAlias(_))
                    {
                        self.reject_subset(RejectionSite::WireAliasNestedForeignReturn, "wire-mapped aliases are supported only as direct foreign-function returns", pos.clone());
                    }
                    let mut params = Vec::with_capacity(sig.params.len());
                    for (index, parameter) in sig.params.iter().enumerate() {
                        let ast_parameter = f.function.params.get(index);
                        let parameter_pos =
                            ast_parameter.map_or_else(|| pos.clone(), |p| self.pos(p.span));
                        let foreign_provenance = self.foreign_parameter_provenance(
                            file,
                            &name,
                            &parameter.name,
                            parameter.ty(),
                            parameter_pos.clone(),
                        );
                        if matches!(parameter.ty(), Type::Func(_)) {
                            self.reject_subset(
                                RejectionSite::ForeignDirectCallback,
                                format!(
                                    "mirror `{}` foreign function `{}` parameter `{}` is a \
                                     direct callback; callbacks are supported only as fields \
                                     of mirrored boundary structs",
                                    self.prog.files[file].name, name, parameter.name
                                ),
                                parameter_pos.clone(),
                            );
                        }
                        params.push(hir::Param {
                            escapes: false,
                            default_can_raise: false,
                            name: parameter.name.clone(),
                            ty: parameter.ty().clone(),
                            default: None,
                            foreign_provenance,
                            pos: parameter_pos,
                        });
                    }
                    let completion_result =
                        self.check_completion_result(file, &name, &sig.ret, pos.clone());
                    let unsupported_return = match &sig.ret {
                        Type::Str => Some("a string view"),
                        Type::Array(_) => Some("an array descriptor"),
                        Type::Func(_) => Some("a direct callback"),
                        _ => None,
                    };
                    if let Some(kind) = unsupported_return {
                        self.reject_subset(
                            RejectionSite::ForeignReturnProvenance,
                            format!(
                                "mirror `{}` foreign function `{}` returns {kind}; foreign \
                                 string-view, descriptor, and callback returns are unsupported \
                                 because return provenance cannot be represented by the boundary \
                                 vocabulary",
                                self.prog.files[file].name, name
                            ),
                            pos.clone(),
                        );
                    }
                    let Some(mirror) = self.foreign_mirror_ids.get(&file).copied() else {
                        self.reject_subset(
                            RejectionSite::ForeignFunctionHeaderMissing,
                            format!(
                                "mirror `{}` has no header identity for foreign function `{}`",
                                self.prog.files[file].name, name
                            ),
                            pos,
                        );
                        continue;
                    };
                    self.foreign_defs.push(hir::ForeignFn {
                        completion_result,
                        name: name.clone(),
                        params,
                        ret: sig.ret.clone(),
                        mirror,
                        pos,
                    });
                    self.foreign_sigs.insert(name, sig);
                }
                ast::Decl::Var(v) => {
                    for d in &v.decls {
                        // Pass A reports every other form and binds its
                        // name poisoned (compiler.md §136.1 rule 2).
                        let (ast::Pat::Ident(binding), Some(value)) =
                            (&d.name, mirror_const_value(v, d))
                        else {
                            continue;
                        };
                        // A mirror flag member (§13.2): tsc accepts a bare
                        // literal initializer on an ambient const only
                        // without a type annotation, so the value travels
                        // here and the `u64` flag type is supplied by rule.
                        let symbol = self.declaration_symbol(file, &binding.id.sym);
                        self.ambient_int_consts
                            .insert(symbol.clone(), (value, Type::U64));
                        self.global_sigs.insert(
                            symbol,
                            GlobalSig {
                                function_value_required: None,
                                state: crate::check::initializer::TypeState::decided(Type::U64),

                                initializer: None,
                                mutable: false,
                            },
                        );
                    }
                }
                _ => {}
            }
        }
    }

    // ----- imports -----

    pub(super) fn resolve_imports(&mut self) {
        for file in 0..self.prog.files.len() {
            let module = &self.prog.files[file].module;
            let mut additions: Vec<(String, ScopeItem, Pos, bool)> = Vec::new();
            for item in &module.body {
                if let ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportNamed(export)) = item {
                    self.record_discovery_export(export);
                    continue;
                }
                let ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import)) = item else {
                    continue;
                };
                if import.type_only {
                    if let Some(ast::ImportSpecifier::Namespace(namespace)) =
                        import.specifiers.first()
                    {
                        let pos = self.pos(namespace.local.span);
                        self.reject_subset(
                            RejectionSite::TypeOnlyNamespaceImport,
                            "`import type * as` is outside the decided surface",
                            pos.clone(),
                        );
                        additions.push((
                            namespace.local.sym.to_string(),
                            ScopeItem::Namespace {
                                module: None,
                                source: import.src.value.to_string(),
                            },
                            pos,
                            false,
                        ));
                        continue;
                    }
                }
                let raw = import.src.value.to_string();
                if raw == "node:fs/promises" {
                    let enabled = self.enabled_modules.iter().any(|m| m == &raw);
                    if !enabled {
                        self.file_error(
                            RejectionSite::FileModuleDisabled,
                            "enable this module with --enable-module node:fs/promises",
                            self.pos(import.src.span),
                        );
                    }
                    if import.specifiers.is_empty() {
                        self.file_error(
                            RejectionSite::FileModuleImportForm,
                            super::file_module::FORMS,
                            self.pos(import.src.span),
                        );
                    }
                    for spec in &import.specifiers {
                        if let ast::ImportSpecifier::Named(named) = spec {
                            let local = named.local.sym.to_string();
                            let imported = named
                                .imported
                                .as_ref()
                                .map_or_else(|| local.clone(), |n| n.atom().to_string());
                            let item = match imported.as_str() {
                                "readFile" if enabled => ScopeItem::StandardFile(false),
                                "writeFile" if enabled => ScopeItem::StandardFile(true),
                                "readFile" | "writeFile" => ScopeItem::Poisoned,
                                _ => {
                                    self.file_error(
                                        RejectionSite::FileModuleMember,
                                        super::file_module::FORMS,
                                        self.pos(spec.span()),
                                    );
                                    ScopeItem::Poisoned
                                }
                            };
                            additions.push((
                                local,
                                item,
                                self.pos(named.local.span),
                                type_only_import(import, named),
                            ));
                        } else {
                            self.file_error(
                                RejectionSite::FileModuleImportForm,
                                super::file_module::FORMS,
                                self.pos(spec.span()),
                            );
                        }
                    }
                    continue;
                }
                if raw == super::text_module::SPECIFIER {
                    self.text_module_import(import, &mut additions);
                    continue;
                }
                let stem = normalize_module_specifier(&raw);
                let Some(target) = self.prog.files.iter().position(|f| f.stem == stem) else {
                    let pos = self.pos(import.src.span);
                    let missing_message =
                        format!("imported module `{raw}` is not among the program's files");
                    if self.poison_missing_modules.contains(&stem) {
                        if import.specifiers.is_empty() {
                            self.resolution_error(
                                RejectionSite::NamespaceImportTargetMissing,
                                missing_message,
                                pos,
                            );
                            continue;
                        }
                        let mut names = Vec::new();
                        let mut namespace_local = None;
                        for spec in &import.specifiers {
                            if let ast::ImportSpecifier::Namespace(namespace) = spec {
                                let local = namespace.local.sym.to_string();
                                additions.push((
                                    local.clone(),
                                    ScopeItem::Namespace {
                                        module: None,
                                        source: raw.clone(),
                                    },
                                    self.pos(namespace.local.span),
                                    false,
                                ));
                                namespace_local = Some(local);
                                continue;
                            }
                            let ast::ImportSpecifier::Named(named) = spec else {
                                self.reject_subset(
                                    if raw.starts_with('.') {
                                        RejectionSite::PoisonedRelativeDefaultImport
                                    } else {
                                        RejectionSite::PoisonedDefaultImport
                                    },
                                    "only named imports are in the decided surface",
                                    self.pos(spec.span()),
                                );
                                continue;
                            };
                            let local = named.local.sym.to_string();
                            let imported = named
                                .imported
                                .as_ref()
                                .map_or_else(|| local.clone(), |name| name.atom().to_string());
                            additions.push((
                                local.clone(),
                                ScopeItem::Poisoned,
                                self.pos(named.local.span),
                                type_only_import(import, named),
                            ));
                            names.push((imported, local));
                        }
                        if !names.is_empty() || namespace_local.is_some() {
                            self.poisoned_imports.push(hir::PoisonedImport {
                                module: raw,
                                names,
                                namespace: namespace_local,
                                pos,
                            });
                        }
                    } else {
                        self.resolution_error(
                            if import.specifiers.is_empty() {
                                RejectionSite::NamedImportModuleMissing
                            } else {
                                RejectionSite::NamedImportUnresolvedModule
                            },
                            missing_message,
                            pos,
                        );
                        for spec in &import.specifiers {
                            if let ast::ImportSpecifier::Namespace(namespace) = spec {
                                additions.push((
                                    namespace.local.sym.to_string(),
                                    ScopeItem::Namespace {
                                        module: None,
                                        source: raw.clone(),
                                    },
                                    self.pos(namespace.local.span),
                                    false,
                                ));
                            }
                            if let ast::ImportSpecifier::Named(named) = spec {
                                additions.push((
                                    named.local.sym.to_string(),
                                    ScopeItem::Poisoned,
                                    self.pos(named.local.span),
                                    type_only_import(import, named),
                                ));
                            }
                        }
                    }
                    continue;
                };
                for spec in &import.specifiers {
                    if let ast::ImportSpecifier::Namespace(namespace) = spec {
                        let pos = self.pos(namespace.local.span);
                        let item = ScopeItem::Namespace {
                            module: Some(target),
                            source: raw.clone(),
                        };
                        additions.push((namespace.local.sym.to_string(), item, pos, false));
                        continue;
                    }
                    let ast::ImportSpecifier::Named(named) = spec else {
                        let pos = self.pos(spec.span());
                        self.reject_subset(
                            RejectionSite::DefaultImport,
                            "only named imports are in the decided surface",
                            pos,
                        );
                        continue;
                    };
                    let local = named.local.sym.to_string();
                    let imported = named.imported.as_ref();
                    let imported_name =
                        imported.map_or_else(|| local.clone(), |name| name.atom().to_string());
                    let imported_pos = self.pos(imported.map_or(named.local.span, Spanned::span));
                    let pos = self.pos(named.local.span);
                    let type_only = type_only_import(import, named);
                    match self.exports[target].get(&imported_name) {
                        Some(item) => additions.push((local, item.clone(), pos, type_only)),
                        None if self
                            .rejected_module_exports
                            .get(&stem)
                            .is_some_and(|names| names.contains(&imported_name)) =>
                        {
                            additions.push((local, ScopeItem::Poisoned, pos, type_only));
                        }
                        None => {
                            self.resolution_error(
                                RejectionSite::NamedImportMemberMissing,
                                format!("`{imported_name}` is not exported by `{raw}`"),
                                imported_pos,
                            );
                            additions.push((local, ScopeItem::Poisoned, pos, type_only));
                        }
                    }
                }
            }
            for (name, item, pos, type_only) in additions {
                self.register_scope_binding(
                    file,
                    &name,
                    ScopeBinding {
                        item,
                        imported: true,
                        type_only,
                    },
                    pos,
                );
            }
        }
    }

    // ----- pass B: signatures -----

    pub(super) fn resolve_signatures(&mut self, file: usize) {
        let module = &self.prog.files[file].module;
        for item in &module.body {
            let Some(decl) = module_decl(item) else {
                continue;
            };
            match decl {
                ast::Decl::Class(c) if c.class.type_params.is_none() => {
                    let name = c.ident.sym.to_string();
                    if let Some(&id) = self.class_ids.get(&self.declaration_symbol(file, &name)) {
                        self.resolve_class_shape(id, &c.class, c.declare);
                    }
                }
                ast::Decl::Fn(f) if f.function.type_params.is_none() => {
                    let name = f.ident.sym.to_string();
                    let sig = self.resolve_fn_sig(&f.function, self.pos(f.ident.span));
                    self.fn_sigs
                        .insert(self.declaration_symbol(file, &name), sig);
                }
                ast::Decl::Var(v) => {
                    for d in &v.decls {
                        let ast::Pat::Ident(binding) = &d.name else {
                            continue;
                        };
                        let name = binding.id.sym.to_string();
                        let state = match &binding.type_ann {
                            Some(ann) => TypeState::decided(self.resolve_type(&ann.type_ann)),
                            None => TypeState::Undecided,
                        };
                        let ty = state.ty().clone();
                        if self.is_context_affine_type(&ty) {
                            self.reject_subset(
                                RejectionSite::WorkerEndpointModuleGlobal,
                                "Worker, Inbox, and Outbox values may not be module globals",
                                self.pos(binding.id.span),
                            );
                        }
                        self.global_sigs.insert(
                            self.declaration_symbol(file, &name),
                            GlobalSig {
                                function_value_required: None,
                                state,
                                initializer: d.init.as_ref().map(|e| self.initializer(e, None)),
                                mutable: v.kind == ast::VarDeclKind::Let,
                            },
                        );
                    }
                }
                _ => {}
            }
        }
    }

    /// Resolves a function signature (pass B), including Q34's required
    /// `Promise<T>` view for async declarations.
    pub(crate) fn resolve_fn_sig(&mut self, f: &ast::Function, pos: Pos) -> FnSig {
        let saved = self.task_group_parameters;
        self.task_group_parameters = !f.is_async && !f.is_generator && !self.in_boundary;
        let params = self.resolve_params(&f.params);
        self.task_group_parameters = saved;
        let mut sig = self.resolve_fn_result(f, pos);
        sig.params = params;
        sig
    }

    fn resolve_fn_result(&mut self, f: &ast::Function, pos: Pos) -> FnSig {
        let params = Vec::new();
        if f.is_async {
            if f.is_generator {
                self.reject_subset(
                    RejectionSite::AsyncGeneratorFunction,
                    "a function cannot be both async and a generator",
                    pos,
                );
                return FnSig {
                    generic: f.type_params.is_some(),
                    params,
                    ret: Type::Error,
                    is_generator: false,
                    is_async: true,
                    yield_known: true,
                };
            }
            let ret = match &f.return_type {
                Some(ann) => self.resolve_async_return(&ann.type_ann),
                None => {
                    self.reject_subset(
                        RejectionSite::AsyncReturnAnnotationMissing,
                        "async functions require an explicit `Promise<T>` return annotation",
                        pos,
                    );
                    Type::Error
                }
            };
            return FnSig {
                generic: f.type_params.is_some(),
                params,
                ret,
                is_generator: false,
                is_async: true,
                yield_known: true,
            };
        }
        if f.is_generator {
            // The yield type is inferred from the body (checked in
            // source order); a `Generator<T>` annotation, when present,
            // seeds it.
            let mut yield_ty = None;
            if let Some(ann) = &f.return_type {
                if let ast::TsType::TsTypeRef(r) = &*ann.type_ann {
                    if let ast::TsEntityName::Ident(id) = &r.type_name {
                        if id.sym.as_ref() == "Generator" {
                            if let Some(args) = &r.type_params {
                                if !args.params.is_empty() {
                                    let resolved = self.resolve_type(&ann.type_ann);
                                    yield_ty = Some(match self.apparent_type(&resolved) {
                                        Type::Generator(element) => *element,
                                        _ => Type::Error,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            let known = yield_ty.is_some();
            return FnSig {
                generic: f.type_params.is_some(),
                params,
                // An unknown yield type is a placeholder that `yield_known`
                // marks, not a poisoned component, so the generator form
                // keeps it (compiler.md §132 rule 2a).
                ret: Type::Generator(Box::new(yield_ty.unwrap_or(Type::Error))),
                is_generator: true,
                is_async: false,
                yield_known: known,
            };
        }
        let ret = match &f.return_type {
            Some(ann) => self.resolve_result_type(&ann.type_ann),
            None => {
                self.reject_subset(
                    RejectionSite::FunctionReturnAnnotationMissing,
                    "function return types must be annotated",
                    pos,
                );
                Type::Error
            }
        };
        FnSig {
            generic: f.type_params.is_some(),
            params,
            ret,
            is_generator: false,
            is_async: false,
            yield_known: true,
        }
    }

    pub(crate) fn resolve_params(&mut self, params: &[ast::Param]) -> Vec<ParamSig> {
        params
            .iter()
            .map(|p| self.resolve_param_pat(&p.pat))
            .collect()
    }

    fn inferred_pattern_parameter_name(&mut self) -> String {
        let id = self.next_pattern_id;
        self.next_pattern_id += 1;
        format!("[[pattern#{id}.parameter]]")
    }

    pub(crate) fn resolve_param_pat(&mut self, pat: &ast::Pat) -> ParamSig {
        match pat {
            ast::Pat::Ident(binding) => {
                if binding.id.optional {
                    // C7: optional parameters without defaults imply an
                    // observable `undefined`.
                    let pos = self.pos(binding.id.span);
                    self.reject_subset(
                        RejectionSite::OptionalParameter,
                        "optional parameters imply `undefined`; use a default value or `T | null`",
                        pos,
                    );
                }
                let ty = match &binding.type_ann {
                    Some(ann) => {
                        let saved = self.task_group_type;
                        self.task_group_type = self.task_group_parameters
                            && type_reference_name(Some(&ann.type_ann)) == Some("TaskGroup");
                        let ty = self.resolve_type(&ann.type_ann);
                        self.task_group_type = saved;
                        ty
                    }
                    None => {
                        let pos = self.pos(binding.id.span);
                        self.reject_subset(
                            if self.generic_callback_context {
                                RejectionSite::GenericCallbackParameterAnnotationMissing
                            } else {
                                RejectionSite::NamedParameterAnnotationMissing
                            },
                            "parameters require a type annotation",
                            pos,
                        );
                        Type::Error
                    }
                };
                ParamSig {
                    annotated: binding
                        .type_ann
                        .as_ref()
                        .is_some_and(|ann| self.written_type(&ann.type_ann)),
                    name: binding.id.sym.to_string(),
                    state: crate::check::initializer::TypeState::decided(ty),
                    initializer: None,
                    has_default: false,
                }
            }
            ast::Pat::Assign(assign) => {
                let infer_name = match &*assign.left {
                    ast::Pat::Ident(binding) if binding.type_ann.is_none() => {
                        Some(binding.id.sym.to_string())
                    }
                    ast::Pat::Array(array) if array.type_ann.is_none() && !self.in_boundary => {
                        Some(self.inferred_pattern_parameter_name())
                    }
                    ast::Pat::Object(object) if object.type_ann.is_none() && !self.in_boundary => {
                        Some(self.inferred_pattern_parameter_name())
                    }
                    _ => None,
                };
                let mut inner = if let Some(name) = infer_name {
                    ParamSig {
                        annotated: false,
                        name,
                        state: TypeState::Undecided,
                        initializer: None,
                        has_default: true,
                    }
                } else {
                    self.resolve_param_pat(&assign.left)
                };
                inner.initializer = Some(self.initializer(&assign.right, None));
                inner.has_default = true;
                inner
            }
            // A binding pattern parameter takes its type from the
            // pattern's own annotation, and binds into checker-generated
            // storage that the entry prologue reads (§107.2). A boundary
            // signature has no body, so it holds no such prologue.
            ast::Pat::Array(_) | ast::Pat::Object(_) if !self.in_boundary => {
                let annotation = match pat {
                    ast::Pat::Array(array) => array.type_ann.as_deref(),
                    ast::Pat::Object(object) => object.type_ann.as_deref(),
                    _ => None,
                };
                let ty = match annotation {
                    Some(annotation) => self.resolve_type(&annotation.type_ann),
                    None => {
                        let pos = self.pos(pat.span());
                        self.reject_subset(
                            if self.generic_callback_context {
                                RejectionSite::GenericCallbackParameterAnnotationMissing
                            } else {
                                RejectionSite::PatternParameterAnnotationMissing
                            },
                            "parameters require a type annotation",
                            pos,
                        );
                        Type::Error
                    }
                };
                let id = self.next_pattern_id;
                self.next_pattern_id += 1;
                ParamSig {
                    annotated: annotation.is_some_and(|ann| self.written_type(&ann.type_ann)),
                    name: format!("[[pattern#{id}.parameter]]"),
                    state: TypeState::decided(ty),
                    initializer: None,
                    has_default: false,
                }
            }
            other => {
                let pos = self.pos(other.span());
                self.reject_subset(
                    if matches!(other, ast::Pat::Rest(_)) {
                        RejectionSite::RestParameter
                    } else if self.mirror_array_parameter_noniterable(other) {
                        RejectionSite::MirrorArrayParameterNonIterable
                    } else {
                        RejectionSite::MirrorBindingPatternParameter
                    },
                    "parameter pattern outside the decided surface",
                    pos,
                );
                ParamSig {
                    annotated: false,
                    name: String::new(),
                    state: TypeState::Rejected,
                    initializer: None,
                    has_default: false,
                }
            }
        }
    }

    /// Binds a parameter's pattern names at function entry, in parameter
    /// order (§107.2). Answers with the statements the body starts with.
    pub(super) fn bind_parameter_patterns(
        &mut self,
        params: Vec<(ParamSig, ast::Pat)>,
        fx: &mut FnCtx,
    ) -> Vec<hir::Stmt> {
        let mut prologue = Vec::new();
        for (signature, pat) in params {
            let pat = match &pat {
                ast::Pat::Assign(assign) => assign.left.as_ref(),
                other => other,
            };
            let pattern = pattern::classify(pat);
            match &pattern {
                pattern::Pattern::Rejected(rejection) => self.reject_pattern(rejection, fx),
                _ if pattern.is_destructuring() => {
                    let source = hir::Expr {
                        pending_work: None,
                        kind: hir::ExprKind::Local(
                            signature.name.clone(),
                            signature.ty().clone(),
                            signature.annotated,
                        ),
                        ty: signature.ty().clone(),
                        pos: self.pos(pattern.span()),
                    };
                    self.bind_pattern_from(
                        &pattern,
                        &source,
                        true,
                        signature.annotated,
                        fx,
                        &mut prologue,
                    );
                }
                _ => {}
            }
        }
        prologue
    }
}

/// Answers whether an import specifier binds a type only: the whole
/// declaration is `import type`, or the specifier is written `type X`
/// (compiler.md §134 rule 1).
pub(super) fn type_only_import(
    import: &ast::ImportDecl,
    named: &ast::ImportNamedSpecifier,
) -> bool {
    import.type_only || named.is_type_only
}

impl Checker<'_> {
    /// A written annotation names no parameter of the enclosing generic source.
    pub(crate) fn written_type(&self, ty: &ast::TsType) -> bool {
        match ty {
            ast::TsType::TsKeywordType(_) => true,
            ast::TsType::TsTypeRef(reference) => {
                let name_ok = match &reference.type_name {
                    ast::TsEntityName::Ident(name) => !self.subst.contains_key(name.sym.as_ref()),
                    ast::TsEntityName::TsQualifiedName(_) => true,
                };
                name_ok
                    && reference
                        .type_params
                        .as_ref()
                        .is_none_or(|args| args.params.iter().all(|arg| self.written_type(arg)))
            }
            ast::TsType::TsFnOrConstructorType(ast::TsFnOrConstructorType::TsFnType(function)) => {
                function.type_params.is_none()
                    && self.written_type(&function.type_ann.type_ann)
                    && function.params.iter().all(|param| match param {
                        ast::TsFnParam::Ident(binding) => binding
                            .type_ann
                            .as_ref()
                            .is_some_and(|ann| self.written_type(&ann.type_ann)),
                        _ => false,
                    })
            }
            ast::TsType::TsArrayType(array) => self.written_type(&array.elem_type),
            ast::TsType::TsParenthesizedType(paren) => self.written_type(&paren.type_ann),
            ast::TsType::TsUnionOrIntersectionType(
                ast::TsUnionOrIntersectionType::TsUnionType(union),
            ) => union.types.iter().all(|ty| self.written_type(ty)),
            // Unsupported forms provide no proof for a loop store.
            _ => false,
        }
    }
}
