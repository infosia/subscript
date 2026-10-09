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
        if let hir::Stmt::GeneratorForOf {
            subject,
            ty,
            name: _,
            mutable: _,
            body: _,
            pos: _,
        } = statement
        {
            let signature = hir::OperationSignature {
                target: hir::OperationSignatureTarget::BuiltinMethod(
                    hir::BuiltinMethod::GeneratorNext,
                ),
                parameter_types: vec![subject.ty.clone()],
                return_type: Some(Type::iter_result(ty.clone())),
            };
            if !signatures.contains(&signature) {
                signatures.push(signature);
            }
        }
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
    // compiler.md §135.1 rule 1: the bodies of the opaque check add theirs.
    let entry_file = host_entries::entry_file(prog)?;
    let initializer_files = init_order::files(prog, entry_file);
    let result = (|| {
        let (mut provisional, opaque_loops) =
            run_with_effects(prog, options, entry_file, &initializer_files, None)?;
        let mut analysis = narrowing::Analysis::from_module(&mut provisional);
        analysis.merge_loops(opaque_loops);
        run_with_effects(
            prog,
            options,
            entry_file,
            &initializer_files,
            Some(analysis),
        )
        .map(|(module, _)| module)
    })();
    result.map_err(|mut diagnostics| {
        let declaration_starts = initializer_files
            .iter()
            .map(|&file| {
                prog.files[file]
                    .module
                    .body
                    .iter()
                    .map(|item| {
                        let pos = prog.pos(item.span());
                        (pos.line, pos.col)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        // §156 rule 12: keep emission order within each top-level declaration.
        // Deferred expressions retain the source position of their declaration slot.
        diagnostics.sort_by_key(|d| {
            let module = initializer_files
                .iter()
                .position(|&file| prog.files[file].name == d.pos.file)
                .unwrap_or(initializer_files.len());
            let declaration = declaration_starts.get(module).and_then(|starts| {
                starts
                    .iter()
                    .rposition(|&start| start <= (d.pos.line, d.pos.col))
            });
            (module, declaration)
        });
        diagnostics
    })
}

fn run_with_effects(
    prog: &ParsedProgram,
    options: &CheckOptions,
    entry_file: Option<usize>,
    initializer_files: &[usize],
    narrowing_analysis: Option<narrowing::Analysis>,
) -> Result<(hir::Module, narrowing::Analysis), Vec<Diagnostic>> {
    let provisional = narrowing_analysis.is_none();
    let mut resolved_program = prog.files.iter().any(|file| {
        file.module.body.iter().any(|item| {
            matches!(item, ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import))
                if import.specifiers.iter().any(|specifier| matches!(specifier, ast::ImportSpecifier::Namespace(_))))
        })
    }).then(|| prog.clone());
    let mut ck = Checker {
        enabled_modules: options.enabled_modules.clone(),
        task_group_type: false,
        task_group_local: false,
        task_group_parameters: false,
        deciding_type: false,
        generic_callback_context: false,
        initializer_call: None,
        active_call: None,
        function_value_decision: false,
        initializer_root: None,
        static_method_owners: HashMap::new(),
        next_deferred_id: 0,
        expression_work: Vec::new(),
        deferred_expressions: Vec::new(),
        deferred_captures: HashMap::new(),
        pending_function_bodies: Vec::new(),
        narrowing_analysis,
        narrowing_helper_symbols: HashSet::new(),
        prog,
        diags: DiagnosticSink::default(),
        initializer_context: None,
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
        export_definitions: Vec::new(),
        top_level: Vec::new(),
        poison_missing_modules: options
            .poison_missing_modules
            .iter()
            .map(|specifier| normalize_module_specifier(specifier))
            .collect(),
        poisoned_imports: Vec::new(),
        type_only_value_uses: HashSet::new(),
        rejected_local_names: Vec::new(),
        rejected_module_exports: HashMap::new(),
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
        in_poisoned_context: false,
        in_json_argument: false,
        in_for_of_subject: false,
        aggregate_type_site: None,
        pending_layouts: Vec::new(),
        ambient_int_consts: HashMap::new(),
        next_for_of_id: 0,
        next_lambda_id: 0,
        regex_literals: HashMap::new(),
        next_regex_literal_id: 0,
        next_using_switch_id: 0,
        error_class: ClassId(0),
        next_compound_local_id: 0,
        next_pattern_id: 0,
        opaque_params: HashMap::new(),
        opaque_instances: HashSet::new(),
        opaque_root: false,
        instance_diagnostic_ranges: Vec::new(),
        instance_arguments: HashMap::new(),
        signatures_resolved: false,
        pending_instance_bodies: Vec::new(),
        instance_chain: Vec::new(),
        inferred_edges: HashMap::new(),
        growing_cycles: Vec::new(),
        growth_reports: Vec::new(),
        opaque_loop_effects: narrowing::Analysis::default(),
    };

    // Parse-time provenance has a fixed shape; this pass binds each record
    // to declarations in its own mirror before type resolution discards
    // the source spelling.
    for source in &prog.files {
        if !source.dts {
            continue;
        }
        for item in &source.module.body {
            let declaration = match item {
                ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::TsModule(module))) => module,
                _ => continue,
            };
            if let ast::TsModuleName::Str(name) = &declaration.id {
                let stem = normalize_module_specifier(&name.value);
                ck.poison_missing_modules.insert(stem.clone());
                if let Some(ast::TsNamespaceBody::TsModuleBlock(block)) = &declaration.body {
                    let names = ck.rejected_module_exports.entry(stem).or_default();
                    for item in &block.body {
                        let decl = match item {
                            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => {
                                &export.decl
                            }
                            ast::ModuleItem::Stmt(ast::Stmt::Decl(decl)) => decl,
                            _ => continue,
                        };
                        names.extend(
                            super::exports::declaration_names(decl)
                                .iter()
                                .map(|id| id.sym.to_string()),
                        );
                    }
                }
            }
        }
    }
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
    ck.resolve_exports();
    ck.resolve_imports();
    if let Some(program) = &mut resolved_program {
        ck.resolve_namespace_program(program);
        ck.prog = program;
    }
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
    ck.signatures_resolved = true;
    ck.decide_declarations();
    // §149: symbolic inference records §140 edges before concrete bodies request instances.
    let opaque_diagnostics = ck.check_generic_bodies_opaque();
    while !ck.pending_instance_bodies.is_empty() || !ck.pending_function_bodies.is_empty() {
        ck.check_pending_instance_bodies();
        ck.check_pending_function_bodies();
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
    let mut file_segments = vec![None; prog.files.len()];
    for (i, segment) in file_segments.iter_mut().enumerate() {
        if !prog.files[i].dts {
            let global_start = ck.globals.len();
            let top_start = ck.top_level.len();
            ck.cur_file = i;
            ck.subst.clear();
            ck.check_bodies(i);
            *segment = Some(hir::InitializerSegment {
                top_level: top_start..ck.top_level.len(),
                globals: (global_start..ck.globals.len()).collect(),
            });
        }
    }
    while !ck.pending_instance_bodies.is_empty() || !ck.pending_function_bodies.is_empty() {
        ck.check_pending_instance_bodies();
        ck.check_pending_function_bodies();
    }
    let local_start = ck.diags.len();
    ck.check_local_assignments();
    // The opaque merge removes repeated generic reads from concrete bodies (§135 rule 3).
    ck.instance_diagnostic_ranges
        .push(local_start..ck.diags.len());
    ck.merge_generic_body_diagnostics(opaque_diagnostics);
    let opaque_loops = std::mem::take(&mut ck.opaque_loop_effects);
    let regex_symbols: HashSet<_> = ck.regex_literals.values().map(String::as_str).collect();
    let regex_literal_globals: Vec<_> = ck
        .globals
        .iter()
        .enumerate()
        .filter(|(_, global)| regex_symbols.contains(global.symbol.full_text()))
        .map(|(index, _)| index)
        .collect();
    let regex_indices: HashSet<_> = regex_literal_globals.iter().copied().collect();
    // compiler.md §137 rule 3b: the scan and HIR share these owners.
    for segment in file_segments.iter_mut().flatten() {
        segment
            .globals
            .retain(|index| !regex_indices.contains(index));
    }
    let initializer_diags = module_initializer_diagnostics(&ck, initializer_files, &file_segments);
    ck.diags.extend(initializer_diags);
    ck.validate_layouts();

    if provisional || ck.diags.is_empty() {
        let host_exports = ck.host_exports(entry_file);
        let mut globals = ck.globals;
        let mut top_level = Vec::new();
        let mut initializer_segments = Vec::new();
        for &file in initializer_files {
            let Some(segment) = &file_segments[file] else {
                continue;
            };
            let start = top_level.len();
            top_level.extend_from_slice(&ck.top_level[segment.top_level.clone()]);
            let module_globals = segment.globals.clone();
            for &index in &module_globals {
                globals[index].initializer_index =
                    start + globals[index].initializer_index - segment.top_level.start;
            }
            initializer_segments.push(hir::InitializerSegment {
                top_level: start..top_level.len(),
                globals: module_globals,
            });
        }
        let mut module = hir::Module {
            host_entries: Vec::new(),
            entry_pos: Pos::new(
                entry_file.map_or("", |index| prog.files[index].name.as_str()),
                1,
                1,
            ),
            poisoned_imports: ck.poisoned_imports,
            classes: ck.classes,
            enums: ck.enums,
            string_aliases: ck.string_aliases,
            globals,
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
            top_level,
            initializer_modules: initializer_files
                .iter()
                .map(|&file| prog.files[file].name.clone())
                .collect(),
            regex_literal_globals,
            initializer_segments,
            initializer_can_raise: false,
            source_bytes: prog.source_bytes,
        };
        if ck.diags.is_empty() {
            let host_diagnostics = host_entries::populate(&mut module, host_exports);
            if !host_diagnostics.is_empty() {
                return Err(host_diagnostics);
            }
        }
        if provisional {
            return Ok((module, opaque_loops));
        }
        capture::check(&mut module)?;
        module.operation_signatures = operation_signatures(&mut module);
        crate::trap_sites::decide_index_checks(&mut module);
        crate::raise_sites::decide_can_raise(&mut module);
        Ok((module, opaque_loops))
    } else {
        Err(ck.diags.take())
    }
}
