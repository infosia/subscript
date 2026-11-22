<!-- §353 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 353. A host reads the count and the time of collections

*(Added 2026-11-22.)* Origin: the owner decision of 2026-11-22: a C
function that reports the time of a collection, as the input of a
later decision on an incremental collector. A callback form and a
script form are not in this section.

Problem:

1. **A host cannot see a collection that runs inside a call.** An
   automatic collection runs at a poll or at an outermost exit (§342).
   The host can time its own `subscript_rt_ctx_collect`, and a whole
   export call; the time of a collection inside that call is not
   separate. The count of automatic collections has a runtime entry
   that the host header does not declare (§345 rule 4).
2. **The decision on an incremental collector has one measured
   point.** One collection with about 4 MiB of words that it read took
   about 7 ms on the ship tier, before §344 (the measurement G6 of
   `specs/tracking/automatic-collection.md`): about 13 ns for each
   word. No figure exists for a graph of small objects, and none for
   a program of a host.
3. **Invariant 2 states that the host controls each pause.** A host
   that cannot read the length of a pause cannot tune the headroom,
   or select the host-timed level, on evidence.

### 353.1 Rules

1. **A Context keeps statistics of its collections.** For each
   collection that ends: its class, its time, the words that its mark
   read, and the live bytes after it. The classes are "automatic" (at
   a poll, at an outermost exit, and the detection collection of §342
   rule 14) and "explicit" (`Context.collect()` and
   `subscript_rt_ctx_collect`).
2. **The record.** A C struct of `uint64_t` fields, in this sequence:
   `automatic_collections`, `explicit_collections`, `total_ns`,
   `max_ns`, `last_ns`, `last_words_read`, `max_words_read`,
   `last_live_bytes_after`. The counts and the sums are since the
   Context started or since the last reset. `max_ns` and
   `max_words_read` are each the largest of one collection, and they
   can come from different collections. Before the first collection
   each field is zero.
3. **Two functions.**
   `subscript_rt_ctx_collection_stats(ctx, out)` writes the record.
   `subscript_rt_ctx_reset_collection_stats(ctx)` sets each field to
   zero. A host calls them between script calls, as it calls
   `subscript_rt_ctx_live_bytes`. A null `out` writes nothing.
4. **The time of a collection** is from its start to its end on a
   monotonic clock: the preparation of the roots, the mark, the
   release of the counted fields of unreachable objects, the sweep,
   and the work of §348 and §351 inside it. It does not include the
   delivery of a failure after the collection, and it does not
   include the check walks of a debug build of the runtime (the check
   of the array tails before a collection, and the count of the live
   bytes by a walk after it).
5. **The words that the mark read** is the count of words that the
   mark tested as a possible reference, in the roots and in the
   payloads. It is a function of the heap and the roots, not of the
   host: two runs of one program in one tier give the same count,
   except where a word that is not a reference equals the address of
   a live object (the scan is conservative, §342.5 item 1). The two
   tiers do not give the same count: the dev tier reads the
   requested payload of an object, and the ship tier reads the
   payload of its size class (4 and 6 words for an object of 32
   bytes). A
   later bound on the work of one step of a collector uses this unit.
6. **No script form.** A script cannot read the record: a time that a
   script reads makes a collection observable (§342 rule 2).
7. **Cost.** A collection reads the clock two times and adds to the
   record. The count of the words uses the counter of the mark loop,
   with no work for each word that a build without this section does
   not do. A program with no collection pays nothing.
8. **`ReloadSession` gives the same record** (`collection_stats()`,
   `reset_collection_stats()`): a host of the dev tier holds a
   session and no Context pointer (§168 rule 9). It also gives
   `live_bytes()`, which the session did not have.
9. **Each Context has its own record.** A Worker Context is not
   reachable from the host of the spawning Context, and its
   collections are not in the record of that Context.
10. **The interpreter is not a host tier.** Its collections go
    through the same Context code, so the record exists; no test
    compares its times.

### 353.2 Acceptance

1. **Counts by class.** A runtime test: explicit collections and
   automatic collections (mode 2, and mode 1 with a small headroom)
   each raise their own count and not the other. The count of
   automatic collections equals `subscript_rt_automatic_collection_count`
   plus the count of detection collections: that entry does not count
   a detection collection, and rule 1 puts it in the automatic class.
2. **Words.** For a heap that the test builds (an array of N
   references to objects; an array of N numbers; a chain of N
   objects), `last_words_read` equals the count that the test derives
   from N and the layouts, or, where a root set of the runtime makes
   an exact figure depend on the tier, grows by a derived amount when
   N doubles. Two runs give the same count. `max_words_read` is the
   largest.
3. **Time.** `last_ns` is above zero after a collection of a heap of
   1 MiB of references; `total_ns` is the sum of the `last_ns` values
   that the test read; `max_ns` is not under `last_ns`. No test states
   an upper limit of a time.
4. **Reset.** After a reset each field is zero, and the next
   collection gives counts of one.
5. **The two native tiers.** A ship program with a host hook, and a
   dev JIT session through `ReloadSession`: a script allocates in a
   loop with a small headroom; after the run the record has automatic
   collections above zero, a `max_ns` above zero, and
   `last_live_bytes_after` equal to `subscript_rt_ctx_live_bytes`
   read directly after an explicit collection.
6. **Header.** The generated header declares the struct and the two
   functions. A test of the header checks the size and the offset of
   each field against the Rust record (invariant 1).
7. **Rule 6.** No ambient declaration and no intrinsic reads the
   record (a check that `Context` has no such member).
8. **The curve, measured one time and recorded** (not a gate): the
   ship arena, the median `last_ns` of five explicit collections and
   `last_words_read`, with a live heap of 1, 4, 16, and 64 MiB, for three shapes: an
   array of references to small objects, a linked chain of small
   objects, and an array of numbers. The record states ns for each
   word and ms for each MiB of each shape.
9. `perf-gate --gate` meets each criterion. `tools/gate.sh full`
   passes.

### 353.3 Sections this one amends

- §342 rule 17 and the host header: the two functions join the memory
  functions of a Context.
- CLAUDE.md invariant 2 does not change: the record is the evidence
  for the control that it states.

### 353.4 Not in this section

- A callback for each collection.
- A script form.
- A histogram of the times, and the split of a time into its phases.
- An incremental or a generational collector. This section gives the
  measurement for that decision.

### 353.5 Phase Review

Form.

- The record is `CollectionStats` (`runtime/src/context/collection_stats.rs`),
  `subscript_rt_collection_stats` in the header, in the collection
  state of the Context.
- One bracket, `Context::run_collection`
  (`runtime/src/context/collection.rs`), reads the clock before and
  after each collection: the script and the host collection, a poll,
  the drain poll, an outermost exit, the detection collection, and the
  collections of the interpreter. A collection inside another one is
  part of the outer one. A collection that unwinds is not recorded.
- The count of the words: each drain of the mark work list adds the
  length of the list at its start and the words that marked objects
  push. The scan of a payload returns the count that its loop bound
  is. The mark loop does no new work for a word; it does one add for
  each marked object.
- The check walks of a debug build run before the clock is read and
  after the record ends (`run_own_collection`).
- `ReloadSession` has `collection_stats()`, `reset_collection_stats()`,
  and `live_bytes()`. A hot reload keeps the Context, so the record
  goes on; a reload after a failed initializer makes a new Context.

Measured.

| Item | Result |
|---|---|
| Words, an array of N references to objects of 32 bytes | 1 + header + N + N times the object: the object is 4 words in the dev kind and 6 in the ship kind |
| Words, an array of N numbers | 1 + header, for each N |
| Words, a completed task that a holder reaches | 8 in the dev kind, 12 in the ship kind; 7 and 10 for a task that only an outside holder keeps |
| Time of one collection over 2^17 references, release build | 10.5 ms in the dev kind, 17.5 ms in the ship kind |
| The same in a debug build, with the check walks in the time and out of it | dev kind 146.0 and 146.7 ms to 141.4 and 141.0 ms; ship kind 271.6 and 275.8 ms to 270.5 and 270.0 ms |
| 200 collections of 400,000 references, median ms, before the count and after | dev kind 44.09, 47.68, 44.37 to 44.66, 44.74, 45.20; ship kind 54.83, 54.85, 54.80 to 55.23, 55.60, 55.44 |
| Stress sweep, 614 entries | dev JIT 611 of 611, ship tier 609 of 609, interpreter 499 of 499; 47 to 48 s (about 45 s before this section) |
| `perf-gate --gate`, two passes | each criterion met: `a22` ship tier 1.33x and 1.33x, `collect` ship tier 1.15x and 1.16x |
| The curve of acceptance 8 | `specs/tracking/automatic-collection.md`, G9 |

Phase Review (a subagent with no context): no CRITICAL, 1 MAJOR, 9
MINOR. The review found each path of a collection inside the bracket
one time, the delivery of a failure outside the time, and each path of
the word count counted one time; it computed the figures of G9 again
from the layouts.

| Finding | Close |
|---|---|
| MAJOR: a debug build had two check walks of the heap in each time | the walks are outside the bracket; rule 4 states it |
| No test of the word count went through a completed task | a test with a derived count; it fails with the words of a completion not added |
| The record had no `#[non_exhaustive]`, and the two C records beside it have it | added |
| The cost statement of the test module was wrong | the size and the measured time |
| One statement of G9 did not follow from its figures | corrected: at equal words the cost depends on the shape |
| The tutorials did not state that the record is for one Context, what `max_ns` spans, and that the words differ between the tiers | added |
| The doc of `automatic_collection_count` did not state that it leaves out a detection collection; two texts of the header read wrong | corrected |
| Rule 5 stated more than a conservative scan gives | rule 5 states the exception |
| A collection on an internal-error path can record zero words | open item 3 |

`tools/gate.sh full` on the tree before this record:

```text
gate full 88619116b17f7ba7069235beb687e613482ae29f dirty:20 debug 4123/0/6 release 4104/0/6 skips 2/0 clippy 4/10/13 goldens-moved 0 exit 0
```

`tools/hygiene.sh`: exit 0.

### 353.6 Open

1. The record has no size field. A later field makes the struct
   larger, and a host that was built against the smaller one passes a
   buffer that is too small. The project keeps no compatibility with
   a former header.
2. The cost of one word grows with the heap (G9: 7 to 13 ns in a
   chain, 13 to 23 ns in an array of references, from 1 to 64 MiB).
   The cause is not measured.
3. On two internal-error paths (a detection collection of the
   interpreter that fails; a debug check of the array tails that
   traps) the record gets one collection with zero words.
4. The time of a debug build of the runtime is about 14 to 16 times
   the time of a release build, with the check walks out of it. A host
   reads times from a release build.
5. The interpreter calls `Context::collect` inside its own collection
   for an empty heap; the check walks of that call are inside the
   outer time. No test compares a time of the interpreter (rule 10).

