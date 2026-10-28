//! Multi-argument `push`, `Math.max`, and `Math.min` (compiler.md §201).
//! Cost: in-process checker calls only; no compiler or engine subprocess.

use crate::{check_program, hir, Diagnostic, SourceFile};

const PRELUDE: &str = "let order: i32 = 0;\nfunction g(v: i32): i32 { order = order * 10 + v; return v; }\nfunction arr(xs: i32[]): i32[] { return xs; }\nfunction m(v: f64): f64 { return v; }\n";

fn normalize_expr(expression: &mut hir::Expr) {
    expression.pos = crate::diag::Pos::new("multi.ts", 0, 0);
    for child in expression.children_mut() {
        match child {
            hir::HirChildMut::Expr(child) => normalize_expr(child),
            hir::HirChildMut::Stmt(child) => normalize_stmt(child),
        }
    }
}

fn normalize_stmt(statement: &mut hir::Stmt) {
    match statement {
        hir::Stmt::Expr(expression) => normalize_expr(expression),
        hir::Stmt::Let { init, pos, .. } => {
            *pos = crate::diag::Pos::new("multi.ts", 0, 0);
            normalize_expr(init);
        }
        // A lambda body keeps its positions; no comparison reads them.
        _ => {}
    }
}

/// The statements of `main`, with every position cleared.
fn main_body(body: &str) -> Vec<hir::Stmt> {
    main_body_after("", body)
}

/// The statements of `main` after the module `declarations`.
fn main_body_after(declarations: &str, body: &str) -> Vec<hir::Stmt> {
    let source = format!("{PRELUDE}{declarations}export function main(): void {{\n{body}\n}}\n");
    let module = check_program(&[SourceFile::new("multi.ts", source)])
        .unwrap_or_else(|errors| panic!("the program must check: {errors:?}"));
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    let mut body = main.body.clone();
    body.iter_mut().for_each(normalize_stmt);
    body
}

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_program(&[SourceFile::new("multi.ts", format!("{PRELUDE}{source}"))])
        .expect_err("the program must fail")
}

fn is_push(expression: &hir::Expr) -> bool {
    matches!(
        &expression.kind,
        hir::ExprKind::Call { callee: hir::Callee::Method { name, .. }, args }
            if name.full_text() == "push" && args.len() == 1
    )
}

fn is_store(expression: &hir::Expr) -> bool {
    matches!(
        &expression.kind,
        hir::ExprKind::Assign { op: None, update: None, target, .. }
            if matches!(&target.kind, hir::ExprKind::Local(name, ..) if name.starts_with("[[compound#"))
    )
}

/// Returns the effects and the value of a `Seq`.
fn sequence(expression: &hir::Expr) -> (&[hir::Expr], &hir::Expr) {
    match &expression.kind {
        hir::ExprKind::Seq { effects, value } => (effects, value),
        other => panic!("expected a Seq, got {other:?}"),
    }
}

fn last_expression(body: &[hir::Stmt]) -> &hir::Expr {
    match body.last() {
        Some(hir::Stmt::Expr(expression)) => expression,
        Some(hir::Stmt::Let { init, .. }) => init,
        other => panic!("expected an expression or a let, got {other:?}"),
    }
}

#[test]
fn push_evaluates_every_argument_before_the_first_append() {
    let body = main_body("const xs: i32[] = [];\nxs.push(g(1), g(2), g(3));");
    let (effects, value) = sequence(last_expression(&body));
    // Rule 1: the receiver store and three argument stores, then the first
    // two appends; the value is the last.
    assert_eq!(effects.len(), 6);
    assert!(effects[..4].iter().all(is_store));
    assert!(effects[4..].iter().all(is_push));
    assert!(is_push(value));
    assert_eq!(value.ty, crate::types::Type::I32);
    // Each append reads the local that its argument stored, in order.
    let stored: Vec<_> = effects[1..4]
        .iter()
        .map(|store| match &store.kind {
            hir::ExprKind::Assign { target, .. } => target.as_ref().clone(),
            _ => unreachable!(),
        })
        .collect();
    let appended: Vec<_> = effects[4..]
        .iter()
        .chain(std::iter::once(value))
        .map(|append| match &append.kind {
            hir::ExprKind::Call { args, .. } => args[0].clone(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(stored, appended);
}

#[test]
fn one_argument_push_keeps_the_single_append() {
    let single = main_body("const xs: i32[] = [];\nxs.push(g(1));");
    let expression = last_expression(&single);
    assert!(is_push(expression));
    let hir::ExprKind::Call { args, .. } = &expression.kind else {
        unreachable!()
    };
    assert!(matches!(&args[0].kind, hir::ExprKind::Call { .. }));
    // The single append declares no synthetic local.
    assert_eq!(single.len(), 2);
    // Control: two arguments give a Seq.
    let double = main_body("const xs: i32[] = [];\nxs.push(g(1), g(2));");
    assert!(matches!(
        last_expression(&double).kind,
        hir::ExprKind::Seq { .. }
    ));
}

#[test]
fn value_position_push_is_one_seq_in_source_order() {
    let body = main_body("const xs: i32[] = [];\nconst n = g(9) + xs.push(g(1), g(2));");
    // The synthetic locals are declared before the declarator, unassigned.
    let declared: Vec<_> = body
        .iter()
        .filter_map(|statement| match statement {
            hir::Stmt::Let { name, init, .. } if name.starts_with("[[compound#") => {
                assert!(matches!(init.kind, hir::ExprKind::Unassigned));
                Some(name.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(declared.len(), 3);
    let hir::ExprKind::Binary { left, right, .. } = &last_expression(&body).kind else {
        panic!("expected the addition");
    };
    // The left operand runs before the sequence.
    assert!(matches!(left.kind, hir::ExprKind::Call { .. }));
    let (effects, value) = sequence(right);
    assert!(effects[..3].iter().all(is_store) && is_push(&effects[3]));
    assert!(is_push(value));
}

/// Returns the receiver of an append.
fn receiver(append: &hir::Expr) -> &hir::Expr {
    match &append.kind {
        hir::ExprKind::Call {
            callee: hir::Callee::Method { recv, .. },
            ..
        } => recv,
        other => panic!("expected an append, got {other:?}"),
    }
}

#[test]
fn a_global_receiver_is_read_before_the_arguments() {
    let source = "let store: i32[] = [];\nfunction reset(): i32 { store = []; return 7; }\n";
    let body = main_body_after(source, "store.push(1, reset());");
    let (effects, value) = sequence(last_expression(&body));
    // Rule 4: the global is stored first, and each append reads the local.
    let hir::ExprKind::Assign {
        target,
        value: read,
        ..
    } = &effects[0].kind
    else {
        panic!("expected the receiver store");
    };
    assert!(matches!(read.kind, hir::ExprKind::Global(_)));
    assert_eq!(receiver(&effects[2]), target.as_ref());
    assert_eq!(receiver(value), target.as_ref());
    // Control: order-free arguments read the global with no local.
    let free = main_body_after(source, "store.push(1, 2);");
    let (effects, value) = sequence(last_expression(&free));
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        receiver(&effects[0]).kind,
        hir::ExprKind::Global(_)
    ));
    assert!(matches!(receiver(value).kind, hir::ExprKind::Global(_)));
}

#[test]
fn receiver_and_literal_arguments() {
    // A non-place receiver is stored first; a literal argument is not stored.
    let body = main_body("const xs: i32[] = [];\narr(xs).push(8, 9);");
    let (effects, value) = sequence(last_expression(&body));
    assert_eq!(effects.len(), 2);
    assert!(is_store(&effects[0]));
    assert!(is_push(&effects[1]) && is_push(value));
    // Control: a local receiver with literal arguments needs no local.
    let local = main_body("const xs: i32[] = [];\nxs.push(8, 9);");
    let (effects, _) = sequence(last_expression(&local));
    assert_eq!(effects.len(), 1);
    assert!(is_push(&effects[0]));
}

#[test]
fn push_without_arguments_is_the_length() {
    let body = main_body("const xs: i32[] = [];\nconst n = xs.push();");
    assert!(matches!(
        last_expression(&body).kind,
        hir::ExprKind::Length(_)
    ));
}

#[test]
fn push_in_an_initializer_needs_a_statement_list() {
    let errors = diagnostics(
        "const store: i32[] = [];\nfunction f(n: i32 = arr(store).push(g(1), 2)): i32 { return n; }\nexport function main(): void { f(); }",
    );
    assert!(errors.iter().any(|error| error.message
        == "a non-place receiver or index of `push` cannot be used in an initializer"));
    // Rule 4: when only an argument needs a local, the text names the argument.
    let errors = diagnostics(
        "const store: i32[] = [];\nfunction f(n: i32 = store.push(g(1), 2)): i32 { return n; }\nexport function main(): void { f(); }",
    );
    assert!(errors.iter().any(|error| error.message
        == "an argument of a `push` with two or more arguments needs a synthetic local, \
            which an initializer cannot declare"));
    // Control: literal arguments on a global receiver need no synthetic local.
    let source = format!(
        "{PRELUDE}const store: i32[] = [];\nfunction f(n: i32 = store.push(1, 2)): i32 {{ return n; }}\nexport function main(): void {{ f(); }}"
    );
    assert!(check_program(&[SourceFile::new("multi.ts", source)]).is_ok());
}

#[test]
fn push_spread_keeps_its_rejection() {
    let errors =
        diagnostics("export function main(): void { const xs: i32[] = []; xs.push(1, ...xs); }");
    assert_eq!(
        errors[0].message,
        "spread arguments require variadic parameters, which the language does not have"
    );
}

#[test]
fn math_max_and_min_fold_from_the_left() {
    let fold = main_body("const a = Math.max(m(1), m(2), m(3), m(4));");
    let spelled = main_body("const a = Math.max(Math.max(Math.max(m(1), m(2)), m(3)), m(4));");
    assert_eq!(fold, spelled);
    let fold = main_body("const a = Math.min(m(1), m(2), m(3));");
    let spelled = main_body("const a = Math.min(Math.min(m(1), m(2)), m(3));");
    assert_eq!(fold, spelled);
    // Control: max and min do not fold to the same HIR.
    assert_ne!(
        fold,
        main_body("const a = Math.max(Math.max(m(1), m(2)), m(3));")
    );
}

#[test]
fn math_max_and_min_with_zero_and_one_argument() {
    for (call, expected) in [
        ("Math.max()", f64::NEG_INFINITY),
        ("Math.min()", f64::INFINITY),
    ] {
        let body = main_body(&format!("const a = {call};"));
        assert!(
            matches!(last_expression(&body).kind, hir::ExprKind::Float(value) if value == expected),
            "{call}"
        );
    }
    assert_eq!(
        main_body("const a = Math.max(m(1));"),
        main_body("const a = m(1);")
    );
    // The single argument is still an f64 argument.
    let errors = diagnostics("export function main(): void { const i: i32 = 1; Math.min(i); }");
    assert!(errors[0].message.contains("implicit numeric conversion"));
}

#[test]
fn math_hypot_and_spread_keep_their_rejections() {
    let errors = diagnostics("export function main(): void { Math.hypot(1, 2, 3); }");
    assert!(errors[0]
        .message
        .contains("a fold of `Math.hypot` rounds differently"));
    // Rule 5: each spread gives the call-spread text.
    for call in [
        "Math.max(...xs)",
        "Math.max(1, 2, ...xs)",
        "Math.min(1, ...xs)",
    ] {
        let errors = diagnostics(&format!(
            "export function main(): void {{ const xs: f64[] = []; {call}; }}"
        ));
        assert_eq!(
            errors[0].message,
            "spread arguments require variadic parameters, which the language does not have",
            "{call}"
        );
    }
    // Control: two hypot arguments check.
    let source = format!("{PRELUDE}export function main(): void {{ Math.hypot(1, 2); }}");
    assert!(check_program(&[SourceFile::new("multi.ts", source)]).is_ok());
}

#[test]
fn lambda_arguments_need_no_synthetic_local() {
    let body = main_body(
        "const fs: ((v: i32) => i32)[] = [];\nfs.push((v: i32): i32 => v + 1, (v: i32): i32 => v * 2);",
    );
    let (effects, value) = sequence(last_expression(&body));
    assert_eq!(effects.len(), 1);
    assert!(is_push(&effects[0]) && is_push(value));
    // Control: a call argument is stored.
    let called = main_body("const xs: i32[] = [];\nxs.push(g(1), 2);");
    let (effects, _) = sequence(last_expression(&called));
    assert!(is_store(&effects[0]));
}
