//! Function storage and callback raise edges (compiler.md §118.2).
use subscript_codegen::{lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn named_functions_can_be_pushed_and_called_in_dev_and_ship() {
    let start = std::time::Instant::now();
    let files = [SourceFile::new(
        "array.ts",
        include_str!("../../corpus/accept/a263-array-of-functions-push.ts"),
    )];
    let module = check_program(&files).expect("the array checks");
    let store = module.functions.iter().find(|f| f.name == "store").unwrap();
    assert!(!store.can_raise, "push stores its argument");
    lower_module(&module).expect("the array lowers");
    // The reference interpreter cannot pack function values into arrays.
    for output in [
        run_jit(&files).expect("dev runs"),
        run_c_aot(&files).expect("ship runs"),
    ] {
        assert_eq!(output, b"2 7 7\n");
    }
    let control = check_program(&[SourceFile::new("callback.ts", "export function main(): void { const values: i32[] = [1]; values.forEach((n): void => { throw new Error(\"callback\"); }); }")]).expect("callback checks");
    assert!(
        control
            .functions
            .iter()
            .find(|f| f.name == "main")
            .unwrap()
            .can_raise
    );
    lower_module(&control).expect("callback raise edges verify");
    eprintln!("array function storage: {:?}", start.elapsed());
}
