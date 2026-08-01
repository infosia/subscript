# §29 — AI-facing generated reference: evidence

Status: **landed and verified 2026-08-01** against `compiler.md` §29.

## §29.4 evidence (reviewer-run)

1. One offline command (`generate-api-reference`) regenerates all of
   `generated-docs/`; the gate byte-compares all three files
   (`generated_ai_references_are_byte_identical` beside the §17.4
   API-reference gate).
2. `language-reference.md` (404 lines): all 15 S-codes and 3 W-codes
   with `explanation()` text and machine-excerpted, harness-pinned
   rejection examples; the curated feature blocks were fact-checked
   by the reviewer against the landed contracts (Q32/Q33/Q34, sized
   numerics, memory model, W-codes) — no inaccuracies found.
3. `corpus-index.md` (269 lines) covers every entry: 99 accept, 96
   reject, 3 warn, 46 trap; missing headers fail the generator loud,
   so the header convention is now enforced. 48 entries received
   header-only fixes; the implementer verified non-header text
   byte-identical mechanically, and no golden moved.
4. Gate 48 harnesses, 761 passed, exit 0, read directly; `tsc`
   exit 0.
5. `llms.txt` (20 lines) names the read order, the check/run/bind
   loop, and steers agents away from `specs/` for current-state
   answers.

## Implementer decisions recorded

"Two lines of context" = two before and two after the pinned line;
S100's excerpt comes from the first pin in harness order (`r62`);
multi-file `a19-modules` gets one index row per source file;
`t07`'s header stayed single-line to keep its pinned trap position.
