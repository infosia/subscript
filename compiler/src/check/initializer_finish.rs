//! Places the prefixes that delayed argument checks produce in their source owners.
use super::*;

impl Checker<'_> {
    pub(super) fn finish_expression_children(
        &mut self,
        value: &mut hir::Expr,
    ) -> Vec<(SyntheticOwnerKind, Vec<hir::Stmt>)> {
        let mut prefixes = Vec::new();
        if let hir::ExprKind::Lambda { params, body, .. } = &mut value.kind {
            for parameter in params {
                if let Some(default) = &mut parameter.default {
                    prefixes.extend(self.finish_expression(default));
                }
            }
            self.finish_initializer_body(body);
        } else {
            for child in value.children_mut() {
                if let hir::HirChildMut::Expr(expression) = child {
                    prefixes.extend(self.finish_expression(expression));
                }
            }
        }
        prefixes
    }

    fn finish_initializer_body(&mut self, body: &mut Vec<hir::Stmt>) {
        let mut out = Vec::new();
        for mut statement in std::mem::take(body) {
            out.extend(self.finish_initializer_statement(&mut statement));
            out.push(statement);
        }
        *body = out;
    }

    fn finish_initializer_statement(&mut self, statement: &mut hir::Stmt) -> Vec<hir::Stmt> {
        match statement {
            hir::Stmt::If { then, els, .. } => {
                self.finish_initializer_body(then);
                if let Some(els) = els {
                    self.finish_initializer_body(els);
                }
            }
            hir::Stmt::While { body, .. }
            | hir::Stmt::For { body, .. }
            | hir::Stmt::ForOf { body, .. }
            | hir::Stmt::Block(body)
            | hir::Stmt::Using { body, .. } => self.finish_initializer_body(body),
            hir::Stmt::Switch { cases, .. } => {
                for case in cases {
                    self.finish_initializer_body(&mut case.body);
                }
            }
            hir::Stmt::Try { body, handler, .. } => {
                self.finish_initializer_body(body);
                self.finish_initializer_body(handler);
            }
            _ => {}
        }
        let mut before = Vec::new();
        if let hir::Stmt::For {
            init: Some(init), ..
        } = statement
        {
            before.extend(self.finish_initializer_statement(init));
        }
        let mut condition = Vec::new();
        let mut update = Vec::new();
        for child in statement.children_mut() {
            if let hir::HirChildMut::Expr(expression) = child {
                for (owner, prefix) in self.finish_expression(expression) {
                    match owner {
                        SyntheticOwnerKind::ForCond(_) => condition.extend(prefix),
                        SyntheticOwnerKind::ForUpdate(_) => update.extend(prefix),
                        _ => before.extend(prefix),
                    }
                }
            }
        }
        if (!condition.is_empty() || !update.is_empty())
            && matches!(statement, hir::Stmt::For { .. })
        {
            let hir::Stmt::For {
                init,
                cond,
                step,
                mut body,
                pos,
            } = std::mem::replace(statement, hir::Stmt::Block(Vec::new()))
            else {
                return before;
            };
            if let Some(step) = step {
                update.push(hir::Stmt::Expr(step));
            }
            super::stmt::insert_for_step_before_continues(&mut body, &update);
            body.extend(update);
            let cond = cond.unwrap_or_else(|| hir::Expr {
                pending_work: None,
                kind: hir::ExprKind::Bool(true),
                ty: Type::Bool,
                pos: pos.clone(),
            });
            let (cond, body) = if condition.is_empty() {
                (cond, body)
            } else {
                condition.push(hir::Stmt::If {
                    cond,
                    then: body,
                    els: Some(vec![hir::Stmt::Break(pos.clone())]),
                    pos: pos.clone(),
                });
                (
                    hir::Expr {
                        pending_work: None,
                        kind: hir::ExprKind::Bool(true),
                        ty: Type::Bool,
                        pos: pos.clone(),
                    },
                    condition,
                )
            };
            let mut block = Vec::new();
            if let Some(init) = init {
                block.push(*init);
            }
            block.push(hir::Stmt::While { cond, body, pos });
            *statement = hir::Stmt::Block(block);
        }
        before
    }
}
