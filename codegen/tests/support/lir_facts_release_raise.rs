use super::*;
use subscript_codegen::lir::{lower_module, lower_module_for_reload};
use subscript_compiler::{check_program, SourceFile};

#[path = "operand_raise_program.rs"]
mod fixture;

fn check(hir: &hir::Module, lir: &l::Module, reload: bool) {
    let mut findings = Vec::new();
    compare_for_reload(hir, lir, reload, &mut findings);
    assert!(findings.is_empty(), "reload={reload}: {findings:#?}");
}

fn remove_hold_release(lir: &mut l::Module, function: usize, landing: l::BlockId) {
    let instructions = &mut lir.functions[function].blocks[landing.0 as usize].instructions;
    let release = instructions
        .iter()
        .position(|i| {
            matches!(
                i.kind,
                l::InstructionKind::AsyncHandleRelease
                    | l::InstructionKind::AsyncHandleArrayRelease
            )
        })
        .expect("the exception edge releases its operand hold");
    instructions.remove(release);
}

fn require_missing_release(hir: &hir::Module, lir: &l::Module, reload: bool) {
    let mut findings = Vec::new();
    compare_for_reload(hir, lir, reload, &mut findings);
    assert!(
        findings.iter().any(
            |f| f.contains("exception edge releases 0 owned handles; lexical scopes require 1")
        ),
        "reload={reload}: {findings:#?}"
    );
}

// Cost: one check, two lowerings, and four release comparisons; under 0.02 s on the development host.
#[test]
fn a_descriptor_initializer_keeps_the_earlier_operand_until_its_raise_edge() {
    let hir = check_program(&fixture::program(10, false)).expect("problem program checks");
    for reload in [false, true] {
        let mut lir = lower_module_for_reload(&hir, reload).expect("problem program lowers");
        check(&hir, &lir, reload);
        let main = lir
            .functions
            .iter()
            .position(|f| f.source_name == "main")
            .unwrap();
        let initializer_pos = &hir.classes.iter().find(|c| c.name == "D").unwrap().fields[0]
            .init
            .as_ref()
            .unwrap()
            .pos;
        let landing = lir.functions[main]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match (&i.kind, i.raise_edge()) {
                (l::InstructionKind::Call(_), Some(l::RaiseEdge::Handler(id)))
                    if i.pos == *initializer_pos =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .expect("the explicit descriptor initializer has a handler");
        remove_hold_release(&mut lir, main, landing);
        require_missing_release(&hir, &lir, reload);
    }
}

// Cost: two checks, four lowerings, and six release comparisons; 0.01 s on the development host.
#[test]
fn a_new_initializer_keeps_the_earlier_operand_until_its_raise_edge() {
    for immutable in [false, true] {
        let hir = check_program(&fixture::new_program(10, immutable)).expect("new program checks");
        for reload in [false, true] {
            let mut lir = lower_module_for_reload(&hir, reload).expect("new program lowers");
            check(&hir, &lir, reload);
            if immutable {
                continue;
            }
            let main = lir
                .functions
                .iter()
                .position(|f| f.source_name == "main")
                .unwrap();
            let initializer_pos = &hir.classes.iter().find(|c| c.name == "C").unwrap().fields[0]
                .init
                .as_ref()
                .unwrap()
                .pos;
            let landing = lir.functions[main]
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| match (&i.kind, i.raise_edge()) {
                    (l::InstructionKind::Call(_), Some(l::RaiseEdge::Handler(id)))
                        if i.pos == *initializer_pos =>
                    {
                        Some(*id)
                    }
                    _ => None,
                })
                .expect("the new initializer has a handler");
            remove_hold_release(&mut lir, main, landing);
            require_missing_release(&hir, &lir, reload);
        }
    }
}

// Cost: one check, two lowerings, and three release comparisons; under 0.02 s on the development host.
#[test]
fn a_reload_only_default_call_holds_the_earlier_operand() {
    let hir = check_program(&[SourceFile::new(
        "reload-default.ts",
        r#"
function* gen(): Generator<i32> { yield 1; }
function take(g: Generator<i32>, n: i32 = 1): i32 { return n; }
export function main(): void { try { take(gen()); } catch (e) {} }
"#,
    )])
    .expect("constant default checks");
    let normal = lower_module(&hir).expect("normal build");
    check(&hir, &normal, false);
    let mut reload = lower_module_for_reload(&hir, true).expect("hot reload build");
    check(&hir, &reload, true);
    let main = reload
        .functions
        .iter()
        .position(|f| f.source_name == "main")
        .unwrap();
    let landing = reload.functions[main]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match (&i.kind, i.raise_edge()) {
            (l::InstructionKind::Call(target), Some(l::RaiseEdge::Handler(id))) => {
                let l::CallTargetKind::Function(function) = target.kind else {
                    return None;
                };
                reload.functions[function.0 as usize]
                    .source_name
                    .starts_with("<default ")
                    .then_some(*id)
            }
            _ => None,
        })
        .expect("the constant default call has a reload-only raise edge");
    remove_hold_release(&mut reload, main, landing);
    require_missing_release(&hir, &reload, true);
}

// Cost: one check, two lowerings, and four release comparisons; 0.01 s on the development host.
#[test]
fn a_named_lambda_default_keeps_the_earlier_operand_until_its_raise_edge() {
    let hir = check_program(&[SourceFile::new(
        "lambda-default.ts",
        r#"
function boom(): i32 { throw new Error("boom"); }
let fire: boolean = false;
function initX(): i32 { if (fire) { return boom(); } return 1; }
function* gen(): Generator<i32> { yield 1; }
export function main(): void {
  const run = (g: Generator<i32>, n: i32 = initX()): i32 => { if (n > 5) { return boom(); } return n; };
  try { run(gen()); } catch (e) {}
}
"#,
    )]).expect("lambda default checks");
    for reload in [false, true] {
        let mut lir = lower_module_for_reload(&hir, reload).expect("lambda default lowers");
        check(&hir, &lir, reload);
        let main = lir
            .functions
            .iter()
            .position(|f| f.source_name == "main")
            .unwrap();
        let landing = lir.functions[main]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match (&i.kind, i.raise_edge()) {
                (l::InstructionKind::Call(target), Some(l::RaiseEdge::Handler(id))) => {
                    let l::CallTargetKind::Function(function) = target.kind else {
                        return None;
                    };
                    lir.functions[function.0 as usize]
                        .source_name
                        .starts_with("<default ")
                        .then_some(*id)
                }
                _ => None,
            })
            .expect("the lambda default has a raise edge");
        remove_hold_release(&mut lir, main, landing);
        require_missing_release(&hir, &lir, reload);
    }
}
