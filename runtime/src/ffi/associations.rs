use crate::context::Context;
use crate::trap::TrapKind;

// ----- Map / Set (stdlib.md §10, Q24) -----

pub(super) fn assoc_receiver_is_live(ctx: &mut Context, handle: *const u8, pos_id: u32) -> bool {
    ctx.require_live_handle(handle as usize, pos_id)
}

/// Begins the fixed-bound insertion-order traversal shared by Map/Set
/// `forEach`, fused `for…of`, and array-literal spread.
///
/// # Safety
///
/// `handle` is a live Map or Set payload.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_assoc_iter_begin(
    ctx: *mut Context,
    handle: *mut u8,
    pos_id: u32,
) -> u64 {
    // SAFETY: shared contract.
    if !assoc_receiver_is_live(unsafe { &mut *ctx }, handle, pos_id) {
        return 0;
    }
    // SAFETY: receiver was validated above.
    unsafe { crate::assocops::iteration_begin(handle) as u64 }
}

/// Copies one still-active ordered key/value during a fused traversal.
/// `value != 0` selects a Map value; zero selects a Map/Set key.
///
/// # Safety
///
/// `handle` is the receiver passed to [`subscript_rt_assoc_iter_begin`],
/// `index` is below its returned bound, and `out` is writable for the
/// selected monomorphized field.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_assoc_iter_copy(
    ctx: *mut Context,
    handle: *mut u8,
    index: u64,
    value: u32,
    out: *mut u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    if !assoc_receiver_is_live(unsafe { &mut *ctx }, handle, pos_id) {
        return 0;
    }
    // SAFETY: receiver and output follow the shared traversal ABI.
    i32::from(unsafe { crate::assocops::iteration_copy(handle, index as usize, value != 0, out) })
}

/// Ends a traversal begun by [`subscript_rt_assoc_iter_begin`].
///
/// # Safety
///
/// `ctx` is live; `handle` may have been deleted by script.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_assoc_iter_end(ctx: *mut Context, handle: *mut u8) {
    // SAFETY: forwarded contract.
    unsafe { crate::assocops::iteration_end(ctx, handle) };
}

/// Allocates an empty monomorphized `Map<K, V>`.
///
/// `key_size` / `value_size` are the calling tier's concrete storage
/// widths and `key_kind` is the compiler/runtime ABI tag. Backing entry
/// and index storage stays unallocated until `set`.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_new(
    ctx: *mut Context,
    key_size: u64,
    value_size: u64,
    key_kind: u32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(kind) = crate::assocops::KeyKind::from_u32(key_kind) else {
        ctx.trap(
            TrapKind::Internal,
            format!("unknown Map key-kind code {key_kind}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    crate::assocops::new(
        ctx,
        key_size as usize,
        value_size as usize,
        kind,
        false,
        pos_id,
    )
}

/// Copies a Map in insertion order through the shared insert path (stdlib.md §10.9).
///
/// # Safety
///
/// `ctx` is live and `source` is a live Map owned by that Context.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_from_assoc(
    ctx: *mut Context,
    source: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: the caller supplies the live Context.
    if !assoc_receiver_is_live(unsafe { &mut *ctx }, source, pos_id) {
        return std::ptr::null_mut();
    }
    // SAFETY: shared source and Context contract.
    unsafe { crate::assocops::copy_map(ctx, source, pos_id) }
}

/// Allocates an empty monomorphized `Set<K>`.
///
/// # Safety
///
/// As [`subscript_rt_map_new`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_new(
    ctx: *mut Context,
    key_size: u64,
    key_kind: u32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(kind) = crate::assocops::KeyKind::from_u32(key_kind) else {
        ctx.trap(
            TrapKind::Internal,
            format!("unknown Set key-kind code {key_kind}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    crate::assocops::new(ctx, key_size as usize, 0, kind, true, pos_id)
}

/// `Map.size` / `Set.size`.
///
/// # Safety
///
/// Shared contract; `handle` is a live Map/Set handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_assoc_size(ctx: *mut Context, handle: *const u8) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(ctx, handle, 0) {
        return 0;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::len(handle) }
}

/// `Map.set`: inserts or overwrites and returns the receiver.
///
/// # Safety
///
/// Shared contract; `map` is live and `key` / `value` point at values
/// of the monomorphized widths.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_set(
    ctx: *mut Context,
    map: *mut u8,
    key: *const u8,
    value: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, map, pos_id) {
        return map;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::insert(ctx, map, key, value, pos_id) }
}

/// `Set.add`: inserts and returns the receiver.
///
/// # Safety
///
/// Shared contract; `set` is live and `key` points at its
/// monomorphized key value.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_add(
    ctx: *mut Context,
    set: *mut u8,
    key: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, set, pos_id) {
        return set;
    }
    // SAFETY: caller contract; a set has zero-width values.
    unsafe { crate::assocops::insert(ctx, set, key, std::ptr::null(), pos_id) }
}

/// `Map.get`: copies a present value to `out`, returning 1; returns 0
/// on a miss without writing `out`.
///
/// # Safety
///
/// Shared contract; pointers match the map's monomorphized widths.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_get(
    ctx: *mut Context,
    map: *mut u8,
    key: *const u8,
    out: *mut u8,
) -> i32 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, map, 0) {
        return 0;
    }
    // SAFETY: caller contract.
    i32::from(unsafe { crate::assocops::get(ctx, map, key, out) })
}

/// `Map.getOr`: copies the present value or the supplied fallback.
///
/// # Safety
///
/// As [`subscript_rt_map_get`], and `fallback` is readable for the value width.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_get_or(
    ctx: *mut Context,
    map: *mut u8,
    key: *const u8,
    fallback: *const u8,
    out: *mut u8,
) {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, map, 0) {
        return;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::get_or(ctx, map, key, fallback, out, 0) };
}

/// `Map.has` / `Set.has`.
///
/// # Safety
///
/// Shared contract; `handle` and `key` match its monomorphized shape.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_assoc_has(
    ctx: *mut Context,
    handle: *mut u8,
    key: *const u8,
) -> i32 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, handle, 0) {
        return 0;
    }
    // SAFETY: caller contract.
    i32::from(unsafe { crate::assocops::has(ctx, handle, key) })
}

/// `Map.delete` / `Set.delete`.
///
/// # Safety
///
/// As [`subscript_rt_assoc_has`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_assoc_delete(
    ctx: *mut Context,
    handle: *mut u8,
    key: *const u8,
) -> i32 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, handle, 0) {
        return 0;
    }
    // SAFETY: caller contract.
    i32::from(unsafe { crate::assocops::delete(ctx, handle, key) })
}

/// `Map.clear` / `Set.clear`: eagerly retires ordered and index storage.
///
/// # Safety
///
/// Shared contract; `handle` is a live Map/Set handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_assoc_clear(ctx: *mut Context, handle: *mut u8) {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, handle, 0) {
        return;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::clear(&mut *ctx, handle) };
}

/// `Map.forEach` in insertion order.
///
/// `bridge` is a generated fixed-ABI adapter that loads the map's
/// concrete `V` / `K` and calls `code(ctx, env, value, key)`. The
/// runtime checks the Context trap flag after every bridge return.
///
/// # Safety
///
/// Shared contract; handles and function pointers have the documented
/// generated signatures.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_for_each(
    ctx: *mut Context,
    map: *mut u8,
    code: *const u8,
    env: *const u8,
    bridge: *const u8,
) {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, map, 0) {
        return;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::map_for_each(ctx, map, code, env, bridge) };
}

/// `Set.forEach` in insertion order, through a generated key bridge.
///
/// # Safety
///
/// As [`subscript_rt_map_for_each`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_for_each(
    ctx: *mut Context,
    set: *mut u8,
    code: *const u8,
    env: *const u8,
    bridge: *const u8,
) {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    if !assoc_receiver_is_live(runtime, set, 0) {
        return;
    }
    // SAFETY: caller contract.
    unsafe { crate::assocops::set_for_each(ctx, set, code, env, bridge) };
}

/// `Map.groupBy(items, callback)`: returns a fresh insertion-ordered map
/// whose values are fresh arrays of source elements.
///
/// # Safety
///
/// Shared contract; handles, widths, and function pointers have the
/// generated signatures.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_map_group_by(
    ctx: *mut Context,
    items: *mut u8,
    code: *const u8,
    env: *const u8,
    bridge: *const u8,
    key_size: u64,
    key_kind: u32,
    pos_id: u32,
) -> *mut u8 {
    let runtime = unsafe { &mut *ctx };
    if !runtime.require_live_handle(items as usize, pos_id) {
        return std::ptr::null_mut();
    }
    let Some(kind) = crate::assocops::KeyKind::from_u32(key_kind) else {
        runtime.trap(
            TrapKind::Internal,
            format!("unknown Map.groupBy key-kind code {key_kind}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    unsafe {
        crate::assocops::group_by(
            ctx,
            items,
            code,
            env,
            bridge,
            key_size as usize,
            kind,
            pos_id,
        )
    }
}

unsafe fn set_pair_is_live(ctx: *mut Context, left: *mut u8, right: *mut u8, pos_id: u32) -> bool {
    let runtime = unsafe { &mut *ctx };
    assoc_receiver_is_live(runtime, left, pos_id) && assoc_receiver_is_live(runtime, right, pos_id)
}

/// `Set.union`: returns a fresh result in ES2024 order.
///
/// # Safety
///
/// Shared contract; both operands are live `Set<K>` handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_union(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    if !unsafe { set_pair_is_live(ctx, left, right, pos_id) } {
        return std::ptr::null_mut();
    }
    unsafe { crate::assocops::set_union(ctx, left, right, pos_id) }
}

/// `Set.intersection`: returns a fresh result in ES2024 order.
///
/// # Safety
///
/// As [`subscript_rt_set_union`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_intersection(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    if !unsafe { set_pair_is_live(ctx, left, right, pos_id) } {
        return std::ptr::null_mut();
    }
    unsafe { crate::assocops::set_intersection(ctx, left, right, pos_id) }
}

/// `Set.difference`: returns a fresh receiver-minus-argument result.
///
/// # Safety
///
/// As [`subscript_rt_set_union`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_difference(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    if !unsafe { set_pair_is_live(ctx, left, right, pos_id) } {
        return std::ptr::null_mut();
    }
    unsafe { crate::assocops::set_difference(ctx, left, right, pos_id) }
}

/// `Set.symmetricDifference`: returns a fresh receiver-then-argument
/// result.
///
/// # Safety
///
/// As [`subscript_rt_set_union`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_symmetric_difference(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    if !unsafe { set_pair_is_live(ctx, left, right, pos_id) } {
        return std::ptr::null_mut();
    }
    unsafe { crate::assocops::set_symmetric_difference(ctx, left, right, pos_id) }
}

/// `Set.isSubsetOf`.
///
/// # Safety
///
/// Shared contract; both operands are live `Set<K>` handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_is_subset_of(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
) -> i32 {
    if !unsafe { set_pair_is_live(ctx, left, right, 0) } {
        return 0;
    }
    i32::from(unsafe { crate::assocops::set_is_subset_of(ctx, left, right) })
}

/// `Set.isSupersetOf`.
///
/// # Safety
///
/// As [`subscript_rt_set_is_subset_of`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_is_superset_of(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
) -> i32 {
    if !unsafe { set_pair_is_live(ctx, left, right, 0) } {
        return 0;
    }
    i32::from(unsafe { crate::assocops::set_is_superset_of(ctx, left, right) })
}

/// `Set.isDisjointFrom`.
///
/// # Safety
///
/// As [`subscript_rt_set_is_subset_of`].
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_set_is_disjoint_from(
    ctx: *mut Context,
    left: *mut u8,
    right: *mut u8,
) -> i32 {
    if !unsafe { set_pair_is_live(ctx, left, right, 0) } {
        return 0;
    }
    i32::from(unsafe { crate::assocops::set_is_disjoint_from(ctx, left, right) })
}
