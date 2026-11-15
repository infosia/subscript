//! Owned capture layouts follow the independent HIR capture list (§181, §186 rule 4).
use subscript_compiler::{hir, lir as l, Type};

pub(super) fn compare(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) -> usize {
    compare_with_key(hir, lir, findings, true)
}

pub(crate) fn compare_with_key(
    hir: &hir::Module,
    lir: &l::Module,
    findings: &mut Vec<String>,
    instance_key: bool,
) -> usize {
    let mut environments = Vec::new();
    let mut identities = std::collections::HashSet::new();
    super::walk_module_expressions(hir, &mut |expr| {
        if let hir::ExprKind::Lambda {
            id,
            group,
            owns_environment: true,
            captures,
            ..
        } = &expr.kind
        {
            if !captures.is_empty() {
                // §293 rule 4: HIR group identities determine the number of owned environments.
                identities.insert((owner_instance(hir, expr), group.unwrap_or(*id)));
                environments.push((expr, owner_instance(hir, expr)));
            }
        }
    });
    let mut matched = std::collections::HashSet::new();
    for (expr, owner) in &environments {
        let hir::ExprKind::Lambda { captures, .. } = &expr.kind else {
            continue;
        };
        // §305 rule 3: a spread map and its adapter share a source position.
        // Match the capture layout before checking the parameter, so a
        // different owned lambda cannot hide a missing parameter. The
        // environment value remains identifiable even if its parameter is lost.
        let function = lir.functions.iter().find(|function| {
            function.kind == l::FunctionKind::Lambda
                && function.pos == expr.pos
                && !matched.contains(&function.id)
                && function.values.first().is_some_and(|value| {
                    let l::ValueType::Data(Type::Class(class)) = value.ty else {
                        return false;
                    };
                    class.0 >= hir.classes.len()
                        && lir.classes.get(class.0).is_some_and(|class| {
                            !class.is_value
                                && class.fields.len() == captures.len()
                                && class.fields.iter().zip(captures).all(|(field, capture)| {
                                    field.ty == capture.ty && field.source_name == capture.name
                                })
                        })
                })
                && (!instance_key || belongs_to(lir, *owner, function.id))
        });
        if let Some(function) = function {
            matched.insert(function.id);
        }
        let actual = function
            .and_then(|function| {
                function
                    .parameters
                    .first()
                    .filter(|parameter| parameter.kind == l::ParameterKind::OwnedEnvironment)
                    .and_then(|parameter| function.values.get(parameter.value.0 as usize))
            })
            .and_then(|value| match value.ty {
                l::ValueType::Data(Type::Class(class)) if class.0 >= hir.classes.len() => {
                    lir.classes.get(class.0)
                }
                _ => None,
            });
        if !actual.is_some_and(|class| {
            !class.is_value
                && class.fields.len() == captures.len()
                && class.fields.iter().zip(captures).all(|(field, capture)| {
                    field.ty == capture.ty && field.source_name == capture.name
                })
        }) {
            findings.push(format!(
                "{}: an owning lambda drops its capture class layout or environment parameter",
                expr.pos
            ));
        }
    }
    identities.len()
}

// Declaration order independently identifies each instantiated function in both forms.
fn owner_instance(hir: &hir::Module, target: &hir::Expr) -> Option<l::FunctionId> {
    let functions = hir
        .classes
        .iter()
        .flat_map(|class| class.ctor.iter().chain(class.methods.iter()))
        .chain(hir.functions.iter());
    for (index, function) in functions.enumerate() {
        let mut found = false;
        super::walk_statement_expression_roots(hir, &function.body, &mut |root| {
            super::walk_expr(hir, root, &mut |expr| {
                found |= std::ptr::eq(expr, target);
            });
        });
        if found {
            return Some(l::FunctionId(index as u32));
        }
    }
    None
}

fn belongs_to(lir: &l::Module, owner: Option<l::FunctionId>, target: l::FunctionId) -> bool {
    let Some(owner) = owner.or(lir.initializer) else {
        return false;
    };
    let mut pending = vec![owner];
    while let Some(id) = pending.pop() {
        for block in &lir.functions[id.0 as usize].blocks {
            for instruction in &block.instructions {
                if let l::InstructionKind::MakeClosure(child) = instruction.kind {
                    if child == target {
                        return true;
                    }
                    pending.push(child);
                }
            }
        }
    }
    false
}

#[test]
fn a_same_position_owned_lambda_cannot_hide_a_missing_environment_parameter() {
    let source = r#"
function make(): (() => i32)[] {
    const saved: i32 = 4;
    return [(): i32 => saved, (): i32 => saved];
}
export function main(): void { const fs = make(); print(`${fs[0]()}`); }
"#;
    let mut hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
        "same-position.ts",
        source,
    )])
    .expect("owning lambdas check");
    let make = hir.functions.iter_mut().find(|f| f.name == "make").unwrap();
    let hir::Stmt::Return {
        value: Some(value), ..
    } = make.body.last_mut().unwrap()
    else {
        panic!("return array");
    };
    let hir::ExprKind::ArrayLit(elements) = &mut value.kind else {
        panic!("array of lambdas");
    };
    elements[1].pos = elements[0].pos.clone();
    let lir = subscript_codegen::lir::lower_module(&hir).expect("owning lambdas lower");
    let mut findings = Vec::new();
    compare(&hir, &lir, &mut findings);
    assert!(findings.is_empty(), "clean control: {findings:?}");

    let lambdas: Vec<_> = lir
        .functions
        .iter()
        .filter(|f| f.kind == l::FunctionKind::Lambda)
        .collect();
    assert_eq!(lambdas.len(), 2);
    assert_eq!(lambdas[0].pos, lambdas[1].pos);
    for lambda in &lambdas {
        assert_eq!(
            lambda.parameters[0].kind,
            l::ParameterKind::OwnedEnvironment
        );
    }
    for lambda in lambdas {
        let mut broken = lir.clone();
        broken.functions[lambda.id.0 as usize].parameters.remove(0);
        let mut findings = Vec::new();
        compare(&hir, &broken, &mut findings);
        assert!(
            findings.iter().any(|finding| finding.contains(
                "an owning lambda drops its capture class layout or environment parameter"
            )),
            "{findings:?}"
        );
    }
}
