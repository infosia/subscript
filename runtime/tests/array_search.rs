use std::ptr;
use subscript_runtime::{arrops, ffi, Context, TrapKind};

unsafe extern "C" fn is_one(_: *mut Context, _: *const u8, value: usize, _: i32) -> u8 {
    u8::from(value == 1)
}
unsafe extern "C" fn expand(ctx: *mut Context, _: *const u8, value: i32, index: i32) -> *mut u8 {
    // SAFETY: the test supplies a live exclusive Context.
    let ctx = unsafe { &mut *ctx };
    let out = ctx.array_new(4, 0);
    for v in [value, index] {
        // SAFETY: a live four-byte array and a readable i32.
        unsafe { ctx.array_push(out, (&raw const v).cast(), 0) };
    }
    out
}

#[test]
fn reference_search_exports_and_operations_return_values_and_indices() {
    let mut ctx = Context::new();
    let a = ctx.array_new(size_of::<usize>(), 0);
    for v in [1usize, 2, 1] {
        // SAFETY: matching element width.
        unsafe { ctx.array_push(a, (&raw const v).cast(), 0) };
    }
    let ctx = &mut *ctx as *mut Context;
    let callback = is_one as *const u8;
    // SAFETY: each callback uses the indexed word ABI; the array and Context are live.
    unsafe {
        assert_eq!(
            ffi::subscript_rt_arr_find(ctx, a, callback, ptr::null(), 0, 1) as usize,
            1
        );
        assert_eq!(
            ffi::subscript_rt_arr_find_last(ctx, a, callback, ptr::null(), 0, 1) as usize,
            1
        );
        assert_eq!(
            ffi::subscript_rt_arr_find_last_index(ctx, a, callback, ptr::null(), 0, 1),
            2
        );
        assert_eq!(
            arrops::find(
                ctx,
                a,
                callback,
                ptr::null(),
                arrops::ElemKind::Int,
                true,
                false
            ) as usize,
            1
        );
        assert_eq!(
            arrops::find(
                ctx,
                a,
                callback,
                ptr::null(),
                arrops::ElemKind::Int,
                true,
                true
            ) as usize,
            1
        );
        assert_eq!(
            arrops::find_last_index(ctx, a, callback, ptr::null(), arrops::ElemKind::Int, true),
            2
        );
        let empty = (*ctx).array_new(size_of::<usize>(), 0);
        assert!(ffi::subscript_rt_arr_find(ctx, empty, callback, ptr::null(), 0, 1).is_null());
        assert!(ffi::subscript_rt_arr_find_last(ctx, empty, callback, ptr::null(), 0, 1).is_null());
        assert_eq!(
            ffi::subscript_rt_arr_find_last_index(ctx, empty, callback, ptr::null(), 0, 1),
            -1
        );
    }
}

#[test]
fn flat_map_export_and_operation_share_mapping_and_append() {
    let mut ctx = Context::new();
    let a = ctx.array_new(4, 0);
    for v in [4i32, 5] {
        // SAFETY: matching element width.
        unsafe { ctx.array_push(a, (&raw const v).cast(), 0) };
    }
    let ctx = &mut *ctx as *mut Context;
    // SAFETY: the callback uses the indexed i32 ABI and returns a live i32 array.
    unsafe {
        let outputs = [
            ffi::subscript_rt_arr_flat_map(ctx, a, expand as *const u8, ptr::null(), 5, 5, 4, 9, 1),
            arrops::flat_map(
                ctx,
                a,
                (expand as *const u8, ptr::null()),
                arrops::ElemKind::SignedInt,
                4,
                9,
                true,
            ),
        ];
        for out in outputs {
            assert_eq!((*ctx).array_len(out), 4);
            for (index, expected) in [4, 0, 5, 1].into_iter().enumerate() {
                let value = (*ctx)
                    .array_elem_ptr(out, index as i32, 0)
                    .cast::<i32>()
                    .read_unaligned();
                assert_eq!(value, expected);
            }
        }
    }
}

#[test]
fn at_exports_read_signed_indices_and_preserve_range_traps() {
    let mut ctx = Context::new();
    let a = ctx.array_new(4, 0);
    let v = 42i32;
    // SAFETY: live handles and matching readable/writable storage.
    unsafe {
        ctx.array_push(a, (&raw const v).cast(), 0);
        let mut out = 0i32;
        ffi::subscript_rt_arr_at(&mut *ctx, a, -1, (&raw mut out).cast(), 7);
        assert_eq!(out, 42);
        let s = ctx.alloc_str("hé".as_bytes(), 0);
        let text = ffi::subscript_rt_str_at(&mut *ctx, s, -2, 8);
        assert_eq!(ctx.str_bytes(text), "é".as_bytes());
        ffi::subscript_rt_arr_at(&mut *ctx, a, -2, (&raw mut out).cast(), 9);
    }
    assert_eq!(ctx.trap_record().unwrap().kind, TrapKind::IndexOutOfBounds);
}
