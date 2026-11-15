//! Array invalidation scope before suspension SSA construction.

use super::verify_dominance::{dominators, predecessors};
use super::*;

/// compiler.md §297 rule 2: emission records candidates until the CFG is complete.
pub(super) fn scope_array_invalidations(function: &mut l::Function) {
    let dom = dominators(function, &predecessors(function));
    let is_array = |value: l::ValueId| {
        matches!(
            function.values[value.0 as usize].ty,
            l::ValueType::Data(Type::Array(_))
        )
    };
    let parameters = function
        .parameters
        .iter()
        .map(|parameter| parameter.value)
        .filter(|value| is_array(*value))
        .collect::<BTreeSet<_>>();
    let definitions = function
        .blocks
        .iter()
        .map(|block| {
            block
                .parameters
                .iter()
                .copied()
                .chain(
                    block
                        .instructions
                        .iter()
                        .filter_map(|instruction| instruction.result),
                )
                .filter(|value| is_array(*value))
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();

    for block in &mut function.blocks {
        let mut available = parameters.clone();
        for definition in &dom[block.id.0 as usize] {
            if *definition != block.id {
                available.extend(&definitions[definition.0 as usize]);
            }
        }
        available.extend(block.parameters.iter().copied());
        for instruction in &mut block.instructions {
            instruction
                .invalidates
                .retain(|value| available.contains(value));
            available.extend(instruction.result);
        }
        if let l::Terminator::Suspend { invalidates, .. } = &mut block.terminator {
            invalidates.retain(|value| available.contains(value));
        }
    }
}
