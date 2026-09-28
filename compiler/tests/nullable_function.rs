//! Nullable function flow and capture boundaries (compiler.md §122).

use subscript_compiler::types::{scalar_size_align, FuncType, HandleKind};
use subscript_compiler::{check_program, RuleCode, SourceFile, Type};

#[test]
fn function_signature_and_layout_cover_both_forms() {
    let signature = FuncType {
        params: vec![Type::I32],
        ret: Type::I32,
    };
    let function = Type::Func(Box::new(signature.clone()));
    let nullable = Type::Nullable(Box::new(function.clone()));
    for (ty, kind) in [
        (function, HandleKind::Func),
        (nullable, HandleKind::NullableFunc),
    ] {
        assert_eq!(ty.function_type(), Some(&signature));
        assert_eq!(scalar_size_align(&ty), Some((16, 8)));
        assert_eq!(ty.handle_kind(&[]), Some(kind));
    }
    for ty in [
        Type::I32,
        Type::Null,
        Type::Nullable(Box::new(Type::Object)),
    ] {
        assert_eq!(ty.function_type(), None);
        assert_ne!(scalar_size_align(&ty), Some((16, 8)));
    }
}

fn check_body(
    body: &str,
) -> Result<subscript_compiler::hir::Module, Vec<subscript_compiler::Diagnostic>> {
    check_program(&[SourceFile::new(
        "test.ts",
        format!("function run(f: ((x: i32) => i32) | null): i32 {{ {body} }}"),
    )])
}

#[test]
fn all_null_flow_paths_use_the_same_identifier_read() {
    for body in [
        "if (f !== null) { return f(3); } return 0;",
        "if (null !== f) { return f(3); } return 0;",
        "if (f === null) { return 0; } else { return f(3); }",
        "if (null === f) { return 0; } return f(3);",
        "return f !== null ? f(3) : 0;",
        "return null === f ? 0 : f(3);",
    ] {
        check_body(body).expect(body);
    }
    for body in [
        "return f(3);",
        "if (f === null) { return f(3); } return 0;",
        "return f === null ? f(3) : 0;",
        "if (f !== null) { f = null; return f(3); } return 0;",
        "if (f !== null) { f(3); } return f(3);",
        "return f!(3);",
        "f?.(3); return 0;",
    ] {
        let diagnostics = check_body(body).expect_err(body);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == RuleCode::S100),
            "{body}: {diagnostics:?}"
        );
    }
}

#[test]
fn nullable_capture_follows_the_storage_boundary() {
    let source = |value: &str| {
        [SourceFile::new(
            "test.ts",
            format!(
                r#"
            class Holder {{ callback: (() => i32) | null = null; }}
            function named(): i32 {{ return 7; }}
            export function main(): void {{
                const offset: i32 = 7;
                const holder = new Holder();
                const f: (() => i32) | null = {value};
                holder.callback = f;
            }}
        "#
            ),
        )]
    };
    check_program(&source("named")).expect("named function can be stored");
    let diagnostics =
        check_program(&source("(): i32 => offset")).expect_err("capture must not escape");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == RuleCode::S009));
}

#[test]
fn nullable_function_display_groups_the_function() {
    let function = Type::Func(Box::new(FuncType {
        params: vec![Type::I32],
        ret: Type::I32,
    }));
    assert_eq!(function.to_string(), "(i32) => i32");
    assert_eq!(
        Type::Nullable(Box::new(function)).to_string(),
        "((i32) => i32) | null"
    );
    let diagnostics = check_body("return f(3);").expect_err("unguarded call");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == RuleCode::S100
                && diagnostic
                    .message
                    .contains("type `((i32) => i32) | null` is not callable")
        }),
        "{diagnostics:?}"
    );
}

#[test]
fn nullable_function_capture_diagnostic_groups_the_function() {
    let diagnostics = check_program(&[SourceFile::new(
        "test.ts",
        "function escape(f: ((x: i32) => i32) | null, choose: boolean): ((x: i32) => i32) | null { return choose ? f : null; } export function main(): void { const offset: i32 = 1; const f: ((x: i32) => i32) | null = (x: i32): i32 => x + offset; escape(offset > 0 ? f : null, true); }",
    )]).expect_err("capture must not escape");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == RuleCode::S009
                && diagnostic.message.contains("((i32) => i32) | null")
        }),
        "{diagnostics:?}"
    );
}
