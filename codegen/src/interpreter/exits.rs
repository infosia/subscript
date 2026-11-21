//! The host calls of the standard runner and their outermost exits
//! (`compiler.md` §342 rules 12 to 16).
//!
//! The interpreter marks each outermost exit at the point where the native
//! runner marks it: the initializer, the entry call, the kick of the other
//! async exports, and each checkpoint that dispatches. The schedule of rules
//! 14 and 15 stays in the Context; the interpreter gives it the failure
//! state of its own tasks and its own root walk as the collection.

use super::*;

impl Interpreter<'_> {
    pub(super) fn run(&mut self) -> Result<Vec<u8>, InterpretError> {
        self.clear_trap();
        // compiler.md §115.4 item 1: the host entry converts an exception
        // that no handler catches.
        self.run_entries().map_err(InterpretError::settled)
    }

    fn run_entries(&mut self) -> Result<Vec<u8>, InterpretError> {
        // The native runner calls the initializer of each module, so the
        // initializer exit exists with no initializer function too.
        let initializer = self.module.initializer;
        self.script_call(|interpreter| {
            if let Some(initializer) = initializer {
                let _ = interpreter.call_function(initializer, Vec::new())?;
            }
            Ok(())
        })?;
        let entry = self
            .module
            .entry
            .ok_or_else(|| InterpretError::InvalidLir {
                message: "module has no executable entry".to_string(),
                pos: None,
            })?;
        let entry = self.function(entry)?;
        if !entry.parameters.is_empty() {
            return Err(InterpretError::Unsupported {
                reason: "exported main requires host-supplied parameters".to_string(),
            });
        }
        let entry_id = entry.id;
        let entry_pos = entry.pos.clone();
        // §26.3 and §94.1 rule 10: the standard runner kicks `main`, then
        // each other async target in first entry export-site order, and then
        // steps while pending work remains. An export kick starts a body and
        // never drains ready work (rule 7).
        if !self.context.trapped() {
            self.script_call(|interpreter| {
                let result = interpreter.call_function(entry_id, Vec::new())?;
                if let Value::Coroutine(coroutine) = result {
                    interpreter.async_kick(&coroutine)?;
                }
                Ok(())
            })?;
        }
        if !self.context.trapped() {
            self.script_call(|interpreter| {
                for function in interpreter.module.async_roots.clone() {
                    let Value::Coroutine(coroutine) =
                        interpreter.call_function(function, Vec::new())?
                    else {
                        return Err(
                            interpreter.invalid(None, "async export did not create a coroutine")
                        );
                    };
                    interpreter.async_kick(&coroutine)?;
                }
                Ok(())
            })?;
        }
        while !self.context.trapped() && self.async_pending() != 0 {
            self.async_step()?;
        }
        self.check_runtime(&entry_pos)?;
        Ok(self.context.take_stdout())
    }

    /// One host call into script code: the entry mark, `body`, and the
    /// outermost exit (`compiler.md` §342 rule 16).
    pub(super) fn script_call<R>(
        &mut self,
        body: impl FnOnce(&mut Self) -> Result<R, InterpretError>,
    ) -> Result<R, InterpretError> {
        self.context.enter_script();
        let outcome = body(self);
        self.exit_script(outcome)
    }

    /// Marks return from script code through the shared operation of the
    /// Context, and performs the collection work of an outermost exit with
    /// the interpreter's root walk (`compiler.md` §342 rules 8, 14, 15, and
    /// 16). An error of `outcome` ends the run, so its exit performs no
    /// collection work.
    pub(super) fn exit_script<R>(
        &mut self,
        outcome: Result<R, InterpretError>,
    ) -> Result<R, InterpretError> {
        let handles = &self.async_handles;
        let service = self
            .context
            .exit_script_with_tasks(|| has_failed_unobserved_task(handles));
        let value = outcome?;
        let trapped = self.context.trapped();
        let pos = no_script_site();
        let mut failure = None;
        Context::finish_exit_service(
            self,
            |owner| &mut owner.context,
            service,
            |owner| {
                if failure.is_none() {
                    failure = owner.full_collection(&pos).err();
                }
            },
        );
        if let Some(error) = failure {
            return Err(error);
        }
        if !trapped {
            self.reported_failure()?;
        }
        Ok(value)
    }

    /// `compiler.md` §342 rule 15: the detection collection reports the
    /// recorded failure with the lowest task id at the throw position.
    fn reported_failure(&self) -> Result<(), InterpretError> {
        let Some(trap) = self.context.trap_record() else {
            return Ok(());
        };
        let pos = self
            .failure_position(trap.pos_id)
            .unwrap_or_else(no_script_site);
        Err(InterpretError::Trap {
            kind: trap.kind.rule().to_string(),
            runtime_kind: Some(trap.kind),
            pos,
            message: trap.message.clone(),
        })
    }

    /// The position id that the Context records for the `throw` of a
    /// failed task (`compiler.md` §342 rule 12). Id 0 is the reserved entry
    /// of the interpreter's other traps, so the id is the index in the
    /// saved exception positions plus 1.
    pub(super) fn failure_position_id(&mut self, pos: &Pos) -> Result<u32, InterpretError> {
        let index = match self
            .exception_positions
            .iter()
            .position(|saved| saved == pos)
        {
            Some(index) => index,
            None => {
                self.exception_positions.push(pos.clone());
                self.exception_positions.len() - 1
            }
        };
        u32::try_from(index + 1)
            .map_err(|_| self.invalid(Some(pos.clone()), "exception position id overflow"))
    }

    fn failure_position(&self, id: u32) -> Option<Pos> {
        let index = (id as usize).checked_sub(1)?;
        self.exception_positions.get(index).cloned()
    }
}

/// `compiler.md` §342 rule 14a for the interpreter's tasks: a task completed
/// with an exception that no `await` raised, and no release reported it.
fn has_failed_unobserved_task(handles: &RefCell<HashMap<usize, Rc<RefCell<Coroutine>>>>) -> bool {
    handles.borrow().values().any(|coroutine| {
        let Ok(state) = coroutine.try_borrow() else {
            return false;
        };
        let reported =
            matches!(&state.kind, CoroutineKind::Aggregate(aggregate) if aggregate.reported);
        !reported
            && matches!(
                &state.completion,
                Some(Completion::Exception(payload)) if !payload.observed
            )
    })
}
