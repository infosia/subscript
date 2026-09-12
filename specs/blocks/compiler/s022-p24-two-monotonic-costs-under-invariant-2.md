<!-- §22 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 22. P24 — two monotonic costs under invariant 2

Both items were found by measurement in earlier phases, recorded as
carried forward, and scheduled together on 2026-07-27 (owner) because
they are the same defect wearing different clothes: **something inside
the Context grows without bound in a way the program cannot control,
and no gate can see it.** One is binary size charged to every shipped
program; the other is `Context.collect()` time charged to every long-running
host.

Neither is a bug in what the code computes. Both are bugs in what it
costs, which is why the corpus never noticed.

### 22.1 The code-point table (`stdlib.md` §15.1, P23 carried forward)

`context::CODE_POINT_UTF8` is `[u32; 0x110000]` — **4,456,448 B**, the
UTF-8 bytes of every Unicode scalar at a stable address. It is the
largest single item in a shipped binary, **7× the regex engine**, and
every program that touches a string links it.

**It exists to supply an address, not a computation.** Encoding a
scalar to UTF-8 is a handful of instructions; `str_bytes` returns
`&[u8]`, a borrow, so the bytes must live somewhere addressable, and
the handle `subscript_rt_str_iter_code_point` hands out is a tagged integer
rather than a pointer into real memory.

**Its only consumer is `subscript_rt_str_iter_code_point`** — `for…of` over a
string, and `stdlib.md` §14.3's guarantee that the loop allocates
nothing. `charAt` calls `alloc_str` and always has. *(Both §15.1 and
the runtime's doc comment said `charAt`; corrected 2026-07-27.)*

**The astral range is the entire cost:**

| | scalars | bytes |
|---|---:|---:|
| BMP (`< 0x10000`) | 65,536 | 262,144 |
| **astral (`≥ 0x10000`)** | **1,048,576** | **4,194,304** |

#### What must hold

1. **BMP loses nothing.** Every scalar below `0x10000` — all Latin,
   Greek, Cyrillic, Arabic, Hebrew, Devanagari, kana, common CJK,
   Hangul — keeps its static-table address and allocates nothing. That
   is essentially all text.
2. **Astral is bounded by distinct scalars used, not by iterations.**
   A per-Context intern map: the first occurrence of an astral scalar
   allocates its bytes, every later occurrence returns the same handle.
   A ten-thousand-iteration loop over `"😀"` repeated allocates
   **once**.

   This is the bound `stdlib.md` §15.5a already gives the
   compiled-pattern cache, and it is chosen for the same reason: a
   per-iteration allocation under invariant 2 accumulates until
   `Context.collect()`, which is the defect §15.5a was written for.
3. **The intern map is Context-owned and is not swept.** No program
   reference reaches it, so a sweep would free bytes a live handle
   still points at. It lives and dies with the Context, like the
   compiled-pattern cache.
4. **`str_bytes` stays total.** A handle for an astral scalar that was
   interned in one Context must not be read against another.

**The honest bound, stated rather than glossed:** a program iterating a
string containing a million *distinct* astral scalars allocates a
million times. That is not a shape any real text has, and it is the
price of not carrying 4 MB in every binary.

### 22.2 The dev-tier allocation map (`p4-performance.md`, P21 carried forward)

The dev tier realizes `Context.free`/`Context.collect` by **retain-and-poison**
(§8.1a): freed bytes stay owned by the Context with a dead header, so a
stale handle traps instead of reading reused memory. §8.1a accepted the
*memory* cost of that retention as the price of the guarantee.

**What was not anticipated is that the retention also costs sweep
time.** Dead entries stay in the same map the sweep walks, so **sweep
is proportional to every allocation ever made**, not to what is live.
Measured inside one run as entries grow 120,005 → 720,005: sweep
**0.73 → 3.48 ms, linear**, while mark stays 14–16 ms because mark *is*
proportional to live data.

A host calling `Context.collect()` per level transition or inside a frame
budget therefore sees its collect cost **rise monotonically for the
lifetime of the Context, regardless of how much is live.** `a16`, `a51`
and `a70` allocate too little to show it; a real embedding would not.

#### The fix must not weaken the guarantee

The obvious fix — dropping dead entries — trades the diagnostic away,
and the diagnostic is what §8.1a bought the retention for. It is not
acceptable here.

**The defect is that dead entries sit in the swept structure, not that
they exist.** Segregate them: a sweep that walks only live entries is
proportional to the live set, and every dead entry stays exactly as
poisoned and as trappable as it is today. Memory retention is
unchanged — that cost §8.1a already accepted and this phase does not
reopen.

If segregation proves impossible without changing what traps, **stop
and report it** rather than bounding the poison window; a smaller,
correct win is preferred to a larger one that quietly narrows a
guarantee.

~~**Fold in the adjacent measured finding:** reserving the
`allocations` map avoids two rehashes, worth **~15 ms of 229 ms** on
the same workload.~~

**Withdrawn 2026-07-27, on measurement: the segregation above removed
the rehashes this clause was written to avoid.** Pre-P24 the map's
capacity grew `229,376 → 458,752 → 917,504` because dead entries stayed
in it; Part B leaves the live set only, and one bounded rebuild from
tombstone pressure.

Re-measured on the same `collect` workload:

| `reserve` | median |
|---:|---:|
| 0 (as built) | 208.092 ms |
| 120,005 | 204.300 ms |
| 240,000 | 201.089 ms |
| **720,005** — the pre-P24 peak this clause implied | **215.524 ms** |

**Reserving the figure the original finding pointed at is now the worst
of the four.** A 240,000 reserve does buy ~7 ms, but it pre-pays a
workload-shaped map on **every** dev Context, and Context construction
sits outside the measured span — so that number is a different trade
from the one this clause described, and is not adopted by default.

A clause whose premise a later change removes is withdrawn with the
measurement, not quietly dropped.

*(A third finding from that run — setting any explicit QoS class makes
`particles` 9.5% faster, 622 → 563 ms — is benchmark-harness hygiene,
not runtime behaviour. It belongs to `benchmarks.md`, not here.)*

### 22.3 Corpus and gate (pre-registered)

**Accept.** A `for…of` over a string of BMP scalars asserting the
existing output unchanged; one over a string of repeated astral scalars
(the interning case); one mixing BMP and astral; one over a string of
several *distinct* astral scalars. Each pinned on both tiers.

**Attribution, not a corpus entry**, for the allocation counts, since
the observable is host-side: a both-tier test asserting that iterating
`n` repetitions of one astral scalar performs **one** allocation, and
that BMP iteration performs **none** — using
`subscript_rt_ctx_visit_live_allocations` (§21.2), which already reports
`(class_id, pos_id, payload_bytes)`.

**Traps.** None new. A malformed handle is `str_bytes`'s existing
contract.

**Gate.** The standing differential gate byte-exact on both tiers;
`tsc` clean; no pre-existing accept `.expected` moves — a golden that
moves here means iteration output changed, which nothing in this phase
should do.

### 22.4 Exit criteria (kill or pass, pre-registered)

1. **Binary size drops by 4,194,304 B ± 64 KB**, measured by the
   `regex-size-gate` matched pair (`stdlib.md` §15.7). Both reference
   constants in that gate move; **that is the expected result, not
   drift**, and the commit must say so. A drop materially smaller than
   4 MB means the astral table is still reachable from somewhere and is
   a finding.
2. **BMP `for…of` allocates nothing**, asserted by the attribution
   test, and its emitted C is unchanged on the ship tier.
3. **Astral `for…of` allocates once per distinct scalar**, asserted by
   the same test across at least 1000 iterations.
4. **Sweep time is proportional to live entries, not to cumulative
   allocations.** Re-run the measurement that found it: as entries grow
   120,005 → 720,005 with the live set held constant, sweep must stay
   **flat within noise** instead of going 0.73 → 3.48 ms. A sweep that
   still grows linearly fails this phase.
5. **Every dev-tier use-after-delete that traps today still traps**,
   with the same kind, message and position, at every distance from the
   free — including one deleted before 700,000 subsequent allocations.
   This is the guarantee §8.1a bought and the one this phase is most
   likely to break.
6. **Benchmarks re-run.** Report emitted-C against P21's 1.52× and the
   `collect` workload against its recorded figure. A regression is a
   finding, not a cost to absorb.
7. Standing gate green; `tsc` clean; clippy at its baseline.

### 22.5 What landed

Both parts landed. Every §22.4 criterion was measured and met;
`specs/tracking/p24-monotonic-costs.md` carries the numbers. **One
clause of §22.2 did not land and is withdrawn there with its
re-measurement** — the map `reserve` fold-in, whose premise Part B
itself removed.

**The astral range is gone and the guarantee it bought is narrower than
§14.3 said.** `stdlib.md` §14.3 was headed "the loop allocates nothing"
and §22.1 quoted it as the property the table existed to serve. The
**iterator** costs nothing, which is what that section is about and
which is unchanged; the **element** now allocates once per distinct
astral scalar. `stdlib.md` §14.3a states the bound and §14.3's heading
no longer overclaims.

#### The ship-tier `tree` movement was `Context`'s size, not this phase

The `tree` benchmark — 30×131071 alloc/delete pairs — moved on both
tiers. Only one of the two movements belongs to P24.

**dev-JIT, ~673 → ~500 ms: the phase's own mechanism.** §22.2 moved
dead entries out of the live map, so every later allocation's hash
insert works against the live set rather than against every allocation
ever made. This is the cache-hostile growth §8.1a identified for the
ship tier and fixed there by releasing; P24 fixes it for the dev tier
without giving up retain-and-poison. Confirmed by A/B: padding the
pre-P24 struct does **not** reproduce it.

**ship, ~110 → ~93 ms: layout, and not this phase.** The Phase Review
established the attribution by measurement, against a first record that
named the wrong cause:

- Restoring `CODE_POINT_UTF8` to `[u32; 0x110000]` at P24's HEAD leaves
  ship `tree` unmoved — **92.828 ms** against 92.484 ms. So it is *not*
  the 4.19 MB of static data.
- `size_of::<Context>()` went **920 → 1024 B**; the three new fields
  add exactly 104 B and `Context` is `#[repr(C)]`, so every later field
  shifts. Inserting **104 bytes of dead padding** into the otherwise
  untouched pre-P24 struct reproduces the entire ship win — 120.5 →
  91.8 ms. 64 bytes suffices; 8 bytes does not.

**A win a phase did not cause must not be recorded as one**, and a
figure this sensitive must not be read as a runtime property: the
published ship `tree` row moves ±24% on a struct-size change with no
semantic content. `benchmarks.md` carries that caveat.

*(The first record of this, in the benchmark commit, said "the only
ship-visible change is 4.19 MB less static data, i.e. binary layout".
The instinct — refuse the credit — was right; the named cause was
measurably wrong.)*
