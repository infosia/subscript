<!-- §80 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 80. Array data past `len` is zero

*(Owner decision, 2026-09-01.)* Origin: the `a162` retention that
`specs/tracking/s70-held-async-handle.md` names on 2026-09-01.
`array_pop` decrements `len` and leaves the bytes of the vacated slot.
The marker scans the whole `CLASS_ARRAY_DATA` allocation. A stale
word past `len` marks a string that reused the address.

### 80.1 Measured at `c84dd20`

1. `len` decreases at two sites, both in `runtime/src/context.rs`:
   `array_pop` and `array_truncate`. `arrops::splice` and
   `arrops::shift` move elements and call `array_truncate`. No tier
   writes `len` downward in generated code; emitted C inlines only
   `push`. The dev tier, the ship tier, and the interpreter call
   `subscript_rt_array_pop`.
2. Fresh data storage is zero. The dev tier uses `alloc_zeroed`. The
   ship arena zeroes a reused block of a handle-capable class and bumps
   from zeroed chunks. Growth copies `len * elem_size` bytes into fresh
   storage.
3. The marker marks exact payload addresses only: a map lookup in the
   dev tier, a grid check in the ship tier. So a data allocation is
   reached through the `ArrayHeader.data` word, or through a stale word
   that equals its base.
 4. `ForeignArrayData` (`codegen/src/interpreter.rs`) and the §14.3 / §27
   out-array forms hand the host a raw pointer into array storage. A
   host that writes past `len` breaks rule 1; rule 3 then reports it
   at the next debug `collect` as a trap. That is the early error the
   §27.1 rule ("never grows the array") asks for.
5. An array header is 32 bytes, so it never reaches `arena_alloc_large`.
   Rule 3 walks the large records anyway, so the walk stays total if
   the header grows. The debug walk is O(blocks below bump), which is
   the cost of a total check and is debug-only.

The tracking note proposed a scan bounded by `len`. Rejected: the stale
words stay in storage, and the result depends on which word reaches the
data (fact 3). The rules below remove the words.

### 80.2 The rules

**Rule 1 — the invariant.** For every live dynamic array, every byte of
its data allocation at offset `len * elem_size` and beyond is zero.
This is §68.2 rule 8 applied to array storage: a slot past `len` is
outside the live range of the value that wrote it.

**Rule 2 — the sites.** `array_pop` zeroes the vacated slot after it
copies the element out. Its `# Safety` clause forbids a `dst` that
overlaps the array's storage, because the zeroing follows the copy. `array_truncate` zeroes the bytes from
`new_len * elem_size` to `old_len * elem_size`. No other site changes:
facts 1 and 2 make these the only two that can break rule 1.

**Rule 3 — the total check.** `Context::array_tail_violations(&self)
-> Vec<ArrayTailViolation>` walks every live `CLASS_ARRAY` allocation in
either tier and reports every array whose tail holds a nonzero byte:
the array handle address, `len`, and the first nonzero byte offset in
the data allocation. The tail runs from `len * elem_size` to the end
of the data allocation's **payload** — the region the marker reads —
not to `cap * elem_size`: the ship arena rounds a block up to a power
of two and `arena_mark` scans the whole block payload. `Context::collect`
calls it under `cfg(debug_assertions)`; when the list is not empty it
records an `Internal` trap whose message lists every violation, and
returns without collecting. It does not panic: `subscript_rt_collect`
is an `extern "C"` entry, and a panic there aborts the host process.
Tests call the check directly and read the trap record. It reports
every violating array at once (CLAUDE.md workflow: a total check, not
named sites).

**Rule 4 — the tests build the violating form** (CLAUDE.md core
principle 9).

1. `popped_element_is_unreachable_after_pop`: allocate a reference
   class instance `x`, push its address into a rooted array, pop it,
   hold no other reference, `collect()`. `x` must be freed (`is_live`
   false). This test is Red at `c84dd20`: the stale word marks `x`.
   It is deterministic and needs no allocator reuse.
2. The same shape through `arrops::shift` and `arrops::splice`.
3. `array_tail_violations_reports_a_stale_word`: write a nonzero word
   past `len` into a live array's data through the raw pointer, then
   assert the check reports that array with the offset, and only that
   array. A second, clean array is live in the same Context, so a check
   that reports every array fails. Then push one element (the slot
   becomes live) and pop it (rule 2 zeroes it), and assert the check
   reports nothing. A
   truncate to the same `len` zeroes nothing: rule 2 bounds the zeroed
   range by the old length, because the bytes past it are zero by rule
   1, and a range bounded by `cap` makes `shift()` cost `cap` bytes
   per call.
4. Both tiers: the tests above run under the dev allocator and under
   the ship arena (`Context` with the arena enabled), because rule 3
   walks two different live sets.

**Rule 5 — the corpus witness.** No new corpus entry: no accept entry
can read live bytes from script. `a162` and its gate row
`counted_store_corpus_matches_the_interpreter` remain the corpus
witness. The three tiers share the runtime sites, so no golden moves.

### 80.3 Exit criteria (pre-registered)

1. Rule 4 test 1 is Red at `c84dd20` on a binary built from that pin,
   and Green after.
2. The total check reports the built violating form, and reports
   nothing across the corpus sweep in a debug build (rule 3 runs at
   every `collect`).
3. `a162` measures 256 bytes in 1000 consecutive runs on the x86-64
   host that reproduced the defect, in both profiles. This host is
   arm64 and did not reproduce it; that row is the owner's Windows
   gate.
4. No existing golden or `.expected` moves. Full gate, `tsc` gate,
   `cargo fmt --check`, and `tools/hygiene.sh` green. Clippy at the
   baseline 7 / 18 / 13.
5. `a22` holds its performance-gate value.

Both sections first stated the runtime baseline as `22`. Both hosts
measure `18`, and the `2f9ed28` Windows row measured `18` before either
section landed. `windows-portability.md` holds the measurement.
