//! compiler.md §280: run the host examples with the CLI built for this test.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn examples_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples")
}

fn assert_host_program_matches_golden(directory: &str) {
    let host = examples_root().join(directory);
    let golden = host.join("expected.txt");
    let expected = fs::read(&golden).unwrap();
    let output = Command::new("sh")
        .arg(host.join("build.sh"))
        .env("SUBSCRIPT", env!("CARGO_BIN_EXE_subscript"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{directory}: {}\n{}",
        output.status,
        subscript_codegen::tool_output_report(&output)
    );
    assert_stdout(&output.stdout, &expected, &golden);
}

#[test]
fn capstone_host_builds_runs_and_matches_golden() {
    assert_host_program_matches_golden("host");
}

#[test]
fn context_per_scene_host_builds_runs_and_matches_golden() {
    assert_host_program_matches_golden("context-per-scene");
}

#[test]
fn scripts_build_cargo_only_when_subscript_is_unset() {
    for directory in ["host", "context-per-scene"] {
        let source = fs::read_to_string(examples_root().join(directory).join("build.sh")).unwrap();
        let lines: Vec<_> = source.lines().map(str::trim).collect();
        let cargo: Vec<_> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.starts_with("cargo "))
            .collect();
        assert_eq!(cargo.len(), 1, "{directory}: one Cargo command");
        let index = cargo[0].0;
        assert_eq!(
            lines[index - 1],
            "if [ \"\u{24}{SUBSCRIPT+x}\" != x ]; then"
        );
        assert_eq!(
            lines[index + 1],
            "SUBSCRIPT=\"$REPO_ROOT/target/release/subscript\""
        );
        assert_eq!(lines[index + 2], "fi");
        assert!(source.contains("\"$SUBSCRIPT\" build"));
    }
}

#[test]
fn subscript_override_runs_stub_with_build_source_arguments() {
    let scratch =
        std::env::temp_dir().join(format!("subscript-host-control-{}", std::process::id()));
    fs::create_dir(&scratch).unwrap();
    let _guard = ScratchDir(scratch.clone());
    let stub = scratch.join("stub cli");
    fs::write(
        &stub,
        "#!/bin/sh\n[ \"$1\" = build ] && [ \"$2\" = --source ] || exit 1\nprintf 'host override control\\n'\n",
    )
    .unwrap();
    let status = Command::new("chmod").arg("+x").arg(&stub).status().unwrap();
    assert!(status.success());
    for directory in ["host", "context-per-scene"] {
        let output = Command::new("sh")
            .arg(examples_root().join(directory).join("build.sh"))
            .env("SUBSCRIPT", &stub)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{directory}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"host override control\n");
    }
}

// §302: measured cost: <0.01 s; one synthetic comparison, no host build.
#[test]
fn mismatch_report_contains_text_and_golden_path() {
    let panic = std::panic::catch_unwind(|| {
        assert_stdout(
            b"actual line\n",
            b"expected\n",
            std::path::Path::new("fixture/expected.txt"),
        )
    })
    .unwrap_err();
    let message = panic.downcast_ref::<String>().unwrap();
    assert!(message.contains("actual line"), "{message}");
    assert!(message.contains("fixture/expected.txt"), "{message}");
}
fn assert_stdout(actual: &[u8], expected: &[u8], golden: &std::path::Path) {
    assert!(
        actual == expected,
        "stdout differs from {}\nactual:\n{}\nexpected:\n{}",
        golden.display(),
        String::from_utf8_lossy(actual),
        String::from_utf8_lossy(expected)
    );
}

struct ScratchDir(PathBuf);
impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// §302: measured cost: <0.01 s; one directory and one unwind.
#[test]
fn host_scratch_cleans_on_assertion_failure() {
    let root = std::env::temp_dir().join(format!("subscript-host-guard-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let result = std::panic::catch_unwind(|| {
        let _guard = ScratchDir(root.clone());
        assert!(!root.exists(), "injected assertion");
    });
    assert!(result.is_err());
    let exists = root.exists();
    let _ = fs::remove_dir_all(&root);
    assert!(!exists);
}
