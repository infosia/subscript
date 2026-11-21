//! `Map.groupBy` (stdlib.md §10, Q24).

use std::cell::Cell;

use super::{get, insert, new, KeyKind};
use crate::context::{Context, OperationRoot};

/// Fixed ABI of a generated `Map.groupBy` callback bridge. The bridge
/// passes the element and index into the script ABI and writes the key result.
type GroupBridge =
    unsafe extern "C" fn(*mut Context, *const u8, *const u8, *const u8, *mut u8, i32);

/// Groups an array under callback-produced keys. The output map, every
/// group array, and every stored element own fresh Context storage.
///
/// The current array element is copied before the callback runs. This is
/// required both by C2 aggregate value semantics and because the callback
/// may mutate/grow the source array, invalidating a pointer into its live
/// element block.
///
/// # Safety
///
/// `items` is a live array; `code`/`env` are a callback `(T[, i32]) -> K`;
/// `bridge` has the [`GroupBridge`] signature and writes `key_size`
/// bytes to its `key_out` argument.
pub(crate) unsafe fn group_by(
    ctx: *mut Context,
    items: *mut u8,
    code: *const u8,
    env: *const u8,
    bridge: *const u8,
    key_size: usize,
    kind: KeyKind,
    pos_id: u32,
) -> *mut u8 {
    if items.is_null() || code.is_null() || bridge.is_null() || unsafe { (*ctx).trapped() } {
        return std::ptr::null_mut();
    }
    let out = new(
        unsafe { &mut *ctx },
        key_size,
        std::mem::size_of::<*mut u8>(),
        kind,
        false,
        pos_id,
    );
    if out.is_null() {
        return out;
    }
    // Runtime-owned results are not yet visible in a generated local.
    // Root both the input and the output across the callback: a collection
    // in the callback, by `Context.collect()` or by a poll (`compiler.md`
    // §342 rule 7), keeps them (`compiler.md` §345 rule 1).
    let roots = [Cell::new(items as usize), Cell::new(out as usize)];
    // SAFETY: `roots` outlives the guard, which drops at the return.
    let _roots = unsafe { OperationRoot::cells(ctx, &roots) };

    let elem_size = unsafe { (*ctx).array_elem_size(items) };
    // compiler.md §344 rule 4: each group holds elements of `items`, so it
    // takes the fact of `items`.
    // SAFETY: `items` is a live array (caller contract).
    let elements = unsafe { (*ctx).array_elements(items) };
    let initial_len = unsafe { (*ctx).array_len(items) }.max(0) as usize;
    // Generated bridges perform concrete typed loads from `element` and
    // typed stores into `key`. Back both slots with `u64` storage so the
    // bridge always receives at least 8-byte-aligned addresses.
    let mut element = vec![0u64; elem_size.div_ceil(std::mem::size_of::<u64>())];
    let element_ptr = element.as_mut_ptr().cast::<u8>();
    // compiler.md §345 rule 1: the copy keeps the element that the push
    // reads after the callback, including each handle that it contains.
    // SAFETY: `element` does not reallocate and outlives the guard, which
    // drops at the return, before `_roots`.
    let _element_root = unsafe { OperationRoot::push(ctx, element_ptr, element.len()) };
    let call: GroupBridge = unsafe { std::mem::transmute(bridge) };
    for index in 0..initial_len {
        // Match the existing Array callback methods: appends after entry
        // do not extend the visit count, while removals shorten it.
        if unsafe { (*ctx).array_len(items) }.max(0) as usize <= index {
            break;
        }
        let source = unsafe { (*ctx).array_elem_ptr(items, index as i32, pos_id) };
        if source.is_null() {
            break;
        }
        // SAFETY: source is an in-bounds element slot and `element`
        // owns exactly the tier's element width.
        unsafe { std::ptr::copy_nonoverlapping(source, element_ptr, elem_size) };
        let mut key = 0u64;
        let key_out = (&mut key as *mut u64).cast();
        // SAFETY: generated bridge contract. Q24 key widths are at most
        // eight bytes, validated by `new`.
        unsafe { call(ctx, code, env, element_ptr, key_out, index as i32) };
        if unsafe { (*ctx).trapped() } {
            break;
        }

        let mut group = std::ptr::null_mut::<u8>();
        if unsafe {
            get(
                ctx,
                out,
                (&key as *const u64).cast(),
                (&mut group as *mut *mut u8).cast(),
            )
        } {
            if unsafe { (*ctx).array_push(group, element_ptr, pos_id) } < 0 {
                break;
            }
            continue;
        }

        let created = unsafe { &mut *ctx }.array_new_of(elem_size, elements, pos_id);
        if created.is_null() {
            break;
        }
        if unsafe { (*ctx).array_push(created, element_ptr, pos_id) } < 0 {
            break;
        }
        // `insert` copies the handle into map-owned entry storage; no
        // script callback or collector can run before insertion.
        unsafe {
            insert(
                ctx,
                out,
                (&key as *const u64).cast(),
                (&created as *const *mut u8).cast(),
                pos_id,
            )
        };
        if unsafe { (*ctx).trapped() } {
            break;
        }
    }
    out
}
