//! Async callable producers, ownership, collection, and reload (§167).
use subscript_codegen::{
    interpreter::interpret, lir::lower_module, run_c_aot, run_jit, ReloadSession, RunError,
};
use subscript_compiler::{check_program, SourceFile};
use subscript_runtime::TrapKind;

fn files(source: &str) -> [SourceFile; 1] {
    [SourceFile::new("async-values.ts", source)]
}
fn compare(source: &str, expected: &[u8]) {
    let files = files(source);
    let hir = check_program(&files).expect("checks");
    assert_eq!(
        interpret(&lower_module(&hir).expect("lowers")).expect("interpreter"),
        expected
    );
    assert_eq!(run_jit(&files).expect("JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("C AOT"), expected);
}

#[test]
fn corpus_matches_three_tiers_and_node_golden() {
    compare(
        include_str!("../../corpus/accept/a336-async-function-values.ts"),
        include_bytes!("../../corpus/accept/a336-async-function-values.expected"),
    );
}

#[test]
fn synchronous_handle_arrow_and_async_body_keep_distinct_frames() {
    for asynchronous in [false, true] {
        let flag = if asynchronous { "async " } else { "" };
        let body = if asynchronous {
            "await value(7)"
        } else {
            "value(7)"
        };
        compare(
            &format!(
                "async function value(n: i32): Promise<i32> {{ return n; }}
            export async function main(): Promise<void> {{
                const job = {flag}(): Promise<i32> => {body};
                const handle: Promise<i32> = job();
                print(`${{await handle}}`);
            }}"
            ),
            b"7\n",
        );
    }
}

#[test]
fn indirect_handles_release_after_success_and_failure_with_direct_controls() {
    for indirect in [false, true] {
        for failure in [false, true] {
            let body = if failure {
                "throw failure;"
            } else {
                "return 7;"
            };
            let call = if indirect { "job()" } else { "child()" };
            let source = format!(
                "const failure: Error = new Error(\"failure\");
                async function child(): Promise<i32> {{ {body} }}
                async function exercise(): Promise<void> {{
                    const job: () => Promise<i32> = child;
                    for (let i: i32 = 0; i < 20; i++) {{ try {{ await {call}; }} catch (e) {{ }} }}
                }}
                export async function main(): Promise<void> {{ await exercise(); }}"
            );
            let mut session = ReloadSession::new(&files(&source)).expect("session");
            let baseline = session.live_allocations();
            session.call_main().expect("kick");
            while session.async_pending() != 0 {
                session.async_step().expect("step");
            }
            assert_eq!(
                session.live_allocations(),
                baseline,
                "all frame counts reach zero: indirect={indirect} failure={failure}"
            );
        }
    }
}

#[test]
fn async_arrow_field_survives_collection_with_no_collection_control() {
    for collect in [false, true] {
        let collection = if collect { "Context.collect();" } else { "" };
        compare(&format!("class Jobs {{ job: () => Promise<i32> = async (): Promise<i32> => {{ await Context.suspend(); return 7; }}; }}
            export async function main(): Promise<void> {{ const owner = new Jobs(); {collection} print(`${{await owner.job()}}`); }}"), b"7\n");
    }
}

#[test]
fn arrow_suspended_across_reload_traps_with_no_reload_control() {
    let source = "export async function main(): Promise<void> { const job = async (): Promise<i32> => { await Context.suspend(); return 7; }; print(`${await job()}`); }";
    for reload in [false, true] {
        let mut session = ReloadSession::new(&files(source)).expect("session");
        session.call_main().expect("kick");
        if reload {
            session
                .reload(&files(&source.replace("return 7", "return 9")))
                .expect("reload");
        }
        if reload {
            match session.async_step() {
                Err(RunError::Trap(trap)) => assert_eq!(trap.rule, TrapKind::StaleCoroutine),
                other => panic!("expected stale frame: {other:?}"),
            }
        } else {
            while session.async_pending() != 0 {
                session.async_step().expect("step");
            }
            assert_eq!(session.take_output(), b"7\n");
        }
    }
}

#[test]
fn named_async_value_reaches_new_body_with_no_reload_control() {
    let source = "async function child(): Promise<i32> { return 7; }
        const job: () => Promise<i32> = child;
        export async function main(): Promise<void> { print(`${await job()}`); }";
    for reload in [false, true] {
        let mut session = ReloadSession::new(&files(source)).expect("session");
        if reload {
            session
                .reload(&files(&source.replace("return 7", "return 9")))
                .expect("reload");
        }
        session.call_main().expect("kick");
        while session.async_pending() != 0 {
            session.async_step().expect("step");
        }
        assert_eq!(session.take_output(), if reload { b"9\n" } else { b"7\n" });
    }
}

#[test]
fn try_in_async_arrow_crosses_await_with_named_control() {
    for arrow in [false, true] {
        let function =
            "(): Promise<i32> => { try { await child(); } catch (e) { return 7; } return 0; }";
        let declaration = if arrow {
            format!("const job = async {function};")
        } else {
            "async function job(): Promise<i32> { try { await child(); } catch (e) { return 7; } return 0; }".into()
        };
        compare(&format!("async function child(): Promise<i32> {{ await Context.suspend(); throw new Error(\"failure\"); }}
            {declaration}
            export async function main(): Promise<void> {{ print(`${{await job()}}`); }}"), b"7\n");
    }
}

#[test]
fn static_async_value_field_matches_instance_field_control() {
    for static_field in [false, true] {
        let modifier = if static_field { "static " } else { "" };
        let receiver = if static_field { "Jobs" } else { "new Jobs()" };
        compare(&format!("async function value(): Promise<i32> {{ return 7; }}
            class Jobs {{ {modifier}job: () => Promise<i32> = value; }}
            export async function main(): Promise<void> {{ print(`${{await {receiver}.job()}}`); }}"), b"7\n");
    }
}

#[test]
fn handle_expression_body_rejects_with_explicit_await_order_control() {
    for explicit_await in [true, false] {
        let body = if explicit_await { "await h" } else { "h" };
        let source = format!(
            "async function value(): Promise<i32> {{ return 7; }}
            const job = async (h: Promise<i32>): Promise<i32> => {body};
            async function clock(): Promise<void> {{
                for (let i: i32 = 0; i < 5; i++) {{ print(`turn ${{i}}`); await value(); }}
            }}
            async function work(): Promise<void> {{
                const h: Promise<i32> = value(); print(`result ${{await job(h)}}`); await h;
            }}
            export async function main(): Promise<void> {{
                const task: Promise<void> = work(); const ticker: Promise<void> = clock();
                await task; await ticker;
            }}"
        );
        if explicit_await {
            // TypeScript 5.9.2 accepts this source. Node supplies the output order.
            compare(
                &source,
                b"turn 0\nturn 1\nresult 7\nturn 2\nturn 3\nturn 4\n",
            );
        } else {
            let diagnostics = check_program(&files(&source)).expect_err("handle body rejects");
            assert!(diagnostics.iter().any(|diagnostic| {
                diagnostic.code == subscript_compiler::RuleCode::S100
                    && diagnostic.message
                        == "type mismatch: the return value expects `i32`, got `Promise<i32>`"
            }));
        }
    }
}

// Cost: one interpreter execution, one JIT session, and one C build.
#[test]
fn owned_capture_corpus_matches_three_tiers() {
    compare(
        include_str!("../../corpus/accept/a353-async-arrow-captures.ts"),
        include_bytes!("../../corpus/accept/a353-async-arrow-captures.expected"),
    );
}

// Cost: two JIT sessions for each reload policy and its control.
#[test]
fn owned_arrow_suspended_across_reload_and_old_value_keep_module_identity() {
    let source = "class Box { job: () => Promise<i32> = async (): Promise<i32> => 0; }
        const box = new Box();
        export function install(): void { const n: i32 = 7; box.job = async (): Promise<i32> => { await Context.suspend(); return n + 1; }; }
        export async function main(): Promise<void> { Context.collect(); print(`${await box.job()}`); }";
    for suspended in [false, true] {
        for reload in [false, true] {
            let mut session = ReloadSession::new(&files(source)).expect("session");
            session.call_export("install").expect("install old value");
            if suspended {
                session.call_main().expect("suspend");
            }
            if reload {
                session
                    .reload(&files(
                        &source
                            .replace(
                                "const n: i32 = 7; box.job",
                                "const n: i32 = 7; const extra: string = \"layout\"; box.job",
                            )
                            .replace("n + 1", "n + 9 + extra.length"),
                    ))
                    .expect("reload");
            }
            if !suspended {
                session.call_main().expect("call old value");
            }
            if suspended && reload {
                match session.async_step() {
                    Err(RunError::Trap(trap)) => assert_eq!(trap.rule, TrapKind::StaleCoroutine),
                    other => panic!("expected stale frame: {other:?}"),
                }
            } else {
                while session.async_pending() != 0 {
                    session.async_step().expect("checkpoint");
                }
                assert_eq!(session.take_output(), b"8\n");
            }
        }
    }
}

// Cost: one interpreter run, one JIT session, and one C build per holder.
fn collect_held_arrow(holder: &str) {
    let prefix = "class J { v: i32 = 0; }
        class K { v: i64 = 0; }
        class Jobs { f: () => Promise<i32> = async (): Promise<i32> => 0; }
        function scalar(): () => Promise<i32> { const n: i32 = 41; return async (): Promise<i32> => n + 1; }
        function reference(): () => Promise<i32> { const r = new J(); r.v = 41; return async (): Promise<i32> => r.v + 1; }
        function churn(): void { Context.collect(); const junk: J[] = []; const wide: K[] = [];
            for (let i: i32 = 0; i < 100; i++) { const j = new J(); j.v = 1000 + i; junk.push(j);
                const k = new K(); k.v = 2000; wide.push(k); } }";
    let body = match holder {
        "sync_local" => "function hold(f: () => Promise<i32>): Promise<i32> { const local = f; churn(); return local(); }
            export async function main(): Promise<void> { const h = hold(scalar()); print(`${await h}`); const k = hold(reference()); print(`${await k}`); }",
        "async_local" => "export async function main(): Promise<void> { const f = scalar(); await Context.suspend(); churn(); print(`${await f()}`);
            const g = reference(); await Context.suspend(); churn(); print(`${await g()}`); }",
        "sync_parameter" => "function hold(f: () => Promise<i32>): Promise<i32> { churn(); return f(); }
            export async function main(): Promise<void> { const h = hold(scalar()); print(`${await h}`); const k = hold(reference()); print(`${await k}`); }",
        "global" => "let job: () => Promise<i32> = async (): Promise<i32> => 0;
            function install(ref: boolean): void { job = ref ? reference() : scalar(); }
            export async function main(): Promise<void> { install(false); churn(); print(`${await job()}`); install(true); churn(); print(`${await job()}`); }",
        "field" => "function hold(ref: boolean): Jobs { const box = new Jobs(); box.f = ref ? reference() : scalar(); return box; }
            export async function main(): Promise<void> { const a = hold(false); churn(); print(`${await a.f()}`); const b = hold(true); churn(); print(`${await b.f()}`); }",
        "array" => "function hold(ref: boolean): (() => Promise<i32>)[] { return [ref ? reference() : scalar()]; }
            export async function main(): Promise<void> { const a = hold(false); churn(); print(`${await a[0]()}`); const b = hold(true); churn(); print(`${await b[0]()}`); }",
        _ => panic!("unknown holder"),
    };
    compare(&format!("{prefix}\n{body}"), b"42\n42\n");
}

#[test]
fn function_environment_survives_sync_local_collection() {
    collect_held_arrow("sync_local");
}
#[test]
fn function_environment_survives_async_local_collection() {
    collect_held_arrow("async_local");
}
#[test]
fn function_environment_survives_sync_parameter_collection() {
    collect_held_arrow("sync_parameter");
}
#[test]
fn function_environment_survives_global_collection() {
    collect_held_arrow("global");
}
#[test]
fn function_environment_survives_field_collection() {
    collect_held_arrow("field");
}
#[test]
fn function_environment_survives_array_collection() {
    collect_held_arrow("array");
}

// Cost: one interpreter run, one JIT session, and one C build.
#[test]
fn borrowed_and_owned_environments_keep_distinct_storage() {
    compare(
        "async function value(n: i32): Promise<i32> { return n; }
        export async function main(): Promise<void> {
            const n: i32 = 42;
            const borrowed = (): Promise<i32> => value(n);
            const owned = async (): Promise<i32> => n;
            const a = borrowed; const b = owned;
            await Context.suspend(); Context.collect();
            print(`${await a()}`); print(`${await b()}`);
        }",
        b"42\n42\n",
    );
}

// Cost: four JIT sessions for the reload cases and their controls.
#[test]
fn old_owned_arrow_survives_reload_without_current_owned_environment() {
    let source = "class Cell { value: i32 = 42; }
        class Jobs { job: () => Promise<i32> = fallback; }
        const jobs = new Jobs();
        async function fallback(): Promise<i32> { return 0; }
        export function install(): void {
            const cell = new Cell();
            jobs.job = async (): Promise<i32> => { await Context.suspend(); return cell.value; };
        }
        function churn(): void {
            for (let i: i32 = 0; i < 100; i++) { const cell = new Cell(); cell.value = i; }
        }
        async function invoke(job: () => Promise<i32>): Promise<i32> { return await job(); }
        export async function main(): Promise<void> {
            const delta: i32 = 0;
            const sync = (): i32 => delta;
            const job = sync() === 0 ? jobs.job : fallback;
            const handle = invoke(job);
            churn();
            print(`${await handle}`);
        }";
    let old_install = "const cell = new Cell();\n            jobs.job = async (): Promise<i32> => { await Context.suspend(); return cell.value; };";
    for owned_in_new in [true, false] {
        for reload in [false, true] {
            let mut session = ReloadSession::new(&files(source)).expect("session");
            session
                .call_export("install")
                .expect("install old owned value");
            if reload {
                let replacement = if owned_in_new {
                    source.replace("return cell.value;", "return cell.value + 1;")
                } else {
                    source.replace(old_install, "jobs.job = fallback;")
                };
                session.reload(&files(&replacement)).expect("body reload");
            }
            session.call_main().expect("call old owned value");
            while session.async_pending() != 0 {
                session.async_step().expect("resume old owned value");
            }
            assert_eq!(
                session.take_output(),
                b"42\n",
                "reload={reload}, owned_in_new={owned_in_new}"
            );
        }
    }
}
