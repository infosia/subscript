//! The handle fact of an array (`compiler.md` §344). Cost: each test
//! builds a few Contexts; the largest arrays are 8 MB.

use super::*;
use crate::arrops::{self, ElemKind};
use std::sync::atomic::Ordering::Relaxed;

/// Both allocator tiers: the dev tier and the ship arena.
fn tiers() -> [Box<Context>; 2] {
    [Context::new(), Context::new_releasing()]
}

/// The class id of the element storage of a live array.
fn data_class(ctx: &Context, array: *mut u8) -> Option<u32> {
    // SAFETY: the tests pass live arrays of `ctx`.
    let data = unsafe { (*(array as *const ArrayHeader)).data };
    // SAFETY: a non-null storage pointer is a live payload with a header.
    (!data.is_null() && ctx.is_live(data as usize))
        .then(|| unsafe { header_class_id(data.sub(HEADER_SIZE)) })
}

/// The fact of `array` and the fact that its storage class states.
fn fact(ctx: &Context, array: *mut u8) -> ArrayElements {
    // SAFETY: the tests pass live arrays of `ctx`.
    let elements = unsafe { ctx.array_elements(array) };
    if let Some(class_id) = data_class(ctx, array) {
        let expected = match elements {
            ArrayElements::NoHandle => CLASS_ARRAY_DATA_NO_HANDLE,
            ArrayElements::CanHoldHandle => CLASS_ARRAY_DATA,
        };
        assert_eq!(
            class_id, expected,
            "the storage takes the fact of its header"
        );
    }
    elements
}

/// An array of `count` elements of `elem_size` bytes, each byte `0x5a`,
/// so that no element word is a payload address.
fn filled(ctx: &mut Context, count: usize, elem_size: usize, elements: ArrayElements) -> *mut u8 {
    let array = ctx.array_with_capacity_of(count, elem_size, elements, 0);
    assert!(!array.is_null());
    // SAFETY: the new array owns capacity for `count` elements.
    unsafe {
        let header = &mut *(array as *mut ArrayHeader);
        std::ptr::write_bytes(header.data, 0x5a, count * elem_size);
        header.len = count as u64;
    }
    array
}

/// The payload words that one collection reads, with `arrays` as roots.
fn words_read_by_one_collection(ctx: &mut Context, arrays: &[*mut u8]) -> usize {
    let mut roots: Vec<usize> = arrays.iter().map(|array| *array as usize).collect();
    // SAFETY: `roots` outlives the collection, and the shadow pop below
    // removes the range (section 263).
    unsafe { ctx.shadow_push(roots.as_mut_ptr() as usize, roots.len()) };
    let stats = ctx.test_arena_stats();
    let before = stats.words_read.load(Relaxed);
    ctx.collect();
    let words = stats.words_read.load(Relaxed) - before;
    ctx.shadow_pop();
    for array in arrays {
        assert!(ctx.is_live(*array as usize), "a rooted array survives");
    }
    words
}

/// Acceptance 2: with a live `i32[]` and a live `f64[]`, the words that one
/// collection reads do not grow with the array length. The control with
/// the same arrays that can hold a handle shows that the count sees the
/// storage.
#[test]
fn the_mark_reads_no_word_of_the_storage_of_a_scalar_array() {
    const MB: usize = 1 << 20;
    for mut ctx in tiers() {
        let ship = ctx.uses_ship_arena();
        let mut counts = Vec::new();
        for (elements, megabytes) in [
            (ArrayElements::NoHandle, 4),
            (ArrayElements::NoHandle, 8),
            (ArrayElements::CanHoldHandle, 4),
            (ArrayElements::CanHoldHandle, 8),
        ] {
            let bytes = megabytes * MB;
            let ints = filled(&mut ctx, bytes / 4, 4, elements);
            let floats = filled(&mut ctx, bytes / 8, 8, elements);
            let words = words_read_by_one_collection(&mut ctx, &[ints, floats]);
            counts.push(words);
            ctx.delete(ints as usize, 0);
            ctx.delete(floats as usize, 0);
            ctx.collect();
        }
        println!(
            "ship={ship} words read: no handle 4 MB={}, 8 MB={}; can hold 4 MB={}, 8 MB={}",
            counts[0], counts[1], counts[2], counts[3]
        );
        assert_eq!(
            counts[0], counts[1],
            "ship={ship}: the count grows with the length"
        );
        assert!(counts[0] < 64, "ship={ship}: {} words", counts[0]);
        assert!(
            counts[3] >= counts[2] + 2 * MB / 8,
            "ship={ship}: the control does not read the storage"
        );
    }
}

/// compiler.md §347 rules 1 and 2: a received `i32[]` takes its fact from
/// the read record. With the "no handle" flag (§344 rule 3), the words that
/// one collection reads do not grow with the length, and each element reads
/// the C buffer after the collection. The control without the flag reads a
/// count that grows with the length.
#[test]
fn the_mark_reads_no_word_of_the_storage_of_a_received_scalar_array() {
    use crate::ffi::subscript_rt_boundary_receive;
    const MB: usize = 1 << 20;
    // The `RECEIVED_NO_HANDLE` flag of `ffi/receive.rs`.
    const NO_HANDLE_FLAG: u32 = 512;
    let record = |flags: u32| -> Vec<u8> {
        [flags, 4, 4, 0]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect()
    };
    let buffer: Vec<i32> = (0..(8 * MB / 4) as i32).map(|i| i ^ 0x5a5a).collect();
    let path = b"host.Out.items";
    for mut ctx in tiers() {
        let ship = ctx.uses_ship_arena();
        let mut counts = Vec::new();
        for (flags, megabytes) in [(NO_HANDLE_FLAG, 4), (NO_HANDLE_FLAG, 8), (0, 4), (0, 8)] {
            let count = megabytes * MB / 4;
            let schema = record(flags);
            // SAFETY: `buffer` holds at least `count` elements, and `schema`
            // is a complete record of a scalar element of 4 bytes.
            let array = unsafe {
                subscript_rt_boundary_receive(
                    &mut *ctx,
                    buffer.as_ptr().cast(),
                    count as u64,
                    schema.as_ptr(),
                    path.as_ptr(),
                    path.len() as u32,
                    0,
                    std::ptr::null(),
                )
            };
            assert!(!array.is_null(), "ship={ship}: the receive fails");
            counts.push(words_read_by_one_collection(&mut ctx, &[array]));
            let expected = if flags == NO_HANDLE_FLAG {
                ArrayElements::NoHandle
            } else {
                ArrayElements::CanHoldHandle
            };
            assert_eq!(fact(&ctx, array), expected, "ship={ship}: the fact");
            // SAFETY: `array` is a live array of `count` 4-byte elements, and
            // no allocation runs while the slice lives.
            let received =
                unsafe { std::slice::from_raw_parts(ctx.array_data(array).cast::<i32>(), count) };
            assert!(
                received == &buffer[..count],
                "ship={ship}: the elements differ from the C buffer"
            );
            ctx.delete(array as usize, 0);
            ctx.collect();
        }
        println!(
            "ship={ship} received words read: no handle 4 MB={}, 8 MB={}; can hold 4 MB={}, 8 MB={}",
            counts[0], counts[1], counts[2], counts[3]
        );
        assert_eq!(
            counts[0], counts[1],
            "ship={ship}: the count grows with the length"
        );
        assert!(counts[0] < 64, "ship={ship}: {} words", counts[0]);
        assert!(
            counts[3] >= counts[2] + 2 * MB / 8,
            "ship={ship}: the control does not read the storage"
        );
    }
}

/// Acceptance 1, the firing control: an array of reference objects that
/// states "no handle" loses its elements in a collection. The same array
/// that can hold a handle keeps them.
#[test]
fn an_array_of_references_with_the_wrong_fact_loses_its_elements() {
    for mut ctx in tiers() {
        let ship = ctx.uses_ship_arena();
        for (elements, survives) in [
            (ArrayElements::NoHandle, false),
            (ArrayElements::CanHoldHandle, true),
        ] {
            let array = ctx.array_new_of(8, elements, 0);
            let object = ctx.alloc(16, 1, 0);
            // SAFETY: `array` is live and `object` is one 8-byte element.
            unsafe {
                ctx.array_push(array, (&object as *const *mut u8).cast(), 0);
            }
            let _ = words_read_by_one_collection(&mut ctx, &[array]);
            assert_eq!(
                ctx.is_live(object as usize),
                survives,
                "ship={ship}, {elements:?}"
            );
        }
    }
}

extern "C" fn keep_odd(_ctx: *mut Context, _env: *const u8, value: i32) -> u8 {
    u8::from(value % 2 != 0)
}

extern "C" fn keep_all(_ctx: *mut Context, _env: *const u8, _value: u64) -> u8 {
    1
}

extern "C" fn half(_ctx: *mut Context, _env: *const u8, value: i32) -> f64 {
    f64::from(value) / 2.0
}

extern "C" fn same(_ctx: *mut Context, _env: *const u8, value: u64) -> u64 {
    value
}

extern "C" fn pair(ctx: *mut Context, _env: *const u8, value: i32) -> *mut u8 {
    // SAFETY: the runtime passes its live Context to the callback.
    let ctx = unsafe { &mut *ctx };
    let out = ctx.array_new_of(4, ArrayElements::NoHandle, 0);
    for item in [value, value + 1] {
        // SAFETY: `out` is a live array of 4-byte elements.
        unsafe { ctx.array_push(out, (&item as *const i32).cast(), 0) };
    }
    out
}

extern "C" fn width(_ctx: *mut Context, _env: *const u8, value: u64) -> i32 {
    (value & 0xff) as i32
}

/// Acceptance 3 in the runtime: growth, `slice`, `concat`, `splice`,
/// `filter`, and `map` give the fact of rule 4, for an array of numbers and
/// an array of reference objects. `split` and a byte array give the fact of
/// rule 5.
#[test]
fn each_runtime_operation_gives_the_fact_of_its_elements() {
    use ArrayElements::{CanHoldHandle, NoHandle};
    for mut ctx in tiers() {
        let ship = ctx.uses_ship_arena();
        let numbers = ctx.array_new_of(4, NoHandle, 0);
        let references = ctx.array_new_of(8, CanHoldHandle, 0);
        for index in 0..20i32 {
            let object = ctx.alloc(16, 1, 0);
            // SAFETY: both arrays are live; each source is one element.
            unsafe {
                ctx.array_push(numbers, (&index as *const i32).cast(), 0);
                ctx.array_push(references, (&object as *const *mut u8).cast(), 0);
            }
        }
        // Growth went from no storage to a capacity of 32: the fact holds.
        assert_eq!(fact(&ctx, numbers), NoHandle, "ship={ship}: growth");
        assert_eq!(fact(&ctx, references), CanHoldHandle, "ship={ship}: growth");
        let p: *mut Context = &mut *ctx;
        for (source, expected) in [(numbers, NoHandle), (references, CanHoldHandle)] {
            // SAFETY: live arrays of `ctx` and callbacks of the element ABI.
            unsafe {
                let sliced = arrops::slice(p, source, 2, 9, 0);
                assert_eq!(fact(&ctx, sliced), expected, "ship={ship}: slice");
                let joined = arrops::concat(p, source, sliced, 0);
                assert_eq!(fact(&ctx, joined), expected, "ship={ship}: concat");
                let removed = arrops::splice(p, joined, 1, 3, 0);
                assert_eq!(fact(&ctx, removed), expected, "ship={ship}: splice");
                let filtered = if expected == NoHandle {
                    arrops::filter(
                        p,
                        source,
                        keep_odd as *const u8,
                        std::ptr::null(),
                        ElemKind::SignedInt,
                        0,
                        false,
                    )
                } else {
                    arrops::filter(
                        p,
                        source,
                        keep_all as *const u8,
                        std::ptr::null(),
                        ElemKind::Int,
                        0,
                        false,
                    )
                };
                assert_eq!(fact(&ctx, filtered), expected, "ship={ship}: filter");
            }
        }
        // SAFETY: live arrays and callbacks of the element and result ABIs.
        unsafe {
            let to_number = arrops::map(
                p,
                numbers,
                half as *const u8,
                std::ptr::null(),
                ElemKind::SignedInt,
                ElemKind::F64,
                8,
                0,
                false,
                ArrayElements::NoHandle,
            );
            assert_eq!(fact(&ctx, to_number), NoHandle, "ship={ship}: map to f64");
            let from_references = arrops::map(
                p,
                references,
                width as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                ElemKind::SignedInt,
                4,
                0,
                false,
                ArrayElements::NoHandle,
            );
            assert_eq!(
                fact(&ctx, from_references),
                NoHandle,
                "ship={ship}: map to i32"
            );
            let to_reference = arrops::map(
                p,
                references,
                same as *const u8,
                std::ptr::null(),
                ElemKind::Int,
                ElemKind::Int,
                8,
                0,
                false,
                ArrayElements::CanHoldHandle,
            );
            assert_eq!(
                fact(&ctx, to_reference),
                CanHoldHandle,
                "ship={ship}: map to a reference"
            );
        }
        // SAFETY: a live array and a callback of the element ABI that returns
        // an array of 4-byte elements.
        unsafe {
            for fact_given in [NoHandle, CanHoldHandle] {
                let flat = arrops::flat_map(
                    p,
                    numbers,
                    (pair as *const u8, std::ptr::null()),
                    ElemKind::SignedInt,
                    4,
                    0,
                    false,
                    fact_given,
                );
                assert_eq!(ctx.array_len(flat), 40, "ship={ship}: flatMap");
                assert_eq!(fact(&ctx, flat), fact_given, "ship={ship}: flatMap");
            }
        }
        // Rule 5: an array of strings can hold a handle; a byte array holds none.
        let text = ctx.alloc_str(b"a,b,c", 0);
        let comma = ctx.alloc_str(b",", 0);
        // SAFETY: live string handles of `ctx`.
        let pieces = unsafe { crate::ffi::subscript_rt_str_split(p, text, comma, -1, 0) };
        assert_eq!(fact(&ctx, pieces), CanHoldHandle, "ship={ship}: split");
        let source = [1u8, 2, 3];
        // SAFETY: `source` is readable for its length.
        let bytes = unsafe { ctx.array_from_bytes(source.as_ptr(), source.len(), 0) };
        assert_eq!(fact(&ctx, bytes), NoHandle, "ship={ship}: byte array");
    }
}

/// compiler.md §344 rule 2: a host sees the array classes for both facts.
#[test]
fn a_host_sees_the_array_classes_for_both_facts() {
    unsafe extern "C" fn record(
        userdata: *mut c_void,
        class_id: u32,
        _pos_id: u32,
        _payload_bytes: u64,
    ) {
        // SAFETY: the test passes a live Vec of this type.
        unsafe { (*userdata.cast::<Vec<u32>>()).push(class_id) };
    }
    for mut ctx in tiers() {
        let array = ctx.array_with_capacity_of(4, 4, ArrayElements::NoHandle, 0);
        let other = ctx.array_new(8, 0);
        let mut facts = ctx.live_array_facts();
        facts.sort_unstable_by_key(|(_, elements)| *elements == ArrayElements::NoHandle);
        assert_eq!(
            facts,
            vec![
                (0, ArrayElements::CanHoldHandle),
                (0, ArrayElements::NoHandle)
            ]
        );
        ctx.delete(other as usize, 0);
        let _ = array;
        let mut classes = Vec::new();
        // SAFETY: `record` accepts the live Vec as userdata.
        unsafe { ctx.visit_live_allocations(Some(record), (&mut classes as *mut Vec<u32>).cast()) };
        classes.sort_unstable();
        assert_eq!(classes, vec![CLASS_ARRAY, CLASS_ARRAY_DATA]);
        assert!(is_array_class(CLASS_ARRAY_NO_HANDLE) && is_array_class(CLASS_ARRAY));
        assert!(is_array_data_class(CLASS_ARRAY_DATA_NO_HANDLE));
        assert!(!is_array_class(CLASS_ARRAY_DATA_NO_HANDLE));
        assert!(class_holds_no_handle(CLASS_ARRAY_DATA_NO_HANDLE));
        assert!(!class_holds_no_handle(CLASS_ARRAY_NO_HANDLE));
        assert!(!class_holds_no_handle(CLASS_ARRAY_DATA));
    }
}

/// compiler.md §344 rule 3: the two creation entries for each fact. The
/// `_no_handle` entries make an array that holds no handle; the plain
/// entries make an array that can hold one.
#[test]
fn each_creation_entry_gives_its_fact() {
    use crate::ffi::{
        subscript_rt_array_new, subscript_rt_array_new_no_handle, subscript_rt_array_with_capacity,
        subscript_rt_array_with_capacity_no_handle,
    };
    for mut ctx in tiers() {
        let p: *mut Context = &mut *ctx;
        // SAFETY: a live Context and representable sizes.
        let (plain, no_handle, plain_capacity, no_handle_capacity) = unsafe {
            (
                subscript_rt_array_new(p, 8, 0),
                subscript_rt_array_new_no_handle(p, 8, 0),
                subscript_rt_array_with_capacity(p, 4, 8, 0),
                subscript_rt_array_with_capacity_no_handle(p, 4, 8, 0),
            )
        };
        assert_eq!(fact(&ctx, plain), ArrayElements::CanHoldHandle);
        assert_eq!(fact(&ctx, no_handle), ArrayElements::NoHandle);
        assert_eq!(fact(&ctx, plain_capacity), ArrayElements::CanHoldHandle);
        assert_eq!(fact(&ctx, no_handle_capacity), ArrayElements::NoHandle);
        assert_eq!(data_class(&ctx, plain_capacity), Some(CLASS_ARRAY_DATA));
        assert_eq!(
            data_class(&ctx, no_handle_capacity),
            Some(CLASS_ARRAY_DATA_NO_HANDLE)
        );
    }
}

/// `ArrayElements::and` gives "no handle" only when both inputs hold no
/// handle. `concat` uses it on two arrays of one static element type. Each
/// creation site gives the fact of the predicate over that type, so the two
/// facts of such a pair are equal in a program; the runtime API builds the
/// mixed pair here to show that the result takes the safe fact.
#[test]
fn and_gives_no_handle_only_for_two_arrays_without_a_handle() {
    use ArrayElements::{CanHoldHandle, NoHandle};
    assert_eq!(NoHandle.and(NoHandle), NoHandle);
    assert_eq!(NoHandle.and(CanHoldHandle), CanHoldHandle);
    assert_eq!(CanHoldHandle.and(NoHandle), CanHoldHandle);
    assert_eq!(CanHoldHandle.and(CanHoldHandle), CanHoldHandle);
    for mut ctx in tiers() {
        let a = filled(&mut ctx, 3, 8, NoHandle);
        let b = filled(&mut ctx, 2, 8, CanHoldHandle);
        let p: *mut Context = &mut *ctx;
        // SAFETY: two live arrays of `ctx` with equal element sizes.
        let (mixed, reversed, same) = unsafe {
            (
                arrops::concat(p, a, b, 0),
                arrops::concat(p, b, a, 0),
                arrops::concat(p, a, a, 0),
            )
        };
        assert_eq!(fact(&ctx, mixed), CanHoldHandle);
        assert_eq!(fact(&ctx, reversed), CanHoldHandle);
        assert_eq!(fact(&ctx, same), NoHandle);
    }
}

/// compiler.md §344 rule 2: a mark-trace record names the class id that a
/// host sees, and states the fact of an array class that holds no handle.
#[test]
fn a_mark_trace_record_names_the_reported_class_and_the_fact() {
    assert_eq!(
        trace_class(CLASS_STRING),
        format!("class_id={CLASS_STRING}")
    );
    assert_eq!(trace_class(CLASS_ARRAY), format!("class_id={CLASS_ARRAY}"));
    assert_eq!(
        trace_class(CLASS_ARRAY_NO_HANDLE),
        format!("class_id={CLASS_ARRAY} elements=no_handle")
    );
    assert_eq!(
        trace_class(CLASS_ARRAY_DATA_NO_HANDLE),
        format!("class_id={CLASS_ARRAY_DATA} elements=no_handle")
    );
}
