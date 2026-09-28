use super::*;

impl<'p> Checker<'p> {
    /// Pass B for a mirror file: resolves type aliases first (so later
    /// declarations may reference them), then boundary-struct shapes,
    /// foreign-function signatures, and ambient-constant types. Runs with
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
                    if let Some(&id) = self.class_ids.get(&name) {
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
                        if Self::contains_string_alias(&parameter.ty)
                            && !Self::supported_wire_alias_boundary_type(&parameter.ty)
                        {
                            self.error(
                                RuleCode::S100,
                                format!(
                                    "wire-mapped aliases are supported only as direct foreign-function parameters or array-descriptor elements; `{}` nests one inside another boundary type",
                                    parameter.name
                                ),
                                pos.clone(),
                            );
                        }
                    }
                    if Self::contains_string_alias(&sig.ret)
                        && !matches!(sig.ret, Type::StringAlias(_))
                    {
                        self.error(
                            RuleCode::S100,
                            "wire-mapped aliases are supported only as direct foreign-function returns",
                            pos.clone(),
                        );
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
                            &parameter.ty,
                            parameter_pos.clone(),
                        );
                        if matches!(parameter.ty, Type::Func(_)) {
                            self.error(
                                RuleCode::S100,
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
                            name: parameter.name.clone(),
                            ty: parameter.ty.clone(),
                            default: None,
                            foreign_provenance,
                            pos: parameter_pos,
                        });
                    }
                    let unsupported_return = match &sig.ret {
                        Type::Str => Some("a string view"),
                        Type::Array(_) => Some("an array descriptor"),
                        Type::Func(_) => Some("a direct callback"),
                        _ => None,
                    };
                    if let Some(kind) = unsupported_return {
                        self.error(
                            RuleCode::S100,
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
                        self.error(
                            RuleCode::S100,
                            format!(
                                "mirror `{}` has no header identity for foreign function `{}`",
                                self.prog.files[file].name, name
                            ),
                            pos,
                        );
                        continue;
                    };
                    self.foreign_defs.push(hir::ForeignFn {
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
                        let ast::Pat::Ident(binding) = &d.name else {
                            continue;
                        };
                        let name = binding.id.sym.to_string();
                        let ty = match &binding.type_ann {
                            Some(ann) => self.resolve_type(&ann.type_ann),
                            None => match d.init.as_deref().and_then(int_literal_value) {
                                // A mirror flag member (§13.2):
                                // `declare const X = <int literal>;`. tsc
                                // accepts a bare literal initializer on an
                                // ambient const only without a type
                                // annotation, so the value travels here and
                                // the `u64` flag type is supplied by rule.
                                Some(value) => {
                                    self.ambient_int_consts
                                        .insert(name.clone(), (value, Type::U64));
                                    Type::U64
                                }
                                None => {
                                    let pos = self.pos(binding.id.span);
                                    self.error(
                                        RuleCode::S100,
                                        "ambient constants require a type annotation \
                                         or an integer-literal initializer",
                                        pos,
                                    );
                                    Type::Error
                                }
                            },
                        };
                        self.global_sigs
                            .insert(name, GlobalSig { ty, mutable: false });
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
            let mut additions: Vec<(String, ScopeItem, Pos)> = Vec::new();
            for item in &module.body {
                let ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import)) = item else {
                    continue;
                };
                let raw = import.src.value.to_string();
                let stem = normalize_module_specifier(&raw);
                let Some(target) = self.prog.files.iter().position(|f| f.stem == stem) else {
                    let pos = self.pos(import.src.span);
                    let missing_message =
                        format!("imported module `{raw}` is not among the program's files");
                    if self.poison_missing_modules.contains(&stem) {
                        if import.specifiers.is_empty() {
                            self.error(RuleCode::S100, missing_message, pos);
                            continue;
                        }
                        let mut names = Vec::new();
                        for spec in &import.specifiers {
                            let ast::ImportSpecifier::Named(named) = spec else {
                                self.error(
                                    RuleCode::S100,
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
                            ));
                            names.push((imported, local));
                        }
                        if !names.is_empty() {
                            self.poisoned_imports.push(hir::PoisonedImport {
                                module: raw,
                                names,
                                pos,
                            });
                        }
                    } else {
                        self.error(RuleCode::S100, missing_message, pos);
                    }
                    continue;
                };
                for spec in &import.specifiers {
                    let ast::ImportSpecifier::Named(named) = spec else {
                        let pos = self.pos(spec.span());
                        self.error(
                            RuleCode::S100,
                            "only named imports are in the decided surface",
                            pos,
                        );
                        continue;
                    };
                    let local = named.local.sym.to_string();
                    let pos = self.pos(named.local.span);
                    if !self.exports[target].contains(&local) {
                        self.error(
                            RuleCode::S016,
                            format!("`{}` is not exported by `{}`", local, raw),
                            pos.clone(),
                        );
                        additions.push((local, ScopeItem::Poisoned, pos));
                        continue;
                    }
                    match self.file_scopes[target].get(&local) {
                        Some(item) => additions.push((local, item.clone(), pos)),
                        None => {
                            self.error(
                                RuleCode::S016,
                                format!("`{}` is not defined in `{}`", local, raw),
                                pos,
                            );
                        }
                    }
                }
            }
            for (name, item, pos) in additions {
                self.register_scope_item(file, &name, item, pos);
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
                    if let Some(&id) = self.class_ids.get(&name) {
                        self.resolve_class_shape(id, &c.class, c.declare);
                    }
                }
                ast::Decl::Fn(f) if f.function.type_params.is_none() => {
                    let name = f.ident.sym.to_string();
                    let sig = self.resolve_fn_sig(&f.function, self.pos(f.ident.span));
                    if self.exports[file].contains(&name) {
                        if sig.is_async && (!sig.params.is_empty() || sig.ret != Type::Void) {
                            self.error(
                                RuleCode::S100,
                                format!(
                                    "exported async function `{name}` must have the host entry signature `(): Promise<void>`"
                                ),
                                self.pos(f.ident.span),
                            );
                        }
                        let aliases_boundary = sig.params.iter().any(|parameter| {
                            Self::contains_string_alias(&parameter.ty)
                                && !self.is_wire_alias(&parameter.ty)
                        }) || Self::contains_string_alias(&sig.ret);
                        if aliases_boundary {
                            self.error_diverging(
                                RuleCode::S100,
                                format!(
                                    "exported function `{name}` has a string-literal union \
                                     alias in its boundary signature"
                                ),
                                self.pos(f.ident.span),
                                Divergence::EntryParameterType,
                            );
                        }
                    }
                    self.fn_sigs.insert(name, sig);
                }
                ast::Decl::Var(v) => {
                    for d in &v.decls {
                        let ast::Pat::Ident(binding) = &d.name else {
                            continue;
                        };
                        let name = binding.id.sym.to_string();
                        let ty = match &binding.type_ann {
                            Some(ann) => self.resolve_type(&ann.type_ann),
                            None => {
                                let pos = self.pos(binding.id.span);
                                self.error(
                                    RuleCode::S100,
                                    "module-level variables require a type annotation",
                                    pos,
                                );
                                Type::Error
                            }
                        };
                        if Self::is_context_affine_type(&ty) {
                            self.error_diverging(
                                RuleCode::S100,
                                "Worker, Inbox, and Outbox values may not be module globals",
                                self.pos(binding.id.span),
                                Divergence::WorkerContextAffinity,
                            );
                        }
                        self.global_sigs.insert(
                            name,
                            GlobalSig {
                                ty,
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
        let params = self.resolve_params(&f.params);
        if f.is_async {
            if f.is_generator {
                self.error(
                    RuleCode::S100,
                    "a function cannot be both async and a generator",
                    pos,
                );
                return FnSig {
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
                    self.error(
                        RuleCode::S100,
                        "async functions require an explicit `Promise<T>` return annotation",
                        pos,
                    );
                    Type::Error
                }
            };
            return FnSig {
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
                                if let Some(first) = args.params.first() {
                                    yield_ty = Some(self.resolve_type(first));
                                }
                            }
                        }
                    }
                }
            }
            let known = yield_ty.is_some();
            return FnSig {
                params,
                ret: Type::Generator(Box::new(yield_ty.unwrap_or(Type::Error))),
                is_generator: true,
                is_async: false,
                yield_known: known,
            };
        }
        let ret = match &f.return_type {
            Some(ann) => self.resolve_type(&ann.type_ann),
            None => {
                self.error(
                    RuleCode::S100,
                    "function return types must be annotated",
                    pos,
                );
                Type::Error
            }
        };
        FnSig {
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

    pub(crate) fn resolve_param_pat(&mut self, pat: &ast::Pat) -> ParamSig {
        match pat {
            ast::Pat::Ident(binding) => {
                if binding.id.optional {
                    // C7: optional parameters without defaults imply an
                    // observable `undefined`.
                    let pos = self.pos(binding.id.span);
                    self.error(
                        RuleCode::S012,
                        "optional parameters imply `undefined`; use a default value or `T | null`",
                        pos,
                    );
                }
                let ty = match &binding.type_ann {
                    Some(ann) => self.resolve_type(&ann.type_ann),
                    None => {
                        let pos = self.pos(binding.id.span);
                        self.error(RuleCode::S100, "parameters require a type annotation", pos);
                        Type::Error
                    }
                };
                ParamSig {
                    name: binding.id.sym.to_string(),
                    ty,
                    has_default: false,
                }
            }
            ast::Pat::Assign(assign) => {
                let mut inner = self.resolve_param_pat(&assign.left);
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
                        self.error(RuleCode::S100, "parameters require a type annotation", pos);
                        Type::Error
                    }
                };
                let id = self.next_pattern_id;
                self.next_pattern_id += 1;
                ParamSig {
                    name: format!("[[pattern#{id}.parameter]]"),
                    ty,
                    has_default: false,
                }
            }
            other => {
                let pos = self.pos(other.span());
                self.error(
                    RuleCode::S100,
                    "parameter pattern outside the decided surface",
                    pos,
                );
                ParamSig {
                    name: String::new(),
                    ty: Type::Error,
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
                        kind: hir::ExprKind::Local(signature.name.clone(), signature.ty.clone()),
                        ty: signature.ty.clone(),
                        pos: self.pos(pattern.span()),
                    };
                    self.bind_pattern_from(&pattern, &source, true, fx, &mut prologue);
                }
                _ => {}
            }
        }
        prologue
    }
}
