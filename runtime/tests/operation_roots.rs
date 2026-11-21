//! A runtime operation keeps what it reads after a callback
//! (`compiler.md` §345 rule 1, acceptance 3).
//!
//! Each callback replaces every element of the receiver with a fresh
//! allocation and collects, so only the root of the operation holds the
//! element that it passed. A callback that raises ends the operation, and
//! the operation leaves no shadow frame. The depth that each callback reads
//! is the control: it shows that the operation holds a frame while the
//! callback runs. Cost: one Context per case, no compile.

use std::cell::Cell;
use std::ptr;

use subscript_runtime::arrops::{self, ElemKind};
use subscript_runtime::{ffi, Context};

/// The class id of the test objects (a plain reference class).
const CLASS: u32 = 1;

/// What a callback reads and changes.
struct State {
    array: *mut u8,
    raise: bool,
    depth: Cell<usize>,
    calls: Cell<usize>,
}

/// Replaces each element of the receiver with a fresh object, collects, and
/// records the shadow depth. With `raise`, raises an exception instead.
///
/// # Safety
///
/// `ctx` is live and `state` is a live [`State`] whose array holds words.
unsafe fn visit(ctx: *mut Context, state: *const u8) {
    // SAFETY: the caller passes a live State.
    let state = unsafe { &*state.cast::<State>() };
    // SAFETY: `ctx` is the live test Context.
    let context = unsafe { &mut *ctx };
    state.depth.set(context.shadow_depth());
    state.calls.set(state.calls.get() + 1);
    if state.raise {
        let error = context.alloc(16, CLASS, 0);
        // SAFETY: `error` is a live object of the Context.
        unsafe { context.raise_exception(error, "raised".to_owned(), 7) };
        return;
    }
    // SAFETY: the array is live and rooted by the test.
    let len = unsafe { context.array_len(state.array) };
    for index in 0..len {
        let fresh = context.alloc(16, CLASS, 0) as usize;
        // SAFETY: `index` is in bounds of a word-element array.
        unsafe {
            context
                .array_elem_ptr(state.array, index, 0)
                .cast::<usize>()
                .write_unaligned(fresh)
        };
    }
    context.collect();
}

unsafe extern "C" fn keep_all(ctx: *mut Context, state: *const u8, _value: usize) -> u8 {
    // SAFETY: test contract.
    unsafe { visit(ctx, state) };
    1
}

unsafe extern "C" fn compare(ctx: *mut Context, state: *const u8, a: usize, b: usize) -> i32 {
    // SAFETY: test contract.
    unsafe { visit(ctx, state) };
    // SAFETY: each argument is a live 16-byte object whose first word is its rank.
    let (a, b) = unsafe { (*(a as *const u64), *(b as *const u64)) };
    a.cmp(&b) as i32
}

unsafe extern "C" fn group_bridge(
    ctx: *mut Context,
    _code: *const u8,
    state: *const u8,
    _element: *const u8,
    key: *mut u8,
    _index: i32,
) {
    // SAFETY: test contract.
    unsafe { visit(ctx, state) };
    // SAFETY: the runtime passes an eight-byte key slot.
    unsafe { key.cast::<u64>().write_unaligned(0) };
}

/// A Context with a rooted array of `count` objects whose first word is
/// a rank in descending order. Returns the Context, the root slot, and the
/// objects.
fn fixture(count: u64) -> (Box<Context>, Box<[usize; 1]>, Vec<usize>) {
    let mut ctx = Context::new();
    let array = ctx.array_new(size_of::<usize>(), 0);
    let mut objects = Vec::new();
    for rank in (0..count).rev() {
        let object = ctx.alloc(16, CLASS, 0);
        // SAFETY: a live 16-byte object.
        unsafe { object.cast::<u64>().write(rank) };
        let word = object as usize;
        // SAFETY: a live word-element array and a readable word.
        unsafe { ctx.array_push(array, (&raw const word).cast(), 0) };
        objects.push(word);
    }
    let root = Box::new([array as usize]);
    // SAFETY: the boxed slot lives as long as the Context in each test.
    unsafe { ctx.shadow_push(root.as_ptr() as usize, 1) };
    (ctx, root, objects)
}

fn state(array: usize, raise: bool) -> State {
    State {
        array: array as *mut u8,
        raise,
        depth: Cell::new(0),
        calls: Cell::new(0),
    }
}

/// The operations under test. Each runs one callback form over the array.
#[derive(Clone, Copy, Debug)]
enum Operation {
    Filter,
    Find,
    FindLast,
    Sort,
    GroupBy,
}

/// Runs `operation`; returns the handle it returns (null for `sort`).
///
/// # Safety
///
/// `ctx` is live; `state` names its rooted array.
unsafe fn run(ctx: *mut Context, operation: Operation, state: &State) -> *mut u8 {
    let env = (state as *const State).cast::<u8>();
    let array = state.array;
    // SAFETY: each callback matches the word ABI of its operation.
    unsafe {
        match operation {
            Operation::Filter => arrops::filter(
                ctx,
                array,
                keep_all as *const u8,
                env,
                ElemKind::Int,
                0,
                false,
            ),
            Operation::Find | Operation::FindLast => arrops::find(
                ctx,
                array,
                keep_all as *const u8,
                env,
                ElemKind::Int,
                false,
                matches!(operation, Operation::FindLast),
            ),
            Operation::Sort => {
                arrops::sort(ctx, array, compare as *const u8, env, ElemKind::Int);
                ptr::null_mut()
            }
            Operation::GroupBy => ffi::subscript_rt_map_group_by(
                ctx,
                array,
                group_bridge as *const u8,
                env,
                group_bridge as *const u8,
                8,
                0,
                0,
            ),
        }
    }
}

/// compiler.md §345 rule 1: each element that the operation reads after a
/// callback survives the collection in the callback, although the callback
/// replaced it in the receiver.
#[test]
fn an_operation_keeps_the_element_that_its_callback_replaces() {
    for operation in [
        Operation::Filter,
        Operation::Find,
        Operation::FindLast,
        Operation::Sort,
        Operation::GroupBy,
    ] {
        let (mut ctx, root, objects) = fixture(4);
        let state = state(root[0], false);
        let p: *mut Context = &mut *ctx;
        // SAFETY: `p` is live and the array is rooted.
        let result = unsafe { run(p, operation, &state) };
        assert!(!ctx.trapped(), "{operation:?}");
        // The first callback replaces every element, so the element that it
        // passed (the first one, or the last for findLast) has no other holder.
        let passed = match operation {
            Operation::FindLast => objects[3],
            _ => objects[0],
        };
        assert!(ctx.is_live(passed), "{operation:?}: {passed:#x}");
        if matches!(operation, Operation::Find | Operation::FindLast) {
            assert_eq!(result as usize, passed, "{operation:?}");
        }
        if matches!(operation, Operation::Sort) {
            // The sort writes back the original objects in ascending rank.
            // SAFETY: a live word-element array.
            let sorted: Vec<usize> = (0..4)
                .map(|i| unsafe {
                    ctx.array_elem_ptr(state.array, i, 0)
                        .cast::<usize>()
                        .read_unaligned()
                })
                .collect();
            let mut expected = objects.clone();
            expected.reverse();
            assert_eq!(sorted, expected);
            for object in &objects {
                assert!(ctx.is_live(*object), "sort: {object:#x}");
            }
        }
        assert_eq!(ctx.shadow_depth(), 1, "{operation:?}");
        ctx.shadow_pop();
        drop(root);
    }
}

/// compiler.md §345 rule 1 and acceptance 3: a callback that raises ends
/// the operation, and the operation leaves no frame. Control: the depth
/// that the callback reads is above the depth before the operation.
#[test]
fn a_raising_callback_leaves_no_frame_of_the_operation() {
    for operation in [
        Operation::Filter,
        Operation::Find,
        Operation::FindLast,
        Operation::Sort,
        Operation::GroupBy,
    ] {
        let (mut ctx, root, _objects) = fixture(4);
        let before = ctx.shadow_depth();
        let state = state(root[0], true);
        let p: *mut Context = &mut *ctx;
        // SAFETY: `p` is live and the array is rooted.
        unsafe { run(p, operation, &state) };
        assert!(ctx.trapped(), "{operation:?}");
        assert_eq!(state.calls.get(), 1, "{operation:?}");
        assert!(state.depth.get() > before, "{operation:?}: control");
        assert_eq!(ctx.shadow_depth(), before, "{operation:?}");
        ctx.shadow_pop();
        drop(root);
    }
}
