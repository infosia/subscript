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
   diagnostic. `void` is not a contextual type here: it gives no
   candidate, so `run(() => id(1))` with a `() => void` parameter
   keeps the text of the pin. *(Amended 2026-10-31 after the Phase
   Review, m3.)*
1a. A context that is only a literal hint is not a contextual type
   here. A call at an arithmetic operand, a unary `-` or `~` operand,
   an index, a compound assignment value, or a `switch` case test takes
   no contextual candidate, also through parentheses and a conditional
   branch. `const x: i32 = -zero();` and `xs[zero()]` stay rejected, as
   `tsc` TS2571 and TS2538 do. The right operand of `??` (except §212.4
   item 6a), a conditional branch in a position of rule 1, and the
   operand of `satisfies` keep the context. *(Added 2026-10-30: the coding agent measured these
   forms.)* The type of an earlier element of an array literal is a
   hint too: a later element of an array literal with no contextual
   type takes no contextual candidate. `const xs = [new P(), zero()];`
   stays rejected. `tsc` accepts that declaration, gives the element
   the type `unknown`, and gives TS2571 at a read of a member.
   *(Amended 2026-10-31 after the Phase Review, m2.)*
2. The order is the order of §197 rule 3. A non-literal argument
   candidate comes first, the contextual candidate second, and the C4
   default of a literal last. A non-literal argument is checked with
   the context of its parameter type, with each contextual candidate
   replaced, so `const b: Buffer<Buffer<P>> = wrap(empty());` checks
   `empty()` with the context `Buffer<P>`.
3. A contextual candidate does not decide a type parameter `T` if a
   `null` argument meets a parameter of type `T`, or of a union with
   `T` as a member. `tsc` takes `null` as the candidate of `T` in these
   forms: `const x: P[] = f(null);`, `f<T>(x: T | null): T[]`, gives
   TS2322. The call keeps the §149.1 rule 5 diagnostic. A `null`
   argument for another shape gives `T` no candidate, so the context
   decides `T`: `const b: Buf<P> = nb(null);`, `nb<T>(x: Buf<T> | null):
   Buf<T>`, is accepted, as `tsc` accepts it. An argument that the
   checker reads before the `null` keeps its context: `g(empty(), null)`
   is accepted, as `tsc` accepts it. A constructor keeps §197.
   *(Amended 2026-10-31 after the Phase Review, M1.)*
4. An argument candidate that disagrees with the contextual type stays
   as the argument gives it. The contextual type then reports the
   mismatch, as at the pin. A literal argument takes the contextual
   candidate (rule 2), so `const x: i32 = id("a");` reports the mismatch
   at the argument. The program stays rejected.
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

### 212.3 Phase Review

*(Added 2026-10-31.)* A fresh review of `b1ec9a9b..ea1514c1` gave one
MAJOR and eight MINOR findings.

1. MAJOR, realistic, fixed: rule 3 stopped the context for a `null`
   argument at any depth. `nb<T>(x: Buf<T> | null): Buf<T>` with
   `const b: Buf<P> = nb(null);` gave S100; `tsc` accepts it. Rule 3
   now names the two forms where `tsc` takes `null` as the candidate.
2. MINOR, realistic, open: the right operand of `??` in a generic body
   has no context (§212.4 item 6a). A fix with a context changed
   `a ?? id(b ? 1 : 2)`, against rule 6, so the fix did not land.
3. MINOR, realistic, fixed: an earlier array element acted as a
   context, so `const xs = [new P(), zero()];` was accepted. Rule 1a.
4. MINOR, realistic, fixed: a `void` context gave a candidate and a
   new text. Rule 1.
5. MINOR, fixed: three marks of rule 1a had no test. A deletion of each
   mark now fails a unit test.
6. MINOR, fixed: `docs/tutorial-typescript.md`, the
   `GENERICINFERENCEMISSING` text, and §149.1 rule 7 did not state the
   new rule.
7. MINOR, fixed: rule 3 and rule 4 text.
8. MINOR, open: §212.4 items 2 to 7 record forms that stay rejected.
9. MINOR, open: §212.4 item 8 records a text that the pin also gives.

### 212.4 Open

1. `k?.make()` with a context takes no contextual candidate: the
   optional call has no context. `tsc` accepts it.
2. A `Promise<T>` result takes no contextual candidate: `Promise` is
   not a §149.1 rule 2 form. `await af()`, `const p: Promise<Buf<P>> =
   af();`, and `return af();` stay rejected. `tsc` accepts them.
3. `empty() as Buf<P>` takes no contextual candidate: `as` gives no
   context. `tsc` accepts it.
4. `Array<T>` as a declared result is not a §149.1 rule 2 form, so
   `mk<T>(): Array<T>` with `const x: P[] = mk();` stays rejected. `tsc`
   accepts it.
5. A lambda result gives no context through a built-in method or a
   type parameter with no candidate:
   `const r: Buf<P>[] = xs.map((x: i32) => empty());` and
   `const b: Buf<P> = mapb(new P(), (v: P) => empty());` stay rejected.
   `tsc` accepts them. An annotated lambda result is accepted.
6. `new Set([1, zero()])` stays rejected: its array has no contextual
   type (rule 1a). `tsc` accepts it.
6a. In a generic body, the right operand of `??` has no context if the
   left operand involves a type parameter, so
   `function f<U>(a: Buf<U> | null): Buf<U> { return a ?? empty(); }`
   and `a ?? []` stay rejected. `tsc` accepts them. A context there
   changes the type of `a ?? 1` and `a ?? id(1)`, which rule 6 forbids.
7. `!zero()`, `c || zero()`, `c && zero()`, and `empty() ?? null` take
   no contextual candidate. `tsc` accepts them.
8. `id<Mode>("a")` with `type Mode = "a" | "b"` gives "the argument
   expects `string`, got `Mode`", at the pin too. `tsc` accepts it.
   `const m: Mode = zero();` now reaches the same text.
