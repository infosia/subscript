//! Runtime capture and exception behavior for compiler.md §291.
//! Measured debug test time: 0.94 s for 12 tests, excluding the build.
//! The collection test compiles each loop count once in the JIT.
use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

fn compare(source: &str, expected: &[u8]) {
    let files = [SourceFile::new("defaults.ts", source)];
    let hir = check_program(&files).expect("checks");
    assert_eq!(
        interpret(&lower_module(&hir).expect("lowers")).expect("interpreter"),
        expected
    );
    assert_eq!(run_jit(&files).expect("JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("C AOT"), expected);
}

#[test]
fn synchronous_default_throw_leaves_the_call() {
    compare(
        r#"
        function fail(): i32 { print("default"); throw new Error("failed"); }
        function f(x: i32 = fail()): void { print("body"); }
        export function main(): void {
            try { f(); print("after"); } catch (e) { print("caught function"); }
            const lambda = (x: i32 = fail()): void => { print("body"); };
            try { lambda(); print("after"); } catch (e) { print("caught lambda"); }
        }
    "#,
        b"default\ncaught function\ndefault\ncaught lambda\n",
    );
}

#[test]
fn a_self_default_reads_the_environment_before_a_body_shadow() {
    compare(
        r#"
        export function main(): void {
            const k: i32 = 5;
            const rec = (n: i32, x: i32 = k): void => {
                print(`${n}:${x}`);
                { const k: i32 = 100; if (n > 0) rec(n - 1); }
            };
            rec(1);
        }
    "#,
        b"1:5\n0:5\n",
    );
}

#[test]
fn async_defaults_preserve_explicit_argument_throws_and_capture_owners() {
    compare(
        r#"
        function fail(): i32 { throw new Error("failed"); }
        class Box { value: i32 = 9; }
        export async function main(): Promise<void> {
            const text: string = "owned";
            const box = new Box();
            const f = async (n: i32, s: string = text, b: Box = box, x: i32 = fail()): Promise<void> => {
                print("body");
            };
            try { const p = f(fail()); print("after argument"); await p; }
            catch (e) { print("argument caught"); }
            try { const p = f(1); print("after default"); await p; }
            catch (e) { print("default caught"); }
            print(text);
            print(`${box.value}`);
        }
    "#,
        b"argument caught\nafter default\ndefault caught\nowned\n9\n",
    );
}

#[test]
fn a_synchronous_promise_producer_still_throws_at_the_call() {
    compare(
        r#"
        function fail(): i32 { throw new Error("failed"); }
        async function job(): Promise<void> {}
        export async function main(): Promise<void> {
            const sync = (x: i32 = fail()): Promise<void> => job();
            try { const p = sync(); print("after"); await p; }
            catch (e) { print("caught"); }
        }
    "#,
        b"caught\n",
    );
}

#[test]
fn global_async_lambda_and_async_self_defaults_fail_handles() {
    compare(
        r#"
        function value(n: i32): i32 { if (n == 0) throw new Error("failed"); return n; }
        const global = async (x: i32 = value(0)): Promise<void> => { print("bad global body"); };
        export async function main(): Promise<void> {
            try { const p = global(); print("after global"); await p; }
            catch (e) { print("global caught"); }
            const text: string = "owned";
            const rec = async (n: i32, s: string = text, x: i32 = value(n)): Promise<void> => {
                print(s);
                if (n > 0) { const p = rec(n - 1); print("after self"); await p; }
            };
            try { await rec(1); } catch (e) { print("self caught"); }
        }
    "#,
        b"after global\nglobal caught\nowned\nafter self\nself caught\n",
    );
}

#[test]
fn captured_string_and_reference_defaults_survive_collection_without_growth() {
    use subscript_codegen::run_jit_with_memory_accounting;
    let exercise = r#"
        class Box { value: i32 = 9; }
        function exercise(): void {
            const text: string = `owned ${5}`;
            const box = new Box();
            const str = (s: string = text): string => s;
            const ref = (b: Box = box): Box => b;
            const nested = (): void => {
                Context.collect();
                print(str());
                const copy = ref();
                Context.collect();
                print(`${copy.value}`);
            };
            nested();
        }
    "#;
    let mut live = Vec::new();
    for count in [1, 20] {
        let source = format!("{exercise} export function main(): void {{ for (let i: i32 = 0; i < {count}; i++) {{ exercise(); Context.collect(); }} }}");
        let expected = "owned 5\n9\n".repeat(count);
        let (output, memory) =
            run_jit_with_memory_accounting(&[SourceFile::new("memory.ts", source)], true)
                .expect("memory accounting");
        assert_eq!(output, expected.as_bytes());
        live.push(memory.live_bytes);
    }
    assert_eq!(
        live[0], live[1],
        "captured defaults retain no unreachable values: {live:?}"
    );
}

#[test]
fn successful_async_defaults_still_run_eagerly_and_return_values() {
    compare(
        r#"
        function value(n: i32): i32 { print("default"); if (n < 0) throw new Error("failed"); return n; }
        async function af(n: i32 = value(7)): Promise<string> { print("function body"); return `${n}`; }
        export async function main(): Promise<void> {
            const p = af(); print("after function"); print(await p);
            const text: string = "owned";
            const al = async (n: i32 = value(8), s: string = text): Promise<string> => { print("lambda body"); return `${s}${n}`; };
            const q = al(); print("after lambda"); print(await q);
        }
    "#,
        b"default\nfunction body\nafter function\n7\ndefault\nlambda body\nafter lambda\nowned8\n",
    );
}

#[test]
fn async_default_failures_release_earlier_handle_arguments() {
    use subscript_codegen::ReloadSession;
    let source = r#"
        const error: Error = new Error("failed");
        async function job(): Promise<i32> { return 7; }
        function fail(): i32 { throw error; }
        async function af(p: Promise<i32> = job(), n: i32 = fail()): Promise<void> { print(`${await p}`); }
        export async function main(): Promise<void> {
            const al = async (p: Promise<i32> = job(), n: i32 = fail()): Promise<void> => { print(`${await p}`); };
            for (let i: i32 = 0; i < 20; i++) {
                try { await af(); } catch (e) {}
                try { await al(); } catch (e) {}
            }
        }
    "#;
    compare(source, b"");
    let mut session = ReloadSession::new(&[SourceFile::new("counts.ts", source)]).expect("session");
    let baseline = session.live_allocations();
    session.call_main().expect("kick");
    while session.async_pending() != 0 {
        session.async_step().expect("step");
    }
    assert_eq!(
        session.live_allocations(),
        baseline,
        "failed defaults release all frames"
    );
}

#[test]
fn reference_arguments_and_local_receivers_keep_their_lifetime_sites() {
    // Node 24 prints "caught" for each program. Each program lowers once.
    let programs = [
        r#"function fail(tag: string): i32 { throw new Error(tag); }
        class Box { v: i32 = 3; }
        async function af(b: Box, x: i32 = fail("b")): Promise<void> { print("bad"); }
        export async function main(): Promise<void> {
          const b = new Box(); const p = af(b);
          try { await p; } catch (e) { print("caught"); }
        }"#,
        r#"function fail(tag: string): i32 { throw new Error(tag); }
        async function af(b: i32[], x: i32 = fail("b")): Promise<void> { print("bad"); }
        export async function main(): Promise<void> {
          const b: i32[] = [3]; const p = af(b);
          try { await p; } catch (e) { print("caught"); }
        }"#,
        r#"function fail(tag: string): i32 { throw new Error(tag); }
        class Box { v: i32 = 3; }
        export async function main(): Promise<void> {
          const af = async (b: Box, x: i32 = fail("b")): Promise<void> => { print("bad"); };
          const b = new Box(); const p = af(b);
          try { await p; } catch (e) { print("caught"); }
        }"#,
        r#"function fail(tag: string): i32 { throw new Error(tag); }
        class C { async m(x: i32 = fail("m")): Promise<void> { print("bad"); } }
        export async function main(): Promise<void> {
          const c = new C(); const p = c.m();
          try { await p; } catch (e) { print("caught"); }
        }"#,
        r#"function fail(tag: string): i32 { throw new Error(tag); }
        class Box { v: i32 = 3; }
        async function af(b: Box, x: i32 = fail("b")): Promise<void> { print("bad"); }
        export async function main(): Promise<void> {
          const b = new Box(); try { await af(b); } catch (e) { print("caught"); }
        }"#,
    ];
    for source in programs {
        let hir = check_program(&[SourceFile::new("lifetime.ts", source)]).expect("checks");
        let module = lower_module(&hir).expect("lowers with lifetime sites");
        assert_eq!(interpret(&module).expect("interpreter"), b"caught\n");
    }
}

#[test]
fn successful_async_defaults_keep_callback_order() {
    compare(
        r#"
        function maybe(tag: string, n: i32): i32 { print(`maybe ${tag}`); if (n < 0) throw new Error(tag); return n; }
        async function nop(): Promise<void> {}
        async function ok(a: i32 = maybe("ok", 1)): Promise<i32> { print("ok body"); return a; }
        async function ticker(tag: string): Promise<void> { print(`${tag} 0`); await nop(); print(`${tag} 1`); await nop(); print(`${tag} 2`); await nop(); print(`${tag} 3`); }
        async function plain(): Promise<i32> { print("plain body"); return 5; }
        function fail(tag: string): i32 { throw new Error(tag); }
        async function h(a: i32 = fail("h")): Promise<i32> { return 1; }
        class C { async m(a: i32 = fail("m")): Promise<i32> { return 1; } }
        async function directTicker(tag: string): Promise<void> { print(`${tag} 0`); await nop(); print(`${tag} 1`); await nop(); print(`${tag} 2`); await nop(); print(`${tag} 3`); await nop(); print(`${tag} 4`); }
        export async function main(): Promise<void> {
          const okl = async (a: i32 = maybe("okl", 2)): Promise<i32> => { print("okl body"); return a; };
          const t2 = ticker("U"); const q3 = ok(); const q5 = plain(); const q4 = okl();
          const r3 = q3.then((v: i32): void => { print(`then ok ${v}`); });
          const r5 = q5.then((v: i32): void => { print(`then plain ${v}`); });
          const r6 = q4.then((v: i32): void => { print(`then okl ${v}`); });
          await t2; await r3; await r5; await r6; print("end");
          const success = directTicker("S");
          print(`${await ok()}`);
          await success;
          const t = directTicker("T");
          const al = async (a: i32 = fail("l")): Promise<i32> => 1;
          const c = new C();
          try { await h(); } catch (e) { print("caught h"); }
          try { await al(); } catch (e) { print("caught l"); }
          try { await c.m(); } catch (e) { print("caught m"); }
          await t;

        }
        "#,
        b"U 0\nmaybe ok\nok body\nplain body\nmaybe okl\nokl body\nU 1\nthen ok 1\nthen plain 5\nthen okl 2\nU 2\nU 3\nend\nS 0\nmaybe ok\nok body\nS 1\n1\nS 2\nS 3\nS 4\nT 0\nT 1\ncaught h\nT 2\ncaught l\nT 3\ncaught m\nT 4\n",
    );
}

#[test]
fn a_closure_default_target_must_exist() {
    use subscript_compiler::lir::{CallTargetKind, FunctionId, InstructionKind};
    let hir = check_program(&[SourceFile::new("target.ts",
        "export function main(): void { const k: i32 = 1; const f = (x: i32 = k): void => {}; f(); }"
    )]).expect("checks");
    let mut module = lower_module(&hir).expect("lowers");
    let target = module
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find_map(|i| match &mut i.kind {
            InstructionKind::Call(target)
                if matches!(target.kind, CallTargetKind::ClosureDefault { .. }) =>
            {
                Some(target)
            }
            _ => None,
        })
        .expect("closure default call");
    if let CallTargetKind::ClosureDefault { helper, .. } = &mut target.kind {
        *helper = FunctionId(u32::MAX);
    }
    let errors = subscript_codegen::lir::verify_module(&module).expect_err("missing target");
    assert!(format!("{errors:?}").contains("call target identity/signature is invalid"));
}

#[test]
fn a_closure_default_operand_must_match_its_lambda() {
    use subscript_compiler::lir::{CallTargetKind, InstructionKind, Operand};
    let hir = check_program(&[SourceFile::new("target.ts",
        "export function main(): void { const k: i32 = 1; const f = (x: i32 = k): void => {}; const g = (x: i32 = k): void => {}; f(); g(); }"
    )]).expect("checks");
    let mut module = lower_module(&hir).expect("lowers");
    let function = module
        .functions
        .iter_mut()
        .find(|f| {
            f.blocks.iter().any(|b| {
                b.instructions.iter().any(|i| {
                    matches!(&i.kind, InstructionKind::Call(t)
            if matches!(t.kind, CallTargetKind::ClosureDefault { .. }))
                })
            })
        })
        .expect("caller");
    let other = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| match i.kind {
            InstructionKind::MakeClosure(_) => i.result,
            _ => None,
        })
        .nth(1)
        .expect("other closure");
    let call = function
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| {
            matches!(&i.kind, InstructionKind::Call(t)
            if matches!(t.kind, CallTargetKind::ClosureDefault { .. }))
        })
        .expect("closure default call");
    call.operands[0] = Operand::Value(other);
    let errors = subscript_codegen::lir::verify_module(&module).expect_err("wrong closure");
    assert!(format!("{errors:?}").contains("call target identity/signature is invalid"));
}
