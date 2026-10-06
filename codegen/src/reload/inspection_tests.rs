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
