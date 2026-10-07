//! Task states, ownership, and read-only visits (§169).
use std::ffi::c_void;
use subscript_runtime::context::{AsyncTaskInfo, CLASS_GENERATOR};
use subscript_runtime::ffi::subscript_rt_ctx_visit_async_tasks;
use subscript_runtime::{Context, TrapKind};

unsafe extern "C" fn visit(data: *mut c_void, info: *const AsyncTaskInfo) {
    unsafe { &mut *data.cast::<Vec<AsyncTaskInfo>>() }.push(unsafe { *info });
}
fn tasks(ctx: &Context) -> Vec<AsyncTaskInfo> {
    let before = (
        ctx.async_pending(),
        ctx.async_unfinished(),
        ctx.live_count(),
    );
    let mut records = Vec::new();
    let n = unsafe {
        subscript_rt_ctx_visit_async_tasks(
            ctx,
            Some(visit),
            (&mut records as *mut Vec<AsyncTaskInfo>).cast(),
        )
    };
    assert_eq!(n as usize, records.len());
    let mut direct = Vec::new();
    let count = unsafe {
        ctx.visit_async_tasks(Some(visit), (&mut direct as *mut Vec<AsyncTaskInfo>).cast())
    };
    assert_eq!(count, n);
    assert_eq!(direct, records);
    assert_eq!(
        unsafe { ctx.visit_async_tasks(None, std::ptr::null_mut()) },
        0
    );
    assert_eq!(
        before,
        (
            ctx.async_pending(),
            ctx.async_unfinished(),
            ctx.live_count()
        )
    );
    records
}
unsafe extern "C" fn resume(ctx: *mut Context, _: *mut u8, _: *mut u8) -> u8 {
    let ctx = unsafe { &mut *ctx };
    assert!(tasks(ctx)
        .iter()
        .any(|t| t.state == 4 && t.await_pos_id == 0));
    1
}
unsafe extern "C" fn fail(ctx: *mut Context, _: *mut u8, _: *mut u8) -> u8 {
    unsafe { &mut *ctx }.trap(TrapKind::Internal, "inspection trap", 9);
    0
}
fn frame(
    ctx: &mut Context,
    callback: unsafe extern "C" fn(*mut Context, *mut u8, *mut u8) -> u8,
) -> *mut u8 {
    let f = ctx.alloc(16, CLASS_GENERATOR, 7);
    unsafe {
        f.add(8)
            .cast::<unsafe extern "C" fn(*mut Context, *mut u8, *mut u8) -> u8>()
            .write(callback);
        ctx.async_register(f, 0);
    }
    f
}
#[test]
fn all_invocation_states_and_read_only_control() {
    for trapping in [false, true] {
        let mut ctx = Context::new();
        let a = frame(&mut ctx, if trapping { fail } else { resume });
        let b = frame(&mut ctx, resume);
        unsafe {
            ctx.async_park(a, 11);
            ctx.async_await(b, a, 13);
        }
        let suspended = tasks(&ctx);
        assert_eq!((suspended[0].state, suspended[0].await_pos_id), (2, 11));
        assert_eq!(
            (
                suspended[1].state,
                suspended[1].awaited_task_id,
                suspended[1].await_pos_id
            ),
            (3, 1, 13)
        );
        unsafe {
            ctx.async_step_budget(1);
        }
        let after = tasks(&ctx);
        assert_eq!(after[0].state, if trapping { 1 } else { 5 });
        assert_eq!(after[0].await_pos_id, if trapping { 11 } else { 0 });
        assert_eq!(after[1].state, if trapping { 3 } else { 1 });
        ctx.clear_trap();
        assert_eq!(tasks(&ctx)[0].state, if trapping { 6 } else { 5 });
        unsafe {
            ctx.async_step();
        }
        assert_eq!(tasks(&ctx)[1].state, if trapping { 3 } else { 5 });
        assert!(ctx.take_stdout().is_empty());
        assert_eq!(
            unsafe { subscript_rt_ctx_visit_async_tasks(&*ctx, None, std::ptr::null_mut()) },
            0
        );
    }
}
#[test]
fn aggregate_waiting_and_complete_control() {
    for complete in [false, true] {
        let mut ctx = Context::new();
        let a = frame(&mut ctx, resume);
        if !complete {
            unsafe {
                ctx.async_park(a, 0);
            }
        }
        if complete {
            unsafe {
                ctx.async_complete(a, std::ptr::null(), 0);
            }
        }
        let jobs = ctx.array_new(8, 0);
        unsafe {
            ctx.array_push(jobs, (&a as *const *mut u8).cast(), 0);
        }
        let all = unsafe { ctx.async_all(jobs, 0, 17) };
        let info = tasks(&ctx)[1];
        assert_eq!((info.create_pos_id, info.reserved), (0, 0));
        assert_eq!(
            (
                info.task_id,
                info.kind,
                info.state,
                info.function_pos_id,
                info.await_pos_id,
                info.awaited_task_id
            ),
            (2, 2, 3, 17, 0, 0)
        );
        unsafe {
            ctx.async_step_budget(1);
        }
        assert_eq!(tasks(&ctx)[1].state, if complete { 5 } else { 3 });
        assert_eq!(unsafe { ctx.async_count(all) }, 1);
    }
}
#[test]
fn wait_ring_and_ready_control() {
    for complete in [false, true] {
        let mut ctx = Context::new();
        let fs = [
            frame(&mut ctx, resume),
            frame(&mut ctx, resume),
            frame(&mut ctx, resume),
        ];
        if complete {
            for f in fs {
                unsafe {
                    ctx.async_complete(f, std::ptr::null(), 0);
                }
            }
        }
        for i in 0..3 {
            unsafe {
                ctx.async_await(fs[i], fs[(i + 1) % 3], 21 + i as u32);
            }
        }
        let ts = tasks(&ctx);
        if complete {
            assert_eq!(ctx.async_pending(), 3);
            assert!(ts.iter().all(|t| t.state == 5 && t.awaited_task_id == 0));
        } else {
            assert_eq!(ctx.async_pending(), 0);
            assert_eq!(ctx.async_unfinished(), 3);
            assert_eq!(
                ts.iter().map(|t| t.awaited_task_id).collect::<Vec<_>>(),
                vec![2, 3, 1]
            );
        }
    }
}
#[test]
fn ids_are_not_reused_and_layout_has_no_padding() {
    assert_eq!(std::mem::size_of::<AsyncTaskInfo>(), 40);
    assert_eq!(std::mem::offset_of!(AsyncTaskInfo, task_id), 0);
    assert_eq!(std::mem::offset_of!(AsyncTaskInfo, awaited_task_id), 8);
    assert_eq!(std::mem::offset_of!(AsyncTaskInfo, state), 16);
    assert_eq!(std::mem::offset_of!(AsyncTaskInfo, kind), 20);
    assert_eq!(std::mem::offset_of!(AsyncTaskInfo, function_pos_id), 24);
    assert_eq!(std::mem::offset_of!(AsyncTaskInfo, await_pos_id), 28);
    assert_eq!(std::mem::offset_of!(AsyncTaskInfo, create_pos_id), 32);
    assert_eq!(std::mem::offset_of!(AsyncTaskInfo, reserved), 36);
    let mut ctx = Context::new();
    let a = frame(&mut ctx, resume);
    unsafe {
        ctx.async_complete(a, std::ptr::null(), 0);
        ctx.async_release(a, 0);
    }
    assert!(tasks(&ctx).is_empty());
    let b = frame(&mut ctx, resume);
    unsafe {
        ctx.async_park(b, 0);
    }
    assert_eq!(tasks(&ctx)[0].task_id, 2);
}

#[test]
fn called_prefix_is_active_and_start_runs_once() {
    use subscript_runtime::ffi::subscript_rt_async_start;
    for ffi in [false, true] {
        let mut ctx = Context::new();
        let f = frame(&mut ctx, resume);
        let done = unsafe {
            if ffi {
                subscript_rt_async_start(&mut *ctx, f, std::ptr::null_mut(), 23)
            } else {
                ctx.async_start(f, std::ptr::null_mut(), 23)
            }
        };
        assert_eq!(done, 1);
        unsafe {
            ctx.async_complete(f, std::ptr::null(), 0);
        }
        assert_eq!(tasks(&ctx)[0].state, 5);
        assert_eq!(tasks(&ctx)[0].create_pos_id, 23);
        assert_eq!(tasks(&ctx)[0].reserved, 0);
        assert_eq!(
            unsafe { ctx.async_start(std::ptr::null_mut(), std::ptr::null_mut(), 0) },
            0
        );
    }
}

#[test]
fn prefix_traps_stop_calls_and_host_kicks() {
    for host in [false, true] {
        for trapping in [false, true] {
            let mut ctx = Context::new();
            let callback = if trapping { fail } else { resume };
            let f = frame(&mut ctx, callback);
            unsafe {
                if host {
                    ctx.async_kick(f, callback);
                } else {
                    let done = subscript_runtime::ffi::subscript_rt_async_start(
                        &mut *ctx,
                        f,
                        std::ptr::null_mut(),
                        31,
                    );
                    if done != 0 {
                        ctx.async_complete(f, std::ptr::null(), 0);
                    }
                }
            }
            let before_clear = tasks(&ctx);
            if trapping || !host {
                assert_eq!(before_clear[0].state, if trapping { 6 } else { 5 });
                assert_eq!(before_clear[0].create_pos_id, if host { 0 } else { 31 });
                assert_eq!(before_clear[0].await_pos_id, 0);
                assert_eq!(before_clear[0].reserved, 0);
            } else {
                assert!(before_clear.is_empty());
            }
            ctx.clear_trap();
            unsafe {
                ctx.async_step_budget(1);
            }
            assert_eq!(tasks(&ctx), before_clear);
            assert_eq!(ctx.async_pending(), 0);
            assert_eq!(ctx.async_unfinished(), usize::from(trapping));
        }
    }
}

#[test]
fn a_manual_frame_without_a_scheduler_state_never_causes_a_partial_visit() {
    for parked in [false, true] {
        let mut ctx = Context::new();
        let a = frame(&mut ctx, resume);
        let b = frame(&mut ctx, resume);
        unsafe {
            ctx.async_complete(a, std::ptr::null(), 0);
            if parked {
                ctx.async_park(b, 0);
            }
        }
        let mut records = Vec::<AsyncTaskInfo>::new();
        let count = unsafe {
            ctx.visit_async_tasks(
                Some(visit),
                (&mut records as *mut Vec<AsyncTaskInfo>).cast(),
            )
        };
        assert_eq!(count as usize, records.len());
        assert_eq!(count, if parked { 2 } else { 0 });
    }
}

#[test]
fn one_visit_reads_all_states_with_a_complete_control() {
    unsafe extern "C" fn finish(_: *mut Context, _: *mut u8, _: *mut u8) -> u8 {
        1
    }
    unsafe extern "C" fn read(ctx: *mut Context, _: *mut u8, out: *mut u8) -> u8 {
        let records = unsafe { &mut *out.cast::<Vec<AsyncTaskInfo>>() };
        unsafe {
            (&*ctx).visit_async_tasks(Some(visit), (records as *mut Vec<AsyncTaskInfo>).cast())
        };
        1
    }
    for trapping in [false, true] {
        let mut ctx = Context::new();
        let complete = frame(&mut ctx, resume);
        let ready = frame(&mut ctx, resume);
        let parked = frame(&mut ctx, resume);
        let waiting = frame(&mut ctx, resume);
        let stopped = frame(&mut ctx, if trapping { fail } else { finish });
        let active = frame(&mut ctx, read);
        unsafe {
            ctx.async_complete(complete, std::ptr::null(), 0);
            ctx.async_await(ready, complete, 17);
            ctx.async_park(parked, 19);
            ctx.async_await(waiting, parked, 23);
            let done = ctx.async_start(stopped, std::ptr::null_mut(), 29);
            if done != 0 {
                ctx.async_complete(stopped, std::ptr::null(), 0);
            }
        }
        ctx.clear_trap();
        let before = (
            ctx.async_pending(),
            ctx.async_unfinished(),
            ctx.live_count(),
        );
        let mut records = Vec::<AsyncTaskInfo>::new();
        unsafe {
            ctx.async_start(active, (&mut records as *mut Vec<AsyncTaskInfo>).cast(), 31);
        }
        assert_eq!(records.len(), 6);
        assert_eq!(
            records.iter().map(|t| t.state).collect::<Vec<_>>(),
            vec![5, 1, 2, 3, if trapping { 6 } else { 5 }, 4]
        );
        assert_eq!(
            records.iter().map(|t| t.task_id).collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5, 6]
        );
        assert_eq!(records[3].awaited_task_id, 3);
        assert_eq!(records[3].await_pos_id, 23);
        assert_eq!(records[5].await_pos_id, 0);
        assert_eq!(
            before,
            (
                ctx.async_pending(),
                ctx.async_unfinished(),
                ctx.live_count()
            )
        );
    }
}
