//! Exit and argument facts for compiler.md §164.
use subscript_compiler::divergence::Divergence;
use subscript_compiler::{check_program, hir, sequence_can_fall_through, SourceFile};

#[test]
fn exit_shapes_and_using_scopes_share_the_predicate() {
    let shapes = [
        ("while (true) { return x; }", "while (x > 0) { return x; }"),
        (
            "for (; true;) { return x; }",
            "for (; x > 0;) { return x; }",
        ),
        (
            "while (true) { if (x > 0) { return x; } }",
            "while (x > 0) { if (x > 0) { return x; } }",
        ),
        (
            "while (true) { throw new Error(\"e\"); }",
            "while (x > 0) { throw new Error(\"e\"); }",
        ),
        (
            "while (true) { switch (x) { case 1: break; default: return x; } }",
            "while (x > 0) { switch (x) { case 1: break; default: return x; } }",
        ),
        (
            "try { while (true) { return x; } } catch (e) { return 0; }",
            "try { while (x > 0) { return x; } } catch (e) { return 0; }",
        ),
        ("for (;;) { return x; }", "for (;;) { break; }"),
        ("if (true) { return x; }", "if (x > 0) { return x; }"),
        (
            "if (false) {} else { return x; }",
            "if (x > 0) {} else { return x; }",
        ),
        (
            "switch (x) { case 1: case 2: return 1; default: return 0; }",
            "switch (x) { case 1: case 2: break; default: return 0; }",
        ),
        (
            "if (x > 0) { return 1; } else if (true) { return 2; }",
            "if (x > 0) { return 1; } else if (x < 0) { return 2; }",
        ),
        (
            "switch (label) { case \"a\": case \"b\": return 1; }",
            "switch (label) { case \"a\": case \"b\": break; }",
        ),
    ];
    for (stopping, control) in shapes {
        for using in [false, true] {
            for (body, expected) in [(stopping, false), (control, true)] {
                let body = body
                    .replace("return x;", "return;")
                    .replace("return 0;", "return;")
                    .replace("return 1;", "return;")
                    .replace("return 2;", "return;");
                let prefix = if using { "using r: R = new R();" } else { "" };
                let source = format!("type Label = \"a\" | \"b\"; class R {{ [Symbol.dispose](): void {{}} }} function probe(x:i32, label:Label):void {{ {prefix} {body} }}");
                // Void bodies preserve the leavable control for the predicate.
                let module = check_program(&[SourceFile::new("exit.ts", source)]).unwrap();
                let body = &module
                    .functions
                    .iter()
                    .find(|f| f.name == "probe")
                    .unwrap()
                    .body;
                assert_eq!(
                    sequence_can_fall_through(body),
                    expected,
                    "{using}: {body:?}"
                );
                if using {
                    let hir::Stmt::Using {
                        body,
                        bindings: _,
                        finalizer: _,
                        pos: _,
                    } = body
                        .iter()
                        .find(|s| {
                            matches!(
                                s,
                                hir::Stmt::Using {
                                    bindings: _,
                                    body: _,
                                    finalizer: _,
                                    pos: _
                                }
                            )
                        })
                        .expect("using node")
                    else {
                        panic!("using scope");
                    };
                    assert_eq!(sequence_can_fall_through(body), expected);
                }
            }
        }
        let source = format!(
            "type Label = \"a\" | \"b\"; function probe(x:i32, label:Label):i32 {{ {stopping} }}"
        );
        check_program(&[SourceFile::new("return.ts", source)]).unwrap();
    }
}

#[test]
fn declaration_required_count_ends_at_the_last_required_parameter() {
    for (parameters, args, accepted) in [
        ("a:i32=1,b:i32", "2", false),
        ("a:i32=1,b:i32", "1,2", true),
        ("a:i32,b:i32=1,c:i32=2", "1", true),
    ] {
        let source = format!("function f({parameters}):i32 {{ return a; }} export function main():void {{ print(`${{f({args})}}`); }}");
        let result = check_program(&[SourceFile::entry("args.ts", source)]);
        assert_eq!(result.is_ok(), accepted, "{parameters}: {args}");
        if let Err(errors) = result {
            assert!(errors[0].message.contains("(2 required), got 1"));
            assert_eq!(errors[0].divergence, None);
        }
    }
}

#[test]
fn inferred_binding_facts_split_the_function_value_site() {
    for global in [false, true] {
        for initializer in ["(a:i32,b:i32=5):i32=>a+b", "f", "g"] {
            for (args, diverges) in [("1", true), ("", false)] {
                let declarations = format!("const g = f; const h = {initializer};");
                let (outside, inside) = if global {
                    (declarations.as_str(), "")
                } else {
                    ("", declarations.as_str())
                };
                let source = format!("function f(a:i32,b:i32=5):i32{{return a+b;}} {outside} export function main():void{{ {inside} print(`${{h({args})}}`); }}");
                let files = [SourceFile::entry("value.ts", source)];
                let errors = check_program(&files).unwrap_err();
                let error = &errors[0];
                assert_eq!(
                    error.divergence,
                    diverges.then_some(Divergence::FunctionValueOptionalArguments),
                    "{global}: {initializer}: {args}: {errors:?}"
                );
                assert!(error.message.starts_with("`h` expects 2 argument(s), got"));
                assert!(error
                    .note
                    .unwrap()
                    .contains("function type has no optional parameter"));
                let rendered = subscript_compiler::render_diagnostics(&files, &errors);
                assert!(rendered.contains("= note:"));
                if diverges {
                    assert!(
                        rendered.contains("= why: A function type has no optional parameter (C7).")
                    );
                    assert_eq!(
                        rendered
                            .matches("pass every argument, or call a declaration")
                            .count(),
                        1
                    );
                }
            }
        }
    }
    for source in [
        "function probe(k:(a:i32,b:i32)=>i32):void { k(1); }",
        "function probe():void { const h:(a:i32,b:i32)=>i32=(a:i32,b:i32=5):i32=>a+b; h(1); }",
    ] {
        let errors = check_program(&[SourceFile::new("control.ts", source)]).unwrap_err();
        assert_eq!(errors[0].divergence, None);
    }
}

#[test]
fn inferred_binding_assignments_preserve_the_required_count() {
    for global in [false, true] {
        for initializer in ["(a:i32,b:i32=5):i32=>a+b", "f", "alias"] {
            for (value, accepted) in [
                ("(a:i32,b:i32):i32=>a*b", false),
                ("k", false),
                ("f2", false),
                ("(a:i32,b:i32=7):i32=>a", true),
                ("f", true),
                ("alias", true),
            ] {
                let declarations = format!("const alias=f; const k:(a:i32,b:i32)=>i32=(a:i32,b:i32):i32=>a*b; let h={initializer};");
                let (outside, inside) = if global {
                    (declarations.as_str(), "")
                } else {
                    ("", declarations.as_str())
                };
                let source = format!("function f(a:i32,b:i32=5):i32{{return a+b;}} function f2(a:i32,b:i32):i32{{return a*b;}} {outside} export function main():void{{ {inside} h={value}; print(`${{h(1,2)}}`); }}");
                let result = check_program(&[SourceFile::entry("assignment.ts", source)]);
                assert_eq!(result.is_ok(), accepted, "{global}: {initializer}: {value}");
                if let Err(errors) = result {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].code, subscript_compiler::RuleCode::S100);
                    assert_eq!(errors[0].divergence, None);
                    assert!(errors[0].message.contains("required argument"));
                }
            }
        }
    }
}

#[test]
fn generic_required_count_ends_at_the_last_required_parameter() {
    for (parameters, args, accepted) in [
        ("a:i32=1,b:T", "2", false),
        ("a:i32=1,b:T", "1,2", true),
        ("b:T,a:i32=1", "2", true),
    ] {
        let source = format!("function f<T>({parameters}):T{{return b;}} export function main():void{{print(`${{f({args})}}`);}}");
        let result = check_program(&[SourceFile::entry("generic.ts", source)]);
        assert_eq!(result.is_ok(), accepted, "{parameters}: {args}");
        if let Err(errors) = result {
            assert_eq!(errors[0].code, subscript_compiler::RuleCode::S100);
            assert_eq!(errors[0].divergence, None);
        }
    }
}
