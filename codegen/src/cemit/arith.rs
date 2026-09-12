//! Binary operators, numeric conversions, and guards.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    pub(super) fn emit_binary(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operator: l::BinaryOp,
        operands: &[String],
        operand_types: &[l::ValueType],
    ) -> Result<(), String> {
        let destination = instruction
            .result
            .map(|result| self.value(result))
            .ok_or_else(|| internal("binary instruction has no result"))?;
        let ty = data_type(
            operand_types
                .first()
                .ok_or_else(|| internal("binary operand type is missing"))?,
        )?;
        if *ty == Type::Str {
            return match operator {
                l::BinaryOp::Add => {
                    let trap =
                        self.take_pending_trap(&instruction.traps, l::TrapKind::Allocation)?;
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
                        &[
                            "ctx".into(),
                            operands[0].clone(),
                            operands[1].clone(),
                            format!("{pos}u"),
                        ],
                    );
                    let _ = writeln!(out, "    {destination} = {call};");
                    self.emit_pending_check(out);
                    Ok(())
                }
                l::BinaryOp::Eq | l::BinaryOp::Ne => {
                    let call = self.emitter.runtime_call(
                        "int32_t",
                        "subscript_rt_str_eq",
                        &["void*".into(), "const void*".into(), "const void*".into()],
                        &["ctx".into(), operands[0].clone(), operands[1].clone()],
                    );
                    let negation = if operator == l::BinaryOp::Ne { "!" } else { "" };
                    let _ = writeln!(out, "    {destination} = {negation}({call});");
                    Ok(())
                }
                other => Err(internal(format!(
                    "invalid string binary operator {other:?}"
                ))),
            };
        }
        if *ty == Type::F16 {
            if !matches!(
                operator,
                l::BinaryOp::Eq
                    | l::BinaryOp::Ne
                    | l::BinaryOp::Lt
                    | l::BinaryOp::Le
                    | l::BinaryOp::Gt
                    | l::BinaryOp::Ge
            ) {
                return Err(internal(format!(
                    "invalid f16 binary operator {operator:?}"
                )));
            }
            let left = self.emitter.runtime_call(
                "double",
                "subscript_rt_f16_to_f64",
                &["uint16_t".into()],
                &[operands[0].clone()],
            );
            let right = self.emitter.runtime_call(
                "double",
                "subscript_rt_f16_to_f64",
                &["uint16_t".into()],
                &[operands[1].clone()],
            );
            let symbol = binary_symbol(operator)?;
            let _ = writeln!(out, "    {destination} = ({left}) {symbol} ({right});");
            return Ok(());
        }
        if operator == l::BinaryOp::Rem && ty.is_float() {
            let call = self.emitter.runtime_call(
                "double",
                "subscript_rt_fmod",
                &["void*".into(), "double".into(), "double".into()],
                &["ctx".into(), operands[0].clone(), operands[1].clone()],
            );
            let expression = if *ty == Type::F32 {
                format!("(float)({call})")
            } else {
                call
            };
            let _ = writeln!(out, "    {destination} = {expression};");
            return Ok(());
        }
        if matches!(operator, l::BinaryOp::Div | l::BinaryOp::Rem) && ty.is_integer() {
            let trap = self.take_pending_trap(&instruction.traps, l::TrapKind::DivisionByZero)?;
            let pos = self.emitter.pos_id(&trap.pos);
            let trap_call = self.emitter.runtime_call(
                "void",
                "subscript_rt_trap",
                &["void*".into(), "uint32_t".into(), "uint32_t".into()],
                &[
                    "ctx".into(),
                    format!("{}u", TrapKind::DivisionByZero as u32),
                    format!("{pos}u"),
                ],
            );
            let ctype = self.emitter.ctype(ty)?;
            // The divisor is bound to a local before the guard. The guard
            // makes a zero divisor unreachable, but a literal `x / 0` stays
            // a constant expression that MSVC rejects at translation
            // (`C2124`). The local also gives the divisor one evaluation
            // where the expressions below name it three times.
            let divisor = self.fresh();
            let _ = writeln!(out, "    {ctype} {divisor} = {};", operands[1]);
            let _ = writeln!(
                out,
                "    if (({divisor}) == 0) {{ {trap_call}; goto unwind; }}"
            );
            if is_unsigned(ty) {
                let symbol = if operator == l::BinaryOp::Div {
                    "/"
                } else {
                    "%"
                };
                let _ = writeln!(
                    out,
                    "    {destination} = ({ctype})(({}) {symbol} ({divisor}));",
                    operands[0]
                );
            } else if operator == l::BinaryOp::Div {
                let _ = writeln!(
                    out,
                    "    {destination} = (({divisor}) == ({ctype})-1) ? ({ctype})(0 - ({})({})) : ({ctype})(({}) / ({divisor}));",
                    unsigned_ctype(ty)?,
                    operands[0],
                    operands[0],
                );
            } else {
                let _ = writeln!(
                    out,
                    "    {destination} = (({divisor}) == ({ctype})-1) ? ({ctype})0 : ({ctype})(({}) % ({divisor}));",
                    operands[0]
                );
            }
            return Ok(());
        }
        let expression = match operator {
            l::BinaryOp::Shl | l::BinaryOp::Shr | l::BinaryOp::UShr => {
                shift_expression(operator, ty, &operands[0], &operands[1])?
            }
            l::BinaryOp::Add | l::BinaryOp::Sub | l::BinaryOp::Mul if ty.is_integer() => {
                let symbol = binary_symbol(operator)?;
                let carrier = unsigned_ctype(ty)?;
                let target = self.emitter.ctype(ty)?;
                format!(
                    "(({target})((({carrier})({})) {symbol} (({carrier})({}))))",
                    operands[0], operands[1]
                )
            }
            _ => format!(
                "(({}) {} ({}))",
                operands[0],
                binary_symbol(operator)?,
                operands[1]
            ),
        };
        let _ = writeln!(out, "    {destination} = {expression};");
        Ok(())
    }

    pub(super) fn emit_conversion(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operand: &str,
        operand_type: &l::ValueType,
    ) -> Result<(), String> {
        let result = instruction
            .result
            .ok_or_else(|| internal("conversion has no result"))?;
        let destination = self.value(result);
        let result_type = self.value_type(result)?.clone();
        for trap in &instruction.traps {
            match trap.kind {
                l::TrapKind::NullNarrowing => {
                    self.consume(trap);
                    self.emit_guard(out, &format!("({operand}) != NULL"), trap)?;
                }
                l::TrapKind::ClassMismatch(class) => {
                    self.consume(trap);
                    let offset = rtc::CLASS_ID_OFFSET;
                    self.emit_guard(
                        out,
                        &format!(
                            "(*(const uint32_t*)((const unsigned char*)({operand}) + {offset})) == {}u",
                            class.0
                        ),
                        trap,
                    )?;
                }
                l::TrapKind::DevOnlyLifetime => self.consume(trap),
                ref other => {
                    return Err(internal(format!(
                        "conversion carries unexpected trap {other:?}"
                    )))
                }
            }
        }
        if let (
            l::ValueType::Data(Type::Nullable(source)),
            l::ValueType::Data(Type::Class(target)),
        ) = (operand_type, &result_type)
        {
            if matches!(source.as_ref(), Type::Class(source) if source == target)
                && self.emitter.is_value_class(*target)?
            {
                let class = self.emitter.class_name(*target);
                let _ = writeln!(out, "    {destination} = *(({class}*)({operand}));");
                return Ok(());
            }
        }
        let source = data_type(operand_type)?;
        let target = data_type(&result_type)?;
        let expression = if source == target {
            operand.to_string()
        } else if *target == Type::F16 {
            let value = if *source == Type::F32 {
                format!("(double)({operand})")
            } else {
                operand.to_string()
            };
            self.emitter.runtime_call(
                "uint16_t",
                "subscript_rt_f16_from_f64",
                &["double".into()],
                &[value],
            )
        } else if *source == Type::F16 {
            let wide = self.emitter.runtime_call(
                "double",
                "subscript_rt_f16_to_f64",
                &["uint16_t".into()],
                &[operand.into()],
            );
            format!("({})({wide})", self.emitter.ctype(target)?)
        } else if source.is_float() && target.is_integer() {
            format!("{}({operand})", float_to_int_helper(target)?)
        } else {
            format!("(({})({operand}))", self.emitter.ctype(target)?)
        };
        let _ = writeln!(out, "    {destination} = {expression};");
        Ok(())
    }

    pub(super) fn emit_guard(
        &mut self,
        out: &mut String,
        condition: &str,
        trap: &l::Trap,
    ) -> Result<(), String> {
        let pos = self.emitter.pos_id(&trap.pos);
        let kind = runtime_trap_kind(&trap.kind)
            .ok_or_else(|| internal(format!("trap {:?} has no direct runtime kind", trap.kind)))?
            as u32;
        let call = self.emitter.runtime_call(
            "void",
            "subscript_rt_trap",
            &["void*".into(), "uint32_t".into(), "uint32_t".into()],
            &["ctx".into(), format!("{kind}u"), format!("{pos}u")],
        );
        let _ = writeln!(out, "    if (!({condition})) {{ {call}; goto unwind; }}");
        Ok(())
    }
}
