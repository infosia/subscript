//! Checks the logical assignments `??=`, `||=`, and `&&=` (compiler.md §199).

use swc_ecma_ast as ast;

use crate::check::rejection::RejectionSite;
use crate::check::{static_member_symbol, Checker, FnCtx, SyntheticPrefix};
use crate::diag::Pos;
use crate::hir::{self, BinOp, Callee, ExprKind};
use crate::types::Type;

use super::{is_place_expr, path_key, write_spelling, Place, PlaceSource, WriteExpression};

/// One logical assignment operator (compiler.md §199.1 rule 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LogicalAssign {
    /// `p ??= v` is `p !== null ? p : (p = v)`.
    Nullish,
    /// `p ||= v` is `p ? p : (p = v)`.
    Or,
    /// `p &&= v` is `p ? (p = v) : p`.
    And,
}

impl LogicalAssign {
    /// Selects the logical assignment that `op` spells, if any.
    pub(super) fn of(op: ast::AssignOp) -> Option<Self> {
        match op {
            ast::AssignOp::NullishAssign => Some(Self::Nullish),
            ast::AssignOp::OrAssign => Some(Self::Or),
            ast::AssignOp::AndAssign => Some(Self::And),
            _ => None,
        }
    }

    /// The source spelling of the operator.
    pub(super) fn spelling(self) -> &'static str {
        match self {
            Self::Nullish => "??=",
            Self::Or => "||=",
            Self::And => "&&=",
        }
    }
}

/// The checked form: an expression, or the statement that an accessor or
/// an index signature needs (§199.1 rule 2).
enum Checked {
    Expr(hir::Expr),
    Stmt(hir::Stmt),
}

impl<'p> Checker<'p> {
    /// Checks a logical assignment in value position (§199.1 rule 5).
    pub(super) fn check_logical_assign_value(
        &mut self,
        a: &ast::AssignExpr,
        op: LogicalAssign,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> hir::Expr {
        match self.check_logical_assign(a, op, fx, pos.clone(), None) {
            Checked::Expr(expression) => expression,
            Checked::Stmt(_) => self.err_expr(pos),
        }
    }

    /// Checks a logical assignment in statement position. The statements
    /// that bind a non-place receiver or index come first (§82.1 rule 3).
    pub(super) fn check_logical_assign_statement(
        &mut self,
        a: &ast::AssignExpr,
        op: LogicalAssign,
        fx: &mut FnCtx,
        pos: Pos,
    ) -> Vec<hir::Stmt> {
        let mut prefix = SyntheticPrefix::default();
        let checked = self.check_logical_assign(a, op, fx, pos, Some(&mut prefix));
        let mut out = prefix.into_statements();
        out.push(match checked {
            Checked::Expr(expression) => hir::Stmt::Expr(expression),
            Checked::Stmt(statement) => statement,
        });
        out
    }

    fn check_logical_assign(
        &mut self,
        a: &ast::AssignExpr,
        op: LogicalAssign,
        fx: &mut FnCtx,
        pos: Pos,
        mut prefix: Option<&mut SyntheticPrefix>,
    ) -> Checked {
        let source = match &a.left {
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(binding)) => {
                PlaceSource::Ident(&binding.id)
            }
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) => {
                PlaceSource::Member(member)
            }
            _ => PlaceSource::Unsupported,
        };
        let place = self.check_assign_target(source, fx, &pos);
        match place {
            Place::IndexSignature { .. } | Place::Accessor { .. } => match prefix {
                Some(prefix) => self.check_logical_call_write(a, op, place, fx, pos, prefix),
                None => {
                    self.reject_call_write_value(a, &place, pos.clone());
                    Checked::Expr(self.err_expr(pos))
                }
            },
            place => {
                let target = place.into_read(self);
                Checked::Expr(self.check_logical_place(a, op, target, fx, pos, &mut prefix))
            }
        }
    }

    /// Rejects an accessor or index-signature write in value position, as
    /// the plain assignment is rejected (§58.1 rule 7, §65.1 rule 7).
    fn reject_call_write_value(&mut self, a: &ast::AssignExpr, place: &Place, pos: Pos) {
        let (site, target) = match place {
            Place::IndexSignature { .. } => (
                RejectionSite::IndexAssignmentExpressionValue,
                "a[i]".to_string(),
            ),
            Place::Accessor {
                class,
                receiver: None,
                name,
                ..
            } => (
                RejectionSite::StaticAccessorAssignmentExpressionValue,
                format!("{}.{name}", self.classes[class.0].name),
            ),
            Place::Accessor { name, .. } => (
                RejectionSite::AccessorAssignmentExpressionValue,
                format!("x.{name}"),
            ),
            _ => return,
        };
        let spelling = write_spelling(WriteExpression::Assign(a), &target);
        self.reject_subset(site, format!("{spelling} cannot be used as a value"), pos);
    }

    /// Checks `p op= v` where `p` is a local, a global, a field, a static
    /// field, or an array element.
    fn check_logical_place(
        &mut self,
        a: &ast::AssignExpr,
        op: LogicalAssign,
        target: hir::Expr,
        fx: &mut FnCtx,
        pos: Pos,
        prefix: &mut Option<&mut SyntheticPrefix>,
    ) -> hir::Expr {
        let mut storage = target.clone();
        self.restore_nullable_storage(&mut storage);
        let (mut first, mut again) = self.stabilize_logical_place(target, op, prefix, fx);
        first.ty = storage.ty.clone();
        again.ty = storage.ty.clone();
        if self.apparent_type(&storage.ty) == Type::Error {
            return self.err_expr(pos);
        }
        let (value, result_ty) = match op {
            LogicalAssign::Nullish => {
                let Some(inner) = self.require_nullable_operand(
                    &first,
                    "the left operand of `??=`",
                    RejectionSite::NullishAssignmentNonNullable,
                ) else {
                    return self.err_expr(pos);
                };
                let value = self.check_expr(&a.right, Some(&inner), fx);
                let result_ty = self
                    .nullish_assignment_type(&value, &inner, &storage.ty, fx)
                    .map(|result_ty| (inner, result_ty));
                (value, result_ty)
            }
            LogicalAssign::Or | LogicalAssign::And => {
                let Some(value) = self.check_logical_boolean_value(a, &first, &storage.ty, op, fx)
                else {
                    return self.err_expr(pos);
                };
                (value, Some((storage.ty.clone(), storage.ty.clone())))
            }
        };
        self.check_function_binding_assignment(&again, &a.right, &value, fx);
        let Some((inner, result_ty)) = result_ty else {
            return self.err_expr(pos);
        };
        if let Some(key) = path_key(&again) {
            let descendants = format!("{key}.");
            fx.ended_shared_narrowing
                .retain(|path| path != &key && !path.starts_with(&descendants));
            fx.narrowed
                .retain(|path| path != &key && !path.starts_with(&descendants));
        }
        let mut read = again.clone();
        read.ty = inner;
        let assigned = hir::Expr {
            pending_work: None,
            ty: value.ty.clone(),
            pos: pos.clone(),
            kind: ExprKind::Assign {
                update: None,
                op: None,
                target: Box::new(again),
                value: Box::new(value),
            },
        };
        // §199.1 rule 4: only the statement form narrows the place.
        if prefix.is_some() {
            self.end_shared_narrowing(&assigned, fx);
        } else {
            self.apply_operation_effects(&assigned, fx);
        }
        let (cond, then, els) = match op {
            LogicalAssign::Nullish => (self.null_test(first, pos.clone()), read, assigned),
            LogicalAssign::Or => (first, read, assigned),
            LogicalAssign::And => (first, assigned, read),
        };
        hir::Expr {
            pending_work: None,
            kind: ExprKind::Cond {
                cond: Box::new(cond),
                then: Box::new(then),
                els: Box::new(els),
            },
            ty: result_ty,
            pos,
        }
    }

    /// Returns the type of `p ??= v` (§199.1 rule 3), or `None` after a
    /// diagnostic.
    fn nullish_assignment_type(
        &mut self,
        value: &hir::Expr,
        inner: &Type,
        storage: &Type,
        fx: &FnCtx,
    ) -> Option<Type> {
        if self.apparent_type(&value.ty) == Type::Error {
            return None;
        }
        if self.assignable(&value.ty, inner) {
            return Some(inner.clone());
        }
        if self.assignable(&value.ty, storage) {
            return Some(storage.clone());
        }
        self.require_expr_assignable(value, storage, fx, "the assignment");
        None
    }

    /// Checks the right operand of `||=` or `&&=` against the type of the
    /// place, then requires a `boolean` left operand (§199.1 rules 3 and 6).
    /// A right operand that does not convert fails as `p = v` fails.
    fn check_logical_boolean_value(
        &mut self,
        a: &ast::AssignExpr,
        left: &hir::Expr,
        target_ty: &Type,
        op: LogicalAssign,
        fx: &mut FnCtx,
    ) -> Option<hir::Expr> {
        let value = self.check_expr(&a.right, Some(target_ty), fx);
        if self.apparent_type(&value.ty) == Type::Error {
            return None;
        }
        if !self.assignable(&value.ty, target_ty) {
            self.require_expr_assignable(&value, target_ty, fx, "the assignment");
            return None;
        }
        self.require_logical_boolean(left, op).then_some(value)
    }

    /// Requires a `boolean` operand of `||=` or `&&=` (§199.1 rule 6).
    fn require_logical_boolean(&mut self, operand: &hir::Expr, op: LogicalAssign) -> bool {
        if self.instance_restriction(
            crate::check::opaque::InstanceRestriction::BooleanContext,
            &operand.ty,
        ) || matches!(self.apparent_type(&operand.ty), Type::Bool)
        {
            return true;
        }
        if self.apparent_type(&operand.ty) != Type::Error {
            let name = self.type_name(&operand.ty);
            self.reject_subset(
                RejectionSite::LogicalAssignmentNonBoolean,
                format!(
                    "`{}` requires a boolean operand, got `{name}`",
                    op.spelling()
                ),
                operand.pos.clone(),
            );
        }
        false
    }

    /// Splits a place into its first evaluation and its later reads. A
    /// non-place receiver or index evaluates once (§82.1 rule 3).
    fn stabilize_logical_place(
        &mut self,
        target: hir::Expr,
        op: LogicalAssign,
        prefix: &mut Option<&mut SyntheticPrefix>,
        fx: &mut FnCtx,
    ) -> (hir::Expr, hir::Expr) {
        match &target.kind {
            ExprKind::Field { .. } | ExprKind::Index { .. } => {
                self.stabilize_projection(target, op, prefix, fx)
            }
            _ => (target.clone(), target),
        }
    }

    /// Stabilizes the operands of a field or an element projection. A
    /// value-class or `FixedArray` receiver is a projection too, so the
    /// write reaches the storage that the program names; only a
    /// reference base or an index is bound (§199.1 rule 2).
    fn stabilize_projection(
        &mut self,
        projection: hir::Expr,
        op: LogicalAssign,
        prefix: &mut Option<&mut SyntheticPrefix>,
        fx: &mut FnCtx,
    ) -> (hir::Expr, hir::Expr) {
        let mut first = projection.clone();
        let mut again = projection;
        match (&mut first.kind, &mut again.kind) {
            (ExprKind::Field { obj, .. }, ExprKind::Field { obj: later, .. }) => {
                let (head, tail) = self.stabilize_receiver((**obj).clone(), op, prefix, fx);
                **obj = head;
                **later = tail;
            }
            (
                ExprKind::Index { obj, index, .. },
                ExprKind::Index {
                    obj: later,
                    index: later_index,
                    ..
                },
            ) => {
                let (head, tail) = self.stabilize_receiver((**obj).clone(), op, prefix, fx);
                **obj = head;
                **later = tail;
                let (head, tail) =
                    self.stabilize_logical_operand((**index).clone(), "index", op, prefix, fx);
                **index = head;
                **later_index = tail;
            }
            _ => {}
        }
        (first, again)
    }

    /// Stabilizes a receiver. A value-class or `FixedArray` projection is
    /// never copied into a local: its own operands are stabilized instead.
    fn stabilize_receiver(
        &mut self,
        receiver: hir::Expr,
        op: LogicalAssign,
        prefix: &mut Option<&mut SyntheticPrefix>,
        fx: &mut FnCtx,
    ) -> (hir::Expr, hir::Expr) {
        let value_typed = self.is_value_class(&receiver.ty)
            || matches!(self.apparent_type(&receiver.ty), Type::FixedArray(..));
        if value_typed
            && !is_place_expr(&receiver)
            && matches!(
                receiver.kind,
                ExprKind::Field { .. } | ExprKind::Index { .. }
            )
        {
            return self.stabilize_projection(receiver, op, prefix, fx);
        }
        self.stabilize_logical_operand(receiver, "receiver", op, prefix, fx)
    }

    /// Binds a non-place operand to a synthetic local. A statement binds it
    /// in its prefix. A value-position use stores it at its first
    /// evaluation, so the evaluation order does not change.
    fn stabilize_logical_operand(
        &mut self,
        operand: hir::Expr,
        label: &str,
        op: LogicalAssign,
        prefix: &mut Option<&mut SyntheticPrefix>,
        fx: &mut FnCtx,
    ) -> (hir::Expr, hir::Expr) {
        if is_place_expr(&operand) {
            return (operand.clone(), operand);
        }
        if let Some(prefix) = prefix.as_deref_mut() {
            let local = self.stabilize_compound_operand(operand, label, prefix);
            return (local.clone(), local);
        }
        self.bind_synthetic_operand(operand, label, op.spelling(), fx)
    }

    /// Binds `operand` to a synthetic local that the owner prefix declares.
    /// Returns the store, which runs at the first evaluation, and the later
    /// read. The owner rejects the prefix in an initializer with no
    /// statement list, and the text names `operator` (§82.1 rule 3).
    pub(super) fn bind_synthetic_operand(
        &mut self,
        operand: hir::Expr,
        label: &str,
        operator: &'static str,
        fx: &mut FnCtx,
    ) -> (hir::Expr, hir::Expr) {
        let id = self.next_compound_local_id;
        self.next_compound_local_id += 1;
        let name = format!("[[compound#{id}.{label}]]");
        let ty = operand.ty.clone();
        let pos = operand.pos.clone();
        self.declare_in_context(
            &name,
            crate::check::Local {
                annotated: false,
                ty: ty.clone(),
                mutable: true,
                async_origins: std::collections::HashSet::new(),
                caught: false,
                function_value_required: None,
            },
            fx,
        );
        fx.push_logical_synthetic_prefix(
            hir::Stmt::Let {
                name: name.clone(),
                ty: ty.clone(),
                mutable: true,
                dispose: false,
                init: hir::Expr {
                    pending_work: None,
                    kind: ExprKind::Unassigned,
                    ty: ty.clone(),
                    pos: pos.clone(),
                },
                pos: pos.clone(),
            },
            operator,
        );
        let local = hir::Expr {
            pending_work: None,
            kind: ExprKind::Local(name, ty.clone(), false),
            ty: ty.clone(),
            pos: pos.clone(),
        };
        let stored = hir::Expr {
            pending_work: None,
            kind: ExprKind::Assign {
                update: None,
                op: None,
                target: Box::new(local.clone()),
                value: Box::new(operand),
            },
            ty,
            pos,
        };
        (stored, local)
    }

    /// Checks `p op= v` in statement position where `p` is an accessor or
    /// an index signature. The read runs once, and the write runs only
    /// when the assignment runs (§82.1 rules 1 and 4).
    fn check_logical_call_write(
        &mut self,
        a: &ast::AssignExpr,
        op: LogicalAssign,
        place: Place,
        fx: &mut FnCtx,
        pos: Pos,
        prefix: &mut SyntheticPrefix,
    ) -> Checked {
        let (read, write) = match place {
            Place::IndexSignature {
                receiver,
                index,
                signature,
                pos: target_pos,
            } => {
                if signature.readonly {
                    self.reject_subset(
                        RejectionSite::ReadonlyIndexAssignment,
                        "`a[i] = v` cannot write through a readonly index signature",
                        pos.clone(),
                    );
                    return Checked::Expr(self.err_expr(pos));
                }
                let receiver = self.stabilize_compound_operand(receiver, "receiver", prefix);
                let index = self.stabilize_compound_operand(index, "index", prefix);
                let parameter = signature.element_ty.clone();
                let read = Place::IndexSignature {
                    receiver: receiver.clone(),
                    index: index.clone(),
                    signature,
                    pos: target_pos,
                }
                .into_read(self);
                let write = (
                    Callee::Method {
                        recv: Box::new(receiver),
                        name: hir::Symbol::from_full_text("set"),
                    },
                    vec![index],
                    parameter,
                );
                (read, write)
            }
            Place::Accessor {
                class,
                receiver,
                name,
                ty,
                pos: target_pos,
            } => {
                let class_name = self.classes[class.0].name.clone();
                let write_name = format!("{name}=");
                let signature = if receiver.is_some() {
                    self.class_sigs[class.0].methods.get(&write_name).cloned()
                } else {
                    self.class_sigs[class.0]
                        .static_methods
                        .get(&write_name)
                        .cloned()
                };
                let target = if receiver.is_some() {
                    format!("x.{name}")
                } else {
                    format!("{class_name}.{name}")
                };
                let Some(signature) = signature else {
                    let spelling = write_spelling(WriteExpression::Assign(a), &target);
                    self.reject_subset(
                        if receiver.is_some() {
                            RejectionSite::ReadonlyAccessorAssignment
                        } else {
                            RejectionSite::ReadonlyStaticAccessorAssignment
                        },
                        format!("{spelling} cannot write through a read-only accessor"),
                        pos.clone(),
                    );
                    return Checked::Expr(self.err_expr(pos));
                };
                let Some(parameter) = signature.params.first() else {
                    let (site, message) = if receiver.is_some() {
                        (
                            RejectionSite::SetterParameterMissing,
                            format!("write accessor `{name}` has no parameter signature"),
                        )
                    } else {
                        (
                            RejectionSite::StaticSetterParameterMissing,
                            format!(
                                "static write accessor `{class_name}.{name}` has no parameter signature"
                            ),
                        )
                    };
                    self.reject_subset(site, message, pos.clone());
                    return Checked::Expr(self.err_expr(pos));
                };
                let parameter = parameter.ty().clone();
                let receiver = receiver
                    .map(|receiver| self.stabilize_compound_operand(receiver, "receiver", prefix));
                let read = Place::Accessor {
                    class,
                    receiver: receiver.clone(),
                    name: name.clone(),
                    ty,
                    pos: target_pos,
                }
                .into_read(self);
                let callee = match receiver {
                    Some(receiver) => Callee::Method {
                        recv: Box::new(receiver),
                        name: hir::Symbol::from_full_text(write_name),
                    },
                    None => Callee::Func(hir::Symbol::from_full_text(static_member_symbol(
                        class,
                        &class_name,
                        &write_name,
                    ))),
                };
                (read, (callee, Vec::new(), parameter))
            }
            _ => return Checked::Expr(self.err_expr(pos)),
        };
        if self.apparent_type(&read.ty) == Type::Error {
            return Checked::Expr(self.err_expr(pos));
        }
        let (callee, mut args, parameter) = write;
        let value = match op {
            LogicalAssign::Nullish => {
                let Some(inner) = self.require_nullable_operand(
                    &read,
                    "the left operand of `??=`",
                    RejectionSite::NullishAssignmentNonNullable,
                ) else {
                    return Checked::Expr(self.err_expr(pos));
                };
                let value = self.check_expr(&a.right, Some(&inner), fx);
                if self.apparent_type(&value.ty) != Type::Error {
                    self.require_expr_assignable(&value, &parameter, fx, "the assignment");
                }
                value
            }
            LogicalAssign::Or | LogicalAssign::And => {
                let Some(value) = self.check_logical_boolean_value(a, &read, &parameter, op, fx)
                else {
                    return Checked::Expr(self.err_expr(pos));
                };
                value
            }
        };
        args.push(value);
        let written = hir::Expr {
            pending_work: None,
            kind: ExprKind::Call { callee, args },
            ty: Type::Void,
            pos: pos.clone(),
        };
        self.end_shared_narrowing(&written, fx);
        let cond = match op {
            LogicalAssign::Nullish => hir::Expr {
                pending_work: None,
                kind: ExprKind::Binary {
                    op: BinOp::Eq,
                    left: Box::new(read),
                    right: Box::new(hir::Expr {
                        pending_work: None,
                        kind: ExprKind::Null,
                        ty: Type::Null,
                        pos: pos.clone(),
                    }),
                },
                ty: Type::Bool,
                pos: pos.clone(),
            },
            LogicalAssign::Or => hir::Expr {
                pending_work: None,
                kind: ExprKind::Unary {
                    op: hir::UnOp::Not,
                    operand: Box::new(read),
                },
                ty: Type::Bool,
                pos: pos.clone(),
            },
            LogicalAssign::And => read,
        };
        Checked::Stmt(hir::Stmt::If {
            cond,
            then: vec![hir::Stmt::Expr(written)],
            els: None,
            pos,
        })
    }
}

#[cfg(test)]
#[path = "logical_assign_tests.rs"]
mod tests;
