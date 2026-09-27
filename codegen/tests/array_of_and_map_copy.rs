use subscript_codegen::{interpreter::interpret, lir::lower_module, run_c_aot, run_jit};
use subscript_compiler::{check_program, lir_text::print_module, SourceFile};

#[test]
fn array_of_has_the_literal_lir_text() {
    let lower = |value: &str| {
        let files = [SourceFile::new("test.ts", format!("export function main(): void {{ const xs: i32[] = {value};\n print(`${{xs[0]}}`); }}"))];
        print_module(&lower_module(&check_program(&files).unwrap()).unwrap())
    };
    assert_eq!(lower("Array.of<i32>(\n7\n)"), lower("[\n7\n]"));
    assert_ne!(lower("[\n8\n]"), lower("[\n7\n]"));
}

#[test]
fn corpus_matches_on_three_engines() {
    let files = [SourceFile::new(
        "test.ts",
        include_str!("../../corpus/accept/a269-array-of-and-map-copy.ts"),
    )];
    let expected = include_bytes!("../../corpus/accept/a269-array-of-and-map-copy.expected");
    let module = lower_module(&check_program(&files).unwrap()).unwrap();
    assert_eq!(interpret(&module).unwrap(), expected);
    assert_eq!(run_jit(&files).unwrap(), expected);
    assert_eq!(run_c_aot(&files).unwrap(), expected);
}

#[test]
fn construction_evaluates_each_source_once() {
    let files = [SourceFile::new(
        "test.ts",
        r#"
        function source(): Map<i32, i32> { print("map"); return new Map<i32, i32>(); }
        function value(): i32 { print("value"); return 7; }
        export function main(): void {
            const map = new Map(source());
            const array = Array.of(value());
            const empty: i32[] = Array.of();
            print(`${map.size} ${array[0]} ${empty.length}`);
        }
    "#,
    )];
    let module = lower_module(&check_program(&files).unwrap()).unwrap();
    let expected = b"map\nvalue\n0 7 0\n";
    assert_eq!(interpret(&module).unwrap(), expected);
    assert_eq!(run_jit(&files).unwrap(), expected);
    assert_eq!(run_c_aot(&files).unwrap(), expected);
}
