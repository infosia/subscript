//! Namespace execution parity. Ship C compilation costs one compiler invocation per test.
#[allow(dead_code)]
#[path = "corpus/mod.rs"]
mod corpus;

use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn namespace_corpus_matches_three_engines() {
    let accept = corpus::corpus_accept();
    let id = "a318-namespace-import";
    let files = corpus::entry_sources(&accept, id);
    let expected = corpus::golden_bytes(&accept, id);
    let module = lower_module(&check_program(&files).unwrap()).unwrap();
    assert_eq!(interpret(&module).unwrap(), expected);
    assert_eq!(run_jit(&files).unwrap(), expected);
    assert_eq!(run_c_aot(&files).unwrap(), expected);
}

#[test]
fn local_shadow_runs_on_three_engines() {
    let files = [
        SourceFile::entry("main.ts", "import * as ns from './lib'; class Local { count: string = 'local'; } export function main(): void { const ns = new Local(); print(ns.count); }"),
        SourceFile::new("lib.ts", "export let count: i32 = 4;"),
    ];
    let module = lower_module(&check_program(&files).unwrap()).unwrap();
    assert_eq!(interpret(&module).unwrap(), b"local\n");
    assert_eq!(run_jit(&files).unwrap(), b"local\n");
    assert_eq!(run_c_aot(&files).unwrap(), b"local\n");
}

#[test]
fn namespace_cycle_keeps_dependency_initialization_order() {
    let files = [
        SourceFile::entry("main.ts", "import * as ns from './x'; print('main'); export function main(): void { print(`${ns.x}`); }"),
        SourceFile::new("x.ts", "import * as ns from './y'; export const x: i32 = ns.y + 1; print('x');"),
        SourceFile::new("y.ts", "import * as ns from './x'; export const y: i32 = 2; print('y');"),
    ];
    let module = lower_module(&check_program(&files).unwrap()).unwrap();
    let expected = b"y\nx\nmain\n3\n";
    assert_eq!(interpret(&module).unwrap(), expected);
    assert_eq!(run_jit(&files).unwrap(), expected);
    assert_eq!(run_c_aot(&files).unwrap(), expected);
    let mut reordered = files.to_vec();
    reordered.reverse();
    let reordered = lower_module(&check_program(&reordered).unwrap()).unwrap();
    assert_eq!(interpret(&reordered).unwrap(), expected);
}
