//! Shared live-range storage plan for managed LIR values.

use std::collections::{BTreeSet, HashSet};

use subscript_compiler::lir as l;

use crate::layout::{managed_words, Layouts};

#[derive(Debug, Clone)]
pub(crate) struct RootSlot {
    pub(crate) representative: l::ValueId,
    pub(crate) words: u32,
    pub(crate) offset: u32,
    members: Vec<l::ValueId>,
    ty: l::ValueType,
}

#[derive(Debug, Clone)]
pub(crate) struct RootStoragePlan {
    pub(crate) slots: Vec<RootSlot>,
    pub(crate) value_slots: Vec<Option<usize>>,
    pub(crate) clear_at_block_entry: Vec<Vec<usize>>,
    pub(crate) clear_after_instruction: Vec<Vec<Vec<usize>>>,
    pub(crate) words: u32,
}

fn internal(message: impl AsRef<str>) -> String {
    format!("internal error: {}", message.as_ref())
}

fn origin(function: &l::Function, value: l::ValueId) -> Result<l::ValueId, String> {
    function
        .liveness
        .value_origins
        .get(value.0 as usize)
        .copied()
        .ok_or_else(|| internal(format!("value {} has no liveness origin", value.0)))
}

fn record_operand(
    function: &l::Function,
    values: &mut BTreeSet<l::ValueId>,
    operand: &l::Operand,
) -> Result<(), String> {
    if let l::Operand::Value(value) = operand {
        values.insert(origin(function, *value)?);
    }
    Ok(())
}

fn record_target(
    function: &l::Function,
    values: &mut BTreeSet<l::ValueId>,
    target: &l::BlockTarget,
) -> Result<(), String> {
    for argument in &target.arguments {
        record_operand(function, values, argument)?;
    }
    Ok(())
}

fn record_terminator(
    function: &l::Function,
    values: &mut BTreeSet<l::ValueId>,
    terminator: &l::Terminator,
) -> Result<(), String> {
    match terminator {
        l::Terminator::Branch(target) => record_target(function, values, target)?,
        l::Terminator::ConditionalBranch {
            condition,
            then_target,
            else_target,
        } => {
            record_operand(function, values, condition)?;
            record_target(function, values, then_target)?;
            record_target(function, values, else_target)?;
        }
        l::Terminator::Switch {
            value,
            arms,
            default,
        } => {
            record_operand(function, values, value)?;
            for arm in arms {
                record_target(function, values, &arm.target)?;
            }
            record_target(function, values, default)?;
        }
        l::Terminator::Return { value, .. } => {
            if let Some(value) = value {
                record_operand(function, values, value)?;
            }
        }
        l::Terminator::Suspend {
            kind,
            arguments,
            invalidates,
            ..
        } => {
            match kind {
                l::SuspendKind::Yield(value) => {
                    if let Some(value) = value {
                        values.insert(origin(function, *value)?);
                    }
                }
                l::SuspendKind::Async => {}
                l::SuspendKind::AsyncCall { operands, .. } => {
                    for value in operands {
                        values.insert(origin(function, *value)?);
                    }
                }
                l::SuspendKind::AsyncHandle { handle } => {
                    values.insert(origin(function, *handle)?);
                }
            }
            for argument in arguments {
                record_operand(function, values, argument)?;
            }
            for value in invalidates {
                values.insert(origin(function, *value)?);
            }
        }
        l::Terminator::Unreachable { .. } | l::Terminator::Trap(_) => {}
    }
    Ok(())
}

fn successors(terminator: &l::Terminator) -> Vec<l::BlockId> {
    match terminator {
        l::Terminator::Branch(target) => vec![target.block],
        l::Terminator::ConditionalBranch {
            then_target,
            else_target,
            ..
        } => vec![then_target.block, else_target.block],
        l::Terminator::Switch { arms, default, .. } => arms
            .iter()
            .map(|arm| arm.target.block)
            .chain(std::iter::once(default.block))
            .collect(),
        l::Terminator::Suspend { successor, .. } => vec![*successor],
        l::Terminator::Return { .. }
        | l::Terminator::Unreachable { .. }
        | l::Terminator::Trap(_) => Vec::new(),
    }
}

fn add_interference(interference: &mut [HashSet<l::ValueId>], left: l::ValueId, right: l::ValueId) {
    if left == right {
        return;
    }
    interference[left.0 as usize].insert(right);
    interference[right.0 as usize].insert(left);
}

fn live_ins(function: &l::Function) -> Result<Vec<BTreeSet<l::ValueId>>, String> {
    if function.liveness.live_ins.len() != function.blocks.len() {
        return Err(internal(format!(
            "function {} carries {} live-in sets for {} blocks",
            function.id.0,
            function.liveness.live_ins.len(),
            function.blocks.len()
        )));
    }
    if function.liveness.value_origins.len() != function.values.len() {
        return Err(internal(format!(
            "function {} carries {} liveness origins for {} values",
            function.id.0,
            function.liveness.value_origins.len(),
            function.values.len()
        )));
    }
    function
        .liveness
        .live_ins
        .iter()
        .map(|values| {
            values
                .iter()
                .map(|value| origin(function, *value))
                .collect::<Result<BTreeSet<_>, _>>()
        })
        .collect()
}

pub(crate) fn value_interference(
    function: &l::Function,
) -> Result<Vec<HashSet<l::ValueId>>, String> {
    let live_in = live_ins(function)?;
    let live_out = function
        .blocks
        .iter()
        .map(|block| {
            successors(&block.terminator)
                .into_iter()
                .flat_map(|successor| live_in[successor.0 as usize].iter().copied())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut interference = vec![HashSet::new(); function.values.len()];
    for block in &function.blocks {
        let mut live = live_out[block.id.0 as usize].clone();
        record_terminator(function, &mut live, &block.terminator)?;
        for instruction in block.instructions.iter().rev() {
            if let Some(result) = instruction.result {
                let result = origin(function, result)?;
                for other in live.iter().copied() {
                    add_interference(&mut interference, result, other);
                }
                // A LIR instruction is one evaluation unit even when a
                // transcriber expands it into several runtime operations.
                // Its operands must therefore remain rooted until the result
                // has been produced and cannot share the result's slot.
                for operand in &instruction.operands {
                    if let l::Operand::Value(operand) = operand {
                        add_interference(&mut interference, result, origin(function, *operand)?);
                    }
                }
                for invalidated in &instruction.invalidates {
                    add_interference(&mut interference, result, origin(function, *invalidated)?);
                }
                live.remove(&result);
            }
            for operand in &instruction.operands {
                record_operand(function, &mut live, operand)?;
            }
            for value in &instruction.invalidates {
                live.insert(origin(function, *value)?);
            }
        }
        let parameters = block
            .parameters
            .iter()
            .map(|parameter| origin(function, *parameter))
            .collect::<Result<Vec<_>, _>>()?;
        for parameter in &parameters {
            for other in live.iter().copied() {
                add_interference(&mut interference, *parameter, other);
            }
            for other in &parameters {
                add_interference(&mut interference, *parameter, *other);
            }
        }
    }

    let entry_live = &live_in[function.entry.0 as usize];
    let parameters = function
        .parameters
        .iter()
        .map(|parameter| origin(function, parameter.value))
        .collect::<Result<Vec<_>, _>>()?;
    for parameter in &parameters {
        for other in entry_live.iter().copied() {
            add_interference(&mut interference, *parameter, other);
        }
        for other in &parameters {
            add_interference(&mut interference, *parameter, *other);
        }
    }
    Ok(interference)
}

fn managed_value_words(layouts: &Layouts, ty: &l::ValueType) -> Result<u32, String> {
    match ty {
        l::ValueType::Data(ty) => managed_words(layouts, ty),
        l::ValueType::Iterator(_) => Ok(4),
        l::ValueType::Address(_) => Ok(0),
    }
}

fn occupied_slots(
    value_slots: &[Option<usize>],
    values: impl IntoIterator<Item = l::ValueId>,
) -> BTreeSet<usize> {
    values
        .into_iter()
        .filter_map(|value| value_slots[value.0 as usize])
        .collect()
}

pub(crate) fn plan(function: &l::Function, layouts: &Layouts) -> Result<RootStoragePlan, String> {
    let live_in = live_ins(function)?;
    let interference = value_interference(function)?;
    let mut slots = Vec::<RootSlot>::new();
    let mut origin_slots = vec![None; function.values.len()];
    for value in &function.values {
        let value_origin = origin(function, value.id)?;
        if value_origin != value.id {
            continue;
        }
        let words = managed_value_words(layouts, &value.ty)?;
        if words == 0 {
            continue;
        }
        let reusable = slots.iter().position(|slot| {
            slot.ty == value.ty
                && slot
                    .members
                    .iter()
                    .all(|member| !interference[value.id.0 as usize].contains(member))
        });
        let slot = if let Some(slot) = reusable {
            slots[slot].members.push(value.id);
            slot
        } else {
            let slot = slots.len();
            slots.push(RootSlot {
                representative: value.id,
                words,
                offset: 0,
                members: vec![value.id],
                ty: value.ty.clone(),
            });
            slot
        };
        origin_slots[value.id.0 as usize] = Some(slot);
    }

    let mut words = 0u32;
    for slot in &mut slots {
        slot.offset = words;
        words = words
            .checked_add(slot.words)
            .ok_or_else(|| internal("LIR shadow value layout overflows u32"))?;
    }
    let value_slots = function
        .values
        .iter()
        .map(|value| {
            origin(function, value.id).map(|value_origin| origin_slots[value_origin.0 as usize])
        })
        .collect::<Result<Vec<_>, _>>()?;

    let live_out = function
        .blocks
        .iter()
        .map(|block| {
            successors(&block.terminator)
                .into_iter()
                .flat_map(|successor| live_in[successor.0 as usize].iter().copied())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut block_starts = vec![BTreeSet::new(); function.blocks.len()];
    let mut block_ends = vec![BTreeSet::new(); function.blocks.len()];
    let mut clear_after_instruction = function
        .blocks
        .iter()
        .map(|block| vec![Vec::new(); block.instructions.len()])
        .collect::<Vec<_>>();
    for block in &function.blocks {
        let block_index = block.id.0 as usize;
        let mut live = live_out[block_index].clone();
        record_terminator(function, &mut live, &block.terminator)?;
        block_ends[block_index] = live.clone();
        for (instruction_index, instruction) in block.instructions.iter().enumerate().rev() {
            let live_after = live.clone();
            let mut occupied_during = live_after.clone();
            if let Some(result) = instruction.result {
                let result = origin(function, result)?;
                occupied_during.insert(result);
                live.remove(&result);
            }
            for operand in &instruction.operands {
                record_operand(function, &mut live, operand)?;
            }
            for value in &instruction.invalidates {
                live.insert(origin(function, *value)?);
            }
            occupied_during.extend(live.iter().copied());
            let occupied_after = occupied_slots(&value_slots, live_after.iter().copied());
            clear_after_instruction[block_index][instruction_index] =
                occupied_slots(&value_slots, occupied_during)
                    .difference(&occupied_after)
                    .copied()
                    .collect();
        }
        block_starts[block_index] = live;
    }

    let mut predecessors = vec![Vec::<l::BlockId>::new(); function.blocks.len()];
    for block in &function.blocks {
        for successor in successors(&block.terminator) {
            predecessors[successor.0 as usize].push(block.id);
        }
    }
    let mut clear_at_block_entry = vec![Vec::new(); function.blocks.len()];
    for block in &function.blocks {
        let block_index = block.id.0 as usize;
        let mut candidates = predecessors[block_index]
            .iter()
            .flat_map(|predecessor| block_ends[predecessor.0 as usize].iter().copied())
            .collect::<BTreeSet<_>>();
        for parameter in &block.parameters {
            candidates.insert(origin(function, *parameter)?);
        }
        if block.id == function.entry {
            for parameter in &function.parameters {
                candidates.insert(origin(function, parameter.value)?);
            }
        }
        let occupied = occupied_slots(&value_slots, block_starts[block_index].iter().copied());
        clear_at_block_entry[block_index] = occupied_slots(&value_slots, candidates)
            .difference(&occupied)
            .copied()
            .collect();
    }

    Ok(RootStoragePlan {
        slots,
        value_slots,
        clear_at_block_entry,
        clear_after_instruction,
        words,
    })
}
