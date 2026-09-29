use super::*;

impl<'p> Checker<'p> {
    /// Renders a type with real class/enum names, for messages.
    pub(crate) fn type_name(&self, ty: &Type) -> String {
        crate::types::display_type(
            ty,
            &|id| {
                self.classes
                    .get(id.0)
                    .map(|c| {
                        hir::declaration_label(
                            &c.name,
                            &c.pos,
                            self.classes
                                .iter()
                                .filter(|other| other.name == c.name)
                                .count()
                                > 1,
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
        matches!(ty, Type::Class(id) if self.classes[id.0].is_value)
    }

    pub(crate) fn is_reference_class(&self, ty: &Type) -> bool {
        ty.uses_reference_identity(&self.type_handle_classes)
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
        if matches!(from, Type::Error) || matches!(to, Type::Error) {
            return true;
        }
        if from == to {
            return true;
        }
        match (from, to) {
            (Type::Null, Type::Nullable(_)) => true,
            (f, Type::Nullable(inner)) => {
                f == &**inner || (self.is_reference_class(f) && **inner == Type::Object)
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

    pub(super) fn is_wire_alias(&self, ty: &Type) -> bool {
        matches!(ty, Type::StringAlias(alias) if self
            .string_aliases
            .get(alias.0)
            .is_some_and(|definition| definition.wire_values.is_some()))
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
        self.require_assignable_with(from, to, pos, what, None);
    }

    pub(crate) fn require_assignable_with(
        &mut self,
        from: &Type,
        to: &Type,
        pos: Pos,
        what: &str,
        divergence: Option<Divergence>,
    ) {
        if self.assignable(from, to) {
            return;
        }
        let from_n = self.type_name(from);
        let to_n = self.type_name(to);
        let class_like = |t: &Type| match t {
            Type::Class(_) | Type::Map(..) | Type::Set(_) => true,
            Type::Nullable(inner) => {
                matches!(**inner, Type::Class(_) | Type::Map(..) | Type::Set(_))
            }
            _ => false,
        };
        if class_like(from) && class_like(to) {
            let message = format!(
                "nominal types are not interchangeable: {} expects `{}`, got `{}`",
                what, to_n, from_n
            );
            if matches!((from, to), (Type::Class(_), Type::Class(_))) {
                self.error_diverging(
                    RuleCode::S005,
                    message,
                    pos,
                    Divergence::NominalClassIdentity,
                );
            } else {
                self.error(RuleCode::S005, message, pos);
            }
        } else if from.is_numeric() && to.is_numeric() {
            self.error_diverging(
                RuleCode::S007,
                format!(
                    "implicit numeric conversion from `{}` to `{}`; spell it `as {}`",
                    from_n, to_n, to_n
                ),
                pos,
                Divergence::SizedOperandWidths,
            );
        } else if self.is_value_class(from) && matches!(to, Type::Nullable(_)) {
            self.error(
                RuleCode::S011,
                format!("value class `{}` cannot be nullable", from_n),
                pos,
            );
        } else {
            let message = format!(
                "type mismatch: {} expects `{}`, got `{}`",
                what, to_n, from_n
            );
            let divergence = divergence.or_else(|| {
                matches!((from, to), (Type::StringAlias(_), Type::StringAlias(_)))
                    .then_some(Divergence::LiteralUnionAlias)
            });
            if let Some(divergence) = divergence {
                self.error_diverging(RuleCode::S100, message, pos, divergence);
            } else {
                self.error(RuleCode::S100, message, pos);
            }
        }
    }
}
