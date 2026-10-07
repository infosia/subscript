//! Per-value liveness for frame fields and interpreter slots.

use subscript_compiler::lir as l;

/// Native storage coalesces origins. Interpreter slots keep distinct SSA values.
pub(crate) fn value_interference(
    function: &l::Function,
) -> Result<crate::root_storage::Interference, String> {
    use std::collections::BTreeSet;

    let mut function = function.clone();
    function.liveness.value_origins = function.values.iter().map(|value| value.id).collect();
    let dependencies = environment_dependencies(&function);
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

/// A copied environment retains the environments of captured function values.
pub(super) fn environment_dependencies(
    function: &l::Function,
) -> Vec<std::collections::BTreeSet<l::ValueId>> {
    use std::collections::BTreeSet;
    let is_function = |value: l::ValueId| matches!(&function.values[value.0 as usize].ty, l::ValueType::Data(ty) if ty.function_type().is_some());
    let mut dependencies = vec![BTreeSet::new(); function.values.len()];
    loop {
        let mut changed = false;
        let mut propagate = |destination: l::ValueId, source: l::ValueId, capture: bool| {
            if !is_function(destination) || !is_function(source) {
                return;
            }
            let mut incoming = dependencies[source.0 as usize].clone();
            if capture {
                incoming.insert(source);
            }
            let stored = &mut dependencies[destination.0 as usize];
            let previous = stored.len();
            stored.extend(incoming);
            changed |= previous != stored.len();
        };
        for block in &function.blocks {
            for instruction in &block.instructions {
                let Some(result) = instruction.result else {
                    continue;
                };
                let capture = matches!(instruction.kind, l::InstructionKind::MakeClosure(_));
                if capture
                    || matches!(
                        instruction.kind,
                        l::InstructionKind::Copy
                            | l::InstructionKind::Coerce
                            | l::InstructionKind::Cast
                            | l::InstructionKind::NarrowNonNull(_)
                    )
                {
                    for operand in &instruction.operands {
                        if let l::Operand::Value(source) = operand {
                            propagate(result, *source, capture);
                        }
                    }
                }
            }
            for target in block.terminator.targets() {
                for (argument, parameter) in target
                    .arguments
                    .iter()
                    .zip(&function.blocks[target.block.0 as usize].parameters)
                {
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
                for (argument, parameter) in arguments.iter().zip(
                    function.blocks[successor.0 as usize]
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
