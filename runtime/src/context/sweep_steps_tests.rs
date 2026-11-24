//! The sweep of a collection in steps (`compiler.md` §357).
//!
//! Cost: the largest heap is the 1,000,000 blocks of acceptance 1: 32 MB of
//! blocks, built two times (sweep steps, and a sweep of one pause), one
//! Context at a time. The heap of the late sweep is 20,000 blocks and
//! 2 MiB of requests. The other tests build heaps under 1 MiB. The search
//! of acceptance 4 reads each source file of the runtime one time.

use super::*;

/// A user class id that can hold a handle.
const OBJECT_CLASS: u32 = 900;

/// The request of one small object: a block of 32 bytes, a payload of two
/// words.
const OBJECT_BYTES: usize = 16;

/// The slots of one chunk of the smallest class.
const SMALLEST_CHUNK_SLOTS: usize = CHUNK_SIZE / SMALLEST_BLOCK;

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

/// A ship Context that keeps the result of the check of rule 15 at the
/// completion of each sweep.
fn ship() -> Box<Context> {
    let mut ctx = Context::new_releasing();
    ctx.record_sweep_checks();
    ctx
}

/// A chain of `count` objects. Word 0 of each object holds the next one.
/// After each object of the chain, `garbage` objects that nothing holds.
fn chain_with_garbage(ctx: &mut Context, count: usize, garbage: usize) -> usize {
    let mut next = 0;
    for _ in 0..count {
        let block = object(ctx);
        store(block, 0, next);
        next = block;
        for _ in 0..garbage {
            let _ = object(ctx);
        }
    }
    next
}

/// Runs host-timed pauses of `budget` words until the end of the cycle
/// starts a sweep.
fn run_to_sweep(ctx: &mut Context, budget: usize) {
    for _ in 0..1_000_000 {
        assert_eq!(ctx.collect_step(budget), 1, "the cycle ends with a sweep");
        if ctx.sweep_in_progress() {
            return;
        }
    }
    panic!("the cycle does not end");
}

/// Runs automatic pauses until the end of the cycle starts a sweep.
fn poll_to_sweep(ctx: &mut Context) {
    for _ in 0..1_000_000 {
        ctx.collect_automatic();
        if ctx.sweep_in_progress() {
            return;
        }
    }
    panic!("the cycle does not end");
}

/// Runs host-timed pauses of `budget` words until the sweep completes.
/// Rule 12: each call returns 1 until the call that completes the sweep,
/// which returns 0.
fn finish_sweep(ctx: &mut Context, budget: usize) {
    for _ in 0..10_000_000 {
        match ctx.collect_step(budget) {
            1 => assert!(ctx.sweep_in_progress() || ctx.mark_in_progress()),
            0 => {
                assert!(!ctx.sweep_in_progress());
                return;
            }
            other => panic!("collect_step returned {other}"),
        }
    }
    panic!("the sweep does not complete");
}

/// The check of rule 15 ran at the completion of each sweep, `sweeps`
/// times, and found nothing.
fn assert_sweep_checks_pass(ctx: &Context, sweeps: usize) {
    let checks = ctx.sweep_checks();
    assert_eq!(checks.len(), sweeps, "a check runs at each completion");
    for check in checks {
        assert_eq!(check.count(), 0, "{}", check.message());
    }
}

/// The record after one cycle of 1,000,000 small blocks that nothing holds,
/// with a sweep in steps of `budget` words, or with one collection of one
/// pause for a budget of 0.
fn million_unreachable(budget: usize) -> CollectionStats {
    let mut ctx = Context::new_releasing();
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    for _ in 0..1_000_000 {
        let _ = object(&mut ctx);
    }
    ctx.reset_collection_stats();
    if budget == 0 {
        assert!(ctx.set_collection_step(0));
        ctx.collect_automatic();
    } else {
        run_to_sweep(&mut ctx, budget);
        assert_eq!(ctx.live_count(), 1_000_000, "rule 3: the end releases none");
        finish_sweep(&mut ctx, budget);
    }
    assert_eq!(ctx.live_count(), 0);
    assert_eq!(ctx.live_bytes(), 0);
    ctx.collection_stats()
}

/// Acceptance 1: with a budget of 50,000 words, no pause sweeps more than
/// 250,000 slots plus the slots of one chunk, and the sweep has more than
/// 3 steps. Control: a sweep of one pause reads each slot in that pause.
#[test]
fn a_million_unreachable_blocks_sweep_in_steps() {
    let steps = million_unreachable(50_000);
    assert_eq!(steps.automatic_collections, 1);
    assert!(steps.sweep_steps > 3, "{steps:?}");
    assert!(
        steps.max_pause_slots_swept <= (250_000 + SMALLEST_CHUNK_SLOTS) as u64,
        "{steps:?}"
    );
    assert_eq!(steps.late_sweeps, 0);
    assert_eq!(steps.last_live_bytes_after, 0);
    let one_pause = million_unreachable(0);
    assert_eq!(one_pause.sweep_steps, 0);
    assert!(
        one_pause.max_pause_slots_swept >= 1_000_000,
        "{one_pause:?}"
    );
}

/// The heap of acceptance 2: a chain of small objects with garbage between
/// them, a reachable and an unreachable large block, and objects of a
/// larger class. Returns the roots.
fn mixed_heap(ctx: &mut Context) -> Roots {
    let head = chain_with_garbage(ctx, 3_000, 2);
    let kept_large = ctx.alloc(64 << 10, OBJECT_CLASS, 0) as usize;
    let _ = ctx.alloc(64 << 10, OBJECT_CLASS, 0);
    let mut wide = 0;
    for index in 0..500 {
        let block = ctx.alloc(200, OBJECT_CLASS, 0) as usize;
        if index % 3 == 0 {
            store(block, 0, wide);
            wide = block;
        }
    }
    store(kept_large, 0, wide);
    Roots::new(ctx, &[head, kept_large])
}

/// Acceptance 2: after a sweep in steps, the count of the live blocks, the
/// live bytes, the record, and the limit equal those after a collection
/// with a budget of 0.
#[test]
fn a_sweep_in_steps_releases_the_blocks_of_a_sweep_of_one_pause() {
    let mut steps = ship();
    let _steps_roots = mixed_heap(&mut steps);
    assert!(steps.set_collection(CollectionMode::Off, 1 << 20));
    run_to_sweep(&mut steps, 100);
    assert!(steps.live_count() > 3_000 * 3);
    finish_sweep(&mut steps, 100);
    assert_sweep_checks_pass(&steps, 1);
    let stats = steps.collection_stats();
    assert!(stats.sweep_steps > 10, "{stats:?}");
    assert!(stats.max_pause_slots_swept <= 500, "{stats:?}");

    let mut one = Context::new_releasing();
    let _one_roots = mixed_heap(&mut one);
    assert!(one.set_collection(CollectionMode::Off, 1 << 20));
    assert!(one.set_collection_step(0));
    one.collect_automatic();
    assert!(!one.sweep_in_progress());

    assert_eq!(steps.live_count(), one.live_count());
    assert_eq!(steps.live_bytes(), one.live_bytes());
    assert_eq!(steps.live_bytes(), steps.live_bytes_by_walk());
    assert_eq!(
        stats.last_live_bytes_after,
        one.collection_stats().last_live_bytes_after
    );
    assert_eq!(steps.collection_limit(), one.collection_limit());
    assert_eq!(steps.live_count(), 3_000 + 1 + 167);
}

/// Acceptance 3 and rule 7: an allocation during a sweep stays live, in a
/// free slot of a part that the sweep did not reach and in a bump slot.
#[test]
fn an_allocation_during_a_sweep_stays_live() {
    let mut ctx = ship();
    let mut blocks = Vec::new();
    for _ in 0..100 {
        blocks.push(object(&mut ctx));
    }
    let mut roots = Roots::new(&mut ctx, &[blocks[0], 0, 0]);
    // The slot of block 50 is on the free list before the cycle.
    ctx.delete(blocks[50], 0);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    run_to_sweep(&mut ctx, 1);
    // The sweep did not reach slot 50, which is the only free slot.
    assert_eq!(ctx.sweep_position(), (0, 0));
    let free = object(&mut ctx);
    assert_eq!(free, blocks[50], "the allocation takes the free slot");
    let bump = object(&mut ctx);
    assert_eq!(
        ctx.arena_lookup_slot(bump),
        Some((0, 100)),
        "the allocation takes a bump slot above the limit"
    );
    roots.set(1, free);
    roots.set(2, bump);
    // A block that nothing holds, allocated during the sweep, is live by
    // rule 1 until the next cycle.
    let unheld = object(&mut ctx);
    finish_sweep(&mut ctx, 1);
    assert_sweep_checks_pass(&ctx, 1);
    assert!(ctx.is_live(free) && ctx.is_live(bump) && ctx.is_live(unheld));
    assert!(ctx.is_live(blocks[0]));
    assert!(!ctx.is_live(blocks[1]), "the sweep released the garbage");
    assert_eq!(ctx.live_count(), 4);
}

/// The bytes of the payload of one small object.
const OBJECT_PAYLOAD: usize = SMALLEST_BLOCK - HEADER_SIZE;

/// Acceptance 3 and rule 8: `Context.free` during a sweep, of a live block
/// and of a block to release that the sweep did not reach. The limit and
/// the record after the sweep are those of a sweep of one pause: the live
/// block counts, because it was live at the end.
#[test]
fn context_free_during_a_sweep() {
    let mut ctx = ship();
    let kept = object(&mut ctx);
    let freed_live = object(&mut ctx);
    let mut garbage = Vec::new();
    for _ in 0..40 {
        garbage.push(object(&mut ctx));
    }
    let mut roots = Roots::new(&mut ctx, &[kept, freed_live]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    run_to_sweep(&mut ctx, 1);
    let at_end = ctx.live_bytes();
    assert_eq!(at_end, 42 * OBJECT_PAYLOAD);
    ctx.delete(freed_live, 0);
    roots.set(1, 0);
    ctx.delete(garbage[30], 0);
    assert_eq!(ctx.live_bytes(), 40 * OBJECT_PAYLOAD);
    finish_sweep(&mut ctx, 1);
    assert_sweep_checks_pass(&ctx, 1);
    assert_eq!(ctx.live_bytes(), OBJECT_PAYLOAD);
    assert_eq!(ctx.live_bytes(), ctx.live_bytes_by_walk());
    let stats = ctx.collection_stats();
    assert_eq!(stats.last_live_bytes_after, (2 * OBJECT_PAYLOAD) as u64);
    assert_eq!(ctx.collection_limit(), DEFAULT_MIN_HEADROOM_BYTES);
    assert!(ctx.is_live(kept) && !ctx.is_live(freed_live));
    // A double free of the block to release is a no-op (Q6).
    ctx.delete(garbage[30], 0);
    assert_eq!(ctx.live_bytes(), OBJECT_PAYLOAD);
}

/// The live bytes after an explicit collection that a heap with garbage
/// allocated after the start of a sweep gets, or with a budget of 0.
fn live_after_explicit(sweep: bool) -> (usize, usize) {
    let mut ctx = ship();
    let head = chain_with_garbage(&mut ctx, 200, 3);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    if sweep {
        run_to_sweep(&mut ctx, 1);
        assert_eq!(ctx.collect_step(1), 1, "one sweep step");
    } else {
        assert!(ctx.set_collection_step(0));
        ctx.collect_automatic();
    }
    for _ in 0..300 {
        let _ = object(&mut ctx);
    }
    // Rule 10: the explicit collection completes the sweep in its own
    // pause. The pause sweeps the rest of the sweep and then each slot of
    // the arena, and it is not a sweep step.
    let left = ctx.sweep_slots_left();
    let slots: usize = ctx.chunks.iter().map(|chunk| chunk.bump).sum();
    assert_eq!(left > 0, sweep);
    ctx.reset_collection_stats();
    ctx.collect();
    let stats = ctx.collection_stats();
    assert_eq!(stats.sweep_steps, 0, "{stats:?}");
    assert_eq!(stats.automatic_collections, 0);
    assert_eq!(
        stats.max_pause_slots_swept,
        (left + slots) as u64,
        "{stats:?}"
    );
    assert!(!ctx.sweep_in_progress());
    assert_eq!(stats.explicit_collections, 1);
    assert_eq!(ctx.sweep_checks().len(), 2, "two sweeps completed");
    for check in ctx.sweep_checks() {
        assert_eq!(check.count(), 0, "{}", check.message());
    }
    (ctx.live_bytes(), ctx.live_count())
}

/// Acceptance 3 and rule 10: an explicit collection during a sweep
/// completes the sweep, and gives the live bytes of a Context with a budget
/// of 0.
#[test]
fn an_explicit_collection_during_a_sweep_gives_the_live_bytes_of_one_pause() {
    let during = live_after_explicit(true);
    assert_eq!(during, live_after_explicit(false));
    assert_eq!(during, (200 * OBJECT_PAYLOAD, 200));
}

/// Acceptance 3 and rule 9: the requested bytes reach the start of the next
/// cycle before the sweep ends. From there each poll runs one sweep step
/// with the budget, no cycle starts during the sweep, and the cycle starts
/// at the first poll after it. The record counts the late sweep one time.
#[test]
fn a_late_sweep_runs_a_step_at_each_poll_and_the_cycle_starts_after_it() {
    const BUDGET: usize = 100;
    let mut ctx = ship();
    for _ in 0..20_000 {
        let _ = object(&mut ctx);
    }
    let _roots = Roots::new(&mut ctx, &[0]);
    assert!(ctx.set_collection(CollectionMode::Automatic, 1 << 20));
    assert!(ctx.set_collection_step(BUDGET));
    ctx.reset_collection_stats();
    poll_to_sweep(&mut ctx);
    assert!(!ctx.collection_pending());
    // 2 MiB of requests with no poll pass the start of the next cycle.
    for _ in 0..(2 << 20) / OBJECT_BYTES {
        let _ = object(&mut ctx);
    }
    assert!(ctx.collection_pending());
    let mut polls = 0;
    while ctx.sweep_in_progress() {
        assert!(ctx.collection_pending(), "the flag stays set");
        ctx.collect_poll();
        polls += 1;
        assert!(!ctx.mark_in_progress(), "no cycle starts during a sweep");
    }
    let stats = ctx.collection_stats();
    assert_eq!(stats.late_sweeps, 1, "{stats:?}");
    assert_eq!(stats.sweep_steps, polls);
    assert!(polls > 10, "{polls}");
    assert!(
        stats.max_pause_slots_swept <= (5 * BUDGET) as u64,
        "{stats:?}"
    );
    assert_eq!(stats.automatic_collections, 1);
    assert!(ctx.collection_pending(), "the next cycle is due");
    ctx.collect_poll();
    assert!(ctx.mark_in_progress(), "the cycle starts after the sweep");
    assert_sweep_checks_pass(&ctx, 1);
}

/// Rule 6: in mode 1 the sweep steps follow the allocation, and the sweep
/// ends before the start of the next cycle. Control of the late sweep.
#[test]
fn a_sweep_in_mode_one_ends_before_the_next_cycle() {
    const BUDGET: usize = 100;
    let mut ctx = ship();
    for _ in 0..20_000 {
        let _ = object(&mut ctx);
    }
    let _roots = Roots::new(&mut ctx, &[0]);
    assert!(ctx.set_collection(CollectionMode::Automatic, 1 << 20));
    assert!(ctx.set_collection_step(BUDGET));
    ctx.reset_collection_stats();
    poll_to_sweep(&mut ctx);
    let mut allocations = 0usize;
    while ctx.sweep_in_progress() {
        let _ = object(&mut ctx);
        allocations += 1;
        ctx.collect_poll();
        assert!(!ctx.mark_in_progress());
    }
    let stats = ctx.collection_stats();
    assert_eq!(stats.late_sweeps, 0, "{stats:?}");
    assert!(stats.sweep_steps > 10, "{stats:?}");
    assert!(allocations * OBJECT_BYTES < ctx.collection_limit() - ctx.collection_limit() / 8);
    assert!(!ctx.collection_pending());
    assert_sweep_checks_pass(&ctx, 1);
}

/// Acceptance 3: a Context and a worker Context dropped during a sweep
/// free each chunk and each large allocation. Control: each holds both
/// kinds before the drop.
#[test]
fn a_context_and_a_worker_context_dropped_during_a_sweep_free_each_allocation() {
    for worker in [false, true] {
        let mut ctx = if worker {
            Context::new_worker(true)
        } else {
            Context::new_releasing()
        };
        let head = chain_with_garbage(&mut ctx, 2_000, 2);
        let large = ctx.alloc(64 << 10, OBJECT_CLASS, 0) as usize;
        let _roots = Roots::new(&mut ctx, &[head, large]);
        assert!(ctx.set_collection(CollectionMode::Off, 0));
        run_to_sweep(&mut ctx, 10);
        assert_eq!(ctx.collect_step(10), 1, "one sweep step");
        assert!(ctx.sweep_in_progress());
        let stats = ctx.test_arena_stats();
        let (chunks, larges) = stats.owned_resources();
        assert!(chunks > 1 && larges == 1, "{chunks} {larges}");
        drop(ctx);
        assert_eq!(stats.owned_resources(), (0, 0));
    }
}

/// Acceptance 3 and rule 4: a large block that is not reachable is released
/// at the end of the cycle, before the sweep steps. A reachable one stays.
#[test]
fn an_unreachable_large_block_is_released_at_the_end() {
    let mut ctx = ship();
    let kept = ctx.alloc(64 << 10, OBJECT_CLASS, 0) as usize;
    let dropped = ctx.alloc(64 << 10, OBJECT_CLASS, 0) as usize;
    let _ = chain_with_garbage(&mut ctx, 1, 100);
    let _roots = Roots::new(&mut ctx, &[kept]);
    let stats = ctx.test_arena_stats();
    assert_eq!(stats.owned_resources().1, 2);
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    run_to_sweep(&mut ctx, 1);
    assert!(ctx.sweep_in_progress());
    assert_eq!(stats.owned_resources().1, 1);
    assert!(ctx.is_live(kept) && !ctx.is_live(dropped));
    finish_sweep(&mut ctx, 1);
    assert_sweep_checks_pass(&ctx, 1);
    assert!(ctx.is_live(kept));
    assert_eq!(ctx.live_count(), 1);
}

/// Acceptance 3 and rule 10: a budget of 0 set during a sweep completes it
/// at the next poll, and that poll starts no cycle. Control: before the
/// change the sweep is in progress and the flag is clear.
#[test]
fn a_budget_of_zero_during_a_sweep_completes_it_at_the_next_poll() {
    let mut ctx = ship();
    for _ in 0..5_000 {
        let _ = object(&mut ctx);
    }
    let _roots = Roots::new(&mut ctx, &[0]);
    assert!(ctx.set_collection(CollectionMode::Automatic, 1 << 20));
    assert!(ctx.set_collection_step(10));
    poll_to_sweep(&mut ctx);
    assert!(!ctx.collection_pending());
    assert!(ctx.set_collection_step(0));
    assert!(ctx.collection_pending());
    ctx.collect_poll();
    assert!(!ctx.sweep_in_progress());
    assert!(!ctx.mark_in_progress());
    assert_eq!(ctx.live_count(), 0);
    assert_eq!(ctx.collection_stats().automatic_collections, 1);
    assert_sweep_checks_pass(&ctx, 1);
}

/// Rule 15: a step of mode 3 reads 5 slots for each word of the budget, or
/// an eighth of the slots of the sweep, and a new cycle starts at the poll
/// after the sweep completes.
#[test]
fn a_sweep_of_mode_three_has_at_most_eight_steps() {
    let mut ctx = ship();
    let head = chain_with_garbage(&mut ctx, 1_000, 3);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::StepStress, 0));
    assert!(ctx.set_collection_step(4));
    ctx.reset_collection_stats();
    let mut polls = 0;
    while ctx.collection_stats().automatic_collections == 0 {
        ctx.collect_poll();
        polls += 1;
        assert!(polls < 1_000, "the cycle does not end");
    }
    assert!(ctx.sweep_in_progress());
    let mut sweep_polls = 0;
    while ctx.sweep_in_progress() {
        ctx.collect_poll();
        sweep_polls += 1;
    }
    assert!(sweep_polls <= 8, "{sweep_polls}");
    let stats = ctx.collection_stats();
    assert_eq!(stats.sweep_steps, sweep_polls);
    assert!(stats.max_pause_slots_swept <= 4_000_usize.div_ceil(8) as u64);
    ctx.collect_poll();
    assert!(ctx.mark_in_progress(), "a new cycle starts");
    assert_sweep_checks_pass(&ctx, 1);
    assert_eq!(ctx.live_count(), 1_000);
}

/// Acceptance 5: a reachable block gets the word of a block to release
/// before a sweep step. In a part that the sweep did not reach, the sweep
/// releases it, and the check reports a released block that the roots
/// reach. In a part that the sweep read, the check reports a block to
/// release that the roots reach and that the walk of the slots finds.
/// Control: the same heap with no write reports nothing.
#[test]
fn the_check_finds_a_block_that_the_sweep_releases_wrongly() {
    for (write, reached) in [(false, false), (true, false), (true, true)] {
        let mut ctx = ship();
        let mut blocks = Vec::new();
        for _ in 0..100 {
            blocks.push(object(&mut ctx));
        }
        let held = blocks[60];
        let _roots = Roots::new(&mut ctx, &[blocks[0], held]);
        assert!(ctx.set_collection(CollectionMode::Off, 0));
        run_to_sweep(&mut ctx, 1);
        if reached {
            for _ in 0..13 {
                assert_eq!(ctx.collect_step(1), 1);
            }
            assert_eq!(ctx.sweep_position(), (0, 65));
        }
        if write {
            // SAFETY: `held` is a live payload with a complete header.
            unsafe {
                ((held - HEADER_SIZE) as *mut u64).write(ctx.marked_word());
            }
        }
        finish_sweep(&mut ctx, 1);
        let checks = ctx.sweep_checks();
        assert_eq!(checks.len(), 1);
        let check = &checks[0];
        match (write, reached) {
            (false, _) => assert_eq!(check.count(), 0, "{}", check.message()),
            (true, false) => {
                assert_eq!(check.released_reachable, 1, "{}", check.message());
                assert_eq!(check.to_release_reachable + check.to_release_slots, 0);
                assert_eq!(check.findings[0].block, held);
                assert_eq!(check.findings[0].holder_class, None);
                assert!(check.message().contains("reaches 1 released blocks"));
            }
            (true, true) => {
                assert_eq!(check.released_reachable, 0);
                assert_eq!(check.to_release_reachable, 1, "{}", check.message());
                assert_eq!(check.to_release_slots, 1);
                assert!(check.findings.iter().all(|finding| finding.block == held));
            }
        }
    }
}

/// Acceptance 4 and rule 2: no source file of the runtime compares a
/// header with a constant live word or a constant marked word, apart from
/// the places that this list names with their count of each name: the
/// definitions and the first words of a Context. A test file is outside
/// the search. Control: the search finds the names where the list says.
#[test]
fn no_runtime_code_compares_a_header_with_a_constant_word() {
    const ALLOWED: [(&str, usize, usize); 2] = [
        // The definitions, the module documentation, and the documentation
        // of `write_header` and of the marked word.
        ("context.rs", 4, 1),
        // The first live word, `alloc_state`, and the first marked word.
        ("context/mark_cycle.rs", 2, 1),
    ];
    fn walk(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read the source directory") {
            let path = entry.expect("read an entry").path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut found = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(&root)
            .expect("a file of the tree")
            .to_string_lossy()
            .replace('\\', "/");
        let name = relative.rsplit('/').next().unwrap_or_default();
        if name.contains("tests") || relative.contains("tests/") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("read a source file");
        let live = source.matches("LIVE_STATE").count();
        let marked = source.matches("MARK_STATE").count();
        if live + marked > 0 {
            found.push((relative, live, marked));
        }
    }
    found.sort();
    let allowed: Vec<(String, usize, usize)> = ALLOWED
        .iter()
        .map(|(file, live, marked)| ((*file).to_owned(), *live, *marked))
        .collect();
    assert_eq!(found, allowed);
}

/// The variable that makes the test below run its child side.
const SWEEP_MODE_THREE_CHILD: &str = "SUBSCRIPT_SWEEP_MODE_THREE_CHILD";

/// The child side: the shape of acceptance 5 in mode 3. The completion of
/// the sweep stops the process, so a return is a failure.
fn sweep_mode_three_child() {
    let mut ctx = Context::new_releasing();
    let mut blocks = Vec::new();
    for _ in 0..100 {
        blocks.push(object(&mut ctx));
    }
    let held = blocks[60];
    let _roots = Roots::new(&mut ctx, &[blocks[0], held]);
    assert!(ctx.set_collection(CollectionMode::StepStress, 0));
    assert!(ctx.set_collection_step(1));
    for _ in 0..1_000 {
        if ctx.sweep_in_progress() {
            break;
        }
        ctx.collect_poll();
    }
    assert!(ctx.sweep_in_progress());
    assert_eq!(ctx.sweep_position(), (0, 0));
    // SAFETY: `held` is a live payload with a complete header.
    unsafe { ((held - HEADER_SIZE) as *mut u64).write(ctx.marked_word()) };
    for _ in 0..1_000 {
        ctx.collect_poll();
    }
    panic!("mode 3 did not stop the process");
}

/// Rule 15: in mode 3 a failed check at the completion of a sweep stops the
/// process with the message. The control is
/// `a_sweep_of_mode_three_has_at_most_eight_steps`: the same mode with no
/// wrong word completes its sweep. Cost: one process start and about 20
/// Context pauses.
#[test]
fn mode_three_stops_the_process_when_the_sweep_check_fails() {
    if std::env::var_os(SWEEP_MODE_THREE_CHILD).is_some() {
        sweep_mode_three_child();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "context::sweep_steps_tests::mode_three_stops_the_process_when_the_sweep_check_fails",
            "--exact",
            "--test-threads",
            "1",
            "--nocapture",
        ])
        .env(SWEEP_MODE_THREE_CHILD, "1")
        .output()
        .expect("run the child");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr
            .contains("compiler.md §357 rule 15: a walk from the roots reaches 1 released blocks"),
        "{stderr}"
    );
    assert!(!stderr.contains("did not stop"), "{stderr}");
}

/// A change to mode 0 during a sweep keeps the sweep, and no poll serves
/// it; the live blocks stay valid, and `collect_step` ends it. A trap
/// during a sweep stops the polls, and the sweep continues after the host
/// clears the trap. Control: in mode 1, before the change, a poll runs a
/// sweep step.
#[test]
fn mode_zero_and_a_trap_during_a_sweep_keep_the_sweep() {
    let mut ctx = ship();
    let head = chain_with_garbage(&mut ctx, 500, 4);
    let _roots = Roots::new(&mut ctx, &[head]);
    assert!(ctx.set_collection(CollectionMode::Automatic, 1 << 20));
    assert!(ctx.set_collection_step(10));
    poll_to_sweep(&mut ctx);
    let before = ctx.collection_stats().sweep_steps;
    ctx.collect_automatic();
    assert_eq!(
        ctx.collection_stats().sweep_steps,
        before + 1,
        "the control"
    );
    // A trap stops the polls; the sweep stays.
    ctx.trap(TrapKind::Internal, "a trap during a sweep", 0);
    for _ in 0..100 {
        let _ = object(&mut ctx);
        ctx.collection_pending = 1;
        ctx.collect_poll();
    }
    assert_eq!(ctx.collection_stats().sweep_steps, before + 1);
    assert!(ctx.sweep_in_progress());
    ctx.clear_trap();
    ctx.collect_poll();
    assert_eq!(ctx.collection_stats().sweep_steps, before + 2);
    // Mode 0: no poll serves the sweep.
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    assert!(!ctx.collection_pending());
    for _ in 0..100 {
        let _ = object(&mut ctx);
        ctx.collect_poll();
    }
    assert!(ctx.sweep_in_progress());
    assert_eq!(ctx.collection_stats().sweep_steps, before + 2);
    assert!(ctx.is_live(head));
    finish_sweep(&mut ctx, 10);
    assert_sweep_checks_pass(&ctx, 1);
    // The chain and the 200 blocks allocated during the sweep.
    assert_eq!(ctx.live_count(), 500 + 200);
}
