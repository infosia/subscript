//! Function-builder core: blocks, values, locals, bindings, owners, and scopes.

use super::address_taken::address_taken_bindings;
use super::*;

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    pub(super) fn new(
        lowering: &'a mut Lowering<'m>,
        id: l::FunctionId,
        function: FunctionInput,
        kind: l::FunctionKind,
        receiver: Option<ClassId>,
        captures: Vec<hir::Capture>,
    ) -> Result<Self, LowerError> {
        let address_taken =
            address_taken_bindings(lowering.hir, &lowering.classes, &function, &captures);
        let mut builder = Self {
            lowering,
            function,
            id,
            kind,
            parameters: Vec::new(),
            locals: Vec::new(),
            values: Vec::new(),
            blocks: Vec::new(),
            entry: l::BlockId(0),
            current: None,
            scopes: vec![HashMap::new()],
            bindings: Vec::new(),
            address_taken,
            substitutions: Vec::new(),
            this_value: None,
            controls: Vec::new(),
            array_values: Vec::new(),
            moved_async_owners: HashSet::new(),
            handlers: Vec::new(),
            usings: Vec::new(),
            exit_return: None,
            exit_return_depth: 0,
            finalizer_scope_depth: None,
            finalizer_exception_active: false,
            completion: None,
            generator_cleanup: Vec::new(),
            generator_close: Vec::new(),
        };
        let entry = builder.new_block(Vec::new(), Some("entry".to_string()));
        builder.entry = entry;
        builder.current = Some(entry);

        if let Some(class_id) = receiver {
            let class = builder
                .lowering
                .hir
                .classes
                .get(class_id.0)
                .ok_or_else(|| builder.error(&builder.function.pos, "receiver class is missing"))?;
            let ty = if class.is_value {
                l::ValueType::Address(l::AddressType {
                    pointee: Type::Class(class_id),
                    array_base: None,
                })
            } else {
                l::ValueType::Data(Type::Class(class_id))
            };
            let operand = builder.add_parameter(
                "this".to_string(),
                ty,
                l::ParameterKind::Receiver,
                builder.function.pos.clone(),
            )?;
            builder.this_value = Some(operand);
        }
        let mut captures = captures.into_iter();
        if builder.function.owned_environment {
            if let Some(environment) = captures.next() {
                let Type::Class(class) = environment.ty else {
                    return Err(builder.error(
                        &builder.function.pos,
                        "an owned capture has no environment object",
                    ));
                };
                let pos = builder.function.pos.clone();
                let object = builder.add_parameter(
                    environment.name,
                    l::ValueType::Data(Type::Class(class)),
                    l::ParameterKind::OwnedEnvironment,
                    pos.clone(),
                )?;
                for (index, capture) in captures.enumerate() {
                    let field = builder.lowering.classes[class.0].fields[index].id;
                    let address = builder
                        .emit(
                            l::InstructionKind::AddressOfField(l::FieldRef::Class(field)),
                            vec![object.clone()],
                            Some(l::ValueType::Address(l::AddressType {
                                pointee: capture.ty.clone(),
                                array_base: None,
                            })),
                            false,
                            Vec::new(),
                            pos.clone(),
                        )?
                        .ok_or_else(|| builder.error(&pos, "capture field has no address"))?;
                    let value = builder
                        .emit(
                            l::InstructionKind::LoadAddress,
                            vec![address],
                            Some(l::ValueType::Data(capture.ty.clone())),
                            false,
                            Vec::new(),
                            pos.clone(),
                        )?
                        .ok_or_else(|| builder.error(&pos, "capture field has no value"))?;
                    builder.declare_binding(
                        capture.name,
                        l::ValueType::Data(capture.ty),
                        false,
                        value,
                        pos.clone(),
                        Some(hir::AsyncCopySite::Binding),
                    )?;
                }
            }
        } else {
            for capture in captures {
                builder.add_parameter(
                    capture.name,
                    l::ValueType::Data(capture.ty),
                    l::ParameterKind::Capture,
                    builder.function.pos.clone(),
                )?;
            }
        }
        for parameter in builder.function.params.clone() {
            builder.add_parameter(
                parameter.name,
                l::ValueType::Data(parameter.ty),
                l::ParameterKind::Explicit,
                parameter.pos,
            )?;
        }
        if builder.function.is_generator {
            builder.generator_cleanup.push(l::GeneratorCleanup::new(
                None,
                builder
                    .parameters
                    .iter()
                    .filter(|p| {
                        p.kind != l::ParameterKind::Capture
                            && is_async_owner_type(&builder.values[p.value.0 as usize].ty)
                    })
                    .map(|p| p.value)
                    .collect(),
            ));
        }
        Ok(builder)
    }

    /// Reachability follows normal edges and instruction handler edges.
    pub(super) fn block_reachable(&self, target: l::BlockId) -> bool {
        let mut pending = vec![self.entry];
        let mut seen = HashSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            if id == target {
                return true;
            }
            let block = &self.blocks[id.0 as usize];
            pending.extend(
                block
                    .instructions
                    .iter()
                    .filter_map(l::Instruction::handler),
            );
            if let Some(terminator) = &block.terminator {
                pending.extend(terminator.successors());
            }
        }
        false
    }

    pub(super) fn finish(mut self) -> Result<l::Function, LowerError> {
        if !self.usings_closed() {
            return Err(self.error(&self.function.pos, "a `using` node is still open"));
        }
        let cfg_fallthrough = self
            .current
            .is_some_and(|block| self.block_reachable(block));
        let body_fallthrough = subscript_compiler::sequence_can_fall_through(&self.function.body);
        if cfg_fallthrough && !body_fallthrough {
            return Err(self.error(
                &self.function.pos,
                format!(
                    "function `{}` exit facts differ: CFG fallthrough = {}, body fallthrough = {}",
                    self.function.name, cfg_fallthrough, body_fallthrough,
                ),
            ));
        }
        if !cfg_fallthrough {
            self.current = None;
        }
        if let Some(block) = self.current {
            if self.blocks[block.0 as usize].terminator.is_none() {
                if self.function.ret == Type::Void || self.function.is_generator {
                    let pos = self.function.pos.clone();
                    self.exit_actions(0, 0, &pos)?;
                    self.blocks[block.0 as usize].terminator = Some(l::Terminator::Return {
                        value: None,
                        pos: self.function.pos.clone(),
                    });
                } else {
                    return Err(self.error(
                        &self.function.pos,
                        "non-void function has a reachable fallthrough",
                    ));
                }
            }
        }
        for block in &mut self.blocks {
            if block.terminator.is_none() {
                block.terminator = Some(l::Terminator::Unreachable {
                    pos: self.function.pos.clone(),
                });
            }
        }
        let function = l::Function {
            id: self.id,
            source_name: self.function.name,
            kind: self.kind,
            exported: self.function.exported,
            is_generator: self.function.is_generator,
            is_async: self.function.is_async,
            creation_traps: convert_traps(&self.function.creation_traps),
            host_entry_traps: self.function.host_entry_traps.as_deref().map(convert_traps),
            can_raise: self.function.can_raise || self.lowering.reload,
            parameters: self.parameters,
            return_type: self.function.ret,
            locals: self.locals,
            values: self.values,
            liveness: l::Liveness {
                generator_cleanup: self.generator_cleanup,
                generator_close: self.generator_close,
                ..Default::default()
            },
            blocks: self
                .blocks
                .into_iter()
                .map(|block| l::BasicBlock {
                    id: block.id,
                    source_name: block.source_name,
                    parameters: block.parameters,
                    instructions: block.instructions,
                    terminator: block.terminator.expect("terminator filled"),
                })
                .collect(),
            entry: self.entry,
            pos: self.function.pos,
        };
        Ok(function)
    }

    pub(super) fn error(&self, pos: &Pos, message: impl Into<String>) -> LowerError {
        LowerError {
            pos: pos.clone(),
            message: message.into(),
        }
    }

    pub(super) fn new_block(
        &mut self,
        parameter_types: Vec<l::ValueType>,
        source_name: Option<String>,
    ) -> l::BlockId {
        let id = l::BlockId(self.blocks.len() as u32);
        let parameters = parameter_types
            .into_iter()
            .map(|ty| self.new_value(ty, None))
            .collect();
        self.blocks.push(BlockDraft {
            id,
            source_name,
            parameters,
            state_bindings: Vec::new(),
            instructions: Vec::new(),
            terminator: None,
        });
        id
    }

    pub(super) fn new_state_block(
        &mut self,
        prefix_types: Vec<l::ValueType>,
        source_name: Option<String>,
        forced: &[BindingId],
    ) -> l::BlockId {
        let mut state_bindings = self.visible_mutable_bindings();
        if self.function.is_generator {
            state_bindings.extend(
                self.scopes
                    .iter()
                    .flat_map(|scope| scope.values().copied())
                    .filter(|binding| is_async_owner_type(&self.bindings[binding.0].ty)),
            );
        }
        state_bindings.extend(forced.iter().copied());
        state_bindings.sort_unstable();
        state_bindings.dedup();
        state_bindings.retain(|binding| {
            self.bindings
                .get(binding.0)
                .is_some_and(|binding| binding.storage.is_none())
        });
        let mut parameter_types = prefix_types;
        parameter_types.extend(
            state_bindings
                .iter()
                .map(|binding| self.bindings[binding.0].ty.clone()),
        );
        let block = self.new_block(parameter_types, source_name);
        self.blocks[block.0 as usize].state_bindings = state_bindings;
        block
    }

    fn new_value(&mut self, ty: l::ValueType, source_name: Option<String>) -> l::ValueId {
        let id = l::ValueId(self.values.len() as u32);
        if matches!(&ty, l::ValueType::Data(Type::Array(_))) {
            self.array_values.push(id);
        }
        self.values.push(l::Value {
            id,
            ty,
            fresh_owner: false,
            source_name,
        });
        id
    }

    pub(super) fn add_local(
        &mut self,
        source_name: String,
        ty: l::ValueType,
        mutable: bool,
        pos: Pos,
    ) -> Result<l::LocalId, LowerError> {
        let id = l::LocalId(self.locals.len() as u32);
        self.locals.push(l::Local {
            id,
            source_name: source_name.clone(),
            ty,
            mutable,
            storage: l::LocalStorageClass::Activation,
            pos: pos.clone(),
        });
        Ok(id)
    }

    pub(super) fn declare_binding(
        &mut self,
        source_name: String,
        ty: l::ValueType,
        mutable: bool,
        value: l::Operand,
        pos: Pos,
        copy_site: Option<hir::AsyncCopySite>,
    ) -> Result<(BindingId, Option<l::LocalId>), LowerError> {
        let storage = if self
            .address_taken
            .contains(&BindingSite::new(&source_name, &pos))
        {
            Some(self.add_local(source_name.clone(), ty.clone(), mutable, pos.clone())?)
        } else {
            None
        };
        if let Some(local) = storage {
            self.emit_store_instruction(
                l::InstructionKind::StoreLocal(local),
                vec![value.clone()],
                vec![StoredOperand {
                    index: 0,
                    ty: ty.clone(),
                    action: copy_site.map_or(OwnerStoreAction::Move, OwnerStoreAction::Acquire),
                    pos: pos.clone(),
                }],
                (None, false),
                Vec::new(),
                pos.clone(),
            )?;
        } else if let Some(copy_site) = copy_site {
            self.acquire_owner(copy_site, &value, &ty, &pos)?;
        }
        let id = BindingId(self.bindings.len());
        self.bindings.push(Binding {
            source_name: source_name.clone(),
            ty,
            mutable,
            storage,
            value: storage.is_none().then_some(value),
        });
        let scope = self.scopes.last_mut().expect("one binding scope");
        if scope.insert(source_name.clone(), id).is_some() {
            return Err(self.error(
                &pos,
                format!("duplicate local `{source_name}` in one scope"),
            ));
        }
        Ok((id, storage))
    }

    pub(super) fn declare_hidden_binding(
        &mut self,
        source_name: &str,
        ty: l::ValueType,
        value: l::Operand,
    ) -> BindingId {
        let id = BindingId(self.bindings.len());
        self.bindings.push(Binding {
            source_name: source_name.to_string(),
            ty,
            mutable: true,
            storage: None,
            value: Some(value),
        });
        id
    }

    fn add_parameter(
        &mut self,
        source_name: String,
        ty: l::ValueType,
        kind: l::ParameterKind,
        pos: Pos,
    ) -> Result<l::Operand, LowerError> {
        let value = self.new_value(ty.clone(), Some(source_name.clone()));
        let operand = l::Operand::Value(value);
        let (_, storage) = self.declare_binding(
            source_name.clone(),
            ty.clone(),
            true,
            operand.clone(),
            pos.clone(),
            None,
        )?;
        self.parameters.push(l::Parameter {
            storage,
            value,
            source_name,
            kind,
            pos,
        });
        Ok(operand)
    }

    pub(super) fn read_lifetime(&self, ty: &l::ValueType, pos: &Pos) -> Vec<l::Trap> {
        let l::ValueType::Data(ty) = ty else {
            return Vec::new();
        };
        if ty
            .handle_kind(&self.lowering.handle_classes)
            .is_some_and(subscript_compiler::types::HandleKind::needs_lifetime_trap)
        {
            vec![l::Trap {
                kind: l::TrapKind::DevOnlyLifetime(0),
                pos: pos.clone(),
            }]
        } else {
            Vec::new()
        }
    }

    pub(super) fn emit(
        &mut self,
        kind: l::InstructionKind,
        operands: Vec<l::Operand>,
        result_type: Option<l::ValueType>,
        invalidates_arrays: bool,
        mut traps: Vec<l::Trap>,
        pos: Pos,
    ) -> Result<Option<l::Operand>, LowerError> {
        let block = self.current.ok_or_else(|| {
            self.error(&pos, "attempted to emit an instruction after a terminator")
        })?;
        self.resolve_raise_edges(&mut traps)?;
        let result = result_type
            .as_ref()
            .map(|ty| self.new_value(ty.clone(), None));
        let fresh = if matches!(
            &kind,
            l::InstructionKind::Call(l::CallTarget {
                kind: l::CallTargetKind::Intrinsic(l::Intrinsic {
                    family: l::IntrinsicFamily::Array | l::IntrinsicFamily::Map,
                    ..
                }),
                ..
            })
        ) {
            array_ownership::produces_fresh_owner_indexed(&kind)
        } else {
            kind.produces_fresh_async_owner()
        };
        if fresh && result_type.as_ref().is_some_and(is_async_owner_type) {
            if let Some(value) = result {
                self.values[value.0 as usize].fresh_owner = true;
            }
        }
        let invalidates = if invalidates_arrays {
            self.array_values.clone()
        } else {
            Vec::new()
        };
        let action_type = match &kind {
            l::InstructionKind::ArraySpreadLiteral(_) => match result_type.as_ref() {
                Some(l::ValueType::Data(Type::Array(element))) => Some((**element).clone()),
                _ => None,
            },
            l::InstructionKind::Call(target)
                if matches!(
                    array_ownership::array_operation_name(
                        operation_table::lookup(&target.kind),
                        &target.kind
                    ),
                    Some("Fill" | "CopyWithin" | "Slice" | "Concat")
                ) || matches!(
                    target.kind,
                    l::CallTargetKind::BuiltinMethod(l::BuiltinMethod::ArrayClear)
                ) =>
            {
                operands
                    .first()
                    .map(|operand| self.operand_type(operand, &pos))
                    .transpose()?
                    .and_then(|ty| match ty {
                        l::ValueType::Data(Type::Array(element) | Type::FixedArray(element, _)) => {
                            Some(*element)
                        }
                        _ => None,
                    })
            }
            l::InstructionKind::MapFromSource => result_type.as_ref().and_then(|ty| match ty {
                l::ValueType::Data(Type::Map(_, value)) => Some((**value).clone()),
                _ => None,
            }),
            l::InstructionKind::Call(target)
                if array_ownership::map_operation_name(
                    operation_table::lookup(&target.kind),
                    &target.kind,
                )
                .is_some() =>
            {
                operands
                    .first()
                    .and_then(|operand| self.operand_type(operand, &pos).ok())
                    .or_else(|| result_type.as_ref().cloned())
                    .and_then(|ty| match ty {
                        l::ValueType::Data(Type::Map(_, value)) => Some(*value),
                        _ => None,
                    })
            }
            _ => None,
        };
        let count_action = action_type.as_ref().map(l::CountAction::for_type);
        if count_action
            .as_ref()
            .is_some_and(|action| action.release_type().is_some())
            && !traps.iter().any(|trap| trap.kind == l::TrapKind::Call)
        {
            traps.push(l::Trap {
                kind: l::TrapKind::Call,
                pos: pos.clone(),
            });
        }
        self.blocks[block.0 as usize]
            .instructions
            .push(l::Instruction {
                result,
                kind,
                count_action,
                operands,
                invalidates,
                traps,
                pos,
            });
        Ok(result.map(l::Operand::Value))
    }

    pub(super) fn emit_store_instruction(
        &mut self,
        kind: l::InstructionKind,
        operands: Vec<l::Operand>,
        stored: Vec<StoredOperand>,
        result: (Option<l::ValueType>, bool),
        traps: Vec<l::Trap>,
        pos: Pos,
    ) -> Result<Option<l::Operand>, LowerError> {
        self.acquire_stored_operands(&operands, stored)?;
        let (result_type, invalidates_arrays) = result;
        self.emit(kind, operands, result_type, invalidates_arrays, traps, pos)
    }

    pub(super) fn acquire_stored_operands(
        &mut self,
        operands: &[l::Operand],
        stored: Vec<StoredOperand>,
    ) -> Result<(), LowerError> {
        for store in stored {
            let value = operands.get(store.index).ok_or_else(|| {
                self.error(
                    &store.pos,
                    format!("store operand {} is missing", store.index),
                )
            })?;
            match store.action {
                OwnerStoreAction::Acquire(site) => {
                    self.acquire_owner(site, value, &store.ty, &store.pos)?;
                }
                OwnerStoreAction::Move => {}
            }
        }
        Ok(())
    }

    pub(super) fn terminate(
        &mut self,
        mut terminator: l::Terminator,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        if self.function.is_generator && matches!(terminator, l::Terminator::Suspend { .. }) {
            let block = self
                .current
                .ok_or_else(|| self.error(pos, "suspension has no block"))?;
            let mut bindings = self
                .scopes
                .iter()
                .flat_map(|s| s.values().copied())
                .collect::<Vec<_>>();
            bindings.sort_unstable();
            bindings.dedup();
            let mut owners = Vec::new();
            for binding in bindings {
                if is_async_owner_type(&self.bindings[binding.0].ty) {
                    let owner = self.read_binding(binding, pos)?;
                    if let l::Operand::Value(value) = owner {
                        owners.push(value);
                    }
                }
            }
            if let l::Terminator::Suspend {
                successor,
                ownership,
                arguments,
                ..
            } = &mut terminator
            {
                ownership.resize(arguments.len(), false);
                for value in &owners {
                    let operand = l::Operand::Value(*value);
                    if !arguments.contains(&operand) {
                        let ty = self.values[value.0 as usize].ty.clone();
                        let parameter = self.new_value(ty, None);
                        self.blocks[successor.0 as usize].parameters.push(parameter);
                        arguments.push(operand);
                        ownership.push(false);
                    }
                }
            }
            self.generator_cleanup
                .push(l::GeneratorCleanup::new(Some(block), owners));
        }
        let block = self
            .current
            .take()
            .ok_or_else(|| self.error(pos, "block already has a terminator"))?;
        let draft = &mut self.blocks[block.0 as usize];
        if draft.terminator.replace(terminator).is_some() {
            return Err(self.error(pos, format!("block {} has two terminators", block.0)));
        }
        Ok(())
    }

    pub(super) fn terminate_return(
        &mut self,
        value: Option<l::Operand>,
        ty: l::ValueType,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let crosses_finalizer = self.finalizers_pending();
        let return_type = ty.clone();
        let prior_return = self.exit_return.take();
        if let Some(value) = &value {
            self.acquire_owner(hir::AsyncCopySite::Return, value, &ty, pos)?;
        }
        if let Some((held, held_ty)) = &prior_return {
            self.release_owner(held.clone(), held_ty, pos)?;
        }
        self.exit_return = value
            .as_ref()
            .filter(|_| is_async_owner_type(&ty))
            .map(|value| (value.clone(), ty));
        let prior_completion = self.completion.replace((
            l::FinalizerCompletion::Return,
            value.clone().into_iter().collect(),
        ));
        let prior_exception = std::mem::replace(&mut self.finalizer_exception_active, false);
        let actions = self.exit_actions(0, 0, pos);
        self.finalizer_exception_active = prior_exception;
        self.completion = prior_completion;
        self.exit_return = prior_return;
        actions?;
        if self.current.is_none() {
            return Ok(());
        }
        if crosses_finalizer && is_async_owner_type(&return_type) {
            if let Some(value) = &value {
                // Transfer the held completion owner at the final return block.
                self.acquire_owner(hir::AsyncCopySite::Return, value, &return_type, pos)?;
                self.release_owner(value.clone(), &return_type, pos)?;
            }
        }
        self.terminate(
            l::Terminator::Return {
                value,
                pos: pos.clone(),
            },
            pos,
        )
    }

    pub(super) fn operand_type(
        &self,
        operand: &l::Operand,
        pos: &Pos,
    ) -> Result<l::ValueType, LowerError> {
        match operand {
            l::Operand::Value(id) => self
                .values
                .get(id.0 as usize)
                .map(|value| value.ty.clone())
                .ok_or_else(|| self.error(pos, format!("value {} is not declared", id.0))),
            l::Operand::Constant(constant) => Ok(l::ValueType::Data(constant.ty.clone())),
        }
    }

    pub(super) fn acquire_owner(
        &mut self,
        site: hir::AsyncCopySite,
        value: &l::Operand,
        ty: &l::ValueType,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        match site {
            hir::AsyncCopySite::Binding
            | hir::AsyncCopySite::Assignment
            | hir::AsyncCopySite::ArrayElement
            | hir::AsyncCopySite::SpreadElement
            | hir::AsyncCopySite::CallArgument
            | hir::AsyncCopySite::Return
            | hir::AsyncCopySite::ForOfBinding => {}
            hir::AsyncCopySite::ConditionalResult | hir::AsyncCopySite::DiscardedResult => {
                return Err(self.error(pos, "the copy site does not acquire an owner"));
            }
        }
        if matches!(value, l::Operand::Value(value)
            if self.values.get(value.0 as usize).is_some_and(|value| value.fresh_owner))
        {
            if let l::Operand::Value(value) = value {
                if self.moved_async_owners.insert(*value) {
                    return Ok(());
                }
            }
        }
        let kind = match ty {
            l::ValueType::Data(Type::AsyncHandle(_)) => l::InstructionKind::AsyncHandleRetain,
            l::ValueType::Data(ty) if ty.counted_type().is_some() => {
                l::InstructionKind::AsyncHandleArrayRetain
            }
            _ => return Ok(()),
        };
        self.emit(
            kind,
            vec![value.clone()],
            None,
            false,
            self.read_lifetime(ty, pos),
            pos.clone(),
        )?;
        Ok(())
    }

    pub(super) fn release_owner(
        &mut self,
        value: l::Operand,
        ty: &l::ValueType,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let kind = match ty {
            l::ValueType::Data(Type::AsyncHandle(_)) => l::InstructionKind::AsyncHandleRelease,
            l::ValueType::Data(ty) if ty.counted_type().is_some() => {
                l::InstructionKind::AsyncHandleArrayRelease
            }
            _ => return Ok(()),
        };
        // compiler.md §116.1 rule 4: a release that frees a frame holding an
        // unobserved exception traps, so the release checks the word.
        let mut traps = self.read_lifetime(ty, pos);
        traps.push(l::Trap {
            kind: l::TrapKind::Call,
            pos: pos.clone(),
        });
        self.emit(kind, vec![value], None, false, traps, pos.clone())?;
        Ok(())
    }

    pub(super) fn discard_owner(
        &mut self,
        site: hir::AsyncCopySite,
        value: l::Operand,
        ty: &l::ValueType,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        match site {
            hir::AsyncCopySite::DiscardedResult => self.release_owner(value, ty, pos),
            hir::AsyncCopySite::Binding
            | hir::AsyncCopySite::Assignment
            | hir::AsyncCopySite::ArrayElement
            | hir::AsyncCopySite::SpreadElement
            | hir::AsyncCopySite::CallArgument
            | hir::AsyncCopySite::Return
            | hir::AsyncCopySite::ForOfBinding
            | hir::AsyncCopySite::ConditionalResult => {
                Err(self.error(pos, "the copy site does not discard an owner"))
            }
        }
    }

    pub(super) fn release_scopes_from(
        &mut self,
        depth: usize,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        if self.current.is_none() {
            return Ok(());
        }
        let mut bindings = self
            .scopes
            .iter()
            .skip(depth)
            .flat_map(|scope| scope.values().copied())
            .collect::<Vec<_>>();
        bindings.sort_unstable();
        bindings.dedup();
        bindings.reverse();
        for binding in bindings {
            let entry = self.bindings[binding.0].clone();
            // compiler.md §116.1 rule 4c: capture parameters borrow their handles.
            let capture = self.parameters.iter().any(|parameter| {
                parameter.kind == l::ParameterKind::Capture
                    && self
                        .scopes
                        .first()
                        .is_some_and(|scope| scope.get(&parameter.source_name) == Some(&binding))
            });
            let parameter = self.parameters.iter().any(|parameter| {
                self.scopes
                    .first()
                    .is_some_and(|scope| scope.get(&parameter.source_name) == Some(&binding))
            });
            if entry.ty == l::ValueType::Data(Type::TaskGroup) && !parameter {
                let value = self.read_binding(binding, pos)?;
                self.emit(
                    l::InstructionKind::TaskGroup(hir::TaskGroupOperation::Release),
                    vec![value],
                    None,
                    false,
                    vec![l::Trap {
                        kind: l::TrapKind::Call,
                        pos: pos.clone(),
                    }],
                    pos.clone(),
                )?;
            } else if is_async_owner_type(&entry.ty) && !capture {
                let value = self.read_binding(binding, pos)?;
                self.release_owner(value, &entry.ty, pos)?;
            }
        }
        Ok(())
    }

    pub(super) fn lookup_binding(&self, name: &str, pos: &Pos) -> Result<BindingId, LowerError> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .ok_or_else(|| self.error(pos, format!("unknown local `{name}`")))
    }

    pub(super) fn read_binding(
        &mut self,
        binding: BindingId,
        pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        let entry = self
            .bindings
            .get(binding.0)
            .cloned()
            .ok_or_else(|| self.error(pos, format!("binding {} is missing", binding.0)))?;
        if let Some(local) = entry.storage {
            self.load_local(local, pos)
        } else {
            entry.value.ok_or_else(|| {
                self.error(
                    pos,
                    format!("binding `{}` has no current SSA value", entry.source_name),
                )
            })
        }
    }

    pub(super) fn write_binding(
        &mut self,
        binding: BindingId,
        value: l::Operand,
        pos: &Pos,
        traps: Vec<l::Trap>,
    ) -> Result<(), LowerError> {
        let entry = self
            .bindings
            .get(binding.0)
            .cloned()
            .ok_or_else(|| self.error(pos, format!("binding {} is missing", binding.0)))?;
        let value = self.coerce_operand(value, entry.ty.clone(), pos)?;
        let old_owner = if is_async_owner_type(&entry.ty) {
            Some(self.read_binding(binding, pos)?)
        } else {
            None
        };
        if let Some(local) = entry.storage {
            self.emit_store_instruction(
                l::InstructionKind::StoreLocal(local),
                vec![value],
                vec![StoredOperand {
                    index: 0,
                    ty: entry.ty.clone(),
                    action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Assignment),
                    pos: pos.clone(),
                }],
                (None, false),
                traps,
                pos.clone(),
            )?;
            if let Some(old_owner) = old_owner {
                self.release_owner(old_owner, &entry.ty, pos)?;
            }
        } else {
            self.acquire_owner(hir::AsyncCopySite::Assignment, &value, &entry.ty, pos)?;
            if let Some(old_owner) = old_owner {
                self.release_owner(old_owner, &entry.ty, pos)?;
            }
            self.bindings[binding.0].value = Some(value);
        }
        Ok(())
    }

    pub(super) fn binding_snapshot(&self) -> Vec<Option<l::Operand>> {
        self.bindings
            .iter()
            .map(|binding| binding.value.clone())
            .collect()
    }

    pub(super) fn restore_bindings(&mut self, snapshot: &[Option<l::Operand>]) {
        for (binding, value) in self.bindings.iter_mut().zip(snapshot) {
            if binding.storage.is_none() {
                binding.value = value.clone();
            }
        }
    }

    fn visible_mutable_bindings(&self) -> Vec<BindingId> {
        let mut visible = BTreeSet::new();
        for scope in &self.scopes {
            for binding in scope.values() {
                if self.bindings[binding.0].mutable && self.bindings[binding.0].storage.is_none() {
                    visible.insert(*binding);
                }
            }
        }
        visible.into_iter().collect()
    }

    pub(super) fn block_target(
        &self,
        block: l::BlockId,
        mut prefix: Vec<l::Operand>,
    ) -> Result<l::BlockTarget, LowerError> {
        let draft = self.blocks.get(block.0 as usize).ok_or_else(|| {
            self.error(
                &self.function.pos,
                format!("branch target block {} is missing", block.0),
            )
        })?;
        for binding in &draft.state_bindings {
            let entry = &self.bindings[binding.0];
            let value = entry.value.clone().ok_or_else(|| {
                self.error(
                    &self.function.pos,
                    format!(
                        "binding `{}` has no value for block {}",
                        entry.source_name, block.0
                    ),
                )
            })?;
            prefix.push(value);
        }
        let prefix_len = prefix.len() - draft.state_bindings.len();
        let mut edge = target(block, prefix);
        for index in 0..prefix_len {
            edge.ownership[index] = matches!(&edge.arguments[index], l::Operand::Value(value)
                if self.values[value.0 as usize].fresh_owner && is_async_owner_type(&self.values[value.0 as usize].ty));
        }
        for (index, binding) in draft.state_bindings.iter().enumerate() {
            edge.ownership[prefix_len + index] = is_async_owner_type(&self.bindings[binding.0].ty);
        }
        Ok(edge)
    }

    pub(super) fn enter_block(&mut self, block: l::BlockId) -> Result<(), LowerError> {
        let draft = self.blocks.get(block.0 as usize).ok_or_else(|| {
            self.error(
                &self.function.pos,
                format!("entered block {} is missing", block.0),
            )
        })?;
        let state_start = draft.parameters.len() - draft.state_bindings.len();
        let updates = draft
            .state_bindings
            .iter()
            .copied()
            .zip(draft.parameters[state_start..].iter().copied())
            .collect::<Vec<_>>();
        for (binding, value) in updates {
            self.bindings[binding.0].value = Some(l::Operand::Value(value));
        }
        self.current = Some(block);
        Ok(())
    }
}

#[cfg(test)]
mod exit_tests {
    use super::*;
    use subscript_compiler::{check_program, SourceFile};

    #[test]
    fn a_reachable_cfg_end_cannot_contradict_the_body_predicate() {
        let module = check_program(&[SourceFile::new(
            "exit.ts",
            "function probe():void { while(true) { return; } }",
        )])
        .unwrap();
        for violate in [false, true] {
            let mut lowering = Lowering::new(&module, false).unwrap();
            let input = FunctionInput::from(module.functions[0].clone());
            let mut builder = FunctionBuilder::new(
                &mut lowering,
                l::FunctionId(0),
                input,
                l::FunctionKind::Free,
                None,
                Vec::new(),
            )
            .unwrap();
            // The violating builder leaves its entry open; the checked body remains intact.
            if !violate {
                builder
                    .lower_statements(&builder.function.body.clone())
                    .unwrap();
            }
            let result = builder.finish();
            if violate {
                assert_eq!(result.unwrap_err().message, "function `probe` exit facts differ: CFG fallthrough = true, body fallthrough = false");
            } else {
                result.unwrap();
            }
        }
    }
}
