use crate::context::Context;
use crate::trap::TrapKind;

// ----- Error formatting and URI text (stdlib.md §19) -----

/// Formats an Error name and message with the uncaught-report rule.
///
/// # Safety
/// Shared contract; both operands are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_error_to_string(
    ctx: *mut Context,
    name: *const u8,
    message: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: exclusive Context and live string handles.
    let ctx = unsafe { &mut *ctx };
    let text =
        unsafe { crate::exception::exception_message(ctx.str_bytes(name), ctx.str_bytes(message)) };
    ctx.alloc_str(text.as_bytes(), pos_id)
}

/// Percent-encodes a UTF-8 string.
///
/// # Safety
/// Shared contract; `text` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_encode_uri(
    ctx: *mut Context,
    text: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: exclusive Context and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_bytes(text) };
    let output = crate::uri::encode(bytes, false);
    ctx.alloc_str(&output, pos_id)
}

/// Percent-encodes a UTF-8 string.
///
/// # Safety
/// Shared contract; `text` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_encode_uri_component(
    ctx: *mut Context,
    text: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: exclusive Context and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_bytes(text) };
    let output = crate::uri::encode(bytes, true);
    ctx.alloc_str(&output, pos_id)
}

/// Decodes a validated UTF-8 URI string.
///
/// # Safety
/// Shared contract; `text` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_decode_uri(
    ctx: *mut Context,
    text: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: exclusive Context and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_bytes(text) };
    let output = match crate::uri::decode(bytes, false) {
        Ok(value) => value,
        Err(_) => {
            ctx.trap(
                TrapKind::Internal,
                "URI decode requires validated input",
                pos_id,
            );
            return std::ptr::null_mut();
        }
    };
    ctx.alloc_str(&output, pos_id)
}

/// Decodes a validated UTF-8 URI string.
///
/// # Safety
/// Shared contract; `text` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_decode_uri_component(
    ctx: *mut Context,
    text: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: exclusive Context and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_bytes(text) };
    let output = match crate::uri::decode(bytes, true) {
        Ok(value) => value,
        Err(_) => {
            ctx.trap(
                TrapKind::Internal,
                "URI decode requires validated input",
                pos_id,
            );
            return std::ptr::null_mut();
        }
    };
    ctx.alloc_str(&output, pos_id)
}

/// Returns a malformed-escape message, or an empty string for valid input.
///
/// # Safety
/// Shared contract; `text` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_decode_uri_failure(
    ctx: *mut Context,
    text: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: exclusive Context and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_bytes(text) };
    let output = crate::uri::failure(bytes, false);
    ctx.alloc_str(output.as_bytes(), pos_id)
}

/// Returns a malformed-escape message, or an empty string for valid input.
///
/// # Safety
/// Shared contract; `text` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_decode_uri_component_failure(
    ctx: *mut Context,
    text: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: exclusive Context and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_bytes(text) };
    let output = crate::uri::failure(bytes, true);
    ctx.alloc_str(output.as_bytes(), pos_id)
}
