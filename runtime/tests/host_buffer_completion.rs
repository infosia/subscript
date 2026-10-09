//! §184 gates use both allocators. The balance gate completes 40,000 sources in total.
//! The other gates use small buffers. The ignored copy probe times twelve 1 MiB calls.
use std::mem::size_of;
use subscript_runtime::context::{completion_kind, CompletionEndpoint, CompletionStatus};
use subscript_runtime::ffi::*;
use subscript_runtime::{Context, TrapKind};

fn context(ship: bool) -> Box<Context> {
    if ship {
        Context::new_releasing()
    } else {
        Context::new()
    }
}
fn source(ctx: &mut Context, kind: u32) -> (*mut u8, CompletionEndpoint) {
    let mut endpoint = CompletionEndpoint::default();
    let handle = unsafe {
        subscript_rt_async_host_operation(
            ctx,
            if kind == completion_kind::VOID {
                0
            } else {
                size_of::<usize>() as u64
            },
            kind,
            42,
            [24u64, 0, 0, 8, 16, 0].as_ptr(),
            &mut endpoint,
        )
    };
    assert!(!handle.is_null());
    assert_eq!(unsafe { ctx.async_count(handle) }, 2);
    (handle, endpoint)
}
unsafe fn deliver(
    ctx: &mut Context,
    ep: CompletionEndpoint,
    kind: u32,
    bytes: *const u8,
    len: usize,
) -> CompletionStatus {
    unsafe {
        match kind {
            completion_kind::VALUE => subscript_rt_complete_value(ctx, ep, bytes.cast(), len),
            completion_kind::VOID => subscript_rt_complete_void(ctx, ep),
            completion_kind::STRING => subscript_rt_complete_string(ctx, ep, bytes.cast(), len),
            completion_kind::BYTES => subscript_rt_complete_bytes(ctx, ep, bytes, len),
            4 => subscript_rt_complete_error(ctx, ep, bytes.cast(), len),
            _ => unreachable!(),
        }
    }
}
fn pending(ctx: &mut Context, handle: *mut u8, before: usize) {
    assert_eq!(unsafe { ctx.async_count(handle) }, 2);
    assert_eq!(ctx.async_unfinished(), 1);
    assert_eq!(ctx.async_pending(), 0);
    assert_eq!(ctx.live_count(), before);
    let mut value = 0usize;
    assert!(!unsafe {
        ctx.async_result(
            handle,
            (&mut value as *mut usize).cast(),
            size_of::<usize>(),
        )
    });
    assert_eq!(value, 0);
    assert!(!ctx.trapped());
}
fn result(ctx: &mut Context, handle: *mut u8) -> *mut u8 {
    let mut out = std::ptr::null_mut::<u8>();
    assert!(unsafe {
        ctx.async_result(
            handle,
            (&mut out as *mut *mut u8).cast(),
            size_of::<usize>(),
        )
    });
    assert_eq!(unsafe { ctx.async_count(handle) }, 1);
    out
}
// The runtime array ABI contains five native words (§12).
#[repr(C)]
struct Array {
    len: u64,
    cap: u64,
    elem_size: u64,
    data: *mut u8,
    holders: u64,
}
fn buffer(ctx: &Context, value: *mut u8, kind: u32) -> &[u8] {
    assert!(!value.is_null());
    unsafe {
        if kind == completion_kind::STRING {
            ctx.str_bytes(value)
        } else {
            let a = &*value.cast::<Array>();
            assert_eq!(a.elem_size, 1);
            if a.len == 0 {
                &[]
            } else {
                std::slice::from_raw_parts(a.data, a.len as usize)
            }
        }
    }
}
fn observe_error(ctx: &mut Context, handle: *mut u8, text: &[u8]) {
    result(ctx, handle);
    let error = ctx.catch_exception();
    assert!(!error.is_null());
    let message = unsafe { error.add(16).cast::<*mut u8>().read_unaligned() };
    assert_eq!(unsafe { ctx.str_bytes(message) }, text);
}
#[test]
fn all_result_kind_pairs_have_unchanged_pending_sources_and_firing_controls() {
    for ship in [false, true] {
        for target in [
            completion_kind::VALUE,
            completion_kind::VOID,
            completion_kind::STRING,
            completion_kind::BYTES,
        ] {
            for api in [
                completion_kind::VALUE,
                completion_kind::VOID,
                completion_kind::STRING,
                completion_kind::BYTES,
            ] {
                let mut ctx = context(ship);
                let (handle, ep) = source(&mut ctx, target);
                let before = ctx.live_count();
                let data = 37usize.to_ne_bytes();
                let status = unsafe { deliver(&mut ctx, ep, api, data.as_ptr(), data.len()) };
                assert_eq!(
                    status,
                    if target == api {
                        CompletionStatus::Ok
                    } else {
                        CompletionStatus::Mismatch
                    }
                );
                if target != api {
                    pending(&mut ctx, handle, before);
                    assert_eq!(
                        unsafe { deliver(&mut ctx, ep, target, data.as_ptr(), data.len()) },
                        CompletionStatus::Ok
                    );
                }
                assert_eq!(unsafe { ctx.async_count(handle) }, 1);
                assert_eq!(ctx.async_unfinished(), 0);
            }
        }
    }
}
#[test]
fn input_statuses_allow_retry_and_length_checks_do_not_read_the_pointer() {
    for ship in [false, true] {
        for api in [completion_kind::STRING, completion_kind::BYTES, 4] {
            for invalid in [false, true] {
                if invalid && api == completion_kind::BYTES {
                    continue;
                }
                for raw in [&[255u8][..], &[128][..], &[226, 130][..]] {
                    let mut ctx = context(ship);
                    let (handle, ep) = source(
                        &mut ctx,
                        if api == 4 {
                            completion_kind::VALUE
                        } else {
                            api
                        },
                    );
                    let before = ctx.live_count();
                    let status = unsafe {
                        deliver(
                            &mut ctx,
                            ep,
                            api,
                            if invalid {
                                raw.as_ptr()
                            } else {
                                std::ptr::dangling()
                            },
                            if invalid {
                                raw.len()
                            } else {
                                i32::MAX as usize + 1
                            },
                        )
                    };
                    assert_eq!(
                        status,
                        if invalid {
                            CompletionStatus::InvalidUtf8
                        } else {
                            CompletionStatus::TooLarge
                        }
                    );
                    pending(&mut ctx, handle, before);
                    let good = b"valid";
                    assert_eq!(
                        unsafe { deliver(&mut ctx, ep, api, good.as_ptr(), good.len()) },
                        CompletionStatus::Ok
                    );
                    if api == 4 {
                        observe_error(&mut ctx, handle, good);
                    } else {
                        let value = result(&mut ctx, handle);
                        assert_eq!(buffer(&ctx, value, api), good);
                    }
                }
            }
        }
    }
}
#[test]
fn earlier_statuses_win_over_invalid_or_unreadable_input_with_firing_controls() {
    for ship in [false, true] {
        for api in [completion_kind::STRING, completion_kind::BYTES, 4] {
            let mut ctx = context(ship);
            let kind = if api == 4 {
                completion_kind::VALUE
            } else {
                api
            };
            let (handle, ep) = source(&mut ctx, kind);
            let before = ctx.live_count();
            let bad = CompletionEndpoint {
                context_id: 0,
                operation_id: ep.operation_id,
            };
            ctx.trap(TrapKind::DivisionByZero, "control", 0);
            assert_eq!(
                unsafe { deliver(&mut ctx, bad, api, std::ptr::dangling(), usize::MAX) },
                CompletionStatus::Trapped
            );
            assert_eq!(unsafe { ctx.async_count(handle) }, 2);
            ctx.clear_trap();
            assert_eq!(
                unsafe { deliver(&mut ctx, bad, api, std::ptr::dangling(), usize::MAX) },
                CompletionStatus::Stale
            );
            pending(&mut ctx, handle, before);
            assert_eq!(
                unsafe { deliver(&mut ctx, ep, api, b"x".as_ptr(), 1) },
                CompletionStatus::Ok
            );
            assert_eq!(
                unsafe { deliver(&mut ctx, ep, api, std::ptr::dangling(), usize::MAX) },
                CompletionStatus::Duplicate
            );
            assert_eq!(unsafe { ctx.async_count(handle) }, 1);
            if api == 4 {
                observe_error(&mut ctx, handle, b"x");
            } else {
                let value = result(&mut ctx, handle);
                assert_eq!(buffer(&ctx, value, api), b"x");
            }
            unsafe {
                ctx.async_release(handle, 0);
            }
            assert_eq!(
                unsafe { deliver(&mut ctx, ep, api, std::ptr::dangling(), usize::MAX) },
                CompletionStatus::Stale
            );
            let (_, control) = source(&mut ctx, kind);
            assert_eq!(
                unsafe { deliver(&mut ctx, control, api, b"x".as_ptr(), 1) },
                CompletionStatus::Ok
            );
            if api != 4 {
                let mut ctx = context(ship);
                let (handle, ep) = source(&mut ctx, completion_kind::VALUE);
                let before = ctx.live_count();
                assert_eq!(
                    unsafe { deliver(&mut ctx, ep, api, std::ptr::dangling(), usize::MAX) },
                    CompletionStatus::Mismatch
                );
                pending(&mut ctx, handle, before);
                let raw = 7usize.to_ne_bytes();
                assert_eq!(
                    unsafe {
                        deliver(
                            &mut ctx,
                            ep,
                            completion_kind::VALUE,
                            raw.as_ptr(),
                            raw.len(),
                        )
                    },
                    CompletionStatus::Ok
                );
            }
        }
    }
}
#[test]
fn each_allocation_failure_releases_partial_values_before_return() {
    for ship in [false, true] {
        for (api, allocations) in [
            (completion_kind::STRING, 1),
            (completion_kind::BYTES, 2),
            (4, 3),
        ] {
            for failure in 1..=allocations {
                for fail in [false, true] {
                    let mut ctx = context(ship);
                    let (handle, ep) = source(
                        &mut ctx,
                        if api == 4 {
                            completion_kind::VALUE
                        } else {
                            api
                        },
                    );
                    let (before, bytes) = (ctx.live_count(), ctx.live_bytes());
                    ctx.fail_alloc_after(if fail { failure } else { 0 });
                    assert_eq!(
                        unsafe { deliver(&mut ctx, ep, api, b"abc".as_ptr(), 3) },
                        if fail {
                            CompletionStatus::Trapped
                        } else {
                            CompletionStatus::Ok
                        }
                    );
                    if fail {
                        assert_eq!(ctx.trap_record().unwrap().kind, TrapKind::AllocationFailure);
                        assert_eq!(ctx.live_count(), before);
                        assert_eq!(ctx.live_bytes(), bytes);
                        assert_eq!(unsafe { ctx.async_count(handle) }, 2);
                        ctx.clear_trap();
                        pending(&mut ctx, handle, before);
                        assert_eq!(
                            unsafe { deliver(&mut ctx, ep, api, b"abc".as_ptr(), 3) },
                            CompletionStatus::Ok
                        );
                    }
                    if api == 4 {
                        observe_error(&mut ctx, handle, b"abc");
                    } else {
                        let value = result(&mut ctx, handle);
                        assert_eq!(buffer(&ctx, value, api), b"abc");
                    }
                }
            }
        }
    }
}
#[test]
fn copied_empty_and_arbitrary_bytes_survive_collection_and_source_release() {
    for ship in [false, true] {
        for kind in [completion_kind::STRING, completion_kind::BYTES] {
            for empty in [false, true] {
                let mut ctx = context(ship);
                let (handle, ep) = source(&mut ctx, kind);
                let mut input = if kind == completion_kind::STRING {
                    b"hello".to_vec()
                } else {
                    vec![0, 128, 255]
                };
                let expected = if empty { vec![] } else { input.clone() };
                assert_eq!(
                    unsafe {
                        deliver(
                            &mut ctx,
                            ep,
                            kind,
                            if empty {
                                std::ptr::null()
                            } else {
                                input.as_ptr()
                            },
                            expected.len(),
                        )
                    },
                    CompletionStatus::Ok
                );
                input.fill(b'x');
                ctx.collect();
                let value = result(&mut ctx, handle);
                assert_eq!(buffer(&ctx, value, kind), expected);
                unsafe {
                    ctx.async_release(handle, 0);
                }
                assert_eq!(buffer(&ctx, value, kind), expected);
                ctx.collect();
                assert_eq!(ctx.live_count(), 0);
            }
            for early in [false, true] {
                let mut ctx = context(ship);
                let (handle, ep) = source(&mut ctx, kind);
                if early {
                    unsafe {
                        ctx.async_release(handle, 0);
                    }
                }
                assert_eq!(
                    unsafe { deliver(&mut ctx, ep, kind, b"abc".as_ptr(), 3) },
                    CompletionStatus::Ok
                );
                if !early {
                    unsafe {
                        ctx.async_release(handle, 0);
                    }
                }
                assert_eq!(ctx.live_count(), 0, "dropped value needs no collection");
                assert_eq!(ctx.async_unfinished(), 0);
            }
        }
    }
}
#[test]
fn aggregate_results_survive_the_last_input_source_release() {
    for ship in [false, true] {
        for kind in [completion_kind::STRING, completion_kind::BYTES] {
            let mut ctx = context(ship);
            let jobs = ctx.array_with_capacity(2, size_of::<usize>(), 0);
            let header = unsafe { &mut *jobs.cast::<Array>() };
            header.len = 2;
            for i in 0..2 {
                let (handle, ep) = source(&mut ctx, kind);
                assert_eq!(
                    unsafe { deliver(&mut ctx, ep, kind, b"abc".as_ptr(), 3) },
                    CompletionStatus::Ok
                );
                unsafe {
                    header.data.cast::<*mut u8>().add(i).write(handle);
                }
            }
            let aggregate =
                unsafe { ctx.async_all(jobs, size_of::<usize>(), size_of::<usize>(), 0) };
            for i in 0..2 {
                let handle = unsafe { header.data.cast::<*mut u8>().add(i).read() };
                unsafe {
                    ctx.async_release(handle, 0);
                }
            }
            unsafe {
                ctx.async_step();
            }
            ctx.collect();
            let mut out = std::ptr::null_mut::<u8>();
            assert!(unsafe {
                ctx.async_result(
                    aggregate,
                    (&mut out as *mut *mut u8).cast(),
                    size_of::<usize>(),
                )
            });
            let array = unsafe { &*out.cast::<Array>() };
            for i in 0..2 {
                let value = unsafe { array.data.cast::<*mut u8>().add(i).read() };
                assert_eq!(buffer(&ctx, value, kind), b"abc");
            }
            unsafe {
                ctx.async_release(aggregate, 0);
            }
            ctx.collect();
            assert_eq!(ctx.live_count(), 0);
        }
    }
}

#[test]
fn ten_thousand_completions_per_kind_balance_counts_and_allocations() {
    for ship in [false, true] {
        for kind in [completion_kind::STRING, completion_kind::BYTES] {
            let mut ctx = context(ship);
            let before = ctx.live_count();
            for _ in 0..10_000 {
                let (handle, ep) = source(&mut ctx, kind);
                assert_eq!(
                    unsafe { deliver(&mut ctx, ep, kind, b"abc".as_ptr(), 3) },
                    CompletionStatus::Ok
                );
                let value = result(&mut ctx, handle);
                assert_eq!(buffer(&ctx, value, kind), b"abc");
                unsafe {
                    ctx.async_release(handle, 0);
                }
            }
            assert_eq!(ctx.async_unfinished(), 0);
            ctx.collect();
            assert_eq!(ctx.live_count(), before);
            assert_eq!(ctx.live_bytes(), 0);
        }
    }
}
#[test]
#[ignore = "release-only copy cost probe; no timing threshold"]
fn one_mib_copy_cost() {
    let input = vec![b'a'; 1 << 20];
    for ship in [false, true] {
        for kind in [completion_kind::STRING, completion_kind::BYTES] {
            let mut samples = Vec::new();
            for _ in 0..3 {
                let mut ctx = context(ship);
                let (handle, ep) = source(&mut ctx, kind);
                let start = std::time::Instant::now();
                let status = unsafe { deliver(&mut ctx, ep, kind, input.as_ptr(), input.len()) };
                samples.push(start.elapsed().as_nanos());
                assert_eq!(status, CompletionStatus::Ok);
                let value = result(&mut ctx, handle);
                assert_eq!(buffer(&ctx, value, kind), input);
                unsafe {
                    ctx.async_release(handle, 0);
                }
                ctx.collect();
                assert_eq!(ctx.live_count(), 0);
            }
            println!(
                "ship={ship} kind={kind} ns={samples:?} best={}",
                samples.iter().min().unwrap()
            );
        }
    }
}

#[test]
fn discarded_await_releases_copied_buffers_with_observed_value_controls() {
    // Cost: eight small sources; isolated test binary: 0.008046 s, including startup, excluding the Rust build.
    for ship in [false, true] {
        for kind in [completion_kind::STRING, completion_kind::BYTES] {
            for take_value in [false, true] {
                let mut ctx = context(ship);
                let (handle, ep) = source(&mut ctx, kind);
                assert_eq!(
                    unsafe { deliver(&mut ctx, ep, kind, b"abc".as_ptr(), 3) },
                    CompletionStatus::Ok
                );
                let value = if take_value {
                    result(&mut ctx, handle)
                } else {
                    assert!(unsafe { ctx.async_result(handle, std::ptr::null_mut(), 0) });
                    std::ptr::null_mut()
                };
                unsafe { ctx.async_release(handle, 0) };
                if take_value {
                    assert!(ctx.is_live(value as usize));
                    assert_eq!(buffer(&ctx, value, kind), b"abc");
                    ctx.collect();
                }
                assert_eq!(ctx.live_count(), 0);
                assert_eq!(ctx.live_bytes(), 0);
                assert!(!ctx.trapped());
            }
        }
    }
}

#[test]
fn bytes_error_drop_traps_with_await_controls() {
    // Cost: four small sources; isolated test binary: 0.003396 s, including startup, excluding the Rust build.
    for ship in [false, true] {
        for observe in [false, true] {
            let mut ctx = context(ship);
            let (handle, ep) = source(&mut ctx, completion_kind::BYTES);
            assert_eq!(
                unsafe {
                    subscript_rt_complete_error(&mut *ctx, ep, b"failure".as_ptr().cast(), 7)
                },
                CompletionStatus::Ok
            );
            if observe {
                observe_error(&mut ctx, handle, b"failure");
            }
            unsafe { ctx.async_release(handle, 0) };
            assert!(!ctx.is_live(handle as usize));
            if observe {
                assert!(!ctx.trapped());
            } else {
                let trap = ctx.trap_record().expect("unobserved Error traps");
                assert_eq!(trap.kind, TrapKind::UncaughtException);
                assert_eq!(trap.kind as u32, 29);
                assert_eq!(trap.pos_id, 42);
                assert_eq!(trap.message, "Error: failure");
            }
        }
    }
}

#[test]
fn admitted_length_boundary_reaches_allocation_failure_without_reading_input() {
    // Cost: two sources; isolated test binary: 0.009798 s, including startup, excluding the Rust build.
    // Injected header failure prevents a 2 GiB allocation or input read.
    for ship in [false, true] {
        let mut ctx = context(ship);
        let (handle, ep) = source(&mut ctx, completion_kind::BYTES);
        let before = (ctx.live_count(), ctx.live_bytes());
        ctx.fail_alloc_after(1);
        assert_eq!(
            unsafe {
                subscript_rt_complete_bytes(&mut *ctx, ep, std::ptr::dangling(), i32::MAX as usize)
            },
            CompletionStatus::Trapped
        );
        assert_eq!(ctx.trap_record().unwrap().kind, TrapKind::AllocationFailure);
        assert_eq!((ctx.live_count(), ctx.live_bytes()), before);
        ctx.clear_trap();
        pending(&mut ctx, handle, before.0);
        assert_eq!(
            unsafe { subscript_rt_complete_bytes(&mut *ctx, ep, b"abc".as_ptr(), 3) },
            CompletionStatus::Ok
        );
        let value = result(&mut ctx, handle);
        assert_eq!(buffer(&ctx, value, completion_kind::BYTES), b"abc");
    }
}
