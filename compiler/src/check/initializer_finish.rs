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
            hir::Stmt::If {
                then,
                els,
                cond: _,
                pos: _,
            } => {
                self.finish_initializer_body(then);
                if let Some(els) = els {
                    self.finish_initializer_body(els);
                }
            }
            hir::Stmt::While {
                body,
                cond: _,
                pos: _,
            }
            | hir::Stmt::ForOf {
                body,
                name: _,
                ty: _,
                subject: _,
                kind: _,
                pos: _,
            }
            | hir::Stmt::GeneratorForOf {
                body,
                name: _,
                ty: _,
                mutable: _,
                subject: _,
                pos: _,
            }
            | hir::Stmt::Block(body) => self.finish_initializer_body(body),
            hir::Stmt::For {
                body,
                step,
                init: _,
                cond: _,
                pos: _,
            } => {
                self.finish_initializer_body(body);
                self.finish_initializer_body(step);
            }
            hir::Stmt::Using {
                body,
                finalizer,
                bindings: _,
                pos: _,
            } => {
                self.finish_initializer_body(body);
                if let Some(finalizer) = finalizer {
                    self.finish_initializer_body(finalizer);
                }
            }
            hir::Stmt::Switch {
                cases,
                disc: _,
                pos: _,
            } => {
                for case in cases {
                    self.finish_initializer_body(&mut case.body);
                }
            }
            hir::Stmt::Try {
                body,
                handler,
                binding: _,
                pos: _,
            } => {
                self.finish_initializer_body(body);
                self.finish_initializer_body(handler);
            }
            _ => {}
        }
        let mut before = Vec::new();
        if let hir::Stmt::For {
            init: Some(init),
            cond: _,
            step: _,
            body: _,
            pos: _,
        } = statement
        {
            before.extend(self.finish_initializer_statement(init));
        }
        let mut condition = Vec::new();
        let mut update = Vec::new();
        let is_for = matches!(
            statement,
            hir::Stmt::For {
                init: _,
                cond: _,
                step: _,
                body: _,
                pos: _
            }
        );
        for child in statement.children_mut() {
            if let hir::HirChildMut::Expr(expression) = child {
                for (owner, prefix) in self.finish_expression(expression) {
                    match owner {
                        SyntheticOwnerKind::ForCond(_) if is_for => condition.extend(prefix),
                        SyntheticOwnerKind::ForUpdate(_) if is_for => update.extend(prefix),
                        _ => before.extend(prefix),
                    }
                }
            }
        }
        if (!condition.is_empty() || !update.is_empty())
            && matches!(
                statement,
                hir::Stmt::For {
                    init: _,
                    cond: _,
                    step: _,
                    body: _,
                    pos: _
                }
            )
        {
            let hir::Stmt::For {
                init,
                cond,
                step,
                body,
                pos,
            } = std::mem::replace(statement, hir::Stmt::Block(Vec::new()))
            else {
                return before;
            };
            update.extend(step);
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
            block.push(hir::Stmt::For {
                init: None,
                cond: Some(cond),
                step: update,
                body,
                pos,
            });
            *statement = hir::Stmt::Block(block);
        }
        before
    }
}
