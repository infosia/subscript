//! Lowering for `throw` and `try`/`catch` (`compiler.md` §115).
//!
//! Each raise site in a `try` block gets its own landing block. The
//! landing block has no parameter and starts with the catch entry, so a
//! raise edge carries no argument (§115.6 rule 2). The landing block then
//! branches to the shared handler block with the Error object and the
//! values of the visible mutable bindings at the raise site.
//!
//! The exception edge of a `using` scope is in `using.rs`.

use super::*;

/// One enclosing `try`, or one binding of a `using` scope, while its block
/// lowers.
pub(super) struct HandlerFrame {
    /// The type of the catch binding, when the clause names one.
    binding_type: Option<l::ValueType>,
    /// Each raise site's landing block and the binding values at the site.
    pub(super) landings: Vec<(l::BlockId, Vec<Option<l::Operand>>)>,
}

impl HandlerFrame {
    /// A frame whose landings bind no Error object: the exception edge of
    /// a `using` binding, or the hooks on that edge.
    pub(super) fn without_binding() -> Self {
        Self {
            binding_type: None,
            landings: Vec::new(),
        }
    }
}

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    /// Gives each `Raise` trap its edge: a fresh landing block of the
    /// nearest enclosing `try`, or the propagate exit when none encloses
    /// the site.
    pub(super) fn resolve_raise_edges(&mut self, traps: &mut [l::Trap]) {
        for trap in traps {
            if !matches!(trap.kind, l::TrapKind::Raise(_)) {
                continue;
            }
            let edge = if self.handlers.is_empty() {
                l::RaiseEdge::Propagate
            } else {
                let landing = self.new_block(Vec::new(), Some("catch.landing".to_string()));
                let snapshot = self.binding_snapshot();
                if let Some(frame) = self.handlers.last_mut() {
                    frame.landings.push((landing, snapshot));
                }
                l::RaiseEdge::Handler(landing)
            };
            trap.kind = l::TrapKind::Raise(edge);
        }
    }

    /// `throw value` (§115.2): read the Error's two fields, record the
    /// pending exception, and leave through the raise edge.
    pub(super) fn lower_throw(&mut self, value: &hir::Expr, pos: &Pos) -> Result<(), LowerError> {
        let object = self.require_expr(value)?;
        let mut fields = Vec::with_capacity(2);
        for name in ["name", "message"] {
            let field = self.resolve_field(&value.ty, name, pos)?;
            let loaded = self
                .emit(
                    l::InstructionKind::LoadField(field),
                    vec![object.clone()],
                    Some(l::ValueType::Data(Type::Str)),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "an Error field load produced no value"))?;
            fields.push(loaded);
        }
        let mut operands = vec![object];
        operands.extend(fields);
        self.emit(
            l::InstructionKind::Throw,
            operands,
            None,
            false,
            vec![l::Trap {
                kind: l::TrapKind::Raise(l::RaiseEdge::Propagate),
                pos: pos.clone(),
            }],
            pos.clone(),
        )?;
        self.terminate(l::Terminator::Unreachable { pos: pos.clone() }, pos)
    }

    /// `try { body } catch (binding) { handler }` (§115.3).
    pub(super) fn lower_try(
        &mut self,
        body: &[hir::Stmt],
        binding: Option<&(String, Type)>,
        handler: &[hir::Stmt],
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let binding_type = binding.map(|(_, ty)| l::ValueType::Data(ty.clone()));
        let catch_block = self.new_state_block(
            binding_type.clone().into_iter().collect(),
            Some("catch".to_string()),
            &[],
        );
        self.handlers.push(HandlerFrame {
            binding_type,
            landings: Vec::new(),
        });
        let lowered = self.lower_scoped(body);
        let frame = self
            .handlers
            .pop()
            .ok_or_else(|| self.error(pos, "the handler stack lost its `try`"))?;
        lowered?;
        let body_end = self.current.take();
        let body_state = self.binding_snapshot();

        let reaches_handler = !frame.landings.is_empty();
        for (landing, snapshot) in frame.landings {
            self.restore_bindings(&snapshot);
            self.current = Some(landing);
            let caught = self.emit(
                l::InstructionKind::CatchEntry,
                Vec::new(),
                frame.binding_type.clone(),
                false,
                Vec::new(),
                pos.clone(),
            )?;
            let edge = self.block_target(catch_block, caught.into_iter().collect())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }

        let mut handler_end = None;
        let mut handler_state = body_state.clone();
        if reaches_handler {
            self.enter_block(catch_block)?;
            self.scopes.push(HashMap::new());
            let mut lowered = Ok(());
            if let Some((name, ty)) = binding {
                let value = l::Operand::Value(
                    *self.blocks[catch_block.0 as usize]
                        .parameters
                        .first()
                        .ok_or_else(|| {
                            self.error(pos, "the handler block has no Error parameter")
                        })?,
                );
                lowered = self
                    .declare_binding(
                        name.clone(),
                        l::ValueType::Data(ty.clone()),
                        false,
                        value,
                        pos.clone(),
                        None,
                    )
                    .map(|_| ());
            }
            if lowered.is_ok() {
                lowered = self.lower_statements(handler);
            }
            if lowered.is_ok() && self.current.is_some() {
                self.release_scopes_from(self.scopes.len() - 1, pos)?;
            }
            self.scopes.pop();
            lowered?;
            handler_end = self.current.take();
            handler_state = self.binding_snapshot();
        }

        if body_end.is_none() && handler_end.is_none() {
            self.current = None;
            return Ok(());
        }
        let join = self.new_state_block(Vec::new(), Some("try.join".to_string()), &[]);
        for (end, state) in [(body_end, body_state), (handler_end, handler_state)] {
            let Some(end) = end else { continue };
            self.restore_bindings(&state);
            self.current = Some(end);
            let edge = self.block_target(join, Vec::new())?;
            self.terminate(l::Terminator::Branch(edge), pos)?;
        }
        self.enter_block(join)
    }
}
