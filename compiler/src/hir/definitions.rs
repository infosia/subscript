use super::*;

impl Module {
    /// Returns every expression owner in the module.
    ///
    /// A pass must traverse nested expressions through [`Expr::children`]
    /// and [`Stmt::children`].
    pub fn expression_owners(&self) -> impl Iterator<Item = ExpressionOwner<'_>> {
        fn add_function<'a>(function: &'a Function, owners: &mut Vec<ExpressionOwner<'a>>) {
            owners.extend(
                function
                    .params
                    .iter()
                    .filter_map(|parameter| parameter.default.as_ref())
                    .map(ExpressionOwner::Expr),
            );
            owners.push(ExpressionOwner::Body {
                statements: &function.body,
                function: Some(function),
            });
        }

        let mut owners = Vec::new();
        for class in &self.classes {
            owners.extend(
                class
                    .fields
                    .iter()
                    .filter_map(|field| field.init.as_ref())
                    .map(ExpressionOwner::Expr),
            );
            if let Some(constructor) = &class.ctor {
                add_function(constructor, &mut owners);
            }
            for method in &class.methods {
                add_function(method, &mut owners);
            }
        }
        owners.extend(
            self.globals
                .iter()
                .map(|global| ExpressionOwner::Expr(&global.init)),
        );
        for function in &self.functions {
            add_function(function, &mut owners);
        }
        owners.push(ExpressionOwner::Body {
            statements: &self.top_level,
            function: None,
        });
        owners.into_iter()
    }

    /// Returns every expression owner in the module with mutable access.
    ///
    /// The mutable references let module passes record facts on expression
    /// nodes. A pass must traverse nested expressions through
    /// [`Expr::children`] and [`Stmt::children`].
    pub fn expression_owners_mut(&mut self) -> impl Iterator<Item = ExpressionOwnerMut<'_>> {
        fn add_function<'a>(function: &'a mut Function, owners: &mut Vec<ExpressionOwnerMut<'a>>) {
            owners.extend(
                function
                    .params
                    .iter_mut()
                    .filter_map(|parameter| parameter.default.as_mut())
                    .map(ExpressionOwnerMut::Expr),
            );
            owners.push(ExpressionOwnerMut::Body(&mut function.body));
        }

        let mut owners = Vec::new();
        for class in &mut self.classes {
            owners.extend(
                class
                    .fields
                    .iter_mut()
                    .filter_map(|field| field.init.as_mut())
                    .map(ExpressionOwnerMut::Expr),
            );
            if let Some(constructor) = &mut class.ctor {
                add_function(constructor, &mut owners);
            }
            for method in &mut class.methods {
                add_function(method, &mut owners);
            }
        }
        owners.extend(
            self.globals
                .iter_mut()
                .map(|global| ExpressionOwnerMut::Expr(&mut global.init)),
        );
        for function in &mut self.functions {
            add_function(function, &mut owners);
        }
        owners.push(ExpressionOwnerMut::Body(&mut self.top_level));
        owners.into_iter()
    }
}

impl From<&ClassDef> for HandleClass {
    fn from(class: &ClassDef) -> Self {
        if !class.is_value {
            Self::Reference
        } else if class.is_boundary {
            Self::BoundaryValue
        } else {
            Self::Value
        }
    }
}

impl StringAliasDef {
    /// The implementation-reserved discriminant for an absent descriptor
    /// member. A plain alias uses `-1` (§43); a wire alias chooses the
    /// first `i32` at or above `i32::MIN` that is outside its wire set.
    #[must_use]
    pub fn absence_discriminant(&self) -> i64 {
        let Some(wire_values) = &self.wire_values else {
            return crate::types::ABSENT_STRING_ALIAS_DISCRIMINANT;
        };
        let mut candidate = i32::MIN;
        while wire_values.contains(&candidate) {
            candidate = candidate
                .checked_add(1)
                .expect("a CEnum with at most i32::MAX members leaves a sentinel");
        }
        i64::from(candidate)
    }

    /// The representation of the declaration-ordered member at `index`.
    /// Wire aliases use the declared wire value; plain aliases use `index`.
    #[must_use]
    pub fn member_discriminant(&self, index: usize) -> Option<i64> {
        match &self.wire_values {
            Some(values) => values.get(index).copied().map(i64::from),
            None => i64::try_from(index).ok(),
        }
    }
}

impl Function {
    /// Fault points owned by entering this function rather than by one of
    /// its body expressions.
    ///
    /// Generator invocation allocates its suspended frame in the generated
    /// creator function. Ordinary functions have no function-level site.
    #[must_use]
    pub fn trap_sites(&self) -> Vec<TrapSite> {
        if self.is_generator || self.is_async {
            vec![TrapSite::Allocation {
                pos: self.pos.clone(),
            }]
        } else {
            Vec::new()
        }
    }

    /// Fault points owned by the host-entry adapter for this function.
    ///
    /// A wire-mapped string alias enters the adapter as its integer wire
    /// value. The adapter validates that value before it calls the script
    /// function. Other function-level and expression-level sites are returned
    /// by [`Function::trap_sites`] and [`Expr::trap_sites`].
    #[must_use]
    pub fn host_entry_trap_sites(&self, module: &Module) -> Option<Vec<TrapSite>> {
        if self.is_generator || self.ret != Type::Void || (self.is_async && !self.params.is_empty())
        {
            return None;
        }
        let parameter_is_supported = |parameter: &Param| {
            parameter.ty.is_numeric()
                || parameter.ty == Type::Bool
                || matches!(&parameter.ty, Type::Class(id) if module
                .classes
                .get(id.0)
                .is_some_and(|class| {
                    !class.is_value
                        && !class.is_descriptor
                        && !class.is_boundary
                        && class.fields.is_empty()
                        && class.ctor.is_none()
                        && class.methods.is_empty()
                        && class.index_signature.is_none()
                }))
                || matches!(&parameter.ty, Type::StringAlias(alias) if module
                    .string_aliases
                    .get(alias.0)
                    .is_some_and(|definition| definition.wire_values.is_some()))
        };
        if !self.params.iter().all(parameter_is_supported) {
            return None;
        }
        Some(
            self.params
                .iter()
                .filter_map(|parameter| {
                    let Type::StringAlias(alias) = &parameter.ty else {
                        return None;
                    };
                    module
                        .string_aliases
                        .get(alias.0)
                        .and_then(|definition| definition.wire_values.as_ref())
                        .map(|_| TrapSite::WireEnumValue {
                            alias: *alias,
                            pos: parameter.pos.clone(),
                        })
                })
                .collect(),
        )
    }
}

impl TrapSite {
    /// Source position owned by this individual guard/check point.
    #[must_use]
    pub fn pos(&self) -> &Pos {
        match self {
            TrapSite::Allocation { pos }
            | TrapSite::Call { pos }
            | TrapSite::Raise { pos }
            | TrapSite::Unreachable { pos }
            | TrapSite::DivisionByZero { pos }
            | TrapSite::IndexRead { pos }
            | TrapSite::IndexWrite { pos }
            | TrapSite::NullNarrowing { pos }
            | TrapSite::ClassMismatch { pos, .. }
            | TrapSite::DevOnlyLifetime { pos, .. }
            | TrapSite::DevOnlyRelease { pos, .. }
            | TrapSite::DevReloadOnlyStaleCoroutine { pos }
            | TrapSite::WireEnumValue { pos, .. } => pos,
        }
    }
}
