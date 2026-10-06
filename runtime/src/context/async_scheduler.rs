use super::*;

/// Host checkpoint counts (`compiler.md` §168). The C layout has no padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct AsyncStepReport {
    /// Jobs that the checkpoint started, including a trapping job.
    pub dispatched: u64,
    /// Ready jobs plus parked registrations at return.
    pub pending: u64,
    /// Registered invocations without a cached completion at return.
    pub unfinished: u64,
    /// One when the budget ends with a job still ready; otherwise zero.
    pub budget_exhausted: u64,
}

#[derive(Default)]
pub(crate) struct AsyncFrameMeta {
    pub(super) task_id: u64,
    pub(super) await_pos_id: u32,
    pub(super) create_pos_id: u32,
    created_epoch: u32,
    // compiler.md §116.2 rule 1: a value or an exception.
    pub(crate) completion: Option<crate::exception::Completion>,
    // The fulfilled-value size the scheduler supplies when it resumes this
    // frame, and the continuations registered on it (§94.1 rule 5).
    result_size: usize,
    pub(crate) waiters: Vec<AsyncJob>,
    // compiler.md §116.1 rule 5: a host-kicked export root has no script
    // holder, so an exception that leaves it settles into a trap.
    pub(crate) host_root: bool,
    pub(super) kind: AsyncKind,
}

/// Aligned result storage for one scheduler resume (`compiler.md` §94.2).
struct ResultStorage {
    words: Vec<u128>,
    size: usize,
}

impl ResultStorage {
    fn new(size: usize) -> Self {
        Self {
            words: vec![0u128; size.div_ceil(16)],
            size,
        }
    }

    fn out(&mut self) -> *mut u8 {
        if self.size == 0 {
            std::ptr::null_mut()
        } else {
            self.words.as_mut_ptr().cast()
        }
    }

    fn value(&self) -> *const u8 {
        if self.size == 0 {
            std::ptr::null()
        } else {
            self.words.as_ptr().cast()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncJob {
    Invocation(*mut u8),
    Aggregate { handle: *mut u8, index: usize },
    Group { group: usize, index: usize },
}
impl AsyncJob {
    pub(super) fn handle(self) -> *mut u8 {
        match self {
            Self::Invocation(frame) => frame,
            Self::Aggregate { handle, .. } => handle,
            Self::Group { .. } => std::ptr::null_mut(),
        }
    }
}
#[derive(Default)]
pub(super) enum AsyncKind {
    #[default]
    Invocation,
    Runtime(Box<RuntimeTask>),
}
pub(super) enum RuntimeTask {
    Aggregate(Aggregate),
    GroupJoin(usize),
}
impl AsyncKind {
    pub(super) fn aggregate(&self) -> Option<&Aggregate> {
        match self {
            Self::Runtime(task) => match task.as_ref() {
                RuntimeTask::Aggregate(state) => Some(state),
                _ => None,
            },
            _ => None,
        }
    }
    pub(super) fn aggregate_mut(&mut self) -> Option<&mut Aggregate> {
        match self {
            Self::Runtime(task) => match task.as_mut() {
                RuntimeTask::Aggregate(state) => Some(state),
                _ => None,
            },
            _ => None,
        }
    }
    pub(super) fn join_group(&self) -> Option<usize> {
        match self {
            Self::Runtime(task) => match task.as_ref() {
                RuntimeTask::GroupJoin(group) => Some(*group),
                _ => None,
            },
            _ => None,
        }
    }
    pub(super) fn task_kind(&self) -> u32 {
        match self {
            Self::Invocation => 1,
            Self::Runtime(task) => match task.as_ref() {
                RuntimeTask::Aggregate(_) => 2,
                RuntimeTask::GroupJoin(_) => 3,
            },
        }
    }
}
pub(super) struct Aggregate {
    inputs: Vec<Option<*mut u8>>,
    result: *mut u8,
    remaining: usize,
    elem_size: usize,
    reported: bool,
}

impl Context {
    /// Registers a fresh compiler-generated async frame. Its reference
    /// count starts at one in the frame header's `reserved` word.
    ///
    /// # Safety
    ///
    /// `frame` is a fresh live coroutine allocation with at least eight
    /// payload bytes and belongs to this Context.
    pub unsafe fn async_register(&mut self, frame: *mut u8, result_size: usize) {
        if frame.is_null() {
            return;
        }
        let Some(next_id) = self.next_async_task_id.checked_add(1) else {
            self.trap(TrapKind::Internal, "async task id exhausted", 0);
            return;
        };
        let task_id = self.next_async_task_id;
        self.next_async_task_id = next_id;
        // SAFETY: guaranteed by the caller; offset four is the aligned
        // `uint32_t reserved` word in every generated coroutine header.
        unsafe { (frame.add(4) as *mut u32).write(1) };
        self.async_frames.insert(
            frame as usize,
            AsyncFrameMeta {
                task_id,
                create_pos_id: 0,
                await_pos_id: 0,
                created_epoch: self.reload_epoch,
                completion: None,
                result_size,
                waiters: Vec::new(),
                host_root: false,
                kind: AsyncKind::Invocation,
            },
        );
    }

    /// Parks a frame that suspended at `Context.suspend()`
    /// (`compiler.md` §94.1 rule 3). The next host checkpoint makes it
    /// runnable, in registration order.
    ///
    /// # Safety
    ///
    /// `frame` is a registered live async frame in this Context.
    pub unsafe fn async_park(&mut self, frame: *mut u8, pos_id: u32) {
        let Some(meta) = self.async_frames.get_mut(&(frame as usize)) else {
            return;
        };
        meta.await_pos_id = pos_id;
        // The registry lookup above proves that this frame has a live count word.
        // The scheduler owns one reference for every registered suspension.
        let count = unsafe { &mut *frame.add(4).cast::<u32>() };
        *count = count.saturating_add(1);
        self.async_parked.push_back(frame);
    }

    /// Registers `frame` as a continuation of `handle` (`compiler.md`
    /// §94.1 rules 4 to 6). A completed handle places the continuation at
    /// the ready queue's tail; an unfinished handle keeps it in its own
    /// registration-ordered list. The registration holds one handle count
    /// until the resume reads its completion and releases that count (§116.1).
    ///
    /// # Safety
    ///
    /// Both pointers are registered live async frames in this Context.
    pub unsafe fn async_await(&mut self, frame: *mut u8, handle: *mut u8, pos_id: u32) {
        if frame.is_null() || !self.async_frames.contains_key(&(frame as usize)) {
            return;
        }
        unsafe {
            self.async_retain(handle);
            self.async_await_owned(frame, handle, pos_id);
        }
    }

    /// Moves the call's handle count to an await registration (§116.1 rule 4a).
    ///
    /// # Safety
    ///
    /// Both pointers are registered live frames. The caller transfers one handle count.
    pub unsafe fn async_await_owned(&mut self, frame: *mut u8, handle: *mut u8, pos_id: u32) {
        let Some(meta) = self.async_frames.get_mut(&(frame as usize)) else {
            return;
        };
        meta.await_pos_id = pos_id;
        // The registry lookup above proves that this frame has a live count word.
        // The scheduler owns one reference for every registered suspension.
        let count = unsafe { &mut *frame.add(4).cast::<u32>() };
        *count = count.saturating_add(1);
        match self.async_frames.get_mut(&(handle as usize)) {
            Some(meta) if meta.completion.is_none() => {
                meta.waiters.push(AsyncJob::Invocation(frame))
            }
            _ => self.async_ready.push_back(AsyncJob::Invocation(frame)),
        }
    }

    /// Reports a scheduled await resume without its cached completion.
    /// `compiler.md` §94.1 makes this an internal protocol defect: the
    /// Context stops under the ordinary trap policy, and no consumer
    /// re-registers, polls, or fabricates a result.
    pub fn async_missing_completion(&mut self, pos_id: u32) {
        self.trap(
            TrapKind::Internal,
            "async resume without completion",
            pos_id,
        );
    }

    /// The runnable continuation count. Test and benchmark
    /// instrumentation; `compiler.md` §94.2 contracts only
    /// `async_pending` and `async_unfinished` as host observers.
    #[must_use]
    pub fn async_ready_len(&self) -> usize {
        self.async_ready.len()
    }

    /// The count of frames that wait for a host checkpoint. Test
    /// instrumentation, as `async_ready_len` is.
    #[must_use]
    pub fn async_parked_len(&self) -> usize {
        self.async_parked.len()
    }

    /// The number of registered async invocations without a cached
    /// completion (`compiler.md` §94.2). A host reads it beside
    /// `async_pending` to tell quiescence from blocked work.
    #[must_use]
    pub fn async_unfinished(&self) -> usize {
        self.async_frames
            .values()
            .filter(|meta| meta.completion.is_none() && matches!(meta.kind, AsyncKind::Invocation))
            .count()
    }

    /// Increments one held async handle count.
    ///
    /// # Safety
    ///
    /// `frame` is a registered live async frame in this Context.
    pub unsafe fn async_retain(&mut self, frame: *mut u8) {
        if frame.is_null() || !self.async_frames.contains_key(&(frame as usize)) {
            return;
        }
        // SAFETY: guaranteed by the caller.
        let count = unsafe { (frame.add(4) as *mut u32).read() };
        // Static ownership checking prevents an unbounded copy count in a
        // valid program; saturating avoids wrapping into a premature free.
        unsafe { (frame.add(4) as *mut u32).write(count.saturating_add(1)) };
    }

    /// Decrements one held async handle count and frees the frame exactly
    /// when the count reaches zero.
    ///
    /// A frame whose completion holds an exception that no `await` raised
    /// traps with [`TrapKind::UncaughtException`] at that point
    /// (`compiler.md` §116.1 rule 4).
    ///
    /// # Safety
    ///
    /// `frame` is a registered live async frame in this Context and the
    /// caller owns one reference.
    pub unsafe fn async_release(&mut self, frame: *mut u8, pos_id: u32) {
        let Some(meta) = self.async_frames.get(&(frame as usize)) else {
            return;
        };
        // SAFETY: guaranteed by the caller.
        let slot = unsafe { &mut *(frame.add(4) as *mut u32) };
        if *slot == 0 {
            return;
        }
        *slot -= 1;
        if *slot == 0 {
            let group = meta.kind.join_group();
            let keep = meta
                .kind
                .aggregate()
                .is_some_and(|state| state.remaining != 0)
                || group.is_some_and(|group| {
                    self.task_groups
                        .get(&group)
                        .is_some_and(|state| state.remaining != 0)
                });
            let unobserved = if keep {
                match meta.completion.as_ref() {
                    Some(crate::exception::Completion::Exception(payload)) if !payload.observed => {
                        Some(payload.exception.clone())
                    }
                    _ => None,
                }
            } else {
                let meta = self.async_frames.remove(&(frame as usize));
                self.delete(frame as usize, pos_id);
                if let Some(group) = group {
                    self.task_group_join_released(group);
                }
                meta.and_then(|meta| meta.completion?.unobserved())
            };
            if let Some(unobserved) = unobserved {
                if let Some(meta) = self.async_frames.get_mut(&(frame as usize)) {
                    if let Some(state) = meta.kind.aggregate_mut() {
                        state.reported = true;
                    }
                }
                self.trap(
                    TrapKind::UncaughtException,
                    unobserved.message,
                    unobserved.pos_id,
                );
            }
        }
    }

    /// Reads a held handle's count for the emitted-layout conformance test.
    ///
    /// # Safety
    ///
    /// `frame` is a live async frame.
    #[must_use]
    pub unsafe fn async_count(&self, frame: *const u8) -> u32 {
        if frame.is_null() {
            return 0;
        }
        // SAFETY: guaranteed by the caller.
        unsafe { (frame.add(4) as *const u32).read() }
    }

    /// Returns whether a registered frame predates the current reload epoch.
    #[must_use]
    pub fn async_is_stale(&self, frame: *const u8) -> bool {
        self.async_frames
            .get(&(frame as usize))
            .is_some_and(|meta| meta.created_epoch != self.reload_epoch)
    }

    /// Caches the fulfilled representation after the first held await.
    ///
    /// A completion is immutable (§94.1 rule 11): a frame that already
    /// completed with an exception keeps it (`compiler.md` §116.2 rule 2).
    ///
    /// # Safety
    ///
    /// `value` is null when `size == 0`, otherwise it points to `size`
    /// readable bytes for the duration of this call.
    pub unsafe fn async_complete(&mut self, frame: *mut u8, value: *const u8, size: usize) {
        let Some(meta) = self.async_frames.get_mut(&(frame as usize)) else {
            return;
        };
        if meta.completion.is_some() {
            return;
        }
        let bytes = if size == 0 {
            Vec::new()
        } else {
            // SAFETY: guaranteed by the caller.
            unsafe { std::slice::from_raw_parts(value, size) }.to_vec()
        };
        meta.completion = Some(crate::exception::Completion::Value(bytes));
        // §94.1 rule 5: completion makes every registered continuation
        // runnable, in registration order, at the queue's tail.
        let waiters = std::mem::take(&mut meta.waiters);
        self.async_ready.extend(waiters);
    }

    /// Copies a cached fulfilled representation into `out`, returning
    /// `true` when the handle had already completed.
    ///
    /// For an exception completion, the call leaves `out` unchanged, makes
    /// the exception pending again with its object, report text, and
    /// position, and returns `true`. The raise site of the `await` then
    /// takes its edge (`compiler.md` §116.1 rules 2 and 3).
    ///
    /// # Safety
    ///
    /// `out` is null when `size == 0`, otherwise it points to `size`
    /// writable bytes.
    pub unsafe fn async_result(&mut self, frame: *const u8, out: *mut u8, size: usize) -> bool {
        let Some(completion) = self
            .async_frames
            .get_mut(&(frame as usize))
            .and_then(|meta| meta.completion.as_mut())
        else {
            return false;
        };
        let bytes = match completion {
            crate::exception::Completion::Value(bytes) => bytes,
            crate::exception::Completion::Exception(payload) => {
                payload.observed = true;
                let exception = payload.exception.clone();
                self.raise_exception(
                    exception.object as *mut u8,
                    exception.message,
                    exception.pos_id,
                );
                return true;
            }
        };
        if size != 0 {
            if bytes.len() != size {
                return false;
            }
            // SAFETY: guaranteed by the caller; the slices do not overlap
            // because cached bytes are Context-owned storage.
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, size) };
        }
        true
    }

    /// Starts a called invocation and keeps it active during host reads (§169).
    /// The caller caches its completion and owns its handle count.
    /// `pos_id` identifies the creation call site (§169).
    ///
    /// # Safety
    /// The frame has a live generated resume pointer at offset eight.
    /// The output holds the invocation's fulfilled representation.
    pub unsafe fn async_start(&mut self, frame: *mut u8, out: *mut u8, pos_id: u32) -> u8 {
        if frame.is_null() || self.trapped() {
            return 0;
        }
        if let Some(meta) = self.async_frames.get_mut(&(frame as usize)) {
            meta.create_pos_id = pos_id;
        }
        self.active_async_frames.push(frame as usize);
        // SAFETY: the caller supplies a live generated frame and matching output.
        let resume = unsafe { frame.add(8).cast::<AsyncResume>().read() };
        let done = unsafe { resume(self, frame, out) };
        self.active_async_frames.pop();
        if self.trapped() {
            self.async_stopped.push(frame);
        }
        done
    }

    /// Runs a newly invoked async root to its first suspension or completion.
    /// `Context.suspend()` puts the root on the parked list (§94.1 rule 3).
    /// An await of an unfinished child puts the root on that child's waiter list
    /// (§94.1 rule 4). An await of a completed handle queues a ready job
    /// (§94.1 rule 6). The kick does not drain ready jobs (§94.1 rule 7).
    ///
    /// # Safety
    ///
    /// `frame` and `resume` must be the matching compiler-generated async
    /// frame and resume function for this Context.
    pub unsafe fn async_kick(&mut self, frame: *mut u8, resume: AsyncResume) {
        if frame.is_null() || self.trapped() {
            return;
        }
        if let Some(meta) = self.async_frames.get_mut(&(frame as usize)) {
            meta.host_root = true;
        }
        self.active_async_frames.push(frame as usize);
        let ctx = self as *mut Context;
        // SAFETY: guaranteed by the caller; the active root keeps the frame
        // reachable if the generated body explicitly collects.
        let done = unsafe { resume(ctx, frame, std::ptr::null_mut()) };
        self.active_async_frames.pop();
        if self.trapped() {
            self.async_stopped.push(frame);
            return;
        }
        if done != 0 {
            // A root returns no value, so its completion record is empty.
            unsafe { self.async_complete(frame, std::ptr::null(), 0) };
        }
        // §94.1 rule 10: an export kick follows the same rules as a call.
        // A suspended root already registered itself with the scheduler,
        // which holds its own reference, so the kick never keeps one.
        unsafe { self.async_release(frame, 0) };
    }

    /// Work a host checkpoint can advance: runnable jobs plus parked waiters.
    #[must_use]
    pub fn async_pending(&self) -> usize {
        self.async_ready.len() + self.async_parked.len()
    }

    /// Runs one host checkpoint (`compiler.md` §94.1 rules 8 and 9): it
    /// appends the whole pre-existing parked list after the jobs that are
    /// already ready, then drains the ready queue in FIFO order. Jobs added
    /// during the drain join the same checkpoint; a frame parked during it
    /// waits for the next one. The drain has no job budget. §168 supplies the bounded form.
    ///
    /// A trap preserves the ready head until clearance (§94.2).
    /// Clearance stops that frame permanently, except for reload staleness.
    /// A stale frame remains ready and reports again after clearance.
    ///
    /// # Safety
    ///
    /// Every queued callback/frame pair was supplied through
    /// [`Context::async_kick`] and its generated code remains live.
    pub unsafe fn async_step(&mut self) -> usize {
        unsafe { self.async_drain(None) }.pending as usize
    }

    /// Starts at most `max_dispatches` jobs (`compiler.md` §168).
    /// A zero budget runs no script and promotes no parked frame.
    /// One dispatch runs to suspension, completion, or a trap; this is no time limit.
    ///
    /// # Safety
    /// Every queued callback/frame pair matches, and its generated code remains live.
    pub unsafe fn async_step_budget(&mut self, max_dispatches: u64) -> AsyncStepReport {
        unsafe { self.async_drain(Some(max_dispatches)) }
    }

    unsafe fn async_drain(&mut self, limit: Option<u64>) -> AsyncStepReport {
        let mut dispatched = 0;
        if self.trapped() || limit == Some(0) {
            return self.async_report(dispatched, limit);
        }
        // §94.1 rule 8: the checkpoint appends the entire pre-existing
        // parked list after the jobs that are already ready, then drains the
        // ready queue in FIFO order. Rule 9 leaves a frame parked during
        // this drain for the next checkpoint.
        let parked = std::mem::take(&mut self.async_parked);
        self.async_ready
            .extend(parked.into_iter().map(AsyncJob::Invocation));
        if self.async_ready.is_empty() {
            return self.async_report(dispatched, limit);
        }
        let active_base = self.active_async_frames.len();
        self.enter_script();
        while limit != Some(dispatched) {
            let Some(job) = self.async_ready.pop_front() else {
                break;
            };
            dispatched += 1;
            let frame = match job {
                AsyncJob::Invocation(frame) => frame,
                AsyncJob::Group { group, index } => {
                    unsafe { self.task_group_react(group, index) };
                    if self.trapped() {
                        if let Some(trap) = self.trap_record() {
                            self.async_trapping = Some((job, trap.kind));
                        }
                        self.async_ready.push_front(job);
                        break;
                    }
                    continue;
                }
                AsyncJob::Aggregate { handle, index } => {
                    unsafe { self.async_all_react(handle, index) };
                    if self.trapped() {
                        if let Some(trap) = self.trap_record() {
                            self.async_trapping = Some((job, trap.kind));
                        }
                        self.async_ready.push_front(job);
                        break;
                    }
                    continue;
                }
            };
            let size = self
                .async_frames
                .get(&(frame as usize))
                .map_or(0, |meta| meta.result_size);
            let mut storage = ResultStorage::new(size);
            self.active_async_frames.push(frame as usize);
            let ctx = self as *mut Context;
            // SAFETY: a queued frame is a registered generated coroutine, so
            // its resume pointer sits at the fixed header offset.
            let resume = unsafe { frame.add(8).cast::<AsyncResume>().read() };
            // SAFETY: frames enter the queue only through the generated
            // registration calls, which supply the matching resume function.
            let done = unsafe { resume(ctx, frame, storage.out()) };
            self.active_async_frames.pop();
            if self.trapped() {
                // §94.2: preserve pending until host clearance. Record the
                // kind now; only reload staleness permits another resume.
                if let Some(trap) = self.trap_record() {
                    self.async_trapping = Some((job, trap.kind));
                }
                self.async_ready.push_front(AsyncJob::Invocation(frame));
                break;
            }
            if done != 0 {
                // SAFETY: `storage` holds `size` initialized bytes.
                unsafe { self.async_complete(frame, storage.value(), size) };
            }
            // The scheduler's own reference ends here. A frame that
            // suspended again registered a new one before it returned.
            unsafe { self.async_release(frame, 0) };
            if self.trapped() {
                // compiler.md §116.1 rule 4: the release freed a frame that
                // holds an unobserved exception. No frame is left to keep.
                break;
            }
        }
        self.active_async_frames.truncate(active_base);
        self.exit_script();
        self.async_report(dispatched, limit)
    }

    fn async_report(&self, dispatched: u64, limit: Option<u64>) -> AsyncStepReport {
        AsyncStepReport {
            dispatched,
            pending: self.async_pending() as u64,
            unfinished: if limit.is_some() {
                self.async_unfinished() as u64
            } else {
                0
            },
            budget_exhausted: u64::from(limit == Some(dispatched) && !self.async_ready.is_empty()),
        }
    }
}

impl Context {
    /// Creates an aggregate from a snapshot of an async handle array (§166).
    /// Results hold no count and use `elem_size` bytes per element.
    ///
    /// # Safety
    /// `jobs` is a live array of registered handles in this Context.
    /// Each input has the same fulfilled representation of `elem_size` bytes.
    pub unsafe fn async_all(&mut self, jobs: *const u8, elem_size: usize, pos_id: u32) -> *mut u8 {
        if !self.require_live_handle(jobs as usize, pos_id) {
            return std::ptr::null_mut();
        }
        let array = unsafe { &*(jobs as *const ArrayHeader) };
        let inputs: Vec<Option<*mut u8>> = (0..array.len as usize)
            .map(|index| Some(unsafe { array.data.cast::<*mut u8>().add(index).read() }))
            .collect();
        let result = self.array_with_capacity(inputs.len(), elem_size, pos_id);
        if self.trapped() {
            return std::ptr::null_mut();
        }
        unsafe {
            (*(result as *mut ArrayHeader)).len = inputs.len() as u64;
        }
        let handle = self.alloc(16, CLASS_GENERATOR, pos_id);
        if handle.is_null() {
            return handle;
        }
        unsafe {
            self.async_register(handle, std::mem::size_of::<*mut u8>());
        }
        let remaining = inputs.len();
        for input in inputs.iter().flatten() {
            unsafe {
                self.async_retain(*input);
            }
        }
        if let Some(meta) = self.async_frames.get_mut(&(handle as usize)) {
            meta.kind = AsyncKind::Runtime(Box::new(RuntimeTask::Aggregate(Aggregate {
                inputs: inputs.clone(),
                result,
                remaining,
                elem_size,
                reported: false,
            })));
        }
        for (index, input) in inputs.iter().flatten().enumerate() {
            let job = AsyncJob::Aggregate { handle, index };
            match self.async_frames.get_mut(&(*input as usize)) {
                Some(meta) if meta.completion.is_none() => meta.waiters.push(job),
                _ => self.async_ready.push_back(job),
            }
        }
        if remaining == 0 {
            unsafe {
                self.async_complete(
                    handle,
                    (&result as *const *mut u8).cast(),
                    std::mem::size_of::<*mut u8>(),
                );
            }
        }
        handle
    }

    unsafe fn async_all_react(&mut self, handle: *mut u8, index: usize) {
        let Some(meta) = self.async_frames.get_mut(&(handle as usize)) else {
            self.async_missing_completion(0);
            return;
        };
        let Some(state) = meta.kind.aggregate_mut() else {
            self.async_missing_completion(0);
            return;
        };
        let Some(input) = state.inputs.get_mut(index).and_then(Option::take) else {
            self.async_missing_completion(0);
            return;
        };
        let result = state.result;
        let elem_size = state.elem_size;
        state.remaining -= 1;
        let last = state.remaining == 0;
        let fulfilled = meta.completion.is_none();
        let completion = self
            .async_frames
            .get_mut(&(input as usize))
            .and_then(|meta| meta.completion.as_mut())
            .map(|completion| {
                if let crate::exception::Completion::Exception(payload) = completion {
                    payload.observed = true;
                }
                completion.clone()
            });
        match completion {
            Some(crate::exception::Completion::Value(bytes)) => {
                if bytes.len() != elem_size {
                    self.async_missing_completion(0);
                    return;
                }
                if fulfilled && elem_size != 0 {
                    let data = unsafe { (*(result as *mut ArrayHeader)).data };
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            bytes.as_ptr(),
                            data.add(index * elem_size),
                            elem_size,
                        );
                    }
                }
                if last && fulfilled {
                    unsafe {
                        self.async_complete(
                            handle,
                            (&result as *const *mut u8).cast(),
                            std::mem::size_of::<*mut u8>(),
                        );
                    }
                }
            }
            Some(crate::exception::Completion::Exception(payload)) if fulfilled => {
                self.raise_exception(
                    payload.exception.object as *mut u8,
                    payload.exception.message,
                    payload.exception.pos_id,
                );
                self.async_complete_exception(handle);
            }
            Some(_) => {}
            None => {
                self.async_missing_completion(0);
                return;
            }
        }
        unsafe {
            self.async_release(input, 0);
        }
        if unsafe { self.async_count(handle) } == 0 {
            let exception = self
                .async_frames
                .get_mut(&(handle as usize))
                .and_then(|meta| {
                    let state = meta.kind.aggregate_mut()?;
                    if state.reported {
                        return None;
                    }
                    let exception = meta.completion.clone()?.unobserved()?;
                    state.reported = true;
                    Some(exception)
                });
            if let Some(exception) = exception {
                self.trap(
                    TrapKind::UncaughtException,
                    exception.message,
                    exception.pos_id,
                );
            }
            if last && !self.trapped() {
                self.async_frames.remove(&(handle as usize));
                self.delete(handle as usize, 0);
            }
        }
    }

    pub(super) fn async_clear_trapping_job(&mut self, job: AsyncJob) {
        match job {
            AsyncJob::Invocation(frame) => self.async_stopped.push(frame),
            AsyncJob::Group { .. } => {}
            AsyncJob::Aggregate { handle, .. } => {
                let done = self
                    .async_frames
                    .get(&(handle as usize))
                    .is_some_and(|meta| {
                        meta.kind
                            .aggregate()
                            .is_some_and(|state| state.remaining == 0)
                    });
                if done && unsafe { self.async_count(handle) } == 0 {
                    self.async_frames.remove(&(handle as usize));
                    self.delete(handle as usize, 0);
                }
            }
        }
    }

    pub(super) fn async_aggregate_roots(&self) -> Vec<(usize, usize, usize)> {
        self.async_frames
            .values()
            .enumerate()
            .flat_map(|(index, meta)| match meta.kind.aggregate() {
                None => Vec::new(),
                Some(state) => std::iter::once((index, 0, state.result as usize))
                    .chain(
                        state
                            .inputs
                            .iter()
                            .flatten()
                            .enumerate()
                            .map(|(word, input)| (index, word + 1, *input as usize)),
                    )
                    .collect(),
            })
            .collect()
    }
}
