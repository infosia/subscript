use super::*;
use crate::context::Context;
use crate::trap::TrapKind;

struct ObservedTrap {
    calls: u32,
    kind: u32,
    pos_id: u32,
    message_ptr: *const u8,
    message_len: u64,
    message: Vec<u8>,
}

impl Default for ObservedTrap {
    fn default() -> Self {
        Self {
            calls: 0,
            kind: 0,
            pos_id: 0,
            message_ptr: std::ptr::null(),
            message_len: 0,
            message: Vec::new(),
        }
    }
}

unsafe extern "C" fn observe_trap(
    userdata: *mut std::ffi::c_void,
    kind: u32,
    pos_id: u32,
    message: *const u8,
    message_len: u64,
) {
    // SAFETY: the tests pass a live `ObservedTrap` as userdata and
    // keep it alive until after every callback.
    let observed = unsafe { &mut *userdata.cast::<ObservedTrap>() };
    observed.calls += 1;
    observed.kind = kind;
    observed.pos_id = pos_id;
    observed.message_ptr = message;
    observed.message_len = message_len;
    // SAFETY: the observer contract supplies `message_len` bytes
    // from the stored record for the callback and beyond.
    observed.message =
        unsafe { std::slice::from_raw_parts(message, message_len as usize) }.to_vec();
}

#[derive(Default)]
struct ObservedPrint {
    lines: Vec<Vec<u8>>,
}

unsafe extern "C" fn observe_print(
    userdata: *mut std::ffi::c_void,
    line: *const u8,
    line_len: u64,
) {
    // SAFETY: the test passes a live `ObservedPrint` and the line is
    // readable for the duration of this callback.
    let observed = unsafe { &mut *userdata.cast::<ObservedPrint>() };
    // SAFETY: the print-observer contract supplies `line_len` readable
    // bytes for this callback.
    let line = unsafe { std::slice::from_raw_parts(line, line_len as usize) };
    observed.lines.push(line.to_vec());
}

#[derive(Default)]
struct ObservedAdvisory {
    calls: u32,
    kind: u32,
    pos_id: u32,
    message: Vec<u8>,
}

unsafe extern "C" fn observe_advisory(
    userdata: *mut std::ffi::c_void,
    kind: u32,
    pos_id: u32,
    message: *const u8,
    message_len: u64,
) {
    // SAFETY: the test passes a live ObservedAdvisory as userdata.
    let observed = unsafe { &mut *userdata.cast::<ObservedAdvisory>() };
    observed.calls += 1;
    observed.kind = kind;
    observed.pos_id = pos_id;
    // SAFETY: the observer contract supplies these readable message
    // bytes for the duration of the call.
    observed.message =
        unsafe { std::slice::from_raw_parts(message, message_len as usize) }.to_vec();
}

#[test]
fn ffi_f16_conversion_round_trips_raw_binary16_storage() {
    let bits = subscript_rt_f16_from_f64(1.0006);
    assert_eq!(bits, 0x3c01);
    assert_eq!(subscript_rt_f16_to_f64(bits), 1.0009765625);
    assert_eq!(
        subscript_rt_f16_to_f64(subscript_rt_f16_from_f64(-0.0)).to_bits(),
        (-0.0f64).to_bits()
    );
}

#[test]
fn ffi_fmod_preserves_ieee_remainder_edges() {
    let ctx = std::ptr::null_mut();
    assert_eq!(subscript_rt_fmod(ctx, 5.5, 2.0), 1.5);
    assert_eq!(subscript_rt_fmod(ctx, -5.5, 2.0), -1.5);
    assert_eq!(subscript_rt_fmod(ctx, 5.5, -2.0), 1.5);
    assert!(subscript_rt_fmod(ctx, 5.5, 0.0).is_nan());
    assert!(subscript_rt_fmod(ctx, f64::INFINITY, 2.0).is_nan());
    assert_eq!(subscript_rt_fmod(ctx, 2.0, f64::INFINITY), 2.0);
    assert!(subscript_rt_fmod(ctx, f64::NAN, 2.0).is_nan());
}

#[test]
fn ffi_string_view_copy_in_owns_bytes_and_zero_view_is_empty() {
    let mut ctx = Context::new();
    let ptr: *mut Context = &mut *ctx;
    let mut bytes = *b"field-view";
    // SAFETY: valid Context and readable views for each call.
    unsafe {
        let copied = subscript_rt_str_from_view(ptr, bytes.as_ptr(), bytes.len() as u64, 9);
        bytes.fill(b'x');
        assert_eq!(ctx.str_bytes(copied), b"field-view");

        let empty = subscript_rt_str_from_view(ptr, std::ptr::null(), 0, 10);
        assert_eq!(ctx.str_bytes(empty), b"");
    }
}

#[test]
fn ffi_host_driver_round_trip() {
    let ctx = subscript_rt_ctx_new();
    assert!(!ctx.is_null());
    // SAFETY: `ctx` is the context just created; released once below.
    unsafe {
        let s = subscript_rt_str_lit(ctx, b"hi".as_ptr(), 2, 0);
        subscript_rt_print(ctx, s);
        let mut len: u64 = 0;
        let p = subscript_rt_ctx_stdout(ctx, &mut len);
        assert_eq!(std::slice::from_raw_parts(p, len as usize), b"hi\n");
        assert_eq!(subscript_rt_ctx_trap_kind(ctx), 0);
        let mut mlen: u64 = 1;
        assert!(subscript_rt_ctx_trap_message(ctx, &mut mlen).is_null());
        assert_eq!(mlen, 0);
        subscript_rt_trap(ctx, TrapKind::EmptyPop as u32, 4);
        assert_eq!(subscript_rt_ctx_trap_kind(ctx), TrapKind::EmptyPop as u32);
        assert_eq!(subscript_rt_ctx_trap_pos_id(ctx), 4);
        let m = subscript_rt_ctx_trap_message(ctx, &mut mlen);
        assert!(!m.is_null() && mlen > 0);
        subscript_rt_ctx_release(ctx);
    }
}

#[test]
fn ffi_print_observer_delivers_without_retention_and_null_restores_sink() {
    let ctx = subscript_rt_ctx_new();
    assert!(!ctx.is_null());
    let mut observed = ObservedPrint::default();

    // SAFETY: `ctx`, callback userdata, and string handles remain live
    // through each call and the Context is released exactly once.
    unsafe {
        subscript_rt_ctx_set_print_observer(
            ctx,
            Some(observe_print),
            std::ptr::from_mut(&mut observed).cast(),
        );
        let delivered = subscript_rt_str_lit(ctx, b"delivered".as_ptr(), 9, 0);
        subscript_rt_print(ctx, delivered);

        let mut len = 1u64;
        let _ = subscript_rt_ctx_stdout(ctx, &mut len);
        assert_eq!(len, 0);
        assert_eq!(observed.lines, [b"delivered".to_vec()]);

        subscript_rt_ctx_set_print_observer(ctx, None, std::ptr::null_mut());
        let retained = subscript_rt_str_lit(ctx, b"retained".as_ptr(), 8, 0);
        subscript_rt_print(ctx, retained);
        let bytes = subscript_rt_ctx_stdout(ctx, &mut len);
        assert_eq!(
            std::slice::from_raw_parts(bytes, len as usize),
            b"retained\n"
        );
        assert_eq!(observed.lines, [b"delivered".to_vec()]);

        subscript_rt_ctx_release(ctx);
    }
}

#[test]
fn ffi_diagnostics_observer_advises_on_callback_userdata_free() {
    fn callback_code() {}

    let ctx = subscript_rt_ctx_new();
    assert!(!ctx.is_null());
    let mut observed = ObservedAdvisory::default();

    // SAFETY: `ctx`, observer userdata, and the allocation remain live
    // through their calls; the Context is released exactly once.
    unsafe {
        subscript_rt_ctx_set_diagnostics_observer(
            ctx,
            Some(observe_advisory),
            std::ptr::from_mut(&mut observed).cast(),
        );
        let registered = subscript_rt_alloc(ctx, 16, 1, 20);
        assert!(!registered.is_null());
        subscript_rt_cb_bind(
            ctx,
            callback_code as *const () as *const u8,
            std::ptr::null(),
            registered,
            std::ptr::null_mut(),
        );
        subscript_rt_delete(ctx, registered, 93);

        assert_eq!(observed.calls, 1);
        assert_eq!(
            observed.kind,
            crate::DIAGNOSTICS_ADVISORY_CALLBACK_USERDATA_FREE
        );
        assert_eq!(observed.pos_id, 93);
        assert_eq!(
            observed.message,
            b"Context.free of registered callback userdata"
        );
        assert_eq!(subscript_rt_ctx_trap_kind(ctx), 0);

        subscript_rt_ctx_set_diagnostics_observer(ctx, None, std::ptr::null_mut());
        let after_clear = subscript_rt_alloc(ctx, 16, 1, 21);
        subscript_rt_cb_bind(
            ctx,
            callback_code as *const () as *const u8,
            std::ptr::null(),
            after_clear,
            std::ptr::null_mut(),
        );
        subscript_rt_delete(ctx, after_clear, 94);
        assert_eq!(observed.calls, 1, "null observer must clear delivery");

        subscript_rt_ctx_release(ctx);
    }
}

#[test]
fn ffi_binding_count_advisory_reports_only_new_identity_at_threshold() {
    fn callback_code() {}

    let ctx = subscript_rt_ctx_new();
    assert!(!ctx.is_null());
    let mut observed = ObservedAdvisory::default();
    let mut first_userdata = 1u8;
    let mut second_userdata = 2u8;

    // SAFETY: `ctx`, observer userdata, and both callback userdata
    // addresses remain live through these calls; the Context is released
    // exactly once.
    unsafe {
        subscript_rt_ctx_set_diagnostics_observer(
            ctx,
            Some(observe_advisory),
            std::ptr::from_mut(&mut observed).cast(),
        );
        subscript_rt_ctx_set_binding_count_advisory(ctx, 2);
        let code = callback_code as *const () as *const u8;
        let first = subscript_rt_cb_bind(
            ctx,
            code,
            std::ptr::null(),
            std::ptr::from_mut(&mut first_userdata),
            std::ptr::null_mut(),
        );
        assert_eq!(observed.calls, 0, "below-threshold binding advised");

        let second = subscript_rt_cb_bind(
            ctx,
            code,
            std::ptr::null(),
            std::ptr::from_mut(&mut second_userdata),
            std::ptr::null_mut(),
        );
        assert_ne!(first, second);
        assert_eq!(observed.calls, 1);
        assert_eq!(observed.kind, crate::DIAGNOSTICS_ADVISORY_BINDING_COUNT);
        assert_eq!(observed.pos_id, 0);
        assert_eq!(
            observed.message,
            b"callback bindings: 2 registered, advisory threshold 2"
        );

        let repeated = subscript_rt_cb_bind(
            ctx,
            code,
            std::ptr::null(),
            std::ptr::from_mut(&mut second_userdata),
            std::ptr::null_mut(),
        );
        assert_eq!(second, repeated);
        assert_eq!(observed.calls, 1, "same identity re-registration advised");

        subscript_rt_ctx_release(ctx);
    }
}

#[test]
fn ffi_freed_handle_diagnostics_setting_controls_double_free_detection() {
    let releasing = subscript_rt_ctx_new();
    let diagnosing = subscript_rt_ctx_new();
    assert!(!releasing.is_null() && !diagnosing.is_null());
    // SAFETY: both pointers are fresh, live Contexts and are released
    // exactly once below.
    unsafe {
        assert_eq!(
            subscript_rt_ctx_set_freed_handle_diagnostics(releasing, 0, u64::MAX, 0),
            1
        );
        let released = subscript_rt_alloc(releasing, 8, 1, 0);
        subscript_rt_delete(releasing, released, 1);
        subscript_rt_delete(releasing, released, 2);
        assert_eq!(subscript_rt_ctx_trap_kind(releasing), 0);
        assert_eq!(
            subscript_rt_ctx_set_freed_handle_diagnostics(releasing, 1, 0, u64::MAX),
            0,
            "the setting must reject a late change"
        );

        assert_eq!(
            subscript_rt_ctx_set_freed_handle_diagnostics(diagnosing, 1, 0, u64::MAX),
            1
        );
        let retained = subscript_rt_alloc(diagnosing, 8, 1, 0);
        subscript_rt_delete(diagnosing, retained, 3);
        assert_eq!(subscript_rt_ctx_live_bytes(diagnosing), 0);
        assert!(subscript_rt_ctx_reserved_bytes(diagnosing) > 0);
        subscript_rt_delete(diagnosing, retained, 4);
        assert_eq!(
            subscript_rt_ctx_trap_kind(diagnosing),
            TrapKind::DoubleDelete as u32
        );

        subscript_rt_ctx_release(releasing);
        subscript_rt_ctx_release(diagnosing);
    }
}

#[test]
fn string_for_of_code_points_have_the_p24_allocation_bound_on_both_tiers() {
    unsafe extern "C" fn record_allocation(
        userdata: *mut std::ffi::c_void,
        class_id: u32,
        pos_id: u32,
        payload_bytes: u64,
    ) {
        // SAFETY: each call below supplies a live Vec of this type.
        unsafe { &mut *userdata.cast::<Vec<(u32, u32, u64)>>() }.push((
            class_id,
            pos_id,
            payload_bytes,
        ));
    }

    unsafe fn snapshot(ctx: *const Context) -> Vec<(u32, u32, u64)> {
        let mut allocations = Vec::new();
        // SAFETY: `ctx` is live and the callback userdata points to
        // `allocations` for the duration of this call.
        unsafe {
            subscript_rt_ctx_visit_live_allocations(
                ctx,
                Some(record_allocation),
                (&mut allocations as *mut Vec<(u32, u32, u64)>).cast(),
            );
        }
        allocations
    }

    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let p: *mut Context = &mut *ctx;

        let bmp_bytes = "é".repeat(1_000).into_bytes();
        let bmp_source = ctx.alloc_str(&bmp_bytes, 10);
        // SAFETY: `p` and `bmp_source` are live; `next` is writable.
        let before_bmp = unsafe { snapshot(p) };
        let mut index = 0;
        let mut bmp_handle: *mut u8 = std::ptr::null_mut();
        for _ in 0..1_000 {
            let mut next = -1;
            let handle =
                unsafe { subscript_rt_str_iter_code_point(p, bmp_source, index, &mut next, 4_200) };
            assert_eq!(next, index + 2, "{tier}: BMP byte step");
            if bmp_handle.is_null() {
                bmp_handle = handle;
            } else {
                assert_eq!(handle, bmp_handle, "{tier}: stable BMP handle");
            }
            index = next;
        }
        assert_eq!(index as usize, bmp_bytes.len(), "{tier}");
        // SAFETY: `bmp_handle` is the returned tagged BMP string.
        assert_eq!(
            unsafe { ctx.str_bytes(bmp_handle) },
            "é".as_bytes(),
            "{tier}"
        );
        assert_eq!(
            unsafe { snapshot(p) },
            before_bmp,
            "{tier}: BMP iteration allocated"
        );

        let astral_bytes = "😀".repeat(1_000).into_bytes();
        let astral_source = ctx.alloc_str(&astral_bytes, 11);
        let before_astral = unsafe { snapshot(p) };
        let mut index = 0;
        let mut astral_handle: *mut u8 = std::ptr::null_mut();
        for _ in 0..1_000 {
            let mut next = -1;
            let handle = unsafe {
                subscript_rt_str_iter_code_point(p, astral_source, index, &mut next, 4_201)
            };
            assert_eq!(next, index + 4, "{tier}: astral byte step");
            if astral_handle.is_null() {
                astral_handle = handle;
                assert_eq!(
                    handle as usize & 15,
                    0,
                    "{tier}: astral handle must be an ordinary allocation"
                );
            } else {
                assert_eq!(handle, astral_handle, "{tier}: astral scalar reinterned");
            }
            index = next;
        }
        assert_eq!(index as usize, astral_bytes.len(), "{tier}");
        // SAFETY: `astral_handle` is the live interned string.
        assert_eq!(
            unsafe { ctx.str_bytes(astral_handle) },
            "😀".as_bytes(),
            "{tier}"
        );
        let after_astral = unsafe { snapshot(p) };
        assert_eq!(
            after_astral.len(),
            before_astral.len() + 1,
            "{tier}: repeated astral scalar must allocate once"
        );
        assert_eq!(
            after_astral
                .iter()
                .filter(|&&(class_id, pos_id, _)| {
                    class_id == crate::context::CLASS_STRING && pos_id == 4_201
                })
                .count(),
            1,
            "{tier}: attribution must show one astral allocation"
        );

        // The ordinary allocated representation must remain
        // indistinguishable everywhere a string handle flows.
        let ordinary = ctx.alloc_str("😀".as_bytes(), 13);
        let suffix = ctx.alloc_str(b"!", 14);
        // SAFETY: all handles and the Context are live.
        unsafe {
            assert_eq!(subscript_rt_str_len(p, astral_handle), 4, "{tier}");
            assert_eq!(subscript_rt_str_eq(p, astral_handle, ordinary), 1, "{tier}");
            let joined = subscript_rt_str_concat(p, astral_handle, suffix, 15);
            assert_eq!(ctx.str_bytes(joined), "😀!".as_bytes(), "{tier}");
            subscript_rt_print(p, astral_handle);
        }
        assert_eq!(ctx.stdout_bytes(), "😀\n".as_bytes(), "{tier}");

        // The intern map, not a program root, keeps this ordinary
        // allocation live across both collection implementations.
        ctx.collect();
        assert!(ctx.is_live(astral_handle as usize), "{tier}");
        assert_eq!(
            ctx.code_point('😀', 9_999),
            astral_handle,
            "{tier}: collect discarded the astral intern entry"
        );

        let distinct = "😀🦀𐍈".repeat(334);
        let distinct_source = ctx.alloc_str(distinct.as_bytes(), 12);
        let before_distinct = unsafe { snapshot(p) };
        let mut index = 0;
        for _ in 0..1_002 {
            let mut next = -1;
            let handle = unsafe {
                subscript_rt_str_iter_code_point(p, distinct_source, index, &mut next, 4_202)
            };
            assert!(!handle.is_null(), "{tier}");
            index = next;
        }
        assert_eq!(index as usize, distinct.len(), "{tier}");
        let after_distinct = unsafe { snapshot(p) };
        // 😀 was already interned; 🦀 and 𐍈 are the two new scalars.
        assert_eq!(after_distinct.len(), before_distinct.len() + 2, "{tier}");
        assert_eq!(
            after_distinct
                .iter()
                .filter(|&&(class_id, pos_id, _)| {
                    class_id == crate::context::CLASS_STRING && pos_id == 4_202
                })
                .count(),
            2,
            "{tier}: one allocation per newly distinct scalar"
        );
    }
}

#[test]
fn exported_delete_invalidates_literal_and_astral_string_interns() {
    static LITERAL: &[u8] = b"interned";

    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let p: *mut Context = &mut *ctx;

        // SAFETY: the static literal and exclusive Context satisfy the
        // exported runtime contracts.
        let literal = unsafe { subscript_rt_str_lit(p, LITERAL.as_ptr(), LITERAL.len() as u64, 1) };
        unsafe { subscript_rt_delete(p, literal, 2) };
        assert!(!ctx.is_live(literal as usize), "{tier}: literal delete");
        let literal_again =
            unsafe { subscript_rt_str_lit(p, LITERAL.as_ptr(), LITERAL.len() as u64, 3) };
        assert!(
            ctx.is_live(literal_again as usize),
            "{tier}: stale literal intern entry"
        );

        let astral = ctx.code_point('😀', 4);
        // SAFETY: `astral` is a live ordinary string allocation.
        unsafe { subscript_rt_delete(p, astral, 5) };
        assert!(!ctx.is_live(astral as usize), "{tier}: astral delete");
        let astral_again = ctx.code_point('😀', 6);
        assert!(
            ctx.is_live(astral_again as usize),
            "{tier}: stale astral intern entry"
        );
        assert!(!ctx.trapped(), "{tier}");
    }
}

#[test]
fn ffi_trap_observer_is_first_wins_and_null_clears_on_both_tier_policies() {
    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let p: *mut Context = &mut *ctx;
        let mut observed = ObservedTrap::default();
        // SAFETY: live exclusive Context and live callback userdata.
        unsafe {
            subscript_rt_ctx_set_trap_observer(
                p,
                Some(observe_trap),
                (&mut observed as *mut ObservedTrap).cast(),
            );
        }

        // SAFETY: live exclusive Context; this brackets the modeled
        // host call exactly as an embedding host does.
        unsafe { subscript_rt_ctx_enter_script(p) };
        ctx.trap(TrapKind::EmptyPop, "first fault", 7);
        // This is deliberately a direct second call to the central
        // runtime trap path, modeling a runtime leaf reached during
        // generated-code unwind. It cannot be optimized away by an
        // early return in generated code.
        ctx.trap(TrapKind::DivisionByZero, "later unwind fault", 9);
        // SAFETY: pairs with the enter above.
        unsafe { subscript_rt_ctx_exit_script(p) };

        let record = ctx.trap_record().expect("first trap record");
        assert_eq!(observed.calls, 1, "{tier}");
        assert_eq!(observed.kind, record.kind as u32, "{tier}");
        assert_eq!(observed.pos_id, record.pos_id, "{tier}");
        assert_eq!(observed.message, record.message.as_bytes(), "{tier}");
        assert!(ctx.trapped(), "{tier}: the observer must not recover");

        let mut message_len = 0;
        // SAFETY: shared live Context and writable length.
        let message = unsafe { subscript_rt_ctx_trap_message(p, &mut message_len) };
        assert_eq!(observed.message_ptr, message, "{tier}: record lifetime");
        assert_eq!(observed.message_len, message_len, "{tier}");
        assert_eq!(
            observed.kind,
            // SAFETY: shared live Context.
            unsafe { subscript_rt_ctx_trap_kind(p) },
            "{tier}"
        );
        assert_eq!(
            observed.pos_id,
            // SAFETY: shared live Context.
            unsafe { subscript_rt_ctx_trap_pos_id(p) },
            "{tier}"
        );

        // SAFETY: at a host boundary. A null observer clears it.
        unsafe {
            assert_eq!(subscript_rt_ctx_clear_trap(p), 1, "{tier}");
            subscript_rt_ctx_set_trap_observer(p, None, std::ptr::null_mut());
        }
        ctx.trap(TrapKind::Internal, "not observed", 11);
        assert_eq!(observed.calls, 1, "{tier}: null did not clear observer");
    }
}

#[test]
fn ffi_clear_trap_checks_depth_and_preserves_state_on_both_tier_policies() {
    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let kept = ctx.alloc(8, 1, 0);
        ctx.print_line(b"before");
        ctx.bump_reload_epoch();
        let live_before = ctx.live_count();
        let epoch_before = ctx.reload_epoch();
        let stdout_before = ctx.stdout_bytes().to_vec();
        let p: *mut Context = &mut *ctx;
        // SAFETY: shared accessors over a live Context.
        let accounting_before = unsafe {
            (
                subscript_rt_ctx_live_allocations(p),
                subscript_rt_ctx_live_bytes(p),
                subscript_rt_ctx_reserved_bytes(p),
            )
        };
        assert_eq!(accounting_before.0, live_before as u64, "{tier}");

        // SAFETY: live exclusive Context at a host boundary.
        unsafe { subscript_rt_ctx_enter_script(p) };
        assert_eq!(ctx.script_depth(), 1, "{tier}: enter did not raise depth");
        ctx.trap(TrapKind::EmptyPop, "pop() on an empty array", 3);
        let record_before = ctx.trap_record().cloned();
        // SAFETY: live exclusive Context. The function itself must
        // reject the live-script state without changing anything.
        assert_eq!(unsafe { subscript_rt_ctx_clear_trap(p) }, 0, "{tier}");
        assert!(ctx.trapped(), "{tier}");
        assert_eq!(ctx.trap_record(), record_before.as_ref(), "{tier}");
        assert_eq!(ctx.live_count(), live_before, "{tier}");
        assert!(ctx.is_live(kept as usize), "{tier}");
        assert_eq!(ctx.reload_epoch(), epoch_before, "{tier}");
        assert_eq!(ctx.stdout_bytes(), stdout_before, "{tier}");

        // SAFETY: pairs with the host enter above.
        unsafe { subscript_rt_ctx_exit_script(p) };
        assert_eq!(ctx.script_depth(), 0, "{tier}: exit did not lower depth");
        // SAFETY: the live script frame has returned.
        assert_eq!(unsafe { subscript_rt_ctx_clear_trap(p) }, 1, "{tier}");
        assert!(!ctx.trapped(), "{tier}");
        assert!(ctx.trap_record().is_none(), "{tier}");
        assert_eq!(ctx.live_count(), live_before, "{tier}");
        assert!(ctx.is_live(kept as usize), "{tier}");
        assert_eq!(ctx.reload_epoch(), epoch_before, "{tier}");
        assert_eq!(ctx.stdout_bytes(), stdout_before, "{tier}");
        // SAFETY: shared accessors over a live Context.
        let accounting_after = unsafe {
            (
                subscript_rt_ctx_live_allocations(p),
                subscript_rt_ctx_live_bytes(p),
                subscript_rt_ctx_reserved_bytes(p),
            )
        };
        assert_eq!(
            accounting_after, accounting_before,
            "{tier}: clearing a trap rolled memory state back"
        );
    }
}

#[test]
fn ffi_release_of_null_is_a_no_op() {
    // SAFETY: null is explicitly accepted.
    unsafe { subscript_rt_ctx_release(std::ptr::null_mut()) };
}

#[test]
fn ffi_string_round_trip() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    static LIT: &[u8] = b"alpha-beta";
    // SAFETY: valid context; literal data is 'static.
    unsafe {
        let s = subscript_rt_str_lit(p, LIT.as_ptr(), LIT.len() as u64, 0);
        assert_eq!(subscript_rt_str_len(p, s), 10);
        let tail = subscript_rt_str_slice(p, s, 6, 10, 0);
        let lit_beta = subscript_rt_str_lit(p, b"beta".as_ptr(), 4, 0);
        assert_eq!(subscript_rt_str_eq(p, tail, lit_beta), 1);
        assert_eq!(subscript_rt_str_eq(p, s, lit_beta), 0);
        let empty = subscript_rt_str_slice(p, s, -2, 3, 0);
        assert_eq!(ctx.str_bytes(empty), b"");
        let joined = subscript_rt_str_concat(p, s, lit_beta, 0);
        assert_eq!(ctx.str_bytes(joined), b"alpha-betabeta");
    }
}

#[test]
fn ffi_concat_direct_writer_matches_the_vec_reference_path() {
    for (left, right) in [
        (&b"left"[..], &b"right"[..]),
        (&b""[..], &b"right"[..]),
        ("é".as_bytes(), "中".as_bytes()),
    ] {
        let mut expected = left.to_vec();
        expected.extend_from_slice(right);
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let left_handle = ctx.alloc_str(left, 0);
        let right_handle = ctx.alloc_str(right, 0);
        // SAFETY: the Context and both input strings stay live.
        let result = unsafe { subscript_rt_str_concat(p, left_handle, right_handle, 0) };
        // SAFETY: `result` is a live string in this Context.
        unsafe { assert_eq!(ctx.str_bytes(result), expected) };
    }
}

#[test]
fn ffi_slice_off_utf8_boundary_traps() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context and 'static literal.
    unsafe {
        let s = subscript_rt_str_lit(p, "héllo".as_bytes().as_ptr(), 6, 0);
        let out = subscript_rt_str_slice(p, s, 0, 2, 42);
        assert!(out.is_null());
    }
    let r = ctx.trap_record().expect("trap");
    assert_eq!(r.kind, TrapKind::StringSlice);
    assert_eq!(r.pos_id, 42);
}

#[test]
fn ffi_str_search_predicates() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    static S: &[u8] = b"hello world";
    // SAFETY: valid context; literal data is 'static.
    unsafe {
        let s = subscript_rt_str_lit(p, S.as_ptr(), S.len() as u64, 0);
        let o = subscript_rt_str_lit(p, b"o".as_ptr(), 1, 0);
        let empty = subscript_rt_str_lit(p, b"".as_ptr(), 0, 0);
        let world = subscript_rt_str_lit(p, b"world".as_ptr(), 5, 0);
        assert_eq!(subscript_rt_str_index_of(p, s, o, 0), 4);
        assert_eq!(subscript_rt_str_index_of(p, s, o, 5), 7);
        assert_eq!(subscript_rt_str_index_of(p, s, o, -3), 4);
        assert_eq!(subscript_rt_str_index_of(p, s, o, 99), -1);
        assert_eq!(subscript_rt_str_index_of(p, s, empty, 99), 11);
        assert_eq!(subscript_rt_str_last_index_of(p, s, o, i32::MAX), 7);
        assert_eq!(subscript_rt_str_last_index_of(p, s, empty, i32::MAX), 11);
        assert_eq!(subscript_rt_str_includes(p, s, world, 0), 1);
        assert_eq!(subscript_rt_str_includes(p, s, world, 7), 0);
        assert_eq!(subscript_rt_str_includes(p, s, empty, 0), 1);
        assert_eq!(subscript_rt_str_starts_with(p, s, world, 0), 0);
        assert_eq!(subscript_rt_str_starts_with(p, s, world, 6), 1);
        assert_eq!(subscript_rt_str_ends_with(p, s, world, i32::MAX), 1);
        assert_eq!(subscript_rt_str_ends_with(p, s, world, 6), 0);
        assert_eq!(subscript_rt_str_char_code_at(p, s, 0, 0), 104);
    }
    // The predicates never trap.
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_str_char_code_at_out_of_range_traps() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context; literal data is 'static.
    unsafe {
        let s = subscript_rt_str_lit(p, b"abc".as_ptr(), 3, 0);
        assert_eq!(subscript_rt_str_char_code_at(p, s, 3, 17), 0);
    }
    let r = ctx.trap_record().expect("trap");
    assert_eq!(r.kind, TrapKind::StrRange);
    assert_eq!(r.pos_id, 17);
    assert!(r.message.contains("charCodeAt(3)"));
}

#[test]
fn ffi_q27_string_ranges_code_points_and_concat() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    static TEXT: &[u8] = "héllo".as_bytes();
    // SAFETY: valid context, static literal bytes, and live handles.
    unsafe {
        let s = subscript_rt_str_lit(p, TEXT.as_ptr(), TEXT.len() as u64, 0);
        let mut roots = [s];
        subscript_rt_shadow_push(p, roots.as_mut_ptr().cast(), roots.len() as u64);
        let reversed = subscript_rt_str_substring(p, s, 4, -2, 0);
        assert_eq!(ctx.str_bytes(reversed), "hél".as_bytes());
        let tail = subscript_rt_str_substr(p, s, -3, i32::MAX, 0);
        assert_eq!(ctx.str_bytes(tail), b"llo");
        let empty = subscript_rt_str_substr(p, s, 3, 0, 0);
        assert_eq!(ctx.str_bytes(empty), b"");
        let multibyte = subscript_rt_str_char_at(p, s, 1, 0);
        assert_eq!(ctx.str_bytes(multibyte), "é".as_bytes());
        let out_of_range = subscript_rt_str_char_at(p, s, 99, 0);
        assert_eq!(ctx.str_bytes(out_of_range), b"");
        assert_eq!(subscript_rt_str_code_point_at(p, s, 1, 0), 'é' as i32);
        let suffix = subscript_rt_str_lit(p, b"!".as_ptr(), 1, 0);
        let joined = subscript_rt_str_concat(p, s, suffix, 0);
        assert_eq!(ctx.str_bytes(joined), "héllo!".as_bytes());
        subscript_rt_shadow_pop(p);
    }
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_q27_string_code_point_boundary_and_range_traps() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    static TEXT: &[u8] = "é".as_bytes();
    // SAFETY: valid context, static literal bytes, and a live handle.
    unsafe {
        let s = subscript_rt_str_lit(p, TEXT.as_ptr(), TEXT.len() as u64, 0);
        assert!(subscript_rt_str_char_at(p, s, 1, 31).is_null());
    }
    let report = ctx.trap_record().expect("charAt boundary trap");
    assert_eq!(report.kind, TrapKind::StrRange);
    assert_eq!(report.pos_id, 31);

    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context, static literal bytes, and a live handle.
    unsafe {
        let s = subscript_rt_str_lit(p, TEXT.as_ptr(), TEXT.len() as u64, 0);
        assert_eq!(subscript_rt_str_code_point_at(p, s, 2, 32), 0);
    }
    let report = ctx.trap_record().expect("codePointAt range trap");
    assert_eq!(report.kind, TrapKind::StrRange);
    assert_eq!(report.pos_id, 32);
}

#[test]
fn ffi_str_split_builds_a_string_array_of_handles() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    static S: &[u8] = b",a,";
    // SAFETY: valid context; handles are live; elements are 8-byte
    // string handles read back through array_data.
    unsafe {
        let s = subscript_rt_str_lit(p, S.as_ptr(), S.len() as u64, 0);
        let comma = subscript_rt_str_lit(p, b",".as_ptr(), 1, 0);
        let arr = subscript_rt_str_split(p, s, comma, -1, 0);
        assert!(!arr.is_null());
        assert_eq!(subscript_rt_array_len(p, arr), 3);
        let data = subscript_rt_array_data(p, arr) as *const u64;
        let expected: [&[u8]; 3] = [b"", b"a", b""];
        for (i, want) in expected.iter().enumerate() {
            let h = data.add(i).read() as *const u8;
            assert_eq!(ctx.str_bytes(h), *want, "piece {i}");
        }
    }
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_str_split_empty_separator_uses_code_points() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context; literal data is 'static.
    unsafe {
        let s = subscript_rt_str_lit(p, b"ab".as_ptr(), 2, 0);
        let empty = subscript_rt_str_lit(p, b"".as_ptr(), 0, 0);
        let arr = subscript_rt_str_split(p, s, empty, -1, 23);
        assert!(!arr.is_null());
        assert_eq!(subscript_rt_array_len(p, arr), 2);
        let data = subscript_rt_array_data(p, arr) as *const u64;
        assert_eq!(ctx.str_bytes(data.read() as *const u8), b"a");
        assert_eq!(ctx.str_bytes(data.add(1).read() as *const u8), b"b");
    }
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_str_trim_family_and_case_allocate_fresh_strings() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    static S: &[u8] = "\u{3000}\u{FEFF}x\u{00A0}".as_bytes();
    // SAFETY: valid context; handles are live.
    unsafe {
        let s = subscript_rt_str_lit(p, S.as_ptr(), S.len() as u64, 0);
        let t = subscript_rt_str_trim(p, s, 0);
        assert_eq!(ctx.str_bytes(t), b"x");
        let ts = subscript_rt_str_trim_start(p, s, 0);
        assert_eq!(ctx.str_bytes(ts), "x\u{00A0}".as_bytes());
        let te = subscript_rt_str_trim_end(p, s, 0);
        assert_eq!(ctx.str_bytes(te), "\u{3000}\u{FEFF}x".as_bytes());
        let mixed_bytes = "ß ﬄ ΣΣς İ ı".as_bytes();
        let mixed = subscript_rt_str_lit(p, mixed_bytes.as_ptr(), mixed_bytes.len() as u64, 0);
        let up = subscript_rt_str_to_upper(p, mixed, 0);
        assert_eq!(ctx.str_bytes(up), "SS FFL ΣΣΣ İ I".as_bytes());
        let low = subscript_rt_str_to_lower(p, up, 0);
        assert_eq!(ctx.str_bytes(low), "ss ffl σσς i\u{0307} i".as_bytes());
        let dotted_i_bytes = "İ".as_bytes();
        let dotted_i =
            subscript_rt_str_lit(p, dotted_i_bytes.as_ptr(), dotted_i_bytes.len() as u64, 0);
        let dotted_i_low = subscript_rt_str_to_lower(p, dotted_i, 0);
        assert_eq!(ctx.str_bytes(dotted_i_low), "i\u{0307}".as_bytes());
        assert_eq!(subscript_rt_str_len(p, dotted_i_low), 3);
        // Fresh allocations, not the receiver handle.
        assert_ne!(te, s as *mut u8);
    }
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_str_repeat_and_negative_count_trap() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context; handles are live.
    unsafe {
        let s = subscript_rt_str_lit(p, b"ab".as_ptr(), 2, 0);
        let three = subscript_rt_str_repeat(p, s, 3, 0);
        assert_eq!(ctx.str_bytes(three), b"ababab");
        let zero = subscript_rt_str_repeat(p, s, 0, 0);
        assert_eq!(ctx.str_bytes(zero), b"");
        assert!(ctx.trap_record().is_none());
        assert!(subscript_rt_str_repeat(p, s, -1, 31).is_null());
    }
    let r = ctx.trap_record().expect("trap");
    assert_eq!(r.kind, TrapKind::StrRange);
    assert_eq!(r.pos_id, 31);
}

#[test]
fn ffi_str_pad_truncation_and_empty_pad_copy() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context; handles are live.
    unsafe {
        let s = subscript_rt_str_lit(p, b"ab".as_ptr(), 2, 0);
        let xy = subscript_rt_str_lit(p, b"xy".as_ptr(), 2, 0);
        // The pinned JS truncation rule: "ab" to 5 with "xy".
        let start = subscript_rt_str_pad_start(p, s, 5, xy, 0);
        assert_eq!(ctx.str_bytes(start), b"xyxab");
        let end = subscript_rt_str_pad_end(p, s, 5, xy, 0);
        assert_eq!(ctx.str_bytes(end), b"abxyx");
        // Already long enough: unchanged bytes, fresh allocation.
        let same = subscript_rt_str_pad_start(p, s, 2, xy, 0);
        assert_eq!(ctx.str_bytes(same), b"ab");
        assert_ne!(same, s as *mut u8);
        let empty = subscript_rt_str_lit(p, b"".as_ptr(), 0, 0);
        for target in [-1, 1, 2, 4] {
            for result in [
                subscript_rt_str_pad_start(p, s, target, empty, 0),
                subscript_rt_str_pad_end(p, s, target, empty, 0),
            ] {
                assert!(!result.is_null());
                assert_eq!(ctx.str_bytes(result), b"ab");
                assert_ne!(result, s as *mut u8);
            }
        }
    }
    assert!(ctx.trap_record().is_none());
}

fn assert_direct_pad_matches_vec_reference(at_start: bool) {
    let cases: &[(&[u8], i32, &[u8])] = &[
        (b"ab", 9, b"xyz"),
        (b"", 5, b"ab"),
        (b"receiver", 3, b"xy"),
        (b"z", 7, "é".as_bytes()),
    ];
    for &(receiver, target, pad) in cases {
        let expected = crate::strops::pad(receiver, target, pad, at_start).expect("boundary cut");
        let mut ctx = Context::new();
        let p: *mut Context = &mut *ctx;
        let receiver_handle = ctx.alloc_str(receiver, 0);
        let pad_handle = ctx.alloc_str(pad, 0);
        // SAFETY: the Context and both input strings stay live.
        let result = unsafe {
            if at_start {
                subscript_rt_str_pad_start(p, receiver_handle, target, pad_handle, 0)
            } else {
                subscript_rt_str_pad_end(p, receiver_handle, target, pad_handle, 0)
            }
        };
        // SAFETY: `result` is a live string in this Context.
        unsafe { assert_eq!(ctx.str_bytes(result), expected) };
    }
}

#[test]
fn ffi_pad_cut_inside_a_sequence_traps_before_the_result_exists() {
    // Each trap case has a control of the same shape: the same receiver
    // and pad, with a target whose cut is on a boundary.
    // (target, pad, pad byte of the cut, padStart text, padEnd text)
    type PadCase<'a> = (i32, &'a [u8], Option<usize>, &'a str, &'a str);
    let a = "あ".as_bytes();
    let cases: &[PadCase] = &[
        (2, a, Some(1), "あA", "Aあ"),
        (4, a, None, "あA", "Aあ"),
        (3, a, Some(2), "", ""),
        (6, "あx".as_bytes(), Some(1), "", ""),
        (8, "あx".as_bytes(), None, "あxあA", "Aあxあ"),
        (4, "𠮷".as_bytes(), Some(3), "", ""),
        (5, "𠮷".as_bytes(), None, "𠮷A", "A𠮷"),
    ];
    for at_start in [true, false] {
        let method = if at_start { "padStart" } else { "padEnd" };
        for &(target, pad, cut, start_text, end_text) in cases {
            let mut ctx = Context::new();
            let p: *mut Context = &mut *ctx;
            let receiver_handle = ctx.alloc_str(b"A", 0);
            let pad_handle = ctx.alloc_str(pad, 0);
            let live = ctx.live_count();
            // SAFETY: the Context and both input strings stay live.
            let result = unsafe {
                if at_start {
                    subscript_rt_str_pad_start(p, receiver_handle, target, pad_handle, 7)
                } else {
                    subscript_rt_str_pad_end(p, receiver_handle, target, pad_handle, 7)
                }
            };
            match cut {
                Some(cut) => {
                    assert!(result.is_null(), "{method}({target})");
                    assert_eq!(ctx.live_count(), live, "{method}({target}) allocated");
                    let record = ctx.trap_record().expect("trap");
                    assert_eq!(record.kind, TrapKind::StringSlice);
                    assert_eq!(record.pos_id, 7);
                    assert_eq!(
                        record.message,
                        format!(
                            "{method}({target}): the cut is at pad byte {cut}, inside a UTF-8 sequence"
                        )
                    );
                }
                None => {
                    assert!(ctx.trap_record().is_none(), "{method}({target})");
                    let text = if at_start { start_text } else { end_text };
                    // SAFETY: `result` is a live string in this Context.
                    unsafe { assert_eq!(ctx.str_bytes(result), text.as_bytes()) };
                }
            }
        }
    }
}

#[test]
fn ffi_pad_start_direct_writer_matches_the_vec_reference_path() {
    assert_direct_pad_matches_vec_reference(true);
}

#[test]
fn ffi_pad_end_direct_writer_matches_the_vec_reference_path() {
    assert_direct_pad_matches_vec_reference(false);
}

#[test]
fn ffi_str_replace_first_all_and_empty_pattern() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context; handles are live.
    unsafe {
        let s = subscript_rt_str_lit(p, b"abcabc".as_ptr(), 6, 0);
        let bc = subscript_rt_str_lit(p, b"bc".as_ptr(), 2, 0);
        let x = subscript_rt_str_lit(p, b"X".as_ptr(), 1, 0);
        let first = subscript_rt_str_replace(p, s, bc, x, 0);
        assert_eq!(ctx.str_bytes(first), b"aXabc");
        let all = subscript_rt_str_replace_all(p, s, bc, x, 0);
        assert_eq!(ctx.str_bytes(all), b"aXaX");
        assert!(ctx.trap_record().is_none());
        let empty = subscript_rt_str_lit(p, b"".as_ptr(), 0, 0);
        // replace accepts an empty pattern (match at 0)...
        let prefixed = subscript_rt_str_replace(p, s, empty, x, 0);
        assert_eq!(ctx.str_bytes(prefixed), b"Xabcabc");
        assert!(ctx.trap_record().is_none());
        let inserted = subscript_rt_str_replace_all(p, s, empty, x, 41);
        assert_eq!(ctx.str_bytes(inserted), b"XaXbXcXaXbXcX");
    }
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_print_and_fmt() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context.
    unsafe {
        let s = subscript_rt_fmt_f64(p, 3.75, 0);
        subscript_rt_print(p, s);
        let t = subscript_rt_fmt_bool(p, 1, 0);
        subscript_rt_print(p, t);
    }
    assert_eq!(ctx.take_stdout(), b"3.75\ntrue\n");
}

#[test]
fn ffi_fmt_i32_direct_writer_matches_the_string_reference_path() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    for value in [i32::MIN, -1, 0, i32::MAX] {
        let expected = value.to_string();
        // SAFETY: the Context stays live.
        let result = unsafe { subscript_rt_fmt_i32(p, value, 0) };
        // SAFETY: `result` is a live string in this Context.
        unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
    }
}

#[test]
fn ffi_fmt_u32_direct_writer_matches_the_string_reference_path() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    for value in [0, 1, u32::MAX] {
        let expected = value.to_string();
        // SAFETY: the Context stays live.
        let result = unsafe { subscript_rt_fmt_u32(p, value, 0) };
        // SAFETY: `result` is a live string in this Context.
        unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
    }
}

#[test]
fn ffi_fmt_i64_direct_writer_matches_the_string_reference_path() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    for value in [i64::MIN, -1, 0, i64::MAX] {
        let expected = value.to_string();
        // SAFETY: the Context stays live.
        let result = unsafe { subscript_rt_fmt_i64(p, value, 0) };
        // SAFETY: `result` is a live string in this Context.
        unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
    }
}

#[test]
fn ffi_fmt_u64_direct_writer_matches_the_string_reference_path() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    for value in [0, 1, u64::MAX] {
        let expected = value.to_string();
        // SAFETY: the Context stays live.
        let result = unsafe { subscript_rt_fmt_u64(p, value, 0) };
        // SAFETY: `result` is a live string in this Context.
        unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
    }
}

#[test]
fn ffi_fmt_f32_direct_writer_matches_the_string_reference_path() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    for value in [-0.0, 0.1, f32::INFINITY] {
        let expected = crate::fmt::fmt_f32(value);
        // SAFETY: the Context stays live.
        let result = unsafe { subscript_rt_fmt_f32(p, value, 0) };
        // SAFETY: `result` is a live string in this Context.
        unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
    }
}

#[test]
fn ffi_fmt_f64_direct_writer_matches_the_string_reference_path() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    for value in [-0.0, 0.1, f64::INFINITY] {
        let expected = crate::fmt::fmt_f64(value);
        // SAFETY: the Context stays live.
        let result = unsafe { subscript_rt_fmt_f64(p, value, 0) };
        // SAFETY: `result` is a live string in this Context.
        unsafe { assert_eq!(ctx.str_bytes(result), expected.as_bytes()) };
    }
}

#[test]
fn ffi_binary32_bit_access_forwards_both_wrappers() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    assert_eq!(subscript_rt_math_f32_to_bits(p, -0.0), 0x8000_0000);
    assert_eq!(subscript_rt_math_f32_from_bits(p, 1), 2.0_f64.powi(-149));
}

#[test]
fn ffi_number_entries_forward_and_trap_ranges() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context; all string handles remain live.
    unsafe {
        assert_eq!(subscript_rt_num_is_nan(p, f64::NAN), 1);
        assert_eq!(subscript_rt_num_is_finite(p, f64::INFINITY), 0);
        assert_eq!(subscript_rt_num_is_integer(p, 7.0), 1);
        assert_eq!(
            subscript_rt_num_is_safe_integer(p, 9_007_199_254_740_992.0),
            0
        );
        let int_s = subscript_rt_str_lit(p, b"fftail".as_ptr(), 6, 0);
        assert_eq!(subscript_rt_num_parse_int(p, int_s, 16, 19), 255.0);
        let float_s = subscript_rt_str_lit(p, b"1.5tail".as_ptr(), 7, 0);
        assert_eq!(subscript_rt_num_parse_float(p, float_s, 20), 1.5);
        let fixed = subscript_rt_num_to_fixed(p, 1.005, 2, 20);
        assert_eq!(ctx.str_bytes(fixed), b"1.00");
        let radix_f32 = subscript_rt_num_to_string_f32(p, 10.5, 2, 20);
        assert_eq!(ctx.str_bytes(radix_f32), b"1010.1");
        let radix = subscript_rt_num_to_string_f64(p, 1234.5678, 36, 20);
        assert_eq!(ctx.str_bytes(radix), b"ya.kfv9yqdpm");
        let exponential = subscript_rt_num_to_exponential(p, 0.0, 2, 20);
        assert_eq!(ctx.str_bytes(exponential), b"0.00e+0");
        let precision = subscript_rt_num_to_precision(p, 123.456, 2, 20);
        assert_eq!(ctx.str_bytes(precision), b"1.2e+2");
        assert_eq!(subscript_rt_math_clz32(p, 0), 32);
        assert_eq!(subscript_rt_math_imul(p, i32::MAX, 2), -2);
        assert_eq!(subscript_rt_math_fround(p, 1.1), 1.100_000_023_841_858);
        assert!(subscript_rt_num_to_fixed(p, 1.0, 101, 21).is_null());
    }
    let report = ctx.trap_record().expect("toFixed range trap");
    assert_eq!(report.kind, TrapKind::NumberRange);
    assert_eq!(report.pos_id, 21);

    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context and live literal string handle.
    unsafe {
        let s = subscript_rt_str_lit(p, b"10".as_ptr(), 2, 0);
        assert!(subscript_rt_num_parse_int(p, s, 1, 22).is_nan());
    }
    let report = ctx.trap_record().expect("parseInt radix trap");
    assert_eq!(report.kind, TrapKind::NumberRange);
    assert_eq!(report.pos_id, 22);
}

#[test]
fn ffi_array_and_trap_reporting() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context; element pointers are valid.
    unsafe {
        let a = subscript_rt_array_new(p, 4, 0);
        let v: i32 = 11;
        assert_eq!(
            subscript_rt_array_push(p, a, &v as *const i32 as *const u8, 0),
            1
        );
        assert_eq!(subscript_rt_array_len(p, a), 1);
        assert!(subscript_rt_array_ptr(p, a, 3, 9).is_null());
    }
    assert_eq!(
        ctx.trap_record().map(|r| (r.kind, r.pos_id)),
        Some((TrapKind::IndexOutOfBounds, 9))
    );
}

#[test]
fn ffi_array_with_capacity_reserves_empty_storage() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: the context and all element pointers are valid.
    unsafe {
        let array = subscript_rt_array_with_capacity(p, 3, 4, 0);
        assert!(!array.is_null());
        assert_eq!(subscript_rt_array_len(p, array), 0);
        let values = [10i32, 20, 30];
        assert_eq!(
            subscript_rt_array_push(p, array, (&raw const values[0]).cast(), 0),
            1
        );
        let data = subscript_rt_array_ptr(p, array, 0, 0);
        for (index, value) in values.iter().enumerate().skip(1) {
            assert_eq!(
                subscript_rt_array_push(p, array, (&raw const *value).cast(), 0),
                index as i32 + 1
            );
        }
        assert_eq!(subscript_rt_array_ptr(p, array, 0, 0), data);
    }
}

#[test]
fn ffi_byte_array_span_and_range_report_exact_storage() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    let source = [1u8, 2, 3, 4];
    // SAFETY: the context and source span are valid.
    let array =
        unsafe { subscript_rt_array_from_bytes(p, source.as_ptr(), source.len() as u32, 5) };
    assert!(!array.is_null());
    // SAFETY: the array is live and both requested ranges use its byte storage.
    unsafe {
        assert_eq!(subscript_rt_array_len(p, array), 4);
        let range = subscript_rt_array_byte_range(p, array, 1, 2, 6);
        assert_eq!(std::slice::from_raw_parts(range, 2), &[2, 3]);
        assert!(subscript_rt_array_byte_range(p, array, 3, 2, 7).is_null());
    }
    let report = ctx.trap_record().expect("byte range trap");
    assert_eq!(report.kind, TrapKind::IndexOutOfBounds);
    assert_eq!(
        report.message,
        "byte range at offset 3 with size 2 exceeds array length 4"
    );
    assert_eq!(report.pos_id, 7);

    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: the null inputs exercise the defensive FFI checks.
    unsafe {
        assert!(subscript_rt_array_from_bytes(p, std::ptr::null(), 1, 8).is_null());
        assert!(subscript_rt_array_byte_range(p, std::ptr::null_mut(), 0, 1, 9).is_null());
    }
    assert!(ctx.trap_record().is_none());

    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
    let p: *mut Context = &mut *ctx;
    // SAFETY: the context and source span are valid.
    let array = unsafe { subscript_rt_array_from_bytes(p, source.as_ptr(), 4, 10) };
    ctx.collect();
    // SAFETY: this call exercises the mode-enabled stale-handle diagnostic.
    assert!(unsafe { subscript_rt_array_byte_range(p, array, 0, 1, 11) }.is_null());
    let report = ctx.trap_record().expect("byte range liveness trap");
    assert_eq!(report.kind, TrapKind::UseAfterDelete);
    assert_eq!(report.pos_id, 11);
}

#[test]
fn ffi_array_push_reports_a_collected_receiver_without_panicking() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid exclusive Context.
    let array = unsafe { subscript_rt_array_new(p, 4, 1) };
    ctx.collect();
    let value = 11i32;
    // SAFETY: this deliberately exercises the mode-enabled stale-handle
    // diagnostic provided by retain-and-poison.
    let result = unsafe { subscript_rt_array_push(p, array, (&value as *const i32).cast(), 77) };
    assert_eq!(result, -1);
    assert_eq!(
        ctx.trap_record().map(|record| (record.kind, record.pos_id)),
        Some((TrapKind::UseAfterDelete, 77))
    );
}

#[test]
fn ffi_emitted_trap_entry_records_kind_and_pos() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context.
    unsafe { subscript_rt_trap(p, TrapKind::UseAfterDelete as u32, 12) };
    let r = ctx.trap_record().expect("trap");
    assert_eq!(r.kind, TrapKind::UseAfterDelete);
    assert_eq!(r.message, "use of a deleted allocation");
    assert_eq!(r.pos_id, 12);
}

#[test]
fn trap_messages_runtime_and_emitted_checks_agree() {
    let mut runtime_ctx = Context::new();
    let array = runtime_ctx.array_new(4, 0);
    // SAFETY: `array` is a live array handle of `runtime_ctx`.
    unsafe { runtime_ctx.array_elem_ptr(array, -1, 1) };
    let runtime_bounds = runtime_ctx
        .trap_record()
        .expect("runtime bounds trap")
        .message
        .clone();

    let mut emitted_ctx = Context::new();
    let emitted_ptr: *mut Context = &mut *emitted_ctx;
    // SAFETY: valid context and materialized bounds.
    unsafe { subscript_rt_trap_index_out_of_bounds(emitted_ptr, -1, 0, 1) };
    assert_eq!(
        emitted_ctx
            .trap_record()
            .expect("emitted bounds trap")
            .message,
        runtime_bounds
    );

    let mut runtime_ctx = Context::new();
    assert!(runtime_ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
    let array = runtime_ctx.array_new(4, 0);
    runtime_ctx.delete(array as usize, 0);
    assert!(!runtime_ctx.require_live_handle(array as usize, 2));
    let runtime_deleted = runtime_ctx
        .trap_record()
        .expect("runtime deleted-allocation trap")
        .message
        .clone();

    let mut emitted_ctx = Context::new();
    let emitted_ptr: *mut Context = &mut *emitted_ctx;
    // SAFETY: valid context and stable trap kind.
    unsafe { subscript_rt_trap(emitted_ptr, TrapKind::UseAfterDelete as u32, 2) };
    assert_eq!(
        emitted_ctx
            .trap_record()
            .expect("emitted deleted-allocation trap")
            .message,
        runtime_deleted
    );
}

/// Reads a Set's keys back in insertion order.
///
/// # Safety
///
/// `set` is a live Set whose key width is `width`.
unsafe fn set_keys(ctx: *mut Context, set: *mut u8, width: usize) -> Vec<[u8; 8]> {
    let mut out = Vec::new();
    // SAFETY: live receiver.
    let bound = unsafe { subscript_rt_assoc_iter_begin(ctx, set, 0) };
    for index in 0..bound {
        let mut scratch = [0u8; 8];
        // SAFETY: scratch covers every accepted key width.
        if unsafe { subscript_rt_assoc_iter_copy(ctx, set, index, 0, scratch.as_mut_ptr(), 0) } != 0
        {
            scratch[width..].fill(0);
            out.push(scratch);
        }
    }
    // SAFETY: matching traversal end.
    unsafe { subscript_rt_assoc_iter_end(ctx, set) };
    out
}

#[test]
fn ffi_set_from_array_collapses_duplicates_in_first_occurrence_order() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    let array = ctx.array_new(4, 1);
    for value in [3i32, 1, 3, 2] {
        // SAFETY: the array is live and the source holds one i32.
        unsafe { subscript_rt_array_push(p, array, (&raw const value).cast(), 2) };
    }
    // SAFETY: live array of i32 keys.
    let set = unsafe { subscript_rt_set_from_array(p, array, 4, 0, 3) };
    assert!(!set.is_null());
    // SAFETY: the construction returned a live Set.
    assert_eq!(unsafe { subscript_rt_assoc_size(p, set) }, 3);
    // SAFETY: live Set with four-byte keys.
    let keys = unsafe { set_keys(p, set, 4) };
    let keys: Vec<i32> = keys
        .iter()
        .map(|bytes| i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect();
    assert_eq!(keys, vec![3, 1, 2]);
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_set_from_fixed_reads_every_buffer_element() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    let mut buffer = Vec::new();
    for value in [5i32, 4, 5, 6] {
        buffer.extend_from_slice(&value.to_le_bytes());
    }
    // SAFETY: the buffer holds four i32 keys.
    let set = unsafe { subscript_rt_set_from_fixed(p, buffer.as_ptr(), 4, 4, 0, 7) };
    assert!(!set.is_null());
    // SAFETY: the construction returned a live Set.
    assert_eq!(unsafe { subscript_rt_assoc_size(p, set) }, 3);
    // SAFETY: live Set with four-byte keys.
    let keys = unsafe { set_keys(p, set, 4) };
    let keys: Vec<i32> = keys
        .iter()
        .map(|bytes| i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect();
    assert_eq!(keys, vec![5, 4, 6]);
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_set_from_assoc_copies_the_source_keys() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: monomorphized i32 key shape.
    let source = unsafe { subscript_rt_set_new(p, 4, 0, 1) };
    for value in [8i32, 9] {
        // SAFETY: live Set receiver and one i32 key.
        unsafe { subscript_rt_set_add(p, source, (&raw const value).cast(), 2) };
    }
    // SAFETY: live Set of i32 keys.
    let copy = unsafe { subscript_rt_set_from_assoc(p, source, 4, 0, 3) };
    assert!(!copy.is_null());
    assert_ne!(copy, source);
    // SAFETY: the construction returned a live Set.
    assert_eq!(unsafe { subscript_rt_assoc_size(p, copy) }, 2);
    // SAFETY: live Set with four-byte keys.
    let keys = unsafe { set_keys(p, copy, 4) };
    let keys: Vec<i32> = keys
        .iter()
        .map(|bytes| i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect();
    assert_eq!(keys, vec![8, 9]);
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_set_from_string_yields_one_code_point_per_element() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    let text = ctx.alloc_str("ba漢a漢".as_bytes(), 1);
    // SAFETY: live string source and the string-handle key width.
    let set = unsafe { subscript_rt_set_from_string(p, text, 8, 3, 2) };
    assert!(!set.is_null());
    // SAFETY: the construction returned a live Set.
    assert_eq!(unsafe { subscript_rt_assoc_size(p, set) }, 3);
    // SAFETY: live Set with eight-byte string-handle keys.
    let keys = unsafe { set_keys(p, set, 8) };
    let text: Vec<String> = keys
        .iter()
        .map(|bytes| {
            let handle = usize::from_le_bytes(*bytes) as *const u8;
            // SAFETY: every key is a live string handle.
            String::from_utf8(unsafe { ctx.str_bytes(handle) }.to_vec())
                .expect("code point is UTF-8")
        })
        .collect();
    assert_eq!(text, vec!["b", "a", "漢"]);
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_map_set_operations_trap_on_deleted_receivers_with_diagnostics() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context and monomorphized i32 shapes.
    unsafe {
        let map = subscript_rt_map_new(p, 4, 4, 0, 1);
        subscript_rt_delete(p, map, 2);
        assert_eq!(subscript_rt_assoc_size(p, map), 0);
    }
    assert_eq!(
        ctx.trap_record().map(|r| (r.kind, r.pos_id)),
        Some((TrapKind::UseAfterDelete, 0))
    );

    ctx.clear_trap();
    // SAFETY: valid context and monomorphized i32 shape. The stale
    // receiver is validated before the key is inspected.
    unsafe {
        let set = subscript_rt_set_new(p, 4, 0, 3);
        subscript_rt_delete(p, set, 4);
        let key = 9i32;
        assert_eq!(
            subscript_rt_set_add(p, set, (&key as *const i32).cast(), 17),
            set
        );
    }
    assert_eq!(
        ctx.trap_record().map(|r| (r.kind, r.pos_id)),
        Some((TrapKind::UseAfterDelete, 17))
    );
}

#[test]
fn ffi_array_family_traps_on_deleted_receivers_with_diagnostics() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
    let p: *mut Context = &mut *ctx;
    let array = ctx.array_new(4, 1);
    let value = 7i32;
    // SAFETY: the array is live and the source has one i32.
    unsafe { subscript_rt_array_push(p, array, (&raw const value).cast(), 2) };
    ctx.delete(array as usize, 3);

    let mut out = 0i32;
    // SAFETY: diagnostics retain the dead allocation for this check.
    unsafe { subscript_rt_array_pop(p, array, (&raw mut out).cast(), 77) };
    let trap = ctx.trap_record().expect("array pop trap");
    assert_eq!((trap.kind, trap.pos_id), (TrapKind::UseAfterDelete, 77));
    assert_eq!(trap.message, TrapKind::UseAfterDelete.message(None));

    ctx.clear_trap();
    // SAFETY: diagnostics retain the dead allocation for this check.
    assert!(unsafe { subscript_rt_array_ptr(p, array, 0, 78) }.is_null());
    let trap = ctx.trap_record().expect("array pointer trap");
    assert_eq!((trap.kind, trap.pos_id), (TrapKind::UseAfterDelete, 78));
    assert_eq!(trap.message, TrapKind::UseAfterDelete.message(None));

    ctx.clear_trap();
    // SAFETY: the array method validates the retained dead receiver first.
    assert_eq!(
        unsafe { subscript_rt_arr_index_of(p, array, (&raw const value).cast(), 0, 0) },
        -1
    );
    let trap = ctx.trap_record().expect("array method trap");
    assert_eq!((trap.kind, trap.pos_id), (TrapKind::UseAfterDelete, 0));
    assert_eq!(trap.message, TrapKind::UseAfterDelete.message(None));

    ctx.clear_trap();
    // SAFETY: the array method validates the retained dead receiver first.
    let _ = unsafe { subscript_rt_arr_slice(p, array, 0, 1, 79) };
    let trap = ctx.trap_record().expect("positioned array method trap");
    assert_eq!((trap.kind, trap.pos_id), (TrapKind::UseAfterDelete, 79));
    assert_eq!(trap.message, TrapKind::UseAfterDelete.message(None));
}

#[test]
fn ffi_unknown_trap_kind_is_reported_as_internal() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context.
    unsafe { subscript_rt_trap(p, 999, 3) };
    let r = ctx.trap_record().expect("trap");
    assert_eq!(r.kind, TrapKind::Internal);
    assert_eq!(r.pos_id, 3);
}

#[test]
fn ffi_math_entries_forward_to_the_math_module() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // One spot value per exported symbol; the semantics themselves
    // are pinned by `crate::math`'s tests.
    assert_eq!(subscript_rt_math_abs(p, -3.5), 3.5);
    assert_eq!(subscript_rt_math_acos(p, 1.0), 0.0);
    assert_eq!(subscript_rt_math_acosh(p, 1.0), 0.0);
    assert_eq!(subscript_rt_math_asin(p, 0.0), 0.0);
    assert_eq!(subscript_rt_math_asinh(p, 0.0), 0.0);
    assert_eq!(subscript_rt_math_atan(p, 0.0), 0.0);
    assert_eq!(subscript_rt_math_atanh(p, 0.0), 0.0);
    assert_eq!(subscript_rt_math_cbrt(p, 27.0), 3.0);
    assert_eq!(subscript_rt_math_ceil(p, 1.2), 2.0);
    assert_eq!(subscript_rt_math_cos(p, 0.0), 1.0);
    assert_eq!(subscript_rt_math_cosh(p, 0.0), 1.0);
    assert_eq!(subscript_rt_math_exp(p, 0.0), 1.0);
    assert_eq!(subscript_rt_math_expm1(p, 0.0), 0.0);
    assert_eq!(subscript_rt_math_floor(p, 1.8), 1.0);
    assert_eq!(subscript_rt_math_log(p, 1.0), 0.0);
    assert_eq!(subscript_rt_math_log1p(p, 0.0), 0.0);
    assert_eq!(subscript_rt_math_log10(p, 1000.0), 3.0);
    assert_eq!(subscript_rt_math_log2(p, 8.0), 3.0);
    assert_eq!(subscript_rt_math_round(p, -2.5), -2.0);
    assert_eq!(subscript_rt_math_sign(p, -7.5), -1.0);
    assert_eq!(subscript_rt_math_sin(p, 0.0), 0.0);
    assert_eq!(subscript_rt_math_sinh(p, 0.0), 0.0);
    assert_eq!(subscript_rt_math_sqrt(p, 9.0), 3.0);
    assert_eq!(subscript_rt_math_tan(p, 0.0), 0.0);
    assert_eq!(subscript_rt_math_tanh(p, 0.0), 0.0);
    assert_eq!(subscript_rt_math_trunc(p, -1.7), -1.0);
    assert_eq!(subscript_rt_math_atan2(p, 0.0, 1.0), 0.0);
    assert_eq!(subscript_rt_math_hypot(p, 3.0, 4.0), 5.0);
    assert_eq!(subscript_rt_math_pow(p, 2.0, 10.0), 1024.0);
    assert_eq!(subscript_rt_math_max(p, 2.5, 7.0), 7.0);
    assert_eq!(subscript_rt_math_min(p, 2.5, 7.0), 2.5);
}

#[test]
fn ffi_date_entries_forward_to_the_date_module() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context.
    unsafe {
        let ms = subscript_rt_date_utc(p, 2020, 5, 15, 12, 34, 56, 789, 0);
        assert_eq!(ms, 1_592_224_496_789);
        assert_eq!(subscript_rt_date_new(p, ms, 0), ms);
        assert_eq!(
            subscript_rt_date_get(p, ms, crate::date::FIELD_FULL_YEAR),
            2020
        );
        assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_MONTH), 5);
        assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_DATE), 15);
        assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_DAY), 1);
        assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_HOURS), 12);
        assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_MINUTES), 34);
        assert_eq!(subscript_rt_date_get(p, ms, crate::date::FIELD_SECONDS), 56);
        assert_eq!(
            subscript_rt_date_get(p, ms, crate::date::FIELD_MILLISECONDS),
            789
        );
        let iso = subscript_rt_date_to_iso(p, ms, 0);
        assert_eq!(ctx.str_bytes(iso), b"2020-06-15T12:34:56.789Z");
        let utc = subscript_rt_date_to_utc_string(p, ms, 0);
        assert_eq!(ctx.str_bytes(utc), b"Mon, 15 Jun 2020 12:34:56 GMT");
    }
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_date_new_out_of_range_traps_with_position() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context.
    unsafe {
        assert_eq!(subscript_rt_date_new(p, 8_640_000_000_000_001, 7), 0);
    }
    let r = ctx.trap_record().expect("trap");
    assert_eq!(r.kind, TrapKind::DateRange);
    assert_eq!(r.pos_id, 7);
}

#[test]
fn ffi_date_utc_out_of_range_traps() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context.
    unsafe {
        assert_eq!(subscript_rt_date_utc(p, 275_760, 8, 14, 0, 0, 0, 0, 9), 0);
    }
    let r = ctx.trap_record().expect("trap");
    assert_eq!(r.kind, TrapKind::DateRange);
    assert_eq!(r.pos_id, 9);
}

#[test]
fn ffi_date_to_iso_out_of_year_range_traps() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context.
    unsafe {
        assert!(subscript_rt_date_to_iso(p, 253_402_300_800_000, 11).is_null());
    }
    let r = ctx.trap_record().expect("trap");
    assert_eq!(r.kind, TrapKind::DateRange);
    assert_eq!(r.pos_id, 11);
    assert!(r.message.contains("0000-9999"), "message: {}", r.message);
}

#[test]
fn ffi_date_get_unknown_field_is_an_internal_trap_not_a_panic() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context.
    unsafe {
        assert_eq!(subscript_rt_date_get(p, 0, 99), 0);
    }
    assert_eq!(ctx.trap_record().map(|r| r.kind), Some(TrapKind::Internal));
}

#[test]
fn ffi_date_now_reads_the_pinned_context_clock() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context.
    unsafe {
        subscript_rt_ctx_set_now(p, 1_592_224_496_789);
        assert_eq!(subscript_rt_date_now(p), 1_592_224_496_789);
        subscript_rt_ctx_set_now(p, -1);
        assert_eq!(subscript_rt_date_now(p), -1);
    }
}

#[test]
fn ffi_regex_budget_setter_updates_context_state() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    // SAFETY: valid context.
    unsafe {
        subscript_rt_ctx_set_regex_budget(p, 7);
    }
    assert_eq!(ctx.regex_budget(), 7);
}

#[test]
fn ffi_random_draws_the_context_stream_and_reseeds() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    let mut reference = crate::math::Rng::new(crate::math::DEFAULT_RANDOM_SEED);
    // SAFETY: valid context.
    unsafe {
        for _ in 0..4 {
            assert_eq!(
                subscript_rt_math_random(p).to_bits(),
                reference.next_f64().to_bits()
            );
        }
        subscript_rt_ctx_seed_random(p, 99);
        let a = subscript_rt_math_random(p);
        subscript_rt_ctx_seed_random(p, 99);
        let b = subscript_rt_math_random(p);
        assert_eq!(a.to_bits(), b.to_bits());
    }
}

unsafe extern "C" fn triple_i32(_ctx: *mut Context, _env: *const u8, v: i32) -> i32 {
    v * 3
}

unsafe extern "C" fn cmp_desc_i32(_ctx: *mut Context, _env: *const u8, a: i32, b: i32) -> i32 {
    b - a
}

#[test]
fn ffi_arr_entries_forward_to_the_arrops_module() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    let a = ctx.array_new(4, 0);
    // SAFETY: valid context; live 4-byte-element array; readable
    // needles; callbacks match the dispatched ABI.
    unsafe {
        for v in [3i32, 1, 2, 1] {
            subscript_rt_array_push(p, a, (&v as *const i32).cast(), 0);
        }
        let one = 1i32;
        assert_eq!(
            subscript_rt_arr_index_of(p, a, (&one as *const i32).cast(), 0, 0),
            1
        );
        assert_eq!(
            subscript_rt_arr_last_index_of(p, a, (&one as *const i32).cast(), 0, i32::MAX),
            3
        );
        assert_eq!(
            subscript_rt_arr_includes(p, a, (&one as *const i32).cast(), 0, 0),
            1
        );
        let sep = ctx.alloc_str(b"-", 0);
        let joined = subscript_rt_arr_join(p, a, sep, 0, 0);
        assert_eq!(ctx.str_bytes(joined), b"3-1-2-1");
        let sl = subscript_rt_arr_slice(p, a, 1, 3, 0);
        assert_eq!(subscript_rt_array_len(p, sl), 2);
        let mapped = subscript_rt_arr_map(
            p,
            a,
            triple_i32 as *const u8,
            std::ptr::null(),
            0,
            0,
            4,
            0,
            0,
        );
        assert_eq!(ctx.array_data(mapped).cast::<i32>().read_unaligned(), 9);
        subscript_rt_arr_sort(p, a, cmp_desc_i32 as *const u8, std::ptr::null(), 0);
        assert_eq!(ctx.array_data(a).cast::<i32>().read_unaligned(), 3);
        let b = subscript_rt_arr_slice(p, a, 0, 1, 0);
        let cat = subscript_rt_arr_concat(p, a, b, 0);
        assert_eq!(subscript_rt_array_len(p, cat), 5);
        let z = 0i32;
        subscript_rt_arr_fill(p, a, (&z as *const i32).cast(), 0, i32::MAX);
        subscript_rt_arr_reverse(p, a);
        assert_eq!(
            subscript_rt_arr_every(p, a, triple_i32 as *const u8, std::ptr::null(), 0, 0,),
            0
        );
    }
    assert!(ctx.trap_record().is_none());
}

#[test]
fn ffi_arr_unknown_kind_tag_traps_internal() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    let a = ctx.array_new(4, 0);
    let x = 1i32;
    // SAFETY: valid context; live array; readable needle.
    unsafe {
        assert_eq!(
            subscript_rt_arr_index_of(p, a, (&x as *const i32).cast(), 99, 0),
            -1
        );
    }
    assert_eq!(ctx.trap_record().map(|r| r.kind), Some(TrapKind::Internal));
}

/// §18.2d: the host's collect entry reclaims an unreachable
/// allocation. The control below runs the same shape with no call.
#[test]
fn ffi_ctx_collect_reclaims_unreachable_bytes() {
    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let dead = ctx.alloc(4096, 1, 0);
        assert!(!dead.is_null(), "{tier}: the allocation failed");
        let p: *mut Context = &mut *ctx;
        // SAFETY: shared Context contract at a host boundary.
        let before = unsafe { subscript_rt_ctx_live_bytes(p) };
        // SAFETY: exclusive Context at a host boundary; depth is 0.
        unsafe { subscript_rt_ctx_collect(p) };
        // SAFETY: shared Context contract at a host boundary.
        let after = unsafe { subscript_rt_ctx_live_bytes(p) };
        assert!(after < before, "{tier}: live_bytes {before} -> {after}");
        assert!(
            !ctx.is_live(dead as usize),
            "{tier}: the allocation survived"
        );
    }
}

/// The firing control for the test above. Nothing collects unbidden
/// (invariant 2), so the same shape without the call keeps the bytes.
#[test]
fn ffi_ctx_collect_control_without_the_call_keeps_the_bytes() {
    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let dead = ctx.alloc(4096, 1, 0);
        assert!(!dead.is_null(), "{tier}: the allocation failed");
        let p: *mut Context = &mut *ctx;
        // SAFETY: shared Context contract at a host boundary.
        let before = unsafe { subscript_rt_ctx_live_bytes(p) };
        // No collect call here. That absence is the control.
        // SAFETY: shared Context contract at a host boundary.
        let after = unsafe { subscript_rt_ctx_live_bytes(p) };
        assert_eq!(after, before, "{tier}: live_bytes fell with no call");
        assert!(
            ctx.is_live(dead as usize),
            "{tier}: the allocation went away"
        );
    }
}

#[test]
fn ffi_root_ranges_cover_aggregate_globals() {
    let mut ctx = Context::new();
    let p: *mut Context = &mut *ctx;
    let a = ctx.alloc(8, 1, 0);
    let b = ctx.alloc(8, 1, 0);
    let range = [a as usize, b as usize];
    // SAFETY: valid context; the range outlives the collect call.
    unsafe {
        subscript_rt_root_add(p, range.as_ptr() as *mut u8, 2);
        subscript_rt_collect(p);
    }
    assert!(ctx.is_live(a as usize));
    assert!(ctx.is_live(b as usize));
}
