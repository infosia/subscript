//! Count obligations come from the operation and the static element type.

use super::verify::{
    counted_instruction_stores, counted_terminator_stores, finding, operand_type, value_type,
};
use super::*;
use std::collections::HashSet;

pub(super) fn verify(module: &l::Module, function: &l::Function, errors: &mut Vec<VerifyError>) {
    let origin = |value: l::ValueId| {
        function
            .liveness
            .value_origins
            .get(value.0 as usize)
            .copied()
            .unwrap_or(value)
    };
    let mut consumed = HashSet::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            let owners = counted_instruction_stores(module, function, instruction)
                .into_iter()
                .map(|(_, operand)| operand);
            for operand in owners {
                if let l::Operand::Value(value) = operand {
                    consumed.insert(origin(*value));
                }
            }
            if matches!(
                instruction.kind,
                l::InstructionKind::AsyncHandleRelease
                    | l::InstructionKind::AsyncHandleArrayRelease
            ) {
                if let Some(l::Operand::Value(value)) = instruction.operands.first() {
                    consumed.insert(origin(*value));
                }
            }
        }
        for (_, operand) in counted_terminator_stores(function, &block.terminator) {
            if let l::Operand::Value(value) = operand {
                consumed.insert(origin(value));
            }
        }
        if let l::Terminator::Suspend {
            kind:
                l::SuspendKind::AsyncHandle {
                    handle,
                    owned: true,
                },
            ..
        } = &block.terminator
        {
            consumed.insert(origin(*handle));
        }
    }
    loop {
        let before = consumed.len();
        for block in &function.blocks {
            let offset = usize::from(matches!(
                block.terminator,
                l::Terminator::Suspend {
                    resume_value: Some(_),
                    ..
                }
            ));
            for target in block.terminator.targets() {
                if let Some(destination) = function.blocks.get(target.block.0 as usize) {
                    for (parameter, argument) in destination
                        .parameters
                        .iter()
                        .skip(offset)
                        .zip(&target.arguments)
                    {
                        if consumed.contains(&origin(*parameter)) {
                            if let l::Operand::Value(value) = argument {
                                consumed.insert(origin(*value));
                            }
                        }
                    }
                }
            }
        }
        if consumed.len() == before {
            break;
        }
    }
    for value in &function.values {
        if value.fresh_owner
            && is_async_owner_type(&value.ty)
            && !consumed.contains(&origin(value.id))
        {
            errors.push(finding(
                function,
                format!("fresh counted owner {} is not consumed", value.id.0),
            ));
        }
    }
    for source in &function.blocks {
        if let l::Terminator::Suspend {
            kind: l::SuspendKind::AsyncCall { .. } | l::SuspendKind::AsyncHandle { .. },
            successor,
            ..
        } = &source.terminator
        {
            if function
                .blocks
                .get(successor.0 as usize)
                .and_then(|block| block.instructions.first())
                .is_none_or(|instruction| instruction.kind != l::InstructionKind::AwaitRaise)
            {
                errors.push(finding(
                    function,
                    format!(
                        "block {} completion read has no count-action instruction",
                        source.id.0
                    ),
                ));
            }
        }
    }
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let class_free = matches!(&instruction.kind, l::InstructionKind::Call(target)
                if array_ownership::map_operation_name(&module.intrinsic_operations, &target.kind) == Some("UnsafeDelete")
                && instruction.operands.first().and_then(|operand| operand_type(function, operand))
                    .is_some_and(|ty| matches!(ty, l::ValueType::Data(Type::Class(id))
                        if module.classes.get(id.0).is_some_and(|class| class.fields.iter().any(|field| field.ty.counted_type().is_some())))));
            let collect = matches!(&instruction.kind, l::InstructionKind::Call(target)
                if matches!(&target.kind, l::CallTargetKind::Intrinsic(intrinsic)
                    if intrinsic.family == l::IntrinsicFamily::Ambient && module.intrinsic_operations.iter().any(|operation| operation.family == intrinsic.family && operation.operation == intrinsic.operation && operation.semantic_name == "Collect")));
            let releases = collect
                || class_free
                || instruction
                    .count_action
                    .as_ref()
                    .is_some_and(|action| action.release_type().is_some())
                || matches!(
                    instruction.kind,
                    l::InstructionKind::AsyncHandleRelease
                        | l::InstructionKind::AsyncHandleArrayRelease
                );
            if releases
                && !instruction
                    .traps
                    .iter()
                    .any(|trap| trap.kind == l::TrapKind::Call)
            {
                errors.push(finding(
                    function,
                    format!(
                        "block {} instruction {index} releases a counted value without a Call trap",
                        block.id.0
                    ),
                ));
            }
            let expected_type = match &instruction.kind {
                l::InstructionKind::ArraySpreadLiteral(spreads) => instruction
                    .operands
                    .first()
                    .and_then(|operand| operand_type(function, operand))
                    .and_then(|ty| match (spreads.first(), ty) {
                        (
                            Some(Some(l::SpreadKind::Array)),
                            l::ValueType::Data(Type::Array(element)),
                        )
                        | (
                            Some(Some(l::SpreadKind::FixedArray)),
                            l::ValueType::Data(Type::FixedArray(element, _)),
                        )
                        | (
                            Some(Some(l::SpreadKind::SetValues)),
                            l::ValueType::Data(Type::Set(element)),
                        ) => Some(*element),
                        (
                            Some(None | Some(l::SpreadKind::StringCodePoints)),
                            l::ValueType::Data(ty),
                        ) => Some(ty),
                        _ => None,
                    })
                    .or_else(|| {
                        instruction
                            .result
                            .and_then(|value| value_type(function, value))
                            .and_then(|ty| match ty {
                                l::ValueType::Data(Type::Array(element)) => {
                                    Some((**element).clone())
                                }
                                _ => None,
                            })
                    }),
                l::InstructionKind::Call(target)
                    if matches!(
                        array_ownership::array_operation_name(
                            &module.intrinsic_operations,
                            &target.kind
                        ),
                        Some("Fill" | "CopyWithin" | "Slice" | "Concat")
                    ) || matches!(
                        target.kind,
                        l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayClear)
                    ) =>
                {
                    instruction
                        .operands
                        .first()
                        .and_then(|operand| operand_type(function, operand))
                        .and_then(|ty| match ty {
                            l::ValueType::Data(
                                Type::Array(element) | Type::FixedArray(element, _),
                            ) => Some(*element),
                            _ => None,
                        })
                }
                l::InstructionKind::AwaitRaise => function.blocks.iter().find_map(|source| {
                    let l::Terminator::Suspend {
                        kind, successor, ..
                    } = &source.terminator
                    else {
                        return None;
                    };
                    if *successor != block.id {
                        return None;
                    }
                    match kind {
                        l::SuspendKind::AsyncCall { target, .. } => Some(
                            target
                                .return_type
                                .as_ref()
                                .and_then(|ty| match ty {
                                    l::ValueType::Data(ty) => Some(ty.clone()),
                                    _ => None,
                                })
                                .unwrap_or(Type::Void),
                        ),
                        l::SuspendKind::AsyncHandle { handle, .. } => {
                            match value_type(function, *handle) {
                                Some(l::ValueType::Data(Type::AsyncHandle(result))) => {
                                    Some((**result).clone())
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    }
                }),
                l::InstructionKind::MapFromSource => instruction
                    .result
                    .and_then(|value| value_type(function, value))
                    .and_then(|ty| match ty {
                        l::ValueType::Data(Type::Map(_, value)) => Some((**value).clone()),
                        _ => None,
                    }),
                l::InstructionKind::Call(target)
                    if array_ownership::map_operation_name(
                        &module.intrinsic_operations,
                        &target.kind,
                    )
                    .is_some() =>
                {
                    instruction
                        .operands
                        .first()
                        .and_then(|operand| operand_type(function, operand))
                        .or_else(|| {
                            instruction
                                .result
                                .and_then(|value| value_type(function, value))
                                .cloned()
                        })
                        .and_then(|ty| match ty {
                            l::ValueType::Data(Type::Map(_, value)) => Some(*value),
                            _ => None,
                        })
                }
                _ => None,
            };
            if let Some(ty) = expected_type {
                let expected = l::CountAction::for_type(&ty);
                if instruction.count_action.as_ref() != Some(&expected) {
                    errors.push(finding(function, format!("block {} instruction {index} has a missing or wrong count action: expected {expected:?}, got {:?}", block.id.0, instruction.count_action)));
                }
            } else if instruction.count_action.is_some() {
                errors.push(finding(
                    function,
                    format!(
                        "block {} instruction {index} has an unexpected count action",
                        block.id.0
                    ),
                ));
            }

            if let l::InstructionKind::Call(target) = &instruction.kind {
                let removal = matches!(
                    target.kind,
                    l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayPop)
                ) || matches!(
                    array_ownership::array_operation_name(
                        &module.intrinsic_operations,
                        &target.kind
                    ),
                    Some("Pop" | "Shift" | "Splice")
                );
                if removal {
                    if let Some(result) = instruction.result.filter(|value| {
                        value_type(function, *value).is_some_and(is_async_owner_type)
                    }) {
                        if !consumed.contains(&origin(result)) {
                            errors.push(finding(function, format!("block {} instruction {index} removes a counted element without consuming its result", block.id.0)));
                        }
                    }
                }
            }
            if instruction.kind != l::InstructionKind::StoreAddress {
                continue;
            }
            let Some(address) = instruction.operands.first() else {
                continue;
            };
            let Some(l::ValueType::Address(ty)) = operand_type(function, address) else {
                continue;
            };
            if ty.pointee.counted_type().is_none() {
                continue;
            }
            let indexed = ty.array_base.is_some()
                || function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .any(|definition| {
                        matches!(definition.kind, l::InstructionKind::AddressOfIndex { .. })
                            && definition.result.map(l::Operand::Value).as_ref() == Some(address)
                    });
            if !indexed {
                continue;
            }
            let old = block.instructions[..index]
                .iter()
                .rev()
                .find(|load| {
                    load.kind == l::InstructionKind::LoadAddress
                        && load.operands.first() == Some(address)
                })
                .and_then(|load| load.result);
            let released = old.is_some_and(|value| {
                block.instructions[index + 1..]
                    .iter()
                    .take_while(|next| {
                        next.kind != l::InstructionKind::StoreAddress
                            || next.operands.first() != Some(address)
                    })
                    .any(|next| {
                        matches!(
                            next.kind,
                            l::InstructionKind::AsyncHandleRelease
                                | l::InstructionKind::AsyncHandleArrayRelease
                        ) && next.operands.first() == Some(&l::Operand::Value(value))
                    })
            });
            if !released {
                errors.push(finding(function, format!("block {} instruction {index} replaces a counted array element without releasing its old value", block.id.0)));
            }
        }
    }
}
