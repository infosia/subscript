//! Lowering for statements: declarations, conditionals, loops, iteration callbacks, and switch.

use super::verify_dominance::successors;
use super::*;

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    pub(super) fn require_expr(&mut self, expr: &hir::Expr) -> Result<l::Operand, LowerError> {
        self.lower_expr(expr)?.ok_or_else(|| {
            self.error(
                &expr.pos,
                format!("void expression used where `{}` is required", expr.ty),
            )
        })
    }

    pub(super) fn lower_statements(&mut self, statements: &[hir::Stmt]) -> Result<(), LowerError> {
        for statement in statements {
            if self.current.is_none() {
                break;
            }
            self.lower_statement(statement)?;
        }
        Ok(())
    }

    fn lower_scoped(&mut self, statements: &[hir::Stmt]) -> Result<(), LowerError> {
        self.scopes.push(HashMap::new());
        let result = self.lower_statements(statements);
        if result.is_ok() && self.current.is_some() {
            let pos = statements
                .last()
                .map(stmt_pos)
                .unwrap_or_else(|| self.function.pos.clone());
            self.release_scopes_from(self.scopes.len() - 1, &pos)?;
        }
        self.scopes.pop();
        result
    }

    fn lower_statement(&mut self, statement: &hir::Stmt) -> Result<(), LowerError> {
        match statement {
            hir::Stmt::Let {
                name,
                ty,
                mutable,
                dispose: _,
                init,
                pos,
            } => {
                let value = self.lower_stored_expr_at(ty, init, pos)?;
                self.declare_binding(
                    name.clone(),
                    l::ValueType::Data(ty.clone()),
                    *mutable,
                    value,
                    pos.clone(),
                    Some(hir::AsyncCopySite::Binding),
                )?;
            }
            hir::Stmt::Expr(expr) => {
                let result = self.lower_expr(expr)?;
                if expr.kind.produces_fresh_async_owner()
                    && is_async_owner_type(&l::ValueType::Data(expr.ty.clone()))
                {
                    if let Some(value) = result {
                        self.discard_owner(
                            hir::AsyncCopySite::DiscardedResult,
                            value,
                            &l::ValueType::Data(expr.ty.clone()),
                            &expr.pos,
                        )?;
                    }
                }
            }
            hir::Stmt::Return { value, pos } => {
                let value = value
                    .as_ref()
                    .map(|value| self.lower_stored_expr_at(&self.function.ret.clone(), value, pos))
                    .transpose()?;
                self.terminate_return(value, l::ValueType::Data(self.function.ret.clone()), pos)?;
            }
            hir::Stmt::If {
                cond,
                then,
                els,
                pos,
            } => self.lower_if(cond, then, els.as_deref().unwrap_or(&[]), pos)?,
            hir::Stmt::While {
                cond, body, pos, ..
            } => self.lower_while(cond, body, pos)?,
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                pos,
            } => self.lower_for(init.as_deref(), cond.as_ref(), step.as_ref(), body, pos)?,
            hir::Stmt::ForOf {
                name,
                ty,
                subject,
                kind,
                body,
                pos,
            } => self.lower_for_of(name, ty, subject, *kind, body, pos)?,
            hir::Stmt::Switch { disc, cases, pos } => {
                self.lower_switch(disc, cases, pos)?;
            }
            hir::Stmt::Break(pos) => {
                let block = self
                    .controls
                    .last()
                    .map(|control| (control.break_target, control.scope_depth))
                    .ok_or_else(|| self.error(pos, "break has no enclosing target"))?;
                self.release_scopes_from(block.1, pos)?;
                let edge = self.block_target(block.0, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), pos)?;
            }
            hir::Stmt::Continue(pos) => {
                let control = self
                    .controls
                    .iter()
                    .rev()
                    .find_map(|control| {
                        control
                            .continue_target
                            .map(|target| (target, control.scope_depth))
                    })
                    .ok_or_else(|| self.error(pos, "continue has no enclosing loop"))?;
                self.release_scopes_from(control.1, pos)?;
                let edge = self.block_target(control.0, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), pos)?;
            }
            hir::Stmt::Block(statements) => self.lower_scoped(statements)?,
        }
        Ok(())
    }

    fn lower_if(
        &mut self,
        cond: &hir::Expr,
        then: &[hir::Stmt],
        els: &[hir::Stmt],
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let condition = self.require_expr(cond)?;
        let branch_state = self.binding_snapshot();
        let then_block = self.new_block(Vec::new(), Some("if.then".to_string()));
        let else_block = self.new_block(Vec::new(), Some("if.else".to_string()));
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition,
                then_target: target(then_block, Vec::new()),
                else_target: target(else_block, Vec::new()),
            },
            pos,
        )?;
        self.current = Some(then_block);
        self.lower_scoped(then)?;
        let then_end = self.current;
        let then_state = self.binding_snapshot();
        self.restore_bindings(&branch_state);
        self.current = Some(else_block);
        self.lower_scoped(els)?;
        let else_end = self.current;
        let else_state = self.binding_snapshot();
        if then_end.is_none() && else_end.is_none() {
            self.current = None;
            return Ok(());
        }
        let join = self.new_state_block(Vec::new(), Some("if.join".to_string()), &[]);
        for (end, state) in [(then_end, then_state), (else_end, else_state)] {
            let Some(end) = end else { continue };
            self.restore_bindings(&state);
            self.current = Some(end);
            let edge = self.block_target(join, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }
        self.enter_block(join)?;
        Ok(())
    }

    fn lower_while(
        &mut self,
        cond: &hir::Expr,
        body: &[hir::Stmt],
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let header = self.new_state_block(Vec::new(), Some("while.cond".to_string()), &[]);
        let body_block = self.new_block(Vec::new(), Some("while.body".to_string()));
        let exit = self.new_state_block(Vec::new(), Some("while.exit".to_string()), &[]);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), pos)?;
        self.enter_block(header)?;
        let condition = self.require_expr(cond)?;
        let exit_target = self.block_target(exit, Vec::new())?;
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition,
                then_target: target(body_block, Vec::new()),
                else_target: exit_target,
            },
            pos,
        )?;
        self.controls.push(Control {
            break_target: exit,
            continue_target: Some(header),
            scope_depth: self.scopes.len(),
        });
        self.current = Some(body_block);
        self.lower_scoped(body)?;
        if self.current.is_some() {
            let edge = self.block_target(header, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }
        self.controls.pop();
        self.enter_block(exit)?;
        Ok(())
    }

    fn lower_for(
        &mut self,
        init: Option<&hir::Stmt>,
        cond: Option<&hir::Expr>,
        step: Option<&hir::Expr>,
        body: &[hir::Stmt],
        pos: &Pos,
    ) -> Result<(), LowerError> {
        self.scopes.push(HashMap::new());
        if let Some(init) = init {
            self.lower_statement(init)?;
        }
        let header = self.new_state_block(Vec::new(), Some("for.cond".to_string()), &[]);
        let body_block = self.new_block(Vec::new(), Some("for.body".to_string()));
        let step_block = self.new_state_block(Vec::new(), Some("for.step".to_string()), &[]);
        let exit = self.new_state_block(Vec::new(), Some("for.exit".to_string()), &[]);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), pos)?;
        self.enter_block(header)?;
        if let Some(cond) = cond {
            let condition = self.require_expr(cond)?;
            let exit_target = self.block_target(exit, Vec::new())?;
            self.terminate(
                l::Terminator::ConditionalBranch {
                    condition,
                    then_target: target(body_block, Vec::new()),
                    else_target: exit_target,
                },
                pos,
            )?;
        } else {
            self.terminate(branch(body_block), pos)?;
        }
        self.controls.push(Control {
            break_target: exit,
            continue_target: Some(step_block),
            scope_depth: self.scopes.len(),
        });
        self.current = Some(body_block);
        self.lower_scoped(body)?;
        if self.current.is_some() {
            let edge = self.block_target(step_block, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }
        let step_reachable = self.blocks.iter().any(|block| {
            block
                .terminator
                .as_ref()
                .is_some_and(|terminator| successors(terminator).contains(&step_block))
        });
        if step_reachable {
            self.enter_block(step_block)?;
            if let Some(step) = step {
                self.lower_expr(step)?;
            }
            if self.current.is_some() {
                let edge = self.block_target(header, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), pos)?;
            }
        } else {
            self.current = None;
        }
        self.controls.pop();
        self.enter_block(exit)?;
        self.release_scopes_from(self.scopes.len() - 1, pos)?;
        self.scopes.pop();
        Ok(())
    }

    fn lower_for_of(
        &mut self,
        name: &str,
        ty: &Type,
        subject: &hir::Expr,
        kind: hir::ForOfKind,
        body: &[hir::Stmt],
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let subject_value = self.require_expr(subject)?;
        let kind = convert_for_of(kind);
        let iterator_type = l::ValueType::Iterator(l::IteratorType {
            kind,
            element: ty.clone(),
        });
        let iterator = self
            .emit(
                l::InstructionKind::IteratorCreate {
                    kind,
                    bound: l::IteratorBoundKind::Live,
                },
                vec![subject_value],
                Some(iterator_type.clone()),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("iterator result");
        let bound = self
            .emit(
                l::InstructionKind::IteratorBound,
                vec![iterator.clone()],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("iterator bound");
        let index = l::Operand::Constant(l::Constant {
            ty: Type::I32,
            kind: l::ConstantKind::Integer(0),
        });
        self.scopes.push(HashMap::new());
        let cursor_binding =
            self.declare_hidden_binding("<for-of cursor>", iterator_type.clone(), iterator);
        let index_binding =
            self.declare_hidden_binding("<for-of index>", l::ValueType::Data(Type::I32), index);
        let bound_binding =
            self.declare_hidden_binding("<for-of bound>", l::ValueType::Data(Type::I32), bound);
        let traversal = [cursor_binding, index_binding, bound_binding];
        let header = self.new_state_block(Vec::new(), Some("for-of.cond".to_string()), &traversal);
        let body_block = self.new_block(Vec::new(), Some("for-of.body".to_string()));
        let step_block =
            self.new_state_block(Vec::new(), Some("for-of.step".to_string()), &traversal);
        let exit = self.new_state_block(Vec::new(), Some("for-of.exit".to_string()), &[]);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), pos)?;
        self.enter_block(header)?;
        let cursor = self.read_binding(cursor_binding, pos)?;
        let index = self.read_binding(index_binding, pos)?;
        let bound = self.read_binding(bound_binding, pos)?;
        let more = self
            .emit(
                l::InstructionKind::IteratorHasNext,
                vec![cursor, index, bound],
                Some(l::ValueType::Data(Type::Bool)),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("iterator condition");
        let exit_target = self.block_target(exit, Vec::new())?;
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition: more,
                then_target: target(body_block, Vec::new()),
                else_target: exit_target,
            },
            pos,
        )?;
        self.controls.push(Control {
            break_target: exit,
            continue_target: Some(step_block),
            scope_depth: self.scopes.len(),
        });
        self.current = Some(body_block);
        let cursor = self.read_binding(cursor_binding, pos)?;
        let index = self.read_binding(index_binding, pos)?;
        let bound = self.read_binding(bound_binding, pos)?;
        let value = self
            .emit(
                l::InstructionKind::IteratorValue,
                vec![cursor, index, bound],
                Some(l::ValueType::Data(ty.clone())),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("iterator value");
        self.scopes.push(HashMap::new());
        self.declare_binding(
            name.to_string(),
            l::ValueType::Data(ty.clone()),
            true,
            value,
            pos.clone(),
            Some(hir::AsyncCopySite::ForOfBinding),
        )?;
        self.lower_scoped(body)?;
        if self.current.is_some() {
            self.release_scopes_from(self.scopes.len() - 1, pos)?;
        }
        self.scopes.pop();
        if self.current.is_some() {
            let edge = self.block_target(step_block, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }
        let step_reachable = self.blocks.iter().any(|block| {
            block
                .terminator
                .as_ref()
                .is_some_and(|terminator| successors(terminator).contains(&step_block))
        });
        if step_reachable {
            self.enter_block(step_block)?;
            let cursor = self.read_binding(cursor_binding, pos)?;
            let index = self.read_binding(index_binding, pos)?;
            let bound = self.read_binding(bound_binding, pos)?;
            let advanced = self
                .emit(
                    l::InstructionKind::IteratorAdvance,
                    vec![cursor, index.clone(), bound],
                    Some(iterator_type),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .expect("advanced iterator");
            let next_index = self
                .emit(
                    l::InstructionKind::Binary(l::BinaryOp::Add),
                    vec![
                        index,
                        l::Operand::Constant(l::Constant {
                            ty: Type::I32,
                            kind: l::ConstantKind::Integer(1),
                        }),
                    ],
                    Some(l::ValueType::Data(Type::I32)),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .expect("advanced iterator index");
            self.bindings[cursor_binding.0].value = Some(advanced);
            self.bindings[index_binding.0].value = Some(next_index);
            let edge = self.block_target(header, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }
        self.controls.pop();
        self.enter_block(exit)?;
        self.release_scopes_from(self.scopes.len() - 1, pos)?;
        self.scopes.pop();
        Ok(())
    }

    fn create_iterator(
        &mut self,
        subject: l::Operand,
        kind: l::ForOfKind,
        element: Type,
        bound: l::IteratorBoundKind,
        pos: &Pos,
    ) -> Result<(l::ValueType, l::Operand), LowerError> {
        let iterator_type = l::ValueType::Iterator(l::IteratorType { kind, element });
        let iterator = self
            .emit(
                l::InstructionKind::IteratorCreate { kind, bound },
                vec![subject],
                Some(iterator_type.clone()),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("iterator result");
        Ok((iterator_type, iterator))
    }

    fn lower_static_callback(
        &mut self,
        callback: &hir::Expr,
    ) -> Result<StaticCallback, LowerError> {
        let Type::Func(_) = &callback.ty else {
            return Err(self.error(&callback.pos, "static callback is not function-typed"));
        };
        match &callback.kind {
            hir::ExprKind::FuncRef(name) => {
                let function = self
                    .lowering
                    .free_functions
                    .get(name)
                    .map(|record| record.id)
                    .ok_or_else(|| {
                        self.error(&callback.pos, format!("unknown callback function `{name}`"))
                    })?;
                let _ = self.require_expr(callback)?;
                Ok(StaticCallback {
                    target: l::CallTargetKind::Function(function),
                    callable: None,
                    ty: callback.ty.clone(),
                })
            }
            hir::ExprKind::Lambda {
                params,
                ret,
                body,
                captures,
            } => {
                let (function, callable) =
                    self.lower_lambda_with_id(params, ret, body, captures, callback)?;
                Ok(StaticCallback {
                    target: l::CallTargetKind::StaticClosure(function),
                    callable: Some(callable),
                    ty: callback.ty.clone(),
                })
            }
            _ => Err(self.error(&callback.pos, "callback is not a known function")),
        }
    }

    fn emit_static_callback_call(
        &mut self,
        callback: &StaticCallback,
        callable: Option<l::Operand>,
        arguments: Vec<l::Operand>,
        traps: Vec<l::Trap>,
        pos: &Pos,
    ) -> Result<Option<l::Operand>, LowerError> {
        let Type::Func(signature) = &callback.ty else {
            return Err(self.error(pos, "static callback type is not callable"));
        };
        let mut operands = Vec::with_capacity(arguments.len() + usize::from(callable.is_some()));
        let mut parameter_types = Vec::with_capacity(operands.capacity());
        if let Some(callable) = callable {
            operands.push(callable);
            parameter_types.push(l::ValueType::Data(callback.ty.clone()));
        }
        operands.extend(arguments);
        parameter_types.extend(signature.params.iter().cloned().map(l::ValueType::Data));
        let return_type =
            (signature.ret != Type::Void).then(|| l::ValueType::Data(signature.ret.clone()));
        self.emit(
            l::InstructionKind::Call(l::CallTarget {
                kind: callback.target.clone(),
                parameter_types,
                return_type: return_type.clone(),
            }),
            operands,
            return_type,
            true,
            traps,
            pos.clone(),
        )
    }

    fn emit_static_array_push(
        &mut self,
        array: l::Operand,
        value: l::Operand,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        self.emit(
            l::InstructionKind::Call(l::CallTarget {
                kind: l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayPush),
                parameter_types: Vec::new(),
                return_type: Some(l::ValueType::Data(Type::I32)),
            }),
            vec![array, value],
            Some(l::ValueType::Data(Type::I32)),
            true,
            Vec::new(),
            pos.clone(),
        )?;
        Ok(())
    }

    pub(super) fn lower_static_array_callback(
        &mut self,
        operation: hir::ArrFn,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        let Some(subject) = args.first() else {
            return Err(self.error(&expr.pos, "static Array callback has no receiver"));
        };
        let Some(callback_expr) = args.get(1) else {
            return Err(self.error(&expr.pos, "static Array callback has no callback"));
        };
        let Type::Array(element) = &subject.ty else {
            return Err(self.error(
                &subject.pos,
                "static Array callback receiver is not dynamic",
            ));
        };
        let element = (**element).clone();
        let subject_value = self.require_expr(subject)?;
        let callback = self.lower_static_callback(callback_expr)?;
        let initial = if matches!(operation, hir::ArrFn::Reduce | hir::ArrFn::ReduceRight) {
            Some(
                args.get(2)
                    .ok_or_else(|| self.error(&expr.pos, "reduce has no initial value"))
                    .and_then(|initial| self.require_expr(initial))?,
            )
        } else {
            None
        };
        let Type::Func(callback_type) = &callback.ty else {
            unreachable!("static callback type was checked")
        };
        let indexed_arity = operation.callback_index_arity();
        let indexed = indexed_arity == Some(callback_type.params.len());
        if !indexed && indexed_arity.is_some_and(|arity| callback_type.params.len() + 1 != arity) {
            return Err(self.error(&callback_expr.pos, "static callback arity is invalid"));
        }

        let reverse = operation == hir::ArrFn::ReduceRight;
        let kind = if reverse {
            l::ForOfKind::ArrayValuesReverse
        } else {
            l::ForOfKind::ArrayValues
        };
        let (iterator_type, iterator) = self.create_iterator(
            subject_value.clone(),
            kind,
            element.clone(),
            l::IteratorBoundKind::Fixed,
            &expr.pos,
        )?;
        let reverse_index_iterator = if reverse && indexed {
            Some(self.create_iterator(
                subject_value,
                l::ForOfKind::ArrayKeysReverse,
                Type::I32,
                l::IteratorBoundKind::Fixed,
                &expr.pos,
            )?)
        } else {
            None
        };
        let bound = self
            .emit(
                l::InstructionKind::IteratorBound,
                vec![iterator.clone()],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("static callback iterator bound");
        let call_traps = convert_traps(&expr.trap_sites(self.lowering.hir))
            .into_iter()
            .filter(|trap| trap.kind == l::TrapKind::Call)
            .collect::<Vec<_>>();

        let output = if matches!(operation, hir::ArrFn::Map | hir::ArrFn::Filter) {
            let Type::Array(output_element) = &expr.ty else {
                return Err(self.error(&expr.pos, "Array producer result is not an array"));
            };
            Some(
                self.emit(
                    l::InstructionKind::ArrayWithCapacity,
                    vec![bound.clone()],
                    Some(l::ValueType::Data(Type::Array(output_element.clone()))),
                    false,
                    call_traps.clone(),
                    expr.pos.clone(),
                )?
                .expect("capacity array result"),
            )
        } else {
            None
        };
        let initial_result = match operation {
            hir::ArrFn::Map | hir::ArrFn::Filter => output.clone(),
            hir::ArrFn::Reduce | hir::ArrFn::ReduceRight => initial,
            hir::ArrFn::Some => Some(bool_constant(false)),
            hir::ArrFn::Every => Some(bool_constant(true)),
            hir::ArrFn::FindIndex => Some(i32_constant(-1)),
            hir::ArrFn::ForEach => None,
            _ => return Err(self.error(&expr.pos, "unsupported static Array callback operation")),
        };

        self.scopes.push(HashMap::new());
        let cursor_binding =
            self.declare_hidden_binding("<array callback cursor>", iterator_type.clone(), iterator);
        let reverse_index_binding = reverse_index_iterator.map(|(ty, iterator)| {
            self.declare_hidden_binding("<array callback reverse index>", ty, iterator)
        });
        let callable_binding = callback.callable.clone().map(|callable| {
            self.declare_hidden_binding(
                "<array callback callable>",
                l::ValueType::Data(callback.ty.clone()),
                callable,
            )
        });
        let index_binding = self.declare_hidden_binding(
            "<array callback step>",
            l::ValueType::Data(Type::I32),
            i32_constant(0),
        );
        let bound_binding = self.declare_hidden_binding(
            "<array callback bound>",
            l::ValueType::Data(Type::I32),
            bound,
        );
        let result_binding = initial_result.map(|value| {
            self.declare_hidden_binding(
                "<array callback result>",
                l::ValueType::Data(expr.ty.clone()),
                value,
            )
        });
        let mut traversal = vec![cursor_binding, index_binding, bound_binding];
        traversal.extend(reverse_index_binding);
        traversal.extend(callable_binding);
        traversal.extend(result_binding);

        let header = self.new_state_block(
            Vec::new(),
            Some("array-callback.cond".to_string()),
            &traversal,
        );
        let body = self.new_block(Vec::new(), Some("array-callback.body".to_string()));
        let step = self.new_state_block(
            Vec::new(),
            Some("array-callback.step".to_string()),
            &traversal,
        );
        let exit_forced = result_binding.into_iter().collect::<Vec<_>>();
        let exit = self.new_state_block(
            Vec::new(),
            Some("array-callback.exit".to_string()),
            &exit_forced,
        );
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;

        self.enter_block(header)?;
        let header_cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let header_index = self.read_binding(index_binding, &expr.pos)?;
        let header_bound = self.read_binding(bound_binding, &expr.pos)?;
        let more = self
            .emit(
                l::InstructionKind::IteratorHasNext,
                vec![header_cursor, header_index, header_bound],
                Some(l::ValueType::Data(Type::Bool)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("static callback iterator condition");
        let exit_target = self.block_target(exit, Vec::new())?;
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition: more,
                then_target: target(body, Vec::new()),
                else_target: exit_target,
            },
            &expr.pos,
        )?;

        self.current = Some(body);
        let cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let step_index = self.read_binding(index_binding, &expr.pos)?;
        let captured_bound = self.read_binding(bound_binding, &expr.pos)?;
        let value = self
            .emit(
                l::InstructionKind::IteratorValue,
                vec![cursor, step_index.clone(), captured_bound.clone()],
                Some(l::ValueType::Data(element.clone())),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("static callback iterator value");
        let callback_index = if let Some(binding) = reverse_index_binding {
            let reverse_cursor = self.read_binding(binding, &expr.pos)?;
            self.emit(
                l::InstructionKind::IteratorValue,
                vec![reverse_cursor, step_index.clone(), captured_bound.clone()],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("reverse callback index")
        } else {
            step_index.clone()
        };
        let mut arguments = if matches!(operation, hir::ArrFn::Reduce | hir::ArrFn::ReduceRight) {
            vec![
                self.read_binding(result_binding.expect("reduce result binding"), &expr.pos)?,
                value.clone(),
            ]
        } else {
            vec![value.clone()]
        };
        if indexed {
            arguments.push(callback_index.clone());
        }
        let callable = callable_binding
            .map(|binding| self.read_binding(binding, &expr.pos))
            .transpose()?;
        let callback_result =
            self.emit_static_callback_call(&callback, callable, arguments, call_traps, &expr.pos)?;

        let mut branch_to_step = true;
        match operation {
            hir::ArrFn::Map => {
                let output =
                    self.read_binding(result_binding.expect("map result binding"), &expr.pos)?;
                self.emit_static_array_push(
                    output,
                    callback_result.expect("map callback result"),
                    &expr.pos,
                )?;
            }
            hir::ArrFn::Filter => {
                let push = self.new_block(Vec::new(), Some("array-callback.push".to_string()));
                let step_target = self.block_target(step, Vec::new())?;
                self.terminate(
                    l::Terminator::ConditionalBranch {
                        condition: callback_result.expect("filter callback result"),
                        then_target: target(push, Vec::new()),
                        else_target: step_target,
                    },
                    &expr.pos,
                )?;
                self.current = Some(push);
                let output =
                    self.read_binding(result_binding.expect("filter result binding"), &expr.pos)?;
                self.emit_static_array_push(output, value, &expr.pos)?;
            }
            hir::ArrFn::Reduce | hir::ArrFn::ReduceRight => {
                self.bindings[result_binding.expect("reduce result binding").0].value =
                    Some(callback_result.expect("reduce callback result"));
            }
            hir::ArrFn::ForEach => {}
            hir::ArrFn::Some | hir::ArrFn::Every | hir::ArrFn::FindIndex => {
                let matched = callback_result.expect("predicate callback result");
                let (finish_on_true, finished_value) = match operation {
                    hir::ArrFn::Some => (true, bool_constant(true)),
                    hir::ArrFn::Every => (false, bool_constant(false)),
                    hir::ArrFn::FindIndex => (true, callback_index),
                    _ => unreachable!(),
                };
                let finish = self.new_block(Vec::new(), Some("array-callback.finish".to_string()));
                let step_target = self.block_target(step, Vec::new())?;
                let (then_target, else_target) = if finish_on_true {
                    (target(finish, Vec::new()), step_target)
                } else {
                    (step_target, target(finish, Vec::new()))
                };
                self.terminate(
                    l::Terminator::ConditionalBranch {
                        condition: matched,
                        then_target,
                        else_target,
                    },
                    &expr.pos,
                )?;
                self.current = Some(finish);
                self.bindings[result_binding.expect("predicate result binding").0].value =
                    Some(finished_value);
                let exit_target = self.block_target(exit, Vec::new())?;
                self.terminate(l::Terminator::Branch(exit_target), &expr.pos)?;
                branch_to_step = false;
            }
            _ => unreachable!(),
        }
        if branch_to_step {
            let edge = self.block_target(step, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), &expr.pos)?;
        }

        self.enter_block(step)?;
        let cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let index = self.read_binding(index_binding, &expr.pos)?;
        let captured_bound = self.read_binding(bound_binding, &expr.pos)?;
        let advanced = self
            .emit(
                l::InstructionKind::IteratorAdvance,
                vec![cursor, index.clone(), captured_bound.clone()],
                Some(iterator_type),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("advanced static callback iterator");
        if reverse {
            self.bindings[cursor_binding.0].value = Some(advanced);
        }
        if let Some(binding) = reverse_index_binding {
            let cursor = self.read_binding(binding, &expr.pos)?;
            let iterator_type = self.bindings[binding.0].ty.clone();
            let advanced = self
                .emit(
                    l::InstructionKind::IteratorAdvance,
                    vec![cursor, index.clone(), captured_bound],
                    Some(iterator_type),
                    false,
                    Vec::new(),
                    expr.pos.clone(),
                )?
                .expect("advanced reverse index iterator");
            self.bindings[binding.0].value = Some(advanced);
        }
        let next_index = self
            .emit(
                l::InstructionKind::Binary(l::BinaryOp::Add),
                vec![index, i32_constant(1)],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("advanced static callback step");
        self.bindings[index_binding.0].value = Some(next_index);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;

        self.enter_block(exit)?;
        let result = result_binding
            .map(|binding| self.read_binding(binding, &expr.pos))
            .transpose()?;
        self.release_scopes_from(self.scopes.len() - 1, &expr.pos)?;
        self.scopes.pop();
        Ok(result)
    }

    /// Lowers `new Set<K>(source)` as one instruction over the
    /// array-literal spread traversal (compiler.md §103.1 rule 4). The
    /// runtime walks the source's own storage behind a shared sink, so
    /// no iterator object and no synthesized call exist on any tier.
    pub(super) fn lower_set_from_source(
        &mut self,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        let [source] = args else {
            return Err(self.error(&expr.pos, "`new Set(source)` takes one source operand"));
        };
        if !matches!(&expr.ty, Type::Set(_)) {
            return Err(self.error(&expr.pos, "`new Set(source)` result is not a Set"));
        }
        let Some((traversal, _)) = source.ty.iteration_element() else {
            return Err(self.error(&source.pos, "`new Set(source)` source is not a container"));
        };
        let spread = convert_spread(hir::SpreadKind::from(traversal));
        let source_value = self.require_expr(source)?;
        let traps = convert_traps(&expr.trap_sites(self.lowering.hir));
        self.emit(
            l::InstructionKind::SetFromSource(spread),
            vec![source_value],
            Some(l::ValueType::Data(expr.ty.clone())),
            false,
            traps,
            expr.pos.clone(),
        )
    }

    pub(super) fn lower_for_each(
        &mut self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        let [subject, callback] = args else {
            return Err(self.error(
                &expr.pos,
                format!("forEach lowering expected 2 operands, got {}", args.len()),
            ));
        };
        let subject_value = self.require_expr(subject)?;
        let callback_value = self.require_expr(callback)?;
        let Type::Func(callback_type) = &callback.ty else {
            return Err(self.error(&callback.pos, "forEach callback is not function-typed"));
        };
        if callback_type.ret != Type::Void {
            return Err(self.error(&callback.pos, "forEach callback does not return void"));
        }

        let (kind, element, secondary, bound) = match (callee, &subject.ty) {
            (hir::Callee::Arr(hir::ArrFn::ForEach), Type::Array(element)) => (
                l::ForOfKind::ArrayValues,
                (**element).clone(),
                None,
                l::IteratorBoundKind::Fixed,
            ),
            (hir::Callee::Arr(hir::ArrFn::ForEach), Type::FixedArray(element, _)) => (
                l::ForOfKind::FixedArrayValues,
                (**element).clone(),
                None,
                l::IteratorBoundKind::Live,
            ),
            (hir::Callee::Map(hir::MapFn::ForEach), Type::Map(key, value)) => (
                l::ForOfKind::MapValues,
                (**value).clone(),
                Some((l::ForOfKind::MapKeys, (**key).clone())),
                l::IteratorBoundKind::Live,
            ),
            (hir::Callee::Set(hir::SetFn::ForEach), Type::Set(key)) => (
                l::ForOfKind::SetValues,
                (**key).clone(),
                None,
                l::IteratorBoundKind::Live,
            ),
            _ => {
                return Err(self.error(&subject.pos, "forEach spelling and receiver type disagree"));
            }
        };

        let (iterator_type, iterator) = self.create_iterator(
            subject_value.clone(),
            kind,
            element.clone(),
            bound,
            &expr.pos,
        )?;
        let secondary_iterator = secondary
            .map(|(kind, element)| {
                self.create_iterator(subject_value.clone(), kind, element, bound, &expr.pos)
            })
            .transpose()?;
        let captured_bound = self
            .emit(
                l::InstructionKind::IteratorBound,
                vec![iterator.clone()],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("iterator bound");
        let index = l::Operand::Constant(l::Constant {
            ty: Type::I32,
            kind: l::ConstantKind::Integer(0),
        });

        self.scopes.push(HashMap::new());
        let cursor_binding =
            self.declare_hidden_binding("<for-each cursor>", iterator_type.clone(), iterator);
        let secondary_binding = secondary_iterator.map(|(ty, iterator)| {
            self.declare_hidden_binding("<for-each secondary cursor>", ty, iterator)
        });
        let callback_binding = self.declare_hidden_binding(
            "<for-each callback>",
            l::ValueType::Data(callback.ty.clone()),
            callback_value,
        );
        let index_binding =
            self.declare_hidden_binding("<for-each index>", l::ValueType::Data(Type::I32), index);
        let bound_binding = self.declare_hidden_binding(
            "<for-each bound>",
            l::ValueType::Data(Type::I32),
            captured_bound,
        );
        let mut traversal = vec![
            cursor_binding,
            callback_binding,
            index_binding,
            bound_binding,
        ];
        traversal.extend(secondary_binding);

        let header =
            self.new_state_block(Vec::new(), Some("for-each.cond".to_string()), &traversal);
        let body = self.new_block(Vec::new(), Some("for-each.body".to_string()));
        let step = self.new_state_block(Vec::new(), Some("for-each.step".to_string()), &traversal);
        let exit = self.new_state_block(Vec::new(), Some("for-each.exit".to_string()), &[]);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;

        self.enter_block(header)?;
        let cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let index = self.read_binding(index_binding, &expr.pos)?;
        let captured_bound = self.read_binding(bound_binding, &expr.pos)?;
        let more = self
            .emit(
                l::InstructionKind::IteratorHasNext,
                vec![cursor, index, captured_bound],
                Some(l::ValueType::Data(Type::Bool)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("iterator condition");
        let exit_target = self.block_target(exit, Vec::new())?;
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition: more,
                then_target: target(body, Vec::new()),
                else_target: exit_target,
            },
            &expr.pos,
        )?;

        self.current = Some(body);
        let cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let index = self.read_binding(index_binding, &expr.pos)?;
        let captured_bound = self.read_binding(bound_binding, &expr.pos)?;
        let value = self
            .emit(
                l::InstructionKind::IteratorValue,
                vec![cursor, index.clone(), captured_bound.clone()],
                Some(l::ValueType::Data(element)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("iterator value");
        let mut call_operands = vec![self.read_binding(callback_binding, &expr.pos)?, value];
        if let Some(secondary_binding) = secondary_binding {
            let secondary_cursor = self.read_binding(secondary_binding, &expr.pos)?;
            let secondary_type = self.bindings[secondary_binding.0].ty.clone();
            let l::ValueType::Iterator(secondary_type) = secondary_type else {
                unreachable!("secondary cursor binding has an iterator type")
            };
            let secondary_value = self
                .emit(
                    l::InstructionKind::IteratorValue,
                    vec![secondary_cursor, index.clone(), captured_bound],
                    Some(l::ValueType::Data(secondary_type.element)),
                    false,
                    Vec::new(),
                    expr.pos.clone(),
                )?
                .expect("secondary iterator value");
            call_operands.push(secondary_value);
        } else if callback_type.params.len() == 2 {
            call_operands.push(index.clone());
        }
        self.emit(
            l::InstructionKind::Call(l::CallTarget {
                kind: l::CallTargetKind::Indirect,
                parameter_types: std::iter::once(l::ValueType::Data(callback.ty.clone()))
                    .chain(callback_type.params.iter().cloned().map(l::ValueType::Data))
                    .collect(),
                return_type: None,
            }),
            call_operands,
            None,
            true,
            convert_traps(&expr.trap_sites(self.lowering.hir)),
            expr.pos.clone(),
        )?;
        let edge = self.block_target(step, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;

        self.enter_block(step)?;
        let cursor = self.read_binding(cursor_binding, &expr.pos)?;
        let index = self.read_binding(index_binding, &expr.pos)?;
        let captured_bound = self.read_binding(bound_binding, &expr.pos)?;
        let advanced = self
            .emit(
                l::InstructionKind::IteratorAdvance,
                vec![cursor, index.clone(), captured_bound.clone()],
                Some(iterator_type),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("advanced iterator");
        self.bindings[cursor_binding.0].value = Some(advanced);
        if let Some(secondary_binding) = secondary_binding {
            let cursor = self.read_binding(secondary_binding, &expr.pos)?;
            let iterator_type = self.bindings[secondary_binding.0].ty.clone();
            let advanced = self
                .emit(
                    l::InstructionKind::IteratorAdvance,
                    vec![cursor, index.clone(), captured_bound],
                    Some(iterator_type),
                    false,
                    Vec::new(),
                    expr.pos.clone(),
                )?
                .expect("advanced secondary iterator");
            self.bindings[secondary_binding.0].value = Some(advanced);
        }
        let next_index = self
            .emit(
                l::InstructionKind::Binary(l::BinaryOp::Add),
                vec![
                    index,
                    l::Operand::Constant(l::Constant {
                        ty: Type::I32,
                        kind: l::ConstantKind::Integer(1),
                    }),
                ],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                expr.pos.clone(),
            )?
            .expect("advanced iterator index");
        self.bindings[index_binding.0].value = Some(next_index);
        let edge = self.block_target(header, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;

        self.enter_block(exit)?;
        self.release_scopes_from(self.scopes.len() - 1, &expr.pos)?;
        self.scopes.pop();
        Ok(None)
    }

    fn lower_switch(
        &mut self,
        disc: &hir::Expr,
        cases: &[hir::SwitchCase],
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let value = self.require_expr(disc)?;
        let exit = self.new_state_block(Vec::new(), Some("switch.exit".to_string()), &[]);
        let exhaustive_alias =
            matches!(disc.ty, Type::StringAlias(_)) && cases.iter().all(|case| case.test.is_some());
        let no_match = if exhaustive_alias {
            let block = self.new_block(Vec::new(), Some("switch.exhaustive".to_string()));
            self.blocks[block.0 as usize].terminator =
                Some(l::Terminator::Unreachable { pos: pos.clone() });
            block
        } else {
            exit
        };
        let case_blocks = cases
            .iter()
            .enumerate()
            .map(|(index, _)| {
                self.new_state_block(Vec::new(), Some(format!("switch.case.{index}")), &[])
            })
            .collect::<Vec<_>>();
        let mut default = no_match;
        for (case, block) in cases.iter().zip(&case_blocks) {
            if case.test.is_none() {
                default = *block;
            }
        }
        let all_constant = cases
            .iter()
            .filter_map(|case| case.test.as_ref())
            .all(|test| constant_expr(test).is_some());
        if all_constant {
            let arms = cases
                .iter()
                .zip(&case_blocks)
                .filter_map(|(case, block)| {
                    case.test.as_ref().map(|test| {
                        Ok(l::SwitchArm {
                            value: constant_expr(test).expect("constant switch case"),
                            target: self.block_target(*block, Vec::new())?,
                        })
                    })
                })
                .collect::<Result<Vec<_>, LowerError>>()?;
            let default = self.block_target(default, Vec::new())?;
            self.terminate(
                l::Terminator::Switch {
                    value,
                    arms,
                    default,
                },
                pos,
            )?;
        } else {
            let tests = cases
                .iter()
                .enumerate()
                .filter_map(|(index, case)| case.test.as_ref().map(|test| (index, test)))
                .collect::<Vec<_>>();
            if tests.is_empty() {
                let edge = self.block_target(default, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), pos)?;
            } else {
                for (test_index, (case_index, test)) in tests.iter().enumerate() {
                    let test_value = self.require_expr(test)?;
                    let test_value = self.coerce_operand(
                        test_value,
                        l::ValueType::Data(disc.ty.clone()),
                        &test.pos,
                    )?;
                    let equal = self
                        .emit(
                            l::InstructionKind::Binary(l::BinaryOp::Eq),
                            vec![value.clone(), test_value],
                            Some(l::ValueType::Data(Type::Bool)),
                            false,
                            Vec::new(),
                            test.pos.clone(),
                        )?
                        .expect("switch test result");
                    let miss = if test_index + 1 == tests.len() {
                        default
                    } else {
                        self.new_state_block(
                            Vec::new(),
                            Some(format!("switch.test.{}", test_index + 1)),
                            &[],
                        )
                    };
                    let then_target = self.block_target(case_blocks[*case_index], Vec::new())?;
                    let else_target = self.block_target(miss, Vec::new())?;
                    self.terminate(
                        l::Terminator::ConditionalBranch {
                            condition: equal,
                            then_target,
                            else_target,
                        },
                        &test.pos,
                    )?;
                    if test_index + 1 != tests.len() {
                        self.enter_block(miss)?;
                    }
                }
            }
        }
        self.controls.push(Control {
            break_target: exit,
            continue_target: None,
            scope_depth: self.scopes.len(),
        });
        let mut previous_end = None;
        for (index, (case, block)) in cases.iter().zip(&case_blocks).enumerate() {
            if let Some(end) = previous_end {
                self.current = Some(end);
                let edge = self.block_target(*block, Vec::new())?;
                self.terminate(l::Terminator::Branch(edge), &case.pos)?;
            }
            self.enter_block(*block)?;
            self.lower_scoped(&case.body)?;
            previous_end = self.current;
            if index + 1 == cases.len() {
                if let Some(end) = previous_end.take() {
                    self.current = Some(end);
                    let edge = self.block_target(exit, Vec::new())?;
                    self.terminate(l::Terminator::Branch(edge), &case.pos)?;
                }
            }
        }
        self.controls.pop();
        let exit_reachable = self.blocks.iter().any(|block| {
            block
                .terminator
                .as_ref()
                .is_some_and(|terminator| successors(terminator).contains(&exit))
        });
        if exit_reachable {
            self.enter_block(exit)?;
        } else {
            self.current = None;
        }
        Ok(())
    }
}
