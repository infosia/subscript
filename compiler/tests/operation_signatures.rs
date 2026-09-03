//! Gates for compiler.md §83: operation signatures are total over complete HIR.

#[path = "corpus/mod.rs"]
mod corpus;

use std::fs;
use std::path::{Path, PathBuf};

use subscript_compiler::{check_program, hir, ClassId, Pos, SourceFile, Type};

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
    let corpus_dir = repository_root().join("corpus");
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let uses_external = source.contains("subExternalDevice");
    let uses_wire_enum = source.contains("subWireMode")
        || source.contains("SubWireMode")
        || source.contains("subBindTone")
        || source.contains("SubBindTone");
    let uses_interop = uses_external || corpus::references_interop(&source);
    let mut files = vec![SourceFile::new(name, source)];
    if uses_external {
        files.insert(
            0,
            read_ambient(
                &corpus_dir.join("interop/external-device.generated.d.ts"),
                "external-device.generated.d.ts",
            ),
        );
    }
    if uses_interop {
        files.insert(
            0,
            read_ambient(
                &corpus_dir.join("interop/interop.generated.d.ts"),
                "interop.generated.d.ts",
            ),
        );
    }
    if uses_wire_enum {
        files.insert(
            0,
            read_ambient(
                &corpus_dir.join("interop/wire-enum.generated.d.ts"),
                "wire-enum.generated.d.ts",
            ),
        );
        files.insert(
            0,
            read_ambient(
                &corpus_dir.join("interop/wire-enum-aliases.d.ts"),
                "wire-enum-aliases.d.ts",
            ),
        );
    }
    files
}

fn operation_calls(module: &hir::Module) -> Vec<(Pos, hir::OperationSignature)> {
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
                found.push((
                    expression.pos.clone(),
                    hir::OperationSignature {
                        target,
                        parameter_types: receiver
                            .into_iter()
                            .cloned()
                            .chain(args.iter().map(|argument| argument.ty.clone()))
                            .collect(),
                        return_type: (expression.ty != Type::Void).then(|| expression.ty.clone()),
                    },
                ));
            }
        }
        for child in expression.children() {
            visit_child(child, found);
        }
    }

    let mut found = Vec::new();
    for owner in module.expression_owners() {
        match owner {
            hir::ExpressionOwner::Expr(expression) => visit_expr(expression, &mut found),
            hir::ExpressionOwner::Body { statements, .. } => {
                for statement in statements {
                    visit_stmt(statement, &mut found);
                }
            }
        }
    }
    found
}

fn missing_operation_calls(module: &hir::Module) -> Vec<(Pos, hir::OperationSignature)> {
    let table = module.operation_signatures.clone();
    operation_calls(module)
        .into_iter()
        .filter(|(_, signature)| !table.contains(signature))
        .collect()
}

fn append_missing(label: &str, module: &mut hir::Module, errors: &mut Vec<String>) {
    errors.extend(
        missing_operation_calls(module)
            .into_iter()
            .map(|(position, signature)| format!("{label}: {position}: missing {signature:?}")),
    );
}

#[test]
fn a180_table_equals_the_hand_written_program_table() {
    let path = repository_root().join("corpus/accept/a180-for-of-generator-only.ts");
    let module = checked_module(
        "a180-for-of-generator-only",
        vec![read_source(&path, "a180-for-of-generator-only.ts")],
    );
    let expected = vec![
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::BuiltinMethod(hir::BuiltinMethod::GeneratorNext),
            parameter_types: vec![Type::Generator(Box::new(Type::I32))],
            return_type: Some(Type::IterResult(Box::new(Type::I32))),
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Ambient(hir::AmbientFn::Print),
            parameter_types: vec![Type::Str],
            return_type: None,
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::BuiltinMethod(hir::BuiltinMethod::GeneratorNext),
            parameter_types: vec![Type::Generator(Box::new(Type::Class(ClassId(0))))],
            return_type: Some(Type::IterResult(Box::new(Type::Class(ClassId(0))))),
        },
    ];
    assert_eq!(module.operation_signatures, expected);
}

#[test]
fn a181_table_equals_the_hand_written_program_table() {
    let path = repository_root().join("corpus/accept/a181-operation-in-every-owner.ts");
    let module = checked_module(
        "a181-operation-in-every-owner",
        vec![read_source(&path, "a181-operation-in-every-owner.ts")],
    );
    let expected = vec![
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Math(hir::MathFn::Floor),
            parameter_types: vec![Type::F64],
            return_type: Some(Type::F64),
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Math(hir::MathFn::Abs),
            parameter_types: vec![Type::F64],
            return_type: Some(Type::F64),
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Math(hir::MathFn::Min),
            parameter_types: vec![Type::F64, Type::F64],
            return_type: Some(Type::F64),
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Math(hir::MathFn::Trunc),
            parameter_types: vec![Type::F64],
            return_type: Some(Type::F64),
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Math(hir::MathFn::Pow),
            parameter_types: vec![Type::F64, Type::F64],
            return_type: Some(Type::F64),
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Math(hir::MathFn::Ceil),
            parameter_types: vec![Type::F64],
            return_type: Some(Type::F64),
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Math(hir::MathFn::Max),
            parameter_types: vec![Type::F64, Type::F64],
            return_type: Some(Type::F64),
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Math(hir::MathFn::Sign),
            parameter_types: vec![Type::F64],
            return_type: Some(Type::F64),
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Math(hir::MathFn::Sqrt),
            parameter_types: vec![Type::F64],
            return_type: Some(Type::F64),
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Ambient(hir::AmbientFn::Print),
            parameter_types: vec![Type::Str],
            return_type: None,
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Math(hir::MathFn::Round),
            parameter_types: vec![Type::F64],
            return_type: Some(Type::F64),
        },
        hir::OperationSignature {
            target: hir::OperationSignatureTarget::Math(hir::MathFn::Cos),
            parameter_types: vec![Type::F64],
            return_type: Some(Type::F64),
        },
    ];
    assert_eq!(module.operation_signatures, expected);
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
        let mut module = checked_module(&name, corpus_sources(&name, &accept.join(&name)));
        append_missing(&format!("corpus/accept/{name}"), &mut module, &mut errors);
    }
    let modules = accept.join("a19-modules");
    let mut module = checked_module(
        "corpus/accept/a19-modules",
        vec![
            read_source(&modules.join("main.ts"), "main.ts"),
            read_source(&modules.join("math.ts"), "math.ts"),
        ],
    );
    append_missing("corpus/accept/a19-modules", &mut module, &mut errors);

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
        let mut module = checked_module(&name, corpus_sources(&name, &warn.join(&name)));
        append_missing(&format!("corpus/warn/{name}"), &mut module, &mut errors);
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
        let mut module = checked_module(&name, files);
        append_missing(&format!("examples/{name}"), &mut module, &mut errors);
    }

    assert!(
        errors.is_empty(),
        "operation-signature violation(s):\n{}",
        errors.join("\n")
    );
}

fn synthesized_max_call(line: u32) -> hir::Expr {
    let donor = checked_module(
        "positive-control donor",
        vec![SourceFile::new(
            "donor.ts",
            "export function donor(): f64 { return Math.max(2.0, 3.0); }",
        )],
    );
    let function = donor
        .functions
        .iter()
        .find(|function| function.name == "donor")
        .expect("donor function");
    let hir::Stmt::Return {
        value: Some(call), ..
    } = &function.body[0]
    else {
        panic!("donor must return one call");
    };
    let mut call = call.clone();
    call.pos = Pos::new("positive-control.ts", line, 1);
    call
}

#[test]
fn total_check_reports_synthesized_calls_in_two_owner_kinds() {
    let source = SourceFile::new(
        "positive-control.ts",
        "class Box { value: f64 = 1.0; }\n\
         function useDefault(value: f64 = 1.0): f64 { return value; }\n\
         export function main(): void { const box: Box = new Box(); print(`${useDefault()}:${box.value}`); }\n",
    );
    let mut module = checked_module("positive control", vec![source]);
    module
        .classes
        .iter_mut()
        .find(|class| class.name == "Box")
        .expect("Box class")
        .fields[0]
        .init = Some(synthesized_max_call(101));
    module
        .functions
        .iter_mut()
        .find(|function| function.name == "useDefault")
        .expect("useDefault function")
        .params[0]
        .default = Some(synthesized_max_call(102));

    let missing = missing_operation_calls(&mut module);
    assert_eq!(
        missing
            .iter()
            .map(|(position, _)| position.clone())
            .collect::<Vec<_>>(),
        vec![
            Pos::new("positive-control.ts", 101, 1),
            Pos::new("positive-control.ts", 102, 1),
        ]
    );
    assert!(missing.iter().all(|(_, signature)| {
        signature
            == &hir::OperationSignature {
                target: hir::OperationSignatureTarget::Math(hir::MathFn::Max),
                parameter_types: vec![Type::F64, Type::F64],
                return_type: Some(Type::F64),
            }
    }));
}

fn first_expression_position(statements: &[hir::Stmt]) -> Option<Pos> {
    statements.iter().find_map(|statement| {
        statement
            .children()
            .into_iter()
            .find_map(|child| match child {
                hir::HirChild::Expr(expression) => Some(expression.pos.clone()),
                hir::HirChild::Stmt(statement) => {
                    first_expression_position(std::slice::from_ref(statement))
                }
            })
    })
}

#[test]
fn expression_owner_iterators_match_and_reach_eleven_hand_counted_calls() {
    let path = repository_root().join("corpus/accept/a181-operation-in-every-owner.ts");
    let mut module = checked_module(
        "a181-operation-in-every-owner",
        vec![read_source(&path, "a181-operation-in-every-owner.ts")],
    );
    let shared_positions = module
        .expression_owners()
        .map(|owner| match owner {
            hir::ExpressionOwner::Expr(expression) => Some(expression.pos.clone()),
            hir::ExpressionOwner::Body { statements, .. } => first_expression_position(statements),
        })
        .collect::<Vec<_>>();
    let mutable_positions = module
        .expression_owners_mut()
        .map(|owner| match owner {
            hir::ExpressionOwnerMut::Expr(expression) => Some(expression.pos.clone()),
            hir::ExpressionOwnerMut::Body(statements) => first_expression_position(statements),
        })
        .collect::<Vec<_>>();
    assert_eq!(shared_positions, mutable_positions);

    // The walk must count Math call nodes across owners, not iterator arms.
    let calls = operation_calls(&module)
        .into_iter()
        .filter(|(_, signature)| matches!(signature.target, hir::OperationSignatureTarget::Math(_)))
        .count();
    assert_eq!(calls, 11, "each owner kind must contain one Math call");
}
