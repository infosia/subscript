<!-- §44 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 44. OBS-3 — scalar handle fields beside arrays in scratch-lowered structs

Owner decision 2026-08-02 (downstream observation OBS-3, accepted
as a bug; blocking its P5 slice E4). The downstream reported a
run-time abort — `misaligned pointer dereference … address … is
0x1` inside `Context::array_len` — building a render pipeline whose
nullable `fragment` member is **present**, the pointed-to
descriptor carrying a handle, a string, and two array fields. It
type-checks; the failure is at execution, dev tier.

### 44.1 Narrowing established before contracting

The downstream disproved two hypotheses by running them (plain
in-language descriptors/arrays/nullable members; an empty array
field crossing the boundary through its shipped surface). The
reviewer added two more findings at the pin:

1. **Empty arrays are not the trigger.** A probe with the fixture's
   existing §33 shape — present nullable fragment, `constants` and
   `targets` both empty — ran clean and produced selector output.
   `a106` already covers the same shape with non-empty arrays.
2. **The fixture has no struct combining a scalar opaque-handle
   field with array pairs**, and none reached through a nullable
   pointer member. `SubProbeBindGroupEntry` carries handles but no
   arrays; `SubProbePipelineLayoutDescriptor` carries a handle
   *array*, not a scalar handle field.

What remains, and what the fix must reproduce first: a
scratch-lowered struct carrying a **scalar handle field beside
array-pair fields** (with a string field), reached through a
present `_Nullable` struct-pointer member — the downstream's
`GPUFragmentState` (`module` handle, `entryPoint` string,
`constants`, `targets`).

### 44.2 Rule

The §32 recursive-lowering rule stands and is restated for this
composition: **a scratch-lowered struct's field offsets are the C
layout's, whatever mix of handle, string, scalar-pair, and nested
pointer members it carries, at any depth and behind any nullable
pointer.** No field kind may shift another's offset; an accepted
program that crosses the boundary runs. Where a composition cannot
be lowered it fails loud at compile time (§28's "lowered or loud"),
never as a run-time dereference of a mis-read field.

### 44.3 Corpus and fixture

The interop fixture gains the missing composition: a struct with a
string field, a scalar `_Nullable` handle field, and two array
pairs, reached through a `_Nullable` pointer member of an outer
descriptor, with selector-based evidence at every level (the §33
fixture convention). `a119-interop-handle-beside-arrays` (accept)
drives it: fragment present with the handle non-null and null,
arrays empty and non-empty, and fragment absent. Staging is
**Red-first**: the reproduction runs at the current pin and its
observed failure is recorded before any fix; the entry lands green
with the fix.

If the composition reproduces no failure, that outcome is reported
as-is and the exact downstream program is requested rather than
guessed at — a fixture that does not reproduce is evidence, not a
fix.

### 44.4 Exit criteria (pre-registered)

1. The pre-fix observation is recorded verbatim (the failure, or
   the fact that the composition ran clean).
2. `a119` byte-identical under both tiers, covering handle
   non-null/null × arrays empty/non-empty × fragment present/absent.
3. A lowering-level unit test pins the field offsets of the new
   composition against the C layout (the `offsetof` convention).
4. No existing golden moves; full gate green; `tsc` gate green;
   bindgen regeneration byte-compare green.

### 44.5 Round 2 — the remaining axis (2026-08-03)

`a119` did not reproduce (§44 tracking). The downstream then
reported that **both** tiers fail (dev-JIT aborts in
`Context::array_len`; ship-C-AOT takes SIGSEGV), with no program
output first, and supplied its mirror and generated conversion.
The reviewer reproduced six further construction shapes against the
existing fixture — helper-returned struct temporaries as
constructor arguments, `push`-built element arrays, string-bearing
elements, null and non-null pointer elements in one array, a
defaulted array member taking its default, and the maximal
combination — **all clean under the capture harness**.

One structural axis remains untested, visible only in the
downstream's C facade: the struct behind the nullable pointer
**inside an array element** is itself an aggregate of nested
structs (`SGPUBlendState { SGPUBlendComponent color, alpha; }`),
where the fixture's counterpart holds two scalars. §32's recursion
is pinned at other positions but never at this one: array element →
nullable pointer → struct → nested struct.

The fixture gains that depth; `a120-interop-nested-behind-element-pointer`
drives it Red-first with the same null/non-null and empty/non-empty
coverage. If it too runs clean, the fixture axis is exhausted and
the next request is the downstream's **preprocessed C facade
declarations** for these structs — the one artifact not yet
supplied, and the only remaining place the shapes can differ.

### 44.6 Round 3 — the difference is the unmarked reach-through pointer

The downstream supplied its preprocessed C declarations
(2026-08-03). Its measured layouts match what the fixture already
exercises — `SGPUColorTargetState` is 24 bytes with the pointer at
+8 after a 4-byte enum and its hole, and `SGPUConstantEntry` is a
`SGPUStringView` beside a `double`, both shapes the fixture
reproduces field-for-field. Layout is therefore **not** the
difference, and the enum/alias sizing question the reviewer raised
is answered: no.

The difference is a spelling. The downstream states, and its header
shows, that `_Nullable` appears **only on opaque-handle members**;
its reach-through struct-pointer members are written **plain**
(`const SGPUBlendState* blend;`), because Q13 already mirrors a
struct pointer as `X | null`. Every reach-through pointer member in
this project's fixture carries `_Nullable`; every *plain* struct
pointer in the fixture is the data half of a count/pointer pair.
**A single, unmarked, count-less struct-pointer member has never
been exercised** — in an array element or anywhere else.

Rule: **the reach-through lowering (§33) keys off the member's
shape — a count-less pointer to a registered boundary struct — not
off its `_Nullable` spelling.** Nullability annotation is
mirror-typing information (Q13), not a lowering trigger; a mirror
that types a member `X | null` and a lowering that rebuilds it must
agree by construction, or the boundary accepts and miscompiles
(§28's rule; the R6 lesson restated for the annotation axis).

Corpus: `a121-interop-unmarked-reach-through` (accept), Red-first —
the fixture gains an array element type whose reach-through pointer
member is written plain, matching the downstream header exactly,
and the observed pre-fix behavior is recorded before any fix. If a
plain member is genuinely unlowerable, it fails loud at compile
time (§28) — never at run time.

Exit criteria: (1) the pre-fix observation recorded; (2) `a121`
byte-identical under both tiers; (3) a bindgen audit that no
registered struct-pointer member reaches the mirror without a
lowering, whatever its nullability spelling — the class, not the
instance; (4) no existing golden moves; full gates green.

### 44.7 Round 4 — two simultaneously-present pointer members

The downstream's bisection (2026-08-03) reframes the search. Its
matrix, each row a separate run: every configuration with **one**
nullable struct-pointer member present runs; the pair
`depthStencil` + `fragment` present **together** aborts, and keeps
aborting as blend, vertex buffers, and the layout handle are
removed. A by-value third member does not substitute for the
second pointer.

Then, with that pair fixed, varying only a semantically unrelated
**by-value** member flips the outcome non-monotonically: omitted
aborts, `{}` aborts, `{topology}` runs, `{topology,
stripIndexFormat}` aborts. Label length does not matter, so it is
not total bytes. A neighbouring member's *contents* changing the
outcome is the signature of a **scratch sizing or indexing error**,
not of an unsupported construct — and it explains why three
purpose-built fixtures passed: the fault needs a particular scratch
profile to surface.

Confirmed at the pin: **no fixture struct carries two or more
count-less reach-through pointer members.** Every §33 shape has
exactly one; the arc varied the pointer target's depth, never the
number of simultaneously-present pointers.

Rule: **the scratch construction is correct for any number of
simultaneously-lowered members.** Each lowered position owns
storage that no other position can reach, whatever its neighbours'
kinds, contents, or sizes; scratch identity is never a function of
a sibling's payload. This is §32's recursion rule extended from
depth to breadth.

Corpus: `a122-interop-two-pointer-members` (accept), Red-first —
an outer descriptor with **two** count-less reach-through pointer
members present at once, each target itself containing a nested
aggregate and an array pair, with a by-value member between them
whose contents vary across the entry's constructions (the
downstream's `primitive` axis: absent, empty-equivalent, one field
set, two fields set). If the entry does not reproduce, the
scratch-profile axis is driven directly instead: a unit test over
descriptors with 1..N simultaneously-lowered members asserting
every position's storage is disjoint.

Testability defect, fixed in the same round: the JIT and C-AOT run
helpers buffer program output in memory and return it only on
success, so an aborting run loses everything it printed. The
downstream's "no output before the fault" was an artifact of this,
and it cost this investigation a wrong inference. Output already
produced must survive an aborting run in both helpers, with a test.

Exit criteria: (1) the pre-fix observation recorded; (2) `a122`
byte-identical under both tiers, or the disjointness unit test
standing in its place with the non-reproduction recorded; (3) the
run helpers surface partial output on abort, unit-tested; (4) no
existing golden moves; full gates green.

### 44.8 Round 5 — scale, and the hard-termination output gap

The downstream dropped its failing program and generated API layer
into the tree (2026-08-03, untracked evidence files) and reported
three facts obtained with §44.7's output fix.

**Located.** The run now prints three lines and ends at the
`createRenderPipeline` call — the fault is inside that conversion
and call, as its matrix said.

**Visible in code.** The generator's both-present arm passes
**seven** constructor arguments, two of which are separately built
aggregates; every other arm builds at most one. The descriptor
lowered by the following foreign call therefore carries, in one
tree: a string, a nullable handle, a nested state with array pairs
whose elements carry their own pairs, two by-value states, and two
reach-through pointer members whose targets themselves hold array
pairs with pointer-bearing elements. That is far more
simultaneously-lowered positions than any entry or test here has
built — §44.7's direct harness stopped at **six**.

**Mode-sensitive.** Hoisting the two aggregates into locals does
not remove the failure but changes how the run ends; adding more
locals changes it again. Combined with the earlier
`primitive`-contents flip, three independent observations now say
the behavior depends on the scratch profile's size and shape.

Rule (extending §44.7 from a small N to any N): **the scratch
construction is correct at any number of simultaneously-lowered
positions and any nesting depth, and no position's storage,
address, or size depends on how many siblings precede it.** The
existing 1..6 harness is raised to a scale that covers the
downstream's tree, and the two axes are exercised together rather
than separately.

Corpus and tests: `a123-interop-wide-descriptor` (accept),
Red-first — a single foreign call lowering a descriptor with the
downstream's profile: string, nullable handle, nested aggregate
with pointer-bearing array elements, by-value aggregates on both
sides, and two reach-through pointer members present at once whose
targets each hold array pairs. The direct harness is raised from
six positions to at least thirty-two and gains combined
breadth × depth cases.

**Testability, second defect (downstream Fact 4).** §44.7's fix
delivers output when a run ends through the non-unwinding panic
path but not when it ends by a hard signal — including lines
completed much earlier. Output already produced must survive
**however** a run ends, on both tiers, with a test per termination
mode.

Exit criteria: (1) the pre-fix observation recorded; (2) `a123`
byte-identical under both tiers, or the raised harness standing in
its place with the non-reproduction recorded; (3) output survives
each termination mode, unit-tested; (4) no existing golden moves;
full gates green.

### 44.9 Round 6 — module size decides observability

The downstream demonstrated the axis rather than suspecting it
(2026-08-03). Taking a variant that runs cleanly and appending N
functions of the form `function padN(v: u32): u32 { return v + N; }`
to the module — called by nothing, touching nothing — flips the
outcome, non-monotonically: baseline runs, +20 runs, +40 ends
early, +60 ends early, +80 runs, +100 ends early. This is the third
independent non-semantic knob, alongside the `primitive`-contents
flip and the termination mode moving between panic and hard signal
when arguments were merely hoisted into locals.

Conclusion adopted: **module size and layout decide whether the
fault is observable.** Five faithful small fixtures passed for this
reason, and no further small fixture will settle anything.

Rule: **a program's boundary lowering does not depend on the
module's unrelated content.** Adding declarations that nothing
calls cannot change the behavior of an unrelated foreign call, at
any module size.

Two lines of work, both required:

1. **Run the downstream's program here.** It dropped four files as
   untracked evidence — the failing program, its generated API
   layer, the facade header, and the generated mirror. A stub `.c`
   satisfying the header suffices, because a lowering fault does
   not depend on what the C functions do. This replaces
   reconstruction with execution.
2. **A self-contained entry for the class**: a program exercising
   a descriptor with two simultaneously-present reach-through
   pointer members, swept over a padded module of uncalled dummy
   functions (N = 20…120, step 20). The corpus entry pins the
   outcome as invariant across N; the sweep itself belongs in a
   test harness rather than in one golden.

**Output retention, scope defect (downstream round-6 datum).**
§44.8's retention is gated on an environment variable naming a
parent-owned file, so an embedder calling the run helpers directly
— which is how the downstream drives its runs — still loses
everything on a hard signal. Retention must be the default
behavior of the run helpers, without opt-in, and must be reported
as part of the run's error value rather than left for the caller
to discover.

Exit criteria: (1) the downstream program built and run here, with
its observed outcome recorded verbatim whether or not it
reproduces; (2) the padding sweep run and its results recorded;
(3) if a defect is found, fixed with the class pinned; (4) run
helpers retain output on hard termination with no opt-in,
unit-tested; (5) no existing golden moves; full gates green.

### 44.10 Retention needs address-space isolation (2026-08-05)

§44.8 criterion (3) and §44.9 criterion (4) require output retention on
both tiers. Neither states a platform limit. One limit exists.

**The ship tier retains output on every platform.** `run_c_aot*` builds
an executable and runs it as a child process. The parent reads that
child's output after any termination. No host address crosses the
process boundary, because the child links the native library's C
sources.

**The dev tier retains output only where `fork` exists.** The dev tier
runs JIT code that calls caller-supplied native symbols. `NativeLibrary`
holds each symbol as an address in the caller's process
(`codegen/src/native.rs`). A fresh process cannot resolve such an
address. Only a child that inherits the address space can. `fork` gives
that inheritance, and Windows has no equivalent. On a non-Unix host the
dev tier therefore runs the program in the caller's process. A program
that ends its own process ends the caller. No output survives, and
`RunError::AbnormalTermination` is unreachable there.

This limit is structural. While a native symbol is an address in the
caller's process, no later work removes the limit.

Both criteria now read: **output survives each termination mode on each
tier that isolates the run.**

**Consequence for tests.** A test that asserts dev-tier retention must
obtain its run through one shared helper. That helper's return type must
express "this configuration does not isolate the dev run". A call site
that ignores that case must not compile. The rule and its reason are
§11c.3's: a copied guard is a forgotten guard. Measured on
`x86_64-pc-windows-msvc` 2026-08-05, `cargo test -p subscript-codegen
--test native_library` killed its own harness with
`STATUS_STACK_BUFFER_OVERRUN` (exit `0xc0000409`); two tests ended the
harness process and one failed by assertion.

A test that covers both tiers must keep its ship-tier assertion on every
platform. The exclusion applies to the dev-tier part alone. On a Unix
host the helper supplies the run for every test, and the tests compare
exactly what they compare today.
