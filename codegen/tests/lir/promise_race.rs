//! `Promise.race` in the LIR (compiler.md §350 rules 1, 2, 7, and 11).
//! Cost: one check and one lowering for each test; no native compilation.
use super::*;
use subscript_codegen::lir::allocation::{
    instruction_allocation, InstructionAllocation, MethodBodies,
};

const PROGRAM: &str = "async function f(): Promise<i32> { return 1; }
export async function main(): Promise<void> {
  const jobs: Promise<i32>[] = [f(), f()];
  const first: i32 = await Promise.race(jobs);
  const each: i32[] = await Promise.all(jobs);
  print(`${first} ${each.length}`);
}
";

fn lowered() -> Module {
    let files = [SourceFile::entry("promise-race.ts", PROGRAM)];
    let hir = subscript_compiler::check_program(&files).expect("checker");
    let module = lower_module(&hir).expect("lower");
    verify_module(&module).expect("verified");
    module
}

/// The function, block, and instruction index of the first instruction of `kind`.
fn find(module: &Module, kind: &InstructionKind) -> (usize, usize, usize) {
    for (f, function) in module.functions.iter().enumerate() {
        for (b, block) in function.blocks.iter().enumerate() {
            if let Some(i) = block.instructions.iter().position(|i| &i.kind == kind) {
                return (f, b, i);
            }
        }
    }
    panic!("no {kind:?}");
}

fn value_type(function: &lir::Function, id: lir::ValueId) -> ValueType {
    function.values[id.0 as usize].ty.clone()
}

#[test]
fn race_lowers_to_an_allocating_instruction_with_a_call_trap() {
    let module = lowered();
    let methods = MethodBodies::of_module(&module);
    let handle = Type::async_handle(Type::I32);
    for kind in [InstructionKind::AsyncRace, InstructionKind::AsyncAll] {
        let (f, b, i) = find(&module, &kind);
        let function = &module.functions[f];
        let instruction = &function.blocks[b].instructions[i];
        let [Operand::Value(input)] = instruction.operands.as_slice() else {
            panic!("one value operand: {:?}", instruction.operands);
        };
        assert_eq!(
            value_type(function, *input),
            ValueType::Data(Type::array(handle.clone()))
        );
        let result = value_type(function, instruction.result.expect("a result"));
        let traps: Vec<_> = instruction.traps.iter().map(|trap| &trap.kind).collect();
        if kind == InstructionKind::AsyncRace {
            assert_eq!(result, ValueType::Data(handle.clone()));
            // §350 rule 7: the empty array traps at the call.
            assert!(traps.contains(&&TrapKind::Call), "{traps:?}");
        } else {
            assert_eq!(
                result,
                ValueType::Data(Type::async_handle(Type::array(Type::I32)))
            );
            assert!(!traps.contains(&&TrapKind::Call), "{traps:?}");
        }
        assert!(traps.contains(&&TrapKind::Allocation), "{traps:?}");
        // §342 rule 10: both aggregates allocate a frame.
        assert_eq!(
            instruction_allocation(&module, &methods, function, instruction),
            InstructionAllocation::Can
        );
        assert!(kind.produces_fresh_async_owner());
    }
}

/// §350 rules 1 and 2: the verifier rejects a race with the result of
/// `Promise.all`, and a race whose result is counted. Control: the
/// lowered form verifies.
#[test]
fn verifier_rejects_a_wrong_race_signature() {
    let module = lowered();
    let (f, b, i) = find(&module, &InstructionKind::AsyncRace);
    let instruction = &module.functions[f].blocks[b].instructions[i];
    let result = instruction.result.expect("a result");
    let Operand::Value(input) = instruction.operands[0].clone() else {
        panic!("a value operand");
    };
    let wrong = |edit: &dyn Fn(&mut lir::Function)| {
        let mut module = module.clone();
        edit(&mut module.functions[f]);
        verify_module(&module)
            .expect_err("a wrong race verifies")
            .iter()
            .any(|error| error.message.contains("race signature is invalid"))
    };
    assert!(wrong(&|function| {
        function.values[result.0 as usize].ty =
            ValueType::Data(Type::async_handle(Type::array(Type::I32)));
    }));
    assert!(wrong(&|function| {
        let counted = Type::async_handle(Type::array(Type::async_handle(Type::I32)));
        function.values[input.0 as usize].ty = ValueType::Data(Type::array(counted.clone()));
        function.values[result.0 as usize].ty = ValueType::Data(counted);
    }));
    // Rule 2 covers each counted type: a nested array of handles and a
    // generator.
    for counted in [
        Type::array(Type::array(Type::async_handle(Type::I32))),
        Type::Generator(Box::new(Type::I32)),
    ] {
        assert!(wrong(&|function| {
            let handle = Type::async_handle(counted.clone());
            function.values[input.0 as usize].ty = ValueType::Data(Type::array(handle.clone()));
            function.values[result.0 as usize].ty = ValueType::Data(handle);
        }));
    }
    verify_module(&module).expect("the lowered race verifies");
}

/// §166 rule 12: the verifier rejects `Promise.all` over each counted
/// result type. Control: the lowered form verifies.
#[test]
fn verifier_rejects_an_aggregate_of_each_counted_result() {
    let module = lowered();
    let (f, b, i) = find(&module, &InstructionKind::AsyncAll);
    let instruction = &module.functions[f].blocks[b].instructions[i];
    let result = instruction.result.expect("a result");
    let Operand::Value(input) = instruction.operands[0].clone() else {
        panic!("a value operand");
    };
    for counted in [
        Type::async_handle(Type::I32),
        Type::array(Type::array(Type::async_handle(Type::I32))),
        Type::Generator(Box::new(Type::I32)),
    ] {
        let mut wrong = module.clone();
        let function = &mut wrong.functions[f];
        function.values[input.0 as usize].ty =
            ValueType::Data(Type::array(Type::async_handle(counted.clone())));
        function.values[result.0 as usize].ty =
            ValueType::Data(Type::async_handle(Type::array(counted.clone())));
        assert!(
            verify_module(&wrong)
                .expect_err("a counted aggregate verifies")
                .iter()
                .any(|error| error.message.contains("aggregate signature is invalid")),
            "{counted:?}"
        );
    }
    verify_module(&module).expect("the lowered aggregate verifies");
}
