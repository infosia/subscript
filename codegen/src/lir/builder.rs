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
        for capture in captures {
            builder.add_parameter(
                capture.name,
                l::ValueType::Data(capture.ty),
                l::ParameterKind::Capture,
                builder.function.pos.clone(),
            )?;
        }
        for parameter in builder.function.params.clone() {
            builder.add_parameter(
                parameter.name,
                l::ValueType::Data(parameter.ty),
                l::ParameterKind::Explicit,
                parameter.pos,
            )?;
        }
        Ok(builder)
    }

    pub(super) fn finish(mut self) -> Result<l::Function, LowerError> {
        if let Some(block) = self.current {
            if self.blocks[block.0 as usize].terminator.is_none() {
                if self.function.ret == Type::Void || self.function.is_generator {
                    let pos = self.function.pos.clone();
                    self.release_scopes_from(0, &pos)?;
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
            parameters: self.parameters,
            return_type: self.function.ret,
            locals: self.locals,
            values: self.values,
            liveness: l::Liveness::default(),
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

    fn add_local(
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

    pub(super) fn emit(
        &mut self,
        kind: l::InstructionKind,
        operands: Vec<l::Operand>,
        result_type: Option<l::ValueType>,
        invalidates_arrays: bool,
        traps: Vec<l::Trap>,
        pos: Pos,
    ) -> Result<Option<l::Operand>, LowerError> {
        let block = self.current.ok_or_else(|| {
            self.error(&pos, "attempted to emit an instruction after a terminator")
        })?;
        let result = result_type
            .as_ref()
            .map(|ty| self.new_value(ty.clone(), None));
        if kind.produces_fresh_async_owner()
            && result_type.as_ref().is_some_and(is_async_owner_type)
        {
            if let Some(value) = result {
                self.values[value.0 as usize].fresh_owner = true;
            }
        }
        let invalidates = if invalidates_arrays {
            self.array_values.clone()
        } else {
            Vec::new()
        };
        self.blocks[block.0 as usize]
            .instructions
            .push(l::Instruction {
                result,
                kind,
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
        terminator: l::Terminator,
        pos: &Pos,
    ) -> Result<(), LowerError> {
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
        if let Some(value) = &value {
            self.acquire_owner(hir::AsyncCopySite::Return, value, &ty, pos)?;
        }
        self.release_scopes_from(0, pos)?;
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

    fn acquire_owner(
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
            l::ValueType::Data(Type::Array(element))
                if matches!(&**element, Type::AsyncHandle(_)) =>
            {
                l::InstructionKind::AsyncHandleArrayRetain
            }
            _ => return Ok(()),
        };
        self.emit(
            kind,
            vec![value.clone()],
            None,
            false,
            Vec::new(),
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
            l::ValueType::Data(Type::Array(element))
                if matches!(&**element, Type::AsyncHandle(_)) =>
            {
                l::InstructionKind::AsyncHandleArrayRelease
            }
            _ => return Ok(()),
        };
        self.emit(kind, vec![value], None, false, Vec::new(), pos.clone())?;
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
            if is_async_owner_type(&entry.ty) {
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
        Ok(target(block, prefix))
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
