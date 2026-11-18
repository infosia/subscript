//! C callback places use the ordinary prefix rule (compiler.md §328).
//! Cost: short checker runs, with no code generation or external process.

#[path = "corpus/interop.rs"]
#[allow(dead_code)]
mod interop;

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

fn check(source: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[
        interop::mirror("interop.generated.d.ts", SourceFile::ambient),
        SourceFile::new("callback.ts", source),
    ])
}

#[test]
fn a_named_prefix_becomes_an_environment_free_callback_adapter() {
    for (declaration, count) in [
        ("function named(message: string): void {}", 1),
        ("function named(): void {}", 0),
    ] {
        let module = check(&format!(
            "{declaration} export function main(): void {{ const cb: SubLogCallback = named; }}"
        ))
        .unwrap();
        let main = module.functions.iter().find(|f| f.name == "main").unwrap();
        let [hir::Stmt::Let { init, .. }] = main.body.as_slice() else {
            panic!("{:?}", main.body);
        };
        let callback = init.ty.function_type().unwrap();
        assert_eq!(callback.callback_typedef.as_deref(), Some("SubLogCallback"));
        assert_eq!(callback.params.len(), 3);
        let hir::ExprKind::FuncRef(symbol) = &init.kind else {
            panic!("{init:?}");
        };
        let helper = module
            .functions
            .iter()
            .find(|f| f.symbol == *symbol)
            .unwrap();
        assert!(helper.synthesized_helper);
        assert_eq!(helper.params.len(), 3);
        let [hir::Stmt::Expr(hir::Expr {
            kind: hir::ExprKind::Call { args, .. },
            ..
        })] = helper.body.as_slice()
        else {
            panic!("{:?}", helper.body);
        };
        assert_eq!(args.len(), count);
        for (argument, parameter) in args.iter().zip(&helper.params) {
            assert!(
                matches!(&argument.kind, hir::ExprKind::Local(name, ..) if *name == parameter.name)
            );
        }
    }
}

#[test]
fn named_adapters_share_only_the_same_function_and_callback_type() {
    let mirror = "// @subscript-c-header include=\"callback.h\"\n\
                  // @subscript-c-callback typedef=\"Callback\" aggregate=\"SubStringView\" data=\"data\" len=\"len\"\n\
                  type Callback = (a: i32, b: i32) => i32;\n\
                  // @subscript-c-callback typedef=\"Other\" aggregate=\"SubStringView\" data=\"data\" len=\"len\"\n\
                  type Other = (a: i32, b: i32) => i32;";
    let module = check_program(&[
        SourceFile::ambient("callback.generated.d.ts", mirror),
        SourceFile::new("callback.ts", "function named(a: i32): i32 { return a; } function full(a: i32, b: i32): i32 { return a + b; } export function main(): void { const a: Callback = named; const b: Callback = named; const c: Other = named; const d: Callback = full; }"),
    ]).unwrap();
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let symbols: Vec<_> = main
        .body
        .iter()
        .map(|statement| {
            let hir::Stmt::Let { init, .. } = statement else {
                panic!("{statement:?}");
            };
            let hir::ExprKind::FuncRef(symbol) = &init.kind else {
                panic!("{init:?}");
            };
            symbol
        })
        .collect();
    assert_eq!(symbols[0], symbols[1]);
    assert_ne!(symbols[0], symbols[2]);
    assert_eq!(symbols[3].source_name(), "full");
    assert_eq!(
        module
            .functions
            .iter()
            .filter(|f| f.synthesized_helper)
            .count(),
        2
    );
}

#[test]
fn callback_prefixes_keep_the_ordinary_parameter_and_result_rules() {
    for value in [
        "named",
        "(): i32 => 7",
        "(message: string | null): void => {}",
    ] {
        check(&format!(
            "function named(message: string | null): i32 {{ return 7; }} \
             export function main(): void {{ const cb: SubLogCallback = {value}; }}"
        ))
        .unwrap_or_else(|errors| panic!("{value}: {errors:?}"));
    }
}

#[test]
fn incompatible_parameters_name_the_parameter_and_both_types() {
    let errors = check(include_str!(
        "../../corpus/reject/r541-c-callback-named-function-parameter-type.ts"
    ))
    .unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert_eq!(errors[0].pos.line, 13);
    assert_eq!(errors[0].divergence, None);
    assert_eq!(
        errors[0].message,
        "a function whose parameter 1 has type `i32` but must accept `string` cannot take the C callback type `SubLogCallback`"
    );
    let errors = check(
        "export function main(): void { const cb: SubLogCallback = (message: i32): void => {}; }",
    )
    .unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("parameter 1")));
}

#[test]
fn incompatible_results_and_extra_parameters_name_the_cause() {
    let mirror = "// @subscript-c-header include=\"callback.h\"\n\
                  // @subscript-c-callback typedef=\"Callback\" aggregate=\"SubStringView\" data=\"data\" len=\"len\"\n\
                  type Callback = (a: i32, b: i32) => i32;";
    for (declaration, cause) in [
        (
            "function named(a: i32): string { return \"wrong\"; }",
            "return type `string` does not fit `i32`",
        ),
        (
            "function named(a: i32, b: i32, c: i32): i32 { return c; }",
            "parameter 3 has no corresponding callback parameter",
        ),
        (
            "function named(a: i32, b: i32, c: i32 = 7): i32 { return c; }",
            "parameter 3 has no corresponding callback parameter",
        ),
        (
            "function named(a: i32 = 1): i32 { return a; }",
            "default parameter",
        ),
    ] {
        let source =
            format!("{declaration} export function main(): void {{ const cb: Callback = named; }}");
        let errors = check_program(&[
            SourceFile::ambient("callback.generated.d.ts", mirror),
            SourceFile::new("callback.ts", source),
        ])
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains(cause)
                    && e.message.contains("C callback type `Callback`")),
            "{errors:?}"
        );
        if declaration.contains("a: i32 = 1") {
            assert!(errors
                .iter()
                .any(|e| e.message.contains("extra parameters")));
        }
    }
}

#[test]
fn an_async_function_names_its_async_kind() {
    let errors = check(
        "async function named(message: string): Promise<void> {} export function main(): void { const cb: SubLogCallback = named; }",
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|e| e
            .message
            .contains("an async function cannot take the C callback type `SubLogCallback`")),
        "{errors:?}"
    );
}

#[test]
fn a_prefix_never_converts_a_capture_or_a_plain_stored_function() {
    for body in [
        "const n: i32 = 7; const cb: SubLogCallback = (message: string): void => { print(`${n}`); };",
        "const plain = (message: string): void => {}; const cb: SubLogCallback = plain;",
    ] {
        let errors = check(&format!("export function main(): void {{ {body} }}")).unwrap_err();
        assert!(errors.iter().any(|e| e.code == RuleCode::S100
            && e.message.contains("C callback type `SubLogCallback`")));
    }
    // The stored C callback keeps its identity (compiler.md §274 rule 7).
    let module = check(
        "function named(): void {} export function main(): void { const cb: SubLogCallback = named; const copy: SubLogCallback = cb; }",
    )
    .unwrap();
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let hir::Stmt::Let { init, .. } = &main.body[1] else {
        panic!("{:?}", main.body);
    };
    assert!(matches!(init.ty, Type::Func(_)));
    assert_eq!(
        init.ty.function_type().unwrap().callback_typedef.as_deref(),
        Some("SubLogCallback")
    );
}
