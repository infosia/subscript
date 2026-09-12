//! Verification of terminator types and block edges.

use super::verify::{finding, operand_type, value_type, verify_operand_type};
use super::verify_instruction::{
    call_type_matches, declared_call_signature, declared_function, declared_method_function,
    declared_parameters_match,
};
use super::*;

pub(super) fn verify_terminator_types(
    module: &l::Module,
    function: &l::Function,
    block: &l::BasicBlock,
    errors: &mut Vec<VerifyError>,
) {
    if !matches!(block.terminator, l::Terminator::Suspend { .. }) {
        for target in block.terminator.targets() {
            verify_edge(function, block, &target, errors);
        }
    }
    match &block.terminator {
        l::Terminator::Branch(_) => {}
        l::Terminator::ConditionalBranch { condition, .. } => {
            verify_operand_type(
                function,
                condition,
                &l::ValueType::Data(Type::Bool),
                &format!("block {} conditional", block.id.0),
                errors,
            );
        }
        l::Terminator::Switch { value, arms, .. } => {
            let discriminant = operand_type(function, value);
            for arm in arms {
                if discriminant != Some(l::ValueType::Data(arm.value.ty.clone())) {
                    errors.push(finding(
                        function,
                        format!(
                            "block {} switch arm type differs from discriminant",
                            block.id.0
                        ),
                    ));
                }
            }
        }
        l::Terminator::Return { value: None, .. } if function.is_generator => {}
        l::Terminator::Return { value, .. } => match (value, &function.return_type) {
            (None, Type::Void) => {}
            (Some(value), ty) if *ty != Type::Void => verify_operand_type(
                function,
                value,
                &l::ValueType::Data(ty.clone()),
                &format!("block {} return", block.id.0),
                errors,
            ),
            _ => errors.push(finding(
                function,
                format!("block {} return type is invalid", block.id.0),
            )),
        },
        l::Terminator::Trap(_) | l::Terminator::Unreachable { .. } => {}
        l::Terminator::Suspend {
            kind,
            pos,
            traps,
            successor,
            resume_value,
            arguments,
            invalidates,
            ..
        } => {
            let Some(destination) = function.blocks.get(successor.0 as usize) else {
                errors.push(finding(
                    function,
                    format!(
                        "block {} suspend successor {} is missing",
                        block.id.0, successor.0
                    ),
                ));
                return;
            };
            if resume_value.is_some() && *resume_value != destination.parameters.first().copied() {
                errors.push(finding(
                    function,
                    format!(
                        "block {} suspend resume value is not its successor parameter",
                        block.id.0
                    ),
                ));
            }
            let parameters = &destination.parameters
                [usize::from(resume_value.is_some()).min(destination.parameters.len())..];
            if arguments.len() != parameters.len() {
                errors.push(finding(
                    function,
                    format!(
                        "block {} suspend edge to {} has {} arguments for {} live-in parameters",
                        block.id.0,
                        successor.0,
                        arguments.len(),
                        parameters.len()
                    ),
                ));
            }
            for (argument, parameter) in arguments.iter().zip(parameters) {
                if let Some(expected) = value_type(function, *parameter) {
                    verify_operand_type(
                        function,
                        argument,
                        expected,
                        &format!("suspend edge {} -> {}", block.id.0, successor.0),
                        errors,
                    );
                }
            }
            match kind {
                l::SuspendKind::Yield(value) => {
                    if let Some(value) = value {
                        if value_type(function, *value).is_none() {
                            errors.push(finding(
                                function,
                                format!(
                                    "block {} yield names unknown value {}",
                                    block.id.0, value.0
                                ),
                            ));
                        }
                    }
                }
                l::SuspendKind::Async => {}
                l::SuspendKind::AsyncCall { target, operands } => {
                    // §94.1 rule 4: the suspension creates and starts a
                    // frame, so its target is an async function.
                    let declared_async = match target.kind {
                        l::CallTargetKind::Function(id) => declared_function(module, id),
                        l::CallTargetKind::Method(id) => declared_method_function(module, id),
                        _ => None,
                    };
                    if declared_async.is_none_or(|function| !function.is_async) {
                        errors.push(finding(
                            function,
                            format!("block {} async-call target is not async", block.id.0),
                        ));
                    }
                    if let Some((parameters, result)) =
                        declared_call_signature(module, &target.kind, &target.parameter_types)
                    {
                        if !declared_parameters_match(
                            module,
                            &target.kind,
                            &target.parameter_types,
                            &parameters,
                        ) || target.return_type != result
                        {
                            errors.push(finding(
                                function,
                                format!(
                                    "block {} async-call signature disagrees with the target declaration",
                                    block.id.0
                                ),
                            ));
                        }
                    } else {
                        errors.push(finding(
                            function,
                            format!(
                                "block {} async-call target declaration is missing",
                                block.id.0
                            ),
                        ));
                    }
                    if operands.len() != target.parameter_types.len() {
                        errors.push(finding(
                            function,
                            format!("block {} async-call arity is invalid", block.id.0),
                        ));
                    }
                    for (operand, expected) in operands.iter().zip(&target.parameter_types) {
                        if value_type(function, *operand)
                            .is_none_or(|actual| !call_type_matches(actual, expected))
                        {
                            errors.push(finding(
                                function,
                                format!("block {} async-call operand type is invalid", block.id.0),
                            ));
                        }
                    }
                    if target.return_type.as_ref()
                        != resume_value.and_then(|value| value_type(function, value))
                    {
                        errors.push(finding(
                            function,
                            format!("block {} async-call resume type is invalid", block.id.0),
                        ));
                    }
                }
                l::SuspendKind::AsyncHandle { handle } => {
                    // §94.1: a held await resumes from the scheduler, so its
                    // stale-coroutine site must exist and must carry the
                    // suspension's own position. The resume reports there,
                    // before any body effect.
                    let stale: Vec<&l::Trap> = traps
                        .iter()
                        .filter(|trap| trap.kind == l::TrapKind::DevReloadOnlyStaleCoroutine)
                        .collect();
                    if stale.len() != 1 || stale[0].pos != *pos {
                        errors.push(finding(
                            function,
                            format!(
                                "block {} held await needs one stale-coroutine site at its own position",
                                block.id.0
                            ),
                        ));
                    }
                    let Some(l::ValueType::Data(Type::AsyncHandle(value))) =
                        value_type(function, *handle)
                    else {
                        errors.push(finding(
                            function,
                            format!("block {} held await has an invalid handle", block.id.0),
                        ));
                        return;
                    };
                    let expected =
                        (**value != Type::Void).then(|| l::ValueType::Data((**value).clone()));
                    if expected.as_ref()
                        != resume_value.and_then(|value| value_type(function, value))
                    {
                        errors.push(finding(
                            function,
                            format!("block {} held await resume type is invalid", block.id.0),
                        ));
                    }
                }
            }
            for invalidated in invalidates {
                if !matches!(
                    value_type(function, *invalidated),
                    Some(l::ValueType::Data(Type::Array(_)))
                ) {
                    errors.push(finding(
                        function,
                        format!(
                            "block {} suspend invalidates non-array value {}",
                            block.id.0, invalidated.0
                        ),
                    ));
                }
            }
        }
    }
}

fn verify_edge(
    function: &l::Function,
    source: &l::BasicBlock,
    edge: &l::BlockTarget,
    errors: &mut Vec<VerifyError>,
) {
    let Some(destination) = function.blocks.get(edge.block.0 as usize) else {
        errors.push(finding(
            function,
            format!(
                "block {} branches to missing block {}",
                source.id.0, edge.block.0
            ),
        ));
        return;
    };
    let parameters = destination.parameters.as_slice();
    if edge.arguments.len() != parameters.len() {
        errors.push(finding(
            function,
            format!(
                "edge {} -> {} has {} arguments for {} parameters",
                source.id.0,
                edge.block.0,
                edge.arguments.len(),
                parameters.len()
            ),
        ));
    }
    for (argument, parameter) in edge.arguments.iter().zip(parameters) {
        if let Some(expected) = value_type(function, *parameter) {
            verify_operand_type(
                function,
                argument,
                expected,
                &format!("edge {} -> {}", source.id.0, edge.block.0),
                errors,
            );
        }
    }
}
