//! Layouts, allocation, addresses, and value storage.

use super::*;

impl Interpreter<'_> {
    pub(super) fn compute_class_layouts(&mut self) -> Result<(), InterpretError> {
        let class_ids = self
            .module
            .classes
            .iter()
            .map(|class| class.id)
            .collect::<Vec<_>>();
        for id in class_ids {
            let _ = self.class_layout(id)?;
        }
        let mut types = Vec::new();
        for class in &self.module.classes {
            types.push(Type::Class(class.id));
            types.extend(class.fields.iter().map(|field| field.ty.clone()));
        }
        types.extend(self.module.globals.iter().map(|global| global.ty.clone()));
        for foreign in &self.module.foreign_functions {
            types.push(foreign.return_type.clone());
            types.extend(
                foreign
                    .parameters
                    .iter()
                    .map(|parameter| parameter.ty.clone()),
            );
        }
        for function in &self.module.functions {
            types.push(function.return_type.clone());
            types.extend(function.locals.iter().map(|local| match &local.ty {
                l::ValueType::Data(ty)
                | l::ValueType::Address(l::AddressType { pointee: ty, .. }) => ty.clone(),
                l::ValueType::Iterator(iterator) => iterator.element.clone(),
            }));
            types.extend(function.values.iter().map(|value| match &value.ty {
                l::ValueType::Data(ty)
                | l::ValueType::Address(l::AddressType { pointee: ty, .. }) => ty.clone(),
                l::ValueType::Iterator(iterator) => iterator.element.clone(),
            }));
        }
        while let Some(ty) = types.pop() {
            types.extend(ty.contained_types().into_iter().cloned());
            let _ = self.type_layout(&ty)?;
        }
        Ok(())
    }

    fn class_layout(&mut self, id: ClassId) -> Result<Layout, InterpretError> {
        if let Some(layout) = self.class_layouts.get(&id) {
            return Ok(Layout {
                size: layout.size,
                align: layout.align,
            });
        }
        let class = self
            .module
            .classes
            .get(id.0)
            .filter(|class| class.id == id)
            .cloned()
            .ok_or_else(|| self.invalid(None, format!("class {:?} is missing", id)))?;
        // Insert a sentinel before recursion so an invalid by-value cycle is
        // diagnosed rather than recursing indefinitely.
        self.class_layouts.insert(id, Layout { size: 0, align: 1 });
        let mut offset = 0usize;
        let mut aggregate_align = 1usize;
        let mut padding = Vec::new();
        for field in &class.fields {
            let layout = self.type_layout(&field.ty)?;
            let field_offset = align_up(offset, layout.align);
            if offset < field_offset {
                padding.push(offset..field_offset);
            }
            if let Some(field_padding) = self.padding_cache.get(&format!("{:?}", field.ty)) {
                padding.extend(
                    field_padding
                        .iter()
                        .map(|range| field_offset + range.start..field_offset + range.end),
                );
            }
            offset = field_offset;
            self.field_layouts
                .insert(field.id, (offset, field.ty.clone()));
            offset = offset.checked_add(layout.size).ok_or_else(|| {
                self.invalid(Some(field.pos.clone()), "class layout overflows host usize")
            })?;
            aggregate_align = aggregate_align.max(layout.align);
        }
        if let Some(override_align) = class.alignment {
            aggregate_align = aggregate_align.max(override_align as usize);
        }
        let layout = Layout {
            size: align_up(offset, aggregate_align),
            align: aggregate_align,
        };
        if offset < layout.size {
            padding.push(offset..layout.size);
        }
        self.class_layouts.insert(
            id,
            Layout {
                size: layout.size,
                align: layout.align,
            },
        );
        self.padding_cache
            .insert(format!("{:?}", Type::Class(id)), padding);
        Ok(layout)
    }

    pub(super) fn type_layout(&mut self, ty: &Type) -> Result<Layout, InterpretError> {
        let key = format!("{ty:?}");
        if let Some(layout) = self.layout_cache.get(&key).copied() {
            return Ok(layout);
        }
        let (layout, padding) = if let Some((size, align)) = scalar_size_align(ty) {
            (
                Layout {
                    size: size as usize,
                    align: align as usize,
                },
                Vec::new(),
            )
        } else {
            match ty {
                Type::Class(id) => {
                    let class =
                        self.module.classes.get(id.0).ok_or_else(|| {
                            self.invalid(None, format!("class {:?} is missing", id))
                        })?;
                    if class.is_value {
                        let layout = self.class_layout(*id)?;
                        let padding = self.padding_cache.get(&key).cloned().unwrap_or_default();
                        (layout, padding)
                    } else {
                        (Layout { size: 8, align: 8 }, Vec::new())
                    }
                }
                Type::FixedArray(element_ty, count) => {
                    let element_key = format!("{element_ty:?}");
                    let element = self.type_layout(element_ty)?;
                    let stride = align_up(element.size, element.align);
                    let element_padding = self
                        .padding_cache
                        .get(&element_key)
                        .cloned()
                        .unwrap_or_default();
                    let mut padding = Vec::new();
                    for index in 0..*count as usize {
                        let base = index * stride;
                        padding.extend(
                            element_padding
                                .iter()
                                .map(|range| base + range.start..base + range.end),
                        );
                        if element.size < stride {
                            padding.push(base + element.size..base + stride);
                        }
                    }
                    (
                        Layout {
                            size: stride.checked_mul(*count as usize).ok_or_else(|| {
                                self.invalid(None, "fixed-array layout overflows host usize")
                            })?,
                            align: element.align,
                        },
                        padding,
                    )
                }
                Type::IterResult(value) => {
                    // The done guard reads the Boolean field (compiler.md §145).
                    let _ = self.type_layout(&Type::Bool)?;
                    let value = self.type_layout(value)?;
                    let value_offset = align_up(1, value.align);
                    (
                        Layout {
                            size: align_up(value_offset + value.size, value.align),
                            align: value.align,
                        },
                        Vec::new(),
                    )
                }
                other => return Err(self.invalid(None, format!("no storage layout for {other:?}"))),
            }
        };
        self.layout_cache.insert(key, layout);
        self.padding_cache.insert(format!("{ty:?}"), padding);
        Ok(layout)
    }

    pub(super) fn zero(&self, ty: &Type) -> Value {
        match ty {
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Enum(_)
            | Type::StringAlias(_)
            | Type::Date => Value::I(0),
            Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::F16 => Value::U(0),
            Type::F32 => Value::F32(0.0),
            Type::F64 => Value::F64(0.0),
            Type::Bool => Value::Bool(false),
            Type::Class(id)
                if self
                    .module
                    .classes
                    .get(id.0)
                    .is_some_and(|class| class.is_value) =>
            {
                let size = self.class_layouts.get(id).map_or(0, |layout| layout.size);
                Value::Blob(vec![0; size])
            }
            Type::FixedArray(_, _) | Type::IterResult(_) => {
                // Layouts are already computed by `new`; zero-sized fallback is
                // rejected when the value is used if the module is inconsistent.
                let size = self.layout_cached(ty).map_or(0, |layout| layout.size);
                Value::Blob(vec![0; size])
            }
            Type::Void => Value::Void,
            Type::Null
            | Type::Str
            | Type::RegExp
            | Type::TaskGroup
            | Type::Object
            | Type::Class(_)
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Worker(_, _)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Func(_)
            | Type::Nullable(_)
            | Type::Generator(_)
            | Type::AsyncHandle(_) => Value::Null,
            Type::Error => Value::Void,
            _ => Value::Void,
        }
    }

    pub(super) fn layout_cached(&self, ty: &Type) -> Option<Layout> {
        self.layout_cache.get(&format!("{ty:?}")).copied()
    }

    pub(super) fn address_pointee(
        &self,
        result_ty: Option<&l::ValueType>,
        instruction: &l::Instruction,
    ) -> Result<Type, InterpretError> {
        match result_ty {
            Some(l::ValueType::Address(address)) => Ok(address.pointee.clone()),
            _ => Err(self.invalid(
                Some(instruction.pos.clone()),
                format!("{:?} has no address result type", instruction.kind),
            )),
        }
    }

    pub(super) fn data_result_type<'a>(
        &self,
        result_ty: Option<&'a l::ValueType>,
        instruction: &l::Instruction,
    ) -> Result<&'a Type, InterpretError> {
        match result_ty {
            Some(l::ValueType::Data(ty)) => Ok(ty),
            _ => Err(self.invalid(
                Some(instruction.pos.clone()),
                format!("{:?} has no data result type", instruction.kind),
            )),
        }
    }

    pub(super) fn allocate_class(
        &mut self,
        id: ClassId,
        result_ty: Option<&l::ValueType>,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let class = self
            .module
            .classes
            .get(id.0)
            .filter(|class| class.id == id)
            .ok_or_else(|| self.invalid(Some(pos.clone()), format!("class {:?} is missing", id)))?;
        let layout = self.class_layouts.get(&id).ok_or_else(|| {
            self.invalid(Some(pos.clone()), format!("class {:?} has no layout", id))
        })?;
        if class.is_value {
            if !matches!(
                result_ty,
                Some(l::ValueType::Address(l::AddressType {
                    pointee: Type::Class(result),
                    array_base: None,
                })) if *result == id
            ) {
                return Err(self.invalid(
                    Some(pos.clone()),
                    "value-class allocation result type is inconsistent",
                ));
            }
            return Ok(Value::Address(Address {
                target: AddressTarget::Slot(Rc::new(RefCell::new(Value::Blob(vec![
                    0;
                    layout.size
                ])))),
                pointee: Type::Class(id),
                poison: Rc::new(RefCell::new(None)),
            }));
        }
        if !matches!(result_ty, Some(l::ValueType::Data(Type::Class(result))) if *result == id) {
            return Err(self.invalid(
                Some(pos.clone()),
                "AllocateClass result type is inconsistent",
            ));
        }
        let handle = self.context.alloc(layout.size, id.0 as u32, 0);
        self.check_runtime(pos)?;
        if !class.field_releases.is_empty() {
            let layouts = &self.layouts;
            let description = crate::counted::class_description(layouts, class)
                .map_err(|message| self.invalid(Some(pos.clone()), message))?;
            // SAFETY: the resolved class layout supplies all offsets and recursive nodes.
            unsafe { self.context.describe_object(handle as usize, &description) };
        }

        Ok(Value::Handle(handle))
    }

    pub(super) fn box_boundary_value(
        &mut self,
        value: &Value,
        payload: ClassId,
        result_ty: Option<&l::ValueType>,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let Some(l::ValueType::Data(Type::Nullable(inner))) = result_ty else {
            return Err(self.invalid(
                Some(pos.clone()),
                "BoxBoundaryValue result type is inconsistent",
            ));
        };
        let Type::Class(_) = inner.as_ref() else {
            return Err(self.invalid(Some(pos.clone()), "BoxBoundaryValue target is not a class"));
        };
        let layout = self.class_layouts.get(&payload).ok_or_else(|| {
            self.invalid(
                Some(pos.clone()),
                format!("class {:?} has no box layout", payload),
            )
        })?;
        let bytes = self.pack(&Type::Class(payload), value)?;
        let handle = self.context.alloc(layout.size, payload.0 as u32, 0);
        self.check_runtime(pos)?;
        if !handle.is_null() && !bytes.is_empty() {
            // SAFETY: the Context allocated exactly `layout.size` payload
            // bytes, and `pack` returns that class layout's exact byte image.
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), handle, bytes.len()) };
        }

        Ok(Value::Handle(handle))
    }

    pub(super) fn alloc_string(
        &mut self,
        bytes: &[u8],
        pos: &Pos,
    ) -> Result<*mut u8, InterpretError> {
        let handle = self.context.alloc_str(bytes, 0);
        self.check_runtime(pos)?;

        Ok(handle)
    }

    pub(super) fn string_bytes(&self, handle: *mut u8) -> Result<Vec<u8>, InterpretError> {
        if handle.is_null() {
            return Err(self.invalid(None, "null used as a string"));
        }
        // SAFETY: handles in interpreter values are produced by this Context.
        Ok(unsafe { self.context.str_bytes(handle) }.to_vec())
    }

    fn field_info(&self, field: l::FieldRef) -> Result<(usize, Type), InterpretError> {
        match field {
            l::FieldRef::Class(id) => self
                .field_layouts
                .get(&id)
                .cloned()
                .ok_or_else(|| self.invalid(None, format!("field {} has no layout", id.0))),
            l::FieldRef::IterDone => Ok((0, Type::Bool)),
            l::FieldRef::IterValue => Err(self.invalid(
                None,
                "IterResult.value requires its result address type to determine the offset",
            )),
        }
    }

    pub(super) fn address_field(
        &self,
        base: &Value,
        field: l::FieldRef,
        result_ty: Option<&l::ValueType>,
        instruction: &l::Instruction,
    ) -> Result<Address, InterpretError> {
        let pointee = match result_ty {
            Some(l::ValueType::Address(address)) => address.pointee.clone(),
            Some(l::ValueType::Data(ty)) => ty.clone(),
            _ => match field {
                l::FieldRef::Class(id) => self.field_info(l::FieldRef::Class(id))?.1,
                l::FieldRef::IterDone => Type::Bool,
                l::FieldRef::IterValue => {
                    return Err(self.invalid(
                        Some(instruction.pos.clone()),
                        "LoadField IterValue has no result type",
                    ));
                }
            },
        };
        let offset = self.field_offset(field, &pointee, instruction)?;
        match base {
            Value::Handle(handle) if !handle.is_null() => Ok(Address {
                // SAFETY: the verified field layout is within this class payload.
                target: AddressTarget::Pointer(unsafe { handle.add(offset) }),
                pointee,
                poison: Rc::new(RefCell::new(None)),
            }),
            Value::Address(address) => {
                address.check(&instruction.kind)?;
                Ok(Address {
                    target: match &address.target {
                        AddressTarget::Slot(slot) => AddressTarget::SlotBytes {
                            slot: slot.clone(),
                            offset,
                        },
                        AddressTarget::SlotBytes { slot, offset: base } => {
                            AddressTarget::SlotBytes {
                                slot: slot.clone(),
                                offset: base + offset,
                            }
                        }
                        AddressTarget::Pointer(pointer) => {
                            // SAFETY: the verified nested field layout is in bounds.
                            AddressTarget::Pointer(unsafe { pointer.add(offset) })
                        }
                    },
                    pointee,
                    poison: address.poison.clone(),
                })
            }
            other => Err(type_error("class handle or aggregate address", other)),
        }
    }

    pub(super) fn field_offset(
        &self,
        field: l::FieldRef,
        pointee: &Type,
        instruction: &l::Instruction,
    ) -> Result<usize, InterpretError> {
        Ok(match field {
            l::FieldRef::Class(id) => self.field_info(l::FieldRef::Class(id))?.0,
            l::FieldRef::IterDone => 0,
            l::FieldRef::IterValue => {
                let layout = self.layout_cached(pointee).ok_or_else(|| {
                    self.invalid(
                        Some(instruction.pos.clone()),
                        "IterResult.value type has no layout",
                    )
                })?;
                align_up(1, layout.align)
            }
        })
    }

    pub(super) fn address_index(
        &mut self,
        base: &Value,
        index: i64,
        result_ty: Option<&l::ValueType>,
        instruction: &l::Instruction,
    ) -> Result<Address, InterpretError> {
        let pointee = self.address_pointee(result_ty, instruction)?;
        let poison = Rc::new(RefCell::new(None));
        match base {
            Value::Handle(array) => {
                // The runtime owns bounds checks and the dynamic storage.
                // SAFETY: the handle is a live runtime array and `array_elem_ptr`
                // validates the index before returning an element pointer.
                let pointer = unsafe {
                    self.context
                        .array_elem_ptr(*array, i32::try_from(index).unwrap_or(i32::MIN), 0)
                };
                self.check_runtime(&instruction.pos)?;
                Ok(Address {
                    target: AddressTarget::Pointer(pointer),
                    pointee,
                    poison,
                })
            }
            Value::Address(address) => {
                address.check(&instruction.kind)?;
                let layout = self.layout_cached(&pointee).ok_or_else(|| {
                    self.invalid(
                        Some(instruction.pos.clone()),
                        "indexed element has no layout",
                    )
                })?;
                let offset = usize::try_from(index)
                    .ok()
                    .and_then(|index| index.checked_mul(align_up(layout.size, layout.align)))
                    .ok_or_else(|| {
                        self.invalid(
                            Some(instruction.pos.clone()),
                            "fixed-array index is negative or overflows",
                        )
                    })?;
                Ok(Address {
                    target: match &address.target {
                        AddressTarget::Slot(slot) => AddressTarget::SlotBytes {
                            slot: slot.clone(),
                            offset,
                        },
                        AddressTarget::SlotBytes { slot, offset: base } => {
                            AddressTarget::SlotBytes {
                                slot: slot.clone(),
                                offset: base + offset,
                            }
                        }
                        AddressTarget::Pointer(pointer) => {
                            // SAFETY: fixed-array bounds are guarded by the LIR trap site.
                            AddressTarget::Pointer(unsafe { pointer.add(offset) })
                        }
                    },
                    pointee,
                    poison: address.poison.clone(),
                })
            }
            other => Err(type_error("array handle or fixed-array address", other)),
        }
    }

    pub(super) fn load_address(&self, address: &Address) -> Result<Value, InterpretError> {
        match &address.target {
            AddressTarget::Slot(slot) => Ok(slot.borrow().clone()),
            AddressTarget::SlotBytes { slot, offset } => {
                let borrowed = slot.borrow();
                let Value::Blob(bytes) = &*borrowed else {
                    return Err(type_error("aggregate storage", &borrowed));
                };
                self.unpack(&address.pointee, bytes.get(*offset..).unwrap_or_default())
            }
            AddressTarget::Pointer(pointer) => {
                let layout = self.layout_cached(&address.pointee).ok_or_else(|| {
                    self.invalid(
                        None,
                        format!("no layout for address pointee {:?}", address.pointee),
                    )
                })?;
                if pointer.is_null() {
                    return Err(self.invalid(None, "null address"));
                }
                // SAFETY: the address was derived from verified storage and covers
                // the pointee layout.
                let bytes = unsafe { std::slice::from_raw_parts(*pointer, layout.size) };
                self.unpack(&address.pointee, bytes)
            }
        }
    }

    pub(super) fn store_address(
        &self,
        address: &Address,
        value: &Value,
    ) -> Result<(), InterpretError> {
        match &address.target {
            AddressTarget::Slot(slot) => {
                *slot.borrow_mut() = value.clone();
                Ok(())
            }
            AddressTarget::SlotBytes { slot, offset } => {
                let mut borrowed = slot.borrow_mut();
                let Value::Blob(bytes) = &mut *borrowed else {
                    return Err(type_error("aggregate storage", &borrowed));
                };
                self.pack_into(
                    &address.pointee,
                    value,
                    bytes.get_mut(*offset..).unwrap_or_default(),
                )
            }
            AddressTarget::Pointer(pointer) => {
                let layout = self.layout_cached(&address.pointee).ok_or_else(|| {
                    self.invalid(
                        None,
                        format!("no layout for address pointee {:?}", address.pointee),
                    )
                })?;
                if pointer.is_null() {
                    return Err(self.invalid(None, "null address"));
                }
                // SAFETY: the address was derived from writable verified storage.
                let bytes = unsafe { std::slice::from_raw_parts_mut(*pointer, layout.size) };
                self.pack_into(&address.pointee, value, bytes)
            }
        }
    }

    pub(super) fn zero_padding(
        &self,
        ty: &Type,
        bytes: &mut [u8],
        base: usize,
    ) -> Result<(), InterpretError> {
        let ranges = self
            .padding_cache
            .get(&format!("{ty:?}"))
            .ok_or_else(|| self.invalid(None, format!("{ty:?} has no padding layout")))?;
        for range in ranges {
            self.zero_byte_range(bytes, base + range.start, base + range.end)?;
        }
        Ok(())
    }

    fn zero_byte_range(
        &self,
        bytes: &mut [u8],
        start: usize,
        end: usize,
    ) -> Result<(), InterpretError> {
        let range = bytes
            .get_mut(start..end)
            .ok_or_else(|| self.invalid(None, "padding range exceeds aggregate storage"))?;
        range.fill(0);
        Ok(())
    }

    pub(super) fn pack(&self, ty: &Type, value: &Value) -> Result<Vec<u8>, InterpretError> {
        let layout = self
            .layout_cached(ty)
            .ok_or_else(|| self.invalid(None, format!("no layout for {ty:?}")))?;
        let mut bytes = vec![0; layout.size];
        self.pack_into(ty, value, &mut bytes)?;
        Ok(bytes)
    }

    pub(super) fn pack_into(
        &self,
        ty: &Type,
        value: &Value,
        out: &mut [u8],
    ) -> Result<(), InterpretError> {
        if let Type::Nullable(inner) = ty {
            if inner.function_type().is_some() {
                return self.pack_into(inner, value, out);
            }
        }
        let layout = self
            .layout_cached(ty)
            .ok_or_else(|| self.invalid(None, format!("no layout for {ty:?}")))?;
        if out.len() < layout.size {
            return Err(self.invalid(None, format!("storage for {ty:?} is too short")));
        }
        match ty {
            Type::I8 => out[0] = value.as_i64()? as i8 as u8,
            Type::U8 | Type::Bool => {
                out[0] = if matches!(ty, Type::Bool) {
                    u8::from(value.as_bool()?)
                } else {
                    value.as_u64()? as u8
                }
            }
            Type::I16 => out[..2].copy_from_slice(&(value.as_i64()? as i16).to_ne_bytes()),
            Type::U16 | Type::F16 => {
                out[..2].copy_from_slice(&(value.as_u64()? as u16).to_ne_bytes())
            }
            Type::I32 | Type::Enum(_) | Type::StringAlias(_) => {
                out[..4].copy_from_slice(&(value.as_i64()? as i32).to_ne_bytes())
            }
            Type::U32 => out[..4].copy_from_slice(&(value.as_u64()? as u32).to_ne_bytes()),
            Type::I64 | Type::Date => out[..8].copy_from_slice(&value.as_i64()?.to_ne_bytes()),
            Type::U64 => out[..8].copy_from_slice(&value.as_u64()?.to_ne_bytes()),
            Type::F32 => {
                let Value::F32(value) = value else {
                    return Err(type_error("f32", value));
                };
                out[..4].copy_from_slice(&value.to_bits().to_ne_bytes());
            }
            Type::F64 => out[..8].copy_from_slice(&value.as_f64()?.to_bits().to_ne_bytes()),
            Type::Class(id)
                if self
                    .module
                    .classes
                    .get(id.0)
                    .is_some_and(|class| class.is_value) =>
            {
                let Value::Blob(value) = value else {
                    return Err(type_error("aggregate", value));
                };
                if value.len() != layout.size {
                    return Err(self.invalid(None, "aggregate size disagrees with its type"));
                }
                out[..layout.size].copy_from_slice(value);
            }
            Type::FixedArray(_, _) | Type::IterResult(_) => {
                let Value::Blob(value) = value else {
                    return Err(type_error("aggregate", value));
                };
                if value.len() != layout.size {
                    return Err(self.invalid(None, "aggregate size disagrees with its type"));
                }
                out[..layout.size].copy_from_slice(value);
            }
            Type::AsyncHandle(_) => {
                let Value::Coroutine(handle) = value else {
                    return Err(type_error("async handle", value));
                };
                let key = Rc::as_ptr(handle) as usize as u64;
                out[..8].copy_from_slice(&key.to_ne_bytes());
            }
            // §106.3 rules 2 and 4 own this arm.
            Type::Generator(_) => {
                let key = match value {
                    Value::Coroutine(handle) => Rc::as_ptr(handle) as usize as u64,
                    Value::Null => 0,
                    other => return Err(type_error("generator", other)),
                };
                out[..8].copy_from_slice(&key.to_ne_bytes());
            }
            Type::Str
            | Type::RegExp
            | Type::TaskGroup
            | Type::Object
            | Type::Class(_)
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Nullable(_)
            | Type::Worker(_, _)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Null => {
                let handle = value.as_handle()? as usize as u64;
                out[..8].copy_from_slice(&handle.to_ne_bytes());
            }
            Type::Func(_) => {
                if matches!(value, Value::Null) {
                    out[..16].fill(0);
                    return Ok(());
                }
                let Value::Callable(callable) = value else {
                    return Err(type_error("callable", value));
                };
                let environment = match callable.captures.as_slice() {
                    [] => 0,
                    [Value::Handle(pointer)]
                        if self.function(callable.function)?.parameters.iter().any(
                            |parameter| parameter.kind == l::ParameterKind::OwnedEnvironment,
                        ) =>
                    {
                        *pointer as usize as u64
                    }
                    _ => {
                        return Err(self.invalid(None, "a stored function must own its environment"))
                    }
                };
                out[..8].copy_from_slice(&(callable.function.0 as u64 + 1).to_ne_bytes());
                out[8..16].copy_from_slice(&environment.to_ne_bytes());
            }
            Type::Void | Type::Error => {}
            _ => return Err(self.invalid(None, format!("packing {ty:?} is not defined"))),
        }
        Ok(())
    }

    pub(super) fn unpack(&self, ty: &Type, bytes: &[u8]) -> Result<Value, InterpretError> {
        if let Type::Nullable(inner) = ty {
            if inner.function_type().is_some() {
                return self.unpack(inner, bytes);
            }
        }
        let need = self
            .layout_cached(ty)
            .ok_or_else(|| self.invalid(None, format!("no layout for {ty:?}")))?
            .size;
        if bytes.len() < need {
            return Err(self.invalid(None, format!("storage for {ty:?} is too short")));
        }
        Ok(match ty {
            Type::I8 => Value::I(i8::from_ne_bytes([bytes[0]]) as i64),
            Type::U8 => Value::U(bytes[0] as u64),
            Type::Bool => Value::Bool(bytes[0] != 0),
            Type::I16 => {
                Value::I(i16::from_ne_bytes(bytes[..2].try_into().unwrap_or([0; 2])) as i64)
            }
            Type::U16 | Type::F16 => {
                Value::U(u16::from_ne_bytes(bytes[..2].try_into().unwrap_or([0; 2])) as u64)
            }
            Type::I32 | Type::Enum(_) | Type::StringAlias(_) => {
                Value::I(i32::from_ne_bytes(bytes[..4].try_into().unwrap_or([0; 4])) as i64)
            }
            Type::U32 => {
                Value::U(u32::from_ne_bytes(bytes[..4].try_into().unwrap_or([0; 4])) as u64)
            }
            Type::I64 | Type::Date => {
                Value::I(i64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8])))
            }
            Type::U64 => Value::U(u64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8]))),
            Type::F32 => Value::F32(f32::from_bits(u32::from_ne_bytes(
                bytes[..4].try_into().unwrap_or([0; 4]),
            ))),
            Type::F64 => Value::F64(f64::from_bits(u64::from_ne_bytes(
                bytes[..8].try_into().unwrap_or([0; 8]),
            ))),
            Type::Class(id)
                if self
                    .module
                    .classes
                    .get(id.0)
                    .is_some_and(|class| class.is_value) =>
            {
                Value::Blob(bytes[..need].to_vec())
            }
            Type::FixedArray(_, _) | Type::IterResult(_) => Value::Blob(bytes[..need].to_vec()),
            Type::AsyncHandle(_) => {
                let key = u64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8])) as usize;
                if key == 0 {
                    Value::Null
                } else {
                    let handle = self
                        .async_handles
                        .borrow()
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| self.invalid(None, "unknown packed async handle"))?;
                    Value::Coroutine(handle)
                }
            }
            // §106.3 rules 2, 3 and 4 own this arm.
            Type::Generator(_) => {
                let key = u64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8])) as usize;
                if key == 0 {
                    Value::Null
                } else {
                    let handle = self
                        .generator_handles
                        .borrow()
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| self.invalid(None, "unknown packed generator"))?;
                    Value::Coroutine(handle)
                }
            }
            Type::Str
            | Type::RegExp
            | Type::TaskGroup
            | Type::Object
            | Type::Class(_)
            | Type::Array(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::Nullable(_)
            | Type::Worker(_, _)
            | Type::Inbox(_)
            | Type::Outbox(_)
            | Type::Null => {
                let handle =
                    u64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8])) as usize as *mut u8;
                if handle.is_null() {
                    Value::Null
                } else {
                    Value::Handle(handle)
                }
            }
            Type::Func(_) => {
                let code = u64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0; 8]));
                if code == 0 {
                    return Ok(Value::Null);
                }
                let index = code
                    .checked_sub(1)
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| self.invalid(None, "invalid packed function id"))?;
                let function = l::FunctionId(index);
                self.function(function)?;
                let environment = u64::from_ne_bytes(bytes[8..16].try_into().unwrap_or([0; 8]));
                Value::Callable(Rc::new(Callable {
                    function,
                    captures: if environment == 0 {
                        Vec::new()
                    } else {
                        vec![Value::Handle(environment as usize as *mut u8)]
                    },
                }))
            }
            Type::Void | Type::Error => Value::Void,
            _ => return Err(self.invalid(None, format!("unpacking {ty:?} is not defined"))),
        })
    }

    pub(super) fn copy_value(
        &self,
        value: &Value,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        if let Some(l::ValueType::Data(Type::Class(id))) = result_ty {
            if self
                .module
                .classes
                .get(id.0)
                .is_some_and(|class| class.is_value)
            {
                let Value::Blob(bytes) = value else {
                    return Err(type_error("value class", value));
                };
                return Ok(Value::Blob(bytes.clone()));
            }
        }
        Ok(value.clone())
    }
}
