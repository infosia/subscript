//! §109.0 "Memory, precisely": the quota charges the bytes the allocator
//! reserves for an allocation, so the process memory a full Context
//! attributes to itself is the quota plus a fixed overhead, whatever the
//! allocation sizes.
//!
//! The program fills a Context to the quota with 8-byte objects, and
//! separately with 4,096-byte objects, in both memory modes, and reports
//! the bytes the process holds at the trap.
//!
//! Resident bytes have no portable reader in a test, so the counting
//! global allocator of `quota_peak.rs` reports the bytes instead: the
//! sum of the live layout sizes the process asked the system for. The
//! value excludes the system allocator's own rounding and metadata.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use subscript_runtime::context::CLASS_STRING;
use subscript_runtime::{Context, TrapKind};

/// Live bytes the process holds, as the allocator sees them.
static LIVE: AtomicUsize = AtomicUsize::new(0);

/// The system allocator with a live-bytes counter.
struct Counting;

// SAFETY: every method forwards to the system allocator with the same
// pointer and layout it was given; the counter adds no requirement.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded caller contract.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: forwarded caller contract.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded caller contract.
        let fresh = unsafe { System.realloc(pointer, layout, new_size) };
        if !fresh.is_null() {
            LIVE.fetch_add(new_size, Ordering::Relaxed);
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        fresh
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded caller contract.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The counter is process-wide, so one test at a time reads it.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// The quota every fill runs under, in bytes (64 MiB).
const QUOTA: u64 = 67_108_864;

/// The bytes a full Context can hold above the quota.
///
/// Twice the largest overhead the four fills measure: 2,573,784 bytes,
/// for 8-byte objects in the exact-size mode.
const SLACK: usize = 5_147_568;

/// The object sizes the fills use, in bytes.
const OBJECT_SIZES: [usize; 2] = [8, 4096];

/// What one fill of a Context to the quota held.
struct Fill {
    mode: &'static str,
    object: usize,
    /// Bytes the process held for this Context when the quota stopped it.
    held: usize,
    /// Live allocations at the trap.
    allocations: usize,
    /// `Context::live_bytes` at the trap.
    live_bytes: usize,
    trap: Option<TrapKind>,
}

impl Fill {
    /// The bytes held divided by the quota.
    fn ratio(&self) -> f64 {
        self.held as f64 / QUOTA as f64
    }

    /// The reason this fill breaks §109.0 "Memory, precisely", if it does.
    fn violation(&self) -> Option<String> {
        let (mode, object) = (self.mode, self.object);
        if self.trap != Some(TrapKind::AllocationQuota) {
            return Some(format!(
                "{mode}/{object}-byte objects: the fill recorded {:?}, not the quota trap",
                self.trap
            ));
        }
        // The control for the bound below: a full Context holds most of
        // its quota, so a measurement that reads nothing fails here
        // instead of passing the upper bound.
        if self.held < QUOTA as usize / 2 {
            return Some(format!(
                "{mode}/{object}-byte objects: {} bytes held is under half the quota {QUOTA}",
                self.held
            ));
        }
        let bound = QUOTA as usize + SLACK;
        if self.held > bound {
            return Some(format!(
                "{mode}/{object}-byte objects: {} bytes held is over the quota {QUOTA} plus {SLACK}",
                self.held
            ));
        }
        None
    }
}

/// Fills one Context with `object`-byte allocations until the quota
/// stops it, and reports the bytes the process holds at that point.
fn fill(ship_arena: bool, object: usize) -> Fill {
    let mut ctx = if ship_arena {
        Context::new_releasing()
    } else {
        Context::new()
    };
    ctx.set_alloc_quota(QUOTA);
    let before = LIVE.load(Ordering::Relaxed);
    while !ctx.trapped() {
        if ctx.alloc(object, CLASS_STRING, 1).is_null() {
            break;
        }
    }
    let held = LIVE.load(Ordering::Relaxed).saturating_sub(before);
    Fill {
        mode: if ship_arena { "arena" } else { "exact-size" },
        object,
        held,
        allocations: ctx.live_count(),
        live_bytes: ctx.live_bytes(),
        trap: ctx.trap_record().map(|record| record.kind),
    }
}

/// Takes the process-wide counter lock, ignoring an earlier panic.
fn one_at_a_time() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn a_context_full_to_the_quota_holds_the_quota_plus_a_fixed_slack() {
    let _guard = one_at_a_time();
    let mut fills: Vec<Fill> = Vec::new();
    for ship_arena in [false, true] {
        for object in OBJECT_SIZES {
            fills.push(fill(ship_arena, object));
        }
    }
    println!("| mode | object | allocations | live_bytes | held | held/quota |");
    println!("|---|---|---|---|---|---|");
    for one in &fills {
        println!(
            "| {} | {} | {} | {} | {} | {:.2}x |",
            one.mode,
            one.object,
            one.allocations,
            one.live_bytes,
            one.held,
            one.ratio()
        );
    }
    let violations: Vec<String> = fills.iter().filter_map(Fill::violation).collect();
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}
