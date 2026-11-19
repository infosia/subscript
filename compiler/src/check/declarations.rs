use super::*;
use crate::check::rejection::{RejectionFailure, RejectionSite};

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
                type_only: false,
            },
            pos,
        );
    }

    /// Records the position of the name of a class, a generic class, or an
    /// enum of a program file (compiler.md §241 rule 1).
    fn record_declaration_start(&mut self, file: usize, name: &str, pos: &Pos) {
        if !self.prog.files[file].dts {
            self.declaration_starts
                .insert((file, name.to_string()), pos.clone());
        }
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
            let site = if matches!(
                (&scope[name].item, &binding.item),
                (ScopeItem::Class(_), ScopeItem::Class(_))
            ) {
                RejectionSite::DuplicateTopLevelClass
            } else if matches!(binding.item, ScopeItem::Func(_))
                && super::rejection_facts::unimplemented_function_group(
                    &self.prog.files[file].module,
                    name,
                )
            {
                RejectionSite::TopLevelOverloadImplementationMissing
            } else {
                RejectionSite::TopLevelNameClash
            };
            self.reject_subset(site, format!("duplicate top-level name `{}`", name), pos);
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
                ast::Statement::ExportDeclaration(export) => (&export.declaration, true),
                ast::Statement::ImportDeclaration(_) => continue,
                ast::Statement::ExportNamedDeclaration(export) => {
                    self.collect_named_exports(file, exports::NamedExport::local(export));
                    continue;
                }
                ast::Statement::ExportFromDeclaration(export) => {
                    self.collect_named_exports(file, exports::NamedExport::remote(export));
                    continue;
                }
                ast::Statement::ExportAllDeclaration(export) if export.exported.is_some() => {
                    let exported = export.exported.as_ref();
                    let end = exported.map_or(export.span.end, |name| name.span().start);
                    let prefix = oxc_span::Span::new(export.span.start, end);
                    let start = self
                        .prog
                        .snippet(prefix)
                        .and_then(|text| {
                            text.bytes().enumerate().find_map(|(offset, byte)| {
                                let at = prefix.start + offset as u32;
                                (byte == b'*'
                                    && !module.comments.iter().any(|comment| {
                                        comment.span.start <= at && at < comment.span.end
                                    }))
                                .then_some(at)
                            })
                        })
                        .unwrap_or(export.span.start);
                    self.reject_subset(
                        RejectionSite::ExportSpecifierKind,
                        "the module surface requires named exports",
                        self.pos(oxc_span::Span::new(start, start)),
                    );
                    continue;
                }
                ast::Statement::ExportAllDeclaration(_)
                | ast::Statement::ExportDefaultDeclaration(_) => {
                    self.reject_subset(
                        if matches!(item, ast::Statement::ExportAllDeclaration(_)) {
                            RejectionSite::ExportAllDeclaration
                        } else {
                            RejectionSite::DefaultExportDeclaration
                        },
                        "the module surface requires named exports",
                        self.pos(item.span()),
                    );
                    continue;
                }
                ast::Statement::TSImportEqualsDeclaration(import) => {
                    self.reject_subset(
                        if matches!(
                            import.module_reference,
                            ast::TSModuleReference::ExternalModuleReference(_)
                        ) {
                            RejectionSite::ExternalImportEqualsDeclaration
                        } else {
                            RejectionSite::UnsupportedModuleDeclaration
                        },
                        "only `export` declarations and named imports are in the decided surface",
                        self.pos(import.span),
                    );
                    continue;
                }
                other if other.is_module_declaration() => {
                    self.reject_subset(
                        if matches!(other, ast::Statement::TSExportAssignment(_)) {
                            RejectionSite::ExportAssignmentEsModule
                        } else {
                            RejectionSite::UnsupportedModuleDeclaration
                        },
                        "only `export` declarations and named imports are in the decided surface",
                        self.pos(other.span()),
                    );
                    continue;
                }
                statement => {
                    let Some(declaration) = statement.as_declaration() else {
                        continue;
                    };
                    (declaration, false)
                }
            };
            self.collect_decl(file, decl);
            if !self.prog.files[file].dts {
                for name in super::exports::declaration_names(decl) {
                    if !self.file_scopes[file].contains_key(name.name.as_ref()) {
                        self.register_scope_item(
                            file,
                            name.name.as_ref(),
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

    fn collect_decl(&mut self, file: usize, decl: &ast::Declaration) {
        if self.prog.files[file].dts {
            self.collect_mirror_decl(file, decl);
            return;
        }
        match decl {
            ast::Declaration::ClassDeclaration(c) => self.collect_class(file, c),
            ast::Declaration::FunctionDeclaration(f) => self.collect_fn(file, f),
            ast::Declaration::VariableDeclaration(v)
                if !matches!(
                    v.kind,
                    ast::VariableDeclarationKind::Using | ast::VariableDeclarationKind::AwaitUsing
                ) =>
            {
                self.collect_globals(file, v)
            }
            ast::Declaration::TSEnumDeclaration(e) => self.collect_enum(file, e),
            ast::Declaration::TSTypeAliasDeclaration(alias) => {
                self.collect_string_alias(file, alias)
            }
            ast::Declaration::TSInterfaceDeclaration(interface) => {
                self.collect_interface(file, interface)
            }
            ast::Declaration::VariableDeclaration(using) => {
                self.reject_subset(
                    RejectionSite::ModuleUsing,
                    if using.kind == ast::VariableDeclarationKind::AwaitUsing {
                        "module-level `await using` is not in the decided surface"
                    } else {
                        "module-level `using` is not in the decided surface"
                    },
                    self.pos(using.span),
                );
            }
            other => {
                let pos = self.pos(other.span());
                self.reject_subset(
                    RejectionSite::SourceNamespaceDeclaration,
                    "declaration form outside the decided surface",
                    pos,
                );
            }
        }
    }

    fn value_type_alignment(
        call: &ast::CallExpression,
        source_decorator: bool,
    ) -> Result<u32, RejectionFailure> {
        if call.arguments.len() != 1 || call.arguments[0].is_spread() {
            return Err(RejectionFailure::new(
                RejectionSite::ValueTypeArgumentCount,
                "`@ValueType` accepts exactly one object-literal argument",
            ));
        }
        let ast::Expression::ObjectExpression(options) = argument_expression(&call.arguments[0])
        else {
            return Err(RejectionFailure::new(
                RejectionSite::ValueTypeOptionsNonLiteral,
                "`@ValueType` accepts exactly one object-literal argument",
            ));
        };
        if options.properties.len() != 1 {
            return Err(RejectionFailure::new(
                RejectionSite::ValueTypeOptionCount,
                "`@ValueType` options must contain only the `align` key",
            ));
        }
        let ast::ObjectPropertyKind::ObjectProperty(property) = &options.properties[0] else {
            return Err(RejectionFailure::new(
                RejectionSite::ValueTypeOptionSpread,
                "`@ValueType` options must contain only the `align` key",
            ));
        };
        if property.method || property.shorthand || property.kind != ast::PropertyKind::Init {
            return Err(RejectionFailure::new(
                RejectionSite::ValueTypeOptionPropertyForm,
                "`@ValueType` options must contain only the `align` key",
            ));
        }
        let is_align = match &property.key {
            ast::PropertyKey::StaticIdentifier(key) => key.name.as_ref() == "align",
            ast::PropertyKey::StringLiteral(key) => key.value.as_str() == "align",
            _ => false,
        };
        if !is_align {
            return Err(RejectionFailure::new(
                RejectionSite::ValueTypeOptionKey,
                "`@ValueType` options must contain only the `align` key",
            ));
        }
        let ast::Expression::NumericLiteral(number) = &property.value else {
            return Err(RejectionFailure::new(
                RejectionSite::ValueTypeAlignmentNonLiteral,
                "`@ValueType` alignment must be an integer literal in {2, 4, 8, 16}",
            ));
        };
        let value = number.value;
        if value.fract() != 0.0 || !matches!(value as u32, 2 | 4 | 8 | 16) {
            return Err(RejectionFailure::new(
                if source_decorator {
                    RejectionSite::ValueTypeAlignmentOutsideSet
                } else {
                    RejectionSite::BuiltinValueTypeAlignmentOutsideSet
                },
                "`@ValueType` alignment must be an integer literal in {2, 4, 8, 16}",
            ));
        }
        Ok(value as u32)
    }

    fn class_decorators(
        &mut self,
        class: &ast::Class,
        name: &str,
    ) -> (bool, bool, Option<hir::AlignmentOverride>) {
        let mut is_value = false;
        let mut value_decorator_pos = None;
        let mut is_descriptor = false;
        let mut alignment_override = None;
        for dec in &class.decorators {
            match &dec.expression {
                ast::Expression::Identifier(id) if id.name.as_ref() == "ValueType" => {
                    is_value = true;
                    value_decorator_pos = Some(self.pos(dec.span));
                }
                ast::Expression::Identifier(id) if id.name.as_ref() == "Descriptor" => {
                    is_descriptor = true;
                }
                ast::Expression::CallExpression(call)
                    if matches!(
                        &call.callee,
                        ast::Expression::Identifier(id) if id.name.as_ref() == "ValueType"
                    ) =>
                {
                    is_value = true;
                    value_decorator_pos = Some(self.pos(dec.span));
                    match Self::value_type_alignment(
                        call,
                        self.source_function_declared("ValueType"),
                    ) {
                        Ok(value) => {
                            alignment_override = Some(hir::AlignmentOverride {
                                value,
                                pos: self.pos(dec.span),
                            });
                        }
                        Err(message) => {
                            self.reject_subset(message.site, message.message, self.pos(dec.span));
                        }
                    }
                }
                ast::Expression::CallExpression(call)
                    if matches!(
                        &call.callee,
                        ast::Expression::Identifier(id) if id.name.as_ref() == "Descriptor"
                    ) =>
                {
                    is_descriptor = true;
                    self.reject_subset(
                        if self.source_function_declared("Descriptor") {
                            RejectionSite::DescriptorOptions
                        } else {
                            RejectionSite::BuiltinDescriptorOptions
                        },
                        "`@Descriptor` does not accept options",
                        self.pos(dec.span),
                    );
                }
                _ => {
                    let pos = self.pos(dec.span);
                    self.reject_subset(RejectionSite::UnsupportedClassDecorator, "the only decided decorators are the ambient `@ValueType` and `@Descriptor`", pos);
                }
            }
        }
        if let Some(pos) = value_decorator_pos {
            if let Some(accessibility) = class
                .body
                .body
                .iter()
                .find_map(|member| match member {
                    ast::ClassElement::MethodDefinition(constructor)
                        if constructor.kind == ast::MethodDefinitionKind::Constructor =>
                    {
                        constructor.accessibility
                    }
                    _ => None,
                })
                .filter(|accessibility| {
                    matches!(
                        accessibility,
                        ast::TSAccessibility::Private | ast::TSAccessibility::Protected
                    )
                })
            {
                let modifier = if accessibility == ast::TSAccessibility::Private {
                    "private"
                } else {
                    "protected"
                };
                self.reject_subset(
                    RejectionSite::ValueTypeRestrictedConstructor,
                    format!("`@ValueType` rejects a {modifier} constructor in class `{name}`; the constructor must be public"),
                    pos,
                );
            }
        }
        if is_value && is_descriptor {
            self.reject_subset(
                RejectionSite::DescriptorValueType,
                "`@Descriptor` declares a reference class and cannot be combined with `@ValueType`",
                self.pos(class.span),
            );
        }
        (is_value, is_descriptor, alignment_override)
    }

    fn collect_class(&mut self, file: usize, c: &ast::Class) {
        let name = c.declaration_name().to_string();
        let symbol = self.declaration_symbol(file, &name);
        let pos = self.pos(c.declaration_name_span());
        if !c.declare {
            self.record_declaration_start(file, &name, &pos);
        }
        let (is_value, is_descriptor, alignment_override) = self.class_decorators(c, &name);
        if c.type_parameters
            .as_deref()
            .is_some_and(|params| self.reject_type_parameter_defaults(params))
        {
            self.register_scope_item(file, &name, ScopeItem::Poisoned, pos);
            return;
        }
        if let Some(tp) = &c.type_parameters {
            let static_members = c.body.body.iter().filter_map(|member| match member {
                ast::ClassElement::PropertyDefinition(property) if property.r#static => {
                    Some(property.span)
                }
                ast::ClassElement::MethodDefinition(method) if method.r#static => Some(method.span),
                _ => None,
            });
            let mut has_static_member = false;
            for span in static_members {
                has_static_member = true;
                self.reject_subset(
                    RejectionSite::GenericClassStaticMember,
                    "generic classes cannot declare static members",
                    self.pos(span),
                );
            }
            // §82.4 rule 5: the checker holds one substitution, so a
            // generic method on a generic class is out of the surface.
            let generic_methods = c.body.body.iter().filter_map(|member| match member {
                ast::ClassElement::MethodDefinition(method)
                    if method.kind == ast::MethodDefinitionKind::Method
                        && method.value.type_parameters.is_some() =>
                {
                    Some(method)
                }
                _ => None,
            });
            let mut rejected_generic_methods: HashMap<(Option<String>, bool), GenericMethod<'p>> =
                HashMap::new();
            for method in generic_methods {
                if let Some(template) =
                    GenericMethod::rejected(file, &method.value, self.prog.allocator)
                {
                    rejected_generic_methods.insert(
                        (Self::class_method_name(&method.key), method.r#static),
                        template,
                    );
                }
                // The static-member rule already reports a static method.
                if !method.r#static {
                    self.reject_subset(
                        RejectionSite::GenericClassGenericMethod,
                        "generic classes cannot declare generic methods",
                        self.pos(method.span),
                    );
                }
            }
            let type_params: Vec<String> =
                tp.params.iter().map(|p| p.name.name.to_string()).collect();
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
                    class: Rc::new(c.clone_in(self.prog.allocator)),
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
            symbol: hir::Symbol::from_full_text(name),
            name: source_name(name),
            is_value,
            alignment_override,
            is_descriptor,
            is_boundary: false,
            boundary_header: None,
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

    fn collect_fn(&mut self, file: usize, f: &ast::Function) {
        let name = f.declaration_name().to_string();
        let symbol = self.declaration_symbol(file, &name);
        if let Some(class) = self
            .prog
            .local_functions
            .classes
            .get(&f.declaration_name_span().start)
        {
            self.local_function_classes
                .insert(symbol.clone(), (file, class.clone()));
        }
        let pos = self.pos(f.declaration_name_span());
        if f.type_parameters
            .as_deref()
            .is_some_and(|params| self.reject_type_parameter_defaults(params))
        {
            self.register_scope_item(file, &name, ScopeItem::Poisoned, pos);
            return;
        }
        if let Some(tp) = &f.type_parameters {
            let bodiless = f.body.is_none();
            if bodiless {
                self.reject_subset(
                    RejectionSite::SourceFunctionBodyMissing,
                    "function bodies are required",
                    pos.clone(),
                );
            }
            let (type_params, duplicate_type_parameter) = self.collect_type_parameter_names(tp);
            self.generic_fns.insert(
                symbol.clone(),
                GenericFn {
                    file,
                    type_params,
                    function: Rc::new(f.clone_in(self.prog.allocator)),
                    rejected: bodiless || duplicate_type_parameter,
                },
            );
            self.register_scope_item(file, &name, ScopeItem::GenericFunc(symbol.clone()), pos);
        } else {
            // Placeholder; pass B fills the real signature.
            self.fn_sigs.insert(
                symbol.clone(),
                FnSig {
                    result_template: None,
                    generic: false,
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

    pub(super) fn reject_type_parameter_defaults(
        &mut self,
        params: &ast::TSTypeParameterDeclaration,
    ) -> bool {
        let mut rejected = false;
        for parameter in &params.params {
            if let Some(default) = &parameter.default {
                rejected = true;
                self.reject_subset(RejectionSite::TypeParameterDefault, "type-parameter defaults are not in the decided surface; pass an explicit type argument", self.pos(default.span()));
            }
        }
        rejected
    }

    pub(super) fn collect_type_parameter_names(
        &mut self,
        params: &ast::TSTypeParameterDeclaration,
    ) -> (Vec<String>, bool) {
        let mut names = HashSet::new();
        let mut duplicate = false;
        let names = params
            .params
            .iter()
            .map(|parameter| {
                let name = parameter.name.name.to_string();
                if !names.insert(name.clone()) {
                    duplicate = true;
                    self.reject_subset(
                        RejectionSite::DuplicateTypeParameter,
                        format!("duplicate type parameter `{name}`"),
                        self.pos(parameter.name.span),
                    );
                }
                name
            })
            .collect();
        (names, duplicate)
    }

    fn collect_globals(&mut self, file: usize, v: &ast::VariableDeclaration) {
        for d in &v.declarations {
            let ast::BindingPattern::BindingIdentifier(binding) = &d.id else {
                self.reject_outer_pattern(file, &d.id);
                continue;
            };
            let name = binding.name.to_string();
            let pos = self.pos(binding.span);
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
    fn reject_outer_pattern(&mut self, file: usize, pat: &ast::BindingPattern) {
        self.reject_subset(RejectionSite::ModuleBindingPattern, "a binding pattern binds inside a function body; a declaration outside one binds one name", self.pos(pat.span()));
        for binding in pattern::collect_names(pat) {
            let name = binding.name.to_string();
            let pos = self.pos(binding.span);
            self.register_scope_item(file, &name, ScopeItem::Poisoned, pos);
        }
    }

    fn collect_enum(&mut self, file: usize, e: &ast::TSEnumDeclaration) {
        let name = e.id.name.to_string();
        let pos = self.pos(e.id.span);
        // compiler.md §241 rule 1: a `const enum` has no value at run time,
        // so a read before it is not a read before its declaration.
        if !e.declare && !e.r#const {
            self.record_declaration_start(file, &name, &pos);
        }
        let diagnostics_before = self.diags.len();
        let mut members = Vec::new();
        // `None` means the previous member's value + 1 overflows i32, so
        // the next implicit value has no representation.
        let mut next: Option<i64> = Some(0);
        for m in &e.body.members {
            let member_name = match &m.id {
                ast::TSEnumMemberName::Identifier(id) => id.name.to_string(),
                ast::TSEnumMemberName::String(s) => {
                    let p = self.pos(s.span);
                    self.reject_subset(
                        RejectionSite::EnumStringMemberName,
                        "string enum member names are not decided",
                        p,
                    );
                    continue;
                }
                ast::TSEnumMemberName::ComputedString(_)
                | ast::TSEnumMemberName::ComputedTemplateString(_) => {
                    self.reject_subset(
                        RejectionSite::ParserSyntaxError,
                        "parse error: A computed property name is not allowed in an enum.",
                        self.pos(m.span),
                    );
                    continue;
                }
            };
            let value = match &m.initializer {
                None => match next {
                    Some(v) => v,
                    None => {
                        let p = self.pos(m.span);
                        self.reject_subset(
                            RejectionSite::EnumImplicitValueOverflow,
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
                        self.reject_subset(
                            if matches!(init, ast::Expression::StringLiteral(_)) {
                                RejectionSite::EnumStringValue
                            } else {
                                RejectionSite::EnumNonIntegerMember
                            },
                            "enum members must have integer literal values",
                            p,
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
        self.register_scope_item(
            file,
            &name,
            if self.diags.len() == diagnostics_before {
                ScopeItem::Enum(id)
            } else {
                ScopeItem::Poisoned
            },
            pos,
        );
    }

    fn collect_string_alias(&mut self, file: usize, alias: &ast::TSTypeAliasDeclaration) {
        let name = alias.id.name.to_string();
        let pos = self.pos(alias.id.span);
        if alias
            .type_parameters
            .as_deref()
            .is_some_and(|params| self.reject_type_parameter_defaults(params))
        {
            self.register_scope_item(file, &name, ScopeItem::Poisoned, pos);
            return;
        }
        if alias.type_parameters.is_some() {
            self.reject_subset(
                if !crate::ambient::lib_type_name(&name) {
                    RejectionSite::GenericUserAlias
                } else {
                    RejectionSite::GenericSourceAlias
                },
                "source type aliases cannot be generic",
                pos,
            );
            return;
        }
        if let Some(mapping) = wire_alias_literal(&alias.type_annotation) {
            self.collect_wire_string_alias(file, alias, mapping);
            return;
        }
        if self.type_scope_item("TaskGroup").is_none()
            && Self::annotation_has_task_group(&alias.type_annotation)
        {
            self.reject_subset(
                RejectionSite::TaskGroupPosition,
                "a type alias cannot hold a TaskGroup",
                pos.clone(),
            );
            return;
        }
        if let ast::TSType::TSUnionType(union) = &alias.type_annotation {
            // §195.1 rule 5: a type alias is not the result position.
            if let Some(member) = union
                .types
                .iter()
                .find(|member| matches!(&**member, ast::TSType::TSUndefinedKeyword(_)))
            {
                self.reject_subset(
                    RejectionSite::UndefinedUnionMember,
                    "`T | undefined` is legal only as the result type of a function declaration or a class method",
                    self.pos(member.span()),
                );
                return;
            }
        }
        let Some(members) = string_alias_members(&alias.type_annotation) else {
            // §202 rule 1: a second name of the aliased type, which
            // resolves after every module name is bound (rule 3).
            let id = self.source_aliases.len();
            self.source_aliases
                .push(super::source_alias::SourceAlias::new(
                    file,
                    &name,
                    pos.clone(),
                ));
            self.register_scope_item(file, &name, ScopeItem::SourceAlias(id), pos);
            return;
        };
        if members.len() > i32::MAX as usize {
            self.reject_subset(
                RejectionSite::LiteralAliasDiscriminantLimit,
                "string-literal union has more members than fit its i32 discriminant",
                self.pos(alias.type_annotation.span()),
            );
            return;
        }
        let mut seen = HashSet::new();
        if let Some(duplicate) = members
            .iter()
            .find(|member| !seen.insert((*member).clone()))
        {
            self.reject_subset(
                RejectionSite::DuplicateLiteralAliasMember,
                format!("duplicate string-literal union member `{duplicate}`"),
                self.pos(alias.type_annotation.span()),
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
        alias: &ast::TSTypeAliasDeclaration,
        mapping: &ast::TSTypeLiteral,
    ) {
        let name = alias.id.name.to_string();
        let pos = self.pos(alias.id.span);
        if mapping.members.is_empty() {
            self.reject_subset(
                RejectionSite::WireEnumEmpty,
                "wire-mapped string-literal union must have at least one member",
                self.pos(mapping.span),
            );
            return;
        }
        if mapping.members.len() > i32::MAX as usize {
            self.reject_subset(
                RejectionSite::WireAliasDiscriminantLimit,
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
            let ast::TSSignature::TSPropertySignature(property) = element else {
                self.reject_subset(
                    if matches!(element, ast::TSSignature::TSMethodSignature(_)) {
                        RejectionSite::WireEnumMethodMember
                    } else {
                        RejectionSite::WireEnumMemberForm
                    },
                    "CEnum mappings contain only named properties with integer-literal values",
                    self.pos(element.span()),
                );
                return;
            };
            let member = match &property.key {
                ast::PropertyKey::StringLiteral(value) => value.value.to_string(),
                ast::PropertyKey::StaticIdentifier(value) if !property.computed => {
                    value.name.to_string()
                }
                _ => {
                    self.reject_subset(
                        RejectionSite::WireEnumMemberKey,
                        "CEnum member keys must be string literals or identifiers",
                        self.pos(property.key.span()),
                    );
                    return;
                }
            };
            if !seen_members.insert(member.clone()) {
                self.reject_subset(
                    RejectionSite::DuplicateWireAliasMember,
                    format!("duplicate string-literal union member `{member}`"),
                    self.pos(property.key.span()),
                );
                return;
            }
            let Some(annotation) = &property.type_annotation else {
                self.reject_subset(
                    RejectionSite::WireEnumUntypedMember,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(property.span),
                );
                return;
            };
            let number = match &annotation.type_annotation {
                ast::TSType::TSLiteralType(literal) => match &literal.literal {
                    ast::TSLiteral::NumericLiteral(number) => {
                        Some((number.as_ref(), false, number.span))
                    }
                    ast::TSLiteral::UnaryExpression(unary)
                        if unary.operator == ast::UnaryOperator::UnaryNegation =>
                    {
                        match &unary.argument {
                            ast::Expression::NumericLiteral(number) => {
                                Some((number.as_ref(), true, unary.span))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                },
                _ => None,
            };
            let Some((number, negative, number_span)) = number else {
                self.reject_subset(
                    RejectionSite::WireEnumMemberNonIntegerSyntax,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(annotation.type_annotation.span()),
                );
                return;
            };
            let value = if negative {
                -number.value
            } else {
                number.value
            };
            if !value.is_finite() || value.fract() != 0.0 {
                self.reject_subset(
                    RejectionSite::WireEnumMemberNonIntegerValue,
                    format!("wire value for CEnum member `{member}` must be an integer literal"),
                    self.pos(number_span),
                );
                return;
            }
            if value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
                let spelling = number.raw.as_ref().map_or_else(
                    || value.to_string(),
                    |raw| {
                        if negative {
                            format!("-{raw}")
                        } else {
                            raw.to_string()
                        }
                    },
                );
                self.reject_subset(
                    RejectionSite::WireEnumValueRange,
                    format!(
                        "wire value {spelling} for CEnum member `{member}` is outside the i32 range"
                    ),
                    self.pos(number_span),
                );
                return;
            }
            let wire = value as i32;
            if let Some(first) = seen_wires.insert(wire, member.clone()) {
                self.reject_subset(
                    RejectionSite::DuplicateWireEnumValue,
                    format!(
                        "duplicate CEnum wire value {wire} for members `{first}` and `{member}`"
                    ),
                    self.pos(number_span),
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

    fn const_int_of(&self, e: &ast::Expression) -> Option<i32> {
        fn read(e: &ast::Expression, negate: bool) -> Option<i32> {
            match e {
                ast::Expression::NumericLiteral(number) => {
                    let raw = number.raw.as_deref()?;
                    i32::try_from(parse_integer_spelling(raw, negate)?).ok()
                }
                ast::Expression::UnaryExpression(unary)
                    if unary.operator == ast::UnaryOperator::UnaryNegation =>
                {
                    read(&unary.argument, !negate)
                }
                ast::Expression::ParenthesizedExpression(paren) => read(&paren.expression, negate),
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
    fn collect_mirror_decl(&mut self, file: usize, decl: &ast::Declaration) {
        match decl {
            ast::Declaration::TSInterfaceDeclaration(i) => self.collect_handle(file, i),
            ast::Declaration::ClassDeclaration(c) if c.type_parameters.is_none() => {
                self.collect_boundary_struct(file, c)
            }
            ast::Declaration::TSTypeAliasDeclaration(t) => {
                if string_alias_members(&t.type_annotation).is_some()
                    || wire_alias_literal(&t.type_annotation).is_some()
                {
                    self.collect_string_alias(file, t);
                } else {
                    // Reserve the name; the aliased type is resolved in pass B.
                    self.type_aliases
                        .entry(t.id.name.to_string())
                        .or_insert(Type::Error);
                }
            }
            ast::Declaration::FunctionDeclaration(f) => {
                let name = f.declaration_name().to_string();
                let pos = self.pos(f.declaration_name_span());
                self.register_scope_item(file, &name, ScopeItem::Foreign(name.clone()), pos);
            }
            ast::Declaration::VariableDeclaration(v) => self.collect_ambient_consts(file, v),
            ast::Declaration::TSEnumDeclaration(e) => self.collect_enum(file, e),
            other => {
                let pos = self.pos(other.span());
                self.reject_subset(
                    if matches!(other, ast::Declaration::TSExternalModuleDeclaration(_)) {
                        RejectionSite::MirrorStringModuleDeclaration
                    } else {
                        RejectionSite::MirrorModuleDeclaration
                    },
                    "mirror declaration form outside the decided surface",
                    pos,
                );
                for name in super::exports::declaration_names(other) {
                    self.register_scope_item(
                        file,
                        name.name.as_ref(),
                        ScopeItem::Poisoned,
                        self.pos(name.span),
                    );
                }
            }
        }
    }

    /// An ambient `interface` in a mirror is an opaque handle (Q13): an
    /// empty branded nominal type. Its members (the phantom brand) carry
    /// no in-language meaning and are ignored; it lowers to a
    /// pointer-sized handle (a reference-shaped nominal, non-value).
    fn collect_handle(&mut self, file: usize, i: &ast::TSInterfaceDeclaration) {
        let name = i.id.name.to_string();
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
    fn collect_boundary_struct(&mut self, file: usize, c: &ast::Class) {
        let name = c.declaration_name().to_string();
        let pos = self.pos(c.declaration_name_span());
        let id = self.new_class(
            &self.declaration_symbol(file, &name),
            true,
            false,
            None,
            pos.clone(),
        );
        self.boundary_classes.insert(id);
        self.classes[id.0].is_boundary = true;
        self.classes[id.0].boundary_header = self
            .foreign_mirror_ids
            .get(&file)
            .map(|mirror| self.foreign_mirrors[mirror.0].include.clone());
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

    /// A mirror `declare const X = <integer literal>;` (flag constant,
    /// Q13): a read-only ambient global whose value pass B folds. Every
    /// other mirror variable declaration is one S100 at the declared name,
    /// and the name binds poisoned, so a use reports nothing more
    /// (compiler.md §136.1 rules 1 and 2).
    fn collect_ambient_consts(&mut self, file: usize, v: &ast::VariableDeclaration) {
        for d in &v.declarations {
            let ast::BindingPattern::BindingIdentifier(binding) = &d.id else {
                self.reject_outer_pattern(file, &d.id);
                continue;
            };
            let name = binding.name.to_string();
            let pos = self.pos(binding.span);
            if mirror_const_value(v, d).is_none() {
                self.reject_subset(
                    RejectionSite::MirrorVariableForm,
                    format!(
                        "mirror variable `{name}` is outside the decided surface; the one \
                         accepted form is `declare const X = <integer literal>;`"
                    ),
                    pos.clone(),
                );
                self.register_scope_item(file, &name, ScopeItem::Poisoned, pos);
                continue;
            }
            self.register_scope_item(
                file,
                &name,
                ScopeItem::Global(self.declaration_symbol(file, &name)),
                pos,
            );
        }
    }
}
