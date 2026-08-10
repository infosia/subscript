//! Caller-supplied native-library surface, explicit resolution errors, and
//! abort-time output preservation for the run helpers.

// Naming the dev-dependency propagates its test-only native archive into
// this integration-test link.
extern crate subscript_archive_fixture;

use std::path::{Path, PathBuf};
use std::process::Command;

use subscript_codegen::{
    run_aot_with_native_libraries, run_c_aot, run_c_aot_with_native_libraries, run_jit,
    run_jit_with_native_libraries, NativeLibrary, RunError, JIT_OUTPUT_FILE_ENV,
};
use subscript_compiler::SourceFile;

const ABORT_CHILD_MODE: &str = "SUBSCRIPT_CODEGEN_ABORT_OUTPUT_CHILD_MODE";
const ABORT_FIXTURE_DIR: &str = "SUBSCRIPT_CODEGEN_ABORT_OUTPUT_FIXTURE_DIR";
const BEFORE_ABORT: &str = "before-native-abort\nstill-before-native-abort\n";
const DEV_RETENTION_SKIP: &str =
    "dev-JIT retention skipped: this platform does not isolate the run (compiler.md §44.10)";
const ARCHIVE_ONLY_EXPECTED: &[u8] = b"22\n";

extern "C" {
    fn subArchiveOnlyProbe(value: i32) -> i32;
}

type IsolatedDevRun = fn(&[SourceFile], &[NativeLibrary]) -> Result<Vec<u8>, RunError>;

#[cfg(unix)]
fn isolated_dev_run() -> Option<IsolatedDevRun> {
    Some(subscript_codegen::run_jit_with_native_libraries)
}

#[cfg(not(unix))]
fn isolated_dev_run() -> Option<IsolatedDevRun> {
    None
}

unsafe extern "C" fn panic_across_jit_boundary() {
    panic!("forced panic-abort after output");
}

unsafe extern "C" fn hard_terminate_jit_process() {
    std::process::abort();
}

fn aborting_program(fixture: &Path, mode: &str) -> (Vec<SourceFile>, NativeLibrary) {
    let (foreign_name, foreign_address) = match mode {
        "panic" => ("subscriptTestPanic", panic_across_jit_boundary as *const u8),
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
fn static_archive_link_input_follows_translation_units_on_all_tiers() {
    let files = vec![
        SourceFile::ambient(
            "archive-only.generated.d.ts",
            "// @subscript-c-header include=\"archive-only.h\"\n\
             declare function subArchiveOnlyProbe(value: i32): i32;\n",
        ),
        SourceFile::new(
            "main.ts",
            "export function main(): void {\n\
               print(`${subArchiveOnlyProbe(7)}`);\n\
             }\n",
        ),
    ];
    // SAFETY: the fixture crate links this static-lifetime function into
    // the test process with the signature in the inline mirror and header.
    let library = unsafe {
        NativeLibrary::new(
            vec![PathBuf::from(subscript_archive_fixture::CRATE_DIRECTORY)],
            vec![PathBuf::from(subscript_archive_fixture::ARCHIVE_PATH)],
            vec![(
                "subArchiveOnlyProbe".to_string(),
                subArchiveOnlyProbe as *const u8,
            )],
        )
    };

    let c_aot = run_c_aot_with_native_libraries(&files, std::slice::from_ref(&library))
        .expect("ship C-AOT tier runs with the static archive");
    assert_eq!(c_aot, ARCHIVE_ONLY_EXPECTED, "ship C-AOT tier output");

    let object_aot = run_aot_with_native_libraries(&files, std::slice::from_ref(&library))
        .expect("retained Cranelift-object AOT tier runs with the static archive");
    assert_eq!(
        object_aot, ARCHIVE_ONLY_EXPECTED,
        "retained Cranelift-object AOT tier output"
    );

    let jit = run_jit_with_native_libraries(&files, std::slice::from_ref(&library))
        .expect("dev JIT tier runs with the registered archive symbol");
    assert_eq!(jit, c_aot, "dev JIT tier differs from ship C-AOT tier");
    assert_eq!(
        jit, object_aot,
        "dev JIT tier differs from retained Cranelift-object AOT tier"
    );
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

/// Subprocess target that keeps the environment-variable compatibility path
/// isolated from the test runner's parallel process environment.
#[test]
fn jit_output_file_override_child() {
    let Ok(mode) = std::env::var(ABORT_CHILD_MODE) else {
        return;
    };
    let Some(run_dev) = isolated_dev_run() else {
        println!("{DEV_RETENTION_SKIP}");
        return;
    };
    let fixture = PathBuf::from(std::env::var_os(ABORT_FIXTURE_DIR).expect("fixture directory"));
    let (files, library) = aborting_program(&fixture, &mode);
    assert_abnormal_output(run_dev(&files, &[library]), "dev-JIT output-file override");
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

fn assert_abnormal_output(result: Result<Vec<u8>, RunError>, label: &str) {
    let termination = match result {
        Err(RunError::AbnormalTermination(termination)) => termination,
        other => panic!("{label}: expected abnormal termination, got {other:?}"),
    };
    let stdout = String::from_utf8_lossy(&termination.stdout);
    assert!(
        stdout.contains(BEFORE_ABORT),
        "{label}: partial output was lost; retained stdout={stdout:?}, stderr={:?}",
        String::from_utf8_lossy(&termination.stderr)
    );
}

#[test]
fn non_unwinding_panic_surfaces_output_already_produced() {
    let Some(run_dev) = isolated_dev_run() else {
        println!("{DEV_RETENTION_SKIP}");
        return;
    };
    let fixture = abort_fixture("panic");
    let (files, library) = aborting_program(&fixture, "panic");
    assert_abnormal_output(run_dev(&files, &[library]), "dev-JIT panic");
    std::fs::remove_dir_all(&fixture).expect("remove abort fixture directory");
}

#[test]
fn jit_output_file_override_still_retains_child_process_output() {
    let Some(_run_dev) = isolated_dev_run() else {
        println!("{DEV_RETENTION_SKIP}");
        return;
    };
    let fixture = abort_fixture("env-signal");
    let output_file = fixture.join("jit-signal.out");
    std::fs::File::create(&output_file).expect("create parent-owned JIT output file");
    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "jit_output_file_override_child", "--nocapture"])
        .env(ABORT_CHILD_MODE, "signal")
        .env(ABORT_FIXTURE_DIR, &fixture)
        .env(JIT_OUTPUT_FILE_ENV, &output_file)
        .output()
        .expect("spawn JIT output-file override child");
    assert!(
        output.status.success(),
        "JIT output-file override child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let persisted = std::fs::read(&output_file).expect("read parent-owned JIT output file");
    assert!(
        String::from_utf8_lossy(&persisted).contains(BEFORE_ABORT),
        "JIT output-file override lost output: {:?}",
        String::from_utf8_lossy(&persisted)
    );
    std::fs::remove_dir_all(&fixture).expect("remove abort fixture directory");
}

#[test]
fn no_opt_in_hard_signal_returns_retained_output_on_both_tiers() {
    let fixture = abort_fixture("signal");
    if let Some(run_dev) = isolated_dev_run() {
        let (jit_files, jit_library) = aborting_program(&fixture, "signal");
        assert_abnormal_output(run_dev(&jit_files, &[jit_library]), "dev-JIT hard signal");
    } else {
        println!("{DEV_RETENTION_SKIP}");
    }
    let (ship_files, ship_library) = aborting_program(&fixture, "signal");
    assert_abnormal_output(
        run_c_aot_with_native_libraries(&ship_files, &[ship_library]),
        "C-AOT hard signal",
    );
    std::fs::remove_dir_all(&fixture).expect("remove abort fixture directory");
}
