//! Counted owners use the same array holder field as the native tiers.

use super::*;

impl Interpreter<'_> {
    pub(super) fn clear_array(
        &mut self,
        operands: &[Value],
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        let pos = pos
            .cloned()
            .ok_or_else(|| self.invalid(None, "array clear has no position"))?;
        let array = operands
            .first()
            .ok_or_else(|| self.invalid(Some(pos.clone()), "array clear has no receiver"))?
            .as_handle()?;
        let element = self
            .count_action
            .as_ref()
            .and_then(l::CountAction::release_type);
        let mut removed = Vec::new();
        if let Some(element) = &element {
            let size = self.type_layout(element)?.size;
            let len =
                unsafe { ffi::subscript_rt_array_len(&mut *self.context, array) }.max(0) as usize;
            let data = unsafe { ffi::subscript_rt_array_data(&*self.context, array) };
            for index in 0..len {
                // SAFETY: each element lies inside the live array storage.
                let bytes = unsafe { std::slice::from_raw_parts(data.add(index * size), size) };
                removed.push(self.unpack(element, bytes)?);
            }
        }
        // SAFETY: the array is live; this call only clears element storage.
        unsafe {
            ffi::subscript_rt_counted_array_operation(
                &mut *self.context,
                array,
                std::ptr::null(),
                3,
                std::ptr::null(),
                0,
                0,
                0,
                0,
            )
        };
        self.check_runtime(&pos)?;
        if let Some(element) = element {
            for value in removed {
                self.counted_owner(&element, &value, true, &pos)?;
            }
        }
        Ok(Value::Void)
    }

    /// Ends one holder's ownership of a handle. The last release drops the
    /// interpreter's handle table entry; the completion cache and the values
    /// reachable from it live as long as some owner holds them (§94.2).
    ///
    /// The last release of a handle that holds an exception that no `await`
    /// raised is the uncaught-exception trap (`compiler.md` §116.1 rule 4).
    pub(super) fn release_coroutine(
        &mut self,
        coroutine: &Rc<RefCell<Coroutine>>,
        pos: &Pos,
    ) -> Result<(), InterpretError> {
        let key = Rc::as_ptr(coroutine) as usize;
        let mut state = coroutine.borrow_mut();
        if state.owners != 0 {
            state.owners -= 1;
        }
        if state.owners == 0 {
            if !state.completed {
                return Ok(());
            }
            let mut unobserved = match &state.completion {
                Some(Completion::Exception(payload)) if !payload.observed => {
                    Some(payload.exception.clone())
                }
                _ => None,
            };
            if let CoroutineKind::Aggregate(aggregate) = &mut state.kind {
                if aggregate.reported {
                    unobserved = None;
                }
                if unobserved.is_some() {
                    aggregate.reported = true;
                }
            }
            let unread = match &state.kind {
                CoroutineKind::Aggregate(a) => a.inputs.iter().any(Option::is_some),
                CoroutineKind::GroupJoin(group) => {
                    group.borrow().inputs.iter().any(Option::is_some)
                }
                _ => false,
            };
            if !unread {
                if let CoroutineKind::GroupJoin(group) = &state.kind {
                    group.borrow_mut().join = None;
                }
            }
            if state.completed && !unread {
                self.async_registry.borrow_mut().remove(&state.task_id);
            }
            let payload = if !unread {
                let completion = state.completion.take();
                match (&state.kind, completion) {
                    (CoroutineKind::Invocation(frame), Some(Completion::Value(value))) => {
                        let ty = self.module.functions[frame.borrow().function.0 as usize]
                            .return_type
                            .clone();
                        Some((ty, value))
                    }
                    _ => None,
                }
            } else {
                None
            };
            drop(state);
            if let Some((ty, value)) = payload {
                if ty.counted_type().is_some() {
                    self.counted_owner(&ty, &value, true, pos)?;
                }
            }
            if !unread {
                self.async_handles.borrow_mut().remove(&key);
            }
            if let Some((object, message, pos)) = unobserved {
                return Err(InterpretError::Exception {
                    object,
                    message,
                    pos,
                }
                .settled());
            }
        }
        Ok(())
    }

    pub(super) fn read_counted_map_value(
        &mut self,
        map: *mut u8,
        value_ty: &Type,
        key: &[u8],
    ) -> Result<Option<Value>, InterpretError> {
        if value_ty.counted_type().is_none() {
            return Ok(None);
        }
        let mut bytes = vec![0; self.type_layout(value_ty)?.size];
        let context = &mut *self.context as *mut Context;
        let present =
            unsafe { ffi::subscript_rt_map_get(context, map, key.as_ptr(), bytes.as_mut_ptr()) }
                != 0;
        if present {
            Ok(Some(self.unpack(value_ty, &bytes)?))
        } else {
            Ok(None)
        }
    }

    pub(super) fn counted_map_values(
        &mut self,
        map: *mut u8,
        value_ty: &Type,
    ) -> Result<Vec<Value>, InterpretError> {
        if value_ty.counted_type().is_none() {
            return Ok(Vec::new());
        }
        let size = self.type_layout(value_ty)?.size;
        let bound = unsafe { ffi::subscript_rt_assoc_iter_begin(&mut *self.context, map, 0) };
        let result = (|| {
            let mut values = Vec::new();
            let mut bytes = vec![0; size];
            for index in 0..bound {
                if unsafe {
                    ffi::subscript_rt_assoc_iter_copy(
                        &mut *self.context,
                        map,
                        index,
                        1,
                        bytes.as_mut_ptr(),
                        0,
                    )
                } != 0
                {
                    values.push(self.unpack(value_ty, &bytes)?);
                }
            }
            Ok(values)
        })();
        unsafe { ffi::subscript_rt_assoc_iter_end(&mut *self.context, map) };
        result
    }

    pub(super) fn counted_array_elements(
        &mut self,
        array: *mut u8,
        element: &Type,
        release: bool,
        pos: &Pos,
    ) -> Result<(), InterpretError> {
        let size = self.type_layout(element)?.size;
        let len = unsafe { ffi::subscript_rt_array_len(&mut *self.context, array) }.max(0) as usize;
        let data = unsafe { ffi::subscript_rt_array_data(&*self.context, array) };
        for index in 0..len {
            let bytes = unsafe { std::slice::from_raw_parts(data.add(index * size), size) };
            let value = self.unpack(element, bytes)?;
            self.counted_owner(element, &value, release, pos)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn replace_counted_array_range(
        &mut self,
        array: *mut u8,
        element: &Type,
        fill: Option<&Value>,
        target: i32,
        start: i32,
        end: i32,
        pos: &Pos,
    ) -> Result<(), InterpretError> {
        let size = self.type_layout(element)?.size;
        let len = unsafe { ffi::subscript_rt_array_len(&mut *self.context, array) }.max(0) as usize;
        let clamp = |index: i32| {
            if index < 0 {
                (len as i64 + i64::from(index)).max(0) as usize
            } else {
                (index as usize).min(len)
            }
        };
        let from = clamp(start);
        let to = if fill.is_some() { from } else { clamp(target) };
        let count = clamp(end).saturating_sub(from).min(len - to);
        if count == 0 {
            return Ok(());
        }
        let data = unsafe { ffi::subscript_rt_array_data(&*self.context, array) }.cast_mut();
        let source = if let Some(value) = fill {
            self.pack(element, value)?
        } else {
            unsafe { std::slice::from_raw_parts(data.add(from * size), count * size) }.to_vec()
        };
        let replaced =
            unsafe { std::slice::from_raw_parts(data.add(to * size), count * size) }.to_vec();
        for index in 0..count {
            let offset = if fill.is_some() { 0 } else { index * size };
            let value = self.unpack(element, &source[offset..offset + size])?;
            self.counted_owner(element, &value, false, pos)?;
        }
        for index in 0..count {
            let offset = if fill.is_some() { 0 } else { index * size };
            unsafe {
                std::ptr::copy_nonoverlapping(
                    source.as_ptr().add(offset),
                    data.add((to + index) * size),
                    size,
                )
            };
        }
        for index in 0..count {
            let value = self.unpack(element, &replaced[index * size..(index + 1) * size])?;
            self.counted_owner(element, &value, true, pos)?;
        }
        Ok(())
    }

    pub(super) fn counted_owner(
        &mut self,
        ty: &Type,
        value: &Value,
        release: bool,
        pos: &Pos,
    ) -> Result<(), InterpretError> {
        match ty {
            Type::AsyncHandle(_) => {
                if let Value::Coroutine(handle) = value {
                    if release {
                        self.release_coroutine(handle, pos)?;
                    } else {
                        let mut handle = handle.borrow_mut();
                        handle.owners = handle.owners.saturating_add(1);
                    }
                } else if !matches!(value, Value::Null) {
                    return Err(
                        self.invalid(Some(pos.clone()), "counted value is not an async handle")
                    );
                }
            }
            Type::Array(element) => {
                let array = value.as_handle()?;
                let last = unsafe {
                    ffi::subscript_rt_array_holder(&mut *self.context, array, u32::from(release), 0)
                };
                if last == 0 {
                    return Ok(());
                }
                let len = unsafe { ffi::subscript_rt_array_len(&mut *self.context, array) }.max(0)
                    as usize;
                let data = unsafe { ffi::subscript_rt_array_data(&*self.context, array) };
                let size = self.type_layout(element)?.size;
                let mut first = None;
                for index in 0..len {
                    let bytes = unsafe { std::slice::from_raw_parts(data.add(index * size), size) };
                    let value = self.unpack(element, bytes)?;
                    if let Err(error) = self.counted_owner(element, &value, true, pos) {
                        if first.is_none() {
                            first = Some(error);
                        }
                    }
                }
                unsafe { ffi::subscript_rt_array_holder(&mut *self.context, array, 2, 0) };
                if let Some(error) = first {
                    return Err(error);
                }
            }
            Type::FixedArray(element, count) => {
                let bytes = self.pack(ty, value)?;
                let size = self.type_layout(element)?.size;
                let mut first = None;
                for index in 0..*count as usize {
                    let value = self.unpack(element, &bytes[index * size..(index + 1) * size])?;
                    if let Err(error) = self.counted_owner(element, &value, release, pos) {
                        if first.is_none() {
                            first = Some(error);
                        }
                    }
                }
                if let Some(error) = first {
                    return Err(error);
                }
            }
            Type::IterResult(element) => {
                let bytes = self.pack(ty, value)?;
                if bytes.first() == Some(&0) {
                    let offset = self.type_layout(element)?.align;
                    let value = self.unpack(element, &bytes[offset..])?;
                    self.counted_owner(element, &value, release, pos)?;
                }
            }
            _ => {
                return Err(
                    self.invalid(Some(pos.clone()), "owner operation requires a counted type")
                )
            }
        }
        Ok(())
    }
}

impl Interpreter<'_> {
    pub(super) fn free_counted_map(
        &mut self,
        operands: &[Value],
        parameter_types: &[l::ValueType],
        pos: &Pos,
    ) -> Result<(), InterpretError> {
        let Some(l::ValueType::Data(Type::Map(_, value_ty))) = parameter_types.first() else {
            return Ok(());
        };
        if value_ty.counted_type().is_none() {
            return Ok(());
        }
        let map = operands
            .first()
            .ok_or_else(|| self.invalid(Some(pos.clone()), "Map free has no receiver"))?
            .as_handle()?;
        if !self.context.is_live(map as usize) {
            self.context.delete(map as usize, 0);
            return self.check_runtime(pos);
        }
        let values = self.counted_map_values(map, value_ty)?;
        unsafe { ffi::subscript_rt_assoc_clear(&mut *self.context, map) };
        let mut first = None;
        for value in values {
            if let Err(error) = self.counted_owner(value_ty, &value, true, pos) {
                if first.is_none() {
                    first = Some(error);
                }
            }
        }
        self.context.delete(map as usize, 0);
        if let Some(error) = first {
            Err(error)
        } else {
            Ok(())
        }
    }
}

impl Interpreter<'_> {
    pub(super) fn free_counted_object(
        &mut self,
        operands: &[Value],
        pos: &Pos,
    ) -> Result<(), InterpretError> {
        let handle = operands
            .first()
            .ok_or_else(|| self.invalid(Some(pos.clone()), "object free has no receiver"))?
            .as_handle()?;
        let (leaves, storage) = self
            .context
            .take_object_releases(handle as usize, 0)
            .map_err(|message| self.invalid(Some(pos.clone()), message))?;
        let mut first = None;
        for leaf in leaves {
            let coroutine = self.async_handles.borrow().get(&leaf).cloned();
            if let Some(coroutine) = coroutine {
                if let Err(error) = self.release_coroutine(&coroutine, pos) {
                    if first.is_none() {
                        first = Some(error);
                    }
                }
            } else if leaf != 0 && first.is_none() {
                first = Some(self.invalid(Some(pos.clone()), "unknown packed async handle"));
            }
        }
        for address in storage {
            self.context.delete(address, 0);
        }
        // Retire the object even if a leaf reports an unobserved exception.
        self.context.delete(handle as usize, 0);
        first.map_or(Ok(()), Err)
    }
}
