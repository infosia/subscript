//! Lowering for new expressions, class descriptors, and field stores.

use super::*;

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    pub(super) fn lower_new(
        &mut self,
        class_id: ClassId,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let class = self
            .lowering
            .hir
            .classes
            .get(class_id.0)
            .cloned()
            .ok_or_else(|| self.error(&expr.pos, "constructed class id is missing"))?;
        let allocation_traps = convert_traps(&expr.trap_sites(self.lowering.hir))
            .into_iter()
            .filter(|trap| trap.kind == l::TrapKind::Allocation && trap.pos == expr.pos)
            .collect();
        let allocated = self
            .emit(
                l::InstructionKind::AllocateClass(class_id),
                Vec::new(),
                Some(self.allocated_type(class_id, &expr.pos)?),
                false,
                allocation_traps,
                expr.pos.clone(),
            )?
            .expect("class allocation");

        // compiler.md §57.1 orders a construction: the explicit
        // arguments, the field initializers, then the defaults of the
        // absent arguments, which read `this`.
        let mut constructor_args = Vec::new();
        let mut pending = None;
        let params = class.ctor.as_ref().map(|constructor| {
            constructor
                .params
                .iter()
                .map(CallParam::from)
                .collect::<Vec<_>>()
        });
        if let Some(params) = &params {
            pending = Some(self.lower_explicit_arguments(params, args, false)?);
        }
        for (index, field) in class.fields.iter().enumerate() {
            if let Some(initializer) = &field.init {
                let saved_this = self.this_value.replace(allocated.clone());
                let value = self.lower_stored_expr_at(&field.ty, initializer, &field.pos);
                self.this_value = saved_this;
                let value = value?;
                self.store_class_field(class_id, index, allocated.clone(), value, &field.pos)?;
            }
        }
        if let (Some(params), Some(mut pending)) = (params, pending) {
            let receiver = PreparedBase::Value(allocated.clone());
            self.lower_argument_defaults(&params, args, Some(&receiver), false, &mut pending)?;
            constructor_args = self.finish_call_arguments(pending)?;
        }
        if class.is_boundary {
            if args.len() != class.fields.len() {
                return Err(self.error(
                    &expr.pos,
                    format!(
                        "boundary class `{}` has {} fields but {} constructor arguments",
                        class.name,
                        class.fields.len(),
                        args.len()
                    ),
                ));
            }
            for (index, (field, argument)) in class.fields.iter().zip(args).enumerate() {
                let value = self.lower_argument_value(&field.ty, argument)?;
                self.store_class_field(class_id, index, allocated.clone(), value, &argument.pos)?;
            }
        }
        if let Some(constructor) = &class.ctor {
            let record =
                self.lowering
                    .method_record(class_id.0, "constructor", &constructor.pos)?;
            let mut operands = vec![allocated.clone()];
            operands.extend(constructor_args);
            let receiver_type = if class.is_value {
                l::ValueType::Address(l::AddressType {
                    pointee: Type::Class(class_id),
                    array_base: None,
                })
            } else {
                l::ValueType::Data(Type::Class(class_id))
            };
            let parameter_types = std::iter::once(receiver_type)
                .chain(
                    constructor
                        .params
                        .iter()
                        .map(|parameter| l::ValueType::Data(parameter.ty.clone())),
                )
                .collect();
            let call_traps = convert_traps(&expr.trap_sites(self.lowering.hir))
                .into_iter()
                .filter(|trap| trap.kind == l::TrapKind::Call)
                .collect();
            let stored = constructor
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| StoredOperand {
                    index: index + 1,
                    ty: l::ValueType::Data(parameter.ty.clone()),
                    action: OwnerStoreAction::Acquire(hir::AsyncCopySite::CallArgument),
                    pos: args
                        .get(index)
                        .map_or_else(|| parameter.pos.clone(), |argument| argument.pos.clone()),
                })
                .collect();
            self.emit_store_instruction(
                l::InstructionKind::Call(l::CallTarget {
                    kind: l::CallTargetKind::Method(record.method.expect("constructor method id")),
                    parameter_types,
                    return_type: None,
                }),
                operands,
                stored,
                (None, true),
                call_traps,
                expr.pos.clone(),
            )?;
        } else if !class.is_boundary && !args.is_empty() {
            return Err(self.error(
                &expr.pos,
                format!(
                    "class `{}` has no constructor but received {} arguments",
                    class.name,
                    args.len()
                ),
            ));
        }
        if class.is_value {
            self.emit(
                l::InstructionKind::LoadAddress,
                vec![allocated],
                Some(l::ValueType::Data(Type::Class(class_id))),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .ok_or_else(|| self.error(&expr.pos, "value construction produced no value"))
        } else {
            Ok(allocated)
        }
    }

    pub(super) fn lower_descriptor(
        &mut self,
        class_id: ClassId,
        slots: &[Option<hir::Expr>],
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let class = self
            .lowering
            .hir
            .classes
            .get(class_id.0)
            .cloned()
            .ok_or_else(|| self.error(&expr.pos, "descriptor class id is missing"))?;
        if !class.is_descriptor || class.is_value || slots.len() != class.fields.len() {
            return Err(self.error(
                &expr.pos,
                format!("invalid descriptor construction for `{}`", class.name),
            ));
        }
        let allocated = self
            .emit(
                l::InstructionKind::AllocateClass(class_id),
                Vec::new(),
                Some(l::ValueType::Data(Type::Class(class_id))),
                false,
                convert_traps(&expr.trap_sites(self.lowering.hir)),
                expr.pos.clone(),
            )?
            .expect("descriptor allocation");
        for (index, (slot, field)) in slots.iter().zip(&class.fields).enumerate() {
            let value = if let Some(value) = slot {
                self.lower_stored_expr(&field.ty, value)?
            } else if field.is_absence_capable {
                let Type::StringAlias(alias) = field.ty else {
                    return Err(
                        self.error(&field.pos, "absence-capable field is not a string alias")
                    );
                };
                let discriminant = self
                    .lowering
                    .hir
                    .string_aliases
                    .get(alias.0)
                    .map(hir::StringAliasDef::absence_discriminant)
                    .ok_or_else(|| self.error(&field.pos, "absence alias is missing"))?;
                l::Operand::Constant(l::Constant {
                    ty: field.ty.clone(),
                    kind: l::ConstantKind::Integer(discriminant),
                })
            } else {
                let default = field.init.as_ref().ok_or_else(|| {
                    self.error(
                        &field.pos,
                        format!("descriptor field `{}` has no value or default", field.name),
                    )
                })?;
                let saved_this = self.this_value.replace(allocated.clone());
                let value = self.lower_stored_expr(&field.ty, default);
                self.this_value = saved_this;
                value?
            };
            self.store_class_field(class_id, index, allocated.clone(), value, &field.pos)?;
        }
        Ok(allocated)
    }

    fn store_class_field(
        &mut self,
        class: ClassId,
        index: usize,
        object: l::Operand,
        value: l::Operand,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let definition = self
            .lowering
            .hir
            .classes
            .get(class.0)
            .and_then(|class| class.fields.get(index))
            .ok_or_else(|| self.error(pos, "class field index is missing"))?;
        let field = self
            .lowering
            .fields
            .get(&(class.0, definition.name.clone()))
            .copied()
            .ok_or_else(|| self.error(pos, "class field id is missing"))?;
        let value = self.coerce_operand(value, l::ValueType::Data(definition.ty.clone()), pos)?;
        let base_type = self.operand_type(&object, pos)?;
        let array_base = match base_type {
            l::ValueType::Address(address) => address.array_base,
            _ => None,
        };
        let address = self
            .emit(
                l::InstructionKind::AddressOfField(l::FieldRef::Class(field)),
                vec![object],
                Some(l::ValueType::Address(l::AddressType {
                    pointee: definition.ty.clone(),
                    array_base,
                })),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("field address");
        self.emit_store_instruction(
            l::InstructionKind::StoreAddress,
            vec![address, value],
            vec![StoredOperand {
                index: 1,
                ty: l::ValueType::Data(definition.ty.clone()),
                action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Assignment),
                pos: pos.clone(),
            }],
            (None, false),
            Vec::new(),
            pos.clone(),
        )?;
        Ok(())
    }

    pub(super) fn lower_argument_value(
        &mut self,
        expected: &Type,
        argument: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        if self.embedded_header_extension(expected, argument).is_some() {
            self.lower_stored_expr(expected, argument)
        } else {
            self.require_expr(argument)
        }
    }

    pub(super) fn lower_foreign_argument_value(
        &mut self,
        expected: &Type,
        argument: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        if self.embedded_header_extension(expected, argument).is_some() {
            return self.lower_stored_expr(expected, argument);
        }
        if !self.is_boundary_box_narrowing(expected, &argument.ty) {
            return self.require_expr(argument);
        }
        let value = if is_place_expr(argument) {
            let place = self.prepare_place(argument)?;
            self.materialize_address_inner(&place, &argument.pos, false)?
        } else {
            self.require_expr(argument)?
        };
        match self.operand_type(&value, &argument.pos)? {
            l::ValueType::Address(_) | l::ValueType::Data(Type::Nullable(_)) => Ok(value),
            l::ValueType::Data(Type::Class(class))
                if expected == &Type::Nullable(Box::new(Type::Class(class))) =>
            {
                self.emit(
                    l::InstructionKind::AddressOfValue,
                    vec![value],
                    Some(l::ValueType::Address(l::AddressType {
                        pointee: Type::Class(class),
                        array_base: None,
                    })),
                    false,
                    Vec::new(),
                    argument.pos.clone(),
                )?
                .ok_or_else(|| self.error(&argument.pos, "foreign argument address is missing"))
            }
            other => Err(self.error(
                &argument.pos,
                format!("nullable boundary argument has invalid LIR type {other:?}"),
            )),
        }
    }
}
