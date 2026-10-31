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
            hir::Callee::Arr(operation)
                if static_array_callback(*operation, args).is_some()
                    && !has_fixed_receiver(args) =>
            {
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
            require(&mut expected, pos_key(&expr.pos), fact);
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

/// Adds the cursors of one HIR spelling. Each generic instance repeats
/// the source position of its template, so the counts add.
fn require(
    expected: &mut BTreeMap<(String, u32, u32), ExpectedIteratorBound>,
    key: (String, u32, u32),
    fact: ExpectedIteratorBound,
) {
    expected
        .entry(key)
        .and_modify(|known| known.count += fact.count)
        .or_insert(fact);
}

fn collect_for_of_bounds(
    hir: &hir::Module,
    statements: &[hir::Stmt],
    expected: &mut BTreeMap<(String, u32, u32), ExpectedIteratorBound>,
) {
    for statement in statements {
        match statement {
            hir::Stmt::ForOf {
                body,
                pos,
                name: _,
                ty: _,
                subject: _,
                kind,
            } => {
                // An index is the whole state of a `FixedArray` traversal.
                if *kind != hir::ForOfKind::FixedArrayValues {
                    require(
                        expected,
                        pos_key(pos),
                        ExpectedIteratorBound {
                            bound: l::IteratorBoundKind::Live,
                            spelling: "for-of",
                            count: 1,
                        },
                    );
                }
                collect_for_of_bounds(hir, body, expected);
            }
            hir::Stmt::If {
                then,
                els,
                cond: _,
                pos: _,
            } => {
                collect_for_of_bounds(hir, then, expected);
                if let Some(els) = els {
                    collect_for_of_bounds(hir, els, expected);
                }
            }
            hir::Stmt::GeneratorForOf {
                body,
                name: _,
                ty: _,
                mutable: _,
                subject: _,
                pos: _,
            }
            | hir::Stmt::While {
                body,
                cond: _,
                pos: _,
            }
            | hir::Stmt::DoWhile {
                body,
                cond: _,
                pos: _,
            }
            | hir::Stmt::For {
                body,
                init: _,
                cond: _,
                step: _,
                pos: _,
            }
            | hir::Stmt::Block(body) => collect_for_of_bounds(hir, body, expected),
            hir::Stmt::Switch {
                cases,
                disc: _,
                pos: _,
            } => {
                for case in cases {
                    collect_for_of_bounds(hir, &case.body, expected);
                }
            }
            hir::Stmt::Try {
                body,
                handler,
                binding: _,
                pos: _,
            } => {
                collect_for_of_bounds(hir, body, expected);
                if try_body_raises(hir, body) {
                    collect_for_of_bounds(hir, handler, expected);
                }
            }
            hir::Stmt::Using {
                body,
                bindings: _,
                finalizer,
                pos: _,
            } => {
                collect_for_of_bounds(hir, body, expected);
                if let Some(finalizer) = finalizer {
                    collect_for_of_bounds(hir, finalizer, expected);
                }
            }
            hir::Stmt::Let { .. }
            | hir::Stmt::Expr(_)
            | hir::Stmt::Return { .. }
            | hir::Stmt::Break(_)
            | hir::Stmt::Continue(_)
            | hir::Stmt::Throw { .. } => {}
        }
    }
}

fn has_fixed_receiver(args: &[hir::Expr]) -> bool {
    matches!(
        args.first().map(|argument| &argument.ty),
        Some(Type::FixedArray(..))
    )
}

/// The callback of a call that lowers to a loop. A `FixedArray` receiver takes
/// a loop with each callback (compiler.md §220.1 rule 10), and a dynamic array
/// takes one with a known function.
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
        Some(Type::Array(_) | Type::FixedArray(..))
    ) {
        return None;
    }
    let callback = args.get(1)?;
    (has_fixed_receiver(args)
        || matches!(
            callback.kind,
            hir::ExprKind::FuncRef(_) | hir::ExprKind::Lambda { .. }
        ))
    .then_some(callback)
}

type PositionKey = (String, u32, u32);

/// The Array callback calls of the HIR, counted by source position. A
/// chain gives each of its calls the position of the chain start, so one
/// position can hold more than one call.
#[derive(Default)]
struct CallbackSites {
    /// Static `map` and `filter` calls: each one has an output array.
    outputs: BTreeMap<PositionKey, usize>,
    /// Function value calls of one operation: each one is a runtime intrinsic.
    intrinsics: BTreeMap<(PositionKey, &'static str), usize>,
    /// Static calls of one callback: each one is a direct call target.
    direct: BTreeMap<(PositionKey, String), usize>,
}

fn is_array_callback_site(operation: hir::ArrFn, args: &[hir::Expr]) -> bool {
    matches!(
        operation,
        hir::ArrFn::Map
            | hir::ArrFn::Filter
            | hir::ArrFn::Reduce
            | hir::ArrFn::ReduceRight
            | hir::ArrFn::ForEach
            | hir::ArrFn::Some
            | hir::ArrFn::Every
            | hir::ArrFn::FindIndex
    ) && matches!(
        args.first().map(|argument| &argument.ty),
        Some(Type::Array(_) | Type::FixedArray(..))
    ) && args.get(1).is_some()
}

/// Names one static callback: a function by its symbol, a lambda by its position.
fn callback_identity(callback: &hir::Expr) -> String {
    match &callback.kind {
        hir::ExprKind::FuncRef(name) => format!("function {name:?}"),
        _ => format!("lambda {}", callback.pos),
    }
}

fn callback_sites_by_position(hir: &hir::Module) -> CallbackSites {
    let mut sites = CallbackSites::default();
    walk_module_expressions(hir, &mut |expr| {
        let hir::ExprKind::Call {
            callee: hir::Callee::Arr(operation),
            args,
        } = &expr.kind
        else {
            return;
        };
        if !is_array_callback_site(*operation, args) {
            return;
        }
        let key = pos_key(&expr.pos);
        let Some(callback) = static_array_callback(*operation, args) else {
            *sites.intrinsics.entry((key, operation.name())).or_default() += 1;
            return;
        };
        *sites
            .direct
            .entry((key.clone(), callback_identity(callback)))
            .or_default() += 1;
        if matches!(operation, hir::ArrFn::Map | hir::ArrFn::Filter) {
            *sites.outputs.entry(key).or_default() += 1;
        }
    });
    sites
}

pub(super) fn compare_static_array_callbacks(
    hir: &hir::Module,
    lir: &l::Module,
    findings: &mut Vec<String>,
) {
    let sites = callback_sites_by_position(hir);
    walk_module_expressions(hir, &mut |expr| {
        let hir::ExprKind::Call {
            callee: hir::Callee::Arr(operation),
            args,
        } = &expr.kind
        else {
            return;
        };
        if !is_array_callback_site(*operation, args) {
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

        let required_intrinsics = sites
            .intrinsics
            .get(&(key.clone(), operation.name()))
            .copied()
            .unwrap_or(0);
        let Some(static_callback) = static_array_callback(*operation, args) else {
            if intrinsic_count != required_intrinsics {
                findings.push(format!(
                    "{}: Array callback function value carries {intrinsic_count} runtime intrinsic call(s); HIR requires {required_intrinsics}, one for each function value call of `{}` at this position",
                    expr.pos,
                    operation.name()
                ));
            }
            return;
        };
        if intrinsic_count != required_intrinsics {
            findings.push(format!(
                "{}: static Array callback keeps {intrinsic_count} runtime intrinsic call(s); HIR requires {required_intrinsics}, one for each function value call of `{}` at this position",
                expr.pos,
                operation.name()
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
                    (hir::ExprKind::FuncRef(_) | hir::ExprKind::Lambda { .. }, _) => false,
                    (_, l::CallTargetKind::Indirect) => true,
                    _ => false,
                }
            })
            .count();
        let required_direct = sites
            .direct
            .get(&(key.clone(), callback_identity(static_callback)))
            .copied()
            .unwrap_or(0);
        if direct != required_direct {
            findings.push(format!(
                "{}: static Array callback carries {direct} matching direct call target(s); HIR requires {required_direct}, one for each static call of this callback at this position",
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
        let required = sites.outputs.get(&key).copied().unwrap_or(0);
        if output_count != required || push_count != required {
            findings.push(format!(
                "{}: static {operation:?} carries {output_count} capacity allocation(s) and {push_count} push site(s); HIR requires {required} each, one for each static `map` or `filter` call at this position",
                expr.pos
            ));
        }
    });
}

#[cfg(test)]
mod static_output_tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    const CHAIN: &str = "export function main(): void {\n\
        const ns: i32[] = [1, 2, 3];\n\
        print(ns.filter((x: i32): boolean => x > 1).map((x: i32): string => `${x}`).join(\",\"));\n\
        }\n";
    const FUNCTION_CHAIN: &str = "function f(x: i32): i32 { return x + 1; }\n\
        export function main(): void {\n\
        const ns: i32[] = [1, 2, 3];\n\
        print(ns.map(f).map(f).join(\",\"));\n\
        }\n";
    const VALUE_CHAIN: &str = "export function main(): void {\n\
        const g = (x: i32): boolean => x > 1;\n\
        const ns: i32[] = [1, 2, 3];\n\
        print(ns.filter(g).filter(g).join(\",\"));\n\
        }\n";

    fn checked_source(source: &str) -> (hir::Module, l::Module) {
        let hir = check_program(&[SourceFile::new("chain.ts", source)]).expect("the chain checks");
        let lir = subscript_codegen::lir::lower_module(&hir).expect("the chain lowers");
        (hir, lir)
    }

    fn checked() -> (hir::Module, l::Module) {
        checked_source(CHAIN)
    }

    fn findings(hir: &hir::Module, lir: &l::Module) -> Vec<String> {
        let mut findings = Vec::new();
        compare_static_array_callbacks(hir, lir, &mut findings);
        findings
    }

    /// Replaces the first instruction that `selected` takes with a `Copy`.
    fn remove_first(lir: &mut l::Module, selected: impl Fn(&l::InstructionKind) -> bool) {
        let instruction = lir
            .functions
            .iter_mut()
            .flat_map(|function| &mut function.blocks)
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| selected(&instruction.kind))
            .expect("the chain has the instruction");
        instruction.kind = l::InstructionKind::Copy;
    }

    #[test]
    fn a_chain_requires_one_output_for_each_static_call_at_the_position() {
        let (hir, lir) = checked();
        assert_eq!(findings(&hir, &lir), Vec::<String>::new());
    }

    #[test]
    fn a_function_callback_chain_requires_one_direct_call_for_each_static_call() {
        let (hir, mut lir) = checked_source(FUNCTION_CHAIN);
        assert_eq!(findings(&hir, &lir), Vec::<String>::new());
        remove_first(&mut lir, |kind| {
            matches!(kind, l::InstructionKind::Call(target)
                if matches!(target.kind, l::CallTargetKind::Function(_)))
        });
        let findings = findings(&hir, &lir);
        assert!(
            findings.iter().any(|finding| finding
                .contains("carries 1 matching direct call target(s); HIR requires 2")),
            "{findings:?}"
        );
    }

    #[test]
    fn a_function_value_chain_requires_one_intrinsic_for_each_call() {
        let (hir, mut lir) = checked_source(VALUE_CHAIN);
        assert_eq!(findings(&hir, &lir), Vec::<String>::new());
        remove_first(&mut lir, |kind| {
            matches!(kind, l::InstructionKind::Call(target)
                if matches!(&target.kind, l::CallTargetKind::Intrinsic(intrinsic)
                    if intrinsic.family == l::IntrinsicFamily::Array))
        });
        let findings = findings(&hir, &lir);
        assert!(
            findings
                .iter()
                .any(|finding| finding
                    .contains("carries 1 runtime intrinsic call(s); HIR requires 2")),
            "{findings:?}"
        );
    }

    #[test]
    fn a_chain_with_a_missing_capacity_allocation_gives_a_finding() {
        let (hir, mut lir) = checked();
        remove_first(&mut lir, |kind| {
            *kind == l::InstructionKind::ArrayWithCapacity
        });
        let findings = findings(&hir, &lir);
        assert!(
            findings.iter().any(|finding| finding.contains(
                "carries 1 capacity allocation(s) and 2 push site(s); HIR requires 2 each"
            )),
            "{findings:?}"
        );
    }

    #[test]
    fn a_chain_with_a_missing_push_gives_a_finding() {
        let (hir, mut lir) = checked();
        remove_first(&mut lir, |kind| {
            matches!(kind, l::InstructionKind::Call(target)
                if target.kind == l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayPush))
        });
        let findings = findings(&hir, &lir);
        assert!(
            findings.iter().any(|finding| finding.contains(
                "carries 2 capacity allocation(s) and 1 push site(s); HIR requires 2 each"
            )),
            "{findings:?}"
        );
    }
}
