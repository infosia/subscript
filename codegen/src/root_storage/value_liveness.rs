//! Per-value liveness for frame fields and interpreter slots.

use subscript_compiler::lir as l;

/// Native storage coalesces origins. Interpreter slots keep distinct SSA values.
pub(crate) fn value_interference(
    function: &l::Function,
) -> Result<crate::root_storage::Interference, String> {
    use std::collections::BTreeSet;

    let mut function = function.clone();
    function.liveness.value_origins = function.values.iter().map(|value| value.id).collect();
    let dependencies = value_dependencies(&function);
    let mut uses = vec![BTreeSet::new(); function.blocks.len()];
    let mut definitions = vec![BTreeSet::new(); function.blocks.len()];
    for block in &function.blocks {
        let index = block.id.0 as usize;
        definitions[index].extend(block.parameters.iter().copied());
        for instruction in &block.instructions {
            for operand in &instruction.operands {
                if let l::Operand::Value(value) = operand {
                    for value in std::iter::once(value).chain(&dependencies[value.0 as usize]) {
                        if !definitions[index].contains(value) {
                            uses[index].insert(*value);
                        }
                    }
                }
            }
            if let Some(result) = instruction.result {
                definitions[index].insert(result);
            }
        }
        for value in block.terminator.value_uses() {
            for value in std::iter::once(&value).chain(&dependencies[value.0 as usize]) {
                if !definitions[index].contains(value) {
                    uses[index].insert(*value);
                }
            }
        }
    }
    let mut live_ins = vec![BTreeSet::new(); function.blocks.len()];
    loop {
        let mut changed = false;
        for block in function.blocks.iter().rev() {
            let index = block.id.0 as usize;
            let mut live = uses[index].clone();
            for successor in block.successors() {
                live.extend(
                    live_ins[successor.0 as usize]
                        .difference(&definitions[index])
                        .copied(),
                );
            }
            if live != live_ins[index] {
                live_ins[index] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    function.liveness.live_ins = live_ins
        .into_iter()
        .map(|live| live.into_iter().collect())
        .collect();
    crate::root_storage::Interference::build_with_dependencies(&function, &dependencies)
}

/// A derived address retains its storage owner (compiler.md §323 rule 1).
/// A copied environment retains the environments of captured function values.
pub(crate) fn value_dependencies(
    function: &l::Function,
) -> Vec<std::collections::BTreeSet<l::ValueId>> {
    use std::collections::BTreeSet;
    let is_function = |value: l::ValueId| {
        function.values.get(value.0 as usize).is_some_and(
            |value| matches!(&value.ty, l::ValueType::Data(ty) if ty.function_type().is_some()),
        )
    };
    let mut dependencies = vec![BTreeSet::new(); function.values.len()];
    loop {
        let mut changed = false;
        let mut propagate = |destination: l::ValueId, source: l::ValueId, capture: bool| {
            let address = function
                .values
                .get(destination.0 as usize)
                .is_some_and(|value| matches!(value.ty, l::ValueType::Address(_)));
            if !address && (!is_function(destination) || !is_function(source)) {
                return;
            }
            let Some(mut incoming) = dependencies.get(source.0 as usize).cloned() else {
                return;
            };
            if capture {
                incoming.insert(source);
            }
            let Some(stored) = dependencies.get_mut(destination.0 as usize) else {
                return;
            };
            let previous = stored.len();
            stored.extend(incoming);
            changed |= previous != stored.len();
        };
        for block in &function.blocks {
            for instruction in &block.instructions {
                let Some(result) = instruction.result else {
                    continue;
                };
                let address = function
                    .values
                    .get(result.0 as usize)
                    .is_some_and(|value| matches!(value.ty, l::ValueType::Address(_)));
                let capture =
                    matches!(instruction.kind, l::InstructionKind::MakeClosure(_)) || address;
                if capture
                    || matches!(
                        instruction.kind,
                        l::InstructionKind::Copy
                            | l::InstructionKind::Coerce
                            | l::InstructionKind::StringOperand
                            | l::InstructionKind::Cast
                            | l::InstructionKind::NarrowNonNull(_)
                    )
                {
                    for operand in
                        instruction
                            .operands
                            .iter()
                            .take(if address { 1 } else { usize::MAX })
                    {
                        if let l::Operand::Value(source) = operand {
                            propagate(result, *source, capture);
                        }
                    }
                }
            }
            for target in block.terminator.targets() {
                let Some(destination) = function.blocks.get(target.block.0 as usize) else {
                    continue;
                };
                for (argument, parameter) in target.arguments.iter().zip(&destination.parameters) {
                    if let l::Operand::Value(source) = argument {
                        propagate(*parameter, *source, false);
                    }
                }
            }
            if let l::Terminator::Suspend {
                arguments,
                successor,
                resume_value,
                ..
            } = &block.terminator
            {
                let Some(destination) = function.blocks.get(successor.0 as usize) else {
                    continue;
                };
                for (argument, parameter) in arguments.iter().zip(
                    destination
                        .parameters
                        .iter()
                        .skip(usize::from(resume_value.is_some())),
                ) {
                    if let l::Operand::Value(source) = argument {
                        propagate(*parameter, *source, false);
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    dependencies
}
