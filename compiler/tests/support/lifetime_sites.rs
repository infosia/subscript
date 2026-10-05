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
            BuiltinMethod::ArrayPop | BuiltinMethod::StringSlice | BuiltinMethod::GeneratorNext,
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

fn violations(signature: &OperationSignature, expression: &Expr, module: &Module) -> Vec<String> {
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
