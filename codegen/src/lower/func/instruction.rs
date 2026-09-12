//! Instruction operands and the per-instruction emission.

use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    fn clone_value(&mut self, value: RV, ty: &l::ValueType) -> Result<RV, String> {
        match value_repr(&self.ml.layouts, ty)? {
            Repr::Agg { size, align } => {
                let source = self.expect_aggregate(value)?;
                let destination = self.stack_slot(size, align);
                self.copy_bytes(destination, source, size, align);
                Ok(RV::Aggregate(destination))
            }
            _ => Ok(value),
        }
    }

    fn instruction_operands(&mut self, instruction: &l::Instruction) -> Result<Vec<RV>, String> {
        instruction
            .operands
            .iter()
            .map(|operand| self.operand(operand))
            .collect()
    }

    fn instruction_operand_types(
        &self,
        instruction: &l::Instruction,
    ) -> Result<Vec<l::ValueType>, String> {
        instruction
            .operands
            .iter()
            .map(|operand| self.operand_type(operand))
            .collect()
    }

    fn result_type(&self, instruction: &l::Instruction) -> Result<Option<l::ValueType>, String> {
        instruction
            .result
            .map(|result| self.value_type(result).cloned())
            .transpose()
    }

    fn length(&mut self, value: RV, ty: &l::ValueType) -> Result<RV, String> {
        let length = match ty {
            l::ValueType::Data(Type::Array(_)) => {
                let handle = self.expect_scalar(value)?;
                let length = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), handle, ARRAY_LEN_OFFSET);
                self.builder.ins().ireduce(types::I32, length)
            }
            l::ValueType::Data(Type::FixedArray(_, count)) => {
                self.iconst(types::I32, i64::from(*count))
            }
            l::ValueType::Data(Type::Str) => {
                let handle = self.expect_scalar(value)?;
                self.call_runtime(self.ml.rt.str_len, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("string length has no result"))?
            }
            other => return Err(internal(format!("length has invalid operand {other:?}"))),
        };
        Ok(RV::Scalar(length))
    }

    fn call(
        &mut self,
        target: &l::CallTarget,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        if matches!(target.kind, l::CallTargetKind::Method(_)) {
            let receiver = self.expect_scalar(
                *operands
                    .first()
                    .ok_or_else(|| internal("method call has no receiver"))?,
            )?;
            for trap in traps {
                if trap.kind == l::TrapKind::DevOnlyLifetime {
                    self.emit_trap(trap, TrapOperand::Value(receiver))?;
                }
            }
        }
        let result = match &target.kind {
            l::CallTargetKind::Function(function) => self.script_call(
                *function,
                operands,
                parameter_types,
                target.return_type.as_ref(),
                false,
            )?,
            l::CallTargetKind::StaticClosure(function) => self.static_closure_call(
                *function,
                operands,
                parameter_types,
                target.return_type.as_ref(),
            )?,
            l::CallTargetKind::Method(method) => self.script_call(
                self.method_function(*method)?,
                operands,
                parameter_types,
                target.return_type.as_ref(),
                true,
            )?,
            l::CallTargetKind::Indirect => {
                self.indirect_call(operands, parameter_types, target.return_type.as_ref())?
            }
            l::CallTargetKind::Foreign(function) => self.foreign_call(
                *function,
                operands,
                parameter_types,
                target.return_type.as_ref(),
                traps,
                pos,
            )?,
            l::CallTargetKind::Intrinsic(intrinsic) => self.intrinsic_call(
                intrinsic,
                operands,
                parameter_types,
                target.return_type.as_ref(),
                traps,
                pos,
            )?,
            l::CallTargetKind::BuiltinMethod(method) => self.builtin_call(
                *method,
                operands,
                parameter_types,
                target.return_type.as_ref(),
                traps,
                pos,
            )?,
        };
        if matches!(
            target.kind,
            l::CallTargetKind::Function(_)
                | l::CallTargetKind::StaticClosure(_)
                | l::CallTargetKind::Method(_)
                | l::CallTargetKind::Indirect
        ) {
            for trap in traps {
                if trap.kind == l::TrapKind::Call {
                    self.emit_trap(trap, TrapOperand::Pending)?;
                }
            }
        }
        Ok(result)
    }

    pub(super) fn emit_instruction(&mut self, instruction: &l::Instruction) -> Result<(), String> {
        let operands = self.instruction_operands(instruction)?;
        let operand_types = self.instruction_operand_types(instruction)?;
        let result_ty = self.result_type(instruction)?;
        let result = match &instruction.kind {
            l::InstructionKind::Copy => Some(
                self.clone_value(
                    *operands
                        .first()
                        .ok_or_else(|| internal("Copy has no operand"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("Copy has no operand type"))?,
                )?,
            ),
            l::InstructionKind::StringLiteral(text) => {
                Some(self.string_literal(text, &instruction.traps, &instruction.pos)?)
            }
            l::InstructionKind::LoadLocal(local) => {
                let slot = self
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| internal(format!("local {} is missing", local.0)))?;
                let ty = self
                    .function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| internal(format!("local {} has no type", local.0)))?
                    .ty
                    .clone();
                let value = self.load_value_type(&ty, slot.address, 0)?;
                Some(self.clone_value(value, &ty)?)
            }
            l::InstructionKind::StoreLocal(local) => {
                let address = self
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| internal(format!("local {} is missing", local.0)))?
                    .address;
                let ty = self
                    .function
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| internal(format!("local {} has no type", local.0)))?
                    .ty
                    .clone();
                self.store_value_type(
                    &ty,
                    address,
                    0,
                    *operands
                        .first()
                        .ok_or_else(|| internal("StoreLocal has no value"))?,
                )?;
                None
            }
            l::InstructionKind::AddressOfLocal(local) => Some(RV::Scalar(
                self.locals
                    .get(local.0 as usize)
                    .ok_or_else(|| internal(format!("local {} is missing", local.0)))?
                    .address,
            )),
            l::InstructionKind::LoadGlobal(global) => {
                let (address, ty) = self.global_address(*global)?;
                let value = self.load_data(&ty, address, 0)?;
                Some(self.clone_value(value, &l::ValueType::Data(ty))?)
            }
            l::InstructionKind::StoreGlobal(global) => {
                let (address, ty) = self.global_address(*global)?;
                self.store_data(
                    &ty,
                    address,
                    0,
                    *operands
                        .first()
                        .ok_or_else(|| internal("StoreGlobal has no value"))?,
                )?;
                None
            }
            l::InstructionKind::AddressOfGlobal(global) => {
                let (address, _) = self.global_address(*global)?;
                Some(RV::Scalar(address))
            }
            l::InstructionKind::FunctionRef(function) => Some(self.function_reference(*function)?),
            l::InstructionKind::Unary(operator) => {
                let ty = data_type(
                    operand_types
                        .first()
                        .ok_or_else(|| internal("unary operand type is missing"))?,
                )?;
                Some(
                    self.unary(
                        *operator,
                        *operands
                            .first()
                            .ok_or_else(|| internal("unary operand is missing"))?,
                        ty,
                    )?,
                )
            }
            l::InstructionKind::Binary(operator) => {
                let ty = data_type(
                    operand_types
                        .first()
                        .ok_or_else(|| internal("binary operand type is missing"))?,
                )?;
                Some(
                    self.binary(
                        *operator,
                        *operands
                            .first()
                            .ok_or_else(|| internal("binary lhs is missing"))?,
                        *operands
                            .get(1)
                            .ok_or_else(|| internal("binary rhs is missing"))?,
                        ty,
                        &instruction.traps,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::Cast | l::InstructionKind::Coerce => {
                if matches!(
                    (operand_types.first(), result_ty.as_ref()),
                    (
                        Some(l::ValueType::Data(Type::Nullable(source))),
                        Some(l::ValueType::Data(Type::Class(target)))
                    ) if matches!(source.as_ref(), Type::Class(source)
                        if source == target && self.is_value_class(&Type::Class(*target)))
                ) {
                    let pointer = self.expect_scalar(
                        *operands
                            .first()
                            .ok_or_else(|| internal("conversion operand is missing"))?,
                    )?;
                    Some(
                        self.clone_value(
                            RV::Aggregate(pointer),
                            result_ty
                                .as_ref()
                                .ok_or_else(|| internal("conversion result type is missing"))?,
                        )?,
                    )
                } else {
                    let source = data_type(
                        operand_types
                            .first()
                            .ok_or_else(|| internal("conversion source type is missing"))?,
                    )?;
                    let target = data_type(
                        result_ty
                            .as_ref()
                            .ok_or_else(|| internal("conversion result type is missing"))?,
                    )?;
                    Some(
                        self.convert(
                            *operands
                                .first()
                                .ok_or_else(|| internal("conversion operand is missing"))?,
                            source,
                            target,
                            &instruction.traps,
                        )?,
                    )
                }
            }
            l::InstructionKind::AllocateClass(class) => {
                let stable_address = instruction.result.and_then(|result| {
                    self.stable_addresses
                        .get(&result)
                        .copied()
                        .zip(self.frame)
                        .map(|(offset, frame)| self.address_offset(frame, i64::from(offset)))
                });
                let value = self.allocate_class(
                    *class,
                    stable_address,
                    &instruction.traps,
                    &instruction.pos,
                )?;
                Some(match (result_ty.as_ref(), value) {
                    (Some(l::ValueType::Address(_)), RV::Aggregate(address)) => RV::Scalar(address),
                    (_, value) => value,
                })
            }
            l::InstructionKind::BoxBoundaryValue { payload } => Some(
                self.box_boundary_value(
                    *operands
                        .first()
                        .ok_or_else(|| internal("BoxBoundaryValue operand is missing"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("BoxBoundaryValue type is missing"))?,
                    *payload,
                    &instruction.traps,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::AddressOfValue => {
                let ty = data_type(
                    operand_types
                        .first()
                        .ok_or_else(|| internal("AddressOfValue type is missing"))?,
                )?;
                let (size, align) = self.ml.layouts.size_align(ty)?;
                let stable = instruction
                    .result
                    .and_then(|result| self.stable_addresses.get(&result).copied())
                    .zip(self.frame);
                let address = if let Some((offset, frame)) = stable {
                    self.address_offset(frame, i64::from(offset))
                } else {
                    self.stack_slot(size.max(1), align.max(1))
                };
                self.store_data(
                    ty,
                    address,
                    0,
                    *operands
                        .first()
                        .ok_or_else(|| internal("AddressOfValue operand is missing"))?,
                )?;
                Some(RV::Scalar(address))
            }
            l::InstructionKind::AddressOfField(field) => {
                let (address, _) = self.field_address(
                    *field,
                    *operands
                        .first()
                        .ok_or_else(|| internal("field base is missing"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("field base type is missing"))?,
                    &instruction.traps,
                )?;
                Some(RV::Scalar(address))
            }
            l::InstructionKind::AddressOfIndex { checked } => {
                let index = self.expect_scalar(
                    *operands
                        .get(1)
                        .ok_or_else(|| internal("index is missing"))?,
                )?;
                let index_ty = data_type(
                    operand_types
                        .get(1)
                        .ok_or_else(|| internal("index type is missing"))?,
                )?;
                let (address, _) = self.index_address(
                    *operands
                        .first()
                        .ok_or_else(|| internal("indexed base is missing"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("indexed base type is missing"))?,
                    index,
                    index_ty,
                    *checked,
                    &instruction.traps,
                )?;
                Some(RV::Scalar(address))
            }
            l::InstructionKind::LoadAddress => {
                let address = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("load address is missing"))?,
                )?;
                let ty = data_type(
                    result_ty
                        .as_ref()
                        .ok_or_else(|| internal("load result type is missing"))?,
                )?;
                let value = self.load_data(ty, address, 0)?;
                Some(self.clone_value(value, &l::ValueType::Data(ty.clone()))?)
            }
            l::InstructionKind::StoreAddress => {
                let address = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("store address is missing"))?,
                )?;
                let l::ValueType::Address(address_ty) = operand_types
                    .first()
                    .ok_or_else(|| internal("store address type is missing"))?
                else {
                    return Err(internal("StoreAddress operand is not an address"));
                };
                self.store_data(
                    &address_ty.pointee,
                    address,
                    0,
                    *operands
                        .get(1)
                        .ok_or_else(|| internal("stored value is missing"))?,
                )?;
                None
            }
            l::InstructionKind::LoadField(field) => {
                let (address, ty) = self.field_address(
                    *field,
                    *operands
                        .first()
                        .ok_or_else(|| internal("field base is missing"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("field base type is missing"))?,
                    &instruction.traps,
                )?;
                self.guard_json_result_value(
                    *field,
                    *operands
                        .first()
                        .ok_or_else(|| internal("field base is missing"))?,
                    &instruction.traps,
                )?;
                let value = self.load_data(&ty, address, 0)?;
                self.validate_wire_alias_traps(&ty, value, &instruction.traps)?;
                Some(self.clone_value(value, &l::ValueType::Data(ty))?)
            }
            l::InstructionKind::Length => Some(
                self.length(
                    *operands
                        .first()
                        .ok_or_else(|| internal("length operand is missing"))?,
                    operand_types
                        .first()
                        .ok_or_else(|| internal("length operand type is missing"))?,
                )?,
            ),
            l::InstructionKind::ForeignArrayData => {
                let handle = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("foreign array data has no array operand"))?,
                )?;
                let data = self
                    .call_runtime(self.ml.rt.array_data, &[self.ctx, handle], false)?
                    .ok_or_else(|| internal("foreign array data snapshot has no result"))?;
                Some(RV::Scalar(data))
            }
            l::InstructionKind::ArrayLiteral => Some(
                self.array_literal(
                    data_type(
                        result_ty
                            .as_ref()
                            .ok_or_else(|| internal("array result type is missing"))?,
                    )?,
                    &operands,
                    &instruction.traps,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::ArrayWithCapacity => Some(
                self.array_with_capacity(
                    data_type(
                        result_ty
                            .as_ref()
                            .ok_or_else(|| internal("capacity array result type is missing"))?,
                    )?,
                    *operands
                        .first()
                        .ok_or_else(|| internal("capacity array bound is missing"))?,
                    &instruction.traps,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::SetFromSource(spread) => Some(
                self.set_from_source(
                    data_type(
                        result_ty
                            .as_ref()
                            .ok_or_else(|| internal("Set source result type is missing"))?,
                    )?,
                    *spread,
                    &operands,
                    &operand_types,
                    &instruction.traps,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::ArraySpreadLiteral(spreads) => Some(
                self.spread_array_literal(
                    data_type(
                        result_ty
                            .as_ref()
                            .ok_or_else(|| internal("spread result type is missing"))?,
                    )?,
                    spreads,
                    &operands,
                    &operand_types,
                    &instruction.traps,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::Template(parts) => {
                Some(self.template(parts, &operands, &instruction.traps, &instruction.pos)?)
            }
            l::InstructionKind::MakeClosure(function) => {
                Some(self.make_closure(*function, &operands)?)
            }
            l::InstructionKind::Call(target) => Some(self.call(
                target,
                &operands,
                &operand_types,
                &instruction.traps,
                &instruction.pos,
            )?),
            l::InstructionKind::AsyncHandleCreate(target) => {
                let handle =
                    self.create_async_child_from_values(target, &operands, &instruction.traps)?;
                self.start_async_handle(target, handle)?;
                Some(RV::Scalar(handle))
            }
            l::InstructionKind::AsyncHandleRetain => {
                let frame = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("async retain has no handle"))?,
                )?;
                self.call_runtime(self.ml.rt.async_retain, &[self.ctx, frame], false)?;
                None
            }
            l::InstructionKind::AsyncHandleRelease => {
                let frame = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("async release has no handle"))?,
                )?;
                let pos = self.position_id(&instruction.pos);
                let pos = self.iconst(types::I32, pos);
                self.call_runtime(self.ml.rt.async_release, &[self.ctx, frame, pos], false)?;
                None
            }
            l::InstructionKind::AsyncHandleArrayRetain => {
                let array = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("async array retain has no array"))?,
                )?;
                self.call_runtime(self.ml.rt.async_retain_array, &[self.ctx, array], false)?;
                None
            }
            l::InstructionKind::AsyncHandleArrayRelease => {
                let array = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("async array release has no array"))?,
                )?;
                let pos = self.position_id(&instruction.pos);
                let pos = self.iconst(types::I32, pos);
                self.call_runtime(
                    self.ml.rt.async_release_array,
                    &[self.ctx, array, pos],
                    false,
                )?;
                None
            }
            l::InstructionKind::IteratorCreate { kind, bound } => {
                let iterator_ty = match result_ty.as_ref() {
                    Some(l::ValueType::Iterator(ty)) => ty,
                    _ => return Err(internal("IteratorCreate has no iterator result type")),
                };
                Some(
                    self.iterator_create(
                        *kind,
                        *bound,
                        *operands
                            .first()
                            .ok_or_else(|| internal("iterator subject is missing"))?,
                        operand_types
                            .first()
                            .ok_or_else(|| internal("iterator subject type is missing"))?,
                        iterator_ty,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::IteratorHasNext => {
                let iterator_ty = match operand_types.first() {
                    Some(l::ValueType::Iterator(ty)) => ty,
                    _ => return Err(internal("IteratorHasNext has no iterator type")),
                };
                Some(
                    self.iterator_has_next(
                        *operands
                            .first()
                            .ok_or_else(|| internal("iterator is missing"))?,
                        iterator_ty,
                        self.expect_scalar(
                            *operands
                                .get(1)
                                .ok_or_else(|| internal("iterator index is missing"))?,
                        )?,
                        self.expect_scalar(
                            *operands
                                .get(2)
                                .ok_or_else(|| internal("iterator bound is missing"))?,
                        )?,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::IteratorValue => {
                let iterator_ty = match operand_types.first() {
                    Some(l::ValueType::Iterator(ty)) => ty,
                    _ => return Err(internal("IteratorValue has no iterator type")),
                };
                Some(
                    self.iterator_value(
                        *operands
                            .first()
                            .ok_or_else(|| internal("iterator is missing"))?,
                        iterator_ty,
                        self.expect_scalar(
                            *operands
                                .get(1)
                                .ok_or_else(|| internal("iterator index is missing"))?,
                        )?,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::IteratorBound => {
                let iterator_ty = match operand_types.first() {
                    Some(l::ValueType::Iterator(ty)) => ty,
                    _ => return Err(internal("IteratorBound has no iterator type")),
                };
                Some(
                    self.iterator_bound(
                        *operands
                            .first()
                            .ok_or_else(|| internal("iterator is missing"))?,
                        iterator_ty,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::IteratorAdvance => {
                let iterator_ty = match operand_types.first() {
                    Some(l::ValueType::Iterator(ty)) => ty,
                    _ => return Err(internal("IteratorAdvance has no iterator type")),
                };
                Some(
                    self.iterator_advance(
                        *operands
                            .first()
                            .ok_or_else(|| internal("iterator is missing"))?,
                        iterator_ty,
                        self.expect_scalar(
                            *operands
                                .get(2)
                                .ok_or_else(|| internal("iterator bound is missing"))?,
                        )?,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::Zero => Some(
                self.zero(data_type(
                    result_ty
                        .as_ref()
                        .ok_or_else(|| internal("Zero result type is missing"))?,
                )?)?,
            ),
        };
        match (instruction.result, result) {
            (Some(id), Some(value)) => self.set_value(id, value),
            (None, None) => Ok(()),
            (Some(_), None) => Err(internal(format!(
                "{:?} declared a result but produced none",
                instruction.kind
            ))),
            (None, Some(RV::None)) => Ok(()),
            (None, Some(value)) => Err(internal(format!(
                "{:?} produced undeclared value {value:?}",
                instruction.kind
            ))),
        }
    }
}
