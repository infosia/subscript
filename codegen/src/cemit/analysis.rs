//! Per-function emission index, address folding, value coalescing, and declaration scopes.

use super::*;

impl<'f> EmissionIndex<'f> {
    pub(super) fn build(function: &'f l::Function) -> Result<Self, String> {
        Self::validate_ids(function)?;
        let mut index = Self {
            definitions: HashMap::new(),
            definition_blocks: vec![None; function.values.len()],
            use_blocks: vec![HashSet::new(); function.values.len()],
            reachable_use_rows: vec![None; function.values.len()],
            reachable_uses: Vec::new(),
            parameter_blocks: vec![None; function.values.len()],
            incoming_targets: vec![Vec::new(); function.blocks.len()],
            local_seeds: vec![LocalSeed::Unused; function.values.len()],
            local_addresses: Vec::new(),
            first_stores: HashMap::new(),
            iterator_seeds: Vec::new(),
            propagated_values: Vec::new(),
        };
        for block in &function.blocks {
            for parameter in &block.parameters {
                index.parameter_blocks[parameter.0 as usize] = Some(block.id);
            }
            for instruction in &block.instructions {
                if let Some(result) = instruction.result {
                    index.definitions.insert(result, instruction);
                    index.definition_blocks[result.0 as usize] = Some(block.id);
                }
                match instruction.kind {
                    l::InstructionKind::AddressOfLocal(local) => {
                        index.local_addresses.push((local, instruction.result));
                    }
                    l::InstructionKind::StoreLocal(local) => {
                        if let Some(l::Operand::Value(value)) = instruction.operands.first() {
                            index.first_stores.entry(local).or_insert(*value);
                        }
                    }
                    l::InstructionKind::IteratorCreate { bound, .. } => {
                        if let Some(result) = instruction.result {
                            index.iterator_seeds.push((result, bound));
                        }
                    }
                    l::InstructionKind::Copy | l::InstructionKind::IteratorAdvance => {
                        if let (Some(result), Some(l::Operand::Value(source))) =
                            (instruction.result, instruction.operands.first())
                        {
                            index.propagated_values.push((*source, result));
                        }
                    }
                    _ => {}
                }
                for (position, operand) in instruction.operands.iter().enumerate() {
                    if let l::Operand::Value(value) = operand {
                        index.use_blocks[value.0 as usize].insert(block.id);
                        let seed = &mut index.local_seeds[value.0 as usize];
                        *seed = match (&instruction.kind, position, *seed) {
                            (l::InstructionKind::StoreLocal(local), 0, LocalSeed::Unused) => {
                                LocalSeed::Local(*local)
                            }
                            (
                                l::InstructionKind::StoreLocal(local),
                                0,
                                LocalSeed::Local(previous),
                            ) if *local == previous => *seed,
                            _ => LocalSeed::Other,
                        };
                    }
                }
                for value in &instruction.invalidates {
                    index.use_blocks[value.0 as usize].insert(block.id);
                    index.local_seeds[value.0 as usize] = LocalSeed::Other;
                }
            }
            for value in block.terminator.value_uses() {
                index.use_blocks[value.0 as usize].insert(block.id);
                index.local_seeds[value.0 as usize] = LocalSeed::Other;
            }
            if let l::Terminator::Suspend { invalidates, .. } = &block.terminator {
                for value in invalidates {
                    index.use_blocks[value.0 as usize].insert(block.id);
                    index.local_seeds[value.0 as usize] = LocalSeed::Other;
                }
            } else {
                for target in block.terminator.targets() {
                    index.incoming_targets[target.block.0 as usize].push(target);
                }
            }
        }
        // Only unused edge parameters ask whether their source has a later use.
        let mut queried_pairs = Vec::new();
        for (block, targets) in index.incoming_targets.iter().enumerate() {
            for target in targets {
                for (argument, parameter) in target
                    .arguments
                    .iter()
                    .zip(&function.blocks[block].parameters)
                {
                    if index.use_blocks[parameter.0 as usize].is_empty() {
                        if let l::Operand::Value(source) = argument {
                            queried_pairs.push((*source, target.block));
                        }
                    }
                }
            }
        }
        queried_pairs.sort_unstable();
        queried_pairs.dedup();
        let mut queried_sources = Vec::new();
        for (source, _) in queried_pairs {
            let row = &mut index.reachable_use_rows[source.0 as usize];
            if row.is_none() {
                *row = Some(queried_sources.len());
                queried_sources.push(source);
            }
        }
        let block_count = function.blocks.len();
        index.reachable_uses = vec![false; queried_sources.len() * block_count];
        if queried_sources.is_empty() {
            return Ok(index);
        }
        let mut predecessors = vec![Vec::new(); block_count];
        for block in &function.blocks {
            for successor in terminator_successors(&block.terminator) {
                predecessors[successor.0 as usize].push(block.id);
            }
        }
        // Reverse reachability stops at the parameter that replaces this value.
        // Each queried source has one flat row, shared by all its target queries.
        let mut pending = Vec::new();
        for (row, source) in queried_sources.into_iter().enumerate() {
            let value = source.0 as usize;
            let offset = row * block_count;
            pending.extend(index.use_blocks[value].iter().copied());
            while let Some(block) = pending.pop() {
                if index.parameter_blocks[value] == Some(block)
                    || index.reachable_uses[offset + block.0 as usize]
                {
                    continue;
                }
                index.reachable_uses[offset + block.0 as usize] = true;
                pending.extend(predecessors[block.0 as usize].iter().copied());
            }
        }
        Ok(index)
    }

    /// Reject invalid ids before any dense table access.
    fn validate_ids(function: &l::Function) -> Result<(), String> {
        let value_id = |value: l::ValueId| {
            if value.0 as usize >= function.values.len() {
                Err(internal(format!("invalid emission value id {}", value.0)))
            } else {
                Ok(())
            }
        };
        let block_id = |block: l::BlockId| {
            if block.0 as usize >= function.blocks.len() {
                Err(internal(format!("invalid emission block id {}", block.0)))
            } else {
                Ok(())
            }
        };
        let local_id = |local: l::LocalId| {
            if local.0 as usize >= function.locals.len() {
                Err(internal(format!("invalid emission local id {}", local.0)))
            } else {
                Ok(())
            }
        };
        let value_type = |ty: &l::ValueType| {
            if let l::ValueType::Address(l::AddressType {
                array_base: Some(base),
                ..
            }) = ty
            {
                value_id(*base)?;
            }
            Ok::<_, String>(())
        };
        let call_target = |target: &l::CallTarget| {
            for ty in target.parameter_types.iter().chain(&target.return_type) {
                value_type(ty)?;
            }
            Ok::<_, String>(())
        };
        block_id(function.entry)?;
        for value in &function.values {
            value_id(value.id)?;
            value_type(&value.ty)?;
        }
        for local in &function.locals {
            local_id(local.id)?;
            value_type(&local.ty)?;
        }
        for parameter in &function.parameters {
            value_id(parameter.value)?;
            if let Some(storage) = parameter.storage {
                local_id(storage)?;
            }
        }
        for value in function
            .liveness
            .live_ins
            .iter()
            .flatten()
            .chain(&function.liveness.value_origins)
        {
            value_id(*value)?;
        }
        for block in &function.blocks {
            block_id(block.id)?;
            for parameter in &block.parameters {
                value_id(*parameter)?;
            }
            for instruction in &block.instructions {
                if let l::InstructionKind::StoreLocal(local)
                | l::InstructionKind::LoadLocal(local)
                | l::InstructionKind::AddressOfLocal(local) = instruction.kind
                {
                    local_id(local)?;
                }
                for value in instruction.result.iter().chain(&instruction.invalidates) {
                    value_id(*value)?;
                }
                for operand in &instruction.operands {
                    if let l::Operand::Value(value) = operand {
                        value_id(*value)?;
                    }
                }
                if let l::InstructionKind::Call(target)
                | l::InstructionKind::AsyncHandleCreate(target) = &instruction.kind
                {
                    call_target(target)?;
                }
            }
            for value in block.terminator.value_uses() {
                value_id(value)?;
            }
            for successor in terminator_successors(&block.terminator) {
                block_id(successor)?;
            }
            if let l::Terminator::Suspend {
                kind,
                resume_value,
                invalidates,
                ..
            } = &block.terminator
            {
                for value in resume_value.iter().chain(invalidates) {
                    value_id(*value)?;
                }
                if let l::SuspendKind::AsyncCall { target, .. } = kind {
                    call_target(target)?;
                }
            }
        }
        Ok(())
    }
}

fn has_local_address_origin(
    value: l::ValueId,
    definitions: &HashMap<l::ValueId, &l::Instruction>,
    memo: &mut HashMap<l::ValueId, bool>,
    visiting: &mut HashSet<l::ValueId>,
) -> bool {
    if let Some(result) = memo.get(&value) {
        return *result;
    }
    if !visiting.insert(value) {
        return false;
    }
    let result = definitions.get(&value).is_some_and(|instruction| {
        if matches!(instruction.kind, l::InstructionKind::AddressOfLocal(_)) {
            return true;
        }
        if !matches!(
            instruction.kind,
            l::InstructionKind::AddressOfField(_) | l::InstructionKind::AddressOfIndex { .. }
        ) {
            return false;
        }
        let Some(l::Operand::Value(base)) = instruction.operands.first() else {
            return false;
        };
        has_local_address_origin(*base, definitions, memo, visiting)
    });
    visiting.remove(&value);
    memo.insert(value, result);
    result
}

fn record_address_escape(uses: &mut HashMap<l::ValueId, Vec<AddressUse>>, operand: &l::Operand) {
    if let l::Operand::Value(value) = operand {
        if let Some(uses) = uses.get_mut(value) {
            uses.push(AddressUse::Escape);
        }
    }
}

fn record_terminator_address_escapes(
    uses: &mut HashMap<l::ValueId, Vec<AddressUse>>,
    terminator: &l::Terminator,
) {
    for value in terminator.value_uses() {
        record_address_escape(uses, &l::Operand::Value(value));
    }
}

fn address_has_only_terminal_consumers(
    value: l::ValueId,
    uses: &HashMap<l::ValueId, Vec<AddressUse>>,
    memo: &mut HashMap<l::ValueId, bool>,
    visiting: &mut HashSet<l::ValueId>,
) -> bool {
    if let Some(result) = memo.get(&value) {
        return *result;
    }
    if !visiting.insert(value) {
        return false;
    }
    let result = uses.get(&value).is_some_and(|value_uses| {
        value_uses.iter().all(|use_| match use_ {
            AddressUse::Chain(child) => {
                address_has_only_terminal_consumers(*child, uses, memo, visiting)
            }
            AddressUse::Terminal => true,
            AddressUse::Escape => false,
        })
    });
    visiting.remove(&value);
    memo.insert(value, result);
    result
}

pub(super) fn foldable_local_addresses(
    function: &l::Function,
    definitions: &HashMap<l::ValueId, &l::Instruction>,
) -> HashSet<l::ValueId> {
    let mut origin_memo = HashMap::new();
    let candidates = definitions
        .keys()
        .copied()
        .filter(|value| {
            has_local_address_origin(*value, definitions, &mut origin_memo, &mut HashSet::new())
        })
        .collect::<HashSet<_>>();
    let mut uses = candidates
        .iter()
        .map(|value| (*value, Vec::new()))
        .collect::<HashMap<_, _>>();

    for block in &function.blocks {
        for instruction in &block.instructions {
            for (index, operand) in instruction.operands.iter().enumerate() {
                let l::Operand::Value(value) = operand else {
                    continue;
                };
                let Some(value_uses) = uses.get_mut(value) else {
                    continue;
                };
                let use_ = match (&instruction.kind, index, instruction.result) {
                    (l::InstructionKind::LoadAddress, 0, _)
                    | (l::InstructionKind::StoreAddress, 0, _) => AddressUse::Terminal,
                    (l::InstructionKind::AddressOfField(_), 0, Some(child))
                    | (l::InstructionKind::AddressOfIndex { .. }, 0, Some(child))
                        if candidates.contains(&child) =>
                    {
                        AddressUse::Chain(child)
                    }
                    _ => AddressUse::Escape,
                };
                value_uses.push(use_);
            }
            for value in &instruction.invalidates {
                if let Some(value_uses) = uses.get_mut(value) {
                    value_uses.push(AddressUse::Escape);
                }
            }
        }
        record_terminator_address_escapes(&mut uses, &block.terminator);
    }

    let mut memo = HashMap::new();
    candidates
        .into_iter()
        .filter(|value| {
            address_has_only_terminal_consumers(*value, &uses, &mut memo, &mut HashSet::new())
        })
        .collect()
}

pub(super) fn promoted_local_values(
    function: &l::Function,
    index: &EmissionIndex<'_>,
    folded_addresses: &HashSet<l::ValueId>,
) -> HashMap<l::LocalId, l::ValueId> {
    let materialized_locals = index
        .local_addresses
        .iter()
        .filter_map(|(local, result)| {
            (!result.is_some_and(|result| folded_addresses.contains(&result))).then_some(*local)
        })
        .collect::<HashSet<_>>();
    let parameter_values = function
        .parameters
        .iter()
        .filter_map(|parameter| parameter.storage.map(|local| (local, parameter.value)))
        .collect::<HashMap<_, _>>();
    let mut used_values = HashSet::new();
    function
        .locals
        .iter()
        .filter(|local| !materialized_locals.contains(&local.id))
        .filter_map(|local| {
            let value = parameter_values
                .get(&local.id)
                .copied()
                .or_else(|| index.first_stores.get(&local.id).copied())?;
            (used_values.insert(value)
                && match index.local_seeds[value.0 as usize] {
                    LocalSeed::Unused => true,
                    LocalSeed::Local(candidate) => candidate == local.id,
                    LocalSeed::Other => false,
                })
            .then_some((local.id, value))
        })
        .collect()
}

pub(super) fn declaration_can_use_instruction_assignment(kind: &l::InstructionKind) -> bool {
    matches!(
        kind,
        l::InstructionKind::Copy
            | l::InstructionKind::StringLiteral(_)
            | l::InstructionKind::Zero
            | l::InstructionKind::LoadLocal(_)
            | l::InstructionKind::AddressOfLocal(_)
            | l::InstructionKind::LoadGlobal(_)
            | l::InstructionKind::AddressOfGlobal(_)
            | l::InstructionKind::FunctionRef(_)
            | l::InstructionKind::Unary(_)
            | l::InstructionKind::AllocateClass(_)
            | l::InstructionKind::AddressOfValue
            | l::InstructionKind::LoadAddress
            | l::InstructionKind::LoadField(_)
            | l::InstructionKind::Length
            | l::InstructionKind::ForeignArrayData
            | l::InstructionKind::ArrayLiteral
            | l::InstructionKind::ArrayWithCapacity
            | l::InstructionKind::IteratorCreate { .. }
    )
}

fn record_value_reference(
    references: &mut [HashSet<l::BlockId>],
    value_storage: &[l::ValueId],
    operand: &l::Operand,
    block: l::BlockId,
) {
    if let l::Operand::Value(value) = operand {
        references[value_storage[value.0 as usize].0 as usize].insert(block);
    }
}

fn record_terminator_value_references(
    function: &l::Function,
    references: &mut [HashSet<l::BlockId>],
    value_storage: &[l::ValueId],
    forced_function: &mut HashSet<l::ValueId>,
    block: l::BlockId,
    terminator: &l::Terminator,
) {
    for value in terminator.value_uses() {
        references[value_storage[value.0 as usize].0 as usize].insert(block);
    }
    if let l::Terminator::Suspend {
        successor,
        resume_value,
        ..
    } = terminator
    {
        // Declaration scope follows emitted reads, not invalidation metadata.
        for parameter in &function.blocks[successor.0 as usize].parameters {
            forced_function.insert(value_storage[parameter.0 as usize]);
        }
        if let Some(value) = resume_value {
            forced_function.insert(value_storage[value.0 as usize]);
        }
    } else {
        for target in terminator.targets() {
            for parameter in &function.blocks[target.block.0 as usize].parameters {
                references[value_storage[parameter.0 as usize].0 as usize].insert(block);
            }
        }
    }
}

fn terminator_successors(terminator: &l::Terminator) -> Vec<l::BlockId> {
    terminator.successors()
}

pub(super) fn fixed_iterator_values(
    function: &l::Function,
    index: &EmissionIndex<'_>,
) -> HashSet<l::ValueId> {
    // Each bit crosses each propagation edge at most once, including cycles.
    const FIXED: u8 = 1;
    const LIVE: u8 = 2;
    if index.iterator_seeds.is_empty() {
        return HashSet::new();
    }
    let mut bounds = vec![0u8; function.values.len()];
    let mut dependents = vec![Vec::new(); function.values.len()];
    let mut pending = std::collections::VecDeque::new();
    for &(result, bound) in &index.iterator_seeds {
        let bit = match bound {
            l::IteratorBoundKind::Fixed => FIXED,
            l::IteratorBoundKind::Live => LIVE,
        };
        bounds[result.0 as usize] = bit;
        pending.push_back((result, bit));
    }
    for &(source, result) in &index.propagated_values {
        dependents[source.0 as usize].push(result);
    }
    for block in &function.blocks {
        for target in &index.incoming_targets[block.id.0 as usize] {
            for (argument, parameter) in target.arguments.iter().zip(&block.parameters) {
                if let l::Operand::Value(source) = argument {
                    dependents[source.0 as usize].push(*parameter);
                }
            }
        }
    }
    while let Some((source, incoming)) = pending.pop_front() {
        for destination in &dependents[source.0 as usize] {
            let bound = &mut bounds[destination.0 as usize];
            let added = incoming & !*bound;
            if added != 0 {
                *bound |= added;
                pending.push_back((*destination, added));
            }
        }
    }
    bounds
        .into_iter()
        .enumerate()
        .filter_map(|(index, bound)| (bound == FIXED).then_some(l::ValueId(index as u32)))
        .collect()
}

pub(super) fn removable_block_parameter_copies(
    function: &l::Function,
    index: &EmissionIndex<'_>,
) -> (
    HashSet<(l::BlockId, l::BlockId, usize)>,
    HashSet<l::ValueId>,
) {
    let mut removable = HashSet::new();
    let mut incoming = HashMap::<l::ValueId, usize>::new();
    let mut removable_incoming = HashMap::<l::ValueId, usize>::new();
    for block in &function.blocks {
        if matches!(block.terminator, l::Terminator::Suspend { .. }) {
            continue;
        }
        for target in block.terminator.targets() {
            let destination = &function.blocks[target.block.0 as usize];
            let last_arguments = target
                .arguments
                .iter()
                .enumerate()
                .filter_map(|(position, operand)| {
                    if let l::Operand::Value(value) = operand {
                        Some((*value, position))
                    } else {
                        None
                    }
                })
                .collect::<HashMap<_, _>>();
            for (position, (argument, parameter)) in target
                .arguments
                .iter()
                .zip(&destination.parameters)
                .enumerate()
            {
                *incoming.entry(*parameter).or_default() += 1;
                if !index.use_blocks[parameter.0 as usize].is_empty() {
                    continue;
                }
                let source_dead = match argument {
                    l::Operand::Constant(_) => true,
                    l::Operand::Value(source) => {
                        let used_from_target = index.reachable_use_rows[source.0 as usize]
                            .is_some_and(|row| {
                                index.reachable_uses
                                    [row * function.blocks.len() + target.block.0 as usize]
                            });
                        !used_from_target && last_arguments.get(source) == Some(&position)
                    }
                };
                if source_dead {
                    removable.insert((block.id, target.block, position));
                    *removable_incoming.entry(*parameter).or_default() += 1;
                }
            }
        }
    }
    let elided_values = incoming
        .into_iter()
        .filter_map(|(value, count)| {
            (removable_incoming.get(&value).copied() == Some(count)).then_some(value)
        })
        .collect();
    (removable, elided_values)
}

impl Coalescing {
    fn new(origins: &[l::ValueId]) -> Self {
        Self {
            parents: (0..origins.len()).collect(),
            groups: origins
                .iter()
                .copied()
                .map(root_storage::InterferenceGroup::Origin)
                .collect(),
        }
    }

    fn root(&self, value: l::ValueId) -> usize {
        let mut root = value.0 as usize;
        while self.parents[root] != root {
            root = self.parents[root];
        }
        root
    }

    fn try_merge(
        &mut self,
        left: l::ValueId,
        right: l::ValueId,
        interference: &root_storage::Interference,
    ) {
        let left = self.root(left);
        let right = self.root(right);
        if left == right {
            return;
        }
        if self.groups[left].interferes(&self.groups[right], interference) {
            return;
        }
        let (representative, merged) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        self.parents[merged] = representative;
        let merged_group = std::mem::take(&mut self.groups[merged]);
        self.groups[representative].merge(merged_group, interference);
    }

    fn representatives(&self) -> Vec<l::ValueId> {
        (0..self.parents.len())
            .map(|index| l::ValueId(self.root(l::ValueId(index as u32)) as u32))
            .collect()
    }
}

pub(super) fn coalesced_value_storage(
    function: &l::Function,
    root_storage: &RootStoragePlan,
    interference: &root_storage::Interference,
    folded_addresses: &HashSet<l::ValueId>,
    removable_edge_copies: &HashSet<(l::BlockId, l::BlockId, usize)>,
    elided_values: &HashSet<l::ValueId>,
    promoted_local_values: &HashSet<l::ValueId>,
) -> Result<Vec<l::ValueId>, String> {
    let mut coalescing = Coalescing::new(&function.liveness.value_origins);
    for (index, slot) in root_storage.value_slots.iter().copied().enumerate() {
        if let Some(slot) = slot {
            coalescing.try_merge(
                l::ValueId(index as u32),
                root_storage.slots[slot].representative,
                interference,
            );
        }
    }
    // Prefer every block-parameter copy. A merge is valid only when no value
    // in either storage group interferes with a value in the other group.
    for block in &function.blocks {
        if matches!(block.terminator, l::Terminator::Suspend { .. }) {
            continue;
        }
        for target in block.terminator.targets() {
            let destination = &function.blocks[target.block.0 as usize];
            for (index, (argument, parameter)) in target
                .arguments
                .iter()
                .zip(&destination.parameters)
                .enumerate()
            {
                if removable_edge_copies.contains(&(block.id, target.block, index))
                    || elided_values.contains(parameter)
                    || folded_addresses.contains(parameter)
                    || promoted_local_values.contains(parameter)
                {
                    continue;
                }
                let l::Operand::Value(argument) = argument else {
                    continue;
                };
                if elided_values.contains(argument)
                    || folded_addresses.contains(argument)
                    || promoted_local_values.contains(argument)
                    || function.values[argument.0 as usize].ty
                        != function.values[parameter.0 as usize].ty
                    || root_storage.value_slots[argument.0 as usize]
                        != root_storage.value_slots[parameter.0 as usize]
                {
                    continue;
                }
                coalescing.try_merge(*argument, *parameter, interference);
            }
        }
    }
    Ok(coalescing.representatives())
}

pub(super) fn declaration_scopes(
    function: &l::Function,
    coroutine: bool,
    rooted_values: &HashSet<l::ValueId>,
    folded_addresses: &HashSet<l::ValueId>,
    elided_values: &HashSet<l::ValueId>,
    value_storage: &[l::ValueId],
    promoted_locals: &HashMap<l::LocalId, l::ValueId>,
) -> DeclarationScopes {
    let block_count = function.blocks.len();
    let mut block_values = vec![Vec::new(); block_count];
    let mut dominator_children = vec![Vec::new(); block_count];
    if coroutine {
        return DeclarationScopes {
            function_values: function
                .values
                .iter()
                .map(|value| value.id)
                .filter(|value| {
                    value_storage[value.0 as usize] == *value
                        && !rooted_values.contains(value)
                        && !folded_addresses.contains(value)
                        && !elided_values.contains(value)
                })
                .collect(),
            block_values,
            dominator_children,
            graph_roots: function.blocks.iter().map(|block| block.id).collect(),
        };
    }

    let mut references = vec![HashSet::new(); function.values.len()];
    let mut forced_function = function
        .parameters
        .iter()
        .map(|parameter| value_storage[parameter.value.0 as usize])
        .collect::<HashSet<_>>();
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Some(result) = instruction.result {
                references[value_storage[result.0 as usize].0 as usize].insert(block.id);
            }
            for operand in &instruction.operands {
                record_value_reference(&mut references, value_storage, operand, block.id);
            }
            let promoted_local = match &instruction.kind {
                l::InstructionKind::LoadLocal(local)
                | l::InstructionKind::StoreLocal(local)
                | l::InstructionKind::AddressOfLocal(local) => promoted_locals.get(local),
                _ => None,
            };
            if let Some(value) = promoted_local {
                references[value_storage[value.0 as usize].0 as usize].insert(block.id);
            }
            for value in &instruction.invalidates {
                references[value_storage[value.0 as usize].0 as usize].insert(block.id);
            }
        }
        record_terminator_value_references(
            function,
            &mut references,
            value_storage,
            &mut forced_function,
            block.id,
            &block.terminator,
        );
    }

    let mut predecessors = vec![Vec::new(); block_count];
    for block in &function.blocks {
        for successor in terminator_successors(&block.terminator) {
            predecessors[successor.0 as usize].push(block.id);
        }
    }
    let entry = function.entry.0 as usize;
    let mut reachable = vec![false; block_count];
    let mut pending = vec![function.entry];
    while let Some(block) = pending.pop() {
        let index = block.0 as usize;
        if reachable[index] {
            continue;
        }
        reachable[index] = true;
        pending.extend(terminator_successors(&function.blocks[index].terminator));
    }
    let reachable_blocks = reachable
        .iter()
        .enumerate()
        .filter_map(|(index, reachable)| reachable.then_some(l::BlockId(index as u32)))
        .collect::<HashSet<_>>();
    let mut dominators = vec![HashSet::new(); block_count];
    for (index, is_reachable) in reachable.iter().copied().enumerate() {
        if !is_reachable {
            dominators[index].insert(l::BlockId(index as u32));
        } else if index == entry {
            dominators[index].insert(function.entry);
        } else {
            dominators[index] = reachable_blocks.clone();
        }
    }
    loop {
        let mut changed = false;
        for index in 0..block_count {
            if index == entry || !reachable[index] {
                continue;
            }
            let mut incoming = predecessors[index]
                .iter()
                .copied()
                .filter(|predecessor| reachable[predecessor.0 as usize]);
            let mut next = incoming
                .next()
                .map(|predecessor| dominators[predecessor.0 as usize].clone())
                .unwrap_or_default();
            for predecessor in incoming {
                next.retain(|dominator| dominators[predecessor.0 as usize].contains(dominator));
            }
            next.insert(l::BlockId(index as u32));
            if dominators[index] != next {
                dominators[index] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for index in 0..block_count {
        if index == entry || !reachable[index] {
            continue;
        }
        let block = l::BlockId(index as u32);
        let immediate = dominators[index]
            .iter()
            .copied()
            .filter(|dominator| *dominator != block)
            .max_by_key(|dominator| dominators[dominator.0 as usize].len());
        if let Some(immediate) = immediate {
            dominator_children[immediate.0 as usize].push(block);
        }
    }
    for children in &mut dominator_children {
        children.sort_by_key(|block| block.0);
    }

    let mut function_values = HashSet::new();
    for value in &function.values {
        if value_storage[value.id.0 as usize] != value.id
            || rooted_values.contains(&value.id)
            || folded_addresses.contains(&value.id)
            || elided_values.contains(&value.id)
        {
            continue;
        }
        if forced_function.contains(&value.id) || references[value.id.0 as usize].is_empty() {
            function_values.insert(value.id);
            continue;
        }
        let mut blocks = references[value.id.0 as usize].iter().copied();
        let Some(first) = blocks.next() else {
            function_values.insert(value.id);
            continue;
        };
        if !reachable[first.0 as usize] {
            function_values.insert(value.id);
            continue;
        }
        let mut common = dominators[first.0 as usize].clone();
        let mut all_reachable = true;
        for block in blocks {
            if !reachable[block.0 as usize] {
                all_reachable = false;
                break;
            }
            common.retain(|dominator| dominators[block.0 as usize].contains(dominator));
        }
        let scope = all_reachable
            .then(|| {
                common
                    .into_iter()
                    .max_by_key(|dominator| dominators[dominator.0 as usize].len())
            })
            .flatten();
        if let Some(scope) = scope {
            block_values[scope.0 as usize].push(value.id);
        } else {
            function_values.insert(value.id);
        }
    }
    let mut graph_roots = vec![function.entry];
    graph_roots.extend(
        function
            .blocks
            .iter()
            .filter(|block| !reachable[block.id.0 as usize])
            .map(|block| block.id),
    );
    DeclarationScopes {
        function_values,
        block_values,
        dominator_children,
        graph_roots,
    }
}
