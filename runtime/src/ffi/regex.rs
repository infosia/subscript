use crate::context::Context;

// ----- RegExp (stdlib.md §15, Q31) -----

/// Compiles or reuses a Context-cached regular expression.
///
/// # Safety
///
/// Shared contract; `pattern` and `flags` are live string handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_new(
    ctx: *mut Context,
    pattern: *const u8,
    flags: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::new(unsafe { &mut *ctx }, pattern, flags, pos_id)
}

/// `RegExp.test`, with distinguishable budget-exhaustion trapping.
///
/// # Safety
///
/// Shared contract; `regex` and `subject` are live handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_test(
    ctx: *mut Context,
    regex: *const u8,
    subject: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::test(unsafe { &mut *ctx }, regex, subject, pos_id)
}

/// Returns `RegExp.source` without allocating.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_source(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::source(unsafe { &mut *ctx }, regex, pos_id)
}

/// Returns canonical `RegExp.flags` without allocating.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_flags(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::flags(unsafe { &mut *ctx }, regex, pos_id)
}

/// `string.search(RegExp)`, returning a UTF-8 byte offset or -1.
///
/// # Safety
///
/// Shared contract; `subject` and `regex` are live handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_search(
    ctx: *mut Context,
    subject: *const u8,
    regex: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::search(unsafe { &mut *ctx }, subject, regex, pos_id)
}

/// `string.replace(RegExp, replacement)` using the shared substituter.
///
/// # Safety
///
/// Shared contract; every pointer after `ctx` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_replace(
    ctx: *mut Context,
    subject: *const u8,
    regex: *const u8,
    replacement: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::replace(unsafe { &mut *ctx }, subject, regex, replacement, pos_id)
}

/// `string.replaceAll(RegExp, replacement)`, requiring `g`.
///
/// # Safety
///
/// Shared contract; every pointer after `ctx` is a live handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_replace_all(
    ctx: *mut Context,
    subject: *const u8,
    regex: *const u8,
    replacement: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::replace_all(unsafe { &mut *ctx }, subject, regex, replacement, pos_id)
}

/// `string.split(RegExp, limit)` with capture reinjection and a ToUint32 limit.
///
/// # Safety
///
/// Shared contract; `subject` and `regex` are live handles.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_split(
    ctx: *mut Context,
    subject: *const u8,
    regex: *const u8,
    limit: i32,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::split(unsafe { &mut *ctx }, subject, regex, limit, pos_id)
}

/// Returns the last match's capture start byte, or -1.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_match_start(
    ctx: *mut Context,
    regex: *const u8,
    group: i32,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::match_boundary(unsafe { &mut *ctx }, regex, group, false, pos_id)
}

/// Returns the last match's capture end byte, or -1.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_match_end(
    ctx: *mut Context,
    regex: *const u8,
    group: i32,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::match_boundary(unsafe { &mut *ctx }, regex, group, true, pos_id)
}

/// Returns `RegExp.global`.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_global(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::has_flag(unsafe { &mut *ctx }, regex, b'g', pos_id)
}
/// Returns `RegExp.ignoreCase`.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_ignore_case(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::has_flag(unsafe { &mut *ctx }, regex, b'i', pos_id)
}
/// Returns `RegExp.multiline`.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_multiline(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::has_flag(unsafe { &mut *ctx }, regex, b'm', pos_id)
}
/// Returns `RegExp.dotAll`.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_dot_all(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::has_flag(unsafe { &mut *ctx }, regex, b's', pos_id)
}
/// Returns `RegExp.unicode`.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_unicode(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::has_flag(unsafe { &mut *ctx }, regex, b'u', pos_id)
}
/// Returns `RegExp.hasIndices`.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_has_indices(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::has_flag(unsafe { &mut *ctx }, regex, b'd', pos_id)
}
/// Returns `RegExp.sticky`.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_sticky(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> i32 {
    // SAFETY: shared contract.
    crate::regexops::has_flag(unsafe { &mut *ctx }, regex, b'y', pos_id)
}
/// Returns `RegExp.toString`.
///
/// # Safety
///
/// Shared contract; `regex` is a live RegExp handle.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_regex_to_string(
    ctx: *mut Context,
    regex: *const u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    crate::regexops::to_string(unsafe { &mut *ctx }, regex, pos_id)
}
