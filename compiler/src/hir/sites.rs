use super::*;

impl Expr {
    /// The ordered trap sites owned directly by this operation.
    ///
    /// Sites in child expressions are carried by those children and occur
    /// where normal evaluation reaches them. This method is the single HIR
    /// policy used by both lowering tiers; neither backend decides whether
    /// a call, literal, cast, or index operation is checked.
    #[must_use]
    pub fn trap_sites(&self, module: &Module) -> Vec<TrapSite> {
        self.trap_sites_for_reload(module, false)
    }

    /// Derives operation sites for the selected compile mode (compiler.md §121.1).
    /// Reload calls carry raise sites even when their callees cannot raise.
    #[must_use]
    pub fn trap_sites_for_reload(&self, module: &Module, reload: bool) -> Vec<TrapSite> {
        use BinOp as B;
        use ExprKind as K;

        let allocation = |pos: &Pos| TrapSite::Allocation { pos: pos.clone() };
        let call = |pos: &Pos| TrapSite::Call { pos: pos.clone() };
        let lifetime = |pos: &Pos| TrapSite::DevOnlyLifetime {
            operand: LifetimeOperand::Receiver,
            pos: pos.clone(),
        };
        let handle_classes = module
            .classes
            .iter()
            .map(HandleClass::from)
            .collect::<Vec<_>>();
        // Fact filter: can dereferencing this value observe a freed Context allocation?
        let reference_value = |ty: &Type| {
            ty.handle_kind(&handle_classes)
                .is_some_and(HandleKind::needs_lifetime_trap)
        };
        let boundary_box_store = |stored: &Type, value: &Type| {
            matches!(stored, Type::Nullable(inner)
            if matches!(inner.as_ref(), Type::Class(class)
                if value == &Type::Class(*class)
                    && module.classes.get(class.0).is_some_and(|definition| {
                        definition.is_value && definition.is_boundary
                    })))
        };
        let embedded_header_store = |stored: &Type, value: &Expr| {
            let Type::Nullable(inner) = stored else {
                return false;
            };
            let Type::Class(header) = inner.as_ref() else {
                return false;
            };
            let K::Field { obj, name } = &value.kind else {
                return false;
            };
            let Type::Class(extension) = obj.ty else {
                return false;
            };
            let nullable = Type::Nullable(Box::new(Type::Class(*header)));
            let linked = module.classes.iter().any(|class| {
                class.is_boundary && class.fields.iter().any(|field| field.ty == nullable)
            }) || module.foreign_fns.iter().any(|function| {
                function
                    .params
                    .iter()
                    .any(|parameter| parameter.ty == nullable)
            });
            module
                .classes
                .get(extension.0)
                .filter(|class| class.is_value && class.is_boundary)
                .and_then(|class| class.fields.first())
                .is_some_and(|field| {
                    field.name == *name && field.ty == Type::Class(*header) && linked
                })
        };
        let index_site = |target: &Expr, write: bool| {
            let K::Index { checked, .. } = &target.kind else {
                return None;
            };
            if !*checked {
                return None;
            }
            Some(if write {
                TrapSite::IndexWrite {
                    pos: target.pos.clone(),
                }
            } else {
                TrapSite::IndexRead {
                    pos: target.pos.clone(),
                }
            })
        };

        let declared = match &self.kind {
            K::Global(name) => module
                .globals
                .iter()
                .find(|global| global.symbol == *name)
                .map(|global| &global.ty),
            K::Field { obj, name } => declared_field_type(&obj.ty, name, &module.classes),
            K::Cast(_)
            | K::Local(..)
            | K::This
            | K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Str(_)
            | K::Null
            | K::FuncRef(_)
            | K::EnumMember { .. }
            | K::Unary { .. }
            | K::Binary { .. }
            | K::AbsenceTest { .. }
            | K::Assign { .. }
            | K::Call { .. }
            | K::New { .. }
            | K::DescriptorLit { .. }
            | K::Zero
            | K::Unassigned
            | K::RawNew { .. }
            | K::Length(_)
            | K::Index { .. }
            | K::ArrayLit(_)
            | K::ArraySpreadLit(_)
            | K::Template(_)
            | K::Lambda { .. }
            | K::Yield(_)
            | K::AsyncSuspend
            | K::AsyncCall { .. }
            | K::AsyncHandleCreate { .. }
            | K::AsyncHandleAwait(_)
            | K::TaskGroup { .. }
            | K::AsyncAll { .. }
            | K::AsyncHandleTransfer { .. }
            | K::Cond { .. } => None,
        };
        let narrowed = self.is_shared_location(&module.classes)
            && matches!(declared, Some(Type::Nullable(inner)) if inner.as_ref() == &self.ty);
        let mut sites = match &self.kind {
            K::Str(_) => vec![allocation(&self.pos)],
            K::Binary { op, left, .. } if *op == B::Add && left.ty == Type::Str => {
                vec![allocation(&self.pos)]
            }
            K::Binary { op, left, .. } if matches!(op, B::Div | B::Rem) && left.ty.is_integer() => {
                vec![TrapSite::DivisionByZero {
                    pos: self.pos.clone(),
                }]
            }
            K::Assign { op, target, .. } => {
                let mut sites = Vec::new();
                if let K::Field { obj, .. } = &target.kind {
                    if reference_value(&obj.ty) {
                        sites.push(lifetime(&obj.pos));
                    }
                }
                if let K::Index { obj, .. } = &target.kind {
                    if matches!(obj.ty, Type::Array(_)) {
                        sites.push(lifetime(&obj.pos));
                    }
                    if matches!((&obj.ty, op), (Type::Array(_), Some(_)))
                        || matches!((&obj.ty, op), (Type::FixedArray(..), Some(_)))
                    {
                        if let Some(site) = index_site(target, false) {
                            sites.push(site);
                        }
                    }
                }
                if matches!(op, Some(B::Add)) && target.ty == Type::Str {
                    sites.push(allocation(&target.pos));
                } else if matches!(op, Some(B::Div | B::Rem)) && target.ty.is_integer() {
                    sites.push(TrapSite::DivisionByZero {
                        pos: target.pos.clone(),
                    });
                }
                if let K::Index { obj, .. } = &target.kind {
                    if matches!(obj.ty, Type::Array(_)) || op.is_none() {
                        if let Some(site) = index_site(target, true) {
                            sites.push(site);
                        }
                    }
                }
                sites
            }
            K::Cast(inner)
                if matches!(self.ty, Type::Class(_))
                    && (matches!(inner.ty, Type::Object)
                        || matches!(&inner.ty, Type::Nullable(ty) if **ty == Type::Object)) =>
            {
                let Type::Class(class) = self.ty else {
                    unreachable!()
                };
                vec![
                    TrapSite::NullNarrowing {
                        pos: self.pos.clone(),
                    },
                    lifetime(&self.pos),
                    TrapSite::ClassMismatch {
                        class,
                        pos: self.pos.clone(),
                    },
                ]
            }
            K::Call {
                callee: Callee::Ambient(AmbientFn::Unreachable),
                ..
            } => vec![TrapSite::Unreachable {
                pos: self.pos.clone(),
            }],
            K::Call { callee, args } => {
                let mut sites = Vec::new();
                if matches!(callee, Callee::Ambient(AmbientFn::UnsafeDelete)) {
                    sites.push(TrapSite::DevOnlyRelease {
                        operand: LifetimeOperand::Argument(0),
                        pos: self.pos.clone(),
                    });
                    if args.first().is_some_and(|argument| {
                        matches!(&argument.ty,
                        Type::Map(_, value) if value.counted_type().is_some())
                        || matches!(argument.ty, Type::Class(id) if module.classes.get(id.0).is_some_and(|class| class.fields.iter().any(|field| field.ty.counted_type().is_some())))
                    }) {
                        sites.push(call(&self.pos));
                    }
                    return sites;
                }
                if let Callee::Method { recv, name } = callee {
                    if reference_value(&recv.ty) {
                        sites.push(lifetime(&recv.pos));
                    }
                    if name.full_text() == "next" && matches!(recv.ty, Type::Generator(_)) {
                        sites.push(TrapSite::DevReloadOnlyStaleCoroutine {
                            pos: self.pos.clone(),
                        });
                    }
                }
                let operation = operation_signature_target(callee);
                let helper = matches!(callee, Callee::Func(name)
                    if module.synthesized_helpers.contains(name));
                let parameter_types = match callee {
                    Callee::Func(name) => module
                        .functions
                        .iter()
                        .find(|function| function.symbol == *name)
                        .map(|function| {
                            function
                                .params
                                .iter()
                                .map(|parameter| parameter.ty.clone())
                                .collect::<Vec<_>>()
                        }),
                    Callee::Foreign(name) => module
                        .foreign_fns
                        .iter()
                        .find(|function| function.name == *name)
                        .map(|function| {
                            function
                                .params
                                .iter()
                                .map(|parameter| parameter.ty.clone())
                                .collect::<Vec<_>>()
                        }),
                    Callee::Value(value) => match &value.ty {
                        Type::Func(signature) => Some(signature.params.clone()),
                        _ => None,
                    },
                    Callee::Method { recv, name } => match recv.ty {
                        Type::Class(class) => module.classes.get(class.0).and_then(|definition| {
                            definition
                                .methods
                                .iter()
                                .find(|method| method.symbol == *name)
                                .map(|method| {
                                    method
                                        .params
                                        .iter()
                                        .map(|parameter| parameter.ty.clone())
                                        .collect::<Vec<_>>()
                                })
                        }),
                        _ => None,
                    },
                    _ => None,
                }
                .or_else(|| {
                    let (target, prefix) = operation_signature_target(callee)?;
                    let prefix_count = usize::from(prefix.is_some());
                    module
                        .operation_signatures
                        .iter()
                        .find(|signature| {
                            signature.target == target
                                && signature.parameter_types.len() == args.len() + prefix_count
                                && prefix.is_none_or(|prefix| {
                                    signature.parameter_types.first() == Some(prefix)
                                })
                                && signature.parameter_types[prefix_count..]
                                    .iter()
                                    .zip(args)
                                    .all(|(parameter, argument)| {
                                        parameter == &argument.ty
                                            || matches!(parameter, Type::Nullable(inner)
                                                if argument.ty == Type::Null || inner.as_ref() == &argument.ty)
                                            || boundary_box_store(parameter, &argument.ty)
                                    })
                        })
                        .map(|signature| signature.parameter_types[prefix_count..].to_vec())
                });
                if operation.is_some() || helper {
                    for (index, argument) in args.iter().enumerate() {
                        let execution_index =
                            index + usize::from(matches!(callee, Callee::Method { .. }));
                        let parameter = parameter_types.as_ref().and_then(|types| types.get(index));
                        let reference = reference_value(&argument.ty)
                            || (argument.ty == Type::Null
                                && parameter.is_some_and(reference_value));
                        if reference
                            && !operation.as_ref().is_some_and(|(target, _)| {
                                target.copies_lifetime_operand(execution_index)
                            })
                        {
                            sites.push(TrapSite::DevOnlyLifetime {
                                operand: LifetimeOperand::Argument(index),
                                pos: argument.pos.clone(),
                            });
                        }
                    }
                }
                let counted_map = matches!(callee, Callee::Map(_))
                    && args.first().map(|argument| &argument.ty).into_iter()
                        .chain(std::iter::once(&self.ty))
                        .any(|ty| matches!(ty, Type::Map(_, value) if value.counted_type().is_some()));
                if callee.has_call_site() || counted_map {
                    sites.push(call(&self.pos));
                }
                if (reload && callee.has_call_site())
                    || crate::raise_sites::call_can_raise(module, callee, args)
                {
                    sites.push(TrapSite::Raise {
                        pos: self.pos.clone(),
                    });
                }
                if let Some(parameter_types) = parameter_types {
                    sites.extend(
                        parameter_types
                            .iter()
                            .zip(args)
                            .filter(|(parameter, argument)| {
                                boundary_box_store(parameter, &argument.ty)
                                    && (!matches!(callee, Callee::Foreign(_))
                                        || embedded_header_store(parameter, argument))
                            })
                            .map(|(_, argument)| allocation(&argument.pos)),
                    );
                }
                if let Callee::Foreign(name) = callee {
                    let wire_alias = module
                        .foreign_fns
                        .iter()
                        .find(|foreign| foreign.name == *name)
                        .and_then(|foreign| match foreign.ret {
                            Type::StringAlias(alias) => Some(alias),
                            _ => None,
                        })
                        .filter(|alias| {
                            module
                                .string_aliases
                                .get(alias.0)
                                .is_some_and(|definition| definition.wire_values.is_some())
                        });
                    if let Some(alias) = wire_alias {
                        sites.push(TrapSite::WireEnumValue {
                            alias,
                            pos: self.pos.clone(),
                        });
                    }
                }
                sites
            }
            K::TaskGroup {
                operation: TaskGroupOperation::Create | TaskGroupOperation::Join,
                ..
            } => vec![allocation(&self.pos), call(&self.pos)],
            K::TaskGroup { .. } => vec![call(&self.pos)],
            K::AsyncAll { .. } => vec![allocation(&self.pos), lifetime(&self.pos)],
            K::AsyncCall { callee, .. } | K::AsyncHandleCreate { callee, .. } => {
                let mut sites = Vec::new();
                if let Some(receiver) = callee.receiver() {
                    if reference_value(&receiver.ty) {
                        sites.push(lifetime(&receiver.pos));
                    }
                }
                sites.push(call(&self.pos));
                // compiler.md §116.1 and §121.1: direct awaits use the
                // selected call facts. A held handle raises at its await.
                if matches!(self.kind, K::AsyncCall { .. })
                    && (reload || crate::raise_sites::async_callee_can_raise(module, callee))
                {
                    sites.push(TrapSite::Raise {
                        pos: self.pos.clone(),
                    });
                }
                sites
            }
            // compiler.md §116.1 rule 2: the `await` of a held handle is a
            // raise site.
            K::AsyncHandleAwait(_) => vec![
                TrapSite::DevReloadOnlyStaleCoroutine {
                    pos: self.pos.clone(),
                },
                call(&self.pos),
                TrapSite::Raise {
                    pos: self.pos.clone(),
                },
            ],
            K::New { class, args } => {
                let Some(def) = module.classes.get(class.0) else {
                    return Vec::new();
                };
                let mut sites = Vec::new();
                if !def.is_value {
                    sites.push(allocation(&self.pos));
                }
                sites.extend(
                    def.fields
                        .iter()
                        .zip(args)
                        .filter(|(field, argument)| boundary_box_store(&field.ty, &argument.ty))
                        .map(|(_, argument)| allocation(&argument.pos)),
                );
                if def.ctor.is_some() {
                    sites.push(call(&self.pos));
                }
                if (reload && def.ctor.is_some())
                    || crate::raise_sites::construction_call_can_raise(def)
                {
                    sites.push(TrapSite::Raise {
                        pos: self.pos.clone(),
                    });
                }
                sites
            }
            K::DescriptorLit { .. } => vec![allocation(&self.pos)],
            K::RawNew { .. } => vec![allocation(&self.pos)],
            K::Length(value) => value.statement_read_sites(module),
            K::Field { obj, name } => {
                let mut sites = Vec::new();
                if name == "value"
                    && matches!(&obj.ty, Type::IterResult(value)
                    if !value.zero_is_value(&handle_classes, &|id| module.classes[id.0].fields.iter().map(|field| &field.ty), &|id| module.string_aliases[id.0].wire_values.as_deref()))
                {
                    sites.push(TrapSite::GeneratorDoneValue {
                        pos: self.pos.clone(),
                    });
                }
                if reference_value(&obj.ty) {
                    sites.push(lifetime(&obj.pos));
                }
                if let (Type::StringAlias(alias), Type::Class(class)) = (&self.ty, &obj.ty) {
                    let wire_member = module
                        .classes
                        .get(class.0)
                        .is_some_and(|definition| definition.is_boundary)
                        && module
                            .string_aliases
                            .get(alias.0)
                            .is_some_and(|definition| definition.wire_values.is_some());
                    if wire_member {
                        sites.push(TrapSite::WireEnumValue {
                            alias: *alias,
                            pos: self.pos.clone(),
                        });
                    }
                }
                sites
            }
            K::Index { obj, checked, .. } => {
                let mut sites = Vec::new();
                if matches!(obj.ty, Type::Array(_)) {
                    sites.push(lifetime(&obj.pos));
                }
                if *checked {
                    sites.push(TrapSite::IndexRead {
                        pos: self.pos.clone(),
                    });
                }
                sites
            }
            K::ArrayLit(elems) if matches!(self.ty, Type::Array(_)) => {
                let mut sites = Vec::with_capacity(elems.len() + 1);
                sites.push(allocation(&self.pos));
                sites.extend(elems.iter().map(|elem| allocation(&elem.pos)));
                sites
            }
            K::ArraySpreadLit(elems) => {
                let mut sites = Vec::with_capacity(elems.len() + 1);
                sites.extend(
                    elems
                        .iter()
                        .enumerate()
                        .filter(|(_, element)| {
                            element.spread.is_some() && reference_value(&element.expr.ty)
                        })
                        .map(|(index, element)| TrapSite::DevOnlyLifetime {
                            operand: LifetimeOperand::Argument(index),
                            pos: element.expr.pos.clone(),
                        }),
                );
                sites.push(allocation(&self.pos));
                sites.extend(elems.iter().map(|elem| allocation(&elem.expr.pos)));
                sites
            }
            K::Template(parts) => {
                if parts.is_empty() {
                    return vec![allocation(&self.pos)];
                }
                let mut sites = Vec::new();
                for (index, part) in parts.iter().enumerate() {
                    match part {
                        TplPart::Text(_) => sites.push(allocation(&self.pos)),
                        TplPart::Expr(expr) if expr.ty != Type::Str => {
                            sites.push(allocation(&expr.pos));
                        }
                        TplPart::Expr(_) => {}
                    }
                    if index != 0 {
                        sites.push(allocation(&self.pos));
                    }
                }
                sites
            }
            K::Cond { then, els, .. } => [then, els]
                .into_iter()
                .filter(|arm| boundary_box_store(&self.ty, &arm.ty))
                .map(|arm| allocation(&arm.pos))
                .collect(),
            K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Null
            | K::This
            | K::Local(..)
            | K::Global(_)
            | K::FuncRef(_)
            | K::EnumMember { .. }
            | K::Zero
            | K::Unassigned
            | K::Unary { .. }
            | K::Binary { .. }
            | K::AbsenceTest { .. }
            | K::Cast(_)
            | K::ArrayLit(_)
            | K::Lambda { .. }
            | K::Yield(_)
            | K::AsyncSuspend
            | K::AsyncHandleTransfer { .. } => Vec::new(),
        };
        if narrowed {
            sites.push(TrapSite::NullNarrowing {
                pos: self.pos.clone(),
            });
        }
        sites
    }
}

pub(super) fn declared_field_type<'a>(
    ty: &Type,
    name: &str,
    classes: &'a [ClassDef],
) -> Option<&'a Type> {
    match ty {
        Type::Class(id) => classes
            .get(id.0)
            .and_then(|class| class.fields.iter().find(|field| field.name == name))
            .map(|field| &field.ty),
        Type::Nullable(inner) => declared_field_type(inner, name, classes),
        Type::IterResult(_)
        | Type::I8
        | Type::U8
        | Type::I16
        | Type::U16
        | Type::I32
        | Type::U32
        | Type::I64
        | Type::U64
        | Type::F32
        | Type::F64
        | Type::F16
        | Type::Bool
        | Type::Str
        | Type::Date
        | Type::RegExp
        | Type::Void
        | Type::Null
        | Type::TaskGroup
        | Type::Object
        | Type::Enum(_)
        | Type::StringAlias(_)
        | Type::FixedArray(..)
        | Type::Array(_)
        | Type::Map(..)
        | Type::Set(_)
        | Type::Worker(..)
        | Type::Inbox(_)
        | Type::Outbox(_)
        | Type::Func(_)
        | Type::Generator(_)
        | Type::AsyncHandle(_)
        | Type::Error => None,
        // §143: no type parameter reaches the HIR.
        Type::TypeParameter(_) | Type::GenericNumber | Type::GenericUnion(_) => None,
    }
}
