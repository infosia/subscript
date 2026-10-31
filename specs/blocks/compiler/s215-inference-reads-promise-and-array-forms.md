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
3a. A synchronous callee under `await` takes no such context. So
   `const x: P = await zero();` keeps the no-candidate text. A contextual
   type through `C | null` works as in §212.1 rule 1:
   `const b: Buf<P> | null = await af();` is accepted. *(Added
   2026-10-31: the coding agent measured these forms.)*
4. §212.1 rule 1a applies through `await`. A call under an `await` at
   an arithmetic operand, a unary operand, an index, a compound
   assignment value, a `switch` case test, or a later array element with
   no contextual type takes no contextual candidate. `tsc` rejects
   `const b: i32 = (await az())[0];`.
5. `return af();` in an async function stays rejected, also with written
   type arguments: §167.1 rule 3 has no promise adoption.
6. No accepted program changes its meaning. A literal array at an
   `Array<T>` parameter now gives a candidate, as at a `T[]` parameter,
   so `arrv([1, 2], 3.5)`, `arrv<T>(a: Array<T>, v: T)`, gives the §149
   conflict that the `T[]` form gives. The pin accepted that call.
   *(Amended 2026-10-31 after the Phase Review, m1.)*
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

### 215.3 Phase Review

*(Added 2026-10-31.)* A fresh review of `fbf3509a..23fcf221` gave no
CRITICAL or MAJOR finding and seven MINOR findings.

1. MINOR, realistic, recorded: rule 6 was false for a literal array at
   an `Array<T>` parameter. Rule 6 and §215.4 item 4.
2. MINOR, fixed: this section said that `tsc` accepts
   `Promise.all([af()])`. It gives TS2322.
3. MINOR, fixed: rule 4 named a `tsc` code that depends on the
   declaration.
4. MINOR, contrived, fixed: the expanding-edge check had no handle
   form, so a polymorphic recursion through `Promise<T>` stopped the
   compiler with a stack overflow in place of the §140 text.
5. MINOR, fixed: the tracking note had no Red run for the final `a398`.
6. MINOR, fixed: no test pinned the restore of the call context after
   an `await`.
7. MINOR, fixed: two paths passed a context that has no effect.

### 215.4 Open

`tsc` accepts each of these, and each stays rejected:

1. `await (c ? af() : af())`: a non-call operand of `await` has no
   context.
2. `await k?.make()` (§212.4 item 1).
3. `await saf()`, with a synchronous `saf<T>(): Promise<T>` (rule 3a).
4. `arrv([1, 2], 3.5)` (rule 6): `tsc` gives `T` the type `number`. The
   language has no such join of `i32` and `f64`.
