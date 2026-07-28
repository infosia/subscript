//! Caller-supplied native-library surface and explicit resolution errors.

use subscript_codegen::{run_c_aot, run_jit, RunError};
use subscript_compiler::SourceFile;

fn missing_symbol_program() -> Vec<SourceFile> {
    vec![
        SourceFile::ambient(
            "missing.generated.d.ts",
            "// @subscript-c-header include=\"missing.h\"\n\
             declare function stage4MissingForeignSymbol(): void;\n",
        ),
        SourceFile::new(
            "main.ts",
            "export function main(): void {\n  stage4MissingForeignSymbol();\n}\n",
        ),
    ]
}

#[test]
fn empty_library_set_runs_programs_without_foreign_calls() {
    let files = [SourceFile::new(
        "main.ts",
        "export function main(): void {\n  print(`local`);\n}\n",
    )];
    assert_eq!(run_jit(&files).expect("dev JIT"), b"local\n");
    assert_eq!(run_c_aot(&files).expect("ship C AOT"), b"local\n");
}

#[test]
fn unregistered_foreign_symbol_is_named_before_platform_lookup() {
    for (tier, result) in [
        ("dev JIT", run_jit(&missing_symbol_program())),
        ("ship C AOT", run_c_aot(&missing_symbol_program())),
    ] {
        match result {
            Err(RunError::UnresolvedForeignSymbol(name)) => {
                assert_eq!(name, "stage4MissingForeignSymbol", "{tier}");
            }
            other => panic!("{tier}: expected explicit unresolved-symbol error, got {other:?}"),
        }
    }
}
