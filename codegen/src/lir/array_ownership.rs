//! Array calls borrow their inputs except the inserted unshift element.

use super::*;

pub(super) fn array_operation_name<'a>(
    operations: &'a [l::IntrinsicOperation],
    kind: &l::CallTargetKind,
) -> Option<&'a str> {
    let l::CallTargetKind::Intrinsic(intrinsic) = kind else {
        return None;
    };
    if intrinsic.family != l::IntrinsicFamily::Array {
        return None;
    }
    operations
        .iter()
        .find(|operation| {
            operation.family == intrinsic.family && operation.operation == intrinsic.operation
        })
        .map(|operation| operation.semantic_name.as_str())
}

pub(super) fn produces_fresh_owner(
    operations: &[l::IntrinsicOperation],
    kind: &l::InstructionKind,
) -> bool {
    if let l::InstructionKind::Call(target) = kind {
        if let Some(name) = array_operation_name(operations, &target.kind) {
            let class = hir::ArrFn::ALL
                .iter()
                .find(|operation| format!("{operation:?}") == name)
                .map(|operation| operation.counted_class());
            return !matches!(
                class,
                Some(
                    hir::CountedArrayMethod::Borrow
                        | hir::CountedArrayMethod::Replace
                        | hir::CountedArrayMethod::Reorder
                )
            ) && name != "Sort";
        }
    }
    kind.produces_fresh_async_owner()
}

impl FunctionBuilder<'_, '_> {
    pub(super) fn temporary_owner(&self, value: &l::Operand) -> bool {
        matches!(value, l::Operand::Value(id)
            if self.values.get(id.0 as usize).is_some_and(|value| value.fresh_owner && is_async_owner_type(&value.ty))
                && !self.moved_async_owners.contains(id))
    }

    pub(super) fn finish_temporary_read(
        &mut self,
        mut value: l::Operand,
        receiver: &l::Operand,
        ty: &Type,
        pos: &Pos,
        result_used: bool,
    ) -> Result<l::Operand, LowerError> {
        if self.temporary_owner(receiver) {
            if result_used {
                value = self.own_conditional_branch(value, ty, pos)?;
            }
            let receiver_type = self.operand_type(receiver, pos)?;
            self.discard_owner(
                hir::AsyncCopySite::DiscardedResult,
                receiver.clone(),
                &receiver_type,
                pos,
            )?;
            if let l::Operand::Value(id) = receiver {
                self.moved_async_owners.insert(*id);
            }
        }
        Ok(value)
    }

    pub(super) fn lower_read_expr(
        &mut self,
        expr: &hir::Expr,
        result_used: bool,
    ) -> Result<Option<l::Operand>, LowerError> {
        self.scopes.push(HashMap::new());
        let result = self.lower_read_with_holds(expr, result_used)?;
        self.finish_input_holds(result, &expr.ty, &expr.pos, result_used)
    }

    fn lower_read_with_holds(
        &mut self,
        expr: &hir::Expr,
        result_used: bool,
    ) -> Result<Option<l::Operand>, LowerError> {
        let (value, receiver) = match &expr.kind {
            hir::ExprKind::Field { obj, name } => {
                let receiver = self.require_expr(obj)?;
                let field = self.resolve_field(&obj.ty, name, &expr.pos)?;
                let stored = self.resolved_field_type(field, &obj.ty, &expr.pos)?;
                let value = self
                    .emit(
                        l::InstructionKind::LoadField(field),
                        vec![receiver.clone()],
                        Some(l::ValueType::Data(stored)),
                        false,
                        convert_traps(
                            &expr
                                .trap_sites_for_reload(self.lowering.hir, self.lowering.reload)
                                .into_iter()
                                .filter(|site| !matches!(site, hir::TrapSite::NullNarrowing { .. }))
                                .collect::<Vec<_>>(),
                        ),
                        expr.pos.clone(),
                    )?
                    .ok_or_else(|| self.error(&expr.pos, "field read has no value"))?;
                (self.coerce_shared_read(value, expr)?, Some(receiver))
            }
            hir::ExprKind::Length(subject) => {
                let receiver = self.require_expr(subject)?;
                let value = self
                    .emit(
                        l::InstructionKind::Length,
                        vec![receiver.clone()],
                        Some(l::ValueType::Data(expr.ty.clone())),
                        false,
                        convert_traps(
                            &expr.trap_sites_for_reload(self.lowering.hir, self.lowering.reload),
                        ),
                        expr.pos.clone(),
                    )?
                    .ok_or_else(|| self.error(&expr.pos, "length read has no value"))?;
                (value, Some(receiver))
            }
            hir::ExprKind::Index { .. } => {
                let place = self.prepare_place(expr)?;
                let value = self.load_place(&place, &expr.pos)?;
                let receiver = match &place.kind {
                    PreparedPlaceKind::Index {
                        base: PreparedBase::Value(receiver),
                        ..
                    } => Some(receiver.clone()),
                    _ => None,
                };
                (value, receiver)
            }
            _ => return Err(self.error(&expr.pos, "read needs a field, index, or length")),
        };
        let value = self.coerce_operand(value, l::ValueType::Data(expr.ty.clone()), &expr.pos)?;
        let value = if let Some(receiver) = receiver {
            self.finish_temporary_read(value, &receiver, &expr.ty, &expr.pos, result_used)?
        } else {
            value
        };
        Ok(Some(value))
    }
}

/// A script call or a suspension can change a previously read counted input.
pub(super) fn runs_user_code(expr: &hir::Expr) -> bool {
    if matches!(expr.kind, hir::ExprKind::Lambda { .. }) {
        return false;
    }
    matches!(
        expr.kind,
        hir::ExprKind::Call { .. }
            | hir::ExprKind::AsyncCall { .. }
            | hir::ExprKind::AsyncHandleCreate { .. }
            | hir::ExprKind::AsyncHandleAwait(_)
            | hir::ExprKind::AsyncSuspend
            | hir::ExprKind::New { .. }
            | hir::ExprKind::Yield(_)
    ) || expr.children().into_iter().any(|child| match child {
        hir::HirChild::Expr(child) => runs_user_code(child),
        hir::HirChild::Stmt(_) => false,
    })
}

impl FunctionBuilder<'_, '_> {
    pub(super) fn input_needs_hold(&self, expr: &hir::Expr) -> bool {
        if let hir::ExprKind::Local(name, _, _) = &expr.kind {
            if let Ok(binding) = self.lookup_binding(name, &expr.pos) {
                // An immutable local keeps its owner through the current expression.
                return self.bindings[binding.0].mutable;
            }
        }
        true
    }

    pub(super) fn hold_input(&mut self, value: &l::Operand, pos: &Pos) -> Result<(), LowerError> {
        let ty = self.operand_type(value, pos)?;
        if is_async_owner_type(&ty) {
            self.declare_binding(
                format!("<input hold {}>", self.bindings.len()),
                ty,
                false,
                value.clone(),
                pos.clone(),
                Some(hir::AsyncCopySite::Binding),
            )?;
        }
        Ok(())
    }

    pub(super) fn finish_input_holds(
        &mut self,
        mut result: Option<l::Operand>,
        ty: &Type,
        pos: &Pos,
        result_used: bool,
    ) -> Result<Option<l::Operand>, LowerError> {
        if self.scopes.last().is_some_and(|scope| !scope.is_empty()) {
            if result_used {
                if let Some(value) = result {
                    result = Some(self.own_conditional_branch(value, ty, pos)?);
                }
            }
            self.release_scopes_from(self.scopes.len() - 1, pos)?;
        }
        self.scopes.pop();
        Ok(result)
    }
}
