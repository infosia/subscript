<!-- §301 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 301. A type argument is inferred from the narrowed argument

*(Added 2026-11-15.)* Origin: §252.4 item 1. On 2026-11-15 the owner
selected this item.

**Withdrawn 2026-11-15, on measurement.** No rule of this section
lands. On the same day the owner decided to withdraw it: rule 2 needs a
`null` type for a read and for a type argument, which the language does
not have (C7). §301.3 records the measurement.

Problem, measured at `877407be` with `tsc` 5.9.2 and the debug CLI.
`type CB = (x: i32) => void;` and `two<T>(a: T, b: T): void`.

| Form | Checker | `tsc` |
|---|---|---|
| `const g: CB \| null = null; two(g, (x) => print(\`${x}\`));` | accepts; the lambda takes the parameter types of `CB` | TS2345, TS7006 |

`tsc` narrows a read of `g` to the type of its initializer: `g` is
`null` at the call, so `T` is `null`, the lambda has no function type
in its place, and its parameter has no type. The checker infers `T`
from the declared type `CB | null` of `g`, so the lambda takes the
context of `CB` (§251). The program is accepted, and `tsc` rejects it
(invariant 5).

A non-null narrowing already reaches the inference: with
`const a: A | null = new A();`, `const r: A = id(a);` is accepted by
the checker and by `tsc`, and with a parameter `a: A | null` and no
narrowing, `tsc` gives TS2322 to the same initializer. The gap is the
fact "is `null`" of an initializer `null`.

### 301.1 Rules

1. The inference of a type argument from an argument reads the type of
   the argument at the call after narrowing (§124, §159, §162): the
   type that a read of the argument has at that place. `tsc` infers
   from the narrowed type.
2. The flow analysis carries a fact "is `null`" (`FactKind` in
   `compiler/src/check/narrowing_fact.rs`). A `const` binding with a
   declared nullable type and the initializer `null` gets the fact at
   its declaration, and keeps it in its scope: a `const` cannot change.
   A read under the fact has the type `null`, as `tsc` gives it, at
   each place that reads the narrowed type, the inference of rule 1
   included. The round reports each other place where the result
   changes, with the `tsc` result of the form.
2a. The true branch of `x === null`, and the false branch of
   `x !== null`, do not get the fact in this section. `tsc` narrows
   them to `null` too; §301.4 records the difference.
3. The place check of the argument against the parameter does not
   change: it reads the same narrowed type.
4. No program that `tsc` accepts changes from accepted to rejected. If a
   corpus entry, a golden, the generic `tsc` matrix, or the
   rejection-site total check moves, the round reports each move with
   the `tsc` result of the form.

### 301.2 Acceptance

1. Red first, at the contract commit: the reject entry
   `r512-a-type-argument-is-inferred-from-the-narrowed-argument` holds
   the form of the table. At the contract commit, `subscript check`
   accepts it. The tracking note records the output.
2. After the change, `r512` is rejected with the diagnostic of a lambda
   parameter with no context (§252 rule 2), at the lambda. Its header
   states the `tsc` result.
3. A unit test pins rule 1 with a non-null narrowing: with
   `const a: A | null = new A();`, `id(a)` has the result type `A`, and
   `tsc` gives the same. A control in the same test: a parameter
   `a: A | null` with no narrowing gives `id(a)` the result type
   `A | null`.
4. No golden moves. The full gate passes.

### 301.3 Measurement

The coding agent measured two rounds. No code landed.

1. Rules 1 and 3 already hold: a non-null narrowing reaches the
   inference, and the place check reads the same type.
2. The flow analysis had no fact "is `null`" (`FactKind`). The first
   round stopped there (core principle 8), and the contract added the
   fact to rule 2.
3. With the fact, `r512` gave S100 "parameters require a type
   annotation" at the lambda, as `tsc` rejects it. But
   `const g: CB | null = null; const r = id(g);`, which `tsc` accepts
   with `T` as `null`, became S100 "cannot infer type parameter T of
   id: no candidate": the inference has no candidate type `null`.
   `if (g !== null) { g(1); }`, which `tsc` rejects with TS2349 (`never`
   is not callable), stopped the checker at an internal assertion.
   `r426` changed its S012 text.
4. A read with the type `null` needs a `null` type that a binding and a
   type argument can have. C7 has no such type. So rule 2 cannot land
   without a change of C7, and the form of the problem is a `const`
   that always holds `null`.

### 301.4 Open

1. §252.4 item 1 stays: `const g: CB | null = null; two(g, (x) => …);`
   is accepted, and `tsc` rejects it (TS2345, TS7006). It needs a
   `null` type (C7).
