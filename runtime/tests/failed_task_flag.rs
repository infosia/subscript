//! The flag that a failed unobserved task can exist (`compiler.md` §342
//! rule 14b). No compiler or external process runs.

use std::ffi::c_char;
use subscript_runtime::context::{
    completion_kind, CollectionMode, CompletionEndpoint, CompletionStatus, CLASS_GENERATOR,
};
use subscript_runtime::ffi::*;
use subscript_runtime::Context;

/// A registered async frame with no completion.
fn frame(ctx: &mut Context) -> *mut u8 {
    let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
    // SAFETY: the fresh allocation has the async header.
    unsafe { ctx.async_register(frame, 0) };
    frame
}

/// Completes `frame` with an exception through `async_complete_exception`.
fn fail(ctx: &mut Context, frame: *mut u8, pos_id: u32) {
    let error = ctx.alloc(24, 1, 0);
    // SAFETY: the error is a live handle that exception roots retain.
    unsafe { ctx.raise_exception(error, "Error: flag".to_string(), pos_id) };
    ctx.async_complete_exception(frame);
}

/// Reads the failure of `frame` as an `await` does, and catches it.
fn observe(ctx: &mut Context, frame: *mut u8) {
    // SAFETY: the frame is registered and has a zero-size result.
    assert!(unsafe { ctx.async_result(frame, std::ptr::null_mut(), 0) });
    let _ = ctx.catch_exception();
}

/// One outermost exit with no work.
fn exit(ctx: &mut Context) {
    ctx.enter_script();
    ctx.exit_script();
}

/// One root word that keeps `value` reachable.
fn root(ctx: &mut Context, value: usize) -> Box<usize> {
    let mut slot = Box::new(value);
    // SAFETY: the box stays allocated and unmoved while the Context lives.
    unsafe { ctx.root_add(&mut *slot as *mut usize as usize, 1) };
    slot
}

/// (a) The flag is clear with no failed task, set after a task fails with
/// no `await`, stays set at an outermost exit while the failure is
/// unobserved, and clears at the first outermost exit after the `await`.
#[test]
fn the_flag_follows_a_failed_task_until_an_await_observes_it() {
    for ship in [false, true] {
        let mut ctx = if ship {
            Context::new_releasing()
        } else {
            Context::new()
        };
        assert!(ctx.set_collection(CollectionMode::Automatic, 0));
        let task = frame(&mut ctx);
        let _root = root(&mut ctx, task as usize);
        exit(&mut ctx);
        assert!(!ctx.task_failure_possible(), "no task failed");
        fail(&mut ctx, task, 30);
        assert!(ctx.task_failure_possible());
        exit(&mut ctx);
        assert!(!ctx.trapped(), "the task is reachable");
        assert!(ctx.task_failure_possible(), "the failure is unobserved");
        observe(&mut ctx, task);
        assert!(ctx.task_failure_possible(), "no outermost exit ran");
        exit(&mut ctx);
        assert!(!ctx.task_failure_possible());
        assert!(!ctx.trapped());
    }
}

/// (b) Each kind of site that stores a failure in a task sets the flag.
/// Each case starts with a clear flag, then stores one failure through one
/// site.
///
/// | Case | Site |
/// |---|---|
/// | invocation | `Context::async_complete_exception` (`exception.rs`) |
/// | aggregate | the `Promise.all` reaction, through `async_complete_exception` |
/// | host operation | `host_operation_finish` (`host_operation.rs`), from `subscript_rt_complete_error` |
/// | task group join | `task_group_settle` (`task_group.rs`) |
/// | recorded failure | `record_unobserved_failure` (`collection.rs`), from `Context::record_task_failure` |
#[test]
fn each_site_that_stores_a_failure_sets_the_flag() {
    // Invocation.
    let mut ctx = Context::new();
    let task = frame(&mut ctx);
    assert!(!ctx.task_failure_possible());
    fail(&mut ctx, task, 31);
    assert!(ctx.task_failure_possible(), "invocation");

    // Aggregate: the input failure is observed and the flag clears before
    // the aggregate reacts.
    let mut ctx = Context::new();
    let input = frame(&mut ctx);
    fail(&mut ctx, input, 32);
    observe(&mut ctx, input);
    exit(&mut ctx);
    assert!(!ctx.task_failure_possible());
    let array = ctx.array_new(std::mem::size_of::<*mut u8>(), 0);
    // SAFETY: the pointer has one native word of readable storage.
    unsafe { ctx.array_push(array, (&input as *const *mut u8).cast(), 0) };
    // SAFETY: the array holds one registered handle with a zero-size result.
    let all = unsafe { ctx.async_all(array, 0, 0, 0) };
    assert!(!all.is_null());
    assert!(!ctx.task_failure_possible());
    // SAFETY: the Context is live and exclusive.
    unsafe { ctx.async_step() };
    assert!(ctx.task_failure_possible(), "aggregate");

    // Host operation.
    let mut ctx = Context::new();
    let metadata: [u64; 6] = [32, 41, 16, 8, 24, 0];
    let mut endpoint = CompletionEndpoint::default();
    // SAFETY: the metadata describes a 32-byte error object.
    let handle = unsafe {
        subscript_rt_async_host_operation(
            &mut *ctx,
            0,
            completion_kind::VOID,
            33,
            metadata.as_ptr(),
            &mut endpoint,
        )
    };
    assert!(!handle.is_null());
    assert!(!ctx.task_failure_possible());
    // SAFETY: the message bytes are readable.
    let status = unsafe {
        subscript_rt_complete_error(&mut *ctx, endpoint, b"failure".as_ptr().cast::<c_char>(), 7)
    };
    assert_eq!(status, CompletionStatus::Ok);
    assert!(ctx.task_failure_possible(), "host operation");

    // Task group join: the input failure is observed by the group reaction,
    // and the flag clears before the join.
    let mut ctx = Context::new();
    assert!(ctx.set_collection(CollectionMode::Automatic, 0));
    let group = ctx.task_group_create(0);
    let input = frame(&mut ctx);
    fail(&mut ctx, input, 34);
    // SAFETY: the group has a live lexical owner and the input is registered.
    unsafe { ctx.task_group_add(group, input, 0) };
    // SAFETY: the Context is live and exclusive.
    unsafe { ctx.async_step() };
    exit(&mut ctx);
    assert!(!ctx.trapped());
    assert!(!ctx.task_failure_possible());
    // SAFETY: the group has a live lexical owner.
    let join = unsafe { ctx.task_group_join(group, 0) };
    assert!(!join.is_null());
    assert!(ctx.task_failure_possible(), "task group join");

    // Recorded failure.
    let mut ctx = Context::new();
    let error = ctx.alloc(24, 1, 0) as usize;
    assert!(!ctx.task_failure_possible());
    let recorded = Context::automatic_collection_with(
        &mut *ctx,
        |ctx| ctx,
        |ctx| ctx.record_task_failure(9, error, "Error: 9".to_string(), 35),
    );
    assert!(recorded);
    assert!(ctx.task_failure_possible(), "recorded failure");
}
