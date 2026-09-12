//! Block graph walk, instruction dispatch, assignment, and trap bookkeeping.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    pub(super) fn emit_graph(&mut self, out: &mut String) -> Result<(), String> {
        for root in self.graph_roots.clone() {
            self.emit_dominator_subtree(out, root)?;
        }
        Ok(())
    }

    fn emit_dominator_subtree(
        &mut self,
        out: &mut String,
        block_id: l::BlockId,
    ) -> Result<(), String> {
        let block = self
            .function
            .blocks
            .get(block_id.0 as usize)
            .filter(|block| block.id == block_id)
            .ok_or_else(|| internal(format!("block {} is missing", block_id.0)))?
            .clone();
        let _ = writeln!(out, "b{}:\n    ;\n    {{", block.id.0);
        let entry_clears = self.root_storage.clear_at_block_entry[block.id.0 as usize].clone();
        self.emit_root_clears(out, &entry_clears)?;
        for value in self.block_value_declarations[block.id.0 as usize].clone() {
            if self.delayed_declarations.contains_key(&self.value(value)) {
                continue;
            }
            let _ = writeln!(
                out,
                "    {} v{} = {};",
                self.emitter.value_ctype(self.value_type(value)?)?,
                value.0,
                self.emitter.zero(self.value_type(value)?)?
            );
        }
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            self.emit_instruction(out, instruction).map_err(|error| {
                internal(format!(
                    "function {} block {} instruction {:?}: {error}",
                    self.function.id.0, block.id.0, instruction.kind
                ))
            })?;
            let mut clears = self.root_storage.clear_after_instruction[block.id.0 as usize]
                [instruction_index]
                .clone();
            if let Some(result) = instruction
                .result
                .filter(|result| self.dead_forward_iterator_results.contains(result))
            {
                if let Some(slot) = self.root_storage.value_slots[result.0 as usize] {
                    // The result has no use, so C emits no assignment. Remove
                    // the clear for that omitted root assignment too.
                    clears.retain(|candidate| *candidate != slot);
                }
            }
            self.emit_root_clears(out, &clears)?;
        }
        self.emit_terminator(out, &block)?;
        for child in self.dominator_children[block.id.0 as usize].clone() {
            self.emit_dominator_subtree(out, child)?;
        }
        out.push_str("    }\n");
        Ok(())
    }

    fn emit_instruction(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
    ) -> Result<(), String> {
        let operands = instruction
            .operands
            .iter()
            .map(|operand| self.operand(operand))
            .collect::<Result<Vec<_>, _>>()?;
        let operand_types = instruction
            .operands
            .iter()
            .map(|operand| self.operand_type(operand))
            .collect::<Result<Vec<_>, _>>()?;
        let result = instruction.result.map(|id| self.value(id));
        match &instruction.kind {
            l::InstructionKind::Copy => {
                if let Some(id) = instruction.result {
                    if self.is_function_value(id)? {
                        return self.assign_function_value(out, id, &operands[0]);
                    }
                }
                self.assign(out, result, &operands[0])
            }
            l::InstructionKind::StringLiteral(text) => {
                let trap = self.take_pending_trap(&instruction.traps, l::TrapKind::Allocation)?;
                let pos = self.emitter.pos_id(&trap.pos);
                let data = self.emitter.language_string_pointer(text.as_bytes());
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_str_lit",
                    &[
                        "void*".into(),
                        "const unsigned char*".into(),
                        "uint64_t".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        data,
                        format!("{}ull", text.len()),
                        format!("{pos}u"),
                    ],
                );
                self.assign(out, result, &call)?;
                self.emit_pending_check(out);
                Ok(())
            }
            l::InstructionKind::Zero => {
                let id = instruction
                    .result
                    .ok_or_else(|| internal("Zero has no result"))?;
                let zero = self.emitter.zero(self.value_type(id)?)?;
                self.assign(out, result, &zero)
            }
            l::InstructionKind::LoadLocal(local) => self.assign(out, result, &self.local(*local)),
            l::InstructionKind::StoreLocal(local) => {
                let local = self.local(*local);
                if local == operands[0] {
                    Ok(())
                } else {
                    self.assign(out, Some(local), &operands[0])
                }
            }
            l::InstructionKind::AddressOfLocal(local) => {
                if instruction
                    .result
                    .is_some_and(|result| self.folded_addresses.contains(&result))
                {
                    Ok(())
                } else {
                    self.assign(out, result, &format!("&{}", self.local(*local)))
                }
            }
            l::InstructionKind::LoadGlobal(global) => self.assign(
                out,
                result,
                &format!("subscript_globals(ctx)->g{}", global.0),
            ),
            l::InstructionKind::StoreGlobal(global) => self.assign(
                out,
                Some(format!("subscript_globals(ctx)->g{}", global.0)),
                &operands[0],
            ),
            l::InstructionKind::AddressOfGlobal(global) => self.assign(
                out,
                result,
                &format!("&subscript_globals(ctx)->g{}", global.0),
            ),
            l::InstructionKind::FunctionRef(function) => self.assign(
                out,
                result,
                &format!("(SubFn){{ (void*)&sub_w{}, NULL }}", function.0),
            ),
            l::InstructionKind::Unary(operator) => {
                let expression = match operator {
                    l::UnaryOp::Neg => format!("(-({}))", operands[0]),
                    l::UnaryOp::Not => format!("(!({}))", operands[0]),
                    l::UnaryOp::BitNot => format!("(~({}))", operands[0]),
                };
                self.assign(out, result, &expression)
            }
            l::InstructionKind::Binary(operator) => {
                self.emit_binary(out, instruction, *operator, &operands, &operand_types)
            }
            l::InstructionKind::Cast | l::InstructionKind::Coerce => {
                self.emit_conversion(out, instruction, &operands[0], &operand_types[0])
            }
            l::InstructionKind::AllocateClass(class) => {
                self.emit_allocate_class(out, instruction, *class, result)
            }
            l::InstructionKind::BoxBoundaryValue { payload } => {
                self.emit_box_boundary_value(out, instruction, *payload, &operands[0], result)
            }
            l::InstructionKind::AddressOfValue => {
                let id = instruction
                    .result
                    .ok_or_else(|| internal("AddressOfValue has no result"))?;
                if self.coroutine {
                    let _ = writeln!(out, "    frame->stable_v{} = {};", id.0, operands[0]);
                    self.assign(out, result, &format!("&frame->stable_v{}", id.0))
                } else {
                    let temporary = self.fresh();
                    let pointee = match self.value_type(id)? {
                        l::ValueType::Address(address) => &address.pointee,
                        _ => return Err(internal("AddressOfValue result is not an address")),
                    };
                    let _ = writeln!(
                        out,
                        "    {} {temporary} = {};",
                        self.emitter.ctype(pointee)?,
                        operands[0]
                    );
                    self.assign(out, result, &format!("&{temporary}"))
                }
            }
            l::InstructionKind::AddressOfField(field) => {
                self.emit_field_address(out, instruction, *field, &operands, &operand_types, result)
            }
            l::InstructionKind::AddressOfIndex { checked } => self.emit_index_address(
                out,
                instruction,
                *checked,
                &operands,
                &operand_types,
                result,
            ),
            l::InstructionKind::LoadAddress => {
                let expression = match instruction.operands.first() {
                    Some(l::Operand::Value(address)) if self.folded_addresses.contains(address) => {
                        self.folded_address_expression(*address)?
                    }
                    _ => format!("*({})", operands[0]),
                };
                self.assign(out, result, &expression)
            }
            l::InstructionKind::StoreAddress => {
                let destination = match instruction.operands.first() {
                    Some(l::Operand::Value(address)) if self.folded_addresses.contains(address) => {
                        self.folded_address_expression(*address)?
                    }
                    _ => format!("*({})", operands[0]),
                };
                self.assign(out, Some(destination), &operands[1])
            }
            l::InstructionKind::LoadField(field) => {
                self.emit_load_field(out, instruction, *field, &operands, &operand_types, result)
            }
            l::InstructionKind::Length => {
                self.emit_length(out, &operands[0], &operand_types[0], result)
            }
            l::InstructionKind::ForeignArrayData => {
                let call = self.emitter.runtime_call(
                    "const void*",
                    "subscript_rt_array_data",
                    &["void*".into(), "const void*".into()],
                    &["ctx".into(), operands[0].clone()],
                );
                self.assign(out, result, &call)
            }
            l::InstructionKind::ArrayLiteral => {
                self.emit_array_literal(out, instruction, &operands, result)
            }
            l::InstructionKind::ArrayWithCapacity => {
                self.emit_array_with_capacity(out, instruction, &operands, result)
            }
            l::InstructionKind::ArraySpreadLiteral(spreads) => {
                self.emit_spread_array(out, instruction, spreads, &operands, &operand_types, result)
            }
            l::InstructionKind::SetFromSource(spread) => self.emit_set_from_source(
                out,
                instruction,
                *spread,
                &operands,
                &operand_types,
                result,
            ),
            l::InstructionKind::Template(parts) => {
                self.emit_template(out, instruction, parts, &operands, result)
            }
            l::InstructionKind::MakeClosure(function) => {
                self.emit_closure(out, instruction, *function, &operands, result)
            }
            l::InstructionKind::Call(target) => {
                self.emit_call(out, instruction, target, &operands, &operand_types, result)
            }
            l::InstructionKind::AsyncHandleCreate(target) => {
                let function = match target.kind {
                    l::CallTargetKind::Function(function) => function,
                    l::CallTargetKind::Method(method) => self.emitter.method_function(method)?,
                    ref other => {
                        return Err(internal(format!("held async target {other:?} is invalid")))
                    }
                };
                let separator = if operands.is_empty() { "" } else { ", " };
                let handle = result
                    .clone()
                    .ok_or_else(|| internal("async handle creation has no result"))?;
                self.assign(
                    out,
                    result,
                    &format!("sub_f{}(ctx{separator}{})", function.0, operands.join(", ")),
                )?;
                self.consume_runtime_traps(out, &instruction.traps, true, true)?;
                let (output, size) = if let Some(ty) = &target.return_type {
                    let value = self.fresh();
                    let _ = writeln!(
                        out,
                        "    {} {value} = {};",
                        self.emitter.value_ctype(ty)?,
                        self.emitter.zero(ty)?
                    );
                    (format!("&{value}"), format!("sizeof({value})"))
                } else {
                    ("NULL".into(), "0u".into())
                };
                let done = self.fresh();
                let _ = writeln!(
                    out,
                    "    uint8_t {done} = sub_f{}_resume(ctx, {handle}, {output});",
                    function.0
                );
                self.emit_pending_check(out);
                let complete = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_async_complete",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "uint64_t".into(),
                    ],
                    &["ctx".into(), handle, output, size],
                );
                let _ = writeln!(out, "    if ({done}) {complete};");
                Ok(())
            }
            l::InstructionKind::AsyncHandleRetain => {
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_async_retain",
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), operands[0].clone()],
                );
                let _ = writeln!(out, "    {call};");
                Ok(())
            }
            l::InstructionKind::AsyncHandleRelease => {
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_async_release",
                    &["void*".into(), "void*".into(), "uint32_t".into()],
                    &["ctx".into(), operands[0].clone(), format!("{pos}u")],
                );
                let _ = writeln!(out, "    {call};");
                Ok(())
            }
            l::InstructionKind::AsyncHandleArrayRetain => {
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_async_retain_array",
                    &["void*".into(), "const void*".into()],
                    &["ctx".into(), operands[0].clone()],
                );
                let _ = writeln!(out, "    {call};");
                Ok(())
            }
            l::InstructionKind::AsyncHandleArrayRelease => {
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_async_release_array",
                    &["void*".into(), "const void*".into(), "uint32_t".into()],
                    &["ctx".into(), operands[0].clone(), format!("{pos}u")],
                );
                let _ = writeln!(out, "    {call};");
                Ok(())
            }
            l::InstructionKind::IteratorCreate { kind, bound } => {
                let iterator_type = instruction
                    .result
                    .ok_or_else(|| internal("IteratorCreate has no result"))
                    .and_then(|result| self.value_type(result))?
                    .clone();
                let l::ValueType::Iterator(iterator_type) = iterator_type else {
                    return Err(internal("IteratorCreate result is not an iterator"));
                };
                self.emit_iterator_create(
                    out,
                    instruction,
                    *kind,
                    *bound,
                    &iterator_type,
                    &operands[0],
                    &operand_types[0],
                    result,
                )
            }
            l::InstructionKind::IteratorBound => {
                self.emit_iterator_bound(out, instruction, &operands[0], &operand_types[0], result)
            }
            l::InstructionKind::IteratorHasNext => {
                self.emit_iterator_has_next(out, instruction, &operands, &operand_types, result)
            }
            l::InstructionKind::IteratorValue => {
                self.emit_iterator_value(out, instruction, &operands, &operand_types, result)
            }
            l::InstructionKind::IteratorAdvance => {
                self.emit_iterator_advance(out, instruction, &operands, &operand_types, result)
            }
        }
    }

    pub(super) fn assign(
        &mut self,
        out: &mut String,
        destination: Option<String>,
        value: &str,
    ) -> Result<(), String> {
        let Some(destination) = destination else {
            return Ok(());
        };
        if let Some(delayed) = self.delayed_declarations.remove(&destination) {
            let ctype = self.emitter.value_ctype(self.value_type(delayed)?)?;
            let _ = writeln!(out, "    {ctype} {destination} = {value};");
            return Ok(());
        }
        let _ = writeln!(out, "    {destination} = {value};");
        Ok(())
    }

    pub(super) fn operand(&mut self, operand: &l::Operand) -> Result<String, String> {
        match operand {
            l::Operand::Value(value) => Ok(self.value(*value)),
            l::Operand::Constant(constant) => self.constant(constant),
        }
    }

    pub(super) fn operand_type(&self, operand: &l::Operand) -> Result<l::ValueType, String> {
        operand_type(self.function, operand)
    }

    pub(super) fn constant(&mut self, constant: &l::Constant) -> Result<String, String> {
        Ok(match &constant.kind {
            l::ConstantKind::Integer(value) => int_literal(*value, &constant.ty),
            l::ConstantKind::FloatBits(bits) => match constant.ty {
                Type::F16 => self.emitter.runtime_call(
                    "uint16_t",
                    "subscript_rt_f16_from_f64",
                    &["double".into()],
                    &[float_literal(f64::from_bits(*bits), &Type::F64)],
                ),
                Type::F32 => float_literal(f64::from(f32::from_bits(*bits as u32)), &constant.ty),
                Type::F64 => float_literal(f64::from_bits(*bits), &constant.ty),
                ref other => return Err(internal(format!("float bits have type {other:?}"))),
            },
            l::ConstantKind::Boolean(value) => i32::from(*value).to_string(),
            l::ConstantKind::Null => "NULL".into(),
        })
    }

    pub(super) fn take_pending_trap(
        &mut self,
        traps: &[l::Trap],
        kind: l::TrapKind,
    ) -> Result<l::Trap, String> {
        let trap = traps
            .iter()
            .find(|trap| trap.kind == kind)
            .cloned()
            .ok_or_else(|| internal(format!("operation has no {kind:?} trap")))?;
        self.consumed_traps.push(trap.clone());
        Ok(trap)
    }

    pub(super) fn consume(&mut self, trap: &l::Trap) {
        self.consumed_traps.push(trap.clone());
    }

    pub(super) fn emit_pending_check(&self, out: &mut String) {
        out.push_str("    if (*(const uint32_t*)ctx != 0u) goto unwind;\n");
    }

    pub(super) fn emit_pop(&mut self, out: &mut String) {
        if self.shadow_frame {
            let call = self.emitter.runtime_call(
                "void",
                "subscript_rt_shadow_pop",
                &["void*".into()],
                &["ctx".into()],
            );
            let _ = writeln!(out, "    {call};");
        }
    }

    pub(super) fn emit_unwind(&mut self, out: &mut String) -> Result<(), String> {
        out.push_str("unwind:\n    ;\n");
        self.emit_pop(out);
        if self.coroutine {
            out.push_str("    return 1;\ncoroutine_done:\n    ;\n    return 1;\n");
        } else if self.function.return_type == Type::Void {
            out.push_str("    return;\n");
        } else {
            let zero = self
                .emitter
                .zero(&l::ValueType::Data(self.function.return_type.clone()))?;
            let _ = writeln!(out, "    return {zero};");
        }
        Ok(())
    }
}
