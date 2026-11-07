//! An async return adopts a handle (compiler.md §236).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, SourceFile};

const DECLARATIONS: &str = r#"
type Mode = "a" | "b";
class C { v: i32 = 1; }
async function v(): Promise<i32> { return 1; }
async function c(): Promise<C> { return new C(); }
"#;

fn check(body: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    let source = format!("{DECLARATIONS}{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())])
}

fn accepts(body: &str) -> hir::Module {
    check(body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"))
}

/// Rules 1 and 2: a returned handle is held, one turn passes, and the
/// return reads the `await` of the handle.
#[test]
fn an_async_return_adopts_a_handle() {
    for body in [
        "async function h(): Promise<i32> { return v(); }",
        "async function h(): Promise<C | null> { return c(); }",
        "async function h(p: Promise<i32>): Promise<i32> { const q: Promise<i32> = p; return q; }",
        "function f(): void { const job = async (p: Promise<i32>): Promise<i32> => p; }",
        // The controls: a value return and the explicit await.
        "async function h(): Promise<i32> { return 3; }",
        "async function h(): Promise<i32> { return await v(); }",
    ] {
        accepts(body);
    }
    let module = accepts("async function h(): Promise<i32> { return v(); }");
    let h = module
        .functions
        .iter()
        .find(|function| function.name == "h")
        .expect("h");
    // The return holds the handle, writes the state, and returns; the
    // epilogue's `finally` waits one turn and returns the `await`.
    let body = guarded_body(&h.body);
    assert!(
        matches!(
            body,
            [
                hir::Stmt::Expr(hir::Expr {
                    kind: hir::ExprKind::Assign { .. },
                    ..
                }),
                hir::Stmt::Expr(hir::Expr {
                    kind: hir::ExprKind::Assign { .. },
                    ..
                }),
                hir::Stmt::Return {
                    value: Some(hir::Expr {
                        kind: hir::ExprKind::Zero,
                        ..
                    }),
                    ..
                },
            ]
        ),
        "{body:?}"
    );
    let [.., hir::Stmt::Using {
        finalizer: Some(finalizer),
        ..
    }] = h.body.as_slice()
    else {
        panic!("{:?}", h.body);
    };
    assert!(
        matches!(finalizer.as_slice(), [hir::Stmt::If { then, .. }] if matches!(then.as_slice(), [
            hir::Stmt::Expr(hir::Expr { kind: hir::ExprKind::AsyncCall { .. }, .. }),
            hir::Stmt::Return { value: Some(hir::Expr { kind: hir::ExprKind::AsyncHandleAwait(_), .. }), .. },
        ])),
        "{finalizer:?}"
    );
}

/// The statements of a function body inside its completion state and
/// epilogue: the state and handle locals, then a `try` whose `finally` is
/// the epilogue, around a `catch` that clears the state and throws again.
fn guarded_body(body: &[hir::Stmt]) -> &[hir::Stmt] {
    let [hir::Stmt::Let { name: state, .. }, hir::Stmt::Let { name: handle, .. }, hir::Stmt::Using {
        body,
        finalizer: Some(_),
        ..
    }] = body
    else {
        panic!("{body:?}");
    };
    assert!(state.ends_with(".state]]"), "{state}");
    assert!(handle.starts_with("[[adopt#"), "{handle}");
    let [hir::Stmt::Try {
        body,
        handler,
        binding: Some(_),
        ..
    }] = body.as_slice()
    else {
        panic!("{body:?}");
    };
    assert!(
        matches!(
            handler.as_slice(),
            [hir::Stmt::Expr(_), hir::Stmt::Throw { .. }]
        ),
        "{handler:?}"
    );
    body
}

/// Rule 3: the operand takes the context `Promise<T>`, so a `then`
/// callback takes `T` as the context of its result.
#[test]
fn a_returned_reaction_takes_the_context_of_the_result() {
    accepts(r#"async function f(): Promise<Mode> { return v().then((x: i32) => "a"); }"#);
}

/// Rule 4: a return of a handle adopts it in each part of a `try`
/// statement; the epilogue runs outside each handler (rule 2).
#[test]
fn a_returned_handle_in_a_try_statement_adopts() {
    for body in [
        "async function h(): Promise<i32> { try { return v(); } finally { print(\"f\"); } }",
        "async function h(): Promise<i32> { try { return 1; } catch (e) { return v(); } }",
        "async function h(): Promise<i32> { try { print(\"t\"); } finally { return v(); } }",
    ] {
        let module = accepts(body);
        let h = module
            .functions
            .iter()
            .find(|function| function.name == "h")
            .expect("h");
        // The epilogue: the state and handle locals, then a `try` whose
        // `finally` reads the state.
        guarded_body(&h.body);
    }
}

/// Rule 3: each leaf of a conditional takes the context of its kind, and a
/// return that mixes a handle leaf and a value leaf is one return per leaf.
#[test]
fn the_leaves_of_a_returned_conditional_decide_its_context() {
    for body in [
        "async function h(b: boolean): Promise<i32> { return b ? v() : v(); }",
        "async function h(b: boolean, p: Promise<i32>, q: Promise<i32>): Promise<i32> { return b ? p : q; }",
        "function f(): void { const job = async (b: boolean): Promise<i32> => b ? v() : v(); }",
        "async function h(b: boolean): Promise<i32> { return b ? 1 : 2; }",
    ] {
        accepts(body);
    }
    // A mixed return is one return per leaf: the value leaf returns, and
    // the handle leaf adopts through the epilogue.
    for body in [
        "async function h(b: boolean): Promise<i32> { return b ? v() : 3; }",
        "async function h(b: boolean, c: boolean): Promise<i32> { return b ? 1 : (c ? v() : 2); }",
        "function f(): void { const job = async (b: boolean): Promise<i32> => b ? 3 : v(); }",
        // An unannotated body takes the value type of its handle leaf.
        "function f(): void { const job = async (b: boolean) => b ? 3 : v(); }",
        // A field of a `C | null` receiver is a handle leaf.
        "class Box { h: Promise<i32>; constructor(h: Promise<i32>) { this.h = h; } }\nasync function h(x: Box | null): Promise<i32> { return x !== null ? x.h : 4; }",
    ] {
        accepts(body);
    }
    let module = accepts("async function h(b: boolean): Promise<i32> { return b ? v() : 3; }");
    let h = module
        .functions
        .iter()
        .find(|function| function.name == "h")
        .expect("h");
    let body = guarded_body(&h.body);
    // The condition runs once; the then branch adopts, the else branch
    // returns the value.
    let [hir::Stmt::If {
        then,
        els: Some(els),
        ..
    }] = body
    else {
        panic!("{body:?}");
    };
    assert!(
        matches!(
            then.as_slice(),
            [
                hir::Stmt::Expr(_),
                hir::Stmt::Expr(_),
                hir::Stmt::Return {
                    value: Some(hir::Expr {
                        kind: hir::ExprKind::Zero,
                        ..
                    }),
                    ..
                },
            ]
        ),
        "{then:?}"
    );
    assert!(
        matches!(
            els.as_slice(),
            [
                hir::Stmt::Expr(_),
                hir::Stmt::Return {
                    value: Some(hir::Expr {
                        kind: hir::ExprKind::Int(3),
                        ..
                    }),
                    ..
                }
            ]
        ),
        "{els:?}"
    );
}

/// Rule 2a: a value return clears the completion state before it returns,
/// so a later completion replaces a pending adoption. An adopted return
/// writes the state; a second handle type gets its own handle local.
#[test]
fn the_last_completion_of_the_function_wins() {
    let module =
        accepts("async function h(): Promise<i32> { try { return v(); } finally { return 9; } }");
    let h = module
        .functions
        .iter()
        .find(|function| function.name == "h")
        .expect("h");
    let [hir::Stmt::Let { name: state, .. }, ..] = h.body.as_slice() else {
        panic!("{:?}", h.body);
    };
    let [hir::Stmt::Using {
        finalizer: Some(finalizer),
        ..
    }] = guarded_body(&h.body)
    else {
        panic!("{:?}", h.body);
    };
    // `return 9` writes the state 0 first.
    assert!(
        matches!(finalizer.as_slice(), [
            hir::Stmt::Expr(hir::Expr { kind: hir::ExprKind::Assign { target, value, .. }, .. }),
            hir::Stmt::Return { .. },
        ] if matches!(&target.kind, hir::ExprKind::Local(name, ..) if name == state)
            && matches!(value.kind, hir::ExprKind::Int(0))),
        "{finalizer:?}"
    );
    // Two handle types: one state, two handle locals, one turn.
    let module = accepts(
        "async function h(b: boolean, n: Promise<C | null>): Promise<C | null> { if (b) { return c(); } return n; }",
    );
    let h = module
        .functions
        .iter()
        .find(|function| function.name == "h")
        .expect("h");
    assert!(
        matches!(
            h.body.as_slice(),
            [
                hir::Stmt::Let { .. },
                hir::Stmt::Let { .. },
                hir::Stmt::Let { .. },
                hir::Stmt::Using { .. }
            ]
        ),
        "{:?}",
        h.body
    );
}

/// Rule 5: the return reads the handle, so S013 does not fire for it; a
/// handle that no return reads still fires.
#[test]
fn the_returned_handle_is_read() {
    accepts("async function h(): Promise<i32> { return v(); }");
    let errors =
        check("async function h(): Promise<i32> { v(); return 1; }").expect_err("floating");
    assert_eq!(
        errors[0].code,
        subscript_compiler::RuleCode::S013,
        "{errors:?}"
    );
}
