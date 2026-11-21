//! Unit tests of the `Array` method operations (stdlib.md §9, Q22).

use super::*;

/// A fresh dev-policy context.
fn ctx() -> Box<Context> {
    Context::new()
}

/// Builds an `i32[]` from `values`.
fn arr_i32(ctx: &mut Context, values: &[i32]) -> *mut u8 {
    let h = ctx.array_new(4, 0);
    assert!(!h.is_null());
    for v in values {
        // SAFETY: `h` is a live 4-byte-element array; `v` readable.
        unsafe { ctx.array_push(h, (v as *const i32).cast(), 0) };
    }
    h
}

/// Builds an `f64[]` from `values`.
fn arr_f64(ctx: &mut Context, values: &[f64]) -> *mut u8 {
    let h = ctx.array_new(8, 0);
    assert!(!h.is_null());
    for v in values {
        // SAFETY: as `arr_i32`, with 8-byte elements.
        unsafe { ctx.array_push(h, (v as *const f64).cast(), 0) };
    }
    h
}

/// Builds a `string[]` from `values`.
fn arr_str(ctx: &mut Context, values: &[&str]) -> *mut u8 {
    let h = ctx.array_new(8, 0);
    assert!(!h.is_null());
    for v in values {
        let s = ctx.alloc_str(v.as_bytes(), 0) as u64;
        // SAFETY: as `arr_i32`, with 8-byte handle elements.
        unsafe { ctx.array_push(h, (&s as *const u64).cast(), 0) };
    }
    h
}

fn assert_shifted_element_is_unreachable(mut ctx: Box<Context>, tier: &str) {
    let x = ctx.alloc(16, 1, 0);
    let h = ctx.array_new(std::mem::size_of::<usize>(), 0);
    let mut root = h as usize;
    // SAFETY: root lives through collection; Context drop does not scan roots (section 263).
    unsafe { ctx.root_add((&mut root as *mut usize) as usize, 1) };
    let value = x as usize;
    // SAFETY: `h` is a live word array, and `value` is readable.
    unsafe { ctx.array_push(h, (&raw const value).cast(), 0) };
    let mut out = 0usize;
    // SAFETY: `h` is a nonempty live word array, and `out` is writable.
    unsafe { shift(&mut *ctx, h, (&raw mut out).cast(), 0) };

    ctx.collect();

    assert!(!ctx.is_live(x as usize), "{tier}: shifted element");
}

fn assert_spliced_element_is_unreachable(mut ctx: Box<Context>, tier: &str) {
    let x = ctx.alloc(16, 1, 0);
    let h = ctx.array_new(std::mem::size_of::<usize>(), 0);
    let mut root = h as usize;
    // SAFETY: root lives through collection; Context drop does not scan roots (section 263).
    unsafe { ctx.root_add((&mut root as *mut usize) as usize, 1) };
    let value = x as usize;
    // SAFETY: `h` is a live word array, and `value` is readable.
    unsafe { ctx.array_push(h, (&raw const value).cast(), 0) };
    // SAFETY: `h` is a live one-element array owned by `ctx`.
    let removed = unsafe { splice(&mut *ctx, h, 0, 1, 0) };
    assert!(!removed.is_null(), "{tier}: splice result");

    ctx.collect();

    assert!(!ctx.is_live(x as usize), "{tier}: spliced element");
}

#[test]
fn shifted_element_is_unreachable_after_shift() {
    assert_shifted_element_is_unreachable(Context::new(), "dev");
    assert_shifted_element_is_unreachable(Context::new_releasing(), "ship");
}

#[test]
fn spliced_element_is_unreachable_after_splice() {
    assert_spliced_element_is_unreachable(Context::new(), "dev");
    assert_spliced_element_is_unreachable(Context::new_releasing(), "ship");
}

fn i32_items(ctx: &Context, h: *const u8) -> Vec<i32> {
    // SAFETY: `h` is a live 4-byte-element array of `ctx`.
    unsafe {
        let n = ctx.array_len(h) as usize;
        let data = ctx.array_data(h);
        (0..n)
            .map(|i| data.add(i * 4).cast::<i32>().read_unaligned())
            .collect()
    }
}

unsafe extern "C" fn double_i32(_ctx: *mut Context, _env: *const u8, v: i32) -> i32 {
    v * 2
}

unsafe extern "C" fn i32_to_f64(_ctx: *mut Context, _env: *const u8, v: i32) -> f64 {
    f64::from(v) + 0.5
}

unsafe extern "C" fn is_even(_ctx: *mut Context, _env: *const u8, v: i32) -> u8 {
    u8::from(v % 2 == 0)
}

unsafe extern "C" fn add_into_env(_ctx: *mut Context, env: *const u8, v: i32) {
    // SAFETY: the test passes a pointer to an i64 accumulator.
    unsafe { *(env as *mut i64) += i64::from(v) };
}

unsafe extern "C" fn count_and_test(_ctx: *mut Context, env: *const u8, v: i32) -> u8 {
    // SAFETY: the test passes a pointer to an i64 counter.
    unsafe { *(env as *mut i64) += 1 };
    u8::from(v >= 3)
}

unsafe extern "C" fn sum_f64_i32(_ctx: *mut Context, _env: *const u8, acc: f64, v: i32) -> f64 {
    acc + f64::from(v)
}

unsafe extern "C" fn append_digit(_ctx: *mut Context, _env: *const u8, acc: i32, v: i32) -> i32 {
    acc * 10 + v
}

unsafe extern "C" fn add_index(_ctx: *mut Context, _env: *const u8, v: i32, i: i32) -> i32 {
    v + i
}

unsafe extern "C" fn odd_index(_ctx: *mut Context, _env: *const u8, _v: i32, i: i32) -> u8 {
    u8::from(i % 2 != 0)
}

unsafe extern "C" fn append_index(
    _ctx: *mut Context,
    _env: *const u8,
    acc: i32,
    _v: i32,
    i: i32,
) -> i32 {
    acc * 10 + i
}

unsafe extern "C" fn record_index(_ctx: *mut Context, env: *const u8, _v: i32, i: i32) {
    // SAFETY: the test passes a pointer to an i32 accumulator.
    unsafe { *(env as *mut i32) = *(env as *mut i32) * 10 + i };
}

unsafe extern "C" fn cmp_i32(_ctx: *mut Context, _env: *const u8, a: i32, b: i32) -> i32 {
    a - b
}

/// Comparator over key-encoded pairs (`key * 10 + seq`): orders by
/// key only, so the seq digits expose stability.
unsafe extern "C" fn cmp_key(_ctx: *mut Context, _env: *const u8, a: i32, b: i32) -> i32 {
    a / 10 - b / 10
}

unsafe extern "C" fn trap_on_two(ctx: *mut Context, _env: *const u8, v: i32) -> i32 {
    if v == 2 {
        // SAFETY: `ctx` is the live test context.
        unsafe { &mut *ctx }.trap(TrapKind::IndexOutOfBounds, "test trap", 42);
    }
    v
}

unsafe extern "C" fn cmp_trapping(ctx: *mut Context, _env: *const u8, a: i32, b: i32) -> i32 {
    // SAFETY: `ctx` is the live test context.
    unsafe { &mut *ctx }.trap(TrapKind::IndexOutOfBounds, "cmp trap", 7);
    a - b
}

#[test]
fn elem_and_fmt_kind_codes_round_trip() {
    for (v, k) in [
        (0, ElemKind::Int),
        (1, ElemKind::F32),
        (2, ElemKind::F64),
        (3, ElemKind::Str),
        (4, ElemKind::F16),
        (5, ElemKind::SignedInt),
    ] {
        assert_eq!(ElemKind::from_u32(v), Some(k));
    }
    assert_eq!(ElemKind::from_u32(9), None);
    for (v, k) in [
        (0, FmtKind::I32),
        (1, FmtKind::U32),
        (2, FmtKind::I64),
        (3, FmtKind::U64),
        (4, FmtKind::F32),
        (5, FmtKind::F64),
        (6, FmtKind::Bool),
        (7, FmtKind::Str),
        (8, FmtKind::I8),
        (9, FmtKind::U8),
        (10, FmtKind::I16),
        (11, FmtKind::U16),
        (12, FmtKind::F16),
    ] {
        assert_eq!(FmtKind::from_u32(v), Some(k));
    }
    assert_eq!(FmtKind::from_u32(13), None);
}

#[test]
fn abi_dispatch_is_kind_plus_width() {
    assert_eq!(abi_of(ElemKind::Int, 1), Some(Abi::U8));
    assert_eq!(abi_of(ElemKind::Int, 2), Some(Abi::U16));
    assert_eq!(abi_of(ElemKind::Int, 4), Some(Abi::U32));
    assert_eq!(abi_of(ElemKind::Int, 8), Some(Abi::U64));
    assert_eq!(abi_of(ElemKind::SignedInt, 1), Some(Abi::S8));
    assert_eq!(abi_of(ElemKind::SignedInt, 2), Some(Abi::S16));
    assert_eq!(abi_of(ElemKind::SignedInt, 4), Some(Abi::I32));
    assert_eq!(abi_of(ElemKind::SignedInt, 8), Some(Abi::I64));
    assert_eq!(abi_of(ElemKind::Str, 8), Some(Abi::U64));
    assert_eq!(abi_of(ElemKind::F16, 2), Some(Abi::U16));
    assert_eq!(abi_of(ElemKind::F32, 4), Some(Abi::F32));
    assert_eq!(abi_of(ElemKind::F64, 8), Some(Abi::F64));
    assert_eq!(abi_of(ElemKind::Int, 3), None);
    assert_eq!(abi_of(ElemKind::F32, 8), None);
    assert_eq!(abi_of(ElemKind::F64, 4), None);
    assert_eq!(abi_of(ElemKind::F16, 4), None);
}

#[test]
fn index_of_family_int_and_float_and_str() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    let h = arr_i32(&mut c, &[4, 7, 4, 9]);
    let (n4, n5, n7) = (4i32, 5i32, 7i32);
    let needle = |v: &i32| (v as *const i32).cast::<u8>();
    // SAFETY: live arrays/handles of `c`; needles readable.
    unsafe {
        assert_eq!(index_of(p, h, needle(&n4), ElemKind::Int, 0), 0);
        assert_eq!(last_index_of(p, h, needle(&n4), ElemKind::Int, i32::MAX), 2);
        assert_eq!(index_of(p, h, needle(&n5), ElemKind::Int, 0), -1);
        assert_eq!(includes(p, h, needle(&n7), ElemKind::Int, 0), 1);
        assert_eq!(includes(p, h, needle(&n5), ElemKind::Int, 0), 0);

        // Floats: -0 == 0 under both rules. SameValueZero makes
        // `includes` find NaN while index searches retain `===`.
        let f = arr_f64(&mut c, &[0.0, 1.5, f64::NAN]);
        let nz = -0.0f64;
        assert_eq!(
            index_of(p, f, (&nz as *const f64).cast(), ElemKind::F64, 0),
            0
        );
        let nan = f64::NAN;
        assert_eq!(
            index_of(p, f, (&nan as *const f64).cast(), ElemKind::F64, 0),
            -1
        );
        assert_eq!(
            last_index_of(p, f, (&nan as *const f64).cast(), ElemKind::F64, i32::MAX),
            -1
        );
        assert_eq!(
            includes(p, f, (&nan as *const f64).cast(), ElemKind::F64, 0),
            1
        );

        // Strings by content: a distinct allocation still matches.
        let s = arr_str(&mut c, &["alpha", "beta"]);
        let fresh = c.alloc_str(b"beta", 0) as u64;
        assert_eq!(
            index_of(p, s, (&fresh as *const u64).cast(), ElemKind::Str, 0),
            1
        );
        let miss = c.alloc_str(b"gamma", 0) as u64;
        assert_eq!(
            index_of(p, s, (&miss as *const u64).cast(), ElemKind::Str, 0),
            -1
        );
    }
}

#[test]
fn join_formats_per_kind_with_separator() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    let sep = c.alloc_str(b", ", 0);
    // SAFETY: live arrays/handles of `c`.
    unsafe {
        let h = arr_i32(&mut c, &[1, -2, 3]);
        let out = join(p, h, sep, FmtKind::I32, 0);
        assert_eq!(c.str_bytes(out), b"1, -2, 3");

        let f = arr_f64(&mut c, &[0.5, -0.0, 7.0]);
        let out = join(p, f, sep, FmtKind::F64, 0);
        assert_eq!(c.str_bytes(out), b"0.5, 0, 7");

        let i8s = c.array_new(1, 0);
        for v in [-128i8, 127i8] {
            c.array_push(i8s, (&v as *const i8).cast(), 0);
        }
        let out = join(p, i8s, sep, FmtKind::I8, 0);
        assert_eq!(c.str_bytes(out), b"-128, 127");

        let u16s = c.array_new(2, 0);
        for v in [0u16, u16::MAX] {
            c.array_push(u16s, (&v as *const u16).cast(), 0);
        }
        let out = join(p, u16s, sep, FmtKind::U16, 0);
        assert_eq!(c.str_bytes(out), b"0, 65535");

        let halves = c.array_new(2, 0);
        for v in [1.5, -0.0] {
            let bits = crate::half::from_f64(v);
            c.array_push(halves, (&bits as *const u16).cast(), 0);
        }
        let out = join(p, halves, sep, FmtKind::F16, 0);
        assert_eq!(c.str_bytes(out), b"1.5, 0");

        let s = arr_str(&mut c, &["a", "b"]);
        let out = join(p, s, sep, FmtKind::Str, 0);
        assert_eq!(c.str_bytes(out), b"a, b");

        // Booleans at width 1 (dev tier) and width 4 (ship-C tier).
        let b1 = c.array_new(1, 0);
        for v in [1u8, 0u8] {
            c.array_push(b1, (&v as *const u8).cast(), 0);
        }
        let out = join(p, b1, sep, FmtKind::Bool, 0);
        assert_eq!(c.str_bytes(out), b"true, false");
        let b4 = arr_i32(&mut c, &[0, 1]);
        let out = join(p, b4, sep, FmtKind::Bool, 0);
        assert_eq!(c.str_bytes(out), b"false, true");

        // Empty array joins to "".
        let e = arr_i32(&mut c, &[]);
        let out = join(p, e, sep, FmtKind::I32, 0);
        assert_eq!(c.str_bytes(out), b"");
    }
}

#[test]
fn slice_applies_js_negative_and_clamp_rules() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    let h = arr_i32(&mut c, &[10, 20, 30, 40, 50]);
    // SAFETY: live arrays of `c`.
    unsafe {
        assert_eq!(i32_items(&c, slice(p, h, 1, 3, 0)), vec![20, 30]);
        assert_eq!(i32_items(&c, slice(p, h, -2, i32::MAX, 0)), vec![40, 50]);
        assert_eq!(i32_items(&c, slice(p, h, 0, -1, 0)), vec![10, 20, 30, 40]);
        assert_eq!(i32_items(&c, slice(p, h, 3, 99, 0)), vec![40, 50]);
        assert_eq!(i32_items(&c, slice(p, h, 4, 2, 0)), Vec::<i32>::new());
        // The receiver is untouched.
        assert_eq!(i32_items(&c, h), vec![10, 20, 30, 40, 50]);
    }
}

#[test]
fn fill_reverse_concat() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    // SAFETY: live arrays of `c`; fill values readable.
    unsafe {
        let h = arr_i32(&mut c, &[1, 2, 3, 4, 5]);
        let z = 0i32;
        fill(p, h, (&z as *const i32).cast(), 1, 3);
        assert_eq!(i32_items(&c, h), vec![1, 0, 0, 4, 5]);
        let s = 7i32;
        fill(p, h, (&s as *const i32).cast(), -2, i32::MAX);
        assert_eq!(i32_items(&c, h), vec![1, 0, 0, 7, 7]);

        let r = arr_i32(&mut c, &[1, 2, 3, 4]);
        reverse(p, r);
        assert_eq!(i32_items(&c, r), vec![4, 3, 2, 1]);
        let odd = arr_i32(&mut c, &[1, 2, 3]);
        reverse(p, odd);
        assert_eq!(i32_items(&c, odd), vec![3, 2, 1]);

        let a = arr_i32(&mut c, &[1, 2]);
        let b = arr_i32(&mut c, &[3]);
        let cat = concat(p, a, b, 0);
        assert_eq!(i32_items(&c, cat), vec![1, 2, 3]);
        assert_eq!(i32_items(&c, a), vec![1, 2]);
        assert_eq!(i32_items(&c, b), vec![3]);
    }
}

#[test]
fn splice_shift_unshift_and_copy_within_match_js_rules() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    // SAFETY: live arrays of `c`; element pointers are readable or
    // writable for one i32 as required.
    unsafe {
        let middle = arr_i32(&mut c, &[1, 2, 3, 4, 5]);
        let removed = splice(p, middle, 1, 2, 0);
        assert_eq!(i32_items(&c, removed), vec![2, 3]);
        assert_eq!(i32_items(&c, middle), vec![1, 4, 5]);

        let negative = arr_i32(&mut c, &[1, 2, 3, 4, 5]);
        let removed = splice(p, negative, -2, 2, 0);
        assert_eq!(i32_items(&c, removed), vec![4, 5]);
        assert_eq!(i32_items(&c, negative), vec![1, 2, 3]);

        let past_start = arr_i32(&mut c, &[1, 2, 3]);
        let removed = splice(p, past_start, 99, 5, 0);
        assert_eq!(i32_items(&c, removed), Vec::<i32>::new());
        assert_eq!(i32_items(&c, past_start), vec![1, 2, 3]);

        let past_end = arr_i32(&mut c, &[1, 2, 3]);
        let removed = splice(p, past_end, 1, 99, 0);
        assert_eq!(i32_items(&c, removed), vec![2, 3]);
        assert_eq!(i32_items(&c, past_end), vec![1]);

        let negative_count = arr_i32(&mut c, &[1, 2, 3]);
        let removed = splice(p, negative_count, -2, -1, 0);
        assert_eq!(i32_items(&c, removed), Vec::<i32>::new());
        assert_eq!(i32_items(&c, negative_count), vec![1, 2, 3]);

        let shifted = arr_i32(&mut c, &[1, 2, 3]);
        let mut first = 0i32;
        shift(p, shifted, (&mut first as *mut i32).cast(), 0);
        assert_eq!(first, 1);
        assert_eq!(i32_items(&c, shifted), vec![2, 3]);

        let prepended = arr_i32(&mut c, &[1]);
        let zero = 0i32;
        assert_eq!(unshift(p, prepended, (&zero as *const i32).cast(), 0), 2);
        assert_eq!(i32_items(&c, prepended), vec![0, 1]);

        let to_front = arr_i32(&mut c, &[1, 2, 3, 4, 5]);
        copy_within(p, to_front, 0, 3, i32::MAX);
        assert_eq!(i32_items(&c, to_front), vec![4, 5, 3, 4, 5]);

        let one = arr_i32(&mut c, &[1, 2, 3, 4, 5]);
        copy_within(p, one, 1, 3, 4);
        assert_eq!(i32_items(&c, one), vec![1, 4, 3, 4, 5]);

        let negative = arr_i32(&mut c, &[1, 2, 3, 4, 5]);
        copy_within(p, negative, -2, -4, -3);
        assert_eq!(i32_items(&c, negative), vec![1, 2, 3, 2, 5]);
    }
}

#[test]
fn empty_shift_uses_the_empty_pop_kind_with_a_shift_message() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    let empty = arr_i32(&mut c, &[]);
    let mut out = 0i32;
    // SAFETY: live empty array; `out` is writable for one i32.
    unsafe { crate::ffi::subscript_rt_arr_shift(p, empty, (&raw mut out).cast(), 73) };
    let trap = c.trap_record().expect("empty shift must trap");
    assert_eq!(trap.kind, TrapKind::EmptyPop);
    assert_eq!(trap.message, "shift() on an empty array");
    assert_eq!(trap.pos_id, 73);
}

#[test]
fn for_each_and_map_and_filter() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    // SAFETY: live arrays of `c`; callbacks match the dispatched ABI.
    unsafe {
        let h = arr_i32(&mut c, &[1, 2, 3]);
        let mut acc: i64 = 0;
        for_each(
            p,
            h,
            add_into_env as *const u8,
            (&mut acc as *mut i64).cast(),
            ElemKind::Int,
            false,
        );
        assert_eq!(acc, 6);

        let doubled = map(
            p,
            h,
            double_i32 as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            ElemKind::Int,
            4,
            0,
            false,
            ArrayElements::NoHandle,
        );
        assert_eq!(i32_items(&c, doubled), vec![2, 4, 6]);

        // Type-changing map: i32 -> f64.
        let floats = map(
            p,
            h,
            i32_to_f64 as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            ElemKind::F64,
            8,
            0,
            false,
            ArrayElements::NoHandle,
        );
        assert_eq!(c.array_len(floats), 3);
        let d = c.array_data(floats);
        assert_eq!(d.cast::<f64>().read_unaligned(), 1.5);

        let evens = filter(
            p,
            h,
            is_even as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            0,
            false,
        );
        assert_eq!(i32_items(&c, evens), vec![2]);
    }
}

#[test]
fn reduce_with_differing_accumulator_kind() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    // SAFETY: live array of `c`; callback matches the dispatched ABI.
    unsafe {
        let h = arr_i32(&mut c, &[1, 2, 3]);
        let mut acc: f64 = 100.0;
        reduce(
            p,
            h,
            sum_f64_i32 as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            ElemKind::F64,
            8,
            (&mut acc as *mut f64).cast(),
            false,
        );
        assert_eq!(acc, 106.0);

        let mut right = 0i32;
        reduce_right(
            p,
            h,
            append_digit as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            ElemKind::Int,
            4,
            (&mut right as *mut i32).cast(),
            false,
        );
        assert_eq!(right, 321);
    }
}

#[test]
fn indexed_callbacks_receive_zero_based_indices_in_visit_order() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    // SAFETY: live array of `c`; every callback matches its selected
    // indexed ABI.
    unsafe {
        let h = arr_i32(&mut c, &[10, 20, 30]);
        let mut trace = 0i32;
        for_each(
            p,
            h,
            record_index as *const u8,
            (&mut trace as *mut i32).cast(),
            ElemKind::Int,
            true,
        );
        assert_eq!(trace, 12);

        let mapped = map(
            p,
            h,
            add_index as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            ElemKind::Int,
            4,
            0,
            true,
            ArrayElements::NoHandle,
        );
        assert_eq!(i32_items(&c, mapped), vec![10, 21, 32]);

        let filtered = filter(
            p,
            h,
            odd_index as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            0,
            true,
        );
        assert_eq!(i32_items(&c, filtered), vec![20]);
        assert_eq!(
            some(
                p,
                h,
                odd_index as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                true,
            ),
            1
        );
        assert_eq!(
            every(
                p,
                h,
                odd_index as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                true,
            ),
            0
        );
        assert_eq!(
            find_index(
                p,
                h,
                odd_index as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                true,
            ),
            1
        );

        let mut left = 0i32;
        reduce(
            p,
            h,
            append_index as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            ElemKind::Int,
            4,
            (&mut left as *mut i32).cast(),
            true,
        );
        assert_eq!(left, 12);

        let mut right = 0i32;
        reduce_right(
            p,
            h,
            append_index as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            ElemKind::Int,
            4,
            (&mut right as *mut i32).cast(),
            true,
        );
        assert_eq!(right, 210);
    }
}

#[test]
fn some_every_find_index_short_circuit() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    // SAFETY: live array of `c`; callback matches the dispatched ABI.
    unsafe {
        let h = arr_i32(&mut c, &[1, 2, 3, 4, 5]);
        let mut probes: i64 = 0;
        let env = (&mut probes as *mut i64).cast::<u8>();
        assert_eq!(
            some(p, h, count_and_test as *const u8, env, ElemKind::Int, false,),
            1
        );
        assert_eq!(probes, 3); // stopped at the first hit (v == 3)
        probes = 0;
        assert_eq!(
            every(p, h, count_and_test as *const u8, env, ElemKind::Int, false,),
            0
        );
        assert_eq!(probes, 1); // stopped at the first miss (v == 1)
        assert_eq!(
            find_index(
                p,
                h,
                is_even as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                false,
            ),
            1
        );
        let odd = arr_i32(&mut c, &[1, 3]);
        assert_eq!(
            find_index(
                p,
                odd,
                is_even as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                false,
            ),
            -1
        );
    }
}

#[test]
fn sort_is_stable_and_ascending() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    // SAFETY: live arrays of `c`; comparators match the ABI.
    unsafe {
        let h = arr_i32(&mut c, &[5, 1, 4, 2, 3]);
        sort(p, h, cmp_i32 as *const u8, std::ptr::null(), ElemKind::Int);
        assert_eq!(i32_items(&c, h), vec![1, 2, 3, 4, 5]);

        // Stability: key*10+seq pairs ordered by key keep seq order.
        let s = arr_i32(&mut c, &[20, 11, 22, 13, 14]);
        sort(p, s, cmp_key as *const u8, std::ptr::null(), ElemKind::Int);
        assert_eq!(i32_items(&c, s), vec![11, 13, 14, 20, 22]);
    }
}

#[test]
fn callback_trap_aborts_and_leaves_defensible_state() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    // SAFETY: live arrays of `c`; callbacks match the ABI.
    unsafe {
        // map: the trap at v == 2 keeps element 1 only.
        let h = arr_i32(&mut c, &[1, 2, 3]);
        let out = map(
            p,
            h,
            trap_on_two as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            ElemKind::Int,
            4,
            0,
            false,
            ArrayElements::NoHandle,
        );
        assert!(c.trapped());
        assert_eq!(i32_items(&c, out), vec![1]);
        let rec = c.trap_record().expect("trap recorded");
        assert_eq!(rec.pos_id, 42);
        // Already trapped: callback entries do not run script code.
        let out2 = map(
            p,
            h,
            double_i32 as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            ElemKind::Int,
            4,
            0,
            false,
            ArrayElements::NoHandle,
        );
        assert!(out2.is_null());
        c.clear_trap();

        // sort: a comparator trap leaves the array exactly as it was.
        let s = arr_i32(&mut c, &[3, 1, 2]);
        sort(
            p,
            s,
            cmp_trapping as *const u8,
            std::ptr::null(),
            ElemKind::Int,
        );
        assert!(c.trapped());
        assert_eq!(i32_items(&c, s), vec![3, 1, 2]);
        c.clear_trap();

        // reduce: the last completed accumulator survives the trap.
        let r = arr_i32(&mut c, &[1, 2, 3]);
        let mut acc: f64 = 0.0;
        reduce(
            p,
            r,
            sum_and_trap_on_two as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            ElemKind::F64,
            8,
            (&mut acc as *mut f64).cast(),
            false,
        );
        assert!(c.trapped());
        assert_eq!(acc, 1.0);
        c.clear_trap();
    }
}

unsafe extern "C" fn sum_and_trap_on_two(
    ctx: *mut Context,
    _env: *const u8,
    acc: f64,
    v: i32,
) -> f64 {
    if v == 2 {
        // SAFETY: `ctx` is the live test context.
        unsafe { &mut *ctx }.trap(TrapKind::IndexOutOfBounds, "reduce trap", 9);
    }
    acc + f64::from(v)
}

#[test]
fn unknown_abi_shape_traps_in_the_equality_searches() {
    // The searches validate the element shape exactly as the
    // callback entries do. Two 3-byte elements have no ABI class;
    // a comparison as zero-extended words reports them equal, so
    // the entry traps and misses instead.
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    let h = c.array_new(3, 0);
    let (a, b) = ([1u8, 2, 3], [4u8, 5, 6]);
    // SAFETY: live 3-byte-element array; sources readable for 3 bytes.
    unsafe {
        c.array_push(h, a.as_ptr(), 0);
        c.array_push(h, b.as_ptr(), 0);
        assert_eq!(index_of(p, h, b.as_ptr(), ElemKind::Int, 0), -1);
    }
    assert!(c.trapped());
    assert_eq!(c.trap_record().map(|r| r.kind), Some(TrapKind::Internal));
    c.clear_trap();
    // SAFETY: as above.
    unsafe {
        assert_eq!(last_index_of(p, h, a.as_ptr(), ElemKind::Int, i32::MAX), -1);
        assert_eq!(includes(p, h, a.as_ptr(), ElemKind::Int, 0), 0);
    }
    assert!(c.trapped());
    assert_eq!(c.trap_record().map(|r| r.kind), Some(TrapKind::Internal));
}

#[test]
fn unknown_abi_shape_traps_internal() {
    let mut c = ctx();
    let p: *mut Context = &mut *c;
    // A 3-byte element has no ABI class; the entry must trap
    // Internal instead of guessing.
    let h = c.array_new(3, 0);
    // SAFETY: live array of `c`.
    unsafe {
        for_each(
            p,
            h,
            double_i32 as *const u8,
            std::ptr::null(),
            ElemKind::Int,
            false,
        );
    }
    assert!(c.trapped());
    assert_eq!(c.trap_record().map(|r| r.kind), Some(TrapKind::Internal));
}
