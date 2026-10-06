//! The C-ABI boundary called by generated code.
//!
//! Every function here is `extern "C"` with a stable signature; the
//! code generator declares them by name and the JIT resolves them by
//! symbol registration. Guarantee: no unwinding ever crosses this
//! boundary — script faults are reported through the Context trap
//! state, never through panics, and *Context* allocation failure is
//! a trap. Host-heap exhaustion inside `format!`/`Vec` growth aborts
//! the process (Rust's default OOM behavior); that is the accepted
//! FFI-boundary exception of CLAUDE.md core principle 5, not an
//! unwind.
//!
//! Shared safety contract (each function's `# Safety` builds on it):
//! `ctx` is the non-null Context of the current script run, created by
//! [`Context::new`] and passed to the script entry by the driver;
//! handles were produced by this context's allocation functions and
//! the script ran under the emitted trap-check discipline, so a null
//! result from a trapping function is never fed into another call.

use crate::context::{
    AllocationVisitor, AsyncStepReport, Context, DiagnosticsObserver, PrintObserver, TrapObserver,
};
use crate::trap::TrapKind;
use crate::worker::{Worker, WorkerEntry, WorkerInbox, WorkerInit, WorkerOutbox};

/// A `(ptr, len)` string view, ABI-identical to the synthetic header's
/// `SubStringView` (`{ const char*; size_t; }`) and to the language's
/// own string representation (Q5). It is the by-value first argument the
/// C callback ABI hands [`subscript_rt_cb_trampoline`].
#[repr(C)]
pub struct SubStrView {
    /// UTF-8 bytes; no NUL terminator assumed.
    pub data: *const u8,
    /// Byte length.
    pub len: usize,
}

#[macro_use]
mod conversions;
mod array_methods;
mod arrays;
mod associations;
mod async_frames;
mod boundary;
mod callbacks;
mod dates;
mod json;
mod memory;
mod numbers;
mod regex;
mod strings;
mod uri;

pub use array_methods::*;
pub use arrays::*;
pub use associations::*;
pub use async_frames::*;
pub use boundary::*;
pub use callbacks::*;
pub use dates::*;
pub use json::*;
pub use memory::*;
pub use numbers::*;
pub use regex::*;
pub use strings::*;
pub use uri::*;

#[cfg(test)]
mod tests;

/// Reseeds the Context's `Math.random` stream by re-expanding `seed`
/// (stdlib.md §2, host replay control).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_seed_random(ctx: *mut Context, seed: u64) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.seed_random(seed);
}

/// Pins the Context's `Date.now` clock to `ms` (stdlib.md §3; tests and
/// replays). The default, unpinned source is the system UTC clock.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_set_now(ctx: *mut Context, ms: i64) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.set_now(ms);
}

/// Sets the deterministic Context regex execution budget.
///
/// The budget applies to every regular-expression search in this
/// Context.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_set_regex_budget(ctx: *mut Context, budget: u64) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.set_regex_budget(budget);
}

/// Enables or disables freed-handle diagnostics for this Context.
///
/// When enabled, freed allocations whose requested payload is at least
/// `min_payload_bytes` are retained within `max_retained_bytes` of layout
/// storage. When a new retained allocation would exceed that budget, the
/// oldest retained allocations are evicted and released first. Memory held
/// by the mode is bounded by `max_retained_bytes`.
///
/// Dangling-handle and double-free diagnostics are guaranteed for the most
/// recently retained frees whose layouts fit the budget, within the class
/// covered by the threshold, and best-effort otherwise. Freeing a pointer
/// the Context never owned still traps regardless of the threshold or
/// budget. A zero threshold covers every payload; a zero budget retains
/// nothing. `UINT64_MAX` is effectively unbounded. `min_payload_bytes` and
/// `max_retained_bytes` are ignored when `enabled` is 0.
/// The setting is disabled by default.
///
/// This must be called before the first allocation. Returns 1 when the
/// setting was applied, or 0 when allocation had already started and the
/// setting was left unchanged.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_set_freed_handle_diagnostics(
    ctx: *mut Context,
    enabled: u32,
    min_payload_bytes: u64,
    max_retained_bytes: u64,
) -> i32 {
    let min_payload_bytes = usize::try_from(min_payload_bytes).unwrap_or(usize::MAX);
    let max_retained_bytes = usize::try_from(max_retained_bytes).unwrap_or(usize::MAX);
    // SAFETY: exclusive Context contract.
    i32::from(unsafe { &mut *ctx }.set_freed_handle_diagnostics(
        enabled != 0,
        min_payload_bytes,
        max_retained_bytes,
    ))
}

/// Refuses the `n`-th subsequent object-level Context allocation.
///
/// The count is independent of the allocator tier: arena chunk
/// allocations are implementation details and are not counted. `n == 0`
/// disables a pending injected failure.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_fail_alloc_after(ctx: *mut Context, n: u64) {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.fail_alloc_after(n);
}

/// Ends one callback registration (§111 rule 5).
///
/// The call states a guarantee: the host starts no more calls through
/// this registration. Calls that already run can return. The call does
/// not cancel native work and does not unregister a native callback.
/// The host adapter does those first.
///
/// The call returns 1 when `registration` is an open registration of
/// `ctx`. For every other pointer it changes nothing and returns 0:
/// null, a binding of `subscript_rt_cb_bind`, a registration this call
/// already closed, and a registration that ended. The runtime tests
/// membership in its live set first, so it never reads a pointer that
/// the set does not hold.
///
/// After a registration ends, a later registration can take its address.
/// A second release of the old pointer then closes the new registration.
/// That case is in the best-effort class of §111 rule 14: the host ends
/// each registration one time, and the runtime keeps no record of the
/// registrations that ended.
///
/// Release removes a collection root and does nothing else (§111 rule
/// 7). It does not collect, it does not free the userdata, and it does
/// not walk the userdata graph. The userdata then follows the
/// reachability rules of the Context: a script reference keeps it, and
/// the next explicit collection reclaims it when nothing reaches it.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract. `registration` is any
/// pointer value; the call reads it only as an address.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_callback_release(
    ctx: *mut Context,
    registration: *mut std::ffi::c_void,
) -> i32 {
    // SAFETY: shared exclusive Context contract.
    i32::from(unsafe { &mut *ctx }.release_callback_registration(registration))
}

/// Answers which Context one callback registration belongs to
/// (§111 rule 5a).
///
/// A host function that a script calls receives the mirrored arguments
/// and no Context. A host that runs more than one Context therefore
/// cannot name the Context that `subscript_rt_ctx_callback_release`
/// needs. The registration carries that fact, and this call reads it.
///
/// A null `registration` answers null. Every other pointer is read
/// through, so the host calls this while the registration is certainly
/// live: at the crossing that delivers the registration, or inside a
/// fire. A pointer that is not a live registration is a violation of the
/// host's guarantee, in the class of §111 rule 14.
///
/// The call reads one field. It allocates nothing, it changes nothing,
/// and it starts no call.
///
/// # Safety
///
/// `registration` is null, or a registration that
/// `subscript_rt_cb_register` produced and no release ended.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_cb_registration_context(
    registration: *mut std::ffi::c_void,
) -> *mut Context {
    if registration.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: the caller contract supplies a live registration.
    unsafe { crate::registration::registration_context(registration.cast()) }
}

/// Spawns a runtime-owned OS thread with a fresh dedicated Context.
///
/// The worker thread calls `init`, then calls `entry` with its Context and
/// worker-side endpoints unless initialization trapped. `input_descriptor`
/// describes messages accepted by [`subscript_rt_worker_post`], and
/// `output_descriptor` describes messages accepted by
/// [`subscript_rt_worker_outbox_post`]. Both queues are unbounded byte-copy
/// queues. The runtime copies both descriptors during this call. The returned
/// handle is owned by `parent` and remains valid until that Context is
/// released. Null is returned after a parent trap when a callback or
/// descriptor is invalid, or thread creation fails.
///
/// # Safety
///
/// `parent` follows the exclusive Context contract. `init` and `entry` must
/// be linked C-callable functions that obey the runtime trap discipline and
/// remain callable until the worker is joined. Both descriptors and their
/// offset arrays must be readable for this call.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_worker_spawn(
    parent: *mut Context,
    init: Option<WorkerInit>,
    entry: Option<WorkerEntry>,
    input_descriptor: *const crate::worker::WorkerMessageDescriptor,
    output_descriptor: *const crate::worker::WorkerMessageDescriptor,
) -> *mut Worker {
    // SAFETY: shared exclusive Context contract.
    let parent = unsafe { &mut *parent };
    let Some(init) = init else {
        parent.trap(
            TrapKind::Internal,
            "worker spawn requires an initializer",
            0,
        );
        return std::ptr::null_mut();
    };
    let Some(entry) = entry else {
        parent.trap(TrapKind::Internal, "worker spawn requires an entry", 0);
        return std::ptr::null_mut();
    };
    // SAFETY: generated code supplies readable program-image descriptors.
    let input_descriptor =
        match unsafe { crate::worker::QueueDescriptor::copy_from(input_descriptor) } {
            Ok(descriptor) => descriptor,
            Err(message) => {
                parent.trap(TrapKind::Internal, message, 0);
                return std::ptr::null_mut();
            }
        };
    // SAFETY: generated code supplies readable program-image descriptors.
    let output_descriptor =
        match unsafe { crate::worker::QueueDescriptor::copy_from(output_descriptor) } {
            Ok(descriptor) => descriptor,
            Err(message) => {
                parent.trap(TrapKind::Internal, message, 0);
                return std::ptr::null_mut();
            }
        };
    parent.worker_spawn(init, entry, input_descriptor, output_descriptor)
}

/// Copies one fixed-size payload into a worker's parent-to-worker queue.
///
/// Posting never blocks. Returns 1 when accepted and 0 when the worker input
/// is closed or the Context is trapped.
///
/// # Safety
///
/// `parent` follows the exclusive Context contract; `worker` belongs to it.
/// `payload` points to the input payload size supplied at spawn (and may be
/// null only when that size is zero).
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_worker_post(
    parent: *mut Context,
    worker: *mut Worker,
    payload: *const u8,
) -> i32 {
    // SAFETY: forwarded parent, worker, and payload contracts.
    i32::from(unsafe { &mut *parent }.worker_post(worker, payload))
}

/// Non-blockingly receives one worker-to-parent message.
///
/// A message is copied into a fresh allocation owned by `parent` and that
/// allocation is returned. Null means that no message is currently queued,
/// the output is closed and drained, or the Context trapped while allocating.
///
/// # Safety
///
/// `parent` follows the exclusive Context contract and `worker` belongs to it.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_worker_poll(
    parent: *mut Context,
    worker: *mut Worker,
) -> *mut u8 {
    // SAFETY: forwarded parent and worker contracts.
    unsafe { &mut *parent }.worker_poll(worker)
}

/// Closes a worker's parent-to-worker queue.
///
/// Already queued messages remain receivable. After they are drained, worker
/// inbox receives observe end-of-input as a null result. The operation is
/// idempotent.
///
/// # Safety
///
/// `parent` follows the exclusive Context contract and `worker` belongs to it.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_worker_close(parent: *mut Context, worker: *mut Worker) {
    // SAFETY: forwarded parent and worker contracts.
    unsafe { &mut *parent }.worker_close(worker);
}

/// Joins a worker thread, blocking until its entry and Context teardown end.
///
/// Returns 1 for a clean worker. If the worker Context trapped, this returns 0
/// and records trap kind 22 (`worker-trapped`) on the joining Context. Joining
/// an already joined worker repeats its recorded outcome.
///
/// # Safety
///
/// `parent` follows the exclusive Context contract and `worker` belongs to it.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_worker_join(
    parent: *mut Context,
    worker: *mut Worker,
) -> i32 {
    // SAFETY: forwarded parent and worker contracts.
    i32::from(unsafe { &mut *parent }.worker_join(worker))
}

/// Blocks until one parent-to-worker message or end-of-input is available.
///
/// A message is copied into a fresh allocation owned by the worker `ctx`.
/// Null reports closed-and-drained input or a trap. The blocking path sleeps
/// on an OS condition variable and never spins.
///
/// # Safety
///
/// `ctx` is the current worker's exclusive Context and `inbox` is the live
/// endpoint passed to that worker's entry.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_worker_inbox_wait(
    ctx: *mut Context,
    inbox: *mut WorkerInbox,
) -> *mut u8 {
    // SAFETY: forwarded worker Context and endpoint contracts.
    unsafe { crate::worker::inbox_wait(&mut *ctx, inbox) }
}

/// Non-blockingly receives one parent-to-worker message.
///
/// A message is copied into a fresh allocation owned by the worker `ctx`.
/// Null means no queued message, closed-and-drained input, or a trap.
///
/// # Safety
///
/// `ctx` is the current worker's exclusive Context and `inbox` is the live
/// endpoint passed to that worker's entry.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_worker_inbox_poll(
    ctx: *mut Context,
    inbox: *mut WorkerInbox,
) -> *mut u8 {
    // SAFETY: forwarded worker Context and endpoint contracts.
    unsafe { crate::worker::inbox_poll(&mut *ctx, inbox) }
}

/// Copies one fixed-size payload into the worker-to-parent queue.
///
/// Posting never blocks. Returns 1 when accepted and 0 when the endpoint or
/// Context is no longer usable.
///
/// # Safety
///
/// `ctx` is the current worker's exclusive Context, `outbox` is its live
/// endpoint, and `payload` points to the output payload size supplied at
/// spawn (or is null only when that size is zero).
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_worker_outbox_post(
    ctx: *mut Context,
    outbox: *mut WorkerOutbox,
    payload: *const u8,
) -> i32 {
    // SAFETY: forwarded worker Context, endpoint, and payload contracts.
    let ctx = unsafe { &mut *ctx };
    match unsafe { crate::worker::outbox_post(ctx, outbox, payload) } {
        crate::worker::PostResult::Posted => 1,
        crate::worker::PostResult::Closed => 0,
        crate::worker::PostResult::NullPayload => {
            ctx.trap(
                TrapKind::Internal,
                "worker outbox post received a null non-empty payload",
                0,
            );
            0
        }
        crate::worker::PostResult::AllocationFailed => {
            ctx.trap(
                TrapKind::AllocationFailure,
                "worker message record allocation failed",
                0,
            );
            0
        }
    }
}

/// Creates a Context and transfers ownership to the caller, who must
/// return it with [`subscript_rt_ctx_release`]. Never null.
///
/// The returned Context is a ship-tier (releasing) Context (§8.1a/§8.1b):
/// its `Context.free`/`Context.collect` release storage immediately — arena
/// blocks to their free lists, large allocations to the system — rather
/// than retaining and poisoning (built via [`Context::new_releasing`]).
/// Freed-handle diagnostics are disabled by default.
#[no_mangle]
pub extern "C" fn subscript_rt_ctx_new() -> *mut Context {
    Box::into_raw(Context::new_releasing())
}

/// Destroys a Context created by [`subscript_rt_ctx_new`], freeing every
/// allocation it owns.
///
/// # Safety
///
/// `ctx` must be a pointer returned by [`subscript_rt_ctx_new`] that has not
/// been released yet; no handle into it may be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_release(ctx: *mut Context) {
    if ctx.is_null() {
        return;
    }
    // SAFETY: caller guarantees `ctx` came from `subscript_rt_ctx_new` and is
    // released exactly once.
    drop(unsafe { Box::from_raw(ctx) });
}

/// Borrows the captured stdout bytes: returns the base pointer and
/// writes the byte length through `len`. The bytes stay valid until
/// the next script call or the Context's release.
///
/// # Safety
///
/// Shared contract; `len` is a writable `u64`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_stdout(ctx: *const Context, len: *mut u64) -> *const u8 {
    // SAFETY: shared contract.
    let bytes = unsafe { &*ctx }.stdout_bytes();
    if !len.is_null() {
        // SAFETY: caller guarantees `len` is writable.
        unsafe { len.write(bytes.len() as u64) };
    }
    bytes.as_ptr()
}

/// The pending trap's kind as its stable `u32`, or 0 when the run did
/// not trap.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_trap_kind(ctx: *const Context) -> u32 {
    // SAFETY: shared contract.
    unsafe { &*ctx }.trap_record().map_or(0, |r| r.kind as u32)
}

/// The pending trap's position-table index, or 0 when the run did not
/// trap.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_trap_pos_id(ctx: *const Context) -> u32 {
    // SAFETY: shared contract.
    unsafe { &*ctx }.trap_record().map_or(0, |r| r.pos_id)
}

/// Borrows the pending trap's message bytes (UTF-8, no terminator);
/// writes the length through `len`. Null with length 0 when the run
/// did not trap.
///
/// # Safety
///
/// Shared contract; `len` is a writable `u64`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_trap_message(
    ctx: *const Context,
    len: *mut u64,
) -> *const u8 {
    // SAFETY: shared contract.
    let msg = unsafe { &*ctx }.trap_record().map(|r| r.message.as_bytes());
    let bytes = msg.unwrap_or(&[]);
    if !len.is_null() {
        // SAFETY: caller guarantees `len` is writable.
        unsafe { len.write(bytes.len() as u64) };
    }
    if bytes.is_empty() {
        std::ptr::null()
    } else {
        bytes.as_ptr()
    }
}

/// Installs the callback invoked when `ctx` records its first trap.
/// Passing a null `observer` clears it.
///
/// The callback receives no Context handle. It runs from inside
/// [`Context::trap`] while the Context is exclusively borrowed, so it
/// must not call any `subscript_rt_*` function taking that Context (including
/// by recovering the pointer from `userdata`); doing so is an aliasing
/// violation and undefined behaviour. The message points into the
/// stored trap record and remains valid until the trap is cleared or
/// the Context is released.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract. `observer`, when
/// present, must be callable with `userdata` and obey the no-re-entry
/// rule above.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_set_trap_observer(
    ctx: *mut Context,
    observer: Option<TrapObserver>,
    userdata: *mut std::ffi::c_void,
) {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.set_trap_observer(observer, userdata);
}

/// Installs the callback invoked for each line printed by `ctx`. Passing a
/// null `observer` clears it.
///
/// While set, the callback receives each line without its trailing newline
/// and the Context stdout sink retains none of that line's bytes. The line
/// is valid only for the duration of the callback.
///
/// The callback receives no Context handle. It runs from inside
/// [`Context::print_line`] while the Context is exclusively borrowed, so it
/// must not call any `subscript_rt_*` function taking that Context (including by
/// recovering the pointer from `userdata`); doing so is an aliasing
/// violation and undefined behaviour.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract. `observer`, when present,
/// must be callable with `userdata` and obey the no-re-entry rule above.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_set_print_observer(
    ctx: *mut Context,
    observer: Option<PrintObserver>,
    userdata: *mut std::ffi::c_void,
) {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.set_print_observer(observer, userdata);
}

/// Installs the observation-only callback invoked for optional runtime
/// diagnostics advisories. Passing a null `observer` clears it.
///
/// The first advisory kind is
/// `SUBSCRIPT_RT_DIAGNOSTICS_ADVISORY_CALLBACK_USERDATA_FREE`: immediately
/// before an explicit free releases an address held in either userdata slot
/// of a live callback binding. Freeing such userdata is legal;
/// the advisory does not trap, cancel, or otherwise change the release.
///
/// `SUBSCRIPT_RT_DIAGNOSTICS_ADVISORY_BINDING_COUNT` reports each newly
/// interned callback binding at or above the host-configured count threshold;
/// its position id is zero and its message carries the count and threshold.
///
/// The callback receives no Context handle. It runs while the Context is
/// exclusively borrowed, so it must not call any `subscript_rt_*` function
/// taking that Context (including by recovering the pointer from `userdata`);
/// doing so is an aliasing violation and undefined behaviour. It must not
/// call back into script. The message is valid only for the duration of the
/// callback.
///
/// With no observer installed (the default), explicit frees skip the
/// registered-binding check entirely.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract. `observer`, when present,
/// must be callable with `userdata` and obey the no-re-entry rule above.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_set_diagnostics_observer(
    ctx: *mut Context,
    observer: Option<DiagnosticsObserver>,
    userdata: *mut std::ffi::c_void,
) {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.set_diagnostics_observer(observer, userdata);
}

/// Sets the callback-binding count advisory threshold.
///
/// Whenever a new callback binding is interned and the resulting count is at
/// least `threshold`, the installed diagnostics observer receives
/// `SUBSCRIPT_RT_DIAGNOSTICS_ADVISORY_BINDING_COUNT`, position id zero, and a
/// message carrying the count and threshold. Re-registering an existing
/// binding identity never advises.
///
/// The threshold has literal semantics: zero advises on the first record.
/// The default is `UINT64_MAX`. With the default threshold or no diagnostics
/// observer, the check retains no event or message state.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_set_binding_count_advisory(
    ctx: *mut Context,
    threshold: u64,
) {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.set_binding_count_advisory(threshold);
}

/// Marks entry into an exported script function.
///
/// A host that uses [`subscript_rt_ctx_clear_trap`] must call this immediately
/// before each `subscript_init` or `subscript_export_<name>` call and pair it with
/// [`subscript_rt_ctx_exit_script`] after the call returns.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_enter_script(ctx: *mut Context) {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.enter_script();
}

/// Marks return from an exported script function.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract and this call pairs with
/// a preceding [`subscript_rt_ctx_enter_script`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_exit_script(ctx: *mut Context) {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.exit_script();
}

/// Clears the pending trap reporting state when no script call is live.
///
/// Returns 1 after clearing. Returns 0, without changing the Context,
/// while a trap observer is active or `script_depth != 0`.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_clear_trap(ctx: *mut Context) -> i32 {
    // SAFETY: exclusive Context contract.
    let ctx = unsafe { &mut *ctx };
    if !ctx.can_clear_trap() {
        return 0;
    }
    ctx.clear_trap();
    1
}

/// Returns the work a host checkpoint can advance: runnable continuations
/// and aggregate reactions, plus frames that wait for the next checkpoint.
///
/// # Safety
///
/// `ctx` follows the shared Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_async_pending(ctx: *const Context) -> u64 {
    // SAFETY: shared Context contract.
    unsafe { &*ctx }.async_pending() as u64
}

/// Returns the number of started invocations without a completion.
///
/// # Safety
///
/// `ctx` follows the shared Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_async_unfinished(ctx: *const Context) -> u64 {
    // SAFETY: shared Context contract.
    unsafe { &*ctx }.async_unfinished() as u64
}

/// Makes every parked waiter runnable, then drains the ready queue to
/// empty, and returns the work still pending. On a trapped Context this
/// is a no-op returning the current count; an empty Context returns zero.
/// `compiler.md` §168 supplies the bounded form.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract. Generated code for every
/// registered frame remains linked and callable.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_async_step(ctx: *mut Context) -> u64 {
    // SAFETY: exclusive Context contract and queued callbacks were installed
    // by generated code from the same live program.
    unsafe { (&mut *ctx).async_step() as u64 }
}

/// Starts at most `max_dispatches` jobs and returns checkpoint counts (`compiler.md` §168).
/// Call this API each frame. Work remains while `pending` is not zero.
/// A zero budget runs no script and promotes no parked frame.
/// One dispatch runs to its next suspension, completion, or a trap.
/// The budget does not bound that segment's time. This API is no time limit.
/// A trapped Context starts no job and returns the current counts.
///
/// # Safety
/// `ctx` follows the exclusive Context contract. All queued generated code remains callable.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_async_step_budget(
    ctx: *mut Context,
    max_dispatches: u64,
) -> AsyncStepReport {
    // SAFETY: the caller supplies a live exclusive Context and live callbacks.
    unsafe { (&mut *ctx).async_step_budget(max_dispatches) }
}

/// Number of live Context-owned allocations.
///
/// # Safety
///
/// `ctx` follows the shared Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_live_allocations(ctx: *const Context) -> u64 {
    // SAFETY: shared Context contract.
    unsafe { &*ctx }.live_count() as u64
}

/// Payload capacity in live Context-owned allocations.
///
/// Development reports exact requested sizes; ship reports size-class
/// capacity for arena blocks and exact sizes for large allocations.
///
/// # Safety
///
/// `ctx` follows the shared Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_live_bytes(ctx: *const Context) -> u64 {
    // SAFETY: shared Context contract.
    unsafe { &*ctx }.live_bytes() as u64
}

/// Bytes currently reserved from the system for Context allocations.
///
/// # Safety
///
/// `ctx` follows the shared Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_reserved_bytes(ctx: *const Context) -> u64 {
    // SAFETY: shared Context contract.
    unsafe { &*ctx }.reserved_bytes() as u64
}

/// The host's explicit collection (invariant 2).
///
/// This is the same collector that `Context.collect()` reaches from
/// script, so the two callers share one mark-sweep. The script
/// intrinsic keeps its own symbol, `subscript_rt_collect`, which the
/// generated host header does not declare.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract. The host calls this
/// between script calls, at script depth 0. A call from inside a
/// script call, such as a host callback, aliases the Context that
/// generated code holds.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_collect(ctx: *mut Context) {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.collect();
}

/// Visits each live Context-owned allocation and returns the number visited.
///
/// The callback receives the allocation's class id, allocating position
/// id, and tier-specific payload byte figure. The iteration order is
/// unspecified. A null visitor returns zero without visiting.
///
/// # Safety
///
/// `ctx` follows the shared Context contract. `visitor`, when present,
/// must be callable with `userdata` for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_visit_live_allocations(
    ctx: *const Context,
    visitor: Option<AllocationVisitor>,
    userdata: *mut std::ffi::c_void,
) -> u64 {
    // SAFETY: shared Context contract plus the callback/userdata contract
    // documented above.
    unsafe { (&*ctx).visit_live_allocations(visitor, userdata) }
}
