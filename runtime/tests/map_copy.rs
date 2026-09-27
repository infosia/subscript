use subscript_runtime::{ffi::*, Context};

#[test]
fn map_copy_preserves_order_and_reference_values_with_independent_storage() {
    let mut context = Context::new();
    let ctx = &mut *context as *mut Context;
    let object = context.alloc(4, 0, 0);
    assert!(!object.is_null());
    // SAFETY: all maps have i32 keys and pointer-width values; all buffers have those widths.
    unsafe {
        let source = subscript_rt_map_new(ctx, 4, std::mem::size_of::<*mut u8>() as u64, 0, 0);
        for key in [3i32, 1, 2] {
            subscript_rt_map_set(
                ctx,
                source,
                (&raw const key).cast(),
                (&raw const object).cast(),
                0,
            );
        }
        let removed = 1i32;
        subscript_rt_assoc_delete(ctx, source, (&raw const removed).cast());
        let copy = subscript_rt_map_from_assoc(ctx, source, 0);
        assert!(!copy.is_null());
        assert_ne!(copy, source);
        let mut keys = Vec::new();
        let bound = subscript_rt_assoc_iter_begin(ctx, copy, 0);
        for index in 0..bound {
            let mut key = 0i32;
            if subscript_rt_assoc_iter_copy(ctx, copy, index, 0, (&raw mut key).cast(), 0) != 0 {
                keys.push(key);
            }
        }
        subscript_rt_assoc_iter_end(ctx, copy);
        assert_eq!(keys, [3, 2]);
        let key = 3i32;
        let mut copied: *mut u8 = std::ptr::null_mut();
        assert_eq!(
            subscript_rt_map_get(ctx, copy, (&raw const key).cast(), (&raw mut copied).cast()),
            1
        );
        assert_eq!(copied, object);
        subscript_rt_assoc_delete(ctx, source, (&raw const key).cast());
        assert_eq!(
            subscript_rt_assoc_has(ctx, copy, (&raw const key).cast()),
            1
        );
        subscript_rt_map_set(
            ctx,
            copy,
            (&raw const removed).cast(),
            (&raw const object).cast(),
            0,
        );
        assert_eq!(
            subscript_rt_assoc_has(ctx, source, (&raw const removed).cast()),
            0
        );
        subscript_rt_assoc_clear(ctx, copy);
        assert_eq!(subscript_rt_assoc_size(ctx, source), 1);
    }
    assert!(context.trap_record().is_none());
}

#[test]
fn empty_map_copy_is_fresh_and_empty() {
    let mut context = Context::new();
    let ctx = &mut *context as *mut Context;
    // SAFETY: source is a live Map in this Context.
    unsafe {
        let source = subscript_rt_map_new(ctx, 4, 4, 0, 0);
        let copy = subscript_rt_map_from_assoc(ctx, source, 0);
        assert!(!copy.is_null());
        assert_ne!(source, copy);
        assert_eq!(subscript_rt_assoc_size(ctx, copy), 0);
    }
    assert!(context.trap_record().is_none());
}

#[test]
fn map_copy_reports_allocation_failure() {
    for fail_after in [1, 2, 3] {
        let mut context = Context::new();
        let ctx = &mut *context as *mut Context;
        // SAFETY: key and value buffers match the live Map's widths.
        unsafe {
            let source = subscript_rt_map_new(ctx, 4, 4, 0, 0);
            let key = 1i32;
            let value = 7i32;
            subscript_rt_map_set(
                ctx,
                source,
                (&raw const key).cast(),
                (&raw const value).cast(),
                0,
            );
            context.fail_alloc_after(fail_after);
            subscript_rt_map_from_assoc(ctx, source, 19);
        }
        assert_eq!(
            context.trap_record().unwrap().kind,
            subscript_runtime::TrapKind::AllocationFailure
        );
    }
}
