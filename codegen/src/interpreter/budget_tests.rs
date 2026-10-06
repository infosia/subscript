//! The three witnesses share checkpoint reports and output (§168).
use super::*;
use crate::{
    emit_c, host_c_compiler, runtime_staticlib_path, runtime_system_libraries, ReloadSession,
};
use subscript_compiler::{check_program, SourceFile};

const PROGRAMS: &[&str] = &[
    r#"
async function done(): Promise<i32> { return 7; }
export async function main(): Promise<void> {
    const h: Promise<i32> = done();
    for (let i: i32 = 0; i < 8; i++) { const x: i32 = await h; print(`${x}`); }
}
"#,
    r#"
async function done(): Promise<i32> { return 7; }
export async function main(): Promise<void> {
    const a: Promise<i32> = done();
    const b: Promise<i32> = done();
    const xs: i32[] = await Promise.all([a, b]);
    print(`${xs[0]}`);
    await Context.suspend();
    print("end");
}
export async function other(): Promise<void> {
    await Context.suspend(); print("other");
}
"#,
];

fn line(report: subscript_runtime::context::AsyncStepReport) -> String {
    format!(
        "{} {} {} {}\n",
        report.dispatched, report.pending, report.unfinished, report.budget_exhausted
    )
}

#[test]
fn checkpoint_reports_match_jit_ship_and_interpreter() {
    let dir = std::env::temp_dir().join(format!("subscript-budget-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let compiler = host_c_compiler().unwrap();
    let runtime = runtime_staticlib_path().unwrap();
    for &source in PROGRAMS {
        let files = [SourceFile::new("budget.ts", source)];
        let hir = check_program(&files).unwrap();
        let module = crate::lir::lower_module(&hir).unwrap();
        let emitted = emit_c(&hir).unwrap();
        std::fs::write(dir.join("program.c"), &emitted.source).unwrap();
        std::fs::write(dir.join("program.h"), &emitted.host_header).unwrap();
        for budget in [1, 3, u64::MAX] {
            let mut jit = ReloadSession::new(&files).unwrap();
            jit.call_export("main").unwrap();
            if source.contains("function other") {
                jit.call_export("other").unwrap();
            }
            let mut jit_reports = line(jit.async_step_budget(0).unwrap());
            while jit.async_pending() != 0 {
                jit_reports.push_str(&line(jit.async_step_budget(budget).unwrap()));
            }
            assert_eq!(jit.async_unfinished(), 0);
            let expected_reports = if source.contains("function other") {
                match budget {
                    1 => "1 2 2 1\n1 2 2 1\n1 1 1 1\n1 1 1 0\n1 0 0 0\n".to_string(),
                    3 => "3 1 1 1\n1 1 1 0\n1 0 0 0\n".to_string(),
                    _ => "4 1 1 0\n1 0 0 0\n".to_string(),
                }
            } else {
                match budget {
                    1 => "1 1 1 1\n".repeat(7) + "1 0 0 0\n",
                    3 => "3 1 1 1\n3 1 1 1\n2 0 0 0\n".to_string(),
                    _ => "8 0 0 0\n".to_string(),
                }
            };
            let zero = if source.contains("function other") {
                "0 3 2 1\n"
            } else {
                "0 1 1 1\n"
            };
            assert_eq!(jit_reports, format!("{zero}{expected_reports}"));
            let mut interpreter = Interpreter::new(&module).unwrap();
            if let Some(init) = module.initializer {
                interpreter.call_function(init, vec![]).unwrap();
            }
            for name in ["main", "other"] {
                if let Some(f) = module
                    .functions
                    .iter()
                    .find(|f| f.exported && f.source_name == name)
                {
                    let Value::Coroutine(frame) = interpreter.call_function(f.id, vec![]).unwrap()
                    else {
                        panic!("async root");
                    };
                    interpreter.async_kick(&frame).unwrap();
                }
            }
            let mut reports = line(interpreter.async_step_budget(0).unwrap());
            while interpreter.async_pending() != 0 {
                reports.push_str(&line(interpreter.async_step_budget(budget).unwrap()));
            }
            assert_eq!(reports, jit_reports);
            let output = jit.take_output();
            assert_eq!(
                output,
                if source.contains("function other") {
                    b"other\n7\nend\n".to_vec()
                } else {
                    b"7\n".repeat(8)
                }
            );
            assert_eq!(interpreter.context.take_stdout(), output);
            let host = crate::host_entry(
                r#"
#include <stdio.h>
#include <stddef.h>
_Static_assert(sizeof(subscript_rt_async_step_report) == 32, "report size");
_Static_assert(offsetof(subscript_rt_async_step_report, dispatched) == 0, "dispatched");
_Static_assert(offsetof(subscript_rt_async_step_report, pending) == 8, "pending");
_Static_assert(offsetof(subscript_rt_async_step_report, unfinished) == 16, "unfinished");
_Static_assert(offsetof(subscript_rt_async_step_report, budget_exhausted) == 24, "budget");
int main(void) {
 subscript_rt_context* ctx = subscript_rt_ctx_new();
 subscript_rt_ctx_enter_script(ctx);
 subscript_init(ctx); subscript_export_main(ctx); OTHER
 subscript_rt_ctx_exit_script(ctx);
 subscript_rt_async_step_report zero = subscript_rt_ctx_async_step_budget(ctx, 0);
 fprintf(stderr, "%llu %llu %llu %llu\n", (unsigned long long)zero.dispatched,
  (unsigned long long)zero.pending, (unsigned long long)zero.unfinished, (unsigned long long)zero.budget_exhausted);
 while (subscript_rt_ctx_async_pending(ctx)) {
  subscript_rt_ctx_collect(ctx);
  subscript_rt_async_step_report r = subscript_rt_ctx_async_step_budget(ctx, UINT64_C(BUDGET));
  fprintf(stderr, "%llu %llu %llu %llu\n", (unsigned long long)r.dispatched,
   (unsigned long long)r.pending, (unsigned long long)r.unfinished, (unsigned long long)r.budget_exhausted);
 }
 uint64_t n = 0; const uint8_t* bytes = subscript_rt_ctx_stdout(ctx, &n);
 fwrite(bytes, 1, n, stdout); subscript_rt_ctx_release(ctx); return 0;
}
"#, &emitted.host_header).unwrap()
            .replace("BUDGET", &budget.to_string())
            .replace("OTHER", if source.contains("function other") {
                "subscript_export_other(ctx);"
            } else { "" });
            std::fs::write(dir.join("host.c"), host).unwrap();
            let exe = dir.join(format!("budget{}", std::env::consts::EXE_SUFFIX));
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
            assert_eq!(ran.stdout, output);
            assert_eq!(String::from_utf8(ran.stderr).unwrap(), reports);
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn budgeted_traps_preserve_work_until_clearance() {
    let source = r#"
export async function main(): Promise<void> {
    await Context.suspend();
    print("once");
    const xs: i32[] = []; print(`${xs[1]}`);
}
export function clear(): void {}
export async function other(): Promise<void> {
    await Context.suspend(); print("peer");
}
"#;
    let files = [SourceFile::new("budget-trap.ts", source)];
    let hir = check_program(&files).unwrap();
    let module = crate::lir::lower_module(&hir).unwrap();
    let mut jit = ReloadSession::new(&files).unwrap();
    jit.call_export("main").unwrap();
    jit.call_export("other").unwrap();
    assert!(matches!(
        jit.async_step_budget(1),
        Err(crate::RunError::Trap(_))
    ));
    assert_eq!((jit.async_pending(), jit.async_unfinished()), (2, 2));
    assert_eq!(jit.take_output(), b"once\n");
    assert!(matches!(
        jit.async_step_budget(10),
        Err(crate::RunError::Trap(_))
    ));
    assert!(jit.take_output().is_empty());
    jit.call_export("clear").unwrap();
    let report = jit.async_step_budget(1).unwrap();
    assert_eq!(
        (
            report.dispatched,
            report.pending,
            report.unfinished,
            report.budget_exhausted
        ),
        (1, 0, 1, 0)
    );
    assert_eq!(jit.take_output(), b"peer\n");

    let mut interpreter = Interpreter::new(&module).unwrap();
    if let Some(init) = module.initializer {
        interpreter.call_function(init, vec![]).unwrap();
    }
    for name in ["main", "other"] {
        let f = module
            .functions
            .iter()
            .find(|f| f.exported && f.source_name == name)
            .unwrap();
        let Value::Coroutine(frame) = interpreter.call_function(f.id, vec![]).unwrap() else {
            panic!("async root");
        };
        interpreter.async_kick(&frame).unwrap();
    }
    assert!(interpreter.async_step_budget(1).is_err());
    assert_eq!(interpreter.async_pending(), 2);
    assert_eq!(interpreter.context.take_stdout(), b"once\n");
    assert!(interpreter.async_step_budget(10).is_err());
    assert!(interpreter.context.take_stdout().is_empty());
    interpreter.clear_trap();
    let report = interpreter.async_step_budget(1).unwrap();
    assert_eq!(
        (
            report.dispatched,
            report.pending,
            report.unfinished,
            report.budget_exhausted
        ),
        (1, 0, 1, 0)
    );
    assert_eq!(interpreter.context.take_stdout(), b"peer\n");
}
