use super::*;
use crate::check::rejection::RejectionSite;

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

        // §206.1 rule 1: one private name names one member, so a static
        // and an instance member cannot share it.
        let other = if is_static {
            &self.class_sigs[id.0].member_namespace
        } else {
            &self.class_sigs[id.0].static_member_namespace
        };
        if crate::parse::private_name::is_private_name(name) && other.contains_key(name) {
            self.reject_subset(
                RejectionSite::PrivateNameStaticAndInstance,
                format!("static and instance members cannot share the private name `{name}`"),
                pos,
            );
            return false;
        }
        let namespace = if is_static {
            &self.class_sigs[id.0].static_member_namespace
        } else {
            &self.class_sigs[id.0].member_namespace
        };
        let existing = namespace.get(name).copied();
        let entry = match (existing, declaration) {
            (None, Field) => ClassMemberNamespaceEntry::Field,
            (None, Method { has_body }) => ClassMemberNamespaceEntry::Method { has_body },
            (None, ReadAccessor) => Accessor {
                read: true,
                write: false,
            },
            (None, WriteAccessor) => Accessor {
                read: false,
                write: true,
            },
            (Some(Accessor { read: true, .. }), ReadAccessor) => {
                self.reject_subset(
                    RejectionSite::DuplicateReadAccessor,
                    format!(
                        "two {}accessors cannot declare the read member `{name}`",
                        if is_static { "static " } else { "" }
                    ),
                    pos,
                );
                return false;
            }
            (Some(Accessor { write: true, .. }), WriteAccessor) => {
                self.reject_subset(
                    RejectionSite::DuplicateWriteAccessor,
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
                    ClassMemberNamespaceEntry::Method { .. } => "method",
                    Accessor { .. } => "accessor",
                };
                let declared_kind = match declaration {
                    Field => "field",
                    Method { .. } => "method",
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
                let overload = matches!((existing, declaration),
                    (ClassMemberNamespaceEntry::Method { has_body: first }, Method { has_body: second }) if !first || !second);
                self.reject_subset(
                    if overload {
                        RejectionSite::ClassMemberNameClash
                    } else {
                        RejectionSite::ClassMemberDuplicateImplementation
                    },
                    message,
                    pos,
                );
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
    pub(super) fn class_method_name(key: &ast::PropertyKey) -> Option<String> {
        if is_dispose_method_key(key) {
            return Some(hir::DISPOSE_METHOD_NAME.to_string());
        }
        match key {
            ast::PropertyKey::StaticIdentifier(key) => Some(key.name.to_string()),
            ast::PropertyKey::StringLiteral(key) => Some(key.value.to_string()),
            ast::PropertyKey::NumericLiteral(key) => Some(key.value.to_string()),
            ast::PropertyKey::BigIntLiteral(key) => Some(key.value.to_string()),
            _ => None,
        }
    }

    fn resolve_class_method(
        &mut self,
        id: ClassId,
        method: &ast::MethodDefinition,
        declared: bool,
        write_accessors: &mut Vec<(String, Pos, bool)>,
    ) {
        let is_value = self.classes[id.0].is_value;
        let is_descriptor = self.classes[id.0].is_descriptor;
        let (name, key_pos, is_dispose) = match &method.key {
            ast::PropertyKey::StaticIdentifier(key) => {
                (key.name.to_string(), self.pos(key.span), false)
            }
            key if is_dispose_method_key(key) => (
                hir::DISPOSE_METHOD_NAME.to_string(),
                self.pos(method.span),
                true,
            ),
            _ => {
                let pos = self.pos(method.span);
                self.reject_subset(
                    RejectionSite::ComputedMethodName,
                    "computed method names are not decided",
                    pos,
                );
                return;
            }
        };
        if is_descriptor {
            if is_dispose {
                self.reject_subset(
                    RejectionSite::DescriptorDisposeMethod,
                    "descriptor classes cannot declare `[Symbol.dispose]()`",
                    key_pos,
                );
                return;
            }
            self.reject_subset(
                RejectionSite::DescriptorMethod,
                if method.kind != ast::MethodDefinitionKind::Method {
                    "descriptor classes cannot declare accessors"
                } else {
                    "descriptor classes cannot declare methods"
                },
                key_pos,
            );
            return;
        }
        if method.r#static && (self.in_boundary || self.classes[id.0].is_boundary) {
            self.reject_subset(
                RejectionSite::MirrorStaticMethod,
                "mirror classes cannot declare static methods or accessors",
                key_pos,
            );
            return;
        }
        if is_dispose && method.r#static {
            self.reject_subset(
                RejectionSite::DisposeStatic,
                "`[Symbol.dispose]()` must be non-static",
                key_pos,
            );
            return;
        }
        if method.r#static && method.value.r#async {
            self.reject_subset(
                RejectionSite::AsyncStaticMethod,
                "async static methods are not in the decided surface",
                self.pos(method.span),
            );
            return;
        }
        if is_dispose && is_value {
            self.reject_subset(
                RejectionSite::ValueClassDisposeMethod,
                "value classes cannot declare `[Symbol.dispose]()`",
                key_pos,
            );
            return;
        }
        if method.kind != ast::MethodDefinitionKind::Method && self.in_boundary {
            let pos = self.pos(method.span);
            self.reject_subset(
                RejectionSite::MirrorAccessor,
                "mirror classes cannot declare accessors",
                pos,
            );
            return;
        }
        if method.kind == ast::MethodDefinitionKind::Get {
            if !self.claim_class_member_name(
                id,
                &name,
                ClassMemberDeclaration::ReadAccessor,
                method.r#static,
                key_pos.clone(),
            ) {
                return;
            }
            if function_parameter_count(&method.value) != 0 {
                self.reject_subset(
                    RejectionSite::ReadAccessorParameters,
                    "a read accessor must declare no parameters",
                    key_pos.clone(),
                );
                return;
            }
            let Some(return_type) = &method.value.return_type else {
                self.reject_subset(
                    RejectionSite::ReadAccessorReturnMissing,
                    "a read accessor requires an explicit return type",
                    key_pos,
                );
                return;
            };
            let sig = FnSig {
                result_template: None,
                generic: false,
                params: Vec::new(),
                ret: self.resolve_result_type(&return_type.type_annotation),
                is_generator: false,
                is_async: false,
                yield_known: true,
            };
            if method.r#static {
                let symbol = static_member_symbol(id, &self.classes[id.0].name, &name);
                self.class_sigs[id.0]
                    .static_methods
                    .insert(name.clone(), sig.clone());
                self.static_method_owners
                    .insert(symbol.clone(), (id, name.clone()));
                self.fn_sigs.insert(symbol, sig);
            } else {
                self.class_sigs[id.0].methods.insert(name, sig);
            }
            return;
        }
        if method.kind == ast::MethodDefinitionKind::Set {
            let write_name = format!("{name}=");
            if is_value && !method.r#static {
                let class_name = self.classes[id.0].name.clone();
                self.reject_subset(
                    RejectionSite::ValueClassWriteAccessor,
                    format!("value class `{class_name}` cannot declare a write accessor"),
                    key_pos,
                );
                return;
            }
            if !self.claim_class_member_name(
                id,
                &name,
                ClassMemberDeclaration::WriteAccessor,
                method.r#static,
                key_pos.clone(),
            ) {
                return;
            }
            let [parameter] = method.value.params.items.as_slice() else {
                self.reject_subset(
                    RejectionSite::WriteAccessorParameterCount,
                    "a write accessor must declare exactly one parameter",
                    key_pos.clone(),
                );
                return;
            };
            if method.value.params.rest.is_some() || method.value.this_param.is_some() {
                self.reject_subset(
                    RejectionSite::WriteAccessorParameterCount,
                    "a write accessor must declare exactly one parameter",
                    key_pos.clone(),
                );
                return;
            }
            let binding = match &parameter.pattern {
                ast::BindingPattern::BindingIdentifier(binding) => binding,
                _ => {
                    self.reject_subset(
                        RejectionSite::WriteAccessorPattern,
                        "a write accessor parameter must be an identifier",
                        key_pos.clone(),
                    );
                    return;
                }
            };
            let Some(annotation) = &parameter.type_annotation else {
                self.reject_subset(
                    RejectionSite::WriteAccessorTypeMissing,
                    "a write accessor parameter requires a type annotation",
                    key_pos.clone(),
                );
                return;
            };
            let sig = FnSig {
                result_template: None,
                generic: false,
                params: vec![ParamSig {
                    annotated: self.written_type(&annotation.type_annotation),
                    name: binding.name.to_string(),
                    state: crate::check::initializer::TypeState::decided(
                        self.resolve_type(&annotation.type_annotation),
                    ),
                    initializer: None,
                    has_default: false,
                }],
                ret: Type::Void,
                is_generator: false,
                is_async: false,
                yield_known: true,
            };
            write_accessors.push((name.clone(), key_pos, method.r#static));
            if method.r#static {
                let symbol = static_member_symbol(id, &self.classes[id.0].name, &write_name);
                self.class_sigs[id.0]
                    .static_methods
                    .insert(write_name.clone(), sig.clone());
                self.static_method_owners
                    .insert(symbol.clone(), (id, write_name.clone()));
                self.fn_sigs.insert(symbol, sig);
            } else {
                self.class_sigs[id.0].methods.insert(write_name, sig);
            }
            return;
        }
        if !self.claim_class_member_name(
            id,
            &name,
            ClassMemberDeclaration::Method {
                has_body: method.value.body.is_some(),
            },
            method.r#static,
            key_pos.clone(),
        ) {
            return;
        }
        if method.value.generator && !method.r#static {
            let pos = self.pos(method.span);
            if method.value.r#async {
                self.reject_subset(
                    RejectionSite::AsyncGeneratorMethod,
                    "async generator methods are not in the decided surface",
                    pos,
                );
            } else {
                self.reject_subset(
                    RejectionSite::GeneratorMethodDeclaration,
                    "generator methods are not in the decided surface",
                    pos,
                );
            }
            return;
        }
        if method.value.r#async && is_value && !method.r#static {
            let pos = self.pos(method.span);
            self.reject_subset(
                RejectionSite::ValueClassAsyncMethod,
                "async methods on `@ValueType` value classes are not in the decided surface",
                pos,
            );
            return;
        }
        let rejected_default = method
            .value
            .type_parameters
            .as_deref()
            .is_some_and(|params| self.reject_type_parameter_defaults(params));
        // §82.4 rules 1 and 5: a method with type parameters
        // collects as a template. Each call instantiates it.
        if !(is_dispose || self.in_boundary || self.classes[id.0].is_boundary)
            && method.value.type_parameters.is_some()
        {
            let bodiless = method.value.body.is_none();
            if bodiless && declared && !method.value.r#async {
                self.reject_subset(
                    RejectionSite::MethodBodyMissing,
                    "function bodies are required",
                    key_pos.clone(),
                );
            } else if bodiless {
                self.reject_subset(
                    if super::rejection_facts::has_function_implementation(
                        &self.prog.files[self.cur_file].module,
                        &method.value,
                    ) {
                        RejectionSite::GenericMethodBodyMissing
                    } else {
                        RejectionSite::GenericMethodImplementationMissing
                    },
                    "function bodies are required",
                    key_pos.clone(),
                );
            }
            let (type_params, duplicate_type_parameter) = method
                .value
                .type_parameters
                .as_deref()
                .map(|declaration| self.collect_type_parameter_names(declaration))
                .unwrap_or_default();
            let template = GenericMethod {
                file: self.cur_file,
                type_params,
                function: Rc::new(method.value.as_ref().clone_in(self.prog.allocator)),
                rejected: bodiless || duplicate_type_parameter || rejected_default,
            };
            if method.r#static {
                self.class_sigs[id.0]
                    .static_generic_methods
                    .insert(name, template);
            } else {
                self.class_sigs[id.0].generic_methods.insert(name, template);
            }
            return;
        }
        if is_dispose && method.value.r#async {
            self.reject_subset(
                RejectionSite::DisposeAsync,
                "`[Symbol.dispose]()` must be synchronous",
                key_pos,
            );
            return;
        }
        let sig = self.resolve_fn_sig(&method.value, key_pos.clone());
        if is_dispose && (!sig.params.is_empty() || sig.ret != Type::Void) {
            self.reject_subset(
                RejectionSite::DisposeSignature,
                "`[Symbol.dispose]()` takes no parameters and returns `void`",
                key_pos,
            );
            return;
        }
        if method.r#static {
            let symbol = static_member_symbol(id, &self.classes[id.0].name, &name);
            self.class_sigs[id.0]
                .static_methods
                .insert(name.clone(), sig.clone());
            self.static_method_owners
                .insert(symbol.clone(), (id, name.clone()));
            self.fn_sigs.insert(symbol, sig);
        } else {
            self.class_sigs[id.0].methods.insert(name, sig);
        }
    }

    /// Resolves a class's fields and callable signatures (pass B), and
    /// enforces C2 (no inheritance for value classes; field whitelist).
    pub(crate) fn resolve_class_shape(&mut self, id: ClassId, class: &ast::Class, declared: bool) {
        self.class_sigs[id.0].modifiers =
            super::member_modifiers::ClassModifiers::from_class(class);
        if declared {
            self.declared_classes.insert(id);
        }
        let is_value = self.classes[id.0].is_value;
        let is_descriptor = self.classes[id.0].is_descriptor;
        let mut index_signature_pos = None;
        let mut write_accessors = Vec::new();
        if let Some(sup) = &class.heritage {
            let pos = self.pos(sup.expression.span());
            if is_value {
                self.reject_subset(
                    RejectionSite::ValueClassInheritance,
                    "value classes do not inherit",
                    pos,
                );
            } else if is_descriptor {
                self.reject_subset(
                    RejectionSite::DescriptorInheritance,
                    "descriptor classes do not inherit",
                    pos,
                );
            } else {
                self.reject_subset(
                    if matches!(&sup.expression, ast::Expression::Identifier(id) if id.name.as_ref() == "Array") {
                        RejectionSite::ArrayClassInheritance
                    } else if matches!(&sup.expression, ast::Expression::Identifier(id) if id.name.as_ref() == "Set") {
                        RejectionSite::SetClassInheritance
                    } else {
                        RejectionSite::ReferenceClassInheritance
                    },
                    "class inheritance is not in the decided surface",
                    pos,
                );
            }
        }
        // §198.1 rule 5a: the diagnostics of each parameter-property field
        // annotation, by the span that the field shares with its parameter.
        let mut shared_annotations: HashMap<oxc_span::Span, Vec<Diagnostic>> = HashMap::new();
        for member in &class.body.body {
            let private_key = match member {
                ast::ClassElement::PropertyDefinition(property) => Some(&property.key),
                ast::ClassElement::MethodDefinition(method) => Some(&method.key),
                _ => None,
            };
            if let Some(ast::PropertyKey::PrivateIdentifier(key)) = private_key {
                self.reject_subset(
                    RejectionSite::MirrorPrivateName,
                    format!(
                        "mirror classes cannot declare the private name `{}`",
                        crate::parse::private_name::member_name(key)
                    ),
                    self.pos(key.span),
                );
                continue;
            }
            match member {
                ast::ClassElement::PropertyDefinition(prop) => {
                    let ast::PropertyKey::StaticIdentifier(key) = &prop.key else {
                        let pos = self.pos(prop.span);
                        self.reject_subset(
                            if self.computed_field_unbound_name(&prop.key) {
                                RejectionSite::ComputedFieldUnboundName
                            } else {
                                RejectionSite::ComputedFieldDeclaration
                            },
                            "computed or non-identifier field names are not decided",
                            pos,
                        );
                        continue;
                    };
                    if prop.r#type == ast::PropertyDefinitionType::TSAbstractPropertyDefinition {
                        self.reject_subset(
                            RejectionSite::AbstractMember,
                            format!("abstract member `{}` of `{}` is not supported because class inheritance is rejected", key.name, self.classes[id.0].name),
                            self.pos(key.span),
                        );
                    }
                    // §206.1 rule 6: a descriptor member is a public data member.
                    if is_descriptor && crate::parse::private_name::is_private_name(&key.name) {
                        self.reject_subset(
                            RejectionSite::DescriptorPrivateName,
                            format!(
                                "descriptor classes cannot declare the private name `{}`",
                                key.name
                            ),
                            self.pos(key.span),
                        );
                        continue;
                    }
                    if prop.r#static {
                        let pos = self.pos(key.span);
                        if is_descriptor {
                            self.reject_subset(
                                RejectionSite::DescriptorStaticField,
                                "descriptor classes cannot declare static fields",
                                pos,
                            );
                            continue;
                        }
                        if self.in_boundary || self.classes[id.0].is_boundary {
                            self.reject_subset(
                                RejectionSite::MirrorStaticField,
                                "mirror classes cannot declare static fields",
                                pos,
                            );
                            continue;
                        }
                        let name = key.name.to_string();
                        if !self.claim_class_member_name(
                            id,
                            &name,
                            ClassMemberDeclaration::Field,
                            true,
                            self.pos(key.span),
                        ) {
                            continue;
                        }
                        if prop.optional {
                            self.reject_subset(
                                RejectionSite::StaticFieldOptional,
                                "optional static fields imply `undefined`; use `T | null`",
                                self.pos(prop.span),
                            );
                        }
                        let state = match &prop.type_annotation {
                            Some(annotation) => {
                                TypeState::decided(self.resolve_type(&annotation.type_annotation))
                            }
                            None if prop.value.is_some() => TypeState::Undecided,
                            None => {
                                self.reject_subset(RejectionSite::StaticFieldTypeWithoutInitializer,
                                    "static fields without an initializer require a type annotation", self.pos(key.span));
                                TypeState::Rejected
                            }
                        };
                        let ty = state.ty().clone();
                        if self.is_context_affine_type(&ty) {
                            self.reject_subset(
                                RejectionSite::ContextAffineStaticField,
                                "Worker, Inbox, and Outbox values may not be static fields",
                                self.pos(key.span),
                            );
                        }
                        let signature = GlobalSig {
                            annotated: prop.type_annotation.is_some(),
                            named_lambda: false,
                            lambda_defaults: None,
                            function_value_required: None,
                            state,
                            initializer: prop.value.as_ref().map(|e| self.initializer(e, Some(id))),
                            mutable: !prop.readonly,
                        };
                        let symbol = static_member_symbol(id, &self.classes[id.0].name, &name);
                        self.global_sigs.insert(symbol, signature.clone());
                        self.class_sigs[id.0].static_fields.insert(name, signature);
                        continue;
                    }
                    let name = key.name.to_string();
                    if !self.claim_class_member_name(
                        id,
                        &name,
                        ClassMemberDeclaration::Field,
                        false,
                        self.pos(key.span),
                    ) {
                        continue;
                    }
                    let parameter_property =
                        crate::parse::parameter_property::declares_field(class, prop);
                    let is_defaulted =
                        is_descriptor && prop.optional && prop.value.is_some() && !prop.definite;
                    if is_descriptor {
                        match (prop.definite, prop.optional, prop.value.is_some()) {
                            (true, false, false) | (false, true, true) | (false, true, false) => {}
                            (_, true, false) => {
                                let pos = self.pos(prop.span);
                                self.reject_subset(
                                    RejectionSite::DescriptorOptionalDefaultMissing,
                                    "optional descriptor members require a default initializer",
                                    pos,
                                );
                            }
                            (false, false, true) => {
                                let pos = self.pos(prop.span);
                                self.reject_subset(RejectionSite::DescriptorInitializerWithoutOptional, "a descriptor member initializer requires the optional `?` spelling", pos);
                            }
                            _ => {
                                let pos = self.pos(prop.span);
                                self.reject_subset(
                                    if !declared && self.descriptor_field_unassigned(class, prop) {
                                        RejectionSite::DescriptorRequiredFieldUnassigned
                                    } else {
                                        RejectionSite::DescriptorRequiredWithoutDefinite
                                    },
                                    "required descriptor members must be spelled `name!: T`",
                                    pos,
                                );
                            }
                        }
                    } else if prop.optional {
                        let pos = self.pos(prop.span);
                        self.reject_subset(
                            RejectionSite::InstanceFieldOptional,
                            "optional properties imply `undefined`; use `T | null`",
                            pos,
                        );
                    }
                    let pos = self.pos(key.span);
                    // §198.1 rule 6: a literal default gives the field its type.
                    let literal_default_type =
                        if parameter_property && prop.type_annotation.is_none() {
                            crate::parse::parameter_property::parameter_default(class, prop)
                                .and_then(super::expr::literal::literal_default_type)
                        } else {
                            None
                        };
                    let ty = match &prop.type_annotation {
                        Some(ann) => {
                            let allow_wire =
                                self.in_boundary && self.boundary_classes.contains(&id);
                            self.allow_wire_alias_boundary = allow_wire;
                            let diagnostics_before = self.diags.len();
                            let ty = self.resolve_type(&ann.type_annotation);
                            if parameter_property {
                                shared_annotations
                                    .insert(prop.span, self.diags.since(diagnostics_before));
                            }
                            self.allow_wire_alias_boundary = false;
                            ty
                        }
                        None if parameter_property => match literal_default_type.clone() {
                            Some(ty) => ty,
                            None => {
                                self.reject_subset(
                                    RejectionSite::ParameterPropertyTypeAnnotation,
                                    "a parameter property requires a type annotation",
                                    pos.clone(),
                                );
                                Type::Error
                            }
                        },
                        None => {
                            if prop.value.is_none() {
                                let site = if !class.body.body.iter().any(|member| {
                                    matches!(member, ast::ClassElement::MethodDefinition(_))
                                }) {
                                    RejectionSite::UnassignedFieldTypeWithoutInitializer
                                } else {
                                    RejectionSite::FieldTypeWithoutInitializer
                                };
                                self.reject_subset(
                                    site,
                                    "fields without an initializer require a type annotation",
                                    pos.clone(),
                                );
                            }
                            Type::Error
                        }
                    };
                    if self.in_boundary
                        && Self::contains_string_alias(&ty)
                        && !Self::supported_wire_alias_boundary_type(&ty)
                    {
                        self.reject_subset(RejectionSite::WireAliasNestedField, format!(
                                "wire-mapped aliases are supported only as direct boundary-struct members or array-pair elements; member `{}` nests one inside another boundary type",
                                key.name
                            ), pos.clone());
                    }
                    let is_absence_capable = is_descriptor
                        && !prop.definite
                        && prop.optional
                        && prop.value.is_none()
                        && matches!(&self.apparent_type(&ty), Type::StringAlias(_));
                    if is_descriptor
                        && !prop.definite
                        && prop.optional
                        && prop.value.is_none()
                        && !matches!(&self.apparent_type(&ty), Type::StringAlias(_) | Type::Error)
                    {
                        self.reject_subset(
                            RejectionSite::DescriptorOptionalInitializerMissing,
                            "optional descriptor members require a default initializer",
                            self.pos(prop.span),
                        );
                    }
                    let context_affine = self.is_context_affine_type(&ty);
                    if context_affine {
                        self.reject_subset(
                            RejectionSite::ContextAffineInstanceField,
                            "Worker, Inbox, and Outbox values may not be class fields",
                            pos.clone(),
                        );
                    }
                    let foreign_provenance =
                        if self.in_boundary && matches!(&self.apparent_type(&ty), Type::Func(_)) {
                            self.callback_provenance(
                                self.cur_file,
                                prop.type_annotation
                                    .as_deref()
                                    .map(|annotation| &annotation.type_annotation),
                                pos.clone(),
                            )
                        } else if self.in_boundary {
                            self.member_const_provenance(&self.classes[id.0].name, &key.name, &ty)
                        } else {
                            None
                        };
                    // Boundary structs (mirror-ingested) relax the C2
                    // value-field whitelist: they may carry `X | null`,
                    // `object | null`, and function-pointer fields.
                    if is_value
                        && !self.boundary_classes.contains(&id)
                        && !context_affine
                        && ty != Type::Error
                        && !self.value_field_ok(&ty)
                    {
                        self.reject_subset(
                            RejectionSite::ValueFieldOutsideWhitelist,
                            format!(
                                "field type `{}` is outside the value-class whitelist \
                                 (sized numerics, boolean, value classes, FixedArray, enums)",
                                self.type_name(&ty)
                            ),
                            pos.clone(),
                        );
                    }
                    if prop.type_annotation.is_none() && literal_default_type.is_none() {
                        let initializer = prop
                            .value
                            .as_ref()
                            .map(|e| self.field_source(e, id, class, prop));
                        self.class_sigs[id.0].fields.insert(
                            name.clone(),
                            GlobalSig {
                                annotated: false,
                                named_lambda: false,
                                lambda_defaults: None,
                                function_value_required: None,
                                state: if prop.value.is_some() {
                                    TypeState::Undecided
                                } else {
                                    TypeState::Rejected
                                },
                                initializer,
                                mutable: !prop.readonly,
                            },
                        );
                    }
                    let written_non_null = prop
                        .type_annotation
                        .as_ref()
                        .is_some_and(|ann| self.written_type(&ann.type_annotation))
                        && !matches!(
                            self.apparent_type(&ty),
                            Type::Null | Type::Nullable(_, _) | Type::Error
                        );
                    self.classes[id.0].fields.push(hir::Field {
                        written_non_null,
                        name: key.name.to_string(),
                        ty,
                        is_defaulted,
                        is_absence_capable,
                        init: None,
                        foreign_provenance,
                        pos,
                    });
                }
                ast::ClassElement::MethodDefinition(ctor)
                    if ctor.kind == ast::MethodDefinitionKind::Constructor =>
                {
                    if is_descriptor {
                        self.reject_subset(
                            RejectionSite::DescriptorConstructor,
                            "descriptor classes cannot declare constructors",
                            self.pos(ctor.span),
                        );
                        continue;
                    }
                    let mut params = Vec::new();
                    for param in &ctor.value.params.items {
                        {
                            self.allow_wire_alias_boundary = self.in_boundary;
                            let saved = self.task_group_parameters;
                            self.task_group_parameters = !self.in_boundary;
                            let diagnostics_before = self.diags.len();
                            let instances_before = self.instance_diagnostic_ranges.len();
                            let resolved = self.resolve_param_pat(param);
                            // §198.1 rule 5a: a diagnostic that the field already
                            // gave for the shared annotation reports once. A new
                            // instance range indexes these diagnostics, so none
                            // is dropped then.
                            if let Some(earlier) = shared_annotations.get(&param.span) {
                                if self.instance_diagnostic_ranges.len() == instances_before {
                                    self.diags.drop_repeats(diagnostics_before, earlier);
                                }
                            }
                            self.task_group_parameters = saved;
                            self.allow_wire_alias_boundary = false;
                            if self.in_boundary
                                && Self::contains_string_alias(resolved.ty())
                                && !Self::supported_wire_alias_boundary_type(resolved.ty())
                            {
                                self.reject_subset(RejectionSite::WireAliasNestedConstructorParameter, format!(
                                            "wire-mapped aliases are supported only as direct mirror-constructor parameters or array-pair elements; parameter `{}` nests one inside another boundary type",
                                            resolved.name
                                        ), self.pos(param.span));
                            }
                            params.push(resolved);
                        }
                    }
                    if let Some(rest) = &ctor.value.params.rest {
                        self.reject_subset(
                            RejectionSite::RestParameter,
                            "parameter pattern outside the decided surface",
                            self.pos(rest.span),
                        );
                        params.push(ParamSig {
                            annotated: false,
                            name: String::new(),
                            state: TypeState::Rejected,
                            initializer: None,
                            has_default: false,
                        });
                    }
                    self.class_sigs[id.0].ctor = Some(params);
                }
                ast::ClassElement::MethodDefinition(method) => {
                    let diagnostics_before = self.diags.len();
                    self.resolve_class_method(id, method, declared, &mut write_accessors);
                    // §93 rule 12: every collection rejection preserves a
                    // template, including exits before normal collection.
                    if self.diags.len() != diagnostics_before {
                        if let Some(template) = GenericMethod::rejected(
                            self.cur_file,
                            &method.value,
                            self.prog.allocator,
                        ) {
                            if let Some(name) = Self::class_method_name(&method.key) {
                                let templates = if method.r#static {
                                    &mut self.class_sigs[id.0].static_generic_methods
                                } else {
                                    &mut self.class_sigs[id.0].generic_methods
                                };
                                templates.insert(name, template);
                            }
                        }
                    }
                }
                ast::ClassElement::TSIndexSignature(signature) if !self.in_boundary => {
                    let pos = self.pos(signature.span);
                    if index_signature_pos.is_some() {
                        self.reject_subset(
                            if self.classes[id.0].index_signature.as_ref().is_some_and(|first| matches!(self.apparent_type(&first.index_ty), Type::I32 | Type::U32))
                                && matches!(&signature.parameter.type_annotation.type_annotation, ast::TSType::TSTypeReference(reference)
                                        if matches!(&reference.type_name, ast::TSTypeName::IdentifierReference(name) if matches!(name.name.as_ref(), "i32" | "u32"))) {
                            RejectionSite::DuplicateNumericIndexSignature
                        } else { RejectionSite::ClassIndexSignatureCount },
                            "a class can declare at most one index signature",
                            pos,
                        );
                        continue;
                    }
                    index_signature_pos = Some(pos.clone());
                    if is_value || is_descriptor {
                        self.reject_subset(
                            RejectionSite::ClassIndexSignatureNonReference,
                            "only reference classes can declare an index signature",
                            pos.clone(),
                        );
                    }
                    if signature.r#static {
                        self.reject_subset(
                            RejectionSite::ClassIndexSignatureStatic,
                            "a class index signature cannot be static",
                            pos.clone(),
                        );
                    }
                    let index_ty =
                        self.resolve_type(&signature.parameter.type_annotation.type_annotation);
                    if !matches!(
                        self.apparent_type(&index_ty),
                        Type::I32 | Type::U32 | Type::Error
                    ) {
                        let actual = self.type_name(&index_ty);
                        self.reject_subset(RejectionSite::ClassIndexSignatureIndexType, format!(
                                "a class index signature requires an `i32` or `u32` index, got `{actual}`"
                            ), pos.clone());
                    }
                    let element_ty = self.resolve_type(&signature.type_annotation.type_annotation);
                    self.classes[id.0].index_signature = Some(hir::IndexSignature {
                        index_ty,
                        element_ty,
                        readonly: signature.readonly,
                    });
                }

                // §206.1 rule 6: the front end lowers the private names of a
                // program file only, so a mirror class keeps each one.
                other => {
                    let pos = self.pos(other.span());
                    self.reject_subset(
                        match other {
                            ast::ClassElement::StaticBlock(_) => {
                                RejectionSite::StaticBlockDeclaration
                            }
                            ast::ClassElement::AccessorProperty(_) => {
                                RejectionSite::AutoAccessorDeclaration
                            }
                            _ => RejectionSite::UnsupportedClassMemberKind,
                        },
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
                self.reject_subset(
                    RejectionSite::WriteAccessorWithoutRead,
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
                .map(|parameter| parameter.ty().clone());
            if let (Some(read_type), Some(write_type)) = (read_type, write_type) {
                if read_type != write_type {
                    self.reject_subset(
                        RejectionSite::AccessorTypeMismatch,
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
                    && *method.params[0].ty() == signature.index_ty
                    && method.ret == signature.element_ty
            });
        if !get_matches {
            let index = self.type_name(&signature.index_ty);
            let element = self.type_name(&signature.element_ty);
            self.reject_subset(RejectionSite::IndexSignatureGetterMismatch, format!(
                    "the index signature requires `get(index: {index}): {element}` with exactly matching types"
                ), pos.clone());
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
                    && *method.params[0].ty() == signature.index_ty
                    && *method.params[1].ty() == signature.element_ty
                    && method.ret == Type::Void
            });
        if !set_matches {
            let index = self.type_name(&signature.index_ty);
            let element = self.type_name(&signature.element_ty);
            self.reject_subset(RejectionSite::ClassIndexSetSignature, format!(
                    "the index signature requires `set(index: {index}, value: {element}): void` with exactly matching types"
                ), pos);
        }
    }

    pub(crate) fn plain_value_leaf(&self, ty: &Type) -> bool {
        matches!(
            &self.apparent_type(ty),
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

    pub(super) fn check_inferred_field_type(&mut self, id: ClassId, ty: &Type, pos: Pos) {
        if self.is_context_affine_type(ty) {
            self.reject_subset(
                RejectionSite::ContextAffineInstanceField,
                "Worker, Inbox, and Outbox values may not be class fields",
                pos,
            );
        } else if self.classes[id.0].is_value
            && !self.classes[id.0].is_boundary
            && *ty != Type::Error
            && !self.value_field_ok(ty)
        {
            self.reject_subset(RejectionSite::ValueFieldOutsideWhitelist,
                format!("field type `{}` is outside the value-class whitelist (sized numerics, boolean, value classes, FixedArray, enums)", self.type_name(ty)), pos);
        }
    }

    pub(super) fn value_field_ok(&self, ty: &Type) -> bool {
        if self.instance_restriction(super::opaque::InstanceRestriction::ValueField, ty)
            || self.plain_value_leaf(ty)
        {
            return true;
        }
        match &self.apparent_type(ty) {
            Type::Class(id) => self.classes[id.0].is_value,
            Type::FixedArray(elem, _) => self.value_field_ok(elem),
            _ => false,
        }
    }
}
