//! Checks array literals, array spread literals, and descriptor object literals.

use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{Checker, FnCtx};
use crate::diag::{Pos, RuleCode};
use crate::divergence::Divergence;
use crate::hir::{self, ExprKind};
use crate::types::Type;

use super::DescriptorProp;

impl<'p> Checker<'p> {
    pub(super) fn check_array_lit(
        &mut self,
        a: &ast::ArrayLit,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if a.elems
            .iter()
            .flatten()
            .any(|element| element.spread.is_some())
        {
            return self.check_array_spread_lit(a, ctx, fx, pos);
        }
        let mut elems: Vec<&ast::ExprOrSpread> = Vec::new();
        for e in &a.elems {
            match e {
                Some(e) if e.spread.is_none() => elems.push(e),
                Some(_) => unreachable!("spread literal dispatched above"),
                None => {
                    self.error(RuleCode::S100, "array holes are not decided", pos.clone());
                }
            }
        }
        match ctx {
            Some(Type::Array(elem_ty)) => {
                let elem_ty = (**elem_ty).clone();
                let mut out = Vec::new();
                for e in elems {
                    let checked = self.check_expr(&e.expr, Some(&elem_ty), fx);
                    self.require_assignable(
                        &checked.ty.clone(),
                        &elem_ty,
                        checked.pos.clone(),
                        "the array element",
                    );
                    if self.is_capturing_value(&checked, fx) {
                        self.error(
                            RuleCode::S009,
                            "capturing lambdas may not be stored in arrays",
                            checked.pos.clone(),
                        );
                    }
                    out.push(checked);
                }
                hir::Expr {
                    kind: ExprKind::ArrayLit(out),
                    ty: Type::Array(Box::new(elem_ty)),
                    pos,
                }
            }
            Some(Type::FixedArray(elem_ty, n)) => {
                let elem_ty = (**elem_ty).clone();
                let n = *n;
                if elems.len() != n as usize {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "FixedArray length mismatch: the annotation says {}, \
                             the literal has {} elements",
                            n,
                            elems.len()
                        ),
                        pos.clone(),
                    );
                }
                let mut out = Vec::new();
                for e in elems {
                    let checked = self.check_expr(&e.expr, Some(&elem_ty), fx);
                    self.require_assignable(
                        &checked.ty.clone(),
                        &elem_ty,
                        checked.pos.clone(),
                        "the array element",
                    );
                    if self.is_capturing_value(&checked, fx) {
                        self.error(
                            RuleCode::S009,
                            "capturing lambdas may not be stored in arrays",
                            checked.pos.clone(),
                        );
                    }
                    out.push(checked);
                }
                hir::Expr {
                    kind: ExprKind::ArrayLit(out),
                    ty: Type::FixedArray(Box::new(elem_ty), n),
                    pos,
                }
            }
            _ => {
                if elems.is_empty() {
                    self.error(
                        RuleCode::S100,
                        "cannot infer the type of an empty array literal without context",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let first = self.check_expr(&elems[0].expr, None, fx);
                let elem_ty = first.ty.clone();
                if Self::is_context_affine_type(&elem_ty) {
                    self.error(
                        RuleCode::S100,
                        "Worker, Inbox, and Outbox values may not be array elements",
                        first.pos.clone(),
                    );
                }
                let mut out = vec![first];
                for e in &elems[1..] {
                    let checked = self.check_expr(&e.expr, Some(&elem_ty), fx);
                    self.require_assignable(
                        &checked.ty.clone(),
                        &elem_ty,
                        checked.pos.clone(),
                        "the array element",
                    );
                    out.push(checked);
                }
                for checked in &out {
                    if self.is_capturing_value(checked, fx) {
                        self.error(
                            RuleCode::S009,
                            "capturing lambdas may not be stored in arrays",
                            checked.pos.clone(),
                        );
                    }
                }
                hir::Expr {
                    kind: ExprKind::ArrayLit(out),
                    ty: Type::Array(Box::new(elem_ty)),
                    pos,
                }
            }
        }
    }

    pub(super) fn check_descriptor_lit(
        &mut self,
        object: &ast::ObjectLit,
        class_id: crate::types::ClassId,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        let class = self.classes[class_id.0].clone();
        let mut provided: Vec<(String, DescriptorProp<'_>, Pos)> = Vec::new();
        for prop in &object.props {
            let (name, value, prop_pos) = match prop {
                ast::PropOrSpread::Spread(spread) => {
                    self.error(
                        RuleCode::S100,
                        "spread properties are not supported in descriptor literals",
                        self.pos(spread.dot3_token),
                    );
                    continue;
                }
                ast::PropOrSpread::Prop(prop) => match &**prop {
                    ast::Prop::KeyValue(key_value) => {
                        let ast::PropName::Ident(key) = &key_value.key else {
                            self.error(
                                RuleCode::S100,
                                "descriptor literal member names must be identifiers",
                                self.pos(key_value.key.span()),
                            );
                            continue;
                        };
                        (
                            key.sym.to_string(),
                            DescriptorProp::Expr(&key_value.value),
                            self.pos(key.span),
                        )
                    }
                    ast::Prop::Shorthand(ident) => (
                        ident.sym.to_string(),
                        DescriptorProp::Shorthand(ident),
                        self.pos(ident.span),
                    ),
                    other => {
                        self.error(
                            RuleCode::S100,
                            "descriptor literals contain data properties only",
                            self.pos(other.span()),
                        );
                        continue;
                    }
                },
            };
            if provided.iter().any(|(existing, _, _)| existing == &name) {
                self.error(
                    RuleCode::S100,
                    format!("duplicate descriptor literal member `{name}`"),
                    prop_pos,
                );
                continue;
            }
            if !class.fields.iter().any(|field| field.name == name) {
                self.error(
                    RuleCode::S004,
                    format!(
                        "descriptor class `{}` has no declared property `{name}`",
                        class.name
                    ),
                    prop_pos,
                );
                continue;
            }
            provided.push((name, value, prop_pos));
        }

        let mut fields = Vec::with_capacity(class.fields.len());
        for field in &class.fields {
            let explicit = provided
                .iter()
                .find(|(name, _, _)| name == &field.name)
                .map(|(_, value, _)| *value);
            let checked = match explicit {
                Some(DescriptorProp::Expr(value)) => {
                    Some(self.check_expr(value, Some(&field.ty), fx))
                }
                Some(DescriptorProp::Shorthand(ident)) => Some(self.check_ident(ident, None, fx)),
                None if field.is_defaulted => None,
                None if field.is_absence_capable => {
                    let sentinel = match &field.ty {
                        Type::StringAlias(id) => self
                            .string_aliases
                            .get(id.0)
                            .map_or(-1, hir::StringAliasDef::absence_discriminant),
                        _ => -1,
                    };
                    Some(hir::Expr {
                        kind: ExprKind::Int(sentinel),
                        ty: field.ty.clone(),
                        pos: pos.clone(),
                    })
                }
                None => {
                    self.error(
                        RuleCode::S100,
                        format!(
                            "descriptor literal for `{}` is missing required member `{}`",
                            class.name, field.name
                        ),
                        pos.clone(),
                    );
                    None
                }
            };
            if let Some(checked) = &checked {
                self.require_assignable(
                    &checked.ty.clone(),
                    &field.ty,
                    checked.pos.clone(),
                    "the descriptor member",
                );
                if self.is_capturing_value(checked, fx) {
                    self.error(
                        RuleCode::S009,
                        "capturing lambdas may not escape into descriptor objects",
                        checked.pos.clone(),
                    );
                }
            }
            fields.push(checked);
        }

        hir::Expr {
            kind: ExprKind::DescriptorLit {
                class: class_id,
                fields,
            },
            ty: Type::Class(class_id),
            pos,
        }
    }

    /// Checks an array literal containing spread (stdlib.md §14). It is a
    /// distinct HIR form, so ordinary literals and `FixedArray` in-place
    /// construction keep their own lowering.
    fn check_array_spread_lit(
        &mut self,
        a: &ast::ArrayLit,
        ctx: Option<&Type>,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        if matches!(ctx, Some(Type::FixedArray(..))) {
            self.error(
                RuleCode::S014,
                "array-literal spread produces a fresh T[]; it cannot construct a FixedArray",
                pos.clone(),
            );
        }
        let context_elem = match ctx {
            Some(Type::Array(elem)) => Some((**elem).clone()),
            _ => None,
        };
        let mut checked = Vec::new();
        let mut inferred: Option<Type> = context_elem.clone();
        for slot in &a.elems {
            let Some(slot) = slot else {
                self.error(RuleCode::S100, "array holes are not decided", pos.clone());
                continue;
            };
            let is_spread = slot.spread.is_some();
            let expr = self.check_expr(
                &slot.expr,
                if is_spread { None } else { inferred.as_ref() },
                fx,
            );
            let (spread, element_ty) = if is_spread {
                let spread_pos = self.pos(slot.spread.unwrap_or(a.span));
                let selected = match &expr.ty {
                    // compiler.md §104.1 rules 1 and 4: the operand is
                    // rejected on its resolved type. §79 rule 6: the site
                    // serves both `tsc` classes, and its variant explains
                    // the unannotated form that stock `tsc` accepts.
                    Type::Map(..) => {
                        self.error_diverging(
                            RuleCode::S014,
                            "a bare `Map` is not an array-literal spread operand: this \
                             language binds `K` and TypeScript binds a `[K, V]` pair, so an \
                             accepted program fails the `tsc` gate; push `map.keys()` or \
                             `map.values()` into the array with a `for…of` loop",
                            spread_pos,
                            Divergence::BareMapToArray,
                        );
                        None
                    }
                    Type::Generator(_) => {
                        self.error_diverging(
                            RuleCode::S014,
                            "Generator<T> is single-use; array-literal spread would consume \
                             a value expression",
                            spread_pos,
                            Divergence::GeneratorSingleUse,
                        );
                        None
                    }
                    Type::Error => None,
                    other => match other.iteration_element() {
                        Some((kind, element)) => Some((hir::SpreadKind::from(kind), element)),
                        None => {
                            let actual = self.type_name(other);
                            self.error(
                                RuleCode::S014,
                                format!(
                                    "array-literal spread accepts T[], FixedArray<T, N>, Set, \
                                     or string; got `{actual}`"
                                ),
                                spread_pos,
                            );
                            None
                        }
                    },
                };
                match selected {
                    Some((kind, ty)) => (Some(kind), ty),
                    None => (None, Type::Error),
                }
            } else {
                (None, expr.ty.clone())
            };
            if inferred.is_none() && !matches!(element_ty, Type::Error) {
                inferred = Some(element_ty.clone());
            }
            if let Some(expected) = &inferred {
                self.require_assignable(
                    &element_ty,
                    expected,
                    expr.pos.clone(),
                    "the array element",
                );
            }
            if !is_spread && self.is_capturing_value(&expr, fx) {
                self.error(
                    RuleCode::S009,
                    "capturing lambdas may not be stored in arrays",
                    expr.pos.clone(),
                );
            }
            checked.push(hir::ArrayLitElem { expr, spread });
        }
        // A spread literal holds at least one element, so the element
        // type is absent only after a hole or a rejected element, and
        // each of those already carries its own diagnostic. A second
        // message here names a shape this literal does not have
        // (compiler.md §103: a reason must fit the form it rejects).
        let Some(elem_ty) = inferred else {
            return self.err_expr(pos);
        };
        if Self::is_context_affine_type(&elem_ty) {
            self.error(
                RuleCode::S100,
                "Worker, Inbox, and Outbox values may not be array elements",
                pos.clone(),
            );
        }
        hir::Expr {
            kind: ExprKind::ArraySpreadLit(checked),
            ty: Type::Array(Box::new(elem_ty)),
            pos,
        }
    }
}
