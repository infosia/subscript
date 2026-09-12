//! Lowering for places: addresses, loads, stores, and coercions.

use super::*;

impl<'a, 'm> FunctionBuilder<'a, 'm> {
    pub(super) fn terminator_value(
        &mut self,
        operand: l::Operand,
        pos: &Pos,
    ) -> Result<l::ValueId, LowerError> {
        let ty = self.operand_type(&operand, pos)?;
        let value = match operand {
            l::Operand::Value(value) => value,
            constant @ l::Operand::Constant(_) => {
                let materialized = self
                    .emit(
                        l::InstructionKind::Copy,
                        vec![constant],
                        Some(ty.clone()),
                        false,
                        Vec::new(),
                        pos.clone(),
                    )?
                    .expect("copy result");
                let l::Operand::Value(value) = materialized else {
                    unreachable!()
                };
                value
            }
        };
        Ok(value)
    }

    pub(super) fn prepare_place(&mut self, expr: &hir::Expr) -> Result<PreparedPlace, LowerError> {
        self.prepare_place_with_traps(expr, None)
    }

    fn prepare_place_with_traps(
        &mut self,
        expr: &hir::Expr,
        traps: Option<Vec<l::Trap>>,
    ) -> Result<PreparedPlace, LowerError> {
        let mut traps = traps.unwrap_or_else(|| convert_traps(&expr.trap_sites(self.lowering.hir)));
        let kind = match &expr.kind {
            hir::ExprKind::Local(name) => {
                let binding = self.lookup_binding(name, &expr.pos)?;
                let ty = match &self.bindings[binding.0].ty {
                    l::ValueType::Data(ty) => ty.clone(),
                    other => {
                        return Err(self.error(
                            &expr.pos,
                            format!("local place has invalid LIR type {other:?}"),
                        ));
                    }
                };
                let local = self.bindings[binding.0].storage.ok_or_else(|| {
                    self.error(
                        &expr.pos,
                        format!("local `{name}` is used as an address but has no storage"),
                    )
                })?;
                PreparedPlaceKind::Local(local, ty)
            }
            hir::ExprKind::Global(name) => {
                let global = self
                    .lowering
                    .globals
                    .get(name)
                    .copied()
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown global `{name}`")))?;
                let ty = self
                    .lowering
                    .hir
                    .globals
                    .iter()
                    .find(|definition| definition.name == *name)
                    .map(|definition| definition.ty.clone())
                    .ok_or_else(|| self.error(&expr.pos, format!("unknown global `{name}`")))?;
                PreparedPlaceKind::Global(global, ty)
            }
            hir::ExprKind::Field { obj, name } => {
                let base = if is_stored_aggregate(self.lowering.hir, &obj.ty) && is_place_expr(obj)
                {
                    PreparedBase::Place(Box::new(self.prepare_place(obj)?))
                } else {
                    PreparedBase::Value(self.require_expr(obj)?)
                };
                let field = self.resolve_field(&obj.ty, name, &expr.pos)?;
                let ty = self.resolved_field_type(field, &obj.ty, &expr.pos)?;
                PreparedPlaceKind::Field { base, field, ty }
            }
            hir::ExprKind::Index {
                obj,
                index,
                checked,
            } => {
                let base = if matches!(obj.ty, Type::FixedArray(..)) && is_place_expr(obj) {
                    PreparedBase::Place(Box::new(self.prepare_place(obj)?))
                } else {
                    PreparedBase::Value(self.require_expr(obj)?)
                };
                let index = self.require_expr(index)?;
                PreparedPlaceKind::Index {
                    base,
                    index,
                    checked: *checked,
                    ty: self.indexed_element_type(&obj.ty, &expr.pos)?,
                }
            }
            hir::ExprKind::This if self.this_value.is_some() => {
                let value = self.this_value.clone().expect("this value");
                if matches!(
                    self.operand_type(&value, &expr.pos)?,
                    l::ValueType::Address(_)
                ) {
                    PreparedPlaceKind::ExistingAddress(value, expr.ty.clone())
                } else {
                    return Err(
                        self.error(&expr.pos, "reference `this` is not an addressable value")
                    );
                }
            }
            other => {
                return Err(self.error(
                    &expr.pos,
                    format!("assignment/mutable receiver is not an addressable form: {other:?}"),
                ));
            }
        };
        let mut nested = Vec::new();
        let base = match &kind {
            PreparedPlaceKind::Field { base, .. } | PreparedPlaceKind::Index { base, .. } => {
                Some(base)
            }
            PreparedPlaceKind::ExistingAddress(..)
            | PreparedPlaceKind::BoxedBoundary(..)
            | PreparedPlaceKind::Local(..)
            | PreparedPlaceKind::Global(..) => None,
        };
        if let Some(PreparedBase::Place(base)) = base {
            collect_place_traps(base, &mut nested);
        }
        for nested_trap in nested {
            if let Some(index) = traps.iter().position(|trap| *trap == nested_trap) {
                traps.remove(index);
            }
        }
        let place = PreparedPlace { kind, traps };
        let stored = self.place_type(&place).clone();
        if self.is_boundary_box_narrowing(&stored, &expr.ty) {
            let handle = self.load_place(&place, &expr.pos)?;
            return Ok(PreparedPlace {
                kind: PreparedPlaceKind::BoxedBoundary(handle, expr.ty.clone()),
                traps: Vec::new(),
            });
        }
        Ok(place)
    }

    pub(super) fn materialize_address(
        &mut self,
        place: &PreparedPlace,
        pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        self.materialize_address_inner(place, pos, true)
    }

    pub(super) fn materialize_address_inner(
        &mut self,
        place: &PreparedPlace,
        pos: &Pos,
        include_traps: bool,
    ) -> Result<l::Operand, LowerError> {
        let traps = if include_traps {
            place.traps.clone()
        } else {
            Default::default()
        };
        match &place.kind {
            PreparedPlaceKind::ExistingAddress(address, _) => Ok(address.clone()),
            PreparedPlaceKind::BoxedBoundary(handle, _) => Ok(handle.clone()),
            PreparedPlaceKind::Local(local, ty) => self
                .emit(
                    l::InstructionKind::AddressOfLocal(*local),
                    Vec::new(),
                    Some(l::ValueType::Address(l::AddressType {
                        pointee: ty.clone(),
                        array_base: None,
                    })),
                    false,
                    traps,
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "local address produced no value")),
            PreparedPlaceKind::Global(global, ty) => self
                .emit(
                    l::InstructionKind::AddressOfGlobal(*global),
                    Vec::new(),
                    Some(l::ValueType::Address(l::AddressType {
                        pointee: ty.clone(),
                        array_base: None,
                    })),
                    false,
                    traps,
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "global address produced no value")),
            PreparedPlaceKind::Field { base, field, ty } => {
                let base = self.materialize_base(base, pos, include_traps)?;
                let array_base = address_base(&self.operand_type(&base, pos)?);
                self.emit(
                    l::InstructionKind::AddressOfField(*field),
                    vec![base],
                    Some(l::ValueType::Address(l::AddressType {
                        pointee: ty.clone(),
                        array_base,
                    })),
                    false,
                    traps,
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "field address produced no value"))
            }
            PreparedPlaceKind::Index {
                base,
                index,
                checked,
                ty,
            } => {
                let base = self.materialize_base(base, pos, include_traps)?;
                let base_type = self.operand_type(&base, pos)?;
                let array_base = match (&base, &base_type) {
                    (l::Operand::Value(value), l::ValueType::Data(Type::Array(_))) => Some(*value),
                    (_, l::ValueType::Address(address)) => address.array_base,
                    _ => None,
                };
                self.emit(
                    l::InstructionKind::AddressOfIndex {
                        checked: *checked && include_traps,
                    },
                    vec![base, index.clone()],
                    Some(l::ValueType::Address(l::AddressType {
                        pointee: ty.clone(),
                        array_base,
                    })),
                    false,
                    traps,
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "index address produced no value"))
            }
        }
    }

    fn materialize_base(
        &mut self,
        base: &PreparedBase,
        pos: &Pos,
        include_traps: bool,
    ) -> Result<l::Operand, LowerError> {
        match base {
            PreparedBase::Value(value) => Ok(value.clone()),
            PreparedBase::Place(place) => self.materialize_address_inner(place, pos, include_traps),
        }
    }

    pub(super) fn load_place(
        &mut self,
        place: &PreparedPlace,
        pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        match &place.kind {
            PreparedPlaceKind::BoxedBoundary(handle, ty) => {
                self.coerce_operand(handle.clone(), l::ValueType::Data(ty.clone()), pos)
            }
            PreparedPlaceKind::Local(local, _) => self.load_local(*local, pos),
            PreparedPlaceKind::Global(global, ty) => self
                .emit(
                    l::InstructionKind::LoadGlobal(*global),
                    Vec::new(),
                    Some(l::ValueType::Data(ty.clone())),
                    false,
                    place.traps.clone(),
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "global load produced no value")),
            _ => {
                let ty = self.place_type(place).clone();
                let address = self.materialize_address(place, pos)?;
                self.emit(
                    l::InstructionKind::LoadAddress,
                    vec![address],
                    Some(l::ValueType::Data(ty)),
                    false,
                    Vec::new(),
                    pos.clone(),
                )?
                .ok_or_else(|| self.error(pos, "address load produced no value"))
            }
        }
    }

    pub(super) fn store_place(
        &mut self,
        place: &PreparedPlace,
        value: l::Operand,
        pos: &Pos,
    ) -> Result<(), LowerError> {
        let ty = l::ValueType::Data(self.place_type(place).clone());
        let value = self.coerce_operand(value, ty.clone(), pos)?;
        let counted = is_async_owner_type(&ty);
        match &place.kind {
            PreparedPlaceKind::Local(local, _) => {
                let old_owner = counted.then(|| self.load_local(*local, pos)).transpose()?;
                self.emit_store_instruction(
                    l::InstructionKind::StoreLocal(*local),
                    vec![value],
                    vec![StoredOperand {
                        index: 0,
                        ty: ty.clone(),
                        action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Assignment),
                        pos: pos.clone(),
                    }],
                    (None, false),
                    place.traps.clone(),
                    pos.clone(),
                )?;
                if let Some(old_owner) = old_owner {
                    self.release_owner(old_owner, &ty, pos)?;
                }
            }
            PreparedPlaceKind::Global(global, _) => {
                let old_owner = if counted {
                    self.emit(
                        l::InstructionKind::LoadGlobal(*global),
                        Vec::new(),
                        Some(ty.clone()),
                        false,
                        Vec::new(),
                        pos.clone(),
                    )?
                } else {
                    None
                };
                self.emit_store_instruction(
                    l::InstructionKind::StoreGlobal(*global),
                    vec![value],
                    vec![StoredOperand {
                        index: 0,
                        ty: ty.clone(),
                        action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Assignment),
                        pos: pos.clone(),
                    }],
                    (None, false),
                    place.traps.clone(),
                    pos.clone(),
                )?;
                if let Some(old_owner) = old_owner {
                    self.release_owner(old_owner, &ty, pos)?;
                }
            }
            _ => {
                let address = self.materialize_address(place, pos)?;
                let old_owner = if counted {
                    self.emit(
                        l::InstructionKind::LoadAddress,
                        vec![address.clone()],
                        Some(ty.clone()),
                        false,
                        Vec::new(),
                        pos.clone(),
                    )?
                } else {
                    None
                };
                self.emit_store_instruction(
                    l::InstructionKind::StoreAddress,
                    vec![address, value],
                    vec![StoredOperand {
                        index: 1,
                        ty: ty.clone(),
                        action: OwnerStoreAction::Acquire(hir::AsyncCopySite::Assignment),
                        pos: pos.clone(),
                    }],
                    (None, false),
                    Vec::new(),
                    pos.clone(),
                )?;
                if let Some(old_owner) = old_owner {
                    self.release_owner(old_owner, &ty, pos)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn place_type<'p>(&self, place: &'p PreparedPlace) -> &'p Type {
        match &place.kind {
            PreparedPlaceKind::ExistingAddress(_, ty)
            | PreparedPlaceKind::BoxedBoundary(_, ty)
            | PreparedPlaceKind::Local(_, ty)
            | PreparedPlaceKind::Global(_, ty)
            | PreparedPlaceKind::Field { ty, .. }
            | PreparedPlaceKind::Index { ty, .. } => ty,
        }
    }

    pub(super) fn embedded_header_extension(
        &self,
        expected: &Type,
        expr: &hir::Expr,
    ) -> Option<(ClassId, ClassId)> {
        let header = boundary_box_class(self.lowering.hir, expected)?;
        let hir::ExprKind::Field { obj, name } = &expr.kind else {
            return None;
        };
        let Type::Class(extension) = obj.ty else {
            return None;
        };
        let definition = self.lowering.hir.classes.get(extension.0)?;
        let first = definition.fields.first()?;
        (definition.is_value
            && definition.is_boundary
            && first.name == *name
            && first.ty == Type::Class(header)
            && self
                .lowering
                .classes
                .get(header.0)
                .is_some_and(|header| header.is_embedded_header))
        .then_some((extension, header))
    }

    pub(super) fn lower_stored_expr(
        &mut self,
        expected: &Type,
        expr: &hir::Expr,
    ) -> Result<l::Operand, LowerError> {
        self.lower_stored_expr_at(expected, expr, &expr.pos)
    }

    pub(super) fn lower_stored_expr_at(
        &mut self,
        expected: &Type,
        expr: &hir::Expr,
        coercion_pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        if let Some((extension, header)) = self.embedded_header_extension(expected, expr) {
            let hir::ExprKind::Field { obj, .. } = &expr.kind else {
                unreachable!("embedded header projection is a field")
            };
            let value = self.require_expr(obj)?;
            let actual = self.operand_type(&value, &expr.pos)?;
            let expected_operand = l::ValueType::Data(Type::Class(extension));
            if actual != expected_operand {
                return Err(self.error(
                    &expr.pos,
                    format!(
                        "embedded header extension has LIR type {actual:?}, expected {expected_operand:?}"
                    ),
                ));
            }
            return self
                .emit(
                    l::InstructionKind::BoxBoundaryValue { payload: extension },
                    vec![value],
                    Some(l::ValueType::Data(Type::Nullable(Box::new(Type::Class(
                        header,
                    ))))),
                    false,
                    vec![l::Trap {
                        kind: l::TrapKind::Allocation,
                        pos: expr.pos.clone(),
                    }],
                    expr.pos.clone(),
                )?
                .ok_or_else(|| self.error(&expr.pos, "embedded header box produced no handle"));
        }
        let value = self.require_expr(expr)?;
        self.coerce_operand(value, l::ValueType::Data(expected.clone()), coercion_pos)
    }

    pub(super) fn is_boundary_box_narrowing(&self, stored: &Type, narrowed: &Type) -> bool {
        boundary_box_class(self.lowering.hir, stored)
            .is_some_and(|class| narrowed == &Type::Class(class))
    }

    pub(super) fn foreign_boundary_pointer_representation(
        &self,
        declared: &Type,
        actual: &l::ValueType,
    ) -> bool {
        let Some(class) = boundary_box_class(self.lowering.hir, declared) else {
            return false;
        };
        matches!(actual, l::ValueType::Address(address) if address.pointee == Type::Class(class))
    }

    pub(super) fn allocated_type(
        &self,
        class: ClassId,
        pos: &Pos,
    ) -> Result<l::ValueType, LowerError> {
        let definition = self
            .lowering
            .hir
            .classes
            .get(class.0)
            .ok_or_else(|| self.error(pos, "allocated class id is missing"))?;
        Ok(if definition.is_value {
            l::ValueType::Address(l::AddressType {
                pointee: Type::Class(class),
                array_base: None,
            })
        } else {
            l::ValueType::Data(Type::Class(class))
        })
    }

    pub(super) fn resolve_field(
        &self,
        object_type: &Type,
        name: &str,
        pos: &Pos,
    ) -> Result<l::FieldRef, LowerError> {
        match object_type {
            Type::Class(class) => self
                .lowering
                .fields
                .get(&(class.0, name.to_string()))
                .copied()
                .map(l::FieldRef::Class)
                .ok_or_else(|| {
                    self.error(
                        pos,
                        format!("class #{} has no resolved field `{name}`", class.0),
                    )
                }),
            Type::IterResult(_) if name == "done" => Ok(l::FieldRef::IterDone),
            Type::IterResult(_) if name == "value" => Ok(l::FieldRef::IterValue),
            _ => Err(self.error(
                pos,
                format!("field `{name}` on `{object_type}` has no LIR field id"),
            )),
        }
    }

    pub(super) fn resolved_field_type(
        &self,
        field: l::FieldRef,
        object_type: &Type,
        pos: &Pos,
    ) -> Result<Type, LowerError> {
        match field {
            l::FieldRef::Class(field_id) => self
                .lowering
                .classes
                .iter()
                .flat_map(|class| &class.fields)
                .find(|field| field.id == field_id)
                .map(|field| field.ty.clone())
                .ok_or_else(|| self.error(pos, "resolved class field type is missing")),
            l::FieldRef::IterDone => Ok(Type::Bool),
            l::FieldRef::IterValue => match object_type {
                Type::IterResult(value) => Ok((**value).clone()),
                _ => Err(self.error(pos, "iterator value field has a non-iterator base")),
            },
        }
    }

    fn indexed_element_type(&self, ty: &Type, pos: &Pos) -> Result<Type, LowerError> {
        match ty {
            Type::Array(element) | Type::FixedArray(element, _) => Ok((**element).clone()),
            _ => Err(self.error(pos, format!("indexed base `{ty}` is not an array"))),
        }
    }

    pub(super) fn lookup_substitution(&self, name: &str) -> Option<l::Operand> {
        self.substitutions
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    pub(super) fn load_local(
        &mut self,
        local: l::LocalId,
        pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        let ty = self
            .locals
            .get(local.0 as usize)
            .map(|local| local.ty.clone())
            .ok_or_else(|| self.error(pos, format!("local {} is missing", local.0)))?;
        self.emit(
            l::InstructionKind::LoadLocal(local),
            Vec::new(),
            Some(ty),
            false,
            Vec::new(),
            pos.clone(),
        )?
        .ok_or_else(|| self.error(pos, "local load produced no value"))
    }

    pub(super) fn coerce_operand(
        &mut self,
        operand: l::Operand,
        expected: l::ValueType,
        pos: &Pos,
    ) -> Result<l::Operand, LowerError> {
        let actual = self.operand_type(&operand, pos)?;
        if actual == expected {
            return Ok(operand);
        }
        if let l::ValueType::Data(Type::Nullable(target)) = &expected {
            if let Type::Class(target) = target.as_ref() {
                let boundary = self
                    .lowering
                    .hir
                    .classes
                    .get(target.0)
                    .is_some_and(|definition| definition.is_value && definition.is_boundary);
                if boundary {
                    let value = match &actual {
                        l::ValueType::Address(address)
                            if address.pointee == Type::Class(*target) =>
                        {
                            self.emit(
                                l::InstructionKind::LoadAddress,
                                vec![operand.clone()],
                                Some(l::ValueType::Data(Type::Class(*target))),
                                false,
                                Vec::new(),
                                pos.clone(),
                            )?
                            .expect("boundary value load")
                        }
                        l::ValueType::Data(Type::Class(source)) if source == target => {
                            operand.clone()
                        }
                        _ => {
                            return self
                                .emit(
                                    l::InstructionKind::Coerce,
                                    vec![operand],
                                    Some(expected),
                                    false,
                                    Vec::new(),
                                    pos.clone(),
                                )?
                                .ok_or_else(|| {
                                    self.error(pos, "implicit coercion produced no value")
                                });
                        }
                    };
                    if matches!(
                        self.operand_type(&value, pos)?,
                        l::ValueType::Data(Type::Class(source)) if source == *target
                    ) {
                        return self
                            .emit(
                                l::InstructionKind::BoxBoundaryValue { payload: *target },
                                vec![value],
                                Some(expected),
                                false,
                                vec![l::Trap {
                                    kind: l::TrapKind::Allocation,
                                    pos: pos.clone(),
                                }],
                                pos.clone(),
                            )?
                            .ok_or_else(|| {
                                self.error(pos, "boundary value box produced no handle")
                            });
                    }
                }
            }
        }
        let kind = match (&actual, &expected) {
            (l::ValueType::Address(address), l::ValueType::Data(result))
                if address.pointee == *result =>
            {
                l::InstructionKind::LoadAddress
            }
            _ => l::InstructionKind::Coerce,
        };
        self.emit(
            kind,
            vec![operand],
            Some(expected),
            false,
            Vec::new(),
            pos.clone(),
        )?
        .ok_or_else(|| self.error(pos, "implicit coercion produced no value"))
    }
}
