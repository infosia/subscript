//! Value storage, memory access, runtime calls, and the trap guards.

use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn iconst(&mut self, ty: types::Type, value: i64) -> Value {
        let value = if ty == types::I8 {
            i64::from(value as i8)
        } else if ty == types::I16 {
            i64::from(value as i16)
        } else if ty == types::I32 {
            i64::from(value as i32)
        } else {
            value
        };
        self.builder.ins().iconst(ty, value)
    }

    pub(super) fn zero_scalar(&mut self, ty: types::Type) -> Value {
        if ty == types::F32 {
            self.builder.ins().f32const(0.0)
        } else if ty == types::F64 {
            self.builder.ins().f64const(0.0)
        } else {
            self.iconst(ty, 0)
        }
    }

    pub(super) fn address_offset(&mut self, address: Value, offset: i64) -> Value {
        if offset == 0 {
            address
        } else {
            self.builder.ins().iadd_imm(address, offset)
        }
    }

    pub(super) fn stack_slot(&mut self, size: u32, align: u32) -> Value {
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size.max(1),
            align_shift(align.max(1)),
        ));
        self.builder.ins().stack_addr(types::I64, slot, 0)
    }

    pub(super) fn copy_bytes(&mut self, destination: Value, source: Value, size: u32, align: u32) {
        let config = self.ml.module.isa().frontend_config();
        self.builder.emit_small_memory_copy(
            config,
            destination,
            source,
            u64::from(size),
            align.max(1) as u8,
            align.max(1) as u8,
            true,
            MemFlags::new(),
        );
    }

    fn closure_environment_address(&mut self, id: l::ValueId) -> Result<Value, String> {
        let offset = self.closure_environments.get(&id).copied().ok_or_else(|| {
            internal(format!(
                "function value {} has no environment storage",
                id.0
            ))
        })?;
        let base = if self.coroutine.is_some() {
            self.frame
                .ok_or_else(|| internal("coroutine environment storage has no frame"))?
        } else {
            self.shadow
                .ok_or_else(|| internal("closure environment storage has no shadow frame"))?
        };
        Ok(self.address_offset(base, i64::from(offset)))
    }

    fn relocate_closure_environment(
        &mut self,
        code: Value,
        environment: Value,
        destination: Value,
    ) -> Result<RV, String> {
        let Some((size, align)) = self.closure_environment_layout else {
            return Ok(RV::Pair(code, environment));
        };
        let copy = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let present = self.builder.ins().icmp_imm(IntCC::NotEqual, environment, 0);
        self.builder
            .ins()
            .brif(present, copy, &[], done, &[BlockArg::Value(environment)]);
        self.builder.switch_to_block(copy);
        self.copy_bytes(destination, environment, size, align);
        self.builder
            .ins()
            .jump(done, &[BlockArg::Value(destination)]);
        self.builder.switch_to_block(done);
        Ok(RV::Pair(code, self.builder.block_params(done)[0]))
    }

    fn own_closure_environment(&mut self, id: l::ValueId, value: RV) -> Result<RV, String> {
        let (code, environment) = self.expect_pair(value)?;
        let destination = self.closure_environment_address(id)?;
        self.relocate_closure_environment(code, environment, destination)
    }

    pub(super) fn snapshot_closure_environment(&mut self, value: RV) -> Result<RV, String> {
        let Some((size, align)) = self.closure_environment_layout else {
            return Ok(value);
        };
        let (code, environment) = self.expect_pair(value)?;
        let destination = self.stack_slot(size, align);
        self.zero_bytes(destination, size, align);
        self.relocate_closure_environment(code, environment, destination)
    }

    pub(super) fn zero_bytes(&mut self, destination: Value, size: u32, align: u32) {
        let config = self.ml.module.isa().frontend_config();
        self.builder.emit_small_memset(
            config,
            destination,
            0,
            u64::from(size),
            align.max(1) as u8,
            MemFlags::new(),
        );
    }

    pub(super) fn load_data(
        &mut self,
        ty: &Type,
        address: Value,
        offset: i32,
    ) -> Result<RV, String> {
        Ok(match self.ml.layouts.repr(ty)? {
            Repr::None => RV::None,
            Repr::Scalar(value) => {
                RV::Scalar(self.builder.ins().load(value, flags(), address, offset))
            }
            Repr::Pair => {
                let code = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), address, offset);
                let env = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), address, offset + 8);
                RV::Pair(code, env)
            }
            Repr::Agg { .. } => RV::Aggregate(self.address_offset(address, i64::from(offset))),
        })
    }

    pub(super) fn store_data(
        &mut self,
        ty: &Type,
        address: Value,
        offset: i32,
        value: RV,
    ) -> Result<(), String> {
        match (self.ml.layouts.repr(ty)?, value) {
            (Repr::None, _) => Ok(()),
            (Repr::Scalar(_), RV::Scalar(value)) => {
                self.builder.ins().store(flags(), value, address, offset);
                Ok(())
            }
            (Repr::Pair, RV::Pair(code, env)) => {
                self.builder.ins().store(flags(), code, address, offset);
                self.builder.ins().store(flags(), env, address, offset + 8);
                Ok(())
            }
            (Repr::Agg { size, align }, RV::Aggregate(source)) => {
                let destination = self.address_offset(address, i64::from(offset));
                self.copy_bytes(destination, source, size, align);
                Ok(())
            }
            (Repr::Scalar(_), RV::Aggregate(source)) if self.is_boundary_struct_pointer(ty) => {
                self.builder.ins().store(flags(), source, address, offset);
                Ok(())
            }
            (repr, value) => Err(internal(format!("store mismatch {repr:?} and {value:?}"))),
        }
    }

    pub(super) fn load_value_type(
        &mut self,
        ty: &l::ValueType,
        address: Value,
        offset: i32,
    ) -> Result<RV, String> {
        match ty {
            l::ValueType::Data(ty) => self.load_data(ty, address, offset),
            l::ValueType::Address(_) => Ok(RV::Scalar(self.builder.ins().load(
                types::I64,
                flags(),
                address,
                offset,
            ))),
            l::ValueType::Iterator(_) => Ok(RV::Aggregate(
                self.address_offset(address, i64::from(offset)),
            )),
        }
    }

    pub(super) fn store_value_type(
        &mut self,
        ty: &l::ValueType,
        address: Value,
        offset: i32,
        value: RV,
    ) -> Result<(), String> {
        match ty {
            l::ValueType::Data(ty) => self.store_data(ty, address, offset, value),
            l::ValueType::Address(_) => {
                let value = self.expect_scalar(value)?;
                self.builder.ins().store(flags(), value, address, offset);
                Ok(())
            }
            l::ValueType::Iterator(_) => {
                let source = self.expect_aggregate(value)?;
                let destination = self.address_offset(address, i64::from(offset));
                self.copy_bytes(destination, source, 32, 8);
                Ok(())
            }
        }
    }

    pub(super) fn expect_scalar(&self, value: RV) -> Result<Value, String> {
        match value {
            RV::Scalar(value) => Ok(value),
            other => Err(internal(format!("expected scalar, got {other:?}"))),
        }
    }

    pub(super) fn expect_pair(&self, value: RV) -> Result<(Value, Value), String> {
        match value {
            RV::Pair(code, env) => Ok((code, env)),
            other => Err(internal(format!("expected pair, got {other:?}"))),
        }
    }

    pub(super) fn expect_aggregate(&self, value: RV) -> Result<Value, String> {
        match value {
            RV::Aggregate(value) => Ok(value),
            other => Err(internal(format!("expected aggregate, got {other:?}"))),
        }
    }

    pub(super) fn value_type(&self, id: l::ValueId) -> Result<&l::ValueType, String> {
        value_type(self.function, id)
    }

    pub(super) fn value(&mut self, id: l::ValueId) -> Result<RV, String> {
        if let Some(root) = self.value_roots.get(&id).copied() {
            let base = self
                .shadow
                .ok_or_else(|| internal("managed value has no shadow frame"))?;
            let address = self.address_offset(base, i64::from(root) * 8);
            let ty = self.value_type(id)?.clone();
            return self.load_value_type(&ty, address, 0);
        }
        self.values
            .get(id.0 as usize)
            .and_then(|value| *value)
            .ok_or_else(|| internal(format!("value {} is not available", id.0)))
    }

    pub(super) fn set_value(&mut self, id: l::ValueId, mut value: RV) -> Result<(), String> {
        let ty = self.value_type(id)?.clone();
        if self.closure_environment_layout.is_some()
            && matches!(ty, l::ValueType::Data(Type::Func(_)))
        {
            value = self.own_closure_environment(id, value)?;
        }
        if let Some(root) = self.value_roots.get(&id).copied() {
            let base = self
                .shadow
                .ok_or_else(|| internal("managed value has no shadow frame"))?;
            let address = self.address_offset(base, i64::from(root) * 8);
            self.store_value_type(&ty, address, 0, value)?;
            if matches!(value_repr(&self.ml.layouts, &ty)?, Repr::Agg { .. }) {
                value = RV::Aggregate(address);
            }
        }
        let slot = self
            .values
            .get_mut(id.0 as usize)
            .ok_or_else(|| internal(format!("value {} slot is missing", id.0)))?;
        *slot = Some(value);
        Ok(())
    }

    pub(super) fn clear_root_slots(&mut self, slots: &[usize]) -> Result<(), String> {
        if slots.is_empty() {
            return Ok(());
        }
        let shadow = self
            .shadow
            .ok_or_else(|| internal("root clear has no shadow frame"))?;
        let clears = slots
            .iter()
            .map(|slot| {
                self.root_storage
                    .slots
                    .get(*slot)
                    .map(|slot| (slot.offset, slot.words))
                    .ok_or_else(|| internal(format!("root slot {slot} is missing")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (offset, words) in clears {
            let address = self.address_offset(shadow, i64::from(offset) * 8);
            self.zero_bytes(address, words * 8, 8);
        }
        Ok(())
    }

    pub(super) fn constant(&mut self, constant: &l::Constant) -> Result<RV, String> {
        Ok(match (&constant.ty, &constant.kind) {
            (Type::F32, l::ConstantKind::FloatBits(bits)) => {
                RV::Scalar(self.builder.ins().f32const(f32::from_bits(*bits as u32)))
            }
            (Type::F64, l::ConstantKind::FloatBits(bits)) => {
                RV::Scalar(self.builder.ins().f64const(f64::from_bits(*bits)))
            }
            (Type::F16, l::ConstantKind::FloatBits(bits)) => {
                let wide = self.builder.ins().f64const(f64::from_bits(*bits));
                let result = self
                    .call_runtime(self.ml.rt.f16_from_f64, &[wide], false)?
                    .ok_or_else(|| internal("f16 constant conversion has no result"))?;
                RV::Scalar(result)
            }
            (Type::Bool, l::ConstantKind::Boolean(value)) => {
                RV::Scalar(self.iconst(types::I8, i64::from(*value)))
            }
            (_, l::ConstantKind::Null) => RV::Scalar(self.iconst(types::I64, 0)),
            (ty, l::ConstantKind::Integer(value)) => {
                let Repr::Scalar(repr) = self.ml.layouts.repr(ty)? else {
                    return Err(internal(format!(
                        "integer constant has aggregate type {ty:?}"
                    )));
                };
                RV::Scalar(self.iconst(repr, *value))
            }
            (ty, kind) => {
                return Err(internal(format!(
                    "constant payload {kind:?} disagrees with {ty:?}"
                )))
            }
        })
    }

    pub(super) fn operand(&mut self, operand: &l::Operand) -> Result<RV, String> {
        match operand {
            l::Operand::Value(value) => self.value(*value),
            l::Operand::Constant(constant) => self.constant(constant),
        }
    }

    pub(super) fn operand_type(&self, operand: &l::Operand) -> Result<l::ValueType, String> {
        operand_type(self.function, operand)
    }

    pub(super) fn position_id(&mut self, position: &Pos) -> i64 {
        i64::from(self.ml.pos_id(position))
    }

    pub(super) fn call_runtime(
        &mut self,
        function: cranelift_module::FuncId,
        arguments: &[Value],
        checked: bool,
    ) -> Result<Option<Value>, String> {
        let reference = self
            .ml
            .module
            .declare_func_in_func(function, self.builder.func);
        let call = self.builder.ins().call(reference, arguments);
        if checked {
            self.trap_check();
        }
        Ok(self.builder.inst_results(call).first().copied())
    }

    pub(super) fn call_script(
        &mut self,
        key: &FnKey,
        arguments: &[Value],
        checked: bool,
    ) -> Result<Vec<Value>, String> {
        if !self.ml.opts.reload {
            return self.call_script_direct(key, arguments, checked);
        }
        let id = self.ml.func_id(key)?;
        let slot = self.ml.slot_of(key)?;
        let displacement = i32::try_from(u64::from(slot) * 8)
            .map_err(|_| internal("function slot offset does not fit in i32"))?;
        let signature = self.builder.import_signature(self.ml.signature_of(id));
        let table_offset = ctx_off(rtc::Context::fn_table_offset())?;
        let table = self
            .builder
            .ins()
            .load(types::I64, flags(), self.ctx, table_offset);
        let code = self
            .builder
            .ins()
            .load(types::I64, flags(), table, displacement);
        let call = self.builder.ins().call_indirect(signature, code, arguments);
        if checked {
            self.trap_check();
        }
        Ok(self.builder.inst_results(call).to_vec())
    }

    pub(super) fn call_script_direct(
        &mut self,
        key: &FnKey,
        arguments: &[Value],
        checked: bool,
    ) -> Result<Vec<Value>, String> {
        let id = self.ml.func_id(key)?;
        let reference = self.ml.module.declare_func_in_func(id, self.builder.func);
        let call = self.builder.ins().call(reference, arguments);
        if checked {
            self.trap_check();
        }
        Ok(self.builder.inst_results(call).to_vec())
    }

    pub(super) fn unwind_block(&mut self) -> Block {
        if let Some(block) = self.unwind {
            block
        } else {
            let block = self.builder.create_block();
            self.unwind = Some(block);
            block
        }
    }

    pub(super) fn trap_check(&mut self) {
        let trap = self.builder.ins().load(types::I32, flags(), self.ctx, 0);
        let clear = self.builder.ins().icmp_imm(IntCC::Equal, trap, 0);
        let next = self.builder.create_block();
        let unwind = self.unwind_block();
        self.builder.ins().brif(clear, next, &[], unwind, &[]);
        self.builder.switch_to_block(next);
    }

    fn guard(&mut self, condition: Value, kind: TrapKind, position: &Pos) -> Result<(), String> {
        let ok = self.builder.create_block();
        let bad = self.builder.create_block();
        self.builder.ins().brif(condition, ok, &[], bad, &[]);
        self.builder.switch_to_block(bad);
        let kind = self.iconst(types::I32, i64::from(kind as u32));
        let position_id = self.position_id(position);
        let position_id = self.iconst(types::I32, position_id);
        self.call_runtime(self.ml.rt.trap, &[self.ctx, kind, position_id], false)?;
        let unwind = self.unwind_block();
        self.builder.ins().jump(unwind, &[]);
        self.builder.switch_to_block(ok);
        Ok(())
    }

    fn index_guard(
        &mut self,
        condition: Value,
        index: Value,
        length: Value,
        position: &Pos,
    ) -> Result<(), String> {
        let ok = self.builder.create_block();
        let bad = self.builder.create_block();
        self.builder.ins().brif(condition, ok, &[], bad, &[]);
        self.builder.switch_to_block(bad);
        let position_id = self.position_id(position);
        let position_id = self.iconst(types::I32, position_id);
        self.call_runtime(
            self.ml.rt.trap_index_out_of_bounds,
            &[self.ctx, index, length, position_id],
            false,
        )?;
        let unwind = self.unwind_block();
        self.builder.ins().jump(unwind, &[]);
        self.builder.switch_to_block(ok);
        Ok(())
    }

    pub(super) fn live_check(&mut self, pointer: Value, position: &Pos) -> Result<(), String> {
        let state = self
            .builder
            .ins()
            .load(types::I64, flags(), pointer, rtc::STATE_OFFSET);
        let live = self
            .builder
            .ins()
            .icmp_imm(IntCC::Equal, state, rtc::LIVE_STATE as i64);
        let kind = runtime_trap_kind(&l::TrapKind::DevOnlyLifetime)
            .ok_or_else(|| internal("lifetime trap has no direct runtime kind"))?;
        self.guard(live, kind, position)
    }

    pub(super) fn reload_epoch_check(
        &mut self,
        frame: Value,
        position: &Pos,
    ) -> Result<(), String> {
        if !self.ml.opts.reload {
            return Ok(());
        }
        let valid = if self.function.is_async {
            let stale = self
                .call_runtime(self.ml.rt.async_is_stale, &[self.ctx, frame], false)?
                .ok_or_else(|| internal("async stale check has no result"))?;
            self.builder.ins().icmp_imm(IntCC::Equal, stale, 0)
        } else {
            let offset = ctx_off(rtc::Context::reload_epoch_offset())?;
            let current = self
                .builder
                .ins()
                .load(types::I32, flags(), self.ctx, offset);
            let created =
                self.builder
                    .ins()
                    .load(types::I32, flags(), frame, GENERATOR_EPOCH_OFFSET);
            self.builder.ins().icmp(IntCC::Equal, current, created)
        };
        let kind = runtime_trap_kind(&l::TrapKind::DevReloadOnlyStaleCoroutine)
            .ok_or_else(|| internal("stale-coroutine trap has no direct runtime kind"))?;
        self.guard(valid, kind, position)
    }

    pub(super) fn emit_trap(&mut self, trap: &l::Trap, operand: TrapOperand) -> Result<(), String> {
        let runtime_kind = runtime_trap_kind(&trap.kind);
        let direct_kind = || {
            runtime_kind
                .ok_or_else(|| internal(format!("trap {:?} has no direct runtime kind", trap.kind)))
        };
        let value = match operand {
            TrapOperand::Value(value) | TrapOperand::Condition(value) => Some(value),
            _ => None,
        };
        match &trap.kind {
            l::TrapKind::Allocation | l::TrapKind::Call => {
                if !matches!(operand, TrapOperand::Pending) {
                    return Err(internal("pending trap received an explicit operand"));
                }
                self.trap_check();
            }
            l::TrapKind::Unreachable => {
                let false_value = self.iconst(types::I8, 0);
                self.guard(false_value, direct_kind()?, &trap.pos)?;
            }
            l::TrapKind::DivisionByZero => {
                let divisor = value.ok_or_else(|| internal("division trap has no divisor"))?;
                let nonzero = self.builder.ins().icmp_imm(IntCC::NotEqual, divisor, 0);
                self.guard(nonzero, direct_kind()?, &trap.pos)?;
            }
            l::TrapKind::IndexRead | l::TrapKind::IndexWrite => match operand {
                TrapOperand::Pending => self.trap_check(),
                TrapOperand::Index {
                    condition,
                    index,
                    length,
                } => {
                    if direct_kind()? != TrapKind::IndexOutOfBounds {
                        return Err(internal("index trap has a non-index runtime kind"));
                    }
                    self.index_guard(condition, index, length, &trap.pos)?;
                }
                TrapOperand::Value(_)
                | TrapOperand::Condition(_)
                | TrapOperand::WireValue { .. } => {
                    return Err(internal("index trap received no index/length payload"))
                }
            },
            l::TrapKind::JsonResultValue(_) => {
                let condition =
                    value.ok_or_else(|| internal("JSON result trap has no condition"))?;
                self.guard(condition, direct_kind()?, &trap.pos)?;
            }
            l::TrapKind::NullNarrowing => {
                let pointer = value.ok_or_else(|| internal("null trap has no pointer"))?;
                let nonnull = self.builder.ins().icmp_imm(IntCC::NotEqual, pointer, 0);
                self.guard(nonnull, direct_kind()?, &trap.pos)?;
            }
            l::TrapKind::ClassMismatch(class) => {
                let pointer = value.ok_or_else(|| internal("class trap has no pointer"))?;
                let class_id =
                    self.builder
                        .ins()
                        .load(types::I32, flags(), pointer, rtc::CLASS_ID_OFFSET);
                let matches = self
                    .builder
                    .ins()
                    .icmp_imm(IntCC::Equal, class_id, class.0 as i64);
                self.guard(matches, direct_kind()?, &trap.pos)?;
            }
            l::TrapKind::DevOnlyLifetime => match operand {
                TrapOperand::Pending => self.trap_check(),
                TrapOperand::Value(pointer) | TrapOperand::Condition(pointer) => {
                    self.live_check(pointer, &trap.pos)?;
                }
                TrapOperand::Index { .. } | TrapOperand::WireValue { .. } => {
                    return Err(internal("lifetime trap received a wire operand"))
                }
            },
            l::TrapKind::DevReloadOnlyStaleCoroutine => {
                let frame = value.ok_or_else(|| internal("stale trap has no frame"))?;
                self.reload_epoch_check(frame, &trap.pos)?;
            }
            l::TrapKind::WireEnumValue(alias) => {
                if direct_kind()? != TrapKind::WireEnumUnknownValue {
                    return Err(internal("wire-enum trap has a non-wire runtime kind"));
                }
                let TrapOperand::WireValue { wire, valid } = operand else {
                    return Err(internal("wire enum trap has no wire operand"));
                };
                let definition = self
                    .ml
                    .lir
                    .string_aliases
                    .get(alias.0)
                    .ok_or_else(|| internal(format!("string alias {} is missing", alias.0)))?;
                let data = self.ml.literal_data(definition.source_name.as_bytes())?;
                let global = self.ml.module.declare_data_in_func(data, self.builder.func);
                let name = self.builder.ins().symbol_value(types::I64, global);
                let length = self.iconst(types::I64, definition.source_name.len() as i64);
                let position = self.position_id(&trap.pos);
                let position = self.iconst(types::I32, position);
                let ok = self.builder.create_block();
                let bad = self.builder.create_block();
                self.builder.ins().brif(valid, ok, &[], bad, &[]);
                self.builder.switch_to_block(bad);
                self.call_runtime(
                    self.ml.rt.trap_wire_enum,
                    &[self.ctx, name, length, wire, position],
                    false,
                )?;
                let unwind = self.unwind_block();
                self.builder.ins().jump(unwind, &[]);
                self.builder.switch_to_block(ok);
            }
        }
        self.consumed_traps.push(trap.clone());
        Ok(())
    }

    fn is_boundary_struct_pointer(&self, ty: &Type) -> bool {
        boundary_box_class(self.ml.lir, ty).is_some()
    }
}
