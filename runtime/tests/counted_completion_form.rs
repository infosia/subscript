//! A completion owns its counted payload and a counted read acquires a fresh owner.

use subscript_runtime::{context::CLASS_GENERATOR, Context};

fn frame(ctx: &mut Context, size: usize) -> *mut u8 {
    let handle = ctx.alloc(16, CLASS_GENERATOR, 0);
    // SAFETY: the fresh allocation holds a complete async header.
    unsafe {
        subscript_runtime::ffi::subscript_rt_async_register_uncounted(
            &mut *ctx,
            handle,
            size as u64,
        )
    };
    handle
}

#[test]
fn completion_release_frees_its_counted_payload() {
    for cached in [false, true] {
        let mut ctx = Context::new();
        let payload = frame(&mut ctx, 0);
        // SAFETY: the payload completes with an empty value.
        unsafe { ctx.async_complete(payload, std::ptr::null(), 0) };
        let owner = ctx.alloc(16, CLASS_GENERATOR, 0);
        let description = [1u64, 0, 0];
        // SAFETY: the description matches the result and outlives the frame.
        unsafe {
            subscript_runtime::ffi::subscript_rt_async_register(
                &mut *ctx,
                owner,
                std::mem::size_of::<*mut u8>() as u64,
                description.as_ptr().cast(),
            );
        }
        if cached {
            // SAFETY: this return transfers the payload's initial count to the cache.
            unsafe {
                ctx.async_complete(
                    owner,
                    (&payload as *const *mut u8).cast(),
                    std::mem::size_of::<*mut u8>(),
                );
            }
            let mut result = std::ptr::null_mut::<u8>();
            // SAFETY: the cache contains one pointer-sized value. A host read borrows it.
            assert!(unsafe { ctx.async_result(owner, (&mut result as *mut *mut u8).cast(), 8) });
            assert_eq!(unsafe { ctx.async_count(payload) }, 1);
            // The counted entry acquires the result that the local releases below.
            assert!(unsafe {
                subscript_runtime::ffi::subscript_rt_async_result_counted(
                    &mut *ctx,
                    owner,
                    (&mut result as *mut *mut u8).cast(),
                    std::mem::size_of::<*mut u8>() as u64,
                    description.as_ptr().cast(),
                ) == 1
            });
            assert_eq!(result, payload);
            // The completion read acquires the local's count.
            assert_eq!(unsafe { ctx.async_count(payload) }, 2);
            unsafe { ctx.async_release(result, 0) };
        } else {
            // The control transfers directly to a local, with no completion cache.
            unsafe {
                ctx.async_release(payload, 0);
                let empty: *mut u8 = std::ptr::null_mut();
                ctx.async_complete(owner, (&raw const empty).cast(), 8);
            };
        }
        // SAFETY: the caller owns the parent's initial count.
        unsafe { ctx.async_release(owner, 0) };
        assert!(!ctx.is_live(owner as usize));
        assert!(!ctx.is_live(payload as usize));
        assert!(!ctx.trapped());
    }
}

#[test]
fn emitted_completion_reads_use_the_supplied_count_action() {
    use subscript_runtime::ffi::{
        subscript_rt_async_result_counted, subscript_rt_async_result_uncounted,
    };
    let mut ctx = Context::new();
    let payload = frame(&mut ctx, 0);
    let owner = frame(&mut ctx, std::mem::size_of::<*mut u8>());
    let description = [1u64, 0, 0];
    let mut out = std::ptr::null_mut::<u8>();
    // SAFETY: the frames and result slots match the tested handle representation.
    unsafe {
        ctx.async_complete(payload, std::ptr::null(), 0);
        ctx.async_complete(owner, (&payload as *const *mut u8).cast(), 8);
        assert_eq!(
            subscript_rt_async_result_uncounted(
                &mut *ctx,
                owner,
                (&mut out as *mut *mut u8).cast(),
                8
            ),
            1
        );
        assert_eq!(out, payload);
        assert_eq!(ctx.async_count(payload), 1);
        assert_eq!(
            subscript_rt_async_result_counted(
                &mut *ctx,
                owner,
                (&mut out as *mut *mut u8).cast(),
                8,
                description.as_ptr().cast()
            ),
            1
        );
        assert_eq!(ctx.async_count(payload), 2);
        ctx.async_release(payload, 0);
        ctx.async_release(owner, 0);
        ctx.async_release(payload, 0);
    }
    assert!(!ctx.is_live(payload as usize));
    assert!(!ctx.is_live(owner as usize));
}
