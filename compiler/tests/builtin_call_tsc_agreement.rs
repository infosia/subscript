//! A built-in call is accepted only where `tsc` accepts it (compiler.md
//! §230): the result of a `reduce` callback (rule 1), and the type
//! argument list of a built-in call (rule 3).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process. The whole file took 0.06 s in a
//! debug build.

use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

const DECLARATIONS: &str = r#"type Mode = "a" | "b";
@ValueType
class V { x: f32 = 1; }
class M { value: i32 = 1; }
function echo(inbox: Inbox<M>, outbox: Outbox<M>): void {}
function id<T>(value: T): T { return value; }
class U {
  static make<T>(value: T): T { return value; }
  keep<T>(value: T): T { return value; }
}
"#;

fn check(body: &str) -> Result<(), Vec<Diagnostic>> {
    let source = format!(
        "{DECLARATIONS}function probe(xs: i32[], ms: Mode[], v: V, m: Map<i32, i32>, s: Set<i32>): void {{\n{body}\n}}\nexport function main(): void {{}}\n"
    );
    check_program(&[SourceFile::entry("test.ts", source.as_str())]).map(|_| ())
}

fn accepts(body: &str) {
    if let Err(errors) = check(body) {
        panic!("{body}: {errors:?}");
    }
}

fn rejects(body: &str, code: RuleCode, message: &str) {
    let errors = check(body).expect_err(body);
    assert_eq!(errors.len(), 1, "{body}: {errors:?}");
    assert_eq!(errors[0].code, code, "{body}: {errors:?}");
    assert_eq!(errors[0].message, message, "{body}");
}

fn widens(method: &str) -> String {
    format!(
        "the `{method}` callback result `\"b\"` has the type `string`, and `string` is not \
         assignable to the accumulator `Mode`; `tsc` rejects the call (TS2769)\nnote: write \
         `{method}<Mode>(...)`, or annotate the callback result: `(...): Mode => ...`"
    )
}

/// Rule 1: each form of the first table, and the literal forms that `tsc`
/// widens the same way.
#[test]
fn a_literal_reduce_result_widens_under_the_element_type() {
    for (method, body) in [
        (
            "reduce",
            r#"const r: Mode = xs.reduce((acc: Mode, x: i32) => "b", "a");"#,
        ),
        (
            "reduceRight",
            r#"const r: Mode = xs.reduceRight((acc: Mode, x: i32) => "b", "a");"#,
        ),
        (
            "reduce",
            r#"const k: Mode = "a"; const r: Mode = xs.reduce((acc: Mode, x: i32) => "b", k);"#,
        ),
        (
            "reduce",
            r#"const r: Mode = xs.reduce(((acc: Mode, x: i32) => ("b")), "a");"#,
        ),
        (
            "reduce",
            r#"const r: Mode = xs.reduce((acc: Mode, x: i32) => `b`, "a");"#,
        ),
        (
            "reduce",
            r#"const r: Mode = xs.reduce((acc: Mode, x: i32) => x > 0 ? "b" : "b", "a");"#,
        ),
    ] {
        rejects(body, RuleCode::S100, &widens(method));
    }
}

/// Rules 1 and 2: each neighbour of the second table, and each call that
/// keeps its rule, stays accepted.
#[test]
fn the_neighbours_of_a_widened_result_stay_accepted() {
    for body in [
        r#"const r: Mode = xs.reduce<Mode>((acc: Mode, x: i32) => "b", "a");"#,
        r#"const r: Mode = xs.reduce((acc: Mode, x: i32): Mode => "b", "a");"#,
        r#"const r: Mode = xs.reduce((acc: Mode, x: i32) => x > 0 ? "b" : acc, "a");"#,
        r#"const r: string = xs.reduce((acc: string, x: i32) => "b", "a");"#,
        "const r: i32 = xs.reduce((acc: i32, x: i32) => acc + x, 0);",
        // Two different literals give a union, and `tsc` keeps it.
        r#"const r: Mode = xs.reduce((acc: Mode, x: i32) => x > 0 ? "a" : "b", "a");"#,
        // The element type has a string literal type.
        r#"const r: Mode = ms.reduce((acc: Mode, x: Mode) => "b", "a");"#,
        r#"const r: Mode = xs.reduceRight<Mode>((acc: Mode, x: i32) => "b", "a");"#,
        r#"const r: Mode = xs.reduce((acc: Mode, x: i32): Mode => { return "b"; }, "a");"#,
        r#"const r: string | null = xs.reduce((acc: string | null, x: i32) => "b", null);"#,
    ] {
        accepts(body);
    }
    // Rule 2: a call with no initial value keeps its rule.
    let errors = check(r#"const r: Mode = ms.reduce((acc: Mode, x: Mode) => "b");"#)
        .expect_err("reduce(callback)");
    assert_eq!(errors[0].code, RuleCode::S014, "{errors:?}");
}

/// Rule 3: each built-in that reads a type argument accepts its count and
/// rejects another count. The message names the count.
#[test]
fn a_built_in_that_reads_a_type_argument_accepts_only_its_count() {
    for body in [
        "const r: i32[] = xs.map<i32>((x: i32) => x);",
        "const r: i32[] = xs.flatMap<i32>((x: i32) => [x]);",
        r#"const r: Mode = xs.reduce<Mode>((acc: Mode, x: i32) => "b", "a");"#,
        r#"const r: Mode = xs.reduceRight<Mode>((acc: Mode, x: i32) => "b", "a");"#,
        "const r: i32[] = Array.of<i32>(1);",
        "const r: i32[] = Array.from<i32>(xs);",
        r#"const r: i32 = JSON.parse<i32>("1");"#,
        "const r: u8[] = Context.bytesOf<V>(v);",
        "const r: V = Context.fromBytes<V>(Context.bytesOf<V>(v), 0);",
        "const t: u8[] = Context.bytesOf<V>(v); Context.bytesInto<V>(v, t, 0);",
        "const w: Worker<M, M> = Worker.spawn<M, M>(echo);",
        "const g: Map<i32, i32[]> = Map.groupBy<i32, i32>(xs, (x: i32) => x % 2);",
    ] {
        accepts(body);
    }
    for (body, code, message) in [
        (
            "const r: i32[] = xs.map<i32, i32>((x: i32) => x);",
            RuleCode::S100,
            "`map` takes 1 type argument, got 2",
        ),
        (
            "const r: i32[] = xs.flatMap<i32, i32>((x: i32) => [x]);",
            RuleCode::S100,
            "`flatMap` takes 1 type argument, got 2",
        ),
        (
            r#"const r: Mode = xs.reduce<Mode, i32>((acc: Mode, x: i32) => "b", "a");"#,
            RuleCode::S100,
            "`reduce` takes 1 type argument, got 2",
        ),
        (
            r#"const r: Mode = xs.reduceRight<Mode, i32>((acc: Mode, x: i32) => "b", "a");"#,
            RuleCode::S100,
            "`reduceRight` takes 1 type argument, got 2",
        ),
        (
            "const r: i32[] = Array.of<i32, i32>(1);",
            RuleCode::S100,
            "`Array.of<T>` takes exactly one type argument",
        ),
        (
            "const r: i32[] = Array.from<i32, i32>(xs);",
            RuleCode::S014,
            "`Array.from<T>` takes exactly one type argument",
        ),
        (
            r#"const r: i32 = JSON.parse<i32, i32>("1");"#,
            RuleCode::S014,
            "`JSON.parse<T>` takes exactly one type argument",
        ),
        (
            "const r: u8[] = Context.bytesOf<V, V>(v);",
            RuleCode::S014,
            "`Context.bytesOf<T>` takes exactly one type argument",
        ),
        (
            "const t: u8[] = Context.bytesOf<V>(v); Context.bytesInto<V, V>(v, t, 0);",
            RuleCode::S014,
            "`Context.bytesInto<T>` takes exactly one type argument",
        ),
        (
            "const r: V = Context.fromBytes<V, V>(Context.bytesOf<V>(v), 0);",
            RuleCode::S014,
            "`Context.fromBytes<T>` takes exactly one type argument",
        ),
        (
            "const g: Map<i32, i32[]> = Map.groupBy<i32>(xs, (x: i32) => x % 2);",
            RuleCode::S100,
            "`groupBy` takes 2 type arguments, got 1",
        ),
        (
            "const w: Worker<M, M> = Worker.spawn<M>(echo);",
            RuleCode::S100,
            "`Worker.spawn` takes exactly two explicit type arguments",
        ),
    ] {
        let errors = check(body).expect_err(body);
        assert_eq!(errors[0].code, code, "{body}: {errors:?}");
        assert_eq!(errors[0].message, message, "{body}");
    }
}

/// Rule 3: a built-in that reads no type argument rejects each list.
#[test]
fn a_built_in_that_reads_no_type_argument_rejects_a_list() {
    for (body, name) in [
        (
            "const r: i32[] = xs.filter<i32>((x: i32) => x > 0);",
            "filter",
        ),
        (r#"const r: string = "a".trim<i32>();"#, "trim"),
        ("const r: f64 = Math.abs<f64>(1.0);", "abs"),
        (r#"print<string>("a");"#, "print"),
        ("const r: string = JSON.stringify<i32>(1);", "stringify"),
        ("const r: boolean = m.has<i32>(1);", "has"),
    ] {
        rejects(
            body,
            RuleCode::S100,
            &format!("`{name}` takes no type arguments, got 1"),
        );
    }
}

/// Rule 1a: an annotated accumulator beside a parameter with no annotation
/// gives the arrow no rule 1 result, as `tsc` types its body after the
/// inference. Each form with no such pair stays rejected.
#[test]
fn a_context_sensitive_reduce_callback_keeps_its_rule() {
    for body in [
        r#"const r: Mode = xs.reduce((acc: Mode, x) => "b", "a");"#,
        r#"const k: Mode = "a"; const r: Mode = xs.reduce((acc: Mode, x) => "b", k);"#,
        r#"const r: Mode = xs.reduce((acc: Mode, x: i32, i) => "b", "a");"#,
        r#"const k: Mode = "a"; const r: Mode = xs.reduce((acc: Mode, x: i32, i) => "b", k);"#,
    ] {
        accepts(body);
    }
    for body in [
        r#"const r: Mode = xs.reduce((acc: Mode, x: i32) => "b", "a");"#,
        r#"const r: Mode = xs.reduce((acc: Mode) => "b", "a");"#,
    ] {
        rejects(body, RuleCode::S100, &widens("reduce"));
    }
    // With no annotated accumulator, `init` decides `string`, and the
    // result fails the context of the declaration.
    for body in [
        r#"const r: Mode = xs.reduce((acc, x: i32) => "b", "a");"#,
        r#"const r: Mode = xs.reduce((acc, x) => "b", "a");"#,
    ] {
        rejects(
            body,
            RuleCode::S100,
            "type mismatch: the initializer expects `Mode`, got `string`",
        );
    }
}

/// Rule 3: each call enters the checker through one function, so a call in
/// a `for…of` subject and an `unreachable` statement are checked too.
#[test]
fn each_call_entry_checks_its_type_arguments() {
    for (body, name) in [
        ("for (const k of m.keys<i32>()) { print(`${k}`); }", "keys"),
        (
            "for (const v of xs.values<i32>()) { print(`${v}`); }",
            "values",
        ),
        (
            "for (const v of s.values<i32>()) { print(`${v}`); }",
            "values",
        ),
        ("unreachable<i32>();", "unreachable"),
    ] {
        rejects(
            body,
            RuleCode::S100,
            &format!("`{name}` takes no type arguments, got 1"),
        );
    }
    for body in [
        "for (const k of m.keys()) { print(`${k}`); }",
        "for (const v of s.values()) { print(`${v}`); }",
    ] {
        accepts(body);
    }
}

/// Rule 3 covers built-ins: a user generic call with type arguments keeps
/// its rule.
#[test]
fn a_user_generic_call_with_type_arguments_stays_accepted() {
    for body in [
        "const r: i32 = id<i32>(1);",
        "const r: i32 = U.make<i32>(1);",
        "const r: i32 = new U().keep<i32>(1);",
    ] {
        accepts(body);
    }
}
