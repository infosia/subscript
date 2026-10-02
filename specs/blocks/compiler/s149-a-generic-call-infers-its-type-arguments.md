<!-- §149 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 149. A generic call infers its type arguments

*(Added 2026-10-02.)* Origin: a downstream proposal (`HANDOFF.md`,
2026-10-02, item 4). Owner decisions of the same day: take it fourth;
a literal argument follows the non-literal arguments, and with none, the
C4 default.

Problem: a call of a generic function with no type arguments is
rejected ("generic function `f` requires explicit type arguments"),
where `tsc` infers them. Measured at `ea1b34c0` through `subscript
check`; `tsc` 5.9.2 accepts every row:

| Call | `subscript check` |
|---|---|
| `id(n)`, `n: i64` | S100 requires explicit type arguments |
| `id(7)` | S100 |
| `pair(n, 1)`, `n: i64` | S100 |
| `pair(n, f)`, `n: i32`, `f: f64` | S100 |
| `first(xs)`, `xs: u8[]` | S100 |
| `orNull(b, new Box())`, `b: Box \| null`, `orNull<T>(x: T \| null, d: T)` | S100 |

`id<i64>(7)` with an explicit type argument is accepted.

### 149.1 Rules

1. A call of a directly named generic function (a module function, an
   imported or namespace-qualified one included, §148) with no type
   arguments infers them from its arguments, before the instantiation
   step. The result is the call with those type arguments written
   explicitly; every later rule (constraints §143 rule 2b,
   monomorphization, §140 instance chains) applies to it unchanged.
2. Candidates come from each argument whose parameter type mentions a
   type parameter, matched structurally through `T`, `T[]`,
   `FixedArray<T, N>`, `T | null`, `Map<K, V>`, `Set<T>`, a generic class
   instance `G<T>`, and a function type `(x: T) => U` (from a function
   argument's declared signature). An argument of type `null` gives no
   candidate.
3. A literal argument (a numeric or string literal, or an array
   literal of them) gives no candidate while a non-literal argument
   gives one for the same type parameter; it is then checked with the
   inferred type as its context (C4), as §146 rule 1b2 does for a
   conditional. If only literal arguments mention a type parameter, the
   C4 defaults decide it: an integer literal `i32`, a fractional literal
   `f64`, a string literal `string`. Two literal-only defaults that
   differ (`pair(1, 2.5)`) are rejected as conflicting candidates, as C4
   rejects `[1, 2.5]`. *(Added 2026-10-02 after the Phase Review.)*
4. Two non-literal candidates for one type parameter must be the same
   type, or one must be `C` and the other `C | null` (the result is
   `C | null`). Otherwise the call is rejected with S100 that names both
   candidates and the type parameter, and asks for explicit type
   arguments. A `null` argument beside a candidate `C` joins to
   `C | null` when `C | null` is legal (`pair(null, new Box())` infers
   `Box | null`), as the `C` and `C | null` pair does. *(Added 2026-10-02
   after the Phase Review: the `null` argument was rejected as a plain
   type mismatch.)*
5. A type parameter with no candidate is rejected with S100 that names
   it and asks for explicit type arguments.
6. Explicit type arguments override inference. Each argument is
   evaluated once, in order; inference is a check-time step with no
   runtime effect.
7. Out of scope, still rejected as before: inference for a generic
   method, a generic constructor (`new G(x)`), a callback's parameter
   types from context, and the return type from context.

### 149.2 Acceptance

1. Red first: an accept entry with each Problem-table call, a call
   through a namespace import, two type parameters, a nested shape
   (`Map<string, T[]>`), and each call beside the same call with
   explicit type arguments printing the same result; a side-effect
   witness for rule 6. Rejected at the contract pin; `node` output equals
   the golden.
2. Reject entries: two conflicting non-literal candidates (`pair(n, f)`
   with `i32` and `f64`), a type parameter with no candidate (only
   `null`, or a return-only `T`), and a generic method call with no type
   arguments (rule 7); each header with the measured `tsc` result.
3. The §143 matrix: list each cell whose verdict changes; only cells
   that rule 1 accepts may change.
4. No existing `.expected` golden moves.

### 149.3 Open

These items are MINOR under CLAUDE.md invariant 6.

1. An argument made only of literals that is not itself a literal (a
   conditional of literals, literal arithmetic) gives a non-literal
   candidate with no context: `pair(n, c ? 1 : 2)` with `n: i64` is
   rejected as conflicting `i64` and `i32`; `tsc` accepts it.
2. A generic function used as a value (`apply(id, 3)`) asks for explicit
   type arguments, but the explicit spelling `apply(id<i32>, 3)` is
   outside the surface, so the message names a fix that does not exist.
