use super::*;

#[test]
fn held_async_count_uses_emitted_header_and_frees_without_collect() {
    assert_eq!(core::mem::offset_of!(TestCountedAsyncFrame, state), 0);
    assert_eq!(core::mem::offset_of!(TestCountedAsyncFrame, count), 4);
    assert_eq!(core::mem::offset_of!(TestCountedAsyncFrame, resume), 8);
    assert_eq!(core::mem::size_of::<TestCountedAsyncFrame>(), 16);

    let mut ctx = Context::new();
    let frame = ctx.alloc(
        core::mem::size_of::<TestCountedAsyncFrame>(),
        CLASS_GENERATOR,
        70,
    );
    assert!(!frame.is_null());
    // SAFETY: the allocation has exactly the emitted prefix layout.
    unsafe {
        frame
            .cast::<TestCountedAsyncFrame>()
            .write(TestCountedAsyncFrame {
                state: 0,
                count: 0,
                resume: counted_test_resume,
            });
        ctx.async_register(frame, 4);
        assert_eq!((*frame.cast::<TestCountedAsyncFrame>()).count, 1);
        assert_eq!(ctx.async_count(frame), 1);

        // Compiler-emitted copy retain.
        ctx.async_retain(frame);
        assert_eq!((*frame.cast::<TestCountedAsyncFrame>()).count, 2);

        // Compiler-emitted inner-scope exit release.
        ctx.async_release(frame, 70);
        assert_eq!((*frame.cast::<TestCountedAsyncFrame>()).count, 1);

        // Await caches/reads completion but does not change ownership.
        let fulfilled = 37i32;
        ctx.async_complete(frame, (&fulfilled as *const i32).cast(), 4);
        let mut observed = 0i32;
        assert!(ctx.async_result(frame, (&mut observed as *mut i32).cast(), 4));
        assert_eq!(observed, fulfilled);
        assert_eq!((*frame.cast::<TestCountedAsyncFrame>()).count, 1);

        // The final lexical decrement frees immediately. No collect call
        // occurs anywhere in this test.
        ctx.async_release(frame, 70);
    }
    assert!(!ctx.is_live(frame as usize));
    assert_eq!(ctx.live_bytes(), 0);
}

#[test]
fn async_step_promotes_parked_frames_in_registration_order() {
    let mut ctx = Context::new();
    let one = spawn_test_frame(&mut ctx, 1, parking_test_resume);
    let two = spawn_test_frame(&mut ctx, 2, parking_test_resume);
    // SAFETY: both frames are registered and live.
    unsafe {
        ctx.async_kick(one, parking_test_resume);
        ctx.async_kick(two, parking_test_resume);
    }
    assert_eq!(ctx.take_stdout(), b"one\ntwo\n");
    // The kick parks each frame; nothing is ready (§94.1 rules 3 and 7).
    assert_eq!((ctx.async_ready_len(), ctx.async_parked_len()), (0, 2));
    assert_eq!(ctx.async_pending(), 2);
    assert_eq!(ctx.async_unfinished(), 2);

    // The first checkpoint promotes the whole parked list, in
    // registration order. Each frame parks again, so rule 9 defers that
    // work to the next checkpoint rather than looping inside this one.
    // SAFETY: both frames are still registered and live.
    assert_eq!(unsafe { ctx.async_step() }, 2);
    assert_eq!(ctx.take_stdout(), b"one\ntwo\n");
    assert_eq!((ctx.async_ready_len(), ctx.async_parked_len()), (0, 2));

    // The second checkpoint completes both, in the same order.
    // SAFETY: both frames are still registered and live.
    assert_eq!(unsafe { ctx.async_step() }, 0);
    assert_eq!(ctx.take_stdout(), b"one\ntwo\n");
    assert_eq!(ctx.async_unfinished(), 0);
    assert_eq!(ctx.live_bytes(), 0);
}

#[test]
fn async_step_on_trapped_context_is_no_op() {
    let mut ctx = Context::new();
    let frame = spawn_test_frame(&mut ctx, 1, parking_test_resume);
    // SAFETY: the frame is registered and live.
    unsafe { ctx.async_kick(frame, parking_test_resume) };
    assert_eq!(ctx.take_stdout(), b"one\n");
    assert_eq!(ctx.async_pending(), 1);
    ctx.trap(TrapKind::Internal, "test trap", 7);
    // The trap contract stops the checkpoint before any resume, and the
    // parked registration is preserved.
    // SAFETY: the frame is still registered and live.
    assert_eq!(unsafe { ctx.async_step() }, 1);
    assert_eq!(ctx.take_stdout(), b"");
    assert_eq!(ctx.async_pending(), 1);
    assert_eq!(ctx.async_parked_len(), 1);
}

#[test]
fn dropping_context_does_not_resume_registered_async_frames() {
    TEARDOWN_RESUMES.store(0, std::sync::atomic::Ordering::Relaxed);
    {
        let mut ctx = Context::new();
        let frame = spawn_test_frame(&mut ctx, 1, teardown_park_resume);
        // SAFETY: the frame is registered and live.
        unsafe { ctx.async_kick(frame, teardown_park_resume) };
        assert_eq!(ctx.async_pending(), 1);
        assert_eq!(ctx.async_unfinished(), 1);
    }
    assert_eq!(
        TEARDOWN_RESUMES.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "teardown must not run a continuation"
    );
}

/// The invalid-protocol report of `compiler.md` §94.1. A scheduled await
/// resume without its cached completion is an internal defect with a
/// fixed message, reported at the suspension's position.
#[test]
fn async_missing_completion_reports_the_internal_protocol_defect() {
    let mut ctx = Context::new();
    ctx.async_missing_completion(31);
    let record = ctx.trap_record().expect("the defect stops the Context");
    assert_eq!(record.kind, TrapKind::Internal);
    assert_eq!(record.message, "async resume without completion");
    assert_eq!(record.pos_id, 31);
    assert!(ctx.trapped());
}

/// `async_unfinished` counts registered invocations without a cached
/// completion, and nothing else (`compiler.md` §94.2).
#[test]
fn async_unfinished_counts_invocations_without_a_completion() {
    let mut ctx = Context::new();
    assert_eq!(ctx.async_unfinished(), 0);
    let first = spawn_test_frame(&mut ctx, 1, parking_test_resume);
    let second = spawn_test_frame(&mut ctx, 2, parking_test_resume);
    assert_eq!(ctx.async_unfinished(), 2);
    // A completion removes one from the count while its holder keeps it.
    // SAFETY: `first` is a registered live frame and the value is empty.
    unsafe { ctx.async_complete(first, std::ptr::null(), 0) };
    assert_eq!(ctx.async_unfinished(), 1);
    assert_eq!(ctx.async_pending(), 0, "a completion is not pending work");
    // Releasing the last owner preserves an unfinished invocation.
    // SAFETY: each frame holds exactly the registration reference.
    unsafe {
        ctx.async_release(first, 0);
        ctx.async_release(second, 0);
    }
    assert_eq!(ctx.async_unfinished(), 1);
    assert!(ctx.is_live(second as usize));
    // SAFETY: completion ends the unowned invocation before the final scheduler release.
    unsafe {
        ctx.async_complete(second, std::ptr::null(), 0);
        ctx.async_release(second, 0);
    }
    assert_eq!(ctx.async_unfinished(), 0);
}

#[test]
fn async_step_keeps_queued_frames_live_during_collection() {
    let mut ctx = Context::new();
    let collector = spawn_test_frame(&mut ctx, 3, collecting_park_resume);
    let other = spawn_test_frame(&mut ctx, 2, parking_test_resume);
    // SAFETY: both frames are registered and live.
    unsafe {
        ctx.async_kick(collector, collecting_park_resume);
        ctx.async_kick(other, parking_test_resume);
    }
    let _ = ctx.take_stdout();
    // The collector runs first and collects while `other` is still an
    // unreached ready job. §94.2 makes every scheduler state a root.
    // SAFETY: both frames are still registered and live.
    assert_eq!(unsafe { ctx.async_step() }, 2);
    assert!(
        ctx.is_live(other as usize),
        "a collect inside the drain must retain the frames still queued"
    );
    assert!(ctx.is_live(collector as usize));
}

// The emitted-C SsArrayHeader (codegen/src/cemit.rs, §10a) mirrors this
// layout; a reorder here is caught by this test.
#[test]
fn array_header_offsets_match_the_abi_contract() {
    assert_eq!(core::mem::offset_of!(ArrayHeader, len), 0);
    assert_eq!(core::mem::offset_of!(ArrayHeader, cap), 8);
    assert_eq!(core::mem::offset_of!(ArrayHeader, elem_size), 16);
    assert_eq!(core::mem::offset_of!(ArrayHeader, data), 24);
    assert_eq!(core::mem::offset_of!(ArrayHeader, holders), 32);
    assert_eq!(core::mem::size_of::<ArrayHeader>(), 40);
}

#[test]
fn random_stream_is_default_seeded_on_both_construction_paths() {
    // Dev (`new`) and ship (`new_releasing`) Contexts draw the same
    // contract stream (stdlib.md §2).
    let mut dev = Context::new();
    let mut ship = Context::new_releasing();
    let reference: Vec<u64> = {
        let mut r = crate::math::Rng::new(crate::math::DEFAULT_RANDOM_SEED);
        (0..8).map(|_| r.next_f64().to_bits()).collect()
    };
    let dev_draws: Vec<u64> = (0..8).map(|_| dev.random_f64().to_bits()).collect();
    let ship_draws: Vec<u64> = (0..8).map(|_| ship.random_f64().to_bits()).collect();
    assert_eq!(dev_draws, reference);
    assert_eq!(ship_draws, reference);
}

#[test]
fn seed_random_restarts_the_stream() {
    let mut ctx = Context::new();
    ctx.seed_random(7);
    let first: Vec<u64> = (0..4).map(|_| ctx.random_f64().to_bits()).collect();
    ctx.seed_random(7);
    let again: Vec<u64> = (0..4).map(|_| ctx.random_f64().to_bits()).collect();
    assert_eq!(first, again);
}

#[test]
fn now_defaults_to_the_system_clock_and_pins_on_set() {
    let mut ctx = Context::new();
    // Unpinned: a valid, non-decreasing time value from the system
    // clock (stdlib.md §3).
    let a = ctx.now_utc_ms();
    let b = ctx.now_utc_ms();
    assert!(
        crate::date::in_range(a),
        "system clock out of TimeClip: {a}"
    );
    assert!(b >= a);
    // Pinned: exactly the set value, stable across reads, negative
    // (pre-1970) values included.
    ctx.set_now(123);
    assert_eq!(ctx.now_utc_ms(), 123);
    assert_eq!(ctx.now_utc_ms(), 123);
    ctx.set_now(-456);
    assert_eq!(ctx.now_utc_ms(), -456);
}

#[test]
fn trap_flag_is_at_offset_zero() {
    let ctx = Context::new();
    let base = &*ctx as *const Context as usize;
    let flag = &ctx.trap_flag as *const u32 as usize;
    assert_eq!(flag - base, Context::trap_flag_offset());
}

#[test]
fn reload_prefix_offsets_match_the_abi_contract() {
    let ctx = Context::new();
    let base = &*ctx as *const Context as usize;
    assert_eq!(
        &ctx.reload_epoch as *const u32 as usize - base,
        Context::reload_epoch_offset()
    );
    assert_eq!(
        &ctx.fn_table as *const *const *const u8 as usize - base,
        Context::fn_table_offset()
    );
    assert_eq!(
        &ctx.globals as *const *mut u8 as usize - base,
        Context::globals_offset()
    );
}

#[test]
fn reload_epoch_and_script_depth_track_swaps_and_entries() {
    let mut ctx = Context::new();
    assert_eq!(ctx.reload_epoch(), 0);
    ctx.bump_reload_epoch();
    assert_eq!(ctx.reload_epoch(), 1);
    assert_eq!(ctx.script_depth(), 0);
    ctx.enter_script();
    ctx.enter_script();
    assert_eq!(ctx.script_depth(), 2);
    ctx.exit_script();
    ctx.exit_script();
    ctx.exit_script();
    assert_eq!(ctx.script_depth(), 0);
}

#[test]
fn observer_active_guard_blocks_clear_and_resets_on_rust_unwind() {
    let mut ctx = Context::new();
    ctx.trap(TrapKind::EmptyPop, "pending", 1);
    ctx.trap_observer_active = true;
    assert!(!ctx.can_clear_trap());
    let p: *mut Context = &mut *ctx;
    // SAFETY: this is a host-boundary probe over a live Context. The
    // test raises only the guard bit, without entering a callback and
    // therefore without creating an aliasing violation.
    assert_eq!(unsafe { crate::ffi::subscript_rt_ctx_clear_trap(p) }, 0);
    assert!(ctx.trapped(), "the refused clear changed trap state");
    ctx.trap_observer_active = false;
    assert!(ctx.can_clear_trap());

    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.with_trap_observer_active(|| panic!("test observer unwind"));
    }));
    assert!(unwind.is_err());
    assert!(
        !ctx.trap_observer_active,
        "the observer-active flag remained stuck after a Rust unwind"
    );
    assert!(ctx.can_clear_trap());
}

#[test]
fn fn_table_and_globals_pointers_round_trip() {
    let mut ctx = Context::new();
    let table: [*const u8; 2] = [std::ptr::null(), std::ptr::null()];
    let mut block = [0u8; 16];
    ctx.set_fn_table(table.as_ptr());
    ctx.set_globals(block.as_mut_ptr());
    assert_eq!(ctx.fn_table, table.as_ptr());
    assert_eq!(ctx.globals, block.as_mut_ptr());
}

#[test]
fn ship_module_globals_are_context_owned_zeroed_and_reused() {
    let mut ctx = Context::new_releasing();
    let first = ctx.init_module_globals(24, 16);
    assert!(!first.is_null());
    assert_eq!(first as usize % 16, 0);
    assert_eq!(ctx.globals, first);
    // SAFETY: the Context owns 24 writable bytes at `first`.
    unsafe { std::ptr::write_bytes(first, 0xA5, 24) };

    let second = ctx.init_module_globals(24, 16);
    assert_eq!(second, first, "same image must reuse its Context block");
    // SAFETY: the reused block still owns 24 readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(second, 24) };
    assert_eq!(bytes, &[0; 24]);
}

#[test]
fn print_observer_controls_delivery_and_sink_retention() {
    unsafe extern "C" fn observe(userdata: *mut c_void, line: *const u8, line_len: u64) {
        // SAFETY: the test passes a live Vec with this exact type and
        // the observer contract keeps the line readable for this call.
        let lines = unsafe { &mut *userdata.cast::<Vec<Vec<u8>>>() };
        // SAFETY: `line` addresses `line_len` readable bytes for this
        // callback.
        let line = unsafe { std::slice::from_raw_parts(line, line_len as usize) };
        lines.push(line.to_vec());
    }

    let mut unset = Context::new();
    unset.print_line(b"default");
    assert_eq!(unset.take_stdout(), b"default\n");

    let mut ctx = Context::new();
    let mut observed = Vec::<Vec<u8>>::new();
    ctx.set_print_observer(Some(observe), std::ptr::from_mut(&mut observed).cast());
    ctx.print_line(b"first");
    ctx.print_line(b"second");
    assert_eq!(observed, [b"first".to_vec(), b"second".to_vec()]);
    assert!(ctx.stdout_bytes().is_empty());

    ctx.set_print_observer(None, std::ptr::from_mut(&mut observed).cast());
    ctx.print_line(b"after-unset");
    assert_eq!(observed, [b"first".to_vec(), b"second".to_vec()]);
    assert_eq!(ctx.stdout_bytes(), b"after-unset\n");
    assert!(ctx.print_observer_userdata.is_null());
}

#[test]
fn callback_bindings_are_interned_by_identity() {
    fn first_code() {}

    let mut ctx = Context::new();
    let code = first_code as *const () as *const u8;
    let mut userdata1 = 1u8;
    let mut userdata2 = 2u8;
    let mut other_userdata2 = 3u8;
    let userdata1 = std::ptr::from_mut(&mut userdata1);
    let userdata2 = std::ptr::from_mut(&mut userdata2);
    let other_userdata2 = std::ptr::from_mut(&mut other_userdata2);

    let first = ctx.bind_callback(code, std::ptr::null(), userdata1, userdata2);
    let repeated = ctx.bind_callback(code, std::ptr::null(), userdata1, userdata2);
    assert_eq!(first, repeated);
    assert_eq!(ctx.callbacks.len(), 1);

    let second = ctx.bind_callback(code, std::ptr::null(), userdata1, other_userdata2);
    assert_ne!(first, second);
    assert_eq!(ctx.callbacks.len(), 2);
}

#[test]
fn binding_count_advisory_reports_distinct_identity_at_threshold() {
    fn callback_code() {}

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
        // SAFETY: the message remains readable for this callback.
        let message = unsafe { std::slice::from_raw_parts(message, message_len as usize) }.to_vec();
        observed.0.push((kind, pos_id, message));
    }

    let mut ctx = Context::new();
    let mut observed = Advisories::default();
    ctx.set_diagnostics_observer(Some(observe), std::ptr::from_mut(&mut observed).cast());
    ctx.set_binding_count_advisory(2);
    let code = callback_code as *const () as *const u8;
    let mut first_userdata = 1u8;
    let mut second_userdata = 2u8;

    ctx.bind_callback(
        code,
        std::ptr::null(),
        std::ptr::from_mut(&mut first_userdata),
        std::ptr::null_mut(),
    );
    assert!(observed.0.is_empty(), "below-threshold binding advised");

    ctx.bind_callback(
        code,
        std::ptr::null(),
        std::ptr::from_mut(&mut second_userdata),
        std::ptr::null_mut(),
    );
    assert_eq!(
        observed.0,
        [(
            DIAGNOSTICS_ADVISORY_BINDING_COUNT,
            0,
            b"callback bindings: 2 registered, advisory threshold 2".to_vec(),
        )]
    );

    let mut zero_ctx = Context::new();
    let mut zero_observed = Advisories::default();
    zero_ctx.set_diagnostics_observer(Some(observe), std::ptr::from_mut(&mut zero_observed).cast());
    zero_ctx.set_binding_count_advisory(0);
    zero_ctx.bind_callback(
        code,
        std::ptr::null(),
        std::ptr::from_mut(&mut first_userdata),
        std::ptr::null_mut(),
    );
    assert_eq!(
        zero_observed.0,
        [(
            DIAGNOSTICS_ADVISORY_BINDING_COUNT,
            0,
            b"callback bindings: 1 registered, advisory threshold 0".to_vec(),
        )],
        "zero must be a literal first-record threshold"
    );
}

#[test]
fn binding_count_advisory_skips_same_identity_reregistration_at_threshold() {
    fn callback_code() {}

    unsafe extern "C" fn observe(
        userdata: *mut c_void,
        _kind: u32,
        _pos_id: u32,
        _message: *const u8,
        _message_len: u64,
    ) {
        // SAFETY: the test passes a live counter.
        unsafe { *userdata.cast::<u32>() += 1 };
    }

    let mut ctx = Context::new();
    let code = callback_code as *const () as *const u8;
    let mut userdata = 1u8;
    let userdata = std::ptr::from_mut(&mut userdata);
    let first = ctx.bind_callback(code, std::ptr::null(), userdata, std::ptr::null_mut());

    let mut calls = 0u32;
    ctx.set_diagnostics_observer(Some(observe), std::ptr::from_mut(&mut calls).cast());
    ctx.set_binding_count_advisory(1);
    let repeated = ctx.bind_callback(code, std::ptr::null(), userdata, std::ptr::null_mut());

    assert_eq!(first, repeated);
    assert_eq!(ctx.callbacks.len(), 1);
    assert_eq!(calls, 0, "an intern hit must never advise");
}

#[test]
fn callback_reregistration_has_zero_record_growth_at_frame_scale() {
    fn callback_code() {}

    let mut ctx = Context::new();
    let code = callback_code as *const () as *const u8;
    let mut userdata1 = 1u8;
    let mut userdata2 = 2u8;
    let userdata1 = std::ptr::from_mut(&mut userdata1);
    let userdata2 = std::ptr::from_mut(&mut userdata2);
    let first = ctx.bind_callback(code, std::ptr::null(), userdata1, userdata2);

    for _ in 1..10_000 {
        assert_eq!(
            ctx.bind_callback(code, std::ptr::null(), userdata1, userdata2),
            first
        );
    }

    assert_eq!(
        ctx.callbacks.len(),
        1,
        "10,000 registrations of one identity must retain one record"
    );
}

#[test]
fn callback_userdata_rooted_survives_collect() {
    fn callback_code() {}

    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let first = ctx.alloc(16, 1, 10);
        let second = ctx.alloc(16, 2, 11);
        assert!(!first.is_null() && !second.is_null(), "{tier}");
        ctx.bind_callback(
            callback_code as *const () as *const u8,
            std::ptr::null(),
            first,
            second,
        );

        ctx.collect();

        assert!(ctx.is_live(first as usize), "{tier}: first userdata");
        assert!(ctx.is_live(second as usize), "{tier}: second userdata");
        assert_eq!(ctx.live_count(), 2, "{tier}: rooted accounting");
    }
}

#[test]
fn callback_userdata_freed_slot_is_skipped_at_mark() {
    fn callback_code() {}

    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
        let freed = ctx.alloc(16, 1, 12);
        let unrooted = ctx.alloc(16, 2, 13);
        assert!(!freed.is_null() && !unrooted.is_null(), "{tier}");
        ctx.bind_callback(
            callback_code as *const () as *const u8,
            std::ptr::null(),
            freed,
            std::ptr::null_mut(),
        );
        ctx.delete(freed as usize, 14);

        ctx.collect();

        assert!(!ctx.trapped(), "{tier}: mark must skip the dead slot");
        assert!(!ctx.is_live(freed as usize), "{tier}: freed slot");
        assert!(
            !ctx.is_live(unrooted as usize),
            "{tier}: unrooted allocation"
        );
        assert_eq!(ctx.live_count(), 0, "{tier}: live accounting");
    }
}

#[test]
fn diagnostics_observer_advises_on_callback_userdata_free() {
    fn callback_code() {}

    #[derive(Debug, Default, PartialEq, Eq)]
    struct Advisory {
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
        // SAFETY: the test passes a live Advisory and the callback
        // contract supplies `message_len` readable bytes.
        let advisory = unsafe { &mut *userdata.cast::<Advisory>() };
        advisory.kind = kind;
        advisory.pos_id = pos_id;
        // SAFETY: the observer contract keeps the message readable for
        // the duration of this call.
        advisory.message =
            unsafe { std::slice::from_raw_parts(message, message_len as usize) }.to_vec();
    }

    let mut ctx = Context::new();
    let registered = ctx.alloc(16, 1, 15);
    assert!(!registered.is_null());
    ctx.bind_callback(
        callback_code as *const () as *const u8,
        std::ptr::null(),
        std::ptr::null_mut(),
        registered,
    );
    let mut advisory = Advisory::default();
    ctx.set_diagnostics_observer(Some(observe), std::ptr::from_mut(&mut advisory).cast());

    ctx.delete(registered as usize, 91);

    assert_eq!(
        advisory,
        Advisory {
            kind: DIAGNOSTICS_ADVISORY_CALLBACK_USERDATA_FREE,
            pos_id: 91,
            message: b"Context.free of registered callback userdata".to_vec(),
        }
    );
    assert!(!ctx.is_live(registered as usize));
    assert!(!ctx.trapped(), "an advisory must not become a trap");
}

#[test]
fn diagnostics_observer_unset_has_zero_change() {
    fn callback_code() {}

    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        assert!(ctx.diagnostics_observer.is_none(), "{tier}");
        assert!(ctx.diagnostics_observer_userdata.is_null(), "{tier}");
        assert_eq!(
            ctx.binding_count_advisory_threshold,
            u64::MAX,
            "{tier}: binding advisory default changed"
        );
        let registered = ctx.alloc(16, 1, 16);
        assert!(!registered.is_null(), "{tier}");
        ctx.bind_callback(
            callback_code as *const () as *const u8,
            std::ptr::null(),
            registered,
            std::ptr::null_mut(),
        );

        ctx.delete(registered as usize, 92);

        assert!(!ctx.is_live(registered as usize), "{tier}");
        assert!(!ctx.trapped(), "{tier}: default free behavior changed");
        assert!(
            ctx.dead_allocations.is_empty(),
            "{tier}: free retained memory"
        );
    }
}

#[test]
fn alloc_is_zeroed_tagged_and_live() {
    let mut ctx = Context::new();
    let p = ctx.alloc(24, 3, 41);
    assert!(!p.is_null());
    assert!(ctx.is_live(p as usize));
    // SAFETY: p is a fresh 24-byte payload with a 16-byte header.
    unsafe {
        assert_eq!(
            (p.offset(STATE_OFFSET as isize) as *const u64).read(),
            LIVE_STATE
        );
        assert_eq!((p.offset(CLASS_ID_OFFSET as isize) as *const u32).read(), 3);
        assert_eq!((p.offset(POS_ID_OFFSET as isize) as *const u32).read(), 41);
        for i in 0..24 {
            assert_eq!(p.add(i).read(), 0);
        }
    }
}

#[test]
fn allocation_fault_counts_object_requests_identically_in_both_tiers() {
    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        ctx.fail_alloc_after(2);
        assert!(!ctx.alloc(8, 1, 10).is_null(), "{tier}: first request");
        assert!(ctx.alloc(5000, 2, 11).is_null(), "{tier}: second request");
        let trap = ctx.trap_record().expect("injected allocation trap");
        assert_eq!(trap.kind, TrapKind::AllocationFailure, "{tier}");
        assert_eq!(trap.pos_id, 11, "{tier}");
        assert_eq!(trap.message, "injected allocation failure", "{tier}");

        ctx.clear_trap();
        assert!(
            !ctx.alloc(8, 3, 12).is_null(),
            "{tier}: fired fault is one-shot"
        );
        ctx.fail_alloc_after(0);
        assert!(
            !ctx.alloc(8, 4, 13).is_null(),
            "{tier}: zero disables injection"
        );
    }
}

#[test]
fn live_allocation_visitor_reports_class_position_and_tier_bytes() {
    unsafe extern "C" fn collect(
        userdata: *mut c_void,
        class_id: u32,
        pos_id: u32,
        payload_bytes: u64,
    ) {
        // SAFETY: the test passes a live Vec of this exact type.
        let triples = unsafe { &mut *userdata.cast::<Vec<(u32, u32, u64)>>() };
        triples.push((class_id, pos_id, payload_bytes));
    }

    for (tier, mut ctx, expected) in [
        (
            "dev",
            Context::new(),
            vec![(10u32, 20u32, 1u64), (12u32, 22u32, 5000u64)],
        ),
        (
            "ship",
            Context::new_releasing(),
            vec![(10u32, 20u32, 16u64), (12u32, 22u32, 5000u64)],
        ),
    ] {
        let first = ctx.alloc(1, 10, 20);
        let deleted = ctx.alloc(40, 11, 21);
        let large = ctx.alloc(5000, 12, 22);
        assert!(!first.is_null() && !deleted.is_null() && !large.is_null());
        ctx.delete(deleted as usize, 23);

        let mut triples = Vec::new();
        // SAFETY: `collect` receives a live Vec as userdata.
        let visited = unsafe {
            ctx.visit_live_allocations(
                Some(collect),
                (&mut triples as *mut Vec<(u32, u32, u64)>).cast(),
            )
        };
        triples.sort_unstable();
        assert_eq!(visited, 2, "{tier}");
        assert_eq!(triples, expected, "{tier}");
        assert_eq!(visited as usize, ctx.live_count(), "{tier}");
    }
}

#[test]
fn memory_accounting_reports_the_tier_dependent_bytes_of_one_program() {
    let mut measured = Vec::new();
    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let first = ctx.alloc(1, 1, 0);
        let deleted = ctx.alloc(17, 1, 0);
        let large = ctx.alloc(5000, 1, 0);
        assert!(!first.is_null() && !deleted.is_null() && !large.is_null());
        assert_eq!(ctx.live_count(), 3, "{tier}: N allocations");
        let reserved_before = ctx.reserved_bytes();

        ctx.delete(deleted as usize, 0);
        assert_eq!(ctx.live_count(), 2, "{tier}: N-M allocations");
        if tier == "dev" {
            assert!(
                ctx.reserved_bytes() < reserved_before,
                "dev default must release the exact-size layout"
            );
        } else {
            assert_eq!(
                ctx.reserved_bytes(),
                reserved_before,
                "ship size-class storage stays reserved in its reusable arena"
            );
        }
        measured.push((
            tier,
            ctx.live_count(),
            ctx.live_bytes(),
            ctx.reserved_bytes(),
        ));
    }

    assert_eq!(measured[0], ("dev", 2, 5001, 5033));
    assert_eq!(measured[1], ("ship", 2, 5016, 136_088));
    assert_eq!(
        measured[0].1, measured[1].1,
        "live allocation count is tier-independent"
    );
    assert_ne!(
        measured[0].2, measured[1].2,
        "size-class payload capacity makes live bytes tier-dependent"
    );
    assert_ne!(
        measured[0].3, measured[1].3,
        "arena chunks make reserved bytes tier-dependent"
    );
}

#[test]
fn the_byte_counter_matches_the_walk_after_an_allocation() {
    for (mode, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let first = ctx.alloc(24, 1, 0);
        let second = ctx.alloc(17, 1, 0);
        assert!(!first.is_null() && !second.is_null(), "{mode}");
        assert!(ctx.live_bytes() > 0, "{mode}: two allocations are live");
        assert_the_counter_matches_the_walk(&ctx, mode);
    }
}

#[test]
fn the_byte_counter_matches_the_walk_after_a_delete() {
    for (mode, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let kept = ctx.alloc(24, 1, 0);
        let deleted = ctx.alloc(17, 1, 0);
        let before = ctx.live_bytes();
        ctx.delete(deleted as usize, 0);
        assert!(
            ctx.live_bytes() < before,
            "{mode}: the delete leaves the live set"
        );
        assert_the_counter_matches_the_walk(&ctx, mode);
        ctx.delete(kept as usize, 0);
        assert_eq!(ctx.live_bytes(), 0, "{mode}: the live set is empty");
        assert_the_counter_matches_the_walk(&ctx, mode);
    }
}

#[test]
fn the_byte_counter_matches_the_walk_after_a_collection() {
    for (mode, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let kept = ctx.alloc(24, 1, 0);
        let dropped = ctx.alloc(64, 1, 0);
        let mut root = kept as usize;
        ctx.root_add(&mut root as *mut usize as usize, 1);
        let before = ctx.live_bytes();

        ctx.collect();

        assert!(ctx.is_live(kept as usize), "{mode}: the root survives");
        assert!(
            !ctx.is_live(dropped as usize),
            "{mode}: the dead object is swept"
        );
        assert!(
            ctx.live_bytes() < before,
            "{mode}: the sweep leaves the live set"
        );
        assert_the_counter_matches_the_walk(&ctx, mode);
    }
}

#[test]
fn the_byte_counter_matches_the_walk_after_a_large_allocation() {
    // Above LARGEST_BLOCK, so the ship arena takes its large path.
    const LARGE: usize = LARGEST_BLOCK + 904;
    for (mode, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let large = ctx.alloc(LARGE, 1, 0);
        assert!(!large.is_null(), "{mode}");
        assert!(
            ctx.live_bytes() >= LARGE,
            "{mode}: the large payload is live"
        );
        assert_the_counter_matches_the_walk(&ctx, mode);

        ctx.delete(large as usize, 0);

        assert_eq!(ctx.live_bytes(), 0, "{mode}: the live set is empty");
        assert_the_counter_matches_the_walk(&ctx, mode);
    }
}

#[test]
fn the_byte_counter_matches_the_walk_after_retention_and_eviction() {
    // One retained layout (32 payload bytes plus the header) fits;
    // two do not, so the second delete evicts the first record.
    const BUDGET: usize = 72;
    for (mode, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        assert!(ctx.set_freed_handle_diagnostics(true, 0, BUDGET), "{mode}");
        let kept = ctx.alloc(32, 1, 0);
        let oldest = ctx.alloc(32, 1, 0);
        let newest = ctx.alloc(32, 1, 0);
        assert_eq!(ctx.live_bytes(), 96, "{mode}: three live payloads");

        ctx.delete(oldest as usize, 0);

        assert_eq!(ctx.retained_allocations.len(), 1, "{mode}: retention");
        assert_eq!(
            ctx.live_bytes(),
            64,
            "{mode}: retention leaves the live set"
        );
        assert_the_counter_matches_the_walk(&ctx, mode);

        ctx.delete(newest as usize, 0);

        assert_eq!(ctx.retained_allocations.len(), 1, "{mode}: eviction");
        assert!(
            !ctx.dead_allocations.contains(&(oldest as usize)),
            "{mode}: the oldest record is evicted"
        );
        assert_eq!(
            ctx.live_bytes(),
            32,
            "{mode}: eviction does not change the live set"
        );
        assert_the_counter_matches_the_walk(&ctx, mode);
        assert!(ctx.is_live(kept as usize), "{mode}");
    }
}

// The firing control for the collection assertion of §113.2 rule 4.
// The assertion is a debug assertion, so the control runs only where
// the assertion is compiled.
#[test]
#[cfg(debug_assertions)]
fn collection_reports_a_live_byte_counter_that_left_the_live_set() {
    for (mode, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let kept = ctx.alloc(24, 1, 0);
        let mut root = kept as usize;
        ctx.root_add(&mut root as *mut usize as usize, 1);
        ctx.collect();
        ctx.test_offset_live_bytes_counter(1);

        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ctx.collect()));
        std::panic::set_hook(previous);

        assert!(
            outcome.is_err(),
            "{mode}: a counter away from the live set must fail collection"
        );
    }
}

#[test]
fn freed_handle_diagnostics_setting_controls_release_and_retention() {
    let mut releasing = Context::new();
    assert!(releasing.set_freed_handle_diagnostics(false, usize::MAX, 0));
    let released_small = releasing.alloc(8, 1, 0);
    releasing.delete(released_small as usize, 0);
    let released_after_small = releasing.reserved_bytes();
    let released_large = releasing.alloc(8, 1, 0);
    releasing.delete(released_large as usize, 0);
    assert_eq!(releasing.live_bytes(), 0);
    assert_eq!(
        releasing.reserved_bytes(),
        released_after_small,
        "mode off must not retain each freed allocation"
    );
    assert!(
        !releasing.set_freed_handle_diagnostics(true, 0, usize::MAX),
        "the setting is immutable after allocation starts"
    );

    let mut diagnosing = Context::new();
    assert!(diagnosing.set_freed_handle_diagnostics(true, 0, usize::MAX));
    let retained_small = diagnosing.alloc(8, 1, 0);
    diagnosing.delete(retained_small as usize, 0);
    let retained_after_small = diagnosing.reserved_bytes();
    let retained_large = diagnosing.alloc(8, 1, 0);
    diagnosing.delete(retained_large as usize, 0);
    assert_eq!(diagnosing.live_bytes(), 0);
    assert!(
        diagnosing.reserved_bytes() > retained_after_small,
        "mode on must retain each freed allocation"
    );

    diagnosing.delete(retained_large as usize, 7);
    assert_eq!(
        diagnosing.trap_record().map(|record| record.kind),
        Some(TrapKind::DoubleDelete),
        "mode on must preserve the diagnostic path"
    );
}

#[test]
fn freed_handle_diagnostics_threshold_controls_delete_and_collect_accounting() {
    for collect in [false, true] {
        let mut ctx = Context::new();
        assert!(ctx.set_freed_handle_diagnostics(true, 32, usize::MAX));

        let below = ctx.alloc(12, 1, 0);
        assert_eq!(ctx.live_bytes(), 12);
        assert_eq!(ctx.reserved_bytes(), 12 + HEADER_SIZE);
        if collect {
            ctx.collect();
        } else {
            ctx.delete(below as usize, 0);
        }
        assert_eq!(ctx.live_bytes(), 0);
        assert_eq!(
            ctx.reserved_bytes(),
            0,
            "payload below the threshold must be released"
        );

        let at = ctx.alloc(32, 1, 0);
        if collect {
            ctx.collect();
        } else {
            ctx.delete(at as usize, 0);
        }
        assert_eq!(ctx.live_bytes(), 0);
        assert_eq!(
            ctx.reserved_bytes(),
            32 + HEADER_SIZE,
            "payload at the threshold must be retained"
        );

        let above = ctx.alloc(128, 1, 0);
        if collect {
            ctx.collect();
        } else {
            ctx.delete(above as usize, 0);
        }
        assert_eq!(ctx.live_bytes(), 0);
        assert_eq!(
            ctx.reserved_bytes(),
            32 + HEADER_SIZE + 128 + HEADER_SIZE,
            "payload above the threshold must be retained"
        );
        assert_eq!(ctx.dead_allocations.len(), 2);
        assert_eq!(ctx.retained_allocations.len(), 2);
    }
}

#[test]
fn retention_budget_never_exceeds_and_evicts_oldest_first() {
    const BUDGET: usize = 72;
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, BUDGET));

    let oldest = ctx.alloc(32, 1, 0);
    ctx.delete(oldest as usize, 0);
    assert_eq!(ctx.retained_bytes, 48);
    assert_eq!(ctx.reserved_bytes(), 48);

    let middle = ctx.alloc(8, 1, 0);
    ctx.delete(middle as usize, 0);
    assert_eq!(ctx.retained_bytes, BUDGET);
    assert_eq!(ctx.reserved_bytes(), BUDGET);

    let newest = ctx.alloc(8, 1, 0);
    ctx.delete(newest as usize, 0);
    assert!(ctx.retained_bytes <= BUDGET);
    assert_eq!(ctx.retained_bytes, 48);
    assert_eq!(
        ctx.reserved_bytes(),
        48,
        "evicting the older larger layout must lower reserved accounting"
    );
    assert!(!ctx.dead_allocations.contains(&(oldest as usize)));
    assert!(ctx.dead_allocations.contains(&(middle as usize)));
    assert!(ctx.dead_allocations.contains(&(newest as usize)));
    // SAFETY: newest remains retained and owned by the Context.
    unsafe {
        assert_eq!(
            (newest.offset(STATE_OFFSET as isize) as *const u64).read(),
            DEAD_STATE
        );
    }
    assert_eq!(
        ctx.retained_allocations
            .iter()
            .map(|allocation| allocation.payload)
            .collect::<Vec<_>>(),
        [middle as usize, newest as usize],
        "the oldest retirement must be evicted first"
    );

    // No allocation has occurred since `oldest` was evicted, so its
    // released address is still in the deterministic best-effort window.
    assert!(!ctx.require_live_handle(oldest as usize, 31));
    assert_eq!(
        ctx.trap_record().map(|record| record.kind),
        Some(TrapKind::UseAfterDelete)
    );
    ctx.clear_trap();

    // The newest free remains retained and therefore has the guaranteed
    // diagnostic coverage funded by the budget.
    assert!(!ctx.require_live_handle(newest as usize, 32));
    assert_eq!(
        ctx.trap_record().map(|record| record.kind),
        Some(TrapKind::UseAfterDelete)
    );
}

#[test]
fn retention_budget_applies_to_collect_sweeps() {
    const BUDGET: usize = 48;
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, BUDGET));

    for _ in 0..3 {
        assert!(!ctx.alloc(8, 1, 0).is_null());
        ctx.collect();
        assert!(ctx.retained_bytes <= BUDGET);
        assert!(ctx.reserved_bytes() <= BUDGET);
    }
    assert_eq!(ctx.retained_bytes, BUDGET);
    assert_eq!(ctx.retained_allocations.len(), 2);
    assert_eq!(ctx.dead_allocations.len(), 2);
}

#[test]
fn allocation_larger_than_retention_budget_is_released_immediately() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, 47));
    let allocation = ctx.alloc(32, 1, 0);
    ctx.delete(allocation as usize, 0);

    assert_eq!(ctx.retained_bytes, 0);
    assert_eq!(ctx.reserved_bytes(), 0);
    assert!(ctx.retained_allocations.is_empty());
    assert!(!ctx.dead_allocations.contains(&(allocation as usize)));
    assert!(!ctx.require_live_handle(allocation as usize, 41));
    assert_eq!(
        ctx.trap_record().map(|record| record.kind),
        Some(TrapKind::UseAfterDelete)
    );
}

#[test]
fn zero_retention_budget_retains_nothing() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, 0));
    let allocation = ctx.alloc(8, 1, 0);
    ctx.delete(allocation as usize, 0);

    assert_eq!(ctx.retained_bytes, 0);
    assert_eq!(ctx.reserved_bytes(), 0);
    assert!(ctx.retained_allocations.is_empty());
    assert!(ctx.dead_allocations.is_empty());
}

#[test]
fn array_growth_applies_diagnostics_threshold_to_retired_backing_payloads() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 32, usize::MAX));
    let array = ctx.array_new(4, 0);
    for value in 0..9i32 {
        // SAFETY: `array` is a live i32 array and `value` supplies one
        // initialized element for the duration of the call.
        assert!(unsafe { ctx.array_push(array, (&value as *const i32).cast(), 0) } > 0);
    }

    assert_eq!(ctx.dead_allocations.len(), 1);
    assert_eq!(ctx.retained_allocations.len(), 1);
    assert_eq!(
        ctx.retained_allocations[0].layout.size() - HEADER_SIZE,
        32,
        "the retired 16-byte backing store is released and the 32-byte store is retained"
    );
}

#[test]
fn freed_handle_diagnostics_setting_refuses_changes_after_allocation_starts() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 32, usize::MAX));
    let below = ctx.alloc(12, 1, 0);
    assert!(
        !ctx.set_freed_handle_diagnostics(true, 0, usize::MAX),
        "mode and threshold must be immutable after allocation starts"
    );
    ctx.delete(below as usize, 0);
    assert_eq!(
        ctx.reserved_bytes(),
        0,
        "a refused change must leave the original threshold in force"
    );
}

#[test]
fn freed_handle_diagnostics_threshold_and_budget_are_ignored_when_disabled() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(false, usize::MAX, 0));
    let allocation = ctx.alloc(128, 1, 0);
    ctx.delete(allocation as usize, 0);
    assert_eq!(ctx.live_bytes(), 0);
    assert_eq!(ctx.reserved_bytes(), 0);
    assert!(ctx.dead_allocations.is_empty());
    assert!(ctx.retained_allocations.is_empty());
}

#[test]
fn below_threshold_stale_handle_traps_before_address_reuse() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 32, usize::MAX));
    let below = ctx.alloc(12, 1, 0);
    ctx.delete(below as usize, 0);
    assert_eq!(ctx.reserved_bytes(), 0);

    assert!(!ctx.require_live_handle(below as usize, 19));
    assert_eq!(
        ctx.trap_record().map(|record| (record.kind, record.pos_id)),
        Some((TrapKind::UseAfterDelete, 19))
    );
}

#[test]
fn delete_poisons_and_double_delete_traps() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
    let p = ctx.alloc(8, 1, 0);
    ctx.delete(p as usize, 5);
    assert!(!ctx.is_live(p as usize));
    // SAFETY: diagnostic mode retains the bytes after delete.
    unsafe {
        assert_eq!(
            (p.offset(STATE_OFFSET as isize) as *const u64).read(),
            DEAD_STATE
        );
    }
    assert!(!ctx.trapped());
    ctx.delete(p as usize, 6);
    assert!(ctx.trapped());
    let r = ctx.trap_record().expect("trap recorded");
    assert_eq!(r.kind, TrapKind::DoubleDelete);
    assert_eq!(r.pos_id, 6);
}

#[test]
fn retained_dead_handles_trap_after_700_000_subsequent_allocations() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
    let oldest = ctx.alloc(8, 1, 1);
    ctx.delete(oldest as usize, 2);
    let mut probes = vec![(oldest, 700_000usize)];

    for i in 0..700_000usize {
        let handle = ctx.alloc(8, 1, 3);
        ctx.delete(handle as usize, 4);
        let distance = 699_999 - i;
        if matches!(distance, 0 | 1 | 1_000) {
            probes.push((handle, distance));
        }
    }

    assert_eq!(ctx.allocations.len(), 0);
    assert_eq!(ctx.dead_allocations.len(), 700_001);
    assert_eq!(ctx.retained_allocations.len(), 700_001);
    for (handle, distance) in probes {
        // The generated-code path reads this same retained header;
        // the runtime receiver path additionally proves segregation
        // still classifies every distance as stale.
        // SAFETY: diagnostic retain-and-poison owns the header through drop.
        unsafe {
            assert_eq!(
                (handle.offset(STATE_OFFSET as isize) as *const u64).read(),
                DEAD_STATE,
                "distance {distance}"
            );
        }
        assert!(
            !ctx.require_live_handle(handle as usize, 91),
            "distance {distance}"
        );
        let trap = ctx.trap_record().expect("use-after-delete trap");
        assert_eq!(trap.kind, TrapKind::UseAfterDelete, "distance {distance}");
        assert_eq!(
            trap.message, "use of a deleted allocation",
            "distance {distance}"
        );
        assert_eq!(trap.pos_id, 91, "distance {distance}");
        ctx.clear_trap();
    }
}

#[test]
fn ordinary_delete_skips_container_path_and_ship_has_one_membership_lookup() {
    use std::sync::atomic::Ordering::SeqCst;

    for mut ctx in [Context::new(), Context::new_releasing()] {
        let uses_ship_arena = ctx.uses_ship_arena();
        let ordinary = ctx.alloc(16, 1, 0);
        let stats = ctx.test_stats();
        let lookups_before = stats.membership_lookups.load(SeqCst);
        let container_entries_before = stats.container_delete_entries.load(SeqCst);

        ctx.delete(ordinary as usize, 0);

        assert_eq!(
            stats.container_delete_entries.load(SeqCst),
            container_entries_before,
            "an ordinary allocation must not enter the Map/Set delete path"
        );
        assert_eq!(
            stats.membership_lookups.load(SeqCst) - lookups_before,
            usize::from(uses_ship_arena),
            "ship delete must combine class resolution with its one release lookup"
        );

        // Prove the path counter is live, not a vacuous zero.
        let map = crate::assocops::new(&mut ctx, 4, 4, crate::assocops::KeyKind::Bits, false, 0);
        ctx.delete(map as usize, 0);
        assert_eq!(
            stats.container_delete_entries.load(SeqCst),
            container_entries_before + 1
        );
    }
}

#[test]
fn class_release_state_is_identical_on_all_three_free_paths() {
    use std::sync::atomic::Ordering::SeqCst;

    #[derive(Clone, Copy, Debug)]
    enum FreePath {
        Dev,
        ArenaChunk,
        ArenaLarge,
    }

    const INTERN_BYTES: &[u8] = b"release-table-test";
    let classes = [CLASS_MAP, CLASS_SET, CLASS_STRING, CLASS_REGEX];
    let paths = [FreePath::Dev, FreePath::ArenaChunk, FreePath::ArenaLarge];

    for path in paths {
        for class_id in classes {
            let mut ctx = match path {
                FreePath::Dev => Context::new(),
                FreePath::ArenaChunk | FreePath::ArenaLarge => Context::new_releasing(),
            };
            let payload_size = match path {
                FreePath::ArenaLarge => LARGEST_BLOCK + 1,
                FreePath::Dev | FreePath::ArenaChunk => {
                    std::mem::size_of::<crate::assocops::AssocHeader>()
                }
            };
            let payload = ctx.alloc(payload_size, class_id, 0);
            assert!(!payload.is_null(), "{path:?} class {class_id}");

            let stats = ctx.test_stats();
            let container_entries_before = stats.container_delete_entries.load(SeqCst);
            match class_id {
                CLASS_STRING => {
                    ctx.interned.insert(
                        (INTERN_BYTES.as_ptr() as usize, INTERN_BYTES.len()),
                        payload as usize,
                    );
                    ctx.astral_code_points.insert('x' as u32, payload as usize);
                }
                CLASS_REGEX => {
                    let pattern = ctx.alloc_str(b"a", 0);
                    let flags = ctx.alloc_str(b"", 0);
                    let registered = crate::regexops::new(&mut ctx, pattern, flags, 0);
                    assert!(!registered.is_null(), "{path:?}");
                    ctx.regex_store()
                        .rekey_value_for_test(registered as usize, payload as usize);
                    ctx.delete(registered as usize, 0);
                    assert!(
                        ctx.regex_store()
                            .value_handles()
                            .contains(&(payload as usize)),
                        "{path:?}"
                    );
                }
                CLASS_MAP | CLASS_SET => {}
                _ => unreachable!(),
            }

            ctx.delete(payload as usize, 0);

            assert!(!ctx.is_live(payload as usize), "{path:?} class {class_id}");
            assert!(!ctx.trapped(), "{path:?} class {class_id}");
            match class_id {
                CLASS_MAP | CLASS_SET => assert_eq!(
                    stats.container_delete_entries.load(SeqCst),
                    container_entries_before + 1,
                    "{path:?} class {class_id}"
                ),
                CLASS_STRING => {
                    assert!(
                        ctx.interned
                            .values()
                            .all(|handle| *handle != payload as usize),
                        "{path:?}"
                    );
                    assert!(
                        ctx.astral_code_points
                            .values()
                            .all(|handle| *handle != payload as usize),
                        "{path:?}"
                    );
                }
                CLASS_REGEX => assert!(
                    !ctx.regex_store()
                        .value_handles()
                        .contains(&(payload as usize)),
                    "{path:?}"
                ),
                _ => unreachable!(),
            }
        }
    }
}
