use super::*;

pub(super) enum AsyncJob {
    Invocation(Rc<RefCell<Coroutine>>),
    Aggregate {
        handle: Rc<RefCell<Coroutine>>,
        index: usize,
    },
}
impl AsyncJob {
    pub(super) fn handle(&self) -> Rc<RefCell<Coroutine>> {
        match self {
            Self::Invocation(frame) => Rc::clone(frame),
            Self::Aggregate { handle, .. } => Rc::clone(handle),
        }
    }
}
pub(super) enum CoroutineKind {
    Invocation(Rc<RefCell<Frame>>),
    Aggregate(Aggregate),
}
impl CoroutineKind {
    pub(super) fn frame(&self) -> Option<Rc<RefCell<Frame>>> {
        match self {
            Self::Invocation(frame) => Some(Rc::clone(frame)),
            Self::Aggregate(_) => None,
        }
    }
}
pub(super) struct Aggregate {
    pub(super) inputs: Vec<Option<Rc<RefCell<Coroutine>>>>,
    result: *mut u8,
    element: Type,
    remaining: usize,
    pub(super) reported: bool,
    pos: Pos,
}
impl<'m> Interpreter<'m> {
    pub(super) fn async_all(
        &mut self,
        jobs: *mut u8,
        element: &Type,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
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
        let size = if *element == Type::Void {
            0
        } else {
            self.layout_cached(element)
                .ok_or_else(|| self.invalid(Some(pos.clone()), "aggregate result has no layout"))?
                .size
        };
        let result = self.context.array_with_capacity(len, size, 0);
        self.check_runtime(pos)?;
        self.root_handle(result);
        // The runtime header has the same length and capacity ABI as other interpreter arrays.
        unsafe {
            result.cast::<u64>().write(len as u64);
        }
        let handle = Rc::new(RefCell::new(Coroutine {
            kind: CoroutineKind::Aggregate(Aggregate {
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
        }));
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
                state.completed = true;
                state.completion = Some(completion);
                std::mem::take(&mut state.waiters)
            };
            self.async_ready.extend(waiters);
        }
        self.release_coroutine(&input)?;
        if handle.borrow().owners == 0 {
            self.release_coroutine(handle)?;
        }
        Ok(())
    }
}
