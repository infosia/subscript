use subscript_compiler::{check_program, hir, RuleCode, SourceFile};

#[test]
fn mirror_class_accessor_has_its_specific_rejection() {
    let diagnostics = check_program(&[
        SourceFile::ambient(
            "values.generated.d.ts",
            "declare class Values {\n  get current(): i32;\n}\n",
        ),
        SourceFile::new("main.ts", "export function main(): void {}\n"),
    ])
    .expect_err("a mirror class accessor must fail");

    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(diagnostics[0].pos.file, "values.generated.d.ts");
    assert_eq!(diagnostics[0].pos.line, 2);
    assert_eq!(
        diagnostics[0].message,
        "mirror classes cannot declare accessors"
    );
}

#[test]
fn write_accessor_requires_a_read_accessor() {
    let source = r#"
class Value {
  set current(value: i32) {}
}

export function main(): void {}
"#;
    let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
        .expect_err("a write-only accessor must fail");

    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(diagnostics[0].pos.line, 3);
    assert_eq!(
        diagnostics[0].message,
        "write accessor `current` requires a read accessor with the same name"
    );
}

#[test]
fn accessor_write_checks_the_value_type() {
    let source = r#"
class Value {
  value: i32 = 0;
  get current(): i32 { return this.value; }
  set current(value: i32) { this.value = value; }
}

export function main(): void {
  const value: Value = new Value();
  value.current = 1 as u32;
}
"#;
    let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
        .expect_err("a wrong accessor value type must fail");

    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S007);
    assert_eq!(diagnostics[0].pos.line, 10);
}

fn main_body(source: &str) -> Vec<hir::Stmt> {
    let module = check_program(&[SourceFile::new("identity.ts", source)])
        .expect("the accessor identity program must check");
    module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function")
        .body
        .clone()
}

#[test]
fn accessor_read_and_spelled_method_call_have_identical_hir() {
    // The aligned columns keep each HIR position equal across the two sources.
    let sugar = "class Value {\n  get name(): i32 { return 7; }\n}\nexport function main(): void {\n  const value: Value = new Value();\n  const read: i32 = value.  name;\n}\n";
    let spelled = "class Value {\n      name(): i32 { return 7; }\n}\nexport function main(): void {\n  const value: Value = new Value();\n  const read: i32 = value.name();\n}\n";

    assert_eq!(main_body(sugar), main_body(spelled));
}

fn one_accessor_diagnostic(source: &str) -> subscript_compiler::Diagnostic {
    let diagnostics = check_program(&[SourceFile::new("test.ts", source)])
        .expect_err("the invalid accessor must fail");
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    diagnostics.into_iter().next().expect("one diagnostic")
}

#[test]
fn write_accessor_rejects_a_return_type() {
    let diagnostic = one_accessor_diagnostic(
        "class Value {\n  get item(): i32 { return 0; }\n  set item(value: i32): string {}\n}\n",
    );
    assert_eq!(diagnostic.code, RuleCode::S100);
    assert_eq!(
        diagnostic.message,
        "a write accessor cannot declare a return type"
    );
}

#[test]
fn write_accessor_rejects_a_parameter_default() {
    let diagnostic = one_accessor_diagnostic(
        "class Value {\n  get item(): i32 { return 0; }\n  set item(value: i32 = 3) {}\n}\n",
    );
    assert_eq!(diagnostic.code, RuleCode::S100);
    assert_eq!(
        diagnostic.message,
        "a write accessor parameter cannot have a default"
    );
}

#[test]
fn accessor_pair_rejects_different_types_after_the_read_accessor() {
    let diagnostic = one_accessor_diagnostic(
        "class Value {\n  get item(): u8 { return 0; }\n  set item(value: i32) {}\n}\n",
    );
    assert_eq!(diagnostic.code, RuleCode::S100);
    assert_eq!(
        diagnostic.message,
        "the read and write accessors of `item` must have the same type"
    );
}

#[test]
fn accessor_pair_rejects_different_types_before_the_read_accessor() {
    let diagnostic = one_accessor_diagnostic(
        "class Value {\n  set item(value: i32) {}\n  get item(): u8 { return 0; }\n}\n",
    );
    assert_eq!(diagnostic.code, RuleCode::S100);
    assert_eq!(
        diagnostic.message,
        "the read and write accessors of `item` must have the same type"
    );
}

#[test]
fn class_rejects_a_second_read_accessor() {
    let diagnostic = one_accessor_diagnostic(
        "class Value {\n  get item(): i32 { return 0; }\n  get item(): i32 { return 1; }\n}\n",
    );
    assert_eq!(diagnostic.code, RuleCode::S017);
    assert_eq!(
        diagnostic.message,
        "two accessors cannot declare the read member `item`"
    );
}

#[test]
fn class_rejects_a_second_write_accessor() {
    let diagnostic = one_accessor_diagnostic(
        "class Value {\n  get item(): i32 { return 0; }\n  set item(value: i32) {}\n  set item(value: i32) {}\n}\n",
    );
    assert_eq!(diagnostic.code, RuleCode::S017);
    assert_eq!(
        diagnostic.message,
        "two accessors cannot declare the write member `item`"
    );
}
