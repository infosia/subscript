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
