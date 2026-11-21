//! Automatic collection of a Context (`compiler.md` §342 rules 5, 7, 8,
//! and 12 to 18, and §346). Each "does not happen" test has a firing control in the same
//! shape.

use subscript_runtime::context::{
    CollectionMode, CLASS_GENERATOR, DEFAULT_MIN_HEADROOM_BYTES, MAX_DETECTION_INTERVAL,
};
use subscript_runtime::{ffi::*, Context, TrapKind};

fn context(ship: bool) -> Box<Context> {
    if ship {
        Context::new_releasing()
    } else {
        Context::new()
    }
}

/// Allocates `count` garbage objects of `size` bytes that no root reaches.
fn garbage(ctx: &mut Context, count: usize, size: usize) {
    for _ in 0..count {
        assert!(!ctx.alloc(size, 900, 0).is_null());
    }
}

/// A failed task whose only owner is a counted holder object. The throw
/// position is `40 + index`, and the message is `Error: <index>`.
struct FailedTask {
    frame: usize,
    error: usize,
    holder: usize,
}

fn failed_task(ctx: &mut Context, index: u32) -> FailedTask {
    let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
    let error = ctx.alloc(24, 1, 0);
    let holder = ctx.alloc(8, 900, 0);
    let description: [u64; 6] = [1, 0, 24, 1, 0, 0];
    // SAFETY: the frame has a complete header, the error is a live handle
    // that exception roots retain, and one handle field fits the holder.
    unsafe {
        ctx.async_register(frame, 0);
        ctx.raise_exception(error, format!("Error: {index}"), 40 + index);
        ctx.async_complete_exception(frame);
        holder.cast::<*mut u8>().write(frame);
        ctx.describe_object(
            holder as usize,
            std::slice::from_raw_parts(description.as_ptr().cast(), 48),
        );
    }
    FailedTask {
        frame: frame as usize,
        error: error as usize,
        holder: holder as usize,
    }
}

/// One root word. Setting it to zero makes its object unreachable.
struct RootSlot(*mut usize);

impl RootSlot {
    fn new(ctx: &mut Context, value: usize) -> Self {
        let slot = Box::into_raw(Box::new(value));
        // SAFETY: the slot stays allocated for the test; the Context drops
        // before the process ends.
        unsafe { ctx.root_add(slot as usize, 1) };
        Self(slot)
    }

    fn set(&self, value: usize) {
        // SAFETY: the slot is a live, exclusive allocation of this test.
        unsafe { self.0.write(value) };
    }
}

fn cycle(ctx: &mut Context, count: usize, size: usize) {
    ctx.enter_script();
    garbage(ctx, count, size);
    ctx.exit_script();
}

/// `compiler.md` §346 rule 2 and §342 rule 5: an allocation that takes the
/// requested bytes over the limit sets the flag and frees nothing.
/// Control: mode 0 never sets the flag.
#[test]
fn an_allocation_over_the_limit_sets_the_flag_and_does_not_collect() {
    for ship in [false, true] {
        for mode in [CollectionMode::Automatic, CollectionMode::Off] {
            let mut ctx = context(ship);
            assert!(ctx.set_collection(mode, 4096));
            let mut set_at = None;
            for index in 1..=200 {
                let before = ctx.live_count();
                garbage(&mut ctx, 1, 64);
                assert_eq!(
                    ctx.live_count(),
                    before + 1,
                    "an allocation freed an object"
                );
                let over = ctx.requested_bytes() > ctx.collection_limit();
                if mode == CollectionMode::Automatic {
                    assert_eq!(ctx.collection_pending(), over || set_at.is_some());
                }
                if ctx.collection_pending() && set_at.is_none() {
                    set_at = Some(index);
                }
            }
            assert_eq!(ctx.live_count(), 200);
            assert_eq!(set_at.is_some(), mode == CollectionMode::Automatic);
            assert_eq!(ctx.automatic_collection_count(), 0);
        }
    }
}

/// The pending flag is the byte at the stated offset of the Context.
#[test]
fn the_pending_flag_is_the_byte_at_its_offset() {
    let mut ctx = Context::new();
    let read = |ctx: &Context| {
        // SAFETY: the offset is inside the Context prefix.
        unsafe {
            (ctx as *const Context)
                .cast::<u8>()
                .add(Context::collection_pending_offset())
                .read()
        }
    };
    assert!(ctx.set_collection(CollectionMode::Automatic, 1024));
    assert_eq!(read(&ctx), 0);
    garbage(&mut ctx, 40, 64);
    assert!(ctx.collection_pending());
    assert_eq!(read(&ctx), 1);
    ctx.collect_automatic();
    assert_eq!(read(&ctx), 0);
}

/// `compiler.md` §346 rule 2: the limit after a collection, for a live set
/// under and over the minimum headroom. The collection sets the requested
/// bytes to zero (§346 rule 1).
#[test]
fn the_limit_follows_the_live_set_after_a_collection() {
    for ship in [false, true] {
        for (count, headroom) in [(10usize, 65_536usize), (2_000, 4_096)] {
            let mut ctx = context(ship);
            assert!(ctx.set_collection(CollectionMode::Automatic, headroom));
            let mut handles: Vec<usize> = (0..count)
                .map(|_| ctx.alloc(100, 900, 0) as usize)
                .collect();
            // SAFETY: the vector stays allocated and unmoved while the Context lives.
            unsafe { ctx.root_add(handles.as_mut_ptr() as usize, handles.len()) };
            garbage(&mut ctx, 50, 100);
            ctx.collect_automatic();
            let live = ctx.live_bytes();
            assert_eq!(ctx.live_count(), count);
            assert_eq!(ctx.collection_limit(), live.max(headroom));
            if count == 10 {
                assert!(live < headroom);
                assert_eq!(ctx.collection_limit(), headroom);
            } else {
                assert!(live > headroom);
                assert_eq!(ctx.collection_limit(), live);
            }
            assert_eq!(ctx.requested_bytes(), 0);
            assert!(!ctx.collection_pending());
            // An explicit collection sets the limit by the same rule.
            garbage(&mut ctx, 50, 100);
            assert_eq!(ctx.requested_bytes(), 5_000);
            ctx.collect();
            assert_eq!(ctx.collection_limit(), live.max(headroom));
            assert_eq!(ctx.requested_bytes(), 0);
            drop(ctx);
            drop(handles);
        }
    }
}

/// `compiler.md` §342 rule 8, §346 rule 2a, and §346 acceptance 3: 10,000
/// host calls that allocate garbage. After each outermost exit (a) the
/// requested bytes are at most the limit plus the request of one call, the
/// same bound in each Context kind, and (b) live bytes are at most the live
/// bytes after the last collection plus the bytes that the tier holds for
/// `limit / request + 1` calls. The ship tier holds the class capacity, so
/// its live bytes can pass the limit. Control: mode 0 grows with the call
/// count.
#[test]
fn the_outermost_exit_keeps_the_requested_bytes_under_the_limit_plus_one_call() {
    const CALLS: usize = 10_000;
    const HEADROOM: usize = 16_384;
    const OBJECTS: usize = 4;
    const SIZE: usize = 256;
    const REQUEST: usize = OBJECTS * SIZE;
    let mut highest_requested = Vec::new();
    for ship in [false, true] {
        // The bytes that the tier holds for one call, read in a control
        // Context with no collection.
        let held = {
            let mut control = context(ship);
            assert!(control.set_collection(CollectionMode::Off, HEADROOM));
            let before = control.live_bytes();
            cycle(&mut control, OBJECTS, SIZE);
            control.live_bytes() - before
        };
        if ship {
            assert!(held > REQUEST, "the ship tier holds the class capacity");
        } else {
            assert_eq!(held, REQUEST, "the dev tier holds the request");
        }
        for mode in [CollectionMode::Automatic, CollectionMode::Off] {
            let mut ctx = context(ship);
            assert!(ctx.set_collection(mode, HEADROOM));
            let mut base = ctx.live_bytes();
            let mut collections = ctx.automatic_collection_count();
            let mut requested_max = 0;
            let mut live_max = 0;
            for _ in 0..CALLS {
                cycle(&mut ctx, OBJECTS, SIZE);
                if ctx.automatic_collection_count() != collections {
                    collections = ctx.automatic_collection_count();
                    base = ctx.live_bytes();
                }
                if mode == CollectionMode::Automatic {
                    let limit = ctx.collection_limit();
                    // (a) compiler.md §346 rule 2: the count passes the
                    // limit by at most one call before the exit collects.
                    assert!(ctx.requested_bytes() <= limit + REQUEST);
                    // (b) compiler.md §346 rule 2a: at most this many calls
                    // run between two collections.
                    let calls_between_collections_max = limit / REQUEST + 1;
                    assert!(ctx.live_bytes() <= base + calls_between_collections_max * held);
                }
                requested_max = requested_max.max(ctx.requested_bytes());
                live_max = live_max.max(ctx.live_bytes());
            }
            if mode == CollectionMode::Off {
                // (c) The control: every call stays.
                assert_eq!(ctx.live_bytes(), CALLS * held);
                assert_eq!(ctx.automatic_collection_count(), 0);
            } else {
                println!(
                    "ship {ship}: held {held} per call, highest requested {requested_max} \
                     (bound {}), highest live {live_max} (bound {}), {} collections",
                    HEADROOM + REQUEST,
                    (HEADROOM / REQUEST + 1) * held,
                    ctx.automatic_collection_count()
                );
                assert!(ctx.automatic_collection_count() > 0);
                highest_requested.push(requested_max);
            }
            assert_eq!(ctx.outermost_exit_count(), CALLS as u64);
        }
    }
    // The count is the same number in each Context kind.
    assert_eq!(highest_requested[0], highest_requested[1]);
}

/// Rules 12 and 13: an automatic collection records and does not trap; a
/// later explicit collection reports the lowest task id, also against a
/// failure that it finds itself.
#[test]
fn an_automatic_collection_records_and_an_explicit_collection_reports() {
    for ship in [false, true] {
        // (first released, expected report, explicit at a script position)
        for (first, at) in [(1usize, false), (0, false), (1, true), (0, true)] {
            let mut ctx = context(ship);
            assert!(ctx.set_collection(CollectionMode::Automatic, 0));
            let tasks = [failed_task(&mut ctx, 0), failed_task(&mut ctx, 1)];
            let slots = tasks
                .each_ref()
                .map(|task| RootSlot::new(&mut ctx, task.holder));
            slots[first].set(0);
            ctx.collect_automatic();
            assert!(!ctx.trapped(), "an automatic collection trapped");
            assert_eq!(ctx.recorded_failure_count(), 1);
            assert!(!ctx.is_live(tasks[first].frame));
            assert!(
                ctx.is_live(tasks[first].error),
                "the recorded error is not a root"
            );
            assert_eq!(ctx.automatic_collection_count(), 1);
            // The other task: released by the explicit collection itself.
            slots[1 - first].set(0);
            if at {
                // SAFETY: the Context is live and exclusive.
                unsafe { subscript_rt_collect_at(&mut *ctx, 99) };
            } else {
                // SAFETY: the Context is live and exclusive.
                unsafe { subscript_rt_ctx_collect(&mut *ctx) };
            }
            let trap = ctx.trap_record().expect("the explicit collection reports");
            assert_eq!(trap.kind, TrapKind::UncaughtException);
            assert_eq!(trap.message, "Error: 0");
            assert_eq!(trap.pos_id, if at { 99 } else { 40 });
            assert_eq!(ctx.recorded_failure_count(), 0);
        }
    }
}

/// Rule 13: an explicit collection that finds nothing reports the recorded
/// failure at the throw position. Control: with no recorded failure it does
/// not trap.
#[test]
fn an_explicit_collection_reports_a_recorded_failure_alone() {
    for record in [true, false] {
        let mut ctx = Context::new();
        assert!(ctx.set_collection(CollectionMode::Automatic, 0));
        let task = failed_task(&mut ctx, 3);
        let slot = RootSlot::new(&mut ctx, task.holder);
        if record {
            slot.set(0);
        }
        ctx.collect_automatic();
        assert!(!ctx.trapped());
        ctx.collect();
        assert_eq!(ctx.trapped(), record);
        if record {
            let trap = ctx.trap_record().expect("trap");
            assert_eq!((trap.message.as_str(), trap.pos_id), ("Error: 3", 43));
        }
    }
}

/// Rules 14 and 15 (a): an unreachable holder of a failed task traps at
/// the first outermost exit, at the throw position. Control: mode 0 does
/// not trap.
#[test]
fn an_unreachable_failed_task_traps_at_the_first_outermost_exit() {
    for ship in [false, true] {
        for mode in [CollectionMode::Automatic, CollectionMode::Off] {
            let mut ctx = context(ship);
            assert!(ctx.set_collection(mode, 0));
            ctx.enter_script();
            ctx.enter_script();
            let _ = failed_task(&mut ctx, 2);
            ctx.exit_script();
            assert!(!ctx.trapped(), "an inner exit collected");
            ctx.exit_script();
            if mode == CollectionMode::Off {
                assert!(!ctx.trapped());
                assert_eq!(ctx.detection_collection_count(), 0);
                continue;
            }
            let trap = ctx.trap_record().expect("the detection collection reports");
            assert_eq!(trap.kind, TrapKind::UncaughtException);
            assert_eq!((trap.message.as_str(), trap.pos_id), ("Error: 2", 42));
            assert_eq!(ctx.detection_collection_count(), 1);
        }
    }
}

/// Rules 14 and 15 (b), acceptance 5 at the runtime level: a failed handle
/// that stays reachable for 2,000 exits gives at most 11 detection
/// collections and no trap. (c): the interval returns to 1 when no failed
/// unobserved task remains.
#[test]
fn a_reachable_failed_task_backs_off_and_the_interval_returns_to_one() {
    for ship in [false, true] {
        let mut ctx = context(ship);
        assert!(ctx.set_collection(CollectionMode::Automatic, 0));
        let task = failed_task(&mut ctx, 5);
        let slot = RootSlot::new(&mut ctx, task.holder);
        for _ in 0..2_000 {
            ctx.enter_script();
            ctx.exit_script();
        }
        assert!(!ctx.trapped());
        assert!(ctx.is_live(task.frame));
        // Exits 1, 3, 7, ..., 1023.
        assert_eq!(ctx.detection_collection_count(), 10);
        assert!(ctx.detection_collection_count() <= 11);
        assert_eq!(ctx.detection_interval(), MAX_DETECTION_INTERVAL);
        // (c) The explicit collection reports the released task.
        slot.set(0);
        ctx.collect();
        assert_eq!(ctx.trap_record().map(|trap| trap.pos_id), Some(45));
        ctx.clear_trap();
        ctx.enter_script();
        ctx.exit_script();
        assert_eq!(ctx.detection_interval(), 1);
        assert_eq!(ctx.detection_collection_count(), 10);
    }
}

/// The collection of a caller that owns roots: the release order of
/// `Context::collect`.
fn owner_collect(ctx: &mut Context) {
    Context::with_collection(
        ctx,
        |ctx| ctx,
        |ctx, handles| {
            for &handle in handles.iter() {
                // SAFETY: the collection plan names each released handle once.
                unsafe { ctx.async_release(handle as *mut u8, 0) };
            }
        },
    );
}

/// Rule 16: `exit_script` and the deferred pair give the same exit count
/// and the same schedule on the same sequence.
#[test]
fn the_exit_and_the_deferred_pair_give_the_same_count_and_schedule() {
    let run = |deferred: bool| {
        let mut ctx = Context::new();
        assert!(ctx.set_collection(CollectionMode::Automatic, 4096));
        let task = failed_task(&mut ctx, 1);
        let slot = RootSlot::new(&mut ctx, task.holder);
        let mut states = Vec::new();
        for step in 0..300 {
            if step == 250 {
                slot.set(0);
            }
            ctx.enter_script();
            ctx.enter_script();
            garbage(&mut ctx, 3, 512);
            for _ in 0..2 {
                if deferred {
                    let service = ctx.exit_script_deferred();
                    Context::finish_exit_service(&mut *ctx, |ctx| ctx, service, owner_collect);
                } else {
                    ctx.exit_script();
                }
            }
            states.push((
                ctx.outermost_exit_count(),
                ctx.detection_collection_count(),
                ctx.detection_interval(),
                ctx.automatic_collection_count(),
                ctx.trap_record().map(|trap| trap.pos_id),
            ));
            if ctx.trapped() {
                break;
            }
        }
        states
    };
    let native = run(false);
    let deferred = run(true);
    assert_eq!(native, deferred);
    // Firing control: the sequence reaches detection, the automatic
    // service, and the trap.
    // The holder becomes unreachable at exit 251; the detection of exit
    // 255 (interval 128) reports it.
    let last = native.last().expect("states");
    assert_eq!(last.0, 255);
    assert!(last.1 > 1);
    assert!(last.3 > 0);
    assert_eq!(last.4, Some(41));
}

/// Rule 17: the return values, the default headroom, and mode 2.
#[test]
fn the_host_setter_checks_its_arguments() {
    let mut ctx = Context::new();
    // SAFETY: the Context is live and exclusive in each call.
    unsafe {
        assert_eq!(subscript_rt_ctx_set_collection(&mut *ctx, 3, 0), -1);
        assert_eq!(subscript_rt_ctx_set_collection(&mut *ctx, -1, 0), -1);
        ctx.enter_script();
        assert_eq!(subscript_rt_ctx_set_collection(&mut *ctx, 1, 4096), -1);
        ctx.exit_script();
        // Rule 17: mode 1 is the default, and a refused call keeps it.
        assert_eq!(ctx.collection_mode(), CollectionMode::Automatic);
        assert_eq!(ctx.collection_min_headroom(), DEFAULT_MIN_HEADROOM_BYTES);
        assert_eq!(subscript_rt_ctx_set_collection(&mut *ctx, 1, 4096), 0);
        assert_eq!(ctx.collection_mode(), CollectionMode::Automatic);
        assert_eq!(ctx.collection_min_headroom(), 4096);
        assert_eq!(subscript_rt_ctx_set_collection(&mut *ctx, 1, 0), 0);
        assert_eq!(ctx.collection_min_headroom(), DEFAULT_MIN_HEADROOM_BYTES);
        assert_eq!(subscript_rt_ctx_set_collection(&mut *ctx, 0, 0), 0);
        assert_eq!(ctx.collection_mode(), CollectionMode::Off);
    }
}

/// Rule 17: mode 2 collects at each outermost exit. Control: mode 1 with
/// little garbage does not.
#[test]
fn mode_two_collects_at_each_outermost_exit() {
    for (raw, expected) in [(2, 20u64), (1, 0)] {
        let mut ctx = Context::new();
        // SAFETY: the Context is live and exclusive.
        assert_eq!(
            unsafe { subscript_rt_ctx_set_collection(&mut *ctx, raw, 0) },
            0
        );
        for _ in 0..20 {
            cycle(&mut ctx, 2, 32);
        }
        assert_eq!(ctx.automatic_collection_count(), expected);
        assert_eq!(ctx.live_count(), if raw == 2 { 0 } else { 40 });
        assert_eq!(ctx.collection_pending(), raw == 2);
    }
}

/// The poll entry collects when the flag is set and does nothing when it
/// is clear.
#[test]
fn the_poll_collects_only_when_the_flag_is_set() {
    for ship in [false, true] {
        let mut ctx = context(ship);
        assert!(ctx.set_collection(CollectionMode::Automatic, 1024));
        garbage(&mut ctx, 4, 64);
        assert!(!ctx.collection_pending());
        // SAFETY: the Context is live and exclusive.
        unsafe { subscript_rt_collect_poll(&mut *ctx) };
        assert_eq!(ctx.live_count(), 4, "a clear flag collected");
        assert_eq!(ctx.automatic_collection_count(), 0);
        garbage(&mut ctx, 40, 64);
        assert!(ctx.collection_pending());
        // SAFETY: the Context is live and exclusive.
        unsafe { subscript_rt_collect_poll(&mut *ctx) };
        assert_eq!(ctx.live_count(), 0);
        assert_eq!(ctx.automatic_collection_count(), 1);
        assert!(!ctx.collection_pending());
    }
}

/// A poll records the failure that it releases and does not trap; the next
/// outermost exit reports it at the throw position.
#[test]
fn a_poll_records_and_the_next_outermost_exit_reports() {
    let mut ctx = Context::new();
    assert!(ctx.set_collection(CollectionMode::EachPoll, 0));
    ctx.enter_script();
    let _ = failed_task(&mut ctx, 4);
    // SAFETY: the Context is live and exclusive.
    unsafe { subscript_rt_collect_poll(&mut *ctx) };
    assert!(!ctx.trapped());
    assert_eq!(ctx.recorded_failure_count(), 1);
    ctx.exit_script();
    let trap = ctx.trap_record().expect("the outermost exit reports");
    assert_eq!((trap.message.as_str(), trap.pos_id), ("Error: 4", 44));
}

static WORKER_SETTINGS: std::sync::Mutex<Option<(CollectionMode, usize)>> =
    std::sync::Mutex::new(None);

unsafe extern "C" fn no_init(_ctx: *mut Context) {}

unsafe extern "C" fn read_settings(
    ctx: *mut Context,
    _inbox: *mut subscript_runtime::WorkerInbox,
    _outbox: *mut subscript_runtime::WorkerOutbox,
) {
    // SAFETY: the worker Context is live for the entry call.
    let ctx = unsafe { &*ctx };
    *WORKER_SETTINGS.lock().expect("lock") =
        Some((ctx.collection_mode(), ctx.collection_min_headroom()));
}

/// Rule 18: a spawned worker has the mode and the headroom of its parent.
/// Control: a parent with the default values gives the default values.
#[test]
fn a_worker_takes_the_collection_settings_of_its_parent() {
    let descriptor = subscript_runtime::worker::WorkerMessageDescriptor {
        payload_size: 8,
        string_slot_count: 0,
        string_slot_offsets: std::ptr::null(),
    };
    for set in [true, false] {
        let mut parent = Context::new();
        if set {
            assert!(parent.set_collection(CollectionMode::EachPoll, 12_345));
        }
        let expected = (parent.collection_mode(), parent.collection_min_headroom());
        // SAFETY: the parent Context is live; the descriptors and the entry
        // functions outlive the worker.
        unsafe {
            let worker = subscript_rt_worker_spawn(
                &mut *parent,
                Some(no_init),
                Some(read_settings),
                &descriptor,
                &descriptor,
                0,
            );
            assert!(!worker.is_null());
            assert_eq!(subscript_rt_worker_join(&mut *parent, worker), 1);
        }
        let seen = WORKER_SETTINGS.lock().expect("lock").take();
        assert_eq!(seen, Some(expected));
        assert_eq!(
            expected == (CollectionMode::EachPoll, 12_345),
            set,
            "the control has the default values"
        );
    }
}

/// One host frame: a time-critical call that allocates garbage, then an
/// update call that allocates nothing. With `switch`, the host sets mode 0
/// around the time-critical call and mode 1 after it.
fn switching_frames(ctx: &mut Context, frames: usize, switch: bool) -> usize {
    let mut peak = 0;
    for _ in 0..frames {
        if switch {
            assert!(ctx.set_collection(CollectionMode::Off, 4096));
        }
        cycle(ctx, 4, 256);
        if switch {
            assert!(ctx.set_collection(CollectionMode::Automatic, 4096));
        }
        cycle(ctx, 0, 0);
        peak = peak.max(ctx.live_bytes());
    }
    peak
}

/// Rule 17: a host that switches 0 to 1 around a call at each frame gets
/// the same collections as a host that leaves mode 1 set, and live bytes
/// stay bounded. Control: the host that leaves mode 1 set collects.
#[test]
fn a_host_that_switches_the_mode_each_frame_collects_as_mode_one() {
    const FRAMES: usize = 2_000;
    for ship in [false, true] {
        let mut kept = context(ship);
        assert!(kept.set_collection(CollectionMode::Automatic, 4096));
        let kept_peak = switching_frames(&mut kept, FRAMES, false);
        let mut switched = context(ship);
        assert!(switched.set_collection(CollectionMode::Automatic, 4096));
        let switched_peak = switching_frames(&mut switched, FRAMES, true);
        assert!(kept.automatic_collection_count() > 0);
        assert_eq!(
            switched.automatic_collection_count(),
            kept.automatic_collection_count()
        );
        assert_eq!(switched_peak, kept_peak);
        assert!(switched_peak < 4 * 4096, "live bytes grow: {switched_peak}");
    }
}

/// `compiler.md` §346 rule 3: a call that changes only the headroom moves
/// the limit, keeps the base (the live bytes after the last collection),
/// and keeps the requested bytes.
#[test]
fn a_headroom_change_moves_the_limit_and_keeps_the_base() {
    for ship in [false, true] {
        let mut ctx = context(ship);
        assert!(ctx.set_collection(CollectionMode::Automatic, 65_536));
        let mut handles: Vec<usize> = (0..40).map(|_| ctx.alloc(100, 900, 0) as usize).collect();
        // SAFETY: the vector stays allocated and unmoved while the Context lives.
        unsafe { ctx.root_add(handles.as_mut_ptr() as usize, handles.len()) };
        ctx.collect_automatic();
        let base = ctx.live_bytes();
        assert!(base > 0);
        garbage(&mut ctx, 30, 100);
        assert!(ctx.live_bytes() > base);
        assert_eq!(ctx.requested_bytes(), 3_000);
        for headroom in [1_000usize, 100_000, base / 2] {
            assert!(ctx.set_collection(CollectionMode::Automatic, headroom));
            assert_eq!(ctx.collection_limit(), base.max(headroom));
            assert_eq!(ctx.requested_bytes(), 3_000);
        }
        drop(ctx);
        drop(handles);
    }
}

/// `compiler.md` §346 rule 3: after allocations in mode 0, the switch to
/// mode 1 sets the flag if the requested bytes exceed the limit. Control:
/// under the limit it does not. Mode 0 clears the flag, and mode 2 sets it.
#[test]
fn a_switch_to_mode_one_sets_the_flag_over_the_limit() {
    for ship in [false, true] {
        for (count, over) in [(40usize, true), (2, false)] {
            let mut ctx = context(ship);
            assert!(ctx.set_collection(CollectionMode::Off, 1024));
            garbage(&mut ctx, count, 64);
            assert!(!ctx.collection_pending());
            assert_eq!(ctx.requested_bytes(), count * 64);
            assert_eq!(ctx.requested_bytes() > 1024, over);
            assert!(ctx.set_collection(CollectionMode::Automatic, 1024));
            assert_eq!(ctx.collection_pending(), over);
            assert!(ctx.set_collection(CollectionMode::Off, 1024));
            assert!(!ctx.collection_pending());
            assert!(ctx.set_collection(CollectionMode::EachPoll, 1024));
            assert!(ctx.collection_pending());
            assert_eq!(ctx.live_count(), count, "the call collected");
        }
    }
}

/// A `Promise.all` aggregate of two inputs whose first input failed. The
/// second input stays unfinished, so the aggregate stays registered after
/// the release of its last holder. Returns the aggregate.
fn failed_aggregate(ctx: &mut Context) -> *mut u8 {
    let inputs: Vec<*mut u8> = (0..2)
        .map(|_| {
            let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
            // SAFETY: the fresh allocation has the async header.
            unsafe { ctx.async_register(frame, 4) };
            frame
        })
        .collect();
    let array = ctx.array_new(std::mem::size_of::<*mut u8>(), 0);
    for handle in &inputs {
        // SAFETY: each pointer has one native word of readable storage.
        unsafe { ctx.array_push(array, (handle as *const *mut u8).cast(), 0) };
    }
    // SAFETY: the array holds registered handles with four-byte results.
    let all = unsafe { ctx.async_all(array, 4, 4, 0) };
    let error = ctx.alloc(24, 1, 0);
    // SAFETY: the error is a live handle that exception roots retain.
    unsafe { ctx.raise_exception(error, "Error: all".to_string(), 51) };
    ctx.async_complete_exception(inputs[0]);
    // SAFETY: the Context is live and exclusive.
    unsafe { ctx.async_step() };
    all
}

/// Rule 14a: a task whose failure the Context reported is not a failed
/// unobserved task, so no detection collection runs after the host clears
/// the trap. Control: the same aggregate, not released, is unreported, and
/// the outermost exit runs a detection collection.
#[test]
fn a_reported_aggregate_failure_runs_no_detection_collection() {
    for ship in [false, true] {
        for release in [true, false] {
            let mut ctx = context(ship);
            assert!(ctx.set_collection(CollectionMode::Automatic, 0));
            ctx.enter_script();
            let all = failed_aggregate(&mut ctx);
            if release {
                // SAFETY: the test owns the one holder of the aggregate.
                unsafe { ctx.async_release(all, 0) };
                let trap = ctx.trap_record().expect("the last holder reports");
                assert_eq!((trap.kind, trap.pos_id), (TrapKind::UncaughtException, 51));
                ctx.clear_trap();
            }
            assert!(!ctx.trapped());
            ctx.exit_script();
            for _ in 0..20 {
                if ctx.trapped() {
                    break;
                }
                ctx.enter_script();
                ctx.exit_script();
            }
            if release {
                assert_eq!(ctx.detection_collection_count(), 0);
                assert_eq!(ctx.detection_interval(), 1);
                assert!(!ctx.trapped());
            } else {
                assert!(ctx.detection_collection_count() > 0);
            }
        }
    }
}

/// `Context::record_task_failure`: outside a collection it records nothing
/// and returns false; inside one it records and returns true, and an
/// explicit collection reports the record at the throw position.
#[test]
fn record_task_failure_records_only_inside_a_collection() {
    for ship in [false, true] {
        let mut ctx = context(ship);
        let error = ctx.alloc(24, 1, 0) as usize;
        assert!(!ctx.record_task_failure(7, error, "Error: 7".to_string(), 77));
        assert_eq!(ctx.recorded_failure_count(), 0);
        let recorded = Context::automatic_collection_with(
            &mut *ctx,
            |ctx| ctx,
            |ctx| ctx.record_task_failure(7, error, "Error: 7".to_string(), 77),
        );
        assert!(recorded);
        assert_eq!(ctx.recorded_failure_count(), 1);
        assert!(!ctx.trapped());
        ctx.collect();
        assert!(ctx.is_live(error), "the recorded error is not a root");
        let trap = ctx.trap_record().expect("the explicit collection reports");
        assert_eq!(
            (trap.kind, trap.message.as_str(), trap.pos_id),
            (TrapKind::UncaughtException, "Error: 7", 77)
        );
    }
}

/// `Context::exit_script_with_tasks`: a closure that returns true runs the
/// schedule of rule 14 with no failed task in the Context. Control: a
/// closure that returns false gives no detection.
#[test]
fn exit_script_with_tasks_runs_the_schedule_for_an_owner_task() {
    for owner_failed in [true, false] {
        let mut ctx = Context::new();
        assert!(ctx.set_collection(CollectionMode::Automatic, 0));
        let mut due = Vec::new();
        for _ in 0..8 {
            ctx.enter_script();
            let service = ctx.exit_script_with_tasks(|| owner_failed);
            due.push(service.detection_due());
            Context::finish_exit_service(&mut *ctx, |ctx| ctx, service, owner_collect);
        }
        if owner_failed {
            // Exits 1, 3, and 7.
            let expected = [true, false, true, false, false, false, true, false];
            assert_eq!(due, expected);
            assert_eq!(ctx.detection_collection_count(), 3);
            assert_eq!(ctx.detection_interval(), 8);
        } else {
            assert_eq!(due, [false; 8]);
            assert_eq!(ctx.detection_collection_count(), 0);
        }
        assert!(!ctx.trapped());
    }
}

/// `Context::automatic_collection_with`: it collects the owner's roots,
/// counts one automatic collection, clears the flag, sets the limit,
/// and records a released failed task instead of a trap. Control: a
/// release of the same task outside a collection traps.
#[test]
fn automatic_collection_with_collects_and_records() {
    for ship in [false, true] {
        for inside in [true, false] {
            let mut ctx = context(ship);
            assert!(ctx.set_collection(CollectionMode::Automatic, 1024));
            garbage(&mut ctx, 40, 64);
            assert!(ctx.collection_pending());
            let task = failed_task(&mut ctx, 6);
            if inside {
                // The holder is unreachable, so the collection releases the
                // last owner of the failed task.
                Context::automatic_collection_with(&mut *ctx, |ctx| ctx, owner_collect);
                assert!(!ctx.trapped());
                assert!(!ctx.is_live(task.frame));
                assert_eq!(ctx.live_count(), 1, "only the recorded error stays");
                assert_eq!(ctx.recorded_failure_count(), 1);
                assert_eq!(ctx.automatic_collection_count(), 1);
                assert!(!ctx.collection_pending());
                let live = ctx.live_bytes();
                assert_eq!(ctx.collection_limit(), live.max(1024));
                assert_eq!(ctx.requested_bytes(), 0);
            } else {
                // SAFETY: the test releases the one reference of the frame.
                unsafe { ctx.async_release(task.frame as *mut u8, 0) };
                let trap = ctx.trap_record().expect("the release reports");
                assert_eq!(trap.pos_id, 46);
                assert_eq!(ctx.automatic_collection_count(), 0);
            }
        }
    }
}

/// `compiler.md` §342 rule 12: the failure record is active only during a
/// collection. A collection that unwinds restores it, so a later call
/// records nothing. The firing control is the record during a collection.
#[test]
fn an_unwinding_collection_ends_the_failure_record() {
    for ship in [false, true] {
        let mut ctx = context(ship);
        assert!(!ctx.record_task_failure(1, 0, "Error: 1".into(), 1));
        let recorded = Context::automatic_collection_with(
            &mut *ctx,
            |ctx| ctx,
            |ctx| ctx.record_task_failure(2, 0, "Error: 2".into(), 2),
        );
        assert!(
            recorded,
            "ship={ship}: the record is active in a collection"
        );
        ctx.collect();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Context::automatic_collection_with(&mut *ctx, |ctx| ctx, |_| panic!("collector"))
        }));
        assert!(unwound.is_err());
        assert!(
            !ctx.record_task_failure(3, 0, "Error: 3".into(), 3),
            "ship={ship}"
        );
        assert_eq!(ctx.recorded_failure_count(), 0, "ship={ship}");
    }
}

/// `compiler.md` §345 rule 4: the exit-count entry gives 0 for a new
/// Context, N for N bracketed pairs, and one count for a nested pair. The
/// inner exit of the nested pair is the control: it does not count.
#[test]
fn the_exit_count_entry_counts_each_outermost_exit() {
    const PAIRS: u64 = 5;
    for ship in [false, true] {
        let mut ctx = context(ship);
        // SAFETY: the Context is live and exclusive in each call below.
        unsafe {
            assert_eq!(subscript_rt_outermost_exit_count(&*ctx), 0);
            for _ in 0..PAIRS {
                subscript_rt_ctx_enter_script(&mut *ctx);
                subscript_rt_ctx_exit_script(&mut *ctx);
            }
            assert_eq!(subscript_rt_outermost_exit_count(&*ctx), PAIRS);
            subscript_rt_ctx_enter_script(&mut *ctx);
            subscript_rt_ctx_enter_script(&mut *ctx);
            subscript_rt_ctx_exit_script(&mut *ctx);
            assert_eq!(
                subscript_rt_outermost_exit_count(&*ctx),
                PAIRS,
                "ship={ship}: an inner exit counted"
            );
            subscript_rt_ctx_exit_script(&mut *ctx);
            assert_eq!(subscript_rt_outermost_exit_count(&*ctx), PAIRS + 1);
            assert_eq!(
                subscript_rt_outermost_exit_count(&*ctx),
                ctx.outermost_exit_count()
            );
        }
    }
}

/// `compiler.md` §345 rule 4: the collection-count entry gives 0 for a new
/// Context and N for N bracketed pairs in mode 2, where each outermost exit
/// collects. A nested pair collects once. Control: mode 0 gives 0 for the
/// same calls.
#[test]
fn the_collection_count_entry_counts_each_automatic_collection() {
    const PAIRS: u64 = 5;
    for ship in [false, true] {
        for mode in [CollectionMode::EachPoll, CollectionMode::Off] {
            let mut ctx = context(ship);
            assert!(ctx.set_collection(mode, 0));
            let per_pair = u64::from(mode == CollectionMode::EachPoll);
            // SAFETY: the Context is live and exclusive in each call below.
            unsafe {
                assert_eq!(subscript_rt_automatic_collection_count(&*ctx), 0);
                for _ in 0..PAIRS {
                    subscript_rt_ctx_enter_script(&mut *ctx);
                    subscript_rt_ctx_exit_script(&mut *ctx);
                }
                assert_eq!(
                    subscript_rt_automatic_collection_count(&*ctx),
                    PAIRS * per_pair,
                    "ship={ship} mode={mode:?}"
                );
                subscript_rt_ctx_enter_script(&mut *ctx);
                subscript_rt_ctx_enter_script(&mut *ctx);
                subscript_rt_ctx_exit_script(&mut *ctx);
                subscript_rt_ctx_exit_script(&mut *ctx);
                assert_eq!(
                    subscript_rt_automatic_collection_count(&*ctx),
                    (PAIRS + 1) * per_pair,
                    "ship={ship} mode={mode:?}"
                );
                assert_eq!(
                    subscript_rt_automatic_collection_count(&*ctx),
                    ctx.automatic_collection_count()
                );
            }
        }
    }
}
