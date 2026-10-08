//! Whole-program escape contracts (compiler.md §118).
#[path = "corpus/interop.rs"]
#[allow(dead_code)]
mod interop;

use subscript_compiler::{check_program, hir, RuleCode, SourceFile};

fn check(source: &str) -> Result<hir::Module, Vec<subscript_compiler::Diagnostic>> {
    check_program(&[SourceFile::new("capture.ts", source)])
}
fn rejects(source: &str, boundary: &str) {
    let diagnostics = check(source).expect_err("the capture must reach the boundary");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == RuleCode::S009 && d.message.contains(boundary)),
        "{diagnostics:?}"
    );
}
fn store(body: &str, boundary: &str) {
    rejects(
        &format!(
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
        ),
        boundary,
    );
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
    rejects("function named(): i32 { return 0; } class Holder { cb: () => i32 = named; constructor(cb: () => i32) { cb(); } } export function main(): void { const value: i32 = 5; const holder = new Holder((): i32 => value); const set = new Set<Holder>(); set.add(holder); }", "Set store");
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
    rejects("function make(): () => i32 { const value: i32 = 5; return (): i32 => value; } export function main(): void {}", "return");
}
#[test]
fn expression_body_return() {
    store("const make = (): (() => i32) => cb;", "return");
}
#[test]
fn yield_boundary() {
    rejects("function* gen(): Generator<() => i32> { const value: i32 = 5; yield (): i32 => value; } export function main(): void {}", "yield");
}
#[test]
fn escaping_lambda_capture() {
    rejects("function make(): () => i32 { const value: i32 = 5; const cb = (): i32 => value; return (): i32 => cb(); } export function main(): void {}", "return");
}
#[test]
fn held_async_argument() {
    rejects("async function run(cb: () => i32): Promise<i32> { return cb(); } export async function main(): Promise<void> { const value: i32 = 5; const cb = (): i32 => value; const h = run(cb); await h; }", "held async argument");
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
            .any(|d| d.code == RuleCode::S009 && d.message.contains("aggregate constructor")),
        "{diagnostics:?}"
    );
}
#[test]
fn local_facts_ignore_source_order_and_loop_order() {
    for body in [
        "let alias: () => i32 = named; if (true) { return alias; } alias = (): i32 => value; return named;",
        "let alias: () => i32 = named; let second: () => i32 = named; for (let i: i32 = 0; i < 2; i++) { if (i === 1) { return alias; } alias = second; second = (): i32 => value; } return named;",
    ] {
        rejects(&format!("function named(): i32 {{ return 0; }} function make(): () => i32 {{ const value: i32 = 5; {body} }} export function main(): void {{}}"), "return");
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
                .escapes
        );
    }
    rejects(
        include_str!("../../corpus/reject/r247-generator-parameter-stored.ts"),
        "parameter `value`",
    );
}
#[test]
fn escape_inference_crosses_two_calls() {
    let source = "function last(cb: () => i32): () => i32 { return cb; } function middle(cb: () => i32): () => i32 { return last(cb); } function first(cb: () => i32): () => i32 { return middle(cb); } export function main(): void { const value: i32 = 5; first((): i32 => value); }";
    rejects(source, "call `first` parameter `cb`");
    check(&source.replace("(): i32 => value", "(): i32 => 5"))
        .expect("clean arguments pass the inferred contract");
}
#[test]
fn constructor_and_method_inference() {
    let source = "function named(): i32 { return 0; } class Holder { cb: () => i32 = named; constructor(cb: () => i32) { this.set(cb); } set(cb: () => i32): void { this.cb = cb; } } export function main(): void { const value: i32 = 5; new Holder((): i32 => value); }";
    rejects(source, "parameter `cb`");
    check(&source.replace("(): i32 => value", "named"))
        .expect("a clean constructor argument can reach a field");
}
#[test]
fn caller_argument_is_the_error() {
    rejects(
        include_str!("../../corpus/reject/r240-parameter-stored-in-field.ts"),
        "call `store` parameter `cb`",
    );
}
#[test]
fn indirect_calls_require_clean_arguments() {
    rejects(
        include_str!("../../corpus/reject/r248-capture-through-indirect-call.ts"),
        "indirect call argument",
    );
    check("function named(): i32 { return 5; } function call(cb: () => i32): i32 { return cb(); } export function main(): void { const indirect = call; indirect(named); }").expect("a clean indirect argument passes");
}
#[test]
fn call_and_downward_pass() {
    check(include_str!(
        "../../corpus/accept/a262-parameter-called-and-passed-down.ts"
    ))
    .expect("downward calls keep captures alive");
    rejects(
        include_str!("../../corpus/reject/r241-parameter-returned.ts"),
        "parameter `cb`",
    );
}
#[test]
fn direct_await_keeps_a_capture_alive() {
    let source = "async function run(cb: () => i32): Promise<i32> { await Context.suspend(); return cb(); } export async function main(): Promise<void> { const value: i32 = 5; const cb = (): i32 => value; await run(cb); }";
    check(source).expect("direct await keeps the caller alive");
    rejects(
        &source.replace("await run(cb);", "const h = run(cb); await h;"),
        "held async argument",
    );
}
#[test]
fn local_generator_keeps_a_capture_alive() {
    check("function* gen(cb: () => i32): Generator<i32> { yield cb(); } export function main(): void { const value: i32 = 5; const local = gen((): i32 => value); print(`${local.next().value}`); }").expect("the generator stays local");
    rejects(
        include_str!("../../corpus/reject/r243-capture-passed-to-generator.ts"),
        "return",
    );
}
#[test]
fn clean_generator_in_a_field() {
    check(include_str!(
        "../../corpus/accept/a217-generator-in-a-class-field.ts"
    ))
    .expect("a clean generator can enter storage");
    rejects(
        include_str!("../../corpus/reject/r247-generator-parameter-stored.ts"),
        "parameter `value`",
    );
}
#[test]
fn builtin_callbacks_stay_downward() {
    check("export function main(): void { const value: i32 = 5; const values: i32[] = [1, 2]; values.forEach((n): void => { print(`${n + value}`); }); const mapped = values.map((n): i32 => n + value); const filtered = values.filter((n): boolean => n < value); const total = values.reduce((sum, n): i32 => sum + n + value, 0); values.sort((a, b): i32 => a - b + value); }").expect("built-in callbacks do not escape");
    rejects(
        include_str!("../../corpus/reject/r248-capture-through-indirect-call.ts"),
        "indirect call argument",
    );
}

#[test]
fn nullable_parameter_inference() {
    let source = "function forward(cb: (() => i32) | null): (() => i32) | null { return cb; } export function main(): void { const value: i32 = 5; forward((): i32 => value); }";
    rejects(source, "call `forward` parameter `cb`");
    check(&source.replace("(): i32 => value", "null")).expect("null is clean");
}

#[test]
fn this_is_clean_but_a_captured_this_is_not() {
    let source = "function named(): i32 { return 1; } class B { cb: () => i32 = named; self(): B { return this; } keep(out: B[]): void { out.push(this); } } export function main(): void { const b = new B(); b.self(); b.keep([]); }";
    check(source).expect("this has clean fields");
    rejects(
        &source.replace(
            "self(): B { return this; }",
            "self(): () => B { const owner = this; return (): B => owner; }",
        ),
        "return",
    );
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
    rejects(
        &source.replace("return named;", "const n: i32 = 5; return (): i32 => n;"),
        "return",
    );
}

#[test]
fn defaults_join_parameter_facts() {
    let source = "function named(): i32 { return 1; } let saved: () => i32 = named; function f(a: () => i32, b: () => i32 = a): i32 { return a() + b(); } export function main(): void { const n: i32 = 5; f((): i32 => n); }";
    check(source).expect("a default passes a capture downward");
    let storing = source.replace("return a() + b();", "saved = b; return 0;");
    let diagnostics = check(&storing).expect_err("the default reaches storage");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == RuleCode::S009 && d.message.contains("call `f` parameter `a`")),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| !d.message.contains("initializer")),
        "{diagnostics:?}"
    );
    check(&storing.replace("(): i32 => n", "named")).expect("a clean caller passes");
}

#[test]
fn builtin_carrier_results_follow_the_receiver() {
    for (body, escapes) in [
        ("const v = values.shift(); return v;", true),
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
                .escapes,
            escapes
        );
    }
}

#[test]
fn diagnostics_qualify_constructors_and_methods() {
    let source = "function named(): i32 { return 0; } class K { cb: () => i32 = named; constructor(cb: () => i32) { this.cb = cb; } set(cb: () => i32): void { this.cb = cb; } } export function main(): void { const n: i32 = 5; const k = new K(named); CALL; }";
    for (call, name) in [
        ("new K((): i32 => n)", "K constructor"),
        ("k.set((): i32 => n)", "K.set"),
    ] {
        rejects(
            &source.replace("CALL", call),
            &format!("call `{name}` parameter `cb`"),
        );
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
