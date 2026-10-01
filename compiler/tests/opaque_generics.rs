//! A generic body is checked for every type argument (compiler.md §135).

use subscript_compiler::{check_program, Diagnostic, RuleCode, SourceFile};

fn check(source: &str) -> Result<(), Vec<Diagnostic>> {
    check_program(&[SourceFile::new("main.ts", source)]).map(|_| ())
}

/// The diagnostics of `source` as `(code, line, column, message)`.
fn diagnostic_list(source: &str) -> Vec<(RuleCode, u32, u32, String)> {
    match check(source) {
        Ok(()) => Vec::new(),
        Err(diagnostics) => diagnostics
            .into_iter()
            .map(|diagnostic| {
                (
                    diagnostic.code,
                    diagnostic.pos.line,
                    diagnostic.pos.col,
                    diagnostic.message,
                )
            })
            .collect(),
    }
}

/// A program with `declarations` on its first line and `main` holding
/// `body` on its second line.
fn program(declarations: &str, body: &str) -> String {
    format!("{declarations}\nexport function main(): void {{ {body} }}\n")
}

/// Each row: one kind that the opaque check keeps (§135.1 rule 2), in a
/// body that no call instantiates, with its one diagnostic; and a control
/// of the same shape through a type parameter, which the opaque check
/// drops.
const KEPT: &[(&str, (RuleCode, u32, &str), &str)] = &[
    // An unknown name (`tsc` TS2304).
    (
        "function g<T>(x: T): i32 { return nope(); }",
        (RuleCode::S016, 35, "unknown function `nope`"),
        "function g<T>(x: T): i32 { return x + 1; }",
    ),
    (
        "function g<T>(x: T): i32 { return missing; }",
        (RuleCode::S016, 35, "unknown name `missing`"),
        "function g<T>(x: T): T { return x; }",
    ),
    // An assignment to a `const` binding (`tsc` TS2588).
    (
        "function g<T>(x: T): void { const a: i32 = 1; a = 2; }",
        (RuleCode::S100, 47, "cannot rebind `const` binding `a`"),
        "function g<T>(x: T, y: T): void { let a = x; a = y; }",
    ),
    // A type mismatch where neither type involves a type parameter
    // (`tsc` TS2322).
    (
        "function g<T>(x: T): void { const s: string = 5; }",
        (
            RuleCode::S100,
            47,
            "type mismatch: the initializer expects `string`, got `i32`",
        ),
        "function g<T>(x: T): void { const s: string = x; }",
    ),
    (
        "class G<T> { v: i32 = 0; run(): void { this.v = \"a\"; } }",
        (
            RuleCode::S100,
            49,
            "type mismatch: the assignment expects `i32`, got `string`",
        ),
        "class G<T> { v: i32 = 0; w: T; constructor(w: T) { this.w = w; } run(): void { this.v = this.w; } }",
    ),
    // A nullable use (S011) whose type involves no type parameter
    // (`tsc` TS18047).
    (
        "class Box { v: i32 = 1; }\nfunction g<T>(x: T, b: Box | null): i32 { return b.v; }",
        (
            RuleCode::S011,
            50,
            "`Box | null` may be null here; narrow with a null check first",
        ),
        "class Box { v: i32 = 1; }\nfunction g<T extends Box>(x: T | null): i32 { return x.v; }",
    ),
    // A member read, a method call, and a call of the value on a type
    // parameter with no constraint (`tsc` TS2339, TS2349).
    (
        "function g<T>(x: T): i32 { return x.v; }",
        (RuleCode::S018, 37, "`T` has no member `v`"),
        "class Box { v: i32 = 1; }\nfunction g<T extends Box>(x: T): i32 { return x.v; }",
    ),
    (
        "function g<T>(x: T): i32 { return x.get(); }",
        (RuleCode::S018, 37, "`T` has no method `get`"),
        "class Box { v: i32 = 1; get(): i32 { return this.v; } }\nfunction g<T extends Box>(x: T): i32 { return x.get(); }",
    ),
    (
        "function g<T>(x: T): void { x(); }",
        (RuleCode::S100, 29, "type `T` is not callable"),
        "class Box { v: i32 = 1; }\nfunction g<T extends Box>(x: T): void { x(); }",
    ),
];

#[test]
fn each_kept_kind_is_reported_in_an_uninstantiated_body() {
    for (source, (code, column, message), control) in KEPT {
        let source = program(source, "");
        let line = source.lines().count() as u32 - 1;
        assert_eq!(
            diagnostic_list(&source),
            [(*code, line, *column, (*message).to_string())],
            "{source}"
        );

        let control = program(control, "");
        assert_eq!(diagnostic_list(&control), [], "{control}");
    }
}

#[test]
fn an_assignment_to_an_import_binding_is_kept() {
    let lib = "export let count: i32 = 4;";
    let files = |main: &str| {
        [
            SourceFile::entry("main.ts", main),
            SourceFile::new("lib.ts", lib),
        ]
    };
    let main = "import { count } from \"./lib\";\nfunction g<T>(x: T): void { count = 5; }\nexport function main(): void {}\n";
    let diagnostics = check_program(&files(main)).expect_err("an import is read-only");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "cannot assign to `count` because it is an import"
    );
    assert_eq!((diagnostics[0].pos.line, diagnostics[0].pos.col), (2, 29));

    // Control: a local of the same name is writable.
    let main = "import { count } from \"./lib\";\nfunction g<T>(x: T): void { let count: i32 = 4; count = 5; }\nexport function main(): void {}\n";
    check_program(&files(main)).expect("a local is writable");
}

/// Programs that `tsc` accepts, whose generic body the opaque check does
/// not decide (§135.1 rule 2). Each row: the declarations, and the body
/// of `main`.
const DROPPED: &[(&str, &str)] = &[
    // Unary `-` and `~` on a type parameter.
    (
        "function g<T>(x: T, y: T): boolean { return -x < -y; }",
        "",
    ),
    (
        "function g<T>(x: T): i32 { const a = -x; const b = ~x; return a + b; }",
        "",
    ),
    ("function g<T>(x: T): void { let a = -x; a = 3; }", ""),
    ("function g<T>(x: T, y: T): void { let a = -x; a = -y; }", ""),
    ("function g<T>(x: T): void { let a = -x; a += 1; }", ""),
    ("function g<T>(x: T): void { let a = -x; a++; }", ""),
    ("function g<T>(x: T): void { const a = [-x, 1]; }", ""),
    // `switch` on a type parameter.
    (
        "function g<T>(x: T): i32 { switch (x) { case 1: return 1; } return 0; }",
        "print(`${g<i32>(1)}`);",
    ),
    (
        "function g<T>(x: T): i32 { switch (x) { case \"a\": return 1; } return 0; }",
        "print(`${g<string>(\"a\")}`);",
    ),
    (
        "function g<T extends i32>(x: T): i32 { switch (x) { case 1: return 1; } return 0; }",
        "print(`${g<i32>(1)}`);",
    ),
    (
        "function g<T extends string>(x: T): i32 { switch (x) { case \"a\": return 1; } return 0; }",
        "print(`${g<string>(\"a\")}`);",
    ),
    (
        "function g<T>(x: T, y: T): i32 { switch (x) { case y: return 1; } return 0; }",
        "print(`${g<i32>(1, 2)}`);",
    ),
    // A constrained type parameter as an iterable, a spread, an index, and
    // an update operand.
    (
        "function g<T extends i32[]>(x: T): i32 { let s = 0; for (const e of x) { s += e; } return s; }",
        "print(`${g<i32[]>([1, 2])}`);",
    ),
    (
        "function g<T extends i32[]>(x: T): i32 { const a = [...x]; return a.length; }",
        "print(`${g<i32[]>([1, 2])}`);",
    ),
    (
        "function g<T extends i32>(a: i32[], x: T): i32 { return a[x]; }",
        "print(`${g<i32>([1, 2], 1)}`);",
    ),
    (
        "function g<T extends i32>(x: T): T { let y = x; y++; return y; }",
        "print(`${g<i32>(1)}`);",
    ),
    // Narrowing of a constrained type parameter.
    (
        "class Box { v: i32 = 1; }\nfunction g<T extends Box | null>(x: T): i32 { if (x === null) return 0; return x.v; }",
        "print(`${g<Box | null>(new Box())}`);",
    ),
    // `&&`, `||`, and `!` on a type parameter.
    (
        "function g<T extends boolean>(x: T, y: T): T { return x || y; }",
        "print(`${g<boolean>(true, false)}`);",
    ),
    (
        "function g<T extends boolean>(x: T, y: T): boolean { const v: boolean = y && x; return v; }",
        "print(`${g<boolean>(true, false)}`);",
    ),
    (
        "function g<T extends boolean>(x: T): boolean { return !x && x; }",
        "print(`${g<boolean>(true)}`);",
    ),
    // `??` with a type parameter operand.
    (
        "class Box { v: i32 = 1; }\nfunction g<T extends Box | null>(x: T, b: Box): Box { return x ?? b; }",
        "print(`${g<Box | null>(null, new Box()).v}`);",
    ),
    (
        "class Box { v: i32 = 1; }\nfunction g<T extends Box>(x: T | null, b: Box): void { const v = x ?? b; }",
        "g<Box>(null, new Box());",
    ),
    (
        "class Box { v: i32 = 1; }\nfunction g<T>(x: T | null, b: Box): void { const v = x ?? b; }",
        "g<Box>(null, new Box());",
    ),
    // A member read through a constraint.
    (
        "class Box { v: i32 = 1; }\nfunction g<T extends Box>(x: T): i32 { return x.v; }",
        "print(`${g<Box>(new Box())}`);",
    ),
    (
        "class Box { v: i32 = 1; get(): i32 { return this.v; } }\nfunction take(b: Box): i32 { return b.v; }\nfunction g<T extends Box>(x: T): i32 { x.v = 3; const b: Box = x; return x.v + x.get() + take(x) + b.v; }",
        "print(`${g<Box>(new Box())}`);",
    ),
];

#[test]
fn each_dropped_form_is_accepted() {
    for (declarations, body) in DROPPED {
        let source = program(declarations, body);
        assert_eq!(diagnostic_list(&source), [], "{source}");

        // Control: an unknown name in the same body is reported, so the
        // opaque check reaches the body.
        let start = declarations.find("function g<").expect("a generic `g`");
        let open = start + declarations[start..].find("{ ").expect("a body") + 2;
        let declarations = format!("{}nope(); {}", &declarations[..open], &declarations[open..]);
        let source = program(&declarations, body);
        let diagnostics = diagnostic_list(&source);
        assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
        assert_eq!(diagnostics[0].0, RuleCode::S016, "{source}");
    }
}

#[test]
fn a_form_that_tsc_rejects_for_a_type_parameter_is_left_to_the_instance() {
    // compiler.md §135.3: `tsc` TS2365 rejects `x > 1`; the opaque check
    // drops it, and no instance of the body is checked.
    let source = program("function g<T>(x: T): boolean { return x > 1; }", "");
    assert_eq!(diagnostic_list(&source), []);

    // Control: an unknown name in the same uninstantiated body is
    // reported, so the opaque check reaches the body.
    let source = program("function g<T>(x: T): boolean { nope(); return x > 1; }", "");
    assert_eq!(
        diagnostic_list(&source),
        [(RuleCode::S016, 1, 32, "unknown function `nope`".to_string())]
    );

    // Control: an instance at a type argument without `>` reports it.
    let source = program(
        "function g<T>(x: T): boolean { return x > 1; }",
        "print(`${g<string>(\"a\")}`);",
    );
    assert_eq!(
        diagnostic_list(&source),
        [(
            RuleCode::S100,
            1,
            39,
            "operator not defined for `string` and `i32`".to_string()
        )]
    );
}

#[test]
fn a_language_restriction_in_an_uninstantiated_body_is_left_to_the_instance() {
    let uninstantiated = "@ValueType\nclass P<T> { v: string = \"\"; w: T; constructor(w: T) { this.w = w; } }\nexport function main(): void {}\n";
    assert_eq!(diagnostic_list(uninstantiated), []);

    // Control: an unknown name in the same uninstantiated body is
    // reported, so the opaque check reaches the body.
    let reached = "@ValueType\nclass P<T> { v: string = \"\"; w: T; constructor(w: T) { this.w = w; nope(); } }\nexport function main(): void {}\n";
    let diagnostics = diagnostic_list(reached);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, RuleCode::S016, "{diagnostics:?}");

    // Control: an instance reports the value-class field rule.
    let instantiated = "@ValueType\nclass P<T> { v: string = \"\"; w: T; constructor(w: T) { this.w = w; } }\nexport function main(): void { const p = new P<i32>(1); }\n";
    let diagnostics = check(instantiated).expect_err("a string field of a value class");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("value-class whitelist"),
        "{diagnostics:?}"
    );
}

#[test]
fn a_deferred_restriction_is_decided_by_the_instance() {
    // Accepted for a numeric argument.
    let numeric = "function g<T>(x: T): string { return `${x}`; }\nexport function main(): void { print(g<i32>(1)); }\n";
    let accepted = check(numeric);
    assert!(accepted.is_ok(), "{:?}", accepted.err());

    // Control: the instance at a class argument rejects the interpolation.
    let class = "class K { v: i32 = 0; }\nfunction g<T>(x: T): string { return `${x}`; }\nexport function main(): void { print(g<K>(new K())); }\n";
    let diagnostics = check(class).expect_err("a class cannot be interpolated");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "type `K` cannot be interpolated into a template"
    );
}

#[test]
fn a_diagnostic_inside_a_body_instantiated_twice_is_reported_once() {
    let unknown = "function g<T>(x: T): i32 { return nope(); }\nexport function main(): void { g<i32>(1); g<string>(\"a\"); }\n";
    let diagnostics = check(unknown).expect_err("an unknown name");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S016);
    assert_eq!((diagnostics[0].pos.line, diagnostics[0].pos.col), (1, 35));

    // The opaque diagnostic takes the place of every instance diagnostic
    // at the same site with the same code.
    let member = "class B { v: i32 = 5; }\nclass C { v: i32 = 6; }\nfunction g<T>(x: T): i32 { return x.v; }\nexport function main(): void { g<B>(new B()); g<i32>(1); g<C>(new C()); g<u8>(2); }\n";
    let diagnostics = check(member).expect_err("a member read on T");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S018);
    assert_eq!(diagnostics[0].message, "`T` has no member `v`");

    // Control: one instance of the same body reports the same one
    // diagnostic.
    let single = "function g<T>(x: T): i32 { return nope(); }\nexport function main(): void { g<i32>(1); }\n";
    let diagnostics = check(single).expect_err("an unknown name");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[test]
fn a_restriction_left_to_instances_is_reported_once_per_site() {
    // Two instances fail the same deferred restriction with the same
    // message at the same site.
    let source = "class K { v: i32 = 0; }\nfunction g<T, U>(x: T, n: U): U { const s: string = `${x}`; return n; }\nexport function main(): void { g<K, i32>(new K(), 1); g<K, u8>(new K(), 2); }\n";
    let diagnostics = check(source).expect_err("a class cannot be interpolated");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "type `K` cannot be interpolated into a template"
    );
}

#[test]
fn no_opaque_instance_reaches_the_hir() {
    let source = "class G<T> { v: T; constructor(v: T) { this.v = v; } get(): T { return this.v; } }\nfunction f<T>(x: T): T { const g = new G<T>(x); return g.get(); }\nclass H { id<T>(x: T): T { return x; } }\nexport function main(): void { print(`${f<i32>(1)}`); }\n";
    let module = check_program(&[SourceFile::new("main.ts", source)])
        .unwrap_or_else(|diagnostics| panic!("{diagnostics:?}"));
    let classes: Vec<&str> = module
        .classes
        .iter()
        .map(|class| class.name.as_str())
        .filter(|name| name.starts_with('G') || name.starts_with('H'))
        .collect();
    assert_eq!(classes, ["H", "G<i32>"]);
    let h = module
        .classes
        .iter()
        .find(|class| class.name == "H")
        .expect("class H");
    assert!(h.methods.is_empty(), "{:?}", h.methods);
    let mut functions: Vec<String> = module
        .functions
        .iter()
        .map(|function| function.symbol.source_name())
        .collect();
    functions.sort();
    assert_eq!(functions, ["f<i32>", "main"]);
}

#[test]
fn a_generic_body_that_instantiates_itself_at_equal_arguments_is_checked() {
    let source = "function g<T>(x: T, n: i32): i32 { if (n > 0) { return g<T>(x, n - 1); } return 0; }\nexport function main(): void {}\n";
    let accepted = check(source);
    assert!(accepted.is_ok(), "{:?}", accepted.err());

    // Control: an error in the same body is reported once.
    let rejected = "function g<T>(x: T, n: i32): i32 { if (n > 0) { return g<T>(x, n - 1); } return nope(); }\nexport function main(): void {}\n";
    let diagnostics = check(rejected).expect_err("an unknown name");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, RuleCode::S016);
}

#[test]
fn a_type_argument_outside_its_constraint_is_rejected() {
    let cases = [
        // A function.
        (
            "function g<T extends i32>(x: T): void {}\nexport function main(): void { g<string>(\"s\"); }\n",
            "function g<T extends i32>(x: T): void {}\nexport function main(): void { g<i32>(1); }\n",
            (2, 34, "type argument `string` does not satisfy the constraint `i32` of `T`"),
        ),
        // A generic method of a class.
        (
            "class Box { v: i32 = 1; }\nclass Other { w: i32 = 2; }\nclass H { run<T extends Box>(x: T): i32 { return x.v; } }\nexport function main(): void { print(`${new H().run<Other>(new Other())}`); }\n",
            "class Box { v: i32 = 1; }\nclass Other { w: i32 = 2; }\nclass H { run<T extends Box>(x: T): i32 { return x.v; } }\nexport function main(): void { print(`${new H().run<Box>(new Box())}`); }\n",
            (4, 53, "type argument `Other` does not satisfy the constraint `Box` of `T`"),
        ),
        // A class, in a construction and in a type annotation.
        (
            "class Box { v: i32 = 1; }\nclass Other { w: i32 = 2; }\nclass G<T extends Box> { b: T; constructor(b: T) { this.b = b; } read(): i32 { return this.b.v; } }\nexport function main(): void { const g = new G<Other>(new Other()); }\n",
            "class Box { v: i32 = 1; }\nclass Other { w: i32 = 2; }\nclass G<T extends Box> { b: T; constructor(b: T) { this.b = b; } read(): i32 { return this.b.v; } }\nexport function main(): void { const g = new G<Box>(new Box()); print(`${g.read()}`); }\n",
            (4, 48, "type argument `Other` does not satisfy the constraint `Box` of `T`"),
        ),
        (
            "class Box { v: i32 = 1; }\nclass G<T extends Box> { v: i32 = 0; }\nfunction f(g: G<i32>): void {}\nexport function main(): void {}\n",
            "class Box { v: i32 = 1; }\nclass G<T extends Box> { v: i32 = 0; }\nfunction f(g: G<Box>): void {}\nexport function main(): void {}\n",
            (3, 17, "type argument `i32` does not satisfy the constraint `Box` of `T`"),
        ),
    ];
    for (rejected, accepted, (line, column, message)) in cases {
        assert_eq!(
            diagnostic_list(rejected),
            [(RuleCode::S100, line, column, message.to_string())],
            "{rejected}"
        );
        assert_eq!(diagnostic_list(accepted), [], "{accepted}");
    }
}

#[test]
fn a_loop_in_an_uninstantiated_body_ends_a_narrowing() {
    // `tsc` TS18047: the loop assigns `null` to `b`, so `b` is not narrowed
    // at the loop head.
    let source = "class Box { v: i32 = 1; }\nfunction g<T>(x: T, c: boolean): void { let b: Box | null = new Box(); if (b !== null) { while (c) { print(`${b.v}`); b = null; } } }\nexport function main(): void {}\n";
    assert_eq!(
        diagnostic_list(source),
        [(
            RuleCode::S011,
            2,
            111,
            "`Box | null` may be null here; narrow with a null check first".to_string()
        )]
    );

    // Control: without the assignment the narrowing holds.
    let source = "class Box { v: i32 = 1; }\nfunction g<T>(x: T, c: boolean): void { let b: Box | null = new Box(); if (b !== null) { while (c) { print(`${b.v}`); } } }\nexport function main(): void {}\n";
    assert_eq!(diagnostic_list(source), []);
}

#[test]
fn an_instance_diagnostic_with_a_different_code_stays() {
    // The opaque check reports the member read on `T`; the `f64` instance
    // reports its own rule at the same site with a different code.
    let source = "function g<T>(x: T): void { const f = x.toFixed; }\nexport function main(): void { g<f64>(1.5); }\n";
    assert_eq!(
        diagnostic_list(source),
        [
            (
                RuleCode::S014,
                1,
                41,
                "numeric method `toFixed` may only appear in an accepted call (Number formatting on f32/f64; Q25/Q26)".to_string()
            ),
            (
                RuleCode::S018,
                1,
                41,
                "`T` has no member `toFixed`".to_string()
            ),
        ]
    );

    // An instance diagnostic at a site that the opaque check drops stays.
    let source = "function g<T>(x: T): void { const v: i32 = -x; }\nexport function main(): void { g<string>(\"s\"); }\n";
    assert_eq!(
        diagnostic_list(source),
        [(
            RuleCode::S100,
            1,
            44,
            "unary `-` requires a numeric operand, got `string`".to_string()
        )]
    );

    // Control: an instance diagnostic with the same code as the opaque one
    // at the same site is replaced by it.
    let source =
        "function g<T>(x: T): i32 { return x.v; }\nexport function main(): void { g<i32>(1); }\n";
    assert_eq!(
        diagnostic_list(source),
        [(RuleCode::S018, 1, 37, "`T` has no member `v`".to_string())]
    );
}

#[test]
fn a_language_unsupported_name_is_kept_in_an_uninstantiated_body() {
    // compiler.md §135.1 rule 2: S016 includes a name that `tsc` binds and
    // the language does not support.
    let source = program("function g<T>(x: T): string { return String(x); }", "");
    assert_eq!(
        diagnostic_list(&source),
        [(
            RuleCode::S016,
            1,
            38,
            "unknown function `String`".to_string()
        )]
    );

    // Control: the same body through an interpolation of `x`, which the
    // opaque check drops.
    let source = program("function g<T>(x: T): string { return `${x}`; }", "");
    assert_eq!(diagnostic_list(&source), []);
}

#[test]
fn a_shared_location_kill_is_kept_in_an_uninstantiated_body() {
    // compiler.md §135.1 rule 2: S011 with the §124 kill of a shared
    // location, where no type involves a type parameter.
    let declarations = "class Box { v: i32 = 1; }\nlet shared: Box | null = new Box();\nfunction touch(): void {}\n";
    let source = format!(
        "{declarations}function g<T>(x: T): i32 {{ if (shared !== null) {{ touch(); return shared.v; }} return 0; }}\nexport function main(): void {{}}\n"
    );
    assert_eq!(
        diagnostic_list(&source),
        [(
            RuleCode::S011,
            4,
            67,
            "`Box | null` may be null here; narrow with a null check first".to_string()
        )]
    );

    // Control: without the call, the narrowing holds.
    let source = format!(
        "{declarations}function g<T>(x: T): i32 {{ if (shared !== null) {{ return shared.v; }} return 0; }}\nexport function main(): void {{}}\n"
    );
    assert_eq!(diagnostic_list(&source), []);

    // Control: the same nullable use through a type parameter, which the
    // opaque check drops.
    let source = format!(
        "{declarations}function g<T extends Box>(x: T | null): i32 {{ touch(); return x.v; }}\nexport function main(): void {{}}\n"
    );
    assert_eq!(diagnostic_list(&source), []);
}

#[test]
fn a_numeric_type_argument_satisfies_a_numeric_constraint() {
    // compiler.md §135.1 rule 2b: every numeric type is the one type
    // `number` for the constraint relation, at any depth.
    let accepted = [
        "function keep<T extends i32>(x: T): void {}\nexport function main(): void { keep<u8>(1); keep<f64>(1.5); }\n",
        "function keep<T extends i64>(x: T): void {}\nexport function main(): void { keep<i32>(1); }\n",
        "function keep<T extends f64>(x: T): void {}\nexport function main(): void { keep<i32>(1); }\n",
        "function keep<T extends i32[]>(x: T): void {}\nexport function main(): void { keep<u8[]>([1]); }\n",
        "function keep<T extends Map<string, f64>>(x: T): void {}\nexport function main(): void { keep<Map<string, i32>>(new Map<string, i32>()); }\n",
        "function two(a: u8): i32 { return 0; }\nfunction keep<T extends (a: i32) => f64>(x: T): void {}\nexport function main(): void { keep<(a: u8) => i32>(two); }\n",
        "class W<T> { v: T; constructor(v: T) { this.v = v; } }\nfunction keep<T extends W<i32> | null>(x: T): void {}\nexport function main(): void { keep<W<u8> | null>(null); keep<W<f64>>(new W<f64>(1.5)); }\n",
        "function keep<T extends f32>(x: T): void {}\nexport function main(): void { keep<f16>(1); }\n",
        // A generic instance at another numeric argument: no `W<number>`
        // instance exists, and the check makes none.
        "class W<T> { v: T; constructor(v: T) { this.v = v; } }\nfunction keep<T extends W<i32>>(x: T): void {}\nexport function main(): void { keep<W<u8>>(new W<u8>(1)); }\n",
        // A numeric difference inside an instance, and `X` to `X | null`.
        "class W<T> { v: T; constructor(v: T) { this.v = v; } }\nfunction keep<T extends W<i32> | null>(x: T): void {}\nexport function main(): void { keep<W<f64>>(new W<f64>(1.5)); }\n",
        // Instances nested in an array and in an instance.
        "class W<T> { v: T; constructor(v: T) { this.v = v; } }\nfunction keep<T extends W<i32>[]>(x: T): void {}\nfunction deep<T extends W<W<i32>>>(x: T): void {}\nexport function main(): void { keep<W<u8>[]>([]); deep<W<W<u16>>>(new W<W<u16>>(new W<u16>(1))); }\n",
    ];
    for source in accepted {
        assert_eq!(diagnostic_list(source), [], "{source}");
    }

    // Control: a non-numeric argument stays outside the constraint.
    let rejected = [
        (
            "function keep<T extends i32>(x: T): void {}\nexport function main(): void { keep<string>(\"a\"); }\n",
            (2, 37, "type argument `string` does not satisfy the constraint `i32` of `T`"),
        ),
        (
            "function keep<T extends i32[]>(x: T): void {}\nexport function main(): void { keep<boolean[]>([true]); }\n",
            (2, 37, "type argument `boolean[]` does not satisfy the constraint `i32[]` of `T`"),
        ),
        (
            "class W<T> { v: T; constructor(v: T) { this.v = v; } }\nfunction keep<T extends W<i32>>(x: T): void {}\nexport function main(): void { keep<W<string>>(new W<string>(\"a\")); }\n",
            (3, 37, "type argument `W<string>` does not satisfy the constraint `W<i32>` of `T`"),
        ),
        (
            "class W<T> { v: T; constructor(v: T) { this.v = v; } }\nfunction keep<T extends W<i32>[]>(x: T): void {}\nexport function main(): void { keep<W<boolean>[]>([]); }\n",
            (3, 37, "type argument `W<boolean>[]` does not satisfy the constraint `W<i32>[]` of `T`"),
        ),
    ];
    for (source, (line, column, message)) in rejected {
        assert_eq!(
            diagnostic_list(source),
            [(RuleCode::S100, line, column, message.to_string())],
            "{source}"
        );
    }
}

#[test]
fn each_site_of_a_repeated_type_argument_list_is_checked() {
    // compiler.md §135.1 rule 2b: the second site reuses the instance and
    // is still checked.
    let source = "function keep<T extends i32>(x: T): void {}\nexport function main(): void { keep<string>(\"a\"); keep<string>(\"b\"); }\n";
    let message = "type argument `string` does not satisfy the constraint `i32` of `T`";
    assert_eq!(
        diagnostic_list(source),
        [
            (RuleCode::S100, 2, 37, message.to_string()),
            (RuleCode::S100, 2, 56, message.to_string()),
        ]
    );

    // A class in a type annotation and in a construction.
    let source = "class Box { v: i32 = 1; }\nclass H<T extends Box> { n: i32 = 0; }\nexport function main(): void { const h: H<Box | null> = new H<Box | null>(); }\n";
    let message = "type argument `Box | null` does not satisfy the constraint `Box` of `T`";
    assert_eq!(
        diagnostic_list(source),
        [
            (RuleCode::S100, 3, 43, message.to_string()),
            (RuleCode::S100, 3, 63, message.to_string()),
        ]
    );

    // A generic method.
    let source = "class Box { v: i32 = 1; }\nclass Other { w: i32 = 2; }\nclass H { run<T extends Box>(x: T): i32 { return 0; } }\nexport function main(): void { const h = new H(); h.run<Other>(new Other()); h.run<Other>(new Other()); }\n";
    let diagnostics = diagnostic_list(source);
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert_ne!(diagnostics[0].2, diagnostics[1].2, "{diagnostics:?}");

    // Control: a repeated satisfied argument list reports nothing.
    let source = "function keep<T extends i32>(x: T): void {}\nexport function main(): void { keep<u8>(1); keep<u8>(2); }\n";
    assert_eq!(diagnostic_list(source), []);
}

#[test]
fn an_opaque_check_that_makes_concrete_instances_reports_a_site_once() {
    // compiler.md §135.1 rule 3: the opaque check of `f` checks `g<i32>`
    // and `g<f64>`, and the opaque check of `g` checks the same site.
    let caller = "function f<T>(x: T): i32 { return g<i32>(1) + g<f64>(1.0); }\n";
    let source = format!(
        "{caller}function g<U>(u: U): i32 {{ return nope(); }}\nexport function main(): void {{}}\n"
    );
    assert_eq!(
        diagnostic_list(&source),
        [(RuleCode::S016, 2, 35, "unknown function `nope`".to_string())]
    );

    let source = format!(
        "{caller}function g<U>(u: U): i32 {{ const s: string = 5; return 0; }}\nexport function main(): void {{}}\n"
    );
    assert_eq!(
        diagnostic_list(&source),
        [(
            RuleCode::S100,
            2,
            46,
            "type mismatch: the initializer expects `string`, got `i32`".to_string()
        )]
    );

    // Control: two different sites in `g` are two diagnostics.
    let source = format!(
        "{caller}function g<U>(u: U): i32 {{ nope(); return nope(); }}\nexport function main(): void {{}}\n"
    );
    assert_eq!(diagnostic_list(&source).len(), 2);
}
