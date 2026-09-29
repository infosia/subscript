use super::*;

impl<'p> Checker<'p> {
    fn claim_class_member_name(
        &mut self,
        id: ClassId,
        name: &str,
        declaration: ClassMemberDeclaration,
        is_static: bool,
        pos: Pos,
    ) -> bool {
        use ClassMemberDeclaration::{Field, Method, ReadAccessor, WriteAccessor};
        use ClassMemberNamespaceEntry::Accessor;

        let namespace = if is_static {
            &self.class_sigs[id.0].static_member_namespace
        } else {
            &self.class_sigs[id.0].member_namespace
        };
        let existing = namespace.get(name).copied();
        let entry = match (existing, declaration) {
            (None, Field) => ClassMemberNamespaceEntry::Field,
            (None, Method) => ClassMemberNamespaceEntry::Method,
            (None, ReadAccessor) => Accessor {
                read: true,
                write: false,
            },
            (None, WriteAccessor) => Accessor {
                read: false,
                write: true,
            },
            (Some(Accessor { read: true, .. }), ReadAccessor) => {
                self.error(
                    RuleCode::S017,
                    format!(
                        "two {}accessors cannot declare the read member `{name}`",
                        if is_static { "static " } else { "" }
                    ),
                    pos,
                );
                return false;
            }
            (Some(Accessor { write: true, .. }), WriteAccessor) => {
                self.error(
                    RuleCode::S017,
                    format!(
                        "two {}accessors cannot declare the write member `{name}`",
                        if is_static { "static " } else { "" }
                    ),
                    pos,
                );
                return false;
            }
            (Some(Accessor { write, .. }), ReadAccessor) => Accessor { read: true, write },
            (Some(Accessor { read, .. }), WriteAccessor) => Accessor { read, write: true },
            (Some(existing), declaration) => {
                let existing_kind = match existing {
                    ClassMemberNamespaceEntry::Field => "field",
                    ClassMemberNamespaceEntry::Method => "method",
                    Accessor { .. } => "accessor",
                };
                let declared_kind = match declaration {
                    Field => "field",
                    Method => "method",
                    ReadAccessor | WriteAccessor => "accessor",
                };
                let message = match (existing_kind, declared_kind) {
                    ("accessor", "field") | ("field", "accessor") => {
                        format!("a field and an accessor cannot share the member name `{name}`")
                    }
                    ("accessor", "method") | ("method", "accessor") => {
                        format!("a method and an accessor cannot share the member name `{name}`")
                    }
                    _ => format!(
                        "a {declared_kind} cannot share the member name `{name}` with a {existing_kind}"
                    ),
                };
                let message = if is_static {
                    message.replacen("a ", "a static ", 1)
                } else {
                    message
                };
                self.error(RuleCode::S017, message, pos);
                return false;
            }
        };
        if is_static {
            self.class_sigs[id.0]
                .static_member_namespace
                .insert(name.to_string(), entry);
        } else {
            self.class_sigs[id.0]
                .member_namespace
                .insert(name.to_string(), entry);
        }
        true
    }

    /// The statically named member of a method declaration, including a
    /// literal key whose spelling the collection rules reject.
    pub(super) fn class_method_name(key: &ast::PropName) -> Option<String> {
        if is_dispose_method_key(key) {
            return Some(hir::DISPOSE_METHOD_NAME.to_string());
        }
        match key {
            ast::PropName::Ident(key) => Some(key.sym.to_string()),
            ast::PropName::Str(key) => Some(key.value.to_string()),
            ast::PropName::Num(key) => Some(key.value.to_string()),
            ast::PropName::BigInt(key) => Some(key.value.to_string()),
            ast::PropName::Computed(key) => match &*key.expr {
                ast::Expr::Lit(ast::Lit::Str(key)) => Some(key.value.to_string()),
                ast::Expr::Lit(ast::Lit::Num(key)) => Some(key.value.to_string()),
                _ => None,
            },
        }
    }

    fn resolve_class_method(
        &mut self,
        id: ClassId,
        method: &ast::ClassMethod,
        declared: bool,
        write_accessors: &mut Vec<(String, Pos, bool)>,
    ) {
        let is_value = self.classes[id.0].is_value;
        let is_descriptor = self.classes[id.0].is_descriptor;
        let (name, key_pos, is_dispose) = match &method.key {
            ast::PropName::Ident(key) => (key.sym.to_string(), self.pos(key.span), false),
            key if is_dispose_method_key(key) => (
                hir::DISPOSE_METHOD_NAME.to_string(),
                self.pos(method.span),
                true,
            ),
            _ => {
                let pos = self.pos(method.span);
                self.error(RuleCode::S100, "computed method names are not decided", pos);
                return;
            }
        };
        if is_descriptor {
            if is_dispose {
                self.error_diverging(
                    RuleCode::S100,
                    "descriptor classes cannot declare `[Symbol.dispose]()`",
                    key_pos,
                    Divergence::UsingDeclaration,
                );
                return;
            }
            self.error(
                RuleCode::S100,
                if method.kind != ast::MethodKind::Method {
                    "descriptor classes cannot declare accessors"
                } else {
                    "descriptor classes cannot declare methods"
                },
                key_pos,
            );
            return;
        }
        if method.is_static && (self.in_boundary || self.classes[id.0].is_boundary) {
            self.error(
                RuleCode::S100,
                "mirror classes cannot declare static methods or accessors",
                key_pos,
            );
            return;
        }
        if is_dispose && method.is_static {
            self.error(
                RuleCode::S100,
                "`[Symbol.dispose]()` must be non-static",
                key_pos,
            );
            return;
        }
        if method.is_static && method.function.is_async {
            self.error_diverging(
                RuleCode::S100,
                "async static methods are not in the decided surface",
                self.pos(method.span),
                Divergence::AsyncFunctionShape,
            );
            return;
        }
        if is_dispose && is_value {
            self.error_diverging(
                RuleCode::S100,
                "value classes cannot declare `[Symbol.dispose]()`",
                key_pos,
                Divergence::UsingDeclaration,
            );
            return;
        }
        if method.kind != ast::MethodKind::Method && self.in_boundary {
            let pos = self.pos(method.span);
            self.error(
                RuleCode::S100,
                "mirror classes cannot declare accessors",
                pos,
            );
            return;
        }
        if method.kind == ast::MethodKind::Getter {
            if !self.claim_class_member_name(
                id,
                &name,
                ClassMemberDeclaration::ReadAccessor,
                method.is_static,
                key_pos.clone(),
            ) {
                return;
            }
            if !method.function.params.is_empty() {
                self.error(
                    RuleCode::S100,
                    "a read accessor must declare no parameters",
                    key_pos.clone(),
                );
                return;
            }
            let Some(return_type) = &method.function.return_type else {
                self.error(
                    RuleCode::S100,
                    "a read accessor requires an explicit return type",
                    key_pos,
                );
                return;
            };
            let sig = FnSig {
                params: Vec::new(),
                ret: self.resolve_type(&return_type.type_ann),
                is_generator: false,
                is_async: false,
                yield_known: true,
            };
            if method.is_static {
                let symbol = static_member_symbol(id, &self.classes[id.0].name, &name);
                self.class_sigs[id.0]
                    .static_methods
                    .insert(name, sig.clone());
                self.fn_sigs.insert(symbol, sig);
            } else {
                self.class_sigs[id.0].methods.insert(name, sig);
            }
            return;
        }
        if method.kind == ast::MethodKind::Setter {
            let write_name = format!("{name}=");
            if is_value && !method.is_static {
                let class_name = self.classes[id.0].name.clone();
                self.error_diverging(
                    RuleCode::S100,
                    format!("value class `{class_name}` cannot declare a write accessor"),
                    key_pos,
                    Divergence::NamedAccessor,
                );
                return;
            }
            if !self.claim_class_member_name(
                id,
                &name,
                ClassMemberDeclaration::WriteAccessor,
                method.is_static,
                key_pos.clone(),
            ) {
                return;
            }
            if method.function.return_type.is_some() {
                self.error(
                    RuleCode::S100,
                    "a write accessor cannot declare a return type",
                    key_pos,
                );
                return;
            }
            let [parameter] = method.function.params.as_slice() else {
                self.error(
                    RuleCode::S100,
                    "a write accessor must declare exactly one parameter",
                    key_pos.clone(),
                );
                return;
            };
            let binding = match &parameter.pat {
                ast::Pat::Ident(binding) => binding,
                ast::Pat::Assign(_) => {
                    self.error(
                        RuleCode::S100,
                        "a write accessor parameter cannot have a default",
                        key_pos.clone(),
                    );
                    return;
                }
                _ => {
                    self.error(
                        RuleCode::S100,
                        "a write accessor parameter must be an identifier",
                        key_pos.clone(),
                    );
                    return;
                }
            };
            let Some(annotation) = &binding.type_ann else {
                self.error(
                    RuleCode::S100,
                    "a write accessor parameter requires a type annotation",
                    key_pos.clone(),
                );
                return;
            };
            let sig = FnSig {
                params: vec![ParamSig {
                    name: binding.id.sym.to_string(),
                    ty: self.resolve_type(&annotation.type_ann),
                    has_default: false,
                }],
                ret: Type::Void,
                is_generator: false,
                is_async: false,
                yield_known: true,
            };
            write_accessors.push((name.clone(), key_pos, method.is_static));
            if method.is_static {
                let symbol = static_member_symbol(id, &self.classes[id.0].name, &write_name);
                self.class_sigs[id.0]
                    .static_methods
                    .insert(write_name, sig.clone());
                self.fn_sigs.insert(symbol, sig);
            } else {
                self.class_sigs[id.0].methods.insert(write_name, sig);
            }
            return;
        }
        if !self.claim_class_member_name(
            id,
            &name,
            ClassMemberDeclaration::Method,
            method.is_static,
            key_pos.clone(),
        ) {
            return;
        }
        if method.function.is_generator && !method.is_static {
            let pos = self.pos(method.span);
            if method.function.is_async {
                self.error_diverging(
                    RuleCode::S100,
                    "async generator methods are not in the decided surface",
                    pos,
                    Divergence::AsyncFunctionShape,
                );
            } else {
                self.error(
                    RuleCode::S100,
                    "generator methods are not in the decided surface",
                    pos,
                );
            }
            return;
        }
        if method.function.is_async && is_value && !method.is_static {
            let pos = self.pos(method.span);
            self.error_diverging(
                RuleCode::S100,
                "async methods on `@CStruct` value classes are not in the decided surface",
                pos,
                Divergence::AsyncFunctionShape,
            );
            return;
        }
        // §82.4 rules 1 and 5: a method with type parameters
        // collects as a template. Each call instantiates it.
        if !(is_dispose || self.in_boundary || self.classes[id.0].is_boundary)
            && method.function.type_params.is_some()
        {
            let bodiless = method.function.body.is_none();
            if bodiless && declared && !method.function.is_async {
                self.error_diverging(
                    RuleCode::S100,
                    "function bodies are required",
                    key_pos.clone(),
                    Divergence::BodilessDeclareGenericMethod,
                );
            } else if bodiless {
                self.error(
                    RuleCode::S100,
                    "function bodies are required",
                    key_pos.clone(),
                );
            }
            let (type_params, duplicate_type_parameter) = method
                .function
                .type_params
                .as_deref()
                .map(|declaration| self.collect_type_parameter_names(declaration))
                .unwrap_or_default();
            let template = GenericMethod {
                file: self.cur_file,
                type_params,
                function: (*method.function).clone(),
                rejected: bodiless || duplicate_type_parameter,
            };
            if method.is_static {
                self.class_sigs[id.0]
                    .static_generic_methods
                    .insert(name, template);
            } else {
                self.class_sigs[id.0].generic_methods.insert(name, template);
            }
            return;
        }
        if is_dispose && method.function.is_async {
            self.error(
                RuleCode::S100,
                "`[Symbol.dispose]()` must be synchronous",
                key_pos,
            );
            return;
        }
        let sig = self.resolve_fn_sig(&method.function, key_pos.clone());
        if is_dispose && (!sig.params.is_empty() || sig.ret != Type::Void) {
            self.error(
                RuleCode::S100,
                "`[Symbol.dispose]()` takes no parameters and returns `void`",
                key_pos,
            );
            return;
        }
        if method.is_static {
            let symbol = static_member_symbol(id, &self.classes[id.0].name, &name);
            self.class_sigs[id.0]
                .static_methods
                .insert(name, sig.clone());
            self.fn_sigs.insert(symbol, sig);
        } else {
            self.class_sigs[id.0].methods.insert(name, sig);
        }
    }

    /// Resolves a class's fields and callable signatures (pass B), and
    /// enforces C2 (no inheritance for value classes; field whitelist).
    pub(crate) fn resolve_class_shape(&mut self, id: ClassId, class: &ast::Class, declared: bool) {
        if declared {
            self.declared_classes.insert(id);
        }
        let is_value = self.classes[id.0].is_value;
        let is_descriptor = self.classes[id.0].is_descriptor;
        let mut index_signature_pos = None;
        let mut write_accessors = Vec::new();
        if let Some(sup) = &class.super_class {
            let pos = self.pos(sup.span());
            if is_value {
                self.error_diverging(
                    RuleCode::S006,
                    "value classes do not inherit",
                    pos,
                    Divergence::ValueClassLayout,
                );
            } else if is_descriptor {
                self.error(RuleCode::S100, "descriptor classes do not inherit", pos);
            } else {
                self.error(
                    RuleCode::S100,
                    "class inheritance is not in the decided surface",
                    pos,
                );
            }
        }
        for member in &class.body {
            match member {
                ast::ClassMember::ClassProp(prop) => {
                    let ast::PropName::Ident(key) = &prop.key else {
                        let pos = self.pos(prop.span);
                        self.error(
                            RuleCode::S100,
                            "computed or non-identifier field names are not decided",
                            pos,
                        );
                        continue;
                    };
                    if prop.is_static {
                        let pos = self.pos(key.span);
                        if is_descriptor {
                            self.error(
                                RuleCode::S100,
                                "descriptor classes cannot declare static fields",
                                pos,
                            );
                            continue;
                        }
                        if self.in_boundary || self.classes[id.0].is_boundary {
                            self.error(
                                RuleCode::S100,
                                "mirror classes cannot declare static fields",
                                pos,
                            );
                            continue;
                        }
                        let name = key.sym.to_string();
                        if !self.claim_class_member_name(
                            id,
                            &name,
                            ClassMemberDeclaration::Field,
                            true,
                            self.pos(key.span),
                        ) {
                            continue;
                        }
                        if prop.is_optional {
                            self.error(
                                RuleCode::S012,
                                "optional static fields imply `undefined`; use `T | null`",
                                self.pos(prop.span),
                            );
                        }
                        let ty = match &prop.type_ann {
                            Some(annotation) => self.resolve_type(&annotation.type_ann),
                            None => {
                                self.error(
                                    RuleCode::S100,
                                    "static fields require a type annotation",
                                    self.pos(key.span),
                                );
                                Type::Error
                            }
                        };
                        if Self::is_context_affine_type(&ty) {
                            self.error(
                                RuleCode::S100,
                                "Worker, Inbox, and Outbox values may not be static fields",
                                self.pos(key.span),
                            );
                        }
                        let signature = GlobalSig {
                            ty,
                            mutable: !prop.readonly,
                        };
                        let symbol = static_member_symbol(id, &self.classes[id.0].name, &name);
                        self.global_sigs.insert(symbol, signature.clone());
                        self.class_sigs[id.0].static_fields.insert(name, signature);
                        continue;
                    }
                    let name = key.sym.to_string();
                    if !self.claim_class_member_name(
                        id,
                        &name,
                        ClassMemberDeclaration::Field,
                        false,
                        self.pos(key.span),
                    ) {
                        continue;
                    }
                    let is_defaulted =
                        is_descriptor && prop.is_optional && prop.value.is_some() && !prop.definite;
                    if is_descriptor {
                        match (prop.definite, prop.is_optional, prop.value.is_some()) {
                            (true, false, false) | (false, true, true) | (false, true, false) => {}
                            (_, true, false) => {
                                let pos = self.pos(prop.span);
                                self.error_diverging(
                                    RuleCode::S012,
                                    "optional descriptor members require a default initializer",
                                    pos,
                                    Divergence::OptionalDescriptorMember,
                                );
                            }
                            (true, _, true) => {
                                let pos = self.pos(prop.span);
                                self.error(
                                    RuleCode::S100,
                                    "a required descriptor member (`name!: T`) cannot have an initializer",
                                    pos,
                                );
                            }
                            (false, false, true) => {
                                let pos = self.pos(prop.span);
                                self.error(
                                    RuleCode::S100,
                                    "a descriptor member initializer requires the optional `?` spelling",
                                    pos,
                                );
                            }
                            _ => {
                                let pos = self.pos(prop.span);
                                self.error(
                                    RuleCode::S100,
                                    "required descriptor members must be spelled `name!: T`",
                                    pos,
                                );
                            }
                        }
                    } else if prop.is_optional {
                        let pos = self.pos(prop.span);
                        self.error(
                            RuleCode::S012,
                            "optional properties imply `undefined`; use `T | null`",
                            pos,
                        );
                    }
                    let pos = self.pos(key.span);
                    let ty = match &prop.type_ann {
                        Some(ann) => {
                            let allow_wire =
                                self.in_boundary && self.boundary_classes.contains(&id);
                            self.allow_wire_alias_boundary = allow_wire;
                            let ty = self.resolve_type(&ann.type_ann);
                            self.allow_wire_alias_boundary = false;
                            ty
                        }
                        None => {
                            self.error(
                                RuleCode::S100,
                                "fields require a type annotation",
                                pos.clone(),
                            );
                            Type::Error
                        }
                    };
                    if self.in_boundary
                        && Self::contains_string_alias(&ty)
                        && !Self::supported_wire_alias_boundary_type(&ty)
                    {
                        self.error(
                            RuleCode::S100,
                            format!(
                                "wire-mapped aliases are supported only as direct boundary-struct members or array-pair elements; member `{}` nests one inside another boundary type",
                                key.sym
                            ),
                            pos.clone(),
                        );
                    }
                    let is_absence_capable = is_descriptor
                        && !prop.definite
                        && prop.is_optional
                        && prop.value.is_none()
                        && matches!(ty, Type::StringAlias(_));
                    if is_descriptor
                        && !prop.definite
                        && prop.is_optional
                        && prop.value.is_none()
                        && !matches!(ty, Type::StringAlias(_) | Type::Error)
                    {
                        self.error_diverging(
                            RuleCode::S012,
                            "optional descriptor members require a default initializer",
                            self.pos(prop.span),
                            Divergence::OptionalDescriptorMember,
                        );
                    }
                    let context_affine = Self::is_context_affine_type(&ty);
                    if context_affine {
                        self.error(
                            RuleCode::S100,
                            "Worker, Inbox, and Outbox values may not be class fields",
                            pos.clone(),
                        );
                    }
                    let foreign_provenance = if self.in_boundary && matches!(ty, Type::Func(_)) {
                        self.callback_provenance(
                            self.cur_file,
                            prop.type_ann
                                .as_deref()
                                .map(|annotation| annotation.type_ann.as_ref()),
                            pos.clone(),
                        )
                    } else {
                        None
                    };
                    // Boundary structs (mirror-ingested) relax the C2
                    // value-field whitelist: they may carry `X | null`,
                    // `object | null`, and function-pointer fields.
                    if is_value
                        && !self.boundary_classes.contains(&id)
                        && !context_affine
                        && !self.value_field_ok(&ty)
                    {
                        self.error(
                            RuleCode::S100,
                            format!(
                                "field type `{}` is outside the value-class whitelist \
                                 (sized numerics, boolean, value classes, FixedArray, enums)",
                                self.type_name(&ty)
                            ),
                            pos.clone(),
                        );
                    }
                    self.classes[id.0].fields.push(hir::Field {
                        name: key.sym.to_string(),
                        ty,
                        is_defaulted,
                        is_absence_capable,
                        init: None,
                        foreign_provenance,
                        pos,
                    });
                }
                ast::ClassMember::Constructor(ctor) => {
                    if is_descriptor {
                        self.error(
                            RuleCode::S100,
                            "descriptor classes cannot declare constructors",
                            self.pos(ctor.span),
                        );
                        continue;
                    }
                    let mut params = Vec::new();
                    for p in &ctor.params {
                        match p {
                            ast::ParamOrTsParamProp::Param(param) => {
                                self.allow_wire_alias_boundary = self.in_boundary;
                                let resolved = self.resolve_param_pat(&param.pat);
                                self.allow_wire_alias_boundary = false;
                                if self.in_boundary
                                    && Self::contains_string_alias(&resolved.ty)
                                    && !Self::supported_wire_alias_boundary_type(&resolved.ty)
                                {
                                    self.error(
                                        RuleCode::S100,
                                        format!(
                                            "wire-mapped aliases are supported only as direct mirror-constructor parameters or array-pair elements; parameter `{}` nests one inside another boundary type",
                                            resolved.name
                                        ),
                                        self.pos(param.span),
                                    );
                                }
                                params.push(resolved);
                            }
                            ast::ParamOrTsParamProp::TsParamProp(pp) => {
                                let pos = self.pos(pp.span);
                                self.error(
                                    RuleCode::S100,
                                    "constructor parameter properties are not decided",
                                    pos,
                                );
                            }
                        }
                    }
                    self.class_sigs[id.0].ctor = Some(params);
                }
                ast::ClassMember::Method(method) => {
                    let diagnostics_before = self.diags.len();
                    self.resolve_class_method(id, method, declared, &mut write_accessors);
                    // §93 rule 12: every collection rejection preserves a
                    // template, including exits before normal collection.
                    if self.diags.len() != diagnostics_before {
                        if let Some(template) =
                            GenericMethod::rejected(self.cur_file, &method.function)
                        {
                            if let Some(name) = Self::class_method_name(&method.key) {
                                let templates = if method.is_static {
                                    &mut self.class_sigs[id.0].static_generic_methods
                                } else {
                                    &mut self.class_sigs[id.0].generic_methods
                                };
                                templates.insert(name, template);
                            }
                        }
                    }
                }
                ast::ClassMember::TsIndexSignature(signature) if !self.in_boundary => {
                    let pos = self.pos(signature.span);
                    if index_signature_pos.is_some() {
                        self.error(
                            RuleCode::S100,
                            "a class can declare at most one index signature",
                            pos,
                        );
                        continue;
                    }
                    index_signature_pos = Some(pos.clone());
                    if is_value || is_descriptor {
                        self.error(
                            RuleCode::S100,
                            "only reference classes can declare an index signature",
                            pos.clone(),
                        );
                    }
                    if signature.is_static {
                        self.error(
                            RuleCode::S100,
                            "a class index signature cannot be static",
                            pos.clone(),
                        );
                    }
                    let index_ty = match signature.params.as_slice() {
                        [ast::TsFnParam::Ident(binding)] => match &binding.type_ann {
                            Some(annotation) => self.resolve_type(&annotation.type_ann),
                            None => {
                                self.error(
                                    RuleCode::S100,
                                    "a class index signature parameter requires a type annotation",
                                    pos.clone(),
                                );
                                Type::Error
                            }
                        },
                        _ => {
                            self.error(
                                RuleCode::S100,
                                "a class index signature requires one identifier parameter",
                                pos.clone(),
                            );
                            Type::Error
                        }
                    };
                    if !matches!(index_ty, Type::I32 | Type::U32 | Type::Error) {
                        let actual = self.type_name(&index_ty);
                        self.error(
                            RuleCode::S100,
                            format!(
                                "a class index signature requires an `i32` or `u32` index, got `{actual}`"
                            ),
                            pos.clone(),
                        );
                    }
                    let element_ty = match &signature.type_ann {
                        Some(annotation) => self.resolve_type(&annotation.type_ann),
                        None => {
                            self.error(
                                RuleCode::S100,
                                "a class index signature requires an element type",
                                pos.clone(),
                            );
                            Type::Error
                        }
                    };
                    self.classes[id.0].index_signature = Some(hir::IndexSignature {
                        index_ty,
                        element_ty,
                        readonly: signature.readonly,
                    });
                }
                ast::ClassMember::Empty(_) => {}
                other => {
                    let pos = self.pos(other.span());
                    self.error(
                        RuleCode::S100,
                        "class member form outside the decided surface",
                        pos,
                    );
                }
            }
        }
        if let Some(pos) = index_signature_pos {
            self.validate_class_index_accessors(id, pos);
        }
        for (name, pos, is_static) in write_accessors {
            let has_read = if is_static {
                self.class_sigs[id.0].has_static_read_accessor(&name)
            } else {
                self.class_sigs[id.0].has_read_accessor(&name)
            };
            if !has_read {
                self.error(
                    RuleCode::S100,
                    format!(
                        "{}write accessor `{name}` requires a read accessor with the same name",
                        if is_static { "static " } else { "" }
                    ),
                    pos,
                );
                continue;
            }
            let methods = if is_static {
                &self.class_sigs[id.0].static_methods
            } else {
                &self.class_sigs[id.0].methods
            };
            let read_type = methods.get(&name).map(|signature| signature.ret.clone());
            let write_type = methods
                .get(&format!("{name}="))
                .and_then(|signature| signature.params.first())
                .map(|parameter| parameter.ty.clone());
            if let (Some(read_type), Some(write_type)) = (read_type, write_type) {
                if read_type != write_type {
                    self.error(
                        RuleCode::S100,
                        format!("the read and write accessors of `{name}` must have the same type"),
                        pos,
                    );
                }
            }
        }
    }

    fn validate_class_index_accessors(&mut self, id: ClassId, pos: Pos) {
        let Some(signature) = self.classes[id.0].index_signature.clone() else {
            return;
        };
        let get_matches = self.class_sigs[id.0]
            .methods
            .get("get")
            .is_some_and(|method| {
                !method.is_async
                    && !method.is_generator
                    && method.params.len() == 1
                    && !method.params[0].has_default
                    && method.params[0].ty == signature.index_ty
                    && method.ret == signature.element_ty
            });
        if !get_matches {
            let index = self.type_name(&signature.index_ty);
            let element = self.type_name(&signature.element_ty);
            self.error_diverging(
                RuleCode::S100,
                format!(
                    "the index signature requires `get(index: {index}): {element}` with exactly matching types"
                ),
                pos.clone(),
                Divergence::ClassIndexSignature,
            );
        }
        if signature.readonly {
            return;
        }
        let set_matches = self.class_sigs[id.0]
            .methods
            .get("set")
            .is_some_and(|method| {
                !method.is_async
                    && !method.is_generator
                    && method.params.len() == 2
                    && method.params.iter().all(|parameter| !parameter.has_default)
                    && method.params[0].ty == signature.index_ty
                    && method.params[1].ty == signature.element_ty
                    && method.ret == Type::Void
            });
        if !set_matches {
            let index = self.type_name(&signature.index_ty);
            let element = self.type_name(&signature.element_ty);
            self.error(
                RuleCode::S100,
                format!(
                    "the index signature requires `set(index: {index}, value: {element}): void` with exactly matching types"
                ),
                pos,
            );
        }
    }

    pub(crate) fn plain_value_leaf(&self, ty: &Type) -> bool {
        matches!(
            ty,
            Type::I8
                | Type::U8
                | Type::I16
                | Type::U16
                | Type::I32
                | Type::U32
                | Type::I64
                | Type::U64
                | Type::F16
                | Type::F32
                | Type::F64
                | Type::Bool
                | Type::Enum(_)
                | Type::StringAlias(_)
                | Type::Error
        )
    }

    fn value_field_ok(&self, ty: &Type) -> bool {
        if self.plain_value_leaf(ty) {
            return true;
        }
        match ty {
            Type::Class(id) => self.classes[id.0].is_value,
            Type::FixedArray(elem, _) => self.value_field_ok(elem),
            _ => false,
        }
    }
}
