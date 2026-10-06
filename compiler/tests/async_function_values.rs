//! Async body facts and capture boundaries (§167).
use subscript_compiler::{check_program, hir::ExprKind, RuleCode, SourceFile, Type};

#[test]
fn deferred_initializers_keep_body_and_callable_results_with_synchronous_control() {
    for asynchronous in [false, true] {
        let flag = if asynchronous { "async " } else { "" };
        let body = if asynchronous {
            "await value()"
        } else {
            "value()"
        };
        let source = format!(
            "async function value(): Promise<i32> {{ return 7; }}
            const job: () => Promise<i32> = {flag}(): Promise<i32> => {body};
            export async function main(): Promise<void> {{ print(`${{await job()}}`); }}"
        );
        let module = check_program(&[SourceFile::new("facts.ts", source)]).expect("checks");
        let initializer = &module
            .globals
            .iter()
            .find(|global| global.name == "job")
            .expect("job")
            .init;
        let ExprKind::Lambda { is_async, ret, .. } = &initializer.kind else {
            panic!("lambda");
        };
        assert_eq!(*is_async, asynchronous);
        assert_eq!(
            *ret,
            if asynchronous {
                Type::I32
            } else {
                Type::AsyncHandle(Box::new(Type::I32))
            }
        );
        assert_eq!(
            initializer.ty.function_type().expect("callable").ret,
            Type::AsyncHandle(Box::new(Type::I32))
        );
    }
}

#[test]
fn local_parameter_and_receiver_captures_name_the_binding_with_clean_controls() {
    for (captured, clean, name) in [
        (
            "function f(): void { const n: i32 = 7; const job = async (): Promise<i32> => n; }",
            "function f(): void { const n: i32 = 7; const job = (): i32 => n; }",
            "n",
        ),
        (
            "function f(n: i32): void { const job = async (): Promise<i32> => n; }",
            "function f(n: i32): void { const job = async (n: i32): Promise<i32> => n; }",
            "n",
        ),
        (
            "class C { n: i32 = 7; f(): void { const job = async (): Promise<i32> => this.n; } }",
            "class C { n: i32 = 7; f(): void { const job = (): i32 => this.n; } }",
            "this",
        ),
    ] {
        let diagnostics =
            check_program(&[SourceFile::new("capture.ts", captured)]).expect_err("capture rejects");
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == RuleCode::S009
                && diagnostic.message.contains(&format!("`{name}`"))
                && diagnostic
                    .message
                    .contains("an async arrow captures nothing")));
        check_program(&[SourceFile::new("control.ts", clean)]).expect("clean control checks");
    }
}

#[test]
fn async_body_requires_observation_with_awaited_control() {
    for observed in [false, true] {
        let call = if observed {
            "await value();"
        } else {
            "value();"
        };
        let source = format!("async function value(): Promise<i32> {{ return 7; }} function f(): void {{ const job = async (): Promise<void> => {{ {call} }}; }}");
        let result = check_program(&[SourceFile::new("observation.ts", source)]);
        if observed {
            result.expect("observed");
        } else {
            assert!(result
                .expect_err("dropped")
                .iter()
                .any(|d| d.code == RuleCode::S013));
        }
    }
}

#[test]
fn handle_returns_reject_with_explicit_await_controls() {
    for declaration in [
        "const job = async (h: Promise<i32>): Promise<i32> => BODY;",
        "const job: (h: Promise<i32>) => Promise<i32> = async (h) => BODY;",
        "const job = async (h: Promise<i32>) => BODY;",
        "const job = async (h: Promise<i32>): Promise<i32> => { return BODY; };",
        "async function job(h: Promise<i32>): Promise<i32> { return BODY; }",
    ] {
        let rejected = declaration.replace("BODY", "h");
        let diagnostics = check_program(&[SourceFile::new("return.ts", rejected)])
            .expect_err("handle return rejects");
        assert_eq!(diagnostics.len(), 1, "{declaration}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S100);
        assert_eq!(
            diagnostics[0].message,
            "type mismatch: the return value expects `i32`, got `Promise<i32>`"
        );
        assert_eq!(
            diagnostics[0].divergence,
            Some(subscript_compiler::divergence::Divergence::AsyncReturnHandle)
        );
        check_program(&[SourceFile::new(
            "control.ts",
            declaration.replace("BODY", "await h"),
        )])
        .expect("explicit await checks");
    }
}

#[test]
fn nested_receiver_capture_rejects_with_synchronous_controls() {
    for depth in [1, 2] {
        let mut body = "this.n".to_string();
        for _ in 0..depth {
            body = format!("(() => {body})()");
        }
        for asynchronous in [false, true] {
            let flag = if asynchronous { "async " } else { "" };
            let result = if asynchronous { "Promise<i32>" } else { "i32" };
            let source = format!("class C {{ n: i32 = 7; f(): void {{ const job = {flag}(): {result} => {body}; }} }}");
            let checked = check_program(&[SourceFile::new("nested-this.ts", source)]);
            if asynchronous {
                assert!(checked
                    .expect_err("nested receiver capture rejects")
                    .iter()
                    .any(|diagnostic| {
                        diagnostic.code == RuleCode::S009
                            && diagnostic.message
                                == "async arrow captures `this`; an async arrow captures nothing"
                    }));
            } else {
                checked.expect("synchronous receiver capture checks");
            }
        }
    }
}
