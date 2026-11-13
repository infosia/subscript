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
    super::walk_module_expressions(hir, &mut |expr| {
        if let hir::ExprKind::Lambda {
            owns_environment: true,
            captures,
            ..
        } = &expr.kind
        {
            if !captures.is_empty() {
                environments.push((expr, owner_instance(hir, expr)));
            }
        }
    });
    for (expr, owner) in &environments {
        let hir::ExprKind::Lambda { captures, .. } = &expr.kind else {
            continue;
        };
        let actual = lir
            .functions
            .iter()
            .find(|function| {
                function.kind == l::FunctionKind::Lambda
                    && function.pos == expr.pos
                    && (!instance_key || belongs_to(lir, *owner, function.id))
            })
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
    environments.len()
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
