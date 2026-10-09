//! Host operation status, ownership, and scheduler tests (§178).
//! Each test uses at most 24 sources and nine waiter dispatches. No compiler or external process runs.
use std::ffi::{c_char, c_void};
use subscript_runtime::context::{
    completion_kind, AsyncResume, AsyncTaskInfo, CompletionEndpoint, CompletionStatus,
    CLASS_GENERATOR,
};
use subscript_runtime::ffi::*;
use subscript_runtime::{Context, TrapKind};

const POS: u32 = 73;
const ERROR_CLASS: u32 = 41;
#[repr(C)]
struct ErrorObject {
    padding: u64,
    name: *mut u8,
    kind: u32,
    message: *mut u8,
}
fn metadata() -> [u64; 6] {
    [
        std::mem::size_of::<ErrorObject>() as u64,
        ERROR_CLASS as u64,
        std::mem::offset_of!(ErrorObject, kind) as u64,
        std::mem::offset_of!(ErrorObject, name) as u64,
        std::mem::offset_of!(ErrorObject, message) as u64,
        0,
    ]
}
fn source(ctx: &mut Context, size: usize, result_kind: u32) -> (*mut u8, CompletionEndpoint) {
    let mut endpoint = CompletionEndpoint::default();
    let handle = unsafe {
        subscript_rt_async_host_operation(
            ctx,
            size as u64,
            result_kind,
            POS,
            metadata().as_ptr(),
            &mut endpoint,
        )
    };
    assert!(!handle.is_null());
    assert_eq!(unsafe { ctx.async_count(handle) }, 2);
    (handle, endpoint)
}
#[derive(Clone, Copy)]
enum Delivery {
    Value,
    Void,
    Error,
}
unsafe fn deliver(
    ctx: &mut Context,
    endpoint: CompletionEndpoint,
    kind: Delivery,
) -> CompletionStatus {
    unsafe {
        match kind {
            Delivery::Value => {
                subscript_rt_complete_value(ctx, endpoint, (&37u32 as *const u32).cast(), 4)
            }
            Delivery::Void => subscript_rt_complete_void(ctx, endpoint),
            Delivery::Error => {
                subscript_rt_complete_error(ctx, endpoint, b"failure".as_ptr().cast::<c_char>(), 7)
            }
        }
    }
}
fn shaped_source(ctx: &mut Context, kind: Delivery) -> (*mut u8, CompletionEndpoint) {
    source(
        ctx,
        if matches!(kind, Delivery::Void) { 0 } else { 4 },
        if matches!(kind, Delivery::Void) {
            completion_kind::VOID
        } else {
            completion_kind::VALUE
        },
    )
}
fn read(ctx: &mut Context, handle: *mut u8) -> u32 {
    let mut value = 0u32;
    assert_eq!(
        unsafe { subscript_rt_async_result(ctx, handle, (&mut value as *mut u32).cast(), 4) },
        1
    );
    value
}
unsafe extern "C" fn visit(data: *mut c_void, info: *const AsyncTaskInfo) {
    unsafe { &mut *data.cast::<Vec<AsyncTaskInfo>>() }.push(unsafe { *info });
}
fn tasks(ctx: &Context) -> Vec<AsyncTaskInfo> {
    let mut records = Vec::new();
    unsafe {
        subscript_rt_ctx_visit_async_tasks(
            ctx,
            Some(visit),
            (&mut records as *mut Vec<AsyncTaskInfo>).cast(),
        )
    };
    records
}

#[test]
fn statuses_context_identity_and_check_order() {
    for kind in [Delivery::Value, Delivery::Void, Delivery::Error] {
        let mut first = Context::new();
        let (handle, endpoint) = shaped_source(&mut first, kind);
        let mut second = Context::new();
        let (_, other) = shaped_source(&mut second, kind);
        assert_ne!(endpoint.context_id, other.context_id);
        assert_eq!(endpoint.operation_id, other.operation_id);
        assert_eq!(
            unsafe { deliver(&mut second, endpoint, kind) },
            CompletionStatus::Stale
        );
        assert_eq!(
            unsafe { deliver(&mut second, other, kind) },
            CompletionStatus::Ok
        );
        assert_eq!(
            unsafe { deliver(&mut first, endpoint, kind) },
            CompletionStatus::Ok
        );
        assert_eq!(
            unsafe { deliver(&mut first, endpoint, kind) },
            CompletionStatus::Duplicate
        );
        // A completed source wins over a wrong result kind or size.
        assert_eq!(
            unsafe { subscript_rt_complete_value(&mut *first, endpoint, std::ptr::null(), 123) },
            CompletionStatus::Duplicate
        );
        if matches!(kind, Delivery::Error) {
            let mut out = 0u32;
            unsafe {
                first.async_result(handle, (&mut out as *mut u32).cast(), 4);
            }
            first.catch_exception();
        }
        unsafe {
            first.async_release(handle, 0);
        }
        assert!(!first.trapped());
        assert_eq!(
            unsafe { deliver(&mut first, endpoint, kind) },
            CompletionStatus::Stale
        );
        let (_, replacement) = shaped_source(&mut first, kind);
        assert!(replacement.operation_id > endpoint.operation_id);
        assert_eq!(
            unsafe { deliver(&mut first, replacement, kind) },
            CompletionStatus::Ok
        );
        let mut trapped = Context::new();
        let (_, live) = shaped_source(&mut trapped, kind);
        trapped.trap(TrapKind::DivisionByZero, "control", 9);
        assert_eq!(
            unsafe { deliver(&mut trapped, live, kind) },
            CompletionStatus::Trapped
        );
        assert_eq!(
            unsafe { deliver(&mut trapped, endpoint, kind) },
            CompletionStatus::Trapped
        );
        trapped.clear_trap();
        assert_eq!(
            unsafe { deliver(&mut trapped, live, kind) },
            CompletionStatus::Ok
        );
    }
    assert_eq!(std::mem::size_of::<CompletionEndpoint>(), 16);
    assert_eq!(std::mem::offset_of!(CompletionEndpoint, operation_id), 8);
    assert_eq!(std::mem::size_of::<CompletionStatus>(), 4);
    assert_eq!(
        [
            CompletionStatus::Ok as u32,
            CompletionStatus::Stale as u32,
            CompletionStatus::Duplicate as u32,
            CompletionStatus::Mismatch as u32,
            CompletionStatus::Trapped as u32,
            CompletionStatus::InvalidUtf8 as u32,
            CompletionStatus::TooLarge as u32
        ],
        [0, 1, 2, 3, 4, 5, 6]
    );
    let header = subscript_runtime::host_header::render().unwrap();
    assert!(header.contains("subscript_rt_completion_status subscript_rt_complete_value(subscript_rt_context* ctx, subscript_rt_completion endpoint, const void* value, size_t size);"));
    assert!(header.contains("subscript_rt_completion_status subscript_rt_complete_string(subscript_rt_context* ctx, subscript_rt_completion endpoint, const char* bytes, size_t length);"));
    assert!(header.contains("subscript_rt_completion_status subscript_rt_complete_bytes(subscript_rt_context* ctx, subscript_rt_completion endpoint, const uint8_t* bytes, size_t length);"));
    assert!(header.contains("SUBSCRIPT_RT_COMPLETION_INVALID_UTF8 = 5"));
    assert!(header.contains("SUBSCRIPT_RT_COMPLETION_TOO_LARGE = 6"));
    assert!(header.contains("subscript_rt_completion_status subscript_rt_complete_void(subscript_rt_context* ctx, subscript_rt_completion endpoint);"));
    assert!(header.contains("subscript_rt_completion_status subscript_rt_complete_error(subscript_rt_context* ctx, subscript_rt_completion endpoint, const char* message, size_t length);"));
}

#[test]
fn each_mismatch_keeps_the_source_pending_with_ok_controls() {
    let mut ctx = Context::new();
    let (value, endpoint) = source(&mut ctx, 4, completion_kind::VALUE);
    assert_eq!(
        unsafe { subscript_rt_complete_value(&mut *ctx, endpoint, std::ptr::null(), 8) },
        CompletionStatus::Mismatch
    );
    assert_eq!(
        unsafe { subscript_rt_complete_void(&mut *ctx, endpoint) },
        CompletionStatus::Mismatch
    );
    assert_eq!(unsafe { ctx.async_count(value) }, 2);
    assert_eq!(ctx.async_unfinished(), 1);
    assert_eq!(
        unsafe { deliver(&mut ctx, endpoint, Delivery::Value) },
        CompletionStatus::Ok
    );
    assert_eq!(read(&mut ctx, value), 37);
    let (_, endpoint) = source(&mut ctx, 0, completion_kind::VOID);
    assert_eq!(
        unsafe { subscript_rt_complete_value(&mut *ctx, endpoint, std::ptr::null(), 0) },
        CompletionStatus::Mismatch
    );
    assert_eq!(
        unsafe { deliver(&mut ctx, endpoint, Delivery::Void) },
        CompletionStatus::Ok
    );
    let (empty, endpoint) = source(&mut ctx, 0, completion_kind::VALUE);
    assert_eq!(
        unsafe { subscript_rt_complete_void(&mut *ctx, endpoint) },
        CompletionStatus::Mismatch
    );
    assert_eq!(
        unsafe { subscript_rt_complete_value(&mut *ctx, endpoint, std::ptr::null(), 0) },
        CompletionStatus::Ok
    );
    assert!(unsafe { ctx.async_result(empty, std::ptr::null_mut(), 0) });
}

#[repr(C)]
struct Waiter {
    state: u32,
    count: u32,
    resume: AsyncResume,
    source: *mut u8,
    value: u32,
    calls: u32,
    error: *mut u8,
    external_calls: *mut u32,
}
unsafe extern "C" fn resume(ctx: *mut Context, frame: *mut u8, _: *mut u8) -> u8 {
    let ctx = unsafe { &mut *ctx };
    let waiter = unsafe { &mut *frame.cast::<Waiter>() };
    assert!(unsafe { ctx.async_result(waiter.source, (&mut waiter.value as *mut u32).cast(), 4) });
    if ctx.exception_pending() {
        waiter.error = ctx.catch_exception();
    }
    waiter.calls += 1;
    if !waiter.external_calls.is_null() {
        unsafe {
            *waiter.external_calls += 1;
        }
    }
    unsafe {
        ctx.async_release(waiter.source, 0);
    }
    1
}
fn waiter(ctx: &mut Context, source: *mut u8) -> *mut Waiter {
    let frame = ctx.alloc(std::mem::size_of::<Waiter>(), CLASS_GENERATOR, 11);
    unsafe {
        frame.cast::<Waiter>().write(Waiter {
            state: 0,
            count: 0,
            resume,
            source,
            value: 0,
            calls: 0,
            error: std::ptr::null_mut(),
            external_calls: std::ptr::null_mut(),
        });
        ctx.async_register(frame, 0);
        ctx.async_await(frame, source, 19);
    }
    frame.cast()
}

#[test]
fn before_after_shared_and_repeated_awaits_copy_cached_values() {
    for early in [false, true] {
        let mut ctx = Context::new();
        let (handle, endpoint) = source(&mut ctx, 4, completion_kind::VALUE);
        if early {
            assert_eq!(
                unsafe { deliver(&mut ctx, endpoint, Delivery::Value) },
                CompletionStatus::Ok
            );
        }
        let waiters = [
            waiter(&mut ctx, handle),
            waiter(&mut ctx, handle),
            waiter(&mut ctx, handle),
        ];
        if !early {
            assert_eq!(ctx.async_pending(), 0);
            assert_eq!(
                unsafe { deliver(&mut ctx, endpoint, Delivery::Value) },
                CompletionStatus::Ok
            );
        }
        assert_eq!(ctx.async_pending(), 3);
        assert!(waiters.iter().all(|w| unsafe { (**w).calls == 0 }));
        unsafe {
            ctx.async_step();
        }
        for w in waiters {
            assert_eq!(unsafe { ((*w).value, (*w).calls) }, (37, 1));
        }
        assert_eq!(read(&mut ctx, handle), 37);
        assert_eq!(read(&mut ctx, handle), 37);
        let w = waiter(&mut ctx, handle);
        unsafe {
            ctx.async_step();
        }
        assert_eq!(unsafe { ((*w).value, (*w).calls) }, (37, 1));
        unsafe {
            ctx.async_release(handle, 0);
        }
        assert_eq!(
            unsafe { deliver(&mut ctx, endpoint, Delivery::Value) },
            CompletionStatus::Stale
        );
        assert!(!ctx.trapped());
    }
}

#[test]
fn error_metadata_message_position_and_observed_drop_control() {
    for early in [false, true] {
        let mut ctx = Context::new();
        let (handle, endpoint) = source(&mut ctx, 4, completion_kind::VALUE);
        if early {
            assert_eq!(
                unsafe { deliver(&mut ctx, endpoint, Delivery::Error) },
                CompletionStatus::Ok
            );
        }
        let w = waiter(&mut ctx, handle);
        unsafe {
            ctx.async_release(handle, 0);
        }
        if !early {
            assert_eq!(
                unsafe { deliver(&mut ctx, endpoint, Delivery::Error) },
                CompletionStatus::Ok
            );
        }
        assert!(!ctx.trapped());
        ctx.collect();
        unsafe {
            ctx.async_step();
        }
        let object = unsafe { (*w).error };
        assert!(!object.is_null());
        let error = unsafe { &*object.cast::<ErrorObject>() };
        assert_eq!(unsafe { ctx.str_bytes(error.name) }, b"Error");
        assert_eq!(unsafe { ctx.str_bytes(error.message) }, b"failure");
        assert_eq!(error.kind, 0);
        assert_eq!(unsafe { object.sub(8).cast::<u32>().read() }, ERROR_CLASS);
        assert_eq!(unsafe { object.sub(4).cast::<u32>().read() }, POS);
        assert_eq!(
            unsafe { deliver(&mut ctx, endpoint, Delivery::Error) },
            CompletionStatus::Stale
        );
        assert!(!ctx.trapped());
    }
    let mut ctx = Context::new();
    let (handle, endpoint) = source(&mut ctx, 4, completion_kind::VALUE);
    assert_eq!(
        unsafe { deliver(&mut ctx, endpoint, Delivery::Error) },
        CompletionStatus::Ok
    );
    let mut out = 0u32;
    assert!(unsafe { ctx.async_result(handle, (&mut out as *mut u32).cast(), 4) });
    ctx.settle_uncaught_exception();
    let trap = ctx.trap_record().unwrap();
    assert_eq!(
        (trap.kind, trap.pos_id, trap.message.as_str()),
        (TrapKind::UncaughtException, POS, "Error: failure")
    );
}

#[test]
fn producer_keeps_a_dropped_handle_until_value_or_unobserved_error() {
    for kind in [Delivery::Value, Delivery::Void, Delivery::Error] {
        let mut ctx = Context::new();
        let (handle, endpoint) = shaped_source(&mut ctx, kind);
        unsafe {
            ctx.async_release(handle, 9);
        }
        assert_eq!(unsafe { ctx.async_count(handle) }, 1);
        ctx.collect();
        assert!(ctx.is_live(handle as usize));
        assert_eq!(
            unsafe { deliver(&mut ctx, endpoint, kind) },
            CompletionStatus::Ok
        );
        assert!(!ctx.is_live(handle as usize));
        assert!(tasks(&ctx).is_empty());
        if matches!(kind, Delivery::Error) {
            let trap = ctx.trap_record().unwrap();
            assert_eq!(
                (trap.kind, trap.pos_id, trap.message.as_str()),
                (TrapKind::UncaughtException, POS, "Error: failure")
            );
            ctx.clear_trap();
        } else {
            assert!(!ctx.trapped());
        }
        assert_eq!(
            unsafe { deliver(&mut ctx, endpoint, kind) },
            CompletionStatus::Stale
        );
    }
}

#[test]
fn active_script_completion_only_queues_and_collection_preserves_waiters() {
    for collect in [false, true] {
        let mut ctx = Context::new();
        let (handle, endpoint) = source(&mut ctx, 4, completion_kind::VALUE);
        let w = waiter(&mut ctx, handle);
        unsafe {
            ctx.async_release(handle, 0);
        }
        if collect {
            ctx.collect();
        }
        assert!(ctx.is_live(w as usize));
        assert!(ctx.is_live(handle as usize));
        ctx.enter_script();
        assert_eq!(
            unsafe { deliver(&mut ctx, endpoint, Delivery::Value) },
            CompletionStatus::Ok
        );
        assert_eq!(unsafe { (*w).calls }, 0);
        assert_eq!(ctx.async_pending(), 1);
        ctx.exit_script();
        unsafe {
            ctx.async_step();
        }
        assert_eq!(unsafe { ((*w).calls, (*w).value) }, (1, 37));
        assert!(!ctx.is_live(handle as usize));
    }
}

#[test]
fn inspection_waiting_complete_and_pending_destruction_control() {
    for complete in [false, true] {
        let mut ctx = Context::new();
        let (_, endpoint) = source(&mut ctx, 4, completion_kind::VALUE);
        assert_eq!(unsafe { subscript_rt_ctx_async_unfinished(&*ctx) }, 1);
        assert_eq!(unsafe { subscript_rt_ctx_async_pending(&*ctx) }, 0);
        let info = tasks(&ctx)[0];
        ctx.bump_reload_epoch();
        assert_eq!(tasks(&ctx)[0].task_id, info.task_id);
        assert_eq!(ctx.async_unfinished(), 1);
        assert_eq!(
            (
                info.kind,
                info.state,
                info.function_pos_id,
                info.await_pos_id,
                info.create_pos_id,
                info.awaited_task_id
            ),
            (4, 3, 0, 0, POS, 0)
        );
        if complete {
            assert_eq!(
                unsafe { deliver(&mut ctx, endpoint, Delivery::Value) },
                CompletionStatus::Ok
            );
            assert_eq!(unsafe { subscript_rt_ctx_async_unfinished(&*ctx) }, 0);
            assert_eq!(tasks(&ctx)[0].state, 5);
            assert_eq!(tasks(&ctx)[0].function_pos_id, 0);
            assert_eq!(tasks(&ctx)[0].create_pos_id, POS);
        }
        assert_eq!(ctx.async_pending(), 0);
        drop(ctx);
    }
}

#[test]
fn source_and_error_allocation_failure_have_same_shape_ok_controls() {
    for fail in [false, true] {
        let mut ctx = Context::new();
        ctx.fail_alloc_after(u64::from(fail));
        let mut endpoint = CompletionEndpoint::default();
        let handle = unsafe {
            subscript_rt_async_host_operation(
                &mut *ctx,
                4,
                completion_kind::VALUE,
                POS,
                metadata().as_ptr(),
                &mut endpoint,
            )
        };
        assert_eq!(handle.is_null(), fail);
        if fail {
            assert_eq!(ctx.trap_record().unwrap().pos_id, POS);
            assert!(tasks(&ctx).is_empty());
        } else {
            assert_eq!(
                unsafe { deliver(&mut ctx, endpoint, Delivery::Value) },
                CompletionStatus::Ok
            );
        }
    }
    for allocation in 1..=3 {
        for fail in [false, true] {
            let mut ctx = Context::new();
            let (handle, endpoint) = source(&mut ctx, 4, completion_kind::VALUE);
            ctx.fail_alloc_after(if fail { allocation } else { 0 });
            assert_eq!(
                unsafe { deliver(&mut ctx, endpoint, Delivery::Error) },
                if fail {
                    CompletionStatus::Trapped
                } else {
                    CompletionStatus::Ok
                }
            );
            if fail {
                assert_eq!(ctx.trap_record().unwrap().kind, TrapKind::AllocationFailure);
                assert_eq!(ctx.trap_record().unwrap().pos_id, POS);
                assert_eq!(unsafe { ctx.async_count(handle) }, 2);
                ctx.clear_trap();
                assert_eq!(
                    unsafe { deliver(&mut ctx, endpoint, Delivery::Error) },
                    CompletionStatus::Ok
                );
            }
        }
    }
}

#[test]
fn destruction_with_a_pending_waiter_runs_no_script() {
    for complete in [false, true] {
        let mut calls = 0u32;
        let mut ctx = Context::new();
        let (handle, endpoint) = source(&mut ctx, 4, completion_kind::VALUE);
        let w = waiter(&mut ctx, handle);
        unsafe {
            (*w).external_calls = &mut calls;
        }
        if complete {
            assert_eq!(
                unsafe { deliver(&mut ctx, endpoint, Delivery::Value) },
                CompletionStatus::Ok
            );
            unsafe {
                ctx.async_step();
            }
        }
        drop(ctx);
        assert_eq!(calls, u32::from(complete));
    }
}

#[test]
fn completion_during_script_preserves_a_pending_exception() {
    for kind in [Delivery::Value, Delivery::Void, Delivery::Error] {
        let mut ctx = Context::new();
        let (handle, endpoint) = shaped_source(&mut ctx, kind);
        let object = ctx.alloc(8, 42, 1);
        ctx.enter_script();
        ctx.raise_exception(object, "Error: outer".to_owned(), 21);
        assert_eq!(
            unsafe { deliver(&mut ctx, endpoint, kind) },
            CompletionStatus::Ok
        );
        assert_eq!(ctx.pending_exception_object(), Some(object));
        assert_eq!(ctx.catch_exception(), object);
        ctx.exit_script();
        if matches!(kind, Delivery::Error) {
            let mut out = 0u32;
            assert!(unsafe { ctx.async_result(handle, (&mut out as *mut u32).cast(), 4) });
            ctx.catch_exception();
        }
        unsafe {
            ctx.async_release(handle, 0);
        }
        assert!(!ctx.trapped());
    }
}

#[test]
fn boundary_struct_bytes_and_ship_collection_controls() {
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Value {
        integer: i64,
        fraction: f64,
    }
    for ship in [false, true] {
        let mut ctx = if ship {
            Context::new_releasing()
        } else {
            Context::new()
        };
        let value = Value {
            integer: -19,
            fraction: 2.5,
        };
        let (handle, endpoint) = source(
            &mut ctx,
            std::mem::size_of::<Value>(),
            completion_kind::VALUE,
        );
        assert_eq!(
            unsafe {
                subscript_rt_complete_value(
                    &mut *ctx,
                    endpoint,
                    (&value as *const Value).cast(),
                    std::mem::size_of::<Value>(),
                )
            },
            CompletionStatus::Ok
        );
        ctx.collect();
        for _ in 0..2 {
            let mut out = Value {
                integer: 0,
                fraction: 0.0,
            };
            assert!(unsafe {
                ctx.async_result(
                    handle,
                    (&mut out as *mut Value).cast(),
                    std::mem::size_of::<Value>(),
                )
            });
            assert_eq!(out, value);
        }
        unsafe {
            ctx.async_release(handle, 0);
        }
        assert_eq!(
            unsafe { subscript_rt_complete_void(&mut *ctx, endpoint) },
            CompletionStatus::Stale
        );
        let (handle, endpoint) = source(&mut ctx, 4, completion_kind::VALUE);
        let w = waiter(&mut ctx, handle);
        unsafe {
            ctx.async_release(handle, 0);
        }
        ctx.collect();
        assert_eq!(
            unsafe { deliver(&mut ctx, endpoint, Delivery::Error) },
            CompletionStatus::Ok
        );
        ctx.collect();
        unsafe {
            ctx.async_step();
        }
        let error = unsafe { &*(*w).error.cast::<ErrorObject>() };
        assert_eq!(unsafe { ctx.str_bytes(error.message) }, b"failure");
        assert!(!ctx.trapped());
    }
}
