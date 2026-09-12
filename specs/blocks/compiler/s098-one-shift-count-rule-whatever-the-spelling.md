<!-- §98 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 98. One shift-count rule, whatever the spelling

*(Owner decision 2026-09-09.)* Origin: the owner asked for three
decided restrictions to be reconsidered on their merits. This is the
second.

Q18 masks a shift count by the operand width minus one, on every tier,
and then rejects a **literal** count at or above the width. Its stated
reasons are that a constant over-shift is a typo, and that C4 already
rejects out-of-range literals.

Both fail on measurement.

**The rule catches one spelling out of four.** Measured at `e2a2b6d`
on this host, with `const x: i32 = 1`:

| Expression | subscript | node |
|---|---|---|
| `x << 32` | S008, exit 1 | `1` |
| `x << (16 + 16)` | `1` | `1` |
| `x << K` where `const K: i32 = 32` | `1` | `1` |
| `x << -1` | `-2147483648` | `-2147483648` |
| `x <<= 32` | S008, exit 1 | `1` |

A constant expression is not a literal, so it passes. A named constant
passes. A negative literal count passes, and by the typo argument it
is the same mistake. Factoring `32` into a name changes acceptance.

**C4 does not cover it.** `32` is representable in `i32`. The
diagnostic nevertheless renders C4's rule and reason:

```
= rule: Numeric literals must fit their context and be integral in an integer context.
= why: A literal takes the sized type of its context, so a value outside
  that range has no representation. (collisions.md C4)
```

The literal has a representation. It fits its context exactly. The
diagnostic also shows `Divergence::IntegerLiteralRange`'s example,
`const big: i32 = 3000000000`, which is a different problem. The rule
borrowed a code, a rule text, a reason, and an example that belong to
something else.

### 98.1 Rule

1. Every accepted integer shift normalizes its count by the operand
   width, as Q18's mask already states: `x op k` shifts by
   `k & (width - 1)`. The rule holds whatever the right operand's
   spelling is — a literal, a local, a parameter, or a constant
   expression — and for `<<`, `>>`, `>>>` and their compound
   assignments.
2. The literal-count-at-or-above-width rejection retires. S008 keeps
   its own rule: a literal outside the range of its type.
3. Contextual representability is unchanged. A right operand literal
   must still fit its type, so `x << 300` with `x: u8` stays S008
   "integer literal 300 out of range for `u8`". §98 does not make
   every literal representable in a narrow right-operand context.
4. Mixed-width operands still require an explicit `as`, integer
   overflow behaviour is unchanged, and the signedness rules that
   pick an arithmetic or a logical right shift are unchanged.
5. No warning replaces the rejection. A typo diagnostic that depends
   on spelling is the defect this section removes, not a thing to
   rebuild in another colour. A future warning is a separate proposal
   with its own evidence.
6. The three tiers already agree and need no change. The interpreter
   masks with `b & (bits - 1)`; the ship tier emits
   `((right) & (width-1)u)` with an unsigned carrier for `<<` and
   `>>>` and a signed carrier for `>>`, so C's undefined shift is
   unreachable; the dev tier masks in hardware. `compiler/src/lir.rs`
   imposes no range rule on a shift count.

### 98.2 What this supersedes

Q18's last sentence retires: "Additionally, a **literal** shift amount
≥ the operand width is rejected at compile time (S008, the
out-of-range literal rule) — a constant over-shift is a typo, and C4
already rejects out-of-range literals rather than silently
reinterpreting them." The mask that the rest of Q18 states is what
remains, and it now has no exception.

`corpus/reject/r37-literal-overshift.ts` retires. Q18 carries no
Accept or Reject list today, so the retirement is new text there, in
the `retired:<name>` form this file's preamble defines. *(Corrected
2026-09-09: this paragraph first said the name stays in an existing
list.)*

`compiler/src/check/expr.rs`
`compound_shift_keeps_the_literal_width_diagnostic` is replaced by a
semantic test of the same shape.

### 98.3 Sites

- `compiler/src/check/expr.rs`: the `literal_shift_amount` branch that
  raises S008 with `Divergence::IntegerLiteralRange`. It is the only
  site; nothing else validates a shift count.
- `specs/blocks/collisions.md` Q18, and every diagnostic explanation
  that equates an over-shift with integer representability.
- `compiler/src/language_reference.rs` and `generated-docs/`.

### 98.4 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the five measured rows above,
recorded on this host with their exit codes.

1. One accept entry, `js-comparable: yes`, using `i32` and `u32`
   shapes whose results agree with node: counts at width minus one,
   the width, the width plus one, and twice the width; all three
   operators; a left operand chosen so a left shift does not depend
   on unrelated overflow behaviour; and the literal, local, parameter
   and constant-expression spellings of the same count compared
   against each other.
2. One accept entry, `js-comparable: no Q18`, for the narrow and
   64-bit widths: signed and unsigned 8, 16 and 64 bit types, with
   independently calculated expected results, and values that
   distinguish an arithmetic from a logical right shift. Its header
   states why node cannot run it: a JavaScript number shifted by 8 is
   256 where `u8` gives 1.
3. Compound assignment for each operator, with a side-effectful
   target evaluated exactly once.
4. A negative count keeps its behaviour, recorded as a regression
   control where the count type admits one.
5. `corpus/reject/r37-literal-overshift.ts` is deleted, its harness
   row removed, and Q18 gains `Reject: retired:r37-literal-overshift`.
6. Two tests the first handoff missed retire with the rule: the
   literal-overshift assertion in `compiler/src/lib.rs`, and the u64
   maximum-count row in `compiler/tests/corpus_reject.rs`. The helper
   the removed branch used goes with them. *(Added 2026-09-09; the
   round found all three.)*
7. Rejections that stay, each with a positive control: `x << 300`
   with `x: u8`; a mixed-width count without `as`; a non-integer
   operand.
8. Gates: `tools/gate.sh full` green in both profiles; clippy at the
   7/18/13 baseline; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden byte-identical. The new entries are
   synchronous, so the aggregate LIR snapshot does not move; if it
   does, report it rather than capturing it.
