//! The mark of a collection in steps (`compiler.md` §356).
//!
//! Cost: the largest heap is the array of 1,000,000 references of
//! acceptance 1: 8 MB of storage and 32 MB of objects, built two times
//! (one Context with steps, one with a budget of 0), one Context at a
//! time. The other tests build heaps under 2 MiB. The module runs in about
//! 0.1 s in a release build and 1.6 s in a debug build, on one thread.

use super::*;
use crate::ffi::{
    subscript_rt_ctx_collect_step, subscript_rt_ctx_set_collection_step, subscript_rt_mark_block,
    subscript_rt_mark_range, subscript_rt_mark_store,
};

/// A user class id that can hold a handle.
const OBJECT_CLASS: u32 = 900;

/// The request of one small object: a block of 32 bytes, a payload of two
/// words.
const OBJECT_BYTES: usize = 16;

fn object(ctx: &mut Context) -> usize {
    let object = ctx.alloc(OBJECT_BYTES, OBJECT_CLASS, 0);
    assert!(!object.is_null());
    object as usize
}

/// Writes `value` into word `index` of the payload at `block`, with no
/// barrier.
fn store(block: usize, index: usize, value: usize) {
    // SAFETY: the tests store into words of payloads that they allocated.
    unsafe { (block as *mut usize).add(index).write(value) };
}

fn load(block: usize, index: usize) -> usize {
    // SAFETY: as `store`.
    unsafe { (block as *const usize).add(index).read() }
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

/// A ship Context that keeps the result of the check at each end.
fn ship() -> Box<Context> {
    let mut ctx = Context::new_releasing();
    ctx.record_mark_checks();
    ctx
}

/// Runs host-timed pauses of `budget` words until the cycle ends.
fn finish_cycle(ctx: &mut Context, budget: usize) {
    for _ in 0..10_000_000 {
        match ctx.collect_step(budget) {
            1 => {}
            0 => return,
            other => panic!("collect_step returned {other}"),
        }
    }
    panic!("the cycle does not end");
}

/// The check of each end found no block with no mark.
fn assert_checks_pass(ctx: &Context, ends: usize) {
    let checks = ctx.mark_checks();
    assert_eq!(checks.len(), ends, "a check runs at each end");
    for check in checks {
        assert_eq!(check.count, 0, "{}", check.message());
    }
}

/// An array of `count` references to new objects.
fn references(ctx: &mut Context, count: usize) -> usize {
    let array = ctx.array_with_capacity_of(count, 8, ArrayElements::CanHoldHandle, 0);
    assert!(!array.is_null());
    for index in 0..count {
        let element = object(ctx);
        // SAFETY: the array owns capacity for `count` elements.
        unsafe {
            let header = &mut *(array as *mut ArrayHeader);
            header.data.cast::<usize>().add(index).write(element);
            header.len = (index + 1) as u64;
        }
    }
    array as usize
}

/// A chain of `count` objects. Word 0 of each object holds the next one.
fn chain(ctx: &mut Context, count: usize) -> usize {
    let mut next = 0;
    for _ in 0..count {
        let block = object(ctx);
        store(block, 0, next);
        next = block;
    }
    next
}

/// Allocates garbage and polls until one automatic collection ends.
/// Returns the count of the polls.
fn poll_until_one_collection(ctx: &mut Context) -> usize {
    let before = ctx.collection_stats().automatic_collections;
    for polls in 1..10_000_000 {
        let _ = object(ctx);
        ctx.collect_poll();
        if ctx.collection_stats().automatic_collections > before {
            return polls;
        }
    }
    panic!("no collection ends");
}

/// The record of the collections of a heap of 1,000,000 references with
/// the step `budget`, after one automatic collection.
fn million_references(budget: usize) -> CollectionStats {
    let mut ctx = ship();
    let array = references(&mut ctx, 1_000_000);
    let _roots = Roots::new(&mut ctx, &[array]);
    assert!(ctx.set_collection(CollectionMode::Automatic, 1 << 20));
    assert!(ctx.set_collection_step(budget));
    ctx.reset_collection_stats();
    poll_until_one_collection(&mut ctx);
    assert!(ctx.is_live(array));
    // SAFETY: the array is live.
    let last = unsafe {
        (*(array as *const ArrayHeader))
            .data
            .cast::<usize>()
            .add(999_999)
            .read()
    };
    assert!(ctx.is_live(last), "the last element survives");
    assert_checks_pass(&ctx, usize::from(budget > 0));
    ctx.collection_stats()
}

/// Acceptance 1: with a budget of 50,000 words, no pause of the cycle
/// reads 60,000 words. With a budget of 0, the same heap is one pause of
/// more than 1,000,000 words.
#[test]
fn a_million_references_give_pauses_under_the_budget() {
    let steps = million_references(50_000);
    assert_eq!(steps.automatic_collections, 1);
    assert!(steps.mark_steps > 20, "{steps:?}");
    assert!(steps.max_pause_words_read < 60_000, "{steps:?}");
    assert!(steps.last_words_read > 1_000_000, "{steps:?}");
    assert_eq!(steps.forced_ends, 0);
    let one_pause = million_references(0);
    assert_eq!(one_pause.mark_steps, 0);
    assert!(one_pause.last_words_read > 1_000_000, "{one_pause:?}");
    assert_eq!(one_pause.max_pause_words_read, one_pause.last_words_read);
}

/// Acceptance 2: a block A has its mark and a block B has none. A raw
/// store writes B into A, and the only other reference to B goes. The check
/// reports B. With the barrier call after the store, it reports nothing.
#[test]
fn the_check_finds_a_store_with_no_barrier() {
    for barrier in [false, true] {
        let mut ctx = ship();
        let a = object(&mut ctx);
        let b = object(&mut ctx);
        let c = object(&mut ctx);
        store(c, 0, b);
        // The work list pops the last root first: A, then C.
        let _roots = Roots::new(&mut ctx, &[c, a]);
        assert!(ctx.set_collection(CollectionMode::Off, 0));
        assert_eq!(ctx.collect_step(1), 1, "the start");
        assert_eq!(ctx.collect_step(1), 1, "one step marks A");
        assert!(ctx.collection_marked(a) && !ctx.collection_marked(b));
        store(a, 0, b);
        if barrier {
            ctx.mark_store(b);
        }
        store(c, 0, 0);
        finish_cycle(&mut ctx, 1);
        let checks = ctx.mark_checks();
        assert_eq!(checks.len(), 1);
        if barrier {
            assert_eq!(checks[0].count, 0, "{}", checks[0].message());
            assert!(ctx.is_live(b));
        } else {
            assert_eq!(checks[0].count, 1);
            let finding = checks[0].findings[0];
            assert_eq!(finding.block, b);
            assert_eq!(finding.class_id, OBJECT_CLASS);
            assert_eq!(finding.holder_class, Some(OBJECT_CLASS));
            assert_eq!(finding.offset, 0);
            assert!(checks[0].message().contains("1 reachable blocks"));
        }
    }
}

/// Rule 20a: a raw store writes the address of B, which has no mark, into
/// A, which has its mark, while C keeps B alive. The check of a pause
/// reports 1 word, and the check of rule 20 at the end reports nothing.
/// With the barrier call after the store, both report nothing.
#[test]
fn the_check_of_a_pause_finds_a_store_with_no_barrier() {
    for barrier in [false, true] {
        let mut ctx = ship();
        let a = object(&mut ctx);
        let b = object(&mut ctx);
        let c = object(&mut ctx);
        store(c, 0, b);
        // The work list pops the last root first: A, then C.
        let _roots = Roots::new(&mut ctx, &[c, a]);
        assert!(ctx.set_collection(CollectionMode::Off, 0));
        assert_eq!(ctx.collect_step(1), 1, "the start");
        assert_eq!(ctx.collect_step(1), 1, "one step marks A");
        assert!(ctx.collection_marked(a) && !ctx.collection_marked(b));
        assert_eq!(ctx.mark_pause_check().count, 0, "before the store");
        store(a, 1, b);
        if barrier {
            ctx.mark_store(b);
        }
        let pause = ctx.mark_pause_check();
        assert!(pause.pause);
        if barrier {
            assert_eq!(pause.count, 0, "{}", pause.message());
        } else {
            assert_eq!(pause.count, 1);
            let finding = pause.findings[0];
            assert_eq!(finding.block, b);
            assert_eq!(finding.class_id, OBJECT_CLASS);
            assert_eq!(finding.holder_class, Some(OBJECT_CLASS));
            assert_eq!(finding.offset, 8);
            assert!(pause.message().contains("rule 20a: 1 words"));
        }
        finish_cycle(&mut ctx, 1);
        assert_checks_pass(&ctx, 1);
    }
}

/// Rule 20a in the steps: a Context that records the check of each pause
/// keeps one result for each step and for the end, and a heap with no
/// store gives no finding.
#[test]
fn the_check_of_a_pause_runs_after_each_step_and_before_the_end() {
    let mut ctx = ship();
    ctx.record_mark_pause_checks();
    let head = chain(&mut ctx, 20);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    finish_cycle(&mut ctx, 2);
    let checks = ctx.mark_pause_checks();
    assert!(checks.len() > 5, "{}", checks.len());
    assert!(checks.iter().all(|check| check.count == 0));
    assert_checks_pass(&ctx, 1);
}

/// Acceptance 3 and rule 4: a block allocated during a mark has its mark,
/// so a reference to it in a marked block with no barrier keeps it.
#[test]
fn an_allocation_during_a_mark_stays_live() {
    let mut ctx = ship();
    let a = object(&mut ctx);
    let _roots = Roots::new(&mut ctx, &[a]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    assert_eq!(ctx.collect_step(1), 1);
    assert_eq!(ctx.collect_step(1), 1);
    assert!(ctx.collection_marked(a));
    let d = object(&mut ctx);
    assert!(
        ctx.collection_marked(d),
        "rule 4: the allocation gives the mark"
    );
    store(a, 1, d);
    finish_cycle(&mut ctx, 1);
    assert!(ctx.is_live(d));
    assert!(
        !ctx.collection_marked(d),
        "the exchange makes the mark the live word"
    );
    assert_checks_pass(&ctx, 1);
}

/// Acceptance 3 and rule 9: `Context.free` during a mark of a block in the
/// work list, and a block that takes its place. A large payload whose scan
/// is divided is released during the scan.
#[test]
fn context_free_during_a_mark_and_a_block_that_takes_the_place() {
    let mut ctx = ship();
    let a = object(&mut ctx);
    let x = object(&mut ctx);
    let large = ctx.alloc(64 << 10, OBJECT_CLASS, 0) as usize;
    for index in 0..(64 << 10) / 8 {
        let element = object(&mut ctx);
        store(large, index, element);
    }
    let mut roots = Roots::new(&mut ctx, &[x, large, a]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    assert_eq!(ctx.collect_step(100), 1, "the start");
    // A step marks A and the large block, and reads part of its payload.
    assert_eq!(ctx.collect_step(100), 1);
    assert!(ctx.collection_marked(large));
    assert!(ctx.mark_work_words() > 1000, "the scan is divided");
    ctx.delete(large, 0);
    roots.set(1, 0);
    ctx.delete(x, 0);
    roots.set(0, 0);
    let y = object(&mut ctx);
    assert_eq!(y, x, "the block takes the place of the released block");
    store(y, 0, a);
    roots.set(0, y);
    finish_cycle(&mut ctx, 100);
    assert!(ctx.is_live(y) && ctx.is_live(a));
    assert!(!ctx.is_live(large));
    assert_checks_pass(&ctx, 1);
}

/// Acceptance 3 and rule 7: past one limit since the start, the cycle is
/// late. From there each poll runs one step with the budget, until the
/// end. The record counts the late cycle one time, and no pause of the
/// cycle reads more than the budget plus one small block (rule 2). The
/// second run stores a reference before each poll, as a loop does.
#[test]
fn a_late_cycle_runs_a_step_with_the_budget_at_each_poll() {
    const BUDGET: usize = 16;
    for stores in [false, true] {
        let mut ctx = ship();
        let head = chain(&mut ctx, 20_000);
        let _roots = Roots::new(&mut ctx, &[head]);
        assert!(ctx.set_collection(CollectionMode::Automatic, 64 << 10));
        assert!(ctx.set_collection_step(BUDGET));
        ctx.reset_collection_stats();
        let mut late_polls = 0;
        for polls in 0.. {
            assert!(polls < 10_000_000, "no collection ends");
            let block = object(&mut ctx);
            if stores {
                store(head, 1, block);
                ctx.mark_store(block);
            }
            let before = ctx.collection_stats();
            let late = before.forced_ends > 0 && ctx.mark_in_progress();
            if late {
                assert!(
                    ctx.collection_pending(),
                    "a late cycle runs a pause at each poll"
                );
            }
            ctx.collect_poll();
            let after = ctx.collection_stats();
            if late {
                late_polls += 1;
                assert_eq!(
                    (after.mark_steps - before.mark_steps)
                        + (after.automatic_collections - before.automatic_collections),
                    1,
                    "one pause"
                );
            }
            if after.automatic_collections > 0 {
                break;
            }
        }
        let stats = ctx.collection_stats();
        assert_eq!(stats.forced_ends, 1, "{stats:?}");
        assert_eq!(stats.automatic_collections, 1);
        // A small object of the test is a payload of two words.
        assert!(
            stats.max_pause_words_read <= (BUDGET + 2) as u64,
            "{stats:?}"
        );
        assert!(late_polls > 1, "{late_polls}");
        let mut block = head;
        let mut count = 0;
        while block != 0 {
            assert!(ctx.is_live(block));
            block = load(block, 0);
            count += 1;
        }
        assert_eq!(count, 20_000);
        if stores {
            assert!(ctx.is_live(load(head, 1)));
        }
        assert_checks_pass(&ctx, 1);
    }
}

/// Rules 6 and 18: a step that leaves the work list empty makes the next
/// pause the end, also when a store gives the list a word before it. A
/// host that stores a reference in each frame and runs one host-timed
/// pause in each frame ends the cycle.
#[test]
fn a_step_that_empties_the_work_list_makes_the_end_next() {
    let mut ctx = ship();
    let head = chain(&mut ctx, 2_000);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    assert_eq!(ctx.collect_step(1_000), 1, "the start");
    let mut pauses = 1;
    loop {
        // One frame: the script stores a reference into a block.
        let block = object(&mut ctx);
        store(head, 1, block);
        ctx.mark_store(block);
        pauses += 1;
        if ctx.collect_step(1_000) == 0 {
            break;
        }
        assert!(pauses < 100, "the cycle does not end");
    }
    let stats = ctx.collection_stats();
    assert_eq!(stats.automatic_collections, 1);
    // compiler.md §357 rule 12: the pauses after the end are sweep steps.
    assert!(stats.sweep_steps > 0);
    assert_eq!((stats.mark_steps + stats.sweep_steps) as usize, pauses - 2);
    assert!(ctx.is_live(load(head, 1)));
    assert_checks_pass(&ctx, 1);
}

/// The live bytes after one explicit collection of a heap, with a cycle in
/// progress from the start of `garbage` allocations, or with no cycle.
fn live_after_explicit(cycle: bool) -> usize {
    let mut ctx = ship();
    let head = chain(&mut ctx, 5_000);
    let _drop = chain(&mut ctx, 3_000);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    if cycle {
        assert_eq!(ctx.collect_step(500), 1);
        assert_eq!(ctx.collect_step(500), 1);
    }
    for _ in 0..1_000 {
        let _ = object(&mut ctx);
    }
    ctx.collect();
    assert!(!ctx.mark_in_progress());
    if cycle {
        assert_checks_pass(&ctx, 1);
        let stats = ctx.collection_stats();
        assert_eq!(stats.automatic_collections, 1, "the end of the cycle");
        assert_eq!(stats.explicit_collections, 1);
    }
    ctx.live_bytes()
}

/// Acceptance 3 and rule 8: an explicit collection during a cycle ends the
/// cycle, then runs one collection with no step. The live bytes are those
/// of a Context with no cycle.
#[test]
fn an_explicit_collection_during_a_cycle_gives_the_live_bytes_of_one_pause() {
    assert_eq!(live_after_explicit(true), live_after_explicit(false));
}

/// The record of a program of 40,000 allocations with polls, for the step
/// `budget`.
fn polled_program(budget: usize) -> (CollectionStats, usize) {
    let mut ctx = ship();
    let head = chain(&mut ctx, 2_000);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::Automatic, 32 << 10));
    assert!(ctx.set_collection_step(budget));
    for _ in 0..40_000 {
        let _ = object(&mut ctx);
        ctx.collect_poll();
        assert!(!ctx.mark_in_progress() || budget > 0);
    }
    (ctx.collection_stats(), ctx.live_bytes())
}

/// The collections of `compiler.md` §346 rule 2 with no cycle: the program
/// of [`polled_program`] in mode 0, with an explicit collection each time
/// the requested bytes exceed the limit. Returns the count of collections
/// and the live bytes.
fn limit_driven_program() -> (u64, usize) {
    let mut ctx = ship();
    let head = chain(&mut ctx, 2_000);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::Off, 32 << 10));
    let mut collections = 0;
    for _ in 0..40_000 {
        let _ = object(&mut ctx);
        if ctx.requested_bytes() > ctx.collection_limit() {
            ctx.collect();
            collections += 1;
        }
    }
    (collections, ctx.live_bytes())
}

/// Acceptance 3 and rule 16: a budget of 0 gives the collections of
/// `compiler.md` §346 rule 2, each in one pause. The default budget is
/// 50,000 words.
#[test]
fn a_budget_of_zero_gives_the_collections_of_one_pause() {
    assert_eq!(DEFAULT_COLLECTION_STEP_WORDS, 50_000);
    assert_eq!(Context::new_releasing().collection_step_budget(), 50_000);
    let (zero, zero_live) = polled_program(0);
    let (limit_driven, limit_driven_live) = limit_driven_program();
    assert_eq!(zero.automatic_collections, limit_driven);
    assert!(zero.automatic_collections > 10);
    assert_eq!(zero.mark_steps, 0);
    assert_eq!(zero.forced_ends, 0);
    assert_eq!(zero.max_pause_words_read, zero.max_words_read);
    assert_eq!(zero_live, limit_driven_live);
    let (steps, _) = polled_program(500);
    assert!(steps.mark_steps > 0);
    assert!(steps.automatic_collections > 0);
}

/// Rules 12 and 21: the flag is at offset 29, beside the pending flag, and
/// it is set only during a cycle of a ship Context.
#[test]
fn the_flag_is_in_the_prefix_and_is_set_only_during_a_cycle() {
    assert_eq!(Context::mark_in_progress_offset(), 29);
    assert_eq!(
        Context::mark_in_progress_offset(),
        Context::collection_pending_offset() + 1
    );
    let mut ctx = ship();
    let _roots = Roots::new(&mut ctx, &[0]);
    assert!(!ctx.mark_in_progress());
    assert_eq!(ctx.collect_step(10), 1);
    assert!(ctx.mark_in_progress());
    // SAFETY: the offset is inside the fixed prefix of the Context.
    let byte = unsafe { (&*ctx as *const Context as *const u8).add(29).read() };
    assert_eq!(byte, 1);
    finish_cycle(&mut ctx, 10);
    assert!(!ctx.mark_in_progress());
}

/// `set_collection_step` and `collection_step_budget`: the call keeps the
/// mode and the headroom, refuses at a script depth above zero, and a
/// worker takes the settings that carry the budget.
#[test]
fn set_collection_step_sets_the_budget_at_depth_zero_only() {
    for mut ctx in [Context::new(), Context::new_releasing()] {
        assert_eq!(ctx.collection_step_budget(), DEFAULT_COLLECTION_STEP_WORDS);
        assert!(ctx.set_collection(CollectionMode::Automatic, 4096));
        assert!(ctx.set_collection_step(700));
        assert_eq!(ctx.collection_step_budget(), 700);
        assert_eq!(ctx.collection_min_headroom(), 4096);
        assert_eq!(ctx.collection_settings().step_budget, 700);
        assert!(ctx.set_collection(CollectionMode::Automatic, 8192));
        assert_eq!(ctx.collection_step_budget(), 700, "set_collection keeps it");
        ctx.enter_script();
        assert!(!ctx.set_collection_step(5));
        ctx.exit_script();
        assert_eq!(ctx.collection_step_budget(), 700);
    }
}

/// The C entry of rule 16.
#[test]
fn the_c_entry_sets_the_budget() {
    let mut ctx = Context::new_releasing();
    // SAFETY: `ctx` is a live Context with no script call.
    unsafe {
        assert_eq!(subscript_rt_ctx_set_collection_step(&mut *ctx, 123), 0);
    }
    assert_eq!(ctx.collection_step_budget(), 123);
    ctx.enter_script();
    // SAFETY: as above; the depth makes the call refuse.
    unsafe {
        assert_eq!(subscript_rt_ctx_set_collection_step(&mut *ctx, 1), -1);
    }
    ctx.exit_script();
    assert_eq!(ctx.collection_step_budget(), 123);
}

/// Rule 18: the host-timed step starts, steps, and ends a cycle, and
/// refuses a budget of 0, a script depth above zero, and modes 2 and 3.
#[test]
fn collect_step_starts_steps_and_ends_a_cycle() {
    let mut ctx = ship();
    let head = chain(&mut ctx, 100);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    assert_eq!(ctx.collect_step(0), -1);
    ctx.enter_script();
    assert_eq!(ctx.collect_step(10), -1);
    ctx.exit_script();
    assert!(!ctx.mark_in_progress(), "a refused call does no work");
    assert_eq!(ctx.collect_step(10), 1, "the start");
    let mut steps = 0;
    while ctx.collect_step(10) == 1 {
        steps += 1;
    }
    assert!(steps > 5);
    let stats = ctx.collection_stats();
    assert_eq!(stats.automatic_collections, 1);
    // compiler.md §357 rule 12: the end returns 1, and each call after it
    // is a sweep step until the call that completes the sweep.
    assert!(stats.sweep_steps > 1, "{stats:?}");
    assert_eq!((stats.mark_steps + stats.sweep_steps) as usize, steps);
    assert!(ctx.is_live(head));
    assert_checks_pass(&ctx, 1);
    for mode in [CollectionMode::EachPoll, CollectionMode::StepStress] {
        assert!(ctx.set_collection(mode, 0));
        assert_eq!(ctx.collect_step(10), -1);
    }
}

/// The C entry of rule 18.
#[test]
fn the_c_entry_runs_one_pause() {
    let mut ctx = Context::new_releasing();
    let _roots = Roots::new(&mut ctx, &[0]);
    // SAFETY: `ctx` is a live Context with no script call.
    unsafe {
        assert_eq!(subscript_rt_ctx_collect_step(&mut *ctx, 0), -1);
        assert_eq!(subscript_rt_ctx_collect_step(&mut *ctx, 8), 1, "the start");
        assert_eq!(subscript_rt_ctx_collect_step(&mut *ctx, 8), 1, "one step");
        assert_eq!(subscript_rt_ctx_collect_step(&mut *ctx, 8), 0, "the end");
    }
    assert_eq!(ctx.collection_stats().automatic_collections, 1);
}

/// Rule 21: a dev Context keeps the budget with no effect, never sets the
/// flag, and the host-timed step runs one whole collection and returns 0.
#[test]
fn a_dev_context_has_no_step_form() {
    let mut ctx = Context::new();
    let head = chain(&mut ctx, 100);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::Automatic, 4096));
    assert!(ctx.set_collection_step(10));
    for _ in 0..5_000 {
        let _ = object(&mut ctx);
        ctx.collect_poll();
        assert!(!ctx.mark_in_progress());
    }
    let stats = ctx.collection_stats();
    assert!(stats.automatic_collections > 0);
    assert_eq!(stats.mark_steps, 0);
    assert_eq!(stats.max_pause_words_read, stats.max_words_read);
    let before = stats.automatic_collections;
    assert_eq!(ctx.collect_step(10), 0);
    assert!(!ctx.mark_in_progress());
    assert_eq!(ctx.collection_stats().automatic_collections, before + 1);
    assert!(ctx.is_live(head));
    // Mode 3 with no step form collects at each poll.
    assert!(ctx.set_collection(CollectionMode::StepStress, 0));
    assert!(ctx.collection_pending());
    ctx.collect_poll();
    assert_eq!(ctx.collection_stats().automatic_collections, before + 2);
    assert!(ctx.collection_pending());
}

/// Rule 19: in mode 3 each poll runs one pause, a new cycle starts when
/// one ends, and each end checks the mark.
#[test]
fn mode_three_runs_one_pause_at_each_poll() {
    assert_eq!(
        CollectionMode::from_raw(3),
        Some(CollectionMode::StepStress)
    );
    assert_eq!(CollectionMode::from_raw(4), None);
    let mut ctx = ship();
    let head = chain(&mut ctx, 300);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::StepStress, 0));
    assert!(ctx.set_collection_step(16));
    for _ in 0..2_000 {
        let _ = object(&mut ctx);
        assert!(ctx.collection_pending());
        ctx.collect_poll();
    }
    let stats = ctx.collection_stats();
    assert!(stats.automatic_collections >= 3, "{stats:?}");
    assert!(stats.mark_steps > stats.automatic_collections);
    // Each cycle has at most the limit of steps; the last cycle can be in
    // progress.
    let limit = super::mark_cycle::STRESS_STEP_LIMIT as u64;
    assert!(
        stats.mark_steps <= limit * (stats.automatic_collections + 1),
        "{stats:?}"
    );
    assert!(ctx.is_live(head));
    assert_checks_pass(&ctx, stats.automatic_collections as usize);
}

/// Rule 19: a cycle of mode 3 ends after at most the limit of steps, also
/// when each store adds to the work list more words than a step reads. The
/// end after the limit is not a forced end of rule 7.
#[test]
fn mode_three_ends_a_cycle_whose_work_list_grows() {
    let mut ctx = ship();
    let head = chain(&mut ctx, 300);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::StepStress, 0));
    assert!(ctx.set_collection_step(4));
    let polls = 1_000u64;
    for _ in 0..polls {
        // Twenty words for a step of four.
        for _ in 0..20 {
            ctx.mark_store(head);
        }
        ctx.collect_poll();
    }
    let stats = ctx.collection_stats();
    // A cycle is its start, at most the limit of steps, and its end: 10
    // pauses, so 1,000 polls end about 100 cycles. With no limit no cycle
    // ends.
    assert!(stats.automatic_collections >= polls / 20, "{stats:?}");
    assert_eq!(stats.forced_ends, 0, "{stats:?}");
    assert!(ctx.is_live(head));
    assert_checks_pass(&ctx, stats.automatic_collections as usize);
}

/// Rule 10: the barrier gives a word to the work list only during a mark.
#[test]
fn mark_store_gives_the_word_only_during_a_mark() {
    let mut ctx = ship();
    let a = object(&mut ctx);
    let b = object(&mut ctx);
    let _roots = Roots::new(&mut ctx, &[a]);
    ctx.mark_store(b);
    assert_eq!(ctx.mark_work_words(), 0, "no mark: no work");
    assert_eq!(ctx.collect_step(1), 1);
    let before = ctx.mark_work_words();
    ctx.mark_store(0);
    assert_eq!(ctx.mark_work_words(), before, "a null word is not given");
    // SAFETY: `ctx` is a live Context.
    unsafe { subscript_rt_mark_store(&mut *ctx, b) };
    assert_eq!(ctx.mark_work_words(), before + 1);
    finish_cycle(&mut ctx, 1);
    assert!(ctx.is_live(b), "the word in the work list keeps its block");
    assert_checks_pass(&ctx, 1);
}

/// Rule 14: a block given again loses its mark and goes to the work list,
/// so the next step reads its payload again.
#[test]
fn mark_block_again_reads_the_payload_again() {
    let mut ctx = ship();
    let a = object(&mut ctx);
    let b = object(&mut ctx);
    let _roots = Roots::new(&mut ctx, &[a]);
    ctx.mark_block_again(a);
    assert!(!ctx.collection_marked(a), "no mark: no work");
    assert_eq!(ctx.collect_step(1), 1);
    assert_eq!(ctx.collect_step(1), 1);
    assert!(ctx.collection_marked(a));
    store(a, 0, b);
    // SAFETY: `ctx` is a live Context and `a` is its payload.
    unsafe { subscript_rt_mark_block(&mut *ctx, a as *mut u8) };
    assert!(!ctx.collection_marked(a));
    finish_cycle(&mut ctx, 1);
    assert!(ctx.is_live(b));
    assert_checks_pass(&ctx, 1);
}

/// The environment variable that makes
/// [`mode_three_stops_the_process_when_the_check_fails`] run its child side.
const MODE_THREE_CHILD: &str = "SUBSCRIPT_MARK_MODE_THREE_CHILD";

/// The child side: the shape of acceptance 2 in mode 3. The end of the
/// cycle stops the process, so a return is a failure.
fn mode_three_child() {
    let mut ctx = Context::new_releasing();
    let a = object(&mut ctx);
    let b = object(&mut ctx);
    let c = object(&mut ctx);
    store(c, 0, b);
    let _roots = Roots::new(&mut ctx, &[c, a]);
    assert!(ctx.set_collection(CollectionMode::StepStress, 0));
    assert!(ctx.set_collection_step(1));
    for _ in 0..1_000 {
        if ctx.collection_marked(a) {
            break;
        }
        ctx.collect_poll();
    }
    assert!(ctx.collection_marked(a) && !ctx.collection_marked(b));
    store(a, 0, b);
    store(c, 0, 0);
    for _ in 0..1_000 {
        ctx.collect_poll();
    }
    panic!("mode 3 did not stop the process");
}

/// Rules 19, 20, and 20a: in mode 3 a failed check stops the process with
/// the message. The store with no barrier fails the check of the pause
/// after it, before the end. The test runs its child side in a child process of
/// the test binary. Cost: one process start and five Context pauses.
#[test]
fn mode_three_stops_the_process_when_the_check_fails() {
    if std::env::var_os(MODE_THREE_CHILD).is_some() {
        mode_three_child();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "context::mark_cycle_tests::mode_three_stops_the_process_when_the_check_fails",
            "--exact",
            "--test-threads",
            "1",
            "--nocapture",
        ])
        .env(MODE_THREE_CHILD, "1")
        .output()
        .expect("run the child");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains("compiler.md §356 rule 20a: 1 words"),
        "{stderr}"
    );
    assert!(!stderr.contains("did not stop"), "{stderr}");
}

/// A Context that is dropped during a cycle frees each chunk and each
/// large allocation. Control: the Context holds both kinds before the
/// drop.
#[test]
fn a_context_dropped_during_a_cycle_frees_each_allocation() {
    let mut ctx = ship();
    let head = chain(&mut ctx, 10_000);
    let array = references(&mut ctx, 10_000);
    let _roots = Roots::new(&mut ctx, &[head, array]);
    assert_eq!(ctx.collect_step(100), 1, "the start");
    assert_eq!(ctx.collect_step(100), 1, "one step");
    assert!(ctx.mark_in_progress());
    let stats = ctx.test_arena_stats();
    let (chunks, large) = stats.owned_resources();
    assert!(chunks > 1 && large > 0, "{chunks} {large}");
    drop(ctx);
    assert_eq!(stats.owned_resources(), (0, 0));
}

/// Rule 16: a budget of 0 during a cycle ends the cycle at the next poll,
/// and the end is one automatic collection. Control: a budget above 0 set
/// again during the cycle makes the next poll run one step.
#[test]
fn a_budget_of_zero_during_a_cycle_ends_it_at_the_next_poll() {
    for zero in [false, true] {
        let mut ctx = ship();
        let head = chain(&mut ctx, 2_000);
        let _roots = Roots::new(&mut ctx, &[head]);
        assert!(ctx.set_collection(CollectionMode::Automatic, 1 << 20));
        assert_eq!(ctx.collect_step(10), 1, "the start");
        assert_eq!(ctx.collect_step(10), 1, "one step");
        let steps = ctx.collection_stats().mark_steps;
        assert!(ctx.set_collection_step(if zero { 0 } else { 10 }));
        assert!(ctx.collection_pending(), "the poll serves the cycle");
        ctx.collect_poll();
        let stats = ctx.collection_stats();
        assert_eq!(ctx.mark_in_progress(), !zero);
        assert_eq!(stats.automatic_collections, u64::from(zero));
        assert_eq!(stats.mark_steps, steps + u64::from(!zero));
        assert!(ctx.is_live(head));
        if zero {
            assert_checks_pass(&ctx, 1);
        }
    }
}

/// Rules 16 and 18 in mode 0: a change to mode 0 during a cycle keeps the
/// cycle. No poll serves it. The host ends it with the host-timed step or
/// with an explicit collection. Rule 10: 1,000 barrier stores of one
/// reference add at most one word to the work list.
#[test]
fn mode_zero_keeps_a_cycle_until_the_host_ends_it() {
    for explicit in [false, true] {
        let mut ctx = ship();
        let head = chain(&mut ctx, 2_000);
        let _roots = Roots::new(&mut ctx, &[head]);
        assert!(ctx.set_collection(CollectionMode::Automatic, 1 << 20));
        assert_eq!(ctx.collect_step(10), 1, "the start");
        assert!(ctx.set_collection(CollectionMode::Off, 0));
        assert!(ctx.mark_in_progress());
        assert!(!ctx.collection_pending());
        ctx.collect_poll();
        assert!(ctx.mark_in_progress(), "no poll serves mode 0");
        let before = ctx.mark_work_words();
        for _ in 0..1_000 {
            ctx.mark_store(head);
        }
        assert!(
            ctx.mark_work_words() <= before + 1,
            "the block is in the list once"
        );
        if explicit {
            ctx.collect();
        } else {
            finish_cycle(&mut ctx, 10);
        }
        assert!(!ctx.mark_in_progress());
        assert!(ctx.is_live(head));
        assert_checks_pass(&ctx, 1);
        let stats = ctx.collection_stats();
        assert_eq!(stats.automatic_collections, 1);
        assert_eq!(stats.explicit_collections, u64::from(explicit));
    }
}

/// Rule 10: a store of a block that has the mark gives nothing to the
/// work list. Control: the first store of a block with no mark gives one
/// word, and a second store of it gives nothing.
#[test]
fn a_store_of_a_marked_block_adds_nothing() {
    let mut ctx = ship();
    let a = object(&mut ctx);
    let b = object(&mut ctx);
    let _roots = Roots::new(&mut ctx, &[a]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    assert_eq!(ctx.collect_step(1), 1, "the start");
    assert_eq!(ctx.collect_step(1), 1, "one step marks A");
    assert!(ctx.collection_marked(a));
    let before = ctx.mark_work_words();
    ctx.mark_store(a);
    assert_eq!(ctx.mark_work_words(), before, "A has the mark");
    store(a, 0, b);
    ctx.mark_store(b);
    assert_eq!(ctx.mark_work_words(), before + 1, "B has no mark");
    ctx.mark_store(b);
    assert_eq!(ctx.mark_work_words(), before + 1, "B is in the list");
    finish_cycle(&mut ctx, 1);
    assert!(ctx.is_live(b));
    assert_checks_pass(&ctx, 1);
}

/// Rule 20a: a block with the state "in the work list" is in the work
/// list, also when its word is not. The test removes the word of B and
/// keeps the state. Control: the same heap with no barrier, so B has the
/// live state, gives one finding.
#[test]
fn the_check_of_a_pause_treats_a_block_in_the_list_as_listed() {
    for barrier in [false, true] {
        let mut ctx = ship();
        let a = object(&mut ctx);
        let b = object(&mut ctx);
        let c = object(&mut ctx);
        store(c, 0, b);
        let _roots = Roots::new(&mut ctx, &[c, a]);
        assert!(ctx.set_collection(CollectionMode::Off, 0));
        assert_eq!(ctx.collect_step(1), 1, "the start");
        assert_eq!(ctx.collect_step(1), 1, "one step marks A");
        store(a, 1, b);
        if barrier {
            ctx.mark_store(b);
        }
        ctx.remove_mark_work_word(b);
        let pause = ctx.mark_pause_check();
        assert_eq!(pause.count, u64::from(!barrier), "{}", pause.message());
        finish_cycle(&mut ctx, 1);
        assert!(ctx.is_live(b));
        assert_checks_pass(&ctx, 1);
    }
}

/// Rules 9 and 10: `Context.free` of a block in the work list, then a
/// step, is safe. The released block is not marked, and a block that takes
/// its place is a block of rule 4.
#[test]
fn context_free_of_a_block_in_the_list_then_a_step_is_safe() {
    let mut ctx = ship();
    let a = object(&mut ctx);
    let b = object(&mut ctx);
    let mut roots = Roots::new(&mut ctx, &[a, 0]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    assert_eq!(ctx.collect_step(1), 1, "the start");
    assert_eq!(ctx.collect_step(1), 1, "one step marks A");
    store(a, 0, b);
    ctx.mark_store(b);
    assert!(ctx.is_live(b), "a block in the list is live");
    store(a, 0, 0);
    ctx.delete(b, 0);
    assert!(!ctx.is_live(b));
    assert_eq!(ctx.collect_step(1), 1, "a step pops the released block");
    let c = object(&mut ctx);
    assert_eq!(c, b, "the block takes the place of the released block");
    assert!(
        ctx.collection_marked(c),
        "an allocation during a mark has the mark"
    );
    roots.set(1, c);
    finish_cycle(&mut ctx, 1);
    assert!(ctx.is_live(a) && ctx.is_live(c));
    assert_checks_pass(&ctx, 1);
}

/// Rule 6b: in mode 1 the mark work follows the stores. A program that
/// changes references among 2,000 objects and does not allocate ends its
/// cycle, at polls and at outermost exits. Each outermost exit runs at
/// most one pause of the cycle, not a whole collection.
#[test]
fn a_cycle_ends_when_the_program_stores_and_does_not_allocate() {
    for exits in [false, true] {
        let mut ctx = ship();
        let head = chain(&mut ctx, 5_000);
        let holder = object(&mut ctx);
        let pool: Vec<usize> = (0..2_000).map(|_| object(&mut ctx)).collect();
        let _roots = Roots::new(&mut ctx, &[head, holder]);
        assert!(ctx.set_collection(CollectionMode::Automatic, 64 << 20));
        assert!(ctx.set_collection_step(1_000));
        ctx.reset_collection_stats();
        assert_eq!(ctx.collect_step(1_000), 1, "the start");
        assert!(!ctx.collection_pending(), "no allocation sets the flag");
        let allocations = ctx.successful_allocations();
        let mut stores = 0;
        for &value in pool.iter().cycle().take(1_000_000) {
            if exits {
                ctx.enter_script();
            }
            store(holder, 0, value);
            ctx.mark_store(value);
            stores += 1;
            let before = ctx.collection_stats();
            if exits {
                ctx.exit_script();
            } else {
                ctx.collect_poll();
            }
            let after = ctx.collection_stats();
            let pauses = (after.mark_steps - before.mark_steps)
                + (after.automatic_collections - before.automatic_collections);
            assert!(pauses <= 1, "one pause at most");
            if !ctx.mark_in_progress() {
                break;
            }
        }
        assert_eq!(ctx.successful_allocations(), allocations);
        assert!(
            !ctx.mark_in_progress(),
            "the cycle ends after {stores} stores"
        );
        let stats = ctx.collection_stats();
        assert_eq!(stats.automatic_collections, 1);
        assert!(stats.mark_steps > 0);
        assert!(ctx.is_live(head) && ctx.is_live(load(holder, 0)));
        assert_checks_pass(&ctx, 1);
    }
}

/// Rule 5: before the mark reaches a block H, the program moves a chain of
/// 2,000 objects from a field of H to a root and clears the field. Only the
/// end finds the chain, and its mark has the budget: no pause reads more
/// than the budget, the words of one small block, and the two root words.
/// The end takes more than one pause, the cycle ends, and the chain is
/// live.
#[test]
fn an_end_that_finds_a_graph_at_a_root_marks_with_the_budget() {
    const BUDGET: usize = 16;
    const ROOT_WORDS: usize = 2;
    let mut ctx = ship();
    let head = chain(&mut ctx, 2_000);
    let holder = object(&mut ctx);
    store(holder, 0, head);
    let mut roots = Roots::new(&mut ctx, &[holder, 0]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    ctx.reset_collection_stats();
    assert_eq!(ctx.collect_step(BUDGET), 1, "the start");
    assert!(!ctx.collection_marked(holder));
    roots.set(1, head);
    store(holder, 0, 0);
    assert_eq!(ctx.collect_step(BUDGET), 1, "one step marks H");
    assert_eq!(ctx.mark_work_words(), 0, "the next pause is the end");
    let steps = ctx.collection_stats().mark_steps;
    finish_cycle(&mut ctx, BUDGET);
    let stats = ctx.collection_stats();
    assert_eq!(stats.automatic_collections, 1);
    // A small object of the test is a payload of two words.
    assert!(
        stats.max_pause_words_read <= (BUDGET + 2 + ROOT_WORDS) as u64,
        "{stats:?}"
    );
    assert!(stats.mark_steps - steps > 100, "{stats:?}");
    let mut block = head;
    let mut count = 0;
    while block != 0 {
        assert!(ctx.is_live(block));
        block = load(block, 0);
        count += 1;
    }
    assert_eq!(count, 2_000);
    assert_checks_pass(&ctx, 1);
}

/// Rule 5 and rule 3: a growth of a large array after the step that left
/// the work list empty gives the end a new storage of 8,192 words. The end
/// reads it in parts: no pause reads more than the budget, the words of one
/// small block, and the root word.
#[test]
fn a_growth_before_the_end_does_not_make_one_pause_read_the_new_storage() {
    const BUDGET: usize = 256;
    let mut ctx = ship();
    let array = references(&mut ctx, 4_096);
    let _roots = Roots::new(&mut ctx, &[array]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    ctx.reset_collection_stats();
    assert_eq!(ctx.collect_step(BUDGET), 1, "the start");
    while ctx.mark_work_words() > 0 {
        assert_eq!(ctx.collect_step(BUDGET), 1);
    }
    // SAFETY: the array is live.
    let storage = unsafe { (*(array as *const ArrayHeader)).data } as usize;
    let element = object(&mut ctx);
    // SAFETY: the array is live, and the value is one readable word.
    assert!(unsafe { ctx.array_push(array as *mut u8, (&element as *const usize).cast(), 0) } > 0);
    // SAFETY: as above.
    let grown = unsafe { (*(array as *const ArrayHeader)).data } as usize;
    assert_ne!(grown, storage, "the push grew the storage");
    assert!(
        ctx.mark_work_words() > 0,
        "the new storage is in the work list"
    );
    let steps = ctx.collection_stats().mark_steps;
    finish_cycle(&mut ctx, BUDGET);
    let stats = ctx.collection_stats();
    assert_eq!(stats.automatic_collections, 1);
    assert!(
        stats.max_pause_words_read <= (BUDGET + 2 + 1) as u64,
        "{stats:?}"
    );
    assert!(
        stats.mark_steps - steps >= 8_192 / BUDGET as u64,
        "{stats:?}"
    );
    assert!(ctx.is_live(element));
    assert_checks_pass(&ctx, 1);
}

/// The environment variable that makes
/// [`a_context_with_a_mark_trace_has_no_step_form`] run its child side.
const MARK_TRACE_CHILD: &str = "SUBSCRIPT_MARK_TRACE_STEP_CHILD";

/// The child side: a ship Context with a budget runs no step. The
/// host-timed step runs one whole collection and returns 0, and the polls
/// of mode 1 end collections with no step.
fn mark_trace_child() {
    let mut ctx = Context::new_releasing();
    let head = chain(&mut ctx, 100);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::Automatic, 4096));
    assert!(ctx.set_collection_step(10));
    assert_eq!(ctx.collect_step(10), 0, "one whole collection");
    assert!(!ctx.mark_in_progress());
    for _ in 0..5_000 {
        let _ = object(&mut ctx);
        ctx.collect_poll();
        assert!(!ctx.mark_in_progress());
    }
    let stats = ctx.collection_stats();
    assert!(stats.automatic_collections > 1, "{stats:?}");
    assert_eq!(stats.mark_steps, 0, "{stats:?}");
    assert!(ctx.is_live(head));
    println!("the child side ran");
}

/// Rule 20b: a Context with a mark trace target has no step form. The test
/// runs its child side in a child process of the test binary, with a trace
/// target that names no block. Control: the same child with no trace
/// target runs steps, and its first assertion fails. Cost: two process
/// starts.
#[test]
fn a_context_with_a_mark_trace_has_no_step_form() {
    if std::env::var_os(MARK_TRACE_CHILD).is_some() {
        mark_trace_child();
        return;
    }
    for traced in [true, false] {
        let mut command =
            std::process::Command::new(std::env::current_exe().expect("the test binary"));
        command
            .args([
                "context::mark_cycle_tests::a_context_with_a_mark_trace_has_no_step_form",
                "--exact",
                "--test-threads",
                "1",
                "--nocapture",
            ])
            .env(MARK_TRACE_CHILD, "1");
        if traced {
            command.env("SUBSCRIPT_MARK_TRACE", "0x10");
        } else {
            command.env_remove("SUBSCRIPT_MARK_TRACE");
        }
        let output = command.output().expect("run the child");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), traced, "{stdout}{stderr}");
        assert_eq!(stdout.contains("the child side ran"), traced, "{stdout}");
        if !traced {
            assert!(stderr.contains("one whole collection"), "{stderr}");
        }
    }
}

/// The barrier of a range (`compiler.md` §356 rule 10) and of a runtime
/// write (rule 14): a range of three words holds a number, a block B with
/// no mark, and null. With no mark the call gives nothing. During a mark B
/// goes to the work list one time, also for a second call.
#[test]
fn mark_range_and_mark_written_words_give_the_blocks_of_the_range() {
    for c_entry in [false, true] {
        let mut ctx = ship();
        let a = object(&mut ctx);
        let b = object(&mut ctx);
        let range = [7usize, b, 0];
        let give = |ctx: &mut Context| {
            if c_entry {
                // SAFETY: `ctx` is live, and the range is 24 readable bytes.
                unsafe { subscript_rt_mark_range(ctx, range.as_ptr().cast(), 24) };
            } else {
                ctx.mark_written_words(range.as_ptr().cast(), 24);
            }
        };
        let _roots = Roots::new(&mut ctx, &[a]);
        assert!(ctx.set_collection(CollectionMode::Off, 0));
        give(&mut ctx);
        assert_eq!(ctx.mark_work_words(), 0, "no mark: no work");
        assert_eq!(ctx.collect_step(1), 1, "the start");
        let before = ctx.mark_work_words();
        give(&mut ctx);
        assert_eq!(ctx.mark_work_words(), before + 1, "B goes to the list");
        give(&mut ctx);
        assert_eq!(ctx.mark_work_words(), before + 1, "B is in the list once");
        finish_cycle(&mut ctx, 1);
        assert!(ctx.is_live(b), "the word in the work list keeps its block");
        assert_checks_pass(&ctx, 1);
    }
}

/// Rule 7 during the tries of the end: a chain of 2,000 objects that only a
/// root holds makes the end take many pauses (rule 5). The program
/// allocates more than one limit during those pauses, so the cycle becomes
/// late, and the record counts it one time.
#[test]
fn a_cycle_that_becomes_late_during_the_tries_of_the_end_is_counted() {
    const BUDGET: usize = 16;
    let mut ctx = ship();
    let head = chain(&mut ctx, 2_000);
    let holder = object(&mut ctx);
    store(holder, 0, head);
    let mut roots = Roots::new(&mut ctx, &[holder, 0]);
    assert!(ctx.set_collection(CollectionMode::Automatic, 64 << 10));
    assert!(ctx.set_collection_step(BUDGET));
    ctx.reset_collection_stats();
    assert_eq!(ctx.collect_step(BUDGET), 1, "the start");
    roots.set(1, head);
    store(holder, 0, 0);
    assert_eq!(ctx.collect_step(BUDGET), 1, "one step marks H");
    assert_eq!(ctx.mark_work_words(), 0, "the next pause is the end");
    for polls in 0.. {
        assert!(polls < 1_000_000, "the cycle does not end");
        assert!(!ctx.alloc(1024, OBJECT_CLASS, 0).is_null());
        ctx.collect_poll();
        if !ctx.mark_in_progress() {
            break;
        }
    }
    let stats = ctx.collection_stats();
    assert_eq!(stats.automatic_collections, 1, "{stats:?}");
    assert_eq!(stats.forced_ends, 1, "{stats:?}");
    assert!(ctx.is_live(head));
    assert_checks_pass(&ctx, 1);
}
