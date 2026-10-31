//! The one equality rule of a nullable string (`compiler.md` §218.1 rule 2).
//!
//! Each tier reads a `Binary(Eq)` of two `string` operands, so the LIR form
//! carries the rule. The tests build the violating forms. Cost: one check and
//! one lowering of a small program per test.

use subscript_codegen::lir::{lower_module, verify_module};
use subscript_compiler::lir as l;
use subscript_compiler::{check_program, SourceFile, Type};

const SOURCE: &str = "function same(a: string | null, b: string | null): boolean {\n\
                      \x20 return a === b;\n\
                      }\n\
                      export function main(): void { print(`${same(null, \"a\")}`); }\n";

fn lowered() -> l::Module {
    let hir = check_program(&[SourceFile::new("equality.ts", SOURCE)]).expect("checks clean");
    lower_module(&hir).expect("lowers")
}

fn same(module: &mut l::Module) -> &mut l::Function {
    module
        .functions
        .iter_mut()
        .find(|function| function.source_name == "same")
        .expect("function same")
}

fn messages(module: &l::Module) -> Vec<String> {
    verify_module(module)
        .expect_err("the form is invalid")
        .iter()
        .map(|error| error.message.clone())
        .collect()
}

#[test]
fn a_content_comparison_reaches_each_tier_with_string_operands() {
    let mut module = lowered();
    let function = same(&mut module);
    let instructions: Vec<_> = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .collect();
    let operands = instructions
        .iter()
        .filter(|instruction| instruction.kind == l::InstructionKind::StringOperand)
        .count();
    assert_eq!(operands, 2);
    let comparison = instructions
        .iter()
        .find(|instruction| instruction.kind == l::InstructionKind::Binary(l::BinaryOp::Eq))
        .expect("one comparison");
    for operand in &comparison.operands {
        let l::Operand::Value(value) = operand else {
            panic!("{operand:?}");
        };
        assert_eq!(
            function.values[value.0 as usize].ty,
            l::ValueType::Data(Type::Str)
        );
    }
}

#[test]
fn a_pointer_compare_of_a_nullable_string_is_invalid() {
    let mut module = lowered();
    assert!(verify_module(&module).is_ok());
    let function = same(&mut module);
    // Take the string operands away, so the comparison reads the nullable values.
    let mut sources = Vec::new();
    for block in &mut function.blocks {
        for instruction in &block.instructions {
            if instruction.kind == l::InstructionKind::StringOperand {
                sources.push(instruction.operands[0].clone());
            }
        }
        block
            .instructions
            .retain(|instruction| instruction.kind != l::InstructionKind::StringOperand);
        for instruction in &mut block.instructions {
            if instruction.kind == l::InstructionKind::Binary(l::BinaryOp::Eq) {
                instruction.operands = sources.clone();
            }
        }
    }
    assert!(
        messages(&module)
            .iter()
            .any(|message| message.contains("binary operand/result types are invalid")),
        "{:?}",
        messages(&module)
    );
}

#[test]
fn a_string_operand_has_no_other_reader() {
    let mut module = lowered();
    let function = same(&mut module);
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            if instruction.kind == l::InstructionKind::Binary(l::BinaryOp::Eq) {
                instruction.kind = l::InstructionKind::Binary(l::BinaryOp::Lt);
            }
        }
    }
    assert!(
        messages(&module)
            .iter()
            .any(|message| message.contains("a string operand has a reader")),
        "{:?}",
        messages(&module)
    );
}
