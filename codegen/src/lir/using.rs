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
pub(super) struct UsingFrame {
    /// The bindings, in declaration order.
    bindings: Vec<hir::UsingBinding>,
    /// For each binding: the binding that holds the resource and the
    /// binding of its active flag, resolved where the node starts.
    storage: Vec<(BindingId, Option<BindingId>)>,
    /// The depth of the handler stack where the node starts.
    handler_depth: usize,
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

    /// The one lowering of `return`, `break`, and `continue`: the hooks of
    /// each `using` node that the edge leaves, innermost first, then the
    /// scope releases, then the terminator.
    pub(super) fn leave(&mut self, edge: Leave, pos: &Pos) -> Result<(), LowerError> {
        match edge {
            Leave::Return(value) => {
                self.run_exit_hooks(0)?;
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
        self.run_exit_hooks(using_depth)?;
        self.release_scopes_from(scope_depth, pos)?;
        let edge = self.block_target(target, Vec::new())?;
        self.terminate(l::Terminator::Branch(edge), pos)
    }

    /// `using` bindings, then `body` (`compiler.md` §115.5 rule 5).
    pub(super) fn lower_using(
        &mut self,
        bindings: &[hir::UsingBinding],
        body: &[hir::Stmt],
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
        for _ in bindings {
            self.handlers.push(HandlerFrame::without_binding());
        }
        self.usings.push(UsingFrame {
            bindings: bindings.to_vec(),
            storage,
            handler_depth,
        });
        let mut lowered = self.lower_scoped(body);
        // §101: no hook where control cannot arrive.
        if lowered.is_ok()
            && self.current.is_some()
            && subscript_compiler::sequence_can_fall_through(body)
        {
            lowered = self.run_exit_hooks(self.usings.len() - 1);
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
        for binding in (0..bindings.len()).rev() {
            let edge = self
                .handlers
                .pop()
                .ok_or_else(|| self.error(pos, "the handler stack lost a `using` binding"))?;
            if self.handlers.len() != frame.handler_depth + binding {
                return Err(self.error(pos, "a `using` node left the handler stack unbalanced"));
            }
            self.lower_exception_edge(&frame, binding, edge)?;
        }
        self.restore_bindings(&body_state);
        self.current = body_end;
        Ok(())
    }

    /// The hooks of the nodes from `first` to the innermost, innermost
    /// first, in reverse declaration order (`compiler.md` §60.1 rule 4).
    fn run_exit_hooks(&mut self, first: usize) -> Result<(), LowerError> {
        for node in (first..self.usings.len()).rev() {
            for binding in (0..self.usings[node].bindings.len()).rev() {
                if self.current.is_none() {
                    return Ok(());
                }
                // A raise in the hook resolves to the handlers that enclose
                // the node and to the edges of the earlier bindings
                // (§115.5 rules 3 and 6).
                let depth = self.usings[node].handler_depth + binding;
                let inner = self.handlers.split_off(depth);
                let lowered = self.lower_hook(node, binding);
                self.handlers.extend(inner);
                lowered?;
            }
        }
        Ok(())
    }

    /// The hook of one binding of the node at `node` on the `using` stack.
    fn lower_hook(&mut self, node: usize, binding: usize) -> Result<(), LowerError> {
        let frame = &self.usings[node];
        let hook = frame.bindings[binding].hook();
        let storage = frame.storage[binding];
        let names = (
            frame.bindings[binding].name.clone(),
            frame.bindings[binding].active.clone(),
        );
        self.lower_hook_statement(&hook, names, storage)
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
        self.handlers.push(HandlerFrame::without_binding());
        let lowered = self.lower_hook_statement(
            &using.hook(),
            (using.name.clone(), using.active.clone()),
            frame.storage[binding],
        );
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

    /// Whether every `using` node has ended. [`FunctionBuilder::finish`]
    /// requires it, so no node can lose its exits.
    pub(super) fn usings_closed(&self) -> bool {
        self.usings.is_empty()
    }
}
