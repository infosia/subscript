//! Negation, loop exits, loop heads, and const captures (§162).
use subscript_compiler::{check_program, divergence::Divergence, RuleCode, SourceFile};

fn check(body: &str, accepted: bool) {
    let source = format!("class A {{ x: i32 = 1; f: A | null = null; }}\nfunction mk(): A | null {{ return new A(); }}\nfunction cond(): boolean {{ return true; }}\n{body}");
    let result = check_program(&[SourceFile::new("test.ts", source)]);
    if accepted {
        assert!(result.is_ok(), "{body}: {result:?}");
    } else {
        let errors = result.expect_err(body);
        assert!(!errors.is_empty());
        assert!(
            errors.iter().any(|d| d.code == RuleCode::S011),
            "{body}: {errors:?}"
        );
    }
}

#[test]
fn negation_exchanges_facts_at_each_depth() {
    for (condition, accepted) in [
        ("!(a === null)", true),
        ("!(a !== null)", false),
        ("!!(a !== null)", true),
        ("!!(a === null)", false),
        ("!(a === null || b === null)", true),
        ("!(a === null && b === null)", false),
    ] {
        check(&format!("function read(a: A | null, b: A | null): void {{ if ({condition}) {{ a.x; b.x; }} }}").replace("a.x; b.x;", if condition.contains('b') { "a.x; b.x;" } else { "a.x;" }), accepted);
    }
    check(
        "function read(a: A | null): i32 { if (!(a !== null)) { return 0; } return a.x; }",
        true,
    );
    check(
        "function read(a: A | null): i32 { if (!(a === null)) { return 0; } return a.x; }",
        false,
    );
    for (op, accepted) in [("===", true), ("!==", false)] {
        check(&format!("function read(): void {{ let a: A | null = null; if (!((a = mk()) {op} null)) {{ a.x; }} }}"), accepted);
    }
}

#[test]
fn loop_exits_intersect_condition_and_break_facts() {
    for (body, accepted) in [
        ("a = new A(); if (cond()) { break; }", true),
        ("if (cond()) { break; } a = new A();", false),
        ("while (true) { break; } a = new A(); break;", true),
        (
            "while (true) { a = new A(); break; } if (cond()) { a = null; break; } break;",
            false,
        ),
        ("if (a !== null) { break; } a = mk();", true),
        ("if (cond()) { break; } a = mk();", false),
    ] {
        check(
            &format!("function read(a: A | null): void {{ for (;;) {{ {body} }} a.x; }}"),
            accepted,
        );
    }
    for (condition, accepted) in [("a === null", true), ("cond()", false)] {
        for loop_kind in ["while", "for"] {
            let head = if loop_kind == "while" {
                format!("while ({condition})")
            } else {
                format!("for (; {condition};)")
            };
            check(
                &format!("function read(a: A | null): void {{ {head} {{ a = mk(); }} a.x; }}"),
                accepted,
            );
        }
    }
    check("function read(a: A | null): void { while (a === null) { if (cond()) { break; } a = mk(); } a.x; }", false);
    check(
        "function read(a: A | null): void { while (true) { break; a = new A(); break; } a.x; }",
        false,
    );
    check("function read(a: A | null): void { while (true) { const a: A | null = new A(); break; } a.x; }", false);
    check("function read(a: A | null): void { if (a === null) { return; } while (true) { const a: A | null = null; break; } a.x; }", true);
    check("function read(a: A | null): void { while (true) { switch (1) { case 1: break; } a = new A(); break; } a.x; }", true);
}

#[test]
fn whole_local_non_null_stores_preserve_loop_head_facts() {
    for (store, accepted) in [("new A()", true), ("mk()", false)] {
        check(&format!("function read(): void {{ let e: A | null = new A(); for (let i: i32 = 0; i < 2; i++) {{ e.x; e = {store}; }} }}"), accepted);
        check(&format!("function read(): void {{ let e: A | null = new A(); while (cond()) {{ e.x; while (cond()) {{ e = {store}; continue; }} continue; }} }}"), accepted);
        check(&format!("function read(): void {{ let e: A | null = new A(); for (; cond(); e = {store}) {{ e.x; continue; }} }}"), accepted);
    }
}

#[test]
fn recorded_missing_store_order_fact_stays_rejected() {
    for stores in ["e = mk(); e = new A();", "e = new A(); e = mk();"] {
        check(&format!("function read(): void {{ let e: A | null = new A(); while (cond()) {{ e.x; {stores} }} }}"), false);
    }
    check("function read(): void { let e = new A(); e.f = new A(); while (true) { e.f.x; e = new A(); break; } }", false);
    check(
        "function read(): void { let e = new A(); e.f = new A(); while (true) { e.f.x; break; } }",
        true,
    );
}

#[test]
fn any_same_name_field_store_ends_the_loop_head_fact() {
    for receiver in ["h", "alias"] {
        let source = format!("class A {{ x: i32 = 1; f: A | null = null; }} function read(): void {{ const h = new A(); const alias = h; h.f = new A(); while (true) {{ h.f.x; {receiver}.f = new A(); break; }} }}");
        let errors = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, RuleCode::S011);
        assert_eq!(
            errors[0].divergence,
            Some(Divergence::SharedLocationNarrowing)
        );
    }
    check("function read(): void { let h: A | null = new A(); while (true) { h.x; h = new A(); break; } }", true);
}

#[test]
fn lambdas_keep_only_whole_const_local_facts() {
    for (binding, accepted) in [("const", true), ("let", false)] {
        for body in ["(): i32 => a.x", "(): i32 => { return a.x; }"] {
            check(&format!("function read(): void {{ {binding} a = mk(); if (a === null) {{ return; }} const g = {body}; }}"), accepted);
        }
    }
    check(
        "function read(): void { const a: A | null = new A(); const g = (): i32 => a.x; }",
        true,
    );
    check(
        "function read(): void { const a = mk(); const g = (): i32 => a.x; }",
        false,
    );
    check(
        "function read(): void { const h = new A(); h.f = new A(); const g = (): i32 => h.f.x; }",
        false,
    );
    check("function read(): void { const h = new A(); const a = h.f; if (a === null) { return; } const g = (): i32 => a.x; }", true);
    check("function read(): void { const a = mk(); if (a === null) { return; } const g = (a: A | null): i32 => a.x; }", false);
}

#[test]
fn a_for_update_sees_every_continue_edge() {
    for (assignment, accepted) in [("a = new A();", true), ("", false)] {
        check(&format!("function read(a: A | null): void {{ for (; cond(); a.x) {{ {assignment} if (cond()) {{ continue; }} a = new A(); }} }}"), accepted);
    }
}

#[test]
fn catch_breaks_join_the_loop_exits() {
    for (assignment, accepted) in [("a = new A();", true), ("", false)] {
        check(&format!("function read(a: A | null): void {{ while (true) {{ try {{ if (cond()) {{ throw new Error(\"x\"); }} a = new A(); break; }} catch (e) {{ {assignment} break; }} }} a.x; }}"), accepted);
    }
}

#[test]
fn loop_exits_keep_the_shared_end_diagnostic() {
    let source = "class A { x: i32 = 1; f: A | null = null; } function touch(): void {} function read(): void { const h = new A(); while (true) { if (h.f === null) { return; } touch(); break; } h.f.x; }";
    let errors = check_program(&[SourceFile::new("test.ts", source)]).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, RuleCode::S011);
    assert_eq!(
        errors[0].divergence,
        Some(Divergence::SharedLocationNarrowing)
    );
    check("function read(): void { const h = new A(); while (true) { if (h.f === null) { return; } break; } h.f.x; }", true);
}

#[test]
fn for_of_exits_restore_the_outer_binding() {
    for (value, accepted) in [("new A()", true), ("mk()", false)] {
        check(&format!("function read(): void {{ let a: A | null = {value}; const xs: i32[] = [1]; for (const a of xs) {{ break; }} a.x; }}"), accepted);
    }
}

#[test]
fn loop_store_values_use_declared_types_without_facts() {
    for (value, accepted) in [
        ("b", false),
        ("new A()", true),
        ("(b)", false),
        ("cond() ? b : new A()", false),
        ("cond() ? new A() : new A()", true),
        ("(c = b)", false),
        ("(c = new A())", true),
    ] {
        for head in [
            "for (let i: i32 = 0; i < 3; i++)",
            "while (cond())",
            "for (const i of xs)",
        ] {
            check(&format!("function read(): void {{ let a: A | null = new A(); let b: A | null = new A(); let c: A | null = new A(); const xs: i32[] = [1, 2, 3]; {head} {{ a.x; a = {value}; b = null; }} }}"), accepted);
        }
        check(&format!("function read(): void {{ let a: A | null = new A(); let b: A | null = new A(); let c: A | null = new A(); while (cond()) {{ a.x; while (cond()) {{ a = {value}; b = null; continue; }} }} }}"), accepted);
    }
    for (value, accepted) in [("b", false), ("new A()", true)] {
        check(&format!("let b: A | null = new A(); function read(): void {{ if (b === null) {{ return; }} let a: A | null = new A(); for (let i: i32 = 0; i < 3; i++) {{ a.x; a = {value}; b = null; }} }}"), accepted);
    }
    check("function read(): void { const b: A = new A(); let a: A | null = new A(); while (cond()) { a.x; a = b; } }", true);
    check("const b = new A(); function read(): void { let a: A | null = new A(); while (cond()) { a.x; a = b; } }", true);
    check("function read(): void { const h = new A(); h.f = new A(); let a: A | null = new A(); while (cond()) { a.x; a = h.f; h.f = null; } }", false);
}

#[test]
fn loop_store_forms_require_annotation_provenance() {
    for (store, accepted) in [
        ("const c = b; a = c;", false),
        ("let c = b; a = c;", false),
        ("a = id(b);", false),
        ("a = [b][0];", false),
        ("const arr = [b]; a = arr[0];", false),
        ("for (const c of [b]) { a = c; }", false),
        ("const c: A = new A(); a = c;", true),
        ("a = p;", true),
        ("const c = new A(); a = c;", false),
    ] {
        check(&format!("function id<T>(v: T): T {{ return v; }} function read(p: A): void {{ let a: A | null = new A(); let b: A | null = new A(); for (let i: i32 = 0; i < 3; i++) {{ a.x; {store} b = null; }} }}"), accepted);
    }
}

#[test]
fn loop_store_parameters_require_written_non_generic_types() {
    for (parameter, elements, accepted) in [
        ("v", "b", false),
        ("v: A", "new A()", true),
        ("v = b", "b", false),
        ("v: A = new A()", "new A()", true),
    ] {
        check(&format!("function read(): void {{ let b: A | null = new A(); for (let i = 0; i < 2; i++) {{ const xs = [{elements}]; xs.forEach(({parameter}): void => {{ let a: A | null = new A(); for (let j = 0; j < 2; j++) {{ a.x; a = v; }} }}); b = null; }} }}"), accepted);
    }
    for (parameter, accepted) in [("p: T", false), ("p: A", true)] {
        for argument in ["A", "A | null"] {
            check(&format!("function g<T extends A | null>({parameter}, q: A): void {{ let a: A | null = q; for (let j = 0; j < 2; j++) {{ a.x; a = p; }} }} function read(): void {{ g<{argument}>(new A(), new A()); }}"), accepted);
        }
    }
}

#[test]
fn loop_field_store_uses_the_class_source_declaration() {
    for (field, ty, accepted) in [("v: T", "T", false), ("v: A", "A", true)] {
        check(&format!("class Box<T> {{ {field}; constructor(v: {ty}) {{ this.v = v; }} }} function read(): void {{ const box = new Box<A>(new A()); let a: A | null = new A(); for (let j = 0; j < 2; j++) {{ a.x; a = box.v; }} }}"), accepted);
    }
}
