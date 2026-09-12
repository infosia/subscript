<!-- §24 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 24. Q32 — string-literal union aliases

Owner decision 2026-07-31 (collisions.md Q32, C7 exception; requested
by the downstream WebGPU binding project, HANDOFF/REPORT exchange).
The decision text lives in Q32; this section is the implementation
contract.

### 24.1 Checker

`type Name = "a" | "b";` at module level declares a closed literal
set, nominal by alias. The checker: accepts member literals in
alias-typed contexts and rejects non-members; treats two
same-membered aliases as distinct; accepts `===`/`!==` between
same-alias values and against member literals, rejecting comparisons
with `string` or other aliases; rejects Q32 aliases in boundary
(mirror) signatures. Inline literal unions keep today's S011
rejection. The rejection codes are whatever the checker's existing
mismatch paths produce — pinned by the corpus entries, not minted
anew unless no existing code fits.

### 24.2 Lowering (both tiers)

An alias value is an `i32` discriminant, member index in declaration
order. Each alias carries a static string table used only when a
value is formatted (template literals); `===`/`!==` lower to integer
compares. The two tiers agree byte-exactly on the printed member
strings (the standing gate).

### 24.3 Corpus

`a91-string-literal-union` (accept): two aliases, one pair sharing
member spellings; assignment, parameter passing, equality against a
literal and a same-alias value, and template-literal printing —
golden output contains the member strings. `r87`: non-member literal
in an alias context, (code, line) pinned. `r88`: inline literal
union in a parameter annotation, S011, pinned. `r89`: assignment
across same-membered aliases, `tsc`-clean, pinned.

### 24.4 Exit criteria (pre-registered)

1. `a91` runs byte-identical under both tiers, member strings in the
   golden.
2. `r87`/`r88`/`r89` pin (code, line); `r89` type-checks under stock
   `tsc` (the strictly-narrower proof).
3. A `cemit` unit test asserts an alias equality lowers to an
   integer compare — no string-comparison call at the comparison
   site in the emitted C.
4. No existing golden moves; full gate and `tsc` gate green; the
   zero-warning sweep is unaffected.
5. Checker unit tests: member accepted, non-member rejected,
   cross-alias rejected, boundary-signature rejection.
