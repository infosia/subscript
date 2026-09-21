//! Builtin methods and the runtime, date, and simple intrinsics.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    pub(super) fn emit_builtin_method(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        method: l::BuiltinMethod,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        match method {
            l::BuiltinMethod::ArrayPush => {
                let l::ValueType::Data(Type::Array(element)) = &operand_types[0] else {
                    return Err(internal("array push receiver is not an array"));
                };
                let header = self.fresh();
                let element_type = self.emitter.ctype(element)?;
                let _ = writeln!(
                    out,
                    "    SsArrayHeader* {header} = (SsArrayHeader*)({});",
                    operands[0]
                );
                if instruction.traps.is_empty() {
                    // Static callback lowering gives this push a dominating
                    // capacity bound. All ordinary pushes carry a Call trap.
                    let _ = writeln!(
                        out,
                        "    (({element_type}*){header}->data)[{header}->len] = {};",
                        operands[1]
                    );
                    let _ = writeln!(out, "    {header}->len += 1u;");
                    return self.assign(out, result, &format!("(int32_t)({header}->len)"));
                }
                let _ = writeln!(out, "    if ({header}->len < {header}->cap) {{");
                let _ = writeln!(
                    out,
                    "        (({element_type}*){header}->data)[{header}->len] = {};",
                    operands[1]
                );
                let _ = writeln!(out, "        {header}->len += 1u;\n    }} else {{");
                let pointer = self.materialize(out, &operands[1], element)?;
                let pos = self.emitter.pos_id(&instruction.pos);
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
                        operands[0].clone(),
                        pointer,
                        format!("{pos}u"),
                    ],
                );
                let _ = writeln!(out, "        (void){call};");
                self.consume_runtime_traps(out, &instruction.traps, true, true)?;
                out.push_str("    }\n");
                self.assign(out, result, &format!("(int32_t)({header}->len)"))
            }
            l::BuiltinMethod::ArrayPop => {
                let l::ValueType::Data(Type::Array(element)) = &operand_types[0] else {
                    return Err(internal("array pop receiver is not an array"));
                };
                let destination = result.ok_or_else(|| internal("array pop has no result"))?;
                let pos = self.emitter.pos_id(&instruction.pos);
                let call = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_array_pop",
                    &[
                        "void*".into(),
                        "void*".into(),
                        "void*".into(),
                        "uint32_t".into(),
                    ],
                    &[
                        "ctx".into(),
                        operands[0].clone(),
                        format!("&{destination}"),
                        format!("{pos}u"),
                    ],
                );
                let _ = element;
                let _ = writeln!(out, "    {call};");
                self.consume_runtime_traps(out, &instruction.traps, true, true)
            }
            l::BuiltinMethod::StringSlice => self.emit_simple_runtime_intrinsic(
                out,
                instruction,
                target,
                "subscript_rt_str_slice",
                operands,
                operand_types,
                true,
                result,
            ),
            l::BuiltinMethod::GeneratorNext => {
                let destination = result.ok_or_else(|| internal("Generator.next has no result"))?;
                let l::ValueType::Data(Type::IterResult(_)) = target
                    .return_type
                    .as_ref()
                    .ok_or_else(|| internal("Generator.next has no result type"))?
                else {
                    return Err(internal("Generator.next result is not IterResult"));
                };
                let _ = writeln!(
                    out,
                    "    {destination} = ({}){{0}};",
                    self.emitter
                        .value_ctype(target.return_type.as_ref().unwrap())?
                );
                let _ = writeln!(out, "    {destination}.done = ((SubCoroutinePrefix*)({}))->resume(ctx, {}, &{destination}.value);", operands[0], operands[0]);
                self.consume_runtime_traps(out, &instruction.traps, true, true)
            }
        }
    }

    pub(super) fn consume_runtime_traps(
        &mut self,
        out: &mut String,
        traps: &[l::Trap],
        pending: bool,
        accepts_dev_traps: bool,
    ) -> Result<(), String> {
        let mut check = false;
        for trap in traps {
            match trap.kind {
                l::TrapKind::DevOnlyLifetime => {
                    if accepts_dev_traps {
                        self.consume(trap);
                    }
                }
                l::TrapKind::DevReloadOnlyStaleCoroutine if accepts_dev_traps => {
                    self.consume(trap);
                }
                l::TrapKind::Allocation | l::TrapKind::Call => {
                    self.consume(trap);
                    check = true;
                }
                ref other => return Err(internal(format!("runtime call carries trap {other:?}"))),
            }
        }
        if pending && check {
            self.emit_pending_check(out);
        }
        Ok(())
    }

    pub(super) fn emit_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        intrinsic: &l::Intrinsic,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        let operation = self.emitter.operation(intrinsic)?;
        let name = operation.semantic_name.clone();
        let runtime_symbol = operation.runtime_symbol().map(str::to_owned);
        match intrinsic.family {
            l::IntrinsicFamily::Ambient => match name.as_str() {
                "Print" => self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    "subscript_rt_print",
                    operands,
                    operand_types,
                    false,
                    result,
                ),
                "Collect" => self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    "subscript_rt_collect",
                    operands,
                    operand_types,
                    false,
                    result,
                ),
                "UnsafeDelete" => self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    "subscript_rt_delete",
                    operands,
                    operand_types,
                    true,
                    result,
                ),
                "Unreachable" => {
                    let trap = instruction
                        .traps
                        .iter()
                        .find(|trap| trap.kind == l::TrapKind::Unreachable)
                        .ok_or_else(|| internal("Unreachable has no trap"))?
                        .clone();
                    self.consume(&trap);
                    let pos = self.emitter.pos_id(&trap.pos);
                    let call = self.emitter.runtime_call(
                        "void",
                        "subscript_rt_trap",
                        &["void*".into(), "uint32_t".into(), "uint32_t".into()],
                        &[
                            "ctx".into(),
                            format!("{}u", TrapKind::UnreachableReached as u32),
                            format!("{pos}u"),
                        ],
                    );
                    let _ = writeln!(out, "    {call};\n    goto unwind;");
                    Ok(())
                }
                other => Err(internal(format!("unknown Ambient intrinsic {other}"))),
            },
            l::IntrinsicFamily::Math => {
                let symbol = runtime_symbol
                    .as_deref()
                    .ok_or_else(|| internal(format!("Math.{name} has no runtime symbol")))?;
                self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    symbol,
                    operands,
                    operand_types,
                    false,
                    result,
                )
            }
            l::IntrinsicFamily::Number => {
                let symbol = runtime_symbol
                    .as_deref()
                    .ok_or_else(|| internal(format!("Number.{name} has no runtime symbol")))?;
                let position = !matches!(
                    name.as_str(),
                    "IsNaN" | "IsFinite" | "IsInteger" | "IsSafeInteger"
                );
                self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    symbol,
                    operands,
                    operand_types,
                    position,
                    result,
                )
            }
            l::IntrinsicFamily::Date => self.emit_date_intrinsic(
                out,
                instruction,
                target,
                &name,
                operands,
                operand_types,
                result,
            ),
            l::IntrinsicFamily::Json => {
                let symbol = runtime_symbol
                    .as_deref()
                    .ok_or_else(|| internal(format!("JSON.{name} has no runtime symbol")))?;
                self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    symbol,
                    operands,
                    operand_types,
                    true,
                    result,
                )
            }
            l::IntrinsicFamily::String => {
                let symbol = runtime_symbol
                    .as_deref()
                    .ok_or_else(|| internal(format!("String.{name} has no runtime symbol")))?;
                let position = !matches!(
                    name.as_str(),
                    "IndexOf" | "LastIndexOf" | "Includes" | "StartsWith" | "EndsWith"
                );
                self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    symbol,
                    operands,
                    operand_types,
                    position,
                    result,
                )
            }
            l::IntrinsicFamily::Regex => {
                let symbol = runtime_symbol
                    .as_deref()
                    .ok_or_else(|| internal(format!("Regex.{name} has no runtime symbol")))?;
                self.emit_simple_runtime_intrinsic(
                    out,
                    instruction,
                    target,
                    symbol,
                    operands,
                    operand_types,
                    true,
                    result,
                )
            }
            l::IntrinsicFamily::Array => self.emit_array_intrinsic(
                out,
                instruction,
                target,
                &name,
                operands,
                operand_types,
                result,
            ),
            l::IntrinsicFamily::Map => self.emit_map_intrinsic(
                out,
                instruction,
                target,
                &name,
                operands,
                operand_types,
                result,
            ),
            l::IntrinsicFamily::Set => self.emit_set_intrinsic(
                out,
                instruction,
                target,
                &name,
                runtime_symbol.as_deref(),
                operands,
                operand_types,
                result,
            ),
            l::IntrinsicFamily::ContextBytes => self.emit_context_bytes(
                out,
                instruction,
                target,
                intrinsic,
                &name,
                operands,
                operand_types,
                result,
            ),
            l::IntrinsicFamily::Worker => self.emit_worker_intrinsic(
                out,
                instruction,
                target,
                intrinsic,
                &name,
                runtime_symbol.as_deref(),
                operands,
                operand_types,
                result,
            ),
        }
    }

    pub(super) fn emit_simple_runtime_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        symbol: &str,
        operands: &[String],
        operand_types: &[l::ValueType],
        position: bool,
        result: Option<String>,
    ) -> Result<(), String> {
        let return_type = target
            .return_type
            .as_ref()
            .map(|ty| self.emitter.value_ctype(ty))
            .transpose()?
            .unwrap_or_else(|| "void".into());
        let mut argument_types = vec!["void*".to_string()];
        argument_types.extend(
            operand_types
                .iter()
                .map(|ty| self.emitter.value_ctype(ty))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let mut arguments = vec!["ctx".to_string()];
        arguments.extend_from_slice(operands);
        if position {
            argument_types.push("uint32_t".into());
            arguments.push(format!("{}u", self.emitter.pos_id(&instruction.pos)));
        }
        let call = self
            .emitter
            .runtime_call(&return_type, symbol, &argument_types, &arguments);
        if let Some(result) = result {
            let _ = writeln!(out, "    {result} = {call};");
        } else {
            let _ = writeln!(out, "    {call};");
        }
        self.consume_runtime_traps(out, &instruction.traps, true, true)
    }

    fn emit_date_intrinsic(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        target: &l::CallTarget,
        name: &str,
        operands: &[String],
        operand_types: &[l::ValueType],
        result: Option<String>,
    ) -> Result<(), String> {
        if let Some(field) = match name {
            "GetUtcFullYear" => Some(0),
            "GetUtcMonth" => Some(1),
            "GetUtcDate" => Some(2),
            "GetUtcDay" => Some(3),
            "GetUtcHours" => Some(4),
            "GetUtcMinutes" => Some(5),
            "GetUtcSeconds" => Some(6),
            "GetUtcMilliseconds" => Some(7),
            _ => None,
        } {
            let destination = result.ok_or_else(|| internal("Date getter has no result"))?;
            let call = self.emitter.runtime_call(
                "int32_t",
                "subscript_rt_date_get",
                &["void*".into(), "int64_t".into(), "uint32_t".into()],
                &["ctx".into(), operands[0].clone(), format!("{field}u")],
            );
            let _ = writeln!(out, "    {destination} = {call};");
            return self.consume_runtime_traps(out, &instruction.traps, true, true);
        }
        let (symbol, position) = match name {
            "New" => ("subscript_rt_date_new", true),
            "Utc" => ("subscript_rt_date_utc", true),
            "Now" => ("subscript_rt_date_now", false),
            "ToIso" => ("subscript_rt_date_to_iso", true),
            other => return Err(internal(format!("unknown Date intrinsic {other}"))),
        };
        self.emit_simple_runtime_intrinsic(
            out,
            instruction,
            target,
            symbol,
            operands,
            operand_types,
            position,
            result,
        )
    }
}
