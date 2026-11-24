//! Received C elements and replaced pointers (compiler.md §325).
use super::strings::alloc_str_from_view;
use crate::{context::Context, trap::TrapKind};

/// The flag of a received-element record whose element holds no handle
/// (compiler.md §344 rule 3). The compiler writes the same bit.
const RECEIVED_NO_HANDLE: u32 = 512;

pub(super) unsafe fn word(record: *const u8, offset: usize) -> u32 {
    // SAFETY: compiler records contain complete little-endian words.
    u32::from_le(unsafe { record.add(offset).cast::<u32>().read_unaligned() })
}
pub(super) unsafe fn address(record: *const u8, offset: usize) -> *const u8 {
    // SAFETY: C pointer slots have pointer width and can be unaligned.
    unsafe { record.add(offset).cast::<*const u8>().read_unaligned() }
}
pub(super) fn fault(ctx: &mut Context, path: &str, reason: &str, pos: u32) {
    ctx.trap(
        TrapKind::ForeignPointerReplaced,
        format!("{path}: {reason}"),
        pos,
    );
}
pub(super) unsafe fn label(data: *const u8, length: u32) -> String {
    // SAFETY: the caller supplies a complete UTF-8 module literal.
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(data, length as usize) })
        .into_owned()
}

/// Checks a changed C pointer against the member's nullability (compiler.md §325 rule 4).
///
/// # Safety
/// Shared FFI contract; `path` contains `length` UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_pointer(
    ctx: *mut Context,
    value: *const u8,
    nullable: u32,
    path: *const u8,
    length: u32,
    pos: u32,
) {
    if value.is_null() && nullable != 0 {
        return;
    }
    let path = unsafe { label(path, length) };
    fault(
        unsafe { &mut *ctx },
        &path,
        "C replaced a pointer that the script cannot receive",
        pos,
    );
}

#[derive(Clone, Copy)]
struct Earlier {
    data: *const u8,
    count: u64,
    array: *const u8,
}

// Path pieces live on the read stack. Only a fault allocates their text.
enum Path<'a> {
    Root(&'a [u8]),
    Field(&'a Path<'a>, &'a [u8]),
    Index(&'a Path<'a>, usize),
}
impl Path<'_> {
    fn append(&self, out: &mut String) {
        use std::fmt::Write;
        match self {
            Self::Root(bytes) => out.push_str(&String::from_utf8_lossy(bytes)),
            Self::Field(parent, bytes) => {
                parent.append(out);
                out.push('.');
                out.push_str(&String::from_utf8_lossy(bytes));
            }
            Self::Index(parent, index) => {
                parent.append(out);
                let _ = write!(out, "[{index}]");
            }
        }
    }
    fn text(&self) -> String {
        let mut out = String::new();
        self.append(&mut out);
        out
    }
}

/// Copies C elements into a new script array by the compiler's read record.
///
/// # Safety
/// Shared FFI contract; `schema` is a complete compiler record. A non-null
/// `data` points to `count` C elements. `path` contains `length` UTF-8 bytes.
/// A non-null `earlier` holds the original C pointer, count, and script array.
/// The caller must finish the original element watches before this call.
/// Original scratch elements are readonly. Their pointer fields retain the input script values.
#[no_mangle]
pub unsafe extern "C" fn subscript_rt_boundary_receive(
    ctx: *mut Context,
    data: *const u8,
    count: u64,
    schema: *const u8,
    path: *const u8,
    length: u32,
    pos: u32,
    earlier: *const u64,
) -> *mut u8 {
    let path = Path::Root(unsafe { std::slice::from_raw_parts(path, length as usize) });
    let earlier = if earlier.is_null() {
        None
    } else {
        Some(Earlier {
            data: unsafe { earlier.read() } as *const u8,
            count: unsafe { earlier.add(1).read() },
            array: unsafe { earlier.add(2).read() } as *const u8,
        })
    };
    unsafe { receive(&mut *ctx, data, count, schema, &path, pos, earlier) }
}

unsafe fn receive(
    ctx: &mut Context,
    data: *const u8,
    count: u64,
    schema: *const u8,
    path: &Path<'_>,
    pos: u32,
    earlier: Option<Earlier>,
) -> *mut u8 {
    if count > i32::MAX as u64 || (data.is_null() && count != 0) {
        fault(
            ctx,
            &path.text(),
            "C returned an invalid pointer/count pair",
            pos,
        );
        return std::ptr::null_mut();
    }
    let earlier = earlier.filter(|earlier| earlier.data == data);
    if earlier.is_some_and(|earlier| count > earlier.count) {
        fault(
            ctx,
            &path.text(),
            "C returned an invalid pointer/count pair",
            pos,
        );
        return std::ptr::null_mut();
    }
    let c_stride = unsafe { word(schema, 4) } as usize;
    let script_stride = unsafe { word(schema, 8) } as usize;
    let prior = earlier.map_or(std::ptr::null(), |earlier| {
        if unsafe { word(schema, 0) } & 256 != 0 {
            // Readonly scratch elements retain their input script slots after their C span.
            unsafe { earlier.data.add(earlier.count as usize * c_stride) }
        } else if earlier.array.is_null() {
            std::ptr::null()
        } else {
            unsafe { ctx.array_data(earlier.array) }
        }
    });
    // compiler.md §344 rule 3: the read record states the handle fact of
    // its element type.
    let elements = crate::context::ArrayElements::from_no_handle(
        unsafe { word(schema, 0) } & RECEIVED_NO_HANDLE != 0,
    );
    let array = ctx.array_received(count as usize, script_stride, elements, pos);
    if array.is_null() {
        return array;
    }
    let target = unsafe { ctx.array_data(array) } as *mut u8;
    for index in 0..count as usize {
        // SAFETY: the C span and the fresh script array contain these elements.
        let ok = unsafe {
            read(
                ctx,
                data.add(index * c_stride),
                target.add(index * script_stride),
                schema,
                &Path::Index(path, index),
                pos,
                if prior.is_null() {
                    prior
                } else {
                    prior.add(index * script_stride)
                },
            )
        };
        if !ok {
            return std::ptr::null_mut();
        }
    }
    // compiler.md §356 rule 14: a retained input slot copies a reference
    // into the new storage, which has the mark and was never read, so the
    // storage goes to the work list.
    // SAFETY: `array` is the live array that this call allocated.
    unsafe { ctx.mark_new_elements(array) };
    array
}

unsafe fn read(
    ctx: &mut Context,
    source: *const u8,
    target: *mut u8,
    schema: *const u8,
    path: &Path<'_>,
    pos: u32,
    prior: *const u8,
) -> bool {
    let tag = unsafe { word(schema, 0) } & 255;
    let size = unsafe { word(schema, 8) } as usize;
    let payload = unsafe { schema.add(16) };
    match tag {
        0 => unsafe { std::ptr::copy_nonoverlapping(source, target, size) },
        1 => {
            let data = unsafe { address(source, 0) };
            let length = unsafe { source.add(8).cast::<u64>().read_unaligned() };
            let string = unsafe { alloc_str_from_view(ctx, data, length, pos) };
            if string.is_null() {
                return false;
            }
            unsafe { target.cast::<*mut u8>().write_unaligned(string) };
        }
        2 => {
            let value = unsafe { address(source, 0) };
            let nullable = unsafe { word(payload, 0) } != 0;
            // compiler.md §325 rule 2: the caller checks writable input slots before receive.
            // Readonly scratch slots keep their script representation, including callback closures.
            if !prior.is_null() && !value.is_null() {
                unsafe { std::ptr::copy_nonoverlapping(prior, target, size) };
                return true;
            }
            if !value.is_null() || !nullable {
                fault(
                    ctx,
                    &path.text(),
                    "C replaced a pointer that the script cannot receive",
                    pos,
                );
                return false;
            }
            unsafe { std::ptr::write_bytes(target, 0, size) };
        }
        3 => {
            let count = unsafe { word(payload, 0) };
            let mut field = unsafe { payload.add(4) };
            for _ in 0..count {
                let c_offset = unsafe { word(field, 0) } as usize;
                let script_offset = unsafe { word(field, 4) } as usize;
                let length = unsafe { word(field, 8) };
                let name = unsafe { std::slice::from_raw_parts(field.add(12), length as usize) };
                let child = unsafe { field.add(12 + length as usize) };
                if !unsafe {
                    read(
                        ctx,
                        source.add(c_offset),
                        target.add(script_offset),
                        child,
                        &Path::Field(path, name),
                        pos,
                        if prior.is_null() {
                            prior
                        } else {
                            prior.add(script_offset)
                        },
                    )
                } {
                    return false;
                }
                let payload_size = unsafe { word(child, 12) } as usize;
                field = unsafe { child.add(16 + payload_size) };
            }
        }
        4 => {
            let first = unsafe { word(payload, 0) } != 0;
            let pointer_offset = if first { 0 } else { 8 };
            let count_offset = if first { 8 } else { 0 };
            let data = unsafe { address(source, pointer_offset) };
            let count = unsafe { source.add(count_offset).cast::<u64>().read_unaligned() };
            let earlier = if prior.is_null() {
                None
            } else {
                // A pair-containing input element is a readonly scratch struct (§187 rule 11).
                let array = unsafe { address(prior, 0) };
                Some(Earlier { data, count, array })
            };
            let array = unsafe { receive(ctx, data, count, payload.add(4), path, pos, earlier) };
            if array.is_null() {
                return false;
            }
            unsafe { target.cast::<*mut u8>().write_unaligned(array) };
        }
        5 => {
            let count = unsafe { word(payload, 0) } as usize;
            let child = unsafe { payload.add(4) };
            let c_stride = unsafe { word(child, 4) } as usize;
            let script_stride = unsafe { word(child, 8) } as usize;
            for index in 0..count {
                if !unsafe {
                    read(
                        ctx,
                        source.add(index * c_stride),
                        target.add(index * script_stride),
                        child,
                        &Path::Index(path, index),
                        pos,
                        if prior.is_null() {
                            prior
                        } else {
                            prior.add(index * script_stride)
                        },
                    )
                } {
                    return false;
                }
            }
        }
        6 => {
            let value = unsafe { source.cast::<i32>().read_unaligned() };
            let count = unsafe { word(payload, 0) } as usize;
            let valid =
                (0..count).any(|index| unsafe { word(payload, 4 + index * 4) } as i32 == value);
            if !valid {
                let length = unsafe { word(payload, 4 + count * 4) };
                let name = unsafe { label(payload.add(8 + count * 4), length) };
                ctx.trap(
                    TrapKind::WireEnumUnknownValue,
                    format!("unknown wire value {value} for CEnum alias `{name}`"),
                    pos,
                );
                return false;
            }
            unsafe { target.cast::<i32>().write_unaligned(value) };
        }
        _ => {
            ctx.trap(TrapKind::Internal, "invalid received-element record", pos);
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }
    #[test]
    fn received_scalar_elements_are_copies_and_bad_pairs_trap() {
        let mut ctx = Context::new();
        let schema = record(&[0, 4, 4, 0]);
        let mut values = [11i32, 22, 33];
        let path = b"host.Out.items";
        let array = unsafe {
            subscript_rt_boundary_receive(
                &mut *ctx,
                values.as_ptr().cast(),
                3,
                schema.as_ptr(),
                path.as_ptr(),
                path.len() as u32,
                9,
                std::ptr::null(),
            )
        };
        assert_eq!(unsafe { ctx.array_len(array) }, 3);
        values[0] = 99;
        assert_eq!(values[0], 99);
        assert_eq!(unsafe { ctx.array_data(array).cast::<i32>().read() }, 11);
        for count in [1, i32::MAX as u64 + 1] {
            let array = unsafe {
                subscript_rt_boundary_receive(
                    &mut *ctx,
                    std::ptr::null(),
                    count,
                    schema.as_ptr(),
                    path.as_ptr(),
                    path.len() as u32,
                    9,
                    std::ptr::null(),
                )
            };
            assert!(array.is_null());
            let trap = ctx.trap_record().expect("bad pair trap");
            assert_eq!(trap.kind, TrapKind::ForeignPointerReplaced);
            assert_eq!(trap.pos_id, 9);
            assert_eq!(
                trap.message,
                "host.Out.items: C returned an invalid pointer/count pair"
            );
            ctx.clear_trap();
        }
        let empty = unsafe {
            subscript_rt_boundary_receive(
                &mut *ctx,
                std::ptr::null(),
                0,
                schema.as_ptr(),
                path.as_ptr(),
                path.len() as u32,
                9,
                std::ptr::null(),
            )
        };
        assert!(!empty.is_null());
        assert_eq!(unsafe { ctx.array_len(empty) }, 0);
    }
    #[test]
    fn retained_pairs_keep_input_slots_and_reject_growth() {
        let mut ctx = Context::new();
        let schema = record(&[2, 8, 8, 4, 1]);
        let marker = 7u64;
        let values = [&marker as *const u64, &marker as *const u64];
        let original = ctx.array_received(2, 8, crate::context::ArrayElements::CanHoldHandle, 9);
        unsafe {
            std::ptr::copy_nonoverlapping(
                values.as_ptr().cast::<u8>(),
                ctx.array_data(original) as *mut u8,
                16,
            )
        };
        let earlier = [values.as_ptr() as u64, 2, original as u64];
        let path = b"host.items";
        let received = unsafe {
            subscript_rt_boundary_receive(
                &mut *ctx,
                values.as_ptr().cast(),
                1,
                schema.as_ptr(),
                path.as_ptr(),
                path.len() as u32,
                9,
                earlier.as_ptr(),
            )
        };
        assert!(!ctx.trapped());
        assert_eq!(unsafe { ctx.array_len(received) }, 1);
        assert_eq!(
            unsafe { ctx.array_data(received).cast::<*const u64>().read() },
            values[0]
        );
        let grown = unsafe {
            subscript_rt_boundary_receive(
                &mut *ctx,
                values.as_ptr().cast(),
                3,
                schema.as_ptr(),
                path.as_ptr(),
                path.len() as u32,
                9,
                earlier.as_ptr(),
            )
        };
        assert!(grown.is_null());
        assert_eq!(
            ctx.trap_record().expect("count fault").message,
            "host.items: C returned an invalid pointer/count pair"
        );
        ctx.clear_trap();
        let replacement = [&marker as *const u64];
        let received = unsafe {
            subscript_rt_boundary_receive(
                &mut *ctx,
                replacement.as_ptr().cast(),
                1,
                schema.as_ptr(),
                path.as_ptr(),
                path.len() as u32,
                9,
                earlier.as_ptr(),
            )
        };
        assert!(received.is_null());
        assert_eq!(
            ctx.trap_record().expect("replacement fault").message,
            "host.items[0]: C replaced a pointer that the script cannot receive"
        );
    }

    #[test]
    fn changed_pointers_obey_the_script_nullability() {
        let mut ctx = Context::new();
        let path = b"host.Out.leaf";
        unsafe {
            subscript_rt_boundary_pointer(
                &mut *ctx,
                std::ptr::null(),
                1,
                path.as_ptr(),
                path.len() as u32,
                7,
            )
        };
        assert!(!ctx.trapped());
        for (value, nullable) in [(std::ptr::null(), 0), (path.as_ptr(), 1)] {
            unsafe {
                subscript_rt_boundary_pointer(
                    &mut *ctx,
                    value,
                    nullable,
                    path.as_ptr(),
                    path.len() as u32,
                    7,
                )
            };
            let trap = ctx.trap_record().expect("pointer trap");
            assert_eq!(trap.kind as u32, 37);
            assert_eq!(
                trap.message,
                "host.Out.leaf: C replaced a pointer that the script cannot receive"
            );
            assert_eq!(trap.pos_id, 7);
            ctx.clear_trap();
        }
    }
}
