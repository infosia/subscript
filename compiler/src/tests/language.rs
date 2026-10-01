use super::*;

/// The checked module carries the bytes of every source the check
/// read (`specs/blocks/compiler.md` §110 rule 3). The dev JIT
/// derives one module's one memory reservation from the number.
#[test]
fn the_checked_module_carries_the_bytes_of_every_source() {
    let one = "export function main(): void {\n  print(\"one\");\n}\n";
    let module = check_one(one).expect("one file checks");
    assert_eq!(module.source_bytes, 49);
    assert_eq!(module.source_bytes, one.len());

    let other = "export function helper(): i32 {\n  return 7;\n}\n";
    let two = check_program(&[
        SourceFile::entry("main.ts", one),
        SourceFile::new("other.ts", other),
    ])
    .expect("two files check");
    assert_eq!(two.source_bytes, 95);
    assert_eq!(two.source_bytes, one.len() + other.len());
}

#[test]
fn empty_program_list_is_an_error() {
    let err = check_program(&[]).unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
}

#[test]
fn minimal_program_checks_clean() {
    let module =
        check_one("export function main(): void {\n  print(\"hello\");\n}\n").expect("clean check");
    assert_eq!(module.functions.len(), 1);
    assert_eq!(module.functions[0].name, "main");
    assert!(module.functions[0].exported);
    assert_eq!(module.functions[0].ret, Type::Void);
}

/// The classification record is a thread-local of the thread the
/// checker runs on. A check runs on the thread that calls it
/// (§114.2 rule 1), so this test reads its own thread.
#[test]
fn assignment_targets_classify_every_place_variant_from_source() {
    use crate::check::{take_classified_places, PlaceKind};

    let _ = take_classified_places();
    check_one(
        "let global: i32 = 0;\n\
         class Holder {\n\
           field: i32 = 0;\n\
           static count: i32 = 0;\n\
           get value(): i32 { return this.field; }\n\
           set value(next: i32) {}\n\
         }\n\
         class Values {\n\
           [index: u32]: i32;\n\
           get(index: u32): i32 { return index as i32; }\n\
           set(index: u32, value: i32): void {}\n\
         }\n\
         export function main(): void {\n\
           let local: i32 = 0;\n\
           const holder: Holder = new Holder();\n\
           const array: i32[] = [0];\n\
           const values: Values = new Values();\n\
           local = 1;\n\
           global = 1;\n\
           holder.field = 1;\n\
           array[0] = 1;\n\
           values[0 as u32] = 1;\n\
           holder.value = 1;\n\
           Holder.count = 1;\n\
         }\n",
    )
    .expect("all assignment place variants check");
    assert_eq!(
        take_classified_places(),
        [
            PlaceKind::Local,
            PlaceKind::Global,
            PlaceKind::Field,
            PlaceKind::Index,
            PlaceKind::IndexSignature,
            PlaceKind::Accessor,
            PlaceKind::StaticField,
        ]
    );
}

#[test]
fn nested_using_shadow_does_not_reuse_switch_storage() {
    fn count_storage_writes(statements: &[hir::Stmt]) -> usize {
        statements
            .iter()
            .map(|statement| match statement {
                hir::Stmt::Expr(hir::Expr {
                    kind: hir::ExprKind::Assign { target, .. },
                    ..
                }) if matches!(
                    &target.kind,
                    hir::ExprKind::Local(name, _) if name.starts_with("[[using.value#")
                ) =>
                {
                    1
                }
                hir::Stmt::If { then, els, .. } => {
                    count_storage_writes(then) + els.as_deref().map_or(0, count_storage_writes)
                }
                hir::Stmt::While { body, .. }
                | hir::Stmt::For { body, .. }
                | hir::Stmt::ForOf { body, .. }
                | hir::Stmt::Block(body)
                | hir::Stmt::Using { body, .. } => count_storage_writes(body),
                hir::Stmt::Switch { cases, .. } => cases
                    .iter()
                    .map(|case| count_storage_writes(&case.body))
                    .sum(),
                _ => 0,
            })
            .sum()
    }

    let module = check_one(
        "class Resource { [Symbol.dispose](): void {} }\n\
         export function main(): void {\n\
           const tag: i32 = 0;\n\
           switch (tag) {\n\
             case 0:\n\
               using resource = new Resource();\n\
               { using resource = new Resource(); }\n\
               break;\n\
             default:\n\
               break;\n\
           }\n\
         }\n",
    )
    .expect("a nested block may shadow a switch using binding");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    assert_eq!(count_storage_writes(&main.body), 1);
}

#[test]
fn invalid_regex_literal_is_a_checker_diagnostic() {
    let diagnostics = check_one("export function main(): void {\n  const regex = /(/;\n}\n")
        .expect_err("invalid literal must be rejected by the checker");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!((diagnostics[0].pos.line, diagnostics[0].pos.col), (2, 17));
    assert!(
        diagnostics[0]
            .message
            .contains("invalid regular-expression literal"),
        "diagnostic: {}",
        diagnostics[0].message
    );
}

#[test]
fn each_regex_literal_site_has_one_module_global_handle() {
    let module = check_one(
        "export function main(): void {\n\
         \x20 const first: RegExp = /x/g;\n\
         \x20 const second: RegExp = /x/g;\n\
         \x20 print(`${first.test(\"x\")} ${second.source}`);\n\
         }\n",
    )
    .expect("regex literals check");
    let regex_globals: Vec<&hir::Global> = module
        .globals
        .iter()
        .filter(|global| global.ty == Type::RegExp)
        .collect();
    assert_eq!(regex_globals.len(), 2);
    for global in regex_globals {
        assert!(global.name.starts_with("__subscript_regex_literal_"));
        assert!(matches!(
            &global.init.kind,
            hir::ExprKind::Call {
                callee: hir::Callee::Regex(hir::RegexFn::New),
                ..
            }
        ));
    }
}

#[test]
fn replace_all_rejects_a_non_global_regex_literal_early() {
    let diagnostics =
        check_one("export function main(): void {\n  print(\"aaa\".replaceAll(/a/, \"Z\"));\n}\n")
            .expect_err("literal without g must be rejected by the checker");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!((diagnostics[0].pos.line, diagnostics[0].pos.col), (2, 26));
    assert!(diagnostics[0].message.contains("requires the `g` flag"));
}

#[test]
fn bare_number_is_s007_with_position() {
    let err = check_one("const x: number = 1;\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S007);
    assert_eq!(err[0].pos.file, "test.ts");
    assert_eq!(err[0].pos.line, 1);
    assert_eq!(err[0].pos.col, 10);
}

#[test]
fn any_is_s001() {
    let err = check_one("const x: any = 1;\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S001);
}

#[test]
fn literal_overflow_is_s008() {
    let err = check_one("const x: i32 = 3000000000;\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S008);
}

#[test]
fn narrow_literal_ranges_are_checked() {
    for src in [
        "const x: i8 = 128;\n",
        "const x: u8 = -1;\n",
        "const x: i16 = 32768;\n",
        "const x: u16 = 65536;\n",
        "const x: f16 = 65520.0;\n",
    ] {
        let err = check_one(src).unwrap_err();
        assert_eq!(err[0].code, RuleCode::S008, "{src}");
    }
    check_one("const x: f16 = 65505.0;\nexport function main(): void {}\n")
        .expect("a finite-rounding f16 literal is accepted");
}

#[test]
fn fractional_literal_in_integer_context_is_s008() {
    let err = check_one("const x: i32 = 1.5;\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S008);
}

#[test]
fn mixed_arithmetic_without_as_is_s007() {
    let err = check_one(
        "export function main(): void {\n  const a: i32 = 1;\n  const b: u32 = 2;\n  const c: i32 = a + b;\n  print(`${c}`);\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S007);
    assert_eq!(err[0].pos.line, 4);
}

#[test]
fn f16_arithmetic_is_s014_with_compute_via_f32_guidance() {
    for body in [
        "const c: f16 = a + b;",
        "const c: f16 = a % b;",
        "const c: f16 = -a;",
        "a += b;",
        "a++;",
    ] {
        let src = format!(
            "export function main(): void {{\n  let a: f16 = 1.0;\n  const b: f16 = 2.0;\n  {body}\n}}\n"
        );
        let err = check_one(&src).unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014, "{body}");
        assert!(err[0].message.contains("as f32"), "{body}");
    }
}

#[test]
fn context_free_integer_literal_defaults_to_i32() {
    let module = check_one("export function main(): void {\n  const x = 3;\n  print(`${x}`);\n}\n")
        .expect("clean");
    let hir::Stmt::Let { ty, .. } = &module.functions[0].body[0] else {
        panic!("expected let");
    };
    assert_eq!(*ty, Type::I32);
}

#[test]
fn context_free_fractional_literal_defaults_to_f64() {
    let module =
        check_one("export function main(): void {\n  const x = 1.5;\n  print(`${x}`);\n}\n")
            .expect("clean");
    let hir::Stmt::Let { ty, .. } = &module.functions[0].body[0] else {
        panic!("expected let");
    };
    assert_eq!(*ty, Type::F64);
}

#[test]
fn undefined_is_s012() {
    let err = check_one("let x: i32 | undefined = undefined;\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S012);
}

#[test]
fn general_union_is_s011() {
    let err = check_one("let x: i32 | string = 1;\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S011);
}

#[test]
fn string_literal_union_members_are_contextually_typed() {
    let module = check_one(
        "type Format = \"a\" | \"b\";\n\
         @ValueType\n\
         class Box {\n\
           value: Format;\n\
           constructor(value: Format) { this.value = value; }\n\
         }\n\
         function pass(value: Format): Format { return value; }\n\
         export function main(): void {\n\
           const value: Format = \"a\";\n\
           const values: Format[] = [\"a\", \"b\"];\n\
           const box: Box = new Box(\"b\");\n\
           print(`${pass(value)}:${values[1]}:${box.value}:${value === \"a\"}`);\n\
         }\n",
    )
    .expect("Q32 member literals check cleanly in every contextual position");
    assert_eq!(module.string_aliases.len(), 1);
    assert_eq!(module.string_aliases[0].members, ["a", "b"]);
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let hir::Stmt::Let { ty, init, .. } = &main.body[0] else {
        panic!("first statement is a binding");
    };
    assert_eq!(*ty, Type::StringAlias(StringAliasId(0)));
    assert_eq!(init.kind, hir::ExprKind::Int(0));
}

#[test]
fn string_literal_union_nonmember_is_rejected() {
    let diagnostics = check_one(
        "type Format = \"a\" | \"b\";\n\
         export function main(): void {\n\
           const value: Format = \"c\";\n\
         }\n",
    )
    .expect_err("non-member literal must be rejected");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(diagnostics[0].pos.line, 3);
}

#[test]
fn same_membered_string_literal_unions_are_nominal() {
    let diagnostics = check_one(
        "type Left = \"a\" | \"b\";\n\
         type Right = \"a\" | \"b\";\n\
         export function main(): void {\n\
           const left: Left = \"a\";\n\
           const right: Right = left;\n\
         }\n",
    )
    .expect_err("same-membered aliases must remain distinct");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(diagnostics[0].pos.line, 5);
}

#[test]
fn string_literal_union_is_rejected_in_a_boundary_signature() {
    let diagnostics = check_program(&[
        SourceFile::ambient(
            "boundary.d.ts",
            "type Format = \"a\" | \"b\";\n\
             declare class Boundary {\n\
               value: Format;\n\
               constructor(value: Format);\n\
             }\n",
        ),
        SourceFile::new(
            "test.ts",
            "export function main(): void { print(\"ok\"); }\n",
        ),
    ])
    .expect_err("Q32 aliases are not boundary types");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(
        diagnostics[0]
            .message
            .contains("cannot appear in a boundary signature"),
        "{}",
        diagnostics[0].message
    );

    let diagnostics = check_one(
        "type Format = \"a\" | \"b\";\n\
         export function boundary(value: Format): void { print(`${value}`); }\n\
         export function main(): void { print(\"ok\"); }\n",
    )
    .expect_err("Q32 aliases are not exported boundary types");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(
        diagnostics[0].message.contains("boundary signature"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn wire_mapped_alias_is_accepted_at_foreign_parameter_and_return_positions() {
    let module = check_program(&[
        SourceFile::ambient(
            "wire.d.ts",
            "// @subscript-c-header include=\"wire.h\"\n\
             type WireMode = CEnum<{ \"m0\": 0x10; \"m1\": 23; \"m2\": -7 }>;\n\
             declare function takeWire(value: WireMode): i32;\n\
             declare function returnWire(): WireMode;\n",
        ),
        SourceFile::new(
            "test.ts",
            "export function main(): void {\n\
               const value: WireMode = returnWire();\n\
               print(`${takeWire(value)}:${value}`);\n\
             }\n",
        ),
    ])
    .expect("R23 aliases are direct foreign boundary scalars");
    assert_eq!(module.string_aliases[0].members, ["m0", "m1", "m2"]);
    assert_eq!(
        module.string_aliases[0].wire_values.as_deref(),
        Some(&[16, 23, -7][..])
    );
    assert!(matches!(
        module.foreign_fns[0].params[0].ty,
        Type::StringAlias(StringAliasId(0))
    ));
    assert!(matches!(
        module.foreign_fns[1].ret,
        Type::StringAlias(StringAliasId(0))
    ));
}

#[test]
fn wire_mapped_alias_is_accepted_in_boundary_struct_pair_and_constructor_positions() {
    let module = check_program(&[
        SourceFile::ambient(
            "wire.d.ts",
            "// @subscript-c-header include=\"wire.h\"\n\
             // @subscript-c-descriptor function=\"takeModes\" parameter=\"modes\" aggregate=\"WireModes\" element=\"WireModeC\" const=true\n\
             type WireMode = CEnum<{ \"m0\": 0x10; \"m1\": 23; \"m2\": -7 }>;\n\
             declare class WireRecord {\n\
               mode: WireMode;\n\
               modes: WireMode[];\n\
               constructor(mode: WireMode, modes: WireMode[]);\n\
             }\n\
             declare function takeModes(modes: WireMode[]): i32;\n",
        ),
        SourceFile::new(
            "test.ts",
            "export function main(): void {\n\
               const record: WireRecord = new WireRecord(\"m2\", [\"m0\", \"m1\"]);\n\
               print(`${record.mode}:${takeModes(record.modes)}`);\n\
             }\n",
        ),
    ])
    .expect("§52 wire aliases are legal in all three new boundary positions");
    let record = module
        .classes
        .iter()
        .find(|class| class.name == "WireRecord")
        .expect("WireRecord boundary class");
    assert!(matches!(record.fields[0].ty, Type::StringAlias(_)));
    assert!(matches!(
        record.fields[1].ty,
        Type::Array(ref element) if matches!(&**element, Type::StringAlias(_))
    ));
    assert!(matches!(
        module.foreign_fns[0].params[0].ty,
        Type::Array(ref element) if matches!(&**element, Type::StringAlias(_))
    ));
}

#[test]
fn plain_string_alias_stays_rejected_in_each_new_boundary_position() {
    for (label, declaration) in [
        (
            "struct member",
            "declare class Boundary { value: PlainMode; constructor(value: i32); }\n",
        ),
        (
            "pair element",
            "declare class Boundary { values: PlainMode[]; constructor(values: i32[]); }\n",
        ),
        (
            "constructor parameter",
            "declare class Boundary { value: i32; constructor(value: PlainMode); }\n",
        ),
    ] {
        let mirror = format!(
            "// @subscript-c-header include=\"plain.h\"\n\
             type PlainMode = \"m0\" | \"m1\";\n{declaration}"
        );
        let diagnostics = check_program(&[
            SourceFile::ambient("plain.d.ts", mirror),
            SourceFile::new("test.ts", "export function main(): void {}\n"),
        ])
        .unwrap_err();
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic
                .message
                .contains("cannot appear in a boundary signature")),
            "plain alias {label} unexpectedly lacked the boundary rejection: {diagnostics:?}"
        );
    }
}

#[test]
fn wire_alias_absence_sentinel_is_outside_the_wire_set() {
    let module = check_one(
        "type Mode = CEnum<{ \"min\": -2147483648; \"next\": -2147483647 }>;\n\
         @Descriptor\n\
         class Options { mode?: Mode; }\n\
         export function main(): void { const options: Options = {}; }\n",
    )
    .expect("wire alias may use values that collide with the plain sentinel policy");
    assert_eq!(module.string_aliases[0].absence_discriminant(), -2147483646);
}

#[test]
fn enum_member_accepts_nested_negation() {
    let module =
        check_one("enum E { A = -(-5) }\nexport function main(): void { print(`${E.A}`); }\n")
            .expect("nested enum negation checks");
    assert_eq!(module.enums[0].members, vec![("A".to_string(), 5)]);
}

#[test]
fn plain_string_alias_stays_rejected_at_foreign_parameter_and_return_positions() {
    let diagnostics = check_program(&[
        SourceFile::ambient(
            "plain.d.ts",
            "// @subscript-c-header include=\"plain.h\"\n\
             type PlainMode = \"m0\" | \"m1\";\n\
             declare function takePlain(value: PlainMode): PlainMode;\n",
        ),
        SourceFile::new("test.ts", "export function main(): void {}\n"),
    ])
    .expect_err("plain Q32 aliases remain barred from foreign signatures");
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("cannot appear in a boundary signature")));
}

#[test]
fn wire_mapped_alias_rejects_fractional_duplicate_out_of_range_and_empty_mappings() {
    for (source, message) in [
        (
            "type Bad = CEnum<{ \"m0\": 1.5 }>;\n",
            "must be an integer literal",
        ),
        (
            "type Bad = CEnum<{ \"m0\": 7; \"m1\": 7 }>;\n",
            "duplicate CEnum wire value 7",
        ),
        (
            "type Bad = CEnum<{ \"m0\": 2147483648 }>;\n",
            "outside the i32 range",
        ),
        ("type Bad = CEnum<{}>;\n", "must have at least one member"),
    ] {
        let diagnostics = check_one(source).expect_err("invalid CEnum mapping must fail");
        assert!(
            diagnostics[0].message.contains(message),
            "{}",
            diagnostics[0].message
        );
    }
}

#[test]
fn exhaustive_string_literal_union_switch_is_accepted() {
    let module = check_one(
        "type Format = \"a\" | \"b\" | \"c\";\n\
         function classify(value: Format): void {\n\
           switch (value) {\n\
             case \"a\": break;\n\
             case \"b\": break;\n\
             case \"c\": break;\n\
           }\n\
         }\n\
         export function main(): void { classify(\"a\"); }\n",
    )
    .expect("an exhaustive Q32 switch checks cleanly");
    let classify = module
        .functions
        .iter()
        .find(|function| function.name == "classify")
        .expect("classify function");
    let hir::Stmt::Switch { cases, .. } = &classify.body[0] else {
        panic!("classify body begins with a switch");
    };
    let discriminants = cases
        .iter()
        .map(|case| case.test.as_ref().map(|test| &test.kind))
        .collect::<Vec<_>>();
    assert_eq!(
        discriminants,
        [
            Some(&hir::ExprKind::Int(0)),
            Some(&hir::ExprKind::Int(1)),
            Some(&hir::ExprKind::Int(2)),
        ]
    );
}

#[test]
fn string_literal_union_switch_default_accepts_a_subset() {
    check_one(
        "type Format = \"a\" | \"b\" | \"c\";\n\
         function classify(value: Format): void {\n\
           switch (value) {\n\
             case \"b\": break;\n\
             default: break;\n\
           }\n\
         }\n\
         export function main(): void { classify(\"a\"); }\n",
    )
    .expect("a default permits a distinct subset of Q32 members");
}

#[test]
fn string_literal_union_switch_missing_member_is_rejected() {
    let diagnostics = check_one(
        "type Format = \"a\" | \"b\" | \"c\";\n\
         function classify(value: Format): void {\n\
           switch (value) {\n\
             case \"a\": break;\n\
             case \"c\": break;\n\
           }\n\
         }\n",
    )
    .expect_err("a default-free Q32 switch must be exhaustive");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(diagnostics[0].message.contains("`Format`"));
    assert!(diagnostics[0].message.contains("\"b\""));
}

#[test]
fn string_literal_union_switch_duplicate_member_is_rejected() {
    let diagnostics = check_one(
        "type Format = \"a\" | \"b\";\n\
         function classify(value: Format): void {\n\
           switch (value) {\n\
             case \"a\": break;\n\
             case \"a\": break;\n\
             case \"b\": break;\n\
           }\n\
         }\n",
    )
    .expect_err("a Q32 switch member may appear only once");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(diagnostics[0]
        .message
        .contains("duplicate case label \"a\""));
    assert!(diagnostics[0].message.contains("`Format`"));
}

#[test]
fn string_literal_union_switch_nonmember_is_rejected() {
    let diagnostics = check_one(
        "type Format = \"a\" | \"b\";\n\
         function classify(value: Format): void {\n\
           switch (value) {\n\
             case \"a\": break;\n\
             case \"other\": break;\n\
             default: break;\n\
           }\n\
         }\n",
    )
    .expect_err("a Q32 switch label must name a member");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(diagnostics[0].message.contains("\"other\""));
    assert!(diagnostics[0].message.contains("`Format`"));
}

#[test]
fn exhaustive_string_literal_union_switch_satisfies_return_flow() {
    check_one(
        "type GPUBufferMapState = \"unmapped\" | \"pending\" | \"mapped\";\n\
         function lower(v: GPUBufferMapState): i32 {\n\
           switch (v) {\n\
             case \"unmapped\": return 1;\n\
             case \"pending\": return 2;\n\
             case \"mapped\": return 3;\n\
           }\n\
         }\n\
         export function main(): void { print(`${lower(\"mapped\")}`); }\n",
    )
    .expect("all diverging arms make an exhaustive Q32 switch diverge");
}

#[test]
fn exhaustive_string_literal_union_switch_with_break_fails_return_flow() {
    let diagnostics = check_one(
        "type Mode = \"a\" | \"b\";\n\
         function classify(value: Mode): i32 {\n\
           switch (value) {\n\
             case \"a\": return 1;\n\
             case \"b\": break;\n\
           }\n\
         }\n",
    )
    .expect_err("a breaking Q32 arm still falls through the switch");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(diagnostics[0].message.contains("not all paths return"));
}

#[test]
fn default_bearing_switch_return_flow_is_unchanged() {
    check_one(
        "function classify(value: string): i32 {\n\
           switch (value) {\n\
             case \"a\": return 1;\n\
             default: return 2;\n\
           }\n\
         }\n\
         export function main(): void { print(`${classify(\"a\")}`); }\n",
    )
    .expect("a default-bearing all-return switch retains existing flow behavior");
}

#[test]
fn defaultless_non_alias_switches_do_not_satisfy_return_flow() {
    for source in [
        "function classify(value: i32): i32 { switch (value) { case 0: return 1; } }\n",
        "function classify(value: string): i32 { switch (value) { case \"a\": return 1; } }\n",
        "enum Mode { A, B }\nfunction classify(value: Mode): i32 { switch (value) { case Mode.A: return 1; case Mode.B: return 2; } }\n",
    ] {
        let diagnostics = check_one(source)
            .expect_err("default-less non-alias switches retain conservative return flow");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert!(diagnostics[0].message.contains("not all paths return"));
    }
}

#[test]
fn exhaustive_alias_switch_divergence_is_recursive() {
    check_one(
        "type Outer = \"a\" | \"b\";\n\
         type Inner = \"x\" | \"y\";\n\
         function classify(outer: Outer, inner: Inner): i32 {\n\
           switch (outer) {\n\
             case \"a\": return 1;\n\
             case \"b\":\n\
               switch (inner) {\n\
                 case \"x\": return 2;\n\
                 case \"y\": unreachable();\n\
               }\n\
           }\n\
         }\n\
         export function main(): void { print(`${classify(\"b\", \"x\")}`); }\n",
    )
    .expect("nested exhaustive Q32 switches compose divergence");
}

#[test]
fn unreachable_call_statement_satisfies_function_return_flow() {
    check_one(
        "function nonnegative(value: i32): i32 {\n\
           if (value >= 0) { return value; }\n\
           unreachable();\n\
         }\n\
         export function main(): void { print(`${nonnegative(4)}`); }\n",
    )
    .expect("an unreachable() tail diverges");
}

#[test]
fn exhaustive_alias_switch_satisfies_lambda_return_flow() {
    check_one(
        "type Mode = \"a\" | \"b\";\n\
         export function main(): void {\n\
           const classify: (value: Mode) => i32 = (value: Mode): i32 => {\n\
             switch (value) {\n\
               case \"a\": return 1;\n\
               case \"b\": return 2;\n\
             }\n\
           };\n\
           print(`${classify(\"a\")}`);\n\
         }\n",
    )
    .expect("the lambda return-flow site shares exhaustive Q32 divergence");
}

#[test]
fn descriptor_required_members_and_defaults_reach_hir() {
    let module = check_one(
        "type Mode = \"fast\" | \"safe\";\n\
         @Descriptor\n\
         class Child {\n\
           value?: i32 = 7;\n\
         }\n\
         @Descriptor\n\
         class Options {\n\
           count!: i32;\n\
           child?: Child = {};\n\
           mode?: Mode = \"safe\";\n\
         }\n\
         export function main(): void {\n\
           const options: Options = { count: 3 };\n\
           print(`${options.count}:${options.mode}`);\n\
         }\n",
    )
    .expect("required-present descriptor literal and defaults check cleanly");
    let options = module
        .classes
        .iter()
        .find(|class| class.name == "Options")
        .expect("Options class");
    assert!(options.is_descriptor);
    assert!(!options.is_value);
    assert!(!options.fields[0].is_defaulted);
    assert!(options.fields[1].is_defaulted);
    assert!(matches!(
        options.fields[1].init.as_ref().map(|expr| &expr.kind),
        Some(hir::ExprKind::DescriptorLit { .. })
    ));
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let hir::Stmt::Let { init, .. } = &main.body[0] else {
        panic!("first statement is the descriptor binding");
    };
    let hir::ExprKind::DescriptorLit { fields, .. } = &init.kind else {
        panic!("object literal lowered to DescriptorLit HIR");
    };
    assert!(fields[0].is_some(), "required member is explicit");
    assert!(fields[1].is_none(), "nested member takes its default");
    assert!(fields[2].is_none(), "Q32 alias member takes its default");
}

#[test]
fn descriptor_literals_use_nullable_context_in_all_positions() {
    check_one(
        "@Descriptor\n\
         class Leaf { value?: i32 = 7; }\n\
         @Descriptor\n\
         class Holder { member!: Leaf | null; }\n\
         @Descriptor\n\
         class Outer { nested!: Holder; }\n\
         function take(value: Leaf | null): void {}\n\
         export function main(): void {\n\
           const member: Holder = { member: {} };\n\
           take({});\n\
           const array: (Leaf | null)[] = [{}];\n\
           const nested: Outer = { nested: { member: {} } };\n\
           print(`${member.member !== null}:${array[0] !== null}:${nested.nested.member !== null}`);\n\
         }\n",
    )
    .expect("descriptor literals use nullable member, argument, array-element, and nested contexts");
}

#[test]
fn null_in_nullable_descriptor_member_keeps_null_hir() {
    let module = check_one(
        "@Descriptor\n\
         class Leaf { value?: i32 = 7; }\n\
         @Descriptor\n\
         class Holder { member!: Leaf | null; }\n\
         export function main(): void {\n\
           const holder: Holder = { member: null };\n\
           print(`${holder.member !== null}`);\n\
         }\n",
    )
    .expect("null remains assignable to a nullable descriptor member");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let hir::Stmt::Let { init, .. } = &main.body[0] else {
        panic!("first statement is the descriptor binding");
    };
    let hir::ExprKind::DescriptorLit { fields, .. } = &init.kind else {
        panic!("holder object lowered to DescriptorLit HIR");
    };
    let member = fields[0].as_ref().expect("required member is explicit");
    assert_eq!(member.ty, Type::Null);
    assert!(matches!(member.kind, hir::ExprKind::Null));
}

#[test]
fn contextual_conditional_uses_nullable_type_in_both_branch_orders() {
    let module = check_one(
        "class C { value: i32; constructor(value: i32) { this.value = value; } }\n\
         export function main(): void {\n\
           const valueFirst: C | null = true ? new C(1) : null;\n\
           const nullFirst: C | null = false ? null : new C(2);\n\
           print(`${valueFirst !== null}:${nullFirst !== null}`);\n\
         }\n",
    )
    .expect("a nullable context types either conditional branch order");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    for statement in &main.body[..2] {
        let hir::Stmt::Let { init, .. } = statement else {
            panic!("conditional binding");
        };
        assert!(matches!(init.kind, hir::ExprKind::Cond { .. }));
        assert!(matches!(init.ty, Type::Nullable(_)));
    }
}

#[test]
fn nested_conditionals_inherit_the_outer_context() {
    let module = check_one(
        "class C { value: i32; constructor(value: i32) { this.value = value; } }\n\
         export function main(): void {\n\
           const value: C | null = true ? (false ? new C(1) : null) : null;\n\
           print(`${value !== null}`);\n\
         }\n",
    )
    .expect("nested conditionals inherit the nullable context");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let hir::Stmt::Let { init, .. } = &main.body[0] else {
        panic!("conditional binding");
    };
    let hir::ExprKind::Cond { then, .. } = &init.kind else {
        panic!("outer conditional");
    };
    assert!(matches!(init.ty, Type::Nullable(_)));
    assert!(matches!(then.kind, hir::ExprKind::Cond { .. }));
    assert_eq!(then.ty, init.ty);
}

#[test]
fn conditional_without_context_keeps_else_to_then_rule() {
    let diagnostics = check_one(
        "class C { value: i32; constructor(value: i32) { this.value = value; } }\n\
         export function main(): void {\n\
           const value = true ? new C(1) : null;\n\
           print(`${value !== null}`);\n\
         }\n",
    )
    .expect_err("an uncontextualized conditional keeps the directional branch rule");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(diagnostics[0].pos.line, 3);
    assert!(diagnostics[0]
        .message
        .contains("the else branch expects `C`, got `null`"));
}

#[test]
fn contextual_conditional_accepts_nominally_distinct_reference_arms() {
    let module = check_program(&[
        SourceFile::ambient(
            "boundary.d.ts",
            "// @subscript-c-header include=\"boundary.h\"\n\
             declare function take(value: object | null): void;\n",
        ),
        SourceFile::new(
            "test.ts",
            "class A { value: i32; constructor() { this.value = 1; } }\n\
             class B { value: i32; constructor() { this.value = 2; } }\n\
             export function main(): void {\n\
               take(true ? new A() : new B());\n\
             }\n",
        ),
    ])
    .expect("both nominal arms are assignable to the boundary object context");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let hir::Stmt::Expr(call) = &main.body[0] else {
        panic!("call statement");
    };
    let hir::ExprKind::Call { args, .. } = &call.kind else {
        panic!("foreign call");
    };
    assert!(matches!(&args[0].ty, Type::Nullable(inner) if **inner == Type::Object));
}

#[test]
fn contextual_conditional_does_not_admit_script_value_class_union() {
    let diagnostics = check_one(
        "@ValueType\n\
         class V { value: i32; constructor(value: i32) { this.value = value; } }\n\
         export function main(): void {\n\
           const value: V | null = true ? new V(1) : null;\n\
         }\n",
    )
    .expect_err("C7 keeps nullable value classes out of script declarations");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
    assert_eq!(diagnostics[0].pos.line, 4);
}

#[test]
fn array_nullable_union_keeps_the_s011_acceptance_boundary() {
    let source = "export function main(): void {\n  let a: i32[] | null = null;\n}\n";
    let files = [SourceFile::new("test.ts", source)];
    let diagnostics =
        check_program(&files).expect_err("dynamic arrays are not accepted as nullable unions");
    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S011);
    assert_eq!(
        diagnostics[0].message,
        "unions are limited to `Ref | null`; `i32[] | null` is not a reference type union"
    );
}

#[test]
fn array_and_regexp_identity_keep_the_s100_acceptance_boundary() {
    let source = "export function main(): void {\n\
                  \x20 let a: i32[] = [1];\n\
                  \x20 let b: i32[] = a;\n\
                  \x20 let first: RegExp = /a/;\n\
                  \x20 let second: RegExp = first;\n\
                  \x20 print(`${a === b} ${first === second}`);\n\
                  }\n";
    let files = [SourceFile::new("test.ts", source)];
    let diagnostics = check_program(&files)
        .expect_err("array and RegExp identity equality remain outside the language");
    assert_eq!(diagnostics.len(), 2, "diagnostics: {diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert_eq!(
        diagnostics[0].message,
        "operator `===` not defined for `i32[]` and `i32[]`"
    );
    assert_eq!(diagnostics[1].code, RuleCode::S100);
    assert_eq!(
        diagnostics[1].message,
        "operator `===` not defined for `RegExp` and `RegExp`"
    );
}

#[test]
fn conditional_arms_narrow_in_both_condition_orders() {
    check_one(
        "class C { x: u32; constructor(x: u32) { this.x = x; } }\n\
         function use(value: C): u32 { return value.x; }\n\
         function nonNullFirst(value: C | null): u32 {\n\
           return value !== null ? use(value) : 0;\n\
         }\n\
         function nullFirst(value: C | null): u32 {\n\
           return value === null ? 0 : use(value);\n\
         }\n\
         export function main(): void {\n\
           print(`${nonNullFirst(new C(1))}:${nullFirst(new C(2))}`);\n\
         }\n",
    )
    .expect("either null-comparison order narrows the matching conditional arm");
}

#[test]
fn nested_conditional_arms_keep_outer_narrowing() {
    check_one(
        "class C { x: u32; constructor(x: u32) { this.x = x; } }\n\
         function use(value: C): u32 { return value.x; }\n\
         function nested(value: C | null, flag: boolean): u32 {\n\
           return value !== null\n\
             ? (flag ? use(value) : use(value))\n\
             : 0;\n\
         }\n\
         export function main(): void { print(`${nested(new C(3), true)}`); }\n",
    )
    .expect("nested conditional arms retain the facts established by outer conditions");
}

#[test]
fn conditional_and_if_narrowing_compose_in_either_nesting_order() {
    check_one(
        "class C { x: u32; constructor(x: u32) { this.x = x; } }\n\
         function use(value: C): u32 { return value.x; }\n\
         function conditionalInsideIf(value: C | null, flag: boolean): u32 {\n\
           if (value !== null) {\n\
             return flag ? use(value) : use(value);\n\
           }\n\
           return 0;\n\
         }\n\
         function ifInsideConditional(value: C | null, flag: boolean): u32 {\n\
           return flag\n\
             ? ((item: C | null): u32 => {\n\
                 if (item !== null) { return use(item); }\n\
                 return 0;\n\
               })(value)\n\
             : 0;\n\
         }\n\
         export function main(): void {\n\
           print(`${conditionalInsideIf(new C(4), true)}:${ifInsideConditional(new C(5), true)}`);\n\
         }\n",
    )
    .expect("conditional expressions and if statements compose in either nesting order");
}

#[test]
fn assignment_inside_conditional_arm_invalidates_narrowing() {
    let diagnostics = check_one(
        "class C { x: u32; constructor(x: u32) { this.x = x; } }\n\
         function pair(reset: C | null, value: C): u32 { return value.x; }\n\
         function invalidated(value: C | null): u32 {\n\
           return value !== null ? pair(value = null, value) : 0;\n\
         }\n\
         export function main(): void { print(`${invalidated(new C(6))}`); }\n",
    )
    .expect_err("an earlier assignment in an arm kills its condition-derived fact");
    assert_eq!(diagnostics[0].code, RuleCode::S005);
    assert_eq!(diagnostics[0].pos.line, 4);
    assert!(diagnostics[0]
        .message
        .contains("the argument expects `C`, got `C | null`"));
}

#[test]
fn conditional_arm_narrowing_does_not_escape_the_expression() {
    let diagnostics = check_one(
        "class C { x: u32; constructor(x: u32) { this.x = x; } }\n\
         function use(value: C): u32 { return value.x; }\n\
         function escaped(value: C | null): u32 {\n\
           const observed: u32 = value !== null ? use(value) : 0;\n\
           return use(value);\n\
         }\n\
         export function main(): void { print(`${escaped(new C(7))}`); }\n",
    )
    .expect_err("a fact established for one conditional arm must not escape");
    assert_eq!(diagnostics[0].code, RuleCode::S005);
    assert_eq!(diagnostics[0].pos.line, 5);
}

#[test]
fn descriptor_missing_and_excess_members_are_rejected() {
    let missing = check_one(
        "@Descriptor\n\
         class Options { count!: i32; }\n\
         export function main(): void {\n\
           const options: Options = {};\n\
         }\n",
    )
    .expect_err("missing required descriptor member");
    assert_eq!(missing[0].code, RuleCode::S100);
    assert!(missing[0]
        .message
        .contains("missing required member `count`"));

    let excess = check_one(
        "@Descriptor\n\
         class Options { count!: i32; }\n\
         export function main(): void {\n\
           const options: Options = { count: 1, extra: 2 };\n\
         }\n",
    )
    .expect_err("excess descriptor member");
    assert_eq!(excess[0].code, RuleCode::S004);
    assert!(excess[0].message.contains("no declared property `extra`"));
}

#[test]
fn object_literal_for_unmarked_class_remains_nominally_rejected() {
    let diagnostics = check_one(
        "class Options { count: i32 = 1; }\n\
         export function main(): void {\n\
           const options: Options = { count: 1 };\n\
         }\n",
    )
    .expect_err("unmarked class must not be literal-constructible");
    assert_eq!(diagnostics[0].code, RuleCode::S005);
}

#[test]
fn object_literal_for_nullable_unmarked_class_remains_nominally_rejected() {
    let diagnostics = check_one(
        "class Options {}\n\
         export function main(): void {\n\
           const options: Options | null = {};\n\
         }\n",
    )
    .expect_err("a nullable unmarked class must not be literal-constructible");
    assert_eq!(diagnostics[0].code, RuleCode::S005);
    assert_eq!(
        diagnostics[0].message,
        "object literals do not satisfy nominal class types"
    );
}

#[test]
fn descriptor_member_forms_are_exact() {
    let optional_without_default = check_one(
        "@Descriptor\n\
         class Options { count?: i32; }\n\
         export function main(): void {}\n",
    )
    .expect_err("optional descriptor member without default");
    assert_eq!(optional_without_default[0].code, RuleCode::S012);

    let definite_with_default = check_one(
        "@Descriptor\n\
         class Options { count!: i32 = 1; }\n\
         export function main(): void {}\n",
    )
    .expect_err("required descriptor member with initializer");
    assert_eq!(definite_with_default[0].code, RuleCode::S100);

    let initializer_without_optional = check_one(
        "@Descriptor\n\
         class Options { count: i32 = 1; }\n\
         export function main(): void {}\n",
    )
    .expect_err("descriptor initializer without optional spelling");
    assert_eq!(initializer_without_optional[0].code, RuleCode::S100);
}

#[test]
fn descriptor_explicit_undefined_stays_rejected() {
    let diagnostics = check_one(
        "@Descriptor\n\
         class Options { count?: i32 = 1; }\n\
         export function main(): void {\n\
           const options: Options = { count: undefined };\n\
         }\n",
    )
    .expect_err("undefined cannot be supplied explicitly");
    assert_eq!(diagnostics[0].code, RuleCode::S012);
}

#[test]
fn absence_capable_alias_member_omission_uses_reserved_discriminant() {
    let module = check_one(
        "type Compare = \"never\" | \"less\";\n\
         @Descriptor\n\
         class Sampler { compare?: Compare; }\n\
         export function main(): void {\n\
           const sampler: Sampler = {};\n\
           if (sampler.compare !== undefined) {\n\
             const present: Compare = sampler.compare;\n\
             print(`${present}`);\n\
           } else {\n\
             print(\"absent\");\n\
           }\n\
           if (sampler.compare === undefined) {\n\
             print(\"still absent\");\n\
           } else {\n\
             const present: Compare = sampler.compare;\n\
             print(`${present}`);\n\
           }\n\
         }\n",
    )
    .expect("presence arms read an absence-capable member as its Q32 alias");

    let sampler = module
        .classes
        .iter()
        .find(|class| class.name == "Sampler")
        .expect("Sampler descriptor");
    assert!(sampler.fields[0].is_absence_capable);
    assert!(!sampler.fields[0].is_defaulted);

    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let hir::Stmt::Let { init, .. } = &main.body[0] else {
        panic!("first statement is the descriptor binding");
    };
    let hir::ExprKind::DescriptorLit { fields, .. } = &init.kind else {
        panic!("object literal lowered to DescriptorLit HIR");
    };
    assert!(matches!(
        fields[0].as_ref().map(|field| (&field.kind, &field.ty)),
        Some((
            hir::ExprKind::Int(types::ABSENT_STRING_ALIAS_DISCRIMINANT),
            Type::StringAlias(_)
        ))
    ));
    let hir::Stmt::If { cond, .. } = &main.body[1] else {
        panic!("first presence test is an if statement");
    };
    assert!(matches!(
        cond.kind,
        hir::ExprKind::AbsenceTest { negated: true, .. }
    ));
    let hir::Stmt::If { cond, .. } = &main.body[2] else {
        panic!("second presence test is an if statement");
    };
    assert!(matches!(
        cond.kind,
        hir::ExprKind::AbsenceTest { negated: false, .. }
    ));
}

#[test]
fn absence_capable_member_read_in_absent_arm_is_rejected() {
    let diagnostics = check_one(
        "type Compare = \"never\" | \"less\";\n\
         @Descriptor\n\
         class Sampler { compare?: Compare; }\n\
         export function main(): void {\n\
           const sampler: Sampler = {};\n\
           if (sampler.compare === undefined) {\n\
             print(`${sampler.compare}`);\n\
           }\n\
         }\n",
    )
    .expect_err("the absent arm must not permit a member read");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(diagnostics[0].message.contains("presence test"));
}

#[test]
fn absence_capable_member_reassignment_invalidates_narrowing() {
    let diagnostics = check_one(
        "type Compare = \"never\" | \"less\";\n\
         @Descriptor\n\
         class Sampler { compare?: Compare; }\n\
         export function main(): void {\n\
           const sampler: Sampler = { compare: \"less\" };\n\
           if (sampler.compare !== undefined) {\n\
             sampler.compare = \"never\";\n\
             print(`${sampler.compare}`);\n\
           }\n\
         }\n",
    )
    .expect_err("field reassignment must kill the presence fact");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(diagnostics[0].message.contains("presence test"));
}

#[test]
fn undefined_outside_absence_presence_tests_stays_rejected() {
    for source in [
        "export function main(): void { const value = undefined; }\n",
        "export function main(): void { const value: i32 = 1; print(`${value !== undefined}`); }\n",
    ] {
        let diagnostics =
            check_one(source).expect_err("ordinary undefined use remains outside the language");
        assert_eq!(diagnostics[0].code, RuleCode::S012);
    }
}

#[test]
fn descriptor_methods_are_rejected() {
    let diagnostics = check_one(
        "@Descriptor\n\
         class Options {\n\
           count?: i32 = 1;\n\
           getCount(): i32 { return this.count; }\n\
         }\n\
         export function main(): void {}\n",
    )
    .expect_err("descriptor method");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    assert!(diagnostics[0].message.contains("cannot declare methods"));
}

#[test]
fn throw_is_s010() {
    let err = check_one("export function main(): void {\n  throw \"x\";\n}\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S010);
    assert_eq!(
        err[0].message,
        "`throw` requires an Error-family object; \
         this operand has type `string`"
    );
    assert_eq!(err[0].pos.line, 2);
}

#[test]
fn async_function_erases_promise_to_its_fulfilled_type() {
    let module = check_one("async function f(): Promise<void> {\n  await Context.suspend();\n}\n")
        .expect("async function");
    assert!(module.functions[0].is_async);
    assert_eq!(module.functions[0].ret, Type::Void);
}

#[test]
fn async_instance_method_hir_carries_receiver_before_arguments() {
    let module = check_one(concat!(
        "class Worker {\n",
        "  async sibling(value: i32): Promise<i32> { return value; }\n",
        "  async run(): Promise<i32> { return await this.sibling(7); }\n",
        "}\n",
        "export async function main(): Promise<void> {\n",
        "  const value: i32 = await new Worker().run();\n",
        "  print(`${value}`);\n",
        "}\n",
    ))
    .expect("async methods");
    let (worker_index, class) = module
        .classes
        .iter()
        .enumerate()
        .find(|(_, class)| class.name == "Worker")
        .expect("Worker class");
    assert!(class.methods.iter().all(|method| method.is_async));

    let run = class
        .methods
        .iter()
        .find(|method| method.name == "run")
        .expect("run method");
    let hir::Stmt::Return {
        value: Some(value), ..
    } = &run.body[0]
    else {
        panic!("run return")
    };
    let hir::ExprKind::AsyncCall { callee, args } = &value.kind else {
        panic!("run async call")
    };
    let hir::AsyncCallee::Method {
        class: target_class,
        receiver,
        name,
    } = callee
    else {
        panic!("method async callee")
    };
    assert_eq!(target_class.0, worker_index);
    assert_eq!(name.full_text(), "sibling");
    assert!(matches!(receiver.kind, hir::ExprKind::This));
    assert!(matches!(
        callee.receiver().map(|expr| &expr.kind),
        Some(hir::ExprKind::This)
    ));
    assert_eq!(args.len(), 1);
    assert!(matches!(args[0].kind, hir::ExprKind::Int(7)));
}

#[test]
fn r13_async_method_boundaries_have_pinned_checker_diagnostics() {
    let cases = [
        (
            "class C {\n  static async m(): Promise<void> {}\n}\nexport function main(): void {}\n",
            RuleCode::S100,
            2,
            "async static methods",
        ),
        (
            "class C {\n  async *m(): AsyncGenerator<i32> { yield 1; }\n}\nexport function main(): void {}\n",
            RuleCode::S100,
            2,
            "async generator methods",
        ),
        (
            "@ValueType\nclass C {\n  async m(): Promise<void> {}\n}\nexport function main(): void {}\n",
            RuleCode::S100,
            3,
            "`@ValueType` value classes",
        ),
        (
            "class C { async m(): Promise<void> {} }\nexport function main(): void {\n  const c: C = new C();\n  c.m();\n}\n",
            RuleCode::S013,
            4,
            "dropped without any await",
        ),
    ];
    for (source, code, line, message) in cases {
        let diagnostics = check_one(source).expect_err("R13 boundary must reject");
        assert_eq!(diagnostics[0].code, code);
        assert_eq!(diagnostics[0].pos.line, line);
        assert!(diagnostics[0].message.contains(message));
    }
}

#[test]
fn awaited_sync_method_and_async_method_value_are_rejected() {
    let awaited = check_one(
        "class C { m(): void {} }\nexport async function main(): Promise<void> {\n  await new C().m();\n}\n",
    )
    .expect_err("awaited sync method");
    assert_eq!(awaited[0].code, RuleCode::S100);
    assert!(awaited[0]
        .message
        .contains("synchronous and cannot be awaited"));

    let value = check_one(
        "class C { async m(): Promise<void> {} }\nexport function main(): void {\n  const c: C = new C();\n  c.m;\n}\n",
    )
    .expect_err("async method value");
    assert_eq!(value[0].code, RuleCode::S100);
    assert!(value[0].message.contains("not a first-class value"));
}

#[test]
fn eval_is_s002() {
    let err = check_one("export function main(): void {\n  eval(\"1\");\n}\n").unwrap_err();
    assert_eq!(err[0].code, RuleCode::S002);
}

#[test]
fn nonwhitelisted_array_member_is_s100_naming_the_member() {
    let err = check_one("export function main(): void {\n  const xs: i32[] = [1];\n  xs.map;\n}\n")
        .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("map"));
}

#[test]
fn nonwhitelisted_string_member_is_s100_naming_the_member() {
    // A member outside both the accepted §8 surface and the named
    // Q21 rejected set takes the generic S100 surface diagnostic.
    let err = check_one(
        "export function main(): void {\n  const s: string = \"a\";\n  print(s.reverse());\n}\n",
    )
    .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("reverse"));
}

#[test]
fn string_methods_type_and_normalize_optional_arguments() {
    // stdlib.md §8: every accepted method resolves to a Callee::Str
    // intrinsic with the receiver first; the optional arguments are
    // normalized at check time (start positions → 0, ending
    // positions → i32::MAX, `pad` → " ") so each runtime symbol
    // has a fixed arity.
    let module = check_one(
        "export function main(): void {\n  const s: string = \"ab\";\n  const sl: string = s.slice();\n  const i: i32 = s.indexOf(\"a\");\n  const b: boolean = s.includes(\"a\", 1);\n  const p: string = s.padStart(5);\n  const parts: string[] = s.split(\"a\");\n  const c: i32 = s.charCodeAt(0);\n  const sub: string = s.substring(1);\n  const at: string = s.charAt(0);\n  const cp: i32 = s.codePointAt(0);\n  const cat: string = s.concat(at);\n  const start: boolean = s.startsWith(\"a\");\n  const end: boolean = s.endsWith(\"b\");\n  print(`${sl}${i}${b}${p}${parts.length}${c}${sub}${cp}${cat}${start}${end}`);\n}\n",
    )
    .expect("clean check");
    let mut found = Vec::new();
    fn walk(e: &hir::Expr, found: &mut Vec<(hir::StrFn, usize)>) {
        if let hir::ExprKind::Call { callee, args } = &e.kind {
            if let hir::Callee::Str(f) = callee {
                found.push((*f, args.len()));
            }
            for a in args {
                walk(a, found);
            }
        }
    }
    for s in &module.functions[0].body {
        match s {
            hir::Stmt::Let { init, .. } => walk(init, &mut found),
            hir::Stmt::Expr(e) => walk(e, &mut found),
            _ => {}
        }
    }
    for f in [
        hir::StrFn::Slice,
        hir::StrFn::IndexOf,
        hir::StrFn::Includes,
        hir::StrFn::PadStart,
        hir::StrFn::Split,
        hir::StrFn::CharCodeAt,
        hir::StrFn::Substring,
        hir::StrFn::CharAt,
        hir::StrFn::CodePointAt,
        hir::StrFn::Concat,
        hir::StrFn::StartsWith,
        hir::StrFn::EndsWith,
    ] {
        let (_, arity) = found
            .iter()
            .find(|(g, _)| *g == f)
            .unwrap_or_else(|| panic!("no Callee::Str({}) call", f.name()));
        assert_eq!(*arity, 1 + f.params().len(), "arity of {}", f.name());
    }
}

#[test]
fn rejected_string_member_is_s014_naming_the_member() {
    for (member, call, q_rule) in [
        ("normalize", "s.normalize()", "Q21"),
        ("localeCompare", "s.localeCompare(s)", "Q21"),
        ("toLocaleLowerCase", "s.toLocaleLowerCase()", "Q21"),
        ("matchAll", "s.matchAll(s)", "Q31"),
        ("search", "s.search(s)", "Q31"),
    ] {
        let err = check_one(&format!(
            "export function main(): void {{\n  const s: string = \"a\";\n  {call};\n}}\n"
        ))
        .unwrap_err();
        assert_eq!(err[0].code, RuleCode::S014, "{member}");
        assert!(
            err[0].message.contains(member),
            "{member}: {}",
            err[0].message
        );
        assert!(
            err[0].message.contains(q_rule),
            "{member}: {}",
            err[0].message
        );
    }
}

#[test]
fn string_method_read_as_a_value_is_rejected() {
    let err =
        check_one("export function main(): void {\n  const s: string = \"a\";\n  s.indexOf;\n}\n")
            .unwrap_err();
    assert_eq!(err[0].code, RuleCode::S100);
    assert!(err[0].message.contains("only be called"));
}

#[test]
fn string_as_a_global_value_or_constructor_is_rejected() {
    // stdlib.md §8: `String` is not an accepted global. It resolves
    // nowhere, so each use fails on the standing unknown-name /
    // unsupported-construct paths — no dedicated S-code needed.
    for src in [
        "export function main(): void {\n  print(String(3));\n}\n",
        "export function main(): void {\n  print(String.fromCharCode(65));\n}\n",
        "export function main(): void {\n  print(String.raw`x`);\n}\n",
        "export function main(): void {\n  const s = new String(\"a\");\n  print(\"x\");\n}\n",
    ] {
        let err = check_one(src).unwrap_err();
        assert!(!err.is_empty(), "{src} was accepted");
    }
}
