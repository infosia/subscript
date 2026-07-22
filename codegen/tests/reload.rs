//! Hot-reload demo (`specs/blocks/compiler.md` §8.2): the three cases
//! the contract requires — an accepted body edit whose new behaviour
//! is observed in output, a rejected layout edit, and a stale-coroutine
//! trap — plus the standing check that reload-mode lowering reproduces
//! the committed goldens.
//!
//! The programs are written inline: they are reload *demos*, not corpus
//! entries, and each one exists as two source revisions.

mod corpus;

use subscript_codegen::{ReloadError, ReloadSession, RunError};
use subscript_compiler::SourceFile;
use subscript_runtime::TrapKind;

fn files(text: &str) -> Vec<SourceFile> {
    vec![SourceFile::new("live.ts", text)]
}

fn output(session: &mut ReloadSession) -> String {
    String::from_utf8(session.take_output()).expect("utf-8 output")
}

// ----- (a) accepted body edit -----

const COUNTER_V1: &str = "\
let counter: i32 = 0;
function step(): i32 {
  counter += 1;
  return counter;
}
export function main(): void {
  print(`${step()}`);
}
";

const COUNTER_V2: &str = "\
let counter: i32 = 0;
function step(): i32 {
  counter += 10;
  return counter;
}
export function main(): void {
  print(`step=${step()}`);
}
";

#[test]
fn accepted_body_edit_changes_behaviour_and_keeps_context_state() {
    let mut s = ReloadSession::new(&files(COUNTER_V1)).expect("session");
    s.call_main().expect("first call");
    s.call_main().expect("second call");
    assert_eq!(output(&mut s), "1\n2\n");

    s.reload(&files(COUNTER_V2)).expect("body edit is accepted");

    // The new bodies run, and `counter` kept the value the pre-swap
    // bodies left in it: 2 + 10 = 12, not 10.
    s.call_main().expect("call after reload");
    assert_eq!(output(&mut s), "step=12\n");
    s.call_main().expect("call after reload");
    assert_eq!(output(&mut s), "step=22\n");
}

#[test]
fn accepted_body_edit_keeps_live_allocations() {
    const V1: &str = "\
class Box {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}
let held: Box | null = null;
export function main(): void {
  if (held === null) {
    held = new Box(41);
  }
  if (held !== null) {
    print(`${held.value}`);
  }
}
";
    const V2: &str = "\
class Box {
  value: i32;
  constructor(value: i32) {
    this.value = value;
  }
}
let held: Box | null = null;
export function main(): void {
  if (held === null) {
    held = new Box(41);
  }
  if (held !== null) {
    held.value += 1;
    print(`bumped ${held.value}`);
  }
}
";
    let mut s = ReloadSession::new(&files(V1)).expect("session");
    s.call_main().expect("first call");
    assert_eq!(output(&mut s), "41\n");
    s.reload(&files(V2)).expect("body edit is accepted");
    // The same allocation is still reachable through the global and
    // still live: the post-swap body mutates it in place.
    s.call_main().expect("call after reload");
    assert_eq!(output(&mut s), "bumped 42\n");
}

// ----- (b) rejected layout edit -----

const SHAPE_V1: &str = "\
@value
class Point {
  x: i32;
  y: i32;
  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
  }
}
let origin: Point = new Point(3, 4);
export function main(): void {
  print(`${origin.x},${origin.y}`);
}
";

/// Same program with one field added to `Point` — a layout change, so
/// the swap must be refused.
const SHAPE_V2: &str = "\
@value
class Point {
  x: i32;
  y: i32;
  z: i32;
  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
    this.z = 0;
  }
}
let origin: Point = new Point(3, 4);
export function main(): void {
  print(`${origin.x},${origin.y},${origin.z}`);
}
";

#[test]
fn rejected_layout_edit_names_the_declaration_and_leaves_the_program_running() {
    let mut s = ReloadSession::new(&files(SHAPE_V1)).expect("session");
    s.call_main().expect("first call");
    assert_eq!(output(&mut s), "3,4\n");

    match s.reload(&files(SHAPE_V2)) {
        Err(ReloadError::DeclarationChanged { declaration }) => {
            assert_eq!(declaration, "class Point");
        }
        other => panic!("expected a refused swap, got {other:?}"),
    }

    // The refused reload changed nothing: same bodies, same Context.
    s.call_main().expect("call after the refused reload");
    assert_eq!(output(&mut s), "3,4\n");
    // And a well-formed body edit is still accepted afterwards.
    let edited = SHAPE_V1.replace("${origin.x},${origin.y}", "(${origin.x};${origin.y})");
    s.reload(&files(&edited)).expect("body edit is accepted");
    s.call_main().expect("call after the accepted reload");
    assert_eq!(output(&mut s), "(3;4)\n");
}

#[test]
fn rejected_signature_edit_names_the_function() {
    const V1: &str = "\
function scale(v: i32): i32 {
  return v * 2;
}
export function main(): void {
  print(`${scale(4)}`);
}
";
    const V2: &str = "\
function scale(v: i64): i64 {
  return v * 2;
}
export function main(): void {
  print(`${scale(4)}`);
}
";
    let mut s = ReloadSession::new(&files(V1)).expect("session");
    s.call_main().expect("first call");
    assert_eq!(output(&mut s), "8\n");
    match s.reload(&files(V2)) {
        Err(ReloadError::DeclarationChanged { declaration }) => {
            assert_eq!(declaration, "function scale");
        }
        other => panic!("expected a refused swap, got {other:?}"),
    }
    s.call_main().expect("call after the refused reload");
    assert_eq!(output(&mut s), "8\n");
}

// ----- (c) stale coroutine -----

const COROUTINE_V1: &str = "\
function* counting() {
  let i: i32 = 0;
  while (i < 100) {
    yield i;
    i += 1;
  }
}
let live: Generator<i32> = counting();
export function main(): void {
  const s = live.next();
  print(`${s.value}`);
}
";

const COROUTINE_V2: &str = "\
function* counting() {
  let i: i32 = 0;
  while (i < 100) {
    yield i * 2;
    i += 1;
  }
}
let live: Generator<i32> = counting();
export function main(): void {
  const s = live.next();
  print(`v2 ${s.value}`);
}
";

#[test]
fn coroutine_suspended_across_a_swap_traps_on_resume() {
    let mut s = ReloadSession::new(&files(COROUTINE_V1)).expect("session");
    s.call_main().expect("first resume");
    s.call_main().expect("second resume");
    assert_eq!(output(&mut s), "0\n1\n");

    s.reload(&files(COROUTINE_V2)).expect("body edit is accepted");

    // `live` was created before the swap and is suspended inside a
    // body that no longer exists; resuming it traps at the `.next()`
    // position (line 10 of the post-swap source).
    match s.call_main() {
        Err(RunError::Trap(t)) => {
            assert_eq!(t.rule, TrapKind::StaleCoroutine);
            assert_eq!(t.message, "stale coroutine after reload");
            assert_eq!(t.pos.file, "live.ts");
            assert_eq!(t.pos.line, 10);
        }
        other => panic!("expected a stale-coroutine trap, got {other:?}"),
    }
    // Nothing was produced by the trapped call.
    assert_eq!(output(&mut s), "");
}

#[test]
fn a_coroutine_created_after_the_swap_runs_the_new_body() {
    // Same program, but the generator is created by the entry rather
    // than at module scope, so each call gets a fresh frame.
    const V1: &str = "\
function* counting() {
  let i: i32 = 0;
  while (i < 10) {
    yield i;
    i += 1;
  }
}
export function main(): void {
  const g = counting();
  const s = g.next();
  print(`${s.value}`);
}
";
    let v2 = V1.replace("yield i;", "yield i + 100;");
    let mut s = ReloadSession::new(&files(V1)).expect("session");
    s.call_main().expect("first call");
    assert_eq!(output(&mut s), "0\n");
    s.reload(&files(&v2)).expect("body edit is accepted");
    s.call_main().expect("call after reload");
    assert_eq!(output(&mut s), "100\n");
}

// ----- a trap ends the call, not the session (§8.2) -----

/// Two independent exports over one Context: `resume` drives a
/// module-level coroutine, `report` touches only a counter. The
/// coroutine goes stale on the first swap; `report` must keep working.
const SESSION_V1: &str = "\
function* counting() {
  let i: i32 = 0;
  while (i < 100) {
    yield i;
    i += 1;
  }
}
let live: Generator<i32> = counting();
let ticks: i32 = 0;
export function resume(): void {
  const s = live.next();
  print(`tick ${s.value}`);
}
export function report(): void {
  ticks += 1;
  print(`ticks=${ticks}`);
}
export function main(): void {
  report();
}
";

#[test]
fn an_unrelated_export_still_runs_after_a_stale_coroutine_trap() {
    let mut s = ReloadSession::new(&files(SESSION_V1)).expect("session");
    s.call_export("resume").expect("first resume");
    assert_eq!(output(&mut s), "tick 0\n");

    let v2 = SESSION_V1.replace("yield i;", "yield i + 1000;");
    s.reload(&files(&v2)).expect("body edit is accepted");
    assert!(matches!(s.call_export("resume"), Err(RunError::Trap(_))));

    // The trap ended that call, not the session: an unrelated export
    // runs normally and sees the Context state the trap left alone.
    s.call_export("report").expect("report after the trap");
    assert_eq!(output(&mut s), "ticks=1\n");
    s.call_export("report").expect("report again");
    assert_eq!(output(&mut s), "ticks=2\n");
}

#[test]
fn an_accepted_reload_after_a_trap_takes_effect_on_the_next_call() {
    let mut s = ReloadSession::new(&files(SESSION_V1)).expect("session");
    s.call_export("resume").expect("first resume");
    assert_eq!(output(&mut s), "tick 0\n");

    let v2 = SESSION_V1.replace("yield i;", "yield i + 1000;");
    s.reload(&files(&v2)).expect("first swap is accepted");
    assert!(matches!(s.call_export("resume"), Err(RunError::Trap(_))));

    // A swap after a trap is applied normally, and its bodies run.
    let v3 = v2.replace("ticks=${ticks}", "count=${ticks}");
    s.reload(&files(&v3)).expect("swap after a trap is accepted");
    s.call_export("report").expect("call after the second swap");
    assert_eq!(output(&mut s), "count=1\n");
}

#[test]
fn a_stale_coroutine_stays_stale_after_the_trap_is_cleared() {
    let mut s = ReloadSession::new(&files(SESSION_V1)).expect("session");
    s.call_export("resume").expect("first resume");
    assert_eq!(output(&mut s), "tick 0\n");

    let v2 = SESSION_V1.replace("yield i;", "yield i + 1000;");
    s.reload(&files(&v2)).expect("body edit is accepted");

    // Clearing is reporting-only: staleness is carried by the frame's
    // epoch, so every later resume traps again, with the same report.
    for attempt in 0..3 {
        match s.call_export("resume") {
            Err(RunError::Trap(t)) => {
                assert_eq!(t.rule, TrapKind::StaleCoroutine, "attempt {attempt}");
                assert_eq!(t.message, "stale coroutine after reload");
            }
            other => panic!("attempt {attempt}: expected a trap, got {other:?}"),
        }
        assert_eq!(output(&mut s), "", "attempt {attempt}: produced output");
        // An interleaved good call must not clear the staleness either.
        s.call_export("report").expect("report between attempts");
        let _ = s.take_output();
    }
}

#[test]
fn an_ordinary_trap_does_not_brick_the_session() {
    const V1: &str = "\
let hits: i32 = 0;
export function fault(): void {
  const xs: i32[] = [1, 2];
  print(`${xs[7]}`);
}
export function ok(): void {
  hits += 1;
  print(`hits=${hits}`);
}
export function main(): void {
  ok();
}
";
    let mut s = ReloadSession::new(&files(V1)).expect("session");
    s.call_export("ok").expect("first good call");
    assert_eq!(output(&mut s), "hits=1\n");

    match s.call_export("fault") {
        Err(RunError::Trap(t)) => assert_eq!(t.rule, TrapKind::IndexOutOfBounds),
        other => panic!("expected an out-of-bounds trap, got {other:?}"),
    }
    assert_eq!(output(&mut s), "");

    // Same Context, same globals: the counter kept its value.
    s.call_export("ok").expect("good call after the trap");
    assert_eq!(output(&mut s), "hits=2\n");
    // And the faulting entry still faults, deterministically.
    assert!(matches!(s.call_export("fault"), Err(RunError::Trap(_))));
    s.call_export("ok").expect("good call after the second trap");
    assert_eq!(output(&mut s), "hits=3\n");
}

// ----- reload-mode lowering is the same language -----

#[test]
fn reload_mode_reproduces_every_committed_golden() {
    let accept = corpus::corpus_accept();
    let mut failures = Vec::new();
    let ids = corpus::golden_ids(&accept);
    assert!(
        ids.len() >= 24,
        "expected at least the 24 committed goldens, found {}",
        ids.len()
    );
    for id in &ids {
        let golden = corpus::golden_bytes(&accept, id);
        let sources = corpus::entry_sources(&accept, id);
        let mut session = match ReloadSession::new(&sources) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{id}: session failed: {e}"));
                continue;
            }
        };
        match session.call_main() {
            Ok(()) => {
                let bytes = session.take_output();
                if bytes != golden {
                    failures.push(format!(
                        "{id}: reload-mode output {:?} != golden {:?}",
                        String::from_utf8_lossy(&bytes),
                        String::from_utf8_lossy(&golden)
                    ));
                }
            }
            Err(e) => failures.push(format!("{id}: run failed: {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} reload-mode failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
