//! Independent frame-owner balances for generator cleanup descriptions (§176).

use super::verify::{counted_instruction_stores, counted_terminator_stores, finding};
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Value(l::ValueId),
    Local(l::LocalId),
}

type Balance = BTreeMap<Key, i64>;

struct Origins(BTreeMap<l::ValueId, Key>);

impl Origins {
    fn new(function: &l::Function) -> Self {
        let locals = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| match instruction.kind {
                l::InstructionKind::LoadLocal(local) => {
                    instruction.result.map(|value| (value, local))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        Self(
            function
                .values
                .iter()
                .map(|value| {
                    let origin = function
                        .liveness
                        .value_origins
                        .get(value.id.0 as usize)
                        .copied()
                        .unwrap_or(value.id);
                    (
                        value.id,
                        locals
                            .get(&origin)
                            .copied()
                            .map_or(Key::Value(origin), Key::Local),
                    )
                })
                .collect(),
        )
    }

    fn key(&self, value: l::ValueId) -> Key {
        self.0.get(&value).copied().unwrap_or(Key::Value(value))
    }
}

fn change(balance: &mut Balance, key: Key, amount: i64) {
    let value = balance.entry(key).or_default();
    *value += amount;
    if *value == 0 {
        balance.remove(&key);
    }
}

fn operand_key(origins: &Origins, operand: &l::Operand) -> Option<Key> {
    match operand {
        l::Operand::Value(value) => Some(origins.key(*value)),
        _ => None,
    }
}

fn compare(
    function: &l::Function,
    origins: &Origins,
    state: Option<l::BlockId>,
    balance: &Balance,
    errors: &mut Vec<VerifyError>,
) {
    let descriptions = function
        .liveness
        .generator_cleanup
        .iter()
        .filter(|description| description.suspension == state)
        .collect::<Vec<_>>();
    if descriptions.len() != 1 {
        errors.push(finding(
            function,
            format!("generator state {state:?} needs one cleanup description"),
        ));
        return;
    }
    let mut described = Balance::new();
    for value in &descriptions[0].owners {
        change(&mut described, origins.key(*value), 1);
    }
    if described != *balance {
        errors.push(finding(function, format!("generator state {state:?} cleanup count differs: balance {balance:?}, description {described:?}")));
    }
}

pub(super) fn verify(module: &l::Module, function: &l::Function, errors: &mut Vec<VerifyError>) {
    if !function.is_generator {
        return;
    }
    let origins = Origins::new(function);
    let mut start = Balance::new();
    for parameter in &function.parameters {
        if parameter.kind != l::ParameterKind::Capture
            && is_async_owner_type(&function.values[parameter.value.0 as usize].ty)
        {
            change(&mut start, origins.key(parameter.value), 1);
        }
    }
    // Start owners use the builder parameter rule, so a comparison adds no independent evidence.
    for description in &function.liveness.generator_cleanup {
        if let Some(id) = description.suspension {
            if !function
                .blocks
                .get(id.0 as usize)
                .is_some_and(|block| matches!(block.terminator, l::Terminator::Suspend { .. }))
            {
                errors.push(finding(
                    function,
                    format!("cleanup description block {} has no suspension", id.0),
                ));
            }
        }
    }
    let mut flags = BTreeMap::new();
    for block in &function.blocks {
        for target in block.terminator.targets() {
            if target.ownership.len() != target.arguments.len() {
                errors.push(finding(
                    function,
                    format!(
                        "block {} edge to {} has a missing ownership flag",
                        block.id.0, target.block.0
                    ),
                ));
                continue;
            }
            let Some(destination) = function.blocks.get(target.block.0 as usize) else {
                continue;
            };
            let skip = usize::from(matches!(
                block.terminator,
                l::Terminator::Suspend {
                    resume_value: Some(_),
                    ..
                }
            ));
            for (parameter, owned) in destination
                .parameters
                .iter()
                .skip(skip)
                .zip(&target.ownership)
            {
                if flags
                    .insert(*parameter, *owned)
                    .is_some_and(|previous| previous != *owned)
                {
                    errors.push(finding(
                        function,
                        format!("block parameter {} has mixed ownership flags", parameter.0),
                    ));
                }
            }
        }
    }
    let mut incoming = vec![None; function.blocks.len()];
    incoming[function.entry.0 as usize] = Some(start);
    let mut queue = VecDeque::from([function.entry]);
    let mut visited = HashSet::new();
    while let Some(id) = queue.pop_front() {
        if !visited.insert(id) {
            continue;
        }
        let Some(block) = function.blocks.get(id.0 as usize) else {
            continue;
        };
        let Some(mut balance) = incoming[id.0 as usize].clone() else {
            continue;
        };
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let Some(value) = instruction.result {
                if function.values[value.0 as usize].fresh_owner
                    && is_async_owner_type(&function.values[value.0 as usize].ty)
                {
                    change(&mut balance, origins.key(value), 1);
                }
            }
            match instruction.kind {
                l::InstructionKind::AsyncHandleRetain
                | l::InstructionKind::AsyncHandleArrayRetain
                    if instruction.result.is_none() =>
                {
                    if let Some(value) = instruction
                        .operands
                        .first()
                        .and_then(|v| operand_key(&origins, v))
                    {
                        change(&mut balance, value, 1);
                    }
                }
                l::InstructionKind::AsyncHandleRelease
                | l::InstructionKind::AsyncHandleArrayRelease => {
                    if let Some(value) = instruction
                        .operands
                        .first()
                        .and_then(|v| operand_key(&origins, v))
                    {
                        if balance.contains_key(&value)
                            || !displaced(block, index, &instruction.operands[0])
                        {
                            change(&mut balance, value, -1);
                        }
                    }
                }
                _ => {}
            }
            for (_, operand) in counted_instruction_stores(module, function, instruction) {
                if let Some(value) = operand_key(&origins, operand) {
                    change(&mut balance, value, -1);
                }
                if let l::InstructionKind::StoreLocal(local) = instruction.kind {
                    change(&mut balance, Key::Local(local), 1);
                }
            }
            negative(function, block.id, &balance, errors);
            if let Some(handler) = instruction.handler() {
                propagate(
                    function,
                    handler,
                    balance.clone(),
                    &mut incoming,
                    &mut queue,
                    errors,
                );
            }
        }
        for (_, operand) in counted_terminator_stores(function, &block.terminator) {
            if let Some(value) = operand_key(&origins, &operand) {
                change(&mut balance, value, -1);
            }
        }
        if matches!(block.terminator, l::Terminator::Suspend { .. }) {
            compare(function, &origins, Some(id), &balance, errors);
        }
        for target in block.terminator.targets() {
            let Some(destination) = function.blocks.get(target.block.0 as usize) else {
                continue;
            };
            let mut routed = balance.clone();
            let skip = usize::from(matches!(
                block.terminator,
                l::Terminator::Suspend {
                    resume_value: Some(_),
                    ..
                }
            ));
            for (argument, owned) in target.arguments.iter().zip(&target.ownership) {
                if *owned {
                    if let Some(value) = operand_key(&origins, argument) {
                        change(&mut routed, value, -1);
                    }
                }
            }
            negative(function, id, &routed, errors);
            for (parameter, owned) in destination
                .parameters
                .iter()
                .skip(skip)
                .zip(&target.ownership)
            {
                if *owned {
                    change(&mut routed, origins.key(*parameter), 1);
                }
            }
            propagate(
                function,
                target.block,
                routed,
                &mut incoming,
                &mut queue,
                errors,
            );
        }
    }
    for block in &function.blocks {
        if matches!(block.terminator, l::Terminator::Suspend { .. }) && !visited.contains(&block.id)
        {
            errors.push(finding(
                function,
                format!(
                    "suspension block {} is unreachable by the count walk",
                    block.id.0
                ),
            ));
        }
    }
}

fn negative(
    function: &l::Function,
    block: l::BlockId,
    balance: &Balance,
    errors: &mut Vec<VerifyError>,
) {
    if balance.values().any(|count| *count < 0) {
        errors.push(finding(
            function,
            format!(
                "block {} has a count balance below zero: {balance:?}",
                block.0
            ),
        ));
    }
}

fn propagate(
    function: &l::Function,
    block: l::BlockId,
    balance: Balance,
    incoming: &mut [Option<Balance>],
    queue: &mut VecDeque<l::BlockId>,
    errors: &mut Vec<VerifyError>,
) {
    let Some(slot) = incoming.get_mut(block.0 as usize) else {
        return;
    };
    if let Some(previous) = slot {
        if previous != &balance {
            errors.push(finding(
                function,
                format!(
                    "block {} has different incoming count balances: {previous:?}, {balance:?}",
                    block.0
                ),
            ));
        }
    } else {
        *slot = Some(balance);
        queue.push_back(block);
    }
}

// A replaced container slot owns this borrowed load. Its release changes no frame owner.
fn displaced(block: &l::BasicBlock, release: usize, operand: &l::Operand) -> bool {
    let l::Operand::Value(value) = operand else {
        return false;
    };
    let Some(load) = block.instructions[..release]
        .iter()
        .rposition(|instruction| {
            instruction.result == Some(*value)
                && matches!(
                    instruction.kind,
                    l::InstructionKind::LoadAddress | l::InstructionKind::LoadGlobal(_)
                )
        })
    else {
        return false;
    };
    let old = &block.instructions[load];
    let mut replaced = false;
    for instruction in &block.instructions[load + 1..release] {
        if !instruction.invalidates.is_empty()
            || matches!(instruction.kind, l::InstructionKind::Call(_))
        {
            return false;
        }
        let same_slot = match (&old.kind, &instruction.kind) {
            (l::InstructionKind::LoadAddress, l::InstructionKind::StoreAddress) => {
                old.operands.first() == instruction.operands.first()
            }
            (l::InstructionKind::LoadGlobal(a), l::InstructionKind::StoreGlobal(b)) => a == b,
            _ => false,
        };
        if same_slot {
            if replaced {
                return false;
            }
            replaced = true;
        }
    }
    replaced
}
