//! Generator close paths and their lexical finalizer check.
use super::verify::finding;
use super::*;

fn lexical_finalizers(body: &[hir::Stmt], yield_pos: &Pos) -> Vec<Pos> {
    fn walk(child: hir::HirChild<'_>, stack: &[Pos], pos: &Pos) -> Option<Vec<Pos>> {
        match child {
            hir::HirChild::Expr(expr) => {
                if matches!(expr.kind, hir::ExprKind::Yield(_)) && &expr.pos == pos {
                    return Some(stack.iter().rev().cloned().collect());
                }
                if matches!(expr.kind, hir::ExprKind::Lambda { .. }) {
                    return None;
                }
                expr.children()
                    .into_iter()
                    .find_map(|child| walk(child, stack, pos))
            }
            hir::HirChild::Stmt(stmt) => {
                if let hir::Stmt::Using {
                    body,
                    finalizer: Some(finalizer),
                    pos: frame_pos,
                    bindings: _,
                } = stmt
                {
                    let mut nested = stack.to_vec();
                    nested.push(frame_pos.clone());
                    return body
                        .iter()
                        .find_map(|stmt| walk(hir::HirChild::Stmt(stmt), &nested, pos))
                        .or_else(|| {
                            finalizer
                                .iter()
                                .find_map(|stmt| walk(hir::HirChild::Stmt(stmt), stack, pos))
                        });
                }
                stmt.children()
                    .into_iter()
                    .find_map(|child| walk(child, stack, pos))
            }
        }
    }
    body.iter()
        .find_map(|stmt| walk(hir::HirChild::Stmt(stmt), &[], yield_pos))
        .unwrap_or_default()
}

impl FunctionBuilder<'_, '_> {
    pub(super) fn lower_generator_close_dispatch(&mut self, pos: &Pos) -> Result<(), LowerError> {
        let suspension = self
            .generator_cleanup
            .last()
            .and_then(|state| state.suspension)
            .ok_or_else(|| self.error(pos, "generator yield has no state"))?;
        let closing = self
            .emit(
                l::InstructionKind::GeneratorIsClosing,
                Vec::new(),
                Some(l::ValueType::Data(Type::Bool)),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .ok_or_else(|| self.error(pos, "generator close flag has no value"))?;
        let close = self.new_state_block(Vec::new(), Some("generator.close".into()), &[]);
        let next = self.new_state_block(Vec::new(), Some("generator.next".into()), &[]);
        let then_target = self.block_target(close, Vec::new())?;
        let else_target = self.block_target(next, Vec::new())?;
        self.terminate(
            l::Terminator::ConditionalBranch {
                condition: closing,
                then_target,
                else_target,
            },
            pos,
        )?;
        self.generator_close.push(l::GeneratorClose::new(
            suspension,
            close,
            lexical_finalizers(&self.function.body, pos),
        ));
        let snapshot = self.binding_snapshot();
        self.enter_block(close)?;
        let prior = self
            .completion
            .replace((l::FinalizerCompletion::FallThrough, Vec::new()));
        self.exit_actions(0, 0, pos)?;
        self.completion = prior;
        if self.current.is_some() {
            self.terminate(
                l::Terminator::Return {
                    value: None,
                    pos: pos.clone(),
                },
                pos,
            )?;
        }
        self.restore_bindings(&snapshot);
        self.enter_block(next)
    }
}

pub(super) fn verify(function: &l::Function, errors: &mut Vec<VerifyError>) {
    if !function.is_generator {
        return;
    }
    for block in &function.blocks {
        if !matches!(
            block.terminator,
            l::Terminator::Suspend {
                kind: l::SuspendKind::Yield(_),
                ..
            }
        ) {
            continue;
        }
        let Some(state) = function
            .liveness
            .generator_close
            .iter()
            .find(|state| state.suspension == block.id)
        else {
            errors.push(finding(
                function,
                "generator suspension has no close continuation",
            ));
            continue;
        };
        let l::Terminator::Suspend { successor, .. } = &block.terminator else {
            continue;
        };
        let dispatch = function.blocks.get(successor.0 as usize);
        let valid_dispatch = dispatch.is_some_and(|dispatch| {
            matches!(&dispatch.terminator, l::Terminator::ConditionalBranch { condition: l::Operand::Value(value), then_target, .. }
                if then_target.block == state.continuation && dispatch.instructions.iter().any(|instruction|
                    instruction.result == Some(*value) && instruction.kind == l::InstructionKind::GeneratorIsClosing))
        });
        if !valid_dispatch {
            errors.push(finding(
                function,
                "generator suspension does not dispatch to its close continuation",
            ));
        }
        let mut work = vec![(state.continuation, 0usize)];
        let mut seen = HashSet::new();
        let mut missing = false;
        while let Some((id, mut index)) = work.pop() {
            if !seen.insert((id, index)) {
                continue;
            }
            let Some(block) = function.blocks.get(id.0 as usize) else {
                missing = true;
                continue;
            };
            for instruction in &block.instructions {
                if let l::InstructionKind::GeneratorFinalizer(pos) = &instruction.kind {
                    if state.finalizers.get(index) == Some(pos) {
                        index += 1;
                    } else if state.finalizers.contains(pos) {
                        missing = true;
                    }
                }
                if let Some(handler) = instruction.handler() {
                    work.push((handler, index));
                }
            }
            match &block.terminator {
                l::Terminator::Return { .. } => missing |= index != state.finalizers.len(),
                l::Terminator::Suspend { .. } => {
                    let nested = function
                        .liveness
                        .generator_close
                        .iter()
                        .find(|next| next.suspension == block.id);
                    missing |= nested.is_none_or(|next| {
                        !state.finalizers[index..]
                            .iter()
                            .all(|pos| next.finalizers.contains(pos))
                    });
                }
                _ => work.extend(
                    block
                        .terminator
                        .successors()
                        .into_iter()
                        .map(|id| (id, index)),
                ),
            }
        }
        if missing {
            errors.push(finding(function, "generator close continuation omits enclosing finalizer or changes its lexical order"));
        }
    }
}
