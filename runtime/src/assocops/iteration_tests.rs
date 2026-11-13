use super::*;

#[test]
fn cursor_bounds_and_frame_release_preserve_nested_visit_depth() {
    let mut ctx = Context::new();
    let map = new(&mut ctx, 4, 4, KeyKind::Bits, false, 0);
    let description = [6u64, 0, 0];
    // SAFETY: the map is live; the cursor starts with its receiver pointer.
    unsafe {
        assert_eq!(iteration_begin(map), 0);
        assert_eq!(iteration_begin(map), 0);
        for _ in 0..4 {
            assert_eq!(
                crate::ffi::subscript_rt_assoc_iter_bound(&mut *ctx, map, 0),
                0
            );
        }
        assert_eq!((*map.cast::<AssocHeader>()).iteration_depth, 2);
        ctx.counted_value(
            (&raw const map).cast(),
            description.as_ptr().cast(),
            true,
            0,
        );
        assert_eq!((*map.cast::<AssocHeader>()).iteration_depth, 1);
        iteration_end(&mut *ctx, map);
        assert_eq!((*map.cast::<AssocHeader>()).iteration_depth, 0);
    }
}
