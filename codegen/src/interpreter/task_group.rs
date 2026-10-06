//! The interpreter uses the same group state and tagged reaction order (§170).

use super::*;
use subscript_compiler::hir::TaskGroupOperation as G;

pub(super) struct Group {
    pos: Pos,
    closed: bool,
    pub(super) inputs: Vec<Option<Rc<RefCell<Coroutine>>>>,
    finished: usize,
    failed: usize,
    first: Option<(usize, String, Pos)>,
    pub(super) join: Option<Rc<RefCell<Coroutine>>>,
}

impl Interpreter<'_> {
    pub(super) fn task_group_operation(
        &mut self,
        operation: G,
        args: &[Value],
        pos: &Pos,
    ) -> Result<Option<Value>, InterpretError> {
        if operation == G::Create {
            let id = self.next_group_id;
            self.next_group_id = id
                .checked_add(2)
                .ok_or_else(|| self.invalid(Some(pos.clone()), "task group id exhausted"))?;
            self.task_groups.insert(
                id,
                Rc::new(RefCell::new(Group {
                    pos: pos.clone(),
                    closed: false,
                    inputs: Vec::new(),
                    finished: 0,
                    failed: 0,
                    first: None,
                    join: None,
                })),
            );
            return Ok(Some(Value::Handle(id as *mut u8)));
        }
        let id = args
            .first()
            .ok_or_else(|| self.invalid(Some(pos.clone()), "group operand is missing"))?
            .as_handle()? as usize;
        let group = self
            .task_groups
            .get(&id)
            .cloned()
            .ok_or_else(|| self.invalid(Some(pos.clone()), "task group is not live"))?;
        match operation {
            G::Add => {
                let Some(Value::Coroutine(input)) = args.get(1) else {
                    return Err(self.invalid(Some(pos.clone()), "group input is not a task"));
                };
                if group.borrow().closed {
                    // The closed-group fault owns this operation, including argument release.
                    let _ = self.release_coroutine(input, pos);
                    return Err(group_trap(pos, "add to a closed task group".into()));
                }
                let index = group.borrow().inputs.len();
                group.borrow_mut().inputs.push(Some(Rc::clone(input)));
                let job = AsyncJob::Group { group, index };
                if input.borrow().completed {
                    self.async_ready.push_back(job);
                } else {
                    input.borrow_mut().waiters.push(job);
                }
                Ok(None)
            }
            G::Join => {
                if group.borrow().closed {
                    return Err(group_trap(pos, "second join of a task group".into()));
                }
                group.borrow_mut().closed = true;
                let task_id = self.register_task_id()?;
                let handle = Rc::new(RefCell::new(Coroutine {
                    task_id,
                    #[cfg(test)]
                    function_pos: pos.clone(),
                    #[cfg(test)]
                    create_pos: pos.clone(),
                    #[cfg(test)]
                    suspension_pos: no_script_site(),
                    #[cfg(test)]
                    active: false,
                    kind: CoroutineKind::GroupJoin(Rc::clone(&group)),
                    completed: false,
                    completion: None,
                    owners: 1,
                    host_root: false,
                    waiters: Vec::new(),
                    awaiting: None,
                }));
                self.async_registry
                    .borrow_mut()
                    .insert(task_id, Rc::downgrade(&handle));
                self.async_handles
                    .borrow_mut()
                    .insert(Rc::as_ptr(&handle) as usize, Rc::clone(&handle));
                group.borrow_mut().join = Some(Rc::clone(&handle));
                self.task_group_settle(&group)?;
                Ok(Some(Value::Coroutine(handle)))
            }
            G::Release => {
                self.task_groups.remove(&id);
                let state = group.borrow();
                if !state.closed {
                    let mut unfinished = 0;
                    let mut failed = state.failed;
                    for input in state.inputs.iter().flatten() {
                        match input.borrow().completion.as_ref() {
                            Some(Completion::Value(_)) => {}
                            Some(Completion::Exception(_)) => failed += 1,
                            None => unfinished += 1,
                        }
                    }
                    if unfinished != 0 || failed != 0 {
                        return Err(group_trap(
                            pos,
                            format!(
                                "task group scope ended: {unfinished} unfinished, {failed} failed"
                            ),
                        ));
                    }
                }
                Ok(None)
            }
            _ => Err(self.invalid(Some(pos.clone()), "unknown group operation")),
        }
    }

    pub(super) fn task_group_react(
        &mut self,
        group: &Rc<RefCell<Group>>,
        index: usize,
    ) -> Result<(), InterpretError> {
        let input = group
            .borrow_mut()
            .inputs
            .get_mut(index)
            .and_then(Option::take)
            .ok_or_else(|| self.invalid(None, "group reaction has no input"))?;
        let exception = match input.borrow_mut().completion.as_mut() {
            Some(Completion::Value(_)) => None,
            Some(Completion::Exception(payload)) => {
                payload.observed = true;
                Some(payload.exception.clone())
            }
            None => return Err(self.invalid(None, "async resume without completion")),
        };
        {
            let mut state = group.borrow_mut();
            state.finished += 1;
            if let Some(exception) = exception {
                state.failed += 1;
                if state.first.is_none() {
                    state.first = Some(exception);
                }
            }
        }
        let pos = group.borrow().pos.clone();
        self.release_coroutine(&input, &pos)?;
        self.task_group_settle(group)
    }

    fn task_group_settle(&mut self, group: &Rc<RefCell<Group>>) -> Result<(), InterpretError> {
        let state = group.borrow();
        let pos = state.pos.clone();
        if state.finished != state.inputs.len() {
            return Ok(());
        }
        let Some(handle) = state.join.clone() else {
            return Ok(());
        };
        let completion = match state.first.clone() {
            Some(exception) => Completion::Exception(Box::new(ExceptionCompletion {
                exception,
                observed: false,
            })),
            None => Completion::Value(Value::Void),
        };
        drop(state);
        let mut state = handle.borrow_mut();
        if state.completed {
            return Ok(());
        }
        state.completed = true;
        state.completion = Some(completion);
        self.async_ready.extend(std::mem::take(&mut state.waiters));
        let dropped = state.owners == 0;
        drop(state);
        if dropped {
            self.release_coroutine(&handle, &pos)?;
        }
        Ok(())
    }
}

fn group_trap(pos: &Pos, message: String) -> InterpretError {
    InterpretError::Trap {
        kind: RuntimeTrapKind::TaskGroup.rule().into(),
        runtime_kind: Some(RuntimeTrapKind::TaskGroup),
        message,
        pos: pos.clone(),
    }
}
