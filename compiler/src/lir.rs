//! Ordered low-level IR shared by the development and shipping tiers.
//!
//! LIR is deliberately expression-free. Instructions consume only named
//! values or constants, and functions are graphs of basic blocks with one
//! terminator apiece. Source spellings are retained only as diagnostic
//! attributes; executable references use ids.

use crate::diag::Pos;
use crate::types::{ClassId, EnumId, StringAliasId, Type};

macro_rules! id_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);
    };
}

id_type!(MethodId, "Module-unique method or constructor id.");
id_type!(FunctionId, "Module-unique executable function id.");
id_type!(ForeignFunctionId, "Module-unique foreign function id.");
id_type!(GlobalId, "Module-unique global id.");
id_type!(FieldId, "Module-unique class-field id.");
id_type!(LocalId, "Function-local storage id.");
id_type!(BlockId, "Function-local basic-block id.");
id_type!(ValueId, "Function-local SSA value id.");

/// One completely lowered checked module.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    /// Executable program entry, absent for host-callable entryless modules.
    pub entry: Option<FunctionId>,
    /// Exported zero-parameter async functions that the standard runner starts.
    pub async_roots: Vec<FunctionId>,
    /// Class declarations in [`ClassId`] order.
    pub classes: Vec<Class>,
    /// Numeric enum declarations in [`EnumId`] order.
    pub enums: Vec<Enum>,
    /// String-alias declarations in [`StringAliasId`] order.
    pub string_aliases: Vec<StringAlias>,
    /// Module globals in [`GlobalId`] order.
    pub globals: Vec<Global>,
    /// Foreign declarations in [`ForeignFunctionId`] order.
    pub foreign_functions: Vec<ForeignFunction>,
    /// Every free, method, constructor, lambda, and synthetic function.
    pub functions: Vec<Function>,
    /// Worker adapters requested by checked worker-spawn sites.
    pub worker_entries: Vec<WorkerEntry>,
    /// Closed intrinsic and built-in operation/signature table.
    pub intrinsic_operations: Vec<IntrinsicOperation>,
    /// Optional synthetic module-initializer function.
    pub initializer: Option<FunctionId>,
}

/// One LIR class declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    /// Stable class id, also used by [`Type::Class`].
    pub id: ClassId,
    /// Source spelling retained for diagnostics and reload keys only.
    pub source_name: String,
    /// Whether the class has C-layout copy semantics.
    pub is_value: bool,
    /// Whether object-literal descriptor construction is allowed.
    pub is_descriptor: bool,
    /// Whether the declaration came from a boundary mirror.
    pub is_boundary: bool,
    /// Whether this boundary value class is an intrusive embedded header.
    pub is_embedded_header: bool,
    /// Explicit alignment, when declared.
    pub alignment: Option<u32>,
    /// Fields in declaration order.
    pub fields: Vec<Field>,
    /// Constructor method, when declared.
    pub constructor: Option<Method>,
    /// Methods in declaration order.
    pub methods: Vec<Method>,
    /// Optional indexed-access contract.
    pub index_signature: Option<IndexSignature>,
    /// Declaration position.
    pub pos: Pos,
}

/// One indexed-access contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSignature {
    /// Accepted index type.
    pub index_type: Type,
    /// Element type.
    pub element_type: Type,
    /// Whether indexed writes are forbidden.
    pub readonly: bool,
}

/// One class field.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Module-unique field id.
    pub id: FieldId,
    /// Source spelling retained as an attribute.
    pub source_name: String,
    /// Stored type.
    pub ty: Type,
    /// Whether omission evaluates a checked default.
    pub is_defaulted: bool,
    /// Whether omission stores the alias absence discriminant.
    pub is_absence_capable: bool,
    /// Foreign boundary spelling, when one was absorbed by the checker.
    pub foreign_provenance: Option<ForeignTypeProvenance>,
    /// Declaration position.
    pub pos: Pos,
}

/// A method entity and its executable function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Method {
    /// Module-unique method id.
    pub id: MethodId,
    /// Executable body id.
    pub function: FunctionId,
    /// Source spelling retained as an attribute.
    pub source_name: String,
}

/// One numeric enum.
#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    /// Stable enum id.
    pub id: EnumId,
    /// Source spelling retained as an attribute.
    pub source_name: String,
    /// Declaration-ordered member spellings and values.
    pub members: Vec<(String, i64)>,
    /// Declaration position.
    pub pos: Pos,
}

/// One nominal string-literal union.
#[derive(Debug, Clone, PartialEq)]
pub struct StringAlias {
    /// Stable alias id.
    pub id: StringAliasId,
    /// Source spelling retained as an attribute.
    pub source_name: String,
    /// Declaration-ordered member spellings.
    pub members: Vec<String>,
    /// Boundary wire values, when this is a wire alias.
    pub wire_values: Option<Vec<i32>>,
    /// Reserved absence representation.
    pub absence_discriminant: i64,
    /// Declaration position.
    pub pos: Pos,
}

/// One module global.
#[derive(Debug, Clone, PartialEq)]
pub struct Global {
    /// Module-unique global id.
    pub id: GlobalId,
    /// Source spelling retained as an attribute.
    pub source_name: String,
    /// Stored type.
    pub ty: Type,
    /// Whether assignments are accepted.
    pub mutable: bool,
    /// Declaration position.
    pub pos: Pos,
}

/// Boundary type information attached to the exact occurrence that uses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForeignTypeProvenance {
    /// A by-value descriptor aggregate.
    Descriptor {
        /// C aggregate spelling.
        aggregate: String,
        /// C element spelling.
        element: String,
        /// Whether the C element pointer is const.
        element_const: bool,
    },
    /// A scalar count followed by an element pointer.
    ScalarPair {
        /// C element spelling.
        element: String,
        /// Whether the C element pointer is const.
        element_const: bool,
    },
    /// A C string-view aggregate.
    StringView {
        /// C aggregate spelling.
        aggregate: String,
    },
    /// A callback typedef with its userdata position.
    Callback {
        /// C function-pointer typedef spelling.
        typedef_name: String,
    },
}

/// One foreign C-ABI declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignFunction {
    /// Module-unique id.
    pub id: ForeignFunctionId,
    /// Source/C spelling retained as an attribute and link key.
    pub source_name: String,
    /// Typed parameters.
    pub parameters: Vec<ForeignParameter>,
    /// Return type.
    pub return_type: Type,
    /// Exact header include spelling.
    pub include: String,
    /// Declaration position.
    pub pos: Pos,
}

/// One foreign parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignParameter {
    /// Source spelling retained as an attribute.
    pub source_name: String,
    /// Boundary type.
    pub ty: Type,
    /// Absorbed C spelling, when present.
    pub foreign_provenance: Option<ForeignTypeProvenance>,
    /// Declaration position.
    pub pos: Pos,
}

/// One worker entry adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerEntry {
    /// Direct script function id.
    pub function: FunctionId,
    /// Parent-to-worker message class.
    pub input: ClassId,
    /// Worker-to-parent message class.
    pub output: ClassId,
}

/// One executable LIR function.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    /// Module-unique function id.
    pub id: FunctionId,
    /// Source spelling retained for diagnostics and reload keys only.
    pub source_name: String,
    /// Declaration/creation role.
    pub kind: FunctionKind,
    /// Whether this free function is host-exported.
    pub exported: bool,
    /// Whether this is a generator resume graph.
    pub is_generator: bool,
    /// Whether this is an async resume graph.
    pub is_async: bool,
    /// Ordered traps owned by creating this coroutine's suspended frame.
    pub creation_traps: Vec<Trap>,
    /// Ordered validation traps owned by the host-entry adapter.
    ///
    /// `None` means that this function has no host entry. `Some` can hold an
    /// empty list when the entry needs no parameter validation.
    pub host_entry_traps: Option<Vec<Trap>>,
    /// Typed parameters; each parameter value is a definition.
    pub parameters: Vec<Parameter>,
    /// Language return type.
    pub return_type: Type,
    /// Every named local storage entity.
    pub locals: Vec<Local>,
    /// Every named value entity.
    pub values: Vec<Value>,
    /// The lowering's single graph liveness result and value-version origins.
    pub liveness: Liveness,
    /// Basic blocks in id order.
    pub blocks: Vec<BasicBlock>,
    /// Entry block.
    pub entry: BlockId,
    /// Declaration or literal position.
    pub pos: Pos,
}

/// The one fixed-point liveness result retained for all LIR consumers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Liveness {
    /// Values live at each block entry, in [`BlockId`] order.
    pub live_ins: Vec<Vec<ValueId>>,
    /// Original logical value for each SSA version, in [`ValueId`] order.
    pub value_origins: Vec<ValueId>,
}

/// The source role of an executable function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionKind {
    /// Module-level source function.
    Free,
    /// Class constructor.
    Constructor {
        /// Owning class.
        class: ClassId,
        /// Constructor entity.
        method: MethodId,
    },
    /// Class method.
    Method {
        /// Owning class.
        class: ClassId,
        /// Method entity.
        method: MethodId,
    },
    /// Lambda body.
    Lambda,
    /// Module initialization graph.
    ModuleInitializer,
}

/// One function parameter definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    /// Backing storage, present only when the parameter's address is taken.
    pub storage: Option<LocalId>,
    /// Entry-defined value id.
    pub value: ValueId,
    /// Source spelling retained as an attribute.
    pub source_name: String,
    /// Parameter role.
    pub kind: ParameterKind,
    /// Declaration position.
    pub pos: Pos,
}

/// How a parameter enters a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterKind {
    /// Explicit source parameter.
    Explicit,
    /// Method or constructor receiver.
    Receiver,
    /// Value copied into a lambda environment.
    Capture,
}

/// One local storage entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    /// Function-local id.
    pub id: LocalId,
    /// Source spelling retained as an attribute.
    pub source_name: String,
    /// Stored type.
    pub ty: ValueType,
    /// Whether source assignments are accepted.
    pub mutable: bool,
    /// Storage lifetime selected by the HIR-to-LIR lowering.
    pub storage: LocalStorageClass,
    /// Declaration position.
    pub pos: Pos,
}

/// The activation component that owns one local storage entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalStorageClass {
    /// Storage exists only for one uninterrupted function activation.
    Activation,
    /// Storage exists in the coroutine frame through function completion.
    Frame,
}

/// One value entity. Its definition is named separately by a parameter,
/// block parameter, or instruction result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    /// Function-local id.
    pub id: ValueId,
    /// Exact value type.
    pub ty: ValueType,
    /// Whether this value is a fresh async handle or handle-array owner.
    pub fresh_owner: bool,
    /// Optional diagnostic source name; temporaries carry `None`.
    pub source_name: Option<String>,
}

/// Types admitted by LIR values and locals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueType {
    /// An ordinary language value.
    Data(Type),
    /// An addressable language value.
    Address(AddressType),
    /// A fused for-of traversal cursor.
    Iterator(IteratorType),
}

/// Address value metadata used by the invalidation verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressType {
    /// Value stored at the address.
    pub pointee: Type,
    /// Dynamic-array value whose movable storage owns this address.
    pub array_base: Option<ValueId>,
}

/// Type of a fused iteration cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IteratorType {
    /// Traversal operation.
    pub kind: ForOfKind,
    /// Bound element type.
    pub element: Type,
}

/// One basic block. Instructions cannot be terminators, and the terminator
/// field is mandatory, so the representation admits exactly one terminator.
#[derive(Debug, Clone, PartialEq)]
pub struct BasicBlock {
    /// Function-local block id.
    pub id: BlockId,
    /// Optional diagnostic source name.
    pub source_name: Option<String>,
    /// Values defined at block entry.
    pub parameters: Vec<ValueId>,
    /// Ordered non-terminating instructions.
    pub instructions: Vec<Instruction>,
    /// The block's sole terminator.
    pub terminator: Terminator,
}

/// One ordered instruction.
#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    /// Value defined by this instruction, when any.
    pub result: Option<ValueId>,
    /// Closed operation code.
    pub kind: InstructionKind,
    /// Flat operands in evaluation order.
    pub operands: Vec<Operand>,
    /// Dynamic-array values whose storage can move after the operands are
    /// consumed and before the following instruction.
    pub invalidates: Vec<ValueId>,
    /// Ordered semantic trap sites owned by the operation.
    pub traps: Vec<Trap>,
    /// Source position of the operation.
    pub pos: Pos,
}

/// Closed instruction set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstructionKind {
    /// Materialize an otherwise-inline operand as a named value.
    Copy,
    /// Materialize a string literal.
    StringLiteral(String),
    /// Read one local.
    LoadLocal(LocalId),
    /// Write one local.
    StoreLocal(LocalId),
    /// Take stable local storage's address.
    AddressOfLocal(LocalId),
    /// Read one global.
    LoadGlobal(GlobalId),
    /// Write one global.
    StoreGlobal(GlobalId),
    /// Take stable global storage's address.
    AddressOfGlobal(GlobalId),
    /// Materialize a direct function value.
    FunctionRef(FunctionId),
    /// Apply a unary operator.
    Unary(UnaryOp),
    /// Apply a binary operator.
    Binary(BinaryOp),
    /// Apply an explicit typed conversion.
    Cast,
    /// Apply a checker-approved implicit assignment/call conversion.
    Coerce,
    /// Allocate zeroed class storage.
    AllocateClass(ClassId),
    /// Allocate a managed box and copy one boundary value into its payload.
    ///
    /// `payload` equals the result's class for an ordinary box. For an
    /// embedded header, it names the enclosing extension class instead.
    BoxBoundaryValue {
        /// Class whose complete layout and class id the box stores.
        payload: ClassId,
    },
    /// Take a temporary aggregate's address.
    AddressOfValue,
    /// Address one field.
    AddressOfField(FieldRef),
    /// Address one indexed element.
    AddressOfIndex {
        /// Whether a bounds guard is required.
        checked: bool,
    },
    /// Read through an address.
    LoadAddress,
    /// Write through an address.
    StoreAddress,
    /// Read an aggregate/reference field without exposing an address.
    LoadField(FieldRef),
    /// Read array/fixed-array/string length.
    Length,
    /// Snapshot a dynamic array's current data pointer for a foreign call.
    ForeignArrayData,
    /// Construct a non-spread array literal.
    ArrayLiteral,
    /// Allocate an empty dynamic array with the supplied element capacity.
    ArrayWithCapacity,
    /// Construct an array literal with per-operand spread modes.
    ArraySpreadLiteral(Vec<Option<SpreadKind>>),
    /// Format and concatenate a template literal.
    Template(Vec<TemplatePart>),
    /// Construct a function value and its capture environment.
    MakeClosure(FunctionId),
    /// Invoke a resolved call target.
    Call(CallTarget),
    /// Create an async coroutine frame without polling it.
    AsyncHandleCreate(CallTarget),
    /// Increment one async frame's non-atomic owner count.
    AsyncHandleRetain,
    /// Decrement one async frame's owner count and free it at zero.
    AsyncHandleRelease,
    /// Retain each async handle stored in one dynamic array.
    AsyncHandleArrayRetain,
    /// Release each async handle stored in one dynamic array.
    AsyncHandleArrayRelease,
    /// Create a fused iteration cursor with its source-selected bound.
    IteratorCreate {
        /// Storage traversal operation.
        kind: ForOfKind,
        /// Bound rule selected by the source spelling.
        bound: IteratorBoundKind,
    },
    /// Test a fused cursor without advancing it.
    IteratorHasNext,
    /// Read the element at the current traversal state.
    IteratorValue,
    /// Read the fixed traversal bound captured at creation.
    IteratorBound,
    /// Advance a traversal and produce its next cursor state.
    IteratorAdvance,
    /// Checker-internal typed zero.
    Zero,
}

impl InstructionKind {
    /// Reports whether this instruction kind can produce a fresh async owner.
    pub fn produces_fresh_async_owner(&self) -> bool {
        matches!(
            self,
            Self::ArrayLiteral
                | Self::ArrayWithCapacity
                | Self::ArraySpreadLiteral(_)
                | Self::Call(_)
                | Self::AsyncHandleCreate(_)
        )
    }
}

/// A class field or a built-in aggregate field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldRef {
    /// Declared class field.
    Class(FieldId),
    /// `IterResult.done`.
    IterDone,
    /// `IterResult.value`.
    IterValue,
}

/// One template instruction segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePart {
    /// Literal bytes.
    Text(String),
    /// Index into the instruction's operand list.
    Operand(u32),
}

/// A resolved call target. Declared functions carry their complete typed
/// calling contract here; intrinsic and built-in targets use the module's
/// checker-derived signature table instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTarget {
    /// Executable target identity.
    pub kind: CallTargetKind,
    /// Flat operand types for declared targets, including a receiver or
    /// indirect callee when applicable. Empty for intrinsics and built-ins.
    pub parameter_types: Vec<ValueType>,
    /// Result type, absent for void calls.
    pub return_type: Option<ValueType>,
}

/// Closed call-target set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallTargetKind {
    /// Direct script function.
    Function(FunctionId),
    /// Direct lambda body with its callable environment as operand zero.
    StaticClosure(FunctionId),
    /// Direct class method or constructor.
    Method(MethodId),
    /// Foreign C-ABI function.
    Foreign(ForeignFunctionId),
    /// A function value supplied as operand zero.
    Indirect,
    /// Checker/runtime intrinsic.
    Intrinsic(Intrinsic),
    /// Built-in receiver method that is not a class method.
    BuiltinMethod(BuiltinMethod),
}

/// An intrinsic's stable family and operation number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intrinsic {
    /// Intrinsic family.
    pub family: IntrinsicFamily,
    /// Declaration-order operation code within the family.
    pub operation: u16,
    /// Explicit monomorphized type argument, when the operation has one.
    pub type_argument: Option<Type>,
    /// Worker-entry index for `Worker.spawn`, otherwise absent.
    pub worker_entry: Option<u32>,
}

/// One valid intrinsic operation. The table is part of LIR, so an operation
/// number never acquires meaning from the order of a Rust `ALL` array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntrinsicOperation {
    /// Intrinsic family.
    pub family: IntrinsicFamily,
    /// Family-local operation number used by call targets.
    pub operation: u16,
    /// Stable semantic variant name carried by LIR.
    pub semantic_name: String,
    /// Shard of the checker-derived intrinsic and built-in signature table.
    pub signatures: Vec<CallSignature>,
}

/// One checker-derived intrinsic or built-in call signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSignature {
    /// Operation whose call contract this row declares.
    pub target: CallSignatureTarget,
    /// Operand types in execution order.
    pub parameter_types: Vec<ValueType>,
    /// Result type, absent for a void operation.
    pub return_type: Option<ValueType>,
}

/// An operation identity in the checker-derived call signature table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallSignatureTarget {
    /// Checker/runtime intrinsic.
    Intrinsic(Intrinsic),
    /// Built-in receiver method.
    BuiltinMethod(BuiltinMethod),
}

/// Closed intrinsic families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicFamily {
    /// Ambient prelude functions.
    Ambient,
    /// Typed Context byte operations.
    ContextBytes,
    /// Math operations.
    Math,
    /// Number/formatting operations.
    Number,
    /// Date operations.
    Date,
    /// JSON helper leaves.
    Json,
    /// String operations.
    String,
    /// Regular-expression operations.
    Regex,
    /// Dynamic-array operations.
    Array,
    /// Map operations.
    Map,
    /// Set operations.
    Set,
    /// Worker/channel operations.
    Worker,
}

/// Built-in receiver operations represented by HIR method syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinMethod {
    /// Dynamic-array `push`.
    ArrayPush,
    /// Dynamic-array `pop`.
    ArrayPop,
    /// String `slice` method form.
    StringSlice,
    /// Generator `next`.
    GeneratorNext,
}

impl Module {
    /// Returns the table signatures for an intrinsic or built-in target.
    pub fn operation_signatures(
        &self,
        kind: &CallTargetKind,
    ) -> impl Iterator<Item = &CallSignature> {
        let target = match kind {
            CallTargetKind::Intrinsic(intrinsic) => {
                Some(CallSignatureTarget::Intrinsic(intrinsic.clone()))
            }
            CallTargetKind::BuiltinMethod(method) => {
                Some(CallSignatureTarget::BuiltinMethod(*method))
            }
            CallTargetKind::Function(_)
            | CallTargetKind::StaticClosure(_)
            | CallTargetKind::Method(_)
            | CallTargetKind::Foreign(_)
            | CallTargetKind::Indirect => None,
        };
        self.intrinsic_operations
            .iter()
            .flat_map(|operation| &operation.signatures)
            .filter(move |signature| target.as_ref() == Some(&signature.target))
    }
}

/// A value use or typed constant.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    /// Named value use.
    Value(ValueId),
    /// Inline typed constant.
    Constant(Constant),
}

/// One typed inline constant.
#[derive(Debug, Clone, PartialEq)]
pub struct Constant {
    /// Constant's exact language type.
    pub ty: Type,
    /// Constant payload.
    pub kind: ConstantKind,
}

/// Constant payloads that do not allocate or nest expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstantKind {
    /// Sized integer, enum, date, or string-alias representation.
    Integer(i64),
    /// Floating-point bits, preserving NaN payloads and signed zero. For
    /// `f16`, these are the source `f64` bits consumed by the opaque
    /// round-to-binary16 operation.
    FloatBits(u64),
    /// Boolean value.
    Boolean(bool),
    /// Null reference.
    Null,
}

/// Unary operation codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Numeric negation.
    Neg,
    /// Boolean negation.
    Not,
    /// Integer bitwise complement.
    BitNot,
}

/// Binary operation codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Addition or string concatenation.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Remainder.
    Rem,
    /// Equality.
    Eq,
    /// Inequality.
    Ne,
    /// Less-than.
    Lt,
    /// Less-than-or-equal.
    Le,
    /// Greater-than.
    Gt,
    /// Greater-than-or-equal.
    Ge,
    /// Integer bitwise and.
    BitAnd,
    /// Integer bitwise or.
    BitOr,
    /// Integer bitwise xor.
    BitXor,
    /// Left shift.
    Shl,
    /// Arithmetic right shift.
    Shr,
    /// Logical right shift.
    UShr,
}

/// Fused for-of traversal kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForOfKind {
    /// Dynamic-array values.
    ArrayValues,
    /// Dynamic-array indices.
    ArrayKeys,
    /// Dynamic-array values from the initial last index to zero.
    ArrayValuesReverse,
    /// Dynamic-array indices from the initial last index to zero.
    ArrayKeysReverse,
    /// Fixed-array values.
    FixedArrayValues,
    /// Map keys.
    MapKeys,
    /// Map values.
    MapValues,
    /// Set values.
    SetValues,
    /// UTF-8 code points.
    StringCodePoints,
}

/// Source-selected bound rule for one fused iteration cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IteratorBoundKind {
    /// Compare the cursor position with the container's current count.
    Live,
    /// Compare the cursor position with the count captured at creation.
    Fixed,
}

/// Array spread traversal kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpreadKind {
    /// Dynamic-array values.
    Array,
    /// Fixed-array values.
    FixedArray,
    /// Map keys.
    MapKeys,
    /// Set values.
    SetValues,
    /// String code points.
    StringCodePoints,
}

/// A block's sole control-flow terminator.
#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    /// Unconditional branch.
    Branch(BlockTarget),
    /// Boolean conditional branch.
    ConditionalBranch {
        /// Boolean condition.
        condition: Operand,
        /// True edge.
        then_target: BlockTarget,
        /// False edge.
        else_target: BlockTarget,
    },
    /// Integer/enum/string-alias switch.
    Switch {
        /// Discriminant.
        value: Operand,
        /// Explicit case edges.
        arms: Vec<SwitchArm>,
        /// Default/no-match edge.
        default: BlockTarget,
    },
    /// Function return.
    Return {
        /// Optional returned value.
        value: Option<Operand>,
        /// Source position of the return operation.
        pos: Pos,
    },
    /// A control-flow successor that checked semantics prove unreachable.
    ///
    /// This is a structural terminator. It does not add a language trap site.
    Unreachable {
        /// Position of the proof-producing construct.
        pos: Pos,
    },
    /// Unconditional semantic trap.
    Trap(Trap),
    /// Coroutine suspension; resumption continues at a named block id.
    Suspend {
        /// Suspension operation.
        kind: SuspendKind,
        /// Source position of the suspension operation.
        pos: Pos,
        /// Resume successor.
        successor: BlockId,
        /// Optional value defined as the successor's first block parameter.
        resume_value: Option<ValueId>,
        /// Values supplied to the successor parameters after the optional
        /// resume value.
        arguments: Vec<Operand>,
        /// Dynamic-array values invalidated before suspension completes.
        invalidates: Vec<ValueId>,
        /// Ordered trap sites owned by starting/resuming the operation.
        traps: Vec<Trap>,
    },
}

impl Terminator {
    /// Returns every control-flow target in edge order.
    #[must_use]
    pub fn targets(&self) -> Vec<BlockTarget> {
        match self {
            Self::Branch(target) => vec![target.clone()],
            Self::ConditionalBranch {
                then_target,
                else_target,
                ..
            } => vec![then_target.clone(), else_target.clone()],
            Self::Switch { arms, default, .. } => arms
                .iter()
                .map(|arm| arm.target.clone())
                .chain(std::iter::once(default.clone()))
                .collect(),
            Self::Suspend {
                successor,
                arguments,
                ..
            } => vec![BlockTarget {
                block: *successor,
                arguments: arguments.clone(),
            }],
            Self::Return { .. } | Self::Unreachable { .. } | Self::Trap(_) => Vec::new(),
        }
    }

    /// Returns every successor block id in edge order.
    #[must_use]
    pub fn successors(&self) -> Vec<BlockId> {
        self.targets()
            .into_iter()
            .map(|target| target.block)
            .collect()
    }

    /// Returns every value that the terminator reads.
    ///
    /// Suspend invalidations name movable storage. The resume value is a
    /// definition. Neither is a read.
    #[must_use]
    pub fn value_uses(&self) -> Vec<ValueId> {
        let mut values = Vec::new();
        let mut push_operand = |operand: &Operand| {
            if let Operand::Value(value) = operand {
                values.push(*value);
            }
        };
        match self {
            Self::Branch(target) => target.arguments.iter().for_each(&mut push_operand),
            Self::ConditionalBranch {
                condition,
                then_target,
                else_target,
            } => {
                push_operand(condition);
                then_target.arguments.iter().for_each(&mut push_operand);
                else_target.arguments.iter().for_each(&mut push_operand);
            }
            Self::Switch {
                value,
                arms,
                default,
            } => {
                push_operand(value);
                for arm in arms {
                    arm.target.arguments.iter().for_each(&mut push_operand);
                }
                default.arguments.iter().for_each(&mut push_operand);
            }
            Self::Return { value, .. } => {
                if let Some(value) = value {
                    push_operand(value);
                }
            }
            Self::Suspend {
                kind, arguments, ..
            } => {
                arguments.iter().for_each(&mut push_operand);
                match kind {
                    SuspendKind::Yield(value) => values.extend(value),
                    SuspendKind::Async => {}
                    SuspendKind::AsyncCall { operands, .. } => {
                        values.extend(operands.iter().copied());
                    }
                    SuspendKind::AsyncHandle { handle } => values.push(*handle),
                }
            }
            Self::Unreachable { .. } | Self::Trap(_) => {}
        }
        values
    }

    /// Replaces each read or invalidation value with the mapper's result.
    ///
    /// The resume value is a definition and is not mapped.
    pub fn map_values(&mut self, mut map: impl FnMut(ValueId) -> ValueId) {
        let map_operand = |operand: &mut Operand, map: &mut dyn FnMut(ValueId) -> ValueId| {
            if let Operand::Value(value) = operand {
                *value = map(*value);
            }
        };
        match self {
            Self::Branch(target) => {
                for operand in &mut target.arguments {
                    map_operand(operand, &mut map);
                }
            }
            Self::ConditionalBranch {
                condition,
                then_target,
                else_target,
            } => {
                map_operand(condition, &mut map);
                for operand in then_target
                    .arguments
                    .iter_mut()
                    .chain(&mut else_target.arguments)
                {
                    map_operand(operand, &mut map);
                }
            }
            Self::Switch {
                value,
                arms,
                default,
            } => {
                map_operand(value, &mut map);
                for arm in arms {
                    for operand in &mut arm.target.arguments {
                        map_operand(operand, &mut map);
                    }
                }
                for operand in &mut default.arguments {
                    map_operand(operand, &mut map);
                }
            }
            Self::Return { value, .. } => {
                if let Some(value) = value {
                    map_operand(value, &mut map);
                }
            }
            Self::Suspend {
                kind,
                arguments,
                invalidates,
                ..
            } => {
                for argument in arguments {
                    map_operand(argument, &mut map);
                }
                for value in invalidates {
                    *value = map(*value);
                }
                match kind {
                    SuspendKind::Yield(value) => {
                        if let Some(value) = value {
                            *value = map(*value);
                        }
                    }
                    SuspendKind::Async => {}
                    SuspendKind::AsyncCall { operands, .. } => {
                        for value in operands {
                            *value = map(*value);
                        }
                    }
                    SuspendKind::AsyncHandle { handle } => *handle = map(*handle),
                }
            }
            Self::Unreachable { .. } | Self::Trap(_) => {}
        }
    }

    /// Returns the source position for a terminator that can own a trap site.
    #[must_use]
    pub fn trap_site_position(&self) -> Option<&Pos> {
        match self {
            Self::Return { pos, .. } | Self::Suspend { pos, .. } => Some(pos),
            Self::Trap(trap) => Some(&trap.pos),
            Self::Branch(_) | Self::ConditionalBranch { .. } | Self::Switch { .. } => None,
            Self::Unreachable { .. } => None,
        }
    }
}

/// A branch edge and its block-parameter arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockTarget {
    /// Destination block.
    pub block: BlockId,
    /// Values supplied to ordinary destination parameters.
    pub arguments: Vec<Operand>,
}

/// One switch case edge.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchArm {
    /// Constant case value.
    pub value: Constant,
    /// Destination block.
    pub target: BlockTarget,
}

/// Suspension operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuspendKind {
    /// Generator yield with an optional yielded value.
    Yield(Option<ValueId>),
    /// Explicit async scheduler suspension.
    Async,
    /// Direct awaited async call.
    AsyncCall {
        /// Resolved async target.
        target: CallTarget,
        /// Flat call operands.
        operands: Vec<ValueId>,
    },
    /// Await a previously created async handle.
    AsyncHandle {
        /// The handle value to poll.
        handle: ValueId,
    },
}

/// One semantic trap point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trap {
    /// Trap/check operation.
    pub kind: TrapKind,
    /// Source position reported to the user.
    pub pos: Pos,
}

/// Closed trap/check set inherited from checker semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrapKind {
    /// Allocation failure.
    Allocation,
    /// A call left the Context trapped.
    Call,
    /// Reached source `unreachable()`.
    Unreachable,
    /// Integer divisor is zero.
    DivisionByZero,
    /// Bounds-checked index read.
    IndexRead,
    /// Bounds-checked index write.
    IndexWrite,
    /// Failed `JsonResult.value` guard, with the boolean field it reads.
    JsonResultValue(FieldId),
    /// Failed null narrowing.
    NullNarrowing,
    /// Failed reference-class narrowing.
    ClassMismatch(ClassId),
    /// Development-tier allocation lifetime check.
    DevOnlyLifetime,
    /// Reload-only stale coroutine check.
    DevReloadOnlyStaleCoroutine,
    /// Invalid C-entered wire alias value.
    WireEnumValue(StringAliasId),
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn pos() -> Pos {
        Pos::new("terminator.ts", 1, 1)
    }

    fn operand(value: u32) -> Operand {
        Operand::Value(ValueId(value))
    }

    fn target(block: u32, values: &[u32]) -> BlockTarget {
        BlockTarget {
            block: BlockId(block),
            arguments: values.iter().copied().map(operand).collect(),
        }
    }

    fn async_target() -> CallTarget {
        CallTarget {
            kind: CallTargetKind::Function(FunctionId(0)),
            parameter_types: Vec::new(),
            return_type: None,
        }
    }

    #[test]
    fn fresh_async_owner_instruction_table_names_allocations_and_calls() {
        assert!(InstructionKind::ArrayLiteral.produces_fresh_async_owner());
        assert!(InstructionKind::ArrayWithCapacity.produces_fresh_async_owner());
        assert!(InstructionKind::ArraySpreadLiteral(Vec::new()).produces_fresh_async_owner());
        assert!(InstructionKind::Call(async_target()).produces_fresh_async_owner());
        assert!(InstructionKind::AsyncHandleCreate(async_target()).produces_fresh_async_owner());
        assert!(!InstructionKind::Copy.produces_fresh_async_owner());
    }

    #[test]
    fn terminator_walks_yield_all_targets_and_read_values() {
        let cases = vec![
            (
                Terminator::Branch(target(1, &[0])),
                vec![target(1, &[0])],
                vec![0],
            ),
            (
                Terminator::ConditionalBranch {
                    condition: operand(1),
                    then_target: target(2, &[2]),
                    else_target: target(3, &[3]),
                },
                vec![target(2, &[2]), target(3, &[3])],
                vec![1, 2, 3],
            ),
            (
                Terminator::Switch {
                    value: operand(4),
                    arms: vec![
                        SwitchArm {
                            value: Constant {
                                ty: Type::I32,
                                kind: ConstantKind::Integer(0),
                            },
                            target: target(4, &[5]),
                        },
                        SwitchArm {
                            value: Constant {
                                ty: Type::I32,
                                kind: ConstantKind::Integer(1),
                            },
                            target: target(5, &[6]),
                        },
                    ],
                    default: target(6, &[7]),
                },
                vec![target(4, &[5]), target(5, &[6]), target(6, &[7])],
                vec![4, 5, 6, 7],
            ),
            (
                Terminator::Return {
                    value: Some(operand(8)),
                    pos: pos(),
                },
                Vec::new(),
                vec![8],
            ),
            (
                Terminator::Unreachable { pos: pos() },
                Vec::new(),
                Vec::new(),
            ),
            (
                Terminator::Trap(Trap {
                    kind: TrapKind::Unreachable,
                    pos: pos(),
                }),
                Vec::new(),
                Vec::new(),
            ),
            (
                Terminator::Suspend {
                    kind: SuspendKind::Yield(Some(ValueId(10))),
                    pos: pos(),
                    successor: BlockId(7),
                    resume_value: Some(ValueId(90)),
                    arguments: vec![operand(9)],
                    invalidates: vec![ValueId(91)],
                    traps: Vec::new(),
                },
                vec![target(7, &[9])],
                vec![9, 10],
            ),
            (
                Terminator::Suspend {
                    kind: SuspendKind::Async,
                    pos: pos(),
                    successor: BlockId(8),
                    resume_value: None,
                    arguments: vec![operand(11)],
                    invalidates: vec![ValueId(92)],
                    traps: Vec::new(),
                },
                vec![target(8, &[11])],
                vec![11],
            ),
            (
                Terminator::Suspend {
                    kind: SuspendKind::AsyncCall {
                        target: async_target(),
                        operands: vec![ValueId(13), ValueId(14)],
                    },
                    pos: pos(),
                    successor: BlockId(9),
                    resume_value: Some(ValueId(93)),
                    arguments: vec![operand(12)],
                    invalidates: vec![ValueId(94)],
                    traps: Vec::new(),
                },
                vec![target(9, &[12])],
                vec![12, 13, 14],
            ),
            (
                Terminator::Suspend {
                    kind: SuspendKind::AsyncHandle {
                        handle: ValueId(16),
                    },
                    pos: pos(),
                    successor: BlockId(10),
                    resume_value: Some(ValueId(95)),
                    arguments: vec![operand(15)],
                    invalidates: vec![ValueId(96)],
                    traps: Vec::new(),
                },
                vec![target(10, &[15])],
                vec![15, 16],
            ),
        ];

        for (terminator, targets, values) in cases {
            assert_eq!(terminator.targets(), targets);
            assert_eq!(
                terminator.successors(),
                targets
                    .iter()
                    .map(|target| target.block)
                    .collect::<Vec<_>>()
            );
            let actual = terminator.value_uses();
            assert_eq!(actual, values.into_iter().map(ValueId).collect::<Vec<_>>());
            assert_eq!(actual.len(), actual.iter().collect::<HashSet<_>>().len());
        }
    }

    #[test]
    fn map_values_maps_reads_and_invalidations_but_not_resume_definitions() {
        let mut terminator = Terminator::Suspend {
            kind: SuspendKind::AsyncCall {
                target: async_target(),
                operands: vec![ValueId(2), ValueId(3)],
            },
            pos: pos(),
            successor: BlockId(1),
            resume_value: Some(ValueId(4)),
            arguments: vec![operand(1)],
            invalidates: vec![ValueId(5)],
            traps: Vec::new(),
        };
        terminator.map_values(|value| ValueId(value.0 + 10));

        assert_eq!(
            terminator.value_uses(),
            vec![ValueId(11), ValueId(12), ValueId(13)]
        );
        let Terminator::Suspend {
            resume_value,
            invalidates,
            ..
        } = terminator
        else {
            panic!("test terminator changed kind");
        };
        assert_eq!(resume_value, Some(ValueId(4)));
        assert_eq!(invalidates, vec![ValueId(15)]);
    }
}
