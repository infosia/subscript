//! The store fact of instructions (`compiler.md` §356 rule 13).
//!
//! [`instruction_block_store`] states, by a total match over
//! [`l::InstructionKind`], whether the emitted code of one instruction
//! writes a word into memory that the mark reads as a payload: a field, an
//! element, a frame of a coroutine, a captured variable (rule 10). A store
//! into a root (a C local, a root slot, a module variable) has no fact
//! (rule 11). [`terminator_block_store`] and [`entry_block_store`] state
//! the same fact for a terminator and for the entry of a coroutine.
//!
//! If the LIR cannot tell whether a destination is inside a block, the fact
//! is that it can be: an extra barrier is harmless. The static type of the
//! stored word is a separate fact; the C emission derives the barrier from
//! the two.
//!
//! The facts are an analysis result, not a stored field, as the allocation
//! facts of `compiler.md` §342 rule 10 are.

use super::*;

/// The store fact of one instruction (`compiler.md` §356 rule 13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BlockStore {
    /// The emitted code of the instruction writes no word into a block.
    /// A write of the runtime has its barrier in the runtime (rule 14).
    None,
    /// The instruction writes the operand at this index into memory that
    /// can be inside a block.
    Operand(usize),
    /// The instruction writes each of its operands into memory that can be
    /// inside a block: the captures of a closure in a coroutine frame.
    EachOperand,
    /// The instruction can write through each of its address operands
    /// whose target can be inside a block: a call with an out-parameter.
    ThroughAddresses,
}

/// True if a function keeps its locals and its saved values in a frame
/// block.
#[must_use]
pub fn is_coroutine(function: &l::Function) -> bool {
    function.is_generator || function.is_async
}

/// The defining instruction of each value of a function.
pub type Definitions<'f> = HashMap<l::ValueId, &'f l::Instruction>;

/// Collects the instructions that define a value.
#[must_use]
pub fn definitions(function: &l::Function) -> Definitions<'_> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| instruction.result.map(|id| (id, instruction)))
        .collect()
}

fn value_type(function: &l::Function, id: l::ValueId) -> Option<&l::ValueType> {
    function
        .values
        .get(id.0 as usize)
        .filter(|value| value.id == id)
        .or_else(|| function.values.iter().find(|value| value.id == id))
        .map(|value| &value.ty)
}

/// True if a value of this type is a C value that holds its fields in
/// place: a value class, a fixed array, or an iterator result. An address
/// derived from such a value is inside the storage of that value.
fn is_inline_aggregate(module: &l::Module, ty: &Type) -> bool {
    match ty {
        Type::Class(class) => module
            .classes
            .get(class.0)
            .is_some_and(|definition| definition.is_value),
        Type::FixedArray(..) | Type::IterResult(_) => true,
        _ => false,
    }
}

/// The most address steps that [`address_can_be_in_block`] follows. A
/// longer chain can be inside a block.
const ADDRESS_DEPTH: usize = 64;

/// True if the address `operand` can name memory inside a block
/// (`compiler.md` §356 rules 10 and 11). The address of a local is inside
/// a block only in a coroutine frame. The address of a module variable is
/// a root. A field or an element of a reference is inside a block. A field
/// or an element of an inline aggregate is where the aggregate is. A
/// parameter, a block parameter, and each other origin can be inside a
/// block.
#[must_use]
pub fn address_can_be_in_block(
    module: &l::Module,
    function: &l::Function,
    definitions: &Definitions<'_>,
    operand: &l::Operand,
) -> bool {
    let mut operand = operand.clone();
    for _ in 0..ADDRESS_DEPTH {
        let l::Operand::Value(id) = operand else {
            return true;
        };
        let Some(definition) = definitions.get(&id) else {
            return true;
        };
        use l::InstructionKind as K;
        let base = match &definition.kind {
            K::AddressOfLocal(local) => {
                return function
                    .locals
                    .get(local.0 as usize)
                    .is_none_or(|local| local.storage == l::LocalStorageClass::Frame);
            }
            K::AddressOfGlobal(_) => return false,
            K::AddressOfValue => return is_coroutine(function),
            // A value class at an address is zeroed local or frame storage
            // (`compiler.md` §344 rule 7).
            K::AllocateClass(class)
                if module
                    .classes
                    .get(class.0)
                    .is_some_and(|definition| definition.is_value) =>
            {
                return is_coroutine(function);
            }
            K::AddressOfField(_) | K::AddressOfIndex { .. } => definition.operands.first(),
            _ => return true,
        };
        let Some(base) = base else {
            return true;
        };
        let base_type = match base {
            l::Operand::Value(base) => value_type(function, *base),
            l::Operand::Constant(_) => None,
        };
        match base_type {
            Some(l::ValueType::Address(_)) => operand = base.clone(),
            // A field or an element of an inline aggregate held in a C
            // value is inside that value.
            Some(l::ValueType::Data(ty)) if is_inline_aggregate(module, ty) => return false,
            _ => return true,
        }
    }
    true
}

/// True if a call target can write through an address operand. A script
/// function writes through its address parameter with its own fact, and a
/// foreign function needs no barrier (`compiler.md` §356 rule 15).
fn call_writes_through_addresses(target: &l::CallTarget) -> bool {
    match &target.kind {
        l::CallTargetKind::Function(_)
        | l::CallTargetKind::LambdaSelf(_)
        | l::CallTargetKind::StaticClosure(_)
        | l::CallTargetKind::ClosureDefault { .. }
        | l::CallTargetKind::Method(_)
        | l::CallTargetKind::Indirect
        | l::CallTargetKind::Foreign(_) => false,
        l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_) => true,
    }
}

/// Returns the store fact of one instruction (`compiler.md` §356 rule 13).
///
/// The match is total, so a new instruction kind cannot exist without its
/// fact. A result is an SSA value, which is a C local or a root slot, so an
/// instruction that only defines a result has no fact.
#[must_use]
pub fn instruction_block_store(
    module: &l::Module,
    function: &l::Function,
    definitions: &Definitions<'_>,
    instruction: &l::Instruction,
) -> BlockStore {
    use l::InstructionKind as K;
    match &instruction.kind {
        K::StoreLocal(local) => {
            if function
                .locals
                .get(local.0 as usize)
                .is_some_and(|local| local.storage == l::LocalStorageClass::Frame)
            {
                BlockStore::Operand(0)
            } else {
                BlockStore::None
            }
        }
        K::StoreAddress => match instruction.operands.first() {
            Some(address) if address_can_be_in_block(module, function, definitions, address) => {
                BlockStore::Operand(1)
            }
            _ => BlockStore::None,
        },
        // A stable value of a coroutine lives in its frame.
        K::AddressOfValue if is_coroutine(function) => BlockStore::Operand(0),
        // The captures of a closure in a coroutine live in its frame; else
        // they live in a root slot.
        K::MakeClosure(_) if is_coroutine(function) => BlockStore::EachOperand,
        // The emitted code copies the value into the payload of a new box.
        // No accepted type reaches a barrier here: the payload is a value
        // class, which cannot hold a reference (S100).
        K::BoxBoundaryValue { .. } => BlockStore::Operand(0),
        K::Call(target) => match &target.kind {
            // The inline path writes the element into the array storage.
            l::CallTargetKind::BuiltinMethod(
                l::BuiltinMethod::ArrayPush | l::BuiltinMethod::ArrayPushReserved,
            ) => BlockStore::Operand(1),
            // compiler.md §359 rule 10: an in-place `filter` writes its
            // kept element (operand 2) into the buffer of the receiver.
            // The end step only removes references.
            l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayStoreKept) => {
                BlockStore::Operand(2)
            }
            l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayTruncateKept) => {
                BlockStore::None
            }
            _ if call_writes_through_addresses(target) => BlockStore::ThroughAddresses,
            _ => BlockStore::None,
        },
        K::StoreGlobal(_)
        | K::AddressOfValue
        | K::MakeClosure(_)
        | K::Copy
        | K::StringLiteral(_)
        | K::AliasString(_)
        | K::LoadLocal(_)
        | K::AddressOfLocal(_)
        | K::LoadGlobal(_)
        | K::AddressOfGlobal(_)
        | K::FunctionRef(_)
        | K::Unary(_)
        | K::Binary(_)
        | K::Cast
        | K::Coerce
        | K::StringOperand
        | K::NarrowNonNull(_)
        | K::AllocateClass(_)
        | K::AddressOfField(_)
        | K::AddressOfIndex { .. }
        | K::LoadAddress
        | K::LoadField(_)
        | K::Length
        | K::ArrayLiteral
        | K::ArrayWithCapacity
        | K::ArraySpreadLiteral(_)
        | K::SetFromSource(_)
        | K::MapFromSource
        | K::Template(_)
        | K::AsyncHandleCreate(_)
        | K::AsyncAll
        | K::AsyncRace
        | K::HostCompletion { .. }
        | K::TaskGroup(_)
        | K::AsyncHandleRetain
        | K::AsyncHandleRelease
        | K::AsyncHandleArrayRetain
        | K::AsyncHandleArrayRelease
        | K::IteratorCreate { .. }
        | K::IteratorHasNext
        | K::IteratorValue
        | K::IteratorBound
        | K::IteratorAdvance
        | K::Zero
        | K::Throw
        | K::CatchEntry
        | K::ExceptionPark
        | K::ExceptionResume
        | K::ExceptionMessage
        | K::ExceptionPosition
        | K::ExceptionRestore
        | K::FinalizerEnter(_)
        | K::GeneratorClose
        | K::GeneratorIsClosing
        | K::GeneratorFinalizer(_)
        | K::AwaitRaise
        | K::CollectionPoll => BlockStore::None,
    }
}

/// Returns true if the emitted code of a terminator writes a word into a
/// block (`compiler.md` §356 rule 13). A suspension saves the arguments of
/// its successor and the awaited handle in the coroutine frame. A yielded
/// value goes to the destination that the resumer supplies, a C local of
/// generated code or a buffer of the runtime.
#[must_use]
pub fn terminator_block_store(terminator: &l::Terminator) -> bool {
    match terminator {
        l::Terminator::Branch(_)
        | l::Terminator::ConditionalBranch { .. }
        | l::Terminator::Switch { .. }
        | l::Terminator::Return { .. }
        | l::Terminator::Unreachable { .. }
        | l::Terminator::Trap(_) => false,
        l::Terminator::Suspend { .. } => true,
    }
}

/// Returns true if the entry of the function writes its parameters into a
/// block (`compiler.md` §356 rule 13): the creation of a coroutine stores
/// each parameter in the new frame.
#[must_use]
pub fn entry_block_store(function: &l::Function) -> bool {
    is_coroutine(function)
}

#[cfg(test)]
#[path = "block_store_tests.rs"]
mod tests;
