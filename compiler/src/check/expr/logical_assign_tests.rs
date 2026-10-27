//! Logical assignment tests (compiler.md §199).
//! Cost: in-process checker calls only; no compiler or engine subprocess.

use swc_ecma_ast as ast;

use super::LogicalAssign;
use crate::{check_program, hir, Diagnostic, SourceFile};

const BOX: &str = "class Box { v: i32 = 1; }\nfunction make(): Box { return new Box(); }\nfunction maybe(): Box | null { return null; }\nfunction touch(): void {}\nfunction flag(): boolean { return true; }\n";

fn normalize_expr(expression: &mut hir::Expr) {
    expression.pos = crate::diag::Pos::new("identity.ts", 0, 0);
    for child in expression.children_mut() {
        match child {
            hir::HirChildMut::Expr(child) => normalize_expr(child),
            hir::HirChildMut::Stmt(child) => normalize_stmt(child),
        }
    }
}

fn normalize_stmt(statement: &mut hir::Stmt) {
    if let hir::Stmt::Expr(expression) = statement {
        normalize_expr(expression);
    } else {
        panic!("the identity body must hold expression statements only");
    }
}

/// The last statement of `main`, with every position cleared.
fn main_tail(body: &str) -> hir::Stmt {
    let source = format!("{BOX}export function main(): void {{\n{body}\n}}\n");
    let module = check_program(&[SourceFile::new("identity.ts", source)])
        .expect("the identity program must check");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    let mut statement = main.body.last().expect("main statement").clone();
    normalize_stmt(&mut statement);
    statement
}

fn diagnostics(body: &str) -> Vec<Diagnostic> {
    let source = format!("{BOX}export function main(): void {{\n{body}\n}}\n");
    check_program(&[SourceFile::new("test.ts", source)]).expect_err("the program must fail")
}

fn checks(source: &str) -> bool {
    check_program(&[SourceFile::new("test.ts", format!("{BOX}{source}"))]).is_ok()
}

#[test]
fn operator_selection_and_spelling() {
    for (op, logical, spelling) in [
        (ast::AssignOp::NullishAssign, LogicalAssign::Nullish, "??="),
        (ast::AssignOp::OrAssign, LogicalAssign::Or, "||="),
        (ast::AssignOp::AndAssign, LogicalAssign::And, "&&="),
    ] {
        assert_eq!(LogicalAssign::of(op), Some(logical));
        assert_eq!(logical.spelling(), spelling);
    }
    // Control: a compound or plain operator is not a logical assignment.
    assert_eq!(LogicalAssign::of(ast::AssignOp::AddAssign), None);
    assert_eq!(LogicalAssign::of(ast::AssignOp::Assign), None);
}

#[test]
fn local_nullish_assignment_has_the_rule_1_hir() {
    let prefix = "  let x: Box | null = maybe();\n";
    let sugar = main_tail(&format!("{prefix}  x ??= make();"));
    let spelled = main_tail(&format!("{prefix}  x !== null ? x : (x = make());"));
    assert_eq!(sugar, spelled);
    let value = main_tail(&format!("{prefix}  print(`${{(x ??= make()).v}}`);"));
    let spelled_value = main_tail(&format!(
        "{prefix}  print(`${{(x !== null ? x : (x = make())).v}}`);"
    ));
    assert_eq!(value, spelled_value);
    // Control: a different spelling gives a different HIR.
    let other = main_tail(&format!("{prefix}  x === null ? (x = make()) : x;"));
    assert_ne!(sugar, other);
}

#[test]
fn local_boolean_logical_assignments_have_the_rule_1_hir() {
    let prefix = "  let a: boolean = flag();\n";
    assert_eq!(
        main_tail(&format!("{prefix}  a ||= flag();")),
        main_tail(&format!("{prefix}  a ? a : (a = flag());"))
    );
    assert_eq!(
        main_tail(&format!("{prefix}  a &&= flag();")),
        main_tail(&format!("{prefix}  a ? (a = flag()) : a;"))
    );
    // Control: the two operators do not share one form.
    assert_ne!(
        main_tail(&format!("{prefix}  a ||= flag();")),
        main_tail(&format!("{prefix}  a &&= flag();"))
    );
}

#[test]
fn statement_nullish_assignment_narrows_a_local() {
    assert!(checks(
        "export function main(): void { let x: Box | null = maybe(); x ??= make(); touch(); print(`${x.v}`); }"
    ));
    // Control: without the assignment the read fails.
    assert!(!checks(
        "export function main(): void { let x: Box | null = maybe(); print(`${x.v}`); }"
    ));
    // Control: a nullable right operand does not narrow.
    assert!(!checks(
        "export function main(): void { let x: Box | null = maybe(); x ??= maybe(); print(`${x.v}`); }"
    ));
}

#[test]
fn statement_nullish_assignment_narrows_a_this_field_until_a_call() {
    let class = |body: &str| {
        format!(
            "class Holder {{ cache: Box | null = null; get(): Box {{ {body} }} }}\nexport function main(): void {{}}\n"
        )
    };
    assert!(checks(&class("this.cache ??= make(); return this.cache;")));
    // §124: a call ends the narrowing of a shared location.
    assert!(!checks(&class(
        "this.cache ??= make(); touch(); return this.cache;"
    )));
    // Control: the spelled if-statement narrows and ends in the same places.
    assert!(checks(&class(
        "if (this.cache === null) { this.cache = make(); } return this.cache;"
    )));
    assert!(!checks(&class(
        "if (this.cache === null) { this.cache = make(); } touch(); return this.cache;"
    )));
    // Control: without the assignment the return fails.
    assert!(!checks(&class("return this.cache;")));
}

#[test]
fn nullish_assignment_requires_a_nullable_place() {
    let result = diagnostics("  let b: Box = make();\n  b ??= make();");
    assert_eq!(result[0].code.as_str(), "S100");
    assert_eq!(
        result[0].message,
        "the left operand of `??=` has type `Box`, which is not nullable"
    );
    assert!(result[0].divergence.is_some());
}

#[test]
fn nullish_assignment_value_fails_as_the_plain_assignment() {
    let plain = diagnostics("  let x: Box | null = maybe();\n  x = 1;");
    let logical = diagnostics("  let x: Box | null = maybe();\n  x ??= 1;");
    assert_eq!(logical[0].code, plain[0].code);
    assert_eq!(logical[0].message, plain[0].message);
}

#[test]
fn boolean_logical_assignment_requires_boolean_operands() {
    let left = diagnostics("  let s: string = \"\";\n  s ||= \"d\";");
    assert_eq!(left[0].code.as_str(), "S100");
    assert_eq!(
        left[0].message,
        "`||=` requires a boolean operand, got `string`"
    );
    let right = diagnostics("  let a: boolean = true;\n  a &&= 1;");
    assert_eq!(
        right[0].message,
        "`&&=` requires a boolean operand, got `i32`"
    );
    // Control: two boolean operands check.
    assert!(checks(
        "export function main(): void { let a: boolean = true; a &&= flag(); a ||= false; }"
    ));
}

#[test]
fn accessor_logical_assignment_is_a_statement() {
    let class = "class Slot { stored: Box | null = null; get item(): Box | null { return this.stored; } set item(value: Box | null) { this.stored = value; } }\n";
    assert!(checks(&format!(
        "{class}export function main(): void {{ const s: Slot = new Slot(); s.item ??= make(); }}"
    )));
    let source = format!(
        "{BOX}{class}export function main(): void {{ const s: Slot = new Slot(); const b = (s.item ??= make()); }}"
    );
    let result = check_program(&[SourceFile::new("test.ts", source)])
        .expect_err("the accessor write used as a value must fail");
    assert_eq!(
        result[0].message,
        "`x.item ??= v` cannot be used as a value"
    );
}

#[test]
fn statement_receiver_call_evaluates_once() {
    let source = "class Holder { cache: Box | null = null; }\nfunction pick(h: Holder): Holder { return h; }\nexport function main(): void { const h: Holder = new Holder(); pick(h).cache ??= make(); }\n";
    let module = check_program(&[SourceFile::new("test.ts", format!("{BOX}{source}"))])
        .expect("the receiver call program must check");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    let text = format!("{:?}", main.body);
    assert_eq!(text.matches("pick").count(), 1, "{text}");
    assert!(text.contains("[[compound#"), "{text}");
}
