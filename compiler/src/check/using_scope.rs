//! The `using` scopes of a checked body (`compiler.md` §115.5 rule 5).
//!
//! The statements that follow a `using` declaration, to the end of its
//! block, become the body of one [`Stmt::Using`]. A later declaration in
//! the same block starts a node inside the body of the earlier one, so the
//! nodes nest in declaration order.
//!
//! A `switch` binding keeps a storage local and an active flag
//! (`compiler.md` §97.1 rule 7): the declaration in a case arm is not
//! visible at the exits of the `switch`. The `switch` statement is the body
//! of one node that carries each such binding by its storage and its flag.
//!
//! The checker places no hook call. The lowering places each one.

use super::fallthrough::sequence_can_fall_through;
use crate::diag::Pos;
use crate::hir::{Expr, ExprKind, Stmt, SwitchCase, UsingBinding};
use crate::types::Type;

/// Returns `statements` with each `using` scope as one [`Stmt::Using`].
/// `next_switch_id` numbers the storage locals of `switch` bindings.
///
/// A statement that control cannot leave ends its sequence: the
/// statements after it are dropped, so no exit of a scope lies where
/// control cannot arrive (`compiler.md` §101 rule 1).
pub(super) fn structure(statements: Vec<Stmt>, next_switch_id: &mut usize) -> Vec<Stmt> {
    let mut out = Vec::with_capacity(statements.len());
    let mut rest = statements.into_iter();
    while let Some(statement) = rest.next() {
        let stops = !sequence_can_fall_through(std::slice::from_ref(&statement));
        match statement {
            Stmt::Let {
                dispose: true,
                ref name,
                ref ty,
                ref pos,
                ..
            } => {
                let binding = UsingBinding::new(name.clone(), ty.clone(), None, pos.clone());
                let pos = pos.clone();
                out.push(statement);
                let body = structure(rest.collect(), next_switch_id);
                out.push(Stmt::Using {
                    bindings: vec![binding],
                    body,
                    pos,
                });
                return out;
            }
            Stmt::Switch { disc, cases, pos } => {
                out.extend(switch(disc, cases, pos, next_switch_id));
            }
            other => out.push(nested(other, next_switch_id)),
        }
        if stops {
            break;
        }
    }
    out
}

/// Structures the blocks that `statement` holds.
fn nested(statement: Stmt, ids: &mut usize) -> Stmt {
    match statement {
        Stmt::If {
            cond,
            then,
            els,
            pos,
        } => Stmt::If {
            cond,
            then: structure(then, ids),
            els: els.map(|els| structure(els, ids)),
            pos,
        },
        Stmt::While { cond, body, pos } => Stmt::While {
            cond,
            body: structure(body, ids),
            pos,
        },
        Stmt::For {
            init,
            cond,
            step,
            body,
            pos,
        } => Stmt::For {
            init,
            cond,
            step,
            body: structure(body, ids),
            pos,
        },
        Stmt::ForOf {
            name,
            ty,
            subject,
            kind,
            body,
            pos,
        } => Stmt::ForOf {
            name,
            ty,
            subject,
            kind,
            body: structure(body, ids),
            pos,
        },
        Stmt::Block(body) => Stmt::Block(structure(body, ids)),
        Stmt::Try {
            body,
            binding,
            handler,
            pos,
        } => Stmt::Try {
            body: structure(body, ids),
            binding,
            handler: structure(handler, ids),
            pos,
        },
        Stmt::Using {
            bindings,
            body,
            pos,
        } => Stmt::Using {
            bindings,
            body: structure(body, ids),
            pos,
        },
        Stmt::Switch { disc, cases, pos } => {
            let mut statements = switch(disc, cases, pos.clone(), ids);
            if statements.len() == 1 {
                statements.remove(0)
            } else {
                Stmt::Block(statements)
            }
        }
        Stmt::Let { .. }
        | Stmt::Expr(_)
        | Stmt::Return { .. }
        | Stmt::Break(_)
        | Stmt::Continue(_)
        | Stmt::Throw { .. } => statement,
    }
}

/// A `switch` whose case arms declare `using` bindings: the storage and
/// the flag of each binding, then one node whose body is the `switch`.
fn switch(disc: Expr, cases: Vec<SwitchCase>, pos: Pos, ids: &mut usize) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut bindings = Vec::new();
    let mut checked_cases = Vec::with_capacity(cases.len());
    for case in cases {
        let mut body = Vec::with_capacity(case.body.len());
        for statement in case.body {
            let stops = !sequence_can_fall_through(std::slice::from_ref(&statement));
            let Stmt::Let {
                dispose: true,
                ref name,
                ref ty,
                pos: ref binding_pos,
                ..
            } = statement
            else {
                body.push(nested(statement, ids));
                if stops {
                    break;
                }
                continue;
            };
            let id = *ids;
            *ids += 1;
            let active = format!("[[using.active#{id}]]");
            let storage = format!("[[using.value#{id}]]");
            let (name, ty, binding_pos) = (name.clone(), ty.clone(), binding_pos.clone());
            out.push(local(
                &active,
                Type::Bool,
                ExprKind::Bool(false),
                &binding_pos,
            ));
            out.push(local(&storage, ty.clone(), ExprKind::Null, &binding_pos));
            bindings.push(UsingBinding::new(
                storage.clone(),
                ty.clone(),
                Some(active.clone()),
                binding_pos.clone(),
            ));
            body.push(statement);
            body.push(assign(
                &storage,
                ty.clone(),
                ExprKind::Local(name, ty.clone()),
                ty,
                &binding_pos,
            ));
            body.push(assign(
                &active,
                Type::Bool,
                ExprKind::Bool(true),
                Type::Bool,
                &binding_pos,
            ));
        }
        checked_cases.push(SwitchCase {
            test: case.test,
            body,
            pos: case.pos,
        });
    }
    let switch = Stmt::Switch {
        disc,
        cases: checked_cases,
        pos: pos.clone(),
    };
    match bindings.first() {
        None => out.push(switch),
        Some(first) => {
            let pos = first.pos.clone();
            out.push(Stmt::Using {
                bindings,
                body: vec![switch],
                pos,
            });
        }
    }
    out
}

/// A mutable checker-internal local.
fn local(name: &str, ty: Type, init: ExprKind, pos: &Pos) -> Stmt {
    Stmt::Let {
        name: name.to_string(),
        ty: ty.clone(),
        mutable: true,
        dispose: false,
        init: Expr {
            kind: init,
            ty,
            pos: pos.clone(),
        },
        pos: pos.clone(),
    }
}

/// `name = value` as a statement.
fn assign(name: &str, ty: Type, value: ExprKind, value_ty: Type, pos: &Pos) -> Stmt {
    Stmt::Expr(Expr {
        kind: ExprKind::Assign {
            op: None,
            target: Box::new(Expr {
                kind: ExprKind::Local(name.to_string(), ty.clone()),
                ty: ty.clone(),
                pos: pos.clone(),
            }),
            value: Box::new(Expr {
                kind: value,
                ty: value_ty,
                pos: pos.clone(),
            }),
        },
        ty,
        pos: pos.clone(),
    })
}

#[cfg(test)]
mod tests {
    use crate::hir::{Function, Stmt};
    use crate::{check_program, SourceFile};

    fn main_body(source: &str) -> Vec<Stmt> {
        let module =
            check_program(&[SourceFile::new("using-scope.ts", source)]).expect("accepted fixture");
        let main: &Function = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main");
        main.body.clone()
    }

    fn holds_hook_call(statements: &[Stmt]) -> bool {
        statements.iter().any(|statement| {
            matches!(statement, Stmt::Expr(expression) if matches!(
                &expression.kind,
                crate::hir::ExprKind::Call {
                    callee: crate::hir::Callee::Method { name, .. },
                    ..
                } if name == crate::hir::DISPOSE_METHOD_NAME
            )) || statement.children().into_iter().any(|child| match child {
                crate::hir::HirChild::Stmt(child) => holds_hook_call(std::slice::from_ref(child)),
                crate::hir::HirChild::Expr(_) => false,
            })
        })
    }

    const RESOURCE: &str = "class R { [Symbol.dispose](): void { print(\"d\"); } }\n";

    #[test]
    fn a_declaration_scopes_the_rest_of_its_block_and_places_no_hook() {
        let body = main_body(&format!(
            "{RESOURCE}export function main(): void {{\n\
               print(\"before\");\n\
               using a = new R();\n\
               using b: R | null = null;\n\
               if (true) {{ return; }}\n\
               print(\"after\");\n\
             }}\n"
        ));
        let [Stmt::Expr(_), Stmt::Let { name: a, .. }, Stmt::Using {
            bindings: outer,
            body: outer_body,
            ..
        }] = body.as_slice()
        else {
            panic!("print, let, node: {body:?}");
        };
        assert_eq!(a, "a");
        assert_eq!(outer.len(), 1);
        assert_eq!(outer[0].name, "a");
        assert!(!outer[0].nullable());
        let [Stmt::Let { name: b, .. }, Stmt::Using {
            bindings: inner,
            body: inner_body,
            ..
        }] = outer_body.as_slice()
        else {
            panic!("let, node: {outer_body:?}");
        };
        assert_eq!(b, "b");
        assert!(inner[0].nullable());
        assert_eq!(
            inner_body.len(),
            1,
            "`if (true) {{ return; }}` cannot be left, so the statement after it drops"
        );
        assert!(!holds_hook_call(&body));
    }

    #[test]
    fn a_switch_binding_is_carried_by_its_storage_and_its_flag() {
        let body = main_body(&format!(
            "{RESOURCE}export function main(): void {{\n\
               const n: i32 = 1;\n\
               switch (n) {{\n\
                 case 0:\n\
                   using a = new R();\n\
                 case 1:\n\
                   using b = new R();\n\
                   break;\n\
               }}\n\
               print(\"after\");\n\
             }}\n"
        ));
        let node = body
            .iter()
            .find_map(|statement| match statement {
                Stmt::Using { bindings, body, .. } => Some((bindings, body)),
                _ => None,
            })
            .expect("the switch node");
        let (bindings, node_body) = node;
        assert_eq!(bindings.len(), 2);
        for binding in bindings.iter() {
            assert!(binding.name.starts_with("[[using.value#"));
            assert!(binding
                .active
                .as_deref()
                .is_some_and(|active| active.starts_with("[[using.active#")));
        }
        assert!(matches!(node_body.as_slice(), [Stmt::Switch { .. }]));
        assert!(
            matches!(body.last(), Some(Stmt::Expr(_))),
            "the statement after the switch is outside the node"
        );
        assert!(!holds_hook_call(&body));
    }

    #[test]
    fn a_switch_without_using_keeps_its_form() {
        let body = main_body(
            "export function main(): void {\n\
               const n: i32 = 1;\n\
               switch (n) { case 0: print(\"zero\"); break; }\n\
             }\n",
        );
        assert!(matches!(
            body.as_slice(),
            [Stmt::Let { .. }, Stmt::Switch { .. }]
        ));
        let mut ids = 0;
        assert_eq!(super::structure(body.clone(), &mut ids), body);
        assert_eq!(ids, 0);
    }
}
