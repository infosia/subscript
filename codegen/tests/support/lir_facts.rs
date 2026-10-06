//! Independent HIR/LIR execution-fact comparison for §68.2 item 12.

use std::collections::BTreeMap;

use subscript_compiler::hir;
use subscript_compiler::lir as l;
use subscript_compiler::{ClassId, Pos, Type};

#[path = "lir_facts/boundary.rs"]
mod boundary;
#[path = "lir_facts/iteration.rs"]
mod iteration;

use boundary::{
    compare_boundary_boxes, compare_foreign_array_snapshots, compare_terminator_positions,
};
use iteration::{compare_iterator_bounds, compare_static_array_callbacks, static_array_callback};

#[path = "lir_facts_lifetime.rs"]
mod lifetime;
#[path = "lir_facts_release.rs"]
mod release;
#[path = "lir_facts_using.rs"]
mod using;

/// Returns every HIR execution fact that the supplied LIR module drops.
pub fn dropped_facts(hir: &hir::Module, lir: &l::Module) -> Vec<String> {
    let mut findings = Vec::new();
    release::compare(hir, lir, &mut findings);
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
            (ExpectedFunctionKind::Free, l::FunctionKind::Free) => {
                !expected.function.synthesized_helper
            }
            (ExpectedFunctionKind::Free, l::FunctionKind::SynthesizedHelper) => {
                expected.function.synthesized_helper
            }
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
    let target_id = |symbol: &hir::Symbol| {
        hir.functions
            .iter()
            .position(|f| f.symbol == *symbol)
            .map(|index| l::FunctionId((free_offset + index) as u32))
    };
    for entry in &hir.host_entries {
        if target_id(&entry.target).is_none() {
            findings.push(format!(
                "{}: malformed HIR: host entry `{}` target symbol {:?} names no module function",
                entry.pos,
                entry.name,
                entry.target.full_text()
            ));
        }
    }
    let expected_entry = hir
        .host_entries
        .iter()
        .find(|e| e.name == "main" && e.signature.parameters.is_empty())
        .and_then(|e| target_id(&e.target));
    if lir.entry != expected_entry {
        findings.push(format!(
            "<module>:1:1: executable entry is {:?}, expected {:?}",
            lir.entry, expected_entry
        ));
    }
    let eligible: std::collections::BTreeSet<_> = hir
        .host_entries
        .iter()
        .filter(|entry| entry.signature.is_async)
        .filter_map(|entry| target_id(&entry.target))
        .filter(|id| Some(*id) != expected_entry)
        .collect();
    let roots: std::collections::BTreeSet<_> = lir.async_roots.iter().copied().collect();
    if roots != eligible || roots.len() != lir.async_roots.len() {
        findings.push(
            "<module>:1:1: async roots must contain each non-main async target exactly once".into(),
        );
    }
    let first_sites: Vec<_> = lir
        .async_roots
        .iter()
        .map(|root| {
            hir.host_entries
                .iter()
                .filter(|entry| target_id(&entry.target) == Some(*root))
                .map(|entry| (entry.pos.line, entry.pos.col))
                .min()
        })
        .collect();
    if first_sites.windows(2).any(|pair| pair[0] > pair[1]) {
        findings.push("<module>:1:1: async roots violate first entry export-site order".into());
    }
    let expected_hosts: Vec<_> = hir
        .host_entries
        .iter()
        .map(|e| (&e.name, target_id(&e.target), &e.signature, &e.pos))
        .collect();
    let actual_hosts: Vec<_> = lir
        .host_entries
        .iter()
        .map(|e| (&e.name, Some(e.target), &e.signature, &e.pos))
        .collect();
    if actual_hosts != expected_hosts {
        findings.push("<module>:1:1: host entry table differs from the checked exports".to_owned());
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
    for key in using::hook_facts(hir).traps {
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

fn collect_trap_expression(
    expression: &hir::Expr,
    hir: &hir::Module,
    expected: &mut BTreeMap<TrapKey, usize>,
    findings: &mut Vec<String>,
    seen_defaults: &mut std::collections::BTreeSet<usize>,
) {
    let mut nodes = Vec::new();
    walk_expr(hir, expression, &mut |node| nodes.push(node));
    for node in nodes {
        // An async lambda's callable allocates its frame at invocation (§167 rule 13).
        if matches!(node.kind, hir::ExprKind::Lambda { is_async: true, .. }) {
            *expected
                .entry(trap_key(&node.pos, "Allocation".into()))
                .or_default() += 1;
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

/// Resolves the module function that a call names. A miss is malformed HIR.
fn declared_function<'a>(
    hir: &'a hir::Module,
    symbol: &hir::Symbol,
    pos: &Pos,
) -> Result<&'a hir::Function, String> {
    hir.functions
        .iter()
        .find(|function| function.symbol == *symbol)
        .ok_or_else(|| {
            format!(
                "{pos}: malformed HIR: callee symbol {:?} names no module function",
                symbol.full_text()
            )
        })
}

/// Resolves the class method that a call names. A miss is malformed HIR.
fn declared_method<'a>(
    hir: &'a hir::Module,
    class: ClassId,
    symbol: &hir::Symbol,
    pos: &Pos,
) -> Result<&'a hir::Function, String> {
    let definition = hir.classes.get(class.0).ok_or_else(|| {
        format!(
            "{pos}: malformed HIR: method callee class {} is absent",
            class.0
        )
    })?;
    definition
        .methods
        .iter()
        .find(|method| method.symbol == *symbol)
        .ok_or_else(|| {
            format!(
                "{pos}: malformed HIR: callee symbol {:?} names no method of class {}",
                symbol.full_text(),
                class.0
            )
        })
}

/// Resolves the foreign function that a call names. A miss is malformed HIR.
fn declared_foreign<'a>(
    hir: &'a hir::Module,
    name: &str,
    pos: &Pos,
) -> Result<&'a hir::ForeignFn, String> {
    hir.foreign_fns
        .iter()
        .find(|function| function.name == name)
        .ok_or_else(|| format!("{pos}: malformed HIR: callee {name:?} names no foreign function"))
}

fn declared_callee_parameters<'a>(
    hir: &'a hir::Module,
    callee: &'a hir::Callee,
    pos: &Pos,
) -> Result<Option<&'a [hir::Param]>, String> {
    Ok(match callee {
        hir::Callee::Func(name) => Some(declared_function(hir, name, pos)?.params.as_slice()),
        hir::Callee::Foreign(name) => Some(declared_foreign(hir, name, pos)?.params.as_slice()),
        hir::Callee::Method { recv, name } => {
            let subscript_compiler::Type::Class(class) = &recv.ty else {
                return Ok(None);
            };
            Some(declared_method(hir, *class, name, pos)?.params.as_slice())
        }
        hir::Callee::Ambient(_)
        | hir::Callee::ContextBytes { .. }
        | hir::Callee::Math(_)
        | hir::Callee::Num(_)
        | hir::Callee::Date(_)
        | hir::Callee::Text(_)
        | hir::Callee::Json(_)
        | hir::Callee::Str(_)
        | hir::Callee::Regex(_)
        | hir::Callee::Arr(_)
        | hir::Callee::Map(_)
        | hir::Callee::Set(_)
        | hir::Callee::Worker(_)
        | hir::Callee::Value(_) => None,
    })
}

fn declared_async_callee_parameters<'a>(
    hir: &'a hir::Module,
    callee: &'a hir::AsyncCallee,
    pos: &Pos,
) -> Result<&'a [hir::Param], String> {
    let function = match callee {
        hir::AsyncCallee::Function(name) => declared_function(hir, name, pos)?,
        hir::AsyncCallee::Method { class, name, .. } => declared_method(hir, *class, name, pos)?,
    };
    Ok(function.params.as_slice())
}

fn collect_missing_parameter_defaults(
    parameters: &[hir::Param],
    call: (usize, Option<Type>),
    hir: &hir::Module,
    expected: &mut BTreeMap<TrapKey, usize>,
    findings: &mut Vec<String>,
    seen_defaults: &mut std::collections::BTreeSet<usize>,
) {
    let classes: Vec<_> = hir
        .classes
        .iter()
        .map(subscript_compiler::types::HandleClass::from)
        .collect();
    for (index, parameter) in parameters.iter().enumerate().skip(call.0) {
        if let Some(default) = &parameter.default {
            *expected
                .entry(trap_key(&default.pos, "Call".to_string()))
                .or_default() += 1;
            if parameter.default_can_raise {
                *expected
                    .entry(trap_key(&default.pos, "Raise".to_string()))
                    .or_default() += 1;
            }
            for ty in call
                .1
                .iter()
                .chain(parameters[..index].iter().map(|parameter| &parameter.ty))
            {
                if ty
                    .handle_kind(&classes)
                    .is_some_and(subscript_compiler::types::HandleKind::needs_lifetime_trap)
                {
                    *expected
                        .entry(trap_key(&default.pos, "DevOnlyLifetime".to_string()))
                        .or_default() += 1;
                }
            }
            // A default body is lowered once; its calls do not expand callee defaults.
            if seen_defaults.insert(default as *const hir::Expr as usize) {
                collect_trap_expression(default, hir, expected, findings, seen_defaults);
            }
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
        walk_statement_expression_roots(hir, &function.body, visit);
    }
    walk_statement_expression_roots(hir, &hir.top_level, visit);
}

fn walk_statement_expression_roots<'a>(
    hir: &hir::Module,
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
                walk_statement_expression_roots(hir, then, visit);
                if let Some(els) = els {
                    walk_statement_expression_roots(hir, els, visit);
                }
            }
            hir::Stmt::While { cond, body, .. } => {
                visit(cond);
                walk_statement_expression_roots(hir, body, visit);
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                if let Some(init) = init {
                    walk_statement_expression_roots(hir, std::slice::from_ref(init), visit);
                }
                if let Some(cond) = cond {
                    visit(cond);
                }
                walk_statement_expression_roots(hir, body, visit);
                if let Some(step) = step {
                    visit(step);
                }
            }
            hir::Stmt::ForOf { subject, body, .. } => {
                visit(subject);
                walk_statement_expression_roots(hir, body, visit);
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                visit(disc);
                for case in cases {
                    if let Some(test) = &case.test {
                        visit(test);
                    }
                    walk_statement_expression_roots(hir, &case.body, visit);
                }
            }
            hir::Stmt::Block(body) => walk_statement_expression_roots(hir, body, visit),
            hir::Stmt::Break(_) | hir::Stmt::Continue(_) => {}
            hir::Stmt::Throw { value, .. } => visit(value),
            // The lowering emits a handler only when a raise site of the
            // `try` block reaches it (compiler.md §115.6 rule 2).
            hir::Stmt::Try { body, handler, .. } => {
                walk_statement_expression_roots(hir, body, visit);
                if try_body_raises(hir, body) {
                    walk_statement_expression_roots(hir, handler, visit);
                }
            }
            // The hooks are not HIR expressions; `using::hook_facts` derives
            // their placements (compiler.md §115.5 rule 5).
            hir::Stmt::Using { body, .. } => walk_statement_expression_roots(hir, body, visit),
        }
        if stops_statement_sequence(hir, statement) {
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
        hir::TrapSite::Raise { .. } => "Raise".to_string(),
        hir::TrapSite::Unreachable { .. } => "Unreachable".to_string(),
        hir::TrapSite::DivisionByZero { .. } => "DivisionByZero".to_string(),
        hir::TrapSite::IndexRead { .. } => "IndexRead".to_string(),
        hir::TrapSite::IndexWrite { .. } => "IndexWrite".to_string(),
        hir::TrapSite::GeneratorDoneValue { .. } => "GeneratorDoneValue".to_string(),
        hir::TrapSite::NullNarrowing { .. } => "NullNarrowing".to_string(),
        hir::TrapSite::ClassMismatch { class, .. } => format!("ClassMismatch({})", class.0),
        hir::TrapSite::DevOnlyRelease { .. } => "DevOnlyRelease".to_string(),
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
        l::TrapKind::Raise(_) => "Raise".to_string(),
        l::TrapKind::Unreachable => "Unreachable".to_string(),
        l::TrapKind::GeneratorDoneValue => "GeneratorDoneValue".to_string(),
        l::TrapKind::DivisionByZero => "DivisionByZero".to_string(),
        l::TrapKind::IndexRead => "IndexRead".to_string(),
        l::TrapKind::IndexWrite => "IndexWrite".to_string(),
        l::TrapKind::NullNarrowing | l::TrapKind::SharedNullNarrowing => {
            "NullNarrowing".to_string()
        }
        l::TrapKind::ClassMismatch(class) => format!("ClassMismatch({})", class.0),
        l::TrapKind::DevOnlyRelease(_) => "DevOnlyRelease".to_string(),
        l::TrapKind::DevOnlyLifetime(_) => "DevOnlyLifetime".to_string(),
        l::TrapKind::DevReloadOnlyStaleCoroutine => "DevReloadOnlyStaleCoroutine".to_string(),
        l::TrapKind::WireEnumValue(alias) => format!("WireEnumValue({})", alias.0),
        l::TrapKind::DisposeRaisedDuringExit => "DisposeRaisedDuringExit".to_string(),
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
                // `new Set<K>(source)` lowers to one instruction over
                // the array-literal spread traversal rather than a call
                // (compiler.md §103.1 rule 4); its source operand is the
                // call's operand.
                if matches!(
                    &instruction.kind,
                    l::InstructionKind::Call(_)
                        | l::InstructionKind::AsyncHandleCreate(_)
                        | l::InstructionKind::SetFromSource(_)
                        | l::InstructionKind::MapFromSource
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

    walk_call_expressions(hir, &mut |expr| {
        let expected = match expected_call_operands(hir, expr) {
            Ok(Some(expected)) => expected,
            Ok(None) => return,
            Err(finding) => {
                findings.push(finding);
                return;
            }
        };
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

/// Defaults execute at omitted-argument call sites, not at declarations.
fn walk_call_expressions<'a>(hir: &'a hir::Module, visit: &mut impl FnMut(&'a hir::Expr)) {
    let defaults: std::collections::BTreeSet<_> = all_declared_functions(hir)
        .flat_map(|function| &function.params)
        .filter_map(|parameter| parameter.default.as_ref())
        .map(|expression| expression as *const hir::Expr)
        .collect();
    let mut roots = Vec::new();
    for owner in hir.expression_owners() {
        match owner {
            hir::ExpressionOwner::Expr(expression)
                if !defaults.contains(&(expression as *const hir::Expr)) =>
            {
                roots.push(expression)
            }
            hir::ExpressionOwner::Expr(_) => {}
            hir::ExpressionOwner::Body { statements, .. } => {
                walk_statement_expression_roots(hir, statements, &mut |expression| {
                    roots.push(expression)
                });
            }
        }
    }
    let mut seen_defaults = std::collections::BTreeSet::new();
    while let Some(root) = roots.pop() {
        walk_expr(hir, root, &mut |expression| {
            visit(expression);
            let parameters = match &expression.kind {
                hir::ExprKind::Call { callee, args } => {
                    declared_callee_parameters(hir, callee, &expression.pos)
                        .ok()
                        .flatten()
                        .map(|parameters| (parameters, args.len()))
                }
                hir::ExprKind::New { class, args } => hir.classes[class.0]
                    .ctor
                    .as_ref()
                    .map(|constructor| (constructor.params.as_slice(), args.len())),
                hir::ExprKind::AsyncCall { callee, args }
                | hir::ExprKind::AsyncHandleCreate { callee, args, .. } => {
                    declared_async_callee_parameters(hir, callee, &expression.pos)
                        .ok()
                        .map(|parameters| (parameters, args.len()))
                }
                _ => None,
            };
            if let Some((parameters, supplied)) = parameters {
                for default in parameters
                    .iter()
                    .skip(supplied)
                    .filter_map(|parameter| parameter.default.as_ref())
                {
                    if seen_defaults.insert(default as *const hir::Expr) {
                        roots.push(default);
                    }
                }
            }
        });
    }
}

fn suspend_position(terminator: &l::Terminator) -> Option<&Pos> {
    let l::Terminator::Suspend { pos, .. } = terminator else {
        return None;
    };
    Some(pos)
}

fn expected_call_operands(hir: &hir::Module, expr: &hir::Expr) -> Result<Option<usize>, String> {
    let pos = &expr.pos;
    Ok(match &expr.kind {
        hir::ExprKind::Call { callee, args } => match callee {
            hir::Callee::Ambient(hir::AmbientFn::Unreachable) => None,
            hir::Callee::Func(name) => Some(declared_function(hir, name, pos)?.params.len()),
            hir::Callee::Foreign(name) => Some(
                declared_foreign(hir, name, pos)?
                    .params
                    .iter()
                    .map(|parameter| {
                        usize::from(matches!(parameter.ty, subscript_compiler::Type::Array(_))) + 1
                    })
                    .sum(),
            ),
            hir::Callee::Arr(operation) if static_array_callback(*operation, args).is_some() => {
                static_array_callback(*operation, args).and_then(|callback| match &callback.ty {
                    Type::Func(function) => Some(
                        function.params.len()
                            + usize::from(matches!(callback.kind, hir::ExprKind::Lambda { .. })),
                    ),
                    _ => None,
                })
            }
            hir::Callee::Map(hir::MapFn::ForEach) | hir::Callee::Set(hir::SetFn::ForEach) => {
                args.get(1).and_then(|callback| match &callback.ty {
                    subscript_compiler::Type::Func(function) => Some(function.params.len() + 1),
                    _ => None,
                })
            }
            hir::Callee::Method { recv, name } => {
                let Type::Class(class) = &recv.ty else {
                    return Ok(None);
                };
                Some(declared_method(hir, *class, name, pos)?.params.len() + 1)
            }
            hir::Callee::Value(_) => Some(args.len() + 1),
            hir::Callee::Ambient(_)
            | hir::Callee::ContextBytes { .. }
            | hir::Callee::Math(_)
            | hir::Callee::Num(_)
            | hir::Callee::Date(_)
            | hir::Callee::Text(_)
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
            hir::AsyncCallee::Function(name) => {
                Some(declared_function(hir, name, pos)?.params.len())
            }
            hir::AsyncCallee::Method { class, name, .. } => {
                Some(declared_method(hir, *class, name, pos)?.params.len() + 1)
            }
        },
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
        | hir::ExprKind::AsyncHandleAwait(_)
        | hir::ExprKind::TaskGroup { .. }
        | hir::ExprKind::AsyncAll { .. }
        | hir::ExprKind::AsyncHandleTransfer { .. }
        | hir::ExprKind::Cond { .. } => None,
    })
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
        | K::NarrowNonNull(_)
        | K::Coerce
        | K::BoxBoundaryValue { .. }
        | K::AddressOfValue
        | K::LoadAddress
        | K::LoadField(_)
        | K::Length
        | K::ForeignArrayData
        | K::ArrayWithCapacity
        | K::SetFromSource(_)
        | K::MapFromSource
        | K::IteratorCreate { .. }
        | K::IteratorBound => Arity::Exact(1),
        K::StringLiteral(_)
        | K::LoadLocal(_)
        | K::AddressOfLocal(_)
        | K::LoadGlobal(_)
        | K::AddressOfGlobal(_)
        | K::FunctionRef(_)
        | K::AllocateClass(_)
        | K::CatchEntry
        | K::ExceptionPark
        | K::ExceptionResume
        | K::AwaitRaise
        | K::Zero => Arity::Exact(0),
        K::Throw => Arity::Exact(3),
        K::StoreLocal(_) | K::StoreGlobal(_) => Arity::Exact(1),
        K::Binary(_) | K::AddressOfIndex { .. } | K::StoreAddress => Arity::Exact(2),
        K::AddressOfField(_) => Arity::Exact(1),
        K::ArrayLiteral => Arity::Variable,
        K::ArraySpreadLiteral(parts) => Arity::MatchesPayload(parts.len()),
        K::Template(parts) => Arity::MatchesPayload(
            parts
                .iter()
                .filter(|part| matches!(part, l::TemplatePart::Operand { .. }))
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
        K::TaskGroup(operation) => Arity::Exact(match operation {
            hir::TaskGroupOperation::Create => 0,
            hir::TaskGroupOperation::Add => 2,
            hir::TaskGroupOperation::Join | hir::TaskGroupOperation::Release => 1,
            _ => usize::MAX,
        }),
        K::AsyncAll
        | K::AsyncHandleRetain
        | K::AsyncHandleRelease
        | K::AsyncHandleArrayRetain
        | K::AsyncHandleArrayRelease => Arity::Exact(1),
        K::IteratorHasNext | K::IteratorValue | K::IteratorAdvance => Arity::Exact(3),
    }
}

fn all_declared_functions(hir: &hir::Module) -> impl Iterator<Item = &hir::Function> {
    hir.expression_owners().filter_map(|owner| match owner {
        hir::ExpressionOwner::Body {
            function: Some(function),
            ..
        } => Some(function),
        hir::ExpressionOwner::Expr(_) | hir::ExpressionOwner::Body { function: None, .. } => None,
    })
}

fn collect_return_positions<'a>(
    hir: &hir::Module,
    statements: &'a [hir::Stmt],
    visit: &mut impl FnMut(&'a Pos),
) {
    for statement in statements {
        match statement {
            hir::Stmt::Return { pos, .. } => visit(pos),
            hir::Stmt::If { then, els, .. } => {
                collect_return_positions(hir, then, visit);
                if let Some(els) = els {
                    collect_return_positions(hir, els, visit);
                }
            }
            hir::Stmt::While { body, .. }
            | hir::Stmt::For { body, .. }
            | hir::Stmt::ForOf { body, .. }
            | hir::Stmt::Block(body) => collect_return_positions(hir, body, visit),
            hir::Stmt::Switch { cases, .. } => {
                for case in cases {
                    collect_return_positions(hir, &case.body, visit);
                }
            }
            hir::Stmt::Try { body, handler, .. } => {
                collect_return_positions(hir, body, visit);
                if try_body_raises(hir, body) {
                    collect_return_positions(hir, handler, visit);
                }
            }
            hir::Stmt::Using { body, .. } => collect_return_positions(hir, body, visit),
            hir::Stmt::Let { .. }
            | hir::Stmt::Expr(_)
            | hir::Stmt::Break(_)
            | hir::Stmt::Continue(_)
            | hir::Stmt::Throw { .. } => {}
        }
        if stops_statement_sequence(hir, statement) {
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
    for owner in hir.expression_owners() {
        match owner {
            hir::ExpressionOwner::Expr(expression) => walk_expr(hir, expression, visit),
            hir::ExpressionOwner::Body { statements, .. } => {
                walk_statement_expression_roots(hir, statements, &mut |expression| {
                    walk_expr(hir, expression, visit);
                });
            }
        }
    }
}

// Describe exits in one exhaustive match. A new HIR statement must supply its exits.
// Break ends an arm's sequence, but becomes fallthrough at its owning switch.
#[derive(Clone, Copy)]
struct SequenceExits {
    next: bool,
    breaks: bool,
}

impl SequenceExits {
    const NEXT: Self = Self {
        next: true,
        breaks: false,
    };
    const STOP: Self = Self {
        next: false,
        breaks: false,
    };

    fn union(self, other: Self) -> Self {
        Self {
            next: self.next || other.next,
            breaks: self.breaks || other.breaks,
        }
    }

    fn then(self, other: Self) -> Self {
        if self.next {
            Self {
                next: other.next,
                breaks: self.breaks || other.breaks,
            }
        } else {
            self
        }
    }
}

fn sequence_exits(hir: &hir::Module, body: &[hir::Stmt]) -> SequenceExits {
    body.iter().fold(SequenceExits::NEXT, |exits, statement| {
        exits.then(statement_exits(hir, statement))
    })
}

fn statement_exits(hir: &hir::Module, statement: &hir::Stmt) -> SequenceExits {
    match statement {
        hir::Stmt::Return { .. } | hir::Stmt::Continue(_) | hir::Stmt::Throw { .. } => {
            SequenceExits::STOP
        }
        // The lowering emits the handler only when a raise site of the
        // `try` block reaches it (compiler.md §115.6 rule 2).
        hir::Stmt::Try { body, handler, .. } => {
            if try_body_raises(hir, body) {
                sequence_exits(hir, body).union(sequence_exits(hir, handler))
            } else {
                sequence_exits(hir, body)
            }
        }
        // A hook that raises leaves through the exception edge, so the
        // exits of the scope are the exits of its body.
        hir::Stmt::Using { body, .. } => sequence_exits(hir, body),
        hir::Stmt::Break(_) => SequenceExits {
            next: false,
            breaks: true,
        },
        hir::Stmt::Expr(expr) => {
            if matches!(
                &expr.kind,
                hir::ExprKind::Call {
                    callee: hir::Callee::Ambient(hir::AmbientFn::Unreachable),
                    ..
                }
            ) {
                SequenceExits::STOP
            } else {
                SequenceExits::NEXT
            }
        }
        hir::Stmt::Block(body) => sequence_exits(hir, body),
        hir::Stmt::If {
            cond, then, els, ..
        } => {
            let then = sequence_exits(hir, then);
            let els = els
                .as_deref()
                .map_or(SequenceExits::NEXT, |els| sequence_exits(hir, els));
            match cond.kind {
                hir::ExprKind::Bool(true) => then,
                hir::ExprKind::Bool(false) => els,
                _ => then.union(els),
            }
        }
        hir::Stmt::Switch { disc, cases, .. } => {
            let mut exits = if cases.iter().any(|case| case.test.is_none())
                || matches!(disc.ty, subscript_compiler::Type::StringAlias(_))
            {
                SequenceExits::STOP
            } else {
                SequenceExits::NEXT
            };
            let mut tail = SequenceExits::NEXT;
            for case in cases.iter().rev() {
                tail = sequence_exits(hir, &case.body).then(tail);
                exits = exits.union(tail);
            }
            SequenceExits {
                next: exits.next || exits.breaks,
                breaks: false,
            }
        }
        hir::Stmt::For {
            init, cond, body, ..
        } => {
            let init = init
                .as_deref()
                .map_or(SequenceExits::NEXT, |init| statement_exits(hir, init));
            let next = cond
                .as_ref()
                .is_some_and(|cond| !matches!(cond.kind, hir::ExprKind::Bool(true)))
                || sequence_exits(hir, body).breaks;
            init.then(SequenceExits {
                next,
                breaks: false,
            })
        }
        hir::Stmt::While { cond, body, .. } => SequenceExits {
            next: !matches!(cond.kind, hir::ExprKind::Bool(true))
                || sequence_exits(hir, body).breaks,
            breaks: false,
        },
        hir::Stmt::Let { .. } | hir::Stmt::ForOf { .. } => SequenceExits::NEXT,
    }
}

/// Whether a lowered raise site of `body` routes to the handler of the
/// `try` that holds `body`. A lambda body is its own function, and a raise
/// site inside a nested `try` block routes to the nested handler.
fn try_body_raises(hir: &hir::Module, body: &[hir::Stmt]) -> bool {
    fn raises_in_expression(hir: &hir::Module, expression: &hir::Expr) -> bool {
        if matches!(expression.kind, hir::ExprKind::Lambda { .. }) {
            return false;
        }
        expression
            .trap_sites(hir)
            .iter()
            .any(|site| matches!(site, hir::TrapSite::Raise { .. }))
            || expression.children().into_iter().any(|child| match child {
                hir::HirChild::Expr(child) => raises_in_expression(hir, child),
                hir::HirChild::Stmt(_) => false,
            })
    }
    fn raises_in_statement(hir: &hir::Module, statement: &hir::Stmt) -> bool {
        match statement {
            hir::Stmt::Let { init, .. } | hir::Stmt::Expr(init) => raises_in_expression(hir, init),
            hir::Stmt::Return { value, .. } => value
                .as_ref()
                .is_some_and(|value| raises_in_expression(hir, value)),
            hir::Stmt::If {
                cond, then, els, ..
            } => {
                raises_in_expression(hir, cond)
                    || raises_in_sequence(hir, then)
                    || els
                        .as_deref()
                        .is_some_and(|els| raises_in_sequence(hir, els))
            }
            hir::Stmt::While { cond, body, .. } => {
                raises_in_expression(hir, cond) || raises_in_sequence(hir, body)
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                init.as_deref()
                    .is_some_and(|init| raises_in_statement(hir, init))
                    || cond
                        .as_ref()
                        .is_some_and(|cond| raises_in_expression(hir, cond))
                    || raises_in_sequence(hir, body)
                    || step
                        .as_ref()
                        .is_some_and(|step| raises_in_expression(hir, step))
            }
            hir::Stmt::ForOf { subject, body, .. } => {
                raises_in_expression(hir, subject) || raises_in_sequence(hir, body)
            }
            hir::Stmt::Switch { disc, cases, .. } => {
                raises_in_expression(hir, disc)
                    || cases.iter().any(|case| {
                        case.test
                            .as_ref()
                            .is_some_and(|test| raises_in_expression(hir, test))
                            || raises_in_sequence(hir, &case.body)
                    })
            }
            hir::Stmt::Block(body) => raises_in_sequence(hir, body),
            hir::Stmt::Break(_) | hir::Stmt::Continue(_) => false,
            hir::Stmt::Throw { .. } => true,
            hir::Stmt::Try { body, handler, .. } => {
                raises_in_sequence(hir, body) && raises_in_sequence(hir, handler)
            }
            // The exception edge resumes after the hooks. A hook that
            // raises on a normal exit leaves the scope with an exception
            // (compiler.md §115.5 rules 3 and 7).
            hir::Stmt::Using { bindings, body, .. } => {
                raises_in_sequence(hir, body)
                    || (bindings
                        .iter()
                        .any(|binding| using::hook_raises(hir, binding))
                        && using::has_normal_exit(hir, body))
            }
        }
    }
    fn raises_in_sequence(hir: &hir::Module, statements: &[hir::Stmt]) -> bool {
        for current in statements {
            if raises_in_statement(hir, current) {
                return true;
            }
            if stops_statement_sequence(hir, current) {
                break;
            }
        }
        false
    }
    raises_in_sequence(hir, body)
}

fn stops_statement_sequence(hir: &hir::Module, statement: &hir::Stmt) -> bool {
    !statement_exits(hir, statement).next
}

fn walk_expr<'a>(hir: &hir::Module, expr: &'a hir::Expr, visit: &mut impl FnMut(&'a hir::Expr)) {
    visit(expr);
    use hir::ExprKind as K;
    match &expr.kind {
        K::Assign { target, value, .. } => {
            walk_place_children(hir, target, visit);
            walk_expr(hir, value, visit);
            return;
        }
        K::Lambda { body, .. } => {
            for child in expr.children() {
                if let hir::HirChild::Expr(expression) = child {
                    walk_expr(hir, expression, visit);
                }
            }
            walk_statement_expression_roots(hir, body, &mut |expression| {
                walk_expr(hir, expression, visit);
            });
            return;
        }
        _ => {}
    }
    for child in expr.children() {
        match child {
            hir::HirChild::Expr(expr) => walk_expr(hir, expr, visit),
            hir::HirChild::Stmt(statement) => {
                walk_statement_expression_roots(
                    hir,
                    std::slice::from_ref(statement),
                    &mut |expression| {
                        walk_expr(hir, expression, visit);
                    },
                );
            }
        }
    }
}

fn walk_place_children<'a>(
    hir: &hir::Module,
    expr: &'a hir::Expr,
    visit: &mut impl FnMut(&'a hir::Expr),
) {
    match &expr.kind {
        hir::ExprKind::Field { obj, .. } => walk_expr(hir, obj, visit),
        hir::ExprKind::Index { obj, index, .. } => {
            walk_expr(hir, obj, visit);
            walk_expr(hir, index, visit);
        }
        hir::ExprKind::Local(..) | hir::ExprKind::Global(_) | hir::ExprKind::This => {}
        _ => walk_expr(hir, expr, visit),
    }
}

#[cfg(test)]
#[path = "lir_facts/call_lookup_tests.rs"]
mod call_lookup_tests;

#[cfg(test)]
mod sequence_tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    fn checked(source: &str) -> (hir::Module, l::Module) {
        let hir = check_program(&[SourceFile::new("sequence.ts", source)])
            .expect("sequence witness checks");
        let lir = subscript_codegen::lir::lower_module(&hir)
            .unwrap_or_else(|error| panic!("sequence witness lowers: {error:?}\n{source}"));
        (hir, lir)
    }

    #[test]
    fn statement_exits_preserve_execution_facts() {
        for body in [
            "{ return; }",
            "switch (0) { case 0: return; default: return; }",
            "switch (0) { default: { return; } case 0: if (stop) { return; } else { return; } }",
            "switch (0) { case 0: return; } print(\"after\");",
            "switch (0) { case 0: return; default: print(\"default\"); } print(\"after\");",
            "switch (0) { case 0: break; default: return; } print(\"after\");",
            "switch (0) { case 0: if (stop) { break; } return; default: return; } print(\"after\");",
            "switch (0) { case 0: default: return; }",
            "switch (0) { case 0: switch (1) { default: break; } return; default: return; }",
            "if (stop) { return; } else { { return; } }",
            "if (stop) { { return; } } print(\"fallthrough\");",
            "if (stop) { return; } else { print(\"else\"); } print(\"after\");",
            "{ print(\"block\"); } print(\"after\");",
        ] {
            let source = format!(
                "class R {{ [Symbol.dispose](): void {{}} }}
                 function run(stop: boolean): void {{
                   using resource: R | null = new R();
                   {body}
                 }}
                 export function main(): void {{ run(true); run(false); }}"
            );
            let (hir, mut lir) = checked(&source);
            assert_eq!(dropped_facts(&hir, &lir), Vec::<String>::new(), "{body}");

            // Delete a reachable call, not its trap metadata. The check must still fail.
            let block = lir
                .functions
                .iter_mut()
                .flat_map(|function| &mut function.blocks)
                .find(|block| {
                    block.instructions.iter().any(|instruction| {
                        matches!(instruction.kind, l::InstructionKind::Call(_))
                            && instruction
                                .traps
                                .iter()
                                .any(|trap| matches!(trap.kind, l::TrapKind::Call))
                    })
                })
                .expect("reachable call block");
            let index = block
                .instructions
                .iter()
                .position(|instruction| {
                    matches!(instruction.kind, l::InstructionKind::Call(_))
                        && instruction
                            .traps
                            .iter()
                            .any(|trap| matches!(trap.kind, l::TrapKind::Call))
                })
                .expect("reachable call");
            block.instructions.remove(index);
            assert!(
                dropped_facts(&hir, &lir)
                    .iter()
                    .any(|finding| { finding.contains("trap \"Call\" carries") }),
                "the missing reachable call must fail: {body}"
            );
        }
    }

    #[test]
    fn a_conditionless_for_drops_its_trailing_execution_facts() {
        let source = "function run(a: i32[], flag: boolean): i32 {
                        if (flag) { for (;;) { return 1; } } return a[0];
                      }
                      export function main(): void {}";
        let (hir, lir) = checked(source);
        assert!(dropped_facts(&hir, &lir).is_empty());
        let function = hir.functions.iter().find(|f| f.name == "run").unwrap();
        let hir::Stmt::If { then, .. } = &function.body[0] else {
            panic!("the if");
        };
        assert!(stops_statement_sequence(&hir, &then[0]));
        let reads = lir
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                instruction
                    .traps
                    .iter()
                    .any(|trap| matches!(trap.kind, l::TrapKind::IndexRead))
            })
            .count();
        assert_eq!(
            reads, 1,
            "the read after the `if` stays; nothing follows the loop"
        );
        let (hir, lir) = checked(
            "function run(a: i32[]): i32 { for (;;) { return 1; } return a[0]; }
             export function main(): void {}",
        );
        assert!(dropped_facts(&hir, &lir).is_empty());
        let reads = lir
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                instruction
                    .traps
                    .iter()
                    .any(|trap| matches!(trap.kind, l::TrapKind::IndexRead))
            })
            .count();
        assert_eq!(
            reads, 0,
            "control cannot leave the loop (compiler.md §101 rule 2)"
        );
    }

    #[test]
    fn loop_conditions_select_trailing_execution_facts() {
        for (body, stops) in [
            ("while (true) { return; }", true),
            ("for (; true;) { return; }", true),
            ("while (a.length > 0) { return; }", false),
            ("for (; a.length > 0;) { return; }", false),
        ] {
            let source = format!(
                "function run(a: i32[]): i32 {{ {body} return a[0]; }}
                 export function main(): void {{}}"
            )
            .replace("{ return; }", "{ return 1; }");
            let (hir, mut lir) = checked(&source);
            assert!(dropped_facts(&hir, &lir).is_empty(), "{body}");
            let function = hir.functions.iter().find(|f| f.name == "run").unwrap();
            assert_eq!(
                stops_statement_sequence(&hir, &function.body[0]),
                stops,
                "{body}"
            );
            let mut removed = 0;
            for block in lir.functions.iter_mut().flat_map(|f| &mut f.blocks) {
                block.instructions.retain(|instruction| {
                    let keep = !instruction
                        .traps
                        .iter()
                        .any(|trap| matches!(trap.kind, l::TrapKind::IndexRead));
                    if !keep {
                        removed += 1;
                    }
                    keep
                });
            }
            assert_eq!(removed > 0, !stops, "{body}");
            assert_eq!(dropped_facts(&hir, &lir).is_empty(), stops, "{body}");
        }
    }

    #[test]
    fn blocks_preserve_break_and_continue_sequence_exits() {
        let (hir, lir) = checked(
            "export function main(): void {
               for (let i: i32 = 0; i < 2; i += 1) {
                 if (i === 0) { { continue; } } else { { break; } }
               }
               print(\"after\");
             }",
        );
        assert!(dropped_facts(&hir, &lir).is_empty());
        let pos = hir
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main")
            .pos
            .clone();
        assert!(stops_statement_sequence(
            &hir,
            &hir::Stmt::Block(vec![hir::Stmt::Break(pos.clone())])
        ));
        assert!(stops_statement_sequence(
            &hir,
            &hir::Stmt::Block(vec![hir::Stmt::Continue(pos)])
        ));
        assert!(!stops_statement_sequence(
            &hir,
            &hir::Stmt::Block(Vec::new())
        ));
    }
}
