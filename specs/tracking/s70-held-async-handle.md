# §70 — a held async handle, by reference count

Contract `340284e`, corrected at `51f5aa7` and `e3a7385`. Landed
`efce1ed`.

Origin: the owner asked whether a `@Shared`-style decorator with a
reference-counted handle relaxes the async restrictions. It does.

## What moved

C8 rejected a floating async call, so the result of an async call had
to be awaited at the call site. Holding a handle, storing it in an
array, and passing it are now legal. Dropping one without awaiting is
still rejected: `r100` and `r105` are rewritten to that form, because
a coroutine that never completes runs none of its effects.

No `Promise` object appears and no scheduler. C8's model is
unchanged. The surface was already `tsc`-clean, so this accepts more
of what `tsc` accepts and invariant 5 needs no gate change.

## Two measurements taken before the work was ordered

**Where the count lives.** The count is at the frame's byte offset 4.
The frame does not grow, the allocation header does not move, and no
emitted offset changes.

**A coroutine frame was never freed.** The emitted C for
`a93-async-chain` called `subscript_rt_free` zero times; a frame was
allocated with class id `CLASS_GENERATOR` and left to the Context's
lifetime. So this section is the first thing that frees one, and that
was recorded in the contract before the round ran rather than
discovered in a measurement afterwards.

    a program that awaits a million calls, peak RSS
      pin       256,819,200 bytes
      now        14,155,776 bytes      94.5 per cent less, 18.1x smaller

A test pins `live_bytes` at zero after the last decrement, with no
`Context.collect()`.

**Invariant 2 holds and improves.** This is not a collector: no
traversal runs and the free happens at a known decrement. The
invariant says a program that never collects is correct, merely
larger. Peak memory for the measured program is 18.1× smaller.

## A correction this session owes the record

The handoff said offset 4 was "four bytes of alignment padding that
nothing reads". **It was the reload epoch**, documented in
`runtime/src/context.rs` as an ABI contract with generated code. The
claim came from reading the struct declaration in `cemit.rs` without
looking for a writer; one `grep` in `runtime/` finds it.

The round took the wrong premise and handled it correctly: an async
frame's epoch moved to Context metadata, and a generator still uses
offset 4. The conclusion held and the reason recorded was wrong.

This is the third time in this arc that a claim of absence came from
reading one file. The other two: "the language has not decided what a
`for...of` does when its container changes", which `a80`'s own header
decides; and "a `SubFn` copy keeps sharing one environment", which
S009's "by value" contradicts. **Confirming that something is absent
costs more than confirming it is present, and this session spent the
same effort on both.**

## The third consumer

The interpreter had to agree, and it is what proves the rules are
implementable from the section rather than from a tier: 101/101 debug,
102/102 release. It gained a per-coroutine owner count, retain and
release, a cached completion value for a second holder, and handle
pack and unpack inside an array.

## Gates at `efce1ed`

Zero warnings. 1031 passed in debug, 1030 in release, no failures. The
full interpreter sweep ran. `cargo fmt --check` and the `tsc` gate
clean. clippy compiler 7, runtime 22, codegen 13. No pre-existing
corpus entry, golden, or `.expected` moved.

`a22` measures 1.34x, the same as the pin, so the count's increments
and decrements do not reach the performance gate. `a22` holds no async
handle, which is why — the check was still needed.

`a154`, `a155`, and `r157` are Red at the pin, verified against a
binary built from it (CLAUDE.md core principle 10).

## `a162` keeps one string allocation, in 2 to 10 runs of 100 — 2026-08-30

Status: **finding, 2026-08-30. Open.** No change is made here.

### Fact

`counted_store_corpus_matches_the_interpreter` (`codegen/tests/lir.rs`)
asserts `live_bytes == 256` after the dev-JIT run of
`a162-async-copy-sites`. The run measures 267 in 2 to 10 runs of 100.
The output matches the golden in every run, failed runs included.

### Evidence

Measured on `x86_64-pc-windows-msvc` at `0541c96`. A probe repeated the
identical run in one process and one thread, and counted each distinct
`(live_bytes, reserved_bytes)` pair.

| Program | Runs | Pairs |
|---|---|---|
| `a162`, release | 200 | (256, 384) ×195; (267, 411) ×5 |
| `a162`, debug | 200 | (256, 384) ×194; (267, 411) ×6 |
| `a162`, release | 300 | (256, 384) ×270; (267, 411) ×30 |
| no final `Context.collect()` | 50 | (331, 587) ×50 |
| no `toString` print | 50 | (256, 384) ×50 |
| last printed value 4 characters wider | 200 | (256, 384) ×196; (271, 415) ×4 |
| one extra print of a 5-digit value | 300 | (256, 384) ×277; (267, 411) ×11; (269, 413) ×12 |
| only the last value printed | 300 | (256, 384) ×285; (267, 411) ×15 |

**One allocation is retained, and it is one string.** `HEADER_SIZE` is
16 and `reserved_bytes` sums `layout.size()`, so each retained
allocation adds 16 plus its payload. Every excess pair holds
`reserved − live = 16`. The count is one, not eight.

**The retained payload is 8 plus the character count of one printed
value.** The last value is `256`, and the payload is 11. A last value
of `2560000` makes it 15. A string allocation is a length word and the
bytes.

**It is not the whole print history.** The variant that prints only the
last value still retains 11 bytes. The variant with an extra 5-digit
print retains 11 or 13, so the retained string is the last allocated
one or the one before it.

**The eight strings are the collectable set.** Without the final
`Context.collect()` the excess is 75 live and 203 reserved. 203 − 75 is
128, which is 16 × 8. The eight payloads are 8 + 1, 8 + 1, 8 + 2,
8 + 1, 8 + 1, 8 + 1, 8 + 1, and 8 + 3, and they sum to 75. Collection
frees all eight, or all but one.

### The gate run suggested a concurrency defect, and it is not one

The full `lir` test binary failed 2 times in 8 in release and 3 times in
8 in debug. It failed 0 times in 8 with `--test-threads=1`. The
single-threaded sample was too small to show the rate: the test runs
`a162` once.

Concurrency raises the rate. Eight threads that each ran `a162` 25 times
measured 34 excesses in 200 runs, against 5 in 200 sequential runs.

`run_jit_with_memory_accounting_and_native_libraries` calls
`execute_entry`, so this helper never forks. The defect is not specific
to the Windows in-process path. The arm64 reference machine passes
because one gate run makes one measurement, and the rate is under 10
per cent.

### What a fix must establish

`Context::collect` (`runtime/src/context.rs`) marks from `self.roots`,
`self.shadow`, the async frames, and each frame's completion storage. It
reads raw words, and it marks every word that equals a live allocation
address.

The shared lowering zeroes a shadow frame before it registers the frame
(`codegen/src/lower/func.rs`), so an uninitialized shadow slot is not
the cause. A slot that a dead local wrote, and a completion buffer that
is wider than the value in it, are both still candidates. Neither is
measured.

The fix must name the word that marks the last string. The marked set
must be a function of the program. `a162` must then measure 256 in 1000
consecutive runs, in both profiles.

### Why the standing gates did not report it

The differential gate compares tier outputs, and the output is correct
in every run (CLAUDE.md core principle 12). The accounting assertion is
the only witness, and it makes one measurement per gate run.

## `a162` retention: the word is named and cleared — 2026-08-30

Landed at `ebc46fd`.

### The word

The marker reached the retained string from the `exerciseCopySites`
coroutine frame, payload word 52, generated member `b21_child`. The
member held the address of the `copiedWork(8)` child frame after that
frame was released. A later `printNumber(256)` allocation sometimes
reused the address for its 11-byte string payload. The stale word then
equaled a live payload address, and the marker kept the string.

A completed await released its child frame and did not clear the
member. Allocator reuse changed the marked set without a program
change. The rate on the arm64 host was 0 in 10,000 pre-fix attempts;
its allocator did not reuse the address.

### The fix

Cranelift and emitted C now zero the child member immediately after the
release, on the direct await path and on the held await path (§68.2
rule 8: the storage scope is the value live range; §70.3 rule 3).
A hand-built LIR test pins the clears in both tiers.

`SUBSCRIPT_MARK_TRACE=<payload address|strings|all>` prints each root or
payload word that reaches a payload to stderr, with the root set, index,
word, value, and reached class. It is off when absent. A subprocess unit
test turns it on for a hand-built module and reads the records.

### Measured

| Profile | Runs at `507eaa6` | 267 bytes | Runs after the fix | 256 bytes |
|---|---:|---:|---:|---:|
| debug | 200 | 6 | 1,000 consecutive | 1,000 |
| release | 300 | 30 | 1,000 consecutive | 1,000 |

The release-mode assertion also measured 0 excesses in 300 further
consecutive runs on the arm64 host. No corpus or golden file moved.
Clippy 7/22/13. Release suite 1,120 passed.

## `ebc46fd` did not clear the retention on x86-64 — 2026-09-01

Status: **open.** Measured on `x86_64-pc-windows-msvc`. One attempt runs
`counted_store_corpus_matches_the_interpreter` one time.

| Commit | Profile | Attempts | 267 bytes |
|---|---|---:|---:|
| `a2beca7` | debug | 60 | 9 |
| `2f9ed28` | debug | 30 | 6 |
| `2f9ed28` | release | 60 | 2 |

Every failure measures 267. No other excess appeared. This is the shape
the section above names.

### The range holds no cause

`git bisect run` over `a2beca7..2f9ed28` returned `f850e3d` as the first
bad commit. `f850e3d` adds 24 lines to one tracking file and changes no
code. A documentation commit cannot introduce the defect, so the `good`
endpoint was wrong. The table above confirms it: `a2beca7` reproduces the
defect in 9 of 60 debug attempts.

### Why the fix measured clean

The "1,000 consecutive" row measured the arm64 host. That host produced 0
reproductions in 10,000 attempts before the fix. A host that cannot
reproduce a defect cannot measure its fix. The x86-64 Windows gate at
`a2beca7` ran the suite one time against a 15 per cent rate.

Rule: measure a fix on the host that reproduced the defect.

### What the pins do not cover

`cranelift_clears_completed_async_child_slots` and
`emitted_coroutine_clears_completed_child_slots` both pass at `2f9ed28`,
and the defect reproduces at the same commit. So the retained word is not
the word those hand-built modules clear, or the clear does not reach
`a162`'s shape. The next step must name the word again with
`SUBSCRIPT_MARK_TRACE` on an x86-64 host.

## The word is named: an array data slot past `len` — 2026-09-01

`SUBSCRIPT_MARK_TRACE=strings` on `x86_64-pc-windows-msvc`, 40 attempts of
`counted_store_corpus_matches_the_interpreter`. Attempt 15 measured 267.
Its trace holds one record that no clean run holds:

```
SUBSCRIPT_MARK_TRACE payload=0x1f2536eeaa0 class_id=4294967041
  source=payload class_id=4294967043 address=0x1f2538d3c20 word=0
```

`0xFFFF_FF01` is `CLASS_STRING`; `0xFFFF_FF03` is `CLASS_ARRAY_DATA`
(`runtime/src/context.rs:230`, `:294`). A clean run holds 14 records and
no `source=payload` record. The failing run holds 15. The one extra
record is the whole difference.

This is not the word `ebc46fd` cleared. That word was the coroutine
frame member `b21_child`, at payload word 52.

### The mechanism

`Context::array_pop` (`runtime/src/context.rs:3218`) decrements `h.len`
and copies the element out. It leaves the bytes of the vacated slot in
the `CLASS_ARRAY_DATA` allocation.

`Context::scan_payload` (`runtime/src/context.rs:2417`) reads `size / 8`
words, so it reads the whole allocation. It reads the slots past `len`.

`a162` runs the two in order:

```ts
popped.pop();      // data word 0 keeps the released frame's address
printNumber(256);  // an 11-byte string payload sometimes reuses it
```

`Context.collect()` then reaches the string from that word and keeps it.
267 - 256 = 11, the payload of the string `"256"`.

### The class, not the site

This is the second word of one class: a stale word inside a payload,
outside the live range of the value that wrote it. `ebc46fd` cleared one
site. `array_pop` is another. `shift`, `splice`, and a truncation each
leave the same shape, and a capacity that exceeds `len` leaves it after
every removal.

CLAUDE.md: a fix that closes named sites does not converge. So this file
states the class and one proposal. The proposal is the owner's to accept.

**Proposal.** The marker bounds an array data scan by the live range.
`CLASS_ARRAY` carries `data`, `len`, and `elem_size` in its header, so
the marker can push the data allocation with `len * elem_size` bytes
instead of its allocation size. The stale slots then leave the reachable
set for every removal operation at once, and no operation needs a clear.

Conservative marking stays conservative inside the live range
(`context.rs:39`). The change removes bytes that hold no live value from
the scan; it does not make the scan precise.

**Cost.** The marker must reach the data allocation through its
`CLASS_ARRAY` header, so a data allocation that a scan reaches by an
interior address needs the same bound. Measure that before the change
lands.

## §80 landed: array data past `len` is zero — 2026-09-01

Contract `d21e38f`, corrected at `7b52729` after review. Landed
`f4f489e`. The bounded-scan proposal above is rejected; §80.1 states
why.

### What moved

`array_pop` zeroes the vacated slot. `array_truncate` zeroes
`new_len * elem_size .. old_len * elem_size` (`shift` and `splice` go
through it). `Context::array_tail_violations` walks every live array in
both tiers and reports every nonzero byte past `len`, up to the end of
the data allocation's payload. A debug `collect` records an `Internal`
trap that lists every violation, and returns without collecting.

### Red at the pin

`popped_element_is_unreachable_after_pop` against the unmodified
`array_pop`, reported by the coding agent:

    thread 'context::tests::popped_element_is_unreachable_after_pop' panicked at runtime/src/context.rs:5106:9:
    dev: popped element
    test result: FAILED. 0 passed; 1 failed

The stale word is the popped element's own address, so the test needs
no allocator reuse.

### Review

One fresh review. MAJOR: the check stopped at `cap * elem_size` while
`arena_mark` reads the whole block payload; closed by the payload
bound. MINOR: a debug `panic!` in `collect` reaches the `extern "C"`
entry and aborts the host; closed by the trap. MINOR: a one-array test
could not fail on "only that array"; a second clean array added. MINOR:
silent returns on a malformed header; `debug_assert!` added. MINOR:
`array_pop`'s `# Safety` did not forbid a `dst` inside the storage;
added. Recorded, no change: the large-record walk is dead for a
32-byte header and stays for totality; the debug walk is O(blocks).

The first round zeroed `new_len .. cap` in `array_truncate` because the
contract's test 3 asked a same-length truncate to clear a stale word.
The test was wrong, not rule 2: a `cap` bound makes `shift()` cost
`cap` bytes per call. Contract test 3 rewritten; code reverted to rule
2.

### Gates at `f4f489e`, arm64

Debug 1166 passed, 0 failed, 1 ignored. Release 1164 passed, 0 failed,
1 ignored; `perf_gate_meets_every_threshold` ok. `cargo fmt --check`
exit 0. Clippy 7 / 18 / 13. `tools/hygiene.sh` exit 0 after two commit
messages were rewritten to drop a session trailer. No golden or
`.expected` moved.

Exit criterion 3 (1000 consecutive `a162` runs at 256 bytes) is the
x86-64 host's row and is open until the owner's Windows gate runs. This
host never reproduced the defect.

Found during the gate: an empty `corpus/accept/.claude/` directory made
`lir` tests fail with `no source files given`. The harness treats a
directory as an entry. The directories are removed; the harness
fragility is recorded here and not fixed in this round.
