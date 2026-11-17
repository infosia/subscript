use super::*;

#[test]
fn item_12_reports_a_foreign_array_without_managed_storage() {
    let accept = corpus::corpus_accept();
    let sources = corpus::entry_sources(&accept, "a26-interop-array-pair");
    let hir = check_program(&sources).expect("a26 checks clean");
    let mut lir = lower_module(&hir).expect("a26 lowers to LIR");
    let call = lir
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| {
            matches!(&instruction.kind, InstructionKind::Call(target)
            if matches!(target.kind, lir::CallTargetKind::Foreign(_)) &&
            target.parameter_types.iter().any(|ty| matches!(ty, ValueType::Data(Type::Array(_)))))
        })
        .expect("a26 has a foreign array call");
    call.operands.clear();
    let findings = lir_facts::dropped_facts(&hir, &lir);
    assert!(findings.iter().any(
        |finding| finding.contains("foreign array argument does not name its managed storage")
    ));
    let errors = verify_module(&lir).expect_err("an unnamed derivation must reject");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("call signature disagrees with its target:")),
        "{errors:?}"
    );
}

#[test]
fn verifier_rejects_a_foreign_array_pointer_without_its_storage_operand() {
    let accept = corpus::corpus_accept();
    let mut module = lower_entry(&accept, "a26-interop-array-pair");
    let function = module.functions.iter_mut().find(|function| function.blocks.iter().flat_map(|block| &block.instructions)
        .any(|instruction| matches!(&instruction.kind, InstructionKind::Call(target)
            if matches!(target.kind, lir::CallTargetKind::Foreign(_)) && target.parameter_types.iter().any(|ty| matches!(ty, ValueType::Data(Type::Array(_)))))))
        .expect("foreign array caller");
    let (block_index, call_index, argument_index, element) = function
        .blocks
        .iter()
        .enumerate()
        .find_map(|(block_index, block)| {
            block
                .instructions
                .iter()
                .enumerate()
                .find_map(|(call_index, instruction)| {
                    let InstructionKind::Call(target) = &instruction.kind else {
                        return None;
                    };
                    if !matches!(target.kind, lir::CallTargetKind::Foreign(_)) {
                        return None;
                    }
                    target
                        .parameter_types
                        .iter()
                        .enumerate()
                        .find_map(|(index, ty)| match ty {
                            ValueType::Data(Type::Array(element)) => {
                                Some((block_index, call_index, index, (**element).clone()))
                            }
                            _ => None,
                        })
                })
        })
        .expect("array argument");
    let call = &mut function.blocks[block_index].instructions[call_index];
    let original = call.operands[argument_index].clone();
    let Operand::Value(base) = original else {
        panic!("managed array value")
    };
    let pointer = lir::ValueId(function.values.len() as u32);
    let address = ValueType::Address(lir::AddressType {
        pointee: element,
        array_base: Some(base),
    });
    function.values.push(lir::Value {
        id: pointer,
        ty: address.clone(),
        fresh_owner: false,
        source_name: None,
    });
    function.liveness.value_origins.push(pointer);
    call.operands[argument_index] = Operand::Value(pointer);
    call.traps
        .retain(|trap| trap.kind != TrapKind::DevOnlyLifetime(argument_index));
    let InstructionKind::Call(target) = &mut call.kind else {
        unreachable!()
    };
    target.parameter_types[argument_index] = address;
    let position = call.pos.clone();
    function.blocks[block_index].instructions.insert(
        call_index,
        lir::Instruction {
            result: Some(pointer),
            kind: InstructionKind::AddressOfIndex { checked: false },
            count_action: None,
            operands: vec![
                Operand::Value(base),
                Operand::Constant(lir::Constant {
                    ty: Type::I32,
                    kind: lir::ConstantKind::Integer(0),
                }),
            ],
            invalidates: Vec::new(),
            traps: vec![lir::Trap {
                kind: TrapKind::DevOnlyLifetime(0),
                pos: position.clone(),
            }],
            pos: position,
        },
    );
    let errors =
        verify_module(&module).expect_err("a foreign array call must name its managed storage");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("call signature disagrees with the target declaration")),
        "{errors:?}"
    );
}

#[test]
fn verifier_rejects_a_suspended_foreign_pointer_without_its_owner() {
    let mirror = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../corpus/interop/interop.generated.d.ts"),
    )
    .expect("interop mirror");
    let source = r#"
class Owner {
  tag: i32;
  value: SubByValueI32One;
  constructor() { this.tag = 0; this.value = new SubByValueI32One(1); }
}
async function later(info: SubCallbackInfo): Promise<SubCallbackInfo> {
  await Context.suspend();
  return info;
}
export async function main(): Promise<void> {
  const owner = new Owner();
  const info = new SubCallbackInfo((message, first, second) => {}, null, null);
  subBorrowPointer(owner.value, await later(info));
}
"#;
    let hir = check_program(&[
        SourceFile::ambient("interop.generated.d.ts", mirror),
        SourceFile::new("suspend.ts", source),
    ])
    .expect("checked pointer argument");
    let mut module = lower_module(&hir).expect("lowered pointer argument");
    let owner = module
        .classes
        .iter()
        .find(|class| class.source_name == "Owner")
        .expect("owner class")
        .id;
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.source_name == "main")
        .expect("caller");
    let values = &function.values;
    let arguments = function.blocks.iter_mut().find_map(|block| match &mut block.terminator {
        Terminator::Suspend { arguments, .. } if arguments.iter().any(|argument| matches!(argument,
            Operand::Value(value) if values[value.0 as usize].ty == ValueType::Data(Type::Class(owner)))) => Some(arguments),
        _ => None,
    }).expect("owner crosses the suspension");
    let index = arguments
        .iter()
        .position(|argument| {
            matches!(argument,
        Operand::Value(value) if values[value.0 as usize].ty == ValueType::Data(Type::Class(owner)))
        })
        .expect("owner argument");
    arguments.remove(index);
    let errors =
        verify_module(&module).expect_err("an address needs its owner across a suspension");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("without storage owner")),
        "{errors:?}"
    );
}

// compiler.md §324 acceptance 3: build a value operand, with its real value type.
#[test]
fn foreign_fixed_array_value_is_rejected_against_the_callee_declaration() {
    let mirror = SourceFile::ambient(
        "array.d.ts",
        "// @subscript-c-header include=\"array.h\"\ndeclare function fill(values: FixedArray<i32, 4>): void;",
    );
    let program = SourceFile::new("main.ts", "export function main(): void { const values: FixedArray<i32, 4> = [1, 2, 3, 4]; fill(values); }");
    let hir = check_program(&[mirror, program]).expect("array parameter checks");
    let mut module = lower_module(&hir).expect("array parameter lowers");
    verify_module(&module).expect("address operand verifies");
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.source_name == "main")
        .expect("caller");
    let block = function
        .blocks
        .iter_mut()
        .find(|block| {
            block
                .instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::Call(_)))
        })
        .expect("call block");
    let index = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::Call(_)))
        .expect("call");
    let address = block.instructions[index].operands[0].clone();
    let value = lir::ValueId(function.values.len() as u32);
    let ty = ValueType::Data(Type::FixedArray(Box::new(Type::I32), 4));
    function.values.push(lir::Value {
        id: value,
        ty: ty.clone(),
        fresh_owner: false,
        source_name: None,
    });
    function.liveness.value_origins.push(value);
    let pos = block.instructions[index].pos.clone();
    block.instructions.insert(
        index,
        lir::Instruction {
            result: Some(value),
            kind: InstructionKind::LoadAddress,
            invalidates: vec![],
            count_action: None,
            operands: vec![address],
            traps: vec![],
            pos,
        },
    );
    let original = &block.instructions[index + 1];
    let InstructionKind::Call(target) = &original.kind else {
        unreachable!()
    };
    // Build the violating call. Its value operand agrees with its call signature.
    // The foreign declaration independently requires an address (compiler.md §324 rule 5).
    block.instructions[index + 1] = lir::Instruction {
        kind: InstructionKind::Call(lir::CallTarget {
            kind: target.kind.clone(),
            parameter_types: vec![ty],
            return_type: None,
        }),
        operands: vec![Operand::Value(value)],
        result: None,
        traps: original.traps.clone(),
        invalidates: original.invalidates.clone(),
        count_action: None,
        pos: original.pos.clone(),
    };
    let errors =
        verify_module(&module).expect_err("a fixed array value is not an array parameter address");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("call signature disagrees with the target declaration")),
        "{errors:?}"
    );
}
