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
        }
        unsafe { self.array_holder(array, 2, pos) };
    }

    // Operation 0 acquires, 1 fills, 2 copies a range, and 3 clears.
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
        if operation == 3 {
            if description.is_null() || len == 0 {
                unsafe { self.array_truncate(array, 0, pos) };
                return;
            }
            let removed = unsafe { std::slice::from_raw_parts(data, len * size) }.to_vec();
            unsafe { self.array_truncate(array, 0, pos) };
            for index in 0..len {
                unsafe {
                    self.counted_value(removed.as_ptr().add(index * size), description, true, pos)
                };
                if self.trapped() {
                    return;
                }
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
            1 | 5 => {
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
                }
                unsafe { self.array_holder(array, 2, pos) };
            }
            3 => {
                for index in 0..count {
                    unsafe { self.counted_value(value.add(index * offset), child, release, pos) };
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

impl Context {
    /// Installs the resolved field description for a live class allocation.
    ///
    /// # Safety
    /// `payload` is live. `description` has valid field offsets and recursive nodes.
    pub unsafe fn describe_object(&mut self, payload: usize, description: &[u8]) {
        if !self.is_live(payload) || description.is_empty() {
            return;
        }
        self.object_descriptions
            .insert(payload, description.to_vec());
    }

    /// Removes a class description and ends its array holders before returning handle leaves.
    /// The caller releases the leaves through its registry, then deletes the returned array storage and the object.
    ///
    /// # Errors
    /// Returns an error if a field description has an invalid size or node chain.
    pub fn take_object_releases(
        &mut self,
        payload: usize,
        pos: u32,
    ) -> Result<(Vec<usize>, Vec<usize>), &'static str> {
        let Some(mut description) = self.object_descriptions.remove(&payload) else {
            return Ok((Vec::new(), Vec::new()));
        };
        if !self.is_live(payload) {
            return Ok((Vec::new(), Vec::new()));
        }
        if description == 0u64.to_ne_bytes()
            && unsafe { header_class_id((payload as *const u8).sub(HEADER_SIZE)) }
                == CLASS_GENERATOR
        {
            // SAFETY: the generator marker refers to a live generated frame.
            description = unsafe { self.generator_description(payload as *mut u8) };
        }
        let word = |offset: usize| -> Result<usize, &'static str> {
            let end = offset
                .checked_add(8)
                .ok_or("malformed object description size")?;
            let bytes = description
                .get(offset..end)
                .ok_or("malformed object description size")?;
            let mut value = [0; 8];
            value.copy_from_slice(bytes);
            usize::try_from(u64::from_ne_bytes(value))
                .map_err(|_| "malformed object description size")
        };
        let mut cursor = 8usize;
        let mut fields = Vec::new();
        for _ in 0..word(0)? {
            let offset = word(cursor)?;
            let size = word(
                cursor
                    .checked_add(8)
                    .ok_or("malformed object description size")?,
            )?;
            cursor = cursor
                .checked_add(16)
                .ok_or("malformed object description size")?;
            let end = cursor
                .checked_add(size)
                .ok_or("malformed object description size")?;
            let nodes = description
                .get(cursor..end)
                .ok_or("malformed object description size")?;
            if nodes.is_empty() || nodes.len() % 24 != 0 {
                return Err("malformed object description size");
            }
            for (index, node) in nodes.chunks_exact(24).enumerate() {
                let mut kind = [0; 8];
                kind.copy_from_slice(&node[..8]);
                match (u64::from_ne_bytes(kind), index + 1 == nodes.len() / 24) {
                    (1 | 5, true) | (2..=4, false) => {}
                    _ => return Err("malformed object description size"),
                }
            }
            fields.push((offset, cursor));
            cursor = end;
        }
        if cursor != description.len() {
            return Err("malformed object description size");
        }
        let mut handles = Vec::new();
        let mut storage = Vec::new();
        for (offset, cursor) in fields {
            // SAFETY: describe_object requires live fields; the checks above establish every node's storage.
            unsafe {
                self.counted_release_leaves(
                    (payload as *const u8).add(offset),
                    description.as_ptr().add(cursor),
                    pos,
                    &mut handles,
                    &mut storage,
                );
            }
        }
        Ok((handles, storage))
    }

    /// Releases container counts and gathers frame keys before storage retirement.
    ///
    /// # Safety
    /// `value` must match the static recursive `description`. All container storage must be live.
    /// The caller must consume each frame key and then retire the returned storage.
    pub unsafe fn counted_release_leaves(
        &mut self,
        value: *const u8,
        description: *const u8,
        pos: u32,
        handles: &mut Vec<usize>,
        storage: &mut Vec<usize>,
    ) {
        let word =
            |offset| unsafe { description.add(offset).cast::<u64>().read_unaligned() as usize };
        let child = unsafe { description.add(24) };
        match word(0) {
            1 | 5 => handles.push(unsafe { value.cast::<usize>().read_unaligned() }),
            2 => {
                let array = unsafe { value.cast::<*mut u8>().read_unaligned() };
                if !unsafe { self.array_holder(array, 1, pos) } {
                    return;
                }
                let header = unsafe { &*array.cast::<ArrayHeader>() };
                let (len, stride, data) =
                    (header.len as usize, header.elem_size as usize, header.data);
                for index in 0..len {
                    unsafe {
                        self.counted_release_leaves(
                            data.add(index * stride),
                            child,
                            pos,
                            handles,
                            storage,
                        )
                    };
                }
                if !data.is_null() {
                    storage.push(data as usize);
                }
                storage.push(array as usize);
            }
            3 => {
                for index in 0..word(16) {
                    unsafe {
                        self.counted_release_leaves(
                            value.add(index * word(8)),
                            child,
                            pos,
                            handles,
                            storage,
                        )
                    };
                }
            }
            4 => {
                if unsafe { value.read() } == 0 {
                    unsafe {
                        self.counted_release_leaves(
                            value.add(word(8)),
                            child,
                            pos,
                            handles,
                            storage,
                        )
                    };
                }
            }
            _ => self.trap(TrapKind::Internal, "unknown counted description", pos),
        }
    }
}

impl Context {
    fn collection_marked(&self, address: usize) -> bool {
        if !self.uses_ship_arena() {
            return self
                .allocations
                .get(&address)
                .is_some_and(|allocation| allocation.marked);
        }
        let block = self
            .arena_lookup_block(address)
            .map(|(block, _)| block)
            .or_else(|| self.large.get(&address).map(|allocation| allocation.base));
        // SAFETY: an exact allocation lookup proves that the header is readable.
        block.is_some_and(|block| unsafe { block.cast::<u64>().read() == MARK_STATE })
    }

    pub(super) fn unreachable_releases(&mut self) -> (Vec<usize>, Vec<usize>) {
        if self.object_descriptions.is_empty() && self.counted_maps.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let objects: Vec<_> = self
            .object_descriptions
            .keys()
            .copied()
            .filter(|address| !self.collection_marked(*address))
            .collect();
        let maps: Vec<_> = self
            .counted_maps
            .keys()
            .copied()
            .filter(|address| !self.collection_marked(*address))
            .collect();
        let mut handles = Vec::new();
        let mut storage = Vec::new();
        for object in objects {
            let (leaves, arrays) = match self.take_object_releases(object, 0) {
                Ok(releases) => releases,
                Err(message) => {
                    self.trap(TrapKind::Internal, message, 0);
                    continue;
                }
            };
            handles.extend(leaves);
            storage.extend(arrays);
        }
        for map in maps {
            let description = self.counted_maps.remove(&map).unwrap_or_default();
            // SAFETY: all unreachable allocation storage remains live until sweep.
            unsafe {
                crate::assocops::release_leaves(
                    self,
                    map as *mut u8,
                    if description.is_empty() {
                        std::ptr::null()
                    } else {
                        description.as_ptr()
                    },
                    &mut handles,
                    &mut storage,
                )
            };
        }
        (handles, storage)
    }
}

impl Context {
    /// Installs a Map value description for a tier with a separate handle registry.
    /// Native Map operations keep their own header description and count consumer.
    ///
    /// # Safety
    /// `payload` is a live Map. `description` matches its resolved value layout.
    pub unsafe fn describe_map(&mut self, payload: usize, description: &[u8]) {
        if self.is_live(payload) && !description.is_empty() {
            self.counted_maps.insert(payload, description.to_vec());
        }
    }
}

impl Context {
    // Keep reference-holder traversal outside the ordinary allocation-release path.
    #[inline(never)]
    pub(super) fn release_reference_holder(&mut self, payload: usize, pos: u32) {
        self.counted_maps.remove(&payload);
        if !self.object_descriptions.contains_key(&payload) || !self.is_live(payload) {
            return;
        }
        let (handles, storage) = match self.take_object_releases(payload, pos) {
            Ok(releases) => releases,
            Err(message) => {
                self.trap(TrapKind::Internal, message, pos);
                return;
            }
        };
        for handle in handles {
            // SAFETY: the allocation description supplies registered handles.
            unsafe { self.async_release(handle as *mut u8, pos) };
        }
        for address in storage {
            self.delete(address, pos);
        }
    }
}

impl Context {
    pub(super) unsafe fn generator_description(&self, frame: *mut u8) -> Vec<u8> {
        // SAFETY: the generator header stores a static generated description at offset 24.
        let description = unsafe {
            frame
                .add(crate::generator_layout::CLEANUP_OFFSET as usize)
                .cast::<*const u8>()
                .read()
        };
        if description.is_null() {
            return 0u64.to_ne_bytes().to_vec();
        }
        let fields = unsafe { description.cast::<u64>().read() };
        let mut size = 8usize;
        for _ in 0..fields {
            let child = unsafe { description.add(size + 8).cast::<u64>().read() } as usize;
            size += 16 + child;
        }
        unsafe { std::slice::from_raw_parts(description, size) }.to_vec()
    }

    pub(super) unsafe fn generator_release(&mut self, frame: *mut u8, pos: u32) {
        if frame.is_null() || !self.object_descriptions.contains_key(&(frame as usize)) {
            return;
        }
        // SAFETY: the registered generator frame has a holder count at offset 16.
        let count = unsafe {
            &mut *frame
                .add(crate::generator_layout::HOLDERS_OFFSET as usize)
                .cast::<u32>()
        };
        if *count != 0 {
            *count -= 1;
        }
        if *count != 0 {
            return;
        }
        self.release_reference_holder(frame as usize, pos);
        self.delete(frame as usize, pos);
    }
}
