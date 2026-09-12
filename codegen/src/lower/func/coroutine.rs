//! The coroutine frame plan, suspension, and async child resume.

use super::*;

pub(super) fn ensure_explicit_frame_supported(
    function: &cranelift_codegen::ir::Function,
    label: &str,
) -> Result<(), String> {
    let mut bytes = 0u32;
    for slot in function.sized_stack_slots.values() {
        bytes = checked_layout_add(bytes, slot.size, "Cranelift explicit stack frame")?;
        bytes = round_up_layout(
            bytes,
            1u32 << slot.align_shift,
            "Cranelift explicit stack frame",
        )?;
    }
    if bytes > MAX_FRAME_BYTES {
        return Err(internal(format!(
            "{label} needs {bytes} bytes of explicit stack storage; maximum is {MAX_FRAME_BYTES}"
        )));
    }
    let _ = CRANELIFT_FRAME_ALIGNMENT;
    Ok(())
}

pub(super) fn plan_coroutine(
    layouts: &Layouts,
    module: &l::Module,
    function: &l::Function,
) -> Result<CoroutinePlan, String> {
    let mut offset = COROUTINE_PAYLOAD_OFFSET;
    let mut parameter_slots = Vec::with_capacity(function.parameters.len());
    for parameter in &function.parameters {
        let ty = function
            .values
            .get(parameter.value.0 as usize)
            .ok_or_else(|| internal(format!("parameter value {} is missing", parameter.value.0)))?
            .ty
            .clone();
        let (size, align) = value_size_align(layouts, &ty)?;
        offset = round_up_layout(offset, align.max(1), "coroutine parameter layout")?;
        parameter_slots.push(FrameSlot { offset, ty });
        offset = checked_layout_add(offset, size.max(1), "coroutine parameter layout")?;
    }
    let mut local_slots = Vec::with_capacity(function.locals.len());
    for local in &function.locals {
        if local.storage == l::LocalStorageClass::Frame {
            let (size, align) = value_size_align(layouts, &local.ty)?;
            offset = round_up_layout(offset, align.max(1), "coroutine local layout")?;
            local_slots.push(Some(FrameSlot {
                offset,
                ty: local.ty.clone(),
            }));
            offset = checked_layout_add(offset, size.max(1), "coroutine local layout")?;
        } else {
            local_slots.push(None);
        }
    }
    let mut suspends = HashMap::new();
    let mut state = 1i64;
    for block in &function.blocks {
        let l::Terminator::Suspend {
            kind,
            successor,
            resume_value,
            ..
        } = &block.terminator
        else {
            continue;
        };
        let destination = function
            .blocks
            .get(successor.0 as usize)
            .ok_or_else(|| internal(format!("suspend successor {} is missing", successor.0)))?;
        let start = usize::from(resume_value.is_some());
        let mut arguments = Vec::new();
        for value in destination.parameters.iter().skip(start) {
            let ty = function
                .values
                .get(value.0 as usize)
                .ok_or_else(|| internal(format!("resume value {} is missing", value.0)))?
                .ty
                .clone();
            let (size, align) = value_size_align(layouts, &ty)?;
            offset = round_up_layout(offset, align.max(1), "suspend live-in layout")?;
            arguments.push(FrameSlot { offset, ty });
            offset = checked_layout_add(offset, size.max(1), "suspend live-in layout")?;
        }
        let child = if matches!(
            kind,
            l::SuspendKind::AsyncCall { .. } | l::SuspendKind::AsyncHandle { .. }
        ) {
            offset = round_up_layout(offset, 8, "async child layout")?;
            let child = offset;
            offset = checked_layout_add(offset, 8, "async child layout")?;
            Some(child)
        } else {
            None
        };
        suspends.insert(
            block.id,
            SuspendPlan {
                state,
                arguments,
                child,
            },
        );
        state += 1;
    }
    let live_across_suspend = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            l::Terminator::Suspend { arguments, .. } => Some(arguments),
            _ => None,
        })
        .flatten()
        .filter_map(|operand| match operand {
            l::Operand::Value(value) => Some(*value),
            l::Operand::Constant(_) => None,
        })
        .collect::<HashSet<_>>();
    let mut stable_addresses = HashMap::new();
    for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
        if !matches!(
            instruction.kind,
            l::InstructionKind::AllocateClass(_) | l::InstructionKind::AddressOfValue
        ) {
            continue;
        }
        let Some(result) = instruction
            .result
            .filter(|result| live_across_suspend.contains(result))
        else {
            continue;
        };
        let Some(l::ValueType::Address(address)) = function
            .values
            .get(result.0 as usize)
            .map(|value| &value.ty)
        else {
            // Reference-class allocations are handles and remain valid across
            // suspension without pinning target storage in the frame.
            continue;
        };
        let (size, align) = layouts.size_align(&address.pointee)?;
        offset = round_up_layout(offset, align.max(1), "stable coroutine address layout")?;
        stable_addresses.insert(result, offset);
        offset = checked_layout_add(offset, size.max(1), "stable coroutine address layout")?;
    }
    let mut closure_environments = HashMap::new();
    if let Some((size, align)) = closure_environment_layout(module, layouts)? {
        for value in &function.values {
            if !matches!(value.ty, l::ValueType::Data(Type::Func(_))) {
                continue;
            }
            offset = round_up_layout(offset, align, "coroutine closure environment layout")?;
            closure_environments.insert(value.id, offset);
            offset = checked_layout_add(offset, size, "coroutine closure environment layout")?;
        }
    }
    let size = round_up_layout(offset, 8, "final coroutine layout")?;
    Ok(CoroutinePlan {
        parameter_slots,
        local_slots,
        suspends,
        stable_addresses,
        closure_environments,
        size,
    })
}

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn emit_suspend(
        &mut self,
        block: l::BlockId,
        terminator: &l::Terminator,
    ) -> Result<(), String> {
        let l::Terminator::Suspend {
            kind,
            arguments,
            traps,
            pos,
            ..
        } = terminator
        else {
            return Err(internal("non-suspend passed to suspend transcriber"));
        };
        let frame = self
            .frame
            .ok_or_else(|| internal("suspension has no frame"))?;
        let plan = self
            .suspend_plans
            .get(&block)
            .ok_or_else(|| internal(format!("suspend block {} has no frame plan", block.0)))?
            .clone();
        for (argument, slot) in arguments.iter().zip(&plan.arguments) {
            let value = self.operand(argument)?;
            self.store_value_type(&slot.ty, frame, slot.offset as i32, value)?;
        }
        match kind {
            l::SuspendKind::Yield(value) => {
                if let Some(value_id) = value {
                    let value = self.value(*value_id)?;
                    let ty = data_type(self.value_type(*value_id)?)?.clone();
                    let output = self.out.ok_or_else(|| internal("yield has no output"))?;
                    self.store_data(&ty, output, 0, value)?;
                }
            }
            l::SuspendKind::Async => {
                // B1 experiment rule 8: a `Context.suspend()` waiter becomes
                // eligible at the next host checkpoint.
                self.call_runtime(self.ml.rt.async_park, &[self.ctx, frame], false)?;
            }
            l::SuspendKind::AsyncCall { target, operands } => {
                let child = self.create_async_child(target, operands, traps)?;
                let child_offset = plan
                    .child
                    .ok_or_else(|| internal("async call has no child-frame slot"))?;
                self.builder
                    .ins()
                    .store(flags(), child, frame, child_offset as i32);
                // B1 experiment rule 1: the call runs the callee's prefix.
                self.start_async_handle(target, child)?;
                return self.await_async_child(block, child, &plan, traps, true);
            }
            l::SuspendKind::AsyncHandle { handle } => {
                let handle_value = self.value(*handle)?;
                let handle = self.expect_scalar(handle_value)?;
                let handle_offset = plan
                    .child
                    .ok_or_else(|| internal("held await has no handle-frame slot"))?;
                self.builder
                    .ins()
                    .store(flags(), handle, frame, handle_offset as i32);
                return self.await_async_handle(block, handle, &plan, traps, true);
            }
        }
        let state = self.iconst(types::I32, plan.state);
        self.builder.ins().store(flags(), state, frame, 0);
        self.pop_shadow()?;
        let zero = self.iconst(types::I8, 0);
        self.builder.ins().return_(&[zero]);
        let _ = pos;
        Ok(())
    }

    fn create_async_child(
        &mut self,
        target: &l::CallTarget,
        operand_ids: &[l::ValueId],
        traps: &[l::Trap],
    ) -> Result<Value, String> {
        let operands = operand_ids
            .iter()
            .map(|id| self.value(*id))
            .collect::<Result<Vec<_>, _>>()?;
        self.create_async_child_from_values(target, &operands, traps)
    }

    pub(super) fn create_async_child_from_values(
        &mut self,
        target: &l::CallTarget,
        operands: &[RV],
        traps: &[l::Trap],
    ) -> Result<Value, String> {
        let function = match target.kind {
            l::CallTargetKind::Function(function) => function,
            l::CallTargetKind::Method(method) => self.method_function(method)?,
            ref other => {
                return Err(internal(format!(
                    "async suspension has invalid target {other:?}"
                )))
            }
        };
        let target_function = self
            .ml
            .lir
            .functions
            .get(function.0 as usize)
            .filter(|candidate| candidate.id == function)
            .ok_or_else(|| internal(format!("async target {} is missing", function.0)))?;
        if !target_function.is_async {
            return Err(internal(format!(
                "async target {} is synchronous",
                function.0
            )));
        }
        let mut arguments = vec![self.ctx];
        for (value, ty) in operands.iter().zip(&target.parameter_types) {
            self.push_argument(&mut arguments, *value, ty)?;
        }
        for trap in traps {
            if trap.kind == l::TrapKind::DevOnlyLifetime {
                if let Some(first) = operands.first() {
                    let pointer = self.expect_scalar(*first)?;
                    self.emit_trap(trap, TrapOperand::Value(pointer))?;
                }
            }
        }
        let results = self.call_script(&FnKey::LirFunction(function), &arguments, false)?;
        for trap in traps {
            if trap.kind == l::TrapKind::Call {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        results
            .first()
            .copied()
            .ok_or_else(|| internal("async creator has no frame result"))
    }

    pub(super) fn start_async_handle(
        &mut self,
        target: &l::CallTarget,
        handle: Value,
    ) -> Result<(), String> {
        let (output, size) = match &target.return_type {
            Some(ty) => {
                let (size, align) = self.ml.layouts.size_align(data_type(ty)?)?;
                let output = self.stack_slot(size.max(1), align.max(1));
                self.zero_bytes(output, size.max(1), align.max(1));
                (output, size)
            }
            None => (self.iconst(types::I64, 0), 0),
        };
        let resume = self
            .builder
            .ins()
            .load(types::I64, flags(), handle, COROUTINE_RESUME_OFFSET);
        let signature = self.builder.import_signature(self.ml.resume_sig());
        let call = self
            .builder
            .ins()
            .call_indirect(signature, resume, &[self.ctx, handle, output]);
        let done = self.builder.inst_results(call)[0];
        self.trap_check();
        let complete = self.builder.create_block();
        let continued = self.builder.create_block();
        self.builder.ins().brif(done, complete, &[], continued, &[]);
        self.builder.switch_to_block(complete);
        let size = self.iconst(types::I64, i64::from(size));
        self.call_runtime(
            self.ml.rt.async_complete,
            &[self.ctx, handle, output, size],
            false,
        )?;
        self.builder.ins().jump(continued, &[]);
        self.builder.switch_to_block(continued);
        Ok(())
    }

    // B1 experiment: an await registers a continuation and suspends. It
    // never resumes the awaited child. The scheduler resumes the caller
    // after the child completes, and the caller reads the cached result.
    fn suspend_after_await(&mut self, plan: &SuspendPlan) -> Result<(), String> {
        let frame = self
            .frame
            .ok_or_else(|| internal("await has no parent frame"))?;
        let state = self.iconst(types::I32, plan.state);
        self.builder.ins().store(flags(), state, frame, 0);
        self.pop_shadow()?;
        let zero = self.iconst(types::I8, 0);
        self.builder.ins().return_(&[zero]);
        Ok(())
    }

    fn await_async_child(
        &mut self,
        block: l::BlockId,
        child: Value,
        plan: &SuspendPlan,
        traps: &[l::Trap],
        consume_traps: bool,
    ) -> Result<(), String> {
        // The `Call` trap of a direct await is consumed where the child is
        // created, exactly as it was before this experiment.
        let _ = (block, traps, consume_traps);
        let frame = self
            .frame
            .ok_or_else(|| internal("async parent has no frame"))?;
        self.call_runtime(self.ml.rt.async_await, &[self.ctx, frame, child], false)?;
        self.suspend_after_await(plan)
    }

    fn await_async_handle(
        &mut self,
        block: l::BlockId,
        handle: Value,
        plan: &SuspendPlan,
        traps: &[l::Trap],
        consume_traps: bool,
    ) -> Result<(), String> {
        let _ = block;
        if let Some(stale) = traps
            .iter()
            .find(|trap| trap.kind == l::TrapKind::DevReloadOnlyStaleCoroutine)
        {
            if consume_traps {
                self.emit_trap(stale, TrapOperand::Value(handle))?;
            } else {
                self.reload_epoch_check(handle, &stale.pos)?;
            }
        }
        if consume_traps {
            for trap in traps {
                if trap.kind == l::TrapKind::Call {
                    self.emit_trap(trap, TrapOperand::Pending)?;
                }
            }
        }
        let frame = self
            .frame
            .ok_or_else(|| internal("held await parent has no frame"))?;
        self.call_runtime(self.ml.rt.async_await, &[self.ctx, frame, handle], false)?;
        self.suspend_after_await(plan)
    }

    // §94.1: a resumed await reads the cached completion. A missing result
    // calls async_missing_completion and unwinds with an Internal trap.
    // It never re-registers the continuation.
    fn resume_async_child(
        &mut self,
        block: l::BlockId,
        target: &l::CallTarget,
        child: Value,
        plan: &SuspendPlan,
    ) -> Result<(), String> {
        let source = self
            .function
            .blocks
            .get(block.0 as usize)
            .ok_or_else(|| internal(format!("async suspend block {} is missing", block.0)))?;
        let l::Terminator::Suspend {
            successor,
            resume_value,
            pos,
            ..
        } = &source.terminator
        else {
            return Err(internal("async attempt source is not a suspension"));
        };
        let successor = *successor;
        let has_resume_value = resume_value.is_some();
        let pos = pos.clone();
        let output = match target.return_type.as_ref() {
            Some(l::ValueType::Data(ty)) => {
                let (size, align) = self.ml.layouts.size_align(ty)?;
                let address = self.stack_slot(size.max(1), align.max(1));
                self.zero_bytes(address, size.max(1), align.max(1));
                Some((address, ty.clone(), size))
            }
            Some(other) => {
                return Err(internal(format!("async result has invalid type {other:?}")))
            }
            None => None,
        };
        self.read_completion(child, output.as_ref(), &pos)?;
        let frame = self
            .frame
            .ok_or_else(|| internal("async parent has no frame"))?;
        let mut arguments = Vec::new();
        if has_resume_value {
            let (address, ty, _) = output
                .as_ref()
                .ok_or_else(|| internal("async resume value has no output slot"))?;
            arguments.extend(rv_args(self.load_data(ty, *address, 0)?));
        }
        for slot in &plan.arguments {
            arguments.extend(rv_args(self.load_value_type(
                &slot.ty,
                frame,
                slot.offset as i32,
            )?));
        }
        let release_pos = self.position_id(&pos);
        let release_pos = self.iconst(types::I32, release_pos);
        self.call_runtime(
            self.ml.rt.async_release,
            &[self.ctx, child, release_pos],
            false,
        )?;
        let child_offset = plan
            .child
            .ok_or_else(|| internal("completed async call has no child-frame slot"))?;
        let zero = self.iconst(types::I64, 0);
        self.builder
            .ins()
            .store(flags(), zero, frame, child_offset as i32);
        let successor = self.blocks[successor.0 as usize];
        self.builder.ins().jump(successor, &arguments);
        Ok(())
    }

    fn resume_async_handle(
        &mut self,
        block: l::BlockId,
        handle: Value,
        plan: &SuspendPlan,
        traps: &[l::Trap],
    ) -> Result<(), String> {
        let source = self
            .function
            .blocks
            .get(block.0 as usize)
            .ok_or_else(|| internal(format!("held-await block {} is missing", block.0)))?;
        let l::Terminator::Suspend {
            successor,
            resume_value,
            pos,
            ..
        } = &source.terminator
        else {
            return Err(internal("held await source is not a suspension"));
        };
        let successor = *successor;
        let resume_value = *resume_value;
        let pos = pos.clone();
        if let Some(stale) = traps
            .iter()
            .find(|trap| trap.kind == l::TrapKind::DevReloadOnlyStaleCoroutine)
        {
            self.reload_epoch_check(handle, &stale.pos)?;
        }
        let output = if let Some(value) = resume_value {
            let ty = data_type(self.value_type(value)?)?.clone();
            let (size, align) = self.ml.layouts.size_align(&ty)?;
            let address = self.stack_slot(size.max(1), align.max(1));
            self.zero_bytes(address, size.max(1), align.max(1));
            Some((address, ty, size))
        } else {
            None
        };
        self.read_completion(handle, output.as_ref(), &pos)?;
        let frame = self
            .frame
            .ok_or_else(|| internal("held await parent has no frame"))?;
        let mut arguments = Vec::new();
        if resume_value.is_some() {
            let (address, ty, _) = output
                .as_ref()
                .ok_or_else(|| internal("held await result has no output slot"))?;
            arguments.extend(rv_args(self.load_data(ty, *address, 0)?));
        }
        for slot in &plan.arguments {
            arguments.extend(rv_args(self.load_value_type(
                &slot.ty,
                frame,
                slot.offset as i32,
            )?));
        }
        let child_offset = plan
            .child
            .ok_or_else(|| internal("completed held await has no child-frame slot"))?;
        let zero = self.iconst(types::I64, 0);
        self.builder
            .ins()
            .store(flags(), zero, frame, child_offset as i32);
        self.builder
            .ins()
            .jump(self.blocks[successor.0 as usize], &arguments);
        Ok(())
    }

    // Reads the awaited handle's cached completion into `output`, and
    // continues in the caller's current block. The scheduler resumes an
    // await only after its handle completes, so a missing completion is an
    // internal protocol defect (`compiler.md` §94.1): the Context stops and
    // nothing re-registers, polls, or fabricates a result.
    fn read_completion(
        &mut self,
        handle: Value,
        output: Option<&(Value, Type, u32)>,
        pos: &Pos,
    ) -> Result<(), String> {
        let output_pointer =
            output.map_or_else(|| self.iconst(types::I64, 0), |(address, _, _)| *address);
        let output_size = self.iconst(
            types::I64,
            i64::from(output.map_or(0, |(_, _, size)| *size)),
        );
        let cached = self
            .call_runtime(
                self.ml.rt.async_result,
                &[self.ctx, handle, output_pointer, output_size],
                false,
            )?
            .ok_or_else(|| internal("await resume has no cached-result check"))?;
        let completed = self.builder.create_block();
        let missing = self.builder.create_block();
        self.builder
            .ins()
            .brif(cached, completed, &[], missing, &[]);
        self.builder.switch_to_block(missing);
        let pos_id = self.position_id(pos);
        let pos_id = self.iconst(types::I32, pos_id);
        self.call_runtime(
            self.ml.rt.async_missing_completion,
            &[self.ctx, pos_id],
            false,
        )?;
        let unwind = self.unwind_block();
        self.builder.ins().jump(unwind, &[]);
        self.builder.switch_to_block(completed);
        Ok(())
    }

    pub(super) fn emit_resume_adapters(&mut self, plan: &CoroutinePlan) -> Result<(), String> {
        for source in &self.function.blocks {
            let l::Terminator::Suspend {
                kind,
                pos,
                successor,
                resume_value,
                traps,
                ..
            } = &source.terminator
            else {
                continue;
            };
            let suspend = plan
                .suspends
                .get(&source.id)
                .ok_or_else(|| internal(format!("suspend block {} has no plan", source.id.0)))?;
            let adapter = self
                .resume_adapters
                .get(&source.id)
                .copied()
                .ok_or_else(|| internal(format!("suspend block {} has no adapter", source.id.0)))?;
            self.builder.switch_to_block(adapter);
            let frame = self
                .frame
                .ok_or_else(|| internal("resume adapter has no frame"))?;
            self.reload_epoch_check(frame, pos)?;
            if matches!(
                kind,
                l::SuspendKind::AsyncCall { .. } | l::SuspendKind::AsyncHandle { .. }
            ) {
                let child = self.builder.ins().load(
                    types::I64,
                    flags(),
                    frame,
                    suspend
                        .child
                        .ok_or_else(|| internal("async adapter has no child slot"))?
                        as i32,
                );
                match kind {
                    l::SuspendKind::AsyncCall { target, .. } => {
                        self.resume_async_child(source.id, target, child, suspend)?;
                    }
                    l::SuspendKind::AsyncHandle { .. } => {
                        self.resume_async_handle(source.id, child, suspend, traps)?;
                    }
                    _ => unreachable!(),
                }
                continue;
            }
            if resume_value.is_some() {
                return Err(internal("non-call suspension defines a resume value"));
            }
            let mut arguments = Vec::new();
            for slot in &suspend.arguments {
                arguments.extend(rv_args(self.load_value_type(
                    &slot.ty,
                    frame,
                    slot.offset as i32,
                )?));
            }
            let successor = self.blocks[successor.0 as usize];
            self.builder.ins().jump(successor, &arguments);
        }
        Ok(())
    }
}
