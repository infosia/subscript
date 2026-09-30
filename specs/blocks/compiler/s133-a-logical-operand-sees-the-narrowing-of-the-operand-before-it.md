<!-- §133 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 133. A logical operand sees the narrowing of the operand before it

*(Added 2026-09-30.)* Origin: found during §124 and ordered by the
owner on 2026-09-28 ("`&&` narrowing on parameters").

Problem: the checker checks both operands of `&&` and `||` with no
narrowing from the left operand
(`compiler/src/check/expr/operator.rs`, `B::LogicalAnd | B::LogicalOr`).
A statement condition combines the facts of both operands for its
branches (`narrow_paths`), so `if (a !== null && b !== null)` narrows
both in the body, but the right operand itself does not see the left
operand's facts. Measured at `567c32e` through `subscript check`, for a
parameter and for a local:

- `hn !== null && hn.c !== null`: S011 at `hn.c` ("`H | null` may be
  null here").
- `l !== null && l.c !== null` in an `if` condition, then `l.c.v` in
  the body: S011 at `l.c` in the condition.
- `l === null || l.c === null` and `l === null || l.c !== null`: S011
  at `l.c`.
- A conditional expression narrows correctly:
  `l !== null ? l.c !== null ? l.c.v : 0 : 0` checks clean.

`tsc` 5.9.2 accepts each program (exit 0, measured with the prelude).

### 133.1 Rules

1. The right operand of `a && b` is checked with the facts that hold
   when `a` is true. The right operand of `a || b` is checked with the
   facts that hold when `a` is false. The facts are the ones a
   statement condition derives (`narrow_paths`), including nested
   operators: in `a && b && c`, `c` sees the facts of `a` and `b`.
2. `narrow_paths` derives the false facts of `a || b` (the facts that
   hold when both are false), as it derives the true facts of `a && b`.
3. The narrowing of a shared location follows §124: a kill inside the
   right operand (a call, a store, an `await`) ends it at that point,
   and every narrowed shared read carries its `NarrowNonNull` site.
4. The facts end with the operand: the expression after the logical
   operator sees only the facts of the whole condition, as today.

### 133.2 Acceptance

1. Red first: an accept entry with each measured shape (a parameter, a
   local, `&&`, `||`, nested `&&` chains, a shared field path and a
   local path), rejected at `567c32e`; `js-comparable`, goldens on all
   three engines and `node`. The round records the Red output.
2. Unit tests: a §124 kill in the right operand ends the narrowing
   there (`h.c !== null && (clear(h), h.c.v > 0)` or its surface form
   gives S011 after the call); the facts do not reach the expression
   after the operator (`const x = a !== null && true; a.v` gives
   S011); controls.
3. No existing `.expected` golden moves.
