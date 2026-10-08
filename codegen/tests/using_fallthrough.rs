//! Scope-exit hooks follow reachable statement exits (compiler.md §101,
//! §115.5 rule 5).
//!
//! The checker drops the statements after a statement that control cannot
//! leave; the lowering places the hooks. Each case counts the hook calls
//! in the lowered LIR of one function. No case runs a program.

use subscript_compiler::lir as l;
use subscript_compiler::{check_program, hir, SourceFile};

/// The checked body of `probe` and the number of hook calls in its LIR.
fn lowered(source: &str) -> (Vec<hir::Stmt>, usize) {
    let source = format!(
        "class R {{ [Symbol.dispose](): void {{}} }}
         function probe(flag: boolean, n: i32): void {{ {source} }}"
    );
    let module = check_program(&[SourceFile::new("using-fallthrough.ts", source)])
        .expect("accepted using scope");
    let body = module
        .functions
        .iter()
        .find(|function| function.name == "probe")
        .expect("probe")
        .body
        .clone();
    let lir = subscript_codegen::lir::lower_module(&module).expect("the program lowers");
    let hooks = lir
        .classes
        .iter()
        .flat_map(|class| &class.methods)
        .filter(|method| method.source_name == hir::DISPOSE_METHOD_NAME)
        .map(|method| method.id)
        .collect::<Vec<_>>();
    let probe = lir
        .functions
        .iter()
        .find(|function| function.source_name == "probe")
        .expect("lowered probe");
    let calls = probe
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter(|instruction| {
            matches!(&instruction.kind, l::InstructionKind::Call(target)
                if matches!(target.kind, l::CallTargetKind::Method(id) if hooks.contains(&id)))
        })
        .count();
    (body, calls)
}

/// The body of the one `using` node that follows the binding `r`.
fn node_body(body: &[hir::Stmt]) -> &[hir::Stmt] {
    let [hir::Stmt::Let { name, .. }, hir::Stmt::Using {
        bindings,
        body,
        finalizer: _,
        pos: _,
    }] = body
    else {
        panic!("a binding and its node: {body:#?}");
    };
    assert_eq!(name, "r");
    assert_eq!(bindings[0].name, "r");
    body
}

#[test]
fn infinite_loop_scopes_have_no_trailing_hook() {
    let cases = [
        ("for (;;) {}", 0),
        ("for (;;) { return; }", 1),
        ("for (let i: i32 = 0;; i += 1) { return; }", 1),
        ("{ for (;;) {} }", 0),
        ("if (true) { for (;;) {} }", 0),
        ("while (true) {}", 0),
        ("while (true) { return; }", 1),
        ("{ while (true) {} }", 0),
        ("if (true) { while (true) {} }", 0),
        ("for (;;) { switch (n) { default: break; } }", 0),
        ("while (true) { for (;;) { break; } }", 0),
    ];
    for ty in ["R", "R | null"] {
        for (shape, expected_hooks) in cases {
            for suffix in [
                "",
                "print(\"dead\");",
                "return;",
                "using late: R = new R(); return;",
            ] {
                let (body, hooks) = lowered(&format!("using r: {ty} = new R(); {shape} {suffix}"));
                assert_eq!(
                    node_body(&body).len(),
                    1,
                    "{ty}: {shape} {suffix}: the statements after the loop drop"
                );
                assert_eq!(hooks, expected_hooks, "{ty}: {shape} {suffix}");
            }
        }
    }
}

#[test]
fn leavable_loop_scopes_keep_the_trailing_hook() {
    for ty in ["R", "R | null"] {
        for shape in [
            "for (;;) { break; }",
            "while (true) { break; }",
            "for (;;) { if (flag) { break; } }",
            "while (flag) {}",
            "for (; flag;) {}",
            "if (flag) { for (;;) {} }",
            "if (false) { while (true) {} }",
            "for (;;) { switch (n) { default: break; } break; }",
        ] {
            let (body, hooks) = lowered(&format!("using r: {ty} = new R(); {shape}"));
            assert_eq!(node_body(&body).len(), 1, "{shape}");
            assert_eq!(hooks, 1, "{ty}: {shape}");
        }
    }
}

#[test]
fn switch_scope_disposes_only_at_reachable_arm_exits() {
    for loop_head in ["for (;;)", "while (true)"] {
        let (_, hooks) = lowered(&format!(
            "switch (n) {{ default: using r: R | null = new R(); {loop_head} {{}} }}"
        ));
        assert_eq!(hooks, 0, "{loop_head}");
        let (_, hooks) = lowered(&format!(
            "switch (n) {{ default: using r: R | null = new R(); {loop_head} {{ break; }} }}"
        ));
        assert_eq!(hooks, 1, "{loop_head}");
    }
    let (_, hooks) = lowered(
        "switch (n) { case 0: using r: R | null = new R();
         default: if (flag) { return; } break; }",
    );
    assert_eq!(hooks, 2);
}
