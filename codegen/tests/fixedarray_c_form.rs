//! Each `FixedArray` type has a C form (compiler.md §254 rule 2). The
//! accept entry `a438` holds the forms of an empty array and a `Map` on the
//! three tiers.
//!
//! Cost: one program on the development tier and the ship tier, one C
//! compile and one run; 0.20 s on an arm64 host, 0.41 s on the first run
//! of a new build.

use subscript_codegen::run_c_aot;
use subscript_compiler::{check_program, SourceFile};

/// Rule 2: a nested `FixedArray` type whose only use is the element of an
/// empty array compiles: the element type of the element has its `typedef`
/// too, before the outer one.
#[test]
fn a_nested_runtime_held_element_type_has_a_typedef() {
    let source = "export function main(): void { const n: FixedArray<FixedArray<u8, 2>, 2>[][] = []; print(`${n.length}`); }";
    let files = [SourceFile::entry("fixed.ts", source)];
    check_program(&files).expect("checks clean");
    let jit = subscript_codegen::run_jit(&files).expect("development tier");
    let ship = run_c_aot(&files).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(ship, b"0\n".to_vec());
    assert_eq!(jit, ship);
}
