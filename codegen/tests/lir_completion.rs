//! §178 LIR gates. Each gate lowers one small module, without native compilation.

use subscript_codegen::interpreter::{interpret, InterpretError};
use subscript_codegen::lir::{lower_module, verify_module};
use subscript_compiler::{check_program, lir, lir_text::print_module, SourceFile, Type};

const MIRROR: &str = r#"// @subscript-c-header include="completion.h"
// @subscript-c-completion function="read" result="int32_t"
// @subscript-c-completion function="pair" result="Pair"
// @subscript-c-completion function="done" result="void"
declare class Pair { x: i32; y: f64; }
declare function read(value: i32): Promise<i32>;
declare function pair(): Promise<Pair>;
declare function done(): Promise<void>;
declare function ordinary(): void;
// @subscript-c-completion function="parameters" result="void"
// @subscript-c-string-view function="parameters" parameter="text" aggregate="View"
// @subscript-c-scalar-pair function="parameters" parameter="values" element="int32_t" const=true
declare function parameters(text: string, values: i32[]): Promise<void>;
"#;
const SCRIPT: &str = r#"export async function main(): Promise<void> {
    const scalar = await read(7);
    const aggregate = await pair();
    await done();
    print(`${scalar}:${aggregate.x}`);
}"#;

const FOREIGN_SCRIPT: &str = "export function main(): void { ordinary(); }";

const PARAMETER_SCRIPT: &str = "export async function main(): Promise<void> { const xs: i32[] = [1, 2]; await parameters(\"x\", xs); }";

fn module_for(script: &str) -> lir::Module {
    let hir = check_program(&[
        SourceFile::ambient("completion.d.ts", MIRROR),
        SourceFile::new("completion.ts", script),
    ])
    .expect("checker");
    lower_module(&hir).expect("lower")
}

fn module() -> lir::Module {
    module_for(SCRIPT)
}

#[test]
fn completion_form_carries_scalar_struct_void_layout_and_error_metadata() {
    let module = module();
    verify_module(&module).expect("verified");
    let calls: Vec<_> = module
        .functions
        .iter()
        .flat_map(|f| {
            f.blocks
                .iter()
                .flat_map(move |b| b.instructions.iter().map(move |i| (f, i)))
        })
        .filter(|(_, i)| matches!(i.kind, lir::InstructionKind::HostCompletion { .. }))
        .collect();
    assert_eq!(calls.len(), 3);
    for (function_body, instruction) in calls {
        let lir::InstructionKind::HostCompletion {
            function,
            result_size,
            is_void,
            error_metadata,
        } = &instruction.kind
        else {
            unreachable!()
        };
        let foreign = &module.foreign_functions[function.0 as usize];
        let expected = match foreign.source_name.as_str() {
            "read" => (4, false, 1),
            "pair" => (16, false, 0),
            "done" => (0, true, 0),
            _ => unreachable!(),
        };
        assert_eq!(
            (*result_size, *is_void, instruction.operands.len()),
            expected
        );
        let error = module
            .classes
            .iter()
            .find(|c| c.source_name == "Error")
            .expect("Error");
        assert_eq!(*error_metadata, [24, error.id.0 as u64, 0, 8, 16, 0]);
        assert_eq!(foreign.return_type, Type::Void);
        assert_eq!(
            foreign
                .parameters
                .last()
                .expect("endpoint")
                .foreign_provenance,
            Some(lir::ForeignTypeProvenance::CompletionEndpoint)
        );
        let owner = &function_body.values[instruction.result.expect("result").0 as usize];
        assert!(owner.fresh_owner);
        assert!(instruction.kind.produces_fresh_async_owner());
    }
    let text = print_module(&module);
    assert!(text.contains("HostCompletion"));
    assert!(text.contains("result_size: 16"));
    assert!(text.contains("CompletionEndpoint"));
}

#[test]
fn verifier_rejects_a_foreign_declaration_without_an_endpoint() {
    let mut module = module();
    verify_module(&module).expect("same-shape control");
    // Construct a C declaration with no endpoint. Keep the instruction unchanged.
    module.foreign_functions[0].parameters.pop();
    let errors = verify_module(&module).expect_err("invalid C declaration");
    assert!(
        errors.iter().any(|e| e
            .message
            .contains("requires a trailing by-value subscript_rt_completion parameter")),
        "{errors:?}"
    );
}

#[test]
fn interpreter_rejects_completion_with_the_foreign_unsupported_path() {
    for (script, name) in [(SCRIPT, "read"), (FOREIGN_SCRIPT, "ordinary")] {
        let module = module_for(script);
        let error = interpret(&module).expect_err("native call");
        let InterpretError::Execution { source, .. } = error else {
            panic!("execution error: {error:?}");
        };
        assert!(
            matches!(*source, InterpretError::Unsupported { ref reason } if reason == &format!("{name} requires a native library")),
            "{source:?}"
        );
    }
}

#[path = "../../compiler/tests/support/tsc.rs"]
mod tsc;

#[test]
fn stock_tsc_checks_lir_inputs_with_the_prelude() {
    // One stock tsc process independently checks all executable source shapes.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("root");
    let dir = std::env::temp_dir().join(format!(
        "subscript-lir-completion-tsc-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("directory");
    let mut files = vec![root.join("prelude/lang.d.ts")];
    for (index, script) in [SCRIPT, FOREIGN_SCRIPT, PARAMETER_SCRIPT]
        .iter()
        .enumerate()
    {
        let path = dir.join(format!("case{index}.ts"));
        std::fs::write(&path, format!("{MIRROR}\n{script}")).expect("source");
        files.push(path);
    }
    let config = dir.join("tsconfig.json");
    std::fs::write(&config, tsc::tsconfig(&files)).expect("config");
    let output = std::process::Command::new(tsc::tsc_binary(root))
        .arg("--project")
        .arg(&config)
        .output()
        .expect("tsc");
    std::fs::remove_dir_all(&dir).expect("cleanup");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn verifier_checks_void_flag_metadata_and_operands() {
    let valid = module();
    verify_module(&valid).expect("valid control");
    for (case, message) in [
        (1, "result is invalid"),
        (2, "Error metadata disagrees"),
        (3, "operand types disagree"),
    ] {
        let mut invalid = valid.clone();
        let instruction = invalid
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
            .find(|i| {
                matches!(
                    i.kind,
                    lir::InstructionKind::HostCompletion {
                        function: lir::ForeignFunctionId(0),
                        ..
                    }
                )
            })
            .expect("scalar form");
        if let lir::InstructionKind::HostCompletion {
            is_void,
            error_metadata,
            ..
        } = &mut instruction.kind
        {
            match case {
                1 => *is_void = true,
                2 => error_metadata[3] = 0,
                3 => instruction.operands.clear(),
                _ => unreachable!(),
            }
        }
        let errors = verify_module(&invalid).expect_err("invalid form");
        assert!(
            errors.iter().any(|e| e.message.contains(message)),
            "{errors:?}"
        );
    }
}

#[test]
fn completion_arguments_keep_foreign_string_and_array_provenance() {
    let module = module_for(PARAMETER_SCRIPT);
    verify_module(&module).expect("foreign argument lowering");
    let instruction = module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, lir::InstructionKind::HostCompletion { .. }))
        .expect("completion");
    assert_eq!(instruction.operands.len(), 3);
    assert!(module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .any(|i| i.kind == lir::InstructionKind::ForeignArrayData));
}

#[test]
fn completion_size_comes_from_the_c_directive() {
    let mut hir = check_program(&[
        SourceFile::ambient("completion.d.ts", MIRROR),
        SourceFile::new("completion.ts", SCRIPT),
    ])
    .expect("checker");
    let valid = lower_module(&hir).expect("lower");
    verify_module(&valid).expect("same-shape control");
    // Construct a different C result declaration; keep the script result type unchanged.
    hir.foreign_fns
        .iter_mut()
        .find(|f| f.name == "read")
        .expect("read")
        .completion_result = Some("int64_t".into());
    let error = lower_module(&hir).expect_err("C and script size disagreement");
    assert!(error.message.contains("result size disagrees"), "{error:?}");

    let mut hir = check_program(&[
        SourceFile::ambient(
            "completion.d.ts",
            format!("{MIRROR}\ndeclare class ShortPair {{ x: i32; y: i32; }}\n"),
        ),
        SourceFile::new("completion.ts", SCRIPT),
    ])
    .expect("checker");
    hir.foreign_fns
        .iter_mut()
        .find(|f| f.name == "pair")
        .expect("pair")
        .completion_result = Some("ShortPair".into());
    let error = lower_module(&hir).expect_err("named C and script struct size disagreement");
    assert!(error.message.contains("result size disagrees"), "{error:?}");
}

#[test]
fn verifier_rejects_wire_alias_completion_results() {
    let hir = check_program(&[
        SourceFile::ambient(
            "completion.d.ts",
            format!("{MIRROR}\ntype WireAlias = \"a\" | \"b\";\n"),
        ),
        SourceFile::new("completion.ts", SCRIPT),
    ])
    .expect("checker");
    let mut module = lower_module(&hir).expect("lower");
    verify_module(&module).expect("same-shape control");
    // The checker never emits this result. Construct the unsupported declared type directly.
    let alias = module
        .string_aliases
        .iter_mut()
        .find(|a| a.source_name == "WireAlias")
        .expect("wire alias");
    alias.wire_values = Some(vec![0, 1]);
    let alias_id = alias.id;
    let foreign = &mut module.foreign_functions[0];
    foreign.completion_result = Some(("WireAlias".into(), Type::StringAlias(alias_id)));
    let errors = verify_module(&module).expect_err("unsupported wire alias");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("outside the boundary result set")),
        "{errors:?}"
    );
}

#[test]
fn verifier_accepts_completion_struct_bool_fields_at_every_depth() {
    // Isolated warm Apple arm64 cost: 0.04 s, excluding the Rust build.
    let mirror = MIRROR.replace("x: i32", "x: boolean");
    let hir = check_program(&[
        SourceFile::ambient("completion.d.ts", format!("{mirror}\ndeclare class Outer {{ inner: Pair; }}\n// @subscript-c-completion function=\"nested\" result=\"Outer\"\ndeclare function nested(): Promise<Outer>;\n")),
        SourceFile::new("completion.ts", "export async function main(): Promise<void> { await pair(); await nested(); }"),
    ]).expect("checker admits bool leaves");
    let module = lower_module(&hir).expect("lower");
    verify_module(&module).expect("bool leaves at both depths");
}

#[test]
fn verifier_rejects_compared_boundary_class_with_empty_header_identity() {
    // Isolated Apple arm64 cost: 0.006413 s, excluding the Rust build.
    // Cost: one small script lowering and one verifier pass; no native compilation.
    let hir = check_program(&[SourceFile::new(
        "missing-header.ts",
        "@ValueType class OnlyValue { a: boolean; b: boolean; constructor(a: boolean, b: boolean) { this.a = a; this.b = b; } } export function main(): void {}",
    )]).expect("script value class");
    let mut module = lower_module(&hir).expect("script class lowering");
    let index = module
        .classes
        .iter()
        .position(|class| class.source_name == "OnlyValue")
        .expect("class index");
    // Construct a boundary declaration without provenance from the script layout.
    module.classes[index] = lir::Class {
        is_boundary: true,
        boundary_header: Some(String::new()),
        copies_boundary_bytes: true,
        ..module.classes[index].clone()
    };
    let errors = verify_module(&module).expect_err("missing boundary header");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message,
        format!("boundary class {index} has no header identity for host layout comparison")
    );
}
