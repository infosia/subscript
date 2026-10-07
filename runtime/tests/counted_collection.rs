//! Collection releases all holders before sweep, in task id order (§172).

use subscript_runtime::{context::CLASS_GENERATOR, ffi::*, Context, TrapKind};

unsafe fn field(ctx: &mut Context, frame: *mut u8) -> *mut u8 {
    let object = ctx.alloc(8, 900, 0);
    let description: [u64; 6] = [1, 0, 24, 1, 0, 0];
    // SAFETY: one handle field fits the object and owns the supplied count.
    unsafe {
        object.cast::<*mut u8>().write(frame);
        ctx.describe_object(
            object as usize,
            std::slice::from_raw_parts(description.as_ptr().cast(), 48),
        );
    }
    object
}

#[test]
fn collection_releases_failed_fields_and_maps_in_task_id_order_with_a_fulfilled_control() {
    for ship in [false, true] {
        for fail in [false, true] {
            for script in [false, true] {
                let mut ctx = if ship {
                    Context::new_releasing()
                } else {
                    Context::new()
                };
                let mut frames = Vec::new();
                // SAFETY: every registered frame has a complete header and one holder count.
                unsafe {
                    for index in 0..3 {
                        let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
                        ctx.async_register(frame, 0);
                        if fail {
                            let error = ctx.alloc(24, 1, 0);
                            ctx.raise_exception(error, format!("Error: {index}"), 40 + index);
                            ctx.async_complete_exception(frame);
                        } else {
                            ctx.async_complete(frame, std::ptr::null(), 0);
                        }
                        frames.push(frame);
                    }
                    // Holder order differs from registration order. The first task belongs to a Map.
                    let _ = field(&mut ctx, frames[2]);
                    let map = subscript_rt_map_new(&mut *ctx, 4, 8, 1, 0);
                    let description: [u64; 3] = [1, 0, 0];
                    subscript_rt_map_describe(&mut *ctx, map, description.as_ptr().cast());
                    let key = 1i32;
                    subscript_rt_map_set(
                        &mut *ctx,
                        map,
                        (&raw const key).cast(),
                        (&raw const frames[0]).cast(),
                        0,
                    );
                    let _ = field(&mut ctx, frames[1]);
                    if script {
                        subscript_rt_collect_at(&mut *ctx, 99);
                    } else {
                        subscript_rt_ctx_collect(&mut *ctx);
                    }
                }
                assert!(frames.iter().all(|frame| !ctx.is_live(*frame as usize)));
                assert_eq!(ctx.trapped(), fail);
                if fail {
                    let trap = ctx.trap_record().expect("unobserved exception");
                    assert_eq!(trap.kind, TrapKind::UncaughtException);
                    assert_eq!(trap.message, "Error: 0");
                    assert_eq!(trap.pos_id, if script { 99 } else { 40 });
                    ctx.clear_trap();
                }
                ctx.collect();
                assert!(!ctx.trapped());
                assert_eq!(ctx.live_count(), 0);
            }
        }
    }
}

#[test]
fn a_foreign_registry_consumes_class_and_map_leaves_before_any_storage_is_freed() {
    for described in [false, true] {
        let mut ctx = Context::new();
        let object = unsafe { field(&mut ctx, 101usize as *mut u8) };
        let map = unsafe { subscript_rt_map_new(&mut *ctx, 4, 8, 1, 0) };
        let key = 1i32;
        let value = 202usize;
        let description: [u64; 3] = [1, 0, 0];
        // SAFETY: the Map owns a foreign registry key, and its value has a pointer-sized layout.
        unsafe {
            subscript_rt_map_set(
                &mut *ctx,
                map,
                (&raw const key).cast(),
                (&raw const value).cast(),
                0,
            );
            if described {
                ctx.describe_map(
                    map as usize,
                    std::slice::from_raw_parts(description.as_ptr().cast(), 24),
                );
            }
        }
        Context::with_collection(
            &mut *ctx,
            |ctx| ctx,
            |ctx, handles| {
                handles.sort_unstable();
                assert_eq!(
                    handles,
                    if described {
                        &[101, 202][..]
                    } else {
                        &[101][..]
                    }
                );
                assert!(ctx.is_live(object as usize));
                assert!(ctx.is_live(map as usize));
            },
        );
        assert_eq!(ctx.live_count(), 0);
        ctx.collect();
        assert!(!ctx.trapped());
    }
}

#[test]
fn the_last_field_release_keeps_an_unfinished_frame_until_completion() {
    for ship in [false, true] {
        for extra_owner in [false, true] {
            let mut ctx = if ship {
                Context::new_releasing()
            } else {
                Context::new()
            };
            let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
            // SAFETY: the live frame owns a void completion and the object owns its initial count.
            unsafe {
                ctx.async_register(frame, 0);
                if extra_owner {
                    ctx.async_retain(frame);
                }
                let object = field(&mut ctx, frame);
                ctx.collect();
                assert!(!ctx.is_live(object as usize));
                assert!(ctx.is_live(frame as usize));
                assert_eq!(ctx.async_count(frame), u32::from(extra_owner));
                ctx.collect();
                assert!(ctx.is_live(frame as usize));
                ctx.async_complete(frame, std::ptr::null(), 0);
                ctx.async_release(frame, 0);
                assert!(!ctx.is_live(frame as usize));
            }
            assert!(!ctx.trapped());
        }
    }
}

#[test]
fn explicit_free_and_sweep_never_release_a_field_twice() {
    for free_first in [false, true] {
        let mut ctx = Context::new();
        ctx.set_freed_handle_diagnostics(true, 0, usize::MAX);
        let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
        // SAFETY: the object owns one count and a separate control holder owns another.
        unsafe {
            ctx.async_register(frame, 0);
            ctx.async_retain(frame);
            ctx.async_complete(frame, std::ptr::null(), 0);
            let object = field(&mut ctx, frame);
            if free_first {
                ctx.delete(object as usize, 7);
            }
            ctx.collect();
            assert_eq!(ctx.async_count(frame), 1);
            if !free_first {
                ctx.delete(object as usize, 7);
                assert_eq!(
                    ctx.trap_record().map(|trap| trap.kind),
                    Some(TrapKind::DoubleDelete)
                );
                ctx.clear_trap();
            }
            ctx.collect();
            assert_eq!(ctx.async_count(frame), 1);
            ctx.async_release(frame, 0);
        }
        assert_eq!(ctx.live_count(), 0);
        assert!(!ctx.trapped());
    }
}

#[test]
fn collection_sweeps_on_consumer_early_return_and_unwind() {
    for unwind in [false, true] {
        let mut ctx = Context::new();
        let object = ctx.alloc(8, 900, 0);
        let mut root = object as usize;
        ctx.shadow_push((&raw mut root) as usize, 1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Context::with_collection(
                &mut *ctx,
                |ctx| ctx,
                |_, _| {
                    if unwind {
                        panic!("consumer stops");
                    }
                    Err::<(), _>("consumer stops")
                },
            )
        }));
        assert_eq!(result.is_err(), unwind);
        ctx.shadow_pop();
        ctx.collect();
        assert!(!ctx.is_live(object as usize));
    }
}

#[test]
fn reachable_words_follow_live_payloads_and_stop_at_scalar_storage() {
    for ship in [false, true] {
        let mut ctx = if ship {
            Context::new_releasing()
        } else {
            Context::new()
        };
        let parent = ctx.alloc(8, 900, 0);
        let child = ctx.alloc(8, 900, 0);
        // SAFETY: both allocations contain one initialized native word.
        unsafe {
            parent.cast::<usize>().write(child as usize);
            child.cast::<usize>().write(101);
        }
        let words = ctx.reachable_words(&[parent as usize]);
        assert!(words.contains(&(parent as usize)));
        assert!(words.contains(&(child as usize)));
        assert!(words.contains(&101));
        assert!(!ctx.reachable_words(&[]).contains(&101));
        let scalar = ctx.alloc(8, subscript_runtime::context::CLASS_STRING, 0);
        // SAFETY: the scalar payload contains one initialized native word.
        unsafe {
            scalar.cast::<usize>().write(202);
        }
        assert!(!ctx.reachable_words(&[scalar as usize]).contains(&202));
        let mut host_root = parent as usize;
        ctx.shadow_push((&raw mut host_root) as usize, 1);
        assert!(ctx.reachable_words(&[]).contains(&101));
        ctx.shadow_pop();
    }
}
