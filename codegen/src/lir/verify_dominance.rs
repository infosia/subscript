//! Verification of dominance, suspend boundaries, and address invalidation.

use super::verify::finding;
use super::*;

#[derive(Clone, Copy)]
enum DefinitionSite {
    Entry,
    BlockEntry(l::BlockId),
    Instruction(l::BlockId, usize),
}

pub(super) fn verify_dominance(function: &l::Function, errors: &mut Vec<VerifyError>) {
    let predecessors = predecessors(function);
    let dominators = dominators(function, &predecessors);
    let mut definitions = vec![None; function.values.len()];
    for parameter in &function.parameters {
        set_definition(&mut definitions, parameter.value, DefinitionSite::Entry);
    }
    for block in &function.blocks {
        for parameter in &block.parameters {
            set_definition(
                &mut definitions,
                *parameter,
                DefinitionSite::BlockEntry(block.id),
            );
        }
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let Some(result) = instruction.result {
                set_definition(
                    &mut definitions,
                    result,
                    DefinitionSite::Instruction(block.id, index),
                );
            }
        }
    }
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            for operand in &instruction.operands {
                if let l::Operand::Value(value) = operand {
                    check_dominates(
                        function,
                        *value,
                        block.id,
                        index,
                        &definitions,
                        &dominators,
                        errors,
                    );
                }
            }
        }
        for value in terminator_values(&block.terminator) {
            check_dominates(
                function,
                value,
                block.id,
                block.instructions.len(),
                &definitions,
                &dominators,
                errors,
            );
        }
    }
    verify_array_base_dominance(function, &definitions, &dominators, errors);
    verify_suspend_definition_boundaries(function, &definitions, &dominators, errors);
}

fn verify_array_base_dominance(
    function: &l::Function,
    definitions: &[Option<DefinitionSite>],
    dominators: &[BTreeSet<l::BlockId>],
    errors: &mut Vec<VerifyError>,
) {
    for value in &function.values {
        let l::ValueType::Address(address) = &value.ty else {
            continue;
        };
        let Some(base) = address.array_base else {
            continue;
        };
        let Some(base_value) = function.values.get(base.0 as usize) else {
            errors.push(finding(
                function,
                format!(
                    "address value {} names undeclared array base value {}",
                    value.id.0, base.0
                ),
            ));
            continue;
        };
        if !matches!(base_value.ty, l::ValueType::Data(Type::Array(_))) {
            errors.push(finding(
                function,
                format!(
                    "address value {} names non-array base value {}",
                    value.id.0, base.0
                ),
            ));
            continue;
        }
        let Some(address_definition) = definitions
            .get(value.id.0 as usize)
            .and_then(|definition| *definition)
        else {
            continue;
        };
        let Some(base_definition) = definitions
            .get(base.0 as usize)
            .and_then(|definition| *definition)
        else {
            errors.push(finding(
                function,
                format!(
                    "address value {} names array base value {} without a definition",
                    value.id.0, base.0
                ),
            ));
            continue;
        };
        if !definition_dominates_definition(base_definition, address_definition, dominators) {
            errors.push(finding(
                function,
                format!(
                    "array base value {} does not dominate address value {}",
                    base.0, value.id.0
                ),
            ));
        }
    }
}

fn verify_suspend_definition_boundaries(
    function: &l::Function,
    definitions: &[Option<DefinitionSite>],
    dominators: &[BTreeSet<l::BlockId>],
    errors: &mut Vec<VerifyError>,
) {
    for suspend_block in &function.blocks {
        let l::Terminator::Suspend { successor, .. } = suspend_block.terminator else {
            continue;
        };
        for block in &function.blocks {
            if !dominators
                .get(block.id.0 as usize)
                .is_some_and(|set| set.contains(&successor))
            {
                continue;
            }
            let mut uses = block
                .instructions
                .iter()
                .flat_map(|instruction| &instruction.operands)
                .filter_map(|operand| match operand {
                    l::Operand::Value(value) => Some(*value),
                    l::Operand::Constant(_) => None,
                })
                .collect::<Vec<_>>();
            uses.extend(terminator_values(&block.terminator));
            for value in uses {
                let Some(definition) = definitions
                    .get(value.0 as usize)
                    .and_then(|definition| *definition)
                else {
                    continue;
                };
                let inside_resume_region = match definition {
                    DefinitionSite::Entry => false,
                    DefinitionSite::BlockEntry(block) | DefinitionSite::Instruction(block, _) => {
                        dominators
                            .get(block.0 as usize)
                            .is_some_and(|set| set.contains(&successor))
                    }
                };
                if !inside_resume_region {
                    errors.push(finding(
                        function,
                        format!(
                            "use of value {} in block {} crosses suspend in block {} without a successor parameter",
                            value.0, block.id.0, suspend_block.id.0
                        ),
                    ));
                }
            }
        }
    }
}

fn definition_dominates_definition(
    definition: DefinitionSite,
    target: DefinitionSite,
    dominators: &[BTreeSet<l::BlockId>],
) -> bool {
    match (definition, target) {
        (DefinitionSite::Entry, _) => true,
        (DefinitionSite::BlockEntry(_), DefinitionSite::Entry)
        | (DefinitionSite::Instruction(_, _), DefinitionSite::Entry) => false,
        (DefinitionSite::BlockEntry(definition), DefinitionSite::BlockEntry(target)) => {
            definition == target
                || dominators
                    .get(target.0 as usize)
                    .is_some_and(|set| set.contains(&definition))
        }
        (DefinitionSite::BlockEntry(definition), DefinitionSite::Instruction(target, _)) => {
            definition == target
                || dominators
                    .get(target.0 as usize)
                    .is_some_and(|set| set.contains(&definition))
        }
        (
            DefinitionSite::Instruction(definition_block, _),
            DefinitionSite::BlockEntry(target_block),
        ) => {
            definition_block != target_block
                && dominators
                    .get(target_block.0 as usize)
                    .is_some_and(|set| set.contains(&definition_block))
        }
        (
            DefinitionSite::Instruction(definition_block, definition_index),
            DefinitionSite::Instruction(target_block, target_index),
        ) => {
            (definition_block == target_block && definition_index < target_index)
                || (definition_block != target_block
                    && dominators
                        .get(target_block.0 as usize)
                        .is_some_and(|set| set.contains(&definition_block)))
        }
    }
}

fn set_definition(
    definitions: &mut [Option<DefinitionSite>],
    value: l::ValueId,
    site: DefinitionSite,
) {
    if let Some(slot) = definitions.get_mut(value.0 as usize) {
        *slot = Some(site);
    }
}

fn check_dominates(
    function: &l::Function,
    value: l::ValueId,
    use_block: l::BlockId,
    use_index: usize,
    definitions: &[Option<DefinitionSite>],
    dominators: &[BTreeSet<l::BlockId>],
    errors: &mut Vec<VerifyError>,
) {
    let Some(definition) = definitions.get(value.0 as usize).and_then(|site| *site) else {
        return;
    };
    let use_site = DefinitionSite::Instruction(use_block, use_index);
    let valid = definition_dominates_definition(definition, use_site, dominators);
    if !valid {
        errors.push(finding(
            function,
            format!(
                "use of value {} in block {} is not dominated by its definition",
                value.0, use_block.0
            ),
        ));
    }
}

pub(super) fn predecessors(function: &l::Function) -> Vec<Vec<l::BlockId>> {
    let mut predecessors = vec![Vec::new(); function.blocks.len()];
    for block in &function.blocks {
        for successor in block.terminator.successors() {
            if let Some(list) = predecessors.get_mut(successor.0 as usize) {
                list.push(block.id);
            }
        }
    }
    predecessors
}

pub(super) fn dominators(
    function: &l::Function,
    predecessors: &[Vec<l::BlockId>],
) -> Vec<BTreeSet<l::BlockId>> {
    let all: BTreeSet<_> = function.blocks.iter().map(|block| block.id).collect();
    let mut sets = vec![all.clone(); function.blocks.len()];
    if let Some(entry) = sets.get_mut(function.entry.0 as usize) {
        *entry = [function.entry].into_iter().collect();
    }
    let mut changed = true;
    while changed {
        changed = false;
        for block in &function.blocks {
            if block.id == function.entry {
                continue;
            }
            let preds = &predecessors[block.id.0 as usize];
            let mut next = if let Some(first) = preds.first() {
                sets[first.0 as usize].clone()
            } else {
                BTreeSet::new()
            };
            for pred in preds.iter().skip(1) {
                next.retain(|candidate| sets[pred.0 as usize].contains(candidate));
            }
            next.insert(block.id);
            if next != sets[block.id.0 as usize] {
                sets[block.id.0 as usize] = next;
                changed = true;
            }
        }
    }
    sets
}

pub(super) fn successors(terminator: &l::Terminator) -> Vec<l::BlockId> {
    terminator.successors()
}

pub(super) fn terminator_values(terminator: &l::Terminator) -> Vec<l::ValueId> {
    terminator.value_uses()
}

pub(super) fn verify_address_invalidation(function: &l::Function, errors: &mut Vec<VerifyError>) {
    for value in &function.values {
        let l::ValueType::Address(address) = &value.ty else {
            continue;
        };
        let Some(base) = address.array_base else {
            continue;
        };
        let Some(start) = definition_position(function, value.id) else {
            continue;
        };
        let mut queue = VecDeque::from([(start.0, start.1, false)]);
        let mut seen = HashSet::new();
        while let Some((block_id, mut index, mut invalidated)) = queue.pop_front() {
            if !seen.insert((block_id, index, invalidated)) {
                continue;
            }
            let Some(block) = function.blocks.get(block_id.0 as usize) else {
                continue;
            };
            while index < block.instructions.len() {
                let instruction = &block.instructions[index];
                if instruction.result == Some(value.id) {
                    // A back edge executes the address definition again;
                    // the new dynamic address starts valid even if the
                    // preceding iteration invalidated its predecessor.
                    invalidated = false;
                }
                if invalidated && instruction_uses(instruction, value.id) {
                    errors.push(finding(
                        function,
                        format!(
                            "address value {} is used in block {} after array value {} was invalidated",
                            value.id.0, block_id.0, base.0
                        ),
                    ));
                    break;
                }
                if instruction.invalidates.contains(&base) {
                    invalidated = true;
                }
                index += 1;
            }
            if invalidated && terminator_values(&block.terminator).contains(&value.id) {
                errors.push(finding(
                    function,
                    format!(
                        "address value {} reaches block {} terminator after array value {} was invalidated",
                        value.id.0, block_id.0, base.0
                    ),
                ));
            }
            let term_invalidates = match &block.terminator {
                l::Terminator::Suspend { invalidates, .. } => invalidates.contains(&base),
                _ => false,
            };
            for successor in successors(&block.terminator) {
                queue.push_back((successor, 0, invalidated || term_invalidates));
            }
        }
    }
}

fn definition_position(function: &l::Function, value: l::ValueId) -> Option<(l::BlockId, usize)> {
    if function
        .parameters
        .iter()
        .any(|parameter| parameter.value == value)
    {
        return Some((function.entry, 0));
    }
    for block in &function.blocks {
        if block.parameters.contains(&value) {
            return Some((block.id, 0));
        }
        for (index, instruction) in block.instructions.iter().enumerate() {
            if instruction.result == Some(value) {
                return Some((block.id, index + 1));
            }
        }
    }
    None
}

fn instruction_uses(instruction: &l::Instruction, value: l::ValueId) -> bool {
    instruction.operands.contains(&l::Operand::Value(value))
}
