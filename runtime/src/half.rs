//! IEEE 754 binary16 storage conversion (Q23).
//!
//! The language never performs arithmetic in this format. These helpers
//! convert between raw binary16 bits and `f64`; both generated tiers reach
//! them only through the opaque C-ABI entries in [`crate::ffi`].

/// Rounds `value / 2^shift` to the nearest integer, breaking ties toward
/// an even low bit.
fn round_shift_even(value: u64, shift: u32) -> u64 {
    if shift == 0 {
        return value;
    }
    let quotient = value >> shift;
    let mask = (1u64 << shift) - 1;
    let remainder = value & mask;
    let halfway = 1u64 << (shift - 1);
    if remainder > halfway || (remainder == halfway && quotient & 1 != 0) {
        quotient + 1
    } else {
        quotient
    }
}

/// Converts an `f64` to raw IEEE 754 binary16 bits, round-to-nearest-even.
pub(crate) fn from_f64(value: f64) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 48) & 0x8000) as u16;
    let exponent = ((bits >> 52) & 0x07ff) as u16;
    let fraction = bits & 0x000f_ffff_ffff_ffff;

    if exponent == 0x07ff {
        if fraction == 0 {
            return sign | 0x7c00;
        }
        let payload = ((fraction >> 42) as u16).max(1);
        return sign | 0x7c00 | payload;
    }
    if exponent == 0 {
        // Every nonzero f64 subnormal is far below binary16's minimum
        // subnormal. Signed zero keeps its sign through this path too.
        return sign;
    }

    let unbiased = i32::from(exponent) - 1023;
    let significand = (1u64 << 52) | fraction;
    if unbiased >= -14 {
        if unbiased > 15 {
            return sign | 0x7c00;
        }
        let rounded = round_shift_even(significand, 42);
        let mut half_exp = unbiased + 15;
        let mantissa = if rounded == 2048 {
            half_exp += 1;
            0
        } else {
            (rounded - 1024) as u16
        };
        if half_exp >= 31 {
            sign | 0x7c00
        } else {
            sign | ((half_exp as u16) << 10) | mantissa
        }
    } else if unbiased >= -25 {
        // A binary16 subnormal is `mantissa * 2^-24`. Re-express the
        // normalized f64 significand in those units before rounding.
        let shift = (28 - unbiased) as u32;
        let mantissa = round_shift_even(significand, shift);
        if mantissa == 1024 {
            sign | 0x0400
        } else {
            sign | mantissa as u16
        }
    } else {
        sign
    }
}

/// Converts raw IEEE 754 binary16 bits to an exactly represented `f64`.
pub(crate) fn to_f64(bits: u16) -> f64 {
    let sign = u64::from(bits & 0x8000) << 48;
    let exponent = (bits >> 10) & 0x001f;
    let mantissa = bits & 0x03ff;
    let out = match exponent {
        0 if mantissa == 0 => sign,
        0 => {
            let top = 15 - mantissa.leading_zeros();
            let unbiased = top as i32 - 24;
            let exp64 = ((unbiased + 1023) as u64) << 52;
            let leading = 1u16 << top;
            let frac64 = u64::from(mantissa - leading) << (52 - top);
            sign | exp64 | frac64
        }
        0x1f if mantissa == 0 => sign | (0x07ffu64 << 52),
        0x1f => sign | (0x07ffu64 << 52) | (u64::from(mantissa) << 42),
        _ => {
            let exp64 = ((i32::from(exponent) - 15 + 1023) as u64) << 52;
            sign | exp64 | (u64::from(mantissa) << 42)
        }
    };
    f64::from_bits(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary16_known_values_and_edges() {
        assert_eq!(from_f64(1.5), 0x3e00);
        assert_eq!(from_f64(-0.0), 0x8000);
        assert_eq!(from_f64(f64::INFINITY), 0x7c00);
        assert_eq!(from_f64(f64::NEG_INFINITY), 0xfc00);
        assert_eq!(from_f64(65_504.0), 0x7bff);
        assert_eq!(from_f64(65_520.0), 0x7c00);
        assert_eq!(from_f64(2.0f64.powi(-24)), 0x0001);
        assert_eq!(from_f64(2.0f64.powi(-25)), 0x0000);
        assert_eq!(from_f64(3.0 * 2.0f64.powi(-25)), 0x0002);
    }

    #[test]
    fn binary16_widening_is_exact_and_preserves_special_values() {
        assert_eq!(to_f64(0x3e00), 1.5);
        assert_eq!(to_f64(0x0001), 2.0f64.powi(-24));
        assert_eq!(to_f64(0x8000).to_bits(), (-0.0f64).to_bits());
        assert!(to_f64(0x7e01).is_nan());
        assert_eq!(from_f64(to_f64(0x7e01)) & 0x7c00, 0x7c00);
    }

    #[test]
    fn every_non_nan_binary16_value_round_trips() {
        for bits in 0u16..=u16::MAX {
            if bits & 0x7c00 == 0x7c00 && bits & 0x03ff != 0 {
                continue;
            }
            assert_eq!(from_f64(to_f64(bits)), bits, "{bits:#06x}");
        }
    }
}
