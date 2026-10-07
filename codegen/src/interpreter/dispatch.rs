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
        #[cfg(not(test))]
        let _ = pos;
        let mut operands = callable.captures.clone();
        operands.extend(arguments);
        let value = self.call_function(callable.function, operands)?;
        if self
            .module
            .functions
            .get(callable.function.0 as usize)
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
}
