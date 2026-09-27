use super::*;

impl ContextBytesFn {
    /// Every typed Context storage-byte operation.
    pub const ALL: [ContextBytesFn; 3] = [
        ContextBytesFn::BytesOf,
        ContextBytesFn::BytesInto,
        ContextBytesFn::FromBytes,
    ];

    /// Source member name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ContextBytesFn::BytesOf => "bytesOf",
            ContextBytesFn::BytesInto => "bytesInto",
            ContextBytesFn::FromBytes => "fromBytes",
        }
    }

    /// Source-level subscript signature.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            ContextBytesFn::BytesOf => "bytesOf<T>(value: T): u8[]",
            ContextBytesFn::BytesInto => "bytesInto<T>(value: T, target: u8[], offset: u32): void",
            ContextBytesFn::FromBytes => "fromBytes<T>(bytes: u8[], offset: u32): T",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            ContextBytesFn::BytesOf => {
                "Returns a new byte array for eligible value storage and clears all padding bytes."
            }
            ContextBytesFn::BytesInto => {
                "Copies eligible value storage into a byte array and clears all padding bytes."
            }
            ContextBytesFn::FromBytes => {
                "Copies byte-array storage into an eligible value without initialization."
            }
        }
    }
}

impl AmbientFn {
    /// Every checker-owned ambient function.
    pub const ALL: [AmbientFn; 4] = [
        AmbientFn::Print,
        AmbientFn::Unreachable,
        AmbientFn::Collect,
        AmbientFn::UnsafeDelete,
    ];

    /// Source name without its optional namespace prefix.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            AmbientFn::Print => "print",
            AmbientFn::Unreachable => "unreachable",
            AmbientFn::Collect => "collect",
            AmbientFn::UnsafeDelete => "free",
        }
    }

    /// Whether the runtime call can leave the Context trapped.
    #[must_use]
    pub fn can_trap(self) -> bool {
        matches!(self, AmbientFn::Unreachable | AmbientFn::UnsafeDelete)
    }

    /// Source-level subscript signature.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            AmbientFn::Print => "print(message: string): void",
            AmbientFn::Unreachable => "unreachable(): never",
            AmbientFn::Collect => "collect(): void",
            AmbientFn::UnsafeDelete => "free(value: object): void",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            AmbientFn::Print => "Writes one line to the Context output sink.",
            AmbientFn::Unreachable => {
                "Marks a call-statement path as diverging and traps if execution reaches it."
            }
            AmbientFn::Collect => "Explicitly collects unreachable Context allocations.",
            AmbientFn::UnsafeDelete => "Immediately releases a reference-class allocation.",
        }
    }
}

impl MathFn {
    /// Every accepted `Math` function, in declaration order; the index
    /// of each variant equals its discriminant, so `f as usize` indexes
    /// tables built from this list.
    pub const ALL: [MathFn; 37] = [
        MathFn::Abs,
        MathFn::Acos,
        MathFn::Acosh,
        MathFn::Asin,
        MathFn::Asinh,
        MathFn::Atan,
        MathFn::Atanh,
        MathFn::Cbrt,
        MathFn::Ceil,
        MathFn::Cos,
        MathFn::Cosh,
        MathFn::Exp,
        MathFn::Expm1,
        MathFn::Floor,
        MathFn::Log,
        MathFn::Log1p,
        MathFn::Log10,
        MathFn::Log2,
        MathFn::Round,
        MathFn::Sign,
        MathFn::Sin,
        MathFn::Sinh,
        MathFn::Sqrt,
        MathFn::Tan,
        MathFn::Tanh,
        MathFn::Trunc,
        MathFn::Atan2,
        MathFn::Hypot,
        MathFn::Pow,
        MathFn::Max,
        MathFn::Min,
        MathFn::Random,
        MathFn::Clz32,
        MathFn::Imul,
        MathFn::Fround,
        MathFn::F32ToBits,
        MathFn::F32FromBits,
    ];

    /// Whether the runtime call can leave the Context trapped.
    #[must_use]
    pub fn can_trap(self) -> bool {
        false
    }

    /// Returns the source member name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            MathFn::Abs => "abs",
            MathFn::Acos => "acos",
            MathFn::Acosh => "acosh",
            MathFn::Asin => "asin",
            MathFn::Asinh => "asinh",
            MathFn::Atan => "atan",
            MathFn::Atanh => "atanh",
            MathFn::Cbrt => "cbrt",
            MathFn::Ceil => "ceil",
            MathFn::Cos => "cos",
            MathFn::Cosh => "cosh",
            MathFn::Exp => "exp",
            MathFn::Expm1 => "expm1",
            MathFn::Floor => "floor",
            MathFn::Log => "log",
            MathFn::Log1p => "log1p",
            MathFn::Log10 => "log10",
            MathFn::Log2 => "log2",
            MathFn::Round => "round",
            MathFn::Sign => "sign",
            MathFn::Sin => "sin",
            MathFn::Sinh => "sinh",
            MathFn::Sqrt => "sqrt",
            MathFn::Tan => "tan",
            MathFn::Tanh => "tanh",
            MathFn::Trunc => "trunc",
            MathFn::Atan2 => "atan2",
            MathFn::Hypot => "hypot",
            MathFn::Pow => "pow",
            MathFn::Max => "max",
            MathFn::Min => "min",
            MathFn::Random => "random",
            MathFn::Clz32 => "clz32",
            MathFn::Imul => "imul",
            MathFn::Fround => "fround",
            MathFn::F32ToBits => "f32ToBits",
            MathFn::F32FromBits => "f32FromBits",
        }
    }

    /// Returns the opaque runtime symbol shared by both tiers.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            MathFn::Abs => "subscript_rt_math_abs",
            MathFn::Acos => "subscript_rt_math_acos",
            MathFn::Acosh => "subscript_rt_math_acosh",
            MathFn::Asin => "subscript_rt_math_asin",
            MathFn::Asinh => "subscript_rt_math_asinh",
            MathFn::Atan => "subscript_rt_math_atan",
            MathFn::Atanh => "subscript_rt_math_atanh",
            MathFn::Cbrt => "subscript_rt_math_cbrt",
            MathFn::Ceil => "subscript_rt_math_ceil",
            MathFn::Cos => "subscript_rt_math_cos",
            MathFn::Cosh => "subscript_rt_math_cosh",
            MathFn::Exp => "subscript_rt_math_exp",
            MathFn::Expm1 => "subscript_rt_math_expm1",
            MathFn::Floor => "subscript_rt_math_floor",
            MathFn::Log => "subscript_rt_math_log",
            MathFn::Log1p => "subscript_rt_math_log1p",
            MathFn::Log10 => "subscript_rt_math_log10",
            MathFn::Log2 => "subscript_rt_math_log2",
            MathFn::Round => "subscript_rt_math_round",
            MathFn::Sign => "subscript_rt_math_sign",
            MathFn::Sin => "subscript_rt_math_sin",
            MathFn::Sinh => "subscript_rt_math_sinh",
            MathFn::Sqrt => "subscript_rt_math_sqrt",
            MathFn::Tan => "subscript_rt_math_tan",
            MathFn::Tanh => "subscript_rt_math_tanh",
            MathFn::Trunc => "subscript_rt_math_trunc",
            MathFn::Atan2 => "subscript_rt_math_atan2",
            MathFn::Hypot => "subscript_rt_math_hypot",
            MathFn::Pow => "subscript_rt_math_pow",
            MathFn::Max => "subscript_rt_math_max",
            MathFn::Min => "subscript_rt_math_min",
            MathFn::Random => "subscript_rt_math_random",
            MathFn::Clz32 => "subscript_rt_math_clz32",
            MathFn::Imul => "subscript_rt_math_imul",
            MathFn::Fround => "subscript_rt_math_fround",
            MathFn::F32ToBits => "subscript_rt_math_f32_to_bits",
            MathFn::F32FromBits => "subscript_rt_math_f32_from_bits",
        }
    }

    /// Number of arguments (exact; the lib's variadic forms are out of
    /// subset, Q19).
    #[must_use]
    pub fn arity(self) -> usize {
        match self {
            MathFn::Random => 0,
            MathFn::Atan2
            | MathFn::Hypot
            | MathFn::Pow
            | MathFn::Max
            | MathFn::Min
            | MathFn::Imul => 2,
            _ => 1,
        }
    }

    /// Source-level subscript signature.
    #[must_use]
    pub(crate) fn api_signature(self) -> String {
        if self == MathFn::Random {
            return "random(): f64".to_string();
        }
        if self == MathFn::Clz32 {
            return "clz32(value: u32): i32".to_string();
        }
        if self == MathFn::Imul {
            return "imul(left: i32, right: i32): i32".to_string();
        }
        if self == MathFn::Fround {
            return "fround(value: f64): f64".to_string();
        }
        if self == MathFn::F32ToBits {
            return "f32ToBits(value: f64): u32".to_string();
        }
        if self == MathFn::F32FromBits {
            return "f32FromBits(bits: u32): f64".to_string();
        }
        let params = match self.arity() {
            1 => "value: f64",
            2 => "left: f64, right: f64",
            _ => "",
        };
        format!("{}({params}): f64", self.name())
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            MathFn::Random => "Draws from the deterministic, Context-owned PRNG.",
            MathFn::Clz32 => "Counts leading zero bits in a `u32`; zero returns 32.",
            MathFn::Imul => "Multiplies two `i32` values with 32-bit wrapping.",
            MathFn::Fround => "Rounds an `f64` through `f32` precision.",
            MathFn::F32ToBits => "Returns the canonical binary32 bit pattern of an `f64` value.",
            MathFn::F32FromBits => "Widens a binary32 bit pattern exactly to `f64`.",
            MathFn::Hypot | MathFn::Max | MathFn::Min => {
                "Accepts exactly two operands; the ES variadic overload is rejected."
            }
            _ => "Uses the accepted `f64` Math intrinsic semantics.",
        }
    }
}

impl NumFn {
    /// Every Q25/Q26 runtime operation in discriminant order.
    pub const ALL: [NumFn; 11] = [
        NumFn::IsNaN,
        NumFn::IsFinite,
        NumFn::IsInteger,
        NumFn::IsSafeInteger,
        NumFn::ParseInt,
        NumFn::ParseFloat,
        NumFn::ToFixed,
        NumFn::ToStringF32,
        NumFn::ToStringF64,
        NumFn::ToExponential,
        NumFn::ToPrecision,
    ];

    /// Surface member/global name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            NumFn::IsNaN => "isNaN",
            NumFn::IsFinite => "isFinite",
            NumFn::IsInteger => "isInteger",
            NumFn::IsSafeInteger => "isSafeInteger",
            NumFn::ParseInt => "parseInt",
            NumFn::ParseFloat => "parseFloat",
            NumFn::ToFixed => "toFixed",
            NumFn::ToStringF32 | NumFn::ToStringF64 => "toString",
            NumFn::ToExponential => "toExponential",
            NumFn::ToPrecision => "toPrecision",
        }
    }

    /// Opaque runtime symbol shared by both tiers.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            NumFn::IsNaN => "subscript_rt_num_is_nan",
            NumFn::IsFinite => "subscript_rt_num_is_finite",
            NumFn::IsInteger => "subscript_rt_num_is_integer",
            NumFn::IsSafeInteger => "subscript_rt_num_is_safe_integer",
            NumFn::ParseInt => "subscript_rt_num_parse_int",
            NumFn::ParseFloat => "subscript_rt_num_parse_float",
            NumFn::ToFixed => "subscript_rt_num_to_fixed",
            NumFn::ToStringF32 => "subscript_rt_num_to_string_f32",
            NumFn::ToStringF64 => "subscript_rt_num_to_string_f64",
            NumFn::ToExponential => "subscript_rt_num_to_exponential",
            NumFn::ToPrecision => "subscript_rt_num_to_precision",
        }
    }

    /// Whether the runtime signature carries a trailing `pos_id` and
    /// may trap.
    #[must_use]
    pub fn takes_pos_id(self) -> bool {
        matches!(
            self,
            NumFn::ParseInt
                | NumFn::ParseFloat
                | NumFn::ToFixed
                | NumFn::ToStringF32
                | NumFn::ToStringF64
                | NumFn::ToExponential
                | NumFn::ToPrecision
        )
    }

    /// Whether the result is an `i32` boolean representation.
    #[must_use]
    pub fn returns_bool(self) -> bool {
        matches!(
            self,
            NumFn::IsNaN | NumFn::IsFinite | NumFn::IsInteger | NumFn::IsSafeInteger
        )
    }

    /// Source-level subscript signature for this surface operation.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            NumFn::IsNaN => "isNaN(value: f64): boolean",
            NumFn::IsFinite => "isFinite(value: f64): boolean",
            NumFn::IsInteger => "isInteger(value: f64): boolean",
            NumFn::IsSafeInteger => "isSafeInteger(value: f64): boolean",
            NumFn::ParseInt => "parseInt(value: string, radix: i32): f64",
            NumFn::ParseFloat => "parseFloat(value: string): f64",
            NumFn::ToFixed => "toFixed(digits?: i32): string",
            NumFn::ToStringF32 | NumFn::ToStringF64 => "toString(radix: i32): string",
            NumFn::ToExponential => "toExponential(digits?: i32): string",
            NumFn::ToPrecision => "toPrecision(digits: i32): string",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            NumFn::IsNaN => "Tests for NaN without coercion.",
            NumFn::IsFinite => "Tests finiteness without coercion.",
            NumFn::IsInteger => "Tests whether an `f64` has an integral value.",
            NumFn::IsSafeInteger => "Tests the ECMA safe-integer range.",
            NumFn::ParseInt => "Parses the longest integer prefix; the radix is required.",
            NumFn::ParseFloat => "Parses the longest decimal floating-point prefix.",
            NumFn::ToFixed => "Formats with a fixed-decimal digit count, defaulting to zero.",
            NumFn::ToStringF32 | NumFn::ToStringF64 => {
                "Formats in an explicit radix from 2 through 36."
            }
            NumFn::ToExponential => "Formats in exponential notation.",
            NumFn::ToPrecision => "Formats with a required significant-digit count.",
        }
    }
}

impl JsonFn {
    /// Every internal JSON runtime leaf in discriminant order.
    pub const ALL: [JsonFn; 29] = [
        JsonFn::Begin,
        JsonFn::BeginTracked,
        JsonFn::Finish,
        JsonFn::Raw,
        JsonFn::Str,
        JsonFn::I32,
        JsonFn::U32,
        JsonFn::I64,
        JsonFn::U64,
        JsonFn::F32,
        JsonFn::F64,
        JsonFn::Bool,
        JsonFn::Date,
        JsonFn::Null,
        JsonFn::Visit,
        JsonFn::Leave,
        JsonFn::ParseBegin,
        JsonFn::ParseEnd,
        JsonFn::ParseRoot,
        JsonFn::ParseIsKind,
        JsonFn::ParseNumberFits,
        JsonFn::ParseNumber,
        JsonFn::ParseInteger,
        JsonFn::ParseBool,
        JsonFn::ParseString,
        JsonFn::ParseArrayLen,
        JsonFn::ParseArrayGet,
        JsonFn::ParseObjectGet,
        JsonFn::ParseFailure,
    ];

    /// Whether the runtime call can leave the Context trapped.
    ///
    /// Every JSON leaf currently carries a source position and may
    /// allocate or report a data-dependent JSON fault.
    #[must_use]
    pub fn can_trap(self) -> bool {
        true
    }

    /// Opaque runtime symbol shared by dev-JIT and ship-C-AOT.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            JsonFn::Begin => "subscript_rt_json_begin",
            JsonFn::BeginTracked => "subscript_rt_json_begin_tracked",
            JsonFn::Finish => "subscript_rt_json_finish",
            JsonFn::Raw => "subscript_rt_json_raw",
            JsonFn::Str => "subscript_rt_json_str",
            JsonFn::I32 => "subscript_rt_json_i32",
            JsonFn::U32 => "subscript_rt_json_u32",
            JsonFn::I64 => "subscript_rt_json_i64",
            JsonFn::U64 => "subscript_rt_json_u64",
            JsonFn::F32 => "subscript_rt_json_f32",
            JsonFn::F64 => "subscript_rt_json_f64",
            JsonFn::Bool => "subscript_rt_json_bool",
            JsonFn::Date => "subscript_rt_json_date",
            JsonFn::Null => "subscript_rt_json_null",
            JsonFn::Visit => "subscript_rt_json_visit",
            JsonFn::Leave => "subscript_rt_json_leave",
            JsonFn::ParseBegin => "subscript_rt_json_parse_begin",
            JsonFn::ParseEnd => "subscript_rt_json_parse_end",
            JsonFn::ParseRoot => "subscript_rt_json_parse_root",
            JsonFn::ParseIsKind => "subscript_rt_json_parse_is_kind",
            JsonFn::ParseNumberFits => "subscript_rt_json_parse_number_fits",
            JsonFn::ParseNumber => "subscript_rt_json_parse_number",
            JsonFn::ParseInteger => "subscript_rt_json_parse_integer",
            JsonFn::ParseBool => "subscript_rt_json_parse_bool",
            JsonFn::ParseString => "subscript_rt_json_parse_string",
            JsonFn::ParseArrayLen => "subscript_rt_json_parse_array_len",
            JsonFn::ParseArrayGet => "subscript_rt_json_parse_array_get",
            JsonFn::ParseObjectGet => "subscript_rt_json_parse_object_get",
            JsonFn::ParseFailure => "subscript_rt_json_parse_failure",
        }
    }

    /// Whether the runtime result is the language boolean representation.
    #[must_use]
    pub fn returns_bool(self) -> bool {
        matches!(
            self,
            JsonFn::Visit | JsonFn::ParseIsKind | JsonFn::ParseNumberFits | JsonFn::ParseBool
        )
    }
}

impl DateFn {
    /// Every accepted Date operation in discriminant order.
    pub const ALL: [DateFn; 13] = [
        DateFn::New,
        DateFn::Utc,
        DateFn::Now,
        DateFn::GetUtcFullYear,
        DateFn::GetUtcMonth,
        DateFn::GetUtcDate,
        DateFn::GetUtcDay,
        DateFn::GetUtcHours,
        DateFn::GetUtcMinutes,
        DateFn::GetUtcSeconds,
        DateFn::GetUtcMilliseconds,
        DateFn::ToIso,
        DateFn::ToUtcString,
    ];

    /// Whether the runtime call can leave the Context trapped.
    #[must_use]
    pub fn can_trap(self) -> bool {
        matches!(
            self,
            DateFn::New | DateFn::Utc | DateFn::ToIso | DateFn::ToUtcString
        )
    }

    /// The lib member name (diagnostics and the checker's lookup).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            DateFn::New => "Date",
            DateFn::Utc => "UTC",
            DateFn::Now => "now",
            DateFn::GetUtcFullYear => "getUTCFullYear",
            DateFn::GetUtcMonth => "getUTCMonth",
            DateFn::GetUtcDate => "getUTCDate",
            DateFn::GetUtcDay => "getUTCDay",
            DateFn::GetUtcHours => "getUTCHours",
            DateFn::GetUtcMinutes => "getUTCMinutes",
            DateFn::GetUtcSeconds => "getUTCSeconds",
            DateFn::GetUtcMilliseconds => "getUTCMilliseconds",
            DateFn::ToIso => "toISOString",
            DateFn::ToUtcString => "toUTCString",
        }
    }

    /// The `subscript_rt_date_get` field code of a UTC accessor (`None` for
    /// the non-accessor operations). The codes are an ABI contract with
    /// the runtime's `date` module; a codegen test asserts the two
    /// tables agree.
    #[must_use]
    pub fn field_code(self) -> Option<u32> {
        Some(match self {
            DateFn::GetUtcFullYear => 0,
            DateFn::GetUtcMonth => 1,
            DateFn::GetUtcDate => 2,
            DateFn::GetUtcDay => 3,
            DateFn::GetUtcHours => 4,
            DateFn::GetUtcMinutes => 5,
            DateFn::GetUtcSeconds => 6,
            DateFn::GetUtcMilliseconds => 7,
            _ => return None,
        })
    }

    /// Source-level subscript signature.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            DateFn::New => "new Date(milliseconds: i64): Date",
            DateFn::Utc => {
                "UTC(year: i32, month?: i32, date?: i32, hours?: i32, minutes?: i32, seconds?: i32, milliseconds?: i32): i64"
            }
            DateFn::Now => "now(): i64",
            DateFn::GetUtcFullYear => "getUTCFullYear(): i32",
            DateFn::GetUtcMonth => "getUTCMonth(): i32",
            DateFn::GetUtcDate => "getUTCDate(): i32",
            DateFn::GetUtcDay => "getUTCDay(): i32",
            DateFn::GetUtcHours => "getUTCHours(): i32",
            DateFn::GetUtcMinutes => "getUTCMinutes(): i32",
            DateFn::GetUtcSeconds => "getUTCSeconds(): i32",
            DateFn::GetUtcMilliseconds => "getUTCMilliseconds(): i32",
            DateFn::ToIso => "toISOString(): string",
            DateFn::ToUtcString => "toUTCString(): string",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            DateFn::New => "Constructs an immutable Date from epoch milliseconds.",
            DateFn::Utc => "Builds epoch milliseconds from UTC components.",
            DateFn::Now => "Reads the Context clock.",
            DateFn::GetUtcFullYear => "Returns the UTC year.",
            DateFn::GetUtcMonth => "Returns the zero-based UTC month.",
            DateFn::GetUtcDate => "Returns the UTC day of the month.",
            DateFn::GetUtcDay => "Returns the UTC weekday, Sunday = 0.",
            DateFn::GetUtcHours => "Returns the UTC hour.",
            DateFn::GetUtcMinutes => "Returns the UTC minute.",
            DateFn::GetUtcSeconds => "Returns the UTC second.",
            DateFn::GetUtcMilliseconds => "Returns the UTC millisecond.",
            DateFn::ToIso => "Formats years 0000 through 9999 as UTC ISO text.",
            DateFn::ToUtcString => "Formats every TimeClip year as UTC text with weekday and GMT.",
        }
    }
}

impl RegexFn {
    /// Every regex intrinsic in discriminant order.
    pub const ALL: [RegexFn; 18] = [
        RegexFn::New,
        RegexFn::Test,
        RegexFn::Source,
        RegexFn::Flags,
        RegexFn::Search,
        RegexFn::Replace,
        RegexFn::ReplaceAll,
        RegexFn::Split,
        RegexFn::MatchStart,
        RegexFn::MatchEnd,
        RegexFn::Global,
        RegexFn::IgnoreCase,
        RegexFn::Multiline,
        RegexFn::DotAll,
        RegexFn::Unicode,
        RegexFn::HasIndices,
        RegexFn::Sticky,
        RegexFn::ToString,
    ];

    /// Opaque runtime symbol used by both execution tiers.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            RegexFn::New => "subscript_rt_regex_new",
            RegexFn::Test => "subscript_rt_regex_test",
            RegexFn::Source => "subscript_rt_regex_source",
            RegexFn::Flags => "subscript_rt_regex_flags",
            RegexFn::Global => "subscript_rt_regex_global",
            RegexFn::IgnoreCase => "subscript_rt_regex_ignore_case",
            RegexFn::Multiline => "subscript_rt_regex_multiline",
            RegexFn::DotAll => "subscript_rt_regex_dot_all",
            RegexFn::Unicode => "subscript_rt_regex_unicode",
            RegexFn::HasIndices => "subscript_rt_regex_has_indices",
            RegexFn::Sticky => "subscript_rt_regex_sticky",
            RegexFn::ToString => "subscript_rt_regex_to_string",
            RegexFn::Search => "subscript_rt_regex_search",
            RegexFn::Replace => "subscript_rt_regex_replace",
            RegexFn::ReplaceAll => "subscript_rt_regex_replace_all",
            RegexFn::Split => "subscript_rt_regex_split",
            RegexFn::MatchStart => "subscript_rt_regex_match_start",
            RegexFn::MatchEnd => "subscript_rt_regex_match_end",
        }
    }

    /// Whether the runtime operation can leave the Context trapped.
    #[must_use]
    pub fn can_trap(self) -> bool {
        matches!(
            self,
            RegexFn::New
                | RegexFn::Test
                | RegexFn::Source
                | RegexFn::Flags
                | RegexFn::Global
                | RegexFn::IgnoreCase
                | RegexFn::Multiline
                | RegexFn::DotAll
                | RegexFn::Unicode
                | RegexFn::HasIndices
                | RegexFn::Sticky
                | RegexFn::ToString
                | RegexFn::Search
                | RegexFn::Replace
                | RegexFn::ReplaceAll
                | RegexFn::Split
                | RegexFn::MatchStart
                | RegexFn::MatchEnd
        )
    }

    /// Source-level signature rendered in the generated API reference.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            RegexFn::New => "new RegExp(pattern: string, flags?: string): RegExp",
            RegexFn::Test => "test(subject: string): boolean",
            RegexFn::Source => "source: string",
            RegexFn::Flags => "flags: string",
            RegexFn::Global => "global: boolean",
            RegexFn::IgnoreCase => "ignoreCase: boolean",
            RegexFn::Multiline => "multiline: boolean",
            RegexFn::DotAll => "dotAll: boolean",
            RegexFn::Unicode => "unicode: boolean",
            RegexFn::HasIndices => "hasIndices: boolean",
            RegexFn::Sticky => "sticky: boolean",
            RegexFn::ToString => "toString(): string",
            RegexFn::Search => "string.search(pattern: RegExp): i32",
            RegexFn::Replace => "string.replace(pattern: RegExp, replacement: string): string",
            RegexFn::ReplaceAll => {
                "string.replaceAll(pattern: RegExp, replacement: string): string"
            }
            RegexFn::Split => "string.split(separator: RegExp, limit?: i32): string[]",
            RegexFn::MatchStart => "matchStart(group: i32): i32",
            RegexFn::MatchEnd => "matchEnd(group: i32): i32",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            RegexFn::New => "Compiles or reuses a Context-cached ECMAScript pattern.",
            RegexFn::Test => "Tests for a budgeted match and records its capture extents.",
            RegexFn::Source => "Returns the constructor pattern text.",
            RegexFn::Flags => "Returns flags in canonical `dgimsuv` order.",
            RegexFn::Global => "Returns whether the flags contain `g`.",
            RegexFn::IgnoreCase => "Returns whether the flags contain `i`.",
            RegexFn::Multiline => "Returns whether the flags contain `m`.",
            RegexFn::DotAll => "Returns whether the flags contain `s`.",
            RegexFn::Unicode => "Returns whether the flags contain `u`.",
            RegexFn::HasIndices => "Returns whether the flags contain `d`.",
            RegexFn::Sticky => "Returns false; sticky matching requires lastIndex.",
            RegexFn::ToString => "Returns the source and flags between slash delimiters.",
            RegexFn::Search => "Returns the first UTF-8 byte offset, or -1.",
            RegexFn::Replace => "Replaces the first match with ECMA `$` substitutions.",
            RegexFn::ReplaceAll => {
                "Replaces every match with ECMA `$` substitutions; the RegExp must be global."
            }
            RegexFn::Split => "Splits with capture reinjection.",
            RegexFn::MatchStart => "Returns a recorded capture's start byte offset, or -1.",
            RegexFn::MatchEnd => "Returns a recorded capture's end byte offset, or -1.",
        }
    }
}

impl StrFn {
    /// Every accepted `String` method, in declaration order; the index
    /// of each variant equals its discriminant, so `f as usize` indexes
    /// tables built from this list.
    pub const ALL: [StrFn; 24] = [
        StrFn::Slice,
        StrFn::IndexOf,
        StrFn::LastIndexOf,
        StrFn::Includes,
        StrFn::StartsWith,
        StrFn::EndsWith,
        StrFn::CharCodeAt,
        StrFn::Split,
        StrFn::Trim,
        StrFn::TrimStart,
        StrFn::TrimEnd,
        StrFn::Repeat,
        StrFn::PadStart,
        StrFn::PadEnd,
        StrFn::ToUpperCase,
        StrFn::ToLowerCase,
        StrFn::Replace,
        StrFn::ReplaceAll,
        StrFn::Substring,
        StrFn::Substr,
        StrFn::CharAt,
        StrFn::CodePointAt,
        StrFn::Concat,
        StrFn::At,
    ];

    /// The lib member name (the checker's lookup and diagnostics).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            StrFn::Slice => "slice",
            StrFn::IndexOf => "indexOf",
            StrFn::LastIndexOf => "lastIndexOf",
            StrFn::Includes => "includes",
            StrFn::StartsWith => "startsWith",
            StrFn::EndsWith => "endsWith",
            StrFn::CharCodeAt => "charCodeAt",
            StrFn::Split => "split",
            StrFn::Trim => "trim",
            StrFn::TrimStart => "trimStart",
            StrFn::TrimEnd => "trimEnd",
            StrFn::Repeat => "repeat",
            StrFn::PadStart => "padStart",
            StrFn::PadEnd => "padEnd",
            StrFn::ToUpperCase => "toUpperCase",
            StrFn::ToLowerCase => "toLowerCase",
            StrFn::Replace => "replace",
            StrFn::ReplaceAll => "replaceAll",
            StrFn::Substring => "substring",
            StrFn::Substr => "substr",
            StrFn::CharAt => "charAt",
            StrFn::CodePointAt => "codePointAt",
            StrFn::At => "at",
            StrFn::Concat => "concat",
        }
    }

    /// The opaque runtime symbol both tiers call.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            StrFn::Slice => "subscript_rt_str_slice",
            StrFn::IndexOf => "subscript_rt_str_index_of",
            StrFn::LastIndexOf => "subscript_rt_str_last_index_of",
            StrFn::Includes => "subscript_rt_str_includes",
            StrFn::StartsWith => "subscript_rt_str_starts_with",
            StrFn::EndsWith => "subscript_rt_str_ends_with",
            StrFn::CharCodeAt => "subscript_rt_str_char_code_at",
            StrFn::Split => "subscript_rt_str_split",
            StrFn::Trim => "subscript_rt_str_trim",
            StrFn::TrimStart => "subscript_rt_str_trim_start",
            StrFn::TrimEnd => "subscript_rt_str_trim_end",
            StrFn::Repeat => "subscript_rt_str_repeat",
            StrFn::PadStart => "subscript_rt_str_pad_start",
            StrFn::PadEnd => "subscript_rt_str_pad_end",
            StrFn::ToUpperCase => "subscript_rt_str_to_upper",
            StrFn::ToLowerCase => "subscript_rt_str_to_lower",
            StrFn::Replace => "subscript_rt_str_replace",
            StrFn::ReplaceAll => "subscript_rt_str_replace_all",
            StrFn::Substring => "subscript_rt_str_substring",
            StrFn::Substr => "subscript_rt_str_substr",
            StrFn::CharAt => "subscript_rt_str_char_at",
            StrFn::CodePointAt => "subscript_rt_str_code_point_at",
            StrFn::At => "subscript_rt_str_at",
            StrFn::Concat => "subscript_rt_str_concat",
        }
    }

    /// Parameter spellings after the receiver, post-normalization (the
    /// checker has already supplied the defaulted `from`/`pad`).
    #[must_use]
    pub fn params(self) -> &'static [StrParam] {
        match self {
            StrFn::Slice | StrFn::Substring | StrFn::Substr => &[StrParam::I32, StrParam::I32],
            StrFn::IndexOf
            | StrFn::LastIndexOf
            | StrFn::Split
            | StrFn::Includes
            | StrFn::StartsWith
            | StrFn::EndsWith => &[StrParam::Str, StrParam::I32],
            StrFn::Concat => &[StrParam::Str],
            StrFn::At | StrFn::CharCodeAt | StrFn::Repeat | StrFn::CharAt | StrFn::CodePointAt => {
                &[StrParam::I32]
            }
            StrFn::Trim
            | StrFn::TrimStart
            | StrFn::TrimEnd
            | StrFn::ToUpperCase
            | StrFn::ToLowerCase => &[],
            StrFn::PadStart | StrFn::PadEnd => &[StrParam::I32, StrParam::Str],
            StrFn::Replace | StrFn::ReplaceAll => &[StrParam::Str, StrParam::Str],
        }
    }

    /// Result spelling.
    #[must_use]
    pub fn ret(self) -> StrRet {
        match self {
            StrFn::IndexOf | StrFn::LastIndexOf | StrFn::CharCodeAt | StrFn::CodePointAt => {
                StrRet::I32
            }
            StrFn::Includes | StrFn::StartsWith | StrFn::EndsWith => StrRet::Bool,
            StrFn::Split => StrRet::StrArray,
            _ => StrRet::Str,
        }
    }

    /// Whether the runtime symbol takes a trailing `pos_id`: true for
    /// every operation that can trap — a Q21 range/argument fault or a
    /// Context allocation (every string/array-returning method
    /// allocates). The pure search predicates take none.
    #[must_use]
    pub fn takes_pos_id(self) -> bool {
        !matches!(
            self,
            StrFn::IndexOf
                | StrFn::LastIndexOf
                | StrFn::Includes
                | StrFn::StartsWith
                | StrFn::EndsWith
        )
    }

    /// Source-level subscript signature, before checker default normalization.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            StrFn::Slice => "slice(start?: i32, end?: i32): string",
            StrFn::IndexOf => "indexOf(needle: string, from?: i32): i32",
            StrFn::LastIndexOf => "lastIndexOf(needle: string, position?: i32): i32",
            StrFn::Includes => "includes(needle: string, from?: i32): boolean",
            StrFn::StartsWith => "startsWith(needle: string, position?: i32): boolean",
            StrFn::EndsWith => "endsWith(needle: string, endPosition?: i32): boolean",
            StrFn::CharCodeAt => "charCodeAt(index: i32): i32",
            StrFn::Split => "split(separator: string, limit?: i32): string[]",
            StrFn::Trim => "trim(): string",
            StrFn::TrimStart => "trimStart(): string",
            StrFn::TrimEnd => "trimEnd(): string",
            StrFn::Repeat => "repeat(count: i32): string",
            StrFn::PadStart => "padStart(length: i32, pad?: string): string",
            StrFn::PadEnd => "padEnd(length: i32, pad?: string): string",
            StrFn::ToUpperCase => "toUpperCase(): string",
            StrFn::ToLowerCase => "toLowerCase(): string",
            StrFn::Replace => "replace(pattern: string, replacement: string): string",
            StrFn::ReplaceAll => "replaceAll(pattern: string, replacement: string): string",
            StrFn::Substring => "substring(start: i32, end?: i32): string",
            StrFn::Substr => "substr(start: i32, length?: i32): string",
            StrFn::CharAt => "charAt(index: i32): string",
            StrFn::CodePointAt => "codePointAt(index: i32): i32",
            StrFn::At => "at(index: i32): string",
            StrFn::Concat => "concat(other: string): string",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            StrFn::Slice => {
                "Returns a fresh UTF-8 byte range using JS clamp and negative-index rules."
            }
            StrFn::IndexOf => "Returns the first matching byte index, or -1.",
            StrFn::LastIndexOf => "Returns the last matching byte index, or -1.",
            StrFn::Includes => "Tests for a substring from an optional byte index.",
            StrFn::StartsWith => "Tests for a prefix at an optional byte position.",
            StrFn::EndsWith => "Tests for a suffix ending at an optional byte position.",
            StrFn::CharCodeAt => "Returns one UTF-8 byte value; out of range traps.",
            StrFn::Split => "Splits on a literal separator; an empty separator splits UTF-8 code points.",
            StrFn::Trim => "Removes ECMA whitespace from both ends.",
            StrFn::TrimStart => "Removes ECMA whitespace from the start.",
            StrFn::TrimEnd => "Removes ECMA whitespace from the end.",
            StrFn::Repeat => "Repeats the UTF-8 byte string.",
            StrFn::PadStart => "Pads to a byte length on the left; an empty pad returns unchanged bytes.",
            StrFn::PadEnd => "Pads to a byte length on the right; an empty pad returns unchanged bytes.",
            StrFn::ToUpperCase => "Applies Unicode Default Case Conversion.",
            StrFn::ToLowerCase => "Applies Unicode Default Case Conversion.",
            StrFn::Replace => "Replaces the first literal match with ECMA `$` substitutions.",
            StrFn::ReplaceAll => {
                "Replaces every literal match with ECMA `$` substitutions; an empty pattern matches each code-point boundary."
            }
            StrFn::Substring => "Slices by clamped UTF-8 byte offsets, swapping a reversed pair.",
            StrFn::Substr => {
                "Slices by UTF-8 byte start and length; a negative start counts from the end."
            }
            StrFn::CharAt => {
                "Returns the code point starting at a UTF-8 byte index, or an empty string."
            }
            StrFn::CodePointAt => {
                "Returns the code point starting at a UTF-8 byte index; out of range traps."
            }
            StrFn::At => "Returns a code point at a signed byte index; invalid indices trap.",
            StrFn::Concat => "Returns a fresh concatenation with exactly one other string.",
        }
    }
}

impl WorkerFn {
    /// Worker operations in stable intrinsic-number order.
    pub const ALL: [Self; 8] = [
        Self::Spawn(0),
        Self::Post,
        Self::Poll,
        Self::Close,
        Self::Join,
        Self::InboxWait,
        Self::InboxPoll,
        Self::OutboxPost,
    ];

    /// Removes the worker-entry payload from the operation identity.
    #[must_use]
    pub fn intrinsic_identity(self) -> Self {
        match self {
            Self::Spawn(_) => Self::Spawn(0),
            other => other,
        }
    }
}
