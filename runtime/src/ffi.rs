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

/// `slice(start, end)` with byte offsets; traps when the range is
/// invalid or off a UTF-8 boundary (Q5).
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
    let (lo, hi) = (i64::from(start), i64::from(end));
    if lo < 0 || hi < lo || hi > len {
        ctx.trap(
            TrapKind::StringSlice,
            format!("slice({start}, {end}) out of range for string length {len}"),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    // Strings are UTF-8 by construction (literals, concatenation, and
    // boundary-checked slices of UTF-8 strings).
    let text = std::str::from_utf8(&bytes).unwrap_or_default();
    let (lo, hi) = (lo as usize, hi as usize);
    if !text.is_char_boundary(lo) || !text.is_char_boundary(hi) {
        ctx.trap(
            TrapKind::StringSlice,
            format!("slice({start}, {end}) is not on a UTF-8 boundary"),
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

// ----- Math (stdlib.md §1/§2) -----
//
// One uniform signature convention: every `sub_rt_math_*` symbol takes
// the Context pointer first (`sub_rt_math_<fn>(ctx, args…) -> f64`,
// stdlib.md §1), so both tiers emit every Math call identically. The
// pure entries ignore `ctx` (and are safe: nothing is dereferenced);
// only `random` reads Context state. Both tiers must call these opaque
// symbols — never a direct libm call, which clang constant-folds at
// `-O2` (stdlib.md §0.2).

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
