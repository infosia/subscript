//! Counted Map values end when their stored slots end (§172).

use super::*;

// Generated code installs the description before the first store.
pub(crate) unsafe fn describe(ctx: &mut Context, handle: *mut u8, description: *const u8) {
    let h = unsafe { &mut *handle.cast::<AssocHeader>() };
    if h.len != 0 {
        ctx.trap(
            TrapKind::Internal,
            "Map description requires an empty Map",
            0,
        );
        return;
    }
    h.value_description = description;
    if !description.is_null() {
        ctx.counted_maps.insert(handle as usize, Vec::new());
    }
}

/// Eagerly retires a container's entry and bucket allocations and resets
/// it to the empty state. This is also called by `Context::delete` before
/// deleting a Map/Set header.
///
/// # Safety
///
/// `handle` is a live `AssocHeader` owned by `ctx`.
pub(crate) unsafe fn clear(ctx: &mut Context, handle: *mut u8) {
    let Some(h) = (unsafe { header(handle) }) else {
        return;
    };
    let description = h.value_description;
    let values = if description.is_null() {
        Vec::new()
    } else {
        (0..h.order_len as usize)
            .filter(|&index| unsafe { entry_active(h, index) })
            .map(|index| {
                unsafe { std::slice::from_raw_parts(entry_value(h, index), h.value_size as usize) }
                    .to_vec()
            })
            .collect::<Vec<_>>()
    };
    let entries = h.entries;
    let buckets = h.buckets;
    h.len = 0;
    h.order_len = 0;
    h.order_cap = 0;
    h.bucket_cap = 0;
    h.tombstones = 0;
    h.entries = std::ptr::null_mut();
    h.buckets = std::ptr::null_mut();
    for value in values {
        unsafe { ctx.counted_value(value.as_ptr(), description, true, 0) };
    }
    if !entries.is_null() {
        ctx.delete(entries as usize, 0);
    }
    if !buckets.is_null() {
        ctx.delete(buckets as usize, 0);
    }
}

// Snapshot every active value before any allocation can be freed.
pub(crate) unsafe fn release_leaves(
    ctx: &mut Context,
    handle: *mut u8,
    description: *const u8,
    handles: &mut Vec<usize>,
    storage: &mut Vec<usize>,
) {
    let h = unsafe { &*handle.cast::<AssocHeader>() };
    let description = if description.is_null() {
        h.value_description
    } else {
        description
    };
    if description.is_null() {
        return;
    }
    for index in 0..h.order_len as usize {
        if unsafe { entry_active(h, index) } {
            unsafe {
                ctx.counted_release_leaves(entry_value(h, index), description, 0, handles, storage);
            }
        }
    }
}
