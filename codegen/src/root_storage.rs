//! Shared live-range storage plan for managed LIR values.

use std::collections::{BTreeSet, HashSet};

use subscript_compiler::lir as l;
#[cfg(test)]
use subscript_compiler::Type;

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

fn value_operand(operand: Option<&l::Operand>) -> Option<l::ValueId> {
    match operand {
        Some(l::Operand::Value(value)) => Some(*value),
        Some(l::Operand::Constant(_)) | None => None,
    }
}

/// Returns the managed value whose address this instruction takes.
///
/// This is the only decision point for rule 8b. Array-element addresses use
/// the direct `array_base` provenance on their LIR value instead.
fn address_taken_value(instruction: &l::Instruction) -> Result<Option<l::ValueId>, String> {
    let Some(source) = value_operand(instruction.operands.first()) else {
        return Ok(None);
    };
    match &instruction.kind {
        l::InstructionKind::AddressOfValue => Ok(Some(source)),
        _ => Ok(None),
    }
}

fn address_taken_values(function: &l::Function) -> Result<BTreeSet<l::ValueId>, String> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .try_fold(BTreeSet::new(), |mut values, instruction| {
            if let Some(value) = address_taken_value(instruction)? {
                values.insert(origin(function, value)?);
            }
            Ok(values)
        })
}

fn record_value(
    function: &l::Function,
    values: &mut BTreeSet<l::ValueId>,
    value: l::ValueId,
) -> Result<(), String> {
    if let l::ValueType::Address(address) = &function.values[value.0 as usize].ty {
        if let Some(array_base) = address.array_base {
            values.insert(origin(function, array_base)?);
        }
    }
    values.insert(origin(function, value)?);
    Ok(())
}

fn record_operand(
    function: &l::Function,
    values: &mut BTreeSet<l::ValueId>,
    operand: &l::Operand,
) -> Result<(), String> {
    if let l::Operand::Value(value) = operand {
        record_value(function, values, *value)?;
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
                        record_value(function, values, *value)?;
                    }
                }
                l::SuspendKind::Async => {}
                l::SuspendKind::AsyncCall { operands, .. } => {
                    for value in operands {
                        record_value(function, values, *value)?;
                    }
                }
                l::SuspendKind::AsyncHandle { handle } => {
                    record_value(function, values, *handle)?;
                }
            }
            for argument in arguments {
                record_operand(function, values, argument)?;
            }
            for value in invalidates {
                record_value(function, values, *value)?;
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

fn live_ins(
    function: &l::Function,
    held_to_exit: &BTreeSet<l::ValueId>,
) -> Result<Vec<BTreeSet<l::ValueId>>, String> {
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
                .try_fold(held_to_exit.clone(), |mut live, value| {
                    record_value(function, &mut live, *value)?;
                    Ok(live)
                })
        })
        .collect()
}

pub(crate) fn value_interference(
    function: &l::Function,
) -> Result<Vec<HashSet<l::ValueId>>, String> {
    let held_to_exit = address_taken_values(function)?;
    let origin_interference = value_interference_with(function, &held_to_exit)?;
    let mut origin_members = vec![Vec::new(); function.values.len()];
    for value in &function.values {
        origin_members[origin(function, value.id)?.0 as usize].push(value.id);
    }
    let mut interference = vec![HashSet::new(); function.values.len()];
    for (left_origin, right_origins) in origin_interference.iter().enumerate() {
        for right_origin in right_origins {
            for left in &origin_members[left_origin] {
                for right in &origin_members[right_origin.0 as usize] {
                    add_interference(&mut interference, *left, *right);
                }
            }
        }
    }
    Ok(interference)
}

fn value_interference_with(
    function: &l::Function,
    held_to_exit: &BTreeSet<l::ValueId>,
) -> Result<Vec<HashSet<l::ValueId>>, String> {
    let live_in = live_ins(function, held_to_exit)?;
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
        live.extend(held_to_exit.iter().copied());
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
    let held_to_exit = address_taken_values(function)?;
    let live_in = live_ins(function, &held_to_exit)?;
    let interference = value_interference_with(function, &held_to_exit)?;
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
        live.extend(held_to_exit.iter().copied());
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

#[cfg(test)]
mod tests {
    use subscript_compiler::{ClassId, Pos};

    use super::*;

    fn pos() -> Pos {
        Pos::new("root-storage.ts", 1, 1)
    }

    fn value(id: u32, ty: l::ValueType) -> l::Value {
        l::Value {
            id: l::ValueId(id),
            ty,
            source_name: None,
        }
    }

    fn instruction(
        result: Option<u32>,
        kind: l::InstructionKind,
        operands: Vec<l::Operand>,
    ) -> l::Instruction {
        l::Instruction {
            result: result.map(l::ValueId),
            kind,
            operands,
            invalidates: Vec::new(),
            traps: Vec::new(),
            pos: pos(),
        }
    }

    fn function(values: Vec<l::Value>, blocks: Vec<l::BasicBlock>) -> l::Function {
        l::Function {
            id: l::FunctionId(0),
            source_name: "rootStorage".into(),
            kind: l::FunctionKind::Free,
            exported: false,
            is_generator: false,
            is_async: false,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            parameters: Vec::new(),
            return_type: Type::Void,
            locals: Vec::new(),
            liveness: l::Liveness {
                live_ins: vec![Vec::new(); blocks.len()],
                value_origins: values.iter().map(|value| value.id).collect(),
            },
            values,
            blocks,
            entry: l::BlockId(0),
            pos: pos(),
        }
    }

    fn return_block(id: u32, instructions: Vec<l::Instruction>) -> l::BasicBlock {
        l::BasicBlock {
            id: l::BlockId(id),
            source_name: None,
            parameters: Vec::new(),
            instructions,
            terminator: l::Terminator::Return {
                value: None,
                pos: pos(),
            },
        }
    }

    fn layouts() -> Layouts {
        Layouts::build_lir(&l::Module {
            entry: None,
            async_roots: Vec::new(),
            classes: Vec::new(),
            enums: Vec::new(),
            string_aliases: Vec::new(),
            globals: Vec::new(),
            foreign_functions: Vec::new(),
            functions: Vec::new(),
            worker_entries: Vec::new(),
            intrinsic_operations: Vec::new(),
            initializer: None,
        })
        .expect("empty layouts")
    }

    #[test]
    fn record_value_uses_direct_array_provenance() {
        let array = Type::Array(Box::new(Type::I32));
        let values = vec![
            value(0, l::ValueType::Data(array)),
            value(
                1,
                l::ValueType::Address(l::AddressType {
                    pointee: Type::I32,
                    array_base: Some(l::ValueId(0)),
                }),
            ),
        ];
        let block = return_block(
            0,
            vec![instruction(
                Some(1),
                l::InstructionKind::AddressOfIndex { checked: true },
                vec![
                    l::Operand::Value(l::ValueId(0)),
                    l::Operand::Constant(l::Constant {
                        ty: Type::I32,
                        kind: l::ConstantKind::Integer(0),
                    }),
                ],
            )],
        );
        let function = function(values, vec![block]);
        let mut live = BTreeSet::new();
        record_value(&function, &mut live, l::ValueId(1)).unwrap();
        assert_eq!(live, BTreeSet::from([l::ValueId(0), l::ValueId(1)]));
    }

    #[test]
    fn address_taken_values_keep_a_base_for_derived_addresses() {
        let class = Type::Class(ClassId(0));
        let values = vec![
            value(0, l::ValueType::Data(class.clone())),
            value(
                1,
                l::ValueType::Address(l::AddressType {
                    pointee: class,
                    array_base: None,
                }),
            ),
            value(
                2,
                l::ValueType::Address(l::AddressType {
                    pointee: Type::I32,
                    array_base: None,
                }),
            ),
        ];
        let block = return_block(
            0,
            vec![
                instruction(
                    Some(1),
                    l::InstructionKind::AddressOfValue,
                    vec![l::Operand::Value(l::ValueId(0))],
                ),
                instruction(
                    Some(2),
                    l::InstructionKind::AddressOfField(l::FieldRef::Class(l::FieldId(0))),
                    vec![l::Operand::Value(l::ValueId(1))],
                ),
            ],
        );
        assert_eq!(
            address_taken_values(&function(values, vec![block])).unwrap(),
            BTreeSet::from([l::ValueId(0)])
        );
    }

    #[test]
    fn address_taken_slot_survives_a_call_and_global_store_until_exit() {
        let string = l::ValueType::Data(Type::Str);
        let address = l::ValueType::Address(l::AddressType {
            pointee: Type::Str,
            array_base: None,
        });
        let values = vec![
            value(0, string.clone()),
            value(1, address.clone()),
            value(2, string.clone()),
            value(3, string.clone()),
        ];
        let block = return_block(
            0,
            vec![
                instruction(
                    Some(0),
                    l::InstructionKind::StringLiteral("base".into()),
                    Vec::new(),
                ),
                instruction(
                    Some(1),
                    l::InstructionKind::AddressOfValue,
                    vec![l::Operand::Value(l::ValueId(0))],
                ),
                instruction(
                    Some(2),
                    l::InstructionKind::Call(l::CallTarget {
                        kind: l::CallTargetKind::Function(l::FunctionId(1)),
                        parameter_types: vec![address],
                        return_type: Some(string.clone()),
                    }),
                    vec![l::Operand::Value(l::ValueId(1))],
                ),
                instruction(
                    None,
                    l::InstructionKind::StoreGlobal(l::GlobalId(0)),
                    vec![l::Operand::Value(l::ValueId(2))],
                ),
                instruction(
                    Some(3),
                    l::InstructionKind::StringLiteral("later".into()),
                    Vec::new(),
                ),
            ],
        );
        let plan = plan(&function(values, vec![block]), &layouts()).unwrap();
        let base_slot = plan.value_slots[0].expect("the managed base has a root slot");
        assert_ne!(plan.value_slots[3], Some(base_slot));
        assert!(plan.clear_after_instruction[0][1..]
            .iter()
            .all(|clears| !clears.contains(&base_slot)));
    }
}
