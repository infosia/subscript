//! The barrier of the runtime's own writes (`compiler.md` §356 rule 14).
//!
//! Each test starts a cycle of a ship Context, runs steps until the block
//! that the operation writes is in the state that can hide a reference (it
//! has its mark, or its divided scan is in progress), runs the operation,
//! removes each other reference, ends the cycle, and reads the check of
//! rule 20. Three tests also build the same shape through a write with no
//! barrier, and the check reports the hidden blocks.
//!
//! Measured with one site off at a time: the insert into an existing entry,
//! the exception completion, and the completion of a task-group join each
//! stop their own test. Two sites are covered by another barrier: the
//! element of a `Promise.all` result by the barrier of the completion of
//! the input, and the moves of `unshift` by the barrier of `array_push`.
//! With both barriers off, the test of each of the two fails.
//!
//! Cost: the largest heap is an array of 4,096 references to small objects
//! (32 KiB of storage and 128 KiB of objects). Each test builds at most two
//! such heaps, one Context at a time. The module runs in under 0.1 s in a
//! release build.

use super::*;
use crate::arrops::{self, ElemKind};
use crate::assocops::{self, KeyKind};
use crate::exception::Completion;

/// A user class id that can hold a handle.
const OBJECT_CLASS: u32 = 900;

/// The count of the elements of an array whose storage is a large payload
/// with a divided scan.
const LARGE_COUNT: usize = 4096;

/// The budget of one step for a large payload.
const LARGE_BUDGET: usize = 256;

fn object(ctx: &mut Context) -> usize {
    let object = ctx.alloc(16, OBJECT_CLASS, 0);
    assert!(!object.is_null());
    object as usize
}

/// A new object whose word 0 holds `block`. The tests remove the reference
/// with a store of null, which needs no barrier.
fn holder(ctx: &mut Context, block: usize) -> usize {
    let holder = object(ctx);
    // SAFETY: the object has a payload of two words.
    unsafe { (holder as *mut usize).write(block) };
    holder
}

fn drop_held(holder: usize) {
    // SAFETY: as `holder`.
    unsafe { (holder as *mut usize).write(0) };
}

/// Root slots that outlive the Context that registers them.
struct Roots(Box<[usize]>);

impl Roots {
    fn new(ctx: &mut Context, words: &[usize]) -> Self {
        let roots = Self(words.to_vec().into_boxed_slice());
        // SAFETY: the slots live until the end of the test, after the
        // Context.
        unsafe { ctx.root_add(roots.0.as_ptr() as usize, roots.0.len()) };
        roots
    }

    fn set(&mut self, index: usize, value: usize) {
        self.0[index] = value;
    }
}

/// A ship Context in mode 0 that keeps the result of the check at each end.
fn ship() -> Box<Context> {
    let mut ctx = Context::new_releasing();
    ctx.record_mark_checks();
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    ctx
}

fn raw(ctx: &mut Context) -> *mut Context {
    ctx
}

std::thread_local! {
    /// The check of a pause that [`finish_cycle`] ran before its steps
    /// (`compiler.md` §356 rule 20a).
    static PAUSE_CHECK: std::cell::RefCell<Option<crate::context::mark_cycle::MarkCheck>> = const { std::cell::RefCell::new(None) };
}

/// Runs the check of a pause after the operation of a test, then runs
/// host-timed pauses of `budget` words until the cycle ends.
fn finish_cycle(ctx: &mut Context, budget: usize) {
    let check = ctx.mark_pause_check();
    PAUSE_CHECK.with(|cell| *cell.borrow_mut() = Some(check));
    for _ in 0..10_000_000 {
        match ctx.collect_step(budget) {
            1 => {}
            0 => return,
            other => panic!("collect_step returned {other}"),
        }
    }
    panic!("the cycle does not end");
}

/// Runs steps of `budget` words until `done` is true.
fn step_until(ctx: &mut Context, budget: usize, done: impl Fn(&Context) -> bool) {
    for _ in 0..1_000_000 {
        if done(ctx) {
            return;
        }
        assert_eq!(ctx.collect_step(budget), 1, "the cycle ended too early");
    }
    panic!("the state is not reached");
}

/// The count of the reachable blocks with no mark at the one end.
fn findings(ctx: &Context) -> u64 {
    let checks = ctx.mark_checks();
    assert_eq!(checks.len(), 1, "a check runs at the end");
    checks[0].count
}

fn assert_no_findings(ctx: &Context) {
    PAUSE_CHECK.with(|cell| {
        let check = cell.borrow_mut().take().expect("the check of a pause ran");
        assert_eq!(check.count, 0, "{}", check.message());
    });
    let checks = ctx.mark_checks();
    assert_eq!(checks.len(), 1, "a check runs at the end");
    assert_eq!(checks[0].count, 0, "{}", checks[0].message());
}

/// An array that can hold handles, with `count` references to new objects.
fn references(ctx: &mut Context, count: usize) -> usize {
    let array = ctx.array_with_capacity_of(count, 8, ArrayElements::CanHoldHandle, 0);
    assert!(!array.is_null());
    for _ in 0..count {
        let element = object(ctx);
        // SAFETY: the array is live and has capacity for `count` elements.
        assert!(unsafe { ctx.array_push(array, (&element as *const usize).cast(), 0) } > 0);
    }
    array as usize
}

fn data(ctx: &Context, array: usize) -> usize {
    // SAFETY: the tests pass live arrays.
    unsafe { ctx.array_data(array as *const u8) as usize }
}

fn element(ctx: &Context, array: usize, index: usize) -> usize {
    // SAFETY: the tests read elements below the length.
    unsafe {
        (ctx.array_data(array as *const u8) as *const usize)
            .add(index)
            .read()
    }
}

fn set_element(ctx: &Context, array: usize, index: usize, value: usize) {
    // SAFETY: the tests write elements below the length.
    unsafe {
        (ctx.array_data(array as *const u8) as *mut usize)
            .add(index)
            .write(value)
    };
}

fn elements(ctx: &Context, array: usize) -> Vec<usize> {
    // SAFETY: the tests pass live arrays.
    let len = unsafe { ctx.array_len(array as *const u8) } as usize;
    (0..len).map(|index| element(ctx, array, index)).collect()
}

/// An array whose storage is a large payload, held by the only root, with a
/// divided scan of its storage at about half of the payload.
fn half_scanned(ctx: &mut Context) -> (usize, Roots) {
    let array = references(ctx, LARGE_COUNT);
    let roots = Roots::new(ctx, &[array]);
    let storage = data(ctx, array);
    assert_eq!(ctx.collect_step(LARGE_BUDGET), 1, "the start");
    step_until(ctx, LARGE_BUDGET, |ctx| {
        ctx.mark_divided_scan()
            .is_some_and(|(payload, next, words)| payload == storage && next >= words / 2)
    });
    (array, roots)
}

/// Each element that is not null is live after the sweep.
fn assert_elements_live(ctx: &Context, array: usize) {
    for element in elements(ctx, array) {
        assert!(element == 0 || ctx.is_live(element));
    }
}

/// An array A has its mark and a scanned storage; a block B has no mark.
/// The push of B grows the storage of A, and the root of B goes. With the
/// barrier of `array_push` the check passes; with the push of no barrier
/// it reports B.
#[test]
fn a_growth_of_an_array_during_a_mark_keeps_the_pushed_block() {
    for barrier in [true, false] {
        let mut ctx = ship();
        let a = references(&mut ctx, 4);
        let b = object(&mut ctx);
        let c = holder(&mut ctx, b);
        // The work list pops the last root first: A, then C.
        let _roots = Roots::new(&mut ctx, &[c, a]);
        let storage = data(&ctx, a);
        assert_eq!(ctx.collect_step(1), 1, "the start");
        step_until(&mut ctx, 1, |ctx| ctx.collection_marked(storage));
        assert!(!ctx.collection_marked(b));
        let src = (&b as *const usize).cast();
        // SAFETY: `a` is a live array of eight-byte elements.
        let len = unsafe {
            if barrier {
                ctx.array_push(a as *mut u8, src, 0)
            } else {
                ctx.array_push_unmarked(a as *mut u8, src, 0).0
            }
        };
        assert_eq!(len, 5);
        assert_ne!(data(&ctx, a), storage, "the push grew the storage");
        drop_held(c);
        finish_cycle(&mut ctx, 1);
        if barrier {
            assert_no_findings(&ctx);
            assert!(ctx.is_live(b));
            assert_elements_live(&ctx, a);
        } else {
            assert_eq!(findings(&ctx), 1);
            assert_eq!(ctx.mark_checks()[0].findings[0].block, b);
        }
    }
}

/// `reverse` on an array whose divided scan is at half of its storage moves
/// the elements that the scan did not read into the part that it read. With
/// the barrier the check passes; with the same moves through raw writes it
/// reports the moved blocks.
#[test]
fn reverse_on_an_array_whose_scan_is_in_progress() {
    for barrier in [true, false] {
        let mut ctx = ship();
        let (array, _roots) = half_scanned(&mut ctx);
        if barrier {
            // SAFETY: `array` is a live array of `ctx`.
            unsafe { arrops::reverse(raw(&mut ctx), array as *mut u8) };
        } else {
            let mut values = elements(&ctx, array);
            values.reverse();
            for (index, value) in values.into_iter().enumerate() {
                set_element(&ctx, array, index, value);
            }
        }
        finish_cycle(&mut ctx, LARGE_BUDGET);
        if barrier {
            assert_no_findings(&ctx);
            assert_elements_live(&ctx, array);
        } else {
            assert!(findings(&ctx) > 100, "{}", ctx.mark_checks()[0].message());
        }
    }
}

/// A comparator that orders the words from the largest to the smallest.
unsafe extern "C" fn descending(_ctx: *mut Context, _env: *const u8, a: u64, b: u64) -> i32 {
    match b.cmp(&a) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// `sort` on an array whose divided scan is in progress.
#[test]
fn sort_on_an_array_whose_scan_is_in_progress() {
    let mut ctx = ship();
    let (array, _roots) = half_scanned(&mut ctx);
    let before = elements(&ctx, array);
    let code = descending as unsafe extern "C" fn(*mut Context, *const u8, u64, u64) -> i32;
    // SAFETY: `array` is a live array of eight-byte words, and the
    // comparator has the shape `(ctx, env, u64, u64) -> i32`.
    unsafe {
        arrops::sort(
            raw(&mut ctx),
            array as *mut u8,
            code as *const u8,
            std::ptr::null(),
            ElemKind::Int,
        )
    };
    assert_ne!(elements(&ctx, array), before, "the sort moved the elements");
    finish_cycle(&mut ctx, LARGE_BUDGET);
    assert_no_findings(&ctx);
    assert_elements_live(&ctx, array);
}

/// `splice` on an array whose divided scan is in progress moves the tail
/// toward the front, into the part that the scan read.
#[test]
fn splice_on_an_array_whose_scan_is_in_progress() {
    let mut ctx = ship();
    let (array, _roots) = half_scanned(&mut ctx);
    // SAFETY: `array` is a live array of `ctx`.
    let removed = unsafe { arrops::splice(raw(&mut ctx), array as *mut u8, 100, 1000, 0) };
    assert!(!removed.is_null());
    // SAFETY: as above.
    assert_eq!(
        unsafe { ctx.array_len(array as *const u8) } as usize,
        LARGE_COUNT - 1000
    );
    finish_cycle(&mut ctx, LARGE_BUDGET);
    assert_no_findings(&ctx);
    assert_elements_live(&ctx, array);
}

/// `fill` on an array whose divided scan is in progress writes a block B
/// with no mark into the part that the scan read, and the root of B goes.
#[test]
fn fill_on_an_array_whose_scan_is_in_progress() {
    let mut ctx = ship();
    let array = references(&mut ctx, LARGE_COUNT);
    let b = object(&mut ctx);
    let c = holder(&mut ctx, b);
    let _roots = Roots::new(&mut ctx, &[c, array]);
    let storage = data(&ctx, array);
    assert_eq!(ctx.collect_step(LARGE_BUDGET), 1, "the start");
    step_until(&mut ctx, LARGE_BUDGET, |ctx| {
        ctx.mark_divided_scan()
            .is_some_and(|(payload, next, words)| payload == storage && next >= words / 2)
    });
    assert!(!ctx.collection_marked(b));
    // SAFETY: `array` is a live array of eight-byte elements, and the
    // value is one readable word.
    unsafe {
        arrops::fill(
            raw(&mut ctx),
            array as *mut u8,
            (&b as *const usize).cast(),
            0,
            100,
        )
    };
    drop_held(c);
    finish_cycle(&mut ctx, LARGE_BUDGET);
    assert_no_findings(&ctx);
    assert!(ctx.is_live(b));
}

/// `copyWithin` on an array whose divided scan is in progress copies the
/// part that the scan did not read into the part that it read. Stores of
/// null then clear the source range, so the copies hold the only
/// references.
#[test]
fn copy_within_on_an_array_whose_scan_is_in_progress() {
    let mut ctx = ship();
    let (array, _roots) = half_scanned(&mut ctx);
    let tail = LARGE_COUNT - 100;
    // SAFETY: `array` is a live array of `ctx`.
    unsafe {
        arrops::copy_within(
            raw(&mut ctx),
            array as *mut u8,
            0,
            tail as i32,
            LARGE_COUNT as i32,
        )
    };
    for index in tail..LARGE_COUNT {
        set_element(&ctx, array, index, 0);
    }
    finish_cycle(&mut ctx, LARGE_BUDGET);
    assert_no_findings(&ctx);
    assert_elements_live(&ctx, array);
}

/// A result of `slice` and of `concat` is allocated during the mark, so it
/// has the mark and is never scanned. The source loses its elements before
/// its scan. The result keeps the elements.
#[test]
fn a_result_of_slice_and_concat_during_a_mark_keeps_the_elements() {
    for concat in [false, true] {
        let mut ctx = ship();
        let a = references(&mut ctx, 8);
        let mut roots = Roots::new(&mut ctx, &[a, 0]);
        assert_eq!(ctx.collect_step(1), 1, "the start");
        assert!(!ctx.collection_marked(a));
        let ctx_ptr = raw(&mut ctx);
        // SAFETY: `a` is a live array of `ctx`.
        let result = unsafe {
            if concat {
                arrops::concat(ctx_ptr, a as *mut u8, a as *mut u8, 0)
            } else {
                arrops::slice(ctx_ptr, a as *mut u8, 0, 8, 0)
            }
        } as usize;
        assert!(ctx.collection_marked(result), "rule 4");
        roots.set(1, result);
        for index in 0..8 {
            set_element(&ctx, a, index, 0);
        }
        finish_cycle(&mut ctx, 1);
        assert_no_findings(&ctx);
        let kept = elements(&ctx, result);
        assert_eq!(kept.len(), if concat { 16 } else { 8 });
        for element in kept {
            assert!(element != 0 && ctx.is_live(element));
        }
    }
}

/// A Map M and its entry storage have their marks, and a block B has none.
/// The insert of B as a value grows the entry storage, and the root of B
/// goes. A Set of references does the same with B as a key.
#[test]
fn a_growth_of_a_map_and_of_a_set_during_a_mark_keeps_the_inserted_block() {
    for is_set in [false, true] {
        let mut ctx = ship();
        let (kind, value_size) = if is_set {
            (KeyKind::Ref, 0)
        } else {
            (KeyKind::Bits, 8)
        };
        let map = assocops::new(&mut ctx, 8, value_size, kind, is_set, 0) as usize;
        assert_ne!(map, 0);
        let ctx_ptr = raw(&mut ctx);
        // The first entries fill the initial storage of four entries.
        for key in 1..=4usize {
            let value = object(&mut ctx);
            let (key, value) = if is_set { (value, 0) } else { (key, value) };
            // SAFETY: `map` is a live container with eight-byte keys and
            // values.
            unsafe {
                assocops::insert(
                    ctx_ptr,
                    map as *mut u8,
                    (&key as *const usize).cast(),
                    (&value as *const usize).cast(),
                    0,
                )
            };
        }
        let entries = entry_storage(map);
        let b = object(&mut ctx);
        let c = holder(&mut ctx, b);
        let _roots = Roots::new(&mut ctx, &[c, map]);
        assert_eq!(ctx.collect_step(1), 1, "the start");
        step_until(&mut ctx, 1, |ctx| ctx.collection_marked(entries));
        assert!(!ctx.collection_marked(b));
        let (key, value) = if is_set { (b, 0) } else { (5usize, b) };
        // SAFETY: as above.
        unsafe {
            assocops::insert(
                ctx_ptr,
                map as *mut u8,
                (&key as *const usize).cast(),
                (&value as *const usize).cast(),
                0,
            )
        };
        let grown = entry_storage(map);
        assert_ne!(grown, entries, "the insert grew the entry storage");
        drop_held(c);
        finish_cycle(&mut ctx, 1);
        assert_no_findings(&ctx);
        assert!(ctx.is_live(b));
    }
}

/// An insert with no growth during a mark: the words of the new entry go to
/// the work list.
#[test]
fn an_insert_into_a_marked_map_keeps_the_value() {
    let mut ctx = ship();
    let map = assocops::new(&mut ctx, 8, 8, KeyKind::Bits, false, 0) as usize;
    let ctx_ptr = raw(&mut ctx);
    let (first_key, first) = (1usize, object(&mut ctx));
    // SAFETY: `map` is a live map with eight-byte keys and values.
    unsafe {
        assocops::insert(
            ctx_ptr,
            map as *mut u8,
            (&first_key as *const usize).cast(),
            (&first as *const usize).cast(),
            0,
        )
    };
    let entries = entry_storage(map);
    let b = object(&mut ctx);
    let c = holder(&mut ctx, b);
    let _roots = Roots::new(&mut ctx, &[c, map]);
    assert_eq!(ctx.collect_step(1), 1, "the start");
    step_until(&mut ctx, 1, |ctx| ctx.collection_marked(entries));
    assert!(!ctx.collection_marked(b));
    let key = 2usize;
    // SAFETY: as above.
    unsafe {
        assocops::insert(
            ctx_ptr,
            map as *mut u8,
            (&key as *const usize).cast(),
            (&b as *const usize).cast(),
            0,
        )
    };
    assert_eq!(entry_storage(map), entries);
    drop_held(c);
    finish_cycle(&mut ctx, 1);
    assert_no_findings(&ctx);
    assert!(ctx.is_live(b));
}

/// The entry storage of a Map or a Set: word 9 of its header.
fn entry_storage(map: usize) -> usize {
    // SAFETY: the tests pass live container headers of 13 words.
    unsafe { (map as *const usize).add(9).read() }
}

fn register(ctx: &mut Context, size: usize) -> usize {
    let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
    assert!(!frame.is_null());
    // SAFETY: the fresh allocation has the emitted async header.
    unsafe { ctx.async_register(frame, size) };
    frame as usize
}

/// A task frame F that an outside holder keeps has its mark, and a block B
/// has none. The completion of F takes B, and the root of B goes. The end
/// reads the completion of a completed task only through its frame, so a
/// completion with no barrier reports B.
#[test]
fn a_completion_value_set_during_a_mark_stays_live() {
    for barrier in [true, false] {
        let mut ctx = ship();
        let frame = register(&mut ctx, 8);
        let b = object(&mut ctx);
        let c = holder(&mut ctx, b);
        let _roots = Roots::new(&mut ctx, &[c, frame]);
        assert_eq!(ctx.collect_step(1), 1, "the start");
        step_until(&mut ctx, 1, |ctx| ctx.collection_marked(frame));
        assert!(!ctx.collection_marked(b));
        if barrier {
            // SAFETY: the value is one readable word of the registered size.
            unsafe { ctx.async_complete(frame as *mut u8, (&b as *const usize).cast(), 8) };
        } else {
            let meta = ctx.async_frames.get_mut(&frame).expect("registered");
            meta.completion = Some(Completion::Value(b.to_ne_bytes().to_vec()));
        }
        drop_held(c);
        finish_cycle(&mut ctx, 1);
        if barrier {
            assert_no_findings(&ctx);
            assert!(ctx.is_live(b));
        } else {
            assert_eq!(findings(&ctx), 1);
            assert_eq!(ctx.mark_checks()[0].findings[0].block, b);
        }
    }
}

/// The result array of `Promise.all` has its mark and a scanned storage.
/// An input completes with a block B during the mark, the reaction copies B
/// into the result array and releases the input, and the root of B goes.
#[test]
fn a_result_array_of_promise_all_during_a_mark_keeps_the_values() {
    let mut ctx = ship();
    let inputs = [register(&mut ctx, 8), register(&mut ctx, 8)];
    let jobs = ctx.array_new(8, 0);
    for input in &inputs {
        // SAFETY: `jobs` is a live array of eight-byte elements.
        unsafe { ctx.array_push(jobs, (input as *const usize).cast(), 0) };
    }
    // SAFETY: the inputs are registered tasks with eight-byte results.
    let all = unsafe { ctx.async_all(jobs, 8, 8, 0) } as usize;
    assert_ne!(all, 0);
    // Word 0 of the roots of an aggregate is its result array.
    let result = ctx
        .async_aggregate_roots()
        .into_iter()
        .find(|root| root.1 == 0)
        .map(|root| root.2)
        .expect("an aggregate");
    let values = [object(&mut ctx), object(&mut ctx)];
    let holders = [holder(&mut ctx, values[0]), holder(&mut ctx, values[1])];
    let _roots = Roots::new(&mut ctx, &[holders[0], holders[1], all]);
    let storage = data(&ctx, result);
    assert_eq!(ctx.collect_step(1), 1, "the start");
    step_until(&mut ctx, 1, |ctx| ctx.collection_marked(storage));
    assert!(!ctx.collection_marked(values[0]) && !ctx.collection_marked(values[1]));
    for (input, value) in inputs.iter().zip(values) {
        // SAFETY: the value is one readable word of the registered size.
        unsafe { ctx.async_complete(*input as *mut u8, (&value as *const usize).cast(), 8) };
    }
    // SAFETY: the ready queue holds the reactions of the aggregate.
    unsafe { ctx.async_step() };
    assert_eq!(elements(&ctx, result), values);
    drop_held(holders[0]);
    drop_held(holders[1]);
    finish_cycle(&mut ctx, 1);
    assert_no_findings(&ctx);
    for value in values {
        assert!(ctx.is_live(value));
    }
}

/// With no mark in progress, the barriers of the runtime give no work.
#[test]
fn the_barriers_give_no_work_with_no_mark() {
    let mut ctx = ship();
    let a = references(&mut ctx, 8);
    let ctx_ptr = raw(&mut ctx);
    // SAFETY: `a` is a live array of `ctx`.
    unsafe {
        arrops::reverse(ctx_ptr, a as *mut u8);
        let _ = arrops::slice(ctx_ptr, a as *mut u8, 0, 8, 0);
    }
    assert!(!ctx.mark_in_progress());
    assert_eq!(ctx.mark_work_words(), 0);
}

/// `shift` on an array whose divided scan is in progress moves the element
/// at the position of the scan into the part that the scan read.
#[test]
fn shift_on_an_array_whose_scan_is_in_progress() {
    let mut ctx = ship();
    let (array, _roots) = half_scanned(&mut ctx);
    let mut first = 0usize;
    // SAFETY: `array` is a live array of eight-byte elements, and `first`
    // is one writable word.
    assert!(unsafe {
        arrops::shift(
            raw(&mut ctx),
            array as *mut u8,
            (&mut first as *mut usize).cast(),
            0,
        )
    });
    finish_cycle(&mut ctx, LARGE_BUDGET);
    assert_no_findings(&ctx);
    assert_elements_live(&ctx, array);
}

/// An insert into an existing entry during a mark: the entry storage has
/// its mark, and the insert replaces the value of the entry with a block B
/// that has none. The root of B goes, so only the entry holds B.
#[test]
fn an_insert_into_an_existing_entry_during_a_mark_keeps_the_value() {
    let mut ctx = ship();
    let map = assocops::new(&mut ctx, 8, 8, KeyKind::Bits, false, 0) as usize;
    let ctx_ptr = raw(&mut ctx);
    let (key, first) = (1usize, object(&mut ctx));
    // SAFETY: `map` is a live map with eight-byte keys and values.
    unsafe {
        assocops::insert(
            ctx_ptr,
            map as *mut u8,
            (&key as *const usize).cast(),
            (&first as *const usize).cast(),
            0,
        )
    };
    let entries = entry_storage(map);
    let b = object(&mut ctx);
    let c = holder(&mut ctx, b);
    let _roots = Roots::new(&mut ctx, &[c, map]);
    assert_eq!(ctx.collect_step(1), 1, "the start");
    step_until(&mut ctx, 1, |ctx| ctx.collection_marked(entries));
    assert!(!ctx.collection_marked(b));
    // SAFETY: as above.
    unsafe {
        assocops::insert(
            ctx_ptr,
            map as *mut u8,
            (&key as *const usize).cast(),
            (&b as *const usize).cast(),
            0,
        )
    };
    assert_eq!(entry_storage(map), entries);
    drop_held(c);
    finish_cycle(&mut ctx, 1);
    assert_no_findings(&ctx);
    assert!(ctx.is_live(b));
}

/// A task frame F that an outside holder keeps has its mark, and a block B
/// has none. B becomes the pending exception, its root goes, and F
/// completes with the exception. The end reads the completion of a
/// completed task only through its frame.
#[test]
fn an_exception_completion_during_a_mark_keeps_the_object() {
    let mut ctx = ship();
    let frame = register(&mut ctx, 0);
    let b = ctx.alloc(24, 1, 0) as usize;
    let c = holder(&mut ctx, b);
    let _roots = Roots::new(&mut ctx, &[c, frame]);
    assert_eq!(ctx.collect_step(1), 1, "the start");
    step_until(&mut ctx, 1, |ctx| ctx.collection_marked(frame));
    assert!(!ctx.collection_marked(b));
    // SAFETY: `b` is a live object of this Context.
    unsafe { ctx.raise_exception(b as *mut u8, "Error: stored".to_string(), 0) };
    drop_held(c);
    ctx.async_complete_exception(frame as *mut u8);
    assert!(!ctx.exception_pending());
    finish_cycle(&mut ctx, 1);
    assert_no_findings(&ctx);
    assert!(ctx.is_live(b));
}

/// `unshift` of a block B on an array whose divided scan is at half of its
/// storage. The element moves to index 0, in the part that the scan read,
/// and the root of B goes.
#[test]
fn unshift_on_an_array_whose_scan_is_in_progress() {
    let mut ctx = ship();
    let (array, mut roots) = half_scanned(&mut ctx);
    let b = object(&mut ctx);
    roots.set(0, array);
    // SAFETY: `array` is a live array of eight-byte elements, and `b` is one
    // readable word.
    let len = unsafe {
        arrops::unshift(
            raw(&mut ctx),
            array as *mut u8,
            (&b as *const usize).cast(),
            0,
        )
    };
    assert_eq!(len as usize, LARGE_COUNT + 1);
    assert_eq!(element(&ctx, array, 0), b);
    finish_cycle(&mut ctx, LARGE_BUDGET);
    assert_no_findings(&ctx);
    assert!(ctx.is_live(b));
    assert_elements_live(&ctx, array);
}

/// The join of a task group whose input failed before the cycle. The join
/// frame has its mark, and the input frame and its exception object have
/// none. The reaction moves the exception into the completion of the join
/// and releases the input before the mark reaches it.
#[test]
fn a_task_group_join_during_a_mark_keeps_the_first_exception() {
    let mut ctx = ship();
    let group = ctx.task_group_create(1);
    let input = register(&mut ctx, 0) as *mut u8;
    let object = ctx.alloc(24, 1, 0) as usize;
    // SAFETY: `object` is a live object of this Context.
    unsafe { ctx.raise_exception(object as *mut u8, "Error: failed".to_string(), 0) };
    ctx.async_complete_exception(input);
    // SAFETY: `group` and `input` are live, and `input` transfers its count.
    let join = unsafe {
        ctx.task_group_add(group, input, 0);
        ctx.task_group_join(group, 0)
    } as usize;
    assert_eq!(ctx.collect_step(1), 1, "the start");
    step_until(&mut ctx, 1, |ctx| ctx.collection_marked(join));
    assert!(!ctx.collection_marked(input as usize));
    assert!(!ctx.collection_marked(object));
    // SAFETY: the ready queue holds the reaction of the group.
    let report = unsafe { ctx.async_step_budget(1) };
    assert_eq!(report.dispatched, 1);
    finish_cycle(&mut ctx, 1);
    assert_no_findings(&ctx);
    assert!(ctx.is_live(object));
}

/// The most pauses after the start that a cycle of the tests of a move
/// between two steps can take. The heap of [`references`] with
/// [`LARGE_COUNT`] elements is about 12,300 words: 49 budgets of
/// [`LARGE_BUDGET`] words.
const MOVE_PAUSES: usize = 200;

/// A comparator that orders the words from the smallest to the largest.
unsafe extern "C" fn ascending(_ctx: *mut Context, _env: *const u8, a: u64, b: u64) -> i32 {
    match a.cmp(&b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// `compiler.md` §356 rule 14a: `operation` runs on an array of
/// [`LARGE_COUNT`] references before each host-timed pause of
/// [`LARGE_BUDGET`] words. The cycle ends within [`MOVE_PAUSES`] pauses,
/// the check of each pause and the check of the end give 0, and each
/// element that is still in the array is live after the sweep. Returns the
/// count of the operations that ran during the divided scan of the storage.
fn moves_between_steps(operation: impl Fn(*mut Context, usize, usize)) -> usize {
    let mut ctx = ship();
    ctx.record_mark_pause_checks();
    let array = references(&mut ctx, LARGE_COUNT);
    let _roots = Roots::new(&mut ctx, &[array]);
    let storage = data(&ctx, array);
    assert_eq!(ctx.collect_step(LARGE_BUDGET), 1, "the start");
    let mut divided = 0;
    for pauses in 0.. {
        assert!(
            pauses < MOVE_PAUSES,
            "the cycle does not end after {pauses} pauses"
        );
        if ctx
            .mark_divided_scan()
            .is_some_and(|(payload, _, _)| payload == storage)
        {
            divided += 1;
        }
        operation(raw(&mut ctx), array, pauses);
        if ctx.collect_step(LARGE_BUDGET) == 0 {
            break;
        }
    }
    assert_eq!(data(&ctx, array), storage, "no operation moved the storage");
    for check in ctx.mark_pause_checks() {
        assert_eq!(check.count, 0, "{}", check.message());
    }
    let checks = ctx.mark_checks();
    assert_eq!(checks.len(), 1, "a check runs at the end");
    assert_eq!(checks[0].count, 0, "{}", checks[0].message());
    assert_elements_live(&ctx, array);
    divided
}

/// Rule 14a: a queue, a loop of `shift`, between each two steps. The
/// position of the divided scan stays, so the cycle ends.
#[test]
fn a_shift_between_each_two_steps_ends_the_cycle() {
    let divided = moves_between_steps(|ctx, array, _| {
        let mut first = 0usize;
        // SAFETY: `array` is a live array of eight-byte elements, and
        // `first` is one writable word.
        assert!(unsafe {
            arrops::shift(ctx, array as *mut u8, (&mut first as *mut usize).cast(), 0)
        });
    });
    assert!(divided > 5, "{divided}");
}

/// Rule 14a: a `sort` between each two steps, in the two orders by turns,
/// so that each sort moves the elements.
#[test]
fn a_sort_between_each_two_steps_ends_the_cycle() {
    let divided = moves_between_steps(|ctx, array, pause| {
        let code = if pause % 2 == 0 {
            descending as unsafe extern "C" fn(*mut Context, *const u8, u64, u64) -> i32
        } else {
            ascending
        };
        // SAFETY: `array` is a live array of eight-byte words, and the
        // comparator has the shape `(ctx, env, u64, u64) -> i32`.
        unsafe {
            arrops::sort(
                ctx,
                array as *mut u8,
                code as *const u8,
                std::ptr::null(),
                ElemKind::Int,
            )
        };
    });
    assert!(divided > 5, "{divided}");
}

/// Rule 14a: a `reverse` between each two steps.
#[test]
fn a_reverse_between_each_two_steps_ends_the_cycle() {
    let divided = moves_between_steps(|ctx, array, _| {
        // SAFETY: `array` is a live array of `ctx`.
        unsafe { arrops::reverse(ctx, array as *mut u8) };
    });
    assert!(divided > 5, "{divided}");
}

std::thread_local! {
    /// The array that [`replacing`] sorts, and the count of its calls.
    static SORTED: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

/// A comparator that, at its first call, clears element 0 of the array
/// that it sorts, so that only the scratch buffer of the sort holds that
/// value, and then runs steps until the scan of the storage is complete.
unsafe extern "C" fn replacing(ctx: *mut Context, env: *const u8, a: u64, b: u64) -> i32 {
    let (array, calls) = SORTED.with(std::cell::Cell::get);
    SORTED.with(|cell| cell.set((array, calls + 1)));
    if calls == 0 {
        // SAFETY: the test passes a live Context with no script call.
        let ctx = unsafe { &mut *ctx };
        set_element(ctx, array, 0, 0);
        let storage = data(ctx, array);
        step_until(ctx, 1, |ctx| ctx.collection_marked(storage));
    }
    // SAFETY: the comparator has the shape of `descending`.
    unsafe { descending(ctx, env, a, b) }
}

/// Rule 14: the write-back of `sort` is a copy from the scratch buffer. A
/// comparator clears element 0, so only the scratch holds a block X, and a
/// step completes the scan of the storage. The write-back puts X into the
/// storage, which has the mark, and the barrier of the written elements
/// gives X.
#[test]
fn a_sort_write_back_gives_a_value_that_only_the_scratch_held() {
    let mut ctx = ship();
    let array = references(&mut ctx, 8);
    let x = element(&ctx, array, 0);
    let _roots = Roots::new(&mut ctx, &[array]);
    assert_eq!(ctx.collect_step(1), 1, "the start");
    SORTED.with(|cell| cell.set((array, 0)));
    let code = replacing as unsafe extern "C" fn(*mut Context, *const u8, u64, u64) -> i32;
    // SAFETY: `array` is a live array of eight-byte words, and the
    // comparator has the shape `(ctx, env, u64, u64) -> i32`.
    unsafe {
        arrops::sort(
            raw(&mut ctx),
            array as *mut u8,
            code as *const u8,
            std::ptr::null(),
            ElemKind::Int,
        )
    };
    assert!(elements(&ctx, array).contains(&x), "the write-back keeps X");
    assert!(!ctx.collection_marked(x));
    finish_cycle(&mut ctx, 1);
    assert_no_findings(&ctx);
    assert!(ctx.is_live(x));
}
