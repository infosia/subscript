use super::*;

impl<'p> Checker<'p> {
    // ----- pass C: bodies -----

    pub(super) fn check_bodies(&mut self, file: usize) {
        let module = &self.prog.files[file].module;
        for item in &module.body {
            match item {
                ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(e)) => {
                    self.check_body_decl(&e.decl, true);
                }
                ast::ModuleItem::Stmt(ast::Stmt::Decl(d)) => self.check_body_decl(d, false),
                ast::ModuleItem::Stmt(s) => {
                    let mut fx = FnCtx::new(Type::Void, false, None, self.diags.clone());
                    let mut out = Vec::new();
                    self.check_stmt(s, &mut fx, &mut out);
                    let out = if has_dispose_binding(&out) {
                        using_scope::structure(out, &mut self.next_using_switch_id)
                    } else {
                        out
                    };
                    self.top_level.extend(out);
                }
                _ => {}
            }
        }
    }

    pub(super) fn check_descriptor_defaults_in_file(&mut self, file: usize) {
        let module = &self.prog.files[file].module;
        for item in &module.body {
            let Some(decl) = module_decl(item) else {
                continue;
            };
            let ast::Decl::Class(class) = decl else {
                continue;
            };
            if class.class.type_params.is_some() {
                continue;
            }
            let Some(&id) = self
                .class_ids
                .get(&self.declaration_symbol(file, class.ident.sym.as_ref()))
            else {
                continue;
            };
            if self.classes[id.0].is_descriptor {
                self.check_descriptor_defaults(id, &class.class);
            }
        }
    }

    pub(super) fn check_descriptor_defaults(&mut self, id: ClassId, class: &ast::Class) {
        let this_ty = Type::Class(id);
        for member in &class.body {
            let ast::ClassMember::ClassProp(prop) = member else {
                continue;
            };
            let ast::PropName::Ident(key) = &prop.key else {
                continue;
            };
            let Some(value) = &prop.value else {
                continue;
            };
            let field_ty = self.classes[id.0]
                .fields
                .iter()
                .find(|field| field.name == key.sym.as_ref() && field.is_defaulted)
                .map(|field| field.ty.clone());
            let Some(field_ty) = field_ty else {
                continue;
            };
            let mut fx = FnCtx::new(Type::Void, false, Some(this_ty.clone()), self.diags.clone());
            let checked = fx
                .with_synthetic_owner(
                    SyntheticOwnerKind::Initializer(self.pos(value.span())),
                    |fx| {
                        let checked = self.check_expr(value, Some(&field_ty), fx);
                        self.require_assignable(
                            &checked.ty.clone(),
                            &field_ty,
                            checked.pos.clone(),
                            "the descriptor member default",
                        );
                        checked
                    },
                )
                .0;
            if let Some(field) = self.classes[id.0]
                .fields
                .iter_mut()
                .find(|field| field.name == key.sym.as_ref())
            {
                field.init = Some(checked);
            }
        }
    }

    fn check_body_decl(&mut self, decl: &ast::Decl, exported: bool) {
        match decl {
            ast::Decl::Fn(f) if f.function.type_params.is_none() => {
                let name = self.declaration_symbol(self.cur_file, f.ident.sym.as_ref());
                let pos = self.pos(f.ident.span);
                let Some(sig) = self.fn_sigs.get(&name).cloned() else {
                    return;
                };
                let function =
                    self.check_function(&f.function, &name, exported, &sig, (None, None), pos);
                if let Some(function) = function {
                    self.functions.push(function);
                }
            }
            ast::Decl::Class(c) if c.class.type_params.is_none() => {
                let name = self.declaration_symbol(self.cur_file, c.ident.sym.as_ref());
                if let Some(&id) = self.class_ids.get(&name) {
                    self.check_class_body(id, &c.class, c.declare);
                }
            }
            ast::Decl::Var(v) => {
                if v.kind == ast::VarDeclKind::Var {
                    let pos = self.pos(v.span);
                    self.error(
                        RuleCode::S100,
                        "`var` is not in the language; use `let` or `const`",
                        pos,
                    );
                    return;
                }
                for d in &v.decls {
                    let ast::Pat::Ident(binding) = &d.name else {
                        continue;
                    };
                    let name = self.declaration_symbol(self.cur_file, binding.id.sym.as_ref());
                    let Some(sig) = self.global_sigs.get(&name).cloned() else {
                        continue;
                    };
                    let pos = self.pos(binding.id.span);
                    let mut fx = FnCtx::new(Type::Void, false, None, self.diags.clone());
                    let init = match &d.init {
                        Some(init) => {
                            fx.with_synthetic_owner(
                                SyntheticOwnerKind::Initializer(self.pos(init.span())),
                                |fx| {
                                    let e = self.check_expr(init, Some(&sig.ty), fx);
                                    self.require_assignable(
                                        &e.ty.clone(),
                                        &sig.ty,
                                        e.pos.clone(),
                                        "the initializer",
                                    );
                                    e
                                },
                            )
                            .0
                        }
                        None => {
                            self.error(
                                RuleCode::S100,
                                "module-level variables require an initializer",
                                pos.clone(),
                            );
                            hir::Expr {
                                kind: hir::ExprKind::Null,
                                ty: Type::Error,
                                pos: pos.clone(),
                            }
                        }
                    };
                    self.globals.push(hir::Global {
                        symbol: hir::Symbol::from_full_text(name.clone()),
                        name: source_name(&name),
                        ty: sig.ty,
                        mutable: sig.mutable,
                        init,
                        initializer_index: self.top_level.len(),
                        pos,
                    });
                }
            }
            _ => {}
        }
    }

    /// Checks a function body against its resolved signature and builds
    /// the HIR function. Returns `None` for poisoned signatures.
    pub(crate) fn check_function(
        &mut self,
        f: &ast::Function,
        name: &str,
        exported: bool,
        sig: &FnSig,
        this: (Option<Type>, Option<Divergence>),
        pos: Pos,
    ) -> Option<hir::Function> {
        let (this_ty, missing_this_divergence) = this;
        let mut fx = FnCtx::new(
            sig.ret.clone(),
            sig.is_generator,
            this_ty,
            self.diags.clone(),
        );
        fx.frames[0].missing_this_divergence = missing_this_divergence;
        fx.frames[0].is_async = sig.is_async;
        if sig.is_generator {
            if let Type::Generator(y) = &self.apparent_type(&sig.ret) {
                if sig.yield_known {
                    fx.frames[0].yield_ty = Some((**y).clone());
                }
            }
        }
        let (params, prologue) = self.bind_params(f, sig, &mut fx);
        let body = match &f.body {
            Some(block) => {
                #[cfg(test)]
                super::body_check_cost::record();
                self.reserve_block_declarations(&block.stmts, &mut fx);
                let mut out = prologue;
                for s in &block.stmts {
                    self.check_stmt(s, &mut fx, &mut out);
                }
                out
            }
            None => {
                self.error(RuleCode::S100, "function bodies are required", pos.clone());
                Vec::new()
            }
        };
        let unhandled = fx
            .async_origins
            .iter()
            .filter(|(_, handled)| !*handled)
            .map(|(pos, _)| pos.clone())
            .collect::<Vec<_>>();
        for origin in unhandled {
            self.error_diverging(
                RuleCode::S013,
                "an async handle is dropped without any await of its completion",
                origin,
                Divergence::DroppedAsyncHandle,
            );
        }
        let body = if has_dispose_binding(&body) {
            using_scope::structure(body, &mut self.next_using_switch_id)
        } else {
            body
        };
        let ret = if sig.is_generator {
            let yield_ty = fx.frames[0].yield_ty.clone().unwrap_or(Type::Void);
            let ret = Type::generator(yield_ty);
            if let Some(entry) = self.fn_sigs.get_mut(name) {
                entry.ret = ret.clone();
                entry.yield_known = true;
            }
            ret
        } else {
            if f.body.is_some()
                && !matches!(self.apparent_type(&sig.ret), Type::Void | Type::Error)
                && !stmt::always_returns(&body)
            {
                self.error(RuleCode::S100, "not all paths return a value", pos.clone());
            }
            sig.ret.clone()
        };
        Some(hir::Function {
            synthesized_helper: false,
            can_raise: false,
            symbol: hir::Symbol::from_full_text(name),
            name: source_name(name),
            exported,
            is_generator: sig.is_generator,
            is_async: sig.is_async,
            params,
            ret,
            body,
            pos,
        })
    }

    /// Declares parameters as locals and checks default values. Answers
    /// with the parameters and the statements that bind every pattern
    /// parameter at entry (§107.2).
    fn bind_params(
        &mut self,
        f: &ast::Function,
        sig: &FnSig,
        fx: &mut FnCtx,
    ) -> (Vec<hir::Param>, Vec<hir::Stmt>) {
        let mut out = Vec::new();
        let mut patterns = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            let Some(ps) = sig.params.get(i) else { break };
            let pos = self.pos(p.span);
            let default = match &p.pat {
                ast::Pat::Assign(a) => Some(
                    fx.with_synthetic_owner(
                        SyntheticOwnerKind::Initializer(self.pos(a.right.span())),
                        |fx| {
                            let e = self.check_expr(&a.right, Some(&ps.ty), fx);
                            self.require_assignable(
                                &e.ty.clone(),
                                &ps.ty,
                                e.pos.clone(),
                                "the default value",
                            );
                            e
                        },
                    )
                    .0,
                ),
                _ => None,
            };
            self.declare_local(
                &ps.name,
                Local {
                    ty: ps.ty.clone(),
                    mutable: true,
                    async_origins: if ps.ty.carries_async_handle() {
                        HashSet::from([fx.register_async_origin(pos.clone())])
                    } else {
                        HashSet::new()
                    },
                    caught: false,
                },
                pos.clone(),
                fx,
            );
            out.push(hir::Param {
                escapes: false,
                name: ps.name.clone(),
                ty: ps.ty.clone(),
                default,
                foreign_provenance: None,
                pos,
            });
            patterns.push((ps.clone(), p.pat.clone()));
        }
        let prologue = self.bind_parameter_patterns(patterns, fx);
        (out, prologue)
    }

    /// Checks field initializers, the constructor, and methods (pass C).
    pub(crate) fn check_class_body(&mut self, id: ClassId, class: &ast::Class, declared: bool) {
        if self.classes[id.0].is_descriptor {
            return;
        }
        let this_ty = Type::Class(id);
        let mut checked_read_accessors = HashSet::new();
        let mut checked_write_accessors = HashSet::new();
        for member in &class.body {
            match member {
                ast::ClassMember::ClassProp(prop) => {
                    let ast::PropName::Ident(key) = &prop.key else {
                        continue;
                    };
                    if prop.is_static {
                        let name = key.sym.to_string();
                        let Some(signature) =
                            self.class_sigs[id.0].static_fields.get(&name).cloned()
                        else {
                            continue;
                        };
                        let pos = self.pos(key.span);
                        let mut fx = FnCtx::new(Type::Void, false, None, self.diags.clone());
                        fx.frames[0].missing_this_divergence =
                            Some(Divergence::StaticMemberSurface);
                        let init = match &prop.value {
                            Some(value) => {
                                fx.with_synthetic_owner(
                                    SyntheticOwnerKind::Initializer(self.pos(value.span())),
                                    |fx| {
                                        let expression =
                                            self.check_expr(value, Some(&signature.ty), fx);
                                        self.require_assignable(
                                            &expression.ty.clone(),
                                            &signature.ty,
                                            expression.pos.clone(),
                                            "the static field initializer",
                                        );
                                        expression
                                    },
                                )
                                .0
                            }
                            None => {
                                self.error(
                                    RuleCode::S100,
                                    "static fields require an initializer",
                                    pos.clone(),
                                );
                                hir::Expr {
                                    kind: hir::ExprKind::Null,
                                    ty: Type::Error,
                                    pos: pos.clone(),
                                }
                            }
                        };
                        self.globals.push(hir::Global {
                            symbol: hir::Symbol::from_full_text(static_member_symbol(
                                id,
                                &self.classes[id.0].name,
                                &name,
                            )),
                            name: format!("{}.{}", self.classes[id.0].name, source_name(&name)),
                            ty: signature.ty,
                            mutable: signature.mutable,
                            init,
                            initializer_index: self.top_level.len(),
                            pos,
                        });
                        continue;
                    }
                    let Some(value) = &prop.value else { continue };
                    let field_ty = self.classes[id.0]
                        .fields
                        .iter()
                        .find(|f| f.name == key.sym.as_ref())
                        .map(|f| f.ty.clone());
                    let Some(field_ty) = field_ty else { continue };
                    let mut fx = FnCtx::new(Type::Void, false, None, self.diags.clone());
                    fx.frames[0].missing_this_divergence = Some(Divergence::ThisInFieldInitializer);
                    let e = fx
                        .with_synthetic_owner(
                            SyntheticOwnerKind::Initializer(self.pos(value.span())),
                            |fx| {
                                let e = self.check_expr(value, Some(&field_ty), fx);
                                self.require_assignable(
                                    &e.ty.clone(),
                                    &field_ty,
                                    e.pos.clone(),
                                    "the field initializer",
                                );
                                e
                            },
                        )
                        .0;
                    if let Some(field) = self.classes[id.0]
                        .fields
                        .iter_mut()
                        .find(|f| f.name == key.sym.as_ref())
                    {
                        field.init = Some(e);
                    }
                }
                ast::ClassMember::Constructor(ctor) => {
                    let Some(params) = self.class_sigs[id.0].ctor.clone() else {
                        continue;
                    };
                    let pos = self.pos(ctor.span);
                    let sig = FnSig {
                        params,
                        ret: Type::Void,
                        is_generator: false,
                        is_async: false,
                        yield_known: true,
                    };
                    let mut fx =
                        FnCtx::new(Type::Void, false, Some(this_ty.clone()), self.diags.clone());
                    let mut hir_params = Vec::new();
                    let mut patterns = Vec::new();
                    for (i, p) in ctor.params.iter().enumerate() {
                        let ast::ParamOrTsParamProp::Param(param) = p else {
                            continue;
                        };
                        let Some(ps) = sig.params.get(i) else { break };
                        let default = match &param.pat {
                            ast::Pat::Assign(a) => Some(
                                fx.with_synthetic_owner(
                                    SyntheticOwnerKind::Initializer(self.pos(a.right.span())),
                                    |fx| self.check_expr(&a.right, Some(&ps.ty), fx),
                                )
                                .0,
                            ),
                            _ => None,
                        };
                        let param_pos = self.pos(param.span);
                        self.declare_local(
                            &ps.name,
                            Local {
                                ty: ps.ty.clone(),
                                mutable: true,
                                async_origins: HashSet::new(),
                                caught: false,
                            },
                            param_pos.clone(),
                            &mut fx,
                        );
                        hir_params.push(hir::Param {
                            escapes: false,
                            name: ps.name.clone(),
                            ty: ps.ty.clone(),
                            default,
                            foreign_provenance: None,
                            pos: param_pos,
                        });
                        patterns.push((ps.clone(), param.pat.clone()));
                    }
                    let mut body = self.bind_parameter_patterns(patterns, &mut fx);
                    if let Some(block) = &ctor.body {
                        self.reserve_block_declarations(&block.stmts, &mut fx);
                        for s in &block.stmts {
                            self.check_stmt(s, &mut fx, &mut body);
                        }
                    }
                    if has_dispose_binding(&body) {
                        body = using_scope::structure(body, &mut self.next_using_switch_id);
                    }
                    self.classes[id.0].ctor = Some(hir::Function {
                        synthesized_helper: false,
                        can_raise: false,
                        symbol: hir::Symbol::from_full_text("constructor"),
                        name: "constructor".to_string(),
                        exported: false,
                        is_generator: false,
                        is_async: false,
                        params: hir_params,
                        ret: Type::Void,
                        body,
                        pos,
                    });
                }
                ast::ClassMember::Method(method) => {
                    let (mut name, pos) = match &method.key {
                        ast::PropName::Ident(key) => (key.sym.to_string(), self.pos(key.span)),
                        key if is_dispose_method_key(key) => {
                            (hir::DISPOSE_METHOD_NAME.to_string(), self.pos(method.span))
                        }
                        _ => continue,
                    };
                    match method.kind {
                        ast::MethodKind::Getter => {
                            let has_accessor = if method.is_static {
                                self.class_sigs[id.0].has_static_accessor(&name)
                            } else {
                                self.class_sigs[id.0].has_accessor(&name)
                            };
                            if !has_accessor
                                || !checked_read_accessors.insert((method.is_static, name.clone()))
                            {
                                continue;
                            }
                        }
                        ast::MethodKind::Setter => {
                            if !checked_write_accessors.insert((method.is_static, name.clone())) {
                                continue;
                            }
                            name.push('=');
                        }
                        ast::MethodKind::Method => {
                            let has_accessor = if method.is_static {
                                self.class_sigs[id.0].has_static_accessor(&name)
                            } else {
                                self.class_sigs[id.0].has_accessor(&name)
                            };
                            if has_accessor {
                                continue;
                            }
                            // §82.4 rule 3: a template has no body of its
                            // own. `instantiate_method` checks each
                            // instance at its first call.
                            if self.class_sigs[id.0].has_generic_method(&name, method.is_static) {
                                continue;
                            }
                        }
                    }
                    let sig = if method.is_static {
                        self.class_sigs[id.0].static_methods.get(&name).cloned()
                    } else {
                        self.class_sigs[id.0].methods.get(&name).cloned()
                    };
                    let Some(sig) = sig else {
                        continue;
                    };
                    let function_name = if method.is_static {
                        static_member_symbol(id, &self.classes[id.0].name, &name)
                    } else {
                        name.clone()
                    };
                    if let Some(func) = self.check_function(
                        &method.function,
                        &function_name,
                        false,
                        &sig,
                        (
                            (!method.is_static).then(|| this_ty.clone()),
                            method.is_static.then_some(Divergence::StaticMemberSurface),
                        ),
                        pos,
                    ) {
                        if method.is_static {
                            self.functions.push(func);
                        } else {
                            self.classes[id.0].methods.push(func);
                        }
                    }
                }
                _ => {}
            }
        }
        self.require_field_values(id, class, declared);
    }

    /// compiler.md §108.1: a declared field carries a value before the
    /// constructor returns. A field with no initializer is assigned at
    /// the constructor's top level, or the class is rejected at the
    /// field. A `!` assertion does not satisfy the rule. Rule 3 keeps a
    /// mirror field, a `@Descriptor` member, and a static field outside:
    /// a mirror class and a `declare class` have no constructor body, a
    /// `@Descriptor` class never reaches this pass, and a static field
    /// is a global with S100's own initializer rule. An instance of a
    /// generic template inherits the template's ambient status, so an
    /// ambient template stays outside the rule.
    ///
    /// The rule reads the constructor's top level only, and a top-level
    /// assignment counts only when no statement before it holds a
    /// `return` (rule 2). The diagnostic distinguishes four shapes,
    /// because §79 rule 4 pairs each with its measured `tsc` class: a
    /// field that no statement assigns (`tsc` answers TS2564), a `!`
    /// field (`tsc` accepts), a field assigned only inside a nested
    /// statement (`tsc` follows definite assignment; this rule does
    /// not), and a field whose top-level assignment stands after a
    /// statement that holds a `return` (`tsc` answers TS2564).
    fn require_field_values(&mut self, id: ClassId, class: &ast::Class, declared: bool) {
        if declared || self.classes[id.0].is_boundary {
            return;
        }
        let mut spellings: HashMap<String, FieldSpelling> = HashMap::new();
        for member in &class.body {
            let ast::ClassMember::ClassProp(prop) = member else {
                continue;
            };
            let ast::PropName::Ident(key) = &prop.key else {
                continue;
            };
            if !prop.is_static {
                spellings.insert(
                    key.sym.to_string(),
                    FieldSpelling {
                        definite: prop.definite,
                        optional: prop.is_optional,
                        // The declared text, not the resolved type: a
                        // generic template declares `T`, and the advice
                        // is written into the template.
                        declared_type: prop
                            .type_ann
                            .as_ref()
                            .and_then(|annotation| self.prog.snippet(annotation.type_ann.span())),
                    },
                );
            }
        }
        let (top_level, anywhere, after_return) = match &self.classes[id.0].ctor {
            Some(ctor) => {
                let mut top_level = HashSet::new();
                let mut after_return = HashSet::new();
                let mut left = false;
                for statement in &ctor.body {
                    if let Some(name) = this_field_assignment(statement) {
                        if left {
                            after_return.insert(name.to_string());
                        } else {
                            top_level.insert(name.to_string());
                        }
                    }
                    left = left || leaves_the_function(statement);
                }
                let mut anywhere = HashSet::new();
                for statement in &ctor.body {
                    collect_this_field_assignments(hir::HirChild::Stmt(statement), &mut anywhere);
                }
                (top_level, anywhere, after_return)
            }
            None => (HashSet::new(), HashSet::new(), HashSet::new()),
        };
        let class_name = self.classes[id.0].name.clone();
        let unassigned: Vec<(String, Type, Pos)> = self.classes[id.0]
            .fields
            .iter()
            .filter(|field| field.init.is_none() && self.apparent_type(&field.ty) != Type::Error)
            .map(|field| (field.name.clone(), field.ty.clone(), field.pos.clone()))
            .collect();
        for (name, ty, pos) in unassigned {
            let spelling = spellings.get(&name).cloned().unwrap_or_default();
            if spelling.optional || top_level.contains(&name) {
                continue;
            }
            // `…` names no field, so the advice never reads as an
            // assignment of the field to itself.
            let declared = spelling
                .declared_type
                .unwrap_or_else(|| self.type_name(&ty));
            let spellings = format!(
                "write `{name}: {declared} = …`, or assign `this.{name} = …` at the top level \
                 of the constructor"
            );
            if spelling.definite {
                self.error_diverging(
                    RuleCode::S100,
                    format!(
                        "field `{name}` of `{class_name}` asserts with `!` a value that nothing \
                         assigns at the constructor's top level; {spellings}"
                    ),
                    pos,
                    Divergence::DefiniteAssignmentAssertion,
                );
            } else if after_return.contains(&name) {
                self.error(
                    RuleCode::S100,
                    format!(
                        "field `{name}` of `{class_name}` is assigned at the constructor's top \
                         level after a statement that holds a `return`, so the constructor can \
                         return before the assignment (stock `tsc` answers TS2564); {spellings}, \
                         before every statement that holds a `return`"
                    ),
                    pos,
                );
            } else if anywhere.contains(&name) {
                self.error_diverging(
                    RuleCode::S100,
                    format!(
                        "field `{name}` of `{class_name}` is assigned inside a nested statement \
                         of the constructor, not at its top level; {spellings}"
                    ),
                    pos,
                    Divergence::NestedFieldAssignment,
                );
            } else {
                self.error(
                    RuleCode::S100,
                    format!(
                        "field `{name}` of `{class_name}` has no initializer, and no constructor \
                         statement assigns it (stock `tsc` answers TS2564); {spellings}"
                    ),
                    pos,
                );
            }
        }
        self.check_this_in_assignment_prefix(id, &spellings);
    }

    /// compiler.md §108.4 rule 6: `this` inside the constructor's
    /// assignment prefix appears in two forms only. Form (a) is the
    /// target of `this.f = …`, at any depth. Form (b) is a read `this.g`
    /// of a field that holds a value at that statement.
    ///
    /// The assignment prefix is the constructor's parameter defaults,
    /// followed by its top-level statements up to and including the last
    /// top-level statement that assigns a field rule 1 reaches. A field
    /// holds a value when it has an initializer, or when a top-level
    /// statement earlier in the prefix assigns it. That is rule 2's
    /// notion: a nested assignment does not count, and the statement's
    /// own target does not count. A class with no rule-1 field has an
    /// empty prefix, and after the prefix every rule-1 field holds a
    /// value.
    ///
    /// Site A is a read of a field that holds no value. Stock `tsc`
    /// answers TS2565 for the measured forms, so the site carries no
    /// variant. Site B is a method or an accessor call on `this`, or
    /// `this` as a value; `tsc` accepts those, because its
    /// definite-assignment analysis does not follow a call.
    fn check_this_in_assignment_prefix(
        &mut self,
        id: ClassId,
        spellings: &HashMap<String, FieldSpelling>,
    ) {
        let rule_one_fields: Vec<String> = self.classes[id.0]
            .fields
            .iter()
            .filter(|field| field.init.is_none() && self.apparent_type(&field.ty) != Type::Error)
            .filter(|field| !spellings.get(&field.name).is_some_and(|s| s.optional))
            .map(|field| field.name.clone())
            .collect();
        if rule_one_fields.is_empty() {
            return;
        }
        let mut held: HashSet<String> = self.classes[id.0]
            .fields
            .iter()
            .map(|field| field.name.clone())
            .filter(|name| !rule_one_fields.contains(name))
            .collect();
        let Some(ctor) = self.classes[id.0].ctor.as_ref() else {
            return;
        };
        let mut collected: Vec<PrefixViolation> = Vec::new();
        let mut found = Vec::new();
        for parameter in &ctor.params {
            if let Some(default) = &parameter.default {
                prefix_this_violations(hir::HirChild::Expr(default), &held, &mut found);
            }
        }
        record_prefix_violations(found, &rule_one_fields, &held, &mut collected);
        // The prefix ends with the first top-level statement after which
        // every rule-1 field holds a value. If no statement completes the
        // set, the prefix is the whole constructor and rule 1 reports as
        // well.
        for statement in &ctor.body {
            let mut found = Vec::new();
            prefix_this_violations(hir::HirChild::Stmt(statement), &held, &mut found);
            record_prefix_violations(found, &rule_one_fields, &held, &mut collected);
            if let Some(name) = this_field_assignment(statement) {
                held.insert(name.to_string());
            }
            if rule_one_fields.iter().all(|field| held.contains(field)) {
                break;
            }
        }
        let class_name = self.classes[id.0].name.clone();
        for violation in collected {
            let PrefixViolation {
                pos,
                kind,
                first_missing,
                other_missing,
            } = violation;
            let (use_site, advice) = match kind {
                PrefixThis::Read(name) => {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "`this.{name}` reads field `{name}` of `{class_name}` before the \
                             constructor assigns it at its top level; move the read after \
                             `this.{name} = …`, or give `{name}` an initializer"
                        ),
                        pos,
                    );
                    continue;
                }
                PrefixThis::Call => ("calls a member of `this`", "call"),
                PrefixThis::Value => ("uses `this` as a value", "use"),
            };
            let named = field_list(&first_missing, &other_missing);
            self.error_diverging(
                RuleCode::S100,
                format!(
                    "the constructor of `{class_name}` {use_site} before {} {} a value; move the \
                     {advice} after {}, or {}",
                    named.subject, named.verb, named.assignments, named.initializers
                ),
                pos,
                Divergence::ThisBeforeFieldValues,
            );
        }
    }
}
