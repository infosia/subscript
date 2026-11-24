//! `sort(cmp)` with a script comparator (stdlib.md §9, Q22).

use super::{abi_or_trap, call2, cb_blocked, len_of, read_elem, Abi, ElemKind};
use crate::context::{ArrayElements, Context, OperationRoot};

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
    let n = unsafe { len_of(ctx, h, 0) };
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
        // compiler.md §345 rule 1: the scratch buffers hold every element
        // that a comparator can remove from the array. Each buffer keeps
        // its storage for the whole sort (the merge swaps the two `Vec`
        // values, not their storage), so a root over each range holds
        // every element until the write-back.
        let words = n * std::mem::size_of::<T>() / std::mem::size_of::<u64>();
        // SAFETY: caller contract.
        let holds_handle = unsafe { (*ctx).array_elements(h) } == ArrayElements::CanHoldHandle;
        // SAFETY: each buffer has `n` elements and does not reallocate
        // before the guards drop at the end of this block, `_tmp_root` first.
        let _buf_root =
            holds_handle.then(|| unsafe { OperationRoot::push(ctx, buf.as_ptr().cast(), words) });
        // SAFETY: as above.
        let _tmp_root =
            holds_handle.then(|| unsafe { OperationRoot::push(ctx, tmp.as_ptr().cast(), words) });
        // SAFETY: comparator contract (caller).
        if !unsafe { merge_sort_by::<T>(ctx, code, env, &mut buf, &mut tmp) } {
            return; // comparator trapped: the array is untouched
        }
        // SAFETY: caller contract.
        if unsafe { len_of(ctx, h, 0) } != n {
            return; // a comparator resized the array: keep it as-is
        }
        for (i, v) in buf.iter().enumerate() {
            // SAFETY: `i < n`; element slots are `esz` writable bytes.
            let p = unsafe { (*ctx).array_elem_ptr(h, i as i32, 0) };
            // SAFETY: as above; `size_of::<T>() == esz` by dispatch.
            unsafe { p.cast::<T>().write_unaligned(*v) };
        }
        // compiler.md §356 rule 14: the write-back copies each element from
        // the scratch buffers, which a comparator can make the only holder
        // of a value, so the written elements go to the work list. The
        // scratch roots end with this block.
        // SAFETY: caller contract; the write-back did not move the storage.
        unsafe {
            let data = (*ctx).array_data(h);
            (*ctx).mark_written_elements(h, data, 0, n);
        }
    });
}
