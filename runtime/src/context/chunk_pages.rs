//! The page table of the ship arena (`compiler.md` §355 rules 1 to 4): an
//! open-addressing hash table from a page of 64 KiB to the index of a
//! chunk that has a byte in that page.

use super::CHUNK_SIZE;

// compiler.md §355 rule 1: a page is the address divided by the chunk size.
const _: () = assert!(CHUNK_SIZE.is_power_of_two());
const PAGE_SHIFT: u32 = CHUNK_SIZE.trailing_zeros();

/// The value of a slot that holds no entry. Its low half is `u32::MAX`,
/// which no chunk index has.
const EMPTY: u64 = u64::MAX;

/// The slots of the table after the first chunk creation.
const FIRST_CAPACITY: usize = 64;

/// The count of chunks that the entry form holds: a chunk index is below
/// `u32::MAX` (`compiler.md` §355 rule 4).
const MAX_CHUNKS: usize = u32::MAX as usize;

/// The reason that a chunk cannot get its entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReserveError {
    /// The chunk index is `MAX_CHUNKS` or more.
    ChunkIndex,
    /// The allocation of a larger table failed.
    Allocation,
    /// An insert found no room: no [`ChunkPages::reserve`] for the chunk.
    NotReserved,
}

/// The page table. An entry is one word: the low 32 bits of the page in
/// the high half and the chunk index in the low half. The hash of an entry
/// is a function of those 32 bits of the page only, so the table can grow
/// from its entries. Two pages with the same low 32 bits are 2^48 bytes
/// apart; their entries share a hash, and the range test of
/// [`ChunkPages::find`] tells them apart. So each address of the host has
/// its correct result, and no address is out of the range of the form.
///
/// The load is at most one half: the table grows before an insert passes
/// it (`compiler.md` §355 rule 3). So each probe sequence ends at an empty
/// slot. No entry is removed, because the arena frees a chunk only at the
/// end of the Context.
#[derive(Debug, Default)]
pub(super) struct ChunkPages {
    slots: Vec<u64>,
    entries: usize,
    // 64 less the log2 of the capacity: the hash is the high bits of the
    // product.
    shift: u32,
}

/// The 32 bits of `page` that an entry holds.
#[inline]
fn page_tag(page: usize) -> u32 {
    page as u32
}

/// The entry of the page with the tag `tag` for the chunk `index`.
#[inline]
fn entry(tag: u32, index: u32) -> u64 {
    (u64::from(tag) << 32) | u64::from(index)
}

impl ChunkPages {
    /// The first slot of the page with the tag `tag`. The capacity is not 0.
    #[inline]
    fn first_slot(&self, tag: u32) -> usize {
        (u64::from(tag).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> self.shift) as usize
    }

    /// Makes the table ready for the two entries of the chunk `index`: it
    /// checks the index against the form, and grows the table if the two
    /// entries would pass the load limit. On an error the table does not
    /// change.
    pub(super) fn reserve(&mut self, index: usize) -> Result<(), ReserveError> {
        if index >= MAX_CHUNKS {
            return Err(ReserveError::ChunkIndex);
        }
        let need = self.entries.saturating_add(2).saturating_mul(2);
        if need <= self.slots.len() {
            return Ok(());
        }
        let mut capacity = self.slots.len().max(FIRST_CAPACITY);
        while capacity < need {
            capacity = capacity.checked_mul(2).ok_or(ReserveError::Allocation)?;
        }
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(capacity)
            .map_err(|_| ReserveError::Allocation)?;
        slots.resize(capacity, EMPTY);
        let old = std::mem::replace(&mut self.slots, slots);
        self.shift = 64 - capacity.trailing_zeros();
        self.entries = 0;
        for value in old {
            if value != EMPTY {
                self.put(value);
            }
        }
        Ok(())
    }

    /// Adds the entries of the chunk `index` with the base `base`: one for
    /// each page that the chunk has a byte in. [`ChunkPages::reserve`] for
    /// `index` must come first. Without it, or for an index that the form
    /// cannot hold, the call adds nothing and returns the error.
    pub(super) fn insert_chunk(&mut self, base: usize, index: usize) -> Result<(), ReserveError> {
        let index = u32::try_from(index)
            .ok()
            .filter(|&index| index != u32::MAX)
            .ok_or(ReserveError::ChunkIndex)?;
        let first = base >> PAGE_SHIFT;
        let last = base.saturating_add(CHUNK_SIZE - 1) >> PAGE_SHIFT;
        let pages = last - first + 1;
        if self.entries.saturating_add(pages).saturating_mul(2) > self.slots.len() {
            return Err(ReserveError::NotReserved);
        }
        for page in first..=last {
            self.put(entry(page_tag(page), index));
        }
        Ok(())
    }

    /// Writes `value` into the first empty slot of its probe sequence. The
    /// load is below one half, so an empty slot exists.
    fn put(&mut self, value: u64) {
        let mask = self.slots.len() - 1;
        let mut slot = self.first_slot((value >> 32) as u32);
        for _ in 0..self.slots.len() {
            if self.slots[slot] == EMPTY {
                self.slots[slot] = value;
                self.entries += 1;
                return;
            }
            slot = (slot + 1) & mask;
        }
    }

    /// The index of the chunk whose range of `CHUNK_SIZE` bytes holds
    /// `address`, or `None`. `base_of` gives the base of a chunk by its
    /// index. Each entry of the page of `address` is a candidate, and the
    /// range of its chunk decides: one page can have the entries of two
    /// chunks (`compiler.md` §355 rule 1).
    #[inline]
    pub(super) fn find(
        &self,
        address: usize,
        base_of: impl Fn(usize) -> Option<usize>,
    ) -> Option<usize> {
        if self.slots.is_empty() {
            return None;
        }
        let tag = page_tag(address >> PAGE_SHIFT);
        let mask = self.slots.len() - 1;
        let mut slot = self.first_slot(tag);
        // The load is at most one half, so the sequence reaches an empty
        // slot.
        loop {
            let value = self.slots[slot];
            if value == EMPTY {
                return None;
            }
            if (value >> 32) as u32 == tag {
                let index = (value & u64::from(u32::MAX)) as usize;
                if address.wrapping_sub(base_of(index)?) < CHUNK_SIZE {
                    return Some(index);
                }
            }
            slot = (slot + 1) & mask;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;

    impl ChunkPages {
        /// The capacity of the table, in slots.
        fn capacity(&self) -> usize {
            self.slots.len()
        }
    }

    /// A table with chunks of the given bases, indexed in that sequence.
    fn table(bases: &[usize]) -> ChunkPages {
        let mut pages = ChunkPages::default();
        for (index, &base) in bases.iter().enumerate() {
            pages.reserve(index).expect("the table grows");
            pages.insert_chunk(base, index).expect("the table has room");
        }
        pages
    }

    // compiler.md §355 acceptance 2: one page has the tail of one chunk
    // and the head of another. The host allocator does not give this
    // layout on request, so the test builds the table through
    // `insert_chunk` and reads it through `find`, the function of the
    // lookup. Both orders of creation. Cost: 16 lookups, below 1 ms.
    #[test]
    fn a_page_with_two_chunks_finds_the_chunk_whose_range_holds_the_address() {
        let shared = 0x1234usize << PAGE_SHIFT;
        let low = shared - CHUNK_SIZE / 2;
        let high = shared + CHUNK_SIZE / 2;
        for bases in [[low, high], [high, low]] {
            let pages = table(&bases);
            let base_of = |index: usize| bases.get(index).copied();
            let index_of = |base: usize| bases.iter().position(|&b| b == base);
            for (address, base) in [
                (shared, low),
                (high - 1, low),
                (high, high),
                (shared + CHUNK_SIZE - 1, high),
                (low, low),
                (high + CHUNK_SIZE - 1, high),
            ] {
                assert_eq!(pages.find(address, base_of), index_of(base), "{address:#x}");
            }
            assert_eq!(pages.find(low - 1, base_of), None);
            assert_eq!(pages.find(high + CHUNK_SIZE, base_of), None);
        }
    }

    // compiler.md §355 acceptance 4: the entry holds 32 bits of the page,
    // so an address 2^48 bytes above a chunk has the tag of that chunk.
    // The lookup gives "no block" there, and for the highest addresses.
    // A chunk index that the form cannot hold is refused before a change.
    // Cost: a table of three chunks, below 1 ms.
    #[test]
    fn an_address_with_the_tag_of_a_chunk_and_another_page_is_no_block() {
        let bases = [0x7000_0000usize, 0x7001_8000, 0x7003_0000];
        let pages = table(&bases);
        let base_of = |index: usize| bases.get(index).copied();
        assert_eq!(pages.find(bases[1] + 16, base_of), Some(1));
        #[cfg(target_pointer_width = "64")]
        for &base in &bases {
            let alias = base + (1usize << 48) + 16;
            assert_eq!(page_tag(alias >> PAGE_SHIFT), page_tag(base >> PAGE_SHIFT));
            assert_eq!(pages.find(alias, base_of), None, "{alias:#x}");
        }
        for address in [usize::MAX, usize::MAX - 15, usize::MAX - CHUNK_SIZE] {
            assert_eq!(pages.find(address, base_of), None, "{address:#x}");
        }

        // A chunk at the top of the address space has its entries.
        let top = usize::MAX - (CHUNK_SIZE - 1);
        let pages = table(&[top]);
        assert_eq!(pages.find(usize::MAX, |_| Some(top)), Some(0));

        let mut pages = table(&bases);
        let capacity = pages.capacity();
        assert_eq!(pages.reserve(MAX_CHUNKS), Err(ReserveError::ChunkIndex));
        assert_eq!(pages.capacity(), capacity);
        assert_eq!(
            pages.insert_chunk(0x9000_0000, MAX_CHUNKS),
            Err(ReserveError::ChunkIndex)
        );
        assert_eq!(pages.find(0x9000_0010, |_| Some(0x9000_0000)), None);
    }

    // compiler.md §355 rule 3: an insert with no reserve for its chunk is
    // refused and changes nothing, in an empty table and in a full one.
    // Cost: 64 slots, below 1 ms.
    #[test]
    fn an_insert_with_no_reserve_is_refused() {
        let mut pages = ChunkPages::default();
        assert_eq!(
            pages.insert_chunk(0x7000_8000, 0),
            Err(ReserveError::NotReserved)
        );
        assert_eq!(pages.find(0x7000_8010, |_| Some(0x7000_8000)), None);

        // 16 chunks of two pages fill the first capacity to its limit.
        let bases: Vec<usize> = (0..16).map(|i| 0x7000_8000 + i * 2 * CHUNK_SIZE).collect();
        let mut pages = table(&bases);
        assert_eq!(pages.capacity(), FIRST_CAPACITY);
        let extra = 0x7100_8000;
        assert_eq!(
            pages.insert_chunk(extra, 16),
            Err(ReserveError::NotReserved)
        );
        assert_eq!(pages.capacity(), FIRST_CAPACITY);
        let base_of = |index: usize| bases.get(index).copied().or((index == 16).then_some(extra));
        assert_eq!(pages.find(extra + 16, base_of), None);
        assert_eq!(pages.find(bases[15] + 16, base_of), Some(15));
        // A control: after the reserve, the same insert succeeds.
        pages.reserve(16).expect("the table grows");
        assert_eq!(pages.insert_chunk(extra, 16), Ok(()));
        assert_eq!(pages.find(extra + 16, base_of), Some(16));
    }

    // compiler.md §355 acceptance 3: 100 chunks of the largest size class
    // need more slots than the first capacity, and each block of each
    // chunk stays found after the growth. Cost: 6.4 MiB of chunks and
    // 1,600 lookups, below 10 ms in a debug build.
    #[test]
    fn the_table_grows_and_each_block_stays_found() {
        const CHUNKS: usize = 100;
        let blocks = CHUNK_SIZE / LARGEST_BLOCK;
        let mut ctx = Context::new_releasing();
        let payloads: Vec<usize> = (0..CHUNKS * blocks)
            .map(|_| ctx.alloc(LARGEST_BLOCK - HEADER_SIZE, CLASS_ARRAY_DATA_NO_HANDLE, 0) as usize)
            .collect();
        assert!(payloads.iter().all(|&p| p != 0));
        assert_eq!(ctx.chunks.len(), CHUNKS);
        assert!(ctx.chunk_pages.capacity() > FIRST_CAPACITY);
        for (i, &payload) in payloads.iter().enumerate() {
            assert_eq!(
                ctx.arena_lookup_slot(payload),
                Some((i / blocks, i % blocks))
            );
        }
    }

    /// The result that a linear search over the chunk records gives for
    /// `address`: the chunk index and block index of a block payload below
    /// the bump mark, or `None`.
    fn linear_search(ctx: &Context, address: usize) -> Option<(usize, usize)> {
        for (index, chunk) in ctx.chunks.iter().enumerate() {
            let base = chunk.base as usize;
            if address < base || address - base >= CHUNK_SIZE {
                continue;
            }
            let offset = (address - base).checked_sub(HEADER_SIZE)?;
            if offset % chunk.block_size != 0 {
                return None;
            }
            let block = offset / chunk.block_size;
            return (block < chunk.bump).then_some((index, block));
        }
        None
    }

    // compiler.md §355 acceptance 1: the lookup and a linear search over
    // the chunk records give the same result for each address of the
    // list of the acceptance, in a Context with chunks of four size
    // classes and two large allocations. Cost: 8 chunks and about 12,000
    // lookups, below 50 ms in a debug build.
    #[test]
    fn the_lookup_gives_the_result_of_a_linear_search() {
        let mut ctx = Context::new_releasing();
        // Two chunks of each class; the second chunk is part full, so each
        // class has slots above its bump mark.
        for payload in [16usize, 48, 1008, 4080] {
            let per_chunk = CHUNK_SIZE / (payload + HEADER_SIZE);
            for _ in 0..per_chunk + per_chunk / 2 {
                assert!(!ctx.alloc(payload, OBJECT_CLASS_FOR_TEST, 0).is_null());
            }
        }
        let large = [
            ctx.alloc(LARGEST_BLOCK, OBJECT_CLASS_FOR_TEST, 0) as usize,
            ctx.alloc(3 * CHUNK_SIZE, OBJECT_CLASS_FOR_TEST, 0) as usize,
        ];
        assert!(large.iter().all(|&p| p != 0));
        assert_eq!(ctx.chunks.len(), 8);
        assert_eq!(ctx.large.len(), 2);

        // The chunks that have a byte in two pages. The count depends on
        // the host allocator, so the test does not assert on it; the
        // direct test of acceptance 2 holds the two-page case.
        let two_pages = ctx
            .chunks
            .iter()
            .filter(|chunk| !(chunk.base as usize).is_multiple_of(CHUNK_SIZE))
            .count();
        println!("chunks with a byte in two pages: {two_pages} of 8");

        let mut addresses = vec![0usize, 8, 16, 4096, usize::MAX, usize::MAX - 15];
        addresses.extend(large);
        addresses.extend(large.map(|p| p + 16));
        for chunk in &ctx.chunks {
            let base = chunk.base as usize;
            // Each block below the bump mark, and an address inside it.
            for block in 0..chunk.bump {
                let payload = base + block * chunk.block_size + HEADER_SIZE;
                addresses.extend([payload, payload + 8]);
            }
            // The header of the chunk, and the first block above the mark.
            addresses.extend([base, base + 8]);
            addresses.push(base + chunk.bump * chunk.block_size + HEADER_SIZE);
            // The first and last byte of each page of the chunk, and a page
            // beside the chunk on each side.
            let first = base >> PAGE_SHIFT;
            let last = (base + CHUNK_SIZE - 1) >> PAGE_SHIFT;
            for page in first..=last {
                addresses.extend([page << PAGE_SHIFT, ((page + 1) << PAGE_SHIFT) - 1]);
            }
            addresses.extend([
                ((first - 1) << PAGE_SHIFT) + HEADER_SIZE,
                ((last + 1) << PAGE_SHIFT) + HEADER_SIZE,
            ]);
            #[cfg(target_pointer_width = "64")]
            addresses.push(base + (1usize << 48) + HEADER_SIZE);
        }
        addresses.sort_unstable();
        addresses.dedup();
        let mut found = 0;
        for &address in &addresses {
            let expected = linear_search(&ctx, address);
            found += usize::from(expected.is_some());
            assert_eq!(ctx.arena_lookup_slot(address), expected, "{address:#x}");
        }
        // A control: the list holds blocks, and the linear search finds them.
        let blocks: usize = ctx.chunks.iter().map(|chunk| chunk.bump).sum();
        assert_eq!(found, blocks);
    }

    /// A user class id that can hold a handle.
    const OBJECT_CLASS_FOR_TEST: u32 = 900;
}
