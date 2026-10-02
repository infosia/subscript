use super::strings::alloc_str_from_view;
use super::SubStrView;
use crate::context::CallbackBinding;
use crate::context::Context;

/// Registers a C-callback binding and returns the stable pointer a
/// boundary marshaler stores in a C `void* userdata` slot. The binding
/// bundles the Context, the language function value's `(code, env)`, and
/// both real userdata slots (§14.4);
/// [`subscript_rt_cb_trampoline`] reads it back. The binding lives for the whole
/// Context (Q13 lifetime rule). Re-registering the same
/// `(code, userdata1, userdata2)` identity returns the same stable pointer
/// and allocates no new binding (§14.4a).
///
/// # Safety
///
/// Shared contract; `code`/`env` are a language function value (a
/// non-capturing wrapper, so `env` is null); `userdata1`/`userdata2`
/// outlive the run.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_cb_bind(
    ctx: *mut Context,
    code: *const u8,
    env: *const u8,
    userdata1: *mut u8,
    userdata2: *mut u8,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.bind_callback(code, env, userdata1, userdata2)
}

/// The generic C-ABI callback trampoline (§14.4). A C API invokes
/// it with the two-userdata callback ABI `(message, userdata1, userdata2)`,
/// where `userdata1` is the binding pointer a marshaler installed via
/// [`subscript_rt_cb_bind`]. It reconstructs the language `string` from the
/// `(ptr, len)` view, then calls the language function value under its own
/// convention `(ctx, env, message, userdata1, userdata2)`.
///
/// The binding is the authoritative source of both language userdata: the
/// marshaler installs the binding in the callback-info's first userdata
/// slot and null in the second, so the trampoline reads both userdata from
/// the binding and ignores its `userdata2` argument. The second C slot
/// exists for the production callback-info shape (offsetof-proven layout)
/// and is wired through the C fire path, but the language values travel in
/// the binding, not the raw C slot.
///
/// The Context reaches the trampoline through the binding (captured at
/// registration), not through global state: scripts are single-threaded
/// and trusted (invariant 6), the trampoline only ever runs synchronously
/// inside a foreign call made by generated code executing under that same
/// Context, so `binding.ctx` is always the live, correct Context. A trap
/// in the language callback sets the Context trap flag and returns a
/// zeroed value; the generated code that made the foreign call checks the
/// flag on return and unwinds, so the trap propagates without crossing
/// this boundary as an unwind.
///
/// # Safety
///
/// `userdata1` is a binding produced by [`subscript_rt_cb_bind`] on the running
/// Context; `message` points at `len` readable bytes (or is null/empty).
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_cb_trampoline(
    message: SubStrView,
    userdata1: *mut u8,
    userdata2: *mut u8,
) {
    // The binding travels in the first slot; the second slot is unused (the
    // binding carries both language userdata).
    let _ = userdata2;
    if userdata1.is_null() {
        return;
    }
    // SAFETY: `userdata1` is a live binding of the running Context.
    let rec = unsafe { &*(userdata1 as *const CallbackBinding) };
    // Copy the record fields before exclusively borrowing its owning
    // Context. Binding storage is stable for the Context's lifetime.
    let ctx_ptr = rec.ctx;
    let code = rec.code;
    let env = rec.env;
    let callback_userdata1 = rec.userdata1;
    let callback_userdata2 = rec.userdata2;
    // SAFETY: `ctx_ptr` is the live Context captured at bind time, and
    // the record carries a language callback of the shared shape.
    unsafe {
        fire_callback(
            ctx_ptr,
            code,
            env,
            callback_userdata1,
            callback_userdata2,
            message,
        );
    }
}

/// Runs one fired callback under the trampoline convention (§14.4).
///
/// Both trampolines share this body: the Context-lifetime one of
/// §14.4a and the explicit-lifetime one of §111 rule 4. It returns
/// without entering script code when the Context already trapped, or
/// when a userdata slot fails the fire-time liveness check
/// (§14.4b (A)). Its caller counts the call, so this function starts
/// and ends no registration.
///
/// # Safety
///
/// `ctx` is the live Context the record captured; `code` is a language
/// callback wrapper of the `(ctx, env, message, userdata1, userdata2)`
/// shape; `message` points at `len` readable bytes (or is null/empty).
pub(crate) unsafe fn fire_callback(
    ctx: *mut Context,
    code: *const u8,
    env: *const u8,
    userdata1: *mut u8,
    userdata2: *mut u8,
    message: SubStrView,
) {
    // SAFETY: the caller supplies the live Context of the record.
    let context = unsafe { &mut *ctx };
    // A trap already stopped the script (e.g. an earlier callback in the
    // same foreign call trapped): do not run script code — a trap stops
    // the run, even when a C API fires the callback more than once.
    if context.trapped() {
        return;
    }
    if !context.validate_callback_userdata(userdata1)
        || !context.validate_callback_userdata(userdata2)
    {
        return;
    }
    // SAFETY: the callback ABI guarantees this readable view. Reuse the
    // boundary copy-in implementation so callback parameters and struct
    // fields have exactly the same null/empty semantics.
    let s = unsafe { alloc_str_from_view(context, message.data, message.len as u64, 0) };
    // The language function value's wrapper takes `(ctx, env, args...)`
    // with the host C calling convention; here the args are the `string`
    // handle and the two userdata slots (§14.4).
    type LangCb = unsafe extern "C" fn(*mut Context, *const u8, *mut u8, *mut u8, *mut u8);
    // SAFETY: `code` is a language callback wrapper of this shape.
    let f: LangCb = unsafe { std::mem::transmute::<*const u8, LangCb>(code) };
    // SAFETY: calling generated code that never unwinds across FFI.
    unsafe { f(ctx, env, s, userdata1, userdata2) };
    // compiler.md §115.4 item 5: no exception crosses the C frame of the
    // host function. A pending exception becomes the uncaught trap here.
    // SAFETY: the same live Context; the callback returned.
    unsafe { &mut *ctx }.settle_uncaught_exception();
}
