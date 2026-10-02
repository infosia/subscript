use crate::context::Context;
use crate::trap::TrapKind;

/// `print(message)`: delivers the string's bytes to the installed print
/// observer, or appends them and a newline to the Context stdout sink when
/// no observer is installed.
///
/// # Safety
///
/// Shared contract; `s` is a live string handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_print(ctx: *mut Context, s: *const u8) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    if s.is_null() {
        return;
    }
    // SAFETY: `s` is a live string handle of this context.
    unsafe { ctx.print_str(s) };
}

/// `Context.collect()`: explicitly invoked collection (Q7).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_collect(ctx: *mut Context) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.collect();
}

/// Allocates `size` payload bytes tagged `class_id`; null on trap.
///
/// Fresh storage and classes that can hold handles are zeroed. A
/// handle-free class must replace all exposed bytes before a read.
/// String operations use [`Context::alloc_str_with`] for that write.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_alloc(
    ctx: *mut Context,
    size: u64,
    class_id: u32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.alloc(size as usize, class_id, pos_id)
}

/// Allocates, zeroes, and installs the ship image's module-global block.
///
/// This is an internal generated-code ABI, not a language allocation. The
/// block is reached through the Context globals slot and freed with the
/// Context.
///
/// # Safety
///
/// Shared contract; `align` is the emitted C block type's alignment.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_globals_init(
    ctx: *mut Context,
    size: u64,
    align: u64,
) -> *mut u8 {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    globals_init_with_conversion(ctx, size, align, |value| usize::try_from(value).ok())
}

fn globals_init_with_conversion(
    ctx: &mut Context,
    size: u64,
    align: u64,
    mut convert: impl FnMut(u64) -> Option<usize>,
) -> *mut u8 {
    let Some(size) = convert(size) else {
        ctx.trap(
            TrapKind::Internal,
            "module-global block layout is not representable",
            0,
        );
        return std::ptr::null_mut();
    };
    let Some(align) = convert(align) else {
        ctx.trap(
            TrapKind::Internal,
            "module-global block layout is not representable",
            0,
        );
        return std::ptr::null_mut();
    };
    ctx.init_module_globals(size, align)
}

/// Begins a nested call-duration scratch scope for recursive boundary
/// element and struct-pointer lowering (§32/§33).
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_scratch_mark(ctx: *mut Context) -> u64 {
    unsafe { &*ctx }.boundary_scratch_mark() as u64
}

/// Allocates one zeroed scratch block in the current boundary scope.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_scratch_alloc(
    ctx: *mut Context,
    size: u64,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    let runtime = unsafe { &mut *ctx };
    runtime.boundary_scratch_alloc(size as usize, pos_id)
}

/// Releases every boundary scratch block allocated since `mark`.
///
/// # Safety
///
/// Shared contract; `mark` came from this Context.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_scratch_release(ctx: *mut Context, mark: u64) {
    unsafe { &mut *ctx }.boundary_scratch_release(mark as usize);
}

/// `Context.free(value)`: frees immediately by default. With freed-handle
/// diagnostics enabled, a threshold-eligible allocation may be retained
/// within the byte budget; double-delete diagnostics then follow §8.1a-3's
/// guaranteed-versus-best-effort coverage.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_delete(ctx: *mut Context, payload: *mut u8, pos_id: u32) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.delete(payload as usize, pos_id);
}

/// Records a trap raised by an emitted check in generated code.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_trap(ctx: *mut Context, kind: u32, pos_id: u32) {
    // SAFETY: shared contract.
    let ctx = unsafe { &mut *ctx };
    // An unknown kind means the code generator and runtime disagree;
    // report it as an internal fault instead of misattributing it.
    let kind = TrapKind::from_u32(kind).unwrap_or(TrapKind::Internal);
    ctx.trap(kind, kind.message(None), pos_id);
}

/// Records an emitted array-bounds trap with its materialized index and
/// length, preserving the runtime's canonical diagnostic across tiers.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_trap_index_out_of_bounds(
    ctx: *mut Context,
    index: i32,
    length: u32,
    pos_id: u32,
) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.trap(
        TrapKind::IndexOutOfBounds,
        TrapKind::IndexOutOfBounds.message(Some((index, u64::from(length)))),
        pos_id,
    );
}

/// Records a boundary trap for an integer outside a `CEnum` wire
/// mapping (compiler.md §50).
///
/// # Safety
///
/// Shared contract; `alias` addresses `alias_len` readable UTF-8 bytes for
/// the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_trap_wire_enum(
    ctx: *mut Context,
    alias: *const u8,
    alias_len: u64,
    wire_value: i32,
    pos_id: u32,
) {
    // SAFETY: shared contract supplies a readable compiler-owned byte span.
    let bytes = unsafe { std::slice::from_raw_parts(alias, alias_len as usize) };
    let alias = std::str::from_utf8(bytes).unwrap_or("<invalid alias name>");
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.trap(
        TrapKind::WireEnumUnknownValue,
        format!("unknown wire value {wire_value} for CEnum alias `{alias}`"),
        pos_id,
    );
}

/// Registers a permanent root range: `words` consecutive 8-byte slots
/// at `base` (module globals of managed type, or global aggregates
/// with managed interior).
///
/// # Safety
///
/// Shared contract; the range outlives the script run.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_root_add(ctx: *mut Context, base: *mut u8, words: u64) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.root_add(base as usize, words as usize);
}

/// Pushes a shadow frame of `slots` managed-local slots at `base`.
///
/// # Safety
///
/// Shared contract; the range stays valid until the matching pop.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_shadow_push(ctx: *mut Context, base: *mut u8, slots: u64) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.shadow_push(base as usize, slots as usize);
}

/// Pops the most recent shadow frame.
///
/// # Safety
///
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_shadow_pop(ctx: *mut Context) {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.shadow_pop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globals_init_conversion_failures_trap_before_returning_null() {
        for converted in [[None, Some(8usize)], [Some(8usize), None]] {
            let mut ctx = Context::new();
            let mut converted = converted.into_iter();
            let globals = globals_init_with_conversion(&mut ctx, 8, 8, |_| {
                converted
                    .next()
                    .expect("one conversion result per argument")
            });
            assert!(globals.is_null());
            let trap = ctx.trap_record().expect("conversion failure traps");
            assert_eq!(trap.kind, TrapKind::Internal);
            assert_eq!(
                trap.message,
                "module-global block layout is not representable"
            );
            assert_eq!(trap.pos_id, 0);
        }
    }
}
