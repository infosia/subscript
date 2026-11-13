//! compiler.md §270 rules 1–4: device destruction ends open registrations.
#![cfg(not(all(windows, target_env = "msvc")))]

extern crate subscript_interop_fixture;

use std::cell::Cell;
use std::ffi::c_void;

use std::ptr::{null, null_mut};
use subscript_runtime::ffi::SubStrView;
use subscript_runtime::registration::subscript_rt_cb_registration_trampoline;
use subscript_runtime::Context;

thread_local! {
    static CALLS: Cell<u8> = const { Cell::new(0) };
    static DEVICE: Cell<*mut c_void> = const { Cell::new(null_mut()) };
}

#[repr(C)]
struct RequestInfo {
    callback: unsafe extern "C" fn(SubStrView, *mut u8, *mut u8),
    userdata: *mut u8,
    userparam: *mut u8,
}

extern "C" {
    fn subDeviceCreate(chain: *mut c_void) -> *mut c_void;
    fn subDeviceRelease(device: *mut c_void);
    fn subRequestStart(device: *mut c_void, payload: u32, immediate: i32, info: RequestInfo)
        -> i32;
    fn subRequestSubscribe(device: *mut c_void, info: RequestInfo) -> i32;
    fn subRequestNotify(device: *mut c_void, payload: u32);
    fn subRequestPump(device: *mut c_void);
    fn subRequestReleaseCount(device: *mut c_void) -> i32;
}

unsafe extern "C" fn completed(
    _ctx: *mut Context,
    _env: *const u8,
    _message: *mut u8,
    _calls: *mut u8,
    _second: *mut u8,
) {
    CALLS.with(|calls| calls.set(calls.get() + 1));
}

fn request(ctx: &mut Context) -> RequestInfo {
    // SAFETY: the callback has the language ABI; its environment is null.
    // The caller keeps the Context alive through device release.
    let userdata =
        unsafe { ctx.register_callback(completed as *const u8, null(), null_mut(), null_mut()) };
    RequestInfo {
        callback: subscript_rt_cb_registration_trampoline,
        userdata,
        userparam: null_mut(),
    }
}

#[test]
fn device_release_ends_pending_request_and_subscription() {
    let mut ctx = Context::new();
    CALLS.with(|calls| calls.set(0));
    // SAFETY: the device and all callback data remain live until release.
    unsafe {
        let device = subDeviceCreate(null_mut());
        assert!(!device.is_null());
        assert_eq!(subRequestStart(device, 3, 0, request(&mut ctx)), 1);
        assert_eq!(subRequestSubscribe(device, request(&mut ctx)), 2);
        assert_eq!(ctx.live_registration_count(), 2);
        assert_eq!(subRequestReleaseCount(device), 0);
        subDeviceRelease(device);
    }
    assert_eq!(ctx.live_registration_count(), 0);
    assert_eq!(CALLS.with(Cell::get), 0);
}

#[test]
fn device_release_does_not_end_a_completed_request_again() {
    let mut ctx = Context::new();
    CALLS.with(|calls| calls.set(0));
    // SAFETY: the device and all callback data remain live until release.
    unsafe {
        let device = subDeviceCreate(null_mut());
        assert!(!device.is_null());
        subRequestStart(device, 3, 0, request(&mut ctx));
        subRequestPump(device);
        assert_eq!(CALLS.with(Cell::get), 1);
        assert_eq!(subRequestReleaseCount(device), 1);
        assert_eq!(ctx.live_registration_count(), 0);
        subDeviceRelease(device);
    }
    assert_eq!(ctx.live_registration_count(), 0);
    assert_eq!(CALLS.with(Cell::get), 1);
}

unsafe extern "C" fn release_device(
    _ctx: *mut Context,
    _env: *const u8,
    _message: *mut u8,
    _first: *mut u8,
    _second: *mut u8,
) {
    CALLS.with(|calls| calls.set(calls.get() + 1));
    // SAFETY: the callback consumes the device reference that the test owns.
    unsafe { subDeviceRelease(DEVICE.with(|device| device.replace(null_mut()))) };
}

#[test]
fn callback_releases_last_device_reference() {
    for mode in 0..3 {
        let mut ctx = Context::new();
        CALLS.with(|calls| calls.set(0));
        // SAFETY: the Context outlives dispatch; the callback consumes the device reference.
        unsafe {
            let device = subDeviceCreate(null_mut());
            assert!(!device.is_null());
            DEVICE.with(|slot| slot.set(device));
            let userdata =
                ctx.register_callback(release_device as *const u8, null(), null_mut(), null_mut());
            let info = RequestInfo {
                callback: subscript_rt_cb_registration_trampoline,
                userdata,
                userparam: null_mut(),
            };
            if mode == 2 {
                subRequestSubscribe(device, info);
                subRequestNotify(device, 3);
                subRequestPump(device);
            } else {
                assert_eq!(subRequestStart(device, 3, mode, info), 1);
                if mode == 0 {
                    subRequestPump(device);
                }
            }
        }
        assert_eq!(CALLS.with(Cell::get), 1);
        assert!(DEVICE.with(Cell::get).is_null());
        assert_eq!(ctx.live_registration_count(), 0);
    }
}
