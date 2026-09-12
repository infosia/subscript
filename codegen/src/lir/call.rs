//! Lowering for calls: target resolution, arguments, and argument defaults.

use super::*;

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    pub(super) fn lower_call(
        &mut self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<Option<l::Operand>, LowerError> {
        if let hir::Callee::Arr(operation) = callee {
            let static_operation = matches!(
                operation,
                hir::ArrFn::Map
                    | hir::ArrFn::Filter
                    | hir::ArrFn::Reduce
                    | hir::ArrFn::ReduceRight
                    | hir::ArrFn::ForEach
                    | hir::ArrFn::Some
                    | hir::ArrFn::Every
                    | hir::ArrFn::FindIndex
            );
            let dynamic_receiver = matches!(
                args.first().map(|argument| &argument.ty),
                Some(Type::Array(_))
            );
            let known_callback = args.get(1).is_some_and(|callback| {
                matches!(
                    callback.kind,
                    hir::ExprKind::FuncRef(_) | hir::ExprKind::Lambda { .. }
                )
            });
            if static_operation && dynamic_receiver && known_callback {
                return self.lower_static_array_callback(*operation, args, expr);
            }
        }
        if matches!(
            callee,
            hir::Callee::Map(hir::MapFn::ForEach) | hir::Callee::Set(hir::SetFn::ForEach)
        ) {
            return self.lower_for_each(callee, args, expr);
        }
        if matches!(callee, hir::Callee::Set(hir::SetFn::New)) && !args.is_empty() {
            return self.lower_set_from_source(args, expr);
        }
        if matches!(callee, hir::Callee::Ambient(hir::AmbientFn::Unreachable)) {
            let trap = convert_traps(&expr.trap_sites(self.lowering.hir))
                .into_iter()
                .find(|trap| trap.kind == l::TrapKind::Unreachable)
                .unwrap_or(l::Trap {
                    kind: l::TrapKind::Unreachable,
                    pos: expr.pos.clone(),
                });
            self.terminate(l::Terminator::Trap(trap), &expr.pos)?;
            return Ok(None);
        }

        let (declared_parameter_types, return_type) =
            self.declared_hir_call_signature(callee, args, expr)?;
        let (kind, mut operands, mut params, receiver_for_defaults) =
            self.resolve_call(callee, expr)?;
        if params.is_empty()
            && matches!(
                kind,
                l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_)
            )
        {
            params = self
                .operation_call_params(&kind, &operands, args, return_type.as_ref())?
                .unwrap_or_else(|| {
                    args.iter()
                        .enumerate()
                        .map(|(index, argument)| CallParam {
                            name: format!("arg{index}"),
                            ty: argument.ty.clone(),
                            default: None,
                            pos: argument.pos.clone(),
                        })
                        .collect()
                });
        }
        let foreign = matches!(kind, l::CallTargetKind::Foreign(_));
        let explicit_offset = operands.len();
        let explicit =
            self.lower_call_arguments(&params, args, receiver_for_defaults.as_ref(), foreign)?;
        operands.extend(explicit);
        if matches!(kind, l::CallTargetKind::Method(_)) {
            if let Some(PreparedBase::Place(place)) = receiver_for_defaults {
                operands[0] = self.materialize_address_inner(&place, &expr.pos, false)?;
            }
        }
        let deleted_field_owners =
            self.owners_destroyed_by_unsafe_delete(callee, args, &operands, explicit_offset)?;
        let table_signature = matches!(
            kind,
            l::CallTargetKind::Intrinsic(_) | l::CallTargetKind::BuiltinMethod(_)
        );
        let parameter_types = if foreign {
            operands
                .iter()
                .map(|operand| self.operand_type(operand, &expr.pos))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            declared_parameter_types
        };
        let target = l::CallTarget {
            kind,
            parameter_types: if table_signature {
                Vec::new()
            } else {
                parameter_types
            },
            return_type: return_type.clone(),
        };
        let stored = if foreign {
            Vec::new()
        } else {
            params
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
                .collect()
        };
        // Nullable boundary boxes are emitted while each argument is lowered;
        // keep their allocation sites on those instructions instead of also
        // attaching them to the eventual call.
        let call_traps = convert_traps(&expr.trap_sites(self.lowering.hir))
            .into_iter()
            .filter(|trap| trap.kind != l::TrapKind::Allocation)
            .collect();
        let result = self.emit_store_instruction(
            l::InstructionKind::Call(target),
            operands,
            stored,
            (return_type, true),
            call_traps,
            expr.pos.clone(),
        )?;
        for (owner, ty, pos) in deleted_field_owners {
            self.release_owner(owner, &ty, &pos)?;
        }
        Ok(result)
    }

    fn operation_call_params(
        &self,
        kind: &l::CallTargetKind,
        prefix: &[l::Operand],
        arguments: &[hir::Expr],
        return_type: Option<&l::ValueType>,
    ) -> Result<Option<Vec<CallParam>>, LowerError> {
        let target = match kind {
            l::CallTargetKind::Intrinsic(intrinsic) => {
                l::CallSignatureTarget::Intrinsic(intrinsic.clone())
            }
            l::CallTargetKind::BuiltinMethod(method) => {
                l::CallSignatureTarget::BuiltinMethod(*method)
            }
            _ => return Ok(None),
        };
        let prefix_types = prefix
            .iter()
            .map(|operand| {
                self.operand_type(
                    operand,
                    arguments
                        .first()
                        .map_or(&self.function.pos, |argument| &argument.pos),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let signature = self
            .lowering
            .hir
            .operation_signatures
            .iter()
            .map(lower_operation_signature)
            .find(|signature| {
                signature.target == target
                    && signature.return_type.as_ref() == return_type
                    && signature.parameter_types.len() == prefix.len() + arguments.len()
                    && signature.parameter_types[..prefix.len()] == prefix_types
                    && signature.parameter_types[prefix.len()..]
                        .iter()
                        .zip(arguments)
                        .all(|(expected, argument)| match expected {
                            l::ValueType::Data(expected) => {
                                expected == &argument.ty
                                    || self.is_boundary_box_narrowing(expected, &argument.ty)
                                    || self.embedded_header_extension(expected, argument).is_some()
                            }
                            l::ValueType::Address(_) | l::ValueType::Iterator(_) => false,
                        })
            });
        Ok(signature.map(|signature| {
            signature.parameter_types[prefix.len()..]
                .iter()
                .zip(arguments)
                .enumerate()
                .map(|(index, (parameter, argument))| CallParam {
                    name: format!("arg{index}"),
                    ty: match parameter {
                        l::ValueType::Data(ty) => ty.clone(),
                        l::ValueType::Address(_) | l::ValueType::Iterator(_) => argument.ty.clone(),
                    },
                    default: None,
                    pos: argument.pos.clone(),
                })
                .collect()
        }))
    }

    fn owners_destroyed_by_unsafe_delete(
        &mut self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        operands: &[l::Operand],
        explicit_offset: usize,
    ) -> Result<Vec<(l::Operand, l::ValueType, Pos)>, LowerError> {
        if !matches!(callee, hir::Callee::Ambient(hir::AmbientFn::UnsafeDelete)) {
            return Ok(Vec::new());
        }
        let Some(argument) = args.first() else {
            return Ok(Vec::new());
        };
        let Type::Class(class_id) = argument.ty else {
            return Ok(Vec::new());
        };
        let class = self
            .lowering
            .hir
            .classes
            .get(class_id.0)
            .ok_or_else(|| self.error(&argument.pos, "deleted class is missing"))?;
        if class.is_value {
            return Ok(Vec::new());
        }
        let fields = class
            .fields
            .iter()
            .filter(|field| is_async_owner_type(&l::ValueType::Data(field.ty.clone())))
            .map(|field| (field.name.clone(), field.ty.clone(), field.pos.clone()))
            .collect::<Vec<_>>();
        let base = operands
            .get(explicit_offset)
            .cloned()
            .ok_or_else(|| self.error(&argument.pos, "deleted class operand is missing"))?;
        let mut owners = Vec::with_capacity(fields.len());
        for (name, field_type, pos) in fields {
            let field = self
                .lowering
                .fields
                .get(&(class_id.0, name))
                .copied()
                .ok_or_else(|| self.error(&pos, "deleted class field is missing"))?;
            let ty = l::ValueType::Data(field_type);
            let owner = self
                .emit(
                    l::InstructionKind::LoadField(l::FieldRef::Class(field)),
                    vec![base.clone()],
                    Some(ty.clone()),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(&pos, "deleted class field produced no owner"))?;
            owners.push((owner, ty, pos));
        }
        Ok(owners)
    }

    fn declared_hir_call_signature(
        &self,
        callee: &hir::Callee,
        args: &[hir::Expr],
        expr: &hir::Expr,
    ) -> Result<(Vec<l::ValueType>, Option<l::ValueType>), LowerError> {
        let data_result = |ty: &Type| (*ty != Type::Void).then(|| l::ValueType::Data(ty.clone()));
        let data_params = |params: &[hir::Param]| {
            params
                .iter()
                .map(|parameter| l::ValueType::Data(parameter.ty.clone()))
                .collect::<Vec<_>>()
        };
        let foreign_params = |params: &[hir::Param]| {
            params
                .iter()
                .flat_map(|parameter| match &parameter.ty {
                    Type::Array(element) => vec![
                        l::ValueType::Address(l::AddressType {
                            pointee: (**element).clone(),
                            array_base: None,
                        }),
                        l::ValueType::Data(Type::I32),
                    ],
                    ty => vec![l::ValueType::Data(ty.clone())],
                })
                .collect::<Vec<_>>()
        };
        match callee {
            hir::Callee::Func(name) => {
                let function = self
                    .lowering
                    .hir
                    .functions
                    .iter()
                    .find(|function| function.name == *name)
                    .ok_or_else(|| self.error(&expr.pos, format!("missing body for `{name}`")))?;
                Ok((data_params(&function.params), data_result(&function.ret)))
            }
            hir::Callee::Foreign(name) => {
                let function = self
                    .lowering
                    .hir
                    .foreign_fns
                    .iter()
                    .find(|function| function.name == *name)
                    .ok_or_else(|| {
                        self.error(&expr.pos, format!("missing foreign declaration `{name}`"))
                    })?;
                Ok((foreign_params(&function.params), data_result(&function.ret)))
            }
            hir::Callee::Value(value) => {
                let Type::Func(signature) = &value.ty else {
                    return Err(self.error(&value.pos, "indirect callee is not function-typed"));
                };
                let mut parameters = vec![l::ValueType::Data(value.ty.clone())];
                parameters.extend(signature.params.iter().cloned().map(l::ValueType::Data));
                Ok((parameters, data_result(&signature.ret)))
            }
            hir::Callee::Method { recv, name } => {
                if let Type::Class(class_id) = recv.ty {
                    let class =
                        self.lowering.hir.classes.get(class_id.0).ok_or_else(|| {
                            self.error(&recv.pos, "method receiver class is missing")
                        })?;
                    let method = class
                        .methods
                        .iter()
                        .find(|method| method.name == *name)
                        .ok_or_else(|| self.error(&expr.pos, "method body is missing"))?;
                    let receiver = if class.is_value {
                        l::ValueType::Address(l::AddressType {
                            pointee: Type::Class(class_id),
                            array_base: None,
                        })
                    } else {
                        l::ValueType::Data(Type::Class(class_id))
                    };
                    let mut parameters = vec![receiver];
                    parameters.extend(data_params(&method.params));
                    return Ok((parameters, data_result(&method.ret)));
                }
                let mut parameters = vec![l::ValueType::Data(recv.ty.clone())];
                parameters.extend(
                    args.iter()
                        .map(|argument| l::ValueType::Data(argument.ty.clone())),
                );
                Ok((parameters, data_result(&expr.ty)))
            }
            _ => Ok((
                args.iter()
                    .map(|argument| l::ValueType::Data(argument.ty.clone()))
                    .collect(),
                data_result(&expr.ty),
            )),
        }
    }

    fn resolve_call(
        &mut self,
        callee: &hir::Callee,
        expr: &hir::Expr,
    ) -> Result<CallResolution, LowerError> {
        match callee {
            hir::Callee::Func(name) => {
                let record = self
                    .lowering
                    .free_functions
                    .get(name)
                    .cloned()
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown function `{name}`")))?;
                let function = self
                    .lowering
                    .hir
                    .functions
                    .iter()
                    .find(|function| function.name == *name)
                    .cloned()
                    .ok_or_else(|| self.error(&expr.pos, format!("missing body for `{name}`")))?;
                Ok((
                    l::CallTargetKind::Function(record.id),
                    Vec::new(),
                    function
                        .params
                        .iter()
                        .map(CallParam::from)
                        .collect::<Vec<_>>(),
                    None,
                ))
            }
            hir::Callee::Foreign(name) => {
                let id = self
                    .lowering
                    .foreign_functions
                    .get(name)
                    .copied()
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown foreign `{name}`")))?;
                let params = self
                    .lowering
                    .hir
                    .foreign_fns
                    .iter()
                    .find(|function| function.name == *name)
                    .map(|function| function.params.iter().map(CallParam::from).collect())
                    .ok_or_else(|| {
                        self.error(&expr.pos, format!("missing foreign declaration `{name}`"))
                    })?;
                Ok((l::CallTargetKind::Foreign(id), Vec::new(), params, None))
            }
            hir::Callee::Value(value) => {
                let callee_value = self.require_expr(value)?;
                let Type::Func(signature) = &value.ty else {
                    return Err(self.error(&value.pos, "indirect callee is not function-typed"));
                };
                let params = signature
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| CallParam {
                        name: format!("arg{index}"),
                        ty: ty.clone(),
                        default: None,
                        pos: value.pos.clone(),
                    })
                    .collect();
                Ok((
                    l::CallTargetKind::Indirect,
                    vec![callee_value],
                    params,
                    None,
                ))
            }
            hir::Callee::Method { recv, name } => {
                self.resolve_method_call(callee, recv, name, expr)
            }
            hir::Callee::Ambient(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Ambient,
                intrinsic_index(&hir::AmbientFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::ContextBytes { function, ty } => Ok(intrinsic_resolution(
                l::IntrinsicFamily::ContextBytes,
                intrinsic_index(&hir::ContextBytesFn::ALL, function),
                Some(ty.clone()),
                None,
            )),
            hir::Callee::Math(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Math,
                intrinsic_index(&hir::MathFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Num(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Number,
                intrinsic_index(&hir::NumFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Date(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Date,
                intrinsic_index(&hir::DateFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Json(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Json,
                intrinsic_index(&hir::JsonFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Str(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::String,
                intrinsic_index(&hir::StrFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Regex(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Regex,
                intrinsic_index(&hir::RegexFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Arr(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Array,
                intrinsic_index(&hir::ArrFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Map(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Map,
                intrinsic_index(&hir::MapFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Set(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Set,
                intrinsic_index(&hir::SetFn::ALL, value),
                None,
                None,
            )),
            hir::Callee::Worker(value) => Ok(intrinsic_resolution(
                l::IntrinsicFamily::Worker,
                intrinsic_index(&hir::WorkerFn::ALL, &value.intrinsic_identity()),
                None,
                match value {
                    hir::WorkerFn::Spawn(index) => Some(*index as u32),
                    _ => None,
                },
            )),
        }
    }

    fn resolve_method_call(
        &mut self,
        callee: &hir::Callee,
        recv: &hir::Expr,
        name: &str,
        expr: &hir::Expr,
    ) -> Result<CallResolution, LowerError> {
        if let Type::Class(class_id) = recv.ty {
            let class = self
                .lowering
                .hir
                .classes
                .get(class_id.0)
                .cloned()
                .ok_or_else(|| self.error(&recv.pos, "method receiver class is missing"))?;
            let record = self
                .lowering
                .methods
                .get(&(class_id.0, name.to_string()))
                .cloned()
                .ok_or_else(|| {
                    self.error(
                        &expr.pos,
                        format!("class `{}` has no resolved method `{name}`", class.name),
                    )
                })?;
            let method = class
                .methods
                .iter()
                .find(|method| method.name == name)
                .cloned()
                .ok_or_else(|| self.error(&expr.pos, "method body is missing"))?;
            let (receiver, prepared) = if class.is_value {
                if is_place_expr(recv) {
                    let place = self.prepare_place(recv)?;
                    let placeholder = self.materialize_address(&place, &recv.pos)?;
                    (placeholder, Some(PreparedBase::Place(Box::new(place))))
                } else {
                    let value = self.require_expr(recv)?;
                    let address = match self.operand_type(&value, &recv.pos)? {
                        l::ValueType::Address(_) => value,
                        l::ValueType::Data(Type::Class(id)) if id == class_id => self
                            .emit(
                                l::InstructionKind::AddressOfValue,
                                vec![value],
                                Some(l::ValueType::Address(l::AddressType {
                                    pointee: Type::Class(class_id),
                                    array_base: None,
                                })),
                                false,
                                Vec::new(),
                                recv.pos.clone(),
                            )?
                            .expect("temporary value-class address"),
                        other => {
                            return Err(self.error(
                                &recv.pos,
                                format!("value-class receiver has invalid LIR type {other:?}"),
                            ));
                        }
                    };
                    (address.clone(), Some(PreparedBase::Value(address)))
                }
            } else {
                (self.require_expr(recv)?, None)
            };
            return Ok((
                l::CallTargetKind::Method(record.method.expect("method id")),
                vec![receiver],
                method.params.iter().map(CallParam::from).collect(),
                prepared,
            ));
        }
        let Some((hir::OperationSignatureTarget::BuiltinMethod(method), _)) =
            hir::operation_signature_target(callee)
        else {
            return Err(self.error(
                &expr.pos,
                format!("unrepresented built-in method `{name}` on `{}`", recv.ty),
            ));
        };
        Ok((
            l::CallTargetKind::BuiltinMethod(lower_builtin_method(method)),
            vec![self.require_expr(recv)?],
            Vec::new(),
            None,
        ))
    }

    pub(super) fn lower_call_arguments(
        &mut self,
        params: &[CallParam],
        args: &[hir::Expr],
        receiver: Option<&PreparedBase>,
        foreign: bool,
    ) -> Result<Vec<l::Operand>, LowerError> {
        let mut pending = self.lower_explicit_arguments(params, args, foreign)?;
        self.lower_argument_defaults(params, args, receiver, foreign, &mut pending)?;
        self.finish_call_arguments(pending)
    }

    /// Lowers the explicit arguments left to right (compiler.md §57.1
    /// step 1). Every absent argument is trailing, so the parameters
    /// this step leaves out are the ones a default supplies.
    pub(super) fn lower_explicit_arguments(
        &mut self,
        params: &[CallParam],
        args: &[hir::Expr],
        foreign: bool,
    ) -> Result<PendingArguments, LowerError> {
        let mut pending = PendingArguments {
            values: Vec::with_capacity(params.len()),
            groups: vec![Vec::new(); params.len()],
            delayed_array_snapshots: Vec::new(),
        };
        for (index, parameter) in params.iter().enumerate() {
            let Some(argument) = args.get(index) else {
                break;
            };
            let value = if foreign {
                self.lower_foreign_argument_value(&parameter.ty, argument)?
            } else {
                self.lower_argument_value(&parameter.ty, argument)?
            };
            self.record_argument(
                index,
                parameter,
                Some(argument),
                value,
                foreign,
                &mut pending,
            )?;
        }
        if args.len() > params.len() {
            return Err(self.error(
                &args[params.len()].pos,
                format!(
                    "call has {} checked arguments but target has {} parameters",
                    args.len(),
                    params.len()
                ),
            ));
        }
        Ok(pending)
    }

    /// Lowers the default of every absent argument left to right, with
    /// `this` bound to `receiver` (compiler.md §57.1 step 4). A default
    /// reads the parameters before it, so the substitutions carry the
    /// values that are already lowered.
    pub(super) fn lower_argument_defaults(
        &mut self,
        params: &[CallParam],
        args: &[hir::Expr],
        receiver: Option<&PreparedBase>,
        foreign: bool,
        pending: &mut PendingArguments,
    ) -> Result<(), LowerError> {
        for (index, parameter) in params.iter().enumerate().skip(args.len()) {
            let default = parameter.default.as_ref().ok_or_else(|| {
                self.error(
                    &parameter.pos,
                    format!("missing argument `{}` with no default", parameter.name),
                )
            })?;
            let substitutions = params
                .iter()
                .zip(&pending.values)
                .map(|(parameter, value)| (parameter.name.clone(), value.clone()))
                .collect();
            self.substitutions.push(substitutions);
            let saved_this = self.this_value.clone();
            if let Some(receiver) = receiver {
                self.this_value = Some(match receiver {
                    PreparedBase::Value(value) => value.clone(),
                    PreparedBase::Place(place) => {
                        let address = self.materialize_address_inner(place, &default.pos, false)?;
                        self.emit(
                            l::InstructionKind::LoadAddress,
                            vec![address],
                            Some(l::ValueType::Data(self.place_type(place).clone())),
                            false,
                            Vec::new(),
                            default.pos.clone(),
                        )?
                        .expect("default receiver load")
                    }
                });
            }
            let lowered = self.lower_stored_expr_at(&parameter.ty, default, &parameter.pos);
            self.this_value = saved_this;
            self.substitutions.pop();
            let value = lowered?;
            self.record_argument(index, parameter, None, value, foreign, pending)?;
        }
        Ok(())
    }

    /// Coerces one lowered argument to its parameter type and keeps it
    /// at its parameter position.
    fn record_argument(
        &mut self,
        index: usize,
        parameter: &CallParam,
        argument: Option<&hir::Expr>,
        value: l::Operand,
        foreign: bool,
        pending: &mut PendingArguments,
    ) -> Result<(), LowerError> {
        let actual = self.operand_type(&value, &parameter.pos)?;
        let expected = l::ValueType::Data(parameter.ty.clone());
        let value = if actual == expected
            || foreign && self.foreign_boundary_pointer_representation(&parameter.ty, &actual)
        {
            value
        } else {
            self.coerce_operand(value, expected, &parameter.pos)?
        };
        pending.values.push(value.clone());
        if foreign {
            if let Type::Array(element) = &parameter.ty {
                let pos =
                    argument.map_or_else(|| parameter.pos.clone(), |argument| argument.pos.clone());
                pending
                    .delayed_array_snapshots
                    .push((index, value, (**element).clone(), pos));
                return Ok(());
            }
        }
        pending.groups[index] = vec![value];
        Ok(())
    }

    /// Takes the snapshot of every foreign array argument and flattens
    /// the parameters into the operands of the call.
    pub(super) fn finish_call_arguments(
        &mut self,
        mut pending: PendingArguments,
    ) -> Result<Vec<l::Operand>, LowerError> {
        for (index, value, element, pos) in std::mem::take(&mut pending.delayed_array_snapshots) {
            pending.groups[index] = self.foreign_array_snapshot(value, &element, pos)?.to_vec();
        }
        Ok(pending.groups.into_iter().flatten().collect())
    }

    fn foreign_array_snapshot(
        &mut self,
        value: l::Operand,
        element: &Type,
        pos: Pos,
    ) -> Result<[l::Operand; 2], LowerError> {
        let data = self
            .emit(
                l::InstructionKind::ForeignArrayData,
                vec![value.clone()],
                Some(l::ValueType::Address(l::AddressType {
                    pointee: element.clone(),
                    array_base: None,
                })),
                false,
                Vec::new(),
                pos.clone(),
            )?
            .expect("foreign array data snapshot");
        let count = self
            .emit(
                l::InstructionKind::Length,
                vec![value],
                Some(l::ValueType::Data(Type::I32)),
                false,
                Vec::new(),
                pos,
            )?
            .expect("foreign array count snapshot");
        Ok([data, count])
    }
}
