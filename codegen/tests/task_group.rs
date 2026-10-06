//! Group semantics use three tiers. Each control has the same task and scope shape.

use subscript_codegen::{
    interpreter::interpret, lir::lower_module, run_c_aot, run_jit, ReloadSession,
};
use subscript_compiler::{check_program, SourceFile};

fn compare(source: &str, expected: &[u8]) {
    let files = [SourceFile::new("group.ts", source)];
    let module = check_program(&files).expect("checked group");
    let lir = lower_module(&module).expect("lowered group");
    assert_eq!(interpret(&lir).expect("interpreter"), expected);
    assert_eq!(run_jit(&files).expect("JIT"), expected);
    assert_eq!(run_c_aot(&files).expect("C"), expected);
}

#[test]
fn group_corpus_matches_three_tiers() {
    compare(
        include_str!("../../corpus/accept/a337-task-group.ts"),
        include_bytes!("../../corpus/accept/a337-task-group.expected"),
    );
}

#[test]
fn joined_scopes_release_all_tasks_after_success_and_failure() {
    for fail in [false, true] {
        let source = format!(
            r#"async function work(): Promise<void> {{
            await Context.suspend(); if ({fail}) {{ throw new Error("failed"); }}
        }}
        function add(group: TaskGroup): void {{ group.add(work()); }}
        export async function main(): Promise<void> {{
            const group: TaskGroup = new TaskGroup(); add(group);
            try {{ await group.join(); }} catch (e) {{}}
        }}"#
        );
        compare(&source, b"");
        let files = [SourceFile::new("counts.ts", source)];
        let mut jit = ReloadSession::new(&files).expect("checked group");
        jit.call_main().expect("kick");
        while jit.async_pending() != 0 {
            jit.async_step().expect("step");
        }
        assert!(jit.async_tasks().is_empty());
    }
}

#[test]
fn exception_exit_checks_the_group_with_a_joined_control() {
    for joined in [false, true] {
        let source = format!(
            r#"async function work(): Promise<void> {{ await Context.suspend(); }}
        export async function main(): Promise<void> {{
            try {{
                const group: TaskGroup = new TaskGroup(); group.add(work());
                if ({joined}) {{ await group.join(); }}
                throw new Error("exit");
            }} catch (e) {{ print("caught"); }}
        }}"#
        );
        let files = [SourceFile::new("exit.ts", &source)];
        let lir =
            lower_module(&check_program(&files).expect("checked exit")).expect("lowered exit");
        if joined {
            compare(&source, b"caught\n");
        } else {
            assert!(format!("{}", interpret(&lir).expect_err("scope trap")).contains("task-group"));
            assert!(format!("{}", run_jit(&files).expect_err("scope trap")).contains("task-group"));
            assert!(
                format!("{}", run_c_aot(&files).expect_err("scope trap")).contains("task-group")
            );
        }
    }
}

#[test]
fn dropped_join_reports_a_late_failure_with_an_observed_control() {
    for observed in [false, true] {
        let source = format!(
            r#"async function work(): Promise<void> {{
            await Context.suspend(); throw new Error("late");
        }}
        export async function main(): Promise<void> {{
            const group = new TaskGroup(); group.add(work());
            const joined = group.join();
            if ({observed}) {{ try {{ await joined; }} catch (e) {{ print("caught"); }} }}
        }}"#
        );
        if observed {
            compare(&source, b"caught\n");
        } else {
            let files = [SourceFile::new("late.ts", source)];
            let lir = lower_module(&check_program(&files).expect("checked late failure"))
                .expect("lowered late failure");
            assert!(
                format!("{}", interpret(&lir).expect_err("unobserved failure")).contains("late")
            );
            assert!(
                format!("{}", run_jit(&files).expect_err("unobserved failure")).contains("late")
            );
            assert!(
                format!("{}", run_c_aot(&files).expect_err("unobserved failure")).contains("late")
            );
        }
    }
}

#[test]
fn join_has_its_own_waiting_task_and_source_positions() {
    for grouped in [false, true] {
        let awaited = if grouped {
            "group.add(work()); await group.join();"
        } else {
            "await work();"
        };
        let source = format!(
            r#"async function work(): Promise<void> {{ await Context.suspend(); }}
export async function main(): Promise<void> {{
    const group = new TaskGroup();
    {awaited}
    if (!{grouped}) {{ await group.join(); }}
}}"#
        );
        let files = [SourceFile::new("inspection.ts", &source)];
        let mut jit = ReloadSession::new(&files).expect("checked inspection");
        jit.call_main().expect("kick");
        let tasks = jit.async_tasks();
        let main = tasks
            .iter()
            .find(|task| task.awaited_task_id != 0)
            .expect("waiter");
        let awaited = tasks
            .iter()
            .find(|task| task.task_id == main.awaited_task_id)
            .expect("awaited");
        assert_eq!(awaited.kind, if grouped { 3 } else { 1 });
        assert_eq!(awaited.state, if grouped { 3 } else { 2 });
        if grouped {
            assert_eq!(awaited.function_pos_id, awaited.create_pos_id);
            assert_ne!(awaited.create_pos_id, 0);
            let line = source.lines().nth(3).expect("join line");
            let column = line.find("group.join()").expect("join call") as u32 + 1;
            assert_eq!(
                (awaited.create_pos.line, awaited.create_pos.col),
                (4, column)
            );
        }
        while jit.async_pending() != 0 {
            jit.async_step().expect("step");
        }
        assert!(jit.async_tasks().is_empty());
    }
}
