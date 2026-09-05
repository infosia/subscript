use subscript_compiler::{
    check_program, divergence::Divergence, hir, Diagnostic, RuleCode, SourceFile,
};

const SUPPORT: &str = "class Box {\n\
                      \x20 v: i32;\n\
                      \x20 constructor(v: i32) { this.v = v; }\n\
                      }\n\
                      function maybe(keep: boolean): Box | null {\n\
                      \x20 return keep ? new Box(1) : null;\n\
                      }\n";

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_program(&[SourceFile::new("test.ts", source)]).expect_err("the program must fail")
}

#[test]
fn a_non_place_switch_case_test_matches_the_initializer_rejection() {
    let initializer = diagnostics(&format!(
        "{SUPPORT}class Holder {{\n\
         \x20 value: i32 = (maybe(false) ?? new Box(1)).v;\n\
         }}\n"
    ));
    let switch_case = diagnostics(&format!(
        "{SUPPORT}export function main(): void {{\n\
         \x20 const fb: Box = new Box(1);\n\
         \x20 switch (0) {{\n\
         \x20   case (maybe(false) ?? fb).v: break;\n\
         \x20   default: break;\n\
         \x20 }}\n\
         }}\n"
    ));

    assert_eq!(initializer.len(), 1, "diagnostics: {initializer:?}");
    assert_eq!(switch_case.len(), 1, "diagnostics: {switch_case:?}");
    assert_eq!(switch_case[0].code, RuleCode::S100);
    assert_eq!(switch_case[0].code, initializer[0].code);
    assert_eq!(switch_case[0].message, initializer[0].message);
    assert_eq!(
        switch_case[0].divergence,
        Some(Divergence::NonPlaceNullishInitializer)
    );
    assert_eq!(switch_case[0].divergence, initializer[0].divergence);
}

#[test]
fn switch_case_tests_without_a_prefix_check_clean() {
    let source = format!(
        "{SUPPORT}function five(): i32 {{ return 5; }}\n\
         export function main(): void {{\n\
         \x20 const fb: Box = new Box(1);\n\
         \x20 switch (0) {{\n\
         \x20   case fb.v: break;\n\
         \x20   case five(): break;\n\
         \x20   default: break;\n\
         \x20 }}\n\
         }}\n"
    );
    check_program(&[SourceFile::new("test.ts", source)])
        .expect("case tests without a synthetic prefix must check");
}

#[test]
fn a_for_condition_prefix_stays_at_the_head_of_the_while_body() {
    let source = format!(
        "{SUPPORT}export function main(): void {{\n\
         \x20 const fb: Box = new Box(1);\n\
         \x20 let count: i32 = 0;\n\
         \x20 for (let i: i32 = 0; i < 3 && (maybe(true) ?? fb).v > 0; i++) {{ count++; }}\n\
         }}\n"
    );
    let module =
        check_program(&[SourceFile::new("test.ts", source)]).expect("the for condition must check");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main must exist");
    let hir::Stmt::Block(loop_block) = &main.body[2] else {
        panic!("the rewritten for must be a block: {:?}", main.body[2]);
    };
    let [hir::Stmt::Let { name: init, .. }, hir::Stmt::While { body, .. }] = loop_block.as_slice()
    else {
        panic!("the block must own the initializer and while: {loop_block:?}");
    };
    assert_eq!(init, "i");
    let [hir::Stmt::Let { name: prefix, .. }, hir::Stmt::If { .. }] = body.as_slice() else {
        panic!("the while body must start with the condition prefix: {body:?}");
    };
    assert!(prefix.starts_with("[["), "synthetic local: {prefix}");
}

#[test]
fn a176_and_a177_shapes_have_no_diagnostics() {
    for (name, source) in [
        (
            "a176-compound-through-accessor.ts",
            include_str!("../../corpus/accept/a176-compound-through-accessor.ts"),
        ),
        (
            "a177-nullish.ts",
            include_str!("../../corpus/accept/a177-nullish.ts"),
        ),
    ] {
        check_program(&[SourceFile::new(name, source)])
            .unwrap_or_else(|diagnostics| panic!("{name} must check: {diagnostics:?}"));
    }
}

fn synthetic_local_count(node: hir::HirChild<'_>) -> usize {
    let (own, children) = match node {
        hir::HirChild::Expr(expression) => (0, expression.children()),
        hir::HirChild::Stmt(statement) => (
            usize::from(matches!(statement, hir::Stmt::Let { name, .. } if name.starts_with("[["))),
            statement.children(),
        ),
    };
    own + children
        .into_iter()
        .map(synthetic_local_count)
        .sum::<usize>()
}

#[test]
fn owner_receiver_matrix_matches_hand_written_hir_and_diagnostics() {
    #[derive(Clone, Copy, Debug)]
    enum Expected {
        Hir { count: usize, index: usize },
        Diagnostic { code: RuleCode, line: u32, col: u32 },
    }

    // Each row states the contract independently of the owner's implementation.
    let rows = [
        (
            "Statement",
            "(maybe() ?? fb).v",
            Expected::Hir { count: 1, index: 0 },
        ),
        (
            "Statement",
            "maybe()?.v ?? 0",
            Expected::Hir { count: 1, index: 0 },
        ),
        (
            "Declarator",
            "(pick(a) ?? fb).v",
            Expected::Hir { count: 1, index: 1 },
        ),
        (
            "Declarator",
            "pick(a)?.v ?? 0",
            Expected::Hir { count: 1, index: 1 },
        ),
        (
            "ForInit",
            "(maybe() ?? fb).v",
            Expected::Hir { count: 1, index: 0 },
        ),
        (
            "ForInit",
            "maybe()?.v ?? 0",
            Expected::Hir { count: 1, index: 0 },
        ),
        (
            "ForCond",
            "(maybe() ?? fb).v",
            Expected::Hir { count: 1, index: 0 },
        ),
        (
            "ForCond",
            "maybe()?.v ?? 0",
            Expected::Hir { count: 1, index: 0 },
        ),
        (
            "ForUpdate",
            "(maybe() ?? fb).v",
            Expected::Hir { count: 1, index: 1 },
        ),
        (
            "ForUpdate",
            "maybe()?.v ?? 0",
            Expected::Hir { count: 1, index: 1 },
        ),
        (
            "ArrowBody",
            "(maybe() ?? fb).v",
            Expected::Hir { count: 1, index: 0 },
        ),
        (
            "ArrowBody",
            "maybe()?.v ?? 0",
            Expected::Hir { count: 1, index: 0 },
        ),
        (
            "Initializer",
            "(maybe() ?? fb).v",
            Expected::Diagnostic {
                code: RuleCode::S100,
                line: 5,
                col: 32,
            },
        ),
        (
            "Initializer",
            "maybe()?.v ?? 0",
            Expected::Diagnostic {
                code: RuleCode::S100,
                line: 5,
                col: 16,
            },
        ),
        (
            "SwitchCase",
            "(maybe() ?? fb).v",
            Expected::Diagnostic {
                code: RuleCode::S100,
                line: 5,
                col: 37,
            },
        ),
        (
            "SwitchCase",
            "maybe()?.v ?? 0",
            Expected::Diagnostic {
                code: RuleCode::S100,
                line: 5,
                col: 21,
            },
        ),
    ];
    const MATRIX_SUPPORT: &str = "class Box { v: i32; constructor(v: i32) { this.v = v; } }\n\
        function maybe(): Box | null { return null; }\n\
        const fb: Box = new Box(1);\n";

    for (kind, receiver, expected) in rows {
        let site = match kind {
            "Statement" => format!("const value: i32 = {receiver};"),
            "Declarator" => format!("let a: Box = new Box(2), b: i32 = {receiver};"),
            "ForInit" => format!("for (let value: i32 = {receiver}; false;) {{}}"),
            "ForCond" => format!("for (; ({receiver}) > 0;) {{}}"),
            "ForUpdate" => format!("for (; false; {receiver}) {{ fb.v; }}"),
            "ArrowBody" => format!("const value: () => i32 = (): i32 => {receiver};"),
            "Initializer" => format!("value: i32 = {receiver};"),
            "SwitchCase" => format!("switch (0) {{ case {receiver}: break; }}"),
            _ => unreachable!(),
        };
        let head = if kind == "Initializer" {
            "class Holder {"
        } else {
            "export function main(): void {"
        };
        let pick = if kind == "Declarator" {
            "function pick(value: Box): Box | null { return value; }\n"
        } else {
            ""
        };
        let source = format!("{MATRIX_SUPPORT}{pick}{head}\n  {site}\n}}\n");
        let result = check_program(&[SourceFile::new("matrix.ts", source)]);
        match expected {
            Expected::Diagnostic { code, line, col } => {
                let diagnostics = result.expect_err("this owner cannot place a prefix");
                assert_eq!(diagnostics.len(), 1, "{kind} / {receiver}: {diagnostics:?}");
                let diagnostic = &diagnostics[0];
                assert_eq!(diagnostic.code, code, "{kind} / {receiver}");
                assert_eq!(diagnostic.pos.file, "matrix.ts");
                assert_eq!(
                    (diagnostic.pos.line, diagnostic.pos.col),
                    (line, col),
                    "{kind} / {receiver}"
                );
                assert_eq!(
                    diagnostic.message,
                    "a non-place receiver of `??` or `?.` cannot be used in an initializer"
                );
                assert_eq!(
                    diagnostic.divergence,
                    Some(Divergence::NonPlaceNullishInitializer)
                );
                println!(
                    "{kind} / {receiver}: expected {code:?} {line}:{col}; measured {:?} {}:{}",
                    diagnostic.code, diagnostic.pos.line, diagnostic.pos.col
                );
            }
            Expected::Hir { count, index } => {
                let module = result
                    .unwrap_or_else(|diagnostics| panic!("{kind} / {receiver}: {diagnostics:?}"));
                let main = module
                    .functions
                    .iter()
                    .find(|function| function.name == "main")
                    .expect("main must exist");
                let measured_count: usize = main
                    .body
                    .iter()
                    .map(|statement| synthetic_local_count(hir::HirChild::Stmt(statement)))
                    .sum();
                let list = match kind {
                    "Statement" => {
                        assert!(
                            matches!(&main.body[1], hir::Stmt::Let { name, .. } if name == "value")
                        );
                        &main.body
                    }
                    "Declarator" => {
                        let bindings: Vec<_> = main
                            .body
                            .iter()
                            .map(|statement| {
                                let hir::Stmt::Let { name, .. } = statement else {
                                    panic!("declarator statement must be Let: {statement:?}");
                                };
                                if name.starts_with("[[") {
                                    "synthetic Let"
                                } else {
                                    name.as_str()
                                }
                            })
                            .collect();
                        assert_eq!(
                            bindings,
                            ["a", "synthetic Let", "b"],
                            "{kind} / {receiver}: declaration order"
                        );
                        &main.body
                    }
                    "ForInit" => {
                        let hir::Stmt::For {
                            init: Some(init), ..
                        } = &main.body[1]
                        else {
                            panic!("initializer must precede a for: {:?}", main.body)
                        };
                        assert!(matches!(&**init, hir::Stmt::Let { name, .. } if name == "value"));
                        &main.body
                    }
                    "ForCond" | "ForUpdate" => {
                        let [hir::Stmt::Block(block)] = main.body.as_slice() else {
                            panic!("loop block: {:?}", main.body)
                        };
                        let [hir::Stmt::While { body, .. }] = block.as_slice() else {
                            panic!("while: {block:?}")
                        };
                        if kind == "ForCond" {
                            assert!(matches!(&body[1], hir::Stmt::If { .. }));
                        } else {
                            assert!(matches!(&body[0], hir::Stmt::Expr(_)));
                            assert!(matches!(&body[2], hir::Stmt::Expr(_)));
                        }
                        body
                    }
                    "ArrowBody" => {
                        let [hir::Stmt::Let { init, .. }] = main.body.as_slice() else {
                            panic!("arrow binding: {:?}", main.body)
                        };
                        let hir::ExprKind::Lambda { body, .. } = &init.kind else {
                            panic!("lambda: {init:?}")
                        };
                        assert!(matches!(&body[1], hir::Stmt::Return { .. }));
                        body
                    }
                    _ => unreachable!(),
                };
                let indices: Vec<_> = list
                    .iter()
                    .enumerate()
                    .filter_map(|(index, statement)| {
                        matches!(statement, hir::Stmt::Let { name, .. } if name.starts_with("[["))
                            .then_some(index)
                    })
                    .collect();
                assert_eq!(measured_count, count, "{kind} / {receiver}");
                assert_eq!(indices, vec![index], "{kind} / {receiver}");
                println!("{kind} / {receiver}: expected count={count}, index={index}; measured count={measured_count}, indices={indices:?}");
            }
        }
    }
}
