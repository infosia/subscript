<!-- §196 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 196. `pop` and `shift` give `undefined` on an empty array

*(Added 2026-10-27.)* Origin: on 2026-10-27 the owner asked for each
standard-library API whose type differs from TypeScript only because
the language had no `undefined`. The owner selected `pop` and `shift`
on 2026-10-27. The measurement is in
`specs/tracking/undefined-result.md`.

Problem, measured at the pin `b4f5c2ba`:

1. The lib types `pop()` and `shift()` as `T | undefined`. The checker
   types them as `T`, and an empty array traps (`stdlib.md` §9).
2. For a reference element, a program that `tsc` accepts cannot read
   the result. `const j = jobs.pop(); if (j !== undefined) { … }`
   gives S012. `jobs.pop() ?? fallback` gives S100.
3. `jobs.pop().id` is accepted. `tsc` gives TS2532.

§195 gives the representation: `T | undefined` is the nullable
pointer of `T | null`.

### 196.1 Rules

*(Amended 2026-10-31 by §219: `string` is in the set of rule 1.)*

1. `pop()` and `shift()` on `T[]` give `T | undefined` if `T` is in
   the set of `Map.get` (§123.1 rule 1, §195.5 rule 3). On an empty
   array they give `undefined`. They do not trap.
2. For each other `T`, `pop()` and `shift()` give `T`, and an empty
   array traps, as at the pin. This includes a scalar, a
   value class, an async handle, a nullable `T`, and a `T` that owns
   counted values (§171), such as `Promise<void>[]`. A nullable
   counted owner has no release: with rule 1 on `Promise<void>[][]`,
   the `t87` and `t88` shapes stopped trapping, and the removed
   element was not released (measured by the implementation round). The call is not
   rejected: a discarded call and a scalar read are common, and the
   trap is the existing answer.
3. The builtin spelling table of §195.5 rule 2 has a row for each of
   `pop` and `shift`. The unit test of §195.5 rule 2 compares each row
   against the lib declaration.
4. A discarded `pop()` or `shift()` keeps its release of the removed
   element (`t87`, `t88`).
5. The change has no run-time cost on a non-empty array. The empty
   test replaces the trap test.

### 196.2 Acceptance

1. Red first, at the pin `b4f5c2ba`: `a375-array-pop-shift-undefined` is
   rejected (S012 ×4, S100 ×2: `==` with `null`, and `??` on a non-nullable left operand). `r439-array-pop-unnarrowed` is
   accepted. The tracking note records each result.
2. `a375` passes `tsc`, and its output under `node` equals its golden.
3. `r439` gives S011 "may be null here".
4. The traps of `pop` and `shift` on an empty array of a scalar, a
   value class, and `string` stay. The divergence witnesses
   `q4-empty-array-pop` and `q27-empty-array-shift` keep their scalar
   programs. A unit test pins the value-class case. *(§219 moves `string`.)*
5. A unit test shows that a discarded `pop()` and `shift()` on a
   reference element give the `t87` and `t88` results.
6. The API reference rows of `pop` and `shift` state rules 1 and 2.
7. No golden moves.

### 196.3 Phase Review

*(Added 2026-10-27.)* A fresh review of `b4f5c2ba..be60a854` gave no
CRITICAL or MAJOR finding and six MINOR findings. The three execution
forms and `node` agree on each probe: a field and a module array, a
generic `Queue<T>`, function, `Map`, `Set`, nested-array, and nullable
elements, and discarded calls.

1. MINOR, fixed: `collisions.md` said that `shift` traps when empty,
   with no exception.
2. MINOR, fixed: `runtime::arrops::shift` returns `bool` and had no
   direct test.
3. MINOR, fixed: a test comment cited the former trap path.

Open:

1. A generic container with a scalar `T` that tests `x === undefined`
   on a `pop` result gets S012 with the text of §195.2. The text does
   not name rule 2 or the instantiation. `tsc` accepts the program.
2. When the runtime archive is stale, each concurrent test process
   still starts `cargo build` (§214.1 rule 3).
3. *(Closed by §214.)* The build of the runtime archive ran in the
   working directory of the caller, and its freshness did not read
   `Cargo.lock`.
4. *(Closed by §217.1 rule 2.)* A `Map` and a class own their values,
   so `Map<string, Promise<void>>[]` has a `pop` of type `T | undefined`.
   `Promise<void>[][]` has a `pop` of type `T`.

