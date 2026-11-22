//! Runtime tests of `Worker.receive()` (`compiler.md` §349.2 item 5).
//!
//! Each test runs in the dev allocator and in the ship arena. A test never
//! sleeps: it waits on the condition variable of the outbox, or on the
//! signal of rule 11 with a watchdog. A wait that does not end fails the
//! test after five seconds.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use std::ffi::c_void;

use super::*;
use crate::context::{AsyncResume, AsyncTaskInfo, CollectionMode, CLASS_GENERATOR};
use crate::ffi::{
    subscript_rt_ctx_async_wait, subscript_rt_ctx_fail_alloc_after, subscript_rt_trap,
    subscript_rt_worker_inbox_wait, subscript_rt_worker_outbox_post, subscript_rt_worker_receive,
};

const BOUND: Duration = Duration::from_secs(5);
const WORD: usize = std::mem::size_of::<u64>();

/// The two allocator tiers: the dev allocator and the ship arena.
fn contexts() -> [Box<Context>; 2] {
    [Context::new(), Context::new_releasing()]
}

unsafe extern "C" fn no_op_init(_ctx: *mut Context) {}

/// Posts each input message back until the input closes.
unsafe extern "C" fn echo_entry(
    ctx: *mut Context,
    inbox: *mut WorkerInbox,
    outbox: *mut WorkerOutbox,
) {
    loop {
        // SAFETY: the live Context and inbox of this entry.
        let message = unsafe { subscript_rt_worker_inbox_wait(ctx, inbox) };
        if message.is_null() {
            return;
        }
        // SAFETY: the input and output payloads are both eight bytes.
        if unsafe { subscript_rt_worker_outbox_post(ctx, outbox, message) } == 0 {
            return;
        }
    }
}

/// Posts the first input message, then traps at position 77.
unsafe extern "C" fn post_then_trap_entry(
    ctx: *mut Context,
    inbox: *mut WorkerInbox,
    outbox: *mut WorkerOutbox,
) {
    // SAFETY: the live Context and endpoints of this entry.
    unsafe {
        let message = subscript_rt_worker_inbox_wait(ctx, inbox);
        if message.is_null() {
            return;
        }
        let _ = subscript_rt_worker_outbox_post(ctx, outbox, message);
        subscript_rt_trap(ctx, TrapKind::EmptyPop as u32, 77);
    }
}

fn spawn(ctx: &mut Context, entry: WorkerEntry) -> *mut Worker {
    let worker = ctx.worker_spawn(
        no_op_init,
        entry,
        QueueDescriptor::fixed(WORD),
        QueueDescriptor::fixed(WORD),
        0,
    );
    assert!(!worker.is_null(), "the Worker starts");
    worker
}

fn post(ctx: &mut Context, worker: *mut Worker, value: u64) {
    // SAFETY: the payload is eight readable bytes, as the descriptor states.
    let posted = unsafe { ctx.worker_post(worker, std::ptr::from_ref(&value).cast()) };
    assert!(posted, "the input accepts the message");
}

/// Waits on the condition variable of the outbox until `done` holds.
fn wait_outbox(worker: *mut Worker, done: impl Fn(&QueueState) -> bool) {
    // SAFETY: the Context that owns the Worker is live and runs no call.
    let queue = &unsafe { &*worker }.output;
    let state = queue.lock();
    let (_state, wait) = match queue.ready.wait_timeout_while(state, BOUND, |s| !done(s)) {
        Ok(result) => result,
        Err(poisoned) => poisoned.into_inner(),
    };
    assert!(!wait.timed_out(), "the Worker did not reach the state");
}

fn wait_messages(worker: *mut Worker, count: usize) {
    wait_outbox(worker, |state| state.messages.len() >= count);
}

fn wait_closed(worker: *mut Worker) {
    wait_outbox(worker, |state| state.closed);
}

/// The completion of a receive: `None` if it has none, else the message
/// value, with `Some(None)` for null.
fn received(ctx: &mut Context, handle: *mut u8) -> Option<Option<u64>> {
    let mut message = 0usize;
    // SAFETY: the output holds the one-word completion.
    let done = unsafe { ctx.async_result(handle, std::ptr::from_mut(&mut message).cast(), WORD) };
    if !done {
        return None;
    }
    if message == 0 {
        return Some(None);
    }
    assert!(ctx.is_live(message), "the message is a live allocation");
    // SAFETY: a materialized message is an eight-byte Context allocation.
    Some(Some(unsafe { (message as *const u64).read() }))
}

fn message_address(ctx: &mut Context, handle: *mut u8) -> usize {
    let mut message = 0usize;
    // SAFETY: the output holds the one-word completion.
    assert!(unsafe { ctx.async_result(handle, std::ptr::from_mut(&mut message).cast(), WORD) });
    message
}

fn step(ctx: &mut Context) -> usize {
    // SAFETY: each queued job in these tests has a live resume function.
    unsafe { ctx.async_step() }
}

fn release(ctx: &mut Context, handle: *mut u8) {
    // SAFETY: the test owns one count of `handle`.
    unsafe { ctx.async_release(handle, 0) };
}

/// Sets the signal after `BOUND` unless the test ends first, so a wait
/// that never returns ends and the test fails on `fired`.
struct Watchdog {
    done: Option<mpsc::Sender<()>>,
    fired: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Watchdog {
    fn new(worker: *mut Worker) -> Self {
        // SAFETY: the Context that owns the Worker is live.
        let signal = Arc::clone(
            unsafe { &*worker }
                .output
                .signal
                .as_ref()
                .expect("an outbox has the signal"),
        );
        let fired = Arc::new(AtomicBool::new(false));
        let (done, wait) = mpsc::channel();
        let thread = {
            let fired = Arc::clone(&fired);
            std::thread::spawn(move || {
                if wait.recv_timeout(BOUND).is_err() {
                    fired.store(true, Ordering::SeqCst);
                    signal.set();
                }
            })
        };
        Self {
            done: Some(done),
            fired,
            thread: Some(thread),
        }
    }

    fn fired(&self) -> bool {
        self.fired.load(Ordering::SeqCst)
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        drop(self.done.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Rule 2: a message in the outbox completes the receive at the call.
#[test]
fn a_message_in_the_outbox_completes_the_receive_at_the_call() {
    for mut ctx in contexts() {
        let worker = spawn(&mut ctx, echo_entry);
        post(&mut ctx, worker, 41);
        wait_messages(worker, 1);
        let handle = ctx.worker_receive(worker, 7);
        assert!(!handle.is_null());
        assert_eq!(received(&mut ctx, handle), Some(Some(41)));
        // SAFETY: the handle is a registered frame.
        assert_eq!(unsafe { ctx.async_count(handle) }, 1);
        assert_eq!(ctx.async_unfinished(), 0);
        // Control: with an empty outbox, the receive has no completion.
        let waits = ctx.worker_receive(worker, 8);
        assert_eq!(received(&mut ctx, waits), None);
        assert_eq!(ctx.async_unfinished(), 1);
        release(&mut ctx, handle);
        release(&mut ctx, waits);
    }
}

/// Rule 3: receives that wait get the messages in call sequence.
#[test]
fn receives_that_wait_get_the_messages_in_call_sequence() {
    for mut ctx in contexts() {
        let worker = spawn(&mut ctx, echo_entry);
        let first = ctx.worker_receive(worker, 1);
        let second = ctx.worker_receive(worker, 2);
        post(&mut ctx, worker, 10);
        post(&mut ctx, worker, 20);
        wait_messages(worker, 2);
        assert_eq!(received(&mut ctx, first), None);
        step(&mut ctx);
        assert_eq!(received(&mut ctx, first), Some(Some(10)));
        assert_eq!(received(&mut ctx, second), Some(Some(20)));
        release(&mut ctx, first);
        release(&mut ctx, second);
    }
}

#[repr(C)]
struct Waiter {
    state: i32,
    count: u32,
    resume: AsyncResume,
    awaited: *mut u8,
    seen: u64,
}

/// Reads the message of the awaited receive, then ends the await count.
unsafe extern "C" fn read_receive(ctx: *mut Context, frame: *mut u8, _out: *mut u8) -> u8 {
    // SAFETY: the test registers this frame with this resume function.
    let ctx = unsafe { &mut *ctx };
    let waiter = frame.cast::<Waiter>();
    // SAFETY: the frame is a live `Waiter`.
    let awaited = unsafe { (*waiter).awaited };
    let seen = received(ctx, awaited).flatten().unwrap_or(u64::MAX);
    // SAFETY: the frame is a live `Waiter`, and the await holds one count.
    unsafe {
        (*waiter).seen = seen;
        ctx.async_release(awaited, 0);
    }
    1
}

/// Rules 4 and 10, with the wait of rule 11: a loop that steps only while
/// the pending count is not zero delivers a message that the Worker posts
/// after the first step, and the waiter of the receive runs in that step.
#[test]
fn a_loop_on_the_pending_count_and_the_wait_delivers_a_later_message() {
    for mut ctx in contexts() {
        let worker = spawn(&mut ctx, echo_entry);
        let watchdog = Watchdog::new(worker);
        let handle = ctx.worker_receive(worker, 3);
        let waiter = ctx.alloc(std::mem::size_of::<Waiter>(), CLASS_GENERATOR, 0);
        // SAFETY: a fresh frame with the `Waiter` layout and resume function.
        unsafe {
            waiter.cast::<Waiter>().write(Waiter {
                state: 0,
                count: 0,
                resume: read_receive,
                awaited: handle,
                seen: 0,
            });
            ctx.async_register(waiter, 0);
            ctx.async_await(waiter, handle, 0);
        }
        // Control: the receive waits and the signal is clear.
        assert_eq!(ctx.async_pending(), 0);
        let mut steps = 0;
        let mut waits = 0;
        // A driver of rule 11 waits once for one post. The bound makes a
        // wait that returns with nothing to deliver fail the test.
        for _ in 0..4 {
            while ctx.async_pending() != 0 {
                assert!(steps < 16, "a step that delivers nothing clears the signal");
                step(&mut ctx);
                steps += 1;
            }
            if steps == 0 {
                step(&mut ctx);
                steps += 1;
                post(&mut ctx, worker, 99);
            }
            if received(&mut ctx, handle).is_some() {
                break;
            }
            // SAFETY: the Context is live.
            if unsafe { subscript_rt_ctx_async_wait(&*ctx) } == 0 {
                break;
            }
            waits += 1;
        }
        assert!(!watchdog.fired(), "the wait returned on the post");
        assert_eq!(received(&mut ctx, handle), Some(Some(99)));
        // SAFETY: the waiter ran, so it is a live `Waiter`.
        assert_eq!(unsafe { (*waiter.cast::<Waiter>()).seen }, 99);
        assert!(waits <= 1, "one wait at most: {waits}");
        assert_eq!(ctx.async_pending(), 0);
        release(&mut ctx, handle);
        release(&mut ctx, waiter);
    }
}

/// Rule 10: a step that delivers nothing clears the signal. The firing
/// control is the pending count of one before that step.
#[test]
fn a_step_that_delivers_nothing_clears_the_signal() {
    for mut ctx in contexts() {
        let waited = spawn(&mut ctx, echo_entry);
        let other = spawn(&mut ctx, echo_entry);
        let handle = ctx.worker_receive(waited, 0);
        assert_eq!(ctx.async_pending(), 0);
        post(&mut ctx, other, 5);
        wait_messages(other, 1);
        assert_eq!(ctx.async_pending(), 1);
        assert_eq!(step(&mut ctx), 0);
        assert_eq!(ctx.async_pending(), 0);
        assert_eq!(received(&mut ctx, handle), None);
        // The message of the other Worker stays for its poll().
        assert_eq!(read_poll(&mut ctx, other), Some(5));
        release(&mut ctx, handle);
    }
}

fn read_poll(ctx: &mut Context, worker: *mut Worker) -> Option<u64> {
    let message = ctx.worker_poll(worker);
    // SAFETY: a materialized message is an eight-byte Context allocation.
    (!message.is_null()).then(|| unsafe { message.cast::<u64>().read() })
}

/// Rule 5 at the call and at a step, and after `close()`.
#[test]
fn an_ended_worker_with_an_empty_outbox_gives_null() {
    for mut ctx in contexts() {
        // At a step: three receives wait, the Worker echoes two messages and
        // ends after `close()`.
        let worker = spawn(&mut ctx, echo_entry);
        let handles: Vec<_> = (0..3).map(|n| ctx.worker_receive(worker, n)).collect();
        post(&mut ctx, worker, 1);
        post(&mut ctx, worker, 2);
        ctx.worker_close(worker);
        wait_closed(worker);
        step(&mut ctx);
        let values: Vec<_> = handles.iter().map(|&h| received(&mut ctx, h)).collect();
        assert_eq!(values, [Some(Some(1)), Some(Some(2)), Some(None)]);
        // At the call: the outbox is empty and the Worker ended.
        let after = ctx.worker_receive(worker, 4);
        assert_eq!(received(&mut ctx, after), Some(None));
        for handle in handles.into_iter().chain([after]) {
            release(&mut ctx, handle);
        }
        // Control: the same Worker shape before its end, at the call.
        let running = spawn(&mut ctx, echo_entry);
        let waits = ctx.worker_receive(running, 5);
        assert_eq!(received(&mut ctx, waits), None);
        release(&mut ctx, waits);
    }
}

fn worker_trap(ctx: &Context) -> (TrapKind, u32, String) {
    let trap = ctx.trap_record().expect("the Context traps");
    (trap.kind, trap.pos_id, trap.message.clone())
}

/// The trap that `join()` gives for `post_then_trap_entry`.
fn trap_of_join() -> (TrapKind, u32, String) {
    let mut ctx = Context::new();
    let worker = spawn(&mut ctx, post_then_trap_entry);
    post(&mut ctx, worker, 5);
    assert!(!ctx.worker_join(worker));
    worker_trap(&ctx)
}

/// Rule 6: the message first, then the trap of `join()`, at the call and
/// at a step.
#[test]
fn a_trapped_worker_with_an_empty_outbox_traps_as_join_does() {
    let join = trap_of_join();
    assert_eq!(join.0, TrapKind::WorkerTrapped);
    assert_eq!(join.1, 77);
    for at_step in [false, true] {
        for mut ctx in contexts() {
            let worker = spawn(&mut ctx, post_then_trap_entry);
            let (first, second) = if at_step {
                let first = ctx.worker_receive(worker, 1);
                let second = ctx.worker_receive(worker, 2);
                post(&mut ctx, worker, 5);
                wait_closed(worker);
                step(&mut ctx);
                (first, second)
            } else {
                post(&mut ctx, worker, 5);
                wait_closed(worker);
                let first = ctx.worker_receive(worker, 1);
                assert!(ctx.trap_record().is_none(), "the message comes first");
                (first, ctx.worker_receive(worker, 2))
            };
            assert_eq!(received(&mut ctx, first), Some(Some(5)));
            assert_eq!(worker_trap(&ctx), join);
            if at_step {
                assert_eq!(received(&mut ctx, second), None);
            } else {
                assert!(second.is_null());
            }
        }
    }
}

/// Rule 2: a receive does not complete at its call while an earlier
/// receive of the same Worker waits, even if the outbox holds messages.
#[test]
fn a_receive_waits_at_its_call_while_an_earlier_receive_waits() {
    for mut ctx in contexts() {
        let worker = spawn(&mut ctx, echo_entry);
        let first = ctx.worker_receive(worker, 1);
        post(&mut ctx, worker, 1);
        post(&mut ctx, worker, 2);
        wait_messages(worker, 2);
        let second = ctx.worker_receive(worker, 2);
        assert_eq!(received(&mut ctx, second), None, "B waits behind A");
        step(&mut ctx);
        assert_eq!(received(&mut ctx, first), Some(Some(1)));
        assert_eq!(received(&mut ctx, second), Some(Some(2)));
        release(&mut ctx, first);
        release(&mut ctx, second);
    }
}

/// Rule 6 after the host clears the trap: the pending count is not zero,
/// and the next step traps again with the same record.
#[test]
fn a_step_after_a_cleared_worker_trap_traps_again() {
    for mut ctx in contexts() {
        let worker = spawn(&mut ctx, post_then_trap_entry);
        let first = ctx.worker_receive(worker, 1);
        let second = ctx.worker_receive(worker, 2);
        post(&mut ctx, worker, 5);
        wait_closed(worker);
        step(&mut ctx);
        assert_eq!(received(&mut ctx, first), Some(Some(5)));
        let trap = worker_trap(&ctx);
        assert_eq!(trap.0, TrapKind::WorkerTrapped);
        ctx.clear_trap();
        assert!(ctx.trap_record().is_none());
        assert_eq!(ctx.async_pending(), 1, "the receive can complete");
        step(&mut ctx);
        assert_eq!(worker_trap(&ctx), trap);
        assert_eq!(received(&mut ctx, second), None);
        ctx.clear_trap();
        release(&mut ctx, first);
        release(&mut ctx, second);
    }
}

/// Rule 13: a message whose materialization traps at a step stays the
/// oldest message, and the first step after the trap is cleared delivers it.
#[test]
fn a_message_that_does_not_materialize_at_a_step_is_kept() {
    for mut ctx in contexts() {
        let worker = spawn(&mut ctx, echo_entry);
        let handle = ctx.worker_receive(worker, 1);
        post(&mut ctx, worker, 8);
        wait_messages(worker, 1);
        let ctx_ptr = std::ptr::from_mut(ctx.as_mut());
        // SAFETY: the live Context runs no call.
        unsafe { subscript_rt_ctx_fail_alloc_after(ctx_ptr, 1) };
        step(&mut ctx);
        assert_eq!(worker_trap(&ctx).0, TrapKind::AllocationFailure);
        assert_eq!(received(&mut ctx, handle), None);
        ctx.clear_trap();
        assert_eq!(ctx.async_pending(), 1);
        step(&mut ctx);
        assert!(ctx.trap_record().is_none());
        assert_eq!(received(&mut ctx, handle), Some(Some(8)));
        release(&mut ctx, handle);
    }
}

/// Rule 7: `poll()` gives null while a receive waits, and the next message
/// after that receive completes.
#[test]
fn poll_gives_null_while_a_receive_waits() {
    for mut ctx in contexts() {
        let worker = spawn(&mut ctx, echo_entry);
        let handle = ctx.worker_receive(worker, 0);
        post(&mut ctx, worker, 1);
        wait_messages(worker, 1);
        assert_eq!(read_poll(&mut ctx, worker), None);
        step(&mut ctx);
        assert_eq!(received(&mut ctx, handle), Some(Some(1)));
        post(&mut ctx, worker, 2);
        wait_messages(worker, 1);
        // Control: with no receive that waits, poll() takes the message.
        assert_eq!(read_poll(&mut ctx, worker), Some(2));
        release(&mut ctx, handle);
    }
}

unsafe extern "C" fn collect_info(userdata: *mut c_void, info: *const AsyncTaskInfo) {
    // SAFETY: the test passes a `Vec<AsyncTaskInfo>` and a live record.
    unsafe { (*userdata.cast::<Vec<AsyncTaskInfo>>()).push(*info) };
}

fn tasks(ctx: &Context) -> Vec<(u32, u32, u32)> {
    let mut infos: Vec<AsyncTaskInfo> = Vec::new();
    // SAFETY: the visitor matches its userdata and changes no Context.
    unsafe { ctx.visit_async_tasks(Some(collect_info), std::ptr::from_mut(&mut infos).cast()) };
    infos
        .iter()
        .map(|info| (info.kind, info.state, info.create_pos_id))
        .collect()
}

/// Rule 9: the kind number, the unfinished count, and the frame as a root
/// of a collection, explicit and in mode 2.
#[test]
fn a_receive_is_a_frame_of_its_own_kind_and_a_root() {
    for mut ctx in contexts() {
        let worker = spawn(&mut ctx, echo_entry);
        let handle = ctx.worker_receive(worker, 12);
        assert_eq!(tasks(&ctx), [(5, 3, 12)]);
        assert_eq!(ctx.async_unfinished(), 1);
        ctx.collect();
        assert!(
            ctx.is_live(handle as usize),
            "an explicit collection keeps it"
        );
        assert!(ctx.set_collection(CollectionMode::EachPoll, 0));
        ctx.enter_script();
        ctx.exit_script();
        assert!(ctx.is_live(handle as usize), "mode 2 keeps it");
        // The step collects at its exit in mode 2, after the delivery.
        post(&mut ctx, worker, 3);
        wait_messages(worker, 1);
        step(&mut ctx);
        assert_eq!(tasks(&ctx), [(5, 5, 12)]);
        assert_eq!(ctx.async_unfinished(), 0);
        let message = message_address(&mut ctx, handle);
        ctx.collect();
        assert!(ctx.is_live(message), "the completion keeps the message");
        assert_eq!(received(&mut ctx, handle), Some(Some(3)));
        // Firing control: after the release, the same collection reclaims
        // the frame and the message.
        release(&mut ctx, handle);
        ctx.collect();
        assert!(!ctx.is_live(handle as usize));
        assert!(!ctx.is_live(message));
        assert!(ctx.trap_record().is_none());
    }
}

/// §168 rule 5: a step with a budget of zero delivers nothing.
#[test]
fn a_step_with_a_budget_of_zero_delivers_nothing() {
    for mut ctx in contexts() {
        let worker = spawn(&mut ctx, echo_entry);
        let handle = ctx.worker_receive(worker, 0);
        post(&mut ctx, worker, 6);
        wait_messages(worker, 1);
        // SAFETY: no job is queued.
        let report = unsafe { ctx.async_step_budget(0) };
        assert_eq!((report.dispatched, report.pending), (0, 1));
        assert_eq!(received(&mut ctx, handle), None);
        // Control: a budget of one delivers.
        // SAFETY: no job is queued.
        let report = unsafe { ctx.async_step_budget(1) };
        assert_eq!((report.dispatched, report.pending), (0, 0));
        assert_eq!(received(&mut ctx, handle), Some(Some(6)));
        release(&mut ctx, handle);
    }
}

/// Rule 11: the wait gives 0 at once with no receive, and 1 after a post
/// from the Worker thread.
#[test]
fn the_wait_returns_at_once_with_no_receive_and_after_a_post() {
    for mut ctx in contexts() {
        let worker = spawn(&mut ctx, echo_entry);
        let watchdog = Watchdog::new(worker);
        // A post sets the signal, but no receive waits.
        post(&mut ctx, worker, 1);
        wait_messages(worker, 1);
        assert!(!ctx.async_wait());
        assert_eq!(read_poll(&mut ctx, worker), Some(1));
        let handle = ctx.worker_receive(worker, 0);
        // Clear the signal of the earlier post with a step.
        step(&mut ctx);
        assert_eq!(ctx.async_pending(), 0);
        post(&mut ctx, worker, 2);
        assert!(ctx.async_wait());
        assert!(!watchdog.fired(), "the wait returned on the post");
        assert_eq!(ctx.async_pending(), 1);
        step(&mut ctx);
        assert_eq!(received(&mut ctx, handle), Some(Some(2)));
        release(&mut ctx, handle);
        // A trapped Context does not wait.
        let _waits = ctx.worker_receive(worker, 0);
        ctx.trap(TrapKind::Internal, "test trap", 0);
        assert!(!ctx.async_wait());
    }
}

/// The release of a Context with receives that wait closes and joins each
/// Worker and frees each frame.
#[test]
fn a_context_with_receives_that_wait_releases_its_workers() {
    let mut ctx = Context::new_releasing();
    let stats = ctx.test_arena_stats();
    let worker = spawn(&mut ctx, echo_entry);
    let _first = ctx.worker_receive(worker, 0);
    let _second = ctx.worker_receive(worker, 0);
    assert_ne!(stats.owned_resources(), (0, 0));
    drop(ctx);
    assert_eq!(stats.owned_resources(), (0, 0));
}

/// The C entry forwards to `Context::worker_receive`.
#[test]
fn the_c_entry_starts_a_receive() {
    let mut ctx = Context::new();
    let worker = spawn(&mut ctx, echo_entry);
    let ctx_ptr = std::ptr::from_mut(ctx.as_mut());
    // SAFETY: the live Context owns the Worker.
    let handle = unsafe { subscript_rt_worker_receive(ctx_ptr, worker, 9) };
    assert_eq!(tasks(&ctx), [(5, 3, 9)]);
    // Control: a handle that the Context does not own traps.
    // SAFETY: the Context is live; the Worker pointer is not one of it.
    let foreign = unsafe { subscript_rt_worker_receive(ctx_ptr, std::ptr::dangling_mut(), 9) };
    assert!(foreign.is_null());
    assert_eq!(worker_trap(&ctx).0, TrapKind::Internal);
    release(&mut ctx, handle);
}
