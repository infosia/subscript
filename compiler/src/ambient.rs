//! Hardcoded ambient prelude surface (Q12): sized-numeric aliases and
//! the host functions `print`, `collect`, `unsafeDelete`.
//!
//! `prelude/lang.d.ts` is the `tsc`-facing reference for these
//! declarations; the checker does not parse it (P1 contract).

use crate::hir::{AmbientFn, MathFn};
use crate::types::Type;

/// Maps a sized-numeric alias name to its type.
pub(crate) fn sized_alias(name: &str) -> Option<Type> {
    match name {
        "i32" => Some(Type::I32),
        "u32" => Some(Type::U32),
        "i64" => Some(Type::I64),
        "u64" => Some(Type::U64),
        "f32" => Some(Type::F32),
        "f64" => Some(Type::F64),
        _ => None,
    }
}

/// Maps an ambient function name to its identity.
pub(crate) fn ambient_fn(name: &str) -> Option<AmbientFn> {
    match name {
        "print" => Some(AmbientFn::Print),
        "collect" => Some(AmbientFn::Collect),
        "unsafeDelete" => Some(AmbientFn::UnsafeDelete),
        _ => None,
    }
}

/// Parameter types of an ambient function (all return `void`).
pub(crate) fn ambient_params(f: AmbientFn) -> &'static [Type] {
    match f {
        AmbientFn::Print => &[Type::Str],
        AmbientFn::Collect => &[],
        AmbientFn::UnsafeDelete => &[Type::Object],
    }
}

/// Maps a `Math` member name to its intrinsic function (stdlib.md §1).
pub(crate) fn math_fn(name: &str) -> Option<MathFn> {
    MathFn::ALL.iter().copied().find(|f| f.name() == name)
}

/// Folded `f64` value of a `Math` constant member (stdlib.md §1): the
/// exact IEEE bit patterns of the Rust `std::f64::consts` doubles.
pub(crate) fn math_const(name: &str) -> Option<f64> {
    use std::f64::consts;
    match name {
        "E" => Some(consts::E),
        "LN2" => Some(consts::LN_2),
        "LN10" => Some(consts::LN_10),
        "LOG2E" => Some(consts::LOG2_E),
        "LOG10E" => Some(consts::LOG10_E),
        "PI" => Some(consts::PI),
        "SQRT1_2" => Some(consts::FRAC_1_SQRT_2),
        "SQRT2" => Some(consts::SQRT_2),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_cover_all_six_sized_numerics() {
        assert_eq!(sized_alias("i32"), Some(Type::I32));
        assert_eq!(sized_alias("u64"), Some(Type::U64));
        assert_eq!(sized_alias("f32"), Some(Type::F32));
        assert_eq!(sized_alias("number"), None);
    }

    #[test]
    fn math_fn_lookup_covers_the_subset_and_nothing_else() {
        assert_eq!(math_fn("floor"), Some(MathFn::Floor));
        assert_eq!(math_fn("atan2"), Some(MathFn::Atan2));
        assert_eq!(math_fn("random"), Some(MathFn::Random));
        // Out-of-subset JS-number ops resolve to nothing (Q19).
        assert_eq!(math_fn("imul"), None);
        assert_eq!(math_fn("clz32"), None);
        assert_eq!(math_fn("fround"), None);
        // Constants are not functions.
        assert_eq!(math_fn("PI"), None);
        // Every declared function round-trips through its name.
        for f in MathFn::ALL {
            assert_eq!(math_fn(f.name()), Some(f));
        }
    }

    #[test]
    fn math_consts_have_the_rust_f64_consts_bit_patterns() {
        use std::f64::consts;
        let cases: &[(&str, f64)] = &[
            ("E", consts::E),
            ("LN2", consts::LN_2),
            ("LN10", consts::LN_10),
            ("LOG2E", consts::LOG2_E),
            ("LOG10E", consts::LOG10_E),
            ("PI", consts::PI),
            ("SQRT1_2", consts::FRAC_1_SQRT_2),
            ("SQRT2", consts::SQRT_2),
        ];
        for (name, expected) in cases {
            let got = math_const(name).unwrap_or_else(|| panic!("missing const {name}"));
            assert_eq!(got.to_bits(), expected.to_bits(), "bits of Math.{name}");
        }
        assert_eq!(math_const("EPSILON"), None);
        assert_eq!(math_const("floor"), None);
    }

    #[test]
    fn ambient_functions_have_expected_shapes() {
        assert_eq!(ambient_fn("print"), Some(AmbientFn::Print));
        assert_eq!(ambient_params(AmbientFn::Print), &[Type::Str]);
        assert!(ambient_params(AmbientFn::Collect).is_empty());
        assert_eq!(ambient_params(AmbientFn::UnsafeDelete), &[Type::Object]);
        assert_eq!(ambient_fn("eval"), None);
    }
}
