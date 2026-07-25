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

use crate::context::{CallbackBinding, Context};
use crate::trap::TrapKind;

/// Narrows an `f64` to raw IEEE 754 binary16 storage bits using
/// round-to-nearest-even. Overflow becomes infinity; subnormals, signed
/// zero, and NaN are preserved (Q23).
#[no_mangle]
pub extern "C" fn sub_rt_f16_from_f64(value: f64) -> u16 {
    crate::half::from_f64(value)
}

/// Widens raw IEEE 754 binary16 storage bits to an exactly represented
/// `f64`, preserving signed zero, infinity, and NaN (Q23).
#[no_mangle]
pub extern "C" fn sub_rt_f16_to_f64(bits: u16) -> f64 {
    crate::half::to_f64(bits)
}

/// A `(ptr, len)` string view, ABI-identical to the synthetic header's
/// `SubStringView` (`{ const char*; size_t; }`) and to the language's
/// own string representation (Q5). It is the by-value first argument the
/// C callback ABI hands [`sub_rt_cb_trampoline`].
#[repr(C)]
pub struct SubStrView {
    /// UTF-8 bytes; no NUL terminator assumed.
    pub data: *const u8,
    /// Byte length.
    pub len: usize,
}

/// `print(message)`: appends the string's bytes and a newline to the
/// Context stdout sink.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_print(ctx: *mut Context, s: *const u8) {
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

/// `collect()`: explicitly invoked collection (Q7).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_collect(ctx: *mut Context) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.collect();
}

/// Allocates `size` zeroed payload bytes tagged `class_id`; null on
/// trap.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_alloc(
    ctx: *mut Context,
    size: u64,
    class_id: u32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc(size as usize, class_id, pos_id)
}

/// `unsafeDelete(value)`: frees immediately; double delete traps (Q6).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_delete(ctx: *mut Context, payload: *mut u8, pos_id: u32) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.delete(payload as usize, pos_id);
}

/// Records a trap raised by an emitted check in generated code.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_trap(ctx: *mut Context, kind: u32, pos_id: u32) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // An unknown kind means the code generator and runtime disagree;
    // report it as an internal fault instead of misattributing it.
    let kind = TrapKind::from_u32(kind).unwrap_or(TrapKind::Internal);
    let message = match kind {
        TrapKind::IndexOutOfBounds => "index out of bounds",
        TrapKind::NullNarrowing => "`as` narrowing applied to null",
        TrapKind::ClassMismatch => "`as` narrowing to a class the instance does not have",
        TrapKind::UseAfterDelete => "use of a deleted allocation",
        TrapKind::DivisionByZero => "integer division by zero",
        TrapKind::Internal => "unknown trap kind raised by generated code",
        TrapKind::StaleCoroutine => "stale coroutine after reload",
        other => other.rule(),
    };
    ctx.trap(kind, message, pos_id);
}

/// Registers a permanent root range: `words` consecutive 8-byte slots
/// at `base` (module globals of managed type, or global aggregates
/// with managed interior).
///
/// # Safety
///
/// Shared contract; the range outlives the script run.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_root_add(ctx: *mut Context, base: *mut u8, words: u64) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.root_add(base as usize, words as usize);
}

/// Pushes a shadow frame of `slots` managed-local slots at `base`.
///
/// # Safety
///
/// Shared contract; the range stays valid until the matching pop.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_shadow_push(ctx: *mut Context, base: *mut u8, slots: u64) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.shadow_push(base as usize, slots as usize);
}

/// Pops the most recent shadow frame.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_shadow_pop(ctx: *mut Context) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.shadow_pop();
}

// ----- Map / Set (stdlib.md §10, Q24) -----

fn assoc_receiver_is_live(ctx: &mut Context, handle: *const u8, pos_id: u32) -> bool {
    ctx.require_live_handle(handle as usize, pos_id)
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
pub unsafe extern "C" fn sub_rt_map_new(
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
/// As [`sub_rt_map_new`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_set_new(
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
pub unsafe extern "C" fn sub_rt_map_size(ctx: *mut Context, map: *const u8) -> i32 {
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
pub unsafe extern "C" fn sub_rt_set_size(ctx: *mut Context, set: *const u8) -> i32 {
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
pub unsafe extern "C" fn sub_rt_map_set(
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
pub unsafe extern "C" fn sub_rt_set_add(
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
pub unsafe extern "C" fn sub_rt_map_get(
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
/// As [`sub_rt_map_get`], and `fallback` is readable for the value width.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_map_get_or(
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
pub unsafe extern "C" fn sub_rt_map_has(
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
/// As [`sub_rt_map_has`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_set_has(
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
/// As [`sub_rt_map_has`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_map_delete(
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
/// As [`sub_rt_map_has`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_set_delete(
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
pub unsafe extern "C" fn sub_rt_map_clear(ctx: *mut Context, map: *mut u8) {
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
pub unsafe extern "C" fn sub_rt_set_clear(ctx: *mut Context, set: *mut u8) {
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
pub unsafe extern "C" fn sub_rt_map_for_each(
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
/// As [`sub_rt_map_for_each`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_set_for_each(
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
pub unsafe extern "C" fn sub_rt_map_group_by(
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

unsafe fn set_pair_is_live(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
    pos_id: u32,
) -> bool {
    let runtime = unsafe { &mut *ctx };
    assoc_receiver_is_live(runtime, left, pos_id)
        && assoc_receiver_is_live(runtime, right, pos_id)
}

/// `Set.union`: returns a fresh result in ES2024 order.
///
/// # Safety
///
/// Shared contract; both operands are live `Set<K>` handles.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_set_union(
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
/// As [`sub_rt_set_union`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_set_intersection(
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
/// As [`sub_rt_set_union`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_set_difference(
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
/// As [`sub_rt_set_union`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_set_symmetric_difference(
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
pub unsafe extern "C" fn sub_rt_set_is_subset_of(
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
/// As [`sub_rt_set_is_subset_of`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_set_is_superset_of(
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
/// As [`sub_rt_set_is_subset_of`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_set_is_disjoint_from(
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
pub unsafe extern "C" fn sub_rt_str_lit(
    ctx: *mut Context,
    ptr: *const u8,
    len: u64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract; literal data outlives the context.
    unsafe { (*ctx).intern_literal(ptr, len as usize, pos_id) }
}

/// String byte length (Q5: `length` is the byte length).
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_str_len(ctx: *mut Context, s: *const u8) -> i32 {
    if s.is_null() {
        return 0;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &*ctx };
    // SAFETY: `s` is a live string handle.
    unsafe { ctx.str_bytes(s).len() as i32 }
}

/// String concatenation (`+` / template literals).
///
/// # Safety
///
/// Shared contract; `a` and `b` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_str_concat(
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
    // SAFETY: live string handles.
    let mut bytes = unsafe { ctx.str_bytes(a) }.to_vec();
    // SAFETY: live string handles.
    bytes.extend_from_slice(unsafe { ctx.str_bytes(b) });
    ctx.alloc_str(&bytes, pos_id)
}

/// `slice(start, end)` with byte offsets and ECMA's negative/clamping
/// rules; a reversed normalized pair produces `""`. Off a UTF-8
/// boundary traps (Q5).
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_str_slice(
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
pub unsafe extern "C" fn sub_rt_str_eq(ctx: *mut Context, a: *const u8, b: *const u8) -> i32 {
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
pub unsafe extern "C" fn sub_rt_str_index_of(
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
pub unsafe extern "C" fn sub_rt_str_last_index_of(
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
pub unsafe extern "C" fn sub_rt_str_includes(
    ctx: *mut Context,
    s: *const u8,
    needle: *const u8,
    from: i32,
) -> i32 {
    // SAFETY: shared contract (forwarded).
    i32::from(unsafe { sub_rt_str_index_of(ctx, s, needle, from) } >= 0)
}

/// `startsWith(needle, position)`: 1 when `needle` begins at the
/// clamped byte position.
///
/// # Safety
///
/// Shared contract; `s` and `needle` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_str_starts_with(
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
pub unsafe extern "C" fn sub_rt_str_ends_with(
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
pub unsafe extern "C" fn sub_rt_str_char_code_at(
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
pub unsafe extern "C" fn sub_rt_str_substring(
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
pub unsafe extern "C" fn sub_rt_str_substr(
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
pub unsafe extern "C" fn sub_rt_str_char_at(
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
pub unsafe extern "C" fn sub_rt_str_code_point_at(
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
pub unsafe extern "C" fn sub_rt_str_method_concat(
    ctx: *mut Context,
    a: *const u8,
    b: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: forwarded shared contract.
    unsafe { sub_rt_str_concat(ctx, a, b, pos_id) }
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
pub unsafe extern "C" fn sub_rt_str_split(
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
pub unsafe extern "C" fn sub_rt_str_trim(ctx: *mut Context, s: *const u8, pos_id: u32) -> *mut u8 {
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
pub unsafe extern "C" fn sub_rt_str_trim_start(
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
pub unsafe extern "C" fn sub_rt_str_trim_end(
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
pub unsafe extern "C" fn sub_rt_str_repeat(
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
    // SAFETY: live string handles. Copied out so the borrows do not
    // overlap the mutable trap/alloc calls below.
    let bytes: Vec<u8> = unsafe { ctx.str_bytes(s) }.to_vec();
    // SAFETY: live string handles.
    let pad_bytes: Vec<u8> = unsafe { ctx.str_bytes(pad) }.to_vec();
    if pad_bytes.is_empty() && (target.max(0) as usize) > bytes.len() {
        ctx.trap(
            TrapKind::StrRange,
            format!(
                "{name}({target}): an empty pad cannot reach the target length \
                 (string length {})",
                bytes.len()
            ),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    ctx.alloc_str(&crate::strops::pad(&bytes, target, &pad_bytes, at_start), pos_id)
}

/// `padStart(len, pad)` — see [`str_pad`]. The checker supplies the
/// defaulted `pad` (`" "`).
///
/// # Safety
///
/// Shared contract; `s` and `pad` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_str_pad_start(
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
pub unsafe extern "C" fn sub_rt_str_pad_end(
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
pub unsafe extern "C" fn sub_rt_str_to_upper(
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
pub unsafe extern "C" fn sub_rt_str_to_lower(
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
pub unsafe extern "C" fn sub_rt_str_replace(
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
pub unsafe extern "C" fn sub_rt_str_replace_all(
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

// ----- Q14 formatting -----

/// Formats an `i32` (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fmt_i32(ctx: *mut Context, v: i32, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc_str(crate::fmt::fmt_i32(v).as_bytes(), pos_id)
}

/// Formats a `u32` (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fmt_u32(ctx: *mut Context, v: u32, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc_str(crate::fmt::fmt_u32(v).as_bytes(), pos_id)
}

/// Formats an `i64` (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fmt_i64(ctx: *mut Context, v: i64, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc_str(crate::fmt::fmt_i64(v).as_bytes(), pos_id)
}

/// Formats a `u64` (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fmt_u64(ctx: *mut Context, v: u64, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc_str(crate::fmt::fmt_u64(v).as_bytes(), pos_id)
}

/// Formats an `f32` at f32 precision (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fmt_f32(ctx: *mut Context, v: f32, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc_str(crate::fmt::fmt_f32(v).as_bytes(), pos_id)
}

/// Formats an `f64` (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fmt_f64(ctx: *mut Context, v: f64, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc_str(crate::fmt::fmt_f64(v).as_bytes(), pos_id)
}

/// Formats a boolean.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fmt_bool(ctx: *mut Context, v: u32, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc_str(crate::fmt::fmt_bool(v != 0).as_bytes(), pos_id)
}

// ----- Number and parsing intrinsics (stdlib.md §11, Q25/Q26) -----
//
// All operations stay behind opaque symbols so both tiers execute the
// same Rust implementation. The predicates are pure; parsing and
// formatting entries carry a position for allocation/range traps.

/// `Number.isNaN(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_num_is_nan(_ctx: *mut Context, value: f64) -> i32 {
    i32::from(crate::num::is_nan(value))
}

/// `Number.isFinite(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_num_is_finite(_ctx: *mut Context, value: f64) -> i32 {
    i32::from(crate::num::is_finite(value))
}

/// `Number.isInteger(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_num_is_integer(_ctx: *mut Context, value: f64) -> i32 {
    i32::from(crate::num::is_integer(value))
}

/// `Number.isSafeInteger(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_num_is_safe_integer(
    _ctx: *mut Context,
    value: f64,
) -> i32 {
    i32::from(crate::num::is_safe_integer(value))
}

/// `parseInt(s, radix)`: explicit radix 2–36, otherwise a Q25 trap.
///
/// # Safety
///
/// Shared contract; `s` is a live UTF-8 string handle.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_num_parse_int(
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
pub unsafe extern "C" fn sub_rt_num_parse_float(
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
pub unsafe extern "C" fn sub_rt_num_to_fixed(
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
pub unsafe extern "C" fn sub_rt_num_to_string_f32(
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
pub unsafe extern "C" fn sub_rt_num_to_string_f64(
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
pub unsafe extern "C" fn sub_rt_num_to_exponential(
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
pub unsafe extern "C" fn sub_rt_num_to_precision(
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
// Every `sub_rt_math_*` symbol takes the Context pointer first, so both
// tiers emit every Math call identically. The f64 subset returns f64;
// clz32 is `(ctx, u32) -> i32`, imul is `(ctx, i32, i32) -> i32`, and
// fround is `(ctx, f64) -> f64`. Pure entries ignore `ctx`; only random
// reads Context state. Both tiers must call these opaque symbols —
// never a direct libm/builtin operation (stdlib.md §0.2/Q26/Q27).

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
    sub_rt_math_abs => abs,
    /// `Math.acos`.
    sub_rt_math_acos => acos,
    /// `Math.acosh`.
    sub_rt_math_acosh => acosh,
    /// `Math.asin`.
    sub_rt_math_asin => asin,
    /// `Math.asinh`.
    sub_rt_math_asinh => asinh,
    /// `Math.atan`.
    sub_rt_math_atan => atan,
    /// `Math.atanh`.
    sub_rt_math_atanh => atanh,
    /// `Math.cbrt`.
    sub_rt_math_cbrt => cbrt,
    /// `Math.ceil`.
    sub_rt_math_ceil => ceil,
    /// `Math.cos`.
    sub_rt_math_cos => cos,
    /// `Math.cosh`.
    sub_rt_math_cosh => cosh,
    /// `Math.exp`.
    sub_rt_math_exp => exp,
    /// `Math.expm1`.
    sub_rt_math_expm1 => expm1,
    /// `Math.floor`.
    sub_rt_math_floor => floor,
    /// `Math.log`.
    sub_rt_math_log => log,
    /// `Math.log1p`.
    sub_rt_math_log1p => log1p,
    /// `Math.log10`.
    sub_rt_math_log10 => log10,
    /// `Math.log2`.
    sub_rt_math_log2 => log2,
    /// `Math.round` (ECMA half-toward-+∞).
    sub_rt_math_round => round,
    /// `Math.sign` (±0/±1/NaN).
    sub_rt_math_sign => sign,
    /// `Math.sin`.
    sub_rt_math_sin => sin,
    /// `Math.sinh`.
    sub_rt_math_sinh => sinh,
    /// `Math.sqrt`.
    sub_rt_math_sqrt => sqrt,
    /// `Math.tan`.
    sub_rt_math_tan => tan,
    /// `Math.tanh`.
    sub_rt_math_tanh => tanh,
    /// `Math.trunc`.
    sub_rt_math_trunc => trunc,
}

math_ffi_binary! {
    /// `Math.atan2(y, x)`.
    sub_rt_math_atan2 => atan2,
    /// `Math.hypot(a, b)` (two arguments, Q19).
    sub_rt_math_hypot => hypot,
    /// `Math.pow(base, exp)` (ECMA edges).
    sub_rt_math_pow => pow,
    /// `Math.max(a, b)` (NaN propagation, zero ordering).
    sub_rt_math_max => max,
    /// `Math.min(a, b)` (NaN propagation, zero ordering).
    sub_rt_math_min => min,
}

/// `Math.clz32(x)`: Rust defines the zero input as 32; this opaque
/// entry prevents the ship tier from emitting C's undefined
/// `__builtin_clz(0)`.
#[no_mangle]
pub extern "C" fn sub_rt_math_clz32(ctx: *mut Context, x: u32) -> i32 {
    let _ = ctx;
    crate::math::clz32(x)
}

/// `Math.imul(a, b)`: wrapping 32-bit multiplication.
#[no_mangle]
pub extern "C" fn sub_rt_math_imul(ctx: *mut Context, a: i32, b: i32) -> i32 {
    let _ = ctx;
    crate::math::imul(a, b)
}

/// `Math.fround(x)`: exact `f64 -> f32 -> f64` rounding.
#[no_mangle]
pub extern "C" fn sub_rt_math_fround(ctx: *mut Context, x: f64) -> f64 {
    let _ = ctx;
    crate::math::fround(x)
}

/// `Math.random()` (stdlib.md §2): the next deterministic draw from the
/// Context-owned xoshiro256++ stream.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_math_random(ctx: *mut Context) -> f64 {
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
pub unsafe extern "C" fn sub_rt_ctx_seed_random(ctx: *mut Context, seed: u64) {
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
pub unsafe extern "C" fn sub_rt_date_utc(
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
pub unsafe extern "C" fn sub_rt_date_new(ctx: *mut Context, ms: i64, pos_id: u32) -> i64 {
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
/// pinned by [`sub_rt_ctx_set_now`], else the system clock.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_date_now(ctx: *mut Context) -> i64 {
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
pub unsafe extern "C" fn sub_rt_date_get(ctx: *mut Context, ms: i64, field: u32) -> i32 {
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
pub unsafe extern "C" fn sub_rt_date_to_iso(
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
pub unsafe extern "C" fn sub_rt_ctx_set_now(ctx: *mut Context, ms: i64) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.set_now(ms);
}

// ----- arrays (Q4) -----

/// Allocates an empty dynamic array of `elem_size`-byte elements.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_array_new(
    ctx: *mut Context,
    elem_size: u64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.array_new(elem_size as usize, pos_id)
}

/// Array length.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_array_len(ctx: *mut Context, a: *const u8) -> i32 {
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
pub unsafe extern "C" fn sub_rt_array_push(
    ctx: *mut Context,
    a: *mut u8,
    src: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    unsafe { (*ctx).array_push(a, src, pos_id) }
}

/// `pop()`: removes the last element into `dst`; traps when empty.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle, `dst` writable for
/// the element size.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_array_pop(
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
pub unsafe extern "C" fn sub_rt_array_ptr(
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

/// Decodes an element-kind tag; an unknown tag records an Internal trap
/// (compiler↔runtime skew, never a program fault).
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
pub unsafe extern "C" fn sub_rt_arr_index_of(
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
/// As [`sub_rt_arr_index_of`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_arr_last_index_of(
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
/// As [`sub_rt_arr_index_of`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_arr_includes(
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
pub unsafe extern "C" fn sub_rt_arr_join(
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
pub unsafe extern "C" fn sub_rt_arr_slice(
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
pub unsafe extern "C" fn sub_rt_arr_fill(
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
pub unsafe extern "C" fn sub_rt_arr_reverse(ctx: *mut Context, a: *mut u8) {
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
pub unsafe extern "C" fn sub_rt_arr_concat(
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
pub unsafe extern "C" fn sub_rt_arr_splice(
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
pub unsafe extern "C" fn sub_rt_arr_shift(
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
pub unsafe extern "C" fn sub_rt_arr_unshift(
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
pub unsafe extern "C" fn sub_rt_arr_copy_within(
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
pub unsafe extern "C" fn sub_rt_arr_for_each(
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
pub unsafe extern "C" fn sub_rt_arr_map(
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
pub unsafe extern "C" fn sub_rt_arr_filter(
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
pub unsafe extern "C" fn sub_rt_arr_reduce(
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
/// As [`sub_rt_arr_reduce`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_arr_reduce_right(
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
pub unsafe extern "C" fn sub_rt_arr_some(
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
/// As [`sub_rt_arr_some`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_arr_every(
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
/// As [`sub_rt_arr_some`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_arr_find_index(
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
pub unsafe extern "C" fn sub_rt_fixed_arr_for_each(
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
/// As [`sub_rt_fixed_arr_for_each`], with the result ABI described by
/// `ret_kind` and `ret_size`.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fixed_arr_map(
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
    let (Some(elem_kind), Some(ret_kind)) = (
        unsafe { decode_elem_kind(ctx, elem_kind) },
        unsafe { decode_elem_kind(ctx, ret_kind) },
    ) else {
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
/// As [`sub_rt_fixed_arr_for_each`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fixed_arr_filter(
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
    let (Some(elem_kind), Some(acc_kind)) = (
        unsafe { decode_elem_kind(ctx, elem_kind) },
        unsafe { decode_elem_kind(ctx, acc_kind) },
    ) else {
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
/// As [`sub_rt_fixed_arr_for_each`]; `acc` is readable and writable for
/// `acc_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fixed_arr_reduce(
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
/// As [`sub_rt_fixed_arr_reduce`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fixed_arr_reduce_right(
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
            true, ctx, data, len, elem_size, code, env, elem_kind, acc_kind, acc_size, acc,
            indexed,
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
/// As [`sub_rt_fixed_arr_for_each`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fixed_arr_some(
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
/// As [`sub_rt_fixed_arr_some`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fixed_arr_every(
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
/// As [`sub_rt_fixed_arr_some`].
#[no_mangle]
pub unsafe extern "C" fn sub_rt_fixed_arr_find_index(
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
pub unsafe extern "C" fn sub_rt_arr_sort(
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
/// [`sub_rt_str_len`].
///
/// # Safety
///
/// Shared contract; `s` is a live string handle (or null).
#[no_mangle]
pub unsafe extern "C" fn sub_rt_str_data(ctx: *const Context, s: *const u8) -> *const u8 {
    if s.is_null() {
        return std::ptr::null();
    }
    // SAFETY: shared contract; live string handle.
    unsafe { (*ctx).str_data(s) }
}

/// Data pointer of a dynamic array: the `const T*` half of a
/// `(ptr, count)` descriptor passed to a foreign call. Count is
/// [`sub_rt_array_len`]. Null for an array that has never grown.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle (or null).
#[no_mangle]
pub unsafe extern "C" fn sub_rt_array_data(ctx: *const Context, a: *const u8) -> *const u8 {
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
/// [`sub_rt_cb_trampoline`] reads it back. The binding lives for the whole
/// Context (Q13 lifetime rule).
///
/// # Safety
///
/// Shared contract; `code`/`env` are a language function value (a
/// non-capturing wrapper, so `env` is null); `userdata1`/`userdata2`
/// outlive the run.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_cb_bind(
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
/// [`sub_rt_cb_bind`]. It reconstructs the language `string` from the
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
/// `userdata1` is a binding produced by [`sub_rt_cb_bind`] on the running
/// Context; `message` points at `len` readable bytes (or is null/empty).
#[no_mangle]
pub unsafe extern "C" fn sub_rt_cb_trampoline(
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
    // SAFETY: `rec.ctx` is the live Context captured at bind time.
    let ctx = unsafe { &mut *rec.ctx };
    // A trap already stopped the script (e.g. an earlier callback in the
    // same foreign call trapped): do not run script code — a trap stops
    // the run, even when a C API fires the callback more than once.
    if ctx.trapped() {
        return;
    }
    let bytes: &[u8] = if message.data.is_null() || message.len == 0 {
        &[]
    } else {
        // SAFETY: caller guarantees `len` readable bytes at `data`.
        unsafe { std::slice::from_raw_parts(message.data, message.len) }
    };
    let s = ctx.alloc_str(bytes, 0);
    // The language function value's wrapper takes `(ctx, env, args...)`
    // with the host C calling convention; here the args are the `string`
    // handle and the two userdata slots (§14.4).
    type LangCb = unsafe extern "C" fn(*mut Context, *const u8, *mut u8, *mut u8, *mut u8);
    // SAFETY: `rec.code` is a language callback wrapper of this shape.
    let f: LangCb = unsafe { std::mem::transmute::<*const u8, LangCb>(rec.code) };
    // SAFETY: calling generated code that never unwinds across FFI.
    unsafe { f(rec.ctx, rec.env, s, rec.userdata1, rec.userdata2) };
}

// ----- host driver entry points -----
//
// These are not called by generated code; they are the C-ABI surface a
// host entry program uses to drive an AOT-linked script
// (`specs/blocks/compiler.md` §8.1): create a Context, call the
// program's exported entries, then read the sink and the trap state.

/// Creates a Context and transfers ownership to the caller, who must
/// return it with [`sub_rt_ctx_release`]. Never null.
///
/// The returned Context is a ship-tier (releasing) Context (§8.1a/§8.1b):
/// its `unsafeDelete`/`collect` release storage immediately — arena
/// blocks to their free lists, large allocations to the system — rather
/// than retaining and poisoning (built via [`Context::new_releasing`]).
#[no_mangle]
pub extern "C" fn sub_rt_ctx_new() -> *mut Context {
    Box::into_raw(Context::new_releasing())
}

/// Destroys a Context created by [`sub_rt_ctx_new`], freeing every
/// allocation it owns.
///
/// # Safety
///
/// `ctx` must be a pointer returned by [`sub_rt_ctx_new`] that has not
/// been released yet; no handle into it may be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_ctx_release(ctx: *mut Context) {
    if ctx.is_null() {
        return;
    }
    // SAFETY: caller guarantees `ctx` came from `sub_rt_ctx_new` and is
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
pub unsafe extern "C" fn sub_rt_ctx_stdout(ctx: *const Context, len: *mut u64) -> *const u8 {
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
pub unsafe extern "C" fn sub_rt_ctx_trap_kind(ctx: *const Context) -> u32 {
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
pub unsafe extern "C" fn sub_rt_ctx_trap_pos_id(ctx: *const Context) -> u32 {
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
pub unsafe extern "C" fn sub_rt_ctx_trap_message(ctx: *const Context, len: *mut u64) -> *const u8 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_f16_conversion_round_trips_raw_binary16_storage() {
        let bits = sub_rt_f16_from_f64(1.0006);
        assert_eq!(bits, 0x3c01);
        assert_eq!(sub_rt_f16_to_f64(bits), 1.0009765625);
        assert_eq!(
            sub_rt_f16_to_f64(sub_rt_f16_from_f64(-0.0)).to_bits(),
            (-0.0f64).to_bits()
        );
    }

    #[test]
    fn ffi_host_driver_round_trip() {
        let ctx = sub_rt_ctx_new();
        assert!(!ctx.is_null());
        // SAFETY: `ctx` is the context just created; released once below.
        unsafe {
            let s = sub_rt_str_lit(ctx, b"hi".as_ptr(), 2, 0);
            sub_rt_print(ctx, s);
            let mut len: u64 = 0;
            let p = sub_rt_ctx_stdout(ctx, &mut len);
            assert_eq!(std::slice::from_raw_parts(p, len as usize), b"hi\n");
            assert_eq!(sub_rt_ctx_trap_kind(ctx), 0);
            let mut mlen: u64 = 1;
            assert!(sub_rt_ctx_trap_message(ctx, &mut mlen).is_null());
            assert_eq!(mlen, 0);
            sub_rt_trap(ctx, TrapKind::EmptyPop as u32, 4);
            assert_eq!(sub_rt_ctx_trap_kind(ctx), TrapKind::EmptyPop as u32);
            assert_eq!(sub_rt_ctx_trap_pos_id(ctx), 4);
            let m = sub_rt_ctx_trap_message(ctx, &mut mlen);
            assert!(!m.is_null() && mlen > 0);
            sub_rt_ctx_release(ctx);
        }
    }

    #[test]
    fn ffi_release_of_null_is_a_no_op() {
        // SAFETY: null is explicitly accepted.
        unsafe { sub_rt_ctx_release(std::ptr::null_mut()) };
    }

    #[test]
    fn ffi_string_round_trip() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        static LIT: &[u8] = b"alpha-beta";
        // SAFETY: valid context; literal data is 'static.
        unsafe {
            let s = sub_rt_str_lit(p, LIT.as_ptr(), LIT.len() as u64, 0);
            assert_eq!(sub_rt_str_len(p, s), 10);
            let tail = sub_rt_str_slice(p, s, 6, 10, 0);
            let lit_beta = sub_rt_str_lit(p, b"beta".as_ptr(), 4, 0);
            assert_eq!(sub_rt_str_eq(p, tail, lit_beta), 1);
            assert_eq!(sub_rt_str_eq(p, s, lit_beta), 0);
            let empty = sub_rt_str_slice(p, s, -2, 3, 0);
            assert_eq!(ctx.str_bytes(empty), b"");
            let joined = sub_rt_str_concat(p, s, lit_beta, 0);
            assert_eq!(ctx.str_bytes(joined), b"alpha-betabeta");
        }
    }

    #[test]
    fn ffi_slice_off_utf8_boundary_traps() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context and 'static literal.
        unsafe {
            let s = sub_rt_str_lit(p, "héllo".as_bytes().as_ptr(), 6, 0);
            let out = sub_rt_str_slice(p, s, 0, 2, 42);
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
            let s = sub_rt_str_lit(p, S.as_ptr(), S.len() as u64, 0);
            let o = sub_rt_str_lit(p, b"o".as_ptr(), 1, 0);
            let empty = sub_rt_str_lit(p, b"".as_ptr(), 0, 0);
            let world = sub_rt_str_lit(p, b"world".as_ptr(), 5, 0);
            assert_eq!(sub_rt_str_index_of(p, s, o, 0), 4);
            assert_eq!(sub_rt_str_index_of(p, s, o, 5), 7);
            assert_eq!(sub_rt_str_index_of(p, s, o, -3), 4);
            assert_eq!(sub_rt_str_index_of(p, s, o, 99), -1);
            assert_eq!(sub_rt_str_index_of(p, s, empty, 99), 11);
            assert_eq!(sub_rt_str_last_index_of(p, s, o), 7);
            assert_eq!(sub_rt_str_last_index_of(p, s, empty), 11);
            assert_eq!(sub_rt_str_includes(p, s, world, 0), 1);
            assert_eq!(sub_rt_str_includes(p, s, world, 7), 0);
            assert_eq!(sub_rt_str_includes(p, s, empty, 0), 1);
            assert_eq!(sub_rt_str_starts_with(p, s, world, 0), 0);
            assert_eq!(sub_rt_str_starts_with(p, s, world, 6), 1);
            assert_eq!(sub_rt_str_ends_with(p, s, world, i32::MAX), 1);
            assert_eq!(sub_rt_str_ends_with(p, s, world, 6), 0);
            assert_eq!(sub_rt_str_char_code_at(p, s, 0, 0), 104);
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
            let s = sub_rt_str_lit(p, b"abc".as_ptr(), 3, 0);
            assert_eq!(sub_rt_str_char_code_at(p, s, 3, 17), 0);
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
            let s = sub_rt_str_lit(p, TEXT.as_ptr(), TEXT.len() as u64, 0);
            let mut roots = [s];
            sub_rt_shadow_push(p, roots.as_mut_ptr().cast(), roots.len() as u64);
            let reversed = sub_rt_str_substring(p, s, 4, -2, 0);
            assert_eq!(ctx.str_bytes(reversed), "hél".as_bytes());
            let tail = sub_rt_str_substr(p, s, -3, i32::MAX, 0);
            assert_eq!(ctx.str_bytes(tail), b"llo");
            let empty = sub_rt_str_substr(p, s, 3, 0, 0);
            assert_eq!(ctx.str_bytes(empty), b"");
            let multibyte = sub_rt_str_char_at(p, s, 1, 0);
            assert_eq!(ctx.str_bytes(multibyte), "é".as_bytes());
            let out_of_range = sub_rt_str_char_at(p, s, 99, 0);
            assert_eq!(ctx.str_bytes(out_of_range), b"");
            assert_eq!(sub_rt_str_code_point_at(p, s, 1, 0), 'é' as i32);
            let suffix = sub_rt_str_lit(p, b"!".as_ptr(), 1, 0);
            let joined = sub_rt_str_method_concat(p, s, suffix, 0);
            assert_eq!(ctx.str_bytes(joined), "héllo!".as_bytes());
            sub_rt_shadow_pop(p);
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
            let s = sub_rt_str_lit(p, TEXT.as_ptr(), TEXT.len() as u64, 0);
            assert!(sub_rt_str_char_at(p, s, 1, 31).is_null());
        }
        let report = ctx.trap_record().expect("charAt boundary trap");
        assert_eq!(report.kind, TrapKind::StrRange);
        assert_eq!(report.pos_id, 31);

        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context, static literal bytes, and a live handle.
        unsafe {
            let s = sub_rt_str_lit(p, TEXT.as_ptr(), TEXT.len() as u64, 0);
            assert_eq!(sub_rt_str_code_point_at(p, s, 2, 32), 0);
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
            let s = sub_rt_str_lit(p, S.as_ptr(), S.len() as u64, 0);
            let comma = sub_rt_str_lit(p, b",".as_ptr(), 1, 0);
            let arr = sub_rt_str_split(p, s, comma, 0);
            assert!(!arr.is_null());
            assert_eq!(sub_rt_array_len(p, arr), 3);
            let data = sub_rt_array_data(p, arr) as *const u64;
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
            let s = sub_rt_str_lit(p, b"ab".as_ptr(), 2, 0);
            let empty = sub_rt_str_lit(p, b"".as_ptr(), 0, 0);
            assert!(sub_rt_str_split(p, s, empty, 23).is_null());
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
            let s = sub_rt_str_lit(p, S.as_ptr(), S.len() as u64, 0);
            let t = sub_rt_str_trim(p, s, 0);
            assert_eq!(ctx.str_bytes(t), b"x");
            let ts = sub_rt_str_trim_start(p, s, 0);
            assert_eq!(ctx.str_bytes(ts), "x\u{00A0}".as_bytes());
            let te = sub_rt_str_trim_end(p, s, 0);
            assert_eq!(ctx.str_bytes(te), "\u{3000}\u{FEFF}x".as_bytes());
            let mixed_bytes = "ß ﬄ ΣΣς İ ı".as_bytes();
            let mixed = sub_rt_str_lit(p, mixed_bytes.as_ptr(), mixed_bytes.len() as u64, 0);
            let up = sub_rt_str_to_upper(p, mixed, 0);
            assert_eq!(ctx.str_bytes(up), "SS FFL ΣΣΣ İ I".as_bytes());
            let low = sub_rt_str_to_lower(p, up, 0);
            assert_eq!(ctx.str_bytes(low), "ss ffl σσς i\u{0307} i".as_bytes());
            let dotted_i_bytes = "İ".as_bytes();
            let dotted_i =
                sub_rt_str_lit(p, dotted_i_bytes.as_ptr(), dotted_i_bytes.len() as u64, 0);
            let dotted_i_low = sub_rt_str_to_lower(p, dotted_i, 0);
            assert_eq!(ctx.str_bytes(dotted_i_low), "i\u{0307}".as_bytes());
            assert_eq!(sub_rt_str_len(p, dotted_i_low), 3);
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
            let s = sub_rt_str_lit(p, b"ab".as_ptr(), 2, 0);
            let three = sub_rt_str_repeat(p, s, 3, 0);
            assert_eq!(ctx.str_bytes(three), b"ababab");
            let zero = sub_rt_str_repeat(p, s, 0, 0);
            assert_eq!(ctx.str_bytes(zero), b"");
            assert!(ctx.trap_record().is_none());
            assert!(sub_rt_str_repeat(p, s, -1, 31).is_null());
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
            let s = sub_rt_str_lit(p, b"ab".as_ptr(), 2, 0);
            let xy = sub_rt_str_lit(p, b"xy".as_ptr(), 2, 0);
            // The pinned JS truncation rule: "ab" to 5 with "xy".
            let start = sub_rt_str_pad_start(p, s, 5, xy, 0);
            assert_eq!(ctx.str_bytes(start), b"xyxab");
            let end = sub_rt_str_pad_end(p, s, 5, xy, 0);
            assert_eq!(ctx.str_bytes(end), b"abxyx");
            // Already long enough: unchanged bytes, fresh allocation.
            let same = sub_rt_str_pad_start(p, s, 2, xy, 0);
            assert_eq!(ctx.str_bytes(same), b"ab");
            assert_ne!(same, s as *mut u8);
            // Empty pad with no fill needed is the documented no-op.
            let empty = sub_rt_str_lit(p, b"".as_ptr(), 0, 0);
            let noop = sub_rt_str_pad_end(p, s, 2, empty, 0);
            assert_eq!(ctx.str_bytes(noop), b"ab");
            assert!(ctx.trap_record().is_none());
            // Empty pad that must fill traps (Q21).
            assert!(sub_rt_str_pad_start(p, s, 5, empty, 37).is_null());
        }
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::StrRange);
        assert_eq!(r.pos_id, 37);
        assert!(r.message.contains("padStart"));
    }

    #[test]
    fn ffi_str_replace_first_all_and_empty_pattern_trap() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context; handles are live.
        unsafe {
            let s = sub_rt_str_lit(p, b"abcabc".as_ptr(), 6, 0);
            let bc = sub_rt_str_lit(p, b"bc".as_ptr(), 2, 0);
            let x = sub_rt_str_lit(p, b"X".as_ptr(), 1, 0);
            let first = sub_rt_str_replace(p, s, bc, x, 0);
            assert_eq!(ctx.str_bytes(first), b"aXabc");
            let all = sub_rt_str_replace_all(p, s, bc, x, 0);
            assert_eq!(ctx.str_bytes(all), b"aXaX");
            assert!(ctx.trap_record().is_none());
            let empty = sub_rt_str_lit(p, b"".as_ptr(), 0, 0);
            // replace accepts an empty pattern (match at 0)...
            let prefixed = sub_rt_str_replace(p, s, empty, x, 0);
            assert_eq!(ctx.str_bytes(prefixed), b"Xabcabc");
            assert!(ctx.trap_record().is_none());
            // ...replaceAll traps on it (Q21).
            assert!(sub_rt_str_replace_all(p, s, empty, x, 41).is_null());
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
            let s = sub_rt_fmt_f64(p, 3.75, 0);
            sub_rt_print(p, s);
            let t = sub_rt_fmt_bool(p, 1, 0);
            sub_rt_print(p, t);
        }
        assert_eq!(ctx.take_stdout(), b"3.75\ntrue\n");
    }

    #[test]
    fn ffi_number_entries_forward_and_trap_ranges() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context; all string handles remain live.
        unsafe {
            assert_eq!(sub_rt_num_is_nan(p, f64::NAN), 1);
            assert_eq!(sub_rt_num_is_finite(p, f64::INFINITY), 0);
            assert_eq!(sub_rt_num_is_integer(p, 7.0), 1);
            assert_eq!(
                sub_rt_num_is_safe_integer(p, 9_007_199_254_740_992.0),
                0
            );
            let int_s = sub_rt_str_lit(p, b"fftail".as_ptr(), 6, 0);
            assert_eq!(sub_rt_num_parse_int(p, int_s, 16, 19), 255.0);
            let float_s = sub_rt_str_lit(p, b"1.5tail".as_ptr(), 7, 0);
            assert_eq!(sub_rt_num_parse_float(p, float_s, 20), 1.5);
            let fixed = sub_rt_num_to_fixed(p, 1.005, 2, 20);
            assert_eq!(ctx.str_bytes(fixed), b"1.00");
            let radix_f32 = sub_rt_num_to_string_f32(p, 10.5, 2, 20);
            assert_eq!(ctx.str_bytes(radix_f32), b"1010.1");
            let radix = sub_rt_num_to_string_f64(p, 1234.5678, 36, 20);
            assert_eq!(ctx.str_bytes(radix), b"ya.kfv9yqdpm");
            let exponential = sub_rt_num_to_exponential(p, 0.0, 2, 20);
            assert_eq!(ctx.str_bytes(exponential), b"0.00e+0");
            let precision = sub_rt_num_to_precision(p, 123.456, 2, 20);
            assert_eq!(ctx.str_bytes(precision), b"1.2e+2");
            assert_eq!(sub_rt_math_clz32(p, 0), 32);
            assert_eq!(sub_rt_math_imul(p, i32::MAX, 2), -2);
            assert_eq!(sub_rt_math_fround(p, 1.1), 1.100_000_023_841_858);
            assert!(sub_rt_num_to_fixed(p, 1.0, 101, 21).is_null());
        }
        let report = ctx.trap_record().expect("toFixed range trap");
        assert_eq!(report.kind, TrapKind::NumberRange);
        assert_eq!(report.pos_id, 21);

        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context and live literal string handle.
        unsafe {
            let s = sub_rt_str_lit(p, b"10".as_ptr(), 2, 0);
            assert!(sub_rt_num_parse_int(p, s, 1, 22).is_nan());
        }
        let report = ctx.trap_record().expect("parseInt radix trap");
        assert_eq!(report.kind, TrapKind::NumberRange);
        assert_eq!(report.pos_id, 22);

        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let s = ctx.alloc_str(&[0xff], 0);
        // SAFETY: valid context and live string handle; the invalid byte
        // exercises the defensive compiler/runtime-skew trap.
        unsafe {
            assert!(sub_rt_num_parse_float(p, s, 23).is_nan());
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
            let a = sub_rt_array_new(p, 4, 0);
            let v: i32 = 11;
            assert_eq!(sub_rt_array_push(p, a, &v as *const i32 as *const u8, 0), 1);
            assert_eq!(sub_rt_array_len(p, a), 1);
            assert!(sub_rt_array_ptr(p, a, 3, 9).is_null());
        }
        assert_eq!(
            ctx.trap_record().map(|r| (r.kind, r.pos_id)),
            Some((TrapKind::IndexOutOfBounds, 9))
        );
    }

    #[test]
    fn ffi_emitted_trap_entry_records_kind_and_pos() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe { sub_rt_trap(p, TrapKind::UseAfterDelete as u32, 12) };
        let r = ctx.trap_record().expect("trap");
        assert_eq!(r.kind, TrapKind::UseAfterDelete);
        assert_eq!(r.pos_id, 12);
    }

    #[test]
    fn ffi_map_set_operations_trap_on_deleted_receivers_in_development() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context and monomorphized i32 shapes.
        unsafe {
            let map = sub_rt_map_new(p, 4, 4, 0, 1);
            sub_rt_delete(p, map, 2);
            assert_eq!(sub_rt_map_size(p, map), 0);
        }
        assert_eq!(
            ctx.trap_record().map(|r| (r.kind, r.pos_id)),
            Some((TrapKind::UseAfterDelete, 0))
        );

        ctx.clear_trap();
        // SAFETY: valid context and monomorphized i32 shape. The stale
        // receiver is validated before the key is inspected.
        unsafe {
            let set = sub_rt_set_new(p, 4, 0, 3);
            sub_rt_delete(p, set, 4);
            let key = 9i32;
            assert_eq!(
                sub_rt_set_add(p, set, (&key as *const i32).cast(), 17),
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
        unsafe { sub_rt_trap(p, 999, 3) };
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
        assert_eq!(sub_rt_math_abs(p, -3.5), 3.5);
        assert_eq!(sub_rt_math_acos(p, 1.0), 0.0);
        assert_eq!(sub_rt_math_acosh(p, 1.0), 0.0);
        assert_eq!(sub_rt_math_asin(p, 0.0), 0.0);
        assert_eq!(sub_rt_math_asinh(p, 0.0), 0.0);
        assert_eq!(sub_rt_math_atan(p, 0.0), 0.0);
        assert_eq!(sub_rt_math_atanh(p, 0.0), 0.0);
        assert_eq!(sub_rt_math_cbrt(p, 27.0), 3.0);
        assert_eq!(sub_rt_math_ceil(p, 1.2), 2.0);
        assert_eq!(sub_rt_math_cos(p, 0.0), 1.0);
        assert_eq!(sub_rt_math_cosh(p, 0.0), 1.0);
        assert_eq!(sub_rt_math_exp(p, 0.0), 1.0);
        assert_eq!(sub_rt_math_expm1(p, 0.0), 0.0);
        assert_eq!(sub_rt_math_floor(p, 1.8), 1.0);
        assert_eq!(sub_rt_math_log(p, 1.0), 0.0);
        assert_eq!(sub_rt_math_log1p(p, 0.0), 0.0);
        assert_eq!(sub_rt_math_log10(p, 1000.0), 3.0);
        assert_eq!(sub_rt_math_log2(p, 8.0), 3.0);
        assert_eq!(sub_rt_math_round(p, -2.5), -2.0);
        assert_eq!(sub_rt_math_sign(p, -7.5), -1.0);
        assert_eq!(sub_rt_math_sin(p, 0.0), 0.0);
        assert_eq!(sub_rt_math_sinh(p, 0.0), 0.0);
        assert_eq!(sub_rt_math_sqrt(p, 9.0), 3.0);
        assert_eq!(sub_rt_math_tan(p, 0.0), 0.0);
        assert_eq!(sub_rt_math_tanh(p, 0.0), 0.0);
        assert_eq!(sub_rt_math_trunc(p, -1.7), -1.0);
        assert_eq!(sub_rt_math_atan2(p, 0.0, 1.0), 0.0);
        assert_eq!(sub_rt_math_hypot(p, 3.0, 4.0), 5.0);
        assert_eq!(sub_rt_math_pow(p, 2.0, 10.0), 1024.0);
        assert_eq!(sub_rt_math_max(p, 2.5, 7.0), 7.0);
        assert_eq!(sub_rt_math_min(p, 2.5, 7.0), 2.5);
    }

    #[test]
    fn ffi_date_entries_forward_to_the_date_module() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe {
            let ms = sub_rt_date_utc(p, 2020, 5, 15, 12, 34, 56, 789, 0);
            assert_eq!(ms, 1_592_224_496_789);
            assert_eq!(sub_rt_date_new(p, ms, 0), ms);
            assert_eq!(sub_rt_date_get(p, ms, crate::date::FIELD_FULL_YEAR), 2020);
            assert_eq!(sub_rt_date_get(p, ms, crate::date::FIELD_MONTH), 5);
            assert_eq!(sub_rt_date_get(p, ms, crate::date::FIELD_DATE), 15);
            assert_eq!(sub_rt_date_get(p, ms, crate::date::FIELD_DAY), 1);
            assert_eq!(sub_rt_date_get(p, ms, crate::date::FIELD_HOURS), 12);
            assert_eq!(sub_rt_date_get(p, ms, crate::date::FIELD_MINUTES), 34);
            assert_eq!(sub_rt_date_get(p, ms, crate::date::FIELD_SECONDS), 56);
            assert_eq!(sub_rt_date_get(p, ms, crate::date::FIELD_MILLISECONDS), 789);
            let iso = sub_rt_date_to_iso(p, ms, 0);
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
            assert_eq!(sub_rt_date_new(p, 8_640_000_000_000_001, 7), 0);
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
            assert_eq!(sub_rt_date_utc(p, 275_760, 8, 14, 0, 0, 0, 0, 9), 0);
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
            assert!(sub_rt_date_to_iso(p, 253_402_300_800_000, 11).is_null());
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
            assert_eq!(sub_rt_date_get(p, 0, 99), 0);
        }
        assert_eq!(ctx.trap_record().map(|r| r.kind), Some(TrapKind::Internal));
    }

    #[test]
    fn ffi_date_now_reads_the_pinned_context_clock() {
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        // SAFETY: valid context.
        unsafe {
            sub_rt_ctx_set_now(p, 1_592_224_496_789);
            assert_eq!(sub_rt_date_now(p), 1_592_224_496_789);
            sub_rt_ctx_set_now(p, -1);
            assert_eq!(sub_rt_date_now(p), -1);
        }
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
                    sub_rt_math_random(p).to_bits(),
                    reference.next_f64().to_bits()
                );
            }
            sub_rt_ctx_seed_random(p, 99);
            let a = sub_rt_math_random(p);
            sub_rt_ctx_seed_random(p, 99);
            let b = sub_rt_math_random(p);
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
                sub_rt_array_push(p, a, (&v as *const i32).cast(), 0);
            }
            let one = 1i32;
            assert_eq!(sub_rt_arr_index_of(p, a, (&one as *const i32).cast(), 0), 1);
            assert_eq!(sub_rt_arr_last_index_of(p, a, (&one as *const i32).cast(), 0), 3);
            assert_eq!(sub_rt_arr_includes(p, a, (&one as *const i32).cast(), 0), 1);
            let sep = ctx.alloc_str(b"-", 0);
            let joined = sub_rt_arr_join(p, a, sep, 0, 0);
            assert_eq!(ctx.str_bytes(joined), b"3-1-2-1");
            let sl = sub_rt_arr_slice(p, a, 1, 3, 0);
            assert_eq!(sub_rt_array_len(p, sl), 2);
            let mapped = sub_rt_arr_map(
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
            sub_rt_arr_sort(p, a, cmp_desc_i32 as *const u8, std::ptr::null(), 0);
            assert_eq!(ctx.array_data(a).cast::<i32>().read_unaligned(), 3);
            let b = sub_rt_arr_slice(p, a, 0, 1, 0);
            let cat = sub_rt_arr_concat(p, a, b, 0);
            assert_eq!(sub_rt_array_len(p, cat), 5);
            let z = 0i32;
            sub_rt_arr_fill(p, a, (&z as *const i32).cast(), 0, i32::MAX);
            sub_rt_arr_reverse(p, a);
            assert_eq!(
                sub_rt_arr_every(
                    p,
                    a,
                    triple_i32 as *const u8,
                    std::ptr::null(),
                    0,
                    0,
                ),
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
            assert_eq!(sub_rt_arr_index_of(p, a, (&x as *const i32).cast(), 99), -1);
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
            sub_rt_root_add(p, range.as_ptr() as *mut u8, 2);
            sub_rt_collect(p);
        }
        assert!(ctx.is_live(a as usize));
        assert!(ctx.is_live(b as usize));
    }
}
