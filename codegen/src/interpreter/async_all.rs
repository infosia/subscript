use super::*;

pub(super) enum AsyncJob {
    Invocation(Rc<RefCell<Coroutine>>),
    Group {
        group: Rc<RefCell<task_group::Group>>,
        index: usize,
    },
    Aggregate {
        handle: Rc<RefCell<Coroutine>>,
        index: usize,
    },
}
impl AsyncJob {
    pub(super) fn handle(&self) -> Option<Rc<RefCell<Coroutine>>> {
        match self {
            Self::Invocation(frame) => Some(Rc::clone(frame)),
            Self::Aggregate { handle, .. } => Some(Rc::clone(handle)),
            Self::Group { group, index } => group
                .borrow()
                .inputs
                .get(*index)
                .and_then(|input| input.clone()),
        }
    }
}
pub(super) enum CoroutineKind {
    Invocation(Rc<RefCell<Frame>>),
    Aggregate(Aggregate),
    GroupJoin(Rc<RefCell<task_group::Group>>),
}
impl CoroutineKind {
    pub(super) fn frame(&self) -> Option<Rc<RefCell<Frame>>> {
        match self {
            Self::Invocation(frame) => Some(Rc::clone(frame)),
            Self::Aggregate(_) | Self::GroupJoin(_) => None,
        }
    }
}
pub(super) struct Aggregate {
    /// True for `Promise.race` (`compiler.md` §350); both combinators have
    /// task kind 2 (§350 rule 10).
    pub(super) race: bool,
    pub(super) inputs: Vec<Option<Rc<RefCell<Coroutine>>>>,
    /// The result array of `Promise.all`; null for a race.
    pub(super) result: *mut u8,
    pub(super) element: Type,
    /// The reaction jobs of this aggregate that are queued or registered.
    pub(super) remaining: usize,
    pub(super) reported: bool,
    pub(super) pos: Pos,
}
impl<'m> Interpreter<'m> {
    pub(super) fn async_all(
        &mut self,
        jobs: *mut u8,
        element: &Type,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let inputs = self.aggregate_inputs(jobs, element, pos)?;
        let len = inputs.len();
        let size = if *element == Type::Void {
            0
        } else {
            self.layout_cached(element)
                .ok_or_else(|| self.invalid(Some(pos.clone()), "aggregate result has no layout"))?
                .size
        };
        // compiler.md §344 rule 4: the results take the fact of their type.
        let fact = self.array_elements(element);
        let result = self.context.array_with_capacity_of(len, size, fact, 0);
        self.check_runtime(pos)?;

        // The runtime header has the same length and capacity ABI as other interpreter arrays.
        unsafe {
            result.cast::<u64>().write(len as u64);
        }
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
                race: false,
                inputs: inputs.clone(),
                result,
                element: element.clone(),
                remaining: len,
                reported: false,
                pos: pos.clone(),
            }),
            completed: len == 0,
            completion: if len == 0 {
                Some(Completion::Value(Value::Handle(result)))
            } else {
                None
            },
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
        for (index, input) in inputs.into_iter().flatten().enumerate() {
            let job = AsyncJob::Aggregate {
                handle: Rc::clone(&handle),
                index,
            };
            if input.borrow().completed {
                self.async_ready.push_back(job);
            } else {
                input.borrow_mut().waiters.push(job);
            }
        }
        Ok(Value::Coroutine(handle))
    }

    /// Reads the handles of an aggregate input array, and takes one count
    /// of each element, duplicates included (§166, §350 rule 12).
    pub(super) fn aggregate_inputs(
        &mut self,
        jobs: *mut u8,
        element: &Type,
        pos: &Pos,
    ) -> Result<Vec<Option<Rc<RefCell<Coroutine>>>>, InterpretError> {
        let len = unsafe { self.context.array_len(jobs) } as usize;
        let mut inputs = Vec::with_capacity(len);
        for index in 0..len {
            let address = unsafe {
                self.context
                    .array_data(jobs)
                    .add(index * std::mem::size_of::<usize>())
            };
            let bytes =
                unsafe { std::slice::from_raw_parts(address, std::mem::size_of::<usize>()) };
            let Value::Coroutine(input) =
                self.unpack(&Type::AsyncHandle(Box::new(element.clone())), bytes)?
            else {
                return Err(
                    self.invalid(Some(pos.clone()), "aggregate input is not an async handle")
                );
            };
            {
                let mut state = input.borrow_mut();
                state.owners = state.owners.saturating_add(1);
            }
            inputs.push(Some(input));
        }
        Ok(inputs)
    }

    pub(super) fn async_all_react(
        &mut self,
        handle: &Rc<RefCell<Coroutine>>,
        index: usize,
    ) -> Result<(), InterpretError> {
        let (input, result, element, last, pos, unsettled) = {
            let mut state = handle.borrow_mut();
            let unsettled = state.completion.is_none();
            let CoroutineKind::Aggregate(aggregate) = &mut state.kind else {
                return Err(self.invalid(None, "reaction targets an invocation"));
            };
            let input = aggregate
                .inputs
                .get_mut(index)
                .and_then(Option::take)
                .ok_or_else(|| {
                    self.invalid(Some(aggregate.pos.clone()), "reaction has no unread input")
                })?;
            aggregate.remaining -= 1;
            (
                input,
                aggregate.result,
                aggregate.element.clone(),
                aggregate.remaining == 0,
                aggregate.pos.clone(),
                unsettled,
            )
        };
        let completion = match input.borrow_mut().completion.as_mut() {
            Some(Completion::Value(value)) => Ok(value.clone()),
            Some(Completion::Exception(payload)) => {
                payload.observed = true;
                Err(payload.exception.clone())
            }
            None => return Err(self.invalid(Some(pos.clone()), "async resume without completion")),
        };
        let completion = match completion {
            Ok(value) => {
                if unsettled && element != Type::Void {
                    let bytes = self.pack(&element, &value)?;
                    let data = unsafe { self.context.array_data(result).add(index * bytes.len()) };
                    unsafe {
                        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast_mut(), bytes.len());
                    }
                }
                if last && unsettled {
                    Some(Completion::Value(Value::Handle(result)))
                } else {
                    None
                }
            }
            Err(exception) if unsettled => {
                Some(Completion::Exception(Box::new(ExceptionCompletion {
                    exception,
                    observed: false,
                })))
            }
            Err(_) => None,
        };
        if let Some(completion) = completion {
            let waiters = {
                let mut state = handle.borrow_mut();
                let completion = observed_by_aggregate(completion, state.observed_by_aggregate);
                state.completed = true;
                state.completion = Some(completion);
                std::mem::take(&mut state.waiters)
            };
            self.async_ready.extend(waiters);
        }
        self.release_coroutine(&input, &pos)?;
        if handle.borrow().owners == 0 {
            self.release_coroutine(handle, &pos)?;
        }
        Ok(())
    }
}

impl Aggregate {
    pub(super) fn root_values(
        &self,
        words: &mut Vec<usize>,
        coroutines: &mut Vec<Rc<RefCell<Coroutine>>>,
    ) {
        // A race has no result array (§350 rule 10).
        if !self.result.is_null() {
            words.push(self.result as usize);
        }
        coroutines.extend(self.inputs.iter().flatten().cloned());
    }
}

/// `compiler.md` §350 rule 5: a failure that a frame stores after a race
/// took it as an input is observed.
pub(super) fn observed_by_aggregate(mut completion: Completion, observed: bool) -> Completion {
    if let Completion::Exception(payload) = &mut completion {
        payload.observed |= observed;
    }
    completion
}
