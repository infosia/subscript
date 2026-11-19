//! Task and trap sites after a rejected reload (compiler.md §338 rule 4).
use subscript_codegen::{ReloadSession, RunError};
use subscript_compiler::{Pos, SourceFile};

// compiler.md §338 rule 4: debug best of three: 0.01 s; 0 C builds.
#[test]
fn rejected_reload_preserves_task_records_and_trap_positions() {
    let source = "export async function main(): Promise<void> { await Context.suspend(); }\nexport function fail(): void { const xs: i32[] = []; print(`${xs[0]}`); }";
    let files = [SourceFile::new("sites.ts", source)];
    let mut session = ReloadSession::new(&files).expect("session");
    session.call_export("main").expect("park task");
    let before = session.async_tasks();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].function_pos, Pos::new("sites.ts", 1, 23));
    assert_eq!(before[0].await_pos, Pos::new("sites.ts", 1, 47));
    let rejected = format!(
        "\n\n{}",
        source.replace("fail(): void", "fail(n: i32): void")
    );
    session
        .reload(&[SourceFile::new("sites.ts", rejected)])
        .expect_err("signature edit");
    assert_eq!(session.async_tasks(), before);
    let Err(RunError::Trap(trap)) = session.call_export("fail") else {
        panic!("expected bounds trap");
    };
    assert_eq!(trap.pos, Pos::new("sites.ts", 2, 63));
}
