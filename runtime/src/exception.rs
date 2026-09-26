//! The pending exception (`specs/blocks/compiler.md` §115).
//!
//! The word at `Context` offset 0 has three states: [`STATE_NONE`],
//! [`STATE_TRAP`], and [`STATE_EXCEPTION`]. Every generated check that
//! stops on a nonzero word stops on both nonzero states. A check at a
//! raise site with a handler then reads the word one more time and takes
//! the handler edge when the word is [`STATE_EXCEPTION`].
//!
//! A `throw` records the Error object and the position of the `throw` as
//! the pending exception. A catch entry takes the object and clears the
//! word. An exception that reaches the outermost script frame of a host
//! entry becomes the trap [`TrapKind::UncaughtException`]. A trap
//! overwrites a pending exception and drops the object.
//!
//! The exception edge of a `using` binding parks the pending exception,
//! runs the dispose hook with a clear word, and resumes the same
//! exception (§115.5 rule 7). A parked exception keeps its object, its
//! report text, and its position, and it is a collection root (rule 8).

use crate::context::Context;
use crate::trap::TrapKind;

/// The word state for no trap and no pending exception.
pub const STATE_NONE: u32 = 0;
/// The word state for a trap.
pub const STATE_TRAP: u32 = 1;
/// The word state for a pending exception.
pub const STATE_EXCEPTION: u32 = 2;

/// One pending exception: the Error object, the report text, and the
/// position of the last `throw`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingException {
    /// Payload address of the Error object. It is a collection root while
    /// the exception is pending (`compiler.md` §115.2 rule 4).
    pub(crate) object: usize,
    /// The report text that an uncaught exception records.
    pub(crate) message: String,
    /// Position-table index of the last `throw`.
    pub(crate) pos_id: u32,
}

/// Returns the report text of an Error with this `name` and `message`.
///
/// The text follows `Error.prototype.toString`: the name alone when the
/// message is empty, the message alone when the name is empty, and
/// `name: message` otherwise.
#[must_use]
pub fn exception_message(name: &[u8], message: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    let message = String::from_utf8_lossy(message);
    if message.is_empty() {
        name.into_owned()
    } else if name.is_empty() {
        message.into_owned()
    } else {
        format!("{name}: {message}")
    }
}

impl Context {
    /// Records `object` as the pending exception, raised at `pos_id`.
    ///
    /// A trap wins over an exception: when the Context is trapped, the
    /// call records nothing. A raise over a pending exception is a
    /// code-generation defect. It records an internal trap.
    pub fn raise_exception(&mut self, object: *mut u8, message: String, pos_id: u32) {
        if self.trap_flag == STATE_TRAP {
            return;
        }
        if self.trap_flag == STATE_EXCEPTION {
            self.trap(
                TrapKind::Internal,
                "an exception was raised while another was pending",
                0,
            );
            return;
        }
        self.pending_exception = Some(PendingException {
            object: object as usize,
            message,
            pos_id,
        });
        self.trap_flag = STATE_EXCEPTION;
    }

    /// True when an exception is pending.
    #[must_use]
    pub fn exception_pending(&self) -> bool {
        self.trap_flag == STATE_EXCEPTION
    }

    /// Takes the pending exception at a catch entry and clears the word.
    ///
    /// A catch entry with no pending exception is a code-generation
    /// defect. It records an internal trap and returns null.
    pub fn catch_exception(&mut self) -> *mut u8 {
        if self.trap_flag != STATE_EXCEPTION {
            self.trap(
                TrapKind::Internal,
                "a catch entry ran with no pending exception",
                0,
            );
            return std::ptr::null_mut();
        }
        self.trap_flag = STATE_NONE;
        self.pending_exception
            .take()
            .map_or(std::ptr::null_mut(), |pending| pending.object as *mut u8)
    }

    /// Converts a pending exception into the trap
    /// [`TrapKind::UncaughtException`] (`compiler.md` §115.4 item 1).
    ///
    /// The trap records the Error's report text and the position of the
    /// last `throw`. With no pending exception, the call does nothing.
    pub fn settle_uncaught_exception(&mut self) {
        if self.trap_flag != STATE_EXCEPTION {
            return;
        }
        self.trap_flag = STATE_NONE;
        if let Some(pending) = self.pending_exception.take() {
            self.trap(TrapKind::UncaughtException, pending.message, pending.pos_id);
        }
    }

    /// Sets the pending exception aside and clears the word, at the start
    /// of the hooks of an exception exit (`compiler.md` §115.5 rule 7).
    ///
    /// A park with no pending exception is a code-generation defect. It
    /// records an internal trap.
    pub fn park_exception(&mut self) {
        if self.trap_flag != STATE_EXCEPTION {
            self.trap(
                TrapKind::Internal,
                "an exception exit ran with no pending exception",
                0,
            );
            return;
        }
        self.trap_flag = STATE_NONE;
        if let Some(pending) = self.pending_exception.take() {
            self.parked_exceptions.push(pending);
        }
    }

    /// Makes the exception of the innermost park pending again, with its
    /// object, report text, and position (`compiler.md` §115.5 rule 7).
    ///
    /// A resume with no parked exception, or with a pending exception or
    /// a trap, is a code-generation defect. It records an internal trap.
    pub fn resume_exception(&mut self) {
        if self.trap_flag != STATE_NONE {
            self.trap(
                TrapKind::Internal,
                "an exception exit resumed while the word was not clear",
                0,
            );
            return;
        }
        match self.parked_exceptions.pop() {
            Some(parked) => {
                self.pending_exception = Some(parked);
                self.trap_flag = STATE_EXCEPTION;
            }
            None => self.trap(
                TrapKind::Internal,
                "an exception exit resumed with no parked exception",
                0,
            ),
        }
    }

    /// The number of exceptions that wait for the hooks of an exception
    /// exit.
    #[must_use]
    pub fn parked_exception_count(&self) -> usize {
        self.parked_exceptions.len()
    }

    /// The address of the pending Error object, if one is pending.
    #[must_use]
    pub fn pending_exception_object(&self) -> Option<*mut u8> {
        self.pending_exception
            .as_ref()
            .map(|pending| pending.object as *mut u8)
    }
}

/// `throw`: records `object` as the pending exception at `pos_id`.
///
/// `name` and `message` are the Error's two string fields at the throw.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract. `object` is a live
/// Error allocation of this Context. `name` and `message` are live string
/// handles of this Context, or null.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_exception_throw(
    ctx: *mut Context,
    object: *mut u8,
    name: *const u8,
    message: *const u8,
    pos_id: u32,
) {
    // SAFETY: exclusive Context contract.
    let ctx = unsafe { &mut *ctx };
    let read = |handle: *const u8| -> Vec<u8> {
        if handle.is_null() {
            Vec::new()
        } else {
            // SAFETY: a non-null handle is a live string of this Context.
            unsafe { ctx.str_bytes(handle) }.to_vec()
        }
    };
    let text = exception_message(&read(name), &read(message));
    ctx.raise_exception(object, text, pos_id);
}

/// A catch entry: takes the pending Error object and clears the word.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_exception_catch(ctx: *mut Context) -> *mut u8 {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.catch_exception()
}

/// An exception boundary: converts a pending exception into the trap
/// [`TrapKind::UncaughtException`] (`compiler.md` §115.4 items 2 and 3).
///
/// The unwind exit of an `async` body and of a generator body calls it,
/// so no exception leaves the body. With no pending exception, the call
/// does nothing.
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_exception_settle(ctx: *mut Context) {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.settle_uncaught_exception();
}

/// The start of the hooks of an exception exit: parks the pending
/// exception and clears the word (`compiler.md` §115.5 rule 7).
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_exception_park(ctx: *mut Context) {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.park_exception();
}

/// The end of the hooks of an exception exit: makes the parked exception
/// pending again (`compiler.md` §115.5 rule 7).
///
/// # Safety
///
/// `ctx` follows the exclusive Context contract.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_exception_resume(ctx: *mut Context) {
    // SAFETY: exclusive Context contract.
    unsafe { &mut *ctx }.resume_exception();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raised(ctx: &mut Context) -> *mut u8 {
        let object = ctx.alloc(24, 1, 0);
        ctx.raise_exception(object, "Error: failure".to_string(), 7);
        object
    }

    #[test]
    fn exception_message_follows_error_to_string() {
        assert_eq!(exception_message(b"Error", b"failure"), "Error: failure");
        assert_eq!(exception_message(b"TypeError", b""), "TypeError");
        assert_eq!(exception_message(b"", b"failure"), "failure");
        assert_eq!(exception_message(b"", b""), "");
    }

    #[test]
    fn raise_sets_the_exception_state_and_catch_clears_it() {
        let mut ctx = Context::new();
        let object = raised(&mut ctx);
        assert!(ctx.trapped(), "a pending exception stops a nonzero check");
        assert!(ctx.exception_pending());
        assert_eq!(ctx.pending_exception_object(), Some(object));
        assert!(ctx.trap_record().is_none(), "an exception is not a trap");
        assert_eq!(ctx.catch_exception(), object);
        assert!(!ctx.trapped());
        assert!(!ctx.exception_pending());
        assert_eq!(ctx.pending_exception_object(), None);
    }

    #[test]
    fn a_trap_overwrites_a_pending_exception_and_wins_over_a_later_raise() {
        let mut ctx = Context::new();
        raised(&mut ctx);
        ctx.trap(TrapKind::DivisionByZero, "integer division by zero", 3);
        assert!(!ctx.exception_pending());
        assert_eq!(ctx.pending_exception_object(), None);
        let object = ctx.alloc(24, 1, 0);
        ctx.raise_exception(object, "Error: later".to_string(), 9);
        assert!(!ctx.exception_pending(), "a trap wins over a later raise");
        assert_eq!(
            ctx.trap_record().map(|record| (record.kind, record.pos_id)),
            Some((TrapKind::DivisionByZero, 3))
        );
    }

    #[test]
    fn settle_converts_a_pending_exception_into_the_uncaught_trap() {
        let mut ctx = Context::new();
        raised(&mut ctx);
        ctx.settle_uncaught_exception();
        assert!(!ctx.exception_pending());
        let record = ctx.trap_record().expect("uncaught trap");
        assert_eq!(record.kind, TrapKind::UncaughtException);
        assert_eq!(record.message, "Error: failure");
        assert_eq!(record.pos_id, 7);
    }

    #[test]
    fn settle_without_a_pending_exception_changes_nothing() {
        let mut ctx = Context::new();
        ctx.settle_uncaught_exception();
        assert!(!ctx.trapped());
        assert!(ctx.trap_record().is_none());
    }

    #[test]
    fn the_outermost_host_exit_settles_a_pending_exception() {
        let mut ctx = Context::new();
        ctx.enter_script();
        ctx.enter_script();
        raised(&mut ctx);
        ctx.exit_script();
        assert!(
            ctx.exception_pending(),
            "an inner host exit keeps the exception"
        );
        ctx.exit_script();
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::UncaughtException)
        );
    }

    #[test]
    fn the_boundary_entry_settles_a_pending_exception_and_keeps_a_trap() {
        let mut ctx = Context::new();
        raised(&mut ctx);
        // SAFETY: live Context.
        unsafe { subscript_rt_exception_settle(&mut *ctx) };
        let record = ctx.trap_record().expect("uncaught trap");
        assert_eq!(
            (record.kind, record.message.as_str(), record.pos_id),
            (TrapKind::UncaughtException, "Error: failure", 7)
        );
        let mut ctx = Context::new();
        ctx.trap(TrapKind::DivisionByZero, "integer division by zero", 3);
        // SAFETY: live Context.
        unsafe { subscript_rt_exception_settle(&mut *ctx) };
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::DivisionByZero),
            "a trap passes the boundary unchanged"
        );
        let mut ctx = Context::new();
        // SAFETY: live Context.
        unsafe { subscript_rt_exception_settle(&mut *ctx) };
        assert!(!ctx.trapped(), "a clear word stays clear");
    }

    /// A script callback that returns with an exception pending.
    unsafe extern "C" fn raising_callback(
        ctx: *mut Context,
        _env: *const u8,
        _message: *mut u8,
        _userdata1: *mut u8,
        _userdata2: *mut u8,
    ) {
        // SAFETY: the trampoline passes the live Context.
        let ctx = unsafe { &mut *ctx };
        let object = ctx.alloc(24, 1, 0);
        ctx.raise_exception(object, "Error: callback failed".to_string(), 12);
    }

    #[test]
    fn the_callback_trampoline_settles_a_pending_exception() {
        let mut ctx = Context::new();
        // SAFETY: live Context; the callback has the trampoline shape.
        let binding = unsafe {
            crate::ffi::subscript_rt_cb_bind(
                &mut *ctx,
                raising_callback as *const () as *const u8,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert!(!binding.is_null());
        let message = crate::ffi::SubStrView {
            data: b"m".as_ptr(),
            len: 1,
        };
        // SAFETY: the binding belongs to this live Context.
        unsafe { crate::ffi::subscript_rt_cb_trampoline(message, binding, std::ptr::null_mut()) };
        assert!(!ctx.exception_pending(), "no exception crosses the C frame");
        let record = ctx.trap_record().expect("uncaught trap");
        assert_eq!(
            (record.kind, record.message.as_str(), record.pos_id),
            (TrapKind::UncaughtException, "Error: callback failed", 12)
        );
    }

    #[test]
    fn park_and_resume_keep_the_object_the_text_and_the_position() {
        let mut ctx = Context::new();
        let outer = raised(&mut ctx);
        ctx.park_exception();
        assert!(!ctx.trapped(), "the hooks run with a clear word");
        assert_eq!(ctx.parked_exception_count(), 1);
        // A hook that throws and catches inside parks nothing.
        let inner = ctx.alloc(24, 1, 0);
        ctx.raise_exception(inner, "Error: inner".to_string(), 11);
        assert_eq!(ctx.catch_exception(), inner);
        ctx.resume_exception();
        assert!(ctx.exception_pending());
        assert_eq!(ctx.parked_exception_count(), 0);
        assert_eq!(ctx.pending_exception_object(), Some(outer));
        ctx.settle_uncaught_exception();
        let record = ctx.trap_record().expect("uncaught trap");
        assert_eq!(
            (record.kind, record.message.as_str(), record.pos_id),
            (TrapKind::UncaughtException, "Error: failure", 7),
            "the report cites the first throw, not the exit"
        );
    }

    #[test]
    fn nested_parks_resume_innermost_first() {
        let mut ctx = Context::new();
        let first = raised(&mut ctx);
        // SAFETY: live Context.
        unsafe { subscript_rt_exception_park(&mut *ctx) };
        let second = ctx.alloc(24, 1, 0);
        ctx.raise_exception(second, "Error: second".to_string(), 8);
        // SAFETY: live Context.
        unsafe { subscript_rt_exception_park(&mut *ctx) };
        assert_eq!(ctx.parked_exception_count(), 2);
        // SAFETY: live Context.
        unsafe { subscript_rt_exception_resume(&mut *ctx) };
        assert_eq!(ctx.catch_exception(), second);
        // SAFETY: live Context.
        unsafe { subscript_rt_exception_resume(&mut *ctx) };
        assert_eq!(ctx.pending_exception_object(), Some(first));
    }

    #[test]
    fn a_trap_drops_the_parked_exceptions() {
        let mut ctx = Context::new();
        raised(&mut ctx);
        ctx.park_exception();
        ctx.trap(TrapKind::DisposeRaisedDuringExit, "hook raised", 5);
        assert_eq!(ctx.parked_exception_count(), 0);
        let mut ctx = Context::new();
        raised(&mut ctx);
        ctx.park_exception();
        ctx.clear_trap();
        assert_eq!(ctx.parked_exception_count(), 0);
    }

    #[test]
    fn park_and_resume_out_of_order_are_internal_traps() {
        let mut ctx = Context::new();
        ctx.park_exception();
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::Internal)
        );
        let mut ctx = Context::new();
        ctx.resume_exception();
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::Internal)
        );
        let mut ctx = Context::new();
        raised(&mut ctx);
        ctx.park_exception();
        raised(&mut ctx);
        ctx.resume_exception();
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::Internal),
            "a resume over a pending exception"
        );
    }

    #[test]
    fn a_parked_exception_is_a_collection_root() {
        let mut ctx = Context::new();
        let object = raised(&mut ctx);
        ctx.park_exception();
        ctx.collect();
        assert!(ctx.is_live(object as usize), "the parked object survives");
        ctx.resume_exception();
        assert_eq!(ctx.catch_exception(), object);
        ctx.collect();
        assert!(!ctx.is_live(object as usize), "the firing control");
    }

    #[test]
    fn a_raise_over_a_pending_exception_is_an_internal_trap() {
        let mut ctx = Context::new();
        let first = raised(&mut ctx);
        assert_eq!(ctx.pending_exception_object(), Some(first));
        let second = ctx.alloc(24, 1, 0);
        ctx.raise_exception(second, "Error: second".to_string(), 9);
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::Internal)
        );
        assert!(!ctx.exception_pending(), "the trap overwrites the word");
        assert_eq!(ctx.pending_exception_object(), None);
        let mut ctx = Context::new();
        raised(&mut ctx);
        assert!(
            ctx.trap_record().is_none(),
            "the firing control: one raise records no trap"
        );
    }

    #[test]
    fn catch_without_a_pending_exception_is_an_internal_trap() {
        let mut ctx = Context::new();
        assert!(ctx.catch_exception().is_null());
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::Internal)
        );
    }

    #[test]
    fn clear_trap_drops_a_pending_exception() {
        let mut ctx = Context::new();
        raised(&mut ctx);
        ctx.clear_trap();
        assert!(!ctx.trapped());
        assert_eq!(ctx.pending_exception_object(), None);
    }

    #[test]
    fn a_pending_exception_is_a_collection_root() {
        let mut ctx = Context::new();
        let object = raised(&mut ctx);
        ctx.collect();
        assert!(ctx.is_live(object as usize), "the pending object survives");
        assert_eq!(ctx.catch_exception(), object);
        ctx.collect();
        assert!(
            !ctx.is_live(object as usize),
            "a caught object with no other root is collected"
        );
    }

    #[test]
    fn the_ffi_entries_format_the_report_text() {
        let mut ctx = Context::new();
        let object = ctx.alloc(24, 1, 0);
        let name = ctx.alloc_str(b"SyntaxError", 0);
        let message = ctx.alloc_str(b"bad token", 0);
        // SAFETY: live Context, object, and string handles.
        unsafe { subscript_rt_exception_throw(&mut *ctx, object, name, message, 4) };
        assert!(ctx.exception_pending());
        ctx.settle_uncaught_exception();
        assert_eq!(
            ctx.trap_record().map(|record| record.message.as_str()),
            Some("SyntaxError: bad token")
        );
        let mut ctx = Context::new();
        let object = ctx.alloc(24, 1, 0);
        // SAFETY: live Context and object; null string handles read as empty.
        unsafe {
            subscript_rt_exception_throw(&mut *ctx, object, std::ptr::null(), std::ptr::null(), 1);
            assert_eq!(subscript_rt_exception_catch(&mut *ctx), object);
        }
    }
}
