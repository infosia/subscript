//! Gates for compiler.md §83: operation signatures are derived from complete HIR.

use std::fs;
use std::path::{Path, PathBuf};

use subscript_compiler::{check_program, hir, Pos, SourceFile, Type};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn read_source(path: &Path, name: impl Into<String>) -> SourceFile {
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    SourceFile::new(name, source)
}

fn read_ambient(path: &Path, name: &str) -> SourceFile {
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    SourceFile::ambient(name, source)
}

fn checked_module(label: &str, files: Vec<SourceFile>) -> hir::Module {
    check_program(&files).unwrap_or_else(|diagnostics| {
        panic!(
            "{label} rejected with {} diagnostic(s):\n{}",
            diagnostics.len(),
            diagnostics
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })
}

fn corpus_sources(name: &str, path: &Path) -> Vec<SourceFile> {
    let root = repository_root();
    let corpus = root.join("corpus");
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    const INTEROP_TOKENS: &[&str] = &[
        "subDevice",
        "subChainPayloadValue",
        "subSlice",
        "SubDrawList",
        "subDrawListTotal",
        "SUB_ACCESS",
        "subAccessMatches",
        "subBulk",
        "subBoundaryString",
        "subProbeTexture",
        "subProbeComputePipeline",
        "subProbeRenderPipeline",
        "subProbeProgrammableStage",
        "subProbeFullRenderPipeline",
        "subProbeBreadthRenderPipeline",
        "subProbeWideRenderPipeline",
        "subProbeQueueSubmit",
        "subProbeSetBindGroup",
        "SUB_STAGE",
        "subStageMatches",
        "subFutureMake",
        "subStatsMake",
        "SubQueryStatus",
        "subByValue",
        "subHostOwnedState",
        "subWireMode",
        "subBindTone",
    ];
    let uses_external = source.contains("subExternalDevice");
    let uses_wire_enum = source.contains("subWireMode")
        || source.contains("SubWireMode")
        || source.contains("subBindTone")
        || source.contains("SubBindTone");
    let uses_interop = uses_external || INTEROP_TOKENS.iter().any(|token| source.contains(token));
    let mut files = vec![SourceFile::new(name, source)];
    if uses_external {
        files.insert(
            0,
            read_ambient(
                &corpus.join("interop/external-device.generated.d.ts"),
                "external-device.generated.d.ts",
            ),
        );
    }
    if uses_interop {
        files.insert(
            0,
            read_ambient(
                &corpus.join("interop/interop.generated.d.ts"),
                "interop.generated.d.ts",
            ),
        );
    }
    if uses_wire_enum {
        files.insert(
            0,
            read_ambient(
                &corpus.join("interop/wire-enum.generated.d.ts"),
                "wire-enum.generated.d.ts",
            ),
        );
        files.insert(
            0,
            read_ambient(
                &corpus.join("interop/wire-enum-aliases.d.ts"),
                "wire-enum-aliases.d.ts",
            ),
        );
    }
    files
}

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

fn collect_call_signatures(module: &hir::Module) -> Vec<(Pos, hir::OperationSignature)> {
    fn visit_child(child: hir::HirChild<'_>, found: &mut Vec<(Pos, hir::OperationSignature)>) {
        match child {
            hir::HirChild::Expr(expression) => visit_expr(expression, found),
            hir::HirChild::Stmt(statement) => visit_stmt(statement, found),
        }
    }

    fn visit_stmt(statement: &hir::Stmt, found: &mut Vec<(Pos, hir::OperationSignature)>) {
        for child in statement.children() {
            visit_child(child, found);
        }
    }

    fn visit_expr(expression: &hir::Expr, found: &mut Vec<(Pos, hir::OperationSignature)>) {
        if let hir::ExprKind::Call { callee, args } = &expression.kind {
            if let Some((target, receiver)) = hir::operation_signature_target(callee) {
                let mut parameter_types = receiver
                    .into_iter()
                    .cloned()
                    .chain(args.iter().map(|argument| argument.ty.clone()))
                    .collect::<Vec<_>>();
                normalize_operation_parameter_types(&target, &mut parameter_types);
                found.push((
                    expression.pos.clone(),
                    hir::OperationSignature {
                        target,
                        parameter_types,
                        return_type: (expression.ty != Type::Void).then(|| expression.ty.clone()),
                    },
                ));
            }
        }
        for child in expression.children() {
            visit_child(child, found);
        }
    }

    fn visit_body(body: &[hir::Stmt], found: &mut Vec<(Pos, hir::OperationSignature)>) {
        for statement in body {
            visit_stmt(statement, found);
        }
    }

    let mut found = Vec::new();
    for class in &module.classes {
        for field in &class.fields {
            if let Some(initializer) = &field.init {
                visit_expr(initializer, &mut found);
            }
        }
        if let Some(constructor) = &class.ctor {
            visit_body(&constructor.body, &mut found);
        }
        for method in &class.methods {
            visit_body(&method.body, &mut found);
        }
    }
    for global in &module.globals {
        visit_expr(&global.init, &mut found);
    }
    for function in &module.functions {
        visit_body(&function.body, &mut found);
    }
    visit_body(&module.top_level, &mut found);
    found
}

fn independent_signature_set(module: &hir::Module) -> Vec<hir::OperationSignature> {
    let mut signatures = Vec::new();
    for (_, signature) in collect_call_signatures(module) {
        if !signatures.contains(&signature) {
            signatures.push(signature);
        }
    }
    signatures
}

fn missing_signatures(label: &str, module: &hir::Module, errors: &mut Vec<String>) {
    for (position, signature) in collect_call_signatures(module) {
        if !module.operation_signatures.contains(&signature) {
            errors.push(format!("{label}: {position}: missing {signature:?}"));
        }
    }
}

#[test]
fn a180_table_equals_an_independent_hir_walk() {
    let path = repository_root().join("corpus/accept/a180-for-of-generator-only.ts");
    let module = checked_module(
        "a180-for-of-generator-only",
        vec![read_source(&path, "a180-for-of-generator-only.ts")],
    );
    let independent = independent_signature_set(&module);
    let only_in_table = module
        .operation_signatures
        .iter()
        .filter(|signature| !independent.contains(signature))
        .collect::<Vec<_>>();
    let only_in_walk = independent
        .iter()
        .filter(|signature| !module.operation_signatures.contains(signature))
        .collect::<Vec<_>>();
    assert_eq!(
        module.operation_signatures.len(),
        independent.len(),
        "signature counts differ: table={:?}, walk={independent:?}",
        module.operation_signatures
    );
    assert!(
        only_in_table.is_empty() && only_in_walk.is_empty(),
        "signature sets differ: only in table={only_in_table:?}; only in walk={only_in_walk:?}"
    );
    assert!(module
        .operation_signatures
        .contains(&hir::OperationSignature {
            target: hir::OperationSignatureTarget::BuiltinMethod(hir::BuiltinMethod::GeneratorNext),
            parameter_types: vec![Type::Generator(Box::new(Type::I32))],
            return_type: Some(Type::IterResult(Box::new(Type::I32))),
        }));
}

#[test]
fn every_executable_source_has_total_operation_signatures() {
    let root = repository_root();
    let mut errors = Vec::new();

    let accept = root.join("corpus/accept");
    let mut accept_entries = fs::read_dir(&accept)
        .expect("read corpus/accept")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".ts"))
        .collect::<Vec<_>>();
    accept_entries.sort();
    for name in accept_entries {
        let module = checked_module(&name, corpus_sources(&name, &accept.join(&name)));
        missing_signatures(&format!("corpus/accept/{name}"), &module, &mut errors);
    }
    let modules = accept.join("a19-modules");
    let module = checked_module(
        "corpus/accept/a19-modules",
        vec![
            read_source(&modules.join("main.ts"), "main.ts"),
            read_source(&modules.join("math.ts"), "math.ts"),
        ],
    );
    missing_signatures("corpus/accept/a19-modules", &module, &mut errors);

    let warn = root.join("corpus/warn");
    let mut warn_entries = fs::read_dir(&warn)
        .expect("read corpus/warn")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".ts"))
        .collect::<Vec<_>>();
    warn_entries.sort();
    for name in warn_entries {
        let module = checked_module(&name, corpus_sources(&name, &warn.join(&name)));
        missing_signatures(&format!("corpus/warn/{name}"), &module, &mut errors);
    }

    let examples = root.join("examples");
    let engine_mirror = fs::read_to_string(examples.join("engine/engine.generated.d.ts"))
        .expect("read engine mirror");
    let mut example_entries = fs::read_dir(&examples)
        .expect("read examples")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with('e') && name.ends_with(".ts"))
        .collect::<Vec<_>>();
    example_entries.sort();
    for name in example_entries {
        let path = examples.join(&name);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let mut files = Vec::new();
        if source.contains("engineWorld") || source.contains("engineFrame") {
            files.push(SourceFile::ambient(
                "engine.generated.d.ts",
                engine_mirror.clone(),
            ));
        }
        files.push(SourceFile::new(name.clone(), source));
        let module = checked_module(&name, files);
        missing_signatures(&format!("examples/{name}"), &module, &mut errors);
    }
    for relative in ["hot-reload/demo.ts", "rust-host/logic.ts"] {
        let path = examples.join(relative);
        let module = checked_module(relative, vec![read_source(&path, relative)]);
        missing_signatures(&format!("examples/{relative}"), &module, &mut errors);
    }

    assert!(
        errors.is_empty(),
        "operation-signature violation(s):\n{}",
        errors.join("\n")
    );
}
