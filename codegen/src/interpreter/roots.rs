//! Collection walks interpreter frames at their current LIR positions.

use super::*;

pub(super) struct Snapshot {
    values: Vec<Value>,
    locals: Vec<Slot>,
}

impl Interpreter<'_> {
    fn root_frame(
        &self,
        frame: &Frame,
        finished: bool,
        words: &mut Vec<usize>,
        coroutines: &mut Vec<Rc<RefCell<Coroutine>>>,
    ) -> Result<(), InterpretError> {
        if finished {
            return Ok(());
        }
        let function = self.function(frame.function)?;
        let mut cache = self.root_interference.borrow_mut();
        let live = match cache.entry(frame.function) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => entry.insert(
                crate::root_storage::value_interference(function)
                    .map_err(|message| self.invalid(Some(function.pos.clone()), message))?,
            ),
        };
        for (index, value) in frame.values.iter().enumerate() {
            if frame.frame_lifetime.contains(&l::ValueId(index as u32))
                || live.live_before(l::ValueId(index as u32), frame.block, frame.instruction)
            {
                if let Some(value) = value {
                    root_value(value, words, coroutines);
                }
            }
        }
        for local in &frame.locals {
            if local.poisoned_at.is_none() {
                root_value(&local.slot().borrow(), words, coroutines);
            }
        }
        if let Some(value) = &frame.resume {
            root_value(value, words, coroutines);
        }
        if let Some((object, _, _)) = &frame.delivered {
            words.push(*object);
        }
        Ok(())
    }

    /// A collection poll (`compiler.md` §342 rule 7). If the pending flag is
    /// set and the Context has no trap, it runs one automatic collection of
    /// the interpreter's roots. The frame of the poll is at the poll, so its
    /// live values are roots.
    pub(super) fn collection_poll(&mut self, pos: &Pos) -> Result<(), InterpretError> {
        if !self.context.collection_pending() || self.context.trapped() {
            return Ok(());
        }
        Context::automatic_collection_with(
            self,
            |owner| &mut owner.context,
            |owner| owner.full_collection(pos),
        )
    }

    /// The explicit collection at `pos` (`compiler.md` §342 rule 13). It
    /// reports the failed task with the lowest task id, of the failures that
    /// it finds and the failures that an automatic collection recorded, at
    /// `pos`.
    pub(super) fn collect_interpreter(&mut self, pos: &Pos) -> Result<(), InterpretError> {
        let trapped = self.context.trapped();
        Context::explicit_collection_with(
            self,
            |owner| &mut owner.context,
            |owner| owner.full_collection(pos),
        )?;
        match self.context.trap_record() {
            Some(trap) if !trapped && trap.kind == RuntimeTrapKind::UncaughtException => {
                Err(InterpretError::Trap {
                    kind: trap.kind.rule().to_string(),
                    runtime_kind: Some(trap.kind),
                    pos: pos.clone(),
                    message: trap.message.clone(),
                })
            }
            _ => self.check_runtime(pos),
        }
    }

    /// One full collection of the interpreter's roots. The Context records
    /// the failure of each released failed unobserved task (`compiler.md`
    /// §342 rule 12); the caller states the report.
    pub(super) fn full_collection(&mut self, pos: &Pos) -> Result<(), InterpretError> {
        if self.context.live_count() == 0 && self.generator_handles.borrow().is_empty() {
            self.context.collect();
            return self.check_runtime(pos);
        }
        let mut words = Vec::new();
        let mut coroutines = Vec::new();
        for global in &self.globals {
            root_value(&global.borrow(), &mut words, &mut coroutines);
        }
        for &frame in &self.active_frames {
            // SAFETY: execute_frame registers its frame for the call's duration.
            // Execution uses the registered raw pointer. No frame reference spans a call.
            self.root_frame(unsafe { &*frame }, false, &mut words, &mut coroutines)?;
        }
        for snapshot in &self.active_roots {
            root_snapshot(snapshot, &mut words, &mut coroutines);
        }
        // compiler.md §348 rules 1 and 5: each async handle is a root, except
        // a completed task. The walk reads a completed task when it reaches
        // it (rule 2).
        let tasks = self.completed_tasks();
        coroutines.extend(
            self.async_handles
                .borrow()
                .iter()
                .filter(|(key, _)| !tasks.contains(*key))
                .map(|(_, coroutine)| Rc::clone(coroutine)),
        );
        // compiler.md §342 rule 12: the Error object of each exception
        // completion is a root, as in the native root set, so a failure that
        // this collection records keeps its Error object. A completed task
        // gives its Error object when the walk reaches it (§348 rule 2), or
        // after the release plan if it ends (§348 rule 4).
        for (key, coroutine) in self.async_handles.borrow().iter() {
            if tasks.contains(key) {
                continue;
            }
            if let Ok(state) = coroutine.try_borrow() {
                if let Some(Completion::Exception(payload)) = &state.completion {
                    words.push(payload.exception.0);
                }
            }
        }

        coroutines.extend(self.async_ready.iter().filter_map(AsyncJob::handle));
        coroutines.extend(self.async_parked.iter().cloned());
        coroutines.extend(self.async_stopped.iter().cloned());
        for job in &self.async_ready {
            if let AsyncJob::Group { group, .. } = job {
                group.borrow().root_values(&mut words, &mut coroutines);
            }
        }
        for group in self.task_groups.values() {
            group.borrow().root_values(&mut words, &mut coroutines);
        }
        if let Some((object, _, _)) = &self.caught {
            words.push(*object);
        }
        words.extend(self.parked.iter().map(|(object, _, _)| *object));
        let mut walk = Walk {
            words,
            coroutines,
            seen: std::collections::HashSet::new(),
            scanned: std::collections::HashSet::new(),
            reached: std::collections::HashSet::new(),
        };
        self.walk(&mut walk, &tasks)?;
        let ending = self.root_tasks_with_outside_holders(&mut walk, &tasks)?;
        let failures: Vec<usize> = ending
            .iter()
            .filter_map(|key| {
                let coroutine = self.async_handles.borrow().get(key).cloned()?;
                let state = coroutine.borrow();
                match &state.completion {
                    Some(Completion::Exception(payload)) if !payload.observed => {
                        Some(payload.exception.0)
                    }
                    _ => None,
                }
            })
            .collect();
        let Walk { words, seen, .. } = walk;
        let unreachable_generators = self
            .generator_handles
            .borrow()
            .keys()
            .filter(|key| !seen.contains(key))
            .copied()
            .collect::<Vec<_>>();
        unsafe {
            // SAFETY: the slot storage stays valid until the matching shadow pop (section 263).
            self.context
                .shadow_push(words.as_ptr() as usize, words.len())
        };
        let first = Context::with_collection_after_task_failures(
            self,
            |owner| &mut owner.context,
            &failures,
            |owner, handles| {
                owner.context.shadow_pop();
                let mut handles = handles.to_vec();
                let mut storage = Vec::new();
                for key in &unreachable_generators {
                    if let Err(error) = owner.sweep_generator(*key, &mut handles, &mut storage, pos)
                    {
                        return Some(error);
                    }
                }
                handles.sort_unstable_by_key(|key| {
                    owner
                        .async_handles
                        .borrow()
                        .get(key)
                        .map_or(0, |frame| frame.borrow().task_id)
                });
                let mut first = None;
                for &key in handles.iter() {
                    let coroutine = owner
                        .async_handles
                        .borrow()
                        .get(&key)
                        .cloned()
                        .or_else(|| owner.generator_handles.borrow().get(&key).cloned());
                    let result = if let Some(coroutine) = coroutine {
                        owner.release_coroutine(&coroutine, pos)
                    } else if key == 0 || unreachable_generators.contains(&key) {
                        Ok(())
                    } else {
                        Err(owner.invalid(Some(pos.clone()), "unknown packed async handle"))
                    };
                    if let Err(mut error) = result {
                        if let InterpretError::Trap {
                            pos: trap_pos,
                            runtime_kind: Some(RuntimeTrapKind::UncaughtException),
                            ..
                        } = &mut error
                        {
                            *trap_pos = pos.clone();
                        }
                        if first.is_none() {
                            first = Some(error);
                        }
                    }
                }
                for address in storage {
                    if owner.context.is_live(address) {
                        owner.context.delete(address, 0);
                    }
                }
                // compiler.md §348 rule 4: the release ended each task that
                // the walk did not reach. A task that is still registered
                // lost fewer counts than its holders in unreachable objects,
                // which is an internal defect.
                let unended = ending
                    .iter()
                    .any(|key| owner.async_handles.borrow().contains_key(key));
                if unended && first.is_none() {
                    first = Some(owner.invalid(
                        Some(pos.clone()),
                        "a completed task outlived the release of its unreachable holders",
                    ));
                }
                first
            },
        );
        if let Some(InterpretError::Trap {
            runtime_kind: Some(kind),
            message,
            ..
        }) = &first
        {
            self.context.trap(*kind, message.clone(), 0);
        }
        first.map_or_else(|| self.check_runtime(pos), Err)
    }

    /// The completed tasks of the handle table (`compiler.md` §348 terms):
    /// a call of an async function with a completion, no waiter, no
    /// scheduler state, and at least one owner. A host-kicked root has the
    /// host as an outside holder, so it stays a root.
    fn completed_tasks(&self) -> std::collections::HashSet<usize> {
        let mut tasks: std::collections::HashSet<usize> = self
            .async_handles
            .borrow()
            .iter()
            .filter(|(_, coroutine)| {
                let Ok(state) = coroutine.try_borrow() else {
                    return false;
                };
                let CoroutineKind::Invocation(frame) = &state.kind else {
                    return false;
                };
                // An active frame holds a mutable borrow of its frame.
                let Ok(frame) = frame.try_borrow() else {
                    return false;
                };
                !self.module.functions[frame.function.0 as usize].is_generator
                    && state.completed
                    && state.completion.is_some()
                    && state.waiters.is_empty()
                    && state.awaiting.is_none()
                    && state.owners != 0
                    && !state.host_root
            })
            .map(|(key, _)| *key)
            .collect();
        if tasks.is_empty() {
            return tasks;
        }
        // §348 terms: a frame in a scheduler state is not a completed task.
        let key = |coroutine: &Rc<RefCell<Coroutine>>| Rc::as_ptr(coroutine) as usize;
        for coroutine in self.async_ready.iter().filter_map(AsyncJob::handle) {
            tasks.remove(&key(&coroutine));
        }
        for coroutine in self.async_parked.iter().chain(&self.async_stopped) {
            tasks.remove(&key(coroutine));
        }
        for coroutine in self.async_handles.borrow().values() {
            if let Ok(state) = coroutine.try_borrow() {
                for waiter in state.waiters.iter().filter_map(AsyncJob::handle) {
                    tasks.remove(&key(&waiter));
                }
            }
        }
        tasks
    }

    /// Walks the coroutines and the heap words of `walk` until no new
    /// coroutine is reached. A generator or a completed task that a heap
    /// word reaches joins the walk (§106.3 rule 5, `compiler.md` §348 rule
    /// 2), and the walk reads the completion of each completed task that
    /// it reaches.
    fn walk(
        &self,
        walk: &mut Walk,
        tasks: &std::collections::HashSet<usize>,
    ) -> Result<(), InterpretError> {
        loop {
            while let Some(coroutine) = walk.coroutines.pop() {
                if !walk.seen.insert(Rc::as_ptr(&coroutine) as usize) {
                    continue;
                }
                let state = coroutine.borrow();
                let (words, coroutines) = (&mut walk.words, &mut walk.coroutines);
                if let Some(completion) = &state.completion {
                    match completion {
                        Completion::Value(value) => root_value(value, words, coroutines),
                        Completion::Exception(payload) => words.push(payload.exception.0),
                    }
                }
                if let Some(awaited) = &state.awaiting {
                    coroutines.push(Rc::clone(&awaited.handle));
                }
                coroutines.extend(state.waiters.iter().filter_map(AsyncJob::handle));
                match &state.kind {
                    CoroutineKind::Invocation(frame) => {
                        // Active frames hold a mutable RefCell borrow and appear in active_frames.
                        if let Ok(frame) = frame.try_borrow() {
                            self.root_frame(&frame, state.completed, words, coroutines)?;
                        }
                    }
                    CoroutineKind::Aggregate(aggregate) => aggregate.root_values(words, coroutines),
                    CoroutineKind::GroupJoin(group) => {
                        group.borrow().root_values(words, coroutines)
                    }
                }
            }
            if self.generator_handles.borrow().is_empty()
                && tasks.iter().all(|key| walk.seen.contains(key))
            {
                break;
            }
            let reachable = self.context.reachable_words(&walk.words);
            for &key in &reachable {
                if walk.scanned.insert(key) && !walk.seen.contains(&key) {
                    if let Some(generator) = self.generator_handles.borrow().get(&key) {
                        walk.coroutines.push(Rc::clone(generator));
                    } else if tasks.contains(&key) {
                        if let Some(task) = self.async_handles.borrow().get(&key) {
                            walk.coroutines.push(Rc::clone(task));
                        }
                    }
                }
            }
            walk.reached = reachable.into_iter().collect();
            if walk.coroutines.is_empty() {
                break;
            }
        }
        Ok(())
    }

    /// `compiler.md` §348 rule 3, in one pass over the completed tasks that
    /// the walk did not reach, in the order of their task ids. A task whose
    /// owners are more than its heap holders in unreachable objects joins
    /// the walk, and the walk continues from it before the pass reads the
    /// next task. The holders are counted once, before the pass: a holder
    /// that the continued walk reaches is read by the walk, so the walk
    /// reaches its task also, and the pass skips each task that the walk
    /// reached. Returns the tasks that the walk did not reach, which end in
    /// this collection (rule 4).
    fn root_tasks_with_outside_holders(
        &self,
        walk: &mut Walk,
        tasks: &std::collections::HashSet<usize>,
    ) -> Result<Vec<usize>, InterpretError> {
        let unreached = |walk: &Walk| -> Vec<(u64, usize)> {
            let handles = self.async_handles.borrow();
            let mut unreached: Vec<(u64, usize)> = tasks
                .iter()
                .filter(|key| !walk.seen.contains(*key))
                .filter_map(|key| Some((handles.get(key)?.borrow().task_id, *key)))
                .collect();
            unreached.sort_unstable();
            unreached
        };
        let pass = unreached(walk);
        if pass.is_empty() {
            return Ok(Vec::new());
        }
        let candidates = pass.iter().map(|(_, key)| *key).collect();
        let holders = self
            .context
            .completed_task_holders(&walk.reached, &candidates);
        for (_, key) in pass {
            if walk.seen.contains(&key) {
                continue;
            }
            let Some(task) = self.async_handles.borrow().get(&key).cloned() else {
                continue;
            };
            if holders.get(&key).copied().unwrap_or(0) < task.borrow().owners {
                walk.coroutines.push(task);
                self.walk(walk, tasks)?;
            }
        }
        Ok(unreached(walk).into_iter().map(|(_, key)| key).collect())
    }

    pub(super) fn describe_interpreter_map(
        &mut self,
        map: *mut u8,
        value: &Type,
        pos: &Pos,
    ) -> Result<(), InterpretError> {
        if value.counted_type().is_none() {
            return Ok(());
        }
        let layouts = &self.layouts;
        let description = crate::counted::description(layouts, value)
            .map_err(|message| self.invalid(Some(pos.clone()), message))?;
        // SAFETY: the checked Map value type supplies the resolved release layout.
        unsafe { self.context.describe_map(map as usize, &description) };
        Ok(())
    }
}

/// The state of one collection walk of the interpreter's roots.
struct Walk {
    /// The heap words that the roots and the reached coroutines give.
    words: Vec<usize>,
    /// The coroutines that wait for the walk.
    coroutines: Vec<Rc<RefCell<Coroutine>>>,
    /// The coroutines that the walk read.
    seen: std::collections::HashSet<usize>,
    /// The heap words that the walk compared with the handle tables.
    scanned: std::collections::HashSet<usize>,
    /// The words that the last heap walk reached.
    reached: std::collections::HashSet<usize>,
}

fn root_snapshot(
    snapshot: &Snapshot,
    words: &mut Vec<usize>,
    coroutines: &mut Vec<Rc<RefCell<Coroutine>>>,
) {
    for value in &snapshot.values {
        root_value(value, words, coroutines);
    }
    for slot in &snapshot.locals {
        root_value(&slot.borrow(), words, coroutines);
    }
}

fn root_value(value: &Value, words: &mut Vec<usize>, coroutines: &mut Vec<Rc<RefCell<Coroutine>>>) {
    match value {
        Value::Handle(handle) => words.push(*handle as usize),
        Value::Blob(bytes) => words.extend(
            bytes
                .chunks_exact(8)
                .map(|word| usize::from_ne_bytes(word.try_into().unwrap_or([0; 8]))),
        ),
        Value::Callable(callable) => {
            for capture in &callable.captures {
                root_value(capture, words, coroutines);
            }
        }
        Value::Coroutine(coroutine) => coroutines.push(Rc::clone(coroutine)),
        Value::Iterator(cursor) => root_value(&cursor.subject, words, coroutines),
        Value::Address(address) => match &address.target {
            AddressTarget::Slot(slot) | AddressTarget::SlotBytes { slot, .. } => {
                root_value(&slot.borrow(), words, coroutines)
            }
            AddressTarget::Pointer(pointer) => words.push(*pointer as usize),
        },
        _ => {}
    }
}

impl Interpreter<'_> {
    pub(super) fn hold_temporary(&mut self, value: Value) {
        if let Some(snapshot) = self.active_roots.last_mut() {
            snapshot.values.push(value);
        }
    }
}

impl Snapshot {
    pub(super) fn temporaries(values: &[Value]) -> Self {
        Self {
            values: values.to_vec(),
            locals: Vec::new(),
        }
    }

    pub(super) fn call(callable: &Rc<Callable>, arguments: &[Value]) -> Self {
        Self {
            values: std::iter::once(Value::Callable(Rc::clone(callable)))
                .chain(arguments.iter().cloned())
                .collect(),
            locals: Vec::new(),
        }
    }
}

/// Parameters, function environments, and stable targets belong to an unfinished coroutine.
pub(super) fn frame_lifetime_values(
    function: &l::Function,
) -> std::collections::BTreeSet<l::ValueId> {
    if !function.is_async && !function.is_generator {
        return Default::default();
    }
    let mut values = crate::root_storage::stable_values(function);
    values.extend(function.parameters.iter().map(|parameter| parameter.value));
    values.extend(function.values.iter().filter_map(|value| {
        matches!(&value.ty, l::ValueType::Data(ty) if ty.function_type().is_some())
            .then_some(value.id)
    }));
    values
}

/// Completion owns its result separately from the frame storage.
pub(super) fn clear_finished_frame(frame: &mut Frame) {
    frame.generator_owners.clear();
    frame.values.fill(None);
    for local in &frame.locals {
        *local.slot().borrow_mut() = Value::Void;
    }
    frame.resume = None;
    frame.delivered = None;
}
