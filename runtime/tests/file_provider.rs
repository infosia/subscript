//! File-provider ABI witnesses use two allocators and no external I/O.
use std::ffi::c_void;
use subscript_runtime::context::{completion_kind, CompletionEndpoint, CompletionStatus};
use subscript_runtime::{ffi::*, Context};

#[derive(Default)]
struct Request {
    path: Vec<u8>,
    data: Vec<u8>,
    form: u32,
    endpoint: CompletionEndpoint,
    deferred: bool,
    /// The path and data handles that the operation must root.
    rooted: [usize; 2],
    /// A handle that nothing roots: the firing control of the rooting check.
    unrooted: usize,
    /// After the callback's collection: path live, data live, control live.
    live: Option<(bool, bool, bool)>,
}
/// Records which handles survive the collection inside a callback.
unsafe fn record_live(ctx: *mut Context, request: &mut Request) {
    let ctx = unsafe { &*ctx };
    let [path, data] = request.rooted;
    if path != 0 {
        request.live = Some((
            ctx.is_live(path),
            data == 0 || ctx.is_live(data),
            ctx.is_live(request.unrooted),
        ));
    }
}
unsafe extern "C" fn read(
    ctx: *mut Context,
    user: *mut c_void,
    path: *const u8,
    n: usize,
    form: u32,
    ep: CompletionEndpoint,
) {
    let request = unsafe { &mut *user.cast::<Request>() };
    // The runtime must root the borrowed input through callback return.
    unsafe {
        (*ctx).collect();
        record_live(ctx, request);
    }
    request.path = unsafe { std::slice::from_raw_parts(path, n) }.to_vec();
    request.form = form;
    request.endpoint = ep;
    if !request.deferred {
        let status = if form == 0 {
            unsafe { subscript_rt_complete_string(ctx, ep, b"result".as_ptr().cast(), 6) }
        } else {
            unsafe { subscript_rt_complete_bytes(ctx, ep, [0, 128, 255].as_ptr(), 3) }
        };
        assert_eq!(status, CompletionStatus::Ok);
    }
}
unsafe extern "C" fn write(
    ctx: *mut Context,
    user: *mut c_void,
    path: *const u8,
    n: usize,
    data: *const u8,
    length: usize,
    form: u32,
    ep: CompletionEndpoint,
) {
    let request = unsafe { &mut *user.cast::<Request>() };
    unsafe {
        (*ctx).collect();
        record_live(ctx, request);
    }
    request.path = unsafe { std::slice::from_raw_parts(path, n) }.to_vec();
    // The data pointer can be NULL when the length is 0.
    request.data = if length == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(data, length) }.to_vec()
    };
    request.form = form;
    request.endpoint = ep;
    assert_eq!(
        unsafe { subscript_rt_complete_void(ctx, ep) },
        CompletionStatus::Ok
    );
}
fn source(ctx: &mut Context, kind: u32) -> (*mut u8, CompletionEndpoint) {
    let mut ep = CompletionEndpoint::default();
    let handle = unsafe {
        subscript_rt_async_host_operation(
            ctx,
            if kind == completion_kind::VOID { 0 } else { 8 },
            kind,
            42,
            [24, 0, 0, 8, 16, 0].as_ptr(),
            &mut ep,
        )
    };
    assert!(!handle.is_null());
    (handle, ep)
}
fn install(ctx: &mut Context, request: &mut Request) {
    let mut provider = FileProvider::default();
    assert_eq!(provider.size, std::mem::size_of::<FileProvider>());
    provider.userdata = (request as *mut Request).cast();
    provider.read = Some(read);
    provider.write = Some(write);
    unsafe {
        subscript_rt_ctx_set_file_provider(ctx, &provider);
    }
}

#[test]
fn all_four_operations_deliver_the_built_path_data_and_form() {
    for ship in [false, true] {
        for operation in 0..4 {
            let mut ctx = if ship {
                Context::new_releasing()
            } else {
                Context::new()
            };
            let mut request = Request::default();
            install(&mut ctx, &mut request);
            let expected_path = "dir/日本.txt".as_bytes();
            let path = ctx.alloc_str(expected_path, 0);
            let expected_data = if operation == 2 {
                "文字".as_bytes()
            } else {
                &[0, 128, 255]
            };
            let data = if operation == 2 {
                ctx.alloc_str(expected_data, 0)
            } else if operation == 3 {
                let array = ctx.array_new(1, 0);
                for byte in expected_data {
                    unsafe {
                        ctx.array_push(array, byte, 0);
                    }
                }
                array
            } else {
                std::ptr::null_mut()
            };
            let kind = match operation {
                0 => completion_kind::STRING,
                1 => completion_kind::BYTES,
                _ => completion_kind::VOID,
            };
            let (handle, ep) = source(&mut ctx, kind);
            request.rooted = [path as usize, data as usize];
            request.unrooted = ctx.alloc_str(b"unrooted", 0) as usize;
            unsafe {
                subscript_rt_file_operation(&mut *ctx, operation, path, data, &ep);
            }
            // The operation roots the path and data; the control is freed.
            assert_eq!(request.live, Some((true, true, false)));
            assert_eq!(request.path, expected_path);
            assert_eq!(request.form, operation % 2);
            assert_eq!(request.endpoint, ep);
            if operation >= 2 {
                assert_eq!(request.data, expected_data);
            }
            assert_eq!(unsafe { ctx.async_count(handle) }, 1);
            let mut result: usize = 0;
            assert_eq!(
                unsafe {
                    subscript_rt_async_result_uncounted(
                        &mut *ctx,
                        handle,
                        (&mut result as *mut usize).cast(),
                        if kind == completion_kind::VOID { 0 } else { 8 },
                    )
                },
                1
            );
            if operation == 0 {
                assert_eq!(unsafe { ctx.str_bytes(result as *const u8) }, b"result");
            }
            if operation == 1 {
                assert_eq!(unsafe { ctx.array_len(result as *const u8) }, 3);
                assert_eq!(
                    unsafe { std::slice::from_raw_parts(ctx.array_data(result as *const u8), 3) },
                    [0, 128, 255]
                );
            }
            unsafe {
                subscript_rt_async_release(&mut *ctx, handle, 42);
            }
        }
    }
}

#[test]
fn deferred_request_keeps_its_source_after_removal_and_invalid_completions() {
    for ship in [false, true] {
        let mut ctx = if ship {
            Context::new_releasing()
        } else {
            Context::new()
        };
        let mut request = Request {
            deferred: true,
            ..Default::default()
        };
        install(&mut ctx, &mut request);
        let path = ctx.alloc_str(b"pending", 0);
        let (handle, ep) = source(&mut ctx, completion_kind::STRING);
        unsafe {
            subscript_rt_file_operation(&mut *ctx, 0, path, std::ptr::null(), &ep);
        }
        unsafe {
            subscript_rt_ctx_set_file_provider(&mut *ctx, std::ptr::null());
        }
        ctx.collect();
        assert_eq!(request.path, b"pending");
        assert_eq!(
            unsafe {
                subscript_rt_complete_string(&mut *ctx, request.endpoint, [255].as_ptr().cast(), 1)
            },
            CompletionStatus::InvalidUtf8
        );
        assert_eq!(unsafe { ctx.async_count(handle) }, 2);
        assert_eq!(ctx.async_unfinished(), 1);
        assert_eq!(
            unsafe {
                subscript_rt_complete_string(
                    &mut *ctx,
                    request.endpoint,
                    std::ptr::null(),
                    i32::MAX as usize + 1,
                )
            },
            CompletionStatus::TooLarge
        );
        assert_eq!(unsafe { ctx.async_count(handle) }, 2);
        assert_eq!(ctx.async_unfinished(), 1);
        assert_eq!(
            unsafe {
                subscript_rt_complete_string(
                    &mut *ctx,
                    request.endpoint,
                    b"later".as_ptr().cast(),
                    5,
                )
            },
            CompletionStatus::Ok
        );
        assert_eq!(ctx.async_unfinished(), 0);
        assert_eq!(unsafe { ctx.async_count(handle) }, 1);
        unsafe {
            subscript_rt_async_release(&mut *ctx, handle, 42);
        }
    }
}

#[test]
fn absent_callbacks_and_short_records_complete_with_a_catchable_error() {
    for short in [false, true] {
        let mut ctx = Context::new();
        let mut provider = FileProvider::default();
        if short {
            provider.size = 0;
        }
        unsafe {
            subscript_rt_ctx_set_file_provider(&mut *ctx, &provider);
        }
        let path = ctx.alloc_str(b"file", 0);
        let (handle, ep) = source(&mut ctx, completion_kind::STRING);
        unsafe {
            subscript_rt_file_operation(&mut *ctx, 0, path, std::ptr::null(), &ep);
        }
        let mut result: usize = 0;
        assert_eq!(
            unsafe {
                subscript_rt_async_result_uncounted(
                    &mut *ctx,
                    handle,
                    (&mut result as *mut usize).cast(),
                    8,
                )
            },
            1
        );
        assert!(ctx.exception_pending());
        assert!(!ctx.catch_exception().is_null());
        assert_eq!(ctx.async_unfinished(), 0);
        unsafe {
            subscript_rt_async_release(&mut *ctx, handle, 42);
        }
        assert!(!ctx.trapped());
    }
}

#[repr(C)]
struct WorkerReply {
    provider_error: u64,
}
unsafe extern "C" fn no_init(_ctx: *mut Context) {}
/// Starts a text read on the Worker Context and posts whether it completed
/// with the rule 5 `Error`.
unsafe extern "C" fn worker_read(
    ctx: *mut Context,
    _inbox: *mut subscript_runtime::WorkerInbox,
    outbox: *mut subscript_runtime::WorkerOutbox,
) {
    let worker = unsafe { &mut *ctx };
    let path = worker.alloc_str(b"file", 0);
    let (handle, ep) = source(worker, completion_kind::STRING);
    unsafe {
        subscript_rt_file_operation(ctx, 0, path, std::ptr::null(), &ep);
    }
    let mut result: usize = 0;
    let ready = unsafe {
        subscript_rt_async_result_uncounted(ctx, handle, (&mut result as *mut usize).cast(), 8)
    };
    let mut message = Vec::new();
    if ready == 1 && worker.exception_pending() {
        let object = worker.catch_exception();
        // The source's Error layout puts the message handle at offset 16.
        let text = unsafe { object.add(16).cast::<*const u8>().read() };
        message = unsafe { worker.str_bytes(text) }.to_vec();
    }
    unsafe {
        subscript_rt_async_release(ctx, handle, 42);
    }
    let reply = WorkerReply {
        provider_error: u64::from(message == b"missing file provider"),
    };
    unsafe {
        // A failed post leaves the parent poll empty, which the test reports.
        let _ = subscript_rt_worker_outbox_post(ctx, outbox, (&reply as *const WorkerReply).cast());
    }
}

/// Rule 5: a Worker Context has no provider, also when its parent has one.
#[test]
fn a_file_call_in_a_worker_completes_with_the_missing_provider_error() {
    let mut parent = Context::new();
    let mut request = Request::default();
    install(&mut parent, &mut request);
    let descriptor = subscript_runtime::worker::WorkerMessageDescriptor {
        payload_size: std::mem::size_of::<WorkerReply>() as u64,
        string_slot_count: 0,
        string_slot_offsets: std::ptr::null(),
    };
    let worker = unsafe {
        subscript_rt_worker_spawn(
            &mut *parent,
            Some(no_init),
            Some(worker_read),
            &descriptor,
            &descriptor,
        )
    };
    assert!(!worker.is_null());
    assert_eq!(unsafe { subscript_rt_worker_join(&mut *parent, worker) }, 1);
    let reply = unsafe { subscript_rt_worker_poll(&mut *parent, worker) };
    assert!(!reply.is_null());
    assert_eq!(
        unsafe { reply.cast::<WorkerReply>().read() }.provider_error,
        1
    );
    // The parent provider never ran.
    assert!(request.path.is_empty());
    // The firing control: the same read on the parent Context reaches its provider.
    let path = parent.alloc_str(b"file", 0);
    let (handle, ep) = source(&mut parent, completion_kind::STRING);
    unsafe {
        subscript_rt_file_operation(&mut *parent, 0, path, std::ptr::null(), &ep);
        subscript_rt_async_release(&mut *parent, handle, 42);
    }
    assert_eq!(request.path, b"file");
}

/// Returns whether a text read on `ctx` reached the provider in `request`.
/// `request` is the provider's userdata pointer. The callback writes through
/// it, so the helper holds no `&mut Request` across the operation.
fn read_reaches(ctx: &mut Context, request: *mut Request) -> bool {
    unsafe { (*request).path.clear() };
    let path = ctx.alloc_str(b"file", 0);
    let (handle, ep) = source(ctx, completion_kind::STRING);
    unsafe {
        subscript_rt_file_operation(&mut *ctx, 0, path, std::ptr::null(), &ep);
    }
    let mut result: usize = 0;
    unsafe {
        subscript_rt_async_result_uncounted(
            &mut *ctx,
            handle,
            (&mut result as *mut usize).cast(),
            8,
        );
    }
    if ctx.exception_pending() {
        let _ = ctx.catch_exception();
    }
    unsafe {
        subscript_rt_async_release(&mut *ctx, handle, 42);
    }
    !unsafe { &*request }.path.is_empty()
}

#[test]
fn the_rust_setter_installs_removes_and_refuses_a_short_record() {
    let mut ctx = Context::new();
    let mut storage = Request::default();
    // One pointer serves the provider and the helper.
    let request: *mut Request = &mut storage;
    let mut provider = FileProvider::default();
    provider.userdata = request.cast();
    provider.read = Some(read);
    provider.write = Some(write);
    unsafe { ctx.set_file_provider(Some(provider)) };
    assert!(read_reaches(&mut ctx, request));
    unsafe { ctx.set_file_provider(None) };
    assert!(!read_reaches(&mut ctx, request));
    let mut short = provider;
    short.size = std::mem::size_of::<FileProvider>() - 1;
    unsafe { ctx.set_file_provider(Some(short)) };
    assert!(!read_reaches(&mut ctx, request));
}

/// Reads and writes files beneath `root` for the cost driver.
struct DiskProvider {
    root: std::path::PathBuf,
}
fn disk_path(user: *mut c_void, path: *const u8, n: usize) -> std::path::PathBuf {
    let provider = unsafe { &*user.cast::<DiskProvider>() };
    let name = std::str::from_utf8(unsafe { std::slice::from_raw_parts(path, n) }).expect("path");
    provider.root.join(name)
}
unsafe extern "C" fn disk_read(
    ctx: *mut Context,
    user: *mut c_void,
    path: *const u8,
    n: usize,
    form: u32,
    ep: CompletionEndpoint,
) {
    let bytes = std::fs::read(disk_path(user, path, n)).expect("read fixture");
    let status = if form == 0 {
        unsafe { subscript_rt_complete_string(ctx, ep, bytes.as_ptr().cast(), bytes.len()) }
    } else {
        unsafe { subscript_rt_complete_bytes(ctx, ep, bytes.as_ptr(), bytes.len()) }
    };
    assert_eq!(status, CompletionStatus::Ok);
}
unsafe extern "C" fn disk_write(
    ctx: *mut Context,
    user: *mut c_void,
    path: *const u8,
    n: usize,
    data: *const u8,
    length: usize,
    _form: u32,
    ep: CompletionEndpoint,
) {
    let data = if length == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(data, length) }
    };
    std::fs::write(disk_path(user, path, n), data).expect("write fixture");
    assert_eq!(
        unsafe { subscript_rt_complete_void(ctx, ep) },
        CompletionStatus::Ok
    );
}

/// The §185 cost record: one 1 MiB read and write per operation and Context
/// form. Each sample times one `subscript_rt_file_operation` call on a fresh
/// Context and source: dispatch, the provider's file I/O, and the completion.
#[test]
#[ignore = "Measures 1 MiB file operation cost; run explicitly in a release build for the cost record."]
fn one_mib_file_operation_cost() {
    const SAMPLES: usize = 11;
    let root = std::env::temp_dir().join(format!("subscript-file-cost-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create cost directory");
    let fixture = vec![b'a'; 1 << 20];
    std::fs::write(root.join("fixture.txt"), &fixture).expect("write cost fixture");
    let mut disk = DiskProvider { root: root.clone() };
    let names = ["read text", "read bytes", "write text", "write bytes"];
    for ship in [false, true] {
        for operation in 0..4u32 {
            let mut samples = Vec::with_capacity(SAMPLES);
            for _ in 0..SAMPLES {
                let mut ctx = if ship {
                    Context::new_releasing()
                } else {
                    Context::new()
                };
                let mut provider = FileProvider::default();
                provider.userdata = (&mut disk as *mut DiskProvider).cast();
                provider.read = Some(disk_read);
                provider.write = Some(disk_write);
                unsafe { ctx.set_file_provider(Some(provider)) };
                let name: &[u8] = if operation < 2 {
                    b"fixture.txt"
                } else {
                    b"output.bin"
                };
                let path = ctx.alloc_str(name, 0);
                let data = match operation {
                    2 => ctx.alloc_str(&fixture, 0),
                    3 => {
                        let array = ctx.array_new(1, 0);
                        for byte in &fixture {
                            unsafe { ctx.array_push(array, byte, 0) };
                        }
                        array
                    }
                    _ => std::ptr::null_mut(),
                };
                let kind = match operation {
                    0 => completion_kind::STRING,
                    1 => completion_kind::BYTES,
                    _ => completion_kind::VOID,
                };
                let (handle, ep) = source(&mut ctx, kind);
                let started = std::time::Instant::now();
                unsafe {
                    subscript_rt_file_operation(&mut *ctx, operation, path, data, &ep);
                }
                samples.push(started.elapsed().as_secs_f64() * 1e6);
                assert_eq!(ctx.async_unfinished(), 0);
                unsafe { subscript_rt_async_release(&mut *ctx, handle, 42) };
            }
            samples.sort_by(f64::total_cmp);
            eprintln!(
                "s185 cost: {} {}: median {:.3} us, minimum {:.3} us",
                if ship { "ship" } else { "dev" },
                names[operation as usize],
                samples[SAMPLES / 2],
                samples[0]
            );
        }
    }
    std::fs::remove_dir_all(root).expect("remove cost directory");
}
