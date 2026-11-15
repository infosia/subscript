//! A context-free list of functions takes the type that each fits
//! (compiler.md §248).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process.

use subscript_compiler::{check_program, hir, Diagnostic, RuleCode, SourceFile, Type};

const DECLARATIONS: &str = r#"
function none(): void {}
function one(x: i32): void {}
function nv(x: i32): i32 { return x; }
function word(s: string): void {}
async function work(x: i32): Promise<void> {}
const LV = (): void => {};
"#;

fn check(body: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    let source = format!("{DECLARATIONS}{body}\nexport function main(): void {{}}\n");
    check_program(&[SourceFile::new("test.ts", source.as_str())])
}

fn accepts(body: &str) -> hir::Module {
    check(body).unwrap_or_else(|errors| panic!("{body}: {errors:?}"))
}

/// The one diagnostic of a rejected body.
fn rejects(body: &str) -> Diagnostic {
    let errors = check(body).expect_err(body);
    assert_eq!(errors.len(), 1, "{body}: {errors:?}");
    errors.into_iter().next().expect("one diagnostic")
}

/// The initializer of the `let` named `name` in `f`.
fn initializer<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Expr {
    let f = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("f");
    f.body
        .iter()
        .find_map(|statement| match statement {
            hir::Stmt::Let {
                name: local, init, ..
            } if local == name => Some(init),
            _ => None,
        })
        .expect("the binding")
}

fn i32_to_void() -> Type {
    Type::func(vec![Type::I32], Type::Void)
}

/// Rules 1 and 3: an array literal takes the type of the first element
/// that each other element fits, in any order, and each other element
/// takes its adapter. The control: a list that each element fits keeps the
/// type of its first element.
#[test]
fn an_array_literal_takes_the_type_that_each_element_fits() {
    for body in [
        "function f(): void { const fs = [none, one]; }",
        "function f(): void { const fs = [one, none]; }",
        "function f(): void { const fs = [LV, nv, one]; }",
        "function f(): void { const fs = [none, (x: i32): void => {}]; }",
    ] {
        let module = accepts(body);
        let init = initializer(&module, "fs");
        assert_eq!(init.ty, Type::array(i32_to_void()), "{body}");
        let hir::ExprKind::ArrayLit(elements) = &init.kind else {
            panic!("{init:?}");
        };
        for element in elements {
            assert_eq!(element.ty, i32_to_void(), "{body}: {element:?}");
        }
    }
    let module = accepts("function f(): void { const fs = [none, none]; }");
    assert_eq!(
        initializer(&module, "fs").ty,
        Type::array(Type::func(Vec::new(), Type::Void))
    );
}

/// Rules 2 and 3: a conditional takes the type of the first branch that
/// the other branch fits, and the other branch takes its adapter.
#[test]
fn a_conditional_takes_the_type_that_the_other_branch_fits() {
    for body in [
        "function f(b: boolean): void { const g = b ? none : one; }",
        "function f(b: boolean): void { const g = b ? one : none; }",
        "function f(b: boolean): void { const g = b ? one : nv; }",
        "function f(b: boolean): void { const g = b ? nv : one; }",
    ] {
        let module = accepts(body);
        let init = initializer(&module, "g");
        assert_eq!(init.ty, i32_to_void(), "{body}");
        let hir::ExprKind::Cond { then, els, .. } = &init.kind else {
            panic!("{init:?}");
        };
        assert_eq!(then.ty, i32_to_void(), "{body}");
        assert_eq!(els.ty, i32_to_void(), "{body}");
    }
}

/// Rules 1, 2, and 4: when no value has a type that each other value fits,
/// the present diagnostic applies. `[one, word]` stays rejected: `tsc`
/// gives it a union element type, and a union of function types has no
/// form here. A handle result follows §247, and a value that is not a
/// function keeps its rule. The firing controls: the accepted forms above.
#[test]
fn a_list_with_no_fitting_type_keeps_its_diagnostic() {
    for (body, code, message) in [
        (
            "function f(): void { const fs = [one, word]; }",
            RuleCode::S100,
            "type mismatch: the array element expects `(i32) => void`, got `(string) => void`",
        ),
        (
            "function f(b: boolean): void { const g = b ? one : word; }",
            RuleCode::S100,
            "conditional branches have no common type: `(i32) => void` and `(string) => void`",
        ),
        (
            "function f(): void { const fs = [one, work]; }",
            RuleCode::S013,
            "function value `work` returns `Promise<void>`, which the place `(i32) => void` drops, and no holder awaits the handle; a place with the result `Promise<void>` can await it",
        ),
        (
            "function f(): void { const fs = [none, work]; }",
            RuleCode::S100,
            "type mismatch: the array element expects `() => void`, got `(i32) => Promise<void>`",
        ),
        (
            "function f(b: boolean): void { const g = b ? one : work; }",
            RuleCode::S100,
            "conditional branches have no common type: `(i32) => void` and `(i32) => Promise<void>`",
        ),
        (
            "function f(b: boolean): void { const g = b ? nv : none; }",
            RuleCode::S100,
            "conditional branches have no common type: `(i32) => i32` and `() => void`",
        ),
        (
            "function f(): void { const fs = [none, 1]; }",
            RuleCode::S100,
            "type mismatch: the array element expects `() => void`, got `i32`",
        ),
    ] {
        let diagnostic = rejects(body);
        assert_eq!(diagnostic.code, code, "{body}: {diagnostic:?}");
        assert_eq!(diagnostic.message, message, "{body}");
    }
}

/// §305 rules 1 and 3: conditional candidates select a list type and place
/// each branch. Measured cost: 30 ms for seven checker runs, with no code generation.
#[test]
fn conditional_elements_take_part_in_function_list_selection() {
    let start = std::time::Instant::now();
    for list in [
        "[none, b ? one : nv]",
        "[none, b ? one : one]",
        "[b ? none : none, one]",
        "[none, (b ? nv : one)]",
        "[none, b ? (b ? one : nv) : one]",
        "[none, b ? (x: i32): void => {} : nv]",
    ] {
        let body = format!("function f(b: boolean): void {{ const fs = {list}; }}");
        let module = accepts(&body);
        let init = initializer(&module, "fs");
        assert_eq!(init.ty, Type::array(i32_to_void()), "{list}");
        let hir::ExprKind::ArrayLit(elements) = &init.kind else {
            panic!("{init:?}");
        };
        for element in elements {
            assert_eq!(element.ty, i32_to_void(), "{list}: {element:?}");
        }
    }
    // A conditional with no common function type keeps its branch diagnostic.
    let error = rejects("function f(b: boolean): void { const fs = [one, b ? one : word]; }");
    assert_eq!(error.code, RuleCode::S100);
    assert_eq!(
        error.message,
        "type mismatch: the else branch expects `(i32) => void`, got `(string) => void`"
    );
    eprintln!("conditional list candidates: {:?}", start.elapsed());
}

/// §305 rules 2 and 3: spread candidates select the type in either order.
/// Measured cost: 30 ms for eight checker runs, with no code generation.
#[test]
fn spread_elements_take_part_in_function_list_selection() {
    let start = std::time::Instant::now();
    for (locals, list) in [
        ("const base = [one];", "[none, ...base]"),
        ("const base = [none];", "[...base, one]"),
        ("const base = [nv];", "[one, ...base]"),
        (
            "const base = [none]; const more = [one];",
            "[...base, ...more]",
        ),
        ("const base = [none];", "[...base, b ? one : nv]"),
        ("", "[none, ...[one]]"),
    ] {
        let body = format!("function f(b: boolean): void {{ {locals} const fs = {list}; }}");
        let module = accepts(&body);
        let init = initializer(&module, "fs");
        assert_eq!(init.ty, Type::array(i32_to_void()), "{list}");
        let hir::ExprKind::ArraySpreadLit(elements) = &init.kind else {
            panic!("{init:?}");
        };
        for element in elements {
            let expected = if element.spread.is_some() {
                Type::array(i32_to_void())
            } else {
                i32_to_void()
            };
            assert_eq!(element.expr.ty, expected, "{list}: {element:?}");
        }
    }
    for body in [
        "function f(): void { const base = [word]; const fs = [one, ...base]; }",
        "function f(): void { const base = [one]; const fs = [...base, word]; }",
    ] {
        let error = rejects(body);
        assert_eq!(error.code, RuleCode::S100);
        assert_eq!(
            error.message,
            "type mismatch: the array element expects `(i32) => void`, got `(string) => void`"
        );
    }
    eprintln!("spread list candidates: {:?}", start.elapsed());
}

/// §305 rules 4 and 5: noncandidates keep diagnostics, and accepted lists
/// keep their type. Measured cost: 26 ms for six checker runs, with no code generation.
#[test]
fn noncandidate_elements_keep_their_rules() {
    let start = std::time::Instant::now();
    for body in [
        "function f(): void { const base = [1]; const fs = [one, ...base]; }",
        "function f(): void { const base: FixedArray<() => void, 1> = [none]; const fs = [...base, one]; }",
    ] {
        let error = rejects(body);
        assert_eq!(error.code, RuleCode::S100);
        assert!(error.message.starts_with("type mismatch: the array element expects"));
    }
    let errors = check("function f(b: boolean): void { const fs = [none, b ? one : word]; }")
        .expect_err("neither branch fits the hint");
    assert_eq!(errors.len(), 2);
    assert!(errors[0]
        .message
        .contains("the then branch expects `() => void`"));
    assert!(errors[1]
        .message
        .contains("the else branch expects `() => void`"));
    for body in [
        "function f(b: boolean): void { const fs = [one, b ? none : nv]; }",
        "function f(): void { const base = [one]; const fs = [...base, one]; }",
        "function f(): void { const base = [one]; const fs = [...base, none]; }",
    ] {
        let module = accepts(body);
        assert_eq!(initializer(&module, "fs").ty, Type::array(i32_to_void()));
    }
    eprintln!("noncandidate list controls: {:?}", start.elapsed());
}

/// An outer array hint still places the leaves of an inner conditional.
#[test]
fn nested_arrays_place_conditional_function_leaves() {
    for (list, index) in [
        ("[[nvoid], [nvoid, b ? nv : nv2]]", 1),
        ("[[nvoid], [b ? nv : nv2]]", 0),
        ("[[nvoid], [...[nvoid], b ? nv : nv2]]", 1),
        ("[[nvoid], [b ? nv : nv2, ...[nvoid]]]", 0),
    ] {
        let source = format!(
            r#"
function nv(x: i32): i32 {{ print(`nv ${{x}}`); return x; }}
function nv2(x: i32): i32 {{ return x + 1; }}
function nvoid(x: i32): void {{}}
export function main(): void {{ const b = true; const a = {list}; a[1][{index}](4); }}
"#
        );
        check_program(&[SourceFile::new("nested.ts", &source)])
            .unwrap_or_else(|errors| panic!("{list}: {errors:?}"));
    }
}

/// A failed list selection keeps the diagnostic at each conditional leaf.
#[test]
fn conditional_call_leaves_keep_the_adapter_name_diagnostic() {
    for (element, count) in [("b ? make(4) : make(5)", 2), ("b ? nv : make(4)", 1)] {
        let source = format!(
            "function none(x: i32): void {{}}
             function nv(x: i32): i32 {{ return x; }}
             function make(k: i32): (x: i32) => i32 {{ const saved = k; return (x: i32): i32 => x + saved; }}
             export function main(): void {{ const b = true; const fs = [none, {element}]; }}"
        );
        let errors = check_program(&[SourceFile::new("adapter.ts", &source)])
            .expect_err("a call cannot take an adapter");
        assert_eq!(errors.len(), count, "{element}: {errors:?}");
        for error in errors {
            assert_eq!(error.code, RuleCode::S100, "{error:?}");
            assert_eq!(error.message, "function value needs an adapter for this place, and only a const local or a module `const` takes one; copy it into a const local, or write a lambda that calls it");
        }
    }
}
