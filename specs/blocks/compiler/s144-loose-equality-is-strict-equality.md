<!-- §144 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 144. Loose equality is strict equality

*(Added 2026-10-02.)* Origin: §143 found that the checker rejects `==`
and `!=` and that no contract section stated the rule; `collisions.md`
C20 recorded it. Owner decision of 2026-10-02: accept both operators.

Problem: the checker rejects `==` and `!=` on every operand type (S100,
"loose equality is not in the language; use `===` / `!==`"). `tsc`
accepts them where the operand types overlap. The reason that the
divergence table gives (`compiler/src/divergence.rs`) is that loose
equality permits coercion. Core principle 14 requires a reason that
holds today; the measurements below show that no coercion is reachable.

Measured with `node` (2026-10-02): `==` and `===` differ only for
`null`/`undefined`, `1`/`"1"`, `0`/`false`, `1n`/`1`, and an object
against a string through `toString`. They agree for same-type numbers
(`NaN`, `0`/`-0` included), strings, booleans, `null`, and object
identity.

Measured with `tsc` 5.9.2 and this project's options: `i32 == string`,
`i32 == boolean`, `Box == string`, and `object == string` each give
TS2367. `i32 == f64`, `(Box | null) == null`, and `object == Box` give
no error.

So no program that passes `tsc` and this checker reaches a pair where
`==` and `===` differ: `tsc` rejects each pair of different primitive
kinds and each object-to-string pair, the language has no `undefined`
(S012), and no bigint (`i64` is `number` in `prelude/lang.d.ts`).

### 144.1 Rules

1. `a == b` has the type rules, the result, and the lowering of
   `a === b`. `a != b` has those of `a !== b`. Each diagnostic that
   `===` or `!==` gives for an operand pair, `==` or `!=` gives for the
   same pair, with the operator spelled as written.
2. The premise of rule 1 is that the language has no `undefined` value
   and no bigint type. A section that adds either one must restate rule
   1 first.
3. `collisions.md` C20 is deleted, with its `Divergence` variant. The
   reject entry `r294-loose-equality` retires
   (`retired:r294-loose-equality`).

### 144.2 Acceptance

1. Red first: an accept entry `a307-loose-equality` that uses `==` and
   `!=` on `i32`, `f64`, `string`, `boolean`, an enum, a reference class
   (identity), a nullable class against `null`, and a generic body with
   two values of one type parameter. Its header says `js-comparable:
   yes`; `node` output equals the golden. It is rejected at the
   contract pin.
2. Every pair that `===` rejects, `==` rejects with the same code: a
   unit test over the operand pairs of the existing `===` tests.
3. The §143 matrix cells for `==` and `!=` change from a C20 waiver to
   an accepted verdict; no cell names C20.
4. Both tiers give byte-identical output for `a307`. No other
   `.expected` golden moves.
