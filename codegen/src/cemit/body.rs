//! Function body setup: value storage, parameter initializers, and coroutine dispatch.

use super::*;

impl<'e, 'm, 'f> Body<'e, 'm, 'f> {
    pub(super) fn new(
        emitter: &'e mut Emitter<'m>,
        function: &'f l::Function,
        coroutine: bool,
    ) -> Result<Self, String> {
        let index = EmissionIndex::build(function)?;
        let (root_storage, interference) =
            root_storage::plan_with_interference(function, &emitter.layouts)?;
        let mut suspend_states = vec![None; function.blocks.len()];
        let mut state = 0;
        for block in &function.blocks {
            if matches!(block.terminator, l::Terminator::Suspend { .. }) {
                state += 1;
                suspend_states[block.id.0 as usize] = Some(state);
            }
        }
        let dead_forward_iterator_results = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| {
                let result = instruction.result?;
                if !matches!(instruction.kind, l::InstructionKind::IteratorAdvance)
                    || !index.use_blocks[result.0 as usize].is_empty()
                {
                    return None;
                }
                let l::Operand::Value(iterator) = instruction.operands.first()? else {
                    return None;
                };
                let l::ValueType::Iterator(iterator) = &function.values[iterator.0 as usize].ty
                else {
                    return None;
                };
                matches!(
                    iterator.kind,
                    l::ForOfKind::ArrayValues | l::ForOfKind::ArrayKeys
                )
                .then_some(result)
            })
            .collect::<HashSet<_>>();
        let fixed_iterators = fixed_iterator_values(function, &index);
        let rooted_values = root_storage
            .value_slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.map(|_| l::ValueId(index as u32)))
            .collect::<HashSet<_>>();
        let address_definitions = &index.definitions;
        let folded_addresses = foldable_local_addresses(function, address_definitions);
        let managed_locals = function
            .locals
            .iter()
            .filter(|local| local.storage == l::LocalStorageClass::Activation)
            .map(|local| {
                local_contains_managed(&emitter.layouts, &local.ty)
                    .map(|managed| managed.then_some(local.id))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<HashSet<_>>();
        let promoted_locals = promoted_local_values(function, &index, &folded_addresses)
            .into_iter()
            .filter(|(local, _)| !managed_locals.contains(local))
            .collect::<HashMap<_, _>>();
        let promoted_local_values = promoted_locals.values().copied().collect::<HashSet<_>>();
        let rooted_locals = function
            .locals
            .iter()
            .filter(|local| {
                local.storage == l::LocalStorageClass::Activation
                    && !promoted_locals.contains_key(&local.id)
            })
            .map(|local| {
                local_contains_managed(&emitter.layouts, &local.ty)
                    .map(|contains| contains.then_some(local.id))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<HashSet<_>>();
        let (removable_edge_copies, elided_values) =
            removable_block_parameter_copies(function, &index);
        let value_storage = coalesced_value_storage(
            function,
            &root_storage,
            &interference,
            &folded_addresses,
            &removable_edge_copies,
            &elided_values,
            &promoted_local_values,
        )?;
        let declaration_scopes = declaration_scopes(
            function,
            coroutine,
            &rooted_values,
            &folded_addresses,
            &elided_values,
            &value_storage,
            &promoted_locals,
        );
        let mut delayed_declarations = HashMap::new();
        for (block, values) in declaration_scopes.block_values.iter().enumerate() {
            for value in values {
                if value_storage[value.0 as usize] == *value
                    && index.definition_blocks[value.0 as usize] == Some(l::BlockId(block as u32))
                    && index.definitions.get(value).is_some_and(|instruction| {
                        declaration_can_use_instruction_assignment(&instruction.kind)
                    })
                {
                    delayed_declarations.insert(format!("v{}", value.0), *value);
                }
            }
        }
        Ok(Self {
            emitter,
            function,
            coroutine,
            suspend_states,
            rooted_values,
            rooted_locals,
            promoted_locals,
            address_definitions: index.definitions,
            folded_addresses,
            function_scoped_values: declaration_scopes.function_values,
            block_value_declarations: declaration_scopes.block_values,
            dominator_children: declaration_scopes.dominator_children,
            graph_roots: declaration_scopes.graph_roots,
            removable_edge_copies,
            value_storage,
            root_storage,
            dead_forward_iterator_results,
            fixed_iterators,
            delayed_declarations,
            consumed_traps: Vec::new(),
            temporary: 0,
            shadow_frame: false,
        })
    }

    pub(super) fn fresh(&mut self) -> String {
        let value = format!("t{}", self.temporary);
        self.temporary += 1;
        value
    }

    pub(super) fn emit_root_clears(
        &mut self,
        out: &mut String,
        slots: &[usize],
    ) -> Result<(), String> {
        let representatives = slots
            .iter()
            .map(|slot| {
                self.root_storage
                    .slots
                    .get(*slot)
                    .map(|slot| slot.representative)
                    .ok_or_else(|| internal(format!("root slot {slot} is missing")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for representative in representatives {
            let zero = self.emitter.zero(self.value_type(representative)?)?;
            let _ = writeln!(out, "    {} = {zero};", self.value(representative));
        }
        Ok(())
    }

    pub(super) fn value(&self, id: l::ValueId) -> String {
        let id = self.value_storage[id.0 as usize];
        if self.rooted_values.contains(&id) {
            format!("roots.v{}", id.0)
        } else {
            format!("v{}", id.0)
        }
    }

    pub(super) fn local(&self, id: l::LocalId) -> String {
        if self
            .function
            .locals
            .get(id.0 as usize)
            .is_some_and(|local| local.storage == l::LocalStorageClass::Frame)
        {
            return format!("frame->l{}", id.0);
        }
        if let Some(value) = self.promoted_locals.get(&id) {
            return self.value(*value);
        }
        if self.rooted_locals.contains(&id) {
            format!("roots.l{}", id.0)
        } else {
            format!("l{}", id.0)
        }
    }

    pub(super) fn is_function_value(&self, id: l::ValueId) -> Result<bool, String> {
        Ok(matches!(
            self.value_type(id)?,
            l::ValueType::Data(Type::Func(_))
        ))
    }

    pub(super) fn closure_environment(&self, id: l::ValueId) -> String {
        if self.coroutine {
            format!("&frame->env_v{}", id.0)
        } else {
            format!("&roots.env_v{}", id.0)
        }
    }

    pub(super) fn assign_function_value(
        &mut self,
        out: &mut String,
        id: l::ValueId,
        source: &str,
    ) -> Result<(), String> {
        if !self.emitter.has_closure_environments() {
            return self.assign(out, Some(self.value(id)), source);
        }
        let temporary = self.fresh();
        let environment = self.closure_environment(id);
        let _ = writeln!(out, "    SubFn {temporary} = {source};");
        let _ = writeln!(
            out,
            "    if ({temporary}.env != NULL) {{ memcpy({environment}, {temporary}.env, sizeof(SubEnvStorage)); {temporary}.env = {environment}; }}"
        );
        self.assign(out, Some(self.value(id)), &temporary)
    }

    pub(super) fn snapshot_function_value(&mut self, out: &mut String, source: &str) -> String {
        let temporary = self.fresh();
        let environment = self.fresh();
        let _ = writeln!(out, "    SubFn {temporary} = {source};");
        let _ = writeln!(out, "    SubEnvStorage {environment} = {{0}};");
        let _ = writeln!(
            out,
            "    if ({temporary}.env != NULL) {{ memcpy(&{environment}, {temporary}.env, sizeof(SubEnvStorage)); {temporary}.env = &{environment}; }}"
        );
        temporary
    }

    pub(super) fn value_type(&self, id: l::ValueId) -> Result<&l::ValueType, String> {
        value_type(self.function, id)
    }

    pub(super) fn folded_address_expression(
        &mut self,
        value: l::ValueId,
    ) -> Result<String, String> {
        if !self.folded_addresses.contains(&value) {
            return Err(internal(format!(
                "address value {} is not foldable",
                value.0
            )));
        }
        let instruction = self
            .address_definitions
            .get(&value)
            .copied()
            .ok_or_else(|| internal(format!("address value {} has no definition", value.0)))?
            .clone();
        match instruction.kind {
            l::InstructionKind::AddressOfLocal(local) => Ok(self.local(local)),
            l::InstructionKind::AddressOfField(field) => {
                let Some(l::Operand::Value(base)) = instruction.operands.first() else {
                    return Err(internal("folded field address has no value base"));
                };
                let base = *base;
                let base_expression = if self.folded_addresses.contains(&base) {
                    self.folded_address_expression(base)?
                } else {
                    format!("*({})", self.value(base))
                };
                match field {
                    l::FieldRef::Class(field) => {
                        let (class, _, _) = self.emitter.field(field)?;
                        let l::ValueType::Address(address) = self.value_type(base)? else {
                            return Err(internal("folded field base is not an address"));
                        };
                        match &address.pointee {
                            Type::Class(id) if self.emitter.is_value_class(*id)? => {
                                Ok(format!("({base_expression}).d{}", field.0))
                            }
                            Type::Class(_) => Ok(format!(
                                "((({}*)({base_expression}))->d{})",
                                self.emitter.class_name(class),
                                field.0
                            )),
                            other => Err(internal(format!(
                                "folded class field base has type {other:?}"
                            ))),
                        }
                    }
                    l::FieldRef::IterDone => Ok(format!("({base_expression}).done")),
                    l::FieldRef::IterValue => Ok(format!("({base_expression}).value")),
                }
            }
            l::InstructionKind::AddressOfIndex { .. } => {
                let Some(l::Operand::Value(base)) = instruction.operands.first() else {
                    return Err(internal("folded index address has no value base"));
                };
                let base = *base;
                let base_expression = if self.folded_addresses.contains(&base) {
                    self.folded_address_expression(base)?
                } else {
                    format!("*({})", self.value(base))
                };
                let index = instruction
                    .operands
                    .get(1)
                    .ok_or_else(|| internal("folded index address has no index"))?;
                let index = self.operand(index)?;
                let l::ValueType::Address(address) = self.value_type(base)? else {
                    return Err(internal("folded index base is not an address"));
                };
                match &address.pointee {
                    Type::FixedArray(_, _) => Ok(format!("({base_expression}).a[{index}]")),
                    other => Err(internal(format!(
                        "folded indexed address points to {other:?}"
                    ))),
                }
            }
            ref other => Err(internal(format!(
                "folded address value {} has definition {other:?}",
                value.0
            ))),
        }
    }

    /// The members of this function's shadow-root frame, in emission order.
    ///
    /// One derivation serves the frame declaration and the frame pop, so a
    /// frame is never declared without a member and never pushed without a
    /// matching pop. C11 6.7.2.1 gives a structure at least one member;
    /// MSVC enforces it (`C2016`), and GCC and clang accept an empty one as
    /// an extension.
    fn shadow_frame_members(&self) -> Result<Vec<String>, String> {
        let owns_closure_environments = !self.coroutine && self.emitter.has_closure_environments();
        let mut members = Vec::new();
        for value in &self.function.values {
            if self.value_storage[value.id.0 as usize] == value.id
                && self.rooted_values.contains(&value.id)
            {
                members.push(format!(
                    "        {} v{};\n",
                    self.emitter.value_ctype(&value.ty)?,
                    value.id.0
                ));
            }
        }
        for local in &self.function.locals {
            if local.storage == l::LocalStorageClass::Activation
                && self.rooted_locals.contains(&local.id)
            {
                members.push(format!(
                    "        {} l{};\n",
                    self.emitter.value_ctype(&local.ty)?,
                    local.id.0
                ));
            }
        }
        if owns_closure_environments {
            for value in &self.function.values {
                if matches!(value.ty, l::ValueType::Data(Type::Func(_))) {
                    members.push(format!("        SubEnvStorage env_v{};\n", value.id.0));
                }
            }
        }
        Ok(members)
    }

    pub(super) fn emit_storage(&mut self, out: &mut String) -> Result<(), String> {
        let members = self.shadow_frame_members()?;
        self.shadow_frame = !members.is_empty();
        if self.shadow_frame {
            out.push_str("    struct {\n");
            for member in &members {
                out.push_str(member);
            }
            out.push_str("    } roots = {0};\n");
            let call = self.emitter.runtime_call(
                "void",
                "subscript_rt_shadow_push",
                &["void*".into(), "void*".into(), "uint64_t".into()],
                &[
                    "ctx".into(),
                    "&roots".into(),
                    "(sizeof roots + 7u) / 8u".into(),
                ],
            );
            let _ = writeln!(out, "    {call};");
        }
        for value in &self.function.values {
            if self.function_scoped_values.contains(&value.id) {
                let ctype = self.emitter.value_ctype(&value.ty)?;
                if let Some(initializer) = self.parameter_declaration_initializer(value.id) {
                    let _ = writeln!(out, "    {ctype} v{} = {initializer};", value.id.0);
                } else {
                    let zero = self.emitter.zero(&value.ty)?;
                    let _ = writeln!(out, "    {ctype} v{} = {zero};", value.id.0);
                }
            }
        }
        for local in &self.function.locals {
            if local.storage == l::LocalStorageClass::Activation
                && !self.rooted_locals.contains(&local.id)
                && !self.promoted_locals.contains_key(&local.id)
            {
                let _ = writeln!(
                    out,
                    "    {} l{} = {};",
                    self.emitter.value_ctype(&local.ty)?,
                    local.id.0,
                    self.emitter.zero(&local.ty)?
                );
            }
        }
        Ok(())
    }

    pub(super) fn emit_parameter_initializers(&mut self, out: &mut String) -> Result<(), String> {
        for parameter in &self.function.parameters {
            let destination = self.value(parameter.value);
            if self
                .parameter_declaration_initializer(self.value_storage[parameter.value.0 as usize])
                .is_none()
            {
                let source = match parameter.kind {
                    l::ParameterKind::Capture => format!(
                        "((SubEnv{}*)environment)->c{}",
                        self.function.id.0, parameter.value.0
                    ),
                    l::ParameterKind::Explicit | l::ParameterKind::Receiver => {
                        format!("a{}", parameter.value.0)
                    }
                };
                if self.is_function_value(parameter.value)? {
                    self.assign_function_value(out, parameter.value, &source)?;
                } else {
                    let _ = writeln!(out, "    {destination} = {source};");
                }
            }
            if let Some(storage) = parameter.storage {
                if self.function.locals[storage.0 as usize].storage == l::LocalStorageClass::Frame {
                    continue;
                }
                let storage = self.local(storage);
                if storage != destination
                    && !self.entry_initializes_parameter_storage(parameter, storage.as_str())
                {
                    let _ = writeln!(out, "    {storage} = {destination};");
                }
            }
        }
        Ok(())
    }

    fn entry_initializes_parameter_storage(&self, parameter: &l::Parameter, storage: &str) -> bool {
        self.function.blocks[self.function.entry.0 as usize]
            .instructions
            .iter()
            .any(|instruction| {
                matches!(instruction.kind, l::InstructionKind::StoreLocal(local)
                    if self.local(local) == storage)
                    && matches!(instruction.operands.as_slice(),
                        [l::Operand::Value(value)] if *value == parameter.value)
            })
    }

    fn parameter_declaration_initializer(&self, value: l::ValueId) -> Option<String> {
        if self.coroutine
            || !self.function_scoped_values.contains(&value)
            || self.is_function_value(value).ok() == Some(true)
        {
            return None;
        }
        self.function.parameters.iter().find_map(|parameter| {
            (self.value_storage[parameter.value.0 as usize] == value).then(|| {
                match parameter.kind {
                    l::ParameterKind::Capture => format!(
                        "((SubEnv{}*)environment)->c{}",
                        self.function.id.0, parameter.value.0
                    ),
                    l::ParameterKind::Explicit | l::ParameterKind::Receiver => {
                        format!("a{}", parameter.value.0)
                    }
                }
            })
        })
    }

    pub(super) fn emit_coroutine_dispatch(&mut self, out: &mut String) -> Result<(), String> {
        for parameter in &self.function.parameters {
            let source = format!("frame->p{}", parameter.value.0);
            if self.is_function_value(parameter.value)? {
                self.assign_function_value(out, parameter.value, &source)?;
            } else {
                let _ = writeln!(out, "    {} = {source};", self.value(parameter.value));
            }
            if let Some(storage) = parameter.storage {
                if self.function.locals[storage.0 as usize].storage == l::LocalStorageClass::Frame {
                    continue;
                }
                let storage = self.local(storage);
                let value = self.value(parameter.value);
                if storage != value {
                    let _ = writeln!(out, "    {storage} = {value};");
                }
            }
        }
        let _ = writeln!(
            out,
            "    if (frame->state == 0) goto b{};",
            self.function.entry.0
        );
        let mut state = 1u32;
        for block in &self.function.blocks {
            if matches!(block.terminator, l::Terminator::Suspend { .. }) {
                let _ = writeln!(
                    out,
                    "    if (frame->state == {state}) goto resume_b{};",
                    block.id.0
                );
                state += 1;
            }
        }
        out.push_str("    goto coroutine_done;\n");
        for block in &self.function.blocks {
            let l::Terminator::Suspend {
                successor,
                resume_value,
                kind,
                ..
            } = &block.terminator
            else {
                continue;
            };
            let _ = writeln!(out, "resume_b{}:\n    ;", block.id.0);
            match kind {
                l::SuspendKind::AsyncCall { .. } => {
                    self.emit_async_child_resume(out, block)?;
                }
                l::SuspendKind::AsyncHandle { .. } => {
                    self.emit_async_handle_resume(out, block)?;
                }
                _ => {
                    if resume_value.is_some() {
                        return Err(internal("non-call suspension defines a resume value"));
                    }
                    self.restore_suspend_arguments(out, block)?;
                    let _ = writeln!(out, "    goto b{};", successor.0);
                }
            }
        }
        Ok(())
    }
}
