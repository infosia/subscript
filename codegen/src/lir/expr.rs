//! Lowering for expressions: operators, short-circuit forms, conditionals, and assignment.

use super::*;

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    pub(super) fn coerce_shared_read(
        &mut self,
        value: l::Operand,
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let sites = expr
            .trap_sites_for_reload(self.lowering.hir, self.lowering.reload)
            .into_iter()
            .filter(|site| matches!(site, hir::TrapSite::NullNarrowing { .. }))
            .collect::<Vec<_>>();
        self.coerce_read(
            value,
            &expr.ty,
            narrow_origin(expr, &self.lowering.hir.classes),
            convert_traps(&sites),
            &expr.pos,
        )
    }

    pub(super) fn coerce_read(
        &mut self,
        value: l::Operand,
        ty: &Type,
        origin: l::NarrowOrigin,
        mut traps: Vec<l::Trap>,
        pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        let actual = self.operand_type(&value, pos)?;
        let result = if matches!(&actual, l::ValueType::Data(stored)
            if self.is_boundary_box_narrowing(stored, ty))
        {
            l::ValueType::Address(l::AddressType {
                pointee: ty.clone(),
                array_base: None,
            })
        } else {
            l::ValueType::Data(ty.clone())
        };
        if !matches!(&actual, l::ValueType::Data(Type::Nullable(inner)) if inner.as_ref() == ty) {
            return self.coerce_operand(value, result, pos);
        }
        for trap in &mut traps {
            trap.kind = l::TrapKind::SharedNullNarrowing;
        }
        self.emit(
            l::InstructionKind::NarrowNonNull(origin),
            vec![value],
            Some(result),
            false,
            traps,
            pos.clone(),
        )?
        .ok_or_else(|| self.error(pos, "shared read conversion produced no value"))
    }

    pub(super) fn lower_expr(
        &mut self,
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        self.scopes.push(HashMap::new());
        let result = self.lower_expr_with_holds(expr)?;
        self.finish_input_holds(result, &expr.ty, &expr.pos, true)
    }

    fn lower_expr_with_holds(
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
                convert_traps(&expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload)),
                expr.pos.clone(),
            )?,
            K::This => Some(
                self.this_value
                    .clone()
                    .ok_or_else(|| self.error(&expr.pos, "`this` has no receiver parameter"))?,
            ),
            K::Local(name, _, _) => {
                if let Some(value) = self.lookup_substitution(name) {
                    Some(self.coerce_read(
                        value,
                        &expr.ty,
                        narrow_origin(expr, &self.lowering.hir.classes),
                        Vec::new(),
                        &expr.pos,
                    )?)
                } else {
                    let binding = self.lookup_binding(name, &expr.pos)?;
                    let value = self.read_binding(binding, &expr.pos)?;
                    Some(self.coerce_read(
                        value,
                        &expr.ty,
                        narrow_origin(expr, &self.lowering.hir.classes),
                        Vec::new(),
                        &expr.pos,
                    )?)
                }
            }
            K::Global(name) => {
                let global = self.lowering.globals.get(name).copied().ok_or_else(|| {
                    self.error(
                        &expr.pos,
                        format!("unknown global `{}`", name.source_name()),
                    )
                })?;
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
                let value = self.coerce_shared_read(value, expr)?;
                Some(self.coerce_operand(value, l::ValueType::Data(expr.ty.clone()), &expr.pos)?)
            }
            K::FuncRef(name) => {
                let function = self
                    .lowering
                    .free_functions
                    .get(name)
                    .map(|record| record.id)
                    .ok_or_else(|| {
                        self.error(
                            &expr.pos,
                            format!("unknown function `{}`", name.source_name()),
                        )
                    })?;
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
                    convert_traps(
                        &expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload),
                    ),
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
                    convert_traps(
                        &expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload),
                    ),
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
                    convert_traps(
                        &expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload),
                    ),
                    expr.pos.clone(),
                )?
            }
            K::Assign {
                op,
                target,
                value,
                update,
            } => Some(self.lower_assignment(*op, *update, target, value, expr, true)?),
            K::Cast(value) => {
                let value = self.require_expr(value)?;
                let kind = if matches!(self.operand_type(&value, &expr.pos)?,
                    l::ValueType::Data(Type::Nullable(ref inner)) if inner.as_ref() == &expr.ty)
                {
                    l::InstructionKind::NarrowNonNull(narrow_origin(
                        expr,
                        &self.lowering.hir.classes,
                    ))
                } else {
                    l::InstructionKind::Cast
                };
                self.emit(
                    kind,
                    vec![value],
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(
                        &expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload),
                    ),
                    expr.pos.clone(),
                )?
            }
            K::Call { callee, args } => self.lower_call(callee, args, expr, true)?,
            K::New { class, args } => Some(self.lower_new(*class, args, expr)?),
            K::DescriptorLit { class, fields } => {
                Some(self.lower_descriptor(*class, fields, expr)?)
            }
            K::Zero | K::Unassigned => self.emit(
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
                convert_traps(&expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload)),
                expr.pos.clone(),
            )?,
            K::Field { .. } | K::Length(_) | K::Index { .. } => self.lower_read_expr(expr, true)?,
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
                    .enumerate()
                    .map(|(index, element)| {
                        let value = self.lower_stored_expr(&element_type, element)?;
                        if self.input_needs_hold(element)
                            && elements[index + 1..]
                                .iter()
                                .any(array_ownership::runs_user_code)
                        {
                            self.hold_input(&value, &expr.pos)?;
                        }
                        Ok(value)
                    })
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
                    convert_traps(
                        &expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload),
                    ),
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
                    .enumerate()
                    .map(|(index, element)| {
                        let value = if element.spread.is_none() {
                            self.lower_stored_expr(element_type, &element.expr)?
                        } else {
                            self.require_expr(&element.expr)?
                        };
                        if self.input_needs_hold(&element.expr)
                            && elements[index + 1..]
                                .iter()
                                .any(|later| array_ownership::runs_user_code(&later.expr))
                        {
                            self.hold_input(&value, &expr.pos)?;
                        }
                        Ok(value)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let spreads = elements
                    .iter()
                    .map(|element| element.spread.map(convert_spread))
                    .collect();
                let stored = elements
                    .iter()
                    .enumerate()
                    .filter(|(_, element)| element.spread != Some(hir::SpreadKind::Array))
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
                let temporary_sources = elements.iter().zip(&operands).filter_map(|(element, operand)| {
                    if element.spread == Some(hir::SpreadKind::Array)
                        && matches!(operand, l::Operand::Value(value)
                            if self.values.get(value.0 as usize).is_some_and(|definition| definition.fresh_owner)
                                && !self.moved_async_owners.contains(value)) {
                        Some((operand.clone(), element.expr.ty.clone(), element.expr.pos.clone()))
                    } else { None }
                }).collect::<Vec<_>>();
                let result = self.emit_store_instruction(
                    l::InstructionKind::ArraySpreadLiteral(spreads),
                    operands,
                    stored,
                    (Some(l::ValueType::Data(expr.ty.clone())), false),
                    convert_traps(
                        &expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload),
                    ),
                    expr.pos.clone(),
                )?;
                for (value, ty, pos) in temporary_sources {
                    self.release_owner(value.clone(), &l::ValueType::Data(ty), &pos)?;
                    if let l::Operand::Value(value) = value {
                        self.moved_async_owners.insert(value);
                    }
                }
                result
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
                    convert_traps(
                        &expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload),
                    )
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
                ..
            } => Some(self.lower_lambda(params, ret, body, captures, expr)?),
            K::Yield(value) => {
                let value = value
                    .as_deref()
                    .map(|value| {
                        let operand = self.require_expr(value)?;
                        self.acquire_owner(
                            hir::AsyncCopySite::Return,
                            &operand,
                            &l::ValueType::Data(value.ty.clone()),
                            &expr.pos,
                        )?;
                        Ok(operand)
                    })
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
            K::TaskGroup {
                operation, args, ..
            } => {
                let mut operands = Vec::new();
                for (index, arg) in args.iter().enumerate() {
                    let value = self.require_expr(arg)?;
                    if *operation == hir::TaskGroupOperation::Add && index == 1 {
                        self.acquire_owner(
                            hir::AsyncCopySite::CallArgument,
                            &value,
                            &l::ValueType::Data(arg.ty.clone()),
                            &arg.pos,
                        )?;
                    }
                    operands.push(value);
                }
                self.emit(
                    l::InstructionKind::TaskGroup(*operation),
                    operands,
                    (expr.ty != Type::Void).then(|| l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(&expr.trap_sites(self.lowering.hir)),
                    expr.pos.clone(),
                )?
            }
            K::AsyncAll { jobs, .. } => {
                let input = self.require_expr(jobs)?;
                let fresh = matches!(&input, l::Operand::Value(value)
                    if self.values.get(value.0 as usize).is_some_and(|value| value.fresh_owner)
                        && !self.moved_async_owners.contains(value));
                let result = self.emit(
                    l::InstructionKind::AsyncAll,
                    vec![input.clone()],
                    Some(l::ValueType::Data(expr.ty.clone())),
                    false,
                    convert_traps(
                        &expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload),
                    ),
                    expr.pos.clone(),
                )?;
                if fresh {
                    self.discard_owner(
                        hir::AsyncCopySite::DiscardedResult,
                        input,
                        &l::ValueType::Data(jobs.ty.clone()),
                        &jobs.pos,
                    )?;
                }
                result
            }
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
        let then_value = self.own_conditional_branch(then_value, &expr.ty, &then.pos)?;
        let result_type = l::ValueType::Data(expr.ty.clone());
        let then_is_fresh = matches!(&then_value, l::Operand::Value(value)
            if self.values.get(value.0 as usize).is_some_and(|value| value.fresh_owner));
        let edge = self.block_target(merge, vec![then_value])?;
        self.terminate(l::Terminator::Branch(edge), &then.pos)?;
        self.restore_bindings(&branch_state);
        self.current = Some(else_block);
        let else_value = self.lower_stored_expr(&expr.ty, els)?;
        let else_value = self.own_conditional_branch(else_value, &expr.ty, &els.pos)?;
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

    pub(super) fn own_conditional_branch(
        &mut self,
        value: l::Operand,
        ty: &Type,
        pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        if ty.counted_type().is_none()
            || matches!(&value, l::Operand::Value(id)
            if self.values.get(id.0 as usize).is_some_and(|value| value.fresh_owner)
                && !self.moved_async_owners.contains(id))
        {
            return Ok(value);
        }
        let kind = if matches!(ty, Type::AsyncHandle(_)) {
            l::InstructionKind::AsyncHandleRetain
        } else {
            l::InstructionKind::AsyncHandleArrayRetain
        };
        let ty = l::ValueType::Data(ty.clone());
        self.emit(
            kind,
            vec![value],
            Some(ty.clone()),
            false,
            self.read_lifetime(&ty, pos),
            pos.clone(),
        )?
        .ok_or_else(|| self.error(pos, "conditional owner copy has no result"))
    }

    pub(super) fn lower_assignment(
        &mut self,
        op: Option<hir::BinOp>,
        update: Option<hir::UpdateKind>,
        target_expr: &hir::Expr,
        value_expr: &hir::Expr,
        whole: &hir::Expr,
        result_used: bool,
    ) -> Result<l::Operand, LowerError> {
        self.scopes.push(HashMap::new());
        let result = self.lower_assignment_with_holds(
            op,
            update,
            target_expr,
            value_expr,
            whole,
            result_used,
        )?;
        self.finish_input_holds(Some(result), &whole.ty, &whole.pos, result_used)?
            .ok_or_else(|| self.error(&whole.pos, "assignment has no result"))
    }

    fn lower_assignment_with_holds(
        &mut self,
        op: Option<hir::BinOp>,
        update: Option<hir::UpdateKind>,
        target_expr: &hir::Expr,
        value_expr: &hir::Expr,
        whole: &hir::Expr,
        result_used: bool,
    ) -> Result<l::Operand, LowerError> {
        let traps =
            convert_traps(&whole.trap_sites_for_reload(self.lowering.hir, self.lowering.reload));
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
        if let hir::ExprKind::Local(name, _, _) = &target_expr.kind {
            let binding = self.lookup_binding(name, &target_expr.pos)?;
            let old = if op.is_some() {
                Some(self.read_binding(binding, &target_expr.pos)?)
            } else {
                None
            };
            let mut assigned_value = None;
            let result = if let Some(op) = op {
                let value = self.require_expr(value_expr)?;
                self.emit(
                    l::InstructionKind::Binary(convert_binary(op)?),
                    vec![old.clone().expect("compound old value"), value],
                    Some(l::ValueType::Data(target_expr.ty.clone())),
                    false,
                    binary_traps(),
                    target_expr.pos.clone(),
                )?
                .expect("compound result")
            } else {
                let (value, stored) = self.lower_assignment_value(target_expr, value_expr)?;
                assigned_value = Some(value);
                stored
            };
            self.write_binding(binding, result.clone(), &target_expr.pos, Vec::new())?;
            return if update == Some(hir::UpdateKind::Postfix) {
                old.ok_or_else(|| self.error(&whole.pos, "postfix update has no previous value"))
            } else {
                Ok(assigned_value.unwrap_or(result))
            };
        }
        let mut place = self.prepare_place(target_expr)?;
        if array_ownership::runs_user_code(value_expr) {
            if let PreparedPlaceKind::Index {
                base: PreparedBase::Value(value),
                ..
            } = &place.kind
            {
                if matches!(&target_expr.kind, hir::ExprKind::Index { obj, .. } if self.input_needs_hold(obj))
                {
                    self.hold_input(value, &whole.pos)?;
                }
            }
        }
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
        let mut assigned_value = None;
        let result = if let Some(op) = op {
            let value = self.require_expr(value_expr)?;
            self.emit(
                l::InstructionKind::Binary(convert_binary(op)?),
                vec![old.clone().expect("compound old value"), value],
                Some(l::ValueType::Data(target_expr.ty.clone())),
                false,
                binary_traps(),
                target_expr.pos.clone(),
            )?
            .expect("compound result")
        } else {
            let (value, stored) = self.lower_assignment_value(target_expr, value_expr)?;
            assigned_value = Some(value);
            stored
        };
        self.store_place(&place, result.clone(), &target_expr.pos)?;
        let result = if let PreparedPlaceKind::Index {
            base: PreparedBase::Value(receiver),
            ..
        } = &place.kind
        {
            let value = assigned_value.take().unwrap_or(result);
            self.finish_temporary_read(value, receiver, &whole.ty, &whole.pos, result_used)?
        } else {
            assigned_value.take().unwrap_or(result)
        };
        if update == Some(hir::UpdateKind::Postfix) {
            old.ok_or_else(|| self.error(&whole.pos, "postfix update has no previous value"))
        } else {
            Ok(assigned_value.unwrap_or(result))
        }
    }

    /// Retains the value operand separately from its storage representation.
    fn lower_assignment_value(
        &mut self,
        target: &hir::Expr,
        value: &hir::Expr,
    ) -> Result<(l::Operand, l::Operand), LowerError> {
        if self.embedded_header_extension(&target.ty, value).is_some() {
            let stored = self.lower_stored_expr_at(&target.ty, value, &target.pos)?;
            let result = self.coerce_read(
                stored.clone(),
                &value.ty,
                l::NarrowOrigin::Local,
                Vec::new(),
                &value.pos,
            )?;
            return Ok((result, stored));
        }
        let result = self.require_expr(value)?;
        let stored = self.coerce_operand(
            result.clone(),
            l::ValueType::Data(target.ty.clone()),
            &target.pos,
        )?;
        Ok((result, stored))
    }
}

// The path classification is shared with the checker and sites (compiler.md §124).
pub(super) fn narrow_origin(expr: &hir::Expr, classes: &[hir::ClassDef]) -> l::NarrowOrigin {
    if expr.is_shared_location(classes) {
        l::NarrowOrigin::SharedRead
    } else {
        l::NarrowOrigin::Local
    }
}
