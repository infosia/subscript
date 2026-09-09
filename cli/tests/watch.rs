//! Unit-level watch transitions plus one polling end-to-end session.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use subscript_cli::watch::{WatchOutcome, WatchSession};
use subscript_compiler::{check_program, render_diagnostics, SourceFile};

fn files(source: &str) -> Vec<SourceFile> {
    vec![SourceFile::new("live.ts", source)]
}

fn call_output(outcome: WatchOutcome) -> Result<Vec<u8>, String> {
    match outcome {
        WatchOutcome::Started(call) | WatchOutcome::Swapped(call) => {
            if let Some(trap) = call.trap {
                Err(format!("unexpected trap: {trap}"))
            } else {
                Ok(call.output)
            }
        }
        other => Err(format!("expected a program call, got {other:?}")),
    }
}

const COUNTER_V1: &str = "\
let counter: i32 = 0;
function editable(): i32 {
  return 10;
}
export function main(): void {
  counter += 1;
  print(`${counter}:old=${editable()}`);
}
";

const COUNTER_V2: &str = "\
let counter: i32 = 0;
function editable(): i32 {
  return 200;
}
export function main(): void {
  counter += 1;
  print(`${counter}:new=${editable()}`);
}
";

#[test]
fn body_edit_runs_new_behavior_with_the_live_context() -> Result<(), String> {
    let mut watch = WatchSession::new(false);
    let started = watch.step(&files(COUNTER_V1));
    assert!(started.diagnostics.is_empty());
    assert!(started.warnings.is_empty());
    assert_eq!(call_output(started.outcome)?, b"1:old=10\n");

    assert!(matches!(
        watch.step(&files(COUNTER_V1)).outcome,
        WatchOutcome::Unchanged
    ));

    let swapped = watch.step(&files(COUNTER_V2));
    assert!(swapped.diagnostics.is_empty());
    assert!(swapped.warnings.is_empty());
    assert_eq!(call_output(swapped.outcome)?, b"2:new=200\n");
    Ok(())
}

const SHAPE_V1: &str = "\
class Box {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}
let counter: i32 = 0;
export function main(): void {
  counter += 1;
  print(`old ${counter}`);
}
";

const SHAPE_CHANGED: &str = "\
class Box {
  value: i32;
  extra: i32;
  constructor(value: i32) {
    this.value = value;
    this.extra = 0;
  }
}
let counter: i32 = 0;
export function main(): void {
  counter += 1;
  print(`old ${counter}`);
}
";

#[test]
fn declaration_refusal_names_the_declaration_then_a_body_edit_swaps() -> Result<(), String> {
    let mut watch = WatchSession::new(false);
    assert_eq!(
        call_output(watch.step(&files(SHAPE_V1)).outcome)?,
        b"old 1\n"
    );

    match watch.step(&files(SHAPE_CHANGED)).outcome {
        WatchOutcome::Refused { declaration } => assert_eq!(declaration, "class Box"),
        other => return Err(format!("expected declaration refusal, got {other:?}")),
    }

    let body = SHAPE_V1.replace("old ${counter}", "accepted ${counter}");
    assert_eq!(
        call_output(watch.step(&files(&body)).outcome)?,
        b"accepted 2\n"
    );
    Ok(())
}

#[test]
fn diagnostics_leave_the_old_program_live_and_a_fix_runs() -> Result<(), String> {
    let mut watch = WatchSession::new(false);
    assert_eq!(
        call_output(watch.step(&files(COUNTER_V1)).outcome)?,
        b"1:old=10\n"
    );

    let broken = COUNTER_V1.replace(
        "counter += 1;",
        "const invalid: number = 1;\n  counter += invalid as i32;",
    );
    let rejected = watch.step(&files(&broken));
    assert!(matches!(rejected.outcome, WatchOutcome::WaitingForFix));
    assert!(!rejected.diagnostics.is_empty());

    let fixed = COUNTER_V1.replace("old=${editable()}", "fixed=${editable()}");
    assert_eq!(
        call_output(watch.step(&files(&fixed)).outcome)?,
        b"2:fixed=10\n"
    );
    Ok(())
}

#[test]
fn a_trap_ends_one_call_but_not_the_watch_session() -> Result<(), String> {
    let trapping = "\
let calls: i32 = 0;
export function main(): void {
  calls += 1;
  print(\"output before the trap\");
  const empty: i32[] = [];
  empty.pop();
}
";
    let fixed = "\
let calls: i32 = 0;
export function main(): void {
  calls += 1;
  print(`recovered ${calls}`);
}
";
    let mut watch = WatchSession::new(false);
    match watch.step(&files(trapping)).outcome {
        WatchOutcome::Started(call) => {
            assert!(call.output.is_empty());
            let trap = call.trap.ok_or("expected trap report")?;
            assert_eq!(trap.stdout, b"output before the trap\n");
        }
        other => return Err(format!("expected trapped start, got {other:?}")),
    }
    assert_eq!(
        call_output(watch.step(&files(fixed)).outcome)?,
        b"recovered 2\n"
    );
    Ok(())
}

#[test]
fn deny_warnings_waits_without_starting_then_accepts_a_clean_edit() -> Result<(), String> {
    let warned = "\
class Token {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}
export function main(): void {
  for (let i: i32 = 0; i < 2; i += 1) {
    const token: Token = new Token(i);
    print(`${token.value}`);
  }
}
";
    let clean = warned.replace(
        "  for (let i: i32 = 0; i < 2; i += 1) {\n    const token: Token = new Token(i);\n    print(`${token.value}`);\n  }",
        "  print(\"clean\");",
    );
    let mut watch = WatchSession::new(true);
    let denied = watch.step(&files(warned));
    assert!(matches!(denied.outcome, WatchOutcome::WaitingForFix));
    assert!(!denied.warnings.is_empty());

    assert_eq!(call_output(watch.step(&files(&clean)).outcome)?, b"clean\n");
    Ok(())
}

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "subscript-cli-watch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("create {}: {error}", path.display()))?;
        Ok(Self(path))
    }

    fn write(&self, relative: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(relative);
        std::fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct CaptureState {
    bytes: Vec<u8>,
    ended: bool,
}

#[derive(Clone)]
struct Capture(Arc<(Mutex<CaptureState>, Condvar)>);

impl Capture {
    fn reader<R: Read + Send + 'static>(mut reader: R) -> (Self, JoinHandle<()>) {
        let capture = Self(Arc::new((
            Mutex::new(CaptureState::default()),
            Condvar::new(),
        )));
        let writer = capture.clone();
        let handle = std::thread::spawn(move || {
            let mut chunk = [0_u8; 1024];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => {
                        let (data, ready) = &*writer.0;
                        if let Ok(mut data) = data.lock() {
                            data.ended = true;
                            ready.notify_all();
                        }
                        break;
                    }
                    Ok(count) => {
                        let (data, ready) = &*writer.0;
                        if let Ok(mut data) = data.lock() {
                            data.bytes.extend_from_slice(&chunk[..count]);
                            ready.notify_all();
                        } else {
                            break;
                        }
                    }
                }
            }
        });
        (capture, handle)
    }

    fn wait_for_count(&self, needle: &[u8], count: usize) -> Result<(), String> {
        let (data, ready) = &*self.0;
        let mut data = data.lock().map_err(|_| "capture lock poisoned")?;
        loop {
            let actual = data
                .bytes
                .windows(needle.len())
                .filter(|window| *window == needle)
                .count();
            if actual >= count {
                return Ok(());
            }
            if data.ended {
                return Err(format!(
                    "input ended before {:?} reached {count} time(s); saw {actual}; captured:\n{}",
                    String::from_utf8_lossy(needle),
                    String::from_utf8_lossy(&data.bytes)
                ));
            }
            data = ready.wait(data).map_err(|_| "capture wait poisoned")?;
        }
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        self.0
             .0
            .lock()
            .map(|data| data.bytes.clone())
            .map_err(|_| "capture lock poisoned".to_string())
    }
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// §102: an exited child supplies a fact, without a latency assertion.
#[test]
fn capture_reports_early_child_exit() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write(
        "main.ts",
        "export function main(): void { print(\"needle\"); print(\"early output\"); }",
    )?;
    let mut child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_subscript"))
            .current_dir(&directory.0)
            .args(["run", "main.ts"])
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| format!("spawn early exit: {error}"))?,
    );
    let (capture, reader) = Capture::reader(child.0.stdout.take().ok_or("missing stdout")?);
    let result = capture.wait_for_count(b"needle", 2);
    assert!(child.0.wait().map_err(|error| error.to_string())?.success());
    reader.join().map_err(|_| "capture reader panicked")?;
    assert_eq!(
        result,
        Err("input ended before \"needle\" reached 2 time(s); saw 1; captured:\nneedle\nearly output\n".to_string())
    );
    // End of input must also be visible to a later waiter.
    assert_eq!(
        capture.wait_for_count(b"missing", 1),
        Err("input ended before \"missing\" reached 1 time(s); saw 0; captured:\nneedle\nearly output\n".to_string())
    );
    capture.wait_for_count(b"needle", 1)?;
    Ok(())
}

#[test]
fn capture_reports_read_error() -> Result<(), String> {
    struct FailedReader;
    impl Read for FailedReader {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("reader failed"))
        }
    }
    let (capture, reader) = Capture::reader(FailedReader);
    assert_eq!(
        capture.wait_for_count(b"needle", 1),
        Err("input ended before \"needle\" reached 1 time(s); saw 0; captured:\n".to_string())
    );
    reader.join().map_err(|_| "capture reader panicked")?;
    Ok(())
}

const HELPER_V1: &str = "\
export function helper(): i32 {
  return 10;
}
";

const HELPER_V2: &str = "\
export function helper(): i32 {
  return 200;
}
";

const ENTRY_V1: &str = "\
import { helper } from \"./helper\";
class Box {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}
let counter: i32 = 0;
export function main(): void {
  counter += 1;
  print(`${counter}:${helper()}`);
}
";

const ENTRY_DECLARATION_EDIT: &str = "\
import { helper } from \"./helper\";
class Box {
  value: i32;
  extra: i32;
  constructor(value: i32) {
    this.value = value;
    this.extra = 0;
  }
}
let counter: i32 = 0;
export function main(): void {
  counter += 1;
  print(`${counter}:${helper()}`);
}
";

const ENTRY_BODY_EDIT: &str = "\
import { helper } from \"./helper\";
class Box {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}
let counter: i32 = 0;
export function main(): void {
  counter += 1;
  print(`body ${counter}:${helper()}`);
}
";

const ENTRY_BROKEN: &str = "\
import { helper } from \"./helper\";
class Box {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}
let counter: i32 = 0;
export function main(): void {
  counter += 1;
  const bad: number = 1;
  print(`broken ${counter}:${helper()}:${bad}`);
}
";

const ENTRY_FIXED: &str = "\
import { helper } from \"./helper\";
class Box {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}
let counter: i32 = 0;
export function main(): void {
  counter += 1;
  print(`fixed ${counter}:${helper()}`);
}
";

#[test]
fn spawned_watch_polls_imports_and_keeps_stdout_program_only() -> Result<(), String> {
    let directory = TestDir::new()?;
    directory.write("main.ts", ENTRY_V1)?;
    directory.write("helper.ts", HELPER_V1)?;

    let mut child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_subscript"))
            .current_dir(&directory.0)
            .arg("run")
            .arg("--watch")
            .arg("main.ts")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("spawn watch: {error}"))?,
    );
    let child_stdout = child.0.stdout.take().ok_or("missing child stdout")?;
    let child_stderr = child.0.stderr.take().ok_or("missing child stderr")?;
    let (stdout, stdout_thread) = Capture::reader(child_stdout);
    let (stderr, stderr_thread) = Capture::reader(child_stderr);

    let result = (|| {
        stderr.wait_for_count(b"watch: started\n", 1)?;
        stdout.wait_for_count(b"1:10\n", 1)?;

        // Only the imported sibling changes here; it must be in the polled
        // loaded-file set.
        directory.write("helper.ts", HELPER_V2)?;
        stderr.wait_for_count(b"watch: swapped\n", 1)?;
        stdout.wait_for_count(b"2:200\n", 1)?;

        directory.write("main.ts", ENTRY_DECLARATION_EDIT)?;
        stderr.wait_for_count(b"watch: refused: class Box\n", 1)?;

        directory.write("main.ts", ENTRY_BODY_EDIT)?;
        stderr.wait_for_count(b"watch: swapped\n", 2)?;
        stdout.wait_for_count(b"body 3:200\n", 1)?;

        directory.write("main.ts", ENTRY_BROKEN)?;
        stderr.wait_for_count(b"watch: waiting for a fix\n", 1)?;
        stderr.wait_for_count(b"error[S007]", 1)?;

        directory.write("main.ts", ENTRY_FIXED)?;
        stderr.wait_for_count(b"watch: swapped\n", 3)?;
        stdout.wait_for_count(b"fixed 4:200\n", 1)?;
        Ok::<(), String>(())
    })();

    let _ = child.0.kill();
    let _ = child.0.wait();
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    result?;

    let captured_stdout = stdout.bytes()?;
    let captured_stderr = stderr.bytes()?;
    assert_eq!(captured_stdout, b"1:10\n2:200\nbody 3:200\nfixed 4:200\n");

    let broken_files = [
        SourceFile::new("main.ts", ENTRY_BROKEN),
        SourceFile::new("helper.ts", HELPER_V2),
    ];
    let diagnostics = check_program(&broken_files).expect_err("broken edit must be rejected");
    let expected_stderr = format!(
        concat!(
            "watch: started\n",
            "watch: swapped\n",
            "watch: refused: class Box\n",
            "watch: swapped\n",
            "{}\n",
            "watch: waiting for a fix\n",
            "watch: swapped\n",
        ),
        render_diagnostics(&broken_files, &diagnostics)
    );
    assert_eq!(captured_stderr, expected_stderr.as_bytes());
    Ok(())
}

// cli.md §13: every trapped watch invocation forwards the committed output.
#[test]
fn spawned_watch_preserves_stdout_before_each_trap() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let source = std::fs::read_to_string(root.join("corpus/trap/t03-loop-stops-at-fault.ts"))
        .map_err(|error| error.to_string())?;
    let expected = std::fs::read(root.join("corpus/trap/t03-loop-stops-at-fault.expected"))
        .map_err(|error| error.to_string())?;
    let directory = TestDir::new()?;
    directory.write("main.ts", &source)?;
    let mut child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_subscript"))
            .current_dir(&directory.0)
            .args(["run", "--watch", "main.ts"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("spawn watch: {error}"))?,
    );
    let (stdout, stdout_thread) = Capture::reader(child.0.stdout.take().ok_or("missing stdout")?);
    let (stderr, stderr_thread) = Capture::reader(child.0.stderr.take().ok_or("missing stderr")?);
    let result = (|| {
        stderr.wait_for_count(b"index-out-of-bounds", 1)?;
        stdout.wait_for_count(&expected, 1)?;
        // This body edit preserves output and forces another trapped call.
        directory.write("main.ts", &source.replace("while (i < 3)", "while (i < 4)"))?;
        stderr.wait_for_count(b"watch: swapped\n", 1)?;
        stderr.wait_for_count(b"index-out-of-bounds", 2)?;
        stdout.wait_for_count(&expected, 2)?;
        Ok::<(), String>(())
    })();
    let _ = child.0.kill();
    let _ = child.0.wait();
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    result?;
    assert_eq!(stdout.bytes()?, expected.repeat(2));
    Ok(())
}
