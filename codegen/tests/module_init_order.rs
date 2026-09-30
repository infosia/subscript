//! Module initializers run in dependency post-order.
//! Cost: warm debug test execution 0.60 s, 7 ship-C program compiles.
//! These compiles check the type-only JSON and module initializers, regex ordering, and the accepted call route on all three engines.

use subscript_codegen::{
    interpreter::interpret, lir::lower_module, run_c_aot_with_native_libraries,
    run_jit_with_native_libraries,
};
use subscript_compiler::{check_program, SourceFile};

fn output(files: &[SourceFile]) -> String {
    let module = check_program(files).expect("valid module program");
    let lir = lower_module(&module).expect("module LIR");
    String::from_utf8(interpret(&lir).expect("module output")).expect("UTF-8 output")
}

#[test]
fn cycle_runs_when_each_module_finishes() {
    let files = [
        SourceFile::entry("main.ts", "import { x } from './x'; print('main'); export function main(): void { print(`${x}`); }"),
        SourceFile::new("x.ts", "import { y } from './y'; export const x: i32 = y + 1; print('x');"),
        SourceFile::new("y.ts", "import { x } from './x'; export const y: i32 = 1; print('y'); export function unused(): i32 { return x; }"),
    ];
    assert_eq!(output(&files), "y\nx\nmain\n2\n");
}

#[test]
fn type_specifier_import_runs_its_module() {
    let files = [
        SourceFile::entry("main.ts", "import { type A } from './a'; function take(value: A): void { print(`${value.value}`); } export function main(): void { print('1'); }"),
        SourceFile::new("a.ts", "export class A { value: i32 = 2; } print('a');"),
    ];
    assert_eq!(output(&files), "a\n1\n");
}

#[test]
fn sibling_imports_keep_source_order() {
    let files = [
        SourceFile::entry("main.ts", "import { b } from './b'; import { a } from './a'; print('main'); export function main(): void { print(`${a + b}`); }"),
        SourceFile::new("a.ts", "export const a: i32 = 1; print('a');"),
        SourceFile::new("b.ts", "export const b: i32 = 2; print('b');"),
    ];
    assert_eq!(output(&files), "b\na\nmain\n3\n");
}

#[test]
fn cycle_read_before_initializer_is_s100() {
    let files = [
        SourceFile::entry(
            "main.ts",
            "import { x } from './x'; export function main(): void { print(`${x}`); }",
        ),
        SourceFile::new(
            "x.ts",
            "import { value } from './y'; export const x: i32 = value + 1;",
        ),
        SourceFile::new(
            "y.ts",
            "import { x } from './x'; export const value: i32 = x + 1;",
        ),
    ];
    let errors = check_program(&files).expect_err("cycle reads a dependency global early");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message, "`x` is accessed before its declaration, directly from this initializer",
        "{errors:?}"
    );
}

#[test]
fn type_only_module_runs_initializers() {
    let files = [
        SourceFile::entry("main.ts", "import type { A } from './a'; function take(value: A): void {} export function main(): void {}"),
        SourceFile::new("a.ts", "export class A { value: i32 = 2; } const hidden: A = new A(); print('hidden');"),
    ];
    assert_eq!(all_engines(&files), vec!["hidden\n"; 3]);
    let invalid = [
        files[0].clone(),
        SourceFile::new(
            "a.ts",
            "export class A { value: i32 = 2; } const hidden: i32 = 'wrong'; print(1);",
        ),
    ];
    let errors = check_program(&invalid).expect_err("module still checks");
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(
        errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>(),
        [
            "type mismatch: the initializer expects `i32`, got `string`",
            "type mismatch: the argument expects `string`, got `i32`"
        ]
    );
}

#[test]
fn every_loaded_module_has_an_initializer_segment() {
    let files = [
        SourceFile::entry("main.ts", "export function main(): void {}"),
        SourceFile::new("empty.ts", ""),
        SourceFile::new("other.ts", "print('other');"),
    ];
    let module = check_program(&files).expect("loaded modules");
    assert_eq!(
        module.initializer_modules,
        ["main.ts", "empty.ts", "other.ts"]
    );
    assert_eq!(module.initializer_segments.len(), 3);
    assert_eq!(output(&files), "other\n");
}

#[test]
fn global_outside_initializer_segments_is_a_lowering_error() {
    let mut module = check_program(&[SourceFile::entry(
        "main.ts",
        "export function main(): void {}",
    )])
    .expect("base module");
    let separate = check_program(&[SourceFile::entry(
        "orphan.ts",
        "const orphan: i32 = 1; export function main(): void {}",
    )])
    .expect("global declaration");
    module.globals.push(separate.globals[0].clone());
    let error = lower_module(&module).expect_err("an unowned global must fail lowering");
    assert_eq!(
        error.message,
        "global `orphan` must have exactly one initializer owner; found 0"
    );
}

#[test]
fn json_type_only_class_methods_read_initialized_globals() {
    let files = [
        SourceFile::entry("main.ts", r#"import type { A } from './a'; export function main(): void { const a: A = JSON.parse<A>('{"value":1}'); print(`${a.value}`); a.greet(); }"#),
        SourceFile::new("a.ts", "const label: string = 'hello'; const nums: i32[] = [1, 2, 3]; export class A { value: i32 = 2; greet(): void { print(label); print(`${nums.length}`); } } print('a init');"),
    ];
    assert_eq!(all_engines(&files), vec!["a init\n1\nhello\n3\n"; 3]);
}

fn all_engines(files: &[SourceFile]) -> Vec<String> {
    vec![
        output(files),
        String::from_utf8(run_jit_with_native_libraries(files, &[]).expect("dev JIT")).unwrap(),
        String::from_utf8(run_c_aot_with_native_libraries(files, &[]).expect("ship C")).unwrap(),
    ]
}

#[test]
fn regex_from_type_only_generic_instantiation_runs_first() {
    let files = [
        SourceFile::entry("main.ts", "import type { A } from './t'; import { run } from './r'; export function main(): void { run(); }"),
        SourceFile::new("t.ts", "import { hit } from './g'; export class A {} function unused(): boolean { return hit<i32>(1); }"),
        SourceFile::new("r.ts", "import { hit } from './g'; export function run(): void { print(`${hit<i32>(1)}`); }"),
        SourceFile::new("g.ts", "export function hit<T>(v: T): boolean { return /a+/.test('aaa'); }"),
    ];
    assert_eq!(all_engines(&files), vec!["true\n"; 3]);
}

#[test]
fn regex_from_later_generic_instantiation_runs_first() {
    let files = [
        SourceFile::entry("main.ts", "import { run } from './r'; import { A } from './t'; export function main(): void { run(); new A(); }"),
        SourceFile::new("r.ts", "import { xv } from './x'; export function run(): void { print(`${xv}`); }"),
        SourceFile::new("t.ts", "import { hit } from './g'; export class A {} function unused(): boolean { return hit<i32>(1); }"),
        SourceFile::new("x.ts", "import { hit } from './g'; export const xv: boolean = hit<i32>(1);"),
        SourceFile::new("g.ts", "export function hit<T>(v: T): boolean { return /a+/.test('aaa'); }"),
    ];
    assert_eq!(all_engines(&files), vec!["true\n"; 3]);
}

#[test]
fn regex_in_later_function_runs_before_global_initializer() {
    let files = [SourceFile::entry(
        "main.ts",
        include_str!("../../corpus/accept/a301-regex-before-module.ts"),
    )];
    // The standing a301 corpus comparison already runs JIT and ship C.
    assert_eq!(output(&files), "true\n");
}

#[test]
fn regex_in_an_imported_module_checks_after_an_entry_statement() {
    let files = [
        SourceFile::entry("main.ts", "import { hit } from './b'; print('m'); export function main(): void { print(`${hit()}`); }"),
        SourceFile::new("b.ts", "export function hit(): boolean { return /a+/.test('aaa'); }"),
    ];
    assert_eq!(all_engines(&files), vec!["m\ntrue\n"; 3]);
}

#[test]
fn regex_in_a_later_run_module_checks_after_a_dependency_statement() {
    let files = [
        SourceFile::entry("main.ts", "import { hit } from './b'; print('m'); export function main(): void { print(`${hit()}`); }"),
        SourceFile::new("b.ts", "import { a } from './a'; export function hit(): boolean { return /a+/.test('aaa') && a === 1; }"),
        SourceFile::new("a.ts", "print('a'); export const a: i32 = 1;"),
    ];
    assert_eq!(all_engines(&files), vec!["a\nm\ntrue\n"; 3]);
}

#[test]
fn type_only_module_checks_initializer_order() {
    let files = [
        SourceFile::entry(
            "main.ts",
            "import type { A } from './a'; export function main(): void {}",
        ),
        SourceFile::new(
            "a.ts",
            "export class A {} const p: i32 = q; const q: i32 = 1;",
        ),
    ];
    let errors = check_program(&files).expect_err("type-only module must check source order");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message, "`q` is accessed before its declaration, directly from this initializer",
        "{errors:?}"
    );
}

#[test]
fn top_level_call_before_global_initializer_is_s100() {
    let files = [SourceFile::entry("main.ts", "class Foo { v: i32 = 1; } function hook(): void { print(`${m.v}`); } hook(); const m: Foo = new Foo(); export function main(): void {}")];
    let errors = check_program(&files).expect_err("top-level call reads an uninitialized global");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message, "`m` is accessed before its declaration, through `hook`",
        "{errors:?}"
    );
}

#[test]
fn cycle_top_level_call_before_global_initializer_is_s100() {
    let files = [
        SourceFile::entry("main.ts", "import { late } from './x'; class Foo { v: i32 = 1; } export function hook(): void { print(`${m.v}`); } const m: Foo = new Foo(); export function main(): void { print(`${late}`); }"),
        SourceFile::new("x.ts", "import { hook } from './main'; hook(); export const late: i32 = 1;"),
    ];
    let errors = check_program(&files).expect_err("cycle call reads an uninitialized global");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message, "`m` is accessed before its declaration, through `hook`",
        "{errors:?}"
    );
}

#[test]
fn top_level_call_after_global_initializer_runs() {
    let files = [SourceFile::entry("main.ts", "class Foo { v: i32 = 1; } function hook(): void { print(`${m.v}`); } const m: Foo = new Foo(); hook(); export function main(): void {}")];
    assert_eq!(all_engines(&files), vec!["1\n"; 3]);
}

#[test]
fn global_in_two_segments_is_a_lowering_error() {
    let mut module = check_program(&[
        SourceFile::entry(
            "main.ts",
            "const shared: i32 = 1; export function main(): void {}",
        ),
        SourceFile::new("other.ts", ""),
    ])
    .expect("two modules");
    // Build a second owner in the otherwise empty module.
    module.initializer_segments[1].globals.push(0);
    let error = lower_module(&module).expect_err("two owners must fail lowering");
    assert_eq!(
        error.message,
        "global `shared` must have exactly one initializer owner; found 2"
    );
}

#[test]
fn initializer_owner_of_missing_global_is_a_lowering_error() {
    let mut module = check_program(&[SourceFile::entry(
        "main.ts",
        "export function main(): void {}",
    )])
    .expect("empty module");
    module.initializer_segments[0].globals.push(0);
    let error = lower_module(&module).expect_err("missing global must fail lowering");
    assert_eq!(
        error.message,
        "initializer owner names missing global index 0"
    );
}

#[test]
fn a_global_position_outside_its_own_segment_is_a_lowering_error() {
    let mut module = check_program(&[
        SourceFile::entry(
            "main.ts",
            "const own: i32 = 1; export function main(): void {}",
        ),
        SourceFile::new("other.ts", "print('other');"),
    ])
    .expect("base segments");
    module.globals[0].initializer_index = 1;
    let error = lower_module(&module).expect_err("position belongs to another segment");
    assert_eq!(
        error.message,
        "global `own` initializer position 1 is outside its owner range 0..=0"
    );
}

#[test]
fn a_segment_past_the_module_body_is_a_lowering_error() {
    let mut module = check_program(&[SourceFile::entry(
        "main.ts",
        "export function main(): void {}",
    )])
    .expect("empty base module");
    module.initializer_modules.push("extra.ts".to_string());
    module
        .initializer_segments
        .push(subscript_compiler::hir::InitializerSegment::new(
            0..1,
            Vec::new(),
        ));
    let error = lower_module(&module).expect_err("segment has a missing statement");
    assert_eq!(
        error.message,
        "initializer segment range 0..1 is outside module body length 0"
    );
}

#[test]
fn a_reversed_segment_is_a_lowering_error() {
    let mut module = check_program(&[SourceFile::entry(
        "main.ts",
        "print('entry'); export function main(): void {}",
    )])
    .expect("base module");
    let start = module.top_level.len();
    module.initializer_segments[0].top_level = start..0;
    let error = lower_module(&module).expect_err("reversed range");
    assert_eq!(
        error.message,
        "initializer segment range 1..0 is outside module body length 1"
    );
}

#[test]
fn a_top_level_statement_without_a_segment_is_a_lowering_error() {
    let mut module = check_program(&[SourceFile::entry(
        "main.ts",
        "print('entry'); export function main(): void {}",
    )])
    .expect("base module");
    module.initializer_segments[0].top_level = 0..0;
    let error = lower_module(&module).expect_err("statement is unowned");
    assert_eq!(
        error.message,
        "top-level statement 0 must have exactly one initializer owner; found 0"
    );
}

#[test]
fn a_top_level_statement_in_two_segments_is_a_lowering_error() {
    let mut module = check_program(&[SourceFile::entry(
        "main.ts",
        "print('entry'); export function main(): void {}",
    )])
    .expect("base module");
    module.initializer_modules.push("other.ts".to_string());
    module
        .initializer_segments
        .push(subscript_compiler::hir::InitializerSegment::new(
            0..1,
            Vec::new(),
        ));
    let error = lower_module(&module).expect_err("statement has two owners");
    assert_eq!(
        error.message,
        "top-level statement 0 must have exactly one initializer owner; found 2"
    );
}

#[test]
fn segment_and_module_counts_must_agree() {
    let mut module = check_program(&[SourceFile::entry(
        "main.ts",
        "export function main(): void {}",
    )])
    .expect("base module");
    module.initializer_modules.push("other.ts".to_string());
    let error = lower_module(&module).expect_err("segment is missing");
    assert_eq!(
        error.message,
        "initializer segment count 1 differs from module count 2"
    );
}
