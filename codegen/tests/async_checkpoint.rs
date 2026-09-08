//! Host-visible facts of the §94 continuation queue: checkpoint boundaries,
//! quiescence counts, the unfinished observer, ownership, and teardown.

use subscript_codegen::{run_jit, run_jit_with_memory_accounting, ReloadSession, RunError};
use subscript_compiler::SourceFile;

fn files(source: &str) -> Vec<SourceFile> {
    vec![SourceFile::new("checkpoint.ts", source)]
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8 output")
}

/// Kicks `main`, then every other exported async function in declaration
/// order, as the standard runner does (§26.3).
fn kick(session: &mut ReloadSession, exports: &[&str]) {
    session.call_main().expect("kick main");
    for export in exports {
        session.call_export(export).expect("kick export");
    }
}

#[test]
fn a_finite_settled_chain_finishes_in_one_checkpoint() {
    let source = "async function settled(value: i32): Promise<i32> {\n\
                  \x20 return value + 1;\n\
                  }\n\
                  export async function main(): Promise<void> {\n\
                  \x20 let total: i32 = 0;\n\
                  \x20 for (let i: i32 = 0; i < 2000; i += 1) {\n\
                  \x20   total = await settled(total);\n\
                  \x20 }\n\
                  \x20 print(`total=${total}`);\n\
                  }\n";
    let mut session = ReloadSession::new(&files(source)).expect("session");
    kick(&mut session, &[]);
    // Every await suspends, so the kick ends with one ready job and no
    // output (§94.1 rules 2 and 7).
    assert_eq!(session.async_pending(), 1);
    assert_eq!(text(&session.take_output()), "");

    // §94.1 rule 8: jobs added during the drain join the same checkpoint,
    // so the whole finite chain finishes in one step.
    assert_eq!(session.async_step().expect("one checkpoint"), 0);
    assert_eq!(text(&session.take_output()), "total=2000\n");
    assert_eq!(session.async_pending(), 0);
    assert_eq!(session.async_unfinished(), 0);
}

#[test]
fn a_checkpoint_promotes_parked_frames_after_the_jobs_already_ready() {
    let source = "async function settled(): Promise<void> {\n\
                  }\n\
                  async function readyWork(): Promise<i32> {\n\
                  \x20 await settled();\n\
                  \x20 print(\"ready\");\n\
                  \x20 return 1;\n\
                  }\n\
                  async function parkedWork(): Promise<i32> {\n\
                  \x20 await Context.suspend();\n\
                  \x20 print(\"parked\");\n\
                  \x20 return 2;\n\
                  }\n\
                  export async function main(): Promise<void> {\n\
                  \x20 const parked: Promise<i32> = parkedWork();\n\
                  \x20 const ready: Promise<i32> = readyWork();\n\
                  \x20 const p: i32 = await parked;\n\
                  \x20 const r: i32 = await ready;\n\
                  \x20 print(`sum=${p + r}`);\n\
                  }\n";
    let mut session = ReloadSession::new(&files(source)).expect("session");
    kick(&mut session, &[]);
    // One ready job and one parked frame, with the holder blocked on the
    // parked one. Pending counts the two the checkpoint can advance.
    assert_eq!(session.async_pending(), 2);
    assert_eq!(session.async_unfinished(), 3);
    assert_eq!(text(&session.take_output()), "");

    assert_eq!(session.async_step().expect("one checkpoint"), 0);
    // The already-ready job runs before the promoted parked frame, even
    // though the parked frame was created first (§94.1 rule 8).
    assert_eq!(text(&session.take_output()), "ready\nparked\nsum=3\n");
    assert_eq!(session.async_unfinished(), 0);
}

#[test]
fn a_frame_parked_during_a_drain_waits_for_the_next_checkpoint() {
    let source = "async function twice(): Promise<i32> {\n\
                  \x20 print(\"a\");\n\
                  \x20 await Context.suspend();\n\
                  \x20 print(\"b\");\n\
                  \x20 await Context.suspend();\n\
                  \x20 print(\"c\");\n\
                  \x20 return 1;\n\
                  }\n\
                  export async function main(): Promise<void> {\n\
                  \x20 print(`v=${await twice()}`);\n\
                  }\n";
    let mut session = ReloadSession::new(&files(source)).expect("session");
    kick(&mut session, &[]);
    assert_eq!(text(&session.take_output()), "a\n");
    assert_eq!(session.async_pending(), 1);

    // §94.1 rule 9: the frame parks again inside the drain, so its next
    // resume belongs to the next checkpoint rather than this one.
    assert_eq!(session.async_step().expect("first checkpoint"), 1);
    assert_eq!(text(&session.take_output()), "b\n");
    assert_eq!(session.async_step().expect("second checkpoint"), 0);
    assert_eq!(text(&session.take_output()), "c\nv=1\n");
    assert_eq!(session.async_unfinished(), 0);
}

#[test]
fn quiescence_can_leave_blocked_work_that_the_unfinished_observer_reports() {
    // Two frames wait on each other through an array of handles. §94.2:
    // pending reaches zero, and `async_unfinished` is what exposes the
    // blocked wait. No deadlock trap and no cancellation API is added.
    let source = "async function waiter(id: i32, others: Promise<i32>[]): Promise<i32> {\n\
                  \x20 await Context.suspend();\n\
                  \x20 const other: Promise<i32> = others[1 - id];\n\
                  \x20 return await other;\n\
                  }\n\
                  export async function main(): Promise<void> {\n\
                  \x20 const handles: Promise<i32>[] = [];\n\
                  \x20 const a: Promise<i32> = waiter(0, handles);\n\
                  \x20 const b: Promise<i32> = waiter(1, handles);\n\
                  \x20 handles.push(a);\n\
                  \x20 handles.push(b);\n\
                  \x20 print(\"held\");\n\
                  \x20 print(`v=${await a}`);\n\
                  \x20 print(`w=${await b}`);\n\
                  }\n";
    let mut session = ReloadSession::new(&files(source)).expect("session");
    kick(&mut session, &[]);
    assert_eq!(text(&session.take_output()), "held\n");
    assert_eq!(session.async_pending(), 2);
    assert_eq!(session.async_unfinished(), 3);

    // The checkpoint registers each frame on the other and returns.
    assert_eq!(session.async_step().expect("checkpoint"), 0);
    assert_eq!(text(&session.take_output()), "");
    assert_eq!(session.async_pending(), 0, "no work can advance");
    assert_eq!(
        session.async_unfinished(),
        3,
        "three invocations never finish"
    );

    // A further step changes nothing, and the Context releases safely.
    assert_eq!(session.async_step().expect("idle checkpoint"), 0);
    assert_eq!(session.async_unfinished(), 3);
    drop(session);
}

#[test]
fn a_dropped_context_with_pending_work_runs_no_continuation() {
    let source = "async function work(): Promise<i32> {\n\
                  \x20 print(\"start\");\n\
                  \x20 await Context.suspend();\n\
                  \x20 print(\"never\");\n\
                  \x20 return 1;\n\
                  }\n\
                  export async function main(): Promise<void> {\n\
                  \x20 print(`v=${await work()}`);\n\
                  }\n";
    let mut session = ReloadSession::new(&files(source)).expect("session");
    kick(&mut session, &[]);
    assert_eq!(text(&session.take_output()), "start\n");
    assert_eq!(session.async_pending(), 1);
    assert_eq!(session.async_unfinished(), 2);
    // §94.2: release discards the work without resumption. The `never` line
    // is the observable proof that no continuation ran at teardown.
    drop(session);
}

#[test]
fn a_no_print_ownership_loop_ends_with_no_live_context_payload() {
    // Repeated held handles with two awaits each, and no print allocation,
    // so `live_bytes` measures the scheduler's own ownership.
    let source = "async function work(): Promise<i32> {\n\
                  \x20 await Context.suspend();\n\
                  \x20 await Context.suspend();\n\
                  \x20 return 42;\n\
                  }\n\
                  export async function main(): Promise<void> {\n\
                  \x20 for (let i: i32 = 0; i < 200; i += 1) {\n\
                  \x20   const h: Promise<i32> = work();\n\
                  \x20   await Context.suspend();\n\
                  \x20   await Context.suspend();\n\
                  \x20   await Context.suspend();\n\
                  \x20   const a: i32 = await h;\n\
                  \x20   const b: i32 = await h;\n\
                  \x20   if (a !== 42 || b !== 42) {\n\
                  \x20     unreachable();\n\
                  \x20   }\n\
                  \x20 }\n\
                  }\n";
    let (output, memory) =
        run_jit_with_memory_accounting(&files(source), false).expect("ownership loop runs");
    assert_eq!(output, b"");
    assert_eq!(
        memory.live_bytes, 0,
        "every frame and completion is released at quiescence"
    );
}

#[test]
fn a_completed_owned_result_survives_a_collection_inside_a_drain() {
    let source = "class Box {\n\
                  \x20 value: i32;\n\
                  \x20 constructor(value: i32) {\n\
                  \x20   this.value = value;\n\
                  \x20 }\n\
                  }\n\
                  async function boxed(): Promise<Box> {\n\
                  \x20 await Context.suspend();\n\
                  \x20 return new Box(7);\n\
                  }\n\
                  async function collector(): Promise<i32> {\n\
                  \x20 await Context.suspend();\n\
                  \x20 Context.collect();\n\
                  \x20 print(\"collected\");\n\
                  \x20 return 1;\n\
                  }\n\
                  export async function main(): Promise<void> {\n\
                  \x20 const box: Promise<Box> = boxed();\n\
                  \x20 const junk: Promise<i32> = collector();\n\
                  \x20 await Context.suspend();\n\
                  \x20 await Context.suspend();\n\
                  \x20 const first: Box = await box;\n\
                  \x20 const second: Box = await box;\n\
                  \x20 print(`first=${first.value} second=${second.value}`);\n\
                  \x20 print(`junk=${await junk}`);\n\
                  }\n";
    // The collection runs while the completed handle's result is reachable
    // only from the completion cache (§94.2), and both awaits read it.
    let output = run_jit(&files(source)).expect("collection inside a drain runs");
    assert_eq!(text(&output), "collected\nfirst=7 second=7\njunk=1\n");
}

#[test]
fn a_trapped_checkpoint_preserves_its_work_and_repeats_as_a_no_op() {
    let source = "async function fail(): Promise<i32> {\n\
                  \x20 await Context.suspend();\n\
                  \x20 print(\"before-trap\");\n\
                  \x20 unreachable();\n\
                  \x20 return 1;\n\
                  }\n\
                  export async function main(): Promise<void> {\n\
                  \x20 print(`v=${await fail()}`);\n\
                  }\n";
    let mut session = ReloadSession::new(&files(source)).expect("session");
    kick(&mut session, &[]);
    assert_eq!(session.async_pending(), 1);

    match session.async_step() {
        Err(RunError::Trap(trap)) => {
            assert_eq!(trap.rule, subscript_runtime::TrapKind::UnreachableReached);
        }
        other => panic!("expected the callee's trap, got {other:?}"),
    }
    assert_eq!(text(&session.take_output()), "before-trap\n");
    let pending = session.async_pending();
    assert_eq!(pending, 1, "the trapping registration is preserved");

    // §94.2: repeated steps are no-ops until the host clears the trap.
    for _ in 0..2 {
        assert!(matches!(session.async_step(), Err(RunError::Trap(_))));
        assert_eq!(text(&session.take_output()), "");
        assert_eq!(session.async_pending(), pending);
    }
}
