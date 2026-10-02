//! Checks for finished generator reference reads (compiler.md §145).

use subscript_codegen::{
    interpreter::{interpret, InterpretError},
    lir::{lower_module, verify_module},
    run_c_aot, run_jit, RunError,
};
use subscript_compiler::{check_program, lir as l, SourceFile};

#[test]
fn controls_match_all_three_execution_forms() {
    for (name, text, expected) in [
        (
            "a310-generator-done-reference-controls.ts",
            include_str!("../../corpus/accept/a310-generator-done-reference-controls.ts"),
            include_bytes!("../../corpus/accept/a310-generator-done-reference-controls.expected")
                .as_slice(),
        ),
        (
            "a311-generator-done-scalar-zero.ts",
            include_str!("../../corpus/accept/a311-generator-done-scalar-zero.ts"),
            include_bytes!("../../corpus/accept/a311-generator-done-scalar-zero.expected")
                .as_slice(),
        ),
    ] {
        let sources = [SourceFile::new(name, text)];
        let hir = check_program(&sources).unwrap();
        let module = lower_module(&hir).unwrap();
        verify_module(&module).unwrap();
        assert_eq!(interpret(&module).unwrap(), expected, "{name}: interpreter");
        assert_eq!(run_jit(&sources).unwrap(), expected, "{name}: dev");
        assert_eq!(run_c_aot(&sources).unwrap(), expected, "{name}: ship");
    }
}

#[test]
fn a_finished_reference_destructuring_traps_at_the_binding() {
    let text = "class Box { v: i32 = 7; }\nfunction* values(): Generator<Box> { yield new Box(); }\nexport function main(): void {\n  const iterator = values();\n  iterator.next();\n  const r = iterator.next();\n  const { value: b } = r;\n  print(`${b.v}`);\n}\n";
    let sources = [SourceFile::new("destructure.ts", text)];
    let hir = check_program(&sources).unwrap();
    let module = lower_module(&hir).unwrap();
    verify_module(&module).unwrap();
    for error in [
        run_jit(&sources).unwrap_err(),
        run_c_aot(&sources).unwrap_err(),
    ] {
        let RunError::Trap(report) = error else {
            panic!("expected trap: {error:?}")
        };
        assert_eq!(report.rule, subscript_runtime::TrapKind::GeneratorDoneValue);
        assert_eq!((report.pos.line, report.pos.col), (7, 18));
    }
    let InterpretError::Execution { source, .. } = interpret(&module).unwrap_err() else {
        panic!("expected execution trap")
    };
    let InterpretError::Trap { kind, pos, .. } = *source else {
        panic!("expected interpreter trap")
    };
    assert_eq!(kind, "generator-done-value");
    assert_eq!((pos.line, pos.col), (7, 18));

    let mut invalid = module.clone();
    for function in &mut invalid.functions {
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                instruction
                    .traps
                    .retain(|trap| trap.kind != l::TrapKind::GeneratorDoneValue);
            }
        }
    }
    assert!(verify_module(&invalid).is_err());
}
