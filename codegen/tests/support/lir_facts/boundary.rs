//! Boundary and terminator execution facts (compiler.md §68.2 item 12).

use super::*;

pub(super) fn compare_foreign_array_snapshots(
    hir: &hir::Module,
    lir: &l::Module,
    findings: &mut Vec<String>,
) {
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

pub(super) fn compare_boundary_boxes(
    hir: &hir::Module,
    lir: &l::Module,
    findings: &mut Vec<String>,
) {
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
            | hir::ExprKind::Local(..)
            | hir::ExprKind::Global(_)
            | hir::ExprKind::FuncRef(_)
            | hir::ExprKind::EnumMember { .. }
            | hir::ExprKind::Unary { .. }
            | hir::ExprKind::Binary { .. }
            | hir::ExprKind::AbsenceTest { .. }
            | hir::ExprKind::Assign { .. }
            | hir::ExprKind::Cast(_)
            | hir::ExprKind::DescriptorLit { .. }
            | hir::ExprKind::Zero
            | hir::ExprKind::Unassigned
            | hir::ExprKind::RawNew { .. }
            | hir::ExprKind::Field { .. }
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
            | hir::ExprKind::TaskGroup { .. }
            | hir::ExprKind::AsyncAll { .. }
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

pub(super) fn compare_terminator_positions(
    hir: &hir::Module,
    lir: &l::Module,
    findings: &mut Vec<String>,
) {
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
                collect_return_positions(hir, body, &mut |pos| {
                    expected.push(pos.clone());
                });
            }
        }
    });

    expected.extend(using::hook_facts(hir).trap_positions);

    for function in all_declared_functions(hir) {
        let subscript_compiler::Type::Class(class) = &function.ret else {
            continue;
        };
        if !hir_boundary_class_contains_pointer(hir, *class, &mut Vec::new()) {
            continue;
        }
        collect_return_positions(hir, &function.body, &mut |pos| {
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
        | K::Local(..)
        | K::Global(_)
        | K::FuncRef(_)
        | K::EnumMember { .. }
        | K::Unary { .. }
        | K::Binary { .. }
        | K::AbsenceTest { .. }
        | K::Assign { .. }
        | K::Cast(_)
        | K::New { .. }
        | K::DescriptorLit { .. }
        | K::Zero
        | K::Unassigned
        | K::RawNew { .. }
        | K::Field { .. }
        | K::Length(_)
        | K::Index { .. }
        | K::ArrayLit(_)
        | K::ArraySpreadLit(_)
        | K::Template(_)
        | K::Lambda { .. }
        | K::AsyncHandleCreate { .. }
        | K::TaskGroup { .. }
        | K::AsyncAll { .. }
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
