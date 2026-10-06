//! Array aliases share element owners and release the last holder (§171).

use subscript_codegen::ReloadSession;
use subscript_compiler::SourceFile;

#[test]
fn synchronous_array_parameter_pop_leaves_no_task_owner() {
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
        assert!(tasks.is_empty());
    }
}
