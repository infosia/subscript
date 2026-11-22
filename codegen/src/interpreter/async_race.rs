//! `Promise.race` in the interpreter (`compiler.md` §350). The interpreter
//! has a queue of its own (§166 rule 14), so this adapter mirrors the
//! runtime race of `subscript_runtime` rule by rule.

use super::async_all::{observed_by_aggregate, Aggregate};
use super::*;

impl Interpreter<'_> {
    /// `Promise.race` over the handle array `jobs`. Returns a handle with one
    /// count, which the caller owns; the caller keeps its own array.
    pub(super) fn async_race(
        &mut self,
        jobs: *mut u8,
        element: &Type,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        // §350 rule 7: an empty array traps at the call.
        // SAFETY: `jobs` is the live `Promise<T>[]` operand of the race
        // instruction, an array that this Context allocated.
        if unsafe { self.context.array_len(jobs) } == 0 {
            let kind = RuntimeTrapKind::PromiseRaceEmpty;
            return Err(InterpretError::Trap {
                kind: kind.rule().into(),
                runtime_kind: Some(kind),
                message: kind.message(None).into_owned(),
                pos: pos.clone(),
            });
        }
        // Rule 12: one count for each element, duplicates included.
        let inputs = self.aggregate_inputs(jobs, element, pos)?;
        let task_id = self.register_task_id()?;
        let handle = Rc::new(RefCell::new(Coroutine {
            task_id,
            #[cfg(test)]
            function_pos: pos.clone(),
            #[cfg(test)]
            create_pos: no_script_site(),
            #[cfg(test)]
            suspension_pos: no_script_site(),
            #[cfg(test)]
            active: false,
            kind: CoroutineKind::Aggregate(Aggregate {
                race: true,
                inputs: inputs.clone(),
                result: std::ptr::null_mut(),
                element: element.clone(),
                remaining: inputs.len(),
                reported: false,
                pos: pos.clone(),
            }),
            completed: false,
            completion: None,
            owners: 1,
            host_root: false,
            waiters: Vec::new(),
            awaiting: None,
            observed_by_aggregate: false,
        }));
        #[cfg(test)]
        self.async_registry
            .borrow_mut()
            .insert(task_id, Rc::downgrade(&handle));
        self.async_handles
            .borrow_mut()
            .insert(Rc::as_ptr(&handle) as usize, Rc::clone(&handle));
        // Rule 4: a job in array order for each input with a completion, a
        // waiter for each other; the race has no completion at the return.
        for (index, input) in inputs.into_iter().flatten().enumerate() {
            let job = AsyncJob::Aggregate {
                handle: Rc::clone(&handle),
                index,
            };
            let mut state = input.borrow_mut();
            // Rule 5: the race observes each input from the call.
            match state.completion.as_mut() {
                Some(Completion::Exception(payload)) => payload.observed = true,
                Some(Completion::Value(_)) => {}
                None => state.observed_by_aggregate = true,
            }
            if state.completed {
                drop(state);
                self.async_ready.push_back(job);
            } else {
                state.waiters.push(job);
            }
        }
        Ok(Value::Coroutine(handle))
    }

    /// One reaction job of a race (§350 rules 3 and 6). The first job that
    /// runs completes the race and detaches it from each other input. A
    /// later job finds its input detached and only ends.
    pub(super) fn async_race_react(
        &mut self,
        handle: &Rc<RefCell<Coroutine>>,
        index: usize,
    ) -> Result<(), InterpretError> {
        let (input, pos, valid) = {
            let mut state = handle.borrow_mut();
            // The release of a race with no holder takes its completion
            // (`release_coroutine`); `completed` stays true.
            let settled = state.completed;
            let CoroutineKind::Aggregate(race) = &mut state.kind else {
                return Err(self.invalid(None, "reaction targets an invocation"));
            };
            let remaining = race.remaining.checked_sub(1);
            race.remaining = remaining.unwrap_or(0);
            let input = race.inputs.get_mut(index).and_then(Option::take);
            // Rule 6: an input that lost has no entry; the detach released it.
            let valid = remaining.is_some() && input.is_some() != settled;
            (input, race.pos.clone(), valid)
        };
        if !valid {
            return Err(self.invalid(Some(pos), "race reaction has no unread input"));
        }
        if let Some(input) = input {
            self.async_race_win(handle, &input, &pos)?;
            self.async_race_detach(handle, &pos)?;
        }
        if handle.borrow().owners == 0 {
            self.release_coroutine(handle, &pos)?;
        }
        Ok(())
    }

    /// Completes the race with the completion of `input` (rule 3), a clone of
    /// the value or the same exception, and releases the count of the race
    /// on `input`.
    fn async_race_win(
        &mut self,
        handle: &Rc<RefCell<Coroutine>>,
        input: &Rc<RefCell<Coroutine>>,
        pos: &Pos,
    ) -> Result<(), InterpretError> {
        let completion = match input.borrow_mut().completion.as_mut() {
            Some(Completion::Value(value)) => Completion::Value(value.clone()),
            Some(Completion::Exception(payload)) => {
                payload.observed = true;
                Completion::Exception(Box::new(ExceptionCompletion {
                    exception: payload.exception.clone(),
                    observed: false,
                }))
            }
            None => return Err(self.invalid(Some(pos.clone()), "async resume without completion")),
        };
        let waiters = {
            let mut state = handle.borrow_mut();
            let completion = observed_by_aggregate(completion, state.observed_by_aggregate);
            state.completed = true;
            state.completion = Some(completion);
            std::mem::take(&mut state.waiters)
        };
        self.async_ready.extend(waiters);
        self.release_coroutine(input, pos)
    }

    /// Rule 6: after the race has its completion, it holds no input. The
    /// detach removes the waiter of the race from each input that has no
    /// completion and releases the count of the race. The queued job of an
    /// input with a completion stays in `remaining` and ends with no read.
    fn async_race_detach(
        &mut self,
        handle: &Rc<RefCell<Coroutine>>,
        pos: &Pos,
    ) -> Result<(), InterpretError> {
        let held: Vec<(usize, Rc<RefCell<Coroutine>>)> = match &mut handle.borrow_mut().kind {
            CoroutineKind::Aggregate(race) => race
                .inputs
                .iter_mut()
                .enumerate()
                .filter_map(|(index, input)| input.take().map(|input| (index, input)))
                .collect(),
            _ => return Ok(()),
        };
        let mut detached = 0;
        let mut first = None;
        for (index, input) in held {
            {
                let mut state = input.borrow_mut();
                if state.completion.is_none() {
                    let position = state.waiters.iter().position(|waiter| {
                        matches!(waiter, AsyncJob::Aggregate { handle: race, index: job }
                            if Rc::ptr_eq(race, handle) && *job == index)
                    });
                    if let Some(position) = position {
                        state.waiters.remove(position);
                        detached += 1;
                    }
                }
            }
            if let Err(error) = self.release_coroutine(&input, pos) {
                first.get_or_insert(error);
            }
        }
        if let CoroutineKind::Aggregate(race) = &mut handle.borrow_mut().kind {
            race.remaining = race.remaining.saturating_sub(detached);
        }
        first.map_or(Ok(()), Err)
    }
}
