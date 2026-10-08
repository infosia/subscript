use super::*;
use subscript_compiler::{check_program, SourceFile};

fn checked(source: &str) -> (hir::Module, l::Module) {
    let hir =
        check_program(&[SourceFile::new("sequence.ts", source)]).expect("sequence witness checks");
    let lir = subscript_codegen::lir::lower_module(&hir)
        .unwrap_or_else(|error| panic!("sequence witness lowers: {error:?}\n{source}"));
    (hir, lir)
}

#[test]
fn statement_exits_preserve_execution_facts() {
    for body in [
        "{ return; }",
        "switch (0) { case 0: return; default: return; }",
        "switch (0) { default: { return; } case 0: if (stop) { return; } else { return; } }",
        "switch (0) { case 0: return; } print(\"after\");",
        "switch (0) { case 0: return; default: print(\"default\"); } print(\"after\");",
        "switch (0) { case 0: break; default: return; } print(\"after\");",
        "switch (0) { case 0: if (stop) { break; } return; default: return; } print(\"after\");",
        "switch (0) { case 0: default: return; }",
        "switch (0) { case 0: switch (1) { default: break; } return; default: return; }",
        "if (stop) { return; } else { { return; } }",
        "if (stop) { { return; } } print(\"fallthrough\");",
        "if (stop) { return; } else { print(\"else\"); } print(\"after\");",
        "{ print(\"block\"); } print(\"after\");",
    ] {
        let source = format!(
            "class R {{ [Symbol.dispose](): void {{}} }}
             function run(stop: boolean): void {{
               using resource: R | null = new R();
               {body}
             }}
             export function main(): void {{ run(true); run(false); }}"
        );
        let (hir, mut lir) = checked(&source);
        assert_eq!(dropped_facts(&hir, &lir), Vec::<String>::new(), "{body}");

        // Delete a reachable call, not its trap metadata. The check must still fail.
        let block = lir
            .functions
            .iter_mut()
            .flat_map(|function| &mut function.blocks)
            .find(|block| {
                block.instructions.iter().any(|instruction| {
                    matches!(instruction.kind, l::InstructionKind::Call(_))
                        && instruction
                            .traps
                            .iter()
                            .any(|trap| matches!(trap.kind, l::TrapKind::Call))
                })
            })
            .expect("reachable call block");
        let index = block
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction.kind, l::InstructionKind::Call(_))
                    && instruction
                        .traps
                        .iter()
                        .any(|trap| matches!(trap.kind, l::TrapKind::Call))
            })
            .expect("reachable call");
        block.instructions.remove(index);
        assert!(
            dropped_facts(&hir, &lir)
                .iter()
                .any(|finding| { finding.contains("trap \"Call\" carries") }),
            "the missing reachable call must fail: {body}"
        );
    }
}

#[test]
fn a_conditionless_for_drops_its_trailing_execution_facts() {
    let source = "function run(a: i32[], flag: boolean): i32 {
                    if (flag) { for (;;) { return 1; } } return a[0];
                  }
                  export function main(): void {}";
    let (hir, lir) = checked(source);
    assert!(dropped_facts(&hir, &lir).is_empty());
    let function = hir.functions.iter().find(|f| f.name == "run").unwrap();
    let hir::Stmt::If {
        then,
        cond: _,
        els: _,
        pos: _,
    } = &function.body[0]
    else {
        panic!("the if");
    };
    assert!(stops_statement_sequence(&hir, &then[0]));
    let reads = lir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| {
            instruction
                .traps
                .iter()
                .any(|trap| matches!(trap.kind, l::TrapKind::IndexRead))
        })
        .count();
    assert_eq!(
        reads, 1,
        "the read after the `if` stays; nothing follows the loop"
    );
    let (hir, lir) = checked(
        "function run(a: i32[]): i32 { for (;;) { return 1; } return a[0]; }
         export function main(): void {}",
    );
    assert!(dropped_facts(&hir, &lir).is_empty());
    let reads = lir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| {
            instruction
                .traps
                .iter()
                .any(|trap| matches!(trap.kind, l::TrapKind::IndexRead))
        })
        .count();
    assert_eq!(
        reads, 0,
        "control cannot leave the loop (compiler.md §101 rule 2)"
    );
}

#[test]
fn loop_conditions_select_trailing_execution_facts() {
    for (body, stops) in [
        ("while (true) { return; }", true),
        ("for (; true;) { return; }", true),
        ("while (a.length > 0) { return; }", false),
        ("for (; a.length > 0;) { return; }", false),
    ] {
        let source = format!(
            "function run(a: i32[]): i32 {{ {body} return a[0]; }}
             export function main(): void {{}}"
        )
        .replace("{ return; }", "{ return 1; }");
        let (hir, mut lir) = checked(&source);
        assert!(dropped_facts(&hir, &lir).is_empty(), "{body}");
        let function = hir.functions.iter().find(|f| f.name == "run").unwrap();
        assert_eq!(
            stops_statement_sequence(&hir, &function.body[0]),
            stops,
            "{body}"
        );
        let mut removed = 0;
        for block in lir.functions.iter_mut().flat_map(|f| &mut f.blocks) {
            block.instructions.retain(|instruction| {
                let keep = !instruction
                    .traps
                    .iter()
                    .any(|trap| matches!(trap.kind, l::TrapKind::IndexRead));
                if !keep {
                    removed += 1;
                }
                keep
            });
        }
        assert_eq!(removed > 0, !stops, "{body}");
        assert_eq!(dropped_facts(&hir, &lir).is_empty(), stops, "{body}");
    }
}

#[test]
fn blocks_preserve_break_and_continue_sequence_exits() {
    let (hir, lir) = checked(
        "export function main(): void {
           for (let i: i32 = 0; i < 2; i += 1) {
             if (i === 0) { { continue; } } else { { break; } }
           }
           print(\"after\");
         }",
    );
    assert!(dropped_facts(&hir, &lir).is_empty());
    let pos = hir
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main")
        .pos
        .clone();
    assert!(stops_statement_sequence(
        &hir,
        &hir::Stmt::Block(vec![hir::Stmt::Break(pos.clone())])
    ));
    assert!(stops_statement_sequence(
        &hir,
        &hir::Stmt::Block(vec![hir::Stmt::Continue(pos)])
    ));
    assert!(!stops_statement_sequence(
        &hir,
        &hir::Stmt::Block(Vec::new())
    ));
}
