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
            if let ast::Declaration::TSTypeAliasDeclaration(t) = decl {
                if string_alias_members(&t.type_annotation).is_some()
                    || wire_alias_literal(&t.type_annotation).is_some()
                {
                    continue;
                }
                let mut ty = self.resolve_type(&t.type_annotation);
                let name = t.id.name.to_string();
                let pos = self.pos(t.id.span);
                self.reject_mirror_nullable_view(file, &name, &ty, pos);
                if let Type::Func(function) = &mut ty {
                    function.callback_typedef = Some(name.clone());
                }

                self.type_aliases.insert(name, ty);
            }
        }
        // Sub-pass 2: struct shapes, foreign signatures, ambient consts.
        for item in &module.body {
            let Some(decl) = module_decl(item) else {
                continue;
            };
            match decl {
                ast::Declaration::ClassDeclaration(c) if c.type_parameters.is_none() => {
                    let name = c.declaration_name().to_string();
                    if let Some(&id) = self.class_ids.get(&self.declaration_symbol(file, &name)) {
                        self.resolve_class_shape(id, c, c.declare);
                        for field in self.classes[id.0].fields.clone() {
                            self.reject_mirror_nullable_view(
                                file,
                                &format!("{name}.{}", field.name),
                                &field.ty,
                                field.pos,
                            );
                        }
                        let class_pos = self.pos(c.declaration_name_span());
                        self.reject_mirror_member_signatures(file, id, &name, class_pos);
                    }
                }
                ast::Declaration::FunctionDeclaration(f) => {
                    let name = f.declaration_name().to_string();
                    let pos = self.pos(f.declaration_name_span());
                    self.allow_wire_alias_boundary = true;
                    let sig = self.resolve_fn_sig(f, pos.clone());
                    self.allow_wire_alias_boundary = false;
                    self.reject_mirror_nullable_view(file, &name, &sig.ret, pos.clone());
                    for parameter in &sig.params {
                        self.reject_mirror_nullable_view(
                            file,
                            &format!("{name}.{}", parameter.name),
                            parameter.ty(),
                            pos.clone(),
                        );
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
                        let parameters = super::function_parameters(f, self.prog.allocator);
                        let ast_parameter = parameters.get(index);
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
                            escape_kind: crate::hir::EscapeKind::None,
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
                ast::Declaration::VariableDeclaration(v) => {
                    for d in &v.declarations {
                        // Pass A reports every other form and binds its
                        // name poisoned (compiler.md §136.1 rule 2).
                        let (ast::BindingPattern::BindingIdentifier(binding), Some(value)) =
                            (&d.id, mirror_const_value(v, d))
                        else {
                            continue;
                        };
                        // A mirror flag member (§13.2): tsc accepts a bare
                        // literal initializer on an ambient const only
                        // without a type annotation, so the value travels
                        // here and the `u64` flag type is supplied by rule.
                        let symbol = self.declaration_symbol(file, &binding.name);
                        self.ambient_int_consts
                            .insert(symbol.clone(), (value, Type::U64));
                        self.global_sigs.insert(
                            symbol,
                            GlobalSig {
                                annotated: false,
                                named_lambda: false,
                                lambda_defaults: None,
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

    /// Applies §217.1 rule 3 and §218.1 rule 9 to the constructor and the methods of a mirror
    /// class, in name order.
    fn reject_mirror_member_signatures(
        &mut self,
        file: usize,
        class: ClassId,
        name: &str,
        pos: Pos,
    ) {
        let signature = &self.class_sigs[class.0];
        let mut types = Vec::new();
        for parameter in signature.ctor.iter().flatten() {
            types.push((
                format!("{name}.constructor.{}", parameter.name),
                parameter.ty().clone(),
            ));
        }
        let mut methods: Vec<_> = signature.methods.iter().collect();
        methods.sort_by_key(|(method, _)| method.as_str());
        for (method, sig) in methods {
            types.push((format!("{name}.{method}"), sig.ret.clone()));
            for parameter in &sig.params {
                types.push((
                    format!("{name}.{method}.{}", parameter.name),
                    parameter.ty().clone(),
                ));
            }
        }
        for (declaration, ty) in types {
            self.reject_mirror_nullable_view(file, &declaration, &ty, pos.clone());
        }
    }

    /// §217.1 rule 3 and §218.1 rule 9: the C ABI gives `T[] | null` and
    /// `string | null` no layout. The check reads
    /// the resolved type of each mirror declaration, so an alias, a callback
    /// signature, and a nested form all reach it.
    fn reject_mirror_nullable_view(&mut self, file: usize, declaration: &str, ty: &Type, pos: Pos) {
        if self.apparent_type(ty).holds_nullable_view() {
            let type_name = self.type_name(ty);
            self.reject_subset(
                RejectionSite::MirrorNullableView,
                format!(
                    "mirror `{}` declaration `{declaration}` has `{type_name}`; the C ABI \
                     gives a nullable array or string no layout",
                    self.prog.files[file].name
                ),
                pos,
            );
        }
    }

    // ----- imports -----

    pub(super) fn resolve_imports(&mut self) {
        for file in 0..self.prog.files.len() {
            let module = &self.prog.files[file].module;
            let mut additions: Vec<(String, ScopeItem, Pos, bool)> = Vec::new();
            for item in &module.body {
                match item {
                    ast::Statement::ExportNamedDeclaration(export) => {
                        self.record_discovery_export(super::exports::NamedExport::local(export));
                        continue;
                    }
                    ast::Statement::ExportFromDeclaration(export) => {
                        self.record_discovery_export(super::exports::NamedExport::remote(export));
                        continue;
                    }
                    _ => {}
                }
                let ast::Statement::ImportDeclaration(import) = item else {
                    continue;
                };
                if import.import_kind == ast::ImportOrExportKind::Type {
                    if let Some(ast::ImportDeclarationSpecifier::ImportNamespaceSpecifier(
                        namespace,
                    )) = import
                        .specifiers
                        .as_ref()
                        .and_then(|specifiers| specifiers.first())
                    {
                        let pos = self.pos(namespace.local.span);
                        self.reject_subset(
                            RejectionSite::TypeOnlyNamespaceImport,
                            "`import type * as` is outside the decided surface",
                            pos.clone(),
                        );
                        additions.push((
                            namespace.local.name.to_string(),
                            ScopeItem::Namespace {
                                module: None,
                                source: import.source.value.to_string(),
                            },
                            pos,
                            false,
                        ));
                        continue;
                    }
                }
                let raw = import.source.value.to_string();
                if raw == "node:fs/promises" {
                    let enabled = self.enabled_modules.iter().any(|m| m == &raw);
                    if !enabled {
                        self.file_error(
                            RejectionSite::FileModuleDisabled,
                            "enable this module with --enable-module node:fs/promises",
                            self.pos(import.source.span),
                        );
                    }
                    if import
                        .specifiers
                        .as_ref()
                        .is_none_or(|specifiers| specifiers.is_empty())
                    {
                        self.file_error(
                            RejectionSite::FileModuleImportForm,
                            super::file_module::FORMS,
                            self.pos(import.source.span),
                        );
                    }
                    for spec in import.specifiers.iter().flatten() {
                        if let ast::ImportDeclarationSpecifier::ImportSpecifier(named) = spec {
                            let local = named.local.name.to_string();
                            let imported = named.imported.name().to_string();
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
                    let pos = self.pos(import.source.span);
                    let missing_message =
                        format!("imported module `{raw}` is not among the program's files");
                    if self.poison_missing_modules.contains(&stem) {
                        if import
                            .specifiers
                            .as_ref()
                            .is_none_or(|specifiers| specifiers.is_empty())
                        {
                            self.resolution_error(
                                RejectionSite::NamespaceImportTargetMissing,
                                missing_message,
                                pos,
                            );
                            continue;
                        }
                        let mut names = Vec::new();
                        let mut namespace_local = None;
                        for spec in import.specifiers.iter().flatten() {
                            if let ast::ImportDeclarationSpecifier::ImportNamespaceSpecifier(
                                namespace,
                            ) = spec
                            {
                                let local = namespace.local.name.to_string();
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
                            let ast::ImportDeclarationSpecifier::ImportSpecifier(named) = spec
                            else {
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
                            let local = named.local.name.to_string();
                            let imported = named.imported.name().to_string();
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
                            if import
                                .specifiers
                                .as_ref()
                                .is_none_or(|specifiers| specifiers.is_empty())
                            {
                                RejectionSite::NamedImportModuleMissing
                            } else {
                                RejectionSite::NamedImportUnresolvedModule
                            },
                            missing_message,
                            pos,
                        );
                        for spec in import.specifiers.iter().flatten() {
                            if let ast::ImportDeclarationSpecifier::ImportNamespaceSpecifier(
                                namespace,
                            ) = spec
                            {
                                additions.push((
                                    namespace.local.name.to_string(),
                                    ScopeItem::Namespace {
                                        module: None,
                                        source: raw.clone(),
                                    },
                                    self.pos(namespace.local.span),
                                    false,
                                ));
                            }
                            if let ast::ImportDeclarationSpecifier::ImportSpecifier(named) = spec {
                                additions.push((
                                    named.local.name.to_string(),
                                    ScopeItem::Poisoned,
                                    self.pos(named.local.span),
                                    type_only_import(import, named),
                                ));
                            }
                        }
                    }
                    continue;
                };
                for spec in import.specifiers.iter().flatten() {
                    if let ast::ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) =
                        spec
                    {
                        let pos = self.pos(namespace.local.span);
                        let item = ScopeItem::Namespace {
                            module: Some(target),
                            source: raw.clone(),
                        };
                        additions.push((namespace.local.name.to_string(), item, pos, false));
                        continue;
                    }
                    let ast::ImportDeclarationSpecifier::ImportSpecifier(named) = spec else {
                        let pos = self.pos(spec.span());
                        self.reject_subset(
                            RejectionSite::DefaultImport,
                            "only named imports are in the decided surface",
                            pos,
                        );
                        continue;
                    };
                    let local = named.local.name.to_string();
                    let imported_name = named.imported.name().to_string();
                    let imported_pos = self.pos(named.imported.span());
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
                ast::Declaration::ClassDeclaration(c) if c.type_parameters.is_none() => {
                    let name = c.declaration_name().to_string();
                    if let Some(&id) = self.class_ids.get(&self.declaration_symbol(file, &name)) {
                        self.resolve_class_shape(id, c, c.declare);
                    }
                }
                ast::Declaration::FunctionDeclaration(f) if f.type_parameters.is_none() => {
                    let name = f.declaration_name().to_string();
                    let sig = self.resolve_fn_sig(f, self.pos(f.declaration_name_span()));
                    self.fn_sigs
                        .insert(self.declaration_symbol(file, &name), sig);
                }
                ast::Declaration::VariableDeclaration(v) => {
                    for d in &v.declarations {
                        let ast::BindingPattern::BindingIdentifier(binding) = &d.id else {
                            continue;
                        };
                        let name = binding.name.to_string();
                        let state = match &d.type_annotation {
                            Some(ann) => {
                                TypeState::decided(self.resolve_type(&ann.type_annotation))
                            }
                            None => TypeState::Undecided,
                        };
                        let ty = state.ty().clone();
                        if self.is_context_affine_type(&ty) {
                            self.reject_subset(
                                RejectionSite::WorkerEndpointModuleGlobal,
                                "Worker, Inbox, and Outbox values may not be module globals",
                                self.pos(binding.span),
                            );
                        }
                        self.global_sigs.insert(
                            self.declaration_symbol(file, &name),
                            GlobalSig {
                                annotated: d.type_annotation.is_some(),
                                named_lambda: v.kind == ast::VariableDeclarationKind::Const
                                    && d.type_annotation.is_none()
                                    && matches!(
                                        d.init.as_ref().map(super::expr::unparen_expr),
                                        Some(ast::Expression::ArrowFunctionExpression(_))
                                    ),
                                lambda_defaults: None,
                                function_value_required: None,
                                state,
                                initializer: d.init.as_ref().map(|e| self.initializer(e, None)),
                                mutable: v.kind == ast::VariableDeclarationKind::Let,
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
    pub(crate) fn resolve_fn_sig(&mut self, f: &ast::Function, pos: Pos) -> FnSig<'p> {
        let saved = self.task_group_parameters;
        self.task_group_parameters = !f.r#async && !f.generator && !self.in_boundary;
        let params = self.resolve_params(&f.params, f.this_param.as_deref());
        self.task_group_parameters = saved;
        let mut sig = self.resolve_fn_result(f, pos);
        sig.params = params;
        sig
    }

    pub(super) fn resolve_fn_result(&mut self, f: &ast::Function, pos: Pos) -> FnSig<'p> {
        let params = Vec::new();
        if f.r#async {
            if f.generator {
                self.reject_subset(
                    RejectionSite::AsyncGeneratorFunction,
                    "a function cannot be both async and a generator",
                    pos,
                );
                return FnSig {
                    result_template: None,
                    generic: f.type_parameters.is_some(),
                    params,
                    ret: Type::Error,
                    is_generator: false,
                    is_async: true,
                    yield_known: true,
                };
            }
            let ret = match &f.return_type {
                Some(ann) => self.resolve_async_return(&ann.type_annotation),
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
                result_template: None,
                generic: f.type_parameters.is_some(),
                params,
                ret,
                is_generator: false,
                is_async: true,
                yield_known: true,
            };
        }
        if f.generator {
            // The yield type is inferred from the body (checked in
            // source order); a `Generator<T>` annotation, when present,
            // seeds it.
            let mut yield_ty = None;
            if let Some(ann) = &f.return_type {
                if let ast::TSType::TSTypeReference(r) = &ann.type_annotation {
                    if let ast::TSTypeName::IdentifierReference(id) = &r.type_name {
                        if id.name.as_ref() == "Generator" {
                            if let Some(args) = &r.type_arguments {
                                if !args.params.is_empty() {
                                    let resolved = self.resolve_type(&ann.type_annotation);
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
                result_template: None,
                generic: f.type_parameters.is_some(),
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
            Some(ann) => {
                // §195.1 rule 4: the result of a module-level function
                // declaration or a class method can be `T | undefined`. A
                // mirror declaration is not one (§195.6 rule 1).
                self.undefined_result = !self.in_boundary;
                let ret = self.resolve_result_type(&ann.type_annotation);
                self.undefined_result = false;
                ret
            }
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
            result_template: None,
            generic: f.type_parameters.is_some(),
            params,
            ret,
            is_generator: false,
            is_async: false,
            yield_known: true,
        }
    }

    pub(crate) fn resolve_params(
        &mut self,
        params: &oxc_ast::ast::FormalParameters<'_>,
        this_param: Option<&oxc_ast::ast::TSThisParameter<'_>>,
    ) -> Vec<ParamSig<'p>> {
        let mut resolved: Vec<_> =
            super::formal_parameters(this_param, params, self.prog.allocator)
                .iter()
                .map(|parameter| self.resolve_param_pat(parameter))
                .collect();
        if let Some(rest) = &params.rest {
            self.reject_subset(
                RejectionSite::RestParameter,
                "parameter pattern outside the decided surface",
                self.pos(rest.span),
            );
            resolved.push(ParamSig {
                annotated: false,
                name: String::new(),
                state: TypeState::Rejected,
                initializer: None,
                has_default: false,
            });
        }
        resolved
    }

    fn inferred_pattern_parameter_name(&mut self) -> String {
        let id = self.next_pattern_id;
        self.next_pattern_id += 1;
        format!("[[pattern#{id}.parameter]]")
    }

    pub(crate) fn resolve_param_pat(
        &mut self,
        pat: &oxc_ast::ast::FormalParameter<'_>,
    ) -> ParamSig<'p> {
        if let Some(initializer) = &pat.initializer {
            let infer_name = if pat.type_annotation.is_none() {
                match &pat.pattern {
                    ast::BindingPattern::BindingIdentifier(binding) => {
                        Some(binding.name.to_string())
                    }
                    ast::BindingPattern::ArrayPattern(_)
                    | ast::BindingPattern::ObjectPattern(_)
                        if !self.in_boundary =>
                    {
                        Some(self.inferred_pattern_parameter_name())
                    }
                    _ => None,
                }
            } else {
                None
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
                self.resolve_parameter_binding(pat)
            };
            inner.initializer = Some(self.initializer(initializer, None));
            inner.has_default = true;
            return inner;
        }
        self.resolve_parameter_binding(pat)
    }

    fn resolve_parameter_binding(&mut self, parameter: &ast::FormalParameter<'_>) -> ParamSig<'p> {
        let annotation = parameter.type_annotation.as_deref();
        match &parameter.pattern {
            ast::BindingPattern::BindingIdentifier(binding) => {
                if parameter.optional {
                    self.reject_subset(
                        RejectionSite::OptionalParameter,
                        "optional parameters imply `undefined`; use a default value or `T | null`",
                        self.pos(binding.span),
                    );
                }
                let ty = if let Some(annotation) = annotation {
                    let saved = self.task_group_type;
                    self.task_group_type = self.task_group_parameters
                        && type_reference_name(Some(&annotation.type_annotation))
                            == Some("TaskGroup");
                    let ty = self.resolve_type(&annotation.type_annotation);
                    self.task_group_type = saved;
                    ty
                } else {
                    self.reject_subset(
                        if self.generic_callback_context {
                            RejectionSite::GenericCallbackParameterAnnotationMissing
                        } else {
                            RejectionSite::NamedParameterAnnotationMissing
                        },
                        "parameters require a type annotation",
                        self.pos(binding.span),
                    );
                    Type::Error
                };
                ParamSig {
                    annotated: annotation
                        .is_some_and(|annotation| self.written_type(&annotation.type_annotation)),
                    name: binding.name.to_string(),
                    state: TypeState::decided(ty),
                    initializer: None,
                    has_default: false,
                }
            }
            ast::BindingPattern::ArrayPattern(_) | ast::BindingPattern::ObjectPattern(_)
                if !self.in_boundary =>
            {
                let ty = if let Some(annotation) = annotation {
                    self.resolve_type(&annotation.type_annotation)
                } else {
                    self.reject_subset(
                        if self.generic_callback_context {
                            RejectionSite::GenericCallbackParameterAnnotationMissing
                        } else {
                            RejectionSite::PatternParameterAnnotationMissing
                        },
                        "parameters require a type annotation",
                        self.pos(parameter.pattern.span()),
                    );
                    Type::Error
                };
                let name = self.inferred_pattern_parameter_name();
                ParamSig {
                    annotated: annotation
                        .is_some_and(|annotation| self.written_type(&annotation.type_annotation)),
                    name,
                    state: TypeState::decided(ty),
                    initializer: None,
                    has_default: false,
                }
            }
            other => {
                self.reject_subset(
                    if self.mirror_array_parameter_noniterable(parameter) {
                        RejectionSite::MirrorArrayParameterNonIterable
                    } else {
                        RejectionSite::MirrorBindingPatternParameter
                    },
                    "parameter pattern outside the decided surface",
                    self.pos(other.span()),
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
        params: Vec<(ParamSig<'p>, ast::BindingPattern<'p>)>,
        rest: Option<&ast::FormalParameterRest<'_>>,
        fx: &mut FnCtx<'p>,
    ) -> Vec<hir::Stmt> {
        let mut prologue = Vec::new();
        for (signature, pat) in params {
            let pat = match &pat {
                ast::BindingPattern::AssignmentPattern(assign) => &assign.left,
                other => other,
            };
            let pattern = pattern::classify(pat);
            match &pattern {
                pattern::Pattern::Rejected(rejection) => self.reject_pattern(rejection, fx),
                _ if pattern.is_destructuring() => {
                    if let Some(scope) = fx.scopes.last_mut() {
                        for binding in pattern::collect_names(pat) {
                            scope.parameter_names.insert(binding.name.to_string());
                        }
                    }
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
        if let Some(rest) = rest {
            let rejection = pattern::PatternRejection {
                span: rest.span,
                message: "a nested binding pattern needs recursive type checks and ordered effects",
                site: RejectionSite::BindingPatternUnsupportedRoot,
                names: pattern::collect_names(&rest.rest.argument),
            };
            self.reject_pattern(&rejection, fx);
        }
        prologue
    }
}

/// Answers whether an import specifier binds a type only: the whole
/// declaration is `import type`, or the specifier is written `type X`
/// (compiler.md §134 rule 1).
pub(super) fn type_only_import(
    import: &ast::ImportDeclaration,
    named: &ast::ImportSpecifier,
) -> bool {
    (import.import_kind == ast::ImportOrExportKind::Type)
        || (named.import_kind == ast::ImportOrExportKind::Type)
}

impl<'p> Checker<'p> {
    /// A written annotation names no parameter of the enclosing generic source.
    pub(crate) fn written_type(&self, ty: &ast::TSType) -> bool {
        match ty {
            ty if ty.is_keyword() => true,
            ast::TSType::TSTypeReference(reference) => {
                let name_ok = match &reference.type_name {
                    ast::TSTypeName::IdentifierReference(name) => {
                        !self.subst.contains_key(name.name.as_ref())
                    }
                    ast::TSTypeName::QualifiedName(_) => true,
                    ast::TSTypeName::ThisExpression(_) => true,
                };
                name_ok
                    && reference
                        .type_arguments
                        .as_ref()
                        .is_none_or(|args| args.params.iter().all(|arg| self.written_type(arg)))
            }
            ast::TSType::TSFunctionType(function) => {
                function.type_parameters.is_none()
                    && self.written_type(&function.return_type.type_annotation)
                    && function.params.rest.is_none()
                    && function.this_param.as_ref().is_none_or(|p| {
                        p.type_annotation
                            .as_ref()
                            .is_some_and(|ann| self.written_type(&ann.type_annotation))
                    })
                    && function.params.items.iter().all(|param| {
                        matches!(param.pattern, ast::BindingPattern::BindingIdentifier(_))
                            && param
                                .type_annotation
                                .as_ref()
                                .is_some_and(|ann| self.written_type(&ann.type_annotation))
                    })
            }
            ast::TSType::TSArrayType(array) => self.written_type(&array.element_type),
            ast::TSType::TSParenthesizedType(paren) => self.written_type(&paren.type_annotation),
            ast::TSType::TSUnionType(union) => union.types.iter().all(|ty| self.written_type(ty)),
            // Unsupported forms provide no proof for a loop store.
            _ => false,
        }
    }
}
