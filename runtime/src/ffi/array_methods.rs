use crate::context::Context;
use crate::trap::TrapKind;

// ----- Array methods (stdlib.md §9, Q22) -----
//
// The logic lives in [`crate::arrops`]; these wrappers decode the kind
// tags. Convention: the receiver handle follows `ctx`; element values
// the runtime receives travel by pointer; script callbacks travel as a
// `(code, env)` language function value; allocating entries carry a
// trailing `pos_id`. Every callback-taking entry returns immediately
// when the Context is already trapped and re-checks the trap flag after
// each callback return (stdlib.md §9).

/// Decodes an element-kind tag. The code generators emit only known
/// tags; an unknown tag records an Internal trap.
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

/// `indexOf(x, from)`: first index under per-kind `===` equality, or −1
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
    from: i32,
) -> i32 {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return -1;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::index_of(ctx, a, x, kind, from) }
}

/// `lastIndexOf(x, from)`: last index or −1, with the position rules of stdlib.md §9.9.
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
    from: i32,
) -> i32 {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return -1;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::last_index_of(ctx, a, x, kind, from) }
}

/// `includes(x, from)`: 1 when found under SameValueZero equality, else 0.
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
    from: i32,
) -> i32 {
    // SAFETY: shared contract (forwarded).
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return 0;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::includes(ctx, a, x, kind, from) }
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

/// `at(i)`: copies the element at a signed index into `out`.
///
/// # Safety
///
/// Shared contract; `a` is live and `out` is writable for one element.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_at(
    ctx: *mut Context,
    a: *mut u8,
    index: i32,
    out: *mut u8,
    pos_id: u32,
) {
    if a.is_null() {
        return;
    }
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    // SAFETY: live array.
    let index = if index < 0 {
        (unsafe { runtime.array_len(a) }) + index
    } else {
        index
    };
    // SAFETY: live array; this accessor checks the index.
    let source = unsafe { runtime.array_elem_ptr(a, index, pos_id) };
    if !source.is_null() {
        // SAFETY: source and destination hold one element and do not overlap.
        unsafe { std::ptr::copy_nonoverlapping(source, out, runtime.array_elem_size(a)) };
    }
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

/// Returns the first matching reference element, or null.
///
/// # Safety
///
/// As [`subscript_rt_arr_some`]; elements are pointer-sized references.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_find(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return std::ptr::null_mut();
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::find(ctx, a, code, env, kind, indexed != 0, false) }
}

/// Returns the last matching reference element, or null.
///
/// # Safety
///
/// As [`subscript_rt_arr_some`]; elements are pointer-sized references.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_find_last(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return std::ptr::null_mut();
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::find(ctx, a, code, env, kind, indexed != 0, true) }
}

/// Returns the last matching index, or minus one.
///
/// # Safety
///
/// As [`subscript_rt_arr_some`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_find_last_index(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: u32,
    indexed: u32,
) -> i32 {
    // SAFETY: shared contract.
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return -1;
    };
    // SAFETY: shared contract.
    unsafe { crate::arrops::find_last_index(ctx, a, code, env, kind, indexed != 0) }
}

/// Concatenates callback arrays at depth one.
///
/// # Safety
///
/// As [`subscript_rt_arr_map`]; the callback returns an array of `ret_size`-byte elements.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_arr_flat_map(
    ctx: *mut Context,
    a: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: u32,
    ret_kind: u32,
    ret_size: u64,
    pos_id: u32,
    indexed: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let (Some(kind), Some(_)) = (unsafe { decode_elem_kind(ctx, kind) }, unsafe {
        decode_elem_kind(ctx, ret_kind)
    }) else {
        return std::ptr::null_mut();
    };
    // SAFETY: shared contract.
    unsafe {
        crate::arrops::flat_map(
            ctx,
            a,
            (code, env),
            kind,
            ret_size as usize,
            pos_id,
            indexed != 0,
        )
    }
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
    let (Some(elem_kind), Some(acc_kind)) = (unsafe { decode_elem_kind(ctx, elem_kind) }, unsafe {
        decode_elem_kind(ctx, acc_kind)
    }) else {
        return;
    };
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
    let (Some(elem_kind), Some(acc_kind)) = (unsafe { decode_elem_kind(ctx, elem_kind) }, unsafe {
        decode_elem_kind(ctx, acc_kind)
    }) else {
        return;
    };
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
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return 0;
    };
    unsafe {
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
    }
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
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return 0;
    };
    unsafe {
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
    }
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
    let Some(kind) = (unsafe { decode_elem_kind(ctx, kind) }) else {
        return -1;
    };
    unsafe {
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
    }
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
