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
    fn function_of(&self, ty: &Type) -> Option<crate::types::FuncType> {
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
    pub(crate) fn place_function_value(&mut self, value: &mut hir::Expr, to: &Type) {
        self.discard_function_result(value, to);
        if !self.needs_function_conversion(&value.ty, to) {
            return;
        }
        let inner = std::mem::replace(value, self.err_expr(value.pos.clone()));
        *value = cast_value(inner, to.clone());
    }

    /// A reference to a function declaration or a static method, in a place
    /// of a function type with the result `void` (compiler.md §240 rules 1
    /// and 2). If the function is not generic and not async, its result is
    /// not `void` and not a handle, and the function with the result `void`
    /// fits the place, `value` becomes a lambda with no environment and the
    /// parameters of the function. The lambda calls the function with its
    /// arguments and discards the result. Another value is unchanged.
    pub(crate) fn discard_function_result(&mut self, value: &mut hir::Expr, to: &Type) {
        let hir::ExprKind::FuncRef(symbol) = &value.kind else {
            return;
        };
        let Some(place) = self.function_of(to) else {
            return;
        };
        if self.apparent_type(&place.ret) != Type::Void {
            return;
        }
        let Some(sig) = self.fn_sigs.get(symbol.full_text()) else {
            return;
        };
        if sig.generic
            || sig.is_async
            || sig.is_generator
            || matches!(
                self.apparent_type(&sig.ret),
                Type::Void | Type::AsyncHandle(_) | Type::Error
            )
        {
            return;
        }
        let params: Vec<Type> = sig.params.iter().map(|p| p.ty().clone()).collect();
        let ret = sig.ret.clone();
        let discarding = crate::types::FuncType {
            params: params.clone(),
            ret: Type::Void,
        };
        if !self.function_fits(&discarding, &place) {
            return;
        }
        let pos = value.pos.clone();
        let names: Vec<String> = (0..params.len())
            .map(|index| format!("[[discard#{}.{index}]]", self.next_lambda_id))
            .collect();
        let call = hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::Call {
                callee: hir::Callee::Func(symbol.clone()),
                args: names
                    .iter()
                    .zip(&params)
                    .map(|(name, ty)| hir::Expr {
                        pending_work: None,
                        kind: hir::ExprKind::Local(name.clone(), ty.clone(), false),
                        ty: ty.clone(),
                        pos: pos.clone(),
                    })
                    .collect(),
            },
            ty: ret,
            pos: pos.clone(),
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
                    .map(|(name, ty)| hir::Param {
                        escapes: false,
                        default_can_raise: false,
                        name: name.clone(),
                        ty: ty.clone(),
                        default: None,
                        foreign_provenance: None,
                        pos: pos.clone(),
                    })
                    .collect(),
                is_async: false,
                owns_environment: false,
                ret: Type::Void,
                body: vec![hir::Stmt::Expr(call)],
                captures: Vec::new(),
                can_raise: false,
            },
            ty: Type::func(params, Type::Void),
            pos,
        };
    }

    /// The check of a value against a place, with no diagnostic (compiler.md
    /// §240 rule 2a): the value takes the conversion of its place
    /// ([`Self::place_function_value`]), then its type is checked. Returns
    /// whether the value fits the place. A value that does not fit is
    /// unchanged.
    pub(crate) fn place_value(&mut self, value: &mut hir::Expr, to: &Type) -> bool {
        self.place_function_value(value, to);
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
    ) -> hir::Expr {
        let Type::Func(own) = self.apparent_type(&callback.ty) else {
            return callback;
        };
        let mut place_params = params.to_vec();
        if own.params.len() == params.len() + 1 {
            place_params.push(Type::I32);
        }
        let place = Type::func(place_params, ret.cloned().unwrap_or(own.ret));
        self.place_function_value(&mut callback, &place);
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
        if !self.place_value(from, to) {
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
