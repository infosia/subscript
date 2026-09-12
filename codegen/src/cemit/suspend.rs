//! Suspension points, await registration, and async child handling.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    pub(super) fn emit_suspend(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
    ) -> Result<(), String> {
        let l::Terminator::Suspend {
            kind,
            arguments,
            traps,
            ..
        } = &block.terminator
        else {
            return Err(internal("non-suspend passed to suspend emitter"));
        };
        self.save_suspend_arguments(out, block, arguments)?;
        let state = self.suspend_state(block.id)?;
        match kind {
            l::SuspendKind::Yield(value) => {
                if let Some(value) = value {
                    let ty = data_type(self.value_type(*value)?)?;
                    let _ = writeln!(
                        out,
                        "    *(({}*)coroutine_out) = {};",
                        self.emitter.ctype(ty)?,
                        self.value(*value)
                    );
                }
                let _ = writeln!(out, "    frame->state = {state};");
                self.emit_pop(out);
                out.push_str("    return 0;\n");
            }
            l::SuspendKind::Async => {
                // B1 experiment rule 8: a `Context.suspend()` waiter becomes
                // eligible at the next host checkpoint.
                let park = self.emitter.runtime_call(
                    "void",
                    "subscript_rt_async_park",
                    &["void*".into(), "void*".into()],
                    &["ctx".into(), "frame".into()],
                );
                let _ = writeln!(out, "    {park};");
                let _ = writeln!(out, "    frame->state = {state};");
                self.emit_pop(out);
                out.push_str("    return 0;\n");
            }
            l::SuspendKind::AsyncCall { target, operands } => {
                self.emit_async_child_create(out, block, target, operands, traps)?;
                self.emit_async_child_start(out, block, target)?;
                self.emit_await_registration(out, block, state)?;
            }
            l::SuspendKind::AsyncHandle { handle } => {
                let _ = writeln!(
                    out,
                    "    frame->b{}_child = {};",
                    block.id.0,
                    self.value(*handle)
                );
                self.emit_async_handle_stale_check(out, block)?;
                self.emit_await_registration(out, block, state)?;
            }
        }
        Ok(())
    }

    fn suspend_state(&self, id: l::BlockId) -> Result<u32, String> {
        self.suspend_states
            .get(id.0 as usize)
            .copied()
            .flatten()
            .ok_or_else(|| internal(format!("suspend block {} is missing", id.0)))
    }

    fn save_suspend_arguments(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
        arguments: &[l::Operand],
    ) -> Result<(), String> {
        let l::Terminator::Suspend {
            successor,
            resume_value,
            ..
        } = &block.terminator
        else {
            return Err(internal("save arguments on non-suspend"));
        };
        let destination = &self.function.blocks[successor.0 as usize];
        for (argument, parameter) in arguments.iter().zip(
            destination
                .parameters
                .iter()
                .skip(usize::from(resume_value.is_some())),
        ) {
            let value = self.operand(argument)?;
            let _ = writeln!(
                out,
                "    frame->b{}_v{} = {value};",
                block.id.0, parameter.0
            );
        }
        Ok(())
    }

    pub(super) fn restore_suspend_arguments(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
    ) -> Result<(), String> {
        let l::Terminator::Suspend {
            successor,
            resume_value,
            ..
        } = &block.terminator
        else {
            return Err(internal("restore arguments on non-suspend"));
        };
        let destination = &self.function.blocks[successor.0 as usize];
        for parameter in destination
            .parameters
            .iter()
            .skip(usize::from(resume_value.is_some()))
        {
            let source = format!("frame->b{}_v{}", block.id.0, parameter.0);
            if self.is_function_value(*parameter)? {
                self.assign_function_value(out, *parameter, &source)?;
            } else {
                let _ = writeln!(out, "    {} = {source};", self.value(*parameter));
            }
        }
        Ok(())
    }

    fn emit_async_child_create(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
        target: &l::CallTarget,
        operands: &[l::ValueId],
        traps: &[l::Trap],
    ) -> Result<(), String> {
        let function = match target.kind {
            l::CallTargetKind::Function(function) => function,
            l::CallTargetKind::Method(method) => self.emitter.method_function(method)?,
            ref other => return Err(internal(format!("async target {other:?} is invalid"))),
        };
        for trap in traps {
            match trap.kind {
                l::TrapKind::DevOnlyLifetime => self.consume(trap),
                l::TrapKind::Call => {}
                _ => {
                    return Err(internal(format!(
                        "unexpected async-call trap {:?}",
                        trap.kind
                    )))
                }
            }
        }
        let args = operands
            .iter()
            .map(|id| self.value(*id))
            .collect::<Vec<_>>();
        let separator = if args.is_empty() { "" } else { ", " };
        let _ = writeln!(
            out,
            "    frame->b{}_child = sub_f{}(ctx{separator}{});",
            block.id.0,
            function.0,
            args.join(", ")
        );
        if let Some(trap) = traps.iter().find(|trap| trap.kind == l::TrapKind::Call) {
            self.consume(trap);
            self.emit_pending_check(out);
        }
        Ok(())
    }

    // B1 experiment: an async call runs the callee's prefix at the call.
    fn emit_async_child_start(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
        target: &l::CallTarget,
    ) -> Result<(), String> {
        let function = match target.kind {
            l::CallTargetKind::Function(function) => function,
            l::CallTargetKind::Method(method) => self.emitter.method_function(method)?,
            ref other => return Err(internal(format!("async target {other:?} is invalid"))),
        };
        let handle = format!("frame->b{}_child", block.id.0);
        let (output, size) = if let Some(ty) = &target.return_type {
            let value = self.fresh();
            let _ = writeln!(
                out,
                "    {} {value} = {};",
                self.emitter.value_ctype(ty)?,
                self.emitter.zero(ty)?
            );
            (format!("&{value}"), format!("sizeof({value})"))
        } else {
            ("NULL".into(), "0u".into())
        };
        let done = self.fresh();
        let _ = writeln!(
            out,
            "    uint8_t {done} = sub_f{}_resume(ctx, {handle}, {output});",
            function.0
        );
        self.emit_pending_check(out);
        let complete = self.emitter.runtime_call(
            "void",
            "subscript_rt_async_complete",
            &[
                "void*".into(),
                "void*".into(),
                "const void*".into(),
                "uint64_t".into(),
            ],
            &["ctx".into(), handle, output, size],
        );
        let _ = writeln!(out, "    if ({done}) {complete};");
        Ok(())
    }

    fn emit_async_handle_stale_check(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
    ) -> Result<(), String> {
        let l::Terminator::Suspend { traps, .. } = &block.terminator else {
            return Err(internal("held await source is not a suspension"));
        };
        for trap in traps {
            match trap.kind {
                l::TrapKind::DevReloadOnlyStaleCoroutine => self.consume(trap),
                l::TrapKind::Call => self.consume(trap),
                ref other => return Err(internal(format!("unexpected held-await trap {other:?}"))),
            }
        }
        let _ = out;
        Ok(())
    }

    // B1 experiment rules 3 to 5: the await registers a continuation on the
    // awaited handle and suspends. It never resumes the awaited child.
    fn emit_await_registration(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
        state: u32,
    ) -> Result<(), String> {
        let register = self.emitter.runtime_call(
            "void",
            "subscript_rt_async_await",
            &["void*".into(), "void*".into(), "void*".into()],
            &[
                "ctx".into(),
                "frame".into(),
                format!("frame->b{}_child", block.id.0),
            ],
        );
        let _ = writeln!(out, "    {register};");
        let _ = writeln!(out, "    frame->state = {state};");
        self.emit_pop(out);
        out.push_str("    return 0;\n");
        Ok(())
    }

    // §94.1: a resumed direct await reads the cached completion. A missing
    // result calls async_missing_completion and unwinds with an Internal
    // trap. It never re-registers the continuation.
    pub(super) fn emit_async_child_resume(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
    ) -> Result<(), String> {
        let l::Terminator::Suspend {
            kind: l::SuspendKind::AsyncCall { .. },
            successor,
            resume_value,
            pos,
            ..
        } = &block.terminator
        else {
            return Err(internal("child resume on non-call suspend"));
        };
        let (output, size) = if let Some(value) = resume_value {
            (
                format!("&{}", self.value(*value)),
                format!("sizeof({})", self.value(*value)),
            )
        } else {
            ("NULL".into(), "0u".into())
        };
        self.emit_completion_read(out, block, &output, &size)?;
        self.restore_suspend_arguments(out, block)?;
        let pos = self.emitter.pos_id(pos);
        let release = self.emitter.runtime_call(
            "void",
            "subscript_rt_async_release",
            &["void*".into(), "void*".into(), "uint32_t".into()],
            &[
                "ctx".into(),
                format!("frame->b{}_child", block.id.0),
                format!("{pos}u"),
            ],
        );
        let _ = writeln!(out, "    {release};");
        let _ = writeln!(out, "    frame->b{}_child = NULL;", block.id.0);
        let _ = writeln!(out, "    goto b{};", successor.0);
        Ok(())
    }

    pub(super) fn emit_async_handle_resume(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
    ) -> Result<(), String> {
        let l::Terminator::Suspend {
            kind: l::SuspendKind::AsyncHandle { .. },
            successor,
            resume_value,
            ..
        } = &block.terminator
        else {
            return Err(internal("held async resume on non-handle suspend"));
        };
        let (output, size) = if let Some(value) = resume_value {
            (
                format!("&{}", self.value(*value)),
                format!("sizeof({})", self.value(*value)),
            )
        } else {
            ("NULL".into(), "0u".into())
        };
        self.emit_completion_read(out, block, &output, &size)?;
        self.restore_suspend_arguments(out, block)?;
        let _ = writeln!(out, "    frame->b{}_child = NULL;", block.id.0);
        let _ = writeln!(out, "    goto b{};", successor.0);
        Ok(())
    }

    // The scheduler resumes an await only after its handle completes, so a
    // missing completion is an internal protocol defect (`compiler.md`
    // §94.1). The Context stops; nothing re-registers, polls, or fabricates
    // a result.
    fn emit_completion_read(
        &mut self,
        out: &mut String,
        block: &l::BasicBlock,
        output: &str,
        size: &str,
    ) -> Result<(), String> {
        let l::Terminator::Suspend { pos, .. } = &block.terminator else {
            return Err(internal("completion read on a non-suspend block"));
        };
        let pos = self.emitter.pos_id(pos);
        let handle = format!("frame->b{}_child", block.id.0);
        let cached = self.emitter.runtime_call(
            "uint8_t",
            "subscript_rt_async_result",
            &[
                "const void*".into(),
                "const void*".into(),
                "void*".into(),
                "uint64_t".into(),
            ],
            &[
                "ctx".into(),
                handle.clone(),
                output.to_string(),
                size.to_string(),
            ],
        );
        let done = self.fresh();
        let _ = writeln!(out, "    uint8_t {done} = {cached};");
        let missing = self.emitter.runtime_call(
            "void",
            "subscript_rt_async_missing_completion",
            &["void*".into(), "uint32_t".into()],
            &["ctx".into(), format!("{pos}u")],
        );
        let _ = writeln!(out, "    if (!{done}) {{ {missing}; goto unwind; }}");
        Ok(())
    }
}
