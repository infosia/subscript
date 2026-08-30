//! Shared value equality for array searches and associative keys.

use crate::context::Context;

/// The runtime kinds that have scalar or string equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueKind {
    /// Integer-like bits, including reference identity.
    Bits,
    /// IEEE binary16, accepted for array storage but not associative keys.
    F16,
    /// IEEE binary32.
    F32,
    /// IEEE binary64.
    F64,
    /// A string handle, compared by UTF-8 content.
    Str,
}

/// Reads `width` (1/2/4/8) bytes at `pointer`, zero-extended to `u64`.
///
/// # Safety
///
/// `pointer` must be readable for `width` bytes. The width must be 1, 2, 4,
/// or 8.
pub(crate) unsafe fn read_uint(pointer: *const u8, width: usize) -> u64 {
    // SAFETY: caller contract; all reads permit unaligned storage.
    unsafe {
        match width {
            1 => u64::from(pointer.read_unaligned()),
            2 => u64::from(pointer.cast::<u16>().read_unaligned()),
            4 => u64::from(pointer.cast::<u32>().read_unaligned()),
            8 => pointer.cast::<u64>().read_unaligned(),
            _ => 0,
        }
    }
}

/// Compares two runtime values with `===` or SameValueZero semantics.
///
/// `same_value_zero` changes only float NaN equality. Both modes compare
/// positive and negative zero as equal.
///
/// # Safety
///
/// `left` and `right` must be readable for `width` bytes. The kind and width
/// must agree. String slots must contain null or live handles of `ctx`.
pub(crate) unsafe fn value_eq(
    ctx: *mut Context,
    kind: ValueKind,
    width: usize,
    left: *const u8,
    right: *const u8,
    same_value_zero: bool,
) -> bool {
    match kind {
        ValueKind::Bits => {
            // SAFETY: caller contract.
            unsafe { read_uint(left, width) == read_uint(right, width) }
        }
        ValueKind::F16 => {
            // SAFETY: the F16 shape contains two readable bytes.
            let a = crate::half::to_f64(unsafe { left.cast::<u16>().read_unaligned() });
            let b = crate::half::to_f64(unsafe { right.cast::<u16>().read_unaligned() });
            a == b || (same_value_zero && a.is_nan() && b.is_nan())
        }
        ValueKind::F32 => {
            // SAFETY: the F32 shape contains four readable bytes.
            let a = unsafe { left.cast::<f32>().read_unaligned() };
            let b = unsafe { right.cast::<f32>().read_unaligned() };
            a == b || (same_value_zero && a.is_nan() && b.is_nan())
        }
        ValueKind::F64 => {
            // SAFETY: the F64 shape contains eight readable bytes.
            let a = unsafe { left.cast::<f64>().read_unaligned() };
            let b = unsafe { right.cast::<f64>().read_unaligned() };
            a == b || (same_value_zero && a.is_nan() && b.is_nan())
        }
        ValueKind::Str => {
            // SAFETY: both slots contain pointer-width string handles.
            let a = unsafe { left.cast::<*const u8>().read_unaligned() };
            let b = unsafe { right.cast::<*const u8>().read_unaligned() };
            if a.is_null() || b.is_null() {
                return a == b;
            }
            // SAFETY: caller contract supplies live string handles.
            unsafe { (*ctx).str_bytes(a) == (*ctx).str_bytes(b) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrops::{elem_value_eq, ElemKind};
    use crate::assocops::{keys_equal, KeyKind};

    unsafe fn callers_agree(
        ctx: *mut Context,
        elem_kind: ElemKind,
        key_kind: KeyKind,
        width: usize,
        left: *const u8,
        right: *const u8,
        expected: bool,
    ) {
        // SAFETY: each call receives the matching test storage below.
        assert_eq!(
            unsafe { elem_value_eq(ctx, elem_kind, width, left, right, true) },
            expected
        );
        // SAFETY: each call receives the matching test storage below.
        assert_eq!(
            unsafe { keys_equal(ctx, key_kind, left, right, width) },
            expected
        );
    }

    #[test]
    fn value_eq_every_kind_and_width_callers_agree() {
        let mut ctx = Context::new();
        let ctx_ptr: *mut Context = &mut *ctx;

        for width in [1, 2, 4, 8] {
            let left = [0x5au8; 8];
            let equal = left;
            let mut different = left;
            different[0] ^= 1;
            // SAFETY: all arrays contain eight readable bytes.
            unsafe {
                callers_agree(
                    ctx_ptr,
                    ElemKind::Int,
                    KeyKind::Bits,
                    width,
                    left.as_ptr(),
                    equal.as_ptr(),
                    true,
                );
                callers_agree(
                    ctx_ptr,
                    ElemKind::SignedInt,
                    KeyKind::Ref,
                    width,
                    left.as_ptr(),
                    different.as_ptr(),
                    false,
                );
            }
        }

        let f32_nan_a = f32::from_bits(0x7f80_0001);
        let f32_nan_b = f32::from_bits(0xffc0_0042);
        let f64_nan_a = f64::from_bits(0x7ff0_0000_0000_0001);
        let f64_nan_b = f64::from_bits(0xfff8_0000_0000_0042);
        // SAFETY: values have the widths selected for their kinds.
        unsafe {
            callers_agree(
                ctx_ptr,
                ElemKind::F32,
                KeyKind::F32,
                4,
                (&raw const f32_nan_a).cast(),
                (&raw const f32_nan_b).cast(),
                true,
            );
            callers_agree(
                ctx_ptr,
                ElemKind::F64,
                KeyKind::F64,
                8,
                (&raw const f64_nan_a).cast(),
                (&raw const f64_nan_b).cast(),
                true,
            );
            assert!(!elem_value_eq(
                ctx_ptr,
                ElemKind::F32,
                4,
                (&raw const f32_nan_a).cast(),
                (&raw const f32_nan_b).cast(),
                false,
            ));
            assert!(!elem_value_eq(
                ctx_ptr,
                ElemKind::F64,
                8,
                (&raw const f64_nan_a).cast(),
                (&raw const f64_nan_b).cast(),
                false,
            ));
        }

        let string_a = ctx.alloc_str(b"same", 0);
        let string_b = ctx.alloc_str(b"same", 0);
        // SAFETY: both slots contain live string handles.
        unsafe {
            callers_agree(
                ctx_ptr,
                ElemKind::Str,
                KeyKind::Str,
                8,
                (&raw const string_a).cast(),
                (&raw const string_b).cast(),
                true,
            );
        }

        let f16_nan_a = 0x7c01u16;
        let f16_nan_b = 0xfe42u16;
        // SAFETY: both values contain readable binary16 bits.
        unsafe {
            assert!(elem_value_eq(
                ctx_ptr,
                ElemKind::F16,
                2,
                (&raw const f16_nan_a).cast(),
                (&raw const f16_nan_b).cast(),
                true,
            ));
            assert!(!elem_value_eq(
                ctx_ptr,
                ElemKind::F16,
                2,
                (&raw const f16_nan_a).cast(),
                (&raw const f16_nan_b).cast(),
                false,
            ));
        }
        assert_eq!(
            KeyKind::from_u32(5),
            None,
            "F16 is not an associative key kind"
        );
    }
}
