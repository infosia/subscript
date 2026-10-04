//! Checks array literals, array spread literals, and descriptor object literals.

use crate::check::rejection::RejectionSite;
use swc_common::Spanned;
use swc_ecma_ast as ast;

use crate::check::{Checker, ContainerSlot, FnCtx};
use crate::diag::Pos;
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
                    self.reject_subset(
                        RejectionSite::ArrayLiteralHole,
                        "array holes are not decided",
                        pos.clone(),
                    );
                }
            }
        }
        match ctx.map(|ty| self.apparent_type(ty)).as_ref() {
            // compiler.md §132 rule 2: the declaration that gives the
            // poisoned context reported the failure. Each element takes
            // the poisoned context, so a nested container reports nothing
            // more and the element still reports its own errors.
            Some(Type::Error) => {
                for e in elems {
                    let _ = self.check_expr(&e.expr, Some(&Type::Error), fx);
                }
                self.err_expr(pos)
            }
            Some(Type::Array(elem_ty)) => {
                let elem_ty = (**elem_ty).clone();
                let mut out = Vec::new();
                for e in elems {
                    let checked = self.check_expr(&e.expr, Some(&elem_ty), fx);
                    self.require_expr_assignable(&checked, &elem_ty, fx, "the array element");
                    out.push(checked);
                }
                hir::Expr {
                    pending_work: None,
                    kind: ExprKind::ArrayLit(out),
                    ty: Type::array(elem_ty),
                    pos,
                }
            }
            Some(Type::FixedArray(elem_ty, n)) => {
                let elem_ty = (**elem_ty).clone();
                let n = *n;
                if elems.len() != n as usize {
                    self.reject_subset(
                        RejectionSite::FixedArrayLiteralLength,
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
                    self.require_expr_assignable(&checked, &elem_ty, fx, "the array element");
                    out.push(checked);
                }
                hir::Expr {
                    pending_work: None,
                    kind: ExprKind::ArrayLit(out),
                    ty: Type::fixed_array(elem_ty, n),
                    pos,
                }
            }
            _ => {
                if elems.is_empty() {
                    self.reject_subset(
                        RejectionSite::EmptyArrayInference,
                        "cannot infer the type of an empty array literal without context",
                        pos.clone(),
                    );
                    return self.err_expr(pos);
                }
                let first = self.check_expr(&elems[0].expr, None, fx);
                let mut elem_ty = first.ty.clone();
                let mut out = vec![first];
                for e in &elems[1..] {
                    let context = (!self.involves_type_parameter(&elem_ty)).then_some(&elem_ty);
                    let checked = self.check_expr(&e.expr, context, fx);
                    if self.involves_type_parameter(&elem_ty)
                        || self.involves_type_parameter(&checked.ty)
                    {
                        elem_ty = self.generic_union(&elem_ty, &checked.ty);
                    } else {
                        self.require_expr_assignable(&checked, &elem_ty, fx, "the array element");
                    }
                    out.push(checked);
                }
                let elem_ty = self.container_argument(
                    ContainerSlot::ArrayElement,
                    elem_ty,
                    out[0].pos.clone(),
                );
                hir::Expr {
                    pending_work: None,
                    kind: ExprKind::ArrayLit(out),
                    ty: Type::array(elem_ty),
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
        let property_diagnostics = self.diags.len();
        let mut provided: Vec<(String, DescriptorProp<'_>, Pos)> = Vec::new();
        for prop in &object.props {
            let (name, value, prop_pos) = match prop {
                ast::PropOrSpread::Spread(spread) => {
                    self.reject_subset(
                        RejectionSite::DescriptorLiteralSpread,
                        "spread properties are not supported in descriptor literals",
                        self.pos(spread.dot3_token),
                    );
                    continue;
                }
                ast::PropOrSpread::Prop(prop) => match &**prop {
                    ast::Prop::KeyValue(key_value) => {
                        let ast::PropName::Ident(key) = &key_value.key else {
                            self.reject_subset(
                                RejectionSite::DescriptorLiteralQuotedKey,
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
                        self.reject_subset(
                            RejectionSite::DescriptorLiteralAccessor,
                            "descriptor literals contain data properties only",
                            self.pos(other.span()),
                        );
                        continue;
                    }
                },
            };
            if provided.iter().any(|(existing, _, _)| existing == &name) {
                self.reject_subset(
                    RejectionSite::DescriptorLiteralDuplicateMember,
                    format!("duplicate descriptor literal member `{name}`"),
                    prop_pos,
                );
                continue;
            }
            if !class.fields.iter().any(|field| field.name == name) {
                self.reject_subset(
                    RejectionSite::DescriptorLiteralUnknownMember,
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

        if self.diags.len() != property_diagnostics {
            return self.err_expr(pos);
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
                    let sentinel = match &self.apparent_type(&field.ty) {
                        Type::StringAlias(id) => self
                            .string_aliases
                            .get(id.0)
                            .map_or(-1, hir::StringAliasDef::absence_discriminant),
                        _ => -1,
                    };
                    Some(hir::Expr {
                        pending_work: None,
                        kind: ExprKind::Int(sentinel),
                        ty: field.ty.clone(),
                        pos: pos.clone(),
                    })
                }
                None => {
                    if object.props.iter().any(|property| matches!(property, ast::PropOrSpread::Prop(property) if matches!(&**property, ast::Prop::KeyValue(value) if matches!(&value.key, ast::PropName::Str(key) if key.value.as_str() == field.name.as_str())))) {
                        fields.push(None);
                        continue;
                    }
                    self.reject_subset(
                        if object.props.iter().any(|property| matches!(property, ast::PropOrSpread::Prop(property) if matches!(&**property, ast::Prop::KeyValue(value) if matches!(&value.key, ast::PropName::Str(key) if key.value.as_str() == field.name.as_str())))) { RejectionSite::DescriptorRequiredMemberQuotedKey } else { RejectionSite::DescriptorRequiredMemberMissing },
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
                self.require_expr_assignable(&checked, &field.ty, fx, "the descriptor member");
            }
            fields.push(checked);
        }

        hir::Expr {
            pending_work: None,
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
        if matches!(
            ctx.map(|ty| self.apparent_type(ty)),
            Some(Type::FixedArray(..))
        ) {
            self.reject_subset(
                RejectionSite::ArraySpreadFixedArray,
                "array-literal spread produces a fresh T[]; it cannot construct a FixedArray",
                pos.clone(),
            );
        }
        let context_elem = match ctx.map(|ty| self.apparent_type(ty)).as_ref() {
            Some(Type::Array(elem)) => Some((**elem).clone()),
            _ => None,
        };
        let mut checked = Vec::new();
        let mut inferred: Option<Type> = context_elem.clone();
        for slot in &a.elems {
            let Some(slot) = slot else {
                self.reject_subset(
                    RejectionSite::ArraySpreadLiteralHole,
                    "array holes are not decided",
                    pos.clone(),
                );
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
                let apparent = self.apparent_type(&expr.ty);
                let selected = match &self.apparent_type(&apparent) {
                    // compiler.md §104.1 rules 1 and 4: the operand is
                    // rejected on its resolved type. §79 rule 6: the site
                    // serves both `tsc` classes, and its variant explains
                    // the unannotated form that stock `tsc` accepts.
                    Type::Map(..) => {
                        self.reject_subset(
                            RejectionSite::ArraySpreadMap,
                            "a bare `Map` is not an array-literal spread operand: Map \
                             traversal binds `K`; a `[K, V]` pair has no tuple representation \
                             in the language; push `map.keys()` or \
                             `map.values()` into the array with a `for…of` loop",
                            spread_pos,
                        );
                        None
                    }
                    Type::Generator(_) => {
                        self.reject_subset(
                            RejectionSite::ArraySpreadGenerator,
                            "Generator<T> is single-use; array-literal spread would consume \
                             a value expression",
                            spread_pos,
                        );
                        None
                    }
                    Type::Error => None,
                    other => match self.apparent_type(other).iteration_element() {
                        Some((kind, element)) => Some((hir::SpreadKind::from(kind), element)),
                        None => {
                            let actual = self.type_name(other);
                            self.reject_subset(
                                RejectionSite::ArraySpreadSource,
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
            if inferred.is_none() && !matches!(self.apparent_type(&element_ty), Type::Error) {
                let saved_context = self.enter_container_context(ctx);
                let admitted = self.container_argument(
                    ContainerSlot::ArrayElement,
                    element_ty.clone(),
                    expr.pos.clone(),
                );
                self.leave_container_context(saved_context);
                inferred = Some(admitted);
            }
            if let Some(expected) = &inferred {
                self.require_assignable(
                    &element_ty,
                    expected,
                    expr.pos.clone(),
                    "the array element",
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
        hir::Expr {
            pending_work: None,
            kind: ExprKind::ArraySpreadLit(checked),
            ty: Type::array(elem_ty),
            pos,
        }
    }
}
