//! Stored declaration types and type-relevant initializer checks (§156).
//! Measured cost: 38 checker/HIR tests, 0.02 s wall time; no native compile.
use subscript_compiler::{check_program, hir, SourceFile, Type};

fn module(source: &str) -> hir::Module {
    check_program(&[SourceFile::entry(
        "main.ts",
        format!("{source}\nexport function main(): void {{}}"),
    )])
    .expect("initializer types must check")
}

#[test]
fn every_declaration_position_uses_local_literal_types() {
    let checked = module(
        r#"
const integer = 1; let fraction = 1.5; const text = "x"; const array = [1, 2];
class C { integer = 1; fraction = 1.5; text = "x"; static limit = 8;
    method(n = 2): i32 { return n; } constructor(n = 3) {} }
function f(n = 2): i32 { return n; }
"#,
    );
    for (name, ty) in [
        ("integer", Type::I32),
        ("fraction", Type::F64),
        ("text", Type::Str),
        ("array", Type::Array(Box::new(Type::I32))),
        ("C.limit", Type::I32),
    ] {
        assert_eq!(
            checked.globals.iter().find(|g| g.name == name).unwrap().ty,
            ty
        );
    }
    let c = checked.classes.iter().find(|c| c.name == "C").unwrap();
    assert_eq!(
        c.fields.iter().map(|f| f.ty.clone()).collect::<Vec<_>>(),
        [Type::I32, Type::F64, Type::Str]
    );
    assert_eq!(c.methods[0].params[0].ty, Type::I32);
    assert_eq!(c.ctor.as_ref().unwrap().params[0].ty, Type::I32);
    assert_eq!(
        checked
            .functions
            .iter()
            .find(|f| f.name == "f")
            .unwrap()
            .params[0]
            .ty,
        Type::I32
    );
}

#[test]
fn an_annotation_breaks_result_dependencies_at_every_depth() {
    module(
        r#"
const g = (n: i32): i32 => n === 0 ? 0 : g(n - 1);
const hs = [(n: i32): i32 => hs.length];
const left = (): i32 => right(); const right = (): i32 => left();
const read = () => value(); const value: () => i32 = (): i32 => read();
const readLater = () => later; const later = 1;
const nested = () => (n: i32): i32 => nested()(n - 1);
const block = (): () => i32 => { return (): i32 => block()(); };
const countdown: (n: i32) => i32 = (n: i32): i32 => n === 0 ? 0 : countdown(n - 1);
"#,
    );
}

#[test]
fn rejected_declarations_poison_later_reads() {
    for source in [
        "const x = null; const y = x;",
        "const x = []; const y = x;",
        "class C { x = null; y = this.x; }",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
    }
}

#[test]
fn cycles_need_an_unfinished_type_and_carry_no_divergence() {
    for source in [
        "const f = (n: i32) => n === 0 ? 0 : f(n - 1);",
        "const x = y; const y = x;",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert!(diagnostics
            .iter()
            .any(|d| d.message == "initializer type depends on its own undecided type"));
        assert!(diagnostics[0].divergence.is_none(), "{diagnostics:?}");
    }
}

#[test]
fn inferred_and_annotated_reads_get_the_same_order_diagnostic() {
    for (inferred, annotated) in [
        (
            "const x = y + 1; const y = 2;",
            "const x: i32 = y + 1; const y: i32 = 2;",
        ),
        (
            "class C { a = this.b + 1; b = 2; }",
            "class C { a: i32 = this.b + 1; b: i32 = 2; }",
        ),
    ] {
        let check = |source| check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        let actual = check(inferred);
        let control = check(annotated);
        assert_eq!(actual.len(), control.len());
        for (a, b) in actual.iter().zip(&control) {
            assert_eq!(a.code, b.code);
            assert_eq!(a.message, b.message);
            assert_eq!(a.divergence, b.divergence);
        }
    }
}

#[test]
fn a_signature_without_an_initializer_still_needs_annotations() {
    for source in [
        "function f(n): i32 { return n; }",
        "function f() { return 1; }",
        "class C { f() { return 1; } }",
    ] {
        assert!(check_program(&[SourceFile::entry("main.ts", source)]).is_err());
    }
}

#[test]
fn a_default_can_use_an_earlier_parameter_and_a_later_result() {
    let checked = module(
        r#"
function f(n: i32, next = n + 1, last = later()): i32 { return next + last; }
function later(): i32 { return 2; }
"#,
    );
    assert!(checked
        .functions
        .iter()
        .find(|f| f.name == "f")
        .unwrap()
        .params
        .iter()
        .all(|p| p.ty == Type::I32));
}

#[test]
fn call_arguments_are_checked_after_parameter_types_settle() {
    let diagnostics = check_program(&[SourceFile::entry(
        "main.ts",
        r#"
function first(n = second("bad")): i32 { return n; }
function second(n = first(1)): i32 { return n; }
"#,
    )])
    .unwrap_err();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("the argument"));
}

#[test]
fn defaults_settle_with_body_effects_and_only_omitted_arguments_read_them() {
    let checked = module(
        r#"
function loud(): i32 { throw new Error("default"); }
function take(n = loud()): i32 { return n; }
function omitted(): i32 { return take(); }
function supplied(): i32 { return take(1); }
function first(n = second(1)): i32 { return n; }
function second(n = first(1)): i32 { return n; }
class C { value: i32; constructor(n = loud()) { this.value = n; }
    take(n = loud()): i32 { return n; } }
function constructOmitted(): C { return new C(); }
function constructSupplied(): C { return new C(1); }
function methodOmitted(c: C): i32 { return c.take(); }
function methodSupplied(c: C): i32 { return c.take(1); }
async function task(n = loud()): Promise<i32> { return n; }
async function asyncOmitted(): Promise<i32> { return await task(); }
async function asyncSupplied(): Promise<i32> { return await task(1); }
function* values(n = loud()): Generator<i32> { yield n; }
function generatorOmitted(): void { for (const n of values()) { print(`${n}`); } }
function generatorSupplied(): void { for (const n of values(1)) { print(`${n}`); } }
"#,
    );
    let function = |name| checked.functions.iter().find(|f| f.name == name).unwrap();
    assert!(!function("take").can_raise);
    assert!(function("take").params[0].default_can_raise);
    for name in [
        "omitted",
        "constructOmitted",
        "methodOmitted",
        "asyncOmitted",
        "generatorOmitted",
    ] {
        assert!(function(name).can_raise, "{name}");
    }
    for name in [
        "supplied",
        "constructSupplied",
        "methodSupplied",
        "asyncSupplied",
        "generatorSupplied",
        "first",
        "second",
        "task",
        "values",
    ] {
        assert!(!function(name).can_raise, "{name}");
    }
    assert!(!function("first").params[0].default_can_raise);
    assert!(!function("second").params[0].default_can_raise);
}

#[test]
fn destructuring_defaults_and_local_lambda_defaults_use_their_declaration_scope() {
    module(
        r#"
function first([n] = [1]): i32 { return n; }
class Box { n = 1; }
function second({n} = new Box()): i32 { return n; }
const outer = (): i32 => { const base = 1; const f = (n = base): i32 => n; return f(1); };
"#,
    );
}

#[test]
fn inferred_initializers_preserve_generic_bodies_and_default_scopes() {
    let checked = module(
        r#"
function identity<T>(value: T): T { print("body"); return value; }
class Box<T> { value: T; constructor(value: T) { this.value = value; } }
class Methods { echo<U>(value: U): U { print("method"); return value; } }
const value = identity<i32>(1);
const box = new Box<i32>(1);
const echoed = new Methods().echo<i32>(1);
"#,
    );
    for function in checked
        .functions
        .iter()
        .filter(|f| f.name.starts_with("identity"))
    {
        let hir::Stmt::Expr(value) = &function.body[0] else {
            panic!("print body");
        };
        let hir::ExprKind::Call { args, .. } = &value.kind else {
            panic!("print call");
        };
        assert_eq!(args.len(), 1);
    }
    for class in checked.classes.iter().filter(|c| c.name.starts_with("Box")) {
        assert!(class.ctor.is_some());
        assert!(class.methods.iter().all(|m| !m.body.is_empty()));
    }
}

#[test]
fn deferred_bodies_keep_transitive_captures() {
    let checked = module(
        r#"
const root = (): i32 => { const captured = 1; const outer = (): i32 => { const inner = (): i32 => captured; return inner(); }; return outer(); };
"#,
    );
    fn find(expression: &hir::Expr, out: &mut Vec<Vec<String>>) {
        if let hir::ExprKind::Lambda { captures, .. } = &expression.kind {
            out.push(captures.iter().map(|c| c.name.clone()).collect());
        }
        for child in expression.children() {
            match child {
                hir::HirChild::Expr(e) => find(e, out),
                hir::HirChild::Stmt(s) => statement(s, out),
            }
        }
    }
    fn statement(s: &hir::Stmt, out: &mut Vec<Vec<String>>) {
        for child in s.children() {
            match child {
                hir::HirChild::Expr(e) => find(e, out),
                hir::HirChild::Stmt(s) => statement(s, out),
            }
        }
    }
    let mut captures = Vec::new();
    find(
        &checked
            .globals
            .iter()
            .find(|g| g.name == "root")
            .unwrap()
            .init,
        &mut captures,
    );
    assert_eq!(
        captures,
        vec![
            vec![],
            vec!["captured".to_string()],
            vec!["captured".to_string()]
        ]
    );
}

#[test]
fn descriptor_defaults_use_the_stored_initializer_and_the_descriptor_context() {
    let checked = module("@Descriptor class C { n? = 1; }");
    let c = checked.classes.iter().find(|c| c.name == "C").unwrap();
    assert_eq!(c.fields[0].ty, Type::I32);
    assert!(c.fields[0].init.is_some());
    let diagnostics = check_program(&[SourceFile::entry(
        "main.ts",
        "@Descriptor class C { n? = 1; m? = this.n; }",
    )])
    .unwrap_err();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("descriptor member default"));
}

#[test]
fn deferred_argument_checks_keep_builtin_receivers_and_synthetic_owners() {
    let checked = module(
        r#"
class C { n = 1; take(n: i32): i32 { return n; } }
function maybe(): C | null { return new C(); }
function take(n: i32): i32 { return n; }
const flag = "x".includes("x");
const result = (): i32 => { return take(maybe()?.n ?? 0); };
const guarded = (): void => { maybe()?.take(1); };
const loop = (): void => { for (let i = 0; take(maybe()?.n ?? 0) > 0; i += take(maybe()?.n ?? 0)) { if (i > 0) { continue; } break; } };
"#,
    );
    let hir::ExprKind::Call { args, .. } = &checked
        .globals
        .iter()
        .find(|g| g.name == "flag")
        .unwrap()
        .init
        .kind
    else {
        panic!("builtin call");
    };
    assert_eq!(
        args.len(),
        3,
        "the receiver, supplied argument, and builtin start default"
    );
    assert!(matches!(&args[1].kind, hir::ExprKind::Str(value) if value == "x"));
    let hir::ExprKind::Lambda { body, .. } = &checked
        .globals
        .iter()
        .find(|g| g.name == "result")
        .unwrap()
        .init
        .kind
    else {
        panic!("lambda");
    };
    assert!(
        matches!(body[0], hir::Stmt::Let { .. }),
        "the receiver must execute before its return: {body:?}"
    );
    let diagnostics = check_program(&[SourceFile::entry(
        "main.ts",
        r#"
class C { n = 1; } function maybe(): C | null { return new C(); }
function take(n: i32): i32 { return n; }
const invalid = take(maybe()?.n ?? 0);
"#,
    )])
    .unwrap_err();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("non-place receiver"));
}

#[test]
fn a_shadowed_parameter_is_a_different_declaration() {
    module(
        r#"
function f(n = ((n: i32) => n + 1)(1)): i32 { return n; }
function g(n = ((): i32 => { const n = 2; return n; })()): i32 { return n; }
function m(list = [5].map((list) => list)): i32[] { return list; }
"#,
    );
}

#[test]
fn supplied_defaults_do_not_add_initializer_reads() {
    module("const total = sum(1); function sum(base = total): i32 { return base; }");
    let diagnostics = check_program(&[SourceFile::entry(
        "main.ts",
        "const total: i32 = sum(); function sum(base: i32 = total): i32 { return base; }",
    )])
    .unwrap_err();
    assert!(diagnostics.iter().any(|d| d.message.contains("before")));
}

#[test]
fn async_poison_does_not_report_another_callee_error() {
    let diagnostics = check_program(&[SourceFile::entry(
        "main.ts",
        "const runner = async () => { }; async function use(): Promise<void> { await runner(); }",
    )])
    .unwrap_err();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[test]
fn generic_instances_requested_in_bodies_have_checked_bodies() {
    let checked = check_program(&[SourceFile::entry("main.ts", r#"
function make<T>(n: i32): T[] { const arr: T[] = []; return arr; }
class Box<T> { store = make<T>(1); }
function fill<T>(v: T, arr = make<T>(0)): i32 { return arr.length; }
export function main(): void { const box = new Box<f64>(); print(`${box.store.length},${fill("a")}`); }
"#)]).unwrap();
    assert!(checked
        .functions
        .iter()
        .filter(|f| f.name.starts_with("make"))
        .all(|f| !f.body.is_empty()));
}

#[test]
fn diagnostics_follow_top_level_declaration_order() {
    let diagnostics = check_program(&[SourceFile::entry(
        "main.ts",
        "const later = () => other;\nconst early = null;\nconst other = null;",
    )])
    .unwrap_err();
    assert!(diagnostics
        .windows(2)
        .all(|pair| pair[0].pos.line <= pair[1].pos.line));
}

#[test]
fn deferred_diagnostics_follow_the_slot_module_and_declaration() {
    let diagnostics = check_program(&[
        SourceFile::entry(
            "main.ts",
            "import { read } from \"./lib\";\nconst first = () => missingFirst; const second = null;",
        ),
        SourceFile::new("lib.ts", "export const read = () => missingLibrary;"),
    ])
    .unwrap_err();
    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
    assert_eq!(diagnostics[0].pos.file, "lib.ts");
    assert_eq!(diagnostics[1].pos.file, "main.ts");
    assert_eq!(diagnostics[2].pos.file, "main.ts");
    assert!(diagnostics[1].message.contains("missingFirst"));
    assert!(diagnostics[2].message.contains("null"));
}

#[test]
fn a_required_hir_parameter_has_no_default_facts() {
    let parameter = hir::Param::new(
        "n",
        Type::I32,
        subscript_compiler::Pos::new("main.ts", 1, 1),
    );
    assert_eq!(parameter.name, "n");
    assert_eq!(parameter.ty, Type::I32);
    assert!(parameter.default.is_none());
    assert!(!parameter.default_can_raise);
}

#[test]
fn a_union_callee_checks_a_deferred_callback_body_once() {
    let diagnostics = check_program(&[SourceFile::entry(
        "main.ts",
        r#"
function invoke<T extends (value: () => i32) => i32, U extends (value: () => i32) => f64>(
    flag: boolean, t: T, u: U, n = (flag ? t : u)((): i32 => "bad")): void {}
export function main(): void {}
"#,
    )])
    .unwrap_err();
    let body_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.message.contains("the lambda body"))
        .collect();
    assert_eq!(body_errors.len(), 1, "{diagnostics:?}");
}

#[test]
fn completing_unknown_arguments_preserves_completed_fixed_slots() {
    let checked = module(
        r#"
function first(cb: () => i32, n = second(1)): i32 { return cb() + n; }
function second(n = first((): i32 => 2, 1)): i32 { return n; }
"#,
    );
    let second = checked
        .functions
        .iter()
        .find(|f| f.name == "second")
        .unwrap();
    let hir::ExprKind::Call { args, .. } = &second.params[0].default.as_ref().unwrap().kind else {
        panic!("default call");
    };
    let hir::ExprKind::Lambda { body, .. } = &args[0].kind else {
        panic!("fixed callback");
    };
    assert!(!body.is_empty());
}

#[test]
fn later_defaults_read_decided_parameter_types() {
    let checked = module(
        r#"
function h(a: i32, b = 2, c = b + 1): i32 { return a + c; }
function range(x = 1, y = x * 2): i32 { return y; }
const add = (a = 1, b = a * 2): i32 => a + b;
class C { v = 3; constructor(x = 1, y = x * 2) {}
    pick(alt = this.v, list = [alt, this.v]): i32 { return list[0]; } }
"#,
    );
    for name in ["h", "range"] {
        assert!(checked
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap()
            .params
            .iter()
            .all(|p| p.ty == Type::I32));
    }
    let diagnostics = check_program(&[SourceFile::entry(
        "main.ts",
        "function h(b = 2, c = b + 1): string { return c; }",
    )])
    .unwrap_err();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("return"));
}

#[test]
fn decisions_use_the_same_this_rules_as_annotated_declarations() {
    for (inferred, annotated) in [
        ("class Btn { count = 0; onClick = (): void => { this.count = this.count + 1; }; }",
         "class Btn { count: i32 = 0; onClick: () => void = (): void => { this.count = this.count + 1; }; }"),
        ("class C { d = 1; g = () => this.d; }",
         "class C { d: i32 = 1; g: () => i32 = (): i32 => this.d; }"),
        ("class C { d = 1; m = [1].map((v) => v + this.d); }",
         "class C { d: i32 = 1; m: i32[] = [1].map((v) => v + this.d); }"),
        ("class C { d = 1; m(k = () => this.d): i32 { return k(); } }",
         "class C { d: i32 = 1; m(k: () => i32 = (): i32 => this.d): i32 { return k(); } }"),
    ] {
        let check = |source| check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        let actual = check(inferred);
        let control = check(annotated);
        assert_eq!(actual.len(), control.len(), "{inferred}: {actual:?} / {control:?}");
        for (a, b) in actual.iter().zip(control) {
            assert_eq!((a.code, &a.message, a.divergence), (b.code, &b.message, b.divergence), "{inferred}");
        }
    }
}

#[test]
fn constructor_signatures_remain_readable_during_decisions() {
    module("class Tree { constructor(n: i32, child = n > 0 ? new Tree(n - 1) : null) {} }");
}

#[test]
fn only_whole_direct_initializers_defer_supplied_arguments() {
    for source in [
        "function h(n: i32, k = h(n - 1, 0)): i32 { return k; }",
        "function h(n: i32, k = n > 0 ? h(n - 1) : 0): i32 { return k; }",
        "function first(n = second(1) + 0): i32 { return n; } function second(n = first(1)): i32 { return n; }",
        "class C { m(n: i32, k = this.m(n - 1, 0)): i32 { return k; } }",
        "function f(a = 1, b = f(0) + 0): i32 { return b; }",
        "class C { f(a = 1, b = this.f(0) + 0): i32 { return b; } }",
        "class C { constructor(a = 1, b = new C(0)) {} }",
        "class C { static f(a = 1, b = C.f(0) + 0): i32 { return b; } }",
    ] { module(source); }
    for initializer in [
        "n > 0 ? h(n - 1, 0) : 0",
        "[h(n - 1, 0)]",
        "h(n - 1, 0) + 1",
    ] {
        let source = format!("function h(n: i32, k = {initializer}): i32 {{ return 0; }}");
        let diagnostics = check_program(&[SourceFile::entry("main.ts", &source)]).unwrap_err();
        assert!(
            diagnostics.iter().any(|d| d.message
                == "initializer type depends on its own undecided type"
                && d.divergence.is_none()),
            "{source}: {diagnostics:?}"
        );
    }
    for source in [
        "class C { m(n: i32, k = n > 0 ? this.m(n - 1, 0) : 0): i32 { return k; } }",
        "class C { static m(n: i32, k = n > 0 ? C.m(n - 1, 0) : 0): i32 { return k; } }",
        "function h<T>(n: T, k = h<T>(n, 0)): i32 { return 0; }",
        "class C { constructor(n: i32, k = new C(n - 1, 0)) {} }",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert!(
            diagnostics.iter().any(|d| d.message
                == "initializer type depends on its own undecided type"
                && d.divergence.is_none()),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn recursive_function_values_need_a_default_without_the_signature_dependency() {
    for source in [
        "const h = f; function f(n: i32, k = h): void {}",
        "function f(n: i32, k = f): void {}",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
        assert_eq!(
            diagnostics[0].divergence,
            Some(
                subscript_compiler::divergence::Divergence::FunctionValueParameterAnnotationNeeded
            )
        );
        assert!(diagnostics[0]
            .message
            .contains("avoid a recursive function value"));
    }
}

#[test]
fn generic_callback_annotation_rejections_carry_c24() {
    for source in [
        "function each<T>(xs: T[], f: (x: T) => void): void {} each([1, 2], (x) => {});",
        "function apply<T>(x: T, f: (x: T) => T): T { return f(x); } apply(1, (x) => x + 1);",
        "function each<T>(xs: T[], f: (x: T[]) => void): void {} each([1, 2], ([x]) => {});",
        "function each<T>(xs: T[], f: (x: T[]) => void): void {} each<i32>([1, 2], ([x]) => {});",
        "class C { each<T>(xs: T[], f: (x: T[]) => void): void {} } new C().each<i32>([1, 2], ([x]) => {});",
        "class C { static each<T>(xs: T[], f: (x: T[]) => void): void {} } C.each<i32>([1, 2], ([x]) => {});",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert!(diagnostics.iter().any(|d| d.message.contains("parameters require") && d.divergence == Some(subscript_compiler::divergence::Divergence::GenericCallbackParameterAnnotationNeeded)), "{source}: {diagnostics:?}");
    }
    let diagnostics =
        check_program(&[SourceFile::entry("main.ts", "const f = (x) => x;")]).unwrap_err();
    assert!(diagnostics[0].divergence.is_none());
}

#[test]
fn initializer_default_route_names_the_function_and_parameter() {
    let diagnostics = check_program(&[SourceFile::entry("main.ts", "const total: i32 = g(); function g(): i32 { return f(); } function f(x: i32 = total): i32 { return x; }")]).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("`g`") && d.message.contains("f (default of x)")),
        "{diagnostics:?}"
    );
    assert!(diagnostics.iter().all(|d| !d.message.contains("[default]")));
}

#[test]
fn unannotated_block_results_require_the_original_annotation() {
    for source in [
        "const callbacks = (n: i32) => { if (n > 0) { return (x: i32) => x; } return (x) => x + 1; };",
        "class K { static walk = (n: i32) => { if (n > 0) { return K.walk(n - 1); } return 0; }; }",
        "const f = (x: i32) => { if (x > 0) { return x; } return; };",
        "class Box {} const f = (b: Box | null) => { if (b !== null) { return b; } return b; };",
        "let loop = (n: i32) => { if (n > 0) { return loop(n - 1); } return 0; };",
        "function local(): void { let loop = (n: i32) => { if (n > 0) { return loop(n - 1); } return 0; }; }",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, subscript_compiler::RuleCode::S100);
        assert_eq!(diagnostics[0].message, "a lambda with a block body requires a return type annotation");
        assert_eq!(diagnostics[0].divergence, Some(subscript_compiler::divergence::Divergence::BlockLambdaReturnAnnotationMissingForm));
    }
}

#[test]
fn generic_class_method_defaults_reject_class_parameter_types_only() {
    for source in [
        "class Cell<T> { v: T; constructor(v: T) { this.v = v; } pair(other = this.v): string { return `${other}`; } }",
        "class Cell<T> { a: T[]; constructor(a: T[]) { this.a = a; } pick(n = this.a.length, xs = this.a): i32 { return n; } }",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
        assert_eq!(diagnostics[0].divergence, Some(subscript_compiler::divergence::Divergence::GenericClassDefaultParameterAnnotationNeeded));
        assert!(diagnostics[0].message.contains("annotate the parameter"));
    }
    module("class Cell<T> { a: T[] = []; b = this.a; constructor(a: T[]) { this.a = a; } pick(n = this.a.length): i32 { return n; } pair(other: T[] = this.a): T[] { return other; } } function f<T>(k: T, other = k): T { return other; }");
}

#[test]
fn every_generic_parameter_supplies_the_rule_six_context() {
    for source in [
        "function g<T>(v: T): void {} g((x) => 1);",
        "function g<T>(v: T): void {} g<i32>((x) => 1);",
        "function g<T>(v: T): void {} g(([x]) => 1);",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message == "parameters require a type annotation"
                    && d.divergence == Some(subscript_compiler::divergence::Divergence::GenericCallbackParameterAnnotationNeeded)),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn generic_function_value_candidates_keep_the_parameter_divergence() {
    for source in [
        "function firstOr<T>(xs: T[], fallback: T): T { return fallback; } const handlers = [(x: i32): i32 => x]; firstOr(handlers, (x) => x + 1);",
        "function pair<T>(a: T, b: T): T { return b; } pair((x: i32): i32 => x, (x) => x + 1);",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert!(diagnostics.iter().any(|d| d.message == "parameters require a type annotation"
            && d.divergence == Some(subscript_compiler::divergence::Divergence::GenericCallbackParameterAnnotationNeeded)), "{source}: {diagnostics:?}");
    }
}

#[test]
fn nested_initializer_cycles_do_not_inherit_the_function_value_decision() {
    let source = "function onA(n: i32, count = handlers.length): void {} function onB(n: i32, count = 1): void {} const handlers = [onA, onB];";
    let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "initializer type depends on its own undecided type"
    );
    assert!(diagnostics[0].divergence.is_none());
    module("function f(n: i32, k = 0): void {}");
}

#[test]
fn a_rejected_block_result_still_checks_non_result_statements() {
    for source in [
        "function f(): void { let counter = 0; const noret = () => { counter += 10; }; }",
        "function f(): void { let counter = 0; const noret = () => { counter += 10; return missing; }; }",
    ] {
    let diagnostics = check_program(&[SourceFile::entry(
        "main.ts",
        source,
    )])
    .unwrap_err();
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert!(diagnostics.iter().any(|d| d.divergence
        == Some(
            subscript_compiler::divergence::Divergence::BlockLambdaReturnAnnotationMissingForm
        )));
    assert!(diagnostics
        .iter()
        .any(|d| d.code == subscript_compiler::RuleCode::S009));
    assert!(diagnostics.iter().all(|d| !d.message.contains("missing")));
    }
}

#[test]
fn same_named_methods_use_the_receivers_generic_fact() {
    for prefix in [
        "class A { m<T>(n: T): i32 { return 1; } }",
        "class A { m(n: i32): i32 { return 1; } }",
    ] {
        module(&format!(
            "{prefix} class B {{ m(n: i32, k = this.m(n - 1, 0)): i32 {{ return k; }} }}"
        ));
        let source = format!("{prefix} class B {{ m<T>(v: T, f: (xs: T[]) => i32): i32 {{ return f([v]); }} }} new B().m<i32>(1, ([x]) => 1);");
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert!(diagnostics.iter().any(|d| d.divergence == Some(subscript_compiler::divergence::Divergence::GenericCallbackParameterAnnotationNeeded)), "{diagnostics:?}");
        let source =
            format!("{prefix} class B {{ m<T>(n: T, k = this.m<T>(n, 0)): i32 {{ return 0; }} }}");
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert!(
            diagnostics.iter().any(|d| d.message
                == "initializer type depends on its own undecided type"
                && d.divergence.is_none()),
            "{diagnostics:?}"
        );
    }
    module("function g<T>(v: T, f: (x: T) => T): T { return f(v); } g<i32>(1, (x) => x + 1);");
    module("class A { static m(n: i32, k = A.m(n - 1, 0)): i32 { return k; } } class B { static m(n: i32, k = B.m(n - 1, 0)): i32 { return k; } } A.m(1); B.m(1);");
}

#[test]
fn recorded_expression_lambda_cycles_retain_the_rule_three_site() {
    for source in [
        "const handlers = [() => handlers.length];",
        "const f = () => f;",
        "const idle = () => running; const running = () => idle;",
    ] {
        let diagnostics = check_program(&[SourceFile::entry("main.ts", source)]).unwrap_err();
        assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
        assert_eq!(
            diagnostics[0].message,
            "initializer type depends on its own undecided type"
        );
        assert!(diagnostics[0].divergence.is_none());
    }
}
