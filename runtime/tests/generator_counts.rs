//! Generator releases keep runtime cleanup at one and at zero (§314).
//! Cost: six native frame cases; measured debug suite: under 0.01 s.
use subscript_runtime::{context::CLASS_GENERATOR, ffi::*, generator_layout::*, Context};

fn frame(ctx: &mut Context, holders: u32) -> *mut u8 {
    let frame = ctx.alloc(PAYLOAD_OFFSET as usize, CLASS_GENERATOR, 0);
    let marker = [0u64];
    // SAFETY: the live allocation holds the native header. The marker has one description word.
    unsafe {
        frame
            .add(HOLDERS_OFFSET as usize)
            .cast::<u32>()
            .write(holders);
        frame
            .add(CLEANUP_OFFSET as usize)
            .cast::<*const u8>()
            .write(std::ptr::null());
        subscript_rt_object_describe(&mut *ctx, frame, marker.as_ptr().cast(), 8);
    }
    frame
}

#[test]
fn last_generator_holder_frees_the_frame() {
    for ship in [false, true] {
        let mut ctx = if ship {
            Context::new_releasing()
        } else {
            Context::new()
        };
        let frame = frame(&mut ctx, 1);
        // SAFETY: this release owns the live generator's last count.
        unsafe { ctx.async_release(frame, 17) };
        assert!(!ctx.is_live(frame as usize));
        assert!(!ctx.trapped());
    }
}

#[test]
fn two_generator_holders_free_only_after_the_second_release() {
    for ship in [false, true] {
        let mut ctx = if ship {
            Context::new_releasing()
        } else {
            Context::new()
        };
        let frame = frame(&mut ctx, 1);
        // SAFETY: the registered generator remains live until the second release.
        unsafe {
            ctx.async_retain(frame);
            ctx.async_release(frame, 17);
            assert!(ctx.is_live(frame as usize));
            assert_eq!(frame.add(HOLDERS_OFFSET as usize).cast::<u32>().read(), 1);
            ctx.async_release(frame, 18);
        }
        assert!(!ctx.is_live(frame as usize));
        assert!(!ctx.trapped());
    }
}

#[test]
fn zero_generator_holders_keep_runtime_cleanup() {
    for ship in [false, true] {
        let mut ctx = if ship {
            Context::new_releasing()
        } else {
            Context::new()
        };
        let frame = frame(&mut ctx, 0);
        // SAFETY: the live registered frame has zero holders, as the runtime slow path permits.
        unsafe { ctx.async_release(frame, 19) };
        assert!(!ctx.is_live(frame as usize));
        assert!(!ctx.trapped());
    }
}
