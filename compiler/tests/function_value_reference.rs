//! Map rejection reasons and function field flow (compiler.md §123).

use subscript_compiler::{check_program, RuleCode, SourceFile};

#[test]
fn map_get_reasons_distinguish_scalars_from_non_nullable_types() {
    let scalar = "A scalar value type has no null miss value";
    let representation = "The value type has no `| null` form of the map's value representation";
    for (ty, reason) in [
        ("i32", scalar),
        ("boolean", scalar),
        ("E", scalar),
        ("Label", scalar),
        ("string", representation),
        ("Date", representation),
        ("RegExp", representation),
        ("Generator<i32>", representation),
        ("Value", representation),
        ("FixedArray<i32, 2>", representation),
        ("SubByValueI64Pair", representation),
    ] {
        let source = |operation: &str| {
            vec![
                SourceFile::ambient(
                    "interop.generated.d.ts",
                    include_str!("../../corpus/interop/interop.generated.d.ts"),
                ),
                SourceFile::new(
                    "test.ts",
                    format!(
                    "enum E {{ A }} type Label = \"a\" | \"b\";
                     @CStruct class Value {{ x: i32 = 0; }}
                     function read(m: Map<string, {ty}>, fallback: {ty}): void {{ m.{operation}; }}"
                ),
                ),
            ]
        };
        let diagnostics = check_program(&source("get(\"key\")")).expect_err(ty);
        assert_eq!(diagnostics.len(), 1, "{ty}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S014);
        assert!(
            diagnostics[0].message.contains(reason),
            "{ty}: {diagnostics:?}"
        );
        assert!(diagnostics[0].message.contains("getOr"), "{diagnostics:?}");
        check_program(&source("getOr(\"key\", fallback)")).expect(ty);
    }
}

#[test]
fn map_get_rejects_a_worker_value() {
    let files = [SourceFile::new(
        "test.ts",
        "class M { value: i32 = 0; }
         export function main(): void {
             const m = new Map<i32, Worker<M, M>>();
             m.get(1);
         }",
    )];
    let diagnostics = check_program(&files).expect_err("Worker map get");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S014);
    assert!(diagnostics[0]
        .message
        .contains("The value type has no `| null` form of the map's value representation"));
}

#[test]
fn nullable_field_calls_share_local_call_diagnostics_and_flow() {
    let source = |body: &str| {
        [SourceFile::new(
            "test.ts",
            format!(
                "class H {{ cb: ((x: i32) => i32) | null = null; }}
                 function run(h: H): i32 {{ {body} }}"
            ),
        )]
    };
    for body in [
        "if (h.cb !== null) { return h.cb(3); } return 0;",
        "if (null !== h.cb) { return h.cb(3); } return 0;",
        "if (h.cb === null) { return 0; } return h.cb(3);",
        "return h.cb !== null ? h.cb(3) : 0;",
        "return null === h.cb ? 0 : h.cb(3);",
    ] {
        check_program(&source(body)).expect(body);
    }
    for body in [
        "return h.cb(3);",
        "const cb = h.cb; return cb(3);",
        "if (h.cb === null) { return h.cb(3); } return 0;",
        "if (h.cb !== null) { h.cb = null; return h.cb(3); } return 0;",
        "if (h.cb !== null) { h.cb(3); } return h.cb(3);",
    ] {
        let diagnostics = check_program(&source(body)).expect_err(body);
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code == RuleCode::S100
                    && diagnostic.message == "type `((i32) => i32) | null` is not callable"
            }),
            "{body}: {diagnostics:?}"
        );
    }
}

#[test]
fn field_call_is_an_indirect_capture_boundary() {
    let source = |value: &str| {
        [SourceFile::new(
            "test.ts",
            format!(
                "function call(cb: () => i32): i32 {{ return cb(); }}
                 function named(): i32 {{ return 7; }}
                 class H {{ cb: (f: () => i32) => i32 = call; }}
                 export function main(): void {{
                   const h = new H(); const offset: i32 = 7;
                   print(`${{h.cb({value})}}`);
                 }}"
            ),
        )]
    };
    check_program(&source("named")).expect("clean argument");
    let diagnostics = check_program(&source("(): i32 => offset")).expect_err("capture escapes");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == RuleCode::S009
                && diagnostic.message.contains("indirect call argument")
        }),
        "{diagnostics:?}"
    );
}

#[test]
fn static_nullable_field_calls_share_instance_flow() {
    for receiver in ["h", "S"] {
        let source = |body: &str| {
            [SourceFile::new(
                "test.ts",
                format!(
                    "class S {{ static opt: ((x: i32) => i32) | null = null; }}
             class H {{ opt: ((x: i32) => i32) | null = null; }}
             function run(h: H): i32 {{ {body} }}"
                ),
            )]
        };
        for body in [
            format!("if ({receiver}.opt !== null) {{ return {receiver}.opt(5); }} return 0;"),
            format!("if ({receiver}.opt !== null) {{ const f = {receiver}.opt; return f(5); }} return 0;"),
            format!("return {receiver}.opt !== null ? {receiver}.opt(5) : 0;"),
        ] {
            check_program(&source(&body)).expect(&body);
        }
        for body in [
            format!("return {receiver}.opt(5);"),
            format!("if ({receiver}.opt !== null) {{ {receiver}.opt = null; return {receiver}.opt(5); }} return 0;"),
            format!("if ({receiver}.opt !== null) {{ {receiver}.opt(5); }} return {receiver}.opt(5);"),
        ] {
            let diagnostics = check_program(&source(&body)).expect_err(&body);
            assert!(diagnostics.iter().any(|d| d.code == RuleCode::S100
                && d.message.contains("is not callable")), "{diagnostics:?}");
        }
    }
}
