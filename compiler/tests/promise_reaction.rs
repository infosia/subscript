//! Checker facts of `then`, `catch`, and `finally` on a handle (compiler.md §186).
use std::path::PathBuf;

use subscript_compiler::{
    check_program,
    hir::{self, ExprKind, HirChild},
    RuleCode, SourceFile,
};

fn reject_entry(name: &str) -> Vec<subscript_compiler::Diagnostic> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../corpus/reject")
        .join(name);
    let source = std::fs::read_to_string(&path).expect("read the reject entry");
    check_program(&[SourceFile::new(name, source)]).expect_err("the entry is rejected")
}

/// The reject entries of §186.2 acceptance 1 fail with their rule messages.
// Cost: three file reads and three checker calls; no native build.
#[test]
fn reject_entries_report_their_rule_messages() {
    for (entry, code, message) in [
        (
            "r402-dropped-then-handle.ts",
            RuleCode::S013,
            "an async handle is dropped without any await of its completion",
        ),
        (
            "r403-then-captures-let.ts",
            RuleCode::S009,
            "lambda captures `n`, which is not a `const` local; capturing lambdas may capture only const locals by value",
        ),
        (
            "r404-catch-callback-type.ts",
            RuleCode::S013,
            "a `catch` callback must return `i32`, the value type of the handle; it returns `string`",
        ),
    ] {
        let diagnostics = reject_entry(entry);
        assert_eq!(
            (diagnostics[0].code, diagnostics[0].message.as_str()),
            (code, message),
            "{entry}"
        );
    }
}

fn lambdas<'a>(expr: &'a hir::Expr, out: &mut Vec<&'a hir::Expr>) {
    if matches!(expr.kind, ExprKind::Lambda { .. }) {
        out.push(expr);
    }
    for child in expr.children() {
        child_lambdas(child, out);
    }
}

fn child_lambdas<'a>(child: HirChild<'a>, out: &mut Vec<&'a hir::Expr>) {
    match child {
        HirChild::Expr(expr) => lambdas(expr, out),
        HirChild::Stmt(stmt) => {
            for child in stmt.children() {
                child_lambdas(child, out);
            }
        }
    }
}

/// Each lambda of `main` with its `owns_environment` fact.
fn ownership(source: &str) -> Vec<(bool, bool)> {
    let module = check_program(&[SourceFile::new("owned.ts", source)]).expect("checks");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let mut found = Vec::new();
    for statement in &main.body {
        for child in statement.children() {
            child_lambdas(child, &mut found);
        }
    }
    found
        .into_iter()
        .map(|lambda| match &lambda.kind {
            ExprKind::Lambda {
                captures,
                owns_environment,
                ..
            } => (!captures.is_empty(), *owns_environment),
            _ => unreachable!("a lambda"),
        })
        .collect()
}

/// §186 rule 4: only a direct synchronous callback owns its captures. The
/// same lambda held in a local first stays borrowed, and §118 rejects it.
// Cost: two checker calls; no native build.
#[test]
fn a_direct_callback_owns_its_captures_and_a_held_lambda_does_not() {
    let prefix = "async function value(n: i32): Promise<i32> { return n; }\n";
    let direct = format!(
        "{prefix}export async function main(): Promise<void> {{
            const k: i32 = 1;
            print(`${{await value(1).then((v: i32): i32 => v + k)}}`);
            print(`${{await value(1).then((v: i32): i32 => v)}}`);
            print(`${{await value(1).catch((e: Error): i32 => k)}}`);
            print(`${{await value(1).finally((): void => {{ print(`${{k}}`); }})}}`);
        }}"
    );
    assert_eq!(
        ownership(&direct),
        [(true, true), (false, false), (true, true), (true, true)]
    );
    let held = format!(
        "{prefix}export async function main(): Promise<void> {{
            const k: i32 = 1;
            const f = (v: i32): i32 => v + k;
            print(`${{await value(1).then(f)}}`);
        }}"
    );
    let diagnostics =
        check_program(&[SourceFile::new("held.ts", held)]).expect_err("a held capture escapes");
    assert_eq!(diagnostics[0].code, RuleCode::S009);
    assert_eq!(
        diagnostics[0].message,
        "value `f` may capture at held async argument"
    );
}

/// The names of the compiler-supplied helpers of a program.
fn helpers(source: &str) -> Vec<String> {
    let module = check_program(&[SourceFile::new("helpers.ts", source)]).expect("checks");
    let mut names = module
        .functions
        .iter()
        .filter(|function| function.name.starts_with("[[Promise."))
        .map(|function| function.name.clone())
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// §186 rule 2: one helper serves each form, callback shape, and type
/// pair; a different shape or type gets its own helper.
// Cost: two checker calls; no native build.
#[test]
fn calls_with_one_shape_and_type_share_one_helper() {
    let prefix = "async function value(n: i32): Promise<i32> { return n; }
        async function text(): Promise<string> { return \"t\"; }\n";
    let same = format!(
        "{prefix}export async function main(): Promise<void> {{
            print(`${{await value(1).then((v: i32): i32 => v + 1)}}`);
            print(`${{await value(2).then((v: i32): i32 => v * 2)}}`);
        }}"
    );
    assert_eq!(helpers(&same), ["[[Promise.then (v) => value]]<i32, i32>"]);
    let different = format!(
        "{prefix}export async function main(): Promise<void> {{
            print(`${{await value(1).then((v: i32): i32 => v + 1)}}`);
            print(`${{await value(1).then((v: i32): Promise<i32> => value(v))}}`);
            print(`${{await value(1).then((): i32 => 0)}}`);
            print(await text().then((s: string): string => s));
            print(`${{await value(1).catch((e: Error): i32 => 0)}}`);
            print(`${{await value(1).then((v: i32): i32 => v, (e: Error): i32 => 0)}}`);
            print(`${{await value(1).finally((): void => {{ }})}}`);
        }}"
    );
    assert_eq!(
        helpers(&different),
        [
            "[[Promise.catch (e) => value]]<i32, i32>",
            "[[Promise.finally () => void]]<i32, i32>",
            "[[Promise.then () => value]]<i32, i32>",
            "[[Promise.then (v) => handle]]<i32, i32>",
            "[[Promise.then (v) => value, (e) => value]]<i32, i32>",
            "[[Promise.then (v) => value]]<i32, i32>",
            "[[Promise.then (v) => value]]<string, string>",
            "[[Promise.turn]]",
        ]
    );
}

/// §186 rule 5: the receiver type selects the reaction. A class method
/// named `then` is an ordinary method call.
// Cost: one checker call; no native build.
#[test]
fn a_class_then_method_is_an_ordinary_call() {
    let source = "class Box { then(cb: (v: i32) => void): void { cb(7); } }
        async function value(n: i32): Promise<i32> { return n; }
        export async function main(): Promise<void> {
            new Box().then((v: i32): void => { print(`${v}`); });
            print(`${await value(1).then((v: i32): i32 => v)}`);
        }";
    assert_eq!(
        helpers(source),
        ["[[Promise.then (v) => value]]<i32, i32>"],
        "only the handle receiver creates a helper"
    );
}

/// §186 rule 4: a `this` capture reports one S009 that shows the accepted
/// form, a `const` copy of the field. A callback that captures a borrowed
/// lambda reports one S009 at its position. The `const` copy is the
/// accepted control.
// Cost: three checker calls; no native build.
#[test]
fn a_rejected_capture_reports_one_diagnostic_with_its_accepted_form() {
    let receiver = "async function f(): Promise<i32> { return 1; }
        class C { n: i32 = 5; async run(): Promise<i32> { return await f().then((v: i32): i32 => v + this.n); } }
        export async function main(): Promise<void> { print(`${await new C().run()}`); }";
    let diagnostics = check_program(&[SourceFile::new("receiver.ts", receiver)])
        .expect_err("a `this` capture is rejected");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S009);
    assert_eq!(
        diagnostics[0].message,
        "a `.then(...)` callback cannot capture `this`; copy the needed field into a `const` first"
    );
    let example = diagnostics[0].example.expect("an accepted form");
    assert!(
        example.subscript.contains("const n = this.n;"),
        "{}",
        example.subscript
    );
    let copied = receiver.replace(
        "return await f().then((v: i32): i32 => v + this.n);",
        "const n = this.n; return await f().then((v: i32): i32 => v + n);",
    );
    check_program(&[SourceFile::new("copied.ts", copied)]).expect("the accepted form checks");
    let borrowed = "async function f(): Promise<i32> { return 1; }
        export async function main(): Promise<void> {
            const k: i32 = 3;
            const add = (x: i32): i32 => x + k;
            print(`${await f().then((v: i32): i32 => add(v))}`);
        }";
    let diagnostics = check_program(&[SourceFile::new("borrowed.ts", borrowed)])
        .expect_err("a borrowed capture is rejected");
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message.as_str()))
            .collect::<Vec<_>>(),
        [(
            RuleCode::S009,
            "lambda capturing `add` may capture at owned callback environment"
        )]
    );
}
