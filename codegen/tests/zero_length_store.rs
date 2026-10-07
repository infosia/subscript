//! A zero store clears an array in each execution tier (§174).

use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn zero_store_corpus_agrees_in_three_tiers() {
    let files = [SourceFile::new(
        "a344.ts",
        include_str!("../../corpus/accept/a344-zero-length-store.ts"),
    )];
    let expected = include_bytes!("../../corpus/accept/a344-zero-length-store.expected");
    let hir = check_program(&files).expect("accepted zero store");
    let lir = lower_module(&hir).expect("verified clear");
    assert_eq!(interpret(&lir).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("C AOT"), expected);
}

#[test]
fn clear_evaluates_a_receiver_once_and_releases_temporary_receivers() {
    let source = r#"
let calls: i32 = 0;
let values: i32[] = [1, 2];
function receiver(): i32[] { calls += 1; return values; }
async function work(): Promise<i32> { return 7; }
async function use(): Promise<void> {
  const h = work(); await h;
  [h, h].length = 0;
  receiver().length = 0;
  print(`${calls} ${values.length} ${await h}`);
}
export async function main(): Promise<void> { await use(); }
"#;
    let files = [SourceFile::new("receiver.ts", source)];
    let lir = lower_module(&check_program(&files).expect("receiver fixture")).expect("clear LIR");
    assert_eq!(interpret(&lir).expect("interpreter"), b"1 0 7\n");
    assert_eq!(run_jit(&files).expect("JIT"), b"1 0 7\n");
    assert_eq!(run_c_aot(&files).expect("C AOT"), b"1 0 7\n");
    let mut session = subscript_codegen::ReloadSession::new(&files).expect("session");
    session.call_main().expect("main");
    while session.async_pending() != 0 {
        session.async_step().expect("checkpoint");
    }
    assert!(session.async_tasks().is_empty());
}

#[test]
fn clear_traps_before_the_next_statement_like_a_discarded_pop() {
    let corpus = include_str!("../../corpus/trap/t102-zero-length-store.ts");
    for clear in [true, false] {
        let source = corpus
            .replace(
                "jobs.length = 0;",
                if clear {
                    "jobs.length = 0;"
                } else {
                    "jobs.pop();"
                },
            )
            .replace(
                "else { await jobs[0]; }",
                "else { await jobs[0]; } print(\"after\");",
            );
        let files = [SourceFile::new("release-trap.ts", source)];
        let lir =
            lower_module(&check_program(&files).expect("trap fixture")).expect("verified trap");
        assert!(interpret(&lir).is_err());
        for result in [run_jit(&files), run_c_aot(&files)] {
            let subscript_codegen::RunError::Trap(trap) = result.expect_err("release traps") else {
                panic!("expected uncaught exception");
            };
            assert_eq!(trap.rule, subscript_runtime::TrapKind::UncaughtException);
            assert!(trap.stdout.is_empty());
            assert_eq!((trap.pos.line, trap.pos.col), (10, 40));
        }
    }
}
