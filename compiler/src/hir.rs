//! Typed high-level IR produced by a successful check.
//!
//! Every expression node carries its resolved [`Type`] and a TS [`Pos`].
//! Generic declarations are monomorphized here: the module contains one
//! function/class per instantiation (e.g. `identity<i32>`), never a
//! generic template.

use crate::diag::Pos;
use crate::types::{ClassId, EnumId, Type};

/// A checked program: all source files merged into one module.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Module {
    /// Class definitions (value and reference), indexed by [`ClassId`].
    pub classes: Vec<ClassDef>,
    /// Enum definitions, indexed by [`EnumId`].
    pub enums: Vec<EnumDef>,
    /// Module-level variables.
    pub globals: Vec<Global>,
    /// Free functions, including monomorphized generic instances.
    /// Constructors and methods live on their [`ClassDef`].
    pub functions: Vec<Function>,
    /// Foreign (C-ABI) functions declared by an ingested ambient mirror
    /// (`declare function` in a `.d.ts`, P5.2). They carry a signature
    /// but no body; lowering a call to one is P5.2b, not P5.2a.
    pub foreign_fns: Vec<ForeignFn>,
    /// Checked top-level non-declaration statements, in source order
    /// (the accept corpus has none; kept for completeness).
    pub top_level: Vec<Stmt>,
}

/// A foreign function declared by an ambient C-header mirror
/// (`declare function`, P5.2). It is neither a script [`Function`] nor a
/// hardcoded [`AmbientFn`]: it names a C-ABI callee resolved at link
/// time, with a mapped boundary signature and no in-language body.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ForeignFn {
    /// C symbol name (also the in-language call name).
    pub name: String,
    /// Parameters, in order, with their mapped boundary types.
    pub params: Vec<Param>,
    /// Return type (a mapped boundary type or `void`).
    pub ret: Type,
    /// Position of the `declare function` in the mirror.
    pub pos: Pos,
}

/// A class definition.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ClassDef {
    /// Unique name; monomorphized instances use `Name<args>` spelling.
    pub name: String,
    /// True for `@CStruct class` (C-layout, copy semantics — C2).
    pub is_value: bool,
    /// True for a mirror-ingested boundary struct (a `declare class` in a
    /// `.d.ts`, P5.2): a C-layout value type whose constructor has no
    /// in-language body. `new` initializes its fields positionally from
    /// the constructor arguments (arg `i` → field `i`), applying the
    /// boundary coercions at each field (the chain-slot address-of for a
    /// `Struct | null` field). Always `false` for ordinary value classes,
    /// which carry a real [`ClassDef::ctor`].
    pub is_boundary: bool,
    /// Declared fields, in declaration order (C layout order).
    pub fields: Vec<Field>,
    /// The constructor, when declared.
    pub ctor: Option<Function>,
    /// Methods, in declaration order.
    pub methods: Vec<Function>,
    /// Position of the declaration.
    pub pos: Pos,
}

/// One declared field of a class.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Field {
    /// Field name.
    pub name: String,
    /// Resolved field type.
    pub ty: Type,
    /// Field initializer, when present.
    pub init: Option<Expr>,
    /// Position of the field declaration.
    pub pos: Pos,
}

/// A numeric enum definition.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EnumDef {
    /// Enum name.
    pub name: String,
    /// Members with their constant values, in declaration order.
    pub members: Vec<(String, i64)>,
    /// Position of the declaration.
    pub pos: Pos,
}

/// A module-level variable.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Global {
    /// Variable name.
    pub name: String,
    /// Resolved type.
    pub ty: Type,
    /// True for `let`, false for `const`.
    pub mutable: bool,
    /// Checked initializer.
    pub init: Expr,
    /// Position of the declaration.
    pub pos: Pos,
}

/// A checked function (free function, constructor, or method).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Function {
    /// Unique name; monomorphized instances use `name<args>` spelling.
    pub name: String,
    /// True when declared `export` (every exported function is a host
    /// entry point — Q12).
    pub exported: bool,
    /// True for `function*` coroutines (C8).
    pub is_generator: bool,
    /// Parameters, in order.
    pub params: Vec<Param>,
    /// Return type. For a generator this is `Generator<Y>` where `Y` is
    /// the yield type; `yield` expressions inside the body have type `Y`.
    pub ret: Type,
    /// Checked body statements.
    pub body: Vec<Stmt>,
    /// Position of the declaration.
    pub pos: Pos,
}

/// One function parameter.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Param {
    /// Parameter name.
    pub name: String,
    /// Resolved type.
    pub ty: Type,
    /// Checked default value, when declared (`a11`).
    pub default: Option<Expr>,
    /// Position of the parameter.
    pub pos: Pos,
}

/// A checked statement.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Stmt {
    /// Local variable declaration.
    Let {
        /// Variable name.
        name: String,
        /// Resolved (annotated or inferred) type.
        ty: Type,
        /// True for `let`, false for `const`.
        mutable: bool,
        /// Checked initializer.
        init: Expr,
        /// Position of the declaration.
        pos: Pos,
    },
    /// Expression statement.
    Expr(Expr),
    /// `return` with optional value.
    Return {
        /// Returned value, when present.
        value: Option<Expr>,
        /// Position of the statement.
        pos: Pos,
    },
    /// `if` / `else`.
    If {
        /// Condition (boolean).
        cond: Expr,
        /// Then-branch statements.
        then: Vec<Stmt>,
        /// Else-branch statements, when present.
        els: Option<Vec<Stmt>>,
        /// Position of the statement.
        pos: Pos,
    },
    /// `while` loop.
    While {
        /// Condition (boolean).
        cond: Expr,
        /// Body statements.
        body: Vec<Stmt>,
        /// Position of the statement.
        pos: Pos,
    },
    /// C-style `for` loop.
    For {
        /// Init statement (`let` or expression), when present.
        init: Option<Box<Stmt>>,
        /// Condition (boolean), when present.
        cond: Option<Expr>,
        /// Step expression, when present.
        step: Option<Expr>,
        /// Body statements.
        body: Vec<Stmt>,
        /// Position of the statement.
        pos: Pos,
    },
    /// `switch` over an integer or enum discriminant.
    Switch {
        /// Discriminant expression.
        disc: Expr,
        /// Cases in source order.
        cases: Vec<SwitchCase>,
        /// Position of the statement.
        pos: Pos,
    },
    /// `break`.
    Break(Pos),
    /// `continue`.
    Continue(Pos),
    /// Nested block scope.
    Block(Vec<Stmt>),
}

/// One `case` (or `default`) arm of a switch.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SwitchCase {
    /// Case test; `None` for `default`.
    pub test: Option<Expr>,
    /// Arm statements.
    pub body: Vec<Stmt>,
    /// Position of the arm.
    pub pos: Pos,
}

/// A checked expression: kind, resolved type, TS position.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Expr {
    /// Expression payload.
    pub kind: ExprKind,
    /// Resolved type. Where flow narrowing applies (C7), this is the
    /// narrowed type, not the declared one.
    pub ty: Type,
    /// Position of the expression.
    pub pos: Pos,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnOp {
    /// Numeric negation.
    Neg,
    /// Boolean not.
    Not,
    /// Bitwise complement (integers; true 64-bit on `i64`/`u64` — Q18).
    BitNot,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BinOp {
    /// Addition (numeric, or string concatenation — Q5).
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division (integer division on integer types, C semantics).
    Div,
    /// Remainder.
    Rem,
    /// Strict equality `===` (by content for strings — Q5).
    Eq,
    /// Strict inequality `!==`.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
    /// Logical and (booleans, short-circuit).
    And,
    /// Logical or (booleans, short-circuit).
    Or,
    /// Bitwise and (Q18).
    BitAnd,
    /// Bitwise or (Q18).
    BitOr,
    /// Bitwise xor (Q18).
    BitXor,
    /// Left shift (Q18).
    Shl,
    /// Sign-propagating right shift (Q18).
    Shr,
    /// Zero-fill right shift (Q18).
    UShr,
}

/// Ambient prelude functions (Q6, Q7, Q12); their signatures are
/// hardcoded in the checker, not parsed from `.d.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AmbientFn {
    /// `print(message: string): void`.
    Print,
    /// `collect(): void`.
    Collect,
    /// `unsafeDelete(value: object): void`.
    UnsafeDelete,
}

/// `Math` intrinsic functions (stdlib.md §1): ambient-namespace member
/// calls typed `f64` in and out, lowered by both tiers to the opaque
/// runtime symbol `sub_rt_math_<name>` — never the foreign-call path
/// and never a direct libm emission (stdlib.md §0.2). The constants
/// (`Math.PI`, …) are not represented here: a constant member read
/// folds to an [`ExprKind::Float`] literal at check time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MathFn {
    /// `Math.abs(x)`.
    Abs,
    /// `Math.acos(x)`.
    Acos,
    /// `Math.acosh(x)`.
    Acosh,
    /// `Math.asin(x)`.
    Asin,
    /// `Math.asinh(x)`.
    Asinh,
    /// `Math.atan(x)`.
    Atan,
    /// `Math.atanh(x)`.
    Atanh,
    /// `Math.cbrt(x)`.
    Cbrt,
    /// `Math.ceil(x)`.
    Ceil,
    /// `Math.cos(x)`.
    Cos,
    /// `Math.cosh(x)`.
    Cosh,
    /// `Math.exp(x)`.
    Exp,
    /// `Math.expm1(x)`.
    Expm1,
    /// `Math.floor(x)`.
    Floor,
    /// `Math.log(x)`.
    Log,
    /// `Math.log1p(x)`.
    Log1p,
    /// `Math.log10(x)`.
    Log10,
    /// `Math.log2(x)`.
    Log2,
    /// `Math.round(x)` (ECMA half-toward-+∞).
    Round,
    /// `Math.sign(x)` (±0/±1/NaN).
    Sign,
    /// `Math.sin(x)`.
    Sin,
    /// `Math.sinh(x)`.
    Sinh,
    /// `Math.sqrt(x)`.
    Sqrt,
    /// `Math.tan(x)`.
    Tan,
    /// `Math.tanh(x)`.
    Tanh,
    /// `Math.trunc(x)`.
    Trunc,
    /// `Math.atan2(y, x)`.
    Atan2,
    /// `Math.hypot(a, b)` (exactly two arguments, Q19).
    Hypot,
    /// `Math.pow(base, exp)`.
    Pow,
    /// `Math.max(a, b)` (exactly two arguments, Q19).
    Max,
    /// `Math.min(a, b)` (exactly two arguments, Q19).
    Min,
    /// `Math.random()` (stdlib.md §2: Context-seeded deterministic).
    Random,
}

impl MathFn {
    /// Every accepted `Math` function, in declaration order; the index
    /// of each variant equals its discriminant, so `f as usize` indexes
    /// tables built from this list.
    pub const ALL: [MathFn; 32] = [
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
    ];

    /// The member name, which is also the runtime symbol suffix
    /// (`sub_rt_math_<name>`).
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
        }
    }

    /// Number of `f64` arguments (exact; the lib's variadic forms are
    /// out of subset, Q19).
    #[must_use]
    pub fn arity(self) -> usize {
        match self {
            MathFn::Random => 0,
            MathFn::Atan2 | MathFn::Hypot | MathFn::Pow | MathFn::Max | MathFn::Min => 2,
            _ => 1,
        }
    }
}

/// `Date` intrinsic operations (stdlib.md §3): the accepted
/// UTC-deterministic subset, lowered by both tiers to the opaque
/// `sub_rt_date_*` runtime symbols. A `Date` value is `i64` epoch
/// milliseconds in generated code ([`crate::types::Type::Date`] erases
/// to `i64`); `getTime()` has no variant here — it is the identity on
/// the representation and folds to the receiver at check time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DateFn {
    /// `new Date(ms)` → `sub_rt_date_new` (TimeClip range check; out of
    /// range traps, Q20 — no Invalid-Date value).
    New,
    /// `Date.UTC(y, m0, d, h, min, s, ms)` → `sub_rt_date_utc`. The
    /// checker normalizes missing trailing arguments to their defaults
    /// (day 1, time components 0), so the call is always 7-argument.
    Utc,
    /// `Date.now()` → `sub_rt_date_now` (the Context clock; pinnable
    /// via `sub_rt_ctx_set_now`).
    Now,
    /// `getUTCFullYear()` → `sub_rt_date_get` field 0.
    GetUtcFullYear,
    /// `getUTCMonth()` (0-based) → `sub_rt_date_get` field 1.
    GetUtcMonth,
    /// `getUTCDate()` → `sub_rt_date_get` field 2.
    GetUtcDate,
    /// `getUTCDay()` (0 = Sunday) → `sub_rt_date_get` field 3.
    GetUtcDay,
    /// `getUTCHours()` → `sub_rt_date_get` field 4.
    GetUtcHours,
    /// `getUTCMinutes()` → `sub_rt_date_get` field 5.
    GetUtcMinutes,
    /// `getUTCSeconds()` → `sub_rt_date_get` field 6.
    GetUtcSeconds,
    /// `getUTCMilliseconds()` → `sub_rt_date_get` field 7.
    GetUtcMilliseconds,
    /// `toISOString()` → `sub_rt_date_to_iso` (years 0000–9999, else a
    /// trap, Q20).
    ToIso,
}

impl DateFn {
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
        }
    }

    /// The `sub_rt_date_get` field code of a UTC accessor (`None` for
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
}

/// `String` intrinsic methods (stdlib.md §8): the accepted Q21 subset,
/// lowered by both tiers to opaque `sub_rt_str_*` runtime symbols. Every
/// index, length, and code unit is a **byte** measure (Q21); case
/// mapping and whitespace are ASCII-only; range and argument errors
/// trap. The receiver is always the call's first argument. The checker
/// normalizes the optional arguments (`from` → 0, `pad` → `" "`) at
/// check time, so every runtime symbol has a fixed arity (the Date.UTC
/// technique, §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StrFn {
    /// `indexOf(needle, from)` — byte index or −1; `from` clamped to
    /// `[0, length]`; an empty needle returns the clamped `from`.
    IndexOf,
    /// `lastIndexOf(needle)` — last byte index or −1; an empty needle
    /// returns the length.
    LastIndexOf,
    /// `includes(needle, from)`.
    Includes,
    /// `startsWith(needle)`.
    StartsWith,
    /// `endsWith(needle)`.
    EndsWith,
    /// `charCodeAt(i)` — the byte value 0–255; out of range traps.
    CharCodeAt,
    /// `split(sep)` — `string[]`; an empty separator traps.
    Split,
    /// `trim()` — ASCII whitespace only.
    Trim,
    /// `trimStart()`.
    TrimStart,
    /// `trimEnd()`.
    TrimEnd,
    /// `repeat(n)` — `n < 0` traps; `repeat(0)` is `""`.
    Repeat,
    /// `padStart(len, pad)` — an empty `pad` with `len > length` traps.
    PadStart,
    /// `padEnd(len, pad)` — same trap rule as `padStart`.
    PadEnd,
    /// `toUpperCase()` — ASCII a–z only.
    ToUpperCase,
    /// `toLowerCase()` — ASCII A–Z only.
    ToLowerCase,
    /// `replace(pat, repl)` — first occurrence, literal (`$` is not
    /// interpreted, Q21).
    Replace,
    /// `replaceAll(pat, repl)` — all occurrences, literal; an empty
    /// `pat` traps.
    ReplaceAll,
}

/// Argument spelling of one [`StrFn`] parameter after the receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StrParam {
    /// A string handle.
    Str,
    /// An `i32` byte index / count / length.
    I32,
}

/// Result spelling of a [`StrFn`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StrRet {
    /// `i32` (byte index or byte value).
    I32,
    /// `boolean` (the runtime symbol returns `i32` 0/1).
    Bool,
    /// A freshly allocated string handle.
    Str,
    /// A freshly allocated `string[]` handle.
    StrArray,
}

impl StrFn {
    /// Every accepted `String` method, in declaration order; the index
    /// of each variant equals its discriminant, so `f as usize` indexes
    /// tables built from this list.
    pub const ALL: [StrFn; 17] = [
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
    ];

    /// The lib member name (the checker's lookup and diagnostics).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
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
        }
    }

    /// The opaque runtime symbol both tiers call.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            StrFn::IndexOf => "sub_rt_str_index_of",
            StrFn::LastIndexOf => "sub_rt_str_last_index_of",
            StrFn::Includes => "sub_rt_str_includes",
            StrFn::StartsWith => "sub_rt_str_starts_with",
            StrFn::EndsWith => "sub_rt_str_ends_with",
            StrFn::CharCodeAt => "sub_rt_str_char_code_at",
            StrFn::Split => "sub_rt_str_split",
            StrFn::Trim => "sub_rt_str_trim",
            StrFn::TrimStart => "sub_rt_str_trim_start",
            StrFn::TrimEnd => "sub_rt_str_trim_end",
            StrFn::Repeat => "sub_rt_str_repeat",
            StrFn::PadStart => "sub_rt_str_pad_start",
            StrFn::PadEnd => "sub_rt_str_pad_end",
            StrFn::ToUpperCase => "sub_rt_str_to_upper",
            StrFn::ToLowerCase => "sub_rt_str_to_lower",
            StrFn::Replace => "sub_rt_str_replace",
            StrFn::ReplaceAll => "sub_rt_str_replace_all",
        }
    }

    /// Parameter spellings after the receiver, post-normalization (the
    /// checker has already supplied the defaulted `from`/`pad`).
    #[must_use]
    pub fn params(self) -> &'static [StrParam] {
        match self {
            StrFn::IndexOf | StrFn::Includes => &[StrParam::Str, StrParam::I32],
            StrFn::LastIndexOf | StrFn::StartsWith | StrFn::EndsWith | StrFn::Split => {
                &[StrParam::Str]
            }
            StrFn::CharCodeAt | StrFn::Repeat => &[StrParam::I32],
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
            StrFn::IndexOf | StrFn::LastIndexOf | StrFn::CharCodeAt => StrRet::I32,
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
}

/// `Array` intrinsic methods (stdlib.md §9, Q22): the accepted subset
/// on `T[]`, lowered by both tiers to opaque `sub_rt_arr_*` runtime
/// symbols. The receiver handle is the call's first argument. Element
/// values the runtime *receives* (search needles, `fill` values,
/// `reduce`'s accumulator) travel by pointer, so every symbol has one
/// fixed C signature; values the runtime *passes to a script callback*
/// travel by value under the language calling convention
/// `(ctx, env, args…)`, dispatched inside the runtime from an
/// [`ArrElemKind`] tag plus the element byte width.
///
/// The checker normalizes optional arguments at check time (the
/// `Date.UTC` technique): `join`'s separator defaults `","`; `slice`'s
/// and `fill`'s missing `start` is `0` and missing `end` is the
/// [`ArrFn::END_SENTINEL`] (clamped to the length at runtime, so it
/// means "to the end").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ArrFn {
    /// `indexOf(x)` — first index by per-kind `===` equality, or −1.
    IndexOf,
    /// `lastIndexOf(x)` — last index or −1.
    LastIndexOf,
    /// `includes(x)` — per-kind `===` equality (so `NaN` is never
    /// found — the contract pins `===` semantics for all three, Q22).
    Includes,
    /// `join(sep)` — Q14 formatting per element; `sep` defaults `","`.
    Join,
    /// `slice(start, end)` — JS negative/clamp rules; fresh array.
    Slice,
    /// `fill(x, start, end)` — in place; the expression's value is the
    /// receiver.
    Fill,
    /// `reverse()` — in place; the expression's value is the receiver.
    Reverse,
    /// `concat(other)` — exactly one array argument; fresh array.
    Concat,
    /// `forEach(f)` — `f: (v: T) => void`.
    ForEach,
    /// `map(f)` — `f: (v: T) => U`; `U` inferred from the callback.
    Map,
    /// `filter(f)` — `f: (v: T) => boolean`; fresh array.
    Filter,
    /// `reduce(f, init)` — `f: (acc: U, v: T) => U`; `init` required
    /// (Q22). The accumulator travels by pointer (in/out).
    Reduce,
    /// `some(f)` — short-circuits on the first `true`.
    Some,
    /// `every(f)` — short-circuits on the first `false`.
    Every,
    /// `findIndex(f)` — first index where `f` is `true`, or −1.
    FindIndex,
    /// `sort(cmp)` — comparator required (Q22); stable merge sort; in
    /// place; the expression's value is the receiver.
    Sort,
}

impl ArrFn {
    /// Every accepted `Array` method, in declaration order; the index
    /// of each variant equals its discriminant, so `f as usize` indexes
    /// tables built from this list.
    pub const ALL: [ArrFn; 16] = [
        ArrFn::IndexOf,
        ArrFn::LastIndexOf,
        ArrFn::Includes,
        ArrFn::Join,
        ArrFn::Slice,
        ArrFn::Fill,
        ArrFn::Reverse,
        ArrFn::Concat,
        ArrFn::ForEach,
        ArrFn::Map,
        ArrFn::Filter,
        ArrFn::Reduce,
        ArrFn::Some,
        ArrFn::Every,
        ArrFn::FindIndex,
        ArrFn::Sort,
    ];

    /// The checker's spelling of a defaulted missing `end` argument of
    /// `slice`/`fill`: `i32::MAX`, which the runtime's JS clamp reduces
    /// to the length ("to the end"). An explicit `end` of this value
    /// means the same thing, so the sentinel is not observable.
    pub const END_SENTINEL: i64 = i32::MAX as i64;

    /// The lib member name (the checker's lookup and diagnostics).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ArrFn::IndexOf => "indexOf",
            ArrFn::LastIndexOf => "lastIndexOf",
            ArrFn::Includes => "includes",
            ArrFn::Join => "join",
            ArrFn::Slice => "slice",
            ArrFn::Fill => "fill",
            ArrFn::Reverse => "reverse",
            ArrFn::Concat => "concat",
            ArrFn::ForEach => "forEach",
            ArrFn::Map => "map",
            ArrFn::Filter => "filter",
            ArrFn::Reduce => "reduce",
            ArrFn::Some => "some",
            ArrFn::Every => "every",
            ArrFn::FindIndex => "findIndex",
            ArrFn::Sort => "sort",
        }
    }

    /// The opaque runtime symbol both tiers call.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            ArrFn::IndexOf => "sub_rt_arr_index_of",
            ArrFn::LastIndexOf => "sub_rt_arr_last_index_of",
            ArrFn::Includes => "sub_rt_arr_includes",
            ArrFn::Join => "sub_rt_arr_join",
            ArrFn::Slice => "sub_rt_arr_slice",
            ArrFn::Fill => "sub_rt_arr_fill",
            ArrFn::Reverse => "sub_rt_arr_reverse",
            ArrFn::Concat => "sub_rt_arr_concat",
            ArrFn::ForEach => "sub_rt_arr_for_each",
            ArrFn::Map => "sub_rt_arr_map",
            ArrFn::Filter => "sub_rt_arr_filter",
            ArrFn::Reduce => "sub_rt_arr_reduce",
            ArrFn::Some => "sub_rt_arr_some",
            ArrFn::Every => "sub_rt_arr_every",
            ArrFn::FindIndex => "sub_rt_arr_find_index",
            ArrFn::Sort => "sub_rt_arr_sort",
        }
    }

    /// True for the methods whose second HIR argument is a script
    /// callback (a `(code, env)` function value).
    #[must_use]
    pub fn takes_callback(self) -> bool {
        matches!(
            self,
            ArrFn::ForEach
                | ArrFn::Map
                | ArrFn::Filter
                | ArrFn::Reduce
                | ArrFn::Some
                | ArrFn::Every
                | ArrFn::FindIndex
                | ArrFn::Sort
        )
    }

    /// Whether the runtime symbol takes a trailing `pos_id`: exactly
    /// the operations that allocate through the Context (a fresh array
    /// or string), whose allocation failure traps at the call site.
    /// The callback-taking non-allocating operations surface only
    /// *callback* traps, which carry their own position.
    #[must_use]
    pub fn takes_pos_id(self) -> bool {
        matches!(
            self,
            ArrFn::Join | ArrFn::Slice | ArrFn::Concat | ArrFn::Map | ArrFn::Filter
        )
    }

    /// Whether the generated call must be followed by a trap check:
    /// every operation that can leave the Context trapped (an
    /// allocation failure, or a script callback that trapped).
    #[must_use]
    pub fn can_trap(self) -> bool {
        self.takes_callback() || self.takes_pos_id()
    }
}

/// The hash/equality kind of a monomorphized `Map` / `Set` key (Q24).
///
/// The stable codes are an ABI contract with
/// `runtime::assocops::KeyKind`; concrete byte width is supplied
/// separately by each codegen tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AssocKeyKind {
    /// Sized integer, boolean, enum, or `Date` bits.
    Bits,
    /// IEEE `f32` (`===`; `NaN` never matches).
    F32,
    /// IEEE `f64` (`===`; `NaN` never matches).
    F64,
    /// String content over UTF-8 bytes.
    Str,
    /// Reference-class identity.
    Ref,
}

impl AssocKeyKind {
    /// Stable runtime ABI code.
    #[must_use]
    pub fn code(self) -> u32 {
        match self {
            AssocKeyKind::Bits => 0,
            AssocKeyKind::F32 => 1,
            AssocKeyKind::F64 => 2,
            AssocKeyKind::Str => 3,
            AssocKeyKind::Ref => 4,
        }
    }

    /// Returns the Q24 key kind of `ty`, or `None` when it is outside
    /// the whitelist. `is_value_class` supplies the program's nominal
    /// class-kind lookup.
    #[must_use]
    pub fn of(ty: &Type, is_value_class: &dyn Fn(ClassId) -> bool) -> Option<AssocKeyKind> {
        Some(match ty {
            Type::I8
            | Type::U8
            | Type::I16
            | Type::U16
            | Type::I32
            | Type::U32
            | Type::I64
            | Type::U64
            | Type::Bool
            | Type::Enum(_)
            | Type::Date => AssocKeyKind::Bits,
            Type::F32 => AssocKeyKind::F32,
            Type::F64 => AssocKeyKind::F64,
            Type::Str => AssocKeyKind::Str,
            Type::Class(id) if !is_value_class(*id) => AssocKeyKind::Ref,
            Type::Map(..) | Type::Set(_) => AssocKeyKind::Ref,
            _ => return None,
        })
    }
}

/// `Map<K, V>` intrinsic operations (stdlib.md §10, Q24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MapFn {
    /// `new Map<K, V>()`.
    New,
    /// `size`.
    Size,
    /// `get(k)`.
    Get,
    /// `getOr(k, fallback)`.
    GetOr,
    /// `set(k, v)`.
    Set,
    /// `has(k)`.
    Has,
    /// `delete(k)`.
    Delete,
    /// `clear()`.
    Clear,
    /// `forEach(f)`.
    ForEach,
}

impl MapFn {
    /// Every accepted operation, in discriminant order.
    pub const ALL: [MapFn; 9] = [
        MapFn::New,
        MapFn::Size,
        MapFn::Get,
        MapFn::GetOr,
        MapFn::Set,
        MapFn::Has,
        MapFn::Delete,
        MapFn::Clear,
        MapFn::ForEach,
    ];

    /// Surface spelling.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            MapFn::New => "Map",
            MapFn::Size => "size",
            MapFn::Get => "get",
            MapFn::GetOr => "getOr",
            MapFn::Set => "set",
            MapFn::Has => "has",
            MapFn::Delete => "delete",
            MapFn::Clear => "clear",
            MapFn::ForEach => "forEach",
        }
    }

    /// Opaque runtime symbol.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            MapFn::New => "sub_rt_map_new",
            MapFn::Size => "sub_rt_map_size",
            MapFn::Get => "sub_rt_map_get",
            MapFn::GetOr => "sub_rt_map_get_or",
            MapFn::Set => "sub_rt_map_set",
            MapFn::Has => "sub_rt_map_has",
            MapFn::Delete => "sub_rt_map_delete",
            MapFn::Clear => "sub_rt_map_clear",
            MapFn::ForEach => "sub_rt_map_for_each",
        }
    }

    /// True when the operation may allocate Context memory.
    #[must_use]
    pub fn allocates(self) -> bool {
        matches!(self, MapFn::New | MapFn::Set)
    }

    /// True when generated code must check the trap flag afterward.
    #[must_use]
    pub fn can_trap(self) -> bool {
        self.allocates() || self == MapFn::ForEach
    }
}

/// `Set<K>` intrinsic operations (stdlib.md §10, Q24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SetFn {
    /// `new Set<K>()`.
    New,
    /// `size`.
    Size,
    /// `add(k)`.
    Add,
    /// `has(k)`.
    Has,
    /// `delete(k)`.
    Delete,
    /// `clear()`.
    Clear,
    /// `forEach(f)`.
    ForEach,
}

impl SetFn {
    /// Every accepted operation, in discriminant order.
    pub const ALL: [SetFn; 7] = [
        SetFn::New,
        SetFn::Size,
        SetFn::Add,
        SetFn::Has,
        SetFn::Delete,
        SetFn::Clear,
        SetFn::ForEach,
    ];

    /// Surface spelling.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            SetFn::New => "Set",
            SetFn::Size => "size",
            SetFn::Add => "add",
            SetFn::Has => "has",
            SetFn::Delete => "delete",
            SetFn::Clear => "clear",
            SetFn::ForEach => "forEach",
        }
    }

    /// Opaque runtime symbol.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            SetFn::New => "sub_rt_set_new",
            SetFn::Size => "sub_rt_set_size",
            SetFn::Add => "sub_rt_set_add",
            SetFn::Has => "sub_rt_set_has",
            SetFn::Delete => "sub_rt_set_delete",
            SetFn::Clear => "sub_rt_set_clear",
            SetFn::ForEach => "sub_rt_set_for_each",
        }
    }

    /// True when the operation may allocate Context memory.
    #[must_use]
    pub fn allocates(self) -> bool {
        matches!(self, SetFn::New | SetFn::Add)
    }

    /// True when generated code must check the trap flag afterward.
    #[must_use]
    pub fn can_trap(self) -> bool {
        self.allocates() || self == SetFn::ForEach
    }
}

/// The marshaling kind of an array element type (stdlib.md §9): what
/// the runtime needs to (a) compare two elements under `===` semantics
/// and (b) pass one element by value to a script callback. The byte
/// width completes the picture and comes from each tier's own element
/// size, so a type whose width differs between tiers (`boolean`) stays
/// correct on both.
///
/// The `u32` codes are an ABI contract with the runtime's `arrops`
/// module (`ElemKind`); a codegen test asserts the two tables agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ArrElemKind {
    /// Bitwise integer equality at the element width; passed in a
    /// zero-extending integer register. Covers unsigned sized integers,
    /// `boolean`, and reference handles (identity).
    Int,
    /// Bitwise integer equality at the element width; passed in a
    /// sign-extending integer register. Covers signed sized integers,
    /// enums, and `Date` (millis).
    SignedInt,
    /// IEEE `f32` equality (`NaN` never equal); float register.
    F32,
    /// IEEE `f64` equality; float register.
    F64,
    /// IEEE binary16 equality after widening through the shared runtime;
    /// raw bits cross callback boundaries in a 16-bit integer register.
    F16,
    /// String handle: content equality; integer (pointer) register.
    Str,
}

impl ArrElemKind {
    /// The stable `u32` code passed to the runtime.
    #[must_use]
    pub fn code(self) -> u32 {
        match self {
            ArrElemKind::Int => 0,
            ArrElemKind::F32 => 1,
            ArrElemKind::F64 => 2,
            ArrElemKind::Str => 3,
            ArrElemKind::F16 => 4,
            ArrElemKind::SignedInt => 5,
        }
    }

    /// The kind of element type `ty`, or `None` when the type cannot
    /// cross the runtime↔script element boundary (value classes,
    /// function values, `FixedArray`, `void`). `is_value_class`
    /// distinguishes value classes (excluded) from reference classes
    /// (identity — included).
    #[must_use]
    pub fn of(ty: &Type, is_value_class: &dyn Fn(ClassId) -> bool) -> Option<ArrElemKind> {
        Some(match ty {
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Object
            | Type::Array(_)
            | Type::Map(..)
            | Type::Set(_) => ArrElemKind::Int,
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Enum(_)
            | Type::Date => ArrElemKind::SignedInt,
            Type::Class(id) if !is_value_class(*id) => ArrElemKind::Int,
            Type::Nullable(inner) if !matches!(**inner, Type::Func(_)) => ArrElemKind::Int,
            Type::F32 => ArrElemKind::F32,
            Type::F64 => ArrElemKind::F64,
            Type::F16 => ArrElemKind::F16,
            Type::Str => ArrElemKind::Str,
            _ => return None,
        })
    }
}

/// The Q14 formatting kind of a `join` element (stdlib.md §9): selects
/// the runtime `fmt_*` family member. `None` for element types that are
/// not interpolatable (`Date` — Q20 — and references), which the
/// checker rejects.
///
/// The `u32` codes are an ABI contract with the runtime's `arrops`
/// module (`FmtKind`); a codegen test asserts the two tables agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ArrFmtKind {
    /// `i8` decimal.
    I8,
    /// `u8` decimal.
    U8,
    /// `i16` decimal.
    I16,
    /// `u16` decimal.
    U16,
    /// `i32` decimal (also enums, which are `i32`-valued).
    I32,
    /// `u32` decimal.
    U32,
    /// `i64` decimal.
    I64,
    /// `u64` decimal.
    U64,
    /// `f32` shortest round-trip.
    F32,
    /// `f64` shortest round-trip.
    F64,
    /// Binary16 widened through the shared runtime, then formatted by the
    /// `f64` Q14 implementation.
    F16,
    /// `true` / `false`.
    Bool,
    /// String elements pass through unformatted.
    Str,
}

impl ArrFmtKind {
    /// The stable `u32` code passed to the runtime.
    #[must_use]
    pub fn code(self) -> u32 {
        match self {
            ArrFmtKind::I32 => 0,
            ArrFmtKind::U32 => 1,
            ArrFmtKind::I64 => 2,
            ArrFmtKind::U64 => 3,
            ArrFmtKind::F32 => 4,
            ArrFmtKind::F64 => 5,
            ArrFmtKind::Bool => 6,
            ArrFmtKind::Str => 7,
            ArrFmtKind::I8 => 8,
            ArrFmtKind::U8 => 9,
            ArrFmtKind::I16 => 10,
            ArrFmtKind::U16 => 11,
            ArrFmtKind::F16 => 12,
        }
    }

    /// The formatting kind of element type `ty`, or `None` when `ty`
    /// is not interpolatable under Q14.
    #[must_use]
    pub fn of(ty: &Type) -> Option<ArrFmtKind> {
        Some(match ty {
            Type::I8 => ArrFmtKind::I8,
            Type::U8 => ArrFmtKind::U8,
            Type::I16 => ArrFmtKind::I16,
            Type::U16 => ArrFmtKind::U16,
            Type::I32 | Type::Enum(_) => ArrFmtKind::I32,
            Type::U32 => ArrFmtKind::U32,
            Type::I64 => ArrFmtKind::I64,
            Type::U64 => ArrFmtKind::U64,
            Type::F32 => ArrFmtKind::F32,
            Type::F64 => ArrFmtKind::F64,
            Type::F16 => ArrFmtKind::F16,
            Type::Bool => ArrFmtKind::Bool,
            Type::Str => ArrFmtKind::Str,
            _ => return None,
        })
    }
}

/// What a call dispatches to.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Callee {
    /// A module function by (possibly monomorphized) name.
    Func(String),
    /// A foreign C-ABI function declared by an ambient mirror (P5.2);
    /// carries the symbol name. No lowering path yet (P5.2b).
    Foreign(String),
    /// An ambient prelude function.
    Ambient(AmbientFn),
    /// A `Math.<fn>` ambient-namespace intrinsic (stdlib.md §1).
    Math(MathFn),
    /// A `Date` intrinsic (stdlib.md §3): `new Date(ms)`, the `Date.UTC`
    /// / `Date.now` statics, the UTC accessors, and `toISOString`. For
    /// the instance operations the receiver is the first argument.
    Date(DateFn),
    /// A `String` method intrinsic (stdlib.md §8, Q21). The receiver is
    /// the first argument; optional arguments were normalized at check
    /// time, so the arity is `1 + f.params().len()` exactly.
    Str(StrFn),
    /// An `Array` method intrinsic (stdlib.md §9, Q22). The receiver is
    /// the first argument; optional arguments were normalized at check
    /// time (`join` separator, `slice`/`fill` range). For `reduce` the
    /// argument order is `[receiver, callback, init]`.
    Arr(ArrFn),
    /// A `Map<K, V>` operation intrinsic (stdlib.md §10, Q24).
    Map(MapFn),
    /// A `Set<K>` operation intrinsic (stdlib.md §10, Q24).
    Set(SetFn),
    /// A function-typed value (function pointer or local lambda).
    Value(Box<Expr>),
    /// A method on a receiver: class methods, and the built-in members
    /// `push`/`pop` (arrays), `slice` (strings), `next` (generators).
    Method {
        /// Receiver expression.
        recv: Box<Expr>,
        /// Method name.
        name: String,
    },
}

/// One interpolation segment of a template literal.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TplPart {
    /// Literal text.
    Text(String),
    /// Interpolated expression (numeric, boolean, string, or enum;
    /// formatting per Q14).
    Expr(Expr),
}

/// Expression payloads.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ExprKind {
    /// Integer literal (value fits the expression's sized integer type).
    Int(i64),
    /// Float literal.
    Float(f64),
    /// Boolean literal.
    Bool(bool),
    /// String literal.
    Str(String),
    /// `null` literal.
    Null,
    /// `this` inside a constructor or method.
    This,
    /// Reference to a local (parameter or `let`/`const` binding).
    Local(String),
    /// Reference to a module-level variable.
    Global(String),
    /// A named function used as a value (non-capturing — C5).
    FuncRef(String),
    /// An enum member, e.g. `Status.Complete`.
    EnumMember {
        /// The enum.
        id: EnumId,
        /// Member name.
        member: String,
        /// Constant member value.
        value: i64,
    },
    /// Unary operation.
    Unary {
        /// Operator.
        op: UnOp,
        /// Operand.
        operand: Box<Expr>,
    },
    /// Binary operation.
    Binary {
        /// Operator.
        op: BinOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// Assignment (plain or compound). The target is a `Local`, `Global`,
    /// `Field`, or `Index` expression.
    Assign {
        /// Compound arithmetic operator, `None` for plain `=`.
        op: Option<BinOp>,
        /// Assignment target.
        target: Box<Expr>,
        /// Assigned value.
        value: Box<Expr>,
    },
    /// Explicit checked conversion `x as T` (C3); the target type is the
    /// expression's `ty`.
    Cast(Box<Expr>),
    /// Function/method call.
    Call {
        /// Dispatch target.
        callee: Callee,
        /// Arguments, in order.
        args: Vec<Expr>,
    },
    /// `new C(...)` construction.
    New {
        /// Constructed class.
        class: ClassId,
        /// Constructor arguments.
        args: Vec<Expr>,
    },
    /// Field access `obj.name` (classes and the `IterResult` shape).
    Field {
        /// Receiver.
        obj: Box<Expr>,
        /// Field name.
        name: String,
    },
    /// `length` of an array, `FixedArray`, or string.
    Length(Box<Expr>),
    /// Index access `obj[i]`.
    Index {
        /// Indexed array.
        obj: Box<Expr>,
        /// Index expression (`i32`).
        index: Box<Expr>,
    },
    /// Array literal; the expression type says whether it constructs a
    /// dynamic array or a `FixedArray` (Q3).
    ArrayLit(Vec<Expr>),
    /// Template literal (Q14 formatting at runtime).
    Template(Vec<TplPart>),
    /// Lambda expression. Non-capturing lambdas are free function
    /// values; capturing ones are stack-only and may not escape (C5).
    Lambda {
        /// Parameters.
        params: Vec<Param>,
        /// Return type.
        ret: Type,
        /// Body statements (an expression body becomes a single
        /// `return`).
        body: Vec<Stmt>,
        /// Names of captured `const` locals, empty when non-capturing.
        captures: Vec<String>,
    },
    /// `yield` inside a generator (C8).
    Yield(Option<Box<Expr>>),
    /// Conditional expression `c ? a : b`.
    Cond {
        /// Condition (boolean).
        cond: Box<Expr>,
        /// Value when true.
        then: Box<Expr>,
        /// Value when false.
        els: Box<Expr>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Pos;

    #[test]
    fn expr_carries_type_and_pos() {
        let e = Expr {
            kind: ExprKind::Int(3),
            ty: Type::I32,
            pos: Pos::new("t.ts", 1, 1),
        };
        assert_eq!(e.ty, Type::I32);
        assert_eq!(e.pos.line, 1);
    }

    #[test]
    fn math_fn_all_is_indexed_by_discriminant() {
        // Runtime-import tables index by `f as usize`; the ALL order
        // must therefore equal declaration order.
        for (i, f) in MathFn::ALL.iter().enumerate() {
            assert_eq!(*f as usize, i, "MathFn::ALL out of order at {i}");
        }
    }

    #[test]
    fn math_fn_arity_matches_the_contract() {
        assert_eq!(MathFn::Abs.arity(), 1);
        assert_eq!(MathFn::Atan2.arity(), 2);
        assert_eq!(MathFn::Hypot.arity(), 2);
        assert_eq!(MathFn::Pow.arity(), 2);
        assert_eq!(MathFn::Max.arity(), 2);
        assert_eq!(MathFn::Min.arity(), 2);
        assert_eq!(MathFn::Random.arity(), 0);
        assert_eq!(MathFn::Random.name(), "random");
        assert_eq!(MathFn::Log1p.name(), "log1p");
    }

    #[test]
    fn date_fn_field_codes_cover_the_eight_accessors_in_order() {
        // The sub_rt_date_get field-code contract (stdlib.md §3): the
        // eight UTC accessors carry codes 0..=7 in accessor order; the
        // non-accessor operations carry none.
        let accessors = [
            DateFn::GetUtcFullYear,
            DateFn::GetUtcMonth,
            DateFn::GetUtcDate,
            DateFn::GetUtcDay,
            DateFn::GetUtcHours,
            DateFn::GetUtcMinutes,
            DateFn::GetUtcSeconds,
            DateFn::GetUtcMilliseconds,
        ];
        for (i, f) in accessors.iter().enumerate() {
            assert_eq!(f.field_code(), Some(i as u32), "field code of {}", f.name());
        }
        for f in [DateFn::New, DateFn::Utc, DateFn::Now, DateFn::ToIso] {
            assert_eq!(f.field_code(), None, "{} is not an accessor", f.name());
        }
        assert_eq!(DateFn::ToIso.name(), "toISOString");
        assert_eq!(DateFn::Utc.name(), "UTC");
    }

    #[test]
    fn str_fn_all_is_indexed_by_discriminant() {
        // Runtime-import tables index by `f as usize`; the ALL order
        // must therefore equal declaration order.
        for (i, f) in StrFn::ALL.iter().enumerate() {
            assert_eq!(*f as usize, i, "StrFn::ALL out of order at {i}");
        }
    }

    #[test]
    fn str_fn_shapes_match_the_section_8_contract() {
        use StrParam as P;
        // Post-normalization parameter spellings (stdlib.md §8).
        assert_eq!(StrFn::IndexOf.params(), &[P::Str, P::I32]);
        assert_eq!(StrFn::Includes.params(), &[P::Str, P::I32]);
        assert_eq!(StrFn::LastIndexOf.params(), &[P::Str]);
        assert_eq!(StrFn::CharCodeAt.params(), &[P::I32]);
        assert_eq!(StrFn::Trim.params(), &[] as &[P]);
        assert_eq!(StrFn::PadStart.params(), &[P::I32, P::Str]);
        assert_eq!(StrFn::ReplaceAll.params(), &[P::Str, P::Str]);
        // Result spellings.
        assert_eq!(StrFn::IndexOf.ret(), StrRet::I32);
        assert_eq!(StrFn::CharCodeAt.ret(), StrRet::I32);
        assert_eq!(StrFn::Includes.ret(), StrRet::Bool);
        assert_eq!(StrFn::EndsWith.ret(), StrRet::Bool);
        assert_eq!(StrFn::Split.ret(), StrRet::StrArray);
        assert_eq!(StrFn::Trim.ret(), StrRet::Str);
        assert_eq!(StrFn::ReplaceAll.ret(), StrRet::Str);
        // pos_id: only the five pure search predicates take none.
        for f in StrFn::ALL {
            let pure = matches!(
                f,
                StrFn::IndexOf
                    | StrFn::LastIndexOf
                    | StrFn::Includes
                    | StrFn::StartsWith
                    | StrFn::EndsWith
            );
            assert_eq!(f.takes_pos_id(), !pure, "pos_id of {}", f.name());
        }
        // Symbols follow the sub_rt_str_* convention, distinctly.
        let mut symbols: Vec<&str> = StrFn::ALL.iter().map(|f| f.symbol()).collect();
        symbols.sort_unstable();
        symbols.dedup();
        assert_eq!(symbols.len(), StrFn::ALL.len());
        assert!(StrFn::ALL.iter().all(|f| f.symbol().starts_with("sub_rt_str_")));
        assert_eq!(StrFn::ToUpperCase.symbol(), "sub_rt_str_to_upper");
        assert_eq!(StrFn::CharCodeAt.name(), "charCodeAt");
    }

    #[test]
    fn arr_fn_all_is_indexed_by_discriminant() {
        // Runtime-import tables index by `f as usize`; the ALL order
        // must therefore equal declaration order.
        for (i, f) in ArrFn::ALL.iter().enumerate() {
            assert_eq!(*f as usize, i, "ArrFn::ALL out of order at {i}");
        }
    }

    #[test]
    fn arr_fn_shapes_match_the_section_9_contract() {
        // Symbols follow the sub_rt_arr_* convention, distinctly.
        let mut symbols: Vec<&str> = ArrFn::ALL.iter().map(|f| f.symbol()).collect();
        symbols.sort_unstable();
        symbols.dedup();
        assert_eq!(symbols.len(), ArrFn::ALL.len());
        assert!(ArrFn::ALL.iter().all(|f| f.symbol().starts_with("sub_rt_arr_")));
        assert_eq!(ArrFn::ForEach.symbol(), "sub_rt_arr_for_each");
        assert_eq!(ArrFn::FindIndex.name(), "findIndex");
        // Callback set: exactly the eight closure-taking methods.
        let with_cb: Vec<ArrFn> = ArrFn::ALL.iter().copied().filter(|f| f.takes_callback()).collect();
        assert_eq!(
            with_cb,
            [
                ArrFn::ForEach,
                ArrFn::Map,
                ArrFn::Filter,
                ArrFn::Reduce,
                ArrFn::Some,
                ArrFn::Every,
                ArrFn::FindIndex,
                ArrFn::Sort,
            ]
        );
        // pos_id: exactly the allocating operations.
        for f in ArrFn::ALL {
            let allocates = matches!(
                f,
                ArrFn::Join | ArrFn::Slice | ArrFn::Concat | ArrFn::Map | ArrFn::Filter
            );
            assert_eq!(f.takes_pos_id(), allocates, "pos_id of {}", f.name());
            assert_eq!(
                f.can_trap(),
                allocates || f.takes_callback(),
                "trap check of {}",
                f.name()
            );
        }
        assert_eq!(ArrFn::END_SENTINEL, i64::from(i32::MAX));
    }

    #[test]
    fn arr_elem_kind_covers_the_marshalable_types_and_nothing_else() {
        use crate::types::FuncType;
        let value_class = |id: ClassId| id.0 == 0; // class 0 is @CStruct, class 1 is a reference
        let of = |ty: &Type| ArrElemKind::of(ty, &value_class);
        for ty in [
            Type::Bool,
            Type::U8,
            Type::U16,
            Type::U32,
            Type::U64,
            Type::Object,
            Type::Class(ClassId(1)),
            Type::Nullable(Box::new(Type::Class(ClassId(1)))),
            Type::Array(Box::new(Type::I32)),
        ] {
            assert_eq!(of(&ty), Some(ArrElemKind::Int), "{ty:?}");
        }
        for ty in [
            Type::I8,
            Type::I16,
            Type::I32,
            Type::I64,
            Type::Date,
            Type::Enum(EnumId(0)),
        ] {
            assert_eq!(of(&ty), Some(ArrElemKind::SignedInt), "{ty:?}");
        }
        assert_eq!(of(&Type::F32), Some(ArrElemKind::F32));
        assert_eq!(of(&Type::F64), Some(ArrElemKind::F64));
        assert_eq!(of(&Type::F16), Some(ArrElemKind::F16));
        assert_eq!(of(&Type::Str), Some(ArrElemKind::Str));
        // Excluded: value classes, function values, FixedArray, void.
        assert_eq!(of(&Type::Class(ClassId(0))), None);
        let ft = Type::Func(Box::new(FuncType {
            params: vec![Type::I32],
            ret: Type::I32,
        }));
        assert_eq!(of(&ft), None);
        assert_eq!(of(&Type::Nullable(Box::new(ft))), None);
        assert_eq!(of(&Type::FixedArray(Box::new(Type::I32), 3)), None);
        assert_eq!(of(&Type::Void), None);
        // Stable ABI codes.
        assert_eq!(ArrElemKind::Int.code(), 0);
        assert_eq!(ArrElemKind::F32.code(), 1);
        assert_eq!(ArrElemKind::F64.code(), 2);
        assert_eq!(ArrElemKind::Str.code(), 3);
        assert_eq!(ArrElemKind::F16.code(), 4);
        assert_eq!(ArrElemKind::SignedInt.code(), 5);
    }

    #[test]
    fn arr_fmt_kind_matches_the_q14_interpolable_set() {
        assert_eq!(ArrFmtKind::of(&Type::I32), Some(ArrFmtKind::I32));
        assert_eq!(ArrFmtKind::of(&Type::I8), Some(ArrFmtKind::I8));
        assert_eq!(ArrFmtKind::of(&Type::U8), Some(ArrFmtKind::U8));
        assert_eq!(ArrFmtKind::of(&Type::I16), Some(ArrFmtKind::I16));
        assert_eq!(ArrFmtKind::of(&Type::U16), Some(ArrFmtKind::U16));
        assert_eq!(ArrFmtKind::of(&Type::Enum(EnumId(0))), Some(ArrFmtKind::I32));
        assert_eq!(ArrFmtKind::of(&Type::U32), Some(ArrFmtKind::U32));
        assert_eq!(ArrFmtKind::of(&Type::I64), Some(ArrFmtKind::I64));
        assert_eq!(ArrFmtKind::of(&Type::U64), Some(ArrFmtKind::U64));
        assert_eq!(ArrFmtKind::of(&Type::F32), Some(ArrFmtKind::F32));
        assert_eq!(ArrFmtKind::of(&Type::F64), Some(ArrFmtKind::F64));
        assert_eq!(ArrFmtKind::of(&Type::F16), Some(ArrFmtKind::F16));
        assert_eq!(ArrFmtKind::of(&Type::Bool), Some(ArrFmtKind::Bool));
        assert_eq!(ArrFmtKind::of(&Type::Str), Some(ArrFmtKind::Str));
        // Not interpolatable (Q20 for Date; references have no Q14 form).
        assert_eq!(ArrFmtKind::of(&Type::Date), None);
        assert_eq!(ArrFmtKind::of(&Type::Class(ClassId(0))), None);
        assert_eq!(ArrFmtKind::of(&Type::Object), None);
        // Stable ABI codes, in declaration order.
        let codes: Vec<u32> = [
            ArrFmtKind::I32,
            ArrFmtKind::U32,
            ArrFmtKind::I64,
            ArrFmtKind::U64,
            ArrFmtKind::F32,
            ArrFmtKind::F64,
            ArrFmtKind::Bool,
            ArrFmtKind::Str,
            ArrFmtKind::I8,
            ArrFmtKind::U8,
            ArrFmtKind::I16,
            ArrFmtKind::U16,
            ArrFmtKind::F16,
        ]
        .iter()
        .map(|k| k.code())
        .collect();
        assert_eq!(codes, (0..13).collect::<Vec<u32>>());
    }

    #[test]
    fn module_is_constructible_empty() {
        let m = Module {
            classes: Vec::new(),
            enums: Vec::new(),
            globals: Vec::new(),
            functions: Vec::new(),
            foreign_fns: Vec::new(),
            top_level: Vec::new(),
        };
        assert!(m.functions.is_empty());
    }
}
