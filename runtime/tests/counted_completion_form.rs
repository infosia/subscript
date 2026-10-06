//! Rule 15 excludes TaskGroup from this completion defect.
//! This test passes when completion release leaves its counted payload live.

use subscript_runtime::{context::CLASS_GENERATOR, Context};

fn frame(ctx: &mut Context, size: usize) -> *mut u8 {
    let handle = ctx.alloc(16, CLASS_GENERATOR, 0);
    // SAFETY: the fresh allocation holds a complete async header.
    unsafe { ctx.async_register(handle, size) };
    handle
}

#[test]
fn completion_release_does_not_release_a_counted_payload() {
    for cached in [false, true] {
        let mut ctx = Context::new();
        let payload = frame(&mut ctx, 0);
        // SAFETY: the payload completes with an empty value.
        unsafe { ctx.async_complete(payload, std::ptr::null(), 0) };
        let owner = frame(&mut ctx, std::mem::size_of::<*mut u8>());
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
            // SAFETY: the cache contains one pointer-sized value.
            assert!(unsafe {
                ctx.async_result(
                    owner,
                    (&mut result as *mut *mut u8).cast(),
                    std::mem::size_of::<*mut u8>(),
                )
            });
            assert_eq!(result, payload);
            // A local copy takes a count and releases it at lexical exit.
            unsafe { ctx.async_retain(result) };
            assert_eq!(unsafe { ctx.async_count(payload) }, 2);
            unsafe { ctx.async_release(result, 0) };
        } else {
            // The control transfers directly to a local, with no completion cache.
            unsafe { ctx.async_release(payload, 0) };
        }
        // SAFETY: the caller owns the parent's initial count.
        unsafe { ctx.async_release(owner, 0) };
        assert!(!ctx.is_live(owner as usize));
        assert_eq!(ctx.is_live(payload as usize), cached);
        if cached {
            assert_eq!(unsafe { ctx.async_count(payload) }, 1);
            // Release the orphan count so the measurement leaves no live frame.
            unsafe { ctx.async_release(payload, 0) };
        }
        assert!(!ctx.is_live(payload as usize));
        assert!(!ctx.trapped());
    }
}
