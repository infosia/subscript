//! The requested bytes and the limit of a Context (`compiler.md` §346).
//! Each "does not happen" test has a firing control in the same shape.

use subscript_runtime::context::{ArrayElements, CollectionMode, DEFAULT_MIN_HEADROOM_BYTES};
use subscript_runtime::ffi::{subscript_rt_map_new, subscript_rt_map_set};
use subscript_runtime::Context;

fn context(ship: bool) -> Box<Context> {
    if ship {
        Context::new_releasing()
    } else {
        Context::new()
    }
}

/// One root word. Setting it to zero makes its object unreachable.
struct RootSlot(*mut usize);

impl RootSlot {
    fn new(ctx: &mut Context) -> Self {
        let slot = Box::into_raw(Box::new(0usize));
        // SAFETY: the slot stays allocated for the test; the Context drops
        // before the process ends.
        unsafe { ctx.root_add(slot as usize, 1) };
        Self(slot)
    }

    fn get(&self) -> usize {
        // SAFETY: the slot is a live, exclusive allocation of this test.
        unsafe { self.0.read() }
    }

    fn set(&self, value: usize) {
        // SAFETY: the slot is a live, exclusive allocation of this test.
        unsafe { self.0.write(value) };
    }
}

/// `compiler.md` §346 rule 1: an allocation adds the requested payload
/// size, the same number in each tier. The ship tier holds more than it
/// requests for these sizes, so the live bytes do not give the count.
#[test]
fn an_allocation_adds_the_requested_size_in_each_tier() {
    let sizes = [1usize, 17, 49, 100, 113, 241, 4_000, 100_000];
    for ship in [false, true] {
        let mut ctx = context(ship);
        assert_eq!(ctx.requested_bytes(), 0, "a new Context starts at zero");
        let mut total = 0;
        for size in sizes {
            let before = ctx.requested_bytes();
            assert!(!ctx.alloc(size, 900, 0).is_null());
            assert_eq!(ctx.requested_bytes() - before, size);
            total += size;
        }
        assert_eq!(ctx.requested_bytes(), total);
        if ship {
            assert!(ctx.live_bytes() > total, "the ship tier holds capacities");
        } else {
            assert_eq!(ctx.live_bytes(), total);
        }
    }
}

/// `compiler.md` §346 rule 1: a release subtracts the request of its block.
/// After an allocation and a release the count is back at its value
/// before, and the count is the same number in the two kinds at each step.
#[test]
fn free_subtracts_the_request_of_the_block() {
    let mut steps = Vec::new();
    for ship in [false, true] {
        let mut ctx = context(ship);
        let mut counts = Vec::new();
        let first = ctx.alloc(100, 900, 0) as usize;
        let second = ctx.alloc(100, 900, 0) as usize;
        counts.push(ctx.requested_bytes());
        ctx.delete(first, 0);
        counts.push(ctx.requested_bytes());
        // A block of 17 requested bytes; the ship tier holds 48 for it.
        for size in [17usize, 24, 49, 241, 4_000, 100_000] {
            let before = ctx.requested_bytes();
            let block = ctx.alloc(size, 900, 0) as usize;
            assert_eq!(ctx.requested_bytes(), before + size);
            ctx.delete(block, 0);
            assert_eq!(ctx.requested_bytes(), before, "size {size}");
            counts.push(ctx.requested_bytes());
        }
        // A reused block records the request of its new allocation.
        let reused = ctx.alloc(20, 900, 0) as usize;
        counts.push(ctx.requested_bytes());
        ctx.delete(reused, 0);
        ctx.delete(second, 0);
        counts.push(ctx.requested_bytes());
        assert_eq!(counts, [200, 100, 100, 100, 100, 100, 100, 100, 120, 0]);
        steps.push(counts);
    }
    assert_eq!(steps[0], steps[1]);
}

/// `compiler.md` §346 rule 1: the count does not go under zero. A release
/// of a block that was allocated before the last collection finds the
/// count at zero, and the count stays zero.
#[test]
fn a_release_after_a_collection_stays_at_zero() {
    for ship in [false, true] {
        let mut ctx = context(ship);
        let root = RootSlot::new(&mut ctx);
        root.set(ctx.alloc(100, 900, 0) as usize);
        ctx.collect();
        assert_eq!(ctx.requested_bytes(), 0);
        ctx.delete(root.get(), 0);
        root.set(0);
        assert_eq!(ctx.requested_bytes(), 0);
        // Control: the count rises from zero by the next request.
        assert!(!ctx.alloc(17, 900, 0).is_null());
        assert_eq!(ctx.requested_bytes(), 17);
    }
}

/// `compiler.md` §346 rule 1: an array growth adds the request of the new
/// buffer and subtracts the request of the old buffer, the same number in
/// the two kinds.
#[test]
fn an_array_growth_subtracts_the_request_of_the_old_buffer() {
    const ELEMENT: usize = 24;
    let mut counts = Vec::new();
    for ship in [false, true] {
        let mut ctx = context(ship);
        let array = ctx.array_new(ELEMENT, 0);
        let header = ctx.requested_bytes();
        let element = [7u8; ELEMENT];
        // The first push allocates four elements; the next three fit.
        for length in 1..=4 {
            // SAFETY: `element` has ELEMENT readable bytes.
            let pushed = unsafe { ctx.array_push(array, element.as_ptr(), 0) };
            assert_eq!(pushed, length);
        }
        assert_eq!(ctx.requested_bytes(), header + 4 * ELEMENT);
        // SAFETY: as above. The fifth push grows the buffer to eight.
        assert_eq!(unsafe { ctx.array_push(array, element.as_ptr(), 0) }, 5);
        assert_eq!(ctx.requested_bytes(), header + 8 * ELEMENT);
        counts.push((header, ctx.requested_bytes()));
    }
    assert_eq!(counts[0], counts[1]);
}

/// `compiler.md` §346 rule 1: a collection sets the count to zero, and the
/// sweep does not subtract from the count after it. The next allocation
/// counts from zero.
#[test]
fn a_collection_sets_the_count_to_zero() {
    for ship in [false, true] {
        for automatic in [false, true] {
            let mut ctx = context(ship);
            let root = RootSlot::new(&mut ctx);
            root.set(ctx.alloc(64, 900, 0) as usize);
            for _ in 0..100 {
                assert!(!ctx.alloc(200, 900, 0).is_null());
            }
            assert_eq!(ctx.requested_bytes(), 64 + 100 * 200);
            if automatic {
                ctx.collect_automatic();
            } else {
                ctx.collect();
            }
            assert_eq!(ctx.live_count(), 1);
            assert_eq!(ctx.requested_bytes(), 0);
            assert!(!ctx.alloc(10, 900, 0).is_null());
            assert_eq!(ctx.requested_bytes(), 10);
            // The release of the survivor after the collection subtracts
            // once, from the count of the new allocation.
            ctx.delete(root.get(), 0);
            root.set(0);
            assert_eq!(ctx.requested_bytes(), 0);
        }
    }
}

/// `compiler.md` §346 rule 2: the flag is set exactly when the count passes
/// the limit, and a free under the limit keeps it clear. Control: mode 0
/// never sets the flag.
#[test]
fn the_flag_is_set_when_the_count_passes_the_limit() {
    const LIMIT: usize = 4096;
    for ship in [false, true] {
        for mode in [CollectionMode::Automatic, CollectionMode::Off] {
            let mut ctx = context(ship);
            assert!(ctx.set_collection(mode, LIMIT));
            assert_eq!(ctx.collection_limit(), LIMIT);
            // 64 blocks of 64 requested bytes reach the limit, not over it.
            let blocks: Vec<usize> = (0..64).map(|_| ctx.alloc(64, 900, 0) as usize).collect();
            assert_eq!(ctx.requested_bytes(), LIMIT);
            assert!(!ctx.collection_pending());
            // A free and a new block of the same size keep the count at or
            // under the limit.
            ctx.delete(blocks[0], 0);
            assert!(!ctx.alloc(64, 900, 0).is_null());
            assert!(ctx.requested_bytes() <= LIMIT);
            assert!(!ctx.collection_pending());
            // One byte more passes the limit in mode 1.
            let extra = LIMIT + 1 - ctx.requested_bytes();
            assert!(!ctx.alloc(extra, 900, 0).is_null());
            assert_eq!(ctx.requested_bytes(), LIMIT + 1);
            assert_eq!(ctx.collection_pending(), mode == CollectionMode::Automatic);
            assert_eq!(ctx.automatic_collection_count(), 0);
        }
    }
}

/// `compiler.md` §346 acceptance 1: the `collect` shape in a smaller scale.
/// Each round allocates nodes that own four strings of 17, 49, 113, and
/// 241 requested bytes, keeps three quarters, polls after each node, and
/// collects explicitly. The ship tier holds about twice the requested
/// bytes. No automatic collection runs between two explicit ones.
#[test]
fn the_collect_shape_runs_no_automatic_collection() {
    // One tenth of the default headroom for one tenth of the nodes.
    const HEADROOM: usize = DEFAULT_MIN_HEADROOM_BYTES / 10;
    for ship in [true, false] {
        assert_eq!(
            collect_shape(ship, HEADROOM),
            0,
            "ship {ship}: automatic collections between explicit ones"
        );
        // Firing control: the same shape with a headroom under the request
        // of one round (936,000 bytes) runs automatic collections.
        assert!(collect_shape(ship, HEADROOM / 4) > 0, "ship {ship}");
    }
}

/// One run of the `collect` shape. Returns the automatic collection count.
fn collect_shape(ship: bool, headroom: usize) -> u64 {
    const COUNT: usize = 2_000;
    const ROUNDS: usize = 6;
    {
        let mut ctx = context(ship);
        assert!(ctx.set_collection(CollectionMode::Automatic, headroom));
        let keep = RootSlot::new(&mut ctx);
        let dropped = RootSlot::new(&mut ctx);
        let mut state: u32 = 0x1234_5678;
        for round in 0..ROUNDS {
            keep.set(0);
            for i in 0..COUNT {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let uid = format!("{}", round * COUNT + i);
                let node = ctx.alloc(48, 900, 0);
                assert!(!node.is_null());
                let next = if state & 3 != 0 { &keep } else { &dropped };
                // SAFETY: the node has 48 writable bytes: the value, four
                // string handles, and the next node.
                unsafe {
                    let words = node.cast::<usize>();
                    words.write(state as usize);
                    words.add(5).write(next.get());
                }
                next.set(node as usize);
                for (field, (len, fill)) in [(9, 'a'), (41, 'b'), (105, 'c'), (233, 'd')]
                    .into_iter()
                    .enumerate()
                {
                    let text = format!("{}{uid}", fill.to_string().repeat(len - uid.len()));
                    let string = ctx.alloc_str(text.as_bytes(), 0);
                    assert!(!string.is_null());
                    // SAFETY: as above; the node is reachable from a root.
                    unsafe { node.cast::<usize>().add(1 + field).write(string as usize) };
                }
                // The poll of the loop back edge (`compiler.md` §342 rule 7).
                ctx.collect_poll();
            }
            dropped.set(0);
            ctx.collect();
        }
        assert!(ctx.live_count() > 3 * COUNT, "the survivors stay live");
        ctx.automatic_collection_count()
    }
}

/// Builds a full binary tree of `depth` with 16-byte nodes. Each node is in
/// `nodes`, which a root covers, so a poll during the build keeps the tree.
/// Polls after each node. Returns the root.
fn build(ctx: &mut Context, nodes: &mut [usize], next: &mut usize, depth: u32) -> usize {
    let (left, right) = if depth == 0 {
        (0, 0)
    } else {
        (
            build(ctx, nodes, next, depth - 1),
            build(ctx, nodes, next, depth - 1),
        )
    };
    let node = ctx.alloc(16, 900, 0);
    assert!(!node.is_null());
    // SAFETY: the node has 16 writable bytes.
    unsafe {
        node.cast::<usize>().write(left);
        node.cast::<usize>().add(1).write(right);
    }
    nodes[*next] = node as usize;
    *next += 1;
    ctx.collect_poll();
    node as usize
}

/// `compiler.md` §346 acceptance 4: the `tree` shape in a smaller scale.
/// With `Context.free` of each node no automatic collection runs. Control:
/// the same shape with no free runs more than one.
#[test]
fn free_of_each_tree_node_runs_no_automatic_collection() {
    const DEPTH: u32 = 10;
    const TREES: usize = 30;
    const NODES: usize = (1 << (DEPTH + 1)) - 1;
    const HEADROOM: usize = 65_536;
    for ship in [false, true] {
        for free in [true, false] {
            let mut ctx = context(ship);
            assert!(ctx.set_collection(CollectionMode::Automatic, HEADROOM));
            let mut nodes = vec![0usize; NODES];
            // SAFETY: the vector stays allocated and unmoved while the
            // Context lives.
            unsafe { ctx.root_add(nodes.as_mut_ptr() as usize, NODES) };
            for _ in 0..TREES {
                let mut next = 0;
                build(&mut ctx, &mut nodes, &mut next, DEPTH);
                assert_eq!(next, NODES);
                for slot in nodes.iter_mut() {
                    if free {
                        ctx.delete(*slot, 0);
                    }
                    *slot = 0;
                }
            }
            if free {
                assert_eq!(ctx.automatic_collection_count(), 0, "ship {ship}");
                assert_eq!(ctx.live_count(), 0);
            } else {
                assert!(ctx.automatic_collection_count() > 1, "ship {ship}");
            }
            drop(ctx);
            drop(nodes);
        }
    }
}

/// The headroom of the acceptance 7 tests (`compiler.md` §346.2 item 7).
const LOOP_HEADROOM: usize = 65_536;

/// Runs `iteration` until the pending flag is set, at most `bound * 4`
/// times. Returns the index (from 1) of the iteration that set the flag,
/// or `None`. No poll runs, so the flag stays set.
fn iterations_until_due(
    ctx: &mut Context,
    bound: usize,
    mut iteration: impl FnMut(&mut Context),
) -> Option<usize> {
    assert!(ctx.set_collection(CollectionMode::Automatic, LOOP_HEADROOM));
    for index in 1..=bound * 4 {
        iteration(ctx);
        if ctx.collection_pending() {
            return Some(index);
        }
    }
    None
}

/// `compiler.md` §346 acceptance 7a: a loop that allocates two objects of
/// 24 bytes, frees one, and drops one makes a collection due after at most
/// `limit / 24 + 1` iterations, the same count in the two kinds.
#[test]
fn a_free_does_not_hide_the_dropped_object() {
    let bound = LOOP_HEADROOM / 24 + 1;
    let mut due = Vec::new();
    for ship in [false, true] {
        let mut ctx = context(ship);
        let found = iterations_until_due(&mut ctx, bound, |ctx| {
            let freed = ctx.alloc(24, 900, 0) as usize;
            assert!(!ctx.alloc(24, 900, 0).is_null());
            ctx.delete(freed, 0);
        });
        println!("ship {ship}: due after {found:?} iterations, bound {bound}");
        let found = found.expect("a collection becomes due");
        assert!(found <= bound, "ship {ship}: {found} > {bound}");
        due.push(found);
    }
    assert_eq!(due[0], due[1]);
}

/// Builds an array of 256 `f64` elements by `push`.
fn pushed_array(ctx: &mut Context) -> *mut u8 {
    let array = ctx.array_new_of(8, ArrayElements::NoHandle, 0);
    assert!(!array.is_null());
    for index in 0..256u32 {
        let value = f64::from(index);
        // SAFETY: `value` has 8 readable bytes, the element size.
        let length = unsafe { ctx.array_push(array, (&raw const value).cast(), 0) };
        assert_eq!(length, index as i32 + 1);
    }
    array
}

/// `compiler.md` §346 acceptance 7b: a loop that builds an array of 256
/// `f64` elements by `push` and drops it makes a collection due after the
/// same count of arrays in the two kinds. One array leaves its header and
/// its last buffer of 2,048 bytes in the count, because each growth
/// subtracts the request of the buffer that it replaces.
#[test]
fn an_array_growth_does_not_hide_the_dropped_array() {
    let mut due = Vec::new();
    for ship in [false, true] {
        let mut probe = context(ship);
        assert!(!probe.array_new_of(8, ArrayElements::NoHandle, 0).is_null());
        let header = probe.requested_bytes();
        let per_array = header + 256 * 8;
        let bound = LOOP_HEADROOM / per_array + 1;
        let mut ctx = context(ship);
        let found = iterations_until_due(&mut ctx, bound, |ctx| {
            pushed_array(ctx);
        });
        println!("ship {ship}: due after {found:?} arrays, {per_array} bytes each, bound {bound}");
        let found = found.expect("a collection becomes due");
        // The flag is set inside the array that takes the count over the
        // limit: no later than the bound, and not before the last whole
        // array under the limit less the largest growth step.
        assert!(found <= bound, "ship {ship}: {found} > {bound}");
        assert!(found + 1 >= bound, "ship {ship}: {found} is early");
        due.push(found);
    }
    assert_eq!(due[0], due[1]);
}

/// Builds a Map of 256 `i32` keys and `f64` values by inserts.
fn inserted_map(ctx: &mut Context) -> *mut u8 {
    // SAFETY: the Context is live; key kind 0 is a bits key of 4 bytes.
    let map = unsafe { subscript_rt_map_new(ctx, 4, 8, 0, 0) };
    assert!(!map.is_null());
    for key in 0..256i32 {
        let value = f64::from(key);
        // SAFETY: `key` and `value` have the stated sizes.
        unsafe {
            subscript_rt_map_set(
                ctx,
                map,
                (&raw const key).cast(),
                (&raw const value).cast(),
                0,
            )
        };
    }
    map
}

/// `compiler.md` §346 acceptance 7d: a Map that
/// grows by inserts and is dropped makes a collection due after the same
/// count of Maps in the two kinds. The count of one Map is read in a
/// separate Context with no collection.
#[test]
fn a_map_growth_does_not_hide_the_dropped_map() {
    let mut due = Vec::new();
    let mut per_map_counts = Vec::new();
    for ship in [false, true] {
        let mut probe = context(ship);
        assert!(probe.set_collection(CollectionMode::Off, LOOP_HEADROOM));
        inserted_map(&mut probe);
        let per_map = probe.requested_bytes();
        assert!(per_map >= 256 * (4 + 8), "the entries stay in the count");
        let bound = LOOP_HEADROOM / per_map + 1;
        let mut ctx = context(ship);
        let found = iterations_until_due(&mut ctx, bound, |ctx| {
            inserted_map(ctx);
        });
        println!("ship {ship}: due after {found:?} Maps, {per_map} bytes each, bound {bound}");
        let found = found.expect("a collection becomes due");
        assert!(found <= bound, "ship {ship}: {found} > {bound}");
        due.push(found);
        per_map_counts.push(per_map);
    }
    assert_eq!(per_map_counts[0], per_map_counts[1]);
    assert_eq!(due[0], due[1]);
}

/// `compiler.md` §346 acceptance 7c: the controls. The same loops with each
/// block freed run no automatic collection, and the count returns to its
/// value before each iteration, in the two kinds.
#[test]
fn the_loops_with_each_block_freed_run_no_automatic_collection() {
    for ship in [false, true] {
        let mut ctx = context(ship);
        assert!(ctx.set_collection(CollectionMode::Automatic, LOOP_HEADROOM));
        for _ in 0..4 * (LOOP_HEADROOM / 24 + 1) {
            let before = ctx.requested_bytes();
            let first = ctx.alloc(24, 900, 0) as usize;
            let second = ctx.alloc(24, 900, 0) as usize;
            ctx.delete(first, 0);
            ctx.delete(second, 0);
            assert_eq!(ctx.requested_bytes(), before);
            ctx.collect_poll();
        }
        for _ in 0..200 {
            let before = ctx.requested_bytes();
            let array = pushed_array(&mut ctx);
            // SAFETY: `array` is a live array of this Context.
            let data = unsafe { ctx.array_data(array) } as usize;
            ctx.delete(data, 0);
            ctx.delete(array as usize, 0);
            assert_eq!(ctx.requested_bytes(), before);
            let map = inserted_map(&mut ctx);
            ctx.delete(map as usize, 0);
            assert_eq!(ctx.requested_bytes(), before);
            ctx.collect_poll();
        }
        assert_eq!(ctx.automatic_collection_count(), 0, "ship {ship}");
        assert_eq!(ctx.live_count(), 0, "ship {ship}");
    }
}

/// `compiler.md` §346 rule 2b, the worked case in a smaller scale. `N`
/// objects of `S` bytes stay through one collection. Then each iteration
/// frees one of them and drops one new object of `S` bytes. The release of
/// a kept object lowers the count, so the count stays at `S` for `N`
/// iterations and then rises by `S` each iteration. A collection becomes
/// due at iteration `N + H / S`, plus or minus one, the same in each kind.
#[test]
fn a_release_of_a_kept_object_delays_the_collection_by_the_kept_bytes() {
    // The ship tier holds 2,032 bytes for each kept object, so the live set
    // stays under the minimum headroom, as in the worked case.
    const N: usize = 20;
    const S: usize = 1_024;
    const H: usize = 65_536;
    let expected = N + H / S;
    let mut due = Vec::new();
    for ship in [false, true] {
        let mut ctx = context(ship);
        assert!(ctx.set_collection(CollectionMode::Automatic, H));
        let mut kept: Vec<usize> = (0..N).map(|_| ctx.alloc(S, 900, 0) as usize).collect();
        // SAFETY: the vector stays allocated and unmoved while the Context
        // lives.
        unsafe { ctx.root_add(kept.as_mut_ptr() as usize, N) };
        ctx.collect();
        assert_eq!(ctx.live_count(), N);
        assert!(ctx.live_bytes() < H, "the limit is the minimum headroom");
        assert_eq!(ctx.collection_limit(), H);
        let mut found = None;
        for iteration in 1..=2 * expected {
            if let Some(slot) = kept.get_mut(iteration - 1) {
                ctx.delete(*slot, 0);
                *slot = 0;
            }
            assert!(!ctx.alloc(S, 900, 0).is_null());
            if ctx.collection_pending() {
                found = Some(iteration);
                break;
            }
        }
        println!("ship {ship}: due at iteration {found:?}, expected {expected}");
        let found = found.expect("a collection becomes due");
        assert!(found.abs_diff(expected) <= 1, "ship {ship}: {found}");
        due.push(found);
        drop(ctx);
        drop(kept);
    }
    assert_eq!(due[0], due[1]);
}

/// `compiler.md` §346 rule 2c: the reserved bytes of the ship arena include
/// the request record of each chunk, 2 bytes for each block of its class.
/// Control: the dev kind reserves the exact layout and no record.
#[test]
fn the_reserved_bytes_include_the_request_record() {
    const CHUNK: usize = 64 * 1024;
    const HEADER: usize = 16;
    for size in [16usize, 100, 1_000] {
        let block = (HEADER + size).next_power_of_two().max(32);
        let mut ship = Context::new_releasing();
        assert_eq!(ship.reserved_bytes(), 0);
        assert!(!ship.alloc(size, 900, 0).is_null());
        assert_eq!(
            ship.reserved_bytes(),
            CHUNK + 2 * (CHUNK / block),
            "size {size}"
        );
        let mut dev = Context::new();
        assert!(!dev.alloc(size, 900, 0).is_null());
        assert_eq!(dev.reserved_bytes(), HEADER + size, "size {size}");
    }
}
