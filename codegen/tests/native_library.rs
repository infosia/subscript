//! Caller-supplied native-library surface, explicit resolution errors, and
//! abort-time output preservation for the run helpers.

use std::path::{Path, PathBuf};
use std::process::Command;

use subscript_codegen::{
    run_c_aot, run_c_aot_with_native_libraries, run_jit, run_jit_with_native_libraries,
    NativeLibrary, RunError,
};
use subscript_compiler::SourceFile;

const ABORT_CHILD_TIER: &str = "SUBSCRIPT_CODEGEN_ABORT_OUTPUT_CHILD_TIER";
const ABORT_FIXTURE_DIR: &str = "SUBSCRIPT_CODEGEN_ABORT_OUTPUT_FIXTURE_DIR";
const BEFORE_ABORT: &str = "before-native-abort\nstill-before-native-abort\n";

unsafe extern "C" fn panic_across_jit_boundary() {
    panic!("forced panic-abort after output");
}

fn aborting_program(fixture: &Path) -> (Vec<SourceFile>, NativeLibrary) {
    let files = vec![
        SourceFile::ambient(
            "abort.generated.d.ts",
            "// @subscript-c-header include=\"abort.h\"\n\
             declare function subscriptTestAbort(): void;\n",
        ),
        SourceFile::new(
            "main.ts",
            "export function main(): void {\n\
               print(`before-native-abort`);\n\
               print(`still-before-native-abort`);\n\
               subscriptTestAbort();\n\
             }\n",
        ),
    ];
    // SAFETY: the Rust function has the declared no-argument C signature and
    // static lifetime; the C source defines the same symbol for the C-AOT
    // link. Both intentionally terminate their respective child process.
    let library = unsafe {
        NativeLibrary::new(
            vec![fixture.to_path_buf()],
            vec![fixture.join("abort.c")],
            vec![(
                "subscriptTestAbort".to_string(),
                panic_across_jit_boundary as *const u8,
            )],
        )
    };
    (files, library)
}

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

/// Subprocess target for `aborting_runs_surface_output_already_produced`.
/// The JIT branch is expected to abort this test process; the C-AOT branch
/// observes the linked child's abort as an Internal error and returns cleanly.
#[test]
fn aborting_run_output_child() {
    let Ok(tier) = std::env::var(ABORT_CHILD_TIER) else {
        return;
    };
    let fixture = PathBuf::from(std::env::var_os(ABORT_FIXTURE_DIR).expect("fixture directory"));
    let (files, library) = aborting_program(&fixture);
    match tier.as_str() {
        "jit" => {
            let result = run_jit_with_native_libraries(&files, &[library]);
            panic!("dev-JIT abort returned to its caller: {result:?}");
        }
        "c-aot" => assert!(
            matches!(
                run_c_aot_with_native_libraries(&files, &[library]),
                Err(RunError::Internal(_))
            ),
            "C-AOT native abort must be an abnormal linked-program exit"
        ),
        other => panic!("unknown abort-output child tier {other}"),
    }
}

#[test]
fn aborting_runs_surface_output_already_produced() {
    let fixture = std::env::temp_dir().join(format!(
        "subscript-codegen-abort-output-{}",
        std::process::id()
    ));
    std::fs::create_dir(&fixture).expect("create abort fixture directory");
    std::fs::write(
        fixture.join("abort.h"),
        "#ifndef SUBSCRIPT_TEST_ABORT_H\n\
         #define SUBSCRIPT_TEST_ABORT_H\n\
         void subscriptTestAbort(void);\n\
         #endif\n",
    )
    .expect("write abort header");
    std::fs::write(
        fixture.join("abort.c"),
        "#include <stdlib.h>\n\
         #include \"abort.h\"\n\
         void subscriptTestAbort(void) { abort(); }\n",
    )
    .expect("write abort source");

    for tier in ["jit", "c-aot"] {
        let output = Command::new(std::env::current_exe().expect("current test executable"))
            .args(["--exact", "aborting_run_output_child", "--nocapture"])
            .env(ABORT_CHILD_TIER, tier)
            .env(ABORT_FIXTURE_DIR, &fixture)
            .output()
            .unwrap_or_else(|error| panic!("spawn {tier} abort child: {error}"));
        if tier == "jit" {
            assert!(
                !output.status.success(),
                "dev-JIT panic across the C ABI must abort the child"
            );
        } else {
            assert!(
                output.status.success(),
                "C-AOT helper must report its linked child's abort without aborting: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(BEFORE_ABORT),
            "{tier}: partial output was lost; child stdout={stdout:?}, stderr={:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    std::fs::remove_dir_all(&fixture).expect("remove abort fixture directory");
}
