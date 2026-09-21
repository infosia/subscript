//! Lowering for lambdas, async calls, and async handles.

use super::*;

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    pub(super) fn lower_lambda(
        &mut self,
        params: &[hir::Param],
        ret: &Type,
        body: &[hir::Stmt],
        captures: &[hir::Capture],
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        self.lower_lambda_with_id(params, ret, body, captures, expr)
            .map(|(_, closure)| closure)
    }

    pub(super) fn lower_lambda_with_id(
        &mut self,
        params: &[hir::Param],
        ret: &Type,
        body: &[hir::Stmt],
        captures: &[hir::Capture],
        expr: &hir::Expr,
    ) -> Result<(l::FunctionId, l::Operand), LowerError> {
        let capture_values = captures
            .iter()
            .map(|capture| {
                let binding = self.lookup_binding(&capture.name, &expr.pos)?;
                self.read_binding(binding, &expr.pos)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let id = self.lowering.allocate_function_id();
        let function = FunctionInput {
            name: format!(
                "<lambda {}:{}:{}>",
                expr.pos.file, expr.pos.line, expr.pos.col
            ),
            exported: false,
            is_generator: false,
            is_async: false,
            creation_traps: Vec::new(),
            host_entry_traps: None,
            params: params.to_vec(),
            ret: ret.clone(),
            body: body.to_vec(),
            pos: expr.pos.clone(),
        };
        self.lowering.lower_function_input(
            id,
            function,
            l::FunctionKind::Lambda,
            None,
            captures.to_vec(),
        )?;
        let closure = self
            .emit(
                l::InstructionKind::MakeClosure(id),
                capture_values,
                Some(l::ValueType::Data(expr.ty.clone())),
                false,
                convert_traps(&expr.trap_sites(self.lowering.hir)),
                expr.pos.clone(),
            )?
            .ok_or_else(|| self.error(&expr.pos, "lambda produced no function value"))?;
        Ok((id, closure))
    }

    fn resolve_async_target(
        &mut self,
        callee: &hir::AsyncCallee,
        args: &[hir::Expr],
        return_type: Option<l::ValueType>,
        pos: &Pos,
    ) -> Result<(l::CallTarget, Vec<l::Operand>, Vec<StoredOperand>), LowerError> {
        let (kind, mut operands, params) = match callee {
            hir::AsyncCallee::Function(name) => {
                let record = self
                    .lowering
                    .free_functions
                    .get(name)
                    .cloned()
                    .ok_or_else(|| self.error(pos, format!("unknown async function `{name}`")))?;
                let function = self
                    .lowering
                    .hir
                    .functions
                    .iter()
                    .find(|function| function.name == *name)
                    .cloned()
                    .ok_or_else(|| self.error(pos, "async function body is missing"))?;
                (
                    l::CallTargetKind::Function(record.id),
                    Vec::new(),
                    function
                        .params
                        .iter()
                        .map(CallParam::from)
                        .collect::<Vec<_>>(),
                )
            }
            hir::AsyncCallee::Method {
                class,
                receiver,
                name,
            } => {
                let record = self.lowering.method_record(class.0, name, pos)?;
                let function = self
                    .lowering
                    .hir
                    .classes
                    .get(class.0)
                    .and_then(|class| class.methods.iter().find(|method| method.name == *name))
                    .cloned()
                    .ok_or_else(|| self.error(pos, "async method body is missing"))?;
                (
                    l::CallTargetKind::Method(record.method.expect("async method id")),
                    vec![self.require_expr(receiver)?],
                    function
                        .params
                        .iter()
                        .map(CallParam::from)
                        .collect::<Vec<_>>(),
                )
            }
        };
        let mut parameter_types = if let hir::AsyncCallee::Method { class, .. } = callee {
            vec![l::ValueType::Data(Type::Class(*class))]
        } else {
            Vec::new()
        };
        parameter_types.extend(
            params
                .iter()
                .map(|parameter| l::ValueType::Data(parameter.ty.clone())),
        );
        let explicit_offset = operands.len();
        operands.extend(self.lower_call_arguments(&params, args, None, false)?);
        let target = l::CallTarget {
            kind,
            parameter_types,
            return_type,
        };
        let stored = params
            .iter()
            .enumerate()
            .map(|(index, parameter)| StoredOperand {
                index: explicit_offset + index,
                ty: l::ValueType::Data(parameter.ty.clone()),
                action: OwnerStoreAction::Acquire(hir::AsyncCopySite::CallArgument),
                pos: args
                    .get(index)
                    .map_or_else(|| parameter.pos.clone(), |argument| argument.pos.clone()),
            })
            .collect();
        Ok((target, operands, stored))
    }

    pub(super) fn lower_async_call(
        &mut self,
        callee: &hir::AsyncCallee,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        let return_type = (expr.ty != Type::Void).then(|| l::ValueType::Data(expr.ty.clone()));
        let (target, operands, stored) =
            self.resolve_async_target(callee, args, return_type.clone(), &expr.pos)?;
        self.acquire_stored_operands(&operands, stored)?;
        let typed_operands = operands
            .into_iter()
            .map(|operand| self.terminator_value(operand, &expr.pos))
            .collect::<Result<Vec<_>, _>>()?;
        let successor = self.new_block(
            return_type.clone().into_iter().collect(),
            Some("async-call.resume".to_string()),
        );
        let resume_value = return_type
            .as_ref()
            .map(|_| self.blocks[successor.0 as usize].parameters[0]);
        self.terminate(
            l::Terminator::Suspend {
                kind: l::SuspendKind::AsyncCall {
                    target,
                    operands: typed_operands,
                },
                pos: expr.pos.clone(),
                successor,
                resume_value,
                arguments: Vec::new(),
                invalidates: self.array_values.clone(),
                traps: convert_traps(&expr.trap_sites(self.lowering.hir)),
            },
            &expr.pos,
        )?;
        self.current = Some(successor);
        Ok(resume_value.map(l::Operand::Value))
    }

    pub(super) fn lower_async_handle_create(
        &mut self,
        callee: &hir::AsyncCallee,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        let Type::AsyncHandle(value) = &expr.ty else {
            return Err(self.error(&expr.pos, "async handle creation has a non-handle type"));
        };
        let return_type = (**value != Type::Void).then(|| l::ValueType::Data((**value).clone()));
        let (target, operands, stored) =
            self.resolve_async_target(callee, args, return_type, &expr.pos)?;
        self.emit_store_instruction(
            l::InstructionKind::AsyncHandleCreate(target),
            operands,
            stored,
            (Some(l::ValueType::Data(expr.ty.clone())), true),
            convert_traps(&expr.trap_sites(self.lowering.hir)),
            expr.pos.clone(),
        )?
        .ok_or_else(|| self.error(&expr.pos, "async handle creation produced no value"))
    }

    pub(super) fn lower_async_handle_await(
        &mut self,
        handle: &hir::Expr,
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        let handle = self.require_expr(handle)?;
        let handle = self.terminator_value(handle, &expr.pos)?;
        let return_type = (expr.ty != Type::Void).then(|| l::ValueType::Data(expr.ty.clone()));
        let successor = self.new_block(
            return_type.clone().into_iter().collect(),
            Some("async-handle.resume".to_string()),
        );
        let resume_value = return_type
            .as_ref()
            .map(|_| self.blocks[successor.0 as usize].parameters[0]);
        self.terminate(
            l::Terminator::Suspend {
                kind: l::SuspendKind::AsyncHandle { handle },
                pos: expr.pos.clone(),
                successor,
                resume_value,
                arguments: Vec::new(),
                invalidates: self.array_values.clone(),
                traps: convert_traps(&expr.trap_sites(self.lowering.hir)),
            },
            &expr.pos,
        )?;
        self.current = Some(successor);
        Ok(resume_value.map(l::Operand::Value))
    }
}
