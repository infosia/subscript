//! Hand-built forms omit operations rather than change ownership records.

use subscript_codegen::lir::verify_module;
use subscript_compiler::{lir as l, Pos, Type};

fn module(element: Type, removal: bool, acquire: bool, release: bool) -> l::Module {
    let pos = Pos::new("violating-counted-operation.ts", 1, 1);
    let array = l::ValueType::Data(Type::Array(Box::new(element.clone())));
    let owner = l::ValueType::Data(element.clone());
    let address = l::ValueType::Address(l::AddressType {
        pointee: element.clone(),
        array_base: Some(l::ValueId(0)),
    });
    let value = |id, ty, fresh_owner| l::Value {
        id: l::ValueId(id),
        ty,
        fresh_owner,
        source_name: None,
    };
    let instruction = |result, kind, operands| l::Instruction {
        count_action: None,
        result,
        kind,
        operands,
        invalidates: Vec::new(),
        traps: Vec::new(),
        pos: pos.clone(),
    };
    let retain = if matches!(element, Type::AsyncHandle(_)) {
        l::InstructionKind::AsyncHandleRetain
    } else {
        l::InstructionKind::AsyncHandleArrayRetain
    };
    let drop = if matches!(element, Type::AsyncHandle(_)) {
        l::InstructionKind::AsyncHandleRelease
    } else {
        l::InstructionKind::AsyncHandleArrayRelease
    };
    let mut values = vec![
        value(0, array.clone(), false),
        value(1, address, false),
        value(2, owner.clone(), false),
    ];
    let mut instructions = Vec::new();
    if removal {
        values.push(value(3, owner, true));
        let target = l::CallTarget {
            kind: l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayPop),
            parameter_types: Vec::new(),
            return_type: Some(l::ValueType::Data(element.clone())),
        };
        let mut call = instruction(
            Some(l::ValueId(3)),
            l::InstructionKind::Call(target),
            vec![l::Operand::Value(l::ValueId(0))],
        );
        call.invalidates.push(l::ValueId(0));
        call.traps.push(l::Trap {
            kind: l::TrapKind::IndexRead,
            pos: pos.clone(),
        });
        call.traps.push(l::Trap {
            kind: l::TrapKind::DevOnlyLifetime(0),
            pos: pos.clone(),
        });
        instructions.push(call);
    } else {
        values.push(value(3, owner, false));
        instructions.push(instruction(
            Some(l::ValueId(3)),
            l::InstructionKind::LoadAddress,
            vec![l::Operand::Value(l::ValueId(1))],
        ));
        if acquire {
            let mut copy = instruction(None, retain, vec![l::Operand::Value(l::ValueId(2))]);
            copy.traps.push(l::Trap {
                kind: l::TrapKind::DevOnlyLifetime(0),
                pos: pos.clone(),
            });
            instructions.push(copy);
        }
        instructions.push(instruction(
            None,
            l::InstructionKind::StoreAddress,
            vec![
                l::Operand::Value(l::ValueId(1)),
                l::Operand::Value(l::ValueId(2)),
            ],
        ));
    }
    if release {
        let mut drop = instruction(None, drop, vec![l::Operand::Value(l::ValueId(3))]);
        drop.traps.push(l::Trap {
            kind: l::TrapKind::DevOnlyLifetime(0),
            pos: pos.clone(),
        });
        drop.traps.push(l::Trap {
            kind: l::TrapKind::Call,
            pos: pos.clone(),
        });
        instructions.push(drop);
    }
    let parameters = (0..3)
        .map(|id| l::Parameter {
            value: l::ValueId(id),
            storage: None,
            source_name: format!("parameter{id}"),
            kind: l::ParameterKind::Explicit,
            pos: pos.clone(),
        })
        .collect();
    let function = l::Function {
        id: l::FunctionId(0),
        source_name: "violating_counted_operation".into(),
        kind: l::FunctionKind::Free,
        exported: false,
        is_generator: false,
        is_async: false,
        creation_traps: Vec::new(),
        host_entry_traps: None,
        can_raise: false,
        parameters,
        return_type: Type::Void,
        locals: Vec::new(),
        values,
        liveness: l::Liveness::default(),
        entry: l::BlockId(0),
        pos: pos.clone(),
        blocks: vec![l::BasicBlock {
            id: l::BlockId(0),
            source_name: None,
            parameters: Vec::new(),
            instructions,
            terminator: l::Terminator::Return { value: None, pos },
        }],
    };
    l::Module {
        functions: vec![function],
        globals: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
        string_aliases: Vec::new(),
        foreign_functions: Vec::new(),
        worker_entries: Vec::new(),
        intrinsic_operations: if removal {
            vec![l::IntrinsicOperation {
                family: l::IntrinsicFamily::Array,
                operation: 0,
                semantic_name: "Pop".into(),
                runtime_symbol: None,
                signatures: vec![l::CallSignature {
                    target: l::CallSignatureTarget::BuiltinMethod(l::BuiltinMethod::ArrayPop),
                    parameter_types: vec![array],
                    return_type: Some(l::ValueType::Data(element)),
                }],
            }]
        } else {
            Vec::new()
        },
        host_entries: Vec::new(),
        async_roots: Vec::new(),
        entry: None,
        initializer: None,
    }
}

#[test]
fn counted_index_replacement_requires_both_count_operations_at_every_depth() {
    for element in [
        Type::AsyncHandle(Box::new(Type::Void)),
        Type::Array(Box::new(Type::Array(Box::new(Type::AsyncHandle(
            Box::new(Type::Void),
        ))))),
    ] {
        verify_module(&module(element.clone(), false, true, true)).expect("complete replacement");
        let errors = verify_module(&module(element.clone(), false, false, true))
            .expect_err("missing acquire");
        assert!(
            errors.iter().any(|error| error
                .message
                .contains("without a fresh single-use owner or a preceding retain")),
            "{errors:?}"
        );
        let errors = verify_module(&module(element, false, true, false))
            .expect_err("missing old-value release");
        assert!(
            errors.iter().any(|error| error.message.contains(
                "instruction 2 replaces a counted array element without releasing its old value"
            )),
            "{errors:?}"
        );
    }
}

#[test]
fn counted_removal_requires_a_result_consumer() {
    let element = Type::AsyncHandle(Box::new(Type::Void));
    verify_module(&module(element.clone(), true, false, true)).expect("consumed removal");
    let errors = verify_module(&module(element, true, false, false)).expect_err("missing discard");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("instruction 0 removes a counted element without consuming its result")),
        "{errors:?}"
    );
}

fn bulk_module(element: Type, operation: &str, action: Option<l::CountAction>) -> l::Module {
    let mut module = module(element.clone(), false, true, true);
    let function = &mut module.functions[0];
    let array = l::ValueType::Data(Type::Array(Box::new(element.clone())));
    function.values[3].ty = array.clone();
    function.values[3].fresh_owner =
        element.counted_type().is_some() && matches!(operation, "Slice" | "Concat" | "Spread");
    let pos = function.pos.clone();
    let mut operands = vec![l::Operand::Value(l::ValueId(0))];
    let mut parameter_types = vec![array.clone()];
    let kind = if operation == "Spread" {
        l::InstructionKind::ArraySpreadLiteral(vec![Some(l::SpreadKind::Array)])
    } else {
        let integer = l::Operand::Constant(l::Constant {
            ty: Type::I32,
            kind: l::ConstantKind::Integer(0),
        });
        match operation {
            "Fill" => {
                operands.push(l::Operand::Value(l::ValueId(2)));
                parameter_types.push(l::ValueType::Data(element.clone()));
                operands.extend([integer.clone(), integer]);
                parameter_types.extend(vec![l::ValueType::Data(Type::I32); 2]);
            }
            "CopyWithin" | "Slice" => {
                let count = if operation == "Slice" { 2 } else { 3 };
                operands.extend(vec![integer; count]);
                parameter_types.extend(vec![l::ValueType::Data(Type::I32); count]);
            }
            "Concat" => {
                operands.push(l::Operand::Value(l::ValueId(0)));
                parameter_types.push(array.clone());
            }
            _ => panic!("unknown fixture operation"),
        }
        let target = l::CallTargetKind::Intrinsic(l::Intrinsic {
            family: l::IntrinsicFamily::Array,
            operation: 0,
            type_argument: None,
            worker_entry: None,
        });
        module.intrinsic_operations.push(l::IntrinsicOperation {
            family: l::IntrinsicFamily::Array,
            operation: 0,
            semantic_name: operation.into(),
            runtime_symbol: None,
            signatures: vec![l::CallSignature {
                target: l::CallSignatureTarget::Intrinsic(l::Intrinsic {
                    family: l::IntrinsicFamily::Array,
                    operation: 0,
                    type_argument: None,
                    worker_entry: None,
                }),
                parameter_types: parameter_types.clone(),
                return_type: Some(array.clone()),
            }],
        });
        l::InstructionKind::Call(l::CallTarget {
            kind: target,
            parameter_types: Vec::new(),
            return_type: Some(array),
        })
    };
    function.blocks[0].instructions = vec![l::Instruction {
        result: Some(l::ValueId(3)),
        kind,
        count_action: action,
        operands,
        invalidates: Vec::new(),
        traps: if operation == "Concat" {
            vec![
                l::Trap {
                    kind: l::TrapKind::DevOnlyLifetime(0),
                    pos: pos.clone(),
                },
                l::Trap {
                    kind: l::TrapKind::DevOnlyLifetime(1),
                    pos: pos.clone(),
                },
            ]
        } else {
            vec![l::Trap {
                kind: l::TrapKind::DevOnlyLifetime(0),
                pos: pos.clone(),
            }]
        },
        pos,
    }];
    if function.values[3].fresh_owner {
        function.return_type = Type::Array(Box::new(element.clone()));
        function.blocks[0].terminator = l::Terminator::Return {
            value: Some(l::Operand::Value(l::ValueId(3))),
            pos: function.pos.clone(),
        };
    }
    function.blocks[0].instructions[0].traps.push(l::Trap {
        kind: l::TrapKind::Call,
        pos: function.pos.clone(),
    });
    module
}

#[test]
fn bulk_operations_require_the_instruction_action_at_every_depth() {
    for element in [
        Type::I32,
        Type::AsyncHandle(Box::new(Type::Void)),
        Type::Array(Box::new(Type::IterResult(Box::new(Type::FixedArray(
            Box::new(Type::AsyncHandle(Box::new(Type::Void))),
            2,
        ))))),
    ] {
        for operation in ["Fill", "CopyWithin", "Slice", "Concat", "Spread"] {
            let valid = l::CountAction::for_type(&element);
            verify_module(&bulk_module(
                element.clone(),
                operation,
                Some(valid.clone()),
            ))
            .unwrap_or_else(|errors| panic!("{operation}: {errors:?}"));
            let wrong = if valid == l::CountAction::Uncounted {
                l::CountAction::Counted(subscript_compiler::types::CountedType::Handle)
            } else {
                l::CountAction::Uncounted
            };
            for action in [
                None,
                Some(wrong),
                Some(l::CountAction::Counted(
                    subscript_compiler::types::CountedType::Array(Box::new(
                        subscript_compiler::types::CountedType::Handle,
                    )),
                )),
            ] {
                let errors = verify_module(&bulk_module(element.clone(), operation, action))
                    .expect_err("wrong instruction action");
                assert!(
                    errors.iter().any(|error| error
                        .message
                        .contains("block 0 instruction 0 has a missing or wrong count action")),
                    "{operation}: {errors:?}"
                );
            }
        }
    }
}

fn completion_module(result: Type, action: Option<l::CountAction>) -> l::Module {
    let mut module = module(Type::AsyncHandle(Box::new(Type::Void)), false, true, true);
    let function = &mut module.functions[0];
    function.is_async = true;
    function.can_raise = true;
    function.values[0].ty = l::ValueType::Data(Type::AsyncHandle(Box::new(result.clone())));
    function.parameters.truncate(1);
    function.values.truncate(1);
    let resume = (result != Type::Void).then_some(l::ValueId(1));
    if resume.is_some() {
        function.values.push(l::Value {
            id: l::ValueId(1),
            ty: l::ValueType::Data(result),
            fresh_owner: true,
            source_name: None,
        });
    }
    let pos = function.pos.clone();
    function.blocks[0].instructions.clear();
    function.blocks[0].terminator = l::Terminator::Suspend {
        ownership: Vec::new(),
        kind: l::SuspendKind::AsyncHandle {
            handle: l::ValueId(0),
            owned: false,
        },
        pos: pos.clone(),
        successor: l::BlockId(1),
        resume_value: resume,
        arguments: Vec::new(),
        invalidates: Vec::new(),
        traps: vec![
            l::Trap {
                kind: l::TrapKind::DevReloadOnlyStaleCoroutine,
                pos: pos.clone(),
            },
            l::Trap {
                kind: l::TrapKind::DevOnlyLifetime(0),
                pos: pos.clone(),
            },
        ],
    };
    function.blocks.push(l::BasicBlock {
        id: l::BlockId(1),
        source_name: None,
        parameters: resume.into_iter().collect(),
        instructions: vec![l::Instruction {
            result: None,
            kind: l::InstructionKind::AwaitRaise,
            count_action: action,
            operands: Vec::new(),
            invalidates: Vec::new(),
            traps: vec![l::Trap {
                kind: l::TrapKind::Raise(l::RaiseEdge::Propagate),
                pos: pos.clone(),
            }],
            pos: pos.clone(),
        }],
        terminator: l::Terminator::Return {
            value: None,
            pos: pos.clone(),
        },
    });
    if function.blocks[1].instructions[0]
        .count_action
        .as_ref()
        .is_some_and(|a| a.release_type().is_some())
    {
        function.blocks[1].instructions[0].traps.push(l::Trap {
            kind: l::TrapKind::Call,
            pos,
        });
    }
    if let Some(value) = resume {
        let kind = if matches!(
            function.values[value.0 as usize].ty,
            l::ValueType::Data(Type::AsyncHandle(_))
        ) {
            l::InstructionKind::AsyncHandleRelease
        } else {
            l::InstructionKind::AsyncHandleArrayRelease
        };
        if matches!(&function.values[value.0 as usize].ty, l::ValueType::Data(ty) if ty.counted_type().is_some())
        {
            function.blocks[1].instructions.push(l::Instruction {
                result: None,
                kind,
                count_action: None,
                operands: vec![l::Operand::Value(value)],
                invalidates: Vec::new(),
                traps: vec![
                    l::Trap {
                        kind: l::TrapKind::DevOnlyLifetime(0),
                        pos: function.pos.clone(),
                    },
                    l::Trap {
                        kind: l::TrapKind::Call,
                        pos: function.pos.clone(),
                    },
                ],
                pos: function.pos.clone(),
            });
        }
    }
    module
}

#[test]
fn completion_reads_require_the_instruction_action_at_every_depth() {
    for result in [
        Type::Void,
        Type::I32,
        Type::AsyncHandle(Box::new(Type::Void)),
        Type::Array(Box::new(Type::Array(Box::new(Type::AsyncHandle(
            Box::new(Type::Void),
        ))))),
    ] {
        let valid = l::CountAction::for_type(&result);
        verify_module(&completion_module(result.clone(), Some(valid.clone()))).expect("valid read");
        let wrong = if valid == l::CountAction::Uncounted {
            l::CountAction::Counted(subscript_compiler::types::CountedType::Handle)
        } else {
            l::CountAction::Uncounted
        };
        for action in [
            None,
            Some(wrong),
            Some(l::CountAction::Counted(
                subscript_compiler::types::CountedType::Array(Box::new(
                    subscript_compiler::types::CountedType::Handle,
                )),
            )),
        ] {
            let errors = verify_module(&completion_module(result.clone(), action))
                .expect_err("wrong instruction action");
            assert!(
                errors.iter().any(|error| error
                    .message
                    .contains("block 1 instruction 0 has a missing or wrong count action")),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn counted_actions_require_a_release_trap_site() {
    let element = Type::AsyncHandle(Box::new(Type::Void));
    for operation in ["Fill", "CopyWithin", "Slice", "Concat", "Spread"] {
        let mut form = bulk_module(
            element.clone(),
            operation,
            Some(l::CountAction::for_type(&element)),
        );
        form.functions[0].blocks[0].instructions[0]
            .traps
            .retain(|trap| trap.kind != l::TrapKind::Call);
        let errors = verify_module(&form).expect_err("missing release trap");
        assert!(
            errors.iter().any(|e| e
                .message
                .contains("instruction 0 releases a counted value without a Call trap")),
            "{errors:?}"
        );
    }
}

#[test]
fn a_fresh_builtin_receiver_requires_a_consumer() {
    let mut form = module(Type::AsyncHandle(Box::new(Type::Void)), true, false, true);
    form.functions[0].parameters.remove(0);
    if let l::ValueType::Address(address) = &mut form.functions[0].values[1].ty {
        address.array_base = None;
    }
    form.functions[0].values[0].fresh_owner = true;
    let pos = form.functions[0].pos.clone();
    form.functions[0].blocks[0].instructions.insert(
        0,
        l::Instruction {
            result: Some(l::ValueId(0)),
            kind: l::InstructionKind::ArrayLiteral,
            count_action: None,
            operands: Vec::new(),
            invalidates: Vec::new(),
            traps: vec![l::Trap {
                kind: l::TrapKind::Allocation,
                pos: pos.clone(),
            }],
            pos: pos.clone(),
        },
    );
    let errors = verify_module(&form).expect_err("receiver has no release");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("fresh counted owner 0 is not consumed")),
        "{errors:?}"
    );
}

#[test]
fn a_fresh_builtin_receiver_with_a_release_passes() {
    let mut form = module(Type::AsyncHandle(Box::new(Type::Void)), true, false, true);
    form.functions[0].parameters.remove(0);
    if let l::ValueType::Address(address) = &mut form.functions[0].values[1].ty {
        address.array_base = None;
    }
    form.functions[0].values[0].fresh_owner = true;
    let pos = form.functions[0].pos.clone();
    form.functions[0].blocks[0].instructions.insert(
        0,
        l::Instruction {
            result: Some(l::ValueId(0)),
            kind: l::InstructionKind::ArrayLiteral,
            count_action: None,
            operands: Vec::new(),
            invalidates: Vec::new(),
            traps: vec![l::Trap {
                kind: l::TrapKind::Allocation,
                pos: pos.clone(),
            }],
            pos: pos.clone(),
        },
    );
    form.functions[0].blocks[0]
        .instructions
        .push(l::Instruction {
            result: None,
            kind: l::InstructionKind::AsyncHandleArrayRelease,
            count_action: None,
            operands: vec![l::Operand::Value(l::ValueId(0))],
            invalidates: Vec::new(),
            traps: vec![
                l::Trap {
                    kind: l::TrapKind::DevOnlyLifetime(0),
                    pos: pos.clone(),
                },
                l::Trap {
                    kind: l::TrapKind::Call,
                    pos: pos.clone(),
                },
            ],
            pos,
        });
    verify_module(&form).expect("receiver release consumes the fresh owner");
}

fn clear_module(element: Type, action: Option<l::CountAction>, trap: bool) -> l::Module {
    // Build a clear form directly. No checker or lowering supplies its obligations.
    let mut form = module(element.clone(), true, false, false);
    let function = &mut form.functions[0];
    function.parameters.truncate(1);
    function.values.truncate(1);
    let pos = function.pos.clone();
    function.blocks[0].instructions = vec![l::Instruction {
        result: None,
        kind: l::InstructionKind::Call(l::CallTarget {
            kind: l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayClear),
            parameter_types: Vec::new(),
            return_type: None,
        }),
        operands: vec![l::Operand::Value(l::ValueId(0))],
        count_action: action,
        invalidates: vec![l::ValueId(0)],
        traps: std::iter::once(l::Trap {
            kind: l::TrapKind::DevOnlyLifetime(0),
            pos: pos.clone(),
        })
        .chain(trap.then_some(l::Trap {
            kind: l::TrapKind::Call,
            pos: pos.clone(),
        }))
        .collect(),
        pos,
    }];
    form.intrinsic_operations[0].signatures = vec![l::CallSignature {
        target: l::CallSignatureTarget::BuiltinMethod(l::BuiltinMethod::ArrayClear),
        parameter_types: vec![l::ValueType::Data(Type::Array(Box::new(element)))],
        return_type: None,
    }];
    form
}

#[test]
fn a_clear_requires_the_static_count_action_and_release_trap_at_every_depth() {
    for element in [
        Type::I32,
        Type::async_handle(Type::Void),
        Type::Array(Box::new(Type::async_handle(Type::Void))),
        Type::Array(Box::new(Type::Array(Box::new(Type::async_handle(
            Type::Void,
        ))))),
    ] {
        let action = l::CountAction::for_type(&element);
        verify_module(&clear_module(element.clone(), Some(action.clone()), true))
            .expect("same-shape clear control");
        let errors =
            verify_module(&clear_module(element.clone(), None, true)).expect_err("missing action");
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("missing or wrong count action")),
            "{errors:?}"
        );
        if element.counted_type().is_some() {
            let errors = verify_module(&clear_module(
                element.clone(),
                Some(l::CountAction::Uncounted),
                true,
            ))
            .expect_err("wrong action");
            assert!(
                errors
                    .iter()
                    .any(|error| error.message.contains("missing or wrong count action")),
                "{errors:?}"
            );
            let errors = verify_module(&clear_module(element, Some(action), false))
                .expect_err("missing trap");
            assert!(
                errors
                    .iter()
                    .any(|error| error.message.contains("without a Call trap")),
                "{errors:?}"
            );
        }
    }
}
