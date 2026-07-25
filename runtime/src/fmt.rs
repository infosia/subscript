//! Q14 numeric formatting (collisions.md §2).
//!
//! Template-literal interpolation is defined by this runtime, not the
//! host libc: integers in decimal; `f32`/`f64` by shortest round-trip
//! with ECMA's exponent thresholds (exponential outside
//! `[1e-6, 1e21)`); integral values in the ordinary range print
//! without a decimal point (`7`, never `7.0`); specials are spelled
//! `-0`, `NaN`, `Infinity`, `-Infinity`.
//!
//! `ryu-js` provides the ECMA shortest-round-trip digits, notation
//! thresholds, and special-value spellings. Q14 deliberately preserves
//! the sign of negative zero on top. Both execution tiers share this
//! implementation.

fn fmt_float<F: ryu_js::Float>(v: F) -> String {
    ryu_js::Buffer::new().format(v).to_owned()
}

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
    if v == 0.0 && v.is_sign_negative() {
        return "-0".to_string();
    }
    fmt_float(v)
}

/// Formats an `f64` by shortest round-trip (Q14).
#[must_use]
pub fn fmt_f64(v: f64) -> String {
    if v == 0.0 && v.is_sign_negative() {
        return "-0".to_string();
    }
    fmt_float(v)
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
    fn exact_decimal_ties_round_to_even() {
        assert_eq!(fmt_f64(2_205_594_957_347_911.25), "2205594957347911.2");
        assert_eq!(fmt_f32(2_261_740.25), "2261740.2");
    }

    #[test]
    fn integral_floats_in_the_ordinary_range_omit_the_decimal_point() {
        assert_eq!(fmt_f64(7.0), "7");
        assert_eq!(fmt_f32(7.0), "7");
        assert_eq!(fmt_f64(-3.0), "-3");
    }

    #[test]
    fn ecma_exponent_boundaries_and_extremes() {
        let below_lower = f64::from_bits(1e-6_f64.to_bits() - 1);
        let below_upper = f64::from_bits(1e21_f64.to_bits() - 1);
        let cases = [
            (1e-6, "0.000001"),
            (below_lower, "9.999999999999997e-7"),
            (1e21, "1e+21"),
            (below_upper, "999999999999999900000"),
            (f64::from_bits(1), "5e-324"),
            (1e300, "1e+300"),
            (-1e-6, "-0.000001"),
            (-below_lower, "-9.999999999999997e-7"),
            (-1e21, "-1e+21"),
            (-below_upper, "-999999999999999900000"),
            (-f64::from_bits(1), "-5e-324"),
            (-1e300, "-1e+300"),
        ];
        for (value, expected) in cases {
            assert_eq!(fmt_f64(value), expected);
        }
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
