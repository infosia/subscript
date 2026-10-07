//! Collection controls use one JIT session and one C build per program.

use super::reference_holder_tests::measured;
use super::*;
use subscript_compiler::{check_program, SourceFile};

#[test]
fn collection_releases_each_reference_holder_shape_and_its_empty_control() {
    for stored in [false, true] {
        let source = format!(
            r#"
class Box<T>{{v:T;constructor(v:T){{this.v=v;}}}}
class Direct{{v:Promise<void>;constructor(v:Promise<void>){{this.v=v;}}}}
class Outer{{v:Direct;constructor(v:Direct){{this.v=v;}}}}
@Descriptor class Required{{v!:Promise<void>;}}
@Descriptor class Default{{v?:Promise<void>[]=[];}}
async function work():Promise<void>{{return;}}
let transfer:Promise<void>[]=[];
function consume(h:Promise<void>):void{{transfer.push(h);}}
function* gen(a:Promise<void>[]):Generator<Promise<void>[]>{{consume(a[0]);transfer.pop();yield a;}}
function* empty():Generator<Promise<void>[]>{{yield [];}}
class ResultBox{{v=empty().next();}}
function generic<T>(v:T):void{{const box=new Box<T>(v);const map=new Map<i32,T>();map.set(1,v);}}
async function use():Promise<void>{{
 const h=work();await h;
 if({stored}){{
  generic(h);generic([h]);generic([[h]]);
  const fixed:FixedArray<Promise<void>,1>=[h];generic(fixed);
  const it=gen([h]);const result=it.next();it.next();generic(result);
  const yielded=new ResultBox();yielded.v=result;
  const direct=new Direct(h);
  const required:Required={{v:h}};const defaulted:Default={{v:[h]}};
  const array:Direct[]=[new Direct(h)];array.pop();
  const values=new Map<i32,Direct>();values.set(1,new Direct(h));values.clear();Context.free(values);
  const keys=new Map<Direct,i32>();keys.set(new Direct(h),1);keys.clear();Context.free(keys);
  const set=new Set<Direct>();set.add(new Direct(h));set.clear();Context.free(set);
  const outer=new Outer(new Direct(h));Context.free(outer);
 }}
}}
export async function main():Promise<void>{{await use();Context.collect();Context.collect();}}
"#
        );
        let (output, interpreter, jit, c) =
            measured(&format!("s172-collection-shapes-{stored}"), &source);
        assert!(output.is_empty());
        assert_eq!((interpreter, jit, c), (0, 0, 0));
    }
}

#[test]
fn collection_preserves_unfinished_work_after_its_last_reference_holder_ends() {
    for free in [false, true] {
        let release = if free {
            "Context.free(box);Context.free(map);"
        } else {
            ""
        };
        let source = format!(
            r#"
class Box{{v:Promise<void>;constructor(v:Promise<void>){{this.v=v;}}}}
async function work():Promise<void>{{await Context.suspend();await Context.suspend();print("finished");}}
async function use():Promise<void>{{const h=work();if(false){{await h;}}const box=new Box(h);const map=new Map<i32,Promise<void>>();map.set(1,h);{release}}}
export async function main():Promise<void>{{await use();Context.collect();}}
"#
        );
        let (output, interpreter, jit, c) = measured(&format!("s172-unfinished-{free}"), &source);
        assert_eq!(output, b"finished\n");
        assert_eq!((interpreter, jit, c), (0, 0, 0));
    }
}

#[test]
fn interpreter_script_allocations_use_frame_roots_and_match_native_live_counts() {
    for count in [0, 1_000, 10_000] {
        let source = format!("class Cell{{v:i32;constructor(v:i32){{this.v=v;}}}}export function main():void{{for(let i:i32=0;i<{count};i++){{const cell=new Cell(i);}}}}export function collect():void{{Context.collect();}}");
        let files = [subscript_compiler::SourceFile::new("s172-live.ts", &source)];
        let hir = subscript_compiler::check_program(&files).expect("checked live-count source");
        let module = crate::lir::lower_module(&hir).expect("verified LIR");
        let mut interpreter = Interpreter::new(&module).expect("interpreter");
        interpreter
            .call_function(module.entry.expect("entry"), Vec::new())
            .expect("allocation loop");
        assert_eq!(interpreter.context.live_count(), count);
        interpreter
            .collect_interpreter(&Pos::new("<host>", 1, 1))
            .expect("host collection");
        let mut jit = crate::ReloadSession::new(&files).expect("JIT");
        jit.call_main().expect("allocation loop");
        assert_eq!(jit.live_allocations(), count);
        jit.call_export("collect").expect("collection");
        assert_eq!(interpreter.context.live_count(), jit.live_allocations());
        assert_eq!(jit.live_allocations(), 0);
        assert_eq!(
            super::reference_holder_tests::ship_allocations(&format!("s172-live-{count}"), &files),
            (count, 0)
        );
    }
}

#[test]
fn interpreter_host_collection_reports_the_first_failure_through_context_trap_state() {
    for fail in [false, true] {
        let source = include_str!("../../../corpus/trap/t98-counted-field-task-order.ts")
            .replace("  Context.collect();", "")
            .replace("  print(\"after\");", "");
        let source = if fail {
            source
        } else {
            source.replace("throw new Error(message);", "return;")
        };
        let files = [subscript_compiler::SourceFile::new(
            "host-collection.ts",
            &source,
        )];
        let module = crate::lir::lower_module(
            &subscript_compiler::check_program(&files).expect("checked source"),
        )
        .expect("LIR");
        let mut interpreter = Interpreter::new(&module).expect("interpreter");
        if let Some(initializer) = module.initializer {
            interpreter
                .call_function(initializer, Vec::new())
                .expect("initializer");
        }
        let Value::Coroutine(root) = interpreter
            .call_function(module.entry.expect("entry"), Vec::new())
            .expect("root")
        else {
            panic!("async root");
        };
        interpreter.async_kick(&root).expect("start");
        while interpreter.async_pending() != 0 {
            interpreter.async_step().expect("checkpoint");
        }
        assert_eq!(interpreter.async_tasks().len(), 2);
        let result = interpreter.collect_interpreter(&Pos::new("<host>", 1, 1));
        assert_eq!(result.is_err(), fail);
        assert_eq!(interpreter.async_tasks().len(), 0);
        assert_eq!(interpreter.context.trapped(), fail);
        if fail {
            let trap = interpreter.context.trap_record().expect("host trap state");
            assert_eq!(trap.kind, RuntimeTrapKind::UncaughtException);
            assert_eq!(trap.message, "Error: first");
        }
        interpreter.clear_trap();
        interpreter
            .collect_interpreter(&Pos::new("<host>", 1, 1))
            .expect("no second release");
    }
}

#[test]
fn interpreter_map_free_retires_the_header_after_a_failed_release_and_its_control() {
    for fail in [false, true] {
        let body = if fail {
            "throw new Error(\"lost\");"
        } else {
            "return;"
        };
        let source = format!("const map=new Map<i32,Promise<void>>();async function work():Promise<void>{{{body}}}async function keep():Promise<void>{{const h=work();if(false){{await h;}}map.set(1,h);}}export async function main():Promise<void>{{await keep();Context.free(map);}}");
        let files = [subscript_compiler::SourceFile::new(
            "map-free-failure.ts",
            &source,
        )];
        let module = crate::lir::lower_module(
            &subscript_compiler::check_program(&files).expect("checked source"),
        )
        .expect("LIR");
        let mut interpreter = Interpreter::new(&module).expect("interpreter");
        let result = interpreter.run();
        assert_eq!(result.is_err(), fail);
        let Value::Handle(map) = *interpreter.globals[0].borrow() else {
            panic!("Map global");
        };
        assert!(!interpreter.context.is_live(map as usize));
        interpreter.clear_trap();
        interpreter
            .collect_interpreter(&Pos::new("<host>", 1, 1))
            .expect("no second Map release");
    }
}

#[test]
fn sort_scratch_roots_survive_receiver_replacement_and_a_collection_control() {
    for replace in [false, true] {
        let source = format!(
            r#"
class Cell{{v:i32;constructor(v:i32){{this.v=v;}}}}
export async function main():Promise<void>{{
 const cells:Cell[]=[new Cell(3),new Cell(1),new Cell(2)];
 cells.sort((a:Cell,b:Cell):i32=>{{
  if({replace}){{cells[0]=new Cell(9);cells[1]=new Cell(9);cells[2]=new Cell(9);}}
  Context.collect();return a.v-b.v;
 }});
 print(`${{cells[0].v}},${{cells[1].v}},${{cells[2].v}}`);
}}
"#
        );
        let files = [SourceFile::new(
            format!("s172-sort-roots-{replace}"),
            source,
        )];
        let module = crate::lir::lower_module(&check_program(&files).expect("checked source"))
            .expect("verified LIR");
        let output = crate::interpreter::interpret(&module).expect("rooted sort scratch");
        assert_eq!(output, b"1,2,3\n");
    }
}

#[test]
fn generator_roots_follow_reachable_iterators_in_all_three_tiers() {
    for keep in [false, true] {
        let global = if keep { "const held=g();" } else { "" };
        let iterator = if keep { "held" } else { "g()" };
        let source = format!("function* g():Generator<i32>{{const xs=[1,2,3];for(const x of xs)yield x;}}{global}export function main():void{{const it={iterator};it.next();}}export function collect():void{{Context.collect();}}");
        let files = [SourceFile::new("generator-roots.ts", source)];
        let module =
            crate::lir::lower_module(&check_program(&files).expect("checked source")).expect("LIR");
        let mut interpreter = Interpreter::new(&module).expect("interpreter");
        if let Some(initializer) = module.initializer {
            interpreter
                .call_function(initializer, Vec::new())
                .expect("initializer");
        }
        interpreter
            .call_function(module.entry.expect("entry"), Vec::new())
            .expect("main");
        interpreter
            .collect_interpreter(&Pos::new("<host>", 1, 1))
            .expect("collection");
        let mut jit = crate::ReloadSession::new(&files).expect("JIT");
        jit.call_main().expect("main");
        jit.call_export("collect").expect("collection");
        let (_, ship) = super::reference_holder_tests::ship_allocations(
            &format!("s172-generator-{keep}"),
            &files,
        );
        let counts = (
            interpreter.context.live_count(),
            jit.live_allocations(),
            ship,
        );
        eprintln!("generator keep={keep}: {counts:?}");
        if keep {
            // Native tiers also allocate the generator frame in the Context.
            assert_eq!(counts, (2, 3, 3));
            assert_eq!(interpreter.generator_handles.borrow().len(), 1);
        } else {
            assert_eq!(counts, (0, 0, 0));
            assert!(interpreter.generator_handles.borrow().is_empty());
        }
    }
}

#[test]
fn reachable_generator_through_heap_storage_resumes_after_collection() {
    for subject in ["held", "box.v", "array[0]"] {
        for keep in [false, true] {
            let replacement = if keep {
                String::new()
            } else {
                format!("{subject}=g();")
            };
            let output = if keep {
                format!("print(`${{{subject}.next().value}}`);")
            } else {
                "print(\"removed\");".to_owned()
            };
            let source = format!("function* g():Generator<i32>{{const xs=[1,2,3];for(const x of xs)yield x;}}class Box{{v:Generator<i32>;constructor(v:Generator<i32>){{this.v=v;}}}}let held=g();const box=new Box(g());const array:Generator<i32>[]=[g()];export function main():void{{{subject}.next();{replacement}Context.collect();{output}}}");
            let files = [SourceFile::new("generator-heap-roots.ts", source)];
            let module = crate::lir::lower_module(&check_program(&files).expect("checked source"))
                .expect("LIR");
            let mut interpreter = Interpreter::new(&module).expect("interpreter");
            interpreter
                .call_function(module.initializer.expect("initializer"), Vec::new())
                .expect("initialize");
            let original = interpreter
                .generator_handles
                .borrow()
                .keys()
                .copied()
                .collect::<std::collections::HashSet<_>>();
            interpreter
                .call_function(module.entry.expect("entry"), Vec::new())
                .expect("main");
            assert_eq!(
                interpreter.context.take_stdout(),
                if keep {
                    b"2\n".as_slice()
                } else {
                    b"removed\n".as_slice()
                }
            );
            let generators = interpreter.generator_handles.borrow();
            assert_eq!(generators.len(), 3);
            assert_eq!(
                original
                    .iter()
                    .filter(|key| !generators.contains_key(key))
                    .count(),
                usize::from(!keep)
            );
        }
    }
}

#[test]
fn drop_removes_an_unstarted_generator_before_collection() {
    let files = [SourceFile::new(
        "unstarted-generator.ts",
        "function* g():Generator<i32>{yield 1;}export function main():void{const it=g();}",
    )];
    let module =
        crate::lir::lower_module(&check_program(&files).expect("checked source")).expect("LIR");
    let mut interpreter = Interpreter::new(&module).expect("interpreter");
    interpreter
        .call_function(module.entry.expect("entry"), Vec::new())
        .expect("main");
    assert_eq!(interpreter.context.live_count(), 0);
    assert_eq!(interpreter.generator_handles.borrow().len(), 0);
    interpreter
        .collect_interpreter(&Pos::new("<host>", 1, 1))
        .expect("collection");
    assert!(interpreter.generator_handles.borrow().is_empty());
}

#[test]
fn array_callback_output_and_caller_frame_survive_collection_and_its_control() {
    for collect in [false, true] {
        let source = format!(
            r#"
class Cell{{v:i32;constructor(v:i32){{this.v=v;}}}}
function build():Cell[]{{
 const cells:Cell[]=[new Cell(1),new Cell(2),new Cell(3)];
 return cells.map((cell:Cell):Cell=>{{
  if({collect}){{Context.collect();}}
  return new Cell(cell.v);
 }});
}}
export function main():void{{
 const caller=new Cell(4);
 const result=build();
 print(`${{result[0].v}},${{result[1].v}},${{result[2].v}},${{caller.v}}`);
}}
"#
        );
        let files = [SourceFile::new("array-callback-output.ts", source)];
        let module = crate::lir::lower_module(&check_program(&files).expect("checked source"))
            .expect("verified LIR");
        assert_eq!(
            interpret(&module).expect("rooted callback output"),
            b"1,2,3,4\n"
        );
    }
}

#[test]
fn unfinished_loop_call_result_does_not_root_the_previous_iteration() {
    let source = include_str!("../../../corpus/trap/t99-loop-result-collect.ts");
    for collect in [false, true] {
        let source = if collect {
            source.to_owned()
        } else {
            source.replace("  Context.collect();", "")
        };
        let files = [SourceFile::new("loop-result.ts", source)];
        let module =
            crate::lir::lower_module(&check_program(&files).expect("checked source")).expect("LIR");
        let result = interpret(&module);
        if collect {
            let InterpretError::Execution { output, source } =
                result.expect_err("second collect releases the previous result")
            else {
                panic!("output and trap");
            };
            assert_eq!(output, b"step 0\n");
            assert!(
                matches!(*source, InterpretError::Trap { runtime_kind: Some(RuntimeTrapKind::UncaughtException), ref message, ref pos, .. } if message == "Error: lost" && pos.line == 13 && pos.col == 3)
            );
        } else {
            assert_eq!(result.expect("no collection control"), b"step 0\nstep 1\n");
        }
    }
}

#[test]
fn unfinished_iterator_result_does_not_root_the_previous_iteration() {
    for collect in [false, true] {
        let source = format!(
            r#"
class Holder{{task:Promise<void>;constructor(t:Promise<void>){{this.task=t;}}}}
async function fail():Promise<void>{{throw new Error("lost");}}
function* g():Generator<Holder>{{for(let i:i32=0;i<2;i++){{if({collect}){{Context.collect();}}print(`step ${{i}}`);yield new Holder(fail());}}}}
export function main():void{{const it=g();for(let i:i32=0;i<2;i++){{it.next();}}}}
"#
        );
        let files = [SourceFile::new("iterator-result.ts", source)];
        let module =
            crate::lir::lower_module(&check_program(&files).expect("checked source")).expect("LIR");
        let result = interpret(&module);
        if collect {
            let InterpretError::Execution { output, source } =
                result.expect_err("second collect releases the previous yield")
            else {
                panic!("output and trap");
            };
            assert_eq!(output, b"step 0\n");
            assert!(
                matches!(*source, InterpretError::Trap { runtime_kind: Some(RuntimeTrapKind::UncaughtException), ref message, .. } if message == "Error: lost")
            );
        } else {
            assert_eq!(result.expect("no collection control"), b"step 0\nstep 1\n");
        }
    }
}

#[test]
fn conditional_await_roots_the_current_holder_and_its_kept_control() {
    let source = include_str!("../../../corpus/trap/t100-conditional-await-collect.ts");
    for keep in [false, true] {
        let source = if keep {
            source
                .replace(
                    "async function run():",
                    "let kept: Holder | null = null;\nasync function run():",
                )
                .replace(
                    "const h = new Holder(fail());",
                    "const h = new Holder(fail()); if (i == 0) { kept = h; }",
                )
                .replace(
                    "await run();",
                    "await run(); if (kept != null) { const t = kept.task; print(\"kept\"); }",
                )
        } else {
            source.to_owned()
        };
        let files = [SourceFile::new("conditional-await.ts", source)];
        let module =
            crate::lir::lower_module(&check_program(&files).expect("checked input")).expect("LIR");
        let result = interpret(&module);
        if keep {
            assert_eq!(
                result.expect("live previous holder"),
                b"step 0\nstep 1\nkept\n"
            );
        } else {
            let error = result.expect_err("dead suspend parameter");
            assert_eq!(error.output(), b"step 0\n");
            let InterpretError::Execution { source, .. } = error else {
                panic!("output and trap");
            };
            assert!(matches!(*source, InterpretError::Trap {
                runtime_kind: Some(RuntimeTrapKind::UncaughtException), ref message, ref pos, ..
            } if message == "Error: lost" && pos.line == 17 && pos.col == 5));
        }
    }
}

#[test]
fn unfinished_loop_call_collect_preserves_a_live_previous_result() {
    let source = include_str!("../../../corpus/trap/t99-loop-result-collect.ts")
        .replace("for (let i: i32 = 0; i < 2; i++) { step(i); }",
            "const kept = step(0); for (let i: i32 = 1; i < 2; i++) { step(i); } const t = kept.task; print(\"kept\");");
    let files = [SourceFile::new("kept-loop-result.ts", source)];
    let module =
        crate::lir::lower_module(&check_program(&files).expect("checked input")).expect("LIR");
    assert_eq!(
        interpret(&module).expect("live previous result"),
        b"step 0\nstep 1\nkept\n"
    );
}
