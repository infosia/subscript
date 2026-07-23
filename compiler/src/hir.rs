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
