//! `Math` runtime (stdlib.md §1/§2): one Rust implementation per
//! accepted member, called by both tiers through the opaque
//! `sub_rt_math_*` boundary in [`crate::ffi`].
//!
//! Result semantics are ECMA-262's for the accepted subset. Most
//! members map directly to the `f64` methods; the ones whose Rust or
//! IEEE counterpart disagrees with ECMA are implemented explicitly and
//! pinned by tests: `round` (half-toward-+∞), `sign` (±0 preserved),
//! `max`/`min` (NaN propagation, zero ordering), `pow` (±1 to an
//! infinite exponent is NaN; a NaN exponent is NaN even for base 1).
//! `clz32` uses Rust's zero-defined [`u32::leading_zeros`] behind the
//! runtime boundary; generated C never emits `__builtin_clz`.
//! `imul` uses [`i32::wrapping_mul`], and `fround` performs the
//! contract's exact `f64 -> f32 -> f64` conversion.
//!
//! `Math.random` (§2) draws from [`Rng`], a xoshiro256++ generator
//! seeded by splitmix64 expansion; the state is owned by the Context so
//! every run is deterministic and host-reseedable.

/// `Math.abs`: magnitude; `abs(-0) === +0`.
#[must_use]
pub fn abs(x: f64) -> f64 {
    x.abs()
}

/// `Math.clz32`: count leading zero bits, including `clz32(0) == 32`.
#[must_use]
pub fn clz32(x: u32) -> i32 {
    x.leading_zeros() as i32
}

/// `Math.imul`: wrapping 32-bit signed multiplication.
#[must_use]
pub fn imul(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b)
}

/// `Math.fround`: round through IEEE binary32, then widen exactly back
/// to binary64.
#[must_use]
pub fn fround(x: f64) -> f64 {
    (x as f32) as f64
}

/// `Math.acos`.
#[must_use]
pub fn acos(x: f64) -> f64 {
    x.acos()
}

/// `Math.acosh`.
#[must_use]
pub fn acosh(x: f64) -> f64 {
    x.acosh()
}

/// `Math.asin`.
#[must_use]
pub fn asin(x: f64) -> f64 {
    x.asin()
}

/// `Math.asinh`.
#[must_use]
pub fn asinh(x: f64) -> f64 {
    x.asinh()
}

/// `Math.atan`.
#[must_use]
pub fn atan(x: f64) -> f64 {
    x.atan()
}

/// `Math.atanh`.
#[must_use]
pub fn atanh(x: f64) -> f64 {
    x.atanh()
}

/// `Math.cbrt`.
#[must_use]
pub fn cbrt(x: f64) -> f64 {
    x.cbrt()
}

/// `Math.ceil`.
#[must_use]
pub fn ceil(x: f64) -> f64 {
    x.ceil()
}

/// `Math.cos`.
#[must_use]
pub fn cos(x: f64) -> f64 {
    x.cos()
}

/// `Math.cosh`.
#[must_use]
pub fn cosh(x: f64) -> f64 {
    x.cosh()
}

/// `Math.exp`.
#[must_use]
pub fn exp(x: f64) -> f64 {
    x.exp()
}

/// `Math.expm1`.
#[must_use]
pub fn expm1(x: f64) -> f64 {
    x.exp_m1()
}

/// `Math.floor`.
#[must_use]
pub fn floor(x: f64) -> f64 {
    x.floor()
}

/// `Math.log` (natural logarithm).
#[must_use]
pub fn log(x: f64) -> f64 {
    x.ln()
}

/// `Math.log1p`.
#[must_use]
pub fn log1p(x: f64) -> f64 {
    x.ln_1p()
}

/// `Math.log10`.
#[must_use]
pub fn log10(x: f64) -> f64 {
    x.log10()
}

/// `Math.log2`.
#[must_use]
pub fn log2(x: f64) -> f64 {
    x.log2()
}

/// `Math.round`: ECMA half-toward-+∞ — `round(2.5) === 3` and
/// `round(-2.5) === -2` (Rust's `f64::round` is half-away-from-zero,
/// which disagrees on negative halves), and values in `[-0.5, -0)`
/// round to `-0`.
#[must_use]
pub fn round(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() || x == 0.0 {
        return x;
    }
    if x < 0.0 && x >= -0.5 {
        return -0.0;
    }
    // `x - floor(x)` is exact: the fractional part of a finite f64 is
    // representable at the same or finer scale than x itself.
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// `Math.sign`: `NaN` for `NaN`, `±0` for `±0`, else `±1`.
#[must_use]
pub fn sign(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        x
    } else if x > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// `Math.sin`.
#[must_use]
pub fn sin(x: f64) -> f64 {
    x.sin()
}

/// `Math.sinh`.
#[must_use]
pub fn sinh(x: f64) -> f64 {
    x.sinh()
}

/// `Math.sqrt`.
#[must_use]
pub fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

/// `Math.tan`.
#[must_use]
pub fn tan(x: f64) -> f64 {
    x.tan()
}

/// `Math.tanh`.
#[must_use]
pub fn tanh(x: f64) -> f64 {
    x.tanh()
}

/// `Math.trunc`.
#[must_use]
pub fn trunc(x: f64) -> f64 {
    x.trunc()
}

/// `Math.atan2(y, x)`.
#[must_use]
pub fn atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}

/// `Math.hypot(a, b)` — the two-argument subset (Q19).
#[must_use]
pub fn hypot(a: f64, b: f64) -> f64 {
    a.hypot(b)
}

/// `Math.pow(base, exp)`: ECMA semantics — `pow(x, ±0) === 1` for every
/// `x` including `NaN` (as IEEE `pow` does), `pow(x, NaN)` is `NaN` for
/// every `x` including `1` (where IEEE `pow(1, NaN)` returns 1), and
/// `pow(±1, ±Infinity)` is `NaN` (where IEEE `pow` returns 1).
#[must_use]
pub fn pow(base: f64, exp: f64) -> f64 {
    // A NaN exponent is never ±0, so this cannot shadow `pow(x, ±0) = 1`.
    if exp.is_nan() {
        return f64::NAN;
    }
    if exp.is_infinite() && (base == 1.0 || base == -1.0) {
        return f64::NAN;
    }
    base.powf(exp)
}

/// `Math.max(a, b)` — two arguments (Q19): propagates `NaN` and orders
/// zeros (`max(+0, -0) === +0`), which `f64::max` does not guarantee.
#[must_use]
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_positive() || b.is_sign_positive() {
            0.0
        } else {
            -0.0
        };
    }
    if a > b {
        a
    } else {
        b
    }
}

/// `Math.min(a, b)` — two arguments (Q19): propagates `NaN` and orders
/// zeros (`min(+0, -0) === -0`), which `f64::min` does not guarantee.
#[must_use]
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() || b.is_sign_negative() {
            -0.0
        } else {
            0.0
        };
    }
    if a < b {
        a
    } else {
        b
    }
}

// ----- Math.random (stdlib.md §2) -----

/// The contract default seed of the Context PRNG (stdlib.md §2); the
/// a41 golden pins the sequence it produces.
pub const DEFAULT_RANDOM_SEED: u64 = 0x5355_4253_5245_4144;

/// One splitmix64 step (the public algorithm definition): advances
/// `state` and returns the next output. Used only to expand a seed
/// into xoshiro256++ state.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The Context-owned PRNG behind `Math.random` (stdlib.md §2):
/// xoshiro256++ (public algorithm definition), seeded by splitmix64
/// expansion of a `u64` seed.
#[derive(Debug, Clone)]
pub struct Rng {
    state: [u64; 4],
}

impl Rng {
    /// Creates the generator by splitmix64-expanding `seed` into the
    /// four state words.
    #[must_use]
    pub fn new(seed: u64) -> Rng {
        let mut rng = Rng { state: [0; 4] };
        rng.reseed(seed);
        rng
    }

    /// Reseeds by re-expanding `seed`; the stream restarts exactly as a
    /// fresh [`Rng::new`] with the same seed.
    pub fn reseed(&mut self, seed: u64) {
        let mut s = seed;
        for w in &mut self.state {
            *w = splitmix64(&mut s);
        }
    }

    /// The next raw 64-bit xoshiro256++ output.
    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.state;
        let result = s[0].wrapping_add(s[3]).rotate_left(23).wrapping_add(s[0]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// The next `Math.random()` draw: the top 53 bits mapped to
    /// `[0, 1)` as `(x >> 11) * 2⁻⁵³` (stdlib.md §2).
    pub fn next_f64(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64) * (1.0 / (1u64 << 53) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(x: f64) -> u64 {
        x.to_bits()
    }

    #[test]
    fn round_is_half_toward_positive_infinity() {
        assert_eq!(round(2.4), 2.0);
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(-2.6), -3.0);
        assert_eq!(round(0.5), 1.0);
        assert_eq!(round(7.0), 7.0);
        assert!(round(f64::NAN).is_nan());
        assert_eq!(round(f64::INFINITY), f64::INFINITY);
        assert_eq!(round(f64::NEG_INFINITY), f64::NEG_INFINITY);
    }

    #[test]
    fn round_preserves_negative_zero_for_small_negatives() {
        assert_eq!(bits(round(-0.4)), bits(-0.0));
        assert_eq!(bits(round(-0.5)), bits(-0.0));
        assert_eq!(bits(round(-0.0)), bits(-0.0));
        assert_eq!(bits(round(0.0)), bits(0.0));
        assert_eq!(bits(round(0.4)), bits(0.0));
    }

    #[test]
    fn sign_returns_signed_zero_one_or_nan() {
        assert_eq!(sign(3.5), 1.0);
        assert_eq!(sign(-3.5), -1.0);
        assert_eq!(bits(sign(0.0)), bits(0.0));
        assert_eq!(bits(sign(-0.0)), bits(-0.0));
        assert!(sign(f64::NAN).is_nan());
    }

    #[test]
    fn max_min_propagate_nan_and_order_zeros() {
        assert!(max(f64::NAN, 1.0).is_nan());
        assert!(max(1.0, f64::NAN).is_nan());
        assert!(min(f64::NAN, 1.0).is_nan());
        assert!(min(1.0, f64::NAN).is_nan());
        assert_eq!(bits(max(0.0, -0.0)), bits(0.0));
        assert_eq!(bits(max(-0.0, 0.0)), bits(0.0));
        assert_eq!(bits(max(-0.0, -0.0)), bits(-0.0));
        assert_eq!(bits(min(0.0, -0.0)), bits(-0.0));
        assert_eq!(bits(min(-0.0, 0.0)), bits(-0.0));
        assert_eq!(bits(min(0.0, 0.0)), bits(0.0));
        assert_eq!(max(2.5, 7.0), 7.0);
        assert_eq!(min(2.5, 7.0), 2.5);
    }

    #[test]
    fn pow_follows_ecma_edges() {
        assert_eq!(pow(f64::NAN, 0.0), 1.0);
        assert_eq!(pow(f64::NAN, -0.0), 1.0);
        assert_eq!(pow(2.0, 10.0), 1024.0);
        assert!(pow(1.0, f64::INFINITY).is_nan());
        assert!(pow(-1.0, f64::NEG_INFINITY).is_nan());
    }

    #[test]
    fn pow_nan_exponent_is_nan_even_for_base_one() {
        // ECMA-262 Number::exponentiate step 1: NaN exponent yields NaN
        // with no exception for base 1 (IEEE pow(1, NaN) returns 1).
        assert!(pow(1.0, f64::NAN).is_nan());
        assert!(pow(-1.0, f64::NAN).is_nan());
        assert!(pow(0.0, f64::NAN).is_nan());
        // The ±0-exponent rule stays ahead of the NaN-exponent rule.
        assert_eq!(pow(f64::NAN, 0.0), 1.0);
    }

    #[test]
    fn abs_clears_the_sign_of_zero() {
        assert_eq!(bits(abs(-0.0)), bits(0.0));
        assert_eq!(abs(-3.5), 3.5);
    }

    #[test]
    fn clz32_is_defined_at_zero_and_bit_extremes() {
        assert_eq!(clz32(0), 32);
        assert_eq!(clz32(1), 31);
        assert_eq!(clz32(1_u32 << 31), 0);
        assert_eq!(clz32(u32::MAX), 0);
    }

    #[test]
    fn imul_wraps_and_fround_uses_binary32_precision() {
        assert_eq!(imul(i32::MAX, 2), -2);
        assert_eq!(imul(i32::MIN, -1), i32::MIN);
        assert_eq!(fround(1.1), 1.100_000_023_841_858);
        assert_eq!(fround(-0.0).to_bits(), (-0.0_f64).to_bits());
    }

    #[test]
    fn direct_wrappers_match_the_f64_methods() {
        // The pass-through members: one spot value each.
        assert_eq!(acos(1.0), 0.0);
        assert_eq!(acosh(1.0), 0.0);
        assert_eq!(asin(0.0), 0.0);
        assert_eq!(asinh(0.0), 0.0);
        assert_eq!(atan(0.0), 0.0);
        assert_eq!(atanh(0.0), 0.0);
        assert_eq!(cbrt(27.0), 3.0);
        assert_eq!(ceil(1.2), 2.0);
        assert_eq!(cos(0.0), 1.0);
        assert_eq!(cosh(0.0), 1.0);
        assert_eq!(exp(0.0), 1.0);
        assert_eq!(expm1(0.0), 0.0);
        assert_eq!(floor(1.8), 1.0);
        assert_eq!(log(1.0), 0.0);
        assert_eq!(log1p(0.0), 0.0);
        assert_eq!(log10(1000.0), 3.0);
        assert_eq!(log2(8.0), 3.0);
        assert_eq!(sin(0.0), 0.0);
        assert_eq!(sinh(0.0), 0.0);
        assert_eq!(sqrt(9.0), 3.0);
        assert!(sqrt(-1.0).is_nan());
        assert_eq!(tan(0.0), 0.0);
        assert_eq!(tanh(0.0), 0.0);
        assert_eq!(trunc(-1.7), -1.0);
        assert_eq!(atan2(0.0, 1.0), 0.0);
        assert_eq!(hypot(3.0, 4.0), 5.0);
    }

    #[test]
    fn rng_default_seed_pins_the_contract_sequence() {
        // The §2 contract sequence: first 8 draws from the default
        // seed, asserted by exact bit pattern. The a41 golden pins the
        // same values through Q14 formatting.
        let mut rng = Rng::new(DEFAULT_RANDOM_SEED);
        let drawn: Vec<u64> = (0..8).map(|_| rng.next_f64().to_bits()).collect();
        let expected: Vec<u64> = PINNED.iter().map(|s| parse_bits(s)).collect();
        assert_eq!(drawn, expected);
    }

    /// The pinned first-8 sequence as exact decimal strings (Q14
    /// shortest round-trip; the a41 golden lines).
    const PINNED: [&str; 8] = [
        "0.7085450778517304",
        "0.7056218027099065",
        "0.990983775174846",
        "0.11117172788539775",
        "0.15235247969973897",
        "0.43309704038842556",
        "0.028964527018885522",
        "0.8813338708298997",
    ];

    fn parse_bits(s: &str) -> u64 {
        s.parse::<f64>().expect("pinned literal").to_bits()
    }

    #[test]
    fn rng_pinned_sequence_formats_to_the_golden_lines() {
        let mut rng = Rng::new(DEFAULT_RANDOM_SEED);
        for expected in PINNED {
            assert_eq!(crate::fmt::fmt_f64(rng.next_f64()), expected);
        }
    }

    #[test]
    fn rng_reseed_restarts_the_stream() {
        let mut a = Rng::new(42);
        let first: Vec<u64> = (0..4).map(|_| a.next_u64()).collect();
        a.reseed(42);
        let again: Vec<u64> = (0..4).map(|_| a.next_u64()).collect();
        assert_eq!(first, again);
        let fresh: Vec<u64> = {
            let mut b = Rng::new(42);
            (0..4).map(|_| b.next_u64()).collect()
        };
        assert_eq!(first, fresh);
        // A different seed is a different stream.
        let mut c = Rng::new(43);
        assert_ne!(first[0], c.next_u64());
    }

    #[test]
    fn rng_draws_stay_in_the_half_open_unit_interval() {
        let mut rng = Rng::new(DEFAULT_RANDOM_SEED);
        for _ in 0..1000 {
            let x = rng.next_f64();
            assert!((0.0..1.0).contains(&x), "draw {x} out of [0, 1)");
        }
    }
}
