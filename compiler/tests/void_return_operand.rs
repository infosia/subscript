//! A `void` function returns a `void` operand (compiler.md §237).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile};

const DECLARATIONS: &str = r#"
const ys: i32[] = [];
function v(): void {}
function g(x: i32): void {}
async function work(): Promise<void> {}
async function count(): Promise<i32> { return 1; }
const flag: boolean = true;
class S {
  private q: i32 = 0;
  get p(): i32 { return this.q; }
  set p(x: i32) { this.q = x; }
  get(): i32 { return 1; }
  m(): void {}
  async am(): Promise<void> {}
}
function run(f: (x: i32) => Promise<void>): void {}
"#;

fn check(body: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    let source = format!("{DECLARATIONS}{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())])
}

fn accepts(body: &str) -> hir::Module {
    check(body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"))
}

/// The one diagnostic of a rejected body.
fn rejects(body: &str) -> Diagnostic {
    let errors = check(body).expect_err(body);
    assert_eq!(errors.len(), 1, "{body}: {errors:?}");
    errors.into_iter().next().expect("one diagnostic")
}

fn function<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Function {
    module
        .functions
        .iter()
        .find(|function| function.name == name)
        .expect("function")
}

/// Rule 2: `return e` with a `void` operand is `e; return;`. The control
/// in the same shape, a value operand of a declared `void` result, is
/// rejected (rule 5).
#[test]
fn a_void_operand_is_a_statement_then_a_bare_return() {
    for body in [
        "function f(): void { return v(); }",
        "const f = (): void => v();",
        "function f(): void { ys.forEach((x: i32) => g(x)); }",
        "async function f(): Promise<void> { return await work(); }",
    ] {
        accepts(body);
    }
    let module = accepts("function f(): void { return v(); }");
    let f = function(&module, "f");
    assert!(
        matches!(
            f.body.as_slice(),
            [
                hir::Stmt::Expr(hir::Expr {
                    kind: hir::ExprKind::Call { .. },
                    ty: subscript_compiler::Type::Void,
                    ..
                }),
                hir::Stmt::Return { value: None, .. },
            ]
        ),
        "{:?}",
        f.body
    );
    let control = rejects("function f(): void { return 3; }");
    assert_eq!(control.code, RuleCode::S100);
    assert_eq!(control.message, "a `void` function cannot return a value");
}

/// Rule 3: an async `Promise<void>` result adopts a returned
/// `Promise<void>` handle; a mixed leaf splits.
#[test]
fn a_promise_void_result_adopts_a_handle() {
    for body in [
        "async function f(): Promise<void> { return work(); }",
        "async function f(b: boolean): Promise<void> { return b ? work() : v(); }",
        "async function f(): Promise<void> { try { return work(); } finally { v(); } }",
        "function h(): void { const job = async (): Promise<void> => work(); }",
    ] {
        accepts(body);
    }
    let module = accepts("async function f(): Promise<void> { return work(); }");
    let f = function(&module, "f");
    let [hir::Stmt::Let { name: state, .. }, hir::Stmt::Let { .. }, hir::Stmt::Using {
        finalizer: Some(finalizer),
        ..
    }] = f.body.as_slice()
    else {
        panic!("{:?}", f.body);
    };
    assert!(state.ends_with(".state]]"), "{state}");
    // The epilogue awaits the handle as a statement, then returns with no
    // value.
    assert!(
        matches!(finalizer.as_slice(), [hir::Stmt::If { then, .. }] if matches!(then.as_slice(), [
            hir::Stmt::Expr(hir::Expr { kind: hir::ExprKind::AsyncCall { .. }, .. }),
            hir::Stmt::Expr(hir::Expr { kind: hir::ExprKind::AsyncHandleAwait(_), .. }),
            hir::Stmt::Return { value: None, .. },
        ])),
        "{finalizer:?}"
    );
    // A handle with a value is not a `Promise<void>`: the control.
    let control = rejects("async function f(): Promise<void> { return count(); }");
    assert_eq!(control.message, "a `void` function cannot return a value");
}

/// Rule 4: a lambda whose result is `void` from its context discards a
/// value operand, in an expression body or a block body. A dropped handle
/// gives S013, as an expression statement does.
#[test]
fn a_contextual_void_lambda_discards_a_value() {
    for body in [
        "function f(): void { ys.forEach((x: i32) => ys.push(x)); }",
        "function f(): void { const cb: (x: i32) => void = (x: i32) => ys.push(x); }",
        "function f(): void { const cb: (x: i32) => void = (x: i32) => { return ys.push(x); }; }",
    ] {
        accepts(body);
    }
    let dropped = rejects("function f(): void { const cb: () => void = () => work(); }");
    assert_eq!(dropped.code, RuleCode::S013, "{dropped:?}");
}

/// Rule 5: a declared `void` or `Promise<void>` result keeps the S100 for
/// a value operand.
#[test]
fn a_declared_void_result_rejects_a_value() {
    for body in [
        "function f(): void { return 3; }",
        "const f = (): void => 3;",
        "async function f(): Promise<void> { return 3; }",
        "function f(): void { const cb: (x: i32) => void = (x: i32): void => ys.push(x); }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(
            diagnostic.message, "a `void` function cannot return a value",
            "{body}"
        );
    }
}

/// Rules 2 and 5: the type of the operand is its type as an expression. An
/// assignment has the type of its value, and an optional chain has the type
/// of its member or its call. The control: an optional `void` call is
/// accepted.
#[test]
fn the_operand_type_is_its_type_as_an_expression() {
    for (body, message) in [
        (
            "function f(): void { return ys.length = 0; }",
            "only `xs.length = 0` as a statement is accepted; use `splice` or `pop` to remove elements",
        ),
        (
            "const f = (): void => ys.length = 0;",
            "only `xs.length = 0` as a statement is accepted; use `splice` or `pop` to remove elements",
        ),
        (
            "function f(s: S): void { return s.p = 3; }",
            "`x.p = v` cannot be used as a value",
        ),
        (
            "function f(s: S | null): void { return s?.get(); }",
            "a `void` function cannot return a value",
        ),
        (
            "function f(a: i32[] | null): void { return a?.push(1); }",
            "a `void` function cannot return a value",
        ),
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
    accepts("function f(c: S | null): void { return c?.m(); }");
}

/// Rules 4 and 5: an async lambda whose result takes `Promise<void>` from
/// its context keeps the S100 for a value operand. The control: the same
/// lambda adopts a `Promise<void>` handle.
#[test]
fn an_async_contextual_lambda_rejects_a_value() {
    for body in [
        "const cb: () => Promise<void> = async () => 3;",
        "const cb: () => Promise<void> = async () => { return 3; };",
        "function h(): void { run(async (x: i32) => ys.push(x)); }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(
            diagnostic.message, "a `void` function cannot return a value",
            "{body}"
        );
    }
    accepts("const cb: () => Promise<void> = async () => work();");
}

/// Rule 2: a conditional whose leaves are each `void` splits into one
/// return per leaf, in a declared `void` result, an async `Promise<void>`
/// result, and a contextual `void` lambda.
#[test]
fn an_all_void_conditional_splits() {
    for body in [
        "function f(b: boolean): void { return b ? v() : v(); }",
        "async function f(b: boolean): Promise<void> { return b ? v() : v(); }",
        "const cb: () => void = () => flag ? v() : v();",
    ] {
        accepts(body);
    }
    let module = accepts("function f(b: boolean): void { return b ? v() : v(); }");
    let f = function(&module, "f");
    assert!(
        matches!(f.body.as_slice(), [hir::Stmt::If { then, els: Some(els), .. }]
            if matches!(then.as_slice(), [hir::Stmt::Expr(_), hir::Stmt::Return { value: None, .. }])
                && matches!(els.as_slice(), [hir::Stmt::Expr(_), hir::Stmt::Return { value: None, .. }])),
        "{:?}",
        f.body
    );
}

/// Rule 1: a constructor and a setter keep the S100 of `return e`. The
/// control: a `void` method, `[Symbol.dispose]`, accepts a `void` operand.
#[test]
fn a_constructor_and_a_setter_keep_the_rejection() {
    for body in [
        "class T { private q: i32 = 0; get p(): i32 { return this.q; } set p(x: i32) { this.q = x; return v(); } }",
        "class T { q: i32; constructor() { this.q = 1; return v(); } }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(
            diagnostic.message,
            "a `void` expression is only allowed as an expression statement",
            "{body}"
        );
    }
    accepts("class T { [Symbol.dispose](): void { return v(); } }");
}

/// Rule 2: `(e)` has the type of `e`, as an operand and as a leaf.
#[test]
fn a_parenthesized_operand_has_the_type_of_its_expression() {
    for body in [
        "const f = (): void => (v());",
        "function f(): void { return (v()); }",
        "function f(b: boolean): void { return b ? (v()) : g(1); }",
        "async function f(): Promise<void> { return (await work()); }",
    ] {
        accepts(body);
    }
}

/// Rule 5: a rejected return gives one diagnostic. An optional async call
/// is a `Promise<void> | undefined`, and the return reads its handle, so
/// S013 does not fire (§237.4 item 4). The leaves of a split conditional
/// give one S100, at the return.
#[test]
fn a_rejected_return_gives_one_diagnostic() {
    for body in [
        "function f(s: S | null): void { return s?.am(); }",
        "async function f(s: S | null): Promise<void> { return s?.am(); }",
        "function f(b: boolean): void { return b ? 1 : 2; }",
        "function f(b: boolean, c: boolean): void { return b ? 1 : (c ? 2 : v()); }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(
            diagnostic.message, "a `void` function cannot return a value",
            "{body}"
        );
    }
    // The split diagnostic is at the `return`, not at a leaf.
    let diagnostic = rejects("function f(b: boolean): void {\n  return b ? 1 : 2;\n}");
    assert_eq!(diagnostic.pos.col, 3, "{diagnostic:?}");
}

/// Rule 6: `yield e` with a `void` operand stays rejected.
#[test]
fn a_void_yield_operand_stays_rejected() {
    let diagnostic = rejects("function* gen(): Generator<void> { yield v(); }");
    assert_eq!(diagnostic.code, RuleCode::S100);
    assert_eq!(
        diagnostic.message,
        "a `void` expression is only allowed as an expression statement"
    );
}
