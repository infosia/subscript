//! The global number predicates, an array in a template, and `satisfies`
//! erase to existing forms (compiler.md §204 rules 1 to 3).
//! Cost: in-process checker calls only; no compiler or engine subprocess.

use crate::{check_program, hir, SourceFile};

fn normalize_expr(expression: &mut hir::Expr) {
    expression.pos = crate::diag::Pos::new("erasure.ts", 0, 0);
    for child in expression.children_mut() {
        match child {
            hir::HirChildMut::Expr(child) => normalize_expr(child),
            hir::HirChildMut::Stmt(child) => normalize_stmt(child),
        }
    }
}

fn normalize_stmt(statement: &mut hir::Stmt) {
    if let hir::Stmt::Let { pos, .. } = statement {
        *pos = crate::diag::Pos::new("erasure.ts", 0, 0);
    }
    for child in statement.children_mut() {
        match child {
            hir::HirChildMut::Expr(child) => normalize_expr(child),
            hir::HirChildMut::Stmt(child) => normalize_stmt(child),
        }
    }
}

/// The statements of `main`, with every position cleared.
fn main_body(body: &str) -> Vec<hir::Stmt> {
    let source = format!("export function main(): void {{\n{body}\n}}\n");
    let module = check_program(&[SourceFile::new("erasure.ts", source)])
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

#[test]
fn an_array_operand_is_its_comma_join() {
    let declared = "const xs: i32[] = [1, 2];\n";
    let operand = main_body(&format!("{declared}print(`<${{xs}}>`);"));
    let joined = main_body(&format!("{declared}print(`<${{xs.join(\",\")}}>`);"));
    assert_eq!(operand, joined);
    // Control: a different separator gives a different HIR.
    let other = main_body(&format!("{declared}print(`<${{xs.join(\";\")}}>`);"));
    assert_ne!(operand, other);
}

#[test]
fn satisfies_is_its_operand() {
    let declared = "const n: i64 = 3;\n";
    let asserted = main_body(&format!(
        "{declared}const m = n satisfies i64;\nprint(`${{m}}`);"
    ));
    let plain = main_body(&format!("{declared}const m = n;\nprint(`${{m}}`);"));
    assert_eq!(asserted, plain);
    // A literal operand takes the target as its context.
    let literal = main_body("const m = 3 satisfies i64;\nprint(`${m}`);");
    let annotated = main_body("const m: i64 = 3;\nprint(`${m}`);");
    let init = |body: &[hir::Stmt]| match &body[0] {
        hir::Stmt::Let { init, .. } => init.clone(),
        other => panic!("expected a let, got {other:?}"),
    };
    assert_eq!(init(&literal), init(&annotated));
    // Control: a different operand gives a different HIR.
    let other = main_body(&format!("{declared}const m = n + 1;\nprint(`${{m}}`);"));
    assert_ne!(asserted, other);
}

#[test]
fn a_global_predicate_is_its_number_form() {
    for name in ["isNaN", "isFinite"] {
        let global = main_body(&format!("const b = {name}(1.5);"));
        let number = main_body(&format!("const b = Number.{name}(1.5);"));
        assert_eq!(global, number, "{name}");
        // An `i32` argument gives the diagnostic of the `Number` form.
        let error = |call: &str| {
            let source =
                format!("export function main(): void {{\nconst n: i32 = 1;\n{call}(n);\n}}\n");
            let errors = check_program(&[SourceFile::new("erasure.ts", source)])
                .expect_err("an i32 argument is rejected");
            errors
                .iter()
                .map(|error| (error.code, error.message.clone()))
                .collect::<Vec<_>>()
        };
        let global_errors = error(name);
        assert!(!global_errors.is_empty());
        assert_eq!(global_errors, error(&format!("Number.{name}")), "{name}");
    }
    // Control: a different predicate gives a different HIR.
    assert_ne!(
        main_body("const b = isNaN(1.5);"),
        main_body("const b = isFinite(1.5);")
    );
}
