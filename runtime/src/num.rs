//! Deterministic `Number` and parsing operations (Q25/Q26).
//!
//! These functions implement the accepted ECMA surface once in Rust;
//! the dev-JIT and ship-C tiers both reach them through opaque
//! `subscript_rt_num_*` symbols. Parsing never coerces non-strings. Numeric
//! formatting uses `ryu-js` or exact integer arithmetic, so it does not
//! inherit a host libc's formatting or rounding.

use num_bigint::BigUint;
use num_traits::ToPrimitive;

const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// ECMA `Number.isNaN`.
#[must_use]
pub fn is_nan(value: f64) -> bool {
    value.is_nan()
}

/// ECMA `Number.isFinite`.
#[must_use]
pub fn is_finite(value: f64) -> bool {
    value.is_finite()
}

/// ECMA `Number.isInteger`.
#[must_use]
pub fn is_integer(value: f64) -> bool {
    value.is_finite() && value.trunc() == value
}

/// ECMA `Number.isSafeInteger`.
#[must_use]
pub fn is_safe_integer(value: f64) -> bool {
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    is_integer(value) && value.abs() <= MAX_SAFE_INTEGER
}

fn trim_start_ecma(value: &str) -> &str {
    value.trim_start_matches(crate::strops::is_ecma_whitespace)
}

fn digit_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'a'..=b'z' => Some(u32::from(byte - b'a') + 10),
        b'A'..=b'Z' => Some(u32::from(byte - b'A') + 10),
        _ => None,
    }
}

/// ECMA `parseInt` with an already-validated explicit radix.
///
/// Leading ECMA whitespace and an optional sign are consumed, followed
/// by the longest valid digit prefix. Radix 16 accepts the standard
/// optional `0x` prefix. No consumed digit yields `NaN`.
#[must_use]
pub fn parse_int(value: &str, radix: u32) -> f64 {
    let trimmed = trim_start_ecma(value);
    let (negative, mut rest) = match trimmed.as_bytes().first() {
        Some(b'+') => (false, &trimmed[1..]),
        Some(b'-') => (true, &trimmed[1..]),
        _ => (false, trimmed),
    };
    if radix == 16 && (rest.starts_with("0x") || rest.starts_with("0X")) {
        rest = &rest[2..];
    }

    let mut integer = BigUint::from(0u8);
    let mut consumed = false;
    for byte in rest.bytes() {
        let Some(digit) = digit_value(byte).filter(|digit| *digit < radix) else {
            break;
        };
        integer *= radix;
        integer += digit;
        consumed = true;
    }
    if !consumed {
        return f64::NAN;
    }
    let magnitude = integer.to_f64().unwrap_or(f64::INFINITY);
    if negative { -magnitude } else { magnitude }
}

fn decimal_prefix_len(value: &str) -> usize {
    let bytes = value.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'+') | Some(b'-')));
    if value[at..].starts_with("Infinity") {
        return at + "Infinity".len();
    }

    let before = at;
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    let mut digits = at - before;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        let fraction = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        digits += at - fraction;
    }
    if digits == 0 {
        return 0;
    }

    if matches!(bytes.get(at), Some(b'e') | Some(b'E')) {
        let exponent_start = at;
        at += 1;
        if matches!(bytes.get(at), Some(b'+') | Some(b'-')) {
            at += 1;
        }
        let exponent_digits = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == exponent_digits {
            at = exponent_start;
        }
    }
    at
}

/// ECMA `parseFloat`: leading whitespace, longest decimal prefix,
/// `Infinity`, and `NaN` when no prefix parses.
#[must_use]
pub fn parse_float(value: &str) -> f64 {
    let trimmed = trim_start_ecma(value);
    let len = decimal_prefix_len(trimmed);
    if len == 0 {
        return f64::NAN;
    }
    let prefix = &trimmed[..len];
    match prefix {
        "Infinity" | "+Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ => prefix.parse::<f64>().unwrap_or(f64::NAN),
    }
}

/// ECMA `Number::toFixed` for a validated `digits` count (0–100).
///
/// Values with magnitude at least `1e21` use Q14 `Number::toString`,
/// as do non-finite values. Negative zero has no sign; a negative
/// nonzero value retains its sign even when it rounds to zero.
#[must_use]
pub fn to_fixed(value: f64, digits: u32) -> String {
    let digits = u8::try_from(digits).unwrap_or(100);
    ryu_js::Buffer::new()
        .format_to_fixed(value, digits)
        .to_owned()
}

fn next_after_up(value: f64) -> f64 {
    if value.is_nan() || value == f64::INFINITY {
        value
    } else if value == 0.0 {
        f64::from_bits(1)
    } else if value.is_sign_positive() {
        f64::from_bits(value.to_bits() + 1)
    } else {
        f64::from_bits(value.to_bits() - 1)
    }
}

fn finite_to_string_radix(mut value: f64, radix: u32) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let negative = value.is_sign_negative();
    if negative {
        value = -value;
    }

    // Adapted from Boa's V8-derived implementation:
    // <https://github.com/boa-dev/boa>,
    // core/engine/src/builtins/number/mod.rs (`to_js_string_radix`).
    // The error interval determines when the generated prefix uniquely
    // identifies the input double.
    let mut integer = value.floor();
    let mut fraction = value - integer;
    let mut delta = 0.5 * (next_after_up(value) - value);
    delta = f64::from_bits(1).max(delta);
    let mut fraction_digits = Vec::new();
    let mut carry_integer = false;

    if fraction >= delta {
        loop {
            fraction *= f64::from(radix);
            delta *= f64::from(radix);
            let digit = fraction as u32;
            fraction_digits.push(digit as u8);
            fraction -= f64::from(digit);

            if fraction + delta > 1.0
                && (fraction > 0.5
                    || ((fraction - 0.5).abs() < f64::EPSILON && digit & 1 != 0))
            {
                while let Some(previous) = fraction_digits.pop() {
                    if u32::from(previous) + 1 < radix {
                        fraction_digits.push(previous + 1);
                        break;
                    }
                }
                if fraction_digits.is_empty() {
                    carry_integer = true;
                }
                break;
            }
            if fraction < delta {
                break;
            }
        }
    }

    if carry_integer {
        integer += 1.0;
    }
    let radix_f64 = f64::from(radix);
    let mut reverse_integer_digits = Vec::new();
    while finite_parts(integer / radix_f64).1 > 0 {
        integer /= radix_f64;
        reverse_integer_digits.push(0);
    }
    loop {
        let remainder = integer % radix_f64;
        reverse_integer_digits.push(remainder as u8);
        integer = (integer - remainder) / radix_f64;
        if integer <= 0.0 {
            break;
        }
    }
    let mut result = String::with_capacity(reverse_integer_digits.len());
    result.extend(
        reverse_integer_digits
            .into_iter()
            .rev()
            .map(|digit| char::from(DIGITS[usize::from(digit)])),
    );
    if !fraction_digits.is_empty() {
        result.push('.');
        result.extend(
            fraction_digits
                .into_iter()
                .map(|digit| char::from(DIGITS[usize::from(digit)])),
        );
    }
    if negative {
        result.insert(0, '-');
    }
    result
}

/// ECMA `Number::toString(radix)` for a validated radix on an `f32`.
///
/// Radix 10 delegates to Q14's `f32` formatter exactly. Other radixes
/// operate on the exact widened `f32` value.
#[must_use]
pub fn to_string_radix_f32(value: f32, radix: u32) -> String {
    if radix == 10 {
        return crate::fmt::fmt_f32(value);
    }
    to_string_radix_f64(f64::from(value), radix)
}

/// ECMA `Number::toString(radix)` for a validated radix on an `f64`.
///
/// Radix 10 delegates to Q14 exactly. Non-finite values use the Q14
/// spellings; negative zero becomes `0` in non-decimal radixes.
#[must_use]
pub fn to_string_radix_f64(value: f64, radix: u32) -> String {
    if radix == 10 {
        return crate::fmt::fmt_f64(value);
    }
    if !value.is_finite() {
        return crate::fmt::fmt_f64(value);
    }
    finite_to_string_radix(value, radix)
}

fn decimal_exponent(value: f64) -> i32 {
    let formatted = crate::fmt::fmt_f64(value);
    if let Some((_, exponent)) = formatted.split_once('e') {
        return exponent.parse::<i32>().unwrap_or(0);
    }
    let bytes = formatted.as_bytes();
    let dot = bytes.iter().position(|byte| *byte == b'.');
    let integer_len = dot.unwrap_or(bytes.len());
    if bytes[..integer_len].iter().any(|byte| *byte != b'0') {
        return integer_len as i32 - 1;
    }
    bytes
        .iter()
        .position(|byte| byte.is_ascii_digit() && *byte != b'0')
        .map_or(0, |first| integer_len as i32 - first as i32)
}

fn finite_parts(value: f64) -> (BigUint, i32) {
    let bits = value.to_bits();
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    if raw_exp == 0 {
        (BigUint::from(fraction), -1074)
    } else {
        (
            BigUint::from((1_u64 << 52) | fraction),
            raw_exp - 1023 - 52,
        )
    }
}

fn rounded_significand(value: f64, precision: u32) -> (String, i32) {
    let mut exponent = decimal_exponent(value);
    let scale = precision as i32 - 1 - exponent;
    let (mut numerator, binary_exponent) = finite_parts(value);
    let mut denominator = BigUint::from(1_u8);
    if binary_exponent >= 0 {
        numerator <<= binary_exponent as usize;
    } else {
        denominator <<= (-binary_exponent) as usize;
    }
    if scale >= 0 {
        numerator *= BigUint::from(10_u8).pow(scale as u32);
    } else {
        denominator *= BigUint::from(10_u8).pow((-scale) as u32);
    }

    let mut rounded = &numerator / &denominator;
    let remainder = numerator % &denominator;
    if (remainder << 1_usize) >= denominator {
        rounded += 1_u8;
    }
    let mut digits = rounded.to_str_radix(10);
    let precision = precision as usize;
    if digits.len() > precision {
        exponent += (digits.len() - precision) as i32;
        digits.truncate(precision);
    } else if digits.len() < precision {
        digits.push_str(&"0".repeat(precision - digits.len()));
    }
    (digits, exponent)
}

fn exponent_suffix(exponent: i32) -> String {
    if exponent >= 0 {
        format!("e+{exponent}")
    } else {
        format!("e{exponent}")
    }
}

fn shortest_exponential(value: f64) -> String {
    if value == 0.0 {
        return "0e+0".to_string();
    }
    let negative = value.is_sign_negative();
    let magnitude = value.abs();
    let formatted = crate::fmt::fmt_f64(magnitude);
    let exponent = decimal_exponent(magnitude);
    let mantissa = formatted
        .split_once('e')
        .map_or(formatted.as_str(), |(mantissa, _)| mantissa);
    let mut digits: String = mantissa.chars().filter(|ch| *ch != '.').collect();
    while digits.starts_with('0') {
        digits.remove(0);
    }
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }

    let mut result = String::new();
    if negative {
        result.push('-');
    }
    result.push(digits.as_bytes()[0] as char);
    if digits.len() > 1 {
        result.push('.');
        result.push_str(&digits[1..]);
    }
    result.push_str(&exponent_suffix(exponent));
    result
}

/// ECMA `Number::toExponential(digits?)`.
///
/// `None` uses the shortest uniquely identifying decimal digits.
/// A supplied, validated digit count is the number of digits after the
/// decimal point.
#[must_use]
pub fn to_exponential(value: f64, digits: Option<u32>) -> String {
    if !value.is_finite() {
        return crate::fmt::fmt_f64(value);
    }
    if digits.is_none() {
        return shortest_exponential(value);
    }
    let precision = digits.unwrap_or(0) + 1;
    let magnitude = value.abs();
    let (significand, exponent) = if magnitude == 0.0 {
        ("0".repeat(precision as usize), 0)
    } else {
        rounded_significand(magnitude, precision)
    };
    let mut result = String::new();
    if value < 0.0 {
        result.push('-');
    }
    result.push(significand.as_bytes()[0] as char);
    if precision > 1 {
        result.push('.');
        result.push_str(&significand[1..]);
    }
    result.push_str(&exponent_suffix(exponent));
    result
}

/// ECMA `Number::toPrecision(digits)` for a validated precision.
#[must_use]
pub fn to_precision(value: f64, precision: u32) -> String {
    if !value.is_finite() {
        return crate::fmt::fmt_f64(value);
    }
    let magnitude = value.abs();
    let (digits, exponent) = if magnitude == 0.0 {
        ("0".repeat(precision as usize), 0)
    } else {
        rounded_significand(magnitude, precision)
    };
    let mut result = String::new();
    if value < 0.0 {
        result.push('-');
    }
    if exponent < -6 || exponent >= precision as i32 {
        result.push(digits.as_bytes()[0] as char);
        if precision > 1 {
            result.push('.');
            result.push_str(&digits[1..]);
        }
        result.push_str(&exponent_suffix(exponent));
    } else if exponent >= 0 {
        let integer_digits = exponent as usize + 1;
        result.push_str(&digits[..integer_digits]);
        if integer_digits < digits.len() {
            result.push('.');
            result.push_str(&digits[integer_digits..]);
        }
    } else {
        result.push_str("0.");
        result.push_str(&"0".repeat((-exponent - 1) as usize));
        result.push_str(&digits);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_predicates_follow_ecma() {
        assert!(is_nan(f64::NAN));
        assert!(!is_nan(f64::INFINITY));
        assert!(is_finite(0.0));
        assert!(!is_finite(f64::INFINITY));
        assert!(is_integer(-0.0));
        assert!(is_integer(1e21));
        assert!(!is_integer(1.5));
        assert!(is_safe_integer(9_007_199_254_740_991.0));
        assert!(!is_safe_integer(9_007_199_254_740_992.0));
    }

    #[test]
    fn parse_int_consumes_the_longest_prefix() {
        assert_eq!(parse_int("  -101tail", 2), -5.0);
        assert_eq!(parse_int("+0x10z", 16), 16.0);
        assert_eq!(parse_int("z!", 36), 35.0);
        assert!(parse_int("2", 2).is_nan());
        assert_eq!(parse_int("\u{FEFF}11", 10), 11.0);
    }

    #[test]
    fn parse_float_consumes_the_longest_prefix() {
        assert_eq!(parse_float("  -1.5e2tail"), -150.0);
        assert_eq!(parse_float("1.5abc"), 1.5);
        assert_eq!(parse_float("1e+"), 1.0);
        assert_eq!(parse_float(".25x"), 0.25);
        assert_eq!(parse_float("+Infinity!"), f64::INFINITY);
        assert!(parse_float("x1").is_nan());
    }

    #[test]
    fn to_fixed_pins_ecma_edges() {
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-2.5, 0), "-3");
        assert_eq!(to_fixed(-0.0, 2), "0.00");
        assert_eq!(to_fixed(-0.0001, 2), "-0.00");
        assert_eq!(to_fixed(12.34, 4), "12.3400");
        assert_eq!(to_fixed(1e21, 2), "1e+21");
        assert_eq!(to_fixed(f64::NAN, 2), "NaN");
        assert_eq!(to_fixed(f64::INFINITY, 2), "Infinity");
        assert_eq!(to_fixed(f64::NEG_INFINITY, 2), "-Infinity");
    }

    #[test]
    fn radix_formatting_includes_fractions_and_q14_decimal() {
        assert_eq!(to_string_radix_f64(1234.5678, 36), "ya.kfv9yqdpm");
        assert_eq!(to_string_radix_f64(0.5, 2), "0.1");
        assert_eq!(to_string_radix_f64(-255.0, 16), "-ff");
        assert_eq!(
            to_string_radix_f64(f64::from_bits(0x5a3d_4d43_13f1_0c3d), 36),
            "4batab8o98w00000000000000000000000000000000000000000000000000000000000000000000000"
        );
        assert_eq!(
            to_string_radix_f64(1234.5678, 10),
            crate::fmt::fmt_f64(1234.5678)
        );
        assert_eq!(
            to_string_radix_f32(0.1, 10),
            crate::fmt::fmt_f32(0.1)
        );
        assert_eq!(to_string_radix_f64(f64::NAN, 16), "NaN");
        assert_eq!(to_string_radix_f64(f64::NEG_INFINITY, 2), "-Infinity");
    }

    #[test]
    fn exponential_formatting_has_exact_rounding_and_unpadded_exponents() {
        assert_eq!(to_exponential(123_456.0, None), "1.23456e+5");
        assert_eq!(to_exponential(0.0, Some(2)), "0.00e+0");
        assert_eq!(to_exponential(0.999, Some(0)), "1e+0");
        assert_eq!(to_exponential(-255.0, Some(1)), "-2.6e+2");
        assert_eq!(to_exponential(f64::from_bits(1), Some(1)), "4.9e-324");
        assert_eq!(to_exponential(f64::INFINITY, Some(2)), "Infinity");
    }

    #[test]
    fn precision_formatting_switches_at_the_ecma_boundaries() {
        assert_eq!(to_precision(123.456, 2), "1.2e+2");
        assert_eq!(to_precision(0.000123, 2), "0.00012");
        assert_eq!(to_precision(0.000000123, 2), "1.2e-7");
        assert_eq!(to_precision(0.0, 2), "0.0");
        assert_eq!(to_precision(9.99, 2), "10");
        assert_eq!(to_precision(f64::from_bits(1), 2), "4.9e-324");
        assert_eq!(to_precision(f64::NAN, 2), "NaN");
    }
}
