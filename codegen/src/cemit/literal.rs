//! Array literals, template strings, value formatting, and closures.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    pub(super) fn materialize(
        &mut self,
        out: &mut String,
        value: &str,
        ty: &Type,
    ) -> Result<String, String> {
        let temporary = self.fresh();
        let _ = writeln!(
            out,
            "    {} {temporary} = {value};",
            self.emitter.ctype(ty)?
        );
        Ok(format!("&{temporary}"))
    }

    pub(super) fn emit_array_literal(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operands: &[String],
        result: Option<String>,
    ) -> Result<(), String> {
        let result_id = instruction
            .result
            .ok_or_else(|| internal("array literal has no result"))?;
        let result_type = data_type(self.value_type(result_id)?)?.clone();
        let destination = result.ok_or_else(|| internal("array literal result is missing"))?;
        match result_type {
            Type::FixedArray(element, count) => {
                if operands.len() != count as usize {
                    return Err(internal("fixed array literal arity mismatch"));
                }
                let initializer = format!(
                    "({}){{ .a = {{ {} }} }}",
                    self.emitter.fixed_array_name(&element, count)?,
                    operands.join(", ")
                );
                self.assign(out, Some(destination), &initializer)?;
                for trap in &instruction.traps {
                    self.consume(trap);
                }
                Ok(())
            }
            Type::Array(element) => {
                let mut traps = instruction.traps.iter();
                let initial = traps
                    .next()
                    .ok_or_else(|| internal("array literal has no allocation trap"))?;
                self.consume(initial);
                let pos = self.emitter.pos_id(&initial.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_array_new",
                    &["void*".into(), "uint64_t".into(), "uint32_t".into()],
                    &[
                        "ctx".into(),
                        format!("(uint64_t)sizeof({})", self.emitter.ctype(&element)?),
                        format!("{pos}u"),
                    ],
                );
                self.assign(out, Some(destination.clone()), &call)?;
                self.emit_pending_check(out);
                for operand in operands {
                    let trap = traps
                        .next()
                        .ok_or_else(|| internal("array push has no allocation trap"))?;
                    self.consume(trap);
                    let pos = self.emitter.pos_id(&trap.pos);
                    let pointer = self.materialize(out, operand, &element)?;
                    let call = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_array_push",
                        &[
                            "void*".into(),
                            "void*".into(),
                            "const void*".into(),
                            "uint32_t".into(),
                        ],
                        &[
                            "ctx".into(),
                            destination.clone(),
                            pointer,
                            format!("{pos}u"),
                        ],
                    );
                    let _ = writeln!(out, "    (void){call};");
                    self.emit_pending_check(out);
                }
                if traps.next().is_some() {
                    return Err(internal("array literal has unused traps"));
                }
                Ok(())
            }
            other => Err(internal(format!("array literal result is {other:?}"))),
        }
    }

    pub(super) fn emit_array_with_capacity(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operands: &[String],
        result: Option<String>,
    ) -> Result<(), String> {
        let result_id = instruction
            .result
            .ok_or_else(|| internal("capacity array has no result"))?;
        let Type::Array(element) = data_type(self.value_type(result_id)?)?.clone() else {
            return Err(internal("capacity array result is not an array"));
        };
        let capacity = operands
            .first()
            .ok_or_else(|| internal("capacity array has no bound"))?;
        let destination = result.ok_or_else(|| internal("capacity array result is missing"))?;
        let pos = if let Some(trap) = instruction.traps.first() {
            self.emitter.pos_id(&trap.pos)
        } else {
            self.emitter.pos_id(&instruction.pos)
        };
        let call = self.emitter.runtime_call(
            "void*",
            "subscript_rt_array_with_capacity",
            &[
                "void*".into(),
                "uint64_t".into(),
                "uint64_t".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                format!("(uint64_t)({capacity})"),
                format!("(uint64_t)sizeof({})", self.emitter.ctype(&element)?),
                format!("{pos}u"),
            ],
        );
        self.assign(out, Some(destination), &call)?;
        self.consume_runtime_traps(out, &instruction.traps, true, false)
    }

    pub(super) fn emit_spread_array(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        spreads: &[Option<l::SpreadKind>],
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let result_id = instruction
            .result
            .ok_or_else(|| internal("spread literal has no result"))?;
        let Type::Array(element) = data_type(self.value_type(result_id)?)? else {
            return Err(internal("spread literal result is not an array"));
        };
        let element = (**element).clone();
        let destination = result.ok_or_else(|| internal("spread result is missing"))?;
        let mut traps = instruction.traps.iter();
        let initial = traps
            .next()
            .ok_or_else(|| internal("spread literal has no allocation trap"))?;
        self.consume(initial);
        let pos = self.emitter.pos_id(&initial.pos);
        let call = self.emitter.runtime_call(
            "void*",
            "subscript_rt_array_new",
            &["void*".into(), "uint64_t".into(), "uint32_t".into()],
            &[
                "ctx".into(),
                format!("(uint64_t)sizeof({})", self.emitter.ctype(&element)?),
                format!("{pos}u"),
            ],
        );
        let _ = writeln!(out, "    {destination} = {call};");
        self.emit_pending_check(out);
        for ((spread, operand), operand_type) in spreads.iter().zip(operands).zip(operand_types) {
            let trap = traps
                .next()
                .ok_or_else(|| internal("spread part has no allocation trap"))?;
            self.consume(trap);
            let pos = self.emitter.pos_id(&trap.pos);
            let call = match spread {
                None => {
                    let pointer = self.materialize(out, operand, &element)?;
                    self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_array_push",
                        &[
                            "void*".into(),
                            "void*".into(),
                            "const void*".into(),
                            "uint32_t".into(),
                        ],
                        &[
                            "ctx".into(),
                            destination.clone(),
                            pointer,
                            format!("{pos}u"),
                        ],
                    )
                }
                Some(l::SpreadKind::Array) => self.emitter.runtime_call(
                    "void",
                    "subscript_rt_array_spread_array",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        destination.clone(),
                        operand.clone(),
                        format!("{pos}u"),
                    ],
                ),
                Some(l::SpreadKind::FixedArray) => {
                    let l::ValueType::Data(Type::FixedArray(_, count)) = operand_type else {
                        return Err(internal("fixed spread source type is invalid"));
                    };
                    self.emitter.runtime_call(
                        "void",
                        "subscript_rt_array_spread_fixed",
                        &[
                            "void*".into(),
                            "void*".into(),
                            "const void*".into(),
                            "uint64_t".into(),
                            "uint32_t".into(),
                        ],
                        &[
                            "ctx".into(),
                            destination.clone(),
                            format!("&({operand})"),
                            format!("{count}ull"),
                            format!("{pos}u"),
                        ],
                    )
                }
                Some(l::SpreadKind::SetValues) => self.emitter.runtime_call(
                    "void",
                    "subscript_rt_array_spread_assoc",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        destination.clone(),
                        operand.clone(),
                        format!("{pos}u"),
                    ],
                ),
                Some(l::SpreadKind::StringCodePoints) => self.emitter.runtime_call(
                    "void",
                    "subscript_rt_array_spread_string",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        destination.clone(),
                        operand.clone(),
                        format!("{pos}u"),
                    ],
                ),
            };
            let _ = writeln!(out, "    (void){call};");
            self.emit_pending_check(out);
        }
        if traps.next().is_some() {
            return Err(internal("spread literal has unused traps"));
        }
        Ok(())
    }

    pub(super) fn emit_template(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        parts: &[l::TemplatePart],
        operands: &[String],
        result: Option<String>,
    ) -> Result<(), String> {
        let destination = result.ok_or_else(|| internal("template has no result"))?;
        if parts.is_empty() {
            if !operands.is_empty() || !instruction.traps.is_empty() {
                return Err(internal("empty template carries operands or traps"));
            }
            let pos = self.emitter.pos_id(&instruction.pos);
            let empty = self.emitter.runtime_call(
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
                    format!("(const unsigned char*){}", c_string_literal(b"")),
                    "0ull".into(),
                    format!("{pos}u"),
                ],
            );
            return self.assign(out, Some(destination), &empty);
        }
        let mut trap_index = 0usize;
        let mut accumulated: Option<String> = None;
        for part in parts {
            let piece = match part {
                l::TemplatePart::Text(text) => {
                    let trap = instruction
                        .traps
                        .get(trap_index)
                        .ok_or_else(|| internal("template text has no allocation trap"))?;
                    trap_index += 1;
                    self.consume(trap);
                    let pos = self.emitter.pos_id(&trap.pos);
                    let data = self.emitter.language_string_pointer(text.as_bytes());
                    self.emitter.runtime_call(
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
                    )
                }
                l::TemplatePart::Operand { index, format } => {
                    let index = *index as usize;
                    if *format == l::FormatKind::Str {
                        operands[index].clone()
                    } else {
                        let trap = instruction
                            .traps
                            .get(trap_index)
                            .ok_or_else(|| internal("template format has no allocation trap"))?;
                        trap_index += 1;
                        self.consume(trap);
                        self.format_value(*format, &operands[index], trap)?
                    }
                }
            };
            let piece_name = self.fresh();
            let _ = writeln!(out, "    void* {piece_name} = {piece};");
            self.emit_pending_check(out);
            accumulated = Some(if let Some(previous) = accumulated {
                let trap = instruction
                    .traps
                    .get(trap_index)
                    .ok_or_else(|| internal("template concat has no allocation trap"))?;
                trap_index += 1;
                self.consume(trap);
                let pos = self.emitter.pos_id(&trap.pos);
                let call = self.emitter.runtime_call(
                    "void*",
                    "subscript_rt_str_concat",
                    &[
                        "void*".into(),
                        "const void*".into(),
                        "const void*".into(),
                        "uint32_t".into(),
                    ],
                    &["ctx".into(), previous, piece_name, format!("{pos}u")],
                );
                let concat = self.fresh();
                let _ = writeln!(out, "    void* {concat} = {call};");
                self.emit_pending_check(out);
                concat
            } else {
                piece_name
            });
        }
        if trap_index != instruction.traps.len() {
            return Err(internal(format!(
                "template consumed {trap_index} of {} traps",
                instruction.traps.len()
            )));
        }
        self.assign(
            out,
            Some(destination),
            accumulated.as_deref().unwrap_or("NULL"),
        )
    }

    fn format_value(
        &mut self,
        format: l::FormatKind,
        value: &str,
        trap: &l::Trap,
    ) -> Result<String, String> {
        let pos = self.emitter.pos_id(&trap.pos);
        if let l::FormatKind::StringAlias(alias) = format {
            let definition = self
                .emitter
                .module
                .string_aliases
                .get(alias.0)
                .ok_or_else(|| internal(format!("string alias {} is missing", alias.0)))?;
            let index = if let Some(wires) = &definition.wire_values {
                let expression = wires
                    .iter()
                    .enumerate()
                    .rev()
                    .fold("0".to_string(), |else_, (index, wire)| {
                        format!("(({value}) == {wire} ? {index} : ({else_}))")
                    });
                expression
            } else {
                format!("(uint32_t)({value})")
            };
            return Ok(self.emitter.runtime_call(
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
                    format!("sub_alias_{}[{index}].data", alias.0),
                    format!("sub_alias_{}[{index}].len", alias.0),
                    format!("{pos}u"),
                ],
            ));
        }
        let (name, ctype, argument) = match format {
            l::FormatKind::I32 => (
                "subscript_rt_fmt_i32",
                "int32_t",
                format!("(int32_t)({value})"),
            ),
            l::FormatKind::U32 => (
                "subscript_rt_fmt_u32",
                "uint32_t",
                format!("(uint32_t)({value})"),
            ),
            l::FormatKind::I64 => ("subscript_rt_fmt_i64", "int64_t", value.into()),
            l::FormatKind::U64 => ("subscript_rt_fmt_u64", "uint64_t", value.into()),
            l::FormatKind::F32 => ("subscript_rt_fmt_f32", "float", value.into()),
            l::FormatKind::F64 => ("subscript_rt_fmt_f64", "double", value.into()),
            l::FormatKind::F16 => {
                let wide = self.emitter.runtime_call(
                    "double",
                    "subscript_rt_f16_to_f64",
                    &["uint16_t".into()],
                    &[value.into()],
                );
                ("subscript_rt_fmt_f64", "double", wide)
            }
            l::FormatKind::Bool => (
                "subscript_rt_fmt_bool",
                "uint32_t",
                format!("(uint32_t)({value})"),
            ),
            l::FormatKind::Str | l::FormatKind::StringAlias(_) => {
                return Err(internal("direct string format reached numeric dispatch"))
            }
        };
        Ok(self.emitter.runtime_call(
            "void*",
            name,
            &["void*".into(), ctype.into(), "uint32_t".into()],
            &["ctx".into(), argument, format!("{pos}u")],
        ))
    }

    pub(super) fn emit_closure(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        function: l::FunctionId,
        operands: &[String],
        result: Option<String>,
    ) -> Result<(), String> {
        let destination = result.ok_or_else(|| internal("closure has no result"))?;
        let target = self.emitter.function(function)?;
        let captures = capture_parameters(target)
            .map(|parameter| parameter.value)
            .collect::<Vec<_>>();
        if captures.is_empty() {
            let _ = writeln!(
                out,
                "    {destination} = (SubFn){{ (void*)&sub_f{}, NULL }};",
                function.0
            );
            return Ok(());
        }
        let result_id = instruction
            .result
            .ok_or_else(|| internal("closure has no id"))?;
        let environment = self.closure_environment(result_id);
        let _ = writeln!(out, "    memset({environment}, 0, sizeof(SubEnvStorage));");
        for ((capture, operand), _) in captures.iter().zip(operands).zip(0..) {
            let _ = writeln!(
                out,
                "    ((SubEnv{}*){environment})->c{} = {operand};",
                function.0, capture.0
            );
        }
        let _ = writeln!(
            out,
            "    {destination} = (SubFn){{ (void*)&sub_f{}, {environment} }};",
            function.0
        );
        Ok(())
    }
}
