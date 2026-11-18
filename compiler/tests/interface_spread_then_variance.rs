//! Handle variance at the three places of compiler.md §334 rules 1 to 4.
//! Cost: short checker calls; the corpus golden sweeps cover both execution tiers.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

fn check(source: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::new("variance.ts", source)])
}

fn rejected(source: &str, text: &str) {
    let errors = check(source).expect_err("the violating form must fail");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100);
    assert!(errors[0].message.contains(text), "{errors:?}");
}

#[test]
fn interface_methods_take_results_covariantly_and_parameters_contravariantly() {
    for (required, found, body, accepted) in [
        ("(): C | null", "(): C", "return new C();", true),
        ("(): string | null", "(): string", "return \"text\";", true),
        ("(x: C): i32", "(x: C | null): i32", "return 1;", true),
        ("(x: C): i32", "(x: C): i32", "return 1;", true),
        ("(x: C | null): i32", "(x: C): i32", "return 1;", false),
        ("(): C", "(): C | null", "return null;", false),
        ("(x: i32): i32", "(x: i64): i32", "return 1;", false),
    ] {
        let source = format!(
            "class C {{ v: i32 = 1; }} interface I {{ m{required}; }}
             class K implements I {{ m{found} {{ {body} }} }}
             export function main(): void {{}}"
        );
        if accepted {
            check(&source).expect("the method fits its interface");
        } else {
            rejected(&source, "the interface requires");
        }
    }
}

#[test]
fn spread_copies_take_fitting_elements_and_keep_source_locations_invariant() {
    for (from, to, accepted) in [
        ("C", "C | null", true),
        ("C", "C", true),
        ("C | null", "C", false),
        ("i32", "i64", false),
    ] {
        let source = format!(
            "class C {{ v: i32 = 1; }} function id(x: C): {from} {{ return new C(); }}
             export function main(): void {{
               const gs: ((x: C) => {from})[] = [id];
               const fs: ((x: C) => {to})[] = [...gs];
             }}"
        );
        // Numeric controls use a function with a numeric result.
        let source = if from == "i32" {
            source.replace("return new C();", "return 1;")
        } else {
            source
        };
        if accepted {
            check(&source).expect("the copied element fits");
        } else {
            rejected(&source, "the array element expects");
        }
    }
    for container in ["Set<(x: C) => C>", "FixedArray<(x: C) => C, 1>"] {
        let init = if container.starts_with("Set") {
            "new Set<(x: C) => C>([id])"
        } else {
            "[id]"
        };
        let source = format!(
            "class C {{ v: i32 = 1; }} function id(x: C): C {{ return x; }}
             export function main(): void {{
               const gs: {container} = {init};
               const fs: ((x: C) => C | null)[] = [...gs];
             }}"
        );
        let errors = check(&source).expect_err("only array copies take element variance");
        assert!(
            errors.iter().any(|error| error.code == RuleCode::S100
                && error.message.contains("the array element expects")),
            "{errors:?}"
        );
        if container.starts_with("Set") {
            assert!(errors.iter().any(|error| error.code == RuleCode::S014));
        }
    }
    rejected(
        "class C { v: i32 = 1; } export function main(): void {
           const xs: C[] = [new C()]; const ys: (C | null)[] = xs;
         }",
        "the initializer expects",
    );
}

/// compiler.md §334 rule 4: each copied function takes the element place.
#[test]
fn a_spread_records_the_function_element_conversion() {
    for (source, target, initializer) in [
        ("(x: C) => C", "(x: C) => C | null", "[id]"),
        (
            "((x: C) => C) | null",
            "((x: C) => C | null) | null",
            "[id, null]",
        ),
    ] {
        let module = check(&format!(
            "class C {{ v: i32 = 7; }}
             function id(x: C): C {{ return x; }}
             export function main(): void {{
               const gs: ({source})[] = {initializer};
               const fs: ({target})[] = [...gs];
             }}",
        ))
        .expect("the function elements fit");
        let main = module
            .functions
            .iter()
            .find(|f| f.name == "main")
            .expect("main");
        let hir::Stmt::Let {
            ty: Type::Array(place),
            init,
            ..
        } = &main.body[1]
        else {
            panic!("the target is an array binding");
        };
        let hir::ExprKind::ArraySpreadLit(elements) = &init.kind else {
            panic!("the initializer is a spread copy");
        };
        let hir::ExprKind::Call {
            callee: hir::Callee::Arr(hir::ArrFn::Map),
            args,
        } = &elements[0].expr.kind
        else {
            panic!("the spread must record each converted element");
        };
        assert_eq!(elements[0].expr.ty, Type::array((**place).clone()));
        let hir::ExprKind::Lambda {
            params, ret, body, ..
        } = &args[1].kind
        else {
            panic!("the copy has an element adapter");
        };
        let Type::Array(source) = &args[0].ty else {
            panic!("the original source is an array");
        };
        assert_eq!(params[0].ty, **source);
        assert_eq!(ret, &**place);
        assert_ne!(source, place);
        let hir::Stmt::Return {
            value: Some(value), ..
        } = &body[0]
        else {
            panic!("the adapter returns its element");
        };
        assert_eq!(value.ty, **place);
        let hir::ExprKind::Cast(expr) = &value.kind else {
            panic!("the element has a recorded conversion");
        };
        assert_eq!(expr.ty, **source);
    }
}

#[test]
fn then_parameters_take_delivered_values_and_record_the_callback_place() {
    for named in [false, true] {
        for (delivered, parameter, accepted) in [
            ("C", "C | null", true),
            ("C", "C", true),
            ("C | null", "C", false),
            ("i32", "i64", false),
        ] {
            let callback = if named {
                "useValue"
            } else {
                "(x: PLACE): i32 => 1"
            };
            let result = if delivered == "i32" { "1" } else { "new C()" };
            let source = format!(
                "class C {{ v: i32 = 1; }}
                 async function mk(): Promise<{delivered}> {{ return {result}; }}
                 function useValue(x: {parameter}): i32 {{ return 1; }}
                 export async function main(): Promise<void> {{
                   print(`${{await mk().then({callback})}}`);
                 }}"
            )
            .replace("PLACE", parameter);
            if accepted {
                let module = check(&source).expect("the delivered value fits the parameter");
                if delivered != parameter {
                    let mut casts = Vec::new();
                    for function in &module.functions {
                        for statement in &function.body {
                            collect_casts(hir::HirChild::Stmt(statement), &mut casts);
                        }
                    }
                    assert!(casts.iter().any(|ty| matches!(ty, Type::Func(signature)
                        if signature.params.len() == 1
                        && matches!(signature.params[0], Type::Class(_)))));
                }
            } else {
                let errors = check(&source).expect_err("the delivered value does not fit");
                assert!(
                    errors
                        .iter()
                        .any(|d| d.message.contains("callback parameter must be")),
                    "{errors:?}"
                );
            }
        }
    }
}

#[test]
fn a_converted_then_callback_keeps_capture_checks_and_ownership() {
    let errors = check(
        "class C { v: i32 = 1; }
         async function mk(): Promise<C> { return new C(); }
         class K { v: i32 = 2; async run(): Promise<i32> {
           return await mk().then((x: C | null): i32 => this.v);
         } }
         export async function main(): Promise<void> { print(`${await new K().run()}`); }",
    )
    .expect_err("a callback cannot capture this");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S009);
    assert!(errors[0].message.contains("cannot capture `this`"));
    let module = check(
        "class C { v: i32 = 1; }
         async function mk(): Promise<C> { return new C(); }
         export async function main(): Promise<void> {
           const n: i32 = 2;
           print(`${await mk().then((x: C | null): i32 => n)}`);
         }",
    )
    .expect("a const capture fits");
    fn owns(child: hir::HirChild<'_>) -> bool {
        match child {
            hir::HirChild::Expr(expr) => {
                matches!(&expr.kind, hir::ExprKind::Lambda {
                    owns_environment: true, captures, ..
                } if !captures.is_empty())
                    || expr.children().into_iter().any(owns)
            }
            hir::HirChild::Stmt(stmt) => stmt.children().into_iter().any(owns),
        }
    }
    assert!(module
        .functions
        .iter()
        .flat_map(|f| &f.body)
        .any(|stmt| owns(hir::HirChild::Stmt(stmt))));
}

fn collect_casts(child: hir::HirChild<'_>, out: &mut Vec<Type>) {
    match child {
        hir::HirChild::Expr(expr) => {
            if matches!(expr.kind, hir::ExprKind::Cast { .. }) {
                out.push(expr.ty.clone());
            }
            for child in expr.children() {
                collect_casts(child, out);
            }
        }
        hir::HirChild::Stmt(stmt) => {
            for child in stmt.children() {
                collect_casts(child, out);
            }
        }
    }
}

#[test]
fn accept_entry_checks_and_narrow_parameter_entry_keeps_its_text() {
    check(include_str!(
        "../../corpus/accept/a484-interface-spread-then-handle-variance.ts"
    ))
    .expect("compiler.md §334 acceptance 1");
    let errors = check(include_str!(
        "../../corpus/reject/r555-interface-method-narrow-parameter.ts"
    ))
    .expect_err("compiler.md §334 acceptance 2");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message,
        "class `K` implements `m` of interface `I` with type `(C) => i32`; the interface requires `(C | null) => i32`");
}
