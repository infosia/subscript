#[allow(dead_code)]
#[path = "corpus/mod.rs"]
mod corpus;

use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::check_program;

#[test]
fn inferred_calls_match_explicit_calls_on_three_forms() {
    let root = corpus::corpus_accept();
    let files = corpus::entry_sources(&root, "a324-generic-inference");
    let expected = corpus::golden_bytes(&root, "a324-generic-inference");
    let lir = lower_module(&check_program(&files).unwrap()).unwrap();
    assert_eq!(interpret(&lir).unwrap(), expected);
    assert_eq!(run_jit(&files).unwrap(), expected);
    assert_eq!(run_c_aot(&files).unwrap(), expected);
}
