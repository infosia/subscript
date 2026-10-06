//! Mutable array aliases share one element owner (§171).

use subscript_runtime::{
    context::CLASS_GENERATOR,
    ffi::{subscript_rt_async_release_array, subscript_rt_async_retain_array},
    Context,
};

#[test]
fn array_aliases_release_the_transferred_element_and_the_last_holder() {
    for alias in [false, true] {
        let mut ctx = Context::new();
        let payload = ctx.alloc(16, CLASS_GENERATOR, 0);
        let array = ctx.array_with_capacity(1, std::mem::size_of::<*mut u8>(), 0);
        // SAFETY: both fresh allocations belong to this Context. The pointer fits the array element.
        unsafe {
            ctx.async_register(payload, 0);
            ctx.async_complete(payload, std::ptr::null(), 0);
            assert_eq!(
                ctx.array_push(array, (&payload as *const *mut u8).cast(), 0),
                1
            );
        }
        if alias {
            // SAFETY: the synchronous parameter copies a live array of registered handles.
            unsafe { subscript_rt_async_retain_array(&mut *ctx, array) };
        }
        assert_eq!(unsafe { ctx.async_count(payload) }, 1);
        let mut popped = std::ptr::null_mut::<u8>();
        // SAFETY: the nonempty array stores one pointer, and the output has that size.
        unsafe {
            ctx.array_pop(array, (&mut popped as *mut *mut u8).cast(), 0);
            assert_eq!(popped, payload);
            // A discarded pop releases the transferred element count.
            ctx.async_release(popped, 0);
            subscript_rt_async_release_array(&mut *ctx, array, 0);
            if alias {
                subscript_rt_async_release_array(&mut *ctx, array, 0);
            }
        }
        assert!(!ctx.is_live(array as usize));
        assert!(!ctx.is_live(payload as usize));
        assert!(!ctx.trapped());
    }
}
