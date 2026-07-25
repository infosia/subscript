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

### C2. Value types (Q2) — `@CStruct class`

A class marked with the ambient `@CStruct` decorator (TC39 standard decorator
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

Accept: `a04`, `a21`. Reject: `r07-value-class-extends` (`@CStruct class`
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
  algorithm), with integral values in the ordinary range printed without
  a decimal point or exponent (`7`, never `7.0` or `7E0`); `-0`, `NaN`,
  `Infinity` spelled `-0`, `NaN`, `Infinity`.
  **Exponent thresholds (owner decision 2026-07-25, correcting the
  original rule):** a magnitude outside `[1e-6, 1e21)` is printed in
  exponential form, exactly as ECMA's `Number::toString` does — `1e-7`,
  `5e-324`, `1e+21`, `1e+300`. The rule as first written said "without
  … exponent" without qualification, which was aimed at `7` rather than
  `7.0`; taken literally it also banned exponents at the extremes, so
  `${5e-324}` produced a **751-character** string and `${1e21}` diverged
  from every JS engine. That was a consequence, not a decision: §0.4 of
  `stdlib.md` makes ECMA the default and no divergence was recorded for
  it. Adopting ECMA's thresholds restores agreement with the surface
  language, removes the pathological output, and makes `toFixed`'s
  ECMA-specified fallback above 1e21 coherent (Q25) instead of
  contradicting the interpolation form for the same value. Consequence:
  one frozen golden moves (`a49`'s f16 subnormal, `0.000…5960464477539063`
  → `5.960464477539063e-8`) under the `compiler.md` §2 golden-change
  procedure.
  Both tiers share one implementation; byte-identical output is a
  standing differential-gate assertion (plan P3).
- **Q17** — decided in C2. **Q18** — `|`, `&`, `^`, `~`, shifts on `i64`/
  `u64` are true 64-bit operations (JS 32-bit truncation is not imported);
  on 32-bit types they match C. Mixed-width bitwise operands require `as`.
  **Shift amount ≥ the operand width (owner decision 2026-07-25):** the
  amount is taken **modulo the operand width** — `x << k` shifts by
  `k & (width − 1)` — for every width including the Q23 narrow types,
  identically on both tiers. C leaves this undefined and the undefined
  behaviour was observed: the ship tier returned different results on
  re-runs of the same `i32` program while the dev tier (Cranelift, which
  masks) was stable, so "match C" has no meaning here and the rule is
  stated explicitly instead. Masking is chosen over trapping because it
  is total, free (both ISAs mask in hardware), already what the dev tier
  does, and what the TypeScript surface leads a reader to expect
  (`1 << 32 === 1`). The ship tier must emit the mask explicitly: C
  promotes a narrow operand to `int` before shifting, so an unmasked
  emission diverges. Additionally, a **literal** shift amount ≥ the
  operand width is rejected at compile time (S008, the out-of-range
  literal rule) — a constant over-shift is a typo, and C4 already
  rejects out-of-range literals rather than silently reinterpreting
  them.
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
- **Q21 (`String` methods)** — strings are immutable UTF-8 byte
  strings; every accepted index/length/code-unit measure is a **byte**
  (the standing meaning of `length`/`slice`). ASCII programs behave as
  JS; non-ASCII values diverge from UTF-16 units (recorded, not
  hidden). Case mapping and whitespace are ASCII-only. Range/argument
  errors trap (`charCodeAt` OOB, `repeat(-1)`, `split("")`,
  `replaceAll("", …)`, empty-`pad` padding — JS returns NaN or silent
  no-ops there). `replace`/`replaceAll` are literal: `$` substitution
  patterns are not interpreted. Rejected members: `stdlib.md` §8.
- **Q22 (`Array` methods)** — the checker accepts the `stdlib.md` §9
  subset on `T[]`. Element equality follows JS `===` per element
  kind (scalars by value, strings by content, `Date` by millis,
  reference classes by identity) — including `includes`, which in JS
  uses SameValueZero (JS finds `NaN`; this language never does — one
  equality rule for all three searches). Callback arities are fixed (no
  optional index/array parameters); `reduce` requires `init` (the
  lib's arity-overloaded no-init form changes meaning silently);
  `sort` requires a comparator (the lib's default sort coerces to
  strings); `find` is rejected (no miss value for scalar element
  types — use `findIndex`). Callbacks are non-escaping (C5) by
  construction; a callback trap aborts the iteration.
- **Q23 (narrow numerics: `i8`/`u8`/`i16`/`u16`/`f16`)** — five further
  ambient aliases of `number`, extending C3's table. They exist because
  real C headers and GPU buffer formats are full of byte and half-width
  fields: without them `bindgen`'s scalar map has no entry for
  `uint8_t`/`uint16_t`/`char`/`short` and fails loud, so a header with a
  single byte field cannot be bound at all.
  - **Storage and interchange, not a new arithmetic domain.** The five
    behave exactly as C3/C4/Q18 already specify for the existing sized
    types: bare `number` still rejected; conversions spelled `x as T`
    with C truncation/wrapping semantics; no implicit conversion; mixed
    width requires `as`; contextual literal typing (C4) with
    out-of-range literals rejected at compile time.
  - **`f16` is storage-only (owner decision 2026-07-25).** `f16`
    declares fields, array elements and boundary parameters, and
    converts to and from `f32`/`f64` with `as`. **Arithmetic on `f16`
    operands is rejected (S014)** — compute via `as f32`. Reason: an
    arithmetic `f16` is a cross-tier determinism hazard of exactly the
    kind §0.2 of `stdlib.md` records for libm — the C tier's `_Float16`
    (arithmetic in half) and `__fp16` (operands promoted to `f32`)
    differ in where rounding happens, and the dev tier's own half
    support is a separate implementation. A rejection can be relaxed
    later on measured evidence; a silently diverging arithmetic cannot
    be un-shipped. Conversion is one runtime implementation behind an
    opaque symbol on both tiers (§0.2's rule), never an emitted
    compiler builtin.
  - **Integer arithmetic on the narrow integer types** follows C3
    unchanged: operands must already share a type, and the result is
    that type with C wrapping — the language does not import C's
    integer promotion (there is no implicit widening to `int` to
    import, since there are no implicit conversions).
  - **`Q18` extends unchanged**: bitwise and shifts on `i8`/`u8`/`i16`/
    `u16` match C at that width; mixed-width operands require `as`.

- **Q24 (`Map`/`Set`)** — accepted per `stdlib.md` §10. They are
  **generic reference classes** monomorphized on first use, so keys and
  values are stored unboxed. **Key kinds are whitelisted** to those Q22
  already defines equality for (sized integers, `boolean`, `enum`,
  `f32`/`f64` by `===`, `string` by content, `Date` by millis,
  reference classes by identity); `f16` (Q23 storage-only), `T[]`,
  `FixedArray`, `@CStruct` value classes, `object`, function types and
  `Nullable<T>` are rejected as keys (S014). **Iteration is insertion
  order** and is normative, not incidental — §0.3 determinism and the
  golden corpus both depend on it; overwriting a present key keeps its
  position, deleting and re-inserting appends. `get` returns
  `V | null` only where `V` is nullable-capable; for a scalar `V` it is
  rejected in favour of `has` plus the total `getOr(k, fallback)`,
  because a zeroed miss value is silently wrong for a program that
  stores zero (the same reasoning that rejected `find` in Q22). The
  hash is the runtime's own, deterministic and **seed-free** — a
  per-Context random seed would make iteration order, the goldens and
  replays non-reproducible. The iterator protocol is not in the
  language, so `keys`/`values`/`entries`/`for…of`/spread and
  iterable construction are rejected; `forEach` is the traversal.

- **Q25 (`Number`, parsing, `toFixed`)** — accepted per `stdlib.md`
  §11. `Number`'s constants and the four `Number.is*` predicates are
  accepted with ECMA semantics; the **coercing** globals `isNaN`/
  `isFinite` and `Number(x)` are rejected (coercion is not in this
  language). `parseInt` **requires an explicit radix** (2–36; out of
  range traps): ECMA's default is context-dependent, the same
  arity-changes-meaning hazard Q22 rejected for `reduce`/`sort`.
  `parseInt`/`parseFloat` return **`f64` with `NaN` as the failure
  value** — the one place a sentinel is accepted, because parse failure
  is *data* rather than a programmer error, and because `NaN` is
  representable in `f64`, outside the domain of any successful parse,
  and checkable with `Number.isNaN`. That is precisely what was absent
  when Q20 rejected Invalid-Date (`Date` erases to `i64`, which has no
  NaN) and when Q24 rejected a zeroed `get` miss (zero is a legitimate
  stored value). `toFixed(digits)` is fixed-decimal and therefore the
  only numeric string that is not Q14's shortest round-trip; its
  half-way, `±0`, `>= 1e21`, `NaN` and infinity cases are pinned by
  golden, and it is one runtime implementation on both tiers rather
  than the host libc, whose rounding is platform-dependent.

## 3. Open items carried forward

- Value-class fields of reference/string/nullable types (C2): undecided
  until a corpus program needs them; the field-type whitelist stands.
- `i64`/`u64` literals above 2^53 − 1 (C3): no surface spelling; revisit
  with evidence.
- Generic constraints/variance beyond monomorphized `a12` shapes: revisit
  with corpus evidence.

## 4. Prelude and gate

- `prelude/lang.d.ts` — ambient declarations for §1/§2: sized-numeric
  aliases, `print`, `collect`, `unsafeDelete`, `CStruct` decorator
  (typed against TS 5 standard `ClassDecoratorContext`), `FixedArray`.
- `tsconfig.json` (repo root) — `strict`, `noEmit`, ES2022 target/lib,
  `types: []`; includes `prelude/**/*.d.ts` and `corpus/accept/**/*.ts`
  only. The reject corpus is excluded (corpus.md §2).
- Gate: `tsc -p tsconfig.json` — zero errors, standing.
