//! Lowering for expressions: operators, short-circuit forms, conditionals, and assignment.

use super::*;

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    pub(super) fn lower_expr(
        &mut self,
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        use hir::ExprKind as K;
        let result = match &expr.kind {
            K::Int(value) => Some(l::Operand::Constant(l::Constant {
                ty: expr.ty.clone(),
                kind: l::ConstantKind::Integer(*value),
            })),
            K::Float(value) => Some(l::Operand::Constant(l::Constant {
                ty: expr.ty.clone(),
                kind: l::ConstantKind::FloatBits(if expr.ty == Type::F32 {
                    u64::from((*value as f32).to_bits())
                } else {
                    value.to_bits()
                }),
            })),
            K::Bool(value) => Some(l::Operand::Constant(l::Constant {
                ty: Type::Bool,
                kind: l::ConstantKind::Boolean(*value),
            })),
            K::Null => Some(l::Operand::Constant(l::Constant {
                ty: expr.ty.clone(),
                kind: l::ConstantKind::Null,
            })),
            K::Str(value) => self.emit(
                l::InstructionKind::StringLiteral(value.clone()),
                Vec::new(),
                Some(l::ValueType::Data(expr.ty.clone())),
                false,
                convert_traps(&expr.trap_sites(self.lowering.hir)),
                expr.pos.clone(),
            )?,
            K::This => Some(
                self.this_value
                    .clone()
                    .ok_or_else(|| self.error(&expr.pos, "`this` has no receiver parameter"))?,
            ),
            K::Local(name) => {
                if let Some(value) = self.lookup_substitution(name) {
                    Some(self.coerce_operand(
                        value,
                        l::ValueType::Data(expr.ty.clone()),
                        &expr.pos,
                    )?)
                } else {
                    let binding = self.lookup_binding(name, &expr.pos)?;
                    let value = self.read_binding(binding, &expr.pos)?;
                    Some(self.coerce_operand(
                        value,
                        l::ValueType::Data(expr.ty.clone()),
                        &expr.pos,
                    )?)
                }
            }
            K::Global(name) => {
                let global = self
                    .lowering
                    .globals
                    .get(name)
                    .copied()
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown global `{name}`")))?;
                let stored_type = self
                    .lowering
                    .hir
                    .globals
                    .get(global.0 as usize)
                    .map(|global| global.ty.clone())
                    .ok_or_else(|| self.error(&expr.pos, "global declaration is missing"))?;
                let value = self
                    .emit(
                        l::InstructionKind::LoadGlobal(global),
                        Vec::new(),
                        Some(l::ValueType::Data(stored_type)),
                        false,
                        Vec::new(),
                        expr.pos.clone(),
                    )?
                    .expect("global load");
                Some(self.coerce_operand(value, l::ValueType::Data(expr.ty.clone()), &expr.pos)?)
            }
            K::FuncRef(name) => {
                let function = self
                    .lowering
                    .free_functions
                    .get(name)
                    .map(|record| record.id)
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown function `{name}`")))?;
                self.emit(
                    l::InstructionKind::FunctionRef(function),
                    Vec::new(),
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    Vec::new(),
                    expr.pos.clone(),
                )?
            }
            K::EnumMember { value, .. } => Some(l::Operand::Constant(l::Constant {
                ty: expr.ty.clone(),
                kind: l::ConstantKind::Integer(*value),
            })),
            K::Unary { op, operand } => {
                let operand = self.require_expr(operand)?;
                self.emit(
                    l::InstructionKind::Unary(convert_unary(*op)),
                    vec![operand],
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::Binary {
                op: hir::BinOp::And,
                left,
                right,
            } => Some(self.lower_short_circuit(left, right, false, expr)?),
            K::Binary {
                op: hir::BinOp::Or,
                left,
                right,
            } => Some(self.lower_short_circuit(left, right, true, expr)?),
            K::Binary { op, left, right } => {
                let left = self.require_expr(left)?;
                let right = self.require_expr(right)?;
                self.emit(
                    l::InstructionKind::Binary(convert_binary(*op)?),
                    vec![left, right],
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::AbsenceTest { value, negated } => {
                let Type::StringAlias(alias) = value.ty else {
                    return Err(self.error(&value.pos, "absence test value is not a string alias"));
                };
                let discriminant = self
                    .lowering
                    .hir
                    .string_aliases
                    .get(alias.0)
                    .map(hir::StringAliasDef::absence_discriminant)
                    .ok_or_else(|| self.error(&value.pos, "absence alias is missing"))?;
                let value = self.require_expr(value)?;
                let absent = l::Operand::Constant(l::Constant {
                    ty: Type::StringAlias(alias),
                    kind: l::ConstantKind::Integer(discriminant),
                });
                self.emit(
                    l::InstructionKind::Binary(if *negated {
                        l::BinaryOp::Ne
                    } else {
                        l::BinaryOp::Eq
                    }),
                    vec![value, absent],
                    Some(l::ValueType::Data(Type::Bool)),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::Assign { op, target, value } => {
                Some(self.lower_assignment(*op, target, value, expr)?)
            }
            K::Cast(value) => {
                let value = self.require_expr(value)?;
                self.emit(
                    l::InstructionKind::Cast,
                    vec![value],
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::Call { callee, args } => self.lower_call(callee, args, expr)?,
            K::New { class, args } => Some(self.lower_new(*class, args, expr)?),
            K::DescriptorLit { class, fields } => {
                Some(self.lower_descriptor(*class, fields, expr)?)
            }
            K::Zero => self.emit(
                l::InstructionKind::Zero,
                Vec::new(),
                Some(l::ValueType::Data(expr.ty.clone())),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?,
            K::RawNew { class } => self.emit(
                l::InstructionKind::AllocateClass(*class),
                Vec::new(),
                Some(self.allocated_type(*class, &expr.pos)?),
                false,
                convert_traps(&expr.trap_sites(self.lowering.hir)),
                expr.pos.clone(),
            )?,
            K::Field { obj, name } => {
                let object = self.require_expr(obj)?;
                let field = self.resolve_field(&obj.ty, name, &expr.pos)?;
                let stored_type = self.resolved_field_type(field, &obj.ty, &expr.pos)?;
                let value = self
                    .emit(
                        l::InstructionKind::LoadField(field),
                        vec![object],
                        Some(l::ValueType::Data(stored_type)),
                        false,
                        convert_traps(&expr.trap_sites(self.lowering.hir)),
                        expr.pos.clone(),
                    )?
                    .expect("field load");
                Some(self.coerce_operand(value, l::ValueType::Data(expr.ty.clone()), &expr.pos)?)
            }
            K::JsonResultValue(obj) => {
                let object = self.require_expr(obj)?;
                let field = self.resolve_field(&obj.ty, "value", &expr.pos)?;
                let ok_field = match self.resolve_field(&obj.ty, "ok", &expr.pos)? {
                    l::FieldRef::Class(field) => field,
                    _ => {
                        return Err(
                            self.error(&expr.pos, "JSON result ok field is not a class field")
                        );
                    }
                };
                let stored_type = self.resolved_field_type(field, &obj.ty, &expr.pos)?;
                let traps = convert_traps(&expr.trap_sites(self.lowering.hir))
                    .into_iter()
                    .map(|mut trap| {
                        if matches!(trap.kind, l::TrapKind::JsonResultValue(_)) {
                            trap.kind = l::TrapKind::JsonResultValue(ok_field);
                        }
                        trap
                    })
                    .collect();
                let value = self
                    .emit(
                        l::InstructionKind::LoadField(field),
                        vec![object],
                        Some(l::ValueType::Data(stored_type)),
                        false,
                        traps,
                        expr.pos.clone(),
                    )?
                    .expect("JSON result field load");
                Some(self.coerce_operand(value, l::ValueType::Data(expr.ty.clone()), &expr.pos)?)
            }
            K::Length(value) => {
                let value = self.require_expr(value)?;
                self.emit(
                    l::InstructionKind::Length,
                    vec![value],
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::Index { .. } => {
                let place = self.prepare_place(expr)?;
                let value = self.load_place(&place, &expr.pos)?;
                Some(self.coerce_operand(value, l::ValueType::Data(expr.ty.clone()), &expr.pos)?)
            }
            K::ArrayLit(elements) => {
                let element_type = match &expr.ty {
                    Type::Array(element) | Type::FixedArray(element, _) => (**element).clone(),
                    _ => {
                        return Err(
                            self.error(&expr.pos, "array literal result is not array-typed")
                        );
                    }
                };
                let operands = elements
                    .iter()
                    .map(|element| self.lower_stored_expr(&element_type, element))
                    .collect::<Result<Vec<_>, _>>()?;
                let stored = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| StoredOperand {
                        index,
                        ty: l::ValueType::Data(element_type.clone()),
                        action: OwnerStoreAction::Acquire(hir::AsyncCopySite::ArrayElement),
                        pos: element.pos.clone(),
                    })
                    .collect();
                self.emit_store_instruction(
                    l::InstructionKind::ArrayLiteral,
                    operands,
                    stored,
                    (Some(l::ValueType::Data(expr.ty.clone())), false),
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::ArraySpreadLit(elements) => {
                let Type::Array(element_type) = &expr.ty else {
                    return Err(
                        self.error(&expr.pos, "spread literal result is not a dynamic array")
                    );
                };
                let operands = elements
                    .iter()
                    .map(|element| {
                        if element.spread.is_none() {
                            self.lower_stored_expr(element_type, &element.expr)
                        } else {
                            self.require_expr(&element.expr)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let spreads = elements
                    .iter()
                    .map(|element| element.spread.map(convert_spread))
                    .collect();
                let stored = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| StoredOperand {
                        index,
                        ty: l::ValueType::Data(if element.spread.is_none() {
                            (**element_type).clone()
                        } else {
                            element.expr.ty.clone()
                        }),
                        action: OwnerStoreAction::Acquire(hir::AsyncCopySite::SpreadElement),
                        pos: element.expr.pos.clone(),
                    })
                    .collect();
                self.emit_store_instruction(
                    l::InstructionKind::ArraySpreadLiteral(spreads),
                    operands,
                    stored,
                    (Some(l::ValueType::Data(expr.ty.clone())), false),
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::Template(parts) => {
                let mut operands = Vec::new();
                let mut lowered_parts = Vec::new();
                for part in parts {
                    match part {
                        hir::TplPart::Text(text) => {
                            lowered_parts.push(l::TemplatePart::Text(text.clone()));
                        }
                        hir::TplPart::Expr(value) => {
                            let index = operands.len() as u32;
                            operands.push(self.require_expr(value)?);
                            let format = match value.ty {
                                Type::I8 | Type::I16 | Type::I32 | Type::Enum(_) => {
                                    l::FormatKind::I32
                                }
                                Type::U8 | Type::U16 | Type::U32 => l::FormatKind::U32,
                                Type::I64 | Type::Date => l::FormatKind::I64,
                                Type::U64 => l::FormatKind::U64,
                                Type::F16 => l::FormatKind::F16,
                                Type::F32 => l::FormatKind::F32,
                                Type::F64 => l::FormatKind::F64,
                                Type::Bool => l::FormatKind::Bool,
                                Type::Str => l::FormatKind::Str,
                                Type::StringAlias(alias) => l::FormatKind::StringAlias(alias),
                                ref other => {
                                    return Err(self.error(
                                        &value.pos,
                                        format!("template operand {other:?} is not formattable"),
                                    ))
                                }
                            };
                            lowered_parts.push(l::TemplatePart::Operand { index, format });
                        }
                        other => {
                            return Err(self.error(
                                &expr.pos,
                                format!("unrecognized template part: {other:?}"),
                            ));
                        }
                    }
                }
                let traps = if lowered_parts.is_empty() {
                    Vec::new()
                } else {
                    convert_traps(&expr.trap_sites(self.lowering.hir))
                };
                self.emit(
                    l::InstructionKind::Template(lowered_parts),
                    operands,
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    traps,
                    expr.pos.clone(),
                )?
            }
            K::Lambda {
                params,
                ret,
                body,
                captures,
            } => Some(self.lower_lambda(params, ret, body, captures, expr)?),
            K::Yield(value) => {
                let value = value
                    .as_deref()
                    .map(|value| self.require_expr(value))
                    .transpose()?
                    .map(|value| self.terminator_value(value, &expr.pos))
                    .transpose()?;
                let successor = self.new_block(Vec::new(), Some("yield.resume".to_string()));
                self.terminate(
                    l::Terminator::Suspend {
                        kind: l::SuspendKind::Yield(value),
                        pos: expr.pos.clone(),
                        successor,
                        resume_value: None,
                        arguments: Vec::new(),
                        invalidates: Vec::new(),
                        traps: Vec::new(),
                    },
                    &expr.pos,
                )?;
                self.current = Some(successor);
                None
            }
            K::AsyncSuspend => {
                let successor = self.new_block(Vec::new(), Some("async.resume".to_string()));
                self.terminate(
                    l::Terminator::Suspend {
                        kind: l::SuspendKind::Async,
                        pos: expr.pos.clone(),
                        successor,
                        resume_value: None,
                        arguments: Vec::new(),
                        invalidates: self.array_values.clone(),
                        traps: Vec::new(),
                    },
                    &expr.pos,
                )?;
                self.current = Some(successor);
                None
            }
            K::AsyncCall { callee, args } => self.lower_async_call(callee, args, expr)?,
            K::AsyncHandleCreate { callee, args, .. } => {
                Some(self.lower_async_handle_create(callee, args, expr)?)
            }
            K::AsyncHandleAwait(handle) => self.lower_async_handle_await(handle, expr)?,
            K::AsyncHandleTransfer { value, .. } => self.lower_expr(value)?,
            K::Cond { cond, then, els } => Some(self.lower_cond(cond, then, els, expr)?),
        };
        Ok(result)
    }

    fn lower_short_circuit(
        &mut self,
        left: &hir::Expr,
        right: &hir::Expr,
        short_value: bool,
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let left = self.require_expr(left)?;
        let branch_state = self.binding_snapshot();
        let right_block = self.new_block(Vec::new(), Some("logic.rhs".to_string()));
        let merge = self.new_state_block(
            vec![l::ValueType::Data(expr.ty.clone())],
            Some("logic.merge".to_string()),
            &[],
        );
        let short = l::Operand::Constant(l::Constant {
            ty: Type::Bool,
            kind: l::ConstantKind::Boolean(short_value),
        });
        let short_target = self.block_target(merge, vec![short])?;
        let (then_target, else_target) = if short_value {
            (short_target, target(right_block, Vec::new()))
        } else {
            (target(right_block, Vec::new()), short_target)
        };
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition: left,
                then_target,
                else_target,
            },
            &expr.pos,
        )?;
        self.current = Some(right_block);
        self.restore_bindings(&branch_state);
        let right = self.require_expr(right)?;
        let edge = self.block_target(merge, vec![right])?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;
        self.enter_block(merge)?;
        Ok(l::Operand::Value(
            self.blocks[merge.0 as usize].parameters[0],
        ))
    }

    fn lower_cond(
        &mut self,
        cond: &hir::Expr,
        then: &hir::Expr,
        els: &hir::Expr,
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let condition = self.require_expr(cond)?;
        let branch_state = self.binding_snapshot();
        let then_block = self.new_block(Vec::new(), Some("cond.then".to_string()));
        let else_block = self.new_block(Vec::new(), Some("cond.else".to_string()));
        let merge = self.new_state_block(
            vec![l::ValueType::Data(expr.ty.clone())],
            Some("cond.merge".to_string()),
            &[],
        );
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition,
                then_target: target(then_block, Vec::new()),
                else_target: target(else_block, Vec::new()),
            },
            &expr.pos,
        )?;
        self.current = Some(then_block);
        let then_value = self.lower_stored_expr(&expr.ty, then)?;
        let result_type = l::ValueType::Data(expr.ty.clone());
        let then_is_fresh = matches!(&then_value, l::Operand::Value(value)
            if self.values.get(value.0 as usize).is_some_and(|value| value.fresh_owner));
        let edge = self.block_target(merge, vec![then_value])?;
        self.terminate(l::Terminator::Branch(edge), &then.pos)?;
        self.restore_bindings(&branch_state);
        self.current = Some(else_block);
        let else_value = self.lower_stored_expr(&expr.ty, els)?;
        let else_is_fresh = matches!(&else_value, l::Operand::Value(value)
            if self.values.get(value.0 as usize).is_some_and(|value| value.fresh_owner));
        let edge = self.block_target(merge, vec![else_value])?;
        self.terminate(l::Terminator::Branch(edge), &els.pos)?;
        self.enter_block(merge)?;
        let result = self.blocks[merge.0 as usize].parameters[0];
        if is_async_owner_type(&result_type) && then_is_fresh && else_is_fresh {
            let transfers_fresh_owner = match hir::AsyncCopySite::ConditionalResult {
                hir::AsyncCopySite::ConditionalResult => true,
                hir::AsyncCopySite::Binding
                | hir::AsyncCopySite::Assignment
                | hir::AsyncCopySite::ArrayElement
                | hir::AsyncCopySite::SpreadElement
                | hir::AsyncCopySite::CallArgument
                | hir::AsyncCopySite::Return
                | hir::AsyncCopySite::ForOfBinding
                | hir::AsyncCopySite::DiscardedResult => false,
            };
            if transfers_fresh_owner {
                self.values[result.0 as usize].fresh_owner = true;
            }
        }
        Ok(l::Operand::Value(result))
    }

    fn lower_assignment(
        &mut self,
        op: Option<hir::BinOp>,
        target_expr: &hir::Expr,
        value_expr: &hir::Expr,
        whole: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let traps = convert_traps(&whole.trap_sites(self.lowering.hir));
        let binary_traps = || {
            traps
                .iter()
                .filter(|trap| {
                    matches!(
                        trap.kind,
                        l::TrapKind::Allocation | l::TrapKind::DivisionByZero
                    )
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        if let hir::ExprKind::Local(name) = &target_expr.kind {
            let binding = self.lookup_binding(name, &target_expr.pos)?;
            let old = if op.is_some() {
                Some(self.read_binding(binding, &target_expr.pos)?)
            } else {
                None
            };
            let result = if let Some(op) = op {
                let value = self.require_expr(value_expr)?;
                self.emit(
                    l::InstructionKind::Binary(convert_binary(op)?),
                    vec![old.expect("compound old value"), value],
                    Some(l::ValueType::Data(target_expr.ty.clone())),
                    false,
                    binary_traps(),
                    target_expr.pos.clone(),
                )?
                .expect("compound result")
            } else {
                self.lower_stored_expr_at(&target_expr.ty, value_expr, &target_expr.pos)?
            };
            self.write_binding(binding, result.clone(), &target_expr.pos, Vec::new())?;
            return Ok(result);
        }
        let mut place = self.prepare_place(target_expr)?;
        let direct_index = matches!(place.kind, PreparedPlaceKind::Index { .. });
        let old = if op.is_some() {
            let old = self.load_place(&place, &target_expr.pos)?;
            if direct_index {
                prepare_direct_index_store(&mut place, &traps);
            } else {
                prepare_place_after_checked_read(&mut place);
            }
            Some(old)
        } else {
            if direct_index {
                prepare_direct_index_assignment(&mut place, &traps);
            }
            None
        };
        let result = if let Some(op) = op {
            let value = self.require_expr(value_expr)?;
            self.emit(
                l::InstructionKind::Binary(convert_binary(op)?),
                vec![old.expect("compound old value"), value],
                Some(l::ValueType::Data(target_expr.ty.clone())),
                false,
                binary_traps(),
                target_expr.pos.clone(),
            )?
            .expect("compound result")
        } else {
            self.lower_stored_expr_at(&target_expr.ty, value_expr, &target_expr.pos)?
        };
        self.store_place(&place, result.clone(), &target_expr.pos)?;
        Ok(result)
    }
}
