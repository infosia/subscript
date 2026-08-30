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
    AllocationVisitor, AsyncResume, CallbackBinding, Context, DiagnosticsObserver, PrintObserver,
    TrapObserver,
};
use crate::trap::TrapKind;
use crate::worker::{Worker, WorkerEntry, WorkerInbox, WorkerInit, WorkerOutbox};

/// Narrows an `f64` to raw IEEE 754 binary16 storage bits using
/// round-to-nearest-even. Overflow becomes infinity; subnormals, signed
/// zero, and NaN are preserved (Q23).
#[no_mangle]
pub extern "C" fn subscript_rt_f16_from_f64(value: f64) -> u16 {
    crate::half::from_f64(value)
}

/// Widens raw IEEE 754 binary16 storage bits to an exactly represented
/// `f64`, preserving signed zero, infinity, and NaN (Q23).
#[no_mangle]
pub extern "C" fn subscript_rt_f16_to_f64(bits: u16) -> f64 {
    crate::half::to_f64(bits)
}

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

/// `print(message)`: delivers the string's bytes to the installed print
/// observer, or appends them and a newline to the Context stdout sink when
/// no observer is installed.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_print(ctx: *mut Context, s: *const u8) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    if s.is_null() {
        return;
    }
    // SAFETY: `s` is a live string handle of this context.
    let bytes = unsafe { ctx.str_bytes(s) };
    let owned = bytes.to_vec();
    ctx.print_line(&owned);
}

/// `Context.collect()`: explicitly invoked collection (Q7).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_collect(ctx: *mut Context) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.collect();
}

/// Allocates `size` payload bytes tagged `class_id`; null on trap.
///
/// Fresh storage and classes that can hold handles are zeroed. A
/// handle-free class must replace all exposed bytes before a read.
/// String operations use [`Context::alloc_str_with`] for that write.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_alloc(
    ctx: *mut Context,
    size: u64,
    class_id: u32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc(size as usize, class_id, pos_id)
}

/// Allocates, zeroes, and installs the ship image's module-global block.
///
/// This is an internal generated-code ABI, not a language allocation. The
/// block is reached through the Context globals slot and freed with the
/// Context.
///
/// # Safety
///
/// Shared contract; `align` is the emitted C block type's alignment.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_globals_init(
    ctx: *mut Context,
    size: u64,
    align: u64,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    globals_init_with_conversion(ctx, size, align, |value| usize::try_from(value).ok())
}

fn globals_init_with_conversion(
    ctx: &mut Context,
    size: u64,
    align: u64,
    mut convert: impl FnMut(u64) -> Option<usize>,
) -> *mut u8 {
    let Some(size) = convert(size) else {
        ctx.trap(
            TrapKind::Internal,
            "module-global block layout is not representable",
            0,
        );
        return std::ptr::null_mut();
    };
    let Some(align) = convert(align) else {
        ctx.trap(
            TrapKind::Internal,
            "module-global block layout is not representable",
            0,
        );
        return std::ptr::null_mut();
    };
    ctx.init_module_globals(size, align)
}

/// Begins a nested call-duration scratch scope for recursive boundary
/// element and struct-pointer lowering (§32/§33).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_scratch_mark(ctx: *mut Context) -> u64 {
    unsafe { &*ctx }.boundary_scratch_mark() as u64
}

/// Allocates one zeroed scratch block in the current boundary scope.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_scratch_alloc(
    ctx: *mut Context,
    size: u64,
    pos_id: u32,
) -> *mut u8 {
    unsafe { &mut *ctx }.boundary_scratch_alloc(size as usize, pos_id)
}

/// Releases every boundary scratch block allocated since `mark`.
///
/// # Safety
///
/// Shared contract; `mark` came from this Context.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_scratch_release(ctx: *mut Context, mark: u64) {
    unsafe { &mut *ctx }.boundary_scratch_release(mark as usize);
}

/// `Context.free(value)`: frees immediately by default. With freed-handle
/// diagnostics enabled, a threshold-eligible allocation may be retained
/// within the byte budget; double-delete diagnostics then follow §8.1a-3's
/// guaranteed-versus-best-effort coverage.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_delete(ctx: *mut Context, payload: *mut u8, pos_id: u32) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.delete(payload as usize, pos_id);
}

/// Records a trap raised by an emitted check in generated code.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_trap(ctx: *mut Context, kind: u32, pos_id: u32) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // An unknown kind means the code generator and runtime disagree;
    // report it as an internal fault instead of misattributing it.
    let kind = TrapKind::from_u32(kind).unwrap_or(TrapKind::Internal);
    ctx.trap(kind, kind.message(None), pos_id);
}

/// Records an emitted array-bounds trap with its materialized index and
/// length, preserving the runtime's canonical diagnostic across tiers.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_trap_index_out_of_bounds(
    ctx: *mut Context,
    index: i32,
    length: u32,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.trap(
        TrapKind::IndexOutOfBounds,
        TrapKind::IndexOutOfBounds.message(Some((index, u64::from(length)))),
        pos_id,
    );
}

/// Records an R23 boundary trap for an integer outside a `CEnum` mapping.
///
/// # Safety
///
/// Shared contract; `alias` addresses `alias_len` readable UTF-8 bytes for
/// the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_trap_wire_enum(
    ctx: *mut Context,
    alias: *const u8,
    alias_len: u64,
    wire_value: i32,
    pos_id: u32,
) {
    // SAFETY: shared contract supplies a readable compiler-owned byte span.
    let bytes = unsafe { std::slice::from_raw_parts(alias, alias_len as usize) };
    let alias = std::str::from_utf8(bytes).unwrap_or("<invalid alias name>");
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.trap(
        TrapKind::WireEnumUnknownValue,
        format!("unknown wire value {wire_value} for CEnum alias `{alias}`"),
        pos_id,
    );
}

/// Registers a permanent root range: `words` consecutive 8-byte slots
/// at `base` (module globals of managed type, or global aggregates
/// with managed interior).
///
/// # Safety
///
/// Shared contract; the range outlives the script run.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_root_add(ctx: *mut Context, base: *mut u8, words: u64) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.root_add(base as usize, words as usize);
}

/// Pushes a shadow frame of `slots` managed-local slots at `base`.
///
/// # Safety
///
/// Shared contract; the range stays valid until the matching pop.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_shadow_push(ctx: *mut Context, base: *mut u8, slots: u64) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.shadow_push(base as usize, slots as usize);
}

/// Pops the most recent shadow frame.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_shadow_pop(ctx: *mut Context) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.shadow_pop();
}

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
pub unsafe extern "C" fn subscript_rt_async_register(ctx: *mut Context, frame: *mut u8) {
    unsafe { &mut *ctx }.async_register(frame);
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
///
/// # Safety
///
/// Shared contract; `frame` is registered in `ctx`, and `out` points to
/// `size` writable bytes when `size` is nonzero.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_async_result(
    ctx: *const Context,
    frame: *const u8,
    out: *mut u8,
    size: u64,
) -> u8 {
    u8::from(unsafe { &*ctx }.async_result(frame, out, size as usize))
}

// ----- Map / Set (stdlib.md §10, Q24) -----

fn assoc_receiver_is_live(ctx: &mut Context, handle: *const u8, pos_id: u32) -> bool {
    ctx.require_live_handle(handle as usize, pos_id)
}

/// Begins the fixed-bound insertion-order traversal shared by Map/Set
/// `forEach`, fused `for…of`, and array-literal spread.
///
/// # Safety
///
/// `handle` is a live Map or Set payload.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_assoc_iter_begin(
    ctx: *mut Context,
    handle: *mut u8,
    pos_id: u32,
) -> u64 {
    // SAFETY: shared contract.
    if !assoc_receiver_is_live(unsafe { &mut *ctx }, handle, pos_id) {
        return 0;
    }
    // SAFETY: receiver was validated above.
    unsafe { crate::assocops::iteration_begin(handle) as u64 }
}

/// Copies one still-active ordered key/value during a fused traversal.
/// `value != 0` selects a Map value; zero selects a Map/Set key.
///
/// # Safety
///
/// `handle` is the receiver passed to [`subscript_rt_assoc_iter_begin`],
/// `index` is below its returned bound, and `out` is writable for the
/// selected monomorphized field.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_assoc_iter_copy(
    ctx: *mut Context,
    handle: *mut u8,
    index: u64,
    value: u32,
    out: *mut u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    if !assoc_receiver_is_live(unsafe { &mut *ctx }, handle, pos_id) {
        return 0;
    }
    // SAFETY: receiver and output follow the shared traversal ABI.
    i32::from(unsafe { crate::assocops::iteration_copy(handle, index as usize, value != 0, out) })
}

/// Ends a traversal begun by [`subscript_rt_assoc_iter_begin`].
///
/// # Safety
///
/// `ctx` is live; `handle` may have been deleted by script.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_assoc_iter_end(ctx: *mut Context, handle: *mut u8) {
    // SAFETY: forwarded contract.
    unsafe { crate::assocops::iteration_end(ctx, handle) };
}

/// Allocates an empty monomorphized `Map<K, V>`.
///
/// `key_size` / `value_size` are the calling tier's concrete storage
/// widths and `key_kind` is the compiler/runtime ABI tag. Backing entry
/// and index storage stays unallocated until `set`.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_new(
    ctx: *mut Context,
    key_size: u64,
    value_size: u64,
    key_kind: u32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(kind) = crate::assocops::KeyKind::from_u32(key_kind) else {
        ctx.trap(
            TrapKind::Internal,
            format!("unknown Map key-kind code {key_kind}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    crate::assocops::new(
        ctx,
        key_size as usize,
        value_size as usize,
        kind,
        false,
        pos_id,
    )
}

/// Allocates an empty monomorphized `Set<K>`.
///
/// # Safety
///
/// As [`subscript_rt_map_new`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_new(
    ctx: *mut Context,
    key_size: u64,
    key_kind: u32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(kind) = crate::assocops::KeyKind::from_u32(key_kind) else {
        ctx.trap(
            TrapKind::Internal,
            format!("unknown Set key-kind code {key_kind}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    crate::assocops::new(ctx, key_size as usize, 0, kind, true, pos_id)
}

/// `Map.size`.
///
/// # Safety
///
/// Shared contract; `map` is a live map handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_size(ctx: *mut Context, map: *const u8) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(ctx, map, 0) {
        return 0;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::len(map) }
}

/// `Set.size`.
///
/// # Safety
///
/// Shared contract; `set` is a live set handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_size(ctx: *mut Context, set: *const u8) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(ctx, set, 0) {
        return 0;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::len(set) }
}

/// `Map.set`: inserts or overwrites and returns the receiver.
///
/// # Safety
///
/// Shared contract; `map` is live and `key` / `value` point at values
/// of the monomorphized widths.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_set(
    ctx: *mut Context,
    map: *mut u8,
    key: *const u8,
    value: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, map, pos_id) {
        return map;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::insert(ctx, map, key, value, pos_id) }
}

/// `Set.add`: inserts and returns the receiver.
///
/// # Safety
///
/// Shared contract; `set` is live and `key` points at its
/// monomorphized key value.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_add(
    ctx: *mut Context,
    set: *mut u8,
    key: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, set, pos_id) {
        return set;
    }
    // SAFETY: caller contract; a set has zero-width values.
    unsafe { crate::assocops::insert(ctx, set, key, std::ptr::null(), pos_id) }
}

/// `Map.get`: copies a present value to `out`, returning 1; returns 0
/// on a miss without writing `out`.
///
/// # Safety
///
/// Shared contract; pointers match the map's monomorphized widths.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_get(
    ctx: *mut Context,
    map: *mut u8,
    key: *const u8,
    out: *mut u8,
) -> i32 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, map, 0) {
        return 0;
    }
    // SAFETY: caller contract.
    i32::from(unsafe { crate::assocops::get(ctx, map, key, out) })
}

/// `Map.getOr`: copies the present value or the supplied fallback.
///
/// # Safety
///
/// As [`subscript_rt_map_get`], and `fallback` is readable for the value width.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_get_or(
    ctx: *mut Context,
    map: *mut u8,
    key: *const u8,
    fallback: *const u8,
    out: *mut u8,
) {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, map, 0) {
        return;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::get_or(ctx, map, key, fallback, out, 0) };
}

/// `Map.has`.
///
/// # Safety
///
/// Shared contract; `map` and `key` match its monomorphized shape.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_has(
    ctx: *mut Context,
    map: *mut u8,
    key: *const u8,
) -> i32 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, map, 0) {
        return 0;
    }
    // SAFETY: caller contract.
    i32::from(unsafe { crate::assocops::has(ctx, map, key) })
}

/// `Set.has`.
///
/// # Safety
///
/// As [`subscript_rt_map_has`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_has(
    ctx: *mut Context,
    set: *mut u8,
    key: *const u8,
) -> i32 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, set, 0) {
        return 0;
    }
    // SAFETY: caller contract.
    i32::from(unsafe { crate::assocops::has(ctx, set, key) })
}

/// `Map.delete`.
///
/// # Safety
///
/// As [`subscript_rt_map_has`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_delete(
    ctx: *mut Context,
    map: *mut u8,
    key: *const u8,
) -> i32 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, map, 0) {
        return 0;
    }
    // SAFETY: caller contract.
    i32::from(unsafe { crate::assocops::delete(ctx, map, key) })
}

/// `Set.delete`.
///
/// # Safety
///
/// As [`subscript_rt_map_has`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_delete(
    ctx: *mut Context,
    set: *mut u8,
    key: *const u8,
) -> i32 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, set, 0) {
        return 0;
    }
    // SAFETY: caller contract.
    i32::from(unsafe { crate::assocops::delete(ctx, set, key) })
}

/// `Map.clear`: eagerly retires its ordered and index storage.
///
/// # Safety
///
/// Shared contract; `map` is a live map handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_clear(ctx: *mut Context, map: *mut u8) {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, map, 0) {
        return;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::clear(&mut *ctx, map) };
}

/// `Set.clear`: eagerly retires its ordered and index storage.
///
/// # Safety
///
/// Shared contract; `set` is a live set handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_clear(ctx: *mut Context, set: *mut u8) {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, set, 0) {
        return;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::clear(&mut *ctx, set) };
}

/// `Map.forEach` in insertion order.
///
/// `bridge` is a generated fixed-ABI adapter that loads the map's
/// concrete `V` / `K` and calls `code(ctx, env, value, key)`. The
/// runtime checks the Context trap flag after every bridge return.
///
/// # Safety
///
/// Shared contract; handles and function pointers have the documented
/// generated signatures.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_for_each(
    ctx: *mut Context,
    map: *mut u8,
    code: *const u8,
    env: *const u8,
    bridge: *const u8,
) {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, map, 0) {
        return;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::map_for_each(ctx, map, code, env, bridge) };
}

/// `Set.forEach` in insertion order, through a generated key bridge.
///
/// # Safety
///
/// As [`subscript_rt_map_for_each`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_for_each(
    ctx: *mut Context,
    set: *mut u8,
    code: *const u8,
    env: *const u8,
    bridge: *const u8,
) {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, set, 0) {
        return;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::set_for_each(ctx, set, code, env, bridge) };
}

/// `Map.groupBy(items, callback)`: returns a fresh insertion-ordered map
/// whose values are fresh arrays of source elements.
///
/// # Safety
///
/// Shared contract; handles, widths, and function pointers have the
/// generated signatures.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_group_by(
    ctx: *mut Context,
    items: *mut u8,
    code: *const u8,
    env: *const u8,
    bridge: *const u8,
    key_size: u64,
    key_kind: u32,
    pos_id: u32,
) -> *mut u8 {
    let runtime = unsafe { &mut *ctx };
    if !runtime.require_live_handle(items as usize, pos_id) {
        return std::ptr::null_mut();
    }
    let Some(kind) = crate::assocops::KeyKind::from_u32(key_kind) else {
        runtime.trap(
            TrapKind::Internal,
            format!("unknown Map.groupBy key-kind code {key_kind}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    unsafe {
        crate::assocops::group_by(
            ctx,
            items,
            code,
            env,
            bridge,
            key_size as usize,
            kind,
            pos_id,
        )
    }
}

unsafe fn set_pair_is_live(ctx: *mut Context, left: *mut u8, right: *mut u8, pos_id: u32) -> bool {
    let runtime = unsafe { &mut *ctx };
    assoc_receiver_is_live(runtime, left, pos_id) && assoc_receiver_is_live(runtime, right, pos_id)
}

/// `Set.union`: returns a fresh result in ES2024 order.
///
/// # Safety
///
/// Shared contract; both operands are live `Set<K>` handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_union(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    if !unsafe { set_pair_is_live(ctx, left, right, pos_id) } {
        return std::ptr::null_mut();
    }
    unsafe { crate::assocops::set_union(ctx, left, right, pos_id) }
}

/// `Set.intersection`: returns a fresh result in ES2024 order.
///
/// # Safety
///
/// As [`subscript_rt_set_union`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_intersection(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    if !unsafe { set_pair_is_live(ctx, left, right, pos_id) } {
        return std::ptr::null_mut();
    }
    unsafe { crate::assocops::set_intersection(ctx, left, right, pos_id) }
}

/// `Set.difference`: returns a fresh receiver-minus-argument result.
///
/// # Safety
///
/// As [`subscript_rt_set_union`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_difference(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    if !unsafe { set_pair_is_live(ctx, left, right, pos_id) } {
        return std::ptr::null_mut();
    }
    unsafe { crate::assocops::set_difference(ctx, left, right, pos_id) }
}

/// `Set.symmetricDifference`: returns a fresh receiver-then-argument
/// result.
///
/// # Safety
///
/// As [`subscript_rt_set_union`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_symmetric_difference(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    if !unsafe { set_pair_is_live(ctx, left, right, pos_id) } {
        return std::ptr::null_mut();
    }
    unsafe { crate::assocops::set_symmetric_difference(ctx, left, right, pos_id) }
}

/// `Set.isSubsetOf`.
///
/// # Safety
///
/// Shared contract; both operands are live `Set<K>` handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_is_subset_of(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
) -> i32 {
    if !unsafe { set_pair_is_live(ctx, left, right, 0) } {
        return 0;
    }
    i32::from(unsafe { crate::assocops::set_is_subset_of(ctx, left, right) })
}

/// `Set.isSupersetOf`.
///
/// # Safety
///
/// As [`subscript_rt_set_is_subset_of`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_is_superset_of(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
) -> i32 {
    if !unsafe { set_pair_is_live(ctx, left, right, 0) } {
        return 0;
    }
    i32::from(unsafe { crate::assocops::set_is_superset_of(ctx, left, right) })
}

/// `Set.isDisjointFrom`.
///
/// # Safety
///
/// As [`subscript_rt_set_is_subset_of`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_is_disjoint_from(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
) -> i32 {
    if !unsafe { set_pair_is_live(ctx, left, right, 0) } {
        return 0;
    }
    i32::from(unsafe { crate::assocops::set_is_disjoint_from(ctx, left, right) })
}

// ----- strings (Q5) -----

/// Interns a string literal embedded in the module's data.
///
/// # Safety
///
/// Shared contract; `ptr` points at `len` bytes of module data that
/// outlive the context.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_lit(
    ctx: *mut Context,
    ptr: *const u8,
    len: u64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract; literal data outlives the context.
    unsafe { (*ctx).intern_literal(ptr, len as usize, pos_id) }
}

/// Materializes a language string by copying a C `(ptr, len)` view.
///
/// This is the field-level form of the callback trampoline's view-to-string
/// copy-in. A null pointer or zero length denotes the empty string, including
/// the all-zero view produced by a zero-filled C boundary struct.
///
/// # Safety
///
/// Shared contract; when `ptr` is non-null and `len` is nonzero it points at
/// `len` readable bytes for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_from_view(
    ctx: *mut Context,
    ptr: *const u8,
    len: u64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: caller guarantees the view contract.
    unsafe { alloc_str_from_view(ctx, ptr, len, pos_id) }
}

/// Shared implementation for callback and boundary-struct view copy-in.
///
/// # Safety
///
/// When `ptr` is non-null and `len` is nonzero it points at `len` readable
/// bytes for this call.
unsafe fn alloc_str_from_view(ctx: &mut Context, ptr: *const u8, len: u64, pos_id: u32) -> *mut u8 {
    let bytes: &[u8] = if ptr.is_null() || len == 0 {
        &[]
    } else {
        let Ok(len) = usize::try_from(len) else {
            ctx.trap(
                TrapKind::Internal,
                "C string view length does not fit the host address space",
                pos_id,
            );
            return std::ptr::null_mut();
        };
        // SAFETY: caller guarantees this readable view.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };
    ctx.alloc_str(bytes, pos_id)
}

/// String byte length (Q5: `length` is the byte length).
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_len(ctx: *mut Context, s: *const u8) -> i32 {
    if s.is_null() {
        return 0;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &*ctx };
    // SAFETY: `s` is a live string handle.
    unsafe { ctx.str_bytes(s).len() as i32 }
}

/// Returns the interned one-code-point string beginning at byte `index`
/// and writes the next byte index to `next`. BMP values are allocation
/// free; each distinct astral scalar allocates once per Context.
///
/// This is the value-producing half of string `for…of`; index movement
/// is by UTF-8 scalar width, never by byte-as-character.
///
/// # Safety
///
/// `s` is a live string handle and `next` is writable.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_iter_code_point(
    ctx: *mut Context,
    s: *const u8,
    index: i32,
    next: *mut i32,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() || next.is_null() || index < 0 {
        return std::ptr::null_mut();
    }
    let at = index as usize;
    let (value, consumed) = {
        // SAFETY: shared contract.
        let bytes = unsafe { (&*ctx).str_bytes(s) };
        if at >= bytes.len() {
            // SAFETY: caller supplies writable output.
            unsafe { next.write(index) };
            return std::ptr::null_mut();
        }
        match std::str::from_utf8(&bytes[at..]) {
            Ok(text) => {
                let value = text.chars().next().unwrap_or('\u{fffd}');
                (value, value.len_utf8())
            }
            Err(error) => {
                let consumed = error.error_len().unwrap_or(1).max(1);
                ('\u{fffd}', consumed)
            }
        }
    };
    // SAFETY: caller supplies writable output.
    unsafe { next.write(index.saturating_add(consumed as i32)) };
    // SAFETY: shared contract; the immutable string borrow ended above.
    unsafe { (&mut *ctx).code_point(value, pos_id) }
}

/// String concatenation (`+` / template literals).
///
/// # Safety
///
/// Shared contract; `a` and `b` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_concat(
    ctx: *mut Context,
    a: *const u8,
    b: *const u8,
    pos_id: u32,
) -> *mut u8 {
    if a.is_null() || b.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handles. Allocating another Context string does
    // not move either immutable input allocation.
    let (a_ptr, a_len) = {
        let bytes = unsafe { ctx.str_bytes(a) };
        (bytes.as_ptr(), bytes.len())
    };
    // SAFETY: as above.
    let (b_ptr, b_len) = {
        let bytes = unsafe { ctx.str_bytes(b) };
        (bytes.as_ptr(), bytes.len())
    };
    let Some(result_len) = a_len.checked_add(b_len) else {
        ctx.trap(
            TrapKind::AllocationFailure,
            "string concatenation length is not representable",
            pos_id,
        );
        return std::ptr::null_mut();
    };
    ctx.alloc_str_with(result_len, pos_id, |destination| {
        // SAFETY: both input ranges stay live during this synchronous
        // writer. The fresh destination does not overlap either input.
        unsafe {
            std::ptr::copy_nonoverlapping(a_ptr, destination.as_mut_ptr(), a_len);
            std::ptr::copy_nonoverlapping(b_ptr, destination.as_mut_ptr().add(a_len), b_len);
        }
        result_len
    })
}

/// `slice(start, end)` with byte offsets and ECMA's negative/clamping
/// rules; a reversed normalized pair produces `""`. Off a UTF-8
/// boundary traps (Q5).
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_slice(
    ctx: *mut Context,
    s: *const u8,
    start: i32,
    end: i32,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handle. Copied out so the borrow does not
    // overlap the mutable trap/alloc calls below.
    let bytes: Vec<u8> = unsafe { ctx.str_bytes(s) }.to_vec();
    let len = bytes.len() as i64;
    let relative = |index: i32| {
        let index = i64::from(index);
        if index < 0 {
            (len + index).max(0)
        } else {
            index.min(len)
        }
    };
    let lo = relative(start);
    let end_boundary = relative(end);
    let hi = end_boundary.max(lo);
    // Strings are UTF-8 by construction (literals, concatenation, and
    // boundary-checked slices of UTF-8 strings).
    let text = std::str::from_utf8(&bytes).unwrap_or_default();
    let (lo, hi) = (lo as usize, hi as usize);
    if !text.is_char_boundary(lo) || !text.is_char_boundary(end_boundary as usize) {
        ctx.trap(
            TrapKind::StringSlice,
            format!(
                "slice({start}, {end}) normalizes to ({lo}, {end_boundary}), \
                 which is not on a UTF-8 boundary"
            ),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    ctx.alloc_str(&bytes[lo..hi], pos_id)
}

/// Content equality (`===` on strings): 1 when equal, else 0.
///
/// # Safety
///
/// Shared contract; `a` and `b` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_eq(ctx: *mut Context, a: *const u8, b: *const u8) -> i32 {
    if a.is_null() || b.is_null() {
        return i32::from(a == b);
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &*ctx };
    // SAFETY: live string handles.
    let equal = unsafe { ctx.str_bytes(a) == ctx.str_bytes(b) };
    i32::from(equal)
}

// ----- String methods (stdlib.md §8, Q21) -----
//
// Byte-measure operations over the immutable UTF-8 string payloads;
// the pure logic lives in [`crate::strops`], these wrappers add the
// Context (bytes in, traps, fresh allocations out). Convention: the
// receiver handle follows `ctx`; entries that can trap or allocate
// carry a trailing `pos_id`, the five pure search predicates take
// none. Every string/array result is a **fresh** Context allocation —
// including the pad no-ops that return the receiver's bytes unchanged.

/// `indexOf(needle, from)`: first byte index or −1 (Q21). `from` is
/// clamped to `[0, length]`; an empty needle returns the clamped
/// `from`. The checker supplies the defaulted `from` (0).
///
/// # Safety
///
/// Shared contract; `s` and `needle` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_index_of(
    ctx: *mut Context,
    s: *const u8,
    needle: *const u8,
    from: i32,
) -> i32 {
    if s.is_null() || needle.is_null() {
        return -1;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &*ctx };
    // SAFETY: live string handles.
    unsafe { crate::strops::index_of(ctx.str_bytes(s), ctx.str_bytes(needle), from) }
}

/// `lastIndexOf(needle)`: last byte index or −1; an empty needle
/// returns the length (Q21).
///
/// # Safety
///
/// Shared contract; `s` and `needle` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_last_index_of(
    ctx: *mut Context,
    s: *const u8,
    needle: *const u8,
) -> i32 {
    if s.is_null() || needle.is_null() {
        return -1;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &*ctx };
    // SAFETY: live string handles.
    unsafe { crate::strops::last_index_of(ctx.str_bytes(s), ctx.str_bytes(needle)) }
}

/// `includes(needle, from)`: 1 when found, else 0. The checker supplies
/// the defaulted `from` (0); an empty needle is included.
///
/// # Safety
///
/// Shared contract; `s` and `needle` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_includes(
    ctx: *mut Context,
    s: *const u8,
    needle: *const u8,
    from: i32,
) -> i32 {
    // SAFETY: shared contract (forwarded).
    i32::from(unsafe { subscript_rt_str_index_of(ctx, s, needle, from) } >= 0)
}

/// `startsWith(needle, position)`: 1 when `needle` begins at the
/// clamped byte position.
///
/// # Safety
///
/// Shared contract; `s` and `needle` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_starts_with(
    ctx: *mut Context,
    s: *const u8,
    needle: *const u8,
    position: i32,
) -> i32 {
    if s.is_null() || needle.is_null() {
        return 0;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &*ctx };
    // SAFETY: live string handles.
    let starts =
        unsafe { crate::strops::starts_with(ctx.str_bytes(s), ctx.str_bytes(needle), position) };
    i32::from(starts)
}

/// `endsWith(needle, endPosition)`: 1 when `needle` ends at the clamped
/// byte position.
///
/// # Safety
///
/// Shared contract; `s` and `needle` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_ends_with(
    ctx: *mut Context,
    s: *const u8,
    needle: *const u8,
    end_position: i32,
) -> i32 {
    if s.is_null() || needle.is_null() {
        return 0;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &*ctx };
    // SAFETY: live string handles.
    let ends =
        unsafe { crate::strops::ends_with(ctx.str_bytes(s), ctx.str_bytes(needle), end_position) };
    i32::from(ends)
}

/// `charCodeAt(i)`: the byte value 0–255 (Q21; JS returns the UTF-16
/// unit). Out of range traps (JS returns NaN) and returns 0.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_char_code_at(
    ctx: *mut Context,
    s: *const u8,
    i: i32,
    pos_id: u32,
) -> i32 {
    if s.is_null() {
        return 0;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handle. Copied out so the borrow does not
    // overlap the mutable trap call below.
    let (len, byte) = {
        // SAFETY: live string handle.
        let bytes = unsafe { ctx.str_bytes(s) };
        let byte = usize::try_from(i).ok().and_then(|i| bytes.get(i).copied());
        (bytes.len(), byte)
    };
    match byte {
        Some(b) => i32::from(b),
        None => {
            ctx.trap(
                TrapKind::StrRange,
                format!("charCodeAt({i}) out of range for string length {len}"),
                pos_id,
            );
            0
        }
    }
}

/// Shared allocation and UTF-8-boundary validation for `substring` and
/// `substr`, after their distinct byte ranges have been normalized.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
unsafe fn str_alloc_range(
    ctx: *mut Context,
    s: *const u8,
    lo: usize,
    hi: usize,
    call: &str,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: `s` is a live string handle. Copy before trapping or
    // allocating through the mutable Context.
    let bytes: Vec<u8> = unsafe { ctx.str_bytes(s) }.to_vec();
    let text = std::str::from_utf8(&bytes).unwrap_or_default();
    if !text.is_char_boundary(lo) || !text.is_char_boundary(hi) {
        ctx.trap(
            TrapKind::StringSlice,
            format!("{call} range ({lo}, {hi}) is not on a UTF-8 boundary"),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    ctx.alloc_str(&bytes[lo..hi], pos_id)
}

/// `substring(start, end)`: clamp negative arguments to zero and
/// arguments beyond the byte length to that length, swap a reversed
/// pair, then require UTF-8 boundaries.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_substring(
    ctx: *mut Context,
    s: *const u8,
    start: i32,
    end: i32,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract and live string handle.
    let len = unsafe { (&*ctx).str_bytes(s).len() };
    let (lo, hi) = crate::strops::substring_range(len, start, end);
    // SAFETY: forwarded shared contract.
    unsafe { str_alloc_range(ctx, s, lo, hi, "substring", pos_id) }
}

/// `substr(start, length)`: a negative byte start counts from the end,
/// and a non-positive length produces an empty range. The normalized
/// boundaries must be UTF-8 code-point boundaries.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_substr(
    ctx: *mut Context,
    s: *const u8,
    start: i32,
    length: i32,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract and live string handle.
    let len = unsafe { (&*ctx).str_bytes(s).len() };
    let (lo, hi) = crate::strops::substr_range(len, start, length);
    // SAFETY: forwarded shared contract.
    unsafe { str_alloc_range(ctx, s, lo, hi, "substr", pos_id) }
}

/// `charAt(i)`: a fresh string containing the code point beginning at
/// byte `i`; out of range returns `""`, while an in-range continuation
/// byte traps.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_char_at(
    ctx: *mut Context,
    s: *const u8,
    i: i32,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handle. Copy before trap/allocation.
    let bytes: Vec<u8> = unsafe { ctx.str_bytes(s) }.to_vec();
    let Some(index) = usize::try_from(i).ok().filter(|&index| index < bytes.len()) else {
        return ctx.alloc_str(b"", pos_id);
    };
    let text = std::str::from_utf8(&bytes).unwrap_or_default();
    if !text.is_char_boundary(index) {
        ctx.trap(
            TrapKind::StrRange,
            format!("charAt({i}) is not on a UTF-8 boundary"),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    let width = text[index..]
        .chars()
        .next()
        .map(char::len_utf8)
        .unwrap_or(0);
    ctx.alloc_str(&bytes[index..index + width], pos_id)
}

/// `codePointAt(i)`: the Unicode scalar value beginning at byte `i`.
/// Out-of-range and continuation-byte indices trap.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_code_point_at(
    ctx: *mut Context,
    s: *const u8,
    i: i32,
    pos_id: u32,
) -> i32 {
    if s.is_null() {
        return 0;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handle. Copy before trapping.
    let bytes: Vec<u8> = unsafe { ctx.str_bytes(s) }.to_vec();
    let Some(index) = usize::try_from(i).ok().filter(|&index| index < bytes.len()) else {
        ctx.trap(
            TrapKind::StrRange,
            format!(
                "codePointAt({i}) out of range for string length {}",
                bytes.len()
            ),
            pos_id,
        );
        return 0;
    };
    let text = std::str::from_utf8(&bytes).unwrap_or_default();
    if !text.is_char_boundary(index) {
        ctx.trap(
            TrapKind::StrRange,
            format!("codePointAt({i}) is not on a UTF-8 boundary"),
            pos_id,
        );
        return 0;
    }
    text[index..].chars().next().map_or(0, |ch| ch as i32)
}

/// Method spelling of string concatenation. It forwards to the same
/// implementation as `+` and template-literal concatenation.
///
/// # Safety
///
/// Shared contract; `a` and `b` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_method_concat(
    ctx: *mut Context,
    a: *const u8,
    b: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: forwarded shared contract.
    unsafe { subscript_rt_str_concat(ctx, a, b, pos_id) }
}

/// `split(sep)`: a fresh `string[]` of the pieces between separator
/// matches (JS piece order; no match → `[whole]`). An empty separator
/// traps (Q21: byte-splitting would fracture UTF-8 code points) and
/// returns null. The elements are string handles stored as 8-byte
/// values, exactly as a `string[]` literal stores them.
///
/// # Safety
///
/// Shared contract; `s` and `sep` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_split(
    ctx: *mut Context,
    s: *const u8,
    sep: *const u8,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() || sep.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handles. Copied out so the borrows do not
    // overlap the mutable trap/alloc calls below.
    let hay: Vec<u8> = unsafe { ctx.str_bytes(s) }.to_vec();
    // SAFETY: live string handles.
    let sep: Vec<u8> = unsafe { ctx.str_bytes(sep) }.to_vec();
    if sep.is_empty() {
        ctx.trap(
            TrapKind::StrRange,
            "split(\"\"): an empty separator is not accepted",
            pos_id,
        );
        return std::ptr::null_mut();
    }
    let arr = ctx.array_new(8, pos_id);
    if arr.is_null() {
        return std::ptr::null_mut();
    }
    for piece in crate::strops::split(&hay, &sep) {
        let handle = ctx.alloc_str(piece, pos_id);
        if handle.is_null() {
            return std::ptr::null_mut();
        }
        let word = handle as u64;
        // SAFETY: `arr` is a live 8-byte-element array of this context;
        // `word` is readable for 8 bytes.
        if unsafe { ctx.array_push(arr, (&word as *const u64).cast(), pos_id) } < 0 {
            return std::ptr::null_mut();
        }
    }
    arr
}

/// Shared body of the `trim` family: selects the strip via `strip`,
/// allocates the fresh result.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
unsafe fn str_trim_with(
    ctx: *mut Context,
    s: *const u8,
    pos_id: u32,
    strip: fn(&[u8]) -> &[u8],
) -> *mut u8 {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handle. Copied out so the borrow does not
    // overlap the mutable alloc call below.
    let bytes: Vec<u8> = unsafe { ctx.str_bytes(s) }.to_vec();
    ctx.alloc_str(strip(&bytes), pos_id)
}

/// `trim()`: strips ECMA WhiteSpace + LineTerminator code points (Q21)
/// from both ends; an all-whitespace string becomes `""`.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_trim(
    ctx: *mut Context,
    s: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract (forwarded).
    unsafe { str_trim_with(ctx, s, pos_id, crate::strops::trim) }
}

/// `trimStart()`: strips leading ECMA WhiteSpace + LineTerminator code
/// points (Q21).
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_trim_start(
    ctx: *mut Context,
    s: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract (forwarded).
    unsafe { str_trim_with(ctx, s, pos_id, crate::strops::trim_start) }
}

/// `trimEnd()`: strips trailing ECMA WhiteSpace + LineTerminator code
/// points (Q21).
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_trim_end(
    ctx: *mut Context,
    s: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract (forwarded).
    unsafe { str_trim_with(ctx, s, pos_id, crate::strops::trim_end) }
}

/// `repeat(n)`: `n` copies; `repeat(0)` is `""`. A negative `n` traps
/// (Q21; JS throws RangeError) and returns null.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_repeat(
    ctx: *mut Context,
    s: *const u8,
    n: i32,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    if n < 0 {
        ctx.trap(
            TrapKind::StrRange,
            format!("repeat({n}): the count must be non-negative"),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    // SAFETY: live string handle. Copied out so the borrow does not
    // overlap the mutable alloc call below.
    let bytes: Vec<u8> = unsafe { ctx.str_bytes(s) }.to_vec();
    ctx.alloc_str(&crate::strops::repeat(&bytes, n), pos_id)
}

/// Shared body of `padStart`/`padEnd` (Q21 byte lengths): pads with
/// cyclic copies of `pad`, the final repeat truncated to the target
/// length. An already-long-enough receiver returns a **fresh copy**
/// with unchanged bytes (§8: documented choice — every §8 string
/// result is a fresh Context allocation). An empty `pad` with
/// `target > length` traps (Q21; JS silently returns the string
/// unchanged, which hides bugs) and returns null.
///
/// # Safety
///
/// Shared contract; `s` and `pad` are live string handles.
unsafe fn str_pad(
    ctx: *mut Context,
    s: *const u8,
    target: i32,
    pad: *const u8,
    at_start: bool,
    name: &str,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() || pad.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handles. Allocating the result does not move
    // either immutable input allocation.
    let (bytes_ptr, bytes_len) = {
        let bytes = unsafe { ctx.str_bytes(s) };
        (bytes.as_ptr(), bytes.len())
    };
    // SAFETY: as above.
    let (pad_ptr, pad_len) = {
        let bytes = unsafe { ctx.str_bytes(pad) };
        (bytes.as_ptr(), bytes.len())
    };
    let target = usize::try_from(target.max(0)).unwrap_or(0);
    if pad_len == 0 && target > bytes_len {
        ctx.trap(
            TrapKind::StrRange,
            format!(
                "{name}({target}): an empty pad cannot reach the target length \
                 (string length {})",
                bytes_len
            ),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    let result_len = target.max(bytes_len);
    ctx.alloc_str_with(result_len, pos_id, |destination| {
        // SAFETY: both input ranges stay live during this synchronous
        // writer. Neither range overlaps the fresh destination.
        let bytes = unsafe { std::slice::from_raw_parts(bytes_ptr, bytes_len) };
        // SAFETY: as above.
        let pad_bytes = unsafe { std::slice::from_raw_parts(pad_ptr, pad_len) };
        crate::strops::pad_into(bytes, pad_bytes, at_start, destination)
    })
}

/// `padStart(len, pad)` — see [`str_pad`]. The checker supplies the
/// defaulted `pad` (`" "`).
///
/// # Safety
///
/// Shared contract; `s` and `pad` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_pad_start(
    ctx: *mut Context,
    s: *const u8,
    target: i32,
    pad: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract (forwarded).
    unsafe { str_pad(ctx, s, target, pad, true, "padStart", pos_id) }
}

/// `padEnd(len, pad)` — see [`str_pad`]. The checker supplies the
/// defaulted `pad` (`" "`).
///
/// # Safety
///
/// Shared contract; `s` and `pad` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_pad_end(
    ctx: *mut Context,
    s: *const u8,
    target: i32,
    pad: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract (forwarded).
    unsafe { str_pad(ctx, s, target, pad, false, "padEnd", pos_id) }
}

/// Shared body of the case mappings: maps via `map`, allocates the
/// fresh result.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
unsafe fn str_case_with(
    ctx: *mut Context,
    s: *const u8,
    pos_id: u32,
    map: fn(&[u8]) -> Vec<u8>,
) -> *mut u8 {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handle. Copied out so the borrow does not
    // overlap the mutable alloc call below.
    let bytes: Vec<u8> = unsafe { ctx.str_bytes(s) }.to_vec();
    ctx.alloc_str(&map(&bytes), pos_id)
}

/// `toUpperCase()`: Unicode Default Case Conversion (Q21).
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_to_upper(
    ctx: *mut Context,
    s: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract (forwarded).
    unsafe { str_case_with(ctx, s, pos_id, crate::strops::to_upper) }
}

/// `toLowerCase()`: Unicode Default Case Conversion (Q21).
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_to_lower(
    ctx: *mut Context,
    s: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract (forwarded).
    unsafe { str_case_with(ctx, s, pos_id, crate::strops::to_lower) }
}

/// `replace(pat, repl)`: first occurrence with ECMA string-pattern `$`
/// substitutions (Q27).
///
/// # Safety
///
/// Shared contract; `s`, `pat`, and `repl` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_replace(
    ctx: *mut Context,
    s: *const u8,
    pat: *const u8,
    repl: *const u8,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() || pat.is_null() || repl.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handles. Copied out so the borrows do not
    // overlap the mutable alloc call below.
    let bytes: Vec<u8> = unsafe { ctx.str_bytes(s) }.to_vec();
    // SAFETY: live string handles.
    let pat: Vec<u8> = unsafe { ctx.str_bytes(pat) }.to_vec();
    // SAFETY: live string handles.
    let repl: Vec<u8> = unsafe { ctx.str_bytes(repl) }.to_vec();
    ctx.alloc_str(&crate::strops::replace_first(&bytes, &pat, &repl), pos_id)
}

/// `replaceAll(pat, repl)`: every occurrence in one left-to-right pass
/// (a replacement is never rescanned), with ECMA string-pattern `$`
/// substitutions (Q27). An empty `pat` traps (JS inserts between every
/// unit) and returns null.
///
/// # Safety
///
/// Shared contract; `s`, `pat`, and `repl` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_replace_all(
    ctx: *mut Context,
    s: *const u8,
    pat: *const u8,
    repl: *const u8,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() || pat.is_null() || repl.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handles. Copied out so the borrows do not
    // overlap the mutable trap/alloc calls below.
    let bytes: Vec<u8> = unsafe { ctx.str_bytes(s) }.to_vec();
    // SAFETY: live string handles.
    let pat: Vec<u8> = unsafe { ctx.str_bytes(pat) }.to_vec();
    // SAFETY: live string handles.
    let repl: Vec<u8> = unsafe { ctx.str_bytes(repl) }.to_vec();
    if pat.is_empty() {
        ctx.trap(
            TrapKind::StrRange,
            "replaceAll(\"\", ...): an empty pattern is not accepted",
            pos_id,
        );
        return std::ptr::null_mut();
    }
    ctx.alloc_str(&crate::strops::replace_all(&bytes, &pat, &repl), pos_id)
}

// ----- RegExp (stdlib.md §15, Q31) -----

/// Compiles or reuses a Context-cached regular expression.
///
/// # Safety
///
/// Shared contract; `pattern` and `flags` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_new(
    ctx: *mut Context,
    pattern: *const u8,
    flags: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::new(unsafe { &mut *ctx }, pattern, flags, pos_id)
}

/// `RegExp.test`, with distinguishable budget-exhaustion trapping.
///
/// # Safety
///
/// Shared contract; `regex` and `subject` are live handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_test(
    ctx: *mut Context,
    regex: *const u8,
    subject: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::test(unsafe { &mut *ctx }, regex, subject, pos_id)
}

/// Returns `RegExp.source` without allocating.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_source(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::source(unsafe { &mut *ctx }, regex, pos_id)
}

/// Returns canonical `RegExp.flags` without allocating.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_flags(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::flags(unsafe { &mut *ctx }, regex, pos_id)
}

/// `string.search(RegExp)`, returning a UTF-8 byte offset or -1.
///
/// # Safety
///
/// Shared contract; `subject` and `regex` are live handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_search(
    ctx: *mut Context,
    subject: *const u8,
    regex: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::search(unsafe { &mut *ctx }, subject, regex, pos_id)
}

/// `string.replace(RegExp, replacement)` using the shared substituter.
///
/// # Safety
///
/// Shared contract; every pointer after `ctx` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_replace(
    ctx: *mut Context,
    subject: *const u8,
    regex: *const u8,
    replacement: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::replace(unsafe { &mut *ctx }, subject, regex, replacement, pos_id)
}

/// `string.replaceAll(RegExp, replacement)`, requiring `g`.
///
/// # Safety
///
/// Shared contract; every pointer after `ctx` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_replace_all(
    ctx: *mut Context,
    subject: *const u8,
    regex: *const u8,
    replacement: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::replace_all(unsafe { &mut *ctx }, subject, regex, replacement, pos_id)
}

/// `string.split(RegExp)` with capture reinjection.
///
/// # Safety
///
/// Shared contract; `subject` and `regex` are live handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_split(
    ctx: *mut Context,
    subject: *const u8,
    regex: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::split(unsafe { &mut *ctx }, subject, regex, pos_id)
}

/// Returns the last match's capture start byte, or -1.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_match_start(
    ctx: *mut Context,
    regex: *const u8,
    group: i32,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::match_boundary(unsafe { &mut *ctx }, regex, group, false, pos_id)
}

/// Returns the last match's capture end byte, or -1.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_match_end(
    ctx: *mut Context,
    regex: *const u8,
    group: i32,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::match_boundary(unsafe { &mut *ctx }, regex, group, true, pos_id)
}

// ----- Q14 formatting -----

fn alloc_formatted(ctx: &mut Context, bytes: &[u8], pos_id: u32) -> *mut u8 {
    ctx.alloc_str_with(bytes.len(), pos_id, |destination| {
        destination.copy_from_slice(bytes);
        bytes.len()
    })
}

/// Formats an `i32` (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fmt_i32(ctx: *mut Context, v: i32, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let mut storage = [0; crate::fmt::INTEGER_BUFFER_SIZE];
    alloc_formatted(ctx, crate::fmt::fmt_i32_into(v, &mut storage), pos_id)
}

/// Formats a `u32` (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fmt_u32(ctx: *mut Context, v: u32, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let mut storage = [0; crate::fmt::INTEGER_BUFFER_SIZE];
    alloc_formatted(ctx, crate::fmt::fmt_u32_into(v, &mut storage), pos_id)
}

/// Formats an `i64` (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fmt_i64(ctx: *mut Context, v: i64, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let mut storage = [0; crate::fmt::INTEGER_BUFFER_SIZE];
    alloc_formatted(ctx, crate::fmt::fmt_i64_into(v, &mut storage), pos_id)
}

/// Formats a `u64` (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fmt_u64(ctx: *mut Context, v: u64, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let mut storage = [0; crate::fmt::INTEGER_BUFFER_SIZE];
    alloc_formatted(ctx, crate::fmt::fmt_u64_into(v, &mut storage), pos_id)
}

/// Formats an `f32` at f32 precision (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fmt_f32(ctx: *mut Context, v: f32, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let mut storage = ryu_js::Buffer::new();
    alloc_formatted(
        ctx,
        crate::fmt::fmt_f32_into(v, &mut storage).as_bytes(),
        pos_id,
    )
}

/// Formats an `f64` (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fmt_f64(ctx: *mut Context, v: f64, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let mut storage = ryu_js::Buffer::new();
    alloc_formatted(
        ctx,
        crate::fmt::fmt_f64_into(v, &mut storage).as_bytes(),
        pos_id,
    )
}

/// Formats a boolean.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fmt_bool(ctx: *mut Context, v: u32, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc_str(crate::fmt::fmt_bool(v != 0).as_bytes(), pos_id)
}

// ----- JSON.stringify (stdlib.md §13, Q28) -----

fn json_builder_result(ctx: &mut Context, ok: bool, operation: &str, pos_id: u32) {
    if !ok {
        ctx.trap(
            TrapKind::Internal,
            format!("unknown JSON builder in {operation}"),
            pos_id,
        );
    }
}

/// Starts an untracked JSON output builder.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_begin(ctx: *mut Context, pos_id: u32) -> u64 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_builders().begin(false) {
        Some(id) => id,
        None => {
            ctx.trap(
                TrapKind::Internal,
                "JSON builder id space exhausted",
                pos_id,
            );
            0
        }
    }
}

/// Starts a JSON output builder with an active-reference set.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_begin_tracked(ctx: *mut Context, pos_id: u32) -> u64 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_builders().begin(true) {
        Some(id) => id,
        None => {
            ctx.trap(
                TrapKind::Internal,
                "JSON builder id space exhausted",
                pos_id,
            );
            0
        }
    }
}

/// Completes a JSON builder and allocates its immutable language string.
///
/// # Safety
///
/// Shared contract; `builder` was returned by one of the begin entries.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_finish(
    ctx: *mut Context,
    builder: u64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_builders().finish(builder) {
        Some(bytes) => ctx.alloc_str(&bytes, pos_id),
        None => {
            ctx.trap(TrapKind::Internal, "unknown JSON builder in finish", pos_id);
            std::ptr::null_mut()
        }
    }
}

/// Appends punctuation or another already-shaped JSON byte sequence.
///
/// # Safety
///
/// Shared contract; `value` is a live language string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_raw(
    ctx: *mut Context,
    builder: u64,
    value: *const u8,
    pos_id: u32,
) {
    // SAFETY: shared contract and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_bytes(value) }.to_vec();
    let ok = ctx.json_builders().raw(builder, &bytes);
    json_builder_result(ctx, ok, "raw append", pos_id);
}

/// Appends one quoted and escaped UTF-8 language string.
///
/// # Safety
///
/// Shared contract; `value` is a live language string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_str(
    ctx: *mut Context,
    builder: u64,
    value: *const u8,
    pos_id: u32,
) {
    // SAFETY: shared contract and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_bytes(value) }.to_vec();
    let ok = ctx.json_builders().string(builder, &bytes);
    json_builder_result(ctx, ok, "string append", pos_id);
}

macro_rules! json_integer {
    ($name:ident, $ty:ty, $method:ident) => {
        #[doc = concat!("Appends one JSON integer through the shared Q14 formatter.")]
        ///
        /// # Safety
        ///
        /// Shared contract; `builder` is live.
        #[no_mangle]
        pub unsafe extern "C" fn $name(ctx: *mut Context, builder: u64, value: $ty, pos_id: u32) {
            // SAFETY: shared contract.
            let ctx = unsafe { &mut *ctx };
            let ok = ctx.json_builders().$method(builder, value);
            json_builder_result(ctx, ok, stringify!($name), pos_id);
        }
    };
}

json_integer!(subscript_rt_json_i32, i32, i32);
json_integer!(subscript_rt_json_u32, u32, u32);
json_integer!(subscript_rt_json_i64, i64, i64);
json_integer!(subscript_rt_json_u64, u64, u64);

/// Appends one finite JSON `f32`, trapping on NaN or infinity.
///
/// # Safety
///
/// Shared contract; `builder` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_f32(
    ctx: *mut Context,
    builder: u64,
    value: f32,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    if !value.is_finite() {
        ctx.trap(
            TrapKind::JsonNumber,
            "JSON.stringify cannot serialize a non-finite number",
            pos_id,
        );
        return;
    }
    let ok = ctx.json_builders().f32(builder, value);
    json_builder_result(ctx, ok, "f32 append", pos_id);
}

/// Appends one finite JSON `f64`, trapping on NaN or infinity.
///
/// # Safety
///
/// Shared contract; `builder` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_f64(
    ctx: *mut Context,
    builder: u64,
    value: f64,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    if !value.is_finite() {
        ctx.trap(
            TrapKind::JsonNumber,
            "JSON.stringify cannot serialize a non-finite number",
            pos_id,
        );
        return;
    }
    let ok = ctx.json_builders().f64(builder, value);
    json_builder_result(ctx, ok, "f64 append", pos_id);
}

/// Appends a JSON boolean.
///
/// # Safety
///
/// Shared contract; `builder` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_bool(
    ctx: *mut Context,
    builder: u64,
    value: u8,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let bytes: &[u8] = if value == 0 { b"false" } else { b"true" };
    let ok = ctx.json_builders().raw(builder, bytes);
    json_builder_result(ctx, ok, "boolean append", pos_id);
}

/// Appends a Date as its quoted `toISOString()` result.
///
/// # Safety
///
/// Shared contract; `builder` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_date(
    ctx: *mut Context,
    builder: u64,
    value: i64,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(iso) = crate::date::to_iso(value) else {
        ctx.trap(
            TrapKind::DateRange,
            "JSON.stringify Date year is outside 0000..9999",
            pos_id,
        );
        return;
    };
    let ok = ctx.json_builders().string(builder, iso.as_bytes());
    json_builder_result(ctx, ok, "Date append", pos_id);
}

/// Appends JSON `null`.
///
/// # Safety
///
/// Shared contract; `builder` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_null(ctx: *mut Context, builder: u64, pos_id: u32) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let ok = ctx.json_builders().raw(builder, b"null");
    json_builder_result(ctx, ok, "null append", pos_id);
}

/// Adds a reference to the tracked serializer's active path. A revisit
/// records the P13 cycle trap and returns zero.
///
/// # Safety
///
/// Shared contract; `builder` is tracked and `reference` is a live
/// reference-class handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_visit(
    ctx: *mut Context,
    builder: u64,
    reference: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_builders().visit(builder, reference as usize) {
        crate::json::Visit::Inserted => 1,
        crate::json::Visit::Cycle => {
            ctx.trap(
                TrapKind::JsonCycle,
                "JSON.stringify encountered a cyclic reference",
                pos_id,
            );
            0
        }
        crate::json::Visit::InvalidBuilder => {
            ctx.trap(
                TrapKind::Internal,
                "unknown or untracked JSON builder in visit",
                pos_id,
            );
            0
        }
    }
}

/// Removes a completed reference from the tracked serializer's active
/// path.
///
/// # Safety
///
/// Shared contract; `builder` is tracked and `reference` was visited.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_leave(
    ctx: *mut Context,
    builder: u64,
    reference: *const u8,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let ok = ctx.json_builders().leave(builder, reference as usize);
    json_builder_result(ctx, ok, "leave", pos_id);
}

// ----- JSON.parse (stdlib.md §13.4, Q28) -----

fn json_parser_invalid(ctx: &mut Context, operation: &str, pos_id: u32) {
    ctx.trap(
        TrapKind::Internal,
        format!("invalid transient JSON parser access in {operation}"),
        pos_id,
    );
}

/// Parses a complete JSON document into transient runtime state.
/// Malformed input returns zero without trapping.
///
/// # Safety
///
/// Shared contract; `text` is a live language string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_begin(
    ctx: *mut Context,
    text: *const u8,
    _pos_id: u32,
) -> u64 {
    // SAFETY: shared contract and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_bytes(text) }.to_vec();
    ctx.json_parsers().begin(&bytes)
}

/// Removes one transient parsed document.
///
/// # Safety
///
/// Shared contract; `parser` is a nonzero handle returned by parse begin.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_end(ctx: *mut Context, parser: u64, pos_id: u32) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    if !ctx.json_parsers().finish(parser) {
        json_parser_invalid(ctx, "end", pos_id);
    }
}

/// Returns the root node handle of a transient parsed document.
///
/// # Safety
///
/// Shared contract; `parser` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_root(
    ctx: *mut Context,
    parser: u64,
    pos_id: u32,
) -> u64 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_parsers().root(parser) {
        Some(root) => root,
        None => {
            json_parser_invalid(ctx, "root", pos_id);
            0
        }
    }
}

/// Tests a parsed node's JSON kind.
///
/// # Safety
///
/// Shared contract; `parser` and `node` are live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_is_kind(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    kind: u32,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_parsers().is_kind(parser, node, kind) {
        Some(value) => i32::from(value),
        None => {
            json_parser_invalid(ctx, "kind test", pos_id);
            0
        }
    }
}

/// Tests whether a parsed number can populate one exact sized numeric
/// target without producing an out-of-range integer or non-finite float.
///
/// # Safety
///
/// Shared contract; `parser` and `node` are live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_number_fits(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    target: u32,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_parsers().number_fits(parser, node, target) {
        Some(value) => i32::from(value),
        None => {
            json_parser_invalid(ctx, "number validation", pos_id);
            0
        }
    }
}

/// Reads a previously validated parsed number as its ECMA `f64` value.
///
/// # Safety
///
/// Shared contract; the node was validated as an f32/f64 number.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_number(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    pos_id: u32,
) -> f64 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_parsers().number(parser, node) {
        Some(value) => value,
        None => {
            json_parser_invalid(ctx, "number read", pos_id);
            0.0
        }
    }
}

/// Reads a previously validated parsed number as one exact sized
/// integer. The returned `u64` carries the target value's bits; no
/// floating-point conversion occurs.
///
/// # Safety
///
/// Shared contract; the node was validated for `target`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_integer(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    target: u32,
    pos_id: u32,
) -> u64 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_parsers().integer(parser, node, target) {
        Some(value) => value,
        None => {
            json_parser_invalid(ctx, "integer read", pos_id);
            0
        }
    }
}

/// Reads a previously validated parsed boolean.
///
/// # Safety
///
/// Shared contract; the node was validated as a boolean.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_bool(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_parsers().boolean(parser, node) {
        Some(value) => i32::from(value),
        None => {
            json_parser_invalid(ctx, "boolean read", pos_id);
            0
        }
    }
}

/// Allocates a language string from a previously validated parsed string.
///
/// # Safety
///
/// Shared contract; the node was validated as a string.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_string(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(bytes) = ctx
        .json_parsers()
        .string(parser, node)
        .map(str::as_bytes)
        .map(<[u8]>::to_vec)
    else {
        json_parser_invalid(ctx, "string read", pos_id);
        return std::ptr::null_mut();
    };
    ctx.alloc_str(&bytes, pos_id)
}

/// Returns a previously validated parsed array's length.
///
/// # Safety
///
/// Shared contract; the node was validated as an array.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_array_len(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_parsers().array_len(parser, node) {
        Some(len) => match i32::try_from(len) {
            Ok(len) => len,
            Err(_) => {
                // Dynamic arrays use i32 indexing in the language, so an
                // unrepresentable JSON length cannot match any T[].
                -1
            }
        },
        None => {
            json_parser_invalid(ctx, "array length", pos_id);
            -1
        }
    }
}

/// Returns one node handle from a previously validated parsed array.
///
/// # Safety
///
/// Shared contract; `index` is in bounds.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_array_get(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    index: i32,
    pos_id: u32,
) -> u64 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let value = usize::try_from(index)
        .ok()
        .and_then(|index| ctx.json_parsers().array_get(parser, node, index));
    match value {
        Some(value) => value,
        None => {
            json_parser_invalid(ctx, "array element", pos_id);
            0
        }
    }
}

/// Returns the last occurrence of an object field, or zero when absent.
///
/// # Safety
///
/// Shared contract; `key` is a live language string handle and the node
/// was validated as an object.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_object_get(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    key: *const u8,
    pos_id: u32,
) -> u64 {
    // SAFETY: shared contract and live string handle.
    let ctx = unsafe { &mut *ctx };
    let key = unsafe { ctx.str_bytes(key) }.to_vec();
    let Some(key) = std::str::from_utf8(&key).ok() else {
        json_parser_invalid(ctx, "object key", pos_id);
        return 0;
    };
    match ctx.json_parsers().object_get(parser, node, key) {
        Some(value) => value,
        None => {
            json_parser_invalid(ctx, "object field", pos_id);
            0
        }
    }
}

// ----- Number and parsing intrinsics (stdlib.md §11, Q25/Q26) -----
//
// All operations stay behind opaque symbols so both tiers execute the
// same Rust implementation. The predicates are pure; parsing and
// formatting entries carry a position for allocation/range traps.

/// IEEE floating remainder used by both code-generation tiers.
#[no_mangle]
pub extern "C" fn subscript_rt_fmod(_ctx: *mut Context, left: f64, right: f64) -> f64 {
    left % right
}

/// `Number.isNaN(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_is_nan(_ctx: *mut Context, value: f64) -> i32 {
    i32::from(crate::num::is_nan(value))
}

/// `Number.isFinite(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_is_finite(_ctx: *mut Context, value: f64) -> i32 {
    i32::from(crate::num::is_finite(value))
}

/// `Number.isInteger(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_is_integer(_ctx: *mut Context, value: f64) -> i32 {
    i32::from(crate::num::is_integer(value))
}

/// `Number.isSafeInteger(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_is_safe_integer(_ctx: *mut Context, value: f64) -> i32 {
    i32::from(crate::num::is_safe_integer(value))
}

/// `parseInt(s, radix)`: explicit radix 2–36, otherwise a Q25 trap.
///
/// # Safety
///
/// Shared contract; `s` is a live UTF-8 string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_parse_int(
    ctx: *mut Context,
    s: *const u8,
    radix: i32,
    pos_id: u32,
) -> f64 {
    if s.is_null() {
        return f64::NAN;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    if !(2..=36).contains(&radix) {
        ctx.trap(
            TrapKind::NumberRange,
            format!("parseInt radix must be in 2..=36, got {radix}"),
            pos_id,
        );
        return f64::NAN;
    }
    // SAFETY: live string handle. Copy out before any mutable Context
    // operation, keeping the borrow boundary explicit.
    let bytes = unsafe { ctx.str_bytes(s) }.to_vec();
    let Ok(value) = std::str::from_utf8(&bytes) else {
        ctx.trap(
            TrapKind::Internal,
            "parseInt received a non-UTF-8 language string",
            pos_id,
        );
        return f64::NAN;
    };
    crate::num::parse_int(value, radix as u32)
}

/// `parseFloat(s)`: ECMA longest-prefix parsing.
///
/// # Safety
///
/// Shared contract; `s` is a live UTF-8 string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_parse_float(
    ctx: *mut Context,
    s: *const u8,
    pos_id: u32,
) -> f64 {
    if s.is_null() {
        return f64::NAN;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handle.
    let bytes = unsafe { ctx.str_bytes(s) }.to_vec();
    let Ok(value) = std::str::from_utf8(&bytes) else {
        ctx.trap(
            TrapKind::Internal,
            "parseFloat received a non-UTF-8 language string",
            pos_id,
        );
        return f64::NAN;
    };
    crate::num::parse_float(value)
}

/// `value.toFixed(digits)`: exact ECMA decimal rounding for digits
/// 0–100, otherwise a Q25 trap.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_to_fixed(
    ctx: *mut Context,
    value: f64,
    digits: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Ok(digits) = u32::try_from(digits) else {
        ctx.trap(
            TrapKind::NumberRange,
            format!("toFixed digits must be in 0..=100, got {digits}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    if digits > 100 {
        ctx.trap(
            TrapKind::NumberRange,
            format!("toFixed digits must be in 0..=100, got {digits}"),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    ctx.alloc_str(crate::num::to_fixed(value, digits).as_bytes(), pos_id)
}

/// `f32_value.toString(radix)`: radix 2–36, with radix 10 delegated
/// exactly to the Q14 `f32` formatter.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_to_string_f32(
    ctx: *mut Context,
    value: f32,
    radix: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Ok(radix) = u32::try_from(radix) else {
        ctx.trap(
            TrapKind::NumberRange,
            format!("toString radix must be in 2..=36, got {radix}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    if !(2..=36).contains(&radix) {
        ctx.trap(
            TrapKind::NumberRange,
            format!("toString radix must be in 2..=36, got {radix}"),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    ctx.alloc_str(
        crate::num::to_string_radix_f32(value, radix).as_bytes(),
        pos_id,
    )
}

/// `f64_value.toString(radix)`: radix 2–36, with radix 10 delegated
/// exactly to Q14.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_to_string_f64(
    ctx: *mut Context,
    value: f64,
    radix: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Ok(radix) = u32::try_from(radix) else {
        ctx.trap(
            TrapKind::NumberRange,
            format!("toString radix must be in 2..=36, got {radix}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    if !(2..=36).contains(&radix) {
        ctx.trap(
            TrapKind::NumberRange,
            format!("toString radix must be in 2..=36, got {radix}"),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    ctx.alloc_str(
        crate::num::to_string_radix_f64(value, radix).as_bytes(),
        pos_id,
    )
}

/// `value.toExponential(digits?)`: `-1` represents the checker-normalized
/// omitted argument; supplied digits must be in 0–100.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_to_exponential(
    ctx: *mut Context,
    value: f64,
    digits: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let digits = if digits == -1 {
        None
    } else {
        match u32::try_from(digits) {
            Ok(digits) if digits <= 100 => Some(digits),
            _ => {
                ctx.trap(
                    TrapKind::NumberRange,
                    format!("toExponential digits must be in 0..=100, got {digits}"),
                    pos_id,
                );
                return std::ptr::null_mut();
            }
        }
    };
    ctx.alloc_str(crate::num::to_exponential(value, digits).as_bytes(), pos_id)
}

/// `value.toPrecision(digits)`: digits must be in 1–100.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_to_precision(
    ctx: *mut Context,
    value: f64,
    digits: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Ok(digits) = u32::try_from(digits) else {
        ctx.trap(
            TrapKind::NumberRange,
            format!("toPrecision digits must be in 1..=100, got {digits}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    if !(1..=100).contains(&digits) {
        ctx.trap(
            TrapKind::NumberRange,
            format!("toPrecision digits must be in 1..=100, got {digits}"),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    ctx.alloc_str(crate::num::to_precision(value, digits).as_bytes(), pos_id)
}

// ----- Math (stdlib.md §1/§2) -----
//
// Every `subscript_rt_math_*` symbol takes the Context pointer first, so both
// tiers emit every Math call identically. The f64 subset returns f64;
// clz32 is `(ctx, u32) -> i32`, imul is `(ctx, i32, i32) -> i32`, and
// fround is `(ctx, f64) -> f64`. The binary32 bit accessors use `u32`
// for their bit-pattern side. Pure entries ignore `ctx`; only random
// reads Context state. Both tiers call these opaque symbols. They never use
// a direct libm or builtin operation (stdlib.md §0.2/Q26/Q27).

/// Declares the C entry of a pure unary `Math` member: `f(ctx, x)`
/// forwarding to [`crate::math`].
macro_rules! math_ffi_unary {
    ($( $(#[$doc:meta])* $sym:ident => $imp:ident ),* $(,)?) => {
        $(
            $(#[$doc])*
            #[no_mangle]
            pub extern "C" fn $sym(ctx: *mut Context, x: f64) -> f64 {
                let _ = ctx; // uniform signature; the operation is pure
                crate::math::$imp(x)
            }
        )*
    };
}

/// Declares the C entry of a pure binary `Math` member: `f(ctx, a, b)`
/// forwarding to [`crate::math`].
macro_rules! math_ffi_binary {
    ($( $(#[$doc:meta])* $sym:ident => $imp:ident ),* $(,)?) => {
        $(
            $(#[$doc])*
            #[no_mangle]
            pub extern "C" fn $sym(ctx: *mut Context, a: f64, b: f64) -> f64 {
                let _ = ctx; // uniform signature; the operation is pure
                crate::math::$imp(a, b)
            }
        )*
    };
}

math_ffi_unary! {
    /// `Math.abs`.
    subscript_rt_math_abs => abs,
    /// `Math.acos`.
    subscript_rt_math_acos => acos,
    /// `Math.acosh`.
    subscript_rt_math_acosh => acosh,
    /// `Math.asin`.
    subscript_rt_math_asin => asin,
    /// `Math.asinh`.
    subscript_rt_math_asinh => asinh,
    /// `Math.atan`.
    subscript_rt_math_atan => atan,
    /// `Math.atanh`.
    subscript_rt_math_atanh => atanh,
    /// `Math.cbrt`.
    subscript_rt_math_cbrt => cbrt,
    /// `Math.ceil`.
    subscript_rt_math_ceil => ceil,
    /// `Math.cos`.
    subscript_rt_math_cos => cos,
    /// `Math.cosh`.
    subscript_rt_math_cosh => cosh,
    /// `Math.exp`.
    subscript_rt_math_exp => exp,
    /// `Math.expm1`.
    subscript_rt_math_expm1 => expm1,
    /// `Math.floor`.
    subscript_rt_math_floor => floor,
    /// `Math.log`.
    subscript_rt_math_log => log,
    /// `Math.log1p`.
    subscript_rt_math_log1p => log1p,
    /// `Math.log10`.
    subscript_rt_math_log10 => log10,
    /// `Math.log2`.
    subscript_rt_math_log2 => log2,
    /// `Math.round` (ECMA half-toward-+∞).
    subscript_rt_math_round => round,
    /// `Math.sign` (±0/±1/NaN).
    subscript_rt_math_sign => sign,
    /// `Math.sin`.
    subscript_rt_math_sin => sin,
    /// `Math.sinh`.
    subscript_rt_math_sinh => sinh,
    /// `Math.sqrt`.
    subscript_rt_math_sqrt => sqrt,
    /// `Math.tan`.
    subscript_rt_math_tan => tan,
    /// `Math.tanh`.
    subscript_rt_math_tanh => tanh,
    /// `Math.trunc`.
    subscript_rt_math_trunc => trunc,
}

math_ffi_binary! {
    /// `Math.atan2(y, x)`.
    subscript_rt_math_atan2 => atan2,
    /// `Math.hypot(a, b)` (two arguments, Q19).
    subscript_rt_math_hypot => hypot,
    /// `Math.pow(base, exp)` (ECMA edges).
    subscript_rt_math_pow => pow,
    /// `Math.max(a, b)` (NaN propagation, zero ordering).
    subscript_rt_math_max => max,
    /// `Math.min(a, b)` (NaN propagation, zero ordering).
    subscript_rt_math_min => min,
}

/// `Math.clz32(x)`: Rust defines the zero input as 32; this opaque
/// entry prevents the ship tier from emitting C's undefined
/// `__builtin_clz(0)`.
#[no_mangle]
pub extern "C" fn subscript_rt_math_clz32(ctx: *mut Context, x: u32) -> i32 {
    let _ = ctx;
    crate::math::clz32(x)
}

/// `Math.imul(a, b)`: wrapping 32-bit multiplication.
#[no_mangle]
pub extern "C" fn subscript_rt_math_imul(ctx: *mut Context, a: i32, b: i32) -> i32 {
    let _ = ctx;
    crate::math::imul(a, b)
}

/// `Math.fround(x)`: exact `f64 -> f32 -> f64` rounding.
#[no_mangle]
pub extern "C" fn subscript_rt_math_fround(ctx: *mut Context, x: f64) -> f64 {
    let _ = ctx;
    crate::math::fround(x)
}

/// `Math.f32ToBits(value)`: narrow and return canonical binary32 bits.
#[no_mangle]
pub extern "C" fn subscript_rt_math_f32_to_bits(ctx: *mut Context, x: f64) -> u32 {
    let _ = ctx;
    crate::math::f32_to_bits(x)
}

/// `Math.f32FromBits(bits)`: widen binary32 bits exactly to binary64.
#[no_mangle]
pub extern "C" fn subscript_rt_math_f32_from_bits(ctx: *mut Context, bits: u32) -> f64 {
    let _ = ctx;
    crate::math::f32_from_bits(bits)
}

/// `Math.random()` (stdlib.md §2): the next deterministic draw from the
/// Context-owned xoshiro256++ stream.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_math_random(ctx: *mut Context) -> f64 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.random_f64()
}

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

// ----- Date (stdlib.md §3) -----
//
// One implementation, both tiers, through these opaque symbols (never a
// direct libc time call in generated code). A `Date` value crosses this
// boundary as its `i64` epoch-millisecond representation. The trapping
// entries (`utc`, `new`, `to_iso`) carry a trailing `pos_id`; the pure
// accessors do not trap and take none.

/// `Date.UTC(y, m0, d, h, min, s, ms)` — the checker always supplies
/// all seven arguments (missing trailing ones default to day 1 / time
/// 0 at check time). ECMA MakeDay/MakeFullYear semantics; a result
/// outside the TimeClip range traps (Q20) and returns 0.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_date_utc(
    ctx: *mut Context,
    year: i32,
    month0: i32,
    day: i32,
    hours: i32,
    minutes: i32,
    seconds: i32,
    millis: i32,
    pos_id: u32,
) -> i64 {
    match crate::date::utc_ms(year, month0, day, hours, minutes, seconds, millis) {
        Some(ms) => ms,
        None => {
            // SAFETY: shared contract.
            unsafe { &mut *ctx }.trap(
                TrapKind::DateRange,
                "Date out of range: Date.UTC result exceeds the valid time range \
                 (|ms| <= 8640000000000000)",
                pos_id,
            );
            0
        }
    }
}

/// `new Date(ms)`: the identity on an in-range time value; out of the
/// TimeClip range traps (Q20 — no Invalid-Date value) and returns 0.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_date_new(ctx: *mut Context, ms: i64, pos_id: u32) -> i64 {
    if crate::date::in_range(ms) {
        return ms;
    }
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.trap(
        TrapKind::DateRange,
        format!("Date out of range: {ms} ms (valid: |ms| <= 8640000000000000)"),
        pos_id,
    );
    0
}

/// `Date.now()`: current UTC milliseconds from the Context clock —
/// pinned by [`subscript_rt_ctx_set_now`], else the system clock.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_date_now(ctx: *mut Context) -> i64 {
    // SAFETY: shared contract.
    unsafe { &*ctx }.now_utc_ms()
}

/// One UTC accessor on a Date's millisecond value, selected by its
/// `FIELD_*` code ([`crate::date`]). An unknown code is a
/// compiler/runtime disagreement: reported as an internal trap, never
/// a panic.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_date_get(ctx: *mut Context, ms: i64, field: u32) -> i32 {
    match crate::date::get_field(ms, field) {
        Some(v) => v,
        None => {
            // SAFETY: shared contract.
            unsafe { &mut *ctx }.trap(
                TrapKind::Internal,
                format!("unknown Date accessor field code {field}"),
                0,
            );
            0
        }
    }
}

/// `toISOString()`: allocates the `YYYY-MM-DDTHH:mm:ss.sssZ` string;
/// a year outside 0000–9999 traps (Q20) and returns null.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_date_to_iso(
    ctx: *mut Context,
    ms: i64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match crate::date::to_iso(ms) {
        Some(s) => ctx.alloc_str(s.as_bytes(), pos_id),
        None => {
            let year = crate::date::decompose(ms).year;
            ctx.trap(
                TrapKind::DateRange,
                format!("toISOString requires a year in 0000-9999, got year {year}"),
                pos_id,
            );
            std::ptr::null_mut()
        }
    }
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

// ----- arrays (Q4) -----

/// Allocates an empty dynamic array of `elem_size`-byte elements.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_new(
    ctx: *mut Context,
    elem_size: u64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.array_new(elem_size as usize, pos_id)
}

/// Allocates an empty dynamic array with storage for `capacity` elements.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_with_capacity(
    ctx: *mut Context,
    capacity: u64,
    elem_size: u64,
    pos_id: u32,
) -> *mut u8 {
    let runtime = unsafe { &mut *ctx };
    let (Ok(capacity), Ok(elem_size)) = (usize::try_from(capacity), usize::try_from(elem_size))
    else {
        runtime.trap(
            TrapKind::AllocationFailure,
            "array capacity is not representable",
            pos_id,
        );
        return std::ptr::null_mut();
    };
    runtime.array_with_capacity(capacity, elem_size, pos_id)
}

/// Allocates a byte array and copies a readable byte span into it.
///
/// # Safety
///
/// Shared contract; `src` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_from_bytes(
    ctx: *mut Context,
    src: *const u8,
    len: u32,
    pos_id: u32,
) -> *mut u8 {
    let runtime = unsafe { &mut *ctx };
    if src.is_null() && len > 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    unsafe { runtime.array_from_bytes(src, len as usize, pos_id) }
}

/// Returns a writable range within a byte array.
///
/// This function traps with `IndexOutOfBounds` when the range exceeds the array length.
///
/// # Safety
///
/// Shared contract; `array` is a live byte-array handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_byte_range(
    ctx: *mut Context,
    array: *mut u8,
    offset: u32,
    size: u32,
    pos_id: u32,
) -> *mut u8 {
    let runtime = unsafe { &mut *ctx };
    if array.is_null() || !runtime.require_live_handle(array as usize, pos_id) {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    unsafe { runtime.array_byte_range(array, offset, size, pos_id) }
}

/// Array length.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_len(ctx: *mut Context, a: *const u8) -> i32 {
    if a.is_null() {
        return 0;
    }
    // SAFETY: shared contract; live array handle.
    unsafe { (*ctx).array_len(a) }
}

/// `push(value)`: appends a copy of `*src`; returns the new length.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle, `src` readable for
/// the element size.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_push(
    ctx: *mut Context,
    a: *mut u8,
    src: *const u8,
    pos_id: u32,
) -> i32 {
    let runtime = unsafe { &mut *ctx };
    if a.is_null() || src.is_null() || !runtime.require_live_handle(a as usize, pos_id) {
        return -1;
    }
    // SAFETY: receiver liveness was checked above; the shared contract
    // guarantees `src` is readable for the element size.
    unsafe { runtime.array_push(a, src, pos_id) }
}

/// Appends a snapshot of a dynamic array to a fresh array literal.
///
/// # Safety
///
/// `out` and `source` are live arrays with identical element width.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_spread_array(
    ctx: *mut Context,
    out: *mut u8,
    source: *mut u8,
    pos_id: u32,
) {
    let runtime = unsafe { &mut *ctx };
    if !runtime.require_live_handle(out as usize, pos_id)
        || !runtime.require_live_handle(source as usize, pos_id)
    {
        return;
    }
    // SAFETY: validated array handles.
    let count = unsafe { runtime.array_len(source) }.max(0) as usize;
    // SAFETY: validated source.
    let width = unsafe { runtime.array_elem_size(source) };
    for index in 0..count {
        // SAFETY: snapshot index is in range and no script runs here.
        let data = unsafe { runtime.array_data(source) };
        // SAFETY: source storage contains `count` initialized elements.
        let value = unsafe { data.add(index * width) };
        // SAFETY: output has the same element width.
        if unsafe { runtime.array_push(out, value, pos_id) } < 0 {
            break;
        }
    }
}

/// Appends a fixed-array buffer to a fresh array literal.
///
/// # Safety
///
/// `data` holds `count` elements of the output array's element width.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_spread_fixed(
    ctx: *mut Context,
    out: *mut u8,
    data: *const u8,
    count: u64,
    pos_id: u32,
) {
    if data.is_null() {
        return;
    }
    let runtime = unsafe { &mut *ctx };
    if !runtime.require_live_handle(out as usize, pos_id) {
        return;
    }
    // SAFETY: validated output array.
    let width = unsafe { runtime.array_elem_size(out) };
    for index in 0..count as usize {
        // SAFETY: caller-provided fixed buffer contract.
        let value = unsafe { data.add(index * width) };
        // SAFETY: value has output width.
        if unsafe { runtime.array_push(out, value, pos_id) } < 0 {
            break;
        }
    }
}

/// Appends the insertion-ordered keys of a Map/Set to a fresh array
/// literal, using the same fixed traversal bound as `forEach`/`for…of`.
///
/// # Safety
///
/// `out` is a live array and `source` a live Map/Set whose key width
/// matches the output element width.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_spread_assoc(
    ctx: *mut Context,
    out: *mut u8,
    source: *mut u8,
    pos_id: u32,
) {
    let runtime = unsafe { &mut *ctx };
    if !runtime.require_live_handle(out as usize, pos_id)
        || !assoc_receiver_is_live(runtime, source, pos_id)
    {
        return;
    }
    // Map/Set keys are limited to at most one machine word.
    let mut scratch = [0u8; 8];
    // SAFETY: validated receiver.
    let bound = unsafe { crate::assocops::iteration_begin(source) };
    for index in 0..bound {
        // SAFETY: scratch covers every accepted key width.
        if unsafe { crate::assocops::iteration_copy(source, index, false, scratch.as_mut_ptr()) }
            && unsafe { runtime.array_push(out, scratch.as_ptr(), pos_id) } < 0
        {
            break;
        }
    }
    // SAFETY: matching traversal end.
    unsafe { crate::assocops::iteration_end(ctx, source) };
}

/// Appends one string handle per UTF-8 code point to a fresh array
/// literal.
///
/// # Safety
///
/// `out` is a live `string[]` and `source` a live string.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_spread_string(
    ctx: *mut Context,
    out: *mut u8,
    source: *const u8,
    pos_id: u32,
) {
    let runtime = unsafe { &mut *ctx };
    if !runtime.require_live_handle(out as usize, pos_id)
        || !runtime.require_live_handle(source as usize, pos_id)
    {
        return;
    }
    // SAFETY: validated string.
    let end = unsafe { runtime.str_bytes(source).len() } as i32;
    let mut index = 0i32;
    while index < end {
        let mut next = index;
        // SAFETY: validated source and writable next index.
        let value =
            unsafe { subscript_rt_str_iter_code_point(ctx, source, index, &mut next, pos_id) };
        if value.is_null() || unsafe { (&*ctx).trapped() } {
            break;
        }
        let stored = value;
        // SAFETY: output element is one string handle.
        if unsafe { (&mut *ctx).array_push(out, (&raw const stored).cast::<u8>(), pos_id) } < 0 {
            break;
        }
        index = next;
    }
}

/// `pop()`: removes the last element into `dst`; traps when empty.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle, `dst` writable for
/// the element size.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_pop(
    ctx: *mut Context,
    a: *mut u8,
    dst: *mut u8,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    unsafe { (*ctx).array_pop(a, dst, pos_id) }
}

/// Bounds-checked element address; null after an out-of-bounds trap.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_ptr(
    ctx: *mut Context,
    a: *mut u8,
    idx: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { (*ctx).array_elem_ptr(a, idx, pos_id) }
}

// ----- Array methods (stdlib.md §9, Q22) -----
//
// The logic lives in [`crate::arrops`]; these wrappers decode the kind
// tags. Convention: the receiver handle follows `ctx`; element values
// the runtime receives travel by pointer; script callbacks travel as a
// `(code, env)` language function value; allocating entries carry a
// trailing `pos_id`. Every callback-taking entry returns immediately
// when the Context is already trapped and re-checks the trap flag after
// each callback return (stdlib.md §9).

/// Decodes an element-kind tag. The code generators emit only known tags.
/// An unknown tag records an Internal trap and means a defect in this compiler,
/// not a program fault or a build mismatch.
///
/// # Safety
///
/// Shared contract.
unsafe fn decode_elem_kind(ctx: *mut Context, kind: u32) -> Option<crate::arrops::ElemKind> {
    let decoded = crate::arrops::ElemKind::from_u32(kind);
    if decoded.is_none() {
        // SAFETY: shared contract.
        unsafe { &mut *ctx }.trap(
            TrapKind::Internal,
            format!("unknown array element kind {kind}"),
            0,
        );
    }
    decoded
}

/// `indexOf(x)`: first index under per-kind `===` equality, or −1
/// (stdlib.md §9). `x` points at one element-sized value.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle, `x` readable for the
/// element size, string elements/needles live handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_index_of(
    ctx: *mut Context,
    a: *mut u8,
    x: *const u8,
    kind: u32,
) -> i32 {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return -1;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::index_of(ctx, a, x, kind) }
}

/// `lastIndexOf(x)`: last index or −1.
///
/// # Safety
///
/// As [`subscript_rt_arr_index_of`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_last_index_of(
    ctx: *mut Context,
    a: *mut u8,
    x: *const u8,
    kind: u32,
) -> i32 {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return -1;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::last_index_of(ctx, a, x, kind) }
}

/// `includes(x)`: 1 when found under SameValueZero equality, else 0.
///
/// # Safety
///
/// As [`subscript_rt_arr_index_of`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_includes(
    ctx: *mut Context,
    a: *mut u8,
    x: *const u8,
    kind: u32,
) -> i32 {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return 0;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::includes(ctx, a, x, kind) }
}

/// `join(sep)`: Q14-formatted elements separated by `sep` (stdlib.md
/// §9); a fresh string handle.
///
/// # Safety
///
/// Shared contract; `a` a live array handle, `sep` a live string
/// handle, string elements live handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_join(
    ctx: *mut Context,
    a: *mut u8,
    sep: *const u8,
    kind: u32,
    pos_id: u32,
) -> *mut u8 {
    let Some(kind) = crate::arrops::FmtKind::from_u32(kind) else {
        // SAFETY: shared contract.
        unsafe { &mut *ctx }.trap(
            TrapKind::Internal,
            format!("unknown array format kind {kind}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::join(ctx, a, sep, kind, pos_id) }
}

/// `slice(start, end)`: a fresh array of the clamped range (JS negative
/// rules; the checker spells a missing `end` as `i32::MAX`).
///
/// # Safety
///
/// Shared contract; `a` is a live array handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_slice(
    ctx: *mut Context,
    a: *mut u8,
    start: i32,
    end: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { crate::arrops::slice(ctx, a, start, end, pos_id) }
}

/// `fill(x, start, end)` in place (JS clamp rules); generated code
/// reuses the receiver handle as the expression's value.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle, `x` readable for the
/// element size.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_fill(
    ctx: *mut Context,
    a: *mut u8,
    x: *const u8,
    start: i32,
    end: i32,
) {
    // SAFETY: shared contract.
    unsafe { crate::arrops::fill(ctx, a, x, start, end) }
}

/// `reverse()` in place; generated code reuses the receiver handle as
/// the expression's value.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_reverse(ctx: *mut Context, a: *mut u8) {
    // SAFETY: shared contract.
    unsafe { crate::arrops::reverse(ctx, a) }
}

/// `concat(other)`: a fresh array of `a`'s then `b`'s elements.
///
/// # Safety
///
/// Shared contract; `a` and `b` are live array handles of equal element
/// size.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_concat(
    ctx: *mut Context,
    a: *mut u8,
    b: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { crate::arrops::concat(ctx, a, b, pos_id) }
}

/// `splice(start, deleteCount)`: delete-only structural mutation,
/// returning the removed elements as a fresh array.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_splice(
    ctx: *mut Context,
    a: *mut u8,
    start: i32,
    delete_count: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { crate::arrops::splice(ctx, a, start, delete_count, pos_id) }
}

/// `shift()`: removes the first element into `out`; an empty array
/// traps at `pos_id`.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle and `out` is writable
/// for one element.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_shift(
    ctx: *mut Context,
    a: *mut u8,
    out: *mut u8,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    unsafe { crate::arrops::shift(ctx, a, out, pos_id) }
}

/// `unshift(x)`: prepends exactly one element and returns the new
/// length.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle and `x` is readable for
/// one element.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_unshift(
    ctx: *mut Context,
    a: *mut u8,
    x: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    unsafe { crate::arrops::unshift(ctx, a, x, pos_id) }
}

/// `copyWithin(target, start, end)` in place with JS clamp rules.
/// Generated code reuses the receiver handle as the expression's value.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_copy_within(
    ctx: *mut Context,
    a: *mut u8,
    target: i32,
    start: i32,
    end: i32,
) {
    // SAFETY: shared contract.
    unsafe { crate::arrops::copy_within(ctx, a, target, start, end) }
}

/// `forEach(f)`: calls the language callback per element; aborts on the
/// first trap (stdlib.md §9).
///
/// # Safety
///
/// Shared contract; `a` is a live array handle; `code`/`env` are a
/// language function value of shape `(ctx, env, T) -> void` or
/// `(ctx, env, T, i32) -> void` for the element ABI; `indexed != 0`
/// selects the latter.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_for_each(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::for_each(ctx, a, code, env, kind, indexed != 0) }
}

/// `map(f)`: a fresh `ret_size`-byte-element array of callback results;
/// a mid-iteration trap aborts and returns the valid partial array.
///
/// # Safety
///
/// Shared contract; callback shape `(ctx, env, T) -> R` or
/// `(ctx, env, T, i32) -> R` for the element and result ABIs;
/// `indexed != 0` selects the latter.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_map(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    elem_kind: u32,
    ret_kind: u32,
    ret_size: u64,
    pos_id: u32,
    indexed: u32,
) -> *mut u8 {
    // SAFETY: shared contract (forwarded).
    let (Some(ek), Some(rk)) = (unsafe { decode_elem_kind(ctx, elem_kind) }, unsafe {
        decode_elem_kind(ctx, ret_kind)
    }) else {
        return std::ptr::null_mut();
    };
    // SAFETY: shared contract.
    unsafe {
        crate::arrops::map(
            ctx,
            a,
            code,
            env,
            ek,
            rk,
            ret_size as usize,
            pos_id,
            indexed != 0,
        )
    }
}

/// `filter(f)`: a fresh array of the elements whose predicate returned
/// true.
///
/// # Safety
///
/// Shared contract; callback shape `(ctx, env, T) -> boolean` or
/// `(ctx, env, T, i32) -> boolean`; `indexed != 0` selects the latter.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_filter(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: u32,
    pos_id: u32,
    indexed: u32,
) -> *mut u8 {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return std::ptr::null_mut();
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::filter(ctx, a, code, env, kind, pos_id, indexed != 0) }
}

/// `reduce(f, init)`: folds left; the accumulator travels in/out
/// through `acc` (`acc_size` bytes of `acc_kind`). On a callback trap
/// the last completed accumulator remains in `acc`.
///
/// # Safety
///
/// Shared contract; callback shape `(ctx, env, A, T) -> A` or
/// `(ctx, env, A, T, i32) -> A`; `acc` is readable and writable for
/// `acc_size` bytes, and `indexed != 0` selects the latter callback
/// shape.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_reduce(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    elem_kind: u32,
    acc_kind: u32,
    acc_size: u64,
    acc: *mut u8,
    indexed: u32,
) {
    // SAFETY: shared contract (forwarded).
    let (Some(ek), Some(ak)) = (unsafe { decode_elem_kind(ctx, elem_kind) }, unsafe {
        decode_elem_kind(ctx, acc_kind)
    }) else {
        return;
    };
    // SAFETY: shared contract.
    unsafe {
        crate::arrops::reduce(
            ctx,
            a,
            code,
            env,
            ek,
            ak,
            acc_size as usize,
            acc,
            indexed != 0,
        )
    }
}

/// `reduceRight(f, init)`: folds right-to-left; the accumulator travels
/// in/out through `acc`.
///
/// # Safety
///
/// As [`subscript_rt_arr_reduce`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_reduce_right(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    elem_kind: u32,
    acc_kind: u32,
    acc_size: u64,
    acc: *mut u8,
    indexed: u32,
) {
    // SAFETY: shared contract (forwarded).
    let (Some(ek), Some(ak)) = (unsafe { decode_elem_kind(ctx, elem_kind) }, unsafe {
        decode_elem_kind(ctx, acc_kind)
    }) else {
        return;
    };
    // SAFETY: shared contract.
    unsafe {
        crate::arrops::reduce_right(
            ctx,
            a,
            code,
            env,
            ek,
            ak,
            acc_size as usize,
            acc,
            indexed != 0,
        )
    }
}

/// `some(f)`: 1 when any element satisfies the predicate
/// (short-circuits), else 0.
///
/// # Safety
///
/// Shared contract; callback shape `(ctx, env, T) -> boolean` or
/// `(ctx, env, T, i32) -> boolean`; `indexed != 0` selects the latter.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_some(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) -> i32 {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return 0;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::some(ctx, a, code, env, kind, indexed != 0) }
}

/// `every(f)`: 1 when every element satisfies the predicate
/// (short-circuits on the first miss), else 0.
///
/// # Safety
///
/// As [`subscript_rt_arr_some`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_every(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) -> i32 {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return 0;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::every(ctx, a, code, env, kind, indexed != 0) }
}

/// `findIndex(f)`: the first satisfying index or −1 (short-circuits).
///
/// # Safety
///
/// As [`subscript_rt_arr_some`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_find_index(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) -> i32 {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return -1;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::find_index(ctx, a, code, env, kind, indexed != 0) }
}

// Q27 FixedArray callback family. Unlike the dynamic-array entries
// above, these receive the in-place element storage, compile-time
// length, and concrete tier element width directly.

/// `FixedArray.forEach` over in-place storage.
///
/// # Safety
///
/// `data` is readable for `len * elem_size` bytes; callback pointers
/// have the selected generated ABI.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fixed_arr_for_each(
    ctx: *mut Context,
    data: *const u8,
    len: u64,
    elem_size: u64,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) {
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return;
    };
    unsafe {
        crate::arrops::fixed_for_each(
            ctx,
            data,
            len as usize,
            elem_size as usize,
            code,
            env,
            kind,
            indexed != 0,
        )
    };
}

/// `FixedArray.map` into a fresh dynamic array.
///
/// # Safety
///
/// As [`subscript_rt_fixed_arr_for_each`], with the result ABI described by
/// `ret_kind` and `ret_size`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fixed_arr_map(
    ctx: *mut Context,
    data: *const u8,
    len: u64,
    elem_size: u64,
    code: *const u8,
    env: *const u8,
    elem_kind: u32,
    ret_kind: u32,
    ret_size: u64,
    pos_id: u32,
    indexed: u32,
) -> *mut u8 {
    let (Some(elem_kind), Some(ret_kind)) = (unsafe { decode_elem_kind(ctx, elem_kind) }, unsafe {
        decode_elem_kind(ctx, ret_kind)
    }) else {
        return std::ptr::null_mut();
    };
    unsafe {
        crate::arrops::fixed_map(
            ctx,
            data,
            len as usize,
            elem_size as usize,
            code,
            env,
            elem_kind,
            ret_kind,
            ret_size as usize,
            pos_id,
            indexed != 0,
        )
    }
}

/// `FixedArray.filter` into a fresh dynamic array.
///
/// # Safety
///
/// As [`subscript_rt_fixed_arr_for_each`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fixed_arr_filter(
    ctx: *mut Context,
    data: *const u8,
    len: u64,
    elem_size: u64,
    code: *const u8,
    env: *const u8,
    kind: u32,
    pos_id: u32,
    indexed: u32,
) -> *mut u8 {
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return std::ptr::null_mut();
    };
    unsafe {
        crate::arrops::fixed_filter(
            ctx,
            data,
            len as usize,
            elem_size as usize,
            code,
            env,
            kind,
            pos_id,
            indexed != 0,
        )
    }
}

unsafe fn fixed_arr_reduce_entry(
    right: bool,
    ctx: *mut Context,
    data: *const u8,
    len: u64,
    elem_size: u64,
    code: *const u8,
    env: *const u8,
    elem_kind: u32,
    acc_kind: u32,
    acc_size: u64,
    acc: *mut u8,
    indexed: u32,
) {
    let (Some(elem_kind), Some(acc_kind)) = (unsafe { decode_elem_kind(ctx, elem_kind) }, unsafe {
        decode_elem_kind(ctx, acc_kind)
    }) else {
        return;
    };
    if right {
        unsafe {
            crate::arrops::fixed_reduce_right(
                ctx,
                data,
                len as usize,
                elem_size as usize,
                code,
                env,
                elem_kind,
                acc_kind,
                acc_size as usize,
                acc,
                indexed != 0,
            )
        };
    } else {
        unsafe {
            crate::arrops::fixed_reduce(
                ctx,
                data,
                len as usize,
                elem_size as usize,
                code,
                env,
                elem_kind,
                acc_kind,
                acc_size as usize,
                acc,
                indexed != 0,
            )
        };
    }
}

/// `FixedArray.reduce` from the left.
///
/// # Safety
///
/// As [`subscript_rt_fixed_arr_for_each`]; `acc` is readable and writable for
/// `acc_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fixed_arr_reduce(
    ctx: *mut Context,
    data: *const u8,
    len: u64,
    elem_size: u64,
    code: *const u8,
    env: *const u8,
    elem_kind: u32,
    acc_kind: u32,
    acc_size: u64,
    acc: *mut u8,
    indexed: u32,
) {
    unsafe {
        fixed_arr_reduce_entry(
            false, ctx, data, len, elem_size, code, env, elem_kind, acc_kind, acc_size, acc,
            indexed,
        )
    };
}

/// `FixedArray.reduceRight` from the right.
///
/// # Safety
///
/// As [`subscript_rt_fixed_arr_reduce`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fixed_arr_reduce_right(
    ctx: *mut Context,
    data: *const u8,
    len: u64,
    elem_size: u64,
    code: *const u8,
    env: *const u8,
    elem_kind: u32,
    acc_kind: u32,
    acc_size: u64,
    acc: *mut u8,
    indexed: u32,
) {
    unsafe {
        fixed_arr_reduce_entry(
            true, ctx, data, len, elem_size, code, env, elem_kind, acc_kind, acc_size, acc, indexed,
        )
    };
}

unsafe fn fixed_arr_search_entry(
    operation: u8,
    ctx: *mut Context,
    data: *const u8,
    len: u64,
    elem_size: u64,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) -> i32 {
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return if operation == 2 { -1 } else { 0 };
    };
    match operation {
        0 => unsafe {
            crate::arrops::fixed_some(
                ctx,
                data,
                len as usize,
                elem_size as usize,
                code,
                env,
                kind,
                indexed != 0,
            )
        },
        1 => unsafe {
            crate::arrops::fixed_every(
                ctx,
                data,
                len as usize,
                elem_size as usize,
                code,
                env,
                kind,
                indexed != 0,
            )
        },
        _ => unsafe {
            crate::arrops::fixed_find_index(
                ctx,
                data,
                len as usize,
                elem_size as usize,
                code,
                env,
                kind,
                indexed != 0,
            )
        },
    }
}

/// `FixedArray.some`.
///
/// # Safety
///
/// As [`subscript_rt_fixed_arr_for_each`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fixed_arr_some(
    ctx: *mut Context,
    data: *const u8,
    len: u64,
    elem_size: u64,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) -> i32 {
    unsafe { fixed_arr_search_entry(0, ctx, data, len, elem_size, code, env, kind, indexed) }
}

/// `FixedArray.every`.
///
/// # Safety
///
/// As [`subscript_rt_fixed_arr_some`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fixed_arr_every(
    ctx: *mut Context,
    data: *const u8,
    len: u64,
    elem_size: u64,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) -> i32 {
    unsafe { fixed_arr_search_entry(1, ctx, data, len, elem_size, code, env, kind, indexed) }
}

/// `FixedArray.findIndex`.
///
/// # Safety
///
/// As [`subscript_rt_fixed_arr_some`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fixed_arr_find_index(
    ctx: *mut Context,
    data: *const u8,
    len: u64,
    elem_size: u64,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) -> i32 {
    unsafe { fixed_arr_search_entry(2, ctx, data, len, elem_size, code, env, kind, indexed) }
}

/// `sort(cmp)`: stable merge sort in place; a comparator trap leaves
/// the array exactly as it was (stdlib.md §9). Generated code reuses
/// the receiver handle as the expression's value.
///
/// # Safety
///
/// Shared contract; comparator shape `(ctx, env, T, T) -> i32`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_sort(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: u32,
) {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::sort(ctx, a, code, env, kind) }
}

// ----- C-boundary marshaling (P5.2b) -----

/// Data pointer of a string handle: the `const char*` half of a
/// `(ptr, len)` string view passed to a foreign call. Length is
/// [`subscript_rt_str_len`].
///
/// # Safety
///
/// Shared contract; `s` is a live string handle (or null).
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_data(ctx: *const Context, s: *const u8) -> *const u8 {
    if s.is_null() {
        return std::ptr::null();
    }
    // SAFETY: shared contract; live string handle.
    unsafe { (*ctx).str_data(s) }
}

/// Data pointer of a dynamic array: the `const T*` half of a
/// `(ptr, count)` descriptor passed to a foreign call. Count is
/// [`subscript_rt_array_len`]. Null for an array that has never grown.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle (or null).
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_data(ctx: *const Context, a: *const u8) -> *const u8 {
    if a.is_null() {
        return std::ptr::null();
    }
    // SAFETY: shared contract; live array handle.
    unsafe { (*ctx).array_data(a) }
}

/// Registers a C-callback binding and returns the stable pointer a
/// boundary marshaler stores in a C `void* userdata` slot (P5.2b). The
/// binding bundles the Context, the language function value's
/// `(code, env)`, and both real userdata slots (§14.4);
/// [`subscript_rt_cb_trampoline`] reads it back. The binding lives for the whole
/// Context (Q13 lifetime rule). Re-registering the same
/// `(code, userdata1, userdata2)` identity returns the same stable pointer
/// and allocates no new binding (§14.4a).
///
/// # Safety
///
/// Shared contract; `code`/`env` are a language function value (a
/// non-capturing wrapper, so `env` is null); `userdata1`/`userdata2`
/// outlive the run.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_cb_bind(
    ctx: *mut Context,
    code: *const u8,
    env: *const u8,
    userdata1: *mut u8,
    userdata2: *mut u8,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.bind_callback(code, env, userdata1, userdata2)
}

/// The generic C-ABI callback trampoline (P5.2b, §14.4). A C API invokes
/// it with the two-userdata callback ABI `(message, userdata1, userdata2)`,
/// where `userdata1` is the binding pointer a marshaler installed via
/// [`subscript_rt_cb_bind`]. It reconstructs the language `string` from the
/// `(ptr, len)` view, then calls the language function value under its own
/// convention `(ctx, env, message, userdata1, userdata2)`.
///
/// The binding is the authoritative source of both language userdata: the
/// marshaler installs the binding in the callback-info's first userdata
/// slot and null in the second, so the trampoline reads both userdata from
/// the binding and ignores its `userdata2` argument. The second C slot
/// exists for the production callback-info shape (offsetof-proven layout)
/// and is wired through the C fire path, but the language values travel in
/// the binding, not the raw C slot.
///
/// The Context reaches the trampoline through the binding (captured at
/// registration), not through global state: scripts are single-threaded
/// and trusted (invariant 6), the trampoline only ever runs synchronously
/// inside a foreign call made by generated code executing under that same
/// Context, so `binding.ctx` is always the live, correct Context. A trap
/// in the language callback sets the Context trap flag and returns a
/// zeroed value; the generated code that made the foreign call checks the
/// flag on return and unwinds, so the trap propagates without crossing
/// this boundary as an unwind.
///
/// # Safety
///
/// `userdata1` is a binding produced by [`subscript_rt_cb_bind`] on the running
/// Context; `message` points at `len` readable bytes (or is null/empty).
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_cb_trampoline(
    message: SubStrView,
    userdata1: *mut u8,
    userdata2: *mut u8,
) {
    // The binding travels in the first slot; the second slot is unused (the
    // binding carries both language userdata).
    let _ = userdata2;
    if userdata1.is_null() {
        return;
    }
    // SAFETY: `userdata1` is a live binding of the running Context.
    let rec = unsafe { &*(userdata1 as *const CallbackBinding) };
    // Copy the record fields before exclusively borrowing its owning
    // Context. Binding storage is stable for the Context's lifetime.
    let ctx_ptr = rec.ctx;
    let code = rec.code;
    let env = rec.env;
    let callback_userdata1 = rec.userdata1;
    let callback_userdata2 = rec.userdata2;
    // SAFETY: `ctx_ptr` is the live Context captured at bind time.
    let ctx = unsafe { &mut *ctx_ptr };
    // A trap already stopped the script (e.g. an earlier callback in the
    // same foreign call trapped): do not run script code — a trap stops
    // the run, even when a C API fires the callback more than once.
    if ctx.trapped() {
        return;
    }
    if !ctx.validate_callback_userdata(callback_userdata1)
        || !ctx.validate_callback_userdata(callback_userdata2)
    {
        return;
    }
    // SAFETY: the callback ABI guarantees this readable view. Reuse the
    // boundary copy-in implementation so callback parameters and struct
    // fields have exactly the same null/empty semantics.
    let s = unsafe { alloc_str_from_view(ctx, message.data, message.len as u64, 0) };
    // The language function value's wrapper takes `(ctx, env, args...)`
    // with the host C calling convention; here the args are the `string`
    // handle and the two userdata slots (§14.4).
    type LangCb = unsafe extern "C" fn(*mut Context, *const u8, *mut u8, *mut u8, *mut u8);
    // SAFETY: `code` is a language callback wrapper of this shape.
    let f: LangCb = unsafe { std::mem::transmute::<*const u8, LangCb>(code) };
    // SAFETY: calling generated code that never unwinds across FFI.
    unsafe { f(ctx_ptr, env, s, callback_userdata1, callback_userdata2) };
}

// ----- host driver entry points -----
//
// These are not called by generated code; they are the C-ABI surface a
// host entry program uses to drive an AOT-linked script
// (`specs/blocks/compiler.md` §8.1): create a Context, call the
// program's exported entries, then read the sink and the trap state.

/// Spawns a runtime-owned OS thread with a fresh dedicated Context.
///
/// The worker thread calls `init`, then calls `entry` with its Context and
/// worker-side endpoints unless initialization trapped. `input_payload_size`
/// is the byte size accepted by [`subscript_rt_worker_post`];
/// `output_payload_size` is the byte size accepted by
/// [`subscript_rt_worker_outbox_post`]. Both queues are unbounded byte-copy
/// queues. The returned handle is owned by `parent` and remains valid until
/// that Context is released. Null is returned after a parent trap when a
/// callback is missing, a size is not representable, or thread creation
/// fails.
///
/// # Safety
///
/// `parent` follows the exclusive Context contract. `init` and `entry` must
/// be linked C-callable functions that obey the runtime trap discipline and
/// remain callable until the worker is joined.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_worker_spawn(
    parent: *mut Context,
    init: Option<WorkerInit>,
    entry: Option<WorkerEntry>,
    input_payload_size: u64,
    output_payload_size: u64,
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
    let Ok(input_payload_size) = usize::try_from(input_payload_size) else {
        parent.trap(
            TrapKind::AllocationFailure,
            "worker input payload size is not representable",
            0,
        );
        return std::ptr::null_mut();
    };
    let Ok(output_payload_size) = usize::try_from(output_payload_size) else {
        parent.trap(
            TrapKind::AllocationFailure,
            "worker output payload size is not representable",
            0,
        );
        return std::ptr::null_mut();
    };
    parent.worker_spawn(init, entry, input_payload_size, output_payload_size)
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

/// Returns the number of suspended async root invocations owned by `ctx`.
///
/// # Safety
///
/// `ctx` follows the shared Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_async_pending(ctx: *const Context) -> u64 {
    // SAFETY: shared Context contract.
    unsafe { &*ctx }.async_pending() as u64
}

/// Resumes every root pending at call entry exactly once, in host kick
/// order, and returns the number still pending. On a trapped Context this
/// is a no-op returning the current count; an empty Context returns zero.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract. Generated code for every
/// pending root remains linked and callable.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_ctx_async_step(ctx: *mut Context) -> u64 {
    // SAFETY: exclusive Context contract and queued callbacks were installed
    // by generated code from the same live program.
    unsafe { (&mut *ctx).async_step() as u64 }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globals_init_conversion_failures_trap_before_returning_null() {
        for converted in [[None, Some(8usize)], [Some(8usize), None]] {
            let mut ctx = Context::new();
            let mut converted = converted.into_iter();
            let globals = globals_init_with_conversion(&mut ctx, 8, 8, |_| {
                converted
                    .next()
                    .expect("one conversion result per argument")
            });
            assert!(globals.is_null());
            let trap = ctx.trap_record().expect("conversion failure traps");
            assert_eq!(trap.kind, TrapKind::Internal);
            assert_eq!(
                trap.message,
                "module-global block layout is not representable"
            );
            assert_eq!(trap.pos_id, 0);
        }
    }

    struct ObservedTrap {
        calls: u32,
        kind: u32,
        pos_id: u32,
        message_ptr: *const u8,
        message_len: u64,
        message: Vec<u8>,
    }

    impl Default for ObservedTrap {
        fn default() -> Self {
            Self {
                calls: 0,
                kind: 0,
                pos_id: 0,
                message_ptr: std::ptr::null(),
                message_len: 0,
                message: Vec::new(),
            }
        }
    }

    unsafe extern "C" fn observe_trap(
        userdata: *mut std::ffi::c_void,
        kind: u32,
        pos_id: u32,
        message: *const u8,
        message_len: u64,
    ) {
        // SAFETY: the tests pass a live `ObservedTrap` as userdata and
        // keep it alive until after every callback.
        let observed = unsafe { &mut *userdata.cast::<ObservedTrap>() };
        observed.calls += 1;
        observed.kind = kind;
        observed.pos_id = pos_id;
        observed.message_ptr = message;
        observed.message_len = message_len;
        // SAFETY: the observer contract supplies `message_len` bytes
        // from the stored record for the callback and beyond.
        observed.message =
            unsafe { std::slice::from_raw_parts(message, message_len as usize) }.to_vec();
    }

    #[derive(Default)]
    struct ObservedPrint {
        lines: Vec<Vec<u8>>,
    }

    unsafe extern "C" fn observe_print(
        userdata: *mut std::ffi::c_void,
        line: *const u8,
        line_len: u64,
    ) {
        // SAFETY: the test passes a live `ObservedPrint` and the line is
        // readable for the duration of this callback.
        let observed = unsafe { &mut *userdata.cast::<ObservedPrint>() };
        // SAFETY: the print-observer contract supplies `line_len` readable
        // bytes for this callback.
        let line = unsafe { std::slice::from_raw_parts(line, line_len as usize) };
        observed.lines.push(line.to_vec());
    }

    #[derive(Default)]
    struct ObservedAdvisory {
        calls: u32,
        kind: u32,
        pos_id: u32,
        message: Vec<u8>,
    }

    unsafe extern "C" fn observe_advisory(
        userdata: *mut std::ffi::c_void,
        kind: u32,
        pos_id: u32,
        message: *const u8,
        message_len: u64,
    ) {
        // SAFETY: the test passes a live ObservedAdvisory as userdata.
        let observed = unsafe { &mut *userdata.cast::<ObservedAdvisory>() };
        observed.calls += 1;
        observed.kind = kind;
        observed.pos_id = pos_id;
        // SAFETY: the observer contract supplies these readable message
        // bytes for the duration of the call.
        observed.message =
            unsafe { std::slice::from_raw_parts(message, message_len as usize) }.to_vec();
    }

    #[test]
    fn ffi_f16_conversion_round_trips_raw_binary16_storage() {
        let bits = subscript_rt_f16_from_f64(1.0006);
        assert_eq!(bits, 0x3c01);
        assert_eq!(subscript_rt_f16_to_f64(bits), 1.0009765625);
        assert_eq!(
            subscript_rt_f16_to_f64(subscript_rt_f16_from_f64(-0.0)).to_bits(),
            (-0.0f64).to_bits()
        );
    }

    #[test]
    fn ffi_fmod_preserves_ieee_remainder_edges() {
        let ctx = std::ptr::null_mut();
        assert_eq!(subscript_rt_fmod(ctx, 5.5, 2.0), 1.5);
        assert_eq!(subscript_rt_fmod(ctx, -5.5, 2.0), -1.5);
        assert_eq!(subscript_rt_fmod(ctx, 5.5, -2.0), 1.5);
        assert!(subscript_rt_fmod(ctx, 5.5, 0.0).is_nan());
        assert!(subscript_rt_fmod(ctx, f64::INFINITY, 2.0).is_nan());
        assert_eq!(subscript_rt_fmod(ctx, 2.0, f64::INFINITY), 2.0);
        assert!(subscript_rt_fmod(ctx, f64::NAN, 2.0).is_nan());
    }

    #[test]
    fn ffi_string_view_copy_in_owns_bytes_and_zero_view_is_empty() {
        let mut ctx = Context::new();
        let ptr: *mut Context = &mut *ctx;
        let mut bytes = *b"field-view";
        // SAFETY: valid Context and readable views for each call.
        unsafe {
            let copied = subscript_rt_str_from_view(ptr, bytes.as_ptr(), bytes.len() as u64, 9);
            bytes.fill(b'x');
            assert_eq!(ctx.str_bytes(copied), b"field-view");

            let empty = subscript_rt_str_from_view(ptr, std::ptr::null(), 0, 10);
            assert_eq!(ctx.str_bytes(empty), b"");
        }
    }

    #[test]
    fn ffi_host_driver_round_trip() {
        let ctx = subscript_rt_ctx_new();
        assert!(!ctx.is_null());
        // SAFETY: `ctx` is the context just created; released once below.
        unsafe {
            let s = subscript_rt_str_lit(ctx, b"hi".as_ptr(), 2, 0);
            subscript_rt_print(ctx, s);
            let mut len: u64 = 0;
            let p = subscript_rt_ctx_stdout(ctx, &mut len);
            assert_eq!(std::slice::from_raw_parts(p, len as usize), b"hi\n");
            assert_eq!(subscript_rt_ctx_trap_kind(ctx), 0);
            let mut mlen: u64 = 1;
            assert!(subscript_rt_ctx_trap_message(ctx, &mut mlen).is_null());
            assert_eq!(mlen, 0);
            subscript_rt_trap(ctx, TrapKind::EmptyPop as u32, 4);
            assert_eq!(subscript_rt_ctx_trap_kind(ctx), TrapKind::EmptyPop as u32);
            assert_eq!(subscript_rt_ctx_trap_pos_id(ctx), 4);
            let m = subscript_rt_ctx_trap_message(ctx, &mut mlen);
            assert!(!m.is_null() && mlen > 0);
            subscript_rt_ctx_release(ctx);
        }
    }

    #[test]
    fn ffi_print_observer_delivers_without_retention_and_null_restores_sink() {
        let ctx = subscript_rt_ctx_new();
        assert!(!ctx.is_null());
        let mut observed = ObservedPrint::default();

        // SAFETY: `ctx`, callback userdata, and string handles remain live
        // through each call and the Context is released exactly once.
        unsafe {
            subscript_rt_ctx_set_print_observer(
                ctx,
                Some(observe_print),
                std::ptr::from_mut(&mut observed).cast(),
            );
            let delivered = subscript_rt_str_lit(ctx, b"delivered".as_ptr(), 9, 0);
            subscript_rt_print(ctx, delivered);

            let mut len = 1u64;
            let _ = subscript_rt_ctx_stdout(ctx, &mut len);
            assert_eq!(len, 0);
            assert_eq!(observed.lines, [b"delivered".to_vec()]);

            subscript_rt_ctx_set_print_observer(ctx, None, std::ptr::null_mut());
            let retained = subscript_rt_str_lit(ctx, b"retained".as_ptr(), 8, 0);
            subscript_rt_print(ctx, retained);
            let bytes = subscript_rt_ctx_stdout(ctx, &mut len);
            assert_eq!(
                std::slice::from_raw_parts(bytes, len as usize),
                b"retained\n"
            );
            assert_eq!(observed.lines, [b"delivered".to_vec()]);

            subscript_rt_ctx_release(ctx);
        }
    }

    #[test]
    fn ffi_diagnostics_observer_advises_on_callback_userdata_free() {
        fn callback_code() {}

        let ctx = subscript_rt_ctx_new();
        assert!(!ctx.is_null());
        let mut observed = ObservedAdvisory::default();

        // SAFETY: `ctx`, observer userdata, and the allocation remain live
        // through their calls; the Context is released exactly once.
        unsafe {
            subscript_rt_ctx_set_diagnostics_observer(
                ctx,
                Some(observe_advisory),
                std::ptr::from_mut(&mut observed).cast(),
            );
            let registered = subscript_rt_alloc(ctx, 16, 1, 20);
            assert!(!registered.is_null());
            subscript_rt_cb_bind(
                ctx,
                callback_code as *const () as *const u8,
                std::ptr::null(),
                registered,
                std::ptr::null_mut(),
            );
            subscript_rt_delete(ctx, registered, 93);

            assert_eq!(observed.calls, 1);
            assert_eq!(
                observed.kind,
                crate::DIAGNOSTICS_ADVISORY_CALLBACK_USERDATA_FREE
            );
            assert_eq!(observed.pos_id, 93);
            assert_eq!(
                observed.message,
                b"Context.free of registered callback userdata"
            );
            assert_eq!(subscript_rt_ctx_trap_kind(ctx), 0);

            subscript_rt_ctx_set_diagnostics_observer(ctx, None, std::ptr::null_mut());
            let after_clear = subscript_rt_alloc(ctx, 16, 1, 21);
            subscript_rt_cb_bind(
                ctx,
                callback_code as *const () as *const u8,
                std::ptr::null(),
                after_clear,
                std::ptr::null_mut(),
            );
            subscript_rt_delete(ctx, after_clear, 94);
            assert_eq!(observed.calls, 1, "null observer must clear delivery");

            subscript_rt_ctx_release(ctx);
        }
    }

    #[test]
    fn ffi_binding_count_advisory_reports_only_new_identity_at_threshold() {
        fn callback_code() {}

        let ctx = subscript_rt_ctx_new();
        assert!(!ctx.is_null());
        let mut observed = ObservedAdvisory::default();
        let mut first_userdata = 1u8;
        let mut second_userdata = 2u8;

        // SAFETY: `ctx`, observer userdata, and both callback userdata
        // addresses remain live through these calls; the Context is released
        // exactly once.
        unsafe {
            subscript_rt_ctx_set_diagnostics_observer(
                ctx,
                Some(observe_advisory),
                std::ptr::from_mut(&mut observed).cast(),
            );
            subscript_rt_ctx_set_binding_count_advisory(ctx, 2);
            let code = callback_code as *const () as *const u8;
            let first = subscript_rt_cb_bind(
                ctx,
                code,
                std::ptr::null(),
                std::ptr::from_mut(&mut first_userdata),
                std::ptr::null_mut(),
            );
            assert_eq!(observed.calls, 0, "below-threshold binding advised");

            let second = subscript_rt_cb_bind(
                ctx,
                code,
                std::ptr::null(),
                std::ptr::from_mut(&mut second_userdata),
                std::ptr::null_mut(),
            );
            assert_ne!(first, second);
            assert_eq!(observed.calls, 1);
            assert_eq!(observed.kind, crate::DIAGNOSTICS_ADVISORY_BINDING_COUNT);
            assert_eq!(observed.pos_id, 0);
            assert_eq!(
                observed.message,
                b"callback bindings: 2 registered, advisory threshold 2"
            );

            let repeated = subscript_rt_cb_bind(
                ctx,
                code,
                std::ptr::null(),
                std::ptr::from_mut(&mut second_userdata),
                std::ptr::null_mut(),
            );
            assert_eq!(second, repeated);
            assert_eq!(observed.calls, 1, "same identity re-registration advised");

            subscript_rt_ctx_release(ctx);
        }
    }

    #[test]
    fn ffi_freed_handle_diagnostics_setting_controls_double_free_detection() {
        let releasing = subscript_rt_ctx_new();
        let diagnosing = subscript_rt_ctx_new();
        assert!(!releasing.is_null() && !diagnosing.is_null());
        // SAFETY: both pointers are fresh, live Contexts and are released
        // exactly once below.
        unsafe {
            assert_eq!(
                subscript_rt_ctx_set_freed_handle_diagnostics(releasing, 0, u64::MAX, 0),
                1
            );
            let released = subscript_rt_alloc(releasing, 8, 1, 0);
            subscript_rt_delete(releasing, released, 1);
            subscript_rt_delete(releasing, released, 2);
            assert_eq!(subscript_rt_ctx_trap_kind(releasing), 0);
            assert_eq!(
                subscript_rt_ctx_set_freed_handle_diagnostics(releasing, 1, 0, u64::MAX),
                0,
                "the setting must reject a late change"
            );

            assert_eq!(
                subscript_rt_ctx_set_freed_handle_diagnostics(diagnosing, 1, 0, u64::MAX),
                1
            );
            let retained = subscript_rt_alloc(diagnosing, 8, 1, 0);
            subscript_rt_delete(diagnosing, retained, 3);
            assert_eq!(subscript_rt_ctx_live_bytes(diagnosing), 0);
            assert!(subscript_rt_ctx_reserved_bytes(diagnosing) > 0);
            subscript_rt_delete(diagnosing, retained, 4);
            assert_eq!(
                subscript_rt_ctx_trap_kind(diagnosing),
                TrapKind::DoubleDelete as u32
            );

            subscript_rt_ctx_release(releasing);
            subscript_rt_ctx_release(diagnosing);
        }
    }

    #[test]
    fn string_for_of_code_points_have_the_p24_allocation_bound_on_both_tiers() {
        unsafe extern "C" fn record_allocation(
            userdata: *mut std::ffi::c_void,
            class_id: u32,
            pos_id: u32,
            payload_bytes: u64,
        ) {
            // SAFETY: each call below supplies a live Vec of this type.
            unsafe { &mut *userdata.cast::<Vec<(u32, u32, u64)>>() }.push((
                class_id,
                pos_id,
                payload_bytes,
            ));
        }

        unsafe fn snapshot(ctx: *const Context) -> Vec<(u32, u32, u64)> {
            let mut allocations = Vec::new();
            // SAFETY: `ctx` is live and the callback userdata points to
            // `allocations` for the duration of this call.
            unsafe {
                subscript_rt_ctx_visit_live_allocations(
                    ctx,
                    Some(record_allocation),
                    (&mut allocations as *mut Vec<(u32, u32, u64)>).cast(),
                );
            }
            allocations
        }

        for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
            let p: *mut Context = &mut *ctx;

            let bmp_bytes = "é".repeat(1_000).into_bytes();
            let bmp_source = ctx.alloc_str(&bmp_bytes, 10);
            // SAFETY: `p` and `bmp_source` are live; `next` is writable.
            let before_bmp = unsafe { snapshot(p) };
            let mut index = 0;
            let mut bmp_handle: *mut u8 = std::ptr::null_mut();
            for _ in 0..1_000 {
                let mut next = -1;
                let handle = unsafe {
                    subscript_rt_str_iter_code_point(p, bmp_source, index, &mut next, 4_200)
                };
                assert_eq!(next, index + 2, "{tier}: BMP byte step");
                if bmp_handle.is_null() {
                    bmp_handle = handle;
                } else {
                    assert_eq!(handle, bmp_handle, "{tier}: stable BMP handle");
                }
                index = next;
            }
            assert_eq!(index as usize, bmp_bytes.len(), "{tier}");
            // SAFETY: `bmp_handle` is the returned tagged BMP string.
            assert_eq!(
                unsafe { ctx.str_bytes(bmp_handle) },
                "é".as_bytes(),
                "{tier}"
            );
            assert_eq!(
                unsafe { snapshot(p) },
                before_bmp,
                "{tier}: BMP iteration allocated"
            );

            let astral_bytes = "😀".repeat(1_000).into_bytes();
            let astral_source = ctx.alloc_str(&astral_bytes, 11);
            let before_astral = unsafe { snapshot(p) };
            let mut index = 0;
            let mut astral_handle: *mut u8 = std::ptr::null_mut();
            for _ in 0..1_000 {
                let mut next = -1;
                let handle = unsafe {
                    subscript_rt_str_iter_code_point(p, astral_source, index, &mut next, 4_201)
                };
                assert_eq!(next, index + 4, "{tier}: astral byte step");
                if astral_handle.is_null() {
                    astral_handle = handle;
                    assert_eq!(
                        handle as usize & 15,
                        0,
                        "{tier}: astral handle must be an ordinary allocation"
                    );
                } else {
                    assert_eq!(handle, astral_handle, "{tier}: astral scalar reinterned");
                }
                index = next;
            }
            assert_eq!(index as usize, astral_bytes.len(), "{tier}");
            // SAFETY: `astral_handle` is the live interned string.
            assert_eq!(
                unsafe { ctx.str_bytes(astral_handle) },
                "😀".as_bytes(),
                "{tier}"
            );
            let after_astral = unsafe { snapshot(p) };
            assert_eq!(
                after_astral.len(),
                before_astral.len() + 1,
                "{tier}: repeated astral scalar must allocate once"
            );
            assert_eq!(
                after_astral
                    .iter()
                    .filter(|&&(class_id, pos_id, _)| {
                        class_id == crate::context::CLASS_STRING && pos_id == 4_201
                    })
                    .count(),
                1,
                "{tier}: attribution must show one astral allocation"
            );

            // The ordinary allocated representation must remain
            // indistinguishable everywhere a string handle flows.
            let ordinary = ctx.alloc_str("😀".as_bytes(), 13);
            let suffix = ctx.alloc_str(b"!", 14);
            // SAFETY: all handles and the Context are live.
            unsafe {
                assert_eq!(subscript_rt_str_len(p, astral_handle), 4, "{tier}");
                assert_eq!(subscript_rt_str_eq(p, astral_handle, ordinary), 1, "{tier}");
                let joined = subscript_rt_str_concat(p, astral_handle, suffix, 15);
                assert_eq!(ctx.str_bytes(joined), "😀!".as_bytes(), "{tier}");
                subscript_rt_print(p, astral_handle);
            }
            assert_eq!(ctx.stdout_bytes(), "😀\n".as_bytes(), "{tier}");

            // The intern map, not a program root, keeps this ordinary
            // allocation live across both collection implementations.
            ctx.collect();
            assert!(ctx.is_live(astral_handle as usize), "{tier}");
            assert_eq!(
                ctx.code_point('😀', 9_999),
                astral_handle,
                "{tier}: collect discarded the astral intern entry"
            );

            let distinct = "😀🦀𐍈".repeat(334);
            let distinct_source = ctx.alloc_str(distinct.as_bytes(), 12);
            let before_distinct = unsafe { snapshot(p) };
            let mut index = 0;
            for _ in 0..1_002 {
                let mut next = -1;
                let handle = unsafe {
                    subscript_rt_str_iter_code_point(p, distinct_source, index, &mut next, 4_202)
                };
                assert!(!handle.is_null(), "{tier}");
                index = next;
            }
            assert_eq!(index as usize, distinct.len(), "{tier}");
            let after_distinct = unsafe { snapshot(p) };
            // 😀 was already interned; 🦀 and 𐍈 are the two new scalars.
            assert_eq!(after_distinct.len(), before_distinct.len() + 2, "{tier}");
            assert_eq!(
                after_distinct
                    .iter()
                    .filter(|&&(class_id, pos_id, _)| {
                        class_id == crate::context::CLASS_STRING && pos_id == 4_202
                    })
                    .count(),
                2,
                "{tier}: one allocation per newly distinct scalar"
            );
        }
    }

    #[test]
    fn exported_delete_invalidates_literal_and_astral_string_interns() {
        static LITERAL: &[u8] = b"interned";

        for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
            let p: *mut Context = &mut *ctx;

            // SAFETY: the static literal and exclusive Context satisfy the
            // exported runtime contracts.
            let literal =
                unsafe { subscript_rt_str_lit(p, LITERAL.as_ptr(), LITERAL.len() as u64, 1) };
            unsafe { subscript_rt_delete(p, literal, 2) };
            assert!(!ctx.is_live(literal as usize), "{tier}: literal delete");
            let literal_again =
                unsafe { subscript_rt_str_lit(p, LITERAL.as_ptr(), LITERAL.len() as u64, 3) };
            assert!(
                ctx.is_live(literal_again as usize),
                "{tier}: stale literal intern entry"
            );

            let astral = ctx.code_point('😀', 4);
            // SAFETY: `astral` is a live ordinary string allocation.
            unsafe { subscript_rt_delete(p, astral, 5) };
            assert!(!ctx.is_live(astral as usize), "{tier}: astral delete");
            let astral_again = ctx.code_point('😀', 6);
            assert!(
                ctx.is_live(astral_again as usize),
                "{tier}: stale astral intern entry"
            );
            assert!(!ctx.trapped(), "{tier}");
        }
    }

    #[test]
    fn ffi_trap_observer_is_first_wins_and_null_clears_on_both_tier_policies() {
        for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
            let p: *mut Context = &mut *ctx;
            let mut observed = ObservedTrap::default();
            // SAFETY: live exclusive Context and live callback userdata.
            unsafe {
                subscript_rt_ctx_set_trap_observer(
                    p,
                    Some(observe_trap),
                    (&mut observed as *mut ObservedTrap).cast(),
                );
            }

            // SAFETY: live exclusive Context; this brackets the modeled
            // host call exactly as an embedding host does.
            unsafe { subscript_rt_ctx_enter_script(p) };
            ctx.trap(TrapKind::EmptyPop, "first fault", 7);
            // This is deliberately a direct second call to the central
            // runtime trap path, modeling a runtime leaf reached during
            // generated-code unwind. It cannot be optimized away by an
            // early return in generated code.
            ctx.trap(TrapKind::DivisionByZero, "later unwind fault", 9);
            // SAFETY: pairs with the enter above.
            unsafe { subscript_rt_ctx_exit_script(p) };

            let record = ctx.trap_record().expect("first trap record");
            assert_eq!(observed.calls, 1, "{tier}");
            assert_eq!(observed.kind, record.kind as u32, "{tier}");
            assert_eq!(observed.pos_id, record.pos_id, "{tier}");
            assert_eq!(observed.message, record.message.as_bytes(), "{tier}");
            assert!(ctx.trapped(), "{tier}: the observer must not recover");

            let mut message_len = 0;
            // SAFETY: shared live Context and writable length.
            let message = unsafe { subscript_rt_ctx_trap_message(p, &mut message_len) };
            assert_eq!(observed.message_ptr, message, "{tier}: record lifetime");
            assert_eq!(observed.message_len, message_len, "{tier}");
            assert_eq!(
                observed.kind,
                // SAFETY: shared live Context.
                unsafe { subscript_rt_ctx_trap_kind(p) },
                "{tier}"
            );
            assert_eq!(
                observed.pos_id,
                // SAFETY: shared live Context.
                unsafe { subscript_rt_ctx_trap_pos_id(p) },
                "{tier}"
            );

            // SAFETY: at a host boundary. A null observer clears it.
            unsafe {
                assert_eq!(subscript_rt_ctx_clear_trap(p), 1, "{tier}");
                subscript_rt_ctx_set_trap_observer(p, None, std::ptr::null_mut());
            }
            ctx.trap(TrapKind::Internal, "not observed", 11);
            assert_eq!(observed.calls, 1, "{tier}: null did not clear observer");
        }
    }

    #[test]
    fn ffi_clear_trap_checks_depth_and_preserves_state_on_both_tier_policies() {
        for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
            let kept = ctx.alloc(8, 1, 0);
            ctx.print_line(b"before");
            ctx.bump_reload_epoch();
            let live_before = ctx.live_count();
            let epoch_before = ctx.reload_epoch();
            let stdout_before = ctx.stdout_bytes().to_vec();
            let p: *mut Context = &mut *ctx;
            // SAFETY: shared accessors over a live Context.
            let accounting_before = unsafe {
                (
                    subscript_rt_ctx_live_allocations(p),
                    subscript_rt_ctx_live_bytes(p),
                    subscript_rt_ctx_reserved_bytes(p),
                )
            };
            assert_eq!(accounting_before.0, live_before as u64, "{tier}");

            // SAFETY: live exclusive Context at a host boundary.
            unsafe { subscript_rt_ctx_enter_script(p) };
            assert_eq!(ctx.script_depth(), 1, "{tier}: enter did not raise depth");
            ctx.trap(TrapKind::EmptyPop, "pop() on an empty array", 3);
            let record_before = ctx.trap_record().cloned();
            // SAFETY: live exclusive Context. The function itself must
            // reject the live-script state without changing anything.
            assert_eq!(unsafe { subscript_rt_ctx_clear_trap(p) }, 0, "{tier}");
            assert!(ctx.trapped(), "{tier}");
            assert_eq!(ctx.trap_record(), record_before.as_ref(), "{tier}");
            assert_eq!(ctx.live_count(), live_before, "{tier}");
            assert!(ctx.is_live(kept as usize), "{tier}");
            assert_eq!(ctx.reload_epoch(), epoch_before, "{tier}");
            assert_eq!(ctx.stdout_bytes(), stdout_before, "{tier}");

            // SAFETY: pairs with the host enter above.
            unsafe { subscript_rt_ctx_exit_script(p) };
            assert_eq!(ctx.script_depth(), 0, "{tier}: exit did not lower depth");
            // SAFETY: the live script frame has returned.
            assert_eq!(unsafe { subscript_rt_ctx_clear_trap(p) }, 1, "{tier}");
            assert!(!ctx.trapped(), "{tier}");
            assert!(ctx.trap_record().is_none(), "{tier}");
            assert_eq!(ctx.live_count(), live_before, "{tier}");
            assert!(ctx.is_live(kept as usize), "{tier}");
            assert_eq!(ctx.reload_epoch(), epoch_before, "{tier}");
            assert_eq!(ctx.stdout_bytes(), stdout_before, "{tier}");
            // SAFETY: shared accessors over a live Context.
            let accounting_after = unsafe {
                (
                    subscript_rt_ctx_live_allocations(p),
                    subscript_rt_ctx_live_bytes(p),
                    subscript_rt_ctx_reserved_bytes(p),
                )
            };
            assert_eq!(
                accounting_after, accounting_before,
                "{tier}: clearing a trap rolled memory state back"
            );
        }
    }

    #[test]
    fn ffi_release_of_null_is_a_no_op() {
        // SAFETY: null is explicitly accepted.
        unsafe { subscript_rt_ctx_release(std::ptr::null_mut()) };
    }

    #[test]
    fn ffi_string_round_trip() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        static LIT: &[u8] = b"alpha-beta";
        // SAFETY: valid context; literal data is 'static.
        unsafe {
            let s = subscript_rt_str_lit(p, LIT.as_ptr(), LIT.len() as u64, 0);
            assert_eq!(subscript_rt_str_len(p, s), 10);
            let tail = subscript_rt_str_slice(p, s, 6, 10, 0);
            let lit_beta = subscript_rt_str_lit(p, b"beta".as_ptr(), 4, 0);
            assert_eq!(subscript_rt_str_eq(p, tail, lit_beta), 1);
            assert_eq!(subscript_rt_str_eq(p, s, lit_beta), 0);
            let empty = subscript_rt_str_slice(p, s, -2, 3, 0);
            assert_eq!(ctx.str_bytes(empty), b"");
            let joined = subscript_rt_str_concat(p, s, lit_beta, 0);
            assert_eq!(ctx.str_bytes(joined), b"alpha-betabeta");
        }
    }

    #[test]
    fn ffi_concat_direct_writer_matches_the_vec_reference_path() {
        for (left, right) in [
            (&b"left"[..], &b"right"[..]),
            (&b""[..], &b"right"[..]),
            ("é".as_bytes(), "中".as_bytes()),
        ] {
            let mut expected = left.to_vec();
            expected.extend_from_slice(right);
            let mut ctx = Context::new();
            let p: *mut Context = &mut *ctx;
            let left_handle = ctx.alloc_str(left, 0);
            let right_handle = ctx.alloc_str(right, 0);
            // SAFETY: the Context and both input strings stay live.
            let result = unsafe { subscript_rt_str_concat(p, left_handle, right_handle, 0) };
            // SAFETY: `result` is a live string in this Context.
            unsafe { assert_eq!(ctx.str_bytes(result), expected) };
        }
    }

    #[test]
    fn ffi_slice_off_utf8_boundary_traps() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context and 'static literal.
        unsafe {
            let s = subscript_rt_str_lit(p, "héllo".as_bytes().as_ptr(), 6, 0);
            let out = subscript_rt_str_slice(p, s, 0, 2, 42);
            assert!(out.is_null());
        }
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::StringSlice);
        assert_eq!(r.pos_id, 42);
    }

    #[test]
    fn ffi_str_search_predicates() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        static S: &[u8] = b"hello world";
        // SAFETY: valid context; literal data is 'static.
        unsafe {
            let s = subscript_rt_str_lit(p, S.as_ptr(), S.len() as u64, 0);
            let o = subscript_rt_str_lit(p, b"o".as_ptr(), 1, 0);
            let empty = subscript_rt_str_lit(p, b"".as_ptr(), 0, 0);
            let world = subscript_rt_str_lit(p, b"world".as_ptr(), 5, 0);
            assert_eq!(subscript_rt_str_index_of(p, s, o, 0), 4);
            assert_eq!(subscript_rt_str_index_of(p, s, o, 5), 7);
            assert_eq!(subscript_rt_str_index_of(p, s, o, -3), 4);
            assert_eq!(subscript_rt_str_index_of(p, s, o, 99), -1);
            assert_eq!(subscript_rt_str_index_of(p, s, empty, 99), 11);
            assert_eq!(subscript_rt_str_last_index_of(p, s, o), 7);
            assert_eq!(subscript_rt_str_last_index_of(p, s, empty), 11);
            assert_eq!(subscript_rt_str_includes(p, s, world, 0), 1);
            assert_eq!(subscript_rt_str_includes(p, s, world, 7), 0);
            assert_eq!(subscript_rt_str_includes(p, s, empty, 0), 1);
            assert_eq!(subscript_rt_str_starts_with(p, s, world, 0), 0);
            assert_eq!(subscript_rt_str_starts_with(p, s, world, 6), 1);
            assert_eq!(subscript_rt_str_ends_with(p, s, world, i32::MAX), 1);
            assert_eq!(subscript_rt_str_ends_with(p, s, world, 6), 0);
            assert_eq!(subscript_rt_str_char_code_at(p, s, 0, 0), 104);
        }
        // The predicates never trap.
        assert!(ctx.trap_record().is_none());
    }

    #[test]
    fn ffi_str_char_code_at_out_of_range_traps() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context; literal data is 'static.
        unsafe {
            let s = subscript_rt_str_lit(p, b"abc".as_ptr(), 3, 0);
            assert_eq!(subscript_rt_str_char_code_at(p, s, 3, 17), 0);
        }
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::StrRange);
        assert_eq!(r.pos_id, 17);
        assert!(r.message.contains("charCodeAt(3)"));
    }

    #[test]
    fn ffi_q27_string_ranges_code_points_and_concat() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        static TEXT: &[u8] = "héllo".as_bytes();
        // SAFETY: valid context, static literal bytes, and live handles.
        unsafe {
            let s = subscript_rt_str_lit(p, TEXT.as_ptr(), TEXT.len() as u64, 0);
            let mut roots = [s];
            subscript_rt_shadow_push(p, roots.as_mut_ptr().cast(), roots.len() as u64);
            let reversed = subscript_rt_str_substring(p, s, 4, -2, 0);
            assert_eq!(ctx.str_bytes(reversed), "hél".as_bytes());
            let tail = subscript_rt_str_substr(p, s, -3, i32::MAX, 0);
            assert_eq!(ctx.str_bytes(tail), b"llo");
            let empty = subscript_rt_str_substr(p, s, 3, 0, 0);
            assert_eq!(ctx.str_bytes(empty), b"");
            let multibyte = subscript_rt_str_char_at(p, s, 1, 0);
            assert_eq!(ctx.str_bytes(multibyte), "é".as_bytes());
            let out_of_range = subscript_rt_str_char_at(p, s, 99, 0);
            assert_eq!(ctx.str_bytes(out_of_range), b"");
            assert_eq!(subscript_rt_str_code_point_at(p, s, 1, 0), 'é' as i32);
            let suffix = subscript_rt_str_lit(p, b"!".as_ptr(), 1, 0);
            let joined = subscript_rt_str_method_concat(p, s, suffix, 0);
            assert_eq!(ctx.str_bytes(joined), "héllo!".as_bytes());
            subscript_rt_shadow_pop(p);
        }
        assert!(ctx.trap_record().is_none());
    }

    #[test]
    fn ffi_q27_string_code_point_boundary_and_range_traps() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        static TEXT: &[u8] = "é".as_bytes();
        // SAFETY: valid context, static literal bytes, and a live handle.
        unsafe {
            let s = subscript_rt_str_lit(p, TEXT.as_ptr(), TEXT.len() as u64, 0);
            assert!(subscript_rt_str_char_at(p, s, 1, 31).is_null());
        }
        let report = ctx.trap_record().expect("charAt boundary trap");
        assert_eq!(report.kind, TrapKind::StrRange);
        assert_eq!(report.pos_id, 31);

        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context, static literal bytes, and a live handle.
        unsafe {
            let s = subscript_rt_str_lit(p, TEXT.as_ptr(), TEXT.len() as u64, 0);
            assert_eq!(subscript_rt_str_code_point_at(p, s, 2, 32), 0);
        }
        let report = ctx.trap_record().expect("codePointAt range trap");
        assert_eq!(report.kind, TrapKind::StrRange);
        assert_eq!(report.pos_id, 32);
    }

    #[test]
    fn ffi_str_split_builds_a_string_array_of_handles() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        static S: &[u8] = b",a,";
        // SAFETY: valid context; handles are live; elements are 8-byte
        // string handles read back through array_data.
        unsafe {
            let s = subscript_rt_str_lit(p, S.as_ptr(), S.len() as u64, 0);
            let comma = subscript_rt_str_lit(p, b",".as_ptr(), 1, 0);
            let arr = subscript_rt_str_split(p, s, comma, 0);
            assert!(!arr.is_null());
            assert_eq!(subscript_rt_array_len(p, arr), 3);
            let data = subscript_rt_array_data(p, arr) as *const u64;
            let expected: [&[u8]; 3] = [b"", b"a", b""];
            for (i, want) in expected.iter().enumerate() {
                let h = data.add(i).read() as *const u8;
                assert_eq!(ctx.str_bytes(h), *want, "piece {i}");
            }
        }
        assert!(ctx.trap_record().is_none());
    }

    #[test]
    fn ffi_str_split_empty_separator_traps() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context; literal data is 'static.
        unsafe {
            let s = subscript_rt_str_lit(p, b"ab".as_ptr(), 2, 0);
            let empty = subscript_rt_str_lit(p, b"".as_ptr(), 0, 0);
            assert!(subscript_rt_str_split(p, s, empty, 23).is_null());
        }
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::StrRange);
        assert_eq!(r.pos_id, 23);
    }

    #[test]
    fn ffi_str_trim_family_and_case_allocate_fresh_strings() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        static S: &[u8] = "\u{3000}\u{FEFF}x\u{00A0}".as_bytes();
        // SAFETY: valid context; handles are live.
        unsafe {
            let s = subscript_rt_str_lit(p, S.as_ptr(), S.len() as u64, 0);
            let t = subscript_rt_str_trim(p, s, 0);
            assert_eq!(ctx.str_bytes(t), b"x");
            let ts = subscript_rt_str_trim_start(p, s, 0);
            assert_eq!(ctx.str_bytes(ts), "x\u{00A0}".as_bytes());
            let te = subscript_rt_str_trim_end(p, s, 0);
            assert_eq!(ctx.str_bytes(te), "\u{3000}\u{FEFF}x".as_bytes());
            let mixed_bytes = "ß ﬄ ΣΣς İ ı".as_bytes();
            let mixed = subscript_rt_str_lit(p, mixed_bytes.as_ptr(), mixed_bytes.len() as u64, 0);
            let up = subscript_rt_str_to_upper(p, mixed, 0);
            assert_eq!(ctx.str_bytes(up), "SS FFL ΣΣΣ İ I".as_bytes());
            let low = subscript_rt_str_to_lower(p, up, 0);
            assert_eq!(ctx.str_bytes(low), "ss ffl σσς i\u{0307} i".as_bytes());
            let dotted_i_bytes = "İ".as_bytes();
            let dotted_i =
                subscript_rt_str_lit(p, dotted_i_bytes.as_ptr(), dotted_i_bytes.len() as u64, 0);
            let dotted_i_low = subscript_rt_str_to_lower(p, dotted_i, 0);
            assert_eq!(ctx.str_bytes(dotted_i_low), "i\u{0307}".as_bytes());
            assert_eq!(subscript_rt_str_len(p, dotted_i_low), 3);
            // Fresh allocations, not the receiver handle.
            assert_ne!(te, s as *mut u8);
        }
        assert!(ctx.trap_record().is_none());
    }

    #[test]
    fn ffi_str_repeat_and_negative_count_trap() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context; handles are live.
        unsafe {
            let s = subscript_rt_str_lit(p, b"ab".as_ptr(), 2, 0);
            let three = subscript_rt_str_repeat(p, s, 3, 0);
            assert_eq!(ctx.str_bytes(three), b"ababab");
            let zero = subscript_rt_str_repeat(p, s, 0, 0);
            assert_eq!(ctx.str_bytes(zero), b"");
            assert!(ctx.trap_record().is_none());
            assert!(subscript_rt_str_repeat(p, s, -1, 31).is_null());
        }
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::StrRange);
        assert_eq!(r.pos_id, 31);
    }

    #[test]
    fn ffi_str_pad_truncation_no_op_copy_and_empty_pad_trap() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context; handles are live.
        unsafe {
            let s = subscript_rt_str_lit(p, b"ab".as_ptr(), 2, 0);
            let xy = subscript_rt_str_lit(p, b"xy".as_ptr(), 2, 0);
            // The pinned JS truncation rule: "ab" to 5 with "xy".
            let start = subscript_rt_str_pad_start(p, s, 5, xy, 0);
            assert_eq!(ctx.str_bytes(start), b"xyxab");
            let end = subscript_rt_str_pad_end(p, s, 5, xy, 0);
            assert_eq!(ctx.str_bytes(end), b"abxyx");
            // Already long enough: unchanged bytes, fresh allocation.
            let same = subscript_rt_str_pad_start(p, s, 2, xy, 0);
            assert_eq!(ctx.str_bytes(same), b"ab");
            assert_ne!(same, s as *mut u8);
            // Empty pad with no fill needed is the documented no-op.
            let empty = subscript_rt_str_lit(p, b"".as_ptr(), 0, 0);
            let noop = subscript_rt_str_pad_end(p, s, 2, empty, 0);
            assert_eq!(ctx.str_bytes(noop), b"ab");
            assert!(ctx.trap_record().is_none());
            // Empty pad that must fill traps (Q21).
            assert!(subscript_rt_str_pad_start(p, s, 5, empty, 37).is_null());
        }
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::StrRange);
        assert_eq!(r.pos_id, 37);
        assert!(r.message.contains("padStart"));
    }

    fn assert_direct_pad_matches_vec_reference(at_start: bool) {
        let cases: &[(&[u8], i32, &[u8])] = &[
            (b"ab", 9, b"xyz"),
            (b"", 5, b"ab"),
            (b"receiver", 3, b"xy"),
            (b"z", 6, "é".as_bytes()),
        ];
        for &(receiver, target, pad) in cases {
            let expected = crate::strops::pad(receiver, target, pad, at_start);
            let mut ctx = Context::new();
            let p: *mut Context = &mut *ctx;
            let receiver_handle = ctx.alloc_str(receiver, 0);
            let pad_handle = ctx.alloc_str(pad, 0);
            // SAFETY: the Context and both input strings stay live.
            let result = unsafe {
                if at_start {
                    subscript_rt_str_pad_start(p, receiver_handle, target, pad_handle, 0)
                } else {
                    subscript_rt_str_pad_end(p, receiver_handle, target, pad_handle, 0)
                }
            };
            // SAFETY: `result` is a live string in this Context.
            unsafe { assert_eq!(ctx.str_bytes(result), expected) };
        }
    }

    #[test]
    fn ffi_pad_start_direct_writer_matches_the_vec_reference_path() {
        assert_direct_pad_matches_vec_reference(true);
    }

    #[test]
    fn ffi_pad_end_direct_writer_matches_the_vec_reference_path() {
        assert_direct_pad_matches_vec_reference(false);
    }

    #[test]
    fn ffi_str_replace_first_all_and_empty_pattern_trap() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context; handles are live.
        unsafe {
            let s = subscript_rt_str_lit(p, b"abcabc".as_ptr(), 6, 0);
            let bc = subscript_rt_str_lit(p, b"bc".as_ptr(), 2, 0);
            let x = subscript_rt_str_lit(p, b"X".as_ptr(), 1, 0);
            let first = subscript_rt_str_replace(p, s, bc, x, 0);
            assert_eq!(ctx.str_bytes(first), b"aXabc");
            let all = subscript_rt_str_replace_all(p, s, bc, x, 0);
            assert_eq!(ctx.str_bytes(all), b"aXaX");
            assert!(ctx.trap_record().is_none());
            let empty = subscript_rt_str_lit(p, b"".as_ptr(), 0, 0);
            // replace accepts an empty pattern (match at 0)...
            let prefixed = subscript_rt_str_replace(p, s, empty, x, 0);
            assert_eq!(ctx.str_bytes(prefixed), b"Xabcabc");
            assert!(ctx.trap_record().is_none());
            // ...replaceAll traps on it (Q21).
            assert!(subscript_rt_str_replace_all(p, s, empty, x, 41).is_null());
        }
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::StrRange);
        assert_eq!(r.pos_id, 41);
    }

    #[test]
    fn ffi_print_and_fmt() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe {
            let s = subscript_rt_fmt_f64(p, 3.75, 0);
            subscript_rt_print(p, s);
            let t = subscript_rt_fmt_bool(p, 1, 0);
            subscript_rt_print(p, t);
        }
        assert_eq!(ctx.take_stdout(), b"3.75\ntrue\n");
    }

    #[test]
    fn ffi_fmt_i32_direct_writer_matches_the_string_reference_path() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        for value in [i32::MIN, -1, 0, i32::MAX] {
            let expected = value.to_string();
            // SAFETY: the Context stays live.
            let result = unsafe { subscript_rt_fmt_i32(p, value, 0) };
            // SAFETY: `result` is a live string in this Context.
            unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
        }
    }

    #[test]
    fn ffi_fmt_u32_direct_writer_matches_the_string_reference_path() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        for value in [0, 1, u32::MAX] {
            let expected = value.to_string();
            // SAFETY: the Context stays live.
            let result = unsafe { subscript_rt_fmt_u32(p, value, 0) };
            // SAFETY: `result` is a live string in this Context.
            unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
        }
    }

    #[test]
    fn ffi_fmt_i64_direct_writer_matches_the_string_reference_path() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        for value in [i64::MIN, -1, 0, i64::MAX] {
            let expected = value.to_string();
            // SAFETY: the Context stays live.
            let result = unsafe { subscript_rt_fmt_i64(p, value, 0) };
            // SAFETY: `result` is a live string in this Context.
            unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
        }
    }

    #[test]
    fn ffi_fmt_u64_direct_writer_matches_the_string_reference_path() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        for value in [0, 1, u64::MAX] {
            let expected = value.to_string();
            // SAFETY: the Context stays live.
            let result = unsafe { subscript_rt_fmt_u64(p, value, 0) };
            // SAFETY: `result` is a live string in this Context.
            unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
        }
    }

    #[test]
    fn ffi_fmt_f32_direct_writer_matches_the_string_reference_path() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        for value in [-0.0, 0.1, f32::INFINITY] {
            let expected = crate::fmt::fmt_f32(value);
            // SAFETY: the Context stays live.
            let result = unsafe { subscript_rt_fmt_f32(p, value, 0) };
            // SAFETY: `result` is a live string in this Context.
            unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
        }
    }

    #[test]
    fn ffi_fmt_f64_direct_writer_matches_the_string_reference_path() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        for value in [-0.0, 0.1, f64::INFINITY] {
            let expected = crate::fmt::fmt_f64(value);
            // SAFETY: the Context stays live.
            let result = unsafe { subscript_rt_fmt_f64(p, value, 0) };
            // SAFETY: `result` is a live string in this Context.
            unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
        }
    }

    #[test]
    fn ffi_binary32_bit_access_forwards_both_wrappers() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        assert_eq!(subscript_rt_math_f32_to_bits(p, -0.0), 0x8000_0000);
        assert_eq!(subscript_rt_math_f32_from_bits(p, 1), 2.0_f64.powi(-149));
    }

    #[test]
    fn ffi_number_entries_forward_and_trap_ranges() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context; all string handles remain live.
        unsafe {
            assert_eq!(subscript_rt_num_is_nan(p, f64::NAN), 1);
            assert_eq!(subscript_rt_num_is_finite(p, f64::INFINITY), 0);
            assert_eq!(subscript_rt_num_is_integer(p, 7.0), 1);
            assert_eq!(
                subscript_rt_num_is_safe_integer(p, 9_007_199_254_740_992.0),
                0
            );
            let int_s = subscript_rt_str_lit(p, b"fftail".as_ptr(), 6, 0);
            assert_eq!(subscript_rt_num_parse_int(p, int_s, 16, 19), 255.0);
            let float_s = subscript_rt_str_lit(p, b"1.5tail".as_ptr(), 7, 0);
            assert_eq!(subscript_rt_num_parse_float(p, float_s, 20), 1.5);
            let fixed = subscript_rt_num_to_fixed(p, 1.005, 2, 20);
            assert_eq!(ctx.str_bytes(fixed), b"1.00");
            let radix_f32 = subscript_rt_num_to_string_f32(p, 10.5, 2, 20);
            assert_eq!(ctx.str_bytes(radix_f32), b"1010.1");
            let radix = subscript_rt_num_to_string_f64(p, 1234.5678, 36, 20);
            assert_eq!(ctx.str_bytes(radix), b"ya.kfv9yqdpm");
            let exponential = subscript_rt_num_to_exponential(p, 0.0, 2, 20);
            assert_eq!(ctx.str_bytes(exponential), b"0.00e+0");
            let precision = subscript_rt_num_to_precision(p, 123.456, 2, 20);
            assert_eq!(ctx.str_bytes(precision), b"1.2e+2");
            assert_eq!(subscript_rt_math_clz32(p, 0), 32);
            assert_eq!(subscript_rt_math_imul(p, i32::MAX, 2), -2);
            assert_eq!(subscript_rt_math_fround(p, 1.1), 1.100_000_023_841_858);
            assert!(subscript_rt_num_to_fixed(p, 1.0, 101, 21).is_null());
        }
        let report = ctx.trap_record().expect("toFixed range trap");
        assert_eq!(report.kind, TrapKind::NumberRange);
        assert_eq!(report.pos_id, 21);

        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context and live literal string handle.
        unsafe {
            let s = subscript_rt_str_lit(p, b"10".as_ptr(), 2, 0);
            assert!(subscript_rt_num_parse_int(p, s, 1, 22).is_nan());
        }
        let report = ctx.trap_record().expect("parseInt radix trap");
        assert_eq!(report.kind, TrapKind::NumberRange);
        assert_eq!(report.pos_id, 22);

        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let s = ctx.alloc_str(&[0xff], 0);
        // SAFETY: valid context and live string handle. The invalid byte
        // exercises the internal trap for a string the compiler never produces.
        unsafe {
            assert!(subscript_rt_num_parse_float(p, s, 23).is_nan());
        }
        let report = ctx.trap_record().expect("parseFloat UTF-8 trap");
        assert_eq!(report.kind, TrapKind::Internal);
        assert_eq!(report.pos_id, 23);
    }

    #[test]
    fn ffi_array_and_trap_reporting() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context; element pointers are valid.
        unsafe {
            let a = subscript_rt_array_new(p, 4, 0);
            let v: i32 = 11;
            assert_eq!(
                subscript_rt_array_push(p, a, &v as *const i32 as *const u8, 0),
                1
            );
            assert_eq!(subscript_rt_array_len(p, a), 1);
            assert!(subscript_rt_array_ptr(p, a, 3, 9).is_null());
        }
        assert_eq!(
            ctx.trap_record().map(|r| (r.kind, r.pos_id)),
            Some((TrapKind::IndexOutOfBounds, 9))
        );
    }

    #[test]
    fn ffi_array_with_capacity_reserves_empty_storage() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: the context and all element pointers are valid.
        unsafe {
            let array = subscript_rt_array_with_capacity(p, 3, 4, 0);
            assert!(!array.is_null());
            assert_eq!(subscript_rt_array_len(p, array), 0);
            let values = [10i32, 20, 30];
            assert_eq!(
                subscript_rt_array_push(p, array, (&raw const values[0]).cast(), 0),
                1
            );
            let data = subscript_rt_array_ptr(p, array, 0, 0);
            for (index, value) in values.iter().enumerate().skip(1) {
                assert_eq!(
                    subscript_rt_array_push(p, array, (&raw const *value).cast(), 0),
                    index as i32 + 1
                );
            }
            assert_eq!(subscript_rt_array_ptr(p, array, 0, 0), data);
        }
    }

    #[test]
    fn ffi_byte_array_span_and_range_report_exact_storage() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let source = [1u8, 2, 3, 4];
        // SAFETY: the context and source span are valid.
        let array =
            unsafe { subscript_rt_array_from_bytes(p, source.as_ptr(), source.len() as u32, 5) };
        assert!(!array.is_null());
        // SAFETY: the array is live and both requested ranges use its byte storage.
        unsafe {
            assert_eq!(subscript_rt_array_len(p, array), 4);
            let range = subscript_rt_array_byte_range(p, array, 1, 2, 6);
            assert_eq!(std::slice::from_raw_parts(range, 2), &[2, 3]);
            assert!(subscript_rt_array_byte_range(p, array, 3, 2, 7).is_null());
        }
        let report = ctx.trap_record().expect("byte range trap");
        assert_eq!(report.kind, TrapKind::IndexOutOfBounds);
        assert_eq!(
            report.message,
            "byte range at offset 3 with size 2 exceeds array length 4"
        );
        assert_eq!(report.pos_id, 7);

        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: the null inputs exercise the defensive FFI checks.
        unsafe {
            assert!(subscript_rt_array_from_bytes(p, std::ptr::null(), 1, 8).is_null());
            assert!(subscript_rt_array_byte_range(p, std::ptr::null_mut(), 0, 1, 9).is_null());
        }
        assert!(ctx.trap_record().is_none());

        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
        let p: *mut Context = &mut *ctx;
        // SAFETY: the context and source span are valid.
        let array = unsafe { subscript_rt_array_from_bytes(p, source.as_ptr(), 4, 10) };
        ctx.collect();
        // SAFETY: this call exercises the mode-enabled stale-handle diagnostic.
        assert!(unsafe { subscript_rt_array_byte_range(p, array, 0, 1, 11) }.is_null());
        let report = ctx.trap_record().expect("byte range liveness trap");
        assert_eq!(report.kind, TrapKind::UseAfterDelete);
        assert_eq!(report.pos_id, 11);
    }

    #[test]
    fn ffi_array_push_reports_a_collected_receiver_without_panicking() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid exclusive Context.
        let array = unsafe { subscript_rt_array_new(p, 4, 1) };
        ctx.collect();
        let value = 11i32;
        // SAFETY: this deliberately exercises the mode-enabled stale-handle
        // diagnostic provided by retain-and-poison.
        let result =
            unsafe { subscript_rt_array_push(p, array, (&value as *const i32).cast(), 77) };
        assert_eq!(result, -1);
        assert_eq!(
            ctx.trap_record().map(|record| (record.kind, record.pos_id)),
            Some((TrapKind::UseAfterDelete, 77))
        );
    }

    #[test]
    fn ffi_emitted_trap_entry_records_kind_and_pos() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe { subscript_rt_trap(p, TrapKind::UseAfterDelete as u32, 12) };
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::UseAfterDelete);
        assert_eq!(r.message, "use of a deleted allocation");
        assert_eq!(r.pos_id, 12);
    }

    #[test]
    fn trap_messages_runtime_and_emitted_checks_agree() {
        let mut runtime_ctx = Context::new();
        let array = runtime_ctx.array_new(4, 0);
        // SAFETY: `array` is a live array handle of `runtime_ctx`.
        unsafe { runtime_ctx.array_elem_ptr(array, -1, 1) };
        let runtime_bounds = runtime_ctx
            .trap_record()
            .expect("runtime bounds trap")
            .message
            .clone();

        let mut emitted_ctx = Context::new();
        let emitted_ptr: *mut Context = &mut *emitted_ctx;
        // SAFETY: valid context and materialized bounds.
        unsafe { subscript_rt_trap_index_out_of_bounds(emitted_ptr, -1, 0, 1) };
        assert_eq!(
            emitted_ctx
                .trap_record()
                .expect("emitted bounds trap")
                .message,
            runtime_bounds
        );

        let mut runtime_ctx = Context::new();
        assert!(runtime_ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
        let array = runtime_ctx.array_new(4, 0);
        runtime_ctx.delete(array as usize, 0);
        assert!(!runtime_ctx.require_live_handle(array as usize, 2));
        let runtime_deleted = runtime_ctx
            .trap_record()
            .expect("runtime deleted-allocation trap")
            .message
            .clone();

        let mut emitted_ctx = Context::new();
        let emitted_ptr: *mut Context = &mut *emitted_ctx;
        // SAFETY: valid context and stable trap kind.
        unsafe { subscript_rt_trap(emitted_ptr, TrapKind::UseAfterDelete as u32, 2) };
        assert_eq!(
            emitted_ctx
                .trap_record()
                .expect("emitted deleted-allocation trap")
                .message,
            runtime_deleted
        );
    }

    #[test]
    fn ffi_map_set_operations_trap_on_deleted_receivers_with_diagnostics() {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context and monomorphized i32 shapes.
        unsafe {
            let map = subscript_rt_map_new(p, 4, 4, 0, 1);
            subscript_rt_delete(p, map, 2);
            assert_eq!(subscript_rt_map_size(p, map), 0);
        }
        assert_eq!(
            ctx.trap_record().map(|r| (r.kind, r.pos_id)),
            Some((TrapKind::UseAfterDelete, 0))
        );

        ctx.clear_trap();
        // SAFETY: valid context and monomorphized i32 shape. The stale
        // receiver is validated before the key is inspected.
        unsafe {
            let set = subscript_rt_set_new(p, 4, 0, 3);
            subscript_rt_delete(p, set, 4);
            let key = 9i32;
            assert_eq!(
                subscript_rt_set_add(p, set, (&key as *const i32).cast(), 17),
                set
            );
        }
        assert_eq!(
            ctx.trap_record().map(|r| (r.kind, r.pos_id)),
            Some((TrapKind::UseAfterDelete, 17))
        );
    }

    #[test]
    fn ffi_unknown_trap_kind_is_reported_as_internal() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe { subscript_rt_trap(p, 999, 3) };
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::Internal);
        assert_eq!(r.pos_id, 3);
    }

    #[test]
    fn ffi_math_entries_forward_to_the_math_module() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // One spot value per exported symbol; the semantics themselves
        // are pinned by `crate::math`'s tests.
        assert_eq!(subscript_rt_math_abs(p, -3.5), 3.5);
        assert_eq!(subscript_rt_math_acos(p, 1.0), 0.0);
        assert_eq!(subscript_rt_math_acosh(p, 1.0), 0.0);
        assert_eq!(subscript_rt_math_asin(p, 0.0), 0.0);
        assert_eq!(subscript_rt_math_asinh(p, 0.0), 0.0);
        assert_eq!(subscript_rt_math_atan(p, 0.0), 0.0);
        assert_eq!(subscript_rt_math_atanh(p, 0.0), 0.0);
        assert_eq!(subscript_rt_math_cbrt(p, 27.0), 3.0);
        assert_eq!(subscript_rt_math_ceil(p, 1.2), 2.0);
        assert_eq!(subscript_rt_math_cos(p, 0.0), 1.0);
        assert_eq!(subscript_rt_math_cosh(p, 0.0), 1.0);
        assert_eq!(subscript_rt_math_exp(p, 0.0), 1.0);
        assert_eq!(subscript_rt_math_expm1(p, 0.0), 0.0);
        assert_eq!(subscript_rt_math_floor(p, 1.8), 1.0);
        assert_eq!(subscript_rt_math_log(p, 1.0), 0.0);
        assert_eq!(subscript_rt_math_log1p(p, 0.0), 0.0);
        assert_eq!(subscript_rt_math_log10(p, 1000.0), 3.0);
        assert_eq!(subscript_rt_math_log2(p, 8.0), 3.0);
        assert_eq!(subscript_rt_math_round(p, -2.5), -2.0);
        assert_eq!(subscript_rt_math_sign(p, -7.5), -1.0);
        assert_eq!(subscript_rt_math_sin(p, 0.0), 0.0);
        assert_eq!(subscript_rt_math_sinh(p, 0.0), 0.0);
        assert_eq!(subscript_rt_math_sqrt(p, 9.0), 3.0);
        assert_eq!(subscript_rt_math_tan(p, 0.0), 0.0);
        assert_eq!(subscript_rt_math_tanh(p, 0.0), 0.0);
        assert_eq!(subscript_rt_math_trunc(p, -1.7), -1.0);
        assert_eq!(subscript_rt_math_atan2(p, 0.0, 1.0), 0.0);
        assert_eq!(subscript_rt_math_hypot(p, 3.0, 4.0), 5.0);
        assert_eq!(subscript_rt_math_pow(p, 2.0, 10.0), 1024.0);
        assert_eq!(subscript_rt_math_max(p, 2.5, 7.0), 7.0);
        assert_eq!(subscript_rt_math_min(p, 2.5, 7.0), 2.5);
    }

    #[test]
    fn ffi_date_entries_forward_to_the_date_module() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe {
            let ms = subscript_rt_date_utc(p, 2020, 5, 15, 12, 34, 56, 789, 0);
            assert_eq!(ms, 1_592_224_496_789);
            assert_eq!(subscript_rt_date_new(p, ms, 0), ms);
            assert_eq!(
                subscript_rt_date_get(p, ms, crate::date::FIELD_FULL_YEAR),
                2020
            );
            assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_MONTH), 5);
            assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_DATE), 15);
            assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_DAY), 1);
            assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_HOURS), 12);
            assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_MINUTES), 34);
            assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_SECONDS), 56);
            assert_eq!(
                subscript_rt_date_get(p, ms, crate::date::FIELD_MILLISECONDS),
                789
            );
            let iso = subscript_rt_date_to_iso(p, ms, 0);
            assert_eq!(ctx.str_bytes(iso), b"2020-06-15T12:34:56.789Z");
        }
        assert!(ctx.trap_record().is_none());
    }

    #[test]
    fn ffi_date_new_out_of_range_traps_with_position() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe {
            assert_eq!(subscript_rt_date_new(p, 8_640_000_000_000_001, 7), 0);
        }
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::DateRange);
        assert_eq!(r.pos_id, 7);
    }

    #[test]
    fn ffi_date_utc_out_of_range_traps() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe {
            assert_eq!(subscript_rt_date_utc(p, 275_760, 8, 14, 0, 0, 0, 0, 9), 0);
        }
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::DateRange);
        assert_eq!(r.pos_id, 9);
    }

    #[test]
    fn ffi_date_to_iso_out_of_year_range_traps() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe {
            assert!(subscript_rt_date_to_iso(p, 253_402_300_800_000, 11).is_null());
        }
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::DateRange);
        assert_eq!(r.pos_id, 11);
        assert!(r.message.contains("0000-9999"), "message: {}", r.message);
    }

    #[test]
    fn ffi_date_get_unknown_field_is_an_internal_trap_not_a_panic() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe {
            assert_eq!(subscript_rt_date_get(p, 0, 99), 0);
        }
        assert_eq!(ctx.trap_record().map(|r| r.kind), Some(TrapKind::Internal));
    }

    #[test]
    fn ffi_date_now_reads_the_pinned_context_clock() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe {
            subscript_rt_ctx_set_now(p, 1_592_224_496_789);
            assert_eq!(subscript_rt_date_now(p), 1_592_224_496_789);
            subscript_rt_ctx_set_now(p, -1);
            assert_eq!(subscript_rt_date_now(p), -1);
        }
    }

    #[test]
    fn ffi_regex_budget_setter_updates_context_state() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe {
            subscript_rt_ctx_set_regex_budget(p, 7);
        }
        assert_eq!(ctx.regex_budget(), 7);
    }

    #[test]
    fn ffi_random_draws_the_context_stream_and_reseeds() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let mut reference = crate::math::Rng::new(crate::math::DEFAULT_RANDOM_SEED);
        // SAFETY: valid context.
        unsafe {
            for _ in 0..4 {
                assert_eq!(
                    subscript_rt_math_random(p).to_bits(),
                    reference.next_f64().to_bits()
                );
            }
            subscript_rt_ctx_seed_random(p, 99);
            let a = subscript_rt_math_random(p);
            subscript_rt_ctx_seed_random(p, 99);
            let b = subscript_rt_math_random(p);
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }

    unsafe extern "C" fn triple_i32(_ctx: *mut Context, _env: *const u8, v: i32) -> i32 {
        v * 3
    }

    unsafe extern "C" fn cmp_desc_i32(_ctx: *mut Context, _env: *const u8, a: i32, b: i32) -> i32 {
        b - a
    }

    #[test]
    fn ffi_arr_entries_forward_to_the_arrops_module() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let a = ctx.array_new(4, 0);
        // SAFETY: valid context; live 4-byte-element array; readable
        // needles; callbacks match the dispatched ABI.
        unsafe {
            for v in [3i32, 1, 2, 1] {
                subscript_rt_array_push(p, a, (&v as *const i32).cast(), 0);
            }
            let one = 1i32;
            assert_eq!(
                subscript_rt_arr_index_of(p, a, (&one as *const i32).cast(), 0),
                1
            );
            assert_eq!(
                subscript_rt_arr_last_index_of(p, a, (&one as *const i32).cast(), 0),
                3
            );
            assert_eq!(
                subscript_rt_arr_includes(p, a, (&one as *const i32).cast(), 0),
                1
            );
            let sep = ctx.alloc_str(b"-", 0);
            let joined = subscript_rt_arr_join(p, a, sep, 0, 0);
            assert_eq!(ctx.str_bytes(joined), b"3-1-2-1");
            let sl = subscript_rt_arr_slice(p, a, 1, 3, 0);
            assert_eq!(subscript_rt_array_len(p, sl), 2);
            let mapped = subscript_rt_arr_map(
                p,
                a,
                triple_i32 as *const u8,
                std::ptr::null(),
                0,
                0,
                4,
                0,
                0,
            );
            assert_eq!(ctx.array_data(mapped).cast::<i32>().read_unaligned(), 9);
            subscript_rt_arr_sort(p, a, cmp_desc_i32 as *const u8, std::ptr::null(), 0);
            assert_eq!(ctx.array_data(a).cast::<i32>().read_unaligned(), 3);
            let b = subscript_rt_arr_slice(p, a, 0, 1, 0);
            let cat = subscript_rt_arr_concat(p, a, b, 0);
            assert_eq!(subscript_rt_array_len(p, cat), 5);
            let z = 0i32;
            subscript_rt_arr_fill(p, a, (&z as *const i32).cast(), 0, i32::MAX);
            subscript_rt_arr_reverse(p, a);
            assert_eq!(
                subscript_rt_arr_every(p, a, triple_i32 as *const u8, std::ptr::null(), 0, 0,),
                0
            );
        }
        assert!(ctx.trap_record().is_none());
    }

    #[test]
    fn ffi_arr_unknown_kind_tag_traps_internal() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let a = ctx.array_new(4, 0);
        let x = 1i32;
        // SAFETY: valid context; live array; readable needle.
        unsafe {
            assert_eq!(
                subscript_rt_arr_index_of(p, a, (&x as *const i32).cast(), 99),
                -1
            );
        }
        assert_eq!(ctx.trap_record().map(|r| r.kind), Some(TrapKind::Internal));
    }

    #[test]
    fn ffi_root_ranges_cover_aggregate_globals() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let a = ctx.alloc(8, 1, 0);
        let b = ctx.alloc(8, 1, 0);
        let range = [a as usize, b as usize];
        // SAFETY: valid context; the range outlives the collect call.
        unsafe {
            subscript_rt_root_add(p, range.as_ptr() as *mut u8, 2);
            subscript_rt_collect(p);
        }
        assert!(ctx.is_live(a as usize));
        assert!(ctx.is_live(b as usize));
    }
}
