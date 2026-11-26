//! `Context.free` of a dynamic array (`compiler.md` §358).
//!
//! The tests call `subscript_rt_delete`, the entry that the development
//! tier and the ship tier call for `Context.free`. The interpreter calls
//! [`Context::free_value`], which that entry calls.
//!
//! Cost: the largest heap is the array of 1,000,000 `i32` of acceptance
//! 2, a 4 MB buffer, built once in each kind. The loop of acceptance 3
//! makes 512 buffers of 64 KiB in each of four runs. The file runs in
//! about 0.1 s in a release build.

use subscript_runtime::context::{ArrayElements, CollectionMode};
use subscript_runtime::ffi::subscript_rt_delete;
use subscript_runtime::{Context, TrapKind};

/// A Context of the dev kind or of the ship arena, with collection off.
fn context(ship: bool) -> Box<Context> {
    let mut ctx = if ship {
        Context::new_releasing()
    } else {
        Context::new()
    };
    assert!(ctx.set_collection(CollectionMode::Off, 0));
    ctx
}

/// `Context.free(array)` through the entry of the native tiers.
fn free(ctx: &mut Context, array: *mut u8) {
    // SAFETY: `ctx` is a live Context and `array` is a payload of it.
    unsafe { subscript_rt_delete(ctx, array, 7) };
}

/// The payload of the buffer of `array`: the fourth word of the header.
fn buffer(array: *mut u8) -> usize {
    // SAFETY: the tests read live array headers only.
    unsafe { array.cast::<usize>().add(3).read() }
}

/// `compiler.md` §358 rule 2 and acceptance 2: after the free of an array,
/// the live bytes and the requested bytes equal their values before the
/// array. One buffer is a size class, one is a large block, and one array
/// has no buffer.
#[test]
fn the_free_of_an_array_releases_the_header_and_the_buffer() {
    for ship in [false, true] {
        for count in [0usize, 16, 1_000_000] {
            let mut ctx = context(ship);
            let kept = ctx.alloc(24, 900, 0);
            assert!(!kept.is_null());
            let (live, requested) = (ctx.live_bytes(), ctx.requested_bytes());
            let array = ctx.array_with_capacity_of(count, 4, ArrayElements::NoHandle, 0);
            assert!(!array.is_null());
            assert_eq!(buffer(array) == 0, count == 0);
            assert!(ctx.live_bytes() >= live + count * 4, "ship {ship}, {count}");
            free(&mut ctx, array);
            assert!(!ctx.trapped(), "ship {ship}, {count}");
            assert_eq!(ctx.live_bytes(), live, "ship {ship}, {count}");
            assert_eq!(ctx.requested_bytes(), requested, "ship {ship}, {count}");
            assert!(!ctx.is_live(array as usize));
            assert!(ctx.is_live(kept as usize), "another block stays");
        }
    }
}

/// `compiler.md` §358 rule 2: an array that grew releases its current
/// buffer one time; the growth released each earlier buffer. With the
/// freed-handle diagnostics on, a second release of a block traps. Firing
/// control: a second release of the buffer after the free traps with
/// `DoubleDelete`.
#[test]
fn a_grown_array_releases_its_buffer_exactly_one_time() {
    for control in [false, true] {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
        assert!(ctx.set_collection(CollectionMode::Off, 0));
        let live = ctx.live_bytes();
        let array = ctx.array_new_of(4, ArrayElements::NoHandle, 0);
        let mut first = 0;
        for value in 0..100i32 {
            // SAFETY: `value` has 4 readable bytes.
            unsafe { ctx.array_push(array, (&raw const value).cast(), 0) };
            if value == 0 {
                first = buffer(array);
            }
        }
        let current = buffer(array);
        assert_ne!(first, current, "the array grew past its first capacity");
        assert!(!ctx.is_live(first), "the growth released the first buffer");
        free(&mut ctx, array);
        assert!(!ctx.trapped(), "{:?}", ctx.trap_record());
        assert_eq!(ctx.live_bytes(), live);
        assert!(!ctx.is_live(current));
        if control {
            ctx.delete(current, 3);
            let trap = ctx.trap_record().expect("the second release traps");
            assert_eq!(trap.kind, TrapKind::DoubleDelete);
        }
    }
}

/// `compiler.md` §358 rule 3: the free does not release an element.
/// Firing control: a free of the element itself releases it.
#[test]
fn the_free_of_an_array_keeps_its_elements() {
    for ship in [false, true] {
        for control in [false, true] {
            let mut ctx = context(ship);
            let element = ctx.alloc(16, 900, 0) as usize;
            let array = ctx.array_new_of(8, ArrayElements::CanHoldHandle, 0);
            // SAFETY: `element` has 8 readable bytes.
            unsafe { ctx.array_push(array, (&raw const element).cast(), 0) };
            free(&mut ctx, array);
            assert!(!ctx.is_live(array as usize));
            if control {
                free(&mut ctx, element as *mut u8);
                assert!(!ctx.trapped(), "ship {ship}");
                assert!(!ctx.is_live(element), "ship {ship}: the control");
            } else {
                assert!(ctx.is_live(element), "ship {ship}");
            }
        }
    }
}

/// `compiler.md` §358 rule 6: a free of an array while a native call
/// borrows it traps with `ArrayStorageBorrowed` and keeps the header and
/// the buffer. Control: the same free after the borrow ends releases
/// both blocks.
#[test]
fn a_free_during_a_native_borrow_keeps_both_blocks() {
    for ship in [false, true] {
        for borrowed in [true, false] {
            let mut ctx = context(ship);
            let array = ctx.array_with_capacity_of(4, 4, ArrayElements::NoHandle, 0);
            assert!(!array.is_null());
            let data = buffer(array);
            assert_ne!(data, 0);
            let mark = ctx.boundary_scope_mark();
            // SAFETY: `array` is a live array header of `ctx`.
            unsafe { ctx.boundary_borrow(array, true) };
            if !borrowed {
                ctx.boundary_scope_release(mark);
            }
            free(&mut ctx, array);
            let live = ctx.is_live(array as usize) && ctx.is_live(data);
            if borrowed {
                let trap = ctx.trap_record().expect("the free traps");
                assert_eq!(trap.kind, TrapKind::ArrayStorageBorrowed, "ship {ship}");
                assert!(live, "ship {ship}: both blocks stay");
            } else {
                assert!(!ctx.trapped(), "ship {ship}");
                assert!(!ctx.is_live(array as usize) && !ctx.is_live(data));
            }
        }
    }
}

/// `compiler.md` §358 rule 4 and acceptance 3, in the form of §346
/// acceptance 4: a loop that makes an array of 64 KiB and frees it, with a
/// headroom of 64 KiB, runs no automatic collection. Control: the same
/// loop with no free runs at least one. The header and the buffer request
/// 64 KiB together, so one array reaches the limit and does not pass it
/// (§346 rule 2).
#[test]
fn a_loop_that_frees_each_array_runs_no_automatic_collection() {
    const ROUNDS: usize = 512;
    const BYTES: usize = 64 << 10;
    let header = {
        let mut ctx = context(false);
        let array = ctx.array_new_of(4, ArrayElements::NoHandle, 0);
        assert!(!array.is_null());
        ctx.requested_bytes()
    };
    let elements = (BYTES - header) / 4;
    for ship in [false, true] {
        for release in [true, false] {
            let mut ctx = if ship {
                Context::new_releasing()
            } else {
                Context::new()
            };
            assert!(ctx.set_collection_step(0));
            assert!(ctx.set_collection(CollectionMode::Automatic, 64 << 10));
            for _ in 0..ROUNDS {
                let array = ctx.array_with_capacity_of(elements, 4, ArrayElements::NoHandle, 0);
                assert!(!array.is_null());
                if release {
                    free(&mut ctx, array);
                }
                // The poll of the loop back edge (`compiler.md` §342 rule 7).
                ctx.collect_poll();
            }
            let count = ctx.automatic_collection_count();
            if release {
                assert_eq!(count, 0, "ship {ship}");
            } else {
                assert!(count > 0, "ship {ship}: the control runs a collection");
            }
        }
    }
}
