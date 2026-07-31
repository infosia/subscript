# Q32 — string-literal union aliases: evidence

Status: **landed and verified 2026-07-31** against `compiler.md` §24
and `collisions.md` Q32/C7. Origin: the downstream WebGPU binding
project's request (HANDOFF/REPORT exchange, R2). Contract committed
first; every §24.4 criterion re-run by the reviewer.

## §24.4 evidence (reviewer-run)

1. `a91-string-literal-union` runs byte-identical under both tiers
   (standing gate); the golden carries the member strings
   (`echo=uint16` … `twin=uint16`), including the same-membered
   twin-alias case.
2. `r87` (non-member literal → S100:9), `r88` (inline literal union →
   S011:7, unchanged from today's rule), `r89` (cross-alias
   assignment, same members → S100:13) pinned in the reject harness.
   `r89` is `tsc`-clean (checked standalone against the prelude,
   recorded in its header) — the strictly-narrower proof: stock
   TypeScript accepts the structural assignment, the language
   rejects it nominally.
3. `string_literal_union_equality_emits_an_integer_compare`
   (`codegen/tests/cemit.rs`) pins `return (left == right);` in the
   emitted C and asserts no string-equality runtime call at the
   comparison site.
4. No existing golden moved; gate 48 harnesses, 728 passed, exit 0,
   read directly; `tsc` gate exit 0 with `a91` in the include set.
5. Checker unit tests cover member/non-member/cross-alias/boundary
   rejection paths.

## Corrections during landing

The contract as first committed named the reject entries `r62`–`r64`
without checking the register — the reject corpus already ran to
`r86` (the reviewer's numbering error, caught in the implementer's
report). Renumbered to `r87`–`r89`; spec references corrected in the
same landing.

## Implementer decisions recorded

No new S-code: Q32 mismatches ride the checker's existing S100
mismatch path, inline unions keep S011. An alias requires at least
two distinct members (a single-literal or duplicate-member alias
stays S100). Boundary rejection covers mirror types and exported
signatures.
