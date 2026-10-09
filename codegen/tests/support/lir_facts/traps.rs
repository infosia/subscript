//! Trap site facts: each HIR trap site against the LIR traps (compiler.md §68.2 item 12).

use super::*;

pub(super) fn compare_traps(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) {
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
                    // A `throw` statement owns its raise site, and the
                    // exception edge of a `using` binding owns the raise
                    // site of its resume; no HIR expression carries either
                    // (compiler.md §115.2, §115.5 rule 7). A handle release
                    // owns its check (§116.1 rule 4). A group scope exit owns its check (§170).
                    // Counted-store checks place
                    // retains; the lifetime verifier checks their read sites.
                    if matches!(
                        instruction.kind,
                        l::InstructionKind::Throw
                            | l::InstructionKind::ExceptionResume
                            | l::InstructionKind::ExceptionRestore
                            | l::InstructionKind::AsyncHandleRetain
                            | l::InstructionKind::AsyncHandleArrayRetain
                            | l::InstructionKind::AsyncHandleRelease
                            | l::InstructionKind::AsyncHandleArrayRelease
                            | l::InstructionKind::TaskGroup(hir::TaskGroupOperation::Release)
                    ) {
                        continue;
                    }
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
    let mut seen_defaults = std::collections::BTreeSet::new();
    walk_execution_root_expressions(hir, &mut |expr| {
        collect_trap_expression(expr, hir, &mut expected, findings, &mut seen_defaults);
    });
    lifetime::statements(&hir.top_level, hir, &mut expected);
    for function in all_declared_functions(hir) {
        lifetime::statements(&function.body, hir, &mut expected);
    }
    let cleanup = using::hook_facts(hir);
    for body in &cleanup.finalizers {
        walk_placed_statement_expression_roots(hir, body, &mut |expr| {
            collect_trap_expression(expr, hir, &mut expected, findings, &mut seen_defaults)
        });
        lifetime::statements(body, hir, &mut expected);
    }
    for key in cleanup.traps {
        *expected.entry(key).or_default() += 1;
    }
    for function in all_declared_functions(hir) {
        for site in function.trap_sites() {
            *expected.entry(hir_trap_key(&site)).or_default() += 1;
        }
        if let Some(sites) = hir
            .host_entries
            .iter()
            .any(|entry| entry.target == function.symbol)
            .then(|| function.host_entry_trap_sites(hir))
            .flatten()
        {
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
    let lir_free_functions = lir.functions.iter().filter(|function| {
        matches!(
            function.kind,
            l::FunctionKind::Free | l::FunctionKind::SynthesizedHelper
        )
    });
    for (expected_function, actual_function) in hir_free_functions.zip(lir_free_functions) {
        let expected_attachment = hir
            .host_entries
            .iter()
            .any(|entry| entry.target == expected_function.symbol);
        let actual_attachment = actual_function.host_entry_traps.is_some();
        if expected_attachment != actual_attachment {
            findings.push(format!(
                "{}: host-entry trap attachment is {actual_attachment}; HIR requires {expected_attachment}",
                expected_function.pos
            ));
        }
    }
}

pub(super) fn collect_trap_expression(
    expression: &hir::Expr,
    hir: &hir::Module,
    expected: &mut BTreeMap<TrapKey, usize>,
    findings: &mut Vec<String>,
    seen_defaults: &mut std::collections::BTreeSet<usize>,
) {
    let mut nodes = Vec::new();
    walk_expr(hir, expression, &mut |node| nodes.push(node));
    for node in nodes {
        // An async lambda's callable allocates its frame at invocation (§167
        // rule 13), and an owning lambda allocates its environment (§181 rule 2).
        if let hir::ExprKind::Lambda {
            is_async,
            owns_environment,
            captures,
            ..
        } = &node.kind
        {
            let allocations =
                usize::from(*is_async) + usize::from(*owns_environment && !captures.is_empty());
            if allocations > 0 {
                *expected
                    .entry(trap_key(&node.pos, "Allocation".into()))
                    .or_default() += allocations;
            }
        }
        lifetime::expression(node, hir, expected);
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
                                collect_trap_expression(
                                    default,
                                    hir,
                                    expected,
                                    findings,
                                    seen_defaults,
                                );
                            }
                        }
                    }
                }
            }
            hir::ExprKind::New { class, args } => {
                if let Some(definition) = hir.classes.get(class.0) {
                    for field in &definition.fields {
                        if let Some(initializer) = &field.init {
                            collect_trap_expression(
                                initializer,
                                hir,
                                expected,
                                findings,
                                seen_defaults,
                            );
                        }
                    }
                    if let Some(constructor) = &definition.ctor {
                        collect_missing_parameter_defaults(
                            &constructor.params,
                            (args.len(), Some(Type::Class(*class))),
                            hir,
                            expected,
                            findings,
                            seen_defaults,
                        );
                    }
                }
            }
            hir::ExprKind::Call { callee, args } => {
                match declared_callee_parameters(hir, callee, &node.pos) {
                    Ok(Some(parameters)) => collect_missing_parameter_defaults(
                        parameters,
                        (
                            args.len(),
                            match callee {
                                hir::Callee::Method { recv, .. } => Some(recv.ty.clone()),
                                _ => None,
                            },
                        ),
                        hir,
                        expected,
                        findings,
                        seen_defaults,
                    ),
                    Ok(None) => {}
                    Err(finding) => findings.push(finding),
                }
            }
            hir::ExprKind::AsyncCall { callee, args }
            | hir::ExprKind::AsyncHandleCreate { callee, args, .. } => {
                match declared_async_callee_parameters(hir, callee, &node.pos) {
                    Ok(parameters) => collect_missing_parameter_defaults(
                        parameters,
                        (
                            args.len(),
                            match callee {
                                hir::AsyncCallee::Method { class, .. } => Some(Type::Class(*class)),
                                _ => None,
                            },
                        ),
                        hir,
                        expected,
                        findings,
                        seen_defaults,
                    ),
                    Err(finding) => findings.push(finding),
                }
            }
            hir::ExprKind::Int(_)
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
            | hir::ExprKind::AsyncHandleAwait(_)
            | hir::ExprKind::TaskGroup { .. }
            | hir::ExprKind::AsyncAll { .. }
            | hir::ExprKind::AsyncHandleTransfer { .. }
            | hir::ExprKind::Cond { .. } => {}
        }
    }
}
