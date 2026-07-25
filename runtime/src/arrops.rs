//! `Array` method operations (stdlib.md §9, Q22): the one shared
//! implementation both execution tiers call through the `sub_rt_arr_*`
//! entries in [`crate::ffi`].
//!
//! # Element marshaling
//!
//! Values the runtime *receives* (search needles, `fill` values, the
//! `reduce` accumulator) travel by pointer, so every C entry has one
//! fixed signature. Values the runtime *passes to a script callback*
//! travel by value under the language calling convention
//! `(ctx, env, args…)`; the concrete C-ABI class is dispatched here
//! from an [`ElemKind`] signedness/kind tag plus the element byte width, so a type
//! whose width differs between tiers (`boolean`: 1 byte under the dev
//! JIT, 4 under the ship-C emitter) stays correct on both — each tier's
//! arrays and callbacks are internally consistent, and this module
//! derives the ABI from that tier's own element size.
//!
//! # Trap discipline
//!
//! After **every** callback return the Context trap flag is checked;
//! a raised flag aborts the iteration immediately and the operation
//! returns a defensible value (a valid partial array, the last good
//! accumulator, a constant). Entries also return immediately when the
//! Context is *already* trapped, so post-trap execution on the ship-C
//! tier (which has no per-call unwind) never re-enters script code.
//! `sort` runs on a scratch buffer and writes back only on completion,
//! so an aborted sort leaves the array exactly as it was.
//!
//! # Callback invocation safety
//!
//! Callbacks are non-escaping by construction (C5): they are called
//! only during the method call and never stored. The `code` pointer is
//! a language function value's code half — a generated function of
//! shape `(ctx, env, args…)` under the host C calling convention — and
//! generated code never unwinds (the trap-flag discipline), so no
//! unwind crosses these calls.

use crate::context::Context;
use crate::trap::TrapKind;

/// Marshaling kind of an array element (ABI contract with the
/// compiler's `hir::ArrElemKind`; the codes must agree — a codegen test
/// pins them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ElemKind {
    /// Bitwise integer equality at the element width; zero-extending
    /// integer register.
    Int,
    /// Bitwise integer equality at the element width; sign-extending
    /// integer register.
    SignedInt,
    /// IEEE `f32` equality; float register.
    F32,
    /// IEEE `f64` equality; float register.
    F64,
    /// String handle: content equality; pointer-sized integer register.
    Str,
    /// IEEE binary16 equality after widening through the shared conversion
    /// implementation; raw bits use a 16-bit integer register.
    F16,
}

impl ElemKind {
    /// Decodes the stable `u32` form; unknown values map to `None`.
    #[must_use]
    pub fn from_u32(v: u32) -> Option<ElemKind> {
        Some(match v {
            0 => ElemKind::Int,
            1 => ElemKind::F32,
            2 => ElemKind::F64,
            3 => ElemKind::Str,
            4 => ElemKind::F16,
            5 => ElemKind::SignedInt,
            _ => return None,
        })
    }
}

/// Q14 formatting kind of a `join` element (ABI contract with the
/// compiler's `hir::ArrFmtKind`; the codes must agree — a codegen test
/// pins them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FmtKind {
    /// `i32` decimal (also enums).
    I32,
    /// `u32` decimal.
    U32,
    /// `i64` decimal.
    I64,
    /// `u64` decimal.
    U64,
    /// `f32` shortest round-trip.
    F32,
    /// `f64` shortest round-trip.
    F64,
    /// `true` / `false`.
    Bool,
    /// String elements pass through unformatted.
    Str,
    /// `i8` decimal.
    I8,
    /// `u8` decimal.
    U8,
    /// `i16` decimal.
    I16,
    /// `u16` decimal.
    U16,
    /// Binary16 widened through the shared conversion implementation,
    /// then formatted by the `f64` Q14 implementation.
    F16,
}

impl FmtKind {
    /// Decodes the stable `u32` form; unknown values map to `None`.
    #[must_use]
    pub fn from_u32(v: u32) -> Option<FmtKind> {
        Some(match v {
            0 => FmtKind::I32,
            1 => FmtKind::U32,
            2 => FmtKind::I64,
            3 => FmtKind::U64,
            4 => FmtKind::F32,
            5 => FmtKind::F64,
            6 => FmtKind::Bool,
            7 => FmtKind::Str,
            8 => FmtKind::I8,
            9 => FmtKind::U8,
            10 => FmtKind::I16,
            11 => FmtKind::U16,
            12 => FmtKind::F16,
            _ => return None,
        })
    }
}

/// The concrete C-ABI class a value crosses the callback boundary as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Abi {
    U8,
    U16,
    U32,
    U64,
    S8,
    S16,
    I32,
    I64,
    F32,
    F64,
}

/// The ABI class of an element of `kind` occupying `size` bytes, or
/// `None` when the pair is not a shape generated code produces.
fn abi_of(kind: ElemKind, size: usize) -> Option<Abi> {
    match kind {
        ElemKind::F32 => (size == 4).then_some(Abi::F32),
        ElemKind::F64 => (size == 8).then_some(Abi::F64),
        ElemKind::F16 => (size == 2).then_some(Abi::U16),
        ElemKind::Int | ElemKind::Str => match size {
            1 => Some(Abi::U8),
            2 => Some(Abi::U16),
            4 => Some(Abi::U32),
            8 => Some(Abi::U64),
            _ => None,
        },
        ElemKind::SignedInt => match size {
            1 => Some(Abi::S8),
            2 => Some(Abi::S16),
            4 => Some(Abi::I32),
            8 => Some(Abi::I64),
            _ => None,
        },
    }
}

/// Records the internal trap for an element shape the code generators
/// never produce (compiler↔runtime version skew, not a program fault).
unsafe fn abi_or_trap(ctx: *mut Context, kind: ElemKind, size: usize) -> Option<Abi> {
    let abi = abi_of(kind, size);
    if abi.is_none() {
        // SAFETY: shared contract (`ctx` is the live Context).
        unsafe { &mut *ctx }.trap(
            TrapKind::Internal,
            format!("array element kind {kind:?} with width {size} has no ABI class"),
            0,
        );
    }
    abi
}

/// Monomorphizes `$body` over the concrete ABI type `$T` of `$abi`.
macro_rules! with_abi {
    ($abi:expr, $T:ident, $body:block) => {
        match $abi {
            Abi::U8 => {
                type $T = u8;
                $body
            }
            Abi::S8 => {
                type $T = i8;
                $body
            }
            Abi::I32 => {
                type $T = i32;
                $body
            }
            Abi::U16 => {
                type $T = u16;
                $body
            }
            Abi::S16 => {
                type $T = i16;
                $body
            }
            Abi::U32 => {
                type $T = u32;
                $body
            }
            Abi::I64 => {
                type $T = i64;
                $body
            }
            Abi::U64 => {
                type $T = u64;
                $body
            }
            Abi::F32 => {
                type $T = f32;
                $body
            }
            Abi::F64 => {
                type $T = f64;
                $body
            }
        }
    };
}

/// Calls a one-value script callback `(ctx, env, v) -> R`.
///
/// # Safety
///
/// `code` is a language function value's code pointer whose C signature
/// is exactly `(ctx, env, A) -> R` after monomorphization; it never
/// unwinds (trap-flag discipline).
unsafe fn call1<A: Copy, R: Copy>(
    code: *const u8,
    ctx: *mut Context,
    env: *const u8,
    a: A,
) -> R {
    // SAFETY: caller guarantees the signature; a fn pointer and a data
    // pointer have the same size on every supported (64-bit) target.
    let f: unsafe extern "C" fn(*mut Context, *const u8, A) -> R =
        unsafe { std::mem::transmute(code) };
    // SAFETY: caller contract.
    unsafe { f(ctx, env, a) }
}

/// Calls a two-value script callback `(ctx, env, a, b) -> R`.
///
/// # Safety
///
/// As [`call1`], with signature `(ctx, env, A, B) -> R`.
unsafe fn call2<A: Copy, B: Copy, R: Copy>(
    code: *const u8,
    ctx: *mut Context,
    env: *const u8,
    a: A,
    b: B,
) -> R {
    // SAFETY: as `call1`.
    let f: unsafe extern "C" fn(*mut Context, *const u8, A, B) -> R =
        unsafe { std::mem::transmute(code) };
    // SAFETY: caller contract.
    unsafe { f(ctx, env, a, b) }
}

/// Reads element `i` of `h` as `T`, re-resolving the data pointer (a
/// callback may have grown the array and moved its storage).
///
/// # Safety
///
/// `h` is a live array of this context; `i < len`; `size_of::<T>()`
/// equals the element size.
unsafe fn read_elem<T: Copy>(ctx: *mut Context, h: *mut u8, i: usize) -> T {
    // SAFETY: caller guarantees `i` in bounds, so no trap fires here.
    let p = unsafe { (*ctx).array_elem_ptr(h, i as i32, 0) };
    // SAFETY: in-bounds element storage of the element width.
    unsafe { p.cast::<T>().read_unaligned() }
}

/// Current length of `h` as `usize`.
///
/// # Safety
///
/// `h` is a live array of this context.
unsafe fn len_of(ctx: *mut Context, h: *const u8) -> usize {
    // SAFETY: caller contract.
    unsafe { (*ctx).array_len(h) }.max(0) as usize
}

/// Reads `size` (1/2/4/8) bytes at `p` zero-extended to `u64`.
///
/// # Safety
///
/// `p` is readable for `size` bytes.
unsafe fn read_uint(p: *const u8, size: usize) -> u64 {
    // SAFETY: caller contract; unaligned reads.
    unsafe {
        match size {
            1 => u64::from(p.read_unaligned()),
            2 => u64::from(p.cast::<u16>().read_unaligned()),
            4 => u64::from(p.cast::<u32>().read_unaligned()),
            8 => p.cast::<u64>().read_unaligned(),
            _ => 0,
        }
    }
}

// ----- equality searches (indexOf / lastIndexOf / includes) -----

/// `===` equality of the element at `p` and the needle at `x`
/// (stdlib.md §9: bitwise per width for integers/handles/`Date`,
/// IEEE for floats — `NaN` never equal — content for strings).
///
/// # Safety
///
/// `p` and `x` are readable for `size` bytes; `(kind, size)` is a shape
/// [`abi_of`] accepts (the searches check it once, before the loop, as
/// the callback operations do); string handles are live handles of
/// `ctx` (or null).
unsafe fn elem_eq(ctx: *mut Context, kind: ElemKind, size: usize, p: *const u8, x: *const u8) -> bool {
    match kind {
        ElemKind::Int | ElemKind::SignedInt => {
            // SAFETY: caller contract.
            unsafe { read_uint(p, size) == read_uint(x, size) }
        }
        // SAFETY: caller contract (4/8 readable bytes).
        ElemKind::F32 => unsafe {
            p.cast::<f32>().read_unaligned() == x.cast::<f32>().read_unaligned()
        },
        // SAFETY: caller contract.
        ElemKind::F64 => unsafe {
            p.cast::<f64>().read_unaligned() == x.cast::<f64>().read_unaligned()
        },
        // SAFETY: caller contract (2 readable bytes).
        ElemKind::F16 => unsafe {
            let a = crate::half::to_f64(p.cast::<u16>().read_unaligned());
            let b = crate::half::to_f64(x.cast::<u16>().read_unaligned());
            a == b
        },
        ElemKind::Str => {
            // SAFETY: caller contract (8 readable bytes each).
            let a = unsafe { p.cast::<*const u8>().read_unaligned() };
            let b = unsafe { x.cast::<*const u8>().read_unaligned() };
            if a.is_null() || b.is_null() {
                return a == b;
            }
            // SAFETY: live string handles of `ctx`.
            unsafe { (*ctx).str_bytes(a) == (*ctx).str_bytes(b) }
        }
    }
}

/// `indexOf(x)`: first index under per-kind `===` equality, or −1.
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null); `x` is readable for the
/// element size; string elements/needles are live handles.
pub unsafe fn index_of(ctx: *mut Context, h: *mut u8, x: *const u8, kind: ElemKind) -> i32 {
    if h.is_null() || x.is_null() {
        return -1;
    }
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h), (*ctx).array_elem_size(h)) };
    // An element shape the code generators never produce is an internal
    // trap, not a silent comparison — the same guard the callback
    // operations apply (compiler↔runtime version skew).
    // SAFETY: caller contract.
    if unsafe { abi_or_trap(ctx, kind, esz) }.is_none() {
        return -1;
    }
    for i in 0..n {
        // SAFETY: `i < n`; caller contract.
        let p = unsafe { (*ctx).array_elem_ptr(h, i as i32, 0) };
        // SAFETY: element storage and needle are `esz` readable bytes.
        if unsafe { elem_eq(ctx, kind, esz, p, x) } {
            return i as i32;
        }
    }
    -1
}

/// `lastIndexOf(x)`: last index or −1.
///
/// # Safety
///
/// As [`index_of`].
pub unsafe fn last_index_of(ctx: *mut Context, h: *mut u8, x: *const u8, kind: ElemKind) -> i32 {
    if h.is_null() || x.is_null() {
        return -1;
    }
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h), (*ctx).array_elem_size(h)) };
    // As `index_of`: an unsupported element shape traps.
    // SAFETY: caller contract.
    if unsafe { abi_or_trap(ctx, kind, esz) }.is_none() {
        return -1;
    }
    for i in (0..n).rev() {
        // SAFETY: `i < n`; caller contract.
        let p = unsafe { (*ctx).array_elem_ptr(h, i as i32, 0) };
        // SAFETY: as `index_of`.
        if unsafe { elem_eq(ctx, kind, esz, p, x) } {
            return i as i32;
        }
    }
    -1
}

/// `includes(x)`: 1 when found under `===` equality (so `NaN` is never
/// found — the §9 contract pins `===` for all three searches), else 0.
///
/// # Safety
///
/// As [`index_of`].
pub unsafe fn includes(ctx: *mut Context, h: *mut u8, x: *const u8, kind: ElemKind) -> i32 {
    // SAFETY: forwarded contract.
    i32::from(unsafe { index_of(ctx, h, x, kind) } >= 0)
}

// ----- join -----

/// `join(sep)`: the Q14-formatted elements separated by `sep`'s bytes;
/// a fresh string handle (null after an allocation-failure trap).
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null); `sep` is a live string
/// handle (or null); string elements are live handles.
pub unsafe fn join(
    ctx: *mut Context,
    h: *mut u8,
    sep: *const u8,
    kind: FmtKind,
    pos_id: u32,
) -> *mut u8 {
    if h.is_null() || sep.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract. Copied out so the borrow does not overlap
    // the alloc below.
    let sep_bytes: Vec<u8> = unsafe { (*ctx).str_bytes(sep) }.to_vec();
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h), (*ctx).array_elem_size(h)) };
    let mut out: Vec<u8> = Vec::new();
    for i in 0..n {
        if i > 0 {
            out.extend_from_slice(&sep_bytes);
        }
        // SAFETY: `i < n`; caller contract.
        let p = unsafe { (*ctx).array_elem_ptr(h, i as i32, 0) };
        match kind {
            // SAFETY (each arm): element storage is `esz` readable bytes
            // of the arm's type; widths are fixed by the kind except the
            // integer-read arms, which read the tier's own width.
            FmtKind::I32 => out.extend_from_slice(
                crate::fmt::fmt_i32(unsafe { p.cast::<i32>().read_unaligned() }).as_bytes(),
            ),
            FmtKind::U32 => out.extend_from_slice(
                crate::fmt::fmt_u32(unsafe { p.cast::<u32>().read_unaligned() }).as_bytes(),
            ),
            FmtKind::I64 => out.extend_from_slice(
                crate::fmt::fmt_i64(unsafe { p.cast::<i64>().read_unaligned() }).as_bytes(),
            ),
            FmtKind::U64 => out.extend_from_slice(
                crate::fmt::fmt_u64(unsafe { p.cast::<u64>().read_unaligned() }).as_bytes(),
            ),
            FmtKind::F32 => out.extend_from_slice(
                crate::fmt::fmt_f32(unsafe { p.cast::<f32>().read_unaligned() }).as_bytes(),
            ),
            FmtKind::F64 => out.extend_from_slice(
                crate::fmt::fmt_f64(unsafe { p.cast::<f64>().read_unaligned() }).as_bytes(),
            ),
            FmtKind::I8 => out.extend_from_slice(
                crate::fmt::fmt_i32(i32::from(unsafe { p.cast::<i8>().read_unaligned() }))
                    .as_bytes(),
            ),
            FmtKind::U8 => out.extend_from_slice(
                crate::fmt::fmt_u32(u32::from(unsafe { p.read_unaligned() })).as_bytes(),
            ),
            FmtKind::I16 => out.extend_from_slice(
                crate::fmt::fmt_i32(i32::from(unsafe { p.cast::<i16>().read_unaligned() }))
                    .as_bytes(),
            ),
            FmtKind::U16 => out.extend_from_slice(
                crate::fmt::fmt_u32(u32::from(unsafe { p.cast::<u16>().read_unaligned() }))
                    .as_bytes(),
            ),
            FmtKind::F16 => out.extend_from_slice(
                crate::fmt::fmt_f64(crate::half::to_f64(unsafe {
                    p.cast::<u16>().read_unaligned()
                }))
                .as_bytes(),
            ),
            // Booleans are 1 byte under the dev JIT and 4 under the
            // ship-C emitter; read the tier's own width.
            FmtKind::Bool => out.extend_from_slice(
                crate::fmt::fmt_bool(unsafe { read_uint(p, esz) } != 0).as_bytes(),
            ),
            FmtKind::Str => {
                // SAFETY: an 8-byte string handle.
                let s = unsafe { p.cast::<*const u8>().read_unaligned() };
                if !s.is_null() {
                    // SAFETY: live string handle. Copied to keep the
                    // borrow away from the alloc below.
                    let bytes: Vec<u8> = unsafe { (*ctx).str_bytes(s) }.to_vec();
                    out.extend_from_slice(&bytes);
                }
            }
        }
    }
    // SAFETY: caller contract.
    unsafe { &mut *ctx }.alloc_str(&out, pos_id)
}

// ----- slice / fill / reverse / concat -----

/// JS slice-range clamp: a negative index counts from the end; the
/// result is clamped to `[0, len]`.
fn clamp_index(v: i32, len: usize) -> usize {
    let len = len as i64;
    let v = i64::from(v);
    let r = if v < 0 { (len + v).max(0) } else { v.min(len) };
    r as usize
}

/// `slice(start, end)`: a fresh array of the clamped range (JS negative
/// rules). Null only after an allocation-failure trap.
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null).
pub unsafe fn slice(ctx: *mut Context, h: *mut u8, start: i32, end: i32, pos_id: u32) -> *mut u8 {
    if h.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h), (*ctx).array_elem_size(h)) };
    let (lo, hi) = (clamp_index(start, n), clamp_index(end, n));
    // SAFETY: caller contract.
    let out = unsafe { &mut *ctx }.array_new(esz, pos_id);
    if out.is_null() {
        return std::ptr::null_mut();
    }
    for i in lo..hi {
        // SAFETY: `i < n`; caller contract.
        let p = unsafe { (*ctx).array_elem_ptr(h, i as i32, 0) };
        // SAFETY: `out` is a live `esz`-element array; `p` is readable
        // for `esz` bytes.
        if unsafe { (*ctx).array_push(out, p, pos_id) } < 0 {
            break; // allocation-failure trap: return the valid partial
        }
    }
    out
}

/// `fill(x, start, end)` in place (JS clamp rules). The receiver is the
/// expression's value; generated code reuses its handle.
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null); `x` is readable for the
/// element size.
pub unsafe fn fill(ctx: *mut Context, h: *mut u8, x: *const u8, start: i32, end: i32) {
    if h.is_null() || x.is_null() {
        return;
    }
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h), (*ctx).array_elem_size(h)) };
    let (lo, hi) = (clamp_index(start, n), clamp_index(end, n));
    for i in lo..hi {
        // SAFETY: `i < n`; caller contract.
        let p = unsafe { (*ctx).array_elem_ptr(h, i as i32, 0) };
        // SAFETY: `x` readable and `p` writable for `esz` bytes; the
        // fill value lives outside the array (a caller temp), so the
        // regions cannot overlap.
        unsafe { std::ptr::copy_nonoverlapping(x, p, esz) };
    }
}

/// `reverse()` in place. The receiver is the expression's value.
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null).
pub unsafe fn reverse(ctx: *mut Context, h: *mut u8) {
    if h.is_null() {
        return;
    }
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h), (*ctx).array_elem_size(h)) };
    let mut tmp = vec![0u8; esz];
    for i in 0..n / 2 {
        let j = n - 1 - i;
        // SAFETY: `i`, `j` in bounds; caller contract.
        let a = unsafe { (*ctx).array_elem_ptr(h, i as i32, 0) };
        // SAFETY: as above.
        let b = unsafe { (*ctx).array_elem_ptr(h, j as i32, 0) };
        // SAFETY: distinct in-bounds slots of `esz` bytes; `tmp` is an
        // `esz`-byte scratch.
        unsafe {
            std::ptr::copy_nonoverlapping(a, tmp.as_mut_ptr(), esz);
            std::ptr::copy_nonoverlapping(b, a, esz);
            std::ptr::copy_nonoverlapping(tmp.as_ptr(), b, esz);
        }
    }
}

/// `concat(other)`: a fresh array holding `a`'s elements then `b`'s.
/// Null only after an allocation-failure trap.
///
/// # Safety
///
/// `a` and `b` are live arrays of `ctx` (or null) with equal element
/// sizes (the checker guarantees the element types match).
pub unsafe fn concat(ctx: *mut Context, a: *mut u8, b: *mut u8, pos_id: u32) -> *mut u8 {
    if a.is_null() || b.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract.
    let esz = unsafe { (*ctx).array_elem_size(a) };
    // SAFETY: caller contract.
    if esz != unsafe { (*ctx).array_elem_size(b) } {
        // Version skew between checker and runtime, not a program fault.
        // SAFETY: caller contract.
        unsafe { &mut *ctx }.trap(
            TrapKind::Internal,
            "concat operands disagree on the element size",
            pos_id,
        );
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract.
    let out = unsafe { &mut *ctx }.array_new(esz, pos_id);
    if out.is_null() {
        return std::ptr::null_mut();
    }
    for src in [a, b] {
        // SAFETY: caller contract.
        let n = unsafe { len_of(ctx, src) };
        for i in 0..n {
            // SAFETY: `i < n`; caller contract.
            let p = unsafe { (*ctx).array_elem_ptr(src, i as i32, 0) };
            // SAFETY: matching element sizes.
            if unsafe { (*ctx).array_push(out, p, pos_id) } < 0 {
                return out; // valid partial after an allocation trap
            }
        }
    }
    out
}

// ----- callback operations -----

/// True when the operation must not (or no longer) run script code:
/// the Context is already trapped, or the inputs are null (post-trap
/// ship-C execution).
///
/// # Safety
///
/// `ctx` is the live Context.
unsafe fn cb_blocked(ctx: *mut Context, h: *mut u8, code: *const u8) -> bool {
    // SAFETY: caller contract.
    h.is_null() || code.is_null() || unsafe { (*ctx).trapped() }
}

/// `forEach(f)`: calls `f(v)` per element in index order; aborts on the
/// first trap.
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null); `code`/`env` are a language
/// function value of shape `(ctx, env, T) -> void` for the element ABI.
pub unsafe fn for_each(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
) {
    // SAFETY: caller contract.
    if unsafe { cb_blocked(ctx, h, code) } {
        return;
    }
    // SAFETY: caller contract.
    let esz = unsafe { (*ctx).array_elem_size(h) };
    // SAFETY: caller contract.
    let Some(abi) = (unsafe { abi_or_trap(ctx, kind, esz) }) else {
        return;
    };
    // SAFETY: caller contract.
    let n = unsafe { len_of(ctx, h) };
    with_abi!(abi, T, {
        for i in 0..n {
            // A callback may mutate the array; stop at the current end.
            // SAFETY: caller contract.
            if unsafe { len_of(ctx, h) } <= i {
                break;
            }
            // SAFETY: `i` in bounds; `size_of::<T>() == esz` by dispatch.
            let v: T = unsafe { read_elem(ctx, h, i) };
            // SAFETY: callback contract (caller).
            let () = unsafe { call1::<T, ()>(code, ctx, env, v) };
            // SAFETY: caller contract.
            if unsafe { (*ctx).trapped() } {
                break;
            }
        }
    });
}

/// `map(f)`: a fresh array of `f(v)` results (`ret_size`-byte elements
/// of `ret_kind`). A mid-iteration callback trap aborts and returns the
/// valid partial array (generated code surfaces the trap before any
/// use).
///
/// # Safety
///
/// As [`for_each`], with callback shape `(ctx, env, T) -> R` for the
/// element and result ABIs.
pub unsafe fn map(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    elem_kind: ElemKind,
    ret_kind: ElemKind,
    ret_size: usize,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: caller contract.
    if unsafe { cb_blocked(ctx, h, code) } {
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract.
    let esz = unsafe { (*ctx).array_elem_size(h) };
    // SAFETY: caller contract.
    let (Some(ea), Some(ra)) = (unsafe { abi_or_trap(ctx, elem_kind, esz) }, unsafe {
        abi_or_trap(ctx, ret_kind, ret_size)
    }) else {
        return std::ptr::null_mut();
    };
    // SAFETY: caller contract.
    let out = unsafe { &mut *ctx }.array_new(ret_size, pos_id);
    if out.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract.
    let n = unsafe { len_of(ctx, h) };
    with_abi!(ea, T, {
        with_abi!(ra, R, {
            for i in 0..n {
                // SAFETY: caller contract.
                if unsafe { len_of(ctx, h) } <= i {
                    break;
                }
                // SAFETY: `i` in bounds; widths match by dispatch.
                let v: T = unsafe { read_elem(ctx, h, i) };
                // SAFETY: callback contract (caller).
                let r: R = unsafe { call1::<T, R>(code, ctx, env, v) };
                // SAFETY: caller contract.
                if unsafe { (*ctx).trapped() } {
                    break;
                }
                // SAFETY: `out` is a live `ret_size`-element array;
                // `size_of::<R>() == ret_size` by dispatch.
                if unsafe {
                    (*ctx).array_push(out, (&r as *const R).cast::<u8>(), pos_id)
                } < 0
                {
                    break;
                }
            }
        })
    });
    out
}

/// `filter(f)`: a fresh array of the elements whose predicate returned
/// `true` (the pre-callback value, JS semantics). Trap-abort as
/// [`map`].
///
/// # Safety
///
/// As [`for_each`], with callback shape `(ctx, env, T) -> boolean`.
pub unsafe fn filter(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: caller contract.
    if unsafe { cb_blocked(ctx, h, code) } {
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract.
    let esz = unsafe { (*ctx).array_elem_size(h) };
    // SAFETY: caller contract.
    let Some(abi) = (unsafe { abi_or_trap(ctx, kind, esz) }) else {
        return std::ptr::null_mut();
    };
    // SAFETY: caller contract.
    let out = unsafe { &mut *ctx }.array_new(esz, pos_id);
    if out.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract.
    let n = unsafe { len_of(ctx, h) };
    with_abi!(abi, T, {
        for i in 0..n {
            // SAFETY: caller contract.
            if unsafe { len_of(ctx, h) } <= i {
                break;
            }
            // SAFETY: `i` in bounds; widths match by dispatch.
            let v: T = unsafe { read_elem(ctx, h, i) };
            // SAFETY: callback contract (caller). A language boolean
            // returns as its low byte on both tiers.
            let keep: u8 = unsafe { call1::<T, u8>(code, ctx, env, v) };
            // SAFETY: caller contract.
            if unsafe { (*ctx).trapped() } {
                break;
            }
            if keep != 0 {
                // SAFETY: `v` is `esz` bytes by dispatch.
                if unsafe {
                    (*ctx).array_push(out, (&v as *const T).cast::<u8>(), pos_id)
                } < 0
                {
                    break;
                }
            }
        }
    });
    out
}

/// `reduce(f, init)`: folds left with the accumulator traveling in/out
/// through `acc_ptr` (its C-ABI class is `acc_kind` at `acc_size`
/// bytes). On a callback trap the last completed accumulator remains in
/// `acc_ptr`.
///
/// # Safety
///
/// As [`for_each`], with callback shape `(ctx, env, A, T) -> A`;
/// `acc_ptr` is readable and writable for `acc_size` bytes.
pub unsafe fn reduce(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    elem_kind: ElemKind,
    acc_kind: ElemKind,
    acc_size: usize,
    acc_ptr: *mut u8,
) {
    if acc_ptr.is_null() {
        return;
    }
    // SAFETY: caller contract.
    if unsafe { cb_blocked(ctx, h, code) } {
        return;
    }
    // SAFETY: caller contract.
    let esz = unsafe { (*ctx).array_elem_size(h) };
    // SAFETY: caller contract.
    let (Some(ea), Some(aa)) = (unsafe { abi_or_trap(ctx, elem_kind, esz) }, unsafe {
        abi_or_trap(ctx, acc_kind, acc_size)
    }) else {
        return;
    };
    // SAFETY: caller contract.
    let n = unsafe { len_of(ctx, h) };
    with_abi!(ea, T, {
        with_abi!(aa, A, {
            // SAFETY: `acc_ptr` readable for `acc_size == size_of::<A>()`.
            let mut acc: A = unsafe { acc_ptr.cast::<A>().read_unaligned() };
            for i in 0..n {
                // SAFETY: caller contract.
                if unsafe { len_of(ctx, h) } <= i {
                    break;
                }
                // SAFETY: `i` in bounds; widths match by dispatch.
                let v: T = unsafe { read_elem(ctx, h, i) };
                // SAFETY: callback contract (caller).
                let r: A = unsafe { call2::<A, T, A>(code, ctx, env, acc, v) };
                // SAFETY: caller contract.
                if unsafe { (*ctx).trapped() } {
                    break; // keep the last completed accumulator
                }
                acc = r;
            }
            // SAFETY: `acc_ptr` writable for `size_of::<A>()`.
            unsafe { acc_ptr.cast::<A>().write_unaligned(acc) };
        })
    });
}

/// Search mode of [`search`]: what the predicate result decides.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SearchMode {
    /// `some`: stop on the first `true`; result 1/0.
    Some,
    /// `every`: stop on the first `false`; result 1/0.
    Every,
    /// `findIndex`: stop on the first `true`; result index or −1.
    FindIndex,
}

/// Shared short-circuiting predicate loop. Returns the defensible
/// constant (0 / 0 / −1) on trap.
///
/// # Safety
///
/// As [`for_each`], with callback shape `(ctx, env, T) -> boolean`.
unsafe fn search(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
    mode: SearchMode,
) -> i32 {
    let miss = match mode {
        SearchMode::Some => 0,
        SearchMode::Every => 1,
        SearchMode::FindIndex => -1,
    };
    let trapped_result = if mode == SearchMode::FindIndex { -1 } else { 0 };
    // SAFETY: caller contract.
    if unsafe { cb_blocked(ctx, h, code) } {
        return trapped_result;
    }
    // SAFETY: caller contract.
    let esz = unsafe { (*ctx).array_elem_size(h) };
    // SAFETY: caller contract.
    let Some(abi) = (unsafe { abi_or_trap(ctx, kind, esz) }) else {
        return trapped_result;
    };
    // SAFETY: caller contract.
    let n = unsafe { len_of(ctx, h) };
    with_abi!(abi, T, {
        for i in 0..n {
            // SAFETY: caller contract.
            if unsafe { len_of(ctx, h) } <= i {
                break;
            }
            // SAFETY: `i` in bounds; widths match by dispatch.
            let v: T = unsafe { read_elem(ctx, h, i) };
            // SAFETY: callback contract (caller).
            let r: u8 = unsafe { call1::<T, u8>(code, ctx, env, v) };
            // SAFETY: caller contract.
            if unsafe { (*ctx).trapped() } {
                return trapped_result;
            }
            match mode {
                SearchMode::Some if r != 0 => return 1,
                SearchMode::Every if r == 0 => return 0,
                SearchMode::FindIndex if r != 0 => return i as i32,
                _ => {}
            }
        }
    });
    miss
}

/// `some(f)`: 1 when any element satisfies `f` (short-circuits), else 0.
///
/// # Safety
///
/// As [`for_each`], with callback shape `(ctx, env, T) -> boolean`.
pub unsafe fn some(ctx: *mut Context, h: *mut u8, code: *const u8, env: *const u8, kind: ElemKind) -> i32 {
    // SAFETY: forwarded contract.
    unsafe { search(ctx, h, code, env, kind, SearchMode::Some) }
}

/// `every(f)`: 1 when every element satisfies `f` (short-circuits on
/// the first miss), else 0.
///
/// # Safety
///
/// As [`some`].
pub unsafe fn every(ctx: *mut Context, h: *mut u8, code: *const u8, env: *const u8, kind: ElemKind) -> i32 {
    // SAFETY: forwarded contract.
    unsafe { search(ctx, h, code, env, kind, SearchMode::Every) }
}

/// `findIndex(f)`: the first satisfying index (short-circuits), or −1.
///
/// # Safety
///
/// As [`some`].
pub unsafe fn find_index(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
) -> i32 {
    // SAFETY: forwarded contract.
    unsafe { search(ctx, h, code, env, kind, SearchMode::FindIndex) }
}

/// Stable bottom-up merge sort of `a` using the script comparator;
/// `false` when a comparator call trapped (the caller then discards the
/// buffer). Stability: the merge takes from the left run when
/// `cmp(left, right) <= 0` (JS: `right` precedes only when the
/// comparator is positive).
///
/// # Safety
///
/// `code`/`env` are a comparator of shape `(ctx, env, T, T) -> i32`
/// (caller contract, as [`sort`]).
unsafe fn merge_sort_by<T: Copy>(
    ctx: *mut Context,
    code: *const u8,
    env: *const u8,
    a: &mut Vec<T>,
    tmp: &mut Vec<T>,
) -> bool {
    let n = a.len();
    let mut width = 1;
    while width < n {
        let mut lo = 0;
        while lo < n {
            let mid = (lo + width).min(n);
            let hi = (lo + 2 * width).min(n);
            let (mut l, mut r, mut k) = (lo, mid, lo);
            while l < mid && r < hi {
                // SAFETY: comparator contract (caller).
                let c: i32 = unsafe { call2::<T, T, i32>(code, ctx, env, a[l], a[r]) };
                // SAFETY: caller contract.
                if unsafe { (*ctx).trapped() } {
                    return false;
                }
                if c <= 0 {
                    tmp[k] = a[l];
                    l += 1;
                } else {
                    tmp[k] = a[r];
                    r += 1;
                }
                k += 1;
            }
            while l < mid {
                tmp[k] = a[l];
                l += 1;
                k += 1;
            }
            while r < hi {
                tmp[k] = a[r];
                r += 1;
                k += 1;
            }
            lo += 2 * width;
        }
        std::mem::swap(a, tmp);
        width *= 2;
    }
    true
}

/// `sort(cmp)`: stable merge sort in place. The sort runs on a scratch
/// buffer and writes back **only on completion**, so a comparator trap
/// leaves the array exactly as it was (trivially a permutation of its
/// input). The write-back is also skipped if a comparator changed the
/// array's length (the buffer no longer describes the array).
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null); `code`/`env` are a language
/// comparator of shape `(ctx, env, T, T) -> i32` for the element ABI.
pub unsafe fn sort(ctx: *mut Context, h: *mut u8, code: *const u8, env: *const u8, kind: ElemKind) {
    // SAFETY: caller contract.
    if unsafe { cb_blocked(ctx, h, code) } {
        return;
    }
    // SAFETY: caller contract.
    let esz = unsafe { (*ctx).array_elem_size(h) };
    // SAFETY: caller contract.
    let Some(abi) = (unsafe { abi_or_trap(ctx, kind, esz) }) else {
        return;
    };
    // SAFETY: caller contract.
    let n = unsafe { len_of(ctx, h) };
    if n < 2 {
        return;
    }
    with_abi!(abi, T, {
        let mut buf: Vec<T> = Vec::with_capacity(n);
        for i in 0..n {
            // SAFETY: `i < n`; widths match by dispatch.
            buf.push(unsafe { read_elem::<T>(ctx, h, i) });
        }
        let mut tmp = buf.clone();
        // SAFETY: comparator contract (caller).
        if !unsafe { merge_sort_by::<T>(ctx, code, env, &mut buf, &mut tmp) } {
            return; // comparator trapped: the array is untouched
        }
        // SAFETY: caller contract.
        if unsafe { len_of(ctx, h) } != n {
            return; // a comparator resized the array: keep it as-is
        }
        for (i, v) in buf.iter().enumerate() {
            // SAFETY: `i < n`; element slots are `esz` writable bytes.
            let p = unsafe { (*ctx).array_elem_ptr(h, i as i32, 0) };
            // SAFETY: as above; `size_of::<T>() == esz` by dispatch.
            unsafe { p.cast::<T>().write_unaligned(*v) };
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh dev-policy context.
    fn ctx() -> Box<Context> {
        Context::new()
    }

    /// Builds an `i32[]` from `values`.
    fn arr_i32(ctx: &mut Context, values: &[i32]) -> *mut u8 {
        let h = ctx.array_new(4, 0);
        assert!(!h.is_null());
        for v in values {
            // SAFETY: `h` is a live 4-byte-element array; `v` readable.
            unsafe { ctx.array_push(h, (v as *const i32).cast(), 0) };
        }
        h
    }

    /// Builds an `f64[]` from `values`.
    fn arr_f64(ctx: &mut Context, values: &[f64]) -> *mut u8 {
        let h = ctx.array_new(8, 0);
        assert!(!h.is_null());
        for v in values {
            // SAFETY: as `arr_i32`, with 8-byte elements.
            unsafe { ctx.array_push(h, (v as *const f64).cast(), 0) };
        }
        h
    }

    /// Builds a `string[]` from `values`.
    fn arr_str(ctx: &mut Context, values: &[&str]) -> *mut u8 {
        let h = ctx.array_new(8, 0);
        assert!(!h.is_null());
        for v in values {
            let s = ctx.alloc_str(v.as_bytes(), 0) as u64;
            // SAFETY: as `arr_i32`, with 8-byte handle elements.
            unsafe { ctx.array_push(h, (&s as *const u64).cast(), 0) };
        }
        h
    }

    fn i32_items(ctx: &Context, h: *const u8) -> Vec<i32> {
        // SAFETY: `h` is a live 4-byte-element array of `ctx`.
        unsafe {
            let n = ctx.array_len(h) as usize;
            let data = ctx.array_data(h);
            (0..n).map(|i| data.add(i * 4).cast::<i32>().read_unaligned()).collect()
        }
    }

    unsafe extern "C" fn double_i32(_ctx: *mut Context, _env: *const u8, v: i32) -> i32 {
        v * 2
    }

    unsafe extern "C" fn i32_to_f64(_ctx: *mut Context, _env: *const u8, v: i32) -> f64 {
        f64::from(v) + 0.5
    }

    unsafe extern "C" fn is_even(_ctx: *mut Context, _env: *const u8, v: i32) -> u8 {
        u8::from(v % 2 == 0)
    }

    unsafe extern "C" fn add_into_env(_ctx: *mut Context, env: *const u8, v: i32) {
        // SAFETY: the test passes a pointer to an i64 accumulator.
        unsafe { *(env as *mut i64) += i64::from(v) };
    }

    unsafe extern "C" fn count_and_test(_ctx: *mut Context, env: *const u8, v: i32) -> u8 {
        // SAFETY: the test passes a pointer to an i64 counter.
        unsafe { *(env as *mut i64) += 1 };
        u8::from(v >= 3)
    }

    unsafe extern "C" fn sum_f64_i32(_ctx: *mut Context, _env: *const u8, acc: f64, v: i32) -> f64 {
        acc + f64::from(v)
    }

    unsafe extern "C" fn cmp_i32(_ctx: *mut Context, _env: *const u8, a: i32, b: i32) -> i32 {
        a - b
    }

    /// Comparator over key-encoded pairs (`key * 10 + seq`): orders by
    /// key only, so the seq digits expose stability.
    unsafe extern "C" fn cmp_key(_ctx: *mut Context, _env: *const u8, a: i32, b: i32) -> i32 {
        a / 10 - b / 10
    }

    unsafe extern "C" fn trap_on_two(ctx: *mut Context, _env: *const u8, v: i32) -> i32 {
        if v == 2 {
            // SAFETY: `ctx` is the live test context.
            unsafe { &mut *ctx }.trap(TrapKind::IndexOutOfBounds, "test trap", 42);
        }
        v
    }

    unsafe extern "C" fn cmp_trapping(ctx: *mut Context, _env: *const u8, a: i32, b: i32) -> i32 {
        // SAFETY: `ctx` is the live test context.
        unsafe { &mut *ctx }.trap(TrapKind::IndexOutOfBounds, "cmp trap", 7);
        a - b
    }

    #[test]
    fn elem_and_fmt_kind_codes_round_trip() {
        for (v, k) in [
            (0, ElemKind::Int),
            (1, ElemKind::F32),
            (2, ElemKind::F64),
            (3, ElemKind::Str),
            (4, ElemKind::F16),
            (5, ElemKind::SignedInt),
        ] {
            assert_eq!(ElemKind::from_u32(v), Some(k));
        }
        assert_eq!(ElemKind::from_u32(9), None);
        for (v, k) in [
            (0, FmtKind::I32),
            (1, FmtKind::U32),
            (2, FmtKind::I64),
            (3, FmtKind::U64),
            (4, FmtKind::F32),
            (5, FmtKind::F64),
            (6, FmtKind::Bool),
            (7, FmtKind::Str),
            (8, FmtKind::I8),
            (9, FmtKind::U8),
            (10, FmtKind::I16),
            (11, FmtKind::U16),
            (12, FmtKind::F16),
        ] {
            assert_eq!(FmtKind::from_u32(v), Some(k));
        }
        assert_eq!(FmtKind::from_u32(13), None);
    }

    #[test]
    fn abi_dispatch_is_kind_plus_width() {
        assert_eq!(abi_of(ElemKind::Int, 1), Some(Abi::U8));
        assert_eq!(abi_of(ElemKind::Int, 2), Some(Abi::U16));
        assert_eq!(abi_of(ElemKind::Int, 4), Some(Abi::U32));
        assert_eq!(abi_of(ElemKind::Int, 8), Some(Abi::U64));
        assert_eq!(abi_of(ElemKind::SignedInt, 1), Some(Abi::S8));
        assert_eq!(abi_of(ElemKind::SignedInt, 2), Some(Abi::S16));
        assert_eq!(abi_of(ElemKind::SignedInt, 4), Some(Abi::I32));
        assert_eq!(abi_of(ElemKind::SignedInt, 8), Some(Abi::I64));
        assert_eq!(abi_of(ElemKind::Str, 8), Some(Abi::U64));
        assert_eq!(abi_of(ElemKind::F16, 2), Some(Abi::U16));
        assert_eq!(abi_of(ElemKind::F32, 4), Some(Abi::F32));
        assert_eq!(abi_of(ElemKind::F64, 8), Some(Abi::F64));
        assert_eq!(abi_of(ElemKind::Int, 3), None);
        assert_eq!(abi_of(ElemKind::F32, 8), None);
        assert_eq!(abi_of(ElemKind::F64, 4), None);
        assert_eq!(abi_of(ElemKind::F16, 4), None);
    }

    #[test]
    fn index_of_family_int_and_float_and_str() {
        let mut c = ctx();
        let p: *mut Context = &mut *c;
        let h = arr_i32(&mut c, &[4, 7, 4, 9]);
        let (n4, n5, n7) = (4i32, 5i32, 7i32);
        let needle = |v: &i32| (v as *const i32).cast::<u8>();
        // SAFETY: live arrays/handles of `c`; needles readable.
        unsafe {
            assert_eq!(index_of(p, h, needle(&n4), ElemKind::Int), 0);
            assert_eq!(last_index_of(p, h, needle(&n4), ElemKind::Int), 2);
            assert_eq!(index_of(p, h, needle(&n5), ElemKind::Int), -1);
            assert_eq!(includes(p, h, needle(&n7), ElemKind::Int), 1);
            assert_eq!(includes(p, h, needle(&n5), ElemKind::Int), 0);

            // Floats: -0 == 0; NaN never equal (=== semantics, Q22).
            let f = arr_f64(&mut c, &[0.0, 1.5, f64::NAN]);
            let nz = -0.0f64;
            assert_eq!(index_of(p, f, (&nz as *const f64).cast(), ElemKind::F64), 0);
            let nan = f64::NAN;
            assert_eq!(index_of(p, f, (&nan as *const f64).cast(), ElemKind::F64), -1);
            assert_eq!(includes(p, f, (&nan as *const f64).cast(), ElemKind::F64), 0);

            // Strings by content: a distinct allocation still matches.
            let s = arr_str(&mut c, &["alpha", "beta"]);
            let fresh = c.alloc_str(b"beta", 0) as u64;
            assert_eq!(
                index_of(p, s, (&fresh as *const u64).cast(), ElemKind::Str),
                1
            );
            let miss = c.alloc_str(b"gamma", 0) as u64;
            assert_eq!(
                index_of(p, s, (&miss as *const u64).cast(), ElemKind::Str),
                -1
            );
        }
    }

    #[test]
    fn join_formats_per_kind_with_separator() {
        let mut c = ctx();
        let p: *mut Context = &mut *c;
        let sep = c.alloc_str(b", ", 0);
        // SAFETY: live arrays/handles of `c`.
        unsafe {
            let h = arr_i32(&mut c, &[1, -2, 3]);
            let out = join(p, h, sep, FmtKind::I32, 0);
            assert_eq!(c.str_bytes(out), b"1, -2, 3");

            let f = arr_f64(&mut c, &[0.5, -0.0, 7.0]);
            let out = join(p, f, sep, FmtKind::F64, 0);
            assert_eq!(c.str_bytes(out), b"0.5, -0, 7");

            let i8s = c.array_new(1, 0);
            for v in [-128i8, 127i8] {
                c.array_push(i8s, (&v as *const i8).cast(), 0);
            }
            let out = join(p, i8s, sep, FmtKind::I8, 0);
            assert_eq!(c.str_bytes(out), b"-128, 127");

            let u16s = c.array_new(2, 0);
            for v in [0u16, u16::MAX] {
                c.array_push(u16s, (&v as *const u16).cast(), 0);
            }
            let out = join(p, u16s, sep, FmtKind::U16, 0);
            assert_eq!(c.str_bytes(out), b"0, 65535");

            let halves = c.array_new(2, 0);
            for v in [1.5, -0.0] {
                let bits = crate::half::from_f64(v);
                c.array_push(halves, (&bits as *const u16).cast(), 0);
            }
            let out = join(p, halves, sep, FmtKind::F16, 0);
            assert_eq!(c.str_bytes(out), b"1.5, -0");

            let s = arr_str(&mut c, &["a", "b"]);
            let out = join(p, s, sep, FmtKind::Str, 0);
            assert_eq!(c.str_bytes(out), b"a, b");

            // Booleans at width 1 (dev tier) and width 4 (ship-C tier).
            let b1 = c.array_new(1, 0);
            for v in [1u8, 0u8] {
                c.array_push(b1, (&v as *const u8).cast(), 0);
            }
            let out = join(p, b1, sep, FmtKind::Bool, 0);
            assert_eq!(c.str_bytes(out), b"true, false");
            let b4 = arr_i32(&mut c, &[0, 1]);
            let out = join(p, b4, sep, FmtKind::Bool, 0);
            assert_eq!(c.str_bytes(out), b"false, true");

            // Empty array joins to "".
            let e = arr_i32(&mut c, &[]);
            let out = join(p, e, sep, FmtKind::I32, 0);
            assert_eq!(c.str_bytes(out), b"");
        }
    }

    #[test]
    fn slice_applies_js_negative_and_clamp_rules() {
        let mut c = ctx();
        let p: *mut Context = &mut *c;
        let h = arr_i32(&mut c, &[10, 20, 30, 40, 50]);
        // SAFETY: live arrays of `c`.
        unsafe {
            assert_eq!(i32_items(&c, slice(p, h, 1, 3, 0)), vec![20, 30]);
            assert_eq!(i32_items(&c, slice(p, h, -2, i32::MAX, 0)), vec![40, 50]);
            assert_eq!(i32_items(&c, slice(p, h, 0, -1, 0)), vec![10, 20, 30, 40]);
            assert_eq!(i32_items(&c, slice(p, h, 3, 99, 0)), vec![40, 50]);
            assert_eq!(i32_items(&c, slice(p, h, 4, 2, 0)), Vec::<i32>::new());
            // The receiver is untouched.
            assert_eq!(i32_items(&c, h), vec![10, 20, 30, 40, 50]);
        }
    }

    #[test]
    fn fill_reverse_concat() {
        let mut c = ctx();
        let p: *mut Context = &mut *c;
        // SAFETY: live arrays of `c`; fill values readable.
        unsafe {
            let h = arr_i32(&mut c, &[1, 2, 3, 4, 5]);
            let z = 0i32;
            fill(p, h, (&z as *const i32).cast(), 1, 3);
            assert_eq!(i32_items(&c, h), vec![1, 0, 0, 4, 5]);
            let s = 7i32;
            fill(p, h, (&s as *const i32).cast(), -2, i32::MAX);
            assert_eq!(i32_items(&c, h), vec![1, 0, 0, 7, 7]);

            let r = arr_i32(&mut c, &[1, 2, 3, 4]);
            reverse(p, r);
            assert_eq!(i32_items(&c, r), vec![4, 3, 2, 1]);
            let odd = arr_i32(&mut c, &[1, 2, 3]);
            reverse(p, odd);
            assert_eq!(i32_items(&c, odd), vec![3, 2, 1]);

            let a = arr_i32(&mut c, &[1, 2]);
            let b = arr_i32(&mut c, &[3]);
            let cat = concat(p, a, b, 0);
            assert_eq!(i32_items(&c, cat), vec![1, 2, 3]);
            assert_eq!(i32_items(&c, a), vec![1, 2]);
            assert_eq!(i32_items(&c, b), vec![3]);
        }
    }

    #[test]
    fn for_each_and_map_and_filter() {
        let mut c = ctx();
        let p: *mut Context = &mut *c;
        // SAFETY: live arrays of `c`; callbacks match the dispatched ABI.
        unsafe {
            let h = arr_i32(&mut c, &[1, 2, 3]);
            let mut acc: i64 = 0;
            for_each(
                p,
                h,
                add_into_env as *const u8,
                (&mut acc as *mut i64).cast(),
                ElemKind::Int,
            );
            assert_eq!(acc, 6);

            let doubled = map(
                p,
                h,
                double_i32 as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                ElemKind::Int,
                4,
                0,
            );
            assert_eq!(i32_items(&c, doubled), vec![2, 4, 6]);

            // Type-changing map: i32 -> f64.
            let floats = map(
                p,
                h,
                i32_to_f64 as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                ElemKind::F64,
                8,
                0,
            );
            assert_eq!(c.array_len(floats), 3);
            let d = c.array_data(floats);
            assert_eq!(d.cast::<f64>().read_unaligned(), 1.5);

            let evens = filter(p, h, is_even as *const u8, std::ptr::null(), ElemKind::Int, 0);
            assert_eq!(i32_items(&c, evens), vec![2]);
        }
    }

    #[test]
    fn reduce_with_differing_accumulator_kind() {
        let mut c = ctx();
        let p: *mut Context = &mut *c;
        // SAFETY: live array of `c`; callback matches the dispatched ABI.
        unsafe {
            let h = arr_i32(&mut c, &[1, 2, 3]);
            let mut acc: f64 = 100.0;
            reduce(
                p,
                h,
                sum_f64_i32 as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                ElemKind::F64,
                8,
                (&mut acc as *mut f64).cast(),
            );
            assert_eq!(acc, 106.0);
        }
    }

    #[test]
    fn some_every_find_index_short_circuit() {
        let mut c = ctx();
        let p: *mut Context = &mut *c;
        // SAFETY: live array of `c`; callback matches the dispatched ABI.
        unsafe {
            let h = arr_i32(&mut c, &[1, 2, 3, 4, 5]);
            let mut probes: i64 = 0;
            let env = (&mut probes as *mut i64).cast::<u8>();
            assert_eq!(some(p, h, count_and_test as *const u8, env, ElemKind::Int), 1);
            assert_eq!(probes, 3); // stopped at the first hit (v == 3)
            probes = 0;
            assert_eq!(every(p, h, count_and_test as *const u8, env, ElemKind::Int), 0);
            assert_eq!(probes, 1); // stopped at the first miss (v == 1)
            assert_eq!(
                find_index(p, h, is_even as *const u8, std::ptr::null(), ElemKind::Int),
                1
            );
            let odd = arr_i32(&mut c, &[1, 3]);
            assert_eq!(
                find_index(p, odd, is_even as *const u8, std::ptr::null(), ElemKind::Int),
                -1
            );
        }
    }

    #[test]
    fn sort_is_stable_and_ascending() {
        let mut c = ctx();
        let p: *mut Context = &mut *c;
        // SAFETY: live arrays of `c`; comparators match the ABI.
        unsafe {
            let h = arr_i32(&mut c, &[5, 1, 4, 2, 3]);
            sort(p, h, cmp_i32 as *const u8, std::ptr::null(), ElemKind::Int);
            assert_eq!(i32_items(&c, h), vec![1, 2, 3, 4, 5]);

            // Stability: key*10+seq pairs ordered by key keep seq order.
            let s = arr_i32(&mut c, &[20, 11, 22, 13, 14]);
            sort(p, s, cmp_key as *const u8, std::ptr::null(), ElemKind::Int);
            assert_eq!(i32_items(&c, s), vec![11, 13, 14, 20, 22]);
        }
    }

    #[test]
    fn callback_trap_aborts_and_leaves_defensible_state() {
        let mut c = ctx();
        let p: *mut Context = &mut *c;
        // SAFETY: live arrays of `c`; callbacks match the ABI.
        unsafe {
            // map: the trap at v == 2 keeps element 1 only.
            let h = arr_i32(&mut c, &[1, 2, 3]);
            let out = map(
                p,
                h,
                trap_on_two as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                ElemKind::Int,
                4,
                0,
            );
            assert!(c.trapped());
            assert_eq!(i32_items(&c, out), vec![1]);
            let rec = c.trap_record().expect("trap recorded");
            assert_eq!(rec.pos_id, 42);
            // Already trapped: callback entries do not run script code.
            let out2 = map(
                p,
                h,
                double_i32 as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                ElemKind::Int,
                4,
                0,
            );
            assert!(out2.is_null());
            c.clear_trap();

            // sort: a comparator trap leaves the array exactly as it was.
            let s = arr_i32(&mut c, &[3, 1, 2]);
            sort(p, s, cmp_trapping as *const u8, std::ptr::null(), ElemKind::Int);
            assert!(c.trapped());
            assert_eq!(i32_items(&c, s), vec![3, 1, 2]);
            c.clear_trap();

            // reduce: the last completed accumulator survives the trap.
            let r = arr_i32(&mut c, &[1, 2, 3]);
            let mut acc: f64 = 0.0;
            reduce(
                p,
                r,
                sum_and_trap_on_two as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                ElemKind::F64,
                8,
                (&mut acc as *mut f64).cast(),
            );
            assert!(c.trapped());
            assert_eq!(acc, 1.0);
            c.clear_trap();
        }
    }

    unsafe extern "C" fn sum_and_trap_on_two(
        ctx: *mut Context,
        _env: *const u8,
        acc: f64,
        v: i32,
    ) -> f64 {
        if v == 2 {
            // SAFETY: `ctx` is the live test context.
            unsafe { &mut *ctx }.trap(TrapKind::IndexOutOfBounds, "reduce trap", 9);
        }
        acc + f64::from(v)
    }

    #[test]
    fn unknown_abi_shape_traps_in_the_equality_searches() {
        // P11 review MINOR 2: the searches validate the element shape
        // exactly as the callback entries do. Two 3-byte elements have
        // no ABI class; comparing them as zero-extended words would
        // report them *equal*, so the entry traps and misses instead.
        let mut c = ctx();
        let p: *mut Context = &mut *c;
        let h = c.array_new(3, 0);
        let (a, b) = ([1u8, 2, 3], [4u8, 5, 6]);
        // SAFETY: live 3-byte-element array; sources readable for 3 bytes.
        unsafe {
            c.array_push(h, a.as_ptr(), 0);
            c.array_push(h, b.as_ptr(), 0);
            assert_eq!(index_of(p, h, b.as_ptr(), ElemKind::Int), -1);
        }
        assert!(c.trapped());
        assert_eq!(c.trap_record().map(|r| r.kind), Some(TrapKind::Internal));
        c.clear_trap();
        // SAFETY: as above.
        unsafe {
            assert_eq!(last_index_of(p, h, a.as_ptr(), ElemKind::Int), -1);
            assert_eq!(includes(p, h, a.as_ptr(), ElemKind::Int), 0);
        }
        assert!(c.trapped());
        assert_eq!(c.trap_record().map(|r| r.kind), Some(TrapKind::Internal));
    }

    #[test]
    fn unknown_abi_shape_traps_internal() {
        let mut c = ctx();
        let p: *mut Context = &mut *c;
        // A 3-byte element has no ABI class; the entry must trap
        // Internal instead of guessing.
        let h = c.array_new(3, 0);
        // SAFETY: live array of `c`.
        unsafe {
            for_each(p, h, double_i32 as *const u8, std::ptr::null(), ElemKind::Int);
        }
        assert!(c.trapped());
        assert_eq!(c.trap_record().map(|r| r.kind), Some(TrapKind::Internal));
    }
}
