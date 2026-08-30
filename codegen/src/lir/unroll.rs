//! Bounded constant-trip loop unrolling for LIR.

use std::collections::{HashMap, HashSet};

use subscript_compiler::lir as l;
use subscript_compiler::Type;

// Eight covers twice a22's measured four trips.
pub(super) const MAX_TRIP_COUNT: usize = 8;
// One iteration includes the source body and the induction step.
// Sixteen covers a22's 13 instructions and caps expansion at 128 instructions.
pub(super) const MAX_BODY_INSTRUCTIONS: usize = 16;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Report {
    pub(super) transformed_loops: usize,
    pub(super) declined_loops: usize,
}

#[derive(Clone)]
struct LoopShape {
    header: l::BlockId,
    body: l::BlockId,
    step: l::BlockId,
    exit_target: l::BlockTarget,
    counter_index: usize,
    step_counter_index: usize,
    trip_count: usize,
}

enum Decision {
    NotConstant,
    Decline,
    Transform(LoopShape),
}

pub(super) fn run(module: &mut l::Module) -> Report {
    let mut report = Report::default();
    for function in &mut module.functions {
        let mut block_index = 0;
        while block_index < function.blocks.len() {
            match analyze(function, l::BlockId(block_index as u32)) {
                Decision::NotConstant => {}
                Decision::Decline => report.declined_loops += 1,
                Decision::Transform(shape) => {
                    if transform(function, &shape) {
                        report.transformed_loops += 1;
                    } else {
                        report.declined_loops += 1;
                    }
                }
            }
            block_index += 1;
        }
    }
    report
}

fn analyze(function: &l::Function, header_id: l::BlockId) -> Decision {
    let Some(mut shape) = constant_trip_shape(function, header_id) else {
        return Decision::NotConstant;
    };
    if shape.trip_count == 0
        || shape.trip_count > MAX_TRIP_COUNT
        || function.is_generator
        || function.is_async
    {
        return Decision::Decline;
    }

    let header = &function.blocks[shape.header.0 as usize];
    let body = &function.blocks[shape.body.0 as usize];
    let step = &function.blocks[shape.step.0 as usize];
    let l::Terminator::ConditionalBranch {
        then_target,
        else_target,
        ..
    } = &header.terminator
    else {
        return Decision::Decline;
    };
    let l::Terminator::Branch(body_target) = &body.terminator else {
        return Decision::Decline;
    };
    let l::Terminator::Branch(backedge) = &step.terminator else {
        return Decision::Decline;
    };

    let predecessors = predecessors(function);
    let step_predecessors = &predecessors[shape.step.0 as usize];
    let exit_predecessors = &predecessors[else_target.block.0 as usize];
    let body_size = body.instructions.len() + step.instructions.len();
    let types_match = header.parameters.len() == step.parameters.len()
        && header
            .parameters
            .iter()
            .zip(&step.parameters)
            .all(|(left, right)| {
                function.values[left.0 as usize].ty == function.values[right.0 as usize].ty
            });
    let trap_free = header
        .instructions
        .iter()
        .chain(&body.instructions)
        .chain(&step.instructions)
        .all(|instruction| instruction.traps.is_empty());
    let clone_safe = body
        .instructions
        .iter()
        .chain(&step.instructions)
        .all(|instruction| instruction_is_clone_safe(&instruction.kind));
    if header.instructions.len() != 1
        || then_target.block != shape.body
        || !then_target.arguments.is_empty()
        || else_target != &shape.exit_target
        || !body.parameters.is_empty()
        || body_target.block != shape.step
        || body_target.arguments.len() != step.parameters.len()
        || backedge.block != shape.header
        || backedge.arguments.len() != header.parameters.len()
        || step_predecessors.as_slice() != [shape.body]
        || exit_predecessors.as_slice() != [shape.header]
        || body_size > MAX_BODY_INSTRUCTIONS
        || !types_match
        || !trap_free
        || !clone_safe
        || body_target.arguments.get(shape.step_counter_index)
            != Some(&l::Operand::Value(header.parameters[shape.counter_index]))
    {
        return Decision::Decline;
    }

    let internal = internal_definitions(header, body, step);
    if has_external_use(function, &internal, &[shape.header, shape.body, shape.step]) {
        return Decision::Decline;
    }
    shape.exit_target = else_target.clone();
    Decision::Transform(shape)
}

fn instruction_is_clone_safe(kind: &l::InstructionKind) -> bool {
    matches!(
        kind,
        l::InstructionKind::Copy
            | l::InstructionKind::LoadLocal(_)
            | l::InstructionKind::StoreLocal(_)
            | l::InstructionKind::AddressOfLocal(_)
            | l::InstructionKind::LoadGlobal(_)
            | l::InstructionKind::StoreGlobal(_)
            | l::InstructionKind::AddressOfGlobal(_)
            | l::InstructionKind::Unary(_)
            | l::InstructionKind::Binary(_)
            | l::InstructionKind::Cast
            | l::InstructionKind::Coerce
            | l::InstructionKind::BoxBoundaryValue { .. }
            | l::InstructionKind::AddressOfValue
            | l::InstructionKind::AddressOfField(_)
            | l::InstructionKind::AddressOfIndex { checked: false }
            | l::InstructionKind::LoadAddress
            | l::InstructionKind::StoreAddress
            | l::InstructionKind::LoadField(_)
            | l::InstructionKind::Length
            | l::InstructionKind::Zero
    )
}

fn constant_trip_shape(function: &l::Function, header_id: l::BlockId) -> Option<LoopShape> {
    let header = function.blocks.get(header_id.0 as usize)?;
    let l::Terminator::ConditionalBranch {
        condition: l::Operand::Value(condition),
        then_target,
        else_target,
    } = &header.terminator
    else {
        return None;
    };
    let condition = header
        .instructions
        .iter()
        .find(|instruction| instruction.result == Some(*condition))?;
    let l::InstructionKind::Binary(operation @ (l::BinaryOp::Lt | l::BinaryOp::Le)) =
        condition.kind
    else {
        return None;
    };
    let [l::Operand::Value(counter), l::Operand::Constant(bound)] = condition.operands.as_slice()
    else {
        return None;
    };
    let counter_index = header
        .parameters
        .iter()
        .position(|parameter| parameter == counter)?;
    let (bound_type, bound) = integer_constant(bound)?;

    let predecessors = predecessors(function);
    let header_predecessors = predecessors.get(header_id.0 as usize)?;
    if header_predecessors.len() != 2 || header_predecessors[0] == header_predecessors[1] {
        return None;
    }

    for (step_id, preheader_id) in [
        (header_predecessors[0], header_predecessors[1]),
        (header_predecessors[1], header_predecessors[0]),
    ] {
        let Some(step) = function.blocks.get(step_id.0 as usize) else {
            continue;
        };
        let Some(preheader) = function.blocks.get(preheader_id.0 as usize) else {
            continue;
        };
        let Some(backedge) = one_target_to(&step.terminator, header_id) else {
            continue;
        };
        let Some(entry_edge) = one_target_to(&preheader.terminator, header_id) else {
            continue;
        };
        let Some((start_type, start)) =
            entry_edge
                .arguments
                .get(counter_index)
                .and_then(|operand| match operand {
                    l::Operand::Constant(constant) => integer_constant(constant),
                    l::Operand::Value(_) => None,
                })
        else {
            continue;
        };
        let Some(next_counter) = backedge.arguments.get(counter_index) else {
            continue;
        };
        let next_counter = match next_counter {
            l::Operand::Value(value) => *value,
            l::Operand::Constant(_) => continue,
        };
        let Some(increment_instruction) = step
            .instructions
            .iter()
            .find(|instruction| instruction.result == Some(next_counter))
        else {
            continue;
        };
        let l::InstructionKind::Binary(l::BinaryOp::Add) = increment_instruction.kind else {
            continue;
        };
        let Some((step_counter, increment_type, increment)) =
            addition_parts(&increment_instruction.operands)
        else {
            continue;
        };
        let Some(step_counter_index) = step
            .parameters
            .iter()
            .position(|parameter| *parameter == step_counter)
        else {
            continue;
        };
        if start_type != bound_type
            || start_type != increment_type
            || function
                .values
                .get(counter.0 as usize)
                .map(|value| &value.ty)
                != Some(&l::ValueType::Data(start_type.clone()))
        {
            continue;
        }
        let Some(trip_count) = trip_count(&start_type, start, bound, increment, operation) else {
            continue;
        };
        return Some(LoopShape {
            header: header_id,
            body: then_target.block,
            step: step_id,
            exit_target: else_target.clone(),
            counter_index,
            step_counter_index,
            trip_count,
        });
    }
    None
}

fn addition_parts(operands: &[l::Operand]) -> Option<(l::ValueId, Type, i64)> {
    let [left, right] = operands else {
        return None;
    };
    match (left, right) {
        (l::Operand::Value(value), l::Operand::Constant(constant))
        | (l::Operand::Constant(constant), l::Operand::Value(value)) => {
            let (ty, increment) = integer_constant(constant)?;
            Some((*value, ty, increment))
        }
        _ => None,
    }
}

fn integer_constant(constant: &l::Constant) -> Option<(Type, i64)> {
    let l::ConstantKind::Integer(value) = constant.kind else {
        return None;
    };
    integer_limits(&constant.ty)?;
    Some((constant.ty.clone(), value))
}

fn integer_limits(ty: &Type) -> Option<(i128, i128)> {
    Some(match ty {
        Type::I8 => (i128::from(i8::MIN), i128::from(i8::MAX)),
        Type::U8 => (0, i128::from(u8::MAX)),
        Type::I16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        Type::U16 => (0, i128::from(u16::MAX)),
        Type::I32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        Type::U32 => (0, i128::from(u32::MAX)),
        Type::I64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
        Type::U64 => (0, i128::from(u64::MAX)),
        _ => return None,
    })
}

fn trip_count(
    ty: &Type,
    start: i64,
    bound: i64,
    increment: i64,
    operation: l::BinaryOp,
) -> Option<usize> {
    let (minimum, maximum) = integer_limits(ty)?;
    let start = i128::from(start);
    let bound = i128::from(bound);
    let increment = i128::from(increment);
    if increment <= 0 || start < minimum || start > maximum || bound < minimum || bound > maximum {
        return None;
    }
    let trips = match operation {
        l::BinaryOp::Lt if start >= bound => 0,
        l::BinaryOp::Lt => {
            let distance = bound.checked_sub(start)?;
            distance
                .checked_add(increment.checked_sub(1)?)?
                .checked_div(increment)?
        }
        l::BinaryOp::Le if start > bound => 0,
        l::BinaryOp::Le => bound.checked_sub(start)?.checked_div(increment)? + 1,
        _ => return None,
    };
    let final_value = start.checked_add(trips.checked_mul(increment)?)?;
    if final_value < minimum || final_value > maximum {
        return None;
    }
    usize::try_from(trips).ok()
}

fn transform(function: &mut l::Function, shape: &LoopShape) -> bool {
    let mut transformed = function.clone();
    if !transform_inner(&mut transformed, shape) {
        return false;
    }
    *function = transformed;
    true
}

fn transform_inner(function: &mut l::Function, shape: &LoopShape) -> bool {
    let header = function.blocks[shape.header.0 as usize].clone();
    let body = function.blocks[shape.body.0 as usize].clone();
    let step = function.blocks[shape.step.0 as usize].clone();
    let l::Terminator::Branch(body_target) = &body.terminator else {
        return false;
    };
    let l::Terminator::Branch(backedge) = &step.terminator else {
        return false;
    };
    let internal = internal_definitions(&header, &body, &step);
    let per_iteration_results = body
        .instructions
        .iter()
        .chain(&step.instructions)
        .filter(|instruction| instruction.result.is_some())
        .count();
    let new_parameter_sets =
        shape.trip_count.saturating_sub(1) - usize::from(shape.trip_count >= 3);
    let new_value_count = new_parameter_sets
        .checked_mul(header.parameters.len())
        .and_then(|count| {
            count
                .checked_add(per_iteration_results.checked_mul(shape.trip_count.saturating_sub(1))?)
        });
    let Some(new_value_count) = new_value_count else {
        return false;
    };
    if function
        .values
        .len()
        .checked_add(new_value_count)
        .is_none_or(|count| u32::try_from(count).is_err())
        || function
            .blocks
            .len()
            .checked_add(shape.trip_count.saturating_sub(3))
            .is_none_or(|count| u32::try_from(count).is_err())
    {
        return false;
    }

    let mut iteration_blocks = vec![shape.header];
    if shape.trip_count >= 2 {
        iteration_blocks.push(shape.body);
    }
    if shape.trip_count >= 3 {
        iteration_blocks.push(shape.step);
    }
    while iteration_blocks.len() < shape.trip_count {
        let id = l::BlockId(function.blocks.len() as u32);
        function.blocks.push(l::BasicBlock {
            id,
            source_name: None,
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: l::Terminator::Unreachable {
                pos: function.pos.clone(),
            },
        });
        iteration_blocks.push(id);
    }

    let header_types = header
        .parameters
        .iter()
        .map(|value| function.values[value.0 as usize].clone())
        .collect::<Vec<_>>();
    let mut iteration_parameters = Vec::with_capacity(shape.trip_count);
    iteration_parameters.push(header.parameters.clone());
    for iteration in 1..shape.trip_count {
        if iteration == 2 {
            iteration_parameters.push(step.parameters.clone());
            continue;
        }
        let mut parameters = Vec::with_capacity(header_types.len());
        for value in &header_types {
            let Some(id) = allocate_value(function, value.ty.clone(), value.source_name.clone())
            else {
                return false;
            };
            parameters.push(id);
        }
        iteration_parameters.push(parameters);
    }

    for (iteration, block_id) in iteration_blocks.iter().copied().enumerate() {
        let mut mapping = header
            .parameters
            .iter()
            .copied()
            .zip(
                iteration_parameters[iteration]
                    .iter()
                    .copied()
                    .map(l::Operand::Value),
            )
            .collect::<HashMap<_, _>>();
        let mut instructions = if iteration == 0 {
            header.instructions.clone()
        } else {
            Vec::new()
        };
        if !append_instructions(
            function,
            &body.instructions,
            &mut instructions,
            &mut mapping,
            &internal,
            iteration == 0,
        ) {
            return false;
        }
        let Some(step_arguments) = remap_operands(&body_target.arguments, &mapping, &internal)
        else {
            return false;
        };
        for (parameter, argument) in step.parameters.iter().copied().zip(step_arguments) {
            mapping.insert(parameter, argument);
        }
        if !append_instructions(
            function,
            &step.instructions,
            &mut instructions,
            &mut mapping,
            &internal,
            iteration == 0,
        ) {
            return false;
        }
        let Some(next_state) = remap_operands(&backedge.arguments, &mapping, &internal) else {
            return false;
        };
        let terminator = if let Some(next_block) = iteration_blocks.get(iteration + 1) {
            l::Terminator::Branch(l::BlockTarget {
                block: *next_block,
                arguments: next_state,
            })
        } else {
            for (parameter, value) in header.parameters.iter().copied().zip(next_state) {
                mapping.insert(parameter, value);
            }
            let Some(arguments) = remap_operands(&shape.exit_target.arguments, &mapping, &internal)
            else {
                return false;
            };
            l::Terminator::Branch(l::BlockTarget {
                block: shape.exit_target.block,
                arguments,
            })
        };
        function.blocks[block_id.0 as usize] = l::BasicBlock {
            id: block_id,
            source_name: Some(format!("for.unrolled.{iteration}")),
            parameters: iteration_parameters[iteration].clone(),
            instructions,
            terminator,
        };
    }

    for unused in [shape.body, shape.step]
        .into_iter()
        .filter(|block| !iteration_blocks.contains(block))
    {
        let parameters = if unused == shape.step {
            step.parameters.clone()
        } else {
            Vec::new()
        };
        function.blocks[unused.0 as usize] = l::BasicBlock {
            id: unused,
            source_name: Some("for.unrolled.unused".to_string()),
            parameters,
            instructions: Vec::new(),
            terminator: l::Terminator::Unreachable {
                pos: function.pos.clone(),
            },
        };
    }
    true
}

fn append_instructions(
    function: &mut l::Function,
    templates: &[l::Instruction],
    output: &mut Vec<l::Instruction>,
    mapping: &mut HashMap<l::ValueId, l::Operand>,
    internal: &HashSet<l::ValueId>,
    preserve_results: bool,
) -> bool {
    for template in templates {
        let Some(operands) = remap_operands(&template.operands, mapping, internal) else {
            return false;
        };
        let Some(invalidates) = template
            .invalidates
            .iter()
            .map(|value| remap_value(*value, mapping, internal))
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let result = if let Some(original) = template.result {
            let Some(mut value) = function.values.get(original.0 as usize).cloned() else {
                return false;
            };
            let Some(ty) = remap_value_type(&value.ty, mapping, internal) else {
                return false;
            };
            let result = if preserve_results {
                function.values[original.0 as usize].ty = ty;
                original
            } else {
                value.ty = ty;
                let Some(result) = allocate_value(function, value.ty, value.source_name.take())
                else {
                    return false;
                };
                result
            };
            mapping.insert(original, l::Operand::Value(result));
            Some(result)
        } else {
            None
        };
        output.push(l::Instruction {
            result,
            kind: template.kind.clone(),
            operands,
            invalidates,
            traps: template.traps.clone(),
            pos: template.pos.clone(),
        });
    }
    true
}

fn allocate_value(
    function: &mut l::Function,
    ty: l::ValueType,
    source_name: Option<String>,
) -> Option<l::ValueId> {
    let id = l::ValueId(u32::try_from(function.values.len()).ok()?);
    function.values.push(l::Value {
        id,
        ty,
        source_name,
    });
    Some(id)
}

fn remap_operands(
    operands: &[l::Operand],
    mapping: &HashMap<l::ValueId, l::Operand>,
    internal: &HashSet<l::ValueId>,
) -> Option<Vec<l::Operand>> {
    operands
        .iter()
        .map(|operand| match operand {
            l::Operand::Value(value) => mapping
                .get(value)
                .cloned()
                .or_else(|| (!internal.contains(value)).then_some(operand.clone())),
            l::Operand::Constant(_) => Some(operand.clone()),
        })
        .collect()
}

fn remap_value(
    value: l::ValueId,
    mapping: &HashMap<l::ValueId, l::Operand>,
    internal: &HashSet<l::ValueId>,
) -> Option<l::ValueId> {
    match mapping.get(&value) {
        Some(l::Operand::Value(value)) => Some(*value),
        Some(l::Operand::Constant(_)) => None,
        None if !internal.contains(&value) => Some(value),
        None => None,
    }
}

fn remap_value_type(
    ty: &l::ValueType,
    mapping: &HashMap<l::ValueId, l::Operand>,
    internal: &HashSet<l::ValueId>,
) -> Option<l::ValueType> {
    let mut ty = ty.clone();
    if let l::ValueType::Address(address) = &mut ty {
        if let Some(base) = address.array_base {
            address.array_base = Some(remap_value(base, mapping, internal)?);
        }
    }
    Some(ty)
}

fn internal_definitions(
    header: &l::BasicBlock,
    body: &l::BasicBlock,
    step: &l::BasicBlock,
) -> HashSet<l::ValueId> {
    header
        .parameters
        .iter()
        .chain(&body.parameters)
        .chain(&step.parameters)
        .copied()
        .chain(
            header
                .instructions
                .iter()
                .chain(&body.instructions)
                .chain(&step.instructions)
                .filter_map(|instruction| instruction.result),
        )
        .collect()
}

fn has_external_use(
    function: &l::Function,
    internal: &HashSet<l::ValueId>,
    loop_blocks: &[l::BlockId],
) -> bool {
    if function.values.iter().any(|value| {
        !internal.contains(&value.id)
            && matches!(&value.ty, l::ValueType::Address(address)
                if address.array_base.is_some_and(|base| internal.contains(&base)))
    }) {
        return true;
    }
    function
        .blocks
        .iter()
        .filter(|block| !loop_blocks.contains(&block.id))
        .any(|block| {
            block.instructions.iter().any(|instruction| {
                instruction.operands.iter().any(|operand| {
                    matches!(operand, l::Operand::Value(value) if internal.contains(value))
                }) || instruction
                    .invalidates
                    .iter()
                    .any(|value| internal.contains(value))
            }) || terminator_values(&block.terminator)
                .iter()
                .any(|value| internal.contains(value))
        })
}

fn predecessors(function: &l::Function) -> Vec<Vec<l::BlockId>> {
    let mut result = vec![Vec::new(); function.blocks.len()];
    for block in &function.blocks {
        for successor in block.terminator.successors() {
            if let Some(predecessors) = result.get_mut(successor.0 as usize) {
                predecessors.push(block.id);
            }
        }
    }
    result
}

fn one_target_to(terminator: &l::Terminator, destination: l::BlockId) -> Option<l::BlockTarget> {
    if matches!(terminator, l::Terminator::Suspend { .. }) {
        return None;
    }
    let targets = terminator
        .targets()
        .into_iter()
        .filter(|target| target.block == destination)
        .collect::<Vec<_>>();
    (targets.len() == 1).then(|| targets[0].clone())
}

fn terminator_values(terminator: &l::Terminator) -> Vec<l::ValueId> {
    let mut values = terminator.value_uses();
    if let l::Terminator::Suspend { invalidates, .. } = terminator {
        // External-use safety needs invalidation mentions outside the loop.
        values.extend(invalidates);
    }
    values
}
