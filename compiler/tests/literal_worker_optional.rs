//! Controls for compiler.md §339 rules 1–7.
use subscript_compiler::{check_program, check_program_with, CheckOptions, Diagnostic, SourceFile};

fn errors(source: &str) -> Vec<Diagnostic> {
    check_program(&[SourceFile::new("main.ts", source)]).expect_err("the source violates the place")
}

// compiler.md §339 rules 1–3. Debug process cost: 0.0371 s; 0 C builds.
#[test]
fn literal_places_keep_const_literals_and_reject_widened_values() {
    let prefix = "type Name = \"a\" | \"b\"; const A = \"a\";";
    for body in [
        "const n: Name = A;",
        "const B = A; const n: Name = B;",
        "const n: Name = `b`;",
        "let x = A;",
    ] {
        check_program(&[SourceFile::new(
            "main.ts",
            format!("{prefix} function f(): void {{ {body} }}"),
        )])
        .expect("literal control");
    }
    for body in [
        "const B = \"c\"; const n: Name = B;",
        "const n: Name = `c`;",
        "let x = A; const n: Name = x;",
        "const x: string = A; const n: Name = x;",
    ] {
        let diagnostics = errors(&format!("{prefix} function f(): void {{ {body} }}"));
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_str() == "S100" && d.message.contains("type mismatch")),
            "{diagnostics:?}"
        );
    }
}

// compiler.md §339 rules 3–5. Debug process cost: 0.0070 s; 0 C builds.
#[test]
fn file_encodings_reject_widened_and_non_utf8_values_without_divergence() {
    let mut options = CheckOptions::default();
    options.enabled_modules = vec!["node:fs/promises".into()];
    for (binding, argument, accepts) in [
        ("const e = \"utf8\";", "e", true),
        ("const e = `utf8`; const f = e;", "f", true),
        ("", "`utf8`", true),
        ("let e = \"utf8\";", "e", false),
        ("let e: string = \"utf8\";", "e", false),
        ("const e = \"utf16\";", "e", false),
    ] {
        let source = format!("import {{ readFile }} from \"node:fs/promises\"; export async function main(): Promise<void> {{ {binding} await readFile(\"a.txt\", {argument}); }}");
        let result = check_program_with(&[SourceFile::new("main.ts", source)], &options);
        if accepts {
            result.expect("literal encoding control");
        } else {
            let diagnostics = result.expect_err("the encoding violates the accepted forms");
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            assert!(diagnostics[0].message.contains("accepted forms"));
            assert!(diagnostics[0].divergence.is_none(), "{diagnostics:?}");
        }
    }
}

// compiler.md §339 rule 6. Debug process cost: 0.0094 s; 0 C builds.
#[test]
fn workers_keep_endpoint_async_and_named_entry_controls() {
    for (entry, argument, accepts) in [
        (
            "function entry(i: Inbox<M>, o: Outbox<M>): i32 { return 1; }",
            "entry",
            true,
        ),
        ("function entry(i: Inbox<M>): void {}", "entry", true),
        ("function entry(): void {}", "entry", true),
        (
            "function entry(i: Inbox<M>, o: Outbox<M>, n: i32 = 1): i32 { return n; }",
            "entry",
            true,
        ),
        ("function entry(i: i32): void {}", "entry", false),
        (
            "async function entry(i: Inbox<M>, o: Outbox<M>): Promise<void> {}",
            "entry",
            false,
        ),
        ("", "(i: Inbox<M>, o: Outbox<M>): void => {}", false),
    ] {
        let source = format!("class M {{ x: i32 = 1; }} {entry} export function main(): void {{ const w = Worker.spawn<M,M>({argument}); w.close(); w.join(); }}");
        let result = check_program(&[SourceFile::new("main.ts", source)]);
        if accepts {
            result.expect("worker control");
        } else {
            let diagnostics = result.expect_err("the entry violates its shape");
            assert!(
                diagnostics.iter().any(|d| d.code.as_str() == "S100"),
                "{diagnostics:?}"
            );
        }
    }
}

// compiler.md §339 rule 6: debug process cost: 0.0113 s; 0 C builds.
#[test]
fn worker_entry_diagnostics_name_the_rejected_cause() {
    let prefix = "class M { x: i32 = 1; }";
    let check = |declarations: &str, call: &str| {
        check_program(&[SourceFile::new(
            "worker-shape.ts",
            format!("{prefix} {declarations} export function main(): void {{ {call}; }}"),
        )])
    };
    check(
        "function entry(): i32 { return 1; }",
        "const w = Worker.spawn<M,M>(entry); w.close(); w.join()",
    )
    .expect("a synchronous prefix with a value result is accepted");
    for (declarations, call, reason) in [
        (
            "function* entry(i: Inbox<M>, o: Outbox<M>): Generator<i32> { yield 1; }",
            "Worker.spawn<M,M>(entry)",
            "Calling a generator creates a suspended frame",
        ),
        (
            "async function job(): Promise<void> {} function entry(): Promise<void> { return job(); }",
            "Worker.spawn<M,M>(entry)",
            "An async handle result needs an await",
        ),
        (
            "function entry(i: Inbox<M>): void {}",
            "Worker.spawn(entry)",
            "no message class for inference",
        ),
        (
            "function inbox(): Inbox<M> { throw new Error(\"default\"); } function entry(i: Inbox<M> = inbox()): void {}",
            "Worker.spawn<M,M>(entry)",
            "An entry with default parameters cannot adapt",
        ),
    ] {
        let diagnostics = check(declarations, call).expect_err("the entry violates its shape");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code.as_str(), "S100");
        assert!(diagnostic.message.contains("assignable to"));
        assert!(diagnostic.message.contains("endpoint prefix"));
        assert!(!diagnostic.message.contains("exact synchronous shape"));
        assert!(
            diagnostic
                .example
                .as_ref()
                .map(|example| example.why)
                .or_else(|| diagnostic.divergence.map(|divergence| divergence.entry().why))
                .is_some_and(|why| why.contains(reason)),
            "{diagnostics:?}"
        );
    }
}

// compiler.md §339 rule 7. Debug process cost: 0.0072 s; 0 C builds.
#[test]
fn optional_calls_stay_rejected_in_value_places() {
    let prefix = "class C { m(): void {} } function use(x: i32): void {}";
    for (body, accepts) in [
        (
            "b ? c?.m() : c?.m(); b || c?.m(); c !== null && c?.m();",
            true,
        ),
        ("const value = c?.m();", false),
        ("use(c?.m());", false),
        ("(b ? c?.m() : c?.m()) ?? 1;", false),
    ] {
        let source = format!("{prefix} function f(b: boolean, c: C | null): void {{ {body} }}");
        let result = check_program(&[SourceFile::new("main.ts", source)]);
        if accepts {
            result.expect("statement control");
        } else {
            let diagnostics = result.expect_err("the optional result occupies a value place");
            assert!(
                diagnostics.iter().any(|d| d.code.as_str() == "S012"),
                "{diagnostics:?}"
            );
        }
    }
}
