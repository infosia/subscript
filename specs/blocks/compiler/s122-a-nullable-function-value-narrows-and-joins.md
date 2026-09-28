<!-- §122 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 122. A nullable function value narrows and joins

*(Added 2026-09-28.)* Origin: the open item of the §118 record
(`specs/tracking/s118-parameter-capture.md`, "Outside §118"). The owner
chose to fix it on 2026-09-28.

Problem: C7 accepts `Ref | null` where `Ref` is a function type, and a
field such as `callback: (() => i32) | null = null` is in the corpus
(`r240`). But a value of that type cannot be used. Measured through
`subscript run` at `9072742`, with `f: ((x: i32) => i32) | null`:

- `if (f !== null) { f(3); }` and `f !== null ? f(3) : 0` fail in the
  checker: `S100: type (i32) => i32 | null is not callable`. The null
  check does not narrow a function type.
- `const g: (x: i32) => i32 = f ?? dbl;` passes the checker and fails
  in lowering: `mismatched argument count for jump` on the dev JIT
  when `f` holds `null`, `Coerce: expected scalar, got Pair` when it
  holds a function. The §118 record measured the ship C tier: `used
  type 'SubFn' where arithmetic or pointer type is required`.
- `f !== null ? f : dbl` and `f ?? ((x: i32): i32 => x + 100)` fail in
  lowering the same way.

`tsc` accepts every probe above. The form is in the accepted surface
and has no working use: a hole whose fix is mechanical (core principle
13).

### 122.1 Rules

1. A null check narrows `F | null` to `F` wherever it narrows a
   reference class `C | null` to `C` (§46 and the narrowing rules it
   extends). The narrowing rule is one rule over every `Ref | null` of
   C7. It carries no list of the types it applies to.
2. `a ?? b` and a conditional expression type a function operand as
   they type a reference-class operand: the same accepted operand
   pairs, the same result type, the same diagnostics.
3. The null function value has one representation. Every engine (dev
   JIT, ship C, reference interpreter) builds it, tests it, and passes
   it through a join (a `??`, a conditional arm, a block argument) in
   that one form. A join of a function value never goes through a
   scalar coercion.
4. `f === null` and `f !== null` compare that representation on every
   engine.
5. `f!` and `f?.()` stay outside the decided surface (their current
   S100 diagnostics do not change).
6. A lambda that captures a value and is stored in `F | null` follows
   §118 as it does when stored in `F`.

### 122.2 Acceptance

1. Red first (core principle 10): a new accept entry
   `a271-nullable-function-value` fails at `9072742` on the checker or
   in lowering, and passes after the change on all three engines,
   byte-identical to its `.expected`. It covers: narrowing by `if`, by a
   conditional expression, and by an early return; `??` with a function
   name, a lambda, and a capturing lambda on the right; a conditional
   join; `=== null` and `!== null` on a null and a non-null value; a
   `F | null` field read, narrowed through a local, and called. It is
   `js-comparable` if `node` gives the same output, and `tsc` accepts
   it.
2. The cause of rule 1 is found and removed, not bypassed: the round
   names the code that restricted narrowing and shows that the
   narrowing rule is now type-generic over C7's `Ref | null`.
3. A test per engine for rule 3: the null function value built, joined,
   and tested.
4. No existing `.expected` golden moves. The LIR text snapshot moves
   only if a271 enters it; the round reports the lines.
