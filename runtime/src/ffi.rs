//! The C-ABI boundary called by generated code.
//!
//! Every function here is `extern "C"` with a stable signature; the
//! code generator declares them by name and the JIT resolves them by
//! symbol registration. None of them unwinds: the runtime reports
//! faults through the Context trap state, never through panics, and
//! the bodies contain no panicking operations on their script-facing
//! paths (allocation failure is reported as a trap).
//!
//! Shared safety contract (each function's `# Safety` builds on it):
//! `ctx` is the non-null Context of the current script run, created by
//! [`Context::new`] and passed to the script entry by the driver;
//! handles were produced by this context's allocation functions and
//! the script ran under the emitted trap-check discipline, so a null
//! result from a trapping function is never fed into another call.

use crate::context::Context;
use crate::trap::TrapKind;

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
    let kind = TrapKind::from_u32(kind).unwrap_or(TrapKind::AllocationFailure);
    let message = match kind {
        TrapKind::IndexOutOfBounds => "index out of bounds",
        TrapKind::NullNarrowing => "`as` narrowing applied to null",
        TrapKind::ClassMismatch => "`as` narrowing to a class the instance does not have",
        TrapKind::UseAfterDelete => "use of a deleted allocation",
        TrapKind::DivisionByZero => "integer division by zero",
        other => other.rule(),
    };
    ctx.trap(kind, message, pos_id);
}

/// Registers a permanent root slot (module global of managed type).
///
/// # Safety
///
/// Shared contract; `slot` is the address of an 8-byte slot that
/// outlives the script run.
#[no_mangle]
pub unsafe extern "C" fn sub_rt_root_add(ctx: *mut Context, slot: *mut u8) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.root_add(slot as usize);
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
    // SAFETY: live string handle.
    let bytes = unsafe { ctx.str_bytes(s) };
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
    let text = std::str::from_utf8(bytes).unwrap_or_default();
    let (lo, hi) = (lo as usize, hi as usize);
    if !text.is_char_boundary(lo) || !text.is_char_boundary(hi) {
        ctx.trap(
            TrapKind::StringSlice,
            format!("slice({start}, {end}) is not on a UTF-8 boundary"),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    let owned = bytes[lo..hi].to_vec();
    ctx.alloc_str(&owned, pos_id)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
