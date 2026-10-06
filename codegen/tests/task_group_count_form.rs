//! TaskGroup rules 2 and 15 exclude completions that omit counted-result release.
//! The tests pass with the existing handle-array defects that the group count path exposes.
//! The synchronous control releases every task; the async result leaves one task live.

use subscript_codegen::ReloadSession;
use subscript_compiler::SourceFile;

#[test]
fn async_return_keeps_a_count_after_the_last_script_holder_exits() {
    for asynchronous in [false, true] {
        let source = format!(
            r#"async function work(): Promise<void> {{ return; }}
{}function make(): {} {{ return [work()]; }}
async function use(): Promise<void> {{
    const jobs: Promise<void>[] = {}make();
    await jobs[0];
}}
export async function main(): Promise<void> {{ await use(); }}"#,
            if asynchronous { "async " } else { "" },
            if asynchronous {
                "Promise<Promise<void>[]>"
            } else {
                "Promise<void>[]"
            },
            if asynchronous { "await " } else { "" },
        );
        let files = [SourceFile::new("count-form.ts", source)];
        let mut jit = ReloadSession::new(&files).expect("checked program");
        jit.call_main().expect("root kick");
        while jit.async_pending() != 0 {
            jit.async_step().expect("checkpoint");
        }
        assert_eq!(jit.async_unfinished(), 0);
        let tasks = jit.async_tasks();
        println!(
            "async return={asynchronous}: retained tasks={}",
            tasks.len()
        );
        assert_eq!(tasks.len(), usize::from(asynchronous));
        if asynchronous {
            assert_eq!(tasks[0].state, 5);
            assert_eq!(tasks[0].kind, 1);
            assert_eq!(tasks[0].function_pos.line, 1);
        }
    }
}
