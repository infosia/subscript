//! A collection reclaims a completed task that only unreachable objects hold
//! (`compiler.md` §348). Each test runs in the dev allocator and in the ship
//! arena. Each "is kept" test has a firing control in the same shape.

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use super::*;

/// The two allocator tiers: the dev allocator and the ship arena.
/// The tests read one automatic collection as one pause: a budget of 0
/// (`compiler.md` §356 rule 16).
fn contexts() -> [Box<Context>; 2] {
    let mut contexts = [Context::new(), Context::new_releasing()];
    for ctx in &mut contexts {
        assert!(ctx.set_collection_step(0));
    }
    contexts
}

/// One counted handle field at offset 0 (`compiler.md` §171).
const HANDLE_FIELD: [u64; 6] = [1, 0, 24, 1, 0, 0];

/// An object of one counted handle field that holds `frame`. The field
/// owns one count of the frame.
fn holder(ctx: &mut Context, frame: *mut u8) -> usize {
    let object = ctx.alloc(8, 900, 0);
    // SAFETY: the object has one 8-byte field, and the description states
    // one handle field at offset 0.
    unsafe {
        object.cast::<*mut u8>().write(frame);
        ctx.describe_object(
            object as usize,
            std::slice::from_raw_parts(HANDLE_FIELD.as_ptr().cast(), 48),
        );
    }
    object as usize
}

/// A registered invocation frame with a count of one and no completion.
fn frame(ctx: &mut Context) -> *mut u8 {
    let frame = ctx.alloc(16, CLASS_GENERATOR, 0);
    // SAFETY: the frame is a fresh coroutine allocation of this Context.
    unsafe { ctx.async_register(frame, 8) };
    frame
}

/// Completes `frame` with the 8-byte value `value`.
fn complete(ctx: &mut Context, frame: *mut u8, value: usize) {
    // SAFETY: the frame is registered, and the value is 8 readable bytes.
    unsafe { ctx.async_complete(frame, (&value as *const usize).cast(), 8) };
}

/// The completion value of a registered completed frame.
fn completion(ctx: &mut Context, frame: *mut u8) -> usize {
    let mut value = 0usize;
    // SAFETY: the output holds the 8-byte completion.
    let done = unsafe { ctx.async_result(frame, (&mut value as *mut usize).cast(), 8) };
    assert!(done, "the frame has a completion");
    value
}

/// The completed task of the `i1` shape: the object `W` holds the only
/// handle of the frame, and the completion value of the frame is `W`.
/// `extra` counts are added for outside holders. Returns the frame and `W`.
fn returns_its_holder(ctx: &mut Context, extra: u32) -> (*mut u8, usize) {
    let task = frame(ctx);
    let object = holder(ctx, task);
    complete(ctx, task, object);
    for _ in 0..extra {
        // SAFETY: the frame is registered.
        unsafe { ctx.async_retain(task) };
    }
    (task, object)
}

/// Rules 3 and 6: a completed task with an outside holder stays, with its
/// completion and its holder object, after a collection. The outside holder
/// owns a count that no heap holder explains. Control: with no outside
/// holder, the task ends and its holder object is freed (rule 4).
#[test]
fn an_outside_holder_keeps_a_completed_task_and_its_completion() {
    for mut ctx in contexts() {
        let (task, object) = returns_its_holder(&mut ctx, 1);
        ctx.collect();
        assert!(ctx.async_frames.contains_key(&(task as usize)));
        assert!(ctx.is_live(task as usize) && ctx.is_live(object));
        // SAFETY: the frame stays registered.
        assert_eq!(unsafe { ctx.async_count(task) }, 2);
        assert_eq!(completion(&mut ctx, task), object);
        // SAFETY: the object stays live, and its field is the handle.
        assert_eq!(unsafe { (object as *const usize).read() }, task as usize);
        assert!(!ctx.trapped());

        let (task, object) = returns_its_holder(&mut ctx, 0);
        ctx.collect();
        assert!(!ctx.async_frames.contains_key(&(task as usize)));
        assert!(!ctx.is_live(task as usize) && !ctx.is_live(object));
        assert!(!ctx.trapped());
    }
}

/// Rule 2: a completed task that the mark reaches keeps its completion. A
/// root reaches an object that holds the task, and only the completion
/// value reaches `kept`. Control: with the root cleared, `kept` is freed.
#[test]
fn a_reached_completed_task_keeps_its_completion_value() {
    for mut ctx in contexts() {
        let task = frame(&mut ctx);
        let object = holder(&mut ctx, task);
        let kept = ctx.alloc(24, 901, 0) as usize;
        complete(&mut ctx, task, kept);
        let root = Box::into_raw(Box::new(object));
        // SAFETY: the slot stays allocated until the end of the test.
        unsafe { ctx.root_add(root as usize, 1) };
        ctx.collect();
        assert!(ctx.is_live(kept) && ctx.is_live(object) && ctx.is_live(task as usize));
        assert_eq!(completion(&mut ctx, task), kept);

        // SAFETY: the slot is a live, exclusive allocation of this test.
        unsafe { root.write(0) };
        ctx.collect();
        assert!(!ctx.is_live(kept) && !ctx.is_live(object));
        assert!(!ctx.async_frames.contains_key(&(task as usize)));
        drop(ctx);
        // SAFETY: the Context that read the slot is dropped.
        drop(unsafe { Box::from_raw(root) });
    }
}

/// Rules 2 and 3: the first task has an outside holder, and its completion
/// is an object that holds the only handle of the second task. Step 3
/// counts that object as an unreachable heap holder of the second task, so
/// step 3 does not keep the second task. The mark from the first task
/// reaches the object, and the object reaches the second task. Control:
/// with no outside holder, both tasks end.
#[test]
fn a_task_that_a_kept_task_reaches_stays() {
    for mut ctx in contexts() {
        for outside in [1, 0] {
            let second = frame(&mut ctx);
            let link = holder(&mut ctx, second);
            let value = ctx.alloc(24, 901, 0) as usize;
            complete(&mut ctx, second, value);
            let first = frame(&mut ctx);
            let first_holder = holder(&mut ctx, first);
            complete(&mut ctx, first, link);
            for _ in 0..outside {
                // SAFETY: the frame is registered.
                unsafe { ctx.async_retain(first) };
            }
            ctx.collect();
            assert!(!ctx.trapped());
            assert!(!ctx.is_live(first_holder));
            let kept = outside != 0;
            for task in [first, second] {
                assert_eq!(ctx.async_frames.contains_key(&(task as usize)), kept);
            }
            assert_eq!(ctx.is_live(link), kept);
            assert_eq!(ctx.is_live(value), kept);
            if kept {
                assert_eq!(completion(&mut ctx, first), link);
                assert_eq!(completion(&mut ctx, second), value);
            }
        }
    }
}

/// Rule 5: an unfinished task with only an unreachable holder stays
/// registered, and so does its frame. Control: the completed task of the
/// same shape ends.
#[test]
fn an_unfinished_task_stays_a_root() {
    for mut ctx in contexts() {
        let unfinished = frame(&mut ctx);
        let object = holder(&mut ctx, unfinished);
        ctx.collect();
        assert!(!ctx.is_live(object));
        assert!(ctx.async_frames.contains_key(&(unfinished as usize)));
        assert!(ctx.is_live(unfinished as usize));

        let (task, _) = returns_its_holder(&mut ctx, 0);
        ctx.collect();
        assert!(!ctx.async_frames.contains_key(&(task as usize)));
    }
}

/// Rule 4 and §342 rule 12: a failed task whose Error object holds its
/// only handle ends in an automatic collection. The failure is recorded,
/// the record keeps the Error object, and the next explicit collection
/// reports it.
#[test]
fn a_failed_task_that_its_error_object_holds_is_recorded() {
    for mut ctx in contexts() {
        let task = frame(&mut ctx);
        let error = holder(&mut ctx, task);
        // SAFETY: the error is a live handle that the exception roots keep.
        unsafe { ctx.raise_exception(error as *mut u8, "Error: cycle".into(), 47) };
        ctx.async_complete_exception(task);
        assert!(!ctx.exception_pending());
        ctx.collect_automatic();
        assert!(!ctx.trapped());
        assert!(!ctx.async_frames.contains_key(&(task as usize)));
        assert_eq!(ctx.recorded_failure_count(), 1);
        assert!(ctx.is_live(error), "the record keeps the Error object");
        ctx.collect();
        let trap = ctx.trap_record().expect("the explicit collection reports");
        assert_eq!(trap.kind, TrapKind::UncaughtException);
        assert_eq!((trap.message.as_str(), trap.pos_id), ("Error: cycle", 47));
    }
}

/// The count of unreachable objects that the step of rule 3 reads.
fn objects_read(ctx: &Context) -> usize {
    ctx.stats
        .completed_task_objects_read
        .load(Ordering::Relaxed)
}

/// Rule 8: a collection with no completed task that the mark did not
/// reach reads no object for this section. The collections have
/// unreachable described objects, a completed task that a root reaches,
/// and an unfinished task. Control: the `i1` shape reads its object.
#[test]
fn the_step_reads_nothing_when_each_completed_task_is_marked() {
    for mut ctx in contexts() {
        let unfinished = frame(&mut ctx);
        holder(&mut ctx, unfinished);
        let reached = frame(&mut ctx);
        let object = holder(&mut ctx, reached);
        complete(&mut ctx, reached, 0);
        let root = Box::into_raw(Box::new(object));
        // SAFETY: the slot stays allocated until the end of the test.
        unsafe { ctx.root_add(root as usize, 1) };
        ctx.collect();
        ctx.collect_automatic();
        assert!(ctx.is_live(object));
        assert_eq!(objects_read(&ctx), 0);

        let (task, _) = returns_its_holder(&mut ctx, 0);
        ctx.collect();
        assert!(!ctx.async_frames.contains_key(&(task as usize)));
        assert_eq!(objects_read(&ctx), 1);
        drop(ctx);
        // SAFETY: the Context that read the slot is dropped.
        drop(unsafe { Box::from_raw(root) });
    }
}

/// Rule 3 reads the counted elements of an unreachable array as the
/// release reads them: an element counts only when the release of the
/// array holders reaches zero. Two unreachable objects hold one counted
/// array of the handle, so the second release reaches zero. Control: a
/// third holder of the array that a root reaches keeps the elements, so
/// the task has fewer heap holders than its count and stays.
#[test]
fn an_array_element_counts_when_the_array_holders_end() {
    // One field at offset 0: an array (kind 2) of handles (kind 1).
    const ARRAY_FIELD: [u64; 9] = [1, 0, 48, 2, 0, 0, 1, 0, 0];
    for mut ctx in contexts() {
        for rooted in [false, true] {
            let task = frame(&mut ctx);
            let array = ctx.array_new(8, 0);
            // SAFETY: the array stores 8-byte elements.
            unsafe { ctx.array_push(array, (&task as *const *mut u8).cast(), 0) };
            // The first object takes the holder count of the new array, and
            // the second object adds one.
            let holders: Vec<usize> = (0..2)
                .map(|index| {
                    let object = ctx.alloc(8, 900, 0);
                    // SAFETY: one 8-byte field holds the array; the
                    // description states it.
                    unsafe {
                        object.cast::<*mut u8>().write(array);
                        if index == 1 {
                            ctx.array_holder(array, 0, 0);
                        }
                        ctx.describe_object(
                            object as usize,
                            std::slice::from_raw_parts(ARRAY_FIELD.as_ptr().cast(), 72),
                        );
                    }
                    object as usize
                })
                .collect();
            let root = Box::into_raw(Box::new(if rooted { array as usize } else { 0 }));
            if rooted {
                // SAFETY: the array is live; the root owns one more holder.
                unsafe { ctx.array_holder(array, 0, 0) };
            }
            // SAFETY: the slot is never freed, so it stays allocated until
            // the Context drops.
            unsafe { ctx.root_add(root as usize, 1) };
            let value = ctx.alloc(24, 901, 0) as usize;
            complete(&mut ctx, task, value);
            ctx.collect();
            assert!(!ctx.trapped());
            assert!(holders.iter().all(|object| !ctx.is_live(*object)));
            assert_eq!(ctx.async_frames.contains_key(&(task as usize)), rooted);
            assert_eq!(ctx.is_live(value), rooted);
            if rooted {
                assert_eq!(completion(&mut ctx, task), value);
            }
        }
    }
}

/// Rule 4: the sweep never frees a registered frame. A task that the plan
/// lists as ending but that is still registered after the release is kept,
/// and the Context traps with `Internal`. The test builds that state
/// directly: no release ran for the listed task. Control: a listed task
/// that the release ended changes nothing.
#[test]
fn a_registered_task_that_the_release_did_not_end_is_kept() {
    for mut ctx in contexts() {
        let (ended, _) = returns_its_holder(&mut ctx, 0);
        ctx.collect();
        assert!(!ctx.async_frames.contains_key(&(ended as usize)));
        ctx.keep_unended_tasks(&[ended as usize]);
        assert!(!ctx.trapped());

        let task = frame(&mut ctx);
        complete(&mut ctx, task, 0);
        ctx.keep_unended_tasks(&[task as usize]);
        assert!(ctx.collection_marked(task as usize));
        let trap = ctx.trap_record().expect("the Context traps");
        assert_eq!(trap.kind, TrapKind::Internal);
    }
    // A consumer that releases no handle leaves the task registered at the
    // sweep. The sweep keeps the frame and the object of its completion.
    for mut ctx in contexts() {
        let (task, object) = returns_its_holder(&mut ctx, 0);
        Context::with_collection(&mut *ctx, |ctx| ctx, |_, _| {});
        assert!(ctx.async_frames.contains_key(&(task as usize)));
        assert!(ctx.is_live(task as usize));
        assert!(ctx.is_live(object), "the completion keeps its object");
        let trap = ctx.trap_record().expect("the Context traps");
        assert_eq!(trap.kind, TrapKind::Internal);
    }
}

/// A Map value description: one handle node (`compiler.md` §172).
static MAP_HANDLE_VALUE: [u64; 3] = [1, 0, 0];

/// A counted Map of `i32` keys whose one value is `task`. The value owns
/// one count of the frame.
fn map_of(ctx: &mut Context, task: *mut u8) -> usize {
    let map = crate::assocops::new(ctx, 4, 8, crate::assocops::KeyKind::Bits, false, 0);
    let key = 1i32;
    // SAFETY: the Map is live and empty, the description is a static
    // handle node, and the key and value have the widths of the Map.
    unsafe {
        crate::assocops::describe(ctx, map, MAP_HANDLE_VALUE.as_ptr().cast());
        crate::assocops::insert(
            ctx,
            map,
            (&key as *const i32).cast(),
            (&task as *const *mut u8).cast(),
            0,
        );
    }
    map as usize
}

/// Rule 3 counts the values of an unreachable counted Map as heap holders.
/// The completion value of the task is a Map whose only value is the
/// handle of the task, so the Map is the only holder, and the task ends
/// (rule 4). Control: with one more count for an outside holder, the task
/// stays with its completion and the Map.
#[test]
fn a_map_value_counts_as_a_heap_holder() {
    for mut ctx in contexts() {
        for outside in [0, 1] {
            let task = frame(&mut ctx);
            let map = map_of(&mut ctx, task);
            complete(&mut ctx, task, map);
            for _ in 0..outside {
                // SAFETY: the frame is registered.
                unsafe { ctx.async_retain(task) };
            }
            ctx.collect();
            assert!(!ctx.trapped());
            let kept = outside != 0;
            assert_eq!(ctx.async_frames.contains_key(&(task as usize)), kept);
            assert_eq!(ctx.is_live(map), kept);
            if kept {
                assert_eq!(completion(&mut ctx, task), map);
            }
        }
    }
}

/// Rule 3 in one pass: the tasks are read in the order of their task ids.
/// The first task has an unreachable holder and an outside holder, so it
/// becomes a root. The mark continues from it before the collector reads
/// the second task. Its completion is `link`, an object that holds the
/// second task. The second task has two counts, `link` and `garbage`. Both
/// are unreachable before the pass, so its heap holders equal its count.
/// The mark from the first task reaches `link` and the second task, so the
/// second task stays. The release of `garbage` decreases its count to one.
/// Control: with no outside holder, both tasks end.
#[test]
fn an_earlier_root_of_the_pass_marks_a_holder_of_a_later_task() {
    for mut ctx in contexts() {
        for outside in [1, 0] {
            let first = frame(&mut ctx);
            let second = frame(&mut ctx);
            assert!(
                ctx.async_frames[&(first as usize)].task_id
                    < ctx.async_frames[&(second as usize)].task_id
            );
            let first_holder = holder(&mut ctx, first);
            let link = holder(&mut ctx, second);
            let garbage = holder(&mut ctx, second);
            // SAFETY: the frame is registered; `garbage` owns this count.
            unsafe { ctx.async_retain(second) };
            let value = ctx.alloc(24, 901, 0) as usize;
            complete(&mut ctx, second, value);
            complete(&mut ctx, first, link);
            for _ in 0..outside {
                // SAFETY: the frame is registered.
                unsafe { ctx.async_retain(first) };
            }
            ctx.collect();
            assert!(!ctx.trapped());
            assert!(!ctx.is_live(garbage) && !ctx.is_live(first_holder));
            let kept = outside != 0;
            for task in [first, second] {
                assert_eq!(ctx.async_frames.contains_key(&(task as usize)), kept);
            }
            assert_eq!(ctx.is_live(link), kept);
            assert_eq!(ctx.is_live(value), kept);
            if kept {
                // SAFETY: the frame stays registered.
                assert_eq!(unsafe { ctx.async_count(second) }, 1);
                assert_eq!(completion(&mut ctx, first), link);
                assert_eq!(completion(&mut ctx, second), value);
            }
        }
    }
}

/// [`Context::completed_task_holders`] counts the holders of a handle in
/// the objects and Maps that `reached` does not contain. The handle is a
/// key of a registry outside the Context, so it is not a registered frame.
/// Two objects and one Map hold it, and `reached` contains one object.
/// Controls: a handle that is not in `tasks` has no entry, and a reached
/// set with each holder gives no count.
#[test]
fn the_holder_count_reads_the_objects_outside_the_reached_set() {
    for mut ctx in contexts() {
        let key = ctx.alloc(8, 901, 0);
        let first = holder(&mut ctx, key);
        let second = holder(&mut ctx, key);
        let map = map_of(&mut ctx, key);
        let tasks: HashSet<usize> = [key as usize].into();
        let reached: HashSet<usize> = [second].into();
        let holders = ctx.completed_task_holders(&reached, &tasks);
        assert_eq!(holders.get(&(key as usize)), Some(&2));
        assert!(ctx
            .completed_task_holders(&reached, &HashSet::from([first]))
            .is_empty());
        let reached: HashSet<usize> = [first, second, map].into();
        assert!(ctx.completed_task_holders(&reached, &tasks).is_empty());
    }
}

/// [`Context::with_collection_after_task_failures`] marks its words after
/// the release plan is made. `error` holds `link`, and `link` holds a
/// handle. Both objects are unreachable before the late mark, so the plan
/// releases the handle of `link`, and the sweep keeps both objects.
/// Control: [`Context::with_collection`] releases the same handle and
/// frees both objects.
#[test]
fn a_late_mark_keeps_its_objects_after_the_release_plan() {
    for mut ctx in contexts() {
        for late in [true, false] {
            // A handle of a registry outside the Context: a word only.
            let key = 0x1000usize;
            let link = holder(&mut ctx, key as *mut u8);
            let error = ctx.alloc(8, 900, 0);
            // SAFETY: the object has one 8-byte word.
            unsafe { error.cast::<usize>().write(link) };
            let failures = [error as usize];
            let consume = |_: &mut Box<Context>, handles: &mut [usize]| handles.to_vec();
            let handles = if late {
                Context::with_collection_after_task_failures(
                    &mut ctx,
                    |ctx| &mut **ctx,
                    &failures,
                    consume,
                )
            } else {
                Context::with_collection(&mut ctx, |ctx| &mut **ctx, consume)
            };
            assert_eq!(handles, vec![key]);
            assert_eq!(ctx.is_live(error as usize), late);
            assert_eq!(ctx.is_live(link), late);
            assert!(!ctx.trapped());
        }
    }
}
