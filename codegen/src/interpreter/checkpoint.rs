use super::*;

impl Interpreter<'_> {
    /// One host checkpoint (§94.1 rules 8 and 9). It appends the whole
    /// pre-existing parked list after the jobs that are already ready, then
    /// drains the ready queue in FIFO order. A frame parked during the drain
    /// waits for the next checkpoint.
    pub(super) fn async_step(&mut self) -> Result<(), InterpretError> {
        self.async_drain(None).map(|_| ())
    }

    #[cfg(test)]
    pub(super) fn async_step_budget(
        &mut self,
        max: u64,
    ) -> Result<subscript_runtime::AsyncStepReport, InterpretError> {
        self.async_drain(Some(max))
    }

    fn async_drain(
        &mut self,
        limit: Option<u64>,
    ) -> Result<subscript_runtime::AsyncStepReport, InterpretError> {
        let mut dispatched = 0;
        if let Some(error) = &self.async_trapping {
            return Err(error.clone());
        }
        if self.context.trapped() || limit == Some(0) {
            return Ok(self.async_report(dispatched, limit));
        }
        let parked = std::mem::take(&mut self.async_parked);
        self.async_ready
            .extend(parked.into_iter().map(AsyncJob::Invocation));
        while limit != Some(dispatched) {
            let Some(frame) = self.async_ready.pop_front() else {
                break;
            };
            dispatched += 1;
            let outcome = match &frame {
                AsyncJob::Invocation(handle) => self.async_resume(handle),
                AsyncJob::Group { group, index } => self.task_group_react(group, *index),
                AsyncJob::Aggregate { handle, index } => self.async_all_react(handle, *index),
            };
            if let Err(error) = outcome {
                if matches!(error, InterpretError::Trap { .. }) {
                    self.async_trapping = Some(error.clone());
                    self.async_ready.push_front(frame);
                }
                return Err(error);
            }
            if self.context.trapped() {
                // The trap policy preserves the trapping registration and
                // every entry this checkpoint has not reached.
                self.async_ready.push_front(frame);
                break;
            }
        }
        Ok(self.async_report(dispatched, limit))
    }

    fn async_report(
        &self,
        dispatched: u64,
        limit: Option<u64>,
    ) -> subscript_runtime::AsyncStepReport {
        let mut report = subscript_runtime::AsyncStepReport::default();
        report.dispatched = dispatched;
        report.pending = self.async_pending() as u64;
        if limit.is_some() {
            let mut work: Vec<_> = self.async_handles.borrow().values().cloned().collect();
            work.extend(self.async_ready.iter().filter_map(AsyncJob::handle));
            work.extend(self.async_parked.iter().cloned());
            work.extend(self.async_stopped.iter().cloned());
            let mut seen = std::collections::HashSet::new();
            while let Some(handle) = work.pop() {
                if !seen.insert(Rc::as_ptr(&handle) as usize) {
                    continue;
                }
                let state = handle.borrow();
                if state.completion.is_none() && matches!(state.kind, CoroutineKind::Invocation(_))
                {
                    report.unfinished += 1;
                }
                work.extend(state.waiters.iter().filter_map(AsyncJob::handle));
                if let Some(awaited) = &state.awaiting {
                    work.push(Rc::clone(&awaited.handle));
                }
                if let CoroutineKind::Aggregate(aggregate) = &state.kind {
                    work.extend(aggregate.inputs.iter().flatten().cloned());
                }
            }
        }
        report.budget_exhausted =
            u64::from(limit == Some(dispatched) && !self.async_ready.is_empty());
        report
    }
}
