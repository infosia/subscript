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
                pos,
            )?,
            l::CallTargetKind::Method(method) => self.script_call(
                self.method_function(*method)?,
                operands,
                parameter_types,
                target.return_type.as_ref(),
                true,
            )?,
            l::CallTargetKind::Indirect => {
                self.indirect_call(operands, parameter_types, target.return_type.as_ref(), pos)?
            }
            l::CallTargetKind::Foreign(function) => self.foreign_call(
                *function,
                operands,
                parameter_types,
                (target.return_type.as_ref(), None),
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
        self.count_action = instruction.count_action.clone();
        let operands = self.instruction_operands(instruction)?;
        let traps = self.consume_lifetimes(&instruction.traps, &operands)?;
        let remaining;
        let instruction = if let std::borrow::Cow::Owned(traps) = traps {
            remaining = l::Instruction {
                traps,
                ..instruction.clone()
            };
            &remaining
        } else {
            instruction
        };
        let operand_types = self.instruction_operand_types(instruction)?;
        let result_ty = self.result_type(instruction)?;
        for trap in &instruction.traps {
            if trap.kind == l::TrapKind::GeneratorDoneValue {
                let base = *operands
                    .first()
                    .ok_or_else(|| internal("generator result is missing"))?;
                let base_type = operand_types
                    .first()
                    .ok_or_else(|| internal("generator result type is missing"))?;
                let (address, _) = self.field_address(l::FieldRef::IterDone, base, base_type)?;
                let done = self.builder.ins().load(types::I8, flags(), address, 0);
                self.emit_trap(trap, TrapOperand::Value(done))?;
            }
        }
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
            l::InstructionKind::Cast
            | l::InstructionKind::Coerce
            | l::InstructionKind::NarrowNonNull(_) => {
                if matches!(result_ty, Some(l::ValueType::Address(_))) {
                    let pointer = self.expect_scalar(
                        *operands
                            .first()
                            .ok_or_else(|| internal("conversion operand is missing"))?,
                    )?;
                    for trap in &instruction.traps {
                        self.emit_trap(trap, TrapOperand::Value(pointer))?;
                    }
                    Some(RV::Scalar(pointer))
                } else if matches!(
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
                    for trap in &instruction.traps {
                        if matches!(
                            trap.kind,
                            l::TrapKind::NullNarrowing | l::TrapKind::SharedNullNarrowing
                        ) {
                            self.emit_trap(trap, TrapOperand::Value(pointer))?;
                        }
                    }
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
            l::InstructionKind::MapFromSource => {
                let source = *operands
                    .first()
                    .ok_or_else(|| internal("Map copy source is missing"))?;
                let source = self.expect_scalar(source)?;
                let position = self.position_id(&instruction.pos);
                let position = self.iconst(types::I32, position);
                let handle = self
                    .call_runtime(
                        self.ml.rt.map_from_assoc,
                        &[self.ctx, source, position],
                        false,
                    )?
                    .ok_or_else(|| internal("Map copy result is missing"))?;
                for trap in &instruction.traps {
                    self.emit_trap(trap, TrapOperand::Pending)?;
                }
                Some(RV::Scalar(handle))
            }
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
            l::InstructionKind::TaskGroup(operation) => {
                let operation = self.iconst(types::I32, *operation as i64);
                let group = match operands.first() {
                    Some(value) => self.expect_scalar(*value)?,
                    None => self.iconst(types::I64, 0),
                };
                let input = match operands.get(1) {
                    Some(value) => self.expect_scalar(*value)?,
                    None => self.iconst(types::I64, 0),
                };
                let pos = self.position_id(&instruction.pos);
                let pos = self.iconst(types::I32, pos);
                let result = self.call_runtime(
                    self.ml.rt.task_group,
                    &[self.ctx, operation, group, input, pos],
                    true,
                )?;
                for trap in &instruction.traps {
                    self.emit_trap(trap, TrapOperand::Pending)?;
                }
                if instruction.result.is_some() {
                    result.map(RV::Scalar)
                } else {
                    None
                }
            }
            l::InstructionKind::AsyncAll => {
                let Some(l::ValueType::Data(Type::Array(input))) = operand_types.first() else {
                    return Err(internal("aggregate input type is missing"));
                };
                let Type::AsyncHandle(element) = &**input else {
                    return Err(internal("aggregate element type is missing"));
                };
                let size = if **element == Type::Void {
                    0
                } else {
                    self.ml.layouts.size_align(element)?.0
                };
                let size = self.builder.ins().iconst(types::I64, size as i64);
                let jobs = self.expect_scalar(operands[0])?;
                let position = self.position_id(&instruction.pos);
                let position = self.iconst(types::I32, position);
                let handle = self
                    .call_runtime(
                        self.ml.rt.async_all,
                        &[self.ctx, jobs, size, size, position],
                        true,
                    )?
                    .ok_or_else(|| internal("aggregate call has no result"))?;
                for trap in &instruction.traps {
                    self.emit_trap(trap, TrapOperand::Pending)?;
                }
                Some(RV::Scalar(handle))
            }
            l::InstructionKind::HostCompletion {
                function,
                result_size,
                is_void,
                error_metadata,
            } => {
                let endpoint = self.stack_slot(16, 8);
                let metadata = self.stack_slot(48, 8);
                for (index, word) in error_metadata.iter().enumerate() {
                    let word = self.builder.ins().iconst(types::I64, *word as i64);
                    self.builder
                        .ins()
                        .store(flags(), word, metadata, (index * 8) as i32);
                }
                let size = self.builder.ins().iconst(types::I64, *result_size as i64);
                let void = self.iconst(types::I32, i64::from(*is_void));
                let position = self.position_id(&instruction.pos);
                let position = self.iconst(types::I32, position);
                let handle = self
                    .call_runtime(
                        self.ml.rt.async_host_operation,
                        &[self.ctx, size, void, position, metadata, endpoint],
                        true,
                    )?
                    .ok_or_else(|| internal("host source call has no result"))?;
                for trap in &instruction.traps {
                    if trap.kind == l::TrapKind::Allocation {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                }
                self.foreign_call(
                    *function,
                    &operands,
                    &operand_types,
                    (None, Some(endpoint)),
                    &instruction.traps,
                    &instruction.pos,
                )?;
                Some(RV::Scalar(handle))
            }
            l::InstructionKind::AsyncHandleCreate(target) => {
                let handle =
                    self.create_async_child_from_values(target, &operands, &instruction.traps)?;
                self.start_async_handle(target, handle, &instruction.pos)?;
                Some(RV::Scalar(handle))
            }
            l::InstructionKind::AsyncHandleRetain => {
                let frame = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("async retain has no handle"))?,
                )?;
                self.async_count(frame, None)?;
                instruction.result.map(|_| RV::Scalar(frame))
            }
            l::InstructionKind::AsyncHandleRelease => {
                let frame = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("async release has no handle"))?,
                )?;
                self.async_count(frame, Some((&instruction.pos, &instruction.traps)))?;
                None
            }
            l::InstructionKind::AsyncHandleArrayRetain
            | l::InstructionKind::AsyncHandleArrayRelease => {
                let ty = operand_types
                    .first()
                    .and_then(|ty| match ty {
                        l::ValueType::Data(ty) => Some(ty),
                        _ => None,
                    })
                    .ok_or_else(|| internal("counted owner has no data type"))?;
                let release = matches!(
                    instruction.kind,
                    l::InstructionKind::AsyncHandleArrayRelease
                );
                // An array acquire needs no element description. The handle-array
                // release wrapper supplies its static leaf description.
                let array_wrapper = matches!(ty, Type::Array(_)) && !release
                    || matches!(ty, Type::Array(element) if matches!(&**element, Type::AsyncHandle(_)));
                if array_wrapper {
                    let array = self.expect_scalar(
                        *operands
                            .first()
                            .ok_or_else(|| internal("counted owner has no operand"))?,
                    )?;
                    if release {
                        let pos = self.position_id(&instruction.pos);
                        let pos = self.iconst(types::I32, pos);
                        self.call_runtime(
                            self.ml.rt.async_release_array,
                            &[self.ctx, array, pos],
                            false,
                        )?;
                        for trap in &instruction.traps {
                            self.emit_trap(trap, TrapOperand::Pending)?;
                        }
                    } else {
                        self.call_runtime(
                            self.ml.rt.async_retain_array,
                            &[self.ctx, array],
                            false,
                        )?;
                    }
                    if let Some(result) = instruction.result {
                        self.set_value(result, RV::Scalar(array))?;
                    }
                    return Ok(());
                }
                let description = crate::counted::description(&self.ml.layouts, ty)?;
                let data = self.ml.literal_data(&description)?;
                let global = self.ml.module.declare_data_in_func(data, self.builder.func);
                let description = self.builder.ins().symbol_value(types::I64, global);
                let value = match operands
                    .first()
                    .copied()
                    .ok_or_else(|| internal("counted owner has no operand"))?
                {
                    RV::Aggregate(address) => address,
                    RV::Scalar(value) => {
                        let address = self.stack_slot(8, 8);
                        self.builder
                            .ins()
                            .store(MemFlags::trusted(), value, address, 0);
                        address
                    }
                    _ => return Err(internal("counted owner has no representation")),
                };
                let operation = self.iconst(types::I32, i64::from(release));
                let pos = self.position_id(&instruction.pos);
                let pos = self.iconst(types::I32, pos);
                self.call_runtime(
                    self.ml.rt.counted_value,
                    &[self.ctx, value, description, operation, pos],
                    false,
                )?;
                if release {
                    for trap in &instruction.traps {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                }
                instruction.result.and_then(|_| operands.first().copied())
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
            l::InstructionKind::Throw => {
                self.emit_throw(&operands, &instruction.pos)?;
                None
            }
            l::InstructionKind::CatchEntry => {
                self.emit_catch_entry(instruction.result.is_some())?
            }
            l::InstructionKind::FinalizerEnter(_) | l::InstructionKind::GeneratorFinalizer(_) => {
                None
            }
            l::InstructionKind::GeneratorIsClosing => {
                let output = self
                    .out
                    .ok_or_else(|| internal("generator close flag has no output"))?;
                let closing = self.builder.ins().icmp_imm(IntCC::Equal, output, 0);
                Some(RV::Scalar(closing))
            }
            l::InstructionKind::GeneratorClose => {
                let frame = self.expect_scalar(operands[0])?;
                let state = self.builder.ins().load(types::I32, flags(), frame, 0);
                let active = self
                    .builder
                    .ins()
                    .icmp_imm(IntCC::SignedGreaterThan, state, 0);
                let close = self.builder.create_block();
                let done = self.builder.create_block();
                let initial = self.builder.ins().icmp_imm(IntCC::Equal, state, 0);
                let exhausted = self.builder.ins().iconst(types::I32, -1);
                let updated = self.builder.ins().select(initial, exhausted, state);
                self.builder.ins().store(flags(), updated, frame, 0);
                self.builder.ins().brif(active, close, &[], done, &[]);
                self.builder.switch_to_block(close);
                let resume =
                    self.builder
                        .ins()
                        .load(types::I64, flags(), frame, COROUTINE_RESUME_OFFSET);
                let signature = self.builder.import_signature(self.ml.resume_sig());
                let l::ValueType::Data(Type::Generator(ty)) = &operand_types[0] else {
                    return Err(internal("generator close type is missing"));
                };
                let (size, align) = self.ml.layouts.size_align(ty)?;
                let result = self.stack_slot(size.max(1), align.max(1));
                self.zero_bytes(result, size.max(1), align.max(1));
                self.builder.ins().store(
                    flags(),
                    result,
                    frame,
                    subscript_runtime::generator_layout::CLOSE_OUTPUT_OFFSET as i32,
                );
                let output = self.builder.ins().iconst(types::I64, 0);
                let call =
                    self.builder
                        .ins()
                        .call_indirect(signature, resume, &[self.ctx, frame, output]);
                let completed = self.builder.inst_results(call)[0];
                self.builder.ins().store(
                    flags(),
                    output,
                    frame,
                    subscript_runtime::generator_layout::CLOSE_OUTPUT_OFFSET as i32,
                );
                if ty.counted_type().is_some() {
                    let release = self.builder.create_block();
                    self.builder.ins().brif(completed, done, &[], release, &[]);
                    self.builder.switch_to_block(release);
                    let bytes = crate::counted::description(&self.ml.layouts, ty)?;
                    let data = self.ml.literal_data(&bytes)?;
                    let global = self.ml.module.declare_data_in_func(data, self.builder.func);
                    let description = self.builder.ins().symbol_value(types::I64, global);
                    let operation = self.iconst(types::I32, 1);
                    let pos = self.position_id(&instruction.pos);
                    let pos = self.iconst(types::I32, pos);
                    self.call_runtime(
                        self.ml.rt.counted_value,
                        &[self.ctx, result, description, operation, pos],
                        false,
                    )?;
                }
                self.builder.ins().jump(done, &[]);
                self.builder.switch_to_block(done);
                self.trap_check();
                None
            }
            l::InstructionKind::ExceptionMessage => self
                .call_runtime(self.ml.rt.exception_message, &[self.ctx], false)?
                .map(RV::Scalar),
            l::InstructionKind::ExceptionPosition => self
                .call_runtime(self.ml.rt.exception_position, &[self.ctx], false)?
                .map(RV::Scalar),
            l::InstructionKind::ExceptionRestore => {
                let mut args = vec![self.ctx];
                args.extend(
                    operands
                        .iter()
                        .map(|value| self.expect_scalar(*value))
                        .collect::<Result<Vec<_>, _>>()?,
                );
                self.call_runtime(self.ml.rt.exception_restore, &args, true)?;
                None
            }
            l::InstructionKind::ExceptionPark => {
                self.call_runtime(self.ml.rt.exception_park, &[self.ctx], false)?;
                None
            }
            l::InstructionKind::ExceptionResume => {
                self.call_runtime(self.ml.rt.exception_resume, &[self.ctx], true)?;
                None
            }
            // compiler.md §116.2 rule 3: the resume made the exception of an
            // exception completion pending; the raise site checks it.
            l::InstructionKind::AwaitRaise => {
                self.consumed_traps.extend(
                    instruction
                        .traps
                        .iter()
                        .filter(|trap| trap.kind == l::TrapKind::Call)
                        .cloned(),
                );
                self.trap_check();
                None
            }
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
