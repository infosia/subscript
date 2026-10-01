//! Typed high-level IR produced by a successful check.
//!
//! Every expression node carries its resolved [`Type`] and a TS [`Pos`].
//! Generic declarations are monomorphized here: the module contains one
//! function/class per instantiation (e.g. `identity<i32>`), never a
//! generic template. A discovery HIR can contain [`Type::Error`] and one
//! or more [`PoisonedImport`] records.

mod names;
pub use names::{declaration_label, source_name, Symbol};

mod collections;
mod definitions;
mod host_entry;
pub use host_entry::{HostEntry, HostSignature};
mod effects;
mod expression;
pub(crate) use effects::NarrowingEffects;
mod intrinsics;
mod shared;
mod sites;
mod text;
pub use crate::lifetime::LifetimeOperand;

pub use text::TextFn;

use crate::diag::Pos;
use crate::types::{CallbackLifetime, ClassId, EnumId, HandleClass, HandleKind, IterKind, Type};

mod function;
mod parameter;
pub use parameter::Param;

mod using;
pub use using::UsingBinding;

/// Names the synchronous disposal hook after the checker lowers `[Symbol.dispose]`.
pub const DISPOSE_METHOD_NAME: &str = "[[Symbol.dispose]]";

/// Names the hidden `u32` kind tag that precedes the two fields of the
/// Error class (`compiler.md` §115.1 rule 3). No source spelling reaches it.
pub const ERROR_KIND_FIELD: &str = "[[kind]]";

/// A lambda unit identity for the initializer scan (compiler.md §137 rule 5b).
/// The checker assigns a separate identity to each generic body instance.
#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LambdaId(pub usize);

/// A checked program: all source files merged into one module.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Module {
    /// Host API resolved from the explicitly named entry module.
    pub host_entries: Vec<HostEntry>,
    /// Stable diagnostic position of the program entry module.
    pub entry_pos: Pos,
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
    /// Declaration symbols of synthesized helper functions.
    pub synthesized_helpers: std::collections::HashSet<Symbol>,
    /// Q35 worker-entry adapters required by `Worker.spawn` call sites,
    /// deduplicated by function declaration symbol and message-class pair.
    pub worker_entries: Vec<WorkerEntry>,
    /// Checker-derived signatures for intrinsic and built-in calls.
    pub operation_signatures: Vec<OperationSignature>,
    /// Foreign (C-ABI) functions declared by an ingested ambient mirror
    /// (`declare function` in a `.d.ts`, §12.2). They carry a signature
    /// but no body; a call to one lowers to an imported C symbol.
    pub foreign_fns: Vec<ForeignFn>,
    /// Ambient mirrors that contribute foreign functions, with the exact
    /// C header include spelling recovered from generated provenance.
    pub foreign_mirrors: Vec<ForeignMirror>,
    /// Checked top-level statements in module run order, with source order inside each module.
    pub top_level: Vec<Stmt>,
    /// Source module identities in run order, including modules with no initializer work.
    pub initializer_modules: Vec<String>,
    /// Indices of regex literal globals that initialize before all module work.
    pub regex_literal_globals: Vec<usize>,
    /// Checker-derived initializer segments, one per identity in `initializer_modules`.
    pub initializer_segments: Vec<InitializerSegment>,
    /// Whether the regex initializers and module segments can leave an exception pending (compiler.md §115.6 rule 3).
    pub initializer_can_raise: bool,
    /// Total bytes of the source texts the check read for this module.
    /// The dev JIT derives one module's one memory reservation from
    /// this number (`specs/blocks/compiler.md` §110 rule 3).
    pub source_bytes: usize,
}

/// The initializer work that one source module owns.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct InitializerSegment {
    /// The range of top-level statements in this source module.
    pub top_level: std::ops::Range<usize>,
    /// Indices into the module's globals vector, in source order.
    pub globals: Vec<usize>,
}

impl InitializerSegment {
    /// Builds the checked ownership of one module initializer.
    pub fn new(top_level: std::ops::Range<usize>, globals: Vec<usize>) -> Self {
        Self { top_level, globals }
    }
}

/// One root that owns expressions in a checked module.
/// This enum must stay exhaustive so cross-crate matches stay total.
pub enum ExpressionOwner<'a> {
    /// One initializer or parameter default.
    Expr(&'a Expr),
    /// One function, method, constructor, or module body.
    Body {
        /// Statements in the body.
        statements: &'a [Stmt],
        /// Function metadata, or `None` for the module body.
        function: Option<&'a Function>,
    },
}

/// One mutable root that owns expressions in a checked module.
pub enum ExpressionOwnerMut<'a> {
    /// One initializer or parameter default.
    Expr(&'a mut Expr),
    /// One function, method, constructor, or module body.
    Body(&'a mut [Stmt]),
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
    /// Error formatting and URI text operations (stdlib.md §19).
    Text(TextFn),
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
    /// Checker-assigned declaration symbol of a module-level script function.
    pub function: Symbol,
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
/// (`declare function`, §12.2). It is neither a script [`Function`] nor a
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
    /// Program-unique, module-qualified class identity assigned by the checker
    /// (compiler.md §125 rule 2, §131 rule 3).
    pub symbol: Symbol,
    /// Source name; monomorphized instances use `Name<args>` spelling.
    pub name: String,
    /// True for `@ValueType class` (C-layout, copy semantics — C2).
    pub is_value: bool,
    /// The explicit value-class alignment and its decorator position.
    pub alignment_override: Option<AlignmentOverride>,
    /// True for a literal-constructible `@Descriptor` reference class
    /// (Q33). Descriptor classes have fields only; object literals lower
    /// through [`ExprKind::DescriptorLit`].
    pub is_descriptor: bool,
    /// True for a mirror-ingested boundary struct (a `declare class` in a
    /// `.d.ts`, §12.2): a C-layout value type whose constructor has no
    /// in-language body. `new` initializes its fields positionally from
    /// the constructor arguments (arg `i` → field `i`), applying the
    /// boundary coercions at each field (the chain-slot address-of for a
    /// `Struct | null` field). Always `false` for ordinary value classes,
    /// which carry a real [`ClassDef::ctor`].
    pub is_boundary: bool,
    /// The lifetime of the callback registrations this boundary class
    /// creates (§111 rule 2). The checker sets it from the mirror's
    /// `@subscript-c-callback-lifetime` record. Every other class carries
    /// [`CallbackLifetime::Context`].
    pub callback_lifetime: CallbackLifetime,
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

/// An explicit alignment on an `@ValueType` value class.
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
    /// True when this is a descriptor field spelled `name?: A` (§43), where
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
    /// Per-member C-boundary values for a `CEnum` alias (§50.1), in the
    /// same declaration order. `None` identifies a plain Q32 alias.
    pub wire_values: Option<Vec<i32>>,
    /// Position of the alias declaration.
    pub pos: Pos,
}

/// A module-level variable.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Global {
    /// Program-unique declaration symbol assigned by the checker (compiler.md §125).
    pub symbol: Symbol,
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
    /// Program-unique symbol for a free function; members use their class identity.
    pub symbol: Symbol,
    /// Checker-synthesized helper without a reload slot (compiler.md §119).
    pub synthesized_helper: bool,
    /// Source name; monomorphized instances use `name<args>` spelling.
    pub name: String,
    /// True when declared `export`. The signature determines host-entry eligibility.
    pub exported: bool,
    /// True for `function*` coroutines (C8).
    pub is_generator: bool,
    /// True for a poll-driven async function or reference-class instance
    /// method (§26, §37). `ret` is the fulfilled value type inside the
    /// source-level `Promise<ret>` view.
    pub is_async: bool,
    /// Parameters, in order.
    pub params: Vec<Param>,
    /// Return type. For a generator this is `Generator<Y>` where `Y` is
    /// the yield type; `yield` expressions inside the body have type `Y`.
    pub ret: Type,
    /// Checked body statements.
    pub body: Vec<Stmt>,
    /// Whether a call to this function can leave an exception pending
    /// (`compiler.md` §115.6 rule 3). One HIR pass derives it after the
    /// check; every engine reads it. It is false for an `async` function
    /// and a generator, whose bodies convert an exception into a trap
    /// (§115.4 items 2 and 3).
    pub can_raise: bool,
    /// Position of the declaration.
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
    /// A raise site: the preceding call can leave an exception pending
    /// (`compiler.md` §115.6 rules 2 and 3). The lowering gives it a
    /// handler edge.
    Raise {
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
        /// The receiver or argument whose allocation is read.
        operand: LifetimeOperand,
        /// Position of the access.
        pos: Pos,
    },
    /// Development-tier release validation (compiler.md §120.1 rule 3b).
    DevOnlyRelease {
        /// The argument whose allocation is released.
        operand: LifetimeOperand,
        /// Position of the release call.
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

/// Ambient prelude functions and namespace members (Q6, Q7, Q12, §42.2);
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

/// Typed `Context` storage-byte operations (stdlib.md §18).
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
    /// malformed input, and the runtime records the failure.
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
    /// Allocates the `SyntaxError` message of the failed `ParseBegin`
    /// (`compiler.md` §115.7 rules 3 and 4).
    ParseFailure,
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
    /// `toUTCString()` formats every TimeClip year as UTC text (stdlib.md §3.1).
    ToUtcString,
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
    /// `split(sep)` — `string[]`; an empty separator splits UTF-8 code points.
    Split,
    /// `trim()` — ECMA WhiteSpace + LineTerminator code points.
    Trim,
    /// `trimStart()`.
    TrimStart,
    /// `trimEnd()`.
    TrimEnd,
    /// `repeat(n)` — `n < 0` traps; `repeat(0)` is `""`.
    Repeat,
    /// `padStart(len, pad)` — an empty `pad` returns unchanged bytes.
    PadStart,
    /// `padEnd(len, pad)` — same empty-pad rule as `padStart`.
    PadEnd,
    /// `toUpperCase()` — Unicode Default Case Conversion.
    ToUpperCase,
    /// `toLowerCase()` — Unicode Default Case Conversion.
    ToLowerCase,
    /// `replace(pat, repl)` — first occurrence with ECMA string-pattern
    /// `$` substitutions (Q27).
    Replace,
    /// `replaceAll(pat, repl)` — all occurrences with ECMA
    /// string-pattern `$` substitutions; an empty `pat` matches each code-point boundary.
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
    /// `at(i)` — a code point at a signed byte index; invalid indices trap.
    At,
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
    /// `re.global`.
    Global,
    /// `re.ignoreCase`.
    IgnoreCase,
    /// `re.multiline`.
    Multiline,
    /// `re.dotAll`.
    DotAll,
    /// `re.unicode`.
    Unicode,
    /// `re.hasIndices`.
    HasIndices,
    /// `re.sticky`.
    Sticky,
    /// `re.toString()`.
    ToString,
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
    /// Reads an element at a signed index; an out-of-range index traps.
    At,
    /// Finds the first matching nullable-capable element, or null.
    Find,
    /// Finds the last matching nullable-capable element, or null.
    FindLast,
    /// Finds the last matching index, or minus one.
    FindLastIndex,
    /// Concatenates callback arrays at depth one.
    FlatMap,
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

/// What a call dispatches to.
#[derive(Debug, Clone, PartialEq)]
pub enum Callee {
    /// A module function by its checker-assigned declaration symbol.
    Func(Symbol),
    /// A foreign C-ABI function declared by an ambient mirror (§12.2);
    /// carries the symbol name. Both tiers lower the call to an imported
    /// C symbol.
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
    /// Error formatting and URI text operations (stdlib.md §19).
    Text(TextFn),
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
        /// Method declaration symbol within the receiver class. For a
        /// built-in member, the full text is the member name.
        name: Symbol,
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
            Callee::Text(_) => true,
            Callee::Str(f) => f.takes_pos_id(),
            Callee::Regex(f) => f.can_trap(),
            Callee::Arr(f) => f.can_trap(),
            Callee::Map(f) => f.can_trap(),
            Callee::Set(f) => f.can_trap(),
            Callee::Worker(_) => true,
        }
    }
}

/// Returns the operation-table target and receiver type for a callee.
///
/// The receiver is present only when the operation uses method syntax and
/// must become the first execution operand.
#[must_use]
pub fn operation_signature_target(
    callee: &Callee,
) -> Option<(OperationSignatureTarget, Option<&Type>)> {
    let target = match callee {
        Callee::Ambient(function) => OperationSignatureTarget::Ambient(*function),
        Callee::ContextBytes { function, ty } => {
            OperationSignatureTarget::ContextBytes(*function, ty.clone())
        }
        Callee::Math(function) => OperationSignatureTarget::Math(*function),
        Callee::Num(function) => OperationSignatureTarget::Num(*function),
        Callee::Date(function) => OperationSignatureTarget::Date(*function),
        Callee::Json(function) => OperationSignatureTarget::Json(*function),
        Callee::Text(function) => OperationSignatureTarget::Text(*function),
        Callee::Str(function) => OperationSignatureTarget::Str(*function),
        Callee::Regex(function) => OperationSignatureTarget::Regex(*function),
        Callee::Arr(function) => OperationSignatureTarget::Arr(*function),
        Callee::Map(function) => OperationSignatureTarget::Map(*function),
        Callee::Set(function) => OperationSignatureTarget::Set(*function),
        Callee::Worker(function) => OperationSignatureTarget::Worker(*function),
        Callee::Method { recv, name } => {
            let method = match (&recv.ty, name.full_text()) {
                (Type::Array(_), "push") => BuiltinMethod::ArrayPush,
                (Type::Array(_), "pop") => BuiltinMethod::ArrayPop,
                (Type::Str, "slice") => BuiltinMethod::StringSlice,
                (Type::Generator(_), "next") => BuiltinMethod::GeneratorNext,
                _ => return None,
            };
            return Some((
                OperationSignatureTarget::BuiltinMethod(method),
                Some(&recv.ty),
            ));
        }
        Callee::Func(_) | Callee::Foreign(_) | Callee::Value(_) => return None,
    };
    Some((target, None))
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
    /// Local name and declared storage type (compiler.md §124).
    Local(String, Type),
    /// Reference to a module-level variable by its declaration symbol.
    Global(Symbol),
    /// A function declaration symbol used as a value (non-capturing — C5).
    FuncRef(Symbol),
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
        /// True for `!= undefined` or `!== undefined`; false for `== undefined` or `=== undefined`.
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
        /// The checker-assigned unit identity. Only the initializer scan reads it.
        id: LambdaId,
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
        /// Whether the body can leave an exception pending
        /// (`compiler.md` §115.6 rule 3). The check derives it after the
        /// function facts; the lowering reads it.
        can_raise: bool,
    },
    /// `yield` inside a generator (C8).
    Yield(Option<Box<Expr>>),
    /// `await Context.suspend()` inside an async function (Q34).
    AsyncSuspend,
    /// A direct async call in await position (§26, §37). The result type is
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

/// Target of a direct async call in await position (§26, §37).
///
/// Keeping the method receiver inside the target makes its source-order
/// relationship to the explicit arguments structural: it is evaluated once,
/// before `ExprKind::AsyncCall::args`, and becomes the first payload slot of
/// the callee frame.
#[derive(Debug, Clone, PartialEq)]
pub enum AsyncCallee {
    /// A module async function by its declaration symbol.
    Function(Symbol),
    /// An async instance method on a plain reference class.
    Method {
        /// Declaring/receiver class.
        class: ClassId,
        /// Receiver expression, evaluated exactly once before arguments.
        receiver: Box<Expr>,
        /// Method declaration symbol within `class`.
        name: Symbol,
    },
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
    /// Set values.
    SetValues,
    /// String code points.
    StringCodePoints,
}

#[cfg(test)]
mod tests;
