//! Intrinsic dispatch and tier-specific release consumers.

use super::*;

impl Interpreter<'_> {
    pub(super) fn invoke_intrinsic(
        &mut self,
        intrinsic: &l::Intrinsic,
        operation: &str,
        operands: Vec<Value>,
        parameter_types: &[l::ValueType],
        result_ty: Option<&l::ValueType>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        match intrinsic.family {
            l::IntrinsicFamily::Ambient => {
                if operation == "Collect" {
                    let pos =
                        pos.ok_or_else(|| self.invalid(None, "collect call has no position"))?;
                    self.collect_interpreter(pos)?;
                    return Ok(Value::Void);
                }
                if operation == "UnsafeDelete" {
                    let pos = pos.ok_or_else(|| self.invalid(None, "free call has no position"))?;
                    if matches!(parameter_types.first(), Some(l::ValueType::Data(Type::Map(_, value))) if value.counted_type().is_some())
                    {
                        self.free_counted_map(&operands, parameter_types, pos)?;
                    } else {
                        self.free_counted_object(&operands, pos)?;
                    }
                    return Ok(Value::Void);
                }
                self.intrinsic_ambient(operation, operands)
            }
            l::IntrinsicFamily::Math => self.intrinsic_math(operation, operands),
            l::IntrinsicFamily::Number => self.intrinsic_number(operation, operands),
            l::IntrinsicFamily::Date => self.intrinsic_date(operation, operands),
            l::IntrinsicFamily::String => self.intrinsic_string(operation, operands),
            l::IntrinsicFamily::Regex => self.intrinsic_regex(operation, operands),
            l::IntrinsicFamily::Text => self.intrinsic_text(operation, operands),
            l::IntrinsicFamily::Json => self.intrinsic_json(operation, operands, result_ty),
            l::IntrinsicFamily::Array => self.intrinsic_array(
                operation,
                operands,
                parameter_types,
                result_ty,
                pos.ok_or_else(|| self.invalid(None, "array call has no position"))?,
            ),
            l::IntrinsicFamily::Map => self.intrinsic_map(
                operation,
                operands,
                parameter_types,
                intrinsic.type_argument.as_ref(),
                result_ty,
                pos.ok_or_else(|| self.invalid(None, "map call has no position"))?,
            ),
            l::IntrinsicFamily::Set => self.intrinsic_set(
                operation,
                operands,
                parameter_types,
                intrinsic.type_argument.as_ref(),
                result_ty,
            ),
            l::IntrinsicFamily::ContextBytes => {
                self.intrinsic_context_bytes(intrinsic, operation, operands, pos)
            }
            l::IntrinsicFamily::Worker => Err(InterpretError::Unsupported {
                reason: format!("Worker.{operation} requires a runtime worker adapter"),
            }),
        }
    }
}

impl Interpreter<'_> {
    pub(super) fn invoke_closure_default(
        &mut self,
        helper: l::FunctionId,
        lambda: l::FunctionId,
        mut operands: Vec<Value>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        let callable = operands
            .first()
            .cloned()
            .ok_or_else(|| self.invalid(pos.cloned(), "default call has no callable"))?;
        let Value::Callable(callable) = callable else {
            return Err(type_error("callable", &callable));
        };
        if callable.function != lambda {
            return Err(self.invalid(
                pos.cloned(),
                "default callable and environment target disagree",
            ));
        }
        operands.remove(0);
        let mut arguments = callable.captures.clone();
        arguments.extend(operands);
        self.active_roots
            .push(roots::Snapshot::call(&callable, &arguments));
        let result = self.invoke_lambda_function(helper, arguments, pos);
        self.active_roots.pop();
        result
    }

    pub(super) fn invoke_callable(
        &mut self,
        callable: &Rc<Callable>,
        arguments: Vec<Value>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        self.active_roots
            .push(roots::Snapshot::call(callable, &arguments));
        let result = self.invoke_callable_effect(callable, arguments, pos);
        self.active_roots.pop();
        result
    }
    fn invoke_callable_effect(
        &mut self,
        callable: &Rc<Callable>,
        arguments: Vec<Value>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        let mut operands = callable.captures.clone();
        operands.extend(arguments);
        self.invoke_lambda_function(callable.function, operands, pos)
    }

    pub(super) fn invoke_lambda_function(
        &mut self,
        function: l::FunctionId,
        operands: Vec<Value>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        #[cfg(not(test))]
        let _ = pos;
        let value = self.call_function(function, operands)?;
        if self
            .module
            .functions
            .get(function.0 as usize)
            .is_some_and(|function| function.is_async)
        {
            let Value::Coroutine(handle) = &value else {
                return Err(self.invalid(None, "async callable returns no handle"));
            };
            #[cfg(test)]
            {
                handle.borrow_mut().create_pos = pos.cloned().unwrap_or_else(no_script_site);
            }
            self.async_start(&Rc::clone(handle))?;
        }
        Ok(value)
    }

    pub(super) fn invoke_builtin(
        &mut self,
        method: l::BuiltinMethod,
        operands: Vec<Value>,
        parameter_types: &[l::ValueType],
        result_ty: Option<&l::ValueType>,
        pos: Option<&Pos>,
    ) -> Result<Value, InterpretError> {
        match method {
            l::BuiltinMethod::ArrayPush | l::BuiltinMethod::ArrayPushReserved => {
                let array = operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Array.push has no receiver"))?
                    .as_handle()?;
                let value = operands
                    .get(1)
                    .ok_or_else(|| self.invalid(None, "Array.push has no value"))?;
                let element_ty = match parameter_types.get(1) {
                    Some(l::ValueType::Data(ty)) => ty,
                    _ => {
                        return Err(
                            self.invalid(None, "Array.push has no declared element parameter type")
                        );
                    }
                };
                let bytes = self.pack(element_ty, value)?;
                // SAFETY: live array and readable element scratch.
                let length = unsafe {
                    ffi::subscript_rt_array_push(&mut *self.context, array, bytes.as_ptr(), 0)
                };
                self.check_runtime(&Pos::new("<builtin>", 1, 1))?;
                Ok(Value::I(length as i64))
            }
            l::BuiltinMethod::ArrayClear => self.clear_array(&operands, pos),
            l::BuiltinMethod::ArrayPop => {
                let array = operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "Array.pop has no receiver"))?
                    .as_handle()?;
                let ty = match result_ty {
                    Some(l::ValueType::Data(ty)) => ty,
                    _ => return Err(self.invalid(None, "Array.pop has no result type")),
                };
                let layout = self
                    .layout_cached(ty)
                    .ok_or_else(|| self.invalid(None, "Array.pop type has no layout"))?;
                let mut bytes = vec![0; layout.size];
                // compiler.md §196.1: the zeroed bytes are the absent value.
                let pop = match parameter_types.first() {
                    Some(l::ValueType::Data(Type::Array(element)))
                        if crate::lir_types::removal_gives_absent(element, result_ty) =>
                    {
                        ffi::subscript_rt_array_pop_or_absent
                    }
                    _ => ffi::subscript_rt_array_pop,
                };
                // SAFETY: live array and writable element storage.
                unsafe { pop(&mut *self.context, array, bytes.as_mut_ptr(), 0) };
                self.check_runtime(&Pos::new("<builtin>", 1, 1))?;
                self.unpack(ty, &bytes)
            }
            l::BuiltinMethod::StringSlice => {
                let string = operands
                    .first()
                    .ok_or_else(|| self.invalid(None, "string.slice has no receiver"))?
                    .as_handle()?;
                let start = operands
                    .get(1)
                    .ok_or_else(|| self.invalid(None, "string.slice has no start"))?
                    .as_i64()? as i32;
                let end = operands
                    .get(2)
                    .ok_or_else(|| self.invalid(None, "string.slice has no end"))?
                    .as_i64()? as i32;
                // SAFETY: live string handle; runtime owns byte-boundary rules.
                let value = unsafe {
                    ffi::subscript_rt_str_slice(&mut *self.context, string, start, end, 0)
                };
                self.check_runtime(&Pos::new("<builtin>", 1, 1))?;
                Ok(Value::Handle(value))
            }
            l::BuiltinMethod::GeneratorNext => {
                let Value::Coroutine(coroutine) = operands
                    .first()
                    .cloned()
                    .ok_or_else(|| self.invalid(None, "generator.next has no receiver"))?
                else {
                    return Err(self.invalid(None, "generator.next receiver is not a coroutine"));
                };
                let value_ty = match result_ty {
                    Some(l::ValueType::Data(Type::IterResult(value))) => value,
                    _ => return Err(self.invalid(None, "generator.next result is not IterResult")),
                };
                self.resume_generator(&coroutine, value_ty)
            }
        }
    }
}
