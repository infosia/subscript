//! Verification of instruction contracts and call signatures.

use super::verify::{finding, operand_type, value_type};
use super::*;

pub(super) fn verify_instruction_contract(
    module: &l::Module,
    function: &l::Function,
    block: &l::BasicBlock,
    instruction_index: usize,
    instruction: &l::Instruction,
    errors: &mut Vec<VerifyError>,
) {
    let context = format!("block {} instruction {instruction_index}", block.id.0);
    let operand_types = instruction
        .operands
        .iter()
        .filter_map(|operand| operand_type(function, operand))
        .collect::<Vec<_>>();
    let result_type = instruction
        .result
        .and_then(|result| value_type(function, result))
        .cloned();
    let bad = |message: &str, errors: &mut Vec<VerifyError>| {
        errors.push(finding(
            function,
            format!(
                "{context} {message}: kind={:?}, operands={:?}, result={:?}",
                instruction.kind, operand_types, result_type
            ),
        ));
    };
    if let Some(result) = instruction.result {
        let expected_fresh = instruction.kind.produces_fresh_async_owner()
            && result_type.as_ref().is_some_and(is_async_owner_type);
        if function
            .values
            .get(result.0 as usize)
            .is_some_and(|value| value.fresh_owner != expected_fresh)
        {
            bad(
                "fresh-owner bit disagrees with the instruction kind",
                errors,
            );
        }
    }
    for trap in &instruction.traps {
        let l::TrapKind::JsonResultValue(ok_field) = trap.kind else {
            continue;
        };
        let valid = match instruction.kind {
            l::InstructionKind::LoadField(l::FieldRef::Class(value_field)) => module
                .classes
                .iter()
                .find(|class| class.fields.iter().any(|field| field.id == value_field))
                .is_some_and(|class| {
                    class
                        .fields
                        .iter()
                        .any(|field| field.id == ok_field && field.ty == Type::Bool)
                }),
            _ => false,
        };
        if !valid {
            bad(
                "JsonResultValue trap names no boolean field in the loaded field's class",
                errors,
            );
        }
    }
    match &instruction.kind {
        l::InstructionKind::Copy => {
            if operand_types.len() != 1 || result_type.as_ref() != operand_types.first() {
                bad("copy input/result types do not match", errors);
            }
        }
        l::InstructionKind::Coerce => {
            let data_coercion =
                matches!(
                    (operand_types.first(), result_type.as_ref()),
                    (Some(l::ValueType::Data(_)), Some(l::ValueType::Data(_)))
                ) && !boundary_box_coercion_signature(module, &operand_types, result_type.as_ref());
            if operand_types.len() != 1 || !data_coercion {
                bad("implicit coercion signature is invalid", errors);
            }
        }
        l::InstructionKind::StringLiteral(_) => {
            if !instruction.operands.is_empty()
                || result_type != Some(l::ValueType::Data(Type::Str))
            {
                bad("string literal signature is invalid", errors);
            }
        }
        l::InstructionKind::LoadLocal(local) => {
            let expected = function.locals.get(local.0 as usize).map(|local| &local.ty);
            if !instruction.operands.is_empty() || result_type.as_ref() != expected {
                bad("local load signature is invalid", errors);
            }
        }
        l::InstructionKind::StoreLocal(local) => {
            let expected = function.locals.get(local.0 as usize).map(|local| &local.ty);
            if operand_types.first() != expected
                || operand_types.len() != 1
                || instruction.result.is_some()
            {
                bad("local store signature is invalid", errors);
            }
        }
        l::InstructionKind::AddressOfLocal(local) => {
            let expected = function.locals.get(local.0 as usize).map(|local| &local.ty);
            let valid = match (expected, result_type.as_ref()) {
                (Some(l::ValueType::Data(stored)), Some(l::ValueType::Address(address))) => {
                    address.pointee == *stored
                }
                _ => false,
            };
            if !valid || !instruction.operands.is_empty() {
                bad("local address signature is invalid", errors);
            }
        }
        l::InstructionKind::LoadGlobal(global) => {
            let expected = module
                .globals
                .get(global.0 as usize)
                .map(|global| l::ValueType::Data(global.ty.clone()));
            if !instruction.operands.is_empty() || result_type != expected {
                bad("global load signature is invalid", errors);
            }
        }
        l::InstructionKind::StoreGlobal(global) => {
            let expected = module
                .globals
                .get(global.0 as usize)
                .map(|global| l::ValueType::Data(global.ty.clone()));
            if operand_types.len() != 1
                || operand_types.first() != expected.as_ref()
                || instruction.result.is_some()
            {
                bad("global store signature is invalid", errors);
            }
        }
        l::InstructionKind::AddressOfGlobal(global) => {
            let expected = module
                .globals
                .get(global.0 as usize)
                .map(|global| &global.ty);
            let valid = match (expected, result_type.as_ref()) {
                (Some(stored), Some(l::ValueType::Address(address))) => {
                    address.pointee == *stored && address.array_base.is_none()
                }
                _ => false,
            };
            if !valid || !instruction.operands.is_empty() {
                bad("global address signature is invalid", errors);
            }
        }
        l::InstructionKind::FunctionRef(target) => {
            if module.functions.get(target.0 as usize).is_none()
                || !instruction.operands.is_empty()
                || !matches!(result_type, Some(l::ValueType::Data(Type::Func(_))))
            {
                bad("function reference signature is invalid", errors);
            }
        }
        l::InstructionKind::Unary(op) => {
            if let ([input], Some(output)) = (operand_types.as_slice(), result_type.as_ref()) {
                let valid = match (op, input, output) {
                    (
                        l::UnaryOp::Not,
                        l::ValueType::Data(Type::Bool),
                        l::ValueType::Data(Type::Bool),
                    ) => true,
                    (l::UnaryOp::Neg, l::ValueType::Data(a), l::ValueType::Data(b)) => {
                        a == b && a.is_numeric()
                    }
                    (l::UnaryOp::BitNot, l::ValueType::Data(a), l::ValueType::Data(b)) => {
                        a == b && a.is_integer()
                    }
                    _ => false,
                };
                if !valid {
                    bad("unary operand/result types are invalid", errors);
                }
            } else {
                bad("unary signature is incomplete", errors);
            }
        }
        l::InstructionKind::Binary(op) => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                (
                    [l::ValueType::Data(left), l::ValueType::Data(right)],
                    Some(l::ValueType::Data(result)),
                ) => match op {
                    l::BinaryOp::Add if *left == Type::Str => {
                        *right == Type::Str && *result == Type::Str
                    }
                    l::BinaryOp::Add
                    | l::BinaryOp::Sub
                    | l::BinaryOp::Mul
                    | l::BinaryOp::Div
                    | l::BinaryOp::Rem => left == right && left == result && left.is_numeric(),
                    l::BinaryOp::Eq
                    | l::BinaryOp::Ne
                    | l::BinaryOp::Lt
                    | l::BinaryOp::Le
                    | l::BinaryOp::Gt
                    | l::BinaryOp::Ge => *result == Type::Bool,
                    l::BinaryOp::BitAnd
                    | l::BinaryOp::BitOr
                    | l::BinaryOp::BitXor
                    | l::BinaryOp::Shl
                    | l::BinaryOp::Shr
                    | l::BinaryOp::UShr => {
                        left.is_integer() && right.is_integer() && left == result
                    }
                },
                _ => false,
            };
            if !valid {
                bad("binary operand/result types are invalid", errors);
            }
        }
        l::InstructionKind::Cast => {
            if operand_types.len() != 1
                || !matches!(operand_types[0], l::ValueType::Data(_))
                || !matches!(result_type, Some(l::ValueType::Data(_)))
            {
                bad("cast signature is invalid", errors);
            }
        }
        l::InstructionKind::AllocateClass(class) => {
            let expected = module.classes.get(class.0).map(|definition| {
                if definition.is_value {
                    l::ValueType::Address(l::AddressType {
                        pointee: Type::Class(*class),
                        array_base: None,
                    })
                } else {
                    l::ValueType::Data(Type::Class(*class))
                }
            });
            if !instruction.operands.is_empty() || result_type != expected {
                bad("class allocation signature is invalid", errors);
            }
        }
        l::InstructionKind::BoxBoundaryValue { payload } => {
            let valid =
                boundary_box_signature(module, *payload, &operand_types, result_type.as_ref());
            if !valid {
                bad("boundary value box signature is invalid", errors);
            }
        }
        l::InstructionKind::AddressOfValue => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                ([l::ValueType::Data(value)], Some(l::ValueType::Address(address))) => {
                    address.pointee == *value && address.array_base.is_none()
                }
                _ => false,
            };
            if !valid {
                bad("temporary address signature is invalid", errors);
            }
        }
        l::InstructionKind::AddressOfField(field) => {
            let expected_field = lir_field_type(module, *field, operand_types.first());
            let base_valid = operand_types
                .first()
                .is_some_and(|base| field_base_accepts(module, *field, base));
            let result_valid = match (expected_field, result_type.as_ref()) {
                (Some(expected), Some(l::ValueType::Address(address))) => {
                    let expected_base = operand_types.first().and_then(address_base);
                    address.pointee == expected && address.array_base == expected_base
                }
                _ => false,
            };
            if operand_types.len() != 1 || !base_valid || !result_valid {
                bad("field address signature is invalid", errors);
            }
        }
        l::InstructionKind::Call(target) => {
            if is_operation_table_target(&target.kind) {
                if !target.parameter_types.is_empty() {
                    bad(
                        "intrinsic or built-in target restates table parameter types",
                        errors,
                    );
                }
                let matching = module.operation_signatures(&target.kind).find(|signature| {
                    call_parameters_match(&operand_types, &signature.parameter_types)
                        && target.return_type == signature.return_type
                });
                if matching.is_none() || result_type != target.return_type {
                    let name = operation_name(module, &target.kind);
                    let declared = module
                        .operation_signatures(&target.kind)
                        .map(|signature| {
                            format!(
                                "{:?} -> {:?}",
                                signature.parameter_types, signature.return_type
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" or ");
                    errors.push(finding(
                        function,
                        format!(
                            "{context} call disagrees with the signature table: {name} declares {declared}, got {operand_types:?} -> {result_type:?}"
                        ),
                    ));
                }
            } else {
                if !call_parameters_match(&operand_types, &target.parameter_types)
                    || result_type != target.return_type
                {
                    bad("call signature disagrees with its target", errors);
                }
                if let Some((parameters, result)) =
                    declared_call_signature(module, &target.kind, &operand_types)
                {
                    if !declared_parameters_match(
                        module,
                        &target.kind,
                        &target.parameter_types,
                        &parameters,
                    ) || target.return_type != result
                    {
                        bad(
                            "call signature disagrees with the target declaration",
                            errors,
                        );
                    }
                }
            }
            let target_exists = match &target.kind {
                l::CallTargetKind::Function(id) => declared_function(module, *id).is_some(),
                l::CallTargetKind::StaticClosure(id) => {
                    declared_function(module, *id)
                        .is_some_and(|function| function.kind == l::FunctionKind::Lambda)
                        && instruction.operands.first().is_some_and(|operand| {
                            static_closure_operand_matches(function, operand, *id)
                        })
                }
                l::CallTargetKind::Method(id) => declared_method_function(module, *id).is_some(),
                l::CallTargetKind::Foreign(id) => module
                    .foreign_functions
                    .get(id.0 as usize)
                    .is_some_and(|foreign| foreign.id == *id),
                l::CallTargetKind::Indirect => matches!(
                    target.parameter_types.first(),
                    Some(l::ValueType::Data(Type::Func(_)))
                ),
                l::CallTargetKind::Intrinsic(intrinsic) => {
                    (intrinsic.family == l::IntrinsicFamily::ContextBytes)
                        == intrinsic.type_argument.is_some()
                        && module.intrinsic_operations.iter().any(|operation| {
                            operation.family == intrinsic.family
                                && operation.operation == intrinsic.operation
                        })
                }
                l::CallTargetKind::BuiltinMethod(_) => {
                    module.operation_signatures(&target.kind).next().is_some()
                }
            };
            if !target_exists {
                bad("call target identity/signature is invalid", errors);
            }
        }
        l::InstructionKind::AsyncHandleCreate(target) => {
            let declared = match target.kind {
                l::CallTargetKind::Function(id) => declared_function(module, id),
                l::CallTargetKind::Method(id) => declared_method_function(module, id),
                _ => None,
            };
            let valid_result = matches!(result_type.as_ref(), Some(l::ValueType::Data(Type::AsyncHandle(value)))
                if target.return_type.as_ref() == ((**value != Type::Void)
                    .then(|| l::ValueType::Data((**value).clone()))) .as_ref());
            if !call_parameters_match(&operand_types, &target.parameter_types)
                || !valid_result
                || declared.is_none_or(|function| !function.is_async)
            {
                bad("async handle creation signature is invalid", errors);
            }
        }
        l::InstructionKind::AsyncHandleRetain | l::InstructionKind::AsyncHandleRelease => {
            if operand_types.len() != 1
                || !matches!(
                    operand_types.first(),
                    Some(l::ValueType::Data(Type::AsyncHandle(_)))
                )
                || instruction.result.is_some()
            {
                bad("async handle ownership instruction is invalid", errors);
            }
        }
        l::InstructionKind::AsyncHandleArrayRetain
        | l::InstructionKind::AsyncHandleArrayRelease => {
            if operand_types.len() != 1
                || !matches!(operand_types.first(), Some(l::ValueType::Data(Type::Array(element)))
                    if matches!(&**element, Type::AsyncHandle(_)))
                || instruction.result.is_some()
            {
                bad("async handle array release signature is invalid", errors);
            }
        }
        l::InstructionKind::LoadAddress => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                ([l::ValueType::Address(address)], Some(l::ValueType::Data(result))) => {
                    address.pointee == *result
                }
                _ => false,
            };
            if !valid {
                bad("address load signature is invalid", errors);
            }
        }
        l::InstructionKind::StoreAddress => {
            let valid = match operand_types.as_slice() {
                [l::ValueType::Address(address), l::ValueType::Data(value)] => {
                    address.pointee == *value
                }
                _ => false,
            };
            if !valid || instruction.result.is_some() {
                bad("address store signature is invalid", errors);
            }
        }
        l::InstructionKind::AddressOfIndex { checked } => {
            let valid_index = matches!(
                operand_types.get(1),
                Some(l::ValueType::Data(Type::I32 | Type::U32))
            );
            let element = operand_types.first().and_then(index_element_type);
            let result_element = match result_type.as_ref() {
                Some(l::ValueType::Address(address)) => Some(&address.pointee),
                _ => None,
            };
            let expected_base = match (instruction.operands.first(), operand_types.first()) {
                (Some(l::Operand::Value(value)), Some(l::ValueType::Data(Type::Array(_)))) => {
                    Some(*value)
                }
                (_, Some(l::ValueType::Address(address))) => address.array_base,
                _ => None,
            };
            let result_base = match result_type.as_ref() {
                Some(l::ValueType::Address(address)) => address.array_base,
                _ => None,
            };
            let has_bounds_trap = instruction
                .traps
                .iter()
                .any(|trap| matches!(trap.kind, l::TrapKind::IndexRead | l::TrapKind::IndexWrite));
            if operand_types.len() != 2
                || !valid_index
                || element != result_element
                || expected_base != result_base
            {
                bad("index address signature is invalid", errors);
            }
            if *checked != has_bounds_trap {
                bad("index address check disagrees with its bounds trap", errors);
            }
        }
        l::InstructionKind::LoadField(field) => {
            let expected_field = lir_field_type(module, *field, operand_types.first());
            let base_valid = operand_types
                .first()
                .is_some_and(|base| field_base_accepts(module, *field, base));
            if operand_types.len() != 1
                || !base_valid
                || result_type != expected_field.map(l::ValueType::Data)
            {
                bad("field load signature is invalid", errors);
            }
        }
        l::InstructionKind::Length => {
            let valid_subject = matches!(
                operand_types.first(),
                Some(l::ValueType::Data(
                    Type::Str | Type::Array(_) | Type::FixedArray(..)
                ))
            );
            if operand_types.len() != 1
                || !valid_subject
                || !matches!(result_type, Some(l::ValueType::Data(Type::I32)))
            {
                bad("length signature is invalid", errors);
            }
        }
        l::InstructionKind::ForeignArrayData => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                (
                    [l::ValueType::Data(Type::Array(element))],
                    Some(l::ValueType::Address(address)),
                ) => address.pointee == **element && address.array_base.is_none(),
                _ => false,
            };
            if !valid {
                bad("foreign array data signature is invalid", errors);
            }
        }
        l::InstructionKind::ArrayLiteral => {
            let valid = match result_type.as_ref() {
                Some(l::ValueType::Data(Type::Array(element))) => operand_types
                    .iter()
                    .all(|ty| ty == &l::ValueType::Data((**element).clone())),
                Some(l::ValueType::Data(Type::FixedArray(element, count))) => {
                    operand_types.len() == *count as usize
                        && operand_types
                            .iter()
                            .all(|ty| ty == &l::ValueType::Data((**element).clone()))
                }
                _ => false,
            };
            if !valid {
                bad("array literal signature is invalid", errors);
            }
        }
        l::InstructionKind::ArrayWithCapacity => {
            let valid = matches!(
                (operand_types.as_slice(), result_type.as_ref()),
                (
                    [l::ValueType::Data(Type::I32)],
                    Some(l::ValueType::Data(Type::Array(_)))
                )
            ) && instruction.traps.len() == 1
                && instruction.traps[0].kind == l::TrapKind::Call;
            if !valid {
                bad("capacity array signature/traps are invalid", errors);
            }
        }
        l::InstructionKind::ArraySpreadLiteral(spreads) => {
            let valid = match result_type.as_ref() {
                Some(l::ValueType::Data(Type::Array(element))) => {
                    spreads.len() == operand_types.len()
                        && spreads.iter().zip(&operand_types).all(
                            |(spread, operand)| match spread {
                                None => operand == &l::ValueType::Data((**element).clone()),
                                Some(_) => matches!(operand, l::ValueType::Data(_)),
                            },
                        )
                }
                _ => false,
            };
            if !valid {
                bad("spread-array literal signature is invalid", errors);
            }
        }
        l::InstructionKind::SetFromSource(spread) => {
            let valid = match (result_type.as_ref(), operand_types.as_slice()) {
                (Some(l::ValueType::Data(Type::Set(key))), [l::ValueType::Data(source)]) => {
                    match (spread, source) {
                        (l::SpreadKind::Array, Type::Array(element))
                        | (l::SpreadKind::FixedArray, Type::FixedArray(element, _))
                        | (l::SpreadKind::SetValues, Type::Set(element)) => element == key,
                        (l::SpreadKind::StringCodePoints, Type::Str) => **key == Type::Str,
                        _ => false,
                    }
                }
                _ => false,
            };
            if !valid {
                bad("Set source-construction signature is invalid", errors);
            }
        }
        l::InstructionKind::Template(parts) => {
            let valid_indices = parts.iter().all(|part| match part {
                l::TemplatePart::Text(_) => true,
                l::TemplatePart::Operand { index, format } => operand_types
                    .get(*index as usize)
                    .and_then(|ty| match ty {
                        l::ValueType::Data(ty) => Some(ty),
                        _ => None,
                    })
                    .is_some_and(|ty| format.accepts(ty)),
            });
            if !valid_indices || result_type != Some(l::ValueType::Data(Type::Str)) {
                bad("template signature is invalid", errors);
            }
        }
        l::InstructionKind::MakeClosure(target) => {
            let capture_count = module.functions.get(target.0 as usize).map(|target| {
                target
                    .parameters
                    .iter()
                    .take_while(|parameter| parameter.kind == l::ParameterKind::Capture)
                    .count()
            });
            if capture_count != Some(operand_types.len())
                || !matches!(result_type, Some(l::ValueType::Data(Type::Func(_))))
            {
                bad("closure signature is invalid", errors);
            }
        }
        l::InstructionKind::IteratorCreate { kind, bound } => {
            let valid = matches!(
                result_type.as_ref(),
                Some(l::ValueType::Iterator(iterator)) if iterator.kind == *kind
            );
            if operand_types.len() != 1 || !valid {
                bad("iterator creation signature is invalid", errors);
            }
            if *bound == l::IteratorBoundKind::Fixed
                && !matches!(
                    kind,
                    l::ForOfKind::ArrayValues
                        | l::ForOfKind::ArrayValuesReverse
                        | l::ForOfKind::ArrayKeysReverse
                )
            {
                bad(
                    "fixed iterator bound requires a dynamic-array value cursor",
                    errors,
                );
            }
        }
        l::InstructionKind::IteratorHasNext => {
            if !valid_iterator_state(&operand_types)
                || result_type != Some(l::ValueType::Data(Type::Bool))
            {
                bad("iterator condition signature is invalid", errors);
            }
        }
        l::InstructionKind::IteratorValue => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                (
                    [l::ValueType::Iterator(iterator), l::ValueType::Data(Type::I32), l::ValueType::Data(Type::I32)],
                    Some(l::ValueType::Data(result)),
                ) => iterator.element == *result,
                _ => false,
            };
            if !valid {
                bad("iterator value signature is invalid", errors);
            }
        }
        l::InstructionKind::IteratorBound => {
            if !matches!(operand_types.as_slice(), [l::ValueType::Iterator(_)])
                || result_type != Some(l::ValueType::Data(Type::I32))
            {
                bad("iterator bound signature is invalid", errors);
            }
        }
        l::InstructionKind::IteratorAdvance => {
            let valid = match (operand_types.as_slice(), result_type.as_ref()) {
                (
                    [l::ValueType::Iterator(input), l::ValueType::Data(Type::I32), l::ValueType::Data(Type::I32)],
                    Some(l::ValueType::Iterator(output)),
                ) => input == output,
                _ => false,
            };
            if !valid {
                bad("iterator advance signature is invalid", errors);
            }
        }
        l::InstructionKind::Zero => {
            if !instruction.operands.is_empty()
                || !matches!(result_type, Some(l::ValueType::Data(_)))
            {
                bad("typed zero signature is invalid", errors);
            }
        }
    }
}

fn lir_field_type(
    module: &l::Module,
    field: l::FieldRef,
    base: Option<&l::ValueType>,
) -> Option<Type> {
    match field {
        l::FieldRef::Class(id) => module
            .classes
            .iter()
            .flat_map(|class| &class.fields)
            .find(|field| field.id == id)
            .map(|field| field.ty.clone()),
        l::FieldRef::IterDone => Some(Type::Bool),
        l::FieldRef::IterValue => match base {
            Some(l::ValueType::Data(Type::IterResult(value))) => Some((**value).clone()),
            Some(l::ValueType::Address(l::AddressType {
                pointee: Type::IterResult(value),
                ..
            })) => Some((**value).clone()),
            _ => None,
        },
    }
}

fn boundary_box_signature(
    module: &l::Module,
    payload: ClassId,
    operands: &[l::ValueType],
    result: Option<&l::ValueType>,
) -> bool {
    let (
        [l::ValueType::Data(Type::Class(source))],
        Some(l::ValueType::Data(Type::Nullable(target))),
    ) = (operands, result)
    else {
        return false;
    };
    let Type::Class(target) = target.as_ref() else {
        return false;
    };
    if *source != payload {
        return false;
    }
    let Some(source_class) = module.classes.get(source.0) else {
        return false;
    };
    if !source_class.is_value || !source_class.is_boundary {
        return false;
    }
    if source == target {
        return true;
    }
    source_class.fields.first().is_some_and(|field| {
        field.ty == Type::Class(*target)
            && module
                .classes
                .get(target.0)
                .is_some_and(|class| class.is_embedded_header)
    })
}

fn boundary_box_coercion_signature(
    module: &l::Module,
    operands: &[l::ValueType],
    result: Option<&l::ValueType>,
) -> bool {
    let [l::ValueType::Data(Type::Class(payload))] = operands else {
        return false;
    };
    boundary_box_signature(module, *payload, operands, result)
}

fn field_base_accepts(module: &l::Module, field: l::FieldRef, base: &l::ValueType) -> bool {
    match field {
        l::FieldRef::Class(id) => {
            let owner = module
                .classes
                .iter()
                .find(|class| class.fields.iter().any(|candidate| candidate.id == id));
            owner.is_some_and(|class| match base {
                l::ValueType::Data(Type::Class(id)) => *id == class.id,
                l::ValueType::Data(Type::Nullable(inner)) => {
                    matches!(inner.as_ref(), Type::Class(id)
                        if *id == class.id && class.is_value && class.is_boundary)
                }
                l::ValueType::Address(address) => address.pointee == Type::Class(class.id),
                _ => false,
            })
        }
        l::FieldRef::IterDone | l::FieldRef::IterValue => matches!(
            base,
            l::ValueType::Data(Type::IterResult(_))
                | l::ValueType::Address(l::AddressType {
                    pointee: Type::IterResult(_),
                    ..
                })
        ),
    }
}

fn index_element_type(ty: &l::ValueType) -> Option<&Type> {
    match ty {
        l::ValueType::Data(Type::Array(element) | Type::FixedArray(element, _)) => Some(element),
        l::ValueType::Address(l::AddressType {
            pointee: Type::FixedArray(element, _),
            ..
        }) => Some(element),
        _ => None,
    }
}

fn valid_iterator_state(types: &[l::ValueType]) -> bool {
    matches!(
        types,
        [
            l::ValueType::Iterator(_),
            l::ValueType::Data(Type::I32),
            l::ValueType::Data(Type::I32)
        ]
    )
}

pub(super) fn declared_function(module: &l::Module, id: l::FunctionId) -> Option<&l::Function> {
    module
        .functions
        .get(id.0 as usize)
        .filter(|function| function.id == id)
}

pub(super) fn declared_method_function(
    module: &l::Module,
    id: l::MethodId,
) -> Option<&l::Function> {
    let function = module.classes.iter().find_map(|class| {
        class
            .constructor
            .iter()
            .chain(&class.methods)
            .find(|method| method.id == id)
            .map(|method| method.function)
    })?;
    declared_function(module, function)
}

fn function_signature(function: &l::Function) -> (Vec<l::ValueType>, Option<l::ValueType>) {
    (
        function
            .parameters
            .iter()
            .filter_map(|parameter| value_type(function, parameter.value).cloned())
            .collect(),
        (function.return_type != Type::Void)
            .then(|| l::ValueType::Data(function.return_type.clone())),
    )
}

pub(super) fn call_type_matches(actual: &l::ValueType, declared: &l::ValueType) -> bool {
    match (actual, declared) {
        (l::ValueType::Address(actual), l::ValueType::Address(declared)) => {
            actual.pointee == declared.pointee
        }
        _ => actual == declared,
    }
}

fn call_parameters_match(actual: &[l::ValueType], declared: &[l::ValueType]) -> bool {
    actual.len() == declared.len()
        && actual
            .iter()
            .zip(declared)
            .all(|(actual, declared)| call_type_matches(actual, declared))
}

pub(super) fn declared_parameters_match(
    module: &l::Module,
    kind: &l::CallTargetKind,
    actual: &[l::ValueType],
    declared: &[l::ValueType],
) -> bool {
    actual.len() == declared.len()
        && actual.iter().zip(declared).all(|(actual, declared)| {
            call_type_matches(actual, declared)
                || matches!(kind, l::CallTargetKind::Foreign(_))
                    && foreign_boundary_pointer_type_matches(module, actual, declared)
        })
}

fn static_closure_operand_matches(
    function: &l::Function,
    operand: &l::Operand,
    target: l::FunctionId,
) -> bool {
    fn collect(
        function: &l::Function,
        operand: &l::Operand,
        target: l::FunctionId,
        visiting: &mut BTreeSet<l::ValueId>,
        saw_closure: &mut bool,
    ) -> bool {
        let l::Operand::Value(value) = operand else {
            return false;
        };
        if !visiting.insert(*value) {
            return true;
        }
        if let Some(instruction) = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find(|instruction| instruction.result == Some(*value))
        {
            let valid = match &instruction.kind {
                l::InstructionKind::MakeClosure(function) => {
                    *saw_closure = true;
                    *function == target
                }
                l::InstructionKind::Copy => instruction.operands.first().is_some_and(|operand| {
                    collect(function, operand, target, visiting, saw_closure)
                }),
                _ => false,
            };
            visiting.remove(value);
            return valid;
        }
        if function
            .parameters
            .iter()
            .any(|parameter| parameter.value == *value)
        {
            visiting.remove(value);
            return false;
        }
        let Some((destination, index)) = function.blocks.iter().find_map(|block| {
            block
                .parameters
                .iter()
                .position(|parameter| parameter == value)
                .map(|index| (block.id, index))
        }) else {
            visiting.remove(value);
            return false;
        };
        let mut found = false;
        let mut valid = true;
        let mut inspect = |edge: &l::BlockTarget| {
            if edge.block != destination {
                return;
            }
            found = true;
            valid &= edge
                .arguments
                .get(index)
                .is_some_and(|operand| collect(function, operand, target, visiting, saw_closure));
        };
        for block in &function.blocks {
            if matches!(block.terminator, l::Terminator::Suspend { .. }) {
                continue;
            }
            for edge in block.terminator.targets() {
                inspect(&edge);
            }
        }
        visiting.remove(value);
        found && valid
    }

    let mut saw_closure = false;
    let valid = collect(
        function,
        operand,
        target,
        &mut BTreeSet::new(),
        &mut saw_closure,
    );
    valid && saw_closure
}

fn foreign_boundary_pointer_type_matches(
    module: &l::Module,
    actual: &l::ValueType,
    declared: &l::ValueType,
) -> bool {
    let (l::ValueType::Address(address), l::ValueType::Data(declared)) = (actual, declared) else {
        return false;
    };
    boundary_box_class(module, declared).is_some_and(|class| address.pointee == Type::Class(class))
}

fn is_operation_table_target(kind: &l::CallTargetKind) -> bool {
    matches!(
        kind,
        l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_)
    )
}

fn operation_name(module: &l::Module, kind: &l::CallTargetKind) -> String {
    match kind {
        l::CallTargetKind::Intrinsic(intrinsic) => {
            let name = module
                .intrinsic_operations
                .iter()
                .find(|operation| {
                    operation.family == intrinsic.family
                        && operation.operation == intrinsic.operation
                })
                .map_or("<unknown>", |operation| operation.semantic_name.as_str());
            format!("{:?}.{name}", intrinsic.family)
        }
        l::CallTargetKind::BuiltinMethod(method) => format!("BuiltinMethod.{method:?}"),
        l::CallTargetKind::Function(_)
        | l::CallTargetKind::StaticClosure(_)
        | l::CallTargetKind::Method(_)
        | l::CallTargetKind::Foreign(_)
        | l::CallTargetKind::Indirect => "<declared call>".to_string(),
    }
}

pub(super) fn declared_call_signature(
    module: &l::Module,
    kind: &l::CallTargetKind,
    operand_types: &[l::ValueType],
) -> Option<(Vec<l::ValueType>, Option<l::ValueType>)> {
    match kind {
        l::CallTargetKind::Function(id) => declared_function(module, *id).map(function_signature),
        l::CallTargetKind::StaticClosure(id) => {
            let function = declared_function(module, *id)?;
            let callable = operand_types.first()?.clone();
            if !matches!(callable, l::ValueType::Data(Type::Func(_))) {
                return None;
            }
            let mut parameters = vec![callable];
            parameters.extend(
                function
                    .parameters
                    .iter()
                    .filter(|parameter| parameter.kind == l::ParameterKind::Explicit)
                    .filter_map(|parameter| value_type(function, parameter.value).cloned()),
            );
            Some((
                parameters,
                (function.return_type != Type::Void)
                    .then(|| l::ValueType::Data(function.return_type.clone())),
            ))
        }
        l::CallTargetKind::Method(id) => {
            declared_method_function(module, *id).map(function_signature)
        }
        l::CallTargetKind::Foreign(id) => module
            .foreign_functions
            .get(id.0 as usize)
            .filter(|function| function.id == *id)
            .map(|function| {
                (
                    function
                        .parameters
                        .iter()
                        .flat_map(|parameter| match &parameter.ty {
                            Type::Array(element) => vec![
                                l::ValueType::Address(l::AddressType {
                                    pointee: (**element).clone(),
                                    array_base: None,
                                }),
                                l::ValueType::Data(Type::I32),
                            ],
                            ty => vec![l::ValueType::Data(ty.clone())],
                        })
                        .collect(),
                    (function.return_type != Type::Void)
                        .then(|| l::ValueType::Data(function.return_type.clone())),
                )
            }),
        l::CallTargetKind::Indirect => {
            let callee = operand_types.first()?.clone();
            let l::ValueType::Data(Type::Func(signature)) = &callee else {
                return None;
            };
            let signature = signature.clone();
            let mut parameters = vec![callee];
            parameters.extend(signature.params.iter().cloned().map(l::ValueType::Data));
            Some((
                parameters,
                (signature.ret != Type::Void).then(|| l::ValueType::Data(signature.ret.clone())),
            ))
        }
        l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_) => module
            .operation_signatures(kind)
            .find(|signature| call_parameters_match(operand_types, &signature.parameter_types))
            .map(|signature| {
                (
                    signature.parameter_types.clone(),
                    signature.return_type.clone(),
                )
            }),
    }
}
