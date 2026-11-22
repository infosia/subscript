//! `Worker.receive()` through both lowerings (`specs/blocks/compiler.md` §349).
//!
//! The first program has every message in the outbox, and the Worker
//! ended, before each receive, so each receive completes at its call
//! (§349 rules 2 and 5). The later programs have receives that wait, so
//! each driver waits on the signal of §349 rule 11.

use std::sync::mpsc;
use std::time::Duration;

use subscript_codegen::lir::{lower_module, verify_module};
use subscript_codegen::{run_c_aot, run_jit, ReloadSession, RunError, TrapReport};
use subscript_compiler::{check_program, lir, SourceFile, Type};
use subscript_runtime::TrapKind;

const PROGRAM: &str = r#"class Msg { v: i32; constructor(v: i32) { this.v = v; } }
function echo(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  let m: Msg | null = inbox.wait();
  while (m !== null) { outbox.post(new Msg(m.v + 1)); m = inbox.wait(); }
}
export async function main(): Promise<void> {
  const w: Worker<Msg, Msg> = Worker.spawn(echo);
  w.post(new Msg(1));
  w.post(new Msg(2));
  w.post(new Msg(3));
  w.close();
  w.join();
  const first: Promise<Msg | null> = w.receive();
  const second: Promise<Msg | null> = w.receive();
  const b: Msg | null = await second;
  const a: Msg | null = await first;
  if (a !== null && b !== null) { print(`${a.v} ${b.v}`); }
  const both: (Msg | null)[] = await Promise.all([w.receive(), w.receive()]);
  const c: Msg | null = both[0];
  print(c !== null ? `${c.v}` : "none");
  print(both[1] === null ? "ended" : "more");
  const after: Msg | null = await w.receive();
  print(after === null ? "ended" : "more");
}
"#;

const EXPECTED: &[u8] = b"2 3\n4\nended\nended\n";

fn files() -> Vec<SourceFile> {
    vec![SourceFile::entry("worker-receive.ts", PROGRAM)]
}

fn run(run: fn(&[SourceFile]) -> Result<Vec<u8>, RunError>) -> Vec<u8> {
    run(&files()).unwrap_or_else(|error| panic!("run: {error:?}"))
}

// Cost: one check and one LIR lowering; no native compilation.
#[test]
fn receive_lowers_to_an_allocating_worker_intrinsic_with_an_owned_handle() {
    let hir = check_program(&files()).expect("checker");
    let module = lower_module(&hir).expect("lower");
    verify_module(&module).expect("verified");
    let operation = module
        .intrinsic_operations
        .iter()
        .find(|operation| {
            operation.family == lir::IntrinsicFamily::Worker && operation.semantic_name == "Receive"
        })
        .expect("the Receive operation is in the table");
    // Appended after the eight earlier Worker operations.
    assert_eq!(operation.operation, 8);
    assert_eq!(
        operation.runtime_symbol(),
        Some("subscript_rt_worker_receive")
    );
    assert!(operation.can_allocate);
    let calls: Vec<_> = module
        .functions
        .iter()
        .flat_map(|function| {
            function.blocks.iter().flat_map(move |block| {
                block
                    .instructions
                    .iter()
                    .map(move |instruction| (function, instruction))
            })
        })
        .filter(|(_, instruction)| {
            matches!(
                &instruction.kind,
                lir::InstructionKind::Call(lir::CallTarget {
                    kind: lir::CallTargetKind::Intrinsic(lir::Intrinsic {
                        family: lir::IntrinsicFamily::Worker,
                        operation: 8,
                        ..
                    }),
                    ..
                })
            )
        })
        .collect();
    assert_eq!(calls.len(), 5, "one call for each receive of the program");
    for (function, instruction) in calls {
        // The one operand is the Worker; the position of the call is the
        // position of the instruction, which each tier passes as `pos_id`.
        assert_eq!(instruction.operands.len(), 1);
        assert!(instruction.pos.line > 0);
        let result = instruction.result.expect("a receive has a result");
        let value = &function.values[result.0 as usize];
        assert!(
            matches!(
                &value.ty,
                lir::ValueType::Data(Type::AsyncHandle(inner))
                    if matches!(inner.as_ref(), Type::Nullable(..))
            ),
            "{:?}",
            value.ty
        );
        // §349 rule 1: the caller owns the returned handle, as it owns the
        // handle of an async call.
        assert!(value.fresh_owner, "the receive result is a fresh owner");
        assert!(instruction.kind.produces_fresh_async_owner());
        // §349 rule 6: the Context can trap in the call, so the call
        // carries a call trap site that the tiers check after it.
        assert!(
            instruction
                .traps
                .iter()
                .any(|trap| trap.kind == lir::TrapKind::Call),
            "{:?}",
            instruction.traps
        );
    }
}

// Cost: one dev JIT run of a 27-line program; one Worker thread.
#[test]
fn dev_receive_gives_each_message_in_call_sequence_then_null() {
    assert_eq!(run(run_jit), EXPECTED);
}

// Cost: one ship-tier build and run; 1 C build.
#[test]
fn ship_receive_gives_each_message_in_call_sequence_then_null() {
    assert_eq!(run(run_c_aot), EXPECTED);
}

/// The bound of each run in this file that can wait (§349 rule 11). A run
/// that does not end in this time fails its test with a message.
const RUN_LIMIT: Duration = Duration::from_secs(60);

/// Runs `work` on a thread of its own and returns its result. Fails the
/// test if `work` does not end in [`RUN_LIMIT`].
fn bounded<T: Send + 'static>(what: &str, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    match receiver.recv_timeout(RUN_LIMIT) {
        Ok(value) => value,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("{what} did not end in {RUN_LIMIT:?}: a wait of §349 rule 11 did not return")
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("{what} stopped with a panic"),
    }
}

/// Each receive of this program waits: the Worker has no message when
/// `receive()` is called. The run ends only if the driver waits (§349
/// rule 11) and steps again.
const WAIT_PROGRAM: &str = r#"class Msg { v: i32; constructor(v: i32) { this.v = v; } }
function echo(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  let m: Msg | null = inbox.wait();
  while (m !== null) { outbox.post(new Msg(m.v + 1)); m = inbox.wait(); }
}
export async function main(): Promise<void> {
  const w: Worker<Msg, Msg> = Worker.spawn(echo);
  w.post(new Msg(1));
  w.post(new Msg(2));
  const a: Msg | null = await w.receive();
  const b: Msg | null = await w.receive();
  if (a !== null && b !== null) { print(`${a.v} ${b.v}`); }
  w.close();
  const end: Msg | null = await w.receive();
  print(end === null ? "ended" : "more");
  w.join();
}
"#;

const WAIT_EXPECTED: &[u8] = b"2 3\nended\n";

fn wait_files() -> Vec<SourceFile> {
    vec![SourceFile::entry("worker-receive-wait.ts", WAIT_PROGRAM)]
}

// Cost: one dev JIT run; one Worker thread; the driver waits.
#[test]
fn dev_driver_waits_for_a_receive_and_steps_again() {
    let stdout = bounded("the dev JIT run", || run_jit(&wait_files()));
    assert_eq!(stdout.expect("dev JIT run"), WAIT_EXPECTED);
}

// Cost: one ship-tier build and run; 1 C build.
#[test]
fn ship_main_waits_for_a_receive_and_steps_again() {
    let stdout = bounded("the ship-tier run", || run_c_aot(&wait_files()));
    assert_eq!(stdout.expect("ship-tier run"), WAIT_EXPECTED);
}

// Cost: one reload session and one `run_main`; one Worker thread.
#[test]
fn reload_run_main_waits_for_a_receive_and_steps_again() {
    let stdout = bounded("ReloadSession::run_main", || {
        let mut session = ReloadSession::new(&wait_files()).expect("reload session");
        session.run_main().map(|()| session.take_output())
    });
    assert_eq!(stdout.expect("run_main"), WAIT_EXPECTED);
}

/// A Worker that posts one message and then reads out of range. `SECOND`
/// is the operation after the first receive.
const TRAP_PROGRAM: &str = r#"class Msg { value: i32; constructor(value: i32) { this.value = value; } }
function crash(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  const m: Msg | null = inbox.wait();
  if (m !== null) {
    outbox.post(new Msg(m.value));
    const xs: i32[] = [1];
    print(`${xs[m.value]}`);
  }
}
export async function main(): Promise<void> {
  const w: Worker<Msg, Msg> = Worker.spawn(crash);
  w.post(new Msg(5));
  const a: Msg | null = await w.receive();
  if (a !== null) { print(`got ${a.value}`); }
  SECOND
  print("after");
}
"#;

fn trap_files(second: &str) -> Vec<SourceFile> {
    vec![SourceFile::entry(
        "worker-receive-trap.ts",
        TRAP_PROGRAM.replace("SECOND", second),
    )]
}

/// The line and column of the read `xs[m.value]` in [`TRAP_PROGRAM`], found
/// in the source text.
fn trap_site() -> (u32, u32) {
    let (index, line) = TRAP_PROGRAM
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("xs[m.value]"))
        .expect("the read is in the program");
    let col = line.find("xs[m.value]").expect("the read") + 1;
    (
        u32::try_from(index + 1).expect("a line"),
        u32::try_from(col).expect("a column"),
    )
}

/// §349 rule 6: the second receive traps the Context as `join()` does, at
/// the trap site of the Worker, after the first receive gave the message.
fn assert_receive_trap_equals_join_trap(
    tier: &'static str,
    run: fn(&[SourceFile]) -> Result<Vec<u8>, RunError>,
) {
    let trap = |second: &'static str| -> TrapReport {
        match bounded(tier, move || run(&trap_files(second))) {
            Err(RunError::Trap(report)) => report,
            other => panic!("{tier} with `{second}`: not a trap: {other:?}"),
        }
    };
    let receive =
        trap("const b: Msg | null = await w.receive(); print(b === null ? \"null\" : \"msg\");");
    let join = trap("w.join();");
    eprintln!(
        "{tier}: receive {:?} {:?} {}:{}:{}; join {:?} {:?} {}:{}:{}",
        receive.rule,
        receive.message,
        receive.pos.file,
        receive.pos.line,
        receive.pos.col,
        join.rule,
        join.message,
        join.pos.file,
        join.pos.line,
        join.pos.col
    );
    assert_eq!(receive.rule, TrapKind::WorkerTrapped);
    assert_eq!(receive.stdout, b"got 5\n");
    let (line, col) = trap_site();
    assert_eq!((receive.pos.line, receive.pos.col), (line, col));
    assert_eq!(receive.rule, join.rule);
    assert_eq!(receive.message, join.message);
    assert_eq!(receive.pos, join.pos);
    assert_eq!(receive.stdout, join.stdout);
}

// Cost: two dev JIT runs; one Worker thread each.
#[test]
fn dev_receive_after_a_worker_trap_traps_as_join_does() {
    assert_receive_trap_equals_join_trap("the dev JIT run", run_jit);
}

// Cost: two ship-tier builds and runs; 2 C builds.
#[test]
fn ship_receive_after_a_worker_trap_traps_as_join_does() {
    assert_receive_trap_equals_join_trap("the ship-tier run", run_c_aot);
}

/// Four tasks that do not finish, and a pending count of zero
/// (`interpreter/inspection_tests.rs`). No receive waits, so the wait of
/// §349 rule 11 returns at once and the run ends with no output.
const RING_PROGRAM: &str = "const ring: Promise<void>[] = [];\nasync function job(next: i32): Promise<void> { await Context.suspend(); await ring[next]; }\nexport async function main(): Promise<void> { ring.push(job(1)); ring.push(job(2)); ring.push(job(0)); await ring[0]; print(\"done\"); }";

fn ring_files() -> Vec<SourceFile> {
    vec![SourceFile::entry("ring.ts", RING_PROGRAM)]
}

// Cost: one dev JIT run, one reload session, and one ship-tier build and
// run; 1 C build.
#[test]
fn a_run_with_no_worker_and_unfinished_tasks_ends_in_each_driver() {
    let dev = bounded("the dev JIT run", || run_jit(&ring_files()));
    assert_eq!(dev.expect("dev JIT run"), b"");
    let reload = bounded("ReloadSession::run_main", || {
        let mut session = ReloadSession::new(&ring_files()).expect("reload session");
        session
            .run_main()
            .map(|()| (session.take_output(), session.async_unfinished()))
    });
    assert_eq!(reload.expect("run_main"), (Vec::new(), 4));
    let ship = bounded("the ship-tier run", || run_c_aot(&ring_files()));
    assert_eq!(ship.expect("ship-tier run"), b"");
}

/// The receive is called before the Worker has an input message, so it
/// waits at its call, and no step runs before the host calls the wait.
const SESSION_WAIT_PROGRAM: &str = r#"class Msg { v: i32; constructor(v: i32) { this.v = v; } }
function echo(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  let m: Msg | null = inbox.wait();
  while (m !== null) { outbox.post(new Msg(m.v + 1)); m = inbox.wait(); }
}
export async function main(): Promise<void> {
  const w: Worker<Msg, Msg> = Worker.spawn(echo);
  const h: Promise<Msg | null> = w.receive();
  w.post(new Msg(1));
  w.close();
  const a: Msg | null = await h;
  print(a !== null ? `${a.v}` : "none");
  w.join();
}
"#;

/// `ReloadSession::async_wait` (§349 rule 11): false at once with no
/// receive that waits; true after the Worker posts, and the next step
/// delivers the message.
// Cost: two reload sessions; one Worker thread.
#[test]
fn reload_session_async_wait_returns_after_a_post_and_at_once_with_no_receive() {
    let control = bounded("ReloadSession::async_wait with no Worker", || {
        let mut session = ReloadSession::new(&ring_files()).expect("reload session");
        session.run_main().expect("run_main");
        session.async_wait()
    });
    assert!(!control, "no receive waits, so the wait returns false");
    let (waited, pending, output, after) = bounded("ReloadSession::async_wait", || {
        let files = vec![SourceFile::entry("session-wait.ts", SESSION_WAIT_PROGRAM)];
        let mut session = ReloadSession::new(&files).expect("reload session");
        session.call_main().expect("kick main");
        assert_eq!(session.take_output(), b"", "the receive waits");
        let waited = session.async_wait();
        let pending = session.async_pending();
        while session.async_pending() != 0 {
            session.async_step().expect("step");
        }
        (waited, pending, session.take_output(), session.async_wait())
    });
    assert!(waited, "the wait returns true after the post");
    assert_ne!(pending, 0, "the receive can complete after the wait");
    assert_eq!(output, b"2\n");
    assert!(!after, "no receive waits after the delivery");
}

/// A receive handle that a global holds across a reload. `start` calls
/// the receive and joins the Worker; `finish` awaits the handle after the
/// reload, so the receive waited through `join()` and the reload.
const RELOAD_PROGRAM: &str = r#"class Msg {
  value: i32;
  constructor(value: i32) { this.value = value; }
}
function plusOne(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {
  const m: Msg | null = inbox.wait();
  if (m !== null) { outbox.post(new Msg(m.value + 1)); }
}
const pending: Promise<Msg | null>[] = [];
export function start(): void {
  const w: Worker<Msg, Msg> = Worker.spawn(plusOne);
  pending.push(w.receive());
  w.post(new Msg(41));
  w.close();
  w.join();
  print("joined");
}
export async function finish(): Promise<void> {
  const m: Msg | null = await pending[0];
  print(m !== null ? `got ${m.value}` : "none");
}
export function main(): void {}
"#;

/// §349 rule 9: hot reload treats a receive that waits as it treats a host
/// operation. A frame that the new generation creates awaits the receive
/// of the old generation, and the await does not report it as stale.
// Cost: one reload session, one reload of the same source; one Worker thread.
#[test]
fn a_receive_that_waits_across_a_reload_is_not_stale() {
    let run = bounded("the reload session", || {
        let files = vec![SourceFile::entry("receive-reload.ts", RELOAD_PROGRAM)];
        let mut session = ReloadSession::new(&files).expect("reload session");
        session.call_export("start")?;
        session.reload(&files).expect("reload of the same source");
        session.call_export("finish")?;
        loop {
            while session.async_pending() != 0 {
                session.async_step()?;
            }
            if !session.async_wait() {
                break;
            }
        }
        Ok::<_, RunError>((session.take_output(), session.async_unfinished()))
    });
    assert_eq!(
        run.expect("no stale trap"),
        (b"joined\ngot 42\n".to_vec(), 0)
    );
}

/// §349.2 acceptance 1: the wait uses no CPU. Each program runs in a child
/// process, and the CPU time (user and system) of the child comes from
/// `wait4`. The dev JIT runs in a child of this test binary; the ship tier
/// runs the linked executable. `wait4` is a POSIX function, so this module
/// is for unix hosts only.
#[cfg(unix)]
mod cpu {
    use std::io::Read;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use subscript_codegen::{
        add_c11_optimized_flags, add_executable_output, emit_c, host_c_compiler,
        include_directory_arg, run_jit, runtime_staticlib_path, runtime_system_libraries,
        tool_output_report, unreferenced_code_removal_arguments, AOT_ENTRY_C,
    };
    use subscript_compiler::{check_program, SourceFile};

    use super::RUN_LIMIT;

    /// Names the program form that the child run of [`dev_cpu_child`] runs.
    const CHILD_ENV: &str = "SUBSCRIPT_WORKER_RECEIVE_CPU_CHILD";
    const CHILD_TEST: &str = "cpu::dev_cpu_child";
    /// The Worker computes for about 200 ms with these counts (release
    /// build, arm64 macOS).
    const SHIP_ITERATIONS: u32 = 200_000_000;
    const DEV_ITERATIONS: u32 = 60_000_000;
    const EXPECTED: &[u8] = b"ok\n";

    /// R awaits `receive()`; J uses `join()` and then `poll()`; L is the
    /// loop of `poll()` and `await Context.suspend()` (§349 problem 2).
    const FORMS: [&str; 3] = ["R", "J", "L"];

    fn program(form: &str, iterations: u32) -> String {
        let wait = match form {
            "R" => "const m: Msg | null = await w.receive();",
            "J" => "w.close(); w.join(); const m: Msg | null = w.poll();",
            "L" => {
                "let m: Msg | null = w.poll(); \
                 while (m === null) { await Context.suspend(); m = w.poll(); }"
            }
            other => panic!("unknown form {other}"),
        };
        format!(
            r#"class Msg {{ v: f64; constructor(v: f64) {{ this.v = v; }} }}
function work(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {{
  let acc: f64 = 0;
  for (let i: i32 = 0; i < {iterations}; i++) {{ acc = acc + (i as f64) * 0.5; }}
  outbox.post(new Msg(acc));
}}
export async function main(): Promise<void> {{
  const w: Worker<Msg, Msg> = Worker.spawn(work);
  {wait}
  print(m !== null && m.v > 0 ? "ok" : "none");
}}
"#
        )
    }

    fn files(form: &str, iterations: u32) -> Vec<SourceFile> {
        vec![SourceFile::entry("cpu.ts", program(form, iterations))]
    }

    /// The child side of the dev JIT measurement. With no [`CHILD_ENV`]
    /// it does nothing.
    // Cost: none in a run of the suite; one dev JIT run in a child.
    #[test]
    fn dev_cpu_child() {
        let Ok(form) = std::env::var(CHILD_ENV) else {
            return;
        };
        let stdout = run_jit(&files(&form, DEV_ITERATIONS)).expect("dev JIT run");
        assert_eq!(stdout, EXPECTED);
    }

    struct Measured {
        cpu: Duration,
        wall: Duration,
        stdout: Vec<u8>,
    }

    fn time_value(value: libc::timeval) -> Duration {
        Duration::from_secs(u64::try_from(value.tv_sec).expect("seconds"))
            + Duration::from_micros(u64::try_from(value.tv_usec).expect("microseconds"))
    }

    /// Runs `command` to its end and returns the CPU time of the process.
    /// Stops the process and fails the test after [`RUN_LIMIT`].
    // `wait4` reaps the child on the path that ends normally, because it
    // gives the CPU time that `Child::wait` does not. A second wait there
    // can wait for another child that got the same pid.
    #[allow(clippy::zombie_processes)]
    fn measure(what: &str, mut command: Command) -> Measured {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let started = Instant::now();
        let mut child = command.spawn().expect("start the child");
        let pid = libc::pid_t::try_from(child.id()).expect("a pid");
        let (status, usage) = loop {
            let mut status = 0;
            // SAFETY: `rusage` is a plain C struct; all zero bytes is a
            // valid value of it.
            let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
            // SAFETY: `pid` is a child of this process that no other code
            // waits for; both pointers are to live locals.
            let waited = unsafe { libc::wait4(pid, &mut status, libc::WNOHANG, &mut usage) };
            if waited == pid {
                break (status, usage);
            }
            assert!(
                waited == 0,
                "wait4 for {what}: {}",
                std::io::Error::last_os_error()
            );
            if started.elapsed() > RUN_LIMIT {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "{what} did not end in {RUN_LIMIT:?}: a wait of §349 rule 11 did not return"
                );
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        let wall = started.elapsed();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        if let Some(mut pipe) = child.stdout.take() {
            pipe.read_to_end(&mut stdout).expect("child stdout");
        }
        if let Some(mut pipe) = child.stderr.take() {
            pipe.read_to_end(&mut stderr).expect("child stderr");
        }
        assert!(
            libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
            "{what} ended with status {status}: {}{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        Measured {
            cpu: time_value(usage.ru_utime) + time_value(usage.ru_stime),
            wall,
            stdout,
        }
    }

    /// Links one ship-tier executable for `form`, as `run_c_aot` does.
    fn ship_build(directory: &Path, form: &str) -> PathBuf {
        let hir = check_program(&files(form, SHIP_ITERATIONS)).expect("checked");
        let program = emit_c(&hir).expect("generated C");
        let directory = directory.join(form);
        std::fs::create_dir_all(&directory).expect("build directory");
        std::fs::write(directory.join("program.c"), &program.source).expect("program.c");
        std::fs::write(directory.join("program.h"), &program.host_header).expect("program.h");
        std::fs::write(directory.join("entry.c"), AOT_ENTRY_C).expect("entry.c");
        let compiler = host_c_compiler().expect("host compiler");
        let executable = directory.join("program");
        let mut command = compiler.command();
        add_c11_optimized_flags(&mut command, compiler.style());
        command
            .arg(include_directory_arg(compiler.style(), &directory))
            .arg(directory.join("program.c"))
            .arg(directory.join("entry.c"))
            .arg(runtime_staticlib_path().expect("runtime archive"))
            .args(runtime_system_libraries(compiler.style()));
        add_executable_output(&mut command, &executable, compiler.style());
        command.args(unreferenced_code_removal_arguments(compiler.style()));
        let compiled = command.output().expect("C compiler");
        assert!(
            compiled.status.success(),
            "{}",
            tool_output_report(&compiled)
        );
        executable
    }

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Asserts acceptance 1 for one tier: `cpu(R) < 1.2 * cpu(J)`, and the
    /// firing control `cpu(L) > 1.5 * cpu(J)`.
    fn assert_tier(tier: &str, measured: &[Measured]) {
        for (form, run) in FORMS.iter().zip(measured) {
            eprintln!(
                "{tier} {form}: cpu {:.1} ms, wall {:.1} ms",
                run.cpu.as_secs_f64() * 1e3,
                run.wall.as_secs_f64() * 1e3
            );
        }
        let [receive, join, poll_loop] = measured else {
            panic!("three runs");
        };
        let join = join.cpu.as_secs_f64();
        assert!(
            receive.cpu.as_secs_f64() < 1.2 * join,
            "{tier}: the receive form used {:?} of CPU against {join} s with join()",
            receive.cpu
        );
        assert!(
            poll_loop.cpu.as_secs_f64() > 1.5 * join,
            "{tier}: the firing control did not fire: the loop used {:?} against {join} s",
            poll_loop.cpu
        );
    }

    // Cost: three ship-tier builds (3 C builds), three ship runs and three
    // dev JIT child runs of about 0.2 s of Worker time each.
    #[test]
    fn a_receive_that_waits_uses_no_cpu() {
        let scratch = Scratch(std::env::temp_dir().join(format!(
            "subscript-worker-receive-cpu-{}",
            std::process::id()
        )));
        std::fs::create_dir_all(&scratch.0).expect("scratch directory");
        let ship: Vec<Measured> = FORMS
            .iter()
            .map(|form| {
                let executable = ship_build(&scratch.0, form);
                let run = measure(
                    &format!("the ship-tier run {form}"),
                    Command::new(executable),
                );
                assert_eq!(run.stdout, EXPECTED, "ship-tier run {form}");
                run
            })
            .collect();
        let dev: Vec<Measured> = FORMS
            .iter()
            .map(|form| {
                let mut command = Command::new(std::env::current_exe().expect("the test binary"));
                command
                    .args([CHILD_TEST, "--exact", "--test-threads", "1"])
                    .env(CHILD_ENV, form);
                measure(&format!("the dev JIT run {form}"), command)
            })
            .collect();
        assert_tier("ship", &ship);
        assert_tier("dev", &dev);
    }
}
