//! A copied mutable array retains elements but records no array holder count.
//! This test passes when synchronous removal leaves one orphan async count.
//! TaskGroup rules 2 and 15 exclude array holders from the group form.

use subscript_codegen::ReloadSession;
use subscript_compiler::SourceFile;

#[test]
fn synchronous_array_parameter_pop_keeps_an_orphan_count() {
    for parameter in [false, true] {
        let source = format!(
            r#"async function work(): Promise<void> {{ return; }}
function remove(jobs: Promise<void>[]): Promise<void> {{
    const job: Promise<void> = jobs[0];
    jobs.pop();
    return job;
}}
export async function main(): Promise<void> {{
    const jobs: Promise<void>[] = [work()];
    await jobs[0];
    {}
}}"#,
            if parameter {
                "const removed: Promise<void> = remove(jobs); await removed;"
            } else {
                "jobs.pop();"
            },
        );
        let files = [SourceFile::new("array-parameter-count.ts", source)];
        let mut jit = ReloadSession::new(&files).expect("checked program");
        jit.call_main().expect("root kick");
        while jit.async_pending() != 0 {
            jit.async_step().expect("checkpoint");
        }
        assert_eq!(jit.async_unfinished(), 0);
        let tasks = jit.async_tasks();
        println!(
            "array parameter={parameter}: retained tasks={}",
            tasks.len()
        );
        assert_eq!(tasks.len(), usize::from(parameter));
        if parameter {
            assert_eq!(tasks[0].state, 5);
            assert_eq!(tasks[0].kind, 1);
            assert_eq!(tasks[0].function_pos.line, 1);
        }
    }
}
