use subscript_compiler::{check_program, RuleCode, SourceFile};

#[test]
fn scalar_searches_keep_the_find_index_diagnostic() {
    for method in ["find", "findLast"] {
        let source = format!(
            "export function main(): void {{ [1, 2].{method}((v: i32): boolean => true); }}"
        );
        let diagnostics = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, RuleCode::S014);
        assert_eq!(diagnostics[0].message, format!("`{method}` is rejected: A scalar element type has no miss value; use `findIndex` (Q22)"));
    }
}

#[test]
fn new_array_forms_keep_argument_and_callback_limits() {
    for expression in [
        "a.at()",
        "a.at(0, 1)",
        "a.at(true)",
        "\"abc\".at()",
        "\"abc\".at(0, 1)",
        "a.flatMap((v: i32): i32 => v)",
        "a.flatMap((v: i32): i32[] => [v], a)",
        "a.flatMap((v: i32, i: i32, xs: i32[]): i32[] => xs)",
        "a.findLastIndex((v: i32): i32 => v)",
        "cs.find((c: C): boolean => true, cs)",
        "cs.findLast((c: C, i: i32, xs: C[]): boolean => true)",
    ] {
        let source = format!("class C {{ v: i32 = 1; }} export function main(): void {{ const a: i32[] = [1]; const cs: C[] = [new C()]; {expression}; }}");
        assert!(
            check_program(&[SourceFile::new("test.ts", source)]).is_err(),
            "{expression}"
        );
    }
    check_program(&[SourceFile::new(
        "test.ts",
        include_str!("../../corpus/accept/a268-array-string-at-find-flatmap.ts"),
    )])
    .unwrap();
}

#[test]
fn nullable_reference_elements_and_handle_elements_check() {
    check_program(&[SourceFile::new(
        "test.ts",
        r#"
class C { v: i32 = 1; }
export function main(): void {
  const cs: (C | null)[] = [new C(), null];
  const first: C | null = cs.find((c: C | null): boolean => c !== null);
  const last: C | null = cs.findLast((c: C | null): boolean => c === null);
  const arrays: i32[][] = [[1]];
  const arr = arrays.find((a: i32[]): boolean => true);
  const maps: Map<i32, C>[] = [new Map<i32, C>()];
  const map: Map<i32, C> | null = maps.find((m: Map<i32, C>): boolean => true);
}
"#,
    )])
    .unwrap();
}

#[test]
fn flat_map_cannot_return_a_capture_from_its_callback() {
    let files = [SourceFile::new(
        "test.ts",
        r#"
class C { callback: () => i32; constructor(f: () => i32) { this.callback = f; } }
export function main(): void {
  const n: i32 = 7;
  [1].flatMap((v: i32): C[] => [new C((): i32 => n)]);
}
"#,
    )];
    let diagnostics = check_program(&files).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == RuleCode::S009),
        "{diagnostics:?}"
    );
    check_program(&[SourceFile::new(
        "test.ts",
        r#"
export function main(): void {
  const n: i32 = 7;
  [1].flatMap((v: i32): i32[] => [v + n]);
}
"#,
    )])
    .unwrap();
}
