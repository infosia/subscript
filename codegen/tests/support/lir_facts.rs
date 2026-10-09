//! Independent HIR/LIR execution-fact comparison for §68.2 item 12.

use std::collections::BTreeMap;

use subscript_compiler::hir;
use subscript_compiler::lir as l;
use subscript_compiler::{ClassId, Pos, Type};

#[path = "lir_facts/async_environments.rs"]
mod async_environments;
#[path = "lir_facts/boundary.rs"]
mod boundary;
#[path = "lir_facts/iteration.rs"]
mod iteration;
#[path = "lir_facts/traps.rs"]
mod traps;

use boundary::{
    compare_boundary_boxes, compare_foreign_array_snapshots, compare_terminator_positions,
};
use iteration::{compare_iterator_bounds, compare_static_array_callbacks, static_array_callback};
use traps::{collect_trap_expression, compare_traps};

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
    let environments = async_environments::compare(hir, lir, findings);
    if hir.classes.len() + environments != lir.classes.len() {
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
        hir::Callee::Standard(_)
        | hir::Callee::Ambient(_)
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
        walk_placed_statement_expression_roots(hir, &function.body, visit);
    }
    walk_placed_statement_expression_roots(hir, &hir.top_level, visit);
}

fn walk_statement_expression_roots<'a>(
    hir: &hir::Module,
    statements: &'a [hir::Stmt],
    visit: &mut impl FnMut(&'a hir::Expr),
) {
    walk_statement_expression_roots_mode(hir, statements, visit, true);
}

// Cleanup placements count each finalizer through hook_facts, rather than its declaration.
fn walk_placed_statement_expression_roots<'a>(
    hir: &hir::Module,
    statements: &'a [hir::Stmt],
    visit: &mut impl FnMut(&'a hir::Expr),
) {
    walk_statement_expression_roots_mode(hir, statements, visit, false);
}

fn walk_statement_expression_roots_mode<'a>(
    hir: &hir::Module,
    statements: &'a [hir::Stmt],
    visit: &mut impl FnMut(&'a hir::Expr),
    lexical_finalizers: bool,
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
                cond,
                then,
                els,
                pos: _,
            } => {
                visit(cond);
                walk_statement_expression_roots_mode(hir, then, visit, lexical_finalizers);
                if let Some(els) = els {
                    walk_statement_expression_roots_mode(hir, els, visit, lexical_finalizers);
                }
            }
            hir::Stmt::While { cond, body, pos: _ } => {
                visit(cond);
                walk_statement_expression_roots_mode(hir, body, visit, lexical_finalizers);
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                pos: _,
            } => {
                if let Some(init) = init {
                    walk_statement_expression_roots_mode(
                        hir,
                        std::slice::from_ref(init),
                        visit,
                        lexical_finalizers,
                    );
                }
                if let Some(cond) = cond {
                    visit(cond);
                }
                walk_statement_expression_roots_mode(hir, body, visit, lexical_finalizers);
                walk_statement_expression_roots_mode(hir, step, visit, lexical_finalizers);
            }
            hir::Stmt::ForOf {
                subject,
                body,
                name: _,
                ty: _,
                kind: _,
                pos: _,
            }
            | hir::Stmt::GeneratorForOf {
                subject,
                body,
                name: _,
                ty: _,
                mutable: _,
                pos: _,
            } => {
                visit(subject);
                walk_statement_expression_roots_mode(hir, body, visit, lexical_finalizers);
            }
            hir::Stmt::Switch {
                disc,
                cases,
                pos: _,
            } => {
                visit(disc);
                for case in cases {
                    if let Some(test) = &case.test {
                        visit(test);
                    }
                    walk_statement_expression_roots_mode(
                        hir,
                        &case.body,
                        visit,
                        lexical_finalizers,
                    );
                }
            }
            hir::Stmt::Block(body) => {
                walk_statement_expression_roots_mode(hir, body, visit, lexical_finalizers)
            }
            hir::Stmt::Break(_) | hir::Stmt::Continue(_) => {}
            hir::Stmt::Throw { value, .. } => visit(value),
            // The lowering emits a handler only when a raise site of the
            // `try` block reaches it (compiler.md §115.6 rule 2).
            hir::Stmt::Try {
                body,
                handler,
                binding: _,
                pos: _,
            } => {
                walk_statement_expression_roots_mode(hir, body, visit, lexical_finalizers);
                if try_body_raises(hir, body) {
                    walk_statement_expression_roots_mode(hir, handler, visit, lexical_finalizers);
                }
            }
            // The hooks are not HIR expressions; `using::hook_facts` derives
            // their placements (compiler.md §115.5 rule 5).
            hir::Stmt::Using {
                body,
                bindings: _,
                finalizer,
                pos: _,
            } => {
                walk_statement_expression_roots_mode(hir, body, visit, lexical_finalizers);
                if let Some(finalizer) = finalizer.as_ref().filter(|_| lexical_finalizers) {
                    walk_statement_expression_roots_mode(hir, finalizer, visit, lexical_finalizers);
                }
            }
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
                        | l::InstructionKind::HostCompletion { .. }
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
                let pos = suspend_position(&block.terminator).unwrap_or(&function.pos);
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
            hir::Callee::Standard(op) => Some(op.parameters().len()),
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
        K::GeneratorClose => Arity::Exact(1),
        K::GeneratorIsClosing | K::GeneratorFinalizer(_) => Arity::Exact(0),
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
        | K::ExceptionMessage
        | K::ExceptionPosition
        | K::ExceptionPark
        | K::ExceptionResume
        | K::AwaitRaise
        | K::Zero => Arity::Exact(0),
        K::Throw | K::ExceptionRestore => Arity::Exact(3),
        K::FinalizerEnter(_) => Arity::Variable,
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
                        .filter(|parameter| {
                            matches!(
                                parameter.kind,
                                l::ParameterKind::Capture | l::ParameterKind::OwnedEnvironment
                            )
                        })
                        .count()
                })
                .unwrap_or(usize::MAX);
            Arity::Exact(captures)
        }
        K::HostCompletion { target, .. } => match target {
            l::HostCompletionTarget::Standard(op) => Arity::Exact(op.parameters().len()),
            // A missing foreign function gives an arity that no operand list meets.
            l::HostCompletionTarget::Foreign(function) => Arity::Exact(
                lir.foreign_functions
                    .get(function.0 as usize)
                    .map_or(usize::MAX, |f| {
                        f.parameters
                            .iter()
                            .filter(|p| {
                                p.foreign_provenance
                                    != Some(l::ForeignTypeProvenance::CompletionEndpoint)
                            })
                            .map(|p| if matches!(p.ty, Type::Array(_)) { 2 } else { 1 })
                            .sum()
                    }),
            ),
            _ => Arity::Exact(usize::MAX),
        },
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
            hir::Stmt::If {
                then,
                els,
                cond: _,
                pos: _,
            } => {
                collect_return_positions(hir, then, visit);
                if let Some(els) = els {
                    collect_return_positions(hir, els, visit);
                }
            }
            hir::Stmt::While {
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
            | hir::Stmt::ForOf {
                body,
                name: _,
                ty: _,
                subject: _,
                kind: _,
                pos: _,
            }
            | hir::Stmt::GeneratorForOf {
                body,
                name: _,
                ty: _,
                mutable: _,
                subject: _,
                pos: _,
            }
            | hir::Stmt::Block(body) => collect_return_positions(hir, body, visit),
            hir::Stmt::Switch {
                cases,
                disc: _,
                pos: _,
            } => {
                for case in cases {
                    collect_return_positions(hir, &case.body, visit);
                }
            }
            hir::Stmt::Try {
                body,
                handler,
                binding: _,
                pos: _,
            } => {
                collect_return_positions(hir, body, visit);
                if try_body_raises(hir, body) {
                    collect_return_positions(hir, handler, visit);
                }
            }
            hir::Stmt::Using {
                body,
                bindings: _,
                finalizer,
                pos: _,
            } => {
                collect_return_positions(hir, body, visit);
                if let Some(finalizer) = finalizer {
                    collect_return_positions(hir, finalizer, visit);
                }
            }
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

fn walk_placed_module_expressions<'a>(hir: &'a hir::Module, visit: &mut impl FnMut(&'a hir::Expr)) {
    for owner in hir.expression_owners() {
        match owner {
            hir::ExpressionOwner::Expr(expression) => walk_expr(hir, expression, visit),
            hir::ExpressionOwner::Body { statements, .. } => {
                walk_placed_statement_expression_roots(hir, statements, &mut |expression| {
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
        hir::Stmt::Try {
            body,
            handler,
            binding: _,
            pos: _,
        } => {
            if try_body_raises(hir, body) {
                sequence_exits(hir, body).union(sequence_exits(hir, handler))
            } else {
                sequence_exits(hir, body)
            }
        }
        // A hook that raises leaves through the exception edge, so the
        // exits of the scope are the exits of its body.
        hir::Stmt::Using {
            body,
            finalizer,
            bindings: _,
            pos: _,
        } => {
            let prior = sequence_exits(hir, body);
            let Some(finalizer) = finalizer else {
                return prior;
            };
            let tail = sequence_exits(hir, finalizer);
            SequenceExits {
                next: prior.next && tail.next,
                breaks: tail.breaks || (tail.next && prior.breaks),
            }
        }
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
            cond,
            then,
            els,
            pos: _,
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
        hir::Stmt::Switch {
            disc,
            cases,
            pos: _,
        } => {
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
            init,
            cond,
            body,
            step: _,
            pos: _,
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
        hir::Stmt::While { cond, body, pos: _ } => SequenceExits {
            next: !matches!(cond.kind, hir::ExprKind::Bool(true))
                || sequence_exits(hir, body).breaks,
            breaks: false,
        },
        hir::Stmt::Let { .. }
        | hir::Stmt::ForOf {
            name: _,
            ty: _,
            subject: _,
            kind: _,
            body: _,
            pos: _,
        }
        | hir::Stmt::GeneratorForOf {
            name: _,
            ty: _,
            mutable: _,
            subject: _,
            body: _,
            pos: _,
        } => SequenceExits::NEXT,
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
                cond,
                then,
                els,
                pos: _,
            } => {
                raises_in_expression(hir, cond)
                    || raises_in_sequence(hir, then)
                    || els
                        .as_deref()
                        .is_some_and(|els| raises_in_sequence(hir, els))
            }
            hir::Stmt::While { cond, body, pos: _ } => {
                raises_in_expression(hir, cond) || raises_in_sequence(hir, body)
            }
            hir::Stmt::For {
                init,
                cond,
                step,
                body,
                pos: _,
            } => {
                init.as_deref()
                    .is_some_and(|init| raises_in_statement(hir, init))
                    || cond
                        .as_ref()
                        .is_some_and(|cond| raises_in_expression(hir, cond))
                    || raises_in_sequence(hir, body)
                    || raises_in_sequence(hir, step)
            }
            hir::Stmt::ForOf {
                subject,
                body,
                name: _,
                ty: _,
                kind: _,
                pos: _,
            }
            | hir::Stmt::GeneratorForOf {
                subject,
                body,
                name: _,
                ty: _,
                mutable: _,
                pos: _,
            } => raises_in_expression(hir, subject) || raises_in_sequence(hir, body),
            hir::Stmt::Switch {
                disc,
                cases,
                pos: _,
            } => {
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
            hir::Stmt::Try {
                body,
                handler,
                binding: _,
                pos: _,
            } => raises_in_sequence(hir, body) && raises_in_sequence(hir, handler),
            // The exception edge resumes after the hooks. A hook that
            // raises on a normal exit leaves the scope with an exception
            // (compiler.md §115.5 rules 3 and 7).
            hir::Stmt::Using {
                bindings,
                body,
                finalizer,
                pos: _,
            } => {
                let prior = raises_in_sequence(hir, body)
                    || (bindings
                        .iter()
                        .any(|binding| using::hook_raises(hir, binding))
                        && using::has_normal_exit(hir, body));
                finalizer.as_ref().map_or(prior, |body| {
                    raises_in_sequence(hir, body) || (sequence_exits(hir, body).next && prior)
                })
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
#[path = "lir_facts/sequence_tests.rs"]
mod sequence_tests;
