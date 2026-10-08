//! §183: scalar host exports agree with their declarations and the dev tier.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::Instant;

use subscript_boundary::{Extension, Kind, Leaf, KINDS};
use subscript_codegen::{
    add_c11_optimized_flags, add_executable_output, add_object_directory, emit_c_without_main,
    host_c_compiler, host_entry, runtime_staticlib_path, runtime_system_libraries,
    tool_output_report, EntryArg, ReloadSession,
};
use subscript_compiler::{check_program, SourceFile};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("s183-host-exports-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// The record selects the witnesses. No separate kind list controls coverage.
fn witnesses(kind: &Kind) -> [(&'static str, EntryArg, &'static str); 2] {
    match (kind.leaf, kind.extension, kind.size) {
        _ if kind.c_type == "bool" => [
            ("true", EntryArg::Bool(true), "true"),
            ("false", EntryArg::Bool(false), "false"),
        ],
        (Leaf::Half, _, _) => [
            ("0x3e00", EntryArg::F16(0x3e00), "1.5"),
            ("0xc100", EntryArg::F16(0xc100), "-2.5"),
        ],
        (Leaf::Float, _, _) => [
            ("1.5f", EntryArg::F32(1.5), "1.5"),
            ("-2.5f", EntryArg::F32(-2.5), "-2.5"),
        ],
        (Leaf::Double, _, _) => [
            ("1.5", EntryArg::F64(1.5), "1.5"),
            ("-2.5", EntryArg::F64(-2.5), "-2.5"),
        ],
        (_, Extension::Signed, 1) => [
            ("-31", EntryArg::I8(-31), "-31"),
            ("17", EntryArg::I8(17), "17"),
        ],
        (_, Extension::Unsigned, 1) => [
            ("3", EntryArg::U8(3), "3"),
            ("201", EntryArg::U8(201), "201"),
        ],
        (_, Extension::Signed, 2) => [
            ("-31", EntryArg::I16(-31), "-31"),
            ("17", EntryArg::I16(17), "17"),
        ],
        (_, Extension::Unsigned, 2) => [
            ("3", EntryArg::U16(3), "3"),
            ("65535", EntryArg::U16(65535), "65535"),
        ],
        (Leaf::Integer, _, 4) if kind.c_type.starts_with('u') => [
            ("3", EntryArg::U32(3), "3"),
            ("4294967295U", EntryArg::U32(4294967295), "4294967295"),
        ],
        (Leaf::Integer, _, 4) => [
            ("-31", EntryArg::I32(-31), "-31"),
            ("17", EntryArg::I32(17), "17"),
        ],
        (Leaf::Integer, _, 8) if kind.c_type.starts_with('u') => [
            ("3", EntryArg::U64(3), "3"),
            (
                "18446744073709551615ULL",
                EntryArg::U64(18446744073709551615),
                "18446744073709551615",
            ),
        ],
        (Leaf::Integer, _, 8) => [
            ("-31", EntryArg::I64(-31), "-31"),
            ("17", EntryArg::I64(17), "17"),
        ],
        other => panic!("add witnesses for {}: {other:?}", kind.language),
    }
}

fn compile(directory: &Scratch, syntax_only: bool) -> Output {
    let compiler = host_c_compiler().unwrap();
    let mut command = compiler.command();
    add_c11_optimized_flags(&mut command, compiler.style());
    add_object_directory(&mut command, &directory.0, compiler.style());
    command.arg(directory.0.join("program.c"));
    if syntax_only {
        command.arg(if compiler.style().is_msvc() {
            "/Zs"
        } else {
            "-fsyntax-only"
        });
    } else {
        command
            .arg(directory.0.join("host.c"))
            .arg(runtime_staticlib_path().unwrap())
            .args(runtime_system_libraries(compiler.style()));
        add_executable_output(
            &mut command,
            &directory
                .0
                .join(format!("host{}", std::env::consts::EXE_SUFFIX)),
            compiler.style(),
        );
    }
    command.output().unwrap()
}

#[test]
fn every_boundary_scalar_export_matches_dev_and_c_checks_a_wrong_definition() {
    // Cost: one module per tier, one C link/run, and one rejected C syntax check.
    let start = Instant::now();
    let mut script = String::new();
    let mut calls = String::new();
    for kind in KINDS {
        script.push_str(&format!(
            "export function take_{}(value: {}): void {{ print(`{}=${{value}}`); }}\n",
            kind.language, kind.language, kind.language
        ));
        for (value, _, _) in witnesses(kind) {
            calls.push_str(&format!(
                "    subscript_export_take_{}(ctx, {value});\n",
                kind.language
            ));
        }
    }
    let files = [SourceFile::entry("exports.ts", script)];
    let module = check_program(&files).unwrap();
    let program = emit_c_without_main(&module).unwrap();
    let directory = Scratch::new();
    std::fs::write(directory.0.join("program.h"), &program.host_header).unwrap();
    std::fs::write(directory.0.join("program.c"), &program.source).unwrap();
    let host = host_entry(
        r#"
#include "program.h"
#include <stdio.h>
int main(void) {
    subscript_rt_context* ctx = subscript_rt_ctx_new();
    if (!ctx) return 1;
    subscript_rt_ctx_enter_script(ctx);
    subscript_init(ctx);
CALLS
    subscript_rt_ctx_exit_script(ctx);
    uint64_t length = 0;
    const uint8_t* output = subscript_rt_ctx_stdout(ctx, &length);
    fwrite(output, 1, length, stdout);
    int result = subscript_rt_ctx_trap_kind(ctx) != 0;
    subscript_rt_ctx_release(ctx);
    return result;
}
"#,
        &program.host_header,
    )
    .unwrap()
    .replace("CALLS", &calls);
    std::fs::write(directory.0.join("host.c"), host).unwrap();
    let compiled = compile(&directory, false);
    assert!(
        compiled.status.success(),
        "boundary export build: {}",
        tool_output_report(&compiled)
    );
    let output = Command::new(
        directory
            .0
            .join(format!("host{}", std::env::consts::EXE_SUFFIX)),
    )
    .output()
    .unwrap();
    assert!(output.status.success(), "{}", tool_output_report(&output));

    let mut dev = ReloadSession::new(&files).unwrap();
    let mut expected = Vec::new();
    for kind in KINDS {
        for (_, argument, _) in witnesses(kind) {
            dev.call_export_with(&format!("take_{}", kind.language), &[argument])
                .unwrap();
        }
        expected.extend(dev.take_output());
    }
    let expected_lines = expected
        .split_inclusive(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    let actual_lines = output
        .stdout
        .split_inclusive(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    assert_eq!(
        actual_lines.len(),
        KINDS.len() * 2,
        "C host prints two values per kind"
    );
    assert_eq!(
        expected_lines.len(),
        actual_lines.len(),
        "dev prints two values per kind"
    );
    for (index, kind) in KINDS.iter().enumerate() {
        for (witness_index, (_, _, printed)) in witnesses(kind).into_iter().enumerate() {
            let literal = format!("{}={printed}\n", kind.language);
            let line_index = index * 2 + witness_index;
            assert_eq!(
                actual_lines[line_index],
                literal.as_bytes(),
                "{} C host output differs from the literal",
                kind.language
            );
            assert_eq!(
                expected_lines[line_index],
                literal.as_bytes(),
                "{} dev output differs from the literal",
                kind.language
            );
        }
        assert_eq!(
            &actual_lines[index * 2..index * 2 + 2],
            &expected_lines[index * 2..index * 2 + 2],
            "{} host export differs between C AOT and dev",
            kind.language
        );
    }

    // Change the definition only. The unchanged host declaration rejects it.
    let control = KINDS.iter().find(|kind| kind.c_type == "bool").unwrap();
    let correct = format!(
        "void subscript_export_take_{}(subscript_rt_context* ctx, {} a0) {{",
        control.language, control.c_type
    );
    let wrong = correct.replace(&format!(", {} a0", control.c_type), ", int32_t a0");
    assert_eq!(program.source.matches(&correct).count(), 1);
    std::fs::write(
        directory.0.join("program.c"),
        program.source.replacen(&correct, &wrong, 1),
    )
    .unwrap();
    let rejected = compile(&directory, true);
    let diagnostic = tool_output_report(&rejected);
    assert!(
        !rejected.status.success(),
        "{} wrong wrapper type escaped the C check",
        control.language
    );
    assert!(
        diagnostic.contains(&format!("subscript_export_take_{}", control.language)),
        "{} control lacks the export diagnostic: {diagnostic}",
        control.language
    );
    eprintln!(
        "{} firing control: rejected wrong wrapper type",
        control.language
    );
    eprintln!(
        "§183: {} kinds, {} values, sweep and control: {:.6} s",
        KINDS.len(),
        KINDS.len() * 2,
        start.elapsed().as_secs_f64()
    );
}
