use super::associations::assoc_receiver_is_live;
use super::{subscript_rt_set_new, subscript_rt_str_iter_code_point};
use crate::context::Context;
use crate::trap::TrapKind;

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
    // SAFETY: shared contract.
    unsafe { (&mut *ctx).array_byte_range(array, offset, size, pos_id) }
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
    if !unsafe { &mut *ctx }.require_live_handle(a as usize, 0) {
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
    if src.is_null() {
        return -1;
    }
    // SAFETY: the shared contract guarantees the receiver and source.
    unsafe { runtime.array_push(a, src, pos_id) }
}

/// The destination of one fused spread traversal (stdlib.md §14.3).
///
/// One walker exists per source kind. The sink decides where its
/// elements go, so a Set construction and an array literal share the
/// walk.
#[derive(Clone, Copy)]
enum SpreadSink {
    /// A fresh array literal; elements append in traversal order.
    Array(*mut u8),
    /// A fresh Set; elements insert under SameValueZero (Q24).
    Set(*mut u8),
}

impl SpreadSink {
    /// Accepts `count` consecutive elements starting at `data`.
    ///
    /// Returns false when the sink stops the traversal.
    ///
    /// # Safety
    ///
    /// `data` is readable for `count` elements of the sink's element
    /// width, and the sink handle is live.
    unsafe fn extend(self, ctx: *mut Context, data: *const u8, count: usize, pos_id: u32) -> bool {
        match self {
            // SAFETY: the arrays have equal element widths.
            SpreadSink::Array(out) => unsafe { (*ctx).array_extend(out, data, count, pos_id) },
            SpreadSink::Set(out) => {
                // SAFETY: validated Set receiver.
                let width = unsafe { crate::assocops::key_size(out) };
                if width == 0 {
                    return false;
                }
                for index in 0..count {
                    // SAFETY: `data` covers `count` keys of `width` bytes.
                    let key = unsafe { data.add(index * width) };
                    // SAFETY: a set has zero-width values.
                    unsafe { crate::assocops::insert(ctx, out, key, std::ptr::null(), pos_id) };
                    // SAFETY: shared contract.
                    if unsafe { (*ctx).trapped() } {
                        return false;
                    }
                }
                true
            }
        }
    }
}

/// Walks a dynamic array into `sink`.
///
/// # Safety
///
/// `source` is a live array whose element width matches the sink's.
unsafe fn spread_from_array(ctx: *mut Context, sink: SpreadSink, source: *mut u8, pos_id: u32) {
    let runtime = unsafe { &mut *ctx };
    if !runtime.require_live_handle(source as usize, pos_id) {
        return;
    }
    // SAFETY: shared receiver contract.
    let count = unsafe { runtime.array_len(source) }.max(0) as usize;
    // SAFETY: source storage contains `count` initialized elements.
    let data = unsafe { runtime.array_data(source) };
    // SAFETY: the source and the sink have equal element widths.
    let _ = unsafe { sink.extend(ctx, data, count, pos_id) };
}

/// Walks a fixed-array buffer into `sink`.
///
/// # Safety
///
/// `data` holds `count` elements of the sink's element width.
unsafe fn spread_from_fixed(
    ctx: *mut Context,
    sink: SpreadSink,
    data: *const u8,
    count: u64,
    pos_id: u32,
) {
    if data.is_null() && count != 0 {
        return;
    }
    // SAFETY: caller supplies the fixed buffer and a matching width.
    let _ = unsafe { sink.extend(ctx, data, count as usize, pos_id) };
}

/// Walks the insertion-ordered keys of a Map/Set into `sink`, using the
/// same fixed traversal bound as `forEach`/`for…of`.
///
/// # Safety
///
/// `source` is a live Map/Set whose key width matches the sink's
/// element width.
unsafe fn spread_from_assoc(ctx: *mut Context, sink: SpreadSink, source: *mut u8, pos_id: u32) {
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, source, pos_id) {
        return;
    }
    // Map/Set keys are limited to at most one machine word.
    let mut scratch = [0u8; 8];
    // SAFETY: validated receiver.
    let bound = unsafe { crate::assocops::iteration_begin(source) };
    for index in 0..bound {
        // SAFETY: scratch covers every accepted key width.
        if unsafe { crate::assocops::iteration_copy(source, index, false, scratch.as_mut_ptr()) }
            && !unsafe { sink.extend(ctx, scratch.as_ptr(), 1, pos_id) }
        {
            break;
        }
    }
    // SAFETY: matching traversal end.
    unsafe { crate::assocops::iteration_end(ctx, source) };
}

/// Walks one string handle per UTF-8 code point into `sink`.
///
/// # Safety
///
/// `source` is a live string and the sink holds string handles.
unsafe fn spread_from_string(ctx: *mut Context, sink: SpreadSink, source: *const u8, pos_id: u32) {
    let runtime = unsafe { &mut *ctx };
    if !runtime.require_live_handle(source as usize, pos_id) {
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
        // SAFETY: one element is one string handle.
        if !unsafe { sink.extend(ctx, (&raw const stored).cast::<u8>(), 1, pos_id) } {
            break;
        }
        index = next;
    }
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
    // SAFETY: shared contract.
    unsafe { spread_from_array(ctx, SpreadSink::Array(out), source, pos_id) };
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
    // SAFETY: shared contract.
    unsafe { spread_from_fixed(ctx, SpreadSink::Array(out), data, count, pos_id) };
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
    // SAFETY: shared contract.
    unsafe { spread_from_assoc(ctx, SpreadSink::Array(out), source, pos_id) };
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
    // SAFETY: shared contract.
    unsafe { spread_from_string(ctx, SpreadSink::Array(out), source, pos_id) };
}

/// Constructs a `Set<K>` from a dynamic array (compiler.md §103.1).
///
/// # Safety
///
/// Shared contract; `source` is a live array whose element width is
/// `key_size`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_from_array(
    ctx: *mut Context,
    source: *mut u8,
    key_size: u64,
    key_kind: u32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let set = unsafe { subscript_rt_set_new(ctx, key_size, key_kind, pos_id) };
    if set.is_null() {
        return set;
    }
    // SAFETY: validated set and shared source contract.
    unsafe { spread_from_array(ctx, SpreadSink::Set(set), source, pos_id) };
    set
}

/// Constructs a `Set<K>` from a fixed-array buffer (compiler.md §103.1).
///
/// # Safety
///
/// Shared contract; `data` holds `count` elements of `key_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_from_fixed(
    ctx: *mut Context,
    data: *const u8,
    count: u64,
    key_size: u64,
    key_kind: u32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let set = unsafe { subscript_rt_set_new(ctx, key_size, key_kind, pos_id) };
    if set.is_null() {
        return set;
    }
    // SAFETY: validated set and shared buffer contract.
    unsafe { spread_from_fixed(ctx, SpreadSink::Set(set), data, count, pos_id) };
    set
}

/// Constructs a `Set<K>` from a Map/Set's keys (compiler.md §103.1).
///
/// # Safety
///
/// Shared contract; `source` is a live Map/Set whose key width is
/// `key_size`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_from_assoc(
    ctx: *mut Context,
    source: *mut u8,
    key_size: u64,
    key_kind: u32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let set = unsafe { subscript_rt_set_new(ctx, key_size, key_kind, pos_id) };
    if set.is_null() {
        return set;
    }
    // SAFETY: validated set and shared receiver contract.
    unsafe { spread_from_assoc(ctx, SpreadSink::Set(set), source, pos_id) };
    set
}

/// Constructs a `Set<string>` from one code point per element
/// (compiler.md §103.1).
///
/// # Safety
///
/// Shared contract; `source` is a live string and `key_size` is the
/// string-handle width.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_from_string(
    ctx: *mut Context,
    source: *const u8,
    key_size: u64,
    key_kind: u32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let set = unsafe { subscript_rt_set_new(ctx, key_size, key_kind, pos_id) };
    if set.is_null() {
        return set;
    }
    // SAFETY: validated set and shared source contract.
    unsafe { spread_from_string(ctx, SpreadSink::Set(set), source, pos_id) };
    set
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
