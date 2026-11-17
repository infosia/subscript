use crate::context::Context;

// ----- C-boundary marshaling (compiler.md §12) -----

/// Data pointer of a string handle: the `const char*` half of a
/// `(ptr, len)` string view passed to a foreign call. Length is
/// [`subscript_rt_str_len`].
///
/// # Safety
///
/// Shared contract; `s` is a live string handle (or null).
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_str_data(ctx: *const Context, s: *const u8) -> *const u8 {
    if s.is_null() {
        return std::ptr::null();
    }
    // SAFETY: shared contract; live string handle.
    unsafe { (*ctx).str_data(s) }
}

/// Data pointer of a dynamic array: the `const T*` half of a
/// `(ptr, count)` descriptor passed to a foreign call. Count is
/// [`subscript_rt_array_len`]. Null for an array that has never grown.
///
/// # Safety
///
/// Shared contract; `a` is a live array handle (or null).
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_array_data(ctx: *const Context, a: *const u8) -> *const u8 {
    if a.is_null() {
        return std::ptr::null();
    }
    // SAFETY: shared contract; live array handle.
    unsafe { (*ctx).array_data(a) }
}

/// Keeps a derived argument in the current native-call scope (compiler.md §323).
///
/// # Safety
/// Shared contract. `value` is null or live; a nonzero `array` identifies an array header.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_borrow(
    ctx: *mut Context,
    value: *mut u8,
    array: u32,
) {
    // SAFETY: the caller supplies the argument and its allocation kind.
    unsafe { (*ctx).boundary_borrow(value, array != 0) };
}

/// Starts a foreign call's storage scope (compiler.md §323).
///
/// # Safety
/// Shared contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_scope_mark(ctx: *mut Context) -> u64 {
    // SAFETY: the caller owns the Context.
    unsafe { (*ctx).boundary_scope_mark() as u64 }
}

/// Ends the foreign call's storage scope (compiler.md §323).
///
/// # Safety
/// Shared contract. `mark` came from this Context's current foreign call.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_scope_release(ctx: *mut Context, mark: u64) {
    // SAFETY: the caller owns the Context and supplies its scope mark.
    unsafe { (*ctx).boundary_scope_release(mark as usize) };
}
