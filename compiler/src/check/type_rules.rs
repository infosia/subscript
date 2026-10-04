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
                (Type::Nullable(a), Type::Nullable(_)) => {
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
        match (from, to) {
            (Type::Null, Type::Nullable(_)) => true,
            (f, Type::Nullable(inner)) => {
                self.assignable_through_constraints(f, inner, seen)
                    || (self.is_reference_class(f) && **inner == Type::Object)
            }
            (f, Type::Object) => self.is_reference_class(f),
            _ => false,
        }
    }

    pub(super) fn contains_string_alias(ty: &Type) -> bool {
        match ty {
            Type::StringAlias(_) => true,
            Type::FixedArray(element, _)
            | Type::Array(element)
            | Type::Set(element)
            | Type::Nullable(element)
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

    /// Emits the rule-specific diagnostic for a failed assignment.
    pub(crate) fn require_assignable(&mut self, from: &Type, to: &Type, pos: Pos, what: &str) {
        if self.assignable(from, to) {
            return;
        }
        self.report_not_assignable(from, to, pos, what);
    }

    /// Keeps the C17 classification at an assignment rejection.
    pub(super) fn require_expr_assignable(
        &mut self,
        from: &hir::Expr,
        to: &Type,
        fx: &FnCtx,
        what: &str,
    ) {
        if !self.assignable(&from.ty, to) {
            if matches!(self.apparent_type(&from.ty), Type::Nullable(inner) if *inner == self.apparent_type(to))
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
        let class_like = |t: &Type| match t {
            Type::Class(_) | Type::Map(..) | Type::Set(_) => true,
            Type::Nullable(inner) => {
                matches!(**inner, Type::Class(_) | Type::Map(..) | Type::Set(_))
            }
            _ => false,
        };
        let from_nominal = match from {
            Type::Nullable(inner) => inner.as_ref(),
            other => other,
        };
        let to_nominal = match to {
            Type::Nullable(inner) => inner.as_ref(),
            other => other,
        };
        if class_like(from) && class_like(to) {
            let message = format!(
                "nominal types are not interchangeable: {} expects `{}`, got `{}`",
                what, to_n, from_n
            );
            if matches!(&self.apparent_type(from), Type::Nullable(inner) if **inner == self.apparent_type(to))
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
        } else if self.is_value_class(from) && matches!(to, Type::Nullable(_)) {
            self.reject_subset(
                RejectionSite::NullableValueClassAssignment,
                format!("value class `{}` cannot be nullable", from_n),
                pos,
            );
        } else {
            let message = format!(
                "type mismatch: {} expects `{}`, got `{}`",
                what, to_n, from_n
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
                        _ => RejectionSite::AssignmentTypeMismatch,
                    },
                    message,
                    pos,
                );
            }
        }
    }
}
