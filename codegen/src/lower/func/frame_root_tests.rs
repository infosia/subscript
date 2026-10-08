//! Native field writes are checked against the shared LIR live sets.
//! One native lowering per coroutine corpus entry; no script runs twice.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

// This test uses only the source discovery functions of the shared corpus helper.
#[allow(dead_code)]
#[path = "../../../tests/corpus/mod.rs"]
mod corpus;

type Point = (u32, usize);

#[derive(Clone, Debug)]
struct Field {
    name: String,
    offset: u32,
    size: u32,
}

fn c_function(source: &str, id: l::FunctionId) -> &str {
    let marker = format!(
        "static uint8_t sub_f{}_resume(void* ctx, void* raw_frame, void* coroutine_out) {{",
        id.0
    );
    source
        .split_once(&marker)
        .expect("C resume body")
        .1
        .split("\n}\n")
        .next()
        .expect("C function end")
}

fn c_clears(source: &str, fields: &[Field]) -> BTreeMap<Point, BTreeSet<String>> {
    let mut point = (u32::MAX, 0);
    let mut clears = BTreeMap::<_, BTreeSet<_>>::new();
    for line in source.lines() {
        if let Some(marker) = line.trim().strip_prefix("// root-point ") {
            let mut parts = marker.split_whitespace();
            point = (
                parts.next().expect("block").parse().expect("block number"),
                parts.next().expect("point").parse().expect("point number"),
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
    clears
}

/// Read actual zero stores, including the byte width of each stored constant.
fn jit_zero_bytes(text: &str) -> BTreeMap<Point, BTreeSet<u32>> {
    jit_bytes_with_frame(text, "v1", 0, false)
}

fn jit_bytes_with_frame(
    text: &str,
    frame: &str,
    initial_size: u32,
    nonzero: bool,
) -> BTreeMap<Point, BTreeSet<u32>> {
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
                .expect("source location");
            let tag = u32::from_str_radix(tag, 16).expect("source location number");
            (
                (tag / 65536 - 1, (tag % 65536) as usize),
                instruction.trim(),
            )
        } else {
            ((u32::MAX, 0), line.trim())
        };
        let code = instruction.split("  ;").next().expect("instruction");
        if let Some((value, expression)) = code.split_once(" = ") {
            if let Some(expression) = expression
                .strip_prefix("iadd_imm.i64 ")
                .or_else(|| expression.strip_prefix("iadd_imm "))
            {
                let (base, offset) = expression.split_once(", ").expect("address operands");
                if let Some(base) = addresses.get(base).copied() {
                    addresses.insert(
                        value.into(),
                        base + offset.parse::<i64>().expect("address offset"),
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
                                    let offset = u32::try_from(*offset).expect("memset offset");
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
                zeros.insert(value.into(), ty.parse::<u32>().expect("constant type") / 8);
            }
        }
        if code.starts_with("store") {
            let Some((prefix, address)) = code.split_once(", ") else {
                continue;
            };
            let value = prefix.split_whitespace().last().expect("store value");
            let zero_size = zeros.get(value).copied();
            let size = zero_size.unwrap_or(8);
            let (base, extra) = match address.split_once('+') {
                Some((base, offset)) => (base, offset.parse::<i64>().expect("store offset")),
                None => (address, 0),
            };
            if let Some(offset) = addresses.get(base).copied() {
                let offset = u32::try_from(offset + extra).expect("frame offset");
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
    bytes
}

/// Derive storage from LIR types and operations, without the native field plan.
fn lir_fields(
    function: &l::Function,
    layouts: &Layouts,
    environments: bool,
) -> (
    BTreeSet<String>,
    BTreeSet<String>,
    BTreeMap<l::BlockId, BTreeSet<String>>,
) {
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
        .expect("parameter type")
            > 0
        {
            lifetime.insert(name);
        }
    }
    for local in &function.locals {
        if local.storage == l::LocalStorageClass::Frame {
            let name = format!("l{}", local.id.0);
            all.insert(name.clone());
            if root_storage::managed_value_words(layouts, &local.ty).expect("local type") > 0 {
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
            if managed_words(layouts, &address.pointee).expect("stable type") > 0 {
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
    let live = root_storage::value_interference(function).expect("shared live values");
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
                .expect("spill type")
                > 0
            {
                if let l::Operand::Value(value) = argument {
                    assert!(
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
    (all, lifetime, suspended)
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

fn c_writes(source: &str, fields: &[Field]) -> BTreeMap<Point, BTreeSet<String>> {
    let mut point = (u32::MAX, 0);
    let mut writes = BTreeMap::<Point, BTreeSet<String>>::new();
    for line in source.lines() {
        if let Some(marker) = line.trim().strip_prefix("// root-point ") {
            let mut parts = marker.split_whitespace();
            point = (
                parts.next().expect("block").parse().expect("block number"),
                parts.next().expect("point").parse().expect("point number"),
            );
        }
        for field in fields {
            if line.contains(&format!("frame->{} =", field.name)) {
                writes.entry(point).or_default().insert(field.name.clone());
            }
        }
    }
    writes
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
fn jit_fields(
    function: &l::Function,
    plan: &CoroutinePlan,
    layouts: &Layouts,
    environment_size: Option<u32>,
) -> (
    BTreeSet<String>,
    Vec<Field>,
    BTreeMap<l::BlockId, Vec<Field>>,
) {
    let mut all = BTreeSet::new();
    let mut lifetime = Vec::new();
    let mut spills = BTreeMap::new();
    let mut add = |name: String, slot: &FrameSlot, fields: &mut Vec<Field>| {
        all.insert(name.clone());
        if root_storage::managed_value_words(layouts, &slot.ty).expect("field type") > 0 {
            fields.push(Field {
                name,
                offset: slot.offset,
                size: value_size_align(layouts, &slot.ty).expect("field size").0,
            });
        }
    };
    for (parameter, slot) in function.parameters.iter().zip(&plan.parameter_slots) {
        add(format!("p{}", parameter.value.0), slot, &mut lifetime);
    }
    for (index, slot) in plan.local_slots.iter().enumerate() {
        if let Some(slot) = slot {
            add(format!("l{index}"), slot, &mut lifetime);
        }
    }
    for (block, suspend) in &plan.suspends {
        let l::Terminator::Suspend {
            successor,
            resume_value,
            ..
        } = &function.blocks[block.0 as usize].terminator
        else {
            panic!("suspend plan");
        };
        let mut fields = Vec::new();
        for (parameter, slot) in function.blocks[successor.0 as usize]
            .parameters
            .iter()
            .skip(usize::from(resume_value.is_some()))
            .zip(&suspend.arguments)
        {
            add(format!("b{}_v{}", block.0, parameter.0), slot, &mut fields);
        }
        spills.insert(*block, fields);
    }
    for (block, suspend) in &plan.suspends {
        if let Some(offset) = suspend.child {
            let name = format!("b{}_child", block.0);
            all.insert(name.clone());
            spills.get_mut(block).expect("suspend fields").push(Field {
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
            panic!("stable address");
        };
        if managed_words(layouts, &address.pointee).expect("stable type") > 0 {
            lifetime.push(Field {
                name,
                offset: *offset,
                size: layouts.size_align(&address.pointee).expect("stable size").0,
            });
        }
    }
    for (value, offset) in &plan.closure_environments {
        let name = format!("env_v{}", value.0);
        all.insert(name.clone());
        lifetime.push(Field {
            name,
            offset: *offset,
            size: environment_size.expect("environment layout"),
        });
    }
    (all, lifetime, spills)
}

#[test]
fn every_corpus_coroutine_field_matches_live_values_at_suspension() {
    let root = corpus::corpus_accept();
    let trap_root = root.parent().expect("corpus root").join("trap");
    let entries = corpus::entry_ids(&root)
        .into_iter()
        .map(|id| (root.clone(), id))
        .chain(
            corpus::entry_ids(&trap_root)
                .into_iter()
                .map(|id| (trap_root.clone(), id)),
        );
    let mut functions = 0;
    let mut suspensions = 0;
    for (directory, id) in entries {
        let sources = corpus::entry_sources(&directory, &id);
        let hir = subscript_compiler::check_program(&sources)
            .unwrap_or_else(|error| panic!("{id}: {error:?}"));
        let lir = crate::lir::lower_module(&hir).expect("corpus LIR");
        if !lir
            .functions
            .iter()
            .any(|function| function.is_async || function.is_generator)
        {
            continue;
        }
        let layouts = Layouts::build_lir(&lir).expect("layouts");
        let c = crate::cemit::emit_lir_c(&lir, false).expect("C emission");
        let isa = cranelift_native::builder()
            .expect("host ISA")
            .finish(crate::lower::dev_flags().expect("flags"))
            .expect("ISA flags");
        let mut native = cranelift_jit::JITModule::new(cranelift_jit::JITBuilder::with_isa(
            isa,
            cranelift_module::default_libcall_names(),
        ));
        let _ = take_defined_function_texts();
        crate::lower::lower_lir_module_with(
            &mut native,
            &lir,
            crate::lower::LowerOptions {
                reload: true,
                ..Default::default()
            },
        )
        .expect("native emission");
        let texts = take_defined_function_texts();
        for function in lir
            .functions
            .iter()
            .filter(|function| function.is_async || function.is_generator)
        {
            let plan = plan_coroutine(&layouts, &lir, function).expect("JIT frame");
            let environment_size = closure_environment_layout(&lir, &layouts)
                .expect("environments")
                .map(|(size, _)| size);
            let (expected_all, expected_lifetime, expected_suspensions) =
                lir_fields(function, &layouts, environment_size.is_some());
            let (jit_all, lifetime, spills) =
                jit_fields(function, &plan, &layouts, environment_size);
            let definition = c
                .source
                .split_once(&format!("typedef struct SubFrame{} {{", function.id.0))
                .expect("C frame")
                .1
                .split_once(&format!("}} SubFrame{};", function.id.0))
                .expect("C frame end")
                .0;
            let c_all = c_field_names(definition);
            equal_fields(&expected_all, &c_all)
                .unwrap_or_else(|error| panic!("{id} C inventory: {error}"));
            equal_fields(&expected_all, &jit_all)
                .unwrap_or_else(|error| panic!("{id} JIT inventory: {error}"));
            assert_eq!(c_all, jit_all, "{id}: C/JIT field sets");
            let native_lifetime = lifetime
                .iter()
                .map(|field| field.name.clone())
                .collect::<BTreeSet<_>>();
            equal_fields(&expected_lifetime, &native_lifetime)
                .unwrap_or_else(|error| panic!("{id} lifetime: {error}"));
            let c_body = c_function(&c.source, function.id);
            let clif = &texts
                .iter()
                .find(|(name, _)| name == &format!("LIR coroutine resume {}", function.id.0))
                .expect("resume IR")
                .1;
            let creator = &texts
                .iter()
                .find(|(name, _)| name == &format!("LIR coroutine creator {}", function.id.0))
                .expect("creator IR")
                .1;
            let frame = creator
                .lines()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("return ")
                        .and_then(|line| line.split_whitespace().next())
                })
                .expect("creator frame");
            let created = jit_bytes_with_frame(creator, frame, plan.size, false);
            let c_creator = c.source.split_once(&format!("static uint8_t sub_f{}_resume(void* ctx, void* raw_frame, void* coroutine_out) {{", function.id.0))
                .expect("C resume definition").0.rsplit_once("    memset(frame, 0, sizeof *frame);")
                .expect("C factory initialization").1;
            for (parameter, slot) in function.parameters.iter().zip(&plan.parameter_slots) {
                if root_storage::managed_value_words(&layouts, &slot.ty).expect("parameter type")
                    == 0
                {
                    continue;
                }
                assert!(
                    c_creator.contains(&format!("frame->p{} =", parameter.value.0)),
                    "{id}: C parameter initialization"
                );
                let size = value_size_align(&layouts, &slot.ty)
                    .expect("parameter size")
                    .0;
                assert!(
                    !created
                        .get(&(u32::MAX, 0))
                        .is_some_and(|bytes| (slot.offset..slot.offset + size)
                            .all(|byte| bytes.contains(&byte))),
                    "{id}: JIT parameter initialization {}",
                    parameter.value.0
                );
            }
            let jit_bytes = jit_zero_bytes(clif);
            let jit_writes = jit_bytes_with_frame(clif, "v1", 0, true);
            assert!(
                plan.size == payload_offset(function)
                    || jit_bytes
                        .get(&(65534, 65534))
                        .is_some_and(|bytes| (payload_offset(function)..plan.size)
                            .all(|byte| bytes.contains(&byte))),
                "{id}: finished JIT payload clear"
            );
            assert!(
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
            let c_zero = c_clears(c_body, &all_fields);
            // Dispatch must retire every spill and child field, while it preserves all lifetime fields.
            for field in &lifetime {
                assert!(
                    !c_zero
                        .get(&(u32::MAX, 0))
                        .is_some_and(|names| names.contains(&field.name)),
                    "{id}: initial C lifetime clear {}",
                    field.name
                );
                assert!(
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
                            let id = l::ValueId(id.parse().expect("field value ID"));
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
                            field.name.strip_prefix('l').is_some_and(|local| {
                                let local = l::LocalId(local.parse().expect("local field ID"));
                                point_index
                                    .checked_sub(1)
                                    .and_then(|index| block.instructions.get(index))
                                    .is_some_and(|instruction| {
                                        matches!(instruction.kind,
                                        l::InstructionKind::StoreLocal(target) if target == local)
                                    })
                            })
                        };
                        if assigned {
                            continue;
                        }
                        assert!(
                            !c_zero
                                .get(&point)
                                .is_some_and(|names| names.contains(&field.name)),
                            "{id}: C lifetime clear {} at {point:?}",
                            field.name
                        );
                        assert!(
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
                assert!(
                    c_body.contains(&c_clear),
                    "{id}: C spill retirement {}",
                    field.name
                );
                assert!(
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
                &c_writes(c_body, &all_fields),
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
                let c_set = c_suspensions.get(block).expect("reachable C suspension");
                let jit_set = jit_suspensions
                    .get(block)
                    .expect("reachable JIT suspension");
                equal_fields(expected, c_set)
                    .unwrap_or_else(|error| panic!("{id} b{} C: {error}", block.0));
                equal_fields(expected, jit_set)
                    .unwrap_or_else(|error| panic!("{id} b{} JIT: {error}", block.0));
                assert_eq!(c_set, jit_set, "{id}: suspension field sets");
                suspensions += 1;
            }
            functions += 1;
        }
        // SAFETY: no generated function address escapes the test.
        unsafe { native.free_memory() };
    }
    eprintln!(
        "frame parity: {functions} coroutines, {suspensions} suspension points, both native tiers"
    );
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
    let (_, expected, _) = lir_fields(function, &layouts, false);
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
        let bytes = jit_bytes_with_frame(text, "v1", 0, true);
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
