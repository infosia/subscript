//! `then`, `catch`, and `finally` on a handle in the three tiers (compiler.md §186).
//!
//! The golden sweeps run `a357`–`a361` in each tier, so this file does not
//! run them again.
use subscript_codegen::{
    add_c11_optimized_flags, add_executable_output, add_object_directory, emit_c, host_c_compiler,
    include_directory_arg, interpreter::interpret, lir::lower_module, run_c_aot, run_jit,
    runtime_staticlib_path, runtime_system_libraries, tool_output_report, ReloadSession, RunError,
    AOT_ENTRY_C,
};
use subscript_compiler::{check_program, lir as l, SourceFile};
use subscript_runtime::TrapKind;

fn files(source: &str) -> [SourceFile; 1] {
    [SourceFile::new("promise-reaction.ts", source)]
}

fn three_tiers(source: &str) -> [Vec<u8>; 3] {
    let files = files(source);
    let hir = check_program(&files).expect("checks");
    [
        interpret(&lower_module(&hir).expect("lowers")).expect("interpreter"),
        run_jit(&files).expect("JIT"),
        run_c_aot(&files).expect("C AOT"),
    ]
}

/// The turn counts of §186 rule 3. `EXPECTED` is the output of `node`
/// v24.18.0 for this program through `corpus/node/run-js-corpus.cjs`.
const TICKS: &str = r#"let tick: i32 = 0;
async function now(n: i32): Promise<i32> { return n; }
async function nothing(): Promise<void> { }
async function fail(): Promise<i32> { throw new Error("x"); }
async function failVoid(): Promise<void> { throw new Error("x"); }
async function ticker(): Promise<void> {
  for (let i: i32 = 0; i < 400; i++) { tick = tick + 1; await now(0); }
}
async function measure(name: string, h: Promise<i32>): Promise<void> {
  const t0 = tick;
  try { await h; } catch (e) { }
  print(`${name} ${tick - t0}`);
}
async function measureVoid(name: string, h: Promise<void>): Promise<void> {
  const t0 = tick;
  try { await h; } catch (e) { }
  print(`${name} ${tick - t0}`);
}
export async function main(): Promise<void> {
  const t = ticker();
  const t0 = tick;
  await now(0);
  print(`await-completed ${tick - t0}`);
  await measure("plain", now(1));
  await measure("then", now(1).then((v: i32): i32 => v));
  await measure("then-zero-parameters", now(1).then((): i32 => 2));
  await measureVoid("then-void", now(1).then((v: i32): void => { }));
  await measure("then-adopt", now(1).then((v: i32): Promise<i32> => now(v)));
  await measureVoid("then-adopt-void", now(1).then((v: i32): Promise<void> => nothing()));
  await measure("then-then", now(1).then((v: i32): i32 => v).then((v: i32): i32 => v));
  await measure("then-on-rejected", fail().then((v: i32): i32 => v));
  await measure("then2-fulfilled", now(1).then((v: i32): i32 => v, (e: Error): i32 => 0));
  await measure("then2-rejected", fail().then((v: i32): i32 => v, (e: Error): i32 => 0));
  await measure("then2-rejected-adopt", fail().then((v: i32): Promise<i32> => now(v), (e: Error): Promise<i32> => now(0)));
  await measure("catch-rejected", fail().catch((e: Error): i32 => 0));
  await measure("catch-fulfilled", now(1).catch((e: Error): i32 => 0));
  await measure("catch-adopt", fail().catch((e: Error): Promise<i32> => now(0)));
  await measureVoid("catch-void", failVoid().catch((e: Error): void => { }));
  await measure("finally-fulfilled", now(1).finally((): void => { }));
  await measure("finally-rejected", fail().finally((): void => { }));
  await measureVoid("finally-void", nothing().finally((): void => { }));
  const k: i32 = 5;
  await measure("then-capture", now(1).then((v: i32): i32 => v + k));
  await measure("catch-capture", fail().catch((e: Error): i32 => k));
  await measure("finally-capture", now(1).finally((): void => { tick = tick + k - k; }));
  await measure("all", Promise.all([now(1), now(2)]).then((xs: i32[]): i32 => xs[0]));
  await t;
}
"#;

const TICKS_NODE: &str = "await-completed 1\nplain 1\nthen 2\nthen-zero-parameters 2\n\
then-void 2\nthen-adopt 4\nthen-adopt-void 4\nthen-then 3\nthen-on-rejected 2\n\
then2-fulfilled 2\nthen2-rejected 2\nthen2-rejected-adopt 4\ncatch-rejected 2\n\
catch-fulfilled 2\ncatch-adopt 4\ncatch-void 2\nfinally-fulfilled 4\nfinally-rejected 4\n\
finally-void 4\nthen-capture 2\ncatch-capture 2\nfinally-capture 4\nall 3\n";

// Cost: one interpreter run, one JIT run, and one C build.
#[test]
fn turn_counts_match_node_in_three_tiers() {
    for output in three_tiers(TICKS) {
        assert_eq!(String::from_utf8_lossy(&output), TICKS_NODE);
    }
}

/// A capturing callback, collected and overwritten while its chain is
/// pending; the control has the same shape and no captures.
fn rooting_source(capture: bool) -> String {
    let callback = if capture {
        "(v: i32): string => `${v + base} ${box.n}`"
    } else {
        "(v: i32): string => `${v + 40} ${v}`"
    };
    format!(
        "class Box {{ n: i32 = 0; constructor(n: i32) {{ this.n = n; }} }}
        class Churn {{ a: i32 = -1; b: Box = new Box(-2); }}
        async function later(n: i32): Promise<i32> {{ await Context.suspend(); return n; }}
        function start(k: i32): Promise<string> {{
            const base: i32 = k * 10;
            const box = new Box(k);
            return later(k).then({callback});
        }}
        function churn(): void {{
            Context.collect();
            const junk: Churn[] = [];
            for (let i: i32 = 0; i < 100; i++) {{ junk.push(new Churn()); }}
        }}
        export async function main(): Promise<void> {{
            const h = start(4);
            churn();
            print(await h);
            const f = later(1).finally((): void => {{ churn(); }});
            churn();
            print(`${{await f}}`);
        }}"
    )
}

/// The environment classes and the owned-environment parameters of a module.
fn environments(source: &str) -> (usize, usize) {
    let lir = lower_module(&check_program(&files(source)).expect("checks")).expect("lowers");
    let classes = lir
        .classes
        .iter()
        .filter(|class| class.source_name.starts_with("<callback environment"))
        .count();
    let parameters = lir
        .functions
        .iter()
        .flat_map(|function| &function.parameters)
        .filter(|parameter| parameter.kind == l::ParameterKind::OwnedEnvironment)
        .count();
    (classes, parameters)
}

/// Emits `source` as C and stores the environment word of each function
/// value pair as `env ^ 1`; each indirect call decodes it. The collector
/// marks exact payload addresses only, so the stored word does not root
/// the environment. Returns the number of edited pairs, the number of
/// edited calls, and the program output.
fn c_aot_with_hidden_environments(source: &str) -> (usize, usize, Result<Vec<u8>, String>) {
    let hir = check_program(&files(source)).expect("checks");
    let program = emit_c(&hir).expect("emits C");
    let mut pairs = 0;
    let mut calls = 0;
    let mut text = String::new();
    let mut rest = program.source.as_str();
    while let Some(start) = rest.find("(SubFn){ (void*)&sub_f") {
        let (before, pair) = rest.split_at(start);
        let comma = pair.find(", ").expect("pair operand");
        let close = pair.find(" }").expect("pair end");
        text.push_str(before);
        text.push_str(&pair[..comma + 2]);
        text.push_str(&format!(
            "(void*)((uintptr_t)({}) ^ (uintptr_t)1u)",
            &pair[comma + 2..close]
        ));
        rest = &pair[close..];
        pairs += 1;
    }
    text.push_str(rest);
    let mut decoded = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find(".code))(ctx, ") {
        let (before, call) = rest.split_at(start + ".code))(ctx, ".len());
        let end = call.find(".env").expect("environment operand");
        decoded.push_str(before);
        decoded.push_str(&format!(
            "(void*)((uintptr_t){}.env ^ (uintptr_t)1u)",
            &call[..end]
        ));
        rest = &call[end + ".env".len()..];
        calls += 1;
    }
    decoded.push_str(rest);
    let directory = std::env::temp_dir().join(format!("subscript-s186-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("directory");
    std::fs::write(directory.join("program.c"), decoded).expect("program");
    std::fs::write(directory.join("program.h"), &program.host_header).expect("header");
    std::fs::write(directory.join("entry.c"), AOT_ENTRY_C).expect("entry");
    let compiler = host_c_compiler().expect("host compiler");
    let executable = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    let mut command = compiler.command();
    add_c11_optimized_flags(&mut command, compiler.style());
    add_object_directory(&mut command, &directory, compiler.style());
    command
        .arg(include_directory_arg(compiler.style(), &directory))
        .arg(directory.join("program.c"))
        .arg(directory.join("entry.c"))
        .arg(runtime_staticlib_path().expect("runtime archive"))
        .args(runtime_system_libraries(compiler.style()));
    add_executable_output(&mut command, &executable, compiler.style());
    let compiled = command.output().expect("C compiler");
    assert!(
        compiled.status.success(),
        "{}",
        tool_output_report(&compiled)
    );
    let run = std::process::Command::new(&executable)
        .output()
        .expect("run");
    let _ = std::fs::remove_dir_all(&directory);
    let output = if run.status.success() {
        Ok(run.stdout)
    } else {
        Err(tool_output_report(&run))
    };
    (pairs, calls, output)
}

// Cost: two checker/lowering calls, two interpreter runs, two JIT runs,
// and three C builds. The firing control is the third C build.
//
// Firing control: the C program with hidden environment words fails the
// output check. The interpreter cannot fail this check: it holds each
// value in an `Rc`, so its leg is an output check only. The dev JIT runs
// only from source, and no test entry takes a changed module, so its leg
// is an output check too; it uses the same runtime collector.
#[test]
fn callback_environment_survives_collection_in_three_tiers_with_a_control_without_one() {
    const EXPECTED: &[u8] = b"44 4\n1\n";
    for capture in [true, false] {
        let source = rooting_source(capture);
        let expected = (usize::from(capture), usize::from(capture));
        assert_eq!(environments(&source), expected, "capture={capture}");
        for output in three_tiers(&source) {
            assert_eq!(output, EXPECTED, "capture={capture}");
        }
    }
    let (pairs, calls, output) = c_aot_with_hidden_environments(&rooting_source(true));
    // The capturing `then` pair and the non-capturing `finally` pair; the
    // `then` call, and the `finally` calls of the normal and the exception path.
    assert_eq!((pairs, calls), (2, 3));
    assert_ne!(
        output.as_deref(),
        Ok(EXPECTED),
        "an unrooted environment changes the output"
    );
}

// Cost: two JIT sessions.
#[test]
fn an_environment_stays_until_an_explicit_collection_with_a_control_without_one() {
    for capture in [true, false] {
        let callback = if capture {
            "(v: i32): i32 => v + k"
        } else {
            "(v: i32): i32 => v + 1"
        };
        let source = format!(
            "async function now(n: i32): Promise<i32> {{ return n; }}
            export async function chains(): Promise<void> {{
                const k: i32 = 1;
                for (let i: i32 = 0; i < 100; i++) {{ await now(i).then({callback}); }}
            }}
            export function collect(): void {{ Context.collect(); }}"
        );
        let mut session = ReloadSession::new(&files(&source)).expect("session");
        let baseline = session.live_allocations();
        session.call_export("chains").expect("chains");
        while session.async_pending() != 0 {
            session.async_step().expect("step");
        }
        let after = session.live_allocations();
        session.call_export("collect").expect("collect");
        let collected = session.live_allocations();
        assert_eq!(
            (after - baseline, collected - baseline),
            if capture { (100, 0) } else { (0, 0) },
            "capture={capture}: invariant 2 keeps each environment until collection"
        );
    }
}

// Cost: four JIT sessions, for the two reload policies and their controls.
#[test]
fn a_callback_edit_reloads_and_a_suspended_chain_is_stale() {
    let source = "async function later(n: i32): Promise<i32> { await Context.suspend(); return n; }
        export async function main(): Promise<void> {
            const k: i32 = 1;
            print(`${await later(1).then((v: i32): i32 => v + k)}`);
        }";
    let edited = source.replace("v + k", "v + k + 10");
    for suspended in [false, true] {
        for reload in [false, true] {
            let mut session = ReloadSession::new(&files(source)).expect("session");
            if suspended {
                session.call_main().expect("suspend");
            }
            if reload {
                session.reload(&files(&edited)).expect("an accepted swap");
            }
            if !suspended {
                session.call_main().expect("main");
            }
            if suspended && reload {
                match session.async_step() {
                    Err(RunError::Trap(trap)) => assert_eq!(trap.rule, TrapKind::StaleCoroutine),
                    other => panic!("expected a stale frame: {other:?}"),
                }
                continue;
            }
            while session.async_pending() != 0 {
                session.async_step().expect("step");
            }
            let expected: &[u8] = if reload { b"12\n" } else { b"2\n" };
            assert_eq!(session.take_output(), expected, "reload={reload}");
        }
    }
}
