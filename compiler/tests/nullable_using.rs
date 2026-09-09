//! Section 97: nullable disposal uses existing HIR forms.

use subscript_compiler::{check_program, hir, RuleCode, SourceFile, Type};

fn check(source: &str) -> hir::Module {
    check_program(&[SourceFile::new("nullable-using.ts", source)]).expect("accepted control")
}

fn main_body(module: &hir::Module) -> &[hir::Stmt] {
    &module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main")
        .body
}

fn disposal_receiver(statement: &hir::Stmt) -> &hir::Expr {
    let hir::Stmt::Expr(hir::Expr {
        kind:
            hir::ExprKind::Call {
                callee: hir::Callee::Method { recv, name },
                args,
            },
        ty: Type::Void,
        ..
    }) = statement
    else {
        panic!("bare disposal call")
    };
    assert_eq!(name, hir::DISPOSE_METHOD_NAME);
    assert!(args.is_empty());
    recv
}

fn null_guard(statement: &hir::Stmt) -> (&hir::Expr, &hir::Expr) {
    let hir::Stmt::If {
        cond,
        then,
        els: None,
        ..
    } = statement
    else {
        panic!("null guard")
    };
    assert_eq!(cond.ty, Type::Bool);
    let hir::ExprKind::Binary {
        op: hir::BinOp::Ne,
        left,
        right,
    } = &cond.kind
    else {
        panic!("strict null inequality")
    };
    assert_eq!(right.kind, hir::ExprKind::Null);
    assert_eq!(right.ty, Type::Null);
    let Type::Nullable(inner) = &left.ty else {
        panic!("nullable comparison operand")
    };
    assert!(matches!(inner.as_ref(), Type::Class(_)));
    assert_eq!(then.len(), 1);
    let recv = disposal_receiver(&then[0]);
    assert_eq!(&recv.ty, inner.as_ref());
    assert_eq!(recv.kind, left.kind);
    (left, recv)
}

#[test]
fn nullable_using_has_null_guard_and_non_null_receiver() {
    let module = check(
        "class R { [Symbol.dispose](): void {} }
        export function main(): void { using resource: R | null = null; }",
    );
    let body = main_body(&module);
    assert_eq!(body.len(), 2);
    assert!(matches!(
        &body[0],
        hir::Stmt::Let {
            ty: Type::Nullable(_),
            mutable: false,
            ..
        }
    ));
    let (left, _) = null_guard(&body[1]);
    assert_eq!(left.kind, hir::ExprKind::Local("resource".into()));
}

#[test]
fn non_nullable_using_keeps_bare_call() {
    let module = check(
        "class R { [Symbol.dispose](): void {} }
        export function main(): void { using resource = new R(); }",
    );
    let body = main_body(&module);
    assert_eq!(body.len(), 2);
    let recv = disposal_receiver(&body[1]);
    assert!(matches!(recv.ty, Type::Class(_)));
    assert_eq!(recv.kind, hir::ExprKind::Local("resource".into()));
}

// Every storage read must occur inside the corresponding active flag's true arm.
fn check_storage_reads(statement: &hir::Stmt, active: &[String]) -> usize {
    fn expression(expr: &hir::Expr, active: &[String]) -> usize {
        if let hir::ExprKind::Assign { target, value, .. } = &expr.kind {
            assert!(matches!(target.kind, hir::ExprKind::Local(_)));
            return expression(value, active);
        }
        let mut reads = 0;
        if let hir::ExprKind::Local(name) = &expr.kind {
            if let Some(id) = name.strip_prefix("[[using.value#") {
                assert!(active.contains(&format!("[[using.active#{id}")));
                reads += 1;
            }
        }
        for child in expr.children() {
            reads += match child {
                hir::HirChild::Expr(expr) => expression(expr, active),
                hir::HirChild::Stmt(stmt) => check_storage_reads(stmt, active),
            };
        }
        reads
    }
    if let hir::Stmt::If {
        cond, then, els, ..
    } = statement
    {
        if let hir::ExprKind::Local(flag) = &cond.kind {
            if flag.starts_with("[[using.active#") {
                assert_eq!(then.len(), 1);
                null_guard(&then[0]);
                assert!(els.is_none());
                let mut inner = active.to_vec();
                inner.push(flag.clone());
                return then
                    .iter()
                    .map(|stmt| check_storage_reads(stmt, &inner))
                    .sum();
            }
        }
    }
    statement
        .children()
        .into_iter()
        .map(|child| match child {
            hir::HirChild::Expr(expr) => expression(expr, active),
            hir::HirChild::Stmt(stmt) => check_storage_reads(stmt, active),
        })
        .sum()
}

#[test]
fn skipped_switch_declaration_reads_no_disposal_storage() {
    let module = check(
        "class R { [Symbol.dispose](): void {} }
        function make(): R | null { return new R(); }
        export function main(): void {
          const selected: i32 = 1;
          switch (selected) {
            case 0: using skipped = make();
            case 1: using live = make(); break;
            default: break;
          }
        }",
    );
    let body = main_body(&module);
    let mut flags = 0;
    for statement in body {
        if let hir::Stmt::Let { name, init, .. } = statement {
            if name.starts_with("[[using.active#") {
                assert_eq!(init.kind, hir::ExprKind::Bool(false));
                flags += 1;
            }
        }
    }
    assert_eq!(flags, 2);
    let reads: usize = body.iter().map(|stmt| check_storage_reads(stmt, &[])).sum();
    assert!(
        reads >= 4,
        "each disposal has a guarded comparison and call read"
    );
}

#[test]
fn retained_rejections_have_positive_controls() {
    let cases = [
        ("class R {} export function main(): void { using r = new R(); }",
         "class R { [Symbol.dispose](): void {} } export function main(): void { using r = new R(); }",
         RuleCode::S100, "the class of a `using` binding must declare `[Symbol.dispose](): void`"),
        ("class R {} function make(): R | null { return null; } export function main(): void { using r = make(); }",
         "class R { [Symbol.dispose](): void {} } function make(): R | null { return null; } export function main(): void { using r = make(); }",
         RuleCode::S100, "the class of a `using` binding must declare `[Symbol.dispose](): void`"),
        ("class R { [Symbol.dispose](): void {} } export async function main(): Promise<void> { await using r = new R(); }",
         "class R { [Symbol.dispose](): void {} } export async function main(): Promise<void> { using r = new R(); }",
         RuleCode::S100, "`await using` is not in the decided surface"),
        ("@CStruct class R { [Symbol.dispose](): void {} } export function main(): void {}",
         "class R { [Symbol.dispose](): void {} } export function main(): void { using r = new R(); }",
         RuleCode::S100, "value classes cannot declare `[Symbol.dispose]()`"),
        ("@Descriptor class R { [Symbol.dispose](): void {} } export function main(): void {}",
         "class R { [Symbol.dispose](): void {} } export function main(): void { using r = new R(); }",
         RuleCode::S100, "descriptor classes cannot declare `[Symbol.dispose]()`"),
        ("class R { label: string = \"r\"; [Symbol.dispose](): void {} } function make(): R | null { return null; } export function main(): void { using r = make(); print(r.label); }",
         "class R { label: string = \"r\"; [Symbol.dispose](): void {} } function make(): R | null { return null; } export function main(): void { using r = make(); if (r !== null) { print(r.label); } }",
         RuleCode::S011, "`R | null` may be null here; narrow with a null check first"),
        ("class R { [Symbol.dispose](): void {} } export function main(): void { using r = null; }",
         "class R { [Symbol.dispose](): void {} } export function main(): void { using r: R | null = null; }",
         RuleCode::S100, "cannot infer a type from `null`; annotate the declaration"),
    ];
    for (rejected, accepted, code, message) in cases {
        let diagnostics = check_program(&[SourceFile::new("reject.ts", rejected)])
            .expect_err("retained rejection");
        assert_eq!(diagnostics[0].code, code);
        assert_eq!(diagnostics[0].message, message);
        check(accepted);
    }
}
