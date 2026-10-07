use super::*;

fn verifier_module(function: lir::Function) -> Module {
    Module {
        host_entries: Vec::new(),
        entry: None,
        async_roots: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
        string_aliases: Vec::new(),
        globals: Vec::new(),
        foreign_functions: Vec::new(),
        functions: vec![function],
        worker_entries: Vec::new(),
        intrinsic_operations: Vec::new(),
        initializer: None,
    }
}

#[test]
fn suspend_successor_rejects_a_pre_suspend_value_without_a_parameter() {
    let pos = subscript_compiler::Pos::new("suspend-boundary.ts", 1, 1);
    let module = verifier_module(lir::Function {
        id: lir::FunctionId(0),
        source_name: "cross_suspend".to_string(),
        kind: lir::FunctionKind::Free,
        exported: false,
        is_generator: false,
        is_async: true,
        creation_traps: Vec::new(),
        host_entry_traps: None,
        can_raise: false,
        parameters: Vec::new(),
        return_type: Type::Void,
        locals: Vec::new(),
        values: vec![
            lir::Value {
                id: lir::ValueId(0),
                ty: ValueType::Data(Type::I32),
                fresh_owner: false,
                source_name: Some("before".to_string()),
            },
            lir::Value {
                id: lir::ValueId(1),
                ty: ValueType::Data(Type::I32),
                fresh_owner: false,
                source_name: Some("after".to_string()),
            },
        ],
        liveness: lir::Liveness::default(),
        blocks: vec![
            lir::BasicBlock {
                id: BlockId(0),
                source_name: Some("entry".to_string()),
                parameters: Vec::new(),
                instructions: vec![lir::Instruction {
                    count_action: None,
                    result: Some(lir::ValueId(0)),
                    kind: InstructionKind::Zero,
                    operands: Vec::new(),
                    invalidates: Vec::new(),
                    traps: Vec::new(),
                    pos: pos.clone(),
                }],
                terminator: Terminator::Suspend {
                    ownership: Vec::new(),
                    kind: lir::SuspendKind::Async,
                    pos: pos.clone(),
                    successor: BlockId(1),
                    resume_value: None,
                    arguments: Vec::new(),
                    invalidates: Vec::new(),
                    traps: Vec::new(),
                },
            },
            lir::BasicBlock {
                id: BlockId(1),
                source_name: Some("resume".to_string()),
                parameters: Vec::new(),
                instructions: vec![lir::Instruction {
                    count_action: None,
                    result: Some(lir::ValueId(1)),
                    kind: InstructionKind::Copy,
                    operands: vec![Operand::Value(lir::ValueId(0))],
                    invalidates: Vec::new(),
                    traps: Vec::new(),
                    pos: pos.clone(),
                }],
                terminator: Terminator::Return {
                    value: None,
                    pos: pos.clone(),
                },
            },
        ],
        entry: BlockId(0),
        pos,
    });
    let errors = verify_module(&module).expect_err("pre-suspend use must fail");
    assert!(errors.iter().any(|error| {
        error.message
            == "function 0 (`cross_suspend`): use of value 0 in block 1 crosses suspend in block 0 without a successor parameter"
    }), "{errors:?}");
}

#[test]
fn interpreter_poison_reports_an_activation_local_read_after_suspend() {
    let pos = subscript_compiler::Pos::new("local-storage-class.ts", 1, 1);
    for (label, read, read_type) in [
        (
            "load",
            InstructionKind::LoadLocal(lir::LocalId(0)),
            ValueType::Data(Type::I32),
        ),
        (
            "address",
            InstructionKind::AddressOfLocal(lir::LocalId(0)),
            ValueType::Address(lir::AddressType {
                pointee: Type::I32,
                array_base: None,
            }),
        ),
    ] {
        let mut module = verifier_module(lir::Function {
            id: lir::FunctionId(0),
            source_name: "bad_activation_local".to_string(),
            kind: lir::FunctionKind::Free,
            exported: false,
            is_generator: false,
            is_async: true,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            can_raise: false,
            parameters: Vec::new(),
            return_type: Type::Void,
            locals: vec![lir::Local {
                id: lir::LocalId(0),
                source_name: "saved".to_string(),
                ty: ValueType::Data(Type::I32),
                mutable: true,
                storage: lir::LocalStorageClass::Activation,
                pos: pos.clone(),
            }],
            values: vec![
                lir::Value {
                    id: lir::ValueId(0),
                    ty: ValueType::Data(Type::I32),
                    fresh_owner: false,
                    source_name: None,
                },
                lir::Value {
                    id: lir::ValueId(1),
                    ty: read_type,
                    fresh_owner: false,
                    source_name: Some("saved".to_string()),
                },
            ],
            liveness: lir::Liveness {
                generator_cleanup: Vec::new(),
                live_ins: vec![Vec::new(); 2],
                value_origins: vec![lir::ValueId(0), lir::ValueId(1)],
            },
            blocks: vec![
                lir::BasicBlock {
                    id: BlockId(0),
                    source_name: Some("entry".to_string()),
                    parameters: Vec::new(),
                    instructions: vec![
                        lir::Instruction {
                            count_action: None,
                            result: Some(lir::ValueId(0)),
                            kind: InstructionKind::Zero,
                            operands: Vec::new(),
                            invalidates: Vec::new(),
                            traps: Vec::new(),
                            pos: pos.clone(),
                        },
                        lir::Instruction {
                            count_action: None,
                            result: None,
                            kind: InstructionKind::StoreLocal(lir::LocalId(0)),
                            operands: vec![Operand::Value(lir::ValueId(0))],
                            invalidates: Vec::new(),
                            traps: Vec::new(),
                            pos: pos.clone(),
                        },
                    ],
                    terminator: Terminator::Suspend {
                        ownership: Vec::new(),
                        kind: lir::SuspendKind::Async,
                        pos: pos.clone(),
                        successor: BlockId(1),
                        resume_value: None,
                        arguments: Vec::new(),
                        invalidates: Vec::new(),
                        traps: Vec::new(),
                    },
                },
                lir::BasicBlock {
                    id: BlockId(1),
                    source_name: Some("resume".to_string()),
                    parameters: Vec::new(),
                    instructions: vec![lir::Instruction {
                        count_action: None,
                        result: Some(lir::ValueId(1)),
                        kind: read,
                        operands: Vec::new(),
                        invalidates: Vec::new(),
                        traps: Vec::new(),
                        pos: pos.clone(),
                    }],
                    terminator: Terminator::Return {
                        value: None,
                        pos: pos.clone(),
                    },
                },
            ],
            entry: BlockId(0),
            pos: pos.clone(),
        });
        module.entry = Some(lir::FunctionId(0));
        let error = interpret(&module).expect_err("the poisoned activation local must fail");
        assert!(
            error.to_string().contains(
                "activation local 0 (`saved`) was loaded after suspend in block 0 at local-storage-class.ts:1:1"
            ),
            "{label}: {error:?}"
        );
    }
}

#[test]
fn address_type_rejects_an_undeclared_array_base() {
    let pos = subscript_compiler::Pos::new("array-base.ts", 1, 1);
    let module = verifier_module(lir::Function {
        id: lir::FunctionId(0),
        source_name: "bad_array_base".to_string(),
        kind: lir::FunctionKind::Free,
        exported: false,
        is_generator: false,
        is_async: false,
        creation_traps: Vec::new(),
        host_entry_traps: None,
        can_raise: false,
        parameters: vec![lir::Parameter {
            storage: None,
            value: lir::ValueId(0),
            source_name: "address".to_string(),
            kind: lir::ParameterKind::Explicit,
            pos: pos.clone(),
        }],
        return_type: Type::Void,
        locals: Vec::new(),
        values: vec![lir::Value {
            id: lir::ValueId(0),
            ty: ValueType::Address(lir::AddressType {
                pointee: Type::I32,
                array_base: Some(lir::ValueId(99)),
            }),
            fresh_owner: false,
            source_name: Some("address".to_string()),
        }],
        liveness: lir::Liveness::default(),
        blocks: vec![lir::BasicBlock {
            id: BlockId(0),
            source_name: Some("entry".to_string()),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return {
                value: None,
                pos: pos.clone(),
            },
        }],
        entry: BlockId(0),
        pos,
    });
    let errors = verify_module(&module).expect_err("undeclared array base must fail");
    assert!(
        errors.iter().any(|error| {
            error.message
            == "function 0 (`bad_array_base`): address value 0 names undeclared array base value 99"
        }),
        "{errors:?}"
    );
}

#[test]
fn intrinsic_call_is_checked_against_the_module_signature_table() {
    let valid = lower_source(
        "math-signature.ts",
        "export function main(): void { print(`${Math.abs(1.0)}`); }\n",
    );
    let pos = subscript_compiler::Pos::new("wrong-math.ts", 1, 1);
    let mut module = verifier_module(lir::Function {
        id: lir::FunctionId(0),
        source_name: "wrong_math_abs".to_string(),
        kind: lir::FunctionKind::Free,
        exported: false,
        is_generator: false,
        is_async: false,
        creation_traps: Vec::new(),
        host_entry_traps: None,
        can_raise: false,
        parameters: (0..3)
            .map(|index| lir::Parameter {
                storage: None,
                value: lir::ValueId(index),
                source_name: format!("arg{index}"),
                kind: lir::ParameterKind::Explicit,
                pos: pos.clone(),
            })
            .collect(),
        return_type: Type::Void,
        locals: Vec::new(),
        values: vec![
            lir::Value {
                id: lir::ValueId(0),
                ty: ValueType::Data(Type::Str),
                fresh_owner: false,
                source_name: Some("arg0".to_string()),
            },
            lir::Value {
                id: lir::ValueId(1),
                ty: ValueType::Data(Type::Str),
                fresh_owner: false,
                source_name: Some("arg1".to_string()),
            },
            lir::Value {
                id: lir::ValueId(2),
                ty: ValueType::Data(Type::Str),
                fresh_owner: false,
                source_name: Some("arg2".to_string()),
            },
            lir::Value {
                id: lir::ValueId(3),
                ty: ValueType::Data(Type::Bool),
                fresh_owner: false,
                source_name: None,
            },
        ],
        liveness: lir::Liveness::default(),
        blocks: vec![lir::BasicBlock {
            id: BlockId(0),
            source_name: Some("entry".to_string()),
            parameters: Vec::new(),
            instructions: vec![lir::Instruction {
                count_action: None,
                result: Some(lir::ValueId(3)),
                kind: InstructionKind::Call(lir::CallTarget {
                    kind: lir::CallTargetKind::Intrinsic(lir::Intrinsic {
                        family: lir::IntrinsicFamily::Math,
                        operation: 0,
                        type_argument: None,
                        worker_entry: None,
                    }),
                    parameter_types: Vec::new(),
                    return_type: Some(ValueType::Data(Type::Bool)),
                }),
                operands: vec![
                    Operand::Value(lir::ValueId(0)),
                    Operand::Value(lir::ValueId(1)),
                    Operand::Value(lir::ValueId(2)),
                ],
                invalidates: Vec::new(),
                traps: Vec::new(),
                pos: pos.clone(),
            }],
            terminator: Terminator::Return {
                value: None,
                pos: pos.clone(),
            },
        }],
        entry: BlockId(0),
        pos,
    });
    module.intrinsic_operations = valid.intrinsic_operations;
    let errors = verify_module(&module).expect_err("wrong Math.Abs signature must fail");
    assert!(errors.iter().any(|error| {
        error.message == "function 0 (`wrong_math_abs`): block 0 instruction 0 call disagrees with the signature table: Math.Abs declares [Data(F64)] -> Some(Data(F64)), got [Data(Str), Data(Str), Data(Str)] -> Some(Data(Bool))"
    }), "{errors:?}");
}

#[test]
fn suspend_invalidates_only_live_arrays() {
    let module = lower_source(
        "suspend-invalidates.ts",
        r#"
async function main(): Promise<void> {
  const dead: i32[] = [1];
  const live: i32[] = [2];
  await Context.suspend();
  print(`${live.length}`);
}
"#,
    );
    let main = module
        .functions
        .iter()
        .find(|function| function.source_name == "main")
        .expect("main function");
    let arrays = main
        .values
        .iter()
        .filter(|value| matches!(value.ty, ValueType::Data(Type::Array(_))))
        .count();
    assert_eq!(arrays, 3, "two arrays plus the threaded live value");
    let invalidates = main
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            Terminator::Suspend {
                kind: lir::SuspendKind::Async,
                invalidates,
                ..
            } => Some(invalidates),
            _ => None,
        })
        .expect("Context.suspend terminator");
    assert_eq!(invalidates.len(), 1);
    assert!(matches!(
        main.values[invalidates[0].0 as usize].ty,
        ValueType::Data(Type::Array(_))
    ));
}

#[test]
fn async_handle_create_invalidates_all_arrays_created_before_the_call() {
    let module = lower_source(
        "async-start-invalidates.ts",
        r#"
async function grow(a: i32[]): Promise<void> { a.push(3); }
export async function main(): Promise<void> {
  const first: i32[] = [1];
  const second: i32[] = [2];
  const third: i32[] = [3];
  const h: Promise<void> = grow(second);
  const later: i32[] = [4];
  print(`${second.length},${later.length}`);
  await h;
}
"#,
    );
    let main = module
        .functions
        .iter()
        .find(|f| f.source_name == "main")
        .expect("main function");
    let instructions = main
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .collect::<Vec<_>>();
    let start_index = instructions
        .iter()
        .position(|instruction| matches!(instruction.kind, InstructionKind::AsyncHandleCreate(_)))
        .expect("async handle creation");
    // Derive the expected storage facts from array definitions, not invalidates.
    let array_definitions = |instructions: &[&subscript_compiler::lir::Instruction]| {
        instructions
            .iter()
            .filter_map(|instruction| instruction.result)
            .filter(|value| {
                matches!(
                    main.values[value.0 as usize].ty,
                    ValueType::Data(Type::Array(_))
                )
            })
            .collect::<Vec<_>>()
    };
    let before = array_definitions(&instructions[..start_index]);
    let after = array_definitions(&instructions[start_index + 1..]);
    assert_eq!(before.len(), 3, "three arrays are defined before the call");
    assert_eq!(after.len(), 1, "one array is defined after the call");
    assert_eq!(
        instructions[start_index].invalidates, before,
        "the call invalidates exactly the preceding array definitions in order"
    );
}
