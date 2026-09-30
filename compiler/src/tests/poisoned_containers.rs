//! compiler.md §132 rule 2a: a type that contains the error type is the
//! error type, so each consumer of a poisoned container reports nothing
//! more than the failure that poisoned it.

use super::*;

/// The declarations that every program below shares.
const DECLS: &str =
    "class M { value: i32 = 0; }\nfunction entry(inbox: Inbox<M>, outbox: Outbox<M>): void {}\n";

/// A consumer of a container whose element type is `{E}`.
struct Consumer {
    label: &'static str,
    /// Module-level declarations, with `{E}` for the element type.
    decls: &'static str,
    /// The body of `main`, with `{E}` for the element type.
    body: &'static str,
    /// The diagnostic that the consumer reports with `{E} = i32`: the
    /// control that the consumer still reports its real errors.
    control: Option<RuleCode>,
    /// The consumer also runs with `{E} = Nope`, an unknown type name.
    unknown_name: bool,
}

const fn consumer(
    label: &'static str,
    decls: &'static str,
    body: &'static str,
    control: Option<RuleCode>,
    unknown_name: bool,
) -> Consumer {
    Consumer {
        label,
        decls,
        body,
        control,
        unknown_name,
    }
}

const CONSUMERS: &[Consumer] = &[
    consumer(
        "filter",
        "function f(a: {E}[]): void { a.filter((w: {E}): boolean => true); }",
        "",
        None,
        true,
    ),
    consumer(
        "map",
        "function f(a: {E}[]): void { a.map((w: {E}): i32 => 0); }",
        "",
        None,
        false,
    ),
    consumer(
        "forEach",
        "function f(a: {E}[]): void { a.forEach((w: {E}): void => {}); }",
        "",
        None,
        false,
    ),
    consumer(
        "find",
        "function f(a: {E}[]): void { a.find((w: {E}): boolean => true); }",
        "",
        Some(RuleCode::S014),
        false,
    ),
    consumer(
        "some",
        "function f(a: {E}[]): void { a.some((w: {E}): boolean => true); }",
        "",
        None,
        false,
    ),
    consumer(
        "every",
        "function f(a: {E}[]): void { a.every((w: {E}): boolean => true); }",
        "",
        None,
        false,
    ),
    consumer(
        "findIndex",
        "function f(a: {E}[]): void { a.findIndex((w: {E}): boolean => true); }",
        "",
        None,
        false,
    ),
    consumer(
        "reduce",
        "function f(a: {E}[]): void { a.reduce((acc: i32, w: {E}): i32 => acc, 0); }",
        "",
        None,
        false,
    ),
    consumer(
        "indexOf",
        "function f(a: {E}[]): void { a.indexOf(a[0]); }",
        "",
        None,
        false,
    ),
    consumer(
        "includes",
        "function f(a: {E}[]): void { a.includes(a[0]); }",
        "",
        None,
        false,
    ),
    consumer(
        "sort",
        "function f(a: {E}[]): void { a.sort(); }",
        "",
        Some(RuleCode::S014),
        false,
    ),
    consumer(
        "join",
        "function f(a: {E}[]): void { a.join(); }",
        "",
        None,
        false,
    ),
    consumer(
        "lastIndexOf",
        "function f(a: {E}[]): void { a.lastIndexOf(a[0]); }",
        "",
        None,
        false,
    ),
    consumer(
        "interpolation",
        "function f(a: {E}[]): void { const s: string = `${a}`; }",
        "",
        Some(RuleCode::S100),
        true,
    ),
    consumer(
        "equality",
        "function f(a: {E}[]): void { const b: boolean = a === a; }",
        "",
        Some(RuleCode::S100),
        false,
    ),
    consumer(
        "assignment",
        "function f(xs: Array<{E}>): void { const ys: string[] = xs; }",
        "",
        Some(RuleCode::S100),
        true,
    ),
    consumer(
        "JSON.parse",
        "",
        "  const a = JSON.parse<{E}[]>(\"[]\");",
        None,
        false,
    ),
    consumer(
        "message transferability",
        "class Msg { ws: {E}[] = []; }\nfunction e2(inbox: Inbox<Msg>, outbox: Outbox<Msg>): void {}",
        "  const w = Worker.spawn(e2);\n  w.close();\n  w.join();",
        Some(RuleCode::S100),
        false,
    ),
    consumer(
        "nested array",
        "function f(a: Array<Array<{E}>>): void { a[0].filter((w: {E}): boolean => true); }",
        "",
        None,
        false,
    ),
    consumer(
        "Map.forEach",
        "function f(m: Map<i32, {E}>): void { m.forEach((v: string, k: i32): void => {}); }",
        "",
        Some(RuleCode::S100),
        true,
    ),
    consumer(
        "Set.forEach",
        "function f(s: Set<{E}>): void { s.forEach((v: string): void => {}); }",
        "",
        Some(RuleCode::S100),
        false,
    ),
    consumer(
        "annotated new Map mismatch",
        "",
        "  const m: Map<i32, {E}> = new Map<i32, string>();",
        Some(RuleCode::S005),
        false,
    ),
    consumer(
        "nested new Map",
        "",
        "  const m: Map<i32, Map<i32, {E}>> = new Map<i32, Map<i32, {E}>>();",
        None,
        false,
    ),
    consumer(
        "new Map in an array literal",
        "",
        "  const a: Map<i32, {E}>[] = [new Map<i32, {E}>()];",
        None,
        false,
    ),
    consumer(
        "empty array in an array literal",
        "",
        "  const a: {E}[][] = [[]];",
        None,
        false,
    ),
];

fn consumer_program(consumer: &Consumer, element: &str) -> String {
    let decls = consumer.decls.replace("{E}", element);
    let body = consumer.body.replace("{E}", element);
    format!("{DECLS}{decls}\nexport function main(): void {{\n{body}\n}}\n")
}

#[test]
fn q35_every_consumer_of_a_poisoned_container_reports_exactly_one_diagnostic() {
    for consumer in CONSUMERS {
        let source = consumer_program(consumer, "Worker<M, M>");
        let diagnostics = check_one(&source).expect_err(consumer.label);
        assert_eq!(diagnostics.len(), 1, "{}: {diagnostics:?}", consumer.label);
        assert_eq!(diagnostics[0].code, RuleCode::S100, "{}", consumer.label);
        assert!(
            diagnostics[0]
                .message
                .starts_with("Worker, Inbox, and Outbox values may not be"),
            "{}: {}",
            consumer.label,
            diagnostics[0].message
        );

        if consumer.unknown_name {
            let source = consumer_program(consumer, "Nope");
            let diagnostics = check_one(&source).expect_err(consumer.label);
            assert_eq!(diagnostics.len(), 1, "{}: {diagnostics:?}", consumer.label);
            assert_eq!(diagnostics[0].code, RuleCode::S016, "{}", consumer.label);
        }

        // Control: the same consumer on a container that is not poisoned
        // reports its real error, or is accepted.
        let source = consumer_program(consumer, "i32");
        match (check_one(&source), consumer.control) {
            (Ok(_), None) => {}
            (Err(diagnostics), Some(code)) => {
                assert_eq!(diagnostics.len(), 1, "{}: {diagnostics:?}", consumer.label);
                assert_eq!(diagnostics[0].code, code, "{}", consumer.label);
            }
            (result, control) => panic!(
                "{} control: expected {control:?}, got {:?}",
                consumer.label,
                result.err()
            ),
        }
    }
}

#[test]
fn q35_poisoned_context_suppresses_only_the_affine_argument() {
    // A construction under a poisoned context still reports an error of
    // its own that the context does not explain.
    let source = format!(
        "{DECLS}export function main(): void {{\n  const m: Map<i32, Worker<M, M>> = new Map<i32, Nope>();\n}}\n"
    );
    let diagnostics = check_one(&source).expect_err("unknown type argument");
    let codes: Vec<RuleCode> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![RuleCode::S100, RuleCode::S016]);

    // Control: without a poisoned context the affine argument of `new`
    // reports at its own position.
    let statement = "const m: Map<i32, string> = new Map<i32, Map<i32, Worker<M, M>>>();";
    let source = format!("{DECLS}export function main(): void {{\n  {statement}\n}}\n");
    let diagnostics = check_one(&source).expect_err("affine nested argument");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S100);
    let column = 3 + statement.find("Worker").expect("affine argument") as u32;
    assert_eq!(
        (diagnostics[0].pos.line, diagnostics[0].pos.col),
        (4, column)
    );
}

#[test]
fn q35_affine_formation_sites_report_the_section_40_1_diagnostic() {
    const ARRAY_ELEMENT: &str = "Worker, Inbox, and Outbox values may not be array elements";
    const CONTAINER_ARGUMENT: &str =
        "Worker, Inbox, and Outbox values may not be container type arguments";
    let cases = [
        (
            "map result",
            "  const xs: i32[] = [1];\n  const ys = xs.map((v: i32): Worker<M, M> => Worker.spawn(entry));",
            ARRAY_ELEMENT,
            (5, 21),
        ),
        (
            "Array.from type argument",
            "  const xs: i32[] = [1];\n  const ys = Array.from<Worker<M, M>>(xs);",
            ARRAY_ELEMENT,
            (5, 25),
        ),
        (
            "Map.groupBy key",
            "  const xs: i32[] = [1];\n  const g = Map.groupBy(xs, (v: i32): Worker<M, M> => Worker.spawn(entry));",
            CONTAINER_ARGUMENT,
            (5, 29),
        ),
        (
            "inferred array literal",
            "  const w: Worker<M, M> = Worker.spawn(entry);\n  const a = [w];\n  w.close();\n  w.join();",
            ARRAY_ELEMENT,
            (5, 14),
        ),
        (
            "spread array literal",
            "  const w: Worker<M, M> = Worker.spawn(entry);\n  const xs: i32[] = [1];\n  const a = [w, ...xs];\n  w.close();\n  w.join();",
            ARRAY_ELEMENT,
            (6, 14),
        ),
    ];
    for (label, body, message, (line, col)) in cases {
        let source = format!("{DECLS}export function main(): void {{\n{body}\n}}\n");
        let diagnostics = check_one(&source).expect_err(label);
        assert_eq!(diagnostics.len(), 1, "{label}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, RuleCode::S100, "{label}");
        assert_eq!(diagnostics[0].message, message, "{label}");
        assert_eq!(
            (diagnostics[0].pos.line, diagnostics[0].pos.col),
            (line, col),
            "{label}"
        );
    }

    // Control: the same forms with a message class are accepted.
    let controls = [
        "  const xs: i32[] = [1];\n  const ys = xs.map((v: i32): M => new M());",
        "  const xs: M[] = [new M()];\n  const ys = Array.from<M>(xs);",
        "  const xs: i32[] = [1];\n  const g = Map.groupBy(xs, (v: i32): M => new M());",
        "  const m = new M();\n  const a = [m];",
        "  const m = new M();\n  const xs: M[] = [m];\n  const a = [m, ...xs];",
    ];
    for body in controls {
        let source = format!("{DECLS}export function main(): void {{\n{body}\n}}\n");
        let result = check_one(&source);
        assert!(result.is_ok(), "{body}: {:?}", result.err());
    }
}

/// The codes, positions, and messages of `diagnostics`, in position order.
fn sorted_diagnostics(diagnostics: &[Diagnostic]) -> Vec<(RuleCode, u32, u32, String)> {
    let mut out: Vec<(RuleCode, u32, u32, String)> = diagnostics
        .iter()
        .map(|d| (d.code, d.pos.line, d.pos.col, d.message.clone()))
        .collect();
    out.sort_by(|a, b| (a.1, a.2).cmp(&(b.1, b.2)));
    out
}

#[test]
fn q35_poisoned_context_does_not_reach_a_generic_instance_body() {
    // The poisoned construction names `G<i32>` first, so the instance
    // body is checked and cached under it. The body still reports its
    // own affine argument.
    let classes = [
        (
            "method body",
            "class G<T> { v: i32 = 0; run(): void { const m = new Map<i32, Worker<M, M>>(); m.clear(); } }\n",
            "  const y = new G<i32>();\n  y.run();",
        ),
        (
            "field",
            "class G<T> { a: Worker<M, M>[] = []; }\n",
            "  const y = new G<i32>();",
        ),
    ];
    for (label, class, uses) in classes {
        // Control: without the poisoned construction.
        let control = format!("{DECLS}{class}export function main(): void {{\n{uses}\n}}\n");
        let control = sorted_diagnostics(&check_one(&control).expect_err(label));
        assert_eq!(control.len(), 1, "{label} control: {control:?}");
        assert_eq!(control[0].0, RuleCode::S100, "{label} control");
        assert_eq!(control[0].1, 3, "{label} control");

        let poisoned = "  const x: Nope = new Map<i32, G<i32>>();";
        let source =
            format!("{DECLS}{class}export function main(): void {{\n{poisoned}\n{uses}\n}}\n");
        let diagnostics = sorted_diagnostics(&check_one(&source).expect_err(label));
        assert_eq!(diagnostics.len(), 2, "{label}: {diagnostics:?}");
        assert_eq!(diagnostics[0], control[0], "{label}");
        assert_eq!(diagnostics[1].0, RuleCode::S016, "{label}");
    }
}

#[test]
fn q35_error_type_argument_keeps_the_arity_diagnostics() {
    let class = "class G<T> { v: i32 = 0; constructor(v: i32) { this.v = v; } }\n";
    let cases: [(&str, &[(RuleCode, &str)]); 6] = [
        (
            "  const a = new G<Nope, i32>(1);",
            &[
                (RuleCode::S100, "`G` expects 1 type argument(s), got 2"),
                (RuleCode::S016, "unknown type name `Nope`"),
            ],
        ),
        (
            "  const a = new G<Nope>(1, 2, 3);",
            &[
                (
                    RuleCode::S100,
                    "`G` expects 1 argument(s) (1 required), got 3",
                ),
                (RuleCode::S016, "unknown type name `Nope`"),
            ],
        ),
        (
            "  const a = new G<Nope>(1);",
            &[(RuleCode::S016, "unknown type name `Nope`")],
        ),
        (
            "  const a: G<Nope, i32> | null = null;",
            &[
                (RuleCode::S100, "`G` expects 1 type argument(s), got 2"),
                (RuleCode::S016, "unknown type name `Nope`"),
            ],
        ),
        // Controls: the same counts without an error type argument.
        (
            "  const a = new G<i32, i32>(1);",
            &[(RuleCode::S100, "`G` expects 1 type argument(s), got 2")],
        ),
        (
            "  const a = new G<i32>(1, 2, 3);",
            &[(
                RuleCode::S100,
                "`G` expects 1 argument(s) (1 required), got 3",
            )],
        ),
    ];
    for (body, expected) in cases {
        let source = format!("{DECLS}{class}export function main(): void {{\n{body}\n}}\n");
        let diagnostics = sorted_diagnostics(&check_one(&source).expect_err(body));
        let actual: Vec<(RuleCode, &str)> = diagnostics
            .iter()
            .map(|(code, _, _, message)| (*code, message.as_str()))
            .collect();
        assert_eq!(actual, expected, "{body}");
    }
    let source =
        format!("{DECLS}{class}export function main(): void {{\n  const a = new G<i32>(1);\n}}\n");
    let result = check_one(&source);
    assert!(result.is_ok(), "{:?}", result.err());
}

#[test]
fn q27_set_algebra_argument_reports_exactly_one_diagnostic() {
    let prefix = "  const values: Set<i32> = new Set<i32>();\n";
    let cases = [
        ("  values.union([1, 2]);", RuleCode::S014),
        ("  values.union();", RuleCode::S100),
        (
            "  const xs: Set<i32>[] = [];\n  values.union(...xs);",
            RuleCode::S014,
        ),
    ];
    for (body, code) in cases {
        let source = format!("{DECLS}export function main(): void {{\n{prefix}{body}\n}}\n");
        let diagnostics = check_one(&source).expect_err(body);
        assert_eq!(diagnostics.len(), 1, "{body}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, code, "{body}");
    }
    // Control: a Set argument is accepted.
    let source = format!(
        "{DECLS}export function main(): void {{\n{prefix}  values.union(new Set<i32>());\n}}\n"
    );
    let result = check_one(&source);
    assert!(result.is_ok(), "{:?}", result.err());
}
