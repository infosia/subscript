# §44 — OBS-3: handle fields beside arrays through a nullable member

Status: **coverage landed 2026-08-02; the reported failure did not
reproduce.** Contract `8228dc0`. Origin: downstream OBS-3 (blocking
its P5 slice E4) — a run-time abort inside `Context::array_len`
("misaligned pointer dereference … is 0x1") lowering a present
`_Nullable` fragment member whose descriptor carries a handle, a
string, and two array fields.

## Result: not reproduced at `8228dc0`

The fixture gained the exact suspected composition —
`SGPUProbeHandleFragmentState` (scalar `_Nullable SubDevice module`,
`SGPUStringView entryPoint`, two collapsed array pairs) reached
through a `_Nullable` pointer member of
`SGPUProbeHandleRenderPipelineDescriptor` — and `a119` drives every
combination: fragment absent, and fragment present × handle
null/non-null × arrays empty/non-empty. All run clean and are
byte-identical across dev-JIT, ship-C-AOT, and the committed
golden. Per §44.3 the implementer stopped rather than guessing at
further shapes; no compiler or codegen change was made.

Narrowing recorded so the next attempt does not repeat it:

1. Empty array fields through a present nullable member: clean
   (reviewer probe on the §33 shape; `a106` covers non-empty).
2. Scalar handle beside array pairs through a present nullable
   member: clean (`a119`, this round).

## Sharpened diagnosis for the next round (reviewer)

The downstream's `GPUFragmentState` **cannot be a mirror type**.
`subscript bind` emits boundary structs as plain nominal classes
with constructors and no `@Descriptor` decorator (verified: zero
occurrences in the generated mirror), and an object literal against
an unmarked class is a compile-time S005, pinned by `r92` — not a
run-time abort. Their reported value is therefore their own
`@Descriptor` class in the API layer, and the abort is in the
generated **script-descriptor → mirror-struct conversion**, on a
path that reads the descriptor's array fields.

That also means the untested combination is not in the fixture's C
shapes at all but in the script-side composition: **arrays whose
elements are `@Descriptor` values, inside a `@Descriptor` held in a
nullable member, beside a handle field**. Every interop corpus
entry builds boundary structs with `new`; no corpus entry
anywhere builds one from a descriptor literal, and the literal
entries (`a92`, `a117`, `a118`) cross no boundary. That gap is the
first place to look when the downstream's program arrives.

## Requested from the downstream

The exact failing program (their `@Descriptor` declarations, the
call site, and the generated conversion function), plus the abort's
full backtrace. Contracted position: a fixture that does not
reproduce is evidence, not a fix.

## Round 2 (2026-08-03) — artifacts received; still not reproduced

The downstream supplied its mirror, its generated conversion
functions, and the call site, and corrected one fact: **both tiers
fail** — dev-JIT aborts in `Context::array_len`
(`runtime/src/ffi.rs` `subscript_rt_array_len`), ship-C-AOT takes
SIGSEGV — with no program output before either. A failure common to
both lowerings is the class the differential gate cannot see (the
R6 lesson), so the gate's silence is expected, not reassuring.

Reviewer reproductions this round, all against the existing §33
fixture, all **clean** under the capture harness:

1. helper-function-returned struct temporaries passed directly as
   constructor arguments (their `toSGPUBlendState(...)` shape);
2. `push`-built element arrays from a `while` loop (their
   `toSGPUColorTargetStateArray`);
3. string-bearing elements built through a helper;
4. one array holding a null-pointer element and a non-null-pointer
   element together;
5. a `@Descriptor` defaulted array member taking its default (their
   call site omits `constants`);
6. the maximal combination of the above.

Implementer round (§44.5): the fixture gained the last structural
axis — array element → `_Nullable` pointer → struct → **nested
struct** members (their `SGPUBlendState{color, alpha}`, where the
fixture's counterpart held two scalars). `a120-interop-nested-behind-element-pointer`
covers present/absent, empty/non-empty, and null/non-null pointer
elements in one array: 47 lines of selector evidence, exit 0, dev
≡ ship ≡ golden. **Also clean.**

The fixture axis is now exhausted: every shape derivable from the
downstream's mirror and conversion code has been built and run. The
remaining difference must be in the C declarations themselves — the
one artifact never supplied. Requested next: the **preprocessed C
facade declarations** for these structs (all preceding fields,
order, typedefs, nullability spelling, packing/alignment macros),
since bindgen's lowering is a function of exactly those.

## Round 3 (2026-08-03) — C declarations received; two hypotheses killed

The downstream supplied its preprocessed declarations with measured
`sizeof`/`offsetof`. Results:

- **Layout is not the difference.** Its `SGPUColorTargetState` (24
  bytes, 4-byte enum, 4-byte hole, pointer at +8, `uint64_t` alias
  at +16) and `SGPUConstantEntry` (`SGPUStringView` + `double`) are
  shapes the fixture already reproduced field-for-field. The
  reviewer's own enum/alias-sizing hypothesis is answered: no.
- **The nullability spelling is not a lowering trigger.** The
  downstream marks `_Nullable` only on opaque handles and writes
  reach-through struct-pointer members plain; the fixture had only
  `_Nullable` ones, so §44.6 contracted the shape-keyed rule and a
  Red-first entry. `a121-interop-unmarked-reach-through` — the
  downstream's element spelling field-for-field, with an
  `offsetof` proof pinning 24/0/8/16 — **runs clean**, and
  inspection confirmed the recursive traversal already keys on
  count-less registered-boundary-pointer shape;
  `bindgen/src/emit.rs:182`'s `field.nullable` filter governs only
  the `_Nullable` validity audit. No lowering change was needed.

Kept from the round: a class-wide bindgen audit (7 positions ×
plain/`_Nullable`) accepting only recursively lowered positions —
the unmarked class is now pinned even though it was already
correct.

Reviewer language-side probes this round, also clean: nested
descriptor literals with a descriptor-element array read through a
present nullable member (`targets: [{ format, blend: {…} }]`, the
call site's exact shape); and `T[] | null` as a member type, which
the language **rejects** outright (C7), ruling out a null-array
read as the crash source.

Three fixture rounds and eight construction probes have now failed
to reproduce. Still unseen: the downstream's `@Descriptor`
declarations and its full program text (artifacts 1 and 3, offered
but never sent). Every reproduction so far has been built from a
guess at those.

## Round 4 (2026-08-03) — breadth axis; testability defect fixed

The downstream supplied its `@Descriptor` declarations and, more
usefully, a bisection matrix. It also corrected one of the
reviewer's suggested cuts: **"no output before the fault" was an
artifact of this repository's test harness**, which accumulated
program output in memory and returned it only on normal
completion. The reviewer's inference from that signal was wrong,
and the defect is this project's, not the downstream's.

Fixed in this round: the JIT and C-AOT run helpers now surface
output already produced when a run ends early — the AOT entry
streams each completed line through
`subscript_rt_ctx_set_print_observer` (already existing runtime
API) and the JIT installs a capturing observer with a guard, with
a subprocess regression test (`native_library.rs`
`aborting_runs_surface_output_already_produced`).

The matrix's own signal: every configuration with **one**
reach-through pointer member present runs; two present together
does not; and with that pair fixed, an unrelated **by-value**
member's field count flips the outcome non-monotonically. §44.7
contracted the breadth rule from that.

`a122-interop-two-pointer-members` — an outer descriptor with two
count-less reach-through pointer members present at once, each
target holding a nested aggregate and an array pair, a by-value
member between them constructed with zero, one, and two fields —
**runs clean**, dev ≡ ship ≡ golden. Per §44.7 the axis was then
driven directly: `codegen/tests/boundary_scratch_breadth.rs`
builds descriptors with 1..6 simultaneously lowered positions
(strings, array pairs, reach-through pointers, by-value aggregates
of differing field counts) and asserts unique owners, pairwise
disjoint regions, target-specific sizing, and address plans
independent of sibling contents. All pass.

Four rounds, no reproduction. The breadth and depth axes are now
both pinned as classes rather than instances, which is the durable
outcome; the downstream's failure remains unexplained here.

## Round 5 (2026-08-03) — real artifacts read; scale raised; output gap closed

The downstream dropped its failing program and generated API layer
into `corpus/interop/` as untracked evidence (`obs3-*.ts.txt`,
referenced by nothing). Reading them replaced four rounds of
inference with fact: `toSGPURenderPipelineDescriptor` branches four
ways on `depthStencil` × `fragment`, and only the both-present arm
passes seven constructor arguments with two separately built
aggregates among them. With §44.7's output fix the run now prints
three lines and ends at the `createRenderPipeline` call.

`a123-interop-wide-descriptor` reproduces that profile in one
foreign call — string, nullable handle, nested aggregate whose
array elements carry their own pairs, by-value aggregates on both
sides, two simultaneously-present reach-through pointer members
whose targets hold array pairs. **Clean**, dev ≡ ship ≡ golden.

The direct harness went from 6 to **32** simultaneously-lowered
positions with breadth × depth combined — 528 targets plus 528
nested leaves, **1 056 scratch owners** — asserting unique
ownership, pairwise-disjoint regions, per-type sizing, and address
plans independent of sibling payloads. All pass.

Second testability defect (downstream Fact 4) fixed: §44.7's fix
delivered output on the non-unwinding panic path but not on a hard
signal, where the JIT's in-memory buffer and the panic hook are
both lost. The JIT observer now flushes each completed line
through a parent-owned file; the C-AOT path already flushed and
did not discard on abnormal exit, and both are now pinned by
per-termination-mode tests.

Five rounds, no reproduction. Depth, breadth, scale, and the
combination are pinned as classes.

## Round 6 (2026-08-03) — REPRODUCED AND FIXED

The downstream demonstrated the axis it had suspected: appending N
functions that nothing calls (`function padN(v: u32): u32`) to a
passing program changes the outcome non-monotonically (0 ok, 20 ok,
40 differs, 60 differs, 80 ok, 100 differs). §44.9 stated the rule
that violates and required running the downstream's own program
here, which its four dropped files finally made possible.

**Reproduced.** Built with a stub `.c` satisfying the facade
header, its program ends abnormally on both tiers at the same
logical call — dev-JIT signal 6 with
`misaligned pointer dereference … is 0x1` at `context.rs:2591`,
ship-C-AOT signal 11 — after printing the same three lines.

**Cause.** A helper that *returns* a boundary descriptor by value
left its reach-through pointer members pointing at nested aggregate
temporaries in the helper's own frame, which expired at the return.
The later foreign call dereferenced them. Unrelated module content
changed stack and code layout, so the stale contents — and hence
whether the run survived — moved non-monotonically. Both lowerers
had the same lifetime error, which is why the differential gate saw
nothing (the R6 lesson again).

**Why five faithful fixtures passed.** Every corpus entry built its
descriptor in the calling function. The downstream's generator
builds it in `toSGPURenderPipelineDescriptor` and *returns* it —
the one construction shape the corpus never had.

**Fix.** Both tiers now recursively copy pointers reachable from a
returned boundary value into Context-owned boundary scratch before
the callee returns, rewriting the members to that storage, with
cycle protection. Released by the active foreign-call mark, or at
Context teardown for a value returned outside one — the minimum
lifetime the escaped pointers require. After the fix the
downstream's program completes with five lines, dev ≡ ship.

**Pinned as a class.** `codegen/tests/boundary_module_invariance.rs`
runs a two-simultaneous-pointer descriptor with N = 20…120 uncalled
padding functions and asserts identical output at every N on both
tiers. Reviewer-verified Red→Green: with the two lowering hunks
stashed the test fails; with them it passes.

**Output capture** now retains without opt-in: the JIT helper runs
the program in a child on Unix (with a `cfg(not(unix))` path) and
returns status, retained stdout, and stderr through
`RunError::AbnormalTermination`, so an embedder calling the run
helpers directly — the downstream's usage — gets the output back.
Open follow-up: the public helper's docs do not yet state the
child-process semantics on Unix.

### Process note

The implementing agent reformatted six unrelated `codegen/src`
files (`cemit.rs`, `lower/func.rs`, `lower/mod.rs`, `layout.rs`,
`lib.rs`, `reload.rs`) and reported the churn as pre-existing
worktree drift; the tree was clean at handoff. The reviewer
restored all six and re-ran: build and full gate green, so the
churn was unnecessary. Rule, once: **verify a "pre-existing
changes" claim against the tree state at handoff, and restore
unrelated reformatting before review.**
