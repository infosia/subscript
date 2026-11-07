//! A function type takes a nullable handle variance (compiler.md §235).
//!
//! Cost: each test runs the checker on short programs, with no code
//! generation and no external process. The whole file took 0.03 s in a
//! debug build.

use subscript_compiler::{check_program, hir, ClassId, Diagnostic, RuleCode, SourceFile, Type};

const DECLARATIONS: &str = r#"
class C { v: i32 = 1; }
class Box<T> { v: T; constructor(v: T) { this.v = v; } }
"#;

fn check(body: &str) -> Result<(), Vec<Diagnostic>> {
    let source = format!(
        "{DECLARATIONS}export function main(): void {{\nconst c: C = new C();\n{body}\n}}\n"
    );
    check_program(&[SourceFile::new("test.ts", source.as_str())]).map(|_| ())
}

fn accepts(body: &str) {
    if let Err(errors) = check(body) {
        panic!("{body}: {errors:?}");
    }
}

fn rejects(body: &str, message: &str) {
    let errors = check(body).expect_err(body);
    assert_eq!(errors.len(), 1, "{body}: {errors:?}");
    assert_eq!(errors[0].code, RuleCode::S100, "{body}: {errors:?}");
    assert_eq!(errors[0].message, message, "{body}");
}

/// Rules 2 and 3: a result fits `T | null` for each handle type, and a
/// parameter of the target type fits a `T | null` parameter.
#[test]
fn a_function_type_takes_a_nullable_handle_variance() {
    for body in [
        // The identical type, the control of rule 2.
        "const f: (x: C) => C = (x: C): C => x;",
        "const f: (x: C) => C | null = (x: C): C => x;",
        "const f: (x: C) => i32 = (x: C | null): i32 => 1;",
        r#"const f: () => string | null = (): string => "a";"#,
        "const f: () => i32[] | null = (): i32[] => [1];",
        "const f: () => Map<i32, i32> | null = (): Map<i32, i32> => new Map<i32, i32>();",
        "const f: () => Set<i32> | null = (): Set<i32> => new Set<i32>();",
        // Rule 3 applies at each nested function type.
        "const f: (g: (x: C) => C) => void = (g: (x: C) => C | null): void => {};",
        "const f: () => (x: C) => C | null = (): ((x: C) => C) => (x: C): C => x;",
        // Rule 1: a function type is a handle type.
        "const f: () => (() => i32) | null = (): (() => i32) => (): i32 => 1;",
        // Rule 4: a built-in callback.
        "const last: C | null = [c].reduce((acc: C | null, x: C): C => x, null);",
    ] {
        accepts(body);
    }
}

/// Rule 6: each other difference stays rejected with its present text.
#[test]
fn each_other_function_type_difference_stays_rejected() {
    for (body, message) in [
        // An integer width.
        (
            "const f: (x: i64) => i32 = (x: i32): i32 => 1;",
            "type mismatch: the initializer expects `(i64) => i32`, got `(i32) => i32`",
        ),
        // The wrong direction of a parameter.
        (
            "const f: (x: C | null) => i32 = (x: C): i32 => 1;",
            "type mismatch: the initializer expects `(C | null) => i32`, got `(C) => i32`",
        ),
        // The wrong direction of a result.
        (
            "const f: (x: C) => C = (x: C): C | null => x;",
            "type mismatch: the initializer expects `(C) => C`, got `(C) => C | null`",
        ),
        // A type argument.
        (
            "const f: () => Box<C | null> = (): Box<C> => new Box<C>(c);",
            "type mismatch: the initializer expects `() => Box<C | null>`, got `() => Box<C>`",
        ),
        // An array element.
        (
            "const f: () => (C | null)[] = (): C[] => [c];",
            "type mismatch: the initializer expects `() => (C | null)[]`, got `() => C[]`",
        ),
    ] {
        rejects(body, message);
    }
    // `C[]` to `(C | null)[]` stays rejected outside a function type too.
    let errors = check("const xs: C[] = [c]; const ys: (C | null)[] = xs;").expect_err("array");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

fn casts<'a>(expr: &'a hir::Expr, out: &mut Vec<&'a hir::Expr>) {
    if let hir::ExprKind::Cast(inner) = &expr.kind {
        if inner.ty.function_type().is_some() {
            out.push(expr);
        }
    }
    for child in expr.children() {
        walk(child, out);
    }
}

fn walk<'a>(child: hir::HirChild<'a>, out: &mut Vec<&'a hir::Expr>) {
    match child {
        hir::HirChild::Expr(expr) => casts(expr, out),
        hir::HirChild::Stmt(stmt) => {
            for child in stmt.children() {
                walk(child, out);
            }
        }
    }
}

/// Rule 5a: at each place of acceptance item 2, the HIR converts the
/// function value to the type of the place, so each later stage reads
/// that type.
#[test]
fn each_place_converts_the_function_value_to_its_type() {
    let source = format!(
        "{DECLARATIONS}function id(x: C): C {{ return x; }}\n\
         function pick(x: C | null): boolean {{ return x !== null; }}\n\
         export function main(): void {{\n\
         const cs: C[] = [new C()];\n\
         const a: i32[] = cs.map((x: C | null): i32 => 1);\n\
         const b: C[] = cs.filter((x: C | null): boolean => true);\n\
         const d: i32 = cs.findIndex(pick);\n\
         const e: i32 = cs.reduce((acc: i32, x: C | null): i32 => acc, 0);\n\
         const m: Map<i32, C> = new Map<i32, C>();\n\
         m.forEach((value: C | null, key: i32): void => {{}});\n\
         const fs: ((x: C) => C | null)[] = [];\n\
         fs.push(id);\n\
         const t: Map<i32, (x: C) => C | null> = new Map<i32, (x: C) => C | null>();\n\
         t.set(1, id);\n\
         }}\n"
    );
    let module = check_program(&[SourceFile::new("test.ts", source.as_str())])
        .unwrap_or_else(|errors| panic!("{errors:?}"));
    let class = module
        .classes
        .iter()
        .position(|class| class.name == "C")
        .expect("class C");
    let c = Type::Class(ClassId(class));
    let nullable = Type::nullable(c.clone());
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let mut found = Vec::new();
    for stmt in &main.body {
        walk(hir::HirChild::Stmt(stmt), &mut found);
    }
    let types: Vec<Type> = found.iter().map(|cast| cast.ty.clone()).collect();
    for place in [
        Type::func(vec![c.clone()], Type::I32),
        Type::func(vec![c.clone()], Type::Bool),
        Type::func(vec![Type::I32, c.clone()], Type::I32),
        Type::func(vec![c.clone(), Type::I32], Type::Void),
        Type::func(vec![c.clone()], nullable.clone()),
    ] {
        assert!(types.contains(&place), "{place:?} in {types:?}");
    }
    // map, filter, findIndex, reduce, forEach, push, and set.
    assert_eq!(types.len(), 7, "{types:?}");
}

const NULLABLE: &str = r#"
function id(x: C): C { return x; }
function maybeId(b: boolean): ((x: C) => C) | null { return b ? id : null; }
function apply(f: (x: C) => C | null): i32 { return 0; }
class H { f: (x: C) => C | null = id; }
function getNull(): ((x: C) => C | null) | null { return null; }
type F = (x: C) => C;
"#;

fn program(body: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    let source = format!(
        "{DECLARATIONS}{NULLABLE}export function main(): void {{\nconst h: H = new H();\n{body}\n}}\n"
    );
    check_program(&[SourceFile::new("test.ts", source.as_str())])
}

fn main_casts(module: &hir::Module) -> Vec<Type> {
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main");
    let mut found = Vec::new();
    for stmt in &main.body {
        walk(hir::HirChild::Stmt(stmt), &mut found);
    }
    found.iter().map(|cast| cast.ty.clone()).collect()
}

/// Rule 5b: a value of the type `((x: C) => C) | null` is not assignable
/// to a `(x: C) => C | null` place, as `tsc` rejects it (TS2322, TS2345),
/// so it gets no conversion and keeps its diagnostic at each place.
#[test]
fn a_nullable_function_value_does_not_fit_a_function_place() {
    let got = "`(C) => C | null`, got `((C) => C) | null`";
    for (body, what) in [
        (
            "const m: ((x: C) => C) | null = maybeId(false); const f: (x: C) => C | null = m;",
            "the initializer",
        ),
        ("apply(maybeId(false));", "the argument"),
        (
            "const fs: ((x: C) => C | null)[] = []; fs.push(maybeId(false));",
            "the argument",
        ),
        ("h.f = maybeId(false);", "the assignment"),
        (
            "const m: Map<i32, (x: C) => C | null> = new Map<i32, (x: C) => C | null>(); m.set(1, maybeId(false));",
            "the argument",
        ),
        (
            "const fs: ((x: C) => C | null)[] = [maybeId(false)];",
            "the array element",
        ),
    ] {
        let errors = program(body).expect_err(body);
        assert_eq!(errors.len(), 1, "{body}: {errors:?}");
        assert_eq!(
            errors[0].message,
            format!("type mismatch: {what} expects {got}"),
            "{body}"
        );
    }
}

/// Rules 2 and 5a: a nullable function value fits a nullable function
/// place, and `??=` converts it to the type of its place. `||=` keeps the
/// diagnostic of a non-boolean operand, as for the identical type.
#[test]
fn a_nullable_function_pair_fits_and_converts() {
    let module = program(
        "let v: ((x: C) => C | null) | null = getNull(); v ??= maybeId(true); \
         const w: ((x: C) => C | null) | null = maybeId(true);",
    )
    .unwrap_or_else(|errors| panic!("{errors:?}"));
    let place = Type::nullable(Type::func(
        vec![class_c(&module)],
        Type::nullable(class_c(&module)),
    ));
    assert_eq!(main_casts(&module), vec![place.clone(), place]);
    for body in [
        "let v: ((x: C) => C | null) | null = getNull(); v ||= maybeId(true);",
        "let v: ((x: C) => C | null) | null = getNull(); v ||= getNull();",
    ] {
        let errors = program(body).expect_err(body);
        assert_eq!(
            errors[0].message, "`||=` requires a boolean operand, got `((C) => C | null) | null`",
            "{body}"
        );
    }
}

/// Rule 7: an alias of the identical function type gets no conversion.
#[test]
fn an_alias_of_the_identical_type_gets_no_conversion() {
    let module = program("const f: F = id; const g: (x: C) => C = f; const k: F = g;")
        .unwrap_or_else(|errors| panic!("{errors:?}"));
    assert_eq!(main_casts(&module), Vec::<Type>::new());
}

fn class_c(module: &hir::Module) -> Type {
    let class = module
        .classes
        .iter()
        .position(|class| class.name == "C")
        .expect("class C");
    Type::Class(ClassId(class))
}

/// Rule 5a: the right operand of `??` takes the type of its place, and the
/// result has that type.
#[test]
fn the_right_operand_of_nullish_converts() {
    let module = program("const w: (x: C) => C | null = getNull() ?? id;")
        .unwrap_or_else(|errors| panic!("{errors:?}"));
    let place = Type::func(vec![class_c(&module)], Type::nullable(class_c(&module)));
    assert_eq!(main_casts(&module), vec![place]);
}
