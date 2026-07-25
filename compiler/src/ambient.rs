//! Hardcoded ambient prelude surface (Q12): sized-numeric aliases and
//! the host functions `print`, `collect`, `unsafeDelete`.
//!
//! `prelude/lang.d.ts` is the `tsc`-facing reference for these
//! declarations; the checker does not parse it (P1 contract).

use crate::hir::{AmbientFn, ArrFn, DateFn, MathFn, NumFn, StrFn};
use crate::types::Type;

/// Maps a sized-numeric alias name to its type.
pub(crate) fn sized_alias(name: &str) -> Option<Type> {
    match name {
        "i8" => Some(Type::I8),
        "u8" => Some(Type::U8),
        "i16" => Some(Type::I16),
        "u16" => Some(Type::U16),
        "i32" => Some(Type::I32),
        "u32" => Some(Type::U32),
        "i64" => Some(Type::I64),
        "u64" => Some(Type::U64),
        "f32" => Some(Type::F32),
        "f64" => Some(Type::F64),
        "f16" => Some(Type::F16),
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

/// Folded `f64` value of a `Number` constant (stdlib.md §11.1).
pub(crate) fn number_const(name: &str) -> Option<f64> {
    Some(match name {
        "MAX_SAFE_INTEGER" => 9_007_199_254_740_991.0,
        "MIN_SAFE_INTEGER" => -9_007_199_254_740_991.0,
        "EPSILON" => f64::EPSILON,
        "MAX_VALUE" => f64::MAX,
        "MIN_VALUE" => f64::from_bits(1),
        "POSITIVE_INFINITY" => f64::INFINITY,
        "NEGATIVE_INFINITY" => f64::NEG_INFINITY,
        "NaN" => f64::NAN,
        _ => return None,
    })
}

/// Accepted `Number.is*` predicate member.
pub(crate) fn number_predicate(name: &str) -> Option<NumFn> {
    Some(match name {
        "isNaN" => NumFn::IsNaN,
        "isFinite" => NumFn::IsFinite,
        "isInteger" => NumFn::IsInteger,
        "isSafeInteger" => NumFn::IsSafeInteger,
        _ => return None,
    })
}

/// Accepted global parser name.
pub(crate) fn number_global(name: &str) -> Option<NumFn> {
    Some(match name {
        "parseInt" => NumFn::ParseInt,
        "parseFloat" => NumFn::ParseFloat,
        _ => return None,
    })
}

/// Maps a `Date` instance-method name to its intrinsic (stdlib.md §3):
/// the eight UTC accessors and `toISOString`. `getTime` is not here —
/// it folds to the receiver value at check time — and the statics
/// (`UTC`, `now`) are resolved on the `Date` namespace, not a receiver.
pub(crate) fn date_method(name: &str) -> Option<DateFn> {
    Some(match name {
        "getUTCFullYear" => DateFn::GetUtcFullYear,
        "getUTCMonth" => DateFn::GetUtcMonth,
        "getUTCDate" => DateFn::GetUtcDate,
        "getUTCDay" => DateFn::GetUtcDay,
        "getUTCHours" => DateFn::GetUtcHours,
        "getUTCMinutes" => DateFn::GetUtcMinutes,
        "getUTCSeconds" => DateFn::GetUtcSeconds,
        "getUTCMilliseconds" => DateFn::GetUtcMilliseconds,
        "toISOString" => DateFn::ToIso,
        _ => return None,
    })
}

/// Maps a `String` method name to its intrinsic (stdlib.md §8, Q21).
/// `slice` is not here — it predates the §8 surface and stays on the
/// `Callee::Method` path; the out-of-subset members resolve to nothing.
pub(crate) fn str_method(name: &str) -> Option<StrFn> {
    StrFn::ALL.iter().copied().find(|f| f.name() == name)
}

/// Maps an `Array` method name to its intrinsic (stdlib.md §9, Q22).
/// `push`/`pop` are not here — they predate the §9 surface and stay on
/// the `Callee::Method` path; the out-of-subset members resolve to
/// nothing.
pub(crate) fn arr_method(name: &str) -> Option<ArrFn> {
    ArrFn::ALL.iter().copied().find(|f| f.name() == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_cover_all_sized_numerics() {
        assert_eq!(sized_alias("i8"), Some(Type::I8));
        assert_eq!(sized_alias("u16"), Some(Type::U16));
        assert_eq!(sized_alias("i32"), Some(Type::I32));
        assert_eq!(sized_alias("u64"), Some(Type::U64));
        assert_eq!(sized_alias("f32"), Some(Type::F32));
        assert_eq!(sized_alias("f16"), Some(Type::F16));
        assert_eq!(sized_alias("number"), None);
    }

    #[test]
    fn math_fn_lookup_covers_the_subset_and_nothing_else() {
        assert_eq!(math_fn("floor"), Some(MathFn::Floor));
        assert_eq!(math_fn("atan2"), Some(MathFn::Atan2));
        assert_eq!(math_fn("random"), Some(MathFn::Random));
        assert_eq!(math_fn("clz32"), Some(MathFn::Clz32));
        // Out-of-subset JS-number ops resolve to nothing (Q19).
        assert_eq!(math_fn("imul"), None);
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
    fn number_surface_lookups_cover_q25() {
        assert_eq!(number_const("MAX_SAFE_INTEGER"), Some(9_007_199_254_740_991.0));
        assert_eq!(number_const("MIN_VALUE").map(f64::to_bits), Some(1));
        assert!(number_const("NaN").is_some_and(f64::is_nan));
        assert_eq!(number_predicate("isNaN"), Some(NumFn::IsNaN));
        assert_eq!(number_predicate("isSafeInteger"), Some(NumFn::IsSafeInteger));
        assert_eq!(number_predicate("parseInt"), None);
        assert_eq!(number_global("parseInt"), Some(NumFn::ParseInt));
        assert_eq!(number_global("parseFloat"), Some(NumFn::ParseFloat));
        assert_eq!(number_global("isNaN"), None);
    }

    #[test]
    fn date_method_lookup_covers_the_subset_and_nothing_else() {
        assert_eq!(date_method("getUTCFullYear"), Some(DateFn::GetUtcFullYear));
        assert_eq!(date_method("getUTCDay"), Some(DateFn::GetUtcDay));
        assert_eq!(date_method("toISOString"), Some(DateFn::ToIso));
        // getTime folds at check time; it is not an intrinsic lookup.
        assert_eq!(date_method("getTime"), None);
        // Out-of-subset members resolve to nothing (Q20).
        assert_eq!(date_method("getFullYear"), None);
        assert_eq!(date_method("setTime"), None);
        assert_eq!(date_method("toString"), None);
        // Statics are namespace members, not instance methods.
        assert_eq!(date_method("UTC"), None);
        assert_eq!(date_method("now"), None);
    }

    #[test]
    fn str_method_lookup_covers_the_subset_and_nothing_else() {
        assert_eq!(str_method("indexOf"), Some(StrFn::IndexOf));
        assert_eq!(str_method("charCodeAt"), Some(StrFn::CharCodeAt));
        assert_eq!(str_method("replaceAll"), Some(StrFn::ReplaceAll));
        // Every declared method round-trips through its name.
        for f in StrFn::ALL {
            assert_eq!(str_method(f.name()), Some(f));
        }
        // `slice` stays on the standing Callee::Method path.
        assert_eq!(str_method("slice"), None);
        // Out-of-subset members resolve to nothing (Q21).
        assert_eq!(str_method("substring"), None);
        assert_eq!(str_method("localeCompare"), None);
        assert_eq!(str_method("match"), None);
        assert_eq!(str_method("toLocaleUpperCase"), None);
        assert_eq!(str_method("concat"), None);
    }

    #[test]
    fn arr_method_lookup_covers_the_subset_and_nothing_else() {
        assert_eq!(arr_method("indexOf"), Some(ArrFn::IndexOf));
        assert_eq!(arr_method("findIndex"), Some(ArrFn::FindIndex));
        assert_eq!(arr_method("sort"), Some(ArrFn::Sort));
        // Every declared method round-trips through its name.
        for f in ArrFn::ALL {
            assert_eq!(arr_method(f.name()), Some(f));
        }
        // `push`/`pop` stay on the standing Callee::Method path.
        assert_eq!(arr_method("push"), None);
        assert_eq!(arr_method("pop"), None);
        // Out-of-subset members resolve to nothing (Q22).
        assert_eq!(arr_method("find"), None);
        assert_eq!(arr_method("reduceRight"), None);
        assert_eq!(arr_method("splice"), None);
        assert_eq!(arr_method("flatMap"), None);
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
