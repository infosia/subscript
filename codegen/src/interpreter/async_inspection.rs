use super::*;

#[cfg(test)]
pub(super) struct TaskInfo {
    pub(super) task_id: u64,
    pub(super) awaited_task_id: u64,
    pub(super) state: u32,
    pub(super) kind: u32,
    pub(super) reserved: u32,
    pub(super) function_pos: Pos,
    pub(super) await_pos: Pos,
    pub(super) create_pos: Pos,
}
impl Interpreter<'_> {
    pub(super) fn register_task_id(&mut self) -> Result<u64, InterpretError> {
        let id = self.next_async_task_id;
        self.next_async_task_id = id
            .checked_add(1)
            .ok_or_else(|| self.invalid(None, "async task id exhausted"))?;
        #[cfg(test)]
        {
            let registry = self.async_registry.get_mut();
            if registry.len() >= self.async_registry_sweep_len.saturating_mul(2).max(64) {
                registry.retain(|_, handle| handle.strong_count() != 0);
                registry.shrink_to_fit();
                self.async_registry_sweep_len = registry.len();
            }
        }
        Ok(id)
    }

    #[cfg(test)]
    pub(super) fn async_tasks(&self) -> Vec<TaskInfo> {
        let handles: Vec<_> = self
            .async_registry
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .collect();
        let mut records = Vec::new();
        for handle in &handles {
            let state = handle.borrow();
            let aggregate = !matches!(state.kind, CoroutineKind::Invocation(_));
            let unread = matches!(&state.kind, CoroutineKind::Aggregate(a) if a.inputs.iter().any(Option::is_some));
            if state.completed && state.owners == 0 && !unread {
                continue;
            }
            let mut awaited_task_id = 0;
            let ready = self
                .async_ready
                .iter()
                .any(|job| matches!(job, AsyncJob::Invocation(f) if Rc::ptr_eq(f, handle)));
            let value =
                if state.completed {
                    5
                } else if aggregate {
                    3
                } else if state.active {
                    4
                } else if ready {
                    1
                } else if self.async_parked.iter().any(|f| Rc::ptr_eq(f, handle)) {
                    2
                } else if self.async_stopped.iter().any(|f| Rc::ptr_eq(f, handle)) {
                    6
                } else {
                    for other in &handles {
                        // A self-wait shares this borrow domain; shared reads remain valid.
                        let other = other.borrow();
                        if other.waiters.iter().any(
                            |job| matches!(job, AsyncJob::Invocation(f) if Rc::ptr_eq(f, handle)),
                        ) {
                            awaited_task_id = other.task_id;
                            break;
                        }
                    }
                    assert_ne!(awaited_task_id, 0, "unclassified async task");
                    3
                };
            records.push(TaskInfo {
                task_id: state.task_id,
                awaited_task_id,
                state: value,
                kind: match state.kind {
                    CoroutineKind::Invocation(_) => 1,
                    CoroutineKind::Aggregate(_) => 2,
                    CoroutineKind::GroupJoin(_) => 3,
                },
                reserved: 0,
                function_pos: state.function_pos.clone(),
                create_pos: state.create_pos.clone(),
                await_pos: if aggregate || value == 4 || value == 5 {
                    no_script_site()
                } else {
                    state.suspension_pos.clone()
                },
            });
        }
        records.sort_unstable_by_key(|t| t.task_id);
        records
    }
}
