//! Dispatch limits, queue boundaries, traps, and roots (§168).
use subscript_runtime::context::AsyncStepReport;
use subscript_runtime::ffi::subscript_rt_ctx_async_step_budget;
use subscript_runtime::{Context, TrapKind};

#[repr(C)]
struct Frame {
    state: u32,
    count: u32,
    resume: unsafe extern "C" fn(*mut Context, *mut u8, *mut u8) -> u8,
    handle: *mut u8,
    left: u32,
    mode: u32,
    id: u8,
}

unsafe extern "C" fn resume(ctx: *mut Context, frame: *mut u8, _: *mut u8) -> u8 {
    let ctx = unsafe { &mut *ctx };
    let f = unsafe { &mut *frame.cast::<Frame>() };
    ctx.print_line(&[f.id]);
    if f.mode == 2 {
        ctx.trap(TrapKind::Internal, "budget trap", 7);
        return 0;
    }
    if f.left == 0 {
        return 1;
    }
    f.left -= 1;
    unsafe {
        if f.mode == 1 {
            ctx.async_park(frame, 0);
        } else {
            ctx.async_await(frame, f.handle, 0);
        }
    }
    0
}

fn chain(ctx: &mut Context, n: u32, mode: u32, id: u8) -> *mut u8 {
    let handle = ctx.alloc(16, subscript_runtime::context::CLASS_GENERATOR, 0);
    unsafe {
        ctx.async_register(handle, 0);
        ctx.async_complete(handle, std::ptr::null(), 0);
    }
    let frame = ctx.alloc(
        std::mem::size_of::<Frame>(),
        subscript_runtime::context::CLASS_GENERATOR,
        0,
    );
    unsafe {
        frame.cast::<Frame>().write(Frame {
            state: 0,
            count: 0,
            resume,
            handle,
            left: n - 1,
            mode,
            id,
        });
        ctx.async_register(frame, 0);
        if mode == 1 {
            ctx.async_park(frame, 0);
        } else {
            ctx.async_await(frame, handle, 0);
        }
        ctx.async_release(frame, 0);
    }
    frame
}

#[test]
fn completed_chain_matches_unbounded_and_large_budget() {
    for budget in [3, 100] {
        let mut ctx = Context::new();
        chain(&mut ctx, 8, 0, b'a');
        chain(&mut ctx, 2, 0, b'b');
        let first = unsafe { ctx.async_step_budget(budget) };
        assert_eq!(first.dispatched, budget.min(10));
        assert_eq!(first.budget_exhausted, u64::from(budget < 10));
        assert_eq!(first.pending, ctx.async_pending() as u64);
        assert_eq!(first.unfinished, ctx.async_unfinished() as u64);
        while ctx.async_pending() != 0 {
            unsafe {
                ctx.async_step_budget(budget);
            }
        }
        let mut control = Context::new();
        chain(&mut control, 8, 0, b'a');
        chain(&mut control, 2, 0, b'b');
        assert_eq!(unsafe { control.async_step() }, 0);
        let output = ctx.take_stdout();
        assert_eq!(output, b"a\nb\na\nb\na\na\na\na\na\na\n");
        assert_eq!(output, control.take_stdout());
        assert_eq!(ctx.async_unfinished(), 0);
    }
}

#[test]
fn zero_keeps_parked_order_and_new_parks_wait() {
    let mut ctx = Context::new();
    chain(&mut ctx, 2, 1, b'p');
    let parked_only = unsafe { ctx.async_step_budget(0) };
    assert_eq!(
        (
            parked_only.dispatched,
            parked_only.pending,
            parked_only.budget_exhausted
        ),
        (0, 1, 0)
    );
    assert_eq!(ctx.async_parked_len(), 1);
    chain(&mut ctx, 2, 0, b'r');
    let zero = unsafe { subscript_rt_ctx_async_step_budget(&mut *ctx, 0) };
    assert_eq!(
        (
            zero.dispatched,
            zero.pending,
            zero.unfinished,
            zero.budget_exhausted
        ),
        (0, 2, 2, 1)
    );
    assert_eq!(ctx.async_parked_len(), 1);
    assert!(ctx.take_stdout().is_empty());
    assert_eq!(unsafe { ctx.async_step_budget(1) }.budget_exhausted, 1);
    assert_eq!(ctx.take_stdout(), b"r\n");
    // The remaining parked job precedes the continuation added by r.
    let next = unsafe { ctx.async_step_budget(100) };
    assert_eq!(
        (next.dispatched, next.pending, next.budget_exhausted),
        (2, 1, 0)
    );
    assert_eq!(ctx.take_stdout(), b"p\nr\n");
    assert_eq!(unsafe { ctx.async_step() }, 0);
    assert_eq!(ctx.take_stdout(), b"p\n");
}

#[test]
fn trap_counts_the_job_and_preserves_the_head() {
    let mut ctx = Context::new();
    chain(&mut ctx, 1, 2, b't');
    chain(&mut ctx, 1, 0, b'r');
    let report = unsafe { ctx.async_step_budget(1) };
    assert_eq!(
        (
            report.dispatched,
            report.pending,
            report.unfinished,
            report.budget_exhausted
        ),
        (1, 2, 2, 1)
    );
    assert_eq!(ctx.take_stdout(), b"t\n");
    let again = unsafe { ctx.async_step_budget(1) };
    assert_eq!(
        (again.dispatched, again.pending, again.budget_exhausted),
        (0, 2, 0)
    );
    assert!(ctx.take_stdout().is_empty());
    ctx.clear_trap();
    assert_eq!(unsafe { ctx.async_step_budget(1) }.pending, 0);
    assert_eq!(ctx.take_stdout(), b"r\n");
    assert_eq!(ctx.async_unfinished(), 1);
}

#[test]
fn collection_keeps_remaining_ready_frames() {
    let mut ctx = Context::new();
    let a = chain(&mut ctx, 4, 0, b'a');
    let b = chain(&mut ctx, 2, 0, b'b');
    assert_eq!(unsafe { ctx.async_step_budget(1) }.pending, 2);
    ctx.collect();
    assert!(ctx.is_live(a as usize));
    assert!(ctx.is_live(b as usize));
    while ctx.async_pending() != 0 {
        unsafe {
            ctx.async_step_budget(1);
        }
    }
    assert_eq!(ctx.take_stdout(), b"a\nb\na\nb\na\na\n");
    assert_eq!(ctx.async_unfinished(), 0);
}

#[test]
fn report_has_c_layout_without_padding() {
    assert_eq!(std::mem::size_of::<AsyncStepReport>(), 32);
    assert_eq!(std::mem::offset_of!(AsyncStepReport, dispatched), 0);
    assert_eq!(std::mem::offset_of!(AsyncStepReport, pending), 8);
    assert_eq!(std::mem::offset_of!(AsyncStepReport, unfinished), 16);
    assert_eq!(std::mem::offset_of!(AsyncStepReport, budget_exhausted), 24);
}

#[test]
fn aggregate_trap_stays_pending_until_clearance() {
    let mut ctx = Context::new();
    let input = ctx.alloc(16, subscript_runtime::context::CLASS_GENERATOR, 0);
    let error = ctx.alloc(24, 1, 0);
    unsafe {
        ctx.async_register(input, 0);
    }
    ctx.raise_exception(error, "aggregate failure".to_string(), 9);
    ctx.async_complete_exception(input);
    let jobs = ctx.array_new(std::mem::size_of::<*mut u8>(), 0);
    unsafe {
        ctx.array_push(jobs, (&input as *const *mut u8).cast(), 0);
    }
    let all = unsafe { ctx.async_all(jobs, 0, 0, 0) };
    unsafe {
        ctx.async_release(all, 0);
    }
    let report = unsafe { ctx.async_step_budget(1) };
    assert_eq!(
        (
            report.dispatched,
            report.pending,
            report.unfinished,
            report.budget_exhausted
        ),
        (1, 1, 0, 1)
    );
    let again = unsafe { ctx.async_step_budget(10) };
    assert_eq!(
        (again.dispatched, again.pending, again.budget_exhausted),
        (0, 1, 0)
    );
    ctx.collect();
    ctx.clear_trap();
    assert_eq!(unsafe { ctx.async_step() }, 0);
    assert!(!ctx.trapped());
}
