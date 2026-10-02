use crate::context::Context;
use crate::trap::TrapKind;

/// Narrows an `f64` to raw IEEE 754 binary16 storage bits using
/// round-to-nearest-even. Overflow becomes infinity; subnormals, signed
/// zero, and NaN are preserved (Q23).
#[no_mangle]
pub extern "C" fn subscript_rt_f16_from_f64(value: f64) -> u16 {
    crate::half::from_f64(value)
}

/// Widens raw IEEE 754 binary16 storage bits to an exactly represented
/// `f64`, preserving signed zero, infinity, and NaN (Q23).
#[no_mangle]
pub extern "C" fn subscript_rt_f16_to_f64(bits: u16) -> f64 {
    crate::half::to_f64(bits)
}
// ----- Q14 formatting -----

format_integer!(subscript_rt_fmt_i32, i32, fmt_i32_into);
format_integer!(subscript_rt_fmt_u32, u32, fmt_u32_into);
format_integer!(subscript_rt_fmt_i64, i64, fmt_i64_into);
format_integer!(subscript_rt_fmt_u64, u64, fmt_u64_into);

/// Formats an `f32` at f32 precision (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fmt_f32(ctx: *mut Context, v: f32, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let mut storage = ryu_js::Buffer::new();
    ctx.alloc_str(crate::fmt::fmt_f32_into(v, &mut storage).as_bytes(), pos_id)
}

/// Formats an `f64` (Q14).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fmt_f64(ctx: *mut Context, v: f64, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let mut storage = ryu_js::Buffer::new();
    ctx.alloc_str(crate::fmt::fmt_f64_into(v, &mut storage).as_bytes(), pos_id)
}

/// Formats a boolean.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_fmt_bool(ctx: *mut Context, v: u32, pos_id: u32) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc_str(crate::fmt::fmt_bool(v != 0).as_bytes(), pos_id)
}

// ----- Number and parsing intrinsics (stdlib.md §11, Q25/Q26) -----
//
// All operations stay behind opaque symbols so both tiers execute the
// same Rust implementation. The predicates are pure; parsing and
// formatting entries carry a position for allocation/range traps.

fn checked_range(
    ctx: &mut Context,
    value: i32,
    lo: i32,
    hi: i32,
    what: &str,
    pos_id: u32,
) -> Option<u32> {
    if !(lo..=hi).contains(&value) {
        ctx.trap(
            TrapKind::NumberRange,
            format!("{what} must be in {lo}..={hi}, got {value}"),
            pos_id,
        );
        return None;
    }
    Some(value as u32)
}

/// IEEE floating remainder used by both code-generation tiers.
#[no_mangle]
pub extern "C" fn subscript_rt_fmod(_ctx: *mut Context, left: f64, right: f64) -> f64 {
    left % right
}

/// `Number.isNaN(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_is_nan(_ctx: *mut Context, value: f64) -> i32 {
    i32::from(crate::num::is_nan(value))
}

/// `Number.isFinite(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_is_finite(_ctx: *mut Context, value: f64) -> i32 {
    i32::from(crate::num::is_finite(value))
}

/// `Number.isInteger(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_is_integer(_ctx: *mut Context, value: f64) -> i32 {
    i32::from(crate::num::is_integer(value))
}

/// `Number.isSafeInteger(value)`.
///
/// # Safety
///
/// Shared contract; `ctx` is intentionally unused.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_is_safe_integer(_ctx: *mut Context, value: f64) -> i32 {
    i32::from(crate::num::is_safe_integer(value))
}

/// `parseInt(s, radix)`: explicit radix 2–36, otherwise a Q25 trap.
///
/// # Safety
///
/// Shared contract; `s` is a live UTF-8 string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_parse_int(
    ctx: *mut Context,
    s: *const u8,
    radix: i32,
    pos_id: u32,
) -> f64 {
    if s.is_null() {
        return f64::NAN;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(radix) = checked_range(ctx, radix, 2, 36, "parseInt radix", pos_id) else {
        return f64::NAN;
    };
    // SAFETY: live string handle. Context string allocations keep immutable
    // input allocation addresses stable.
    let bytes = unsafe { ctx.str_view(s) };
    let value = std::str::from_utf8(bytes).unwrap_or_default();
    crate::num::parse_int(value, radix)
}

/// `parseFloat(s)`: ECMA longest-prefix parsing.
///
/// # Safety
///
/// Shared contract; `s` is a live UTF-8 string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_parse_float(
    ctx: *mut Context,
    s: *const u8,
    _pos_id: u32,
) -> f64 {
    if s.is_null() {
        return f64::NAN;
    }
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // SAFETY: live string handle.
    let bytes = unsafe { ctx.str_view(s) };
    let value = std::str::from_utf8(bytes).unwrap_or_default();
    crate::num::parse_float(value)
}

/// `value.toFixed(digits)`: exact ECMA decimal rounding for digits
/// 0–100, otherwise a Q25 trap.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_to_fixed(
    ctx: *mut Context,
    value: f64,
    digits: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(digits) = checked_range(ctx, digits, 0, 100, "toFixed digits", pos_id) else {
        return std::ptr::null_mut();
    };
    ctx.alloc_str(crate::num::to_fixed(value, digits).as_bytes(), pos_id)
}

/// `f32_value.toString(radix)`: radix 2–36, with radix 10 delegated
/// exactly to the Q14 `f32` formatter.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_to_string_f32(
    ctx: *mut Context,
    value: f32,
    radix: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(radix) = checked_range(ctx, radix, 2, 36, "toString radix", pos_id) else {
        return std::ptr::null_mut();
    };
    ctx.alloc_str(
        crate::num::to_string_radix_f32(value, radix).as_bytes(),
        pos_id,
    )
}

/// `f64_value.toString(radix)`: radix 2–36, with radix 10 delegated
/// exactly to Q14.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_to_string_f64(
    ctx: *mut Context,
    value: f64,
    radix: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(radix) = checked_range(ctx, radix, 2, 36, "toString radix", pos_id) else {
        return std::ptr::null_mut();
    };
    ctx.alloc_str(
        crate::num::to_string_radix_f64(value, radix).as_bytes(),
        pos_id,
    )
}

/// `value.toExponential(digits?)`: `-1` represents the checker-normalized
/// omitted argument; supplied digits must be in 0–100.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_to_exponential(
    ctx: *mut Context,
    value: f64,
    digits: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let digits = if digits == -1 {
        None
    } else {
        match u32::try_from(digits) {
            Ok(digits) if digits <= 100 => Some(digits),
            _ => {
                ctx.trap(
                    TrapKind::NumberRange,
                    format!("toExponential digits must be in 0..=100, got {digits}"),
                    pos_id,
                );
                return std::ptr::null_mut();
            }
        }
    };
    ctx.alloc_str(crate::num::to_exponential(value, digits).as_bytes(), pos_id)
}

/// `value.toPrecision(digits)`: digits must be in 1–100.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_num_to_precision(
    ctx: *mut Context,
    value: f64,
    digits: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Ok(digits) = u32::try_from(digits) else {
        ctx.trap(
            TrapKind::NumberRange,
            format!("toPrecision digits must be in 1..=100, got {digits}"),
            pos_id,
        );
        return std::ptr::null_mut();
    };
    if !(1..=100).contains(&digits) {
        ctx.trap(
            TrapKind::NumberRange,
            format!("toPrecision digits must be in 1..=100, got {digits}"),
            pos_id,
        );
        return std::ptr::null_mut();
    }
    ctx.alloc_str(crate::num::to_precision(value, digits).as_bytes(), pos_id)
}

// ----- Math (stdlib.md §1/§2) -----
//
// Every `subscript_rt_math_*` symbol takes the Context pointer first, so both
// tiers emit every Math call identically. The f64 subset returns f64;
// clz32 is `(ctx, u32) -> i32`, imul is `(ctx, i32, i32) -> i32`, and
// fround is `(ctx, f64) -> f64`. The binary32 bit accessors use `u32`
// for their bit-pattern side. Pure entries ignore `ctx`; only random
// reads Context state. Both tiers call these opaque symbols. They never use
// a direct libm or builtin operation (stdlib.md §0.2/Q26/Q27).

math_ffi_unary! {
    /// `Math.abs`.
    subscript_rt_math_abs => abs,
    /// `Math.acos`.
    subscript_rt_math_acos => acos,
    /// `Math.acosh`.
    subscript_rt_math_acosh => acosh,
    /// `Math.asin`.
    subscript_rt_math_asin => asin,
    /// `Math.asinh`.
    subscript_rt_math_asinh => asinh,
    /// `Math.atan`.
    subscript_rt_math_atan => atan,
    /// `Math.atanh`.
    subscript_rt_math_atanh => atanh,
    /// `Math.cbrt`.
    subscript_rt_math_cbrt => cbrt,
    /// `Math.ceil`.
    subscript_rt_math_ceil => ceil,
    /// `Math.cos`.
    subscript_rt_math_cos => cos,
    /// `Math.cosh`.
    subscript_rt_math_cosh => cosh,
    /// `Math.exp`.
    subscript_rt_math_exp => exp,
    /// `Math.expm1`.
    subscript_rt_math_expm1 => expm1,
    /// `Math.floor`.
    subscript_rt_math_floor => floor,
    /// `Math.log`.
    subscript_rt_math_log => log,
    /// `Math.log1p`.
    subscript_rt_math_log1p => log1p,
    /// `Math.log10`.
    subscript_rt_math_log10 => log10,
    /// `Math.log2`.
    subscript_rt_math_log2 => log2,
    /// `Math.round` (ECMA half-toward-+∞).
    subscript_rt_math_round => round,
    /// `Math.sign` (±0/±1/NaN).
    subscript_rt_math_sign => sign,
    /// `Math.sin`.
    subscript_rt_math_sin => sin,
    /// `Math.sinh`.
    subscript_rt_math_sinh => sinh,
    /// `Math.sqrt`.
    subscript_rt_math_sqrt => sqrt,
    /// `Math.tan`.
    subscript_rt_math_tan => tan,
    /// `Math.tanh`.
    subscript_rt_math_tanh => tanh,
    /// `Math.trunc`.
    subscript_rt_math_trunc => trunc,
}

math_ffi_binary! {
    /// `Math.atan2(y, x)`.
    subscript_rt_math_atan2 => atan2,
    /// `Math.hypot(a, b)` (two arguments, Q19).
    subscript_rt_math_hypot => hypot,
    /// `Math.pow(base, exp)` (ECMA edges).
    subscript_rt_math_pow => pow,
    /// `Math.max(a, b)` (NaN propagation, zero ordering).
    subscript_rt_math_max => max,
    /// `Math.min(a, b)` (NaN propagation, zero ordering).
    subscript_rt_math_min => min,
}

/// `Math.clz32(x)`: Rust defines the zero input as 32; this opaque
/// entry prevents the ship tier from emitting C's undefined
/// `__builtin_clz(0)`.
#[no_mangle]
pub extern "C" fn subscript_rt_math_clz32(ctx: *mut Context, x: u32) -> i32 {
    let _ = ctx;
    crate::math::clz32(x)
}

/// `Math.imul(a, b)`: wrapping 32-bit multiplication.
#[no_mangle]
pub extern "C" fn subscript_rt_math_imul(ctx: *mut Context, a: i32, b: i32) -> i32 {
    let _ = ctx;
    crate::math::imul(a, b)
}

/// `Math.fround(x)`: exact `f64 -> f32 -> f64` rounding.
#[no_mangle]
pub extern "C" fn subscript_rt_math_fround(ctx: *mut Context, x: f64) -> f64 {
    let _ = ctx;
    crate::math::fround(x)
}

/// `Math.f32ToBits(value)`: narrow and return canonical binary32 bits.
#[no_mangle]
pub extern "C" fn subscript_rt_math_f32_to_bits(ctx: *mut Context, x: f64) -> u32 {
    let _ = ctx;
    crate::math::f32_to_bits(x)
}

/// `Math.f32FromBits(bits)`: widen binary32 bits exactly to binary64.
#[no_mangle]
pub extern "C" fn subscript_rt_math_f32_from_bits(ctx: *mut Context, bits: u32) -> f64 {
    let _ = ctx;
    crate::math::f32_from_bits(bits)
}

/// `Math.random()` (stdlib.md §2): the next deterministic draw from the
/// Context-owned xoshiro256++ stream.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_math_random(ctx: *mut Context) -> f64 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.random_f64()
}
