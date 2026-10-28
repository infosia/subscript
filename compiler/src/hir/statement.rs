//! Checked statements: the [`Stmt`] form and its loop and switch parts.

use super::{Expr, UsingBinding};
use crate::diag::Pos;
use crate::types::Type;

/// A checked statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// Local variable declaration.
    Let {
        /// Variable name.
        name: String,
        /// Resolved (annotated or inferred) type.
        ty: Type,
        /// True for `let`, false for `const`.
        mutable: bool,
        /// True when scope exit must call this binding's dispose method.
        dispose: bool,
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
    /// `do…while` loop (`compiler.md` §203): the body runs before the
    /// first evaluation of the condition, and `continue` goes to the
    /// condition.
    DoWhile {
        /// Body statements.
        body: Vec<Stmt>,
        /// Condition (boolean), evaluated after each run of the body.
        cond: Expr,
        /// Position of the statement.
        pos: Pos,
    },
    /// C-style `for` loop.
    For {
        /// Init statement (`let` or expression), when present.
        init: Option<Box<Stmt>>,
        /// Condition (boolean), when present.
        cond: Option<Expr>,
        /// Update statements, after the body exits and its finalizers.
        step: Vec<Stmt>,
        /// Body statements.
        body: Vec<Stmt>,
        /// Position of the statement.
        pos: Pos,
    },
    /// Allocation-free fused `for…of` over one built-in container.
    ///
    /// The subject is evaluated once by a checker-generated enclosing
    /// binding. `kind` fixes both the storage traversal and the value
    /// bound on each visit; no iterator value exists in HIR.
    ForOf {
        /// Loop binding name.
        name: String,
        /// Type bound on each visit.
        ty: Type,
        /// Checked, already-stabilized container subject.
        subject: Expr,
        /// Built-in traversal selected by the checker.
        kind: ForOfKind,
        /// Loop body.
        body: Vec<Stmt>,
        /// Position of the statement.
        pos: Pos,
    },
    /// Generator iteration with a distinct exhaustion exit and consumer exits.
    GeneratorForOf {
        /// Loop binding name.
        name: String,
        /// Type bound on each visit.
        ty: Type,
        /// True for a mutable element binding.
        mutable: bool,
        /// Generator expression evaluated once.
        subject: Expr,
        /// Loop body.
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
    /// `throw` of an Error-family object (`compiler.md` §115.2).
    Throw {
        /// The thrown Error object.
        value: Expr,
        /// Position of the statement: the raise site.
        pos: Pos,
    },
    /// `try` with one `catch` clause (`compiler.md` §115.3).
    Try {
        /// Statements of the `try` block.
        body: Vec<Stmt>,
        /// The catch binding and its type, the Error class; absent for
        /// `catch { }`.
        binding: Option<(String, Type)>,
        /// Statements of the `catch` block.
        handler: Vec<Stmt>,
        /// Position of the statement.
        pos: Pos,
    },
    /// A `using` scope (`compiler.md` §115.5 rule 5): the statements that
    /// follow a `using` declaration, to the end of its block. The lowering
    /// runs the hook of each binding on each edge that leaves `body`.
    Using {
        /// The bindings, in declaration order.
        bindings: Vec<UsingBinding>,
        /// The statements of the scope.
        body: Vec<Stmt>,
        /// The finalizer of this cleanup scope, when it has no resource bindings.
        finalizer: Option<Vec<Stmt>>,
        /// Position of the first declaration.
        pos: Pos,
    },
}

/// Closed set of fused built-in `for…of` traversals (stdlib.md §14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ForOfKind {
    /// Dynamic-array values in index order.
    ArrayValues,
    /// Dynamic-array integer indices (`array.keys()`).
    ArrayKeys,
    /// Fixed-array values in index order.
    FixedArrayValues,
    /// Map keys in insertion order (`map.keys()`).
    MapKeys,
    /// Map values in insertion order (`map.values()`).
    MapValues,
    /// Set values in insertion order (`set.keys()` / `set.values()`).
    SetValues,
    /// UTF-8 code points, each bound as a one-code-point string.
    StringCodePoints,
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
