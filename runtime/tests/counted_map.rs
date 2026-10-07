//! Map stores receive acquired owners from the lowering (§172 rule 1).

use subscript_runtime::{context::CLASS_GENERATOR, ffi::*, Context};

const HANDLE: [u64; 3] = [1, 0, 0];

fn completed(ctx: &mut Context) -> *mut u8 {
    let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
    // SAFETY: the allocation holds an async header and returns no payload.
    unsafe {
        subscript_rt_async_register_uncounted(ctx, frame, 0);
        ctx.async_complete(frame, std::ptr::null(), 0);
    }
    frame
}

#[test]
fn described_map_replacement_delete_clear_copy_and_free_release_each_owner() {
    for store in [false, true] {
        let mut context = Context::new();
        let first = completed(&mut context);
        let second = completed(&mut context);
        // SAFETY: each Map has i32 keys and handle values. HANDLE outlives the Context.
        unsafe {
            let ctx = &mut *context as *mut Context;
            let map = subscript_rt_map_new(ctx, 4, 8, 0, 0);
            subscript_rt_map_describe(ctx, map, HANDLE.as_ptr().cast());
            let key = 1i32;
            if store {
                context.async_retain(first);
                subscript_rt_map_set(
                    ctx,
                    map,
                    (&raw const key).cast(),
                    (&raw const first).cast(),
                    0,
                );
                assert_eq!(context.async_count(first), 2);
                context.async_retain(first);
                subscript_rt_map_set(
                    ctx,
                    map,
                    (&raw const key).cast(),
                    (&raw const first).cast(),
                    0,
                );
                assert_eq!(context.async_count(first), 2);
                context.async_retain(second);
                subscript_rt_map_set(
                    ctx,
                    map,
                    (&raw const key).cast(),
                    (&raw const second).cast(),
                    0,
                );
                assert_eq!(context.async_count(first), 1);
                assert_eq!(context.async_count(second), 2);
                let copy = subscript_rt_map_from_assoc(ctx, map, 0);
                assert_eq!(context.async_count(second), 3);
                assert_eq!(
                    subscript_rt_assoc_delete(ctx, map, (&raw const key).cast()),
                    1
                );
                assert_eq!(
                    subscript_rt_assoc_delete(ctx, map, (&raw const key).cast()),
                    0
                );
                assert_eq!(context.async_count(second), 2);
                subscript_rt_assoc_clear(ctx, copy);
                subscript_rt_assoc_clear(ctx, copy);
                assert_eq!(context.async_count(second), 1);
                // Growth and compaction move owners without a count change.
                for key in 0i32..100 {
                    context.async_retain(second);
                    subscript_rt_map_set(
                        ctx,
                        map,
                        (&raw const key).cast(),
                        (&raw const second).cast(),
                        0,
                    );
                }
                assert_eq!(context.async_count(second), 101);
                context.delete(map as usize, 0);
                assert_eq!(context.async_count(second), 1);
                context.delete(copy as usize, 0);
            } else {
                assert_eq!(context.async_count(first), 1);
                assert_eq!(context.async_count(second), 1);
                context.delete(map as usize, 0);
            }
            context.async_release(first, 0);
            context.async_release(second, 0);
        }
        assert_eq!(context.live_count(), 0);
        assert!(!context.trapped());
    }
}

#[test]
fn map_clear_releases_all_nested_values_after_the_first_exception() {
    const ARRAY: [u64; 6] = [2, 0, 0, 1, 0, 0];
    for fail in [false, true] {
        let mut context = Context::new();
        let mut frames = Vec::new();
        // SAFETY: each Map slot stores an array pointer, and each array slot stores a handle.
        unsafe {
            let ctx = &mut *context as *mut Context;
            let map = subscript_rt_map_new(ctx, 4, 8, 0, 0);
            subscript_rt_map_describe(ctx, map, ARRAY.as_ptr().cast());
            for key in 0i32..2 {
                let array = subscript_rt_array_new(ctx, 8, 0);
                for _ in 0..2 {
                    let frame = context.alloc(16, CLASS_GENERATOR, 0);
                    subscript_rt_async_register_uncounted(ctx, frame, 0);
                    if fail {
                        let error = context.alloc(24, 1, 0);
                        context.raise_exception(error, "Error: lost".into(), 7);
                        context.async_complete_exception(frame);
                    } else {
                        context.async_complete(frame, std::ptr::null(), 0);
                    }
                    frames.push(frame);
                    subscript_rt_array_push(ctx, array, (&raw const frame).cast(), 0);
                }
                subscript_rt_map_set(
                    ctx,
                    map,
                    (&raw const key).cast(),
                    (&raw const array).cast(),
                    0,
                );
            }
            subscript_rt_assoc_clear(ctx, map);
            context.delete(map as usize, 0);
        }
        for frame in frames {
            assert!(!context.is_live(frame as usize));
        }
        assert_eq!(context.trapped(), fail);
        if fail {
            assert_eq!(context.trap_record().expect("first failure").pos_id, 7);
        } else {
            assert_eq!(context.live_count(), 0);
        }
    }
}

#[test]
fn failed_map_copy_acquires_only_the_values_that_it_stores() {
    for fail_after in [None, Some(1), Some(2), Some(3)] {
        let mut context = Context::new();
        let frame = completed(&mut context);
        // SAFETY: the source transfers the frame's initial owner into one handle slot.
        unsafe {
            let ctx = &mut *context as *mut Context;
            let source = subscript_rt_map_new(ctx, 4, 8, 0, 0);
            subscript_rt_map_describe(ctx, source, HANDLE.as_ptr().cast());
            let key = 1i32;
            subscript_rt_map_set(
                ctx,
                source,
                (&raw const key).cast(),
                (&raw const frame).cast(),
                0,
            );
            if let Some(limit) = fail_after {
                context.fail_alloc_after(limit);
            }
            let copy = subscript_rt_map_from_assoc(ctx, source, 19);
            let expected = if fail_after.is_some() { 1 } else { 2 };
            assert_eq!(context.async_count(frame), expected);
            assert_eq!(context.trapped(), fail_after.is_some());
            if !copy.is_null() {
                context.delete(copy as usize, 0);
            }
            assert_eq!(context.async_count(frame), 1);
            context.delete(source as usize, 0);
        }
        assert_eq!(context.live_count(), 0);
    }
}
