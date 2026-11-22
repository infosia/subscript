//! `Promise.race` at the runtime API (`compiler.md` §350 rules 3 to 7, 10,
//! and 12). Each test runs in the dev allocator and in the ship arena. Each
//! test that states an absence has a firing control in the same shape.

use super::*;

/// The two allocator tiers: the dev allocator and the ship arena.
fn contexts() -> [Box<Context>; 2] {
    [Context::new(), Context::new_releasing()]
}

/// A registered input frame with a count of one and no completion.
fn input(ctx: &mut Context, size: usize) -> *mut u8 {
    let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
    // SAFETY: the fresh allocation has the emitted async header.
    unsafe { ctx.async_register(frame, size) };
    frame
}

fn jobs(ctx: &mut Context, inputs: &[*mut u8]) -> *mut u8 {
    let array = ctx.array_new(std::mem::size_of::<*mut u8>(), 0);
    for handle in inputs {
        // SAFETY: each pointer has one native word of readable storage.
        unsafe { ctx.array_push(array, (handle as *const *mut u8).cast(), 0) };
    }
    array
}

fn race(ctx: &mut Context, inputs: &[*mut u8], size: usize) -> *mut u8 {
    let array = jobs(ctx, inputs);
    // SAFETY: the array holds registered handles of `size`-byte results.
    let handle = unsafe { ctx.async_race_of(array, size, 0) };
    assert!(!handle.is_null());
    handle
}

fn complete(ctx: &mut Context, frame: *mut u8, bytes: &[u8]) {
    // SAFETY: `bytes` is readable for its length.
    unsafe { ctx.async_complete(frame, bytes.as_ptr(), bytes.len()) };
}

fn complete_i32(ctx: &mut Context, frame: *mut u8, value: i32) {
    complete(ctx, frame, &value.to_ne_bytes());
}

/// The value completion of `handle`, or `None` for an exception, which is
/// then pending.
fn read(ctx: &mut Context, handle: *mut u8, size: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; size];
    let pointer = if size == 0 {
        std::ptr::null_mut()
    } else {
        out.as_mut_ptr()
    };
    // SAFETY: `out` holds `size` writable bytes.
    assert!(unsafe { ctx.async_result(handle, pointer, size) });
    if ctx.exception_pending() {
        None
    } else {
        Some(out)
    }
}

fn read_i32(ctx: &mut Context, handle: *mut u8) -> i32 {
    let bytes = read(ctx, handle, 4).expect("a value completion");
    i32::from_ne_bytes(bytes.try_into().expect("four bytes"))
}

/// Completes `frame` with an exception whose Error object is a fresh
/// allocation. Returns the Error object.
fn fail(ctx: &mut Context, frame: *mut u8, message: &str, position: u32) -> *mut u8 {
    let error = ctx.alloc(24, 1, 0);
    // SAFETY: the object is a live Context handle and exception roots retain it (section 263).
    unsafe { ctx.raise_exception(error, message.to_string(), position) };
    ctx.async_complete_exception(frame);
    error
}

fn step(ctx: &mut Context) {
    // SAFETY: no generated frame is queued in these tests.
    unsafe { ctx.async_step() };
}

fn release(ctx: &mut Context, frames: &[*mut u8]) {
    for frame in frames {
        // SAFETY: the test owns one count of each frame.
        unsafe { ctx.async_release(*frame, 0) };
    }
}

fn completed(ctx: &Context, handle: *mut u8) -> bool {
    ctx.async_frames[&(handle as usize)].completion.is_some()
}

fn waiters(ctx: &Context, handle: *mut u8) -> usize {
    ctx.async_frames[&(handle as usize)].waiters.len()
}

/// Rule 3: the first value wins, for each result size, and a loser that
/// completes later changes nothing. The race is a task of kind 2.
#[test]
fn the_first_value_wins_for_each_size() {
    for mut ctx in contexts() {
        for size in [0usize, 1, 4, 8, 16] {
            let a = input(&mut ctx, size);
            let b = input(&mut ctx, size);
            let handle = race(&mut ctx, &[a, b], size);
            assert_eq!(ctx.async_frames[&(handle as usize)].kind.task_kind(), 2);
            let first: Vec<u8> = (1..=size as u8).collect();
            let second: Vec<u8> = (101..101 + size as u8).collect();
            complete(&mut ctx, b, &first);
            step(&mut ctx);
            assert_eq!(read(&mut ctx, handle, size), Some(first.clone()));
            complete(&mut ctx, a, &second);
            step(&mut ctx);
            assert_eq!(read(&mut ctx, handle, size), Some(first));
            release(&mut ctx, &[handle, a, b]);
            assert!(!ctx.trapped());
            assert!(ctx.async_frames.is_empty(), "size {size}");
        }
    }
}

/// Rule 3 and rule 10: a class reference that wins stays reachable through
/// the race after a collection. Control: after the release of the race,
/// the collection frees the object.
#[test]
fn a_reference_that_wins_survives_a_collection() {
    for mut ctx in contexts() {
        let word = std::mem::size_of::<*mut u8>();
        let a = input(&mut ctx, word);
        let b = input(&mut ctx, word);
        let handle = race(&mut ctx, &[a, b], word);
        let object = ctx.alloc(16, 1, 0);
        // SAFETY: the object has sixteen writable bytes.
        unsafe { object.cast::<i32>().write(17) };
        complete(&mut ctx, a, &(object as usize).to_ne_bytes());
        step(&mut ctx);
        release(&mut ctx, &[a]);
        ctx.collect();
        let bytes = read(&mut ctx, handle, word).expect("a value completion");
        let value = usize::from_ne_bytes(bytes.try_into().expect("one word"));
        assert_eq!(value, object as usize);
        assert!(ctx.is_live(object as usize));
        // SAFETY: the race completion keeps the object live.
        assert_eq!(unsafe { object.cast::<i32>().read() }, 17);
        release(&mut ctx, &[handle]);
        ctx.collect();
        assert!(!ctx.is_live(object as usize));
        complete(&mut ctx, b, &0usize.to_ne_bytes());
        release(&mut ctx, &[b]);
        assert!(ctx.async_frames.is_empty());
        assert!(!ctx.trapped());
    }
}

/// Rule 3: an exception that wins is raised by a read of the race, with
/// the same Error object. A loser that completes later, with a value or an
/// exception, gives no trap.
#[test]
fn an_exception_that_wins_is_raised_and_a_later_loser_does_not_trap() {
    for mut ctx in contexts() {
        for loser_fails in [false, true] {
            let a = input(&mut ctx, 4);
            let b = input(&mut ctx, 4);
            let handle = race(&mut ctx, &[a, b], 4);
            let error = fail(&mut ctx, a, "Error: first", 31);
            step(&mut ctx);
            assert_eq!(read(&mut ctx, handle, 4), None);
            assert_eq!(ctx.catch_exception(), error);
            if loser_fails {
                fail(&mut ctx, b, "Error: later", 32);
            } else {
                complete_i32(&mut ctx, b, 5);
            }
            step(&mut ctx);
            release(&mut ctx, &[handle, a, b]);
            assert!(!ctx.trapped());
            assert!(ctx.async_frames.is_empty());
        }
    }
}

/// Rule 4: of two inputs with a completion at the call, the first in
/// array order wins, in both orders. The race has no completion at the
/// return of the call; it has one after one step.
#[test]
fn the_first_completed_input_in_array_order_wins() {
    for mut ctx in contexts() {
        for reverse in [false, true] {
            let a = input(&mut ctx, 4);
            let b = input(&mut ctx, 4);
            complete_i32(&mut ctx, a, 1);
            complete_i32(&mut ctx, b, 2);
            let order = if reverse { [b, a] } else { [a, b] };
            let handle = race(&mut ctx, &order, 4);
            assert!(!completed(&ctx, handle));
            assert_eq!(ctx.async_pending(), 2);
            step(&mut ctx);
            assert!(completed(&ctx, handle));
            assert_eq!(read_i32(&mut ctx, handle), if reverse { 2 } else { 1 });
            release(&mut ctx, &[handle, a, b]);
            assert!(ctx.async_frames.is_empty());
        }
    }
}

/// Rule 4: a pending input that completes after the call, before the
/// step, does not pass an input that had its completion at the call.
/// Control: with no completed input at the call, the pending input wins.
#[test]
fn a_pending_input_does_not_pass_one_completed_at_the_call() {
    for mut ctx in contexts() {
        for other_completed in [true, false] {
            let pending = input(&mut ctx, 4);
            let other = input(&mut ctx, 4);
            if other_completed {
                complete_i32(&mut ctx, other, 2);
            }
            let handle = race(&mut ctx, &[pending, other], 4);
            complete_i32(&mut ctx, pending, 1);
            step(&mut ctx);
            assert_eq!(
                read_i32(&mut ctx, handle),
                if other_completed { 2 } else { 1 }
            );
            if !other_completed {
                assert_eq!(waiters(&ctx, other), 0);
                complete_i32(&mut ctx, other, 3);
                step(&mut ctx);
            }
            release(&mut ctx, &[handle, pending, other]);
            assert!(ctx.async_frames.is_empty());
            assert!(!ctx.trapped());
        }
    }
}

/// Rule 5 with §70.3 rule 4: a read of a loser after the race completed
/// gives its value or raises its exception, each time.
#[test]
fn a_loser_read_after_the_race_gives_its_completion() {
    for mut ctx in contexts() {
        let a = input(&mut ctx, 4);
        let b = input(&mut ctx, 4);
        let c = input(&mut ctx, 4);
        let handle = race(&mut ctx, &[a, b, c], 4);
        complete_i32(&mut ctx, a, 1);
        step(&mut ctx);
        complete_i32(&mut ctx, b, 2);
        let error = fail(&mut ctx, c, "Error: late", 62);
        step(&mut ctx);
        for _ in 0..2 {
            assert_eq!(read_i32(&mut ctx, b), 2);
            assert_eq!(read(&mut ctx, c, 4), None);
            assert_eq!(ctx.catch_exception(), error);
        }
        release(&mut ctx, &[handle, a, b, c]);
        assert!(!ctx.trapped());
        assert!(ctx.async_frames.is_empty());
    }
}

/// Rule 5: a loser that fails after the race completed does not trap at
/// its release, and an outermost exit finds no failed unobserved task.
/// Control: the same task with no race traps at its release.
#[test]
fn a_loser_that_fails_later_is_observed() {
    for mut ctx in contexts() {
        for with_race in [false, true] {
            ctx.set_collection(CollectionMode::Automatic, 0);
            let a = input(&mut ctx, 4);
            let b = input(&mut ctx, 4);
            complete_i32(&mut ctx, a, 7);
            if with_race {
                let handle = race(&mut ctx, &[a, b], 4);
                step(&mut ctx);
                assert_eq!(read_i32(&mut ctx, handle), 7);
                release(&mut ctx, &[handle]);
            }
            fail(&mut ctx, b, "Error: late", 61);
            assert!(ctx.task_failure_possible());
            ctx.enter_script();
            ctx.exit_script();
            assert_eq!(ctx.task_failure_possible(), !with_race);
            assert!(!ctx.trapped());
            ctx.collect_automatic();
            assert_eq!(ctx.recorded_failure_count(), 0);
            release(&mut ctx, &[a, b]);
            assert_eq!(ctx.trapped(), !with_race);
            if let Some(trap) = ctx.trap_record() {
                assert_eq!(
                    (trap.kind, trap.message.as_str(), trap.pos_id),
                    (TrapKind::UncaughtException, "Error: late", 61)
                );
            }
            ctx.clear_trap();
            assert!(ctx.async_frames.is_empty());
        }
    }
}

/// One counted handle field at offset 0 (`compiler.md` §171).
const HANDLE_FIELD: [u64; 6] = [1, 0, 24, 1, 0, 0];

/// An object of one counted handle field that holds `frame`. The field
/// owns one count of the frame.
fn holder(ctx: &mut Context, frame: *mut u8) -> *mut u8 {
    let object = ctx.alloc(8, 900, 0);
    // SAFETY: the object has one 8-byte field, and the description states
    // one handle field at offset 0.
    unsafe {
        object.cast::<*mut u8>().write(frame);
        ctx.describe_object(
            object as usize,
            std::slice::from_raw_parts(HANDLE_FIELD.as_ptr().cast(), 48),
        );
    }
    object
}

/// Rule 5 in a collection (§342 rules 12 and 13): a loser whose only
/// holder is its own Error object fails after the race completed, and an
/// automatic collection and an explicit collection end it with no record
/// and no trap. Control: with no race, the automatic collection records
/// the failure and the explicit collection reports it.
#[test]
fn a_collection_that_ends_a_failed_loser_records_nothing() {
    for mut ctx in contexts() {
        for explicit in [false, true] {
            for with_race in [false, true] {
                let a = input(&mut ctx, 4);
                let b = input(&mut ctx, 4);
                complete_i32(&mut ctx, a, 7);
                if with_race {
                    let handle = race(&mut ctx, &[a, b], 4);
                    step(&mut ctx);
                    release(&mut ctx, &[handle]);
                }
                // The test's count of `b` moves to its Error object.
                let error = holder(&mut ctx, b);
                // SAFETY: the error is a live handle that the exception roots keep.
                unsafe { ctx.raise_exception(error, "Error: cycle".into(), 47) };
                ctx.async_complete_exception(b);
                if explicit {
                    ctx.collect();
                } else {
                    ctx.collect_automatic();
                }
                assert!(!ctx.async_frames.contains_key(&(b as usize)));
                if explicit {
                    assert_eq!(ctx.trapped(), !with_race);
                } else {
                    assert!(!ctx.trapped());
                    assert_eq!(ctx.recorded_failure_count(), usize::from(!with_race));
                    ctx.collect();
                    assert_eq!(ctx.trapped(), !with_race);
                }
                if let Some(trap) = ctx.trap_record() {
                    assert_eq!((trap.kind, trap.pos_id), (TrapKind::UncaughtException, 47));
                }
                ctx.clear_trap();
                release(&mut ctx, &[a]);
                assert!(ctx.async_frames.is_empty());
            }
        }
    }
}

/// Rule 5: a loser that failed before the detach, at the call or after
/// it, does not trap at its release. Control: the same failed task with
/// no race traps.
#[test]
fn a_loser_that_failed_before_the_detach_is_observed() {
    for mut ctx in contexts() {
        for failed_at_call in [false, true] {
            for with_race in [false, true] {
                let a = input(&mut ctx, 4);
                let b = input(&mut ctx, 4);
                complete_i32(&mut ctx, a, 7);
                if failed_at_call {
                    fail(&mut ctx, b, "Error: loser", 71);
                }
                if with_race {
                    let handle = race(&mut ctx, &[a, b], 4);
                    if !failed_at_call {
                        fail(&mut ctx, b, "Error: loser", 71);
                    }
                    step(&mut ctx);
                    assert_eq!(read_i32(&mut ctx, handle), 7);
                    release(&mut ctx, &[handle]);
                } else if !failed_at_call {
                    fail(&mut ctx, b, "Error: loser", 71);
                }
                release(&mut ctx, &[a, b]);
                assert_eq!(ctx.trapped(), !with_race);
                if let Some(trap) = ctx.trap_record() {
                    assert_eq!((trap.kind, trap.pos_id), (TrapKind::UncaughtException, 71));
                }
                ctx.clear_trap();
                assert!(ctx.async_frames.is_empty());
            }
        }
    }
}

/// Rule 3 with §116.1 rule 4: the exception of the winner that no read
/// raised traps when the race loses its last holder. A release of the
/// last holder after the race completed traps at that release; a race
/// whose last holder ended before it completed traps in the step that
/// completes it, as `Promise.all` does. Control: a read of the race
/// before the release gives no trap.
#[test]
fn an_unobserved_winner_exception_traps_at_the_last_holder() {
    for mut ctx in contexts() {
        for (released_first, observed) in [(false, false), (true, false), (false, true)] {
            let a = input(&mut ctx, 4);
            let handle = race(&mut ctx, &[a], 4);
            if released_first {
                release(&mut ctx, &[handle]);
                assert!(ctx.async_frames.contains_key(&(handle as usize)));
            }
            fail(&mut ctx, a, "Error: winner", 81);
            step(&mut ctx);
            if observed {
                assert_eq!(read(&mut ctx, handle, 4), None);
                ctx.catch_exception();
            }
            if !released_first {
                assert!(!ctx.trapped());
                release(&mut ctx, &[handle]);
            }
            assert_eq!(ctx.trapped(), !observed);
            if let Some(trap) = ctx.trap_record() {
                assert_eq!(
                    (trap.kind, trap.message.as_str(), trap.pos_id),
                    (TrapKind::UncaughtException, "Error: winner", 81)
                );
            }
            ctx.clear_trap();
            release(&mut ctx, &[a]);
            assert!(!ctx.trapped());
            assert!(ctx.async_frames.is_empty());
        }
    }
}

/// The counts after `turns` turns of one race of a task that completes and
/// one handle that never completes: registered frames, unfinished tasks,
/// waiters of the handle that never completes, and live bytes after a
/// collection.
fn turns(ctx: &mut Context, turns: usize, with_race: bool) -> [usize; 4] {
    let forever = input(ctx, 4);
    for turn in 0..turns {
        let task = input(ctx, 4);
        let handle = if with_race {
            race(ctx, &[task, forever], 4)
        } else {
            let array = jobs(ctx, &[task, forever]);
            // SAFETY: the array holds registered four-byte result handles.
            unsafe { ctx.async_all(array, 4, 4, 0) }
        };
        complete_i32(ctx, task, turn as i32);
        step(ctx);
        if with_race {
            assert_eq!(read_i32(ctx, handle), turn as i32);
        }
        release(ctx, &[handle, task]);
    }
    ctx.collect();
    let counts = [
        ctx.async_frames.len(),
        ctx.async_unfinished(),
        waiters(ctx, forever),
        ctx.live_bytes(),
    ];
    if with_race {
        // Rule 5 holds for the input that every race lost, after the
        // pruning of the list of observed inputs.
        fail(ctx, forever, "Error: forever", 99);
        release(ctx, &[forever]);
        assert!(!ctx.trapped());
    } else {
        release(ctx, &[forever]);
    }
    counts
}

/// Rule 6: after each race has its completion it holds no input, so the
/// counts do not grow with the count of turns. Control: `Promise.all` in
/// the same loop cannot complete, and each aggregate stays registered with
/// a waiter on the handle that never completes.
#[test]
fn a_race_per_turn_keeps_bounded_counts() {
    for with_race in [true, false] {
        let mut counts = Vec::new();
        for count in [100usize, 1_000] {
            let mut ctx = Context::new();
            counts.push(turns(&mut ctx, count, with_race));
            assert!(!ctx.trapped());
        }
        if with_race {
            assert_eq!(counts[0], counts[1]);
            assert_eq!(counts[0][..3], [1, 1, 0]);
        } else {
            assert_eq!(counts[0][..3], [101, 1, 100]);
            assert_eq!(counts[1][..3], [1_001, 1, 1_000]);
            assert!(counts[1][3] > counts[0][3]);
        }
    }
}

/// Rule 6 in the ship arena: the same loop keeps a bounded frame count.
#[test]
fn a_race_per_turn_keeps_bounded_counts_in_the_ship_arena() {
    let mut counts = Vec::new();
    for count in [100usize, 1_000] {
        let mut ctx = Context::new_releasing();
        counts.push(turns(&mut ctx, count, true));
    }
    assert_eq!(counts[0][..3], counts[1][..3]);
}

/// Rule 7: an empty array traps with its own kind at the position of the
/// call and returns null, through the Context and through the C entry.
#[test]
fn an_empty_race_traps_with_its_own_kind() {
    for mut ctx in contexts() {
        for ffi_call in [false, true] {
            let array = jobs(&mut ctx, &[]);
            // SAFETY: the Context is exclusive and the array is live.
            let handle = unsafe {
                if ffi_call {
                    crate::ffi::subscript_rt_async_race(&mut *ctx, array, 4, 73)
                } else {
                    ctx.async_race_of(array, 4, 73)
                }
            };
            assert!(handle.is_null());
            let trap = ctx.trap_record().expect("an empty race traps");
            assert_eq!(
                (trap.kind, trap.message.as_str(), trap.pos_id),
                (
                    TrapKind::PromiseRaceEmpty,
                    "Promise.race of an empty array",
                    73
                )
            );
            assert_eq!(TrapKind::from_u32(38), Some(TrapKind::PromiseRaceEmpty));
            assert_eq!(TrapKind::PromiseRaceEmpty.rule(), "promise-race-empty");
            ctx.clear_trap();
            assert!(ctx.async_frames.is_empty());
        }
    }
}

/// The C entry follows the protocol of the Context call.
#[test]
fn the_c_entry_and_the_context_call_share_the_protocol() {
    for mut ctx in contexts() {
        for ffi_call in [false, true] {
            let a = input(&mut ctx, 4);
            complete_i32(&mut ctx, a, 19);
            let array = jobs(&mut ctx, &[a]);
            // SAFETY: the Context is exclusive and the input array is live.
            let handle = unsafe {
                if ffi_call {
                    crate::ffi::subscript_rt_async_race(&mut *ctx, array, 4, 0)
                } else {
                    ctx.async_race_of(array, 4, 0)
                }
            };
            assert_eq!(unsafe { ctx.async_count(handle) }, 1);
            assert_eq!(ctx.async_pending(), 1);
            step(&mut ctx);
            assert_eq!(read_i32(&mut ctx, handle), 19);
            release(&mut ctx, &[handle, a]);
            assert!(ctx.async_frames.is_empty());
        }
    }
}

/// A host operation that completed with `text` and has a count of one.
fn host_string(ctx: &mut Context, text: &[u8]) -> *mut u8 {
    let mut endpoint = crate::context::CompletionEndpoint::default();
    // SAFETY: the metadata describes the Error layout of the host tests.
    let handle = unsafe {
        crate::ffi::subscript_rt_async_host_operation(
            ctx,
            std::mem::size_of::<usize>() as u64,
            completion_kind::STRING,
            42,
            [24u64, 0, 0, 8, 16, 0].as_ptr(),
            &mut endpoint,
        )
    };
    // SAFETY: `text` is readable for its length.
    let status = unsafe {
        crate::ffi::subscript_rt_complete_string(ctx, endpoint, text.as_ptr().cast(), text.len())
    };
    assert_eq!(status, crate::context::CompletionStatus::Ok);
    handle
}

/// Rule 10: a host string that wins stays readable through the race after
/// the release of its input and after a collection. Control: the string
/// of the input that lost is freed at the release of that input, and
/// after the release of the race each string is freed.
#[test]
fn a_host_string_that_wins_survives_the_release_of_its_input() {
    for mut ctx in contexts() {
        let word = std::mem::size_of::<usize>();
        let first = host_string(&mut ctx, b"abc");
        let second = host_string(&mut ctx, b"xyz");
        let handle = race(&mut ctx, &[first, second], word);
        let live = ctx.live_count();
        step(&mut ctx);
        release(&mut ctx, &[first, second]);
        assert!(!ctx.async_frames.contains_key(&(first as usize)));
        assert!(!ctx.async_frames.contains_key(&(second as usize)));
        // The frames of both inputs and the string that lost are freed.
        assert_eq!(ctx.live_count(), live - 3);
        ctx.collect();
        let bytes = read(&mut ctx, handle, word).expect("a value completion");
        let value = usize::from_ne_bytes(bytes.try_into().expect("one word"));
        // SAFETY: the race completion keeps the string live.
        assert_eq!(unsafe { ctx.str_bytes(value as *const u8) }, b"abc");
        release(&mut ctx, &[handle]);
        ctx.collect();
        assert!(!ctx.is_live(value));
        assert!(ctx.async_frames.is_empty());
        assert!(!ctx.trapped());
    }
}

/// Rule 12: a duplicate handle is one input for each element, with one
/// count and one waiter for each. After the detach the race holds none.
#[test]
fn a_duplicate_handle_is_one_input_for_each_element() {
    for mut ctx in contexts() {
        for duplicate_wins in [true, false] {
            let a = input(&mut ctx, 4);
            let other = input(&mut ctx, 4);
            let handle = race(&mut ctx, &[other, a, a], 4);
            assert_eq!(unsafe { ctx.async_count(a) }, 3);
            assert_eq!(waiters(&ctx, a), 2);
            if duplicate_wins {
                complete_i32(&mut ctx, a, 4);
            } else {
                complete_i32(&mut ctx, other, 9);
            }
            assert_eq!(ctx.async_pending(), if duplicate_wins { 2 } else { 1 });
            step(&mut ctx);
            assert_eq!(
                read_i32(&mut ctx, handle),
                if duplicate_wins { 4 } else { 9 }
            );
            assert_eq!(unsafe { ctx.async_count(a) }, 1);
            assert_eq!(unsafe { ctx.async_count(other) }, 1);
            if duplicate_wins {
                assert_eq!(waiters(&ctx, other), 0);
            } else {
                assert_eq!(waiters(&ctx, a), 0);
            }
            if duplicate_wins {
                complete_i32(&mut ctx, other, 5);
            } else {
                complete_i32(&mut ctx, a, 5);
            }
            step(&mut ctx);
            release(&mut ctx, &[handle, a, other]);
            assert!(ctx.async_frames.is_empty());
            assert!(!ctx.trapped());
        }
    }
}

/// A trap in the reaction job of a race, cleared by the host, leaves no
/// waiter on an input and no queued job that reaches a freed race. With
/// `queued`, a second input had its completion at the call, so its job is
/// still queued at the trap and runs after the clear with no read.
#[test]
fn a_cleared_trap_leaves_no_dangling_reaction() {
    for mut ctx in contexts() {
        for queued in [false, true] {
            let a = input(&mut ctx, 4);
            let b = input(&mut ctx, 4);
            fail(&mut ctx, a, "Error: winner", 91);
            if queued {
                complete_i32(&mut ctx, b, 2);
            }
            let handle = race(&mut ctx, &[a, b], 4);
            release(&mut ctx, &[handle]);
            step(&mut ctx);
            let trap = ctx
                .trap_record()
                .expect("an unobserved winner exception traps");
            assert_eq!((trap.kind, trap.pos_id), (TrapKind::UncaughtException, 91));
            ctx.clear_trap();
            assert_eq!(
                ctx.async_frames.contains_key(&(handle as usize)),
                queued,
                "only a queued job keeps the race registered"
            );
            if !queued {
                assert_eq!(waiters(&ctx, b), 0);
                complete_i32(&mut ctx, b, 3);
            }
            step(&mut ctx);
            assert!(!ctx.trapped());
            assert!(!ctx.async_frames.contains_key(&(handle as usize)));
            release(&mut ctx, &[a, b]);
            assert!(!ctx.trapped());
            assert!(ctx.async_frames.is_empty());
        }
    }
}
