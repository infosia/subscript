//! Function values in initializer routes under compiler.md §137 rule 5b.
//! Cost: warm debug test execution 0.71 s, four ship-C program compiles.

#[path = "support/native_fixture.rs"]
mod native_fixture;

use subscript_codegen::{
    interpreter::interpret, lir::lower_module, run_c_aot_with_native_libraries,
    run_jit_with_native_libraries,
};
use subscript_compiler::{check_program, SourceFile};

fn all_engines(files: &[SourceFile], expected: &[u8]) {
    let module = check_program(files).expect("accepted initializer route");
    assert_eq!(
        interpret(&lower_module(&module).expect("LIR")).expect("interpreter"),
        expected
    );
    assert_eq!(
        run_jit_with_native_libraries(files, &[]).expect("JIT"),
        expected
    );
    assert_eq!(
        run_c_aot_with_native_libraries(files, &[]).expect("ship C"),
        expected
    );
}

#[test]
fn a_host_call_can_initialize_a_global() {
    host_program("const chain: SubChainHeader = new SubChainHeader(SubChainKind.SUB_CHAIN_KIND_BASE, null); const device: SubDevice = subDeviceCreate(chain); export function main(): void { subDeviceRelease(device); }", b"");
}

#[test]
fn a_direct_body_can_call_a_host_to_initialize_a_global() {
    host_program("function poll(): i32 { return subDevicePoll(1); } const first: i32 = poll(); export function main(): void { print(`${first}`); }", b"0\n");
}

fn host_program(source: &str, expected: &[u8]) {
    let files = [
        SourceFile::ambient(
            "interop.generated.d.ts",
            include_str!("../../corpus/interop/interop.generated.d.ts"),
        ),
        SourceFile::entry("main.ts", source),
    ];
    check_program(&files).expect("host initializer has no prior function value");
    let Some(fixture) = native_fixture::fixture() else {
        eprintln!("gate-skip: host initializer: native fixture unavailable");
        return;
    };
    let libraries = [fixture.library()];
    assert_eq!(
        run_jit_with_native_libraries(&files, &libraries).expect("JIT"),
        expected
    );
    assert_eq!(
        run_c_aot_with_native_libraries(&files, &libraries).expect("ship C"),
        expected
    );
}

#[test]
fn a_comparator_in_a_dependency_does_not_read_the_importer() {
    all_engines(&[
        SourceFile::entry("main.ts", "import { first } from './a'; let count: i32 = 0; export function main(): void { print(`${first() + count}`); }"),
        SourceFile::new("a.ts", "const sorted: i32[] = [3, 1, 2]; sorted.sort((a: i32, b: i32): i32 => a - b); export function first(): i32 { return sorted[0]; }"),
    ], b"1\n");
}

#[test]
fn a_map_callback_can_initialize_its_global() {
    all_engines(&[SourceFile::entry("main.ts", "const xs: i32[] = [1, 2]; const ys: i32[] = xs.map((x: i32): i32 => x + 1); export function main(): void { print(`${ys[0]} ${ys[1]}`); }")], b"2 3\n");
}
