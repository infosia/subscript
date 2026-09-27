use subscript_runtime::{arrops, ffi, strops, Context};

#[test]
fn string_last_index_clamps_positions() {
    let mut ctx = Context::new();
    let hay = ctx.alloc_str(b"abcabc", 0);
    for (needle, position, expected) in [
        (&b"c"[..], -5, -1),
        (b"a", -5, 0),
        (b"a", 0, 0),
        (b"c", 3, 2),
        (b"c", 100, 5),
        (b"", -1, 0),
        (b"", 0, 0),
        (b"", 2, 2),
        (b"", 99, 6),
    ] {
        assert_eq!(strops::last_index_of(b"abcabc", needle, position), expected);
        let needle = ctx.alloc_str(needle, 0);
        // SAFETY: both strings belong to the live Context.
        assert_eq!(
            unsafe { ffi::subscript_rt_str_last_index_of(&mut *ctx, hay, needle, position) },
            expected
        );
    }
}

#[test]
fn array_search_positions_clamp_in_both_directions() {
    let mut ctx = Context::new();
    let array = ctx.array_new(4, 0);
    for value in [1i32, 2, 3, 1, 2] {
        // SAFETY: the live array stores i32 elements.
        unsafe { ctx.array_push(array, (&raw const value).cast(), 0) };
    }
    let needle = 1i32;
    let ptr = (&raw const needle).cast();
    for (from, first, last) in [
        (i32::MIN, 0, -1),
        (-100, 0, -1),
        (-3, 3, 0),
        (-2, 3, 3),
        (-1, -1, 3),
        (0, 0, 0),
        (1, 3, 0),
        (2, 3, 0),
        (5, -1, 3),
        (99, -1, 3),
        (i32::MAX, -1, 3),
    ] {
        // SAFETY: the array and needle store i32 elements in the live Context.
        unsafe {
            assert_eq!(
                arrops::index_of(&mut *ctx, array, ptr, arrops::ElemKind::Int, from),
                first
            );
            assert_eq!(
                arrops::last_index_of(&mut *ctx, array, ptr, arrops::ElemKind::Int, from),
                last
            );
            assert_eq!(
                arrops::includes(&mut *ctx, array, ptr, arrops::ElemKind::Int, from),
                i32::from(first >= 0)
            );
            assert_eq!(
                ffi::subscript_rt_arr_index_of(&mut *ctx, array, ptr, 0, from),
                first
            );
            assert_eq!(
                ffi::subscript_rt_arr_last_index_of(&mut *ctx, array, ptr, 0, from),
                last
            );
            assert_eq!(
                ffi::subscript_rt_arr_includes(&mut *ctx, array, ptr, 0, from),
                i32::from(first >= 0)
            );
        }
    }
    let empty = ctx.array_new(4, 0);
    for from in [-1, 0, i32::MAX] {
        // SAFETY: the empty array and needle have matching element storage.
        unsafe {
            assert_eq!(
                ffi::subscript_rt_arr_index_of(&mut *ctx, empty, ptr, 0, from),
                -1
            );
            assert_eq!(
                ffi::subscript_rt_arr_last_index_of(&mut *ctx, empty, ptr, 0, from),
                -1
            );
            assert_eq!(
                ffi::subscript_rt_arr_includes(&mut *ctx, empty, ptr, 0, from),
                0
            );
        }
    }
}

#[test]
fn split_limits_apply_to_strings_regexes_and_captures() {
    let mut ctx = Context::new();
    let subject = ctx.alloc_str(b"a,b,c", 0);
    let separator = ctx.alloc_str(b",", 0);
    let flags = ctx.alloc_str(b"", 0);
    // SAFETY: pattern and flags belong to the Context.
    let regex = unsafe { ffi::subscript_rt_regex_new(&mut *ctx, separator, flags, 0) };
    for (limit, expected) in [(-1, 3), (i32::MIN, 3), (0, 0), (1, 1), (2, 2), (99, 3)] {
        // SAFETY: all input handles belong to the live Context.
        unsafe {
            let literal = ffi::subscript_rt_str_split(&mut *ctx, subject, separator, limit, 0);
            let regular = ffi::subscript_rt_regex_split(&mut *ctx, subject, regex, limit, 0);
            for array in [literal, regular] {
                assert_eq!(ffi::subscript_rt_array_len(&mut *ctx, array), expected);
                for index in 0..expected {
                    let handle = ctx
                        .array_elem_ptr(array, index, 0)
                        .cast::<*const u8>()
                        .read_unaligned();
                    assert_eq!(ctx.str_bytes(handle), [b"a", b"b", b"c"][index as usize]);
                }
            }
        }
    }
    let capture = ctx.alloc_str(b"(,)", 0);
    // SAFETY: pattern and flags belong to the Context.
    let regex = unsafe { ffi::subscript_rt_regex_new(&mut *ctx, capture, flags, 0) };
    for limit in 1..=5 {
        // SAFETY: all handles and the returned array belong to the live Context.
        unsafe {
            let array = ffi::subscript_rt_regex_split(&mut *ctx, subject, regex, limit, 0);
            assert_eq!(ffi::subscript_rt_array_len(&mut *ctx, array), limit);
            for index in 0..limit {
                let handle = ctx
                    .array_elem_ptr(array, index, 0)
                    .cast::<*const u8>()
                    .read_unaligned();
                assert_eq!(
                    ctx.str_bytes(handle),
                    [b"a", b",", b"b", b",", b"c"][index as usize]
                );
            }
        }
    }
}

#[test]
fn splice_to_end_clamps_start() {
    for (start, expected) in [
        (-9, vec![1, 2, 3]),
        (-1, vec![3]),
        (0, vec![1, 2, 3]),
        (9, vec![]),
    ] {
        let mut ctx = Context::new();
        let array = ctx.array_new(4, 0);
        for value in [1i32, 2, 3] {
            // SAFETY: the array stores i32 elements and the source is readable.
            unsafe { ctx.array_push(array, (&raw const value).cast(), 0) };
        }
        // SAFETY: the array belongs to the live Context.
        unsafe {
            let removed = ffi::subscript_rt_arr_splice(&mut *ctx, array, start, i32::MAX, 0);
            assert_eq!(
                ffi::subscript_rt_array_len(&mut *ctx, removed) as usize,
                expected.len()
            );
            assert_eq!(
                ffi::subscript_rt_array_len(&mut *ctx, array) as usize,
                3 - expected.len()
            );
            for (index, value) in expected.into_iter().enumerate() {
                assert_eq!(
                    ctx.array_elem_ptr(removed, index as i32, 0)
                        .cast::<i32>()
                        .read_unaligned(),
                    value
                );
            }
        }
    }
}
