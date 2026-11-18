//! Conditional origin facts and defaults (§317 rules 1–3).
//! Cost: in-process checker calls only; no lowering or child processes.

use std::time::Instant;
use subscript_compiler::{check_program, divergence::Divergence, RuleCode, SourceFile};

fn check(body: &str, accepted: bool) {
    let source = format!(
        "async function v(n: i32): Promise<i32> {{ return n; }}
         async function many(): Promise<Promise<i32>[]> {{ return [v(1)]; }}
         {body}
         export function main(): void {{}}"
    );
    let result = check_program(&[SourceFile::new("conditional.ts", source)]);
    if accepted {
        assert!(result.is_ok(), "{body}: {result:?}");
    } else {
        let errors = result.expect_err(body);
        assert!(
            errors.iter().all(|d| d.code == RuleCode::S013
                && d.divergence == Some(Divergence::DroppedAsyncHandle)),
            "{body}: {errors:?}"
        );
        assert!(errors.iter().all(|d| d.pos.line == 3), "{body}: {errors:?}");
    }
}

/// Measured cost: 76 ms for 50 checker runs with one test thread.
#[test]
fn conditional_values_preserve_origin_kind_and_selected_coverage() {
    let started = Instant::now();
    // These operands differ only in the origin they select. Check aliases,
    // patterns, nested selection, and a made origin beside a selected one.
    let selections = [
        "c ? p : q",
        "c ? p : v(3)",
        "c ? (c ? p : q) : p",
        "c ? (c ? v(3) : p) : q",
    ];
    for value in selections {
        for usage in [
            format!("await ({value});"),
            format!("const r = {value}; const s = r; await s;"),
            format!("const [r] = [{value}]; await r;"),
        ] {
            for discharge in ["", "await p; await q;"] {
                check(
                    &format!("async function f(c: boolean): Promise<void> {{ const p = v(1); const q = v(2); {usage} {discharge} }}"),
                    !discharge.is_empty(),
                );
            }
        }
    }
    for usage in [
        "await (c ? v(1) : v(2));",
        "const r = c ? v(1) : v(2); const s = r; await s;",
        "const [r] = c ? [v(1)] : [v(2)]; await r;",
        "const p = v(1); const r = c ? p : p; await r;",
        "const p = v(1); const alias = p; const r = c ? alias : p; await r;",
        "const p = v(1); const [r] = c ? [p] : [p]; await r;",
        "const r = c ? await many() : await many(); await r[0];",
        "const p = await many(); const q = await many(); const r = c ? p : q; await r[0]; await p[0]; await q[0];",
        "const p = v(1); const q = v(2); const r = c ? p : q; await Promise.all([r, p, q]);",
    ] {
        check(&format!("async function f(c: boolean): Promise<void> {{ {usage} }}"), true);
    }
    check("async function f(c: boolean, p: Promise<i32>, q: Promise<i32>): Promise<i32> { return await (c ? p : q); }", true);
    check("async function f(c: boolean, p: Promise<i32>, q: Promise<i32>): Promise<i32> { await p; await q; return await (c ? p : q); }", true);
    check("async function f(c: boolean): Promise<void> { const p = await many(); const q = await many(); const [r] = c ? p : q; await r; }", false);
    for (declarations, usage) in [
        (
            "async function use(p: Promise<i32>): Promise<void> { await p; }",
            "await use(c ? p : q);",
        ),
        ("", "await Promise.all(c ? [p] : [q]);"),
        ("", "await (c ? p : q).then((n: i32): i32 => n);"),
        (
            "class C { xs: Promise<i32>[] = []; }",
            "const holder = new C(); holder.xs = c ? [p] : [q];",
        ),
        (
            "let stored: Promise<i32>[] = [];",
            "stored = c ? [p] : [q];",
        ),
    ] {
        for discharge in ["", "await p; await q;"] {
            check(&format!("{declarations} async function f(c: boolean): Promise<void> {{ const p = v(1); const q = v(2); {usage} {discharge} }}"), !discharge.is_empty());
        }
    }
    check("async function f(c: boolean, p: Promise<i32>, q: Promise<i32>): Promise<i32> { return c ? p : q; }", true);
    check("async function f(c: boolean, p: Promise<i32>, q: Promise<i32>): Promise<i32> { await p; await q; return c ? p : q; }", true);
    check("async function f(c: boolean): Promise<i32> { const p = v(1); const q = v(2); return c ? p : q; }", false);
    check("async function f(c: boolean): Promise<i32> { const p = v(1); const q = v(2); await p; await q; return c ? p : q; }", true);
    eprintln!("s317 conditional checker cost: {:?}", started.elapsed());
}

/// Measured cost: 13 ms for 16 checker runs with one test thread.
#[test]
fn parameter_defaults_borrow_earlier_parameters() {
    let started = Instant::now();
    for declaration in [
        "async function f(p: Promise<i32>, q: Promise<i32> = p): Promise<i32> { DISCHARGE return await q; }",
        "class C { async f(p: Promise<i32>, q: Promise<i32> = p): Promise<i32> { DISCHARGE return await q; } }",
        "class C { static f(p: Promise<i32>[], q: Promise<i32>[] = p): Promise<i32>[] { DISCHARGE return q; } }",
        "class C { p: Promise<i32>[] = []; constructor(p: Promise<i32>[], q: Promise<i32>[] = p) { DISCHARGE this.p = q; } }",
        "function outer(): void { const f = async (p: Promise<i32>, q: Promise<i32> = p): Promise<i32> => { DISCHARGE return await q; }; }",
        "function* f(p: Promise<i32>[], q: Promise<i32>[] = p): Generator<i32> { DISCHARGE const held = new Map<i32, Promise<i32>[]>(); held.set(1, q); yield 1; }",
    ] {
        check(&declaration.replace("DISCHARGE", ""), true);
        let discharged = if declaration.contains("constructor") {
            "this.p = p;"
        } else if declaration.contains("function*") || declaration.contains("static f") {
            "const used = new Map<i32, Promise<i32>[]>(); used.set(1, p);"
        } else {
            "await p;"
        };
        check(&declaration.replace("DISCHARGE", discharged), true);
    }
    check("async function f(p: Promise<i32>, q: Promise<i32> = v(2)): Promise<i32> { await p; return await q; }", true);
    check("async function f(p: Promise<i32>, q: Promise<i32> = v(2)): Promise<i32> { return await q; }", true);
    check("async function f(c: boolean, p: Promise<i32>, q: Promise<i32> = c ? p : v(2)): Promise<i32> { return await q; }", true);
    check("async function f(c: boolean, p: Promise<i32>, q: Promise<i32> = c ? p : v(2)): Promise<i32> { await p; return await q; }", true);
    eprintln!("s317 default checker cost: {:?}", started.elapsed());
}

/// One-handle alternatives keep handle types at every transfer site.
#[test]
fn one_handle_alternatives_fit_sync_returns_and_all_discharge_forms() {
    for declaration in [
        "function pick(PARAMS): Promise<i32> { return VALUE; }",
        "const pick = (PARAMS): Promise<i32> => { return VALUE; };",
        "class Picker { pick(PARAMS): Promise<i32> { return VALUE; } }",
        "async function pick(PARAMS): Promise<i32> { return VALUE; }",
    ] {
        check(
            &declaration
                .replace("PARAMS", "c: boolean, p: Promise<i32>, q: Promise<i32>")
                .replace("VALUE", "c ? p : q"),
            true,
        );
        check(
            &declaration
                .replace("PARAMS", "p: Promise<i32>")
                .replace("VALUE", "p"),
            true,
        );
    }
    for usage in [
        "await (VALUE);",
        "await use(VALUE);",
        "await Promise.all([VALUE]);",
        "await (VALUE).then((n: i32): i32 => n);",
        "stored = [VALUE];",
        "const holder = new Holder(); holder.xs = [VALUE];",
    ] {
        for parameters in [false, true] {
            let setup = "async function use(p: Promise<i32>): Promise<void> { await p; } let stored: Promise<i32>[] = []; class Holder { xs: Promise<i32>[] = []; }";
            let params = if parameters {
                "c: boolean, p: Promise<i32>, q: Promise<i32>"
            } else {
                "c: boolean"
            };
            let locals = if parameters {
                ""
            } else {
                "const p = v(1); const q = v(2);"
            };
            check(
                &format!(
                    "{setup} async function f({params}): Promise<void> {{ {locals} {} }}",
                    usage.replace("VALUE", "c ? p : q")
                ),
                parameters,
            );
            let params = if parameters { "p: Promise<i32>" } else { "" };
            let locals = if parameters { "" } else { "const p = v(1);" };
            check(
                &format!(
                    "{setup} async function f({params}): Promise<void> {{ {locals} {} }}",
                    usage.replace("VALUE", "p")
                ),
                true,
            );
        }
    }
}
