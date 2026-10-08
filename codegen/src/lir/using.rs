//! Lowering for a `using` scope (`compiler.md` §115.5 rules 5–7).
//!
//! The checker emits one [`hir::Stmt::Using`] node per scope and places no
//! hook. This module places every hook, on each edge that leaves a node:
//!
//! - the fall-through end, in [`FunctionBuilder::lower_using`];
//! - `return`, `break`, and `continue`, in [`FunctionBuilder::leave`], the
//!   one lowering of those three statements;
//! - the exception edge, through one handler frame per binding.
//!
//! Each hook of a normal exit lowers with the handler stack of its own
//! binding: the handlers that enclose the node, then the exception edges
//! of the earlier bindings. So a raise in a hook never reaches a `try`
//! inside the body (rule 6), and the remaining hooks of the exit run on
//! the exception edge (rule 3).

use super::exception::HandlerFrame;
use super::*;

/// One `using` node while its body lowers.
#[derive(Clone)]
pub(super) struct UsingFrame {
    /// The bindings, in declaration order.
    bindings: Vec<hir::UsingBinding>,
    finalizer: Option<Vec<hir::Stmt>>,
    pub(super) generator: Option<BindingId>,
    finalizer_pos: Pos,
    environment: Vec<HashMap<String, BindingId>>,
    /// For each binding: the binding that holds the resource and the
    /// binding of its active flag, resolved where the node starts.
    storage: Vec<(BindingId, Option<BindingId>)>,
    /// The depth of the handler stack where the node starts.
    handler_depth: usize,
    /// The depth of the lexical scopes where the node starts.
    scope_depth: usize,
}

/// An edge that leaves statements: `return`, `break`, or `continue`.
pub(super) enum Leave {
    /// `return`, with the value already evaluated.
    Return(Option<l::Operand>),
    /// `break` to the nearest control.
    Break,
    /// `continue` to the nearest loop.
    Continue,
}

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    /// A control whose exits leave the `using` nodes that start after it.
    pub(super) fn control(
        &self,
        break_target: l::BlockId,
        continue_target: Option<l::BlockId>,
    ) -> Control {
        Control {
            break_target,
            continue_target,
            scope_depth: self.scopes.len(),
            using_depth: self.usings.len(),
        }
    }

    /// The one lowering of `return`, `break`, and `continue`: the scope
    /// actions, innermost first, then the terminator.
    pub(super) fn leave(&mut self, edge: Leave, pos: &Pos) -> Result<(), LowerError> {
        match edge {
            Leave::Return(value) => {
                let ty = l::ValueType::Data(self.function.ret.clone());
                self.terminate_return(value, ty, pos)
            }
            Leave::Break => {
                let control = self
                    .controls
                    .last()
                    .map(|control| {
                        (
                            control.break_target,
                            control.scope_depth,
                            control.using_depth,
                        )
                    })
                    .ok_or_else(|| self.error(pos, "break has no enclosing target"))?;
                self.branch_out(control, pos)
            }
            Leave::Continue => {
                let control = self
                    .controls
                    .iter()
                    .rev()
                    .find_map(|control| {
                        control
                            .continue_target
                            .map(|target| (target, control.scope_depth, control.using_depth))
                    })
                    .ok_or_else(|| self.error(pos, "continue has no enclosing loop"))?;
                self.branch_out(control, pos)
            }
        }
    }

    fn branch_out(
        &mut self,
        (target, scope_depth, using_depth): (l::BlockId, usize, usize),
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let kind = if self
            .controls
            .iter()
            .any(|control| control.continue_target == Some(target))
        {
            l::FinalizerCompletion::Continue(target)
        } else {
            l::FinalizerCompletion::Break(target)
        };
        let using_depth = if matches!(kind, l::FinalizerCompletion::Continue(_))
            && self
                .usings
                .get(using_depth)
                .is_some_and(|frame| frame.generator.is_some())
        {
            using_depth + 1
        } else {
            using_depth
        };
        let prior = self.completion.replace((kind, Vec::new()));
        let replaces = self
            .finalizer_scope_depth
            .is_some_and(|depth| scope_depth <= depth);
        let prior_return = self.exit_return.clone();
        if replaces {
            if let Some((value, ty)) = self.exit_return.take() {
                self.release_owner(value, &ty, pos)?;
            }
        }
        let prior_exception = self.finalizer_exception_active;
        if replaces {
            self.finalizer_exception_active = false;
        }
        let actions = self.exit_actions(scope_depth, using_depth, pos);
        self.finalizer_exception_active = prior_exception;
        self.completion = prior;
        self.exit_return = prior_return;
        actions?;
        if self.current.is_none() {
            return Ok(());
        }
        let edge = self.block_target(target, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), pos)
    }

    /// `using` bindings, then `body` (`compiler.md` §115.5 rule 5).
    pub(super) fn lower_using(
        &mut self,
        bindings: &[hir::UsingBinding],
        body: &[hir::Stmt],
        finalizer: Option<&[hir::Stmt]>,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let mut storage = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let value = self.lookup_binding(&binding.name, &binding.pos)?;
            let active = binding
                .active
                .as_ref()
                .map(|active| self.lookup_binding(active, &binding.pos))
                .transpose()?;
            storage.push((value, active));
        }
        let handler_depth = self.handlers.len();
        for _ in 0..bindings.len().max(usize::from(finalizer.is_some())) {
            self.handlers
                .push(HandlerFrame::without_binding(self.scopes.len()));
        }
        self.usings.push(UsingFrame {
            bindings: bindings.to_vec(),
            finalizer: finalizer.map(<[hir::Stmt]>::to_vec),
            generator: None,
            finalizer_pos: pos.clone(),
            environment: self.scopes.clone(),
            storage,
            handler_depth,
            scope_depth: self.scopes.len(),
        });
        let mut lowered = self.lower_scoped(body);
        // §101: no hook where control cannot arrive.
        if lowered.is_ok()
            && self.current.is_some()
            && subscript_compiler::sequence_can_fall_through(body)
        {
            let prior = self
                .completion
                .replace((l::FinalizerCompletion::FallThrough, Vec::new()));
            lowered = self.exit_actions(self.scopes.len(), self.usings.len() - 1, pos);
            self.completion = prior;
        }
        lowered?;
        let frame = self
            .usings
            .pop()
            .ok_or_else(|| self.error(pos, "the `using` stack lost its node"))?;
        let body_end = self.current.take();
        let body_state = self.binding_snapshot();
        // Innermost binding first. The resume of each edge resolves against
        // the edges of the earlier bindings, which are still on the stack.
        for binding in (0..bindings.len().max(usize::from(finalizer.is_some()))).rev() {
            let edge = self
                .handlers
                .pop()
                .ok_or_else(|| self.error(pos, "the handler stack lost a `using` binding"))?;
            if self.handlers.len() != frame.handler_depth + binding {
                return Err(self.error(pos, "a `using` node left the handler stack unbalanced"));
            }
            if frame.finalizer.is_some() {
                self.lower_finalizer_exception(&frame, edge, pos)?;
            } else {
                self.lower_exception_edge(&frame, binding, edge)?;
            }
        }
        self.restore_bindings(&body_state);
        self.current = body_end;
        Ok(())
    }

    /// Places every exit's actions, from the innermost scope to `depth`.
    /// Each body's releases precede its enclosing node's hooks (§116.1 rule 4b).
    /// The exception edge supplies the hooks through its handler frames.
    pub(super) fn exit_actions(
        &mut self,
        depth: usize,
        first: usize,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let scopes = self.scopes.clone();
        let result = self.place_exit_actions(depth, first, pos);
        self.scopes = scopes;
        result
    }

    fn place_exit_actions(
        &mut self,
        depth: usize,
        first: usize,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let frames = self.usings.clone();
        let result = (|| {
            for node in (first..frames.len()).rev() {
                let scope_depth = frames[node].scope_depth;
                self.release_scopes_from(scope_depth, pos)?;
                self.scopes.truncate(scope_depth);
                if frames[node].finalizer.is_some() || frames[node].generator.is_some() {
                    self.usings.truncate(node);
                    let inner = self.handlers.split_off(frames[node].handler_depth);
                    let lowered = self.run_finalizer(&frames[node], pos);
                    self.handlers.extend(inner);
                    lowered?;
                } else {
                    for binding in (0..frames[node].bindings.len()).rev() {
                        if self.current.is_none() {
                            return Ok(());
                        }
                        self.usings = frames[..=node].to_vec();
                        let inner = self
                            .handlers
                            .split_off(frames[node].handler_depth + binding);
                        let lowered = self.lower_hook(node, binding);
                        self.handlers.extend(inner);
                        lowered?;
                    }
                }
                if self.current.is_none() {
                    return Ok(());
                }
            }
            self.release_scopes_from(depth, pos)
        })();
        self.usings = frames;
        result
    }

    fn run_finalizer(&mut self, frame: &UsingFrame, pos: &Pos) -> Result<(), LowerError> {
        if let Some(holder) = frame.generator {
            let value = self.read_binding(holder, pos)?;
            self.emit(
                l::InstructionKind::GeneratorClose,
                vec![value],
                None,
                false,
                Vec::new(),
                pos.clone(),
            )?;
            return Ok(());
        }
        let Some(body) = &frame.finalizer else {
            return Ok(());
        };
        let (completion, operands) = self
            .completion
            .clone()
            .unwrap_or((l::FinalizerCompletion::FallThrough, Vec::new()));
        let prior_exception = self.finalizer_exception_active;
        // The held exception does not propagate through scopes inside the finalizer.
        self.finalizer_exception_active = false;
        if self.function.is_generator {
            self.emit(
                l::InstructionKind::GeneratorFinalizer(frame.finalizer_pos.clone()),
                Vec::new(),
                None,
                false,
                Vec::new(),
                pos.clone(),
            )?;
        }
        self.emit(
            l::InstructionKind::FinalizerEnter(Some(completion)),
            operands,
            None,
            false,
            Vec::new(),
            pos.clone(),
        )?;
        let scopes = std::mem::replace(&mut self.scopes, frame.environment.clone());
        let depth = self.exit_return_depth;
        self.exit_return_depth = self.handlers.len();
        let prior_scope = self.finalizer_scope_depth.replace(self.scopes.len());
        let lowered = self.lower_scoped(body);
        self.finalizer_scope_depth = prior_scope;
        self.finalizer_exception_active = prior_exception;
        self.exit_return_depth = depth;
        self.scopes = scopes;
        lowered
    }

    fn lower_finalizer_exception(
        &mut self,
        frame: &UsingFrame,
        edge: HandlerFrame,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        if edge.landings.is_empty() {
            return Ok(());
        }
        // Each component is an ordinary rooted value. Coroutine liveness
        // places it in the suspended frame when the finalizer can suspend.
        let error_type = l::ValueType::Data(Type::Class(
            self.lowering
                .hir
                .classes
                .iter()
                .enumerate()
                .find(|(_, class)| class.name == "Error")
                .map(|(id, _)| ClassId(id))
                .ok_or_else(|| self.error(pos, "the Error class is missing"))?,
        ));
        let types = vec![
            error_type.clone(),
            l::ValueType::Data(Type::Str),
            l::ValueType::Data(Type::U32),
        ];
        let exit = self.new_state_block(types, Some("finally.exception".into()), &[]);
        for (landing, snapshot) in edge.landings {
            self.restore_bindings(&snapshot);
            self.current = Some(landing);
            let message = self
                .emit(
                    l::InstructionKind::ExceptionMessage,
                    Vec::new(),
                    Some(l::ValueType::Data(Type::Str)),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "exception message has no value"))?;
            let position = self
                .emit(
                    l::InstructionKind::ExceptionPosition,
                    Vec::new(),
                    Some(l::ValueType::Data(Type::U32)),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "exception position has no value"))?;
            let object = self
                .emit(
                    l::InstructionKind::CatchEntry,
                    Vec::new(),
                    Some(error_type.clone()),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "exception object has no value"))?;
            let target = self.block_target(exit, vec![object, message, position])?;
            self.terminate(l::Terminator::Branch(target), pos)?;
        }
        self.enter_block(exit)?;
        let parameters = self.blocks[exit.0 as usize].parameters[..3].to_vec();
        let operands = parameters
            .into_iter()
            .map(l::Operand::Value)
            .collect::<Vec<_>>();
        let mut saved = Vec::new();
        for (index, (operand, ty)) in operands
            .iter()
            .zip([
                error_type,
                l::ValueType::Data(Type::Str),
                l::ValueType::Data(Type::U32),
            ])
            .enumerate()
        {
            let slot = self.add_local(
                format!("[[finally.exception.{index}]]"),
                ty.clone(),
                false,
                pos.clone(),
            )?;
            self.emit(
                l::InstructionKind::StoreLocal(slot),
                vec![operand.clone()],
                None,
                false,
                Vec::new(),
                pos.clone(),
            )?;
            saved.push((slot, ty));
        }
        let prior = self
            .completion
            .replace((l::FinalizerCompletion::Throw, operands));
        self.run_finalizer(frame, pos)?;
        self.completion = prior;
        if self.current.is_some() {
            let operands = saved
                .into_iter()
                .map(|(slot, ty)| {
                    self.emit(
                        l::InstructionKind::LoadLocal(slot),
                        Vec::new(),
                        Some(ty),
                        false,
                        Vec::new(),
                        pos.clone(),
                    )?
                    .ok_or_else(|| self.error(pos, "the finalizer exception slot has no value"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            self.emit(
                l::InstructionKind::ExceptionRestore,
                operands,
                None,
                false,
                vec![l::Trap {
                    kind: l::TrapKind::Raise(l::RaiseEdge::Propagate),
                    pos: pos.clone(),
                }],
                pos.clone(),
            )?;
            self.terminate(l::Terminator::Unreachable { pos: pos.clone() }, pos)?;
        }
        Ok(())
    }

    /// The hook of one binding of the node at `node` on the `using` stack.
    fn lower_hook(&mut self, node: usize, binding: usize) -> Result<(), LowerError> {
        let frame = self.usings[node].clone();
        let hook = frame.bindings[binding].hook();
        let storage = frame.storage[binding];
        let names = (
            frame.bindings[binding].name.clone(),
            frame.bindings[binding].active.clone(),
        );
        if !self.finalizer_exception_active {
            return self.lower_hook_statement(&hook, names, storage);
        }
        self.handlers
            .push(HandlerFrame::without_binding(self.scopes.len()));
        let lowered = self.lower_hook_statement(&hook, names, storage);
        let guard = self.handlers.pop().ok_or_else(|| {
            self.error(
                &frame.bindings[binding].pos,
                "the dispose exception guard is missing",
            )
        })?;
        lowered?;
        let end = self.current.take();
        let state = self.binding_snapshot();
        for (landing, snapshot) in guard.landings {
            self.restore_bindings(&snapshot);
            self.current = Some(landing);
            let pos = stmt_pos(&hook);
            self.emit(
                l::InstructionKind::CatchEntry,
                Vec::new(),
                None,
                false,
                Vec::new(),
                pos.clone(),
            )?;
            self.terminate(
                l::Terminator::Trap(l::Trap {
                    kind: l::TrapKind::DisposeRaisedDuringExit,
                    pos: pos.clone(),
                }),
                &pos,
            )?;
        }
        self.restore_bindings(&state);
        self.current = end;
        Ok(())
    }

    /// Lowers `hook` with its names bound to the bindings the node
    /// resolved, so a later declaration of the same name does not reach
    /// it.
    fn lower_hook_statement(
        &mut self,
        hook: &hir::Stmt,
        (name, active): (String, Option<String>),
        (value, flag): (BindingId, Option<BindingId>),
    ) -> Result<(), LowerError> {
        let mut scope = HashMap::new();
        scope.insert(name, value);
        if let (Some(active), Some(flag)) = (active, flag) {
            scope.insert(active, flag);
        }
        self.scopes.push(scope);
        let lowered = self.lower_statement(hook);
        self.scopes.pop();
        lowered
    }

    /// The exception edge of one binding (`compiler.md` §115.5 rule 7):
    /// each raise site parks its exception and branches to one block,
    /// which runs the hook and resumes the parked exception. A raise in
    /// that hook traps with `DisposeRaisedDuringExit` (rule 2).
    fn lower_exception_edge(
        &mut self,
        frame: &UsingFrame,
        binding: usize,
        edge: HandlerFrame,
    ) -> Result<(), LowerError> {
        if edge.landings.is_empty() {
            return Ok(());
        }
        let using = &frame.bindings[binding];
        let pos = using.pos.clone();
        let exit_block = self.new_state_block(Vec::new(), Some("using.exit".to_string()), &[]);
        for (landing, snapshot) in edge.landings {
            self.restore_bindings(&snapshot);
            self.current = Some(landing);
            self.emit(
                l::InstructionKind::ExceptionPark,
                Vec::new(),
                None,
                false,
                Vec::new(),
                pos.clone(),
            )?;
            let target = self.block_target(exit_block, Vec::new())?;
            self.terminate(l::Terminator::Branch(target), &pos)?;
        }
        self.enter_block(exit_block)?;
        self.handlers
            .push(HandlerFrame::without_binding(self.scopes.len()));
        let using_depth = self.usings.len();
        self.usings.push(UsingFrame {
            bindings: vec![using.clone()],
            finalizer: None,
            generator: None,
            finalizer_pos: pos.clone(),
            environment: self.scopes.clone(),
            storage: vec![frame.storage[binding]],
            handler_depth: self.handlers.len(),
            scope_depth: self.scopes.len(),
        });
        let lowered = self.exit_actions(self.scopes.len(), using_depth, &pos);
        self.usings.pop();
        let hook_frame = self
            .handlers
            .pop()
            .ok_or_else(|| self.error(&pos, "the handler stack lost its exit hook"))?;
        lowered?;
        let end = self.current.take();
        for (landing, _) in hook_frame.landings {
            self.current = Some(landing);
            self.emit(
                l::InstructionKind::CatchEntry,
                Vec::new(),
                None,
                false,
                Vec::new(),
                pos.clone(),
            )?;
            let trap = l::Trap {
                kind: l::TrapKind::DisposeRaisedDuringExit,
                pos: pos.clone(),
            };
            self.terminate(l::Terminator::Trap(trap), &pos)?;
        }
        self.current = end;
        if self.current.is_some() {
            self.emit(
                l::InstructionKind::ExceptionResume,
                Vec::new(),
                None,
                false,
                vec![l::Trap {
                    kind: l::TrapKind::Raise(l::RaiseEdge::Propagate),
                    pos: pos.clone(),
                }],
                pos.clone(),
            )?;
            self.terminate(l::Terminator::Unreachable { pos: pos.clone() }, &pos)?;
        }
        Ok(())
    }

    pub(super) fn begin_generator_consumer(&mut self, holder: BindingId, pos: &Pos) {
        let handler_depth = self.handlers.len();
        self.handlers
            .push(HandlerFrame::without_binding(self.scopes.len()));
        self.usings.push(UsingFrame {
            bindings: Vec::new(),
            finalizer: None,
            generator: Some(holder),
            finalizer_pos: pos.clone(),
            environment: self.scopes.clone(),
            storage: Vec::new(),
            handler_depth,
            scope_depth: self.scopes.len(),
        });
    }

    pub(super) fn end_generator_consumer(&mut self, pos: &Pos) -> Result<(), LowerError> {
        let frame = self
            .usings
            .pop()
            .ok_or_else(|| self.error(pos, "generator consumer frame is missing"))?;
        let handler = self
            .handlers
            .pop()
            .ok_or_else(|| self.error(pos, "generator consumer handler is missing"))?;
        let end = self.current;
        let state = self.binding_snapshot();
        self.lower_finalizer_exception(&frame, handler, pos)?;
        self.restore_bindings(&state);
        self.current = end;
        Ok(())
    }

    /// Whether every `using` node has ended. [`FunctionBuilder::finish`]
    /// requires it, so no node can lose its exits.
    pub(super) fn finalizers_pending(&self) -> bool {
        self.usings.iter().any(|frame| frame.finalizer.is_some())
    }

    pub(super) fn usings_closed(&self) -> bool {
        self.usings.is_empty()
    }
}
