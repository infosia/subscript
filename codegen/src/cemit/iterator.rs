//! Iterator creation, bounds, and traversal.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    pub(super) fn emit_iterator_create(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        kind: l::ForOfKind,
        bound_kind: l::IteratorBoundKind,
        iterator_type: &l::IteratorType,
        subject: &str,
        subject_type: &l::ValueType,
        result: Option<String>,
    ) -> Result<(), String> {
        let destination = result.ok_or_else(|| internal("iterator creation has no result"))?;
        let subject = match (kind, subject_type) {
            (l::ForOfKind::FixedArrayValues, l::ValueType::Data(Type::FixedArray(_, count))) => {
                let _ = count;
                format!("(void*)&({subject})")
            }
            (l::ForOfKind::FixedArrayValues, other) => {
                return Err(internal(format!(
                    "fixed-array iterator source is {other:?}"
                )))
            }
            _ => format!("(void*)({subject})"),
        };
        let temporary = format!("(SubIter){{ {subject}, 0ull, 0ull, 0ull }}");
        let bound = match (kind, subject_type) {
            (l::ForOfKind::FixedArrayValues, l::ValueType::Data(Type::FixedArray(_, count))) => {
                format!("{count}ull")
            }
            _ => self.iterator_current_bound_expression(instruction, &temporary, iterator_type)?,
        };
        let fixed = u32::from(bound_kind == l::IteratorBoundKind::Fixed);
        let position = if matches!(
            kind,
            l::ForOfKind::ArrayValuesReverse | l::ForOfKind::ArrayKeysReverse
        ) {
            format!("((uint64_t)({bound}) == 0ull ? UINT64_MAX : (uint64_t)({bound}) - 1ull)")
        } else {
            "0ull".to_string()
        };
        let _ = writeln!(
            out,
            "    {destination} = (SubIter){{ {subject}, {position}, (uint64_t)({bound}), {fixed}ull }};"
        );
        if matches!(
            kind,
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues
        ) {
            let candidate = self.fresh();
            let active = self.fresh();
            let value = self.fresh();
            let select = u32::from(kind == l::ForOfKind::MapValues);
            let pos = self.emitter.pos_id(&instruction.pos);
            let call = self.emitter.runtime_call(
                "int32_t",
                "subscript_rt_assoc_iter_copy",
                &[
                    "void*".into(),
                    "void*".into(),
                    "uint64_t".into(),
                    "uint32_t".into(),
                    "void*".into(),
                    "uint32_t".into(),
                ],
                &[
                    "ctx".into(),
                    format!("{destination}.subject"),
                    candidate.clone(),
                    format!("{select}u"),
                    format!("&{value}"),
                    format!("{pos}u"),
                ],
            );
            let _ = writeln!(
                out,
                "    {} {value} = {{0}};\n    uint64_t {candidate} = 0ull;\n    int32_t {active} = 0;",
                self.emitter.ctype(&iterator_type.element)?
            );
            let _ = writeln!(out, "    while ({candidate} < {destination}.bound) {{ {active} = {call}; if ({active}) break; {candidate}++; }}\n    {destination}.position = {candidate};");
        }
        Ok(())
    }

    fn iterator_type<'a>(
        &self,
        operand_type: &'a l::ValueType,
    ) -> Result<&'a l::IteratorType, String> {
        match operand_type {
            l::ValueType::Iterator(iterator) => Ok(iterator),
            other => Err(internal(format!("iterator operand has type {other:?}"))),
        }
    }

    pub(super) fn emit_iterator_bound(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        iterator: &str,
        iterator_type: &l::ValueType,
        result: Option<String>,
    ) -> Result<(), String> {
        let iterator_type = self.iterator_type(iterator_type)?;
        if instruction.operands.first().is_some_and(
            |operand| matches!(operand, l::Operand::Value(value) if self.fixed_iterators.contains(value)),
        ) {
            return self.assign(out, result, &format!("(int32_t)(({iterator}).bound)"));
        }
        let current =
            self.iterator_current_bound_expression(instruction, iterator, iterator_type)?;
        let expression =
            format!("(({iterator}).fixed != 0ull ? ({iterator}).bound : (uint64_t)({current}))");
        self.assign(out, result, &format!("(int32_t)({expression})"))
    }

    fn iterator_current_bound_expression(
        &mut self,
        instruction: &l::Instruction,
        iterator: &str,
        iterator_type: &l::IteratorType,
    ) -> Result<String, String> {
        Ok(match iterator_type.kind {
            l::ForOfKind::ArrayValues
            | l::ForOfKind::ArrayKeys
            | l::ForOfKind::ArrayValuesReverse
            | l::ForOfKind::ArrayKeysReverse => {
                format!("((const SsArrayHeader*)({iterator}).subject)->len")
            }
            l::ForOfKind::FixedArrayValues => {
                format!("({iterator}).bound")
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let pos = self.emitter.pos_id(&instruction.pos);
                self.emitter.runtime_call(
                    "uint64_t",
                    "subscript_rt_assoc_iter_begin",
                    &["void*".into(), "void*".into(), "uint32_t".into()],
                    &[
                        "ctx".into(),
                        format!("({iterator}).subject"),
                        format!("{pos}u"),
                    ],
                )
            }
            l::ForOfKind::StringCodePoints => self.emitter.runtime_call(
                "int32_t",
                "subscript_rt_str_len",
                &["void*".into(), "const void*".into()],
                &["ctx".into(), format!("({iterator}).subject")],
            ),
        })
    }

    pub(super) fn emit_iterator_has_next(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let iterator = self.iterator_type(&operand_types[0])?.clone();
        let destination = result.ok_or_else(|| internal("iterator condition has no result"))?;
        let current =
            self.iterator_current_bound_expression(instruction, &operands[0], &iterator)?;
        let position = if matches!(
            iterator.kind,
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayKeys
        ) {
            operands[1].clone()
        } else {
            format!("{}.position", operands[0])
        };
        let fixed = instruction.operands.first().is_some_and(
            |operand| matches!(operand, l::Operand::Value(value) if self.fixed_iterators.contains(value)),
        );
        if fixed {
            let _ = writeln!(out, "    {destination} = ((uint64_t)({position}) < (uint64_t)({current}) && (uint64_t)({position}) < (uint32_t)({}));", operands[2]);
        } else {
            let _ = writeln!(out, "    {destination} = ((uint64_t)({position}) < (uint64_t)({current}) && ({}.fixed == 0ull || (uint64_t)({position}) < (uint32_t)({})));", operands[0], operands[2]);
        }
        Ok(())
    }

    pub(super) fn emit_iterator_value(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let iterator = self.iterator_type(&operand_types[0])?.clone();
        let destination = result.ok_or_else(|| internal("iterator value has no result"))?;
        match iterator.kind {
            l::ForOfKind::ArrayKeys | l::ForOfKind::ArrayKeysReverse => {
                let position = if iterator.kind == l::ForOfKind::ArrayKeys {
                    operands[1].as_str()
                } else {
                    return self.assign(
                        out,
                        Some(destination),
                        &format!("(int32_t)({}.position)", operands[0]),
                    );
                };
                let _ = writeln!(out, "    {destination} = (int32_t)({position});");
            }
            l::ForOfKind::ArrayValues | l::ForOfKind::ArrayValuesReverse => {
                let position = if iterator.kind == l::ForOfKind::ArrayValues {
                    operands[1].clone()
                } else {
                    format!("{}.position", operands[0])
                };
                let _ = writeln!(
                    out,
                    "    {destination} = (({}*)(((const SsArrayHeader*)({}.subject))->data))[{position}];",
                    self.emitter.ctype(&iterator.element)?,
                    operands[0]
                );
            }
            l::ForOfKind::FixedArrayValues => {
                let _ = writeln!(
                    out,
                    "    {destination} = (({}*)({}.subject))[{}.position];",
                    self.emitter.ctype(&iterator.element)?,
                    operands[0],
                    operands[0]
                );
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let select = i32::from(iterator.kind == l::ForOfKind::MapValues);
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_assoc_iter_copy",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "uint64_t".into(),
                        "uint32_t".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("({}).subject", operands[0]),
                        format!("({}).position", operands[0]),
                        format!("{select}u"),
                        format!("&{destination}"),
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(out, "    (void){call};");
                self.emit_pending_check(out);
            }
            l::ForOfKind::StringCodePoints => {
                let next = self.fresh();
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_str_iter_code_point",
                    &[
                        "void*".into(),
                        "const void*".into(),
                        "int32_t".into(),
                        "int32_t*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("({}).subject", operands[0]),
                        format!("(int32_t)({}.position)", operands[0]),
                        format!("&{next}"),
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(out, "    int32_t {next} = 0;\n    {destination} = {call};");
                self.emit_pending_check(out);
            }
        }
        Ok(())
    }

    pub(super) fn emit_iterator_advance(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let iterator = self.iterator_type(&operand_types[0])?.clone();
        let destination = result.ok_or_else(|| internal("iterator advance has no result"))?;
        if instruction
            .result
            .is_some_and(|result| self.dead_forward_iterator_results.contains(&result))
        {
            // The explicit index advances the forward loop. LIR retains this
            // protocol instruction, but its unused aggregate result has no C effect.
            return Ok(());
        }
        let _ = writeln!(out, "    {destination} = {};", operands[0]);
        match iterator.kind {
            l::ForOfKind::ArrayValues
            | l::ForOfKind::ArrayKeys
            | l::ForOfKind::FixedArrayValues => {
                let _ = writeln!(
                    out,
                    "    {destination}.position = ({}).position + 1ull;",
                    operands[0]
                );
            }
            l::ForOfKind::ArrayValuesReverse | l::ForOfKind::ArrayKeysReverse => {
                let current_bound =
                    self.iterator_current_bound_expression(instruction, &operands[0], &iterator)?;
                let effective = self.fresh();
                let prior = self.fresh();
                let _ = writeln!(
                    out,
                    "    uint64_t {effective} = (uint64_t)({current_bound});\n    if ({destination}.fixed != 0ull && (uint64_t)({}) < {effective}) {effective} = (uint64_t)({});\n    uint64_t {prior} = ({}).position == 0ull ? UINT64_MAX : ({}).position - 1ull;\n    {destination}.position = ({effective} == 0ull || {prior} == UINT64_MAX) ? UINT64_MAX : ({prior} < {effective} - 1ull ? {prior} : {effective} - 1ull);",
                    operands[2], operands[2], operands[0], operands[0]
                );
            }
            l::ForOfKind::StringCodePoints => {
                let next = self.fresh();
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_str_iter_code_point",
                    &[
                        "void*".into(),
                        "const void*".into(),
                        "int32_t".into(),
                        "int32_t*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("({}).subject", operands[0]),
                        format!("(int32_t)({}.position)", operands[0]),
                        format!("&{next}"),
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(out, "    int32_t {next} = 0;\n    (void){call};\n    {destination}.position = (uint64_t){next};");
                self.emit_pending_check(out);
            }
            l::ForOfKind::MapKeys | l::ForOfKind::MapValues | l::ForOfKind::SetValues => {
                let current =
                    self.iterator_current_bound_expression(instruction, &operands[0], &iterator)?;
                let limit = self.fresh();
                let candidate = self.fresh();
                let active = self.fresh();
                let value = self.fresh();
                let select = u32::from(iterator.kind == l::ForOfKind::MapValues);
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "int32_t",
                    "subscript_rt_assoc_iter_copy",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "uint64_t".into(),
                        "uint32_t".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        format!("{destination}.subject"),
                        candidate.clone(),
                        format!("{select}u"),
                        format!("&{value}"),
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(out, "    uint64_t {limit} = (uint64_t)({current});\n    if ({destination}.fixed != 0ull && (uint64_t)({}) < {limit}) {limit} = (uint64_t)({});\n    {} {value} = {{0}};\n    uint64_t {candidate} = {destination}.position + 1ull;\n    int32_t {active} = 0;", operands[2], operands[2], self.emitter.ctype(&iterator.element)?);
                let _ = writeln!(out, "    while ({candidate} < {limit}) {{ {active} = {call}; if ({active}) break; {candidate}++; }}\n    {destination}.position = {candidate};");
            }
        }
        Ok(())
    }
}
