//! Gates for compiler.md §83: operation signatures are total over complete HIR.

#[path = "corpus/mod.rs"]
mod corpus;

mod lifetime_sites {
    //! Independent runtime signature checks for compiler.md §120.

    use subscript_compiler::{hir::*, types::HandleClass, Pos, Type};

    fn template() -> Expr {
        static TEMPLATE: std::sync::OnceLock<Expr> = std::sync::OnceLock::new();
        TEMPLATE
            .get_or_init(|| super::synthesized_max_call(1))
            .clone()
    }

    fn call(signature: &OperationSignature) -> Expr {
        let mut args = signature
            .parameter_types
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                let mut expression = template();
                expression.kind = ExprKind::Local(format!("p{index}"), ty.clone(), true);
                expression.ty = ty.clone();
                expression.pos = Pos::new("signature.ts", 1, index as u32 + 2);
                expression
            })
            .collect::<Vec<_>>();
        let callee = match &signature.target {
            OperationSignatureTarget::Ambient(f) => Callee::Ambient(*f),
            OperationSignatureTarget::ContextBytes(f, ty) => Callee::ContextBytes {
                function: *f,
                ty: ty.clone(),
            },
            OperationSignatureTarget::Math(f) => Callee::Math(*f),
            OperationSignatureTarget::Num(f) => Callee::Num(*f),
            OperationSignatureTarget::Date(f) => Callee::Date(*f),
            OperationSignatureTarget::Json(f) => Callee::Json(*f),
            OperationSignatureTarget::Text(f) => Callee::Text(*f),
            OperationSignatureTarget::Str(f) => Callee::Str(*f),
            OperationSignatureTarget::Regex(f) => Callee::Regex(*f),
            OperationSignatureTarget::Arr(f) => Callee::Arr(*f),
            OperationSignatureTarget::Map(f) => Callee::Map(*f),
            OperationSignatureTarget::Set(f) => Callee::Set(*f),
            OperationSignatureTarget::Worker(f) => Callee::Worker(*f),
            OperationSignatureTarget::BuiltinMethod(method) => {
                let name = match method {
                    BuiltinMethod::ArrayPush => "push",
                    BuiltinMethod::ArrayPop => "pop",
                    BuiltinMethod::ArrayClear => "[[array_clear]]",
                    BuiltinMethod::StringSlice => "slice",
                    BuiltinMethod::GeneratorNext => "next",
                };
                Callee::Method {
                    recv: Box::new(args.remove(0)),
                    name: Symbol::from_full_text(name),
                }
            }
        };
        let mut expression = template();
        expression.kind = ExprKind::Call { callee, args };
        expression.ty = signature.return_type.clone().unwrap_or(Type::Void);
        expression.pos = Pos::new("signature.ts", 1, 1);
        expression
    }

    fn parameters(signature: &str) -> Vec<&str> {
        let Some(start) = signature.find('(') else {
            return Vec::new();
        };
        let mut depth = 0;
        let mut begin = start + 1;
        let mut result = Vec::new();
        for (index, ch) in signature
            .char_indices()
            .skip_while(|(index, _)| *index <= start)
        {
            match ch {
                '(' | '[' | '<' => depth += 1,
                ')' if depth == 0 => {
                    if index > begin {
                        result.push(&signature[begin..index]);
                    }
                    break;
                }
                ')' | ']' => depth -= 1,
                '>' if depth > 0 && signature.as_bytes()[index - 1] != b'=' => depth -= 1,
                ',' if depth == 0 => {
                    result.push(&signature[begin..index]);
                    begin = index + 1;
                }
                _ => {}
            }
        }
        result
    }

    fn stored_indices(target: &OperationSignatureTarget) -> Vec<usize> {
        let (group, name, prefix) = match target {
            OperationSignatureTarget::Arr(operation) => ("T[]", operation.name(), 1),
            OperationSignatureTarget::Map(operation) => match operation {
                MapFn::New => ("Map constructor", "new Map", 0),
                MapFn::GroupBy => ("Map", operation.name(), 0),
                _ => ("Map<K, V>", operation.name(), 1),
            },
            OperationSignatureTarget::Set(operation) => match operation {
                SetFn::New => ("Set constructor", "new Set", 0),
                _ => ("Set<K>", operation.name(), 1),
            },
            OperationSignatureTarget::BuiltinMethod(BuiltinMethod::ArrayPush) => ("T[]", "push", 1),
            // These groups copy no handle into element, key, or value storage.
            // Worker.post copies the message payload (compiler.md §120.1 rule 3a).
            OperationSignatureTarget::Ambient(_)
            | OperationSignatureTarget::ContextBytes(..)
            | OperationSignatureTarget::Math(_)
            | OperationSignatureTarget::Num(_)
            | OperationSignatureTarget::Date(_)
            | OperationSignatureTarget::Json(_)
            | OperationSignatureTarget::Text(_)
            | OperationSignatureTarget::Str(_)
            | OperationSignatureTarget::Regex(_)
            | OperationSignatureTarget::Worker(_)
            | OperationSignatureTarget::BuiltinMethod(
                BuiltinMethod::ArrayPop
                | BuiltinMethod::ArrayClear
                | BuiltinMethod::StringSlice
                | BuiltinMethod::GeneratorNext,
            ) => return Vec::new(),
        };
        // The API projection reads api_signature strings, independently of lifetime parameter roles.
        static API: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let api = API.get_or_init(subscript_compiler::api_reference::render_markdown);
        let mut active = false;
        let signature = api
            .lines()
            .find_map(|line| {
                if let Some(heading) = line.strip_prefix("### ") {
                    active = heading == group;
                }
                let row = active.then(|| line.strip_prefix("| `")).flatten()?;
                let (signature, summary) = row.split_once("` |")?;
                let suffix = signature.strip_prefix(name)?;
                (suffix.starts_with('(') || suffix.starts_with('<') || suffix.starts_with(':'))
                    .then_some((signature, summary.trim()))
            })
            .unwrap_or_else(|| panic!("missing API signature for {target:?}"));
        // Generic parameters are stored only when the API states a storage effect.
        let (signature, summary) = signature;
        if !["Appends ", "Prepends ", "Stores ", "Adds "]
            .iter()
            .any(|effect| summary.starts_with(effect))
        {
            return Vec::new();
        }
        parameters(signature)
            .iter()
            .enumerate()
            .filter_map(|(index, parameter)| {
                let ty = parameter.split_once(':').expect("named parameter").1.trim();
                matches!(ty, "T" | "K" | "V").then_some(index + prefix)
            })
            .collect()
    }

    fn required_indices(signature: &OperationSignature, module: &Module) -> Vec<usize> {
        let classes = module
            .classes
            .iter()
            .map(HandleClass::from)
            .collect::<Vec<_>>();
        if matches!(
            signature.target,
            OperationSignatureTarget::Ambient(AmbientFn::UnsafeDelete)
        ) {
            return vec![0];
        }
        let stored = stored_indices(&signature.target);
        signature
            .parameter_types
            .iter()
            .enumerate()
            .filter_map(|(index, ty)| {
                (!stored.contains(&index)
                    && ty
                        .handle_kind(&classes)
                        .is_some_and(|kind| kind.needs_lifetime_trap()))
                .then_some(index)
            })
            .collect()
    }

    fn violations(
        signature: &OperationSignature,
        expression: &Expr,
        module: &Module,
    ) -> Vec<String> {
        let prefix = usize::from(matches!(
            expression.kind,
            ExprKind::Call {
                callee: Callee::Method { .. },
                ..
            }
        ));
        let actual = expression
            .trap_sites(module)
            .into_iter()
            .filter_map(|site| match site {
                TrapSite::DevOnlyLifetime { operand, pos }
                | TrapSite::DevOnlyRelease { operand, pos } => {
                    Some((operand.evaluated_index(prefix), pos))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let expected = required_indices(signature, module)
            .into_iter()
            .map(|index| {
                let position = if matches!(
                    signature.target,
                    OperationSignatureTarget::Ambient(AmbientFn::UnsafeDelete)
                ) {
                    expression.pos.clone()
                } else {
                    Pos::new("signature.ts", 1, index as u32 + 2)
                };
                (index, position)
            })
            .collect::<Vec<_>>();
        if actual == expected {
            Vec::new()
        } else {
            vec![format!(
                "{:?}: expected {expected:?}, got {actual:?}",
                signature.target
            )]
        }
    }

    pub(super) fn check(module: &Module) -> Vec<String> {
        module
            .operation_signatures
            .iter()
            .flat_map(|signature| violations(signature, &call(signature), module))
            .collect()
    }

    #[test]
    fn missing_argument_site_is_a_form_violation() {
        let module = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
            "empty.ts",
            "export function main(): void {}",
        )])
        .unwrap();
        let signature = OperationSignature {
            target: OperationSignatureTarget::Set(SetFn::Union),
            parameter_types: vec![Type::Set(Box::new(Type::I32)); 2],
            return_type: Some(Type::Set(Box::new(Type::I32))),
        };
        let valid = call(&signature);
        assert!(violations(&signature, &valid, &module).is_empty());
        let mut invalid = valid;
        let ExprKind::Call { args, .. } = &mut invalid.kind else {
            panic!("call form")
        };
        args[1].ty = Type::I32;
        assert_eq!(violations(&signature, &invalid, &module).len(), 1);
    }

    #[test]
    fn builtin_method_table_has_total_lifetime_sites() {
        let module = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
            "empty.ts",
            "export function main(): void {}",
        )])
        .unwrap();
        for (method, parameters, result) in [
            (
                BuiltinMethod::ArrayPush,
                vec![
                    Type::Array(Box::new(Type::Set(Box::new(Type::I32)))),
                    Type::Set(Box::new(Type::I32)),
                ],
                Type::I32,
            ),
            (
                BuiltinMethod::ArrayPop,
                vec![Type::Array(Box::new(Type::I32))],
                Type::I32,
            ),
            (
                BuiltinMethod::ArrayClear,
                vec![Type::Array(Box::new(Type::I32))],
                Type::Void,
            ),
            (
                BuiltinMethod::StringSlice,
                vec![Type::Str, Type::I32, Type::I32],
                Type::Str,
            ),
            (
                BuiltinMethod::GeneratorNext,
                vec![Type::Generator(Box::new(Type::I32))],
                Type::IterResult(Box::new(Type::I32)),
            ),
        ] {
            let signature = OperationSignature {
                target: OperationSignatureTarget::BuiltinMethod(method),
                parameter_types: parameters,
                return_type: Some(result),
            };
            assert!(
                violations(&signature, &call(&signature), &module).is_empty(),
                "{method:?}"
            );
        }
        assert_eq!(LifetimeOperand::Receiver.evaluated_index(1), 0);
        assert_eq!(LifetimeOperand::Argument(2).evaluated_index(1), 3);
    }

    #[test]
    fn synthesized_helper_arguments_follow_the_form_fact() {
        let module = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
        "helper.ts",
        "class Item { value: i32 = 1; } function helper(a: Item, b: Item): void {} export function main(): void { JSON.stringify(new Item()); }",
    )]).unwrap();
        let signature = OperationSignature {
            target: OperationSignatureTarget::Set(SetFn::Union),
            parameter_types: vec![Type::Class(subscript_compiler::ClassId(0)); 2],
            return_type: Some(Type::Void),
        };
        let mut expression = call(&signature);
        let helper = module
            .functions
            .iter()
            .position(|function| function.name.ends_with("helper"))
            .unwrap();
        let ExprKind::Call { callee, .. } = &mut expression.kind else {
            panic!("call")
        };
        *callee = Callee::Func(module.functions[helper].symbol.clone());
        assert!(!expression
            .trap_sites(&module)
            .iter()
            .any(|site| matches!(site, TrapSite::DevOnlyLifetime { .. })));
        let helper = module
            .functions
            .iter()
            .find(|function| function.synthesized_helper)
            .unwrap();
        let ExprKind::Call { callee, .. } = &mut expression.kind else {
            panic!("call")
        };
        *callee = Callee::Func(helper.symbol.clone());
        assert!(violations(&signature, &expression, &module).is_empty());
    }

    #[test]
    fn release_site_is_distinct_from_a_read_site() {
        let module = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
            "empty.ts",
            "export function main(): void {}",
        )])
        .unwrap();
        let expression = call(&OperationSignature {
            target: OperationSignatureTarget::Ambient(AmbientFn::UnsafeDelete),
            parameter_types: vec![Type::Set(Box::new(Type::I32))],
            return_type: Some(Type::Void),
        });
        let sites = expression.trap_sites(&module);
        assert_eq!(sites.len(), 1);
        assert!(
            matches!(&sites[0], TrapSite::DevOnlyRelease { operand: LifetimeOperand::Argument(0), pos } if *pos == expression.pos)
        );
        let read = call(&OperationSignature {
            target: OperationSignatureTarget::Set(SetFn::Size),
            parameter_types: vec![Type::Set(Box::new(Type::I32))],
            return_type: Some(Type::I32),
        });
        assert!(matches!(
            &read.trap_sites(&module)[0],
            TrapSite::DevOnlyLifetime {
                operand: LifetimeOperand::Argument(0),
                ..
            }
        ));
    }
}

use corpus::interop;

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
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let mut files = interop::mirrors_for(&source, SourceFile::ambient);
    files.push(SourceFile::new(name, source));
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
        lifetime_sites::check(module)
            .into_iter()
            .map(|error| format!("{label}: {error}")),
    );
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
            parameter_types: vec![Type::Generator(Box::new(Type::Class(ClassId(1))))],
            return_type: Some(Type::IterResult(Box::new(Type::Class(ClassId(1))))),
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
    for directory in corpus::directories(&accept) {
        let label = directory.display().to_string();
        let mut module = checked_module(&label, corpus::directory_sources(&directory));
        append_missing(&label, &mut module, &mut errors);
    }

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
        if source.contains("engineWorld")
            || source.contains("engineFrame")
            || source.contains("engineRequest")
        {
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
            "function donor(): f64 { return Math.max(2.0, 3.0); }",
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
