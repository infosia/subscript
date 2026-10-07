//! Shallow Map copies through the shared insert path (stdlib.md §10.9).
use super::*;

/// Copies each active entry in insertion order.
///
/// # Safety
/// `ctx` is live and owns the live Map `source`.
pub(crate) unsafe fn copy_map(ctx: *mut Context, source: *mut u8, pos_id: u32) -> *mut u8 {
    // SAFETY: the caller supplies a live Map.
    let h = unsafe { &*source.cast::<AssocHeader>() };
    let out = new(
        unsafe { &mut *ctx },
        h.key_size as usize,
        h.value_size as usize,
        header_kind(h),
        false,
        pos_id,
    );
    if out.is_null() {
        return out;
    }
    unsafe { crate::assocops::describe(&mut *ctx, out, h.value_description) };
    // SAFETY: the source remains live throughout this traversal.
    let bound = unsafe { iteration_begin(source) };
    for index in 0..bound {
        // SAFETY: the index is below the traversal bound.
        if let Some((key, value)) = unsafe { iteration_entry(source, index) } {
            // SAFETY: source fields have the destination's exact storage widths.
            unsafe { insert(ctx, out, key, value, pos_id) };
            if unsafe { (*ctx).trapped() } {
                break;
            }
            // The source owns the value throughout insertion. A failed store acquires nothing.
            if !h.value_description.is_null() {
                unsafe { (*ctx).counted_value(value, h.value_description, false, pos_id) };
            }
        }
    }
    // SAFETY: this balances iteration_begin on the live source.
    unsafe { iteration_end(ctx, source) };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_map_preserves_an_empty_shape() {
        let mut context = Context::new();
        let source = new(&mut context, 4, 8, KeyKind::Bits, false, 0);
        // SAFETY: the source is a live Map in this Context.
        let copy = unsafe { copy_map(&mut *context, source, 0) };
        assert!(!copy.is_null());
        assert_ne!(copy, source);
        // SAFETY: copy_map returned a live Map.
        let header = unsafe { &*copy.cast::<AssocHeader>() };
        assert_eq!(header.key_size, 4);
        assert_eq!(header.value_size, 8);
        assert_eq!(header.len, 0);
    }
}
