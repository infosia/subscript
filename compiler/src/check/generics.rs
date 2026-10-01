use super::*;

impl<'p> Checker<'p> {
    // ----- generic monomorphization (in HIR: templates never survive) -----

    /// Assigns an instance symbol from its template identity and nominal argument types.
    pub(crate) fn mono_name(&mut self, base: &str, args: &[Type]) -> String {
        let key = format!("{base:?}:{args:?}");
        if let Some(symbol) = self.instance_symbols.get(&key) {
            return symbol.clone();
        }
        let rendered: Vec<String> = args.iter().map(|ty| self.type_name(ty)).collect();
        let name = format!("{}<{}>", source_name(base), rendered.join(", "));
        let symbol = identity::instance_symbol(&key, &name);
        self.instance_symbols.insert(key, symbol.clone());
        symbol
    }

    /// Checks roots and concrete instances; nested opaque instances resolve signatures only (§140 rule 4a).
    fn instance_body_is_checked(&self, args: &[Type], root: bool) -> bool {
        root || !args
            .iter()
            .any(|argument| self.involves_type_parameter(argument))
    }

    /// Applies the type-parameter constraints of a template at `args`
    /// (compiler.md §135.1 rule 2b). The caller has bound each parameter.
    ///
    /// The root instance of an opaque check records that its opaque type
    /// parameter type has a constraint (§135.1 rule 2). Every other instance checks each
    /// argument against its constraint and reports an argument that does
    /// not satisfy it at that type argument (`positions`, or `pos` when
    /// the argument has no position). Returns false when an argument does
    /// not satisfy its constraint.
    fn apply_type_parameter_constraints(
        &mut self,
        declaration: Option<&ast::TsTypeParamDecl>,
        args: &[Type],
        root: bool,
        positions: &[Pos],
        pos: &Pos,
    ) -> bool {
        let Some(declaration) = declaration else {
            return true;
        };
        let mut satisfied = true;
        for (index, (parameter, argument)) in declaration.params.iter().zip(args).enumerate() {
            let Some(constraint) = &parameter.constraint else {
                continue;
            };
            let constraint = self.resolve_type(constraint);
            if root && self.is_type_parameter(argument) {
                self.constrain_opaque_param(argument, constraint);
                continue;
            }
            if self.satisfies_constraint(argument, &constraint) {
                continue;
            }
            satisfied = false;
            let argument_name = self.type_name(argument);
            let constraint_name = self.type_name(&constraint);
            self.error(
                RuleCode::S100,
                format!(
                    "type argument `{argument_name}` does not satisfy the constraint `{constraint_name}` of `{}`",
                    parameter.name.sym
                ),
                positions.get(index).cloned().unwrap_or_else(|| pos.clone()),
            );
        }
        if root {
            for (parameter, argument) in declaration.params.iter().zip(args) {
                if self.constraint_cycle(argument) {
                    satisfied = false;
                    self.error(
                        RuleCode::S100,
                        format!(
                            "type parameter `{}` has a circular constraint",
                            parameter.name.sym
                        ),
                        self.pos(parameter.span),
                    );
                }
            }
        }
        satisfied
    }

    /// True when the type argument `argument` satisfies the constraint
    /// `constraint` (compiler.md §135.1 rule 2b): the checker's
    /// assignability after [`Self::numeric_normal_form`] of both types.
    fn satisfies_constraint(&self, argument: &Type, constraint: &Type) -> bool {
        self.assignable(
            &self.numeric_normal_form(argument),
            &self.numeric_normal_form(constraint),
        )
    }

    /// `ty` with every numeric type (the sized integers, `f16`, `f32`,
    /// `f64`, `number`) replaced with the one type `number`, at any depth
    /// (compiler.md §135.1 rule 2b).
    ///
    /// A generic class instance becomes the first instance of its template
    /// whose type arguments have the same normal form, so two instances
    /// compare by their type arguments and the check makes no instance.
    fn numeric_normal_form(&self, ty: &Type) -> Type {
        let normal = |inner: &Type| Box::new(self.numeric_normal_form(inner));
        match ty {
            numeric if numeric.is_numeric() => Type::F64,
            Type::Class(id) => Type::Class(self.normal_instance(*id)),
            Type::FixedArray(element, len) => Type::FixedArray(normal(element), *len),
            Type::Array(element) => Type::Array(normal(element)),
            Type::Map(key, value) => Type::Map(normal(key), normal(value)),
            Type::Set(element) => Type::Set(normal(element)),
            Type::Worker(input, output) => Type::Worker(normal(input), normal(output)),
            Type::Inbox(message) => Type::Inbox(normal(message)),
            Type::Outbox(message) => Type::Outbox(normal(message)),
            Type::Func(signature) => Type::Func(Box::new(crate::types::FuncType {
                params: signature
                    .params
                    .iter()
                    .map(|param| self.numeric_normal_form(param))
                    .collect(),
                ret: self.numeric_normal_form(&signature.ret),
            })),
            Type::Nullable(inner) => Type::Nullable(normal(inner)),
            Type::Generator(item) => Type::Generator(normal(item)),
            Type::AsyncHandle(result) => Type::AsyncHandle(normal(result)),
            Type::IterResult(item) => Type::IterResult(normal(item)),
            other => other.clone(),
        }
    }

    /// The first instance of the template of `id` whose type arguments
    /// have the normal form of the type arguments of `id` (compiler.md
    /// §135.1 rule 2b). A class that is not a generic instance is itself.
    fn normal_instance(&self, id: ClassId) -> ClassId {
        let Some((key, args)) = self.instance_arguments.get(&id) else {
            return id;
        };
        self.instance_arguments
            .iter()
            .filter(|(_, (other_key, other_args))| {
                other_key == key
                    && other_args.len() == args.len()
                    && other_args
                        .iter()
                        .zip(args)
                        .all(|(other, argument)| self.same_normal_form(other, argument))
            })
            .map(|(other, _)| *other)
            .min_by_key(|other| other.0)
            .unwrap_or(id)
    }

    /// True when `a` and `b` have the same [`Self::numeric_normal_form`].
    ///
    /// The comparison walks both types together, so it ends at the depth
    /// of the two types: it does not search the instances of a nested
    /// template.
    fn same_normal_form(&self, a: &Type, b: &Type) -> bool {
        if a.is_numeric() && b.is_numeric() {
            return true;
        }
        match (a, b) {
            (Type::Class(x), Type::Class(y)) => {
                x == y
                    || match (
                        self.instance_arguments.get(x),
                        self.instance_arguments.get(y),
                    ) {
                        (Some((x_key, x_args)), Some((y_key, y_args))) => {
                            x_key == y_key
                                && x_args.len() == y_args.len()
                                && x_args
                                    .iter()
                                    .zip(y_args)
                                    .all(|(x, y)| self.same_normal_form(x, y))
                        }
                        _ => false,
                    }
            }
            (Type::FixedArray(_, x_len), Type::FixedArray(_, y_len)) if x_len != y_len => false,
            _ => {
                let (a_inner, b_inner) = (a.contained_types(), b.contained_types());
                if std::mem::discriminant(a) != std::mem::discriminant(b)
                    || a_inner.len() != b_inner.len()
                {
                    return false;
                }
                if a_inner.is_empty() {
                    return a == b;
                }
                a_inner
                    .into_iter()
                    .zip(b_inner)
                    .all(|(x, y)| self.same_normal_form(x, y))
            }
        }
    }

    /// Checks the constraints of a template at `args` for a type argument
    /// list whose instance already exists (compiler.md §135.1 rule 2b:
    /// each site is checked).
    fn check_constraints_at_site(
        &mut self,
        file: usize,
        type_params: &[String],
        declaration: Option<&ast::TsTypeParamDecl>,
        args: &[Type],
        positions: &[Pos],
        pos: &Pos,
    ) {
        if !declaration.is_some_and(|declaration| {
            declaration
                .params
                .iter()
                .any(|parameter| parameter.constraint.is_some())
        }) {
            return;
        }
        let saved_file = self.cur_file;
        let saved_subst = std::mem::take(&mut self.subst);
        let saved_context = self.suspend_container_context();
        self.cur_file = file;
        for (param, arg) in type_params.iter().zip(args) {
            self.subst.insert(param.clone(), arg.clone());
        }
        let diagnostics = self.diags.len();
        self.apply_type_parameter_constraints(declaration, args, false, positions, pos);
        self.instance_diagnostic_ranges
            .push(diagnostics..self.diags.len());
        self.cur_file = saved_file;
        self.subst = saved_subst;
        self.leave_container_context(saved_context);
    }

    /// Instantiates a generic function at explicit type arguments and
    /// checks its body immediately. Returns the instance name.
    ///
    /// `positions` holds the source position of each type argument.
    pub(crate) fn instantiate_fn(
        &mut self,
        key: &str,
        arguments: &InstanceArguments,
        pos: Pos,
    ) -> Option<String> {
        let args = &arguments.types;
        let positions = &arguments.positions;
        let template = self.generic_fns.get(key)?.clone();
        if template.rejected {
            return None;
        }
        if template.type_params.len() != args.len() {
            self.error(
                RuleCode::S100,
                format!(
                    "`{}` expects {} type argument(s), got {}",
                    source_name(key),
                    template.type_params.len(),
                    args.len()
                ),
                pos,
            );
            return None;
        }
        if !self.enter_instance(key, &template.type_params, arguments, &pos) {
            return None;
        }
        let name = self.mono_name(key, args);
        if self.fn_sigs.contains_key(&name) {
            self.check_constraints_at_site(
                template.file,
                &template.type_params,
                template.function.type_params.as_deref(),
                args,
                positions,
                &pos,
            );
            self.instance_chain.pop();
            return Some(name);
        }
        let saved_file = self.cur_file;
        let saved_subst = std::mem::take(&mut self.subst);
        let saved_context = self.suspend_container_context();
        self.cur_file = template.file;
        for (param, arg) in template.type_params.iter().zip(args) {
            self.subst.insert(param.clone(), arg.clone());
        }
        let root = std::mem::take(&mut self.opaque_root);
        let diagnostics = self.diags.len();
        let satisfied = self.apply_type_parameter_constraints(
            template.function.type_params.as_deref(),
            args,
            root,
            positions,
            &pos,
        );
        let check_body = satisfied && self.instance_body_is_checked(args, root);
        let sig = self.resolve_fn_sig(&template.function, pos.clone());
        self.fn_sigs.insert(name.clone(), sig.clone());
        let function = check_body
            .then(|| self.check_function(&template.function, &name, false, &sig, (None, None), pos))
            .flatten();
        if let Some(function) = function {
            self.functions.push(function);
        }
        self.instance_diagnostic_ranges
            .push(diagnostics..self.diags.len());
        self.cur_file = saved_file;
        self.subst = saved_subst;
        self.leave_container_context(saved_context);
        self.instance_chain.pop();
        Some(name)
    }

    /// Instantiates a generic method at explicit type arguments and
    /// checks its body immediately (§82.4 rule 3). Returns the instance
    /// name, which is the monomorphized name `m<A>`.
    ///
    /// The instance is an ordinary method of the class in the namespace
    /// that `is_static` selects. Every consumer of a method name sees the
    /// instance name; no template reaches the HIR. `positions` holds the
    /// source position of each type argument.
    pub(crate) fn instantiate_method(
        &mut self,
        id: ClassId,
        name: &str,
        arguments: &InstanceArguments,
        is_static: bool,
        pos: Pos,
    ) -> Option<String> {
        let args = &arguments.types;
        let positions = &arguments.positions;
        let template = if is_static {
            self.class_sigs[id.0]
                .static_generic_methods
                .get(name)?
                .clone()
        } else {
            self.class_sigs[id.0].generic_methods.get(name)?.clone()
        };
        if template.rejected {
            return None;
        }
        if template.type_params.len() != args.len() {
            self.error(
                RuleCode::S100,
                format!(
                    "`{}` expects {} type argument(s), got {}",
                    name,
                    template.type_params.len(),
                    args.len()
                ),
                pos,
            );
            return None;
        }
        let base = format!("[[identity:method:{}]]{name}", id.0);
        let chain_key = format!("[[identity:method:{}:{is_static}]]{name}", id.0);
        if !self.enter_instance(&chain_key, &template.type_params, arguments, &pos) {
            return None;
        }
        let instance = self.mono_name(&base, args);
        let known = if is_static {
            self.class_sigs[id.0].static_methods.contains_key(&instance)
        } else {
            self.class_sigs[id.0].methods.contains_key(&instance)
        };
        if known {
            self.check_constraints_at_site(
                template.file,
                &template.type_params,
                template.function.type_params.as_deref(),
                args,
                positions,
                &pos,
            );
            self.instance_chain.pop();
            return Some(instance);
        }
        let saved_file = self.cur_file;
        let saved_subst = std::mem::take(&mut self.subst);
        let saved_context = self.suspend_container_context();
        self.cur_file = template.file;
        for (param, arg) in template.type_params.iter().zip(args) {
            self.subst.insert(param.clone(), arg.clone());
        }
        let root = std::mem::take(&mut self.opaque_root);
        let diagnostics = self.diags.len();
        let satisfied = self.apply_type_parameter_constraints(
            template.function.type_params.as_deref(),
            args,
            root,
            positions,
            &pos,
        );
        let check_body = satisfied && self.instance_body_is_checked(args, root);
        let sig = self.resolve_fn_sig(&template.function, pos.clone());
        // The signature lands before the body check, so a recursive call
        // inside the body resolves against this instance.
        let function_name = if is_static {
            let symbol = static_member_symbol(id, &self.classes[id.0].name, &instance);
            self.class_sigs[id.0]
                .static_methods
                .insert(instance.clone(), sig.clone());
            self.fn_sigs.insert(symbol.clone(), sig.clone());
            symbol
        } else {
            self.class_sigs[id.0]
                .methods
                .insert(instance.clone(), sig.clone());
            instance.clone()
        };
        let function = check_body
            .then(|| {
                self.check_function(
                    &template.function,
                    &function_name,
                    false,
                    &sig,
                    (
                        (!is_static).then_some(Type::Class(id)),
                        is_static.then_some(Divergence::StaticMemberSurface),
                    ),
                    pos,
                )
            })
            .flatten();
        if let Some(function) = function {
            if is_static {
                self.functions.push(function);
            } else {
                self.classes[id.0].methods.push(function);
            }
        }
        self.instance_diagnostic_ranges
            .push(diagnostics..self.diags.len());
        self.cur_file = saved_file;
        self.subst = saved_subst;
        self.leave_container_context(saved_context);
        self.instance_chain.pop();
        Some(instance)
    }

    /// Instantiates a generic class at explicit type arguments, checking
    /// its shape and bodies immediately. Returns the instance id.
    /// `positions` holds the source position of each type argument.
    ///
    /// An argument that is the error type gives no instance: the instance
    /// type is the error type (compiler.md §132 rule 2a). The type-argument
    /// count does not depend on the argument types, so a wrong count
    /// reports first.
    pub(crate) fn instantiate_class(
        &mut self,
        key: &str,
        arguments: &InstanceArguments,
        pos: Pos,
    ) -> Option<ClassId> {
        let args = &arguments.types;
        let positions = &arguments.positions;
        let template = self.generic_classes.get(key)?.clone();
        if template.type_params.len() != args.len() {
            self.error(
                RuleCode::S100,
                format!(
                    "`{}` expects {} type argument(s), got {}",
                    source_name(key),
                    template.type_params.len(),
                    args.len()
                ),
                pos,
            );
            return None;
        }
        if args.contains(&Type::Error) {
            return None;
        }
        if template.has_static_member || !template.rejected_generic_methods.is_empty() {
            return None;
        }
        if !self.enter_instance(key, &template.type_params, arguments, &pos) {
            return None;
        }
        let name = self.mono_name(key, args);
        if let Some(&id) = self.class_ids.get(&name) {
            self.check_constraints_at_site(
                template.file,
                &template.type_params,
                template.class.type_params.as_deref(),
                args,
                positions,
                &pos,
            );
            self.instance_chain.pop();
            return Some(id);
        }
        let saved_file = self.cur_file;
        let saved_subst = std::mem::take(&mut self.subst);
        let saved_context = self.suspend_container_context();
        self.cur_file = template.file;
        for (param, arg) in template.type_params.iter().zip(args) {
            self.subst.insert(param.clone(), arg.clone());
        }
        let root = std::mem::take(&mut self.opaque_root);
        let diagnostics = self.diags.len();
        let satisfied = self.apply_type_parameter_constraints(
            template.class.type_params.as_deref(),
            args,
            root,
            positions,
            &pos,
        );
        let check_body = satisfied && self.instance_body_is_checked(args, root);
        let id = self.new_class(
            &name,
            template.is_value,
            template.is_descriptor,
            template.alignment_override,
            template.pos.clone(),
        );
        self.instance_arguments
            .insert(id, (key.to_string(), args.to_vec()));
        if args
            .iter()
            .any(|argument| self.involves_type_parameter(argument))
        {
            self.opaque_instances.insert(id);
        }
        self.resolve_class_shape(id, &template.class, template.declared);
        if check_body && !self.signatures_resolved {
            self.pending_instance_bodies
                .push((id, self.instance_chain.clone()));
        } else if check_body && template.is_descriptor {
            self.check_descriptor_defaults(id, &template.class);
        } else if check_body {
            self.check_class_body(id, &template.class, template.declared);
        }
        self.instance_diagnostic_ranges
            .push(diagnostics..self.diags.len());
        self.cur_file = saved_file;
        self.subst = saved_subst;
        self.leave_container_context(saved_context);
        self.instance_chain.pop();
        Some(id)
    }

    /// Checks deferred instance bodies with all module signatures (§138 rule 1).
    pub(crate) fn check_pending_instance_bodies(&mut self) {
        for (id, chain) in std::mem::take(&mut self.pending_instance_bodies) {
            let Some((key, args)) = self.instance_arguments.get(&id).cloned() else {
                self.error(
                    RuleCode::S100,
                    "internal error: deferred instance has no type arguments",
                    Pos::new("", 1, 1),
                );
                continue;
            };
            let Some(template) = self.generic_classes.get(&key).cloned() else {
                self.error(
                    RuleCode::S100,
                    "internal error: deferred instance has no generic template",
                    Pos::new("", 1, 1),
                );
                continue;
            };
            let saved_file = self.cur_file;
            let saved_subst = std::mem::take(&mut self.subst);
            let saved_context = self.suspend_container_context();
            self.cur_file = template.file;
            self.subst = template.type_params.iter().cloned().zip(args).collect();
            let saved_chain = std::mem::replace(&mut self.instance_chain, chain);
            let diagnostics = self.diags.len();
            if template.is_descriptor {
                self.check_descriptor_defaults(id, &template.class);
            } else {
                self.check_class_body(id, &template.class, template.declared);
            }
            self.instance_diagnostic_ranges
                .push(diagnostics..self.diags.len());
            self.instance_chain = saved_chain;
            self.cur_file = saved_file;
            self.subst = saved_subst;
            self.leave_container_context(saved_context);
        }
    }

    /// The constructor parameter counts `(total, required)` of the generic
    /// class template `key`, read from its declaration.
    ///
    /// The counts do not depend on the type arguments, so a construction
    /// with an error type argument, which gives no instance (§132 rule
    /// 2a), still reports a wrong argument count. `None` when the template
    /// gives no constructed instance for another reason: an unknown key, a
    /// descriptor or ambient template, or a template that
    /// [`Checker::instantiate_class`] rejects.
    pub(crate) fn template_constructor_arity(&self, key: &str) -> Option<(usize, usize)> {
        let template = self.generic_classes.get(key)?;
        if template.is_descriptor
            || template.declared
            || template.has_static_member
            || !template.rejected_generic_methods.is_empty()
        {
            return None;
        }
        let mut total = 0;
        let mut required = 0;
        for member in &template.class.body {
            let ast::ClassMember::Constructor(ctor) = member else {
                continue;
            };
            // The shape keeps the last constructor.
            total = 0;
            required = 0;
            for parameter in &ctor.params {
                if let ast::ParamOrTsParamProp::Param(parameter) = parameter {
                    total += 1;
                    if !matches!(parameter.pat, ast::Pat::Assign(_)) {
                        required += 1;
                    }
                }
            }
        }
        Some((total, required))
    }
}
