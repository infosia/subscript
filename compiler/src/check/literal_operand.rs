//! Literal operands and source-order exceptions (compiler.md §330 rules 1 to 3a).

use super::{Checker, FnCtx, ScopeItem};
use crate::{hir, Type};
use std::collections::HashSet;
use swc_ecma_ast as ast;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum LiteralOperand {
    String(String),
    Number(f64),
    Boolean(bool),
}

impl LiteralOperand {
    pub(super) fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    fn differs(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other) && self != other
    }

    fn text(&self) -> String {
        match self {
            Self::String(text) => format!("{text:?}"),
            Self::Number(value) => value.to_string(),
            Self::Boolean(value) => value.to_string(),
        }
    }
}

impl Checker<'_> {
    /// The source forms of §299 rule 2a and §316. Local lookup stops at the
    /// owning scope, including parameters and declarations that shadow a const.
    pub(super) fn literal_operand(&self, source: &ast::Expr, fx: &FnCtx) -> Option<LiteralOperand> {
        self.literal_source(source, Some(fx), self.cur_file, None, &mut HashSet::new())
    }

    pub(super) fn checked_literal_operand(
        &self,
        source: &ast::Expr,
        checked: &hir::Expr,
        fx: &FnCtx,
    ) -> Option<LiteralOperand> {
        if let hir::ExprKind::Field { obj, name } = &checked.kind {
            let Type::Class(class) = self.apparent_type(&obj.ty) else {
                return None;
            };
            let signatures = self.class_sigs.get(class.0)?;
            let signature = signatures.fields.get(name)?;
            if signature.mutable || signature.annotated {
                return None;
            }
            let initializer = signature.initializer.as_ref()?;
            let mut seen = HashSet::new();
            seen.insert(format!("field:{}:false:{name}", class.0));
            return self.literal_source(
                &initializer.expression,
                None,
                initializer.file,
                Some(class),
                &mut seen,
            );
        }
        self.literal_operand(source, fx)
    }

    fn literal_source(
        &self,
        source: &ast::Expr,
        fx: Option<&FnCtx>,
        file: usize,
        field_class: Option<crate::types::ClassId>,
        seen: &mut HashSet<String>,
    ) -> Option<LiteralOperand> {
        match source {
            ast::Expr::Lit(ast::Lit::Num(number)) => Some(LiteralOperand::Number(number.value)),
            ast::Expr::Lit(ast::Lit::Bool(value)) => Some(LiteralOperand::Boolean(value.value)),
            ast::Expr::Unary(unary) if unary.op == ast::UnaryOp::Minus => {
                let ast::Expr::Lit(ast::Lit::Num(number)) = super::expr::unparen_expr(&unary.arg)
                else {
                    return None;
                };
                Some(LiteralOperand::Number(-number.value))
            }
            ast::Expr::Lit(ast::Lit::Str(text)) => {
                Some(LiteralOperand::String(text.value.to_string()))
            }
            ast::Expr::Tpl(tpl) if tpl.exprs.is_empty() => Some(LiteralOperand::String(
                tpl.quasis
                    .iter()
                    .map(|q| q.cooked.as_ref().unwrap_or(&q.raw).as_ref())
                    .collect(),
            )),
            ast::Expr::Paren(paren) => {
                self.literal_source(&paren.expr, fx, file, field_class, seen)
            }
            ast::Expr::Ident(id) => {
                let name = id.sym.as_ref();
                if let Some(scope) = fx.and_then(|fx| fx.scopes.iter().rev().find(|s| s.owns(name)))
                {
                    return scope.literal_operands.get(name).cloned();
                }
                let binding = self.file_scopes.get(file)?.get(name)?;
                let ScopeItem::Global(symbol) = &binding.item else {
                    return None;
                };
                if binding.type_only || !seen.insert(symbol.clone()) {
                    return None;
                }
                let sig = self.global_sigs.get(symbol)?;
                if sig.mutable || sig.annotated {
                    return None;
                }
                let init = sig.initializer.as_ref()?;
                self.literal_source(&init.expression, None, init.file, None, seen)
            }
            ast::Expr::Member(member) => {
                let ast::MemberProp::Ident(name) = &member.prop else {
                    return None;
                };
                let (class, is_static) = match super::expr::unparen_expr(&member.obj) {
                    ast::Expr::Ident(id) => {
                        let local = fx.and_then(|fx| {
                            fx.scopes.iter().rev().find(|s| s.owns(id.sym.as_ref()))
                        });
                        if let Some(scope) = local {
                            let Type::Class(class) =
                                self.apparent_type(&scope.vars.get(id.sym.as_ref())?.ty)
                            else {
                                return None;
                            };
                            (class, false)
                        } else {
                            let binding = self.file_scopes.get(file)?.get(id.sym.as_ref())?;
                            if binding.type_only {
                                return None;
                            }
                            match &binding.item {
                                ScopeItem::Class(class) => (*class, true),
                                ScopeItem::Global(symbol) => {
                                    let Type::Class(class) =
                                        self.apparent_type(self.global_sigs.get(symbol)?.ty())
                                    else {
                                        return None;
                                    };
                                    (class, false)
                                }
                                _ => return None,
                            }
                        }
                    }
                    ast::Expr::This(_) => {
                        (fx.and_then(|fx| fx.lexical_class).or(field_class)?, false)
                    }
                    _ => return None,
                };
                let signatures = self.class_sigs.get(class.0)?;
                let signature = if is_static {
                    &signatures.static_fields
                } else {
                    &signatures.fields
                }
                .get(name.sym.as_ref())?;
                if signature.mutable || signature.annotated {
                    return None;
                }
                let key = format!("field:{}:{is_static}:{}", class.0, name.sym);
                if !seen.insert(key) {
                    return None;
                }
                let initializer = signature.initializer.as_ref()?;
                self.literal_source(
                    &initializer.expression,
                    None,
                    initializer.file,
                    Some(class),
                    seen,
                )
            }
            _ => None,
        }
    }

    fn literal_binding_key(value: &hir::Expr, fx: &FnCtx) -> Option<String> {
        match &value.kind {
            hir::ExprKind::Local(name, ..) => {
                let scope = fx.scopes.iter().rposition(|scope| scope.owns(name))?;
                Some(format!("local:{scope}:{name}"))
            }
            hir::ExprKind::Global(symbol, _) => Some(format!("global:{symbol}")),
            hir::ExprKind::This => Some(format!("this:{}", fx.lexical_class?.0)),
            hir::ExprKind::Field { obj, name } => {
                Some(format!("{}.{}", Self::literal_binding_key(obj, fx)?, name))
            }
            _ => None,
        }
    }

    pub(super) fn literal_test_precedes(
        &self,
        value: &hir::Expr,
        before: swc_common::BytePos,
        include_cases: bool,
        fx: &FnCtx,
    ) -> bool {
        let Some(key) = Self::literal_binding_key(value, fx) else {
            return false;
        };
        fx.scopes.iter().any(|scope| {
            scope
                .literal_tests
                .get(&key)
                .is_some_and(|at| *at < before.0)
                || (include_cases
                    && scope
                        .literal_case_tests
                        .get(&key)
                        .is_some_and(|at| *at < before.0))
        })
    }

    fn record_literal_test(
        &self,
        value: &hir::Expr,
        at: swc_common::BytePos,
        case: bool,
        fx: &mut FnCtx,
    ) {
        if let Some(key) = Self::literal_binding_key(value, fx) {
            if let Some(scope) = fx.scopes.last_mut() {
                let tests = if case {
                    &mut scope.literal_case_tests
                } else {
                    &mut scope.literal_tests
                };
                let prior = tests.entry(key).or_insert(at.0);
                *prior = (*prior).min(at.0);
            }
        }
    }

    /// compiler.md §330 rules 2 and 3a: compare values before alias conversion.
    pub(super) fn check_literal_equality(
        &mut self,
        binary: &ast::BinExpr,
        left: &hir::Expr,
        right: &hir::Expr,
        fx: &mut FnCtx,
    ) {
        if !matches!(
            binary.op,
            ast::BinaryOp::EqEqEq
                | ast::BinaryOp::NotEqEq
                | ast::BinaryOp::EqEq
                | ast::BinaryOp::NotEq
        ) {
            return;
        }
        let (Some(a), Some(b)) = (
            self.checked_literal_operand(&binary.left, left, fx),
            self.checked_literal_operand(&binary.right, right, fx),
        ) else {
            return;
        };
        if a == b {
            self.record_literal_test(left, binary.span.hi, false, fx);
            self.record_literal_test(right, binary.span.hi, false, fx);
        } else if a.differs(&b)
            && !self.literal_test_precedes(left, binary.span.lo, true, fx)
            && !self.literal_test_precedes(right, binary.span.lo, true, fx)
        {
            self.reject_subset(
                super::rejection::RejectionSite::DisjointLiteralEquality,
                format!(
                    "comparison of different literals {} and {}",
                    a.text(),
                    b.text()
                ),
                self.pos(binary.span),
            );
        }
    }

    /// compiler.md §330 rules 3 and 3a: only tests before the switch exempt a label.
    pub(super) fn check_literal_case(
        &mut self,
        subject: &hir::Expr,
        literals: (Option<&LiteralOperand>, Option<&LiteralOperand>),
        prior_test: bool,
        label: &ast::Expr,
        fx: &mut FnCtx,
    ) {
        use swc_common::Spanned;
        let (Some(a), Some(b)) = literals else {
            return;
        };
        if a == b {
            self.record_literal_test(subject, label.span().hi, true, fx);
        } else if a.differs(b) && !prior_test {
            self.reject_subset(
                super::rejection::RejectionSite::DisjointLiteralCase,
                format!(
                    "case literal {} differs from subject literal {}",
                    b.text(),
                    a.text()
                ),
                self.pos(label.span()),
            );
        }
    }
}
