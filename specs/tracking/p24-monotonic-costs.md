# P24 — two monotonic costs under invariant 2. COMPLETE 2026-07-27

Contract: `specs/blocks/compiler.md` §22, with §22.5 recording what
landed. Two items carried forward from P21 and P23, scheduled together
because they are the same defect: something inside the Context grew
without bound in a way the program could not control, and no gate could
see it.

Neither was a bug in what the code computes. Both were bugs in what it
costs, which is why the corpus never noticed.

## A. The code-point table

`context::CODE_POINT_UTF8` was `[u32; 0x110000]` — **4,456,448 B**, the
largest symbol in any shipped binary and 7× the regex engine. It is now
BMP-only at **262,144 B**; astral scalars are interned per Context.

**The astral range was the whole cost:** 65,536 BMP scalars need
262,144 B, the 1,048,576 above need the other 4,194,304 B. The table
supplies an *address*, not a computation — `str_bytes` returns a
borrow, so a million scalars cannot have stable addresses for less than
a million entries.

Astral scalars became **ordinary allocated string handles** rather than
a widened tagged form. That keeps `str_bytes` free of a map lookup on
the BMP path and keeps the odd-tagged range from colliding with
16-byte-aligned allocations — verified exhaustively over all 1,112,064
scalars: every BMP handle is odd and at most `0x20001`, every astral
handle is 16-byte aligned and above it.

**Measured:** the matched-pair baseline went **4,832,952 → 605,992 B**,
a 4,226,960 B drop against 4,194,304 predicted, inside the ±64 KB band.
`regress` unmoved at 501,433 B.

## B. The dev-tier allocation map

The sweep walked every entry ever created; dead ones were skipped by
the condition but still iterated and still written. Sweep was therefore
proportional to **every allocation ever made**, not to what is live —
0.73 → 3.48 ms as entries grew 120,005 → 720,005.

Dead entries now live in their own structure the sweep does not walk.
With the live set held at 120,005 and total entries grown to 720,005:
**0.564 → 0.561 ms**, spreads under 0.4%.

**Retention is unchanged.** §8.1a bought it for
trap-on-use-after-delete, and dropping dead entries would have traded
that away. The Phase Review enumerated all twelve pre-P24 readers of
`Allocation::live` and matched each to its replacement, then probed the
shapes: double delete at distance 700,000, delete of an unowned
pointer, a handle retired by `collect()` rather than `unsafeDelete`,
Map and Set backing storage retired recursively, a RegExp handle, and a
handle inside a deleted array. All classify identically.

**Address reuse is impossible, and that is now proven rather than
assumed:** the dev tier frees only at Context drop, and 202,000 retired
addresses collided with no later allocation.

## The ship `tree` movement is not this phase's

Recorded because the first attribution was wrong and the correction is
the phase's most useful finding.

`tree` moved on both tiers. Only the dev-JIT movement is P24's.

| build | ship `tree` | dev-JIT `tree` |
|---|---:|---:|
| pre-P24 | 120.5 / 121.3 ms | 671.5 / 673.2 ms |
| pre-P24 + 8 B dead padding in `Context` | 119.6 ms | 672.8 ms |
| **pre-P24 + 64 B dead padding** | **92.2 ms** | 671.2 ms |
| **pre-P24 + 104 B dead padding** | **91.8 ms** | 672.8 ms |
| P24 | 92.5 ms | **460.5 ms** |
| P24 + full astral table restored | 92.8 ms | 471.4 ms |

`size_of::<Context>()` went **920 → 1024 B** — the three new fields add
exactly 104 B, and `Context` is `#[repr(C)]`, so every later field
shifts. **Dead padding alone reproduces the entire ship win**, and
restoring the 4.19 MB of static data does not undo it.

Two consequences:

- **A win a phase did not cause must not be recorded as one.** The
  benchmark commit refused the credit but named the wrong cause — "the
  only ship-visible change is 4.19 MB less static data". The instinct
  was right and the mechanism was measurably wrong.
- **The published ship `tree` row is layout-sensitive by ±24%** with no
  semantic content behind it. Read as a runtime property it is
  misleading.

The dev-JIT movement *is* the §22.2 mechanism, and the same A/B proves
it: padding does not reproduce it.

## Gate

`cargo test --offline --workspace` **604 passed, 0 failed, 1 ignored**
(a release-only timing probe); build zero warnings; **88 goldens**
byte-exact across dev-JIT ≡ ship-C-AOT ≡ golden; `tsc` exit 0; clippy
at its 16-warning codegen baseline; `git diff --check` clean. No
pre-existing accept `.expected` moved. a84–a87 pin BMP, repeated
astral, mixed and distinct-astral iteration; a88 puts the
never-swept-intern property under the differential gate. All five
goldens are byte-identical to node.

*(602 and 87 at the implementation commit; the review's MINOR fixes
added a88 and two runtime tests.)*

perf-gate emitted-C reads **1.54× / 1.53× / 1.52×** across three valid
runs against P21's 1.52× — not distinguishable. A fourth run was void
with emitted-C decaying 8.669 → 6.051 ms monotonically, which is a
warm-up ramp rather than noise; `--warmup 40` settles it. P24 changes
neither path the perf-gate workload executes.

`collect` improves modestly — dev-JIT 221.703 → ~214 ms — because the
workload's 20000×6 nodes are far too few to show the sweep growth §22.2
removed.

## Phase Review — 0 CRITICAL, 4 MAJOR, 8 MINOR

The engineering was found sound: the segregation preserves
retain-and-poison on every reader and every shape the review could
construct, the interning is correct on both tiers and cannot collide
with the tagged range, the sweep is flat, and the corpus matches node.

**All four MAJORs were about the record, not the behaviour** — an
unguarded 4 MB win, an unimplemented and unwithdrawn contract clause,
spec text still describing the deleted static, and the wrong cause
recorded for the ship movement. All closed.

**The 4 MB win was measured and unguarded**, which is the phase's own
version of the gap it was fixing. `regex-size-gate` asserted only the
matched-pair delta and the `regress` figure — and both sides of the
pair link the table, so its return cancels. Run against the pre-P24
tree with all 4,456,448 B present, the gate exited **0**. It now
asserts the absolute baseline, and that assertion was **watched firing**
against `400ab3b` (exit 1, "baseline side moved up by 4226960 B")
rather than assumed. A guard nobody has seen fire is the same gap.

**§22.2's `reserve` fold-in was withdrawn on measurement**, not
dropped: Part B removed its premise. The map's capacity grew
229,376 → 458,752 → 917,504 only because dead entries stayed in it, and
reserving the pre-P24 peak the finding pointed at is now the *worst* of
four settings — 215.524 ms against 208.092 as built.

## Carried forward

- **The published ship `tree` row is layout-sensitive by ±24%** with no
  semantic content behind it (`benchmarks.md` carries the caveat). No
  workload in the suite is insulated from `Context`'s size, and nothing
  detects when a row moves for that reason. A future phase reading any
  alloc/delete row as a runtime property will be misled the same way
  this one nearly was.
- **`for…of` over CESU-8 lone surrogates** yields three U+FFFD where
  node yields one lone-surrogate unit. Unreachable from valid source
  and unchanged by P24; the day a host-facing byte-to-string API
  exists, that API defines its own invalid-byte semantics.
