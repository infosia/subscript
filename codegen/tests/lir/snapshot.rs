use super::*;

pub(super) fn print_snapshot_module(module: &Module) -> String {
    let mut snapshot = module.clone();
    for function in &mut snapshot.functions {
        let legacy_suspend_invalidates = legacy_suspend_invalidates(function);
        let values = function.values.clone();
        for block in &mut function.blocks {
            if let Terminator::Suspend { invalidates, .. } = &mut block.terminator {
                // The committed text record holds the all-arrays-so-far
                // view. Rebuild it for this snapshot; the verifier and
                // interpreter tests inspect the real terminator.
                *invalidates = legacy_suspend_invalidates[block.id.0 as usize].clone();
            }
            for instruction in &mut block.instructions {
                let InstructionKind::Call(target) = &mut instruction.kind else {
                    continue;
                };
                if !matches!(
                    target.kind,
                    lir::CallTargetKind::Intrinsic(_) | lir::CallTargetKind::BuiltinMethod(_)
                ) {
                    continue;
                }
                let operand_types = instruction
                    .operands
                    .iter()
                    .map(|operand| match operand {
                        Operand::Value(value) => values[value.0 as usize].ty.clone(),
                        Operand::Constant(constant) => ValueType::Data(constant.ty.clone()),
                    })
                    .collect::<Vec<_>>();
                let signature = module
                    .operation_signatures(&target.kind)
                    .find(|signature| {
                        signature.parameter_types == operand_types
                            && signature.return_type == target.return_type
                    })
                    .expect("snapshot call has a checker-derived signature");
                target.parameter_types = signature.parameter_types.clone();
            }
        }
    }
    print_module(&snapshot)
}

#[derive(Clone, Copy)]
enum SnapshotDefinition {
    Entry,
    Block(lir::BlockId),
    Instruction(lir::BlockId, usize),
}

fn legacy_suspend_invalidates(function: &lir::Function) -> Vec<Vec<lir::ValueId>> {
    let block_count = function.blocks.len();
    let mut predecessors = vec![Vec::new(); block_count];
    for block in &function.blocks {
        for successor in snapshot_successors(&block.terminator) {
            if let Some(incoming) = predecessors.get_mut(successor.0 as usize) {
                incoming.push(block.id);
            }
        }
    }
    let all = (0..block_count)
        .map(|index| lir::BlockId(index as u32))
        .collect::<std::collections::BTreeSet<_>>();
    let mut dominators = vec![all; block_count];
    if let Some(entry) = dominators.get_mut(function.entry.0 as usize) {
        entry.clear();
        entry.insert(function.entry);
    }
    loop {
        let mut changed = false;
        for block in &function.blocks {
            if block.id == function.entry {
                continue;
            }
            let mut next = predecessors[block.id.0 as usize]
                .iter()
                .filter_map(|predecessor| dominators.get(predecessor.0 as usize).cloned())
                .reduce(|left, right| left.intersection(&right).copied().collect())
                .unwrap_or_default();
            next.insert(block.id);
            if next != dominators[block.id.0 as usize] {
                dominators[block.id.0 as usize] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut definitions = vec![None; function.values.len()];
    for parameter in &function.parameters {
        definitions[parameter.value.0 as usize] = Some(SnapshotDefinition::Entry);
    }
    for block in &function.blocks {
        for parameter in &block.parameters {
            definitions[parameter.0 as usize] = Some(SnapshotDefinition::Block(block.id));
        }
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let Some(result) = instruction.result {
                definitions[result.0 as usize] =
                    Some(SnapshotDefinition::Instruction(block.id, index));
            }
        }
    }

    let original_arrays = function
        .values
        .iter()
        .filter(|value| {
            function
                .liveness
                .value_origins
                .get(value.id.0 as usize)
                .is_some_and(|origin| *origin == value.id)
                && matches!(value.ty, ValueType::Data(Type::Array(_)))
        })
        .map(|value| value.id)
        .collect::<Vec<_>>();
    let mut result = vec![Vec::new(); block_count];
    for block in &function.blocks {
        let Terminator::Suspend {
            kind, successor, ..
        } = &block.terminator
        else {
            continue;
        };
        if matches!(kind, lir::SuspendKind::Yield(_)) {
            continue;
        }
        let original_value_count = function
            .liveness
            .value_origins
            .iter()
            .enumerate()
            .find_map(|(index, origin)| (*origin != lir::ValueId(index as u32)).then_some(index))
            .unwrap_or(function.values.len());
        let original_successor_parameters = function.blocks[successor.0 as usize]
            .parameters
            .iter()
            .copied()
            .filter(|value| (value.0 as usize) < original_value_count)
            .collect::<Vec<_>>();
        let watermark = original_successor_parameters
            .iter()
            .map(|value| value.0)
            .max()
            .unwrap_or_else(|| {
                snapshot_first_post_suspend_definition(
                    function,
                    *successor,
                    original_value_count,
                    &dominators,
                )
                .map_or(u32::MAX, |value| value.0.saturating_sub(1))
            });
        for origin in original_arrays
            .iter()
            .copied()
            .filter(|origin| origin.0 <= watermark)
        {
            let representative = snapshot_block_representative(function, block, origin)
                .or_else(|| {
                    function
                        .liveness
                        .value_origins
                        .iter()
                        .enumerate()
                        .filter(|(_, candidate_origin)| **candidate_origin == origin)
                        .filter_map(|(index, _)| {
                            let definition =
                                definitions.get(index).and_then(|definition| *definition)?;
                            (snapshot_definition_dominates(definition, block.id, &dominators)
                                && !snapshot_definition_crosses_uncarried_suspend(
                                    function,
                                    block.id,
                                    origin,
                                    definition,
                                    &dominators,
                                ))
                            .then_some((lir::ValueId(index as u32), definition))
                        })
                        .reduce(|left, right| {
                            if snapshot_definition_is_later(right.1, left.1, &dominators) {
                                right
                            } else {
                                left
                            }
                        })
                        .map(|(value, _)| value)
                })
                .unwrap_or(origin);
            result[block.id.0 as usize].push(representative);
        }
    }
    result
}

fn snapshot_definition_crosses_uncarried_suspend(
    function: &lir::Function,
    target: lir::BlockId,
    origin: lir::ValueId,
    definition: SnapshotDefinition,
    dominators: &[std::collections::BTreeSet<lir::BlockId>],
) -> bool {
    function.blocks.iter().any(|block| {
        let Terminator::Suspend { successor, .. } = &block.terminator else {
            return false;
        };
        snapshot_definition_dominates(definition, block.id, dominators)
            && dominators[target.0 as usize].contains(successor)
            && !function.blocks[successor.0 as usize]
                .parameters
                .iter()
                .any(|parameter| {
                    function
                        .liveness
                        .value_origins
                        .get(parameter.0 as usize)
                        .is_some_and(|candidate| *candidate == origin)
                })
            && function.blocks.iter().any(|candidate| {
                dominators[candidate.id.0 as usize].contains(successor)
                    && candidate
                        .instructions
                        .iter()
                        .any(|instruction| instruction.invalidates.contains(&origin))
            })
    })
}

fn snapshot_block_representative(
    function: &lir::Function,
    block: &lir::BasicBlock,
    origin: lir::ValueId,
) -> Option<lir::ValueId> {
    let mut candidates = block.parameters.clone();
    for instruction in &block.instructions {
        candidates.extend(
            instruction
                .operands
                .iter()
                .filter_map(|operand| match operand {
                    Operand::Value(value) => Some(*value),
                    Operand::Constant(_) => None,
                }),
        );
        candidates.extend(instruction.invalidates.iter().copied());
        if let Some(result) = instruction.result {
            candidates.push(result);
            if let ValueType::Address(address) = &function.values[result.0 as usize].ty {
                candidates.extend(address.array_base);
            }
        }
    }
    candidates.extend(snapshot_terminator_values(&block.terminator));
    candidates.into_iter().rev().find(|value| {
        function
            .liveness
            .value_origins
            .get(value.0 as usize)
            .is_some_and(|candidate| *candidate == origin)
    })
}

fn snapshot_terminator_values(terminator: &lir::Terminator) -> Vec<lir::ValueId> {
    let mut values = Vec::new();
    let mut push_operand = |operand: &Operand| {
        if let Operand::Value(value) = operand {
            values.push(*value);
        }
    };
    match terminator {
        Terminator::Branch(target) => target.arguments.iter().for_each(&mut push_operand),
        Terminator::ConditionalBranch {
            condition,
            then_target,
            else_target,
        } => {
            push_operand(condition);
            then_target.arguments.iter().for_each(&mut push_operand);
            else_target.arguments.iter().for_each(&mut push_operand);
        }
        Terminator::Switch {
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
        Terminator::Return { value, .. } => {
            if let Some(value) = value {
                push_operand(value);
            }
        }
        Terminator::Suspend {
            kind, arguments, ..
        } => {
            arguments.iter().for_each(&mut push_operand);
            match kind {
                lir::SuspendKind::Yield(value) => values.extend(*value),
                lir::SuspendKind::Async => {}
                lir::SuspendKind::AsyncCall { operands, .. } => {
                    values.extend(operands.iter().copied())
                }
                lir::SuspendKind::AsyncHandle { handle } => values.push(*handle),
            }
        }
        Terminator::Trap(_) | Terminator::Unreachable { .. } => {}
    }
    values
}

fn snapshot_first_post_suspend_definition(
    function: &lir::Function,
    successor: lir::BlockId,
    original_value_count: usize,
    dominators: &[std::collections::BTreeSet<lir::BlockId>],
) -> Option<lir::ValueId> {
    function
        .blocks
        .iter()
        .filter(|block| dominators[block.id.0 as usize].contains(&successor))
        .flat_map(|block| {
            block
                .parameters
                .iter()
                .copied()
                .filter(move |_| block.id != successor)
                .chain(
                    block
                        .instructions
                        .iter()
                        .filter_map(|instruction| instruction.result),
                )
        })
        .filter(|value| (value.0 as usize) < original_value_count)
        .min_by_key(|value| value.0)
}

fn snapshot_definition_dominates(
    definition: SnapshotDefinition,
    block: lir::BlockId,
    dominators: &[std::collections::BTreeSet<lir::BlockId>],
) -> bool {
    match definition {
        SnapshotDefinition::Entry => true,
        SnapshotDefinition::Block(definition) | SnapshotDefinition::Instruction(definition, _) => {
            dominators
                .get(block.0 as usize)
                .is_some_and(|set| set.contains(&definition))
        }
    }
}

fn snapshot_definition_is_later(
    candidate: SnapshotDefinition,
    current: SnapshotDefinition,
    dominators: &[std::collections::BTreeSet<lir::BlockId>],
) -> bool {
    match (candidate, current) {
        (SnapshotDefinition::Entry, _) => false,
        (_, SnapshotDefinition::Entry) => true,
        (SnapshotDefinition::Block(candidate), SnapshotDefinition::Block(current)) => {
            candidate != current
                && dominators
                    .get(candidate.0 as usize)
                    .is_some_and(|set| set.contains(&current))
        }
        (
            SnapshotDefinition::Instruction(candidate, candidate_index),
            SnapshotDefinition::Instruction(current, current_index),
        ) if candidate == current => candidate_index > current_index,
        (SnapshotDefinition::Instruction(candidate, _), SnapshotDefinition::Block(current))
            if candidate == current =>
        {
            true
        }
        (SnapshotDefinition::Block(candidate), SnapshotDefinition::Instruction(current, _))
            if candidate == current =>
        {
            false
        }
        (candidate, current) => {
            let candidate = match candidate {
                SnapshotDefinition::Block(block) | SnapshotDefinition::Instruction(block, _) => {
                    block
                }
                SnapshotDefinition::Entry => return false,
            };
            let current = match current {
                SnapshotDefinition::Block(block) | SnapshotDefinition::Instruction(block, _) => {
                    block
                }
                SnapshotDefinition::Entry => return true,
            };
            dominators
                .get(candidate.0 as usize)
                .is_some_and(|set| set.contains(&current))
        }
    }
}

fn snapshot_successors(terminator: &lir::Terminator) -> Vec<lir::BlockId> {
    match terminator {
        lir::Terminator::Branch(target) => vec![target.block],
        lir::Terminator::ConditionalBranch {
            then_target,
            else_target,
            ..
        } => vec![then_target.block, else_target.block],
        lir::Terminator::Switch { arms, default, .. } => arms
            .iter()
            .map(|arm| arm.target.block)
            .chain(std::iter::once(default.block))
            .collect(),
        lir::Terminator::Suspend { successor, .. } => vec![*successor],
        lir::Terminator::Return { .. }
        | lir::Terminator::Trap(_)
        | lir::Terminator::Unreachable { .. } => Vec::new(),
    }
}
