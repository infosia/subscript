use super::*;

impl<'p> Checker<'p> {
    // ----- pass A: name collection -----

    pub(super) fn register_scope_item(
        &mut self,
        file: usize,
        name: &str,
        item: ScopeItem,
        pos: Pos,
    ) {
        self.register_scope_binding(
            file,
            name,
            ScopeBinding {
                item,
                imported: false,
            },
            pos,
        );
    }

    pub(super) fn register_scope_binding(
        &mut self,
        file: usize,
        name: &str,
        binding: ScopeBinding,
        pos: Pos,
    ) {
        // Mirror (`.d.ts`) declarations populate the global ambient scope;
        // program declarations populate the per-file scope.
        let scope = if self.prog.files[file].dts {
            &mut self.ambient_scope
        } else {
            &mut self.file_scopes[file]
        };
        if scope.contains_key(name) {
            self.error(
                RuleCode::S017,
                format!("duplicate top-level name `{}`", name),
                pos,
            );
            return;
        }
        scope.insert(name.to_string(), binding);
    }

    pub(super) fn collect_file(&mut self, file: usize) {
        self.file_scopes.push(HashMap::new());
        self.exports.push(HashMap::new());
        self.export_definitions.push(BTreeMap::new());
        let module = &self.prog.files[file].module;
        for item in &module.body {
            let (decl, exported) = match item {
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => (&e.decl, true),
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(_)) => continue,
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportNamed(export)) => {
                    self.collect_named_exports(file, export);
                    continue;
                }
                ast::ModuleItem::ModuleDecl(
                    other @ (ast::ModuleDecl::ExportAll(_)
                    | ast::ModuleDecl::ExportDefaultDecl(_)
                    | ast::ModuleDecl::ExportDefaultExpr(_)),
                ) => {
                    self.error_diverging(
                        RuleCode::S100,
                        "the module surface requires named exports",
                        self.pos(other.span()),
                        Divergence::NamedModuleSurface,
                    );
                    continue;
                }
                ast::ModuleItem::ModuleDecl(other) => {
                    let pos = self.pos(other.span());
                    self.error(
                        RuleCode::S100,
                        "only `export` declarations and named imports are in the decided surface",
                        pos,
                    );
                    continue;
                }
                ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => (d, false),
                ast::ModuleItem::Stmt(_) => continue,
            };
            self.collect_decl(file, decl);
            if !self.prog.files[file].dts {
                for name in super::exports::declaration_names(decl) {
                    if !self.file_scopes[file].contains_key(name.sym.as_ref()) {
                        self.register_scope_item(
                            file,
                            name.sym.as_ref(),
                            ScopeItem::Poisoned,
                            self.pos(name.span),
                        );
                    }
                }
            }
            if exported {
                self.collect_export_declaration(file, decl);
            }
        }
    }

    fn collect_decl(&mut self, file: usize, decl: &ast::Decl) {
        if self.prog.files[file].dts {
            self.collect_mirror_decl(file, decl);
            return;
        }
        match decl {
            ast::Decl::Class(c) => self.collect_class(file, c),
            ast::Decl::Fn(f) => self.collect_fn(file, f),
            ast::Decl::Var(v) => self.collect_globals(file, v),
            ast::Decl::TsEnum(e) => self.collect_enum(file, e),
            ast::Decl::TsTypeAlias(alias) => self.collect_string_alias(file, alias),
            ast::Decl::Using(using) => {
                self.error(
                    RuleCode::S100,
                    if using.is_await {
                        "module-level `await using` is not in the decided surface"
                    } else {
                        "module-level `using` is not in the decided surface"
                    },
                    self.pos(using.span),
                );
            }
            other => {
                let pos = self.pos(other.span());
                self.error(
                    RuleCode::S100,
                    "declaration form outside the decided surface",
                    pos,
                );
            }
        }
    }

    fn cstruct_alignment(call: &ast::CallExpr) -> Result<u32, &'static str> {
        if call.args.len() != 1 || call.args[0].spread.is_some() {
            return Err("`@CStruct` accepts exactly one object-literal argument");
        }
        let ast::Expr::Object(options) = &*call.args[0].expr else {
            return Err("`@CStruct` accepts exactly one object-literal argument");
        };
        if options.props.len() != 1 {
            return Err("`@CStruct` options must contain only the `align` key");
        }
        let ast::PropOrSpread::Prop(prop) = &options.props[0] else {
            return Err("`@CStruct` options must contain only the `align` key");
        };
        let ast::Prop::KeyValue(property) = &**prop else {
            return Err("`@CStruct` options must contain only the `align` key");
        };
        let is_align = match &property.key {
            ast::PropName::Ident(key) => key.sym.as_ref() == "align",
            ast::PropName::Str(key) => key.value.as_str() == "align",
            _ => false,
        };
        if !is_align {
            return Err("`@CStruct` options must contain only the `align` key");
        }
        let ast::Expr::Lit(ast::Lit::Num(number)) = &*property.value else {
            return Err("`@CStruct` alignment must be an integer literal in {2, 4, 8, 16}");
        };
        let value = number.value;
        if value.fract() != 0.0 || !matches!(value as u32, 2 | 4 | 8 | 16) {
            return Err("`@CStruct` alignment must be an integer literal in {2, 4, 8, 16}");
        }
        Ok(value as u32)
    }

    fn class_decorators(
        &mut self,
        class: &ast::Class,
    ) -> (bool, bool, Option<hir::AlignmentOverride>) {
        let mut is_value = false;
        let mut is_descriptor = false;
        let mut alignment_override = None;
        for dec in &class.decorators {
            match &*dec.expr {
                ast::Expr::Ident(id) if id.sym.as_ref() == "CStruct" => is_value = true,
                ast::Expr::Ident(id) if id.sym.as_ref() == "Descriptor" => {
                    is_descriptor = true;
                }
                ast::Expr::Call(call)
                    if matches!(
                        &call.callee,
                        ast::Callee::Expr(callee)
                            if matches!(&**callee, ast::Expr::Ident(id) if id.sym.as_ref() == "CStruct")
                    ) =>
                {
                    is_value = true;
                    match Self::cstruct_alignment(call) {
                        Ok(value) => {
                            alignment_override = Some(hir::AlignmentOverride {
                                value,
                                pos: self.pos(dec.span),
                            });
                        }
                        Err(message) => {
                            self.error(RuleCode::S100, message, self.pos(dec.span));
                        }
                    }
                }
                ast::Expr::Call(call)
                    if matches!(
                        &call.callee,
                        ast::Callee::Expr(callee)
                            if matches!(&**callee, ast::Expr::Ident(id) if id.sym.as_ref() == "Descriptor")
                    ) =>
                {
                    is_descriptor = true;
                    self.error(
                        RuleCode::S100,
                        "`@Descriptor` does not accept options",
                        self.pos(dec.span),
                    );
                }
                _ => {
                    let pos = self.pos(dec.span);
                    self.error(
                        RuleCode::S100,
                        "the only decided decorators are the ambient `@CStruct` and `@Descriptor`",
                        pos,
                    );
                }
            }
        }
        if is_value && is_descriptor {
            self.error(
                RuleCode::S100,
                "`@Descriptor` declares a reference class and cannot be combined with `@CStruct`",
                self.pos(class.span),
            );
        }
        (is_value, is_descriptor, alignment_override)
    }

    fn collect_class(&mut self, file: usize, c: &ast::ClassDecl) {
        let name = c.ident.sym.to_string();
        let symbol = self.declaration_symbol(file, &name);
        let pos = self.pos(c.ident.span);
        let (is_value, is_descriptor, alignment_override) = self.class_decorators(&c.class);
        if let Some(tp) = &c.class.type_params {
            let static_members = c.class.body.iter().filter_map(|member| match member {
                ast::ClassMember::ClassProp(property) if property.is_static => Some(property.span),
                ast::ClassMember::Method(method) if method.is_static => Some(method.span),
                _ => None,
            });
            let mut has_static_member = false;
            for span in static_members {
                has_static_member = true;
                self.error_diverging(
                    RuleCode::S100,
                    "generic classes cannot declare static members",
                    self.pos(span),
                    Divergence::StaticMemberSurface,
                );
            }
            // §82.4 rule 5: the checker holds one substitution, so a
            // generic method on a generic class is out of the surface.
            let generic_methods = c.class.body.iter().filter_map(|member| match member {
                ast::ClassMember::Method(method)
                    if method.kind == ast::MethodKind::Method
                        && method.function.type_params.is_some() =>
                {
                    Some(method)
                }
                _ => None,
            });
            let mut rejected_generic_methods: HashMap<(Option<String>, bool), GenericMethod> =
                HashMap::new();
            for method in generic_methods {
                if let Some(template) = GenericMethod::rejected(file, &method.function) {
                    rejected_generic_methods.insert(
                        (Self::class_method_name(&method.key), method.is_static),
                        template,
                    );
                }
                // The static-member rule already reports a static method.
                if !method.is_static {
                    self.error_diverging(
                        RuleCode::S100,
                        "generic classes cannot declare generic methods",
                        self.pos(method.span),
                        Divergence::GenericMethodOnGenericClass,
                    );
                }
            }
            let type_params: Vec<String> =
                tp.params.iter().map(|p| p.name.sym.to_string()).collect();
            self.generic_classes.insert(
                symbol.clone(),
                GenericClass {
                    file,
                    is_value,
                    is_descriptor,
                    declared: c.declare,
                    alignment_override,
                    type_params,
                    has_static_member,
                    rejected_generic_methods,
                    class: (*c.class).clone(),
                    pos: pos.clone(),
                },
            );
            self.register_scope_item(file, &name, ScopeItem::GenericClass(symbol.clone()), pos);
        } else {
            let id = self.new_class(
                &symbol,
                is_value,
                is_descriptor,
                alignment_override,
                pos.clone(),
            );
            self.register_scope_item(file, &name, ScopeItem::Class(id), pos);
        }
    }

    pub(crate) fn new_class(
        &mut self,
        name: &str,
        is_value: bool,
        is_descriptor: bool,
        alignment_override: Option<hir::AlignmentOverride>,
        pos: Pos,
    ) -> ClassId {
        let id = ClassId(self.classes.len());
        self.classes.push(hir::ClassDef {
            name: source_name(name),
            is_value,
            alignment_override,
            is_descriptor,
            is_boundary: false,
            // §111 rule 1: the Context lifetime is the value of every
            // class that no mirror directive selects.
            callback_lifetime: crate::types::CallbackLifetime::Context,
            fields: Vec::new(),
            ctor: None,
            methods: Vec::new(),
            index_signature: None,
            pos: pos.clone(),
        });
        self.type_handle_classes
            .push(crate::types::HandleClass::from(&self.classes[id.0]));
        self.class_sigs.push(ClassSig::default());
        self.class_ids.insert(name.to_string(), id);
        id
    }

    fn collect_fn(&mut self, file: usize, f: &ast::FnDecl) {
        let name = f.ident.sym.to_string();
        let symbol = self.declaration_symbol(file, &name);
        let pos = self.pos(f.ident.span);
        if let Some(tp) = &f.function.type_params {
            let bodiless = f.function.body.is_none();
            if bodiless {
                self.error(RuleCode::S100, "function bodies are required", pos.clone());
            }
            let (type_params, duplicate_type_parameter) = self.collect_type_parameter_names(tp);
            self.generic_fns.insert(
                symbol.clone(),
                GenericFn {
                    file,
                    type_params,
                    function: (*f.function).clone(),
                    rejected: bodiless || duplicate_type_parameter,
                },
            );
            self.register_scope_item(file, &name, ScopeItem::GenericFunc(symbol.clone()), pos);
        } else {
            // Placeholder; pass B fills the real signature.
            self.fn_sigs.insert(
                symbol.clone(),
                FnSig {
                    params: Vec::new(),
                    ret: Type::Error,
                    is_generator: false,
                    is_async: false,
                    yield_known: false,
                },
            );
            self.register_scope_item(file, &name, ScopeItem::Func(symbol.clone()), pos);
        }
    }

    pub(super) fn collect_type_parameter_names(
        &mut self,
        params: &ast::TsTypeParamDecl,
    ) -> (Vec<String>, bool) {
        let mut names = HashSet::new();
        let mut duplicate = false;
        let names = params
            .params
            .iter()
            .map(|parameter| {
                let name = parameter.name.sym.to_string();
                if !names.insert(name.clone()) {
                    duplicate = true;
                    self.error(
                        RuleCode::S017,
                        format!("duplicate type parameter `{name}`"),
                        self.pos(parameter.name.span),
                    );
                }
                name
            })
            .collect();
        (names, duplicate)
    }

    fn collect_globals(&mut self, file: usize, v: &ast::VarDecl) {
        for d in &v.decls {
            let ast::Pat::Ident(binding) = &d.name else {
                self.reject_outer_pattern(file, &d.name);
                continue;
            };
            let name = binding.id.sym.to_string();
            let pos = self.pos(binding.id.span);
            self.register_scope_item(
                file,
                &name,
                ScopeItem::Global(self.declaration_symbol(file, &name)),
                pos,
            );
        }
    }

    /// Reports a binding pattern in a declaration outside a function body
    /// one time, and poisons every name in the pattern, so no
    /// `unknown name` follows it (§107.4).
    fn reject_outer_pattern(&mut self, file: usize, pat: &ast::Pat) {
        self.error_diverging(
            RuleCode::S100,
            "a binding pattern binds inside a function body; a declaration outside one binds one name",
            self.pos(pat.span()),
            Divergence::ModuleLevelPattern,
        );
        for binding in pattern::collect_names(pat) {
            let name = binding.id.sym.to_string();
            let pos = self.pos(binding.id.span);
            self.register_scope_item(file, &name, ScopeItem::Poisoned, pos);
        }
    }

    fn collect_enum(&mut self, file: usize, e: &ast::TsEnumDecl) {
        let name = e.id.sym.to_string();
        let pos = self.pos(e.id.span);
        let mut members = Vec::new();
        // `None` means the previous member's value + 1 overflows i32, so
        // the next implicit value has no representation.
        let mut next: Option<i64> = Some(0);
        for m in &e.members {
            let member_name = match &m.id {
                ast::TsEnumMemberId::Ident(id) => id.sym.to_string(),
                ast::TsEnumMemberId::Str(s) => {
                    let p = self.pos(s.span);
                    self.error(
                        RuleCode::S100,
                        "string enum member names are not decided",
                        p,
                    );
                    continue;
                }
            };
            let value = match &m.init {
                None => match next {
                    Some(v) => v,
                    None => {
                        let p = self.pos(m.span);
                        self.error(
                            RuleCode::S008,
                            format!(
                                "implicit value for enum member `{}` overflows i32",
                                member_name
                            ),
                            p,
                        );
                        0
                    }
                },
                Some(init) => match self.const_int_of(init) {
                    Some(v) => i64::from(v),
                    None => {
                        let p = self.pos(init.span());
                        self.error_diverging(
                            RuleCode::S100,
                            "enum members must have integer literal values",
                            p,
                            Divergence::IntegerLiteralRange,
                        );
                        next.unwrap_or(0)
                    }
                },
            };
            next = i32::try_from(value)
                .ok()
                .and_then(|value| value.checked_add(1))
                .map(i64::from);
            members.push((member_name, value));
        }
        let id = EnumId(self.enums.len());
        self.enums.push(hir::EnumDef {
            name: name.clone(),
            members,
            pos: pos.clone(),
        });
        self.register_scope_item(file, &name, ScopeItem::Enum(id), pos);
    }

    fn collect_string_alias(&mut self, file: usize, alias: &ast::TsTypeAliasDecl) {
        let name = alias.id.sym.to_string();
        let pos = self.pos(alias.id.span);
        if alias.type_params.is_some() {
            self.error(
                RuleCode::S100,
                "string-literal union aliases cannot be generic",
                pos,
            );
            return;
        }
        if let Some(mapping) = wire_alias_literal(&alias.type_ann) {
            self.collect_wire_string_alias(file, alias, mapping);
            return;
        }
        let Some(members) = string_alias_members(&alias.type_ann) else {
            self.error(
                RuleCode::S100,
                "type aliases are limited to a union of two or more string literals",
                pos,
            );
            return;
        };
        if members.len() > i32::MAX as usize {
            self.error(
                RuleCode::S100,
                "string-literal union has more members than fit its i32 discriminant",
                self.pos(alias.type_ann.span()),
            );
            return;
        }
        let mut seen = HashSet::new();
        if let Some(duplicate) = members
            .iter()
            .find(|member| !seen.insert((*member).clone()))
        {
            self.error(
                RuleCode::S100,
                format!("duplicate string-literal union member `{duplicate}`"),
                self.pos(alias.type_ann.span()),
            );
            return;
        }
        let id = StringAliasId(self.string_aliases.len());
        self.string_aliases.push(hir::StringAliasDef {
            name: name.clone(),
            members,
            wire_values: None,
            pos: pos.clone(),
        });
        self.register_scope_item(file, &name, ScopeItem::StringAlias(id), pos);
    }

    /// Collects and validates one `CEnum<{ key: wire }>` alias (§50.1).
    fn collect_wire_string_alias(
        &mut self,
        file: usize,
        alias: &ast::TsTypeAliasDecl,
        mapping: &ast::TsTypeLit,
    ) {
        let name = alias.id.sym.to_string();
        let pos = self.pos(alias.id.span);
        if mapping.members.is_empty() {
            self.error(
                RuleCode::S100,
                "wire-mapped string-literal union must have at least one member",
                self.pos(mapping.span),
            );
            return;
        }
        if mapping.members.len() > i32::MAX as usize {
            self.error(
                RuleCode::S100,
                "wire-mapped string-literal union has more members than fit its i32 discriminant",
                self.pos(mapping.span),
            );
            return;
        }

        let mut members = Vec::with_capacity(mapping.members.len());
        let mut wire_values = Vec::with_capacity(mapping.members.len());
        let mut seen_members = HashSet::new();
        let mut seen_wires: HashMap<i32, String> = HashMap::new();
        for element in &mapping.members {
            let ast::TsTypeElement::TsPropertySignature(property) = element else {
                self.error(
                    RuleCode::S100,
                    "CEnum mappings contain only named properties with integer-literal values",
                    self.pos(element.span()),
                );
                return;
            };
            let member = match &*property.key {
                ast::Expr::Lit(ast::Lit::Str(value)) => value.value.to_string(),
                ast::Expr::Ident(value) if !property.computed => value.sym.to_string(),
                _ => {
                    self.error(
                        RuleCode::S100,
                        "CEnum member keys must be string literals or identifiers",
                        self.pos(property.key.span()),
                    );
                    return;
                }
            };
            if !seen_members.insert(member.clone()) {
                self.error(
                    RuleCode::S100,
                    format!("duplicate string-literal union member `{member}`"),
                    self.pos(property.key.span()),
                );
                return;
            }
            let Some(annotation) = &property.type_ann else {
                self.error_diverging(
                    RuleCode::S100,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(property.span),
                    Divergence::WireEnumValues,
                );
                return;
            };
            let ast::TsType::TsLitType(ast::TsLitType {
                lit: ast::TsLit::Number(number),
                ..
            }) = &*annotation.type_ann
            else {
                self.error_diverging(
                    RuleCode::S100,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(annotation.type_ann.span()),
                    Divergence::WireEnumValues,
                );
                return;
            };
            if !number.value.is_finite() || number.value.fract() != 0.0 {
                self.error_diverging(
                    RuleCode::S100,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(number.span),
                    Divergence::WireEnumValues,
                );
                return;
            }
            if number.value < f64::from(i32::MIN) || number.value > f64::from(i32::MAX) {
                let spelling = number
                    .raw
                    .as_ref()
                    .map_or_else(|| number.value.to_string(), ToString::to_string);
                self.error_diverging(
                    RuleCode::S100,
                    format!(
                        "wire value {spelling} for CEnum member `{member}` is outside the i32 range"
                    ),
                    self.pos(number.span),
                    Divergence::WireEnumValues,
                );
                return;
            }
            let wire = number.value as i32;
            if let Some(first) = seen_wires.insert(wire, member.clone()) {
                self.error_diverging(
                    RuleCode::S100,
                    format!(
                        "duplicate CEnum wire value {wire} for members `{first}` and `{member}`"
                    ),
                    self.pos(number.span),
                    Divergence::WireEnumValues,
                );
                return;
            }
            members.push(member);
            wire_values.push(wire);
        }

        let id = StringAliasId(self.string_aliases.len());
        self.string_aliases.push(hir::StringAliasDef {
            name: name.clone(),
            members,
            wire_values: Some(wire_values),
            pos: pos.clone(),
        });
        self.register_scope_item(file, &name, ScopeItem::StringAlias(id), pos);
    }

    fn const_int_of(&self, e: &ast::Expr) -> Option<i32> {
        fn read(e: &ast::Expr, negate: bool) -> Option<i32> {
            match e {
                ast::Expr::Lit(ast::Lit::Num(number)) => {
                    let raw = number.raw.as_deref()?;
                    i32::try_from(parse_integer_spelling(raw, negate)?).ok()
                }
                ast::Expr::Unary(unary) if unary.op == ast::UnaryOp::Minus => {
                    read(&unary.arg, !negate)
                }
                ast::Expr::Paren(paren) => read(&paren.expr, negate),
                _ => None,
            }
        }

        read(e, false)
    }

    // ----- mirror (`.d.ts`) ingestion -----

    /// Pass A for a mirror declaration: registers the name (handle,
    /// boundary struct, enum, type alias, foreign function, or ambient
    /// const) into the global ambient scope. Shapes/signatures are
    /// resolved in [`Self::resolve_mirror_signatures`].
    fn collect_mirror_decl(&mut self, file: usize, decl: &ast::Decl) {
        match decl {
            ast::Decl::TsInterface(i) => self.collect_handle(file, i),
            ast::Decl::Class(c) if c.class.type_params.is_none() => {
                self.collect_boundary_struct(file, c)
            }
            ast::Decl::TsTypeAlias(t) => {
                if string_alias_members(&t.type_ann).is_some()
                    || wire_alias_literal(&t.type_ann).is_some()
                {
                    self.collect_string_alias(file, t);
                } else {
                    // Reserve the name; the aliased type is resolved in pass B.
                    self.type_aliases
                        .entry(t.id.sym.to_string())
                        .or_insert(Type::Error);
                }
            }
            ast::Decl::Fn(f) => {
                let name = f.ident.sym.to_string();
                let pos = self.pos(f.ident.span);
                self.register_scope_item(file, &name, ScopeItem::Foreign(name.clone()), pos);
            }
            ast::Decl::Var(v) => self.collect_ambient_consts(file, v),
            ast::Decl::TsEnum(e) => self.collect_enum(file, e),
            other => {
                let pos = self.pos(other.span());
                self.error(
                    RuleCode::S100,
                    "mirror declaration form outside the decided surface",
                    pos,
                );
            }
        }
    }

    /// An ambient `interface` in a mirror is an opaque handle (Q13): an
    /// empty branded nominal type. Its members (the phantom brand) carry
    /// no in-language meaning and are ignored; it lowers to a
    /// pointer-sized handle (a reference-shaped nominal, non-value).
    fn collect_handle(&mut self, file: usize, i: &ast::TsInterfaceDecl) {
        let name = i.id.sym.to_string();
        let pos = self.pos(i.id.span);
        let id = self.new_class(
            &self.declaration_symbol(file, &name),
            false,
            false,
            None,
            pos.clone(),
        );
        self.handle_classes.insert(id);
        self.register_scope_item(file, &name, ScopeItem::Class(id), pos);
    }

    /// A mirror `declare class` is a boundary struct: a C-layout value
    /// type (Q13) whose fields may hold boundary types. Shape resolved
    /// in pass B.
    fn collect_boundary_struct(&mut self, file: usize, c: &ast::ClassDecl) {
        let name = c.ident.sym.to_string();
        let pos = self.pos(c.ident.span);
        let id = self.new_class(
            &self.declaration_symbol(file, &name),
            true,
            false,
            None,
            pos.clone(),
        );
        self.boundary_classes.insert(id);
        self.classes[id.0].is_boundary = true;
        // §111 rule 2: the form carries the selection. The checker reads
        // it from the mirror's own directive.
        if self.prog.files[file]
            .provenance
            .callback_lifetimes
            .contains_key(&name)
        {
            self.classes[id.0].callback_lifetime = crate::types::CallbackLifetime::Explicit;
        }
        self.type_handle_classes[id.0] = crate::types::HandleClass::BoundaryValue;
        self.register_scope_item(file, &name, ScopeItem::Class(id), pos);
    }

    /// A mirror `declare const` (enum/flag constant, Q13): a read-only
    /// ambient global of the given type. Type resolved in pass B.
    fn collect_ambient_consts(&mut self, file: usize, v: &ast::VarDecl) {
        for d in &v.decls {
            let ast::Pat::Ident(binding) = &d.name else {
                self.reject_outer_pattern(file, &d.name);
                continue;
            };
            let name = binding.id.sym.to_string();
            let pos = self.pos(binding.id.span);
            self.register_scope_item(
                file,
                &name,
                ScopeItem::Global(self.declaration_symbol(file, &name)),
                pos,
            );
        }
    }
}
