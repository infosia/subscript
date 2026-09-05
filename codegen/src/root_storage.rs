//! Shared live-range storage plan for managed LIR values.

use std::collections::BTreeSet;

use subscript_compiler::lir as l;
#[cfg(test)]
use subscript_compiler::Type;

use crate::layout::{managed_words, Layouts};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootSlot {
    pub(crate) representative: l::ValueId,
    pub(crate) words: u32,
    pub(crate) offset: u32,
    ty: l::ValueType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

fn record_terminator(
    function: &l::Function,
    values: &mut BTreeSet<l::ValueId>,
    terminator: &l::Terminator,
) -> Result<(), String> {
    for value in terminator.value_uses() {
        record_value(function, values, value)?;
    }
    if let l::Terminator::Suspend { invalidates, .. } = terminator {
        // Root interference needs every storage mention during suspension.
        for value in invalidates {
            record_value(function, values, *value)?;
        }
    }
    Ok(())
}

fn successors(terminator: &l::Terminator) -> Vec<l::BlockId> {
    terminator.successors()
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

/// One inclusive live interval in a block.
/// Coordinates: 0 is entry, i+1 is instruction i, and n+1 is the terminator/exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LiveInterval {
    block: l::BlockId,
    start: usize,
    end: usize,
}

/// Origin live intervals and indexed parameter rules for one function.
#[derive(Debug)]
pub(crate) struct Interference {
    intervals: Vec<Vec<LiveInterval>>,
    parameter_rules: Vec<(BTreeSet<l::ValueId>, BTreeSet<l::ValueId>)>,
    parameter_index: Vec<ParameterIndex>,
    origin_groups: Vec<GroupFacts>,
}

impl Interference {
    /// Builds the function's relation once from its root-storage mentions.
    pub(crate) fn build(function: &l::Function) -> Result<Self, String> {
        let held = address_taken_values(function)?;
        let live_in = live_ins(function, &held)?;
        let mut intervals = vec![Vec::new(); function.values.len()];
        let mut parameter_rules = Vec::new();
        for block in &function.blocks {
            let mut live = successors(&block.terminator)
                .into_iter()
                .flat_map(|successor| live_in[successor.0 as usize].iter().copied())
                .collect::<BTreeSet<_>>();
            live.extend(held.iter().copied());
            record_terminator(function, &mut live, &block.terminator)?;
            // Zero is entry; instructions are 1..=n; n+1 is the terminator/exit.
            let mut ends = live
                .iter()
                .map(|value| (*value, block.instructions.len() + 1))
                .collect::<std::collections::BTreeMap<_, _>>();
            for (index, instruction) in block.instructions.iter().enumerate().rev() {
                let point = index + 1;
                if let Some(result) = instruction.result {
                    let result = origin(function, result)?;
                    let end = ends.remove(&result).unwrap_or(point);
                    intervals[result.0 as usize].push(LiveInterval {
                        block: block.id,
                        start: point,
                        end,
                    });
                    live.remove(&result);
                }
                let mut uses = BTreeSet::new();
                for operand in &instruction.operands {
                    record_operand(function, &mut uses, operand)?;
                }
                for value in uses {
                    ends.entry(value).or_insert(point);
                    live.insert(value);
                }
            }
            let parameters = block
                .parameters
                .iter()
                .map(|value| origin(function, *value))
                .collect::<Result<BTreeSet<_>, _>>()?;
            parameter_rules.push((parameters, live));
            for (value, end) in ends {
                intervals[value.0 as usize].push(LiveInterval {
                    block: block.id,
                    start: 0,
                    end,
                });
            }
        }
        let parameters = function
            .parameters
            .iter()
            .map(|parameter| origin(function, parameter.value))
            .collect::<Result<BTreeSet<_>, _>>()?;
        parameter_rules.push((parameters, live_in[function.entry.0 as usize].clone()));
        for ranges in &mut intervals {
            ranges.sort_by_key(|range| (range.block, range.start, range.end));
        }
        let mut parameter_index = vec![ParameterIndex::default(); function.values.len()];
        let mut origin_groups = intervals
            .iter()
            .enumerate()
            .map(|(origin, intervals)| GroupFacts {
                intervals: merge_intervals(
                    &[],
                    &intervals
                        .iter()
                        .map(|interval| OriginInterval {
                            interval: *interval,
                            origin: l::ValueId(origin as u32),
                        })
                        .collect::<Vec<_>>(),
                ),
                parameters: Vec::new(),
                mentions: Vec::new(),
            })
            .collect::<Vec<_>>();
        for (rule, (parameters, live)) in parameter_rules.iter().enumerate() {
            for parameter in parameters {
                let index = &mut parameter_index[parameter.0 as usize];
                if rule == function.blocks.len() {
                    index.function_parameter = true;
                } else {
                    index.blocks.push(rule);
                }
                origin_groups[parameter.0 as usize]
                    .parameters
                    .push(Membership::single(rule, *parameter));
            }
            // A rule with no parameters cannot prevent any merge.
            if !parameters.is_empty() {
                for value in parameters.union(live) {
                    origin_groups[value.0 as usize]
                        .mentions
                        .push(Membership::single(rule, *value));
                }
            }
        }
        Ok(Self {
            intervals,
            parameter_rules,
            parameter_index,
            origin_groups,
        })
    }

    /// Tests two origins; an origin never interferes with itself.
    pub(crate) fn interferes(&self, a: l::ValueId, b: l::ValueId) -> bool {
        if a == b {
            return false;
        }
        let left = &self.intervals[a.0 as usize];
        let right = &self.intervals[b.0 as usize];
        let (mut i, mut j) = (0, 0);
        while i < left.len() && j < right.len() {
            let (a, b) = (left[i], right[j]);
            if a.block == b.block && a.start <= b.end && b.start <= a.end {
                return true;
            }
            if (a.block, a.end) < (b.block, b.end) {
                i += 1;
            } else {
                j += 1;
            }
        }
        self.parameter_conflict(a, b) || self.parameter_conflict(b, a)
    }

    fn parameter_conflict(&self, parameter: l::ValueId, other: l::ValueId) -> bool {
        let index = &self.parameter_index[parameter.0 as usize];
        let contains = |rule: usize| {
            let (parameters, live) = &self.parameter_rules[rule];
            parameters.contains(&other) || live.contains(&other)
        };
        index.blocks.iter().copied().any(contains)
            || (index.function_parameter && contains(self.parameter_rules.len() - 1))
    }
}

#[derive(Debug, Clone, Default)]
struct ParameterIndex {
    blocks: Vec<usize>,
    function_parameter: bool,
}

#[derive(Debug, Clone, Copy)]
struct OriginInterval {
    interval: LiveInterval,
    origin: l::ValueId,
}

// A membership retains identity to exempt aliases of the same origin.
#[derive(Debug, Clone, Copy)]
struct Membership {
    rule: usize,
    first: l::ValueId,
    multiple: bool,
}

impl Membership {
    fn single(rule: usize, origin: l::ValueId) -> Self {
        Self {
            rule,
            first: origin,
            multiple: false,
        }
    }

    fn differs(self, other: Self) -> bool {
        self.multiple || other.multiple || self.first != other.first
    }
}

/// Sorted interval and parameter-membership unions for a compatible group.
#[derive(Debug, Clone, Default)]
pub(crate) struct GroupFacts {
    intervals: Vec<OriginInterval>,
    parameters: Vec<Membership>,
    mentions: Vec<Membership>,
}

/// A storage group's merged intervals and parameter-rule memberships.
/// Single-origin groups borrow their facts from the shared relation.
#[derive(Debug, Default)]
pub(crate) enum InterferenceGroup {
    #[default]
    Empty,
    Origin(l::ValueId),
    Merged(GroupFacts),
}

impl InterferenceGroup {
    fn facts<'a>(&'a self, interference: &'a Interference) -> Option<&'a GroupFacts> {
        match self {
            Self::Empty => None,
            Self::Origin(origin) => Some(&interference.origin_groups[origin.0 as usize]),
            Self::Merged(facts) => Some(facts),
        }
    }

    /// Tests two groups with one interval sweep and indexed parameter checks.
    pub(crate) fn interferes(&self, other: &Self, interference: &Interference) -> bool {
        if let (Self::Origin(a), Self::Origin(b)) = (self, other) {
            return interference.interferes(*a, *b);
        }
        let (Some(left), Some(right)) = (self.facts(interference), other.facts(interference))
        else {
            return false;
        };
        let separated = left
            .intervals
            .last()
            .zip(right.intervals.first())
            .is_some_and(|(a, b)| {
                (a.interval.block, a.interval.end) < (b.interval.block, b.interval.start)
            })
            || right
                .intervals
                .last()
                .zip(left.intervals.first())
                .is_some_and(|(a, b)| {
                    (a.interval.block, a.interval.end) < (b.interval.block, b.interval.start)
                });
        if !separated {
            // Compatible groups have disjoint, sorted ranges. Skip expired prefixes.
            let mut i = right.intervals.first().map_or(0, |first| {
                left.intervals.partition_point(|range| {
                    (range.interval.block, range.interval.end)
                        < (first.interval.block, first.interval.start)
                })
            });
            let mut j = left.intervals.first().map_or(0, |first| {
                right.intervals.partition_point(|range| {
                    (range.interval.block, range.interval.end)
                        < (first.interval.block, first.interval.start)
                })
            });
            while i < left.intervals.len() && j < right.intervals.len() {
                let (a, b) = (left.intervals[i], right.intervals[j]);
                if a.interval.block == b.interval.block
                    && a.interval.start <= b.interval.end
                    && b.interval.start <= a.interval.end
                    && a.origin != b.origin
                {
                    return true;
                }
                if (a.interval.block, a.interval.end) < (b.interval.block, b.interval.end) {
                    i += 1;
                } else {
                    j += 1;
                }
            }
        }
        memberships_conflict(&left.parameters, &right.mentions)
            || memberships_conflict(&right.parameters, &left.mentions)
    }

    /// Unites compatible groups; ordered new ranges append without a full-list copy.
    pub(crate) fn merge(&mut self, other: Self, interference: &Interference) {
        if let (Self::Origin(a), Self::Origin(b)) = (&*self, &other) {
            if a == b {
                return;
            }
        }
        if matches!(self, Self::Empty) {
            *self = other;
            return;
        }
        if let Self::Merged(left) = self {
            if let Some(right) = other.facts(interference) {
                left.merge(right);
            }
            return;
        }
        if let Self::Merged(mut right) = other {
            if let Some(left) = self.facts(interference) {
                right.merge(left);
            }
            *self = Self::Merged(right);
            return;
        }
        let (Some(left), Some(right)) = (self.facts(interference), other.facts(interference))
        else {
            return;
        };
        let mut merged = left.clone();
        merged.merge(right);
        *self = Self::Merged(merged);
    }
}

impl GroupFacts {
    fn merge(&mut self, other: &Self) {
        if !other.intervals.is_empty() {
            let append = self
                .intervals
                .last()
                .zip(other.intervals.first())
                .is_none_or(|(a, b)| {
                    (a.interval.block, a.interval.end) < (b.interval.block, b.interval.start)
                });
            if append {
                self.intervals.extend_from_slice(&other.intervals);
            } else {
                self.intervals = merge_intervals(&self.intervals, &other.intervals);
            }
        }
        if !other.parameters.is_empty() {
            self.parameters = merge_memberships(&self.parameters, &other.parameters);
        }
        if !other.mentions.is_empty() {
            self.mentions = merge_memberships(&self.mentions, &other.mentions);
        }
    }
}

fn memberships_conflict(parameters: &[Membership], mentions: &[Membership]) -> bool {
    parameters.iter().any(|parameter| {
        mentions
            .binary_search_by_key(&parameter.rule, |mention| mention.rule)
            .is_ok_and(|index| parameter.differs(mentions[index]))
    })
}

fn merge_memberships(left: &[Membership], right: &[Membership]) -> Vec<Membership> {
    let mut merged = Vec::with_capacity(left.len() + right.len());
    let (mut i, mut j) = (0, 0);
    while i < left.len() || j < right.len() {
        if j == right.len() || (i < left.len() && left[i].rule < right[j].rule) {
            merged.push(left[i]);
            i += 1;
        } else if i == left.len() || right[j].rule < left[i].rule {
            merged.push(right[j]);
            j += 1;
        } else {
            merged.push(Membership {
                rule: left[i].rule,
                first: left[i].first,
                multiple: left[i].differs(right[j]),
            });
            i += 1;
            j += 1;
        }
    }
    merged
}

fn merge_intervals(left: &[OriginInterval], right: &[OriginInterval]) -> Vec<OriginInterval> {
    let mut merged = Vec::<OriginInterval>::with_capacity(left.len() + right.len());
    let (mut i, mut j) = (0, 0);
    while i < left.len() || j < right.len() {
        let next = if j == right.len()
            || (i < left.len()
                && (left[i].interval.block, left[i].interval.start)
                    <= (right[j].interval.block, right[j].interval.start))
        {
            let next = left[i];
            i += 1;
            next
        } else {
            let next = right[j];
            j += 1;
            next
        };
        if let Some(last) = merged.last_mut() {
            if last.origin == next.origin
                && last.interval.block == next.interval.block
                && next.interval.start <= last.interval.end
            {
                last.interval.end = last.interval.end.max(next.interval.end);
                continue;
            }
        }
        merged.push(next);
    }
    merged
}

pub(crate) fn managed_value_words(layouts: &Layouts, ty: &l::ValueType) -> Result<u32, String> {
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
    plan_with_interference(function, layouts).map(|(plan, _)| plan)
}

/// Builds one interference relation and returns it with the root-storage plan.
pub(crate) fn plan_with_interference(
    function: &l::Function,
    layouts: &Layouts,
) -> Result<(RootStoragePlan, Interference), String> {
    let interference = Interference::build(function)?;
    let mut slots = Vec::<RootSlot>::new();
    let mut slot_groups = Vec::<InterferenceGroup>::new();
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
        let candidate = InterferenceGroup::Origin(value.id);
        let reusable = slots.iter().enumerate().position(|(index, slot)| {
            slot.ty == value.ty && !slot_groups[index].interferes(&candidate, &interference)
        });
        let slot = if let Some(slot) = reusable {
            slot_groups[slot].merge(candidate, &interference);
            slot
        } else {
            let slot = slots.len();
            slot_groups.push(candidate);
            slots.push(RootSlot {
                representative: value.id,
                words,
                offset: 0,
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

    let mut block_starts = vec![BTreeSet::new(); function.blocks.len()];
    let mut block_ends = vec![BTreeSet::new(); function.blocks.len()];
    let mut clear_after_instruction = function
        .blocks
        .iter()
        .map(|block| vec![Vec::new(); block.instructions.len()])
        .collect::<Vec<_>>();
    for (slot, group) in slot_groups.iter().enumerate() {
        let Some(facts) = group.facts(&interference) else {
            continue;
        };
        for range in &facts.intervals {
            let range = range.interval;
            let block = range.block.0 as usize;
            if range.start == 0 {
                block_starts[block].insert(slot);
            }
            if range.end == function.blocks[block].instructions.len() + 1 {
                block_ends[block].insert(slot);
            } else {
                // Every end is an instruction coordinate or the terminator/exit, hence at least 1.
                clear_after_instruction[block][range.end - 1].push(slot);
            }
        }
    }
    let mut candidates = vec![BTreeSet::new(); function.blocks.len()];
    for block in &function.blocks {
        for successor in successors(&block.terminator) {
            candidates[successor.0 as usize].extend(&block_ends[block.id.0 as usize]);
        }
    }
    let mut clear_at_block_entry = vec![Vec::new(); function.blocks.len()];
    for block in &function.blocks {
        let index = block.id.0 as usize;
        candidates[index].extend(occupied_slots(
            &value_slots,
            block.parameters.iter().copied(),
        ));
        if block.id == function.entry {
            candidates[index].extend(occupied_slots(
                &value_slots,
                function.parameters.iter().map(|p| p.value),
            ));
        }
        clear_at_block_entry[index] = candidates[index]
            .difference(&block_starts[index])
            .copied()
            .collect();
    }
    Ok((
        RootStoragePlan {
            slots,
            value_slots,
            clear_at_block_entry,
            clear_after_instruction,
            words,
        },
        interference,
    ))
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
            fresh_owner: false,
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

    fn parameter(id: u32) -> l::Parameter {
        l::Parameter {
            storage: None,
            value: l::ValueId(id),
            source_name: format!("p{id}"),
            kind: l::ParameterKind::Explicit,
            pos: pos(),
        }
    }

    // %0 dies at %1's definition; %2 leaves b0 and enters b1.
    // The unused b1 parameter %4 tests a nonempty entry clear set.
    fn interval_fixture() -> l::Function {
        let values = (0..5)
            .map(|id| value(id, l::ValueType::Data(Type::Nullable(Box::new(Type::Str)))))
            .collect();
        let mut first = return_block(
            0,
            vec![
                instruction(
                    Some(1),
                    l::InstructionKind::Coerce,
                    vec![l::Operand::Value(l::ValueId(0))],
                ),
                instruction(
                    Some(2),
                    l::InstructionKind::Coerce,
                    vec![l::Operand::Value(l::ValueId(1))],
                ),
            ],
        );
        first.terminator = l::Terminator::Branch(l::BlockTarget {
            block: l::BlockId(1),
            arguments: vec![l::Operand::Constant(l::Constant {
                ty: Type::Nullable(Box::new(Type::Str)),
                kind: l::ConstantKind::Null,
            })],
        });
        let mut second = return_block(
            1,
            vec![instruction(
                Some(3),
                l::InstructionKind::Coerce,
                vec![l::Operand::Value(l::ValueId(2))],
            )],
        );
        second.parameters = vec![l::ValueId(4)];
        let mut function = function(values, vec![first, second]);
        function.parameters = vec![parameter(0)];
        function.liveness.live_ins = vec![vec![l::ValueId(0)], vec![l::ValueId(2)]];
        function
    }

    #[test]
    fn interval_list_has_inclusive_definition_and_use_points() {
        let interference = Interference::build(&interval_fixture()).unwrap();
        assert_eq!(
            interference.intervals,
            vec![
                vec![LiveInterval {
                    block: l::BlockId(0),
                    start: 0,
                    end: 1
                }],
                vec![LiveInterval {
                    block: l::BlockId(0),
                    start: 1,
                    end: 2
                }],
                vec![
                    LiveInterval {
                        block: l::BlockId(0),
                        start: 2,
                        end: 3
                    },
                    LiveInterval {
                        block: l::BlockId(1),
                        start: 0,
                        end: 1
                    }
                ],
                vec![LiveInterval {
                    block: l::BlockId(1),
                    start: 1,
                    end: 1
                }],
                vec![],
            ]
        );
    }

    #[test]
    fn interval_interference_matches_the_hand_written_matrix() {
        let interference = Interference::build(&interval_fixture()).unwrap();
        let expected = [
            [false, true, false, false, false],
            [true, false, true, false, false],
            [false, true, false, true, true],
            [false, false, true, false, false],
            [false, false, true, false, false],
        ];
        for (a, row) in expected.iter().enumerate() {
            for (b, expected) in row.iter().enumerate() {
                assert_eq!(
                    interference.interferes(l::ValueId(a as u32), l::ValueId(b as u32)),
                    *expected,
                    "({a}, {b})"
                );
            }
        }
    }

    #[test]
    fn interval_ends_clear_dead_slots_and_unused_block_parameters() {
        let plan = plan(&interval_fixture(), &layouts()).unwrap();
        assert_eq!(
            plan.value_slots,
            vec![Some(0), Some(1), Some(0), Some(1), Some(1)]
        );
        assert_eq!(
            plan.clear_after_instruction,
            vec![vec![vec![0], vec![1]], vec![vec![0, 1]]]
        );
        assert_eq!(plan.clear_at_block_entry, vec![vec![], vec![1]]);
    }

    #[test]
    fn unused_parameter_pairs_interfere_at_each_kind_of_entry() {
        let values = (0..4)
            .map(|id| value(id, l::ValueType::Data(Type::Str)))
            .collect();
        let first = return_block(0, Vec::new());

        let mut second = return_block(1, Vec::new());
        second.parameters = vec![l::ValueId(2), l::ValueId(3)];
        let mut function = function(values, vec![first, second]);
        function.parameters = vec![parameter(0), parameter(1)];

        let interference = Interference::build(&function).unwrap();
        assert!(interference.intervals.iter().all(Vec::is_empty));
        assert!(interference.interferes(l::ValueId(0), l::ValueId(1)));
        assert!(interference.interferes(l::ValueId(1), l::ValueId(0)));
        assert!(interference.interferes(l::ValueId(2), l::ValueId(3)));
        assert!(interference.interferes(l::ValueId(3), l::ValueId(2)));
        assert!(!interference.interferes(l::ValueId(0), l::ValueId(2)));
        let plan = plan(&function, &layouts()).unwrap();
        assert_eq!(plan.clear_at_block_entry, vec![vec![0, 1], vec![0, 1]]);
    }

    #[test]
    fn merged_groups_preserve_aliases_and_parameter_conflicts() {
        let interference = Interference::build(&interval_fixture()).unwrap();
        let mut left = InterferenceGroup::Origin(l::ValueId(0));
        let later = InterferenceGroup::Origin(l::ValueId(2));
        assert!(!left.interferes(&later, &interference));
        left.merge(later, &interference);
        let alias = InterferenceGroup::Origin(l::ValueId(0));
        assert!(!left.interferes(&alias, &interference));
        left.merge(alias, &interference);
        assert!(left.interferes(&InterferenceGroup::Origin(l::ValueId(1)), &interference));
        let mut right = InterferenceGroup::Origin(l::ValueId(0));
        let unused_parameter = InterferenceGroup::Origin(l::ValueId(4));
        assert!(!right.interferes(&unused_parameter, &interference));
        right.merge(unused_parameter, &interference);
        assert!(left.interferes(&right, &interference));
        assert!(right.interferes(&left, &interference));
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
