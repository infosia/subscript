//! Caller-side async defaults and failed handles (compiler.md §291 rules 3 and 3a).
use super::*;

pub(super) fn async_lambdas(module: &hir::Module) -> HashSet<hir::LambdaId> {
    fn visit(child: hir::HirChild<'_>, ids: &mut HashSet<hir::LambdaId>) {
        let children = match child {
            hir::HirChild::Expr(expr) => {
                if let hir::ExprKind::Lambda {
                    id, is_async: true, ..
                } = &expr.kind
                {
                    ids.insert(*id);
                }
                expr.children()
            }
            hir::HirChild::Stmt(stmt) => stmt.children(),
        };
        for child in children {
            visit(child, ids);
        }
    }
    let mut ids = HashSet::new();
    for owner in module.expression_owners() {
        match owner {
            hir::ExpressionOwner::Expr(expr) => visit(hir::HirChild::Expr(expr), &mut ids),
            hir::ExpressionOwner::Body { statements, .. } => {
                for stmt in statements {
                    visit(hir::HirChild::Stmt(stmt), &mut ids);
                }
            }
        }
    }
    ids
}

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    pub(super) fn needs_async_defaults(&self, params: &[CallParam], supplied: usize) -> bool {
        params.iter().skip(supplied).any(|p| p.default_can_raise)
    }

    fn default_error_type(&self, pos: &Pos) -> Result<Type, LowerError> {
        self.lowering
            .hir
            .classes
            .iter()
            .position(|c| c.name == "Error")
            .map(|index| Type::Class(ClassId(index)))
            .ok_or_else(|| self.error(pos, "async defaults require the Error class"))
    }

    /// Explicit arguments precede this region. Only defaults enter its handler.
    pub(super) fn begin_async_defaults(&mut self, pos: &Pos) -> Result<(), LowerError> {
        let binding_type = Some(l::ValueType::Data(self.default_error_type(pos)?));
        self.handlers.push(exception::HandlerFrame {
            binding_type,
            scope_depth: self.scopes.len(),
            landings: Vec::new(),
        });
        self.scopes.push(HashMap::new());
        Ok(())
    }

    /// The success edge carries the direct callee's handle. The failure edge
    /// creates a handle from an async helper that throws the caught Error.
    pub(super) fn finish_async_defaults(
        &mut self,
        boundary: Option<exception::HandlerFrame>,
        value: l::Operand,
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let Some(frame) = boundary else {
            return Ok(value);
        };
        let value = self
            .finish_input_holds(Some(value), &expr.ty, &expr.pos, true)?
            .ok_or_else(|| self.error(&expr.pos, "async defaults lost their handle"))?;
        let join = self.new_state_block(
            vec![l::ValueType::Data(expr.ty.clone())],
            Some("async-defaults.join".into()),
            &[],
        );
        let edge = self.block_target(join, vec![value])?;
        self.terminate(l::Terminator::Branch(edge), &expr.pos)?;
        let success_state = self.binding_snapshot();
        for (landing, snapshot) in frame.landings {
            self.restore_bindings(&snapshot);
            self.current = Some(landing);
            let error = self
                .emit(
                    l::InstructionKind::CatchEntry,
                    Vec::new(),
                    frame.binding_type.clone(),
                    false,
                    Vec::new(),
                    expr.pos.clone(),
                )?
                .ok_or_else(|| self.error(&expr.pos, "async defaults lost their exception"))?;
            let failed = self.failed_default_handle(error, expr)?;
            let edge = self.block_target(join, vec![failed])?;
            self.terminate(l::Terminator::Branch(edge), &expr.pos)?;
        }
        self.restore_bindings(&success_state);
        self.enter_block(join)?;
        let value = self.blocks[join.0 as usize].parameters[0];
        self.values[value.0 as usize].fresh_owner = true;
        Ok(l::Operand::Value(value))
    }

    /// Failed defaults resume through the same await boundary as the direct call.
    pub(super) fn finish_direct_async_defaults(
        &mut self,
        boundary: Option<exception::HandlerFrame>,
        successor: l::BlockId,
        resume_value: Option<l::ValueId>,
        expr: &hir::Expr,
    ) -> Result<(), LowerError> {
        let Some(frame) = boundary else {
            return Ok(());
        };
        let success_state = self.binding_snapshot();
        for (landing, snapshot) in frame.landings {
            self.restore_bindings(&snapshot);
            self.current = Some(landing);
            self.scopes.push(HashMap::new());
            let error = self
                .emit(
                    l::InstructionKind::CatchEntry,
                    Vec::new(),
                    frame.binding_type.clone(),
                    false,
                    Vec::new(),
                    expr.pos.clone(),
                )?
                .ok_or_else(|| self.error(&expr.pos, "async defaults lost their exception"))?;
            let mut handle_expr = expr.clone();
            handle_expr.ty = Type::async_handle(expr.ty.clone());
            let handle = self.failed_default_handle(error, &handle_expr)?;
            let handle = self
                .finish_input_holds(Some(handle), &handle_expr.ty, &expr.pos, true)?
                .ok_or_else(|| self.error(&expr.pos, "async defaults lost their handle"))?;
            let handle = self.terminator_value(handle, &expr.pos)?;
            let traps = vec![l::Trap {
                kind: l::TrapKind::DevReloadOnlyStaleCoroutine,
                pos: expr.pos.clone(),
            }];
            let edge = self.block_target(successor, Vec::new())?;
            self.terminate(
                l::Terminator::Suspend {
                    ownership: edge.ownership,
                    kind: l::SuspendKind::AsyncHandle {
                        handle,
                        owned: true,
                    },
                    pos: expr.pos.clone(),
                    successor,
                    resume_value,
                    arguments: edge.arguments,
                    invalidates: self.array_values.clone(),
                    traps,
                },
                &expr.pos,
            )?;
        }
        self.restore_bindings(&success_state);
        Ok(())
    }

    fn failed_default_handle(
        &mut self,
        error: l::Operand,
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let Type::AsyncHandle(result) = &expr.ty else {
            return Err(self.error(&expr.pos, "async defaults require a handle result"));
        };
        let result = (**result).clone();
        let error_type = self.default_error_type(&expr.pos)?;
        let id = if let Some(id) = self.lowering.failed_default_functions.get(&result) {
            *id
        } else {
            let id = self.lowering.allocate_function_id();
            self.lowering
                .failed_default_functions
                .insert(result.clone(), id);
            let pos = Pos::new(format!("<failed async default {result:?}>"), 1, 1);
            let mut parameter = expr.clone();
            parameter.pos = pos.clone();
            parameter.ty = error_type.clone();
            parameter.kind = hir::ExprKind::Local("error".into(), error_type.clone(), false);
            let input = FunctionInput {
                lambda_id: None,
                owned_environment: false,
                name: pos.file.clone(),
                exported: false,
                is_generator: false,
                is_async: true,
                creation_traps: vec![hir::TrapSite::Allocation { pos: pos.clone() }],
                host_entry_traps: None,
                can_raise: true,
                params: vec![hir::Param::new("error", error_type.clone(), pos.clone())],
                ret: result.clone(),
                body: vec![hir::Stmt::Throw {
                    value: parameter,
                    pos: pos.clone(),
                }],
                pos: pos.clone(),
            };
            self.lowering.lower_function_input(
                id,
                input,
                l::FunctionKind::SynthesizedHelper,
                None,
                Vec::new(),
            )?;
            id
        };
        let mut traps = self.read_lifetime(&l::ValueType::Data(error_type.clone()), &expr.pos);
        traps.push(l::Trap {
            kind: l::TrapKind::Call,
            pos: expr.pos.clone(),
        });
        self.emit_store_instruction(
            l::InstructionKind::AsyncHandleCreate(l::CallTarget {
                kind: l::CallTargetKind::Function(id),
                parameter_types: vec![l::ValueType::Data(error_type.clone())],
                return_type: (result != Type::Void).then_some(l::ValueType::Data(result)),
            }),
            vec![error],
            vec![StoredOperand {
                index: 0,
                ty: l::ValueType::Data(error_type),
                action: OwnerStoreAction::Acquire(hir::AsyncCopySite::CallArgument),
                pos: expr.pos.clone(),
            }],
            (Some(l::ValueType::Data(expr.ty.clone())), true),
            traps,
            expr.pos.clone(),
        )?
        .ok_or_else(|| self.error(&expr.pos, "async defaults produced no failed handle"))
    }
}
