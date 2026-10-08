//! Owned async capture layouts follow the independent HIR capture list (§181).
use subscript_compiler::{hir, lir as l, Type};

pub(super) fn compare(hir: &hir::Module, lir: &l::Module, findings: &mut Vec<String>) -> usize {
    let mut environments = Vec::new();
    super::walk_module_expressions(hir, &mut |expr| {
        if let hir::ExprKind::Lambda {
            is_async: true,
            captures,
            ..
        } = &expr.kind
        {
            if !captures.is_empty() {
                environments.push(expr);
            }
        }
    });
    for expr in &environments {
        let hir::ExprKind::Lambda { captures, .. } = &expr.kind else {
            continue;
        };
        let actual = lir
            .functions
            .iter()
            .find(|function| function.kind == l::FunctionKind::Lambda && function.pos == expr.pos)
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
                "{}: async arrow drops its owned capture class layout or frame root",
                expr.pos
            ));
        }
    }
    environments.len()
}
