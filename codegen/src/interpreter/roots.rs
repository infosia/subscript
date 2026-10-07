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

    pub(super) fn collect_interpreter(&mut self, pos: &Pos) -> Result<(), InterpretError> {
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
        coroutines.extend(self.async_handles.borrow().values().cloned());

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
        let mut seen = std::collections::HashSet::new();
        let mut scanned = std::collections::HashSet::new();
        loop {
            while let Some(coroutine) = coroutines.pop() {
                if !seen.insert(Rc::as_ptr(&coroutine) as usize) {
                    continue;
                }
                let state = coroutine.borrow();
                if let Some(completion) = &state.completion {
                    match completion {
                        Completion::Value(value) => root_value(value, &mut words, &mut coroutines),
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
                            self.root_frame(&frame, state.completed, &mut words, &mut coroutines)?;
                        }
                    }
                    CoroutineKind::Aggregate(aggregate) => {
                        aggregate.root_values(&mut words, &mut coroutines)
                    }
                    CoroutineKind::GroupJoin(group) => {
                        group.borrow().root_values(&mut words, &mut coroutines)
                    }
                }
            }
            if self.generator_handles.borrow().is_empty() {
                break;
            }
            let reachable = self.context.reachable_words(&words);
            for key in reachable {
                if scanned.insert(key) && !seen.contains(&key) {
                    if let Some(generator) = self.generator_handles.borrow().get(&key) {
                        coroutines.push(Rc::clone(generator));
                    }
                }
            }
            if coroutines.is_empty() {
                break;
            }
        }
        let unreachable_generators = self
            .generator_handles
            .borrow()
            .keys()
            .filter(|key| !seen.contains(key))
            .copied()
            .collect::<Vec<_>>();
        self.context
            .shadow_push(words.as_ptr() as usize, words.len());
        let first = Context::with_collection(
            self,
            |owner| &mut owner.context,
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
