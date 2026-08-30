//! Independent HIR/LIR execution-fact comparison for §68.2 item 12.

use std::collections::BTreeMap;

use subscript_compiler::hir;
use subscript_compiler::lir as l;
use subscript_compiler::{ClassId, Pos, Type};

/// Returns every HIR execution fact that the supplied LIR module drops.
pub fn dropped_facts(hir: &hir::Module, lir: &l::Module) -> Vec<String> {
    let mut findings = Vec::new();
    compare_declaration_entities(hir, lir, &mut findings);
    compare_function_entities(hir, lir, &mut findings);
    compare_entry_and_async_roots(hir, lir, &mut findings);
    compare_traps(hir, lir, &mut findings);
    compare_terminator_positions(hir, lir, &mut findings);
    compare_boundary_boxes(hir, lir, &mut findings);
    compare_foreign_array_snapshots(hir, lir, &mut findings);
    compare_call_operands(hir, lir, &mut findings);
    compare_iterator_bounds(hir, lir, &mut findings);
    compare_static_array_callbacks(hir, lir, &mut findings);
    compare_instruction_operands(lir, &mut findings);
    findings
}

#[derive(Clone)]
struct ExpectedIteratorBound {
    bound: l::IteratorBoundKind,
    spelling: &'static str,
    count: usize,
}

fn compare_iterator_bounds(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    let mut expected = BTreeMap::<(String, u32, u32), ExpectedIteratorBound>::new();
    let mut collect_for_of = |statements: &[hir::Stmt]| {
        collect_for_of_bounds(statements, &mut expected);
    };
    for function in all_declared_functions(hir) {
        collect_for_of(&function.body);
    }
    collect_for_of(&hir.top_level);
    walk_module_expressions(hir, &mut |expr| {
        if let hir::ExprKind::Lambda { body, .. } = &expr.kind {
            collect_for_of_bounds(body, &mut expected);
        }
        let hir::ExprKind::Call { callee, args } = &expr.kind else {
            return;
        };
        let fact = match callee {
            hir::Callee::Arr(operation) if static_array_callback(*operation, args).is_some() => {
                let callback = static_array_callback(*operation, args)
                    .expect("guard established a static callback");
                let indexed = matches!(&callback.ty, Type::Func(signature)
                    if operation.callback_index_arity() == Some(signature.params.len()));
                Some(ExpectedIteratorBound {
                    bound: l::IteratorBoundKind::Fixed,
                    spelling: "static Array callback",
                    count: 1 + usize::from(*operation == hir::ArrFn::ReduceRight && indexed),
                })
            }
            hir::Callee::Map(hir::MapFn::ForEach) => Some(ExpectedIteratorBound {
                bound: l::IteratorBoundKind::Live,
                spelling: "Map.forEach",
                count: 2,
            }),
            hir::Callee::Set(hir::SetFn::ForEach) => Some(ExpectedIteratorBound {
                bound: l::IteratorBoundKind::Live,
                spelling: "Set.forEach",
                count: 1,
            }),
            _ => None,
        };
        if let Some(fact) = fact {
            expected.insert(pos_key(&expr.pos), fact);
        }
    });

    let mut actual_positions = BTreeMap::<(String, u32, u32), usize>::new();
    for instruction in lir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
    {
        let l::InstructionKind::IteratorCreate { bound, .. } = instruction.kind else {
            continue;
        };
        let key = pos_key(&instruction.pos);
        *actual_positions.entry(key.clone()).or_default() += 1;
        match expected.get(&key) {
            Some(required) if required.bound != bound => findings.push(format!(
                "{}: iterator bound {bound:?} disagrees with {} spelling, which requires {:?}",
                instruction.pos, required.spelling, required.bound
            )),
            Some(_) => {}
            None => findings.push(format!(
                "{}: iterator bound {bound:?} has no for-of or forEach spelling in HIR",
                instruction.pos
            )),
        }
    }
    for (key, required) in expected {
        let carried = actual_positions.get(&key).copied().unwrap_or(0);
        if carried != required.count {
            findings.push(format!(
                "{}:{}:{}: {} spelling carries {carried} {:?} iterator cursor(s); HIR requires {}",
                key.0, key.1, key.2, required.spelling, required.bound, required.count
            ));
        }
    }
}

fn collect_for_of_bounds(
    statements: &[hir::Stmt],
    expected: &mut BTreeMap<(String, u32, u32), ExpectedIteratorBound>,
) {
    for statement in statements {
        match statement {
            hir::Stmt::ForOf { body, pos, .. } => {
                expected.insert(
                    pos_key(pos),
                    ExpectedIteratorBound {
                        bound: l::IteratorBoundKind::Live,
                        spelling: "for-of",
                        count: 1,
                    },
                );
                collect_for_of_bounds(body, expected);
            }
            hir::Stmt::If { then, els, .. } => {
                collect_for_of_bounds(then, expected);
                if let Some(els) = els {
                    collect_for_of_bounds(els, expected);
                }
            }
            hir::Stmt::While { body, .. }
            | hir::Stmt::For { body, .. }
            | hir::Stmt::Block(body) => collect_for_of_bounds(body, expected),
            hir::Stmt::Switch { cases, .. } => {
                for case in cases {
                    collect_for_of_bounds(&case.body, expected);
                }
            }
            hir::Stmt::Let { .. }
            | hir::Stmt::Expr(_)
            | hir::Stmt::Return { .. }
            | hir::Stmt::Break(_)
            | hir::Stmt::Continue(_) => {}
        }
    }
}

fn static_array_callback(operation: hir::ArrFn, args: &[hir::Expr]) -> Option<&hir::Expr> {
    if !matches!(
        operation,
        hir::ArrFn::Map
            | hir::ArrFn::Filter
            | hir::ArrFn::Reduce
            | hir::ArrFn::ReduceRight
            | hir::ArrFn::ForEach
            | hir::ArrFn::Some
            | hir::ArrFn::Every
            | hir::ArrFn::FindIndex
    ) || !matches!(
        args.first().map(|argument| &argument.ty),
        Some(Type::Array(_))
    ) {
        return None;
    }
    let callback = args.get(1)?;
    matches!(
        callback.kind,
        hir::ExprKind::FuncRef(_) | hir::ExprKind::Lambda { .. }
    )
    .then_some(callback)
}

fn compare_static_array_callbacks(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    walk_module_expressions(hir, &mut |expr| {
        let hir::ExprKind::Call {
            callee: hir::Callee::Arr(operation),
            args,
        } = &expr.kind
        else {
            return;
        };
        if !matches!(
            operation,
            hir::ArrFn::Map
                | hir::ArrFn::Filter
                | hir::ArrFn::Reduce
                | hir::ArrFn::ReduceRight
                | hir::ArrFn::ForEach
                | hir::ArrFn::Some
                | hir::ArrFn::Every
                | hir::ArrFn::FindIndex
        ) || !matches!(
            args.first().map(|argument| &argument.ty),
            Some(Type::Array(_))
        ) {
            return;
        }
        if args.get(1).is_none() {
            return;
        }
        let key = pos_key(&expr.pos);
        let instructions = lir
            .functions
            .iter()
            .flat_map(|function| {
                function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .map(move |instruction| (function, instruction))
            })
            .filter(|(_, instruction)| pos_key(&instruction.pos) == key)
            .collect::<Vec<_>>();
        let operation_id = hir::ArrFn::ALL
            .iter()
            .position(|candidate| candidate == operation)
            .map(|index| index as u16);
        let intrinsic_count = instructions
            .iter()
            .filter(|(_, instruction)| {
                matches!(&instruction.kind, l::InstructionKind::Call(target)
                    if matches!(&target.kind, l::CallTargetKind::Intrinsic(intrinsic)
                        if intrinsic.family == l::IntrinsicFamily::Array
                            && Some(intrinsic.operation) == operation_id))
            })
            .count();

        let Some(static_callback) = static_array_callback(*operation, args) else {
            if intrinsic_count != 1 {
                findings.push(format!(
                    "{}: Array callback function value carries {intrinsic_count} runtime intrinsic call(s); HIR requires 1",
                    expr.pos
                ));
            }
            return;
        };
        if intrinsic_count != 0 {
            findings.push(format!(
                "{}: static Array callback keeps {intrinsic_count} runtime intrinsic call(s)",
                expr.pos
            ));
        }
        let direct = instructions
            .iter()
            .filter(|(_, instruction)| {
                let l::InstructionKind::Call(target) = &instruction.kind else {
                    return false;
                };
                match (&static_callback.kind, &target.kind) {
                    (hir::ExprKind::FuncRef(name), l::CallTargetKind::Function(id)) => {
                        lir.functions.get(id.0 as usize).is_some_and(|function| {
                            function.id == *id
                                && function.kind == l::FunctionKind::Free
                                && function.source_name == *name
                        })
                    }
                    (hir::ExprKind::Lambda { .. }, l::CallTargetKind::StaticClosure(id)) => {
                        lir.functions.get(id.0 as usize).is_some_and(|function| {
                            function.id == *id
                                && function.kind == l::FunctionKind::Lambda
                                && function.pos == static_callback.pos
                        })
                    }
                    _ => false,
                }
            })
            .count();
        if direct != 1 {
            findings.push(format!(
                "{}: static Array callback carries {direct} matching direct call target(s); HIR requires 1",
                expr.pos
            ));
        }
        let output_count = instructions
            .iter()
            .filter(|(_, instruction)| instruction.kind == l::InstructionKind::ArrayWithCapacity)
            .count();
        let push_count = instructions
            .iter()
            .filter(|(_, instruction)| {
                matches!(&instruction.kind,
                l::InstructionKind::Call(target)
                    if target.kind == l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayPush))
            })
            .count();
        let produces_array = matches!(operation, hir::ArrFn::Map | hir::ArrFn::Filter);
        let required = usize::from(produces_array);
        if output_count != required || push_count != required {
            findings.push(format!(
                "{}: static {operation:?} carries {output_count} capacity allocation(s) and {push_count} push site(s); HIR requires {required} each",
                expr.pos
            ));
        }
    });
}

fn compare_foreign_array_snapshots(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    let mut actual = BTreeMap::<(String, u32, u32, String), usize>::new();
    for function in &lir.functions {
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                if instruction.kind != l::InstructionKind::ForeignArrayData {
                    continue;
                }
                let Some(result) = instruction.result else {
                    findings.push(format!(
                        "{}: foreign array data snapshot has no result",
                        instruction.pos
                    ));
                    continue;
                };
                let Some(l::ValueType::Address(address)) = function
                    .values
                    .get(result.0 as usize)
                    .map(|value| &value.ty)
                else {
                    findings.push(format!(
                        "{}: foreign array data snapshot has no address type",
                        instruction.pos
                    ));
                    continue;
                };
                let Some(count) = block.instructions.get(index + 1) else {
                    findings.push(format!(
                        "{}: foreign array data snapshot has no adjacent count snapshot",
                        instruction.pos
                    ));
                    continue;
                };
                let count_type = count
                    .result
                    .and_then(|result| function.values.get(result.0 as usize))
                    .map(|value| &value.ty);
                if count.kind != l::InstructionKind::Length
                    || count.pos != instruction.pos
                    || count.operands != instruction.operands
                    || count_type != Some(&l::ValueType::Data(subscript_compiler::Type::I32))
                {
                    findings.push(format!(
                        "{}: foreign array pointer/count snapshots do not form one call-time pair",
                        instruction.pos
                    ));
                    continue;
                }
                *actual
                    .entry((
                        instruction.pos.file.clone(),
                        instruction.pos.line,
                        instruction.pos.col,
                        format!("{:?}", address.pointee),
                    ))
                    .or_default() += 1;
            }

            for (call_index, instruction) in block.instructions.iter().enumerate() {
                let l::InstructionKind::Call(target) = &instruction.kind else {
                    continue;
                };
                let l::CallTargetKind::Foreign(id) = target.kind else {
                    continue;
                };
                let Some(declaration) = lir
                    .foreign_functions
                    .get(id.0 as usize)
                    .filter(|declaration| declaration.id == id)
                else {
                    continue;
                };
                let mut cursor = 0usize;
                let mut array_operands = Vec::new();
                for parameter in &declaration.parameters {
                    if let subscript_compiler::Type::Array(element) = &parameter.ty {
                        let expected_data = l::ValueType::Address(l::AddressType {
                            pointee: (**element).clone(),
                            array_base: None,
                        });
                        let data = instruction
                            .operands
                            .get(cursor)
                            .and_then(|operand| lir_operand_type(function, operand));
                        let count = instruction
                            .operands
                            .get(cursor + 1)
                            .and_then(|operand| lir_operand_type(function, operand));
                        if data != Some(&expected_data)
                            || count != Some(&l::ValueType::Data(subscript_compiler::Type::I32))
                        {
                            findings.push(format!(
                                "{}: foreign array parameter `{}` does not carry data/count snapshot operands",
                                instruction.pos, parameter.source_name
                            ));
                        }
                        array_operands.push((
                            instruction.operands.get(cursor),
                            instruction.operands.get(cursor + 1),
                        ));
                        cursor += 2;
                    } else {
                        cursor += 1;
                    }
                }
                let snapshot_instruction_count = array_operands.len() * 2;
                let trailing = call_index
                    .checked_sub(snapshot_instruction_count)
                    .and_then(|start| block.instructions.get(start..call_index));
                let ordered = trailing.is_some_and(|trailing| {
                    trailing.chunks_exact(2).zip(&array_operands).all(
                        |(pair, (data_operand, count_operand))| {
                            pair[0].kind == l::InstructionKind::ForeignArrayData
                                && pair[1].kind == l::InstructionKind::Length
                                && pair[0].operands == pair[1].operands
                                && pair[0].result.map(l::Operand::Value).as_ref() == *data_operand
                                && pair[1].result.map(l::Operand::Value).as_ref() == *count_operand
                        },
                    )
                });
                if !array_operands.is_empty() && !ordered {
                    findings.push(format!(
                        "{}: foreign array data/count operands are not read after all arguments and immediately before the call",
                        instruction.pos
                    ));
                }
            }
        }
    }

    walk_module_expressions(hir, &mut |expr| {
        let hir::ExprKind::Call {
            callee: hir::Callee::Foreign(name),
            args,
        } = &expr.kind
        else {
            return;
        };
        let Some(function) = hir
            .foreign_fns
            .iter()
            .find(|function| function.name == *name)
        else {
            return;
        };
        for (index, parameter) in function.params.iter().enumerate() {
            let subscript_compiler::Type::Array(element) = &parameter.ty else {
                continue;
            };
            let Some(argument) = args.get(index).or(parameter.default.as_ref()) else {
                findings.push(format!(
                    "{}: foreign array parameter `{}` has no argument",
                    expr.pos, parameter.name
                ));
                continue;
            };
            let key = (
                argument.pos.file.clone(),
                argument.pos.line,
                argument.pos.col,
                format!("{:?}", element),
            );
            match actual.get_mut(&key) {
                Some(count) if *count != 0 => *count -= 1,
                _ => findings.push(format!(
                    "{}: foreign array argument carries no call-time data/count snapshot",
                    argument.pos
                )),
            }
        }
    });
}

fn lir_operand_type<'a>(
    function: &'a l::Function,
    operand: &l::Operand,
) -> Option<&'a l::ValueType> {
    match operand {
        l::Operand::Value(value) => function.values.get(value.0 as usize).map(|value| &value.ty),
        l::Operand::Constant(_) => None,
    }
}

fn compare_boundary_boxes(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    let mut actual = BTreeMap::<(String, u32, u32), usize>::new();
    for function in &lir.functions {
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            if !matches!(
                instruction.kind,
                l::InstructionKind::BoxBoundaryValue { .. }
            ) {
                continue;
            }
            let Some(l::Operand::Value(value)) = instruction.operands.first() else {
                continue;
            };
            let Some(l::ValueType::Data(source_ty)) =
                function.values.get(value.0 as usize).map(|value| &value.ty)
            else {
                continue;
            };
            let Some(result_ty) = instruction
                .result
                .and_then(|result| function.values.get(result.0 as usize))
                .map(|value| &value.ty)
            else {
                continue;
            };
            if !lir_boundary_box(lir, source_ty, result_ty) {
                continue;
            }
            *actual
                .entry((
                    instruction.pos.file.clone(),
                    instruction.pos.line,
                    instruction.pos.col,
                ))
                .or_default() += 1;
        }
    }

    walk_module_expressions(hir, &mut |expr| {
        let mut require_box = |parameter: &subscript_compiler::Type, argument: &hir::Expr| {
            if !hir_boundary_pointer_type(hir, parameter)
                || !matches!(argument.ty, subscript_compiler::Type::Class(_))
            {
                return;
            }
            let key = (
                argument.pos.file.clone(),
                argument.pos.line,
                argument.pos.col,
            );
            match actual.get_mut(&key) {
                Some(count) if *count != 0 => *count -= 1,
                _ => findings.push(format!(
                    "{}: boundary-pointer argument carries no managed box in LIR",
                    argument.pos
                )),
            }
        };
        match &expr.kind {
            hir::ExprKind::New { class, args } => {
                let Some(definition) = hir
                    .classes
                    .get(class.0)
                    .filter(|definition| definition.is_boundary)
                else {
                    return;
                };
                for (field, argument) in definition.fields.iter().zip(args) {
                    require_box(&field.ty, argument);
                }
            }
            hir::ExprKind::Call { .. }
            | hir::ExprKind::Int(_)
            | hir::ExprKind::Float(_)
            | hir::ExprKind::Bool(_)
            | hir::ExprKind::Str(_)
            | hir::ExprKind::Null
            | hir::ExprKind::This
            | hir::ExprKind::Local(_)
            | hir::ExprKind::Global(_)
            | hir::ExprKind::FuncRef(_)
            | hir::ExprKind::EnumMember { .. }
            | hir::ExprKind::Unary { .. }
            | hir::ExprKind::Binary { .. }
            | hir::ExprKind::Assign { .. }
            | hir::ExprKind::Cast(_)
            | hir::ExprKind::DescriptorLit { .. }
            | hir::ExprKind::Zero
            | hir::ExprKind::RawNew { .. }
            | hir::ExprKind::Field { .. }
            | hir::ExprKind::JsonResultValue(_)
            | hir::ExprKind::Length(_)
            | hir::ExprKind::Index { .. }
            | hir::ExprKind::ArrayLit(_)
            | hir::ExprKind::ArraySpreadLit(_)
            | hir::ExprKind::Template(_)
            | hir::ExprKind::Lambda { .. }
            | hir::ExprKind::Yield(_)
            | hir::ExprKind::AsyncSuspend
            | hir::ExprKind::AsyncCall { .. }
            | hir::ExprKind::AsyncHandleCreate { .. }
            | hir::ExprKind::AsyncHandleAwait(_)
            | hir::ExprKind::AsyncHandleTransfer { .. }
            | hir::ExprKind::Cond { .. } => {}
        }
    });
}

fn hir_boundary_pointer_type(hir: &hir::Module, ty: &subscript_compiler::Type) -> bool {
    matches!(ty, subscript_compiler::Type::Nullable(inner)
    if matches!(&**inner, subscript_compiler::Type::Class(class)
        if hir.classes.get(class.0).is_some_and(|definition| {
            definition.is_value && definition.is_boundary
        })))
}

fn lir_boundary_box(
    lir: &l::Module,
    source: &subscript_compiler::Type,
    result: &l::ValueType,
) -> bool {
    let (
        subscript_compiler::Type::Class(source),
        l::ValueType::Data(subscript_compiler::Type::Nullable(target)),
    ) = (source, result)
    else {
        return false;
    };
    let subscript_compiler::Type::Class(target) = target.as_ref() else {
        return false;
    };
    if source == target {
        return lir
            .classes
            .get(target.0)
            .is_some_and(|definition| definition.is_value && definition.is_boundary);
    }
    lir.classes.get(source.0).is_some_and(|definition| {
        definition.is_value
            && definition.is_boundary
            && definition.fields.first().is_some_and(|field| {
                field.ty == subscript_compiler::Type::Class(*target)
                    && lir
                        .classes
                        .get(target.0)
                        .is_some_and(|class| class.is_embedded_header)
            })
    })
}

fn compare_terminator_positions(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    let mut actual = BTreeMap::<(String, u32, u32), usize>::new();
    for function in &lir.functions {
        for block in &function.blocks {
            let carries_fact_position = match &block.terminator {
                l::Terminator::Suspend { .. } | l::Terminator::Trap(_) => true,
                l::Terminator::Return { .. } => {
                    matches!(&function.return_type, subscript_compiler::Type::Class(class)
                        if lir_boundary_class_contains_pointer(lir, *class, &mut Vec::new()))
                }
                l::Terminator::Branch(_)
                | l::Terminator::ConditionalBranch { .. }
                | l::Terminator::Switch { .. }
                | l::Terminator::Unreachable { .. } => false,
            };
            if let Some(pos) = carries_fact_position
                .then(|| block.terminator.trap_site_position())
                .flatten()
            {
                *actual
                    .entry((pos.file.clone(), pos.line, pos.col))
                    .or_default() += 1;
            }
        }
    }

    let mut expected = Vec::<Pos>::new();
    walk_module_expressions(hir, &mut |expr| {
        if expression_owns_terminator_position(expr) {
            expected.push(expr.pos.clone());
        }
        if let hir::ExprKind::Lambda { ret, body, .. } = &expr.kind {
            if matches!(ret, subscript_compiler::Type::Class(class)
                if hir_boundary_class_contains_pointer(hir, *class, &mut Vec::new()))
            {
                collect_return_positions(body, &mut |pos| {
                    expected.push(pos.clone());
                });
            }
        }
    });

    for function in all_declared_functions(hir) {
        let subscript_compiler::Type::Class(class) = &function.ret else {
            continue;
        };
        if !hir_boundary_class_contains_pointer(hir, *class, &mut Vec::new()) {
            continue;
        }
        collect_return_positions(&function.body, &mut |pos| {
            expected.push(pos.clone());
        });
    }

    for pos in expected {
        let key = (pos.file.clone(), pos.line, pos.col);
        match actual.get_mut(&key) {
            Some(count) if *count != 0 => *count -= 1,
            _ => findings.push(format!(
                "{pos}: terminator trap-site position is absent from LIR"
            )),
        }
    }
    for ((file, line, col), count) in actual {
        for _ in 0..count {
            findings.push(format!(
                "{file}:{line}:{col}: LIR terminator trap-site position is absent from HIR"
            ));
        }
    }
}

fn expression_owns_terminator_position(expr: &hir::Expr) -> bool {
    use hir::ExprKind as K;
    match &expr.kind {
        K::Yield(_) | K::AsyncSuspend | K::AsyncCall { .. } | K::AsyncHandleAwait(_) => true,
        K::Call {
            callee: hir::Callee::Ambient(hir::AmbientFn::Unreachable),
            ..
        } => true,
        K::Call { .. }
        | K::Int(_)
        | K::Float(_)
        | K::Bool(_)
        | K::Str(_)
        | K::Null
        | K::This
        | K::Local(_)
        | K::Global(_)
        | K::FuncRef(_)
        | K::EnumMember { .. }
        | K::Unary { .. }
        | K::Binary { .. }
        | K::Assign { .. }
        | K::Cast(_)
        | K::New { .. }
        | K::DescriptorLit { .. }
        | K::Zero
        | K::RawNew { .. }
        | K::Field { .. }
        | K::JsonResultValue(_)
        | K::Length(_)
        | K::Index { .. }
        | K::ArrayLit(_)
        | K::ArraySpreadLit(_)
        | K::Template(_)
        | K::Lambda { .. }
        | K::AsyncHandleCreate { .. }
        | K::AsyncHandleTransfer { .. }
        | K::Cond { .. } => false,
    }
}

fn hir_boundary_class_contains_pointer(
    hir: &hir::Module,
    class: ClassId,
    visiting: &mut Vec<ClassId>,
) -> bool {
    if visiting.contains(&class) {
        return false;
    }
    visiting.push(class);
    let contains = hir
        .classes
        .get(class.0)
        .filter(|definition| definition.is_value)
        .is_some_and(|definition| {
            definition.fields.iter().any(|field| match &field.ty {
                subscript_compiler::Type::Nullable(inner) => matches!(
                    &**inner,
                    subscript_compiler::Type::Class(inner)
                        if hir.classes.get(inner.0).is_some_and(|class| class.is_value)
                ),
                subscript_compiler::Type::Class(inner) => {
                    hir_boundary_class_contains_pointer(hir, *inner, visiting)
                }
                subscript_compiler::Type::Array(inner) => match &**inner {
                    subscript_compiler::Type::Class(inner) => {
                        hir_boundary_class_contains_pointer(hir, *inner, visiting)
                    }
                    _ => false,
                },
                _ => false,
            })
        });
    visiting.pop();
    contains
}

fn lir_boundary_class_contains_pointer(
    lir: &l::Module,
    class: ClassId,
    visiting: &mut Vec<ClassId>,
) -> bool {
    if visiting.contains(&class) {
        return false;
    }
    visiting.push(class);
    let contains = lir
        .classes
        .get(class.0)
        .filter(|definition| definition.is_value)
        .is_some_and(|definition| {
            definition.fields.iter().any(|field| match &field.ty {
                subscript_compiler::Type::Nullable(inner) => matches!(
                    &**inner,
                    subscript_compiler::Type::Class(inner)
                        if lir.classes.get(inner.0).is_some_and(|class| class.is_value)
                ),
                subscript_compiler::Type::Class(inner) => {
                    lir_boundary_class_contains_pointer(lir, *inner, visiting)
                }
                subscript_compiler::Type::Array(inner) => match &**inner {
                    subscript_compiler::Type::Class(inner) => {
                        lir_boundary_class_contains_pointer(lir, *inner, visiting)
                    }
                    _ => false,
                },
                _ => false,
            })
        });
    visiting.pop();
    contains
}

fn compare_declaration_entities(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    if hir.classes.len() != lir.classes.len() {
        findings.push(format!(
            "<module>: class table has {} entities for {} HIR declarations",
            lir.classes.len(),
            hir.classes.len()
        ));
    }
    let mut next_field = 0_u32;
    let mut next_method = 0_u32;
    for (class_index, class) in hir.classes.iter().enumerate() {
        let expected_id = ClassId(class_index);
        let Some(lowered) = lir.classes.get(class_index) else {
            findings.push(format!(
                "{}: class entity id {:?} is absent",
                class.pos, expected_id
            ));
            next_field += class.fields.len() as u32;
            next_method += u32::from(class.ctor.is_some()) + class.methods.len() as u32;
            continue;
        };
        if lowered.id != expected_id {
            findings.push(format!(
                "{}: class entity id is {:?}, expected {:?}",
                class.pos, lowered.id, expected_id
            ));
        }
        for (field_index, field) in class.fields.iter().enumerate() {
            let expected = l::FieldId(next_field);
            next_field += 1;
            match lowered.fields.get(field_index) {
                Some(actual) if actual.id == expected => {}
                Some(actual) => findings.push(format!(
                    "{}: field entity id is {:?}, expected {:?}",
                    field.pos, actual.id, expected
                )),
                None => findings.push(format!(
                    "{}: field entity id {:?} is absent",
                    field.pos, expected
                )),
            }
        }
        if let Some(constructor) = &class.ctor {
            let expected = l::MethodId(next_method);
            next_method += 1;
            match &lowered.constructor {
                Some(actual) if actual.id == expected => {}
                Some(actual) => findings.push(format!(
                    "{}: constructor entity id is {:?}, expected {:?}",
                    constructor.pos, actual.id, expected
                )),
                None => findings.push(format!(
                    "{}: constructor entity id {:?} is absent",
                    constructor.pos, expected
                )),
            }
        }
        for (method_index, method) in class.methods.iter().enumerate() {
            let expected = l::MethodId(next_method);
            next_method += 1;
            match lowered.methods.get(method_index) {
                Some(actual) if actual.id == expected => {}
                Some(actual) => findings.push(format!(
                    "{}: method entity id is {:?}, expected {:?}",
                    method.pos, actual.id, expected
                )),
                None => findings.push(format!(
                    "{}: method entity id {:?} is absent",
                    method.pos, expected
                )),
            }
        }
    }

    compare_indexed_table(
        "enum",
        hir.enums.iter().map(|entity| &entity.pos),
        lir.enums.iter().map(|entity| (entity.id.0, &entity.pos)),
        findings,
    );
    compare_indexed_table(
        "string alias",
        hir.string_aliases.iter().map(|entity| &entity.pos),
        lir.string_aliases
            .iter()
            .map(|entity| (entity.id.0, &entity.pos)),
        findings,
    );
    compare_indexed_table(
        "global",
        hir.globals.iter().map(|entity| &entity.pos),
        lir.globals
            .iter()
            .map(|entity| (entity.id.0 as usize, &entity.pos)),
        findings,
    );
    compare_indexed_table(
        "foreign function",
        hir.foreign_fns.iter().map(|entity| &entity.pos),
        lir.foreign_functions
            .iter()
            .map(|entity| (entity.id.0 as usize, &entity.pos)),
        findings,
    );
}

fn compare_indexed_table<'a>(
    label: &str,
    expected: impl Iterator<Item = &'a Pos>,
    actual: impl Iterator<Item = (usize, &'a Pos)>,
    findings: &mut Vec<String>,
) {
    let expected = expected.collect::<Vec<_>>();
    let actual = actual.collect::<Vec<_>>();
    if expected.len() != actual.len() {
        findings.push(format!(
            "<module>: {label} table has {} entities for {} HIR declarations",
            actual.len(),
            expected.len()
        ));
    }
    for (index, pos) in expected.into_iter().enumerate() {
        match actual.get(index) {
            Some((id, _)) if *id == index => {}
            Some((id, _)) => findings.push(format!(
                "{pos}: {label} entity id is {id}, expected {index}"
            )),
            None => findings.push(format!("{pos}: {label} entity id {index} is absent")),
        }
    }
}

fn compare_function_entities(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    let mut expected = Vec::new();
    for (class_index, class) in hir.classes.iter().enumerate() {
        if let Some(function) = &class.ctor {
            expected.push(ExpectedFunction {
                function,
                kind: ExpectedFunctionKind::Constructor(ClassId(class_index)),
            });
        }
        for function in &class.methods {
            expected.push(ExpectedFunction {
                function,
                kind: ExpectedFunctionKind::Method(ClassId(class_index)),
            });
        }
    }
    expected.extend(hir.functions.iter().map(|function| ExpectedFunction {
        function,
        kind: ExpectedFunctionKind::Free,
    }));

    for (index, expected) in expected.iter().enumerate() {
        let id = l::FunctionId(index as u32);
        let Some(actual) = lir.functions.get(index) else {
            findings.push(format!(
                "{}: function entity id {:?} is absent",
                expected.function.pos, id
            ));
            continue;
        };
        if actual.id != id {
            findings.push(format!(
                "{}: function entity id is {:?}, expected {:?}",
                expected.function.pos, actual.id, id
            ));
        }
        let kind_matches = match (&expected.kind, &actual.kind) {
            (ExpectedFunctionKind::Free, l::FunctionKind::Free) => true,
            (
                ExpectedFunctionKind::Constructor(class),
                l::FunctionKind::Constructor { class: actual, .. },
            ) => class == actual,
            (
                ExpectedFunctionKind::Method(class),
                l::FunctionKind::Method { class: actual, .. },
            ) => class == actual,
            _ => false,
        };
        if !kind_matches {
            findings.push(format!(
                "{}: function {:?} drops its declaration role",
                expected.function.pos, id
            ));
        }
        if actual.exported != expected.function.exported
            || actual.is_generator != expected.function.is_generator
            || actual.is_async != expected.function.is_async
            || actual.return_type != expected.function.ret
        {
            findings.push(format!(
                "{}: function {:?} drops an execution flag or its return type",
                expected.function.pos, id
            ));
        }
        let receiver_count = usize::from(!matches!(&expected.kind, ExpectedFunctionKind::Free));
        let actual_explicit = actual
            .parameters
            .iter()
            .filter(|parameter| parameter.kind == l::ParameterKind::Explicit)
            .collect::<Vec<_>>();
        let actual_receivers = actual
            .parameters
            .iter()
            .filter(|parameter| parameter.kind == l::ParameterKind::Receiver)
            .count();
        if actual_receivers != receiver_count
            || actual_explicit.len() != expected.function.params.len()
        {
            findings.push(format!(
                "{}: function {:?} carries {} receivers and {} explicit operands; HIR requires {} and {}",
                expected.function.pos,
                id,
                actual_receivers,
                actual_explicit.len(),
                receiver_count,
                expected.function.params.len()
            ));
        } else {
            for (parameter, actual_parameter) in
                expected.function.params.iter().zip(actual_explicit)
            {
                let actual_type = actual
                    .values
                    .get(actual_parameter.value.0 as usize)
                    .and_then(|value| match &value.ty {
                        l::ValueType::Data(ty) => Some(ty),
                        l::ValueType::Address(_) | l::ValueType::Iterator(_) => None,
                    });
                if actual_type != Some(&parameter.ty) {
                    findings.push(format!(
                        "{}: function {:?} parameter type is absent or changed",
                        parameter.pos, id
                    ));
                }
            }
        }
    }

    for (index, function) in lir.functions.iter().enumerate() {
        if function.id != l::FunctionId(index as u32) {
            findings.push(format!(
                "{}: function table position {index} carries id {:?}",
                function.pos, function.id
            ));
        }
        check_function_local_ids(function, findings);
    }

    let hir_lambdas = collect_lambdas(hir);
    let lir_lambdas = lir
        .functions
        .iter()
        .filter(|function| function.kind == l::FunctionKind::Lambda)
        .collect::<Vec<_>>();
    if hir_lambdas.len() != lir_lambdas.len() {
        findings.push(format!(
            "<module>: LIR carries {} lambda function ids for {} HIR lambdas",
            lir_lambdas.len(),
            hir_lambdas.len()
        ));
    }
    let mut actual_lambda_positions = multiset(lir_lambdas.iter().map(|function| &function.pos));
    for pos in hir_lambdas {
        if !take_multiset(&mut actual_lambda_positions, &pos_key(pos)) {
            findings.push(format!("{pos}: lambda function entity id is absent"));
        }
    }

    let needs_initializer = !hir.globals.is_empty() || !hir.top_level.is_empty();
    if lir.initializer.is_some() != needs_initializer {
        findings.push("<module>: module-initializer function id is absent or spurious".to_string());
    }
}

fn check_function_local_ids(function: &l::Function, findings: &mut Vec<String>) {
    for (index, local) in function.locals.iter().enumerate() {
        if local.id != l::LocalId(index as u32) {
            findings.push(format!(
                "{}: function {:?} local table position {index} carries id {:?}",
                local.pos, function.id, local.id
            ));
        }
    }
    for (index, value) in function.values.iter().enumerate() {
        if value.id != l::ValueId(index as u32) {
            findings.push(format!(
                "{}: function {:?} value table position {index} carries id {:?}",
                function.pos, function.id, value.id
            ));
        }
    }
    for (index, block) in function.blocks.iter().enumerate() {
        if block.id != l::BlockId(index as u32) {
            findings.push(format!(
                "{}: function {:?} block table position {index} carries id {:?}",
                function.pos, function.id, block.id
            ));
        }
    }
}

enum ExpectedFunctionKind {
    Free,
    Constructor(ClassId),
    Method(ClassId),
}

struct ExpectedFunction<'a> {
    function: &'a hir::Function,
    kind: ExpectedFunctionKind,
}

fn compare_entry_and_async_roots(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    let free_offset = hir
        .classes
        .iter()
        .map(|class| usize::from(class.ctor.is_some()) + class.methods.len())
        .sum::<usize>();
    if let Some((index, entry)) = hir
        .functions
        .iter()
        .enumerate()
        .find(|(_, function)| function.exported && function.name == "main")
    {
        let expected = Some(l::FunctionId((free_offset + index) as u32));
        if lir.entry != expected {
            findings.push(format!(
                "{}: executable entry is {:?}, expected {:?}",
                entry.pos, lir.entry, expected
            ));
        }
    } else if lir.entry.is_some() {
        findings.push(format!(
            "<module>:1:1: entryless HIR has unexpected executable entry {:?}",
            lir.entry
        ));
    }
    let expected_roots = hir
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| {
            function.exported
                && function.is_async
                && function.name != "main"
                && function.params.is_empty()
        })
        .map(|(index, _)| l::FunctionId((free_offset + index) as u32))
        .collect::<Vec<_>>();
    if lir.async_roots != expected_roots {
        let pos = hir
            .functions
            .iter()
            .find(|function| {
                function.exported
                    && function.is_async
                    && function.name != "main"
                    && function.params.is_empty()
            })
            .map_or_else(
                || Pos::new("<module>", 1, 1),
                |function| function.pos.clone(),
            );
        findings.push(format!(
            "{pos}: async roots are {:?}, expected {:?}",
            lir.async_roots, expected_roots
        ));
    }
}

fn compare_traps(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    let mut actual = BTreeMap::<TrapKey, usize>::new();
    for function in &lir.functions {
        for trap in &function.creation_traps {
            *actual.entry(lir_trap_key(trap)).or_default() += 1;
        }
        if let Some(traps) = &function.host_entry_traps {
            for trap in traps {
                *actual.entry(lir_trap_key(trap)).or_default() += 1;
            }
        }
        for block in &function.blocks {
            for instruction in &block.instructions {
                for trap in &instruction.traps {
                    *actual.entry(lir_trap_key(trap)).or_default() += 1;
                }
            }
            match &block.terminator {
                l::Terminator::Trap(trap) => {
                    *actual.entry(lir_trap_key(trap)).or_default() += 1;
                }
                l::Terminator::Suspend { traps, .. } => {
                    for trap in traps {
                        *actual.entry(lir_trap_key(trap)).or_default() += 1;
                    }
                }
                _ => {}
            }
        }
    }

    let mut expected = BTreeMap::<TrapKey, usize>::new();
    walk_execution_root_expressions(hir, &mut |expr| {
        collect_trap_expression(expr, hir, &mut expected);
    });
    for function in all_declared_functions(hir) {
        for site in function.trap_sites() {
            *expected.entry(hir_trap_key(&site)).or_default() += 1;
        }
        if let Some(sites) = function.host_entry_trap_sites(hir) {
            for site in sites {
                *expected.entry(hir_trap_key(&site)).or_default() += 1;
            }
        }
    }

    let keys = expected
        .keys()
        .chain(actual.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    for key in keys {
        let required = expected.get(&key).copied().unwrap_or(0);
        let carried = actual.get(&key).copied().unwrap_or(0);
        if carried != required {
            findings.push(format!(
                "{}:{}:{}: trap {:?} carries {carried} site(s); HIR requires {required}",
                key.file, key.line, key.col, key.kind
            ));
        }
    }

    let hir_free_functions = hir.functions.iter();
    let lir_free_functions = lir
        .functions
        .iter()
        .filter(|function| function.kind == l::FunctionKind::Free);
    for (expected_function, actual_function) in hir_free_functions.zip(lir_free_functions) {
        let expected_attachment = expected_function.host_entry_trap_sites(hir).is_some();
        let actual_attachment = actual_function.host_entry_traps.is_some();
        if expected_attachment != actual_attachment {
            findings.push(format!(
                "{}: host-entry trap attachment is {actual_attachment}; HIR requires {expected_attachment}",
                expected_function.pos
            ));
        }
    }
}

fn collect_trap_expression(
    expression: &hir::Expr,
    hir: &hir::Module,
    expected: &mut BTreeMap<TrapKey, usize>,
) {
    let mut nodes = Vec::new();
    walk_expr(expression, &mut |node| nodes.push(node));
    for node in nodes {
        if !matches!(&node.kind, hir::ExprKind::Template(parts) if parts.is_empty()) {
            for site in node.trap_sites(hir) {
                *expected.entry(hir_trap_key(&site)).or_default() += 1;
            }
            if let hir::ExprKind::Call {
                callee: hir::Callee::Arr(operation @ (hir::ArrFn::Map | hir::ArrFn::Filter)),
                args,
            } = &node.kind
            {
                if static_array_callback(*operation, args).is_some() {
                    if let Some(site) = node
                        .trap_sites(hir)
                        .into_iter()
                        .find(|site| matches!(site, hir::TrapSite::Call { .. }))
                    {
                        *expected.entry(hir_trap_key(&site)).or_default() += 1;
                    }
                }
            }
        }
        match &node.kind {
            hir::ExprKind::DescriptorLit { class, fields } => {
                if let Some(definition) = hir.classes.get(class.0) {
                    for (slot, field) in fields.iter().zip(&definition.fields) {
                        if slot.is_none() && !field.is_absence_capable {
                            if let Some(default) = &field.init {
                                collect_trap_expression(default, hir, expected);
                            }
                        }
                    }
                }
            }
            hir::ExprKind::New { class, args } => {
                if let Some(definition) = hir.classes.get(class.0) {
                    for field in &definition.fields {
                        if let Some(initializer) = &field.init {
                            collect_trap_expression(initializer, hir, expected);
                        }
                    }
                    if let Some(constructor) = &definition.ctor {
                        collect_missing_parameter_defaults(
                            &constructor.params,
                            args.len(),
                            hir,
                            expected,
                        );
                    }
                }
            }
            hir::ExprKind::Call { callee, args } => {
                let parameters = declared_callee_parameters(hir, callee);
                if let Some(parameters) = parameters {
                    collect_missing_parameter_defaults(parameters, args.len(), hir, expected);
                }
            }
            hir::ExprKind::AsyncCall { callee, args }
            | hir::ExprKind::AsyncHandleCreate { callee, args, .. } => {
                let parameters = declared_async_callee_parameters(hir, callee);
                if let Some(parameters) = parameters {
                    collect_missing_parameter_defaults(parameters, args.len(), hir, expected);
                }
            }
            hir::ExprKind::Int(_)
            | hir::ExprKind::Float(_)
            | hir::ExprKind::Bool(_)
            | hir::ExprKind::Str(_)
            | hir::ExprKind::Null
            | hir::ExprKind::This
            | hir::ExprKind::Local(_)
            | hir::ExprKind::Global(_)
            | hir::ExprKind::FuncRef(_)
            | hir::ExprKind::EnumMember { .. }
            | hir::ExprKind::Unary { .. }
            | hir::ExprKind::Binary { .. }
            | hir::ExprKind::Assign { .. }
            | hir::ExprKind::Cast(_)
            | hir::ExprKind::Zero
            | hir::ExprKind::RawNew { .. }
            | hir::ExprKind::Field { .. }
            | hir::ExprKind::JsonResultValue(_)
            | hir::ExprKind::Length(_)
            | hir::ExprKind::Index { .. }
            | hir::ExprKind::ArrayLit(_)
            | hir::ExprKind::ArraySpreadLit(_)
            | hir::ExprKind::Template(_)
            | hir::ExprKind::Lambda { .. }
            | hir::ExprKind::Yield(_)
            | hir::ExprKind::AsyncSuspend
            | hir::ExprKind::AsyncHandleAwait(_)
            | hir::ExprKind::AsyncHandleTransfer { .. }
            | hir::ExprKind::Cond { .. } => {}
        }
    }
}

fn declared_callee_parameters<'a>(
    hir: &'a hir::Module,
    callee: &'a hir::Callee,
) -> Option<&'a [hir::Param]> {
    match callee {
        hir::Callee::Func(name) => hir
            .functions
            .iter()
            .find(|function| function.name == *name)
            .map(|function| function.params.as_slice()),
        hir::Callee::Foreign(name) => hir
            .foreign_fns
            .iter()
            .find(|function| function.name == *name)
            .map(|function| function.params.as_slice()),
        hir::Callee::Method { recv, name } => {
            let subscript_compiler::Type::Class(class) = &recv.ty else {
                return None;
            };
            hir.classes
                .get(class.0)
                .and_then(|definition| {
                    definition
                        .methods
                        .iter()
                        .find(|method| method.name == *name)
                })
                .map(|function| function.params.as_slice())
        }
        hir::Callee::Ambient(_)
        | hir::Callee::ContextBytes { .. }
        | hir::Callee::Math(_)
        | hir::Callee::Num(_)
        | hir::Callee::Date(_)
        | hir::Callee::Json(_)
        | hir::Callee::Str(_)
        | hir::Callee::Regex(_)
        | hir::Callee::Arr(_)
        | hir::Callee::Map(_)
        | hir::Callee::Set(_)
        | hir::Callee::Worker(_)
        | hir::Callee::Value(_) => None,
    }
}

fn declared_async_callee_parameters<'a>(
    hir: &'a hir::Module,
    callee: &'a hir::AsyncCallee,
) -> Option<&'a [hir::Param]> {
    match callee {
        hir::AsyncCallee::Function(name) => hir
            .functions
            .iter()
            .find(|function| function.name == *name)
            .map(|function| function.params.as_slice()),
        hir::AsyncCallee::Method { class, name, .. } => hir
            .classes
            .get(class.0)
            .and_then(|definition| {
                definition
                    .methods
                    .iter()
                    .find(|method| method.name == *name)
            })
            .map(|function| function.params.as_slice()),
    }
}

fn collect_missing_parameter_defaults(
    parameters: &[hir::Param],
    supplied: usize,
    hir: &hir::Module,
    expected: &mut BTreeMap<TrapKey, usize>,
) {
    for parameter in parameters.iter().skip(supplied) {
        if let Some(default) = &parameter.default {
            collect_trap_expression(default, hir, expected);
        }
    }
}

fn walk_execution_root_expressions<'a>(
    hir: &'a hir::Module,
    visit: &mut impl FnMut(&'a hir::Expr),
) {
    for global in &hir.globals {
        visit(&global.init);
    }
    for function in all_declared_functions(hir) {
        walk_statement_expression_roots(&function.body, visit);
    }
    walk_statement_expression_roots(&hir.top_level, visit);
}

fn walk_statement_expression_roots<'a>(
    statements: &'a [hir::Stmt],
    visit: &mut impl FnMut(&'a hir::Expr),
) {
    for statement in statements {
        match statement {
            hir::Stmt::Let { init, .. } | hir::Stmt::Expr(init) => visit(init),
            hir::Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    visit(value);
                }
            }
            hir::Stmt::If {
                cond, then, els, ..
            } => {
                visit(cond);
                walk_statement_expression_roots(then, visit);
                if let Some(els) = els {
                    walk_statement_expression_roots(els, visit);
                }
            }
            hir::Stmt::While { cond, body, .. } => {
                visit(cond);
                walk_statement_expression_roots(body, visit);
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                if let Some(init) = init {
                    walk_statement_expression_roots(std::slice::from_ref(init), visit);
                }
                if let Some(cond) = cond {
                    visit(cond);
                }
                walk_statement_expression_roots(body, visit);
                if let Some(step) = step {
                    visit(step);
                }
            }
            hir::Stmt::ForOf { subject, body, .. } => {
                visit(subject);
                walk_statement_expression_roots(body, visit);
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                visit(disc);
                for case in cases {
                    if let Some(test) = &case.test {
                        visit(test);
                    }
                    walk_statement_expression_roots(&case.body, visit);
                }
            }
            hir::Stmt::Block(body) => walk_statement_expression_roots(body, visit),
            hir::Stmt::Break(_) | hir::Stmt::Continue(_) => {}
        }
        if stops_statement_sequence(statement) {
            break;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TrapKey {
    file: String,
    line: u32,
    col: u32,
    kind: String,
}

fn hir_trap_key(trap: &hir::TrapSite) -> TrapKey {
    let kind = match trap {
        hir::TrapSite::Allocation { .. } => "Allocation".to_string(),
        hir::TrapSite::Call { .. } => "Call".to_string(),
        hir::TrapSite::Unreachable { .. } => "Unreachable".to_string(),
        hir::TrapSite::DivisionByZero { .. } => "DivisionByZero".to_string(),
        hir::TrapSite::IndexRead { .. } => "IndexRead".to_string(),
        hir::TrapSite::IndexWrite { .. } => "IndexWrite".to_string(),
        hir::TrapSite::JsonResultValue { .. } => "JsonResultValue".to_string(),
        hir::TrapSite::NullNarrowing { .. } => "NullNarrowing".to_string(),
        hir::TrapSite::ClassMismatch { class, .. } => format!("ClassMismatch({})", class.0),
        hir::TrapSite::DevOnlyLifetime { .. } => "DevOnlyLifetime".to_string(),
        hir::TrapSite::DevReloadOnlyStaleCoroutine { .. } => {
            "DevReloadOnlyStaleCoroutine".to_string()
        }
        hir::TrapSite::WireEnumValue { alias, .. } => format!("WireEnumValue({})", alias.0),
    };
    trap_key(trap.pos(), kind)
}

fn lir_trap_key(trap: &l::Trap) -> TrapKey {
    let kind = match &trap.kind {
        l::TrapKind::Allocation => "Allocation".to_string(),
        l::TrapKind::Call => "Call".to_string(),
        l::TrapKind::Unreachable => "Unreachable".to_string(),
        l::TrapKind::DivisionByZero => "DivisionByZero".to_string(),
        l::TrapKind::IndexRead => "IndexRead".to_string(),
        l::TrapKind::IndexWrite => "IndexWrite".to_string(),
        l::TrapKind::JsonResultValue(_) => "JsonResultValue".to_string(),
        l::TrapKind::NullNarrowing => "NullNarrowing".to_string(),
        l::TrapKind::ClassMismatch(class) => format!("ClassMismatch({})", class.0),
        l::TrapKind::DevOnlyLifetime => "DevOnlyLifetime".to_string(),
        l::TrapKind::DevReloadOnlyStaleCoroutine => "DevReloadOnlyStaleCoroutine".to_string(),
        l::TrapKind::WireEnumValue(alias) => format!("WireEnumValue({})", alias.0),
    };
    trap_key(&trap.pos, kind)
}

fn trap_key(pos: &Pos, kind: String) -> TrapKey {
    TrapKey {
        file: pos.file.clone(),
        line: pos.line,
        col: pos.col,
        kind,
    }
}

fn compare_call_operands(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
    let mut actual = BTreeMap::<(String, u32, u32, usize), usize>::new();
    for function in &lir.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if matches!(
                    &instruction.kind,
                    l::InstructionKind::Call(_) | l::InstructionKind::AsyncHandleCreate(_)
                ) {
                    *actual
                        .entry((
                            instruction.pos.file.clone(),
                            instruction.pos.line,
                            instruction.pos.col,
                            instruction.operands.len(),
                        ))
                        .or_default() += 1;
                }
            }
            if let l::Terminator::Suspend {
                kind: l::SuspendKind::AsyncCall { operands, .. },
                ..
            } = &block.terminator
            {
                let pos = suspend_position(&block.terminator).unwrap_or_else(|| &function.pos);
                *actual
                    .entry((pos.file.clone(), pos.line, pos.col, operands.len()))
                    .or_default() += 1;
            }
        }
    }

    walk_module_expressions(hir, &mut |expr| {
        let expected = expected_call_operands(hir, expr);
        let Some(expected) = expected else { return };
        let key = (expr.pos.file.clone(), expr.pos.line, expr.pos.col, expected);
        let carried = actual.get(&key).copied().unwrap_or(0);
        if carried == 0 {
            findings.push(format!(
                "{}: call operand count {expected} is absent from LIR",
                expr.pos
            ));
        }
    });
}

fn suspend_position(terminator: &l::Terminator) -> Option<&Pos> {
    let l::Terminator::Suspend { pos, .. } = terminator else {
        return None;
    };
    Some(pos)
}

fn expected_call_operands(hir: &hir::Module, expr: &hir::Expr) -> Option<usize> {
    match &expr.kind {
        hir::ExprKind::Call { callee, args } => match callee {
            hir::Callee::Ambient(hir::AmbientFn::Unreachable) => None,
            hir::Callee::Func(name) => hir
                .functions
                .iter()
                .find(|function| function.name == *name)
                .map(|function| function.params.len()),
            hir::Callee::Foreign(name) => hir
                .foreign_fns
                .iter()
                .find(|function| function.name == *name)
                .map(|function| {
                    function
                        .params
                        .iter()
                        .map(|parameter| {
                            usize::from(matches!(parameter.ty, subscript_compiler::Type::Array(_)))
                                + 1
                        })
                        .sum()
                }),
            hir::Callee::Arr(operation) if static_array_callback(*operation, args).is_some() => {
                let callback = static_array_callback(*operation, args)?;
                match &callback.ty {
                    Type::Func(function) => Some(
                        function.params.len()
                            + usize::from(matches!(callback.kind, hir::ExprKind::Lambda { .. })),
                    ),
                    _ => None,
                }
            }
            hir::Callee::Map(hir::MapFn::ForEach) | hir::Callee::Set(hir::SetFn::ForEach) => {
                args.get(1).and_then(|callback| match &callback.ty {
                    subscript_compiler::Type::Func(function) => Some(function.params.len() + 1),
                    _ => None,
                })
            }
            hir::Callee::Value(_) | hir::Callee::Method { .. } => Some(args.len() + 1),
            hir::Callee::Ambient(_)
            | hir::Callee::ContextBytes { .. }
            | hir::Callee::Math(_)
            | hir::Callee::Num(_)
            | hir::Callee::Date(_)
            | hir::Callee::Json(_)
            | hir::Callee::Str(_)
            | hir::Callee::Regex(_)
            | hir::Callee::Arr(_)
            | hir::Callee::Map(_)
            | hir::Callee::Set(_)
            | hir::Callee::Worker(_) => Some(args.len()),
        },
        hir::ExprKind::New { class, .. } => hir
            .classes
            .get(class.0)
            .and_then(|class| class.ctor.as_ref())
            .map(|constructor| constructor.params.len() + 1),
        hir::ExprKind::AsyncCall { callee, .. }
        | hir::ExprKind::AsyncHandleCreate { callee, .. } => match callee {
            hir::AsyncCallee::Function(name) => hir
                .functions
                .iter()
                .find(|function| function.name == *name)
                .map(|function| function.params.len()),
            hir::AsyncCallee::Method { class, name, .. } => hir
                .classes
                .get(class.0)
                .and_then(|class| class.methods.iter().find(|method| method.name == *name))
                .map(|method| method.params.len() + 1),
        },
        hir::ExprKind::Int(_)
        | hir::ExprKind::Float(_)
        | hir::ExprKind::Bool(_)
        | hir::ExprKind::Str(_)
        | hir::ExprKind::Null
        | hir::ExprKind::This
        | hir::ExprKind::Local(_)
        | hir::ExprKind::Global(_)
        | hir::ExprKind::FuncRef(_)
        | hir::ExprKind::EnumMember { .. }
        | hir::ExprKind::Unary { .. }
        | hir::ExprKind::Binary { .. }
        | hir::ExprKind::Assign { .. }
        | hir::ExprKind::Cast(_)
        | hir::ExprKind::DescriptorLit { .. }
        | hir::ExprKind::Zero
        | hir::ExprKind::RawNew { .. }
        | hir::ExprKind::Field { .. }
        | hir::ExprKind::JsonResultValue(_)
        | hir::ExprKind::Length(_)
        | hir::ExprKind::Index { .. }
        | hir::ExprKind::ArrayLit(_)
        | hir::ExprKind::ArraySpreadLit(_)
        | hir::ExprKind::Template(_)
        | hir::ExprKind::Lambda { .. }
        | hir::ExprKind::Yield(_)
        | hir::ExprKind::AsyncSuspend
        | hir::ExprKind::AsyncHandleAwait(_)
        | hir::ExprKind::AsyncHandleTransfer { .. }
        | hir::ExprKind::Cond { .. } => None,
    }
}

fn compare_instruction_operands(lir: &l::Module, findings: &mut Vec<String>) {
    for function in &lir.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                let count = instruction.operands.len();
                let expected = instruction_arity(lir, instruction, function);
                match expected {
                    Arity::Exact(required) if count != required => findings.push(format!(
                        "{}: function {:?} block {:?} instruction {:?} carries {count} operands; operation requires {required}",
                        instruction.pos, function.id, block.id, instruction.kind
                    )),
                    Arity::MatchesPayload(required) if count != required => findings.push(format!(
                        "{}: function {:?} block {:?} instruction {:?} carries {count} operands; payload requires {required}",
                        instruction.pos, function.id, block.id, instruction.kind
                    )),
                    _ => {}
                }
            }
        }
    }
}

enum Arity {
    Exact(usize),
    MatchesPayload(usize),
    Variable,
}

fn instruction_arity(
    lir: &l::Module,
    instruction: &l::Instruction,
    _function: &l::Function,
) -> Arity {
    use l::InstructionKind as K;
    match &instruction.kind {
        K::Copy
        | K::Unary(_)
        | K::Cast
        | K::Coerce
        | K::BoxBoundaryValue { .. }
        | K::AddressOfValue
        | K::LoadAddress
        | K::LoadField(_)
        | K::Length
        | K::ForeignArrayData
        | K::ArrayWithCapacity
        | K::IteratorCreate { .. }
        | K::IteratorBound => Arity::Exact(1),
        K::StringLiteral(_)
        | K::LoadLocal(_)
        | K::AddressOfLocal(_)
        | K::LoadGlobal(_)
        | K::AddressOfGlobal(_)
        | K::FunctionRef(_)
        | K::AllocateClass(_)
        | K::Zero => Arity::Exact(0),
        K::StoreLocal(_) | K::StoreGlobal(_) => Arity::Exact(1),
        K::Binary(_) | K::AddressOfIndex { .. } | K::StoreAddress => Arity::Exact(2),
        K::AddressOfField(_) => Arity::Exact(1),
        K::ArrayLiteral => Arity::Variable,
        K::ArraySpreadLiteral(parts) => Arity::MatchesPayload(parts.len()),
        K::Template(parts) => Arity::MatchesPayload(
            parts
                .iter()
                .filter(|part| matches!(part, l::TemplatePart::Operand(_)))
                .count(),
        ),
        K::MakeClosure(target) => {
            let captures = lir
                .functions
                .get(target.0 as usize)
                .map(|target| {
                    target
                        .parameters
                        .iter()
                        .filter(|parameter| parameter.kind == l::ParameterKind::Capture)
                        .count()
                })
                .unwrap_or(usize::MAX);
            Arity::Exact(captures)
        }
        K::Call(target) | K::AsyncHandleCreate(target) => Arity::MatchesPayload(
            if matches!(
                target.kind,
                l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_)
            ) {
                lir.operation_signatures(&target.kind)
                    .next()
                    .map_or(usize::MAX, |signature| signature.parameter_types.len())
            } else {
                target.parameter_types.len()
            },
        ),
        K::AsyncHandleRetain
        | K::AsyncHandleRelease
        | K::AsyncHandleArrayRetain
        | K::AsyncHandleArrayRelease => Arity::Exact(1),
        K::IteratorHasNext | K::IteratorValue | K::IteratorAdvance => Arity::Exact(3),
    }
}

fn all_declared_functions(hir: &hir::Module) -> Vec<&hir::Function> {
    hir.classes
        .iter()
        .flat_map(|class| class.ctor.iter().chain(class.methods.iter()))
        .chain(hir.functions.iter())
        .collect()
}

fn collect_return_positions<'a>(statements: &'a [hir::Stmt], visit: &mut impl FnMut(&'a Pos)) {
    for statement in statements {
        match statement {
            hir::Stmt::Return { pos, .. } => visit(pos),
            hir::Stmt::If { then, els, .. } => {
                collect_return_positions(then, visit);
                if let Some(els) = els {
                    collect_return_positions(els, visit);
                }
            }
            hir::Stmt::While { body, .. }
            | hir::Stmt::For { body, .. }
            | hir::Stmt::ForOf { body, .. }
            | hir::Stmt::Block(body) => collect_return_positions(body, visit),
            hir::Stmt::Switch { cases, .. } => {
                for case in cases {
                    collect_return_positions(&case.body, visit);
                }
            }
            hir::Stmt::Let { .. }
            | hir::Stmt::Expr(_)
            | hir::Stmt::Break(_)
            | hir::Stmt::Continue(_) => {}
        }
        if stops_statement_sequence(statement) {
            break;
        }
    }
}

fn collect_lambdas(hir: &hir::Module) -> Vec<&Pos> {
    let mut positions = Vec::new();
    walk_module_expressions(hir, &mut |expr| {
        if matches!(&expr.kind, hir::ExprKind::Lambda { .. }) {
            positions.push(&expr.pos);
        }
    });
    positions
}

fn multiset<'a>(positions: impl Iterator<Item = &'a Pos>) -> BTreeMap<(String, u32, u32), usize> {
    let mut values = BTreeMap::new();
    for pos in positions {
        *values.entry(pos_key(pos)).or_default() += 1;
    }
    values
}

fn take_multiset(
    values: &mut BTreeMap<(String, u32, u32), usize>,
    key: &(String, u32, u32),
) -> bool {
    let Some(count) = values.get_mut(key) else {
        return false;
    };
    *count -= 1;
    if *count == 0 {
        values.remove(key);
    }
    true
}

fn pos_key(pos: &Pos) -> (String, u32, u32) {
    (pos.file.clone(), pos.line, pos.col)
}

fn walk_module_expressions<'a>(hir: &'a hir::Module, visit: &mut impl FnMut(&'a hir::Expr)) {
    for class in &hir.classes {
        for field in &class.fields {
            if let Some(init) = &field.init {
                walk_expr(init, visit);
            }
        }
    }
    for global in &hir.globals {
        walk_expr(&global.init, visit);
    }
    for function in all_declared_functions(hir) {
        for parameter in &function.params {
            if let Some(default) = &parameter.default {
                walk_expr(default, visit);
            }
        }
        walk_statements(&function.body, visit);
    }
    walk_statements(&hir.top_level, visit);
}

fn walk_statements<'a>(statements: &'a [hir::Stmt], visit: &mut impl FnMut(&'a hir::Expr)) {
    for statement in statements {
        match statement {
            hir::Stmt::Let { init, .. } | hir::Stmt::Expr(init) => walk_expr(init, visit),
            hir::Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    walk_expr(value, visit);
                }
            }
            hir::Stmt::If {
                cond, then, els, ..
            } => {
                walk_expr(cond, visit);
                walk_statements(then, visit);
                if let Some(els) = els {
                    walk_statements(els, visit);
                }
            }
            hir::Stmt::While { cond, body, .. } => {
                walk_expr(cond, visit);
                walk_statements(body, visit);
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                if let Some(init) = init {
                    walk_statements(std::slice::from_ref(init), visit);
                }
                if let Some(cond) = cond {
                    walk_expr(cond, visit);
                }
                walk_statements(body, visit);
                if let Some(step) = step {
                    walk_expr(step, visit);
                }
            }
            hir::Stmt::ForOf { subject, body, .. } => {
                walk_expr(subject, visit);
                walk_statements(body, visit);
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                walk_expr(disc, visit);
                for case in cases {
                    if let Some(test) = &case.test {
                        walk_expr(test, visit);
                    }
                    walk_statements(&case.body, visit);
                }
            }
            hir::Stmt::Block(body) => walk_statements(body, visit),
            hir::Stmt::Break(_) | hir::Stmt::Continue(_) => {}
        }
        if stops_statement_sequence(statement) {
            break;
        }
    }
}

fn stops_statement_sequence(statement: &hir::Stmt) -> bool {
    match statement {
        hir::Stmt::Return { .. } | hir::Stmt::Break(_) | hir::Stmt::Continue(_) => true,
        hir::Stmt::Expr(expr) => matches!(
            &expr.kind,
            hir::ExprKind::Call {
                callee: hir::Callee::Ambient(hir::AmbientFn::Unreachable),
                ..
            }
        ),
        hir::Stmt::Let { .. }
        | hir::Stmt::If { .. }
        | hir::Stmt::While { .. }
        | hir::Stmt::For { .. }
        | hir::Stmt::ForOf { .. }
        | hir::Stmt::Switch { .. }
        | hir::Stmt::Block(_) => false,
    }
}

fn walk_expr<'a>(expr: &'a hir::Expr, visit: &mut impl FnMut(&'a hir::Expr)) {
    visit(expr);
    use hir::ExprKind as K;
    match &expr.kind {
        K::Unary { operand, .. }
        | K::Cast(operand)
        | K::JsonResultValue(operand)
        | K::Length(operand) => walk_expr(operand, visit),
        K::Binary { left, right, .. } => {
            walk_expr(left, visit);
            walk_expr(right, visit);
        }
        K::Assign { target, value, .. } => {
            walk_place_children(target, visit);
            walk_expr(value, visit);
        }
        K::Call { callee, args } => {
            match callee {
                hir::Callee::Value(value) => walk_expr(value, visit),
                hir::Callee::Method { recv, .. } => walk_expr(recv, visit),
                hir::Callee::Func(_)
                | hir::Callee::Foreign(_)
                | hir::Callee::Ambient(_)
                | hir::Callee::ContextBytes { .. }
                | hir::Callee::Math(_)
                | hir::Callee::Num(_)
                | hir::Callee::Date(_)
                | hir::Callee::Json(_)
                | hir::Callee::Str(_)
                | hir::Callee::Regex(_)
                | hir::Callee::Arr(_)
                | hir::Callee::Map(_)
                | hir::Callee::Set(_)
                | hir::Callee::Worker(_) => {}
            }
            for argument in args {
                walk_expr(argument, visit);
            }
        }
        K::New { args, .. } => {
            for argument in args {
                walk_expr(argument, visit);
            }
        }
        K::DescriptorLit { fields, .. } => {
            for field in fields.iter().flatten() {
                walk_expr(field, visit);
            }
        }
        K::Field { obj, .. } | K::Index { obj, .. } => {
            walk_expr(obj, visit);
            if let K::Index { index, .. } = &expr.kind {
                walk_expr(index, visit);
            }
        }
        K::ArrayLit(elements) => {
            for element in elements {
                walk_expr(element, visit);
            }
        }
        K::ArraySpreadLit(elements) => {
            for element in elements {
                walk_expr(&element.expr, visit);
            }
        }
        K::Template(parts) => {
            for part in parts {
                if let hir::TplPart::Expr(value) = part {
                    walk_expr(value, visit);
                }
            }
        }
        K::Lambda { params, body, .. } => {
            for parameter in params {
                if let Some(default) = &parameter.default {
                    walk_expr(default, visit);
                }
            }
            walk_statements(body, visit);
        }
        K::Yield(value) => {
            if let Some(value) = value {
                walk_expr(value, visit);
            }
        }
        K::AsyncCall { callee, args } | K::AsyncHandleCreate { callee, args, .. } => {
            if let Some(receiver) = callee.receiver() {
                walk_expr(receiver, visit);
            }
            for argument in args {
                walk_expr(argument, visit);
            }
        }
        K::AsyncHandleAwait(handle) | K::AsyncHandleTransfer { value: handle, .. } => {
            walk_expr(handle, visit);
        }
        K::Cond { cond, then, els } => {
            walk_expr(cond, visit);
            walk_expr(then, visit);
            walk_expr(els, visit);
        }
        K::Int(_)
        | K::Float(_)
        | K::Bool(_)
        | K::Str(_)
        | K::Null
        | K::This
        | K::Local(_)
        | K::Global(_)
        | K::FuncRef(_)
        | K::EnumMember { .. }
        | K::Zero
        | K::RawNew { .. }
        | K::AsyncSuspend => {}
    }
}

fn walk_place_children<'a>(expr: &'a hir::Expr, visit: &mut impl FnMut(&'a hir::Expr)) {
    match &expr.kind {
        hir::ExprKind::Field { obj, .. } => walk_expr(obj, visit),
        hir::ExprKind::Index { obj, index, .. } => {
            walk_expr(obj, visit);
            walk_expr(index, visit);
        }
        hir::ExprKind::Local(_) | hir::ExprKind::Global(_) | hir::ExprKind::This => {}
        hir::ExprKind::Int(_)
        | hir::ExprKind::Float(_)
        | hir::ExprKind::Bool(_)
        | hir::ExprKind::Str(_)
        | hir::ExprKind::Null
        | hir::ExprKind::FuncRef(_)
        | hir::ExprKind::EnumMember { .. }
        | hir::ExprKind::Unary { .. }
        | hir::ExprKind::Binary { .. }
        | hir::ExprKind::Assign { .. }
        | hir::ExprKind::Cast(_)
        | hir::ExprKind::Call { .. }
        | hir::ExprKind::New { .. }
        | hir::ExprKind::DescriptorLit { .. }
        | hir::ExprKind::Zero
        | hir::ExprKind::RawNew { .. }
        | hir::ExprKind::JsonResultValue(_)
        | hir::ExprKind::Length(_)
        | hir::ExprKind::ArrayLit(_)
        | hir::ExprKind::ArraySpreadLit(_)
        | hir::ExprKind::Template(_)
        | hir::ExprKind::Lambda { .. }
        | hir::ExprKind::Yield(_)
        | hir::ExprKind::AsyncSuspend
        | hir::ExprKind::AsyncCall { .. }
        | hir::ExprKind::AsyncHandleCreate { .. }
        | hir::ExprKind::AsyncHandleAwait(_)
        | hir::ExprKind::AsyncHandleTransfer { .. }
        | hir::ExprKind::Cond { .. } => walk_expr(expr, visit),
    }
}
