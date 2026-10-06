//! Array, map, and set intrinsics.

use super::*;

impl Interpreter<'_> {
    pub(super) fn intrinsic_array(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
        parameter_types: &[l::ValueType],
        result_ty: Option<&l::ValueType>,
        pos: &Pos,
    ) -> Result<Value, InterpretError> {
        let receiver_ty = match parameter_types.first() {
            Some(l::ValueType::Data(ty @ (Type::Array(_) | Type::FixedArray(_, _)))) => ty,
            _ => {
                return Err(
                    self.invalid(None, format!("Array.{operation} receiver type is missing"))
                );
            }
        };
        let element_ty = match receiver_ty {
            Type::Array(element) | Type::FixedArray(element, _) => element.as_ref(),
            _ => {
                return Err(
                    self.invalid(None, format!("Array.{operation} receiver type is invalid"))
                );
            }
        };
        if matches!(
            operation,
            "ForEach"
                | "Map"
                | "Filter"
                | "Reduce"
                | "Some"
                | "Every"
                | "FindIndex"
                | "Find"
                | "FindLast"
                | "FindLastIndex"
                | "FlatMap"
                | "Sort"
                | "ReduceRight"
        ) {
            return self.intrinsic_array_callback(
                operation,
                operands,
                receiver_ty,
                element_ty,
                result_ty,
            );
        }
        let array = operands
            .first()
            .ok_or_else(|| self.invalid(None, format!("Array.{operation} has no receiver")))?
            .as_handle()?;
        let context = &mut *self.context as *mut Context;
        let integer = |index: usize| -> Result<i32, InterpretError> {
            Ok(operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("Array.{operation} has no operand {index}"))
                })?
                .as_i64()? as i32)
        };
        let handle = |index: usize| -> Result<*mut u8, InterpretError> {
            operands
                .get(index)
                .ok_or_else(|| {
                    self.invalid(None, format!("Array.{operation} has no operand {index}"))
                })?
                .as_handle()
        };
        let packed = |index: usize| -> Result<Vec<u8>, InterpretError> {
            self.pack(
                element_ty,
                operands.get(index).ok_or_else(|| {
                    self.invalid(None, format!("Array.{operation} has no operand {index}"))
                })?,
            )
        };
        let count_type = self
            .count_action
            .as_ref()
            .and_then(l::CountAction::release_type);
        if let Some(count_type) = count_type
            .as_ref()
            .filter(|_| matches!(operation, "Fill" | "CopyWithin"))
        {
            let (fill, target, start, end) = if operation == "Fill" {
                (operands.get(1).cloned(), 0, integer(2)?, integer(3)?)
            } else {
                (None, integer(1)?, integer(2)?, integer(3)?)
            };
            self.replace_counted_array_range(
                array,
                count_type,
                fill.as_ref(),
                target,
                start,
                end,
                pos,
            )?;
            return Ok(Value::Handle(array));
        }
        let kind = array_elem_kind(element_ty, self.module);
        let value = match operation {
            "IndexOf" => {
                let needle = packed(1)?;
                // SAFETY: live array and correctly packed element.
                Value::I(unsafe {
                    ffi::subscript_rt_arr_index_of(
                        context,
                        array,
                        needle.as_ptr(),
                        kind,
                        integer(2)?,
                    )
                } as i64)
            }
            "LastIndexOf" => {
                let needle = packed(1)?;
                // SAFETY: live array and correctly packed element.
                Value::I(unsafe {
                    ffi::subscript_rt_arr_last_index_of(
                        context,
                        array,
                        needle.as_ptr(),
                        kind,
                        integer(2)?,
                    )
                } as i64)
            }
            "Includes" => {
                let needle = packed(1)?;
                // SAFETY: live array and correctly packed element.
                Value::Bool(
                    unsafe {
                        ffi::subscript_rt_arr_includes(
                            context,
                            array,
                            needle.as_ptr(),
                            kind,
                            integer(2)?,
                        )
                    } != 0,
                )
            }
            "Join" => {
                // SAFETY: live array/string; runtime owns Q14 formatting.
                Value::Handle(unsafe {
                    ffi::subscript_rt_arr_join(
                        context,
                        array,
                        handle(1)?,
                        array_fmt_kind(element_ty),
                        0,
                    )
                })
            }
            "Slice" => {
                // SAFETY: live array; runtime owns clamp/allocation.
                Value::Handle(unsafe {
                    ffi::subscript_rt_arr_slice(context, array, integer(1)?, integer(2)?, 0)
                })
            }
            "Fill" => {
                let fill = packed(1)?;
                // SAFETY: live array and correctly packed element.
                unsafe {
                    ffi::subscript_rt_arr_fill(
                        context,
                        array,
                        fill.as_ptr(),
                        integer(2)?,
                        integer(3)?,
                    )
                };
                Value::Handle(array)
            }
            "Reverse" => {
                // SAFETY: live array.
                unsafe { ffi::subscript_rt_arr_reverse(context, array) };
                Value::Handle(array)
            }
            "Concat" => {
                // SAFETY: live equal-width arrays.
                Value::Handle(unsafe {
                    ffi::subscript_rt_arr_concat(context, array, handle(1)?, 0)
                })
            }
            "Splice" => {
                // SAFETY: live array; runtime owns structural mutation.
                Value::Handle(unsafe {
                    ffi::subscript_rt_arr_splice(context, array, integer(1)?, integer(2)?, 0)
                })
            }
            "Shift" | "At" => {
                let layout = self
                    .layout_cached(element_ty)
                    .ok_or_else(|| self.invalid(None, "shift element has no layout"))?;
                let mut bytes = vec![0; layout.size];
                // SAFETY: live array and writable element storage.
                unsafe {
                    if operation == "At" {
                        ffi::subscript_rt_arr_at(
                            context,
                            array,
                            integer(1)?,
                            bytes.as_mut_ptr(),
                            0,
                        );
                    } else {
                        ffi::subscript_rt_arr_shift(context, array, bytes.as_mut_ptr(), 0);
                    }
                };
                self.check_runtime(pos)?;
                self.unpack(element_ty, &bytes)?
            }
            "Unshift" => {
                let value = packed(1)?;
                // SAFETY: live array and correctly packed element.
                Value::I(
                    unsafe { ffi::subscript_rt_arr_unshift(context, array, value.as_ptr(), 0) }
                        as i64,
                )
            }
            "CopyWithin" => {
                // SAFETY: live array; runtime owns overlapping copy/clamps.
                unsafe {
                    ffi::subscript_rt_arr_copy_within(
                        context,
                        array,
                        integer(1)?,
                        integer(2)?,
                        integer(3)?,
                    )
                };
                Value::Handle(array)
            }
            _ => return Err(self.invalid(None, format!("unknown Array intrinsic {operation}"))),
        };
        self.check_runtime(pos)?;
        if let Some(count_type) = count_type
            .as_ref()
            .filter(|_| matches!(operation, "Slice" | "Concat"))
        {
            self.counted_array_elements(value.as_handle()?, count_type, false, pos)?;
        }
        if let Value::Handle(handle) = value {
            if handle != array || matches!(operation, "Slice" | "Concat" | "Splice") {
                self.root_handle(handle);
            }
            Ok(Value::Handle(handle))
        } else {
            let _ = result_ty;
            Ok(value)
        }
    }

    fn intrinsic_array_callback(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
        receiver_ty: &Type,
        element_ty: &Type,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        let receiver = operands
            .first()
            .cloned()
            .ok_or_else(|| self.invalid(None, format!("Array.{operation} has no receiver")))?;
        let callable = self.callable_operand(operands.get(1), &format!("Array.{operation}"))?;
        let arity = self
            .function(callable.function)?
            .parameters
            .iter()
            .filter(|parameter| parameter.kind == l::ParameterKind::Explicit)
            .count();
        let indexed = match operation {
            "Reduce" | "ReduceRight" => arity == 3,
            "Sort" => false,
            _ => arity == 2,
        };
        let initial_len = self.array_subject_len(&receiver, receiver_ty)?;
        let callback_arguments = |value: Value, index: usize| {
            if indexed {
                vec![value, Value::I(index as i64)]
            } else {
                vec![value]
            }
        };
        match operation {
            "ForEach" => {
                for index in 0..initial_len {
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        break;
                    };
                    let _ =
                        self.invoke_callable(&callable, callback_arguments(value, index), None)?;
                }
                Ok(Value::Void)
            }
            "Map" | "FlatMap" => {
                let result_element = match result_ty {
                    Some(l::ValueType::Data(Type::Array(element))) => element.as_ref(),
                    _ => return Err(self.invalid(None, "Array.map result is not a dynamic array")),
                };
                let out = self.new_array(result_element)?;
                for index in 0..initial_len {
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        break;
                    };
                    let mapped =
                        self.invoke_callable(&callable, callback_arguments(value, index), None)?;
                    if operation == "FlatMap" {
                        let array_ty = Type::Array(Box::new(result_element.clone()));
                        let count = self.array_subject_len(&mapped, &array_ty)?;
                        for inner in 0..count {
                            let value = self
                                .array_subject_value(&mapped, &array_ty, result_element, inner)?
                                .ok_or_else(|| {
                                    self.invalid(None, "flatMap result element is missing")
                                })?;
                            self.array_push_value(out, result_element, &value)?;
                        }
                    } else {
                        self.array_push_value(out, result_element, &mapped)?;
                    }
                }
                Ok(Value::Handle(out))
            }
            "Filter" => {
                let out = self.new_array(element_ty)?;
                for index in 0..initial_len {
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        break;
                    };
                    let keep = self
                        .invoke_callable(&callable, callback_arguments(value.clone(), index), None)?
                        .as_bool()?;
                    if keep {
                        self.array_push_value(out, element_ty, &value)?;
                    }
                }
                Ok(Value::Handle(out))
            }
            "Reduce" | "ReduceRight" => {
                let mut accumulator = operands.get(2).cloned().ok_or_else(|| {
                    self.invalid(None, format!("Array.{operation} has no initial value"))
                })?;
                for step in 0..initial_len {
                    let index = if operation == "Reduce" {
                        step
                    } else {
                        initial_len - 1 - step
                    };
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        if operation == "Reduce" {
                            break;
                        }
                        continue;
                    };
                    let mut arguments = vec![accumulator, value];
                    if indexed {
                        arguments.push(Value::I(index as i64));
                    }
                    accumulator = self.invoke_callable(&callable, arguments, None)?;
                }
                Ok(accumulator)
            }
            "Some" | "Every" | "FindIndex" | "Find" | "FindLast" | "FindLastIndex" => {
                let reverse = matches!(operation, "FindLast" | "FindLastIndex");
                for step in 0..initial_len {
                    let index = if reverse {
                        initial_len - 1 - step
                    } else {
                        step
                    };
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        if reverse {
                            continue;
                        }
                        break;
                    };
                    let matched = self
                        .invoke_callable(&callable, callback_arguments(value.clone(), index), None)?
                        .as_bool()?;
                    if operation == "Some" && matched {
                        return Ok(Value::Bool(true));
                    }
                    if operation == "Every" && !matched {
                        return Ok(Value::Bool(false));
                    }
                    if matches!(operation, "Find" | "FindLast") && matched {
                        return Ok(value);
                    }
                    if matches!(operation, "FindIndex" | "FindLastIndex") && matched {
                        return Ok(Value::I(index as i64));
                    }
                }
                Ok(match operation {
                    "Every" => Value::Bool(true),
                    "FindIndex" | "FindLastIndex" => Value::I(-1),
                    "Find" | "FindLast" => Value::Handle(std::ptr::null_mut()),
                    _ => Value::Bool(false),
                })
            }
            "Sort" => {
                if !matches!(receiver_ty, Type::Array(_)) {
                    return Err(self.invalid(None, "FixedArray.sort is not an accepted operation"));
                }
                let mut sorted = Vec::with_capacity(initial_len);
                for index in 0..initial_len {
                    let Some(value) =
                        self.array_subject_value(&receiver, receiver_ty, element_ty, index)?
                    else {
                        break;
                    };
                    sorted.push(value);
                }
                // Stable insertion into scratch storage. No receiver bytes are
                // changed until every comparator invocation has succeeded.
                for index in 1..sorted.len() {
                    let mut cursor = index;
                    while cursor > 0 {
                        let comparison = self
                            .invoke_callable(
                                &callable,
                                vec![sorted[cursor - 1].clone(), sorted[cursor].clone()],
                                None,
                            )?
                            .as_i64()?;
                        if comparison <= 0 {
                            break;
                        }
                        sorted.swap(cursor - 1, cursor);
                        cursor -= 1;
                    }
                }
                let handle = receiver.as_handle()?;
                for (index, value) in sorted.iter().enumerate() {
                    self.array_store_value(handle, element_ty, index, value)?;
                }
                Ok(Value::Handle(handle))
            }
            _ => Err(self.invalid(
                None,
                format!("unknown Array callback intrinsic {operation}"),
            )),
        }
    }

    fn array_subject_len(
        &self,
        receiver: &Value,
        receiver_ty: &Type,
    ) -> Result<usize, InterpretError> {
        match (receiver_ty, receiver) {
            (Type::Array(_), Value::Handle(handle)) => {
                // SAFETY: receiver is a live runtime array.
                let len = unsafe { self.context.array_len(*handle) };
                usize::try_from(len).map_err(|_| self.invalid(None, "array length is negative"))
            }
            (Type::FixedArray(_, count), Value::Blob(_)) => usize::try_from(*count)
                .map_err(|_| self.invalid(None, "fixed-array count does not fit usize")),
            (Type::Array(_), other) => Err(type_error("runtime array handle", other)),
            (Type::FixedArray(_, _), other) => Err(type_error("fixed array", other)),
            _ => Err(self.invalid(None, "array callback receiver has a non-array type")),
        }
    }

    fn array_subject_value(
        &mut self,
        receiver: &Value,
        receiver_ty: &Type,
        element_ty: &Type,
        index: usize,
    ) -> Result<Option<Value>, InterpretError> {
        let layout = self
            .layout_cached(element_ty)
            .ok_or_else(|| self.invalid(None, "array callback element has no layout"))?;
        match (receiver_ty, receiver) {
            (Type::Array(_), Value::Handle(handle)) => {
                // Callbacks can shorten the receiver. Appends never extend the
                // captured `initial_len`, but a removed suffix ends traversal.
                let current_len = unsafe { self.context.array_len(*handle) };
                if i32::try_from(index).map_or(true, |index| index >= current_len) {
                    return Ok(None);
                }
                let pointer = unsafe { self.context.array_elem_ptr(*handle, index as i32, 0) };
                self.check_runtime(&Pos::new("<array-callback>", 1, 1))?;
                let bytes = unsafe { std::slice::from_raw_parts(pointer, layout.size) };
                self.unpack(element_ty, bytes).map(Some)
            }
            (Type::FixedArray(_, count), Value::Blob(bytes)) => {
                let count = usize::try_from(*count)
                    .map_err(|_| self.invalid(None, "fixed-array count does not fit usize"))?;
                if index >= count {
                    return Ok(None);
                }
                let start = index
                    .checked_mul(layout.size)
                    .ok_or_else(|| self.invalid(None, "fixed-array callback offset overflows"))?;
                let end = start
                    .checked_add(layout.size)
                    .ok_or_else(|| self.invalid(None, "fixed-array callback range overflows"))?;
                let element = bytes.get(start..end).ok_or_else(|| {
                    self.invalid(None, "fixed-array callback reads outside its blob")
                })?;
                self.unpack(element_ty, element).map(Some)
            }
            (Type::Array(_), other) => Err(type_error("runtime array handle", other)),
            (Type::FixedArray(_, _), other) => Err(type_error("fixed array", other)),
            _ => Err(self.invalid(None, "array callback receiver has a non-array type")),
        }
    }

    fn new_array(&mut self, element_ty: &Type) -> Result<*mut u8, InterpretError> {
        let layout = self
            .layout_cached(element_ty)
            .ok_or_else(|| self.invalid(None, "array result element has no layout"))?;
        let handle = self.context.array_new(layout.size, 0);
        self.check_runtime(&Pos::new("<array-callback>", 1, 1))?;
        self.root_handle(handle);
        Ok(handle)
    }

    fn array_push_value(
        &mut self,
        array: *mut u8,
        element_ty: &Type,
        value: &Value,
    ) -> Result<(), InterpretError> {
        let packed = self.pack(element_ty, value)?;
        // SAFETY: live array and one exactly packed element.
        unsafe { ffi::subscript_rt_array_push(&mut *self.context, array, packed.as_ptr(), 0) };
        self.check_runtime(&Pos::new("<array-callback>", 1, 1))
    }

    fn array_store_value(
        &mut self,
        array: *mut u8,
        element_ty: &Type,
        index: usize,
        value: &Value,
    ) -> Result<(), InterpretError> {
        let packed = self.pack(element_ty, value)?;
        let index = i32::try_from(index)
            .map_err(|_| self.invalid(None, "array callback index overflows i32"))?;
        // SAFETY: index came from the live receiver's captured length.
        let pointer = unsafe { self.context.array_elem_ptr(array, index, 0) };
        self.check_runtime(&Pos::new("<array-callback>", 1, 1))?;
        unsafe { std::ptr::copy_nonoverlapping(packed.as_ptr(), pointer, packed.len()) };
        Ok(())
    }

    pub(super) fn intrinsic_map(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
        parameter_types: &[l::ValueType],
        _type_argument: Option<&Type>,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        if operation == "GroupBy" {
            let element_ty = match parameter_types.first() {
                Some(l::ValueType::Data(Type::Array(element))) => element.as_ref(),
                _ => return Err(self.invalid(None, "Map.groupBy items type is not an array")),
            };
            let key_ty = match result_ty {
                Some(l::ValueType::Data(Type::Map(key, value))) if matches!(value.as_ref(), Type::Array(element) if element.as_ref() == element_ty) => {
                    key.as_ref()
                }
                _ => return Err(self.invalid(None, "Map.groupBy result type is not Map<K,T[]>")),
            };
            let items = operands
                .first()
                .ok_or_else(|| self.invalid(None, "Map.groupBy has no items"))?
                .as_handle()?;
            let callable = self.callable_operand(operands.get(1), "Map.groupBy")?;
            let key_layout = self
                .layout_cached(key_ty)
                .ok_or_else(|| self.invalid(None, "Map.groupBy key has no layout"))?;
            let mut state = CallbackState {
                interpreter: (self as *mut Self).cast(),
                callable,
                first_ty: element_ty.clone(),
                second_ty: Some(key_ty.clone()),
                error: None,
            };
            // SAFETY: the runtime call is synchronous; the bridge copies the
            // element, invokes the interpreter callback, and writes one key.
            let result = unsafe {
                ffi::subscript_rt_map_group_by(
                    &mut *self.context,
                    items,
                    group_by_callback_bridge as *const u8,
                    (&mut state as *mut CallbackState).cast(),
                    group_by_callback_bridge as *const u8,
                    key_layout.size as u64,
                    assoc_key_kind(key_ty, self.module),
                    0,
                )
            };
            if let Some(error) = state.error {
                return Err(error);
            }
            self.check_runtime(&Pos::new("<map>", 1, 1))?;
            self.root_handle(result);
            return Ok(Value::Handle(result));
        }
        let shape = parameter_types
            .first()
            .or(result_ty)
            .and_then(|ty| match ty {
                l::ValueType::Data(Type::Map(key, value)) => Some((key.as_ref(), value.as_ref())),
                _ => None,
            })
            .ok_or_else(|| self.invalid(None, format!("Map.{operation} has no Map<K,V> type")))?;
        let key_layout = self
            .layout_cached(shape.0)
            .ok_or_else(|| self.invalid(None, "Map key has no layout"))?;
        let value_layout = self
            .layout_cached(shape.1)
            .ok_or_else(|| self.invalid(None, "Map value has no layout"))?;
        let context = &mut *self.context as *mut Context;
        let interpreter = (self as *mut Self).cast();
        let receiver = || -> Result<*mut u8, InterpretError> {
            operands
                .first()
                .ok_or_else(|| self.invalid(None, format!("Map.{operation} has no receiver")))?
                .as_handle()
        };
        let packed_key = || -> Result<Vec<u8>, InterpretError> {
            self.pack(
                shape.0,
                operands
                    .get(1)
                    .ok_or_else(|| self.invalid(None, format!("Map.{operation} has no key")))?,
            )
        };
        let value = match operation {
            "New" => {
                // SAFETY: concrete widths and runtime key-kind table.
                Value::Handle(unsafe {
                    ffi::subscript_rt_map_new(
                        context,
                        key_layout.size as u64,
                        value_layout.size as u64,
                        assoc_key_kind(shape.0, self.module),
                        0,
                    )
                })
            }
            // SAFETY: live Map receiver.
            "Size" => {
                Value::I(unsafe { ffi::subscript_rt_assoc_size(context, receiver()?) } as i64)
            }
            "Get" => {
                let key = packed_key()?;
                let mut out = vec![0; value_layout.size];
                // SAFETY: live receiver and exact key/value storage.
                if unsafe {
                    ffi::subscript_rt_map_get(context, receiver()?, key.as_ptr(), out.as_mut_ptr())
                } == 0
                {
                    Value::Null
                } else {
                    self.unpack(shape.1, &out)?
                }
            }
            "GetOr" => {
                let key = packed_key()?;
                let fallback = self.pack(
                    shape.1,
                    operands
                        .get(2)
                        .ok_or_else(|| self.invalid(None, "Map.getOr has no fallback"))?,
                )?;
                let mut out = vec![0; value_layout.size];
                // SAFETY: live receiver and exact key/value storage.
                unsafe {
                    ffi::subscript_rt_map_get_or(
                        context,
                        receiver()?,
                        key.as_ptr(),
                        fallback.as_ptr(),
                        out.as_mut_ptr(),
                    )
                };
                self.unpack(shape.1, &out)?
            }
            "Set" => {
                let key = packed_key()?;
                let stored = self.pack(
                    shape.1,
                    operands
                        .get(2)
                        .ok_or_else(|| self.invalid(None, "Map.set has no value"))?,
                )?;
                // SAFETY: live receiver and exact key/value storage.
                Value::Handle(unsafe {
                    ffi::subscript_rt_map_set(
                        context,
                        receiver()?,
                        key.as_ptr(),
                        stored.as_ptr(),
                        0,
                    )
                })
            }
            "Has" => {
                let key = packed_key()?;
                // SAFETY: live receiver and exact key storage.
                Value::Bool(
                    unsafe { ffi::subscript_rt_assoc_has(context, receiver()?, key.as_ptr()) } != 0,
                )
            }
            "Delete" => {
                let key = packed_key()?;
                // SAFETY: live receiver and exact key storage.
                Value::Bool(
                    unsafe { ffi::subscript_rt_assoc_delete(context, receiver()?, key.as_ptr()) }
                        != 0,
                )
            }
            "Clear" => {
                // SAFETY: live receiver.
                unsafe { ffi::subscript_rt_assoc_clear(context, receiver()?) };
                Value::Void
            }
            "ForEach" => {
                let callable = self.callable_operand(operands.get(1), "Map.forEach")?;
                let mut state = CallbackState {
                    interpreter,
                    callable,
                    first_ty: shape.1.clone(),
                    second_ty: Some(shape.0.clone()),
                    error: None,
                };
                // SAFETY: the runtime owns insertion-order traversal; the
                // bridge copies the entry before invoking script code.
                unsafe {
                    ffi::subscript_rt_map_for_each(
                        context,
                        receiver()?,
                        map_callback_bridge as *const u8,
                        (&mut state as *mut CallbackState).cast(),
                        map_callback_bridge as *const u8,
                    )
                };
                if let Some(error) = state.error {
                    return Err(error);
                }
                Value::Void
            }
            _ => return Err(self.invalid(None, format!("unknown Map intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<map>", 1, 1))?;
        if let Value::Handle(handle) = value {
            if operation == "New" {
                self.root_handle(handle);
            }
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }

    pub(super) fn intrinsic_set(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
        parameter_types: &[l::ValueType],
        _type_argument: Option<&Type>,
        result_ty: Option<&l::ValueType>,
    ) -> Result<Value, InterpretError> {
        let key_ty = parameter_types
            .first()
            .and_then(|ty| match ty {
                l::ValueType::Data(Type::Set(key)) => Some(key.as_ref()),
                _ => None,
            })
            .or({
                // `new Set<K>()` has no receiver; the explicit monomorphized
                // type argument is carried by the intrinsic.
                _type_argument
            })
            .or_else(|| match result_ty {
                Some(l::ValueType::Data(Type::Set(key))) => Some(key.as_ref()),
                _ => None,
            })
            .ok_or_else(|| self.invalid(None, format!("Set.{operation} has no key type")))?;
        let layout = self
            .layout_cached(key_ty)
            .ok_or_else(|| self.invalid(None, "Set key has no layout"))?;
        let context = &mut *self.context as *mut Context;
        let interpreter = (self as *mut Self).cast();
        let receiver = || -> Result<*mut u8, InterpretError> {
            operands
                .first()
                .ok_or_else(|| self.invalid(None, format!("Set.{operation} has no receiver")))?
                .as_handle()
        };
        let packed_key = || -> Result<Vec<u8>, InterpretError> {
            self.pack(
                key_ty,
                operands
                    .get(1)
                    .ok_or_else(|| self.invalid(None, format!("Set.{operation} has no key")))?,
            )
        };
        let value = match operation {
            "New" => {
                // SAFETY: concrete width and runtime key-kind table.
                Value::Handle(unsafe {
                    ffi::subscript_rt_set_new(
                        context,
                        layout.size as u64,
                        assoc_key_kind(key_ty, self.module),
                        0,
                    )
                })
            }
            // SAFETY: live Set receiver.
            "Size" => {
                Value::I(unsafe { ffi::subscript_rt_assoc_size(context, receiver()?) } as i64)
            }
            "Add" => {
                let key = packed_key()?;
                // SAFETY: live receiver and exact key storage.
                Value::Handle(unsafe {
                    ffi::subscript_rt_set_add(context, receiver()?, key.as_ptr(), 0)
                })
            }
            "Has" => {
                let key = packed_key()?;
                // SAFETY: live receiver and exact key storage.
                Value::Bool(
                    unsafe { ffi::subscript_rt_assoc_has(context, receiver()?, key.as_ptr()) } != 0,
                )
            }
            "Delete" => {
                let key = packed_key()?;
                // SAFETY: live receiver and exact key storage.
                Value::Bool(
                    unsafe { ffi::subscript_rt_assoc_delete(context, receiver()?, key.as_ptr()) }
                        != 0,
                )
            }
            "Clear" => {
                // SAFETY: live receiver.
                unsafe { ffi::subscript_rt_assoc_clear(context, receiver()?) };
                Value::Void
            }
            "ForEach" => {
                let callable = self.callable_operand(operands.get(1), "Set.forEach")?;
                let mut state = CallbackState {
                    interpreter,
                    callable,
                    first_ty: key_ty.clone(),
                    second_ty: None,
                    error: None,
                };
                // SAFETY: runtime owns fixed-bound insertion-order traversal;
                // the bridge copies each key before calling script code.
                unsafe {
                    ffi::subscript_rt_set_for_each(
                        context,
                        receiver()?,
                        set_callback_bridge as *const u8,
                        (&mut state as *mut CallbackState).cast(),
                        set_callback_bridge as *const u8,
                    )
                };
                if let Some(error) = state.error {
                    return Err(error);
                }
                Value::Void
            }
            "Union" => Value::Handle(unsafe {
                ffi::subscript_rt_set_union(
                    context,
                    receiver()?,
                    operands
                        .get(1)
                        .ok_or_else(|| self.invalid(None, "Set.union has no argument"))?
                        .as_handle()?,
                    0,
                )
            }),
            "Intersection" => Value::Handle(unsafe {
                ffi::subscript_rt_set_intersection(
                    context,
                    receiver()?,
                    operands
                        .get(1)
                        .ok_or_else(|| self.invalid(None, "Set.intersection has no argument"))?
                        .as_handle()?,
                    0,
                )
            }),
            "Difference" => Value::Handle(unsafe {
                ffi::subscript_rt_set_difference(
                    context,
                    receiver()?,
                    operands
                        .get(1)
                        .ok_or_else(|| self.invalid(None, "Set.difference has no argument"))?
                        .as_handle()?,
                    0,
                )
            }),
            "SymmetricDifference" => Value::Handle(unsafe {
                ffi::subscript_rt_set_symmetric_difference(
                    context,
                    receiver()?,
                    operands
                        .get(1)
                        .ok_or_else(|| {
                            self.invalid(None, "Set.symmetricDifference has no argument")
                        })?
                        .as_handle()?,
                    0,
                )
            }),
            "IsSubsetOf" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_set_is_subset_of(
                        context,
                        receiver()?,
                        operands
                            .get(1)
                            .ok_or_else(|| self.invalid(None, "Set.isSubsetOf has no argument"))?
                            .as_handle()?,
                    )
                } != 0,
            ),
            "IsSupersetOf" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_set_is_superset_of(
                        context,
                        receiver()?,
                        operands
                            .get(1)
                            .ok_or_else(|| self.invalid(None, "Set.isSupersetOf has no argument"))?
                            .as_handle()?,
                    )
                } != 0,
            ),
            "IsDisjointFrom" => Value::Bool(
                unsafe {
                    ffi::subscript_rt_set_is_disjoint_from(
                        context,
                        receiver()?,
                        operands
                            .get(1)
                            .ok_or_else(|| {
                                self.invalid(None, "Set.isDisjointFrom has no argument")
                            })?
                            .as_handle()?,
                    )
                } != 0,
            ),
            _ => return Err(self.invalid(None, format!("unknown Set intrinsic {operation}"))),
        };
        self.check_runtime(&Pos::new("<set>", 1, 1))?;
        if let Value::Handle(handle) = value {
            if matches!(
                operation,
                "New" | "Union" | "Intersection" | "Difference" | "SymmetricDifference"
            ) {
                self.root_handle(handle);
            }
            Ok(Value::Handle(handle))
        } else {
            Ok(value)
        }
    }
}
