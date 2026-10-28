//! §206.1 rule 3: `#x` and `x` are two members of one class, and their
//! C names are different. A C member name comes from the module-unique
//! field id, so no source name gives the C name of `#x`.
//!
//! Cost: one check, one LIR lowering, and one C transcription of a
//! six-line program, with no C compiler run.
use subscript_compiler::{check_program, SourceFile};

const SOURCE: &str = "class P {
  #x: i32 = 1;
  x: i32 = 2;
  sum(): i32 { return this.#x + this.x; }
}
export function main(): void { print(`${new P().sum()}`); }
";

#[test]
fn a_private_name_and_its_plain_name_are_two_fields_with_different_c_names() {
    let hir = check_program(&[SourceFile::new("private.ts", SOURCE)]).expect("checks");
    let module = crate::lir::lower_module(&hir).expect("lowers");
    let class = module
        .classes
        .iter()
        .find(|class| class.source_name == "P")
        .expect("class P");
    let names: Vec<&str> = class
        .fields
        .iter()
        .map(|field| field.source_name.as_str())
        .collect();
    assert_eq!(names, ["#x", "x"]);
    let program = crate::emit_c(&hir).expect("emits C");
    let opening = format!("struct SubC{} {{\n", class.id.0);
    let start = program.source.find(&opening).expect("struct definition") + opening.len();
    let end = start + program.source[start..].find("};").expect("struct end");
    let members: Vec<&str> = program.source[start..end]
        .lines()
        .map(|line| line.trim().trim_end_matches(';'))
        .map(|line| line.rsplit(' ').next().expect("member name"))
        .collect();
    let expected: Vec<String> = class
        .fields
        .iter()
        .map(|field| format!("d{}", field.id.0))
        .collect();
    assert_eq!(members, expected);
    assert_ne!(members[0], members[1]);
}
