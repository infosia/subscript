//! The generator verifier checks descriptions against independent count actions.

use subscript_codegen::lir::{lower_module, verify_module};
use subscript_compiler::{check_program, lir as l, SourceFile};

fn module() -> l::Module {
    let source = "let elsewhere:Promise<i32>[]=[]; function consume(h:Promise<i32>):void{elsewhere.push(h);} function* gen(a:Promise<i32>[], b:Promise<i32>[], flag:boolean):Generator<i32> { let selected=a; if(flag){selected=b;} yield 1; consume(a[0]); elsewhere.pop(); consume(b[0]); elsewhere.pop(); } export function main():void{}";
    lower_module(&check_program(&[SourceFile::new("generator-counts.ts", source)]).unwrap())
        .unwrap()
}

fn message(module: &l::Module, expected: &str) {
    let errors = verify_module(module).expect_err("the violating graph must fail");
    assert!(
        errors.iter().any(|error| error.message.contains(expected)),
        "{errors:?}"
    );
}

#[test]
fn a_missing_cleanup_count_reports_the_state() {
    let mut module = module();
    verify_module(&module).unwrap();
    module
        .functions
        .iter_mut()
        .find(|function| function.is_generator)
        .unwrap()
        .liveness
        .generator_cleanup
        .iter_mut()
        .find(|description| description.suspension.is_some())
        .unwrap()
        .owners
        .clear();
    message(&module, "cleanup count differs");
}

#[test]
fn an_extra_cleanup_count_reports_the_state() {
    let mut module = module();
    verify_module(&module).unwrap();
    let value = module
        .functions
        .iter_mut()
        .find(|function| function.is_generator)
        .unwrap()
        .parameters[0]
        .value;
    let description = module
        .functions
        .iter_mut()
        .find(|function| function.is_generator)
        .unwrap()
        .liveness
        .generator_cleanup
        .iter_mut()
        .find(|description| description.suspension.is_some())
        .unwrap();
    description.owners.push(value);
    message(&module, "cleanup count differs");
}

#[test]
fn an_owned_edge_without_a_count_reports_the_balance() {
    let mut module = module();
    verify_module(&module).unwrap();
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.is_generator)
        .unwrap();
    let value = l::ValueId(function.values.len() as u32);
    function.values.push(l::Value {
        id: value,
        ty: function.values[0].ty.clone(),
        fresh_owner: false,
        source_name: None,
    });
    function.blocks[1].instructions.push(l::Instruction {
        result: Some(value),
        kind: l::InstructionKind::LoadGlobal(l::GlobalId(0)),
        count_action: None,
        operands: Vec::new(),
        invalidates: Vec::new(),
        traps: Vec::new(),
        pos: function.pos.clone(),
    });
    let l::Terminator::Branch(edge) = &mut function.blocks[1].terminator else {
        panic!("branch");
    };
    let index = edge.ownership.iter().position(|owned| *owned).unwrap();
    edge.arguments[index] = l::Operand::Value(value);
    message(&module, "count balance below zero");
}

#[test]
fn a_parameter_with_mixed_flags_reports_the_parameter() {
    let mut module = module();
    verify_module(&module).unwrap();
    let l::Terminator::Branch(edge) = &mut module
        .functions
        .iter_mut()
        .find(|function| function.is_generator)
        .unwrap()
        .blocks[1]
        .terminator
    else {
        panic!("branch");
    };
    let index = edge.ownership.iter().position(|owned| *owned).unwrap();
    edge.ownership[index] = false;
    message(&module, "mixed ownership flags");
}

#[test]
fn conditional_results_replacements_and_loops_have_count_routes() {
    for body in [
        "const selected = flag ? a : b; yield 1; consume(selected[0]); elsewhere.pop();",
        "a[0] = a[1]; yield 1;",
        "for (const batch of [a, b]) { yield 1; consume(batch[0]); elsewhere.pop(); }",
        "let selected = a; while (flag) { selected = b; yield 1; } yield 2;",
    ] {
        let source = format!("let elsewhere:Promise<i32>[]=[]; function consume(h:Promise<i32>):void{{elsewhere.push(h);}} function* gen(a:Promise<i32>[], b:Promise<i32>[], flag:boolean):Generator<i32>{{{body} consume(a[0]); elsewhere.pop(); consume(b[0]); elsewhere.pop();}} export function main():void{{}}");
        let hir = check_program(&[SourceFile::new("generator-routes.ts", source)]).unwrap();
        let module = lower_module(&hir).unwrap();
        verify_module(&module).unwrap();
    }
}

#[test]
fn a_cleanup_description_without_a_suspension_reports_the_block() {
    let mut module = module();
    verify_module(&module).unwrap();
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.is_generator)
        .unwrap();
    function
        .liveness
        .generator_cleanup
        .push(l::GeneratorCleanup::new(Some(function.entry), Vec::new()));
    message(&module, "has no suspension");
}

#[test]
fn an_unreachable_suspension_reports_the_walk_gap() {
    let mut module = module();
    verify_module(&module).unwrap();
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.is_generator)
        .unwrap();
    let mut block = function
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, l::Terminator::Suspend { .. }))
        .unwrap()
        .clone();
    block.id = l::BlockId(function.blocks.len() as u32);
    function.blocks.push(block);
    message(&module, "unreachable by the count walk");
}

#[test]
fn a_non_generator_edge_needs_one_flag_per_argument() {
    let hir = check_program(&[SourceFile::new("ordinary-edge.ts", "export function main():void { let n:i32=0; for(let i:i32=0;i<3;i++){n+=i;}print(`${n}`); }")]).unwrap();
    let mut module = lower_module(&hir).unwrap();
    verify_module(&module).unwrap();
    let edge = module
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .find_map(|block| match &mut block.terminator {
            l::Terminator::Branch(edge) if !edge.arguments.is_empty() => Some(edge),
            _ => None,
        })
        .unwrap();
    edge.ownership.pop();
    message(&module, "one ownership flag per argument");
}

#[test]
fn async_suspensions_carry_flags_for_their_live_arguments() {
    let source = "async function work():Promise<i32>{return 7;} export async function main():Promise<void>{const text=\"live\"; const h=work(); const first=await work();print(text);const second=await h;print(`${first+second}`);await Context.suspend();print(text);}";
    let module =
        lower_module(&check_program(&[SourceFile::new("async-flags.ts", source)]).unwrap())
            .unwrap();
    verify_module(&module).unwrap();
    let mut suspensions = 0;
    for function in &module.functions {
        for block in &function.blocks {
            if let l::Terminator::Suspend {
                arguments,
                ownership,
                ..
            } = &block.terminator
            {
                assert_eq!(arguments.len(), ownership.len());
                assert!(!arguments.is_empty());
                suspensions += 1;
            }
        }
    }
    assert_eq!(suspensions, 3);
}
