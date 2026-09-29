//! Iterator and callback execution facts (compiler.md §68.2 item 12).

use super::*;

#[derive(Clone)]
struct ExpectedIteratorBound {
    bound: l::IteratorBoundKind,
    spelling: &'static str,
    count: usize,
}

pub(super) fn compare_iterator_bounds(
    hir: &hir::Module,
    lir: &l::Module,
    findings: &mut Vec<String>,
) {
    let mut expected = BTreeMap::<(String, u32, u32), ExpectedIteratorBound>::new();
    let mut collect_for_of = |statements: &[hir::Stmt]| {
        collect_for_of_bounds(hir, statements, &mut expected);
    };
    for function in all_declared_functions(hir) {
        collect_for_of(&function.body);
    }
    collect_for_of(&hir.top_level);
    walk_module_expressions(hir, &mut |expr| {
        if let hir::ExprKind::Lambda { body, .. } = &expr.kind {
            collect_for_of_bounds(hir, body, &mut expected);
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
    hir: &hir::Module,
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
                collect_for_of_bounds(hir, body, expected);
            }
            hir::Stmt::If { then, els, .. } => {
                collect_for_of_bounds(hir, then, expected);
                if let Some(els) = els {
                    collect_for_of_bounds(hir, els, expected);
                }
            }
            hir::Stmt::While { body, .. }
            | hir::Stmt::For { body, .. }
            | hir::Stmt::Block(body) => collect_for_of_bounds(hir, body, expected),
            hir::Stmt::Switch { cases, .. } => {
                for case in cases {
                    collect_for_of_bounds(hir, &case.body, expected);
                }
            }
            hir::Stmt::Try { body, handler, .. } => {
                collect_for_of_bounds(hir, body, expected);
                if try_body_raises(hir, body) {
                    collect_for_of_bounds(hir, handler, expected);
                }
            }
            hir::Stmt::Using { body, .. } => collect_for_of_bounds(hir, body, expected),
            hir::Stmt::Let { .. }
            | hir::Stmt::Expr(_)
            | hir::Stmt::Return { .. }
            | hir::Stmt::Break(_)
            | hir::Stmt::Continue(_)
            | hir::Stmt::Throw { .. } => {}
        }
    }
}

pub(super) fn static_array_callback(
    operation: hir::ArrFn,
    args: &[hir::Expr],
) -> Option<&hir::Expr> {
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

pub(super) fn compare_static_array_callbacks(
    hir: &hir::Module,
    lir: &l::Module,
    findings: &mut Vec<String>,
) {
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
                                && hir.functions.iter().any(|declaration| {
                                    declaration.symbol == *name
                                        && declaration.name == function.source_name
                                        && declaration.pos == function.pos
                                })
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
