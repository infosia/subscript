//! Regex accessor and flag controls.
//! Cost: warm debug test execution 0.52 s, one ship-C program compile.

use subscript_codegen::{interpreter::interpret, lir::lower_module};
use subscript_compiler::{check_program, SourceFile};

#[test]
fn checked_sticky_is_false_for_every_accepted_flag_set() {
    let mut source = String::from("export function main(): void {");
    let mut expected = String::new();
    for mask in 0..128 {
        let flags: String = b"dgimsuv"
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, flag)| char::from(*flag))
            .collect();
        if flags.contains('u') && flags.contains('v') {
            continue;
        }
        let expression = if flags.contains('v') {
            format!("new RegExp(\"a\", \"{flags}\")")
        } else {
            format!("/a/{flags}")
        };
        source.push_str(&format!("print(`${{{expression}.sticky}}`);"));
        expected.push_str("false\n");
    }
    source.push('}');
    let checked = check_program(&[SourceFile::new("test.ts", source)]).expect("accepted flag sets");
    let lir = lower_module(&checked).expect("lower sticky accessors");
    assert_eq!(interpret(&lir).expect("sticky values"), expected.as_bytes());
}

#[test]
fn unicode_literal_and_unicode_sets_constructor_run_on_all_engines() {
    let files = [SourceFile::new(
        "test.ts",
        r#"export function main(): void {
  const literal: RegExp = /a/u;
  const constructor: RegExp = new RegExp("a", "v");
  print(`${literal.test("a")} ${literal.flags}`);
  print(`${constructor.test("a")} ${constructor.flags}`);
}"#,
    )];
    let checked = check_program(&files).expect("accepted regex controls");
    let lir = lower_module(&checked).expect("regex control LIR");
    let expected = b"true u\ntrue v\n";
    assert_eq!(interpret(&lir).expect("interpreter controls"), expected);
    assert_eq!(
        subscript_codegen::run_jit_with_native_libraries(&files, &[]).expect("dev JIT controls"),
        expected
    );
    assert_eq!(
        subscript_codegen::run_c_aot_with_native_libraries(&files, &[]).expect("ship C controls"),
        expected
    );
}
