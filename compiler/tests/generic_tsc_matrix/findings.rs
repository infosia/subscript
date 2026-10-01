//! Concrete controls for the derived-value forms of §143 rule 1a.

use super::*;

pub(super) fn cells() -> Vec<Cell> {
    let forms = [
        ("function-field-return", "class G<T extends () => i32> { f: T; constructor(f: T) { this.f = f; } run(): i32 { return this.f(); } } function one(): i32 { return 1; }", "const g = new G<() => i32>(one); g.run();"),
        ("function-field-parameter", "class G<T extends (x: i32) => void> { f: T; constructor(f: T) { this.f = f; } run(): void { this.f(1); } } function consume(x: i32): void {}", "const g = new G<(x: i32) => void>(consume); g.run();"),
        ("object-pattern", "function g<T extends Box>(x: T): i32 { const { v } = x; return v; }", "g<Box>(new Box());"),
        ("parameter-pattern", "function g<T extends Box>({ v }: T): i32 { return v; }", "g<Box>(new Box());"),
        ("iteration-pattern", "function g<T extends Box>(xs: T[]): i32 { let n: i32 = 0; for (const { v } of xs) { n += v; } return n; }", "g<Box>([new Box()]);"),
        ("narrowed-pattern", "function g<T extends Box>(x: T | null): i32 { if (x !== null) { const { v } = x; return v; } return 0; }", "g<Box>(new Box());"),
        ("array-pattern", "function g<T extends i32[]>(x: T): i32 { const [a] = x; return a; }", "g<i32[]>([1]);"),
        ("ternary-number", "function g<T extends i32>(x: T, c: boolean): void { const a = c ? x : 0; }", "g<i32>(1, true);"),
        ("ternary-class", "function g<T extends Box>(x: T, c: boolean): void { const a = c ? x : new Box(); const v = a.v; }", "g<Box>(new Box(), true);"),
        ("ternary-parameters", "function g<T extends Box, U extends Box>(x: T, y: U, c: boolean): i32 { return (c ? x : y).v; }", "g<Box, Box>(new Box(), new Box(), true);"),
        ("array-class", "function g<T extends Box>(x: T, b: Box): void { const a = [x, b]; const v = a[0].v; }", "g<Box>(new Box(), new Box());"),
        ("array-number", "function g<T extends i32>(x: T): void { const a = [x, 1]; }", "g<i32>(1);"),
        ("nullish-member", "function g<T extends Box>(x: T | null, b: Box): i32 { return (x ?? b).v; }", "g<Box>(new Box(), new Box());"),
        ("nullish-new-member", "function g<T extends Box>(x: T | null): i32 { return (x ?? new Box()).v; }", "g<Box>(new Box());"),
        ("nullish-method", "function g<T extends Box>(x: T | null, b: Box): i32 { const a = x ?? b; return a.get(); }", "g<Box>(new Box(), new Box());"),
        ("throw-unconstrained", "function g<T>(x: T): void { throw x; }", "g<i32>(1);"),
        ("throw-error", "function g<E extends Error>(e: E): void { throw e; }", "g<Error>(new Error(\"x\"));"),
        ("union-shared-member", "class Other { v: i32 = 2; get(): i32 { return this.v; } } function g<T extends Box, U extends Other>(x: T, y: U, c: boolean): i32 { const a = c ? x : y; return a.v + a.get(); }", "g<Box, Other>(new Box(), new Other(), true);"),
        ("ternary-number-operation", "function g<T extends i32>(x: T, c: boolean): i32 { return (c ? x : 0) + 1; }", "g<i32>(1, true);"),
        ("f16-unary", "function g<T extends f16>(x: T): void { const a = -x; }", "g<i32>(1);"),
        ("date-template", "function g<T extends Date>(x: T): string { return `${x}`; }", "g<Date>(new Date(0 as i64));"),
    ];
    let mut cells = Vec::new();
    for (kind, constraint, argument, value) in [
        ("array", "i32[]", "i32[]", "[1, 2]"),
        (
            "map",
            "Map<i32, i32>",
            "Map<i32, i32>",
            "new Map<i32, i32>()",
        ),
        ("function-return", "() => i32", "() => i32", "one"),
        (
            "function-parameter",
            "(x: i32) => void",
            "(x: i32) => void",
            "consume",
        ),
    ] {
        for (site, body) in [
            ("forward-argument", "return inner<T>(x);"),
            (
                "class-field-type",
                "const holder = new G<T>(x); return holder.value;",
            ),
        ] {
            let declaration = format!(
                "function one(): i32 {{ return 1; }} function consume(x: i32): void {{}} \
                 function inner<A extends {constraint}>(x: A): A {{ return x; }} \
                 class G<A extends {constraint}> {{ value: A; constructor(value: A) {{ this.value = value; }} }} \
                 function g<T extends {constraint}>(x: T): T {{ {body} }}"
            );
            let main = format!("g<{argument}>({value});");
            for instance in [false, true] {
                cells.push(build_cell(CellInput {
                    name: &format!("derived-{kind}-{site}"),
                    declaration: &declaration,
                    main_body: if instance { &main } else { "" },
                    instance,
                    concrete_source: None,
                    divergence: None,
                }));
            }
        }
    }
    for (name, body, main) in forms {
        let declaration =
            format!("class Box {{ v: i32 = 1; get(): i32 {{ return this.v; }} }} {body}");
        for instance in [false, true] {
            if instance && name == "date-template" {
                continue;
            }
            cells.push(build_cell(CellInput {
                name: &format!("derived-{name}"),
                declaration: &declaration,
                main_body: if instance { main } else { "" },
                instance,
                concrete_source: None,
                divergence: if name == "throw-unconstrained" {
                    Some(Divergence {
                        code: RuleCode::S010,
                        record: "compiler.md §115",
                        token: "`throw expr` requires the static type of an Error-family class",
                    })
                } else if instance && name == "union-shared-member" {
                    Some(Divergence {
                        code: RuleCode::S005,
                        record: "C1",
                        token: "nominal",
                    })
                } else {
                    None
                },
            }));
        }
    }
    cells
}
