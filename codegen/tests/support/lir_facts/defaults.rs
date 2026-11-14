//! Independent synthesized default facts from checked HIR (§291 rules 1 and 3).
use super::*;

fn lambda(hir: &hir::Module, id: hir::LambdaId) -> Option<&hir::Expr> {
    let mut found = None;
    walk_module_expressions(hir, &mut |expr| {
        if matches!(expr.kind, hir::ExprKind::Lambda { id: found_id, .. } if found_id == id) {
            found = Some(expr);
        }
    });
    found
}

/// An async default boundary catches defaults at the caller.
pub(super) fn async_inputs<'a>(hir: &hir::Module, expr: &'a hir::Expr) -> Option<&'a [hir::Expr]> {
    let (params, args) = match &expr.kind {
        hir::ExprKind::AsyncCall { callee, args }
        | hir::ExprKind::AsyncHandleCreate { callee, args, .. } => (
            declared_async_callee_parameters(hir, callee, &expr.pos).ok()?,
            args,
        ),
        hir::ExprKind::Call { callee, args } => {
            let defaults = callee.lambda_defaults()?;
            if !matches!(
                lambda(hir, defaults.id)?.kind,
                hir::ExprKind::Lambda { is_async: true, .. }
            ) {
                return None;
            }
            (defaults.params.as_slice(), args)
        }
        _ => return None,
    };
    params
        .iter()
        .skip(args.len())
        .any(|p| p.default_can_raise)
        .then_some(args)
}

/// A failed-handle helper exists once per result type that needs a boundary.
pub(super) fn failed_results(hir: &hir::Module) -> Vec<Type> {
    let mut results = Vec::new();
    walk_placed_module_expressions(hir, &mut |expr| {
        if async_inputs(hir, expr).is_some() {
            let result = match &expr.kind {
                hir::ExprKind::AsyncCall { .. } => &expr.ty,
                _ => match &expr.ty {
                    Type::AsyncHandle(result) => &**result,
                    _ => return,
                },
            };
            if !results.contains(result) {
                results.push(result.clone());
            }
        }
    });
    results
}

/// A captured default has its own lambda-ABI helper, once per owner and parameter.
pub(super) fn captured_helpers(hir: &hir::Module) -> Vec<&Pos> {
    let mut helpers = BTreeMap::new();
    walk_placed_module_expressions(hir, &mut |expr| {
        let hir::ExprKind::Call { callee, args } = &expr.kind else {
            return;
        };
        let Some(defaults) = callee.lambda_defaults() else {
            return;
        };
        if !matches!(lambda(hir, defaults.id).map(|e| &e.kind), Some(hir::ExprKind::Lambda { captures, .. }) if !captures.is_empty())
        {
            return;
        }
        for (index, param) in defaults.params.iter().enumerate().skip(args.len()) {
            if let Some(default) = &param.default {
                helpers.insert((defaults.id.0, index), &default.pos);
            }
        }
    });
    helpers.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use subscript_codegen::lir::lower_module;
    use subscript_compiler::{check_program, SourceFile};

    fn fixture() -> (hir::Module, l::Module) {
        let hir = check_program(&[SourceFile::new(
            "defaults.ts",
            r#"
            function fail(): i32 { throw new Error("failed"); }
            export async function main(): Promise<void> {
                const k: i32 = 5;
                const cap = (n: i32 = k): i32 => n;
                print(`${cap()}`);
                const al = async (n: i32 = fail()): Promise<void> => {};
                try { const p = al(); print("after"); await p; } catch (e) {}
            }
        "#,
        )])
        .expect("checks");
        let lir = lower_module(&hir).expect("lowers");
        assert!(super::super::dropped_facts(&hir, &lir).is_empty());
        (hir, lir)
    }

    #[test]
    fn removing_the_failed_handle_allocation_is_a_finding() {
        let (hir, mut lir) = fixture();
        let helper = lir
            .functions
            .iter_mut()
            .find(|f| f.source_name.starts_with("<failed async default"))
            .expect("failed handle helper");
        helper.creation_traps.clear();
        assert!(super::super::dropped_facts(&hir, &lir)
            .iter()
            .any(|f| f.contains("trap \"Allocation\"")));
    }

    #[test]
    fn removing_the_captured_default_lambda_role_is_a_finding() {
        let (hir, mut lir) = fixture();
        let helper = lir
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .find_map(|i| match &i.kind {
                l::InstructionKind::Call(l::CallTarget {
                    kind: l::CallTargetKind::ClosureDefault { helper, .. },
                    ..
                }) => Some(*helper),
                _ => None,
            })
            .expect("captured default helper");
        lir.functions[helper.0 as usize].kind = l::FunctionKind::SynthesizedHelper;
        assert!(super::super::dropped_facts(&hir, &lir)
            .iter()
            .any(|f| f.contains("lambda function ids")));
    }
}
