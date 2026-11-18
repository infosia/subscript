//! The alias-to-string decision for value places (§259) and operands (§299).

use super::rejection::RejectionSite;
use super::type_rules::cast_value;
use super::{Checker, FnCtx};
use crate::{hir, Type};
use std::collections::HashSet;
use swc_ecma_ast::{self as ast, BinaryOp};

pub(super) enum StringOperand<'a> {
    Place(&'a Type),
    Binary {
        op: BinaryOp,
        other: &'a mut hir::Expr,
        literal: Option<&'a str>,
        compound: bool,
        left: bool,
    },
    Member,
    Assertion(&'a Type),
    Case {
        subject: &'a mut hir::Expr,
        literal: Option<&'a str>,
    },
    ArrayElement(&'a hir::Expr),
}

impl Checker<'_> {
    /// Every site reads this decision before constructing the §259 conversion.
    pub(super) fn convert_string_operand(
        &mut self,
        value: &mut hir::Expr,
        usage: StringOperand<'_>,
    ) -> bool {
        let Type::StringAlias(alias) = self.apparent_type(&value.ty) else {
            return false;
        };
        // §299 rules 2 and 2a: literal operands take check_lit's existing
        // member decision. A non-member remains a string literal and keeps
        // the existing operator/case diagnostic instead of widening the alias.
        let literal_other = match usage {
            StringOperand::Binary {
                op: BinaryOp::EqEqEq | BinaryOp::NotEqEq,
                ref other,
                literal: Some(text),
                compound: false,
                ..
            } => (self.apparent_type(&other.ty) == Type::Str).then_some(text),
            StringOperand::Case {
                ref subject,
                literal: Some(text),
            } => (self.apparent_type(&subject.ty) == Type::Str).then_some(text),
            _ => None,
        };
        if let Some(text) = literal_other {
            let case = matches!(usage, StringOperand::Case { .. });
            let other = match usage {
                StringOperand::Binary { other, .. } => other,
                StringOperand::Case { subject, .. } => subject,
                _ => return false,
            };
            let original_field =
                matches!(other.kind, hir::ExprKind::Field { .. }).then(|| other.clone());
            *other = self.check_lit(
                &ast::Lit::Str(ast::Str {
                    span: Default::default(),
                    value: text.into(),
                    raw: None,
                }),
                Some(&value.ty),
                other.pos.clone(),
            );
            if case && self.apparent_type(&other.ty) == Type::Str {
                let alias_name = self.type_name(&value.ty);
                self.reject_subset(RejectionSite::LiteralAliasUnknownCase,
                    format!("case label {text:?} is not a member of string-literal union alias `{alias_name}`"), other.pos.clone());
                *value = self.err_expr(value.pos.clone());
            }
            if let Some(original) =
                original_field.filter(|_| self.apparent_type(&other.ty) != Type::Str)
            {
                // §316: retain receiver evaluation when the literal supplies the discriminant.
                other.kind = hir::ExprKind::Seq {
                    effects: vec![original],
                    value: Box::new(other.clone()),
                };
            }
            return false;
        }
        let string = |ty: &Type| {
            let apparent = self.apparent_type(ty);
            apparent == Type::Str
                || matches!(apparent, Type::Nullable(ref inner, _) if self.apparent_type(inner) == Type::Str)
        };
        let converts = match usage {
            StringOperand::Place(target) => string(target),
            StringOperand::Member => true,
            StringOperand::Assertion(target) => self.apparent_type(target) == Type::Str,
            StringOperand::Case { subject, .. } => self.apparent_type(&subject.ty) == Type::Str,
            StringOperand::Binary {
                op,
                other,
                compound,
                left,
                ..
            } => {
                let literal = matches!(&other.kind, hir::ExprKind::Str(_));
                let other = self.apparent_type(&other.ty);
                match op {
                    BinaryOp::Add if compound => !left && other == Type::Str,
                    BinaryOp::Add => other == Type::Str || other == Type::StringAlias(alias),
                    BinaryOp::EqEqEq | BinaryOp::NotEqEq if !compound && !literal => string(&other),
                    _ => false,
                }
            }
            StringOperand::ArrayElement(other) => self.apparent_type(&other.ty) == Type::Str,
        };
        if converts {
            *value = cast_value(value.clone(), Type::Str);
        }
        converts
    }

    /// §299 rules 1.6 and 4: any string widens a context-free alias list.
    pub(super) fn string_alias_array_element(
        &mut self,
        previous: &mut [hir::Expr],
        next: &mut hir::Expr,
        element: &Type,
    ) -> Option<Type> {
        let apparent = self.apparent_type(element);
        if let Type::StringAlias(_) = apparent {
            let first = previous.first_mut()?;
            if self.convert_string_operand(first, StringOperand::ArrayElement(next)) {
                for value in previous {
                    self.convert_string_operand(value, StringOperand::Place(&Type::Str));
                }
                return Some(Type::Str);
            }
        } else if apparent == Type::Str
            && self.convert_string_operand(next, StringOperand::Place(&Type::Str))
        {
            return Some(Type::Str);
        }
        None
    }

    pub(super) fn checked_string_literal_operand(
        &self,
        source: &ast::Expr,
        checked: &hir::Expr,
        fx: &FnCtx,
    ) -> Option<String> {
        self.checked_literal_operand(source, checked, fx)
            .and_then(|literal| literal.as_string().map(str::to_owned))
    }

    pub(super) fn check_switch_case_test(
        &mut self,
        t: &ast::Expr,
        subject: (
            &hir::Expr,
            Option<&super::literal_operand::LiteralOperand>,
            bool,
        ),
        alias_switch: Option<&(String, Vec<String>)>,
        alias_members_seen: &mut HashSet<usize>,
        alias_labels_valid: &mut bool,
        fx: &mut FnCtx,
    ) -> hir::Expr {
        let (disc, subject_literal, prior_test) = subject;
        let mut subject_value = disc.clone();
        let disc = &mut subject_value;
        self.mark_operand_calls(t);
        let mut checked = self.check_expr(t, Some(&disc.ty), fx);
        let label_operand = self.checked_literal_operand(t, &checked, fx);
        self.check_literal_case(
            disc,
            (subject_literal, label_operand.as_ref()),
            prior_test,
            t,
            fx,
        );
        let subject_literal = subject_literal.and_then(|literal| literal.as_string());
        let label_literal = label_operand
            .as_ref()
            .and_then(|literal| literal.as_string())
            .map(str::to_owned);
        self.convert_string_operand(
            disc,
            StringOperand::Binary {
                op: BinaryOp::EqEqEq,
                other: &mut checked,
                literal: label_literal.as_deref(),
                compound: false,
                left: true,
            },
        );
        self.convert_string_operand(
            &mut checked,
            StringOperand::Case {
                subject: disc,
                literal: subject_literal,
            },
        );
        let disc_ty = &disc.ty;
        let string_subject = self.apparent_type(disc_ty) == Type::Str;
        if let Some((alias_name, members)) = alias_switch {
            match label_literal {
                Some(label) => {
                    if let Some(index) = members.iter().position(|member| member == &label) {
                        self.require_expr_assignable(&mut checked, disc_ty, fx, "the case label");
                        if !alias_members_seen.insert(index) {
                            *alias_labels_valid = false;
                            self.reject_subset(RejectionSite::LiteralAliasDuplicateCase, format!(
                                    "duplicate case label {label:?} for string-literal union alias `{alias_name}`"
                                ), checked.pos.clone());
                        }
                    } else {
                        *alias_labels_valid = false;
                        self.reject_subset(RejectionSite::LiteralAliasUnknownCase, format!(
                                "case label {label:?} is not a member of string-literal union alias `{alias_name}`"
                            ), checked.pos.clone());
                    }
                }
                _ if string_subject => {}
                _ => {
                    *alias_labels_valid = false;
                    self.reject_subset(RejectionSite::AliasCaseNonLiteral, format!(
                            "case labels for string-literal union alias `{alias_name}` must be string literals naming a member"
                        ), checked.pos.clone());
                }
            }
        } else if (self.involves_type_parameter(disc_ty)
            || self.involves_type_parameter(&checked.ty))
            && self.generic_overlap(disc_ty, &checked.ty)
        {
            // §143 rule 1a: case labels compare values, rather than assign them.
        } else {
            self.require_expr_assignable(&mut checked, disc_ty, fx, "the case label");
        }
        checked
    }

    /// §321: save the subject once and compare each label in source order.
    pub(super) fn finish_alias_string_switch(
        &mut self,
        disc: &mut hir::Expr,
        cases: &mut [hir::SwitchCase],
        out: &mut Vec<hir::Stmt>,
    ) {
        if !matches!(self.apparent_type(&disc.ty), Type::StringAlias(_))
            || !cases.iter().filter_map(|c| c.test.as_ref()).any(|test| {
                let ty = self.apparent_type(&test.ty);
                ty == Type::Str
                    || matches!(ty, Type::Nullable(ref inner, _) if self.apparent_type(inner) == Type::Str)
            })
        {
            return;
        }
        let name = format!("#switch:{}:{}", disc.pos.line, disc.pos.col);
        out.push(hir::Stmt::Let {
            name: name.clone(),
            ty: disc.ty.clone(),
            mutable: false,
            dispose: false,
            init: disc.clone(),
            pos: disc.pos.clone(),
        });
        for case in cases {
            if let Some(test) = case.test.take() {
                let mut subject = hir::Expr {
                    kind: hir::ExprKind::Local(name.clone(), disc.ty.clone(), false),
                    ..disc.clone()
                };
                let mut other = test.clone();
                self.convert_string_operand(
                    &mut subject,
                    StringOperand::Binary {
                        op: BinaryOp::EqEqEq,
                        other: &mut other,
                        literal: None,
                        compound: false,
                        left: true,
                    },
                );
                case.test = Some(hir::Expr {
                    pending_work: None,
                    kind: hir::ExprKind::Binary {
                        op: if Type::compares_string_content(
                            &self.apparent_type(&subject.ty),
                            &self.apparent_type(&test.ty),
                        ) {
                            hir::BinOp::StrEq
                        } else {
                            hir::BinOp::Eq
                        },
                        left: Box::new(subject),
                        right: Box::new(test),
                    },
                    ty: Type::Bool,
                    pos: case.pos.clone(),
                });
            }
        }
        *disc = hir::Expr {
            kind: hir::ExprKind::Bool(true),
            ty: Type::Bool,
            ..disc.clone()
        };
    }

    /// §299 rule 2: a literal subject can meet alias and string labels in
    /// one switch. A boolean dispatch preserves label evaluation order,
    /// fallthrough, and the no-match exit while each pair keeps its compare.
    pub(super) fn finish_literal_string_switch(
        &mut self,
        disc: &mut hir::Expr,
        cases: &mut [hir::SwitchCase],
        literal: Option<&str>,
    ) {
        let Some(text) = literal else {
            return;
        };
        if self.apparent_type(&disc.ty) != Type::Str
            || !cases
                .iter()
                .filter_map(|case| case.test.as_ref())
                .any(|test| matches!(self.apparent_type(&test.ty), Type::StringAlias(_)))
        {
            return;
        }
        for case in cases {
            if let Some(test) = case.test.take() {
                let mut test = test;
                let mut subject = disc.clone();
                self.convert_string_operand(
                    &mut test,
                    StringOperand::Binary {
                        op: BinaryOp::EqEqEq,
                        other: &mut subject,
                        literal: Some(text),
                        compound: false,
                        left: false,
                    },
                );
                case.test = Some(hir::Expr {
                    pending_work: None,
                    kind: hir::ExprKind::Binary {
                        op: hir::BinOp::Eq,
                        left: Box::new(subject),
                        right: Box::new(test),
                    },
                    ty: Type::Bool,
                    pos: case.pos.clone(),
                });
            }
        }
        *disc = hir::Expr {
            pending_work: None,
            kind: hir::ExprKind::Bool(true),
            ty: Type::Bool,
            pos: disc.pos.clone(),
        };
    }
}
