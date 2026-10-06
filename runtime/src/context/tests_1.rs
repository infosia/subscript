use super::*;

#[test]
fn ship_mode_delete_frees_and_removes_the_entry() {
    let mut ctx = Context::new_releasing();
    let a = ctx.alloc(8, 1, 0);
    let b = ctx.alloc(8, 1, 0);
    assert_eq!(ctx.live_count(), 2);
    assert_eq!(ctx.allocation_count(), 2);

    ctx.delete(a as usize, 0);
    assert_eq!(ctx.live_count(), 1);
    // The block is released, not merely marked dead.
    assert!(!ctx.is_live(a as usize));
    assert_eq!(
        ctx.allocation_count(),
        1,
        "ship mode leaves no entry behind"
    );

    // A second delete of the now-released pointer does NOT trap
    // (undefined-but-safe no-op, §8.1b), unlike the diagnostic-mode
    // double-delete trap covered by
    // `delete_poisons_and_double_delete_traps`. (Checked before the
    // next alloc: the arena's LIFO free list would hand `a`'s block
    // back, making a later delete of `a` a live delete.)
    ctx.delete(a as usize, 9);
    assert!(!ctx.trapped());

    // A fresh allocation still succeeds after the release.
    let c = ctx.alloc(8, 1, 0);
    assert!(!c.is_null());
    assert!(ctx.is_live(c as usize));

    // Contrast: an equivalent diagnostic-mode delete retains the entry.
    let mut dev = Context::new();
    assert!(dev.set_freed_handle_diagnostics(true, 0, usize::MAX));
    let d0 = dev.alloc(8, 1, 0);
    let _d1 = dev.alloc(8, 1, 0);
    assert_eq!(dev.allocation_count(), 2);
    dev.delete(d0 as usize, 0);
    assert_eq!(
        dev.allocation_count(),
        2,
        "diagnostic mode retains the poisoned entry"
    );
    // `b` and `c` keep the ship context's live set non-trivial.
    assert!(ctx.is_live(b as usize));
}

#[test]
fn delete_of_unowned_pointer_traps_with_nonzero_diagnostics_threshold() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 32, usize::MAX));
    ctx.delete(0x1000, 1);
    assert_eq!(
        ctx.trap_record().map(|r| r.kind),
        Some(TrapKind::InvalidDelete)
    );
}

#[test]
fn clear_trap_resets_reporting_state_and_nothing_else() {
    let mut ctx = Context::new();
    let kept = ctx.alloc(8, 1, 0);
    let deleted = ctx.alloc(8, 1, 0);
    ctx.delete(deleted as usize, 0);
    ctx.print_line(b"before");
    ctx.bump_reload_epoch();

    ctx.trap(TrapKind::EmptyPop, "pop() on an empty array", 3);
    assert!(ctx.trapped());
    ctx.clear_trap();

    // Reporting state is gone...
    assert!(!ctx.trapped());
    assert!(ctx.trap_record().is_none());
    assert_eq!(ctx.trap_flag, 0, "the offset-0 flag is the cleared bit");
    // ...and nothing else moved.
    assert!(ctx.is_live(kept as usize));
    assert!(
        !ctx.is_live(deleted as usize),
        "a deleted allocation stays dead"
    );
    assert_eq!(ctx.reload_epoch(), 1, "staleness survives the clear");
    assert_eq!(ctx.stdout_bytes(), b"before\n");

    // A later fault records normally (the first-trap-wins rule is
    // per uncleared run, not per Context).
    ctx.trap(TrapKind::DivisionByZero, "integer division by zero", 9);
    assert_eq!(
        ctx.trap_record().map(|r| (r.kind, r.pos_id)),
        Some((TrapKind::DivisionByZero, 9))
    );
}

#[test]
fn clear_trap_on_an_untrapped_context_is_a_no_op() {
    let mut ctx = Context::new();
    ctx.clear_trap();
    assert!(!ctx.trapped());
    assert!(ctx.trap_record().is_none());
}

#[test]
fn first_trap_wins() {
    let mut ctx = Context::new();
    ctx.trap(TrapKind::EmptyPop, "first", 1);
    ctx.trap(TrapKind::DivisionByZero, "second", 2);
    let r = ctx.trap_record().expect("trap");
    assert_eq!(r.kind, TrapKind::EmptyPop);
    assert_eq!(r.pos_id, 1);
}

#[test]
fn collect_frees_unreachable_and_keeps_rooted() {
    let mut ctx = Context::new();
    let kept = ctx.alloc(8, 1, 0);
    let dropped = ctx.alloc(8, 1, 0);
    let mut slot: usize = kept as usize;
    let slot_ptr: *mut usize = &mut slot;
    ctx.root_add(slot_ptr as usize, 1);
    ctx.collect();
    assert!(ctx.is_live(kept as usize));
    assert!(!ctx.is_live(dropped as usize));
    // Dropping the last reference frees the rest on the next
    // collect. Written through the registered pointer — the same
    // way generated code updates its shadow slots.
    // SAFETY: `slot` is alive for the whole test.
    unsafe { slot_ptr.write(0) };
    ctx.collect();
    assert!(!ctx.is_live(kept as usize));
}

#[test]
fn collect_moves_unreachable_records_out_of_the_swept_map() {
    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
    let kept = ctx.alloc(8, 1, 0);
    let dropped = ctx.alloc(8, 1, 0);
    let mut root = kept as usize;
    ctx.root_add(&mut root as *mut usize as usize, 1);

    ctx.collect();

    assert_eq!(ctx.allocations.len(), 1, "only the live set is swept");
    assert_eq!(ctx.dead_allocations.len(), 1, "dead record is retained");
    assert!(ctx.allocations.contains_key(&(kept as usize)));
    assert!(ctx.dead_allocations.contains(&(dropped as usize)));
    assert!(ctx.is_live(kept as usize));
    assert!(!ctx.is_live(dropped as usize));
    // A second collection does not touch or discard the dead record.
    ctx.collect();
    assert_eq!(ctx.allocations.len(), 1);
    assert_eq!(ctx.dead_allocations.len(), 1);
}

#[test]
#[ignore = "P24 exit-criterion timing probe; run serialized in release mode"]
fn p24_sweep_time_is_independent_of_retained_dead_entries() {
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    fn median_sweep(ctx: &mut Context) -> (Duration, f64) {
        const WARMUP_SAMPLES: usize = 3;
        const TIMED_SAMPLES: usize = 11;
        const SWEEPS_PER_SAMPLE: usize = 256;

        let mut samples = Vec::with_capacity(TIMED_SAMPLES);
        for sample in 0..WARMUP_SAMPLES + TIMED_SAMPLES {
            let mut total = Duration::ZERO;
            for _ in 0..SWEEPS_PER_SAMPLE {
                for allocation in ctx.allocations.values_mut() {
                    allocation.marked = true;
                }
                let start = Instant::now();
                ctx.sweep_dev_allocations(0);
                total += start.elapsed();
                black_box(ctx.allocations.len());
            }
            if sample >= WARMUP_SAMPLES {
                samples.push(total.as_secs_f64() / SWEEPS_PER_SAMPLE as f64);
            }
        }
        samples.sort_by(f64::total_cmp);
        let median = samples[samples.len() / 2];
        let spread =
            ((samples[samples.len() - 1] - median) / median).max((median - samples[0]) / median);
        assert!(
            spread <= 0.20,
            "P24 sweep measurement spread {:.1}% exceeds the ±20% publication gate",
            spread * 100.0
        );
        (Duration::from_secs_f64(median), spread)
    }

    let mut ctx = Context::new();
    assert!(ctx.set_freed_handle_diagnostics(true, 0, usize::MAX));
    for _ in 0..120_005 {
        assert!(!ctx.alloc(8, 1, 0).is_null());
    }
    let (low, low_spread) = median_sweep(&mut ctx);
    assert_eq!(ctx.allocation_count(), 120_005);

    for _ in 0..600_000 {
        let handle = ctx.alloc(8, 1, 0);
        assert!(!handle.is_null());
        ctx.delete(handle as usize, 0);
    }
    let (high, high_spread) = median_sweep(&mut ctx);
    assert_eq!(ctx.allocations.len(), 120_005);
    assert_eq!(ctx.dead_allocations.len(), 600_000);
    assert_eq!(ctx.retained_allocations.len(), 600_000);
    assert_eq!(ctx.allocation_count(), 720_005);

    eprintln!(
        "P24 dev sweep medians: 120,005 total = {:.3} ms (spread ±{:.1}%); \
             720,005 total = {:.3} ms (spread ±{:.1}%); 120,005 live at both points",
        low.as_secs_f64() * 1_000.0,
        low_spread * 100.0,
        high.as_secs_f64() * 1_000.0,
        high_spread * 100.0,
    );
    // A wide guard catches a cumulative-map walk without turning
    // ordinary sub-millisecond timing noise into a gate.
    assert!(
        high <= low.saturating_mul(3) / 2 + Duration::from_micros(100),
        "sweep grew with retained-dead entries: {low:?} -> {high:?}"
    );
}

#[test]
fn ship_mode_collect_frees_and_removes_unreachable() {
    let mut ctx = Context::new_releasing();
    let kept = ctx.alloc(8, 1, 0);
    let _dropped = ctx.alloc(8, 1, 0);
    assert_eq!(ctx.allocation_count(), 2);
    let mut slot: usize = kept as usize;
    ctx.root_add(&mut slot as *mut usize as usize, 1);
    ctx.collect();
    assert!(ctx.is_live(kept as usize));
    // The unreachable block is released, not poisoned.
    assert_eq!(ctx.allocation_count(), 1, "ship mode releases swept blocks");
    assert_eq!(ctx.live_count(), 1);
}

#[test]
fn dev_collect_does_not_trace_handle_address_in_string_payload() {
    let mut ctx = Context::new();
    let target = ctx.alloc(8, 1, 0) as usize;
    let string = {
        let bytes = target.to_ne_bytes();
        ctx.alloc_str(&bytes, 0)
    };
    let mut root = string as usize;
    ctx.root_add(&mut root as *mut usize as usize, 1);

    ctx.collect();

    assert!(ctx.is_live(string as usize));
    assert!(!ctx.is_live(target));
}

#[test]
fn dev_collect_traces_handle_address_in_reference_payload() {
    let mut ctx = Context::new();
    let inner = ctx.alloc(8, 1, 0);
    let outer = ctx.alloc(8, 1, 0);
    // outer.field0 = inner
    // SAFETY: outer payload is 8 writable bytes.
    unsafe { (outer as *mut usize).write(inner as usize) };
    let mut slot: usize = outer as usize;
    ctx.root_add(&mut slot as *mut usize as usize, 1);
    ctx.collect();
    assert!(ctx.is_live(outer as usize));
    assert!(ctx.is_live(inner as usize));
}

#[test]
fn mark_trace_environment_child() {
    if std::env::var_os(MARK_TRACE_TEST_CHILD).is_none() {
        return;
    }
    let mut ctx = Context::new();
    let inner = ctx.alloc_str(b"trace", 0) as usize;
    let outer = ctx.alloc(8, 7, 0);
    // SAFETY: `outer` has one writable payload word.
    unsafe { outer.cast::<usize>().write(inner) };
    let root = outer as usize;
    ctx.root_add(&root as *const usize as usize, 1);

    ctx.collect();
}

#[test]
fn mark_trace_environment_names_root_and_payload_words_on_stderr() {
    let output =
        std::process::Command::new(std::env::current_exe().expect("runtime test executable path"))
            .args([
                "--exact",
                "context::tests::tests_1::mark_trace_environment_child",
                "--nocapture",
            ])
            .env(MARK_TRACE_TEST_CHILD, "1")
            .env("SUBSCRIPT_MARK_TRACE", "all")
            .output()
            .expect("mark trace child process");

    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("mark trace stderr is UTF-8");
    assert!(
        stderr.contains("class_id=7 source=root set=roots index=0 word=0"),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "class_id={CLASS_STRING} source=payload class_id=7"
        )) && stderr.contains("word=0 value=0x"),
        "{stderr}"
    );
}

#[test]
fn ship_collect_does_not_trace_handle_address_in_string_payload() {
    let mut ctx = Context::new_releasing();
    let small_target = ctx.alloc(8, 1, 0) as usize;
    let small_string = {
        let bytes = small_target.to_ne_bytes();
        ctx.alloc_str(&bytes, 0)
    };

    let large_target = ctx.alloc(8, 1, 0) as usize;
    let large_string = {
        let mut bytes = vec![0; LARGEST_BLOCK];
        bytes[..core::mem::size_of::<usize>()].copy_from_slice(&large_target.to_ne_bytes());
        ctx.alloc_str(&bytes, 0)
    };
    let mut roots = [small_string as usize, large_string as usize];
    ctx.root_add(roots.as_mut_ptr() as usize, roots.len());

    ctx.collect();

    assert!(ctx.is_live(small_string as usize));
    assert!(ctx.is_live(large_string as usize));
    assert!(!ctx.is_live(small_target));
    assert!(!ctx.is_live(large_target));
}

#[test]
fn ship_collect_traces_handle_address_in_reference_payload() {
    let mut ctx = Context::new_releasing();
    let small_target = ctx.alloc(8, 1, 0);
    let small_outer = ctx.alloc(8, 1, 0);
    // SAFETY: small_outer has one writable payload word.
    unsafe { (small_outer as *mut usize).write(small_target as usize) };

    let large_target = ctx.alloc(8, 1, 0);
    let large_outer = ctx.alloc(LARGEST_BLOCK, 1, 0);
    // SAFETY: large_outer has at least one writable payload word.
    unsafe { (large_outer as *mut usize).write(large_target as usize) };
    let mut roots = [small_outer as usize, large_outer as usize];
    ctx.root_add(roots.as_mut_ptr() as usize, roots.len());

    ctx.collect();

    assert!(ctx.is_live(small_outer as usize));
    assert!(ctx.is_live(large_outer as usize));
    assert!(ctx.is_live(small_target as usize));
    assert!(ctx.is_live(large_target as usize));
}

#[test]
fn root_ranges_scan_every_word() {
    // A two-word root range (e.g. a global FixedArray of two
    // references): both interior handles must survive collection.
    let mut ctx = Context::new();
    let a = ctx.alloc(8, 1, 0);
    let b = ctx.alloc(8, 1, 0);
    let range = [a as usize, b as usize];
    ctx.root_add(range.as_ptr() as usize, 2);
    ctx.collect();
    assert!(ctx.is_live(a as usize));
    assert!(ctx.is_live(b as usize));
}

#[test]
fn shadow_frames_root_locals_and_pop_unroots_them() {
    let mut ctx = Context::new();
    let p = ctx.alloc(8, 1, 0);
    let slots = [p as usize];
    ctx.shadow_push(slots.as_ptr() as usize, 1);
    ctx.collect();
    assert!(ctx.is_live(p as usize));
    ctx.shadow_pop();
    ctx.collect();
    assert!(!ctx.is_live(p as usize));
}

#[test]
fn strings_alloc_read_and_intern() {
    let mut ctx = Context::new();
    let h = ctx.alloc_str(b"alpha-beta", 0);
    // SAFETY: h is a live string handle from this context.
    unsafe {
        assert_eq!(ctx.str_bytes(h), b"alpha-beta");
    }
    static LIT: &[u8] = b"hello";
    // SAFETY: LIT is 'static.
    let a = unsafe { ctx.intern_literal(LIT.as_ptr(), LIT.len(), 0) };
    // SAFETY: as above.
    let b = unsafe { ctx.intern_literal(LIT.as_ptr(), LIT.len(), 0) };
    assert_eq!(a, b, "literal interning reuses one allocation");
    // Interned literals survive collection with no other roots.
    ctx.collect();
    assert!(ctx.is_live(a as usize));
}

#[test]
fn arrays_push_index_pop_and_traps() {
    let mut ctx = Context::new();
    let h = ctx.array_new(4, 0);
    // SAFETY: h is a live array handle; sources/dests are valid.
    unsafe {
        assert_eq!(ctx.array_len(h), 0);
        for v in [10i32, 20, 30, 40, 50] {
            let n = ctx.array_push(h, &v as *const i32 as *const u8, 1);
            assert!(n > 0);
        }
        assert_eq!(ctx.array_len(h), 5);
        let p2 = ctx.array_elem_ptr(h, 2, 2);
        assert_eq!((p2 as *const i32).read(), 30);
        let mut out: i32 = 0;
        ctx.array_pop(h, &mut out as *mut i32 as *mut u8, 3);
        assert_eq!(out, 50);
        assert_eq!(ctx.array_len(h), 4);
        // OOB traps and returns null.
        assert!(ctx.array_elem_ptr(h, 4, 9).is_null());
    }
    let r = ctx.trap_record().expect("oob trap");
    assert_eq!(r.kind, TrapKind::IndexOutOfBounds);
    assert_eq!(r.pos_id, 9);
}

#[test]
fn popped_element_is_unreachable_after_pop() {
    assert_popped_element_is_unreachable(Context::new(), "dev");
    assert_popped_element_is_unreachable(Context::new_releasing(), "ship");
}

#[test]
fn array_tail_violations_reports_a_stale_word() {
    assert_array_tail_violation_is_reported(Context::new(), "dev");
    assert_array_tail_violation_is_reported(Context::new_releasing(), "ship");
}

#[test]
fn array_tail_violations_reports_missing_data_allocation() {
    assert_missing_array_data_is_reported(Context::new(), "dev");
    assert_missing_array_data_is_reported(Context::new_releasing(), "ship");
}

#[cfg(debug_assertions)]
#[test]
fn collect_traps_and_returns_on_array_tail_violation() {
    assert_collect_traps_on_array_tail_violation(Context::new(), "dev");
    assert_collect_traps_on_array_tail_violation(Context::new_releasing(), "ship");
}

#[test]
fn array_with_capacity_reserves_empty_storage() {
    let mut ctx = Context::new();
    let h = ctx.array_with_capacity(3, std::mem::size_of::<i32>(), 0);
    assert!(!h.is_null());
    // SAFETY: `h` is a live array handle from this context.
    let header = unsafe { &*(h as *const ArrayHeader) };
    assert_eq!(header.len, 0);
    assert_eq!(header.cap, 3);
    assert_eq!(header.elem_size, 4);
    assert!(!header.data.is_null());
    let data = header.data;
    for value in [10i32, 20, 30] {
        // SAFETY: `h` is live and `value` has the array element type.
        assert!(unsafe { ctx.array_push(h, (&raw const value).cast(), 0) } > 0);
    }
    // SAFETY: `h` remains live and the reserved pushes do not grow it.
    let header = unsafe { &*(h as *const ArrayHeader) };
    assert_eq!(header.len, 3);
    assert_eq!(header.data, data);
}

#[test]
fn array_elem_size_reports_the_creation_size() {
    let mut ctx = Context::new();
    let a = ctx.array_new(4, 0);
    let b = ctx.array_new(16, 0);
    // SAFETY: live array handles of this context.
    unsafe {
        assert_eq!(ctx.array_elem_size(a), 4);
        assert_eq!(ctx.array_elem_size(b), 16);
    }
}

#[test]
fn empty_pop_traps() {
    let mut ctx = Context::new();
    let h = ctx.array_new(4, 0);
    let mut out: i32 = 0;
    // SAFETY: h is a live array handle; dst is valid.
    unsafe { ctx.array_pop(h, &mut out as *mut i32 as *mut u8, 7) };
    assert_eq!(ctx.trap_record().map(|r| r.kind), Some(TrapKind::EmptyPop));
}

#[test]
fn array_data_is_reached_by_conservative_marking() {
    let mut ctx = Context::new();
    let h = ctx.array_new(8, 0);
    let inner = ctx.alloc(8, 1, 0);
    // SAFETY: valid array handle and element source.
    unsafe {
        let v = inner as usize;
        ctx.array_push(h, &v as *const usize as *const u8, 0);
    }
    let mut slot: usize = h as usize;
    ctx.root_add(&mut slot as *mut usize as usize, 1);
    ctx.collect();
    assert!(ctx.is_live(h as usize));
    assert!(
        ctx.is_live(inner as usize),
        "element reached via data pointer"
    );
}

// §8.1a-1: array growth with diagnostics off frees each retired data
// block instead of retaining it poisoned, so allocation_count does not
// grow with the number of capacity doublings. Diagnostic mode retains
// them.
#[test]
fn ship_mode_array_growth_frees_retired_blocks() {
    // Push enough u32 elements to force several capacity doublings
    // (cap: 0 -> 4 -> 8 -> 16 -> 32), retiring the old data block each
    // time (4 retires for 20 pushes).
    let pushes = 20u32;

    let mut ship = Context::new_releasing();
    let sh = ship.array_new(4, 0);
    for v in 0..pushes {
        // SAFETY: sh is a live u32 array handle; src is a valid u32.
        let n = unsafe { ship.array_push(sh, &v as *const u32 as *const u8, 0) };
        assert!(n > 0);
    }
    // Header + current live data block only; retired blocks are freed
    // and removed, so the map holds a small constant, not 1 + N.
    let ship_count = ship.allocation_count();
    assert!(
        ship_count <= 2,
        "ship tier should hold header + live data only, got {ship_count}"
    );

    let mut dev = Context::new();
    assert!(dev.set_freed_handle_diagnostics(true, 0, usize::MAX));
    let dh = dev.array_new(4, 0);
    for v in 0..pushes {
        // SAFETY: dh is a live u32 array handle; src is a valid u32.
        let n = unsafe { dev.array_push(dh, &v as *const u32 as *const u8, 0) };
        assert!(n > 0);
    }
    // Dev tier retains every retired block poisoned, so the map is
    // strictly larger.
    let dev_count = dev.allocation_count();
    assert!(
        dev_count > ship_count,
        "diagnostic mode retains retired blocks: dev {dev_count} vs ship {ship_count}"
    );
}

// ----- ship-tier arena (§8.1b) -----

// alloc→delete→alloc of the same class pops the free-listed block
// (LIFO: the same address comes back) and never grows a chunk.
#[test]
fn ship_arena_reuses_free_listed_block_without_chunk_growth() {
    let mut ctx = Context::new_releasing();
    let first = ctx.alloc(16, 1, 0);
    assert!(!first.is_null());
    assert_eq!(ctx.chunk_count(), 1);
    for _ in 0..10_000 {
        ctx.delete(first as usize, 0);
        let again = ctx.alloc(16, 1, 0);
        assert_eq!(again, first, "LIFO free list returns the same block");
    }
    assert_eq!(ctx.chunk_count(), 1, "reuse cycles must not grow chunks");
    assert_eq!(ctx.live_count(), 1);
}

// Free-list reuse must return a zeroed payload (§8.1b): the free-list
// link occupies the payload's first word and the previous contents
// the rest, so both must be scrubbed.
#[test]
fn ship_arena_free_list_reuse_returns_zeroed_payload() {
    let mut ctx = Context::new_releasing();
    let p = ctx.alloc(16, 1, 0);
    // SAFETY: p is a live 16-byte payload.
    unsafe { std::ptr::write_bytes(p, 0xAB, 16) };
    ctx.delete(p as usize, 0);
    let q = ctx.alloc(16, 1, 0);
    assert_eq!(q, p, "the dirtied block is the one reused");
    // SAFETY: q is a live 16-byte payload.
    unsafe {
        for i in 0..16 {
            assert_eq!(q.add(i).read(), 0, "byte {i} not zeroed on reuse");
        }
    }
}

#[test]
fn alloc_str_with_writes_exact_result_bytes_in_both_tiers() {
    for (tier, mut ctx) in [("dev", Context::new()), ("ship", Context::new_releasing())] {
        let handle = ctx.alloc_str_with(5, 17, |destination| {
            assert_eq!(destination.len(), 5, "{tier}");
            destination.copy_from_slice(b"write");
        });
        assert!(!handle.is_null(), "{tier}");
        // SAFETY: `handle` is a live string in this Context.
        unsafe { assert_eq!(ctx.str_bytes(handle), b"write", "{tier}") };
    }
}

#[test]
fn ship_string_writer_replaces_exposed_bytes_without_zeroing_padding() {
    let mut ctx = Context::new_releasing();
    let first = ctx.alloc_str(b"a", 0);
    // A one-byte string uses 9 exposed payload bytes in a class with
    // 16 payload bytes. Mark the seven padding bytes before release.
    // SAFETY: `first` has 16 bytes of size-class payload capacity.
    unsafe { std::ptr::write_bytes(first.add(9), 0xAB, 7) };
    ctx.delete(first as usize, 0);

    let reused = ctx.alloc_str_with(1, 0, |destination| {
        destination[0] = b'z';
    });
    assert_eq!(reused, first, "the string free list must reuse the block");
    // SAFETY: `reused` is a live string in this Context. The padding
    // read stays inside its size-class payload capacity.
    unsafe {
        assert_eq!(ctx.str_bytes(reused), b"z");
        assert_eq!(reused.add(9).read(), 0xAB);
    }
}

// Context drop frees every chunk and large record (no leak), observed
// through the test-only resource balance that outlives the Context.
#[test]
fn ship_context_drop_frees_all_chunks_and_large_records() {
    let mut ctx = Context::new_releasing();
    // Several classes, enough small blocks for a real chunk, and two
    // large records (one deleted before the drop).
    for _ in 0..100 {
        assert!(!ctx.alloc(16, 1, 0).is_null());
        assert!(!ctx.alloc(200, 1, 0).is_null());
    }
    let big = ctx.alloc(LARGEST_BLOCK + 1, 2, 0);
    let big2 = ctx.alloc(64 * 1024, 2, 0);
    assert!(!big.is_null() && !big2.is_null());
    ctx.delete(big2 as usize, 0);
    let stats = ctx.test_stats();
    use std::sync::atomic::Ordering::SeqCst;
    assert!(
        stats.chunks.load(SeqCst) >= 2,
        "distinct classes use distinct chunks"
    );
    assert_eq!(stats.large.load(SeqCst), 1);
    drop(ctx);
    assert_eq!(stats.chunks.load(SeqCst), 0, "drop must free every chunk");
    assert_eq!(
        stats.large.load(SeqCst),
        0,
        "drop must free every large record"
    );
}

// Arena edition of the collect tests: unreachable classed blocks are
// released (live_count drops, storage is reusable), rooted ones
// survive with their header restored to LIVE_STATE.
#[test]
fn ship_collect_releases_unreachable_and_keeps_rooted() {
    let mut ctx = Context::new_releasing();
    let kept = ctx.alloc(16, 1, 0);
    let dropped = ctx.alloc(16, 1, 0);
    let dropped2 = ctx.alloc(16, 1, 0);
    // kept.field0 = inner: reached transitively through the
    // header-recorded payload size.
    let inner = ctx.alloc(16, 1, 0);
    // SAFETY: kept payload is 16 writable bytes.
    unsafe { (kept as *mut usize).write(inner as usize) };
    let mut slot: usize = kept as usize;
    let slot_ptr: *mut usize = &mut slot;
    ctx.root_add(slot_ptr as usize, 1);
    assert_eq!(ctx.live_count(), 4);
    ctx.collect();
    assert!(ctx.is_live(kept as usize));
    assert!(ctx.is_live(inner as usize), "traced through payload words");
    assert!(!ctx.is_live(dropped as usize));
    assert!(!ctx.is_live(dropped2 as usize));
    assert_eq!(ctx.live_count(), 2);
    // Survivor headers are LIVE again (mark state fully restored).
    // SAFETY: kept is a live payload with a 16-byte header.
    unsafe {
        assert_eq!(
            (kept.offset(STATE_OFFSET as isize) as *const u64).read(),
            LIVE_STATE
        );
    }
    // Swept blocks are on the free list: the next same-class alloc
    // reuses one instead of bumping.
    let chunks = ctx.chunk_count();
    let reused = ctx.alloc(16, 1, 0);
    assert!(reused == dropped || reused == dropped2);
    assert_eq!(ctx.chunk_count(), chunks);
    // Unrooting frees the rest on the next collect.
    // SAFETY: `slot` is alive for the whole test.
    unsafe { slot_ptr.write(0) };
    ctx.collect();
    assert!(!ctx.is_live(kept as usize));
    assert!(!ctx.is_live(inner as usize));
}

// The large-record path (§8.1b): membership is an exact address
// match, tracing uses the record's payload size, collect frees an
// unreached record, and delete frees immediately.
#[test]
fn ship_large_allocations_membership_trace_collect_and_delete() {
    let mut ctx = Context::new_releasing();
    let big = ctx.alloc(2 * LARGEST_BLOCK, 1, 0);
    assert!(!big.is_null());
    assert!(ctx.is_live(big as usize));
    assert!(
        !ctx.is_live(big as usize + 8),
        "interior address is not a payload"
    );
    assert_eq!(ctx.live_count(), 1);
    // A classed block referenced from the large payload's interior
    // survives collect: the record's size drives the trace.
    let inner = ctx.alloc(16, 1, 0);
    // SAFETY: big is a live payload of 2*LARGEST_BLOCK bytes.
    unsafe { (big.add(LARGEST_BLOCK) as *mut usize).write(inner as usize) };
    let mut slot: usize = big as usize;
    let slot_ptr: *mut usize = &mut slot;
    ctx.root_add(slot_ptr as usize, 1);
    ctx.collect();
    assert!(ctx.is_live(big as usize));
    assert!(
        ctx.is_live(inner as usize),
        "traced through the large payload"
    );
    // Unrooted, collect frees the record (and the inner block).
    // SAFETY: `slot` is alive for the whole test.
    unsafe { slot_ptr.write(0) };
    ctx.collect();
    assert!(!ctx.is_live(big as usize));
    assert_eq!(ctx.live_count(), 0);
    use std::sync::atomic::Ordering::SeqCst;
    assert_eq!(ctx.test_stats().large.load(SeqCst), 0);
    // Direct delete of a large allocation frees it too.
    let big3 = ctx.alloc(LARGEST_BLOCK + 100, 1, 0);
    assert_eq!(ctx.test_stats().large.load(SeqCst), 1);
    let reserved_before_delete = ctx.reserved_bytes();
    ctx.delete(big3 as usize, 0);
    assert!(!ctx.is_live(big3 as usize));
    assert_eq!(ctx.test_stats().large.load(SeqCst), 0);
    assert!(
        ctx.reserved_bytes() < reserved_before_delete,
        "a large delete returns its individual allocation to the system"
    );
}

// The exact membership test (§8.1b): chunk range, block grid, bump
// watermark, live header — all four. Near-miss addresses are not
// blocks: is_live says no and delete is a no-op that never traps and
// never corrupts the free list.
#[test]
fn ship_membership_rejects_off_grid_and_above_watermark_addresses() {
    let mut ctx = Context::new_releasing();
    let p = ctx.alloc(16, 1, 0);
    let q = ctx.alloc(16, 1, 0);
    assert_eq!(ctx.live_count(), 2);
    // Off-grid: interior of a live payload.
    assert!(!ctx.is_live(p as usize + 8));
    ctx.delete(p as usize + 8, 0);
    // In-chunk but above the bump watermark (the next grid slot).
    let next_slot = q as usize + SMALLEST_BLOCK;
    assert!(!ctx.is_live(next_slot));
    ctx.delete(next_slot, 0);
    // Outside any chunk.
    assert!(!ctx.is_live(0x1000));
    ctx.delete(0x1000, 0);
    assert!(!ctx.trapped(), "ship-tier delete never traps");
    assert_eq!(ctx.live_count(), 2, "no-ops must not release live blocks");
    assert!(ctx.is_live(p as usize) && ctx.is_live(q as usize));
    // The allocator still works: both blocks delete and reuse cleanly.
    ctx.delete(p as usize, 0);
    ctx.delete(q as usize, 0);
    let r = ctx.alloc(16, 1, 0);
    assert_eq!(r, q, "free list intact after the no-op deletes");
}

// Ship-tier interning and array retirement ride the arena: interned
// strings survive collect (roots), and retired array data blocks land
// on free lists without growing the live set.
#[test]
fn ship_interned_strings_and_arrays_work_on_the_arena() {
    let mut ctx = Context::new_releasing();
    static LIT: &[u8] = b"arena-lit";
    // SAFETY: LIT is 'static.
    let a = unsafe { ctx.intern_literal(LIT.as_ptr(), LIT.len(), 0) };
    // SAFETY: as above.
    let b = unsafe { ctx.intern_literal(LIT.as_ptr(), LIT.len(), 0) };
    assert_eq!(a, b);
    ctx.collect();
    assert!(ctx.is_live(a as usize), "interned literal is a root");
    // SAFETY: a is a live string handle of this context.
    unsafe { assert_eq!(ctx.str_bytes(a), b"arena-lit") };

    let h = ctx.array_new(4, 0);
    for v in 0..20u32 {
        // SAFETY: h is a live u32 array handle; src is a valid u32.
        let n = unsafe { ctx.array_push(h, &v as *const u32 as *const u8, 0) };
        assert!(n > 0);
    }
    // SAFETY: h is a live array handle.
    unsafe {
        assert_eq!(ctx.array_len(h), 20);
        let p7 = ctx.array_elem_ptr(h, 7, 0);
        assert_eq!((p7 as *const u32).read(), 7);
    }
}
