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
