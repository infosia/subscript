use crate::context::AsyncResume;
use crate::context::Context;

/// Runs a compiler-created async root to its first suspension or completion.
/// This is an internal generated-code ABI; embedding hosts use the
/// `subscript_rt_ctx_async_*` driver functions below.
///
/// # Safety
///
/// Shared contract; `frame` and `resume` are a matching generated pair.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_kick(
    ctx: *mut Context,
    frame: *mut u8,
    resume: Option<AsyncResume>,
) {
    let Some(resume) = resume else { return };
    // SAFETY: shared contract and the caller supplies a matching pair.
    unsafe { &mut *ctx }.async_kick(frame, resume);
}

/// Registers a freshly allocated async frame and initializes its count to one.
///
/// # Safety
///
/// Shared contract; `frame` is a fresh live generated async frame owned by `ctx`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_register(
    ctx: *mut Context,
    frame: *mut u8,
    result_size: u64,
) {
    unsafe { &mut *ctx }.async_register(frame, result_size as usize);
}

/// Parks a frame suspended at `Context.suspend()` (`compiler.md` §94.1
/// rule 3). The next host checkpoint makes it runnable.
///
/// # Safety
///
/// Shared contract; `frame` is a registered async frame owned by `ctx`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_park(ctx: *mut Context, frame: *mut u8, pos_id: u32) {
    unsafe { &mut *ctx }.async_park(frame, pos_id);
}

/// Registers `frame` as a continuation of `handle` (`compiler.md` §94.1
/// rules 4 to 6).
///
/// # Safety
///
/// Shared contract; both are registered async frames owned by `ctx`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_await(
    ctx: *mut Context,
    frame: *mut u8,
    handle: *mut u8,
    pos_id: u32,
) {
    unsafe { &mut *ctx }.async_await(frame, handle, pos_id);
}

/// Moves the call's handle count to an await registration (§116.1 rule 4a).
///
/// # Safety
///
/// Shared contract; both frames are live. The caller transfers one handle count.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_await_owned(
    ctx: *mut Context,
    frame: *mut u8,
    handle: *mut u8,
    pos_id: u32,
) {
    unsafe { &mut *ctx }.async_await_owned(frame, handle, pos_id);
}

/// Reports a scheduled await resume whose awaited handle carries no
/// completion (`compiler.md` §94.1). This is an internal protocol defect,
/// never a source-language trap, and no consumer recovers from it.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_missing_completion(ctx: *mut Context, pos_id: u32) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.async_missing_completion(pos_id);
}

/// Copies a held async handle, incrementing its frame count.
///
/// # Safety
///
/// Shared contract; `frame` is a registered live async frame owned by `ctx`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_retain(ctx: *mut Context, frame: *mut u8) {
    unsafe { &mut *ctx }.async_retain(frame);
}

/// Ends one held async-handle ownership scope.
///
/// # Safety
///
/// Shared contract; `frame` is a registered async frame and the caller owns
/// one reference to it.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_release(
    ctx: *mut Context,
    frame: *mut u8,
    pos_id: u32,
) {
    unsafe { &mut *ctx }.async_release(frame, pos_id);
}

/// Releases every held async handle stored in a dynamic array.
///
/// # Safety
///
/// Shared contract; `array` is null or a live dynamic array of registered
/// async-frame pointers owned by `ctx`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_release_array(
    ctx: *mut Context,
    array: *const u8,
    pos_id: u32,
) {
    if array.is_null() {
        return;
    }
    let runtime = unsafe { &mut *ctx };
    let len = unsafe { runtime.array_len(array) }.max(0) as usize;
    let data = unsafe { runtime.array_data(array) };
    for index in 0..len {
        // Async handles are pointer-sized scalar array elements.
        let frame = unsafe { (data.add(index * 8) as *const *mut u8).read_unaligned() };
        unsafe { runtime.async_release(frame, pos_id) };
    }
}

/// Retains every held async handle stored in a dynamic array.
///
/// # Safety
///
/// Shared contract; `array` is null or a live dynamic array of registered
/// async-frame pointers owned by `ctx`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_retain_array(ctx: *mut Context, array: *const u8) {
    if array.is_null() {
        return;
    }
    let runtime = unsafe { &mut *ctx };
    let len = unsafe { runtime.array_len(array) }.max(0) as usize;
    let data = unsafe { runtime.array_data(array) };
    for index in 0..len {
        let frame = unsafe { (data.add(index * 8) as *const *mut u8).read_unaligned() };
        unsafe { runtime.async_retain(frame) };
    }
}

/// Returns one when a reload-mode frame predates the current Context epoch.
///
/// # Safety
///
/// Shared contract; `frame` is a registered async frame owned by `ctx`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_is_stale(ctx: *const Context, frame: *const u8) -> u8 {
    u8::from(unsafe { &*ctx }.async_is_stale(frame))
}

/// Stores the fulfilled representation produced by the first held await.
///
/// # Safety
///
/// Shared contract; `frame` is registered in `ctx`, and `value` points to
/// `size` readable bytes when `size` is nonzero.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_complete(
    ctx: *mut Context,
    frame: *mut u8,
    value: *const u8,
    size: u64,
) {
    unsafe { &mut *ctx }.async_complete(frame, value, size as usize);
}

/// Copies the cached fulfilled representation for a later held await.
/// An exception completion becomes the pending exception instead
/// (`compiler.md` §116.1 rule 2).
///
/// # Safety
///
/// Shared contract; `frame` is registered in `ctx`, and `out` points to
/// `size` writable bytes when `size` is nonzero.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_result(
    ctx: *mut Context,
    frame: *const u8,
    out: *mut u8,
    size: u64,
) -> u8 {
    u8::from(unsafe { &mut *ctx }.async_result(frame, out, size as usize))
}

/// Creates an aggregate handle from an array snapshot (§166).
///
/// # Safety
/// Shared contract; `jobs` is a live handle array with non-counted results of `elem_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_all(
    ctx: *mut Context,
    jobs: *const u8,
    elem_size: u64,
    pos_id: u32,
) -> *mut u8 {
    unsafe { (&mut *ctx).async_all(jobs, elem_size as usize, pos_id) }
}

/// Starts a called async invocation and exposes ACTIVE during its body (§169).
/// `pos_id` identifies the creation call site.
///
/// # Safety
/// The frame has a live generated resume pointer. The output matches its result representation.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_start(
    ctx: *mut Context,
    frame: *mut u8,
    out: *mut u8,
    pos_id: u32,
) -> u8 {
    unsafe { (&mut *ctx).async_start(frame, out, pos_id) }
}

/// Executes an internal group operation: create=0, add=1, join=2, release=3 (§170).
///
/// # Safety
/// `ctx` is live. Add transfers one live void task count; join borrows the group.
/// Release ends its unique lexical owner. Create ignores both pointers.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_task_group(
    ctx: *mut Context,
    operation: u32,
    group: *mut u8,
    input: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    let ctx = unsafe { &mut *ctx };
    match operation {
        0 => ctx.task_group_create(pos_id),
        1 => {
            unsafe { ctx.task_group_add(group, input, pos_id) };
            std::ptr::null_mut()
        }
        2 => unsafe { ctx.task_group_join(group, pos_id) },
        3 => {
            unsafe { ctx.task_group_release(group, pos_id) };
            std::ptr::null_mut()
        }
        _ => {
            ctx.trap(
                crate::TrapKind::Internal,
                "unknown task group operation",
                pos_id,
            );
            std::ptr::null_mut()
        }
    }
}
