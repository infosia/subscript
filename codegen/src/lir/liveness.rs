//! Suspension live-in threading and local storage classification.

use super::verify_dominance::{dominators, predecessors, successors, terminator_values};
use super::*;

/// Turns every value live across a suspend edge into an explicit successor
/// definition and repairs SSA at downstream joins. The input graph is already
/// ordinary SSA; a suspension is the additional definition boundary.
pub(super) fn thread_suspension_live_ins(function: &mut l::Function) -> Result<(), LowerError> {
    let original_value_count = function.values.len();
    let live_in = lir_live_ins(function, original_value_count);
    for block in &mut function.blocks {
        let l::Terminator::Suspend {
            successor,
            invalidates,
            ..
        } = &mut block.terminator
        else {
            continue;
        };
        let successor_live_in = live_in
            .get(successor.0 as usize)
            .cloned()
            .unwrap_or_default();
        invalidates.retain(|value| successor_live_in.contains(value));
    }
    let mut value_origins = (0..original_value_count)
        .map(|index| l::ValueId(index as u32))
        .collect::<Vec<_>>();
    let block_count = function.blocks.len();
    let mut carried = vec![Vec::<(l::ValueId, l::ValueId)>::new(); block_count];
    let mut suspend_successors = BTreeSet::new();
    for block in &function.blocks {
        if let l::Terminator::Suspend { successor, .. } = block.terminator {
            suspend_successors.insert(successor);
        }
    }

    for successor in suspend_successors {
        let Some(destination) = function.blocks.get(successor.0 as usize) else {
            return Err(LowerError {
                pos: function.pos.clone(),
                message: format!("suspend successor block {} is missing", successor.0),
            });
        };
        let values = live_in
            .get(successor.0 as usize)
            .cloned()
            .unwrap_or_default();
        let destination_id = destination.id;
        for original in values {
            let definition = function
                .values
                .get(original.0 as usize)
                .cloned()
                .ok_or_else(|| LowerError {
                    pos: function.pos.clone(),
                    message: format!("live-in value {} is missing", original.0),
                })?;
            let parameter = l::ValueId(function.values.len() as u32);
            function.values.push(l::Value {
                id: parameter,
                ty: definition.ty,
                fresh_owner: definition.fresh_owner,
                source_name: definition.source_name,
            });
            value_origins.push(original);
            function.blocks[destination_id.0 as usize]
                .parameters
                .push(parameter);
            carried[destination_id.0 as usize].push((original, parameter));
        }
    }

    let origins = carried
        .iter()
        .flat_map(|values| values.iter().map(|(original, _)| *original))
        .collect::<BTreeSet<_>>();
    if origins.is_empty() {
        function.liveness = l::Liveness {
            live_ins: live_in
                .into_iter()
                .map(|values| values.into_iter().collect())
                .collect(),
            value_origins,
        };
        return Ok(());
    }

    let predecessors = predecessors(function);
    let reachable = reachable_blocks(function);
    let function_parameters = function
        .parameters
        .iter()
        .map(|parameter| parameter.value)
        .collect::<HashSet<_>>();

    for origin in origins {
        let mut special = vec![None; block_count];
        for (block_index, values) in carried.iter().enumerate() {
            special[block_index] = values
                .iter()
                .find_map(|(candidate, parameter)| (*candidate == origin).then_some(*parameter));
        }
        let mut merges = vec![None; block_count];
        let mut incoming = vec![None; block_count];
        let mut outgoing = vec![None; block_count];

        loop {
            let mut changed = false;
            for block_index in 0..block_count {
                if !reachable[block_index] {
                    continue;
                }
                let block_id = function.blocks[block_index].id;
                let block_parameter_definition = function.blocks[block_index]
                    .parameters
                    .iter()
                    .take_while(|value| (value.0 as usize) < original_value_count)
                    .any(|value| *value == origin);
                let function_parameter_definition =
                    block_id == function.entry && function_parameters.contains(&origin);
                let instruction_definition = function.blocks[block_index]
                    .instructions
                    .iter()
                    .any(|instruction| instruction.result == Some(origin));

                let next_in = if let Some(parameter) = special[block_index] {
                    Some(parameter)
                } else if block_parameter_definition || function_parameter_definition {
                    Some(origin)
                } else if let Some(parameter) = merges[block_index] {
                    Some(parameter)
                } else {
                    let pred_versions = predecessors[block_index]
                        .iter()
                        .filter(|predecessor| reachable[predecessor.0 as usize])
                        .map(|predecessor| outgoing[predecessor.0 as usize])
                        .collect::<Vec<_>>();
                    let versions = pred_versions.into_iter().flatten().collect::<BTreeSet<_>>();
                    if versions.is_empty() {
                        None
                    } else {
                        if versions.len() == 1 {
                            versions.first().copied()
                        } else if live_in[block_index].contains(&origin) {
                            let definition = function.values[origin.0 as usize].clone();
                            let parameter = l::ValueId(function.values.len() as u32);
                            function.values.push(l::Value {
                                id: parameter,
                                ty: definition.ty,
                                fresh_owner: definition.fresh_owner,
                                source_name: definition.source_name,
                            });
                            value_origins.push(origin);
                            function.blocks[block_index].parameters.push(parameter);
                            merges[block_index] = Some(parameter);
                            changed = true;
                            Some(parameter)
                        } else {
                            versions.first().copied()
                        }
                    }
                };
                let next_out = if block_parameter_definition
                    || function_parameter_definition
                    || instruction_definition
                {
                    Some(origin)
                } else {
                    next_in
                };
                if incoming[block_index] != next_in {
                    incoming[block_index] = next_in;
                    changed = true;
                }
                if outgoing[block_index] != next_out {
                    outgoing[block_index] = next_out;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for block_index in 0..block_count {
            if !reachable[block_index] {
                continue;
            }
            let block_id = function.blocks[block_index].id;
            let block_parameter_definition = function.blocks[block_index]
                .parameters
                .iter()
                .take_while(|value| (value.0 as usize) < original_value_count)
                .any(|value| *value == origin);
            let function_parameter_definition =
                block_id == function.entry && function_parameters.contains(&origin);
            let mut current = if block_parameter_definition || function_parameter_definition {
                Some(origin)
            } else {
                incoming[block_index]
            };

            let parameters = function.blocks[block_index].parameters.clone();
            for parameter in parameters {
                if parameter != origin {
                    replace_address_base(function, parameter, origin, current);
                }
            }
            let instruction_count = function.blocks[block_index].instructions.len();
            for instruction_index in 0..instruction_count {
                let instruction = &mut function.blocks[block_index].instructions[instruction_index];
                replace_operands(&mut instruction.operands, origin, current);
                replace_ids(&mut instruction.invalidates, origin, current);
                let result = instruction.result;
                if let Some(result) = result {
                    replace_address_base(function, result, origin, current);
                }
                if result == Some(origin) {
                    current = Some(origin);
                }
            }
            replace_terminator_uses(
                &mut function.blocks[block_index].terminator,
                origin,
                current,
            );
        }

        for (destination_index, merge) in merges.iter().enumerate().take(block_count) {
            let Some(_parameter) = merge else {
                continue;
            };
            let destination = function.blocks[destination_index].id;
            for (source_index, version) in outgoing.iter().copied().enumerate().take(block_count) {
                if !reachable[source_index] {
                    continue;
                }
                if let Some(version) = version {
                    append_normal_edge_argument(
                        &mut function.blocks[source_index].terminator,
                        destination,
                        l::Operand::Value(version),
                    );
                }
            }
        }

        for (source_index, version) in outgoing.iter().copied().enumerate().take(block_count) {
            let successor = match &function.blocks[source_index].terminator {
                l::Terminator::Suspend { successor, .. } => *successor,
                _ => continue,
            };
            if carried[successor.0 as usize]
                .iter()
                .any(|(candidate, _)| *candidate == origin)
            {
                let version = version.ok_or_else(|| LowerError {
                    pos: function.pos.clone(),
                    message: format!(
                        "value {} is live at suspend in block {} but has no reaching definition",
                        origin.0, function.blocks[source_index].id.0
                    ),
                })?;
                if let l::Terminator::Suspend { arguments, .. } =
                    &mut function.blocks[source_index].terminator
                {
                    arguments.push(l::Operand::Value(version));
                }
            }
        }
    }
    function.liveness = l::Liveness {
        live_ins: live_in
            .into_iter()
            .map(|values| values.into_iter().collect())
            .collect(),
        value_origins,
    };
    Ok(())
}

/// Computes the value live-ins with the lowering's single graph fixed point.
fn lir_live_ins(function: &l::Function, original_value_count: usize) -> Vec<BTreeSet<l::ValueId>> {
    let mut uses = vec![BTreeSet::new(); function.blocks.len()];
    let mut definitions = vec![BTreeSet::new(); function.blocks.len()];
    for block in &function.blocks {
        let index = block.id.0 as usize;
        definitions[index].extend(
            block
                .parameters
                .iter()
                .copied()
                .filter(|value| (value.0 as usize) < original_value_count),
        );
        for instruction in &block.instructions {
            for operand in &instruction.operands {
                if let l::Operand::Value(value) = operand {
                    if !definitions[index].contains(value) {
                        uses[index].insert(*value);
                    }
                }
            }
            if let Some(result) = instruction.result {
                definitions[index].insert(result);
            }
        }
        for value in terminator_values(&block.terminator) {
            if !definitions[index].contains(&value) {
                uses[index].insert(value);
            }
        }
    }

    let mut live_in = vec![BTreeSet::new(); function.blocks.len()];
    let mut live_out = vec![BTreeSet::new(); function.blocks.len()];
    loop {
        let mut changed = false;
        for block in function.blocks.iter().rev() {
            let index = block.id.0 as usize;
            let next_out = successors(&block.terminator)
                .into_iter()
                .filter_map(|successor| live_in.get(successor.0 as usize))
                .flat_map(|values| values.iter().copied())
                .collect::<BTreeSet<_>>();
            let mut next_in = uses[index].clone();
            next_in.extend(
                next_out
                    .iter()
                    .filter(|value| !definitions[index].contains(value))
                    .copied(),
            );
            if live_out[index] != next_out {
                live_out[index] = next_out;
                changed = true;
            }
            if live_in[index] != next_in {
                live_in[index] = next_in;
                changed = true;
            }
        }
        if !changed {
            return live_in;
        }
    }
}

fn reachable_blocks(function: &l::Function) -> Vec<bool> {
    let mut reachable = vec![false; function.blocks.len()];
    let mut queue = VecDeque::from([function.entry]);
    while let Some(block) = queue.pop_front() {
        let Some(mark) = reachable.get_mut(block.0 as usize) else {
            continue;
        };
        if *mark {
            continue;
        }
        *mark = true;
        if let Some(block) = function.blocks.get(block.0 as usize) {
            queue.extend(successors(&block.terminator));
        }
    }
    reachable
}

/// Marks storage that a resumed activation must read before any redefinition.
pub(super) fn classify_local_storage(function: &mut l::Function) {
    let predecessors = predecessors(function);
    let dominators = dominators(function, &predecessors);
    let mut store_blocks = vec![BTreeSet::new(); function.locals.len()];
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let l::InstructionKind::StoreLocal(local) = instruction.kind {
                if let Some(stores) = store_blocks.get_mut(local.0 as usize) {
                    stores.insert(block.id);
                }
            }
        }
    }
    let required = function
        .locals
        .iter()
        .filter(|local| local_requires_frame(function, local.id, &store_blocks, &dominators))
        .map(|local| local.id)
        .collect::<HashSet<_>>();
    for local in &mut function.locals {
        local.storage = if required.contains(&local.id) {
            l::LocalStorageClass::Frame
        } else {
            l::LocalStorageClass::Activation
        };
    }
}

fn local_requires_frame(
    function: &l::Function,
    local: l::LocalId,
    store_blocks: &[BTreeSet<l::BlockId>],
    dominators: &[BTreeSet<l::BlockId>],
) -> bool {
    let Some(store_blocks) = store_blocks.get(local.0 as usize) else {
        return false;
    };
    function.blocks.iter().any(|suspend| {
        let l::Terminator::Suspend { successor, .. } = suspend.terminator else {
            return false;
        };
        let definition_dominates = store_blocks.iter().any(|definition| {
            *definition == suspend.id
                || dominators
                    .get(suspend.id.0 as usize)
                    .is_some_and(|blocks| blocks.contains(definition))
        });
        definition_dominates && local_read_before_redefinition(function, successor, local)
    })
}

fn local_read_before_redefinition(
    function: &l::Function,
    start: l::BlockId,
    local: l::LocalId,
) -> bool {
    let mut pending = VecDeque::from([start]);
    let mut visited = HashSet::new();
    while let Some(block) = pending.pop_front() {
        if !visited.insert(block) {
            continue;
        }
        let Some(block) = function.blocks.get(block.0 as usize) else {
            continue;
        };
        let mut redefined = false;
        for instruction in &block.instructions {
            match instruction.kind {
                l::InstructionKind::LoadLocal(id) | l::InstructionKind::AddressOfLocal(id)
                    if id == local =>
                {
                    return true;
                }
                l::InstructionKind::StoreLocal(id) if id == local => {
                    redefined = true;
                    break;
                }
                _ => {}
            }
        }
        if !redefined {
            pending.extend(successors(&block.terminator));
        }
    }
    false
}

fn replace_address_base(
    function: &mut l::Function,
    value: l::ValueId,
    original: l::ValueId,
    replacement: Option<l::ValueId>,
) {
    let Some(replacement) = replacement else {
        return;
    };
    if let Some(l::Value {
        ty: l::ValueType::Address(address),
        ..
    }) = function.values.get_mut(value.0 as usize)
    {
        if address.array_base == Some(original) {
            address.array_base = Some(replacement);
        }
    }
}

fn replace_operands(
    operands: &mut [l::Operand],
    original: l::ValueId,
    replacement: Option<l::ValueId>,
) {
    let Some(replacement) = replacement else {
        return;
    };
    for operand in operands {
        if matches!(operand, l::Operand::Value(value) if *value == original) {
            *operand = l::Operand::Value(replacement);
        }
    }
}

fn replace_ids(values: &mut [l::ValueId], original: l::ValueId, replacement: Option<l::ValueId>) {
    let Some(replacement) = replacement else {
        return;
    };
    for value in values {
        if *value == original {
            *value = replacement;
        }
    }
}

fn replace_terminator_uses(
    terminator: &mut l::Terminator,
    original: l::ValueId,
    replacement: Option<l::ValueId>,
) {
    if let Some(replacement) = replacement {
        // A replacement must update invalidation mentions with the read uses.
        terminator.map_values(|value| {
            if value == original {
                replacement
            } else {
                value
            }
        });
    }
}

fn append_normal_edge_argument(
    terminator: &mut l::Terminator,
    destination: l::BlockId,
    argument: l::Operand,
) {
    let append = |target: &mut l::BlockTarget| {
        if target.block == destination {
            target.arguments.push(argument.clone());
        }
    };
    match terminator {
        l::Terminator::Branch(target) => append(target),
        l::Terminator::ConditionalBranch {
            then_target,
            else_target,
            ..
        } => {
            append(then_target);
            append(else_target);
        }
        l::Terminator::Switch { arms, default, .. } => {
            for arm in arms {
                append(&mut arm.target);
            }
            append(default);
        }
        l::Terminator::Return { .. }
        | l::Terminator::Unreachable { .. }
        | l::Terminator::Trap(_)
        | l::Terminator::Suspend { .. } => {}
    }
}
