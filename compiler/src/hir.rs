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
    /// True for `@value class` (C-layout, copy semantics — C2).
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
