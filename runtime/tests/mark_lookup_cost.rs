//! The cost of the mark for each word in the ship arena, and the cost of
//! an allocation (`compiler.md` §355 acceptance 6). This file holds
//! measurements, not gates: each test is ignored and states no limit.
//!
//! An integration test links the runtime without `cfg(test)`, so no
//! test-only counter adds to a lookup. Run each test alone, in a release
//! build, with nothing else on the host:
//!
//! `cargo test --offline --release -p subscript-runtime --test
//! mark_lookup_cost -- --ignored --nocapture --test-threads=1`

use std::time::Instant;

use subscript_runtime::context::{ArrayElements, CLASS_ARRAY_DATA_NO_HANDLE};
use subscript_runtime::Context;

/// The payload sizes of the four size classes of the heap: blocks of 32,
/// 64, 128, and 256 bytes.
const PAYLOADS: [usize; 4] = [16, 48, 112, 240];
const BLOCKS: [usize; 4] = [32, 64, 128, 256];
/// The part of the heap in each size class, in eighths of 15.
const WEIGHTS: [usize; 4] = [8, 4, 2, 1];
/// The live bytes of the heap: 1.5 MiB in blocks.
const LIVE_BYTES: usize = 1536 * 1024;
/// The passes of each median.
const PASSES: usize = 31;
/// A user class id that can hold a handle.
const OBJECT_CLASS: u32 = 900;
/// The requested bytes of one object of the shapes of G9.
const OBJECT_BYTES: usize = 32;

/// A buffer that the test writes before a pass, so that the pass starts
/// with no heap data in the cache.
struct Evict(Vec<u8>);

impl Evict {
    fn new() -> Self {
        Self(vec![0u8; 64 << 20])
    }

    fn run(&mut self, round: usize) {
        let value = round as u8;
        for line in self.0.chunks_mut(64) {
            line[0] = value;
        }
        std::hint::black_box(&self.0);
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    /// The live blocks are the first blocks of each size class, so they
    /// fill few chunks.
    Dense,
    /// The live blocks are spread over each chunk.
    Sparse,
}

/// Allocates `slots` blocks in four size classes and returns the live
/// blocks of `shape`: 1.5 MiB, or fewer if the heap is smaller.
fn build(ctx: &mut Context, slots: usize, shape: Shape) -> Vec<usize> {
    let weight_sum: usize = WEIGHTS.iter().sum();
    let mut live = Vec::new();
    for class in 0..4 {
        let in_class = slots * WEIGHTS[class] / weight_sum;
        let live_in_class = (LIVE_BYTES / 4 / BLOCKS[class]).min(in_class);
        let mut all = Vec::with_capacity(in_class);
        for _ in 0..in_class {
            let payload = ctx.alloc(PAYLOADS[class], CLASS_ARRAY_DATA_NO_HANDLE, 0);
            assert!(!payload.is_null());
            all.push(payload as usize);
        }
        match shape {
            Shape::Dense => live.extend_from_slice(&all[..live_in_class]),
            Shape::Sparse => {
                live.extend((0..live_in_class).map(|i| all[i * in_class / live_in_class]));
            }
        }
    }
    live
}

/// The chunks of a heap of `slots` blocks: a chunk has 64 KiB.
fn chunks_of(slots: usize) -> usize {
    let weight_sum: usize = WEIGHTS.iter().sum();
    (0..4)
        .map(|class| (slots * WEIGHTS[class] / weight_sum).div_ceil((64 << 10) / BLOCKS[class]))
        .sum()
}

/// The mark for each word at 108 and 4,168 chunks, dense and sparse.
///
/// The roots are 23,040 words, and each one is a live block that holds no
/// handle, so each word read is a lookup that finds a block. The record of
/// §353 gives the time of a whole collection. The test takes the median
/// time with the roots, then releases the roots and takes the median time
/// of the same heap with no live block. The difference is the mark of the
/// roots and the restore of their state in the sweep; the sweep of the
/// other slots is in both. Then each root word moves 8 bytes into its
/// block, off the block grid: the difference to the heap with no root is
/// the lookup of a word that finds a chunk and no block. A pass starts
/// after a write of 64 MiB.
///
/// Cost: about 1.5 s in a release build. Memory peaks at about 350 MiB:
/// 261 MiB of chunks for 4,168 chunks, 8 MiB of request records, the
/// list of 2.1 million blocks of one class (17 MiB), and the 64 MiB buffer.
#[test]
#[ignore = "a measurement of compiler.md §355 acceptance 6, run by hand in a release build"]
fn the_mark_for_each_word() {
    let mut evict = Evict::new();
    // A warm-up: the first writes of the buffer map its pages.
    for round in 0..4 {
        evict.run(round);
    }
    for slots in [100_000usize, 4_000_000] {
        for shape in [Shape::Dense, Shape::Sparse] {
            let mut ctx = Context::new_releasing();
            let mut roots = build(&mut ctx, slots, shape).into_boxed_slice();
            // SAFETY: the box holds `roots.len()` words until the pop below.
            unsafe { ctx.shadow_push(roots.as_mut_ptr() as usize, roots.len()) };
            ctx.collect();
            let chunks = chunks_of(slots);
            let mut with_roots = Vec::new();
            for round in 0..PASSES {
                evict.run(round);
                ctx.collect();
                with_roots.push(ctx.collection_stats().last_ns as f64);
            }
            let words = ctx.collection_stats().last_words_read;
            assert_eq!(words, roots.len() as u64);
            ctx.shadow_pop();
            ctx.collect();
            let mut no_roots = Vec::new();
            for round in 0..PASSES {
                evict.run(round);
                ctx.collect();
                no_roots.push(ctx.collection_stats().last_ns as f64);
            }
            for root in roots.iter_mut() {
                *root += 8;
            }
            // SAFETY: the box holds `roots.len()` words until the pop below.
            unsafe { ctx.shadow_push(roots.as_mut_ptr() as usize, roots.len()) };
            let mut interior = Vec::new();
            for round in 0..PASSES {
                evict.run(round);
                ctx.collect();
                interior.push(ctx.collection_stats().last_ns as f64);
            }
            assert_eq!(ctx.collection_stats().last_words_read, words);
            ctx.shadow_pop();
            let (full, sweep, lookup) = (median(with_roots), median(no_roots), median(interior));
            println!(
                "MARK slots={slots} shape={} chunks={chunks} words={words} collect_us={:.1} no_live_us={:.1} mark_ns_per_word={:.2} lookup_ns_per_word={:.2}",
                if shape == Shape::Dense { "dense" } else { "sparse" },
                full / 1e3,
                sweep / 1e3,
                (full - sweep) / words as f64,
                (lookup - sweep) / words as f64,
            );
            drop(ctx);
            drop(roots);
        }
    }
}

/// An array of references to `count` objects of 32 bytes.
fn references(ctx: &mut Context, count: usize) -> *mut u8 {
    let array = ctx.array_with_capacity_of(count, 8, ArrayElements::CanHoldHandle, 0);
    assert!(!array.is_null());
    for _ in 0..count {
        let object = ctx.alloc(OBJECT_BYTES, OBJECT_CLASS, 0);
        assert!(!object.is_null());
        let element = object as usize;
        // SAFETY: `array` is a live array of this Context with an element
        // size of 8, and `element` is 8 readable bytes.
        let len = unsafe { ctx.array_push(array, (&raw const element).cast(), 0) };
        assert!(len > 0);
    }
    array
}

/// A chain of `count` objects of 32 bytes: each one holds the previous one.
fn chain(ctx: &mut Context, count: usize) -> *mut u8 {
    let mut next = 0usize;
    for _ in 0..count {
        let object = ctx.alloc(OBJECT_BYTES, OBJECT_CLASS, 0);
        assert!(!object.is_null());
        // SAFETY: the new object has `OBJECT_BYTES` writable bytes.
        unsafe { object.cast::<usize>().write(next) };
        next = object as usize;
    }
    next as *mut u8
}

type Build = fn(&mut Context, usize) -> *mut u8;

/// The shapes of G9 of `specs/tracking/automatic-collection.md` with no
/// test-only counter: the median time of five explicit collections for a
/// live heap of 1, 4, 16, and 64 MiB. An object of 32 bytes takes a
/// block of 64 bytes, a payload of 48 bytes.
///
/// Cost: about 0.5 s in a release build. Memory peaks at about 90 MiB:
/// at 64 MiB, 1.4 million blocks of 64 bytes for the chain, and 1.2
/// million blocks and a 9.6 MiB array for the references.
#[test]
#[ignore = "a measurement of compiler.md §355 acceptance 6, run by hand in a release build"]
fn the_collection_time_curve_without_the_test_counter() {
    let object = 48usize;
    let shapes: [(&str, Build, usize); 2] = [
        ("references", references, object + 8),
        ("chain", chain, object),
    ];
    for (name, build, bytes_per_element) in shapes {
        for mebibytes in [1usize, 4, 16, 64] {
            let mut ctx = Context::new_releasing();
            let mut head = build(&mut ctx, (mebibytes << 20) / bytes_per_element) as usize;
            // SAFETY: `head` outlives the shadow frame, which the test pops.
            unsafe { ctx.shadow_push(&raw mut head as usize, 1) };
            let mut times = Vec::new();
            for _ in 0..5 {
                ctx.collect();
                times.push(ctx.collection_stats().last_ns as f64);
            }
            let stats = ctx.collection_stats();
            let ns = median(times);
            println!(
                "CURVE shape={name} mib={mebibytes} words={} ms={:.3} ns_per_word={:.2}",
                stats.last_words_read,
                ns / 1e6,
                ns / stats.last_words_read as f64,
            );
            ctx.shadow_pop();
        }
    }
}

/// The time of one bump allocation, of one reuse from a free list, and of
/// one allocation that creates a chunk, median of nine runs.
///
/// A bump allocation and a reuse are each 2^20 requests of 16 and 48
/// bytes. A chunk creation is the first request of 4,080 bytes into a
/// full chunk of the largest size class, timed one at a time over 2,000
/// chunks; the time includes the first write into the chunk.
///
/// Cost: about 0.5 s in a release build. Memory peaks at about 130 MiB:
/// the 2,000 chunks of the creation run (125 MiB).
#[test]
#[ignore = "a measurement of compiler.md §355 acceptance 6, run by hand in a release build"]
fn the_cost_of_an_allocation() {
    const REQUESTS: usize = 1 << 20;
    const CHUNKS: usize = 2000;
    const LARGEST_PAYLOAD: usize = 4096 - 16;
    let mut bump = Vec::new();
    let mut reuse = Vec::new();
    let mut creation = Vec::new();
    for _ in 0..9 {
        let mut ctx = Context::new_releasing();
        let start = Instant::now();
        for i in 0..REQUESTS {
            std::hint::black_box(ctx.alloc(PAYLOADS[i & 1], CLASS_ARRAY_DATA_NO_HANDLE, 0));
        }
        bump.push(start.elapsed().as_nanos() as f64 / REQUESTS as f64);
        ctx.collect();
        let start = Instant::now();
        for i in 0..REQUESTS {
            std::hint::black_box(ctx.alloc(PAYLOADS[i & 1], CLASS_ARRAY_DATA_NO_HANDLE, 0));
        }
        reuse.push(start.elapsed().as_nanos() as f64 / REQUESTS as f64);
        drop(ctx);

        let mut ctx = Context::new_releasing();
        let mut ns = 0u128;
        for i in 0..CHUNKS * 16 {
            if i % 16 == 0 {
                let start = Instant::now();
                std::hint::black_box(ctx.alloc(LARGEST_PAYLOAD, CLASS_ARRAY_DATA_NO_HANDLE, 0));
                ns += start.elapsed().as_nanos();
            } else {
                std::hint::black_box(ctx.alloc(LARGEST_PAYLOAD, CLASS_ARRAY_DATA_NO_HANDLE, 0));
            }
        }
        creation.push(ns as f64 / CHUNKS as f64);
    }
    println!(
        "ALLOC bump_ns={:.2} reuse_ns={:.2} chunk_creation_ns={:.0}",
        median(bump),
        median(reuse),
        median(creation),
    );
}

/// The median of `PASSES` passes of `f` over `count` items, in ns for
/// each item.
fn per_item(count: usize, mut f: impl FnMut() -> usize) -> f64 {
    let mut times = Vec::new();
    for _ in 0..PASSES {
        let start = Instant::now();
        std::hint::black_box(f());
        times.push(start.elapsed().as_nanos() as f64 / count as f64);
    }
    median(times)
}

/// The cost of the map of the large allocations: N = 1, 64, 1,024, and
/// 8,192 allocations of 4,096 and of 16,384 bytes, each above the largest
/// size class. It prints the count of payloads whose base has an alignment
/// of 4 KiB and of 16 KiB, then ns for one `is_live` of a payload (a hit),
/// of 0, of a small integer, and of an address inside a chunk block (each
/// a miss of the map), and ns for one large allocation and one delete.
///
/// Cost: about 0.3 s in a release build. Memory peaks at about 135 MiB:
/// 8,192 allocations of 16 KiB and 1 MiB of small blocks.
#[test]
#[ignore = "a measurement of compiler.md §355 rule 5, run by hand in a release build"]
fn the_lookup_of_a_large_allocation() {
    const MISSES: usize = 1 << 14;
    for size in [4096usize, 16384] {
        for count in [1usize, 64, 1024, 8192] {
            let mut ctx = Context::new_releasing();
            let blocks: Vec<usize> = (0..MISSES)
                .map(|i| ctx.alloc(PAYLOADS[i & 3], CLASS_ARRAY_DATA_NO_HANDLE, 0) as usize + 8)
                .collect();
            let start = Instant::now();
            let payloads: Vec<usize> = (0..count)
                .map(|_| ctx.alloc(size, CLASS_ARRAY_DATA_NO_HANDLE, 0) as usize)
                .collect();
            let alloc_ns = start.elapsed().as_nanos() as f64 / count as f64;
            assert!(payloads.iter().all(|&p| p != 0));
            let aligned_4k = payloads.iter().filter(|&&p| (p - 16) % 4096 == 0).count();
            let aligned_16k = payloads.iter().filter(|&&p| (p - 16) % 16384 == 0).count();
            let rounds = (MISSES / count).max(1);
            let hit = per_item(rounds * count, || {
                let mut live = 0;
                for _ in 0..rounds {
                    live += payloads.iter().filter(|&&p| ctx.is_live(p)).count();
                }
                live
            });
            let zeros = vec![0usize; MISSES];
            let small: Vec<usize> = (1..=MISSES).map(|i| i * 8).collect();
            let mut misses = Vec::new();
            for set in [&zeros, &small, &blocks] {
                misses.push(per_item(MISSES, || {
                    set.iter().filter(|&&p| ctx.is_live(p)).count()
                }));
            }
            let start = Instant::now();
            for &payload in &payloads {
                ctx.delete(payload, 0);
            }
            let delete_ns = start.elapsed().as_nanos() as f64 / count as f64;
            println!(
                "LARGE size={size} n={count} aligned_4k={aligned_4k} aligned_16k={aligned_16k} hit_ns={hit:.2} miss_zero_ns={:.2} miss_small_ns={:.2} miss_block_ns={:.2} alloc_ns={alloc_ns:.0} delete_ns={delete_ns:.0}",
                misses[0], misses[1], misses[2],
            );
        }
    }
}
