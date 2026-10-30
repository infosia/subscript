<!-- §212 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 212. A generic call infers from its contextual result type

*(Added 2026-10-30.)* Origin: a downstream change request
(`HANDOFF.md`, 2026-10-30). The measurement is in
`specs/tracking/contextual-result-inference.md`.

Problem, measured at the pin `b1ec9a9b`: a type parameter that only
the result type of a generic function or method names has no
candidate. `const a: Buffer<Particle> = createBuffer(device, 1);` gives
S100 "cannot infer type parameter `T` of `createBuffer`: no candidate;
use explicit type arguments". `tsc` 5.9.2 accepts the call, with the
context in a local annotation, a parameter, a `return`, a field
initializer, an assignment, an array element, and `C | null`. A
partial list `createBuffer<Particle>(device, 1)` is not a remedy:
`tsc` rejects it with TS2558. A downstream project spells the device
class at 81 sites for this reason. §197 rule 3 gives this candidate
to a constructor and leaves out a function and a method.

### 212.1 Rules

1. A call of a generic function or a generic method with no type
   arguments and with a contextual type (§197 rule 4, and the target
   type of an assignment) takes contextual candidates. The declared
   result type is matched against the contextual type by the §149.1
   rule 2 forms. The match also goes through `C | null` in the
   contextual type. Each match gives that type parameter a contextual
   candidate. A shape that does not match gives no candidate and no
   diagnostic.
2. The order is the order of §197 rule 3. A non-literal argument
   candidate comes first, the contextual candidate second, and the C4
   default of a literal last. A non-literal argument is checked with
   the context of its parameter type, with each contextual candidate
   replaced, so `const b: Buffer<Buffer<P>> = wrap(empty());` checks
   `empty()` with the context `Buffer<P>`.
3. A contextual candidate does not decide a type parameter if a `null`
   argument mentions that parameter. `tsc` takes `null` as the
   candidate there: `const x: P[] = f(null);`, `f<T>(x: T | null): T[]`,
   gives TS2322. The call keeps the §149.1 rule 5 diagnostic.
4. An argument candidate that disagrees with the contextual type stays
   as the argument gives it. The contextual type then reports the
   mismatch, as at the pin.
5. A call with no contextual type keeps the §149.1 rule 5 diagnostic.
   `tsc` infers `unknown` there, and the language has no such type. A
   partial type argument list stays rejected, as `tsc` TS2558 does.
6. No accepted program changes its meaning. A contextual candidate
   decides only a type parameter that gave a diagnostic at the pin, or
   a literal default whose call failed the context at the pin.
7. Inference is a check-time step. The program has no run-time cost.
8. §149.1 rule 7 and §197 rule 3 cite this section: they no longer
   keep the result type out.

### 212.2 Acceptance

1. Red first, at the pin `b1ec9a9b`: the accept entry
   `a397-contextual-result-inference` is rejected. The tracking note
   records the result. This section adds no reject entry: each form
   that stays rejected gives its text at the pin, so a unit test pins
   it.
2. `a397` passes `tsc`, and its output under `node` equals its golden.
   It has each context of rule 1, a constrained type parameter beside a
   result-only one, a method and a static method, a literal argument
   that takes the context, a nested generic call, and a `using`
   annotation.
3. A unit test pins rule 2, the order, with a control: an argument
   candidate against a different context gives the mismatch of rule 4.
4. A unit test pins rule 3 with a control, rule 5 for a call with no
   context, and a context of another shape.
5. The rejection-site total check passes.
6. No golden moves. The LIR text snapshot does not move: `a397` has no
   generator and no async function.
