//! Function storage and callback raise edges (compiler.md §118.2).
use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
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

/// Measured cost: 0.54 s for one short program in three tiers. This probes
/// generated adapter environments under mutation and collection; a461
/// already supplies the language-level corpus entry and Node golden.
#[test]
fn spread_adapters_capture_each_element_and_keep_the_source() {
    let start = std::time::Instant::now();
    let files = [SourceFile::new(
        "spread.ts",
        r#"
function none(): void { print("none"); }
function value(): i32 { print("value"); return 9; }
function one(x: i32): void { print(`one ${x}`); }
function two(x: i32): void { print(`two ${x}`); }
function source(): (() => i32)[] { print("source"); return [value, value]; }
export function main(): void {
  const base = [none, none];
  const fs = [...base, one];
  base[0] = value;
  Context.collect();
  fs[0](1); fs[1](2); fs[2](3);
  const more = [one, two];
  const gs = [none, ...more];
  gs[1](4); gs[2](5); more[0](6);
  const hs = [one, ...source()];
  Context.collect();
  hs[1](7); hs[2](8);
}
"#,
    )];
    let expected = b"none\nnone\none 3\none 4\ntwo 5\none 6\nsource\nvalue\nvalue\n";
    let hir = check_program(&files).expect("spread capture program checks");
    let lir = lower_module(&hir).expect("spread capture program lowers");
    for output in [
        run_jit(&files).expect("development runs"),
        run_c_aot(&files).expect("ship runs"),
        interpret(&lir).expect("interpreter runs"),
    ] {
        assert_eq!(output, expected);
    }
    eprintln!("spread capture three tiers: {:?}", start.elapsed());
}
