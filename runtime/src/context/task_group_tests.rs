use super::*;
use crate::ffi::subscript_rt_task_group;

fn task(ctx: &mut Context, fail: bool) -> *mut u8 {
    let input = ctx.alloc(16, CLASS_GENERATOR, 0);
    unsafe { ctx.async_register(input, 0) };
    if fail {
        let object = ctx.alloc(24, 1, 0);
        ctx.raise_exception(object, "failed".into(), 41);
        ctx.async_complete_exception(input);
    } else {
        unsafe { ctx.async_complete(input, std::ptr::null(), 0) };
    }
    input
}

#[test]
fn add_registers_a_reaction_and_join_waits_for_it_with_success_control() {
    for fail in [false, true] {
        let mut ctx = Context::new();
        let group = ctx.task_group_create(1);
        let input = task(&mut ctx, fail);
        unsafe { ctx.task_group_add(group, input, 2) };
        assert_eq!(unsafe { ctx.async_count(input) }, 1);
        assert_eq!(ctx.async_ready_len(), 1);
        let join = unsafe { ctx.task_group_join(group, 3) };
        assert!(ctx.async_frames[&(join as usize)].completion.is_none());
        ctx.collect();
        assert!(ctx.is_live(group as usize));
        assert!(ctx.is_live(input as usize));
        let report = unsafe { ctx.async_step_budget(1) };
        assert_eq!(report.dispatched, 1);
        assert!(!ctx.is_live(input as usize));
        assert!(unsafe { ctx.async_result(join, std::ptr::null_mut(), 0) });
        assert_eq!(ctx.exception_pending(), fail);
        if fail {
            ctx.catch_exception();
        }
        unsafe {
            ctx.task_group_release(group, 4);
            ctx.async_release(join, 4);
        }
        assert!(ctx.task_groups.is_empty());
        assert!(ctx.async_frames.is_empty());
        assert!(!ctx.is_live(group as usize));
        assert!(!ctx.trapped());
    }
}

#[test]
fn reactions_record_first_failure_and_wait_for_all_with_success_control() {
    for fail in [false, true] {
        let mut ctx = Context::new();
        let group = ctx.task_group_create(0);
        let a = task(&mut ctx, fail);
        let b = task(&mut ctx, fail);
        let exception_object =
            |input: *mut u8| match ctx.async_frames[&(input as usize)].completion.as_ref() {
                Some(crate::exception::Completion::Exception(payload)) => {
                    Some(payload.exception.object)
                }
                _ => None,
            };
        let first_object = exception_object(b);
        let second_object = exception_object(a);
        if fail {
            if let Some(crate::exception::Completion::Exception(payload)) = ctx
                .async_frames
                .get_mut(&(b as usize))
                .and_then(|meta| meta.completion.as_mut())
            {
                payload.exception.message = "second task first reaction".into();
            }
        }
        unsafe {
            ctx.task_group_add(group, b, 0);
            ctx.task_group_add(group, a, 0);
        }
        let join = unsafe { ctx.task_group_join(group, 0) };
        unsafe { ctx.async_step_budget(1) };
        assert!(ctx.async_frames[&(join as usize)].completion.is_none());
        assert!(!ctx.is_live(b as usize));
        ctx.collect();
        if let Some(object) = first_object {
            // The input is gone and the join has no completion. Only the group roots this object.
            assert!(ctx.is_live(object));
        }
        unsafe { ctx.async_step_budget(1) };
        assert!(!ctx.is_live(a as usize));
        ctx.collect();
        if let Some(object) = second_object {
            // The same failure shape is not the first exception, so the group does not root it.
            assert!(!ctx.is_live(object));
            assert!(ctx.is_live(first_object.expect("first failure")));
        }
        assert!(unsafe { ctx.async_result(join, std::ptr::null_mut(), 0) });
        if fail {
            assert_eq!(
                ctx.pending_exception_object().map(|object| object as usize),
                first_object
            );
            assert_eq!(
                ctx.pending_exception.as_ref().expect("exception").message,
                "second task first reaction"
            );
            ctx.catch_exception();
        }
        unsafe {
            ctx.async_release(join, 0);
            ctx.task_group_release(group, 0);
        }
        assert!(ctx.task_groups.is_empty());
        assert!(ctx.async_frames.is_empty());
        ctx.collect();
        if let Some(object) = first_object {
            assert!(
                !ctx.is_live(object),
                "the released group no longer roots the first exception"
            );
        }
    }
}

#[test]
fn scope_exit_reads_completed_inputs_before_their_reactions_with_success_control() {
    for fail in [false, true] {
        let mut ctx = Context::new();
        let group = ctx.task_group_create(0);
        let input = task(&mut ctx, fail);
        unsafe {
            ctx.task_group_add(group, input, 0);
            ctx.task_group_release(group, 7);
        }
        assert_eq!(ctx.trapped(), fail);
        if fail {
            let trap = ctx.trap_record().expect("scope trap");
            assert_eq!(trap.kind, TrapKind::TaskGroup);
            assert_eq!(
                trap.message,
                "task group scope ended: 0 unfinished, 1 failed"
            );
        } else {
            unsafe { ctx.async_step() };
            assert!(ctx.task_groups.is_empty());
            assert!(ctx.async_frames.is_empty());
        }
    }
}

#[test]
fn scope_exit_reports_unfinished_work_with_joined_control() {
    for joined in [false, true] {
        let mut ctx = Context::new();
        let group = ctx.task_group_create(0);
        let input = ctx.alloc(16, CLASS_GENERATOR, 0);
        unsafe {
            ctx.async_register(input, 0);
            ctx.task_group_add(group, input, 0);
        }
        let join = joined.then(|| unsafe { ctx.task_group_join(group, 0) });
        unsafe { ctx.task_group_release(group, 5) };
        assert_eq!(ctx.trapped(), !joined);
        if let Some(join) = join {
            unsafe {
                ctx.async_complete(input, std::ptr::null(), 0);
                ctx.async_step();
                ctx.async_release(join, 0);
            }
            assert!(ctx.task_groups.is_empty());
            assert!(ctx.async_frames.is_empty());
        } else {
            assert_eq!(
                ctx.trap_record().expect("trap").message,
                "task group scope ended: 1 unfinished, 0 failed"
            );
        }
    }
}

#[test]
fn closed_operations_trap_with_open_group_controls_and_ffi_matches_context() {
    for operation in [1, 2] {
        for closed in [false, true] {
            let mut ctx = Context::new();
            let group = unsafe {
                subscript_rt_task_group(&mut *ctx, 0, std::ptr::null_mut(), std::ptr::null_mut(), 0)
            };
            let first = closed.then(|| unsafe { ctx.task_group_join(group, 0) });
            let input = if operation == 1 {
                task(&mut ctx, false)
            } else {
                std::ptr::null_mut()
            };
            let result = unsafe { subscript_rt_task_group(&mut *ctx, operation, group, input, 8) };
            assert_eq!(ctx.trapped(), closed);
            if closed {
                assert_eq!(ctx.trap_record().expect("trap").kind, TrapKind::TaskGroup);
            } else {
                let join = if operation == 1 {
                    unsafe { ctx.task_group_join(group, 0) }
                } else {
                    result
                };
                unsafe {
                    ctx.async_step();
                    subscript_rt_task_group(&mut *ctx, 3, group, std::ptr::null_mut(), 0);
                    ctx.async_release(join, 0);
                }
                assert!(ctx.task_groups.is_empty());
                assert!(ctx.async_frames.is_empty());
            }
            let _ = first;
        }
    }
    assert_eq!(TrapKind::from_u32(33), Some(TrapKind::TaskGroup));
    assert_eq!(TrapKind::TaskGroup.rule(), "task-group");
}

#[test]
fn closed_add_keeps_its_trap_when_the_transferred_input_failed() {
    for closed in [false, true] {
        let mut ctx = Context::new();
        let group = ctx.task_group_create(0);
        let first = closed.then(|| unsafe { ctx.task_group_join(group, 0) });
        let input = task(&mut ctx, true);
        unsafe { ctx.task_group_add(group, input, 9) };
        if closed {
            assert_eq!(
                ctx.trap_record().expect("closed trap").kind,
                TrapKind::TaskGroup
            );
            assert!(!ctx.is_live(input as usize));
        } else {
            let join = unsafe { ctx.task_group_join(group, 0) };
            unsafe {
                ctx.async_step();
                ctx.async_result(join, std::ptr::null_mut(), 0);
            }
            assert_eq!(
                ctx.pending_exception
                    .as_ref()
                    .expect("observed failure")
                    .message,
                "failed"
            );
            ctx.catch_exception();
            unsafe {
                ctx.async_release(join, 0);
                ctx.task_group_release(group, 0);
            }
            assert!(!ctx.trapped());
            assert!(ctx.task_groups.is_empty());
        }
        let _ = first;
    }
}
