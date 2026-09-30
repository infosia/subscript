//! A logical operand sees the narrowing of the operand before it
//! (compiler.md §133).

use subscript_compiler::{check_program, divergence::Divergence, RuleCode, SourceFile};

type Listed = (RuleCode, u32, u32, String, Option<Divergence>);

const CLASSES: &str = "class Cell { v: i32 = 7; }
class Holder { c: Cell | null = new Cell(); }
function clear(h: Holder): boolean { h.c = null; return true; }
async function tick(h: Holder): Promise<boolean> { h.c = null; return true; }
";

/// Checks `CLASSES` followed by `body` and lists every diagnostic.
fn diagnostics(body: &str) -> Vec<Listed> {
    let source = format!("{CLASSES}{body}\n");
    match check_program(&[SourceFile::new("test.ts", source)]) {
        Ok(_) => Vec::new(),
        Err(found) => found
            .into_iter()
            .map(|d| (d.code, d.pos.line, d.pos.col, d.message, d.divergence))
            .collect(),
    }
}

fn may_be_null(ty: &str, line: u32, col: u32, divergence: Option<Divergence>) -> Listed {
    (
        RuleCode::S011,
        line,
        col,
        format!("`{ty}` may be null here; narrow with a null check first"),
        divergence,
    )
}

#[test]
fn the_right_operand_sees_the_left_facts() {
    for condition in [
        "h.c !== null && h.c.v > 0",
        "h.c === null || h.c.v > 0",
        "c !== null && c.v > 0",
        "c === null || c.v > 0",
        "c !== null && h.c !== null && c.v + h.c.v > 0",
    ] {
        let body = format!(
            "function read(h: Holder, c: Cell | null): boolean {{\n  const b: boolean = {condition};\n  return b;\n}}"
        );
        assert_eq!(diagnostics(&body), Vec::new(), "{condition}");
    }
}

#[test]
fn a_call_in_the_right_operand_ends_the_shared_narrowing() {
    for (condition, col) in [
        ("h.c !== null && clear(h) && h.c.v > 0", 52),
        ("h.c === null || !clear(h) || h.c.v > 0", 53),
    ] {
        let body = format!(
            "function read(h: Holder): boolean {{\n  const b: boolean = {condition};\n  return b;\n}}"
        );
        assert_eq!(
            diagnostics(&body),
            vec![may_be_null(
                "Cell | null",
                6,
                col,
                Some(Divergence::SharedLocationNarrowing)
            )],
            "{condition}"
        );
    }
    // Control: the read before the call keeps the narrowing.
    let body = "function read(h: Holder): boolean {\n  const b: boolean = h.c !== null && h.c.v > 0 && clear(h);\n  return b;\n}";
    assert_eq!(diagnostics(body), Vec::new());
    // Control: a local survives the call (compiler.md §124 rule 4).
    let body = "function read(h: Holder, c: Cell | null): boolean {\n  const b: boolean = c !== null && clear(h) && c.v > 0;\n  return b;\n}";
    assert_eq!(diagnostics(body), Vec::new());
}

#[test]
fn an_await_in_the_right_operand_ends_the_shared_narrowing() {
    let body = "async function read(h: Holder): Promise<boolean> {\n  const b: boolean = h.c !== null && (await tick(h)) && h.c.v > 0;\n  return b;\n}";
    assert_eq!(
        diagnostics(body),
        vec![may_be_null(
            "Cell | null",
            6,
            59,
            Some(Divergence::SharedLocationNarrowing)
        )]
    );
}

#[test]
fn the_facts_end_with_the_operand() {
    for condition in ["a !== null && true", "a === null || false"] {
        let body = format!(
            "function read(a: Cell | null): i32 {{\n  const x: boolean = {condition};\n  return x ? a.v : 0;\n}}"
        );
        assert_eq!(
            diagnostics(&body),
            vec![may_be_null("Cell | null", 7, 14, None)],
            "{condition}"
        );
    }
    // Control: a statement condition keeps the facts of the whole condition.
    let body = "function read(a: Cell | null): i32 {\n  if (a !== null && true) { return a.v; }\n  if (a === null || false) { return 0; }\n  return a.v;\n}";
    assert_eq!(diagnostics(body), Vec::new());
}

#[test]
fn a_statement_else_branch_reads_the_false_facts_of_logical_or() {
    let body = "function read(a: Cell | null, b: Cell | null): i32 {\n  if (a === null || b === null) { return 0; } else { return a.v + b.v; }\n}";
    assert_eq!(diagnostics(body), Vec::new());
    // Control: `a === null && b === null` is false with either path null.
    let body = "function read(a: Cell | null, b: Cell | null): i32 {\n  if (a === null && b === null) { return 0; } else { return a.v + b.v; }\n}";
    assert_eq!(
        diagnostics(body),
        vec![
            may_be_null("Cell | null", 6, 61, None),
            may_be_null("Cell | null", 6, 67, None)
        ]
    );
}

#[test]
fn a_call_in_the_right_operand_ends_the_statement_facts() {
    let body = "function read(h: Holder): i32 {\n  if (h.c === null || !clear(h)) { return 0; } else { return h.c.v; }\n}";
    assert_eq!(
        diagnostics(body),
        vec![may_be_null(
            "Cell | null",
            6,
            64,
            Some(Divergence::SharedLocationNarrowing)
        )]
    );
    let body = "function read(h: Holder): i32 {\n  if (h.c !== null && clear(h)) { return h.c.v; }\n  return 0;\n}";
    assert_eq!(
        diagnostics(body),
        vec![may_be_null(
            "Cell | null",
            6,
            44,
            Some(Divergence::SharedLocationNarrowing)
        )]
    );
    // Control: without the call the else branch keeps the narrowing.
    let body = "function read(h: Holder): i32 {\n  if (h.c === null || false) { return 0; } else { return h.c.v; }\n}";
    assert_eq!(diagnostics(body), Vec::new());
}
