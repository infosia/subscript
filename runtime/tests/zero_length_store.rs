//! A clear releases elements in index order and keeps the array holder (§174).

use subscript_runtime::{context::CLASS_GENERATOR, ffi, Context, TrapKind};

fn task(ctx: &mut Context) -> *mut u8 {
    let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
    // SAFETY: fresh frame storage and an empty completion.
    unsafe {
        ctx.async_register(frame, 0);
        ctx.async_complete(frame, std::ptr::null(), 0);
    }
    frame
}

#[test]
fn clear_and_discarded_pop_controls_release_recursive_elements() {
    for depth in 0..3 {
        for clear in [false, true] {
            let mut ctx = Context::new();
            let frames = [task(&mut ctx), task(&mut ctx)];
            let array = ctx.array_new(8, 0);
            let mut description = vec![1u64, 0, 0];
            for _ in 0..depth {
                description.splice(0..0, [2, 0, 0]);
            }
            for frame in frames {
                let mut value = frame;
                for _ in 0..depth {
                    let inner = ctx.array_new(8, 0);
                    // SAFETY: the new array takes the value's initial owner.
                    unsafe {
                        ctx.array_push(inner, (&raw const value).cast(), 0);
                    }
                    value = inner;
                }
                // SAFETY: the outer array takes the element's initial owner.
                unsafe {
                    ctx.array_push(array, (&raw const value).cast(), 0);
                }
            }
            // Keep a second holder across the mutation.
            unsafe {
                ffi::subscript_rt_array_holder(&mut *ctx, array, 0, 0);
            }
            if clear {
                unsafe {
                    ffi::subscript_rt_counted_array_operation(
                        &mut *ctx,
                        array,
                        description.as_ptr().cast(),
                        3,
                        std::ptr::null(),
                        0,
                        0,
                        0,
                        17,
                    );
                }
            } else {
                for _ in 0..2 {
                    let mut removed = std::ptr::null_mut::<u8>();
                    unsafe {
                        ctx.array_pop(array, (&raw mut removed).cast(), 17);
                        ffi::subscript_rt_counted_value(
                            &mut *ctx,
                            (&raw const removed).cast(),
                            description.as_ptr().cast(),
                            1,
                            17,
                        );
                    }
                }
            }
            assert_eq!(unsafe { ctx.array_len(array) }, 0);
            assert_eq!(unsafe { array.add(32).cast::<u32>().read() }, 2);
            assert!(ctx.is_live(array as usize));
            for frame in frames {
                assert!(!ctx.is_live(frame as usize));
            }
            assert!(!ctx.trapped());
            let mut replacement = task(&mut ctx);
            for _ in 0..depth {
                let inner = ctx.array_new(8, 0);
                unsafe {
                    ctx.array_push(inner, (&raw const replacement).cast(), 0);
                }
                replacement = inner;
            }
            // SAFETY: use the empty array again with its original element type.
            unsafe {
                ctx.array_push(array, (&raw const replacement).cast(), 0);
            }
            assert_eq!(unsafe { ctx.array_len(array) }, 1);
        }
    }
}

#[test]
fn uncounted_clear_handles_empty_storage_and_reuse() {
    let mut ctx = Context::new();
    let array = ctx.array_new(4, 0);
    for fill in [false, true, false] {
        if fill {
            unsafe {
                let value = 7i32;
                ctx.array_push(array, (&raw const value).cast(), 0);
            }
        }
        unsafe {
            ffi::subscript_rt_counted_array_operation(
                &mut *ctx,
                array,
                std::ptr::null(),
                3,
                std::ptr::null(),
                0,
                0,
                0,
                17,
            );
        }
        assert_eq!(unsafe { ctx.array_len(array) }, 0);
        assert!(ctx.is_live(array as usize));
        assert!(!ctx.trapped());
    }
}

#[test]
fn clear_releases_in_index_order_and_stops_at_the_first_trap() {
    let mut ctx = Context::new();
    let array = ctx.array_new(8, 0);
    let mut frames = Vec::new();
    for (message, position) in [("first", 23), ("second", 29)] {
        let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
        unsafe {
            ctx.async_register(frame, 0);
        }
        ctx.raise_exception(std::ptr::null_mut(), message.into(), position);
        ctx.async_complete_exception(frame);
        unsafe {
            ctx.array_push(array, (&raw const frame).cast(), 0);
        }
        frames.push(frame);
    }
    unsafe {
        ffi::subscript_rt_counted_array_operation(
            &mut *ctx,
            array,
            [1u64, 0, 0].as_ptr().cast(),
            3,
            std::ptr::null(),
            0,
            0,
            0,
            17,
        );
    }
    let trap = ctx.trap_record().expect("unobserved exception");
    assert_eq!(trap.kind, TrapKind::UncaughtException);
    assert_eq!(trap.message, "first");
    assert_eq!(trap.pos_id, 23);
    assert!(!ctx.is_live(frames[0] as usize));
    assert!(ctx.is_live(frames[1] as usize));
}
