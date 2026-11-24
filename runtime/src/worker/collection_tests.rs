//! A worker Context takes the collection settings of its parent at the
//! spawn (`compiler.md` §342 rule 18, §356 rule 16), and a worker that
//! ends during a cycle of §356 frees its arena.
//!
//! Cost: one Worker thread for each Context kind, and one Worker thread
//! that allocates under 1 MiB.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// The budget that the worker Context of the last spawn read.
static WORKER_BUDGET: AtomicUsize = AtomicUsize::new(usize::MAX);

unsafe extern "C" fn record_budget(ctx: *mut Context) {
    // SAFETY: the live worker Context of this init.
    WORKER_BUDGET.store(unsafe { &*ctx }.collection_step_budget(), Ordering::SeqCst);
}

unsafe extern "C" fn return_entry(
    _ctx: *mut Context,
    _inbox: *mut WorkerInbox,
    _outbox: *mut WorkerOutbox,
) {
}

#[test]
fn a_worker_context_takes_the_step_budget_of_its_parent() {
    for (mut ctx, budget) in [(Context::new(), 300), (Context::new_releasing(), 700)] {
        WORKER_BUDGET.store(usize::MAX, Ordering::SeqCst);
        assert!(ctx.set_collection_step(budget));
        let worker = ctx.worker_spawn(
            record_budget,
            return_entry,
            QueueDescriptor::fixed(8),
            QueueDescriptor::fixed(8),
            0,
        );
        assert!(!worker.is_null(), "the Worker starts");
        // The drop of the parent joins each Worker.
        drop(ctx);
        assert_eq!(WORKER_BUDGET.load(Ordering::SeqCst), budget);
    }
}

/// The arena balance of the worker Context of the last spawn.
static WORKER_ARENA: std::sync::Mutex<Option<std::sync::Arc<crate::context::ArenaStats>>> =
    std::sync::Mutex::new(None);
/// True if a cycle was in progress when the worker entry returned.
static CYCLE_AT_RETURN: AtomicUsize = AtomicUsize::new(usize::MAX);
/// The chunks of the worker Context when the entry returned.
static CHUNKS_AT_RETURN: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn no_init(_ctx: *mut Context) {}

/// Allocates with polls and a small headroom until a cycle of
/// `compiler.md` §356 starts, and returns during that cycle.
unsafe extern "C" fn cycle_entry(
    ctx: *mut Context,
    _inbox: *mut WorkerInbox,
    _outbox: *mut WorkerOutbox,
) {
    // SAFETY: the live worker Context of this entry.
    let ctx = unsafe { &mut *ctx };
    *WORKER_ARENA.lock().expect("the lock") = Some(ctx.test_arena_stats());
    for _ in 0..20_000 {
        assert!(!ctx.alloc(16, 900, 0).is_null());
        ctx.collect_poll();
        if ctx.mark_in_progress() {
            break;
        }
    }
    CYCLE_AT_RETURN.store(usize::from(ctx.mark_in_progress()), Ordering::SeqCst);
    CHUNKS_AT_RETURN.store(ctx.test_arena_stats().owned_resources().0, Ordering::SeqCst);
}

/// A worker Context that ends during a cycle ends clean, and its drop
/// frees each chunk. Control: the Context holds chunks at the return.
#[test]
fn a_worker_context_that_ends_during_a_cycle_frees_each_allocation() {
    let mut ctx = Context::new_releasing();
    assert!(ctx.set_collection(crate::context::CollectionMode::Automatic, 16 << 10));
    assert!(ctx.set_collection_step(64));
    let worker = ctx.worker_spawn(
        no_init,
        cycle_entry,
        QueueDescriptor::fixed(8),
        QueueDescriptor::fixed(8),
        0,
    );
    assert!(!worker.is_null(), "the Worker starts");
    assert!(ctx.worker_join(worker), "the Worker ends clean");
    assert_eq!(CYCLE_AT_RETURN.load(Ordering::SeqCst), 1);
    assert!(CHUNKS_AT_RETURN.load(Ordering::SeqCst) > 0);
    let arena = WORKER_ARENA
        .lock()
        .expect("the lock")
        .take()
        .expect("stats");
    assert_eq!(arena.owned_resources(), (0, 0));
}
