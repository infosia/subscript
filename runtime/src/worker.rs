//! Runtime-owned worker threads and their two message byte queues.
//!
//! This module is the runtime's only shared-mutable-state implementation.
//! Queue state is protected by a mutex, blocking receives sleep on a
//! condition variable, and Contexts themselves never cross threads.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

use crate::context::Context;
use crate::trap::{TrapKind, TrapRecord};

const CLASS_WORKER_MESSAGE: u32 = 0xFFFF_FF0A;

/// Initializer called on a newly created worker Context before its entry.
pub type WorkerInit = unsafe extern "C" fn(ctx: *mut Context);

/// Entry called on a worker thread with its dedicated Context and endpoints.
pub type WorkerEntry =
    unsafe extern "C" fn(ctx: *mut Context, inbox: *mut WorkerInbox, outbox: *mut WorkerOutbox);

/// Program-image description of one worker message class.
///
/// `string_slot_offsets` points to `string_slot_count` ascending byte offsets
/// in the fixed payload. A null offsets pointer is valid only when the count
/// is zero.
#[repr(C)]
pub struct WorkerMessageDescriptor {
    /// Fixed C-layout payload size in bytes.
    pub payload_size: u64,
    /// Number of string handle slots in the fixed payload.
    pub string_slot_count: u64,
    /// Program-image array of string handle byte offsets.
    pub string_slot_offsets: *const u64,
}

#[derive(Debug)]
pub(crate) struct QueueDescriptor {
    payload_size: usize,
    string_slot_offsets: Box<[usize]>,
}

impl QueueDescriptor {
    /// Copies and validates one generated program-image descriptor.
    ///
    /// # Safety
    ///
    /// `descriptor` must point to a live descriptor and its offset array must
    /// remain readable for this call.
    pub(crate) unsafe fn copy_from(
        descriptor: *const WorkerMessageDescriptor,
    ) -> Result<Self, String> {
        if descriptor.is_null() {
            return Err("worker message descriptor is null".into());
        }
        // SAFETY: the caller supplies one readable descriptor.
        let descriptor = unsafe { &*descriptor };
        if descriptor.payload_size > isize::MAX as u64 {
            return Err("worker message payload size is not representable".into());
        }
        let payload_size = descriptor.payload_size as usize;
        let max_offset_count = (isize::MAX as usize) / std::mem::size_of::<u64>();
        if descriptor.string_slot_count > max_offset_count as u64 {
            return Err("worker message string slot count is not representable".into());
        }
        let count = descriptor.string_slot_count as usize;
        if count != 0 && descriptor.string_slot_offsets.is_null() {
            return Err("worker message string slot offsets are null".into());
        }
        let raw_offsets = if count == 0 {
            &[][..]
        } else {
            // SAFETY: the descriptor promises `count` readable offsets.
            unsafe { std::slice::from_raw_parts(descriptor.string_slot_offsets, count) }
        };
        let mut offsets = Vec::new();
        offsets
            .try_reserve_exact(count)
            .map_err(|_| "worker message string slot table allocation failed".to_string())?;
        let mut previous = None;
        for &raw_offset in raw_offsets {
            let Some(end) = raw_offset.checked_add(std::mem::size_of::<*const u8>() as u64) else {
                return Err("worker message string slot is outside the payload".into());
            };
            if end > descriptor.payload_size {
                return Err("worker message string slot is outside the payload".into());
            }
            let offset = raw_offset as usize;
            if previous.is_some_and(|prior| prior >= offset) {
                return Err("worker message string slot offsets are not ascending".into());
            }
            offsets.push(offset);
            previous = Some(offset);
        }
        Ok(Self {
            payload_size,
            string_slot_offsets: offsets.into_boxed_slice(),
        })
    }

    #[cfg(test)]
    fn fixed(payload_size: usize) -> Self {
        Self {
            payload_size,
            string_slot_offsets: Box::new([]),
        }
    }
}

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
    descriptor: Arc<QueueDescriptor>,
    state: Mutex<QueueState>,
    ready: Condvar,
}

#[derive(Default)]
struct QueueState {
    messages: VecDeque<Box<[u8]>>,
    closed: bool,
}

pub(crate) enum Receive {
    Message {
        record: Box<[u8]>,
        descriptor: Arc<QueueDescriptor>,
    },
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
    AllocationFailed,
}

impl Queue {
    fn new(descriptor: QueueDescriptor) -> Queue {
        Queue {
            descriptor: Arc::new(descriptor),
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

    fn post(&self, copied: Box<[u8]>) -> bool {
        let mut state = self.lock();
        if state.closed {
            return false;
        }
        state.messages.push_back(copied);
        self.ready.notify_one();
        true
    }

    unsafe fn post_fixed(&self, ctx: &Context, payload: *const u8) -> PostResult {
        let bytes = if self.descriptor.payload_size == 0 {
            &[]
        } else {
            if payload.is_null() {
                return PostResult::NullPayload;
            }
            // SAFETY: the caller supplies one readable fixed-size payload.
            unsafe { std::slice::from_raw_parts(payload, self.descriptor.payload_size) }
        };
        let mut record = Vec::new();
        if record.try_reserve_exact(bytes.len()).is_err() {
            return PostResult::AllocationFailed;
        }
        record.extend_from_slice(bytes);
        for &offset in self.descriptor.string_slot_offsets.iter() {
            // SAFETY: descriptor validation proved that one pointer-sized
            // slot begins at `offset` inside the readable fixed payload.
            let handle = unsafe { payload.add(offset).cast::<*const u8>().read_unaligned() };
            let string = if handle.is_null() {
                &[][..]
            } else {
                // SAFETY: generated message fields contain live strings from
                // the sender Context.
                unsafe { ctx.str_bytes(handle) }
            };
            let additional = std::mem::size_of::<u64>().saturating_add(string.len());
            if record.try_reserve_exact(additional).is_err() {
                return PostResult::AllocationFailed;
            }
            record.extend_from_slice(&(string.len() as u64).to_le_bytes());
            record.extend_from_slice(string);
        }
        if self.post(record.into_boxed_slice()) {
            PostResult::Posted
        } else {
            PostResult::Closed
        }
    }

    fn poll(&self) -> Receive {
        let mut state = self.lock();
        if let Some(message) = state.messages.pop_front() {
            return Receive::Message {
                record: message,
                descriptor: Arc::clone(&self.descriptor),
            };
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
                return Receive::Message {
                    record: message,
                    descriptor: Arc::clone(&self.descriptor),
                };
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
        input_descriptor: QueueDescriptor,
        output_descriptor: QueueDescriptor,
        releasing_context: bool,
        fn_table: usize,
    ) -> std::io::Result<Box<Worker>> {
        let input = Arc::new(Queue::new(input_descriptor));
        let output = Arc::new(Queue::new(output_descriptor));
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

    unsafe fn post(&self, ctx: &Context, payload: *const u8) -> PostResult {
        unsafe { self.input.post_fixed(ctx, payload) }
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
        input_descriptor: QueueDescriptor,
        output_descriptor: QueueDescriptor,
        releasing_context: bool,
        fn_table: usize,
    ) -> std::io::Result<*mut Worker> {
        let mut worker = Worker::start(
            init,
            entry,
            input_descriptor,
            output_descriptor,
            releasing_context,
            fn_table,
        )?;
        let handle = std::ptr::from_mut(worker.as_mut());
        self.workers.push(worker);
        Ok(handle)
    }

    pub(crate) unsafe fn post(
        &self,
        ctx: &Context,
        handle: *mut Worker,
        payload: *const u8,
    ) -> Option<PostResult> {
        let worker = self.find(handle)?;
        // SAFETY: forwarded fixed-payload pointer contract.
        Some(unsafe { worker.post(ctx, payload) })
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
    // SAFETY: Context::worker_spawn erased this pointer to `usize` only to
    // cross the thread boundary. ReloadSession refuses code replacement
    // until all workers join and drops the parent Context before its JIT
    // modules, so reconstructing the exact table pointer here is valid for
    // the worker Context's entire lifetime.
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

pub(crate) fn materialize(ctx: &mut Context, receive: Receive) -> *mut u8 {
    let Receive::Message { record, descriptor } = receive else {
        return std::ptr::null_mut();
    };
    if ctx.trapped() {
        return std::ptr::null_mut();
    }
    let payload = ctx.alloc(descriptor.payload_size, CLASS_WORKER_MESSAGE, 0);
    if payload.is_null() {
        return payload;
    }
    let Some(fixed) = record.get(..descriptor.payload_size) else {
        ctx.trap(
            TrapKind::Internal,
            "worker message record has no fixed payload",
            0,
        );
        return std::ptr::null_mut();
    };
    if !fixed.is_empty() {
        // SAFETY: `payload` is a fresh allocation of `payload_size` bytes and
        // `fixed` owns exactly that many readable bytes.
        unsafe { std::ptr::copy_nonoverlapping(fixed.as_ptr(), payload, fixed.len()) };
    }
    let mut cursor = descriptor.payload_size;
    for &offset in descriptor.string_slot_offsets.iter() {
        let Some(length_bytes) = record.get(cursor..cursor.saturating_add(8)) else {
            ctx.trap(
                TrapKind::Internal,
                "worker message record has no string length",
                0,
            );
            return std::ptr::null_mut();
        };
        let length = u64::from_le_bytes(length_bytes.try_into().expect("eight-byte string length"));
        let Ok(length) = usize::try_from(length) else {
            ctx.trap(
                TrapKind::Internal,
                "worker message string length is not representable",
                0,
            );
            return std::ptr::null_mut();
        };
        cursor += 8;
        let Some(bytes) = record.get(cursor..cursor.saturating_add(length)) else {
            ctx.trap(
                TrapKind::Internal,
                "worker message record has incomplete string bytes",
                0,
            );
            return std::ptr::null_mut();
        };
        let string = ctx.alloc_str(bytes, 0);
        if string.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: descriptor validation proved that one pointer-sized slot
        // begins at `offset` inside the new fixed payload.
        unsafe {
            payload
                .add(offset)
                .cast::<*mut u8>()
                .write_unaligned(string)
        };
        cursor += length;
    }
    if cursor != record.len() {
        ctx.trap(
            TrapKind::Internal,
            "worker message record has trailing bytes",
            0,
        );
        return std::ptr::null_mut();
    }
    payload
}

pub(crate) unsafe fn inbox_wait(ctx: &mut Context, inbox: *mut WorkerInbox) -> *mut u8 {
    if inbox.is_null() || ctx.trapped() {
        return std::ptr::null_mut();
    }
    // SAFETY: the worker entry receives its live stack-owned endpoint.
    materialize(ctx, unsafe { &*inbox }.queue.wait())
}

pub(crate) unsafe fn inbox_poll(ctx: &mut Context, inbox: *mut WorkerInbox) -> *mut u8 {
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
    // SAFETY: forwarded fixed-payload contract.
    unsafe { queue.post_fixed(ctx, payload) }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Barrier;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::ffi::{
        subscript_rt_ctx_clear_trap, subscript_rt_ctx_fail_alloc_after,
        subscript_rt_ctx_live_allocations, subscript_rt_ctx_new, subscript_rt_ctx_release,
        subscript_rt_ctx_trap_kind, subscript_rt_trap, subscript_rt_worker_close,
        subscript_rt_worker_inbox_poll, subscript_rt_worker_inbox_wait, subscript_rt_worker_join,
        subscript_rt_worker_outbox_post, subscript_rt_worker_poll, subscript_rt_worker_post,
        subscript_rt_worker_spawn,
    };
    use crate::trap::TrapKind;

    const EMPTY_DESCRIPTOR: WorkerMessageDescriptor = WorkerMessageDescriptor {
        payload_size: 0,
        string_slot_count: 0,
        string_slot_offsets: std::ptr::null(),
    };

    fn fixed_descriptor(payload_size: usize) -> WorkerMessageDescriptor {
        WorkerMessageDescriptor {
            payload_size: payload_size as u64,
            string_slot_count: 0,
            string_slot_offsets: std::ptr::null(),
        }
    }

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
            subscript_rt_worker_spawn(
                ctx,
                Some(no_op_init),
                Some(clean_entry),
                &EMPTY_DESCRIPTOR,
                &EMPTY_DESCRIPTOR,
            )
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

    #[repr(C)]
    struct TwoStringMessage {
        first: *mut u8,
        count: i32,
        second: *mut u8,
    }

    fn string_descriptor(payload_size: usize, offsets: &[usize]) -> QueueDescriptor {
        QueueDescriptor {
            payload_size,
            string_slot_offsets: offsets.to_vec().into_boxed_slice(),
        }
    }

    fn hand_built_receive(payload_size: usize, offsets: &[usize], record: Vec<u8>) -> Receive {
        Receive::Message {
            record: record.into_boxed_slice(),
            descriptor: Arc::new(string_descriptor(payload_size, offsets)),
        }
    }

    #[test]
    fn post_two_string_slots_matches_a_hand_written_record() {
        let mut sender = Context::new();
        let first = sender.alloc_str("héllo".as_bytes(), 0);
        let second = sender.alloc_str(b"world", 0);
        let message = TwoStringMessage {
            first,
            count: 41,
            second,
        };
        let queue = Queue::new(string_descriptor(
            std::mem::size_of::<TwoStringMessage>(),
            &[0, 16],
        ));
        // SAFETY: `message` has the descriptor's fixed layout and both
        // string handles belong to `sender`.
        assert!(matches!(
            unsafe { queue.post_fixed(&sender, std::ptr::from_ref(&message).cast()) },
            PostResult::Posted
        ));
        let Receive::Message { record, .. } = queue.poll() else {
            panic!("posted record is missing");
        };
        let mut expected = Vec::new();
        expected.extend_from_slice(&(first as usize).to_ne_bytes());
        expected.extend_from_slice(&41i32.to_ne_bytes());
        expected.extend_from_slice(&[0, 0, 0, 0]);
        expected.extend_from_slice(&(second as usize).to_ne_bytes());
        expected.extend_from_slice(&6u64.to_le_bytes());
        expected.extend_from_slice("héllo".as_bytes());
        expected.extend_from_slice(&5u64.to_le_bytes());
        expected.extend_from_slice(b"world");
        assert_eq!(&*record, expected);
    }

    #[test]
    fn materialize_two_string_slots_from_a_hand_written_record() {
        let mut sender = Context::new();
        let mut receiver = Context::new();
        let first = sender.alloc_str("héllo".as_bytes(), 0);
        let second = sender.alloc_str(b"world", 0);
        let mut record = Vec::new();
        record.extend_from_slice(&(first as usize).to_ne_bytes());
        record.extend_from_slice(&41i32.to_ne_bytes());
        record.extend_from_slice(&[0, 0, 0, 0]);
        record.extend_from_slice(&(second as usize).to_ne_bytes());
        record.extend_from_slice(&6u64.to_le_bytes());
        record.extend_from_slice("héllo".as_bytes());
        record.extend_from_slice(&5u64.to_le_bytes());
        record.extend_from_slice(b"world");
        let copy = materialize(
            &mut receiver,
            hand_built_receive(std::mem::size_of::<TwoStringMessage>(), &[0, 16], record),
        )
        .cast::<TwoStringMessage>();
        assert!(!copy.is_null());
        // SAFETY: `copy` is a materialized `TwoStringMessage` allocation.
        let copy = unsafe { &*copy };
        assert_eq!(copy.count, 41);
        for copied in [copy.first, copy.second] {
            assert_ne!(copied, first);
            assert_ne!(copied, second);
        }
        // SAFETY: both copied handles belong to `receiver`.
        assert_eq!(
            unsafe { receiver.str_bytes(copy.first) },
            "héllo".as_bytes()
        );
        assert_eq!(unsafe { receiver.str_bytes(copy.second) }, b"world");
    }

    #[test]
    fn null_string_slot_materializes_as_a_fresh_empty_string() {
        let mut receiver = Context::new();
        let mut record = vec![0; std::mem::size_of::<TwoStringMessage>()];
        record.extend_from_slice(&0u64.to_le_bytes());
        let copy = materialize(
            &mut receiver,
            hand_built_receive(std::mem::size_of::<TwoStringMessage>(), &[0], record),
        )
        .cast::<TwoStringMessage>();
        assert!(!copy.is_null());
        // SAFETY: `copy` is a materialized `TwoStringMessage` allocation.
        let copied = unsafe { (*copy).first };
        assert!(!copied.is_null());
        // SAFETY: the copied handle belongs to `receiver`.
        assert!(unsafe { receiver.str_bytes(copied) }.is_empty());
    }

    #[test]
    fn allocated_empty_string_materializes_from_a_hand_written_record() {
        let mut sender = Context::new();
        let mut receiver = Context::new();
        let empty = sender.alloc_str(b"", 0);
        let mut record = Vec::new();
        record.extend_from_slice(&(empty as usize).to_ne_bytes());
        record.extend_from_slice(&[0; 16]);
        record.extend_from_slice(&0u64.to_le_bytes());
        let copy = materialize(
            &mut receiver,
            hand_built_receive(std::mem::size_of::<TwoStringMessage>(), &[0], record),
        )
        .cast::<TwoStringMessage>();
        assert!(!copy.is_null());
        // SAFETY: `copy` is a materialized `TwoStringMessage` allocation.
        let copied = unsafe { (*copy).first };
        assert_ne!(copied, empty);
        // SAFETY: the copied handle belongs to `receiver`.
        assert!(unsafe { receiver.str_bytes(copied) }.is_empty());
    }

    #[test]
    fn count_zero_post_matches_a_hand_written_fixed_record() {
        let sender = Context::new();
        let expected = [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01];
        let value = u64::from_ne_bytes(expected);
        let queue = Queue::new(QueueDescriptor::fixed(std::mem::size_of::<u64>()));
        // SAFETY: `value` is one readable fixed payload.
        assert!(matches!(
            unsafe { queue.post_fixed(&sender, std::ptr::from_ref(&value).cast()) },
            PostResult::Posted
        ));
        let Receive::Message { record, descriptor } = queue.poll() else {
            panic!("fixed message is missing");
        };
        assert_eq!(&*record, &expected);
        assert_eq!(descriptor.payload_size, std::mem::size_of::<u64>());
        assert!(descriptor.string_slot_offsets.is_empty());
    }

    #[repr(C)]
    struct FixedStringMessage {
        tags: [*mut u8; 2],
        count: i32,
    }

    #[test]
    fn fixed_array_string_slots_materialize_from_a_hand_written_record() {
        let mut sender = Context::new();
        let mut receiver = Context::new();
        let left = sender.alloc_str(b"left", 0);
        let right = sender.alloc_str(b"right", 0);
        let mut record = Vec::new();
        record.extend_from_slice(&(left as usize).to_ne_bytes());
        record.extend_from_slice(&(right as usize).to_ne_bytes());
        record.extend_from_slice(&2i32.to_ne_bytes());
        record.extend_from_slice(&[0, 0, 0, 0]);
        record.extend_from_slice(&4u64.to_le_bytes());
        record.extend_from_slice(b"left");
        record.extend_from_slice(&5u64.to_le_bytes());
        record.extend_from_slice(b"right");
        let copy = materialize(
            &mut receiver,
            hand_built_receive(std::mem::size_of::<FixedStringMessage>(), &[0, 8], record),
        )
        .cast::<FixedStringMessage>();
        assert!(!copy.is_null());
        // SAFETY: `copy` is a materialized `FixedStringMessage` allocation.
        let copy = unsafe { &*copy };
        assert_eq!(copy.count, 2);
        for copied in copy.tags {
            assert_ne!(copied, left);
            assert_ne!(copied, right);
        }
        // SAFETY: both copied handles belong to `receiver`.
        assert_eq!(unsafe { receiver.str_bytes(copy.tags[0]) }, b"left");
        assert_eq!(unsafe { receiver.str_bytes(copy.tags[1]) }, b"right");
    }

    fn assert_spawn_descriptor_rejection(
        descriptor: *const WorkerMessageDescriptor,
        expected_message: &str,
    ) {
        let parent = subscript_rt_ctx_new();
        // SAFETY: the invalid input descriptor is deliberate and the output
        // descriptor is readable for this validation call.
        let worker = unsafe {
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(clean_entry),
                descriptor,
                &EMPTY_DESCRIPTOR,
            )
        };
        assert!(worker.is_null());
        // SAFETY: `parent` remains live until the release below.
        let trap = unsafe { &*parent }.trap_record().expect("descriptor trap");
        assert_eq!(trap.kind, TrapKind::Internal);
        assert_eq!(trap.message, expected_message);
        // SAFETY: parent is released exactly once.
        unsafe { subscript_rt_ctx_release(parent) };
    }

    #[test]
    fn spawn_rejects_every_invalid_worker_message_descriptor() {
        assert_spawn_descriptor_rejection(std::ptr::null(), "worker message descriptor is null");

        let unrepresentable_size = WorkerMessageDescriptor {
            payload_size: u64::MAX,
            string_slot_count: 0,
            string_slot_offsets: std::ptr::null(),
        };
        assert_spawn_descriptor_rejection(
            &unrepresentable_size,
            "worker message payload size is not representable",
        );

        let one_offset = [0u64];
        let unrepresentable_count = WorkerMessageDescriptor {
            payload_size: 8,
            string_slot_count: u64::MAX,
            string_slot_offsets: one_offset.as_ptr(),
        };
        assert_spawn_descriptor_rejection(
            &unrepresentable_count,
            "worker message string slot count is not representable",
        );

        let null_offsets = WorkerMessageDescriptor {
            payload_size: 8,
            string_slot_count: 1,
            string_slot_offsets: std::ptr::null(),
        };
        assert_spawn_descriptor_rejection(
            &null_offsets,
            "worker message string slot offsets are null",
        );

        let outside_offsets = [8u64];
        let outside_payload = WorkerMessageDescriptor {
            payload_size: 8,
            string_slot_count: 1,
            string_slot_offsets: outside_offsets.as_ptr(),
        };
        assert_spawn_descriptor_rejection(
            &outside_payload,
            "worker message string slot is outside the payload",
        );

        let descending_offsets = [8u64, 0];
        let not_ascending = WorkerMessageDescriptor {
            payload_size: 16,
            string_slot_count: 2,
            string_slot_offsets: descending_offsets.as_ptr(),
        };
        assert_spawn_descriptor_rejection(
            &not_ascending,
            "worker message string slot offsets are not ascending",
        );
    }

    fn assert_materialize_rejection(record: Vec<u8>, expected_message: &str) {
        let mut receiver = Context::new();
        let copy = materialize(&mut receiver, hand_built_receive(8, &[0], record));
        assert!(copy.is_null());
        let trap = receiver.trap_record().expect("malformed record trap");
        assert_eq!(trap.kind, TrapKind::Internal);
        assert_eq!(trap.message, expected_message);
    }

    #[test]
    fn materialize_rejects_every_hand_built_malformed_string_record() {
        let mut short_length = vec![0; 8];
        short_length.extend_from_slice(&[1, 2, 3]);
        assert_materialize_rejection(short_length, "worker message record has no string length");

        let mut length_past_record = vec![0; 8];
        length_past_record.extend_from_slice(&4u64.to_le_bytes());
        length_past_record.extend_from_slice(b"abc");
        assert_materialize_rejection(
            length_past_record,
            "worker message record has incomplete string bytes",
        );

        let mut trailing_bytes = vec![0; 8];
        trailing_bytes.extend_from_slice(&0u64.to_le_bytes());
        trailing_bytes.push(0xFF);
        assert_materialize_rejection(trailing_bytes, "worker message record has trailing bytes");
    }

    #[test]
    fn materialize_reports_the_p21_injected_allocation_failure() {
        let mut receiver = Context::new();
        let receiver_ptr = std::ptr::from_mut(receiver.as_mut());
        // SAFETY: `receiver_ptr` is the live exclusive Context pointer.
        unsafe { subscript_rt_ctx_fail_alloc_after(receiver_ptr, 1) };
        let mut record = vec![0; 8];
        record.extend_from_slice(&0u64.to_le_bytes());
        let copy = materialize(&mut receiver, hand_built_receive(8, &[0], record));
        assert!(copy.is_null());
        let trap = receiver.trap_record().expect("injected allocation trap");
        assert_eq!(trap.kind, TrapKind::AllocationFailure);
        assert_eq!(trap.message, "injected allocation failure");
    }

    #[test]
    fn c_abi_echo_worker_round_trips_fixed_payload_copies() {
        const N: u64 = 32;

        let parent = subscript_rt_ctx_new();
        let descriptor = fixed_descriptor(std::mem::size_of::<u64>());
        // SAFETY: fresh parent Context and linked callbacks.
        let worker = unsafe {
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(echo_entry),
                &descriptor,
                &descriptor,
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
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(nested_worker_entry),
                &EMPTY_DESCRIPTOR,
                &EMPTY_DESCRIPTOR,
            )
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
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(trap_entry),
                &EMPTY_DESCRIPTOR,
                &EMPTY_DESCRIPTOR,
            )
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
        let descriptor = fixed_descriptor(std::mem::size_of::<usize>());
        // SAFETY: fresh parent Context and linked callbacks.
        let worker = unsafe {
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(poll_entry),
                &descriptor,
                &descriptor,
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
        let descriptor = fixed_descriptor(std::mem::size_of::<u64>());
        // SAFETY: fresh parent Context and linked callbacks.
        let first = unsafe {
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(echo_entry),
                &descriptor,
                &descriptor,
            )
        };
        let second = unsafe {
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(echo_entry),
                &descriptor,
                &descriptor,
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
        let descriptor = fixed_descriptor(std::mem::size_of::<usize>());
        // SAFETY: `parent` remains live while its test stats handle is cloned.
        let stats = unsafe { &*parent }.test_arena_stats();
        // SAFETY: fresh parent Context and linked callbacks.
        let worker = unsafe {
            subscript_rt_worker_spawn(
                parent,
                Some(no_op_init),
                Some(release_probe_entry),
                &descriptor,
                &descriptor,
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
