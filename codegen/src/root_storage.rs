//! Shared live-range storage plan for managed LIR values.

use std::collections::{BTreeSet, HashSet};

use subscript_compiler::{lir as l, Type};

use crate::layout::{managed_words, Layouts};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum StorageOwner {
    Value(l::ValueId),
    Local(l::LocalId),
}

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

fn extend_values(target: &mut BTreeSet<l::ValueId>, source: &BTreeSet<l::ValueId>) -> bool {
    let before = target.len();
    target.extend(source.iter().copied());
    target.len() != before
}

fn value_operand(operand: Option<&l::Operand>) -> Option<l::ValueId> {
    match operand {
        Some(l::Operand::Value(value)) => Some(*value),
        Some(l::Operand::Constant(_)) | None => None,
    }
}

fn can_carry_borrow(ty: &l::ValueType) -> bool {
    match ty {
        l::ValueType::Address(_) | l::ValueType::Iterator(_) => true,
        l::ValueType::Data(ty) => matches!(
            ty,
            Type::Class(_)
                | Type::FixedArray(_, _)
                | Type::Array(_)
                | Type::Nullable(_)
                | Type::Func(_)
                | Type::IterResult(_)
        ),
    }
}

fn address_owners(function: &l::Function) -> Vec<BTreeSet<StorageOwner>> {
    let mut owners = vec![BTreeSet::new(); function.values.len()];
    let mut local_owners = vec![BTreeSet::new(); function.locals.len()];
    for parameter in &function.parameters {
        if matches!(
            function.values[parameter.value.0 as usize].ty,
            l::ValueType::Address(_)
        ) {
            owners[parameter.value.0 as usize].insert(StorageOwner::Value(parameter.value));
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for block in &function.blocks {
            for instruction in &block.instructions {
                let first = value_operand(instruction.operands.first());
                let additions = match (instruction.result, &instruction.kind) {
                    (Some(result), l::InstructionKind::AllocateClass(_))
                    | (Some(result), l::InstructionKind::AddressOfValue) => {
                        BTreeSet::from([StorageOwner::Value(result)])
                    }
                    (Some(_), l::InstructionKind::AddressOfLocal(local)) => {
                        BTreeSet::from([StorageOwner::Local(*local)])
                    }
                    (Some(_), l::InstructionKind::LoadLocal(local)) => {
                        local_owners[local.0 as usize].clone()
                    }
                    (Some(_), l::InstructionKind::Copy) => {
                        first.map_or_else(BTreeSet::new, |base| owners[base.0 as usize].clone())
                    }
                    (
                        Some(_),
                        l::InstructionKind::AddressOfField(_)
                        | l::InstructionKind::AddressOfIndex { .. },
                    ) => first.map_or_else(BTreeSet::new, |base| {
                        if matches!(function.values[base.0 as usize].ty, l::ValueType::Data(_)) {
                            BTreeSet::from([StorageOwner::Value(base)])
                        } else {
                            owners[base.0 as usize].clone()
                        }
                    }),
                    _ => BTreeSet::new(),
                };
                if let Some(result) = instruction.result {
                    let before = owners[result.0 as usize].len();
                    owners[result.0 as usize].extend(additions);
                    changed |= owners[result.0 as usize].len() != before;
                }
                if let l::InstructionKind::StoreLocal(local) = instruction.kind {
                    let additions =
                        first.map_or_else(BTreeSet::new, |value| owners[value.0 as usize].clone());
                    let before = local_owners[local.0 as usize].len();
                    local_owners[local.0 as usize].extend(additions);
                    changed |= local_owners[local.0 as usize].len() != before;
                }
            }

            let mut extend_target = |target: &l::BlockTarget, skip_parameters: usize| {
                let parameters = function.blocks[target.block.0 as usize]
                    .parameters
                    .iter()
                    .skip(skip_parameters);
                for (argument, parameter) in target.arguments.iter().zip(parameters) {
                    let Some(argument) = value_operand(Some(argument)) else {
                        continue;
                    };
                    let additions = owners[argument.0 as usize].clone();
                    let before = owners[parameter.0 as usize].len();
                    owners[parameter.0 as usize].extend(additions);
                    changed |= owners[parameter.0 as usize].len() != before;
                }
            };
            match &block.terminator {
                l::Terminator::Branch(target) => extend_target(target, 0),
                l::Terminator::ConditionalBranch {
                    then_target,
                    else_target,
                    ..
                } => {
                    extend_target(then_target, 0);
                    extend_target(else_target, 0);
                }
                l::Terminator::Switch { arms, default, .. } => {
                    for arm in arms {
                        extend_target(&arm.target, 0);
                    }
                    extend_target(default, 0);
                }
                l::Terminator::Suspend {
                    successor,
                    resume_value,
                    arguments,
                    ..
                } => extend_target(
                    &l::BlockTarget {
                        block: *successor,
                        arguments: arguments.clone(),
                    },
                    usize::from(resume_value.is_some()),
                ),
                l::Terminator::Return { .. }
                | l::Terminator::Unreachable { .. }
                | l::Terminator::Trap(_) => {}
            }
        }
    }
    owners
}

fn extend_edge_dependencies(
    function: &l::Function,
    target: &l::BlockTarget,
    skip_parameters: usize,
    dependencies: &mut [BTreeSet<l::ValueId>],
) -> bool {
    let parameters = function.blocks[target.block.0 as usize]
        .parameters
        .iter()
        .skip(skip_parameters);
    let mut changed = false;
    for (argument, parameter) in target.arguments.iter().zip(parameters) {
        let Some(argument) = value_operand(Some(argument)) else {
            continue;
        };
        let source = dependencies[argument.0 as usize].clone();
        changed |= extend_values(&mut dependencies[parameter.0 as usize], &source);
    }
    changed
}

/// Computes the managed bases that each value borrows.
///
/// The fixed point follows addresses through aggregate storage and SSA
/// edges. Thus every live derived address retains each managed value whose
/// storage it can expose.
fn borrowed_bases(function: &l::Function) -> Vec<BTreeSet<l::ValueId>> {
    let mut dependencies = vec![BTreeSet::new(); function.values.len()];
    let has_address = function
        .values
        .iter()
        .any(|value| matches!(value.ty, l::ValueType::Address(_)));
    let has_value_pointer_coercion = function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            let (Some(result), Some(source)) = (
                instruction.result,
                value_operand(instruction.operands.first()),
            ) else {
                return false;
            };
            matches!(
                (
                    &instruction.kind,
                    &function.values[source.0 as usize].ty,
                    &function.values[result.0 as usize].ty,
                ),
                (
                    l::InstructionKind::Coerce,
                    l::ValueType::Data(Type::Class(source)),
                    l::ValueType::Data(Type::Nullable(target)),
                ) if matches!(target.as_ref(), Type::Class(target) if target == source)
            )
        })
    });
    if !has_address && !has_value_pointer_coercion {
        return dependencies;
    }

    let owners = address_owners(function);
    let mut local_dependencies = vec![BTreeSet::new(); function.locals.len()];

    for value in &function.values {
        if let l::ValueType::Address(address) = &value.ty {
            if let Some(base) = address.array_base {
                dependencies[value.id.0 as usize].insert(base);
            }
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for block in &function.blocks {
            for instruction in &block.instructions {
                let result = instruction.result;
                let first = value_operand(instruction.operands.first());
                match &instruction.kind {
                    l::InstructionKind::AddressOfValue => {
                        if let (Some(result), Some(source)) = (result, first) {
                            let source_dependencies = dependencies[source.0 as usize].clone();
                            changed |= dependencies[result.0 as usize].insert(source);
                            changed |= extend_values(
                                &mut dependencies[result.0 as usize],
                                &source_dependencies,
                            );
                        }
                    }
                    l::InstructionKind::AddressOfField(_)
                    | l::InstructionKind::AddressOfIndex { .. }
                    | l::InstructionKind::ForeignArrayData => {
                        if let (Some(result), Some(base)) = (result, first) {
                            let include_base = matches!(
                                function.values[base.0 as usize].ty,
                                l::ValueType::Data(_)
                            );
                            let source = dependencies[base.0 as usize].clone();
                            if include_base {
                                changed |= dependencies[result.0 as usize].insert(base);
                            }
                            changed |= extend_values(&mut dependencies[result.0 as usize], &source);
                        }
                    }
                    l::InstructionKind::LoadLocal(local) => {
                        if let Some(result) = result.filter(|result| {
                            can_carry_borrow(&function.values[result.0 as usize].ty)
                        }) {
                            let source = local_dependencies[local.0 as usize].clone();
                            changed |= extend_values(&mut dependencies[result.0 as usize], &source);
                        }
                    }
                    l::InstructionKind::AddressOfLocal(local) => {
                        if let Some(result) = result {
                            let source = local_dependencies[local.0 as usize].clone();
                            changed |= extend_values(&mut dependencies[result.0 as usize], &source);
                        }
                    }
                    l::InstructionKind::StoreLocal(local) => {
                        if let Some(value) = first {
                            let source = dependencies[value.0 as usize].clone();
                            changed |=
                                extend_values(&mut local_dependencies[local.0 as usize], &source);
                        }
                    }
                    l::InstructionKind::StoreAddress => {
                        let address = first;
                        let value = value_operand(instruction.operands.get(1));
                        if let (Some(address), Some(value)) = (address, value) {
                            let source = dependencies[value.0 as usize].clone();
                            for owner in owners[address.0 as usize].iter().copied() {
                                match owner {
                                    StorageOwner::Value(owner) => {
                                        changed |= extend_values(
                                            &mut dependencies[owner.0 as usize],
                                            &source,
                                        );
                                    }
                                    StorageOwner::Local(owner) => {
                                        changed |= extend_values(
                                            &mut local_dependencies[owner.0 as usize],
                                            &source,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    l::InstructionKind::Copy
                    | l::InstructionKind::Coerce
                    | l::InstructionKind::LoadAddress
                    | l::InstructionKind::LoadField(_)
                    | l::InstructionKind::ArrayLiteral
                    | l::InstructionKind::ArraySpreadLiteral(_)
                    | l::InstructionKind::MakeClosure(_)
                    | l::InstructionKind::Call(_)
                    | l::InstructionKind::IteratorCreate(_)
                    | l::InstructionKind::IteratorValue
                    | l::InstructionKind::IteratorAdvance => {
                        if let Some(result) = result {
                            for operand in &instruction.operands {
                                if let Some(source) = value_operand(Some(operand)) {
                                    let source = dependencies[source.0 as usize].clone();
                                    changed |= extend_values(
                                        &mut dependencies[result.0 as usize],
                                        &source,
                                    );
                                }
                            }
                            if matches!(instruction.kind, l::InstructionKind::Coerce) {
                                if let Some(source) = first {
                                    let source_type = &function.values[source.0 as usize].ty;
                                    let result_type = &function.values[result.0 as usize].ty;
                                    let borrows_source = matches!(
                                        (source_type, result_type),
                                        (
                                            l::ValueType::Data(Type::Class(source)),
                                            l::ValueType::Data(Type::Nullable(target))
                                        ) if matches!(target.as_ref(), Type::Class(target) if target == source)
                                    );
                                    if borrows_source
                                        && matches!(source_type, l::ValueType::Data(_))
                                    {
                                        changed |= dependencies[result.0 as usize].insert(source);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            changed |= match &block.terminator {
                l::Terminator::Branch(target) => {
                    extend_edge_dependencies(function, target, 0, &mut dependencies)
                }
                l::Terminator::ConditionalBranch {
                    then_target,
                    else_target,
                    ..
                } => {
                    extend_edge_dependencies(function, then_target, 0, &mut dependencies)
                        | extend_edge_dependencies(function, else_target, 0, &mut dependencies)
                }
                l::Terminator::Switch { arms, default, .. } => {
                    let mut edge_changed =
                        extend_edge_dependencies(function, default, 0, &mut dependencies);
                    for arm in arms {
                        edge_changed |=
                            extend_edge_dependencies(function, &arm.target, 0, &mut dependencies);
                    }
                    edge_changed
                }
                l::Terminator::Suspend {
                    successor,
                    resume_value,
                    arguments,
                    ..
                } => {
                    let target = l::BlockTarget {
                        block: *successor,
                        arguments: arguments.clone(),
                    };
                    extend_edge_dependencies(
                        function,
                        &target,
                        usize::from(resume_value.is_some()),
                        &mut dependencies,
                    )
                }
                l::Terminator::Return { .. }
                | l::Terminator::Unreachable { .. }
                | l::Terminator::Trap(_) => false,
            };
        }
    }
    dependencies
}

fn record_value(
    function: &l::Function,
    dependencies: &[BTreeSet<l::ValueId>],
    values: &mut BTreeSet<l::ValueId>,
    value: l::ValueId,
) -> Result<(), String> {
    for dependency in &dependencies[value.0 as usize] {
        values.insert(origin(function, *dependency)?);
    }
    values.insert(origin(function, value)?);
    Ok(())
}

fn record_operand(
    function: &l::Function,
    dependencies: &[BTreeSet<l::ValueId>],
    values: &mut BTreeSet<l::ValueId>,
    operand: &l::Operand,
) -> Result<(), String> {
    if let l::Operand::Value(value) = operand {
        record_value(function, dependencies, values, *value)?;
    }
    Ok(())
}

fn record_target(
    function: &l::Function,
    dependencies: &[BTreeSet<l::ValueId>],
    values: &mut BTreeSet<l::ValueId>,
    target: &l::BlockTarget,
) -> Result<(), String> {
    for argument in &target.arguments {
        record_operand(function, dependencies, values, argument)?;
    }
    Ok(())
}

fn record_terminator(
    function: &l::Function,
    dependencies: &[BTreeSet<l::ValueId>],
    values: &mut BTreeSet<l::ValueId>,
    terminator: &l::Terminator,
) -> Result<(), String> {
    match terminator {
        l::Terminator::Branch(target) => record_target(function, dependencies, values, target)?,
        l::Terminator::ConditionalBranch {
            condition,
            then_target,
            else_target,
        } => {
            record_operand(function, dependencies, values, condition)?;
            record_target(function, dependencies, values, then_target)?;
            record_target(function, dependencies, values, else_target)?;
        }
        l::Terminator::Switch {
            value,
            arms,
            default,
        } => {
            record_operand(function, dependencies, values, value)?;
            for arm in arms {
                record_target(function, dependencies, values, &arm.target)?;
            }
            record_target(function, dependencies, values, default)?;
        }
        l::Terminator::Return { value, .. } => {
            if let Some(value) = value {
                record_operand(function, dependencies, values, value)?;
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
                        record_value(function, dependencies, values, *value)?;
                    }
                }
                l::SuspendKind::Async => {}
                l::SuspendKind::AsyncCall { operands, .. } => {
                    for value in operands {
                        record_value(function, dependencies, values, *value)?;
                    }
                }
                l::SuspendKind::AsyncHandle { handle } => {
                    record_value(function, dependencies, values, *handle)?;
                }
            }
            for argument in arguments {
                record_operand(function, dependencies, values, argument)?;
            }
            for value in invalidates {
                record_value(function, dependencies, values, *value)?;
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
    dependencies: &[BTreeSet<l::ValueId>],
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
                .try_fold(BTreeSet::new(), |mut expanded, value| {
                    record_value(function, dependencies, &mut expanded, *value)?;
                    Ok(expanded)
                })
        })
        .collect()
}

pub(crate) fn value_interference(
    function: &l::Function,
) -> Result<Vec<HashSet<l::ValueId>>, String> {
    let dependencies = borrowed_bases(function);
    value_interference_with(function, &dependencies)
}

fn value_interference_with(
    function: &l::Function,
    dependencies: &[BTreeSet<l::ValueId>],
) -> Result<Vec<HashSet<l::ValueId>>, String> {
    let live_in = live_ins(function, dependencies)?;
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
        record_terminator(function, dependencies, &mut live, &block.terminator)?;
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
                record_operand(function, dependencies, &mut live, operand)?;
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
    let dependencies = borrowed_bases(function);
    let live_in = live_ins(function, &dependencies)?;
    let interference = value_interference_with(function, &dependencies)?;
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
        record_terminator(function, &dependencies, &mut live, &block.terminator)?;
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
                record_operand(function, &dependencies, &mut live, operand)?;
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

    #[test]
    fn borrowed_bases_cover_array_provenance_and_derived_addresses() {
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
            value(2, l::ValueType::Data(Type::Class(ClassId(0)))),
            value(
                3,
                l::ValueType::Address(l::AddressType {
                    pointee: Type::Class(ClassId(0)),
                    array_base: None,
                }),
            ),
            value(
                4,
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
                    l::InstructionKind::AddressOfIndex { checked: true },
                    vec![
                        l::Operand::Value(l::ValueId(0)),
                        l::Operand::Constant(l::Constant {
                            ty: Type::I32,
                            kind: l::ConstantKind::Integer(0),
                        }),
                    ],
                ),
                instruction(
                    Some(3),
                    l::InstructionKind::AddressOfValue,
                    vec![l::Operand::Value(l::ValueId(2))],
                ),
                instruction(
                    Some(4),
                    l::InstructionKind::AddressOfField(l::FieldRef::Class(l::FieldId(0))),
                    vec![l::Operand::Value(l::ValueId(3))],
                ),
            ],
        );
        let dependencies = borrowed_bases(&function(values, vec![block]));
        assert_eq!(dependencies[1], BTreeSet::from([l::ValueId(0)]));
        assert_eq!(dependencies[3], BTreeSet::from([l::ValueId(2)]));
        assert_eq!(dependencies[4], BTreeSet::from([l::ValueId(2)]));
    }

    #[test]
    fn borrowed_bases_follow_aggregate_stores_and_ssa_edges() {
        let inner = Type::Class(ClassId(0));
        let outer = Type::Class(ClassId(1));
        let nullable_inner = Type::Nullable(Box::new(inner.clone()));
        let address = |pointee| {
            l::ValueType::Address(l::AddressType {
                pointee,
                array_base: None,
            })
        };
        let values = vec![
            value(0, l::ValueType::Data(inner.clone())),
            value(1, l::ValueType::Data(nullable_inner.clone())),
            value(2, address(outer.clone())),
            value(3, address(nullable_inner.clone())),
            value(4, l::ValueType::Data(outer.clone())),
            value(5, address(inner.clone())),
            value(6, address(inner)),
            value(7, address(outer.clone())),
            value(8, address(outer)),
            value(9, address(nullable_inner)),
        ];
        let entry = l::BasicBlock {
            id: l::BlockId(0),
            source_name: None,
            parameters: Vec::new(),
            instructions: vec![
                instruction(
                    Some(1),
                    l::InstructionKind::Coerce,
                    vec![l::Operand::Value(l::ValueId(0))],
                ),
                instruction(
                    Some(2),
                    l::InstructionKind::AllocateClass(ClassId(1)),
                    Vec::new(),
                ),
                instruction(
                    Some(3),
                    l::InstructionKind::AddressOfField(l::FieldRef::Class(l::FieldId(0))),
                    vec![l::Operand::Value(l::ValueId(2))],
                ),
                instruction(
                    None,
                    l::InstructionKind::StoreAddress,
                    vec![
                        l::Operand::Value(l::ValueId(3)),
                        l::Operand::Value(l::ValueId(1)),
                    ],
                ),
                instruction(
                    Some(4),
                    l::InstructionKind::LoadAddress,
                    vec![l::Operand::Value(l::ValueId(2))],
                ),
                instruction(
                    None,
                    l::InstructionKind::StoreLocal(l::LocalId(0)),
                    vec![l::Operand::Value(l::ValueId(4))],
                ),
                instruction(
                    Some(7),
                    l::InstructionKind::AddressOfLocal(l::LocalId(0)),
                    Vec::new(),
                ),
                instruction(
                    Some(9),
                    l::InstructionKind::AddressOfField(l::FieldRef::Class(l::FieldId(0))),
                    vec![l::Operand::Value(l::ValueId(8))],
                ),
                instruction(
                    None,
                    l::InstructionKind::StoreAddress,
                    vec![
                        l::Operand::Value(l::ValueId(9)),
                        l::Operand::Value(l::ValueId(1)),
                    ],
                ),
                instruction(
                    Some(5),
                    l::InstructionKind::AddressOfValue,
                    vec![l::Operand::Value(l::ValueId(0))],
                ),
            ],
            terminator: l::Terminator::Branch(l::BlockTarget {
                block: l::BlockId(1),
                arguments: vec![l::Operand::Value(l::ValueId(5))],
            }),
        };
        let mut successor = return_block(1, Vec::new());
        successor.parameters.push(l::ValueId(6));
        let mut function = function(values, vec![entry, successor]);
        function.parameters.push(l::Parameter {
            storage: None,
            value: l::ValueId(8),
            source_name: "receiver".into(),
            kind: l::ParameterKind::Receiver,
            pos: pos(),
        });
        function.locals.push(l::Local {
            id: l::LocalId(0),
            source_name: "outer".into(),
            ty: l::ValueType::Data(Type::Class(ClassId(1))),
            mutable: true,
            pos: pos(),
        });
        function.liveness.live_ins[1] = vec![l::ValueId(7), l::ValueId(9)];
        let dependencies = borrowed_bases(&function);
        for dependent in [1, 2, 3, 4, 5, 6, 7, 8, 9] {
            assert!(dependencies[dependent].contains(&l::ValueId(0)));
        }
        assert!(live_ins(&function, &dependencies).unwrap()[1].contains(&l::ValueId(0)));
    }
}
