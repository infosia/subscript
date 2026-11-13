use super::*;
use subscript_runtime::TrapKind;

fn src(text: &str) -> Vec<SourceFile> {
    vec![SourceFile::new("live.ts", text)]
}

fn hash_of(text: &str) -> DeclarationHash {
    let m = subscript_compiler::check_program(&src(text)).expect("checks");
    declaration_hash(&m)
}

#[test]
fn declaration_hash_ignores_bodies() {
    let a = hash_of("export function main(): void {\n  print(\"a\");\n}\n");
    let b = hash_of("export function main(): void {\n  print(\"b\");\n  print(\"c\");\n}\n");
    assert_eq!(a, b);
    assert_eq!(a.first_difference(&b), None);

    // Section 261: constructor and method bodies do not change layout inputs.
    let a = hash_of("@ValueType class V { x: i32; constructor() { this.x = 1; } get(): i32 { return this.x; } }");
    let b = hash_of("@ValueType class V { x: i32; constructor() { this.x = 2; } get(): i32 { return this.x + 1; } }");
    assert_eq!(a, b);
    assert_eq!(a.first_difference(&b), None);
}

#[test]
fn declaration_hash_is_stable_across_recompiles() {
    let text = "class C { x: i32; constructor() { this.x = 1; } }\nlet n: i32 = 0;\nexport function main(): void {\n  n += 1;\n}\n";
    assert_eq!(hash_of(text).value(), hash_of(text).value());
}

#[test]
fn declaration_hash_changes_for_a_new_field() {
    let a = hash_of(
        "class C { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {}\n",
    );
    let b = hash_of("class C { x: i32; y: i32; constructor() { this.x = 1; this.y = 2; } }\nexport function main(): void {}\n");
    assert_ne!(a, b);
    assert_eq!(a.first_difference(&b).as_deref(), Some("class C"));
}

// Section 261: an explicit alignment change must change the class hash.
#[test]
fn declaration_hash_changes_for_alignment() {
    let a = hash_of("@ValueType({ align: 4 })\nclass V { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {}\n");
    let b = hash_of("@ValueType({ align: 16 })\nclass V { x: i32; constructor() { this.x = 1; } }\nexport function main(): void {}\n");
    assert_ne!(a, b);
    assert_eq!(a.first_difference(&b).as_deref(), Some("class V"));
}

// Section 261: a mirror class comes from an ambient declaration.
// A source edit that changes the boundary kind also changes the declaration file.
#[test]
fn declaration_hash_changes_for_boundary_kind() {
    let a = subscript_compiler::check_program(&[SourceFile::new(
        "live.ts",
        "@ValueType class V { x: i32 = 0; }",
    )])
    .expect("value class checks");
    let b = subscript_compiler::check_program(&[SourceFile::ambient(
        "live.ts",
        "declare class V { x: i32; }",
    )])
    .expect("boundary class checks");
    let value = a.classes.iter().find(|c| c.name == "V").unwrap();
    let boundary = b.classes.iter().find(|c| c.name == "V").unwrap();
    assert!(value.is_value && boundary.is_value);
    assert!(!value.is_boundary && boundary.is_boundary);
    assert!(value.alignment_override.is_none() && boundary.alignment_override.is_none());
    assert_eq!(
        value.fields.iter().map(|f| &f.ty).collect::<Vec<_>>(),
        boundary.fields.iter().map(|f| &f.ty).collect::<Vec<_>>()
    );
    let a = declaration_hash(&a);
    let b = declaration_hash(&b);
    // Section 261: compare the class entry apart from the module run order.
    let class_entry = |hash: &DeclarationHash| {
        hash.entries
            .iter()
            .find(|(name, _)| name == "class V")
            .unwrap()
            .1
    };
    assert_ne!(class_entry(&a), class_entry(&b));
}

#[test]
fn declaration_hash_changes_for_field_reordering() {
    let a = hash_of("@ValueType\nclass V { x: i32; y: f32; constructor(x: i32, y: f32) { this.x = x; this.y = y; } }\nexport function main(): void {}\n");
    let b = hash_of("@ValueType\nclass V { y: f32; x: i32; constructor(x: i32, y: f32) { this.x = x; this.y = y; } }\nexport function main(): void {}\n");
    assert_ne!(a, b);
}

#[test]
fn declaration_hash_changes_for_enum_values_and_fixed_array_shapes() {
    let a = hash_of("enum E { A = 1, B = 2 }\nexport function main(): void {}\n");
    let b = hash_of("enum E { A = 1, B = 3 }\nexport function main(): void {}\n");
    assert_ne!(a, b);
    assert_eq!(a.first_difference(&b).as_deref(), Some("enum E"));

    let c = hash_of("let g: FixedArray<i32, 4> = [0, 0, 0, 0];\nexport function main(): void {}\n");
    let d =
        hash_of("let g: FixedArray<i32, 5> = [0, 0, 0, 0, 0];\nexport function main(): void {}\n");
    assert_ne!(c, d);
    assert_eq!(c.first_difference(&d).as_deref(), Some("variable g"));
}

#[test]
fn declaration_hash_changes_for_a_signature_edit() {
    let a = hash_of("function f(x: i32): i32 {\n  return x;\n}\nexport function main(): void {}\n");
    let b = hash_of(
        "function f(x: i64): i32 {\n  return x as i32;\n}\nexport function main(): void {}\n",
    );
    assert_ne!(a, b);
    assert_eq!(a.first_difference(&b).as_deref(), Some("function f"));
}

#[test]
fn declaration_hash_includes_map_and_set_type_arguments() {
    let map_i32 = hash_of(
        "let values: Map<i32, i32> = new Map<i32, i32>();\n\
             export function main(): void { print(`${values.size}`); }\n",
    );
    let map_i64 = hash_of(
        "let values: Map<i64, i32> = new Map<i64, i32>();\n\
             export function main(): void { print(`${values.size}`); }\n",
    );
    assert_ne!(map_i32, map_i64);
    assert_eq!(
        map_i32.first_difference(&map_i64).as_deref(),
        Some("variable values")
    );

    let set_i32 = hash_of(
        "let values: Set<i32> = new Set<i32>();\n\
             export function main(): void { print(`${values.size}`); }\n",
    );
    let set_i64 = hash_of(
        "let values: Set<i64> = new Set<i64>();\n\
             export function main(): void { print(`${values.size}`); }\n",
    );
    assert_ne!(set_i32, set_i64);
}

#[test]
fn declarations_are_listed_in_declaration_order() {
    let h = hash_of("class C { x: i32; constructor() { this.x = 0; } }\nlet g: i32 = 1;\nexport function main(): void {}\n");
    assert_eq!(
        h.declarations(),
        vec![
            r#"module run order/set ["live.ts"]"#,
            "host entry main",
            "class Error",
            "constructor Error",
            "class C",
            "constructor C",
            "variable g",
            "function main"
        ]
    );
}

#[test]
fn declarations_include_carrier_lambda_parameter_escapes() {
    let h = hash_of(
            "export function main(): void { let run: (cb: () => i32) => void = (cb: () => i32): void => { cb(); }; }",
        );
    assert_eq!(
        h.declarations(),
        vec![
            r#"module run order/set ["live.ts"]"#,
            "host entry main",
            "class Error",
            "constructor Error",
            "function main",
            "lambda parameter escapes"
        ]
    );
}

#[test]
fn non_carrier_lambda_parameters_preserve_the_declaration_hash() {
    let before = hash_of("export function main(): void {}");
    let after =
        hash_of("export function main(): void { let run: (n: i32) => i32 = (n: i32): i32 => n; }");
    assert_eq!(
        after.declarations(),
        vec![
            r#"module run order/set ["live.ts"]"#,
            "host entry main",
            "class Error",
            "constructor Error",
            "function main"
        ]
    );
    assert_eq!(before, after);
}

#[test]
fn session_runs_and_reports_output() {
    let mut s = ReloadSession::new(&src(
        "export function main(): void {\n  print(\"one\");\n}\n",
    ))
    .expect("session");
    s.call_main().expect("call");
    assert_eq!(s.take_output(), b"one\n");
    assert_eq!(s.take_output(), b"");
}

#[test]
fn live_allocations_reads_the_runtime_count() {
    let mut session = ReloadSession::new(&src(
        "export function main(): void { const values: i32[] = [1]; }",
    ))
    .expect("session");
    let before = session.live_allocations();
    session.call_main().expect("main");
    assert!(session.live_allocations() > before);
}

#[test]
fn live_boundary_scratch_allocations_reads_the_context_count() {
    let mut s = ReloadSession::new(&src("export function main(): void {}")).expect("s");
    assert_eq!(s.live_boundary_scratch_allocations(), 0);
    assert!(!s.ctx.boundary_scratch_alloc(16, 0).is_null());
    assert_eq!(s.live_boundary_scratch_allocations(), 1);
    s.ctx.boundary_scratch_release(0);
    assert_eq!(s.live_boundary_scratch_allocations(), 0);
}

#[test]
fn session_reports_a_trap_without_killing_the_host() {
    let mut s = ReloadSession::new(&src(
        "export function main(): void {\n  const xs: i32[] = [];\n  xs.pop();\n}\n",
    ))
    .expect("session");
    match s.call_main() {
        Err(RunError::Trap(t)) => assert_eq!(t.rule, TrapKind::EmptyPop),
        other => panic!("expected a trap, got {other:?}"),
    }
}

#[test]
fn suspended_async_frame_is_stale_after_reload() {
    let before = "export async function main(): Promise<void> {\n\
                      \x20 print(\"before\");\n\
                      \x20 await Context.suspend();\n\
                      \x20 print(\"old continuation\");\n\
                      }\n";
    let after = "export async function main(): Promise<void> {\n\
                     \x20 print(\"before\");\n\
                     \x20 await Context.suspend();\n\
                     \x20 print(\"new continuation\");\n\
                     }\n";
    let mut session = ReloadSession::new(&src(before)).expect("session");
    session.call_main().expect("async kick");
    assert_eq!(session.take_output(), b"before\n");
    assert_eq!(session.ctx.async_pending(), 1);
    session.reload(&src(after)).expect("body-only reload");

    // SAFETY: the session retains every JIT generation, including the
    // callback queued by the pre-reload async wrapper.
    assert_eq!(unsafe { session.ctx.async_step() }, 1);
    match session.check_trap() {
        Err(RunError::Trap(trap)) => assert_eq!(trap.rule, TrapKind::StaleCoroutine),
        other => panic!("expected stale async-frame trap, got {other:?}"),
    }
}

// ----- §94 async reload: one test per scheduler state -----

/// The 1-based line and column of `needle` in `source`, so a position
/// assertion is derived from the program rather than from a repeat of
/// the report it checks (CLAUDE.md core principle 9).
fn source_position(source: &str, needle: &str) -> (u32, u32) {
    let offset = source.find(needle).expect("the program contains the site");
    let line = source[..offset].matches('\n').count() + 1;
    let column = offset - source[..offset].rfind('\n').map_or(0, |index| index + 1) + 1;
    (line as u32, column as u32)
}

/// The host trap-clear mechanism, exactly as an embedding host calls it.
fn host_clear_trap(session: &mut ReloadSession) -> i32 {
    let ctx: *mut Context = &mut *session.ctx;
    // SAFETY: the session owns this Context and no script frame is live.
    unsafe { subscript_runtime::ffi::subscript_rt_ctx_clear_trap(ctx) }
}

fn expect_stale(result: Result<usize, RunError>, what: &str) -> TrapReport {
    match result {
        Err(RunError::Trap(trap)) => {
            assert_eq!(trap.rule, TrapKind::StaleCoroutine, "{what}: trap kind");
            assert_eq!(
                trap.message, "stale coroutine after reload",
                "{what}: trap message"
            );
            trap
        }
        other => panic!("{what}: expected a stale-coroutine trap, got {other:?}"),
    }
}

const READY_ROOT: &str = "async function settled(): Promise<i32> {\n\
                              \x20 return 7;\n\
                              }\n\
                              export async function main(): Promise<void> {\n\
                              \x20 print(\"before\");\n\
                              \x20 const h: Promise<i32> = settled();\n\
                              \x20 const v: i32 = await h;\n\
                              \x20 print(`old ${v}`);\n\
                              }\n";

const BLOCKED_PARENT: &str = "async function child(): Promise<i32> {\n\
                                  \x20 print(\"child:before\");\n\
                                  \x20 await Context.suspend();\n\
                                  \x20 print(\"child:old\");\n\
                                  \x20 return 5;\n\
                                  }\n\
                                  export async function main(): Promise<void> {\n\
                                  \x20 print(\"main:before\");\n\
                                  \x20 const h: Promise<i32> = child();\n\
                                  \x20 const v: i32 = await h;\n\
                                  \x20 print(`main:old ${v}`);\n\
                                  }\n";

/// A ready job carries a reload's staleness: the checkpoint resumes it,
/// the epoch check fires before any body effect, and clearing the trap
/// does not clear the staleness (`compiler.md` §94.2).
#[test]
fn a_ready_async_job_is_stale_after_reload_and_stays_stale() {
    let after = READY_ROOT.replace("old ", "new ");
    let mut session = ReloadSession::new(&src(READY_ROOT)).expect("session");
    session.call_main().expect("async kick");
    assert_eq!(session.take_output(), b"before\n");
    // An await of an already-completed handle leaves a ready job, so the
    // reload finds work in the ready queue rather than the parked list.
    assert_eq!(session.async_pending(), 1);
    assert_eq!(session.async_unfinished(), 1);

    session.reload(&src(&after)).expect("body-only reload");

    // The position is the suspension's, derived from the program.
    let (line, column) = source_position(READY_ROOT, "await h");
    let trap = expect_stale(session.async_step(), "first step after the reload");
    assert_eq!((trap.pos.line, trap.pos.col), (line, column));
    assert_eq!(trap.pos.file, "live.ts");
    assert_eq!(session.take_output(), b"", "no post-await body ran");
    assert_eq!(session.async_pending(), 1, "the trapping job is preserved");

    // Repeated steps while the trap is set are no-ops.
    let again = expect_stale(session.async_step(), "second step while trapped");
    assert_eq!(again.pos, trap.pos);
    assert_eq!(session.take_output(), b"");
    assert_eq!(session.async_pending(), 1);

    // Clearing the trap does not clear frame staleness.
    assert_eq!(
        host_clear_trap(&mut session),
        1,
        "the host cleared the trap"
    );
    let third = expect_stale(session.async_step(), "step after the trap clear");
    assert_eq!(third.pos, trap.pos);
    assert_eq!(session.take_output(), b"");
    assert_eq!(session.async_pending(), 1);
}

/// A parked frame carries the same staleness, at its own suspension.
#[test]
fn a_parked_async_frame_reports_its_own_suspension_position() {
    let before = "export async function main(): Promise<void> {\n\
                      \x20 print(\"before\");\n\
                      \x20 await Context.suspend();\n\
                      \x20 print(\"old continuation\");\n\
                      }\n";
    let after = before.replace("old continuation", "new continuation");
    let mut session = ReloadSession::new(&src(before)).expect("session");
    session.call_main().expect("async kick");
    assert_eq!(session.take_output(), b"before\n");
    assert_eq!(session.async_pending(), 1);
    session.reload(&src(&after)).expect("body-only reload");
    let (line, column) = source_position(before, "await Context.suspend()");
    let trap = expect_stale(session.async_step(), "step after the reload");
    assert_eq!((trap.pos.line, trap.pos.col), (line, column));
    assert_eq!(session.take_output(), b"");
    assert_eq!(session.async_pending(), 1);
}

/// A blocked parent stays blocked when its parked child traps first. The
/// checkpoint stops at the child, so the parent's own stale check does
/// not run, and its registration is preserved.
#[test]
fn a_blocked_parent_keeps_waiting_when_its_parked_child_is_stale() {
    let after = BLOCKED_PARENT
        .replace("child:old", "child:new")
        .replace("main:old", "main:new");
    let mut session = ReloadSession::new(&src(BLOCKED_PARENT)).expect("session");
    session.call_main().expect("async kick");
    assert_eq!(session.take_output(), b"main:before\nchild:before\n");
    // The child parked; the parent is a continuation registered on it,
    // so it is unfinished but not pending work of its own.
    assert_eq!(session.async_pending(), 1);
    assert_eq!(session.async_unfinished(), 2);

    session.reload(&src(&after)).expect("body-only reload");

    // The child is the frame the checkpoint reaches, so the report names
    // the child's suspension, not the parent's await.
    let (line, column) = source_position(BLOCKED_PARENT, "await Context.suspend()");
    let trap = expect_stale(session.async_step(), "first step after the reload");
    assert_eq!((trap.pos.line, trap.pos.col), (line, column));
    assert_eq!(session.take_output(), b"", "no post-await body ran");
    assert_eq!(session.async_pending(), 1);
    assert_eq!(session.async_unfinished(), 2, "the parent stays blocked");

    let again = expect_stale(session.async_step(), "second step while trapped");
    assert_eq!(again.pos, trap.pos);
    assert_eq!(session.take_output(), b"");
    assert_eq!(session.async_unfinished(), 2);

    assert_eq!(
        host_clear_trap(&mut session),
        1,
        "the host cleared the trap"
    );
    let third = expect_stale(session.async_step(), "step after the trap clear");
    assert_eq!(third.pos, trap.pos);
    assert_eq!(session.take_output(), b"");
    assert_eq!(session.async_unfinished(), 2);
}

#[test]
fn reload_is_refused_while_the_context_has_a_live_worker() {
    let source = "class Message { value: i32 = 0; }\n\
                      function blocked(inbox: Inbox<Message>, outbox: Outbox<Message>): void {\n\
                      \x20 const message: Message | null = inbox.wait();\n\
                      }\n\
                      export function main(): void {\n\
                      \x20 const worker: Worker<Message, Message> = Worker.spawn(blocked);\n\
                      }\n";
    let mut session = ReloadSession::new(&src(source)).expect("worker reload session");
    session.call_main().expect("spawn live worker");
    assert!(matches!(
        session.reload(&src(source)),
        Err(ReloadError::LiveWorkers)
    ));
}

#[test]
fn reload_mode_worker_echo_round_trip_completes_without_a_trap() {
    let source = "class Message {\n\
                      \x20 value: i32;\n\
                      \x20 constructor(value: i32) { this.value = value; }\n\
                      }\n\
                      function echo(inbox: Inbox<Message>, outbox: Outbox<Message>): void {\n\
                      \x20 const message: Message | null = inbox.wait();\n\
                      \x20 if (message !== null) { outbox.post(message); }\n\
                      }\n\
                      export function main(): void {\n\
                      \x20 const worker: Worker<Message, Message> = Worker.spawn(echo);\n\
                      \x20 worker.post(new Message(37));\n\
                      \x20 worker.close();\n\
                      \x20 worker.join();\n\
                      \x20 const reply: Message | null = worker.poll();\n\
                      \x20 if (reply !== null) { print(`echo=${reply.value}`); }\n\
                      }\n";
    let mut session = ReloadSession::new(&src(source)).expect("worker reload session");
    session.call_main().expect("reload-mode worker round trip");
    assert_eq!(session.take_output(), b"echo=37\n");
    assert!(session.ctx.trap_record().is_none());
    assert!(!session.ctx.has_live_workers());
}

#[test]
fn reload_session_declares_context_before_jit_modules() {
    let source = include_str!("../reload.rs");
    let fields = source
        .split_once("pub struct ReloadSession {")
        .expect("ReloadSession declaration")
        .1
        .split_once('}')
        .expect("ReloadSession field list")
        .0;
    let context = fields.find("ctx: Box<Context>").expect("Context field");
    let modules = fields
        .find("modules: Vec<JITModule>")
        .expect("JIT modules field");
    assert!(
        context < modules,
        "ReloadSession must drop its Context, which joins workers, before JIT modules"
    );
}

#[test]
fn initializer_trap_can_be_captured_without_dropping_the_session() {
    let (mut s, trap) =
        ReloadSession::new_capturing_initializer_trap(&src("let xs: i32[] = [];\n\
             let value: i32 = xs.pop();\n\
             export function main(): void {\n\
             \x20 print(\"still live\");\n\
             }\n"))
        .expect("session");
    assert_eq!(trap.map(|report| report.rule), Some(TrapKind::EmptyPop));
    assert!(s.take_output().is_empty());

    assert!(matches!(s.call_main(), Err(RunError::Trap(_))));
    assert!(s.take_output().is_empty());
}

#[test]
fn unknown_entry_is_an_internal_error() {
    let mut s = ReloadSession::new(&src("export function main(): void {}\n")).expect("session");
    assert!(matches!(s.call_export("nope"), Err(RunError::Internal(_))));
}

#[test]
fn reload_of_unchecked_sources_is_rejected() {
    let mut s = ReloadSession::new(&src("export function main(): void {}\n")).expect("session");
    assert!(matches!(
        s.reload(&src("const x: number = 1;\n")),
        Err(ReloadError::Rejected(_))
    ));
}

#[test]
fn errors_render_their_cause() {
    let e = ReloadError::DeclarationChanged {
        declaration: "class C".to_string(),
    };
    assert!(e.to_string().contains("class C"));
    assert!(ReloadError::ScriptOnStack.to_string().contains("stack"));
    assert!(ReloadError::LiveWorkers
        .to_string()
        .contains("live workers"));
}

#[test]
fn debug_shows_generations_and_slots() {
    let s = ReloadSession::new(&src("export function main(): void {}\n")).expect("session");
    let text = format!("{s:?}");
    assert!(text.contains("generations"), "got {text}");
}
// compiler.md §272 rule 2: a failed initializer blocks every entry.
#[test]
fn failed_initializer_blocks_entries() {
    let (mut session, trap) = ReloadSession::new_capturing_initializer_trap(&src(
            "print('init'); throw new Error('init failed'); const g: i32 = 7; export function main(): void { print(`${g}`); }",
        )).expect("session");
    assert!(trap.is_some());
    let _ = session.take_output();
    let result = session.call_export("main");
    let output = session.take_output();
    assert!(
        result.is_err(),
        "entry succeeded; output: {}",
        String::from_utf8_lossy(&output)
    );
    assert!(output.is_empty());
}

// compiler.md §272 rules 2 and 3: each entry refuses, and reload retries initialization.
#[test]
fn failed_initializer_reload_starts_again() {
    let broken = "const values: i32[] = [1, 2, 3]; print('init'); throw new Error('init failed'); const g: i32 = 7; export function main(): void { print(`${g}`); }";
    let fixed = broken.replace("throw new Error('init failed');", "");
    let (mut session, _) = ReloadSession::new_capturing_initializer_trap(&src(broken)).unwrap();
    assert_eq!(session.take_output(), b"init\n");
    for result in [
        session.call_main(),
        session.run_main(),
        session.call_export("main"),
        session.call_export_with("main", &[]),
        session.async_step().map(|_| ()),
        session.async_step_budget(1).map(|_| ()),
    ] {
        let Err(RunError::Trap(trap)) = result else {
            panic!("expected initializer trap")
        };
        assert!(trap.message.contains("initializer trap:") && trap.message.contains("init failed"));
        assert!(trap.stdout.is_empty());
    }
    assert!(session.take_output().is_empty());
    assert!(session.reload(&src("broken source")).is_err());
    session.reload(&src(broken)).unwrap();
    assert_eq!(session.take_output(), b"init\n");
    assert!(matches!(session.call_main(), Err(RunError::Trap(_))));
    session.reload(&src(&fixed)).unwrap();
    let fresh = ReloadSession::new(&src(&fixed)).unwrap();
    assert_eq!(session.live_allocations(), fresh.live_allocations());
    session.call_main().unwrap();
    assert_eq!(session.take_output(), b"init\n7\n");
}

// compiler.md §272 rule 4: completed sessions retain globals across a swap.
#[test]
fn completed_initializer_reload_preserves_globals() {
    let source =
        "print('init'); let g: i32 = 7; export function main(): void { g += 1; print(`${g}`); }";
    let mut session = ReloadSession::new(&src(source)).unwrap();
    session.call_main().unwrap();
    assert_eq!(session.take_output(), b"init\n8\n");
    session
        .reload(&src(&source.replace("= 7", "= 100")))
        .unwrap();
    session.call_main().unwrap();
    assert_eq!(session.take_output(), b"9\n");
}

// compiler.md §272 rule 3: unread output survives each initializer retry.
#[test]
fn failed_initializer_reload_retains_unread_output() {
    let broken = "print('old'); throw new Error('failed'); export function main(): void {}";
    let (mut session, _) = ReloadSession::new_capturing_initializer_trap(&src(broken)).unwrap();
    assert_eq!(session.take_output(), b"old\n");
    session.reload(&src(broken)).unwrap();
    session.reload(&src(broken)).unwrap();
    session
        .reload(&src("print('new'); export function main(): void {};"))
        .unwrap();
    assert_eq!(session.take_output(), b"old\nold\nnew\n");
    assert!(session.take_output().is_empty());
}

// compiler.md §272 rule 3: Context drop closes workers from a failed initializer.
#[test]
fn failed_initializer_with_worker_reload_succeeds() {
    let broken = "class Message { value: i32 = 0; } function work(input: Inbox<Message>, output: Outbox<Message>): void { input.wait(); } function start(): i32 { const worker: Worker<Message, Message> = Worker.spawn(work); throw new Error('failed'); } const started: i32 = start(); export function main(): void { print('fixed'); }";
    let (mut session, trap) = ReloadSession::new_capturing_initializer_trap(&src(broken)).unwrap();
    assert!(trap.is_some());
    assert!(session.ctx.has_live_workers());
    session
        .reload(&src("export function main(): void { print('fixed'); }"))
        .unwrap();
    session.call_main().unwrap();
    assert_eq!(session.take_output(), b"fixed\n");
}
