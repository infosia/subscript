//! The HIR→LIR lowering of a `Seq` (compiler.md §201.1 rule 4a).
//! Cost: three `check_program` and `lower_module` calls; no engine run.
//! The corpus entry a384 runs the form on the three engines.

use crate::lir::lower_module;
use subscript_compiler::{check_program, lir_text::print_module, SourceFile};

/// The operations of `main`, in order, without values and positions.
fn main_operations(body: &str) -> Vec<String> {
    let source = format!(
        "function g(): i32 {{ return 1; }}\nexport function main(): void {{\nconst xs: i32[] = [];\n{body}\n}}\n"
    );
    let module = check_program(&[SourceFile::new("seq.ts", source)]).expect("the program checks");
    let text = print_module(&lower_module(&module).expect("the program lowers"));
    let main = text
        .split("\nfn ")
        .find(|function| function.contains(" \"main\" "))
        .expect("main function");
    main.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            if line.starts_with("value ") || line.starts_with("fn") {
                return None;
            }
            if line.starts_with('b') && line.contains(':') && !line.contains('=') {
                return Some("block".to_owned());
            }
            let operation = line
                .split_once(" = ")
                .map_or(line, |(_, operation)| operation);
            let operation = operation.split(" @ ").next().unwrap_or(operation);
            let operation = operation.split(" traps=").next().unwrap_or(operation);
            let operation = operation.split(" invalidates=").next().unwrap_or(operation);
            Some(operation.to_owned())
        })
        .collect()
}

#[test]
fn value_position_push_lowers_as_its_effects_then_its_value() {
    let sequence = main_operations("const n = xs.push(g(), g());\nprint(`${n}`);");
    let spelled = main_operations(
        "let r: i32[];\nlet a: i32;\nlet b: i32;\nr = xs;\na = g();\nb = g();\nr.push(a);\nconst n = r.push(b);\nprint(`${n}`);",
    );
    assert_eq!(sequence, spelled);
    // The sequence adds no block, so it adds no branch.
    assert_eq!(sequence.iter().filter(|line| *line == "block").count(), 1);
    // Control: the spelled form that appends before the second call differs.
    let interleaved = main_operations(
        "let r: i32[];\nlet a: i32;\nlet b: i32;\nr = xs;\na = g();\nr.push(a);\nb = g();\nconst n = r.push(b);\nprint(`${n}`);",
    );
    assert_ne!(sequence, interleaved);
}
