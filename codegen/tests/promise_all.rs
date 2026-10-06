use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

fn compare(source: &str, golden: &[u8]) {
    let files = [SourceFile::new("promise-all.ts", source)];
    let hir = check_program(&files).expect("fixture checks");
    let lir = lower_module(&hir).expect("fixture lowers");
    assert_eq!(interpret(&lir).expect("interpreter"), golden);
    assert_eq!(run_jit(&files).expect("dev JIT"), golden);
    assert_eq!(run_c_aot(&files).expect("ship C AOT"), golden);
}

#[test]
fn aggregate_corpus_matches_three_tiers_and_node_golden() {
    compare(
        include_str!("../../corpus/accept/a335-promise-all.ts"),
        include_bytes!("../../corpus/accept/a335-promise-all.expected"),
    );
}

#[test]
fn non_counted_array_results_and_direct_await_control_match_three_tiers() {
    for aggregate in [false, true] {
        let completion = if aggregate {
            "const jobs: Promise<f64[]>[] = [result()]; const xs: f64[][] = await Promise.all(jobs); print(`${xs[0][0]}`);"
        } else {
            "const xs: f64[] = await result(); print(`${xs[0]}`);"
        };
        compare(&format!("async function value(): Promise<i32> {{ return 0; }}
            async function result(): Promise<f64[]> {{ const xs: f64[] = [7]; await value(); return xs; }}
            export async function main(): Promise<void> {{ {completion} }}"), b"7\n");
    }
}

#[test]
fn partial_reference_results_survive_collection_with_a_control() {
    for collect in [false, true] {
        let checkpoint = if collect { "Context.collect();" } else { "" };
        compare(
            &format!(
                "class Box {{ n: i32; constructor(n: i32) {{ this.n = n; }} }}
            async function value(): Promise<i32> {{ return 0; }}
            async function box(n: i32, turns: i32): Promise<Box> {{
                for (let i: i32 = 0; i < turns; i++) {{ await value(); }}
                return new Box(n);
            }}
            export async function main(): Promise<void> {{
                const jobs: Promise<Box>[] = [box(7, 0), box(11, 5)];
                const all: Promise<Box[]> = Promise.all(jobs);
                await value(); {checkpoint}
                const xs: Box[] = await all;
                {checkpoint}
                print(`${{xs[0].n}} ${{xs[1].n}}`);
            }}"
            ),
            b"7 11\n",
        );
    }
}

#[test]
fn temporary_input_arrays_release_their_counts_with_a_stored_array_control() {
    for temporary in [false, true] {
        let completion = if temporary {
            "await Promise.all([value()]);"
        } else {
            "const jobs: Promise<i32>[] = [value()]; await Promise.all(jobs);"
        };
        let source = format!(
            "async function value(): Promise<i32> {{ return 7; }}
            async function exercise(): Promise<void> {{ {completion} }}
            export async function main(): Promise<void> {{ await exercise(); }}
            export function collect(): void {{ Context.collect(); }}"
        );
        let files = [SourceFile::new("temporary-input.ts", source)];
        let mut session = subscript_codegen::ReloadSession::new(&files).expect("session");
        session.call_main().expect("root starts");
        while session.async_pending() != 0 {
            session.async_step().expect("checkpoint");
        }
        session.call_export("collect").expect("explicit collection");
        assert_eq!(session.live_allocations(), 0, "temporary={temporary}");
    }
}

#[test]
fn unobserved_aggregate_exception_traps_with_an_observed_control() {
    let source = include_str!("../../corpus/trap/t83-unobserved-aggregate-exception.ts");
    let files = [SourceFile::new("aggregate-trap.ts", source)];
    let hir = check_program(&files).expect("trap fixture checks");
    let lir = lower_module(&hir).expect("trap fixture lowers");
    let error = interpret(&lir).expect_err("the aggregate exception is unobserved");
    assert_eq!(error.output(), b"release aggregate\n");
    let subscript_codegen::interpreter::InterpretError::Execution { source, .. } = error else {
        panic!("the interpreter preserves pre-trap output");
    };
    let subscript_codegen::interpreter::InterpretError::Trap {
        runtime_kind,
        pos,
        message,
        ..
    } = *source
    else {
        panic!("the last release reports a trap");
    };
    assert_eq!(
        runtime_kind,
        Some(subscript_runtime::TrapKind::UncaughtException)
    );
    assert_eq!((pos.line, pos.col), (10, 40));
    assert_eq!(message, "Error: aggregate dropped");
    let observed = include_str!("../../corpus/trap/t83-unobserved-aggregate-exception.ts").replace(
        "  print(\"release aggregate\");",
        "  try { await holder.job; } catch (e) {}\n  print(\"release aggregate\");",
    );
    compare(&observed, b"release aggregate\nunreached\n");
}
