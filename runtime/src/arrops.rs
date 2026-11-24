//! `Array` method operations (stdlib.md §9, Q22): the one shared
//! implementation both execution tiers call through the `subscript_rt_arr_*`
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

use crate::context::{ArrayElements, Context};
use crate::trap::TrapKind;
use crate::valeq::{read_uint, value_eq, ValueKind};

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
/// never produce.
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

mod callback;
mod sort;
pub use callback::{
    every, filter, find, find_index, find_last_index, flat_map, for_each, map, reduce,
    reduce_right, some,
};
pub use sort::sort;

/// True when the operation must not run script code: the Context is
/// already trapped, or the inputs are null (post-trap ship-C
/// execution).
///
/// # Safety
///
/// `ctx` is the live Context.
unsafe fn cb_blocked(ctx: *mut Context, h: *mut u8, code: *const u8) -> bool {
    // SAFETY: caller contract.
    h.is_null() || code.is_null() || unsafe { (*ctx).trapped() }
}

/// Calls a one-value script callback `(ctx, env, v) -> R`.
///
/// # Safety
///
/// `code` is a language function value's code pointer whose C signature
/// is exactly `(ctx, env, A) -> R` after monomorphization; it never
/// unwinds (trap-flag discipline).
unsafe fn call1<A: Copy, R: Copy>(code: *const u8, ctx: *mut Context, env: *const u8, a: A) -> R {
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

/// Calls a three-value script callback `(ctx, env, a, b, c) -> R`.
///
/// # Safety
///
/// As [`call1`], with signature `(ctx, env, A, B, C) -> R`.
unsafe fn call3<A: Copy, B: Copy, C: Copy, R: Copy>(
    code: *const u8,
    ctx: *mut Context,
    env: *const u8,
    a: A,
    b: B,
    c: C,
) -> R {
    // SAFETY: as `call1`.
    let f: unsafe extern "C" fn(*mut Context, *const u8, A, B, C) -> R =
        unsafe { std::mem::transmute(code) };
    // SAFETY: caller contract.
    unsafe { f(ctx, env, a, b, c) }
}

/// Calls `f(value)` or `f(value, index)` according to its checked shape.
///
/// # Safety
///
/// `code` has the shape selected by `indexed`.
unsafe fn call_value<A: Copy, R: Copy>(
    code: *const u8,
    ctx: *mut Context,
    env: *const u8,
    value: A,
    index: i32,
    indexed: bool,
) -> R {
    if indexed {
        // SAFETY: caller contract selects the two-value callback shape.
        unsafe { call2::<A, i32, R>(code, ctx, env, value, index) }
    } else {
        // SAFETY: caller contract selects the one-value callback shape.
        unsafe { call1::<A, R>(code, ctx, env, value) }
    }
}

/// Calls `f(acc, value)` or `f(acc, value, index)` according to its
/// checked shape.
///
/// # Safety
///
/// `code` has the shape selected by `indexed`.
unsafe fn call_reduce<A: Copy, T: Copy>(
    code: *const u8,
    ctx: *mut Context,
    env: *const u8,
    acc: A,
    value: T,
    index: i32,
    indexed: bool,
) -> A {
    if indexed {
        // SAFETY: caller contract selects the three-value callback shape.
        unsafe { call3::<A, T, i32, A>(code, ctx, env, acc, value, index) }
    } else {
        // SAFETY: caller contract selects the two-value callback shape.
        unsafe { call2::<A, T, A>(code, ctx, env, acc, value) }
    }
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
unsafe fn len_of(ctx: *mut Context, h: *const u8, pos_id: u32) -> usize {
    if h.is_null() || !unsafe { &mut *ctx }.require_live_handle(h as usize, pos_id) {
        return 0;
    }
    // SAFETY: caller contract.
    unsafe { (*ctx).array_len(h) }.max(0) as usize
}

/// Element storage used by the shared callback loops.
///
/// A dynamic array re-resolves its length and storage on every visit.
#[derive(Clone, Copy)]
enum ElementSource {
    Dynamic(*mut u8),
}

impl ElementSource {
    /// True when this source or callback cannot enter script code.
    unsafe fn cb_blocked(self, ctx: *mut Context, code: *const u8) -> bool {
        code.is_null()
            || match self {
                ElementSource::Dynamic(handle) => handle.is_null(),
            }
            // SAFETY: caller contract.
            || unsafe { (*ctx).trapped() }
    }

    /// Returns the handle fact of the source (compiler.md §344 rule 4).
    unsafe fn elements(self, ctx: *mut Context) -> ArrayElements {
        match self {
            // SAFETY: caller contract.
            ElementSource::Dynamic(handle) => unsafe { (*ctx).array_elements(handle) },
        }
    }

    /// Returns the source element width.
    unsafe fn elem_size(self, ctx: *mut Context) -> usize {
        match self {
            // SAFETY: caller contract.
            ElementSource::Dynamic(handle) => unsafe { (*ctx).array_elem_size(handle) },
        }
    }

    /// Returns the source length for the current iteration.
    unsafe fn len(self, ctx: *mut Context, pos_id: u32) -> usize {
        match self {
            // SAFETY: caller contract.
            ElementSource::Dynamic(handle) => unsafe { len_of(ctx, handle, pos_id) },
        }
    }

    /// Reads one element after the current length check.
    unsafe fn read<T: Copy>(self, ctx: *mut Context, index: usize) -> T {
        match self {
            // SAFETY: caller contract and the current length check.
            ElementSource::Dynamic(handle) => unsafe { read_elem(ctx, handle, index) },
        }
    }
}

// ----- equality searches (indexOf / lastIndexOf / includes) -----

fn value_kind(kind: ElemKind) -> ValueKind {
    match kind {
        ElemKind::Int | ElemKind::SignedInt => ValueKind::Bits,
        ElemKind::F16 => ValueKind::F16,
        ElemKind::F32 => ValueKind::F32,
        ElemKind::F64 => ValueKind::F64,
        ElemKind::Str => ValueKind::Str,
    }
}

/// Compares an array element and a needle with the selected equality mode.
///
/// # Safety
///
/// `p` and `x` are readable for `size` bytes; `(kind, size)` is a shape
/// [`abi_of`] accepts (the searches check it once, before the loop, as
/// the callback operations do); string handles are live handles of
/// `ctx` (or null).
pub(crate) unsafe fn elem_value_eq(
    ctx: *mut Context,
    kind: ElemKind,
    size: usize,
    p: *const u8,
    x: *const u8,
    same_value_zero: bool,
) -> bool {
    // SAFETY: caller contract is the shared equality contract.
    unsafe { value_eq(ctx, value_kind(kind), size, p, x, same_value_zero) }
}

unsafe fn position(
    ctx: *mut Context,
    h: *mut u8,
    x: *const u8,
    kind: ElemKind,
    from: i32,
    reverse: bool,
    same_value_zero: bool,
) -> i32 {
    if h.is_null() || x.is_null() {
        return -1;
    }
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h, 0), (*ctx).array_elem_size(h)) };
    // The code generators never produce this element shape; the trap
    // prevents a silent wrong comparison.
    // SAFETY: caller contract.
    if unsafe { abi_or_trap(ctx, kind, esz) }.is_none() {
        return -1;
    }
    let start = if from < 0 {
        (n as i64) + i64::from(from)
    } else {
        i64::from(from)
    };
    let (first, count) = if reverse {
        let first = start.min((n as i64) - 1);
        (first, (first + 1).max(0))
    } else {
        let first = start.max(0).min(n as i64);
        (first, (n as i64) - first)
    };
    for step in 0..count {
        let i = if reverse { first - step } else { first + step };
        // SAFETY: `i < n`; caller contract.
        let p = unsafe { (*ctx).array_elem_ptr(h, i as i32, 0) };
        // SAFETY: element storage and needle are `esz` readable bytes.
        if unsafe { elem_value_eq(ctx, kind, esz, p, x, same_value_zero) } {
            return i as i32;
        }
    }
    -1
}

/// `indexOf(x, from)`: first index under per-kind `===` equality, or −1.
/// The position follows the negative and clamp rules of stdlib.md §9.9.
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null); `x` is readable for the
/// element size; string elements/needles are live handles.
pub unsafe fn index_of(
    ctx: *mut Context,
    h: *mut u8,
    x: *const u8,
    kind: ElemKind,
    from: i32,
) -> i32 {
    unsafe { position(ctx, h, x, kind, from, false, false) }
}

/// `lastIndexOf(x, from)`: last index or −1, with the position rules of stdlib.md §9.9.
///
/// # Safety
///
/// As [`index_of`].
pub unsafe fn last_index_of(
    ctx: *mut Context,
    h: *mut u8,
    x: *const u8,
    kind: ElemKind,
    from: i32,
) -> i32 {
    unsafe { position(ctx, h, x, kind, from, true, false) }
}

/// `includes(x, from)`: 1 when found under SameValueZero equality, else 0.
///
/// # Safety
///
/// As [`index_of`].
pub unsafe fn includes(
    ctx: *mut Context,
    h: *mut u8,
    x: *const u8,
    kind: ElemKind,
    from: i32,
) -> i32 {
    i32::from(unsafe { position(ctx, h, x, kind, from, false, true) } >= 0)
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
    // SAFETY: caller contract. Context string allocations keep immutable input
    // allocation addresses stable.
    let sep_bytes = unsafe { (*ctx).str_view(sep) };
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h, pos_id), (*ctx).array_elem_size(h)) };
    let mut out: Vec<u8> = Vec::new();
    for i in 0..n {
        if i > 0 {
            out.extend_from_slice(sep_bytes);
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
                    // SAFETY: live string handle. Context string allocations
                    // keep immutable input allocation addresses stable.
                    let bytes = unsafe { (*ctx).str_view(s) };
                    out.extend_from_slice(bytes);
                }
            }
        }
    }
    // SAFETY: caller contract.
    let ctx = unsafe { &mut *ctx };
    ctx.alloc_str(&out, pos_id)
}

// ----- slice / fill / reverse / concat / structural mutation -----

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
    let (n, esz) = unsafe { (len_of(ctx, h, pos_id), (*ctx).array_elem_size(h)) };
    let (lo, hi) = (clamp_index(start, n), clamp_index(end, n));
    // compiler.md §344 rule 4: a slice takes the fact of its source.
    // SAFETY: caller contract.
    let elements = unsafe { (*ctx).array_elements(h) };
    // SAFETY: caller contract.
    let out = unsafe { &mut *ctx }.array_new_of(esz, elements, pos_id);
    if out.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: the clamped range is inside the source data allocation.
    let data = unsafe { (*ctx).array_data(h) };
    let start = if lo == 0 {
        data
    } else {
        // SAFETY: `lo < hi <= n` when this branch contributes elements.
        unsafe { data.add(lo * esz) }
    };
    // SAFETY: output width matches the source range.
    let _ = unsafe { (*ctx).array_extend(out, start, hi.saturating_sub(lo), pos_id) };
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
    let (n, esz) = unsafe { (len_of(ctx, h, 0), (*ctx).array_elem_size(h)) };
    let (lo, hi) = (clamp_index(start, n), clamp_index(end, n));
    for i in lo..hi {
        // SAFETY: `i < n`; caller contract.
        let p = unsafe { (*ctx).array_elem_ptr(h, i as i32, 0) };
        // SAFETY: `x` readable and `p` writable for `esz` bytes; the
        // fill value lives outside the array (a caller temp), so the
        // regions cannot overlap.
        unsafe { std::ptr::copy_nonoverlapping(x, p, esz) };
    }
    // compiler.md §356 rule 14: the filled elements go to the work list.
    // SAFETY: caller contract; the fill did not move the storage.
    unsafe {
        let data = (*ctx).array_data(h);
        (*ctx).mark_written_elements(h, data, lo, hi);
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
    let (n, esz) = unsafe { (len_of(ctx, h, 0), (*ctx).array_elem_size(h)) };
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
    // compiler.md §356 rule 14a: the elements moved inside the storage, each
    // to any position.
    // SAFETY: caller contract.
    unsafe { (*ctx).mark_moved_elements(h, 0, n, None) };
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
        // The code generators never give concat operands different
        // element sizes.
        // SAFETY: caller contract.
        unsafe { &mut *ctx }.trap(
            TrapKind::Internal,
            "concat operands disagree on the element size",
            pos_id,
        );
        return std::ptr::null_mut();
    }
    // compiler.md §344 rule 4: a concatenation takes the fact of its
    // sources.
    // SAFETY: caller contract.
    let elements = unsafe { (*ctx).array_elements(a).and((*ctx).array_elements(b)) };
    // SAFETY: caller contract.
    let out = unsafe { &mut *ctx }.array_new_of(esz, elements, pos_id);
    if out.is_null() {
        return std::ptr::null_mut();
    }
    for src in [a, b] {
        // SAFETY: equal-width live arrays.
        if !unsafe { append_array(ctx, out, src, pos_id) } {
            return out;
        }
    }
    out
}

/// Appends one array through the shared contiguous extension operation.
unsafe fn append_array(ctx: *mut Context, out: *mut u8, src: *mut u8, pos_id: u32) -> bool {
    if src.is_null() {
        return false;
    }
    // SAFETY: caller supplies live arrays of equal element width.
    let n = unsafe { len_of(ctx, src, pos_id) };
    // SAFETY: the source contains `n` initialized elements.
    let data = unsafe { (*ctx).array_data(src) };
    // SAFETY: matching element widths and live source storage.
    unsafe { (*ctx).array_extend(out, data, n, pos_id) }
}

/// `splice(start, deleteCount)`: returns the removed elements in a
/// fresh array, then removes them from the receiver. `start` uses JS
/// negative/clamp rules; a negative `deleteCount` deletes nothing and
/// a count past the end is clamped.
///
/// The receiver is not mutated until the result array has been fully
/// built, so an allocation trap leaves it unchanged.
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null).
pub unsafe fn splice(
    ctx: *mut Context,
    h: *mut u8,
    start: i32,
    delete_count: i32,
    pos_id: u32,
) -> *mut u8 {
    if h.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h, pos_id), (*ctx).array_elem_size(h)) };
    let lo = clamp_index(start, n);
    let count = usize::try_from(delete_count.max(0))
        .unwrap_or(usize::MAX)
        .min(n - lo);
    // compiler.md §344 rule 4: the removed elements take the fact of the
    // source.
    // SAFETY: caller contract.
    let elements = unsafe { (*ctx).array_elements(h) };
    // SAFETY: caller contract.
    let out = unsafe { &mut *ctx }.array_new_of(esz, elements, pos_id);
    if out.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract; source storage contains `n` elements.
    let data = unsafe { (*ctx).array_data(h) };
    let removed = if lo == 0 {
        data
    } else {
        // SAFETY: `lo <= n`; a zero count does not dereference the pointer.
        unsafe { data.add(lo * esz) }
    };
    // SAFETY: the selected range contains `count` elements.
    if !unsafe { (*ctx).array_extend(out, removed, count, pos_id) } {
        return out; // allocation trap: receiver stays unchanged
    }
    if count == 0 {
        return out;
    }

    // Close the deleted gap with memmove semantics.
    // SAFETY: caller contract; the live data block contains `n * esz`
    // bytes and the source/destination ranges may overlap.
    let data = unsafe { (*ctx).array_data(h) }.cast_mut();
    unsafe {
        std::ptr::copy(
            data.add((lo + count) * esz),
            data.add(lo * esz),
            (n - lo - count) * esz,
        );
    }
    // SAFETY: `count <= n`; this only removes the moved tail suffix.
    unsafe { (*ctx).array_truncate(h, n - count, pos_id) };
    // compiler.md §356 rule 14a: the tail moved `count` elements toward the
    // start.
    // SAFETY: caller contract.
    unsafe { (*ctx).mark_moved_elements(h, lo, n - count, Some(count)) };
    out
}

/// `shift()`: removes the first element into `dst`. It returns false
/// on an empty or dead receiver, and the caller owns the empty trap.
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null); `dst` is writable for the
/// element size.
pub unsafe fn shift(ctx: *mut Context, h: *mut u8, dst: *mut u8, pos_id: u32) -> bool {
    if h.is_null() || dst.is_null() {
        return false;
    }
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h, pos_id), (*ctx).array_elem_size(h)) };
    if n == 0 {
        return false;
    }
    // SAFETY: element zero exists and `dst` is writable for `esz`.
    let first = unsafe { (*ctx).array_elem_ptr(h, 0, 0) };
    unsafe { std::ptr::copy_nonoverlapping(first, dst, esz) };
    // SAFETY: caller contract; the source and destination overlap for
    // arrays longer than two elements, so this is memmove semantics.
    let data = unsafe { (*ctx).array_data(h) }.cast_mut();
    unsafe { std::ptr::copy(data.add(esz), data, (n - 1) * esz) };
    // SAFETY: `n > 0`; this removes the copied tail slot.
    unsafe { (*ctx).array_truncate(h, n - 1, pos_id) };
    // compiler.md §356 rule 14a: the elements moved one element toward the
    // start.
    // SAFETY: caller contract.
    unsafe { (*ctx).mark_moved_elements(h, 0, n - 1, Some(1)) };
    true
}

/// `unshift(x)`: prepends exactly one element and returns the new
/// length, or −1 after an allocation trap.
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null); `x` is readable for the
/// element size.
pub unsafe fn unshift(ctx: *mut Context, h: *mut u8, x: *const u8, pos_id: u32) -> i32 {
    if h.is_null() || x.is_null() {
        return -1;
    }
    // Copy before growth so the operation is correct even if `x`
    // aliases receiver storage retired by `array_push`.
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h, pos_id), (*ctx).array_elem_size(h)) };
    // SAFETY: `x` is readable for `esz` bytes.
    let value = unsafe { std::slice::from_raw_parts(x, esz) }.to_vec();
    // SAFETY: caller contract; `value` is one readable element.
    let new_len = unsafe { (*ctx).array_push(h, value.as_ptr(), pos_id) };
    if new_len < 0 {
        return -1;
    }
    // SAFETY: the grown data block contains `n + 1` slots; move the
    // former contents one slot right with memmove semantics.
    let data = unsafe { (*ctx).array_data(h) }.cast_mut();
    unsafe {
        std::ptr::copy(data, data.add(esz), n * esz);
        std::ptr::copy_nonoverlapping(value.as_ptr(), data, esz);
    }
    // compiler.md §356 rules 14 and 14a: `array_push` gave the new value,
    // or the grown storage. The move is toward the end, so no word of the
    // part that a divided scan did not read enters the part that it read.
    new_len
}

/// `copyWithin(target, start, end)` in place using JS negative/clamp
/// rules. Generated code reuses the receiver handle as the expression's
/// value.
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null).
pub unsafe fn copy_within(ctx: *mut Context, h: *mut u8, target: i32, start: i32, end: i32) {
    if h.is_null() {
        return;
    }
    // SAFETY: caller contract.
    let (n, esz) = unsafe { (len_of(ctx, h, 0), (*ctx).array_elem_size(h)) };
    let (to, from, final_) = (
        clamp_index(target, n),
        clamp_index(start, n),
        clamp_index(end, n),
    );
    let count = final_.saturating_sub(from).min(n - to);
    if count == 0 {
        return;
    }
    // SAFETY: both clamped ranges fit in the live data block. `copy`
    // deliberately provides memmove semantics for overlapping ranges.
    let data = unsafe { (*ctx).array_data(h) }.cast_mut();
    unsafe {
        std::ptr::copy(data.add(from * esz), data.add(to * esz), count * esz);
    }
    // compiler.md §356 rule 14a: a copy toward the start can bring a word
    // from the part that a divided scan did not read. A copy toward the end
    // cannot.
    if from > to {
        // SAFETY: caller contract.
        unsafe { (*ctx).mark_moved_elements(h, to, to + count, Some(from - to)) };
    }
}

#[cfg(test)]
mod tests;
