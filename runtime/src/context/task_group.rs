//! A group has one lexical owner and reactions registered at add (§170).

use super::async_scheduler::{AsyncKind, RuntimeTask};
use super::*;
use crate::exception::{Completion, ExceptionCompletion, PendingException};

pub(super) struct Group {
    lexical: bool,
    closed: bool,
    inputs: Vec<Option<*mut u8>>,
    pub(super) remaining: usize,
    finished: usize,
    failed: usize,
    first: Option<PendingException>,
    join: Option<*mut u8>,
}

impl Context {
    /// Creates an open group with one lexical owner (§170).
    #[must_use]
    pub fn task_group_create(&mut self, pos_id: u32) -> *mut u8 {
        // The payload is an identity, not a counted coroutine header.
        let handle = self.alloc(8, CLASS_GENERATOR, pos_id);
        if !handle.is_null() {
            self.task_groups.insert(
                handle as usize,
                Group {
                    lexical: true,
                    closed: false,
                    inputs: Vec::new(),
                    remaining: 0,
                    finished: 0,
                    failed: 0,
                    first: None,
                    join: None,
                },
            );
        }
        handle
    }

    /// Moves one task count to a group and registers its reaction (§170 rule 4).
    ///
    /// # Safety
    /// The caller owns one count of a live void task in this Context.
    /// The group has a live lexical owner or a synchronous borrow.
    pub unsafe fn task_group_add(&mut self, group: *mut u8, input: *mut u8, pos_id: u32) {
        let Some(state) = self.task_groups.get_mut(&(group as usize)) else {
            self.trap(TrapKind::Internal, "task group is not live", pos_id);
            return;
        };
        if state.closed {
            // The argument transfers its count even when this call traps.
            self.trap(TrapKind::TaskGroup, "add to a closed task group", pos_id);
            unsafe { self.async_release(input, pos_id) };
            return;
        }
        let index = state.inputs.len();
        state.inputs.push(Some(input));
        state.remaining += 1;
        let job = AsyncJob::Group {
            group: group as usize,
            index,
        };
        match self.async_frames.get_mut(&(input as usize)) {
            Some(meta) if meta.completion.is_none() => meta.waiters.push(job),
            Some(_) => self.async_ready.push_back(job),
            None => self.async_missing_completion(pos_id),
        }
    }

    /// Closes a group and returns its new void join task (§170 rule 5).
    ///
    /// # Safety
    /// The group has a live lexical owner or a synchronous borrow.
    pub unsafe fn task_group_join(&mut self, group: *mut u8, pos_id: u32) -> *mut u8 {
        let Some(state) = self.task_groups.get(&(group as usize)) else {
            self.trap(TrapKind::Internal, "task group is not live", pos_id);
            return std::ptr::null_mut();
        };
        if state.closed {
            self.trap(TrapKind::TaskGroup, "second join of a task group", pos_id);
            return std::ptr::null_mut();
        }
        let handle = self.alloc(16, CLASS_GENERATOR, pos_id);
        if handle.is_null() {
            return handle;
        }
        unsafe { self.async_register(handle, 0) };
        if let Some(meta) = self.async_frames.get_mut(&(handle as usize)) {
            meta.kind = AsyncKind::Runtime(Box::new(RuntimeTask::GroupJoin(group as usize)));
            meta.create_pos_id = pos_id;
        }
        if let Some(state) = self.task_groups.get_mut(&(group as usize)) {
            state.closed = true;
            state.join = Some(handle);
        }
        self.task_group_settle(group as usize);
        handle
    }

    /// Ends the declaring scope and checks unjoined work (§170 rule 7).
    ///
    /// # Safety
    /// The caller ends the group's unique lexical owner exactly once.
    pub unsafe fn task_group_release(&mut self, group: *mut u8, pos_id: u32) {
        let Some(state) = self.task_groups.get_mut(&(group as usize)) else {
            return;
        };
        state.lexical = false;
        if !state.closed {
            // A completed input can still have its reaction in the ready queue.
            // Read its completion for the exit report without dispatch or observation.
            let mut unfinished = 0;
            let mut failed = state.failed;
            for input in state.inputs.iter().flatten() {
                match self
                    .async_frames
                    .get(&(*input as usize))
                    .and_then(|meta| meta.completion.as_ref())
                {
                    Some(Completion::Value(_)) => {}
                    Some(Completion::Exception(_)) => failed += 1,
                    None => unfinished += 1,
                }
            }
            if unfinished != 0 || failed != 0 {
                self.trap(
                    TrapKind::TaskGroup,
                    format!("task group scope ended: {unfinished} unfinished, {failed} failed"),
                    pos_id,
                );
            }
        }
        self.task_group_cleanup(group as usize);
    }

    pub(super) unsafe fn task_group_react(&mut self, group: usize, index: usize) {
        let input = self
            .task_groups
            .get_mut(&group)
            .and_then(|state| state.inputs.get_mut(index))
            .and_then(Option::take);
        let Some(input) = input else {
            self.async_missing_completion(0);
            return;
        };
        let completion = self
            .async_frames
            .get_mut(&(input as usize))
            .and_then(|meta| meta.completion.as_mut())
            .map(|completion| {
                if let Completion::Exception(payload) = completion {
                    payload.observed = true;
                }
                completion.clone()
            });
        let Some(completion) = completion else {
            self.async_missing_completion(0);
            return;
        };
        if let Some(state) = self.task_groups.get_mut(&group) {
            state.remaining -= 1;
            state.finished += 1;
            if let Completion::Exception(payload) = completion {
                state.failed += 1;
                if state.first.is_none() {
                    state.first = Some(payload.exception);
                }
            }
        }
        unsafe { self.async_release(input, 0) };
        self.task_group_settle(group);
        self.task_group_cleanup(group);
    }

    fn task_group_settle(&mut self, group: usize) {
        let Some(state) = self.task_groups.get(&group) else {
            return;
        };
        if state.remaining != 0 {
            return;
        }
        let Some(handle) = state.join else {
            return;
        };
        let completion = match state.first.clone() {
            Some(exception) => Completion::Exception(Box::new(ExceptionCompletion {
                exception,
                observed: false,
            })),
            None => Completion::Value(Vec::new()),
        };
        if let Some(meta) = self.async_frames.get_mut(&(handle as usize)) {
            if meta.completion.is_some() {
                return;
            }
            meta.completion = Some(completion);
            self.async_ready.extend(std::mem::take(&mut meta.waiters));
        }
        // A dropped, unfinished join remains live until its reactions finish.
        if unsafe { self.async_count(handle) } == 0 {
            let meta = self.async_frames.remove(&(handle as usize));
            self.delete(handle as usize, 0);
            self.task_group_join_released(group);
            if let Some(exception) = meta.and_then(|meta| meta.completion?.unobserved()) {
                self.trap(
                    TrapKind::UncaughtException,
                    exception.message,
                    exception.pos_id,
                );
            }
        }
    }

    pub(super) fn task_group_join_released(&mut self, group: usize) {
        if let Some(state) = self.task_groups.get_mut(&group) {
            state.join = None;
        }
        self.task_group_cleanup(group);
    }

    fn task_group_cleanup(&mut self, group: usize) {
        if self
            .task_groups
            .get(&group)
            .is_some_and(|state| !state.lexical && state.join.is_none() && state.remaining == 0)
        {
            self.task_groups.remove(&group);
            self.delete(group, 0);
        }
    }

    pub(super) fn task_group_roots(&self) -> Vec<(usize, usize, usize)> {
        self.task_groups
            .iter()
            .enumerate()
            .flat_map(|(index, (&group, state))| {
                std::iter::once(group)
                    .chain(state.inputs.iter().flatten().map(|input| *input as usize))
                    .chain(state.join.map(|join| join as usize))
                    .chain(state.first.as_ref().map(|exception| exception.object))
                    .enumerate()
                    .map(move |(word, root)| (index, word, root))
            })
            .collect()
    }
}
