//! compiler.md §108.1: a declared field carries a value before the
//! constructor returns. A field has an initializer, or the constructor
//! assigns it at its top level, before every statement that holds a
//! `return`; otherwise the class is rejected at the field. A `!`
//! assertion does not satisfy the rule. Rule 3 keeps a mirror field, an
//! ambient declaration, a `@Descriptor` member, and a static field
//! outside.
//!
//! compiler.md §108.4 adds two rules. Rule 5 rejects `new` on an
//! ambient class that is not a mirror. Rule 6 rejects every `this`
//! inside the constructor's assignment prefix except the target of
//! `this.f = …` and a read of a field that holds a value.

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

#[test]
fn new_on_a_program_file_declare_class_reports_the_ambient_variant() {
    // compiler.md §108.4 rule 5: a `declare class` in a program file has
    // no constructor body and no positional store, so no argument
    // reaches a field.
    let source = "declare class Ext {\n  value: i32;\n}\nexport function main(): void {\n  const ext: Ext = new Ext();\n  print(`${ext.value}`);\n}\n";
    let reported = diagnostics(source);
    assert_eq!(reported.len(), 1, "{reported:?}");
    let first = &reported[0];
    assert_eq!(first.code, RuleCode::S100);
    assert_eq!(
        (first.pos.line, first.pos.col),
        (5, 20),
        "{}",
        first.message
    );
    assert_eq!(
        first.divergence,
        Some(Divergence::AmbientClassConstruction),
        "{}",
        first.message
    );
    for needle in [
        "ambient class `Ext`",
        "obtained from the host, not constructed",
        "no constructor body",
    ] {
        assert!(
            first.message.contains(needle),
            "missing {needle:?}: {}",
            first.message
        );
    }
    // A generic instance reaches the same site through its template.
    let generic = diagnostics(
        "declare class Ext<T> {\n  value: T;\n}\nexport function main(): void {\n  const ext: Ext<i32> = new Ext<i32>();\n  print(`${ext.value}`);\n}\n",
    );
    assert_eq!(generic.len(), 1, "{generic:?}");
    assert_eq!(
        generic[0].divergence,
        Some(Divergence::AmbientClassConstruction)
    );
    // Firing control: a mirror class stays constructible, because
    // `lower_new` stores every argument into the field at its position.
    let mirror = "declare class Device {\n  handle: i32;\n}\n";
    let program =
        "export function main(): void {\n  const d: Device = new Device();\n  print(`${d.handle}`);\n}\n";
    if let Err(diagnostics) = check_program(&[
        SourceFile::ambient("device.d.ts", mirror),
        SourceFile::new("main.ts", program),
    ]) {
        panic!("a mirror class is constructed positionally: {diagnostics:?}");
    }
}

#[test]
fn a_read_before_the_assignment_reports_the_this_without_a_variant() {
    // compiler.md §108.4 rule 6 site A. Stock `tsc` answers TS2565 for
    // this form, so the site carries no variant.
    let source = format!(
        "class Inner {{\n  value: i32 = 3;\n}}\nclass Holder {{\n  inner: Inner;\n  constructor() {{\n    print(`${{this.inner.value}}`);\n    this.inner = new Inner();\n  }}\n}}\n{MAIN}"
    );
    let diagnostics = diagnostics(&source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let first = &diagnostics[0];
    assert_eq!(first.code, RuleCode::S100);
    assert_eq!(
        (first.pos.line, first.pos.col),
        (7, 14),
        "{}",
        first.message
    );
    assert_eq!(first.divergence, None, "{}", first.message);
    // §108.4: the diagnostic names two spellings.
    for needle in [
        "`this.inner` reads field `inner` of `Holder`",
        "before the constructor assigns it at its top level",
        "move the read after `this.inner = \u{2026}`",
        "or give `inner` an initializer",
    ] {
        assert!(
            first.message.contains(needle),
            "missing {needle:?}: {}",
            first.message
        );
    }
}

#[test]
fn a_method_call_on_this_before_the_assignment_reports_its_variant() {
    // compiler.md §108.4 rule 6 site B: `tsc` accepts, because its
    // definite-assignment analysis does not follow a call.
    let source = format!(
        "class Inner {{\n  value: i32 = 3;\n}}\nclass Holder {{\n  inner: Inner;\n  constructor() {{\n    this.show();\n    this.inner = new Inner();\n  }}\n  show(): void {{\n    print(`${{this.inner.value}}`);\n  }}\n}}\n{MAIN}"
    );
    let diagnostics = diagnostics(&source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let first = &diagnostics[0];
    assert_eq!(first.code, RuleCode::S100);
    assert_eq!((first.pos.line, first.pos.col), (7, 5), "{}", first.message);
    assert_eq!(
        first.divergence,
        Some(Divergence::ThisBeforeFieldValues),
        "{}",
        first.message
    );
    // §108.4: the diagnostic names two spellings.
    for needle in [
        "the constructor of `Holder` calls a member of `this`",
        "before field `inner` holds a value",
        "move the call after the assignment of `inner`",
        "or give `inner` an initializer",
    ] {
        assert!(
            first.message.contains(needle),
            "missing {needle:?}: {}",
            first.message
        );
    }
}

#[test]
fn this_as_an_argument_before_the_assignment_reports_its_variant() {
    // compiler.md §108.4 rule 6 site B, the value half of the site.
    let source = format!(
        "class Inner {{\n  value: i32 = 3;\n}}\nclass Holder {{\n  inner: Inner;\n  constructor() {{\n    show(this);\n    this.inner = new Inner();\n  }}\n}}\nfunction show(holder: Holder): void {{\n  print(`${{holder.inner.value}}`);\n}}\n{MAIN}"
    );
    let diagnostics = diagnostics(&source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let first = &diagnostics[0];
    assert_eq!(first.code, RuleCode::S100);
    assert_eq!(
        (first.pos.line, first.pos.col),
        (7, 10),
        "{}",
        first.message
    );
    assert_eq!(
        first.divergence,
        Some(Divergence::ThisBeforeFieldValues),
        "{}",
        first.message
    );
    // §108.4: the diagnostic names two spellings.
    for needle in [
        "the constructor of `Holder` uses `this` as a value",
        "before field `inner` holds a value",
        "move the use after the assignment of `inner`",
        "or give `inner` an initializer",
    ] {
        assert!(
            first.message.contains(needle),
            "missing {needle:?}: {}",
            first.message
        );
    }
}

#[test]
fn two_fields_that_hold_no_value_are_both_named() {
    let source = format!(
        "class Inner {{\n  value: i32 = 3;\n}}\nclass Holder {{\n  first: Inner;\n  second: Inner;\n  constructor() {{\n    this.show();\n    this.first = new Inner();\n    this.second = new Inner();\n  }}\n  show(): void {{\n    print(`${{this.first.value}}`);\n  }}\n}}\n{MAIN}"
    );
    let diagnostics = diagnostics(&source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    // §108.4: both spellings name both fields, and each agrees with the
    // count.
    for needle in [
        "before fields `first`, `second` hold a value",
        "move the call after the assignments of `first`, `second`",
        "or give `first`, `second` initializers",
    ] {
        assert!(
            diagnostics[0].message.contains(needle),
            "missing {needle:?}: {}",
            diagnostics[0].message
        );
    }
}

#[test]
fn every_read_shape_reads_the_field_of_a_rule_one_field_that_holds_no_value() {
    // compiler.md §108.4 rule 6 form (b): a read is every use that is
    // not the target of `this.f = …`. Each body below reads `count`,
    // which holds no value until the last statement assigns it.
    for (stem, read, column) in [
        ("compound-assignment", "this.count += 1;", 5),
        ("increment", "this.count++;", 5),
        ("member-write", "this.holder.value = 1;", 5),
        ("operand", "const doubled: i32 = this.count * 2;", 26),
    ] {
        let source = format!(
            "class Inner {{\n  value: i32 = 3;\n}}\nclass Holder {{\n  count: i32;\n  holder: Inner;\n  constructor() {{\n    {read}\n    this.count = 1;\n    this.holder = new Inner();\n  }}\n}}\n{MAIN}"
        );
        let diagnostics = diagnostics(&source);
        assert_eq!(diagnostics.len(), 1, "{stem}: {diagnostics:?}");
        let first = &diagnostics[0];
        assert_eq!((first.pos.line, first.pos.col), (8, column), "{stem}");
        assert_eq!(first.divergence, None, "{stem}: {}", first.message);
        assert!(
            first.message.contains("reads field"),
            "{stem}: {}",
            first.message
        );
    }
    // An accessor read on `this` lowers to a method call, so it reaches
    // site B rather than site A.
    let source = format!(
        "class Holder {{\n  count: i32;\n  get doubled(): i32 {{\n    return this.count * 2;\n  }}\n  constructor() {{\n    print(`${{this.doubled}}`);\n    this.count = 1;\n  }}\n}}\n{MAIN}"
    );
    let diagnostics = diagnostics(&source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].divergence,
        Some(Divergence::ThisBeforeFieldValues),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn the_accepted_prefix_forms_stay_accepted_and_an_early_call_is_rejected() {
    // The firing control of the two silent cases: every `this` of the
    // accept entry's constructor is form (a) or form (b), and the call
    // and the argument stand after the prefix.
    let accepted_source = "class Inner {\n  value: i32 = 3;\n}\nclass Holder {\n  count: i32 = 1;\n  first: Inner;\n  inner: Inner;\n  constructor() {\n    this.count = this.count + 1;\n    this.first = new Inner();\n    this.inner = this.first;\n    this.show();\n    describe(this);\n  }\n  show(): void {\n    print(`${this.inner.value} ${this.count}`);\n  }\n}\nfunction describe(holder: Holder): void {\n  print(`${holder.first.value}`);\n}\nexport function main(): void {\n  const holder: Holder = new Holder();\n  holder.show();\n}\n";
    accepted(accepted_source);
    // The same program with the method call before the last assignment.
    let moved = accepted_source.replace(
        "    this.inner = this.first;\n    this.show();",
        "    this.show();\n    this.inner = this.first;",
    );
    assert_ne!(moved, accepted_source, "the control must change the source");
    let diagnostics = diagnostics(&moved);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].divergence,
        Some(Divergence::ThisBeforeFieldValues),
        "{}",
        diagnostics[0].message
    );
    // A constructor with no rule-1 field has an empty prefix, so every
    // `this` form is accepted there.
    accepted(&format!(
        "class Holder {{\n  count: i32 = 1;\n  constructor() {{\n    this.show();\n  }}\n  show(): void {{\n    print(`${{this.count}}`);\n  }}\n}}\n{MAIN}"
    ));
}

#[test]
fn the_receiver_form_reaches_site_a_and_a_held_receiver_is_accepted() {
    // compiler.md §108.4 rule 6 form (b): the receiver of `this.g.m()`
    // is a read of `g`, so the form reaches site A and not site B.
    let source = format!(
        "class Inner {{\n  value: i32 = 3;\n  get(): i32 {{\n    return this.value;\n  }}\n}}\nclass Holder {{\n  first: Inner;\n  count: i32;\n  constructor() {{\n    this.count = this.first.get();\n    this.first = new Inner();\n  }}\n}}\n{MAIN}"
    );
    let diagnostics = diagnostics(&source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let first = &diagnostics[0];
    assert_eq!(first.code, RuleCode::S100);
    assert_eq!(
        (first.pos.line, first.pos.col),
        (11, 18),
        "{}",
        first.message
    );
    assert_eq!(first.divergence, None, "{}", first.message);
    assert!(
        first
            .message
            .contains("`this.first` reads field `first` of `Holder`"),
        "{}",
        first.message
    );
    // The held case: the same receiver after the assignment of `first`.
    accepted(&format!(
        "class Inner {{\n  value: i32 = 3;\n  get(): i32 {{\n    return this.value;\n  }}\n}}\nclass Holder {{\n  first: Inner;\n  count: i32;\n  constructor() {{\n    this.first = new Inner();\n    this.count = this.first.get();\n  }}\n}}\n{MAIN}"
    ));
}

#[test]
fn the_prefix_ends_when_every_rule_one_field_holds_a_value() {
    // compiler.md §108.4 rule 6: the prefix ends with the first
    // top-level statement after which every rule-1 field holds a value.
    // A later assignment of a field that already holds one does not
    // extend the prefix, so the call stands after it.
    let accepted_source = "class Holder {\n  a: i32;\n  b: i32;\n  constructor() {\n    this.a = 1;\n    this.b = 2;\n    this.show();\n    this.a = 3;\n  }\n  show(): void {\n    print(`${this.a} ${this.b}`);\n  }\n}\nexport function main(): void {\n  const holder: Holder = new Holder();\n  holder.show();\n}\n";
    accepted(accepted_source);
    // Firing control: the same call before the assignment of `b`.
    let moved = accepted_source.replace(
        "    this.b = 2;\n    this.show();",
        "    this.show();\n    this.b = 2;",
    );
    assert_ne!(moved, accepted_source, "the control must change the source");
    let diagnostics = diagnostics(&moved);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let first = &diagnostics[0];
    assert_eq!(
        first.divergence,
        Some(Divergence::ThisBeforeFieldValues),
        "{}",
        first.message
    );
    // `a` holds a value at the call, so the message names `b` alone.
    assert!(
        first.message.contains("before field `b` holds a value"),
        "{}",
        first.message
    );
    assert!(!first.message.contains("`a`"), "{}", first.message);
}

#[test]
fn a_parameter_default_reads_a_field_that_holds_a_value() {
    // compiler.md §108.4 rule 6: the prefix starts with the parameter
    // defaults, and a default evaluates after the field initializers
    // (§57.1 step 4). An initialized field holds a value there.
    accepted(&format!(
        "class Holder {{\n  count: i32 = 5;\n  seen: i32;\n  constructor(n: i32 = this.count) {{\n    this.seen = n;\n  }}\n}}\n{MAIN}"
    ));
    // A rule-1 field holds no value in a default. No top-level
    // statement assigns `inner` here, so rule 1 reports at the field and
    // rule 6 reports at the `this`. Stock `tsc` answers TS2564 and
    // TS2565 for this program, measured.
    let source = format!(
        "class Inner {{\n  value: i32 = 3;\n}}\nclass Holder {{\n  inner: Inner;\n  constructor(n: i32 = this.inner.value) {{\n    print(`${{n}}`);\n  }}\n}}\n{MAIN}"
    );
    let diagnostics = diagnostics(&source);
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    let rule_one = &diagnostics[0];
    assert_eq!(rule_one.code, RuleCode::S100);
    assert_eq!(
        (rule_one.pos.line, rule_one.pos.col),
        (5, 3),
        "{}",
        rule_one.message
    );
    assert!(rule_one.message.contains("TS2564"), "{}", rule_one.message);
    let site_a = &diagnostics[1];
    assert_eq!(site_a.code, RuleCode::S100);
    assert_eq!(
        (site_a.pos.line, site_a.pos.col),
        (6, 24),
        "{}",
        site_a.message
    );
    assert_eq!(site_a.divergence, None, "{}", site_a.message);
    assert!(
        site_a
            .message
            .contains("`this.inner` reads field `inner` of `Holder`"),
        "{}",
        site_a.message
    );
}
