//! Module declaration identities (§125) and entry-module host names (§129).

use subscript_compiler::{check_program, Pos, RuleCode, SourceFile};

fn files(first: &str, second: &str) -> Vec<SourceFile> {
    vec![
        SourceFile::new("first.ts", first),
        SourceFile::new("second.ts", second),
        SourceFile::entry("api.ts", ""),
    ]
}

fn host_case(source: &str) {
    let mut input = vec![
        SourceFile::entry("first.ts", source),
        SourceFile::new("second.ts", source),
    ];
    for reverse in [false, true] {
        if reverse {
            input.reverse();
        }
        let module = check_program(&input).expect("only the entry export is a host entry");
        assert_eq!(module.host_entries.len(), 1);
        assert_eq!(module.host_entries[0].name, "update");
        assert_eq!(module.host_entries[0].pos.file, "first.ts");
    }
    input
        .iter_mut()
        .find(|file| file.entry)
        .unwrap()
        .source
        .push_str("\nexport { update };\n");
    let errors = check_program(&input).expect_err("duplicate entry export");
    assert_eq!(errors[0].code, RuleCode::S017);
}

#[test]
fn synchronous_host_entry_names_are_scoped() {
    host_case("export function update(): void {}");
}

#[test]
fn scalar_host_entry_names_are_scoped() {
    host_case("export function update(value: i32, enabled: boolean): void {}");
}

#[test]
fn handle_host_entry_names_are_scoped() {
    host_case("class Handle {} export function update(value: Handle): void {}");
}

#[test]
fn asynchronous_host_entry_names_are_scoped() {
    host_case("export async function update(): Promise<void> { await Context.suspend(); }");
}

#[test]
fn exports_without_host_symbols_keep_module_scope() {
    for declaration in [
        "export function update(): i32 { return 1; }",
        "export function update(value: string): void { print(value); }",
        "export function* update(): Generator<i32> { yield 1; }",
        "export function update<T>(value: T): T { return value; }",
    ] {
        check_program(&files(declaration, declaration)).expect("non-host exports");
    }
    host_case("export function update(): void {}");
}

#[test]
fn all_top_level_kinds_bind_in_their_own_module() {
    for declaration in [
        "let value: i32 = 1;",
        "function value(): i32 { return 1; }",
        "class value { item: i32 = 1; }",
        "function value<T>(item: T): T { return item; }",
        "class value<T> { item: T; constructor(item: T) { this.item = item; } }",
    ] {
        for export in ["", "export "] {
            let source = format!("{export}{declaration}");
            check_program(&files(&source, &source)).expect("independent module declarations");
            let duplicate = format!("{source}\n{source}");
            let errors = check_program(&[SourceFile::new("one.ts", duplicate)])
                .expect_err("one module has one namespace");
            assert!(errors.iter().any(|error| error.code == RuleCode::S017));
        }
    }
}

#[test]
fn a_module_hides_a_mirror_constant_only_in_its_own_scope() {
    let mut input = files(
        "let K: string = \"local\"; export function local(): string { return K; }",
        "export function mirror(): u64 { return K; }",
    );
    input.insert(
        0,
        SourceFile::ambient("mirror.d.ts", "declare const K = 11;"),
    );
    let module = check_program(&input).expect("ambient constant remains visible in second.ts");
    assert_eq!(module.globals[0].name, "K");
    assert_eq!(module.globals[0].ty, subscript_compiler::Type::Str);
    let hidden = "let K: string = \"local\"; export function mirror(): u64 { return K; }";
    input[2] = SourceFile::new("second.ts", hidden);
    check_program(&input).expect_err("a local string cannot return as the mirror integer");
}

#[test]
fn hir_keeps_source_names_beside_distinct_symbols() {
    let declarations = "let x: i32 = 1; export function read(): i32 { return x; }";
    let module = check_program(&files(declarations, declarations)).expect("module identities");
    assert_eq!(module.globals[0].name, "x");
    assert_eq!(module.globals[1].name, "x");
    assert_ne!(module.globals[0].symbol, module.globals[1].symbol);
    assert_eq!(module.functions[0].name, "read");
    assert_eq!(module.functions[1].name, "read");
    assert_ne!(module.functions[0].symbol, module.functions[1].symbol);
}

#[test]
fn class_symbols_are_unique_for_one_source_name_in_two_modules() {
    let first = "@ValueType class RandomF32 { a: f32 = 0.0; b: f32 = 0.0; } \
                 class Box<T> { item: T; constructor(item: T) { this.item = item; } } \
                 export function one(): f32 { const r = new RandomF32(); return r.a + new Box<f32>(1.0).item; }";
    let second = "@ValueType class RandomF32 { a: f32 = 0.0; b: f32 = 0.0; c: f32 = 0.0; } \
                  class Box<T> { item: T; constructor(item: T) { this.item = item; } } \
                  export function two(): f32 { const r = new RandomF32(); return r.c + new Box<f32>(2.0).item; }";
    let module = check_program(&files(first, second)).expect("two modules");
    for name in ["RandomF32", "Box<f32>"] {
        let classes: Vec<_> = module.classes.iter().filter(|c| c.name == name).collect();
        // Control: the source name alone does not tell the two classes apart.
        assert_eq!(classes.len(), 2, "{name}");
        assert_eq!(classes[0].name, classes[1].name);
        assert_ne!(classes[0].symbol, classes[1].symbol, "{name}");
        assert!(classes
            .iter()
            .all(|class| class.symbol.source_name() == class.name));
    }
    let random: Vec<_> = module
        .classes
        .iter()
        .filter(|c| c.name == "RandomF32")
        .map(|c| (c.pos.file.as_str(), c.fields.len()))
        .collect();
    assert_eq!(random, [("first.ts", 2), ("second.ts", 3)]);
    let symbols: std::collections::HashSet<_> = module.classes.iter().map(|c| &c.symbol).collect();
    assert_eq!(symbols.len(), module.classes.len());
}

#[test]
fn generic_instance_identity_includes_the_template_module() {
    let source = "function pick<T>(value: T): T { return value; } export function main(): void { print(`${pick<i32>(7)}`); }";
    let instance = |file| {
        let module = check_program(&[SourceFile::new(file, source)]).expect("generic instance");
        module
            .functions
            .into_iter()
            .find(|f| f.name == "pick<i32>")
            .expect("pick instance")
    };
    let first = instance("first.ts");
    let second = instance("second.ts");
    assert_eq!(first.name, second.name);
    assert_ne!(first.symbol, second.symbol);
    assert_eq!(first.symbol, instance("first.ts").symbol);
}

#[test]
fn mismatched_nominal_types_name_both_modules() {
    let errors = check_program(&[
        SourceFile::entry("main.ts", "import { take } from \"./lib\"; class C { value: i32 = 7; } export function main(): void { take(new C()); }"),
        SourceFile::new("lib.ts", "class C { value: i32 = 100; } export function take(value: C): i32 { return value.value; }"),
    ]).expect_err("nominal classes differ");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("C (main.ts)")
                && error.message.contains("C (lib.ts)")),
        "{errors:?}"
    );
    assert!(errors
        .iter()
        .all(|error| !error.message.contains("[[identity:")));
}

#[test]
fn initializer_routes_disambiguate_same_name_functions() {
    let errors = check_program(&[
        SourceFile::entry("main.ts", "import { libRead } from \"./lib\"; export function read(): i32 { return x; } let x: i32 = 100;"),
        SourceFile::new("lib.ts", "import { read as mainRead } from \"./main\"; function read(): i32 { return mainRead(); } export function libRead(): i32 { return read(); } let y: i32 = libRead();"),
    ]).expect_err("entry global is not initialized yet");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].message, "`x` is accessed before its declaration, through `libRead` -> `read (lib.ts)` -> `read (main.ts)`");
    assert!(errors
        .iter()
        .all(|error| !error.message.contains("[[identity:")));
}

#[test]
fn initializer_routes_disambiguate_same_name_constructors() {
    let errors = check_program(&[
        SourceFile::entry("main.ts", "import { libRead } from \"./lib\"; class C { value: i32; constructor() { this.value = x; } } export function mainRead(): i32 { return new C().value; } let x: i32 = 100;"),
        SourceFile::new("lib.ts", "import { mainRead } from \"./main\"; class C { value: i32; constructor() { this.value = mainRead(); } } export function libRead(): i32 { return new C().value; } let y: i32 = libRead();"),
    ]).expect_err("entry global is not initialized yet");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].message, "`x` is accessed before its declaration, through `libRead` -> `C.constructor (lib.ts)` -> `mainRead` -> `C.constructor (main.ts)`");
    assert!(errors
        .iter()
        .all(|error| !error.message.contains("[[identity:")));
}

#[test]
fn a_module_class_hides_a_mirror_type_alias() {
    let input = [
        SourceFile::ambient("mirror.d.ts", "type C = (value: i32) => i32;"),
        SourceFile::entry(
            "main.ts",
            "class C { value: i32 = 7; } const c: C = new C();",
        ),
        SourceFile::new("lib.ts", "const c: C = (value: i32): i32 => value;"),
    ];
    check_program(&input).expect("module class and sibling alias");
    let mut wrong = input.clone();
    wrong[2] = SourceFile::new("lib.ts", "const c: C = 7;");
    check_program(&wrong).expect_err("sibling still sees callback alias");
}

#[test]
fn builtin_error_diagnostics_keep_source_and_module_names() {
    let errors = check_program(&files("class Error {}", "const q: i32 = new Error(\"x\");"))
        .expect_err("Error is not i32");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("Error (prelude/lang.d.ts)")),
        "{errors:?}"
    );
    assert!(
        errors.iter().all(|e| !e.message.contains("[[")),
        "{errors:?}"
    );
}

#[test]
fn each_mirror_scope_kind_yields_to_the_module_scope() {
    // (mirror, sibling, the first diagnostic when the sibling declares its own
    // `K`: its code and column in `lib.ts`)
    for (mirror, sibling, code, col) in [
        (
            "declare const K = 1;",
            "function read(): u64 { return K; }",
            RuleCode::S007,
            47,
        ),
        (
            "declare function K(): i32;",
            "function read(): i32 { return K(); }",
            RuleCode::S100,
            47,
        ),
        (
            "interface K {}",
            "function read(value: K): K { return value; }",
            RuleCode::S016,
            38, // §156 reports the parameter position before the result position.
        ),
        (
            "declare class K { value: i32; }",
            "function read(value: K): i32 { return value.value; }",
            RuleCode::S016,
            38,
        ),
        (
            "declare enum K { One = 1 }",
            "function read(): K { return K.One; }",
            RuleCode::S016,
            34,
        ),
        (
            "type K = \"one\" | \"two\";",
            "function read(): K { return \"one\"; }",
            RuleCode::S016,
            34,
        ),
    ] {
        let input = [
            SourceFile::ambient(
                "mirror.d.ts",
                format!("// @subscript-c-header include=\"mirror.h\"\n{mirror}"),
            ),
            SourceFile::entry(
                "main.ts",
                "class K { value: i32 = 7; } const local: K = new K();",
            ),
            SourceFile::new("lib.ts", sibling),
        ];
        check_program(&input).unwrap_or_else(|errors| panic!("{mirror}: {errors:?}"));
        let mut hidden = input.clone();
        hidden[2] = SourceFile::new("lib.ts", format!("let K: i32 = 7; {sibling}"));
        let errors =
            check_program(&hidden).expect_err("local value hides the mirror in the sibling too");
        assert_eq!(
            (errors[0].code, errors[0].pos.clone()),
            (code, Pos::new("lib.ts", 1, col)),
            "{mirror}: {errors:?}"
        );
    }
}

#[test]
fn symbol_bearing_error_paths_render_source_names() {
    for (source, spelling) in [
        ("function f(x: i32): void {} f();", "f"),
        ("class C { f<T>(cb: () => T): void { const stored: (() => T)[] = [cb]; } } export function main(): void { const x: i32 = 7; new C().f<i32>((): i32 => x); }", "C.f<i32>"),
        ("class C { static readonly x: i32 = 1; } C.x = 2;", "C.x"),
        ("class C { static readonly x: i32 = 1; } C.x++;", "C.x"),
        ("function f<T>(x: T): T { return x; } f<i32, i32>(1);", "f"),
        ("class C<T> { value: T; constructor(value: T) { this.value = value; } } new C<i32, i32>(1);", "C"),
        ("class C { static f(x: i32): void {} } C.f();", "C.f"),
        ("function f<T>(x: T): T { return x; } f<i32>();", "f<i32>"),
        ("class C { f<T>(x: T): T { return x; } } new C().f<i32>();", "f<i32>"),
        ("function g(): void { f(); } function* f() { yield 1; }", "f"),
    ] {
        let errors = check_program(&[SourceFile::entry("main.ts", source)]).expect_err("invalid call");
        assert!(errors.iter().any(|e| e.message.contains(spelling)), "{errors:?}");
        assert!(errors.iter().all(|e| !e.message.contains("[[identity:")), "{errors:?}");
    }
}

#[test]
fn mirror_non_string_alias_call_reports_type_alias() {
    assert_alias_value_error("SubAccess();", "SubAccess");
}

#[test]
fn mirror_non_string_alias_member_reports_type_alias() {
    assert_alias_value_error("SubLogCallback.foo;", "SubLogCallback");
}

fn assert_alias_value_error(expression: &str, name: &str) {
    let input = [
        SourceFile::ambient(
            "mirror.d.ts",
            "type SubAccess = i32; type SubLogCallback = (value: string) => void;",
        ),
        SourceFile::entry("main.ts", expression),
    ];
    let errors = check_program(&input).expect_err("a type alias is not a value");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message,
        format!("type alias `{name}` used as a value")
    );
}

#[test]
fn retired_r267_accepts_two_module_updates() {
    let mut input = [
        SourceFile::entry(
            "main.ts",
            include_str!("../../corpus/accept/a293-module-only-update/main.ts"),
        ),
        SourceFile::new(
            "lib.ts",
            include_str!("../../corpus/accept/a293-module-only-update/lib.ts"),
        ),
    ];
    input[0].entry = true;
    let module = check_program(&input).expect("module exports do not collide");
    assert_eq!(
        module
            .host_entries
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        ["main", "update"]
    );
    input[0].source.push_str("\nexport { main as update };\n");
    let errors = check_program(&input).expect_err("entry export names do collide");
    assert_eq!(errors[0].code, subscript_compiler::RuleCode::S017);
}

#[test]
fn lambda_ids_are_unique_across_generic_instances() {
    fn ids(
        expression: &subscript_compiler::hir::Expr,
        out: &mut Vec<(subscript_compiler::hir::LambdaId, subscript_compiler::Pos)>,
    ) {
        if let subscript_compiler::hir::ExprKind::Lambda { id, .. } = expression.kind {
            out.push((id, expression.pos.clone()));
        }
        for child in expression.children() {
            match child {
                subscript_compiler::hir::HirChild::Expr(expression) => ids(expression, out),
                subscript_compiler::hir::HirChild::Stmt(statement) => {
                    statements(std::slice::from_ref(statement), out)
                }
            }
        }
    }
    fn statements(
        body: &[subscript_compiler::hir::Stmt],
        out: &mut Vec<(subscript_compiler::hir::LambdaId, subscript_compiler::Pos)>,
    ) {
        for statement in body {
            for child in statement.children() {
                match child {
                    subscript_compiler::hir::HirChild::Expr(expression) => ids(expression, out),
                    subscript_compiler::hir::HirChild::Stmt(statement) => {
                        statements(std::slice::from_ref(statement), out)
                    }
                }
            }
        }
    }
    let module = check_program(&[SourceFile::entry("main.ts", "function apply<T>(value: T): T { const copy: T = value; const cb: () => T = (): T => copy; return cb(); } const cb: () => i32 = (): i32 => 1; export function main(): void { print(`${apply<i32>(cb())} ${apply<string>('s')}`); }")]).expect("generic lambdas");
    let mut found = Vec::new();
    for global in &module.globals {
        ids(&global.init, &mut found);
    }
    for function in &module.functions {
        statements(&function.body, &mut found);
    }
    assert_eq!(found.len(), 3, "{found:?}");
    let unique = found
        .iter()
        .map(|(id, _)| *id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(unique.len(), found.len(), "{found:?}");
    assert!(
        found
            .iter()
            .enumerate()
            .any(|(i, (_, pos))| found[i + 1..].iter().any(|(_, other)| other == pos)),
        "the generic instances must share a source position"
    );
}
