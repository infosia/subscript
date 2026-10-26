<!-- §195 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 195. A reference result can be `undefined`

*(Added 2026-10-26.)* Origin: the owner asked on 2026-10-26 for a
script API that returns `undefined` on a miss, as `Map.get` does. The
owner selected the spelling-flag form on 2026-10-26. The measurement
is in `specs/tracking/undefined-result.md`.

Problem, measured at the pin `ee65e587`:

1. A script class cannot declare the result of the standard search
   APIs. `get(i: i32): Box | undefined` with `return undefined;` gives
   S012. The lib already types `Map.get`, `find`, and `findLast` as
   `T | undefined`, and both tiers hold `null` there (C22).
2. `missing === undefined` on a `Map.get` miss gives S012. `node`
   prints `true`.
3. `missing === null` on the same miss prints `true`. `node` prints
   `false` (C22).

`tsc` 5.9.2 with this project's options keeps the two spellings apart:
`return null` in a `Box | undefined` function gives TS2322, a
`Box | undefined` value does not convert to `Box | null` (TS2322), and
`a !== null` does not narrow `Box | undefined` (TS18048). `tsc` accepts
`u === null`, `n === undefined`, and `u === n` for
`u: Box | undefined` and `n: Box | null`.

### 195.1 The type

1. **The absence spelling.** A nullable type carries one of two
   spellings, `null` or `undefined`. `T | null` has the `null`
   spelling. `T | undefined` has the `undefined` spelling. The set of
   `T` is the set of `T | null` (C7, §123.1 rule 1). The two spellings
   have one representation: the nullable pointer of `T | null`.
2. The two spellings are different types. Neither converts to the
   other, as `tsc` requires. A `T` converts to both.
3. `T | null | undefined` is rejected (S011). A type that needs both
   spellings has no representation.
4. **The result position.** `T | undefined` is legal as the declared
   result type of a module-level function declaration or of a class
   method. The method can be generic or be a method of a generic class.
   A getter and a `declare function` are not in this position.
   A type argument that makes `T` a type outside the set of rule 1
   gives S011, as `V | null` does.
5. Each other position of a `T | undefined` annotation is rejected
   (S012). This includes a local, a parameter, a field, a type
   argument, a type alias, a function type, an arrow result, an
   `async` result (`Promise<T | undefined>`), and a generator result.
6. **The whole type of a value.** The `undefined` spelling is legal as
   the whole type of an expression and of a local whose type comes from
   its initializer. Each type that holds the spelling below its top
   level is rejected (S012): an array literal element, an inferred
   type argument, and a closure result. A module variable and a field
   whose type comes from an initializer are storage, not a local, so
   the spelling is rejected there too.
7. A `let` local of the `undefined` spelling accepts an assignment of a
   value of the same type.

### 195.2 The token

1. The `undefined` token is legal in three positions:
   - `return undefined;` in a function whose result is `T | undefined`;
   - an operand of `===`, `!==`, `==`, or `!=` whose other operand
     has a nullable type, under §195.3 (`undefined === undefined` and
     `undefined === 1` stay rejected);
   - the presence comparison of §43 (unchanged).
2. Each other position gives S012. This includes `return undefined;`
   in a function whose result is `T | null` (`tsc` gives TS2322), an
   assignment, an argument, an initializer, and an operand of `??`.
3. A function whose result is `T | undefined` returns with
   `return undefined;` or `return <expr>;`. A `return;` and an end of
   the body that control reaches give S012. `tsc` accepts both forms;
   this rule keeps the absent result visible in the source.

### 195.3 Equality

This rule restates §144.1 rule 1, as §144.1 rule 2 requires.

1. The spelling of an operand is the spelling of its type. The `null`
   literal has the `null` spelling. The `undefined` token has the
   `undefined` spelling. A non-nullable operand has no spelling.
2. If the two operands have different spellings, `===` and `!==` are
   rejected (S012). `node` gives `false` for `===` there, also when
   both operands are absent. The message names `==` as the
   replacement for `===`, and `!=` for `!==`.
3. Each other pair keeps §144.1 rule 1: `==` has the type rules, the
   result, and the lowering of `===`.
4. A comparison with the `undefined` token tests absence, as a
   comparison with `null` does. It narrows as a null test narrows
   (C7, §159, §162).
5. Measured with `node` v24.22.1 (2026-10-26) for `u: Box | undefined` and
   `n: Box | null`, both absent: `u === undefined` true,
   `u == null` true, `u == undefined` true, `n == undefined` true,
   `u === null` false, `n === undefined` false, `u === n` false,
   `u == n` true.

### 195.4 Other operators

1. `a ?? b` accepts a left operand of either spelling (§82.3). If `b`
   is nullable, the result has the type of `b`, with the spelling of
   `b`. Else the result is `T`. This is the `tsc` result type.
2. An optional chain on a left operand of the `undefined` spelling has
   the two positions of §82.3.
3. Template interpolation, `!`, and a truth position stay rejected for
   both spellings (S100), as they are for `T | null` at the pin.

### 195.5 The standard library

1. `Map.get`, `find`, and `findLast` give the `undefined` spelling.
   This is the lib's result type. `get` returns `V | undefined`;
   `find` and `findLast` return `T | undefined`.
2. A rule of a builtin result comes from the lib or prelude
   declaration of that member. A builtin whose declared result is
   `X | undefined` and whose checked result is nullable gives the
   `undefined` spelling. The checker table carries the spelling for
   each such member, and a unit test compares each row against the
   declared result in the TypeScript lib or `prelude/lang.d.ts`.
3. `get`, `find`, and `findLast` are rejected (S014) where `V` or `T`
   is itself nullable (`Map<K, Box | null>`, `(Box | null)[]`). The
   lib result is `Box | null | undefined`, which rule 3 of §195.1
   rejects. A miss and a stored `null` have one representation, so no
   result is correct. `has` and `getOr` stay legal. At the pin, `get`
   on such a map returns `Box | null`, and a miss and a stored `null`
   give the same value. No corpus entry uses this form at the pin.
4. `collisions.md` C22 narrows to the generator `value`. A `Map.get`,
   `find`, or `findLast` miss tests with `=== undefined` and agrees
   with `node`. The divergence entry of the reference search miss
   changes its TypeScript side to `missing === null` (rule 2 of
   §195.3).

### 195.6 The boundary

1. A function that crosses the C boundary has no `T | undefined` in
   its signature (S012): an exported entry function, a host API module
   function, and a callback type that a C header declares. C has one
   null pointer, and the host reads it as `NULL` (invariant 4).
2. A function whose result is `T | undefined` is only a callee
   (S100). A read of it as a value is rejected, as a method read is
   rejected at the pin. A function type cannot name its result
   (§195.1 rule 5).

### 195.7 The form

1. The checker type carries the spelling on the nullable type. The
   derived equality of the type distinguishes the spellings, so
   §195.1 rule 2 is the conversion rule that already exists.
2. **HIR carries the `null` spelling only.** The HIR builder maps each
   `undefined` spelling to `null`. LIR, the three engines, the
   runtime, and the C emitter do not see the spelling.
3. The check of rule 2 compares two forms that are built separately:
   a unit test lowers a program with the `undefined` spelling, and the
   same program with `null` in each place, and compares the two LIR
   texts. They are identical. A positive control in the same test
   changes one operand and shows that the LIR texts differ.
4. The change has no run-time cost: rule 3 shows that the code is the
   code of `T | null`.

### 195.8 Diagnostics

| Form | Code | Message (substring that a reject entry pins) |
|---|---|---|
| `T \| undefined` outside the result position | S012 | `` `T \| undefined` is legal only as the result type of a function declaration or a class method `` |
| a nested `undefined` spelling (§195.1 rule 6) | S012 | `` the `undefined` spelling is legal only as the whole type of a value `` |
| the `undefined` token outside §195.2 rule 1 | S012 | `` `undefined` is legal only in `return undefined;` and in an absence comparison `` |
| `return;` or the end of the body | S012 | `` a function whose result is `T \| undefined` returns with `return undefined;` `` |
| `===` or `!==` across spellings | S012 | `` `===` compares a `null` and an `undefined` absence; use `==` `` |
| a scalar `T` | S011 | the existing S011 text, with `undefined` in place of `null` |
| the boundary | S012 | `` a function that crosses the C boundary has no `T \| undefined` `` |
| a function value | S100 | `` a function whose result is `T \| undefined` may only be called `` |
| `get`, `find`, `findLast` on a nullable element | S014 | `` the result needs both `null` and `undefined` `` |

`r13-undefined` keeps its program. Its expected error becomes the
first row: the local annotation is checked before the scalar.

### 195.9 Acceptance

1. Red first, at the pin `ee65e587`: `a373-undefined-result` is
   rejected with S012. Each reject entry `r422`–`r435` fails its
   expected error at the pin. The tracking note records each failure.
2. `a373` is `js-comparable: yes`. `node` output equals the golden.
   The dev JIT, the ship C, and the interpreter give the golden, byte
   for byte.
3. Each reject entry gives its row of §195.8. Each `tsc: accepts`
   entry has a divergence entry that cites C7.
4. The unit test of §195.7 rule 3 passes, with its positive control.
5. The unit test of §195.5 rule 2 compares each builtin row with its
   declared result.
6. Three accept entries compare a `Map.get` result with `=== null`
   (`a51`, `a66`, `a272`). Rule 2 of §195.3 rejects that comparison,
   so each source changes to `=== undefined`. Each output stays the
   same. No `.expected` golden moves. The LIR text snapshot does not move:
   `a373` has no generator and no `async` function. The generated
   reference documents move where they quote C22, S012, and
   `Map.get`.
7. `tools/gate.sh full` passes.

Corpus: accept `a373-undefined-result`; reject
`r422-undefined-bare-return`, `r423-undefined-fall-through`,
`r424-undefined-strict-null-compare`,
`r425-null-strict-undefined-compare`,
`r426-undefined-strict-cross-compare`,
`r427-undefined-scalar-result`, `r428-undefined-local-annotation`,
`r429-undefined-field-annotation`,
`r430-undefined-parameter-annotation`,
`r431-undefined-function-value`, `r432-undefined-array-element`,
`r433-undefined-exported-result`, `r434-undefined-arrow-result`,
`r435-map-get-nullable-value`.
