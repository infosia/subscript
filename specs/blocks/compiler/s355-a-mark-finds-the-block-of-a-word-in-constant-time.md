<!-- §355 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 355. A mark finds the block of a word in constant time

*(Added 2026-11-23.)* Origin: a downstream report of collection
pauses, and the measurements G10 and G11 of
`specs/tracking/automatic-collection.md`.

Problem, measured at `b8e593c1` in the ship arena (G11):

1. **The cost of one marked word grows with the heap.** For each
   candidate word the mark searches the sorted chunk map:
   ceil(log2 n) dependent reads. The mark costs 7.3 ns for each word
   at 108 chunks and 13.4 ns at 4,168, and the growth is the growth
   of the search.
2. **A word that is no block pays a hash of the default hasher** when
   the Context holds one large allocation or more: about 7 ns. A
   conservative scan reads mostly such words: five of the six payload
   words of a small object are 0.
3. **The mark is the part of a pause that the headroom does not
   reduce** (G10). In the downstream run it is about 0.3 ms of a
   pause of 1.1 ms. In the curve of §353 (G9) it is the whole pause:
   181 ms for 64 MiB of small objects in an array.

### 355.1 Rules

1. **The arena finds the chunk of an address with a page table.** A
   page is the address divided by the chunk size (64 KiB). The table
   is an open-addressing hash table from a page to the index of a
   chunk. A chunk has an entry for each page that it has a byte in:
   one or two, as a chunk base has no promised alignment. One page
   can have the entries of two chunks. A lookup reads the entries of
   the page of the address and takes the chunk whose range holds the
   address.
2. **The lookup gives the result of the search that it replaces**,
   for each address: the same chunk and block for an address of a
   block below the bump mark, and "no block" for each other address.
   The grid test and the bump test do not change.
3. **The table follows the chunks.** A chunk creation adds its
   entries, and the table grows before its load passes a stated
   limit. The arena frees a chunk only at the end of the Context, so
   no entry is removed. The sorted chunk map is deleted, with its
   sorted insert.
4. **The table is total for each address.** No address of the host
   gives a wrong chunk. An entry can hold a part of the page; a match
   of that part is a candidate only, and the range test against the
   chunk record decides. The entry form limits the count of chunks.
   At that limit, and if the table cannot grow, the allocation takes
   the trap of an allocation that fails, and the table does not
   change. A chunk that the table does not take is not a chunk of the
   arena.
5. **The map of the large allocations uses the address hasher** of
   the runtime (`AddressHasher`). Its keys are payload addresses. The
   low bits of a hash depend on each bit of the address, so payloads
   with one page offset do not start at one bucket.
6. **The reserved bytes do not count the table.** They count the
   chunks, the request records, and the large allocations, and did
   not count the chunk map at the pin.
7. **No observable result changes**: the output of a program, the
   live bytes, the count of collections, the count of words read
   (§353 rule 5), and the trigger (§346).
8. The lookup is that of the ship arena: a releasing Context and a
   Worker Context of one, at each of the three levels of invariant 2.
   The dev kind and a Context with the diagnostics of freed handles
   keep their map of allocations and did not use the chunk map.

### 355.2 What is not in this section

- A chunk with an alignment of 64 KiB, and a prefetch in the mark
  loop (G11: measured, not selected).
- A sweep that skips free chunks or runs in parts (G10).

### 355.3 Acceptance

1. A unit test of rule 2 with a second derivation: for a Context
   with chunks in several size classes and one large allocation or
   more, a linear search over the chunk records gives the expected
   result, and the lookup gives the same, for: each block address, an
   address inside a block and not at its start, an address in the
   chunk header, an address above the bump mark, the first and the
   last byte of each page of each chunk, an address in a page beside
   a chunk, 0, a small integer, and an address that has the entry
   part of a chunk page and is in another page.
2. A unit test of rule 1 for a page with two chunks. If the allocator
   of the host does not give that layout, the test builds the table
   state directly through a function that the lookup also uses.
3. A unit test of rule 3: the table grows over more chunks than its
   first capacity, and each block of each chunk stays found.
4. Unit tests of rule 4: an address with the entry part of a chunk
   page in another page is no block; a chunk at the top of the
   address space is found; the limit of the chunk count gives an
   error and leaves the table as it was; the table refuses an insert
   that it has no room for. A unit test of rule 5: page-aligned
   payload addresses give many values of the low bits of the hash.
5. The tests of the runtime, the stress sweep, and `tools/gate.sh
   full` pass; no golden moves.
6. Measured, release build, and recorded: the mark for each word at
   108 and 4,168 chunks, dense and sparse; the shapes of G9; a bump
   allocation, a reuse, and a chunk creation; `perf-gate --gate`, two
   passes. Expected from G11: the cost of a word does not grow with
   the chunk count in the dense shape; `collect` on the ship tier is
   not slower than at the pin; the allocation cost is inside the
   spread of the pin. The benchmark of the mark stays as an ignored
   test with a stated cost, or the record states how to build it.
7. G9 of the tracking note gets the figures without the test-only
   counter.
