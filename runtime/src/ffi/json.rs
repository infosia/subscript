use crate::context::Context;
use crate::trap::TrapKind;

// ----- JSON.stringify (stdlib.md §13, Q28) -----

fn json_builder_result(ctx: &mut Context, ok: bool, operation: &str, pos_id: u32) {
    if !ok {
        ctx.trap(
            TrapKind::Internal,
            format!("unknown JSON builder in {operation}"),
            pos_id,
        );
    }
}

fn json_begin(ctx: &mut Context, tracked: bool, pos_id: u32) -> u64 {
    match ctx.json_builders().begin(tracked) {
        Some(id) => id,
        None => {
            ctx.trap(
                TrapKind::Internal,
                "JSON builder id space exhausted",
                pos_id,
            );
            0
        }
    }
}

/// Starts an untracked JSON output builder.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_begin(ctx: *mut Context, pos_id: u32) -> u64 {
    // SAFETY: shared contract.
    json_begin(unsafe { &mut *ctx }, false, pos_id)
}

/// Starts a JSON output builder with an active-reference set.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_begin_tracked(ctx: *mut Context, pos_id: u32) -> u64 {
    // SAFETY: shared contract.
    json_begin(unsafe { &mut *ctx }, true, pos_id)
}

/// Completes a JSON builder and allocates its immutable language string.
///
/// # Safety
///
/// Shared contract; `builder` was returned by one of the begin entries.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_finish(
    ctx: *mut Context,
    builder: u64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_builders().finish(builder) {
        Some(bytes) => ctx.alloc_str(&bytes, pos_id),
        None => {
            ctx.trap(TrapKind::Internal, "unknown JSON builder in finish", pos_id);
            std::ptr::null_mut()
        }
    }
}

/// Appends punctuation or another already-shaped JSON byte sequence.
///
/// # Safety
///
/// Shared contract; `value` is a live language string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_raw(
    ctx: *mut Context,
    builder: u64,
    value: *const u8,
    pos_id: u32,
) {
    // SAFETY: shared contract and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_view(value) };
    let ok = ctx.json_builders().raw(builder, bytes);
    json_builder_result(ctx, ok, "raw append", pos_id);
}

/// Appends one quoted and escaped UTF-8 language string.
///
/// # Safety
///
/// Shared contract; `value` is a live language string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_str(
    ctx: *mut Context,
    builder: u64,
    value: *const u8,
    pos_id: u32,
) {
    // SAFETY: shared contract and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_view(value) };
    let ok = ctx.json_builders().string(builder, bytes);
    json_builder_result(ctx, ok, "string append", pos_id);
}

json_integer!(subscript_rt_json_i32, i32, i32);
json_integer!(subscript_rt_json_u32, u32, u32);
json_integer!(subscript_rt_json_i64, i64, i64);
json_integer!(subscript_rt_json_u64, u64, u64);

fn json_float<T>(
    ctx: &mut Context,
    builder: u64,
    value: T,
    finite: bool,
    append: impl FnOnce(&mut crate::json::JsonBuilders, u64, T) -> bool,
    operation: &str,
    pos_id: u32,
) {
    if !finite {
        ctx.trap(
            TrapKind::JsonNumber,
            "JSON.stringify cannot serialize a non-finite number",
            pos_id,
        );
        return;
    }
    let ok = append(ctx.json_builders(), builder, value);
    json_builder_result(ctx, ok, operation, pos_id);
}

/// Appends one finite JSON `f32`, trapping on NaN or infinity.
///
/// # Safety
///
/// Shared contract; `builder` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_f32(
    ctx: *mut Context,
    builder: u64,
    value: f32,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    json_float(
        unsafe { &mut *ctx },
        builder,
        value,
        value.is_finite(),
        crate::json::JsonBuilders::f32,
        "f32 append",
        pos_id,
    );
}

/// Appends one finite JSON `f64`, trapping on NaN or infinity.
///
/// # Safety
///
/// Shared contract; `builder` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_f64(
    ctx: *mut Context,
    builder: u64,
    value: f64,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    json_float(
        unsafe { &mut *ctx },
        builder,
        value,
        value.is_finite(),
        crate::json::JsonBuilders::f64,
        "f64 append",
        pos_id,
    );
}

/// Appends a JSON boolean.
///
/// # Safety
///
/// Shared contract; `builder` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_bool(
    ctx: *mut Context,
    builder: u64,
    value: u8,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let bytes: &[u8] = if value == 0 { b"false" } else { b"true" };
    let ok = ctx.json_builders().raw(builder, bytes);
    json_builder_result(ctx, ok, "boolean append", pos_id);
}

/// Appends a Date as its quoted `toISOString()` result.
///
/// # Safety
///
/// Shared contract; `builder` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_date(
    ctx: *mut Context,
    builder: u64,
    value: i64,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(iso) = crate::date::to_iso(value) else {
        ctx.trap(
            TrapKind::DateRange,
            "JSON.stringify Date year is outside 0000..9999",
            pos_id,
        );
        return;
    };
    let ok = ctx.json_builders().string(builder, iso.as_bytes());
    json_builder_result(ctx, ok, "Date append", pos_id);
}

/// Appends JSON `null`.
///
/// # Safety
///
/// Shared contract; `builder` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_null(ctx: *mut Context, builder: u64, pos_id: u32) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let ok = ctx.json_builders().raw(builder, b"null");
    json_builder_result(ctx, ok, "null append", pos_id);
}

/// Adds a reference to the tracked serializer's active path. A revisit
/// records the JSON cycle trap and returns zero.
///
/// # Safety
///
/// Shared contract; `builder` is tracked and `reference` is a live
/// reference-class handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_visit(
    ctx: *mut Context,
    builder: u64,
    reference: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    match ctx.json_builders().visit(builder, reference as usize) {
        crate::json::Visit::Inserted => 1,
        crate::json::Visit::Cycle => {
            ctx.trap(
                TrapKind::JsonCycle,
                "JSON.stringify encountered a cyclic reference",
                pos_id,
            );
            0
        }
        crate::json::Visit::InvalidBuilder => {
            ctx.trap(
                TrapKind::Internal,
                "unknown or untracked JSON builder in visit",
                pos_id,
            );
            0
        }
    }
}

/// Removes a completed reference from the tracked serializer's active
/// path.
///
/// # Safety
///
/// Shared contract; `builder` is tracked and `reference` was visited.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_leave(
    ctx: *mut Context,
    builder: u64,
    reference: *const u8,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let left = ctx.json_builders().leave(builder, reference as usize);
    if !left {
        ctx.trap(TrapKind::Internal, "unknown JSON builder in leave", pos_id);
    }
}

// ----- JSON.parse (stdlib.md §13.4, Q28) -----

fn json_parser_invalid(ctx: &mut Context, operation: &str, pos_id: u32) {
    ctx.trap(
        TrapKind::Internal,
        format!("invalid transient JSON parser access in {operation}"),
        pos_id,
    );
}

fn parsed<T>(ctx: &mut Context, value: Option<T>, default: T, operation: &str, pos_id: u32) -> T {
    match value {
        Some(value) => value,
        None => {
            json_parser_invalid(ctx, operation, pos_id);
            default
        }
    }
}

/// Parses a complete JSON document into transient runtime state.
/// Malformed input returns zero without trapping, and records the
/// failure for `subscript_rt_json_parse_failure`.
///
/// # Safety
///
/// Shared contract; `text` is a live language string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_begin(
    ctx: *mut Context,
    text: *const u8,
    _pos_id: u32,
) -> u64 {
    // SAFETY: shared contract and live string handle.
    let ctx = unsafe { &mut *ctx };
    let bytes = unsafe { ctx.str_view(text) };
    ctx.json_parsers().begin(bytes)
}

/// Returns the `SyntaxError` message of the last parse begin that returned
/// zero, and clears the record (`compiler.md` §115.7 rules 3 and 4). A
/// call with no recorded failure is an internal fault.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_failure(
    ctx: *mut Context,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let Some(failure) = ctx.json_parsers().take_failure() else {
        json_parser_invalid(ctx, "failure", pos_id);
        return std::ptr::null_mut();
    };
    ctx.alloc_str(failure.message().as_bytes(), pos_id)
}

/// Removes one transient parsed document.
///
/// # Safety
///
/// Shared contract; `parser` is a nonzero handle returned by parse begin.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_end(ctx: *mut Context, parser: u64, pos_id: u32) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    if !ctx.json_parsers().finish(parser) {
        json_parser_invalid(ctx, "end", pos_id);
    }
}

/// Returns the root node handle of a transient parsed document.
///
/// # Safety
///
/// Shared contract; `parser` is live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_root(
    ctx: *mut Context,
    parser: u64,
    pos_id: u32,
) -> u64 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let value = ctx.json_parsers().root(parser);
    parsed(ctx, value, 0, "root", pos_id)
}

/// Tests a parsed node's JSON kind.
///
/// # Safety
///
/// Shared contract; `parser` and `node` are live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_is_kind(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    kind: u32,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let value = ctx
        .json_parsers()
        .is_kind(parser, node, kind)
        .map(i32::from);
    parsed(ctx, value, 0, "kind test", pos_id)
}

/// Tests whether a parsed number can populate one exact sized numeric
/// target without producing an out-of-range integer or non-finite float.
///
/// # Safety
///
/// Shared contract; `parser` and `node` are live.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_number_fits(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    target: u32,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let value = ctx
        .json_parsers()
        .number_fits(parser, node, target)
        .map(i32::from);
    parsed(ctx, value, 0, "number validation", pos_id)
}

/// Reads a previously validated parsed number as its ECMA `f64` value.
///
/// # Safety
///
/// Shared contract; the node was validated as an f32/f64 number.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_number(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    pos_id: u32,
) -> f64 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let value = ctx.json_parsers().number(parser, node);
    parsed(ctx, value, 0.0, "number read", pos_id)
}

/// Reads a previously validated parsed number as one exact sized
/// integer. The returned `u64` carries the target value's bits; no
/// floating-point conversion occurs.
///
/// # Safety
///
/// Shared contract; the node was validated for `target`.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_integer(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    target: u32,
    pos_id: u32,
) -> u64 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let value = ctx.json_parsers().integer(parser, node, target);
    parsed(ctx, value, 0, "integer read", pos_id)
}

/// Reads a previously validated parsed boolean.
///
/// # Safety
///
/// Shared contract; the node was validated as a boolean.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_bool(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let value = ctx.json_parsers().boolean(parser, node).map(i32::from);
    parsed(ctx, value, 0, "boolean read", pos_id)
}

/// Allocates a language string from a previously validated parsed string.
///
/// # Safety
///
/// Shared contract; the node was validated as a string.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_string(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let value = ctx
        .json_parsers()
        .string(parser, node)
        .map(str::as_bytes)
        .map(<[u8]>::to_vec);
    let bytes = parsed(ctx, value, Vec::new(), "string read", pos_id);
    if ctx.trapped() {
        return std::ptr::null_mut();
    }
    ctx.alloc_str(&bytes, pos_id)
}

/// Returns a previously validated parsed array's length.
///
/// # Safety
///
/// Shared contract; the node was validated as an array.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_array_len(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // Dynamic arrays use i32 indexing in the language. An unrepresentable
    // JSON length cannot match any T[].
    let value = ctx
        .json_parsers()
        .array_len(parser, node)
        .map(|len| i32::try_from(len).unwrap_or(-1));
    parsed(ctx, value, -1, "array length", pos_id)
}

/// Returns one node handle from a previously validated parsed array.
///
/// # Safety
///
/// Shared contract; `index` is in bounds.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_array_get(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    index: i32,
    pos_id: u32,
) -> u64 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    let value = usize::try_from(index)
        .ok()
        .and_then(|index| ctx.json_parsers().array_get(parser, node, index));
    parsed(ctx, value, 0, "array element", pos_id)
}

/// Returns the last occurrence of an object field, or zero when absent.
///
/// # Safety
///
/// Shared contract; `key` is a live language string handle and the node
/// was validated as an object.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_json_parse_object_get(
    ctx: *mut Context,
    parser: u64,
    node: u64,
    key: *const u8,
    pos_id: u32,
) -> u64 {
    // SAFETY: shared contract and live string handle.
    let ctx = unsafe { &mut *ctx };
    let key = unsafe { ctx.str_view(key) };
    let Some(key) = std::str::from_utf8(key).ok() else {
        json_parser_invalid(ctx, "object key", pos_id);
        return 0;
    };
    let value = ctx.json_parsers().object_get(parser, node, key);
    parsed(ctx, value, 0, "object field", pos_id)
}
