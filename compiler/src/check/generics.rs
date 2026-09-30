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

    /// Instantiates a generic function at explicit type arguments and
    /// checks its body immediately. Returns the instance name.
    pub(crate) fn instantiate_fn(&mut self, key: &str, args: &[Type], pos: Pos) -> Option<String> {
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
        let name = self.mono_name(key, args);
        if self.fn_sigs.contains_key(&name) {
            return Some(name);
        }
        let saved_file = self.cur_file;
        let saved_subst = std::mem::take(&mut self.subst);
        let saved_context = self.suspend_container_context();
        self.cur_file = template.file;
        for (param, arg) in template.type_params.iter().zip(args) {
            self.subst.insert(param.clone(), arg.clone());
        }
        let sig = self.resolve_fn_sig(&template.function, pos.clone());
        self.fn_sigs.insert(name.clone(), sig.clone());
        if let Some(function) =
            self.check_function(&template.function, &name, false, &sig, (None, None), pos)
        {
            self.functions.push(function);
        }
        self.cur_file = saved_file;
        self.subst = saved_subst;
        self.leave_container_context(saved_context);
        Some(name)
    }

    /// Instantiates a generic method at explicit type arguments and
    /// checks its body immediately (§82.4 rule 3). Returns the instance
    /// name, which is the monomorphized name `m<A>`.
    ///
    /// The instance is an ordinary method of the class in the namespace
    /// that `is_static` selects. Every consumer of a method name sees the
    /// instance name; no template reaches the HIR.
    pub(crate) fn instantiate_method(
        &mut self,
        id: ClassId,
        name: &str,
        args: &[Type],
        is_static: bool,
        pos: Pos,
    ) -> Option<String> {
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
        let instance = self.mono_name(&base, args);
        let known = if is_static {
            self.class_sigs[id.0].static_methods.contains_key(&instance)
        } else {
            self.class_sigs[id.0].methods.contains_key(&instance)
        };
        if known {
            return Some(instance);
        }
        let saved_file = self.cur_file;
        let saved_subst = std::mem::take(&mut self.subst);
        let saved_context = self.suspend_container_context();
        self.cur_file = template.file;
        for (param, arg) in template.type_params.iter().zip(args) {
            self.subst.insert(param.clone(), arg.clone());
        }
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
        if let Some(function) = self.check_function(
            &template.function,
            &function_name,
            false,
            &sig,
            (
                (!is_static).then_some(Type::Class(id)),
                is_static.then_some(Divergence::StaticMemberSurface),
            ),
            pos,
        ) {
            if is_static {
                self.functions.push(function);
            } else {
                self.classes[id.0].methods.push(function);
            }
        }
        self.cur_file = saved_file;
        self.subst = saved_subst;
        self.leave_container_context(saved_context);
        Some(instance)
    }

    /// Instantiates a generic class at explicit type arguments, checking
    /// its shape and bodies immediately. Returns the instance id.
    ///
    /// An argument that is the error type gives no instance: the instance
    /// type is the error type (compiler.md §132 rule 2a). The type-argument
    /// count does not depend on the argument types, so a wrong count
    /// reports first.
    pub(crate) fn instantiate_class(
        &mut self,
        key: &str,
        args: &[Type],
        pos: Pos,
    ) -> Option<ClassId> {
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
        let name = self.mono_name(key, args);
        if let Some(&id) = self.class_ids.get(&name) {
            return Some(id);
        }
        let saved_file = self.cur_file;
        let saved_subst = std::mem::take(&mut self.subst);
        let saved_context = self.suspend_container_context();
        self.cur_file = template.file;
        for (param, arg) in template.type_params.iter().zip(args) {
            self.subst.insert(param.clone(), arg.clone());
        }
        let id = self.new_class(
            &name,
            template.is_value,
            template.is_descriptor,
            template.alignment_override,
            template.pos.clone(),
        );
        self.resolve_class_shape(id, &template.class, template.declared);
        if template.is_descriptor {
            self.check_descriptor_defaults(id, &template.class);
        } else {
            self.check_class_body(id, &template.class, template.declared);
        }
        self.cur_file = saved_file;
        self.subst = saved_subst;
        self.leave_container_context(saved_context);
        Some(id)
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
