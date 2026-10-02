use crate::context::Context;
use crate::trap::TrapKind;

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
pub(super) unsafe fn alloc_str_from_view(
    ctx: &mut Context,
    ptr: *const u8,
    len: u64,
    pos_id: u32,
) -> *mut u8 {
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
    // SAFETY: live string handle. Context string allocations keep immutable
    // input allocation addresses stable.
    // SAFETY: live string handle.
    let len = unsafe { ctx.str_bytes(s).len() };
    let (lo, hi, end_boundary) = crate::strops::slice_range(len, start, end);
    let error = format!(
        "slice({start}, {end}) normalizes to ({lo}, {end_boundary}), \
         which is not on a UTF-8 boundary"
    );
    // SAFETY: forwarded shared contract.
    unsafe { str_alloc_range(ctx, s, lo, hi, end_boundary, error, pos_id) }
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

/// `lastIndexOf(needle, position)`: last byte index at or before the clamped position.
/// An empty needle returns the clamped position (stdlib.md §8.9).
///
/// # Safety
///
/// Shared contract; `s` and `needle` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_last_index_of(
    ctx: *mut Context,
    s: *const u8,
    needle: *const u8,
    position: i32,
) -> i32 {
    if s.is_null() || needle.is_null() {
        return -1;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &*ctx };
    // SAFETY: live string handles.
    unsafe { crate::strops::last_index_of(ctx.str_bytes(s), ctx.str_bytes(needle), position) }
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
    // SAFETY: live string handle. Context string allocations keep immutable
    // input allocation addresses stable.
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
    checked_hi: usize,
    error: String,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: `s` is live. Context string allocations keep immutable input
    // allocation addresses stable.
    let bytes = unsafe { ctx.str_view(s) };
    let text = std::str::from_utf8(bytes).unwrap_or_default();
    if !text.is_char_boundary(lo) || !text.is_char_boundary(checked_hi) {
        ctx.trap(TrapKind::StringSlice, error, pos_id);
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
    let error = format!("substring range ({lo}, {hi}) is not on a UTF-8 boundary");
    unsafe { str_alloc_range(ctx, s, lo, hi, hi, error, pos_id) }
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
    let error = format!("substr range ({lo}, {hi}) is not on a UTF-8 boundary");
    unsafe { str_alloc_range(ctx, s, lo, hi, hi, error, pos_id) }
}

enum CodePointReason {
    OutOfRange { len: usize },
    NotBoundary,
}

fn code_point_at(bytes: &[u8], index: i32) -> Result<(usize, char), CodePointReason> {
    let Some(index) = usize::try_from(index)
        .ok()
        .filter(|&index| index < bytes.len())
    else {
        return Err(CodePointReason::OutOfRange { len: bytes.len() });
    };
    let text = std::str::from_utf8(bytes).unwrap_or_default();
    if !text.is_char_boundary(index) {
        return Err(CodePointReason::NotBoundary);
    }
    let ch = text[index..]
        .chars()
        .next()
        .ok_or(CodePointReason::OutOfRange { len: bytes.len() })?;
    Ok((index, ch))
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
    // SAFETY: live string handle. Context string allocations keep immutable
    // input allocation addresses stable.
    let bytes = unsafe { ctx.str_view(s) };
    let (index, ch) = match code_point_at(bytes, i) {
        Ok(found) => found,
        Err(CodePointReason::OutOfRange { .. }) => return ctx.alloc_str(b"", pos_id),
        Err(CodePointReason::NotBoundary) => {
            ctx.trap(
                TrapKind::StrRange,
                format!("charAt({i}) is not on a UTF-8 boundary"),
                pos_id,
            );
            return std::ptr::null_mut();
        }
    };
    let width = ch.len_utf8();
    ctx.alloc_str(&bytes[index..index + width], pos_id)
}

/// `at(i)`: a code point at a signed byte index; invalid indices trap.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_at(
    ctx: *mut Context,
    s: *const u8,
    i: i32,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract and live immutable string.
    let bytes = unsafe { (*ctx).str_view(s) };
    let index = if i < 0 {
        bytes.len() as i64 + i64::from(i)
    } else {
        i64::from(i)
    };
    let index = index as i32;
    match code_point_at(bytes, index) {
        Ok(_) => {}
        Err(CodePointReason::OutOfRange { len }) => {
            // SAFETY: shared contract.
            unsafe {
                (*ctx).trap(
                    TrapKind::StrRange,
                    format!("codePointAt({index}) out of range for string length {len}"),
                    pos_id,
                )
            };
            return std::ptr::null_mut();
        }
        Err(CodePointReason::NotBoundary) => {
            // SAFETY: shared contract.
            unsafe {
                (*ctx).trap(
                    TrapKind::StrRange,
                    format!("charAt({index}) is not on a UTF-8 boundary"),
                    pos_id,
                )
            };
            return std::ptr::null_mut();
        }
    }
    // SAFETY: shared contract; the index names a complete code point.
    unsafe { subscript_rt_str_char_at(ctx, s, index, pos_id) }
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
    // SAFETY: live string handle. Context string allocations keep immutable
    // input allocation addresses stable.
    let bytes = unsafe { ctx.str_view(s) };
    match code_point_at(bytes, i) {
        Ok((_, ch)) => ch as i32,
        Err(CodePointReason::OutOfRange { len }) => {
            ctx.trap(
                TrapKind::StrRange,
                format!("codePointAt({i}) out of range for string length {len}"),
                pos_id,
            );
            0
        }
        Err(CodePointReason::NotBoundary) => {
            ctx.trap(
                TrapKind::StrRange,
                format!("codePointAt({i}) is not on a UTF-8 boundary"),
                pos_id,
            );
            0
        }
    }
}

/// `split(sep, limit)`: a fresh `string[]`, capped by the ToUint32 limit, between separator
/// matches (JS piece order; no match → `[whole]`). An empty separator
/// splits at UTF-8 code-point boundaries. The elements are string handles stored as 8-byte
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
    limit: i32,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() || sep.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handles. Context string allocations keep immutable
    // input allocation addresses stable.
    let hay = unsafe { ctx.str_view(s) };
    // SAFETY: live string handles.
    let sep = unsafe { ctx.str_view(sep) };
    let arr = ctx.array_new(8, pos_id);
    if arr.is_null() {
        return std::ptr::null_mut();
    }
    // §113.2 rule 5: every piece is a Context allocation, and the scan
    // holds no list of pieces of its own, so the first failed piece
    // stops it.
    let mut failed = false;
    let mut remaining = limit as u32;
    crate::strops::split_each(hay, sep, |piece| {
        if remaining == 0 {
            return false;
        }
        remaining -= 1;
        let handle = ctx.alloc_str(piece, pos_id);
        if handle.is_null() {
            failed = true;
            return false;
        }
        let word = handle as u64;
        // SAFETY: `arr` is a live 8-byte-element array of this context;
        // `word` is readable for 8 bytes.
        if unsafe { ctx.array_push(arr, (&word as *const u64).cast(), pos_id) } < 0 {
            failed = true;
            return false;
        }
        true
    });
    if failed {
        return std::ptr::null_mut();
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
    // SAFETY: live string handle. Context string allocations keep immutable
    // input allocation addresses stable.
    let bytes = unsafe { ctx.str_view(s) };
    ctx.alloc_str(strip(bytes), pos_id)
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
    // SAFETY: live string handle. Allocating the result does not move the
    // immutable input allocation.
    let (bytes_ptr, bytes_len) = {
        let bytes = unsafe { ctx.str_bytes(s) };
        (bytes.as_ptr(), bytes.len())
    };
    // §113.2 rule 5: the result size is `len * n`, so the result goes
    // straight into the Context allocation. No second copy of it exists.
    let Some(result_len) = bytes_len.checked_mul(n as usize) else {
        ctx.trap(
            TrapKind::AllocationFailure,
            format!("repeat({n}) of {bytes_len} bytes is not representable"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    ctx.alloc_str_with(result_len, pos_id, |destination| {
        // SAFETY: the input range stays live during this synchronous
        // writer, and it does not overlap the fresh destination.
        let bytes = unsafe { std::slice::from_raw_parts(bytes_ptr, bytes_len) };
        crate::strops::repeat_into(bytes, destination);
    })
}

/// Shared body of `padStart`/`padEnd` (Q21 byte lengths): pads with
/// cyclic copies of `pad`, the final repeat truncated to the target
/// length. An already-long-enough receiver returns a **fresh copy**
/// with unchanged bytes (§8: documented choice — every §8 string
/// result is a fresh Context allocation). An empty `pad` returns
/// a fresh copy with the receiver length at every target.
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
    let result_len = if pad_len == 0 {
        bytes_len
    } else {
        target.max(bytes_len)
    };
    ctx.alloc_str_with(result_len, pos_id, |destination| {
        // SAFETY: both input ranges stay live during this synchronous
        // writer. Neither range overlaps the fresh destination.
        let bytes = unsafe { std::slice::from_raw_parts(bytes_ptr, bytes_len) };
        // SAFETY: as above.
        let pad_bytes = unsafe { std::slice::from_raw_parts(pad_ptr, pad_len) };
        crate::strops::pad_into(bytes, pad_bytes, at_start, destination);
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
    unsafe { str_pad(ctx, s, target, pad, true, pos_id) }
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
    unsafe { str_pad(ctx, s, target, pad, false, pos_id) }
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
    // SAFETY: live string handle. Context string allocations keep immutable
    // input allocation addresses stable.
    let bytes = unsafe { ctx.str_view(s) };
    ctx.alloc_str(&map(bytes), pos_id)
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
    // SAFETY: live string handles. Context string allocations keep immutable
    // input allocation addresses stable.
    let bytes = unsafe { ctx.str_view(s) };
    // SAFETY: live string handles.
    let pat = unsafe { ctx.str_view(pat) };
    // SAFETY: live string handles.
    let repl = unsafe { ctx.str_view(repl) };
    let out = crate::strops::replace_first(bytes, pat, repl);
    ctx.alloc_str(&out, pos_id)
}

/// `replaceAll(pat, repl)`: every occurrence in one left-to-right pass
/// (a replacement is never rescanned), with ECMA string-pattern `$`
/// substitutions (Q27). An empty `pat` matches at every UTF-8
/// code-point boundary, including both ends.
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
    // SAFETY: live string handles. Context string allocations keep immutable
    // input allocation addresses stable.
    let bytes = unsafe { ctx.str_view(s) };
    // SAFETY: live string handles.
    let pat = unsafe { ctx.str_view(pat) };
    // SAFETY: live string handles.
    let repl = unsafe { ctx.str_view(repl) };
    let out = crate::strops::replace_all(bytes, pat, repl);
    ctx.alloc_str(&out, pos_id)
}
