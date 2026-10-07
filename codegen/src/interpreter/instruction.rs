//! Instruction execution, operands, and trap dispatch.

use super::*;

impl Interpreter<'_> {
    pub(super) fn execute_instruction(
        &mut self,
        frame: *mut Frame,
        function: &l::Function,
        instruction: &l::Instruction,
    ) -> Result<(), InterpretError> {
        let enclosing_traps = std::mem::replace(&mut self.active_traps, instruction.traps.clone());
        let action = std::mem::replace(&mut self.count_action, instruction.count_action.clone());
        let outcome = self.execute_instruction_effect(frame, function, instruction);
        self.count_action = action;
        self.active_traps = enclosing_traps;
        outcome
    }

    fn execute_instruction_effect(
        &mut self,
        frame: *mut Frame,
        function: &l::Function,
        instruction: &l::Instruction,
    ) -> Result<(), InterpretError> {
        // SAFETY: execute_frame owns the registered frame for this call.
        // Operand and local references end before a call can collect.
        // The result write starts after all calls return.
        let operand_types = instruction
            .operands
            .iter()
            .map(|operand| match operand {
                l::Operand::Constant(constant) => Ok(l::ValueType::Data(constant.ty.clone())),
                l::Operand::Value(value) => function
                    .values
                    .get(value.0 as usize)
                    .map(|value| value.ty.clone())
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("operand value {} has no type", value.0),
                        )
                    }),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let operands = instruction
            .operands
            .iter()
            .map(|operand| self.operand(unsafe { &*frame }, operand, &instruction.pos))
            .collect::<Result<Vec<_>, _>>()?;
        let result_ty = instruction
            .result
            .and_then(|value| function.values.get(value.0 as usize))
            .map(|value| &value.ty);
        self.dispatch_instruction_traps(function, instruction, &operands, None, TrapPhase::Before)?;
        let result = match &instruction.kind {
            l::InstructionKind::Throw => {
                let text = |index: usize| -> Result<Vec<u8>, InterpretError> {
                    let handle = operands
                        .get(index)
                        .ok_or_else(|| self.missing_operand(instruction, index))?
                        .as_handle()?;
                    self.string_bytes(handle)
                };
                let message = subscript_runtime::exception::exception_message(&text(1)?, &text(2)?);
                let object = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_handle()?;
                return Err(InterpretError::Exception {
                    object: object as usize,
                    message,
                    pos: instruction.pos.clone(),
                });
            }
            l::InstructionKind::CatchEntry => {
                let caught = self.caught.take().ok_or_else(|| {
                    self.invalid(
                        Some(instruction.pos.clone()),
                        "a catch entry ran with no exception on its edge",
                    )
                })?;
                // A callback bridge stopped its runtime loop with the
                // exception state; the handler clears it.
                if self.context.exception_pending() {
                    self.context.catch_exception();
                }
                instruction
                    .result
                    .map(|_| Value::Handle(caught.0 as *mut u8))
            }
            l::InstructionKind::ExceptionPark => {
                let caught = self.caught.take().ok_or_else(|| {
                    self.invalid(
                        Some(instruction.pos.clone()),
                        "an exception exit ran with no exception on its edge",
                    )
                })?;
                if self.context.exception_pending() {
                    self.context.catch_exception();
                }
                self.parked.push(caught);
                None
            }
            // compiler.md §116.2 rule 3: the resume of an exception
            // completion delivered its exception; this raise site raises it.
            l::InstructionKind::AwaitRaise => {
                if let Some((object, message, pos)) = unsafe { &mut (*frame).delivered }.take() {
                    return Err(InterpretError::Exception {
                        object,
                        message,
                        pos,
                    });
                }
                None
            }
            l::InstructionKind::ExceptionResume => {
                let (object, message, pos) = self.parked.pop().ok_or_else(|| {
                    self.invalid(
                        Some(instruction.pos.clone()),
                        "an exception exit resumed with no parked exception",
                    )
                })?;
                return Err(InterpretError::Exception {
                    object,
                    message,
                    pos,
                });
            }
            l::InstructionKind::Copy => Some(
                self.copy_value(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    result_ty,
                )?,
            ),
            l::InstructionKind::StringLiteral(text) => Some(Value::Handle(
                self.alloc_string(text.as_bytes(), &instruction.pos)?,
            )),
            l::InstructionKind::Zero => Some(match result_ty {
                Some(l::ValueType::Data(ty)) => self.zero(ty),
                _ => {
                    return Err(
                        self.invalid(Some(instruction.pos.clone()), "Zero has no data result")
                    );
                }
            }),
            l::InstructionKind::LoadLocal(local) => {
                let stored = unsafe { &(*frame).locals }
                    .get(local.0 as usize)
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("local {} is missing", local.0),
                        )
                    })?;
                if let Some((suspend, suspend_pos)) = &stored.poisoned_at {
                    let name = function
                        .locals
                        .get(local.0 as usize)
                        .map_or("<missing>", |local| local.source_name.as_str());
                    return Err(self.invalid(
                    Some(instruction.pos.clone()),
                    format!(
                        "activation local {} (`{name}`) was loaded after suspend in block {} at {suspend_pos}",
                        local.0, suspend.0
                    ),
                ));
                }
                Some(stored.slot().borrow().clone())
            }
            l::InstructionKind::StoreLocal(local) => {
                let value = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                let stored = unsafe { &mut (*frame).locals }
                    .get_mut(local.0 as usize)
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("local {} is missing", local.0),
                        )
                    })?;
                *stored.slot().borrow_mut() = value.clone();
                stored.poisoned_at = None;
                None
            }
            l::InstructionKind::AddressOfLocal(local) => {
                let stored = unsafe { &(*frame).locals }
                    .get(local.0 as usize)
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("local {} is missing", local.0),
                        )
                    })?;
                if let Some((suspend, suspend_pos)) = &stored.poisoned_at {
                    let name = function
                        .locals
                        .get(local.0 as usize)
                        .map_or("<missing>", |local| local.source_name.as_str());
                    return Err(self.invalid(
                    Some(instruction.pos.clone()),
                    format!(
                        "activation local {} (`{name}`) was loaded after suspend in block {} at {suspend_pos}",
                        local.0, suspend.0
                    ),
                ));
                }
                Some(Value::Address(Address {
                    target: AddressTarget::Slot(stored.slot().clone()),
                    pointee: self.address_pointee(result_ty, instruction)?,
                    poison: Rc::new(RefCell::new(None)),
                }))
            }
            l::InstructionKind::LoadGlobal(global) => Some(
                self.globals
                    .get(global.0 as usize)
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("global {} is missing", global.0),
                        )
                    })?
                    .borrow()
                    .clone(),
            ),
            l::InstructionKind::StoreGlobal(global) => {
                let value = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                *self
                    .globals
                    .get(global.0 as usize)
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            format!("global {} is missing", global.0),
                        )
                    })?
                    .borrow_mut() = value.clone();
                None
            }
            l::InstructionKind::AddressOfGlobal(global) => Some(Value::Address(Address {
                target: AddressTarget::Slot(
                    self.globals
                        .get(global.0 as usize)
                        .ok_or_else(|| {
                            self.invalid(
                                Some(instruction.pos.clone()),
                                format!("global {} is missing", global.0),
                            )
                        })?
                        .clone(),
                ),
                pointee: self.address_pointee(result_ty, instruction)?,
                poison: Rc::new(RefCell::new(None)),
            })),
            l::InstructionKind::FunctionRef(function) => Some(Value::Callable(Rc::new(Callable {
                function: *function,
                captures: Vec::new(),
            }))),
            l::InstructionKind::MakeClosure(function) => Some(Value::Callable(Rc::new(Callable {
                function: *function,
                captures: operands,
            }))),
            l::InstructionKind::Unary(operator) => Some(
                self.unary(
                    *operator,
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    result_ty,
                )?,
            ),
            l::InstructionKind::Binary(operator) => Some(
                self.binary(
                    *operator,
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    operands
                        .get(1)
                        .ok_or_else(|| self.missing_operand(instruction, 1))?,
                    result_ty,
                    function,
                    instruction,
                )?,
            ),
            l::InstructionKind::Cast
            | l::InstructionKind::Coerce
            | l::InstructionKind::NarrowNonNull(_) => Some(
                self.convert(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    result_ty,
                    self.instruction_operand_type(function, instruction, 0),
                    instruction,
                )?,
            ),
            l::InstructionKind::AllocateClass(class) => {
                Some(self.allocate_class(*class, result_ty, &instruction.pos)?)
            }
            l::InstructionKind::BoxBoundaryValue { payload } => Some(
                self.box_boundary_value(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    *payload,
                    result_ty,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::AddressOfValue => {
                let value = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .clone();
                Some(Value::Address(Address {
                    target: AddressTarget::Slot(Rc::new(RefCell::new(value))),
                    pointee: self.address_pointee(result_ty, instruction)?,
                    poison: Rc::new(RefCell::new(None)),
                }))
            }
            l::InstructionKind::AddressOfField(field) => {
                let base = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                Some(Value::Address(self.address_field(
                    base,
                    *field,
                    result_ty,
                    instruction,
                )?))
            }
            l::InstructionKind::AddressOfIndex { checked: _ } => {
                let base = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                let index = operands
                    .get(1)
                    .ok_or_else(|| self.missing_operand(instruction, 1))?
                    .as_i64()?;
                Some(Value::Address(self.address_index(
                    base,
                    index,
                    result_ty,
                    instruction,
                )?))
            }
            l::InstructionKind::LoadAddress => {
                let address = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_address()?;
                address.check(&instruction.kind)?;
                Some(self.load_address(address)?)
            }
            l::InstructionKind::StoreAddress => {
                let address = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_address()?;
                address.check(&instruction.kind)?;
                let value = operands
                    .get(1)
                    .ok_or_else(|| self.missing_operand(instruction, 1))?;
                self.store_address(address, value)?;
                None
            }
            l::InstructionKind::LoadField(field) => {
                let base = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                Some(match base {
                    Value::Blob(bytes) => {
                        let pointee = self.data_result_type(result_ty, instruction)?;
                        let offset = self.field_offset(*field, pointee, instruction)?;
                        self.unpack(pointee, bytes.get(offset..).unwrap_or_default())?
                    }
                    _ => {
                        let address = self.address_field(base, *field, result_ty, instruction)?;
                        self.load_address(&address)?
                    }
                })
            }
            l::InstructionKind::Length => Some(Value::I(
                self.length(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    self.instruction_operand_type(function, instruction, 0),
                )? as i64,
            )),
            l::InstructionKind::ForeignArrayData => {
                let handle = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_handle()?;
                // SAFETY: verified LIR supplies a live dynamic-array handle.
                let data = unsafe { ffi::subscript_rt_array_data(&*self.context, handle) };
                Some(Value::Address(Address {
                    target: AddressTarget::Pointer(data.cast_mut()),
                    pointee: self.address_pointee(result_ty, instruction)?,
                    poison: Rc::new(RefCell::new(None)),
                }))
            }
            l::InstructionKind::ArrayLiteral => {
                let ty = self.data_result_type(result_ty, instruction)?;
                Some(self.array_literal(ty, &operands, &instruction.pos)?)
            }
            l::InstructionKind::ArrayWithCapacity => {
                let ty = self.data_result_type(result_ty, instruction)?;
                let capacity = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_i64()?;
                Some(self.array_with_capacity(ty, capacity, &instruction.pos)?)
            }
            l::InstructionKind::ArraySpreadLiteral(parts) => {
                let ty = self.data_result_type(result_ty, instruction)?;
                Some(self.array_spread_literal(ty, parts, &operands, &instruction.pos)?)
            }
            l::InstructionKind::MapFromSource => {
                let source = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_handle()?;
                // SAFETY: the verified source is a live Map in this Context.
                let handle =
                    unsafe { ffi::subscript_rt_map_from_assoc(&mut *self.context, source, 0) };
                self.check_runtime(&instruction.pos)?;
                if let Some(value_ty) = instruction
                    .count_action
                    .as_ref()
                    .and_then(l::CountAction::release_type)
                {
                    self.describe_interpreter_map(handle, &value_ty, &instruction.pos)?;
                    let values = self.counted_map_values(handle, &value_ty)?;
                    for value in values {
                        self.counted_owner(&value_ty, &value, false, &instruction.pos)?;
                    }
                }

                Some(Value::Handle(handle))
            }
            l::InstructionKind::SetFromSource(spread) => {
                let ty = self.data_result_type(result_ty, instruction)?.clone();
                Some(
                    self.set_from_source(
                        &ty,
                        *spread,
                        operands
                            .first()
                            .ok_or_else(|| self.missing_operand(instruction, 0))?,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::Template(parts) => Some(Value::Handle(self.template(
                parts,
                &operands,
                instruction,
            )?)),
            l::InstructionKind::Call(target) => Some(self.invoke_target(
                target,
                operands,
                Some(&operand_types),
                Some(&instruction.pos),
            )?),
            l::InstructionKind::TaskGroup(operation) => {
                self.task_group_operation(*operation, &operands, &instruction.pos)?
            }
            l::InstructionKind::AsyncAll => {
                let jobs = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_handle()?;
                let Some(l::ValueType::Data(Type::Array(input))) = operand_types.first() else {
                    return Err(self.invalid(
                        Some(instruction.pos.clone()),
                        "aggregate input type is missing",
                    ));
                };
                let Type::AsyncHandle(element) = &**input else {
                    return Err(self.invalid(
                        Some(instruction.pos.clone()),
                        "aggregate element type is missing",
                    ));
                };
                Some(self.async_all(jobs, element, &instruction.pos)?)
            }
            l::InstructionKind::AsyncHandleCreate(target) => {
                let value = self.invoke_target(
                    target,
                    operands,
                    Some(&operand_types),
                    Some(&instruction.pos),
                )?;
                let Value::Coroutine(handle) = &value else {
                    return Err(self.invalid(
                        Some(instruction.pos.clone()),
                        "async creation did not return a handle",
                    ));
                };
                // §94.1 rule 1: the call runs the body to its first await or
                // return. The child becomes no runnable job here, and the
                // caller does not suspend.
                let handle = Rc::clone(handle);
                #[cfg(test)]
                {
                    handle.borrow_mut().create_pos = instruction.pos.clone();
                }
                self.async_start(&handle)?;
                Some(value)
            }
            l::InstructionKind::AsyncHandleRetain => {
                let Value::Coroutine(handle) = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                else {
                    return Err(self.invalid(
                        Some(instruction.pos.clone()),
                        "async retain operand is not a handle",
                    ));
                };
                let mut handle = handle.borrow_mut();
                handle.owners = handle.owners.saturating_add(1);
                instruction.result.and_then(|_| operands.first().cloned())
            }
            l::InstructionKind::AsyncHandleRelease => {
                let value = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                match value {
                    Value::Coroutine(handle) => self.release_coroutine(handle, &instruction.pos)?,
                    Value::Null => {}
                    _ => {
                        return Err(self.invalid(
                            Some(instruction.pos.clone()),
                            "async release operand is not a handle",
                        ));
                    }
                }
                None
            }
            l::InstructionKind::AsyncHandleArrayRetain
            | l::InstructionKind::AsyncHandleArrayRelease => {
                let ty = match operand_types.first() {
                    Some(l::ValueType::Data(ty)) => ty,
                    _ => {
                        return Err(self.invalid(
                            Some(instruction.pos.clone()),
                            "counted owner has no data type",
                        ))
                    }
                };
                let value = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?;
                self.counted_owner(
                    ty,
                    value,
                    matches!(
                        instruction.kind,
                        l::InstructionKind::AsyncHandleArrayRelease
                    ),
                    &instruction.pos,
                )?;
                instruction.result.map(|_| value.clone())
            }
            l::InstructionKind::IteratorCreate { kind, bound } => {
                let subject_ty = self
                    .instruction_operand_type(function, instruction, 0)
                    .cloned();
                Some(
                    self.iterator_create(
                        *kind,
                        *bound,
                        operands
                            .first()
                            .ok_or_else(|| self.missing_operand(instruction, 0))?
                            .clone(),
                        subject_ty.as_ref(),
                        result_ty,
                        &instruction.pos,
                    )?,
                )
            }
            l::InstructionKind::IteratorBound => Some(Value::I(
                self.iterator_bound(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    &instruction.pos,
                )? as i64,
            )),
            l::InstructionKind::IteratorHasNext => {
                let index = operands
                    .get(1)
                    .ok_or_else(|| self.missing_operand(instruction, 1))?
                    .as_i64()?;
                let bound = operands
                    .get(2)
                    .ok_or_else(|| self.missing_operand(instruction, 2))?
                    .as_i64()?;
                let cursor = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_iterator()?;
                Some(Value::Bool(self.iterator_has_next(
                    cursor,
                    index,
                    bound,
                    &instruction.pos,
                )?))
            }
            l::InstructionKind::IteratorValue => Some(
                self.iterator_value(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    operands
                        .get(1)
                        .ok_or_else(|| self.missing_operand(instruction, 1))?
                        .as_i64()?,
                    result_ty,
                    &instruction.pos,
                )?,
            ),
            l::InstructionKind::IteratorAdvance => Some(
                self.iterator_advance(
                    operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?,
                    operands
                        .get(2)
                        .ok_or_else(|| self.missing_operand(instruction, 2))?
                        .as_i64()?,
                    &instruction.pos,
                )?,
            ),
        };
        self.dispatch_instruction_traps(
            function,
            instruction,
            &[],
            result.as_ref(),
            TrapPhase::After,
        )?;
        if let Some(id) = instruction.result {
            let result = result.ok_or_else(|| {
                self.invalid(
                    Some(instruction.pos.clone()),
                    format!("{:?} declares a result but produced none", instruction.kind),
                )
            })?;
            if let Some(l::ValueType::Address(address_ty)) = result_ty {
                if let (Some(base), Value::Address(address)) = (address_ty.array_base, &result) {
                    self.poison_registry
                        .entry(base)
                        .or_default()
                        .push(Rc::downgrade(&address.poison));
                }
            }
            self.set_value(
                unsafe { &mut (*frame).values },
                id,
                result,
                &instruction.pos,
            )?;
        }
        Ok(())
    }

    pub(super) fn operand(
        &self,
        frame: &Frame,
        operand: &l::Operand,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        match operand {
            l::Operand::Value(value) => self.get_value(frame, *value, pos),
            l::Operand::Constant(value) => self.constant(value),
        }
    }

    pub(super) fn instruction_operand_type<'a>(
        &'a self,
        function: &'a l::Function,
        instruction: &'a l::Instruction,
        index: usize,
    ) -> Option<&'a Type> {
        match instruction.operands.get(index)? {
            l::Operand::Constant(constant) => Some(&constant.ty),
            l::Operand::Value(value) => match &function.values.get(value.0 as usize)?.ty {
                l::ValueType::Data(ty) => Some(ty),
                l::ValueType::Address(address) => Some(&address.pointee),
                l::ValueType::Iterator(_) => None,
            },
        }
    }

    pub(super) fn constant(&self, constant: &l::Constant) -> Result<Value, InterpretError> {
        Ok(match (&constant.ty, &constant.kind) {
            (
                Type::I8
                | Type::I16
                | Type::I32
                | Type::I64
                | Type::Enum(_)
                | Type::StringAlias(_)
                | Type::Date,
                l::ConstantKind::Integer(v),
            ) => Value::I(*v),
            (Type::U8 | Type::U16 | Type::U32 | Type::U64, l::ConstantKind::Integer(v)) => {
                Value::U(*v as u64)
            }
            (Type::F32, l::ConstantKind::FloatBits(v)) => Value::F32(f32::from_bits(*v as u32)),
            (Type::F64, l::ConstantKind::FloatBits(v)) => Value::F64(f64::from_bits(*v)),
            (Type::F16, l::ConstantKind::FloatBits(v)) => {
                Value::U(ffi::subscript_rt_f16_from_f64(f64::from_bits(*v)) as u64)
            }
            (Type::Bool, l::ConstantKind::Boolean(v)) => Value::Bool(*v),
            (_, l::ConstantKind::Null) => Value::Null,
            _ => {
                return Err(self.invalid(
                    None,
                    format!("constant payload disagrees with type {:?}", constant.ty),
                ));
            }
        })
    }

    pub(super) fn get_value(
        &self,
        frame: &Frame,
        id: l::ValueId,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        frame
            .values
            .get(id.0 as usize)
            .and_then(Clone::clone)
            .ok_or_else(|| {
                self.invalid(
                    Some(pos.clone()),
                    format!("value %{} is not present in the current live-in set", id.0),
                )
            })
    }

    pub(super) fn set_value(
        &self,
        values: &mut [Option<Value>],
        id: l::ValueId,
        value: Value,
        pos: &Pos,
    ) -> Result<(), InterpretError> {
        let slot = values.get_mut(id.0 as usize).ok_or_else(|| {
            self.invalid(Some(pos.clone()), format!("value %{} is missing", id.0))
        })?;
        *slot = Some(value);
        Ok(())
    }

    pub(super) fn invalidate(
        &mut self,
        bases: &[l::ValueId],
        pos: &Pos,
        instruction: impl FnOnce() -> String,
    ) {
        let mut instruction = Some(instruction);
        let mut description = None;
        for base in bases {
            if let Some(addresses) = self.poison_registry.get_mut(base) {
                addresses.retain(|address| {
                    if let Some(address) = address.upgrade() {
                        let mut poison = address.borrow_mut();
                        if poison.is_none() {
                            let description = description.get_or_insert_with(|| {
                                instruction
                                    .take()
                                    .expect("invalidation description used once")(
                                )
                            });
                            *poison = Some(Invalidation {
                                instruction: description.clone(),
                                pos: pos.clone(),
                            });
                        }
                        true
                    } else {
                        false
                    }
                });
            }
        }
    }

    pub(super) fn invalid(&self, pos: Option<Pos>, message: impl Into<String>) -> InterpretError {
        InterpretError::InvalidLir {
            message: message.into(),
            pos,
        }
    }

    fn missing_operand(&self, instruction: &l::Instruction, index: usize) -> InterpretError {
        self.invalid(
            Some(instruction.pos.clone()),
            format!("{:?} is missing operand {index}", instruction.kind),
        )
    }

    /// Evaluates every checker-owned trap site in declaration order. Operand
    /// predicates run before the instruction effect; result and pending
    /// runtime predicates complete through the same dispatch after the
    /// effect. No instruction-kind arm owns an individual trap check.
    fn dispatch_instruction_traps(
        &mut self,
        function: &l::Function,
        instruction: &l::Instruction,
        operands: &[Value],
        result: Option<&Value>,
        phase: TrapPhase,
    ) -> Result<(), InterpretError> {
        for trap in &instruction.traps {
            let fired = match (&trap.kind, phase) {
                (
                    l::TrapKind::Allocation | l::TrapKind::Call | l::TrapKind::DevOnlyRelease(_),
                    TrapPhase::After,
                ) => {
                    if let Some(runtime) = self.context.trap_record() {
                        return Err(InterpretError::Trap {
                            kind: runtime.kind.rule().to_string(),
                            runtime_kind: Some(runtime.kind),
                            pos: trap.pos.clone(),
                            message: runtime.message.clone(),
                        });
                    }
                    false
                }
                // The instruction's own effect raises; the frame loop takes
                // the edge (compiler.md §115.6 rule 5).
                (l::TrapKind::Raise(_), _) => false,
                (l::TrapKind::GeneratorDoneValue, TrapPhase::Before) => {
                    let base = operands
                        .first()
                        .ok_or_else(|| self.missing_operand(instruction, 0))?;
                    match base {
                        Value::Blob(bytes) => self.unpack(&Type::Bool, bytes)?.as_bool()?,
                        _ => {
                            let address = self.address_field(
                                base,
                                l::FieldRef::IterDone,
                                Some(&l::ValueType::Data(Type::Bool)),
                                instruction,
                            )?;
                            self.load_address(&address)?.as_bool()?
                        }
                    }
                }
                (l::TrapKind::Unreachable, TrapPhase::Before) => true,
                (l::TrapKind::DivisionByZero, TrapPhase::Before) => operands
                    .get(1)
                    .is_some_and(|divisor| divisor.as_u64().is_ok_and(|value| value == 0)),
                (l::TrapKind::IndexRead | l::TrapKind::IndexWrite, TrapPhase::Before) => {
                    let index = operands
                        .get(1)
                        .ok_or_else(|| self.missing_operand(instruction, 1))?
                        .as_i64()?;
                    let length = self.indexed_length(function, instruction, operands)?;
                    index < 0 || index >= length
                }
                (
                    l::TrapKind::NullNarrowing | l::TrapKind::SharedNullNarrowing,
                    TrapPhase::Before,
                ) => operands.first().is_some_and(|value| match value {
                    Value::Null => true,
                    Value::Handle(handle) => handle.is_null(),
                    _ => false,
                }),
                (l::TrapKind::ClassMismatch(class), TrapPhase::Before) => operands
                    .first()
                    .is_some_and(|value| !self.value_has_class(value, *class)),
                (
                    l::TrapKind::DevOnlyLifetime(index) | l::TrapKind::DevOnlyRelease(index),
                    TrapPhase::Before,
                ) => {
                    let value = operands
                        .get(*index)
                        .ok_or_else(|| self.missing_operand(instruction, *index))?;
                    matches!(value, Value::Handle(handle)
                        if !handle.is_null() && !self.context.is_live(*handle as usize))
                }
                // The reference interpreter does not hot-reload a module, so
                // a frame created by this run cannot have a stale epoch.
                (l::TrapKind::DevReloadOnlyStaleCoroutine, TrapPhase::Before) => false,
                (l::TrapKind::WireEnumValue(alias), TrapPhase::After) => {
                    let definition = self
                        .module
                        .string_aliases
                        .get(alias.0)
                        .filter(|definition| definition.id == *alias)
                        .ok_or_else(|| {
                            self.invalid(
                                Some(trap.pos.clone()),
                                format!("wire string alias {} is missing", alias.0),
                            )
                        })?;
                    let wire = result
                        .ok_or_else(|| {
                            self.invalid(
                                Some(trap.pos.clone()),
                                "wire-enum trap has no instruction result",
                            )
                        })?
                        .as_i64()?;
                    !definition
                        .wire_values
                        .as_ref()
                        .is_some_and(|values| values.iter().any(|value| i64::from(*value) == wire))
                }
                (
                    l::TrapKind::Allocation
                    | l::TrapKind::Call
                    | l::TrapKind::Unreachable
                    | l::TrapKind::GeneratorDoneValue
                    | l::TrapKind::DivisionByZero
                    | l::TrapKind::IndexRead
                    | l::TrapKind::IndexWrite
                    | l::TrapKind::NullNarrowing
                    | l::TrapKind::SharedNullNarrowing
                    | l::TrapKind::ClassMismatch(_)
                    | l::TrapKind::DevOnlyLifetime(_)
                    | l::TrapKind::DevReloadOnlyStaleCoroutine
                    | l::TrapKind::WireEnumValue(_)
                    | l::TrapKind::DisposeRaisedDuringExit,
                    _,
                ) => false,
            };
            if fired {
                return Err(self.trap_error(trap));
            }
        }
        Ok(())
    }

    fn indexed_length(
        &mut self,
        function: &l::Function,
        instruction: &l::Instruction,
        operands: &[Value],
    ) -> Result<i64, InterpretError> {
        let base_type = match instruction.operands.first() {
            Some(l::Operand::Constant(constant)) => l::ValueType::Data(constant.ty.clone()),
            Some(l::Operand::Value(value)) => function
                .values
                .get(value.0 as usize)
                .map(|value| value.ty.clone())
                .ok_or_else(|| {
                    self.invalid(
                        Some(instruction.pos.clone()),
                        format!("indexed base value {} has no type", value.0),
                    )
                })?,
            None => return Err(self.missing_operand(instruction, 0)),
        };
        match base_type {
            l::ValueType::Data(Type::Array(_)) => {
                let handle = operands
                    .first()
                    .ok_or_else(|| self.missing_operand(instruction, 0))?
                    .as_handle()?;
                // SAFETY: verification restricts this operand to a dynamic
                // array. A preceding lifetime site rejects a stale handle.
                Ok(i64::from(unsafe { self.context.array_len(handle) }))
            }
            l::ValueType::Data(Type::FixedArray(_, count))
            | l::ValueType::Address(l::AddressType {
                pointee: Type::FixedArray(_, count),
                ..
            }) => Ok(i64::from(count)),
            other => Err(self.invalid(
                Some(instruction.pos.clone()),
                format!("index trap has non-indexable base type {other:?}"),
            )),
        }
    }

    fn value_has_class(&self, value: &Value, expected: ClassId) -> bool {
        let Value::Handle(handle) = value else {
            return false;
        };
        if handle.is_null() || !self.context.is_live(*handle as usize) {
            return false;
        }
        // SAFETY: Context::is_live proves `handle` is an exact live payload;
        // its class id is the u32 header word eight bytes before the payload.
        unsafe { (handle.sub(8) as *const u32).read() == expected.0 as u32 }
    }

    pub(super) fn trap_error(&self, trap: &l::Trap) -> InterpretError {
        let kind = runtime_trap_kind(&trap.kind).map_or_else(
            || format!("{:?}", trap.kind),
            |kind| kind.rule().to_string(),
        );
        InterpretError::Trap {
            kind,
            runtime_kind: runtime_trap_kind(&trap.kind),
            pos: trap.pos.clone(),
            message: if matches!(trap.kind, l::TrapKind::DevOnlyRelease(_)) {
                RuntimeTrapKind::DoubleDelete.message(None).into_owned()
            } else if let Some(kind) = runtime_trap_kind(&trap.kind).filter(|kind| {
                matches!(
                    kind,
                    RuntimeTrapKind::NullNarrowing | RuntimeTrapKind::SharedNullNarrowing
                )
            }) {
                kind.message(None).into_owned()
            } else {
                "LIR trap terminator/check fired".to_string()
            },
        }
    }

    pub(super) fn check_runtime(&self, pos: &Pos) -> Result<(), InterpretError> {
        let Some(trap) = self.context.trap_record() else {
            return Ok(());
        };
        let pos = match runtime_trap_site(trap.kind, &self.active_traps) {
            TrapSite::Site(site) => site.pos.clone(),
            // §112 rule 2: this kind has no script site, so the report
            // carries the reserved entry every tier answers for id 0.
            TrapSite::NoScriptSite => no_script_site(),
            // No site of this function carries the kind, so the report
            // keeps the instruction that ran.
            TrapSite::NoMatch => pos.clone(),
        };
        Err(InterpretError::Trap {
            kind: trap.kind.rule().to_string(),
            runtime_kind: Some(trap.kind),
            pos,
            message: trap.message.clone(),
        })
    }
}
