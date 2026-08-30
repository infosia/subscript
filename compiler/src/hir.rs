//! Typed high-level IR produced by a successful check.
//!
//! Every expression node carries its resolved [`Type`] and a TS [`Pos`].
//! Generic declarations are monomorphized here: the module contains one
//! function/class per instantiation (e.g. `identity<i32>`), never a
//! generic template. A discovery HIR can contain [`Type::Error`] and one
//! or more [`PoisonedImport`] records.

use crate::diag::Pos;
use crate::types::{ClassId, EnumId, HandleClass, HandleKind, IterKind, Type};

/// Names the synchronous disposal hook after the checker lowers `[Symbol.dispose]`.
pub const DISPOSE_METHOD_NAME: &str = "[[Symbol.dispose]]";

/// A checked program: all source files merged into one module.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Module {
    /// Imports that refer to absent modules during a discovery check.
    pub poisoned_imports: Vec<PoisonedImport>,
    /// Class definitions (value and reference), indexed by [`ClassId`].
    pub classes: Vec<ClassDef>,
    /// Enum definitions, indexed by [`EnumId`].
    pub enums: Vec<EnumDef>,
    /// Nominal string-literal union aliases, indexed by
    /// [`crate::types::StringAliasId`].
    pub string_aliases: Vec<StringAliasDef>,
    /// Module-level variables.
    pub globals: Vec<Global>,
    /// Free functions, including monomorphized generic instances.
    /// Constructors and methods live on their [`ClassDef`].
    pub functions: Vec<Function>,
    /// Q35 worker-entry adapters required by `Worker.spawn` call sites,
    /// deduplicated by directly named function and message-class pair.
    pub worker_entries: Vec<WorkerEntry>,
    /// Checker-derived signatures for intrinsic and built-in calls.
    pub operation_signatures: Vec<OperationSignature>,
    /// Foreign (C-ABI) functions declared by an ingested ambient mirror
    /// (`declare function` in a `.d.ts`, P5.2). They carry a signature
    /// but no body; lowering a call to one is P5.2b, not P5.2a.
    pub foreign_fns: Vec<ForeignFn>,
    /// Ambient mirrors that contribute foreign functions, with the exact
    /// C header include spelling recovered from generated provenance.
    pub foreign_mirrors: Vec<ForeignMirror>,
    /// Checked top-level non-declaration statements, in source order
    /// (the accept corpus has none; kept for completeness).
    pub top_level: Vec<Stmt>,
}

/// One checker-derived intrinsic or built-in call signature.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationSignature {
    /// Semantic operation identity.
    pub target: OperationSignatureTarget,
    /// Normalized operand types in execution order.
    pub parameter_types: Vec<Type>,
    /// Result type, absent for a void operation.
    pub return_type: Option<Type>,
}

/// An intrinsic or built-in operation identity from the checker.
#[derive(Debug, Clone, PartialEq)]
pub enum OperationSignatureTarget {
    /// An ambient prelude function.
    Ambient(AmbientFn),
    /// A typed Context storage-byte operation.
    ContextBytes(ContextBytesFn, Type),
    /// A Math operation.
    Math(MathFn),
    /// A Number operation.
    Num(NumFn),
    /// A Date operation.
    Date(DateFn),
    /// A JSON operation.
    Json(JsonFn),
    /// A String operation.
    Str(StrFn),
    /// A regular-expression operation.
    Regex(RegexFn),
    /// An Array operation.
    Arr(ArrFn),
    /// A Map operation.
    Map(MapFn),
    /// A Set operation.
    Set(SetFn),
    /// A worker or channel-endpoint operation.
    Worker(WorkerFn),
    /// A built-in receiver method.
    BuiltinMethod(BuiltinMethod),
}

/// A built-in receiver method whose signature the checker declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinMethod {
    /// `Array.push`.
    ArrayPush,
    /// `Array.pop`.
    ArrayPop,
    /// `String.slice`.
    StringSlice,
    /// `Generator.next`.
    GeneratorNext,
}

/// One import statement of an absent module, accepted during a discovery check.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PoisonedImport {
    /// The specifier as written in the import.
    pub module: String,
    /// `(imported, local)` name pairs in source order.
    pub names: Vec<(String, String)>,
    /// Source position of the module specifier string.
    pub pos: Pos,
}

/// One monomorphized Q35 runtime-to-script worker entry adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkerEntry {
    /// Directly named module-level script function.
    pub function: String,
    /// Parent-to-worker message class.
    pub input: ClassId,
    /// Worker-to-parent message class.
    pub output: ClassId,
}

/// Stable index into [`Module::foreign_mirrors`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForeignMirrorId(pub usize);

/// One ingested C-header mirror that contributes foreign functions.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ForeignMirror {
    /// Ambient source name used in diagnostics.
    pub source_name: String,
    /// Basename written by the host in a C `#include`.
    pub include: String,
}

/// Typed C spelling attached directly to the boundary type occurrence that
/// absorbed it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ForeignTypeProvenance {
    /// A by-value C `(pointer, count)` descriptor mapped to a language array.
    Descriptor {
        /// C descriptor struct name used by a compound literal.
        aggregate: String,
        /// C element type name used by a mutable element-pointer cast.
        element: String,
        /// True when the descriptor's element pointer is const.
        element_const: bool,
    },
    /// Two adjacent C parameters `size_t <n>Count, [const] E* <n>` mapped
    /// to one language array parameter (§27/§34).
    ScalarPair {
        /// C element spelling used for the emitted-C pointer cast.
        element: String,
        /// True when the C element pointer is const (input direction).
        element_const: bool,
    },
    /// A by-value length-carrying C string view mapped to `string`.
    StringView {
        /// C string-view struct name used by a compound literal.
        aggregate: String,
    },
    /// A C function-pointer typedef attached to a mirrored struct field.
    Callback {
        /// C typedef name used to cast the runtime callback trampoline.
        typedef_name: String,
    },
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
    /// Mirror whose header declares this C symbol.
    pub mirror: ForeignMirrorId,
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
    /// The explicit value-class alignment and its decorator position.
    pub alignment_override: Option<AlignmentOverride>,
    /// True for a literal-constructible `@Descriptor` reference class
    /// (Q33). Descriptor classes have fields only; object literals lower
    /// through [`ExprKind::DescriptorLit`].
    pub is_descriptor: bool,
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
    /// The class index signature that rewrites indexed access to `get` and `set` calls.
    pub index_signature: Option<IndexSignature>,
    /// Position of the declaration.
    pub pos: Pos,
}

impl From<&ClassDef> for HandleClass {
    fn from(class: &ClassDef) -> Self {
        if !class.is_value {
            Self::Reference
        } else if class.is_boundary {
            Self::BoundaryValue
        } else {
            Self::Value
        }
    }
}

/// An explicit alignment on an `@CStruct` value class.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AlignmentOverride {
    /// The requested alignment in bytes.
    pub value: u32,
    /// Position of the decorator that requests the alignment.
    pub pos: Pos,
}

/// The accessor types for one class index signature.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct IndexSignature {
    /// The required index type. This type is `i32` or `u32`.
    pub index_ty: Type,
    /// The element type that `get` returns and `set` accepts.
    pub element_ty: Type,
    /// True when the signature does not permit indexed writes.
    pub readonly: bool,
}

/// One declared field of a class.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Field {
    /// Field name.
    pub name: String,
    /// Resolved field type.
    pub ty: Type,
    /// True when this is a Q33 descriptor field spelled `name?: T = expr`.
    /// For every other field this is false.
    pub is_defaulted: bool,
    /// True when this is an R16 descriptor field spelled `name?: A`, where
    /// `A` is a Q32 string-literal union alias. Omission stores the reserved
    /// absent discriminant instead of evaluating a default.
    pub is_absence_capable: bool,
    /// Field initializer, when present.
    pub init: Option<Expr>,
    /// C typedef attached to a mirrored callback field, when present.
    pub foreign_provenance: Option<ForeignTypeProvenance>,
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

/// A nominal closed set of string literals (Q32).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StringAliasDef {
    /// Source-level alias name.
    pub name: String,
    /// Member spellings in declaration/discriminant order.
    pub members: Vec<String>,
    /// Per-member C-boundary values for an R23 `CEnum` alias, in the same
    /// declaration order. `None` identifies a plain Q32 alias.
    pub wire_values: Option<Vec<i32>>,
    /// Position of the alias declaration.
    pub pos: Pos,
}

impl StringAliasDef {
    /// The implementation-reserved discriminant for an absent descriptor
    /// member. Plain aliases retain R16's `-1`; a wire alias chooses the
    /// first `i32` at or above `i32::MIN` that is outside its wire set.
    #[must_use]
    pub fn absence_discriminant(&self) -> i64 {
        let Some(wire_values) = &self.wire_values else {
            return crate::types::ABSENT_STRING_ALIAS_DISCRIMINANT;
        };
        let mut candidate = i32::MIN;
        while wire_values.contains(&candidate) {
            candidate = candidate
                .checked_add(1)
                .expect("a CEnum with at most i32::MAX members leaves a sentinel");
        }
        i64::from(candidate)
    }

    /// The representation of the declaration-ordered member at `index`.
    /// Wire aliases use the declared wire value; plain aliases use `index`.
    #[must_use]
    pub fn member_discriminant(&self, index: usize) -> Option<i64> {
        match &self.wire_values {
            Some(values) => values.get(index).copied().map(i64::from),
            None => i64::try_from(index).ok(),
        }
    }
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
    /// Number of checked top-level statements that run before this initializer.
    pub initializer_index: usize,
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
    /// True for a poll-driven async function or reference-class instance
    /// method (Q34/R13). `ret` is the fulfilled value type inside the
    /// source-level `Promise<ret>` view.
    pub is_async: bool,
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

impl Function {
    /// Fault points owned by entering this function rather than by one of
    /// its body expressions.
    ///
    /// Generator invocation allocates its suspended frame in the generated
    /// creator function. Ordinary functions have no function-level site.
    #[must_use]
    pub fn trap_sites(&self) -> Vec<TrapSite> {
        if self.is_generator || self.is_async {
            vec![TrapSite::Allocation {
                pos: self.pos.clone(),
            }]
        } else {
            Vec::new()
        }
    }

    /// Fault points owned by the host-entry adapter for this function.
    ///
    /// A wire-mapped string alias enters the adapter as its integer wire
    /// value. The adapter validates that value before it calls the script
    /// function. Other function-level and expression-level sites are returned
    /// by [`Function::trap_sites`] and [`Expr::trap_sites`].
    #[must_use]
    pub fn host_entry_trap_sites(&self, module: &Module) -> Option<Vec<TrapSite>> {
        if !self.exported
            || self.is_generator
            || self.ret != Type::Void
            || (self.is_async && !self.params.is_empty())
        {
            return None;
        }
        let parameter_is_supported = |parameter: &Param| {
            parameter.ty.is_numeric()
                || parameter.ty == Type::Bool
                || matches!(&parameter.ty, Type::Class(id) if module
                .classes
                .get(id.0)
                .is_some_and(|class| {
                    !class.is_value
                        && !class.is_descriptor
                        && !class.is_boundary
                        && class.fields.is_empty()
                        && class.ctor.is_none()
                        && class.methods.is_empty()
                        && class.index_signature.is_none()
                }))
                || matches!(&parameter.ty, Type::StringAlias(alias) if module
                    .string_aliases
                    .get(alias.0)
                    .is_some_and(|definition| definition.wire_values.is_some()))
        };
        if !self.params.iter().all(parameter_is_supported) {
            return None;
        }
        Some(
            self.params
                .iter()
                .filter_map(|parameter| {
                    let Type::StringAlias(alias) = &parameter.ty else {
                        return None;
                    };
                    module
                        .string_aliases
                        .get(alias.0)
                        .and_then(|definition| definition.wire_values.as_ref())
                        .map(|_| TrapSite::WireEnumValue {
                            alias: *alias,
                            pos: parameter.pos.clone(),
                        })
                })
                .collect(),
        )
    }
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
    /// C spelling absorbed at this foreign boundary parameter.
    pub foreign_provenance: Option<ForeignTypeProvenance>,
    /// Position of the parameter.
    pub pos: Pos,
}

/// One immutable local copied by value into a closure environment.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Capture {
    /// Captured local name.
    pub name: String,
    /// Resolved type stored in the environment.
    pub ty: Type,
}

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
    /// Map keys in insertion order (bare `Map` and `map.keys()`).
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

/// One immediate node below an HIR expression or statement.
#[derive(Debug, Clone, Copy)]
pub enum HirChild<'a> {
    /// An expression child.
    Expr(&'a Expr),
    /// A statement child.
    Stmt(&'a Stmt),
}

/// One mutable immediate node below an HIR expression or statement.
#[derive(Debug)]
pub(crate) enum HirChildMut<'a> {
    /// An expression child.
    Expr(&'a mut Expr),
    /// A statement child.
    Stmt(&'a mut Stmt),
}

/// Closed set of sites that copy or consume a counted async owner (§70.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncCopySite {
    /// A local or global binding stores the value.
    Binding,
    /// An assignment stores the value.
    Assignment,
    /// An array literal stores one element.
    ArrayElement,
    /// An array spread literal stores one element or source array.
    SpreadElement,
    /// A call stores one argument in its parameter.
    CallArgument,
    /// A return stores the value in its caller-owned result.
    Return,
    /// A fused `for…of` binding stores the current element.
    ForOfBinding,
    /// A conditional stores one arm in its result.
    ConditionalResult,
    /// A statement consumes and discards a fresh result.
    DiscardedResult,
}

/// One fault point carried by typed HIR.
///
/// The variants describe the guard and its operand roles; a lowering
/// combines a site with values it has already materialized. In particular,
/// a lowering must never satisfy a site's operands by re-emitting an HIR
/// expression. This enum deliberately is exhaustive across crates: adding a
/// variant must make both lowering matches fail to compile until they state
/// how the new site is handled.
#[derive(Debug, Clone, PartialEq)]
pub enum TrapSite {
    /// A runtime allocation whose failure leaves the Context trapped.
    Allocation {
        /// Position passed to the allocating runtime operation.
        pos: Pos,
    },
    /// Unwind after a call that can leave the Context trapped.
    Call {
        /// Position of the call.
        pos: Pos,
    },
    /// A reached `unreachable()` call statement traps unconditionally.
    Unreachable {
        /// Position of the call.
        pos: Pos,
    },
    /// The materialized integer divisor must be nonzero.
    DivisionByZero {
        /// Position of the division or remainder.
        pos: Pos,
    },
    /// Bounds-checked read through a materialized array handle/base and
    /// index.
    IndexRead {
        /// Position of the index expression.
        pos: Pos,
    },
    /// Bounds-checked write through a materialized array handle/base and
    /// index.
    IndexWrite {
        /// Position of the assignment target.
        pos: Pos,
    },
    /// `JsonResult.value` requires the materialized sibling `ok` value.
    JsonResultValue {
        /// Position of the `.value` read.
        pos: Pos,
    },
    /// Reference narrowing requires a non-null materialized pointer.
    NullNarrowing {
        /// Position of the `as` expression.
        pos: Pos,
    },
    /// Reference narrowing requires the materialized allocation's class id
    /// to match `class`.
    ClassMismatch {
        /// Required reference class.
        class: ClassId,
        /// Position of the `as` expression.
        pos: Pos,
    },
    /// Q6's dev-tier-only allocation-lifetime validation.
    ///
    /// The releasing C tier intentionally has no corresponding check
    /// (`compiler.md` §8.1b), but it must still match this explicit site.
    DevOnlyLifetime {
        /// Position of the access or delete.
        pos: Pos,
    },
    /// Reload-mode-only coroutine epoch validation.
    ///
    /// A shipped C body cannot become stale because it has no body-swap
    /// mode; both lowerings still match the site explicitly.
    DevReloadOnlyStaleCoroutine {
        /// Position of the generator `.next()` call.
        pos: Pos,
    },
    /// A C-entered wire-alias value must be a declared member value.
    WireEnumValue {
        /// Wire-mapped alias whose table is used at the crossing.
        alias: crate::types::StringAliasId,
        /// Position of the foreign call.
        pos: Pos,
    },
}

impl TrapSite {
    /// Source position owned by this individual guard/check point.
    #[must_use]
    pub fn pos(&self) -> &Pos {
        match self {
            TrapSite::Allocation { pos }
            | TrapSite::Call { pos }
            | TrapSite::Unreachable { pos }
            | TrapSite::DivisionByZero { pos }
            | TrapSite::IndexRead { pos }
            | TrapSite::IndexWrite { pos }
            | TrapSite::JsonResultValue { pos }
            | TrapSite::NullNarrowing { pos }
            | TrapSite::ClassMismatch { pos, .. }
            | TrapSite::DevOnlyLifetime { pos }
            | TrapSite::DevReloadOnlyStaleCoroutine { pos }
            | TrapSite::WireEnumValue { pos, .. } => pos,
        }
    }
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

/// Ambient prelude functions and namespace members (Q6, Q7, Q12, R15);
/// their signatures are hardcoded in the checker, not parsed from
/// `.d.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AmbientFn {
    /// `print(message: string): void`.
    Print,
    /// `unreachable(): never`, legal only as a call statement.
    Unreachable,
    /// `Context.collect(): void`.
    Collect,
    /// `Context.free(value: object): void`.
    UnsafeDelete,
}

/// Typed `Context` storage-byte operations (stdlib.md section 18, R34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContextBytesFn {
    /// `Context.bytesOf<T>(value): u8[]`.
    BytesOf,
    /// `Context.bytesInto<T>(value, target, offset): void`.
    BytesInto,
    /// `Context.fromBytes<T>(bytes, offset): T`.
    FromBytes,
}

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

/// Both tiers lower `Math` intrinsic calls (stdlib.md §1) to opaque
/// runtime symbols. These calls never use the foreign-call path or emit
/// direct libm calls (stdlib.md §0.2). The checker folds each constant
/// member read to an [`ExprKind::Float`] literal at check time.
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
    /// `Math.clz32(x)` with a `u32` argument and `i32` result.
    Clz32,
    /// `Math.imul(a, b)` with `i32` arguments and wrapping `i32` result.
    Imul,
    /// `Math.fround(x)` with an `f64` argument and `f32`-rounded `f64`
    /// result.
    Fround,
    /// `Math.f32ToBits(value)` with an `f64` argument and `u32` result.
    F32ToBits,
    /// `Math.f32FromBits(bits)` with a `u32` argument and `f64` result.
    F32FromBits,
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

/// `Number` and parsing intrinsics (stdlib.md §11, Q25/Q26).
/// Constants fold to [`ExprKind::Float`] at check time; every operation
/// represented here calls one opaque `subscript_rt_num_*` runtime symbol on
/// both execution tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NumFn {
    /// `Number.isNaN(value)`.
    IsNaN,
    /// `Number.isFinite(value)`.
    IsFinite,
    /// `Number.isInteger(value)`.
    IsInteger,
    /// `Number.isSafeInteger(value)`.
    IsSafeInteger,
    /// Global `parseInt(s, radix)`; the radix is required.
    ParseInt,
    /// Global `parseFloat(s)`.
    ParseFloat,
    /// `value.toFixed(digits)` after an `f32` receiver is widened
    /// exactly to `f64` by the checker.
    ToFixed,
    /// `f32_value.toString(radix)`; kept at `f32` so radix 10 is
    /// exactly the Q14 `f32` form.
    ToStringF32,
    /// `f64_value.toString(radix)`.
    ToStringF64,
    /// `value.toExponential(digits?)`; omission is normalized to a
    /// `-1` digit sentinel by the checker.
    ToExponential,
    /// `value.toPrecision(digits)`.
    ToPrecision,
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
            NumFn::ToFixed => "toFixed(digits: i32): string",
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
            NumFn::ToFixed => "Formats with a required fixed-decimal digit count.",
            NumFn::ToStringF32 | NumFn::ToStringF64 => {
                "Formats in an explicit radix from 2 through 36."
            }
            NumFn::ToExponential => "Formats in exponential notation.",
            NumFn::ToPrecision => "Formats with a required significant-digit count.",
        }
    }
}

/// Internal operations used by the checker-generated, monomorphized
/// `JSON.stringify<T>` serializers and `JSON.parse<T>` deserializers
/// (stdlib.md §13, Q28). These are not independently callable source
/// members: the checker expands one accepted call into ordinary typed
/// helper functions whose only special leaves are these opaque runtime
/// calls. Both execution tiers therefore lower the same finite HIR graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum JsonFn {
    /// Starts an output builder with no cycle-tracking state.
    Begin,
    /// Starts an output builder and its active-reference set.
    BeginTracked,
    /// Completes a builder and returns the immutable string.
    Finish,
    /// Appends an already-JSON-shaped string's bytes.
    Raw,
    /// Appends one language string with JSON quoting and escaping.
    Str,
    /// Appends a signed 32-bit integer.
    I32,
    /// Appends an unsigned 32-bit integer.
    U32,
    /// Appends a signed 64-bit integer.
    I64,
    /// Appends an unsigned 64-bit integer.
    U64,
    /// Appends a finite `f32`, trapping on NaN or infinity.
    F32,
    /// Appends a finite `f64`, trapping on NaN or infinity.
    F64,
    /// Appends a boolean.
    Bool,
    /// Appends a Date as a quoted ISO string.
    Date,
    /// Appends JSON `null`.
    Null,
    /// Inserts a reference in the active-path set; false means a cycle
    /// was found and a trap was recorded.
    Visit,
    /// Removes a reference from the active-path set.
    Leave,
    /// Parses complete text into a transient syntax tree; zero means
    /// malformed input and is data, not a trap.
    ParseBegin,
    /// Removes a transient parsed syntax tree.
    ParseEnd,
    /// Returns the root node handle.
    ParseRoot,
    /// Tests a node's JSON kind tag.
    ParseIsKind,
    /// Tests whether a number fits one exact sized numeric target.
    ParseNumberFits,
    /// Reads a validated number as `f64`.
    ParseNumber,
    /// Reads a validated sized integer exactly from its JSON token text.
    ParseInteger,
    /// Reads a validated boolean.
    ParseBool,
    /// Allocates a language string from a validated string node.
    ParseString,
    /// Returns a validated array node's length.
    ParseArrayLen,
    /// Returns an array element node.
    ParseArrayGet,
    /// Returns the last occurrence of an object field, or zero if absent.
    ParseObjectGet,
}

impl JsonFn {
    /// Every internal JSON runtime leaf in discriminant order.
    pub const ALL: [JsonFn; 28] = [
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

/// `Date` intrinsic operations (stdlib.md §3): the accepted
/// UTC-deterministic subset, lowered by both tiers to the opaque
/// `subscript_rt_date_*` runtime symbols. A `Date` value is `i64` epoch
/// milliseconds in generated code ([`crate::types::Type::Date`] erases
/// to `i64`); `getTime()` has no variant here — it is the identity on
/// the representation and folds to the receiver at check time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DateFn {
    /// `new Date(ms)` → `subscript_rt_date_new` (TimeClip range check; out of
    /// range traps, Q20 — no Invalid-Date value).
    New,
    /// `Date.UTC(y, m0, d, h, min, s, ms)` → `subscript_rt_date_utc`. The
    /// checker normalizes missing trailing arguments to their defaults
    /// (day 1, time components 0), so the call is always 7-argument.
    Utc,
    /// `Date.now()` → `subscript_rt_date_now` (the Context clock; pinnable
    /// via `subscript_rt_ctx_set_now`).
    Now,
    /// `getUTCFullYear()` → `subscript_rt_date_get` field 0.
    GetUtcFullYear,
    /// `getUTCMonth()` (0-based) → `subscript_rt_date_get` field 1.
    GetUtcMonth,
    /// `getUTCDate()` → `subscript_rt_date_get` field 2.
    GetUtcDate,
    /// `getUTCDay()` (0 = Sunday) → `subscript_rt_date_get` field 3.
    GetUtcDay,
    /// `getUTCHours()` → `subscript_rt_date_get` field 4.
    GetUtcHours,
    /// `getUTCMinutes()` → `subscript_rt_date_get` field 5.
    GetUtcMinutes,
    /// `getUTCSeconds()` → `subscript_rt_date_get` field 6.
    GetUtcSeconds,
    /// `getUTCMilliseconds()` → `subscript_rt_date_get` field 7.
    GetUtcMilliseconds,
    /// `toISOString()` → `subscript_rt_date_to_iso` (years 0000–9999, else a
    /// trap, Q20).
    ToIso,
}

impl DateFn {
    /// Every accepted Date operation in discriminant order.
    pub const ALL: [DateFn; 12] = [
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
    ];

    /// Whether the runtime call can leave the Context trapped.
    #[must_use]
    pub fn can_trap(self) -> bool {
        matches!(self, DateFn::New | DateFn::Utc | DateFn::ToIso)
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
                "UTC(year: i32, month: i32, date?: i32, hours?: i32, minutes?: i32, seconds?: i32, milliseconds?: i32): i64"
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
        }
    }
}

/// `String` intrinsic methods (stdlib.md §8): the accepted Q21/Q27 subset,
/// lowered by both tiers to opaque `subscript_rt_str_*` runtime symbols. Every
/// index, length, and code unit is a **byte** measure (Q21); case
/// mapping uses Unicode Default Case Conversion and trimming uses ECMA
/// whitespace; range and argument errors trap. The receiver is always
/// the call's first argument. The checker normalizes the optional
/// arguments (positions → their ECMA defaults, `pad` → `" "`) at check
/// time, so every runtime symbol has a fixed arity (the Date.UTC
/// technique, §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StrFn {
    /// `slice(start, end)` — JS negative/clamp rules over UTF-8 byte
    /// offsets; off-boundary indices trap.
    Slice,
    /// `indexOf(needle, from)` — byte index or −1; `from` clamped to
    /// `[0, length]`; an empty needle returns the clamped `from`.
    IndexOf,
    /// `lastIndexOf(needle)` — last byte index or −1; an empty needle
    /// returns the length.
    LastIndexOf,
    /// `includes(needle, from)`.
    Includes,
    /// `startsWith(needle, position)` with a byte position.
    StartsWith,
    /// `endsWith(needle, endPosition)` with a byte position.
    EndsWith,
    /// `charCodeAt(i)` — the byte value 0–255; out of range traps.
    CharCodeAt,
    /// `split(sep)` — `string[]`; an empty separator traps.
    Split,
    /// `trim()` — ECMA WhiteSpace + LineTerminator code points.
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
    /// `toUpperCase()` — Unicode Default Case Conversion.
    ToUpperCase,
    /// `toLowerCase()` — Unicode Default Case Conversion.
    ToLowerCase,
    /// `replace(pat, repl)` — first occurrence with ECMA string-pattern
    /// `$` substitutions (Q27).
    Replace,
    /// `replaceAll(pat, repl)` — all occurrences with ECMA
    /// string-pattern `$` substitutions; an empty `pat` traps.
    ReplaceAll,
    /// `substring(start, end)` — negative offsets clamp to zero and a
    /// reversed pair is swapped; byte boundaries are required.
    Substring,
    /// `substr(start, length)` — a negative start counts from the end;
    /// byte boundaries are required.
    Substr,
    /// `charAt(i)` — the code point beginning at byte `i`, or `""`
    /// when out of range; an off-boundary index traps.
    CharAt,
    /// `codePointAt(i)` — the code point beginning at byte `i`; an
    /// out-of-range or off-boundary index traps.
    CodePointAt,
    /// `concat(other)` — exactly one string argument.
    Concat,
}

/// Regular-expression intrinsics (stdlib.md §15, Q31).
///
/// Every value crossing this ABI is scalar: Context, string, array, and
/// RegExp values are handles and capture indices are `i32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RegexFn {
    /// `new RegExp(pattern, flags)` and a regex literal.
    New,
    /// `re.test(subject)`.
    Test,
    /// `re.source`.
    Source,
    /// `re.flags`.
    Flags,
    /// `subject.search(re)`.
    Search,
    /// `subject.replace(re, replacement)`.
    Replace,
    /// `subject.replaceAll(re, replacement)`.
    ReplaceAll,
    /// `subject.split(re)`.
    Split,
    /// `re.matchStart(group)`.
    MatchStart,
    /// `re.matchEnd(group)`.
    MatchEnd,
}

impl RegexFn {
    /// Every regex intrinsic in discriminant order.
    pub const ALL: [RegexFn; 10] = [
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
    ];

    /// Opaque runtime symbol used by both execution tiers.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            RegexFn::New => "subscript_rt_regex_new",
            RegexFn::Test => "subscript_rt_regex_test",
            RegexFn::Source => "subscript_rt_regex_source",
            RegexFn::Flags => "subscript_rt_regex_flags",
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
            RegexFn::Search => "string.search(pattern: RegExp): i32",
            RegexFn::Replace => "string.replace(pattern: RegExp, replacement: string): string",
            RegexFn::ReplaceAll => {
                "string.replaceAll(pattern: RegExp, replacement: string): string"
            }
            RegexFn::Split => "string.split(separator: RegExp): string[]",
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
    pub const ALL: [StrFn; 23] = [
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
            StrFn::Concat => "subscript_rt_str_concat",
        }
    }

    /// Parameter spellings after the receiver, post-normalization (the
    /// checker has already supplied the defaulted `from`/`pad`).
    #[must_use]
    pub fn params(self) -> &'static [StrParam] {
        match self {
            StrFn::Slice | StrFn::Substring | StrFn::Substr => &[StrParam::I32, StrParam::I32],
            StrFn::IndexOf | StrFn::Includes | StrFn::StartsWith | StrFn::EndsWith => {
                &[StrParam::Str, StrParam::I32]
            }
            StrFn::LastIndexOf | StrFn::Split | StrFn::Concat => &[StrParam::Str],
            StrFn::CharCodeAt | StrFn::Repeat | StrFn::CharAt | StrFn::CodePointAt => {
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
            StrFn::LastIndexOf => "lastIndexOf(needle: string): i32",
            StrFn::Includes => "includes(needle: string, from?: i32): boolean",
            StrFn::StartsWith => "startsWith(needle: string, position?: i32): boolean",
            StrFn::EndsWith => "endsWith(needle: string, endPosition?: i32): boolean",
            StrFn::CharCodeAt => "charCodeAt(index: i32): i32",
            StrFn::Split => "split(separator: string): string[]",
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
            StrFn::Split => "Splits on a literal non-empty string separator.",
            StrFn::Trim => "Removes ECMA whitespace from both ends.",
            StrFn::TrimStart => "Removes ECMA whitespace from the start.",
            StrFn::TrimEnd => "Removes ECMA whitespace from the end.",
            StrFn::Repeat => "Repeats the UTF-8 byte string.",
            StrFn::PadStart => "Pads to a byte length on the left.",
            StrFn::PadEnd => "Pads to a byte length on the right.",
            StrFn::ToUpperCase => "Applies Unicode Default Case Conversion.",
            StrFn::ToLowerCase => "Applies Unicode Default Case Conversion.",
            StrFn::Replace => "Replaces the first literal match with ECMA `$` substitutions.",
            StrFn::ReplaceAll => {
                "Replaces every literal match with ECMA `$` substitutions; an empty pattern traps."
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
            StrFn::Concat => "Returns a fresh concatenation with exactly one other string.",
        }
    }
}

/// `Array` intrinsic methods (stdlib.md §9, Q22): the accepted subset
/// on `T[]`, lowered by both tiers to opaque `subscript_rt_arr_*` runtime
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
    /// `includes(x)` — per-kind SameValueZero equality (so float NaNs
    /// are found, unlike `indexOf`/`lastIndexOf`, Q22).
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
    /// `forEach(f)` — `f: (v: T) => void` or
    /// `f: (v: T, i: i32) => void`.
    ForEach,
    /// `map(f)` — `f: (v: T) => U` or
    /// `f: (v: T, i: i32) => U`; `U` inferred from the callback.
    Map,
    /// `filter(f)` — `f: (v: T) => boolean` or
    /// `f: (v: T, i: i32) => boolean`; fresh array.
    Filter,
    /// `reduce(f, init)` — `f: (acc: U, v: T) => U` or
    /// `f: (acc: U, v: T, i: i32) => U`; `init` required (Q22). The
    /// accumulator travels by pointer (in/out).
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
    /// `reduceRight(f, init)` — `reduce` from right to left; `init` is
    /// required (Q27). The accumulator travels by pointer (in/out).
    ReduceRight,
    /// `splice(start, deleteCount)` — delete-only; returns the removed
    /// elements as a fresh array and mutates the receiver in place.
    Splice,
    /// `shift()` — removes and returns the first element; an empty
    /// receiver traps.
    Shift,
    /// `unshift(x)` — prepends exactly one element and returns the new
    /// length.
    Unshift,
    /// `copyWithin(target, start, end)` — JS negative/clamp rules; in
    /// place; the expression's value is the receiver.
    CopyWithin,
}

impl ArrFn {
    /// Every accepted `Array` method, in declaration order; the index
    /// of each variant equals its discriminant, so `f as usize` indexes
    /// tables built from this list.
    pub const ALL: [ArrFn; 21] = [
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
        ArrFn::ReduceRight,
        ArrFn::Splice,
        ArrFn::Shift,
        ArrFn::Unshift,
        ArrFn::CopyWithin,
    ];

    /// The checker's spelling of a defaulted missing `end` argument of
    /// `slice`/`fill`/`copyWithin`: `i32::MAX`, which the runtime's JS clamp reduces
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
            ArrFn::ReduceRight => "reduceRight",
            ArrFn::Splice => "splice",
            ArrFn::Shift => "shift",
            ArrFn::Unshift => "unshift",
            ArrFn::CopyWithin => "copyWithin",
        }
    }

    /// The opaque runtime symbol both tiers call.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            ArrFn::IndexOf => "subscript_rt_arr_index_of",
            ArrFn::LastIndexOf => "subscript_rt_arr_last_index_of",
            ArrFn::Includes => "subscript_rt_arr_includes",
            ArrFn::Join => "subscript_rt_arr_join",
            ArrFn::Slice => "subscript_rt_arr_slice",
            ArrFn::Fill => "subscript_rt_arr_fill",
            ArrFn::Reverse => "subscript_rt_arr_reverse",
            ArrFn::Concat => "subscript_rt_arr_concat",
            ArrFn::ForEach => "subscript_rt_arr_for_each",
            ArrFn::Map => "subscript_rt_arr_map",
            ArrFn::Filter => "subscript_rt_arr_filter",
            ArrFn::Reduce => "subscript_rt_arr_reduce",
            ArrFn::Some => "subscript_rt_arr_some",
            ArrFn::Every => "subscript_rt_arr_every",
            ArrFn::FindIndex => "subscript_rt_arr_find_index",
            ArrFn::Sort => "subscript_rt_arr_sort",
            ArrFn::ReduceRight => "subscript_rt_arr_reduce_right",
            ArrFn::Splice => "subscript_rt_arr_splice",
            ArrFn::Shift => "subscript_rt_arr_shift",
            ArrFn::Unshift => "subscript_rt_arr_unshift",
            ArrFn::CopyWithin => "subscript_rt_arr_copy_within",
        }
    }

    /// The Q27 `FixedArray<T, N>` callback-family runtime symbol, when
    /// this operation is accepted on an in-place fixed buffer.
    #[must_use]
    pub fn fixed_symbol(self) -> Option<&'static str> {
        Some(match self {
            ArrFn::ForEach => "subscript_rt_fixed_arr_for_each",
            ArrFn::Map => "subscript_rt_fixed_arr_map",
            ArrFn::Filter => "subscript_rt_fixed_arr_filter",
            ArrFn::Reduce => "subscript_rt_fixed_arr_reduce",
            ArrFn::Some => "subscript_rt_fixed_arr_some",
            ArrFn::Every => "subscript_rt_fixed_arr_every",
            ArrFn::FindIndex => "subscript_rt_fixed_arr_find_index",
            ArrFn::ReduceRight => "subscript_rt_fixed_arr_reduce_right",
            _ => return None,
        })
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
                | ArrFn::ReduceRight
                | ArrFn::Some
                | ArrFn::Every
                | ArrFn::FindIndex
                | ArrFn::Sort
        )
    }

    /// Callback arity for the accepted form that includes the trailing
    /// element index, or `None` when this operation has no index callback
    /// form. `sort` is deliberately excluded: JavaScript calls its
    /// comparator with only the two compared values.
    #[must_use]
    pub fn callback_index_arity(self) -> Option<usize> {
        match self {
            ArrFn::ForEach
            | ArrFn::Map
            | ArrFn::Filter
            | ArrFn::Some
            | ArrFn::Every
            | ArrFn::FindIndex => Some(2),
            ArrFn::Reduce | ArrFn::ReduceRight => Some(3),
            _ => None,
        }
    }

    /// Whether the runtime symbol takes a trailing `pos_id`: the
    /// operations that allocate through the Context (a fresh array or
    /// string), plus `shift`, whose empty-receiver trap is at the call
    /// site.
    /// The callback-taking non-allocating operations surface only
    /// *callback* traps, which carry their own position.
    #[must_use]
    pub fn takes_pos_id(self) -> bool {
        matches!(
            self,
            ArrFn::Join
                | ArrFn::Slice
                | ArrFn::Concat
                | ArrFn::Map
                | ArrFn::Filter
                | ArrFn::Splice
                | ArrFn::Shift
                | ArrFn::Unshift
        )
    }

    /// Whether the generated call must be followed by a trap check:
    /// every operation that can leave the Context trapped (an
    /// allocation failure, or a script callback that trapped).
    #[must_use]
    pub fn can_trap(self) -> bool {
        self.takes_callback() || self.takes_pos_id()
    }

    /// Source-level generic subscript signature.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            ArrFn::IndexOf => "indexOf(value: T): i32",
            ArrFn::LastIndexOf => "lastIndexOf(value: T): i32",
            ArrFn::Includes => "includes(value: T): boolean",
            ArrFn::Join => "join(separator?: string): string",
            ArrFn::Slice => "slice(start?: i32, end?: i32): T[]",
            ArrFn::Fill => "fill(value: T, start?: i32, end?: i32): T[]",
            ArrFn::Reverse => "reverse(): T[]",
            ArrFn::Concat => "concat(other: T[]): T[]",
            ArrFn::ForEach => "forEach(callback: ((value: T) => void) | ((value: T, index: i32) => void)): void",
            ArrFn::Map => "map<U>(callback: ((value: T) => U) | ((value: T, index: i32) => U)): U[]",
            ArrFn::Filter => "filter(callback: ((value: T) => boolean) | ((value: T, index: i32) => boolean)): T[]",
            ArrFn::Reduce => "reduce<U>(callback: ((acc: U, value: T) => U) | ((acc: U, value: T, index: i32) => U), init: U): U",
            ArrFn::Some => "some(callback: ((value: T) => boolean) | ((value: T, index: i32) => boolean)): boolean",
            ArrFn::Every => "every(callback: ((value: T) => boolean) | ((value: T, index: i32) => boolean)): boolean",
            ArrFn::FindIndex => "findIndex(callback: ((value: T) => boolean) | ((value: T, index: i32) => boolean)): i32",
            ArrFn::Sort => "sort(comparator: (left: T, right: T) => i32): T[]",
            ArrFn::ReduceRight => "reduceRight<U>(callback: ((acc: U, value: T) => U) | ((acc: U, value: T, index: i32) => U), init: U): U",
            ArrFn::Splice => "splice(start: i32, deleteCount: i32): T[]",
            ArrFn::Shift => "shift(): T",
            ArrFn::Unshift => "unshift(value: T): i32",
            ArrFn::CopyWithin => "copyWithin(target: i32, start: i32, end?: i32): T[]",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            ArrFn::IndexOf => "Returns the first `===`-equal element index, or -1.",
            ArrFn::LastIndexOf => "Returns the last `===`-equal element index, or -1.",
            ArrFn::Includes => "Uses SameValueZero equality.",
            ArrFn::Join => "Formats elements with the language interpolation rules.",
            ArrFn::Slice => "Returns a fresh range using JS clamp and negative-index rules.",
            ArrFn::Fill => "Stores one value across a range and returns the receiver.",
            ArrFn::Reverse => "Reverses in place and returns the receiver.",
            ArrFn::Concat => "Returns a fresh array from exactly one other array.",
            ArrFn::ForEach => "Calls a non-escaping callback with a value and optional index.",
            ArrFn::Map => "Maps through a non-escaping callback and infers `U`.",
            ArrFn::Filter => "Returns elements selected by a non-escaping callback.",
            ArrFn::Reduce => "Folds from a required initial accumulator.",
            ArrFn::Some => "Short-circuits on the first true callback result.",
            ArrFn::Every => "Short-circuits on the first false callback result.",
            ArrFn::FindIndex => "Returns the first matching callback index, or -1.",
            ArrFn::Sort => "Stable-sorts in place with a required comparator.",
            ArrFn::ReduceRight => "Folds right-to-left from a required initial accumulator.",
            ArrFn::Splice => "Deletes a clamped range in place and returns the removed elements.",
            ArrFn::Shift => "Removes the first element; an empty array traps.",
            ArrFn::Unshift => "Prepends one element and returns the new length.",
            ArrFn::CopyWithin => {
                "Copies a clamped range within the receiver and returns the receiver."
            }
        }
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
    /// `Map.groupBy(items, f)`.
    GroupBy,
}

impl MapFn {
    /// Every accepted operation, in discriminant order.
    pub const ALL: [MapFn; 10] = [
        MapFn::New,
        MapFn::Size,
        MapFn::Get,
        MapFn::GetOr,
        MapFn::Set,
        MapFn::Has,
        MapFn::Delete,
        MapFn::Clear,
        MapFn::ForEach,
        MapFn::GroupBy,
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
            MapFn::GroupBy => "groupBy",
        }
    }

    /// Opaque runtime symbol.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            MapFn::New => "subscript_rt_map_new",
            MapFn::Size => "subscript_rt_assoc_size",
            MapFn::Get => "subscript_rt_map_get",
            MapFn::GetOr => "subscript_rt_map_get_or",
            MapFn::Set => "subscript_rt_map_set",
            MapFn::Has => "subscript_rt_assoc_has",
            MapFn::Delete => "subscript_rt_assoc_delete",
            MapFn::Clear => "subscript_rt_assoc_clear",
            MapFn::ForEach => "subscript_rt_map_for_each",
            MapFn::GroupBy => "subscript_rt_map_group_by",
        }
    }

    /// True when the operation may allocate Context memory.
    #[must_use]
    pub fn allocates(self) -> bool {
        matches!(self, MapFn::New | MapFn::Set | MapFn::GroupBy)
    }

    /// True when generated code must check the trap flag afterward.
    #[must_use]
    pub fn can_trap(self) -> bool {
        self.allocates() || matches!(self, MapFn::ForEach | MapFn::GroupBy)
    }

    /// Source-level generic subscript signature.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            MapFn::New => "new Map<K, V>(): Map<K, V>",
            MapFn::Size => "size: i32",
            MapFn::Get => "get(key: K): V | null",
            MapFn::GetOr => "getOr(key: K, fallback: V): V",
            MapFn::Set => "set(key: K, value: V): Map<K, V>",
            MapFn::Has => "has(key: K): boolean",
            MapFn::Delete => "delete(key: K): boolean",
            MapFn::Clear => "clear(): void",
            MapFn::ForEach => "forEach(callback: (value: V, key: K) => void): void",
            MapFn::GroupBy => "groupBy<K, T>(items: T[], callback: (value: T) => K): Map<K, T[]>",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            MapFn::New => "Constructs an empty insertion-ordered map.",
            MapFn::Size => "Returns the entry count as `i32`.",
            MapFn::Get => "Returns a nullable reference value; scalar values must use `getOr`.",
            MapFn::GetOr => "Returns the stored value or the explicit fallback.",
            MapFn::Set => "Stores a value and returns the receiver.",
            MapFn::Has => "Tests key presence with the key kind's equality.",
            MapFn::Delete => "Deletes a key and reports whether it was present.",
            MapFn::Clear => "Removes every entry.",
            MapFn::ForEach => "Traverses in insertion order with a fixed two-parameter callback.",
            MapFn::GroupBy => "Groups array values under whitelisted keys in first-seen key order.",
        }
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
    /// `union(other)`.
    Union,
    /// `intersection(other)`.
    Intersection,
    /// `difference(other)`.
    Difference,
    /// `symmetricDifference(other)`.
    SymmetricDifference,
    /// `isSubsetOf(other)`.
    IsSubsetOf,
    /// `isSupersetOf(other)`.
    IsSupersetOf,
    /// `isDisjointFrom(other)`.
    IsDisjointFrom,
}

impl SetFn {
    /// Every accepted operation, in discriminant order.
    pub const ALL: [SetFn; 14] = [
        SetFn::New,
        SetFn::Size,
        SetFn::Add,
        SetFn::Has,
        SetFn::Delete,
        SetFn::Clear,
        SetFn::ForEach,
        SetFn::Union,
        SetFn::Intersection,
        SetFn::Difference,
        SetFn::SymmetricDifference,
        SetFn::IsSubsetOf,
        SetFn::IsSupersetOf,
        SetFn::IsDisjointFrom,
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
            SetFn::Union => "union",
            SetFn::Intersection => "intersection",
            SetFn::Difference => "difference",
            SetFn::SymmetricDifference => "symmetricDifference",
            SetFn::IsSubsetOf => "isSubsetOf",
            SetFn::IsSupersetOf => "isSupersetOf",
            SetFn::IsDisjointFrom => "isDisjointFrom",
        }
    }

    /// Opaque runtime symbol.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            SetFn::New => "subscript_rt_set_new",
            SetFn::Size => "subscript_rt_assoc_size",
            SetFn::Add => "subscript_rt_set_add",
            SetFn::Has => "subscript_rt_assoc_has",
            SetFn::Delete => "subscript_rt_assoc_delete",
            SetFn::Clear => "subscript_rt_assoc_clear",
            SetFn::ForEach => "subscript_rt_set_for_each",
            SetFn::Union => "subscript_rt_set_union",
            SetFn::Intersection => "subscript_rt_set_intersection",
            SetFn::Difference => "subscript_rt_set_difference",
            SetFn::SymmetricDifference => "subscript_rt_set_symmetric_difference",
            SetFn::IsSubsetOf => "subscript_rt_set_is_subset_of",
            SetFn::IsSupersetOf => "subscript_rt_set_is_superset_of",
            SetFn::IsDisjointFrom => "subscript_rt_set_is_disjoint_from",
        }
    }

    /// True when the operation may allocate Context memory.
    #[must_use]
    pub fn allocates(self) -> bool {
        matches!(
            self,
            SetFn::New
                | SetFn::Add
                | SetFn::Union
                | SetFn::Intersection
                | SetFn::Difference
                | SetFn::SymmetricDifference
        )
    }

    /// True when generated code must check the trap flag afterward.
    #[must_use]
    pub fn can_trap(self) -> bool {
        self.allocates() || self == SetFn::ForEach
    }

    /// Source-level generic subscript signature.
    #[must_use]
    pub(crate) fn api_signature(self) -> &'static str {
        match self {
            SetFn::New => "new Set<K>(): Set<K>",
            SetFn::Size => "size: i32",
            SetFn::Add => "add(key: K): Set<K>",
            SetFn::Has => "has(key: K): boolean",
            SetFn::Delete => "delete(key: K): boolean",
            SetFn::Clear => "clear(): void",
            SetFn::ForEach => "forEach(callback: (key: K) => void): void",
            SetFn::Union => "union(other: Set<K>): Set<K>",
            SetFn::Intersection => "intersection(other: Set<K>): Set<K>",
            SetFn::Difference => "difference(other: Set<K>): Set<K>",
            SetFn::SymmetricDifference => "symmetricDifference(other: Set<K>): Set<K>",
            SetFn::IsSubsetOf => "isSubsetOf(other: Set<K>): boolean",
            SetFn::IsSupersetOf => "isSupersetOf(other: Set<K>): boolean",
            SetFn::IsDisjointFrom => "isDisjointFrom(other: Set<K>): boolean",
        }
    }

    /// API-reference summary.
    #[must_use]
    pub(crate) fn api_summary(self) -> &'static str {
        match self {
            SetFn::New => "Constructs an empty insertion-ordered set.",
            SetFn::Size => "Returns the entry count as `i32`.",
            SetFn::Add => "Adds a key and returns the receiver.",
            SetFn::Has => "Tests key presence with the key kind's equality.",
            SetFn::Delete => "Deletes a key and reports whether it was present.",
            SetFn::Clear => "Removes every entry.",
            SetFn::ForEach => "Traverses in insertion order with a fixed one-parameter callback.",
            SetFn::Union => "Returns a fresh union in ES2024 result order.",
            SetFn::Intersection => "Returns a fresh intersection in ES2024 result order.",
            SetFn::Difference => "Returns a fresh receiver-minus-argument set.",
            SetFn::SymmetricDifference => {
                "Returns a fresh symmetric difference in receiver-then-argument order."
            }
            SetFn::IsSubsetOf => "Tests whether every receiver key is in the argument.",
            SetFn::IsSupersetOf => "Tests whether every argument key is in the receiver.",
            SetFn::IsDisjointFrom => "Tests whether the sets have no common key.",
        }
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
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Enum(_) | Type::Date => {
                ArrElemKind::SignedInt
            }
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
pub enum Callee {
    /// A module function by (possibly monomorphized) name.
    Func(String),
    /// A foreign C-ABI function declared by an ambient mirror (P5.2);
    /// carries the symbol name. No lowering path yet (P5.2b).
    Foreign(String),
    /// An ambient prelude function.
    Ambient(AmbientFn),
    /// A typed Context storage-byte operation and its concrete storage type.
    ContextBytes {
        /// The storage-byte operation.
        function: ContextBytesFn,
        /// The explicit concrete type argument.
        ty: Type,
    },
    /// A `Math.<fn>` ambient-namespace intrinsic (stdlib.md §1).
    Math(MathFn),
    /// A `Number` or parsing intrinsic (stdlib.md §11, Q25/Q26).
    /// Receiver methods carry their receiver as the first argument.
    Num(NumFn),
    /// A `Date` intrinsic (stdlib.md §3): `new Date(ms)`, the `Date.UTC`
    /// / `Date.now` statics, the UTC accessors, and `toISOString`. For
    /// the instance operations the receiver is the first argument.
    Date(DateFn),
    /// One internal leaf of a checker-generated `JSON.stringify<T>` or
    /// `JSON.parse<T>` helper graph (stdlib.md §13, Q28).
    Json(JsonFn),
    /// A `String` method intrinsic (stdlib.md §8, Q21). The receiver is
    /// the first argument; optional arguments were normalized at check
    /// time, so the arity is `1 + f.params().len()` exactly.
    Str(StrFn),
    /// A regular-expression intrinsic (stdlib.md §15, Q31).
    Regex(RegexFn),
    /// An `Array` method intrinsic (stdlib.md §9, Q22). The receiver is
    /// the first argument; optional arguments were normalized at check
    /// time (`join` separator, `slice`/`fill` range). For `reduce` the
    /// argument order is `[receiver, callback, init]`.
    Arr(ArrFn),
    /// A `Map<K, V>` operation intrinsic (stdlib.md §10, Q24).
    Map(MapFn),
    /// A `Set<K>` operation intrinsic (stdlib.md §10, Q24).
    Set(SetFn),
    /// A Q35 worker or channel-endpoint intrinsic.
    Worker(WorkerFn),
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

impl Callee {
    /// Whether this callee contributes a `TrapSite::Call`.
    ///
    /// Kept private so `Expr::trap_sites` is the only backend-visible
    /// answer to which checks an operation carries.
    fn has_call_site(&self) -> bool {
        match self {
            Callee::Func(_) | Callee::Value(_) | Callee::Method { .. } | Callee::Foreign(_) => true,
            Callee::Ambient(f) => f.can_trap(),
            Callee::ContextBytes { .. } => true,
            Callee::Math(f) => f.can_trap(),
            Callee::Num(f) => f.takes_pos_id(),
            Callee::Date(f) => f.can_trap(),
            Callee::Json(f) => f.can_trap(),
            Callee::Str(f) => f.takes_pos_id(),
            Callee::Regex(f) => f.can_trap(),
            Callee::Arr(f) => f.can_trap(),
            Callee::Map(f) => f.can_trap(),
            Callee::Set(f) => f.can_trap(),
            Callee::Worker(_) => true,
        }
    }
}

/// Q35 worker/channel operations lowered onto the runtime worker C API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WorkerFn {
    /// `Worker.spawn(entry)`; indexes [`Module::worker_entries`].
    Spawn(usize),
    /// Parent-side `Worker.post(message)`.
    Post,
    /// Parent-side non-blocking `Worker.poll()`.
    Poll,
    /// Parent-side `Worker.close()`.
    Close,
    /// Parent-side blocking `Worker.join()`.
    Join,
    /// Worker-side blocking `Inbox.wait()`.
    InboxWait,
    /// Worker-side non-blocking `Inbox.poll()`.
    InboxPoll,
    /// Worker-side `Outbox.post(message)`.
    OutboxPost,
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
    /// A test of a string-alias value against its reserved absence marker.
    AbsenceTest {
        /// The absence-capable value.
        value: Box<Expr>,
        /// True for `!== undefined`; false for `=== undefined`.
        negated: bool,
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
    /// Q33 descriptor construction from a contextually typed object
    /// literal. `fields` is in class declaration order; `None` means the
    /// declared default is evaluated for this construction.
    DescriptorLit {
        /// Constructed descriptor class.
        class: ClassId,
        /// Explicit values or omitted-default markers, one per field.
        fields: Vec<Option<Expr>>,
    },
    /// Checker-internal zero value used by typed JSON.parse construction.
    Zero,
    /// Checker-internal raw allocation of a reference class, bypassing
    /// field initializers and its source constructor.
    RawNew {
        /// Allocated reference class.
        class: ClassId,
    },
    /// Field access `obj.name` (classes and the `IterResult` shape).
    Field {
        /// Receiver.
        obj: Box<Expr>,
        /// Field name.
        name: String,
    },
    /// Checked read of `JsonResult<T>.value`. Both backends guard the
    /// ordinary field load with the sibling `ok` field.
    JsonResultValue(Box<Expr>),
    /// `length` of an array, `FixedArray`, or string.
    Length(Box<Expr>),
    /// Index access `obj[i]`.
    Index {
        /// Indexed array.
        obj: Box<Expr>,
        /// Index expression (`i32`).
        index: Box<Expr>,
        /// Whether HIR's shared interval pass retained the bounds check.
        ///
        /// Dynamic arrays always retain it. A `FixedArray` access may set
        /// this false only when the index is proven in range.
        checked: bool,
    },
    /// Array literal; the expression type says whether it constructs a
    /// dynamic array or a `FixedArray` (Q3).
    ArrayLit(Vec<Expr>),
    /// Dynamic array literal containing at least one spread operand
    /// (stdlib.md §14.4). The result is always a fresh `T[]`.
    ArraySpreadLit(Vec<ArrayLitElem>),
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
        /// Captured `const` locals and their resolved storage types,
        /// empty when non-capturing.
        captures: Vec<Capture>,
    },
    /// `yield` inside a generator (C8).
    Yield(Option<Box<Expr>>),
    /// `await Context.suspend()` inside an async function (Q34).
    AsyncSuspend,
    /// A direct async call in await position (Q34/R13). The result type is
    /// the callee's fulfilled value type; no Promise value exists in HIR.
    AsyncCall {
        /// Direct free-function or reference-class method target.
        callee: AsyncCallee,
        /// Explicit arguments evaluated after a method receiver and before
        /// the callee frame is created.
        args: Vec<Expr>,
    },
    /// Creates a held async frame handle without polling it (§70).
    AsyncHandleCreate {
        /// Direct free-function or reference-class method target.
        callee: AsyncCallee,
        /// Explicit arguments evaluated before the callee frame is created.
        args: Vec<Expr>,
        /// Checker-local obligation joined through copies and storage.
        origin: u32,
    },
    /// Polls a previously created async handle and yields its cached result.
    AsyncHandleAwait(Box<Expr>),
    /// Transfers a held async handle through a synchronous call boundary,
    /// carrying the caller's must-await obligation.
    AsyncHandleTransfer {
        /// The synchronously returned handle or handle array.
        value: Box<Expr>,
        /// Function-local must-await obligation identity.
        origin: u32,
    },
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

impl ExprKind {
    /// Reports whether this expression kind can produce a fresh async owner.
    pub fn produces_fresh_async_owner(&self) -> bool {
        match self {
            Self::AsyncHandleCreate { .. }
            | Self::AsyncHandleTransfer { .. }
            | Self::Call { .. }
            | Self::ArrayLit(_)
            | Self::ArraySpreadLit(_) => true,
            Self::Cond { then, els, .. } => {
                then.kind.produces_fresh_async_owner() && els.kind.produces_fresh_async_owner()
            }
            _ => false,
        }
    }
}

/// Target of a direct async call in await position (Q34/R13).
///
/// Keeping the method receiver inside the target makes its source-order
/// relationship to the explicit arguments structural: it is evaluated once,
/// before `ExprKind::AsyncCall::args`, and becomes the first payload slot of
/// the callee frame.
#[derive(Debug, Clone, PartialEq)]
pub enum AsyncCallee {
    /// A module async function by HIR name.
    Function(String),
    /// An async instance method on a plain reference class.
    Method {
        /// Declaring/receiver class.
        class: ClassId,
        /// Receiver expression, evaluated exactly once before arguments.
        receiver: Box<Expr>,
        /// Method name within `class`.
        name: String,
    },
}

impl AsyncCallee {
    /// Returns the receiver of an async method target, if this is one.
    #[must_use]
    pub fn receiver(&self) -> Option<&Expr> {
        match self {
            Self::Function(_) => None,
            Self::Method { receiver, .. } => Some(receiver),
        }
    }

    /// Returns the receiver mutably for HIR analysis/rewriting passes.
    #[must_use]
    pub(crate) fn receiver_mut(&mut self) -> Option<&mut Expr> {
        match self {
            Self::Function(_) => None,
            Self::Method { receiver, .. } => Some(receiver),
        }
    }
}

/// One element of a dynamic array literal containing spread.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ArrayLitElem {
    /// Checked value or container operand.
    pub expr: Expr,
    /// `None` for an ordinary element; otherwise the fused storage
    /// traversal used to append this operand.
    pub spread: Option<SpreadKind>,
}

/// Closed set of array-literal spread traversals (stdlib.md §14.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpreadKind {
    /// Dynamic-array values.
    Array,
    /// Fixed-array values.
    FixedArray,
    /// Map keys (the language's bare-Map iteration value).
    MapKeys,
    /// Set values.
    SetValues,
    /// String code points.
    StringCodePoints,
}

impl From<IterKind> for SpreadKind {
    fn from(kind: IterKind) -> Self {
        match kind {
            IterKind::Array => Self::Array,
            IterKind::FixedArray => Self::FixedArray,
            IterKind::MapKeys => Self::MapKeys,
            IterKind::SetValues => Self::SetValues,
            IterKind::StringCodePoints => Self::StringCodePoints,
        }
    }
}

impl From<IterKind> for ForOfKind {
    fn from(kind: IterKind) -> Self {
        match kind {
            IterKind::Array => Self::ArrayValues,
            IterKind::FixedArray => Self::FixedArrayValues,
            IterKind::MapKeys => Self::MapKeys,
            IterKind::SetValues => Self::SetValues,
            IterKind::StringCodePoints => Self::StringCodePoints,
        }
    }
}

impl Expr {
    /// Returns the leaves that can flow into this expression's value.
    pub fn flow_leaves(&self) -> impl Iterator<Item = &Expr> {
        fn collect<'a>(expression: &'a Expr, leaves: &mut Vec<&'a Expr>) {
            match &expression.kind {
                ExprKind::Cast(inner) | ExprKind::Assign { value: inner, .. } => {
                    collect(inner, leaves);
                }
                ExprKind::Cond { then, els, .. } => {
                    collect(then, leaves);
                    collect(els, leaves);
                }
                ExprKind::ArrayLit(elements) => {
                    for element in elements {
                        collect(element, leaves);
                    }
                }
                _ => leaves.push(expression),
            }
        }

        let mut leaves = Vec::new();
        collect(self, &mut leaves);
        leaves.into_iter()
    }

    /// Returns every immediate expression or statement child in source order.
    pub fn children(&self) -> Vec<HirChild<'_>> {
        use ExprKind as K;

        match &self.kind {
            K::Unary { operand, .. }
            | K::Cast(operand)
            | K::JsonResultValue(operand)
            | K::Length(operand)
            | K::AsyncHandleAwait(operand)
            | K::AsyncHandleTransfer { value: operand, .. } => {
                vec![HirChild::Expr(operand)]
            }
            K::AbsenceTest { value, .. } => vec![HirChild::Expr(value)],
            K::Binary { left, right, .. }
            | K::Assign {
                target: left,
                value: right,
                ..
            } => vec![HirChild::Expr(left), HirChild::Expr(right)],
            K::Call { callee, args } => {
                let mut children = Vec::with_capacity(args.len() + 1);
                match callee {
                    Callee::Value(value) => children.push(HirChild::Expr(value)),
                    Callee::Method { recv, .. } => children.push(HirChild::Expr(recv)),
                    _ => {}
                }
                children.extend(args.iter().map(HirChild::Expr));
                children
            }
            K::New { args, .. } => args.iter().map(HirChild::Expr).collect(),
            K::DescriptorLit { fields, .. } => {
                fields.iter().flatten().map(HirChild::Expr).collect()
            }
            K::Field { obj, .. } => vec![HirChild::Expr(obj)],
            K::Index { obj, index, .. } => {
                vec![HirChild::Expr(obj), HirChild::Expr(index)]
            }
            K::ArrayLit(elements) => elements.iter().map(HirChild::Expr).collect(),
            K::ArraySpreadLit(elements) => elements
                .iter()
                .map(|element| HirChild::Expr(&element.expr))
                .collect(),
            K::Template(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    TplPart::Expr(expr) => Some(HirChild::Expr(expr)),
                    TplPart::Text(_) => None,
                })
                .collect(),
            K::Lambda { body, .. } => body.iter().map(HirChild::Stmt).collect(),
            K::Yield(Some(value)) => vec![HirChild::Expr(value)],
            K::AsyncCall { callee, args } | K::AsyncHandleCreate { callee, args, .. } => {
                let mut children = Vec::with_capacity(args.len() + 1);
                if let Some(receiver) = callee.receiver() {
                    children.push(HirChild::Expr(receiver));
                }
                children.extend(args.iter().map(HirChild::Expr));
                children
            }
            K::Cond { cond, then, els } => vec![
                HirChild::Expr(cond),
                HirChild::Expr(then),
                HirChild::Expr(els),
            ],
            K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Str(_)
            | K::Null
            | K::This
            | K::Local(_)
            | K::Global(_)
            | K::FuncRef(_)
            | K::EnumMember { .. }
            | K::Zero
            | K::RawNew { .. }
            | K::Yield(None)
            | K::AsyncSuspend => Vec::new(),
        }
    }

    /// Returns every mutable immediate child in source order.
    pub(crate) fn children_mut(&mut self) -> Vec<HirChildMut<'_>> {
        use ExprKind as K;

        match &mut self.kind {
            K::Unary { operand, .. }
            | K::Cast(operand)
            | K::JsonResultValue(operand)
            | K::Length(operand)
            | K::AsyncHandleAwait(operand)
            | K::AsyncHandleTransfer { value: operand, .. } => {
                vec![HirChildMut::Expr(operand)]
            }
            K::AbsenceTest { value, .. } => vec![HirChildMut::Expr(value)],
            K::Binary { left, right, .. }
            | K::Assign {
                target: left,
                value: right,
                ..
            } => vec![HirChildMut::Expr(left), HirChildMut::Expr(right)],
            K::Call { callee, args } => {
                let mut children = Vec::with_capacity(args.len() + 1);
                match callee {
                    Callee::Value(value) => children.push(HirChildMut::Expr(value)),
                    Callee::Method { recv, .. } => children.push(HirChildMut::Expr(recv)),
                    _ => {}
                }
                children.extend(args.iter_mut().map(HirChildMut::Expr));
                children
            }
            K::New { args, .. } => args.iter_mut().map(HirChildMut::Expr).collect(),
            K::DescriptorLit { fields, .. } => {
                fields.iter_mut().flatten().map(HirChildMut::Expr).collect()
            }
            K::Field { obj, .. } => vec![HirChildMut::Expr(obj)],
            K::Index { obj, index, .. } => {
                vec![HirChildMut::Expr(obj), HirChildMut::Expr(index)]
            }
            K::ArrayLit(elements) => elements.iter_mut().map(HirChildMut::Expr).collect(),
            K::ArraySpreadLit(elements) => elements
                .iter_mut()
                .map(|element| HirChildMut::Expr(&mut element.expr))
                .collect(),
            K::Template(parts) => parts
                .iter_mut()
                .filter_map(|part| match part {
                    TplPart::Expr(expr) => Some(HirChildMut::Expr(expr)),
                    TplPart::Text(_) => None,
                })
                .collect(),
            K::Lambda { body, .. } => body.iter_mut().map(HirChildMut::Stmt).collect(),
            K::Yield(Some(value)) => vec![HirChildMut::Expr(value)],
            K::AsyncCall { callee, args } | K::AsyncHandleCreate { callee, args, .. } => {
                let mut children = Vec::with_capacity(args.len() + 1);
                if let Some(receiver) = callee.receiver_mut() {
                    children.push(HirChildMut::Expr(receiver));
                }
                children.extend(args.iter_mut().map(HirChildMut::Expr));
                children
            }
            K::Cond { cond, then, els } => vec![
                HirChildMut::Expr(cond),
                HirChildMut::Expr(then),
                HirChildMut::Expr(els),
            ],
            K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Str(_)
            | K::Null
            | K::This
            | K::Local(_)
            | K::Global(_)
            | K::FuncRef(_)
            | K::EnumMember { .. }
            | K::Zero
            | K::RawNew { .. }
            | K::Yield(None)
            | K::AsyncSuspend => Vec::new(),
        }
    }
}

impl Stmt {
    /// Returns every immediate expression or statement child in source order.
    pub fn children(&self) -> Vec<HirChild<'_>> {
        match self {
            Stmt::Let { init, .. } | Stmt::Expr(init) => vec![HirChild::Expr(init)],
            Stmt::Return { value, .. } => value.iter().map(HirChild::Expr).collect(),
            Stmt::If {
                cond, then, els, ..
            } => {
                let mut children =
                    Vec::with_capacity(1 + then.len() + els.as_ref().map_or(0, Vec::len));
                children.push(HirChild::Expr(cond));
                children.extend(then.iter().map(HirChild::Stmt));
                children.extend(els.iter().flatten().map(HirChild::Stmt));
                children
            }
            Stmt::While { cond, body, .. } => {
                let mut children = Vec::with_capacity(1 + body.len());
                children.push(HirChild::Expr(cond));
                children.extend(body.iter().map(HirChild::Stmt));
                children
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                let mut children = Vec::with_capacity(3 + body.len());
                children.extend(init.iter().map(|stmt| HirChild::Stmt(stmt)));
                children.extend(cond.iter().map(HirChild::Expr));
                children.extend(step.iter().map(HirChild::Expr));
                children.extend(body.iter().map(HirChild::Stmt));
                children
            }
            Stmt::ForOf { subject, body, .. } => {
                let mut children = Vec::with_capacity(1 + body.len());
                children.push(HirChild::Expr(subject));
                children.extend(body.iter().map(HirChild::Stmt));
                children
            }
            Stmt::Switch { disc, cases, .. } => {
                let mut children = Vec::new();
                children.push(HirChild::Expr(disc));
                for case in cases {
                    children.extend(case.test.iter().map(HirChild::Expr));
                    children.extend(case.body.iter().map(HirChild::Stmt));
                }
                children
            }
            Stmt::Block(body) => body.iter().map(HirChild::Stmt).collect(),
            Stmt::Break(_) | Stmt::Continue(_) => Vec::new(),
        }
    }

    /// Returns every mutable immediate child in source order.
    pub(crate) fn children_mut(&mut self) -> Vec<HirChildMut<'_>> {
        match self {
            Stmt::Let { init, .. } | Stmt::Expr(init) => vec![HirChildMut::Expr(init)],
            Stmt::Return { value, .. } => value.iter_mut().map(HirChildMut::Expr).collect(),
            Stmt::If {
                cond, then, els, ..
            } => {
                let mut children =
                    Vec::with_capacity(1 + then.len() + els.as_ref().map_or(0, Vec::len));
                children.push(HirChildMut::Expr(cond));
                children.extend(then.iter_mut().map(HirChildMut::Stmt));
                children.extend(els.iter_mut().flatten().map(HirChildMut::Stmt));
                children
            }
            Stmt::While { cond, body, .. } => {
                let mut children = Vec::with_capacity(1 + body.len());
                children.push(HirChildMut::Expr(cond));
                children.extend(body.iter_mut().map(HirChildMut::Stmt));
                children
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                let mut children = Vec::with_capacity(3 + body.len());
                children.extend(init.iter_mut().map(|stmt| HirChildMut::Stmt(stmt)));
                children.extend(cond.iter_mut().map(HirChildMut::Expr));
                children.extend(step.iter_mut().map(HirChildMut::Expr));
                children.extend(body.iter_mut().map(HirChildMut::Stmt));
                children
            }
            Stmt::ForOf { subject, body, .. } => {
                let mut children = Vec::with_capacity(1 + body.len());
                children.push(HirChildMut::Expr(subject));
                children.extend(body.iter_mut().map(HirChildMut::Stmt));
                children
            }
            Stmt::Switch { disc, cases, .. } => {
                let mut children = Vec::new();
                children.push(HirChildMut::Expr(disc));
                for case in cases {
                    children.extend(case.test.iter_mut().map(HirChildMut::Expr));
                    children.extend(case.body.iter_mut().map(HirChildMut::Stmt));
                }
                children
            }
            Stmt::Block(body) => body.iter_mut().map(HirChildMut::Stmt).collect(),
            Stmt::Break(_) | Stmt::Continue(_) => Vec::new(),
        }
    }
}

impl Expr {
    /// The ordered trap sites owned directly by this operation.
    ///
    /// Sites in child expressions are carried by those children and occur
    /// where normal evaluation reaches them. This method is the single HIR
    /// policy used by both lowering tiers; neither backend decides whether
    /// a call, literal, cast, or index operation is checked.
    #[must_use]
    pub fn trap_sites(&self, module: &Module) -> Vec<TrapSite> {
        use BinOp as B;
        use ExprKind as K;

        let allocation = |pos: &Pos| TrapSite::Allocation { pos: pos.clone() };
        let call = |pos: &Pos| TrapSite::Call { pos: pos.clone() };
        let lifetime = |pos: &Pos| TrapSite::DevOnlyLifetime { pos: pos.clone() };
        let handle_classes = module
            .classes
            .iter()
            .map(HandleClass::from)
            .collect::<Vec<_>>();
        // Fact filter: can dereferencing this value observe a freed Context allocation?
        let reference_value = |ty: &Type| {
            ty.handle_kind(&handle_classes)
                .is_some_and(HandleKind::needs_lifetime_trap)
        };
        let boundary_box_store = |stored: &Type, value: &Type| {
            matches!(stored, Type::Nullable(inner)
            if matches!(inner.as_ref(), Type::Class(class)
                if value == &Type::Class(*class)
                    && module.classes.get(class.0).is_some_and(|definition| {
                        definition.is_value && definition.is_boundary
                    })))
        };
        let embedded_header_store = |stored: &Type, value: &Expr| {
            let Type::Nullable(inner) = stored else {
                return false;
            };
            let Type::Class(header) = inner.as_ref() else {
                return false;
            };
            let K::Field { obj, name } = &value.kind else {
                return false;
            };
            let Type::Class(extension) = obj.ty else {
                return false;
            };
            let nullable = Type::Nullable(Box::new(Type::Class(*header)));
            let linked = module.classes.iter().any(|class| {
                class.is_boundary && class.fields.iter().any(|field| field.ty == nullable)
            }) || module.foreign_fns.iter().any(|function| {
                function
                    .params
                    .iter()
                    .any(|parameter| parameter.ty == nullable)
            });
            module
                .classes
                .get(extension.0)
                .filter(|class| class.is_value && class.is_boundary)
                .and_then(|class| class.fields.first())
                .is_some_and(|field| {
                    field.name == *name && field.ty == Type::Class(*header) && linked
                })
        };
        let index_site = |target: &Expr, write: bool| {
            let K::Index { checked, .. } = &target.kind else {
                return None;
            };
            if !*checked {
                return None;
            }
            Some(if write {
                TrapSite::IndexWrite {
                    pos: target.pos.clone(),
                }
            } else {
                TrapSite::IndexRead {
                    pos: target.pos.clone(),
                }
            })
        };

        match &self.kind {
            K::Str(_) => vec![allocation(&self.pos)],
            K::Binary { op, left, .. } if *op == B::Add && left.ty == Type::Str => {
                vec![allocation(&self.pos)]
            }
            K::Binary { op, left, .. } if matches!(op, B::Div | B::Rem) && left.ty.is_integer() => {
                vec![TrapSite::DivisionByZero {
                    pos: self.pos.clone(),
                }]
            }
            K::Assign { op, target, .. } => {
                let mut sites = Vec::new();
                if let K::Field { obj, .. } = &target.kind {
                    if reference_value(&obj.ty) {
                        sites.push(lifetime(&obj.pos));
                    }
                }
                if let K::Index { obj, .. } = &target.kind {
                    if matches!(obj.ty, Type::Array(_)) {
                        sites.push(lifetime(&obj.pos));
                    }
                    if matches!((&obj.ty, op), (Type::Array(_), Some(_)))
                        || matches!((&obj.ty, op), (Type::FixedArray(..), Some(_)))
                    {
                        if let Some(site) = index_site(target, false) {
                            sites.push(site);
                        }
                    }
                }
                if matches!(op, Some(B::Add)) && target.ty == Type::Str {
                    sites.push(allocation(&target.pos));
                } else if matches!(op, Some(B::Div | B::Rem)) && target.ty.is_integer() {
                    sites.push(TrapSite::DivisionByZero {
                        pos: target.pos.clone(),
                    });
                }
                if let K::Index { obj, .. } = &target.kind {
                    if matches!(obj.ty, Type::Array(_)) || op.is_none() {
                        if let Some(site) = index_site(target, true) {
                            sites.push(site);
                        }
                    }
                }
                sites
            }
            K::Cast(inner)
                if matches!(self.ty, Type::Class(_))
                    && (matches!(inner.ty, Type::Object)
                        || matches!(&inner.ty, Type::Nullable(ty) if **ty == Type::Object)) =>
            {
                let Type::Class(class) = self.ty else {
                    unreachable!()
                };
                vec![
                    TrapSite::NullNarrowing {
                        pos: self.pos.clone(),
                    },
                    lifetime(&self.pos),
                    TrapSite::ClassMismatch {
                        class,
                        pos: self.pos.clone(),
                    },
                ]
            }
            K::Call {
                callee: Callee::Ambient(AmbientFn::Unreachable),
                ..
            } => vec![TrapSite::Unreachable {
                pos: self.pos.clone(),
            }],
            K::Call { callee, args } => {
                let mut sites = Vec::new();
                if matches!(callee, Callee::Ambient(AmbientFn::UnsafeDelete)) {
                    sites.push(lifetime(&self.pos));
                    return sites;
                }
                if let Callee::Method { recv, name } = callee {
                    if reference_value(&recv.ty) {
                        sites.push(lifetime(&recv.pos));
                    }
                    if name == "next" && matches!(recv.ty, Type::Generator(_)) {
                        sites.push(TrapSite::DevReloadOnlyStaleCoroutine {
                            pos: self.pos.clone(),
                        });
                    }
                }
                if callee.has_call_site() {
                    sites.push(call(&self.pos));
                }
                let parameter_types = match callee {
                    Callee::Func(name) => module
                        .functions
                        .iter()
                        .find(|function| function.name == *name)
                        .map(|function| {
                            function
                                .params
                                .iter()
                                .map(|parameter| parameter.ty.clone())
                                .collect::<Vec<_>>()
                        }),
                    Callee::Foreign(name) => module
                        .foreign_fns
                        .iter()
                        .find(|function| function.name == *name)
                        .map(|function| {
                            function
                                .params
                                .iter()
                                .map(|parameter| parameter.ty.clone())
                                .collect::<Vec<_>>()
                        }),
                    Callee::Value(value) => match &value.ty {
                        Type::Func(signature) => Some(signature.params.clone()),
                        _ => None,
                    },
                    Callee::Method { recv, name } => match recv.ty {
                        Type::Class(class) => module.classes.get(class.0).and_then(|definition| {
                            definition
                                .methods
                                .iter()
                                .find(|method| method.name == *name)
                                .map(|method| {
                                    method
                                        .params
                                        .iter()
                                        .map(|parameter| parameter.ty.clone())
                                        .collect::<Vec<_>>()
                                })
                        }),
                        _ => None,
                    },
                    _ => None,
                }
                .or_else(|| {
                    let (target, prefix) = match callee {
                        Callee::Ambient(function) => {
                            (OperationSignatureTarget::Ambient(*function), None)
                        }
                        Callee::ContextBytes { function, ty } => (
                            OperationSignatureTarget::ContextBytes(*function, ty.clone()),
                            None,
                        ),
                        Callee::Math(function) => (OperationSignatureTarget::Math(*function), None),
                        Callee::Num(function) => (OperationSignatureTarget::Num(*function), None),
                        Callee::Date(function) => (OperationSignatureTarget::Date(*function), None),
                        Callee::Json(function) => (OperationSignatureTarget::Json(*function), None),
                        Callee::Str(function) => (OperationSignatureTarget::Str(*function), None),
                        Callee::Regex(function) => {
                            (OperationSignatureTarget::Regex(*function), None)
                        }
                        Callee::Arr(function) => (OperationSignatureTarget::Arr(*function), None),
                        Callee::Map(function) => (OperationSignatureTarget::Map(*function), None),
                        Callee::Set(function) => (OperationSignatureTarget::Set(*function), None),
                        Callee::Worker(function) => {
                            (OperationSignatureTarget::Worker(*function), None)
                        }
                        Callee::Method { recv, name } => {
                            let method = match (&recv.ty, name.as_str()) {
                                (Type::Array(_), "push") => BuiltinMethod::ArrayPush,
                                (Type::Array(_), "pop") => BuiltinMethod::ArrayPop,
                                (Type::Str, "slice") => BuiltinMethod::StringSlice,
                                (Type::Generator(_), "next") => BuiltinMethod::GeneratorNext,
                                _ => return None,
                            };
                            (
                                OperationSignatureTarget::BuiltinMethod(method),
                                Some(&recv.ty),
                            )
                        }
                        Callee::Func(_) | Callee::Foreign(_) | Callee::Value(_) => return None,
                    };
                    let prefix_count = usize::from(prefix.is_some());
                    module
                        .operation_signatures
                        .iter()
                        .find(|signature| {
                            signature.target == target
                                && signature.parameter_types.len() == args.len() + prefix_count
                                && prefix.is_none_or(|prefix| {
                                    signature.parameter_types.first() == Some(prefix)
                                })
                                && signature.parameter_types[prefix_count..]
                                    .iter()
                                    .zip(args)
                                    .all(|(parameter, argument)| {
                                        parameter == &argument.ty
                                            || boundary_box_store(parameter, &argument.ty)
                                    })
                        })
                        .map(|signature| signature.parameter_types[prefix_count..].to_vec())
                });
                if let Some(parameter_types) = parameter_types {
                    sites.extend(
                        parameter_types
                            .iter()
                            .zip(args)
                            .filter(|(parameter, argument)| {
                                boundary_box_store(parameter, &argument.ty)
                                    && (!matches!(callee, Callee::Foreign(_))
                                        || embedded_header_store(parameter, argument))
                            })
                            .map(|(_, argument)| allocation(&argument.pos)),
                    );
                }
                if let Callee::Foreign(name) = callee {
                    let wire_alias = module
                        .foreign_fns
                        .iter()
                        .find(|foreign| foreign.name == *name)
                        .and_then(|foreign| match foreign.ret {
                            Type::StringAlias(alias) => Some(alias),
                            _ => None,
                        })
                        .filter(|alias| {
                            module
                                .string_aliases
                                .get(alias.0)
                                .is_some_and(|definition| definition.wire_values.is_some())
                        });
                    if let Some(alias) = wire_alias {
                        sites.push(TrapSite::WireEnumValue {
                            alias,
                            pos: self.pos.clone(),
                        });
                    }
                }
                sites
            }
            K::AsyncCall { callee, .. } | K::AsyncHandleCreate { callee, .. } => {
                let mut sites = Vec::new();
                if let Some(receiver) = callee.receiver() {
                    if reference_value(&receiver.ty) {
                        sites.push(lifetime(&receiver.pos));
                    }
                }
                sites.push(call(&self.pos));
                sites
            }
            K::AsyncHandleAwait(_) => vec![
                TrapSite::DevReloadOnlyStaleCoroutine {
                    pos: self.pos.clone(),
                },
                call(&self.pos),
            ],
            K::New { class, args } => {
                let Some(def) = module.classes.get(class.0) else {
                    return Vec::new();
                };
                let mut sites = Vec::new();
                if !def.is_value {
                    sites.push(allocation(&self.pos));
                }
                sites.extend(
                    def.fields
                        .iter()
                        .zip(args)
                        .filter(|(field, argument)| boundary_box_store(&field.ty, &argument.ty))
                        .map(|(_, argument)| allocation(&argument.pos)),
                );
                if def.ctor.is_some() {
                    sites.push(call(&self.pos));
                }
                sites
            }
            K::DescriptorLit { .. } => vec![allocation(&self.pos)],
            K::RawNew { .. } => vec![allocation(&self.pos)],
            K::Field { obj, .. } => {
                let mut sites = Vec::new();
                if reference_value(&obj.ty) {
                    sites.push(lifetime(&obj.pos));
                }
                if let (Type::StringAlias(alias), Type::Class(class)) = (&self.ty, &obj.ty) {
                    let wire_member = module
                        .classes
                        .get(class.0)
                        .is_some_and(|definition| definition.is_boundary)
                        && module
                            .string_aliases
                            .get(alias.0)
                            .is_some_and(|definition| definition.wire_values.is_some());
                    if wire_member {
                        sites.push(TrapSite::WireEnumValue {
                            alias: *alias,
                            pos: self.pos.clone(),
                        });
                    }
                }
                sites
            }
            K::JsonResultValue(obj) => vec![
                lifetime(&obj.pos),
                TrapSite::JsonResultValue {
                    pos: self.pos.clone(),
                },
            ],
            K::Index { obj, checked, .. } if *checked => {
                let mut sites = Vec::new();
                if matches!(obj.ty, Type::Array(_)) {
                    sites.push(lifetime(&obj.pos));
                }
                sites.push(TrapSite::IndexRead {
                    pos: self.pos.clone(),
                });
                sites
            }
            K::ArrayLit(elems) if matches!(self.ty, Type::Array(_)) => {
                let mut sites = Vec::with_capacity(elems.len() + 1);
                sites.push(allocation(&self.pos));
                sites.extend(elems.iter().map(|elem| allocation(&elem.pos)));
                sites
            }
            K::ArraySpreadLit(elems) => {
                let mut sites = Vec::with_capacity(elems.len() + 1);
                sites.push(allocation(&self.pos));
                sites.extend(elems.iter().map(|elem| allocation(&elem.expr.pos)));
                sites
            }
            K::Template(parts) => {
                if parts.is_empty() {
                    return vec![allocation(&self.pos)];
                }
                let mut sites = Vec::new();
                for (index, part) in parts.iter().enumerate() {
                    match part {
                        TplPart::Text(_) => sites.push(allocation(&self.pos)),
                        TplPart::Expr(expr) if expr.ty != Type::Str => {
                            sites.push(allocation(&expr.pos));
                        }
                        TplPart::Expr(_) => {}
                    }
                    if index != 0 {
                        sites.push(allocation(&self.pos));
                    }
                }
                sites
            }
            K::Cond { then, els, .. } => [then, els]
                .into_iter()
                .filter(|arm| boundary_box_store(&self.ty, &arm.ty))
                .map(|arm| allocation(&arm.pos))
                .collect(),
            K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Null
            | K::This
            | K::Local(_)
            | K::Global(_)
            | K::FuncRef(_)
            | K::EnumMember { .. }
            | K::Zero
            | K::Unary { .. }
            | K::Binary { .. }
            | K::AbsenceTest { .. }
            | K::Cast(_)
            | K::Length(_)
            | K::Index { .. }
            | K::ArrayLit(_)
            | K::Lambda { .. }
            | K::Yield(_)
            | K::AsyncSuspend
            | K::AsyncHandleTransfer { .. } => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Pos;

    fn child_expr(value: i64) -> Expr {
        Expr {
            kind: ExprKind::Int(value),
            ty: Type::I32,
            pos: Pos::new("children.ts", 1, 1),
        }
    }

    fn child_stmt(value: i64) -> Stmt {
        Stmt::Expr(child_expr(value))
    }

    fn child_values(children: Vec<HirChild<'_>>) -> Vec<i64> {
        children
            .into_iter()
            .map(|child| match child {
                HirChild::Expr(Expr {
                    kind: ExprKind::Int(value),
                    ..
                })
                | HirChild::Stmt(Stmt::Expr(Expr {
                    kind: ExprKind::Int(value),
                    ..
                })) => *value,
                other => panic!("unexpected test child {other:?}"),
            })
            .collect()
    }

    #[test]
    fn flow_leaves_follow_only_value_positions() {
        let expression = Expr {
            kind: ExprKind::Cond {
                cond: Box::new(child_expr(0)),
                then: Box::new(Expr {
                    kind: ExprKind::Cast(Box::new(child_expr(1))),
                    ty: Type::I32,
                    pos: Pos::new("flow.ts", 1, 1),
                }),
                els: Box::new(Expr {
                    kind: ExprKind::ArrayLit(vec![child_expr(2), child_expr(3)]),
                    ty: Type::Array(Box::new(Type::I32)),
                    pos: Pos::new("flow.ts", 1, 1),
                }),
            },
            ty: Type::I32,
            pos: Pos::new("flow.ts", 1, 1),
        };
        let values = expression
            .flow_leaves()
            .map(|leaf| match leaf.kind {
                ExprKind::Int(value) => value,
                _ => -1,
            })
            .collect::<Vec<_>>();
        assert_eq!(values, vec![1, 2, 3]);
    }

    fn test_expr(kind: ExprKind) -> Expr {
        Expr {
            kind,
            ty: Type::I32,
            pos: Pos::new("children.ts", 1, 1),
        }
    }

    #[test]
    fn fresh_async_owner_expression_table_keeps_fresh_conditionals() {
        assert!(ExprKind::ArrayLit(Vec::new()).produces_fresh_async_owner());
        assert!(ExprKind::Cond {
            cond: Box::new(child_expr(0)),
            then: Box::new(test_expr(ExprKind::ArrayLit(Vec::new()))),
            els: Box::new(test_expr(ExprKind::ArraySpreadLit(Vec::new()))),
        }
        .produces_fresh_async_owner());
        assert!(!ExprKind::Cond {
            cond: Box::new(child_expr(0)),
            then: Box::new(test_expr(ExprKind::ArrayLit(Vec::new()))),
            els: Box::new(child_expr(1)),
        }
        .produces_fresh_async_owner());
    }

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
    fn expr_children_yield_every_child() {
        let leaf_kinds = vec![
            ExprKind::Int(0),
            ExprKind::Float(0.0),
            ExprKind::Bool(false),
            ExprKind::Str(String::new()),
            ExprKind::Null,
            ExprKind::This,
            ExprKind::Local(String::new()),
            ExprKind::Global(String::new()),
            ExprKind::FuncRef(String::new()),
            ExprKind::EnumMember {
                id: EnumId(0),
                member: String::new(),
                value: 0,
            },
            ExprKind::Zero,
            ExprKind::RawNew { class: ClassId(0) },
            ExprKind::Yield(None),
            ExprKind::AsyncSuspend,
        ];
        for kind in leaf_kinds {
            assert!(test_expr(kind).children().is_empty());
        }

        let cases = vec![
            (
                ExprKind::Unary {
                    op: UnOp::Neg,
                    operand: Box::new(child_expr(1)),
                },
                vec![1],
            ),
            (
                ExprKind::Binary {
                    op: BinOp::Add,
                    left: Box::new(child_expr(1)),
                    right: Box::new(child_expr(2)),
                },
                vec![1, 2],
            ),
            (
                ExprKind::Assign {
                    op: None,
                    target: Box::new(child_expr(1)),
                    value: Box::new(child_expr(2)),
                },
                vec![1, 2],
            ),
            (ExprKind::Cast(Box::new(child_expr(1))), vec![1]),
            (
                ExprKind::Call {
                    callee: Callee::Value(Box::new(child_expr(1))),
                    args: vec![child_expr(2)],
                },
                vec![1, 2],
            ),
            (
                ExprKind::Call {
                    callee: Callee::Method {
                        recv: Box::new(child_expr(1)),
                        name: String::new(),
                    },
                    args: vec![child_expr(2)],
                },
                vec![1, 2],
            ),
            (
                ExprKind::New {
                    class: ClassId(0),
                    args: vec![child_expr(1), child_expr(2)],
                },
                vec![1, 2],
            ),
            (
                ExprKind::DescriptorLit {
                    class: ClassId(0),
                    fields: vec![Some(child_expr(1)), None, Some(child_expr(2))],
                },
                vec![1, 2],
            ),
            (
                ExprKind::Field {
                    obj: Box::new(child_expr(1)),
                    name: String::new(),
                },
                vec![1],
            ),
            (ExprKind::JsonResultValue(Box::new(child_expr(1))), vec![1]),
            (ExprKind::Length(Box::new(child_expr(1))), vec![1]),
            (
                ExprKind::Index {
                    obj: Box::new(child_expr(1)),
                    index: Box::new(child_expr(2)),
                    checked: true,
                },
                vec![1, 2],
            ),
            (
                ExprKind::ArrayLit(vec![child_expr(1), child_expr(2)]),
                vec![1, 2],
            ),
            (
                ExprKind::ArraySpreadLit(vec![
                    ArrayLitElem {
                        expr: child_expr(1),
                        spread: None,
                    },
                    ArrayLitElem {
                        expr: child_expr(2),
                        spread: Some(SpreadKind::Array),
                    },
                ]),
                vec![1, 2],
            ),
            (
                ExprKind::Template(vec![
                    TplPart::Text(String::new()),
                    TplPart::Expr(child_expr(1)),
                    TplPart::Expr(child_expr(2)),
                ]),
                vec![1, 2],
            ),
            (
                ExprKind::Lambda {
                    params: Vec::new(),
                    ret: Type::Void,
                    body: vec![child_stmt(1), child_stmt(2)],
                    captures: Vec::new(),
                },
                vec![1, 2],
            ),
            (ExprKind::Yield(Some(Box::new(child_expr(1)))), vec![1]),
            (
                ExprKind::AsyncCall {
                    callee: AsyncCallee::Method {
                        class: ClassId(0),
                        receiver: Box::new(child_expr(1)),
                        name: String::new(),
                    },
                    args: vec![child_expr(2)],
                },
                vec![1, 2],
            ),
            (
                ExprKind::AsyncHandleCreate {
                    callee: AsyncCallee::Method {
                        class: ClassId(0),
                        receiver: Box::new(child_expr(1)),
                        name: String::new(),
                    },
                    args: vec![child_expr(2)],
                    origin: 0,
                },
                vec![1, 2],
            ),
            (ExprKind::AsyncHandleAwait(Box::new(child_expr(1))), vec![1]),
            (
                ExprKind::AsyncHandleTransfer {
                    value: Box::new(child_expr(1)),
                    origin: 0,
                },
                vec![1],
            ),
            (
                ExprKind::Cond {
                    cond: Box::new(child_expr(1)),
                    then: Box::new(child_expr(2)),
                    els: Box::new(child_expr(3)),
                },
                vec![1, 2, 3],
            ),
        ];
        for (kind, expected) in cases {
            assert_eq!(child_values(test_expr(kind).children()), expected);
        }
    }

    #[test]
    fn stmt_children_yield_every_child() {
        let pos = Pos::new("children.ts", 1, 1);
        let cases = vec![
            (
                Stmt::Let {
                    name: String::new(),
                    ty: Type::I32,
                    mutable: false,
                    dispose: false,
                    init: child_expr(1),
                    pos: pos.clone(),
                },
                vec![1],
            ),
            (Stmt::Expr(child_expr(1)), vec![1]),
            (
                Stmt::Return {
                    value: Some(child_expr(1)),
                    pos: pos.clone(),
                },
                vec![1],
            ),
            (
                Stmt::Return {
                    value: None,
                    pos: pos.clone(),
                },
                Vec::new(),
            ),
            (
                Stmt::If {
                    cond: child_expr(1),
                    then: vec![child_stmt(2)],
                    els: Some(vec![child_stmt(3)]),
                    pos: pos.clone(),
                },
                vec![1, 2, 3],
            ),
            (
                Stmt::While {
                    cond: child_expr(1),
                    body: vec![child_stmt(2)],
                    pos: pos.clone(),
                },
                vec![1, 2],
            ),
            (
                Stmt::For {
                    init: Some(Box::new(child_stmt(1))),
                    cond: Some(child_expr(2)),
                    step: Some(child_expr(3)),
                    body: vec![child_stmt(4)],
                    pos: pos.clone(),
                },
                vec![1, 2, 3, 4],
            ),
            (
                Stmt::ForOf {
                    name: String::new(),
                    ty: Type::I32,
                    subject: child_expr(1),
                    kind: ForOfKind::ArrayValues,
                    body: vec![child_stmt(2)],
                    pos: pos.clone(),
                },
                vec![1, 2],
            ),
            (
                Stmt::Switch {
                    disc: child_expr(1),
                    cases: vec![SwitchCase {
                        test: Some(child_expr(2)),
                        body: vec![child_stmt(3)],
                        pos: pos.clone(),
                    }],
                    pos: pos.clone(),
                },
                vec![1, 2, 3],
            ),
            (Stmt::Block(vec![child_stmt(1), child_stmt(2)]), vec![1, 2]),
            (Stmt::Break(pos.clone()), Vec::new()),
            (Stmt::Continue(pos), Vec::new()),
        ];
        for (stmt, expected) in cases {
            assert_eq!(child_values(stmt.children()), expected);
        }
    }

    #[test]
    fn host_entry_trap_sites_name_each_wire_parameter() {
        let parameter_pos = Pos::new("wire-entry.ts", 3, 27);
        let function = Function {
            name: "configure".to_string(),
            exported: true,
            is_generator: false,
            is_async: false,
            params: vec![Param {
                name: "mode".to_string(),
                ty: Type::StringAlias(crate::types::StringAliasId(0)),
                default: None,
                foreign_provenance: None,
                pos: parameter_pos.clone(),
            }],
            ret: Type::Void,
            body: Vec::new(),
            pos: Pos::new("wire-entry.ts", 3, 1),
        };
        let module = Module {
            poisoned_imports: Vec::new(),
            classes: Vec::new(),
            enums: Vec::new(),
            string_aliases: vec![StringAliasDef {
                name: "WireMode".to_string(),
                members: vec!["m0".to_string()],
                wire_values: Some(vec![16]),
                pos: Pos::new("wire-entry.ts", 1, 1),
            }],
            globals: Vec::new(),
            functions: vec![function.clone()],
            worker_entries: Vec::new(),
            operation_signatures: Vec::new(),
            foreign_fns: Vec::new(),
            foreign_mirrors: Vec::new(),
            top_level: Vec::new(),
        };
        assert_eq!(
            function.host_entry_trap_sites(&module),
            Some(vec![TrapSite::WireEnumValue {
                alias: crate::types::StringAliasId(0),
                pos: parameter_pos,
            }])
        );
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
        assert_eq!(MathFn::Clz32.arity(), 1);
        assert_eq!(MathFn::Imul.arity(), 2);
        assert_eq!(MathFn::Fround.arity(), 1);
        assert_eq!(MathFn::F32ToBits.arity(), 1);
        assert_eq!(MathFn::F32FromBits.arity(), 1);
        assert_eq!(MathFn::Random.name(), "random");
        assert_eq!(MathFn::Log1p.name(), "log1p");
        assert_eq!(MathFn::Clz32.name(), "clz32");
        assert_eq!(MathFn::Imul.name(), "imul");
        assert_eq!(MathFn::Fround.name(), "fround");
        assert_eq!(MathFn::F32ToBits.name(), "f32ToBits");
        assert_eq!(MathFn::F32FromBits.name(), "f32FromBits");
        assert_eq!(MathFn::F32ToBits.symbol(), "subscript_rt_math_f32_to_bits");
        assert_eq!(
            MathFn::F32FromBits.symbol(),
            "subscript_rt_math_f32_from_bits"
        );
    }

    #[test]
    fn num_fn_table_matches_the_section_11_contract() {
        for (index, f) in NumFn::ALL.iter().enumerate() {
            assert_eq!(*f as usize, index, "NumFn::ALL out of order at {index}");
            assert!(f.symbol().starts_with("subscript_rt_num_"));
        }
        assert!(NumFn::IsNaN.returns_bool());
        assert!(!NumFn::ParseFloat.returns_bool());
        assert!(NumFn::ParseInt.takes_pos_id());
        assert!(NumFn::ParseFloat.takes_pos_id());
        assert!(NumFn::ToFixed.takes_pos_id());
        assert!(NumFn::ToStringF32.takes_pos_id());
        assert!(NumFn::ToStringF64.takes_pos_id());
        assert!(NumFn::ToExponential.takes_pos_id());
        assert!(NumFn::ToPrecision.takes_pos_id());
        assert!(!NumFn::IsFinite.takes_pos_id());
    }

    #[test]
    fn date_fn_field_codes_cover_the_eight_accessors_in_order() {
        // The subscript_rt_date_get field-code contract (stdlib.md §3): the
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
    fn callee_trap_policy_delegates_to_operation_predicates() {
        assert!(!Callee::Ambient(AmbientFn::Print).has_call_site());
        assert!(Callee::Ambient(AmbientFn::Unreachable).has_call_site());
        assert!(Callee::Ambient(AmbientFn::UnsafeDelete).has_call_site());
        assert!(!Callee::Math(MathFn::Abs).has_call_site());
        assert!(Callee::Num(NumFn::ParseInt).has_call_site());
        assert!(!Callee::Num(NumFn::IsFinite).has_call_site());
        assert!(Callee::Date(DateFn::New).has_call_site());
        assert!(!Callee::Date(DateFn::Now).has_call_site());
        assert!(Callee::Json(JsonFn::Finish).has_call_site());
        assert!(Callee::Str(StrFn::CharCodeAt).has_call_site());
        assert!(!Callee::Str(StrFn::Includes).has_call_site());
        assert!(Callee::Arr(ArrFn::ForEach).has_call_site());
        assert!(!Callee::Arr(ArrFn::Reverse).has_call_site());
        assert!(Callee::Map(MapFn::Set).has_call_site());
        assert!(!Callee::Map(MapFn::Get).has_call_site());
        assert!(Callee::Set(SetFn::Union).has_call_site());
        assert!(!Callee::Set(SetFn::Has).has_call_site());
        assert!(Callee::Func("script".to_string()).has_call_site());
        assert!(Callee::Foreign("host".to_string()).has_call_site());
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
        // Symbols follow the subscript_rt_str_* convention, distinctly.
        let mut symbols: Vec<&str> = StrFn::ALL.iter().map(|f| f.symbol()).collect();
        symbols.sort_unstable();
        symbols.dedup();
        assert_eq!(symbols.len(), StrFn::ALL.len());
        assert!(StrFn::ALL
            .iter()
            .all(|f| f.symbol().starts_with("subscript_rt_str_")));
        assert_eq!(StrFn::ToUpperCase.symbol(), "subscript_rt_str_to_upper");
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
        // Symbols follow the subscript_rt_arr_* convention, distinctly.
        let mut symbols: Vec<&str> = ArrFn::ALL.iter().map(|f| f.symbol()).collect();
        symbols.sort_unstable();
        symbols.dedup();
        assert_eq!(symbols.len(), ArrFn::ALL.len());
        assert!(ArrFn::ALL
            .iter()
            .all(|f| f.symbol().starts_with("subscript_rt_arr_")));
        assert_eq!(ArrFn::ForEach.symbol(), "subscript_rt_arr_for_each");
        assert_eq!(ArrFn::FindIndex.name(), "findIndex");
        // Callback set: exactly the nine closure-taking methods.
        let with_cb: Vec<ArrFn> = ArrFn::ALL
            .iter()
            .copied()
            .filter(|f| f.takes_callback())
            .collect();
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
                ArrFn::ReduceRight,
            ]
        );
        for f in [
            ArrFn::ForEach,
            ArrFn::Map,
            ArrFn::Filter,
            ArrFn::Some,
            ArrFn::Every,
            ArrFn::FindIndex,
        ] {
            assert_eq!(f.callback_index_arity(), Some(2), "{}", f.name());
            assert!(f.api_signature().contains("index: i32"), "{}", f.name());
        }
        for f in [ArrFn::Reduce, ArrFn::ReduceRight] {
            assert_eq!(f.callback_index_arity(), Some(3), "{}", f.name());
            assert!(f.api_signature().contains("index: i32"), "{}", f.name());
        }
        assert_eq!(ArrFn::Sort.callback_index_arity(), None);
        assert!(!ArrFn::Sort.api_signature().contains("index"));
        // pos_id: the allocating operations plus the trapping shift.
        for f in ArrFn::ALL {
            let carries_pos = matches!(
                f,
                ArrFn::Join
                    | ArrFn::Slice
                    | ArrFn::Concat
                    | ArrFn::Map
                    | ArrFn::Filter
                    | ArrFn::Splice
                    | ArrFn::Shift
                    | ArrFn::Unshift
            );
            assert_eq!(f.takes_pos_id(), carries_pos, "pos_id of {}", f.name());
            assert_eq!(
                f.can_trap(),
                carries_pos || f.takes_callback(),
                "trap check of {}",
                f.name()
            );
        }
        assert_eq!(ArrFn::END_SENTINEL, i64::from(i32::MAX));
    }

    #[test]
    fn map_set_fn_tables_match_the_section_10_contract() {
        for (i, f) in MapFn::ALL.iter().enumerate() {
            assert_eq!(*f as usize, i, "MapFn::ALL out of order at {i}");
            assert!(
                f.symbol().starts_with("subscript_rt_map_")
                    || f.symbol().starts_with("subscript_rt_assoc_")
            );
        }
        for (i, f) in SetFn::ALL.iter().enumerate() {
            assert_eq!(*f as usize, i, "SetFn::ALL out of order at {i}");
            assert!(
                f.symbol().starts_with("subscript_rt_set_")
                    || f.symbol().starts_with("subscript_rt_assoc_")
            );
        }
        assert_eq!(MapFn::Size.symbol(), SetFn::Size.symbol());
        assert_eq!(MapFn::Has.symbol(), SetFn::Has.symbol());
        assert_eq!(MapFn::Delete.symbol(), SetFn::Delete.symbol());
        assert_eq!(MapFn::Clear.symbol(), SetFn::Clear.symbol());
        assert!(MapFn::New.allocates());
        assert!(MapFn::Set.allocates());
        assert!(MapFn::GroupBy.allocates());
        assert!(!MapFn::Get.allocates());
        assert!(MapFn::ForEach.can_trap());
        assert!(MapFn::GroupBy.can_trap());
        assert!(SetFn::New.allocates());
        assert!(SetFn::Add.allocates());
        assert!(SetFn::Union.allocates());
        assert!(SetFn::Intersection.allocates());
        assert!(SetFn::Difference.allocates());
        assert!(SetFn::SymmetricDifference.allocates());
        assert!(!SetFn::Has.allocates());
        assert!(!SetFn::IsSubsetOf.allocates());
        assert!(SetFn::ForEach.can_trap());
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
        assert_eq!(
            ArrFmtKind::of(&Type::Enum(EnumId(0))),
            Some(ArrFmtKind::I32)
        );
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
            poisoned_imports: Vec::new(),
            classes: Vec::new(),
            enums: Vec::new(),
            string_aliases: Vec::new(),
            globals: Vec::new(),
            functions: Vec::new(),
            worker_entries: Vec::new(),
            operation_signatures: Vec::new(),
            foreign_fns: Vec::new(),
            foreign_mirrors: Vec::new(),
            top_level: Vec::new(),
        };
        assert!(m.functions.is_empty());
    }

    #[test]
    fn worker_intrinsic_identity_uses_the_all_table_order() {
        assert_eq!(WorkerFn::ALL.len(), 8);
        assert_eq!(WorkerFn::Spawn(37).intrinsic_identity(), WorkerFn::ALL[0]);
        assert_eq!(WorkerFn::OutboxPost.intrinsic_identity(), WorkerFn::ALL[7]);
    }
}
