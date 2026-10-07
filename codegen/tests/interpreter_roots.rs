//! Root parity controls cost one JIT session and one C build per input.

use subscript_codegen::{
    interpreter::{interpret, InterpretError},
    lir::lower_module,
    run_c_aot, run_jit, RunError,
};
use subscript_compiler::{check_program, SourceFile};

#[derive(Debug, PartialEq)]
enum Outcome {
    Output(Vec<u8>),
    Trap(Vec<u8>, String),
}

fn native(result: Result<Vec<u8>, RunError>) -> Outcome {
    match result {
        Ok(output) => Outcome::Output(output),
        Err(RunError::Trap(report)) => {
            eprintln!("trap site: {}", report.pos);
            Outcome::Trap(report.stdout, report.message)
        }
        Err(error) => panic!("native execution: {error}"),
    }
}

fn interpreted(result: Result<Vec<u8>, InterpretError>) -> Outcome {
    match result {
        Ok(output) => Outcome::Output(output),
        Err(InterpretError::Execution { output, source }) => match *source {
            InterpretError::Trap { message, .. } => Outcome::Trap(output, message),
            error => panic!("interpreter execution: {error}"),
        },
        Err(InterpretError::Trap { message, .. }) => Outcome::Trap(Vec::new(), message),
        Err(error) => panic!("interpreter execution: {error}"),
    }
}

fn measure(name: &str, source: &str) -> [Outcome; 3] {
    let files = [SourceFile::new(name, source)];
    let module = lower_module(&check_program(&files).expect("checked input")).expect("LIR");
    for function in &module.functions {
        if !function.locals.is_empty() {
            eprintln!(
                "{name} {} locals={:?}",
                function.source_name, function.locals
            );
        }
    }
    let outcomes = [
        native(run_jit(&files)),
        native(run_c_aot(&files)),
        interpreted(interpret(&module)),
    ];
    eprintln!("{name}: JIT, C, interpreter = {outcomes:?}");
    outcomes
}

const PREFIX: &str = "class Holder { task: Promise<void>; constructor(t: Promise<void>) { this.task = t; } }\nasync function fail(): Promise<void> { throw new Error(\"lost\"); }\n";

#[test]
fn conditional_await_loop_roots_each_value_and_its_survival_control() {
    let source = include_str!("../../corpus/trap/t100-conditional-await-collect.ts");
    let trap = measure("conditional-await.ts", source);
    let suspended = source.replace(
        "async function tick(): Promise<void> {}",
        "async function tick(): Promise<void> { await Context.suspend(); }",
    );
    let suspended_outcomes = measure("conditional-await-suspended.ts", &suspended);
    for outcome in suspended_outcomes {
        assert_eq!(
            outcome,
            Outcome::Trap(b"step 0\n".to_vec(), "Error: lost".into())
        );
    }
    let control = source
        .replace(
            "const h = new Holder(fail());",
            "const h = new Holder(fail()); if (i == 0) { kept = h; }",
        )
        .replace(
            "async function run():",
            "let kept:Holder|null=null;\nasync function run():",
        )
        .replace(
            "await run();",
            "await run();if(kept!=null){const t=kept.task;print(\"kept\");}",
        );
    let survived = measure("conditional-await-control.ts", &control);
    for outcome in trap {
        assert_eq!(
            outcome,
            Outcome::Trap(b"step 0\n".to_vec(), "Error: lost".into())
        );
    }
    for outcome in survived {
        assert_eq!(outcome, Outcome::Output(b"step 0\nstep 1\nkept\n".to_vec()));
    }
}

#[test]
fn addressed_locals_root_active_frames_and_finished_frames_root_nothing() {
    let programs = [
        ("addressed-local.ts", "export function main():void{const h:FixedArray<Holder,1>=[new Holder(fail())];const t=h[0].task;Context.collect();print(\"after\");}"),
        ("frame-local.ts", "async function tick():Promise<void>{await Context.suspend();}export async function main():Promise<void>{const h:FixedArray<Holder,1>=[new Holder(fail())];await tick();const t=h[0].task;Context.collect();print(\"after\");}"),
        ("finished-async-frame.ts", "let held:Promise<void>[]=[];async function run():Promise<void>{const h:FixedArray<Holder,1>=[new Holder(fail())];await Context.suspend();const t=h[0].task;}export async function main():Promise<void>{const p=run();held.push(p);await p;Context.collect();print(\"after\");}"),
        ("finished-frame.ts", "function* g():Generator<i32>{const h:FixedArray<Holder,1>=[new Holder(fail())];yield 1;const t=h[0].task;yield 2;}const held=g();export async function main():Promise<void>{held.next();held.next();held.next();await Context.suspend();Context.collect();print(\"after\");}"),
    ];
    let mut measured = Vec::new();
    for (name, body) in programs {
        measured.push(measure(name, &format!("{PREFIX}{body}")));
    }
    for (measured_index, outcomes) in measured.into_iter().enumerate() {
        for outcome in outcomes {
            let expected = if measured_index < 2 {
                Outcome::Output(b"after\n".to_vec())
            } else {
                Outcome::Trap(Vec::new(), "Error: lost".into())
            };
            assert_eq!(outcome, expected);
        }
    }
}

#[test]
fn kept_call_result_survives_the_next_loop_call_collection() {
    let source = include_str!("../../corpus/trap/t99-loop-result-collect.ts");
    let source = source.replace("for (let i: i32 = 0; i < 2; i++) { step(i); }", "const kept = step(0); for (let i: i32 = 1; i < 2; i++) { step(i); } const t = kept.task; print(\"kept\");");
    for outcome in measure("kept-loop-result.ts", &source) {
        assert_eq!(outcome, Outcome::Output(b"step 0\nstep 1\nkept\n".to_vec()));
    }
}

#[test]
fn current_t99_traps_in_each_tier() {
    for outcome in measure(
        "t99.ts",
        include_str!("../../corpus/trap/t99-loop-result-collect.ts"),
    ) {
        assert_eq!(
            outcome,
            Outcome::Trap(b"step 0\n".to_vec(), "Error: lost".into())
        );
    }
}

#[test]
fn finished_coroutine_does_not_root_its_activation_locals() {
    let source = format!("{PREFIX}function* g():Generator<i32>{{yield 1;const h:FixedArray<Holder,1>=[new Holder(fail())];const t=h[0].task;}}const held=g();export async function main():Promise<void>{{held.next();held.next();await Context.suspend();Context.collect();print(\"after\");}}");
    let outcomes = measure("finished-activation.ts", &source);
    for outcome in outcomes {
        assert_eq!(outcome, Outcome::Trap(Vec::new(), "Error: lost".into()));
    }
}

#[test]
fn coroutine_fields_survive_until_completion_with_controls() {
    let probes = [
        (
            "t101-unused-coroutine-parameter",
            include_str!("../../corpus/trap/t101-unused-coroutine-parameter.ts"),
            b"start\nafter\n".as_slice(),
            "    print(\"after\");",
            "    const t = h.task;\n    print(\"after\");",
        ),
        (
            "t102-generator-parameter",
            include_str!("../../corpus/trap/t102-generator-parameter.ts"),
            b"after\n".as_slice(),
            "    print(\"after\");",
            "    kept = h;\n    print(\"after\");",
        ),
        (
            "t103-unstarted-generator-parameter",
            include_str!("../../corpus/trap/t103-unstarted-generator-parameter.ts"),
            b"after\n".as_slice(),
            "function* work(h: Holder): Generator<i32> {",
            "function* work(h: Holder): Generator<i32> {\n    const t = h.task;",
        ),
        (
            "t104-coroutine-closure-environment",
            include_str!("../../corpus/trap/t104-coroutine-closure-environment.ts"),
            b"after\n".as_slice(),
            "    print(\"after\");",
            "    f();\n    print(\"after\");",
        ),
    ];
    for (name, source, output, from, to) in probes {
        for outcome in measure(name, source) {
            assert_eq!(
                outcome,
                Outcome::Trap(output.to_vec(), "Error: lost".into())
            );
        }
        let control = source
            .replace(from, to)
            .replace(" Context.collect(); }", " }")
            .replace(
                "    g.next();\n    Context.collect();\n}",
                "    g.next();\n}",
            );
        for outcome in measure(&format!("{name}-control.ts"), &control) {
            let output = if name.contains("unused-coroutine") {
                b"start\nafter\n".as_slice()
            } else {
                b"after\n".as_slice()
            };
            assert_eq!(outcome, Outcome::Output(output.to_vec()));
        }
    }
}

#[test]
fn value_classes_reject_reference_fields() {
    let source = format!(
        "{PREFIX}@ValueType class V {{ h: Holder; constructor(h: Holder) {{ this.h = h; }} }}"
    );
    let errors = check_program(&[SourceFile::new("value-reference.ts", source)])
        .expect_err("reference field must fail");
    assert!(errors
        .iter()
        .any(|error| error.code.to_string() == "S100"
            && error.message.contains("value-class whitelist")));
}

#[test]
fn nested_closure_environments_survive_until_completion() {
    let source = include_str!("../../corpus/trap/t104-coroutine-closure-environment.ts").replace(
        "const f = (): void => { const t = h.task; };",
        "const inner = (): void => { const t = h.task; }; const f = (): void => { inner(); };",
    );
    for outcome in measure("nested-dead.ts", &source) {
        assert_eq!(
            outcome,
            Outcome::Trap(b"after\n".to_vec(), "Error: lost".into())
        );
    }
    let control = source
        .replace(" Context.collect(); }", " }")
        .replace("    print(\"after\");", "    f(); print(\"after\");");
    for outcome in measure("nested-live.ts", &control) {
        assert_eq!(outcome, Outcome::Output(b"after\n".to_vec()));
    }
}

#[test]
fn defined_closure_environment_survives_function_replacement() {
    let source = format!("{PREFIX}async function tick():Promise<void>{{}} function plain():i32{{return 2;}} export async function main():Promise<void>{{const h=new Holder(fail());let f:()=>i32=():i32=>{{const t=h.task;return 1;}};for(let i:i32=0;i<2;i++){{await tick();Context.collect();print(`value=${{f()}}`);f=plain;}}}}");
    for outcome in measure("replaced-capture.ts", &source) {
        assert_eq!(outcome, Outcome::Output(b"value=1\nvalue=2\n".to_vec()));
    }
    let control = source.replace("f=plain;", "");
    for outcome in measure("retained-capture.ts", &control) {
        assert_eq!(outcome, Outcome::Output(b"value=1\nvalue=1\n".to_vec()));
    }
}

#[test]
fn mutable_coroutine_parameter_survives_resume_until_completion() {
    let source = format!("{PREFIX}function* work(h:Holder|null):Generator<i32>{{yield 1;if(h!=null){{const t=h.task;print(\"found\");}}h=null;}}export async function main():Promise<void>{{const g=work(new Holder(fail()));g.next();await Context.suspend();Context.collect();print(\"kept\");g.next();Context.collect();print(\"after\");}}");
    for outcome in measure("mutable-parameter.ts", &source) {
        assert_eq!(
            outcome,
            Outcome::Trap(b"kept\nfound\n".to_vec(), "Error: lost".into())
        );
    }
}

#[test]
fn awaited_nested_closure_retains_caller_environment() {
    for outcome in measure(
        "a342.ts",
        include_str!("../../corpus/accept/a342-awaited-nested-closure.ts"),
    ) {
        assert_eq!(outcome, Outcome::Output(b"8\n8\n".to_vec()));
    }
}

#[test]
fn awaited_nested_closure_survives_actual_suspension() {
    let source = include_str!("../../corpus/accept/a342-awaited-nested-closure.ts").replace(
        "async function tick(): Promise<void> {}",
        "async function tick(): Promise<void> { await Context.suspend(); }",
    );
    for outcome in measure("nested-suspended.ts", &source) {
        assert_eq!(outcome, Outcome::Output(b"8\n8\n".to_vec()));
    }
}
