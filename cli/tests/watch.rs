//! Unit-level watch transitions plus one polling end-to-end session.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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
  print(\"partial output is discarded\");
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
            assert!(call.trap.is_some());
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

#[derive(Clone)]
struct Capture(Arc<(Mutex<Vec<u8>>, Condvar)>);

impl Capture {
    fn reader<R: Read + Send + 'static>(mut reader: R) -> (Self, JoinHandle<()>) {
        let capture = Self(Arc::new((Mutex::new(Vec::new()), Condvar::new())));
        let writer = capture.clone();
        let handle = std::thread::spawn(move || {
            let mut chunk = [0_u8; 1024];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        let (data, ready) = &*writer.0;
                        if let Ok(mut data) = data.lock() {
                            data.extend_from_slice(&chunk[..count]);
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
        let deadline = Instant::now() + Duration::from_secs(20);
        let (data, ready) = &*self.0;
        let mut data = data.lock().map_err(|_| "capture lock poisoned")?;
        loop {
            let actual = data
                .windows(needle.len())
                .filter(|window| *window == needle)
                .count();
            if actual >= count {
                return Ok(());
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(format!(
                    "timed out waiting for {:?} {count} time(s); captured:\n{}",
                    String::from_utf8_lossy(needle),
                    String::from_utf8_lossy(&data)
                ));
            }
            let remaining = deadline.saturating_duration_since(now);
            let (next, _) = ready
                .wait_timeout(data, remaining)
                .map_err(|_| "capture wait poisoned")?;
            data = next;
        }
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        self.0
             .0
            .lock()
            .map(|data| data.clone())
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
