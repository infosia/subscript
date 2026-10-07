//! The reference-holder corpus checks output and counts before Context destruction.
//! Cost: each count input creates one JIT session and one C build.

use super::*;
use crate::ReloadSession;
use subscript_compiler::{check_program, SourceFile};

pub(super) fn measured(name: &str, source: &str) -> (Vec<u8>, usize, usize, usize) {
    let files = [SourceFile::new(name, source)];
    let module = crate::lir::lower_module(&check_program(&files).expect("checked source"))
        .expect("verified LIR");
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
    let output = interpreter.context.take_stdout();
    let count = interpreter.async_tasks().len();
    let mut jit = ReloadSession::new(&files).expect("JIT session");
    jit.call_main().expect("start");
    while jit.async_pending() != 0 {
        jit.async_step().expect("checkpoint");
    }
    (
        output,
        count,
        jit.async_tasks().len(),
        ship_count(name, &files),
    )
}

#[test]
fn reference_holder_accept_and_its_zero_control_release_every_task() {
    let source = include_str!("../../../corpus/accept/a341-reference-holder-release.ts");
    let control = source
        .replace("  const h = work(11);\n", "")
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("m.set(")
                || line.trim_start().starts_with("m.delete(")
                || line.contains("const box = new Box")
            {
                String::new()
            } else {
                line.replace("m.getOr(1, fallback)", "fallback")
                    .replace("m.getOr(1, a)", "a")
                    .replace("await box.value", "await h")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (output, interpreted, native, ship) = measured("a341.ts", source);
    let (_, control_interpreted, control_native, control_ship) =
        measured("a341-control.ts", &control);
    println!("a341: interpreter={interpreted}, JIT={native}, C={ship}; control: interpreter={control_interpreted}, JIT={control_native}, C={control_ship}");
    assert_eq!(
        output,
        include_bytes!("../../../corpus/accept/a341-reference-holder-release.expected")
    );
    assert_eq!(
        (control_interpreted, control_native, control_ship),
        (0, 0, 0),
        "zero control"
    );
    assert_eq!(
        (interpreted, native, ship),
        (0, 0, 0),
        "released reference holders"
    );
}

#[test]
fn counted_map_operations_and_borrowed_reads_release_every_task() {
    let source = include_str!("../../../corpus/accept/a341-reference-holder-release.ts")
        .replace("  const box = new Box<Promise<i32>>(h);", "")
        .replace("await box.value", "await h");
    let control = source
        .replace("  const h = work(11);\n", "")
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("m.set(") || line.trim_start().starts_with("m.delete(")
            {
                String::new()
            } else {
                line.replace("m.getOr(1, fallback)", "fallback")
                    .replace("m.getOr(1, a)", "a")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    for (name, source) in [("map-operations", source), ("map-control", control)] {
        let (output, interpreted, native, ship) = measured(name, &source);
        assert_eq!((interpreted, native, ship), (0, 0, 0), "{name}");
        if name == "map-operations" {
            assert_eq!(
                output,
                include_bytes!("../../../corpus/accept/a341-reference-holder-release.expected")
            );
        }
    }
}

#[test]
fn copied_map_values_have_independent_counts() {
    for copied in [false, true] {
        let body = if copied {
            "const n = new Map(m); m.clear(); const value = n.getOr(1, [h]); await value[0]; Context.free(n);"
        } else {
            "m.clear(); const value = [h]; await value[0];"
        };
        let source = format!("async function work():Promise<i32>{{return 7;}} export async function main():Promise<void>{{const h=work();await h;const m=new Map<i32,Promise<i32>[]>();m.set(1,[h]);{body} Context.free(m);}}");
        let (_, interpreted, native, ship) = measured(&format!("map-copy-{copied}"), &source);
        assert_eq!((interpreted, native, ship), (0, 0, 0));
    }
}

#[test]
fn explicit_map_free_and_zero_control_release_every_task() {
    for stored in [false, true] {
        let body = if stored { "m.set(1, [h]);" } else { "" };
        let source = format!("async function work():Promise<void>{{return;}}export async function main():Promise<void>{{const h=work();await h;const m=new Map<i32,Promise<void>[]>();{body}Context.free(m);}}");
        let (_, interpreted, native, ship) = measured(&format!("map-free-{stored}"), &source);
        assert_eq!((interpreted, native, ship), (0, 0, 0));
    }
}

#[test]
fn reference_holder_traps_and_reject_match_the_contract() {
    let entries = [
        (
            "t94.ts",
            include_str!("../../../corpus/trap/t94-counted-map-delete.ts"),
        ),
        (
            "t95.ts",
            include_str!("../../../corpus/trap/t95-counted-map-clear.ts"),
        ),
        (
            "t96.ts",
            include_str!("../../../corpus/trap/t96-counted-map-replace.ts"),
        ),
        (
            "t97.ts",
            include_str!("../../../corpus/trap/t97-counted-field-collect.ts"),
        ),
        (
            "t98.ts",
            include_str!("../../../corpus/trap/t98-counted-field-task-order.ts"),
        ),
        (
            "r388.ts",
            include_str!("../../../corpus/reject/r388-counted-map-for-each.ts"),
        ),
    ];
    let mut failures = Vec::new();
    for (name, source) in entries {
        let files = [SourceFile::new(name, source)];
        let checked = check_program(&files);
        let result = checked.as_ref().ok().map(crate::lir::lower_module);
        let interpreted = result
            .as_ref()
            .and_then(|module| module.as_ref().ok())
            .map(interpret);
        println!(
            "{name}: checked={}, LIR={:?}, interpreter={interpreted:?}",
            checked.is_ok(),
            result.as_ref().map(|r| r.as_ref().err())
        );
        if name == "r388.ts" {
            if !checked.as_ref().is_err_and(|errors| {
                errors
                    .iter()
                    .any(|error| error.code == subscript_compiler::RuleCode::S014)
            }) {
                failures.push(name);
            }
        } else {
            let expected = match name {
                "t97.ts" => (19, 3, "Error: first"),
                "t98.ts" => (20, 3, "Error: first"),
                _ => (10, 40, "Error: lost"),
            };
            let tuple = interpreted
                .and_then(|result| result.err())
                .and_then(trap_tuple);
            if tuple != Some((expected.0, expected.1, expected.2.to_owned())) {
                failures.push(name);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "missing reference-holder traps or rejection: {failures:?}"
    );
}

pub(super) fn ship_count(name: &str, files: &[SourceFile]) -> usize {
    ship_measure(name, files, false).0
}

pub(super) fn ship_allocations(name: &str, files: &[SourceFile]) -> (usize, usize) {
    let (_, before, after) = ship_measure(name, files, true);
    (before, after)
}

fn ship_measure(name: &str, files: &[SourceFile], collect: bool) -> (usize, usize, usize) {
    let hir = check_program(files).expect("checked C source");
    let emitted = crate::emit_c(&hir).expect("C emission");
    let dir = std::env::temp_dir().join(format!("subscript-s172-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("fixture directory");
    std::fs::write(dir.join("program.c"), &emitted.source).expect("C source");
    std::fs::write(dir.join("program.h"), &emitted.host_header).expect("C header");
    std::fs::write(
        dir.join("program.alloc.h"),
        &emitted.allocation_metadata_header,
    )
    .expect("positions");
    let module = crate::lir::lower_module(&hir).expect("verified C source");
    let initialize = if module.initializer.is_some() {
        "subscript_init(ctx);"
    } else {
        ""
    };
    let host = crate::host_entry(r#"
#include <stdio.h>
static void visit(void* data, const subscript_rt_async_task_info* task) {
 (void)task; ++*(uint64_t*)data;
}
int main(void) {
 subscript_rt_context* ctx=subscript_rt_ctx_new();
 subscript_rt_ctx_enter_script(ctx); INITIALIZE; subscript_export_main(ctx); subscript_rt_ctx_exit_script(ctx);
 while (subscript_rt_ctx_async_pending(ctx) && !subscript_rt_ctx_trap_kind(ctx)) subscript_rt_ctx_async_step(ctx);
 uint64_t before=subscript_rt_ctx_live_allocations(ctx);
 COLLECT;
 uint64_t after=subscript_rt_ctx_live_allocations(ctx);
 uint64_t count=0; subscript_rt_ctx_visit_async_tasks(ctx,visit,&count);
 uint32_t trap=subscript_rt_ctx_trap_kind(ctx);
 printf("%llu %llu %llu\n",(unsigned long long)count,(unsigned long long)before,(unsigned long long)after);
 subscript_rt_ctx_release(ctx); return trap ? 255 : 0;
}
"#, &emitted.host_header).expect("C host").replace("INITIALIZE", initialize).replace("COLLECT", if collect { "subscript_export_collect(ctx)" } else { "" });
    std::fs::write(dir.join("host.c"), host).expect("host source");
    let compiler = crate::host_c_compiler().expect("C compiler");
    let mut command = compiler.command();
    crate::add_c11_optimized_flags(&mut command, compiler.style());
    crate::add_object_directory(&mut command, &dir, compiler.style());
    command
        .arg(dir.join("program.c"))
        .arg(dir.join("host.c"))
        .arg(crate::runtime_staticlib_path().expect("runtime archive"))
        .args(crate::runtime_system_libraries(compiler.style()));
    let exe = dir.join(format!("measurement{}", std::env::consts::EXE_SUFFIX));
    crate::add_executable_output(&mut command, &exe, compiler.style());
    let built = command.output().expect("C build");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let ran = std::process::Command::new(exe).output().expect("C run");
    let count = ran.status.code().expect("C exit code");
    assert!(
        (0..255).contains(&count),
        "C trap or invalid count: {count}"
    );
    std::fs::remove_dir_all(dir).expect("remove fixture");
    let counts = String::from_utf8(ran.stdout)
        .expect("C metric UTF-8")
        .split_whitespace()
        .map(|word| word.parse::<usize>().expect("C metric"))
        .collect::<Vec<_>>();
    assert_eq!(counts.len(), 3);
    (counts[0], counts[1], counts[2])
}

fn trap_tuple(error: InterpretError) -> Option<(u32, u32, String)> {
    match error {
        InterpretError::Execution { output, source } if output.is_empty() => trap_tuple(*source),
        InterpretError::Trap {
            runtime_kind: Some(RuntimeTrapKind::UncaughtException),
            pos,
            message,
            ..
        } => Some((pos.line, pos.col, message)),
        _ => None,
    }
}

#[test]
fn explicit_class_free_uses_resolved_fields_at_each_counted_depth() {
    for inhabited in [false, true] {
        let source = format!(
            r#"
class Box<T>{{tag:i32=7;v:T;constructor(v:T){{this.v=v;}}}}
@Descriptor class Descriptor{{v!:Promise<void>[];}}
async function work():Promise<void>{{return;}}
let transfer:Promise<void>[]=[];
function consume(h:Promise<void>):void{{transfer.push(h);}}
function* gen(a:Promise<void>[]):Generator<Promise<void>[]>{{consume(a[0]);transfer.pop();yield a;}}
function* empty():Generator<Promise<void>[]>{{yield [];}}
class ResultBox{{v=empty().next();}}
async function use():Promise<void>{{
  const h=work();await h;
  if({inhabited}){{
    const direct=new Box<Promise<void>>(h);Context.free(direct);
    const array=new Box<Promise<void>[]>([h]);Context.free(array);
    const nested=new Box<Promise<void>[][]>([[h]]);Context.free(nested);
    const fixed=new Box<FixedArray<Promise<void>,1>>([h]);Context.free(fixed);
    const it=gen([h]);const result=it.next();it.next();
    const yielded=new ResultBox();yielded.v=result;Context.free(yielded);
    const descriptor:Descriptor={{v:[h]}};Context.free(descriptor);
  }}
}}
export async function main():Promise<void>{{await use();}}
"#
        );
        let (_, interpreted, native, ship) = measured(&format!("class-free-{inhabited}"), &source);
        assert_eq!((interpreted, native, ship), (0, 0, 0));
    }
}
