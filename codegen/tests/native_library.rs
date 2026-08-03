//! Caller-supplied native-library surface, explicit resolution errors, and
//! abort-time output preservation for the run helpers.

use std::path::{Path, PathBuf};
use std::process::Command;

use subscript_codegen::{
    run_c_aot, run_c_aot_with_native_libraries, run_jit, run_jit_with_native_libraries,
    NativeLibrary, RunError, JIT_OUTPUT_FILE_ENV,
};
use subscript_compiler::SourceFile;

const ABORT_CHILD_TIER: &str = "SUBSCRIPT_CODEGEN_ABORT_OUTPUT_CHILD_TIER";
const ABORT_CHILD_MODE: &str = "SUBSCRIPT_CODEGEN_ABORT_OUTPUT_CHILD_MODE";
const ABORT_FIXTURE_DIR: &str = "SUBSCRIPT_CODEGEN_ABORT_OUTPUT_FIXTURE_DIR";
const BEFORE_ABORT: &str = "before-native-abort\nstill-before-native-abort\n";

unsafe extern "C" fn panic_across_jit_boundary() {
    panic!("forced panic-abort after output");
}

unsafe extern "C" fn hard_terminate_jit_process() {
    std::process::abort();
}

fn aborting_program(fixture: &Path, mode: &str) -> (Vec<SourceFile>, NativeLibrary) {
    let (foreign_name, foreign_address) = match mode {
        "panic" => (
            "subscriptTestPanic",
            panic_across_jit_boundary as *const u8,
        ),
        "signal" => (
            "subscriptTestSignal",
            hard_terminate_jit_process as *const u8,
        ),
        other => panic!("unknown abort-output child mode {other}"),
    };
    let files = vec![
        SourceFile::ambient(
            "abort.generated.d.ts",
            format!(
                "// @subscript-c-header include=\"abort.h\"\n\
                 declare function {foreign_name}(): void;\n"
            ),
        ),
        SourceFile::new(
            "main.ts",
            format!(
                "export function main(): void {{\n\
               print(`before-native-abort`);\n\
               print(`still-before-native-abort`);\n\
               {foreign_name}();\n\
             }}\n"
            ),
        ),
    ];
    // SAFETY: both Rust functions have the selected no-argument C signature
    // and static lifetime; the C source defines the same selected symbol for
    // the C-AOT link. Both intentionally terminate their child process.
    let library = unsafe {
        NativeLibrary::new(
            vec![fixture.to_path_buf()],
            vec![fixture.join("abort.c")],
            vec![(foreign_name.to_string(), foreign_address)],
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

/// Subprocess target for the abort-output parent tests below.
#[test]
fn aborting_run_output_child() {
    let Ok(tier) = std::env::var(ABORT_CHILD_TIER) else {
        return;
    };
    let mode = std::env::var(ABORT_CHILD_MODE).expect("abort termination mode");
    let fixture = PathBuf::from(std::env::var_os(ABORT_FIXTURE_DIR).expect("fixture directory"));
    let (files, library) = aborting_program(&fixture, &mode);
    match tier.as_str() {
        "jit" => {
            let result = run_jit_with_native_libraries(&files, &[library]);
            panic!("dev-JIT {mode} returned to its caller: {result:?}");
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

fn abort_fixture(mode: &str) -> PathBuf {
    let fixture = std::env::temp_dir().join(format!(
        "subscript-codegen-abort-output-{}-{mode}",
        std::process::id(),
    ));
    std::fs::create_dir(&fixture).expect("create abort fixture directory");
    std::fs::write(
        fixture.join("abort.h"),
        "#ifndef SUBSCRIPT_TEST_ABORT_H\n\
         #define SUBSCRIPT_TEST_ABORT_H\n\
         void subscriptTestPanic(void);\n\
         void subscriptTestSignal(void);\n\
         #endif\n",
    )
    .expect("write abort header");
    std::fs::write(
        fixture.join("abort.c"),
        "#include <stdlib.h>\n\
         #include \"abort.h\"\n\
         void subscriptTestPanic(void) { abort(); }\n\
         void subscriptTestSignal(void) { abort(); }\n",
    )
    .expect("write abort source");
    fixture
}

fn run_abort_output_child(
    fixture: &Path,
    tier: &str,
    mode: &str,
) -> (std::process::Output, Vec<u8>) {
    let output_file = fixture.join(format!("{tier}-{mode}.out"));
    std::fs::File::create(&output_file).expect("create parent-owned JIT output file");
    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "aborting_run_output_child", "--nocapture"])
        .env(ABORT_CHILD_TIER, tier)
        .env(ABORT_CHILD_MODE, mode)
        .env(ABORT_FIXTURE_DIR, fixture)
        .env(JIT_OUTPUT_FILE_ENV, &output_file)
        .output()
        .unwrap_or_else(|error| panic!("spawn {tier} {mode} child: {error}"));
    let persisted = std::fs::read(&output_file).expect("read parent-owned JIT output file");
    (output, persisted)
}

fn assert_partial_output(bytes: &[u8], stderr: &[u8], label: &str) {
    let stdout = String::from_utf8_lossy(bytes);
    assert!(
        stdout.contains(BEFORE_ABORT),
        "{label}: partial output was lost; child stdout={stdout:?}, stderr={:?}",
        String::from_utf8_lossy(stderr)
    );
}

#[test]
fn non_unwinding_panic_surfaces_output_already_produced() {
    let fixture = abort_fixture("panic");
    let (output, persisted) = run_abort_output_child(&fixture, "jit", "panic");
    assert!(
        !output.status.success(),
        "dev-JIT panic across the C ABI must abort the child"
    );
    assert_partial_output(&output.stdout, &output.stderr, "dev-JIT panic");
    assert_partial_output(&persisted, &output.stderr, "dev-JIT panic file");
    std::fs::remove_dir_all(&fixture).expect("remove abort fixture directory");
}

#[test]
fn hard_signal_surfaces_output_already_produced_on_both_tiers() {
    let fixture = abort_fixture("signal");
    for tier in ["jit", "c-aot"] {
        let (output, persisted) = run_abort_output_child(&fixture, tier, "signal");
        if tier == "jit" {
            assert!(
                !output.status.success(),
                "dev-JIT hard termination must abort the child"
            );
        } else {
            assert!(
                output.status.success(),
                "C-AOT helper must report its linked child's abort without aborting: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let retained = if tier == "jit" {
            &persisted
        } else {
            &output.stdout
        };
        assert_partial_output(retained, &output.stderr, &format!("{tier} hard signal"));
    }
    std::fs::remove_dir_all(&fixture).expect("remove abort fixture directory");
}
