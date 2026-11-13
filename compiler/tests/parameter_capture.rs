//! Whole-program escape contracts (compiler.md §118).
#[path = "corpus/interop.rs"]
#[allow(dead_code)]
mod interop;

use subscript_compiler::{check_program, hir, RuleCode, SourceFile};

fn check(source: &str) -> Result<hir::Module, Vec<subscript_compiler::Diagnostic>> {
    check_program(&[SourceFile::new("capture.ts", source)])
}
fn flags(child: hir::HirChild<'_>, result: &mut Vec<bool>) {
    match child {
        hir::HirChild::Expr(e) => {
            if let hir::ExprKind::Lambda {
                captures,
                owns_environment,
                ..
            } = &e.kind
            {
                if !captures.is_empty() {
                    result.push(*owns_environment);
                }
            }
            for child in e.children() {
                flags(child, result);
            }
        }
        hir::HirChild::Stmt(s) => {
            for child in s.children() {
                flags(child, result);
            }
        }
    }
}
fn accepts_owned(source: &str) {
    let module = check(source).expect("compiler.md §274 rule 1 accepts an owned capture");
    let mut result = Vec::new();
    for owner in module.expression_owners() {
        if let hir::ExpressionOwner::Body { statements, .. } = owner {
            for s in statements {
                flags(hir::HirChild::Stmt(s), &mut result);
            }
        }
    }
    assert!(result.contains(&true), "{source}: {result:?}");
}

fn store(body: &str, _boundary: &str) {
    accepts_owned(&format!(
        r#"
function named(): i32 {{ return 0; }}
class Holder {{ cb: () => i32 = named; }}
let global: () => i32 = named;
export function main(): void {{
    const value: i32 = 5;
    const cb = (): i32 => value;
    const holder = new Holder();
    const array: (() => i32)[] = [named];
    {body}
}}
"#
    ));
}
#[test]
fn field_store() {
    store("holder.cb = cb;", "field store");
}
#[test]
fn global_store() {
    store("global = cb;", "global store");
}
#[test]
fn element_store() {
    store("array[0] = cb;", "element store");
}
#[test]
fn aggregate_literal() {
    for body in [
        "const a = [cb];",
        "const a = [...array, cb];",
        "const a: FixedArray<() => i32, 1> = [cb];",
    ] {
        store(body, "array literal");
    }
}
#[test]
fn map_store() {
    store(
        "const map = new Map<i32, () => i32>(); map.set(1, cb);",
        "Map store",
    );
}
#[test]
fn set_store() {
    accepts_owned("function named(): i32 { return 0; } class Holder { cb: () => i32 = named; constructor(cb: () => i32) { cb(); } } export function main(): void { const value: i32 = 5; const holder = new Holder((): i32 => value); const set = new Set<Holder>(); set.add(holder); }");
}
#[test]
fn array_mutators() {
    for (operation, boundary) in [
        ("push", "array push"),
        ("fill", "array store"),
        ("unshift", "array store"),
    ] {
        store(&format!("array.{operation}(cb);"), boundary);
    }
}
#[test]
fn return_boundary() {
    accepts_owned("function make(): () => i32 { const value: i32 = 5; return (): i32 => value; } export function main(): void {}");
}
#[test]
fn expression_body_return() {
    store("const make = (): (() => i32) => cb;", "return");
}
#[test]
fn yield_boundary() {
    accepts_owned("function* gen(): Generator<() => i32> { const value: i32 = 5; yield (): i32 => value; } export function main(): void {}");
}
#[test]
fn escaping_lambda_capture() {
    accepts_owned("function make(): () => i32 { const value: i32 = 5; const cb = (): i32 => value; return (): i32 => cb(); } export function main(): void {}");
}
#[test]
fn held_async_argument() {
    accepts_owned("async function run(cb: () => i32): Promise<i32> { return cb(); } export async function main(): Promise<void> { const value: i32 = 5; const cb = (): i32 => value; const h = run(cb); await h; }");
}
#[test]
fn c_callback_slot() {
    let source = "export function main(): void { const value: i32 = 5; const info = new SubCallbackInfo((message, userdata, userparam): void => { print(`${value}`); }, null, null); }";
    let diagnostics = check_program(&[
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("capture.ts", source),
    ])
    .expect_err("the callback slot stores its lambda");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == RuleCode::S100 && d.message.contains("SubLogCallback")),
        "{diagnostics:?}"
    );
}
#[test]
fn local_facts_ignore_source_order_and_loop_order() {
    for body in [
        "let alias: () => i32 = named; if (true) { return alias; } alias = (): i32 => value; return named;",
        "let alias: () => i32 = named; let second: () => i32 = named; for (let i: i32 = 0; i < 2; i++) { if (i === 1) { return alias; } alias = second; second = (): i32 => value; } return named;",
    ] {
        accepts_owned(&format!("function named(): i32 {{ return 0; }} function make(): () => i32 {{ const value: i32 = 5; {body} }} export function main(): void {{}}"));
    }
}
#[test]
fn container_reads_infer_escape() {
    for (ty, body) in [
        ("(() => i32)[]", "return values[0];"),
        ("(() => i32)[][]", "return values[0][0];"),
        ("FixedArray<() => i32, 1>", "return values[0];"),
        ("Generator<() => i32>", "return values.next().value;"),
        (
            "Generator<() => i32>",
            "for (const value of values) { return value; } return named;",
        ),
        ("Map<i32, () => i32>", "return values.getOr(0, named);"),
    ] {
        let module = check(&format!("function named(): i32 {{ return 0; }} function read(values: {ty}): () => i32 {{ {body} }} export function main(): void {{}}")).expect("escaping parameters are clean inside their body");
        assert!(
            module
                .functions
                .iter()
                .find(|f| f.name == "read")
                .unwrap()
                .params[0]
                .escape_kind
                != hir::EscapeKind::None
        );
    }
    accepts_owned(
        "function* gen(cb: () => i32): Generator<i32> { yield cb(); }\nfunction store(values: Generator<i32>[], value: Generator<i32>): void {\n  values.push(value);\n}\nexport function main(): void {\n  const value: i32 = 5;\n  const values: Generator<i32>[] = [];\n  store(values, gen((): i32 => value));\n}");
}
#[test]
fn escape_inference_crosses_two_calls() {
    let source = "function last(cb: () => i32): () => i32 { return cb; } function middle(cb: () => i32): () => i32 { return last(cb); } function first(cb: () => i32): () => i32 { return middle(cb); } export function main(): void { const value: i32 = 5; first((): i32 => value); }";
    accepts_owned(source);
    check(&source.replace("(): i32 => value", "(): i32 => 5"))
        .expect("clean arguments pass the inferred contract");
}
#[test]
fn constructor_and_method_inference() {
    let source = "function named(): i32 { return 0; } class Holder { cb: () => i32 = named; constructor(cb: () => i32) { this.set(cb); } set(cb: () => i32): void { this.cb = cb; } } export function main(): void { const value: i32 = 5; new Holder((): i32 => value); }";
    accepts_owned(source);
    check(&source.replace("(): i32 => value", "named"))
        .expect("a clean constructor argument can reach a field");
}
#[test]
fn escaping_caller_argument_owns_environment() {
    accepts_owned(
        "class Holder {\n  callback: (() => i32) | null = null;\n}\nfunction store(holder: Holder, cb: () => i32): void {\n  holder.callback = cb;\n}\nexport function main(): void {\n  const holder = new Holder();\n  const value: i32 = 5;\n  store(holder, (): i32 => value);\n}");
}
#[test]
fn indirect_calls_accept_owned_arguments() {
    accepts_owned(
        "function call(cb: () => i32): i32 { return cb(); }\nexport function main(): void {\n  const value: i32 = 5;\n  const indirect: (cb: () => i32) => i32 = call;\n  print(`${indirect((): i32 => value)}`);\n}");
    check("function named(): i32 { return 5; } function call(cb: () => i32): i32 { return cb(); } export function main(): void { const indirect = call; indirect(named); }").expect("a clean indirect argument passes");
}
#[test]
fn call_and_downward_pass() {
    check(include_str!(
        "../../corpus/accept/a262-parameter-called-and-passed-down.ts"
    ))
    .expect("downward calls keep captures alive");
    accepts_owned(
        "function forward(cb: () => i32): () => i32 {\n  return cb;\n}\nexport function main(): void {\n  const value: i32 = 5;\n  const callback = forward((): i32 => value);\n  print(`${callback()}`);\n}");
}
#[test]
fn direct_await_keeps_a_capture_alive() {
    let source = "async function run(cb: () => i32): Promise<i32> { await Context.suspend(); return cb(); } export async function main(): Promise<void> { const value: i32 = 5; const cb = (): i32 => value; await run(cb); }";
    check(source).expect("direct await keeps the caller alive");
    accepts_owned(&source.replace("await run(cb);", "const h = run(cb); await h;"));
}
#[test]
fn local_generator_keeps_a_capture_alive() {
    check("function* gen(cb: () => i32): Generator<i32> { yield cb(); } export function main(): void { const value: i32 = 5; const local = gen((): i32 => value); print(`${local.next().value}`); }").expect("the generator stays local");
    accepts_owned(
        "function* gen(cb: () => i32): Generator<i32> {\n  yield cb();\n  yield cb();\n}\nfunction make(): Generator<i32> {\n  const value: i32 = 5;\n  return gen((): i32 => value);\n}\nexport function main(): void {\n  for (const value of make()) { print(`${value}`); }\n}");
}
#[test]
fn clean_generator_in_a_field() {
    check(include_str!(
        "../../corpus/accept/a217-generator-in-a-class-field.ts"
    ))
    .expect("a clean generator can enter storage");
    accepts_owned(
        "function* gen(cb: () => i32): Generator<i32> { yield cb(); }\nfunction store(values: Generator<i32>[], value: Generator<i32>): void {\n  values.push(value);\n}\nexport function main(): void {\n  const value: i32 = 5;\n  const values: Generator<i32>[] = [];\n  store(values, gen((): i32 => value));\n}");
}
#[test]
fn builtin_callbacks_stay_downward() {
    check("export function main(): void { const value: i32 = 5; const values: i32[] = [1, 2]; values.forEach((n): void => { print(`${n + value}`); }); const mapped = values.map((n): i32 => n + value); const filtered = values.filter((n): boolean => n < value); const total = values.reduce((sum, n): i32 => sum + n + value, 0); values.sort((a, b): i32 => a - b + value); }").expect("built-in callbacks do not escape");
    accepts_owned(
        "function call(cb: () => i32): i32 { return cb(); }\nexport function main(): void {\n  const value: i32 = 5;\n  const indirect: (cb: () => i32) => i32 = call;\n  print(`${indirect((): i32 => value)}`);\n}");
}

#[test]
fn nullable_parameter_inference() {
    let source = "function forward(cb: (() => i32) | null): (() => i32) | null { return cb; } export function main(): void { const value: i32 = 5; forward((): i32 => value); }";
    accepts_owned(source);
    check(&source.replace("(): i32 => value", "null")).expect("null is clean");
}

#[test]
fn this_and_a_const_receiver_copy_are_clean() {
    let source = "function named(): i32 { return 1; } class B { cb: () => i32 = named; self(): B { return this; } keep(out: B[]): void { out.push(this); } } export function main(): void { const b = new B(); b.self(); b.keep([]); }";
    check(source).expect("this has clean fields");
    accepts_owned(&source.replace(
        "self(): B { return this; }",
        "self(): () => B { const owner = this; return (): B => owner; }",
    ));
}

#[test]
fn held_await_result_is_clean() {
    let source = "function named(): i32 { return 1; } let saved: () => i32 = named; async function make(): Promise<() => i32> { await Context.suspend(); return named; } export async function main(): Promise<void> { const h = make(); saved = await h; }";
    check(source).expect("a held await returns a clean value");
    check(&source.replace(
        "const h = make(); saved = await h;",
        "saved = await make();",
    ))
    .expect("direct await returns a clean value");
    accepts_owned(&source.replace("return named;", "const n: i32 = 5; return (): i32 => n;"));
}

#[test]
fn defaults_join_parameter_facts() {
    let source = "function named(): i32 { return 1; } let saved: () => i32 = named; function f(a: () => i32, b: () => i32 = a): i32 { return a() + b(); } export function main(): void { const n: i32 = 5; f((): i32 => n); }";
    check(source).expect("a default passes a capture downward");
    let storing = source.replace("return a() + b();", "saved = b; return 0;");
    check(&storing).expect("the stored capture owns its environment");
    check(&storing.replace("(): i32 => n", "named")).expect("a clean caller passes");
}

#[test]
fn builtin_carrier_results_follow_the_receiver() {
    for (body, escapes) in [
        (
            "const v = values.shift(); if (v === undefined) { return named; } return v;",
            true,
        ),
        ("const v = values.slice(); return v[0];", true),
        ("values.shift(); return named;", false),
    ] {
        let module = check(&format!("function named(): i32 {{ return 1; }} function take(values: (() => i32)[]): () => i32 {{ {body} }} export function main(): void {{ take([named]); }}")).expect("clean arrays pass");
        assert_eq!(
            module
                .functions
                .iter()
                .find(|f| f.name == "take")
                .unwrap()
                .params[0]
                .escape_kind
                != hir::EscapeKind::None,
            escapes
        );
    }
}

#[test]
fn constructors_and_methods_accept_owned_arguments() {
    let source = "function named(): i32 { return 0; } class K { cb: () => i32 = named; constructor(cb: () => i32) { this.cb = cb; } set(cb: () => i32): void { this.cb = cb; } } export function main(): void { const n: i32 = 5; const k = new K(named); CALL; }";
    for (call, _name) in [
        ("new K((): i32 => n)", "K constructor"),
        ("k.set((): i32 => n)", "K.set"),
    ] {
        accepts_owned(&source.replace("CALL", call));
        check(&source.replace("CALL", &call.replace("(): i32 => n", "named")))
            .expect("a clean argument passes");
    }
}

#[test]
fn carrier_type_predicate_checks_nested_and_plain_types() {
    let module = check("function named(): i32 { return 0; } class Holder { cb: () => i32 = named; } export function main(): void {}").unwrap();
    let id = module
        .classes
        .iter()
        .position(|c| c.name == "Holder")
        .map(subscript_compiler::types::ClassId)
        .unwrap();
    assert!(
        module.carries_capture(&subscript_compiler::Type::Array(Box::new(
            subscript_compiler::Type::Class(id)
        )))
    );
    assert!(
        !module.carries_capture(&subscript_compiler::Type::Array(Box::new(
            subscript_compiler::Type::I32
        )))
    );
}

#[test]
fn owned_capture_c_callback_parameter() {
    let source = "function install(cb: SubLogCallback): void { new SubCallbackInfo(cb, null, null); } export function main(): void { const value:i32=7; install((message, userdata, userparam):void=>{print(`${value}`);}); }";
    let result = check_program(&[
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("capture.ts", source),
    ]);
    assert!(
        result.is_err(),
        "a C callback parameter cannot receive a capture"
    );
}
