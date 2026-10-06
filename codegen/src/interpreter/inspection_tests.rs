//! Task snapshots use resolved positions across all three witnesses (§169).
use super::*;
use crate::{
    emit_c, host_c_compiler, runtime_staticlib_path, runtime_system_libraries, ReloadSession,
};
use subscript_compiler::{check_program, SourceFile};

const PROGRAMS: &[&str] = &[
    "async function done(): Promise<i32> { return 7; }\nexport async function main(): Promise<void> { const h = done(); const x = await h; await Context.suspend(); print(`${x}`); }",
    "async function done(): Promise<i32> { await Context.suspend(); return 7; }\nexport async function main(): Promise<void> { const a = done(); const b = done(); const xs = await Promise.all([a,b]); print(`${xs[0]}`); }",
    "const ring: Promise<void>[] = [];\nasync function job(next: i32): Promise<void> { await Context.suspend(); await ring[next]; }\nexport async function main(): Promise<void> { ring.push(job(1)); ring.push(job(2)); ring.push(job(0)); await ring[0]; }",
    "const ring: Promise<void>[] = [];\nasync function job(next: i32): Promise<void> { await Context.suspend(); }\nexport async function main(): Promise<void> { ring.push(job(1)); ring.push(job(2)); ring.push(job(0)); await ring[0]; }",
    "export async function main(): Promise<void> { const f = async (): Promise<i32> => { await Context.suspend(); return 9; }; const x = await f(); print(`${x}`); }",
    "export async function main(): Promise<void> { const xs: i32[] = [7]; print(`${xs[1]}`); await Context.suspend(); }\nexport function reset(): void {}",
    "export async function main(): Promise<void> { const xs: i32[] = [7]; print(`${xs[0]}`); await Context.suspend(); }\nexport function reset(): void {}",
    "async function child(): Promise<void> { const xs: i32[] = [7]; print(`${xs[1]}`); await Context.suspend(); }\nexport async function main(): Promise<void> { await child(); }\nexport function reset(): void {}",
    "async function child(): Promise<void> { const xs: i32[] = [7]; print(`${xs[0]}`); await Context.suspend(); }\nexport async function main(): Promise<void> { await child(); }\nexport function reset(): void {}",
];
fn record(
    id: u64,
    awaited: u64,
    state: u32,
    kind: u32,
    function: &Pos,
    suspension: &Pos,
    creation: &Pos,
) -> String {
    format!(
        "{id} {awaited} {state} {kind} {} {} {} {} {} {} {} {} {}\n",
        function.file,
        function.line,
        function.col,
        suspension.file,
        suspension.line,
        suspension.col,
        creation.file,
        creation.line,
        creation.col
    )
}
fn jit_snapshot(jit: &ReloadSession) -> String {
    jit.async_tasks()
        .iter()
        .map(|t| {
            assert_eq!(t.reserved, 0);
            record(
                t.task_id,
                t.awaited_task_id,
                t.state,
                t.kind,
                &t.function_pos,
                &t.await_pos,
                &t.create_pos,
            )
        })
        .collect()
}
fn interpreter_snapshot(interpreter: &Interpreter<'_>) -> String {
    interpreter
        .async_tasks()
        .iter()
        .map(|t| {
            assert_eq!(t.reserved, 0);
            record(
                t.task_id,
                t.awaited_task_id,
                t.state,
                t.kind,
                &t.function_pos,
                &t.await_pos,
                &t.create_pos,
            )
        })
        .collect()
}
#[test]
fn async_task_snapshots_match_three_tiers() {
    let dir = std::env::temp_dir().join(format!("subscript-inspection-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let compiler = host_c_compiler().unwrap();
    let runtime = runtime_staticlib_path().unwrap();
    for source in PROGRAMS {
        let files = [SourceFile::new("tasks.ts", *source)];
        let hir = check_program(&files).unwrap();
        let module = crate::lir::lower_module(&hir).unwrap();
        let emitted = emit_c(&hir).unwrap();
        let mut jit = ReloadSession::new(&files).unwrap();
        let prefix = source.contains("const xs: i32[]");
        let trapping = prefix && source.contains("xs[1]");
        assert_eq!(jit.call_export("main").is_err(), trapping);
        let initial = jit.async_tasks();
        assert_eq!(initial[0].task_id, 1);
        let position = |needle: &str| {
            let offset = source.find(needle).unwrap();
            let prefix = &source[..offset];
            Pos::new(
                "tasks.ts",
                prefix.bytes().filter(|b| *b == b'\n').count() as u32 + 1,
                prefix.rsplit('\n').next().unwrap().len() as u32 + 1,
            )
        };
        assert_eq!(initial[0].function_pos, position("main"));
        assert_eq!(initial[0].create_pos, no_script_site());
        assert_eq!(initial[0].reserved, 0);
        if prefix {
            assert_eq!(
                initial[0].state,
                if trapping {
                    6
                } else if source.contains("await child()") {
                    3
                } else {
                    2
                }
            );
            if source.contains("await child()") {
                assert_eq!(initial[1].create_pos, position("await child()"));
                assert_eq!(initial[1].state, if trapping { 6 } else { 2 });
            }
        } else {
            if source.contains("const h = done();") {
                assert_eq!(initial[1].create_pos, position("done();"));
            }
            if source.contains("await f()") {
                assert_eq!(initial[1].create_pos, position("f()"));
            }
            let await_site = if source.contains("await ring[0]") {
                "await ring[0]"
            } else if source.contains("await Promise.all") {
                "await Promise.all"
            } else if source.contains("await f()") {
                "await f()"
            } else {
                "await h"
            };
            assert_eq!(initial[0].await_pos, position(await_site));
            assert_eq!(
                initial[0].state,
                if source.contains("return 7; }")
                    && !source.contains("await Context.suspend(); return 7")
                {
                    1
                } else {
                    3
                }
            );
        }
        let mut interpreter = Interpreter::new(&module).unwrap();
        if let Some(init) = module.initializer {
            interpreter.call_function(init, vec![]).unwrap();
        }
        let f = module
            .functions
            .iter()
            .find(|f| f.exported && f.source_name == "main")
            .unwrap();
        let Value::Coroutine(root) = interpreter.call_function(f.id, vec![]).unwrap() else {
            panic!("root")
        };
        assert_eq!(interpreter.async_kick(&root).is_err(), trapping);
        let mut snapshots = String::new();
        loop {
            let before = (
                jit.async_pending(),
                jit.async_unfinished(),
                jit.live_allocations(),
            );
            let snapshot = jit_snapshot(&jit);
            assert_eq!(
                before,
                (
                    jit.async_pending(),
                    jit.async_unfinished(),
                    jit.live_allocations()
                )
            );
            assert_eq!(snapshot, interpreter_snapshot(&interpreter));
            snapshots.push_str(&snapshot);
            snapshots.push_str("--\n");
            if jit.async_pending() == 0 {
                break;
            }
            jit.async_step_budget(1).unwrap();
            interpreter.async_step_budget(1).unwrap();
        }
        if source.contains("await ring[next]") {
            let ts = jit.async_tasks();
            assert_eq!((jit.async_pending(), jit.async_unfinished()), (0, 4));
            assert_eq!(
                ts.iter()
                    .map(|t| (t.task_id, t.awaited_task_id, t.state))
                    .collect::<Vec<_>>(),
                vec![(1, 2, 3), (2, 3, 3), (3, 4, 3), (4, 2, 3)]
            );
        } else if trapping {
            assert_eq!(
                jit.async_unfinished(),
                if source.contains("await child()") {
                    2
                } else {
                    1
                }
            );
            let stopped = jit_snapshot(&jit);
            jit.call_export("reset").unwrap();
            interpreter.clear_trap();
            jit.async_step_budget(1).unwrap();
            interpreter.async_step_budget(1).unwrap();
            assert_eq!(jit_snapshot(&jit), stopped);
            assert_eq!(interpreter_snapshot(&interpreter), stopped);
            snapshots.push_str(&stopped);
            snapshots.push_str("--\n");
        } else {
            assert_eq!(jit.async_unfinished(), 0);
        }
        std::fs::write(dir.join("program.c"), &emitted.source).unwrap();
        std::fs::write(dir.join("program.h"), &emitted.host_header).unwrap();
        std::fs::write(
            dir.join("program.alloc.h"),
            &emitted.allocation_metadata_header,
        )
        .unwrap();
        let host=crate::host_entry(r#"
#include <stdio.h>
#include <stddef.h>
#include <stdlib.h>
#include "program.alloc.h"
_Static_assert(sizeof(subscript_rt_async_task_info)==40,"task size");
_Static_assert(offsetof(subscript_rt_async_task_info,task_id)==0,"task_id offset");
_Static_assert(offsetof(subscript_rt_async_task_info,awaited_task_id)==8,"awaited_task_id offset");
_Static_assert(offsetof(subscript_rt_async_task_info,state)==16,"state offset");
_Static_assert(offsetof(subscript_rt_async_task_info,kind)==20,"kind offset");
_Static_assert(offsetof(subscript_rt_async_task_info,function_pos_id)==24,"function_pos_id offset");
_Static_assert(offsetof(subscript_rt_async_task_info,await_pos_id)==28,"await_pos_id offset");
_Static_assert(offsetof(subscript_rt_async_task_info,create_pos_id)==32,"create_pos_id offset");
_Static_assert(offsetof(subscript_rt_async_task_info,reserved)==36,"reserved offset");
static void visit(void* data, const subscript_rt_async_task_info* t) {
 (void)data;
 subscript_alloc_position_info f=subscript_alloc_positions[t->function_pos_id];
 subscript_alloc_position_info c=subscript_alloc_positions[t->create_pos_id];
 subscript_alloc_position_info a=subscript_alloc_positions[t->await_pos_id];
 fprintf(stderr,"%llu %llu %u %u %s %u %u %s %u %u %s %u %u\n",(unsigned long long)t->task_id,
 (unsigned long long)t->awaited_task_id,t->state,t->kind,f.file,f.line,f.column,a.file,a.line,a.column,c.file,c.line,c.column);
 if(t->reserved) abort();
}
int main(void) {
 subscript_rt_context* ctx=subscript_rt_ctx_new();
 subscript_rt_ctx_enter_script(ctx);subscript_init(ctx);subscript_export_main(ctx);subscript_rt_ctx_exit_script(ctx);
 for (;;) {
  subscript_rt_ctx_visit_async_tasks(ctx,visit,NULL);fprintf(stderr,"--\n");
  if (!subscript_rt_ctx_async_pending(ctx)) break;
  subscript_rt_ctx_async_step_budget(ctx,1);
 }
 if (subscript_rt_ctx_trap_kind(ctx)) {
  if (!subscript_rt_ctx_clear_trap(ctx)) abort();
  subscript_rt_ctx_async_step_budget(ctx,1);
  subscript_rt_ctx_visit_async_tasks(ctx,visit,NULL);fprintf(stderr,"--\n");
 }
 uint64_t n=0;const uint8_t* bytes=subscript_rt_ctx_stdout(ctx,&n);fwrite(bytes,1,n,stdout);
 subscript_rt_ctx_release(ctx);return 0;
}
"#,&emitted.host_header).unwrap();
        std::fs::write(dir.join("host.c"), host).unwrap();
        let exe = dir.join(format!("tasks{}", std::env::consts::EXE_SUFFIX));
        let mut command = compiler.command();
        crate::add_c11_optimized_flags(&mut command, compiler.style());
        crate::add_object_directory(&mut command, &dir, compiler.style());
        command
            .arg(dir.join("program.c"))
            .arg(dir.join("host.c"))
            .arg(&runtime)
            .args(runtime_system_libraries(compiler.style()));
        crate::add_executable_output(&mut command, &exe, compiler.style());
        let built = command.output().unwrap();
        assert!(
            built.status.success(),
            "{}",
            crate::tool_output_report(&built)
        );
        let ran = std::process::Command::new(&exe).output().unwrap();
        assert!(ran.status.success(), "{}", crate::tool_output_report(&ran));
        assert_eq!(String::from_utf8(ran.stderr).unwrap(), snapshots);
        let output = jit.take_output();
        assert_eq!(ran.stdout, output);
        assert_eq!(interpreter.context.take_stdout(), output);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
