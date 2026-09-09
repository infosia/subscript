# §98 — one shift-count rule, whatever the spelling

Contract: `specs/blocks/compiler.md` §98. Origin: the owner's
2026-09-09 request to reconsider three decided restrictions on their
merits, under core principle 14.

Pin for every measurement: `e2a2b6da53eb73314321ef5c601fc9c592402f73`.
Host aarch64 macOS. rustc 1.95.0. node v24.18.0.
Command `target/debug/subscript run <file>`.

## Red at the pin

With `const x: i32 = 1`:

| Expression | exit | subscript | node |
|---|---:|---|---|
| `x << 32` | 1 | S008 "literal shift amount 32 is out of range for `i32` width 32" | `1` |
| `x << (16 + 16)` | 0 | `1` | `1` |
| `x << K`, `const K: i32 = 32` | 0 | `1` | `1` |
| `x << -1` | 0 | `-2147483648` | `-2147483648` |
| `x <<= 32` | 1 | the same S008 | `1` |
| `x << 300` with `x: u8` | 1 | S008 "integer literal 300 out of range for `u8`" | n/a |

The last row is the control. It is the genuine C4 rule and it stays.

## Why the restriction goes

**It catches one spelling out of four.** A constant expression, a
named constant, and a negative literal all pass. Factoring `32` into a
name changes acceptance, which no representation rule can justify.

**C4 does not cover it.** `32` is representable in `i32`, yet the
diagnostic renders C4's rule and reason:

```
= rule: Numeric literals must fit their context and be integral in an integer context.
= why: A literal takes the sized type of its context, so a value outside
  that range has no representation. (collisions.md C4)
```

The literal fits its context exactly. The block also shows
`Divergence::IntegerLiteralRange`'s example, `const big: i32 =
3000000000`, which is a different problem.

## The tiers already agree

Verified by reading each, and by measuring the variable-count forms
that the checker already accepts:

- interpreter, `codegen/src/interpreter.rs`: `(b & u64::from(bits - 1))`
- ship C, `codegen/src/cemit.rs` `shift_expression`:
  `((right) & (width-1)u)`, unsigned carrier for `<<` and `>>>`,
  signed carrier for `>>`
- dev JIT: masks in hardware
- `compiler/src/lir.rs` imposes no range rule on a shift count

Measured on the dev tier with variable counts:

| Expression | subscript | node |
|---|---|---|
| `i32 1 << 31` | `-2147483648` | same |
| `i32 1 << 32` | `1` | same |
| `i32 1 << 33` | `2` | same |
| `i32 1 << 64` | `1` | same |
| `u8 1 << 8` | `1` | JS number gives `256` (Q18) |
| `i8 -8 >> 1` | `-4` | same |
| `u32 4294967288 >>> 1` | `2147483644` | same |
| `i64 -8 >> 1` | `-4` | 64-bit, no JS equal (Q18) |
| `i64 -8 << 64` | `-8` | as above |

The removal is a checker-only change: one branch in
`compiler/src/check/expr.rs`.

## Landed, 2026-09-09

Gate verdict:

```
gate full ddd696ebb41639df457b1ad6c49166ed25f4f018 dirty:11 debug 1359/0/2 release 1357/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

No golden moved, and the aggregate LIR snapshot did not grow: both new
entries are synchronous.

### The round's findings, all correct

1. **Q18 carries no Accept or Reject list.** §98.2 said the retired
   name stays in an existing list. There is none, so the retirement is
   new text. The round raised it before implementing, as asked.
2. **Two obsolete tests the first handoff missed**: the
   literal-overshift assertion in `compiler/src/lib.rs` and the u64
   maximum-count row in `compiler/tests/corpus_reject.rs`. The first
   full gate failed on the second one, exit 1, `1358/1/2`. Both retire
   with the rule, and the helper the removed branch used goes with
   them.

### Verified here after the change

| Expression | before | after | node |
|---|---|---|---|
| `x << 32`, `x: i32` | S008 | `1` | `1` |
| `x <<= 32` | S008 | `1` | `1` |
| `x << (16 + 16)` | `1` | `1` | `1` |
| `x << K`, `const K: i32 = 32` | `1` | `1` | `1` |
| `x << -1` | `-2147483648` | `-2147483648` | same |
| `x << 300`, `x: u8` | S008 | S008 | control, unchanged |

The control keeps the genuine C4 diagnostic, "integer literal 300 out
of range for `u8`". a202 and a203 match their goldens.
