//! Error formatting and URI runtime calls.
use super::*;

impl Interpreter<'_> {
    pub(super) fn intrinsic_text(
        &mut self,
        operation: &str,
        operands: Vec<Value>,
    ) -> Result<Value, InterpretError> {
        let first = operands
            .first()
            .ok_or_else(|| self.invalid(None, "Text operation has no operand"))?
            .as_handle()?;
        let context = &mut *self.context as *mut Context;
        // SAFETY: interpreter operands contain live string handles in this Context.
        let result = unsafe {
            match operation {
                "ErrorToString" => ffi::subscript_rt_error_to_string(
                    context,
                    first,
                    operands
                        .get(1)
                        .ok_or_else(|| self.invalid(None, "Error.toString has no message"))?
                        .as_handle()?,
                    0,
                ),
                "EncodeUri" => ffi::subscript_rt_encode_uri(context, first, 0),
                "EncodeComponent" => ffi::subscript_rt_encode_uri_component(context, first, 0),
                "DecodeUri" => ffi::subscript_rt_decode_uri(context, first, 0),
                "DecodeComponent" => ffi::subscript_rt_decode_uri_component(context, first, 0),
                "UriFailure" => ffi::subscript_rt_decode_uri_failure(context, first, 0),
                "ComponentFailure" => {
                    ffi::subscript_rt_decode_uri_component_failure(context, first, 0)
                }
                _ => return Err(self.invalid(None, format!("unknown Text operation {operation}"))),
            }
        };
        Ok(Value::Handle(result))
    }
}
