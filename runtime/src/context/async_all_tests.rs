use super::*;

fn input(ctx: &mut Context, size: usize) -> *mut u8 {
    let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
    // SAFETY: the fresh allocation has the emitted async header.
    unsafe { ctx.async_register(frame, size) };
    frame
}

fn jobs(ctx: &mut Context, inputs: &[*mut u8]) -> *mut u8 {
    let array = ctx.array_new(std::mem::size_of::<*mut u8>(), 0);
    for handle in inputs {
        // SAFETY: each pointer has one native word of readable storage.
        unsafe { ctx.array_push(array, (handle as *const *mut u8).cast(), 0) };
    }
    array
}

fn complete(ctx: &mut Context, frame: *mut u8, value: i32) {
    // SAFETY: the completion matches the registered four-byte result.
    unsafe { ctx.async_complete(frame, (&value as *const i32).cast(), 4) };
}

fn result(ctx: &mut Context, frame: *mut u8) -> *mut u8 {
    let mut result = std::ptr::null_mut::<u8>();
    // SAFETY: the aggregate caches one array pointer.
    assert!(unsafe {
        ctx.async_result(
            frame,
            (&mut result as *mut *mut u8).cast(),
            std::mem::size_of::<*mut u8>(),
        )
    });
    result
}

fn fail(ctx: &mut Context, frame: *mut u8, message: &str, position: u32) {
    let error = ctx.alloc(24, 1, 0);
    ctx.raise_exception(error, message.to_string(), position);
    ctx.async_complete_exception(frame);
}

#[test]
fn snapshot_duplicates_counts_and_completed_registration() {
    for duplicate in [false, true] {
        let mut ctx = Context::new();
        let a = input(&mut ctx, 4);
        let b = if duplicate { a } else { input(&mut ctx, 4) };
        complete(&mut ctx, a, 7);
        let array = jobs(&mut ctx, &[a, b]);
        // SAFETY: the input array holds registered four-byte result handles.
        let all = unsafe { ctx.async_all(array, 4, 4, 0) };
        assert_eq!(unsafe { ctx.async_count(a) }, if duplicate { 3 } else { 2 });
        assert_eq!(ctx.async_ready_len(), if duplicate { 2 } else { 1 });
        assert_eq!(ctx.async_unfinished(), usize::from(!duplicate));
        let replacement = input(&mut ctx, 4);
        complete(&mut ctx, replacement, 99);
        // Changing the array after the call cannot change the snapshot.
        unsafe {
            (*(array as *mut ArrayHeader))
                .data
                .cast::<*mut u8>()
                .write(replacement)
        };
        if !duplicate {
            complete(&mut ctx, b, 11);
        }
        assert_eq!(ctx.async_pending(), 2);
        assert_eq!(unsafe { ctx.async_step() }, 0);
        let output = result(&mut ctx, all);
        let bytes = unsafe { ctx.array_data(output).cast::<i32>() };
        assert_eq!(unsafe { bytes.read() }, 7);
        assert_eq!(
            unsafe { bytes.add(1).read() },
            if duplicate { 7 } else { 11 }
        );
        assert_eq!(unsafe { ctx.async_count(a) }, 1);
        assert_eq!(unsafe { ctx.async_count(all) }, 1);
        unsafe {
            ctx.async_release(all, 0);
            ctx.async_release(a, 0);
            if !duplicate {
                ctx.async_release(b, 0);
            }
            ctx.async_release(replacement, 0);
        }
        assert!(ctx.async_frames.is_empty());
        assert!(!ctx.trapped());
    }
}

#[test]
fn empty_and_void_aggregates_complete_only_at_the_required_boundary() {
    for count in [0, 1] {
        let mut ctx = Context::new();
        let input = input(&mut ctx, 0);
        unsafe { ctx.async_complete(input, std::ptr::null(), 0) };
        let array = jobs(
            &mut ctx,
            if count == 0 {
                &[]
            } else {
                std::slice::from_ref(&input)
            },
        );
        let all = unsafe { ctx.async_all(array, 0, 0, 0) };
        assert_eq!(ctx.async_pending(), count);
        assert_eq!(ctx.async_unfinished(), 0);
        assert_eq!(
            ctx.async_frames[&(all as usize)].completion.is_some(),
            count == 0
        );
        unsafe { ctx.async_step() };
        let output = result(&mut ctx, all);
        assert_eq!(unsafe { ctx.array_len(output) }, count as i32);
        unsafe {
            ctx.async_release(all, 0);
            ctx.async_release(input, 0);
        }
        assert!(ctx.async_frames.is_empty());
    }
}

#[repr(C)]
struct Observer {
    state: i32,
    count: u32,
    resume: AsyncResume,
    observed: *mut u8,
}

unsafe extern "C" fn observe(ctx: *mut Context, frame: *mut u8, _out: *mut u8) -> u8 {
    // SAFETY: this test registers this exact frame and resume pair.
    let ctx = unsafe { &mut *ctx };
    let input = unsafe { (*frame.cast::<Observer>()).observed };
    ctx.print_line(
        if ctx.async_frames[&(input as usize)].completion.is_some() {
            b"complete"
        } else {
            b"pending"
        },
    );
    1
}

#[test]
fn aggregate_reactions_and_frame_continuations_share_one_fifo() {
    for aggregate_first in [false, true] {
        let mut ctx = Context::new();
        let input = input(&mut ctx, 4);
        let array = jobs(&mut ctx, &[input]);
        let observer = ctx.alloc(std::mem::size_of::<Observer>(), CLASS_GENERATOR, 0);
        unsafe {
            observer.cast::<Observer>().write(Observer {
                state: 0,
                count: 0,
                resume: observe,
                observed: std::ptr::null_mut(),
            });
            ctx.async_register(observer, 0);
        }
        if !aggregate_first {
            unsafe { ctx.async_await(observer, input, 0) };
        }
        let all = unsafe { ctx.async_all(array, 4, 4, 0) };
        unsafe { (*observer.cast::<Observer>()).observed = all };
        if aggregate_first {
            unsafe { ctx.async_await(observer, input, 0) };
        }
        complete(&mut ctx, input, 7);
        assert_eq!(ctx.async_pending(), 2);
        unsafe { ctx.async_step() };
        assert_eq!(
            ctx.take_stdout(),
            if aggregate_first {
                b"complete\n".as_slice()
            } else {
                b"pending\n".as_slice()
            }
        );
        // The observer's mock resume reads no result; release its await count here.
        unsafe {
            ctx.async_release(input, 0);
            ctx.async_release(input, 0);
            ctx.async_release(observer, 0);
            ctx.async_release(all, 0);
        }
        assert!(ctx.async_frames.is_empty());
    }
}

#[test]
fn first_failure_follows_reaction_order_and_later_failures_stay_observed() {
    for reverse in [false, true] {
        let mut ctx = Context::new();
        let a = input(&mut ctx, 4);
        let b = input(&mut ctx, 4);
        let array = jobs(&mut ctx, &[a, b]);
        let all = unsafe { ctx.async_all(array, 4, 4, 0) };
        let (first, later) = if reverse { (b, a) } else { (a, b) };
        fail(&mut ctx, first, "first", 31);
        unsafe { ctx.async_step() };
        let mut pointer = std::ptr::null_mut::<u8>();
        assert!(unsafe {
            ctx.async_result(
                all,
                (&mut pointer as *mut *mut u8).cast(),
                std::mem::size_of::<*mut u8>(),
            )
        });
        assert!(ctx.exception_pending());
        ctx.catch_exception();
        // The remaining reaction survives the aggregate's last holder.
        unsafe { ctx.async_release(all, 0) };
        assert!(ctx.async_frames.contains_key(&(all as usize)));
        fail(&mut ctx, later, "later", 32);
        unsafe { ctx.async_step() };
        unsafe {
            ctx.async_release(a, 0);
            ctx.async_release(b, 0);
        }
        assert!(ctx.async_frames.is_empty());
        assert!(!ctx.trapped());
    }
}

#[test]
fn last_holder_traps_only_for_an_unobserved_aggregate_failure() {
    for observed in [false, true] {
        let mut ctx = Context::new();
        let input = input(&mut ctx, 4);
        let array = jobs(&mut ctx, &[input]);
        let all = unsafe { ctx.async_all(array, 4, 4, 0) };
        fail(&mut ctx, input, "failure", 41);
        unsafe { ctx.async_step() };
        if observed {
            result(&mut ctx, all);
            ctx.catch_exception();
        }
        unsafe {
            ctx.async_release(all, 0);
            ctx.async_release(input, 0);
        }
        assert_eq!(ctx.trapped(), !observed);
        if let Some(trap) = ctx.trap_record() {
            assert_eq!((trap.kind, trap.pos_id), (TrapKind::UncaughtException, 41));
        }
        assert!(ctx.async_frames.is_empty());
    }
}

#[test]
fn collection_preserves_unread_inputs_partial_results_and_cached_results() {
    for collect in [false, true] {
        let mut ctx = Context::new();
        let a = input(&mut ctx, std::mem::size_of::<*mut u8>());
        let b = input(&mut ctx, std::mem::size_of::<*mut u8>());
        let array = jobs(&mut ctx, &[a, b]);
        let all = unsafe {
            ctx.async_all(
                array,
                std::mem::size_of::<*mut u8>(),
                std::mem::size_of::<*mut u8>(),
                0,
            )
        };
        let object = ctx.alloc(16, 1, 0);
        unsafe {
            object.cast::<i32>().write(17);
            ctx.async_complete(
                a,
                (&object as *const *mut u8).cast(),
                std::mem::size_of::<*mut u8>(),
            );
        }
        unsafe {
            ctx.async_release(a, 0);
            ctx.async_release(b, 0);
            ctx.async_step();
        }
        if collect {
            ctx.collect();
        }
        assert_eq!(unsafe { object.cast::<i32>().read() }, 17);
        assert!(ctx.async_frames.contains_key(&(b as usize)));
        let second = ctx.alloc(16, 1, 0);
        unsafe {
            second.cast::<i32>().write(23);
            ctx.async_complete(
                b,
                (&second as *const *mut u8).cast(),
                std::mem::size_of::<*mut u8>(),
            );
            ctx.async_step();
        }
        if collect {
            ctx.collect();
        }
        let output = result(&mut ctx, all);
        let data = unsafe { ctx.array_data(output).cast::<*mut i32>() };
        assert_eq!(unsafe { data.read().read() }, 17);
        assert_eq!(unsafe { data.add(1).read().read() }, 23);
        assert!(!ctx.async_frames.contains_key(&(a as usize)));
        assert!(!ctx.async_frames.contains_key(&(b as usize)));
        unsafe { ctx.async_release(all, 0) };
        assert!(ctx.async_frames.is_empty());
    }
}

#[test]
fn an_unfinished_input_keeps_the_aggregate_alive_after_last_release() {
    for finish in [false, true] {
        let mut ctx = Context::new();
        let input = input(&mut ctx, 4);
        let array = jobs(&mut ctx, &[input]);
        let all = unsafe { ctx.async_all(array, 4, 4, 0) };
        unsafe {
            ctx.async_release(all, 0);
            ctx.async_release(input, 0);
        }
        ctx.collect();
        assert_eq!(ctx.async_unfinished(), 1);
        assert_eq!(ctx.async_pending(), 0);
        assert!(ctx.async_frames.contains_key(&(all as usize)));
        if finish {
            complete(&mut ctx, input, 7);
            unsafe { ctx.async_step() };
            assert!(ctx.async_frames.is_empty());
        }
        assert!(!ctx.trapped());
    }
}

#[test]
fn cleared_aggregate_trap_does_not_report_again_for_a_later_input() {
    for observe in [false, true] {
        let mut ctx = Context::new();
        let a = input(&mut ctx, 4);
        let b = input(&mut ctx, 4);
        let array = jobs(&mut ctx, &[a, b]);
        // SAFETY: both inputs are registered four-byte result handles.
        let all = unsafe { ctx.async_all(array, 4, 4, 0) };
        fail(&mut ctx, a, "first", 51);
        // SAFETY: the queued reaction has the registered input completion.
        unsafe { ctx.async_step() };
        if observe {
            result(&mut ctx, all);
            ctx.catch_exception();
        }
        // SAFETY: the holder owns the aggregate's initial count.
        unsafe { ctx.async_release(all, 0) };
        assert_eq!(ctx.trapped(), !observe);
        ctx.clear_trap();
        fail(&mut ctx, b, "later", 52);
        // SAFETY: the remaining reaction owns its input count.
        unsafe {
            ctx.async_step();
            ctx.async_release(a, 0);
            ctx.async_release(b, 0);
        }
        assert!(!ctx.trapped());
        assert!(ctx.async_frames.is_empty());
    }
}

#[test]
fn ffi_and_context_calls_use_the_same_aggregate_protocol() {
    for ffi_call in [false, true] {
        let mut ctx = Context::new();
        let input = input(&mut ctx, 4);
        complete(&mut ctx, input, 19);
        let array = jobs(&mut ctx, &[input]);
        // SAFETY: the Context is exclusive and the input array is live.
        let all = unsafe {
            if ffi_call {
                crate::ffi::subscript_rt_async_all(&mut *ctx, array, 4, 4, 0)
            } else {
                ctx.async_all(array, 4, 4, 0)
            }
        };
        assert_eq!(ctx.async_pending(), 1);
        // SAFETY: the queued reaction has a cached input completion.
        unsafe { ctx.async_step() };
        let output = result(&mut ctx, all);
        // SAFETY: the result array contains one initialized i32 element.
        assert_eq!(unsafe { ctx.array_data(output).cast::<i32>().read() }, 19);
        // SAFETY: each holder owns one registered handle count.
        unsafe {
            ctx.async_release(all, 0);
            ctx.async_release(input, 0);
        }
        assert!(ctx.async_frames.is_empty());
    }
}

#[test]
fn boolean_input_size_and_array_element_size_are_separate() {
    for (input_size, elem_size, valid) in [(1, 1, true), (1, 4, true), (4, 4, false)] {
        let mut ctx = Context::new();
        let inputs = [input(&mut ctx, 1), input(&mut ctx, 1)];
        for (frame, value) in inputs.into_iter().zip([1u8, 0]) {
            // SAFETY: each frame records one byte and the value has one readable byte.
            unsafe { ctx.async_complete(frame, &value, 1) };
        }
        let array = jobs(&mut ctx, &inputs);
        // SAFETY: the array holds live, non-counted boolean handles.
        let all = unsafe { ctx.async_all(array, input_size, elem_size, 0) };
        unsafe { ctx.async_step() };
        assert_eq!(ctx.trapped(), !valid);
        if valid {
            let output = result(&mut ctx, all);
            let data = unsafe { ctx.array_data(output) };
            if elem_size == 1 {
                assert_eq!(unsafe { std::slice::from_raw_parts(data, 2) }, &[1, 0]);
            } else {
                assert_eq!(
                    unsafe { std::slice::from_raw_parts(data.cast::<i32>(), 2) },
                    &[1, 0]
                );
            }
            unsafe {
                ctx.async_release(all, 0);
                for frame in inputs {
                    ctx.async_release(frame, 0);
                }
            }
        }
    }
}
