//! Class allocations carry resolved field offsets and recursive release nodes (§172).

use subscript_runtime::{context::CLASS_GENERATOR, ffi::*, Context, TrapKind};

fn completed(ctx: &mut Context) -> *mut u8 {
    let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
    // SAFETY: the allocation holds an async header and a void completion.
    unsafe {
        subscript_rt_async_register_uncounted(ctx, frame, 0);
        ctx.async_complete(frame, std::ptr::null(), 0);
    }
    frame
}

#[test]
fn described_object_free_releases_each_field_once_and_keeps_unowned_controls() {
    for ship in [false, true] {
        for described in [false, true] {
            let mut ctx = if ship {
                Context::new_releasing()
            } else {
                Context::new()
            };
            let frame = completed(&mut ctx);
            let object = ctx.alloc(24, 900, 0);
            // One counted FixedArray field at byte offset 8, with two handle slots.
            let description: [u64; 9] = [1, 8, 48, 3, 8, 2, 1, 0, 0];
            // SAFETY: both field slots fit the live object. Each slot owns one count.
            unsafe {
                object.add(8).cast::<*mut u8>().write(frame);
                object.add(16).cast::<*mut u8>().write(frame);
                ctx.async_retain(frame);
                ctx.async_retain(frame);
                if described {
                    subscript_rt_object_describe(
                        &mut *ctx,
                        object,
                        description.as_ptr().cast(),
                        72,
                    );
                }
                ctx.delete(object as usize, 10);
                assert_eq!(ctx.async_count(frame), if described { 1 } else { 3 });
                ctx.collect();
                assert_eq!(ctx.async_count(frame), if described { 1 } else { 3 });
                assert!(!ctx.trapped());
            }
        }
    }
}

#[test]
fn object_release_leaves_use_the_supplied_registry_before_storage_free() {
    for described in [false, true] {
        let mut ctx = Context::new();
        // These values are foreign registry keys, not native frame pointers.
        let leaves = [101usize, 202];
        let object = ctx.alloc(16, 900, 0);
        let description: [u64; 9] = [1, 0, 48, 3, 8, 2, 1, 0, 0];
        // SAFETY: the live inline field has two pointer-sized slots and no native release occurs.
        unsafe {
            std::ptr::copy_nonoverlapping(leaves.as_ptr().cast::<u8>(), object, 16);
            if described {
                ctx.describe_object(
                    object as usize,
                    std::slice::from_raw_parts(description.as_ptr().cast(), 72),
                );
            }
        }
        let (handles, storage) = ctx
            .take_object_releases(object as usize, 0)
            .expect("valid description");
        assert_eq!(
            handles,
            if described {
                leaves.to_vec()
            } else {
                Vec::new()
            }
        );
        assert!(storage.is_empty());
        assert!(ctx.is_live(object as usize));
        assert_eq!(
            ctx.take_object_releases(object as usize, 0)
                .expect("valid description"),
            (Vec::new(), Vec::new())
        );
        ctx.delete(object as usize, 0);
        assert!(!ctx.is_live(object as usize));
        assert!(!ctx.trapped());
    }
}

#[test]
fn object_release_keeps_array_storage_until_the_consumer_releases_leaves() {
    for done in [false, true] {
        let mut ctx = Context::new();
        let frame = completed(&mut ctx);
        // A counted IterResult whose value is an array of handles.
        let description: [u64; 12] = [1, 0, 72, 4, 8, 1, 2, 0, 0, 1, 0, 0];
        let object = ctx.alloc(16, 900, 0);
        // SAFETY: the result layout has its discriminant at 0 and its array handle at 8.
        unsafe {
            let array = subscript_rt_array_new(&mut *ctx, 8, 0);
            subscript_rt_array_push(&mut *ctx, array, (&raw const frame).cast(), 0);
            let data = subscript_rt_array_data(&*ctx, array);
            object.write(u8::from(done));
            object.add(8).cast::<*mut u8>().write(array);
            ctx.describe_object(
                object as usize,
                std::slice::from_raw_parts(description.as_ptr().cast(), 96),
            );
            let (leaves, storage) = ctx
                .take_object_releases(object as usize, 0)
                .expect("valid description");
            assert_eq!(
                leaves,
                if done {
                    Vec::new()
                } else {
                    vec![frame as usize]
                }
            );
            assert!(ctx.is_live(array as usize));
            assert!(ctx.is_live(data as usize));
            for leaf in leaves {
                ctx.async_release(leaf as *mut u8, 0);
            }
            for address in storage {
                ctx.delete(address, 0);
            }
            assert_eq!(ctx.is_live(array as usize), done);
            assert_eq!(ctx.is_live(frame as usize), done);
            ctx.delete(object as usize, 0);
        }
    }
}

#[test]
fn sweep_removes_the_description_before_an_uncounted_allocation_reuses_the_address() {
    for described in [false, true] {
        let mut ctx = Context::new_releasing();
        let first = completed(&mut ctx);
        let second = completed(&mut ctx);
        let object = ctx.alloc(16, 900, 0);
        let description: [u64; 6] = [1, 0, 24, 1, 0, 0];
        // SAFETY: the first object stores one acquired handle. The second object only borrows.
        unsafe {
            object.cast::<*mut u8>().write(first);
            ctx.async_retain(first);
            if described {
                ctx.describe_object(
                    object as usize,
                    std::slice::from_raw_parts(description.as_ptr().cast(), 48),
                );
            }
            ctx.collect();
            let reused = ctx.alloc(16, 900, 0);
            assert_eq!(reused, object);
            reused.cast::<*mut u8>().write(second);
            ctx.delete(reused as usize, 0);
            assert_eq!(ctx.async_count(second), 1);
            assert!(ctx.is_live(first as usize));
            assert!(!ctx.trapped());
        }
    }
}

#[test]
fn object_free_releases_all_failed_fields_before_it_frees_storage() {
    for fail in [false, true] {
        let mut ctx = Context::new();
        let object = ctx.alloc(16, 900, 0);
        let description: [u64; 9] = [1, 0, 48, 3, 8, 2, 1, 0, 0];
        let mut frames = Vec::new();
        // SAFETY: the object's inline FixedArray owns both registered frames.
        unsafe {
            for index in 0..2 {
                let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
                subscript_rt_async_register_uncounted(&mut *ctx, frame, 0);
                if fail {
                    let error = ctx.alloc(24, 1, 0);
                    ctx.raise_exception(error, "Error: field".into(), 7);
                    ctx.async_complete_exception(frame);
                } else {
                    ctx.async_complete(frame, std::ptr::null(), 0);
                }
                object.add(index * 8).cast::<*mut u8>().write(frame);
                frames.push(frame);
            }
            ctx.describe_object(
                object as usize,
                std::slice::from_raw_parts(description.as_ptr().cast(), 72),
            );
            ctx.delete(object as usize, 11);
        }
        assert!(!ctx.is_live(object as usize));
        assert!(frames.iter().all(|frame| !ctx.is_live(*frame as usize)));
        assert_eq!(ctx.trapped(), fail);
        if fail {
            assert_eq!(ctx.trap_record().expect("first failure").pos_id, 7);
        }
    }
}

#[test]
fn malformed_object_description_sizes_return_errors_without_releases() {
    for words in [
        vec![],
        vec![1],
        vec![1, 0, u64::MAX],
        vec![1, 0, 24, 1],
        vec![1, 0, 0],
        vec![1, 0, 24, 2, 0, 0],
    ] {
        let mut ctx = Context::new();
        let object = ctx.alloc(8, 900, 0);
        let bytes: Vec<_> = words.iter().flat_map(|word| word.to_ne_bytes()).collect();
        // SAFETY: the object is live. This test deliberately supplies malformed description sizes.
        unsafe {
            ctx.describe_object(object as usize, &bytes);
        }
        let result = ctx.take_object_releases(object as usize, 0);
        if words.is_empty() {
            assert_eq!(result.expect("empty description"), (vec![], vec![]));
        } else {
            assert_eq!(
                result.expect_err("malformed size"),
                "malformed object description size"
            );
        }
    }
}

#[test]
fn null_object_description_traps_before_slice_construction() {
    for size in [0, 24, u64::MAX] {
        let mut ctx = Context::new();
        let object = ctx.alloc(8, 900, 0);
        // SAFETY: the Context and object are live; the API checks the null description.
        unsafe {
            subscript_rt_object_describe(&mut *ctx, object, std::ptr::null(), size);
        }
        assert_eq!(
            ctx.trap_record().expect("null description trap").kind,
            TrapKind::Internal
        );
    }
}
