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

### 355.4 Phase Review

Form.

- `runtime/src/context/chunk_pages.rs`: `ChunkPages`. An entry is one
  64-bit word: the low 32 bits of the page in the high half and the
  chunk index in the low half. The hash is a multiplicative hash of
  the page part. The first capacity is 64 slots, the load is 1/2 at
  most, and the table doubles. The table removes no entry, so each
  entry of a page part is before the first empty slot of its probe
  sequence.
- `arena_new_chunk` reserves the room, allocates the chunk, inserts
  it, and then adds the chunk record. A failure of each step takes
  the trap of an allocation that fails and leaves the chunks and the
  table equal.
- `AddressHasher` rotates the product by 32 bits. A fold of the high
  half into the low half made the `deep-chains` workload of
  `async-cost` about 6.5 % slower in three runs; the rotation did
  not.
- The prototype of G11 kept the low 16 bits of the page in the
  compare and did not find a chunk with a base at 2^48 or above. The
  landed form does not have that limit.

Measured, release build, arm64 macOS. The mark for each word is the
time of a collection with 23,040 live root words minus the time of
the same heap with no live block, so it holds the restore of the
marked blocks in the sweep.

| Item | Pin | After |
|---|---|---|
| Mark, ns for each word, 108 chunks, dense; sparse | 9.3 to 9.5; 11.2 to 11.4 | 5.5; 8.3 to 8.9 |
| The same, 4,168 chunks | 16.3 to 16.8; 48.4 | 8.2 to 8.5; 23.1 to 25.1 |
| Lookup only, ns for each word, 108 and 4,168 chunks, dense | 7.6 and 12.8 to 13.4 | 1.2 to 4.4 at each count |
| A bump allocation; a reuse; a chunk creation, ns | 5.61; 6.28; 1,289 to 1,299 | 5.61; 6.25; 1,078 |
| `perf-gate --gate`, `collect`, ship tier | 36.730 and 36.490 ms (1.17x, 1.16x) | 30.368 and 30.187 ms (0.97x, 0.96x) |
| The same, `a22` | 5.270 and 5.282 ms | 5.259 and 5.256 ms |
| A hit in the large map, ns, with 64, 1,024, and 8,192 allocations of 16 KiB | 5.0, 9.5, 88.2 | 3.3 to 3.6 at each count |
| `async-cost` `deep-chains`, ms | 9.49 to 9.59 | 9.15 to 9.29 |
| Stress sweep; tests of the runtime, debug and release | | pass; pass |

The curve of G9 without the test counter is in
`specs/tracking/automatic-collection.md`, G9.

The benchmarks are ignored tests of `runtime/tests/mark_lookup_cost.rs`
(about 3 s in a release build, 350 MiB at most).

Phase Review (a subagent with no context): no CRITICAL, no MAJOR, 7
MINOR. The review compared the lookup with the search that it
replaces for each class of address and found no difference; it found
the load bound, the end of each probe, and the rehash correct, and no
path that leaves a chunk outside the table.

| Finding | Close |
|---|---|
| The address hasher gave one start bucket to page-aligned payloads | measured: a hit took 88 ns with 8,192 allocations; the rotation; rule 5 and a test |
| An insert with no reserved room returned with no signal | the insert returns an error, and the arena does not keep the chunk; a test |
| Rule 4 stated an address range that the entry form does not have; rules 6 and 8 left a fact to the round | the rules state the form, the reserved bytes, and the Context kinds |
| Two memory figures of the benchmark were low | corrected |
| No test puts a chunk with two pages through the allocator | open item 1 |
| No test reaches the trap of a table that cannot grow | open item 2 |

`tools/gate.sh full` on the tree before this record:

```text
gate full baf8ac144b84eceb5ca7d9d5dabe840d8e897eb6 dirty:5 debug 4146/0/10 release 4127/0/10 skips 2/0 clippy 4/10/13 goldens-moved 0 exit 0
```

`tools/hygiene.sh`: exit 0.

### 355.5 Open

1. The test of rule 2 gets chunks from the allocator of the host,
   which gives a chunk with two pages in some runs only. The test of
   rule 1 builds the table state of two chunks in one page directly.
2. No test reaches the trap of rule 4: it needs 2^32 chunks, or a
   table allocation that fails.
3. The stress sweep builds the runtime archive in its first entries
   if the archive is older than the runtime source. The first ship
   link can then fail with "library not found". The full gate builds
   the archive before the sweep. One run of the sweep alone, after an
   edit of the runtime, failed in this way.
4. The sparse shape, and the prefetch
   (`specs/tracking/automatic-collection.md`, G11 open items).

