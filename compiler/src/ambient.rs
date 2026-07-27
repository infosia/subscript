//! Hardcoded ambient prelude surface (Q12): sized-numeric aliases and
//! the host functions `print`, `collect`, `unsafeDelete`.
//!
//! `prelude/lang.d.ts` is the `tsc`-facing reference for these
//! declarations; the checker does not parse it (P1 contract).

use crate::diag::RuleCode;
#[cfg(feature = "regex")]
use crate::hir::RegexFn;
use crate::hir::{AmbientFn, ArrFn, DateFn, MapFn, MathFn, NumFn, SetFn, StrFn};
use crate::types::Type;

/// One accepted checker-owned API entry rendered by the generated
/// reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApiItem {
    /// Receiver or namespace heading.
    pub group: &'static str,
    /// Source-level subscript signature.
    pub signature: String,
    /// Human-readable behavior summary, colocated with the checker table.
    pub summary: &'static str,
}

/// One named standard-library rejection consulted by the checker and
/// rendered by the generated reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApiRejection {
    /// Receiver, namespace, or source-form heading.
    pub group: &'static str,
    /// Rejected spelling or call shape.
    pub surface: &'static str,
    /// Stable checker diagnostic code.
    pub code: RuleCode,
    /// Collision-register rule.
    pub q_rule: &'static str,
    /// Accepted replacement, when the checker contract names one.
    pub replacement: Option<&'static str>,
    /// Human-readable reason.
    pub summary: &'static str,
    /// Reject-corpus entry pinning the code, when one exists.
    pub corpus: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
struct NamedF64 {
    name: &'static str,
    value: f64,
    summary: &'static str,
}

const MATH_CONSTS: &[NamedF64] = &[
    NamedF64 {
        name: "E",
        value: std::f64::consts::E,
        summary: "Euler's number.",
    },
    NamedF64 {
        name: "LN2",
        value: std::f64::consts::LN_2,
        summary: "Natural logarithm of 2.",
    },
    NamedF64 {
        name: "LN10",
        value: std::f64::consts::LN_10,
        summary: "Natural logarithm of 10.",
    },
    NamedF64 {
        name: "LOG2E",
        value: std::f64::consts::LOG2_E,
        summary: "Base-2 logarithm of E.",
    },
    NamedF64 {
        name: "LOG10E",
        value: std::f64::consts::LOG10_E,
        summary: "Base-10 logarithm of E.",
    },
    NamedF64 {
        name: "PI",
        value: std::f64::consts::PI,
        summary: "Ratio of a circle's circumference to its diameter.",
    },
    NamedF64 {
        name: "SQRT1_2",
        value: std::f64::consts::FRAC_1_SQRT_2,
        summary: "Square root of one half.",
    },
    NamedF64 {
        name: "SQRT2",
        value: std::f64::consts::SQRT_2,
        summary: "Square root of 2.",
    },
];

const NUMBER_CONSTS: &[NamedF64] = &[
    NamedF64 {
        name: "MAX_SAFE_INTEGER",
        value: 9_007_199_254_740_991.0,
        summary: "Largest exactly representable safe integer.",
    },
    NamedF64 {
        name: "MIN_SAFE_INTEGER",
        value: -9_007_199_254_740_991.0,
        summary: "Smallest exactly representable safe integer.",
    },
    NamedF64 {
        name: "EPSILON",
        value: f64::EPSILON,
        summary: "Difference between 1 and the next `f64`.",
    },
    NamedF64 {
        name: "MAX_VALUE",
        value: f64::MAX,
        summary: "Largest finite `f64`.",
    },
    NamedF64 {
        name: "MIN_VALUE",
        value: f64::from_bits(1),
        summary: "Smallest positive nonzero `f64`.",
    },
    NamedF64 {
        name: "POSITIVE_INFINITY",
        value: f64::INFINITY,
        summary: "Positive infinity.",
    },
    NamedF64 {
        name: "NEGATIVE_INFINITY",
        value: f64::NEG_INFINITY,
        summary: "Negative infinity.",
    },
    NamedF64 {
        name: "NaN",
        value: f64::NAN,
        summary: "The `f64` not-a-number value.",
    },
];

const STRING_REJECTIONS: &[ApiRejection] = &[
    rejection(
        "string",
        "at",
        "Q21",
        Some("slice"),
        "The language has no scalar miss value.",
        None,
    ),
    rejection(
        "string",
        "localeCompare",
        "Q21",
        None,
        "Locale-dependent collation is unavailable.",
        Some("r26-string-localecompare.ts"),
    ),
    rejection(
        "string",
        "toLocaleUpperCase",
        "Q21",
        Some("toUpperCase"),
        "Locale-sensitive case conversion is unavailable.",
        Some("r28-string-tolocaleupper.ts"),
    ),
    rejection(
        "string",
        "toLocaleLowerCase",
        "Q21",
        Some("toLowerCase"),
        "Locale-sensitive case conversion is unavailable.",
        None,
    ),
    rejection(
        "string",
        "normalize",
        "Q21",
        None,
        "Unicode normalization tables are unavailable.",
        None,
    ),
];

#[cfg(feature = "regex")]
const REGEX_STRING_REJECTIONS: &[ApiRejection] = &[
    rejection(
        "string",
        "match",
        "Q31",
        None,
        "`RegExpMatchArray.index` is optional under stock `tsc --strict`, so the result cannot satisfy the language's `i32` index contract.",
        Some("r27-string-match.ts"),
    ),
    rejection(
        "string",
        "matchAll",
        "Q31/Q30",
        None,
        "It needs a Q30 fusion decision and each iteration step still yields an object.",
        Some("r81-regex-match-all.ts"),
    ),
];

#[cfg(not(feature = "regex"))]
const REGEX_STRING_REJECTIONS: &[ApiRejection] = &[
    rejection(
        "string",
        "match",
        "Q31",
        None,
        "This build does not include the `regex` Cargo feature.",
        Some("r27-string-match.ts"),
    ),
    rejection(
        "string",
        "matchAll",
        "Q31",
        None,
        "This build does not include the `regex` Cargo feature.",
        Some("r89-regex-off-match-all.ts"),
    ),
    rejection(
        "string",
        "search",
        "Q31",
        None,
        "This build does not include the `regex` Cargo feature.",
        Some("r90-regex-off-search.ts"),
    ),
];

#[cfg(feature = "regex")]
const REGEX_REJECTIONS: &[ApiRejection] = &[
    rejection(
        "RegExp",
        "exec",
        "Q31",
        None,
        "Its result needs an array with extra fields and a tuple type, neither of which the language has.",
        Some("r80-regex-exec.ts"),
    ),
    rejection(
        "RegExp",
        "lastIndex",
        "Q31",
        None,
        "Mutable global-match state would drive `exec`, whose result is not representable.",
        Some("r82-regex-last-index.ts"),
    ),
    rejection(
        "RegExpMatchArray",
        "groups",
        "Q31",
        None,
        "Named groups require an object with dynamic keys, which the language does not have.",
        Some("r83-regex-groups.ts"),
    ),
];

const ARRAY_REJECTIONS: &[ApiRejection] = &[
    rejection(
        "T[]",
        "find",
        "Q22",
        Some("findIndex"),
        "A scalar element type has no miss value.",
        Some("r30-array-find.ts"),
    ),
    rejection(
        "T[]",
        "findLast",
        "Q22",
        Some("findIndex"),
        "A scalar element type has no miss value.",
        None,
    ),
    rejection(
        "T[]",
        "flat",
        "Q22",
        None,
        "Runtime flattening depth cannot determine a static result type.",
        None,
    ),
    rejection(
        "T[]",
        "flatMap",
        "Q22",
        None,
        "Runtime flattening depth cannot determine a static result type.",
        None,
    ),
    rejection(
        "T[]",
        "entries",
        "Q30",
        None,
        "`entries()` yields a pair, but the language has no tuple type.",
        None,
    ),
    rejection(
        "T[]",
        "keys",
        "Q30",
        None,
        "`keys()` is accepted only as the direct subject of `for…of`; elsewhere \
         it would create a stateful iterator value that outlives its call.",
        None,
    ),
    rejection(
        "T[]",
        "values",
        "Q30",
        None,
        "`values()` is accepted only as the direct subject of `for…of`; elsewhere \
         it would create a stateful iterator value that outlives its call.",
        None,
    ),
];

const DATE_LOCAL_REJECTIONS: &[ApiRejection] = &[
    rejection(
        "Date",
        "getFullYear",
        "Q20",
        Some("getUTCFullYear"),
        "Local-time accessors are unavailable.",
        Some("r19-date-local-accessor.ts"),
    ),
    rejection(
        "Date",
        "getMonth",
        "Q20",
        Some("getUTCMonth"),
        "Local-time accessors are unavailable.",
        None,
    ),
    rejection(
        "Date",
        "getDate",
        "Q20",
        Some("getUTCDate"),
        "Local-time accessors are unavailable.",
        None,
    ),
    rejection(
        "Date",
        "getDay",
        "Q20",
        Some("getUTCDay"),
        "Local-time accessors are unavailable.",
        None,
    ),
    rejection(
        "Date",
        "getHours",
        "Q20",
        Some("getUTCHours"),
        "Local-time accessors are unavailable.",
        None,
    ),
    rejection(
        "Date",
        "getMinutes",
        "Q20",
        Some("getUTCMinutes"),
        "Local-time accessors are unavailable.",
        None,
    ),
    rejection(
        "Date",
        "getSeconds",
        "Q20",
        Some("getUTCSeconds"),
        "Local-time accessors are unavailable.",
        None,
    ),
    rejection(
        "Date",
        "getMilliseconds",
        "Q20",
        Some("getUTCMilliseconds"),
        "Local-time accessors are unavailable.",
        None,
    ),
    rejection(
        "Date",
        "getTimezoneOffset",
        "Q20",
        None,
        "The runtime has no timezone database.",
        None,
    ),
    rejection(
        "Date",
        "getYear",
        "Q20",
        Some("getUTCFullYear"),
        "Local-time accessors are unavailable.",
        None,
    ),
];

const DATE_STRING_REJECTIONS: &[ApiRejection] = &[
    rejection(
        "Date",
        "toString",
        "Q20",
        Some("toISOString"),
        "Local-time formatting is unavailable.",
        None,
    ),
    rejection(
        "Date",
        "toDateString",
        "Q20",
        Some("toISOString"),
        "Local-time formatting is unavailable.",
        None,
    ),
    rejection(
        "Date",
        "toTimeString",
        "Q20",
        Some("toISOString"),
        "Local-time formatting is unavailable.",
        None,
    ),
    rejection(
        "Date",
        "toLocaleString",
        "Q20",
        Some("toISOString"),
        "Locale and timezone formatting is unavailable.",
        None,
    ),
    rejection(
        "Date",
        "toLocaleDateString",
        "Q20",
        Some("toISOString"),
        "Locale and timezone formatting is unavailable.",
        None,
    ),
    rejection(
        "Date",
        "toLocaleTimeString",
        "Q20",
        Some("toISOString"),
        "Locale and timezone formatting is unavailable.",
        None,
    ),
    rejection(
        "Date",
        "toUTCString",
        "Q20",
        Some("toISOString"),
        "Outside the checker-owned Date formatting subset.",
        None,
    ),
    rejection(
        "Date",
        "toJSON",
        "Q20",
        Some("toISOString"),
        "Outside the checker-owned Date formatting subset.",
        None,
    ),
    rejection(
        "Date",
        "valueOf",
        "Q20",
        Some("getTime"),
        "Implicit Date numeric conversion is unavailable.",
        None,
    ),
];

const MAP_REJECTIONS: &[ApiRejection] = &[
    rejection(
        "Map<K, V>",
        "keys",
        "Q30",
        Some("use directly as a for…of subject"),
        "`keys()` is accepted only as the direct subject of `for…of`; elsewhere it \
         would create a stateful iterator value that outlives its call.",
        Some("r42-map-iterator-member.ts"),
    ),
    rejection(
        "Map<K, V>",
        "values",
        "Q30",
        Some("use directly as a for…of subject"),
        "`values()` is accepted only as the direct subject of `for…of`; elsewhere it \
         would create a stateful iterator value that outlives its call.",
        None,
    ),
    rejection(
        "Map<K, V>",
        "entries",
        "Q30",
        None,
        "`entries()` yields a pair, but the language has no tuple type.",
        Some("r79-assign-entries.ts"),
    ),
];

const SET_REJECTIONS: &[ApiRejection] = &[
    rejection(
        "Set<K>",
        "keys",
        "Q30",
        Some("use directly as a for…of subject"),
        "`keys()` is accepted only as the direct subject of `for…of`; elsewhere it \
         would create a stateful iterator value that outlives its call.",
        None,
    ),
    rejection(
        "Set<K>",
        "values",
        "Q30",
        Some("use directly as a for…of subject"),
        "`values()` is accepted only as the direct subject of `for…of`; elsewhere it \
         would create a stateful iterator value that outlives its call.",
        None,
    ),
    rejection(
        "Set<K>",
        "entries",
        "Q30",
        None,
        "`entries()` yields a pair, but the language has no tuple type.",
        None,
    ),
];

const JSON_REJECTIONS: &[ApiRejection] = &[
    rejection(
        "JSON",
        "stringify(Map<K, V>)",
        "Q28",
        None,
        "Map is rejected rather than silently serialized as an empty object.",
        Some("r56-json-stringify-map.ts"),
    ),
    rejection(
        "JSON",
        "stringify(Set<K>)",
        "Q28",
        None,
        "Set is rejected rather than silently serialized as an empty object.",
        Some("r57-json-stringify-set.ts"),
    ),
    rejection(
        "JSON",
        "stringify(object)",
        "Q28",
        None,
        "The boundary-opaque object type has no static field shape to serialize.",
        Some("r58-json-stringify-object.ts"),
    ),
    rejection(
        "JSON",
        "stringify(function)",
        "Q28",
        None,
        "Function values are not JSON data.",
        Some("r59-json-stringify-function.ts"),
    ),
    rejection(
        "JSON",
        "stringify(f16)",
        "Q28",
        None,
        "f16 is a storage-only type with no arithmetic/formatting domain.",
        None,
    ),
    rejection(
        "JSON",
        "parse(text) without target type",
        "Q28",
        Some("JSON.parse<T>(text)"),
        "The checker has no static type to monomorphize.",
        Some("r60-json-parse-no-context.ts"),
    ),
    rejection(
        "JSON",
        "parse<Date>(text)",
        "Q28",
        None,
        "An untagged ISO string cannot identify a Date, so the target could never match.",
        Some("r61-json-parse-date.ts"),
    ),
];

const FORM_REJECTIONS: &[ApiRejection] = &[
    rejection("global", "isNaN(value)", "Q25", Some("Number.isNaN"), "The global form coerces its argument.", Some("r46-number-global-isnan.ts")),
    rejection("global", "isFinite(value)", "Q25", Some("Number.isFinite"), "The global form coerces its argument.", None),
    rejection("global", "parseInt(value)", "Q25", Some("parseInt(value, radix)"), "The radix is a required `i32` argument.", Some("r50-parse-int-no-radix.ts")),
    rejection("Number", "Number(value)", "Q25", Some("value as f64"), "Numeric coercion is not part of the language.", Some("r47-number-coercion.ts")),
    rejection("Number", "new Number(value)", "Q25", Some("value as f64"), "Boxed numbers and numeric coercion are unavailable.", None),
    rejection("f32 / f64", "toLocaleString", "Q25", None, "Locale-sensitive number formatting is unavailable.", None),
    rejection("f32 / f64", "toString()", "Q26", Some("toString(radix)"), "An explicit radix is required.", Some("r49-number-to-string-radix.ts")),
    rejection("f32 / f64", "toPrecision()", "Q26", Some("toPrecision(digits)"), "An explicit digit count is required.", Some("r48-number-to-precision.ts")),
    rejection("sized integers", "toFixed/toString/toExponential/toPrecision", "Q25/Q26", Some("convert to f32 or f64 first"), "Number formatting methods are accepted only on floating-point receivers.", None),
    rejection("Math", "max/min/hypot with more than two arguments", "Q19", None, "Variadic parameters are outside the language.", Some("r16-math-variadic-max.ts")),
    rejection("Math", "Math used as a value", "Q19", Some("Math.<member>"), "Math is a compiler-owned namespace.", Some("r18-math-value.ts")),
    rejection("Date", "Date.parse", "Q20", Some("Date.UTC"), "Parsing depends on timezone rules the runtime does not provide.", None),
    rejection("Date", "new Date()", "Q20", Some("new Date(Date.now())"), "The zero-argument constructor reads nondeterministic current time.", Some("r23-date-zero-arg-ctor.ts")),
    rejection("Date", "new Date(year, month, ...)", "Q20", Some("new Date(Date.UTC(...))"), "The multi-argument constructor uses local time.", Some("r21-date-multiarg-ctor.ts")),
    rejection("Date", "template interpolation", "Q20", Some("toISOString"), "Date has no implicit local-time string form.", Some("r22-date-template.ts")),
    rejection("Date", "direct comparison", "Q20", Some("compare getTime() values"), "Date values do not compare implicitly.", Some("r24-date-compare.ts")),
    rejection("Date", "set*", "Q20", Some("construct a new Date"), "Date is an immutable value.", Some("r20-date-setter.ts")),
    rejection("T[]", "sort()", "Q22", Some("sort(comparator)"), "The no-argument overload coerces elements to strings.", Some("r29-array-sort-noarg.ts")),
    rejection("T[]", "reduce(callback)", "Q22", Some("reduce(callback, init)"), "An explicit initial accumulator is required.", Some("r31-array-reduce-noinit.ts")),
    rejection("T[]", "reduceRight(callback)", "Q27", Some("reduceRight(callback, init)"), "An explicit initial accumulator is required.", None),
    rejection("T[]", "callback(value, index, array)", "Q27", Some("callback(value, index)"), "Passing the iterated container reference to its callback violates C5's non-escaping-by-construction rule.", Some("r55-array-callback-container.ts")),
    rejection("T[]", "splice(start, deleteCount, ...items)", "Q27", None, "Variadic parameters are the missing prerequisite for insertion through `splice`.", Some("r32-array-splice.ts")),
    rejection("T[]", "unshift(value, ...values)", "Q27", None, "Variadic parameters are the missing prerequisite for prepending multiple elements.", Some("r51-array-unshift-variadic.ts")),
    rejection("FixedArray<T, N>", "non-callback T[] methods", "Q22/Q27", None, "Q27 accepts the closure-taking callback family; the other checker-owned Array methods remain dynamic-array-only.", None),
    rejection("Map<K, scalar V>", "get(key)", "Q24", Some("getOr"), "A scalar value type has no null miss value.", Some("r41-map-scalar-get.ts")),
    rejection("Map / Set", "new Map/Set(iterable)", "Q30", Some("construct empty, then add/set"), "`new Map([[k, v]])` requires a pair element, but the language has no tuple type.", Some("r43-map-iterable-constructor.ts")),
    rejection("Object", "groupBy", "Q27", None, "It returns a null-prototype object, and the language has no such type.", Some("r52-object-groupby.ts")),
    rejection("Set<K>", "algebra(non-Set)", "Q27", Some("pass a Set<K>"), "The language has no set-like protocol.", Some("r53-set-algebra-nonset.ts")),
];

const fn rejection(
    group: &'static str,
    surface: &'static str,
    q_rule: &'static str,
    replacement: Option<&'static str>,
    summary: &'static str,
    corpus: Option<&'static str>,
) -> ApiRejection {
    ApiRejection {
        group,
        surface,
        code: RuleCode::S014,
        q_rule,
        replacement,
        summary,
        corpus,
    }
}

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
    AmbientFn::ALL.iter().copied().find(|f| f.name() == name)
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
    MATH_CONSTS.iter().find(|c| c.name == name).map(|c| c.value)
}

/// Folded `f64` value of a `Number` constant (stdlib.md §11.1).
pub(crate) fn number_const(name: &str) -> Option<f64> {
    NUMBER_CONSTS
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.value)
}

/// Accepted `Number` namespace call. The parser variants are the same
/// [`NumFn`] identities as the globals, so checker and runtime
/// implementations cannot fork.
pub(crate) fn number_static(name: &str) -> Option<NumFn> {
    Some(match name {
        "isNaN" => NumFn::IsNaN,
        "isFinite" => NumFn::IsFinite,
        "isInteger" => NumFn::IsInteger,
        "isSafeInteger" => NumFn::IsSafeInteger,
        "parseInt" => NumFn::ParseInt,
        "parseFloat" => NumFn::ParseFloat,
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
    DateFn::ALL
        .iter()
        .copied()
        .filter(|f| !matches!(f, DateFn::New | DateFn::Utc | DateFn::Now))
        .find(|f| f.name() == name)
}

/// Maps a `String` method name to its intrinsic (stdlib.md §8, Q21).
/// The out-of-subset members resolve to nothing.
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

/// Maps a `Map` method name to the intrinsic table the checker lowers.
pub(crate) fn map_method(name: &str) -> Option<MapFn> {
    MapFn::ALL
        .iter()
        .copied()
        .filter(|f| !matches!(f, MapFn::New | MapFn::Size | MapFn::GroupBy))
        .find(|f| f.name() == name)
}

/// Maps a `Set` method name to the intrinsic table the checker lowers.
pub(crate) fn set_method(name: &str) -> Option<SetFn> {
    SetFn::ALL
        .iter()
        .copied()
        .filter(|f| !matches!(f, SetFn::New | SetFn::Size))
        .find(|f| f.name() == name)
}

/// Checker-owned accepted API projection used by the Markdown
/// generator.
pub(crate) fn accepted_api() -> Vec<ApiItem> {
    let mut out = Vec::new();
    for f in AmbientFn::ALL {
        out.push(ApiItem {
            group: "Global",
            signature: f.api_signature().to_string(),
            summary: f.api_summary(),
        });
    }
    out.push(ApiItem {
        group: "Global",
        signature: "NaN: f64".to_string(),
        summary: "Ambient NaN literal used by floating-point APIs.",
    });
    for f in [NumFn::ParseInt, NumFn::ParseFloat] {
        out.push(ApiItem {
            group: "Global",
            signature: f.api_signature().to_string(),
            summary: f.api_summary(),
        });
    }
    for c in MATH_CONSTS {
        out.push(ApiItem {
            group: "Math",
            signature: format!("{}: f64", c.name),
            summary: c.summary,
        });
    }
    for f in MathFn::ALL {
        out.push(ApiItem {
            group: "Math",
            signature: f.api_signature(),
            summary: f.api_summary(),
        });
    }
    for c in NUMBER_CONSTS {
        out.push(ApiItem {
            group: "Number",
            signature: format!("{}: f64", c.name),
            summary: c.summary,
        });
    }
    for f in [
        NumFn::IsNaN,
        NumFn::IsFinite,
        NumFn::IsInteger,
        NumFn::IsSafeInteger,
        NumFn::ParseInt,
        NumFn::ParseFloat,
    ] {
        out.push(ApiItem {
            group: "Number",
            signature: f.api_signature().to_string(),
            summary: f.api_summary(),
        });
    }
    for f in DateFn::ALL {
        let group = match f {
            DateFn::New => "Date constructor",
            DateFn::Utc | DateFn::Now => "Date",
            _ => "Date instance",
        };
        out.push(ApiItem {
            group,
            signature: f.api_signature().to_string(),
            summary: f.api_summary(),
        });
    }
    out.push(ApiItem {
        group: "Date instance",
        signature: "getTime(): i64".to_string(),
        summary: "Returns epoch milliseconds.",
    });
    for (group, f) in [
        ("f32", NumFn::ToFixed),
        ("f32", NumFn::ToStringF32),
        ("f32", NumFn::ToExponential),
        ("f32", NumFn::ToPrecision),
        ("f64", NumFn::ToFixed),
        ("f64", NumFn::ToStringF64),
        ("f64", NumFn::ToExponential),
        ("f64", NumFn::ToPrecision),
    ] {
        out.push(ApiItem {
            group,
            signature: f.api_signature().to_string(),
            summary: f.api_summary(),
        });
    }
    out.push(ApiItem {
        group: "string",
        signature: "length: i32".to_string(),
        summary: "Returns the UTF-8 byte length.",
    });
    for f in StrFn::ALL {
        out.push(ApiItem {
            group: "string",
            signature: f.api_signature().to_string(),
            summary: f.api_summary(),
        });
    }
    #[cfg(feature = "regex")]
    {
        out.push(ApiItem {
            group: "RegExp [requires feature `regex`]",
            signature: "/pattern/flags: RegExp".to_string(),
            summary: "Compiles a checker-validated literal through the Context pattern cache.",
        });
        for f in RegexFn::ALL {
            let group = match f {
                RegexFn::New => "RegExp constructor [requires feature `regex`]",
                RegexFn::Search | RegexFn::Replace | RegexFn::ReplaceAll | RegexFn::Split => {
                    "string [requires feature `regex`]"
                }
                _ => "RegExp [requires feature `regex`]",
            };
            out.push(ApiItem {
                group,
                signature: f.api_signature().to_string(),
                summary: f.api_summary(),
            });
        }
    }
    for (signature, summary) in [
        ("length: i32", "Returns the element count."),
        (
            "push(value: T): i32",
            "Appends one element and returns the new length.",
        ),
        (
            "pop(): T",
            "Removes the last element; an empty array traps.",
        ),
    ] {
        out.push(ApiItem {
            group: "T[]",
            signature: signature.to_string(),
            summary,
        });
    }
    for f in ArrFn::ALL {
        out.push(ApiItem {
            group: "T[]",
            signature: f.api_signature().to_string(),
            summary: f.api_summary(),
        });
    }
    out.push(ApiItem {
        group: "FixedArray<T, N>",
        signature: "length: i32".to_string(),
        summary: "Returns the compile-time fixed element count.",
    });
    for f in ArrFn::ALL
        .into_iter()
        .filter(|f| f.fixed_symbol().is_some())
    {
        out.push(ApiItem {
            group: "FixedArray<T, N>",
            signature: f.api_signature().to_string(),
            summary: f.api_summary(),
        });
    }
    for f in MapFn::ALL {
        out.push(ApiItem {
            group: match f {
                MapFn::New => "Map constructor",
                MapFn::GroupBy => "Map",
                _ => "Map<K, V>",
            },
            signature: f.api_signature().to_string(),
            summary: f.api_summary(),
        });
    }
    for f in SetFn::ALL {
        out.push(ApiItem {
            group: if f == SetFn::New {
                "Set constructor"
            } else {
                "Set<K>"
            },
            signature: f.api_signature().to_string(),
            summary: f.api_summary(),
        });
    }
    out.push(ApiItem {
        group: "JSON",
        signature: "stringify<T>(value: T): string".to_string(),
        summary: "Serializes one statically known P13 type; cycle tracking is emitted only when its reference-class field graph can cycle.",
    });
    out.push(ApiItem {
        group: "JSON",
        signature: "parse<T>(text: string): JsonResult<T>".to_string(),
        summary: "Parses and validates one statically known P13 type; malformed, mismatched, or over-128-depth data returns ok=false, and the caller releases the result with unsafeDelete.",
    });
    out.extend([
        ApiItem {
            group: "JsonResult<T>",
            signature: "ok: boolean".to_string(),
            summary: "Reports whether parsing and complete static-type validation succeeded.",
        },
        ApiItem {
            group: "JsonResult<T>",
            signature: "value: T".to_string(),
            summary: "Carries the parsed value on success; reading it when ok is false traps.",
        },
    ]);
    out.extend([
        ApiItem {
            group: "Generator<T>",
            signature: "next(): IteratorResult<T>".to_string(),
            summary: "Resumes a coroutine and returns its step result.",
        },
        ApiItem {
            group: "IteratorResult<T>",
            signature: "done: boolean".to_string(),
            summary: "Reports whether the coroutine has completed.",
        },
        ApiItem {
            group: "IteratorResult<T>",
            signature: "value: T".to_string(),
            summary: "Carries the yielded value, or the zero-initialized value when done.",
        },
    ]);
    out
}

/// Every checker-owned named rejection rendered by the generated
/// reference.
pub(crate) fn rejected_api() -> Vec<ApiRejection> {
    let out: Vec<ApiRejection> = [
        STRING_REJECTIONS,
        REGEX_STRING_REJECTIONS,
        ARRAY_REJECTIONS,
        DATE_LOCAL_REJECTIONS,
        DATE_STRING_REJECTIONS,
        MAP_REJECTIONS,
        SET_REJECTIONS,
        JSON_REJECTIONS,
        FORM_REJECTIONS,
    ]
    .into_iter()
    .flatten()
    .copied()
    .collect();
    #[cfg(feature = "regex")]
    let out = {
        let mut out = out;
        out.extend(REGEX_REJECTIONS.iter().copied());
        out
    };
    out
}

/// Named String rejection, if the checker gives the member an S014
/// subset diagnostic rather than the generic S100 surface diagnostic.
pub(crate) fn string_rejection(name: &str) -> Option<ApiRejection> {
    STRING_REJECTIONS
        .iter()
        .chain(REGEX_STRING_REJECTIONS)
        .copied()
        .find(|r| r.surface == name)
}

/// Named Array rejection, if the checker gives the member an S014
/// subset diagnostic rather than the generic S100 surface diagnostic.
pub(crate) fn array_rejection(name: &str) -> Option<ApiRejection> {
    ARRAY_REJECTIONS.iter().copied().find(|r| r.surface == name)
}

/// Named Date instance rejection. Setter spellings share the generated
/// `set*` row.
pub(crate) fn date_rejection(name: &str) -> Option<ApiRejection> {
    DATE_LOCAL_REJECTIONS
        .iter()
        .chain(DATE_STRING_REJECTIONS)
        .copied()
        .find(|r| r.surface == name)
        .or_else(|| {
            name.starts_with("set").then_some(()).and_then(|()| {
                FORM_REJECTIONS
                    .iter()
                    .copied()
                    .find(|r| r.group == "Date" && r.surface == "set*")
            })
        })
}

/// Named Map rejection.
pub(crate) fn map_rejection(name: &str) -> Option<ApiRejection> {
    MAP_REJECTIONS.iter().copied().find(|r| r.surface == name)
}

/// Named Set rejection.
pub(crate) fn set_rejection(name: &str) -> Option<ApiRejection> {
    SET_REJECTIONS.iter().copied().find(|r| r.surface == name)
}

/// Named JSON.stringify rejection selected from the checked static type.
pub(crate) fn json_rejection(ty: &Type) -> Option<ApiRejection> {
    let surface = match ty {
        Type::Map(..) => "stringify(Map<K, V>)",
        Type::Set(_) => "stringify(Set<K>)",
        Type::Object => "stringify(object)",
        Type::Func(_) => "stringify(function)",
        Type::F16 => "stringify(f16)",
        _ => return None,
    };
    JSON_REJECTIONS
        .iter()
        .copied()
        .find(|rejection| rejection.surface == surface)
}

/// The named rejection for a JSON.parse target containing Date.
pub(crate) fn json_parse_date_rejection() -> ApiRejection {
    JSON_REJECTIONS
        .iter()
        .copied()
        .find(|rejection| rejection.surface == "parse<Date>(text)")
        .expect("JSON.parse<Date> rejection metadata")
}

/// Checker-owned rejection for a non-member call or source form.
pub(crate) fn form_rejection(group: &str, surface: &str) -> Option<ApiRejection> {
    FORM_REJECTIONS
        .iter()
        .copied()
        .find(|rejection| rejection.group == group && rejection.surface == surface)
}

/// Formats the diagnostic shared by a named checker rejection and the
/// generated API-reference row.
pub(crate) fn rejection_message(rejection: ApiRejection, actual: &str) -> String {
    match rejection.replacement {
        Some(replacement) => format!(
            "`{actual}` is rejected: {}; use `{replacement}` ({})",
            rejection.summary, rejection.q_rule
        ),
        None => format!(
            "`{actual}` is rejected: {} ({})",
            rejection.summary, rejection.q_rule
        ),
    }
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
        assert_eq!(math_fn("imul"), Some(MathFn::Imul));
        assert_eq!(math_fn("fround"), Some(MathFn::Fround));
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
        assert_eq!(
            number_const("MAX_SAFE_INTEGER"),
            Some(9_007_199_254_740_991.0)
        );
        assert_eq!(number_const("MIN_VALUE").map(f64::to_bits), Some(1));
        assert!(number_const("NaN").is_some_and(f64::is_nan));
        assert_eq!(number_static("isNaN"), Some(NumFn::IsNaN));
        assert_eq!(number_static("isSafeInteger"), Some(NumFn::IsSafeInteger));
        assert_eq!(number_static("parseInt"), Some(NumFn::ParseInt));
        assert_eq!(number_static("parseFloat"), Some(NumFn::ParseFloat));
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
        assert_eq!(str_method("slice"), Some(StrFn::Slice));
        assert_eq!(str_method("substring"), Some(StrFn::Substring));
        assert_eq!(str_method("substr"), Some(StrFn::Substr));
        assert_eq!(str_method("charAt"), Some(StrFn::CharAt));
        assert_eq!(str_method("codePointAt"), Some(StrFn::CodePointAt));
        assert_eq!(str_method("concat"), Some(StrFn::Concat));
        // Out-of-subset members resolve to nothing (Q21/Q27).
        assert_eq!(str_method("localeCompare"), None);
        assert_eq!(str_method("match"), None);
        assert_eq!(str_method("toLocaleUpperCase"), None);
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
        // Q27 stage 3 additions are checker-owned intrinsics.
        assert_eq!(arr_method("reduceRight"), Some(ArrFn::ReduceRight));
        assert_eq!(arr_method("splice"), Some(ArrFn::Splice));
        assert_eq!(arr_method("shift"), Some(ArrFn::Shift));
        assert_eq!(arr_method("unshift"), Some(ArrFn::Unshift));
        assert_eq!(arr_method("copyWithin"), Some(ArrFn::CopyWithin));
        // Out-of-subset members resolve to nothing (Q22/Q27).
        assert_eq!(arr_method("find"), None);
        assert_eq!(arr_method("flatMap"), None);
    }

    #[test]
    fn map_set_method_lookups_cover_q27_stage_four() {
        assert_eq!(map_method("get"), Some(MapFn::Get));
        assert_eq!(map_method("groupBy"), None);
        for f in [
            SetFn::Union,
            SetFn::Intersection,
            SetFn::Difference,
            SetFn::SymmetricDifference,
            SetFn::IsSubsetOf,
            SetFn::IsSupersetOf,
            SetFn::IsDisjointFrom,
        ] {
            assert_eq!(set_method(f.name()), Some(f));
        }
        assert_eq!(set_rejection("union"), None);
    }

    #[test]
    fn ambient_functions_have_expected_shapes() {
        assert_eq!(ambient_fn("print"), Some(AmbientFn::Print));
        assert_eq!(ambient_params(AmbientFn::Print), &[Type::Str]);
        assert!(ambient_params(AmbientFn::Collect).is_empty());
        assert_eq!(ambient_params(AmbientFn::UnsafeDelete), &[Type::Object]);
        assert_eq!(ambient_fn("eval"), None);
    }

    #[test]
    fn accepted_api_rows_are_exactly_the_checker_tables() {
        let rows = accepted_api();
        let has = |group: &str, signature: &str| {
            rows.iter()
                .any(|item| item.group == group && item.signature == signature)
        };

        for f in AmbientFn::ALL {
            assert!(has("Global", f.api_signature()), "ambient {}", f.name());
        }
        for c in MATH_CONSTS {
            assert!(has("Math", &format!("{}: f64", c.name)), "Math.{}", c.name);
        }
        for f in MathFn::ALL {
            assert!(has("Math", &f.api_signature()), "Math.{}", f.name());
        }
        for c in NUMBER_CONSTS {
            assert!(
                has("Number", &format!("{}: f64", c.name)),
                "Number.{}",
                c.name
            );
        }
        for f in [NumFn::ParseInt, NumFn::ParseFloat] {
            assert!(has("Global", f.api_signature()), "global {}", f.name());
        }
        for f in [
            NumFn::IsNaN,
            NumFn::IsFinite,
            NumFn::IsInteger,
            NumFn::IsSafeInteger,
            NumFn::ParseInt,
            NumFn::ParseFloat,
        ] {
            assert!(has("Number", f.api_signature()), "Number.{}", f.name());
        }
        for (group, f) in [
            ("f32", NumFn::ToFixed),
            ("f32", NumFn::ToStringF32),
            ("f32", NumFn::ToExponential),
            ("f32", NumFn::ToPrecision),
            ("f64", NumFn::ToFixed),
            ("f64", NumFn::ToStringF64),
            ("f64", NumFn::ToExponential),
            ("f64", NumFn::ToPrecision),
        ] {
            assert!(has(group, f.api_signature()), "{group}.{}", f.name());
        }
        for f in DateFn::ALL {
            let group = match f {
                DateFn::New => "Date constructor",
                DateFn::Utc | DateFn::Now => "Date",
                _ => "Date instance",
            };
            assert!(has(group, f.api_signature()), "Date.{}", f.name());
        }
        for f in StrFn::ALL {
            assert!(has("string", f.api_signature()), "string.{}", f.name());
        }
        for f in ArrFn::ALL {
            assert!(has("T[]", f.api_signature()), "T[].{}", f.name());
            if f.fixed_symbol().is_some() {
                assert!(
                    has("FixedArray<T, N>", f.api_signature()),
                    "FixedArray.{}",
                    f.name()
                );
            }
        }
        for f in MapFn::ALL {
            let group = match f {
                MapFn::New => "Map constructor",
                MapFn::GroupBy => "Map",
                _ => "Map<K, V>",
            };
            assert!(has(group, f.api_signature()), "Map.{}", f.name());
        }
        for f in SetFn::ALL {
            let group = if f == SetFn::New {
                "Set constructor"
            } else {
                "Set<K>"
            };
            assert!(has(group, f.api_signature()), "Set.{}", f.name());
        }
        for (group, signature) in [
            ("Global", "NaN: f64"),
            ("Date instance", "getTime(): i64"),
            ("string", "length: i32"),
            ("T[]", "length: i32"),
            ("T[]", "push(value: T): i32"),
            ("T[]", "pop(): T"),
            ("FixedArray<T, N>", "length: i32"),
            ("JSON", "stringify<T>(value: T): string"),
            ("JSON", "parse<T>(text: string): JsonResult<T>"),
            ("JsonResult<T>", "ok: boolean"),
            ("JsonResult<T>", "value: T"),
            ("Generator<T>", "next(): IteratorResult<T>"),
            ("IteratorResult<T>", "done: boolean"),
            ("IteratorResult<T>", "value: T"),
        ] {
            assert!(has(group, signature), "{group} {signature}");
        }

        #[cfg(feature = "regex")]
        let regex_rows = 1 + RegexFn::ALL.len();
        #[cfg(not(feature = "regex"))]
        let regex_rows = 0;
        let expected = AmbientFn::ALL.len()
            + 1
            + 2
            + MATH_CONSTS.len()
            + MathFn::ALL.len()
            + NUMBER_CONSTS.len()
            + 6
            + DateFn::ALL.len()
            + 1
            + 8
            + 1
            + StrFn::ALL.len()
            + 3
            + ArrFn::ALL.len()
            + 1
            + ArrFn::ALL
                .into_iter()
                .filter(|f| f.fixed_symbol().is_some())
                .count()
            + MapFn::ALL.len()
            + SetFn::ALL.len()
            + regex_rows
            + 7;
        assert_eq!(
            rows.len(),
            expected,
            "generated accepted rows and checker tables disagree"
        );
    }
}
