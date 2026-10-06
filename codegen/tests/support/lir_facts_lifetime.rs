//! Additional lifetime sites from statement and expanded-operation semantics.

use super::*;

fn add_read(
    expr: &hir::Expr,
    hir: &hir::Module,
    count: usize,
    expected: &mut BTreeMap<TrapKey, usize>,
) {
    for site in expr.statement_read_sites(hir) {
        *expected.entry(hir_trap_key(&site)).or_default() += count;
    }
}

pub(super) fn expression(
    expr: &hir::Expr,
    hir: &hir::Module,
    expected: &mut BTreeMap<TrapKey, usize>,
) {
    use hir::{Callee, ExprKind as K};
    let counted_action = match &expr.kind {
        K::Call {
            callee: Callee::Arr(hir::ArrFn::Fill | hir::ArrFn::CopyWithin),
            args,
        } => args
            .first()
            .is_some_and(|receiver| receiver.ty.counted_type().is_some()),
        K::ArraySpreadLit(_) | K::AsyncCall { .. } | K::AsyncHandleAwait(_) => {
            expr.ty.counted_type().is_some()
        }
        _ => false,
    };
    if counted_action {
        let site = hir::TrapSite::Call {
            pos: expr.pos.clone(),
        };
        *expected.entry(hir_trap_key(&site)).or_default() += 1;
    }
    match &expr.kind {
        K::Assign {
            op: Some(_),
            target,
            ..
        } => {
            repeated_address(target, hir, expected);
        }
        K::Call {
            callee: Callee::Method { recv, .. },
            ..
        } => {
            if matches!(recv.ty, Type::Class(id) if hir.classes[id.0].is_value) {
                repeated_address(recv, hir, expected);
            }
        }
        K::Call {
            callee: Callee::Foreign(_),
            args,
        } => {
            for argument in args {
                if matches!(argument.ty, Type::Array(_)) {
                    add_read(argument, hir, 2, expected);
                }
            }
        }
        K::Call {
            callee: Callee::Ambient(hir::AmbientFn::UnsafeDelete),
            args,
        } => {
            if let Some(argument) = args.first() {
                if let Type::Class(id) = argument.ty {
                    let count = hir.classes[id.0]
                        .fields
                        .iter()
                        .filter(|field| field.ty.counted_type().is_some())
                        .count();
                    let site = hir::TrapSite::DevOnlyRelease {
                        operand: hir::LifetimeOperand::Argument(0),
                        pos: expr.pos.clone(),
                    };
                    *expected.entry(hir_trap_key(&site)).or_default() += count;
                }
            }
        }
        K::Call {
            callee: Callee::Map(hir::MapFn::ForEach),
            args,
        } => {
            add_read(&args[0], hir, 1, expected);
        }
        K::Call {
            callee: Callee::Arr(operation),
            args,
        } => {
            if let Some(callback) = static_array_callback(*operation, args) {
                if matches!(operation, hir::ArrFn::Map | hir::ArrFn::Filter) {
                    let site = hir::TrapSite::DevOnlyLifetime {
                        operand: hir::LifetimeOperand::Receiver,
                        pos: expr.pos.clone(),
                    };
                    *expected.entry(hir_trap_key(&site)).or_default() += 1;
                }
                if *operation == hir::ArrFn::ReduceRight
                    && matches!(&callback.ty, Type::Func(signature) if operation.callback_index_arity() == Some(signature.params.len()))
                {
                    add_read(&args[0], hir, 1, expected);
                }
            }
        }
        K::Lambda { body, .. } => statements(body, hir, expected),
        _ => {}
    }
}

pub(super) fn statements(
    body: &[hir::Stmt],
    hir: &hir::Module,
    expected: &mut BTreeMap<TrapKey, usize>,
) {
    for stmt in body {
        use hir::Stmt as S;
        match stmt {
            S::ForOf { subject, body, .. } => {
                add_read(subject, hir, 1, expected);
                statements(body, hir, expected);
            }
            S::Throw { value, .. } => add_read(value, hir, 2, expected),
            S::If { then, els, .. } => {
                statements(then, hir, expected);
                if let Some(els) = els {
                    statements(els, hir, expected);
                }
            }
            S::While { body, .. } | S::Block(body) | S::Using { body, .. } => {
                statements(body, hir, expected)
            }
            S::For { init, body, .. } => {
                if let Some(init) = init {
                    statements(std::slice::from_ref(init), hir, expected);
                }
                statements(body, hir, expected);
            }
            S::Switch { cases, .. } => {
                for case in cases {
                    statements(&case.body, hir, expected);
                }
            }
            S::Try { body, handler, .. } => {
                statements(body, hir, expected);
                if try_body_raises(hir, body) {
                    statements(handler, hir, expected);
                }
            }
            S::Let { .. } | S::Expr(_) | S::Return { .. } | S::Break(_) | S::Continue(_) => {}
        }
        if stops_statement_sequence(hir, stmt) {
            break;
        }
    }
}

fn repeated_address(expr: &hir::Expr, hir: &hir::Module, expected: &mut BTreeMap<TrapKey, usize>) {
    match &expr.kind {
        hir::ExprKind::Field { obj, .. } | hir::ExprKind::Index { obj, .. } => {
            add_read(obj, hir, 1, expected);
            if matches!(obj.ty, Type::Class(id) if hir.classes[id.0].is_value)
                || matches!(obj.ty, Type::FixedArray(..))
            {
                repeated_address(obj, hir, expected);
            }
        }
        _ => {}
    }
}

#[test]
fn an_extra_lifetime_site_is_a_fact_mismatch() {
    let hir = subscript_compiler::check_program(&[subscript_compiler::SourceFile::new(
        "extra-site.ts",
        "export function main(): void { const x = new Set<i32>(); x.has(1); }",
    )])
    .unwrap();
    let mut lir = subscript_codegen::lir::lower_module(&hir).unwrap();
    assert!(dropped_facts(&hir, &lir).is_empty());
    let instruction = lir
        .functions
        .iter_mut()
        .filter(|function| function.source_name == "main")
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| {
            instruction
                .traps
                .iter()
                .any(|trap| matches!(trap.kind, l::TrapKind::DevOnlyLifetime(_)))
        })
        .unwrap();
    let trap = instruction
        .traps
        .iter()
        .find(|trap| matches!(trap.kind, l::TrapKind::DevOnlyLifetime(_)))
        .unwrap()
        .clone();
    instruction.traps.push(trap);
    let findings = dropped_facts(&hir, &lir);
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("carries 2 site(s); HIR requires 1")),
        "{findings:?}"
    );
}
