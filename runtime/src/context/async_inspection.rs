use super::*;

/// One registered task (§169). The C layout has no padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct AsyncTaskInfo {
    /// Context-local registration id, starting at one and never reused.
    pub task_id: u64,
    /// The awaited task for a waiting invocation; otherwise zero.
    pub awaited_task_id: u64,
    /// READY=1, PARKED=2, WAITING=3, ACTIVE=4, COMPLETE=5, STOPPED=6.
    pub state: u32,
    /// Invocation=1, aggregate=2, group join=3.
    pub kind: u32,
    /// Allocation position of the function or the aggregate/group join call.
    pub function_pos_id: u32,
    /// Suspension position; zero for active, complete, aggregate, group join, and initial ready tasks.
    pub await_pos_id: u32,
    /// Call position; zero for host roots and aggregates.
    pub create_pos_id: u32,
    /// Reserved; always zero.
    pub reserved: u32,
}

/// Receives one task record (§169). The pointer is valid only during the call.
/// The callback must not change or release the Context.
pub type AsyncTaskVisitor = unsafe extern "C" fn(userdata: *mut c_void, info: *const AsyncTaskInfo);

impl Context {
    /// Visits each registered task in id order (§169), with no script or Context allocation.
    /// A null visitor returns zero. The visit changes no state.
    ///
    /// # Safety
    /// The callback and userdata match. The callback must not change or release this Context.
    /// Registered frames must follow the generated scheduler protocol.
    pub unsafe fn visit_async_tasks(
        &self,
        visitor: Option<AsyncTaskVisitor>,
        userdata: *mut c_void,
    ) -> u64 {
        let Some(visitor) = visitor else {
            return 0;
        };
        let mut frames: Vec<_> = self.async_frames.iter().collect();
        frames.sort_unstable_by_key(|(_, meta)| meta.task_id);
        let ready: std::collections::HashSet<_> = self
            .async_ready
            .iter()
            .filter_map(|job| match job {
                AsyncJob::Invocation(frame) => Some(*frame as usize),
                _ => None,
            })
            .collect();
        let parked: std::collections::HashSet<_> = self
            .async_parked
            .iter()
            .map(|frame| *frame as usize)
            .collect();
        let stopped: std::collections::HashSet<_> = self
            .async_stopped
            .iter()
            .map(|frame| *frame as usize)
            .collect();
        let active: std::collections::HashSet<_> =
            self.active_async_frames.iter().copied().collect();
        let mut waiting = HashMap::new();
        for meta in self.async_frames.values() {
            for job in &meta.waiters {
                if let AsyncJob::Invocation(frame) = job {
                    waiting.entry(*frame as usize).or_insert(meta.task_id);
                }
            }
        }
        let mut records = Vec::with_capacity(frames.len());
        for (&frame, meta) in &frames {
            let aggregate = meta.kind.task_kind() != 1;
            let mut awaited_task_id = 0;
            let state = if meta.completion.is_some() {
                5
            } else if aggregate {
                3
            } else if active.contains(&frame) {
                4
            } else if ready.contains(&frame) {
                1
            } else if parked.contains(&frame) {
                2
            } else if stopped.contains(&frame) {
                6
            } else if let Some(&awaited) = waiting.get(&frame) {
                awaited_task_id = awaited;
                3
            } else {
                // An unclassified frame violates the scheduler invariant (§169).
                return 0;
            };
            // SAFETY: the registry retains a live allocation with its initialized header.
            let function_pos_id = unsafe { header_pos_id((frame as *const u8).sub(HEADER_SIZE)) };
            let info = AsyncTaskInfo {
                task_id: meta.task_id,
                awaited_task_id,
                state,
                kind: meta.kind.task_kind(),
                function_pos_id,
                create_pos_id: meta.create_pos_id,
                reserved: 0,
                await_pos_id: if aggregate || state == 4 || state == 5 {
                    0
                } else {
                    meta.await_pos_id
                },
            };
            records.push(info);
        }
        for info in &records {
            // SAFETY: the caller supplies the callback and its userdata.
            unsafe { visitor(userdata, info) };
        }
        records.len() as u64
    }
}
