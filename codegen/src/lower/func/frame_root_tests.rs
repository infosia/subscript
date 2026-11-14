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

fn c_clears(source: &str, fields: &[Field]) -> Result<BTreeMap<Point, BTreeSet<String>>, String> {
    let mut point = (u32::MAX, 0);
    let mut clears = BTreeMap::<_, BTreeSet<_>>::new();
    for line in source.lines() {
        if let Some(marker) = line.trim().strip_prefix("// root-point ") {
            let mut parts = marker.split_whitespace();
            point = (
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
        }
        for field in fields {
            if (line.trim().starts_with("memset(")
                && line.contains(&format!("memset(&frame->{}, 0,", field.name)))
                || line.trim() == format!("frame->{} = NULL;", field.name)
            {
                clears.entry(point).or_default().insert(field.name.clone());
            } else if line.contains(&format!("&frame->{}", field.name))
                && (line.contains("memcpy(") || line.contains("->c"))
                || line.contains(&format!("frame->{} =", field.name))
            {
                clears.entry(point).or_default().remove(&field.name);
            }
        }
    }
    Ok(clears)
}

/// Read actual zero stores, including the byte width of each stored constant.
fn jit_zero_bytes(text: &str) -> Result<BTreeMap<Point, BTreeSet<u32>>, String> {
    jit_bytes_with_frame(text, "v1", 0, false)
}

fn jit_bytes_with_frame(
    text: &str,
    frame: &str,
    initial_size: u32,
    nonzero: bool,
) -> Result<BTreeMap<Point, BTreeSet<u32>>, String> {
    let mut addresses = BTreeMap::<String, i64>::from([(frame.into(), 0)]);
    let mut zeros = BTreeMap::<String, u32>::new();
    let mut constants = BTreeMap::<String, u32>::new();
    let memset = text
        .lines()
        .filter_map(|line| {
            line.trim()
                .split_once(" = %Memset ")
                .map(|(name, _)| name.to_string())
        })
        .collect::<BTreeSet<_>>();
    let mut bytes = BTreeMap::<_, BTreeSet<_>>::new();
    bytes
        .entry((u32::MAX, 0))
        .or_default()
        .extend(0..if nonzero { 0 } else { initial_size });
    for line in text.lines() {
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
                                    let written = bytes.entry(point).or_default();
                                    if nonzero {
                                        for byte in offset..offset + size {
                                            written.remove(&byte);
                                        }
                                    } else {
                                        written.extend(offset..offset + size);
                                    }
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
            let size = zero_size.unwrap_or(8);
            let (base, extra) = match address.split_once('+') {
                Some((base, offset)) => (base, offset.parse::<i64>().finding("store offset")?),
                None => (address, 0),
            };
            if let Some(offset) = addresses.get(base).copied() {
                let offset = u32::try_from(offset + extra).finding("frame offset")?;
                let written = bytes.entry(point).or_default();
                for byte in offset..offset + size {
                    if zero_size.is_some() != nonzero {
                        written.insert(byte);
                    } else {
                        written.remove(&byte);
                    }
                }
            }
        }
    }
    Ok(bytes)
}

/// Derive storage from LIR types and operations, without the native field plan.
type LirFields = (
    BTreeSet<String>,
    BTreeSet<String>,
    BTreeMap<l::BlockId, BTreeSet<String>>,
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
        let mut fields = lifetime.clone();
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
    Ok((all, lifetime, suspended))
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

fn c_writes(source: &str, fields: &[Field]) -> Result<BTreeMap<Point, BTreeSet<String>>, String> {
    let mut point = (u32::MAX, 0);
    let mut writes = BTreeMap::<Point, BTreeSet<String>>::new();
    for line in source.lines() {
        if let Some(marker) = line.trim().strip_prefix("// root-point ") {
            let mut parts = marker.split_whitespace();
            point = (
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
        }
        for field in fields {
            if line.contains(&format!("frame->{} =", field.name)) {
                writes.entry(point).or_default().insert(field.name.clone());
            }
        }
    }
    Ok(writes)
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
    writes: &BTreeMap<Point, BTreeSet<String>>,
    clears: &BTreeMap<Point, BTreeSet<String>>,
    spills: &BTreeMap<l::BlockId, Vec<Field>>,
) -> BTreeMap<l::BlockId, BTreeSet<String>> {
    let mut incoming = vec![BTreeSet::new(); function.blocks.len()];
    incoming[function.entry.0 as usize] = lifetime.clone();
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
    let hir = corpus::check_program(sources).finding("corpus HIR")?;
    let lir = crate::lir::lower_module(&hir).finding("corpus LIR")?;
    if !lir
        .functions
        .iter()
        .any(|function| function.is_async || function.is_generator)
    {
        return Ok(());
    }
    let layouts = Layouts::build_lir(&lir).finding("layouts")?;
    let c = crate::cemit::emit_lir_c(&lir, false).finding("C emission")?;
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
            &lir,
            crate::lower::LowerOptions {
                reload: true,
                ..Default::default()
            },
        )
        .finding("native emission")?;
        let texts = take_defined_function_texts();
        for function in lir
            .functions
            .iter()
            .filter(|function| function.is_async || function.is_generator)
        {
            let plan = plan_coroutine(&layouts, &lir, function).finding("JIT frame")?;
            let environment_size = closure_environment_layout(&lir, &layouts)
                .finding("environments")?
                .map(|(size, _)| size);
            let (expected_all, expected_lifetime, expected_suspensions) =
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
            let clif = &texts
                .iter()
                .find(|(name, _)| name == &format!("LIR coroutine resume {}", function.id.0))
                .finding("resume IR")?
                .1;
            let creator = &texts
                .iter()
                .find(|(name, _)| name == &format!("LIR coroutine creator {}", function.id.0))
                .finding("creator IR")?
                .1;
            let frame = creator
                .lines()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("return ")
                        .and_then(|line| line.split_whitespace().next())
                })
                .finding("creator frame")?;
            let created = jit_bytes_with_frame(creator, frame, plan.size, false)?;
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
            let jit_bytes = jit_zero_bytes(clif)?;
            let jit_writes = jit_bytes_with_frame(clif, "v1", 0, true)?;
            require!(
                plan.size == payload_offset(function)
                    || jit_bytes
                        .get(&(65534, 65534))
                        .is_some_and(|bytes| (payload_offset(function)..plan.size)
                            .all(|byte| bytes.contains(&byte))),
                "{id}: finished JIT payload clear"
            );
            require!(
                c_body.contains(if function.is_generator {
                    "memset((unsigned char*)frame + 40"
                } else {
                    "memset((unsigned char*)frame + sizeof frame->state"
                }),
                "{id}: finished C payload clear"
            );
            let all_fields = lifetime
                .iter()
                .chain(spills.values().flatten())
                .cloned()
                .collect::<Vec<_>>();
            let c_zero = c_clears(c_body, &all_fields)?;
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
                    !jit_bytes
                        .get(&(u32::MAX, 0))
                        .is_some_and(|bytes| (field.offset..field.offset + field.size)
                            .all(|byte| bytes.contains(&byte))),
                    "{id}: initial JIT lifetime clear {}\n{clif}",
                    field.name
                );
            }
            // An interval clear at any earlier LIR point must also fail the suspension contract.
            for block in &function.blocks {
                let last_point = block.instructions.len()
                    + usize::from(matches!(block.terminator, l::Terminator::Suspend { .. }));
                for point_index in 0..=last_point {
                    let point = (block.id.0, point_index);
                    for field in &lifetime {
                        let assigned = if let Some(id) = field
                            .name
                            .strip_prefix("env_v")
                            .or_else(|| field.name.strip_prefix("stable_v"))
                        {
                            let id = l::ValueId(id.parse().finding("field value ID")?);
                            function
                                .blocks
                                .iter()
                                .any(|block| block.parameters.contains(&id))
                                || point_index
                                    .checked_sub(1)
                                    .and_then(|index| block.instructions.get(index))
                                    .is_some_and(|instruction| instruction.result == Some(id))
                        } else {
                            // A local assignment can replace its value, including a null value.
                            if let Some(local) = field.name.strip_prefix('l') {
                                let local = l::LocalId(local.parse().finding("local field ID")?);
                                point_index
                                    .checked_sub(1)
                                    .and_then(|index| block.instructions.get(index))
                                    .is_some_and(|instruction| {
                                        matches!(instruction.kind,
                                        l::InstructionKind::StoreLocal(target) if target == local)
                                    })
                            } else {
                                false
                            }
                        };
                        if assigned {
                            continue;
                        }
                        require!(
                            !c_zero
                                .get(&point)
                                .is_some_and(|names| names.contains(&field.name)),
                            "{id}: C lifetime clear {} at {point:?}",
                            field.name
                        );
                        require!(
                            !jit_bytes
                                .get(&point)
                                .is_some_and(|bytes| (field.offset..field.offset + field.size)
                                    .all(|byte| bytes.contains(&byte))),
                            "{id}: JIT lifetime clear {} at {point:?}",
                            field.name
                        );
                    }
                }
            }
            for field in spills.values().flatten() {
                let c_clear = if field.name.ends_with("_child") {
                    format!("frame->{} = NULL;", field.name)
                } else {
                    format!("memset(&frame->{}, 0,", field.name)
                };
                require!(
                    c_body.contains(&c_clear),
                    "{id}: C spill retirement {}",
                    field.name
                );
                require!(
                    jit_bytes
                        .get(&(u32::MAX, 0))
                        .is_some_and(|bytes| (field.offset..field.offset + field.size)
                            .all(|byte| bytes.contains(&byte))),
                    "{id}: JIT spill retirement {}",
                    field.name
                );
            }
            let c_suspensions = suspension_fields(
                function,
                &native_lifetime,
                &c_writes(c_body, &all_fields)?,
                &c_zero,
                &spills,
            );
            let jit_suspensions = suspension_fields(
                function,
                &native_lifetime,
                &field_bytes(&jit_writes, &all_fields),
                &field_bytes(&jit_bytes, &all_fields),
                &spills,
            );
            for (block, expected) in &expected_suspensions {
                let c_set = c_suspensions.get(block).finding("reachable C suspension")?;
                let jit_set = jit_suspensions
                    .get(block)
                    .finding("reachable JIT suspension")?;
                equal_fields(expected, c_set)
                    .map_err(|error| format!("{id} b{} C: {error}", block.0))?;
                equal_fields(expected, jit_set)
                    .map_err(|error| format!("{id} b{} JIT: {error}", block.0))?;
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
    let (_, expected, _) = lir_fields(function, &layouts, false).expect("fixture fields");
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
}

fn payload_offset(function: &l::Function) -> u32 {
    if function.is_generator {
        // The generator ABI places the payload after its 40-byte header.
        40
    } else {
        COROUTINE_PAYLOAD_OFFSET
    }
}
