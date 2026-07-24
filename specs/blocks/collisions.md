# Semantic collisions — decided rules

Status: Rev 0, 2026-07-22. Resolves every semantic collision between the
TS surface syntax and the language's semantics, and every Q-id in
`specs/blocks/corpus.md` §5 not marked deferred. Each rule names its
accept and reject corpus entries. The design principle applied
throughout: **stock `tsc` accepts a superset; this compiler narrows.** A
construct `tsc` cannot police is policed by this compiler with its own
diagnostic; the `tsc` gate stays trivially green and soundness lives here.

## 1. Collision rules

### C1. Structural vs nominal — nominal per declaration

Every `class` (value or reference) is a distinct nominal type. Two
identically-shaped classes are not interchangeable; object literals do not
satisfy class types. `tsc` cannot enforce this (TS classes without private
members are structural); this compiler rejects structural substitution.
Accept: `a05`. Reject: `r06-structural-substitution` (passes a same-shaped
class instance where the other nominal type is expected — `tsc`-clean by
design).

### C2. Value types (Q2) — `@value class`

A class marked with the ambient `@value` decorator (TC39 standard decorator
syntax, TS 5 default; no `experimentalDecorators`) is a value type:
C-layout, copy-on-assign, copy-on-pass, copy-on-index. Rules:

- Value classes may not `extends` anything and may not be extended.
- Field types: sized numerics, `boolean`, other value classes,
  `FixedArray`, enums. Reference-class fields, `string` fields, and
  nullable fields inside value classes are deferred until a corpus program
  needs them (not decided, tracked as open in §3).
- Plain `class` is a reference type: heap, Context-allocated, manual
  lifetime (Q6).
- `const` binding of a value struct blocks rebinding only; field writes
  through it are legal (Q17 — matches `tsc`; C-style `const struct`
  semantics are not imported).

Accept: `a04`, `a21`. Reject: `r07-value-class-extends` (`@value class`
with an `extends` clause; `tsc`-clean).

### C3. `number` and sized numerics (Q1) — bare `number` rejected

`i32`, `u32`, `i64`, `u64`, `f32`, `f64` are ambient aliases of `number`
(AssemblyScript precedent). They are aliases, not brands: `tsc` sees
`number` everywhere; this compiler tracks the declared sized type and
enforces it. Bare `number` in any declaration is rejected — there is no
default numeric type. Conversions are spelled `x as T` (Q1): under `tsc`
an identity assertion, under this compiler an explicit checked conversion
with C semantics (truncation, wrapping per target type). Implicit numeric
conversions do not exist; mixed-type arithmetic without `as` is rejected.
`as` also spells checked reference narrowing: `x as C`, where `x` is the
boundary-opaque `object | null` (C7 boundary forms), traps (C6 model) on
`null` or on class mismatch, in both tiers.
`i64`/`u64` values are exact 64-bit at runtime in both tiers; the `tsc`
view (`number`) cannot express integer literals above 2^53 − 1, so such
literals are out of the surface syntax until a need is evidenced.
Accept: `a02`. Reject: `r08-bare-number` (`number` in a declaration).

### C4. Integer literals — contextual typing

A suffix-less integer literal adopts the sized type of its context
(initializer annotation, parameter type, field type, array element type).
A literal with a fraction or exponent adopts the contextual float type;
it is an error in an integer context. Out-of-range literals for the
contextual type are rejected at compile time. Context-free integer
literals (e.g. `const x = 3` with no annotation) default to `i32`;
context-free fractional literals default to `f64`.
Accept: `a03`. Reject: `r09-int-literal-overflow` (`const x: i32 =
3000000000`; `tsc`-clean).

### C5. Closures (Q10) — non-escaping capture only

- Function values that capture nothing are function pointers; freely
  passable, storable, and usable as C callbacks.
- A lambda may capture only immutable (`const`) locals, by value, and only
  if the lambda does not escape its defining function: it may be called
  locally and passed downward as an argument, but not returned, not stored
  in a field or array, and not passed where a C callback is expected.
  Its environment lives on the stack; no allocation.
- C callbacks (plan §4 pattern 4) therefore take the manual form: a
  non-capturing function plus explicit `userdata`.

Accept: `a13`, `a14`. Reject: `r10-escaping-capture` (returns a capturing
lambda; `tsc`-clean).

### C6. Exceptions (Q9) — out

`throw`, `try`/`catch`/`finally` are not in the language. Fallible
operations return result values (`a18` pattern). Runtime faults (index out
of bounds, failed narrowing, allocation failure) trap: the Context stops
with a diagnostic carrying a source position; the host decides what
happens next. Trapping is not catchable in-language.
Accept: `a18`. Reject: `r11-throw` (`throw` statement; `tsc`-clean).

### C7. Unions, `null`, `undefined` (Q8) — `T | null` only

In-language, the only union form is `Ref | null` where `Ref` is a
reference class, opaque handle, function type, or boundary struct pointer.
General unions (`i32 | string`), value-class-with-null, and every use of
`undefined` (including optional properties `x?: T` and optional parameters
without defaults) are rejected.

At the C boundary two additional null forms exist; neither is available
to general declarations:

- `object | null` for `void*` userdata slots. `object` is the
  boundary-opaque reference type — any reference-class instance may cross;
  it returns to a concrete type only through checked `as` narrowing (C3).
- `Struct | null` for zeroable by-value sub-layouts, where `null` lowers
  to the zeroed struct.

Parameters with defaults (`a11`) are legal — the default fills the value;
no `undefined` is observable. In-language `Ref | null` lowers to a
nullable pointer; narrowing is required before member access (`tsc`
already enforces this under `strictNullChecks`).
Accept: `a17`. Reject: `r12-general-union` (`i32 | string` field;
`tsc`-clean), `r13-undefined` (`undefined` in an annotation/expression;
`tsc`-clean).

### C8. `async` / generators (Q11) — coroutines only

`function*` maps to a language coroutine: the host (or script) drives it
with `.next()`; `yield` suspends. `.next()` returns the language-level
value-struct shape `{ done: boolean; value: T }`, with `value`
zero-initialized when `done` — the `undefined`-bearing TS lib
`IteratorResult` is only the `tsc` view (accepted superset; C7's
`undefined` ban applies to the language type, not the lib's spelling). No
event loop exists. `async`, `await`, and `Promise` are rejected. Host
async C APIs surface as C-style callbacks (plan §4 pattern 4), not
promises.
Accept: `a20`. Reject: `r14-async` (`async function`; `tsc`-clean).

## 2. Q-register resolutions not covered above

- **Q3 (`FixedArray`)** — ambient
  `interface FixedArray<T, N extends number> { [index: number]: T;
  readonly length: i32; }`. `N` is not used structurally (a plain array
  literal must remain assignable for construction); this compiler reads
  `N` from the annotation and enforces length and element type. Lowers to
  a C array `T[N]` in-place.
- **Q4/Q15 (arrays and slices)** — `T[]` is the language's dynamic array:
  Context-allocated storage, explicit growth via `push`. The permitted
  surface is `length`, indexing, `push`, `pop`; other `Array.prototype`
  members are rejected by this compiler (`tsc` accepts them; no reject
  entry per member — the whitelist is the rule). At a C boundary a `T[]`
  argument lowers to its `(ptr, len)` pair; the callee borrows, the
  caller retains ownership.
- **Q5 (strings)** — `string` is an immutable UTF-8 byte view
  `(ptr, len)`; no NUL terminator assumed. `length` is the byte length
  (`i32`). `slice(start, end)` takes byte offsets and traps off a UTF-8
  boundary (C6 trap model). Permitted surface: `length`, `slice`,
  `+`/template-literal concatenation, `===`/`!==` (by content).
- **Q6 (`unsafeDelete`)** — `declare function unsafeDelete(value: object):
  void;` frees a reference-class instance immediately. Double delete and
  use-after-delete trap in the development tier and are undefined in AOT
  (trusted scripts, invariant 6).
- **Q7 (`collect`)** — `declare function collect(): void;` — explicitly
  invoked collection of unreachable Context allocations. Also invocable
  host-side. Never runs unbidden (invariant 2).
- **Q12 (entry and host API)** — every `export function` is a
  host-callable entry point. The corpus runner calls `main(): void`;
  `a23` exports a lifecycle trio (`init`/`update`/`shutdown`) for
  host-driven use, and for the corpus run its own `main` drives them so
  the run set stays headless. Prelude host API for the corpus:
  `print(message: string): void`. (Q16, host-created handles, is deferred
  to P5 — corpus.md §5.)
- **Q13 (host C-header mirror)** — deferred to P5 (corpus.md §5). Boundary
  typing rules already decided here and binding on the P5 generator:
  opaque handles are branded empty interfaces (nominal enough that handles
  do not cross-assign under `tsc`); struct pointers and zeroable by-value
  sub-layouts are `X | null` (C7 boundary forms); length-carrying string
  views are `string` (Q5 makes the shapes identical); flag sets are `u64`
  aliases combined with `|` (Q18); callback userdata slots are
  `object | null`, narrowed with `as` (C3), with the lifetime rule that
  userdata must outlive the registration that holds it.
- **Q14 (numeric formatting)** — template-literal interpolation of sized
  numerics is defined by the language runtime, not the host libc:
  integers in decimal; `f32`/`f64` by shortest round-trip (Ryu class
  algorithm), with integral values printed without a decimal point or
  exponent (`7`, never `7.0` or `7E0`); `-0`, `NaN`, `Infinity` spelled
  `-0`, `NaN`, `Infinity`.
  Both tiers share one implementation; byte-identical output is a
  standing differential-gate assertion (plan P3).
- **Q17** — decided in C2. **Q18** — `|`, `&`, `^`, `~`, shifts on `i64`/
  `u64` are true 64-bit operations (JS 32-bit truncation is not imported);
  on 32-bit types they match C. Mixed-width bitwise operands require `as`.
- **Q19 (`Math`)** — the checker accepts a deterministic subset of the
  lib's `Math` with `f64` signatures and ECMA result semantics
  (`stdlib.md` §1). Rejected: `imul`/`clz32`/`fround` (JS-number ops;
  the language has sized integers), variadic `max`/`min`/`hypot` (two
  arguments only). `Math.random` diverges from JS: Context-seeded
  deterministic PRNG, host-reseedable (`stdlib.md` §2).
- **Q20 (`Date`)** — the checker accepts the UTC-deterministic subset
  only, as an immutable value erasing to `i64` epoch millis
  (`stdlib.md` §3); on that subset semantics equal JS on a UTC host.
  Rejected: local-time accessors, setters, `parse`, `toString`
  family, the multi-argument constructor (lib semantics are local
  time — accepting it as UTC would silently change meaning), the
  zero-argument constructor (nondeterministic current time — write
  `new Date(Date.now())`), `Date` in template literals, and direct
  `Date` comparison (`===`, `<`, … — compare `getTime()` values).
  Out-of-range times trap; there is no Invalid-Date value.

## 3. Open items carried forward

- Value-class fields of reference/string/nullable types (C2): undecided
  until a corpus program needs them; the field-type whitelist stands.
- `i64`/`u64` literals above 2^53 − 1 (C3): no surface spelling; revisit
  with evidence.
- Generic constraints/variance beyond monomorphized `a12` shapes: revisit
  with corpus evidence.

## 4. Prelude and gate

- `prelude/lang.d.ts` — ambient declarations for §1/§2: sized-numeric
  aliases, `print`, `collect`, `unsafeDelete`, `value` decorator
  (typed against TS 5 standard `ClassDecoratorContext`), `FixedArray`.
- `tsconfig.json` (repo root) — `strict`, `noEmit`, ES2022 target/lib,
  `types: []`; includes `prelude/**/*.d.ts` and `corpus/accept/**/*.ts`
  only. The reject corpus is excluded (corpus.md §2).
- Gate: `tsc -p tsconfig.json` — zero errors, standing.
