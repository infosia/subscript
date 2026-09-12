//! Array and set literals, value formatting, and template strings.

use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn materialize(&mut self, value: RV, ty: &Type) -> Result<Value, String> {
        match self.ml.layouts.repr(ty)? {
            Repr::None => Ok(self.iconst(types::I64, 0)),
            Repr::Agg { .. } => self.expect_aggregate(value),
            Repr::Scalar(repr) => {
                let slot = self.stack_slot(repr.bytes(), repr.bytes());
                let value = self.expect_scalar(value)?;
                self.builder.ins().store(flags(), value, slot, 0);
                Ok(slot)
            }
            Repr::Pair => {
                let slot = self.stack_slot(16, 8);
                let (code, env) = self.expect_pair(value)?;
                self.builder.ins().store(flags(), code, slot, 0);
                self.builder.ins().store(flags(), env, slot, 8);
                Ok(slot)
            }
        }
    }

    pub(super) fn array_literal(
        &mut self,
        result_ty: &Type,
        operands: &[RV],
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        match result_ty {
            Type::FixedArray(element, count) => {
                if operands.len() != *count as usize {
                    return Err(internal("fixed array literal arity mismatch"));
                }
                let (size, align) = self.ml.layouts.size_align(result_ty)?;
                let destination = self.stack_slot(size, align);
                let stride = self.ml.layouts.stride(element)?;
                for (index, value) in operands.iter().copied().enumerate() {
                    let offset = u32::try_from(index)
                        .ok()
                        .and_then(|index| index.checked_mul(stride))
                        .and_then(|offset| i32::try_from(offset).ok())
                        .ok_or_else(|| internal("fixed array literal offset overflows"))?;
                    self.store_data(element, destination, offset, value)?;
                }
                Ok(RV::Aggregate(destination))
            }
            Type::Array(element) => {
                let stride = self.ml.layouts.stride(element)?;
                let stride = self.iconst(types::I64, i64::from(stride));
                let first_position = traps.first().map_or(pos, |trap| &trap.pos);
                let position = self.position_id(first_position);
                let position = self.iconst(types::I32, position);
                let handle = self
                    .call_runtime(self.ml.rt.array_new, &[self.ctx, stride, position], false)?
                    .ok_or_else(|| internal("array literal allocation has no result"))?;
                if let Some(trap) = traps.first() {
                    self.emit_trap(trap, TrapOperand::Pending)?;
                }
                for (index, value) in operands.iter().copied().enumerate() {
                    let pointer = self.materialize(value, element)?;
                    let trap = traps.get(index + 1);
                    let position = trap.map_or(pos, |trap| &trap.pos);
                    let position = self.position_id(position);
                    let position = self.iconst(types::I32, position);
                    self.call_runtime(
                        self.ml.rt.array_push,
                        &[self.ctx, handle, pointer, position],
                        false,
                    )?;
                    if let Some(trap) = trap {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                }
                Ok(RV::Scalar(handle))
            }
            other => Err(internal(format!(
                "array literal has invalid type {other:?}"
            ))),
        }
    }

    pub(super) fn array_with_capacity(
        &mut self,
        result_ty: &Type,
        capacity: RV,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let Type::Array(element) = result_ty else {
            return Err(internal("capacity array result is not an array"));
        };
        let capacity = self.expect_scalar(capacity)?;
        let capacity = if self.builder.func.dfg.value_type(capacity) == types::I64 {
            capacity
        } else {
            self.builder.ins().uextend(types::I64, capacity)
        };
        let stride = self.ml.layouts.stride(element)?;
        let stride = self.iconst(types::I64, i64::from(stride));
        let diagnostic = traps.first().map_or(pos, |trap| &trap.pos);
        let diagnostic = self.position_id(diagnostic);
        let diagnostic = self.iconst(types::I32, diagnostic);
        let mut signature = Signature::new(self.ml.call_conv);
        for ty in [types::I64, types::I64, types::I64, types::I32] {
            signature.params.push(AbiParam::new(ty));
        }
        signature.returns.push(AbiParam::new(types::I64));
        let function = self
            .ml
            .module
            .declare_function(
                "subscript_rt_array_with_capacity",
                Linkage::Import,
                &signature,
            )
            .map_err(|error| internal(format!("declare array capacity allocator: {error}")))?;
        let handle = self
            .call_runtime(function, &[self.ctx, capacity, stride, diagnostic], false)?
            .ok_or_else(|| internal("capacity array allocation has no result"))?;
        for trap in traps {
            if trap.kind == l::TrapKind::Call {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        Ok(RV::Scalar(handle))
    }

    /// Lowers `new Set<K>(source)` as one fused runtime traversal
    /// (compiler.md §103.1 rule 4).
    pub(super) fn set_from_source(
        &mut self,
        result_ty: &Type,
        spread: l::SpreadKind,
        operands: &[RV],
        operand_types: &[l::ValueType],
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let Type::Set(key) = result_ty else {
            return Err(internal("Set source construction result is not a Set"));
        };
        let (key_size, _) = self.ml.layouts.size_align(key)?;
        let kind = association_key_kind(self.ml.lir, key)?;
        let position = traps.first().map_or(pos, |trap| &trap.pos);
        let position = self.position_id(position);
        let key_size = self.iconst(types::I64, i64::from(key_size));
        let kind = self.iconst(types::I32, i64::from(kind));
        let position = self.iconst(types::I32, position);
        let value = *operands
            .first()
            .ok_or_else(|| internal("Set source construction has no operand"))?;
        let (function, arguments) = match spread {
            l::SpreadKind::Array => {
                let source = self.expect_scalar(value)?;
                (
                    self.ml.rt.set_from_array,
                    vec![self.ctx, source, key_size, kind, position],
                )
            }
            l::SpreadKind::FixedArray => {
                let source = self.expect_aggregate(value)?;
                let l::ValueType::Data(Type::FixedArray(_, count)) = operand_types
                    .first()
                    .ok_or_else(|| internal("Set fixed source has no type"))?
                else {
                    return Err(internal("Set fixed source has invalid source type"));
                };
                let count = self.iconst(types::I64, i64::from(*count));
                (
                    self.ml.rt.set_from_fixed,
                    vec![self.ctx, source, count, key_size, kind, position],
                )
            }
            l::SpreadKind::SetValues => {
                let source = self.expect_scalar(value)?;
                (
                    self.ml.rt.set_from_assoc,
                    vec![self.ctx, source, key_size, kind, position],
                )
            }
            l::SpreadKind::StringCodePoints => {
                let source = self.expect_scalar(value)?;
                (
                    self.ml.rt.set_from_string,
                    vec![self.ctx, source, key_size, kind, position],
                )
            }
        };
        let handle = self
            .call_runtime(function, &arguments, false)?
            .ok_or_else(|| internal("Set source construction has no result"))?;
        for trap in traps {
            self.emit_trap(trap, TrapOperand::Pending)?;
        }
        Ok(RV::Scalar(handle))
    }

    pub(super) fn spread_array_literal(
        &mut self,
        result_ty: &Type,
        spreads: &[Option<l::SpreadKind>],
        operands: &[RV],
        operand_types: &[l::ValueType],
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let Type::Array(element) = result_ty else {
            return Err(internal("spread literal result is not an array"));
        };
        let stride = self.ml.layouts.stride(element)?;
        let stride = self.iconst(types::I64, i64::from(stride));
        let first_position = traps.first().map_or(pos, |trap| &trap.pos);
        let position = self.position_id(first_position);
        let position = self.iconst(types::I32, position);
        let handle = self
            .call_runtime(self.ml.rt.array_new, &[self.ctx, stride, position], false)?
            .ok_or_else(|| internal("spread literal allocation has no result"))?;
        if let Some(trap) = traps.first() {
            self.emit_trap(trap, TrapOperand::Pending)?;
        }
        for (index, ((spread, value), value_ty)) in
            spreads.iter().zip(operands).zip(operand_types).enumerate()
        {
            let trap = traps.get(index + 1);
            let position = trap.map_or(pos, |trap| &trap.pos);
            let position = self.position_id(position);
            let position = self.iconst(types::I32, position);
            match spread {
                None => {
                    let pointer = self.materialize(*value, element)?;
                    self.call_runtime(
                        self.ml.rt.array_push,
                        &[self.ctx, handle, pointer, position],
                        false,
                    )?;
                }
                Some(l::SpreadKind::Array) => {
                    let source = self.expect_scalar(*value)?;
                    self.call_runtime(
                        self.ml.rt.array_spread_array,
                        &[self.ctx, handle, source, position],
                        false,
                    )?;
                }
                Some(l::SpreadKind::FixedArray) => {
                    let source = self.expect_aggregate(*value)?;
                    let l::ValueType::Data(Type::FixedArray(_, count)) = value_ty else {
                        return Err(internal("fixed spread has invalid source type"));
                    };
                    let count = self.iconst(types::I64, i64::from(*count));
                    self.call_runtime(
                        self.ml.rt.array_spread_fixed,
                        &[self.ctx, handle, source, count, position],
                        false,
                    )?;
                }
                Some(l::SpreadKind::SetValues) => {
                    let source = self.expect_scalar(*value)?;
                    self.call_runtime(
                        self.ml.rt.array_spread_assoc,
                        &[self.ctx, handle, source, position],
                        false,
                    )?;
                }
                Some(l::SpreadKind::StringCodePoints) => {
                    let source = self.expect_scalar(*value)?;
                    self.call_runtime(
                        self.ml.rt.array_spread_string,
                        &[self.ctx, handle, source, position],
                        false,
                    )?;
                }
            }
            if let Some(trap) = trap {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        Ok(RV::Scalar(handle))
    }

    fn format_value(
        &mut self,
        value: RV,
        format: l::FormatKind,
        trap: Option<&l::Trap>,
        pos: &Pos,
    ) -> Result<Value, String> {
        let value = self.expect_scalar(value)?;
        if format == l::FormatKind::Str {
            return Ok(value);
        }
        let position = trap.map_or(pos, |trap| &trap.pos);
        let position = self.position_id(position);
        let position = self.iconst(types::I32, position);
        if let l::FormatKind::StringAlias(alias) = format {
            let table = self.ml.string_alias_table_data(alias)?;
            let global = self
                .ml
                .module
                .declare_data_in_func(table, self.builder.func);
            let base = self.builder.ins().symbol_value(types::I64, global);
            let definition = self
                .ml
                .lir
                .string_aliases
                .get(alias.0)
                .ok_or_else(|| internal(format!("string alias {} is missing", alias.0)))?;
            let index = if let Some(wire_values) = &definition.wire_values {
                let mut selected = self.iconst(types::I64, 0);
                for (index, wire) in wire_values.iter().enumerate() {
                    let equal = self
                        .builder
                        .ins()
                        .icmp_imm(IntCC::Equal, value, i64::from(*wire));
                    let index = self.iconst(types::I64, index as i64);
                    selected = self.builder.ins().select(equal, index, selected);
                }
                selected
            } else {
                self.builder.ins().uextend(types::I64, value)
            };
            let offset = self.builder.ins().ishl_imm(index, 4);
            let entry = self.builder.ins().iadd(base, offset);
            let pointer = self.builder.ins().load(types::I64, flags(), entry, 0);
            let length = self.builder.ins().load(types::I64, flags(), entry, 8);
            let result = self
                .call_runtime(
                    self.ml.rt.str_lit,
                    &[self.ctx, pointer, length, position],
                    false,
                )?
                .ok_or_else(|| internal("string alias formatting has no result"))?;
            if let Some(trap) = trap {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
            return Ok(result);
        }
        let (function, argument) = match format {
            l::FormatKind::I32 => {
                let argument = if self.builder.func.dfg.value_type(value) == types::I32 {
                    value
                } else {
                    self.builder.ins().sextend(types::I32, value)
                };
                (self.ml.rt.fmt_i32, argument)
            }
            l::FormatKind::U32 => {
                let argument = if self.builder.func.dfg.value_type(value) == types::I32 {
                    value
                } else {
                    self.builder.ins().uextend(types::I32, value)
                };
                (self.ml.rt.fmt_u32, argument)
            }
            l::FormatKind::I64 => (self.ml.rt.fmt_i64, value),
            l::FormatKind::U64 => (self.ml.rt.fmt_u64, value),
            l::FormatKind::F32 => (self.ml.rt.fmt_f32, value),
            l::FormatKind::F64 => (self.ml.rt.fmt_f64, value),
            l::FormatKind::F16 => {
                let wide = self
                    .call_runtime(self.ml.rt.f16_to_f64, &[value], false)?
                    .ok_or_else(|| internal("f16 formatting conversion has no result"))?;
                (self.ml.rt.fmt_f64, wide)
            }
            l::FormatKind::Bool => (
                self.ml.rt.fmt_bool,
                self.builder.ins().uextend(types::I32, value),
            ),
            l::FormatKind::Str | l::FormatKind::StringAlias(_) => {
                return Err(internal("direct string format reached numeric dispatch"))
            }
        };
        let result = self
            .call_runtime(function, &[self.ctx, argument, position], false)?
            .ok_or_else(|| internal("formatting runtime call has no result"))?;
        if let Some(trap) = trap {
            self.emit_trap(trap, TrapOperand::Pending)?;
        }
        Ok(result)
    }

    pub(super) fn template(
        &mut self,
        parts: &[l::TemplatePart],
        operands: &[RV],
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let mut trap_cursor = 0usize;
        let mut accumulated = None;
        for part in parts {
            let piece = match part {
                l::TemplatePart::Text(text) => {
                    let trap = traps.get(trap_cursor);
                    trap_cursor += usize::from(trap.is_some());
                    let piece =
                        self.string_literal(text, trap.map_or(&[], std::slice::from_ref), pos)?;
                    self.expect_scalar(piece)?
                }
                l::TemplatePart::Operand { index, format } => {
                    let index = *index as usize;
                    let value = *operands
                        .get(index)
                        .ok_or_else(|| internal("template operand index is out of range"))?;
                    let trap = (*format != l::FormatKind::Str)
                        .then(|| traps.get(trap_cursor))
                        .flatten();
                    trap_cursor += usize::from(trap.is_some());
                    self.format_value(value, *format, trap, pos)?
                }
            };
            accumulated = Some(match accumulated {
                None => piece,
                Some(previous) => {
                    let trap = traps.get(trap_cursor);
                    trap_cursor += usize::from(trap.is_some());
                    let position = trap.map_or(pos, |trap| &trap.pos);
                    let position = self.position_id(position);
                    let position = self.iconst(types::I32, position);
                    let result = self
                        .call_runtime(
                            self.ml.rt.str_concat,
                            &[self.ctx, previous, piece, position],
                            false,
                        )?
                        .ok_or_else(|| internal("template concatenation has no result"))?;
                    if let Some(trap) = trap {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                    result
                }
            });
        }
        if let Some(value) = accumulated {
            Ok(RV::Scalar(value))
        } else {
            self.string_literal("", traps, pos)
        }
    }
}
