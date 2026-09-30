//! Total initializer routes under compiler.md §137 rules 5b and 5c.
//! Cost: warm debug test execution 0.98 s, 15 ship-C program compiles.
//! Each accepted control checks the runtime route on the engines; the foreign control checks the native callback on JIT and ship C.

#[path = "support/native_fixture.rs"]
mod native_fixture;

use subscript_codegen::{
    interpreter::interpret, lir::lower_module, run_c_aot_with_native_libraries,
    run_jit_with_native_libraries,
};
use subscript_compiler::{check_program, RuleCode, SourceFile};

fn rejected(files: &[SourceFile], message: &str) {
    let errors = match check_program(files) {
        Ok(_) => panic!("expected S100: {message}; checker accepted the program"),
        Err(errors) => errors,
    };
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(errors[0].message, message);
}

fn all_engines(source: &str, expected: &str) {
    let files = [SourceFile::entry("main.ts", source)];
    let hir = check_program(&files).expect("accepted route control");
    let lir = lower_module(&hir).expect("control LIR");
    assert_eq!(interpret(&lir).expect("interpreter"), expected.as_bytes());
    assert_eq!(
        run_jit_with_native_libraries(&files, &[]).expect("JIT"),
        expected.as_bytes()
    );
    assert_eq!(
        run_c_aot_with_native_libraries(&files, &[]).expect("ship C"),
        expected.as_bytes()
    );
}

#[test]
fn using_dispose_route_rejects_early_read_and_runs_after_global() {
    let definitions = "class Foo { v: i32 = 1; } class R { [Symbol.dispose](): void { print(`d ${m.v}`); } } function scope(): void { using r: R = new R(); print('in'); }";
    let global = "const m: Foo = new Foo();";
    let early = format!("{definitions} scope(); {global} export function main(): void {{}}");
    rejected(
        &[SourceFile::entry("main.ts", early)],
        "`m` is accessed before its declaration, through `scope` -> `R.[[Symbol.dispose]]`",
    );
    all_engines(
        &format!("{definitions} {global} scope(); export function main(): void {{}}"),
        "in\nd 1\n",
    );
}

#[test]
fn foreign_callback_rejects_early_read_and_runs_after_global() {
    let definitions = "class Foo { v: i32 = 1; }";
    let global = "const late: Foo = new Foo();";
    let block = "{ const chain: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_BASE, null); const device: SubDevice = subDeviceCreate(chain); const info: SubCallbackInfo = new SubCallbackInfo((message, userdata1, userdata2) => { print(`f ${late.v}`); }, null, null); subDeviceSetLogger(device, info); subDeviceRelease(device); }";
    let mirror = SourceFile::ambient(
        "interop.generated.d.ts",
        include_str!("../../corpus/interop/interop.generated.d.ts"),
    );
    let sources = |body: String| [mirror.clone(), SourceFile::entry("main.ts", body)];
    rejected(
        &sources(format!(
            "{definitions} {block} {global} export function main(): void {{}}"
        )),
        "`late` is accessed before its declaration, through an indirect call",
    );
    let files = sources(format!(
        "{definitions} {global} {block} export function main(): void {{}}"
    ));
    let Some(fixture) = native_fixture::fixture() else {
        eprintln!("gate-skip: foreign callback control: native fixture unavailable");
        return;
    };
    let libraries = [fixture.library()];
    assert_eq!(
        run_jit_with_native_libraries(&files, &libraries).expect("JIT callback"),
        b"f 1\n"
    );
    assert_eq!(
        run_c_aot_with_native_libraries(&files, &libraries).expect("ship callback"),
        b"f 1\n"
    );
}

#[test]
fn for_each_literal_reads_only_its_direct_callback_body() {
    let global = "const later: i32 = 1;";
    let setup = "const xs: i32[] = [1];";
    let call = "xs.forEach((value: i32): void => { print(`${value + later}`); });";
    rejected(
        &[SourceFile::entry(
            "main.ts",
            format!("{setup} {call} {global} export function main(): void {{}}"),
        )],
        "`later` is accessed before its declaration, through a lambda",
    );
    all_engines(
        &format!("{setup} xs.forEach((value: i32): void => {{ print(`${{value}}`); }}); {global} export function main(): void {{}}"),
        "1\n",
    );
}

#[test]
fn stored_function_indirect_call_rejects_later_global_and_runs_after_it() {
    let definitions = "function work(): void { print(`${later}`); }";
    let setup = "const stored: () => void = work;";
    let global = "const later: i32 = 1;";
    rejected(
        &[SourceFile::entry(
            "main.ts",
            format!("{definitions} {setup} stored(); {global} export function main(): void {{}}"),
        )],
        "`later` is accessed before its declaration, through an indirect call",
    );
    all_engines(
        &format!("function work(): void {{ print('stored'); }} {setup} stored(); {global} export function main(): void {{}}"),
        "stored\n",
    );
}

#[test]
fn lambda_argument_indirect_call_rejects_later_global_and_runs_after_it() {
    let definitions = "function apply(cb: () => i32): void { print(`${cb()}`); }";
    let call = "apply((): i32 => later);";
    let global = "const later: i32 = 1;";
    rejected(
        &[SourceFile::entry(
            "main.ts",
            format!("{definitions} {call} {global} export function main(): void {{}}"),
        )],
        "`later` is accessed before its declaration, through `apply` -> an indirect call",
    );
    all_engines(
        &format!("{definitions} apply((): i32 => 3); {global} export function main(): void {{}}"),
        "3\n",
    );
}

#[test]
fn async_direct_call_scans_global_read_after_first_await() {
    let definitions =
        "async function go(): Promise<void> { await Context.suspend(); print(`${m}`); }";
    let global = "const m: i32 = 1;";
    rejected(
        &[SourceFile::entry(
            "main.ts",
            format!("{definitions} go(); {global} export function main(): void {{}}"),
        )],
        "`m` is accessed before its declaration, through `go`",
    );
    all_engines(
        &format!("{definitions} {global} go(); export function main(): void {{}}"),
        "1\n",
    );
}

#[test]
fn declared_async_call_does_not_read_unrelated_globals() {
    all_engines("async function go(): Promise<void> { await Context.suspend(); print('go'); } go(); const later: i32 = 1; export function main(): void {}", "go\n");
}

#[test]
fn generator_creation_makes_a_unit_and_steps_scan_its_body() {
    let definitions = "function* values(): Generator<i32> { print(`${later}`); yield later; }";
    rejected(&[SourceFile::entry("main.ts", format!("{definitions} const live: Generator<i32> = values(); live.next(); const later: i32 = 2; export function main(): void {{}}"))], "`later` is accessed before its declaration, through an indirect call");
    all_engines(&format!("{definitions} const live: Generator<i32> = values(); const later: i32 = 2; export function main(): void {{ print(`${{live.next().value}}`); }}"), "2\n2\n");
}

#[test]
fn shadowed_generator_parameter_cannot_hide_body_read() {
    let definitions = "class Foo { v: i32 = 7; } function* safe(): Generator<i32> { yield 1; } function* bad(): Generator<i32> { yield m.v; } function step(g: Generator<i32>): i32 { { const g: Generator<i32> = safe(); g.next(); } const r = g.next(); return r.done ? 0 : r.value; }";
    let early = "const early: i32 = step(bad());";
    let global = "const m: Foo = new Foo();";
    let main = "export function main(): void { print(`${early}`); }";
    rejected(
        &[SourceFile::entry(
            "main.ts",
            format!("{definitions} {early} {global} {main}"),
        )],
        "`m` is accessed before its declaration, through `step` -> an indirect call",
    );
    all_engines(&format!("{definitions} {global} {early} {main}"), "7\n");
}

#[test]
fn generator_parameter_for_of_scans_the_made_body() {
    rejected(&[SourceFile::entry("main.ts", "function* counting(): Generator<i32> { yield later; } function sum(g: Generator<i32>): i32 { let t: i32 = 0; for (const v of g) { t += v; } return t; } const s: i32 = sum(counting()); const later: i32 = 3; export function main(): void {}")], "`later` is accessed before its declaration, through `sum` -> an indirect call");
    all_engines("function* counting(): Generator<i32> { yield 1; yield 2; } function sum(g: Generator<i32>): i32 { let t: i32 = 0; for (const v of g) { t += v; } return t; } const s: i32 = sum(counting()); const later: i32 = 3; export function main(): void { print(`${s + later}`); }", "6\n");
}

#[test]
fn generator_parameter_next_scans_the_made_body() {
    rejected(&[SourceFile::entry("main.ts", "function* values(): Generator<i32> { yield later; } function step(value: Generator<i32>): i32 { return value.next().value; } const early: i32 = step(values()); const later: i32 = 1; export function main(): void {}")], "`later` is accessed before its declaration, through `step` -> an indirect call");
    all_engines("function* values(): Generator<i32> { yield 2; } function step(value: Generator<i32>): i32 { return value.next().value; } const early: i32 = step(values()); const later: i32 = 1; export function main(): void { print(`${early + later}`); }", "3\n");
}

#[test]
fn values_made_in_followed_bodies_reach_later_indirect_calls() {
    let definitions = "function make(): void { const cb: () => i32 = (): i32 => later; } function apply(cb: () => i32): void { print(`${cb()}`); }";
    rejected(
        &[SourceFile::entry("main.ts", format!("{definitions} make(); apply((): i32 => 3); const later: i32 = 1; export function main(): void {{}}"))],
        "`later` is accessed before its declaration, through `apply` -> an indirect call",
    );
    all_engines(
        "function make(): void { const cb: () => i32 = (): i32 => 2; } function apply(cb: () => i32): void { print(`${cb()}`); } make(); apply((): i32 => 3); const later: i32 = 1; export function main(): void {}",
        "3\n",
    );
}

#[test]
fn an_indirect_call_follows_new_values_to_a_fixed_point() {
    rejected(
        &[SourceFile::entry("main.ts", "function make(): void { const cb: () => i32 = (): i32 => later; } const stored: () => void = make; stored(); const later: i32 = 1; export function main(): void {}")],
        "`later` is accessed before its declaration, through an indirect call",
    );
    all_engines("function make(): void { const cb: () => i32 = (): i32 => 2; } const stored: () => void = make; stored(); const later: i32 = 1; export function main(): void { print(`${later}`); }", "1\n");
}

#[test]
fn lambda_creation_alone_does_not_read_its_body() {
    all_engines("const cb: () => i32 = (): i32 => later; const later: i32 = 4; export function main(): void { print(`${cb()}`); }", "4\n");
    rejected(&[SourceFile::entry("main.ts", "const cb: () => i32 = (): i32 => later; cb(); const later: i32 = 4; export function main(): void {}")], "`later` is accessed before its declaration, through an indirect call");
}

#[test]
fn a_later_lambda_is_not_followed_by_an_earlier_call() {
    all_engines("function apply(cb: () => i32): void { print(`${cb()}`); } apply((): i32 => 3); const cb: () => i32 = (): i32 => later; const later: i32 = 1; export function main(): void {}", "3\n");
    rejected(&[SourceFile::entry("main.ts", "function apply(cb: () => i32): void { print(`${cb()}`); } const cb: () => i32 = (): i32 => later; apply((): i32 => 3); const later: i32 = 1; export function main(): void {}")], "`later` is accessed before its declaration, through `apply` -> an indirect call");
}

#[test]
fn a_registered_callback_remains_available_at_a_later_host_call() {
    let mirror = SourceFile::ambient(
        "interop.generated.d.ts",
        include_str!("../../corpus/interop/interop.generated.d.ts"),
    );
    let files = [mirror, SourceFile::entry("main.ts", "class Foo { v: i32 = 1; } const chain: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_BASE, null); const device: SubDevice = subDeviceCreate(chain); const info: SubCallbackInfo = new SubCallbackInfo((message, userdata1, userdata2) => { print(`${late.v}`); }, null, null); subDeviceSetLogger(device, info); subDevicePoll(1); const late: Foo = new Foo(); export function main(): void { subDeviceRelease(device); }")];
    let errors = check_program(&files).expect_err("registered callback reads late");
    assert_eq!(errors.len(), 2, "{errors:?}");
    for error in errors {
        assert_eq!(error.code, RuleCode::S100);
        assert_eq!(
            error.message,
            "`late` is accessed before its declaration, through an indirect call"
        );
    }
}
