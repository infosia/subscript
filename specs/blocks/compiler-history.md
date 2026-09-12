# Compiler and runtime — contract history

This file holds the parts of `compiler.md` that no longer state a
current rule: the revision log, a diagram a later revision replaced,
and sections that a later section resolved or superseded. Section
numbers are the numbers they had in `compiler.md`; a stub with the
same number stays there (since 2026-09-12, as its own file under
`specs/blocks/compiler/`) and points here, so a citation of `§19` in
a tracking note still resolves. Nothing here is edited; it is the text
as it stood when it moved.

Rule for moving text *(2026-09-06)*: when a section of `compiler.md`
is next revised, its dated notes older than that revision move here
under the same section number, and the section keeps the current
rule and the note of the latest revision only.

## Revision log, Rev 0 to Rev 25 (moved 2026-09-06)

Status: Rev 25, 2026-07-27 (Rev 0: 2026-07-22; Rev 1 moves the mobile link
spike from P3 to P0.5 — plan §8; Rev 2 adds the §6 P1 checker contract;
Rev 3 adds the §7 P2 runtime/JIT contract; Rev 4 adds the §8 P3
AOT/reload contract; Rev 5 scopes trap recovery; Rev 6 adds the §9 P4
measurement methodology; Rev 7 adds the §10 P4.1 optimization contract;
Rev 8 makes the ship tier C emission — §11; Rev 9 adds the §12 P5 binding contract; Rev 10 scopes dev-tier boundary-struct marshaling to arm64 — §12.3a; Rev 11 makes the crate build's C compilation target-portable so the workspace builds on Windows-MSVC — §11a; Rev 12 makes the runtime C toolchain clang-portable — §11b — and extends dev-JIT struct-by-value marshaling to Win64 — §12.3a — for a test-green Windows-x64 gate; Rev 13 inlines emitted-C growable-array element access — §10a; Rev 14 adds the §13 P6 production-C-header interop contract; Rev 15 adds the §14 P7 async/Future + remaining-shapes contract; Rev 16 adds the §8.1b P8 ship-tier arena allocator contract; Rev 17 adds the §15 P9 stdlib pointer; Rev 18 adds the §16 P14 narrow-numerics contract — `i8`/`u8`/`i16`/`u16`/`f16`, `f16` storage-only; Rev 19 adds the §17 P16 generated-API-reference contract; Rev 23, 2026-07-26, adds the §21 P21 allocation-path contract — fault injection and per-allocation attribution, superseding §18.2e; Rev 22, 2026-07-26, adds the §20 P20 trap-site-IR contract; Rev 21, 2026-07-26, adds the §19 P19 trap-unwind-parity contract — CRITICAL; Rev 20, 2026-07-26, contracts the host `subscript_rt_ctx_*` API retroactively and adds the §18.2 trap observer §18.1a host enter/exit, §18.1b the generated host header, §18.2b `subscript_rt_ctx_clear_trap`, and §18.2d memory accounting; Rev 24, 2026-07-27, adds the §22 P24 contract for two monotonic costs under invariant 2 — the 4.25 MiB code-point table and the dev tier's cumulative-allocation sweep; Rev 25, 2026-07-27, adds §22.5 What landed, including the measured correction that the ship-tier `tree` movement is `Context`'s 104-byte growth and not this phase). Contract for
the plan's P0.5–P5 phases
(`specs/subscript-project-plan.md` §6). Evidence lands in
`specs/tracking/<phase>.md`.

## §1 Architecture — the diagram and the lowering note as of Rev 25 (moved 2026-09-06)

The diagram `compiler.md` §1 carried until 2026-09-06. The ship-tier
line and the target list were superseded by Rev 8 / §11 (C emission)
and by the desktop targets of 2026-08-09; the "one HIR→CLIF lowering"
bullet by §11 and §68 (one HIR→LIR lowering, three consumers,
agreement by verification).

```
SWC parse (TS-subset front end, Rust)
  → semantic checker (C1–C8 + Q rules; rule-specific diagnostics, TS positions)
  → typed HIR
      ├─ dev tier: cranelift-jit (Windows/Mac), hot reload
      └─ ship tier: cranelift-object AOT → .o → Xcode / NDK link
                     (aarch64-apple-ios, aarch64-linux-android; arm64-only)
  both call one runtime crate across a C-ABI-stable boundary:
  Context memory (manual delete, explicit collect), values, strings,
  arrays, traps, coroutine state, Q14 numeric formatting
```

- One HIR→CLIF lowering serves both tiers; dev/ship semantics coincide by
  construction. *(Superseded for the ship tier by Rev 8 / §11: the ship
  tier is HIR→C→`clang` (LLVM), a second lowering, after P4 measured
  Cranelift AOT at 23× a C baseline. dev/ship agreement is then
  established by verification — the standing gate — not by construction.
  The dev tier is unchanged: Cranelift JIT with hot reload. The diagram's
  ship-target list is superseded too — §11 ships the arm64 mobile device
  triples and `x86_64-unknown-linux-gnu`; the "arm64-only" note predates
  the latter.)*

### 18.2e Per-allocation attribution — superseded by §21.2

**Contracted in §21.2**, folded into P21 with allocator fault
injection because both touch `Context::alloc` and the allocation
header. The finding that motivated it is kept below; the design lives
in §21.2.

The question was whether a live-allocation figure could name the
script variable behind it. **It cannot, and the obstacle is not
cost**: an allocation is not bound to a variable — values move between
variables and into fields and arrays — so "the variable naming this
allocation" is not well defined, and a leaked allocation is usually
reachable from no named variable at all, which is why it leaked.

What *is* available is better suited to the purpose. Each allocation
carries a 16-byte header holding a state word and a `class_id`, with
**four bytes unused**. And `alloc(size, class_id, pos_id)` already
receives `pos_id` — the compiler position-table index for the
allocation **site** — and currently discards it, using it only if the
allocation itself fails. Storing it costs no space and one `u32` store.

An allocation site discriminates where a variable name would not: a
loop allocating ten thousand times has one site and no useful name.

Sketch, to be settled when this is contracted:

```c
typedef void (*subscript_rt_alloc_visitor)(void* userdata, uint32_t class_id,
                                     uint32_t pos_id, uint64_t payload_bytes);
uint64_t subscript_rt_ctx_visit_live_allocations(
    const subscript_rt_context*, subscript_rt_alloc_visitor, void* userdata);
```

Two things to settle then, both of which the sketch does not answer:

- **The `class_id` and position tables are the compiler's, not the
  Context's.** §18.1 already records this for a trap's `pos_id`. A
  host cannot turn either id into a name without an artifact this
  repository does not currently emit — the same gap as §18.1b's
  missing header, and it should be closed the same way.
- **The extra store lands in the ship tier's arena path (§8.1b)**,
  which is performance-sensitive; that tier's justification is that
  emitted C is close to hand-written C. One `u32` store per allocation
  is expected to be negligible, but it is to be **measured**, not
  asserted.

## 19. P19 — trap unwind parity (CRITICAL) — RESOLVED 2026-07-26

**Everything in §19.1–19.4 below is the state that was found, kept in
the past tense it was written in rather than rewritten, because the
evidence is the reason the fix took the shape it did.** §19.7 records
what landed. The one item still open is the emitted-C ratio in §19.7.

Found 2026-07-26 by a fresh no-context investigation, opened as its own
phase because it predates P13 and P18 and is larger than either.

### 19.1 What is wrong

**The two tiers execute different amounts of code between a fault and
the stop, and therefore produce different output.** Measured: of 19
trapping programs, **14 differ in stdout bytes** between dev-JIT and
ship-C-AOT. Trap tuples agree in every case; only what happens before
the stop differs.

The bound is not "one statement". It is **the end of the enclosing
function**:

- a loop containing the fault **runs to completion** (5 and 4
  iterations measured), and the statements after it execute;
- execution **enters another script function and completes its body**;
- an array is **pushed to twice after the fault**, its length going
  0 → 2 where the dev tier leaves it 0 — so **live Context state
  diverges**, not only stdout.

**And the continuation path corrupts memory.** An out-of-range write to
a 320-byte `@CStruct` array element goes through `subscript_arr_at`, which
records the trap and then returns `subscript_scratch` — `static unsigned char
subscript_scratch[256]` — and the caller writes 320 bytes into it.
AddressSanitizer reports `global-buffer-overflow`. The dev tier branches
to unwind before the address is computed and never reaches the store.
**This write happens only on the post-trap path.**

### 19.2 Why the gate did not catch it

Three independent reasons, each sufficient:

1. `corpus.md`'s determinism rule requires every accept program to
   terminate with deterministic output, so **a trapping program cannot
   be an accept entry** and never reaches the golden comparison.
2. `corpus.md` states, for the `trap/` category added the same day,
   that a trap entry's "observable result is the trap tuple, **not
   stdout**". The gate was told not to look.
3. **The API discards it**: `run_jit` drops the Context on the
   `RunError::Trap` path, and `run_c_aot` returns `run.stdout` only on
   success. The 25 trap-parity tests in `codegen/tests/cemit.rs` use
   these two functions and compare tuples alone. Comparing pre-trap
   output is not currently possible.

So the founding invariant — dev-JIT ≡ ship-C-AOT, byte-exact — fails
for a whole class of programs that the gate structurally excludes.

### 19.3 The rule, and which tier is right

**The dev tier is the reference.** `collisions.md` C6 says a fault
means "the Context stops", and the dev tier implements that: `guard()`
branches to unwind at the fault point, and `trap_check()` follows every
call that can leave the Context trapped, selected by the shared
predicates `ArrFn::can_trap()`, `StrFn::takes_pos_id()`,
`MapFn`/`SetFn::can_trap()`, `NumFn::takes_pos_id()`.

**The ship tier consults none of those predicates.** `cemit.rs` never
references `can_trap()`; it emits a check after script calls,
constructors, value-position `pop`, generator `.next()` and
`JsonResult.value`, and after nothing else — not after `Callee::Str`,
`Arr`, `Map`, `Set`, `Math`, `Num`, `Foreign`, not after
statement-position `push`/`pop`, allocation, `delete` or string
formatting, and not on a loop back-edge.

The fix is structural, not a longer list: **both tiers consult one
shared predicate.** A trap-capable operation added later must become
checked in both tiers by construction, which is the only way this stays
fixed.

### 19.4 Two contradictions in this document, both to be resolved here

- §18.2a step 5 says generated code checks the flag "after every
  **script call**". §18.2b says "after every **fault-capable call** —
  so the *next* exported call unwinds immediately, executing nothing".
  These disagree with each other; the dev tier implements the second,
  the ship tier neither. **The second is correct** and §18.2a is to be
  amended, not the reverse.
- C6's "the Context stops" is true of the dev tier and false of the
  ship tier. C6 is right; the ship tier is wrong.

### 19.5 Staging — the gate first

**Stage B (Red) — make the divergence visible.** Return the stdout sink
on the trap path from both `run_jit` and `run_c_aot`; allow a
`corpus/trap/` entry to carry an `.expected`; amend `corpus.md`'s
"tuple, not stdout" rule; change the 25 trap-parity tests to compare
`(tuple, stdout)`. Expected outcome: **a large number of failures.**
That is the point — this stage is not expected to be green.

**Stage A (Green) — make the tiers agree.**

1. **Read the trap flag inline.** The ship tier's checks currently call
   `subscript_rt_ctx_trap_kind`, an out-of-line `extern` that the link cannot
   inline (no LTO). Measured, 50M iterations, arm64 `-O2`: an inline
   load of the flag costs **≈0.56 ns** per check against **≈6.4 ns**
   for the call — about 11×. Do this **before** raising check density,
   or the density increase pays eleven times over.

   The flag is at Context offset 0, already contracted
   (`Context::trap_flag_offset`) with a test proving the offset, and
   the dev tier already loads it directly. This makes emitted C assume
   the Context layout, which is a **deliberate exception** to §18.1b's
   rule that the generated header is the sole expression of the ABI;
   the exception is recorded rather than left implicit.

2. **Share the predicate** (§19.3) so both tiers check the same set.

3. **Inline the checks inside the helper functions.**
   `subscript_sdiv_*`/`subscript_udiv_*` (16 of them) and `subscript_arr_at`/`subscript_fa_at`
   **cannot be fixed as functions** — a C function cannot make its
   caller return. The check must be expanded at the call site. **The
   `subscript_scratch` corruption closes only this way**; widening the buffer
   would relieve the symptom while leaving a store executing after a
   fault.

**Benchmarks are a gate item here, not a formality**, unlike the two
phases before it: this phase changes emitted code on the hottest paths
(division, indexing) and the ship tier's justification is that emitted
C measures 1.05× hand-written C. Re-run `perf-gate` and the
cross-language suite, and record the rows. A regression is a finding,
not an acceptable cost, without an explicit owner decision.

### 19.7 What landed

**Fully green**: `cargo test --offline` reports 559 passed, 0 failed;
`codegen/tests/cemit.rs` 68 passed. All 11 of stage B's intentional
failures are fixed with no `.expected` touched and no assertion
weakened — including the accept-corpus goldens, which is the check that
non-trapping behaviour did not move.

- **`Callee::can_trap()` in `compiler/src/hir.rs` is the shared
  policy for *calls*,** consumed by both lowerings, and the dev tier's
  behaviour is unchanged by the refactor.

  *(Corrected 2026-07-26 after the Phase Review. This said a
  trap-capable operation added later is checked in both tiers "by
  construction rather than by remembering". That is true of `Callee`
  variants and **false of everything else**: about ten non-call trap
  sites — integer div/rem, index read and write, `JsonResult.value`,
  narrowing casts, use-after-delete, stale-coroutine, and the
  allocation-bearing literals — remain hard-coded separately in each
  tier. Both of this phase's own CRITICALs were instances of that
  duplication failing.)*

  **Finishing the structural fix is possible but is not a small
  predicate.** A shared `TrapSite` classification plus a both-tier
  coverage test is ~0.5–1 day; a real explicit checked-operation IR is
  ~2–4 days and a few hundred lines. A boolean predicate alone cannot
  express what these sites need — the proof-based elision of a proven
  index, the *two* resolutions a compound assignment requires, the
  several trap points inside a template or array literal, and the
  position each guard reports. Recorded as the open item rather than
  claimed as done.
- **One inline Context-layout assumption**, `*(const uint32_t*)ctx` at
  a single site in `cemit.rs` — the §19.5 exception to §18.1b, kept to
  one place so it cannot spread.
- **`subscript_arr_at`, `subscript_fa_at` and `subscript_scratch` are gone**, their checks
  expanded at the call sites. ASan on the 320-byte `@CStruct` case:
  before, `global-buffer-overflow`, `WRITE of size 320`, exit 134;
  after, clean. A regression test pins it.
- **No loop back-edge polling.** The implementer checked the dev tier
  rather than assuming: it has no unconditional back-edge poll either,
  checking exact fault-capable operations instead. Ship-only polling
  was measurably slower and was removed. Both tiers now stop at the
  fault for the same reason.

**P19 did not regress emitted C — it made it 2.4× faster.** Measured as
a controlled pair, both runs by the orchestrator, same machine, same
session, `--warmup 12 --timed 15`, both valid under the ±20% rule:

| tree | emitted-C median | C baseline | ×C |
|---|---:|---:|---:|
| pre-P19 (`2fda4a9`, git worktree) | 14.75 ms | 4.04 ms | **3.65×** |
| post-P19 (`8f8a851`) | 6.08 ms | 3.97 ms | **1.53×** |

The C baselines agree to 1.7%, which is the control that makes the
comparison mean anything.

The result is the opposite of the expected direction. *(The mechanism
originally recorded here — that `subscript_arr_at` was "an out-of-line call
per array element access" — was **wrong**, and the Phase Review
measured it: `subscript_arr_at` was `static`, and clang inlined it. Corrected
below from the emitted assembly.)*

What the old shape cost per access was not a call but everything the
**fallback pointer** forced: a null compare and `csel` choosing
between the returned pointer and `subscript_scratch`, the global address of
`subscript_scratch` held live, a reachable cold-arm `bl _subscript_rt_array_ptr`,
and — because that call is reachable inside the loop — an 80-byte
frame with callee-saved spills and reloads. The loop body was 82
instructions with a full spill set. Expanding the check at the call
site makes the out-of-range arm return immediately, so the cold call
becomes a tail branch and the frame, the spills, the `csel` and the
scratch reference all disappear: 39 instructions, no spills. `a22` is
matrix propagation, so indexing is its inner loop.

In one line: the helper cost **leafness, register pressure and alias
freedom**, not a call. Adding trap checks made the emitted C faster
because expanding them removed all four.

**The gap to 1.05× is closed as a question: there is no regression to
fix, and 1.05× is not a target to return to.** Bisected on the same
machine, with the trap checks emitted into `a22` counted directly:

| tree | out-of-line checks | inline checks | ×C |
|---|---:|---:|---:|
| pre-P13 (`6b5189d`) | **0** | 0 | 1.87× |
| post-P13 (`f3e1d5a`) | 15 | 0 | 3.74× |
| pre-P19 (`2fda4a9`) | 15 | 0 | 3.65× |
| post-P19 (`8f8a851`) | 1 | 24 | **1.53×** |

**The emitted C had no trap check at all before P13.** P13 added the
checking that C6 requires after a script call and paid for it in the
out-of-line form — that is the 1.87× → 3.74× step, and it was the price
of correctness rather than a defect. P19 then fixed the form and
widened the coverage: **25 checks against P13's 15, and 2.4× faster**.

Post-P19 is also faster than the tree that had **no** checks, because
removing `subscript_arr_at`'s per-access call outweighed adding 25 trap
checks.

So **1.05× was measured on an emitter that did not do the checking the
language requires**, and comparing 1.53× against it compares two
different correctness levels. The §3 threshold of 1.50× is very nearly
met with correct semantics, which is the comparison that means
something. CLAUDE.md's citation of 1.05× as the evidence for choosing C
emission should be read as historical; the decision it supports is
unaffected, since emitted C is still ~15× faster than the Cranelift
AOT path it replaced.

Cross-language timings are **not reported**: the run matched all nine
checksums but was voided by the harness on C-subject spread (27–97%)
even at 200 warm-ups. Reporting them would violate §9.

### 19.6 Rejected: `setjmp`/`longjmp`

A real unwind in the ship tier would cost nothing per check. It is
rejected: it skips `shadow_pop`, corrupting the shadow stack that the
GC roots depend on, and breaks the "generated code never unwinds"
property that `codegen/src/jit.rs`'s SAFETY comments rely on.
