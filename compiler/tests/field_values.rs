//! compiler.md §108.1: a declared field carries a value before the
//! constructor returns. A field has an initializer, or the constructor
//! assigns it at its top level, before every statement that holds a
//! `return`; otherwise the class is rejected at the field. A `!`
//! assertion does not satisfy the rule. Rule 3 keeps a mirror field, an
//! ambient declaration, a `@Descriptor` member, and a static field
//! outside.

use subscript_compiler::divergence::Divergence;
use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_program(&[SourceFile::new("main.ts", source)]).expect_err("the program must be rejected")
}

fn accepted(source: &str) {
    if let Err(diagnostics) = check_program(&[SourceFile::new("main.ts", source)]) {
        panic!("the program must be accepted: {diagnostics:?}");
    }
}

const MAIN: &str = "export function main(): void {}\n";

#[test]
fn an_unassigned_field_is_rejected_at_the_field_with_tsc_code_and_both_spellings() {
    let source = format!("class Counter {{\n  count: i32;\n}}\n{MAIN}");
    let diagnostics = diagnostics(&source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let first = &diagnostics[0];
    assert_eq!(first.code, RuleCode::S100);
    assert_eq!((first.pos.line, first.pos.col), (2, 3), "{}", first.message);
    assert_eq!(first.divergence, None);
    for needle in [
        "field `count` of `Counter`",
        "TS2564",
        "`count: i32 = \u{2026}`",
        "`this.count = \u{2026}` at the top level of the constructor",
    ] {
        assert!(
            first.message.contains(needle),
            "missing {needle:?}: {}",
            first.message
        );
    }
}

#[test]
fn every_unassigned_field_reports_once_and_an_initialized_field_reports_nothing() {
    let source =
        format!("class Pair {{\n  left: i32;\n  middle: i32 = 1;\n  right: i32;\n}}\n{MAIN}");
    let diagnostics = diagnostics(&source);
    let lines: Vec<u32> = diagnostics.iter().map(|d| d.pos.line).collect();
    assert_eq!(lines, [2, 4], "{diagnostics:?}");
}

#[test]
fn a_top_level_constructor_assignment_satisfies_the_rule() {
    accepted(&format!(
        "class Counter {{\n  count: i32;\n  constructor(count: i32) {{\n    this.count = count;\n  }}\n}}\n{MAIN}"
    ));
    // Rule 2 reads the top level only, and it reads a `return` before
    // the assignment. A preceding statement that holds no `return`
    // leaves the assignment in force.
    accepted(&format!(
        "class Counter {{\n  count: i32;\n  constructor(flag: boolean) {{\n    if (flag) {{\n      print(\"flag\");\n    }}\n    this.count = 1;\n  }}\n}}\n{MAIN}"
    ));
    // A `!` field that the constructor assigns at its top level carries a
    // value, so the assertion changes nothing.
    accepted(&format!(
        "class Counter {{\n  count!: i32;\n  constructor() {{\n    this.count = 1;\n  }}\n}}\n{MAIN}"
    ));
}

#[test]
fn a_nested_assignment_reports_the_nested_site_with_its_variant() {
    for (stem, body) in [
        ("one-arm", "if (flag) {\n      this.count = 1;\n    }"),
        (
            "both-arms",
            "if (flag) {\n      this.count = 1;\n    } else {\n      this.count = 2;\n    }",
        ),
        ("block", "{\n      this.count = 1;\n    }"),
        ("loop", "while (flag) {\n      this.count = 1;\n    }"),
    ] {
        let source = format!(
            "class Counter {{\n  count: i32;\n  constructor(flag: boolean) {{\n    {body}\n  }}\n}}\n{MAIN}"
        );
        let diagnostics = diagnostics(&source);
        assert_eq!(diagnostics.len(), 1, "{stem}: {diagnostics:?}");
        let first = &diagnostics[0];
        assert_eq!((first.pos.line, first.pos.col), (2, 3), "{stem}");
        assert_eq!(
            first.divergence,
            Some(Divergence::NestedFieldAssignment),
            "{stem}"
        );
        assert!(
            first.message.contains("inside a nested statement"),
            "{stem}: {}",
            first.message
        );
    }
}

#[test]
fn a_top_level_assignment_after_a_return_is_rejected_at_its_own_site() {
    // Rule 2: a top-level assignment counts only when no statement
    // before it holds a `return`. Measured with TypeScript 5.9.2 and the
    // repository `tsconfig.json`: `tsc` answers TS2564 for both forms, so
    // the site carries no divergence (§79 rules 2 and 4).
    for (stem, before) in [
        ("early-return", "if (flag) {\n      return;\n    }"),
        (
            "nested-return",
            "while (flag) {\n      if (flag) {\n        return;\n      }\n    }",
        ),
    ] {
        let source = format!(
            "class Counter {{\n  count: i32;\n  constructor(flag: boolean) {{\n    {before}\n    this.count = 1;\n  }}\n}}\n{MAIN}"
        );
        let diagnostics = diagnostics(&source);
        assert_eq!(diagnostics.len(), 1, "{stem}: {diagnostics:?}");
        let first = &diagnostics[0];
        assert_eq!(first.code, RuleCode::S100, "{stem}");
        assert_eq!((first.pos.line, first.pos.col), (2, 3), "{stem}");
        assert_eq!(first.divergence, None, "{stem}");
        for needle in ["field `count` of `Counter`", "holds a `return`", "TS2564"] {
            assert!(
                first.message.contains(needle),
                "{stem}: missing {needle:?}: {}",
                first.message
            );
        }
    }
}

#[test]
fn an_assignment_before_the_return_and_a_lambda_return_stay_accepted() {
    // The firing control of the test above: the same `return`, after the
    // assignment. `tsc` accepts both forms, measured.
    accepted(&format!(
        "class Counter {{\n  count: i32;\n  constructor(flag: boolean) {{\n    this.count = 1;\n    if (flag) {{\n      return;\n    }}\n    this.count = 2;\n  }}\n}}\n{MAIN}"
    ));
    // A lambda body is a separate function, so its `return` returns from
    // the lambda and rule 2 does not read it.
    accepted(&format!(
        "class Counter {{\n  count: i32;\n  constructor() {{\n    const double = (x: i32): i32 => x * 2;\n    this.count = double(2);\n  }}\n}}\n{MAIN}"
    ));
}

#[test]
fn a_compound_assignment_does_not_initialize() {
    let source = format!(
        "class Counter {{\n  count: i32;\n  constructor() {{\n    this.count += 1;\n  }}\n}}\n{MAIN}"
    );
    let diagnostics = diagnostics(&source);
    let field = diagnostics
        .iter()
        .find(|d| d.message.contains("field `count`"))
        .unwrap_or_else(|| panic!("no field diagnostic: {diagnostics:?}"));
    assert_eq!((field.pos.line, field.pos.col), (2, 3));
    assert_eq!(field.divergence, None);
    assert!(field.message.contains("TS2564"), "{}", field.message);
}

#[test]
fn a_definite_assignment_assertion_reports_its_variant() {
    let source =
        format!("class Inner {{\n  v: i32 = 3;\n}}\nclass Holder {{\n  inner!: Inner;\n}}\n{MAIN}");
    let diagnostics = diagnostics(&source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let first = &diagnostics[0];
    assert_eq!((first.pos.line, first.pos.col), (5, 3));
    assert_eq!(
        first.divergence,
        Some(Divergence::DefiniteAssignmentAssertion)
    );
    assert!(first.message.contains("`!`"), "{}", first.message);
}

#[test]
fn a_value_class_field_is_reached() {
    let source = format!("@CStruct\nclass Vec2 {{\n  x: f32;\n  y: f32 = 0;\n}}\n{MAIN}");
    let diagnostics = diagnostics(&source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].pos.line, diagnostics[0].pos.col), (3, 3));
}

#[test]
fn a_mirror_field_is_outside_the_rule_and_a_program_field_is_not() {
    let mirror = "declare class Device {\n  handle: i32;\n}\n";
    let program = "export function main(): void {\n  const d: Device = new Device();\n  print(`${d.handle}`);\n}\n";
    let files = [
        SourceFile::ambient("device.d.ts", mirror),
        SourceFile::new("main.ts", program),
    ];
    if let Err(diagnostics) = check_program(&files) {
        panic!("a mirror field carries no initializer: {diagnostics:?}");
    }
    // Firing control: the same shape as a program class is rejected.
    let source = "class Device {\n  handle: i32;\n}\nexport function main(): void {\n  const d: Device = new Device();\n  print(`${d.handle}`);\n}\n";
    let diagnostics = diagnostics(source);
    assert_eq!((diagnostics[0].pos.line, diagnostics[0].pos.col), (2, 3));
}

#[test]
fn a_declare_class_in_a_program_file_is_outside_the_rule() {
    // `tsc` exempts every ambient declaration from TS2564, and the class
    // has no constructor body that a rule could read.
    accepted(
        "declare class Ext {\n  x: i32;\n}\nexport function main(): void {\n  print(\"ok\");\n}\n",
    );
}

#[test]
fn a_descriptor_member_is_outside_the_rule() {
    accepted(
        "@Descriptor\nclass Options {\n  id!: i32;\n  size?: i32 = 64;\n}\nexport function main(): void {\n  const o: Options = { id: 7 };\n  print(`${o.id} ${o.size}`);\n}\n",
    );
    // Firing control: the required-member spelling on an unmarked class
    // is the `!` shape, which the rule rejects.
    let diagnostics = diagnostics(&format!("class Options {{\n  id!: i32;\n}}\n{MAIN}"));
    assert_eq!(
        diagnostics[0].divergence,
        Some(Divergence::DefiniteAssignmentAssertion)
    );
}

#[test]
fn a_static_field_keeps_its_own_rule_and_reports_once() {
    let source = format!("class Registry {{\n  static count: i32;\n}}\n{MAIN}");
    let diagnostics = diagnostics(&source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "static fields require an initializer"
    );
}

#[test]
fn a_generic_class_is_checked_per_instance() {
    accepted(
        "class Box<T> {\n  value: T;\n  constructor(value: T) {\n    this.value = value;\n  }\n}\nexport function main(): void {\n  const b: Box<i32> = new Box<i32>(1);\n  print(`${b.value}`);\n}\n",
    );
    // One instance, one diagnostic, at the template's field.
    let one = diagnostics(
        "class Box<T> {\n  value: T;\n}\nexport function main(): void {\n  const b: Box<i32> = new Box<i32>();\n  print(`${b.value}`);\n}\n",
    );
    let sites: Vec<(u32, u32)> = one.iter().map(|d| (d.pos.line, d.pos.col)).collect();
    assert_eq!(sites, [(2, 3)], "{one:?}");
    // Two instances of one template report twice, at the same position.
    // The rule runs per instance, so the count is the instance count.
    let two = diagnostics(
        "class Box<T> {\n  value: T;\n}\nexport function main(): void {\n  const a: Box<i32> = new Box<i32>();\n  const b: Box<string> = new Box<string>();\n  print(`${a.value} ${b.value}`);\n}\n",
    );
    let sites: Vec<(u32, u32)> = two.iter().map(|d| (d.pos.line, d.pos.col)).collect();
    assert_eq!(sites, [(2, 3), (2, 3)], "{two:?}");
    // The advice names the declared type of the template, not the type
    // of the instance, and its placeholder names no field.
    for diagnostic in &two {
        assert!(
            diagnostic.message.contains("`value: T = \u{2026}`"),
            "{}",
            diagnostic.message
        );
    }
}

#[test]
fn a_generic_declare_class_inherits_the_template_ambient_status() {
    // Rule 3: an instance of an ambient template is ambient. Measured
    // with TypeScript 5.9.2 and the repository `tsconfig.json`: `tsc`
    // accepts the ambient form and answers TS2564 for the program form.
    accepted(
        "declare class Ext<T> {\n  value: T;\n}\nexport function read(e: Ext<i32>): i32 {\n  return e.value;\n}\nexport function main(): void {\n  print(\"ok\");\n}\n",
    );
    // Firing control: the same template without `declare`.
    let diagnostics = diagnostics(
        "class Ext<T> {\n  value: T;\n}\nexport function read(e: Ext<i32>): i32 {\n  return e.value;\n}\nexport function main(): void {\n  print(\"ok\");\n}\n",
    );
    let sites: Vec<(u32, u32)> = diagnostics
        .iter()
        .map(|d| (d.pos.line, d.pos.col))
        .collect();
    assert_eq!(sites, [(2, 3)], "{diagnostics:?}");
}
