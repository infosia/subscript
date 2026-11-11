use super::*;
use crate::check::rejection::RejectionSite;

impl<'p> Checker<'p> {
    /// Renders a type with real class/enum names, for messages.
    pub(crate) fn type_name(&self, ty: &Type) -> String {
        crate::types::display_type(
            ty,
            &|id| {
                self.classes
                    .get(id.0)
                    .map(|c| {
                        // compiler.md §135.1: an opaque type is named by its
                        // type parameter and takes no part in the
                        // disambiguation of class names.
                        let opaque =
                            |index: usize| self.opaque_params.contains_key(&ClassId(index));
                        hir::declaration_label(
                            &c.name,
                            &c.pos,
                            !opaque(id.0)
                                && self.classes.iter().enumerate().any(|(index, other)| {
                                    // compiler.md §140: instances of one template share one declaration.
                                    let declaration = |index| {
                                        self.instance_arguments
                                            .get(&ClassId(index))
                                            .map(|(key, _)| key.as_str())
                                            .unwrap_or_else(|| {
                                                self.classes[index].symbol.full_text()
                                            })
                                    };
                                    other.name == c.name
                                        && !opaque(index)
                                        && declaration(index) != declaration(id.0)
                                }),
                        )
                    })
                    .unwrap_or_else(|| format!("<class #{}>", id.0))
            },
            &|id| {
                self.enums
                    .get(id.0)
                    .map(|e| {
                        hir::declaration_label(
                            &e.name,
                            &e.pos,
                            self.enums
                                .iter()
                                .filter(|other| other.name == e.name)
                                .count()
                                > 1,
                        )
                    })
                    .unwrap_or_else(|| format!("<enum #{}>", id.0))
            },
            &|id| {
                self.string_aliases
                    .get(id.0)
                    .map(|alias| alias.name.clone())
                    .unwrap_or_else(|| format!("<string alias #{}>", id.0))
            },
        )
    }

    pub(crate) fn is_value_class(&self, ty: &Type) -> bool {
        matches!(&self.apparent_type(ty), Type::Class(id) if self.classes[id.0].is_value)
    }

    pub(crate) fn is_reference_class(&self, ty: &Type) -> bool {
        self.apparent_type(ty)
            .uses_reference_identity(&self.type_handle_classes)
    }

    /// The Q24 hash/equality kind of a key, or `None` outside the
    /// whitelist.
    pub(crate) fn assoc_key_kind(&self, ty: &Type) -> Option<hir::AssocKeyKind> {
        hir::AssocKeyKind::of(ty, &|id| {
            self.classes.get(id.0).is_some_and(|class| class.is_value)
        })
    }

    /// Structural assignability under nominal semantics: exact type
    /// equality plus the decided widenings (`null`/`T` into `T | null`,
    /// reference classes into the boundary-opaque `object`).
    pub(crate) fn assignable(&self, from: &Type, to: &Type) -> bool {
        self.assignable_through_constraints(from, to, &mut Vec::new())
    }

    fn assignable_through_constraints(
        &self,
        from: &Type,
        to: &Type,
        seen: &mut Vec<usize>,
    ) -> bool {
        if matches!(from, Type::Error) || matches!(to, Type::Error) {
            return true;
        }
        if matches!((from, to), (Type::TypeParameter(a), Type::TypeParameter(b)) if a.identity == b.identity)
        {
            return true;
        }
        if from == to {
            return true;
        }
        if let Type::GenericUnion(members) = from {
            return members
                .iter()
                .all(|member| self.assignable_through_constraints(member, to, seen));
        }
        if let Type::GenericUnion(members) = to {
            return members
                .iter()
                .any(|member| self.assignable_through_constraints(from, member, seen));
        }
        if self.instance_restriction(opaque::InstanceRestriction::SizedNumeric, from) {
            let mut source = self.apparent_type(from);
            let mut target = to;
            loop {
                match (&source, target) {
                    (Type::Array(a), Type::Array(b)) => {
                        source = self.apparent_type(a);
                        target = b;
                    }
                    (Type::FixedArray(a, n), Type::FixedArray(b, m)) if n == m => {
                        source = self.apparent_type(a);
                        target = b;
                    }
                    _ => break,
                }
            }
            if (source.is_numeric() || matches!(source, Type::GenericNumber | Type::Enum(_)))
                && target.is_numeric()
            {
                return true;
            }
        }
        if from.is_numeric() && matches!(to, Type::GenericNumber) {
            return true;
        }
        if let Type::TypeParameter(parameter) = from {
            if seen.contains(&parameter.identity) {
                return false;
            }
            seen.push(parameter.identity);
            let result = self.direct_constraint(parameter).is_some_and(|constraint| {
                self.assignable_through_constraints(constraint, to, seen)
            });
            seen.pop();
            if result {
                return true;
            }
        }
        if let Type::TypeParameter(_) = to {
            return false;
        }
        // §143 rules 1b and 2a: check component typing before concrete invariance.
        if self.instance_restriction(opaque::InstanceRestriction::CompositeAssignability, from)
            || self.instance_restriction(opaque::InstanceRestriction::CompositeAssignability, to)
        {
            let fits = match (from, to) {
                (Type::Nullable(a, from_spelling), Type::Nullable(_, to_spelling))
                    if from_spelling == to_spelling =>
                {
                    self.assignable_through_constraints(a, to, seen)
                }
                (Type::Array(a), Type::Array(b))
                | (Type::Set(a), Type::Set(b))
                | (Type::Inbox(a), Type::Inbox(b))
                | (Type::Outbox(a), Type::Outbox(b))
                | (Type::Generator(a), Type::Generator(b))
                | (Type::AsyncHandle(a), Type::AsyncHandle(b))
                | (Type::IterResult(a), Type::IterResult(b)) => {
                    self.assignable_through_constraints(a, b, seen)
                }
                (Type::FixedArray(a, n), Type::FixedArray(b, m)) => {
                    n == m && self.assignable_through_constraints(a, b, seen)
                }
                (Type::Map(a, b), Type::Map(c, d)) | (Type::Worker(a, b), Type::Worker(c, d)) => {
                    self.assignable_through_constraints(a, c, seen)
                        && self.assignable_through_constraints(b, d, seen)
                }
                (Type::Func(a), Type::Func(b)) => {
                    a.params.len() == b.params.len()
                        && a.params
                            .iter()
                            .zip(&b.params)
                            .all(|(a, b)| self.assignable_through_constraints(b, a, seen))
                        && self.assignable_through_constraints(&a.ret, &b.ret, seen)
                }
                (Type::Class(a), Type::Class(b)) => {
                    match (
                        self.instance_arguments.get(a),
                        self.instance_arguments.get(b),
                    ) {
                        (Some((a_key, a_args)), Some((b_key, b_args))) => {
                            a_key == b_key
                                && a_args.len() == b_args.len()
                                && a_args
                                    .iter()
                                    .zip(b_args)
                                    .all(|(a, b)| self.assignable_through_constraints(a, b, seen))
                        }
                        _ => false,
                    }
                }
                _ => false,
            };
            if fits {
                return true;
            }
        }
        // §235 rules 2 and 3: a concrete function type, or a nullable
        // pair of them, takes a nullable handle variance. A type parameter
        // keeps the §143 composite rules above.
        if self.function_shaped(from)
            && self.function_shaped(to)
            && !self.involves_type_parameter(from)
            && !self.involves_type_parameter(to)
            && self.type_fits(from, to)
        {
            return true;
        }
        match (from, to) {
            // §195.1 rule 2: `null` converts to the `null` spelling only.
            (Type::Null, Type::Nullable(_, spelling)) => *spelling == crate::types::Absence::Null,
            (f, Type::Nullable(inner, _)) => {
                self.assignable_through_constraints(f, inner, seen)
                    || (self.is_reference_class(f) && **inner == Type::Object)
            }
            (f, Type::Object) => self.is_reference_class(f),
            _ => false,
        }
    }

    /// Whether `ty` is a handle type: a reference class, `string`, an
    /// array, `Map`, `Set`, or a function type (compiler.md §235 rule 1).
    /// A value of `T` and a value of `T | null` have the same
    /// representation for these.
    pub(crate) fn is_handle_type(&self, ty: &Type) -> bool {
        self.is_reference_class(ty)
            || matches!(
                self.apparent_type(ty),
                Type::Str | Type::Array(_) | Type::Func(_)
            )
    }

    /// Whether `s` fits `t` (compiler.md §235 rule 2): the same type, `t`
    /// is `s | null` for a handle type `s`, two function types that rule 3
    /// relates, or `s0 | null` and `t0 | null` where `s0` fits `t0`.
    pub(crate) fn type_fits(&self, s: &Type, t: &Type) -> bool {
        if self.apparent_type(s) == self.apparent_type(t) {
            return true;
        }
        match (self.apparent_type(s), self.apparent_type(t)) {
            (
                Type::Nullable(s0, crate::types::Absence::Null),
                Type::Nullable(t0, crate::types::Absence::Null),
            ) => self.type_fits(&s0, &t0),
            (_, Type::Nullable(inner, crate::types::Absence::Null)) => {
                self.apparent_type(&inner) == self.apparent_type(s) && self.is_handle_type(s)
            }
            (Type::Func(f), Type::Func(g)) => self.function_fits(&f, &g),
            _ => false,
        }
    }

    /// The function type of `ty` through `| null`, if `ty` is a function
    /// type or a nullable function type.
    pub(crate) fn function_of(&self, ty: &Type) -> Option<crate::types::FuncType> {
        match self.apparent_type(ty) {
            Type::Func(function) => Some(*function),
            Type::Nullable(inner, _) => match self.apparent_type(&inner) {
                Type::Func(function) => Some(*function),
                _ => None,
            },
            _ => None,
        }
    }

    fn function_shaped(&self, ty: &Type) -> bool {
        self.function_of(ty).is_some()
    }

    /// Whether the function type `f` is assignable to the function type
    /// `g` (compiler.md §235 rule 3): the same parameter count, the result
    /// of `f` fits the result of `g`, and each parameter of `g` fits the
    /// parameter of `f`. No conversion runs (rule 5).
    pub(crate) fn function_fits(
        &self,
        f: &crate::types::FuncType,
        g: &crate::types::FuncType,
    ) -> bool {
        f.params.len() == g.params.len()
            && self.type_fits(&f.ret, &g.ret)
            && g.params
                .iter()
                .zip(&f.params)
                .all(|(g_param, f_param)| self.type_fits(g_param, f_param))
    }

    /// Whether a value of `from` needs a conversion at a place of `to`:
    /// rule 3 relates two function types that are not identical
    /// (compiler.md §235 rule 5a).
    pub(crate) fn needs_function_conversion(&self, from: &Type, to: &Type) -> bool {
        if self.involves_type_parameter(from) || self.involves_type_parameter(to) {
            return false;
        }
        let (Some(f), Some(g)) = (self.function_of(from), self.function_of(to)) else {
            return false;
        };
        // Two types that fit each other way are the same type, maybe under
        // another spelling, so the place needs no conversion.
        f != g && !self.function_fits(&g, &f) && self.assignable(from, to)
    }

    /// Converts a function value to the type `to` of its place when the
    /// value is assignable to `to` and rule 3 relates two function types
    /// that are not identical (compiler.md §235 rules 5a and 5b). The
    /// conversion is a `Cast` whose type is exactly `to`: each later stage
    /// reads that type, and no operation runs (rule 5). A value that is not
    /// assignable gets no conversion and keeps its diagnostic.
    pub(crate) fn place_function_value(&mut self, value: &mut hir::Expr, to: &Type, fx: &FnCtx) {
        self.discard_function_result(value, to, fx);
        if !self.needs_function_conversion(&value.ty, to) {
            return;
        }
        let inner = std::mem::replace(value, self.err_expr(value.pos.clone()));
        *value = cast_value(inner, to.clone());
    }

    /// The context of the branch `branch` of a conditional (compiler.md
    /// §242 rule 1a). With the place `leaves` of a built-in callback, a
    /// nested conditional takes that place for its own leaves, a lambda
    /// literal takes the place with its own number of parameters, and each
    /// other leaf has no context. With no such place, the branch takes
    /// `ctx`.
    pub(crate) fn leaf_context(
        &mut self,
        branch: &ast::Expr,
        leaves: Option<&CallbackLeaves>,
        ctx: Option<&Type>,
    ) -> Option<Type> {
        let Some(leaves) = leaves else {
            return ctx.cloned();
        };
        match super::expr::unparen_expr(branch) {
            ast::Expr::Cond(_) => {
                self.callback_leaf_place = Some(CallbackLeaves {
                    nested: true,
                    ..leaves.clone()
                });
                None
            }
            ast::Expr::Arrow(arrow) => Some(leaves.place(arrow.params.len())),
            _ => None,
        }
    }

    /// Gives each leaf of the conditional with the branches `then` and
    /// `els` the place of the largest arity of the leaves that the place
    /// allows (compiler.md §242 rule 1a, §244 rule 2). A leaf with fewer
    /// parameters takes that place as in §244 rule 1. A nested conditional
    /// takes the type of the place when its two branches have it, and
    /// gives the diagnostic of no common type when they do not.
    pub(crate) fn place_callback_leaves(
        &mut self,
        then: &mut hir::Expr,
        els: &mut hir::Expr,
        leaves: &CallbackLeaves,
        fx: &FnCtx,
    ) {
        let mut arities = Vec::new();
        for branch in [&*then, &*els] {
            self.leaf_arities(branch, &mut arities);
        }
        let Some(&largest) = arities.iter().max() else {
            return;
        };
        // A leaf with more parameters than the place allows keeps its
        // type, so its conditional has no common type.
        let place = leaves.place(largest.clamp(leaves.params.len(), leaves.params.len() + 1));
        let subject = format!("`{}` callback", leaves.method);
        self.place_leaves(then, &place, &subject, fx);
        self.place_leaves(els, &place, &subject, fx);
    }

    fn leaf_arities(&self, expr: &hir::Expr, out: &mut Vec<usize>) {
        if let hir::ExprKind::Cond { then, els, .. } = &expr.kind {
            self.leaf_arities(then, out);
            self.leaf_arities(els, out);
        } else if let Some(function) = self.function_of(&expr.ty) {
            out.push(function.params.len());
        }
    }

    /// The type of a list of function values with no context (compiler.md
    /// §248 rules 1 to 3): the type of the first value that each other
    /// value fits by the value-taking check of §240 rule 2a. Each other
    /// value then takes that type as its place, as a leaf of §242 rule 1a
    /// does, with its adapter. Returns `None` and changes nothing when a
    /// value is not of a function type, a value involves a type
    /// parameter, or no value has such a type.
    pub(crate) fn place_function_list(
        &mut self,
        values: &mut [&mut hir::Expr],
        fx: &FnCtx,
    ) -> Option<Type> {
        if values.iter().any(|value| {
            !matches!(self.apparent_type(&value.ty), Type::Func(_))
                || self.involves_type_parameter(&value.ty)
        }) {
            return None;
        }
        let mut selected = None;
        for candidate in 0..values.len() {
            let ty = values[candidate].ty.clone();
            let mut each_fits = true;
            for (index, value) in values.iter().enumerate() {
                if index != candidate && !self.leaves_fit(value, &ty, fx) {
                    each_fits = false;
                    break;
                }
            }
            if each_fits {
                selected = Some((candidate, ty));
                break;
            }
        }
        let (selected, ty) = selected?;
        for (index, value) in values.iter_mut().enumerate() {
            if index != selected {
                self.place_leaves(value, &ty, "function value", fx);
            }
        }
        Some(ty)
    }

    /// Whether each leaf of `value` fits `to` by the value-taking check of
    /// compiler.md §240 rule 2a. The check runs on a copy, so `value` and
    /// the lambda numbering stay unchanged.
    fn leaves_fit(&mut self, value: &hir::Expr, to: &Type, fx: &FnCtx) -> bool {
        if let hir::ExprKind::Cond { then, els, .. } = &value.kind {
            return self.leaves_fit(then, to, fx) && self.leaves_fit(els, to, fx);
        }
        let next_lambda_id = self.next_lambda_id;
        let fits = self.place_value(&mut value.clone(), to, fx);
        self.next_lambda_id = next_lambda_id;
        fits
    }

    /// Places each leaf of `expr` at `place`. A leaf that drops a handle
    /// gets the diagnostic of compiler.md §247 rule 1 after `subject`, as
    /// the callback itself gets it.
    fn place_leaves(&mut self, expr: &mut hir::Expr, place: &Type, subject: &str, fx: &FnCtx) {
        if let hir::ExprKind::Cond { then, els, .. } = &mut expr.kind {
            self.place_leaves(then, place, subject, fx);
            self.place_leaves(els, place, subject, fx);
            if self.apparent_type(&then.ty) == Type::Error
                || self.apparent_type(&els.ty) == Type::Error
            {
                expr.ty = Type::Error;
            } else if self.apparent_type(&then.ty) == self.apparent_type(&els.ty) {
                expr.ty = then.ty.clone();
            } else {
                let message = format!(
                    "conditional branches have no common type: `{}` and `{}`",
                    self.type_name(&then.ty),
                    self.type_name(&els.ty),
                );
                self.reject_subset(
                    RejectionSite::ConditionalJoinGeneralUnionAndUndefined,
                    message,
                    expr.pos.clone(),
                );
                expr.ty = Type::Error;
            }
            return;
        }
        if self.place_value(expr, place, fx) {
            return;
        }
        // compiler.md §247 rule 3: the leaf that holds the handle names it.
        if let Some(handle) = self.dropped_handle(expr, place, fx) {
            let message = self.dropped_handle_message(expr, &handle, place, subject);
            self.reject_subset(
                RejectionSite::VoidPlaceDropsAsyncHandle,
                message,
                expr.pos.clone(),
            );
            expr.ty = Type::Error;
        }
    }

    /// Whether a call of a place with the result `void` can discard the
    /// result `ret` (compiler.md §240 rule 1, §242 rules 2 and 5): a value
    /// that is not `void` and holds no handle at any depth, by the
    /// predicate of the drop check (§188.1 rule 1). A discarded handle has
    /// no holder that awaits it.
    fn result_can_be_discarded(&self, ret: &Type) -> bool {
        let ret = self.apparent_type(ret);
        !matches!(ret, Type::Void | Type::Error) && !ret.holds_async_handle()
    }

    /// A lambda literal with a value result, in a place of a function type
    /// with the result `void` (compiler.md §242 rules 2 and 3). If the
    /// lambda is not async, its result is not `void` and carries no handle,
    /// and the lambda with the result `void` fits the place, the lambda gets
    /// the result `void`: each `return e;` of its body becomes `e; return;`.
    /// The body was checked against the annotation before. Returns whether
    /// the lambda changed.
    fn discard_lambda_result(&mut self, value: &mut hir::Expr, to: &Type) -> bool {
        let hir::ExprKind::Lambda {
            is_async: false,
            ret,
            body,
            ..
        } = &mut value.kind
        else {
            return false;
        };
        let Some(place) = self.function_of(to) else {
            return false;
        };
        if self.apparent_type(&place.ret) != Type::Void || !self.result_can_be_discarded(ret) {
            return false;
        }
        let Some(own) = self.function_of(&value.ty) else {
            return false;
        };
        let discarding = crate::types::FuncType {
            params: own.params.clone(),
            ret: Type::Void,
        };
        if !self.function_fits(&discarding, &place) {
            return false;
        }
        *ret = Type::Void;
        discard_return_values(body);
        value.ty = Type::func(own.params, Type::Void);
        true
    }

    /// Converts a function value to its place (compiler.md §240 rule 2,
    /// §242 rule 2, §243 rules 1 and 2). A lambda literal with fewer
    /// parameters takes the trailing parameters of the place, then a lambda
    /// literal with a value result takes a `void` result. A reference to a
    /// function declaration or a static method becomes a lambda with no
    /// environment that calls it, when the place has more parameters or a
    /// `void` result that discards the value. Another value is unchanged.
    pub(crate) fn discard_function_result(&mut self, value: &mut hir::Expr, to: &Type, fx: &FnCtx) {
        self.pad_lambda_parameters(value, to);
        if self.discard_lambda_result(value, to) {
            return;
        }
        self.adapt_function_reference(value, to, fx);
    }

    /// Whether the first parameters of the place `place` fit the parameters
    /// `params`, and a value of the result `ret` fits the result of the
    /// place, or the place discards it (compiler.md §243 rules 1 and 2).
    fn prefix_fits(&self, params: &[Type], ret: &Type, place: &crate::types::FuncType) -> bool {
        self.prefix_parameters_fit(params, place) && self.result_fits(ret, place)
    }

    /// Whether the first parameters of the place `place` fit the parameters
    /// `params` (compiler.md §243 rules 1 and 2).
    fn prefix_parameters_fit(&self, params: &[Type], place: &crate::types::FuncType) -> bool {
        params.len() <= place.params.len()
            && place
                .params
                .iter()
                .zip(params)
                .all(|(place_param, param)| self.type_fits(place_param, param))
    }

    /// Whether a value of the result `ret` fits the result of the place
    /// `place`, or the place discards it (compiler.md §240 rule 1, §242
    /// rule 5).
    fn result_fits(&self, ret: &Type, place: &crate::types::FuncType) -> bool {
        self.type_fits(ret, &place.ret)
            || (self.apparent_type(&place.ret) == Type::Void && self.result_can_be_discarded(ret))
    }

    /// A lambda literal with `N` parameters, in a place of a function type
    /// with `M > N` parameters, takes the `M - N` trailing parameters of
    /// the place (compiler.md §243 rule 1). The added parameters have no
    /// name in the source, and the body does not read them.
    fn pad_lambda_parameters(&mut self, value: &mut hir::Expr, to: &Type) {
        let (Some(place), Some(own)) = (self.function_of(to), self.function_of(&value.ty)) else {
            return;
        };
        let hir::ExprKind::Lambda { id, params, .. } = &mut value.kind else {
            return;
        };
        if own.params.len() >= place.params.len()
            || !self.prefix_fits(&own.params, &own.ret, &place)
        {
            return;
        }
        let pos = value.pos.clone();
        let mut padded = own.params.clone();
        for (index, ty) in place.params.iter().enumerate().skip(own.params.len()) {
            params.push(synthetic_param(
                format!("[[pad#{}.{index}]]", id.0),
                ty,
                &pos,
            ));
            padded.push(ty.clone());
        }
        // The lambda keeps its own parameter types; a place of other types
        // that fit adds the cast of §235 rule 5a after this.
        value.ty = Type::func(padded, own.ret);
    }

    /// The callee of an adapter (compiler.md §240 rule 2, §243 rule 2,
    /// §246 rules 1 and 2): a function declaration or a static method, a
    /// const local of a function type, or a module-level `const` of a
    /// function type. A const local is captured by value, as a lambda
    /// written in the source captures it. Another value has no adapter.
    fn adapter_callee(&self, value: &hir::Expr, fx: &FnCtx) -> Option<AdapterCallee> {
        let value_callee = |function: crate::types::FuncType,
                            captures: Vec<hir::Capture>,
                            has_default: bool| AdapterCallee {
            callee: hir::Callee::Value(Box::new(value.clone())),
            refused: false,
            is_async: matches!(self.apparent_type(&function.ret), Type::AsyncHandle(_)),
            own: function.params,
            ret: function.ret,
            captures,
            has_default,
        };
        match &value.kind {
            hir::ExprKind::FuncRef(symbol) => {
                let sig = self.fn_sigs.get(symbol.full_text())?;
                Some(AdapterCallee {
                    callee: hir::Callee::Func(symbol.clone()),
                    own: sig.params.iter().map(|p| p.ty().clone()).collect(),
                    ret: sig.ret.clone(),
                    captures: Vec::new(),
                    refused: sig.generic || sig.is_generator,
                    is_async: sig.is_async,
                    has_default: sig.params.iter().any(|p| p.has_default),
                })
            }
            hir::ExprKind::Local(name, ..) => {
                let local = fx
                    .scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.vars.get(name).cloned())?;
                if local.mutable || local.caught {
                    return None;
                }
                let Type::Func(function) = self.apparent_type(&value.ty) else {
                    return None;
                };
                // The capture of a written lambda (compiler.md §246 rule 3).
                Some(value_callee(
                    *function,
                    vec![super::lookup::local_capture(name, &local)],
                    local.function_value_required.is_some(),
                ))
            }
            hir::ExprKind::Global(symbol) => {
                let sig = self.global_sigs.get(symbol.full_text())?;
                if sig.mutable {
                    return None;
                }
                let has_default = sig.function_value_required.is_some();
                let Type::Func(function) = self.apparent_type(&value.ty) else {
                    return None;
                };
                Some(value_callee(*function, Vec::new(), has_default))
            }
            _ => None,
        }
    }

    /// Whether `value` is a function value that is not a name of
    /// compiler.md §246 rules 1 and 2, and that rule 1 would take if it were
    /// a const local name (§246 rule 5). The decision is the one of rule 1:
    /// the same callee facts, the same fit of the adapter, and the
    /// value-taking check of the adapted type against the place.
    pub(crate) fn needs_adapter_name(&self, value: &hir::Expr, to: &Type, fx: &FnCtx) -> bool {
        if matches!(
            value.kind,
            hir::ExprKind::FuncRef(_) | hir::ExprKind::Lambda { .. } | hir::ExprKind::Cond { .. }
        ) || self.adapter_callee(value, fx).is_some()
        {
            return false;
        }
        let Some(target) = self.callee_facts(value, fx) else {
            return false;
        };
        self.function_of(to)
            .and_then(|place| self.adapter_shape(&target, &place, false))
            .is_some_and(|shape| self.assignable(&shape.ty, to))
    }

    /// The facts of `value` as the callee of an adapter: those of
    /// [`Self::adapter_callee`], or, for another value of a function type,
    /// those of a const local name with its type (compiler.md §246 rule 5,
    /// §247 rule 1).
    fn callee_facts(&self, value: &hir::Expr, fx: &FnCtx) -> Option<AdapterCallee> {
        if let Some(callee) = self.adapter_callee(value, fx) {
            return Some(callee);
        }
        let Type::Func(function) = self.apparent_type(&value.ty) else {
            return None;
        };
        Some(AdapterCallee {
            callee: hir::Callee::Value(Box::new(value.clone())),
            refused: false,
            is_async: matches!(self.apparent_type(&function.ret), Type::AsyncHandle(_)),
            own: function.params,
            ret: function.ret,
            captures: Vec::new(),
            has_default: false,
        })
    }

    /// The result of `value` when the place `to` drops it and the result
    /// holds an async handle (compiler.md §247 rule 1): the place is a
    /// function type with the result `void`, and the value fits it with the
    /// adapter of §240 rule 2, §243, or §246, except for its result. The
    /// decision is the one of the adapter: [`Self::adapter_shape`] with the
    /// result that the place drops, and the adapted type fits the place. A
    /// result that contains the error type has its diagnostic already. A
    /// conditional is not a value here; each of its leaves is (§247 rule
    /// 3).
    pub(crate) fn dropped_handle(&self, value: &hir::Expr, to: &Type, fx: &FnCtx) -> Option<Type> {
        if matches!(value.kind, hir::ExprKind::Cond { .. }) {
            return None;
        }
        let place = self.function_of(to)?;
        if self.apparent_type(&place.ret) != Type::Void {
            return None;
        }
        // The result of the value is the one of its type: the signature of
        // an async function records the awaited result.
        let Type::Func(function) = self.apparent_type(&value.ty) else {
            return None;
        };
        let ret = self.apparent_type(&function.ret);
        if !ret.holds_async_handle() || super::interface::poisoned(&ret) {
            return None;
        }
        let target = AdapterCallee {
            ret: function.ret.clone(),
            ..self.callee_facts(value, fx)?
        };
        self.adapter_shape(&target, &place, true)
            .filter(|shape| self.assignable(&shape.ty, to))
            .map(|_| function.ret)
    }

    /// The text of the diagnostic of compiler.md §247 rule 2 for `value`
    /// with the result `handle` in the place `to`, after `subject`, such as
    /// "callback".
    pub(crate) fn dropped_handle_message(
        &self,
        value: &hir::Expr,
        handle: &Type,
        to: &Type,
        subject: &str,
    ) -> String {
        let name = match &value.kind {
            hir::ExprKind::FuncRef(symbol) => Some(hir::source_name(symbol.full_text())),
            hir::ExprKind::Local(name, ..) => Some(name.clone()),
            hir::ExprKind::Global(symbol) => Some(symbol.source_name()),
            _ => None,
        };
        let subject = match name {
            Some(name) => format!("{subject} `{name}`"),
            None => subject.to_string(),
        };
        let handle = self.type_name(handle);
        format!(
            "{subject} returns `{handle}`, which the place `{}` drops, and no holder awaits the handle; a place with the result `{handle}` can await it",
            self.type_name(to)
        )
    }

    /// The shape of the adapter of `target` in the place `place`, when the
    /// place needs one and `target` takes it (compiler.md §240 rule 2,
    /// §243 rule 2, §246 rule 1): the place has more parameters, or the
    /// result `void` and the value a value result to discard. The value is
    /// not refused, has no default parameter when the place has more
    /// parameters, and its parameters and result fit the place. With
    /// `drops_handle`, the result of the value holds an async handle that
    /// the `void` place drops (compiler.md §247 rule 1): the shape is the
    /// one that the adapter has if the handle were a value to discard, so
    /// an async value is not refused and the result fits.
    fn adapter_shape(
        &self,
        target: &AdapterCallee,
        place: &crate::types::FuncType,
        drops_handle: bool,
    ) -> Option<AdapterShape> {
        let fewer = target.own.len() < place.params.len();
        let place_void = self.apparent_type(&place.ret) == Type::Void;
        let discards = place_void && self.apparent_type(&target.ret) != Type::Void;
        let result_fits = if drops_handle {
            place_void
        } else {
            self.result_fits(&target.ret, place)
        };
        if target.refused
            || (target.is_async && !drops_handle)
            || (fewer && target.has_default)
            || (!fewer && !discards)
            || !self.prefix_parameters_fit(&target.own, place)
            || !result_fits
        {
            return None;
        }
        let ret = if discards {
            Type::Void
        } else {
            target.ret.clone()
        };
        let mut params = target.own.clone();
        params.extend(place.params.iter().skip(target.own.len()).cloned());
        Some(AdapterShape {
            ty: Type::func(params.clone(), ret.clone()),
            params,
            ret,
            discards,
        })
    }

    /// The text of the diagnostic of compiler.md §246 rule 5 for `value`,
    /// after `subject`, such as "callback".
    pub(crate) fn adapter_name_message(&self, value: &hir::Expr, subject: &str) -> String {
        let name = match &value.kind {
            hir::ExprKind::Local(name, ..) => Some(name.clone()),
            hir::ExprKind::Global(symbol) => Some(symbol.source_name()),
            _ => None,
        };
        let subject = match name {
            Some(name) => format!("{subject} `{name}`"),
            None => subject.to_string(),
        };
        format!(
            "{subject} needs an adapter for this place, and only a const local or a module `const` takes one; copy it into a const local, or write a lambda that calls it"
        )
    }

    /// A function value in a place of a function type (compiler.md §240
    /// rules 1 and 2, §243 rule 2, §246 rules 1 and 2). A reference to a
    /// function declaration, a static method, a const local, or a module
    /// `const` becomes a lambda with the parameters of the place, which
    /// calls the value with its own number of arguments, when the place
    /// has more parameters, or when the place has the result `void` and the
    /// value a value result to discard. A const local is captured by value.
    /// The value is not generic, not async, and not a generator. With more
    /// parameters in the place, it has no default parameter. Another value
    /// is unchanged.
    fn adapt_function_reference(&mut self, value: &mut hir::Expr, to: &Type, fx: &FnCtx) {
        let Some(place) = self.function_of(to) else {
            return;
        };
        let Some(target) = self.adapter_callee(value, fx) else {
            return;
        };
        let Some(AdapterShape {
            params,
            ret,
            discards,
            ..
        }) = self
            .adapter_shape(&target, &place, false)
            .filter(|shape| self.assignable(&shape.ty, to))
        else {
            return;
        };
        let own = target.own;
        let call_ret = target.ret;
        let pos = value.pos.clone();
        let names: Vec<String> = (0..params.len())
            .map(|index| format!("[[discard#{}.{index}]]", self.next_lambda_id))
            .collect();
        let call = hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::Call {
                callee: target.callee,
                args: names
                    .iter()
                    .zip(&own)
                    .map(|(name, ty)| hir::Expr {
                        pending_work: None,
                        kind: hir::ExprKind::Local(name.clone(), ty.clone(), false),
                        ty: ty.clone(),
                        pos: pos.clone(),
                    })
                    .collect(),
            },
            ty: call_ret,
            pos: pos.clone(),
        };
        let body = if discards || self.apparent_type(&ret) == Type::Void {
            vec![hir::Stmt::Expr(call)]
        } else {
            vec![hir::Stmt::Return {
                value: Some(call),
                pos: pos.clone(),
            }]
        };
        let id = hir::LambdaId(self.next_lambda_id);
        self.next_lambda_id += 1;
        *value = hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::Lambda {
                id,
                params: names
                    .iter()
                    .zip(&params)
                    .map(|(name, ty)| synthetic_param(name.clone(), ty, &pos))
                    .collect(),
                is_async: false,
                owns_environment: false,
                ret: ret.clone(),
                body,
                captures: target.captures,
                can_raise: false,
            },
            ty: Type::func(params, ret),
            pos,
        };
    }

    /// The check of a value against a place, with no diagnostic (compiler.md
    /// §240 rule 2a): the value takes the conversion of its place
    /// ([`Self::place_function_value`]), then its type is checked. Returns
    /// whether the value fits the place. A value that does not fit is
    /// unchanged.
    pub(crate) fn place_value(&mut self, value: &mut hir::Expr, to: &Type, fx: &FnCtx) -> bool {
        self.place_function_value(value, to, fx);
        self.assignable(&value.ty, to)
    }

    /// Converts an accepted built-in callback to the function type of its
    /// place: the parameters that the method passes (with the trailing
    /// index when the callback takes it), and the result type `ret`, or
    /// the result of the callback when the method reads it from the
    /// callback (compiler.md §235 rule 5a).
    pub(crate) fn place_callback_value(
        &mut self,
        mut callback: hir::Expr,
        params: &[Type],
        ret: Option<&Type>,
        fx: &FnCtx,
    ) -> hir::Expr {
        let Type::Func(own) = self.apparent_type(&callback.ty) else {
            return callback;
        };
        let mut place_params = params.to_vec();
        if own.params.len() == params.len() + 1 {
            place_params.push(Type::I32);
        }
        let place = Type::func(place_params, ret.cloned().unwrap_or(own.ret));
        self.place_function_value(&mut callback, &place, fx);
        callback
    }

    pub(super) fn contains_string_alias(ty: &Type) -> bool {
        match ty {
            Type::StringAlias(_) => true,
            Type::FixedArray(element, _)
            | Type::Array(element)
            | Type::Set(element)
            | Type::Nullable(element, _)
            | Type::Generator(element)
            | Type::IterResult(element) => Self::contains_string_alias(element),
            Type::Map(key, value) => {
                Self::contains_string_alias(key) || Self::contains_string_alias(value)
            }
            Type::Func(function) => {
                function.params.iter().any(Self::contains_string_alias)
                    || Self::contains_string_alias(&function.ret)
            }
            _ => false,
        }
    }

    /// The §52 boundary spellings whose storage is exactly one wire value,
    /// or a zero-copy descriptor of wire-value elements.
    pub(super) fn supported_wire_alias_boundary_type(ty: &Type) -> bool {
        match ty {
            Type::StringAlias(_) => true,
            Type::Array(element) => matches!(&**element, Type::StringAlias(_)),
            _ => false,
        }
    }

    /// Whether `from` and `to` are nullable types of different absence
    /// spellings, and `from` converts to `to` under the spelling of `to`
    /// (compiler.md §195.1 rule 2).
    pub(crate) fn converts_across_spellings(&self, from: &Type, to: &Type) -> bool {
        match (self.apparent_type(from), self.apparent_type(to)) {
            (Type::Nullable(inner, from_spelling), Type::Nullable(_, to_spelling))
                if from_spelling != to_spelling =>
            {
                self.assignable(&Type::nullable_spelled(*inner, to_spelling), to)
            }
            _ => false,
        }
    }

    /// Emits the rule-specific diagnostic for a failed assignment of a
    /// type where no value flows. Such a check cannot record a conversion,
    /// so it accepts no function type that needs one (compiler.md §235
    /// rule 5d): a value check uses [`Self::require_expr_assignable`].
    pub(crate) fn require_assignable(&mut self, from: &Type, to: &Type, pos: Pos, what: &str) {
        if self.assignable(from, to) && !self.needs_function_conversion(from, to) {
            return;
        }
        self.report_not_assignable(from, to, pos, what);
    }

    /// Checks a return value against the result type. An async return of a
    /// handle adopts it before this check (compiler.md §236), so a handle
    /// that reaches this check is a type mismatch.
    pub(super) fn require_return_assignable(
        &mut self,
        from: &mut hir::Expr,
        to: &Type,
        fx: &FnCtx,
    ) {
        self.require_expr_assignable(from, to, fx, "the return value");
    }

    /// The fix that a rejection of a `string | null` operand states
    /// (§218.1 rule 5), or nothing for other operands.
    pub(super) fn null_test_fix(&self, operands: &[&Type]) -> &'static str {
        let nullable_string = operands.iter().any(
            |ty| matches!(self.apparent_type(ty), Type::Nullable(inner, _) if self.apparent_type(&inner) == Type::Str),
        );
        if nullable_string {
            "; a null test first gives a `string`"
        } else {
            ""
        }
    }

    /// Keeps the C17 classification at an assignment rejection.
    pub(super) fn require_expr_assignable(
        &mut self,
        from: &mut hir::Expr,
        to: &Type,
        fx: &FnCtx,
        what: &str,
    ) {
        if !self.place_value(from, to, fx) {
            // compiler.md §247 rule 1: a value that fits except for the
            // handle that the place drops names the handle.
            if let Some(handle) = self.dropped_handle(from, to, fx) {
                let message = self.dropped_handle_message(from, &handle, to, "function value");
                self.reject_subset(
                    RejectionSite::VoidPlaceDropsAsyncHandle,
                    message,
                    from.pos.clone(),
                );
                return;
            }
            // compiler.md §246 rule 5: a value that fits with an adapter
            // names the cause.
            if self.needs_adapter_name(from, to, fx) {
                let message = self.adapter_name_message(from, "function value");
                self.reject_subset(
                    RejectionSite::FunctionValueAdapterNeedsConstName,
                    message,
                    from.pos.clone(),
                );
                return;
            }
            if matches!(self.apparent_type(&from.ty), Type::Nullable(inner, _) if *inner == self.apparent_type(to))
                && self.indexed_nullable_error(
                    from,
                    fx,
                    if matches!(self.apparent_type(to), Type::Func(_)) {
                        RejectionSite::NullableCall
                    } else {
                        RejectionSite::NullableNominalAssignment
                    },
                    from.pos.clone(),
                )
            {
                return;
            }
            if matches!(self.apparent_type(&from.ty), Type::Nullable(inner, _) if *inner == self.apparent_type(to))
                && super::expr::path_key(from)
                    .is_some_and(|key| fx.ended_shared_narrowing.contains(&key))
            {
                let nominal = matches!(
                    self.apparent_type(to),
                    Type::Class(_) | Type::Map(..) | Type::Set(_)
                );
                let (site, reason) = if nominal {
                    (
                        RejectionSite::NullableNominalAssignmentShared,
                        "nominal types are not interchangeable",
                    )
                } else {
                    (RejectionSite::NullableAssignmentShared, "type mismatch")
                };
                self.reject_subset(
                    site,
                    format!(
                        "{}: {} expects `{}`, got `{}`",
                        reason,
                        what,
                        self.type_name(to),
                        self.type_name(&from.ty)
                    ),
                    from.pos.clone(),
                );
                return;
            }
            self.report_not_assignable(&from.ty, to, from.pos.clone(), what);
        }
    }

    fn report_not_assignable(&mut self, from: &Type, to: &Type, pos: Pos, what: &str) {
        let from_n = self.type_name(from);
        let to_n = self.type_name(to);
        if self.converts_across_spellings(from, to) {
            self.reject_subset(
                RejectionSite::ConversionAcrossSpellings,
                format!(
                    "`T | undefined` and `T | null` do not convert to each other: {} expects `{}`, got `{}`",
                    what, to_n, from_n
                ),
                pos,
            );
            return;
        }
        let class_like = |t: &Type| match t {
            Type::Class(_) | Type::Map(..) | Type::Set(_) => true,
            Type::Nullable(inner, _) => {
                matches!(**inner, Type::Class(_) | Type::Map(..) | Type::Set(_))
            }
            _ => false,
        };
        let from_nominal = match from {
            Type::Nullable(inner, _) => inner.as_ref(),
            other => other,
        };
        let to_nominal = match to {
            Type::Nullable(inner, _) => inner.as_ref(),
            other => other,
        };
        if class_like(from) && class_like(to) {
            let message = format!(
                "nominal types are not interchangeable: {} expects `{}`, got `{}`",
                what, to_n, from_n
            );
            if matches!(&self.apparent_type(from), Type::Nullable(inner, _) if **inner == self.apparent_type(to))
            {
                self.reject_subset(RejectionSite::NullableNominalAssignment, message, pos);
            } else if !self.ts_nominal_assignable(from, to) {
                self.reject_subset(RejectionSite::IncompatibleNominalAssignment, message, pos);
            } else if matches!((from_nominal, to_nominal), (Type::Class(a), Type::Class(b)) if self.instance_arguments.get(a).zip(self.instance_arguments.get(b)).is_some_and(|((a, _), (b, _))| a == b))
            {
                self.reject_subset(RejectionSite::ErasedNominalTypeArguments, message, pos);
            } else if matches!((from_nominal, to_nominal), (Type::Class(_), Type::Class(_))) {
                self.reject_subset(RejectionSite::DistinctNominalClassAssignment, message, pos);
            } else {
                self.reject_subset(
                    RejectionSite::DistinctNominalContainerAssignment,
                    message,
                    pos,
                );
            }
        } else if from.is_numeric() && to.is_numeric() {
            self.reject_subset(
                RejectionSite::ImplicitNumericAssignment,
                format!(
                    "implicit numeric conversion from `{}` to `{}`; spell it `as {}`",
                    from_n, to_n, to_n
                ),
                pos,
            );
        } else if self.is_value_class(from) && matches!(to, Type::Nullable(_, _)) {
            self.reject_subset(
                RejectionSite::NullableValueClassAssignment,
                format!("value class `{}` cannot be nullable", from_n),
                pos,
            );
        } else {
            let fix = if self.apparent_type(to) == Type::Str {
                self.null_test_fix(&[from])
            } else {
                ""
            };
            let message = format!(
                "type mismatch: {} expects `{}`, got `{}`{}",
                what, to_n, from_n, fix
            );
            if matches!((from, to), (Type::StringAlias(_), Type::StringAlias(_))) {
                let compatible = match (&self.apparent_type(from), &self.apparent_type(to)) {
                    (Type::StringAlias(a), Type::StringAlias(b)) => self.string_aliases[a.0]
                        .members
                        .iter()
                        .all(|member| self.string_aliases[b.0].members.contains(member)),
                    _ => false,
                };
                self.reject_subset(
                    if compatible {
                        RejectionSite::AssignmentLiteralAlias
                    } else {
                        RejectionSite::DisjointLiteralAliasAssignment
                    },
                    message,
                    pos,
                );
            } else {
                self.reject_subset(
                    match (&self.apparent_type(from), &self.apparent_type(to)) {
                        (Type::StringAlias(_), Type::Str) => RejectionSite::LiteralAliasToString,
                        (Type::Enum(_), integer) if self.apparent_type(integer).is_integer() => {
                            RejectionSite::EnumToInteger
                        }
                        (Type::Array(_), Type::FixedArray(..)) => RejectionSite::ArrayToFixedArray,
                        (Type::Func(_), Type::Func(_)) if self.ts_erased_assignable(from, to) => {
                            RejectionSite::FunctionParameterIdentity
                        }
                        _ if self.ts_erased_assignable(from, to) => {
                            RejectionSite::ErasedAssignableTypeMismatch
                        }
                        _ if matches!(
                            from_nominal,
                            Type::Array(_)
                                | Type::FixedArray(..)
                                | Type::Func(_)
                                | Type::Generator(_)
                                | Type::AsyncHandle(_)
                                | Type::IterResult(_)
                                | Type::Worker(..)
                                | Type::Inbox(_)
                                | Type::Outbox(_)
                        ) && self.ts_nominal_assignable(from, to) =>
                        {
                            RejectionSite::NestedNominalClass
                        }
                        _ => RejectionSite::AssignmentTypeMismatch,
                    },
                    message,
                    pos,
                );
            }
        }
    }
}

/// A `Cast` of `value` to `ty`, the conversion node of compiler.md §235
/// rule 5a.
fn cast_value(value: hir::Expr, ty: Type) -> hir::Expr {
    let pos = value.pos.clone();
    hir::Expr {
        pending_work: None,
        kind: hir::ExprKind::Cast(Box::new(value)),
        ty,
        pos,
    }
}

/// Makes each `return e;` of `body` into `e; return;` (compiler.md §242
/// rule 3). A lambda in an expression is a separate function, so the walk
/// does not enter expressions.
fn discard_return_values(body: &mut Vec<hir::Stmt>) {
    let mut index = 0;
    while index < body.len() {
        match &mut body[index] {
            hir::Stmt::Return { value, pos } => {
                if let Some(value) = value.take() {
                    let pos = pos.clone();
                    body[index] = hir::Stmt::Expr(value);
                    body.insert(index + 1, hir::Stmt::Return { value: None, pos });
                    index += 1;
                }
            }
            hir::Stmt::If { then, els, .. } => {
                discard_return_values(then);
                if let Some(els) = els {
                    discard_return_values(els);
                }
            }
            hir::Stmt::While { body, .. }
            | hir::Stmt::DoWhile { body, .. }
            | hir::Stmt::ForOf { body, .. }
            | hir::Stmt::GeneratorForOf { body, .. }
            | hir::Stmt::Block(body) => discard_return_values(body),
            hir::Stmt::For { body, step, .. } => {
                discard_return_values(body);
                discard_return_values(step);
            }
            hir::Stmt::Switch { cases, .. } => {
                for case in cases {
                    discard_return_values(&mut case.body);
                }
            }
            hir::Stmt::Try { body, handler, .. } => {
                discard_return_values(body);
                discard_return_values(handler);
            }
            hir::Stmt::Using {
                body, finalizer, ..
            } => {
                discard_return_values(body);
                if let Some(finalizer) = finalizer {
                    discard_return_values(finalizer);
                }
            }
            hir::Stmt::Let { .. }
            | hir::Stmt::Expr(_)
            | hir::Stmt::Break(_)
            | hir::Stmt::Continue(_)
            | hir::Stmt::Throw { .. } => {}
        }
        index += 1;
    }
}

/// The place of a built-in callback with an optional trailing index, for
/// the leaves of a conditional (compiler.md §242 rule 1a, §244 rule 2).
#[derive(Debug, Clone)]
pub(crate) struct CallbackLeaves {
    /// The parameters of the place without the index.
    pub(crate) params: Vec<Type>,
    /// The result of the place.
    pub(crate) ret: Type,
    /// The method that takes the callback, as its diagnostics name it.
    pub(crate) method: String,
    /// The conditional is a branch of another conditional, which places
    /// the leaves of both.
    pub(crate) nested: bool,
}

impl CallbackLeaves {
    /// The place for a leaf with `arity` parameters: the place with the
    /// trailing index when the leaf takes one parameter more.
    fn place(&self, arity: usize) -> Type {
        let mut place = self.params.clone();
        if arity == self.params.len() + 1 {
            place.push(Type::I32);
        }
        Type::func(place, self.ret.clone())
    }
}

/// A parameter of a lambda that the checker makes, with no default.
fn synthetic_param(name: String, ty: &Type, pos: &Pos) -> hir::Param {
    hir::Param {
        escapes: false,
        default_can_raise: false,
        name,
        ty: ty.clone(),
        default: None,
        foreign_provenance: None,
        pos: pos.clone(),
    }
}

/// The callee of an adapter and the facts that decide it (compiler.md
/// §246 rules 1, 2, and 4).
struct AdapterCallee {
    callee: hir::Callee,
    own: Vec<Type>,
    ret: Type,
    /// The captured const local, as a lambda written in the source has.
    captures: Vec<hir::Capture>,
    /// A generic or generator value takes no adapter.
    refused: bool,
    /// An async function, or a value whose result is an async handle,
    /// takes no adapter. Its handle result names the diagnostic of
    /// compiler.md §247.
    is_async: bool,
    has_default: bool,
}

/// The parameters, the result, and the type of an adapter (compiler.md
/// §246 rule 1).
struct AdapterShape {
    params: Vec<Type>,
    ret: Type,
    ty: Type,
    discards: bool,
}
