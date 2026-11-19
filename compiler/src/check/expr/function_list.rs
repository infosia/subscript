//! Places spread elements in function lists (compiler.md §305 rules 2 to 4).

use crate::check::{Checker, FnCtx, Local, Scope};
use crate::hir::{self, ExprKind};
use crate::types::Type;
use oxc_ast::ast;

use super::entry::ContextKind;

impl<'p> Checker<'p> {
    /// Only a context-free function list may defer conditional placement
    /// until its candidate selection (§305 rules 1 and 3).
    pub(super) fn check_function_list_element(
        &mut self,
        expr: &ast::Expression,
        hint: Option<&Type>,
        fx: &mut FnCtx<'p>,
    ) -> hir::Expr {
        self.check_expr_as(expr, hint, ContextKind::FunctionListHint, fx)
    }

    /// If list selection fails, the original hint places each conditional
    /// branch, retaining the diagnostics of §242 rule 1a.
    pub(super) fn require_function_list_element(
        &mut self,
        expr: &mut hir::Expr,
        expected: &Type,
        fx: &FnCtx<'p>,
        what: &str,
    ) {
        if !matches!(self.apparent_type(expected), Type::Func(_)) {
            self.require_expr_assignable(expr, expected, fx, what);
            return;
        }
        if let ExprKind::Cond { then, els, .. } = &mut expr.kind {
            self.require_function_list_element(then, expected, fx, "the then branch");
            self.require_function_list_element(els, expected, fx, "the else branch");
            expr.ty = if then.ty == els.ty {
                then.ty.clone()
            } else {
                expected.clone()
            };
        } else {
            self.require_expr_assignable(expr, expected, fx, what);
        }
    }

    /// Each spread array supplies a const element value for type selection.
    /// An adapter maps those values into a fresh source for the spread copy.
    pub(super) fn place_spread_function_list(
        &mut self,
        elements: &mut [hir::ArrayLitElem],
        fx: &FnCtx<'p>,
    ) -> Option<Type> {
        let mut frame = fx.clone();
        frame.scopes.push(Scope::default());
        let mut values = Vec::with_capacity(elements.len());
        let mut sources = Vec::with_capacity(elements.len());
        for (index, element) in elements.iter().enumerate() {
            if element.spread.is_none() {
                values.push(element.expr.clone());
                sources.push(None);
                continue;
            }
            // §305 rule 2: only an array of functions is a spread candidate.
            if element.spread != Some(hir::SpreadKind::Array) {
                return None;
            }
            let Type::Array(ty) = self.apparent_type(&element.expr.ty) else {
                return None;
            };
            if !matches!(self.apparent_type(&ty), Type::Func(_)) {
                return None;
            }
            let name = format!("[[spread#{}.{index}]]", self.next_lambda_id);
            frame.declare(
                &name,
                Local {
                    function_value_required: None,
                    annotated: true,
                    ty: (*ty).clone(),
                    mutable: false,
                    async_origins: Default::default(),
                    caught: false,
                },
                false,
            );
            sources.push(Some((name.clone(), (*ty).clone())));
            values.push(hir::Expr {
                pending_work: None,
                kind: ExprKind::Local(name, (*ty).clone(), false),
                ty: *ty,
                pos: element.expr.pos.clone(),
            });
        }
        let selected =
            self.place_function_list(&mut values.iter_mut().collect::<Vec<_>>(), &frame)?;
        for ((element, value), source) in elements.iter_mut().zip(values).zip(sources) {
            let Some((name, own)) = source else {
                element.expr = value;
                continue;
            };
            if self.apparent_type(&own) == self.apparent_type(&selected) {
                continue;
            }
            self.map_spread_function_value(element, name, own, value, &selected);
        }
        Some(selected)
    }

    /// Records the element conversion of compiler.md §334 rule 4 through
    /// the spread adapter of compiler.md §305 rule 3.
    pub(super) fn place_spread_function_element(
        &mut self,
        element: &mut hir::ArrayLitElem,
        own: &Type,
        expected: &Type,
        fx: &FnCtx<'p>,
    ) {
        if !self.needs_function_conversion(own, expected) {
            return;
        }
        let name = format!("[[spread#{}]]", self.next_lambda_id);
        let mut frame = fx.clone();
        frame.scopes.push(Scope::default());
        frame.declare(
            &name,
            Local {
                function_value_required: None,
                annotated: true,
                ty: own.clone(),
                mutable: false,
                async_origins: Default::default(),
                caught: false,
            },
            false,
        );
        let mut value = hir::Expr {
            pending_work: None,
            kind: ExprKind::Local(name.clone(), own.clone(), false),
            ty: own.clone(),
            pos: element.expr.pos.clone(),
        };
        self.require_expr_assignable(&mut value, expected, &frame, "the array element");
        self.map_spread_function_value(element, name, own.clone(), value, expected);
    }

    /// Copies a checked element value through the existing array-map form.
    fn map_spread_function_value(
        &mut self,
        element: &mut hir::ArrayLitElem,
        name: String,
        own: Type,
        value: hir::Expr,
        selected: &Type,
    ) {
        let pos = element.expr.pos.clone();
        let id = hir::LambdaId(self.next_lambda_id);
        self.next_lambda_id += 1;
        self.next_lambda_ordinal += 1;
        let callback = hir::Expr {
            pending_work: None,
            kind: ExprKind::Lambda {
                group: None,
                id,
                params: vec![hir::Param {
                    escape_kind: hir::EscapeKind::None,
                    default_can_raise: false,
                    name,
                    ty: own.clone(),
                    default: None,
                    foreign_provenance: None,
                    pos: pos.clone(),
                }],
                is_async: false,
                owns_environment: false,
                ret: selected.clone(),
                body: vec![hir::Stmt::Return {
                    value: Some(value),
                    pos: pos.clone(),
                }],
                captures: Vec::new(),
                can_raise: false,
            },
            ty: Type::func(vec![own], selected.clone()),
            pos: pos.clone(),
        };
        let source = std::mem::replace(&mut element.expr, self.err_expr(pos.clone()));
        element.expr = hir::Expr {
            pending_work: None,
            kind: ExprKind::Call {
                callee: hir::Callee::Arr(hir::ArrFn::Map),
                args: vec![source, callback],
            },
            ty: Type::array(selected.clone()),
            pos,
        };
    }
}
