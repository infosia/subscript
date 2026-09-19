//! Callback registrations with an explicit end
//! (`specs/blocks/compiler.md` §111).
//!
//! A boundary callback does not capture, so the state of one host
//! request travels in userdata. The Context-lifetime binding of §14.4a
//! keeps one record for each distinct identity until the Context ends.
//! A registration is the second lifetime: each crossing creates one
//! record, and the host ends that record with one release call.
//!
//! The live set holds open and active registrations only (§111 rule 8).
//! `Context.collect` derives its roots from that set. A record leaves
//! the set when the host released it and no call is active, which is
//! the root predicate `open || active_calls > 0` of §111 rule 6.
//!
//! Everything here runs on the Context owner thread, so the call
//! counter is not atomic (§111 rule 11).

use std::collections::HashMap;
use std::ffi::c_void;
use std::hash::BuildHasherDefault;

use crate::context::{AddressHasher, Context};
use crate::ffi::SubStrView;
use crate::trap::TrapKind;

/// The bytes one registration record charges the allocation quota
/// (§109.4 rule 2, §111 rule 10).
const REGISTRATION_RECORD_BYTES: usize = std::mem::size_of::<CallbackRegistration>();

/// One callback registration (§111 rule 3).
///
/// The record is not interned: two crossings with the same callback and
/// the same userdata create two records, and each one ends apart from
/// the other. The host receives the record address in the first C
/// userdata slot (§111 rule 4) and treats it as opaque. Only the
/// runtime reads the fields, so the record takes the Rust layout and
/// carries no C contract.
pub struct CallbackRegistration {
    /// The Context the script runs under; captured at registration.
    ctx: *mut Context,
    /// The language function value's code pointer (a wrapper taking
    /// `(ctx, env, args...)`, host C calling convention).
    code: *const u8,
    /// The language function value's environment pointer (null for a
    /// non-capturing function — the only kind usable as a C callback,
    /// C5).
    env: *const u8,
    /// The first userdata the script registered (§14.4).
    userdata1: *mut u8,
    /// The second userdata (§14.4); null when the callback-info carries
    /// only one slot.
    userdata2: *mut u8,
    /// True until the host releases the registration (§111 rule 5).
    open: bool,
    /// The number of calls that run through this registration right now
    /// (§111 rule 6).
    active_calls: u32,
}

/// The live set of one Context: the open and the active registrations,
/// keyed by record address (§111 rule 8).
///
/// The map gives O(1) insert, O(1) remove, and O(1) membership by
/// pointer. A `Box` keeps each record at one address for its whole
/// life, so the key and the host's pointer are the same value.
pub(crate) type RegistrationSet =
    HashMap<usize, Box<CallbackRegistration>, BuildHasherDefault<AddressHasher>>;

/// The fields one fired call copies out of a record before it runs
/// (§111 rule 6).
pub(crate) struct FiredRegistration {
    /// The language function value's code pointer.
    pub(crate) code: *const u8,
    /// The language function value's environment pointer.
    pub(crate) env: *const u8,
    /// The first userdata the script registered.
    pub(crate) userdata1: *mut u8,
    /// The second userdata.
    pub(crate) userdata2: *mut u8,
}

/// Reads the Context one registration captured.
///
/// The trampoline receives the record address and nothing else, so it
/// reads this field before it can reach the live set.
///
/// # Safety
///
/// `registration` is a registration pointer that
/// [`Context::register_callback`] returned and the host did not release
/// (§111 rule 14).
pub(crate) unsafe fn registration_context(registration: *mut u8) -> *mut Context {
    // SAFETY: the caller supplies a live registration record.
    unsafe { (*registration.cast::<CallbackRegistration>()).ctx }
}

impl Context {
    /// Creates one callback registration and returns the pointer a
    /// boundary marshaler stores in a C `void *` userdata slot
    /// (§111 rule 3, §111 rule 4).
    ///
    /// The record is not interned. Two crossings with the same callback
    /// and the same userdata create two registrations, and each one
    /// ends apart from the other, so the pointer equality of §14.4a
    /// consequence 1 does not hold here.
    ///
    /// The record charges the allocation quota as a binding record does
    /// (§109.4 rule 2, §111 rule 10). The record exists when the quota
    /// refuses it, because a null in a C `void *` slot is never
    /// published; the recorded trap stops the run at the next
    /// checkpoint.
    ///
    /// Both userdata slots (§14.4) are stored and delivered to the
    /// language callback; a one-slot callback-info passes `userdata2` as
    /// null.
    ///
    /// `pos_id` is the position of the crossing that creates the
    /// registration (§112 rule 4). A quota refusal reports that site.
    pub fn register_callback(
        &mut self,
        code: *const u8,
        env: *const u8,
        userdata1: *mut u8,
        userdata2: *mut u8,
        pos_id: u32,
    ) -> *mut u8 {
        debug_assert!(
            env.is_null(),
            "§111 rule 3 registers a boundary callback, whose env is null"
        );
        self.check_quota(REGISTRATION_RECORD_BYTES, pos_id);
        self.binding_charge = self
            .binding_charge
            .saturating_add(REGISTRATION_RECORD_BYTES);

        let ctx: *mut Context = self;
        let mut record = Box::new(CallbackRegistration {
            ctx,
            code,
            env,
            userdata1,
            userdata2,
            open: true,
            active_calls: 0,
        });
        let pointer: *mut CallbackRegistration = &mut *record;
        self.registrations.insert(pointer as usize, record);
        #[cfg(test)]
        live_records_add(1);
        self.advise_binding_count();
        pointer.cast()
    }

    /// Ends one callback registration (§111 rule 5).
    ///
    /// The call states a guarantee: the host starts no more calls
    /// through this registration. Calls that already run can return.
    /// The call does not cancel native work and does not unregister a
    /// native callback; the host adapter does those first.
    ///
    /// The call returns `true` when `registration` is an open
    /// registration of this Context. For every other pointer it changes
    /// nothing and returns `false`: null, a binding of
    /// [`Context::bind_callback`], a registration it already closed,
    /// and a registration that ended. The live-set membership test
    /// comes first, so the call never reads a pointer the set does not
    /// hold.
    ///
    /// After a registration ends, a later registration can take its
    /// address. A second release of the old pointer then closes the new
    /// registration. That case is in the best-effort class of §111 rule
    /// 14: the host ends each registration one time, and the runtime
    /// keeps no record of the registrations that ended.
    ///
    /// Release removes a root and does nothing else (§111 rule 7). It
    /// does not collect, it does not free the userdata, and it does not
    /// walk the userdata graph. The userdata then follows the
    /// reachability rules of the Context.
    pub fn release_callback_registration(&mut self, registration: *mut c_void) -> bool {
        let key = registration as usize;
        let Some(record) = self.registrations.get_mut(&key) else {
            return false;
        };
        if !record.open {
            return false;
        }
        record.open = false;
        if record.active_calls == 0 {
            self.end_registration(key);
        }
        true
    }

    /// The number of registrations in the live set (§111 rule 8).
    ///
    /// A registration that ended leaves no entry, so the count is the
    /// open registrations plus the closed ones that still have an
    /// active call.
    #[must_use]
    pub fn live_registration_count(&self) -> usize {
        self.registrations.len()
    }

    /// Counts one call up before the trampoline validates userdata,
    /// allocates, or enters script code (§111 rule 6).
    ///
    /// The return carries the fields the call needs. `None` means the
    /// live set holds no open record at this pointer, so no call starts
    /// and no count changes. The record is then closed, or the set does
    /// not hold it, and each of those is a fire through a registration
    /// the host released: the call records the
    /// `callback-registration-ended` trap at position 0 (§111 rule 14).
    /// The refused fire enters no script code, so no script site exists
    /// (§112 rule 4). Id 0 is the reserved entry of §112 rule 1, so
    /// every tier reports the empty position.
    pub(crate) fn registration_enter(
        &mut self,
        registration: *mut u8,
    ) -> Option<FiredRegistration> {
        let open = self
            .registrations
            .get_mut(&(registration as usize))
            .filter(|record| record.open);
        let Some(record) = open else {
            self.trap(
                TrapKind::CallbackRegistrationEnded,
                "a callback fired through a registration the host released",
                0,
            );
            return None;
        };
        record.active_calls = record.active_calls.saturating_add(1);
        Some(FiredRegistration {
            code: record.code,
            env: record.env,
            userdata1: record.userdata1,
            userdata2: record.userdata2,
        })
    }

    /// Counts one call down (§111 rule 6). A trap and a failed userdata
    /// check are exits, so every exit path calls this one time.
    ///
    /// When the host released the registration and no call is active,
    /// the record leaves the live set, returns its charge, and frees
    /// its storage before the trampoline returns.
    pub(crate) fn registration_exit(&mut self, registration: *mut u8) {
        let key = registration as usize;
        let Some(record) = self.registrations.get_mut(&key) else {
            return;
        };
        record.active_calls = record.active_calls.saturating_sub(1);
        if record.open || record.active_calls != 0 {
            return;
        }
        self.end_registration(key);
    }

    /// Removes one record from the live set, returns its quota charge,
    /// and frees its storage (§111 rule 6).
    fn end_registration(&mut self, key: usize) {
        if self.registrations.remove(&key).is_some() {
            self.binding_charge = self
                .binding_charge
                .saturating_sub(REGISTRATION_RECORD_BYTES);
        }
    }

    /// The collection roots the live set derives (§111 rule 8,
    /// §14.4b (C)): each userdata slot that is a live allocation.
    ///
    /// A slot that is null or freed is skipped, exactly as the binding
    /// records' slots are.
    ///
    /// The index a root carries is the record's rank in key order. The
    /// map gives no order, so a mark trace of one run repeats only when
    /// the ranks come from the keys.
    pub(crate) fn registration_roots(&self) -> Vec<(usize, usize, usize)> {
        let mut keys: Vec<usize> = self.registrations.keys().copied().collect();
        keys.sort_unstable();
        let mut roots = Vec::new();
        for (index, key) in keys.into_iter().enumerate() {
            let Some(record) = self.registrations.get(&key) else {
                continue;
            };
            for (word, userdata) in [record.userdata1, record.userdata2].into_iter().enumerate() {
                let address = userdata as usize;
                if !userdata.is_null() && self.is_live(address) {
                    roots.push((index, word, address));
                }
            }
        }
        roots
    }

    /// True when a live registration holds `payload` in a userdata slot
    /// (§111 rule 9, §14.4b (B)).
    pub(crate) fn registration_holds_userdata(&self, payload: usize) -> bool {
        self.registrations.values().any(|record| {
            record.userdata1 as usize == payload || record.userdata2 as usize == payload
        })
    }

    /// The active-call count of one registration (§111 rule 6), or
    /// `None` when the live set does not hold the pointer.
    #[cfg(test)]
    pub(crate) fn registration_active_calls(&self, registration: *mut u8) -> Option<u32> {
        self.registrations
            .get(&(registration as usize))
            .map(|record| record.active_calls)
    }

    /// True when the live set holds `registration` and the host did not
    /// release it (§111 rule 5).
    #[cfg(test)]
    pub(crate) fn registration_is_open(&self, registration: *mut u8) -> bool {
        self.registrations
            .get(&(registration as usize))
            .is_some_and(|record| record.open)
    }
}

/// Creates one callback registration and returns the pointer a boundary
/// marshaler stores in a C `void *` userdata slot (§111 rule 3).
///
/// The parameter shape is that of
/// [`crate::ffi::subscript_rt_cb_bind`]. Nothing is interned: each call
/// creates one record. [`subscript_rt_cb_registration_trampoline`]
/// reads the record back, and
/// [`crate::ffi::subscript_rt_ctx_callback_release`] ends it.
///
/// # Safety
///
/// Shared contract; `code`/`env` are a language function value (a
/// non-capturing wrapper, so `env` is null); `userdata1`/`userdata2`
/// outlive every call the host starts through this registration.
/// `pos_id` is the position of the crossing (§112 rule 4).
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_cb_register(
    ctx: *mut Context,
    code: *const u8,
    env: *const u8,
    userdata1: *mut u8,
    userdata2: *mut u8,
    pos_id: u32,
) -> *mut u8 {
    // SAFETY: shared contract.
    unsafe { &mut *ctx }.register_callback(code, env, userdata1, userdata2, pos_id)
}

/// The explicit-lifetime C-ABI callback trampoline (§111 rule 4).
///
/// The C ABI is that of [`crate::ffi::subscript_rt_cb_trampoline`]. The
/// marshaling writes this function into the callback field, the
/// registration pointer into the first userdata field, and null into
/// the second. The registration is the authoritative source of both
/// language userdata, so this function ignores its `userdata2`
/// argument.
///
/// It finds the record through the live set. For an open record it adds
/// one to the call count before it validates userdata, allocates, or
/// enters script code, and it subtracts one on every exit (§111 rule
/// 6). A trap and a failed userdata check are exits. When the host
/// released the registration and the count reaches zero, the record
/// leaves the live set and frees before this function returns; nothing
/// reads the record after that.
///
/// A fire through a registration the host released enters no script
/// code, changes no count, and records the
/// `callback-registration-ended` trap at position 0 (§111 rule 14).
/// The trap is certain while a call keeps a closed record alive,
/// because the live set still holds the record. After the record ends
/// the trap is best-effort, in the class of §14.4b (A): certain while
/// the storage is not used again, and undefined after that. The
/// function reads the Context out of the record before it reaches the
/// live set, so a fire through storage that is used again is
/// undefined.
///
/// # Safety
///
/// `userdata1` is a registration that [`subscript_rt_cb_register`]
/// produced on the running Context. A fire after the record ended is
/// undefined once the storage is used again (§111 rule 14); `message`
/// points at `len` readable bytes (or is null/empty).
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_cb_registration_trampoline(
    message: SubStrView,
    userdata1: *mut u8,
    userdata2: *mut u8,
) {
    // The registration travels in the first slot; the second slot is
    // unused (the record carries both language userdata).
    let _ = userdata2;
    if userdata1.is_null() {
        return;
    }
    // The trampoline receives the record address and nothing else, so it
    // reads the Context before it can reach the live set. After the
    // record ends this read is the best-effort half of §111 rule 14.
    // SAFETY: the caller contract supplies a registration of the running
    // Context.
    let ctx_ptr = unsafe { registration_context(userdata1) };
    if ctx_ptr.is_null() {
        return;
    }
    // SAFETY: `ctx_ptr` is the live Context captured at registration.
    // Scripts are single-threaded and trusted (invariant 6), and the
    // host brings every notification to the owner thread before it
    // fires (§111 rule 11).
    let fired = unsafe { &mut *ctx_ptr }.registration_enter(userdata1);
    // `registration_enter` recorded the §111 rule 14 trap already.
    let Some(fired) = fired else {
        return;
    };
    // SAFETY: the callback ABI guarantees the readable message view, and
    // the record carries a language callback of the shared shape.
    unsafe {
        crate::ffi::fire_callback(
            ctx_ptr,
            fired.code,
            fired.env,
            fired.userdata1,
            fired.userdata2,
            message,
        );
    }
    // SAFETY: the same live Context. This is the one exit of this call,
    // so the count falls by one whether the callback ran, trapped, or
    // failed its userdata check.
    unsafe { &mut *ctx_ptr }.registration_exit(userdata1);
}

// The live record count of the running thread. A Context belongs to one
// thread (§111 rule 11), so a per-thread counter observes Context
// destruction (§111 rule 12) with no race against the other tests of
// the process.
#[cfg(test)]
thread_local! {
    static LIVE_RECORDS: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
}

/// Moves the running thread's live record count by `delta`.
#[cfg(test)]
fn live_records_add(delta: isize) {
    LIVE_RECORDS.with(|count| count.set(count.get().saturating_add(delta)));
}

/// The running thread's live record count.
#[cfg(test)]
fn live_records() -> isize {
    LIVE_RECORDS.with(std::cell::Cell::get)
}

#[cfg(test)]
impl Drop for CallbackRegistration {
    fn drop(&mut self) {
        live_records_add(-1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CallbackBinding, DIAGNOSTICS_ADVISORY_BINDING_COUNT};
    use std::cell::Cell;
    use std::time::Instant;

    thread_local! {
        /// The number of times a test callback entered script code.
        static CALLS: Cell<u32> = const { Cell::new(0) };
        /// The registration a test callback acts on.
        static SUBJECT: Cell<*mut u8> = const { Cell::new(std::ptr::null_mut()) };
        /// What a test callback measured, read after the fire returns.
        static OBSERVED_LIVE_REGISTRATIONS: Cell<usize> = const { Cell::new(usize::MAX) };
        /// Whether the first userdata was live inside the callback.
        static OBSERVED_USERDATA_LIVE: Cell<bool> = const { Cell::new(false) };
        /// The call count the innermost nested fire observed.
        static OBSERVED_ACTIVE_CALLS: Cell<u32> = const { Cell::new(0) };
        /// The remaining nested fires a test callback starts.
        static NESTING_LEFT: Cell<u32> = const { Cell::new(0) };
        /// What the first release inside a callback answered.
        static OBSERVED_FIRST_RELEASE: Cell<i32> = const { Cell::new(-1) };
        /// What the second release inside the same callback answered.
        static OBSERVED_SECOND_RELEASE: Cell<i32> = const { Cell::new(-1) };
        /// The live record count a test callback read while its call ran.
        static OBSERVED_LIVE_RECORDS: Cell<isize> = const { Cell::new(isize::MIN) };
    }

    /// Clears the thread-local records the test callbacks write.
    fn reset_observations() {
        CALLS.with(|cell| cell.set(0));
        SUBJECT.with(|cell| cell.set(std::ptr::null_mut()));
        OBSERVED_LIVE_REGISTRATIONS.with(|cell| cell.set(usize::MAX));
        OBSERVED_USERDATA_LIVE.with(|cell| cell.set(false));
        OBSERVED_ACTIVE_CALLS.with(|cell| cell.set(0));
        NESTING_LEFT.with(|cell| cell.set(0));
        OBSERVED_FIRST_RELEASE.with(|cell| cell.set(-1));
        OBSERVED_SECOND_RELEASE.with(|cell| cell.set(-1));
        OBSERVED_LIVE_RECORDS.with(|cell| cell.set(isize::MIN));
    }

    /// An empty message view, so no test depends on string copy-in.
    fn empty_message() -> SubStrView {
        SubStrView {
            data: std::ptr::null(),
            len: 0,
        }
    }

    /// A language callback that only counts its entries.
    unsafe extern "C" fn counting(
        _ctx: *mut Context,
        _env: *const u8,
        _message: *mut u8,
        _userdata1: *mut u8,
        _userdata2: *mut u8,
    ) {
        CALLS.with(|cell| cell.set(cell.get() + 1));
    }

    /// A language callback that releases its own registration, collects,
    /// and records what it saw. It writes the records the test reads and
    /// asserts nothing, because an `extern "C"` frame never unwinds.
    unsafe extern "C" fn release_then_collect(
        ctx: *mut Context,
        _env: *const u8,
        _message: *mut u8,
        userdata1: *mut u8,
        _userdata2: *mut u8,
    ) {
        CALLS.with(|cell| cell.set(cell.get() + 1));
        let registration = SUBJECT.with(Cell::get);
        // SAFETY: the trampoline passes the live Context of the record.
        let ctx = unsafe { &mut *ctx };
        let released = ctx.release_callback_registration(registration.cast());
        OBSERVED_LIVE_REGISTRATIONS.with(|cell| cell.set(ctx.live_registration_count()));
        ctx.collect();
        OBSERVED_USERDATA_LIVE.with(|cell| cell.set(ctx.is_live(userdata1 as usize)));
        if !released {
            OBSERVED_ACTIVE_CALLS.with(|cell| cell.set(u32::MAX));
        }
    }

    /// A language callback that fires its own registration again while
    /// this call runs (§111.2 nested calls).
    ///
    /// The Context travels as a raw pointer, and each read of it ends
    /// inside one statement, so this frame holds no reference across the
    /// nested trampoline call.
    unsafe extern "C" fn fire_again(
        ctx: *mut Context,
        _env: *const u8,
        _message: *mut u8,
        _userdata1: *mut u8,
        _userdata2: *mut u8,
    ) {
        CALLS.with(|cell| cell.set(cell.get() + 1));
        let registration = SUBJECT.with(Cell::get);
        let left = NESTING_LEFT.with(Cell::get);
        if left > 0 {
            NESTING_LEFT.with(|cell| cell.set(left - 1));
            // SAFETY: the registration is open on the running Context and
            // the message view is empty.
            unsafe {
                subscript_rt_cb_registration_trampoline(
                    empty_message(),
                    registration,
                    std::ptr::null_mut(),
                );
            }
            return;
        }
        // SAFETY: the trampoline passes the live Context of the record.
        let active = unsafe { (*ctx).registration_active_calls(registration) };
        OBSERVED_ACTIVE_CALLS.with(|cell| cell.set(active.unwrap_or(0)));
    }

    /// A language callback that releases its own registration and then
    /// fires it again from inside the call it runs in (§111 rule 14,
    /// the certain case). It records what the second fire produced.
    ///
    /// The Context travels as a raw pointer, and each read of it ends
    /// inside one statement, so this frame holds no reference across the
    /// nested trampoline call.
    unsafe extern "C" fn release_then_fire_again(
        ctx: *mut Context,
        _env: *const u8,
        _message: *mut u8,
        _userdata1: *mut u8,
        _userdata2: *mut u8,
    ) {
        CALLS.with(|cell| cell.set(cell.get() + 1));
        let registration = SUBJECT.with(Cell::get);
        if NESTING_LEFT.with(Cell::get) == 0 {
            return;
        }
        NESTING_LEFT.with(|cell| cell.set(0));
        // SAFETY: the trampoline passes the live Context of the record.
        let (released, live) = unsafe {
            (
                crate::ffi::subscript_rt_ctx_callback_release(ctx, registration.cast()),
                (*ctx).live_registration_count(),
            )
        };
        OBSERVED_FIRST_RELEASE.with(|cell| cell.set(released));
        OBSERVED_LIVE_REGISTRATIONS.with(|cell| cell.set(live));
        // SAFETY: the record is closed and this call keeps it alive, so
        // the live set still holds its storage.
        unsafe {
            subscript_rt_cb_registration_trampoline(
                empty_message(),
                registration,
                std::ptr::null_mut(),
            );
        }
        // SAFETY: the same live Context; the read ends in this statement.
        let active = unsafe { (*ctx).registration_active_calls(registration) };
        OBSERVED_ACTIVE_CALLS.with(|cell| cell.set(active.unwrap_or(0)));
    }

    /// A language callback that releases its own registration two times
    /// (§111 rule 5). The second release meets the closed record that
    /// this call keeps in the live set (§111 rule 6).
    unsafe extern "C" fn release_two_times(
        ctx: *mut Context,
        _env: *const u8,
        _message: *mut u8,
        _userdata1: *mut u8,
        _userdata2: *mut u8,
    ) {
        CALLS.with(|cell| cell.set(cell.get() + 1));
        let registration = SUBJECT.with(Cell::get);
        // SAFETY: the trampoline passes the live Context of the record,
        // and each read of it ends inside one statement.
        let (first, second, live) = unsafe {
            (
                crate::ffi::subscript_rt_ctx_callback_release(ctx, registration.cast()),
                crate::ffi::subscript_rt_ctx_callback_release(ctx, registration.cast()),
                (*ctx).live_registration_count(),
            )
        };
        OBSERVED_FIRST_RELEASE.with(|cell| cell.set(first));
        OBSERVED_SECOND_RELEASE.with(|cell| cell.set(second));
        OBSERVED_LIVE_REGISTRATIONS.with(|cell| cell.set(live));
    }

    /// A language callback that releases its own registration and reads
    /// the live record count while this call still runs (§111 rule 6).
    unsafe extern "C" fn release_and_count_records(
        ctx: *mut Context,
        _env: *const u8,
        _message: *mut u8,
        _userdata1: *mut u8,
        _userdata2: *mut u8,
    ) {
        CALLS.with(|cell| cell.set(cell.get() + 1));
        let registration = SUBJECT.with(Cell::get);
        // SAFETY: the trampoline passes the live Context of the record.
        let first =
            unsafe { crate::ffi::subscript_rt_ctx_callback_release(ctx, registration.cast()) };
        OBSERVED_FIRST_RELEASE.with(|cell| cell.set(first));
        OBSERVED_LIVE_RECORDS.with(|cell| cell.set(live_records()));
    }

    /// A language callback that records a trap and returns.
    unsafe extern "C" fn trapping(
        ctx: *mut Context,
        _env: *const u8,
        _message: *mut u8,
        _userdata1: *mut u8,
        _userdata2: *mut u8,
    ) {
        CALLS.with(|cell| cell.set(cell.get() + 1));
        // SAFETY: the trampoline passes the live Context of the record.
        unsafe { &mut *ctx }.trap(TrapKind::Internal, "the callback trapped", 0);
    }

    /// The code pointer of one test callback.
    fn code_of(
        callback: unsafe extern "C" fn(*mut Context, *const u8, *mut u8, *mut u8, *mut u8),
    ) -> *const u8 {
        callback as *const () as *const u8
    }

    /// Fires one registration with an empty message.
    fn fire(registration: *mut u8) {
        // SAFETY: the registration is open on its own Context and the
        // message view is empty.
        unsafe {
            subscript_rt_cb_registration_trampoline(
                empty_message(),
                registration,
                std::ptr::null_mut(),
            );
        }
    }

    #[test]
    fn register_callback_creates_one_open_record_for_each_crossing() {
        reset_observations();
        let mut ctx = Context::new();
        let mut userdata = 1u8;
        let userdata = std::ptr::from_mut(&mut userdata);
        let code = code_of(counting);

        let first =
            ctx.register_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);
        assert!(!first.is_null(), "§111 rule 10 publishes no null");
        assert_eq!(ctx.live_registration_count(), 1);
        assert!(ctx.registration_is_open(first));
        assert_eq!(ctx.registration_active_calls(first), Some(0));

        // §111 rule 3: the same (code, userdata) is a second record.
        let second =
            ctx.register_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);
        assert_ne!(first, second, "§111 rule 3 forbids interning");
        assert_eq!(ctx.live_registration_count(), 2);
    }

    #[test]
    fn two_registrations_of_one_identity_end_apart() {
        reset_observations();
        let mut ctx = Context::new();
        // §14.4b (A) checks userdata at fire, so a fired registration
        // carries a live allocation.
        let userdata = ctx.alloc(16, 1, 20);
        assert!(!userdata.is_null());
        let code = code_of(counting);
        let first =
            ctx.register_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);
        let second =
            ctx.register_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);

        assert!(ctx.release_callback_registration(first.cast()));
        assert_eq!(ctx.live_registration_count(), 1);
        assert!(ctx.registration_is_open(second), "the second stays open");

        fire(second);
        assert_eq!(CALLS.with(Cell::get), 1, "the second registration fires");

        assert!(ctx.release_callback_registration(second.cast()));
        assert_eq!(ctx.live_registration_count(), 0);
    }

    #[test]
    fn release_answers_false_for_every_pointer_that_is_not_an_open_registration() {
        reset_observations();
        let mut ctx = Context::new();
        let mut userdata = 1u8;
        let userdata = std::ptr::from_mut(&mut userdata);
        let code = code_of(counting);

        assert!(
            !ctx.release_callback_registration(std::ptr::null_mut()),
            "null is not an open registration"
        );

        let binding = ctx.bind_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);
        assert!(
            !ctx.release_callback_registration(binding.cast()),
            "a §14.4a binding is not a registration"
        );

        let registration =
            ctx.register_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);
        // The firing control: the same call on the open registration.
        assert!(ctx.release_callback_registration(registration.cast()));
        assert_eq!(ctx.live_registration_count(), 0);
        assert!(
            !ctx.release_callback_registration(registration.cast()),
            "a registration ends one time (§111 rule 5)"
        );
    }

    /// §111 rule 5: a second release meets a closed record that an
    /// active call keeps in the live set. It changes nothing, it answers
    /// 0, and the record still ends at the last return (§111 rule 6).
    #[test]
    fn a_second_release_inside_an_active_call_answers_false_and_keeps_the_record() {
        reset_observations();
        let mut ctx = Context::new();
        // §14.4b (A) checks userdata at fire, so a fired registration
        // carries a live allocation.
        let userdata = ctx.alloc(16, 1, 27);
        assert!(!userdata.is_null());
        let registration = ctx.register_callback(
            code_of(release_two_times),
            std::ptr::null(),
            userdata,
            std::ptr::null_mut(),
            0,
        );
        SUBJECT.with(|cell| cell.set(registration));

        fire(registration);

        assert_eq!(CALLS.with(Cell::get), 1);
        assert_eq!(
            OBSERVED_FIRST_RELEASE.with(Cell::get),
            1,
            "§111 rule 5 ends the open record"
        );
        assert_eq!(
            OBSERVED_SECOND_RELEASE.with(Cell::get),
            0,
            "§111 rule 5 answers 0 for a record it already closed"
        );
        assert_eq!(
            OBSERVED_LIVE_REGISTRATIONS.with(Cell::get),
            1,
            "§111 rule 6 keeps the closed record while the call runs"
        );
        assert_eq!(
            ctx.live_registration_count(),
            0,
            "the last return ends the record"
        );
    }

    #[test]
    fn release_inside_a_call_keeps_the_roots_until_the_last_return() {
        reset_observations();
        let mut ctx = Context::new();
        let inner = ctx.alloc(8, 1, 0);
        let outer = ctx.alloc(8, 1, 0);
        assert!(!inner.is_null() && !outer.is_null());
        // SAFETY: `outer` has 8 writable payload bytes.
        unsafe { outer.cast::<usize>().write(inner as usize) };

        let registration = ctx.register_callback(
            code_of(release_then_collect),
            std::ptr::null(),
            outer,
            std::ptr::null_mut(),
            0,
        );
        SUBJECT.with(|cell| cell.set(registration));

        fire(registration);

        assert_eq!(CALLS.with(Cell::get), 1);
        assert_eq!(
            OBSERVED_ACTIVE_CALLS.with(Cell::get),
            0,
            "the release inside the call must answer true"
        );
        assert_eq!(
            OBSERVED_LIVE_REGISTRATIONS.with(Cell::get),
            1,
            "§111 rule 6 keeps the record while a call is active"
        );
        assert!(
            OBSERVED_USERDATA_LIVE.with(Cell::get),
            "§111 rule 6 roots the userdata through the active call"
        );
        assert_eq!(
            ctx.live_registration_count(),
            0,
            "the last return ends the registration"
        );

        // After the last return nothing roots the graph.
        ctx.collect();
        assert!(!ctx.is_live(outer as usize));
        assert!(!ctx.is_live(inner as usize));
    }

    #[test]
    fn a_nested_fire_counts_both_calls_and_ends_after_the_last_return() {
        reset_observations();
        let mut ctx = Context::new();
        // §14.4b (A) checks userdata at fire, so a fired registration
        // carries a live allocation.
        let userdata = ctx.alloc(16, 1, 21);
        assert!(!userdata.is_null());
        let registration = ctx.register_callback(
            code_of(fire_again),
            std::ptr::null(),
            userdata,
            std::ptr::null_mut(),
            0,
        );
        SUBJECT.with(|cell| cell.set(registration));
        NESTING_LEFT.with(|cell| cell.set(1));

        fire(registration);

        assert_eq!(CALLS.with(Cell::get), 2, "the callback fires itself once");
        assert_eq!(
            OBSERVED_ACTIVE_CALLS.with(Cell::get),
            2,
            "§111 rule 6 counts each active call"
        );
        assert_eq!(ctx.registration_active_calls(registration), Some(0));
        assert!(ctx.registration_is_open(registration));

        // A release under the nested shape ends the record at the last
        // return: here every call already returned.
        assert!(ctx.release_callback_registration(registration.cast()));
        assert_eq!(ctx.live_registration_count(), 0);
    }

    #[test]
    fn a_trap_inside_the_callback_counts_the_call_down() {
        reset_observations();
        let mut ctx = Context::new();
        // §14.4b (A) checks userdata at fire, so a fired registration
        // carries a live allocation.
        let userdata = ctx.alloc(16, 1, 22);
        assert!(!userdata.is_null());
        let registration = ctx.register_callback(
            code_of(trapping),
            std::ptr::null(),
            userdata,
            std::ptr::null_mut(),
            0,
        );

        fire(registration);

        assert_eq!(CALLS.with(Cell::get), 1);
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::Internal)
        );
        assert_eq!(
            ctx.registration_active_calls(registration),
            Some(0),
            "§111 rule 6 makes a trap an exit"
        );
        // The record is still open, so release still ends it.
        assert!(ctx.release_callback_registration(registration.cast()));
        assert_eq!(ctx.live_registration_count(), 0);
    }

    #[test]
    fn a_failed_fire_time_userdata_check_counts_the_call_down() {
        reset_observations();
        let mut ctx = Context::new();
        let userdata = ctx.alloc(16, 1, 30);
        assert!(!userdata.is_null());
        let registration = ctx.register_callback(
            code_of(counting),
            std::ptr::null(),
            userdata,
            std::ptr::null_mut(),
            0,
        );
        ctx.delete(userdata as usize, 31);

        fire(registration);

        assert_eq!(
            CALLS.with(Cell::get),
            0,
            "§14.4b (A) stops the call before script code"
        );
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::CallbackUserdataFreed)
        );
        assert_eq!(
            ctx.registration_active_calls(registration),
            Some(0),
            "§111 rule 6 makes a failed check an exit"
        );

        // A release while the registration is closed and idle ends it.
        assert!(ctx.release_callback_registration(registration.cast()));
        assert_eq!(ctx.live_registration_count(), 0);
    }

    /// §111 rule 6: with no active call, the release itself frees the
    /// storage.
    #[test]
    fn a_release_with_no_active_call_frees_the_record_at_the_release() {
        reset_observations();
        let mut ctx = Context::new();
        let mut userdata = 1u8;
        let userdata = std::ptr::from_mut(&mut userdata);
        let before = live_records();
        let registration = ctx.register_callback(
            code_of(counting),
            std::ptr::null(),
            userdata,
            std::ptr::null_mut(),
            0,
        );
        assert_eq!(live_records(), before + 1);

        assert!(ctx.release_callback_registration(registration.cast()));
        assert_eq!(
            live_records(),
            before,
            "§111 rule 6 frees the record at the release"
        );
    }

    /// §111 rule 6: with a call active, the storage lives to the last
    /// return of that call. The callback reads the record counter while
    /// its own call runs, so the two reads are of one fact at two times.
    #[test]
    fn a_release_inside_an_active_call_frees_the_record_at_the_last_return() {
        reset_observations();
        let mut ctx = Context::new();
        // §14.4b (A) checks userdata at fire, so a fired registration
        // carries a live allocation.
        let userdata = ctx.alloc(16, 1, 28);
        assert!(!userdata.is_null());
        let before = live_records();
        let registration = ctx.register_callback(
            code_of(release_and_count_records),
            std::ptr::null(),
            userdata,
            std::ptr::null_mut(),
            0,
        );
        SUBJECT.with(|cell| cell.set(registration));
        assert_eq!(live_records(), before + 1);

        fire(registration);

        assert_eq!(CALLS.with(Cell::get), 1);
        assert_eq!(
            OBSERVED_FIRST_RELEASE.with(Cell::get),
            1,
            "§111 rule 5 ends the open record"
        );
        assert_eq!(
            OBSERVED_LIVE_RECORDS.with(Cell::get),
            before + 1,
            "§111 rule 6 keeps the storage while the call runs"
        );
        assert_eq!(
            live_records(),
            before,
            "§111 rule 6 frees the storage at the last return"
        );
        assert_eq!(ctx.live_registration_count(), 0);
    }

    #[test]
    fn context_destruction_frees_every_registration() {
        reset_observations();
        let before = live_records();
        {
            let mut ctx = Context::new();
            let mut userdata = 1u8;
            let userdata = std::ptr::from_mut(&mut userdata);
            let code = code_of(counting);
            for _ in 0..8 {
                ctx.register_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);
            }
            assert_eq!(ctx.live_registration_count(), 8);
            assert_eq!(live_records(), before + 8);
        }
        assert_eq!(
            live_records(),
            before,
            "§111 rule 12 frees every registration at destruction"
        );
    }

    /// §112 rule 4: the quota refusal of a registration reports the
    /// position of the crossing that asked for it.
    ///
    /// The loop runs two different ids. A body that records a constant
    /// passes one iteration and fails the other, so the second id is
    /// the control of the first.
    ///
    /// Cost: under 1 ms. Two Contexts and four registrations.
    #[test]
    fn a_refused_registration_records_the_position_the_crossing_passed() {
        reset_observations();
        let code = code_of(counting);
        let mut userdata = 1u8;
        let userdata = std::ptr::from_mut(&mut userdata);
        for crossing in [23u32, 907u32] {
            let mut ctx = Context::new();
            let first = ctx.register_callback(
                code,
                std::ptr::null(),
                userdata,
                std::ptr::null_mut(),
                crossing,
            );
            assert!(ctx.trap_record().is_none(), "the first record fits");
            ctx.set_alloc_quota(ctx.charged_bytes() as u64);
            let refused = ctx.register_callback(
                code,
                std::ptr::null(),
                userdata,
                std::ptr::null_mut(),
                crossing,
            );
            let record = ctx
                .trap_record()
                .expect("the quota refuses the second record");
            assert_eq!(record.kind, TrapKind::AllocationQuota);
            assert_eq!(
                record.pos_id, crossing,
                "the refusal reports the crossing, not the reserved entry"
            );
            assert!(ctx.release_callback_registration(first.cast()));
            assert!(ctx.release_callback_registration(refused.cast()));
        }
    }

    #[test]
    fn a_registration_charges_the_quota_and_exists_when_the_quota_refuses() {
        reset_observations();
        let mut ctx = Context::new();
        let mut userdata = 1u8;
        let userdata = std::ptr::from_mut(&mut userdata);
        let code = code_of(counting);
        let charged = ctx.charged_bytes();

        let first =
            ctx.register_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);
        assert_eq!(ctx.charged_bytes(), charged + REGISTRATION_RECORD_BYTES);
        let second =
            ctx.register_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);
        assert_eq!(
            ctx.charged_bytes(),
            charged + 2 * REGISTRATION_RECORD_BYTES,
            "§111 rule 3 charges each crossing"
        );

        assert!(ctx.release_callback_registration(second.cast()));
        assert_eq!(
            ctx.charged_bytes(),
            charged + REGISTRATION_RECORD_BYTES,
            "§111 rule 6 returns the charge"
        );

        // The firing control: a quota under the next record refuses it.
        ctx.set_alloc_quota(ctx.charged_bytes() as u64);
        let refused =
            ctx.register_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::AllocationQuota)
        );
        assert!(!refused.is_null(), "§111 rule 10 publishes no null");
        assert_eq!(
            ctx.live_registration_count(),
            2,
            "§111 rule 10 keeps the record the quota refused"
        );
        assert!(ctx.release_callback_registration(first.cast()));
        assert!(ctx.release_callback_registration(refused.cast()));
    }

    #[test]
    fn registration_userdata_is_rooted_and_release_removes_the_root() {
        reset_observations();
        for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
            let first = ctx.alloc(16, 1, 40);
            let second = ctx.alloc(16, 2, 41);
            assert!(!first.is_null() && !second.is_null(), "{tier}");
            let registration =
                ctx.register_callback(code_of(counting), std::ptr::null(), first, second, 0);

            ctx.collect();

            assert!(ctx.is_live(first as usize), "{tier}: first userdata");
            assert!(ctx.is_live(second as usize), "{tier}: second userdata");
            assert_eq!(ctx.live_count(), 2, "{tier}: rooted accounting");

            assert!(ctx.release_callback_registration(registration.cast()));
            assert!(
                ctx.is_live(first as usize),
                "{tier}: §111 rule 7 frees nothing"
            );

            ctx.collect();
            assert_eq!(ctx.live_count(), 0, "{tier}: the root is gone");
        }
    }

    #[test]
    fn a_freed_registration_userdata_slot_is_skipped_at_mark() {
        reset_observations();
        for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
            assert!(
                ctx.set_freed_handle_diagnostics(true, 0, usize::MAX),
                "{tier}"
            );
            let freed = ctx.alloc(16, 1, 42);
            assert!(!freed.is_null(), "{tier}");
            ctx.register_callback(
                code_of(counting),
                std::ptr::null(),
                freed,
                std::ptr::null_mut(),
                0,
            );
            ctx.delete(freed as usize, 43);

            ctx.collect();

            assert!(!ctx.trapped(), "{tier}: mark must skip the dead slot");
            assert!(!ctx.is_live(freed as usize), "{tier}: freed slot");
            assert_eq!(ctx.live_count(), 0, "{tier}: live accounting");
        }
    }

    #[test]
    fn the_binding_count_advisory_counts_registrations_with_the_bindings() {
        reset_observations();

        #[derive(Default)]
        struct Advisories(Vec<(u32, u32, Vec<u8>)>);

        unsafe extern "C" fn observe(
            userdata: *mut c_void,
            kind: u32,
            pos_id: u32,
            message: *const u8,
            message_len: u64,
        ) {
            // SAFETY: the test passes a live Advisories value, and the
            // observer contract supplies readable message bytes.
            let observed = unsafe { &mut *userdata.cast::<Advisories>() };
            // SAFETY: the message stays readable for this call.
            let message =
                unsafe { std::slice::from_raw_parts(message, message_len as usize) }.to_vec();
            observed.0.push((kind, pos_id, message));
        }

        let mut ctx = Context::new();
        let mut observed = Advisories::default();
        ctx.set_diagnostics_observer(Some(observe), std::ptr::from_mut(&mut observed).cast());
        ctx.set_binding_count_advisory(2);
        let code = code_of(counting);
        let mut first = 1u8;
        let mut second = 2u8;

        ctx.bind_callback(
            code,
            std::ptr::null(),
            std::ptr::from_mut(&mut first),
            std::ptr::null_mut(),
            0,
        );
        assert!(observed.0.is_empty(), "one binding is below the threshold");

        ctx.register_callback(
            code,
            std::ptr::null(),
            std::ptr::from_mut(&mut second),
            std::ptr::null_mut(),
            0,
        );
        assert_eq!(
            observed.0,
            [(
                DIAGNOSTICS_ADVISORY_BINDING_COUNT,
                0,
                b"callback bindings: 2 registered, advisory threshold 2".to_vec(),
            )],
            "§111 rule 9 counts the live registrations with the bindings"
        );
    }

    #[test]
    fn freeing_registration_userdata_is_advised_only_with_an_observer() {
        reset_observations();

        #[derive(Debug, Default, PartialEq, Eq)]
        struct Advisory {
            calls: u32,
            kind: u32,
            pos_id: u32,
            message: Vec<u8>,
        }

        unsafe extern "C" fn observe(
            userdata: *mut c_void,
            kind: u32,
            pos_id: u32,
            message: *const u8,
            message_len: u64,
        ) {
            // SAFETY: the test passes a live Advisory value, and the
            // observer contract supplies readable message bytes.
            let advisory = unsafe { &mut *userdata.cast::<Advisory>() };
            advisory.calls += 1;
            advisory.kind = kind;
            advisory.pos_id = pos_id;
            // SAFETY: the message stays readable for this call.
            advisory.message =
                unsafe { std::slice::from_raw_parts(message, message_len as usize) }.to_vec();
        }

        // The silent half: no observer, so the free reports nothing.
        let mut quiet = Context::new();
        let unobserved = quiet.alloc(16, 1, 50);
        assert!(!unobserved.is_null());
        quiet.register_callback(
            code_of(counting),
            std::ptr::null(),
            unobserved,
            std::ptr::null_mut(),
            0,
        );
        quiet.delete(unobserved as usize, 51);
        assert!(!quiet.trapped(), "the default free path must not trap");
        assert!(!quiet.is_live(unobserved as usize));

        // The firing control: the same shape with an observer set.
        let mut ctx = Context::new();
        let registered = ctx.alloc(16, 1, 52);
        assert!(!registered.is_null());
        ctx.register_callback(
            code_of(counting),
            std::ptr::null(),
            std::ptr::null_mut(),
            registered,
            0,
        );
        let mut advisory = Advisory::default();
        ctx.set_diagnostics_observer(Some(observe), std::ptr::from_mut(&mut advisory).cast());

        ctx.delete(registered as usize, 53);

        assert_eq!(
            advisory,
            Advisory {
                calls: 1,
                kind: crate::DIAGNOSTICS_ADVISORY_CALLBACK_USERDATA_FREE,
                pos_id: 53,
                message: b"Context.free of registered callback userdata".to_vec(),
            },
            "§111 rule 9 advises a free of live registration userdata"
        );
        assert!(!ctx.trapped(), "an advisory must not become a trap");
    }

    #[test]
    fn ffi_register_release_and_trampoline_drive_one_registration() {
        reset_observations();
        let mut ctx = Context::new();
        // §14.4b (A) checks userdata at fire, so a fired registration
        // carries a live allocation.
        let userdata = ctx.alloc(16, 1, 23);
        assert!(!userdata.is_null());
        let pointer: *mut Context = &mut *ctx;

        // SAFETY: `pointer` is the live exclusive Context of this test,
        // `env` is null as a boundary callback's is, and the message
        // view is empty.
        unsafe {
            let registration = subscript_rt_cb_register(
                pointer,
                code_of(counting),
                std::ptr::null(),
                userdata,
                std::ptr::null_mut(),
                0,
            );
            assert!(!registration.is_null());
            assert_eq!((*pointer).live_registration_count(), 1);

            subscript_rt_cb_registration_trampoline(
                empty_message(),
                registration,
                std::ptr::null_mut(),
            );
            assert_eq!(CALLS.with(Cell::get), 1);

            // §111 rule 5: the C result is `int32_t`, 1 or 0.
            assert_eq!(
                crate::ffi::subscript_rt_ctx_callback_release(pointer, registration.cast()),
                1
            );
            assert_eq!(
                crate::ffi::subscript_rt_ctx_callback_release(pointer, registration.cast()),
                0
            );
            assert_eq!(
                crate::ffi::subscript_rt_ctx_callback_release(pointer, std::ptr::null_mut()),
                0
            );
            assert_eq!((*pointer).live_registration_count(), 0);
        }
    }

    /// §111 rule 5a: the registration answers which Context it belongs
    /// to, so a host with more than one Context names the right one.
    #[test]
    fn a_registration_answers_the_context_that_created_it() {
        reset_observations();
        let mut first = Context::new();
        let mut second = Context::new();
        let first_pointer: *mut Context = &mut *first;
        let second_pointer: *mut Context = &mut *second;
        let code = code_of(counting);
        let mut userdata = 1u8;
        let userdata = std::ptr::from_mut(&mut userdata);

        let one =
            first.register_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);
        let other =
            second.register_callback(code, std::ptr::null(), userdata, std::ptr::null_mut(), 0);

        // SAFETY: both registrations are live and were never released.
        unsafe {
            assert_eq!(
                crate::ffi::subscript_rt_cb_registration_context(one.cast()),
                first_pointer,
                "§111 rule 5a answers the Context that registered it"
            );
            assert_eq!(
                crate::ffi::subscript_rt_cb_registration_context(other.cast()),
                second_pointer,
                "two Contexts give two answers"
            );
            assert_eq!(
                crate::ffi::subscript_rt_cb_registration_context(std::ptr::null_mut()),
                std::ptr::null_mut(),
                "§111 rule 5a answers null for null"
            );

            // The answer is the Context the release call needs.
            assert_eq!(
                crate::ffi::subscript_rt_ctx_callback_release(
                    crate::ffi::subscript_rt_cb_registration_context(one.cast()),
                    one.cast()
                ),
                1
            );
        }
        assert_eq!(first.live_registration_count(), 0);
        assert_eq!(second.live_registration_count(), 1);
        assert!(second.release_callback_registration(other.cast()));
    }

    #[test]
    fn the_registration_trampoline_ignores_a_null_first_slot() {
        reset_observations();
        // SAFETY: a null first slot is the one value the trampoline
        // accepts without a record.
        unsafe {
            subscript_rt_cb_registration_trampoline(
                empty_message(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        assert_eq!(CALLS.with(Cell::get), 0);
    }

    /// §111 rule 14, the certain case: the live set still holds the
    /// closed record, because the outer call keeps it alive.
    #[test]
    fn a_fire_through_a_closed_registration_traps_and_runs_no_script_code() {
        reset_observations();
        let mut ctx = Context::new();
        // §14.4b (A) checks userdata at fire, so a fired registration
        // carries a live allocation.
        let userdata = ctx.alloc(16, 1, 24);
        assert!(!userdata.is_null());
        let registration = ctx.register_callback(
            code_of(release_then_fire_again),
            std::ptr::null(),
            userdata,
            std::ptr::null_mut(),
            0,
        );
        SUBJECT.with(|cell| cell.set(registration));
        NESTING_LEFT.with(|cell| cell.set(1));

        fire(registration);

        assert_eq!(
            CALLS.with(Cell::get),
            1,
            "§111 rule 14 enters no script code on the second fire"
        );
        assert_eq!(
            OBSERVED_FIRST_RELEASE.with(Cell::get),
            1,
            "§111 rule 5 ends the open record inside the call"
        );
        assert_eq!(
            OBSERVED_LIVE_REGISTRATIONS.with(Cell::get),
            1,
            "the active call keeps the closed record"
        );
        assert_eq!(
            OBSERVED_ACTIVE_CALLS.with(Cell::get),
            1,
            "§111 rule 14 changes no count"
        );
        assert_eq!(
            ctx.trap_record().map(|record| record.kind),
            Some(TrapKind::CallbackRegistrationEnded)
        );
        assert_eq!(
            ctx.trap_record().map(|record| record.pos_id),
            Some(0),
            "§111 rule 14 records position 0"
        );
        assert_eq!(
            ctx.trap_record().map(|record| record.message.clone()),
            Some("a callback fired through a registration the host released".to_string())
        );
        assert_eq!(
            ctx.live_registration_count(),
            0,
            "the outer call still counts down and ends the record"
        );
    }

    /// §111 rule 14, the second branch: the pointer is not in the live
    /// set. The stand-in record is storage this test owns and never
    /// registered, so reaching it is defined. A record that really
    /// ended is freed storage, and reading it is undefined; this test
    /// therefore reaches the same branch by the one route that is not.
    #[test]
    fn a_fire_through_a_pointer_outside_the_live_set_traps() {
        reset_observations();
        let mut ctx = Context::new();
        let userdata = ctx.alloc(16, 1, 25);
        assert!(!userdata.is_null());
        let pointer: *mut Context = &mut *ctx;
        let mut stand_in = Box::new(CallbackRegistration {
            ctx: pointer,
            code: code_of(counting),
            env: std::ptr::null(),
            userdata1: userdata,
            userdata2: std::ptr::null_mut(),
            open: true,
            active_calls: 0,
        });
        // The record counter counts registrations; this stand-in is not
        // one, so balance the probe its drop decrements.
        live_records_add(1);
        let stand_in_pointer: *mut CallbackRegistration = &mut *stand_in;

        fire(stand_in_pointer.cast());

        assert_eq!(
            CALLS.with(Cell::get),
            0,
            "§111 rule 14 enters no script code"
        );
        // SAFETY: the test owns the stand-in for the whole test.
        assert_eq!(
            unsafe { (*pointer).trap_record() }.map(|record| record.kind),
            Some(TrapKind::CallbackRegistrationEnded)
        );
        assert_eq!(
            unsafe { (*pointer).live_registration_count() },
            0,
            "the stand-in never enters the live set"
        );
    }

    /// The firing control of the two tests above: the same trampoline
    /// on an open registration of the same Context enters script code
    /// and records no trap.
    #[test]
    fn a_fire_through_an_open_registration_records_no_ended_trap() {
        reset_observations();
        let mut ctx = Context::new();
        let userdata = ctx.alloc(16, 1, 26);
        assert!(!userdata.is_null());
        let registration = ctx.register_callback(
            code_of(counting),
            std::ptr::null(),
            userdata,
            std::ptr::null_mut(),
            0,
        );

        fire(registration);

        assert_eq!(CALLS.with(Cell::get), 1);
        assert!(!ctx.trapped(), "an open registration records no trap");
    }

    /// §111.4 criterion 2 at N = 10,000. The test prints the wall time of
    /// each N-sized step, and does the N-sized work one time for each
    /// fact.
    #[test]
    fn ten_thousand_registrations_return_their_charge_and_their_userdata() {
        const N: usize = 10_000;
        reset_observations();
        let code = code_of(counting);

        let mut ctx = Context::new();
        let empty = ctx.charged_bytes();

        // N distinct userdata objects, each holding a traced reference
        // to a second allocation.
        let start = Instant::now();
        let mut userdata = Vec::with_capacity(N);
        for _ in 0..N {
            let inner = ctx.alloc(8, 1, 60);
            let outer = ctx.alloc(8, 1, 61);
            assert!(!inner.is_null() && !outer.is_null());
            // SAFETY: `outer` has 8 writable payload bytes.
            unsafe { outer.cast::<usize>().write(inner as usize) };
            userdata.push(outer);
        }
        let build = start.elapsed();
        let allocated = ctx.charged_bytes();
        assert_eq!(ctx.live_count(), 2 * N);

        let start = Instant::now();
        let mut records = Vec::with_capacity(N);
        for &object in &userdata {
            records.push(ctx.register_callback(
                code,
                std::ptr::null(),
                object,
                std::ptr::null_mut(),
                0,
            ));
        }
        let register = start.elapsed();
        assert_eq!(ctx.live_registration_count(), N);
        assert_eq!(
            ctx.charged_bytes(),
            allocated + N * REGISTRATION_RECORD_BYTES
        );

        let start = Instant::now();
        for &record in &records {
            assert!(ctx.release_callback_registration(record.cast()));
        }
        let release = start.elapsed();
        assert_eq!(
            ctx.live_registration_count(),
            0,
            "§111.4 criterion 2: no live registration is left"
        );
        assert_eq!(
            ctx.charged_bytes(),
            allocated,
            "§111.4 criterion 2: the charge equals its N = 0 value"
        );

        let start = Instant::now();
        ctx.collect();
        let collect = start.elapsed();
        assert_eq!(
            ctx.charged_bytes(),
            empty,
            "§111.4 criterion 2: one collect reclaims the graph"
        );
        assert_eq!(ctx.live_count(), 0);

        // The firing control: the Context-lifetime path grows by N
        // records and keeps the userdata after one collect.
        let mut control = Context::new();
        let control_empty = control.charged_bytes();
        let start = Instant::now();
        let mut control_userdata = Vec::with_capacity(N);
        for _ in 0..N {
            let inner = control.alloc(8, 1, 62);
            let outer = control.alloc(8, 1, 63);
            assert!(!inner.is_null() && !outer.is_null());
            // SAFETY: `outer` has 8 writable payload bytes.
            unsafe { outer.cast::<usize>().write(inner as usize) };
            control_userdata.push(outer);
        }
        let control_build = start.elapsed();
        let control_allocated = control.charged_bytes();

        let start = Instant::now();
        for &object in &control_userdata {
            control.bind_callback(code, std::ptr::null(), object, std::ptr::null_mut(), 0);
        }
        let control_bind = start.elapsed();
        let record = std::mem::size_of::<CallbackBinding>();
        assert_eq!(
            control.charged_bytes(),
            control_allocated + N * record,
            "the control grows by N binding records"
        );

        let start = Instant::now();
        control.collect();
        let control_collect = start.elapsed();
        assert_eq!(
            control.live_count(),
            2 * N,
            "the control keeps its userdata after collect"
        );
        assert!(control.charged_bytes() > control_empty);

        println!(
            "N={N} build {build:?} register {register:?} release {release:?} collect {collect:?}; \
             control build {control_build:?} bind {control_bind:?} collect {control_collect:?}"
        );
    }
}
