//! `join` and the array methods on a string-literal alias element
//! (`compiler.md` §216).

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile};

const MODE: &str = "type Mode = \"z\" | \"a\";\n";

fn module(body: &str) -> hir::Module {
    let source = format!("{MODE}function f(ms: Mode[], ns: i32[]): string {{ {body} }}\n");
    check_program(&[SourceFile::new("test.ts", source)]).expect("the program must check")
}

fn diagnostics(statement: &str) -> Vec<Diagnostic> {
    let source = format!(
        "{MODE}export function main(): void {{ const ms: Mode[] = [\"z\", \"a\"]; {statement} }}\n"
    );
    check_program(&[SourceFile::new("test.ts", source)]).expect_err("the program must fail")
}

fn accepted(statement: &str) -> bool {
    let source = format!(
        "{MODE}export function main(): void {{ const ms: Mode[] = [\"z\", \"a\"]; {statement} }}\n"
    );
    check_program(&[SourceFile::new("test.ts", source)]).is_ok()
}

/// The arguments of the `Callee::Arr(f)` call that `expr` is.
fn array_call(expr: &hir::Expr, f: hir::ArrFn) -> Option<&[hir::Expr]> {
    match &expr.kind {
        hir::ExprKind::Call {
            callee: hir::Callee::Arr(callee),
            args,
        } if *callee == f => Some(args),
        _ => None,
    }
}

fn returned(module: &hir::Module) -> &hir::Expr {
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("function f");
    match function.body.as_slice() {
        [hir::Stmt::Return {
            value: Some(value), ..
        }] => value,
        other => panic!("one return statement expected, got {other:?}"),
    }
}

/// The receiver and the separator of a `join` over a `map` that takes
/// one parameter of type `Mode` to `string`.
fn join_over_alias_map(expr: &hir::Expr) -> Option<(&hir::Expr, &hir::Expr)> {
    let [receiver, separator] = array_call(expr, hir::ArrFn::Join)? else {
        return None;
    };
    let [source, callback] = array_call(receiver, hir::ArrFn::Map)? else {
        return None;
    };
    let hir::ExprKind::Lambda { params, ret, .. } = &callback.kind else {
        return None;
    };
    let [parameter] = params.as_slice() else {
        return None;
    };
    (matches!(parameter.ty, subscript_compiler::Type::StringAlias(_))
        && *ret == subscript_compiler::Type::Str)
        .then_some((source, separator))
}

#[test]
fn join_has_the_hir_shape_of_the_written_map() {
    let synthesized = module("return ms.join(\",\");");
    let written = module("return ms.map((x: Mode): string => `${x}`).join(\",\");");
    let (source, separator) = join_over_alias_map(returned(&synthesized)).expect("the rewrite");
    let (written_source, written_separator) =
        join_over_alias_map(returned(&written)).expect("the written form");
    assert_eq!(source.ty, written_source.ty);
    assert!(matches!(separator.kind, hir::ExprKind::Str(ref text) if text == ","));
    assert!(matches!(written_separator.kind, hir::ExprKind::Str(ref text) if text == ","));
}

#[test]
fn join_with_no_argument_takes_the_comma_separator() {
    let checked = module("return ms.join();");
    let (_, separator) = join_over_alias_map(returned(&checked)).expect("the rewrite");
    assert!(matches!(separator.kind, hir::ExprKind::Str(ref text) if text == ","));
}

#[test]
fn another_element_type_keeps_the_direct_join() {
    let checked = module("return ns.join(\",\");");
    let value = returned(&checked);
    assert!(join_over_alias_map(value).is_none());
    let args = array_call(value, hir::ArrFn::Join).expect("a join call");
    assert!(array_call(&args[0], hir::ArrFn::Map).is_none());
}

#[test]
fn a_wrong_separator_gives_the_join_argument_text() {
    let alias = diagnostics("print(ms.join(1));");
    let source = "export function main(): void { const ns: i32[] = [1]; print(ns.join(1)); }\n";
    let numeric =
        check_program(&[SourceFile::new("test.ts", source)]).expect_err("the program must fail");
    assert_eq!(alias.len(), 1);
    assert_eq!(alias[0].code, RuleCode::S100);
    assert_eq!(alias[0].message, numeric[0].message);
    assert!(accepted("print(ms.join(\"-\"));"));
}

#[test]
fn sort_with_no_comparator_stays_rejected() {
    let rejected = diagnostics("ms.sort();");
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].code, RuleCode::S014);
    assert!(rejected[0].message.contains("`sort()` is rejected"));
    assert!(accepted(
        "ms.sort((p: Mode, q: Mode): i32 => p === q ? 0 : p === \"a\" ? -1 : 1);"
    ));
}

#[test]
fn an_ordering_operator_stays_rejected() {
    let rejected = diagnostics("print(`${ms[0] < ms[1]}`);");
    assert_eq!(rejected[0].code, RuleCode::S100);
    assert!(accepted("print(`${ms[0] === ms[1]}`);"));
}

#[test]
fn the_enum_array_methods_take_the_alias_element() {
    for statement in [
        "print(`${ms.includes(\"a\")}`);",
        "print(`${ms.indexOf(\"a\")}`);",
        "print(`${ms.filter((x: Mode): boolean => x === \"a\").length}`);",
        "ms.forEach((x: Mode): void => { print(`${x}`); });",
    ] {
        assert!(accepted(statement), "{statement}");
    }
}

#[test]
fn find_keeps_the_scalar_element_rejection() {
    let rejected = diagnostics("print(`${ms.find((x: Mode): boolean => x === \"a\")}`);");
    assert_eq!(rejected[0].code, RuleCode::S014);
    assert!(rejected[0].message.contains("`find` is rejected"));
}

fn main_can_raise(statement: &str, field: &str) -> bool {
    let source = format!(
        "{MODE}const G: Mode[] = [\"a\"];\n\
         const N: i32[] = [1];\n\
         @Descriptor class O {{ s?: string = {field}; }}\n\
         export function main(): void {{ {statement} }}\n"
    );
    let checked = check_program(&[SourceFile::new("test.ts", source)]).expect("the program checks");
    checked
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main")
        .can_raise
}

#[test]
fn an_omitted_descriptor_field_carries_the_raise_fact_of_its_initializer() {
    let omitted = "const o: O = {}; print(`${o.s}`);";
    assert!(main_can_raise(omitted, "G.join()"));
    assert!(main_can_raise(
        omitted,
        "N.map((x: i32): string => `${x}`).join()"
    ));
}

#[test]
fn an_explicit_descriptor_field_does_not_evaluate_the_initializer() {
    let explicit = "const o: O = { s: \"x\" }; print(`${o.s}`);";
    assert!(!main_can_raise(explicit, "G.join()"));
    assert!(!main_can_raise(
        "const o: O = {}; print(`${o.s}`);",
        "\"plain\""
    ));
}
