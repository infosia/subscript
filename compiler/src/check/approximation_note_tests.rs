//! §308: each rejection names checked, accepted forms.
//! Measured checker and render cost: 16 tests in 0.14 seconds, with one test thread.
use super::rejection_total;
use crate::{check_program, render_diagnostics, SourceFile};

const LOOP_NOTE: &str = "The narrowing before the loop does not hold in the loop, because the loop stores this location. Copy the value into a `const` local before the loop. Or test the value for null again before this use.";
const DECLARATION_NOTE: &str = "Move the declaration before the code that uses it.";

fn pair(site: &str, code: &str, message: &str, note: &str, rejected: &str, accepted: &[&str]) {
    assert!(!accepted.is_empty());
    if site.contains("LoopHead") {
        assert_eq!(accepted.len(), 2, "copy and null test both need witnesses");
    }
    let start = std::time::Instant::now();
    let files = [SourceFile::entry("main.ts", rejected)];
    rejection_total::clear_reached();
    let diagnostics = check_program(&files).expect_err("rejected form");
    let reached = rejection_total::take_reached();
    eprintln!("{site}:\n{}", render_diagnostics(&files, &diagnostics));
    for source in accepted {
        check_program(&[SourceFile::entry("main.ts", *source)])
            .expect("each named form is accepted");
    }
    eprintln!("{site} measured pair cost: {:?}", start.elapsed());
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let d = &diagnostics[0];
    assert_eq!(d.code.as_str(), code);
    assert_eq!(d.message, message);
    assert_eq!(d.note, Some(note));
    assert!(
        reached
            .iter()
            .any(|(actual, msg, pos)| format!("{actual:?}") == site
                && msg == message
                && pos == &d.pos),
        "{reached:?}"
    );
    if site.contains("LoopHead") {
        assert_eq!(d.divergence, None);
        assert_eq!(d.example, None);
    }
}

fn loop_shapes(site: &str, code: &str, message: &str, prefix: &str, value_type: &str, read: &str) {
    let start = std::time::Instant::now();
    for body in [
        "for (let i: i32 = 0; i < 2; i++) { READ a = next; }",
        "do { READ a = next; } while (false);",
        "for (const i of [1, 2]) { READ a = next; }",
        "while (true) { while (true) { READ a = next; break; } break; }",
        "while ((a = next) !== null) { READ break; }",
        "for (; true; a = next) { READ }",
        // §304 rule 1b removes `a` after the dependent source `b` ends.
        "while (true) { READ a = b; b = next; break; }",
    ] {
        let make = |body: String| {
            format!("{prefix} function f(a: {value_type} | null, next: {value_type} | null): void {{ let b = a; if (a !== null) {{ if (b !== null) {{ {body} }} }} }}")
        };
        let rejected = make(body.replace("READ", read));
        let accepted = make(format!(
            "const saved = a; {}",
            body.replace("READ", &read.replace('a', "saved"))
        ));
        // A condition that checks the newly stored value establishes a fresh
        // fact, so the body is accepted even though the entry check ended.
        if body.starts_with("while ((a = next)") {
            check_program(&[SourceFile::entry("main.ts", &rejected)])
                .expect("loop condition checks again");
        } else {
            pair(
                site,
                code,
                message,
                LOOP_NOTE,
                &rejected,
                &[
                    &accepted,
                    &make(body.replace("READ", &format!("if (a !== null) {{ {read} }}"))),
                ],
            );
        }
        check_program(&[SourceFile::entry("main.ts", &accepted)])
            .expect("const copy across loop forms");
    }
    let rechecked = format!("{prefix} function f(a: {value_type} | null, next: {value_type} | null): void {{ if (a !== null) {{ while (true) {{ a = next; if (a !== null) {{ {read} }} break; }} }} }}");
    check_program(&[SourceFile::entry("main.ts", rechecked)]).expect("check again in the loop");
    let shared = format!("{prefix} class Holder {{ value: {value_type} | null = null; }} function touch(): void {{}} function f(h: Holder): void {{ if (h.value !== null) {{ while (true) {{ {} touch(); break; }} }} }}", read.replace('a', "h.value"));
    let diagnostics =
        check_program(&[SourceFile::entry("main.ts", shared)]).expect_err("C17 shared effect");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].divergence,
        Some(crate::divergence::Divergence::SharedLocationNarrowing)
    );
    assert_eq!(diagnostics[0].note, None);
    eprintln!("{site} loop controls cost: {:?}", start.elapsed());
}

#[test]
fn s308_nullable_member_loop_head() {
    pair("NullableMemberLoopHead", "S011", "`A | null` may be null here; narrow with a null check first", LOOP_NOTE,
        "class A { x: i32 = 1; } class H { f: A | null = null; } function f(h: H): void { if (h.f !== null) { while (true) { h.f.x; h.f = null; break; } } }",
        &["class A { x: i32 = 1; } class H { f: A | null = null; } function f(h: H): void { if (h.f !== null) { const a = h.f; while (true) { a.x; h.f = null; break; } } }", "class A { x: i32 = 1; } class H { f: A | null = null; } function f(h: H): void { if (h.f !== null) { while (true) { if (h.f !== null) { h.f.x; } h.f = null; break; } } }"]);
    pair("NullableMemberLoopHead", "S011", "`A | null` may be null here; narrow with a null check first", LOOP_NOTE,
        "class A { x: i32 = 1; } function f(next: A | null): void { let a: A | null = new A(); while (true) { a.x; a = next; } }",
        &["class A { x: i32 = 1; } function f(next: A | null): void { let a: A | null = new A(); const saved = a; while (true) { saved.x; a = next; } }",
          "class A { x: i32 = 1; } function f(next: A | null): void { let a: A | null = new A(); while (true) { if (a !== null) { a.x; } a = next; } }"]);
    loop_shapes(
        "NullableMemberLoopHead",
        "S011",
        "`A | null` may be null here; narrow with a null check first",
        "class A { x: i32 = 1; }",
        "A",
        "a.x;",
    );
    let start = std::time::Instant::now();
    for body in [
        "while (true) { a.x; a = null; break; }",
        "if (a !== null) { while (true) { a = null; break; } a.x; }",
        "if (a !== null) { while (true) { { let a: A | null = null; a.x; } a = null; break; } }",
        "if (a !== null) { while (true) { const g = (): i32 => a.x; a = null; break; } }",
    ] {
        let source =
            format!("class A {{ x: i32 = 1; }} function f(a: A | null): void {{ {body} }}");
        let ds = check_program(&[SourceFile::entry("main.ts", source)])
            .expect_err("control is rejected");
        assert_eq!(
            ds.iter().filter(|d| d.code.as_str() == "S011").count(),
            1,
            "{ds:?}"
        );
        assert!(
            ds.iter().all(|d| d.note.is_none()),
            "no loop-entry fact of this binding: {ds:?}"
        );
    }
    eprintln!("member provenance controls cost: {:?}", start.elapsed());
}

#[test]
fn s308_nullable_call_loop_head() {
    pair("NullableCallLoopHead", "S100", "type `((i32) => i32) | null` is not callable", LOOP_NOTE,
        "function f(cb: ((x: i32) => i32) | null): void { if (cb !== null) { while (true) { cb(1); cb = null; break; } } }",
        &["function f(cb: ((x: i32) => i32) | null): void { if (cb !== null) { const c = cb; while (true) { c(1); cb = null; break; } } }", "function f(cb: ((x: i32) => i32) | null): void { if (cb !== null) { while (true) { if (cb !== null) { cb(1); } cb = null; break; } } }"]);
    loop_shapes(
        "NullableCallLoopHead",
        "S100",
        "type `((i32) => i32) | null` is not callable",
        "",
        "((x: i32) => i32)",
        "a(1);",
    );
}

#[test]
fn s308_nullable_nominal_assignment_loop_head() {
    pair("NullableNominalAssignmentLoopHead", "S005", "nominal types are not interchangeable: the argument expects `A`, got `A | null`", LOOP_NOTE,
        "class A { x: i32 = 1; } function use(a: A): void {} function f(a: A | null): void { if (a !== null) { while (true) { use(a); a = null; break; } } }",
        &["class A { x: i32 = 1; } function use(a: A): void {} function f(a: A | null): void { if (a !== null) { const b = a; while (true) { use(b); a = null; break; } } }", "class A { x: i32 = 1; } function use(a: A): void {} function f(a: A | null): void { if (a !== null) { while (true) { if (a !== null) { use(a); } a = null; break; } } }"]);
    loop_shapes(
        "NullableNominalAssignmentLoopHead",
        "S005",
        "nominal types are not interchangeable: the argument expects `A`, got `A | null`",
        "class A { x: i32 = 1; } function use(a: A): void {}",
        "A",
        "use(a);",
    );
}

#[test]
fn s308_method_value_truth_test() {
    pair("MethodValueTruthTest", "S100", "method `ready` may only be called, not read as a value", "A method is never null. Call it without the test.",
        "class C { ready(): boolean { return true; } } function f(c: C): void { if (c.ready) { c.ready(); } }",
        &["class C { ready(): boolean { return true; } } function f(c: C): void { c.ready(); }"]);
}

#[test]
fn s308_incompatible_nominal_assignment() {
    pair("IncompatibleNominalAssignment", "S005", "nominal types are not interchangeable: the initializer expects `P<i32>`, got `Q<i32>`", "Classes are nominal. Make an object of the declared class.",
        "class P<T> { constructor(public x: T) {} } class Q<T> { constructor(public x: T) {} } function f(): void { const p: P<i32> = new Q<i32>(1); }",
        &["class P<T> { constructor(public x: T) {} } class Q<T> { constructor(public x: T) {} } function f(): void { const p: P<i32> = new P<i32>(1); }"]);
}

#[test]
fn s308_disjoint_literal_alias_assignment() {
    pair("DisjointLiteralAliasAssignment", "S100", "type mismatch: the initializer expects `B`, got `A`", "Assign a literal member of the target alias.",
        "type A = \"a\" | \"b\"; type B = \"a\" | \"c\"; function f(a: A): void { if (a === \"a\") { const b: B = a; } }",
        &["type A = \"a\" | \"b\"; type B = \"a\" | \"c\"; function f(a: A): void { if (a === \"a\") { const b: B = \"a\"; } }"]);
}

#[test]
fn s308_class_read_before_declaration() {
    pair(
        "ClassReadBeforeDeclaration",
        "S100",
        "`B` is used before its declaration",
        DECLARATION_NOTE,
        "const x: i32 = B.z; class B { static z: i32 = 1; }",
        &["class B { static z: i32 = 1; } const x: i32 = B.z;"],
    );
}

#[test]
fn s308_immediate_pending_local_read() {
    pair(
        "ImmediatePendingLocalRead",
        "S100",
        "`x` is read before its declaration in this block",
        DECLARATION_NOTE,
        "function f(): void { print(`${x}`); const x: i32 = 1; }",
        &["function f(): void { const x: i32 = 1; print(`${x}`); }"],
    );
}

#[test]
fn s308_immediate_shadowed_name_read() {
    pair(
        "ImmediateShadowedNameRead",
        "S100",
        "`Math` is read before its declaration in this block",
        "A later local declaration hides the outer name. Give the local another name.",
        "function f(): void { const x = Math.abs(-2.5); const Math: i32 = 3; }",
        &["function f(): void { const x = Math.abs(-2.5); const localMath: i32 = 3; }"],
    );
}

#[test]
fn s308_immediate_name_write_before_declaration() {
    pair(
        "ImmediateNameWriteBeforeDeclaration",
        "S100",
        "`x` is assigned before its declaration in this block",
        DECLARATION_NOTE,
        "function f(): void { x = 2; let x: i32 = 1; }",
        &["function f(): void { let x: i32 = 1; x = 2; }"],
    );
}

// Each control checks diagnostic provenance, not acceptance. Measure with --nocapture.
fn ordinary_loop_read(body: &str, read: &str, prefix: &str, ty: &str, site: &str) {
    let source = format!("{prefix} function f(a: {ty} | null, next: {ty} | null): void {{ if (a !== null) {{ while (true) {{ {} }} }} }}", body.replace("READ", read));
    let files = [SourceFile::entry("main.ts", source)];
    rejection_total::clear_reached();
    let diagnostics = check_program(&files).expect_err("ordinary nullable rejection");
    let reached = rejection_total::take_reached();
    eprintln!("{}", render_diagnostics(&files, &diagnostics));
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].note, None);
    assert!(
        reached
            .iter()
            .any(|(s, _, pos)| format!("{s:?}") == site && pos == &diagnostics[0].pos),
        "{reached:?}"
    );
}

#[test]
fn s308_loop_note_ends_at_body_store() {
    let start = std::time::Instant::now();
    for (prefix, ty, read, site) in [
        ("class A { x: i32 = 1; }", "A", "a.x;", "NullableMember"),
        ("", "((x: i32) => i32)", "a(1);", "NullableCall"),
        (
            "class A { x: i32 = 1; } function use(a: A): void {}",
            "A",
            "use(a);",
            "NullableNominalAssignment",
        ),
    ] {
        ordinary_loop_read("a = next; READ break;", read, prefix, ty, site);
    }
    eprintln!("body store controls cost: {:?}", start.elapsed());
}

#[test]
fn s308_loop_note_ends_at_body_test() {
    let start = std::time::Instant::now();
    for (prefix, ty, read, site) in [
        ("class A { x: i32 = 1; }", "A", "a.x;", "NullableMember"),
        ("", "((x: i32) => i32)", "a(1);", "NullableCall"),
        (
            "class A { x: i32 = 1; } function use(a: A): void {}",
            "A",
            "use(a);",
            "NullableNominalAssignment",
        ),
    ] {
        ordinary_loop_read(
            "if (a !== null) {} READ a = next; break;",
            read,
            prefix,
            ty,
            site,
        );
        ordinary_loop_read("a === null; READ a = next; break;", read, prefix, ty, site);
        ordinary_loop_read(
            "if (a === null) { READ } a = next; break;",
            read,
            prefix,
            ty,
            site,
        );
    }
    eprintln!("body test controls cost: {:?}", start.elapsed());
}

#[test]
fn s308_same_nominal_class_type_arguments() {
    let start = std::time::Instant::now();
    for (prefix, target, value, fixed) in [
        (
            "",
            "Map<string, string>",
            "new Map<i32, string>()",
            "new Map<string, string>()",
        ),
        ("", "Set<string>", "new Set<i32>()", "new Set<string>()"),
        (
            "class Box<T> { constructor(public x: T) {} }",
            "Box<string>",
            "new Box<i32>(1)",
            "new Box<string>(\"a\")",
        ),
    ] {
        pair("IncompatibleNominalAssignment", "S005", &format!("nominal types are not interchangeable: the initializer expects `{target}`, got `{}`", value.strip_prefix("new ").unwrap().split('(').next().unwrap()), "The type arguments differ. Use the type arguments of the declared type.",
            &format!("{prefix} function f(): void {{ const x: {target} = {value}; }}"),
            &[&format!("{prefix} function f(): void {{ const x: {target} = {fixed}; }}")]);
    }
    eprintln!("same class pairs cost: {:?}", start.elapsed());
}

#[test]
fn s308_shadowed_function_name() {
    pair("ImmediateShadowedNameRead", "S100", "`total` is read before its declaration in this block",
        "A later local declaration hides the outer name. Give the local another name.",
        "function total(): i32 { return 1; } function f(): void { total(); const total: i32 = 3; }",
        &["function total(): i32 { return 1; } function f(): void { total(); const localTotal: i32 = 3; }"]);
}

#[test]
fn s308_no_declaration_move_note_after_declaration() {
    let start = std::time::Instant::now();
    for source in [
        include_str!("../../../corpus/reject/r172-using-in-lambda.ts"),
        "function f(): void { const x: i32 = x; }",
        "function f(): void { const x: i32 = (x = 1); }",
    ] {
        let files = [SourceFile::entry("main.ts", source)];
        let diagnostics = check_program(&files).expect_err("existing rejection");
        eprintln!("{}", render_diagnostics(&files, &diagnostics));
        assert!(
            diagnostics.iter().all(|d| d.note.is_none()),
            "{diagnostics:?}"
        );
    }
    eprintln!("declaration order controls cost: {:?}", start.elapsed());
}

#[test]
fn s308_loop_notes_follow_branch_flow() {
    let start = std::time::Instant::now();
    for body in [
        "if (next === null) { a = next; } else { a.x; } a = null; break;",
        "next === null ? (a = next) : a.x; a = null; break;",
        "next === null && (a = next); a.x; a = null; break;",
    ] {
        let rejected = format!("class A {{ x: i32 = 1; }} function f(a: A | null, next: A | null): void {{ if (a !== null) {{ while (true) {{ {body} }} }} }}");
        let saved = rejected
            .replace("while (true)", "const saved = a; while (true)")
            .replace("a.x", "saved.x");
        let tested = rejected.replace("a.x", "(a !== null ? a.x : 0)");
        pair(
            "NullableMemberLoopHead",
            "S011",
            "`A | null` may be null here; narrow with a null check first",
            LOOP_NOTE,
            &rejected,
            &[&saved, &tested],
        );
    }
    eprintln!("branch provenance pairs cost: {:?}", start.elapsed());
}
