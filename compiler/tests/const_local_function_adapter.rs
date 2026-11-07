//! A const local function value takes an adapter that captures it
//! (compiler.md §246).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

const DECLARATIONS: &str = r#"
type CB1 = (x: i32) => void;
type CB2 = (x: i32, y: i32) => void;
const G: CB1 = (x: i32): void => {};
class H { f: CB2; g: CB1; constructor(f: CB2, g: CB1) { this.f = f; this.g = g; } }
function run(cb: CB2): void { cb(1, 2); }
function both(x: i32, y: i32): void {}
function mk(): CB1 { return G; }
function pick(b: boolean): CB1 | null { return b ? G : null; }
"#;

const RULE5: &str = "needs an adapter for this place, and only a const local or a module `const` takes one; copy it into a const local, or write a lambda that calls it";

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

/// The initializer of the `let` named `name` in `f`.
fn initializer<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Expr {
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    f.body
        .iter()
        .find_map(|statement| match statement {
            hir::Stmt::Let {
                name: local, init, ..
            } if local == name => Some(init),
            _ => None,
        })
        .expect("the binding")
}

const FV: &str = "const k: i32 = 10; const fv = (x: i32): void => { print(`${x + k}`); };";

/// Rule 1: a const local takes the adapter in each place: an
/// initializer, an argument, a `void` result that discards the value, and
/// a built-in callback. The adapter captures the local by value.
#[test]
fn a_const_local_takes_an_adapter_that_captures_it() {
    for body in [
        format!("function f(): void {{ {FV} const h: CB2 = fv; h(1, 2); }}"),
        format!("function f(): void {{ {FV} run(fv); }}"),
        "function f(): void { const fr = (x: i32): i32 => x * 2; const h: CB1 = fr; h(1); }"
            .to_string(),
        format!("function f(m: Map<string, i32>): void {{ {FV} m.forEach(fv); }}"),
        "async function step(): Promise<i32> { return 0; }\nasync function f(): Promise<void> { const fa = (x: i32): void => {}; await step(); run(fa); }"
            .to_string(),
    ] {
        accepts(&body);
    }
    let module = accepts(&format!("function f(): void {{ {FV} const h: CB2 = fv; }}"));
    let init = initializer(&module, "h");
    assert_eq!(init.ty, Type::func(vec![Type::I32, Type::I32], Type::Void));
    let hir::ExprKind::Lambda {
        params,
        captures,
        body,
        ..
    } = &init.kind
    else {
        panic!("{init:?}");
    };
    assert_eq!(params.len(), 2);
    assert!(
        matches!(captures.as_slice(), [hir::Capture { name, .. }] if name == "fv"),
        "{captures:?}"
    );
    let [hir::Stmt::Expr(hir::Expr {
        kind: hir::ExprKind::Call { callee, args },
        ..
    })] = body.as_slice()
    else {
        panic!("{body:?}");
    };
    assert!(
        matches!(callee, hir::Callee::Value(value)
            if matches!(&value.kind, hir::ExprKind::Local(name, ..) if name == "fv")),
        "{callee:?}"
    );
    assert_eq!(args.len(), 1);
}

/// Rule 2: a module `const` takes the same adapter, and it captures
/// nothing.
#[test]
fn a_module_const_takes_an_adapter_with_no_capture() {
    accepts("function f(): void { run(G); }");
    let module = accepts("function f(): void { const h: CB2 = G; }");
    let hir::ExprKind::Lambda { captures, body, .. } = &initializer(&module, "h").kind else {
        panic!("a lambda");
    };
    assert!(captures.is_empty(), "{captures:?}");
    assert!(
        matches!(body.as_slice(), [hir::Stmt::Expr(hir::Expr {
            kind: hir::ExprKind::Call { callee: hir::Callee::Value(value), .. }, ..
        })] if matches!(value.kind, hir::ExprKind::Global(_))),
        "{body:?}"
    );
}

/// Rule 3: each escape that S009 rejects for the lambda written in the
/// source is rejected for the adapter, with the same diagnostic.
#[test]
fn the_adapter_escapes_as_a_written_lambda_does() {
    for (adapter, written) in [
        (
            format!("function f(): CB2 {{ {FV} return fv; }}"),
            format!("function f(): CB2 {{ {FV} return (x: i32, y: i32): void => fv(x); }}"),
        ),
        (
            format!("function f(h: H): void {{ {FV} h.f = fv; }}"),
            format!("function f(h: H): void {{ {FV} h.f = (x: i32, y: i32): void => fv(x); }}"),
        ),
    ] {
        let adapter_diagnostic = rejects(&adapter);
        let written_diagnostic = rejects(&written);
        assert_eq!(adapter_diagnostic.code, RuleCode::S009, "{adapter}");
        assert_eq!(
            adapter_diagnostic.message, written_diagnostic.message,
            "{adapter}"
        );
    }
    for (adapter, written) in [
        (
            format!("function f(): CB2[] {{ {FV} return [fv]; }}"),
            format!("function f(): CB2[] {{ {FV} return [(x: i32, y: i32): void => fv(x)]; }}"),
        ),
        (
            format!("function f(): H {{ {FV} return new H(fv, G); }}"),
            format!("function f(): H {{ {FV} return new H((x: i32, y: i32): void => fv(x), G); }}"),
        ),
    ] {
        let adapter_errors = check(&adapter).expect_err(&adapter);
        let written_errors = check(&written).expect_err(&written);
        let messages = |errors: &[Diagnostic]| -> Vec<String> {
            errors.iter().map(|error| error.message.clone()).collect()
        };
        assert!(
            adapter_errors
                .iter()
                .all(|error| error.code == RuleCode::S009),
            "{adapter}: {adapter_errors:?}"
        );
        assert_eq!(
            messages(&adapter_errors),
            messages(&written_errors),
            "{adapter}"
        );
    }
}

/// Rules 4 and 5: a `let` local, a parameter, a field read, an element
/// read, and a call result take no adapter, and the diagnostic names the
/// cause. An async function value and a function with a default parameter
/// keep their present diagnostic.
#[test]
fn another_value_names_the_cause() {
    for (body, name) in [
        (
            "function f(): void { let lv = (x: i32): void => {}; run(lv); }",
            Some("lv"),
        ),
        ("function f(pv: CB1): void { run(pv); }", Some("pv")),
        ("function f(h: H): void { run(h.g); }", None),
        ("function f(cs: CB1[]): void { run(cs[0]); }", None),
        ("function f(): void { run(mk()); }", None),
        (
            "function f(pv: CB1, m: Map<string, i32>): void { m.forEach(pv); }",
            Some("pv"),
        ),
    ] {
        let diagnostic = rejects(body);
        let subject = match name {
            Some(name) => format!("`{name}` "),
            None => String::new(),
        };
        assert!(
            diagnostic.message.ends_with(&format!("{subject}{RULE5}")),
            "{body}: {diagnostic:?}"
        );
    }
    for (body, message) in [
        (
            "function f(): void { const fa = async (x: i32): Promise<void> => {}; run(fa); }",
            "type mismatch: the argument expects `(i32, i32) => void`, got `(i32) => Promise<void>`",
        ),
        (
            "function f(): void { const fd = (x: i32, y: i32 = 1): void => {}; const h: (x: i32, y: i32, z: i32) => void = fd; }",
            "type mismatch: the initializer expects `(i32, i32, i32) => void`, got `(i32, i32) => void`",
        ),
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
}

/// Rule 6: a leaf of a conditional that is a const local takes the
/// adapter, in a user place and in a built-in callback. The control: a
/// `let` local leaf names the cause.
#[test]
fn a_conditional_leaf_takes_the_adapter() {
    for body in [
        format!("function f(b: boolean): void {{ {FV} run(b ? fv : both); }}"),
        format!("function f(b: boolean): void {{ {FV} [1, 2].forEach(b ? fv : both); }}"),
    ] {
        accepts(&body);
    }
    let diagnostic = rejects(
        "function f(b: boolean): void { let lv = (x: i32): void => {}; run(b ? lv : both); }",
    );
    assert!(diagnostic.message.ends_with(RULE5), "{diagnostic:?}");
}

/// Rules 1 and 3: a narrowed nullable const local takes the adapter in a
/// user place, a built-in callback, and an initializer. The adapter
/// captures the local with the type that a written lambda captures, its
/// storage type.
#[test]
fn a_narrowed_const_local_takes_the_adapter() {
    for body in [
        "function f(): void { const fv: CB1 | null = pick(true); if (fv !== null) { run(fv); } }",
        "function f(): void { const fv: CB1 | null = pick(true); if (fv !== null) { [1, 2].forEach(fv); } }",
        "function f(): void { const fv: CB1 | null = pick(true); if (fv !== null) { const h: CB2 = fv; h(1, 2); } }",
    ] {
        accepts(body);
    }
    let module = accepts(
        "function f(): void { const fv: CB1 | null = pick(true); if (fv !== null) { const h: CB2 = fv; } }",
    );
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    let [_, hir::Stmt::If { then, .. }] = f.body.as_slice() else {
        panic!("{:?}", f.body);
    };
    let [hir::Stmt::Let { init, .. }] = then.as_slice() else {
        panic!("{then:?}");
    };
    let hir::ExprKind::Lambda { captures, .. } = &init.kind else {
        panic!("{init:?}");
    };
    let [capture] = captures.as_slice() else {
        panic!("{captures:?}");
    };
    assert!(matches!(capture.ty, Type::Nullable(..)), "{capture:?}");
}

/// Rule 5 holds only when rule 1 would take the value as a const local
/// name. A nullable value that is not narrowed fits no adapter, so it gets
/// the type mismatch of the place, as a parameter or as a const local.
#[test]
fn a_nullable_value_gets_the_type_mismatch() {
    for body in [
        "function f(p: CB1 | null): void { run(p); }",
        "function f(): void { const fv: CB1 | null = pick(true); run(fv); }",
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, RuleCode::S100, "{body}: {diagnostic:?}");
        assert_eq!(
            diagnostic.message,
            "type mismatch: the argument expects `(i32, i32) => void`, got `((i32) => void) | null`",
            "{body}"
        );
    }
}
