//! Raise sites, `throw`, and the catch entry on the ship tier
//! (`compiler.md` §115.6).

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    /// Emits one instruction. At a raise site, every pending-word check
    /// the instruction emits takes its handler edge for a pending
    /// exception, and the instruction must emit at least one check.
    pub(super) fn emit_raise_site(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
    ) -> Result<(), String> {
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
            return self.emit_instruction(out, instruction);
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
            l::RaiseEdge::Handler(block) => Some(block),
            l::RaiseEdge::Propagate => None,
        };
        let before = self.pending_checks.get();
        let emitted = self.emit_instruction(out, &stripped);
        self.raise_target = None;
        emitted?;
        if self.pending_checks.get() == before
            && !self.direct_call_leaves_word_clear(instruction)?
        {
            return Err(internal(format!(
                "raise site {:?} emitted no pending-word check",
                instruction.kind
            )));
        }
        self.consumed_traps.extend(raise);
        Ok(())
    }

    /// Whether `instruction` calls a script function directly that can
    /// neither trap nor raise, so the word stays zero after the call and
    /// the call needs no check.
    fn direct_call_leaves_word_clear(&self, instruction: &l::Instruction) -> Result<bool, String> {
        let l::InstructionKind::Call(target) = &instruction.kind else {
            return Ok(false);
        };
        let function = match &target.kind {
            l::CallTargetKind::Function(function) | l::CallTargetKind::StaticClosure(function) => {
                *function
            }
            l::CallTargetKind::Method(method) => self.emitter.method_function(*method)?,
            _ => return Ok(false),
        };
        Ok(!script_call_requires_pending_check(
            self.emitter.function(function)?,
        ))
    }

    /// `throw`: records the pending exception, then checks the word.
    pub(super) fn emit_throw(
        &mut self,
        out: &mut String,
        instruction: &l::Instruction,
        operands: &[String],
    ) -> Result<(), String> {
        let [object, name, message] = operands else {
            return Err(internal(
                "Throw needs the object, its name, and its message",
            ));
        };
        let pos = self.emitter.pos_id(&instruction.pos);
        let call = self.emitter.runtime_call(
            "void",
            "subscript_rt_exception_throw",
            &[
                "void*".into(),
                "void*".into(),
                "void*".into(),
                "void*".into(),
                "uint32_t".into(),
            ],
            &[
                "ctx".into(),
                object.clone(),
                name.clone(),
                message.clone(),
                format!("{pos}u"),
            ],
        );
        let _ = writeln!(out, "    {call};");
        self.emit_pending_check(out);
        Ok(())
    }

    /// The catch entry: takes the pending Error object and clears the word.
    pub(super) fn emit_catch_entry(
        &mut self,
        out: &mut String,
        result: Option<String>,
    ) -> Result<(), String> {
        let call = self.emitter.runtime_call(
            "void*",
            "subscript_rt_exception_catch",
            &["void*".into()],
            &["ctx".into()],
        );
        match result {
            Some(_) => self.assign(out, result, &call),
            None => {
                let _ = writeln!(out, "    {call};");
                Ok(())
            }
        }
    }

    /// The start of the hooks of an exception exit: parks the pending
    /// exception and clears the word (compiler.md §115.5 rule 7).
    pub(super) fn emit_exception_park(&mut self, out: &mut String) -> Result<(), String> {
        let call = self.emitter.runtime_call(
            "void",
            "subscript_rt_exception_park",
            &["void*".into()],
            &["ctx".into()],
        );
        let _ = writeln!(out, "    {call};");
        Ok(())
    }

    /// The end of the hooks of an exception exit: resumes the parked
    /// exception, then checks the word.
    pub(super) fn emit_exception_resume(&mut self, out: &mut String) -> Result<(), String> {
        let call = self.emitter.runtime_call(
            "void",
            "subscript_rt_exception_resume",
            &["void*".into()],
            &["ctx".into()],
        );
        let _ = writeln!(out, "    {call};");
        self.emit_pending_check(out);
        Ok(())
    }
}
