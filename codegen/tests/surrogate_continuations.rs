//! Section 96.1: LF and CRLF continuations across all execution witnesses.
use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn surrogate_continuations_match_in_all_three_witnesses() {
    let source = include_str!("../../corpus/accept/a198-surrogate-escapes.ts");
    let expected = include_bytes!("../../corpus/accept/a198-surrogate-escapes.expected");
    assert!(source.contains("\\\n"));
    assert!(!source.contains('\r'));
    for (name, source) in [
        ("LF", source.to_owned()),
        ("CRLF", source.replace('\n', "\r\n")),
    ] {
        let files = [SourceFile::new("surrogate-continuations.ts", source)];
        let hir = check_program(&files).expect("surrogate source checks");
        let lir = lower_module(&hir).expect("surrogate source lowers");
        for (witness, output) in [
            ("dev JIT", run_jit(&files).expect("JIT runs")),
            ("ship C AOT", run_c_aot(&files).expect("C AOT runs")),
            ("interpreter", interpret(&lir).expect("interpreter runs")),
        ] {
            assert_eq!(output, expected, "{name}: {witness}");
            println!(
                "section96 {name} {witness}: {} bytes match a198 golden",
                output.len()
            );
        }
    }
}
