use super::*;
use subscript_compiler::{Pos, SourceFile};

// Each test compiles two small sessions, with and without a body swap.
#[test]
fn kept_arrow_task_sites_resolve_after_reload() {
    let before = "const job = async (): Promise<void> => { await Context.suspend(); };\nexport async function main(): Promise<void> { await job(); }";
    for reload in [false, true] {
        let mut session = ReloadSession::new(&[SourceFile::new("kept.ts", before)]).unwrap();
        if reload {
            session
                .reload(&[SourceFile::new("kept.ts", format!("\n\n{before}"))])
                .unwrap();
        }
        session.call_main().unwrap();
        let tasks = session.async_tasks();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[1].function_pos, Pos::new("kept.ts", 1, 13));
        assert_eq!(tasks[1].await_pos, Pos::new("kept.ts", 1, 42));
    }
}

#[test]
fn kept_lambda_trap_site_resolves_after_reload() {
    let before = "const job = (): void => { const xs: i32[] = [7]; print(`${xs[1]}`); };\nexport function main(): void { job(); }";
    for reload in [false, true] {
        let mut session = ReloadSession::new(&[SourceFile::new("kept.ts", before)]).unwrap();
        if reload {
            session
                .reload(&[SourceFile::new("kept.ts", format!("\n\n{before}"))])
                .unwrap();
        }
        let Err(RunError::Trap(trap)) = session.call_main() else {
            panic!("expected bounds trap")
        };
        assert_eq!(trap.pos, Pos::new("kept.ts", 1, 59));
    }
}

#[test]
fn tasks_keep_their_positions_after_reload() {
    for reload in [false, true] {
        let before = "export async function main(): Promise<void> { await Context.suspend(); }\nexport async function other(): Promise<void> { await Context.suspend(); }";
        let after = "\n\nexport async function main(): Promise<void> { await Context.suspend(); }\n\nexport async function other(): Promise<void> { await Context.suspend(); }";
        let mut session = ReloadSession::new(&[SourceFile::new("tasks.ts", before)]).unwrap();
        session.call_export("main").unwrap();
        if reload {
            session
                .reload(&[SourceFile::new("tasks.ts", after)])
                .unwrap();
        }
        session.call_export("other").unwrap();
        let tasks = session.async_tasks();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].function_pos, Pos::new("tasks.ts", 1, 23));
        assert_eq!(tasks[0].await_pos, Pos::new("tasks.ts", 1, 47));
        assert_eq!(
            tasks[1].function_pos,
            Pos::new("tasks.ts", if reload { 5 } else { 2 }, 23)
        );
        assert_eq!(
            tasks[1].await_pos,
            Pos::new("tasks.ts", if reload { 5 } else { 2 }, 48)
        );
    }
}

// compiler.md §338 rule 2: debug best of three: 0.32 s; 1 C build.
#[test]
fn generated_host_call_reads_the_active_task_in_both_native_tiers() {
    use std::cell::{Cell, RefCell};
    use std::ffi::c_void;
    use subscript_runtime::context::AsyncTaskInfo;
    thread_local! {
        static CONTEXT: Cell<*const Context> = const { Cell::new(std::ptr::null()) };
        static RECORDS: RefCell<Vec<AsyncTaskInfo>> = const { RefCell::new(Vec::new()) };
    }
    unsafe extern "C" fn visit(_: *mut c_void, info: *const AsyncTaskInfo) {
        // SAFETY: the task visitor supplies a live record during this call.
        RECORDS.with(|records| records.borrow_mut().push(unsafe { *info }));
    }
    unsafe extern "C" fn inspect() {
        // SAFETY: the test installs this session's Context before it calls generated code.
        CONTEXT.with(|ctx| unsafe {
            (&*ctx.get()).visit_async_tasks(Some(visit), std::ptr::null_mut())
        });
    }
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let scratch =
        Scratch(std::env::temp_dir().join(format!("subscript-active-host-{}", std::process::id())));
    std::fs::create_dir_all(&scratch.0).unwrap();
    let files = [
        SourceFile::ambient("inspection.d.ts", "// @subscript-c-header include=\"inspection.h\"\ndeclare function inspectTasks(): void;"),
        SourceFile::entry("active.ts", "export async function main(): Promise<void> { inspectTasks(); await Context.suspend(); inspectTasks(); }"),
    ];
    // SAFETY: inspect has the mirror's C ABI and remains live for the session.
    let library = unsafe {
        NativeLibrary::new(
            Vec::new(),
            Vec::new(),
            vec![("inspectTasks".into(), inspect as *const u8)],
        )
    };
    let mut session = ReloadSession::new_with_native_libraries(&files, &[library]).unwrap();
    RECORDS.with(|records| records.borrow_mut().clear());
    CONTEXT.with(|ctx| ctx.set(&*session.ctx));
    session.call_main().unwrap();
    session.async_step().unwrap();
    CONTEXT.with(|ctx| ctx.set(std::ptr::null()));
    let records = RECORDS.with(|records| records.take());
    assert_eq!(records.len(), 2);
    let mut expected = String::new();
    for info in records {
        assert_eq!(
            (
                info.task_id,
                info.awaited_task_id,
                info.state,
                info.kind,
                info.reserved
            ),
            (1, 0, 4, 1, 0)
        );
        let f = session.positions.report_position(info.function_pos_id);
        let a = session.positions.report_position(info.await_pos_id);
        let c = session.positions.report_position(info.create_pos_id);
        assert_eq!(f, Pos::new("active.ts", 1, 23));
        assert_eq!(a, crate::position_table::no_script_site());
        assert_eq!(c, crate::position_table::no_script_site());
        expected.push_str(&format!(
            "{} {} {} {} {} {} {} {} {} {} {} {} {} {}\n",
            info.task_id,
            info.awaited_task_id,
            info.state,
            info.kind,
            info.reserved,
            f.file,
            f.line,
            f.col,
            a.file,
            a.line,
            a.col,
            c.file,
            c.line,
            c.col
        ));
    }
    let program = crate::emit_c(&subscript_compiler::check_program(&files).unwrap()).unwrap();
    for (name, text) in [
        ("program.c", program.source.as_str()),
        ("program.h", program.host_header.as_str()),
        (
            "program.alloc.h",
            program.allocation_metadata_header.as_str(),
        ),
        ("subscript_runtime.h", crate::HOST_HEADER_C),
    ] {
        std::fs::write(scratch.0.join(name), text).unwrap();
    }
    std::fs::write(scratch.0.join("inspection.h"), "void inspectTasks(void);\n").unwrap();
    let host = crate::host_entry(r#"
#include <stdio.h>
#include "program.alloc.h"
static subscript_rt_context* context;
static void visit(void* data, const subscript_rt_async_task_info* t) {
 (void)data;
 subscript_alloc_position_info f=subscript_alloc_positions[t->function_pos_id];
 subscript_alloc_position_info a=subscript_alloc_positions[t->await_pos_id];
 subscript_alloc_position_info c=subscript_alloc_positions[t->create_pos_id];
 printf("%llu %llu %u %u %u %s %u %u %s %u %u %s %u %u\n", (unsigned long long)t->task_id,
 (unsigned long long)t->awaited_task_id,t->state,t->kind,t->reserved,f.file,f.line,f.column,a.file,a.line,a.column,c.file,c.line,c.column);
}
void inspectTasks(void) { subscript_rt_ctx_visit_async_tasks(context, visit, NULL); }
int main(void) {
 context=subscript_rt_ctx_new();
 subscript_rt_ctx_enter_script(context);subscript_init(context);subscript_export_main(context);subscript_rt_ctx_exit_script(context);
 subscript_rt_ctx_async_step(context);
 int status=subscript_rt_ctx_trap_kind(context) != 0;
 subscript_rt_ctx_release(context);return status;
}
"#, program.host_header.as_str()).unwrap();
    std::fs::write(scratch.0.join("host.c"), host).unwrap();
    let compiler = crate::host_c_compiler().unwrap();
    let executable = scratch
        .0
        .join(format!("host{}", std::env::consts::EXE_SUFFIX));
    let mut command = compiler.command();
    crate::add_c11_optimized_flags(&mut command, compiler.style());
    crate::add_object_directory(&mut command, &scratch.0, compiler.style());
    command
        .arg(scratch.0.join("program.c"))
        .arg(scratch.0.join("host.c"))
        .arg(crate::runtime_staticlib_path().unwrap())
        .args(crate::runtime_system_libraries(compiler.style()));
    crate::add_executable_output(&mut command, &executable, compiler.style());
    let built = command.output().unwrap();
    assert!(
        built.status.success(),
        "{}",
        crate::tool_output_report(&built)
    );
    let output = std::process::Command::new(executable).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        crate::tool_output_report(&output)
    );
    assert_eq!(output.stdout, expected.as_bytes());
}
