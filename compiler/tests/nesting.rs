//! `specs/blocks/compiler.md` §113.2 rule 2: the checker visits each
//! syntax node one time, so a chain of `n` operators costs O(n).
//!
//! A walk that recurses into a child and then falls through to its own
//! child loop visits that child twice, which costs `2^n` on a chain. The
//! bound below is a wall clock: an exponential walk cannot meet it at
//! depth 200, whatever the host.
//!
//! Core principle 15: the cost of these two tests is the stack. Each one
//! runs on the thread that libtest gives it, which holds 2,097,152 bytes
//! in every build of Rust. One `check_program` and one `check_warnings`
//! at depth 200 need over 917,504 and under 950,272 bytes unoptimized.
//! The same pair needs over 262,144 and under 524,288 bytes optimized.
//! The margins over the libtest stack are therefore 2.2 and 4.0. Method,
//! on the x86-64 Linux host:
//! `RUST_MIN_STACK=<n> target/<profile>/deps/nesting-* --test-threads=1`.
//! A stack overflow ends the test binary, so the gate reads the loss as
//! a count that fell, not as one named failure. The margin is the guard
//! (`specs/blocks/compiler.md` §114.2 rule 3).

use std::time::{Duration, Instant};

use subscript_compiler::{check_program, check_warnings, hir, SourceFile};

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
    let files = [SourceFile::new(name, source)];
    let started = Instant::now();
    let module = check_program(&files).expect("the chain checks clean");
    let warnings = check_warnings(&module);
    let elapsed = started.elapsed();
    assert!(warnings.is_empty(), "{warnings:?}");
    (elapsed, unary_nodes_in(&module))
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
