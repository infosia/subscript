<!-- §86 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 86. C emission is linear in the function it emits

*(Owner decision 2026-09-05. Rev 1, 2026-09-05: the rules below
replace the first text after the measurement of 86.3 item 1; the
first text assumed values held to exit, and the fixture holds
none.)* Origin: the development-cost review of 2026-09-05
(`specs/tracking/development-cost-review-2026-09-05.md`, finding 2).

Measured at `97c9991`, this host (Apple M2, 8 cores), release
libraries, the §44.8 breadth fixture
(`codegen/tests/boundary_scratch_breadth.rs`), one function and one
block per program, one run per width. Seconds:

| Stage | Width 16 | Width 32 | Site |
|---|---:|---:|---|
| HIR→LIR lowering + verification | 0.011 | 0.070 | |
| `root_storage::plan`: `value_interference_with` | 0.198 | 6.27 | `root_storage.rs` 187 |
| `root_storage::plan`: slot loop | 0.004 | 0.10 | `root_storage.rs` 290 |
| `root_storage::plan`: tail (clear sets) | 0.190 | 4.67 | `root_storage.rs` 323 |
| `Body::new` before coalescing | 0.151 | 5.00 | `cemit.rs` 2379–2450 |
| `coalesced_value_storage` | 0.551 | 17.3 | `cemit.rs` 2107, `root_storage.rs` 165 |
| `Body::new` delayed-declarations loop | 0.058 | 1.75 | `cemit.rs` 2468 |
| `Body::emit_graph` | 0.448 | 7.19 | `cemit.rs` 2921 |
| every other stage together | < 0.02 | < 0.07 | |
| `emit_c` total (round 1 run) | 0.975 | 27.2 | |

LIR instructions 11,072 → 59,520 (5.4×); values 6,928 → 35,104;
root slots 505 → 1,777; `address_taken_values` 0 at every width;
maximum live set 1,619 values; each interference graph holds
62,810,096 entries at width 32; maximum resident set 1.59 GiB. Each
named stage grows 16× to 33× where the instructions grow 5.4×. The
shape of every one: a per-instruction or per-value step that
touches the live set or scans the block.

- `value_interference_with` adds one `HashSet` entry per (result,
  live value) pair and stores the graph as `Vec<HashSet<ValueId>>`.
- `value_interference` (coalescing) computes the same graph again and
  copies it through the origin groups (line 177).
- The plan tail clones the live `BTreeSet` twice per instruction
  (`live_after`, `occupied_during`).
- The delayed-declarations loop scans the block's instructions once
  per value (line 2471).

### 86.1 Rule

1. **One interference computation per function, at the origin
   level.** `Body::new` computes it once and hands it to storage
   planning and to coalescing. Coalescing maps a value to its origin
   before it asks; no graph is expanded through origin members.
2. **Interference is a query over live intervals, not a stored
   edge.** The reverse walk that today records edges records, for
   every origin and block, the intervals over which the origin is
   live: an interval starts at a defining instruction (or at block
   entry for a live-in) and ends at the last use in the block, the
   terminator, or the block exit for a live-out; a result and its
   operands overlap at the defining instruction, so a result
   interferes with its operands and its invalidated values.
   `interferes(a, b)` is "one interval of `a` and one of `b` overlap
   in one block", plus the two parameter rules (block parameters and
   function parameters interfere with each other and with the
   values live at the block's or the function's entry). The
   relation is the one the edge graph holds today, under rule 2a.
2a. **Root-storage liveness is the form's liveness.** *(Added
   2026-09-05, forced, after the migration control of 86.3 item 3
   stopped at its first disagreement.)* A value is live for root
   storage where the LIR says it is live: `liveness.live_ins`,
   instruction operands, and terminator `value_uses()`, plus
   `Suspend.invalidates` at the suspending terminator (§73 rule 2:
   the suspended operation holds those arrays). An instruction's
   `invalidates` list is not a mention for root storage: it names
   the arrays whose addresses go stale (§68 rule 9) and the lowering
   fills it with every array value in scope (`codegen/src/lir.rs`
   `invalidates: self.array_values.clone()`), live or dead.
   Measured at `f683a72`, `a117-descriptor-literal-nullable-member`
   `main`: the walk at `root_storage.rs` lines 230 and 372 inserted
   the invalidated array `%42` into the live set of blocks 4 and 6
   where the form's live-in holds only `%50`, so `%42` held its root
   slot to the last call of each block after its last read, and the
   graph lacked the edge (`%42`, `%50`) that the walk's own live set
   at block 4's entry implies. The two derivations disagreed inside
   one function. The walk's extension is deleted, not propagated:
   root storage reads no liveness the form does not carry (core
   principle 8). The edge "result interferes with the values its
   instruction invalidates" goes with it; a live invalidated array
   is in the live set already. A root slot of a dead array is
   cleared at the array's last read, which is earlier than before;
   an entry whose golden moves under this rule is named in the
   tracking note with its output (rule 6).
2b. **An address operand mentions its base.** *(Added 2026-09-05,
   forced, after the control's second stop.)* The mention set of an
   instruction is what `record_operand` records: each value operand,
   and for an address operand its `array_base`, because the address
   keeps the base rooted while the instruction runs (a159). A result
   interferes with every value in that set, not only with the direct
   value operands. Measured at `75e96a6`, `a03-integer-literals`
   `main`: `%9 = LoadAddress(%8)` where `%8` has base `%0`; the
   walk inserted `%0` into the live set at that instruction, and the
   edge loop added `(%9, %8)` and not `(%9, %0)`; the interval query
   answered `true`. The edge graph's operand loop is corrected to the
   mention set for the control round; the interval query is the
   definition.
3. **Per-point facts come from interval ends.** `clear_after_instruction`
   lists, at instruction `i`, the slots of origins whose last
   interval in the block ends at `i`; `clear_at_block_entry` comes
   from the live-in set and the slots. No per-instruction copy of
   the live set exists.
4. **A per-value lookup uses an index built once.** "The instruction
   that defines value `v`" and "the block of value `v`" are a table
   built in one pass over the function. No loop over values scans a
   block.
5. **The bound.** Each pass of storage planning, coalescing, and C
   emission runs in time proportional to the instruction count plus
   the interval count of the function, plus one term proportional to
   the values per block for a per-block set, and a logarithmic
   factor for an ordered lookup. A pass whose time at width 32 is
   more than 2× the instruction ratio of its time at width 16 is a
   defect of this section, reported with the site.
6. **The plan is the same plan.** For every corpus entry and for
   the fixture at widths 8 and 16, `value_slots`, the slot list, the
   clear sets, and the coalesced storage are identical to today's
   under rule 2a, and the emitted C is byte-identical. The migration
   control of 86.3 item 3 holds both derivations side by side for
   one round: the edge graph with rule 2a applied is the reference
   for the interval query, and the edge graph before rule 2a is
   compared once against the graph after it to count the functions
   whose plan rule 2a changes.

### 86.2 Sites

Task A (`codegen/src/root_storage.rs`, and the two call sites in
`codegen/src/cemit.rs`): rules 1–3, 6; and, under rule 5,
`Coalescing::try_merge` (`cemit.rs` 2073: a group holds one merged
interval list and its parameter-rule memberships, and a merge test
is one sweep of two lists) and the parameter rules of
`Interference::interferes` (`root_storage.rs` 276: a query visits
only the blocks where one of the two origins is a parameter, through
an index built once). *(Added 2026-09-05 after the Task A review:
both were superlinear, one measured at 12.6× for 5.4×, one invisible
to the one-block fixture.)*
Task B (`codegen/src/cemit.rs`: `Body::new` before coalescing —
`fixed_iterator_values`, `address_definitions`,
`foldable_local_addresses`, `promoted_local_values`,
`removable_block_parameter_copies` — the delayed-declarations loop,
`Body::emit_graph` and `emit_dominator_subtree`): rules 4–5.
`codegen/tests/boundary_scratch_breadth.rs`: unchanged assertions.

Task C *(added 2026-09-05 after the Task B review; the one-block
fixture cannot measure these)*: `value_used_from` (`cemit.rs`
~1966, one CFG walk per edge argument with an unused parameter —
`O(uses × blocks)`; derive the predicate once per value from a
reachability table built once, or from `liveness.live_ins` of the
target); `suspend_state` (`cemit.rs` ~3697, a scan of the block list
per emitted suspend; one table built in `Body::new`);
`EmissionIndex::build` range-checks every LIR id once and returns
`Result` (core principle 5); a hand-built index test per table
(`use_blocks`, `local_seeds`, `first_stores`, `parameter_blocks`,
`definition_blocks`) and one more group-test query
(`O3: b0[4,5]` → `true`). Recorded, not assigned:
`declaration_scopes` clones the reachable-block set per block
(`O(blocks²)`), pre-existing and outside every task's list; it
becomes a site when a fixture measures it.

### 86.3 Corpus and gate (pre-registered exit criteria)

1. **Profile before the fix** — done at `97c9991`; the table above
   is the record, and `specs/tracking/s86-c-emission.md` holds the
   full tables.
2. **After Task A and after Task B**, the same stage table, same
   method, widths 16 and 32, release. After both: `emit_c` at width
   32 is at most 2× the instruction ratio (5.4) times the width-16
   figure, and at most 5 s in release on this host; the debug
   `boundary_scratch_breadth` test is under 120 s alone; the
   maximum resident set at width 32 is under 300 MiB. Each figure is
   in the tracking note with the host.
3. **Migration control** (core principle 11): for one round, the
   interval query and the edge graph both exist, both under rule
   2a, and a test asserts
   for every function of every runnable corpus entry and for the
   fixture at widths 8 and 16 that the two relations are equal
   (every pair, both directions) and that the plan of rule 6 is
   identical. The test is deleted, with the graph, in the landing
   commit; the tracking note records its run.
4. `codegen/src/root_storage.rs` unit tests with hand-written
   expectations: (a) a two-block function with one live-in, one
   result whose operand dies at its definition, and one value that
   is live-out: the interval list per (origin, block), written out;
   (b) `interferes` on that function for every pair, as a
   hand-written matrix; (c) `clear_after_instruction` and
   `clear_at_block_entry` for it; (d) a block-parameter pair and a
   function-parameter pair. Each test is a hand-built LIR function,
   not a lowered program.
5. `tools/gate.sh full`: `goldens-moved 0`; the LIR text goldens
   unchanged; the C-text assertions in `codegen/tests/cemit.rs`
   unchanged.
6. The `collect` and `bound-call` perf-gate workloads stay within
   their §3 thresholds (the release gate runs them).
