//! The checked exception surface of `compiler.md` §115.1–§115.3.

use subscript_compiler::hir::{self, Stmt};
use subscript_compiler::{check_program, check_warnings, Diagnostic, RuleCode, SourceFile};

fn check(source: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::new("exception.ts", source)])
}

fn first_error(source: &str) -> Diagnostic {
    check(source)
        .expect_err("the program is rejected")
        .into_iter()
        .next()
        .expect("one diagnostic")
}

fn in_main(body: &str) -> String {
    format!("export function main(): void {{\n{body}\n}}\n")
}

#[test]
fn the_three_classes_construct_with_and_without_a_message() {
    let module = check(&in_main(
        "  const a: Error = new Error();\n\
         \x20 const b: SyntaxError = new SyntaxError(\"b\");\n\
         \x20 const c: TypeError = new TypeError(\"c\");\n\
         \x20 print(`${a.name}${b.name}${c.message}`);",
    ))
    .expect("the Error classes check clean");
    let error = module
        .classes
        .iter()
        .find(|class| class.name == "Error")
        .expect("one Error class");
    let fields = error
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(fields, [hir::ERROR_KIND_FIELD, "name", "message"]);
    assert_eq!(
        module
            .classes
            .iter()
            .filter(
                |class| class.fields.first().map(|field| field.name.as_str())
                    == Some(hir::ERROR_KIND_FIELD)
            )
            .count(),
        1,
        "the three names share one class"
    );
}

#[test]
fn a_call_without_new_and_an_undeclared_member_are_rejected() {
    let call = first_error(&in_main("  const e: Error = Error(\"x\");"));
    assert_eq!(call.code, RuleCode::S100);
    assert!(
        call.message.contains("new Error(message)"),
        "{}",
        call.message
    );

    let member = first_error(&in_main(
        "  const e: Error = new Error(\"x\");\n  print(e.stack);",
    ));
    assert_eq!(member.code, RuleCode::S018);
    assert!(member.message.contains("`stack`"), "{}", member.message);
}

#[test]
fn a_throw_of_a_non_error_operand_is_rejected_and_an_error_operand_is_accepted() {
    for operand in ["42", "\"x\""] {
        let diagnostic = first_error(&in_main(&format!("  throw {operand};")));
        assert_eq!(diagnostic.code, RuleCode::S010, "{operand}");
        assert!(diagnostic.message.contains("`throw` requires"), "{operand}");
    }
    let module = check(&in_main("  throw new TypeError(\"x\");")).expect("an Error operand");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    assert!(matches!(main.body.as_slice(), [Stmt::Throw { .. }]));
    assert!(main.can_raise);
}

#[test]
fn a_throw_ends_the_flow_of_a_non_void_function() {
    check(
        "function never(): i32 {\n  throw new Error(\"x\");\n}\n\
         export function main(): void { print(`${never()}`); }\n",
    )
    .expect("a throw is a terminating statement");
}

#[test]
fn the_catch_binding_has_two_legal_uses_before_narrowing() {
    check(&in_main(
        "  try {\n    throw new Error(\"x\");\n  } catch (e) {\n\
         \x20   if (e instanceof SyntaxError) { print(e.message); }\n\
         \x20   throw e;\n  }",
    ))
    .expect("instanceof and a rethrow are the legal uses");
    check(&in_main("  try {\n    print(\"x\");\n  } catch (e) {\n  }"))
        .expect("an unused binding is accepted");

    for use_site in [
        "print(e.message);",
        "const f: Error = e;",
        "e.message = \"y\";",
    ] {
        let diagnostic = first_error(&in_main(&format!(
            "  try {{\n    print(\"x\");\n  }} catch (e) {{\n    {use_site}\n  }}"
        )));
        assert_eq!(diagnostic.code, RuleCode::S010, "{use_site}");
        assert_eq!(diagnostic.pos.line, 5, "{use_site}");
    }
}

#[test]
fn narrowing_does_not_reach_a_lambda_or_the_code_after_the_test() {
    let lambda = first_error(&in_main(
        "  try {\n    print(\"x\");\n  } catch (e) {\n\
         \x20   if (e instanceof Error) {\n\
         \x20     const show = (): void => { print(e.message); };\n\
         \x20     show();\n    }\n  }",
    ));
    assert_eq!(lambda.code, RuleCode::S010);

    let after = first_error(&in_main(
        "  try {\n    print(\"x\");\n  } catch (e) {\n\
         \x20   if (e instanceof Error) { print(\"narrowed\"); }\n\
         \x20   print(e.name);\n  }",
    ));
    assert_eq!(after.code, RuleCode::S010);
    assert_eq!(after.pos.line, 6);
}

#[test]
fn catch_annotations_and_patterns_follow_the_decided_surface() {
    check(&in_main(
        "  try {\n    print(\"x\");\n  } catch (e: unknown) {\n  }",
    ))
    .expect("`unknown` is the one accepted annotation");
    for binding in ["e: any", "e: Error", "{ message }"] {
        let diagnostic = first_error(&in_main(&format!(
            "  try {{\n    print(\"x\");\n  }} catch ({binding}) {{\n  }}"
        )));
        assert_eq!(diagnostic.code, RuleCode::S010, "{binding}");
        assert_eq!(diagnostic.pos.line, 4, "{binding}");
    }
}

#[test]
fn finally_is_rejected_and_a_try_block_can_hold_a_suspension() {
    let finally = first_error(&in_main(
        "  try {\n    print(\"x\");\n  } finally {\n    print(\"y\");\n  }",
    ));
    assert_eq!(finally.code, RuleCode::S010);
    assert!(finally.message.contains("`finally`"), "{}", finally.message);

    // compiler.md §116.1 rule 7: a `try` block can hold `await` and
    // `yield`.
    check(
        "async function step(): Promise<i32> { return 1; }\n\
         export async function main(): Promise<void> {\n\
         \x20 try {\n    const value: i32 = await step();\n    print(`${value}`);\n  } catch {\n  }\n}\n",
    )
    .expect("a try block can hold an await of a direct call");
    check(
        "async function step(): Promise<i32> { return 1; }\n\
         export async function main(): Promise<void> {\n\
         \x20 const held: Promise<i32> = step();\n\
         \x20 try {\n    await Context.suspend();\n    print(`${await held}`);\n  } catch {\n  }\n}\n",
    )
    .expect("a try block can hold an explicit suspension and a held await");
    check(
        "function* numbers(): Generator<i32> {\n\
         \x20 try {\n    yield 1;\n  } catch {\n  }\n}\n\
         export function main(): void {\n\
         \x20 const values: Generator<i32> = numbers();\n  values.next();\n}\n",
    )
    .expect("a try block can hold a yield");
    check(
        "export async function main(): Promise<void> {\n\
         \x20 try {\n    print(\"x\");\n  } catch {\n    await Context.suspend();\n  }\n}\n",
    )
    .expect("a catch block can hold a suspension");
}

#[test]
fn instanceof_accepts_only_the_error_family() {
    check(&in_main(
        "  const value: Error = new TypeError(\"x\");\n\
         \x20 if (value instanceof TypeError) { print(value.message); }",
    ))
    .expect("a value of an Error-family type");
    let class = first_error(
        "class Box { value: i32 = 1; }\n\
         export function main(): void {\n\
         \x20 const box: Box = new Box();\n\
         \x20 print(`${box instanceof Box}`);\n}\n",
    );
    assert_eq!(class.code, RuleCode::S100);
    let operand = first_error(&in_main(
        "  const n: i32 = 1;\n  print(`${n instanceof Error}`);",
    ));
    assert_eq!(operand.code, RuleCode::S100);
}

#[test]
fn a_thrown_allocation_escapes_its_loop_iteration() {
    let module = check(&in_main(
        "  for (let i: i32 = 0; i < 3; i++) {\n\
         \x20   try {\n\
         \x20     if (i === 1) { throw new Error(`${i}`); }\n\
         \x20     const held: Error = new Error(\"held\");\n\
         \x20     if (i === 2) { throw held; }\n\
         \x20   } catch {\n\
         \x20   }\n\
         \x20 }",
    ))
    .expect("checks clean");
    assert!(
        check_warnings(&module).is_empty(),
        "a thrown object leaves the iteration: {:?}",
        check_warnings(&module)
    );
    let positive = check(&in_main(
        "  for (let i: i32 = 0; i < 3; i++) {\n\
         \x20   const held: Error = new Error(\"held\");\n\
         \x20   print(held.message);\n\
         \x20 }",
    ))
    .expect("checks clean");
    assert_eq!(check_warnings(&positive).len(), 1, "the control warns");
}

/// §115.1 rule 6: the Error classes are not JSON types, at any depth.
#[test]
fn json_rejects_an_error_at_any_depth() {
    let declarations = "class Report {\n\
                        \x20 code: i32 = 0;\n\
                        \x20 failure: Error = new Error(\"x\");\n\
                        }\n\
                        class Outer {\n\
                        \x20 reports: Report[] = [];\n\
                        \x20 last: Report | null = null;\n\
                        }\n\
                        class Plain {\n\
                        \x20 code: i32 = 0;\n\
                        }\n";
    for (body, line, text) in [
        (
            "  print(JSON.stringify(new Report()));",
            13,
            "`JSON.stringify` cannot accept `Report`: it holds an Error",
        ),
        (
            "  print(JSON.stringify(new Outer()));",
            13,
            "`JSON.stringify` cannot accept `Outer`: it holds an Error",
        ),
        (
            "  const errors: Error[] = [new TypeError(\"t\")];\n  print(JSON.stringify(errors));",
            14,
            "`JSON.stringify` cannot accept `Error[]`: it holds an Error",
        ),
        (
            "  const parsed: Error = JSON.parse<Error>(\"{}\");",
            13,
            "`JSON.parse` cannot accept `Error`: the Error classes are not JSON types",
        ),
        (
            "  const parsed: Outer = JSON.parse<Outer>(\"{}\");",
            13,
            "`JSON.parse` cannot accept `Outer`: it holds an Error",
        ),
    ] {
        let source = format!("{declarations}export function main(): void {{\n{body}\n}}\n");
        let diagnostic = first_error(&source);
        assert_eq!(diagnostic.code, RuleCode::S014, "{body}");
        assert_eq!(diagnostic.pos.line, line, "{body}");
        assert!(
            diagnostic.message.contains(text),
            "{body}: {}",
            diagnostic.message
        );
    }
    // The firing control: the same shape without an Error checks clean.
    let clean = format!(
        "{declarations}export function main(): void {{\n\
         \x20 print(JSON.stringify(new Plain()));\n\
         \x20 const parsed: Plain = JSON.parse<Plain>(\"{{\\\"code\\\":1}}\");\n\
         }}\n"
    );
    check(&clean).expect("a class without an Error field is a JSON type");
}

/// §115.7 rules 1 and 5 and §115.6 rule 3: `JSON.parse` returns `T`, is a
/// raise site of its caller, and names the target as the source spells it.
#[test]
fn json_parse_returns_its_target_and_raises_in_its_caller() {
    let module = check(
        "class Config {\n\
         \x20 count: i32 = 0;\n\
         }\n\
         function contextual(text: string): Config {\n\
         \x20 return JSON.parse(text);\n\
         }\n\
         function explicit(text: string): FixedArray<i16,  3> {\n\
         \x20 return JSON.parse<FixedArray<i16,  3>>(text);\n\
         }\n\
         function quiet(): i32 {\n\
         \x20 return 1;\n\
         }\n\
         export function main(): void {\n\
         \x20 print(`${contextual(\"{}\").count} ${explicit(\"[]\")[0]} ${quiet()}`);\n\
         }\n",
    )
    .expect("a direct JSON.parse checks");
    let function = |name: &str| {
        module
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("function")
    };
    assert!(function("contextual").can_raise);
    assert!(function("explicit").can_raise);
    // The firing control: the same shape without a parse cannot raise.
    assert!(!function("quiet").can_raise);
    let messages: Vec<String> = module
        .functions
        .iter()
        .filter(|function| function.name.ends_with(".root]]"))
        .map(|function| format!("{:?}", function.body))
        .collect();
    assert_eq!(messages.len(), 2);
    assert!(messages
        .iter()
        .any(|body| body.contains("JSON.parse: document does not match Config")));
    assert!(messages
        .iter()
        .any(|body| body.contains("JSON.parse: document does not match FixedArray<i16,  3>")));
}
