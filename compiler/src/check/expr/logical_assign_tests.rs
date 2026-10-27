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
    assert!(left[0].divergence.is_some());
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

#[test]
fn boolean_logical_assignment_value_fails_as_the_plain_assignment() {
    for (declaration, logical, plain) in [
        ("let a: boolean = true;", "a &&= 1;", "a = 1;"),
        ("let s: string = \"\";", "s ||= 1;", "s = 1;"),
    ] {
        let plain = diagnostics(&format!("  {declaration}\n  {plain}"));
        let result = diagnostics(&format!("  {declaration}\n  {logical}"));
        assert_eq!(result[0].code, plain[0].code, "{logical}");
        assert_eq!(result[0].message, plain[0].message, "{logical}");
        assert!(result[0].divergence.is_none(), "{logical}");
    }
}

#[test]
fn boolean_logical_assignment_has_the_type_of_the_place() {
    let generic = |tail: &str| {
        format!(
            "function f<T extends boolean>(x: T, y: T, b: boolean): {tail}\nexport function main(): void {{}}\n"
        )
    };
    assert!(checks(&generic("T { return (x ||= y); }")));
    assert!(!checks(&generic("boolean { return (x &&= b); }")));
    // Control: a boolean place gives `boolean`.
    assert!(checks(
        "function g(a: boolean, b: boolean): boolean { return (a ||= b); }\nexport function main(): void {}\n"
    ));
}

#[test]
fn initializer_rejection_names_the_logical_operator() {
    let source = "class H { flags: boolean[] = [false]; }\nconst G: H = new H();\nfunction getH(): H { return G; }\nconst z: boolean = (getH().flags[0] ||= true);\nexport function main(): void {}\n";
    let result = check_program(&[SourceFile::new("test.ts", format!("{BOX}{source}"))])
        .expect_err("a non-place receiver in an initializer must fail");
    assert!(
        result.iter().any(|diagnostic| diagnostic.message
            == "a non-place receiver or index of `||=` cannot be used in an initializer"),
        "{result:?}"
    );
    // Control: `??` keeps its own text.
    let nullish = "const y: Box = maybe() ?? make();\nexport function main(): void {}\n";
    let result = check_program(&[SourceFile::new("test.ts", format!("{BOX}{nullish}"))])
        .expect_err("a non-place `??` receiver in an initializer must fail");
    assert!(
        result.iter().any(|diagnostic| diagnostic.message
            == "a non-place receiver of `??` or `?.` cannot be used in an initializer"),
        "{result:?}"
    );
}

/// Checks `source` and returns the module and the types of the synthetic
/// locals of `main`.
fn synthetic_locals(source: &str) -> (hir::Module, Vec<crate::types::Type>) {
    fn collect(body: &[hir::Stmt], out: &mut Vec<crate::types::Type>) {
        for statement in body {
            if let hir::Stmt::Let { name, ty, .. } = statement {
                if name.starts_with("[[compound#") {
                    out.push(ty.clone());
                }
            }
            for child in statement.children() {
                if let hir::HirChild::Stmt(child) = child {
                    collect(std::slice::from_ref(child), out);
                }
            }
        }
    }
    let module = check_program(&[SourceFile::new("test.ts", format!("{BOX}{source}"))])
        .expect("the projection program must check");
    let mut out = Vec::new();
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    collect(&main.body, &mut out);
    (module, out)
}

#[test]
fn value_projection_receiver_is_never_copied() {
    let classes = "@ValueType\nclass P { on: boolean = false; }\nclass H { p: P = new P(); flags: FixedArray<boolean, 2> = [false, false]; }\nconst G: H = new H();\nfunction getH(): H { return G; }\nfunction at(i: i32): i32 { return i; }\n";
    for body in [
        "const grid: P[] = [new P()]; grid[at(0)].on ||= true;",
        "const grid: P[] = [new P()]; const v: boolean = (grid[at(0)].on ||= true);",
        "getH().p.on ||= true;",
        "const v: boolean = (getH().p.on &&= true);",
        "getH().flags[at(1)] ||= true;",
        "const v: boolean = (getH().flags[at(1)] ||= true);",
    ] {
        let (module, locals) = synthetic_locals(&format!(
            "{classes}export function main(): void {{ {body} }}\n"
        ));
        // Control: the non-place operand is bound, so the check below sees it.
        assert!(
            !locals.is_empty(),
            "{body}: a non-place operand must be bound"
        );
        // The Debug text names the type, so this test reads no type shape.
        let value_classes: Vec<String> = module
            .classes
            .iter()
            .enumerate()
            .filter(|(_, class)| class.is_value)
            .map(|(index, _)| format!("Class(ClassId({index}))"))
            .collect();
        for ty in &locals {
            let text = format!("{ty:?}");
            let copied = text.starts_with("FixedArray(") || value_classes.contains(&text);
            assert!(!copied, "{body}: a value projection is copied: {ty:?}");
        }
    }
}
