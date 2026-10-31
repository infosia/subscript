<!-- §215 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 215. Inference reads `Promise<T>` and `Array<T>`

*(Added 2026-10-31.)* Origin: §212.4 items 2 and 4. On 2026-10-31 the
owner asked for the open work in priority order. The measurement is in
`specs/tracking/inference-promise-array.md`.

Problem, measured at `fbf3509a`: §149.1 rule 2 has no form for
`Promise<T>` and for `Array<T>`, and `await` gives its operand no
context. Each of these gives S100 "cannot infer type parameter", and
`tsc` 5.9.2 accepts each one:

| Form | Declaration |
|---|---|
| `const b: Buf<P> = await af();` | `af<T>(): Promise<Buf<T>>` |
| `const p: Promise<Buf<P>> = af();` | as above |
| `const r = await fr(q);`, `q: Promise<P>` | `fr<T>(p: Promise<T>): Promise<T>` |
| `const x: P[] = mk();` | `mk<T>(): Array<T>` |

An async factory is the async form of the §212 request, and
`Array<T>` is a spelling of `T[]`.

### 215.1 Rules

1. `Array<T>` in a declared parameter or result type is a §149.1 rule 2
   form. It matches an array as `T[]` does.
2. `Promise<T>` in a declared parameter or result type is a §149.1
   rule 2 form. It matches a handle of value type `V` with `T` against
   `V`. This holds for an argument candidate and for a contextual
   candidate.
3. `await e` with the contextual type `C` gives a call `e` the
   contextual type `Promise<C>`. An `await` with no contextual type
   gives none, so `const b = await af();` keeps the §149.1 rule 5
   diagnostic, as §212.1 rule 5 states.
4. §212.1 rule 1a applies through `await`. A call under an `await` at
   an arithmetic operand, a unary operand, an index, a compound
   assignment value, a `switch` case test, or a later array element with
   no contextual type takes no contextual candidate. `tsc` gives TS2322
   for `const b: i32 = (await az())[0];`.
5. `return af();` in an async function stays rejected, also with written
   type arguments: §167.1 rule 3 has no promise adoption.
6. No accepted program changes its meaning. Each form gave a diagnostic
   at the pin.
7. Inference is a check-time step. The program has no run-time cost.

### 215.2 Acceptance

1. Red first, at the pin `fbf3509a`: the accept entry
   `a398-inference-promise-array` is rejected. The tracking note records
   the result.
2. `a398` passes `tsc`, and its output under `node` equals its golden.
   It has each form of the problem table, a `using` annotation, a generic
   method, and a nested generic call under `await`.
3. A unit test pins rule 3 with no context, rule 4 with a control, and
   rule 5.
4. The rejection-site total check passes.
5. No golden moves. The LIR text snapshot grows by the `a398` section
   only, because `a398` has async functions.
