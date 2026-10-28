//! A static method and a local function are function values
//! (compiler.md §207).
//!
//! Cost: eighteen checker calls on one-file or two-file programs, with no
//! code generation and no external process.

use super::*;
use crate::divergence::Divergence;

fn check(src: &str) -> Result<hir::Module, Vec<Diagnostic>> {
    check_program(&[SourceFile::entry("main.ts", src)])
}

fn only(src: &str) -> Diagnostic {
    let diagnostics = check(src).expect_err(src);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    diagnostics.into_iter().next().unwrap()
}

fn symbols(module: &hir::Module, name: &str) -> Vec<String> {
    module
        .functions
        .iter()
        .filter(|function| function.name == name)
        .map(|function| function.symbol.full_text().to_owned())
        .collect()
}

const TWO_LOCALS: &str = "function sq(x: i32): i32 { return x + 1; }\n\
     function a(): i32 { function sq(x: i32): i32 { return x * x; } return sq(2); }\n\
     function b(): i32 { function sq(x: i32): i32 { return x * 3; } return sq(2); }\n\
     export function main(): void { print(`${sq(1)} ${a()} ${b()}`); }\n";

/// Rule 3: two local functions with one name in two functions give two
/// C names, and neither equals the module function with that name.
#[test]
fn two_local_functions_with_one_name_have_two_names() {
    let module = check(TWO_LOCALS).unwrap();
    let names = symbols(&module, "sq");
    assert_eq!(names.len(), 3, "{names:?}");
    let module_name = module
        .functions
        .iter()
        .find(|function| function.name == "sq" && function.pos.line == 1)
        .map(|function| function.symbol.full_text().to_owned())
        .unwrap();
    let locals: Vec<&String> = names.iter().filter(|n| **n != module_name).collect();
    assert_eq!(locals.len(), 2, "{names:?}");
    assert_ne!(locals[0], locals[1]);
    for local in locals {
        assert_eq!(hir::source_name(local), "sq");
        assert!(local.contains("[[identity:local:"), "{local}");
    }
    // The name derives from the enclosing function: a declaration added
    // before `a` leaves the names unchanged.
    let shifted = check(&format!(
        "function extra(): i32 {{ function sq(x: i32): i32 {{ return x; }} return sq(1); }}\n{TWO_LOCALS}"
    ))
    .unwrap();
    for name in &names {
        assert!(symbols(&shifted, "sq").contains(name), "{name}");
    }
}

/// Rule 2: a local function is visible in its whole block, calls itself,
/// and calls another local function of its block.
#[test]
fn a_local_function_is_hoisted_and_recursive() {
    let module = check(
        "export function main(): void {\n\
           print(`${isEven(4)} ${fact(4)}`);\n\
           function fact(x: i32): i32 { return x <= 1 ? 1 : x * fact(x - 1); }\n\
           function isEven(x: i32): boolean { return x === 0 ? true : isOdd(x - 1); }\n\
           function isOdd(x: i32): boolean { return x === 0 ? false : isEven(x - 1); }\n\
         }\n",
    )
    .unwrap();
    for name in ["fact", "isEven", "isOdd"] {
        assert_eq!(symbols(&module, name).len(), 1, "{name}");
    }
}

/// Rule 2: a local function shadows a namespace import and an ambient
/// function in its block.
#[test]
fn a_local_function_shadows_an_import_and_an_ambient_function() {
    let files = [
        SourceFile::entry(
            "main.ts",
            "import * as ns from \"./lib\";\n\
             export function main(): void { function ns(): void {} ns(); }\n",
        ),
        SourceFile::new("lib.ts", "export function f(): void {}\n"),
    ];
    assert!(check_program(&files).is_ok());
    assert!(check(
        "export function main(): void {\n\
           function encodeURI<T>(x: T): string { return \"x\"; }\n\
           encodeURI<string>(\"x\");\n\
         }\n"
    )
    .is_ok());
    // Control: the ambient function is not generic.
    let diagnostic = only("export function main(): void { encodeURI<string>(\"x\"); }\n");
    assert_eq!(diagnostic.message, "`encodeURI` is not generic");
}

fn read(src: &str) -> Diagnostic {
    let diagnostic = only(src);
    assert_eq!(diagnostic.code, RuleCode::S100);
    assert_eq!(
        diagnostic.divergence,
        Some(Divergence::LocalFunctionReadsEnclosingBinding)
    );
    diagnostic
}

/// Rule 4: a read of a local, a parameter, a type parameter, or `this`
/// keeps the function in its block. The text names the first read.
#[test]
fn a_local_function_that_reads_an_enclosing_binding_is_rejected() {
    let local = read(
        "export function main(): void {\n\
           const k: i32 = 1;\n\
           const j: i32 = 2;\n\
           function f(): i32 { return j + k; }\n\
         }\n",
    );
    assert_eq!(
        local.message,
        "local function `f` reads the local `j` of its enclosing function"
    );
    assert_eq!((local.pos.line, local.pos.col), (4, 28));
    let parameter = read("function g(p: i32): void { function f(): i32 { return p; } }\n");
    assert_eq!(
        parameter.message,
        "local function `f` reads the parameter `p` of its enclosing function"
    );
    let type_parameter = read("function g<T>(x: T): void { function f(y: T): void {} }\n");
    assert_eq!(
        type_parameter.message,
        "local function `f` reads the type parameter `T` of its enclosing function"
    );
    let this = read("class C { v: i32 = 0; m(): void { function f(): i32 { return this.v; } } }\n");
    assert_eq!(
        this.message,
        "local function `f` reads `this` of its enclosing function"
    );
    // A read in a lambda or a nested local function is a read of the
    // enclosing local function; the nested function gets no diagnostic.
    let nested = read(
        "export function main(): void {\n\
           const k: i32 = 1;\n\
           function f(): i32 { function g(): i32 { return k; } return g(); }\n\
         }\n",
    );
    assert_eq!(
        nested.message,
        "local function `f` reads the local `k` of its enclosing function"
    );
    // Control: a parameter, a local, and a module variable of its own
    // are not reads of an enclosing binding.
    assert!(check(
        "let m: i32 = 3;\n\
         function g(p: i32): i32 {\n\
           function f(p: i32): i32 { const k: i32 = p; return k + m; }\n\
           return f(p);\n\
         }\n"
    )
    .is_ok());
}

/// Rule 4: a call to a local function that stays in its block is a read.
#[test]
fn a_call_to_a_rejected_local_function_is_a_read() {
    let diagnostics = check(
        "function g(p: i32): i32 {\n\
           function f(): i32 { return p; }\n\
           function h(): i32 { return f(); }\n\
           return h();\n\
         }\n",
    )
    .unwrap_err();
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "local function `f` reads the parameter `p` of its enclosing function",
            "local function `h` reads the local function `f` of its enclosing function",
        ]
    );
}

/// Rule 5: a local class stays rejected with the existing text.
#[test]
fn a_local_class_keeps_its_text() {
    let diagnostic = only("export function main(): void { class C {} }\n");
    assert_eq!(
        diagnostic.message,
        "nested declarations are not in the decided surface"
    );
}

/// Rule 1: a static method is a function value; a static generator
/// method and an assignment target keep their text.
#[test]
fn a_static_method_is_a_function_value() {
    let module = check(
        "class M { static twice(x: i32): i32 { return x * 2; } }\n\
         export function main(): void { const f = M.twice; print(`${f(2)}`); }\n",
    )
    .unwrap();
    assert!(module.functions.iter().any(|f| f.name == "M.twice"));
    let diagnostic = only(
        "class C { static *g(): Generator<i32> { yield 1; } }\n\
         export function main(): void { const f = C.g; }\n",
    );
    assert_eq!(diagnostic.message, "static method `C.g` may only be called");
    assert_eq!(
        diagnostic.divergence,
        Some(Divergence::StaticMethodValueForm)
    );
    // An assignment to a static method keeps the text.
    let write = only(
        "class M { static twice(x: i32): i32 { return x * 2; } }\n\
         export function main(): void { M.twice = M.twice; }\n",
    );
    assert_eq!(write.message, "static method `M.twice` may only be called");
}

/// A diagnostic in a local function names it by its source name.
#[test]
fn a_diagnostic_names_a_local_function_by_its_source_name() {
    let diagnostic =
        only("export function main(): void { function f(x: i32): i32 { return x; } f<i32>(1); }\n");
    assert_eq!(diagnostic.message, "`f` is not generic");
}
