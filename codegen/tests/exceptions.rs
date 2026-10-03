//! Three-engine agreement for the exception core (`compiler.md` §115) and
//! for the delivery of an async body's exception at its `await` (§116).
//!
//! Each program runs on the dev JIT, the ship C tier, and the reference
//! interpreter. Stdout and the trap tuple must agree on all three, and
//! each program's output is written out here as a hand-checked value
//! (CLAUDE.md core principle 12).
//!
//! Cost: each case compiles one C program; the file holds few cases.
//! The `using` exit cases hold a quiet scope and a raising scope in one
//! program each, so each exit kind of §115.5 rule 5 costs one C program.

use subscript_codegen::interpreter::{interpret, InterpretError};
use subscript_codegen::{run_c_aot, run_jit, RunError, TrapReport};
use subscript_compiler::{check_program, SourceFile};
use subscript_runtime::TrapKind;

fn sources(source: &str) -> Vec<SourceFile> {
    vec![SourceFile::new("exception.ts", source)]
}

fn interpreted(files: &[SourceFile]) -> Result<Vec<u8>, InterpretError> {
    let hir = check_program(files).expect("the program checks clean");
    let lir = subscript_codegen::lir::lower_module(&hir).expect("the program lowers");
    interpret(&lir)
}

/// Runs `source` on the three engines and returns the shared stdout.
fn agree_on_stdout(source: &str) -> String {
    let files = sources(source);
    let jit = run_jit(&files).unwrap_or_else(|error| panic!("dev JIT: {error}"));
    let ship = run_c_aot(&files).unwrap_or_else(|error| panic!("ship C: {error}"));
    let interpreter = interpreted(&files).unwrap_or_else(|error| panic!("interpreter: {error}"));
    assert_eq!(
        String::from_utf8_lossy(&jit),
        String::from_utf8_lossy(&ship),
        "dev JIT and ship C disagree"
    );
    assert_eq!(
        String::from_utf8_lossy(&jit),
        String::from_utf8_lossy(&interpreter),
        "dev JIT and the interpreter disagree"
    );
    String::from_utf8(jit).expect("UTF-8 stdout")
}

/// Runs `source` on the three engines, requires one trap, and returns it.
fn agree_on_trap(source: &str) -> TrapReport {
    let files = sources(source);
    let jit = match run_jit(&files) {
        Err(RunError::Trap(report)) => report,
        other => panic!("dev JIT did not trap: {other:?}"),
    };
    let ship = match run_c_aot(&files) {
        Err(RunError::Trap(report)) => report,
        other => panic!("ship C did not trap: {other:?}"),
    };
    assert_eq!(jit, ship, "dev JIT and ship C report different traps");
    match interpreted(&files) {
        Err(InterpretError::Execution { output, source }) => {
            assert_eq!(output, jit.stdout, "interpreter stdout before the trap");
            match *source {
                InterpretError::Trap {
                    kind, pos, message, ..
                } => {
                    assert_eq!(kind, jit.rule.rule(), "interpreter trap kind");
                    assert_eq!(pos, jit.pos, "interpreter trap position");
                    // The interpreter reports its own text for an emitted
                    // check; the uncaught-exception text is the Error's.
                    if jit.rule == TrapKind::UncaughtException {
                        assert_eq!(message, jit.message, "interpreter trap message");
                    }
                }
                other => panic!("interpreter stopped without a trap: {other}"),
            }
        }
        other => panic!("interpreter did not trap: {other:?}"),
    }
    jit
}

#[test]
fn a_catch_clears_the_word_and_later_operations_run() {
    let stdout = agree_on_stdout(
        "function fail(n: i32): i32 {\n\
         \x20 if (n > 1) { throw new TypeError(`bad ${n}`); }\n\
         \x20 return n * 10;\n\
         }\n\
         export function main(): void {\n\
         \x20 let total: i32 = 0;\n\
         \x20 for (let i: i32 = 0; i < 4; i++) {\n\
         \x20   try {\n\
         \x20     total += fail(i);\n\
         \x20   } catch (e) {\n\
         \x20     if (e instanceof TypeError) { print(e.message); }\n\
         \x20   }\n\
         \x20 }\n\
         \x20 const values: i32[] = [1, 2];\n\
         \x20 values.push(3);\n\
         \x20 print(`${total} ${values.length}`);\n\
         }\n",
    );
    assert_eq!(stdout, "bad 2\nbad 3\n10 3\n");
}

#[test]
fn a_mutable_binding_keeps_its_value_at_the_raise_site() {
    let stdout = agree_on_stdout(
        "function fail(): void { throw new Error(\"x\"); }\n\
         export function main(): void {\n\
         \x20 let step: i32 = 0;\n\
         \x20 let text: string = \"a\";\n\
         \x20 try {\n\
         \x20   step = 1;\n\
         \x20   text += \"b\";\n\
         \x20   fail();\n\
         \x20   step = 2;\n\
         \x20   text += \"c\";\n\
         \x20 } catch {\n\
         \x20   step += 10;\n\
         \x20 }\n\
         \x20 print(`${step} ${text}`);\n\
         }\n",
    );
    assert_eq!(stdout, "11 ab\n");
}

#[test]
fn map_and_set_callbacks_propagate_through_the_runtime_loop() {
    let stdout = agree_on_stdout(
        "export function main(): void {\n\
         \x20 const map: Map<i32, i32> = new Map<i32, i32>();\n\
         \x20 map.set(1, 10);\n\
         \x20 map.set(2, 20);\n\
         \x20 map.set(3, 30);\n\
         \x20 const stop = (value: i32, key: i32): void => {\n\
         \x20   if (key === 2) { throw new Error(`map stopped at ${key}`); }\n\
         \x20   print(`map ${key} ${value}`);\n\
         \x20 };\n\
         \x20 try {\n\
         \x20   map.forEach(stop);\n\
         \x20 } catch (e) {\n\
         \x20   if (e instanceof Error) { print(e.message); }\n\
         \x20 }\n\
         \x20 const set: Set<i32> = new Set<i32>();\n\
         \x20 set.add(5);\n\
         \x20 set.add(6);\n\
         \x20 const each = (value: i32): void => {\n\
         \x20   if (value === 6) { throw new SyntaxError(`set stopped at ${value}`); }\n\
         \x20   print(`set ${value}`);\n\
         \x20 };\n\
         \x20 try {\n\
         \x20   set.forEach(each);\n\
         \x20 } catch (e) {\n\
         \x20   if (e instanceof SyntaxError) { print(e.message); }\n\
         \x20 }\n\
         \x20 const values: i32[] = [3, 1, 2];\n\
         \x20 const compare = (a: i32, b: i32): i32 => {\n\
         \x20   if (a === 2 || b === 2) { throw new Error(\"sort stopped\"); }\n\
         \x20   return a - b;\n\
         \x20 };\n\
         \x20 try {\n\
         \x20   values.sort(compare);\n\
         \x20 } catch (e) {\n\
         \x20   if (e instanceof Error) { print(e.message); }\n\
         \x20 }\n\
         \x20 print(\"after\");\n\
         }\n",
    );
    assert_eq!(
        stdout,
        "map 1 10\nmap stopped at 2\nset 5\nset stopped at 6\nsort stopped\nafter\n"
    );
}

#[test]
fn a_constructor_raise_reaches_the_constructing_caller() {
    let stdout = agree_on_stdout(
        "class Checked {\n\
         \x20 value: i32;\n\
         \x20 constructor(value: i32) {\n\
         \x20   if (value < 0) { throw new Error(`negative ${value}`); }\n\
         \x20   this.value = value;\n\
         \x20 }\n\
         }\n\
         export function main(): void {\n\
         \x20 try {\n\
         \x20   const a: Checked = new Checked(4);\n\
         \x20   print(`${a.value}`);\n\
         \x20   const b: Checked = new Checked(-1);\n\
         \x20   print(`${b.value}`);\n\
         \x20 } catch (e) {\n\
         \x20   if (e instanceof Error) { print(e.message); }\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!(stdout, "4\nnegative -1\n");
}

#[test]
fn an_uncaught_exception_is_one_trap_on_every_engine() {
    let report = agree_on_trap(
        "function inner(): void {\n\
         \x20 throw new SyntaxError(\"deep\");\n\
         }\n\
         export function main(): void {\n\
         \x20 print(\"before\");\n\
         \x20 try {\n\
         \x20   print(\"guarded\");\n\
         \x20 } catch (e) {\n\
         \x20   print(\"never\");\n\
         \x20 }\n\
         \x20 inner();\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "SyntaxError: deep");
    assert_eq!((report.pos.line, report.pos.col), (2, 3));
    assert_eq!(report.stdout, b"before\nguarded\n");
}

#[test]
fn a_rethrow_reports_the_position_of_the_last_throw() {
    let report = agree_on_trap(
        "export function main(): void {\n\
         \x20 try {\n\
         \x20   throw new Error();\n\
         \x20 } catch (e) {\n\
         \x20   throw e;\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "Error");
    assert_eq!((report.pos.line, report.pos.col), (5, 5));
}

#[test]
fn a_trap_inside_a_try_block_is_not_caught() {
    let report = agree_on_trap(
        "export function main(): void {\n\
         \x20 const values: i32[] = [1];\n\
         \x20 try {\n\
         \x20   print(`${values[4]}`);\n\
         \x20 } catch (e) {\n\
         \x20   print(\"caught\");\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::IndexOutOfBounds);
    assert_eq!(report.stdout, b"");
}

/// compiler.md §116.1 rules 2 and 5: an `await` raises the exception of its
/// handle with the report text and the position of the `throw`, on a
/// settled await and on a resume that a host checkpoint runs. The
/// host-kicked root has no script holder, so the exception becomes the
/// uncaught-exception trap there.
#[test]
fn an_await_raises_with_the_text_and_position_of_the_throw() {
    let report = agree_on_trap(
        "async function settled(): Promise<i32> { return 1; }\n\
         async function late(): Promise<i32> {\n\
         \x20 const value: i32 = await settled();\n\
         \x20 print(`late:${value}`);\n\
         \x20 throw new SyntaxError(\"after await\");\n\
         }\n\
         export async function main(): Promise<void> {\n\
         \x20 const handle: Promise<i32> = late();\n\
         \x20 print(\"main:held\");\n\
         \x20 print(`main:${await handle}`);\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "SyntaxError: after await");
    assert_eq!((report.pos.line, report.pos.col), (5, 3));
    assert_eq!(report.stdout, b"main:held\nlate:1\n");

    let report = agree_on_trap(
        "async function parked(): Promise<i32> {\n\
         \x20 await Context.suspend();\n\
         \x20 print(\"parked:resumed\");\n\
         \x20 throw new TypeError(\"after the checkpoint\");\n\
         }\n\
         export async function main(): Promise<void> {\n\
         \x20 print(`main:${await parked()}`);\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "TypeError: after the checkpoint");
    assert_eq!((report.pos.line, report.pos.col), (4, 3));
    assert_eq!(report.stdout, b"parked:resumed\n");
}

/// compiler.md §115.4 item 3: `next()` is not a raise site, so a handler
/// around a callback that drives a generator does not catch; a handler
/// inside the generator body does.
#[test]
fn a_generator_body_catches_inside_and_traps_at_its_boundary() {
    let stdout = agree_on_stdout(
        "function fail(n: i32): void { if (n === 1) { throw new Error(`bad ${n}`); } }\n\
         function* guarded(): Generator<i32> {\n\
         \x20 for (let n: i32 = 0; n < 3; n++) {\n\
         \x20   try {\n\
         \x20     fail(n);\n\
         \x20   } catch (e) {\n\
         \x20     if (e instanceof Error) { print(`inside ${e.message}`); }\n\
         \x20   }\n\
         \x20   yield n;\n\
         \x20 }\n\
         }\n\
         export function main(): void {\n\
         \x20 for (const n of guarded()) { print(`n=${n}`); }\n\
         }\n",
    );
    assert_eq!(stdout, "n=0\ninside bad 1\nn=1\nn=2\n");

    let report = agree_on_trap(
        "function* numbers(): Generator<i32> {\n\
         \x20 yield 1;\n\
         \x20 throw new Error(\"second\");\n\
         }\n\
         export function main(): void {\n\
         \x20 const drive = (): void => {\n\
         \x20   const it = numbers();\n\
         \x20   print(`${it.next().value}`);\n\
         \x20   print(`${it.next().value}`);\n\
         \x20 };\n\
         \x20 const runs: i32[] = [0];\n\
         \x20 try {\n\
         \x20   runs.forEach((run: i32): void => { drive(); });\n\
         \x20 } catch {\n\
         \x20   print(\"caught\");\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "Error: second");
    assert_eq!((report.pos.line, report.pos.col), (3, 3));
    assert_eq!(report.stdout, b"1\n");
}

/// compiler.md §115.4 item 4: an exception that leaves a Worker entry is
/// the Worker Context's trap, and the join reports it as every Worker trap
/// is reported. The interpreter has no Worker adapter.
#[test]
fn an_exception_that_leaves_a_worker_entry_is_the_worker_trap() {
    let files = sources(
        "class Job {\n\
         \x20 value: i32;\n\
         \x20 constructor(value: i32) { this.value = value; }\n\
         }\n\
         function work(inbox: Inbox<Job>, outbox: Outbox<Job>): void {\n\
         \x20 const job: Job | null = inbox.wait();\n\
         \x20 if (job !== null) {\n\
         \x20   throw new TypeError(`job ${job.value} failed`);\n\
         \x20 }\n\
         }\n\
         export function main(): void {\n\
         \x20 const worker: Worker<Job, Job> = Worker.spawn(work);\n\
         \x20 worker.post(new Job(7));\n\
         \x20 worker.close();\n\
         \x20 print(\"joining\");\n\
         \x20 worker.join();\n\
         \x20 print(\"unreached\");\n\
         }\n",
    );
    let jit = match run_jit(&files) {
        Err(RunError::Trap(report)) => report,
        other => panic!("dev JIT did not trap: {other:?}"),
    };
    let ship = match run_c_aot(&files) {
        Err(RunError::Trap(report)) => report,
        other => panic!("ship C did not trap: {other:?}"),
    };
    // §152: both tiers report the Worker's throw site and the same message.
    for (tier, report) in [("dev JIT", &jit), ("ship C", &ship)] {
        assert_eq!(report.rule, TrapKind::WorkerTrapped, "{tier}");
        assert_eq!(report.stdout, b"joining\n", "{tier}");
        assert_eq!(report.pos.file, "exception.ts", "{tier}");
        assert_eq!((report.pos.line, report.pos.col), (8, 5), "{tier}");
        assert_eq!(
            report.message, "worker trapped with uncaught-exception: TypeError: job 7 failed",
            "{tier}"
        );
    }
}

/// compiler.md §115.5 rule 7: the exception edge of a `using` scope
/// runs its hooks and resumes the same exception. The uncaught report
/// cites the `throw`, not the scope. A hook that throws and catches
/// inside itself leaves the parked exception unchanged.
#[test]
fn a_using_exit_keeps_the_exception_and_its_position() {
    let report = agree_on_trap(
        "class Resource {\n\
         \x20 label: string;\n\
         \x20 constructor(label: string) { this.label = label; }\n\
         \x20 [Symbol.dispose](): void {\n\
         \x20   try { throw new TypeError(\"inside\"); } catch (e) { print(`hook caught ${this.label}`); }\n\
         \x20 }\n\
         }\n\
         function fail(): void {\n\
         \x20 throw new SyntaxError(\"deep\");\n\
         }\n\
         export function main(): void {\n\
         \x20 using outer = new Resource(\"outer\");\n\
         \x20 using inner = new Resource(\"inner\");\n\
         \x20 fail();\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "SyntaxError: deep");
    assert_eq!((report.pos.line, report.pos.col), (9, 3));
    assert_eq!(report.stdout, b"hook caught inner\nhook caught outer\n");
}

/// compiler.md §115.5 rule 1 in an `async` body: a scope that holds a
/// suspension runs its hook on the exception edge after the resume, and
/// the body boundary then traps (§115.4 item 2).
#[test]
fn a_using_exit_after_a_resume_runs_the_hook_before_the_boundary_trap() {
    let report = agree_on_trap(
        "class Resource {\n\
         \x20 [Symbol.dispose](): void { print(\"dispose\"); }\n\
         }\n\
         function fail(): void { throw new Error(\"after resume\"); }\n\
         async function work(): Promise<i32> {\n\
         \x20 using resource = new Resource();\n\
         \x20 await Context.suspend();\n\
         \x20 print(\"resumed\");\n\
         \x20 fail();\n\
         \x20 return 1;\n\
         }\n\
         export async function main(): Promise<void> {\n\
         \x20 print(`value ${await work()}`);\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "Error: after resume");
    assert_eq!((report.pos.line, report.pos.col), (4, 25));
    assert_eq!(report.stdout, b"resumed\ndispose\n");
}

/// The two resource classes of the `using` exit cases: `Q` prints its
/// label; `F` prints its label and raises an `Error` with the label.
const USING_RESOURCES: &str = "class Q {\n\
    \x20 label: string;\n\
    \x20 constructor(label: string) { this.label = label; }\n\
    \x20 [Symbol.dispose](): void { print(`q ${this.label}`); }\n\
    }\n\
    class F {\n\
    \x20 label: string;\n\
    \x20 constructor(label: string) { this.label = label; }\n\
    \x20 [Symbol.dispose](): void { print(`f ${this.label}`); throw new Error(this.label); }\n\
    }\n";

/// `raising()` inside a handler that prints the caught message.
const USING_MAIN: &str = "export function main(): void {\n\
    \x20 quiet();\n\
    \x20 try { raising(); } catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }\n\
    }\n";

fn using_case(functions: &str) -> String {
    agree_on_stdout(&format!("{USING_RESOURCES}{functions}{USING_MAIN}"))
}

/// compiler.md §115.5 rule 5, the fall-through end: the hooks run in
/// reverse declaration order. A hook that raises there runs outside the
/// `try` of the body (rule 6), and the remaining hook still runs (rule 3).
#[test]
fn a_using_end_runs_the_hooks_outside_the_body_handlers() {
    let stdout = using_case(
        "function quiet(): void {\n\
         \x20 using a = new Q(\"a\");\n\
         \x20 using b = new Q(\"b\");\n\
         \x20 print(\"body\");\n\
         }\n\
         function raising(): void {\n\
         \x20 using a = new Q(\"a\");\n\
         \x20 using b = new F(\"b\");\n\
         \x20 try { print(\"body\"); } catch { print(\"wrong\"); }\n\
         }\n",
    );
    assert_eq!(stdout, "body\nq b\nq a\nbody\nf b\nq a\ncaught b\n");
}

/// compiler.md §115.5 rule 5, `return`: the value evaluates before the
/// hooks, and a hook reads its own binding when a later declaration
/// shadows the name. A hook that raises on a `return` inside a `try`
/// reaches the caller's handler, not that `try`.
#[test]
fn a_using_return_evaluates_the_value_before_the_hooks() {
    let stdout = using_case(
        "function value(): i32 { print(\"value\"); return 7; }\n\
         function quiet(): void { print(`${quietValue()}`); }\n\
         function quietValue(): i32 {\n\
         \x20 using a = new Q(\"a\");\n\
         \x20 {\n\
         \x20   const a: i32 = 5;\n\
         \x20   print(`shadow ${a}`);\n\
         \x20   try { return value(); } catch { return 0; }\n\
         \x20 }\n\
         }\n\
         function raising(): void { print(`${raisingValue()}`); }\n\
         function raisingValue(): i32 {\n\
         \x20 using a = new F(\"a\");\n\
         \x20 using b = new Q(\"b\");\n\
         \x20 try { return value(); } catch { print(\"wrong\"); return 0; }\n\
         }\n",
    );
    assert_eq!(
        stdout,
        "shadow 5\nvalue\nq a\n7\nvalue\nq b\nf a\ncaught a\n"
    );
}

/// compiler.md §115.5 rule 5, `break`: the hooks of the iteration run
/// before the loop exit. A hook that raises on a `break` inside a `try`
/// leaves the function with the exception.
#[test]
fn a_using_break_runs_the_iteration_hooks() {
    let stdout = using_case(
        "function quiet(): void {\n\
         \x20 while (true) {\n\
         \x20   using a = new Q(\"a\");\n\
         \x20   try { break; } catch { print(\"wrong\"); }\n\
         \x20 }\n\
         \x20 print(\"after\");\n\
         }\n\
         function raising(): void {\n\
         \x20 for (let i: i32 = 0; i < 3; i++) {\n\
         \x20   using a = new F(`a${i}`);\n\
         \x20   try { break; } catch { print(\"wrong\"); }\n\
         \x20 }\n\
         \x20 print(\"unreached\");\n\
         }\n",
    );
    assert_eq!(stdout, "q a\nafter\nf a0\ncaught a0\n");
}

/// compiler.md §115.5 rule 5, `continue`: the hooks of each iteration run
/// before the next one. A hook that raises on a `continue` inside a `try`
/// leaves the loop with the exception.
#[test]
fn a_using_continue_runs_the_iteration_hooks() {
    let stdout = using_case(
        "function quiet(): void {\n\
         \x20 for (let i: i32 = 0; i < 2; i++) {\n\
         \x20   using a = new Q(`a${i}`);\n\
         \x20   try { continue; } catch { print(\"wrong\"); }\n\
         \x20 }\n\
         \x20 print(\"after\");\n\
         }\n\
         function raising(): void {\n\
         \x20 for (let i: i32 = 0; i < 2; i++) {\n\
         \x20   using a = new F(`a${i}`);\n\
         \x20   try { continue; } catch { print(\"wrong\"); }\n\
         \x20 }\n\
         \x20 print(\"unreached\");\n\
         }\n",
    );
    assert_eq!(stdout, "q a0\nq a1\nafter\nf a0\ncaught a0\n");
}

/// compiler.md §115.5 rules 2 and 7, the exception edge: a hook that
/// raises while the exception is parked traps with
/// `DisposeRaisedDuringExit` at its binding, after the later binding's
/// hook ran.
#[test]
fn a_hook_that_raises_on_the_exception_edge_traps() {
    let report = agree_on_trap(&format!(
        "{USING_RESOURCES}\
         function fail(): void {{ throw new Error(\"body\"); }}\n\
         export function main(): void {{\n\
         \x20 try {{\n\
         \x20   using a = new F(\"a\");\n\
         \x20   using b = new Q(\"b\");\n\
         \x20   fail();\n\
         \x20 }} catch {{ print(\"wrong\"); }}\n\
         }}\n"
    ));
    assert_eq!(report.rule, TrapKind::DisposeRaisedDuringExit);
    assert_eq!(report.pos.line, 14);
    assert_eq!(report.stdout, b"q b\nf a\n");
}

#[test]
fn an_exception_crosses_two_nested_array_for_each_callbacks() {
    let stdout = agree_on_stdout(
        "export function main(): void {\n\
         \x20 const outer: i32[] = [1, 2];\n\
         \x20 const inner: i32[] = [10, 20];\n\
         \x20 try {\n\
         \x20   outer.forEach((o: i32): void => {\n\
         \x20     print(`outer ${o}`);\n\
         \x20     inner.forEach((i: i32): void => {\n\
         \x20       print(`inner ${i}`);\n\
         \x20       throw new Error(\"stop\");\n\
         \x20     });\n\
         \x20   });\n\
         \x20 } catch (e) {\n\
         \x20   if (e instanceof Error) { print(`caught ${e.message}`); }\n\
         \x20 }\n\
         \x20 print(\"end\");\n\
         }\n",
    );
    assert_eq!(stdout, "outer 1\ninner 10\ncaught stop\nend\n");
}

#[test]
fn an_exception_crosses_nested_runtime_callback_loops() {
    // `Map.groupBy` runs its key callback from a runtime loop on every
    // engine. The exception leaves the inner loop and then the outer one,
    // inside a `Map.forEach` callback and two levels of `Array.forEach`.
    let stdout = agree_on_stdout(
        "export function main(): void {\n\
         \x20 const map: Map<i32, i32> = new Map<i32, i32>();\n\
         \x20 map.set(1, 10);\n\
         \x20 map.set(2, 20);\n\
         \x20 const outer: i32[] = [1, 2];\n\
         \x20 const inner: i32[] = [3, 4];\n\
         \x20 try {\n\
         \x20   map.forEach((value: i32, key: i32): void => {\n\
         \x20     print(`map ${key} ${value}`);\n\
         \x20     outer.forEach((o: i32): void => {\n\
         \x20       print(`outer ${o}`);\n\
         \x20       inner.forEach((i: i32): void => {\n\
         \x20         print(`inner ${i}`);\n\
         \x20         const groups: Map<i32, i32[]> = Map.groupBy(outer, (a: i32): i32 => {\n\
         \x20           print(`group ${a}`);\n\
         \x20           const nested: Map<i32, i32[]> = Map.groupBy(inner, (b: i32): i32 => {\n\
         \x20             print(`nested ${b}`);\n\
         \x20             throw new TypeError(`stop ${b}`);\n\
         \x20           });\n\
         \x20           return nested.size;\n\
         \x20         });\n\
         \x20         print(`size ${groups.size}`);\n\
         \x20       });\n\
         \x20     });\n\
         \x20   });\n\
         \x20 } catch (e) {\n\
         \x20   if (e instanceof TypeError) { print(`caught ${e.message}`); }\n\
         \x20 }\n\
         \x20 const after: Map<i32, i32[]> = Map.groupBy(inner, (b: i32): i32 => b % 2);\n\
         \x20 print(`after ${after.size}`);\n\
         \x20 print(\"end\");\n\
         }\n",
    );
    assert_eq!(
        stdout,
        "map 1 10\nouter 1\ninner 3\ngroup 1\nnested 3\ncaught stop 3\nafter 2\nend\n"
    );
}

/// compiler.md §116.1 rule 1: an exception that leaves an async body
/// completes its handle, also in the part of the body that runs at the
/// call. The call returns the handle, and the caller continues.
#[test]
fn an_exception_completes_the_handle_and_not_the_call() {
    let stdout = agree_on_stdout(
        "async function fails(tag: string): Promise<i32> {\n\
         \x20 print(`fails:${tag}`);\n\
         \x20 throw new Error(`failed ${tag}`);\n\
         }\n\
         async function later(): Promise<i32> {\n\
         \x20 await Context.suspend();\n\
         \x20 print(\"later:resumed\");\n\
         \x20 throw new TypeError(\"failed later\");\n\
         }\n\
         export async function main(): Promise<void> {\n\
         \x20 try {\n\
         \x20   const early: Promise<i32> = fails(\"at call\");\n\
         \x20   print(\"main:created\");\n\
         \x20   const value: i32 = await early;\n\
         \x20   print(`main:unreached ${value}`);\n\
         \x20 } catch (e) {\n\
         \x20   if (e instanceof Error) { print(`main:caught ${e.message}`); }\n\
         \x20 }\n\
         \x20 const late: Promise<i32> = later();\n\
         \x20 print(\"main:late created\");\n\
         \x20 try {\n\
         \x20   await late;\n\
         \x20 } catch (e) {\n\
         \x20   if (e instanceof TypeError) { print(`main:caught ${e.message}`); }\n\
         \x20 }\n\
         \x20 print(\"main:end\");\n\
         }\n",
    );
    assert_eq!(
        stdout,
        "fails:at call\nmain:created\nmain:caught failed at call\nmain:late created\n\
         later:resumed\nmain:caught failed later\nmain:end\n"
    );
}

/// compiler.md §116.1 rule 3: each `await` of one handle raises the same
/// object, from a direct await, from two holders, and from a repeated
/// await of a held handle.
#[test]
fn each_await_raises_the_same_object() {
    let stdout = agree_on_stdout(
        "let first: Error | null = null;\n\
         async function fails(): Promise<i32> {\n\
         \x20 await Context.suspend();\n\
         \x20 throw new SyntaxError(\"shared\");\n\
         }\n\
         async function observe(handle: Promise<i32>, tag: string): Promise<void> {\n\
         \x20 try {\n\
         \x20   await handle;\n\
         \x20 } catch (e) {\n\
         \x20   const seen: Error | null = first;\n\
         \x20   if (e instanceof SyntaxError) {\n\
         \x20     if (seen === null) {\n\
         \x20       first = e;\n\
         \x20       print(`${tag}:${e.message} first`);\n\
         \x20     } else {\n\
         \x20       print(`${tag}:${e.message} same=${e === seen}`);\n\
         \x20     }\n\
         \x20   }\n\
         \x20 }\n\
         }\n\
         export async function main(): Promise<void> {\n\
         \x20 const handle: Promise<i32> = fails();\n\
         \x20 const a: Promise<void> = observe(handle, \"a\");\n\
         \x20 const b: Promise<void> = observe(handle, \"b\");\n\
         \x20 await a;\n\
         \x20 await b;\n\
         \x20 await observe(handle, \"c\");\n\
         \x20 await observe(handle, \"d\");\n\
         }\n",
    );
    assert_eq!(
        stdout,
        "a:shared first\nb:shared same=true\nc:shared same=true\nd:shared same=true\n"
    );
}

/// compiler.md §116.1 rule 4: the last release of a handle that holds an
/// unobserved exception traps with the text and the position of the
/// `throw`, at a field overwrite and at the scheduler's release after a
/// body the program no longer holds. The control: a handle whose exception
/// an `await` raised releases with no trap.
#[test]
fn an_unobserved_exception_traps_when_its_count_reaches_zero() {
    let stdout = agree_on_stdout(
        "class Holder {\n\
         \x20 job: Promise<i32>;\n\
         \x20 constructor(job: Promise<i32>) { this.job = job; }\n\
         }\n\
         async function fails(): Promise<i32> { throw new Error(\"observed\"); }\n\
         export async function main(): Promise<void> {\n\
         \x20 const holder: Holder = new Holder(fails());\n\
         \x20 try {\n\
         \x20   await holder.job;\n\
         \x20 } catch (e) {\n\
         \x20   print(\"caught\");\n\
         \x20 }\n\
         \x20 Context.free(holder);\n\
         \x20 print(\"released after the await\");\n\
         }\n",
    );
    assert_eq!(stdout, "caught\nreleased after the await\n");

    let report = agree_on_trap(
        "class Holder {\n\
         \x20 job: Promise<i32>;\n\
         \x20 constructor(job: Promise<i32>) { this.job = job; }\n\
         }\n\
         async function fails(): Promise<i32> { throw new Error(\"overwritten\"); }\n\
         async function quiet(): Promise<i32> { return 2; }\n\
         export async function main(): Promise<void> {\n\
         \x20 const holder: Holder = new Holder(fails());\n\
         \x20 print(\"created\");\n\
         \x20 const next: Promise<i32> = quiet();\n\
         \x20 holder.job = next;\n\
         \x20 print(\"unreached\");\n\
         \x20 print(`${await next}`);\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "Error: overwritten");
    assert_eq!((report.pos.line, report.pos.col), (5, 40));
    assert_eq!(report.stdout, b"created\n");

    let report = agree_on_trap(
        "class Holder {\n\
         \x20 job: Promise<i32>;\n\
         \x20 constructor(job: Promise<i32>) { this.job = job; }\n\
         }\n\
         async function later(): Promise<i32> {\n\
         \x20 await Context.suspend();\n\
         \x20 print(\"later:resumed\");\n\
         \x20 throw new TypeError(\"nobody holds me\");\n\
         }\n\
         export async function main(): Promise<void> {\n\
         \x20 const holder: Holder = new Holder(later());\n\
         \x20 Context.free(holder);\n\
         \x20 print(\"main:freed\");\n\
         \x20 await Context.suspend();\n\
         \x20 print(\"main:unreached\");\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "TypeError: nobody holds me");
    assert_eq!((report.pos.line, report.pos.col), (8, 3));
    assert_eq!(report.stdout, b"main:freed\nlater:resumed\n");
}

/// compiler.md §116.1 rule 5: a host-kicked async export has no script
/// holder. An exception that leaves it traps at its completion, before its
/// first suspension and after one.
#[test]
fn an_async_export_with_no_holder_traps() {
    let report = agree_on_trap(
        "export async function main(): Promise<void> {\n\
         \x20 print(\"main:start\");\n\
         \x20 throw new Error(\"at the kick\");\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "Error: at the kick");
    assert_eq!((report.pos.line, report.pos.col), (3, 3));
    assert_eq!(report.stdout, b"main:start\n");

    let report = agree_on_trap(
        "export async function main(): Promise<void> {\n\
         \x20 await Context.suspend();\n\
         \x20 print(\"main:resumed\");\n\
         \x20 throw new TypeError(\"after the checkpoint\");\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "TypeError: after the checkpoint");
    assert_eq!((report.pos.line, report.pos.col), (4, 3));
    assert_eq!(report.stdout, b"main:resumed\n");
}

/// compiler.md §116.1 rule 6: the handle holds the Error object as a
/// collection root, and the release of the handle does not free it, so a
/// catch binding reads it after an explicit collection.
#[test]
fn the_error_object_outlives_its_handle() {
    let stdout = agree_on_stdout(
        "let kept: Error | null = null;\n\
         async function fails(tag: string): Promise<i32> {\n\
         \x20 throw new TypeError(`kept ${tag}`);\n\
         }\n\
         export async function main(): Promise<void> {\n\
         \x20 const rooted: Promise<i32> = fails(\"rooted\");\n\
         \x20 Context.collect();\n\
         \x20 try {\n\
         \x20   await rooted;\n\
         \x20 } catch (e) {\n\
         \x20   if (e instanceof TypeError) { print(e.message); }\n\
         \x20 }\n\
         \x20 {\n\
         \x20   const released: Promise<i32> = fails(\"released\");\n\
         \x20   try {\n\
         \x20     await released;\n\
         \x20   } catch (e) {\n\
         \x20     if (e instanceof TypeError) { kept = e; }\n\
         \x20   }\n\
         \x20 }\n\
         \x20 Context.collect();\n\
         \x20 const value: Error | null = kept;\n\
         \x20 if (value !== null) { print(`${value.name}: ${value.message}`); }\n\
         }\n",
    );
    assert_eq!(stdout, "kept rooted\nTypeError: kept released\n");
}

/// compiler.md §116.1 rule 7: a `try` block holds `await` and `yield`. A
/// `yield` inside a `try` block resumes into the handler region, and an
/// exception that leaves the generator body still traps there.
#[test]
fn a_try_block_holds_a_yield_and_a_generator_body_still_traps() {
    let stdout = agree_on_stdout(
        "function* steps(): Generator<i32> {\n\
         \x20 let seen: i32 = 0;\n\
         \x20 try {\n\
         \x20   yield 1;\n\
         \x20   seen += 1;\n\
         \x20   yield 2;\n\
         \x20   seen += 1;\n\
         \x20   throw new Error(\"after two resumes\");\n\
         \x20 } catch (e) {\n\
         \x20   if (e instanceof Error) { print(`caught ${e.message} seen=${seen}`); }\n\
         \x20 }\n\
         \x20 yield 3;\n\
         }\n\
         export function main(): void {\n\
         \x20 for (const n of steps()) { print(`n=${n}`); }\n\
         }\n",
    );
    assert_eq!(stdout, "n=1\nn=2\ncaught after two resumes seen=2\nn=3\n");

    let report = agree_on_trap(
        "function* steps(): Generator<i32> {\n\
         \x20 try {\n\
         \x20   yield 1;\n\
         \x20 } catch (e) {\n\
         \x20   print(\"inner\");\n\
         \x20 }\n\
         \x20 throw new Error(\"leaves the body\");\n\
         }\n\
         export function main(): void {\n\
         \x20 try {\n\
         \x20   for (const n of steps()) { print(`n=${n}`); }\n\
         \x20 } catch {\n\
         \x20   print(\"caught\");\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "Error: leaves the body");
    assert_eq!((report.pos.line, report.pos.col), (7, 3));
    assert_eq!(report.stdout, b"n=1\n");
}

/// compiler.md §116.1 rule 8: a Worker entry does not change. An exception
/// that leaves it is the Worker trap, and a `try` block that holds an
/// `await` in the parent does not catch it. The interpreter runs no Worker,
/// so the two tiers are compared.
#[test]
fn a_worker_exception_stays_the_worker_trap_under_an_await() {
    let files = sources(
        "class Job {\n\
         \x20 value: i32;\n\
         \x20 constructor(value: i32) { this.value = value; }\n\
         }\n\
         function work(inbox: Inbox<Job>, outbox: Outbox<Job>): void {\n\
         \x20 const job: Job | null = inbox.wait();\n\
         \x20 if (job !== null) {\n\
         \x20   throw new TypeError(`job ${job.value} failed`);\n\
         \x20 }\n\
         }\n\
         async function step(): Promise<i32> { return 1; }\n\
         export async function main(): Promise<void> {\n\
         \x20 const worker: Worker<Job, Job> = Worker.spawn(work);\n\
         \x20 worker.post(new Job(7));\n\
         \x20 worker.close();\n\
         \x20 try {\n\
         \x20   print(`step ${await step()}`);\n\
         \x20   worker.join();\n\
         \x20 } catch (e) {\n\
         \x20   print(\"caught\");\n\
         \x20 }\n\
         \x20 print(\"unreached\");\n\
         }\n",
    );
    for (tier, outcome) in [("dev JIT", run_jit(&files)), ("ship C", run_c_aot(&files))] {
        let report = match outcome {
            Err(RunError::Trap(report)) => report,
            other => panic!("{tier} did not trap: {other:?}"),
        };
        assert_eq!(report.rule, TrapKind::WorkerTrapped, "{tier}");
        assert_eq!(report.stdout, b"step 1\n", "{tier}");
        assert!(
            report.message.ends_with(": TypeError: job 7 failed"),
            "{tier}: {}",
            report.message
        );
    }
}

/// compiler.md §116.1 rule 4a: the registration outlives the freed script holder.
#[test]
fn a_waiting_await_holds_an_exception_handle() {
    let stdout = agree_on_stdout(
        r#"class Holder { job: Promise<i32>; constructor(job: Promise<i32>) { this.job = job; } }
async function later(): Promise<i32> { await Context.suspend(); print("later:resumed"); throw new Error("late"); }
async function waitOn(h: Holder): Promise<void> {
  try { print(`waited ${await h.job}`); } catch (e) { if (e instanceof Error) { print(`caught ${e.message}`); } }
}
export async function main(): Promise<void> {
  const holder: Holder = new Holder(later());
  const w: Promise<void> = waitOn(holder);
  Context.free(holder);
  print("freed holder");
  await w;
  print("end");
}
"#,
    );
    assert_eq!(stdout, "freed holder\nlater:resumed\ncaught late\nend\n");
}

/// compiler.md §116.1 rule 4a: the registration outlives the freed script holder.
#[test]
fn a_waiting_await_holds_a_value_handle() {
    let stdout = agree_on_stdout(
        r#"class Holder { job: Promise<i32>; constructor(job: Promise<i32>) { this.job = job; } }
async function later(): Promise<i32> { await Context.suspend(); print("later:resumed"); return 5; }
async function waitOn(h: Holder): Promise<void> {
  try { print(`waited ${await h.job}`); } catch (e) { print("caught"); }
}
export async function main(): Promise<void> {
  const holder: Holder = new Holder(later());
  const w: Promise<void> = waitOn(holder);
  Context.free(holder);
  print("freed holder");
  await w;
  print("end");
}
"#,
    );
    assert_eq!(stdout, "freed holder\nlater:resumed\nwaited 5\nend\n");
}

#[test]
fn an_exception_exit_releases_an_unobserved_handle() {
    let report = agree_on_trap(include_str!(
        "../../corpus/trap/t67-exception-exit-releases-its-handle.ts"
    ));
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "Error: dropped");
    assert_eq!((report.pos.line, report.pos.col), (8, 40));
    assert!(report.stdout.is_empty());
}

/// Each case uses one C compile and the runtime count probe on the dev host.
fn released_value_frames(source: &str, expected: &str) {
    assert_eq!(agree_on_stdout(source), expected);
    let mut session = subscript_codegen::ReloadSession::new(&sources(source)).expect("session");
    let before = session.live_allocations();
    session.call_main().expect("kick main");
    assert!(
        session.live_allocations() > before,
        "the pending root is the firing control"
    );
    while session.async_pending() != 0 {
        session.async_step().expect("checkpoint");
    }
    assert_eq!(
        session.live_allocations(),
        before,
        "the exit releases every value frame"
    );
    assert_eq!(session.take_output(), expected.as_bytes());
}

#[test]
fn a_thrown_exit_releases_a_value_handle() {
    released_value_frames(
        r#"const fault: Error = new Error("exit");
const caught: string = "caught";
const end: string = "end";
const alive: string = "outer alive";
async function value(): Promise<i32> { return 7; }
async function inner(skip: boolean): Promise<i32> {
  const h: Promise<i32> = value();
  if (skip) { throw fault; }
  return await h;
}
export async function main(): Promise<void> {
  try { await inner(true); } catch { print(caught); }
  print(end);
}
"#,
        "caught\nend\n",
    );
}

#[test]
fn a_catch_in_the_same_function_releases_only_the_left_scopes() {
    released_value_frames(
        r#"const fault: Error = new Error("exit");
const caught: string = "caught";
const end: string = "end";
const alive: string = "outer alive";
async function value(): Promise<i32> { return 7; }
export async function main(): Promise<void> {
  const outer: Promise<i32> = value();
  try {
    const inner: Promise<i32> = value();
    if (true) { throw fault; }
    await inner;
  } catch { print(caught); }
  const result: i32 = await outer;
  if (result === 7) { print(alive); }
}
"#,
        "caught\nouter alive\n",
    );
}

#[test]
fn a_hook_that_aborts_a_return_releases_the_returned_owner() {
    let report = agree_on_trap(
        r#"async function fails(): Promise<i32> { throw new Error("returned"); }
class R { [Symbol.dispose](): void { throw new TypeError("hook"); } }
function returning(): Promise<i32> {
  using r = new R();
  const h: Promise<i32> = fails();
  return h;
}
export async function main(): Promise<void> {
  try { const h: Promise<i32> = returning(); await h; } catch { print("caught"); }
}
"#,
    );
    assert_eq!(report.rule, TrapKind::UncaughtException);
    assert_eq!(report.message, "Error: returned");
    assert_eq!((report.pos.line, report.pos.col), (1, 40));
    assert!(report.stdout.is_empty());
}

/// compiler.md §116.1 rule 4c: only the local copy owns a count.
#[test]
fn a_lambda_borrows_a_handle_on_return() {
    let stdout = agree_on_stdout(
        r#"async function value(): Promise<i32> { return 7; }
export async function main(): Promise<void> {
  const h: Promise<i32> = value();
  const f = (): i32 => {
    const local: Promise<i32> = h;
    return 1;
  };
  print(`${f()}`);
  print(`${f()}`);
  print(`got ${await h}`);
}
"#,
    );
    assert_eq!(stdout, "1\n1\ngot 7\n");
}

/// compiler.md §116.1 rule 4c: only the local copy owns a count.
#[test]
fn a_lambda_borrows_a_handle_on_exception() {
    let stdout = agree_on_stdout(
        r#"async function fails(): Promise<i32> { throw new Error("dropped"); }
export async function main(): Promise<void> {
  const h: Promise<i32> = fails();
  const f = (skip: boolean): i32 => {
    const local: Promise<i32> = h;
    if (skip) { throw new TypeError("in lambda"); }
    return 1;
  };
  try { print(`${f(true)}`); } catch (e) { if (e instanceof Error) { print(`caught ${e.name}`); } }
  try { print(`unreached ${await h}`); } catch (e) { if (e instanceof Error) { print(`caught late ${e.message}`); } }
  print("end");
}
"#,
    );
    assert_eq!(stdout, "caught TypeError\ncaught late dropped\nend\n");
}

/// Each case costs one C compile and checks the runtime's live allocation count on the dev host.
#[test]
fn a_direct_await_of_a_failed_call_releases_its_frame() {
    released_value_frames(
        r#"const fault: Error = new Error("failure");
const caught: string = "caught";
async function fails(late: boolean): Promise<i32> {
    if (late) { await Context.suspend(); }
    throw fault;
}
export async function main(): Promise<void> {
    try { await fails(false); } catch { print(caught); }
    try { await fails(true); } catch { print(caught); }
}"#,
        "caught\ncaught\n",
    );
}
