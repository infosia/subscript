//! Deterministic `Number`, parsing, and `toFixed` operations (Q25).
//!
//! These functions implement the accepted ECMA surface once in Rust;
//! the dev-JIT and ship-C tiers both reach them through opaque
//! `sub_rt_num_*` symbols. Parsing never coerces non-strings. `toFixed`
//! uses `ryu-js`, so it does not inherit a host libc's decimal formatting
//! or rounding.

use num_bigint::BigUint;
use num_traits::ToPrimitive;

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

fn is_ecma_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\u{0009}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
            | '\n'
            | '\r'
    )
}

fn trim_start_ecma(value: &str) -> &str {
    value.trim_start_matches(is_ecma_whitespace)
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
}
