//! Declaration type decisions and checked initializer ownership (§156).
use super::*;

#[derive(Debug, Clone)]
pub(crate) enum TypeState {
    Undecided,
    InProgress,
    Decided(Type),
    Rejected,
}

impl TypeState {
    pub(crate) fn decided(ty: Type) -> Self {
        if ty == Type::Error {
            Self::Rejected
        } else {
            Self::Decided(ty)
        }
    }

    pub(crate) fn ty(&self) -> &Type {
        match self {
            Self::Decided(ty) => ty,
            // This is poison for consumers after the decision pass. Reads
            // during a decision go through decide_global or decide_field.
            _ => &Type::Error,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Initializer {
    expression: Box<ast::Expr>,
    file: usize,
    subst: HashMap<String, Type>,
    class: Option<ClassId>,
    checked: Option<hir::Expr>,
    field_context: Option<field_initializer::FieldInitializer>,
    captures: Vec<hir::Capture>,
    pending: Vec<DeferredExpression>,
}

#[derive(Debug, Clone)]
pub(crate) struct DeferredExpression {
    id: usize,
    work: DeferredWork,
    frame: FnCtx,
    file: usize,
    subst: HashMap<String, Type>,
}

#[derive(Debug, Clone)]
pub(crate) enum DeferredWork {
    Lambda {
        source: ast::ArrowExpr,
        params: Vec<ParamSig>,
        result: Type,
    },
    Arguments {
        source: Vec<ast::ExprOrSpread>,
        params: Vec<ParamSig>,
        site: RejectionSite,
        what: String,
        checked: Option<Vec<Option<hir::Expr>>>,
    },
}

#[derive(Clone)]
enum ParameterOwner {
    Function(String),
    Constructor(ClassId),
    Method(ClassId, String, bool),
}

impl<'p> Checker<'p> {
    pub(crate) fn initializer(
        &self,
        expression: &ast::Expr,
        class: Option<ClassId>,
    ) -> Initializer {
        Initializer {
            expression: Box::new(expression.clone()),
            file: self.cur_file,
            subst: self.subst.clone(),
            class,
            checked: None,
            field_context: None,
            captures: Vec::new(),
            pending: Vec::new(),
        }
    }

    pub(crate) fn inferred_initializer_type(&mut self, value: &hir::Expr, pos: Pos) -> Type {
        if self.apparent_type(&value.ty) == Type::Null {
            self.reject_subset(
                RejectionSite::NullInitializerInference,
                "cannot infer a type from `null`; annotate the declaration",
                pos,
            );
            Type::Error
        } else {
            value.ty.clone()
        }
    }

    pub(crate) fn cycle(&mut self, pos: Pos) -> Type {
        if self.function_value_decision {
            return self.parameter_value_cycle(pos);
        }
        self.reject_subset(
            RejectionSite::InitializerTypeCycle,
            "initializer type depends on its own undecided type",
            pos,
        );
        Type::Error
    }

    pub(crate) fn parameter_value_cycle(&mut self, pos: Pos) -> Type {
        self.reject_subset(
            RejectionSite::FunctionValueParameterAnnotationNeeded,
            "avoid a recursive function value in the parameter default",
            pos,
        );
        Type::Error
    }

    pub(crate) fn defers_call_arguments(&self, generic: bool) -> bool {
        self.deciding_type
            && self.initializer_call.is_some()
            && self.initializer_call == self.active_call
            && !generic
    }

    fn decide_initializer(&mut self, source: &mut Initializer, fx: &mut FnCtx) -> Type {
        let file = std::mem::replace(&mut self.cur_file, source.file);
        let subst = std::mem::replace(&mut self.subst, source.subst.clone());
        let deciding = std::mem::replace(&mut self.deciding_type, true);
        let root = expr::unparen_expr(&source.expression);
        let root_call = matches!(root, ast::Expr::Call(_)).then(|| root.span());
        let initializer_root = self.initializer_root.replace(root.span());
        let call = std::mem::replace(&mut self.initializer_call, root_call);
        let direct_value = self.function_value_decision && matches!(root, ast::Expr::Ident(_));
        let function_value = std::mem::replace(&mut self.function_value_decision, direct_value);
        let diagnostics = self.diags.len();
        let pending = std::mem::take(&mut self.deferred_expressions);
        let value = fx
            .with_synthetic_owner(
                SyntheticOwnerKind::Initializer(self.pos(source.expression.span())),
                |fx| self.check_expr(&source.expression, None, fx),
            )
            .0;
        let ty = self.inferred_initializer_type(&value, self.pos(source.expression.span()));
        let ty = if self.diags.len() == diagnostics {
            ty
        } else {
            Type::Error
        };
        source.checked = Some(value);
        source.pending = std::mem::replace(&mut self.deferred_expressions, pending);
        source.captures = fx
            .frames
            .last()
            .map(|frame| frame.captures.clone())
            .unwrap_or_default();
        self.initializer_root = initializer_root;
        self.function_value_decision = function_value;
        self.deciding_type = deciding;
        self.initializer_call = call;
        self.cur_file = file;
        self.subst = subst;
        ty
    }

    pub(crate) fn decide_global(&mut self, name: &str, pos: Pos) -> Type {
        let Some(sig) = self.global_sigs.get(name).cloned() else {
            return Type::Error;
        };
        match sig.state {
            TypeState::Decided(ty) => return ty,
            TypeState::Rejected => return Type::Error,
            TypeState::InProgress => return self.cycle(pos),
            TypeState::Undecided => {}
        }
        if let Some(sig) = self.global_sigs.get_mut(name) {
            sig.state = TypeState::InProgress;
        }
        let mut source = sig.initializer;
        let ty = if let Some(source) = &mut source {
            let mut fx = FnCtx::new(Type::Void, false, None, self.diags.clone());
            fx.lexical_class = source.class;
            if source.class.is_some() {
                fx.frames[0].missing_this_site = Some(RejectionSite::ThisStaticField);
            }
            self.decide_initializer(source, &mut fx)
        } else {
            Type::Error
        };
        if let Some(sig) = self.global_sigs.get_mut(name) {
            sig.state = TypeState::decided(ty.clone());
            sig.initializer = source;
        }
        ty
    }

    pub(crate) fn decide_field(&mut self, id: ClassId, name: &str, pos: Pos) -> Type {
        let Some(sig) = self.class_sigs[id.0].fields.get(name).cloned() else {
            return self.classes[id.0]
                .fields
                .iter()
                .find(|f| f.name == name)
                .map_or(Type::Error, |f| f.ty.clone());
        };
        match sig.state {
            TypeState::Decided(ty) => return ty,
            TypeState::Rejected => return Type::Error,
            TypeState::InProgress => return self.cycle(pos),
            TypeState::Undecided => {}
        }
        if let Some(sig) = self.class_sigs[id.0].fields.get_mut(name) {
            sig.state = TypeState::InProgress;
        }
        let mut source = sig.initializer;
        let ty = if let Some(source) = &mut source {
            let mut fx = FnCtx::new(Type::Void, false, Some(Type::Class(id)), self.diags.clone());
            fx.lexical_class = Some(id);
            if self.classes[id.0].is_descriptor {
                fx.descriptor_default = Some(Type::Class(id));
            } else {
                fx.field_initializer = source.field_context.clone();
            }
            self.decide_initializer(source, &mut fx)
        } else {
            Type::Error
        };
        let diagnostics = self.diags.len();
        self.check_inferred_field_type(id, &ty, pos);
        let ty = if self.diags.len() == diagnostics {
            ty
        } else {
            Type::Error
        };
        if let Some(sig) = self.class_sigs[id.0].fields.get_mut(name) {
            sig.state = TypeState::decided(ty.clone());
            sig.initializer = source;
        }
        if let Some(field) = self.classes[id.0]
            .fields
            .iter_mut()
            .find(|f| f.name == name)
        {
            field.ty = ty.clone();
        }
        ty
    }

    pub(crate) fn decide_lambda_parameters(&mut self, params: &mut [ParamSig], fx: &FnCtx) {
        if params.iter().all(|param| param.initializer.is_none()) {
            return;
        }
        let mut context = fx.clone();
        let mut frame = context.frames.last().cloned().unwrap_or_else(|| {
            FnCtx::new(Type::Void, false, None, self.diags.clone())
                .frames
                .remove(0)
        });
        frame.is_lambda = true;
        frame.lambda_id = None;
        frame.this_ty = None;
        frame.captures.clear();
        context.frames.push(frame);
        context.scopes.push(Scope {
            fn_boundary: true,
            ..Default::default()
        });
        self.decide_parameters_in(params, &mut context, None);
    }

    fn owner_parameter(&mut self, owner: &ParameterOwner, index: usize) -> Option<&mut ParamSig> {
        let parameters = match owner {
            ParameterOwner::Function(name) => &mut self.fn_sigs.get_mut(name)?.params,
            ParameterOwner::Constructor(id) => self.class_sigs.get_mut(id.0)?.ctor.as_mut()?,
            ParameterOwner::Method(id, name, is_static) => {
                let signature = self.class_sigs.get_mut(id.0)?;
                let methods = if *is_static {
                    &mut signature.static_methods
                } else {
                    &mut signature.methods
                };
                &mut methods.get_mut(name)?.params
            }
        };
        parameters.get_mut(index)
    }

    fn publish_parameter(&mut self, owner: &ParameterOwner, index: usize, param: &ParamSig) {
        if let Some(stored) = self.owner_parameter(owner, index) {
            *stored = param.clone();
        }
        if let ParameterOwner::Method(id, name, true) = owner {
            let symbol = static_member_symbol(*id, &self.classes[id.0].name, name);
            if let Some(stored) = self
                .fn_sigs
                .get_mut(&symbol)
                .and_then(|sig| sig.params.get_mut(index))
            {
                *stored = param.clone();
            }
        }
    }

    fn decide_parameters_in(
        &mut self,
        params: &mut [ParamSig],
        fx: &mut FnCtx,
        owner: Option<&ParameterOwner>,
    ) {
        let scope_index = fx.scopes.len().saturating_sub(1);
        for (index, param) in params.iter_mut().enumerate() {
            if let Some(stored) = owner.and_then(|owner| self.owner_parameter(owner, index)) {
                *param = stored.clone();
            }
            if matches!(param.state, TypeState::Undecided | TypeState::InProgress) {
                if let Some(scope) = fx.scopes.get_mut(scope_index) {
                    scope.pending.insert(param.name.clone());
                }
                fx.parameter_decisions
                    .insert((scope_index, param.name.clone()));
                if matches!(param.state, TypeState::InProgress) {
                    continue;
                }
                param.state = TypeState::InProgress;
                if let Some(owner) = owner {
                    self.publish_parameter(owner, index, param);
                }
                let ty = param
                    .initializer
                    .as_mut()
                    .map_or(Type::Error, |source| self.decide_initializer(source, fx));
                let ty = if let Some(ParameterOwner::Method(id, _, _)) = owner {
                    let class_parameters = self
                        .instance_arguments
                        .get(id)
                        .map(|(_, args)| args.clone())
                        .unwrap_or_default();
                    if class_parameters.iter().any(|parameter| {
                        self.involves_type_parameter(parameter)
                            && self.default_contains_class_parameter(&ty, parameter)
                    }) {
                        let pos = param
                            .initializer
                            .as_ref()
                            .map(|source| self.pos(source.expression.span()))
                            .unwrap_or_else(|| self.classes[id.0].pos.clone());
                        self.reject_subset(RejectionSite::GenericClassDefaultParameterAnnotationNeeded,
                            "a generic-class default needs a parameter type; annotate the parameter", pos);
                        Type::Error
                    } else {
                        ty
                    }
                } else {
                    ty
                };
                param.state = TypeState::decided(ty);
                fx.parameter_decisions
                    .remove(&(scope_index, param.name.clone()));
                if let Some(owner) = owner {
                    self.publish_parameter(owner, index, param);
                }
            }
            fx.declare(
                &param.name,
                Local {
                    ty: param.ty().clone(),
                    mutable: true,
                    async_origins: HashSet::new(),
                    caught: false,
                },
            );
        }
        // A delayed default body reads the completed parameter scope.
        let locals = fx
            .scopes
            .get(scope_index)
            .map(|scope| scope.vars.clone())
            .unwrap_or_default();
        for (index, param) in params.iter_mut().enumerate() {
            if let Some(source) = &mut param.initializer {
                for deferred in &mut source.pending {
                    deferred.frame.parameter_decisions.clear();
                    if let Some(scope) = deferred.frame.scopes.get_mut(scope_index) {
                        for name in locals.keys() {
                            scope.pending.remove(name);
                        }
                        scope.vars.extend(locals.clone());
                    }
                }
            }
            if !matches!(param.state, TypeState::InProgress) {
                if let Some(owner) = owner {
                    self.publish_parameter(owner, index, param);
                }
            }
        }
    }

    // Rule 6 reads declared parameter identity; constraint projection loses that identity.
    fn default_contains_class_parameter(&self, ty: &Type, parameter: &Type) -> bool {
        ty == parameter
            || matches!(ty, Type::Class(id) if self.instance_arguments.get(id)
                .is_some_and(|(_, args)| args.iter().any(|ty| self.default_contains_class_parameter(ty, parameter))))
            || ty
                .contained_types()
                .into_iter()
                .any(|ty| self.default_contains_class_parameter(ty, parameter))
    }

    pub(crate) fn decide_function_parameters(&mut self, name: &str) {
        let method = self.static_method_owners.get(name).cloned();
        if let Some((id, method)) = method {
            self.decide_method_parameters(id, &method, true);
            return;
        }
        let Some(mut sig) = self.fn_sigs.get(name).cloned() else {
            return;
        };
        if !sig
            .params
            .iter()
            .any(|p| matches!(p.state, TypeState::Undecided))
        {
            return;
        }
        let mut fx = FnCtx::new(Type::Void, false, None, self.diags.clone());
        self.decide_parameters_in(
            &mut sig.params,
            &mut fx,
            Some(&ParameterOwner::Function(name.to_string())),
        );
    }

    pub(super) fn decide_declarations(&mut self) {
        // Source order controls decisions; recursive reads decide dependencies.
        for file in 0..self.prog.files.len() {
            if self.prog.files[file].dts {
                continue;
            }
            self.cur_file = file;
            for item in &self.prog.files[file].module.body {
                let Some(decl) = module_decl(item) else {
                    continue;
                };
                match decl {
                    ast::Decl::Var(v) => {
                        for d in &v.decls {
                            if let ast::Pat::Ident(b) = &d.name {
                                let symbol = self.declaration_symbol(file, &b.id.sym);
                                self.decide_global(&symbol, self.pos(b.id.span));
                            }
                        }
                    }
                    ast::Decl::Fn(f) => {
                        let symbol = self.declaration_symbol(file, &f.ident.sym);
                        self.decide_function_parameters(&symbol);
                    }
                    _ => {}
                }
            }
        }
        let mut index = 0;
        while index < self.classes.len() {
            let id = ClassId(index);
            let fields: Vec<_> = self.classes[index]
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.pos.clone()))
                .collect();
            for (name, pos) in fields {
                self.decide_field(id, &name, pos);
            }
            let statics: Vec<_> = self.class_sigs[index]
                .static_fields
                .keys()
                .cloned()
                .collect();
            for name in statics {
                let symbol = static_member_symbol(id, &self.classes[index].name, &name);
                let pos = self
                    .global_sigs
                    .get(&symbol)
                    .and_then(|s| s.initializer.as_ref())
                    .map(|s| self.pos(s.expression.span()))
                    .unwrap_or_else(|| self.classes[index].pos.clone());
                let ty = self.decide_global(&symbol, pos.clone());
                if self.is_context_affine_type(&ty) {
                    self.reject_subset(
                        RejectionSite::ContextAffineStaticField,
                        "Worker, Inbox, and Outbox values may not be static fields",
                        pos,
                    );
                }
                if let Some(sig) = self.global_sigs.get(&symbol).cloned() {
                    self.class_sigs[index].static_fields.insert(name, sig);
                }
            }
            self.decide_class_parameters(id);
            index += 1;
        }
    }

    pub(crate) fn decide_class_parameters(&mut self, id: ClassId) {
        self.decide_constructor_parameters(id);
        for is_static in [false, true] {
            let names: Vec<_> = if is_static {
                &self.class_sigs[id.0].static_methods
            } else {
                &self.class_sigs[id.0].methods
            }
            .keys()
            .cloned()
            .collect();
            for name in names {
                self.decide_method_parameters(id, &name, is_static);
            }
        }
    }

    pub(crate) fn decide_constructor_parameters(&mut self, id: ClassId) {
        let Some(mut params) = self.class_sigs[id.0].ctor.clone() else {
            return;
        };
        if !params
            .iter()
            .any(|p| matches!(p.state, TypeState::Undecided))
        {
            return;
        }
        let mut fx = FnCtx::new(Type::Void, false, Some(Type::Class(id)), self.diags.clone());
        fx.lexical_class = Some(id);
        self.decide_parameters_in(&mut params, &mut fx, Some(&ParameterOwner::Constructor(id)));
    }

    pub(crate) fn decide_method_parameters(&mut self, id: ClassId, name: &str, is_static: bool) {
        let methods = if is_static {
            &mut self.class_sigs[id.0].static_methods
        } else {
            &mut self.class_sigs[id.0].methods
        };
        let Some(stored) = methods.get_mut(name) else {
            return;
        };
        if !stored
            .params
            .iter()
            .any(|p| matches!(p.state, TypeState::Undecided))
        {
            return;
        }
        let mut sig = stored.clone();
        let mut fx = FnCtx::new(
            Type::Void,
            false,
            (!is_static).then_some(Type::Class(id)),
            self.diags.clone(),
        );
        fx.lexical_class = Some(id);
        if is_static {
            fx.frames[0].missing_this_site = Some(RejectionSite::ThisStaticField);
            fx.frames[0].static_this_class = Some(id);
        }
        self.decide_parameters_in(
            &mut sig.params,
            &mut fx,
            Some(&ParameterOwner::Method(id, name.to_string(), is_static)),
        );
        if is_static {
            let symbol = static_member_symbol(id, &self.classes[id.0].name, name);
            self.fn_sigs.insert(symbol, sig);
        }
    }

    pub(super) fn with_expression_work(
        &mut self,
        check: impl FnOnce(&mut Self) -> hir::Expr,
    ) -> hir::Expr {
        self.expression_work.push(None);
        let mut value = check(self);
        if let Some(work) = self.expression_work.pop().flatten() {
            value.pending_work = Some(work);
        }
        value
    }

    pub(crate) fn defer_work(&mut self, work: DeferredWork, fx: &FnCtx) {
        let id = self.next_deferred_id;
        self.next_deferred_id += 1;
        if let Some(slot) = self.expression_work.last_mut() {
            *slot = Some(id);
        }
        self.deferred_expressions.push(DeferredExpression {
            id,
            work,
            frame: fx.clone(),
            file: self.cur_file,
            subst: self.subst.clone(),
        });
    }

    pub(crate) fn field_source(
        &self,
        value: &ast::Expr,
        id: ClassId,
        class: &ast::Class,
        current: &ast::ClassProp,
    ) -> Initializer {
        let mut source = self.initializer(value, Some(id));
        let mut earlier = HashSet::new();
        let mut definite_uninitialized = HashSet::new();
        let mut before = true;
        for member in &class.body {
            if let ast::ClassMember::ClassProp(prop) = member {
                if prop.span == current.span {
                    before = false;
                }
                if let ast::PropName::Ident(name) = &prop.key {
                    if !prop.is_static && before && prop.value.is_some() {
                        earlier.insert(name.sym.to_string());
                    }
                    if !prop.is_static && prop.definite && prop.value.is_none() {
                        definite_uninitialized.insert(name.sym.to_string());
                    }
                }
            }
        }
        source.field_context = Some(field_initializer::FieldInitializer {
            class_type: Type::Class(id),
            earlier,
            definite_uninitialized,
            write: false,
        });
        source
    }

    pub(crate) fn initializer_captures(source: &Option<Initializer>) -> &[hir::Capture] {
        source
            .as_ref()
            .map_or(&[], |source| source.captures.as_slice())
    }

    pub(crate) fn finish_initializer(
        &mut self,
        source: &mut Option<Initializer>,
    ) -> Option<hir::Expr> {
        let source = source.as_mut()?;
        let mut value = source.checked.take()?;
        if self.deciding_type {
            self.deferred_expressions.append(&mut source.pending);
        } else {
            let pending = std::mem::replace(
                &mut self.deferred_expressions,
                std::mem::take(&mut source.pending),
            );
            self.finish_expression(&mut value);
            self.deferred_expressions = pending;
        }
        Some(value)
    }

    pub(super) fn finish_expression(
        &mut self,
        value: &mut hir::Expr,
    ) -> Vec<(SyntheticOwnerKind, Vec<hir::Stmt>)> {
        let mut prefixes = self.finish_expression_children(value);
        if let Some(index) = value
            .pending_work
            .take()
            .and_then(|id| self.deferred_expressions.iter().position(|d| d.id == id))
        {
            let mut deferred = self.deferred_expressions.remove(index);
            let file = std::mem::replace(&mut self.cur_file, deferred.file);
            let subst = std::mem::replace(&mut self.subst, deferred.subst);
            let deciding = std::mem::replace(&mut self.deciding_type, false);
            match deferred.work {
                DeferredWork::Lambda {
                    source,
                    params,
                    result,
                } => {
                    *value = self.check_lambda_body(
                        &source,
                        params,
                        Some(result),
                        &mut deferred.frame,
                        value.pos.clone(),
                    );
                }
                DeferredWork::Arguments {
                    source,
                    mut params,
                    site,
                    what,
                    mut checked,
                } => {
                    // The child pass completes fixed argument slots before this call slot.
                    if let Some(checked) = &mut checked {
                        let arguments = match &value.kind {
                            hir::ExprKind::Call { args, .. }
                            | hir::ExprKind::New { args, .. }
                            | hir::ExprKind::AsyncCall { args, .. }
                            | hir::ExprKind::AsyncHandleCreate { args, .. } => Some(args),
                            _ => None,
                        };
                        if let Some(arguments) = arguments {
                            for (slot, value) in checked.iter_mut().zip(arguments) {
                                if slot.is_some() {
                                    *slot = Some(value.clone());
                                }
                            }
                        }
                    }
                    match &value.kind {
                        hir::ExprKind::Call {
                            callee: hir::Callee::Func(symbol),
                            ..
                        }
                        | hir::ExprKind::AsyncHandleCreate {
                            callee: hir::AsyncCallee::Function(symbol),
                            ..
                        }
                        | hir::ExprKind::AsyncCall {
                            callee: hir::AsyncCallee::Function(symbol),
                            ..
                        } => {
                            self.decide_function_parameters(symbol.full_text());
                            if let Some(sig) = self.fn_sigs.get(symbol.full_text()) {
                                params = sig.params.clone();
                            }
                        }
                        hir::ExprKind::Call {
                            callee: hir::Callee::Method { recv, name },
                            ..
                        } => {
                            if let Type::Class(id) = self.apparent_type(&recv.ty) {
                                if let Some(sig) =
                                    self.class_sigs[id.0].methods.get(name.full_text())
                                {
                                    params = sig.params.clone();
                                }
                            }
                        }
                        hir::ExprKind::New { class, .. } => {
                            if let Some(signature) = &self.class_sigs[class.0].ctor {
                                params = signature.clone();
                            }
                        }
                        _ => {}
                    }
                    let owner = deferred
                        .frame
                        .synthetic_owner_kinds
                        .last()
                        .cloned()
                        .unwrap_or_else(|| SyntheticOwnerKind::Initializer(value.pos.clone()));
                    let (completed, prefix) =
                        deferred.frame.with_synthetic_owner(owner.clone(), |fx| {
                            let args = self.check_args_with_arguments(
                                site, &params, &source, fx, &value.pos, &what, checked, false,
                            );
                            let mut completed = value.clone();
                            match &mut completed.kind {
                                hir::ExprKind::Call { args: target, .. }
                                | hir::ExprKind::New { args: target, .. }
                                | hir::ExprKind::AsyncCall { args: target, .. }
                                | hir::ExprKind::AsyncHandleCreate { args: target, .. } => {
                                    *target = args
                                }
                                _ => {}
                            }
                            completed
                        });
                    *value = completed;
                    if !prefix.is_empty() {
                        prefixes.push((owner, prefix.into_statements()));
                    }
                }
            }
            for frame in &deferred.frame.frames {
                if let Some(id) = frame.lambda_id {
                    let captures = self.deferred_captures.entry(id).or_default();
                    for capture in &frame.captures {
                        if !captures.iter().any(|c| c.name == capture.name) {
                            captures.push(capture.clone());
                        }
                    }
                }
            }
            self.cur_file = file;
            self.subst = subst;
            self.deciding_type = deciding;
        }
        prefixes.extend(self.finish_expression_children(value));
        if let hir::ExprKind::Lambda { id, captures, .. } = &mut value.kind {
            for capture in self.deferred_captures.remove(id).unwrap_or_default() {
                if !captures.iter().any(|c| c.name == capture.name) {
                    captures.push(capture);
                }
            }
        }
        prefixes
    }
}
