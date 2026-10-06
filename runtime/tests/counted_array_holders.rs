//! Direct tests of the generated ownership ABI and the array payload (§171).

use subscript_runtime::{context::CLASS_GENERATOR, ffi, Context};

fn task(ctx: &mut Context) -> *mut u8 {
    let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
    // SAFETY: fresh async header and an empty completion.
    unsafe {
        ctx.async_register(frame, 0);
        ctx.async_complete(frame, std::ptr::null(), 0);
    }
    frame
}

unsafe fn count(ctx: &mut Context, value: *const u8, description: &[u64], release: bool) {
    // SAFETY: each test supplies matching live storage and a complete description.
    unsafe {
        ffi::subscript_rt_counted_value(
            ctx,
            value,
            description.as_ptr().cast(),
            u32::from(release),
            17,
        )
    };
}

#[test]
fn array_copies_change_only_the_holder_count_and_free_at_zero() {
    for (ship, direct) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut ctx = if ship {
            Context::new_releasing()
        } else {
            Context::new()
        };
        let frame = task(&mut ctx);
        let array = ctx.array_new(8, 0);
        // SAFETY: the array takes the frame's initial count.
        unsafe { ctx.array_push(array, (&frame as *const *mut u8).cast(), 0) };
        let data = unsafe { ctx.array_data(array) };
        let description = [2, 0, 0, 1, 0, 0];
        let value = (&array as *const *mut u8).cast();
        unsafe {
            assert_eq!(array.add(32).cast::<u32>().read(), 1);
            count(&mut ctx, value, &description, false);
            assert_eq!(array.add(32).cast::<u32>().read(), 2);
            assert_eq!(ctx.async_count(frame), 1);
            if direct {
                ffi::subscript_rt_async_release_array(&mut *ctx, array, 17);
            } else {
                count(&mut ctx, value, &description, true);
            }
            assert!(ctx.is_live(array as usize));
            assert!(ctx.is_live(frame as usize));
            if direct {
                ffi::subscript_rt_async_release_array(&mut *ctx, array, 17);
            } else {
                count(&mut ctx, value, &description, true);
            }
        }
        assert!(!ctx.is_live(array as usize));
        assert!(!ctx.is_live(data as usize));
        assert!(!ctx.is_live(frame as usize));
        assert!(!ctx.trapped());
    }
}

#[test]
fn nested_arrays_release_their_owned_elements_from_the_static_description() {
    let mut ctx = Context::new();
    let frame = task(&mut ctx);
    let inner = ctx.array_new(8, 0);
    let outer = ctx.array_new(8, 0);
    // SAFETY: each insertion transfers its initial owner.
    unsafe {
        ctx.array_push(inner, (&frame as *const *mut u8).cast(), 0);
        ctx.array_push(outer, (&inner as *const *mut u8).cast(), 0);
        count(
            &mut ctx,
            (&outer as *const *mut u8).cast(),
            &[2, 0, 0, 2, 0, 0, 1, 0, 0],
            true,
        );
    }
    assert!(!ctx.is_live(outer as usize));
    assert!(!ctx.is_live(inner as usize));
    assert!(!ctx.is_live(frame as usize));
}

#[test]
fn inline_fixed_array_and_iter_result_owners_acquire_and_release_elements() {
    let mut ctx = Context::new();
    let first = task(&mut ctx);
    let second = task(&mut ctx);
    // bool done, padding, then FixedArray<Promise<void>, 2>.
    let result = [0u64, first as u64, second as u64];
    let description = [4, 8, 1, 3, 8, 2, 1, 0, 0];
    unsafe {
        count(&mut ctx, result.as_ptr().cast(), &description, false);
        assert_eq!(ctx.async_count(first), 2);
        assert_eq!(ctx.async_count(second), 2);
        count(&mut ctx, result.as_ptr().cast(), &description, true);
        assert_eq!(ctx.async_count(first), 1);
        count(&mut ctx, result.as_ptr().cast(), &description, true);
    }
    assert!(!ctx.is_live(first as usize));
    assert!(!ctx.is_live(second as usize));
    let done = [1u64, 0, 0];
    unsafe { count(&mut ctx, done.as_ptr().cast(), &description, true) };
    assert!(!ctx.trapped());
}

#[test]
fn interpreter_holder_abi_separates_last_decrement_and_storage_free() {
    let mut ctx = Context::new();
    let array = ctx.array_with_capacity(1, 8, 0);
    let data = unsafe { ctx.array_data(array) };
    unsafe {
        assert_eq!(ffi::subscript_rt_array_holder(&mut *ctx, array, 0, 0), 0);
        assert_eq!(ffi::subscript_rt_array_holder(&mut *ctx, array, 1, 0), 0);
        assert_eq!(ffi::subscript_rt_array_holder(&mut *ctx, array, 1, 0), 1);
        assert!(ctx.is_live(array as usize));
        assert_eq!(ffi::subscript_rt_array_holder(&mut *ctx, array, 2, 0), 0);
        assert_eq!(
            ffi::subscript_rt_array_holder(&mut *ctx, std::ptr::null_mut(), 0, 0),
            0
        );
    }
    assert!(!ctx.is_live(array as usize));
    assert!(!ctx.is_live(data as usize));
}

#[test]
fn recursive_last_holder_release_reports_the_unobserved_exception() {
    use subscript_runtime::TrapKind;

    for shape in 0..3 {
        let mut ctx = Context::new();
        let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
        // SAFETY: the fresh allocation contains the async header.
        unsafe { ctx.async_register(frame, 0) };
        ctx.raise_exception(std::ptr::null_mut(), "Error: inner".into(), 23);
        ctx.async_complete_exception(frame);
        assert!(!ctx.trapped());
        let (value, description) = match shape {
            0 => {
                let array = ctx.array_new(8, 0);
                let outer = ctx.array_new(8, 0);
                unsafe {
                    ctx.array_push(array, (&frame as *const *mut u8).cast(), 0);
                    ctx.array_push(outer, (&array as *const *mut u8).cast(), 0);
                }
                (vec![outer as u64], vec![2, 0, 0, 2, 0, 0, 1, 0, 0])
            }
            1 => (vec![frame as u64], vec![3, 8, 1, 1, 0, 0]),
            _ => (vec![0, frame as u64], vec![4, 8, 1, 1, 0, 0]),
        };
        unsafe { count(&mut ctx, value.as_ptr().cast(), &description, true) };
        let trap = ctx.trap_record().expect("the last release traps");
        assert_eq!(trap.kind, TrapKind::UncaughtException);
        assert_eq!(trap.message, "Error: inner");
        assert_eq!(trap.pos_id, 23);
        assert!(!ctx.is_live(frame as usize));
    }
}

#[test]
fn copied_elements_fill_and_overlapping_copy_move_element_counts() {
    let mut ctx = Context::new();
    let first = task(&mut ctx);
    let second = task(&mut ctx);
    let array = ctx.array_new(8, 0);
    let element = [1u64, 0, 0];
    unsafe {
        ctx.array_push(array, (&raw const first).cast(), 0);
        ctx.array_push(array, (&raw const second).cast(), 0);
        // A copied array needs element owners, without a source-holder copy.
        ffi::subscript_rt_counted_array_operation(
            &mut *ctx,
            array,
            element.as_ptr().cast(),
            0,
            std::ptr::null(),
            0,
            0,
            0,
            21,
        );
        assert_eq!(ctx.async_count(first), 2);
        assert_eq!(ctx.async_count(second), 2);
        // Release the extra element owners from the copied-element operation.
        count(&mut ctx, (&raw const first).cast(), &element, true);
        count(&mut ctx, (&raw const second).cast(), &element, true);
        ffi::subscript_rt_counted_array_operation(
            &mut *ctx,
            array,
            element.as_ptr().cast(),
            2,
            std::ptr::null(),
            0,
            1,
            i32::MAX,
            22,
        );
        assert!(!ctx.is_live(first as usize));
        assert_eq!(ctx.async_count(second), 2);
        // The fill source aliases a destination slot.
        let value = ctx.array_data(array);
        ffi::subscript_rt_counted_array_operation(
            &mut *ctx,
            array,
            element.as_ptr().cast(),
            1,
            value,
            0,
            -2,
            i32::MAX,
            23,
        );
        assert_eq!(ctx.async_count(second), 2);
        count(
            &mut ctx,
            (&raw const array).cast(),
            &[2, 0, 0, 1, 0, 0],
            true,
        );
    }
    assert!(!ctx.is_live(second as usize));
    assert!(!ctx.is_live(array as usize));
    assert!(!ctx.trapped());
}
