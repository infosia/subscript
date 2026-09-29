//! Checker passes and operation signatures.

use super::*;

fn normalize_operation_parameter_types(
    target: &hir::OperationSignatureTarget,
    parameters: &mut [Type],
) {
    let array_element = parameters.first().and_then(|receiver| match receiver {
        Type::Array(element) => Some((**element).clone()),
        _ => None,
    });
    match target {
        hir::OperationSignatureTarget::BuiltinMethod(hir::BuiltinMethod::ArrayPush) => {
            if let (Some(element), Some(value)) = (array_element, parameters.get_mut(1)) {
                *value = element;
            }
        }
        hir::OperationSignatureTarget::Arr(function) => match function {
            hir::ArrFn::IndexOf
            | hir::ArrFn::LastIndexOf
            | hir::ArrFn::Includes
            | hir::ArrFn::Fill
            | hir::ArrFn::Unshift => {
                if let (Some(element), Some(value)) = (array_element, parameters.get_mut(1)) {
                    *value = element;
                }
            }
            hir::ArrFn::Reduce | hir::ArrFn::ReduceRight => {
                let accumulator = parameters.get(1).and_then(|callback| match callback {
                    Type::Func(signature) => signature.params.first().cloned(),
                    _ => None,
                });
                if let (Some(accumulator), Some(initial)) = (accumulator, parameters.get_mut(2)) {
                    *initial = accumulator;
                }
            }
            _ => {}
        },
        hir::OperationSignatureTarget::Map(function) => {
            let pair = parameters.first().and_then(|receiver| match receiver {
                Type::Map(key, value) => Some(((**key).clone(), (**value).clone())),
                _ => None,
            });
            if let Some((key, value)) = pair {
                if matches!(
                    function,
                    hir::MapFn::Get
                        | hir::MapFn::GetOr
                        | hir::MapFn::Set
                        | hir::MapFn::Has
                        | hir::MapFn::Delete
                ) {
                    if let Some(parameter) = parameters.get_mut(1) {
                        *parameter = key;
                    }
                }
                if matches!(function, hir::MapFn::GetOr | hir::MapFn::Set) {
                    if let Some(parameter) = parameters.get_mut(2) {
                        *parameter = value;
                    }
                }
            }
        }
        hir::OperationSignatureTarget::Set(function) => {
            let key = parameters.first().and_then(|receiver| match receiver {
                Type::Set(key) => Some((**key).clone()),
                _ => None,
            });
            if matches!(
                function,
                hir::SetFn::Add | hir::SetFn::Has | hir::SetFn::Delete
            ) {
                if let (Some(key), Some(parameter)) = (key, parameters.get_mut(1)) {
                    *parameter = key;
                }
            }
        }
        hir::OperationSignatureTarget::Worker(function) => {
            let message = parameters
                .first()
                .and_then(|receiver| match (function, receiver) {
                    (hir::WorkerFn::Post, Type::Worker(input, _)) => Some((**input).clone()),
                    (hir::WorkerFn::OutboxPost, Type::Outbox(message)) => Some((**message).clone()),
                    _ => None,
                });
            if let (Some(message), Some(parameter)) = (message, parameters.get_mut(1)) {
                *parameter = message;
            }
        }
        _ => {}
    }
}

fn operation_signatures(module: &mut hir::Module) -> Vec<hir::OperationSignature> {
    fn visit_child(child: hir::HirChild<'_>, signatures: &mut Vec<hir::OperationSignature>) {
        match child {
            hir::HirChild::Expr(expression) => visit_expr(expression, signatures),
            hir::HirChild::Stmt(statement) => visit_stmt(statement, signatures),
        }
    }

    fn visit_stmt(statement: &hir::Stmt, signatures: &mut Vec<hir::OperationSignature>) {
        for child in statement.children() {
            visit_child(child, signatures);
        }
    }

    fn visit_expr(expression: &hir::Expr, signatures: &mut Vec<hir::OperationSignature>) {
        for child in expression.children() {
            visit_child(child, signatures);
        }
        let hir::ExprKind::Call { callee, args } = &expression.kind else {
            return;
        };
        let Some((target, receiver)) = hir::operation_signature_target(callee) else {
            return;
        };
        let mut parameter_types = receiver
            .into_iter()
            .cloned()
            .chain(args.iter().map(|argument| argument.ty.clone()))
            .collect::<Vec<_>>();
        normalize_operation_parameter_types(&target, &mut parameter_types);
        let signature = hir::OperationSignature {
            target,
            parameter_types,
            return_type: (expression.ty != Type::Void).then(|| expression.ty.clone()),
        };
        if !signatures.contains(&signature) {
            signatures.push(signature);
        }
    }

    fn visit_body(body: &[hir::Stmt], signatures: &mut Vec<hir::OperationSignature>) {
        for statement in body {
            visit_stmt(statement, signatures);
        }
    }

    let mut signatures = Vec::new();
    for owner in module.expression_owners_mut() {
        match owner {
            hir::ExpressionOwnerMut::Expr(expression) => visit_expr(expression, &mut signatures),
            hir::ExpressionOwnerMut::Body(body) => visit_body(body, &mut signatures),
        }
    }
    signatures
}

/// Runs the checker over a parsed program.
pub(crate) fn run(
    prog: &ParsedProgram,
    options: &CheckOptions,
) -> Result<hir::Module, Vec<Diagnostic>> {
    // compiler.md §124: resolved HIR supplies loop effects before the flow check.
    let mut provisional = run_with_effects(prog, options, None)?;
    let analysis = narrowing::Analysis::from_module(&mut provisional);
    run_with_effects(prog, options, Some(analysis))
}

fn run_with_effects(
    prog: &ParsedProgram,
    options: &CheckOptions,
    narrowing_analysis: Option<narrowing::Analysis>,
) -> Result<hir::Module, Vec<Diagnostic>> {
    let provisional = narrowing_analysis.is_none();
    let mut ck = Checker {
        narrowing_analysis,
        prog,
        diags: DiagnosticSink::default(),
        classes: Vec::new(),
        class_sigs: Vec::new(),
        class_ids: HashMap::new(),
        enums: Vec::new(),
        string_aliases: Vec::new(),
        fn_sigs: HashMap::new(),
        functions: Vec::new(),
        worker_entries: Vec::new(),
        global_sigs: HashMap::new(),
        globals: Vec::new(),
        generic_fns: HashMap::new(),
        generic_classes: HashMap::new(),
        instance_symbols: HashMap::new(),
        file_scopes: Vec::new(),
        exports: Vec::new(),
        top_level: Vec::new(),
        poison_missing_modules: options
            .poison_missing_modules
            .iter()
            .map(|specifier| normalize_module_specifier(specifier))
            .collect(),
        poisoned_imports: Vec::new(),
        cur_file: 0,
        subst: HashMap::new(),
        ambient_scope: HashMap::new(),
        foreign_sigs: HashMap::new(),
        foreign_defs: Vec::new(),
        foreign_mirrors: Vec::new(),
        foreign_mirror_ids: HashMap::new(),
        handle_classes: HashSet::new(),
        declared_classes: HashSet::new(),
        type_handle_classes: Vec::new(),
        boundary_classes: HashSet::new(),
        type_aliases: HashMap::new(),
        in_boundary: false,
        allow_wire_alias_boundary: false,
        in_assoc_key: false,
        in_json_argument: false,
        in_for_of_subject: false,
        aggregate_type_divergence: None,
        pending_layouts: Vec::new(),
        ambient_int_consts: HashMap::new(),
        next_for_of_id: 0,
        regex_literals: HashMap::new(),
        next_regex_literal_id: 0,
        next_using_switch_id: 0,
        error_class: ClassId(0),
        next_compound_local_id: 0,
        next_pattern_id: 0,
    };

    // Parse-time provenance has a fixed shape; this pass binds each record
    // to declarations in its own mirror before type resolution discards
    // the source spelling.
    for i in 0..prog.files.len() {
        if prog.files[i].dts {
            ck.collect_mirror_provenance(i);
        }
    }

    ck.error_class = ck.declare_error_class();

    // Pass A: collect top-level names. Mirror (`.d.ts`) declarations land
    // in the global ambient scope; program declarations in per-file scopes.
    for i in 0..prog.files.len() {
        ck.cur_file = i;
        ck.collect_file(i);
    }
    ck.resolve_imports();
    // Pass B: signatures. Mirror files first, in a boundary context (so
    // the boundary null forms resolve), then program files.
    ck.in_boundary = true;
    for i in 0..prog.files.len() {
        if prog.files[i].dts {
            ck.cur_file = i;
            ck.subst.clear();
            ck.resolve_mirror_signatures(i);
        }
    }
    ck.in_boundary = false;
    for i in 0..prog.files.len() {
        if !prog.files[i].dts {
            ck.cur_file = i;
            ck.subst.clear();
            ck.resolve_signatures(i);
        }
    }
    // Descriptor defaults need every class and function signature, but
    // constructing literals in ordinary bodies need the checked defaults.
    // Check all non-generic descriptor defaults in this intermediate pass.
    for i in 0..prog.files.len() {
        if !prog.files[i].dts {
            ck.cur_file = i;
            ck.subst.clear();
            ck.check_descriptor_defaults_in_file(i);
        }
    }
    // Pass C: bodies (program files only; mirror declarations have none).
    for i in 0..prog.files.len() {
        if !prog.files[i].dts {
            ck.cur_file = i;
            ck.subst.clear();
            ck.check_bodies(i);
        }
    }
    let initializer_diags = module_initializer_diagnostics(&ck);
    ck.diags.extend(initializer_diags);
    ck.validate_layouts();

    if provisional || ck.diags.is_empty() {
        let mut module = hir::Module {
            poisoned_imports: ck.poisoned_imports,
            classes: ck.classes,
            enums: ck.enums,
            string_aliases: ck.string_aliases,
            globals: ck.globals,
            synthesized_helpers: ck
                .functions
                .iter()
                .filter(|function| function.synthesized_helper)
                .map(|function| function.symbol.clone())
                .collect(),
            functions: ck.functions,
            worker_entries: ck.worker_entries,
            operation_signatures: Vec::new(),
            foreign_fns: ck.foreign_defs,
            foreign_mirrors: ck.foreign_mirrors,
            top_level: ck.top_level,
            initializer_can_raise: false,
            source_bytes: prog.source_bytes,
        };
        if ck.diags.is_empty() {
            let duplicates = identity::host_entry_diagnostics(&module);
            if !duplicates.is_empty() {
                return Err(duplicates);
            }
        }
        if provisional {
            return Ok(module);
        }
        capture::check(&mut module)?;
        module.operation_signatures = operation_signatures(&mut module);
        crate::trap_sites::decide_index_checks(&mut module);
        crate::raise_sites::decide_can_raise(&mut module);
        Ok(module)
    } else {
        Err(ck.diags.take())
    }
}
