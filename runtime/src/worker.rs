//! Runtime-owned worker threads and their two fixed-payload byte queues.
//!
//! This module is the runtime's only shared-mutable-state implementation.
//! Queue state is protected by a mutex, blocking receives sleep on a
//! condition variable, and Contexts themselves never cross threads.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

use crate::context::Context;
use crate::trap::TrapRecord;

const CLASS_WORKER_MESSAGE: u32 = 0xFFFF_FF0A;

/// Initializer called on a newly created worker Context before its entry.
pub type WorkerInit = unsafe extern "C" fn(ctx: *mut Context);

/// Entry called on a worker thread with its dedicated Context and endpoints.
pub type WorkerEntry = unsafe extern "C" fn(
    ctx: *mut Context,
    inbox: *mut WorkerInbox,
    outbox: *mut WorkerOutbox,
);

/// Parent-owned opaque handle for one runtime worker.
///
/// The spawning Context owns this handle. Its address remains stable until
/// that Context is released, and it must not be freed by the host.
#[repr(C)]
pub struct Worker {
    input: Arc<Queue>,
    output: Arc<Queue>,
    thread: Option<JoinHandle<WorkerOutcome>>,
    outcome: Option<WorkerOutcome>,
}

/// Worker-side opaque receiving endpoint for parent-to-worker messages.
///
/// The endpoint is valid only for the duration of the worker entry call.
#[repr(C)]
pub struct WorkerInbox {
    queue: Arc<Queue>,
}

/// Worker-side opaque sending endpoint for worker-to-parent messages.
///
/// The endpoint is valid only for the duration of the worker entry call.
#[repr(C)]
pub struct WorkerOutbox {
    queue: Arc<Queue>,
}

struct Queue {
    payload_size: usize,
    state: Mutex<QueueState>,
    ready: Condvar,
}

#[derive(Default)]
struct QueueState {
    messages: VecDeque<Box<[u8]>>,
    closed: bool,
}

pub(crate) enum Receive {
    Message(Box<[u8]>),
    Empty,
    Closed,
}

#[derive(Clone)]
pub(crate) enum WorkerOutcome {
    Clean,
    Trapped(TrapRecord),
    ThreadFailed,
}

#[derive(Default)]
pub(crate) struct WorkerSet {
    workers: Vec<Box<Worker>>,
}

pub(crate) enum PostResult {
    Posted,
    Closed,
    NullPayload,
}

impl Queue {
    fn new(payload_size: usize) -> Queue {
        Queue {
            payload_size,
            state: Mutex::new(QueueState::default()),
            ready: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, QueueState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn post(&self, payload: &[u8]) -> bool {
        let copied = payload.to_vec().into_boxed_slice();
        let mut state = self.lock();
        if state.closed {
            return false;
        }
        state.messages.push_back(copied);
        self.ready.notify_one();
        true
    }

    fn poll(&self) -> Receive {
        let mut state = self.lock();
        if let Some(message) = state.messages.pop_front() {
            return Receive::Message(message);
        }
        if state.closed {
            Receive::Closed
        } else {
            Receive::Empty
        }
    }

    fn wait(&self) -> Receive {
        let mut state = self.lock();
        loop {
            if let Some(message) = state.messages.pop_front() {
                return Receive::Message(message);
            }
            if state.closed {
                return Receive::Closed;
            }
            state = match self.ready.wait(state) {
                Ok(next) => next,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
    }

    fn close(&self) {
        let mut state = self.lock();
        if !state.closed {
            state.closed = true;
            self.ready.notify_all();
        }
    }
}

impl Drop for WorkerInbox {
    fn drop(&mut self) {
        self.queue.close();
    }
}

impl Drop for WorkerOutbox {
    fn drop(&mut self) {
        self.queue.close();
    }
}

impl Worker {
    fn start(
        init: WorkerInit,
        entry: WorkerEntry,
        input_payload_size: usize,
        output_payload_size: usize,
        releasing_context: bool,
        fn_table: usize,
    ) -> std::io::Result<Box<Worker>> {
        let input = Arc::new(Queue::new(input_payload_size));
        let output = Arc::new(Queue::new(output_payload_size));
        let thread_input = Arc::clone(&input);
        let thread_output = Arc::clone(&output);
        let thread = std::thread::Builder::new().spawn(move || {
            run_worker(
                init,
                entry,
                thread_input,
                thread_output,
                releasing_context,
                fn_table,
            )
        })?;
        Ok(Box::new(Worker {
            input,
            output,
            thread: Some(thread),
            outcome: None,
        }))
    }

    fn close(&self) {
        self.input.close();
    }

    fn join(&mut self) -> WorkerOutcome {
        if let Some(outcome) = &self.outcome {
            return outcome.clone();
        }
        let outcome = match self.thread.take() {
            Some(thread) => match thread.join() {
                Ok(outcome) => outcome,
                Err(_) => WorkerOutcome::ThreadFailed,
            },
            None => WorkerOutcome::ThreadFailed,
        };
        self.outcome = Some(outcome.clone());
        outcome
    }

    unsafe fn post(&self, payload: *const u8) -> PostResult {
        let bytes = if self.input.payload_size == 0 {
            &[]
        } else {
            if payload.is_null() {
                return PostResult::NullPayload;
            }
            // SAFETY: the caller guarantees one readable fixed-size input
            // payload; the queue copies it before this call returns.
            unsafe { std::slice::from_raw_parts(payload, self.input.payload_size) }
        };
        if self.input.post(bytes) {
            PostResult::Posted
        } else {
            PostResult::Closed
        }
    }

    fn poll(&self) -> Receive {
        self.output.poll()
    }
}

impl WorkerSet {
    pub(crate) fn spawn(
        &mut self,
        init: WorkerInit,
        entry: WorkerEntry,
        input_payload_size: usize,
        output_payload_size: usize,
        releasing_context: bool,
        fn_table: usize,
    ) -> std::io::Result<*mut Worker> {
        let mut worker = Worker::start(
            init,
            entry,
            input_payload_size,
            output_payload_size,
            releasing_context,
            fn_table,
        )?;
        let handle = std::ptr::from_mut(worker.as_mut());
        self.workers.push(worker);
        Ok(handle)
    }

    pub(crate) unsafe fn post(
        &self,
        handle: *mut Worker,
        payload: *const u8,
    ) -> Option<PostResult> {
        let worker = self.find(handle)?;
        // SAFETY: forwarded fixed-payload pointer contract.
        Some(unsafe { worker.post(payload) })
    }

    pub(crate) fn poll(&self, handle: *mut Worker) -> Option<Receive> {
        Some(self.find(handle)?.poll())
    }

    pub(crate) fn close(&self, handle: *mut Worker) -> bool {
        let Some(worker) = self.find(handle) else {
            return false;
        };
        worker.close();
        true
    }

    pub(crate) fn join(&mut self, handle: *mut Worker) -> Option<WorkerOutcome> {
        Some(self.find_mut(handle)?.join())
    }

    pub(crate) fn shutdown(&mut self) {
        for worker in &self.workers {
            worker.close();
        }
        for worker in &mut self.workers {
            let _ = worker.join();
        }
    }

    pub(crate) fn has_live_workers(&self) -> bool {
        self.workers.iter().any(|worker| worker.thread.is_some())
    }

    fn find(&self, handle: *mut Worker) -> Option<&Worker> {
        if handle.is_null() {
            return None;
        }
        self.workers
            .iter()
            .find(|worker| std::ptr::eq(worker.as_ref(), handle.cast_const()))
            .map(Box::as_ref)
    }

    fn find_mut(&mut self, handle: *mut Worker) -> Option<&mut Worker> {
        if handle.is_null() {
            return None;
        }
        self.workers
            .iter_mut()
            .find(|worker| std::ptr::eq(worker.as_ref(), handle.cast_const()))
            .map(Box::as_mut)
    }
}

impl Drop for WorkerSet {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_worker(
    init: WorkerInit,
    entry: WorkerEntry,
    input: Arc<Queue>,
    output: Arc<Queue>,
    releasing_context: bool,
    fn_table: usize,
) -> WorkerOutcome {
    let mut ctx = Context::new_worker(releasing_context);
    ctx.set_fn_table(fn_table as *const *const u8);
    let ctx_ptr = std::ptr::from_mut(ctx.as_mut());
    let mut inbox = WorkerInbox { queue: input };
    let mut outbox = WorkerOutbox { queue: output };
    ctx.enter_script();
    // SAFETY: spawn receives C-callable function pointers from the linked
    // program and all three arguments remain live for these calls.
    unsafe { init(ctx_ptr) };
    if !ctx.trapped() {
        // SAFETY: same function-pointer and argument lifetime contract.
        unsafe { entry(ctx_ptr, &mut inbox, &mut outbox) };
    }
    ctx.exit_script();
    let outcome = match ctx.trap_record() {
        Some(record) => WorkerOutcome::Trapped(record.clone()),
        None => WorkerOutcome::Clean,
    };
    drop(inbox);
    drop(outbox);
    drop(ctx);
    outcome
}

fn materialize(ctx: &mut Context, receive: Receive) -> *mut u8 {
    let Receive::Message(message) = receive else {
        return std::ptr::null_mut();
    };
    if ctx.trapped() {
        return std::ptr::null_mut();
    }
    let payload = ctx.alloc(message.len(), CLASS_WORKER_MESSAGE, 0);
    if payload.is_null() || message.is_empty() {
        return payload;
    }
    // SAFETY: `payload` is a fresh allocation of `message.len()` bytes and
    // `message` owns that many readable bytes.
    unsafe { std::ptr::copy_nonoverlapping(message.as_ptr(), payload, message.len()) };
    payload
}

pub(crate) unsafe fn inbox_wait(
    ctx: &mut Context,
    inbox: *mut WorkerInbox,
) -> *mut u8 {
    if inbox.is_null() || ctx.trapped() {
        return std::ptr::null_mut();
    }
    // SAFETY: the worker entry receives its live stack-owned endpoint.
    materialize(ctx, unsafe { &*inbox }.queue.wait())
}

pub(crate) unsafe fn inbox_poll(
    ctx: &mut Context,
    inbox: *mut WorkerInbox,
) -> *mut u8 {
    if inbox.is_null() || ctx.trapped() {
        return std::ptr::null_mut();
    }
    // SAFETY: the worker entry receives its live stack-owned endpoint.
    materialize(ctx, unsafe { &*inbox }.queue.poll())
}

pub(crate) unsafe fn outbox_post(
    ctx: &mut Context,
    outbox: *mut WorkerOutbox,
    payload: *const u8,
) -> PostResult {
    if outbox.is_null() || ctx.trapped() {
        return PostResult::Closed;
    }
    // SAFETY: the worker entry receives its live stack-owned endpoint.
    let queue = &unsafe { &*outbox }.queue;
    let bytes = if queue.payload_size == 0 {
        &[]
    } else {
        if payload.is_null() {
            return PostResult::NullPayload;
        }
        // SAFETY: the caller guarantees one readable fixed-size output
        // payload; the queue copies it before this call returns.
        unsafe { std::slice::from_raw_parts(payload, queue.payload_size) }
    };
    if queue.post(bytes) {
        PostResult::Posted
    } else {
        PostResult::Closed
    }
}

pub(crate) fn materialize_parent(ctx: &mut Context, receive: Receive) -> *mut u8 {
    materialize(ctx, receive)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Barrier;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::ffi::{
        subscript_rt_ctx_clear_trap, subscript_rt_ctx_live_allocations,
        subscript_rt_ctx_new, subscript_rt_ctx_release, subscript_rt_ctx_trap_kind,
        subscript_rt_trap, subscript_rt_worker_close, subscript_rt_worker_inbox_poll,
        subscript_rt_worker_inbox_wait, subscript_rt_worker_join,
        subscript_rt_worker_outbox_post, subscript_rt_worker_poll,
        subscript_rt_worker_post, subscript_rt_worker_spawn,
    };
    use crate::trap::TrapKind;

    unsafe extern "C" fn no_op_init(_ctx: *mut Context) {}

    unsafe extern "C" fn echo_entry(
        ctx: *mut Context,
        inbox: *mut WorkerInbox,
        outbox: *mut WorkerOutbox,
    ) {
        loop {
            // SAFETY: this is the live Context and inbox supplied to entry.
            let message = unsafe { subscript_rt_worker_inbox_wait(ctx, inbox) };
            if message.is_null() {
                return;
            }
            // SAFETY: the receive allocation has the configured output size.
            if unsafe { subscript_rt_worker_outbox_post(ctx, outbox, message) } == 0 {
                return;
            }
        }
    }

    unsafe extern "C" fn clean_entry(
        _ctx: *mut Context,
        _inbox: *mut WorkerInbox,
        _outbox: *mut WorkerOutbox,
    ) {
    }

    unsafe extern "C" fn trap_entry(
        ctx: *mut Context,
        _inbox: *mut WorkerInbox,
        _outbox: *mut WorkerOutbox,
    ) {
        // SAFETY: entry received the live worker Context.
        unsafe { subscript_rt_trap(ctx, TrapKind::EmptyPop as u32, 77) };
    }

    unsafe extern "C" fn nested_worker_entry(
        ctx: *mut Context,
        _inbox: *mut WorkerInbox,
        _outbox: *mut WorkerOutbox,
    ) {
        // SAFETY: the current worker Context is itself a valid spawn parent;
        // both callbacks stay linked through the nested join.
        let child = unsafe {
            subscript_rt_worker_spawn(ctx, Some(no_op_init), Some(clean_entry), 0, 0)
        };
        if child.is_null() {
            return;
        }
        // SAFETY: `child` belongs to this worker Context.
        let _ = unsafe { subscript_rt_worker_join(ctx, child) };
    }

    unsafe extern "C" fn poll_entry(
        ctx: *mut Context,
        inbox: *mut WorkerInbox,
        outbox: *mut WorkerOutbox,
    ) {
        // SAFETY: live worker Context and endpoint.
        let first = unsafe { subscript_rt_worker_inbox_wait(ctx, inbox) };
        if first.is_null() {
            return;
        }
        // SAFETY: this test configures a `usize` payload containing the
        // address of a Barrier that outlives the worker.
        let barrier = unsafe { &*((first as *const usize).read() as *const Barrier) };
        barrier.wait();
        // The parent posts the second message before entering the barrier,
        // so this non-blocking poll must observe it after the rendezvous.
        // SAFETY: live worker Context and endpoint.
        let second = unsafe { subscript_rt_worker_inbox_poll(ctx, inbox) };
        if second.is_null() {
            // SAFETY: live worker Context.
            unsafe { subscript_rt_trap(ctx, TrapKind::Internal as u32, 0) };
            return;
        }
        // SAFETY: the output payload has the same `usize` size.
        let _ = unsafe { subscript_rt_worker_outbox_post(ctx, outbox, second) };
    }

    unsafe extern "C" fn release_probe_entry(
        ctx: *mut Context,
        inbox: *mut WorkerInbox,
        outbox: *mut WorkerOutbox,
    ) {
        // SAFETY: live worker Context and endpoint.
        let message = unsafe { subscript_rt_worker_inbox_wait(ctx, inbox) };
        if message.is_null() {
            return;
        }
        // SAFETY: the test payload stores a live AtomicBool address.
        let released = unsafe { &*((message as *const usize).read() as *const AtomicBool) };
        // Acknowledge materialization, then block until parent Context
        // teardown closes the input queue.
        // SAFETY: the output size matches the received payload.
        let _ = unsafe { subscript_rt_worker_outbox_post(ctx, outbox, message) };
        // SAFETY: live worker Context and endpoint.
        let end = unsafe { subscript_rt_worker_inbox_wait(ctx, inbox) };
        if end.is_null() {
            released.store(true, Ordering::SeqCst);
        }
    }

    unsafe fn poll_until(parent: *mut Context, worker: *mut Worker) -> *mut u8 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            // SAFETY: the helper is called with a live parent-owned worker.
            let message = unsafe { subscript_rt_worker_poll(parent, worker) };
            if !message.is_null() {
                return message;
            }
            assert!(Instant::now() < deadline, "worker reply timed out");
            std::thread::yield_now();
        }
    }

    #[test]
    fn c_abi_echo_worker_round_trips_fixed_payload_copies() {
        const N: u64 = 32;

        let parent = subscript_rt_ctx_new();
        // SAFETY: fresh parent Context and linked callbacks.
        let worker = unsafe {
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(echo_entry),
                std::mem::size_of::<u64>() as u64,
                std::mem::size_of::<u64>() as u64,
            )
        };
        assert!(!worker.is_null());
        for value in 0..N {
            // SAFETY: live parent-owned worker and readable fixed payload.
            assert_eq!(
                unsafe {
                    subscript_rt_worker_post(parent, worker, std::ptr::from_ref(&value).cast())
                },
                1
            );
        }
        let mut replies = Vec::new();
        for _ in 0..N {
            // SAFETY: the live parent polls its owned worker until each
            // non-blocking receive observes the corresponding reply.
            let message = unsafe { poll_until(parent, worker) };
            assert!(!message.is_null());
            // SAFETY: each reply is a fresh aligned Context allocation of a
            // copied `u64` payload.
            replies.push(unsafe { message.cast::<u64>().read() });
        }
        assert_eq!(replies, (0..N).collect::<Vec<_>>());
        // SAFETY: the handle belongs to `parent`; close wakes the worker's
        // next blocking wait and join observes a clean outcome.
        unsafe { subscript_rt_worker_close(parent, worker) };
        assert_eq!(unsafe { subscript_rt_worker_join(parent, worker) }, 1);
        // SAFETY: the worker output is now closed and drained.
        assert!(unsafe { subscript_rt_worker_poll(parent, worker) }.is_null());
        assert_eq!(
            // SAFETY: shared accounting query on the live parent.
            unsafe { subscript_rt_ctx_live_allocations(parent) },
            N
        );
        // SAFETY: parent is released exactly once.
        unsafe { subscript_rt_ctx_release(parent) };
    }

    #[test]
    fn clean_join_supports_workers_spawning_workers() {
        let parent = subscript_rt_ctx_new();
        // SAFETY: fresh parent Context and linked callbacks.
        let worker = unsafe {
            subscript_rt_worker_spawn(parent, Some(no_op_init), Some(nested_worker_entry), 0, 0)
        };
        assert!(!worker.is_null());
        // SAFETY: both outer and nested workers terminate cleanly.
        assert_eq!(unsafe { subscript_rt_worker_join(parent, worker) }, 1);
        // SAFETY: shared trap query followed by one release.
        assert_eq!(unsafe { subscript_rt_ctx_trap_kind(parent) }, 0);
        unsafe { subscript_rt_ctx_release(parent) };
    }

    #[test]
    fn trapped_worker_traps_joining_context_with_kind_22() {
        let parent = subscript_rt_ctx_new();
        // SAFETY: fresh parent Context and linked callbacks.
        let worker = unsafe {
            subscript_rt_worker_spawn(parent, Some(no_op_init), Some(trap_entry), 0, 0)
        };
        assert!(!worker.is_null());
        // SAFETY: the worker terminates after recording its trap.
        assert_eq!(unsafe { subscript_rt_worker_join(parent, worker) }, 0);
        // SAFETY: shared trap query on the live parent.
        assert_eq!(
            unsafe { subscript_rt_ctx_trap_kind(parent) },
            TrapKind::WorkerTrapped as u32
        );
        // SAFETY: this is a host boundary, so clearing is permitted.
        assert_eq!(unsafe { subscript_rt_ctx_clear_trap(parent) }, 1);
        // A repeated join repeats the loud outcome rather than losing it.
        // SAFETY: the handle remains parent-owned after joining.
        assert_eq!(unsafe { subscript_rt_worker_join(parent, worker) }, 0);
        assert_eq!(
            // SAFETY: shared trap query on the live parent.
            unsafe { subscript_rt_ctx_trap_kind(parent) },
            TrapKind::WorkerTrapped as u32
        );
        // SAFETY: parent is released exactly once.
        unsafe { subscript_rt_ctx_release(parent) };
    }

    #[test]
    fn worker_inbox_poll_is_non_blocking_and_materializes_a_copy() {
        let parent = subscript_rt_ctx_new();
        let barrier = Barrier::new(2);
        let barrier_address = std::ptr::from_ref(&barrier) as usize;
        let value = 0xC0DEusize;
        // SAFETY: fresh parent Context and linked callbacks.
        let worker = unsafe {
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(poll_entry),
                std::mem::size_of::<usize>() as u64,
                std::mem::size_of::<usize>() as u64,
            )
        };
        assert!(!worker.is_null());
        // SAFETY: both payloads are readable and have the configured size.
        assert_eq!(
            unsafe {
                subscript_rt_worker_post(
                    parent,
                    worker,
                    std::ptr::from_ref(&barrier_address).cast(),
                )
            },
            1
        );
        assert_eq!(
            unsafe { subscript_rt_worker_post(parent, worker, std::ptr::from_ref(&value).cast()) },
            1
        );
        barrier.wait();
        // SAFETY: close and join the parent-owned worker.
        unsafe { subscript_rt_worker_close(parent, worker) };
        assert_eq!(unsafe { subscript_rt_worker_join(parent, worker) }, 1);
        // SAFETY: the reply is queued after the rendezvous and before join.
        let reply = unsafe { subscript_rt_worker_poll(parent, worker) };
        assert!(!reply.is_null());
        // SAFETY: fresh `usize`-sized Context allocation.
        assert_eq!(unsafe { reply.cast::<usize>().read() }, value);
        // SAFETY: parent is released exactly once.
        unsafe { subscript_rt_ctx_release(parent) };
    }

    #[test]
    fn two_workers_keep_independent_reply_sets() {
        const COUNT: u64 = 24;

        let parent = subscript_rt_ctx_new();
        // SAFETY: fresh parent Context and linked callbacks.
        let first = unsafe {
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(echo_entry),
                std::mem::size_of::<u64>() as u64,
                std::mem::size_of::<u64>() as u64,
            )
        };
        let second = unsafe {
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(echo_entry),
                std::mem::size_of::<u64>() as u64,
                std::mem::size_of::<u64>() as u64,
            )
        };
        assert!(!first.is_null() && !second.is_null());
        for index in 0..COUNT {
            let left = index * 2;
            let right = index * 2 + 1;
            // SAFETY: both parent-owned handles and payloads remain live.
            assert_eq!(
                unsafe {
                    subscript_rt_worker_post(parent, first, std::ptr::from_ref(&left).cast())
                },
                1
            );
            assert_eq!(
                unsafe {
                    subscript_rt_worker_post(parent, second, std::ptr::from_ref(&right).cast())
                },
                1
            );
        }
        // SAFETY: close both inputs, then join both workers.
        unsafe {
            subscript_rt_worker_close(parent, first);
            subscript_rt_worker_close(parent, second);
        }
        assert_eq!(unsafe { subscript_rt_worker_join(parent, second) }, 1);
        assert_eq!(unsafe { subscript_rt_worker_join(parent, first) }, 1);

        let mut first_replies = Vec::new();
        let mut second_replies = Vec::new();
        for _ in 0..COUNT {
            // SAFETY: the clean joins guarantee all replies are queued.
            let left = unsafe { subscript_rt_worker_poll(parent, first) };
            let right = unsafe { subscript_rt_worker_poll(parent, second) };
            assert!(!left.is_null() && !right.is_null());
            // SAFETY: fresh `u64`-sized Context allocations.
            first_replies.push(unsafe { left.cast::<u64>().read() });
            second_replies.push(unsafe { right.cast::<u64>().read() });
        }
        first_replies.sort_unstable();
        second_replies.sort_unstable();
        assert_eq!(first_replies, (0..COUNT).map(|n| n * 2).collect::<Vec<_>>());
        assert_eq!(
            second_replies,
            (0..COUNT).map(|n| n * 2 + 1).collect::<Vec<_>>()
        );
        // SAFETY: parent is released exactly once.
        unsafe { subscript_rt_ctx_release(parent) };
    }

    #[test]
    fn parent_release_closes_joins_and_frees_a_live_worker() {
        let released = AtomicBool::new(false);
        let released_address = std::ptr::from_ref(&released) as usize;
        let parent = subscript_rt_ctx_new();
        // SAFETY: `parent` remains live while its test stats handle is cloned.
        let stats = unsafe { &*parent }.test_arena_stats();
        // SAFETY: fresh parent Context and linked callbacks.
        let worker = unsafe {
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(release_probe_entry),
                std::mem::size_of::<usize>() as u64,
                std::mem::size_of::<usize>() as u64,
            )
        };
        assert!(!worker.is_null());
        // SAFETY: the atomic address payload remains live through release.
        assert_eq!(
            unsafe {
                subscript_rt_worker_post(
                    parent,
                    worker,
                    std::ptr::from_ref(&released_address).cast(),
                )
            },
            1
        );
        // The acknowledgement proves the worker materialized its input and
        // is now live in its second blocking receive. Polling materializes a
        // corresponding parent allocation covered by arena accounting.
        // SAFETY: live parent-owned worker.
        let acknowledgement = unsafe { poll_until(parent, worker) };
        assert!(!acknowledgement.is_null());
        assert_ne!(stats.owned_resources(), (0, 0));

        // No explicit close or join: Context release must do both before it
        // returns, discard queues, and free the parent allocation.
        // SAFETY: parent is released exactly once with its worker still live.
        unsafe { subscript_rt_ctx_release(parent) };
        assert!(released.load(Ordering::SeqCst));
        assert_eq!(stats.owned_resources(), (0, 0));
    }
}
