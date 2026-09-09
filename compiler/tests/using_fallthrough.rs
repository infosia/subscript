//! Scope-exit disposal follows reachable statement exits (compiler.md §101).

use subscript_compiler::{check_program, hir, SourceFile};

fn check(source: &str) -> Vec<hir::Stmt> {
    let source = format!(
        "class R {{ [Symbol.dispose](): void {{}} }}
         function probe(flag: boolean, n: i32): void {{ {source} }}"
    );
    let module = check_program(&[SourceFile::new("using-fallthrough.ts", source)])
        .expect("accepted using scope");
    module
        .functions
        .iter()
        .find(|function| function.name == "probe")
        .expect("probe")
        .body
        .clone()
}

fn disposal_count(statements: &[hir::Stmt]) -> usize {
    fn expr_count(expr: &hir::Expr) -> usize {
        let own = usize::from(matches!(
            &expr.kind,
            hir::ExprKind::Call { callee: hir::Callee::Method { name, .. }, .. }
                if name == hir::DISPOSE_METHOD_NAME
        ));
        own + expr.children().into_iter().map(child_count).sum::<usize>()
    }
    fn child_count(child: hir::HirChild<'_>) -> usize {
        match child {
            hir::HirChild::Expr(expr) => expr_count(expr),
            hir::HirChild::Stmt(statement) => {
                statement.children().into_iter().map(child_count).sum()
            }
        }
    }
    statements
        .iter()
        .map(|statement| child_count(hir::HirChild::Stmt(statement)))
        .sum()
}

#[test]
fn infinite_loop_scopes_have_no_trailing_disposal() {
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
        for (shape, expected_disposals) in cases {
            for suffix in [
                "",
                "print(\"dead\");",
                "return;",
                "using late: R = new R(); return;",
            ] {
                let body = check(&format!("using r: {ty} = new R(); {shape} {suffix}"));
                assert_eq!(body.len(), 2, "{ty}: {shape} {suffix}: {body:#?}");
                assert_eq!(
                    disposal_count(&body),
                    expected_disposals,
                    "{shape} {suffix}"
                );
            }
        }
    }
}

#[test]
fn leavable_loop_scopes_keep_the_trailing_disposal() {
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
            let body = check(&format!("using r: {ty} = new R(); {shape}"));
            assert_eq!(body.len(), 3, "{shape}: {body:#?}");
            assert_eq!(disposal_count(&body[2..]), 1, "{shape}");
        }
    }
}

#[test]
fn switch_scope_disposes_only_at_reachable_arm_exits() {
    for loop_head in ["for (;;)", "while (true)"] {
        let body = check(&format!(
            "switch (n) {{ default: using r: R | null = new R(); {loop_head} {{}} }}"
        ));
        assert_eq!(disposal_count(&body), 0);
        let body = check(&format!(
            "switch (n) {{ default: using r: R | null = new R(); {loop_head} {{ break; }} }}"
        ));
        assert_eq!(disposal_count(&body), 1);
    }
    let body = check(
        "switch (n) { case 0: using r: R | null = new R();
         default: if (flag) { return; } break; }",
    );
    assert_eq!(disposal_count(&body), 2);
}
