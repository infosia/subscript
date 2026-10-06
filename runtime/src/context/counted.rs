//! Array holders and static release descriptions (§171 rules 2 to 7).

use super::*;

impl Context {
    // Operation 0 acquires, 1 releases, and 2 frees the empty holder.
    // The interpreter releases its own frame registry before operation 2.
    pub(crate) unsafe fn array_holder(&mut self, array: *mut u8, operation: u32, pos: u32) -> bool {
        if array.is_null() {
            return false;
        }
        // SAFETY: callers supply a live array, including its header.
        let header = unsafe { &mut *array.cast::<ArrayHeader>() };
        match operation {
            0 => {
                header.holders = header.holders.saturating_add(1);
                false
            }
            1 => {
                header.holders = header.holders.saturating_sub(1);
                header.holders == 0
            }
            2 => {
                let data = header.data;
                if !data.is_null() {
                    self.delete(data as usize, pos);
                }
                self.delete(array as usize, pos);
                false
            }
            _ => {
                self.trap(TrapKind::Internal, "unknown array holder operation", pos);
                false
            }
        }
    }

    pub(crate) unsafe fn release_handle_array(&mut self, array: *mut u8, pos: u32) {
        if !unsafe { self.array_holder(array, 1, pos) } {
            return;
        }
        let header = unsafe { &*array.cast::<ArrayHeader>() };
        let (len, data) = (header.len as usize, header.data);
        for index in 0..len {
            let frame = unsafe { data.add(index * 8).cast::<*mut u8>().read_unaligned() };
            unsafe { self.async_release(frame, pos) };
            if self.trapped() {
                return;
            }
        }
        unsafe { self.array_holder(array, 2, pos) };
    }

    // Operation 0 acquires copied elements, 1 fills, and 2 copies a range.
    #[allow(clippy::too_many_arguments)]
    pub(crate) unsafe fn counted_array_operation(
        &mut self,
        array: *mut u8,
        description: *const u8,
        operation: u32,
        value: *const u8,
        target: i32,
        start: i32,
        end: i32,
        pos: u32,
    ) {
        if array.is_null() || self.trapped() {
            return;
        }
        let header = unsafe { &*array.cast::<ArrayHeader>() };
        let (len, size, data) = (header.len as usize, header.elem_size as usize, header.data);
        let clamp = |index: i32| {
            let index = i64::from(index);
            if index < 0 {
                (len as i64 + index).max(0) as usize
            } else {
                (index as usize).min(len)
            }
        };
        if operation == 0 {
            for index in 0..len {
                unsafe { self.counted_value(data.add(index * size), description, false, pos) };
            }
            return;
        }
        let from = clamp(start);
        let final_index = clamp(end);
        let to = if operation == 1 { from } else { clamp(target) };
        let count = final_index.saturating_sub(from).min(len - to);
        if operation != 1 && operation != 2 {
            self.trap(TrapKind::Internal, "unknown counted array operation", pos);
            return;
        }
        if count == 0 {
            return;
        }
        // Snapshot both ranges before a replacement. This also protects an
        // overlapping source and a fill value that aliases a receiver slot.
        let source = if operation == 1 {
            unsafe { std::slice::from_raw_parts(value, size) }.to_vec()
        } else {
            unsafe { std::slice::from_raw_parts(data.add(from * size), count * size) }.to_vec()
        };
        let replaced =
            unsafe { std::slice::from_raw_parts(data.add(to * size), count * size) }.to_vec();
        for index in 0..count {
            let offset = if operation == 1 { 0 } else { index * size };
            unsafe { self.counted_value(source.as_ptr().add(offset), description, false, pos) };
        }
        for index in 0..count {
            let offset = if operation == 1 { 0 } else { index * size };
            unsafe {
                std::ptr::copy_nonoverlapping(
                    source.as_ptr().add(offset),
                    data.add((to + index) * size),
                    size,
                );
            }
        }
        for index in 0..count {
            unsafe {
                self.counted_value(replaced.as_ptr().add(index * size), description, true, pos)
            };
            if self.trapped() {
                return;
            }
        }
    }

    pub(crate) unsafe fn counted_value(
        &mut self,
        value: *const u8,
        description: *const u8,
        release: bool,
        pos: u32,
    ) {
        // SAFETY: generated descriptions consist of three u64 words per node.
        let word = |offset| unsafe { description.add(offset).cast::<u64>().read_unaligned() };
        let kind = word(0);
        let offset = word(8) as usize;
        let count = word(16) as usize;
        let child = unsafe { description.add(24) };
        match kind {
            1 => {
                let handle = unsafe { value.cast::<*mut u8>().read_unaligned() };
                if release {
                    unsafe { self.async_release(handle, pos) };
                } else {
                    unsafe { self.async_retain(handle) };
                }
            }
            2 => {
                let array = unsafe { value.cast::<*mut u8>().read_unaligned() };
                // A handle leaf needs no recursive description dispatch per element.
                if release && unsafe { child.cast::<u64>().read_unaligned() } == 1 {
                    unsafe { self.release_handle_array(array, pos) };
                    return;
                }
                if !unsafe { self.array_holder(array, u32::from(release), pos) } {
                    return;
                }
                let header = unsafe { &*array.cast::<ArrayHeader>() };
                let (len, stride, data) =
                    (header.len as usize, header.elem_size as usize, header.data);
                for index in 0..len {
                    unsafe { self.counted_value(data.add(index * stride), child, true, pos) };
                    if self.trapped() {
                        return;
                    }
                }
                unsafe { self.array_holder(array, 2, pos) };
            }
            3 => {
                for index in 0..count {
                    unsafe { self.counted_value(value.add(index * offset), child, release, pos) };
                    if self.trapped() {
                        return;
                    }
                }
            }
            4 => {
                // A completed generator's result has no value owner.
                if unsafe { value.read() } == 0 {
                    unsafe { self.counted_value(value.add(offset), child, release, pos) };
                }
            }
            _ => self.trap(TrapKind::Internal, "unknown counted description", pos),
        }
    }
}
