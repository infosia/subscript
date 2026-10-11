use crate::context::Context;
use icu_normalizer::ComposingNormalizerBorrowed;
use unicode_segmentation::UnicodeSegmentation;

// ----- grapheme clusters and NFC (compiler.md §193) -----
//
// Language strings are always valid UTF-8, and these functions rely on
// that invariant. The grapheme functions read bytes that are not valid
// UTF-8 as the empty string. The result of `normalize` for such bytes
// is not specified.

/// Reads a live string handle as text; bytes that are not UTF-8 read as `""`.
///
/// # Safety
///
/// `ctx` is a live Context and `s` is a live string handle of it.
unsafe fn text_of<'a>(ctx: &Context, s: *const u8) -> &'a str {
    // SAFETY: live handle; Context string storage does not move.
    let bytes = unsafe { ctx.str_view(s) };
    std::str::from_utf8(bytes).unwrap_or_default()
}

/// The byte range of the extended grapheme clusters `[start, end)` of
/// `text`, under the clamping and negative-position rules of `slice`.
/// A reversed range is empty.
fn grapheme_byte_range(text: &str, start: i32, end: i32) -> (usize, usize) {
    // ASCII text with no CR has one cluster per byte (UAX #29 GB999).
    if text.is_ascii() && !text.as_bytes().contains(&b'\r') {
        let (lo, hi, _) = crate::strops::slice_range(text.len(), start, end);
        return (lo, hi);
    }
    let (lo, hi) = if start < 0 || end < 0 {
        let count = text.graphemes(true).count();
        let (lo, hi, _) = crate::strops::slice_range(count, start, end);
        (lo, hi)
    } else {
        let lo = start as usize;
        (lo, (end as usize).max(lo))
    };
    if lo == hi {
        return (0, 0);
    }
    let mut lo_byte = text.len();
    let mut hi_byte = text.len();
    for (cluster, (offset, _)) in text.grapheme_indices(true).enumerate() {
        if cluster == lo {
            lo_byte = offset;
        }
        if cluster == hi {
            hi_byte = offset;
            break;
        }
    }
    (lo_byte, hi_byte)
}

/// `graphemeLength(s)`: the number of extended grapheme clusters (UAX
/// #29). It allocates nothing.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_grapheme_length(ctx: *mut Context, s: *const u8) -> i32 {
    if s.is_null() {
        return 0;
    }
    // SAFETY: shared contract and live string handle.
    let text = unsafe { text_of(&*ctx, s) };
    let count = if text.is_ascii() {
        // In ASCII text, only CR LF joins two bytes into one cluster
        // (UAX #29 GB3); every other byte is one cluster.
        let bytes = text.as_bytes();
        bytes.len() - bytes.windows(2).filter(|pair| *pair == b"\r\n").count()
    } else {
        text.graphemes(true).count()
    };
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// `sliceGraphemes(s, start, end)`: the original bytes of the extended
/// grapheme clusters `[start, end)`, under the clamping and
/// negative-position rules of `slice`. The full range returns the
/// receiver; another range allocates only the result.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_slice_graphemes(
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
    let text = unsafe { text_of(ctx, s) };
    let (lo, hi) = grapheme_byte_range(text, start, end);
    if lo == 0 && hi == text.len() && !text.is_empty() {
        return s as *mut u8;
    }
    ctx.alloc_str(&text.as_bytes()[lo..hi], pos_id)
}

/// `normalize()` and `normalize("NFC")`: the NFC form (UAX #15). Text
/// that is already NFC returns the receiver with no allocation. Other
/// text is normalized into a Rust buffer and copied into a Context string.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_normalize(
    ctx: *mut Context,
    s: *const u8,
    pos_id: u32,
) -> *mut u8 {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handle; Context string storage does not move.
    let bytes = unsafe { ctx.str_view(s) };
    // ASCII text is NFC; this check is faster than the normalizer scan.
    if bytes.is_ascii() {
        return s as *mut u8;
    }
    // The bytes are valid UTF-8 by the string invariant, so they are not
    // checked again. The UTF-8 forms are faster than the `&str` forms
    // (§193 note, section 9.5).
    let normalizer = ComposingNormalizerBorrowed::new_nfc();
    let (head, tail) = normalizer.split_normalized_utf8(bytes);
    if tail.is_empty() {
        return s as *mut u8;
    }
    // One normalizer pass into a Rust buffer, then one copy into the
    // Context string.
    let mut normalized = String::with_capacity(bytes.len());
    normalized.push_str(head);
    // Writing into a `String` never fails.
    let _ = normalizer.normalize_utf8_to(tail, &mut normalized);
    ctx.alloc_str(normalized.as_bytes(), pos_id)
}

#[cfg(test)]
mod tests;
