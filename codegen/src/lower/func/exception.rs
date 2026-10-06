//! Raise sites, `throw`, and the catch entry on the dev tier
//! (`compiler.md` §115.6).

use super::*;

impl<M: Module> Body<'_, '_, '_, '_, M> {
    /// Lowers one instruction. At a raise site, every pending-word check
    /// the instruction emits takes its handler edge for a pending
    /// exception, and the instruction must emit at least one check.
    pub(super) fn emit_raise_site(&mut self, instruction: &l::Instruction) -> Result<(), String> {
        let Some(edge) = instruction.raise_edge().copied() else {
            // An uncounted quiet completion needs no pending check.
            if matches!(instruction.kind, l::InstructionKind::AwaitRaise)
                && !instruction
                    .traps
                    .iter()
                    .any(|trap| trap.kind == l::TrapKind::Call)
            {
                return Ok(());
            }
            return self.emit_instruction(instruction);
        };
        let (raise, other): (Vec<l::Trap>, Vec<l::Trap>) = instruction
            .traps
            .iter()
            .cloned()
            .partition(|trap| matches!(trap.kind, l::TrapKind::Raise(_)));
        let stripped = l::Instruction {
            traps: other,
            ..instruction.clone()
        };
        self.raise_target = match edge {
            l::RaiseEdge::Handler(block) => Some(
                *self
                    .blocks
                    .get(block.0 as usize)
                    .ok_or_else(|| internal(format!("handler block {} is missing", block.0)))?,
            ),
            l::RaiseEdge::Propagate => None,
        };
        let before = self.pending_checks;
        let emitted = self.emit_instruction(&stripped);
        self.raise_target = None;
        emitted?;
        if self.pending_checks == before {
            return Err(internal(format!(
                "raise site {:?} emitted no pending-word check",
                instruction.kind
            )));
        }
        self.consumed_traps.extend(raise);
        Ok(())
    }

    /// `throw`: records the pending exception, then checks the word.
    pub(super) fn emit_throw(&mut self, operands: &[RV], pos: &Pos) -> Result<(), String> {
        let [object, name, message] = operands else {
            return Err(internal(
                "Throw needs the object, its name, and its message",
            ));
        };
        let object = self.expect_scalar(*object)?;
        let name = self.expect_scalar(*name)?;
        let message = self.expect_scalar(*message)?;
        let position = self.position_id(pos);
        let position = self.iconst(types::I32, position);
        self.call_runtime(
            self.ml.rt.exception_throw,
            &[self.ctx, object, name, message, position],
            true,
        )?;
        Ok(())
    }

    /// The catch entry: takes the pending Error object and clears the word.
    pub(super) fn emit_catch_entry(&mut self, bound: bool) -> Result<Option<RV>, String> {
        let object = self
            .call_runtime(self.ml.rt.exception_catch, &[self.ctx], false)?
            .ok_or_else(|| internal("the catch entry returned no object"))?;
        Ok(bound.then_some(RV::Scalar(object)))
    }
}
