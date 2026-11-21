<!-- §346 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 346. The trigger counts the bytes that the program requests

*(Added 2026-11-21.)* Origin: `tools/gate.sh full` at `34d950c1`. The
test `perf_gate_meets_every_threshold` failed: `collect` on the ship
tier read 1.59x of C against a limit of 1.50x (C 31.4 ms, ship tier
50.1 ms). §344.4 recorded the cause and did not change it.

Problem, measured:

1. **The ship tier counts the capacity of a size class as live bytes**
   (§18.2d), and §342 rule 6 compares that count with the threshold.
   The `collect` workload requests one byte over each class size, so
   the capacity is 2.02 times the request. Live bytes after an
   explicit collection were 14.16 MB on the ship tier and 7.02 MB on
   the dev tier. The threshold was 30.94 MB and the peak of a round
   about 33 MB, so each round ran one automatic collection between two
   explicit ones: 90 automatic collections with 105 explicit ones. The
   dev tier ran none.
2. **The cost is the whole difference.** One automatic collection took
   2.8 ms of mark and 0.35 ms of sweep. The same binary with mode 0
   read 37.05 and 38.57 ms, against 36.3 ms at `ca3fc9ed`.
3. **The ratio passed the limit only while the C reference was slow.**
   Earlier runs of the day read C at 42 to 44 ms and the ship tier at
   1.18x to 1.22x. With C at 31.4 ms the same ship time is 1.59x.

### 346.1 Rules

1. **The Context counts the bytes that allocations request since the
   last collection.** A successful allocation adds the payload size
   that the caller requests, in each tier. A release of a block
   outside a collection (`Context.free`, a counted release, the release
   of a replaced buffer of an array or of a Map) subtracts the size
   that the allocation of that block added: its request. The count is
   the same number in the two allocator kinds for the same program.
   The count does not go under zero. A collection of any kind sets it
   to zero.
1a. **A block records its request.** The ship tier holds the request of
   each block of a size class, so that rule 1 can subtract it. The
   block header, the emitted element access, and the liveness test of
   generated code do not change. *(Amended 2026-11-21.)* The first
   form subtracted the capacity of the size class. An array that grows
   by `push` to 256 elements then added 4,112 bytes and subtracted
   3,936, so 176 bytes stayed in the count for 4,128 bytes of garbage.
   A loop that allocates two objects of 24 bytes, frees one, and drops
   one added 48 and subtracted 48, so no collection became due.
2. **An allocation sets the pending flag if that count exceeds the
   limit.** *(Replaces §342 rule 6.)* After each collection the limit
   is the larger of live bytes after the collection and the minimum
   headroom. Live bytes are as the tier counts them (§18.2d). The
   default minimum headroom stays 16 MiB.
2a. **The minimum headroom counts requested bytes.** The memory that
   a tier holds for those bytes can be larger. The ship tier holds the
   capacity of the size class: at most twice the request plus 16
   bytes, and a header of 16 bytes for each block (§8.1b). So the ship tier can hold up to that
   much memory before a collection becomes due. The dev tier holds the
   request. The documents state the headroom as bytes that the program
   allocates.
2b. **A release of a block that the last collection kept lowers the
   count too.** The count did not include that block. So the garbage
   that the count does not see is at most the requested bytes of the
   objects that were live at the last collection. The garbage before a
   collection becomes due is at most the limit plus those bytes plus
   one request. Worked case: 1,000 live objects of 1,024 bytes, and
   each iteration frees one of them and drops one new object of the
   same size. A collection becomes due at iteration 17,384 in each
   tier, with 16,384 KiB plus 1,000 KiB of garbage.
2c. **`subscript_rt_ctx_reserved_bytes` includes the request records**
   (§18.2d): 2 bytes for each block of a size class.
3. **`subscript_rt_ctx_set_collection` keeps the count.** *(Restates
   §342 rule 17.)* The limit is the larger of the live bytes after the
   last collection and the new minimum headroom. In mode 1 the call
   sets the flag if the count exceeds the limit.
4. **Nothing else changes.** Mode 0, mode 2, the polls, the outermost
   exit, the detection schedule, and the memory metrics of §18.2d are
   as before.

### 346.2 Acceptance

1. **`collect`, ship tier.** One run makes no automatic collection
   between two explicit ones (a test reads the count). Red at
   `34d950c1`: 6 automatic collections for each iteration of the
   workload.
2. **`perf-gate --gate` passes** in two quiet passes, and `collect` on
   the ship tier equals `ca3fc9ed` in the noise of the two passes.
3. **Reclamation holds.** The tree without `free` (polls only, and
   with the bracket) keeps live bytes under three trees in the three
   tiers, with no change of the bound of its tests. For 10,000 host
   calls of a straight-line entry, the requested count after each exit
   is at most the limit plus the request of one call, in each tier.
   Live bytes are at most the live bytes after the last collection
   plus the bytes that the tier holds for that many calls. *(This
   replaces the bound of §342.2 item 3, which was in live bytes: the
   ship tier holds 496 bytes for a request of 256, and its peak moved
   from 18,368 to 31,744 bytes in that test.)*
4. **`Context.free` lowers the count.** The `tree` workload with its
   `free` function runs no automatic collection (a test reads the
   count). Control: the same workload with no `free` runs more than
   one.
5. **A host that switches the mode at each frame** gets the same
   collections as a host that leaves mode 1 set (the test of §342 rule
   17 passes).
6. `callbacks`, `tree`, `particles`, `sort`: equal to the figures of
   §344.5 in the noise of two passes.
7. **A release does not hide garbage.** In each allocator kind, with a
   fixed minimum headroom: (a) a loop that allocates two objects of 24
   bytes, frees one, and drops one makes a collection due after at
   most the limit divided by 24, plus one, iterations; (b) a loop that
   builds an array of 256 `f64` elements by `push` and drops it makes
   a collection due after the same count of arrays in the two kinds;
   (c) control: the same loops with each block freed run no automatic
   collection; (d) a Map that grows by 256 inserts and is dropped
   makes a collection due after the same count of Maps in the two
   kinds. Red at `fc3dced7` in the ship kind: (a) never becomes
   due, and (b) becomes due about 12 times later than in the dev kind.
8. `tools/gate.sh full` passes.

### 346.3 Sections this one amends

- §342 rule 6 (rule 2 here) and rule 17 (rule 3 here).
- §344.4 closes.
