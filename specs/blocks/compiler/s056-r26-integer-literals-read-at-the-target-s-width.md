<!-- §56 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 56. R26 — integer literals read at the target's width

A downstream report (R26, 2026-08-11): a `u64` literal above
9007199254740991 (2^53 − 1) fails with S008, and the value is a
valid `u64`. WebGPU types buffer sizes, offsets, and copy sizes as
`u64`. A downstream test computes such a constant instead of
writing it.

Measured here at `d641d6d`: these three literals all fail with
S008.

```
const a: u64 = 9223372036854775807;
const a: u64 = 0xFFFFFFFFFFFFFFFF;
const b: i64 = -9223372036854775808;
```

Stock `tsc` accepts all three shapes (measured: exit 0). TS 80008
is a suggestion, not an error, so invariant 5 holds for the full
64-bit ranges.

The cap is the C3 decision: "no surface spelling above 2^53 − 1;
revisit with evidence" (`collisions.md` §3). R26 is that evidence.
The cause: `check_num_lit` (`compiler/src/check/expr.rs`) reads the
parser's `f64` view of the literal and range-checks through `f64`.
The spelling (`raw`) is available and exact.

### 56.1 Rule

In an integer context of type `T`, the checker reads the literal
from its spelling, not from the `f64` view. The checker accepts the
literal exactly when its mathematical value is in `T`'s range:

- `u64`: 0 to 18446744073709551615.
- `i64`: −9223372036854775808 to 9223372036854775807.
- Narrower types: unchanged ranges.

The reader handles every spelling the parser accepts: decimal,
`0x`/`0X`, `0b`/`0B`, `0o`/`0O`, and `_` separators. A leading
minus reaches the checker as the C4 fold. The reader applies the
sign before the range check, so `-9223372036854775808` is one
`i64` literal. If a literal has no spelling (a synthesized node),
its numeric value is exact and the present path stands.

Unchanged: a fractional or exponent spelling in an integer context
is an error; a float context keeps the `f64` reading; a
context-free integer literal defaults to `i32`; the S008 message
keeps its form.

### 56.2 The bit pattern in the HIR

`ExprKind::Int(i64)` stores the two's-complement bit pattern, and
the expression type names the interpretation. Each consumer reads
the bits at the expression's type:

- Cranelift `iconst` takes the bits. Correct today; no change.
- The C emitter's `int_literal` reinterprets by type
  (`v as u64` with the `ull` suffix). Correct today for `u64`. For
  `i64` it prints `{v}ll`, and that spelling is invalid C for
  `i64::MIN`. It must print `(-9223372036854775807ll - 1)`, the
  treatment the function gives `i32::MIN` today.
- The literal shift-amount check compares the bits as `i64`
  (`check/expr.rs`). A `u64` amount with a negative bit pattern
  passes it. The check reads the amount at the operand's type.

### 56.3 The ambient mirror channel

`int_literal_value` (`compiler/src/check/mod.rs`) feeds mirror flag
constants (`declare const X = <int literal>;`, §13.2) and also
reads through `f64`. It reads the spelling at `u64` width instead.
`bindgen` then drops its fail-loud guard for flag values above
2^53 − 1 (`bindgen/src/emit.rs`). That guard cited the C3 cap
(`p5-interop.md` MINOR m2), and this section removes the cap. The
bindgen unit test for the guard becomes an acceptance test.

### 56.4 What does not change

- The runtime and both tiers carry exact 64-bit integers today.
  The downstream measured it: `base * 2048` prints
  18446744073709549568. This change reaches the literal channel
  only.
- Every pre-existing golden stays byte-identical.
- `as` conversions, the mixed-arithmetic rules, and C4 contextual
  typing stand.

### 56.5 Exit criteria (pre-registered)

1. Record the red first: the new accept entry fails with S008 at
   `d641d6d`.
2. New accept entry `a132-int-literal-64bit`: the `u64` maximum in
   decimal, the same value in `0x` and in `_`-separator form,
   9007199254740993 (2^53 + 1), the `i64` maximum, and the `i64`
   minimum; the program prints each one. The entry passes the
   checker gate, the `tsc` gate, and the tier-differential gate
   with a committed golden.
3. New reject entries with S008, registered in `corpus_reject.rs`:
   `r124-u64-literal-overflow` (18446744073709551616) and
   `r125-i64-literal-underflow` (-9223372036854775809).
4. Unit tests: the checker stores the bit pattern for the `u64`
   maximum and the `i64` minimum; the shift check rejects a `u64`
   amount of `0xFFFFFFFFFFFFFFFF`; the emitted C for an `i64::MIN`
   literal compiles and runs; a mirror flag constant above 2^53
   reaches a program with the exact value; `bindgen` emits a flag
   above 2^53.
5. The workspace gate passes in both profiles; every pre-existing
   golden stays byte-identical; `cargo fmt --check` and the `tsc`
   gate exit 0.
