//! The exception completion of an async handle (`compiler.md` §116).
//!
//! Each test drives Context-owned frames with the emitted coroutine header
//! (§70.2) through the same runtime entries that generated code calls: the
//! call-time start, the unwind exit, the await registration, the completion
//! read, the release, and the host checkpoint.

use super::*;
use crate::context::{AsyncResume, CLASS_GENERATOR};

/// The emitted coroutine header, followed by the test's own state.
#[repr(C)]
struct TestFrame {
    state: i32,
    count: u32,
    resume: AsyncResume,
    handle: *mut u8,
    polls: u32,
    caught: usize,
}

const THROW_POSITION: u32 = 21;

fn spawn(ctx: &mut Context, resume: AsyncResume, handle: *mut u8) -> *mut u8 {
    let frame = ctx.alloc(std::mem::size_of::<TestFrame>(), CLASS_GENERATOR, 0);
    // SAFETY: the allocation has exactly the test-frame payload.
    unsafe {
        frame.cast::<TestFrame>().write(TestFrame {
            state: 0,
            count: 0,
            resume,
            handle,
            polls: 0,
            caught: 0,
        });
        ctx.async_register(frame, 0);
    }
    frame
}

/// The body raises, and the resume function leaves through its unwind
/// exit, as generated code does (`compiler.md` §116.2 rule 2).
fn raise_and_unwind(context: &mut Context, frame: *mut u8) -> u8 {
    let object = context.alloc(24, 1, 0);
    context.raise_exception(object, "Error: body failed".to_string(), THROW_POSITION);
    // SAFETY: the test passes a live frame of this Context.
    unsafe { subscript_rt_async_complete_exception(context, frame) };
    1
}

/// Raises on its first resume.
unsafe extern "C" fn throwing_resume(ctx: *mut Context, frame: *mut u8, _out: *mut u8) -> u8 {
    // SAFETY: the scheduler and the tests pass the live Context.
    raise_and_unwind(unsafe { &mut *ctx }, frame)
}

/// Parks on its first resume and raises on the second, as a body that
/// throws after `await Context.suspend()` does.
unsafe extern "C" fn parked_throwing_resume(
    ctx: *mut Context,
    frame: *mut u8,
    _out: *mut u8,
) -> u8 {
    // SAFETY: the scheduler and the tests pass matching live values.
    let context = unsafe { &mut *ctx };
    let record = unsafe { &mut *frame.cast::<TestFrame>() };
    record.polls += 1;
    if record.polls == 1 {
        // SAFETY: the frame is registered in this Context.
        unsafe { context.async_park(frame, 0) };
        return 0;
    }
    raise_and_unwind(context, frame)
}

/// Parks on each resume and never completes.
unsafe extern "C" fn parking_resume(ctx: *mut Context, frame: *mut u8, _out: *mut u8) -> u8 {
    // SAFETY: the scheduler and the tests pass the live Context.
    unsafe { (*ctx).async_park(frame, 0) };
    0
}

/// Awaits its handle on the first resume. On the second resume it reads
/// the completion, catches the raised exception, and records the object.
unsafe extern "C" fn awaiting_resume(ctx: *mut Context, frame: *mut u8, _out: *mut u8) -> u8 {
    // SAFETY: the scheduler and the tests pass matching live values.
    let context = unsafe { &mut *ctx };
    let record = unsafe { &mut *frame.cast::<TestFrame>() };
    record.polls += 1;
    if record.polls == 1 {
        // SAFETY: both frames are registered in this Context.
        unsafe { context.async_await(frame, record.handle, 0) };
        return 0;
    }
    // SAFETY: the handle is registered; a zero size reads no bytes.
    let completed = unsafe { context.async_result(record.handle, std::ptr::null_mut(), 0) };
    if completed {
        // SAFETY: the completion read consumes the registration's handle count.
        unsafe { context.async_release(record.handle, 0) };
    }
    if completed && context.exception_pending() {
        record.caught = context.catch_exception() as usize;
    }
    1
}

fn completed_with_exception(ctx: &mut Context) -> (*mut u8, *mut u8) {
    let frame = spawn(ctx, throwing_resume, std::ptr::null_mut());
    let object = ctx.alloc(24, 1, 0);
    ctx.raise_exception(object, "TypeError: stored".to_string(), THROW_POSITION);
    ctx.async_complete_exception(frame);
    (frame, object)
}

#[test]
fn the_unwind_exit_completes_the_handle_and_clears_the_word() {
    let mut ctx = Context::new();
    let (frame, _) = completed_with_exception(&mut ctx);
    assert!(!ctx.trapped(), "the exception left the word");
    assert!(ctx.trap_record().is_none(), "no trap is recorded");
    assert_eq!(ctx.async_unfinished(), 0, "the handle has its completion");
    // A value completion after the exception does not replace it.
    let fulfilled = 9i32;
    // SAFETY: `fulfilled` is four readable bytes; the frame is live.
    unsafe { ctx.async_complete(frame, (&fulfilled as *const i32).cast(), 4) };
    let mut out = 0i32;
    // SAFETY: `out` is four writable bytes.
    assert!(unsafe { ctx.async_result(frame, (&mut out as *mut i32).cast(), 4) });
    assert!(
        ctx.exception_pending(),
        "the completion is still the exception"
    );
    assert_eq!(out, 0, "an exception completion writes no value");
}

#[test]
fn each_read_raises_the_same_object_text_and_position() {
    let mut ctx = Context::new();
    let (frame, object) = completed_with_exception(&mut ctx);
    for _ in 0..2 {
        let mut out = -1i32;
        // SAFETY: `out` is four writable bytes.
        assert!(unsafe { ctx.async_result(frame, (&mut out as *mut i32).cast(), 4) });
        assert_eq!(out, -1, "the read leaves the output unchanged");
        assert_eq!(ctx.pending_exception_object(), Some(object));
        ctx.settle_uncaught_exception();
        let record = ctx.trap_record().expect("the settled read");
        assert_eq!(
            (record.kind, record.message.as_str(), record.pos_id),
            (
                TrapKind::UncaughtException,
                "TypeError: stored",
                THROW_POSITION
            )
        );
        ctx.clear_trap();
    }
}

#[test]
fn the_last_release_of_an_unobserved_exception_traps() {
    let mut ctx = Context::new();
    let (frame, _) = completed_with_exception(&mut ctx);
    // SAFETY: the test owns the creation reference.
    unsafe { ctx.async_release(frame, 3) };
    assert!(!ctx.is_live(frame as usize), "the release frees the frame");
    let record = ctx.trap_record().expect("compiler.md §116.1 rule 4");
    assert_eq!(
        (record.kind, record.message.as_str(), record.pos_id),
        (
            TrapKind::UncaughtException,
            "TypeError: stored",
            THROW_POSITION
        )
    );

    // The controls: an observed exception and a value release quietly.
    let mut ctx = Context::new();
    let (frame, object) = completed_with_exception(&mut ctx);
    // SAFETY: a zero size reads no bytes.
    assert!(unsafe { ctx.async_result(frame, std::ptr::null_mut(), 0) });
    assert_eq!(ctx.catch_exception(), object);
    // SAFETY: the test owns the creation reference.
    unsafe { ctx.async_release(frame, 3) };
    assert!(ctx.trap_record().is_none(), "an observed exception");
    let value = spawn(&mut ctx, throwing_resume, std::ptr::null_mut());
    // SAFETY: a zero size reads no bytes; the test owns the reference.
    unsafe {
        ctx.async_complete(value, std::ptr::null(), 0);
        ctx.async_release(value, 3);
    }
    assert!(ctx.trap_record().is_none(), "a value completion");
}

#[test]
fn a_host_root_settles_and_a_trap_passes() {
    let mut ctx = Context::new();
    let root = spawn(&mut ctx, throwing_resume, std::ptr::null_mut());
    // SAFETY: the frame and its resume function match.
    unsafe { ctx.async_kick(root, throwing_resume) };
    let record = ctx.trap_record().expect("compiler.md §116.1 rule 5");
    assert_eq!(
        (record.kind, record.pos_id),
        (TrapKind::UncaughtException, THROW_POSITION)
    );
    assert_eq!(ctx.async_unfinished(), 1, "the root holds no completion");

    let mut ctx = Context::new();
    let frame = spawn(&mut ctx, throwing_resume, std::ptr::null_mut());
    ctx.trap(TrapKind::DivisionByZero, "integer division by zero", 4);
    ctx.async_complete_exception(frame);
    assert_eq!(
        ctx.trap_record().map(|record| record.kind),
        Some(TrapKind::DivisionByZero),
        "a trap passes the unwind exit unchanged"
    );
    assert_eq!(ctx.async_unfinished(), 1);

    let mut ctx = Context::new();
    let object = ctx.alloc(24, 1, 0);
    ctx.raise_exception(object, "Error: unregistered".to_string(), 8);
    ctx.async_complete_exception(object);
    assert_eq!(
        ctx.trap_record().map(|record| record.kind),
        Some(TrapKind::UncaughtException),
        "a frame the Context does not hold settles"
    );
}

#[test]
fn a_held_exception_is_a_collection_root_and_outlives_its_handle() {
    let mut ctx = Context::new();
    let (frame, object) = completed_with_exception(&mut ctx);
    ctx.collect();
    assert!(ctx.is_live(object as usize), "the handle roots the object");
    // SAFETY: a zero size reads no bytes.
    assert!(unsafe { ctx.async_result(frame, std::ptr::null_mut(), 0) });
    assert_eq!(ctx.catch_exception(), object);
    // SAFETY: the test owns the creation reference.
    unsafe { ctx.async_release(frame, 3) };
    assert!(
        ctx.is_live(object as usize),
        "compiler.md §116.1 rule 6: the release does not free the object"
    );
    ctx.collect();
    assert!(
        !ctx.is_live(object as usize),
        "the firing control: an object that nothing holds is collected"
    );
}

/// compiler.md §116.1 rule 2 and §94: an await of a handle completed with an
/// exception resumes once, raises, and leaves no pending frame in the queue.
#[test]
fn an_await_of_an_exception_completion_leaves_the_queue_empty() {
    let mut ctx = Context::new();
    let handle = spawn(&mut ctx, parked_throwing_resume, std::ptr::null_mut());
    // The call-time start runs the body to its first suspension.
    // SAFETY: the frame and its resume function match.
    assert_eq!(
        unsafe { parked_throwing_resume(&mut *ctx, handle, std::ptr::null_mut()) },
        0
    );
    let waiter = spawn(&mut ctx, awaiting_resume, handle);
    // The test holds the waiter, so its frame outlives the checkpoint.
    // SAFETY: the frame and its resume function match.
    unsafe {
        ctx.async_retain(waiter);
        ctx.async_kick(waiter, awaiting_resume);
    }
    assert_eq!((ctx.async_ready_len(), ctx.async_parked_len()), (0, 1));
    assert_eq!(ctx.async_unfinished(), 2);

    // SAFETY: every queued frame came through the generated-code entries.
    let pending = unsafe { ctx.async_step() };
    assert_eq!(pending, 0, "no frame waits after the checkpoint");
    assert!(!ctx.trapped(), "the waiter caught the exception");
    assert_eq!((ctx.async_ready_len(), ctx.async_parked_len()), (0, 0));
    assert_eq!(ctx.async_unfinished(), 0, "both frames completed");
    // SAFETY: the test's reference keeps the waiter frame live.
    let caught = unsafe { (*waiter.cast::<TestFrame>()).caught };
    assert_ne!(caught, 0, "the waiter caught the raised object");
    // SAFETY: the test owns these references.
    unsafe {
        ctx.async_release(waiter, 3);
        ctx.async_release(handle, 3);
    }
    assert!(
        ctx.trap_record().is_none(),
        "the await observed the exception"
    );
}

/// compiler.md §116.1 rule 4 at a checkpoint: a body the program no longer
/// holds raises after its suspension. The scheduler's release frees the
/// frame and traps, and the other ready work stays queued.
#[test]
fn a_checkpoint_release_of_an_unobserved_exception_traps() {
    let mut ctx = Context::new();
    let handle = spawn(&mut ctx, parked_throwing_resume, std::ptr::null_mut());
    // SAFETY: the frame and its resume function match.
    unsafe { parked_throwing_resume(&mut *ctx, handle, std::ptr::null_mut()) };
    // The program releases its only holder; the parked registration keeps
    // the frame.
    // SAFETY: the test owns the creation reference.
    unsafe { ctx.async_release(handle, 3) };
    assert!(ctx.is_live(handle as usize));
    let other = spawn(&mut ctx, parking_resume, std::ptr::null_mut());
    // SAFETY: the frame and its resume function match.
    unsafe { ctx.async_kick(other, parking_resume) };
    assert_eq!(ctx.async_parked_len(), 2);

    // SAFETY: every queued frame came through the generated-code entries.
    let pending = unsafe { ctx.async_step() };
    let record = ctx.trap_record().expect("the release traps");
    assert_eq!(
        (record.kind, record.message.as_str(), record.pos_id),
        (
            TrapKind::UncaughtException,
            "Error: body failed",
            THROW_POSITION
        )
    );
    assert!(!ctx.is_live(handle as usize), "the frame is freed");
    assert_eq!(pending, 1, "the other frame stays ready");
    assert_eq!(ctx.async_ready_len(), 1);
}

/// compiler.md §116.1 rule 4a: blocked and ready awaits hold one handle count.
#[test]
fn an_await_registration_holds_its_handle_until_the_completion_read() {
    for completed in [false, true] {
        let mut ctx = Context::new();
        let handle = spawn(&mut ctx, parked_throwing_resume, std::ptr::null_mut());
        if completed {
            raise_and_unwind(&mut ctx, handle);
        } else {
            // SAFETY: the frame and its resume function match.
            unsafe { parked_throwing_resume(&mut *ctx, handle, std::ptr::null_mut()) };
        }
        let waiter = spawn(&mut ctx, awaiting_resume, handle);
        // SAFETY: both frames belong to this Context; the test holds the waiter.
        unsafe {
            ctx.async_retain(waiter);
            ctx.async_kick(waiter, awaiting_resume);
            assert_eq!(
                (*handle.cast::<TestFrame>()).count,
                if completed { 2 } else { 3 }
            );
            ctx.async_release(handle, 3);
        }
        assert!(ctx.is_live(handle as usize));
        assert!(!ctx.trapped());
        // SAFETY: each queued frame has the matching resume pointer.
        assert_eq!(unsafe { ctx.async_step() }, 0);
        assert!(
            !ctx.trapped(),
            "the registered await observes the exception"
        );
        assert!(
            !ctx.is_live(handle as usize),
            "the completion read releases its count"
        );
        // SAFETY: the test's reference keeps the waiter live.
        unsafe {
            assert_eq!((*waiter.cast::<TestFrame>()).polls, 2);
            assert_ne!((*waiter.cast::<TestFrame>()).caught, 0);
            ctx.async_release(waiter, 3);
        }

        let mut control = Context::new();
        let handle = spawn(&mut control, parked_throwing_resume, std::ptr::null_mut());
        if completed {
            raise_and_unwind(&mut control, handle);
        } else {
            // SAFETY: the frame and its resume function match.
            unsafe { parked_throwing_resume(&mut *control, handle, std::ptr::null_mut()) };
        }
        // SAFETY: the control releases its sole script holder without an await.
        unsafe {
            control.async_release(handle, 3);
            control.async_step();
        }
        assert_eq!(
            control.trap_record().map(|trap| trap.kind),
            Some(TrapKind::UncaughtException)
        );
        assert!(!control.is_live(handle as usize));
    }
}

#[test]
fn async_completion_layout() {
    assert_eq!(std::mem::size_of::<crate::context::AsyncFrameMeta>(), 88);
    assert_eq!(std::mem::size_of::<Option<Completion>>(), 24);
}

#[test]
fn a_direct_await_moves_one_count_and_a_held_await_retains_one() {
    use crate::ffi::subscript_rt_async_await_owned;

    for owned in [true, false] {
        let mut ctx = Context::new();
        let (handle, object) = completed_with_exception(&mut ctx);
        let waiter = spawn(&mut ctx, parking_resume, std::ptr::null_mut());
        // SAFETY: both frames are live; the test owns the handle's creation count.
        unsafe {
            assert_eq!(ctx.async_count(handle), 1);
            if owned {
                subscript_rt_async_await_owned(&mut *ctx, waiter, handle, 0);
            } else {
                ctx.async_await(waiter, handle, 0);
            }
            assert_eq!(ctx.async_count(handle), if owned { 1 } else { 2 });
            assert!(ctx.async_result(handle, std::ptr::null_mut(), 0));
            ctx.async_release(handle, 0);
        }
        assert_eq!(ctx.is_live(handle as usize), !owned);
        assert!(ctx.trap_record().is_none());
        assert_eq!(ctx.catch_exception(), object);
        if !owned {
            // SAFETY: the held creation count remains live after the registration ends.
            unsafe {
                assert_eq!(ctx.async_count(handle), 1);
                ctx.async_release(handle, 0);
            }
            assert!(!ctx.is_live(handle as usize));
        }
    }
}
