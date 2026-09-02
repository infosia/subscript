use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use subscript_compiler::{check_program, SourceFile};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the compiler crate must have a workspace parent")
        .to_path_buf()
}

fn test_directory(name: &str) -> PathBuf {
    let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "section82-round2-{}-{name}-{id}",
        std::process::id()
    ))
}

fn command_output(mut command: Command, tier: &str) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("the {tier} command must start: {error}"));
    assert!(
        output.status.success(),
        "the {tier} command failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_both_tiers(name: &str, source: &str, expected: &[u8]) {
    let workspace = workspace();
    let directory = test_directory(name);
    std::fs::create_dir_all(&directory).expect("the scratch directory must be created");
    let source_path = directory.join("probe.ts");
    std::fs::write(&source_path, source).expect("the probe source must be written");

    let mut executable = workspace.join("target").join("debug").join("subscript");
    executable.set_extension(std::env::consts::EXE_EXTENSION);
    assert!(
        executable.is_file(),
        "cargo build --workspace --all-targets must create {}",
        executable.display()
    );

    let mut dev = Command::new(&executable);
    dev.current_dir(&workspace).arg("run").arg(&source_path);
    let dev = command_output(dev, "dev JIT");

    let runtime_name = if cfg!(all(windows, target_env = "msvc")) {
        "subscript_runtime.lib"
    } else {
        "libsubscript_runtime.a"
    };
    let ship_directory = directory.join("ship");
    let mut ship = Command::new(&executable);
    ship.current_dir(&workspace)
        .arg("build")
        .arg("--source")
        .arg(&source_path)
        .arg("-o")
        .arg(&ship_directory)
        .arg("--run")
        .arg("--runtime-lib")
        .arg(workspace.join("target").join("debug").join(runtime_name))
        .arg("--runtime-include")
        .arg(workspace.join("runtime").join("include"));
    let ship = command_output(ship, "ship tier");

    assert_eq!(dev.stdout, expected, "the dev JIT output must match");
    assert_eq!(ship.stdout, expected, "the ship tier output must match");
    assert_eq!(
        dev.stdout, ship.stdout,
        "the tier output must be byte-identical"
    );
    std::fs::remove_dir_all(&directory).expect("the scratch directory must be removed");
}

const SUPPORT: &str = "class Box {\n\
                      \x20 v: i32;\n\
                      \x20 constructor(v: i32) { this.v = v; }\n\
                      }\n\
                      function maybe(keep: boolean): Box | null {\n\
                      \x20 return keep ? new Box(1) : null;\n\
                      }\n";

#[test]
fn for_condition_with_nullish_access_runs_on_both_tiers() {
    let source = format!(
        "{SUPPORT}export function main(): void {{\n\
         \x20 const fb: Box = new Box(1);\n\
         \x20 let count: i32 = 0;\n\
         \x20 for (let i: i32 = 0; i < 3 && (maybe(true) ?? fb).v > 0; i++) {{ count++; }}\n\
         \x20 print(`${{count}}`);\n\
         }}\n"
    );
    run_both_tiers("for-nullish", &source, b"3\n");
}

#[test]
fn for_condition_with_optional_access_runs_on_both_tiers() {
    let source = format!(
        "{SUPPORT}export function main(): void {{\n\
         \x20 const fb: Box = new Box(1);\n\
         \x20 let count: i32 = 0;\n\
         \x20 for (let i: i32 = 0; i < 3 && (maybe(true)?.v ?? 0) > 0; i++) {{ count++; }}\n\
         \x20 print(`${{count}}:${{fb.v}}`);\n\
         }}\n"
    );
    run_both_tiers("for-optional", &source, b"3:1\n");
}

#[test]
fn for_initializer_with_nullish_access_runs_on_both_tiers() {
    let source = format!(
        "{SUPPORT}export function main(): void {{\n\
         \x20 const fb: Box = new Box(1);\n\
         \x20 let i: i32 = 0;\n\
         \x20 let count: i32 = 0;\n\
         \x20 for (i = (maybe(true) ?? fb).v; i < 3; i++) {{ count++; }}\n\
         \x20 print(`${{count}}`);\n\
         }}\n"
    );
    run_both_tiers("for-initializer", &source, b"2\n");
}

#[test]
fn initializer_and_arrow_owners_run_on_both_tiers() {
    let source = format!(
        "{SUPPORT}export function main(): void {{\n\
         \x20 const fb: Box = new Box(1);\n\
         \x20 const n: i32 = (maybe(true) ?? fb).v + ((): i32 => 2)();\n\
         \x20 print(`${{n}}`);\n\
         }}\n"
    );
    run_both_tiers("arrow-owner", &source, b"3\n");
}

#[test]
fn empty_for_body_with_nullish_condition_runs_on_both_tiers() {
    let source = format!(
        "{SUPPORT}export function main(): void {{\n\
         \x20 const fb: Box = new Box(1);\n\
         \x20 let j: i32 = 0;\n\
         \x20 for (j = 0; j < 3 && (maybe(false) ?? fb).v > 0; j++) {{ }}\n\
         \x20 print(`${{j}}`);\n\
         }}\n"
    );
    run_both_tiers("empty-for-body", &source, b"3\n");
}

#[test]
fn a176_and_a177_shapes_have_no_diagnostics() {
    for (name, source) in [
        (
            "a176-compound-through-accessor.ts",
            include_str!("../../corpus/accept/a176-compound-through-accessor.ts"),
        ),
        (
            "a177-nullish.ts",
            include_str!("../../corpus/accept/a177-nullish.ts"),
        ),
    ] {
        check_program(&[SourceFile::new(name, source)])
            .unwrap_or_else(|diagnostics| panic!("{name} must check: {diagnostics:?}"));
    }
}
