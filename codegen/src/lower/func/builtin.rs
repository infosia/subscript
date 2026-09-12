//! Function references, closure construction, and the builtin calls.

use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn function_reference(&mut self, function: l::FunctionId) -> Result<RV, String> {
        let wrapper = self.ml.func_id(&FnKey::LirWrapper(function))?;
        let reference = self
            .ml
            .module
            .declare_func_in_func(wrapper, self.builder.func);
        let code = self.builder.ins().func_addr(types::I64, reference);
        let env = self.iconst(types::I64, 0);
        Ok(RV::Pair(code, env))
    }

    pub(super) fn make_closure(
        &mut self,
        function: l::FunctionId,
        operands: &[RV],
    ) -> Result<RV, String> {
        let target = self
            .ml
            .lir
            .functions
            .get(function.0 as usize)
            .filter(|target| target.id == function)
            .ok_or_else(|| internal(format!("closure function {} is missing", function.0)))?;
        let captures = capture_parameters(target)
            .map(|parameter| {
                target
                    .values
                    .get(parameter.value.0 as usize)
                    .map(|value| value.ty.clone())
                    .ok_or_else(|| {
                        internal(format!("capture value {} is missing", parameter.value.0))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut offset = 0u32;
        let mut align = 1u32;
        let mut fields = Vec::with_capacity(captures.len());
        for ty in &captures {
            let (size, field_align) = value_size_align(&self.ml.layouts, ty)?;
            offset = round_up_layout(offset, field_align.max(1), "closure environment layout")?;
            fields.push(offset);
            offset = checked_layout_add(offset, size.max(1), "closure environment layout")?;
            align = align.max(field_align.max(1));
        }
        let _ = round_up_layout(offset.max(1), align, "final closure environment layout")?;
        let environment = if captures.is_empty() {
            self.iconst(types::I64, 0)
        } else {
            let (size, align) = self
                .closure_environment_layout
                .ok_or_else(|| internal("capturing closure has no uniform environment layout"))?;
            let environment = self.stack_slot(size, align);
            self.zero_bytes(environment, size, align);
            environment
        };
        for (((value, ty), offset), _) in
            operands.iter().copied().zip(&captures).zip(fields).zip(0..)
        {
            self.store_value_type(ty, environment, offset as i32, value)?;
        }
        let id = self.ml.func_id(&FnKey::LirFunction(function))?;
        let reference = self.ml.module.declare_func_in_func(id, self.builder.func);
        let code = self.builder.ins().func_addr(types::I64, reference);
        Ok(RV::Pair(code, environment))
    }

    pub(super) fn builtin_call(
        &mut self,
        method: l::BuiltinMethod,
        operands: &[RV],
        parameter_types: &[l::ValueType],
        return_type: Option<&l::ValueType>,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        match method {
            l::BuiltinMethod::ArrayPush => {
                let l::ValueType::Data(Type::Array(element)) = parameter_types
                    .first()
                    .ok_or_else(|| internal("array push has no receiver type"))?
                else {
                    return Err(internal("array push receiver is not an array"));
                };
                let handle = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("array push has no receiver"))?,
                )?;
                for trap in traps {
                    if trap.kind == l::TrapKind::DevOnlyLifetime {
                        self.emit_trap(trap, TrapOperand::Value(handle))?;
                    }
                }
                let value = *operands
                    .get(1)
                    .ok_or_else(|| internal("array push has no value"))?;
                let length = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), handle, ARRAY_LEN_OFFSET);
                let capacity =
                    self.builder
                        .ins()
                        .load(types::I64, flags(), handle, ARRAY_CAP_OFFSET);
                let available = self
                    .builder
                    .ins()
                    .icmp(IntCC::UnsignedLessThan, length, capacity);
                let fast = self.builder.create_block();
                let slow = self.builder.create_block();
                let done = self.builder.create_block();
                self.builder.ins().brif(available, fast, &[], slow, &[]);

                self.builder.switch_to_block(fast);
                let data = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), handle, ARRAY_DATA_OFFSET);
                let stride = self.ml.layouts.stride(element)?;
                let offset = self.builder.ins().imul_imm(length, i64::from(stride));
                let destination = self.builder.ins().iadd(data, offset);
                self.store_data(element, destination, 0, value)?;
                let next_length = self.builder.ins().iadd_imm(length, 1);
                self.builder
                    .ins()
                    .store(flags(), next_length, handle, ARRAY_LEN_OFFSET);
                self.builder.ins().jump(done, &[]);

                self.builder.switch_to_block(slow);
                let value = self.materialize(value, element)?;
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    self.ml.rt.array_push,
                    &[self.ctx, handle, value, position],
                    false,
                )?;
                for trap in traps {
                    if matches!(trap.kind, l::TrapKind::Allocation | l::TrapKind::Call) {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                }
                self.builder.ins().jump(done, &[]);

                self.builder.switch_to_block(done);
                let result = self
                    .builder
                    .ins()
                    .load(types::I64, flags(), handle, ARRAY_LEN_OFFSET);
                let result = self.builder.ins().ireduce(types::I32, result);
                Ok(RV::Scalar(result))
            }
            l::BuiltinMethod::ArrayPop => {
                let l::ValueType::Data(Type::Array(element)) = parameter_types
                    .first()
                    .ok_or_else(|| internal("array pop has no receiver type"))?
                else {
                    return Err(internal("array pop receiver is not an array"));
                };
                let handle = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("array pop has no receiver"))?,
                )?;
                let (size, align) = self.ml.layouts.size_align(element)?;
                let output = self.stack_slot(size.max(8), align.max(8));
                self.zero_bytes(output, size.max(8), align.max(8));
                let position = self.position_id(pos);
                let position = self.iconst(types::I32, position);
                self.call_runtime(
                    self.ml.rt.array_pop,
                    &[self.ctx, handle, output, position],
                    false,
                )?;
                for trap in traps {
                    match trap.kind {
                        l::TrapKind::DevOnlyLifetime => {
                            self.emit_trap(trap, TrapOperand::Value(handle))?
                        }
                        l::TrapKind::Allocation | l::TrapKind::Call => {
                            self.emit_trap(trap, TrapOperand::Pending)?
                        }
                        _ => {}
                    }
                }
                self.load_data(element, output, 0)
            }
            l::BuiltinMethod::StringSlice => {
                let operation = self
                    .ml
                    .lir
                    .intrinsic_operations
                    .iter()
                    .find(|operation| {
                        operation.family == l::IntrinsicFamily::String
                            && operation.semantic_name == "Slice"
                    })
                    .ok_or_else(|| internal("String.Slice operation is missing"))?;
                let function = *self
                    .ml
                    .rt
                    .str_ops
                    .get(operation.operation as usize)
                    .ok_or_else(|| internal("String.Slice runtime function is missing"))?;
                self.simple_runtime_intrinsic(function, operands, Some(pos), true, false)
            }
            l::BuiltinMethod::GeneratorNext => {
                let frame = self.expect_scalar(
                    *operands
                        .first()
                        .ok_or_else(|| internal("generator next has no receiver"))?,
                )?;
                for trap in traps {
                    match trap.kind {
                        l::TrapKind::DevOnlyLifetime | l::TrapKind::DevReloadOnlyStaleCoroutine => {
                            self.emit_trap(trap, TrapOperand::Value(frame))?
                        }
                        _ => {}
                    }
                }
                let l::ValueType::Data(Type::IterResult(value)) =
                    return_type.ok_or_else(|| internal("generator next has no result type"))?
                else {
                    return Err(internal("generator next result is not IterResult"));
                };
                let result_ty = Type::IterResult(value.clone());
                let (size, align) = self.ml.layouts.size_align(&result_ty)?;
                let result = self.stack_slot(size, align);
                self.zero_bytes(result, size, align);
                let value_offset = self.ml.layouts.iter_result_value_offset(value)?;
                let output = self.address_offset(result, i64::from(value_offset));
                let resume =
                    self.builder
                        .ins()
                        .load(types::I64, flags(), frame, COROUTINE_RESUME_OFFSET);
                let signature = self.builder.import_signature(self.ml.resume_sig());
                let call =
                    self.builder
                        .ins()
                        .call_indirect(signature, resume, &[self.ctx, frame, output]);
                let done = self.builder.inst_results(call)[0];
                for trap in traps {
                    if trap.kind == l::TrapKind::Call {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                }
                self.builder.ins().store(flags(), done, result, 0);
                Ok(RV::Aggregate(result))
            }
        }
    }
}
