//! A value that holds a handle has an origin, and each discharge closes
//! it (compiler.md §188.1 rules 1–4). Each case checks a program that
//! drops one origin and a control in the same shape that discharges it.
//! Cost: two `check_program` runs per case; no lowering and no execution.

use subscript_compiler::{check_program, divergence::Divergence, RuleCode, SourceFile};

/// The comment that marks the origin site in a dropped program.
const ORIGIN: &str = "/*o*/";

/// One case: the declarations and the `main` body of the dropped program
/// and of its control.
struct Case<'a> {
    name: &'static str,
    dropped: (&'a str, &'a str),
    control: (&'a str, &'a str),
}

fn program(declarations: &str, body: &str) -> String {
    format!(
        "async function work(v: i32): Promise<i32> {{ return v; }}
async function many(): Promise<Promise<i32>[]> {{ return [work(1), work(2)]; }}
function grid(): Promise<i32>[][] {{ return [[work(1)]]; }}
function pair(): FixedArray<Promise<i32>, 2> {{ return [work(1), work(2)]; }}
{declarations}
export async function main(): Promise<void> {{
  {body}
}}
"
    )
}

/// Returns the 1-based line and column of the token after the marker.
fn marked_site(source: &str) -> (u32, u32) {
    for (index, line) in source.lines().enumerate() {
        if let Some(column) = line.find(ORIGIN) {
            let line = u32::try_from(index + 1).expect("line fits");
            let column = u32::try_from(column + ORIGIN.len() + 1).expect("column fits");
            return (line, column);
        }
    }
    panic!("the dropped program has no origin marker");
}

fn run(cases: &[Case]) {
    for case in cases {
        let dropped = program(case.dropped.0, case.dropped.1);
        let site = marked_site(&dropped);
        let errors =
            check_program(&[SourceFile::new("origin.ts", dropped.clone())]).expect_err(case.name);
        let found = errors
            .iter()
            .map(|error| (error.code, error.divergence, error.pos.line, error.pos.col))
            .collect::<Vec<_>>();
        assert_eq!(
            found,
            vec![(
                RuleCode::S013,
                Some(Divergence::DroppedAsyncHandle),
                site.0,
                site.1
            )],
            "{}: {dropped}",
            case.name
        );
        let control = program(case.control.0, case.control.1);
        if let Err(errors) = check_program(&[SourceFile::new("origin.ts", control.clone())]) {
            panic!("{} control: {errors:?}\n{control}", case.name);
        }
    }
}

#[test]
fn each_origin_kind_is_reported_at_its_site_and_its_control_is_accepted() {
    run(&[
        Case {
            name: "a call result of Promise<i32>[][]",
            dropped: ("", "const g = /*o*/grid(); print(`${g.length}`);"),
            control: ("", "const g = grid(); print(`${await g[0][0]}`);"),
        },
        Case {
            name: "a call result of FixedArray<Promise<i32>, 2>",
            dropped: ("", "const p = /*o*/pair(); print(\"x\");"),
            control: ("", "const p = pair(); print(`${await p[0]}`);"),
        },
        Case {
            name: "an await result",
            dropped: ("", "const xs = /*o*/await many(); print(`${xs.length}`);"),
            control: ("", "const xs = await many(); print(`${await xs[0]}`);"),
        },
        Case {
            name: "a then callback parameter",
            dropped: (
                "",
                "const n = await many().then((/*o*/xs: Promise<i32>[]): i32 => xs.length); print(`${n}`);",
            ),
            control: (
                "",
                "const n = await many().then((xs: Promise<i32>[]): Promise<i32[]> => Promise.all(xs)); print(`${n.length}`);",
            ),
        },
        Case {
            name: "a declaration pattern binding",
            dropped: ("", "const [a, b] = /*o*/await many(); print(\"x\");"),
            control: ("", "const [a, b] = await many(); print(`${await a}`);"),
        },
        Case {
            name: "a for-of pattern binding",
            dropped: ("", "for (const [a] of /*o*/grid()) { print(\"x\"); }"),
            control: ("", "for (const [a] of grid()) { print(`${await a}`); }"),
        },
    ]);
}

#[test]
fn each_discharge_closes_the_origin_and_its_same_shape_use_does_not() {
    run(&[
        Case {
            name: "an argument of a parameter that holds a handle",
            dropped: (
                "function use(n: i32): i32 { return n; }",
                "const g = /*o*/grid(); print(`${use(g.length)}`);",
            ),
            control: (
                "async function use(xs: Promise<i32>[][]): Promise<i32> { return await xs[0][0]; }",
                "const g = grid(); print(`${await use(g)}`);",
            ),
        },
        Case {
            name: "a return of a type that holds a handle",
            dropped: (
                "function hold(): i32 { const g = /*o*/grid(); return g.length; }",
                "print(`${hold()}`);",
            ),
            control: (
                "function hold(): Promise<i32>[][] { const g = grid(); return g; }",
                "print(`${await hold()[0][0]}`);",
            ),
        },
        Case {
            name: "the operand of Promise.all",
            dropped: ("", "const xs = /*o*/await many(); print(`${xs.length}`);"),
            control: (
                "",
                "const xs = await many(); print(`${(await Promise.all(xs)).length}`);",
            ),
        },
        Case {
            name: "the receiver of then",
            dropped: ("", "const xs = /*o*/await many(); const h = xs[0]; print(\"x\");"),
            control: (
                "",
                "const xs = await many(); const h = xs[0]; print(`${await h.then((v: i32): i32 => v + 1)}`);",
            ),
        },
        Case {
            name: "a store into a field",
            dropped: (
                "class H { jobs: Promise<i32>[] = []; }",
                "const h = new H(); let local: Promise<i32>[] = h.jobs; local = /*o*/await many(); print(\"x\");",
            ),
            control: (
                "class H { jobs: Promise<i32>[] = []; }",
                "const h = new H(); h.jobs = await many(); print(\"x\");",
            ),
        },
        Case {
            name: "a store into an element of a field",
            dropped: (
                "class H { grid: Promise<i32>[][] = [[]]; }",
                "const h = new H(); const local: Promise<i32>[][] = [[]]; local[0] = /*o*/await many(); print(\"x\");",
            ),
            control: (
                "class H { grid: Promise<i32>[][] = [[]]; }",
                "const h = new H(); h.grid[0] = await many(); print(\"x\");",
            ),
        },
        Case {
            name: "a store into a module global",
            dropped: (
                "let G: Promise<i32>[] = [];",
                "let local: Promise<i32>[] = G; local = /*o*/await many(); print(\"x\");",
            ),
            control: (
                "let G: Promise<i32>[] = [];",
                "G = await many(); print(\"x\");",
            ),
        },
        Case {
            name: "a store into a nested element of a local adds to the local",
            dropped: (
                "",
                "const local: Promise<i32>[][] = [[]]; local[0][0] = /*o*/work(1); print(\"x\");",
            ),
            control: (
                "",
                "const local: Promise<i32>[][] = [[]]; local[0][0] = work(1); print(`${await local[0][0]}`);",
            ),
        },
    ]);
}

/// Runtime transfers and borrowed parameters are accepted (§188 rule 5, §329).
#[test]
fn runtime_cases_and_parameter_borrows_are_accepted() {
    let runtime_cases = [
        ("f10", "", "const [a, b] = await many(); print(`${await a}`);"),
        ("f16", "", "const m = new Map<i32, Promise<i32>>(); m.set(1, work(1)); print(`${m.size}`);"),
        ("f22", "let G: Promise<i32>[] = [];", "G.push(work(1)); G = []; print(\"end\");"),
        ("f23", "", "print(`${await (await many())[0]}`);"),
        (
            "g03",
            "let stored: Promise<i32>[] = [];
function consume(job: Promise<i32>): void { stored.push(job); }
function* gen(): Generator<Promise<i32>> { const job = work(1); consume(job); stored.pop(); yield job; }",
            "const r = gen().next(); print(`${r.done}`);",
        ),
        (
            "g04",
            "let stored: Promise<i32>[] = [];
function consume(job: Promise<i32>): void { stored.push(job); }
function* gen(): Generator<Promise<i32>> { const job = work(1); consume(job); stored.pop(); yield job; }",
            "let n: i32 = 0; for (const h of gen()) { n++; } print(`${n}`);",
        ),
        (
            "g05",
            "let stored: Promise<i32>[] = [];
function consume(job: Promise<i32>): void { stored.push(job); }
function* gen(): Generator<Promise<i32>> { const job = work(1); consume(job); stored.pop(); yield job; }",
            "print(`${gen().next().done}`);",
        ),
    ];
    for (id, declarations, body) in runtime_cases {
        let source = program(declarations, body);
        if let Err(errors) = check_program(&[SourceFile::new("runtime.ts", source.clone())]) {
            panic!("{id}: {errors:?}\n{source}");
        }
    }
    let borrowed_forms = [
        (
            "h03",
            "function* gen(): Generator<Promise<i32>> { yield work(1); }",
            "for (const h of gen()) { print(`${await h}`); }",
        ),
        (
            "k10",
            "function count(xs: Promise<i32>[]): i32 { return xs.length; }",
            "const xs = [work(1)]; print(`${count(xs)} ${(await Promise.all(xs)).length}`);",
        ),
    ];
    for (id, declarations, body) in borrowed_forms {
        let source = program(declarations, body);
        let result = check_program(&[SourceFile::new("borrow.ts", source)]);
        assert!(result.is_ok(), "{id}: {:?}", result.err());
    }
}

/// Each borrowed parameter has a control that makes and drops a local
/// origin in the same body (compiler.md §329 rule 1).
#[test]
fn parameters_borrow_and_local_origins_stay_owned() {
    run(&[
        Case {
            name: "function borrow",
            dropped: ("function count(xs: Promise<i32>[][]): i32 { const g = /*o*/grid(); return g.length; }", "print(`${count(grid())}`);"),
            control: ("function count(xs: Promise<i32>[][]): i32 { return xs.length; }", "const g = grid(); print(`${count(g)} ${await g[0][0]}`);"),
        },
        Case {
            name: "method borrow",
            dropped: ("class C { size(xs: Promise<i32>[]): i32 { const ps = /*o*/work(1); return xs.length; } }", "const c = new C(); print(`${c.size([work(1)])}`);"),
            control: ("class C { size(xs: Promise<i32>[]): i32 { return xs.length; } }", "const c = new C(); const xs = [work(1)]; print(`${c.size(xs)} ${(await Promise.all(xs)).length}`);"),
        },
        Case {
            name: "constructor borrow",
            dropped: ("class C { n: i32; constructor(jobs: Promise<i32>[]) { const p = /*o*/work(1); this.n = jobs.length; } }", "const c = new C([work(1)]); print(`${c.n}`);"),
            control: ("class C { n: i32; constructor(jobs: Promise<i32>[]) { this.n = jobs.length; } }", "const jobs = [work(1)]; const c = new C(jobs); print(`${c.n} ${(await Promise.all(jobs)).length}`);"),
        },
        Case {
            name: "lambda borrow through a function value",
            dropped: ("", "const f = (p: Promise<i32>): i32 => { const q = /*o*/work(2); return 1; }; print(`${f(work(1))}`);"),
            control: ("", "const f = (p: Promise<i32>): i32 => 1; const g: (p: Promise<i32>) => i32 = f; const p = work(1); print(`${g(p)} ${await p}`);"),
        },
    ]);
}

/// Borrowing must preserve the TypeScript condition check on the constraint
/// (compiler.md §329 rule 1, §143 rule 1a).
#[test]
fn borrowed_promise_constraints_keep_the_boolean_check() {
    for body in ["if (p) {}", "const n = p ? 1 : 2;"] {
        let source = program(
            &format!("function test<T extends Promise<i32>>(p: T): void {{ {body} }}"),
            "",
        );
        let errors = check_program(&[SourceFile::new("condition.ts", source)])
            .expect_err("a Promise condition is always true");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, RuleCode::S100);
        assert_eq!(errors[0].divergence, None);
        assert!(errors[0].message.starts_with("condition must be boolean"));
    }
    let source = program(
        "function test<T extends Promise<i32>>(p: T): void { const alias = p; }",
        "",
    );
    assert!(check_program(&[SourceFile::new("condition.ts", source)]).is_ok());
}

/// A function value at a callback place borrows. Only the lambda written
/// at that place owns its parameter (compiler.md §329 rule 2).
#[test]
fn reaction_function_values_borrow_and_direct_lambdas_own() {
    for declarations in [
        "function count(xs: Promise<i32>[]): i32 { return xs.length; } async function probe(): Promise<i32> { return await many().then(count); }",
        "async function probe(f: (xs: Promise<i32>[]) => i32): Promise<i32> { return await many().then(f); }",
        "async function probe(): Promise<i32> { const count = (xs: Promise<i32>[]): i32 => xs.length; return await many().then(count); }",
        "const probe = many().then((xs: Promise<i32>[]): i32 => { const f = (p: Promise<i32>): i32 => 1; return f(xs[0]); });",
    ] {
        let result = check_program(&[SourceFile::new("callback.ts", program(declarations, ""))]);
        assert!(result.is_ok(), "{declarations}: {:?}", result.err());
    }
    run(&[Case {
        name: "direct callback origin",
        dropped: ("", "print(`${await many().then((/*o*/xs: Promise<i32>[]): i32 => xs.length)}`);"),
        control: ("", "print(`${await many().then((xs: Promise<i32>[]): Promise<i32[]> => Promise.all(xs))}`);"),
    }]);
    // A global initializer defers an annotated lambda body (§329 rule 2).
    let source = program(
        "const n = many().then((/*o*/xs: Promise<i32>[]): i32 => xs.length);",
        "print(`${await n}`);",
    );
    let site = marked_site(&source);
    let errors =
        check_program(&[SourceFile::new("callback.ts", source)]).expect_err("callback origin");
    assert_eq!(errors.len(), 1);
    assert_eq!(
        (errors[0].code, errors[0].pos.line, errors[0].pos.col),
        (RuleCode::S013, site.0, site.1)
    );
}

/// A default owns each new origin; a borrowed earlier parameter makes no
/// origin (compiler.md §329 rule 3).
#[test]
fn defaults_carry_their_own_origins_into_the_body() {
    for (dropped, control) in [
        ("async function f(p: Promise<i32> = /*o*/work(1)): Promise<i32> { return 0; }", "async function f(p: Promise<i32> = work(1)): Promise<i32> { return await p; }"),
        ("async function f(p = /*o*/work(1)): Promise<i32> { return 0; }", "async function f(p = work(1)): Promise<i32> { return await p; }"),
        ("class C { async f(p: Promise<i32> = /*o*/work(1)): Promise<i32> { return 0; } }", "class C { async f(p: Promise<i32> = work(1)): Promise<i32> { return await p; } }"),
        ("class C { static f(p = /*o*/work(1)): Promise<i32> { return work(2); } }", "class C { static f(p = work(1)): Promise<i32> { return p; } }"),
        ("class C { constructor(p = /*o*/work(1)) {} }", "class C { p: Promise<i32>; constructor(p = work(1)) { this.p = p; } }"),
        ("const f = async (p = /*o*/work(1)): Promise<i32> => 0;", "const f = async (p = work(1)): Promise<i32> => await p;"),
        ("async function f(p: Promise<i32> = /*o*/work(1), q: Promise<i32> = p): Promise<i32> { return await q; }", "async function f(p: Promise<i32> = work(1), q: Promise<i32> = p): Promise<i32> { await p; return await q; }"),
    ] {
        run(&[Case { name: "default origin", dropped: (dropped, ""), control: (control, "") }]);
    }
    run(&[Case {
        name: "each array default origin",
        dropped: ("async function f(ps: Promise<i32>[] = [/*o*/work(1)]): Promise<i32> { return ps.length; }", ""),
        control: ("async function f(ps: Promise<i32>[] = [work(1)]): Promise<i32> { return (await Promise.all(ps)).length; }", ""),
    }]);
}

/// A captured origin can pass through a lambda default. An earlier
/// parameter of that lambda stays separate (§329 rule 3, §317 rule 2).
#[test]
fn lambda_defaults_distinguish_captures_from_earlier_parameters() {
    run(&[Case {
        name: "a lambda default captures an enclosing origin",
        dropped: (
            "async function f(): Promise<i32> { const g = async (p: Promise<i32> = /*o*/work(1), q: Promise<i32> = p): Promise<i32> => await q; return await g(); }",
            "print(`${await f()}`);",
        ),
        control: (
            "async function f(): Promise<i32> { const p = work(1); const g = async (q: Promise<i32> = p): Promise<i32> => await q; return await g(); }",
            "print(`${await f()}`);",
        ),
    }]);
}

/// A yield transfers its handle operand, and a scalar yield leaves a
/// separate origin owned by the generator (compiler.md §329 rule 4).
#[test]
fn yields_discharge_only_the_origins_they_transfer() {
    run(&[Case {
        name: "yield transfer",
        dropped: (
            "function* gen(): Generator<i32> { const p = /*o*/work(1); yield 1; }",
            "for (const n of gen()) { print(`${n}`); }",
        ),
        control: (
            "function* gen(): Generator<Promise<i32>> { yield work(1); }",
            "for (const p of gen()) { print(`${await p}`); }",
        ),
    }]);
}

/// A built-in result carries receiver origins only if its type holds a
/// handle. The result has no independent origin (compiler.md §329 rule 5).
#[test]
fn builtin_results_carry_receiver_origins_without_making_an_origin() {
    for method in [
        "slice(0)",
        "concat([])",
        "slice(0).concat([]).slice(0)",
        "reverse()",
        "splice(0, 1)",
    ] {
        let dropped = format!("const ps = (/*o*/await many()).{method}; print(`${{ps.length}}`);");
        let control = format!("const ps = (await many()).{method}; await Promise.all(ps);");
        run(&[Case {
            name: "receiver origin",
            dropped: ("", &dropped),
            control: ("", &control),
        }]);
    }
    run(&[Case {
        name: "scalar result keeps receiver origin",
        dropped: (
            "",
            "const ps = /*o*/await many(); print(`${ps.push(work(3))}`);",
        ),
        control: (
            "",
            "const ps = await many(); print(`${ps.push(work(3))}`); await Promise.all(ps);",
        ),
    }]);
    let source = program("async function f(ps: Promise<i32>[]): Promise<i32> { const copy = ps.slice(0); return copy.length; }", "");
    assert!(check_program(&[SourceFile::new("builtin.ts", source)]).is_ok());
}
