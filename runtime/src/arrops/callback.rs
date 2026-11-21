//! Callback operations of `Array` (stdlib.md §9, Q22): `forEach`, `map`,
//! `flatMap`, `filter`, `reduce`, `reduceRight`, `some`, `every`, `find`,
//! `findIndex`, and `findLastIndex`.

use std::cell::Cell;

use super::{abi_or_trap, append_array, call_reduce, call_value, Abi, ElemKind, ElementSource};
use crate::context::{root_word, ArrayElements, Context, OperationRoot};

/// `forEach(f)`: calls `f(v)` or `f(v, i)` per element in index order;
/// aborts on the first trap.
///
/// # Safety
///
/// `h` is a live array of `ctx` (or null); `code`/`env` are a language
/// function value of shape `(ctx, env, T) -> void` or
/// `(ctx, env, T, i32) -> void` for the element ABI, selected by
/// `indexed`.
pub unsafe fn for_each(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
    indexed: bool,
) {
    // SAFETY: caller contract.
    unsafe { for_each_source(ctx, ElementSource::Dynamic(h), code, env, kind, indexed) };
}

/// Shared dynamic-array and fixed-array `forEach` loop.
unsafe fn for_each_source(
    ctx: *mut Context,
    source: ElementSource,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
    indexed: bool,
) {
    // SAFETY: caller contract.
    if unsafe { source.cb_blocked(ctx, code) } {
        return;
    }
    // SAFETY: caller contract.
    let esz = unsafe { source.elem_size(ctx) };
    // SAFETY: caller contract.
    let Some(abi) = (unsafe { abi_or_trap(ctx, kind, esz) }) else {
        return;
    };
    // SAFETY: caller contract.
    let n = unsafe { source.len(ctx, 0) };
    with_abi!(abi, T, {
        for i in 0..n {
            // A callback may mutate a dynamic array. A fixed source
            // returns the same constant length on every iteration.
            // SAFETY: caller contract.
            if unsafe { source.len(ctx, 0) } <= i {
                break;
            }
            // SAFETY: `i` in bounds; `size_of::<T>() == esz` by dispatch.
            let v: T = unsafe { source.read(ctx, i) };
            // SAFETY: callback contract (caller).
            let () = unsafe { call_value::<T, ()>(code, ctx, env, v, i as i32, indexed) };
            // SAFETY: caller contract.
            if unsafe { (*ctx).trapped() } {
                break;
            }
        }
    });
}

/// `map(f)`: a fresh array of `f(v)` results (`ret_size`-byte elements
/// of `ret_kind`) with the handle fact `elements` that the compiler gives
/// for the result element type (compiler.md §344 rule 4). A mid-iteration
/// callback trap aborts and returns the valid partial array (generated code
/// surfaces the trap before any use).
///
/// # Safety
///
/// As [`for_each`], with callback shape `(ctx, env, T) -> R` or
/// `(ctx, env, T, i32) -> R` for the element and result ABIs.
pub unsafe fn map(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    elem_kind: ElemKind,
    ret_kind: ElemKind,
    ret_size: usize,
    pos_id: u32,
    indexed: bool,
    elements: ArrayElements,
) -> *mut u8 {
    // SAFETY: caller contract.
    unsafe {
        map_source(
            ctx,
            ElementSource::Dynamic(h),
            (code, env),
            elem_kind,
            MapResult::Value {
                kind: ret_kind,
                size: ret_size,
                elements,
            },
            pos_id,
            indexed,
        )
    }
}

/// Concatenates each callback array before the next callback runs.
/// `callback` contains the code pointer and environment pointer. The
/// result has the handle fact `elements` that the compiler gives for the
/// result element type (compiler.md §344 rule 4).
///
/// # Safety
///
/// As [`map`]; the callback returns an array of `ret_size`-byte elements.
// The parameters mirror the C entry `subscript_rt_arr_flat_map` one to one.
#[allow(clippy::too_many_arguments)]
pub unsafe fn flat_map(
    ctx: *mut Context,
    h: *mut u8,
    callback: (*const u8, *const u8),
    elem_kind: ElemKind,
    ret_size: usize,
    pos_id: u32,
    indexed: bool,
    elements: ArrayElements,
) -> *mut u8 {
    // SAFETY: forwarded callback and array contract.
    unsafe {
        map_source(
            ctx,
            ElementSource::Dynamic(h),
            callback,
            elem_kind,
            MapResult::Array {
                elem_size: ret_size,
                elements,
            },
            pos_id,
            indexed,
        )
    }
}

/// The callback result shape and output element size.
enum MapResult {
    Value {
        kind: ElemKind,
        size: usize,
        elements: ArrayElements,
    },
    Array {
        elem_size: usize,
        elements: ArrayElements,
    },
}

/// Shared dynamic-array and fixed-array `map` loop.
unsafe fn map_source(
    ctx: *mut Context,
    source: ElementSource,
    callback: (*const u8, *const u8),
    elem_kind: ElemKind,
    result: MapResult,
    pos_id: u32,
    indexed: bool,
) -> *mut u8 {
    let (code, env) = callback;
    let (ret_kind, ret_size, flatten, elements) = match result {
        MapResult::Value {
            kind,
            size,
            elements,
        } => (kind, size, false, elements),
        MapResult::Array {
            elem_size,
            elements,
        } => (ElemKind::Int, elem_size, true, elements),
    };
    // SAFETY: caller contract.
    if unsafe { source.cb_blocked(ctx, code) } {
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract.
    let esz = unsafe { source.elem_size(ctx) };
    // SAFETY: caller contract.
    let (Some(ea), Some(ra)) = (unsafe { abi_or_trap(ctx, elem_kind, esz) }, unsafe {
        abi_or_trap(
            ctx,
            ret_kind,
            if flatten {
                std::mem::size_of::<usize>()
            } else {
                ret_size
            },
        )
    }) else {
        return std::ptr::null_mut();
    };
    // compiler.md §344 rule 4: the result takes the fact that the
    // compiler gives for the result element type.
    // SAFETY: caller contract.
    let out = unsafe { &mut *ctx }.array_new_of(ret_size, elements, pos_id);
    if out.is_null() {
        return std::ptr::null_mut();
    }
    // compiler.md §345 rule 1: the slot keeps the result across each callback.
    let root = [Cell::new(out as usize)];
    // SAFETY: `root` outlives the guard, which drops at the return.
    let _root = unsafe { OperationRoot::cells(ctx, &root) };
    // SAFETY: caller contract.
    let n = unsafe { source.len(ctx, pos_id) };
    with_abi!(ea, T, {
        with_abi!(ra, R, {
            for i in 0..n {
                // SAFETY: caller contract.
                if unsafe { source.len(ctx, pos_id) } <= i {
                    break;
                }
                // SAFETY: `i` in bounds; widths match by dispatch.
                let v: T = unsafe { source.read(ctx, i) };
                // SAFETY: callback contract (caller).
                let r: R = unsafe { call_value::<T, R>(code, ctx, env, v, i as i32, indexed) };
                // SAFETY: caller contract.
                if unsafe { (*ctx).trapped() } {
                    break;
                }
                if flatten {
                    // SAFETY: flatten dispatch uses the pointer-sized integer ABI.
                    let array =
                        unsafe { std::ptr::read_unaligned((&r as *const R).cast::<*mut u8>()) };
                    // SAFETY: the callback returns an array with the output element type.
                    if !unsafe { append_array(ctx, out, array, pos_id) } {
                        break;
                    }
                    continue;
                }
                // SAFETY: `out` is a live `ret_size`-element array;
                // `size_of::<R>() == ret_size` by dispatch.
                if unsafe { (*ctx).array_push(out, (&r as *const R).cast::<u8>(), pos_id) } < 0 {
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
/// As [`for_each`], with callback shape `(ctx, env, T) -> boolean` or
/// `(ctx, env, T, i32) -> boolean`.
pub unsafe fn filter(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
    pos_id: u32,
    indexed: bool,
) -> *mut u8 {
    // SAFETY: caller contract.
    unsafe {
        filter_source(
            ctx,
            ElementSource::Dynamic(h),
            code,
            env,
            kind,
            pos_id,
            indexed,
        )
    }
}

/// Shared dynamic-array and fixed-array `filter` loop.
unsafe fn filter_source(
    ctx: *mut Context,
    source: ElementSource,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
    pos_id: u32,
    indexed: bool,
) -> *mut u8 {
    // SAFETY: caller contract.
    if unsafe { source.cb_blocked(ctx, code) } {
        return std::ptr::null_mut();
    }
    // SAFETY: caller contract.
    let esz = unsafe { source.elem_size(ctx) };
    // SAFETY: caller contract.
    let Some(abi) = (unsafe { abi_or_trap(ctx, kind, esz) }) else {
        return std::ptr::null_mut();
    };
    // compiler.md §344 rule 4: a filter takes the fact of its source.
    // SAFETY: caller contract.
    let elements = unsafe { source.elements(ctx) };
    // SAFETY: caller contract.
    let out = unsafe { &mut *ctx }.array_new_of(esz, elements, pos_id);
    if out.is_null() {
        return std::ptr::null_mut();
    }
    // compiler.md §345 rule 1: slot 0 keeps the result and slot 1 keeps
    // the element that the push reads after the callback.
    let roots = [Cell::new(out as usize), Cell::new(0)];
    // SAFETY: `roots` outlives the guard, which drops at the return.
    let _root = unsafe { OperationRoot::cells(ctx, &roots) };
    // SAFETY: caller contract.
    let n = unsafe { source.len(ctx, pos_id) };
    with_abi!(abi, T, {
        for i in 0..n {
            // SAFETY: caller contract.
            if unsafe { source.len(ctx, pos_id) } <= i {
                break;
            }
            // SAFETY: `i` in bounds; widths match by dispatch.
            let v: T = unsafe { source.read(ctx, i) };
            roots[1].set(root_word(&v));
            // SAFETY: callback contract (caller). A language boolean
            // returns as its low byte on both tiers.
            let keep: u8 = unsafe { call_value::<T, u8>(code, ctx, env, v, i as i32, indexed) };
            // SAFETY: caller contract.
            if unsafe { (*ctx).trapped() } {
                break;
            }
            if keep != 0 {
                // SAFETY: `v` is `esz` bytes by dispatch.
                if unsafe { (*ctx).array_push(out, (&v as *const T).cast::<u8>(), pos_id) } < 0 {
                    break;
                }
            }
        }
    });
    out
}

/// Direction of a reduction.
#[derive(Clone, Copy)]
enum ReduceDirection {
    /// Visit indices from zero upward.
    Left,
    /// Visit indices from the initial last index downward.
    Right,
}

/// Shared `reduce`/`reduceRight` implementation.
///
/// # Safety
///
/// As [`for_each`], with callback shape `(ctx, env, A, T) -> A` or
/// `(ctx, env, A, T, i32) -> A`; `acc_ptr` is readable and writable for
/// `acc_size` bytes.
unsafe fn reduce_direction(
    ctx: *mut Context,
    source: ElementSource,
    code: *const u8,
    env: *const u8,
    elem_kind: ElemKind,
    acc_kind: ElemKind,
    acc_size: usize,
    acc_ptr: *mut u8,
    direction: ReduceDirection,
    indexed: bool,
) {
    if acc_ptr.is_null() {
        return;
    }
    // SAFETY: caller contract.
    if unsafe { source.cb_blocked(ctx, code) } {
        return;
    }
    // SAFETY: caller contract.
    let esz = unsafe { source.elem_size(ctx) };
    // SAFETY: caller contract.
    let (Some(ea), Some(aa)) = (unsafe { abi_or_trap(ctx, elem_kind, esz) }, unsafe {
        abi_or_trap(ctx, acc_kind, acc_size)
    }) else {
        return;
    };
    // SAFETY: caller contract.
    let n = unsafe { source.len(ctx, 0) };
    with_abi!(ea, T, {
        with_abi!(aa, A, {
            // SAFETY: `acc_ptr` readable for `acc_size == size_of::<A>()`.
            let mut acc: A = unsafe { acc_ptr.cast::<A>().read_unaligned() };
            for step in 0..n {
                let i = match direction {
                    ReduceDirection::Left => step,
                    ReduceDirection::Right => n - 1 - step,
                };
                // SAFETY: caller contract.
                if unsafe { source.len(ctx, 0) } <= i {
                    if matches!(direction, ReduceDirection::Left) {
                        break;
                    }
                    continue;
                }
                // SAFETY: `i` in bounds; widths match by dispatch.
                let v: T = unsafe { source.read(ctx, i) };
                // SAFETY: callback contract (caller).
                let r: A = unsafe { call_reduce(code, ctx, env, acc, v, i as i32, indexed) };
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

/// `reduce(f, init)`: folds left with the accumulator traveling in/out
/// through `acc_ptr` (its C-ABI class is `acc_kind` at `acc_size`
/// bytes). On a callback trap the last completed accumulator remains in
/// `acc_ptr`.
///
/// # Safety
///
/// As [`for_each`], with callback shape `(ctx, env, A, T) -> A` or
/// `(ctx, env, A, T, i32) -> A`; `acc_ptr` is readable and writable for
/// `acc_size` bytes.
pub unsafe fn reduce(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    elem_kind: ElemKind,
    acc_kind: ElemKind,
    acc_size: usize,
    acc_ptr: *mut u8,
    indexed: bool,
) {
    // SAFETY: forwarded contract.
    unsafe {
        reduce_direction(
            ctx,
            ElementSource::Dynamic(h),
            code,
            env,
            elem_kind,
            acc_kind,
            acc_size,
            acc_ptr,
            ReduceDirection::Left,
            indexed,
        )
    };
}

/// `reduceRight(f, init)`: folds right-to-left. On a callback trap the
/// last completed accumulator remains in `acc_ptr`.
///
/// # Safety
///
/// As [`reduce`].
pub unsafe fn reduce_right(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    elem_kind: ElemKind,
    acc_kind: ElemKind,
    acc_size: usize,
    acc_ptr: *mut u8,
    indexed: bool,
) {
    // SAFETY: forwarded contract.
    unsafe {
        reduce_direction(
            ctx,
            ElementSource::Dynamic(h),
            code,
            env,
            elem_kind,
            acc_kind,
            acc_size,
            acc_ptr,
            ReduceDirection::Right,
            indexed,
        )
    };
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
    /// Reverse search for a matching index.
    FindLastIndex,
}

/// Shared short-circuiting predicate loop. Returns the defensible
/// constant (0 / 0 / −1) on trap.
///
/// # Safety
///
/// As [`for_each`], with callback shape `(ctx, env, T) -> boolean` or
/// `(ctx, env, T, i32) -> boolean`.
unsafe fn search(
    ctx: *mut Context,
    source: ElementSource,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
    mode: SearchMode,
    indexed: bool,
    found: *mut u8,
) -> i32 {
    let miss = match mode {
        SearchMode::Some => 0,
        SearchMode::Every => 1,
        SearchMode::FindIndex | SearchMode::FindLastIndex => -1,
    };
    let trapped_result = if matches!(mode, SearchMode::FindIndex | SearchMode::FindLastIndex) {
        -1
    } else {
        0
    };
    // SAFETY: caller contract.
    if unsafe { source.cb_blocked(ctx, code) } {
        return trapped_result;
    }
    // SAFETY: caller contract.
    let esz = unsafe { source.elem_size(ctx) };
    // SAFETY: caller contract.
    let Some(abi) = (unsafe { abi_or_trap(ctx, kind, esz) }) else {
        return trapped_result;
    };
    // compiler.md §345 rule 1: the slot keeps the element that the copy
    // to `found` reads after the callback.
    let root = [Cell::new(0)];
    // SAFETY: `root` outlives the guard, which drops at the return.
    let _root = unsafe { OperationRoot::cells(ctx, &root) };
    // SAFETY: caller contract.
    let n = unsafe { source.len(ctx, 0) };
    with_abi!(abi, T, {
        for step in 0..n {
            let i = if mode == SearchMode::FindLastIndex {
                n - 1 - step
            } else {
                step
            };
            // SAFETY: caller contract.
            if unsafe { source.len(ctx, 0) } <= i {
                if mode == SearchMode::FindLastIndex {
                    continue;
                }
                break;
            }
            // SAFETY: `i` in bounds; widths match by dispatch.
            let v: T = unsafe { source.read(ctx, i) };
            root[0].set(root_word(&v));
            // SAFETY: callback contract (caller).
            let r: u8 = unsafe { call_value::<T, u8>(code, ctx, env, v, i as i32, indexed) };
            // SAFETY: caller contract.
            if unsafe { (*ctx).trapped() } {
                return trapped_result;
            }
            match mode {
                SearchMode::Some if r != 0 => return 1,
                SearchMode::Every if r == 0 => return 0,
                SearchMode::FindIndex | SearchMode::FindLastIndex if r != 0 => {
                    if !found.is_null() {
                        // SAFETY: `found` holds one element; `v` retains the pre-callback value.
                        unsafe {
                            std::ptr::copy_nonoverlapping((&v as *const T).cast::<u8>(), found, esz)
                        };
                    }
                    return i as i32;
                }
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
/// As [`for_each`], with callback shape `(ctx, env, T) -> boolean` or
/// `(ctx, env, T, i32) -> boolean`.
pub unsafe fn some(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
    indexed: bool,
) -> i32 {
    // SAFETY: forwarded contract.
    unsafe {
        search(
            ctx,
            ElementSource::Dynamic(h),
            code,
            env,
            kind,
            SearchMode::Some,
            indexed,
            std::ptr::null_mut(),
        )
    }
}

/// `every(f)`: 1 when every element satisfies `f` (short-circuits on
/// the first miss), else 0.
///
/// # Safety
///
/// As [`some`].
pub unsafe fn every(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
    indexed: bool,
) -> i32 {
    // SAFETY: forwarded contract.
    unsafe {
        search(
            ctx,
            ElementSource::Dynamic(h),
            code,
            env,
            kind,
            SearchMode::Every,
            indexed,
            std::ptr::null_mut(),
        )
    }
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
    indexed: bool,
) -> i32 {
    // SAFETY: forwarded contract.
    unsafe {
        search(
            ctx,
            ElementSource::Dynamic(h),
            code,
            env,
            kind,
            SearchMode::FindIndex,
            indexed,
            std::ptr::null_mut(),
        )
    }
}

/// Finds a matching reference element in the selected direction.
///
/// # Safety
///
/// As [`some`]; each element has pointer width.
pub unsafe fn find(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
    indexed: bool,
    reverse: bool,
) -> *mut u8 {
    let mut value: *mut u8 = std::ptr::null_mut();
    let mode = if reverse {
        SearchMode::FindLastIndex
    } else {
        SearchMode::FindIndex
    };
    // SAFETY: the output holds one reference element.
    unsafe {
        search(
            ctx,
            ElementSource::Dynamic(h),
            code,
            env,
            kind,
            mode,
            indexed,
            (&mut value as *mut *mut u8).cast(),
        )
    };
    value
}

/// Finds the last matching index, or minus one.
///
/// # Safety
///
/// As [`some`].
pub unsafe fn find_last_index(
    ctx: *mut Context,
    h: *mut u8,
    code: *const u8,
    env: *const u8,
    kind: ElemKind,
    indexed: bool,
) -> i32 {
    // SAFETY: forwarded contract.
    unsafe {
        search(
            ctx,
            ElementSource::Dynamic(h),
            code,
            env,
            kind,
            SearchMode::FindLastIndex,
            indexed,
            std::ptr::null_mut(),
        )
    }
}
