use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

static GATE_LOCK: Mutex<()> = Mutex::new(());

struct Stubs {
    dir: PathBuf,
    records_before: BTreeSet<PathBuf>,
}

impl Stubs {
    fn new(case: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "subscript-gate-test-{}-{nonce}-{case}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).unwrap();
        let stubs = Self {
            dir,
            records_before: gate_records(),
        };
        stubs.write(
            "cargo",
            &format!(
                r#"#!/bin/sh
case "$1" in
    -V) echo 'cargo stub' ;;
    fmt) exit 0 ;;
    build)
        if [ '{case}' = warning ]; then echo 'warning: build warning' >&2; fi
        ;;
    test)
        if [ '{case}' = sleep ]; then
            touch "$(dirname "$0")/test-started"
            sleep 20
        fi
        case "$*" in
            *--release*)
                [ "$SUBSCRIPT_FULL_INTERPRETER_SWEEP" = 1 ] || exit 90
                echo 'release sweep: 1'
                ;;
        esac
        if [ '{case}' = failed ]; then
            echo 'test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s'
            exit 1
        fi
        echo 'test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s'
        echo 'test result: ok. 3 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s'
        if [ '{case}' = skip ]; then echo 'gate-skip: stub_suite unset fixture variable'; fi
        ;;
    clippy)
        if [ '{case}' = clippy-compiler ]; then
            echo 'warning: `subscript-compiler` (lib) generated 8 warnings' >&2
        else
            echo 'warning: `subscript-compiler` (lib) generated 7 warnings' >&2
        fi
        if [ '{case}' = clippy-runtime ]; then
            echo 'warning: `subscript-runtime` (lib) generated 19 warnings' >&2
        else
            echo 'warning: `subscript-runtime` (lib) generated 18 warnings' >&2
        fi
        if [ '{case}' = clippy ]; then
            echo 'warning: `subscript-codegen` (lib) generated 14 warnings' >&2
        else
            echo 'warning: `subscript-codegen` (lib) generated 13 warnings' >&2
        fi
        echo 'warning: `subscript-codegen` (lib test) generated 99 warnings' >&2
        echo 'warning: `subscript-compiler` (test "fixture") generated 99 warnings' >&2
        echo 'warning: `another-crate` (lib) generated 99 warnings' >&2
        ;;
    *) exit 91 ;;
esac
"#
            ),
        );
        stubs.write("node", "#!/bin/sh\necho 'v22.0.0-stub'\n");
        stubs.write(
            "tsc",
            "#!/bin/sh\ncase \"$1\" in -v) echo 'Version stub';; -p) echo 'tsc ran';; *) exit 92;; esac\n",
        );
        stubs.write("cc", "#!/bin/sh\necho 'cc stub'\necho 'second cc line'\n");
        stubs
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.dir.join(name);
        std::fs::write(&path, text).unwrap();
        assert!(Command::new("chmod")
            .arg("+x")
            .arg(path)
            .status()
            .unwrap()
            .success());
    }

    fn command(&self, shape: &str) -> Command {
        let mut command = Command::new("sh");
        command
            .arg(root().join("tools/gate.sh"))
            .arg(shape)
            .current_dir(root())
            .env("CARGO", self.dir.join("cargo"))
            .env("NODE", self.dir.join("node"))
            .env("TSC", self.dir.join("tsc"))
            .env("CC", self.dir.join("cc"))
            .env("GIT", "git");
        command
    }

    fn run(&self, shape: &str) -> std::process::Output {
        self.command(shape).output().unwrap()
    }
}

impl Drop for Stubs {
    fn drop(&mut self) {
        // Remove only records that name this case's unique stub directory.
        for path in gate_records().difference(&self.records_before) {
            if std::fs::read_to_string(path)
                .is_ok_and(|record| record.contains(self.dir.to_string_lossy().as_ref()))
            {
                std::fs::remove_file(path).expect("delete the case's gate record");
            }
        }
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

fn gate_records() -> BTreeSet<PathBuf> {
    let dir = root().join("target/gate");
    if !dir.exists() {
        return BTreeSet::new();
    }
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect()
}

fn diagnostic(stdout: &str) -> String {
    stdout.lines().map(|line| format!("| {line}\n")).collect()
}

fn git(args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root())
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn assert_record(output: &std::process::Output, shape: &str, expected: &str) -> String {
    let rev = git(&["rev-parse", "HEAD"]);
    let dirty = git(&["status", "--porcelain"]);
    let state = if dirty.is_empty() {
        "clean".to_owned()
    } else {
        format!("dirty:{}", dirty.lines().count())
    };
    assert_record_with_identity(output, shape, expected, rev.trim(), &state)
}

fn assert_record_with_identity(
    output: &std::process::Output,
    shape: &str,
    expected: &str,
    rev: &str,
    state: &str,
) -> String {
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    assert!(lines.len() >= 2, "{}", diagnostic(&stdout));
    let path = lines[lines.len() - 2];
    assert!(path.starts_with("target/gate/"), "| {path}");
    assert!(path.ends_with(&format!("-{shape}.md")), "| {path}");
    let stamp = &path["target/gate/".len()..][..16];
    assert_eq!(stamp.len(), 16);
    assert_eq!(&stamp[8..9], "T");
    assert_eq!(&stamp[15..], "Z");
    assert!(stamp[..8].bytes().all(|b| b.is_ascii_digit()));
    assert!(stamp[9..15].bytes().all(|b| b.is_ascii_digit()));
    assert!(root().join(path).is_file());
    let record = std::fs::read_to_string(root().join(path)).unwrap();
    let verdict = format!("gate {shape} {} {state} {expected}", rev.trim());
    assert_eq!(lines.last().unwrap(), &verdict);
    assert_eq!(record.lines().last().unwrap(), verdict);
    assert!(record.starts_with(&format!(
        "shape: {shape}\nUTC: {stamp}\nrevision: {}\ndirty: ",
        rev.trim()
    )));
    assert!(record.contains("cargo stub\nv22.0.0-stub\nVersion stub\ncc stub\n```"));
    record
}

#[test]
fn quick_sums_results_and_lists_skip() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("skip");
    let output = stubs.run("quick");
    assert_eq!(output.status.code(), Some(0));
    let record = assert_record(
        &output,
        "quick",
        "debug 5/0/3 skips 1 goldens-moved 0 exit 0",
    );
    assert!(record.contains("tests: 5/0/3\ngate-skip count: 1\n```text\ngate-skip: stub_suite unset fixture variable\n```"));
    assert!(
        record.contains(" test --offline --locked --workspace --no-fail-fast\nenvironment: none\n")
    );
    assert!(!record.contains("## release\n"));
}

#[test]
fn full_fails_on_release_skip() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("skip");
    let output = stubs.run("full");
    assert_eq!(output.status.code(), Some(1));
    let record = assert_record(
        &output,
        "full",
        "debug 5/0/3 release 5/0/3 skips 1/1 clippy 7/18/13 goldens-moved 0 exit 1",
    );
    let release = record.split("## release\n").nth(1).unwrap();
    assert!(release.contains(" test --offline --locked --workspace --no-fail-fast --release\nenvironment: SUBSCRIPT_FULL_INTERPRETER_SWEEP=1\n"));
    assert!(release.contains("tests: 5/0/3\ngate-skip count: 1\n```text\ngate-skip: stub_suite unset fixture variable\n```"));
    assert!(release.contains("release sweep: 1"));
    assert!(release.contains("## hygiene\ncommand: tools/hygiene.sh\n"));
}

#[test]
fn quick_stops_at_build_warning() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("warning");
    let output = stubs.run("quick");
    assert_eq!(output.status.code(), Some(1));
    let record = assert_record(
        &output,
        "quick",
        "debug 0/0/0 skips 0 goldens-moved 0 exit 1",
    );
    assert!(
        record.contains(" build --offline --locked --workspace --all-targets\nenvironment: none\n")
    );
    assert!(record.contains("stderr:\n```text\nwarning: build warning\n"));
    assert!(!record.contains("## debug\n"));
    assert!(!record.contains(" test --offline"));
}

#[test]
fn full_continues_after_failed_tests() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("failed");
    let output = stubs.run("full");
    assert_eq!(output.status.code(), Some(1));
    let record = assert_record(
        &output,
        "full",
        "debug 2/1/0 release 2/1/0 skips 0/0 clippy 7/18/13 goldens-moved 0 exit 1",
    );
    assert!(record.contains("exit status: 1\ntests: 2/1/0\ngate-skip count: 0\n"));
    assert!(record.contains("release sweep: 1"));
    assert!(record.contains("stdout:\n```text\ntsc ran\n"));
    assert!(record.contains("## hygiene\ncommand: tools/hygiene.sh\nenvironment: none\n"));
}

#[test]
fn full_fails_above_codegen_clippy_baseline() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("clippy");
    let output = stubs.run("full");
    assert_eq!(output.status.code(), Some(1));
    let record = assert_record(
        &output,
        "full",
        "debug 5/0/3 release 5/0/3 skips 0/0 clippy 7/18/14 goldens-moved 0 exit 1",
    );
    assert!(record
        .contains(" clippy --offline --locked --workspace --all-targets\nenvironment: none\n"));
    assert!(record.contains("warning: `subscript-codegen` (lib) generated 14 warnings"));
    assert!(record.contains("## hygiene\ncommand: tools/hygiene.sh\n"));
}

#[test]
fn unknown_shape_has_usage_and_exit_two() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("unused");
    let output = stubs.run("unknown");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "usage: tools/gate.sh <quick|full>\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn full_stops_at_build_warning_without_unrun_fields() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("warning");
    let output = stubs.run("full");
    assert_eq!(output.status.code(), Some(1));
    let record = assert_record(
        &output,
        "full",
        "debug 0/0/0 skips 0 goldens-moved 0 exit 1",
    );
    assert!(record.contains("stderr:\n```text\nwarning: build warning\n"));
    assert!(!record.contains("## debug\n"));
    assert!(!record.contains("## release\n"));
    assert!(!record.contains("## clippy\n"));
}

#[test]
fn full_passes_at_all_clippy_baselines() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("plain");
    let output = stubs.run("full");
    assert_eq!(output.status.code(), Some(0));
    let record = assert_record(
        &output,
        "full",
        "debug 5/0/3 release 5/0/3 skips 0/0 clippy 7/18/13 goldens-moved 0 exit 0",
    );
    assert!(record.contains("## hygiene\ncommand: tools/hygiene.sh\nenvironment: none\n"));
    assert!(record.contains("tests: 5/0/3\ngate-skip count: 0\n"));
}

#[test]
fn full_fails_above_compiler_clippy_baseline() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("clippy-compiler");
    let output = stubs.run("full");
    assert_eq!(output.status.code(), Some(1));
    let record = assert_record(
        &output,
        "full",
        "debug 5/0/3 release 5/0/3 skips 0/0 clippy 8/18/13 goldens-moved 0 exit 1",
    );
    assert!(record.contains("warning: `subscript-compiler` (lib) generated 8 warnings"));
    assert!(record.contains("## hygiene\ncommand: tools/hygiene.sh\n"));
}

#[test]
fn full_fails_above_runtime_clippy_baseline() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("clippy-runtime");
    let output = stubs.run("full");
    assert_eq!(output.status.code(), Some(1));
    let record = assert_record(
        &output,
        "full",
        "debug 5/0/3 release 5/0/3 skips 0/0 clippy 7/19/13 goldens-moved 0 exit 1",
    );
    assert!(record.contains("warning: `subscript-runtime` (lib) generated 19 warnings"));
    assert!(record.contains("## hygiene\ncommand: tools/hygiene.sh\n"));
}

#[test]
fn moved_goldens_are_listed_without_failure() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("plain");
    stubs.write(
        "git",
        r#"#!/bin/sh
case "$*" in
    'rev-parse HEAD') echo '0123456789012345678901234567890123456789' ;;
    'status --porcelain')
        echo ' M corpus/accept/x.expected'
        echo 'D  codegen/tests/lir-goldens/corpus.txt'
        echo ' M codegen/src/lib.rs'
        ;;
    *) exit 93 ;;
esac
"#,
    );
    let output = stubs
        .command("full")
        .env("GIT", stubs.dir.join("git"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let record = assert_record_with_identity(
        &output,
        "full",
        "debug 5/0/3 release 5/0/3 skips 0/0 clippy 7/18/13 goldens-moved 2 exit 0",
        "0123456789012345678901234567890123456789",
        "dirty:3",
    );
    assert!(record.contains("dirty: 3\n```text\n M corpus/accept/x.expected\nD  codegen/tests/lir-goldens/corpus.txt\n M codegen/src/lib.rs\n```"));
    let goldens = record
        .split("## Modified or deleted goldens\n")
        .nth(1)
        .unwrap();
    assert!(goldens.starts_with(
        "```text\n M corpus/accept/x.expected\nD  codegen/tests/lir-goldens/corpus.txt\n```\n"
    ));
    assert!(!goldens.contains("codegen/src/lib.rs"));
}

struct RunningChild(std::process::Child);

impl Drop for RunningChild {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            self.0.kill().expect("stop the unfinished stub run");
        }
        self.0.wait().expect("reap the stub run");
    }
}

#[test]
fn term_during_debug_deletes_the_partial_record() {
    let _guard = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stubs = Stubs::new("sleep");
    let before = gate_records();
    let mut child = RunningChild(
        stubs
            .command("full")
            .env("TMPDIR", &stubs.dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    // The marker proves that TERM reaches the sleep inside the debug step.
    loop {
        if !gate_records()
            .difference(&before)
            .collect::<Vec<_>>()
            .is_empty()
            && stubs.dir.join("test-started").exists()
        {
            break;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "stub exited before its test step"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "stub test step did not start"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(Command::new("kill")
        .arg("-TERM")
        .arg(child.0.id().to_string())
        .status()
        .unwrap()
        .success());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "TERM did not end the stub run"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert!(!status.success());
    // Assert before Stubs::drop, so test cleanup cannot hide a script defect.
    assert_eq!(gate_records(), before);
    assert!(!std::fs::read_dir(&stubs.dir).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("subscript-gate.")
    }));
}
