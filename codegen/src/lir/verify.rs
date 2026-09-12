//! The LIR verifier: counted stores, structure, types, and constants.

use super::verify_dominance::{terminator_values, verify_address_invalidation, verify_dominance};
use super::verify_instruction::verify_instruction_contract;
use super::verify_terminator::verify_terminator_types;
use super::*;

pub(super) fn verify_function(
    module: &l::Module,
    function: &l::Function,
    errors: &mut Vec<VerifyError>,
) {
    verify_structure_and_types(module, function, errors);
    verify_counted_stores(function, errors);
    verify_dominance(function, errors);
    verify_address_invalidation(function, errors);
}

fn verify_counted_stores(function: &l::Function, errors: &mut Vec<VerifyError>) {
    let use_counts = value_use_counts(function);
    let fresh = function
        .values
        .iter()
        .map(|value| value.fresh_owner)
        .collect::<Vec<_>>();

    for block in &function.blocks {
        let mut retains = HashMap::<l::ValueId, usize>::new();
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            if matches!(
                instruction.kind,
                l::InstructionKind::AsyncHandleRetain | l::InstructionKind::AsyncHandleArrayRetain
            ) {
                if let Some(l::Operand::Value(value)) = instruction.operands.first() {
                    *retains.entry(*value).or_default() += 1;
                }
            }
            for (operand_index, operand) in counted_instruction_stores(function, instruction) {
                verify_counted_store_operand(
                    function,
                    block.id,
                    &format!("instruction {instruction_index}"),
                    operand_index,
                    operand,
                    &use_counts,
                    &fresh,
                    &mut retains,
                    errors,
                );
            }
        }
        for (operand_index, operand) in counted_terminator_stores(function, &block.terminator) {
            verify_counted_store_operand(
                function,
                block.id,
                "terminator",
                operand_index,
                &operand,
                &use_counts,
                &fresh,
                &mut retains,
                errors,
            );
        }
    }
}

// Keeping the complete store-site context explicit makes every verifier
// diagnostic identify the exact counted operand that violated the invariant.
#[allow(clippy::too_many_arguments)]
fn verify_counted_store_operand(
    function: &l::Function,
    block: l::BlockId,
    site: &str,
    operand_index: usize,
    operand: &l::Operand,
    use_counts: &[usize],
    fresh: &[bool],
    retains: &mut HashMap<l::ValueId, usize>,
    errors: &mut Vec<VerifyError>,
) {
    let l::Operand::Value(value) = operand else {
        errors.push(finding(
            function,
            format!(
                "block {} {site} stores counted operand {operand_index} ({operand:?}) without an owner",
                block.0
            ),
        ));
        return;
    };
    let single_use_fresh = fresh.get(value.0 as usize).copied().unwrap_or(false)
        && use_counts.get(value.0 as usize).copied() == Some(1);
    if single_use_fresh {
        return;
    }
    if let Some(available) = retains.get_mut(value) {
        if *available != 0 {
            *available -= 1;
            return;
        }
    }
    errors.push(finding(
        function,
        format!(
            "block {} {site} stores counted operand {operand_index} (value {}) without a fresh single-use owner or a preceding retain",
            block.0, value.0
        ),
    ));
}

fn counted_instruction_stores<'i>(
    function: &l::Function,
    instruction: &'i l::Instruction,
) -> Vec<(usize, &'i l::Operand)> {
    let start = match &instruction.kind {
        l::InstructionKind::StoreLocal(_) | l::InstructionKind::StoreGlobal(_) => Some(0),
        l::InstructionKind::StoreAddress => Some(1),
        l::InstructionKind::ArrayLiteral | l::InstructionKind::ArraySpreadLiteral(_) => Some(0),
        l::InstructionKind::Call(target) => counted_operand_start(&target.kind),
        l::InstructionKind::AsyncHandleCreate(target) => match target.kind {
            l::CallTargetKind::Function(_) => Some(0),
            l::CallTargetKind::Method(_) => Some(1),
            _ => None,
        },
        _ => None,
    };
    start
        .into_iter()
        .flat_map(|start| instruction.operands.iter().enumerate().skip(start))
        .filter(|(_, operand)| {
            operand_type(function, operand)
                .as_ref()
                .is_some_and(is_async_owner_type)
        })
        .collect()
}

fn counted_terminator_stores(
    function: &l::Function,
    terminator: &l::Terminator,
) -> Vec<(usize, l::Operand)> {
    match terminator {
        l::Terminator::Return { value, .. } => value
            .iter()
            .filter(|operand| {
                operand_type(function, operand).is_some_and(|ty| is_async_owner_type(&ty))
            })
            .cloned()
            .map(|operand| (0, operand))
            .collect(),
        l::Terminator::Suspend {
            kind: l::SuspendKind::AsyncCall { target, operands },
            ..
        } => {
            let start = counted_operand_start(&target.kind);
            start
                .into_iter()
                .flat_map(|start| operands.iter().copied().enumerate().skip(start))
                .filter(|(_, value)| value_type(function, *value).is_some_and(is_async_owner_type))
                .map(|(index, value)| (index, l::Operand::Value(value)))
                .collect()
        }
        _ => Vec::new(),
    }
}

fn counted_operand_start(kind: &l::CallTargetKind) -> Option<usize> {
    match kind {
        l::CallTargetKind::Function(_) | l::CallTargetKind::Intrinsic(_) => Some(0),
        l::CallTargetKind::StaticClosure(_)
        | l::CallTargetKind::Method(_)
        | l::CallTargetKind::Indirect
        | l::CallTargetKind::BuiltinMethod(_) => Some(1),
        l::CallTargetKind::Foreign(_) => None,
    }
}

fn value_use_counts(function: &l::Function) -> Vec<usize> {
    let mut counts = vec![0; function.values.len()];
    for block in &function.blocks {
        for instruction in &block.instructions {
            for operand in &instruction.operands {
                if let l::Operand::Value(value) = operand {
                    if let Some(count) = counts.get_mut(value.0 as usize) {
                        *count += 1;
                    }
                }
            }
        }
        for value in terminator_values(&block.terminator) {
            if let Some(count) = counts.get_mut(value.0 as usize) {
                *count += 1;
            }
        }
    }
    counts
}

pub(super) fn finding(function: &l::Function, message: impl Into<String>) -> VerifyError {
    VerifyError {
        message: format!(
            "function {} (`{}`): {}",
            function.id.0,
            function.source_name,
            message.into()
        ),
    }
}

fn verify_structure_and_types(
    module: &l::Module,
    function: &l::Function,
    errors: &mut Vec<VerifyError>,
) {
    let mut definitions = vec![0_u32; function.values.len()];
    if function
        .blocks
        .get(function.entry.0 as usize)
        .is_none_or(|block| block.id != function.entry)
    {
        errors.push(finding(
            function,
            format!("entry block {} is missing", function.entry.0),
        ));
    }
    for (index, local) in function.locals.iter().enumerate() {
        if local.id.0 as usize != index {
            errors.push(finding(
                function,
                format!("local table index {index} carries id {}", local.id.0),
            ));
        }
    }
    for (index, value) in function.values.iter().enumerate() {
        if value.id.0 as usize != index {
            errors.push(finding(
                function,
                format!("value table index {index} carries id {}", value.id.0),
            ));
        }
    }
    for parameter in &function.parameters {
        count_definition(function, parameter.value, &mut definitions, errors);
        if function
            .values
            .get(parameter.value.0 as usize)
            .is_some_and(|value| value.fresh_owner)
        {
            errors.push(finding(
                function,
                format!(
                    "parameter value {} is marked as a fresh async owner",
                    parameter.value.0
                ),
            ));
        }
        if let Some(storage) = parameter.storage {
            let parameter_type = value_type(function, parameter.value);
            if function
                .locals
                .get(storage.0 as usize)
                .is_none_or(|local| local.id != storage || Some(&local.ty) != parameter_type)
            {
                errors.push(finding(
                    function,
                    format!(
                        "parameter value {} has invalid address-taken storage {}",
                        parameter.value.0, storage.0
                    ),
                ));
            }
        }
    }
    for (block_index, block) in function.blocks.iter().enumerate() {
        if block.id.0 as usize != block_index {
            errors.push(finding(
                function,
                format!("block table index {block_index} carries id {}", block.id.0),
            ));
        }
        for parameter in &block.parameters {
            count_definition(function, *parameter, &mut definitions, errors);
        }
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            if let Some(result) = instruction.result {
                count_definition(function, result, &mut definitions, errors);
            }
            for (operand_index, operand) in instruction.operands.iter().enumerate() {
                if operand_type(function, operand).is_none() {
                    errors.push(finding(
                        function,
                        format!(
                            "block {} instruction {instruction_index} operand {operand_index} names an unknown value",
                            block.id.0
                        ),
                    ));
                }
                verify_constant(
                    function,
                    operand,
                    &format!(
                        "block {} instruction {instruction_index} operand {operand_index}",
                        block.id.0
                    ),
                    errors,
                );
            }
            verify_instruction_contract(
                module,
                function,
                block,
                instruction_index,
                instruction,
                errors,
            );
            for invalidated in &instruction.invalidates {
                if !matches!(
                    value_type(function, *invalidated),
                    Some(l::ValueType::Data(Type::Array(_)))
                ) {
                    errors.push(finding(
                        function,
                        format!(
                            "block {} instruction {instruction_index} invalidates non-array value {}",
                            block.id.0, invalidated.0
                        ),
                    ));
                }
            }
        }
        verify_terminator_types(module, function, block, errors);
    }
    for (id, count) in definitions.into_iter().enumerate() {
        if count != 1 {
            errors.push(finding(
                function,
                format!("value {id} has {count} definitions (expected exactly one)"),
            ));
        }
    }
}

fn count_definition(
    function: &l::Function,
    value: l::ValueId,
    definitions: &mut [u32],
    errors: &mut Vec<VerifyError>,
) {
    if let Some(count) = definitions.get_mut(value.0 as usize) {
        *count += 1;
    } else {
        errors.push(finding(
            function,
            format!("definition names undeclared value {}", value.0),
        ));
    }
}

pub(super) fn value_type(function: &l::Function, value: l::ValueId) -> Option<&l::ValueType> {
    function
        .values
        .get(value.0 as usize)
        .filter(|entry| entry.id == value)
        .map(|entry| &entry.ty)
}

pub(super) fn operand_type<'a>(
    function: &'a l::Function,
    operand: &'a l::Operand,
) -> Option<l::ValueType> {
    match operand {
        l::Operand::Value(value) => value_type(function, *value).cloned(),
        l::Operand::Constant(constant) => Some(l::ValueType::Data(constant.ty.clone())),
    }
}

pub(super) fn verify_operand_type(
    function: &l::Function,
    operand: &l::Operand,
    expected: &l::ValueType,
    context: &str,
    errors: &mut Vec<VerifyError>,
) {
    match operand_type(function, operand) {
        Some(actual) if actual == *expected => {}
        Some(actual) => errors.push(finding(
            function,
            format!("{context} has type {actual:?}, expected {expected:?}"),
        )),
        None => errors.push(finding(
            function,
            format!("{context} names an unknown value"),
        )),
    }
    verify_constant(function, operand, context, errors);
}

fn verify_constant(
    function: &l::Function,
    operand: &l::Operand,
    context: &str,
    errors: &mut Vec<VerifyError>,
) {
    if let l::Operand::Constant(constant) = operand {
        let valid = match constant.kind {
            l::ConstantKind::Boolean(_) => constant.ty == Type::Bool,
            l::ConstantKind::Null => matches!(
                constant.ty,
                Type::Null
                    | Type::Nullable(_)
                    | Type::Object
                    | Type::Class(_)
                    | Type::Array(_)
                    | Type::Map(..)
                    | Type::Set(_)
                    | Type::Worker(..)
                    | Type::Inbox(_)
                    | Type::Outbox(_)
                    | Type::Func(_)
                    | Type::Generator(_)
                    | Type::RegExp
            ),
            l::ConstantKind::FloatBits(_) => {
                matches!(constant.ty, Type::F16 | Type::F32 | Type::F64)
            }
            l::ConstantKind::Integer(_) => matches!(
                constant.ty,
                Type::I8
                    | Type::U8
                    | Type::I16
                    | Type::U16
                    | Type::I32
                    | Type::U32
                    | Type::I64
                    | Type::U64
                    | Type::F16
                    | Type::Date
                    | Type::Enum(_)
                    | Type::StringAlias(_)
            ),
        };
        if !valid {
            errors.push(finding(
                function,
                format!(
                    "{context} has an invalid constant/type pairing: {:?}",
                    constant
                ),
            ));
        }
    }
}
