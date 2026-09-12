//! Literals, unary and binary operators, conversions, and allocation.

use super::*;

impl<'f, 'm, 'a, 'l, M: Module> Body<'f, 'm, 'a, 'l, M> {
    pub(super) fn string_literal(
        &mut self,
        text: &str,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let data = self.ml.literal_data(text.as_bytes())?;
        let global = self.ml.module.declare_data_in_func(data, self.builder.func);
        let pointer = self.builder.ins().symbol_value(types::I64, global);
        let length = self.iconst(types::I64, text.len() as i64);
        let position = traps
            .iter()
            .find(|trap| trap.kind == l::TrapKind::Allocation)
            .map_or(pos, |trap| &trap.pos);
        let position = self.position_id(position);
        let position = self.iconst(types::I32, position);
        let value = self
            .call_runtime(
                self.ml.rt.str_lit,
                &[self.ctx, pointer, length, position],
                false,
            )?
            .ok_or_else(|| internal("string literal has no result"))?;
        for trap in traps {
            if trap.kind == l::TrapKind::Allocation {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        Ok(RV::Scalar(value))
    }

    pub(super) fn zero(&mut self, ty: &Type) -> Result<RV, String> {
        Ok(match self.ml.layouts.repr(ty)? {
            Repr::None => RV::None,
            Repr::Scalar(ty) => RV::Scalar(self.zero_scalar(ty)),
            Repr::Pair => {
                let zero = self.iconst(types::I64, 0);
                RV::Pair(zero, zero)
            }
            Repr::Agg { size, align } => {
                let address = self.stack_slot(size, align);
                self.zero_bytes(address, size, align);
                RV::Aggregate(address)
            }
        })
    }

    pub(super) fn unary(&mut self, op: l::UnaryOp, operand: RV, ty: &Type) -> Result<RV, String> {
        let value = self.expect_scalar(operand)?;
        Ok(RV::Scalar(match op {
            l::UnaryOp::Not => self.builder.ins().bxor_imm(value, 1),
            l::UnaryOp::BitNot => self.builder.ins().bnot(value),
            l::UnaryOp::Neg if ty == &Type::F16 => {
                self.builder.ins().bxor_imm(value, i64::from(i16::MIN))
            }
            l::UnaryOp::Neg if ty.is_float() => self.builder.ins().fneg(value),
            l::UnaryOp::Neg => self.builder.ins().ineg(value),
        }))
    }

    pub(super) fn binary(
        &mut self,
        op: l::BinaryOp,
        left: RV,
        right: RV,
        operand_ty: &Type,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let left = self.expect_scalar(left)?;
        let right = self.expect_scalar(right)?;
        if operand_ty == &Type::Str {
            return match op {
                l::BinaryOp::Add => {
                    let position = self.position_id(pos);
                    let position = self.iconst(types::I32, position);
                    let result = self
                        .call_runtime(
                            self.ml.rt.str_concat,
                            &[self.ctx, left, right, position],
                            false,
                        )?
                        .ok_or_else(|| internal("string concatenation has no result"))?;
                    for trap in traps {
                        self.emit_trap(trap, TrapOperand::Pending)?;
                    }
                    Ok(RV::Scalar(result))
                }
                l::BinaryOp::Eq | l::BinaryOp::Ne => {
                    let result = self
                        .call_runtime(self.ml.rt.str_eq, &[self.ctx, left, right], false)?
                        .ok_or_else(|| internal("string equality has no result"))?;
                    let condition = self.builder.ins().icmp_imm(
                        if op == l::BinaryOp::Eq {
                            IntCC::NotEqual
                        } else {
                            IntCC::Equal
                        },
                        result,
                        0,
                    );
                    Ok(RV::Scalar(condition))
                }
                other => Err(internal(format!(
                    "invalid string binary operator {other:?}"
                ))),
            };
        }
        if operand_ty == &Type::F16 {
            let left = self
                .call_runtime(self.ml.rt.f16_to_f64, &[left], false)?
                .ok_or_else(|| internal("f16 left conversion has no result"))?;
            let right = self
                .call_runtime(self.ml.rt.f16_to_f64, &[right], false)?
                .ok_or_else(|| internal("f16 right conversion has no result"))?;
            let cc = match op {
                l::BinaryOp::Eq => FloatCC::Equal,
                l::BinaryOp::Ne => FloatCC::NotEqual,
                l::BinaryOp::Lt => FloatCC::LessThan,
                l::BinaryOp::Le => FloatCC::LessThanOrEqual,
                l::BinaryOp::Gt => FloatCC::GreaterThan,
                l::BinaryOp::Ge => FloatCC::GreaterThanOrEqual,
                other => return Err(internal(format!("invalid f16 operator {other:?}"))),
            };
            return Ok(RV::Scalar(self.builder.ins().fcmp(cc, left, right)));
        }
        let float = operand_ty.is_float();
        let unsigned = is_unsigned(operand_ty);
        let right = if matches!(op, l::BinaryOp::Shl | l::BinaryOp::Shr | l::BinaryOp::UShr) {
            self.builder.ins().band_imm(right, shift_mask(operand_ty)?)
        } else {
            right
        };
        let result = match op {
            l::BinaryOp::Add => {
                if float {
                    self.builder.ins().fadd(left, right)
                } else {
                    self.builder.ins().iadd(left, right)
                }
            }
            l::BinaryOp::Sub => {
                if float {
                    self.builder.ins().fsub(left, right)
                } else {
                    self.builder.ins().isub(left, right)
                }
            }
            l::BinaryOp::Mul => {
                if float {
                    self.builder.ins().fmul(left, right)
                } else {
                    self.builder.ins().imul(left, right)
                }
            }
            l::BinaryOp::Div | l::BinaryOp::Rem if float => {
                if op == l::BinaryOp::Div {
                    self.builder.ins().fdiv(left, right)
                } else {
                    let (left, right) = if operand_ty == &Type::F32 {
                        (
                            self.builder.ins().fpromote(types::F64, left),
                            self.builder.ins().fpromote(types::F64, right),
                        )
                    } else {
                        (left, right)
                    };
                    let remainder = self
                        .call_runtime(self.ml.rt.fmod, &[self.ctx, left, right], false)?
                        .ok_or_else(|| internal("floating remainder has no runtime result"))?;
                    if operand_ty == &Type::F32 {
                        self.builder.ins().fdemote(types::F32, remainder)
                    } else {
                        remainder
                    }
                }
            }
            l::BinaryOp::Div | l::BinaryOp::Rem => {
                for trap in traps {
                    if trap.kind == l::TrapKind::DivisionByZero {
                        self.emit_trap(trap, TrapOperand::Value(right))?;
                    }
                }
                if unsigned {
                    if op == l::BinaryOp::Div {
                        self.builder.ins().udiv(left, right)
                    } else {
                        self.builder.ins().urem(left, right)
                    }
                } else {
                    return self
                        .signed_division(op == l::BinaryOp::Div, left, right)
                        .map(RV::Scalar);
                }
            }
            l::BinaryOp::Eq
            | l::BinaryOp::Ne
            | l::BinaryOp::Lt
            | l::BinaryOp::Le
            | l::BinaryOp::Gt
            | l::BinaryOp::Ge => {
                if float {
                    let cc = match op {
                        l::BinaryOp::Eq => FloatCC::Equal,
                        l::BinaryOp::Ne => FloatCC::NotEqual,
                        l::BinaryOp::Lt => FloatCC::LessThan,
                        l::BinaryOp::Le => FloatCC::LessThanOrEqual,
                        l::BinaryOp::Gt => FloatCC::GreaterThan,
                        _ => FloatCC::GreaterThanOrEqual,
                    };
                    self.builder.ins().fcmp(cc, left, right)
                } else {
                    let cc = match (op, unsigned) {
                        (l::BinaryOp::Eq, _) => IntCC::Equal,
                        (l::BinaryOp::Ne, _) => IntCC::NotEqual,
                        (l::BinaryOp::Lt, false) => IntCC::SignedLessThan,
                        (l::BinaryOp::Le, false) => IntCC::SignedLessThanOrEqual,
                        (l::BinaryOp::Gt, false) => IntCC::SignedGreaterThan,
                        (l::BinaryOp::Ge, false) => IntCC::SignedGreaterThanOrEqual,
                        (l::BinaryOp::Lt, true) => IntCC::UnsignedLessThan,
                        (l::BinaryOp::Le, true) => IntCC::UnsignedLessThanOrEqual,
                        (l::BinaryOp::Gt, true) => IntCC::UnsignedGreaterThan,
                        _ => IntCC::UnsignedGreaterThanOrEqual,
                    };
                    self.builder.ins().icmp(cc, left, right)
                }
            }
            l::BinaryOp::BitAnd => self.builder.ins().band(left, right),
            l::BinaryOp::BitOr => self.builder.ins().bor(left, right),
            l::BinaryOp::BitXor => self.builder.ins().bxor(left, right),
            l::BinaryOp::Shl => self.builder.ins().ishl(left, right),
            l::BinaryOp::Shr if unsigned => self.builder.ins().ushr(left, right),
            l::BinaryOp::Shr => self.builder.ins().sshr(left, right),
            l::BinaryOp::UShr => self.builder.ins().ushr(left, right),
        };
        Ok(RV::Scalar(result))
    }

    fn signed_division(
        &mut self,
        division: bool,
        left: Value,
        right: Value,
    ) -> Result<Value, String> {
        let ty = self.builder.func.dfg.value_type(left);
        let minus_one = self.builder.ins().icmp_imm(IntCC::Equal, right, -1);
        let exceptional = self.builder.create_block();
        let ordinary = self.builder.create_block();
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, ty);
        self.builder
            .ins()
            .brif(minus_one, exceptional, &[], ordinary, &[]);
        self.builder.switch_to_block(exceptional);
        let value = if division {
            self.builder.ins().ineg(left)
        } else {
            self.zero_scalar(ty)
        };
        self.builder.ins().jump(merge, &[BlockArg::Value(value)]);
        self.builder.switch_to_block(ordinary);
        let value = if division {
            self.builder.ins().sdiv(left, right)
        } else {
            self.builder.ins().srem(left, right)
        };
        self.builder.ins().jump(merge, &[BlockArg::Value(value)]);
        self.builder.switch_to_block(merge);
        Ok(self.builder.block_params(merge)[0])
    }

    pub(super) fn convert(
        &mut self,
        value: RV,
        source: &Type,
        target: &Type,
        traps: &[l::Trap],
    ) -> Result<RV, String> {
        if source == target {
            return Ok(value);
        }
        if let RV::Aggregate(address) = value {
            if matches!(target, Type::Nullable(_)) {
                return Ok(RV::Scalar(address));
            }
            return Err(internal(format!(
                "cannot convert aggregate {source:?} to {target:?}"
            )));
        }
        let scalar = self.expect_scalar(value)?;
        for trap in traps {
            if matches!(
                trap.kind,
                l::TrapKind::NullNarrowing
                    | l::TrapKind::DevOnlyLifetime
                    | l::TrapKind::ClassMismatch(_)
            ) {
                self.emit_trap(trap, TrapOperand::Value(scalar))?;
            }
        }
        if matches!(self.ml.layouts.repr(target)?, Repr::Agg { .. }) {
            return Ok(RV::Aggregate(scalar));
        }
        if target == &Type::F16 {
            let wide = match source {
                Type::F32 => self.builder.ins().fpromote(types::F64, scalar),
                Type::F64 => scalar,
                other => return Err(internal(format!("cannot convert {other:?} to f16"))),
            };
            let result = self
                .call_runtime(self.ml.rt.f16_from_f64, &[wide], false)?
                .ok_or_else(|| internal("f16 conversion has no result"))?;
            return Ok(RV::Scalar(result));
        }
        if source == &Type::F16 {
            let wide = self
                .call_runtime(self.ml.rt.f16_to_f64, &[scalar], false)?
                .ok_or_else(|| internal("f16 widening has no result"))?;
            return Ok(RV::Scalar(match target {
                Type::F32 => self.builder.ins().fdemote(types::F32, wide),
                Type::F64 => wide,
                other => return Err(internal(format!("cannot convert f16 to {other:?}"))),
            }));
        }
        if (source.is_integer() || matches!(source, Type::Enum(_))) && target.is_integer() {
            let Repr::Scalar(source_repr) = self.ml.layouts.repr(source)? else {
                return Err(internal("integer source has a non-scalar representation"));
            };
            let Repr::Scalar(target_repr) = self.ml.layouts.repr(target)? else {
                return Err(internal("integer target has a non-scalar representation"));
            };
            let result = if source_repr == target_repr {
                scalar
            } else if source_repr.bits() < target_repr.bits() {
                if is_unsigned(source) {
                    self.builder.ins().uextend(target_repr, scalar)
                } else {
                    self.builder.ins().sextend(target_repr, scalar)
                }
            } else {
                self.builder.ins().ireduce(target_repr, scalar)
            };
            return Ok(RV::Scalar(result));
        }
        if source.is_integer() && matches!(target, Type::F32 | Type::F64) {
            let target_repr = if target == &Type::F32 {
                types::F32
            } else {
                types::F64
            };
            let Repr::Scalar(source_repr) = self.ml.layouts.repr(source)? else {
                return Err(internal("integer source has a non-scalar representation"));
            };
            let scalar = if source_repr.bits() < 32 {
                if is_unsigned(source) {
                    self.builder.ins().uextend(types::I32, scalar)
                } else {
                    self.builder.ins().sextend(types::I32, scalar)
                }
            } else {
                scalar
            };
            return Ok(RV::Scalar(if is_unsigned(source) {
                self.builder.ins().fcvt_from_uint(target_repr, scalar)
            } else {
                self.builder.ins().fcvt_from_sint(target_repr, scalar)
            }));
        }
        if matches!(source, Type::F32 | Type::F64) && target.is_integer() {
            let Repr::Scalar(target_repr) = self.ml.layouts.repr(target)? else {
                return Err(internal("integer target has a non-scalar representation"));
            };
            let result = if target_repr.bits() >= 32 {
                if is_unsigned(target) {
                    self.builder.ins().fcvt_to_uint_sat(target_repr, scalar)
                } else {
                    self.builder.ins().fcvt_to_sint_sat(target_repr, scalar)
                }
            } else if is_unsigned(target) {
                let wide = self.builder.ins().fcvt_to_uint_sat(types::I32, scalar);
                let maximum = self.iconst(types::I32, (1i64 << target_repr.bits()) - 1);
                let clamped = self.builder.ins().umin(wide, maximum);
                self.builder.ins().ireduce(target_repr, clamped)
            } else {
                let wide = self.builder.ins().fcvt_to_sint_sat(types::I32, scalar);
                let minimum = self.iconst(types::I32, -(1i64 << (target_repr.bits() - 1)));
                let maximum = self.iconst(types::I32, (1i64 << (target_repr.bits() - 1)) - 1);
                let low = self.builder.ins().smax(wide, minimum);
                let clamped = self.builder.ins().smin(low, maximum);
                self.builder.ins().ireduce(target_repr, clamped)
            };
            return Ok(RV::Scalar(result));
        }
        Ok(RV::Scalar(match (source, target) {
            (Type::F32, Type::F64) => self.builder.ins().fpromote(types::F64, scalar),
            (Type::F64, Type::F32) => self.builder.ins().fdemote(types::F32, scalar),
            _ => scalar,
        }))
    }

    pub(super) fn allocate_class(
        &mut self,
        class: ClassId,
        stable_address: Option<Value>,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let definition = self
            .ml
            .lir
            .classes
            .get(class.0)
            .ok_or_else(|| internal(format!("class {} is missing", class.0)))?;
        let (size, align) = {
            let layout = self.ml.layouts.class(class.0)?;
            (layout.size, layout.align)
        };
        if definition.is_value {
            let address = stable_address.unwrap_or_else(|| self.stack_slot(size, align));
            self.zero_bytes(address, size, align);
            return Ok(RV::Aggregate(address));
        }
        let size = self.iconst(types::I64, i64::from(size));
        let class_id = self.iconst(types::I32, class.0 as i64);
        let position = traps
            .iter()
            .find(|trap| trap.kind == l::TrapKind::Allocation)
            .map_or(pos, |trap| &trap.pos);
        let position = self.position_id(position);
        let position = self.iconst(types::I32, position);
        let pointer = self
            .call_runtime(
                self.ml.rt.alloc,
                &[self.ctx, size, class_id, position],
                false,
            )?
            .ok_or_else(|| internal("class allocation has no result"))?;
        for trap in traps {
            if trap.kind == l::TrapKind::Allocation {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        Ok(RV::Scalar(pointer))
    }

    pub(super) fn box_boundary_value(
        &mut self,
        value: RV,
        ty: &l::ValueType,
        payload: ClassId,
        traps: &[l::Trap],
        pos: &Pos,
    ) -> Result<RV, String> {
        let l::ValueType::Data(Type::Class(class)) = ty else {
            return Err(internal("BoxBoundaryValue operand is not a class value"));
        };
        if *class != payload {
            return Err(internal(
                "BoxBoundaryValue payload disagrees with its operand",
            ));
        }
        let layout = self.ml.layouts.class(payload.0)?;
        if !layout.is_value {
            return Err(internal("BoxBoundaryValue operand is not a value class"));
        }
        let size = self.iconst(types::I64, i64::from(layout.size));
        let class_id = self.iconst(types::I32, payload.0 as i64);
        let position = traps
            .iter()
            .find(|trap| trap.kind == l::TrapKind::Allocation)
            .map_or(pos, |trap| &trap.pos);
        let position = self.position_id(position);
        let position = self.iconst(types::I32, position);
        let pointer = self
            .call_runtime(
                self.ml.rt.alloc,
                &[self.ctx, size, class_id, position],
                false,
            )?
            .ok_or_else(|| internal("boundary value box allocation has no result"))?;
        for trap in traps {
            if trap.kind == l::TrapKind::Allocation {
                self.emit_trap(trap, TrapOperand::Pending)?;
            }
        }
        self.store_data(&Type::Class(payload), pointer, 0, value)?;
        Ok(RV::Scalar(pointer))
    }
}
