//! §109.2 rule 1: the checker visits each syntax node one time, so a
//! chain of `n` operators costs O(n).
//!
//! A walk that recurses into a child and then falls through to its own
//! child loop visits that child twice, which costs `2^n` on a chain. The
//! bound below is a wall clock: an exponential walk cannot meet it at
//! depth 200, whatever the host.

use std::time::{Duration, Instant};

use subscript_compiler::{
    check_program, check_warnings, hir, on_the_compile_thread, parse_import_specifiers, Profile,
    SourceFile,
};

/// The chain length every test here builds.
const DEPTH: usize = 200;

/// The wall-clock bound one chain must meet on a debug build.
///
/// The measured times on this project's arm64 host are 5.5 ms for the
/// `!` chain and 5.8 ms for the `-` chain, so the bound holds a margin
/// over 30x. The shape it rejects is `2^200`, not a slow host.
const BOUND: Duration = Duration::from_millis(200);

/// One chain of `count` copies of `operator` over `leaf`, typed `ty`.
fn chain(ty: &str, operator: &str, count: usize, leaf: &str) -> String {
    format!(
        "export function main(): void {{\n  const value: {ty} = {}{leaf};\n  print(`${{value}}`);\n}}\n",
        operator.repeat(count)
    )
}

/// Every `Unary` node under one expression, including the expression.
fn unary_nodes(expr: &hir::Expr) -> usize {
    let here = usize::from(matches!(expr.kind, hir::ExprKind::Unary { .. }));
    here + expr
        .children()
        .into_iter()
        .map(|child| match child {
            hir::HirChild::Expr(child) => unary_nodes(child),
            hir::HirChild::Stmt(_) => 0,
        })
        .sum::<usize>()
}

/// Every `Unary` node in one checked module.
fn unary_nodes_in(module: &hir::Module) -> usize {
    module
        .functions
        .iter()
        .flat_map(|function| function.body.iter())
        .flat_map(hir::Stmt::children)
        .map(|child| match child {
            hir::HirChild::Expr(expr) => unary_nodes(expr),
            hir::HirChild::Stmt(_) => 0,
        })
        .sum()
}

/// Checks `source`, runs the warning walk, and answers the time both
/// took with the number of `Unary` nodes the checker built.
///
/// The warning walk is part of the cost, because it walks the same tree.
/// The node count is the firing control: it proves the chain reached the
/// checker, so a fast run cannot be an empty one.
fn check_and_warn(name: &str, source: String) -> (Duration, usize) {
    on_the_compile_thread(move || {
        let files = [SourceFile::new(name, source)];
        let started = Instant::now();
        let module = check_program(&files).expect("the chain checks clean");
        let warnings = check_warnings(&module);
        let elapsed = started.elapsed();
        assert!(warnings.is_empty(), "{warnings:?}");
        (elapsed, unary_nodes_in(&module))
    })
}

#[test]
fn a_two_hundred_deep_not_chain_checks_in_linear_time() {
    let (elapsed, nodes) = check_and_warn("not-chain.ts", chain("boolean", "!", DEPTH, "true"));
    println!("{DEPTH}-deep `!` chain: {elapsed:?}, {nodes} unary nodes");
    assert_eq!(nodes, DEPTH, "the checker must build one node per operator");
    assert!(elapsed < BOUND, "{DEPTH}-deep `!` chain took {elapsed:?}");
}

#[test]
fn a_two_hundred_deep_negation_chain_checks_in_linear_time() {
    let (elapsed, nodes) = check_and_warn("neg-chain.ts", chain("i32", "- ", DEPTH, "1"));
    println!("{DEPTH}-deep `-` chain: {elapsed:?}, {nodes} unary nodes");
    // The innermost `- 1` folds into the literal (C4), so the chain
    // builds one node less than it spells.
    assert_eq!(
        nodes,
        DEPTH - 1,
        "the checker must build one node per operator"
    );
    assert!(elapsed < BOUND, "{DEPTH}-deep `-` chain took {elapsed:?}");
}

/// §109.2 rule 3: the whole compile runs on the compile thread, whose
/// stack the compiler sizes. The default profile keeps no nesting limit,
/// so a chain this deep reaches the parser and the checker.
///
/// Measured with the release CLI: `subscript check` answers "no errors"
/// in 0.13 s on this source and does not abort.
#[test]
fn a_forty_thousand_level_conditional_chain_returns_under_the_default_profile() {
    let source = format!(
        "export function main(): void {{\n  const value: i32 = {}1;\n  print(`${{value}}`);\n}}\n",
        "true ? 1 : ".repeat(40_000)
    );
    let bytes = source.len();
    let started = Instant::now();
    let checked = on_the_compile_thread(move || {
        check_program(&[SourceFile::new("deep-cond.ts", source)])
            .map(|module| module.functions.len())
    });
    println!(
        "40,000-level `? :` chain: {bytes} bytes, {:?}, {checked:?}",
        started.elapsed()
    );
    assert_eq!(checked, Ok(1), "the chain must check clean");
}

/// §109.2 rule 3: `check_program_with` spawns the compile thread itself,
/// so a host that embeds this crate gets the stack bound from the API.
///
/// The source nests 2,000 parentheses, and the caller is a 2 MiB
/// thread, the size of an ordinary test thread. The call carries no
/// wrapper: a body that runs on this thread overflows its stack and
/// aborts the process.
#[test]
fn a_bare_check_returns_from_a_two_mebibyte_caller_thread() {
    let source = format!(
        "export function main(): void {{\n  const value: i32 = {}7{};\n  print(`${{value}}`);\n}}\n",
        "(".repeat(2_000),
        ")".repeat(2_000)
    );
    let checked = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            check_program(&[SourceFile::new("deep.ts", source)])
                .map(|module| module.functions.len())
        })
        .expect("spawn the 2 MiB caller thread")
        .join()
        .expect("the check returns from a 2 MiB caller thread");
    assert_eq!(checked, Ok(1), "the deep source must check clean");
    println!("2,000 nested parentheses: a bare check returns from a 2 MiB caller thread");
}

/// §109.2 rule 3: `parse_import_specifiers` spawns the compile thread
/// itself, so every public entry that parses gets the stack bound from
/// the API.
///
/// The source nests 40,000 type arguments in 120,041 bytes, under the
/// S026 byte limit, so the byte check admits it and the parser runs the
/// whole nest. The caller is a 2 MiB thread and the call carries no
/// wrapper: a parse that ran on this thread would overflow its stack and
/// abort the process.
#[test]
fn a_bare_import_scan_returns_from_a_two_mebibyte_caller_thread() {
    let source = format!("let d: {}i32{};\n", "A<".repeat(40_000), ">".repeat(40_000));
    let bytes = source.len();
    let started = Instant::now();
    let scanned = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            parse_import_specifiers(
                &SourceFile::new("deep-imports.ts", source),
                Profile::Sandbox,
            )
        })
        .expect("spawn the 2 MiB caller thread")
        .join()
        .expect("the import scan returns from a 2 MiB caller thread");
    println!(
        "40,000 nested type arguments: {bytes} bytes, {:?}, {scanned:?}",
        started.elapsed()
    );
    assert_eq!(scanned, Ok(Vec::new()), "the deep source holds no import");
}
