//! Nullable conversion checks from LIR origins and types (compiler.md §124).

use super::verify::{finding, operand_type, value_type};
use super::*;

pub(super) fn verify_narrowing(function: &l::Function, errors: &mut Vec<VerifyError>) {
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let required = if let l::InstructionKind::NarrowNonNull(origin) = instruction.kind {
                match origin {
                    l::NarrowOrigin::SharedRead => Some(l::TrapKind::SharedNullNarrowing),
                    l::NarrowOrigin::Local => None,
                }
            } else {
                None
            };
            if let Some(kind) = required {
                if !instruction.traps.iter().any(|trap| trap.kind == kind) {
                    errors.push(finding(
                        function,
                        format!(
                            "block {} instruction {index} {:?} requires {kind:?}",
                            block.id.0, instruction.kind
                        ),
                    ));
                }
            }
            let target = instruction
                .result
                .and_then(|value| value_type(function, value));
            let converts = is_conversion(&instruction.kind)
                && instruction.operands.iter().any(|operand| {
                    let Some(l::ValueType::Data(Type::Nullable(source))) =
                        operand_type(function, operand)
                    else {
                        return false;
                    };
                    match target {
                        Some(l::ValueType::Data(target)) => source.as_ref() == target,
                        Some(l::ValueType::Address(address)) => source.as_ref() == &address.pointee,
                        _ => false,
                    }
                });
            if converts && !matches!(instruction.kind, l::InstructionKind::NarrowNonNull(_)) {
                errors.push(finding(function, format!("block {} instruction {index} nullable-to-value conversion requires NarrowNonNull: {:?}", block.id.0, instruction.kind)));
            }
        }
    }
}

// compiler.md §124: a call's argument and result types do not make it a conversion.
pub(super) fn is_conversion(kind: &l::InstructionKind) -> bool {
    use l::InstructionKind as K;
    match kind {
        K::Copy | K::Cast | K::Coerce | K::NarrowNonNull(_) => true,
        K::StringLiteral(_)
        | K::LoadLocal(_)
        | K::StoreLocal(_)
        | K::AddressOfLocal(_)
        | K::LoadGlobal(_)
        | K::StoreGlobal(_)
        | K::AddressOfGlobal(_)
        | K::FunctionRef(_)
        | K::Unary(_)
        | K::Binary(_)
        | K::AllocateClass(_)
        | K::BoxBoundaryValue { .. }
        | K::AddressOfValue
        | K::AddressOfField(_)
        | K::AddressOfIndex { .. }
        | K::LoadAddress
        | K::StoreAddress
        | K::LoadField(_)
        | K::Length
        | K::ForeignArrayData
        | K::ArrayLiteral
        | K::ArrayWithCapacity
        | K::ArraySpreadLiteral(_)
        | K::SetFromSource(_)
        | K::MapFromSource
        | K::Template(_)
        | K::MakeClosure(_)
        | K::Call(_)
        | K::AsyncHandleCreate(_)
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
        | K::AwaitRaise => false,
    }
}
