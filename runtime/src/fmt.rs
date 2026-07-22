//! Q14 numeric formatting (collisions.md §2).
//!
//! Template-literal interpolation is defined by this runtime, not the
//! host libc: integers in decimal; `f32`/`f64` by shortest round-trip
//! with integral values printed without a decimal point or exponent
//! (`7`, never `7.0` or `7E0`); specials spelled `-0`, `NaN`,
//! `Infinity`, `-Infinity`.
//!
//! Rust's std `{}` float display is shortest-round-trip, prints
//! integral values without `.0`, and preserves the sign of `-0`, but
//! spells the infinities `inf`/`-inf`; those two spellings are mapped
//! here. Both execution tiers share this one implementation.

/// Formats an `i32` in decimal.
#[must_use]
pub fn fmt_i32(v: i32) -> String {
    v.to_string()
}

/// Formats a `u32` in decimal.
#[must_use]
pub fn fmt_u32(v: u32) -> String {
    v.to_string()
}

/// Formats an `i64` in decimal.
#[must_use]
pub fn fmt_i64(v: i64) -> String {
    v.to_string()
}

/// Formats a `u64` in decimal.
#[must_use]
pub fn fmt_u64(v: u64) -> String {
    v.to_string()
}

/// Formats an `f32` by shortest round-trip at f32 precision (Q14).
#[must_use]
pub fn fmt_f32(v: f32) -> String {
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    // NaN, -0, and finite values already match the Q14 spellings.
    format!("{v}")
}

/// Formats an `f64` by shortest round-trip (Q14).
#[must_use]
pub fn fmt_f64(v: f64) -> String {
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    format!("{v}")
}

/// Formats a boolean as `true` / `false`.
#[must_use]
pub fn fmt_bool(v: bool) -> String {
    if v { "true" } else { "false" }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_are_decimal_at_the_extremes() {
        assert_eq!(fmt_i32(-12), "-12");
        assert_eq!(fmt_u32(u32::MAX), "4294967295");
        assert_eq!(fmt_i64(i64::MIN), "-9223372036854775808");
        assert_eq!(fmt_u64(u64::MAX), "18446744073709551615");
    }

    #[test]
    fn shortest_round_trip_fractions() {
        assert_eq!(fmt_f64(0.1), "0.1");
        assert_eq!(fmt_f64(1.5), "1.5");
        assert_eq!(fmt_f64(3.75), "3.75");
    }

    #[test]
    fn integral_floats_print_without_decimal_point_or_exponent() {
        assert_eq!(fmt_f64(7.0), "7");
        assert_eq!(fmt_f32(7.0), "7");
        assert_eq!(fmt_f64(-3.0), "-3");
        assert_eq!(fmt_f64(1e21), "1000000000000000000000");
    }

    #[test]
    fn negative_zero_keeps_its_sign() {
        assert_eq!(fmt_f64(-0.0), "-0");
        assert_eq!(fmt_f32(-0.0), "-0");
        assert_eq!(fmt_f64(0.0), "0");
    }

    #[test]
    fn specials_use_the_q14_spellings() {
        assert_eq!(fmt_f64(f64::NAN), "NaN");
        assert_eq!(fmt_f32(f32::NAN), "NaN");
        assert_eq!(fmt_f64(f64::INFINITY), "Infinity");
        assert_eq!(fmt_f64(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(fmt_f32(f32::INFINITY), "Infinity");
        assert_eq!(fmt_f32(f32::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn f32_uses_f32_shortest_form_not_f64() {
        // 0.1f32 promoted to f64 is 0.10000000149011612; the f32
        // shortest form is "0.1".
        assert_eq!(fmt_f32(0.1), "0.1");
        assert_eq!(fmt_f64(f64::from(0.1f32)), "0.10000000149011612");
        // A value with different shortest forms at the two widths.
        assert_eq!(fmt_f32(16777217.0_f32), "16777216");
        assert_eq!(fmt_f64(16777217.0_f64), "16777217");
    }

    #[test]
    fn booleans() {
        assert_eq!(fmt_bool(true), "true");
        assert_eq!(fmt_bool(false), "false");
    }
}
