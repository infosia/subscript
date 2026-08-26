//! Deterministic, human-readable rendering of LIR.
//!
//! This module is deliberately presentation-only. It reads an already-built
//! module and never supplies defaults, validates facts, or decides semantics.

use std::fmt::Write;

use crate::lir::{self, Operand};

/// Renders a complete LIR module as stable review text.
#[must_use]
pub fn print_module(module: &lir::Module) -> String {
    let mut out = String::new();
    writeln!(&mut out, "module initializer={:?}", module.initializer).unwrap();

    for class in &module.classes {
        writeln!(
            &mut out,
            "class c{} {:?} value={} descriptor={} boundary={} align={:?} @ {}",
            class.id.0,
            class.source_name,
            class.is_value,
            class.is_descriptor,
            class.is_boundary,
            class.alignment,
            class.pos
        )
        .unwrap();
        if let Some(index) = &class.index_signature {
            writeln!(
                &mut out,
                "  index {:?} -> {:?} readonly={}",
                index.index_type, index.element_type, index.readonly
            )
            .unwrap();
        }
        for field in &class.fields {
            writeln!(
                &mut out,
                "  field d{} {:?}: {:?} defaulted={} absence={} foreign={:?} @ {}",
                field.id.0,
                field.source_name,
                field.ty,
                field.is_defaulted,
                field.is_absence_capable,
                field.foreign_provenance,
                field.pos
            )
            .unwrap();
        }
        for method in class.constructor.iter().chain(&class.methods) {
            writeln!(
                &mut out,
                "  method m{} {:?} -> f{}",
                method.id.0, method.source_name, method.function.0
            )
            .unwrap();
        }
    }
    for enumeration in &module.enums {
        writeln!(
            &mut out,
            "enum e{} {:?} {:?} @ {}",
            enumeration.id.0, enumeration.source_name, enumeration.members, enumeration.pos
        )
        .unwrap();
    }
    for alias in &module.string_aliases {
        writeln!(
            &mut out,
            "string-alias s{} {:?} members={:?} wire={:?} absence={} @ {}",
            alias.id.0,
            alias.source_name,
            alias.members,
            alias.wire_values,
            alias.absence_discriminant,
            alias.pos
        )
        .unwrap();
    }
    for global in &module.globals {
        writeln!(
            &mut out,
            "global g{} {:?}: {:?} mutable={} @ {}",
            global.id.0, global.source_name, global.ty, global.mutable, global.pos
        )
        .unwrap();
    }
    for foreign in &module.foreign_functions {
        writeln!(
            &mut out,
            "foreign x{} {:?} -> {:?} include={:?} @ {}",
            foreign.id.0, foreign.source_name, foreign.return_type, foreign.include, foreign.pos
        )
        .unwrap();
        for parameter in &foreign.parameters {
            writeln!(
                &mut out,
                "  param {:?}: {:?} foreign={:?} @ {}",
                parameter.source_name, parameter.ty, parameter.foreign_provenance, parameter.pos
            )
            .unwrap();
        }
    }
    for worker in &module.worker_entries {
        writeln!(
            &mut out,
            "worker f{} input=c{} output=c{}",
            worker.function.0, worker.input.0, worker.output.0
        )
        .unwrap();
    }
    for intrinsic in &module.intrinsic_operations {
        writeln!(
            &mut out,
            "intrinsic {:?}.{} {:?}",
            intrinsic.family, intrinsic.operation, intrinsic.semantic_name
        )
        .unwrap();
    }

    for function in &module.functions {
        writeln!(
            &mut out,
            "fn f{} {:?} kind={:?} exported={} generator={} async={} -> {:?} entry=b{} @ {}",
            function.id.0,
            function.source_name,
            function.kind,
            function.exported,
            function.is_generator,
            function.is_async,
            function.return_type,
            function.entry.0,
            function.pos
        )
        .unwrap();
        if !function.creation_traps.is_empty() {
            writeln!(&mut out, "  creation-traps {:?}", function.creation_traps).unwrap();
        }
        for parameter in &function.parameters {
            writeln!(
                &mut out,
                "  param %{} {:?}: {} kind={:?} storage={:?} @ {}",
                parameter.value.0,
                parameter.source_name,
                value_type(function, parameter.value),
                parameter.kind,
                parameter.storage,
                parameter.pos
            )
            .unwrap();
        }
        for local in &function.locals {
            writeln!(
                &mut out,
                "  local l{} {:?}: {:?} mutable={} @ {}",
                local.id.0, local.source_name, local.ty, local.mutable, local.pos
            )
            .unwrap();
        }
        for value in &function.values {
            writeln!(
                &mut out,
                "  value %{}: {:?} name={:?}",
                value.id.0, value.ty, value.source_name
            )
            .unwrap();
        }
        for block in &function.blocks {
            write!(&mut out, "  b{}", block.id.0).unwrap();
            if !block.parameters.is_empty() {
                write!(&mut out, "(").unwrap();
                for (index, parameter) in block.parameters.iter().enumerate() {
                    if index != 0 {
                        write!(&mut out, ", ").unwrap();
                    }
                    write!(
                        &mut out,
                        "%{}: {}",
                        parameter.0,
                        value_type(function, *parameter)
                    )
                    .unwrap();
                }
                write!(&mut out, ")").unwrap();
            }
            writeln!(&mut out, " {:?}:", block.source_name).unwrap();
            for instruction in &block.instructions {
                write!(&mut out, "    ").unwrap();
                if let Some(result) = instruction.result {
                    write!(
                        &mut out,
                        "%{}: {} = ",
                        result.0,
                        value_type(function, result)
                    )
                    .unwrap();
                }
                write!(&mut out, "{:?}", instruction.kind).unwrap();
                write_operands(&mut out, &instruction.operands);
                if !instruction.invalidates.is_empty() {
                    write!(&mut out, " invalidates={:?}", instruction.invalidates).unwrap();
                }
                if !instruction.traps.is_empty() {
                    write!(&mut out, " traps={:?}", instruction.traps).unwrap();
                }
                writeln!(&mut out, " @ {}", instruction.pos).unwrap();
            }
            writeln!(&mut out, "    -> {:?}", block.terminator).unwrap();
        }
    }
    out
}

fn value_type(function: &lir::Function, value: lir::ValueId) -> String {
    function
        .values
        .get(value.0 as usize)
        .filter(|entry| entry.id == value)
        .map_or_else(
            || "<missing>".to_string(),
            |entry| format!("{:?}", entry.ty),
        )
}

fn write_operands(out: &mut String, operands: &[Operand]) {
    write!(out, "(").unwrap();
    for (index, operand) in operands.iter().enumerate() {
        if index != 0 {
            write!(out, ", ").unwrap();
        }
        match operand {
            Operand::Value(value) => write!(out, "%{}", value.0).unwrap(),
            Operand::Constant(constant) => {
                write!(out, "{:?}:{:?}", constant.kind, constant.ty).unwrap()
            }
        }
    }
    write!(out, ")").unwrap();
}
