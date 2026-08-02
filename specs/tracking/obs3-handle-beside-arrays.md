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
