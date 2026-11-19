//! Native field writes are checked against the shared LIR live sets.
//! One native lowering per coroutine corpus entry; no script runs twice.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

// This test uses the source discovery and build-option functions of the shared corpus helper.
#[allow(dead_code)]
#[path = "../../../tests/corpus/mod.rs"]
mod corpus;

// §279: this module is test-only through its parent declaration.
trait Finding<T> {
    fn finding(self, label: &str) -> Result<T, String>;
}
impl<T, E: std::fmt::Debug> Finding<T> for Result<T, E> {
    fn finding(self, label: &str) -> Result<T, String> {
        self.map_err(|error| format!("{label}: {error:?}"))
    }
}
impl<T> Finding<T> for Option<T> {
    fn finding(self, label: &str) -> Result<T, String> {
        self.ok_or_else(|| label.to_string())
    }
}
macro_rules! require {
    ($condition:expr, $($message:tt)*) => {
        if !$condition { return Err(format!($($message)*)); }
    };
}
macro_rules! require_eq {
    ($left:expr, $right:expr, $($message:tt)*) => {
        match (&$left, &$right) {
            (left, right) => {
                if left != right {
                    return Err(format!("{}\nleft: {left:?}\nright: {right:?}", format_args!($($message)*)));
                }
            }
        }
    };
}

type Point = (u32, usize);

mod ordered_operations;
use ordered_operations::{
    jit_operations, ordered_clears, ByteOperation, FieldOperations, Operation,
};

#[derive(Clone, Debug)]
struct Field {
    name: String,
    offset: u32,
    size: u32,
}

fn c_function(source: &str, id: l::FunctionId) -> Result<&str, String> {
    let marker = format!(
        "static uint8_t sub_f{}_resume(void* ctx, void* raw_frame, void* coroutine_out) {{",
        id.0
    );
    source
        .split_once(&marker)
        .finding("C resume body")?
        .1
        .split("\n}\n")
        .next()
        .finding("C function end")
}

#[derive(Default)]
struct CFieldFacts {
    operations: FieldOperations,
    raw_clears: BTreeMap<Point, BTreeSet<String>>,
    clears: BTreeMap<Point, BTreeSet<String>>,
    writes: BTreeMap<Point, BTreeSet<String>>,
}

// compiler.md §338 rule 5: one text pass supplies dispatch and resume-successor views.
fn c_field_facts(
    source: &str,
    fields: &[Field],
    entry: Option<l::BlockId>,
    resumes: &BTreeMap<u32, l::BlockId>,
) -> Result<CFieldFacts, String> {
    let known = fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut raw = (u32::MAX, 0);
    let mut located = entry.map_or(raw, |block| (block.0, 0));
    let mut facts = CFieldFacts::default();
    for line in source.lines() {
        let line = line.trim();
        if let Some(marker) = line.strip_prefix("// root-point ") {
            let mut parts = marker.split_whitespace();
            raw = (
                parts
                    .next()
                    .finding("block")?
                    .parse()
                    .finding("block number")?,
                parts
                    .next()
                    .finding("point")?
                    .parse()
                    .finding("point number")?,
            );
            located = raw;
        } else if let Some(id) = line
            .strip_prefix("resume_b")
            .and_then(|label| label.strip_suffix(':'))
        {
            if let Some(successor) = id.parse::<u32>().ok().and_then(|id| resumes.get(&id)) {
                located = (successor.0, 0);
            }
        }
        let capture = line.contains("->c");
        let copy = line.contains("memcpy(");
        let memset = line.contains("memset(");
        let mut references = BTreeMap::<&str, (bool, bool, bool, bool)>::new();
        let mut remaining = line;
        while let Some((prefix, rest)) = remaining.split_once("frame->") {
            let length = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            let name = &rest[..length];
            if known.contains(name) {
                let flags = references.entry(name).or_default();
                flags.0 |= rest[length..].starts_with(" =");
                flags.1 |= prefix.ends_with('&');
                flags.3 |= prefix.is_empty() && rest[length..].starts_with(" = ")
                    || prefix.ends_with("memcpy(&")
                    || prefix.ends_with("memset(&")
                    || prefix.ends_with('&')
                        && rest[length..].starts_with(")->c")
                        && line.contains(" = ");
                flags.2 |= line.starts_with("memset(")
                    && prefix.ends_with("memset(&")
                    && rest[length..].starts_with(", 0,");
            }
            remaining = &rest[length..];
        }
        for (name, (assigned, addressed, zeroed, destination)) in references {
            let clear = zeroed
                || (assigned
                    && line
                        .strip_prefix("frame->")
                        .and_then(|rest| rest.strip_prefix(name))
                        == Some(" = NULL;"));
            let replace = assigned || addressed && (copy || capture);
            for (point, clears) in [(raw, &mut facts.raw_clears), (located, &mut facts.clears)] {
                if clear {
                    clears.entry(point).or_default().insert(name.to_string());
                } else if replace {
                    clears.entry(point).or_default().remove(name);
                }
            }
            if assigned || addressed && (copy || memset || capture) {
                if destination {
                    facts.operations.entry(raw).or_default().push((
                        name.to_string(),
                        if clear {
                            Operation::Zero
                        } else {
                            Operation::Write
                        },
                    ));
                }
                facts
                    .writes
                    .entry(located)
                    .or_default()
                    .insert(name.to_string());
            }
        }
    }
    Ok(facts)
}

fn c_writes(source: &str, fields: &[Field]) -> Result<BTreeMap<Point, BTreeSet<String>>, String> {
    Ok(c_field_facts(source, fields, None, &BTreeMap::new())?.writes)
}

#[derive(Default)]
struct JitByteFacts {
    operations: BTreeMap<Point, Vec<ByteOperation>>,
    zero: BTreeMap<Point, BTreeSet<u32>>,
    positive: BTreeMap<Point, BTreeSet<u32>>,
}

fn jit_bytes_with_frame(
    text: &str,
    frame: &str,
    initial_size: u32,
    nonzero: bool,
) -> Result<BTreeMap<Point, BTreeSet<u32>>, String> {
    let facts = jit_byte_facts(text, frame, initial_size)?;
    Ok(if nonzero { facts.positive } else { facts.zero })
}

fn jit_byte_facts(text: &str, frame: &str, initial_size: u32) -> Result<JitByteFacts, String> {
    let mut addresses = BTreeMap::<String, i64>::from([(frame.into(), 0)]);
    let mut zeros = BTreeMap::<String, u32>::new();
    let mut constants = BTreeMap::<String, u32>::new();
    let mut widths = BTreeMap::<String, u32>::new();
    let mut returns = BTreeMap::<String, u32>::new();
    for line in text.lines() {
        if let Some((name, expression)) = line.trim().split_once(" = ") {
            if name.starts_with("sig") {
                if let Some(width) = expression
                    .split_once(" -> ")
                    .and_then(|(_, ty)| clif_width(ty))
                {
                    returns.insert(name.into(), width);
                }
            } else if name.starts_with("fn") {
                if let Some(width) = expression
                    .split_whitespace()
                    .last()
                    .and_then(|signature| returns.get(signature))
                    .copied()
                {
                    returns.insert(name.into(), width);
                }
            }
        }
    }

    let memset = text
        .lines()
        .filter_map(|line| {
            line.trim()
                .split_once(" = %Memset ")
                .map(|(name, _)| name.to_string())
        })
        .collect::<BTreeSet<_>>();
    let memcpy = text
        .lines()
        .filter_map(|line| {
            line.trim()
                .split_once(" = %Memcpy ")
                .map(|(name, _)| name.to_string())
        })
        .collect::<BTreeSet<_>>();
    let mut facts = JitByteFacts::default();
    facts
        .zero
        .entry((u32::MAX, 0))
        .or_default()
        .extend(0..initial_size);
    for line in text.lines() {
        if line.trim().starts_with("block") {
            if let Some((_, parameters)) = line.split_once('(') {
                for parameter in parameters.trim_end_matches("):").split(", ") {
                    if let Some((value, ty)) = parameter.split_once(": ") {
                        if let Some(width) = clif_width(ty) {
                            widths.insert(value.into(), width);
                        }
                    }
                }
            }
        }
        let (point, instruction) = if let Some(line) = line.trim().strip_prefix('@') {
            let (tag, instruction) = line
                .split_once(char::is_whitespace)
                .finding("source location")?;
            let tag = u32::from_str_radix(tag, 16).finding("source location number")?;
            (
                (tag / 65536 - 1, (tag % 65536) as usize),
                instruction.trim(),
            )
        } else {
            ((u32::MAX, 0), line.trim())
        };
        let code = instruction.split("  ;").next().finding("instruction")?;
        if let Some((value, expression)) = code.split_once(" = ") {
            let operation = expression.split_whitespace().next().unwrap_or("");
            let width = if operation.starts_with("icmp") || operation.starts_with("fcmp") {
                Some(1)
            } else if operation == "f32const" {
                Some(4)
            } else if operation == "f64const" {
                Some(8)
            } else if operation == "call" {
                expression
                    .strip_prefix("call ")
                    .and_then(|call| call.split_once('('))
                    .and_then(|(callee, _)| returns.get(callee))
                    .copied()
            } else {
                operation
                    .split_once('.')
                    .and_then(|(_, ty)| clif_width(ty))
                    .or_else(|| {
                        expression
                            .split_whitespace()
                            .nth(1)
                            .and_then(|operand| widths.get(operand.trim_end_matches(',')))
                            .copied()
                    })
            };
            if let Some(width) = width {
                widths.insert(value.into(), width);
            }
            if let Some(expression) = expression
                .strip_prefix("iadd_imm.i64 ")
                .or_else(|| expression.strip_prefix("iadd_imm "))
            {
                let (base, offset) = expression.split_once(", ").finding("address operands")?;
                if let Some(base) = addresses.get(base).copied() {
                    addresses.insert(
                        value.into(),
                        base + offset.parse::<i64>().finding("address offset")?,
                    );
                }
            }
            if expression.starts_with("iconst.") {
                if let Some(number) = expression.split_whitespace().last().and_then(|number| {
                    let number = number.replace('_', "");
                    match number.strip_prefix("0x") {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => number.parse::<u32>().ok(),
                    }
                }) {
                    constants.insert(value.into(), number);
                }
            }
            if expression.starts_with("uextend.") || expression.starts_with("ireduce.") {
                if let Some(number) = expression
                    .split_whitespace()
                    .last()
                    .and_then(|source| constants.get(source))
                    .copied()
                {
                    constants.insert(value.into(), number);
                }
            }
            if let Some(call) = expression.strip_prefix("call ") {
                if let Some((callee, arguments)) = call.split_once('(') {
                    if memcpy.contains(callee) {
                        let arguments = arguments
                            .trim_end_matches(')')
                            .split(", ")
                            .collect::<Vec<_>>();
                        if let [destination, _, size] = arguments.as_slice() {
                            if let (Some(offset), Some(size)) =
                                (addresses.get(*destination), constants.get(*size))
                            {
                                let offset = u32::try_from(*offset).finding("memcpy offset")?;
                                facts
                                    .operations
                                    .entry(point)
                                    .or_default()
                                    .push(ByteOperation {
                                        offset,
                                        size: *size,
                                        operation: Operation::Write,
                                    });
                                facts
                                    .positive
                                    .entry(point)
                                    .or_default()
                                    .extend(offset..offset + size);
                                let zeros = facts.zero.entry(point).or_default();
                                for byte in offset..offset + size {
                                    zeros.remove(&byte);
                                }
                            }
                        }
                    }
                    if memset.contains(callee) {
                        let arguments = arguments
                            .trim_end_matches(')')
                            .split(", ")
                            .collect::<Vec<_>>();
                        if let [destination, zero, size] = arguments.as_slice() {
                            if constants.get(*zero) == Some(&0) {
                                if let (Some(offset), Some(size)) =
                                    (addresses.get(*destination), constants.get(*size))
                                {
                                    let offset = u32::try_from(*offset).finding("memset offset")?;
                                    facts.operations.entry(point).or_default().push(
                                        ByteOperation {
                                            offset,
                                            size: *size,
                                            operation: Operation::Zero,
                                        },
                                    );
                                    facts
                                        .zero
                                        .entry(point)
                                        .or_default()
                                        .extend(offset..offset + size);
                                }
                            }
                        }
                    }
                }
            }
            if let Some(ty) = expression
                .strip_prefix("iconst.i")
                .and_then(|s| s.strip_suffix(" 0"))
            {
                zeros.insert(
                    value.into(),
                    ty.parse::<u32>().finding("constant type")? / 8,
                );
            }
        }
        if code.starts_with("store") {
            let Some((prefix, address)) = code.split_once(", ") else {
                continue;
            };
            let value = prefix.split_whitespace().last().finding("store value")?;
            let zero_size = zeros.get(value).copied();
            let size = prefix
                .split_whitespace()
                .next()
                .and_then(|operation| operation.split_once('.'))
                .and_then(|(_, ty)| clif_width(ty))
                .or_else(|| widths.get(value).copied())
                .or(zero_size)
                .unwrap_or(8);
            let (base, extra) = match address.split_once('+') {
                Some((base, offset)) => (base, offset.parse::<i64>().finding("store offset")?),
                None => (address, 0),
            };
            if let Some(offset) = addresses.get(base).copied() {
                let offset = u32::try_from(offset + extra).finding("frame offset")?;
                facts
                    .operations
                    .entry(point)
                    .or_default()
                    .push(ByteOperation {
                        offset,
                        size,
                        operation: if zero_size.is_some() {
                            Operation::Zero
                        } else {
                            Operation::Write
                        },
                    });
                let zeros = facts.zero.entry(point).or_default();
                if zero_size.is_some() {
                    zeros.extend(offset..offset + size);
                } else {
                    facts
                        .positive
                        .entry(point)
                        .or_default()
                        .extend(offset..offset + size);
                    for byte in offset..offset + size {
                        zeros.remove(&byte);
                    }
                }
            }
        }
    }
    Ok(facts)
}

fn clif_width(ty: &str) -> Option<u32> {
    let bits = ty
        .strip_prefix('i')
        .or_else(|| ty.strip_prefix('f'))?
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse::<u32>()
        .ok()?;
    Some((bits / 8).max(1))
}

/// Derive storage from LIR types and operations, without the native field plan.
type LirFields = (
    BTreeSet<String>,
    BTreeSet<String>,
    BTreeMap<l::BlockId, BTreeSet<String>>,
    root_storage::Interference,
);

fn lir_fields(
    function: &l::Function,
    layouts: &Layouts,
    environments: bool,
) -> Result<LirFields, String> {
    let mut all = BTreeSet::new();
    let mut lifetime = BTreeSet::new();
    let mut suspended = BTreeMap::new();
    for parameter in &function.parameters {
        let name = format!("p{}", parameter.value.0);
        all.insert(name.clone());
        if root_storage::managed_value_words(
            layouts,
            &function.values[parameter.value.0 as usize].ty,
        )
        .finding("parameter type")?
            > 0
        {
            lifetime.insert(name);
        }
    }
    for local in &function.locals {
        if local.storage == l::LocalStorageClass::Frame {
            let name = format!("l{}", local.id.0);
            all.insert(name.clone());
            if root_storage::managed_value_words(layouts, &local.ty).finding("local type")? > 0 {
                lifetime.insert(name);
            }
        }
    }
    // Independent stable-storage derivation: address-producing operations whose result is a suspend operand.
    let suspend_operands = function
        .blocks
        .iter()
        .flat_map(|block| match &block.terminator {
            l::Terminator::Suspend { arguments, .. } => arguments.as_slice(),
            _ => &[],
        })
        .filter_map(|operand| match operand {
            l::Operand::Value(value) => Some(*value),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
        if !matches!(
            instruction.kind,
            l::InstructionKind::AllocateClass(_) | l::InstructionKind::AddressOfValue
        ) {
            continue;
        }
        let Some(result) = instruction
            .result
            .filter(|result| suspend_operands.contains(result))
        else {
            continue;
        };
        if let l::ValueType::Address(address) = &function.values[result.0 as usize].ty {
            let name = format!("stable_v{}", result.0);
            all.insert(name.clone());
            if managed_words(layouts, &address.pointee).finding("stable type")? > 0 {
                lifetime.insert(name);
            }
        }
    }
    if environments {
        for value in &function.values {
            if matches!(&value.ty, l::ValueType::Data(ty) if ty.function_type().is_some()) {
                let name = format!("env_v{}", value.id.0);
                all.insert(name.clone());
                lifetime.insert(name);
            }
        }
    }
    let live = root_storage::value_interference(function).finding("shared live values")?;
    for block in &function.blocks {
        let l::Terminator::Suspend {
            successor,
            resume_value,
            arguments,
            kind,
            ..
        } = &block.terminator
        else {
            continue;
        };
        let mut fields = BTreeSet::new();
        for (parameter, argument) in function.blocks[successor.0 as usize]
            .parameters
            .iter()
            .skip(usize::from(resume_value.is_some()))
            .zip(arguments)
        {
            let name = format!("b{}_v{}", block.id.0, parameter.0);
            all.insert(name.clone());
            if root_storage::managed_value_words(layouts, &function.values[parameter.0 as usize].ty)
                .finding("spill type")?
                > 0
            {
                if let l::Operand::Value(value) = argument {
                    require!(
                        live.live_before(*value, block.id, block.instructions.len() + 1),
                        "spill outside shared live set"
                    );
                }
                fields.insert(name);
            }
        }
        if matches!(
            kind,
            l::SuspendKind::AsyncCall { .. } | l::SuspendKind::AsyncHandle { .. }
        ) {
            let name = format!("b{}_child", block.id.0);
            all.insert(name.clone());
            fields.insert(name);
        }
        suspended.insert(block.id, fields);
    }
    Ok((all, lifetime, suspended, live))
}

fn c_field_names(definition: &str) -> BTreeSet<String> {
    // The ABI checks header fields; close_output points to the caller's result storage.
    definition
        .lines()
        .filter_map(|line| {
            let line = line.trim().strip_suffix(';')?;
            let name = line.split_whitespace().last()?;
            (!matches!(
                name,
                "state"
                    | "reserved"
                    | "resume"
                    | "epoch"
                    | "holders"
                    | "padding"
                    | "cleanup"
                    | "close_output"
            ))
            .then(|| name.to_string())
        })
        .collect()
}

fn field_bytes(
    bytes: &BTreeMap<Point, BTreeSet<u32>>,
    fields: &[Field],
) -> BTreeMap<Point, BTreeSet<String>> {
    bytes
        .iter()
        .map(|(point, bytes)| {
            (
                *point,
                fields
                    .iter()
                    .filter(|field| {
                        (field.offset..field.offset + field.size).all(|byte| bytes.contains(&byte))
                    })
                    .map(|field| field.name.clone())
                    .collect(),
            )
        })
        .collect()
}

/// Propagate actual native writes through LIR edges. Restores retire the source suspension's fields.
fn suspension_fields(
    function: &l::Function,
    lifetime: &BTreeSet<String>,
    initial: &BTreeSet<String>,
    writes: &BTreeMap<Point, BTreeSet<String>>,
    clears: &BTreeMap<Point, BTreeSet<String>>,
    spills: &BTreeMap<l::BlockId, Vec<Field>>,
) -> BTreeMap<l::BlockId, BTreeSet<String>> {
    let mut incoming = vec![BTreeSet::new(); function.blocks.len()];
    incoming[function.entry.0 as usize] = initial.clone();
    let mut visited = vec![false; function.blocks.len()];
    let mut pending = vec![function.entry];
    let mut suspended = BTreeMap::new();
    while let Some(id) = pending.pop() {
        visited[id.0 as usize] = true;
        let block = &function.blocks[id.0 as usize];
        let mut occupied = incoming[id.0 as usize].clone();
        let mut successors = BTreeMap::<l::BlockId, BTreeSet<String>>::new();
        for point_index in 0..=block.instructions.len() + 1 {
            if let Some(handler) = point_index
                .checked_sub(1)
                .and_then(|index| block.instructions.get(index))
                .and_then(|instruction| instruction.handler())
            {
                successors
                    .entry(handler)
                    .or_default()
                    .extend(occupied.iter().cloned());
            }
            let point = (block.id.0, point_index);
            if let Some(fields) = writes.get(&point) {
                occupied.extend(fields.iter().cloned());
            }
            if let Some(fields) = clears.get(&point) {
                // Value assignments can write null into lifetime storage; that storage still belongs to the frame.
                for field in fields.difference(lifetime) {
                    occupied.remove(field);
                }
            }
        }
        if matches!(block.terminator, l::Terminator::Suspend { .. }) {
            suspended.insert(block.id, occupied.clone());
            for field in &spills[&block.id] {
                occupied.remove(&field.name);
            }
        }
        for successor in block.terminator.successors() {
            successors
                .entry(successor)
                .or_default()
                .extend(occupied.iter().cloned());
        }
        for (successor, fields) in successors {
            let next = &mut incoming[successor.0 as usize];
            let previous = next.len();
            next.extend(fields);
            if !visited[successor.0 as usize] || next.len() != previous {
                pending.push(successor);
            }
        }
    }
    suspended
}

fn equal_fields(expected: &BTreeSet<String>, actual: &BTreeSet<String>) -> Result<(), String> {
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "missing {:?}, outside {:?}",
            expected.difference(actual).collect::<Vec<_>>(),
            actual.difference(expected).collect::<Vec<_>>()
        ))
    }
}

/// Read the native plan that the JIT uses, independently of the LIR contract derivation.
type JitFields = (
    BTreeSet<String>,
    Vec<Field>,
    BTreeMap<l::BlockId, Vec<Field>>,
);

fn jit_fields(
    function: &l::Function,
    plan: &CoroutinePlan,
    layouts: &Layouts,
    environment_size: Option<u32>,
) -> Result<JitFields, String> {
    let mut all = BTreeSet::new();
    let mut lifetime = Vec::new();
    let mut spills = BTreeMap::new();
    let mut add = |name: String, slot: &FrameSlot, fields: &mut Vec<Field>| -> Result<(), String> {
        all.insert(name.clone());
        if root_storage::managed_value_words(layouts, &slot.ty).finding("field type")? > 0 {
            fields.push(Field {
                name,
                offset: slot.offset,
                size: value_size_align(layouts, &slot.ty).finding("field size")?.0,
            });
        }
        Ok(())
    };
    for (parameter, slot) in function.parameters.iter().zip(&plan.parameter_slots) {
        add(format!("p{}", parameter.value.0), slot, &mut lifetime)?;
    }
    for (index, slot) in plan.local_slots.iter().enumerate() {
        if let Some(slot) = slot {
            add(format!("l{index}"), slot, &mut lifetime)?;
        }
    }
    for (block, suspend) in &plan.suspends {
        let l::Terminator::Suspend {
            successor,
            resume_value,
            ..
        } = &function.blocks[block.0 as usize].terminator
        else {
            return Err("suspend plan".into());
        };
        let mut fields = Vec::new();
        for (parameter, slot) in function.blocks[successor.0 as usize]
            .parameters
            .iter()
            .skip(usize::from(resume_value.is_some()))
            .zip(&suspend.arguments)
        {
            add(format!("b{}_v{}", block.0, parameter.0), slot, &mut fields)?;
        }
        spills.insert(*block, fields);
    }
    for (block, suspend) in &plan.suspends {
        if let Some(offset) = suspend.child {
            let name = format!("b{}_child", block.0);
            all.insert(name.clone());
            spills
                .get_mut(block)
                .finding("suspend fields")?
                .push(Field {
                    name,
                    offset,
                    size: 8,
                });
        }
    }
    for (value, offset) in &plan.stable_addresses {
        let name = format!("stable_v{}", value.0);
        all.insert(name.clone());
        let l::ValueType::Address(address) = &function.values[value.0 as usize].ty else {
            return Err("stable address".into());
        };
        if managed_words(layouts, &address.pointee).finding("stable type")? > 0 {
            lifetime.push(Field {
                name,
                offset: *offset,
                size: layouts
                    .size_align(&address.pointee)
                    .finding("stable size")?
                    .0,
            });
        }
    }
    for (value, offset) in &plan.closure_environments {
        let name = format!("env_v{}", value.0);
        all.insert(name.clone());
        lifetime.push(Field {
            name,
            offset: *offset,
            size: environment_size.finding("environment layout")?,
        });
    }
    Ok((all, lifetime, spills))
}

#[derive(Default)]
struct EntryReport {
    id: String,
    findings: Vec<String>,
    counts: (usize, usize),
}

fn check_entry(
    sources: &[subscript_compiler::SourceFile],
    id: &str,
    counts: &mut (usize, usize),
) -> Result<(), String> {
    check_entry_with_c(sources, id, counts, &|_, _| Ok(()))
}

fn check_entry_with_c(
    sources: &[subscript_compiler::SourceFile],
    id: &str,
    counts: &mut (usize, usize),
    corrupt: &dyn Fn(&l::Module, &mut String) -> Result<(), String>,
) -> Result<(), String> {
    check_entry_with_forms(sources, id, counts, corrupt, &|_, _| Ok(()))
}

type NativeTexts = BTreeMap<String, String>;
type NativeMutation<'a> = dyn Fn(&l::Module, &mut NativeTexts) -> Result<(), String> + 'a;

fn check_entry_with_forms(
    sources: &[subscript_compiler::SourceFile],
    id: &str,
    counts: &mut (usize, usize),
    corrupt: &dyn Fn(&l::Module, &mut String) -> Result<(), String>,
    corrupt_jit: &NativeMutation<'_>,
) -> Result<(), String> {
    let hir = corpus::check_program(sources).finding("corpus HIR")?;
    let lir = crate::lir::lower_module(&hir).finding("corpus LIR")?;
    check_module_with_forms(&lir, id, counts, corrupt, corrupt_jit)
}

fn check_module_with_forms(
    lir: &l::Module,
    id: &str,
    counts: &mut (usize, usize),
    corrupt: &dyn Fn(&l::Module, &mut String) -> Result<(), String>,
    corrupt_jit: &NativeMutation<'_>,
) -> Result<(), String> {
    if !lir
        .functions
        .iter()
        .any(|function| function.is_async || function.is_generator)
    {
        return Ok(());
    }
    let layouts = Layouts::build_lir(lir).finding("layouts")?;
    let mut c = crate::cemit::emit_lir_c(lir, false).finding("C emission")?;
    corrupt(lir, &mut c.source)?;
    let isa = cranelift_native::builder()
        .finding("host ISA")?
        .finish(crate::lower::dev_flags().finding("flags")?)
        .finding("ISA flags")?;
    let mut native = cranelift_jit::JITModule::new(cranelift_jit::JITBuilder::with_isa(
        isa,
        cranelift_module::default_libcall_names(),
    ));
    let result = (|| -> Result<(), String> {
        let _ = take_defined_function_texts();
        crate::lower::lower_lir_module_with(
            &mut native,
            lir,
            crate::lower::LowerOptions {
                reload: true,
                ..Default::default()
            },
        )
        .finding("native emission")?;
        let mut texts = take_defined_function_texts()
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        corrupt_jit(lir, &mut texts)?;
        let environment_size = closure_environment_layout(lir, &layouts)
            .finding("environments")?
            .map(|(size, _)| size);
        for function in lir
            .functions
            .iter()
            .filter(|function| function.is_async || function.is_generator)
        {
            let plan = plan_coroutine(&layouts, lir, function).finding("JIT frame")?;
            let (expected_all, expected_lifetime, expected_suspensions, crossing) =
                lir_fields(function, &layouts, environment_size.is_some())?;
            let (jit_all, lifetime, spills) =
                jit_fields(function, &plan, &layouts, environment_size)?;
            let definition = c
                .source
                .split_once(&format!("typedef struct SubFrame{} {{", function.id.0))
                .finding("C frame")?
                .1
                .split_once(&format!("}} SubFrame{};", function.id.0))
                .finding("C frame end")?
                .0;
            let c_all = c_field_names(definition);
            equal_fields(&expected_all, &c_all)
                .map_err(|error| format!("{id} C inventory: {error}"))?;
            equal_fields(&expected_all, &jit_all)
                .map_err(|error| format!("{id} JIT inventory: {error}"))?;
            require_eq!(c_all, jit_all, "{id}: C/JIT field sets");
            let native_lifetime = lifetime
                .iter()
                .map(|field| field.name.clone())
                .collect::<BTreeSet<_>>();
            equal_fields(&expected_lifetime, &native_lifetime)
                .map_err(|error| format!("{id} lifetime: {error}"))?;
            let c_body = c_function(&c.source, function.id)?;
            let clif = texts
                .get(&format!("LIR coroutine resume {}", function.id.0))
                .finding("resume IR")?;
            let creator = texts
                .get(&format!("LIR coroutine creator {}", function.id.0))
                .finding("creator IR")?;
            let frame = creator
                .lines()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("return ")
                        .and_then(|line| line.split_whitespace().next())
                })
                .finding("creator frame")?;
            let creator_facts = jit_byte_facts(creator, frame, plan.size)?;
            let created = &creator_facts.zero;
            let c_creator = c.source.split_once(&format!("static uint8_t sub_f{}_resume(void* ctx, void* raw_frame, void* coroutine_out) {{", function.id.0))
                .finding("C resume definition")?.0.rsplit_once("    memset(frame, 0, sizeof *frame);")
                .finding("C factory initialization")?.1;
            for (parameter, slot) in function.parameters.iter().zip(&plan.parameter_slots) {
                if root_storage::managed_value_words(&layouts, &slot.ty)
                    .finding("parameter type")?
                    == 0
                {
                    continue;
                }
                require!(
                    c_creator.contains(&format!("frame->p{} =", parameter.value.0)),
                    "{id}: C parameter initialization"
                );
                let size = value_size_align(&layouts, &slot.ty)
                    .finding("parameter size")?
                    .0;
                require!(
                    !created
                        .get(&(u32::MAX, 0))
                        .is_some_and(|bytes| (slot.offset..slot.offset + size)
                            .all(|byte| bytes.contains(&byte))),
                    "{id}: JIT parameter initialization {}",
                    parameter.value.0
                );
            }
            let jit_facts = jit_byte_facts(clif, "v1", 0)?;
            let jit_bytes = &jit_facts.zero;
            let jit_writes = &jit_facts.positive;
            jit_finish_clears(function, clif, jit_bytes, plan.size)
                .map_err(|error| format!("{id}: {error}"))?;
            c_finish_clears(c_body, function.is_generator)
                .map_err(|error| format!("{id}: {error}"))?;
            let all_fields = lifetime
                .iter()
                .chain(spills.values().flatten())
                .cloned()
                .collect::<Vec<_>>();
            let resumes = function
                .blocks
                .iter()
                .filter_map(|block| match block.terminator {
                    l::Terminator::Suspend { successor, .. } => Some((block.id.0, successor)),
                    _ => None,
                })
                .collect();
            let c_facts = c_field_facts(c_body, &all_fields, Some(function.entry), &resumes)?;
            let c_zero = &c_facts.raw_clears;
            let jit_zero_fields = field_bytes(jit_bytes, &all_fields);
            let assignments = lifetime_assignments(function, &expected_lifetime);
            // Dispatch must retire every spill and child field, while it preserves all lifetime fields.
            for field in &lifetime {
                require!(
                    !c_zero
                        .get(&(u32::MAX, 0))
                        .is_some_and(|names| names.contains(&field.name)),
                    "{id}: initial C lifetime clear {}",
                    field.name
                );
                require!(
                    !jit_zero_fields
                        .get(&(u32::MAX, 0))
                        .is_some_and(|names| names.contains(&field.name)),
                    "{id}: initial JIT lifetime clear {}\n{clif}",
                    field.name
                );
            }
            let jit_ordered = jit_operations(function, &jit_facts.operations, &lifetime);
            ordered_clears(
                function,
                &expected_lifetime,
                &assignments,
                &c_facts.operations,
                &jit_ordered,
            )
            .map_err(|error| format!("{id}: {error}"))?;
            for field in spills.values().flatten() {
                require!(
                    c_zero
                        .get(&(u32::MAX, 0))
                        .is_some_and(|names| names.contains(&field.name)),
                    "{id}: C spill retirement {}",
                    field.name
                );
                require!(
                    jit_zero_fields
                        .get(&(u32::MAX, 0))
                        .is_some_and(|names| names.contains(&field.name)),
                    "{id}: JIT spill retirement {}",
                    field.name
                );
            }
            // compiler.md §338 rule 5: only native creator writes enter the native flow.
            let c_initial = c_writes(c_creator, &lifetime)?
                .into_values()
                .flatten()
                .collect();
            let jit_initial = field_bytes(&creator_facts.positive, &lifetime)
                .into_values()
                .flatten()
                .collect();
            let c_suspensions = suspension_fields(
                function,
                &native_lifetime,
                &c_initial,
                &c_facts.writes,
                &c_facts.clears,
                &spills,
            );
            let native_writes = jit_frame_writes(
                function,
                &native_lifetime,
                &all_fields,
                jit_writes,
                jit_bytes,
            )?;
            let jit_suspensions = suspension_fields(
                function,
                &native_lifetime,
                &jit_initial,
                &native_writes,
                &jit_zero_fields,
                &spills,
            );
            let initial = function
                .parameters
                .iter()
                .flat_map(|parameter| {
                    let p = format!("p{}", parameter.value.0);
                    let env = format!("env_v{}", parameter.value.0);
                    [p, env]
                        .into_iter()
                        .filter(|name| expected_lifetime.contains(name))
                })
                .collect();
            let definitions = function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .filter_map(|instruction| instruction.result.map(|result| (result, instruction)))
                .collect::<BTreeMap<_, _>>();
            let mut stored_lifetime = BTreeSet::new();
            for name in &expected_lifetime {
                if let Some(value) = name.strip_prefix("env_v") {
                    let value = l::ValueId(value.parse().finding("environment value")?);
                    if let Some(instruction) = definitions.get(&value) {
                        let needs_storage = match &instruction.kind {
                            l::InstructionKind::MakeClosure(target) => {
                                let target = &lir.functions[target.0 as usize];
                                target
                                    .parameters
                                    .iter()
                                    .any(|p| p.kind == l::ParameterKind::Capture)
                                    && !target
                                        .parameters
                                        .iter()
                                        .any(|p| p.kind == l::ParameterKind::OwnedEnvironment)
                            }
                            l::InstructionKind::Copy | l::InstructionKind::Cast => true,
                            _ => false,
                        };
                        if !needs_storage {
                            continue;
                        }
                    }
                }
                stored_lifetime.insert(name.clone());
            }
            let writes = assignments
                .iter()
                .map(|(point, names)| {
                    (
                        *point,
                        names.intersection(&stored_lifetime).cloned().collect(),
                    )
                })
                .collect();
            let needed = suspension_fields(
                function,
                &expected_lifetime,
                &initial,
                &writes,
                &BTreeMap::new(),
                &spills,
            );
            let mut required_lifetime = BTreeSet::new();
            for (block, fields) in &needed {
                for name in fields {
                    let value = name
                        .strip_prefix("env_v")
                        .or_else(|| name.strip_prefix("stable_v"))
                        .and_then(|id| id.parse::<u32>().ok())
                        .map(l::ValueId);
                    if value.is_none_or(|value| {
                        function.parameters.iter().any(|p| p.value == value)
                            || crossing.live_before(
                                value,
                                *block,
                                function.blocks[block.0 as usize].instructions.len() + 1,
                            )
                    }) {
                        required_lifetime.insert(name.clone());
                    }
                }
            }
            assignment_writes(
                function,
                &required_lifetime,
                &writes,
                &c_facts.writes,
                false,
            )
            .map_err(|error| format!("{id}: {error}"))?;
            assignment_writes(function, &required_lifetime, &writes, &native_writes, true)
                .map_err(|error| format!("{id}: {error}"))?;
            for (block, expected) in &expected_suspensions {
                let c_set = c_suspensions.get(block).finding("reachable C suspension")?;
                let jit_set = jit_suspensions
                    .get(block)
                    .finding("reachable JIT suspension")?;
                let mut required = expected.clone();
                for name in needed.get(block).into_iter().flatten() {
                    let value = name
                        .strip_prefix("env_v")
                        .or_else(|| name.strip_prefix("stable_v"))
                        .and_then(|id| id.parse::<u32>().ok())
                        .map(l::ValueId);
                    if value.is_none_or(|value| {
                        function.parameters.iter().any(|p| p.value == value)
                            || crossing.live_before(
                                value,
                                *block,
                                function.blocks[block.0 as usize].instructions.len() + 1,
                            )
                    }) {
                        required.insert(name.clone());
                    }
                }
                let relevant = |actual: &BTreeSet<String>| {
                    actual
                        .iter()
                        .filter(|name| required.contains(*name) || !native_lifetime.contains(*name))
                        .cloned()
                        .collect::<BTreeSet<_>>()
                };
                let c_set = relevant(c_set);
                let jit_set = relevant(jit_set);
                equal_fields(&required, &c_set).map_err(|error| {
                    format!("{id} {} b{} C: {error}", function.source_name, block.0)
                })?;
                equal_fields(&required, &jit_set).map_err(|error| {
                    format!("{id} {} b{} JIT: {error}", function.source_name, block.0)
                })?;
                require_eq!(c_set, jit_set, "{id}: suspension field sets");
                counts.1 += 1;
            }
            counts.0 += 1;
        }
        Ok(())
    })();
    // SAFETY: no generated function address escapes this entry.
    unsafe { native.free_memory() };
    result
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("non-text panic payload")
}

fn run_pool<T: Sync>(
    entries: &[(String, T)],
    check: impl Fn(&T, &str, &mut (usize, usize)) -> Result<(), String> + Sync,
) -> Vec<EntryReport> {
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(entries.len());
    let mut indexed = std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|worker| {
                let check = &check;
                scope.spawn(move || {
                    entries
                        .iter()
                        .enumerate()
                        .skip(worker)
                        .step_by(workers)
                        .map(|(index, (id, sources))| {
                            let mut report = EntryReport {
                                id: id.clone(),
                                ..Default::default()
                            };
                            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                check(sources, id, &mut report.counts)
                            })) {
                                Ok(Ok(())) => {}
                                Ok(Err(text)) => report.findings.push(text),
                                Err(payload) => report
                                    .findings
                                    .push(format!("panic: {}", panic_text(payload.as_ref()))),
                            }
                            (index, report)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("frame-root worker"))
            .collect::<Vec<_>>()
    });
    indexed.sort_by_key(|(index, _)| *index);
    indexed
        .into_iter()
        .map(|(_, report)| report)
        .collect::<Vec<_>>()
}

fn assert_reports(reports: &[EntryReport]) {
    let findings = reports
        .iter()
        .flat_map(|report| {
            report
                .findings
                .iter()
                .map(|text| format!("{}: {text}", report.id))
        })
        .collect::<Vec<_>>();
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}

// compiler.md §338 rule 5: debug best of three: 8.38 s; 0 C builds.
// One native lowering per entry compares independent LIR, C, and JIT facts.
// Each assignment needs a native write; each native return needs its own finish check.
// Ordered operations distinguish initialization before a write from a clear after it.
// Width-aware JIT reads prove actual writes; edge and point flows reject missing writes and premature clears.
// One C pass supplies raw and resume views; one JIT pass records zero and positive bytes.
#[test]
fn every_corpus_coroutine_field_matches_live_values_at_suspension() {
    // §279: the test thread loads corpus sources and interop mirrors before the pool starts.
    // Workers read these immutable sources; each entry owns its compiler state.
    // The compiler owns its checker state. Its test-only captures are absent in this dependency build.
    // LIR operation_table::TABLE and array_ownership::CLASSES use OnceLock and contain immutable tables.
    // LIR operation_table::ROW_COPIES and lower::func::DEFINED_FUNCTION_TEXTS are thread-local.
    // Each entry clears and takes DEFINED_FUNCTION_TEXTS on its worker thread.
    // Native ISA detection reads host features; dev_flags builds fresh flags and uses no environment overrides.
    // Each entry owns its JIT module. No generated code runs, so runtime hooks and output caches are unused.
    let root = corpus::corpus_accept();
    let trap_root = root.parent().expect("corpus root").join("trap");
    let entries = corpus::entry_ids(&root)
        .into_iter()
        .map(|id| (root.clone(), id))
        .chain(
            corpus::entry_ids(&trap_root)
                .into_iter()
                .map(|id| (trap_root.clone(), id)),
        )
        .map(|(directory, id)| {
            let sources = corpus::entry_sources(&directory, &id);
            (id, sources)
        })
        .collect::<Vec<_>>();
    let reports = run_pool(&entries, |sources, id, counts| {
        check_entry(sources, id, counts)
    });
    let (functions, suspensions) = reports.iter().fold((0, 0), |total, report| {
        (total.0 + report.counts.0, total.1 + report.counts.1)
    });
    eprintln!(
        "frame parity: {functions} coroutines, {suspensions} suspension points, both native tiers"
    );
    assert_reports(&reports);
}

#[test]
fn entry_finding_fails_with_its_id() {
    check_pool_failure(false);
}

#[test]
fn entry_panic_fails_with_its_id() {
    check_pool_failure(true);
}

fn check_pool_failure(panics: bool) {
    // §279: one entry fails through the same pool path as the corpus check.
    let entries = (0..17)
        .map(|id| (format!("control-{id}"), ()))
        .collect::<Vec<_>>();
    let reports = run_pool(&entries, |_, id, _| {
        if id == "control-1" {
            if panics {
                panic!("injected panic");
            }
            return Err("injected finding".into());
        }
        Ok(())
    });
    assert_eq!(reports.len(), entries.len());
    for (report, (id, _)) in reports.iter().zip(&entries) {
        assert_eq!(&report.id, id);
        assert_eq!(report.findings.len(), usize::from(id == "control-1"));
    }
    let failure = std::panic::catch_unwind(|| assert_reports(&reports))
        .expect_err("entry failure must fail the test");
    let expected = if panics {
        "panic: injected panic"
    } else {
        "injected finding"
    };
    assert!(panic_text(failure.as_ref()).contains(&format!("control-1: {expected}")));
}

// compiler.md §338 rule 5: debug best of three: <0.01 s; 0 C builds.
#[test]
fn hand_built_native_storage_rejects_extra_and_missing_roots() {
    let files = [subscript_compiler::SourceFile::new("storage.ts", 
        "class Box { n:i32; constructor(n:i32){this.n=n;} } async function tick():Promise<void>{} async function work(h:Box):Promise<void>{await tick();} export async function main():Promise<void>{await work(new Box(7));}")];
    let lir = crate::lir::lower_module(
        &subscript_compiler::check_program(&files).expect("checked fixture"),
    )
    .expect("LIR fixture");
    let function = lir
        .functions
        .iter()
        .find(|function| function.source_name == "work")
        .expect("work function");
    let layouts = Layouts::build_lir(&lir).expect("fixture layouts");
    let (_, expected, _, _) = lir_fields(function, &layouts, false).expect("fixture fields");
    assert_eq!(expected, BTreeSet::from(["p0".to_string()]));
    for definition in ["void* p0;\nvoid* stray;", ""] {
        assert!(equal_fields(&expected, &c_field_names(definition)).is_err());
    }
    equal_fields(&expected, &c_field_names("void* p0;")).expect("valid C storage");
    // Hand-built native forms write reference words after the sixteen-byte header.
    let fields = [
        Field {
            name: "p0".into(),
            offset: 16,
            size: 8,
        },
        Field {
            name: "stray".into(),
            offset: 24,
            size: 8,
        },
    ];
    for (text, valid) in [
        (
            "v2 = iconst.i64 0\n@10001 store v3, v1+16\n@10001 store v2, v1+24",
            true,
        ),
        ("@10001 store v3, v1+16\n@10001 store v3, v1+24", false),
        (
            "v2 = iconst.i64 0\n@10001 store v2, v1+16\n@10001 store v2, v1+24",
            false,
        ),
    ] {
        let bytes = jit_bytes_with_frame(text, "v1", 0, true).expect("fixture writes");
        let actual = fields
            .iter()
            .filter(|field| {
                bytes.get(&(0, 1)).is_some_and(|bytes| {
                    (field.offset..field.offset + field.size).all(|byte| bytes.contains(&byte))
                })
            })
            .map(|field| field.name.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(equal_fields(&expected, &actual).is_ok(), valid);
    }
    let narrow =
        jit_bytes_with_frame("v2 = iconst.i32 7\n@10001 store v2, v1+16", "v1", 0, true).unwrap();
    assert_eq!(narrow[&(0, 1)], (16..20).collect());
}

fn payload_offset(function: &l::Function) -> u32 {
    if function.is_generator {
        // The generator ABI places the payload after its 40-byte header.
        40
    } else {
        COROUTINE_PAYLOAD_OFFSET
    }
}

// compiler.md §338 rule 5: derive assignment points independently from native operations.
fn lifetime_assignments(
    function: &l::Function,
    lifetime: &BTreeSet<String>,
) -> BTreeMap<Point, BTreeSet<String>> {
    let mut assignments = BTreeMap::<Point, BTreeSet<String>>::new();
    let mut value = |id: l::ValueId, point| {
        for prefix in ["env_v", "stable_v"] {
            if let Some(name) = lifetime.get(&format!("{prefix}{}", id.0)) {
                assignments.entry(point).or_default().insert(name.clone());
            }
        }
    };
    for parameter in &function.parameters {
        value(parameter.value, (function.entry.0, 0));
    }
    for block in &function.blocks {
        for parameter in &block.parameters {
            value(*parameter, (block.id.0, 0));
        }
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let Some(result) = instruction.result {
                value(result, (block.id.0, index + 1));
            }
        }
        if !matches!(block.terminator, l::Terminator::Suspend { .. }) {
            for target in block.terminator.targets() {
                for (parameter, _) in function.blocks[target.block.0 as usize]
                    .parameters
                    .iter()
                    .zip(&target.arguments)
                {
                    value(*parameter, (block.id.0, block.instructions.len() + 1));
                }
            }
        }
    }
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let l::InstructionKind::StoreLocal(local) = instruction.kind {
                if let Some(name) = lifetime.get(&format!("l{}", local.0)) {
                    assignments
                        .entry((block.id.0, index + 1))
                        .or_default()
                        .insert(name.clone());
                }
            }
        }
    }
    assignments
}

fn lifetime_assignment(
    function: &l::Function,
    name: &str,
    block: &l::BasicBlock,
    point: usize,
) -> Result<bool, String> {
    Ok(
        lifetime_assignments(function, &BTreeSet::from([name.to_string()]))
            .get(&(block.id.0, point))
            .is_some_and(|names| names.contains(name)),
    )
}

// compiler.md §338 rule 5: each completed exit must clear its payload on that path.
fn c_finish_clears(source: &str, generator: bool) -> Result<(), String> {
    let marker = if generator {
        "memset((unsigned char*)frame + 40"
    } else {
        "memset((unsigned char*)frame + sizeof frame->state"
    };
    let mut cleared = false;
    let mut already_done = false;
    for (index, line) in source.lines().enumerate() {
        let line = line.trim();
        if line.ends_with(':') {
            cleared = false;
            already_done = line == "coroutine_done:";
        }
        if line.contains(marker) {
            cleared = true;
        }
        if line == "return 1;" {
            require!(
                cleared || already_done,
                "C finish exit without payload clear at line {}",
                index + 1
            );
            cleared = false;
        }
    }
    Ok(())
}

fn jit_frame_writes(
    function: &l::Function,
    lifetime: &BTreeSet<String>,
    fields: &[Field],
    positive: &BTreeMap<Point, BTreeSet<u32>>,
    zero: &BTreeMap<Point, BTreeSet<u32>>,
) -> Result<BTreeMap<Point, BTreeSet<String>>, String> {
    let mut initialized = positive.clone();
    for (point, bytes) in zero {
        initialized.entry(*point).or_default().extend(bytes);
    }
    let writes = field_bytes(&initialized, fields);
    // compiler.md §338 rule 5: each incoming edge supplies a fresh parameter value.
    // A shared receiving-block store covers every edge; a predecessor store covers only that edge.
    for source in &function.blocks {
        if matches!(source.terminator, l::Terminator::Suspend { .. }) {
            continue;
        }
        for target in source.terminator.targets() {
            for parameter in &function.blocks[target.block.0 as usize].parameters {
                let name = format!("env_v{}", parameter.0);
                if !lifetime.contains(&name) {
                    continue;
                }
                let entry = (target.block.0, 0);
                let edge = (source.id.0, source.instructions.len() + 1);
                require!(
                    writes
                        .get(&entry)
                        .is_some_and(|fields| fields.contains(&name))
                        || writes
                            .get(&edge)
                            .is_some_and(|fields| fields.contains(&name)),
                    "JIT edge b{} -> b{} missing write {name}",
                    source.id.0,
                    target.block.0
                );
            }
        }
    }
    Ok(writes)
}

mod negative_tests;

// compiler.md §338 rule 5: each LIR assignment requires its own native write.
fn assignment_writes(
    function: &l::Function,
    required: &BTreeSet<String>,
    assignments: &BTreeMap<Point, BTreeSet<String>>,
    writes: &BTreeMap<Point, BTreeSet<String>>,
    jit: bool,
) -> Result<(), String> {
    let parameters = function
        .blocks
        .iter()
        .flat_map(|target| {
            target
                .parameters
                .iter()
                .map(move |p| (format!("env_v{}", p.0), (target.id.0, 0)))
        })
        .collect::<BTreeMap<_, _>>();
    for (point, names) in assignments {
        for name in names.intersection(required) {
            // C assigns block parameters on incoming edges, rather than at block entry.
            if !jit && point.1 == 0 && parameters.get(name) == Some(point) {
                continue;
            }
            let at_point = writes
                .get(point)
                .is_some_and(|fields| fields.contains(name));
            let receiving = jit
                && parameters.get(name).is_some_and(|entry| {
                    (point == entry
                        || point.1 == function.blocks[point.0 as usize].instructions.len() + 1
                            && function.blocks[point.0 as usize]
                                .terminator
                                .targets()
                                .iter()
                                .any(|edge| edge.block.0 == entry.0))
                        && writes
                            .get(entry)
                            .is_some_and(|fields| fields.contains(name))
                });
            let incoming = jit && parameters.get(name) == Some(point) && {
                let edges = function
                    .blocks
                    .iter()
                    .filter(|source| {
                        source
                            .terminator
                            .successors()
                            .iter()
                            .any(|target| target.0 == point.0)
                    })
                    .collect::<Vec<_>>();
                !edges.is_empty()
                    && edges.iter().all(|source| {
                        !matches!(source.terminator, l::Terminator::Suspend { .. })
                            && writes
                                .get(&(source.id.0, source.instructions.len() + 1))
                                .is_some_and(|fields| fields.contains(name))
                    })
            };
            require!(
                at_point || receiving || incoming,
                "{} assignment b{}:{} missing write {name}",
                if jit { "JIT" } else { "C" },
                point.0,
                point.1
            );
        }
    }
    Ok(())
}

// compiler.md §338 rule 5: return and unwind exits have distinct test-only source tags.
fn jit_finish_clears(
    function: &l::Function,
    text: &str,
    zero: &BTreeMap<Point, BTreeSet<u32>>,
    size: u32,
) -> Result<(), String> {
    let expected = function
        .blocks
        .iter()
        .filter(|block| matches!(block.terminator, l::Terminator::Return { .. }))
        .map(|block| (block.id.0, 65534))
        .collect::<BTreeSet<_>>();
    let mut exits = BTreeSet::new();
    let mut done = 0;
    for line in text.lines() {
        let line = line.trim();
        let (point, code) = if let Some(tagged) = line.strip_prefix('@') {
            let (tag, code) = tagged
                .split_once(char::is_whitespace)
                .finding("return tag")?;
            let tag = u32::from_str_radix(tag, 16).finding("return tag number")?;
            (Some((tag / 65536 - 1, (tag % 65536) as usize)), code.trim())
        } else {
            (None, line)
        };
        if !code.starts_with("return ") {
            continue;
        }
        match point {
            None => done += 1,
            Some(point) if expected.contains(&point) || point == (65532, 1) => {
                exits.insert(point);
            }
            Some((block, point))
                if function.blocks.get(block as usize).is_some_and(|block| {
                    matches!(block.terminator, l::Terminator::Suspend { .. })
                        && point == block.instructions.len() + 1
                }) => {}
            Some(point) => return Err(format!("JIT unrecognized return at {point:?}")),
        }
    }
    require_eq!(
        done,
        1,
        "JIT already-done dispatch must be the only untagged return"
    );
    require!(expected.is_subset(&exits), "JIT missing tagged return exit");
    for exit in exits {
        require!(
            size == payload_offset(function)
                || zero.get(&exit).is_some_and(
                    |bytes| (payload_offset(function)..size).all(|byte| bytes.contains(&byte))
                ),
            "JIT finish exit {exit:?} without payload clear"
        );
    }
    Ok(())
}
