//! A value that holds a handle has an origin, and each discharge closes
//! it (compiler.md §188.1 rules 1–4). Each case checks a program that
//! drops one origin and a control in the same shape that discharges it.
//! Cost: two `check_program` runs per case; no lowering and no execution.

use subscript_compiler::{check_program, divergence::Divergence, RuleCode, SourceFile};

/// The comment that marks the origin site in a dropped program.
const ORIGIN: &str = "/*o*/";

/// One case: the declarations and the `main` body of the dropped program
/// and of its control.
struct Case {
    name: &'static str,
    dropped: (&'static str, &'static str),
    control: (&'static str, &'static str),
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
            name: "a function parameter of Promise<i32>[][]",
            dropped: (
                "function count(/*o*/xs: Promise<i32>[][]): i32 { return xs.length; }",
                "print(`${count(grid())}`);",
            ),
            control: (
                "async function count(xs: Promise<i32>[][]): Promise<i32> { return await xs[0][0]; }",
                "print(`${await count(grid())}`);",
            ),
        },
        Case {
            name: "a method parameter of FixedArray<Promise<i32>, 2>",
            dropped: (
                "class C { size(/*o*/xs: FixedArray<Promise<i32>, 2>): i32 { return 2; } }",
                "const c = new C(); print(`${c.size(pair())}`);",
            ),
            control: (
                "class C { async size(xs: FixedArray<Promise<i32>, 2>): Promise<i32> { return await xs[0]; } }",
                "const c = new C(); print(`${await c.size(pair())}`);",
            ),
        },
        Case {
            name: "a constructor parameter",
            dropped: (
                "class C { n: i32; constructor(/*o*/jobs: Promise<i32>[]) { this.n = jobs.length; } }",
                "const c = new C([work(1)]); print(`${c.n}`);",
            ),
            control: (
                "class C { jobs: Promise<i32>[]; constructor(jobs: Promise<i32>[]) { this.jobs = jobs; } }",
                "const c = new C([work(1)]); print(`${await c.jobs[0]}`);",
            ),
        },
        Case {
            name: "a lambda parameter",
            dropped: (
                "",
                "const f = (/*o*/p: Promise<i32>): i32 => 1; print(`${f(work(1))}`);",
            ),
            control: (
                "",
                "const f = (p: Promise<i32>): Promise<i32> => p; print(`${await f(work(1))}`);",
            ),
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

/// Rule 5 and 188.3: the runtime cases are accepted, and the open forms
/// stay S013. Each program is one measured form of the §188 note.
#[test]
fn runtime_cases_are_accepted_and_open_forms_stay_rejected() {
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
    let open_forms = [
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
    for (id, declarations, body) in open_forms {
        let source = program(declarations, body);
        let errors = check_program(&[SourceFile::new("open.ts", source)]).expect_err(id);
        assert!(
            errors
                .iter()
                .all(|error| error.divergence == Some(Divergence::DroppedAsyncHandle)),
            "{id}: {errors:?}"
        );
    }
}
