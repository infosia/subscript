# Semantic collisions — decided rules

Status: Rev 0, 2026-07-22. Resolves every semantic collision between the
TS surface syntax and the language's semantics, and every Q-id in
`specs/blocks/corpus.md` §5 not marked deferred. Each rule names its
accept and reject corpus entries. The design principle applied
throughout: **stock `tsc` accepts a superset; this compiler narrows.** A
construct `tsc` cannot police is policed by this compiler with its own
diagnostic; the `tsc` gate stays trivially green and soundness lives here.

**A retired entry keeps its record.** A corpus name this file no
longer expects to exist is written `retired:<name>`, so the check of
§69 stage 3 skips it by its spelling and not by reading the prose
around it. *(Added 2026-08-27. Three names — `r14-async` twice and
`r104` — were retired in prose, and a check that reads names would
report them as missing for ever. Deleting them would lose why the
entry went; keying the check on the word "retired" in a sentence
would put a check back on prose, which §69 exists to end.)*

## 1. Collision rules

### C1. Structural vs nominal — nominal per declaration

Every `class` (value or reference) is a distinct nominal type. Two
identically-shaped classes are not interchangeable; object literals do not
satisfy class types — with one contextual exception: a literal in a
position expecting a **Q33 `@Descriptor` class** *constructs* that class
(the literal has no standalone type, so nominality is undisturbed).
`tsc` cannot enforce this (TS classes without private
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
`i64`/`u64` values are exact 64-bit at runtime in both tiers. An integer
literal reads from its spelling at the target's width, so the full `i64`
and `u64` ranges have surface spellings (R26 evidence; `compiler.md`
§56). The `tsc` view (`number`) rounds such a literal and accepts it:
TS 80008 is a suggestion, not an error (measured, `tsc` exit 0).
Accept: `a02`, `a132`. Reject: `r08-bare-number` (`number` in a
declaration), `r124` (`u64` overflow), `r125` (`i64` underflow).

*Revised 2026-08-28 (§68.7.2):* a float to integer `as` conversion
saturates, and `NaN` converts to `0`. JavaScript has no such
conversion; `as` is a no-op there and the double prints as itself
(`1e10` against `2147483647`). A program that prints such a value
cites C3. Float `%` is the C `fmod`, which equals JavaScript's `%` in
every IEEE case, so it is comparable and cites nothing. Accept adds
`a167` (the conversions; cites C3) and `a165` (float `%` and the empty
template; comparable).

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

**Q33 exception (owner, 2026-07-31): defaulted optional members on
descriptor classes.** Inside a `@Descriptor` class (Q33) — and only
there — `name?: T = expr` is legal: `?` requires the initializer, the
initializer requires the `?`, omission in a constructing literal takes
the default, and no `undefined` is observable — C7's parameter-default
rule (`a11`) extended to members. Everywhere else `?` stays rejected.

**Q32 exception (owner, 2026-07-31): closed string-literal unions as
named aliases.** `type Name = "a" | "b";` declares a closed,
nominal-by-alias literal set (Q32; `compiler.md` §24). This is far
inside the rejected general-union space — one primitive, literals
only, closed, alias-only: an *inline* literal union in any other
position stays rejected as a general union.

*Revised 2026-08-02 (R14):* a Q32 alias is a legal `switch`
discriminant with `case "member":` arms — integer dispatch, and
closed-set exhaustiveness checked (`compiler.md` §41; accept adds
`a115`, reject adds `r112`–`r114`).
*Revised 2026-08-02 (R16):* inside a `@Descriptor` class, `name?: A`
with no initializer (A a Q32 alias) declares an absence-capable
member — absent is distinct from every value, spellable only by
omission, read only through `!== undefined` presence narrowing;
that comparison is the single legal appearance of the `undefined`
token (C7 stands everywhere else). `compiler.md` §43; accept adds
`a118`, reject adds `r117`–`r118`.
Accept: `a17`, `a91` (Q32 aliases). Reject: `r12-general-union`
(`i32 | string` field; `tsc`-clean), `r13-undefined` (`undefined` in
an annotation/expression; `tsc`-clean), `r87`–`r89` (Q32 boundaries).

### C8. `async` / generators (Q11) — coroutines only

`function*` maps to a language coroutine: the host (or script) drives it
with `.next()`; `yield` suspends. `.next()` returns the language-level
value-struct shape `{ done: boolean; value: T }`, with `value`
zero-initialized when `done` — the `undefined`-bearing TS lib
`IteratorResult` is only the `tsc` view (accepted superset; C7's
`undefined` ban applies to the language type, not the lib's spelling). No
event loop exists.

**Revised 2026-07-31 (Q34): `async`/`await` are accepted** as poll-driven
sugar over the same Context-owned frame machinery — no scheduler, no
microtask queue, no `Promise` object; the lib `Promise<T>` is only the
`tsc` view of an async function's value, exactly as `IteratorResult` is
for coroutines. The model, lifetimes, and the retired entry
`retired:r14-async` are Q34's; `Promise` construction and combinators stay rejected. Host
async C APIs still surface as C-style callbacks plus poll functions
(plan §4 pattern 4); `await` consumes them through script-level polling.
Accept: `a20`, `a93`–`a95`. Reject: `r96`–`r100` (Q34 boundaries;
`retired:r14-async` by Q34 — the construct it pinned is now legal).
*Revised 2026-08-02 (R13):* async instance methods on plain,
non-generic reference classes join the surface —
`await recv.m(...)` as a third direct-await form (`compiler.md`
§37). Accept adds `a110`–`a111`; reject adds `r101`–`r105`. *Revised 2026-08-23 (R36):* the class can be generic, and
a generic async function with explicit type arguments is awaitable
(`compiler.md` §64). Accept adds `a143`; reject adds `r140`;
`retired:r104`. *Revised 2026-08-27 (§70):* the result of an async
call is a reference-counted handle that can be held, stored, passed,
and awaited later; dropping one without an await stays rejected
(`r100`, `r105` rewritten to that form). Accept adds `a154`–`a155`;
reject adds `r157`.

### C9. Field initializers — every construction, no `this`

*(R27, 2026-08-15; `compiler.md` §57.)* A declared field
initializer runs on every construction, in declaration order, with
or without a declared constructor. Constructor arguments evaluate
before the initializers; the constructor body runs after them.
This is the TS order (measured under `node`, exit 0).

A field initializer must not read `this`. Stock `tsc` accepts
`this` there, so this is a narrowing: the checker rejects it with
S100. Before R27, no program with `this` in a field initializer
ran on the dev tier (internal lowering error), so the narrowing
retires no working program.

Accept: `a133`, `a134`. Reject: `r126-this-in-field-init`.

### C10. Class index signatures — accessor sugar

*(R29, 2026-08-15; `compiler.md` §58.)* A reference class can
declare one index signature, `[index: I]: T` or
`readonly [index: I]: T`, with `I` = `i32` or `u32`. The class
must declare `get(index: I): T`, and, when the signature is not
`readonly`, `set(index: I, value: T): void`. A read `a[i]` checks
to the same HIR as `a.get(i)`; a statement write `a[i] = v` checks
to the same HIR as `a.set(i, v)`.

The divergence from JS: an index signature in TS types numeric
property access, and JS reads the property. subscript has no
dynamic properties, so the same spelling calls the declared
accessor. Stock `tsc` accepts every accepted program through the
declared signature. Narrowings on top of `tsc`: a write through a
`readonly` signature, compound assignment, increment, decrement,
the write used as a value, and an index signature on a value
class all fail at check time.

Accept: `a136`. Reject: `r128-readonly-index-write`,
`r129-index-signature-no-get`, `r130-index-compound-assign`.

### C11. `using` declarations — no null binding, no dispose on trap

*(R31, 2026-08-16; `compiler.md` §60.)* `using x = expr` binds an
immutable reference to a class that declares
`[Symbol.dispose](): void`, and the hook runs at every scope exit
in reverse declaration order. The exit-order semantics are the TS
semantics (measured under `node` v24.18.0, exit 0): the return
expression evaluates first, a loop disposes per iteration, and an
`async` frame that suspended disposes at completion.

Two divergences from JS, both narrowings or subtractions:

- JS skips disposal for a `null` or `undefined` binding. subscript
  rejects a nullable initializer at check time (owner decision,
  2026-08-16): narrow first, then bind.
- JS runs disposal during throw-unwind. subscript has no
  exceptions (C6), and a trap does not run dispose (§18.1b, no
  rollback; owner decision, 2026-08-16).

`await using` is rejected (S100). The explicit spelling
`x[Symbol.dispose]()` stays rejected; the manual cleanup call is
an ordinary method the class declares.

Accept: `a138`, `a139`. Reject: `r131-using-nullable-init`,
`r132-await-using`, `r133-using-without-dispose`.

### C12. Named accessors — method sugar

*(R37, 2026-08-25; `compiler.md` §65.)* A class declares
`get name(): T { ... }` and `set name(value: T) { ... }`, both with
a body. A read accessor is legal on a reference class and on a
`@CStruct` value class; a write accessor is legal on a reference
class only. A read `x.name` checks to the same HIR as a call of the
read accessor; a statement write `x.name = v` checks to the same
HIR as a call of the write accessor. The pair records as the
methods `name` and `name=`, and it owns the name in the class
member namespace. A read accessor that returns a value class
returns a copy, so a write into that copy is dropped, exactly as a
write into the result of the spelled method call is dropped (C2).

The divergence from JS: JS runs an accessor function on property
access, and the property is dynamic. subscript has no dynamic
properties, so the same spelling calls the declared method. Stock
`tsc` accepts every accepted program. Narrowings on top of `tsc`:
a write through a read-only accessor, compound assignment,
increment, decrement, the write used as a value, a write accessor
on a value class, and a static accessor all fail at check time.

Accept: `a144`. Reject: `r141-value-class-write-accessor`,
`r142-readonly-accessor-write`, `r143-accessor-compound-assign`,
`r144-accessor-increment`, `r145-accessor-write-as-value`,
`r146-accessor-field-name-clash`, `r147-static-accessor`.

### C13. Iteration over a container that changes — a fixed entry bound

`for...of` and `forEach` fix the entry bound when the traversal starts.
**An append does not extend the traversal, and a removal shortens it.**
JavaScript re-reads the length each step for an array, and a `Map` or a
`Set` iterator observes an entry appended during iteration.

*(Added 2026-08-27 by §69 stage 2, which measured the divergence and
had no id to cite. The behaviour was decided before this entry:
`corpus/accept/a80-for-of-foreach-mutation` states it in its own header
and pins it. This entry records the decision; it does not make one.)*

Measured on `a80`, `node` v24.18.0 against the committed golden:

    subscript   mut-map:1  mut-map:3
    node        mut-map:1  mut-map:3  mut-map:4

**The array half is matchable at a cost; the `Map` half is not.**
Matching an array needs the length re-read and, under §68.2 item 9, the
base address re-materialized every step, on the loop `a22` measures.
Matching a `Map` needs the traversal to observe an entry appended after
it started. This language's `Map` is flat insertion-ordered storage, so
an append can rehash and no position survives it. A cursor stable across
a rehash is an iterator object, and `stdlib.md` §14.3 rules that out:
"an iterator held as a value would be stateful and outlive the call that
produced it — the first escaping temporary in the language, and a
memory-model change (invariant 2) rather than a syntax addition."

So this is a decided divergence, forced by §14.3's fused index loop and
by the `Map`'s storage, not a gap to close later.

Accept: `a80`. Reject: none — the shape is legal and its value differs.

### C14. Declaration scope and order — this compiler rejects where it would diverge

Where this compiler and TypeScript disagree about which declaration a
name resolves to, **this compiler rejects. It never accepts a program
and gives it a different value.** A `switch` body is one scope, as
TypeScript has it; two declarations of one name in one scope are
rejected, a parameter and a body local included; a block-scoped
declaration owns its name for the whole block, against an ambient
namespace and a class name as well as a local.

*(Added 2026-08-27 by §69 stage 2. `compiler.md` §67.1 decided the rule
and its nine reject entries; the collision table carried none of it. The
table's granularity is a collision class, not one id per reject entry —
twelve ids covered 151 rejects before this — so this entry is the class,
not the temporal dead zone alone.)*

The temporal dead zone is the instance §66 measurement 6i recorded:
`node` resolved the name as `4` and this compiler as `3`. Under this
rule the shape is now rejected instead, so no program is accepted with
a different value.

**Matching TypeScript here is not available.** To match, this compiler
would accept the programs §66 and §67 measured, and those are the
programs whose two tiers printed different numbers with no diagnostic.

Accept: `a147`, `a148`. Reject: `r148`–`r156`.

## 2. Q-register resolutions not covered above

- **Q29 (the size limits)** — **two** limits, because two different
  things overflow. *(Revised 2026-07-26 after the P21 Phase Review; the
  first version had one limit and was wrong in both directions — see
  below.)*

  1. **A single aggregate: 2 147 483 647 bytes (`i32::MAX`).** Bounds
     one independently addressable aggregate. Cranelift addresses class
     fields, frame slots and globals with a **signed 32-bit
     displacement**, so an aggregate past this has no valid offset.
  2. **Accumulated Cranelift stack-frame storage: 2 147 483 632 bytes.**
     Derived, not chosen: the AArch64 ABI aligns the frame to 16 bytes,
     so the largest representable aligned frame is
     `floor((2^31 − 1) / 16) × 16 = 2^31 − 16`. Anything larger rounds
     up to at least `2^31`, where the ABI's negation of the offset
     overflows.

  Reference-class fields are subject to (1) but not (2) unless an
  instance actually occupies stack-frame storage — the exposure is
  stack-frame layout, and a large reference-class shape compiles.

  **Enforced by the checker**, with S100 at the responsible position
  and the limit named in the message, for: `FixedArray` layouts
  including nested and class-dependent ones; `@CStruct` value-class
  layouts; reference-class object layouts; `.next` `IterResult<T>`
  layouts; **closure environments**, by the resolved types of captured
  values; **generator frames**, including header, parameters and
  locals; and the accumulated stack-frame storage of (2).

  *(Added 2026-07-26. Before it, a program the checker accepted could
  **panic the compiler** — `@CStruct class Big { data:
  FixedArray<u8, 4294967295>; }` reached `attempt to add with overflow`
  in codegen's layout arithmetic. That violates core principle 5, and
  the shape was reachable from source **by construction** rather than
  by accident: layout multiplies an element size by a length taken from
  a source annotation. Found while checking whether P21's allocation
  fault-injection work could reach the "not representable" raise point
  without new machinery — it could not, but it found this instead.)*

  **The first revision of this entry did not achieve its own stated
  purpose, and the Phase Review measured it.** Two defects:

  - It set the limit at `i32::MAX` and bounded **one aggregate rather
    than the frame**, so `FixedArray<u8, 2147483640>` in a `@CStruct`
    local still panicked — `attempt to negate with overflow` in
    Cranelift's aarch64 ABI. The bisected boundary was 2 147 483 632,
    which is now limit (2). **The value this entry used as its own
    example, `i32::MAX`, was itself a panicking input.** And the
    workspace has no `[profile.release]`, so with `overflow-checks`
    off a release-built compiler would have **wrapped that negation
    instead of panicking, emitting a wrong stack offset silently** —
    the debug panic was the friendly face of it.
  - It **claimed checker enforcement for closure environments and
    generator frames when neither was checked.** They were summed only
    in codegen, so the user got an internal compiler error instead of
    a positioned S100 — and worse, the **ship tier accepted programs
    the dev tier refused to compile**, which the two-tier equivalence
    premise forbids.

  The scalar layout table is now **shared between the checker and
  codegen** rather than duplicated. It was byte-for-byte identical in
  two crates with no test that they agreed, and a drift would move the
  checker's enforced limit off the backend's real bound — which is
  exactly how the first revision failed.

  *(Added 2026-07-26. Before it, a program the checker accepted could
  **panic the compiler** — `@CStruct class Big { data:
  FixedArray<u8, 4294967295>; }` reached `attempt to add with overflow`
  in codegen's layout arithmetic. That violates core principle 5, and
  the shape was reachable from source **by construction** rather than
  by accident: layout multiplies an element size by a length taken from
  a source annotation. Found while checking whether P21's allocation
  fault-injection work could reach the "not representable" raise point
  without new machinery — it could not, but it found this instead.)*

  The check lives in the checker, where every other size rule lives
  (C3's out-of-range literals, C4's contextual typing, the existing
  `FixedArray` length-mismatch check), so the user gets a source
  position. Codegen's layout arithmetic is **also** fully checked and
  returns `Err` rather than panicking, so HIR that reaches it by some
  other route still cannot crash the compiler.

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
  boundary (C6 trap model). **Out-of-range offsets clamp rather than
  trap** (JS's negative/clamp rules) — changed by P18, recorded in
  `stdlib.md` §8 with the reason and the cost. Permitted surface: `length`, `slice`,
  `+`/template-literal concatenation, `===`/`!==` (by content).
- **Q6 (`Context.free`)** — `Context.free(value: object): void` frees a
  reference-class instance immediately. Double free and use-after-free are
  **undefined in both tiers by default** (trusted scripts, invariant 6).
  The development tier can be made to **trap** on them by a host-set,
  per-Context setting that is off by default (`compiler.md` §8.1a-1);
  freeing a pointer the Context never owned is gated by the same setting.
  The setting carries a size threshold (§8.1a-2) and a retention byte
  budget (§8.1a-3, default 1 GiB): the trap is guaranteed only for
  allocations whose
  payload meets the host-set minimum and whose retained records still fit
  the budget — oldest evicted first — and is best-effort otherwise.

  *(Amended 2026-07-29. This clause said the dev tier traps, full stop.
  That became false when retention moved off the default path, and this
  table is the subset's definition — CLAUDE.md — so it cannot lag the
  contract.)*

  *(Renamed from `unsafeDelete` 2026-07-28, owner decision. `free` names
  the C memory model the language advertises, and a reader who writes C
  ABI hosts already pairs it with use-after-free, so the warning survives
  the adjective's removal. `delete` was considered and rejected:
  JavaScript's `delete` is a property-removal operator, and a language
  whose premise is TypeScript syntax over C semantics should not add an
  avoidable entry to this table.*

  *A second argument was given at the time and is **withdrawn**: that
  `unsafe` described the ship tier only, because the dev tier trapped.
  Under the default that premise no longer holds, and the old name would
  now be the more literal one. The rename stands on the C-vocabulary
  argument alone, which never depended on the trap — every C `free` has
  exactly these semantics and none is called `unsafeFree`.)*
- **Q7 (`Context.collect`)** — `Context.collect(): void` — explicitly
  invoked collection of unreachable Context allocations. Also invocable
  host-side. Never runs unbidden (invariant 2).

  *(Renamed from the bare global `collect` 2026-07-28, owner decision. A
  top-level `collect()` names no owner and reads as a library helper;
  qualifying it names the object that owns the memory. `Context` is
  already the host's word for it — `subscript_rt_context*` and `subscript_rt_ctx_*` in
  `runtime/include/subscript_runtime.h` — so script and host now say the
  same word for the same object.)*

  *(`Context.gc` was considered and rejected 2026-07-29, owner decision.
  `gc` names a subsystem this language does not have (invariant 2) and
  imports the wrong intuition — `System.gc()`-style calls are advisory
  hints, while `Context.collect()` is deterministic and synchronous.
  `collect` is also the established verb for the explicit act even where
  a GC exists: `GC.Collect()` in C#, `collectgarbage("collect")` in Lua
  (docs).)*

  **Spelling.** `Context` is an ambient namespace, never a class: a script
  cannot construct or hold one. `declare namespace Context { function
  collect(): void; function free(value: object): void; }` is `tsc`-clean.
  Should a future member need a reserved word, the namespace form fails —
  measured 2026-07-28, `declare namespace C { function delete(): void }`
  is TS1359 — and the fallback is the object form, `declare const Context:
  { … }`, which accepts reserved-word members and was verified to accept a
  `delete` member and its call site.

  `print` stays a bare global. It writes to the same Context sink, but it
  is not a memory operation and it is the most-called name in every corpus
  entry and example; the change would cost more than it names.
- **Q12 (entry and host API)** — every `export function` is a
  host-callable entry point. The corpus runner calls `main(): void`;
  `a23` exports a lifecycle trio (`init`/`update`/`shutdown`) for
  host-driven use, and for the corpus run its own `main` drives them so
  the run set stays headless. Prelude host API for the corpus:
  `print(message: string): void`. (Q16, host-created handles, is deferred
  to P5 — corpus.md §5.)

  **A non-returning export is unrecoverable by design** *(owner,
  2026-07-29 — `specs/tracking/long-run-audit.md` finding 3)*. Exported
  calls are synchronous and nothing can interrupt one: no fuel, no
  watchdog, no cross-thread cancellation. A script that fails to return
  freezes the host's calling thread until the process ends. This is the
  cost of invariant 6 (trusted scripts) and of an execution model with no
  per-iteration overhead; the regex budget stays the one deliberate
  exception, because pathological regex cost is data-driven rather than a
  code bug. Hosts that need isolation against a hung script must supply
  it themselves (a worker thread they can abandon, a process boundary).
- **Q13 (host C-header mirror)** — deferred to P5 (corpus.md §5). Boundary
  typing rules already decided here and binding on the P5 generator:
  opaque handles are branded empty interfaces (nominal enough that handles
  do not cross-assign under `tsc`); struct pointers and zeroable by-value
  sub-layouts are `X | null` (C7 boundary forms); length-carrying string
  views are `string` (Q5 makes the shapes identical); flag sets are `u64`
  aliases combined with `|` (Q18); callback userdata slots are
  `object | null`, narrowed with `as` (C3), with the lifetime rule that
  userdata must outlive the registration that holds it.
  **The binding's cost is bounded by distinct identities, not by
  registrations** *(2026-07-29 — `compiler.md` §14.4a)*: bindings are
  interned by `(code, userdata1, userdata2)`, so re-registering the same
  callback with the same userdata allocates nothing, and the honest bound
  is the astral-intern/pattern-cache one — distinct tuples used.


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
  **Closed — the decimal-tie divergence no longer exists.** It was
  recorded 2026-07-25 by the P12 review: Rust's shortest-round-trip
  writer broke an exact tie away from zero where ECMA breaks to even,
  339 divergences over 3 010 916 `f64` bit patterns, and the entry
  concluded that matching ECMA "needs a custom shortest-float writer"
  not worth hand-rolling. **That conclusion was wrong**: `ryu-js` — Ryū
  forked for ECMA semantics, the crate Boa uses — does exactly this and
  was already in the local cargo cache. Adopted the same day (`=1.0.3`,
  runtime only, still behind the opaque `subscript_rt_*` symbols per
  `stdlib.md` §0.2). Verified after the change: 200 000 random bit
  patterns, zero divergences from node on either tier; the entry's own
  example now agrees (`2205594957347911.25` prints
  `2205594957347911.2` here and in node). It also removed the
  hand-written exponent thresholds and `toFixed` rounding — net 111
  lines of hand-written float code deleted.
  **The one Q14 divergence that remains is the `-0` spelling above**,
  which is deliberate. The episode produced the standing rule recorded
  in `specs/tracking/js-alignment-audit.md`: a negative claim — "no
  solution exists" — needs investigation most of all.
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
  (`stdlib.md` §1). **`clz32` accepted** (Q26); **`imul` and `fround`
  accepted** (Q27, implemented 2026-07-26). Both entries were
  previously rejected, and both rejections have now been withdrawn for
  different reasons. `clz32` was rejected as a "JS-number op", which
  never applied to it: counting leading zeros has **no spelling in this
  language at all**. `imul` and `fround` genuinely *are* exact
  duplicates — `a * b` on `i32` and `x as f32` — and were rejected on
  that ground; the owner's rule (2026-07-25) is that a second spelling
  is not grounds for rejection when JS has the name. Variadic
  `max`/`min`/`hypot` stay rejected: the language has no variadic
  parameters, a missing prerequisite rather than a cost.
  `Math.random` diverges from JS: Context-seeded deterministic PRNG,
  host-reseedable (`stdlib.md` §2).
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
  hidden).
  **Case mapping and `trim` whitespace are full Unicode** (revised
  2026-07-25; both were ASCII-only, and the Boa audit —
  `specs/tracking/js-alignment-audit.md` — found the limit unnecessary).
  `toUpperCase`/`toLowerCase` follow the Unicode Default Case
  Conversion, matching JS including the special-casing table
  (`ß`→`SS`, `ﬄ`→`FFL`, final sigma, `ᾀ`→`ἈΙ`): Rust's standard library
  already implements it, which is what Boa's non-locale path uses; ICU
  is needed only for the locale-sensitive `toLocale*` variants, which
  stay rejected. `trim`/`trimStart`/`trimEnd` use ECMA's WhiteSpace +
  LineTerminator set, which is an explicit ~15-codepoint predicate, not
  a table — Rust's own `trim` is **not** equivalent (it uses
  `\p{White_Space}`, which adds U+0085 and omits U+FEFF), so the
  predicate is written out, as Boa writes it out.
  Byte-measured `length`/`slice` are unaffected and still diverge from
  JS's UTF-16 units on non-ASCII input — that is Q5's representation
  choice, not a limit that was lifted here.
  Range/argument
  errors trap (`charCodeAt` OOB, `repeat(-1)`, `split("")`,
  `replaceAll("", …)`, empty-`pad` padding — JS returns NaN or silent
  no-ops there). `replace`/`replaceAll` are literal: `$` substitution
  patterns are not interpreted. Rejected members: `stdlib.md` §8.
- **Q22 (`Array` methods)** — the checker accepts the `stdlib.md` §9
  subset on `T[]`. Element equality follows JS `===` per element
  kind (scalars by value, strings by content, `Date` by millis,
  reference classes by identity) for `indexOf`/`lastIndexOf` — which is
  what JS uses there too.
  **`includes` uses SameValueZero** (revised 2026-07-25), so `NaN` is
  found, as in JS. The earlier rule put all three searches on `===` for
  a single equality story; the cost was that `[NaN].includes(NaN)`
  answered `false` where every JS engine answers `true`. That bought
  internal tidiness no program can observe, and paid for it with a
  silently wrong answer. Adopting SameValueZero imports JS's own
  inconsistency — `indexOf` finds no `NaN`, `includes` does, same array
  — and that is the entire cost of the change. SameValueZero differs
  from `===` in exactly one case, `NaN`; `+0`/`-0` compare equal under
  both. Callback arities are fixed (no
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
  `f32`/`f64` by **SameValueZero**, `string` by content, `Date` by
  millis, reference classes by identity); `f16` (Q23 storage-only), `T[]`,
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
  language. **Revised by Q30 (2026-07-27):** `for…of` over a `Map` or
  `Set`, and `keys()`/`values()` as its direct subject, are accepted
  and **fuse into the same traversal `forEach` uses** — so they inherit
  this section's iteration order and its mutation rule rather than
  defining their own. `entries()` and construction from an iterable
  stay rejected, both for the want of a **tuple type**, not for an
  iterator reason.
  - **Float keys use SameValueZero** (revised 2026-07-25), as JS does
    for `Map`/`Set`: a `NaN` key is retrievable. The earlier rule used
    `===`, under which a `NaN` key could be **inserted and then never
    read back** — data loss the language itself manufactured — and it
    forced a compile-time rejection of a literal `NaN` key for the sole
    reason that the entry would be unreachable. **That rejection is
    withdrawn**; its stated reason no longer exists. All `NaN` payloads
    are one key.
  - **A `-0` key normalizes to `+0` on insert**, as ECMA specifies, so
    `forEach` reports `0`. Under the old `===` rule the two compared
    and hashed equal but `-0` was *stored*, which Q14 then spelled
    `-0` where JS prints `0` — a divergence in key traversal that the
    equality rule alone did not remove.

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
  stored value).
  **Divergence — `parseInt` is *more* precise than node at radixes
  outside {2,4,8,10,16,32}** (recorded 2026-07-25): over 80 413 cases
  the two differ 8 times, all at radix 3/35/36, and in every one this
  language's result is the correctly-rounded double while node's is 1
  ulp off. ECMA-262 §19.2.5 explicitly permits an implementation
  approximation at exactly those radixes and requires exactness at the
  others, where there are zero divergences. Recorded because
  `stdlib.md` §11.7 requires divergences be recorded rather than
  absorbed — not because anything needs changing.
  `toFixed(digits)` is fixed-decimal and therefore the
  only numeric string that is not Q14's shortest round-trip; its
  half-way, `±0`, `>= 1e21`, `NaN` and infinity cases are pinned by
  golden, and it is one runtime implementation on both tiers rather
  than the host libc, whose rounding is platform-dependent.
  **`(-0).toFixed(d)` follows ECMA and drops the sign** (`0.00`): the
  sign is taken only when `x < 0`, which is false for `-0`. A value that
  merely *rounds* to zero keeps it (`(-0.0001).toFixed(2)` is `-0.00`),
  as in every JS engine. This is deliberately unlike Q14's interpolation
  rule, which spells `-0` as `-0`: `${x}` is the language's only
  general-purpose number-to-string path, so losing the sign there would
  discard information a program has no other way to see, whereas
  `toFixed` is a specific formatting request with ECMA-defined
  semantics and `${}` remains available when the sign matters.
  *(An earlier revision of this entry claimed the opposite — that the
  sign is kept — and was wrong: it was written from an assumption about
  a corpus line rather than from the source. The 2026-07-25 Phase Review
  caught the contradiction between the spec and the implementation,
  goldens and unit tests, all of which have always been ECMA's.)*

- **Q26 (radix and precision formatting; `Math.clz32`)** — accepted per
  `stdlib.md` §11.5 and §1: `toString(radix)`, `toExponential`,
  `toPrecision` on `f32`/`f64`, and `Math.clz32`.

  All four were previously rejected, and **the recorded reasons were
  wrong in the same way**: each named a policy ("not in v1", "JS-number
  op") where the real content was cost. Owner rule, 2026-07-25: *a JS
  API that exists and is implementable at realistic cost is
  implemented, regardless of expected demand.* Measured cost for the
  three `Number` methods is about 440 lines with no external
  dependency; `clz32` is one line.

  `toString(radix)` also closed a genuine asymmetry: `parseInt(s, 16)`
  could **read** hexadecimal and nothing could **write** it, the Q14
  template form being base 10 only.

  **The radix and `toPrecision`'s digit count are required arguments**,
  unlike JS, where both have a no-argument form that means something
  else — the arity-changes-meaning hazard Q22 rejected for
  `reduce`/`sort` and Q25 for `parseInt`.

  Two implementation traps are normative, both being cases where the C
  tier's obvious lowering is wrong:
  - **`Math.clz32(0)` is `32`** (verified against node v24.18.0), but
    C's `__builtin_clz(0)` is undefined. This is the live-UB class P14
    hit with over-width shifts. The runtime uses Rust's
    `leading_zeros()`, which is defined at zero, behind an opaque
    symbol; the C tier never emits the builtin.
  - **ECMA does not pad the exponent**: node gives
    `(0).toExponential(2)` as `0.00e+0` where C's `%e` gives
    `0.00e+00`. A second reason for §11.4's standing rule that these
    never reach libc.

  What this rule does **not** reach, and why the boundary is not
  arbitrary:
  - `Number(x)`, global `isNaN`/`isFinite` (Q25) — they **coerce**.
    Adding them adds the unsoundness the language exists to reject, so
    the cost is not the objection.
  - *(Superseded 2026-07-25.)* This bullet listed `Math.imul`/`fround`
    as still rejected, on the ground that the rule was "about APIs
    whose absence costs a capability, and these cost none". The owner
    rejected that reading the same day: **a second spelling of an
    existing operation is not grounds for rejection.** Both are
    accepted under Q27 and implemented. The class the bullet described
    is now empty, and the surviving reasons are the two below.
  - `toLocaleString`, `Date` local-time accessors — locale and timezone
    data the project does not have. `js-alignment-audit.md` records
    that Boa needs the same data, so these are a missing prerequisite,
    not a cost question.

- **Q27 (the rejection sweep — thirteen groups reinstated)** —
  **contract written 2026-07-25; fully implemented 2026-07-26**
  (`stdlib.md` §12, six stages — the sixth added after the Phase
  Review found the `FixedArray` group contracted but unstaged). `generated-docs/api-reference.md` reports the
  checker rather than the contract (`compiler.md` §17.1), so it is the
  present tense wherever the two differ.
  Accepted per `stdlib.md` §1, §8, §9, §10 and §11. The 2026-07-25 sweep
  (`specs/tracking/js-api-sweep.md`) applied the owner's Q26 rule to
  every rejection in the contract. These failed no surviving reason:

  - `Math.imul(a: i32, b: i32): i32`, `Math.fround(x: f64): f64`.
    Duplicate spellings of `a * b` on `i32` and `x as f32`; **being a
    second spelling is not grounds for rejection** (owner, 2026-07-25),
    which is the clarification that reinstated them.
  - `String`: `substring`, `substr`, `charAt`, `concat`,
    `codePointAt`, the position argument of `startsWith`/`endsWith`,
    and `$` substitution in `replace`/`replaceAll`.
  - `Array`: `reduceRight` (with a required `init`), `splice`, `shift`,
    `unshift`, `copyWithin`, the **index parameter on callbacks**, and
    the `every` family on `FixedArray`.
  - `Map`/`Set`: `Map.groupBy` and the ES2024 set algebra.
  - `Number.parseInt`/`parseFloat`.

  **`substring` and `substr` are not duplicates of `slice`.** Measured
  on node v24.18.0: `"hello".substring(-2, 3)` is `"hel"` — negative
  arguments clamp to `0` and a reversed pair is swapped — where
  `slice(-2, 3)` is `""`. They add behaviour, so the rule was not even
  needed for them.

  Three narrowings, each because the wider form hits a reason that
  **does** survive the rule:

  - **The `array` parameter on callbacks stays rejected** while the
    index parameter is accepted. `f(v, i)` is a value and an integer;
    `f(v, i, arr)` hands the callback a reference to the very container
    being iterated. That is the defect class the P15 review found in
    aggregate `Map.forEach` (a raw pointer into live entry storage) and
    it contradicts C5, under which callbacks are non-escaping *by
    construction*.
  - **`splice` is delete-only and `unshift` takes one element.** JS
    makes both variadic (`splice(1, 2, 9, 9, 9)`, `unshift(a, b, c)`)
    and the language has no variadic parameters — the same missing
    prerequisite that keeps `Math.max` at two arguments. The accepted
    forms are `splice(start, deleteCount): T[]` and
    `unshift(x: T): i32`, the latter matching `push`, which is already
    single-element. This is a **recorded subset, not parity**.
  - **`Map.groupBy` only.** `Object.groupBy` returns a null-prototype
    object, which is not a type this language has.

  **`shift` traps when empty**, which is not a new rule: `pop` already
  traps there (Q4/Q15), so the miss-value objection that keeps `find`
  and `at` out does not reach `shift`.

  `Set` algebra takes a `Set<K>`, not JS's "set-like" duck type, which
  would need a protocol the language does not have. Result order is
  **normative**, as Q24 requires of all `Map`/`Set` traversal.
  `union`, `difference` and `symmetricDifference` are receiver order
  first, then the argument's contribution; **`intersection` iterates
  the smaller set, ties going to the receiver** — so its output order
  depends on the operands' relative sizes, which is deterministic but
  unlike the other three. *(Corrected 2026-07-26: this entry claimed
  all four were receiver-first, generalized from `{1,2,3}` against
  `{3,4}`, a case where both rules give the same answer. Measured:
  `{5,4,3,2,1}.intersection({1,3})` is `1,3`, not `3,1`.)* Full detail
  and the discriminating cases are in `stdlib.md` §10.4.

  **`$` substitution closes a recorded Q21 divergence** rather than
  opening one. `$$`, `$&`, `` $` `` and `$'` are substituted; `$1`–`$9`
  are **not**, which is ECMA's own behaviour for a string pattern (a
  string has no capture groups) and needs no regex engine — verified:
  `"a-b".replace("-", "[$1]")` is `"a[$1]b"` on node.

  **UTF-8 indexing follows Q5, not JS's UTF-16 units.** `charAt(i)` and
  `codePointAt(i)` take a **byte** offset and read the code point
  starting there, trapping off a code-point boundary exactly as `slice`
  does. `codePointAt` out of range traps where JS returns `undefined`,
  as `charCodeAt` already does; `charAt` out of range is `""`, which is
  JS's own answer and needs no miss value. The UTF-16-versus-byte
  difference is Q5's standing divergence, not a new one.

- **Q28 (`JSON`)** — accepted per `stdlib.md` §13.
  `JSON.stringify<T>(value: T): string` and
  `JSON.parse<T>(text: string): JsonResult<T>`, both monomorphized at
  the call site.

  **No RTTI.** The roadmap had listed layout descriptors as P13's new
  machinery. The language has no inheritance (`extends` is S006 on a
  value class, S100 on a reference class), no `any`, and no
  heterogeneous container (C7 admits `Ref | null` only), so every
  value's static type is its dynamic type and the checked type is
  enough. P13 adds no mechanism the language did not already have.

  **`NaN` and `±Infinity` trap** where JS writes `null`. JS's answer
  loses information silently — `0` comes back where a `NaN` went in —
  and this is the third application of one rule, after Q20 refused
  Invalid-Date and Q24 refused a zeroed `get` miss. **`-0` serializes
  as `0`**, as JS does; this does not contradict Q14's `-0` spelling,
  for the reason Q25 gave about `toFixed`: Q14 governs `${…}`, the only
  general-purpose number-to-string path, where the sign is information
  the program cannot otherwise see, while JSON is a specific
  interchange format with an ECMA-defined answer.

  **`parse` reports failure as data, not as a trap.** `JsonResult<T>`
  is an ambient generic reference class — the machinery Q24 built for
  `Map`/`Set` — carrying `ok` and `value`; the caller releases it with
  `Context.free` (Q6). Trapping was rejected because it contradicts the
  reasoning Q25 committed to: a parse failure is **data**, which is why
  `parseInt` may return `NaN` where Q20 and Q24 could not have a
  sentinel. JSON reaching a script has usually crossed the host
  boundary. The cost is one allocation per parse and a release
  obligation, stated rather than hidden. `ok` is `false` both for
  malformed text and for well-formed text that does not match `T`.

  **Reading `JsonResult.value` when `ok` is `false` traps**
  (`json-result-value`, both tiers). The contract first said only that
  the field "must not be read"; the P13 review measured that a failed
  scalar parse was byte-identical to a successful parse of `0`, and
  that a reference-class read segfaulted — the zeroed-miss pattern this
  register refuses elsewhere, mitigated by a sentence. The trap fires
  on a programmer error, not on data, so failure-as-data stands.
  **Input nesting is capped at 128**; deeper input is an ordinary
  `ok = false`, after the review found unbounded recursion aborting the
  host process on a 20 000-deep document.

  **Integer `parse` targets are converted from the number's text, not
  through `f64`** — an `i64` target given `9007199254740993` yields
  that value, not `…92`. The stage-2 implementation initially routed
  every number through `f64` before consulting the target, returning a
  different integer with `ok = true`; that is the silently-wrong class
  this register refuses elsewhere. `f32`/`f64` targets keep the `f64`
  path, where the inexactness belongs to the type.

  **`Date` is rejected as a `parse` target** (S014) while staying a
  `stringify` output: it serializes to an untagged ISO string that no
  parser can distinguish from a `string` field of the same text, so
  the target is unreachable by construction — the shape this register
  refused for a literal `NaN` `Map` key.

  **`Map`/`Set` are rejected as `stringify` input**, not serialized:
  JS gives `{}` for both, a silently empty result for a container the
  program filled, and any other shape would be a divergence invented
  here.

  Field order is **declaration order**. JS's rule that integer-like
  keys sort numerically first cannot arise — field names are
  identifiers, the checker rejecting computed and non-identifier ones.

  **Cycles cost nothing where they are impossible.** Monomorphization
  lets the checker decide statically whether `T`'s field graph can
  reach a reference class from itself; only then does the emitted
  serializer carry a visited set, and it traps on a revisit where JS
  throws.

- **Q30 (`for…of`, container iteration, array-literal spread)** —
  accepted per `stdlib.md` §14. Owner decision 2026-07-27, after the
  `js-api-sweep.md` audit recorded the iterator protocol as wanted at
  high priority.

  **User-defined iterables are impossible here, and that is forced
  rather than chosen.** JS binds iteration through `Symbol.iterator`;
  `Symbol` is a permanent stdlib non-goal. Any substitute spelling — an
  `iterator()` method, a decorator — leaves the class **not iterable
  under stock `tsc`**, so `for (const x of mine)` would not type-check
  and invariant 5 would be broken. `for…of` is therefore over **built-in
  containers only**: `T[]`, `FixedArray<T, N>`, `Map`, `Set`, `string`,
  and a `Generator<T>` from a `function*`.

  This is the rare case where the TS-subset invariant *removes* a design
  question instead of constraining one.

  **`keys()`/`values()` are accepted only as the direct subject of a
  `for…of`**, where they fuse into the loop. Elsewhere — assigned to a
  variable, passed, returned — they are S014. **`entries()` is rejected
  everywhere**, `for…of` included. *(Corrected 2026-07-27: this entry
  first listed `entries()` with the other two, contradicting
  `stdlib.md` §14.1 in the same commit. `entries()` yields a pair and
  the language has no tuple type — a type-system gap, not an iterator
  decision, and the same one that keeps `new Map([[k, v]])` out.)*

  The reason is the memory model, not taste. C5 makes callbacks
  non-escaping *by construction*; an iterator held as a value is
  stateful and outlives the call that made it, which would be the first
  escaping temporary in the language. Fusing removes the object
  entirely: **`for…of` lowers to an index loop over the container's
  storage, allocating nothing**, at the same cost as the `forEach` that
  Q24 made the traversal. `Generator<T>` remains the one iterator that
  *is* a value, because C8 already contracted it and it is
  frame-allocated by the coroutine machinery rather than by iteration.

  Iteration order is **Q24's insertion order** for `Map`/`Set`, which
  `for…of` inherits rather than re-decides.

  **Spread is accepted in an array literal only** — `[...xs]`,
  `[0, ...xs, 9]` — where the element count is a runtime value the
  literal can grow into. **`f(...xs)` is rejected**: it needs variadic
  parameters, which the language does not have (the same missing
  prerequisite that keeps `Math.max` at two arguments, Q19).

  **Construction from an iterable stays rejected**, and not for an
  iterator reason: `new Map([[k, v]])` needs a **tuple type**, and this
  language has none. That is a type-system gap independent of Q30 and
  is recorded as such rather than folded in.

  **Mutation during iteration** follows the rule the runtime already
  applies to `forEach` (`stdlib.md` §10.7): appends after entry do not
  extend the visit, removals shorten it. `for…of` fuses into the same
  traversal, so it inherits that rule by construction rather than
  needing its own.

- **Q31 (regular expressions)** —
  accepted per `stdlib.md` §15. Owner decision 2026-07-27. `RegExp` had
  been a **permanent stdlib non-goal**; the reversal follows the shape
  §7 used for `Map`/`Set` — evidence first.

  *(This entry was referenced throughout §15 from the day the contract
  was written and **not actually added to this register until
  2026-07-27**, when the implementer noticed the register ended at Q30.
  Recorded rather than quietly inserted, because a Q-id cited by a
  contract and absent from the register is the drift §17 exists to
  catch, appearing here in prose instead of in a generated table.)*

  **`regress` matches UTF-8 and returns byte offsets**, so the index
  domain is Q5's with no conversion. Its `utf16` feature must stay off:
  it is documented as additive but removes the byte-prefix search on
  the `&str` path, measured 1.4–69× slower.

  **A match position is a byte offset**, agreeing with `indexOf`,
  `slice` and `charAt` and diverging from JS, which counts UTF-16
  units. That divergence is Q5's, already recorded — the alternative
  would have made regex agree with JS and disagree with every other
  string API in this language.

  **Budget exhaustion traps.** `regress` bounds nothing; a pattern with
  a trailing mismatch goes 4.0 ms at 17 bytes to 650 ms at 25. The fork
  adds a budget whose exhaustion is a distinct `Err`, never a miss —
  `test` returning `false` would be Q24's zeroed-`get` objection and
  `search` returning `-1` would be Q20's Invalid-Date objection. The
  budget is a Context field, so it is part of the deterministic state
  §0.3 governs, like the seeded PRNG and the pinned clock.

  The bound converts a pathological pattern from **exponential to
  linear**, not to constant: the prefix scan and a long backreference
  still run inside one charged unit.

  **Unconditional — there is no build switch.** One was contracted and
  then removed the same day: it was argued from binary size, and the
  measurement showed the linker already charges that cost per
  *program*, 80 bytes for one that never calls regex. A switch would
  have made what the compiler accepts depend on a build flag, which
  this project has nowhere else, at the price of a second corpus
  meaning and a doubled gate.

  **`exec`, `match`, `matchAll`, `lastIndex` and `groups` are rejected
  for language gaps, not engine ones**, and `match` is the sharpest:
  it **fails stock `tsc` under `strict`**, because
  `RegExpMatchArray.index` is `index?: number`. Invariant 5 excludes
  it; no design choice was involved.

- **Q32 (string-literal unions)** — *(Owner, 2026-07-31; requested by
  the downstream WebGPU binding project, whose JS-shaped API needs
  `type GPUIndexFormat = "uint16" | "uint32";`.)* A **type alias of a
  closed union of string literals** is a language type:

  - **Alias-only and nominal-by-alias.** The alias is the type's
    identity: two aliases with identical members are distinct types
    (`tsc` sees them structurally — the accepted-superset rule,
    invariant 5; the language narrows, as with C1 nominality). Inline
    literal unions in any other position remain rejected general
    unions (C7).
  - **Values** are member literals in a context typed by the alias
    (variables, parameters, fields, returns, array elements).
    A non-member literal is a compile error.
  - **Operations**: assignment and `===`/`!==` against member
    literals and same-alias values; template-literal interpolation
    prints the member string. Comparison or assignment with plain
    `string` (or another alias) is rejected.
  - **Representation**: an `i32` discriminant (declaration order);
    a per-alias static string table used **only** for formatting —
    comparisons lower to integer compares, never string compares.
  - **Boundary**: Q32 aliases may not appear in mirrors or boundary
    signatures (v1); a binding layer lowers them to integer enums
    before its C facade. *Revised 2026-08-08 (R23): an alias declared
    through the prelude `CEnum` generic carries a per-member integer
    wire value and is legal in boundary signatures at parameter and
    return positions (`compiler.md` §50). The checker owns the
    conversion at the crossing; an unknown wire value traps there.
    A plain alias stays barred from boundary signatures.*
    *Revised 2026-08-09 (§52): a wire-mapped alias is also legal as
    a boundary-struct member, as a boundary array-pair element, and
    as a mirror-class constructor parameter; its discriminant is the
    wire value itself (`compiler.md` §52). A read of an alias member
    from a boundary struct validates membership and traps on an
    unknown value.*

  Contract and exit criteria: `compiler.md` §24. Accept: `a91`.
  Reject: `r87` (non-member literal), `r88` (inline literal union;
  S011 as today), `r89` (cross-alias assignment, same members —
  `tsc`-clean, proving the language is strictly narrower here).

- **Q33 (literal-constructible descriptor classes)** — *(Owner,
  2026-07-31; requested by the downstream WebGPU binding project,
  whose JS-shaped API constructs every descriptor as a dictionary
  literal: `device.createBuffer({ size: 256, usage: ... })`.)*

  - **Declaration.** `@Descriptor class` (ambient decorator beside
    `@CStruct` in the prelude) declares a **data-only reference
    class**: no constructor, no methods, no `extends`. Members are
    either **required** — spelled `name!: T`, the definite-assignment
    form stock `tsc` mandates under `strict` for initializer-less
    members (measured 2026-07-31; the `!` is imposed, not chosen) —
    or **defaulted** — `name?: T = expr` per the C7/Q33 exception.
    Member types are those a reference-class field may hold, plus
    nested `@Descriptor` classes, arrays of them, and Q32 aliases.
  - **Construction.** An object literal in a context whose expected
    type is a `@Descriptor` class constructs that class: required
    members must appear, omitted defaulted members take their
    declared default at construction, excess members are rejected
    (C1's closed-property discipline), explicit `undefined` stays
    rejected (C7). Nesting and arrays construct recursively;
    `{}` is legal when every member has a default. Contexts:
    arguments, annotated initializers, fields, array elements,
    nested members.
  - **Runtime.** Sugar only: a literal lowers to a normal
    reference-class allocation plus member stores — Context
    lifetime, `Context.free`/`collect` as usual, no new runtime
    type, no `undefined` representable.
  - **Nominality.** Unchanged (C1): constructed *values* are nominal
    — a `BufferDescriptor` value does not pass where a same-shaped
    other class is expected; the literal itself has no standalone
    type and constructs whatever its context demands. Literals
    against unmarked classes stay rejected.
  - **Boundary.** `@Descriptor` classes are in-language types, not
    mirror/boundary types (v1).

  - **`new` is rejected on descriptor classes** *(added at landing,
    2026-07-31 — the implementer's resolution, adopted: literal
    construction is the only construction, so required members can
    never be left uninitialized).*

  Contract and exit criteria: `compiler.md` §25. Accept: `a92`.
  Reject: `r90`–`r95`.

- **Q34 (async/await — poll-driven, schedulerless)** — *(Owner,
  2026-07-31; downstream request R4. Boa v0.21.1 was source-read as
  the design reference for the no-scheduler architecture — see the
  HANDOFF appendix record in `specs/tracking/q34-async.md`.)*

  - **Awaitables are exactly two forms**, and neither is a value:
    `Context.suspend()` — the primitive suspension point, ambient in
    the prelude as `suspend(): Promise<void>`, resumed at the next
    explicit step — and a **direct call of an `async function` in
    await position**. *(Revised 2026-08-27, §70: the result of an
    async call is a handle. A handle can be held, stored, passed,
    and awaited later; a handle dropped without an await is
    rejected — stock `tsc` allows the floating promise, so that
    reject entry is a strictly-narrower pin.)* A handle is never
    combined: `new Promise`, `.then`/`.catch`/`.finally` calls,
    and `Promise.all/race/resolve/reject` are rejected. The lib
    `Promise<T>` type is the `tsc` view only (C8 precedent).
    *(Revised 2026-08-02, R13: a direct async **method** call in
    await position — `await recv.m(...)` on a plain, non-generic
    reference class — is the third form; `compiler.md` §37. Still
    not a value; the same immediate-await rule applies. R36,
    2026-08-23: the class can be generic, and the named function
    can be a generic instance; `compiler.md` §64.)*
  - **Structure.** Each root invocation of an async export forms a
    single linear chain of Context-owned frames: `await f(...)` runs
    the callee until it suspends, and suspension propagates to the
    root. Stepping resumes the innermost suspended frame.
    Concurrency is **multiple pending root invocations** (the
    downstream GPU norm); one frame awaits one value at a time, and
    combinators are impossible by construction, not merely
    out of scope.
  - **Driving.** Nothing schedules. The host (or the CLI runner, or
    the generated AOT entry) steps pending roots explicitly:
    `subscript_rt_ctx_async_pending` / `subscript_rt_ctx_async_step`
    (`compiler.md` §26.3), stepping each pending root once per call
    in kick order — deterministic, gate-comparable. Script
    evaluation never pumps (the Boa separation, kept).
  - **Lifetime and teardown (R4.4).** Suspended frames are
    Context-owned like coroutine frames; releasing the Context drops
    them without running continuations, and **no cleanup construct
    is guaranteed to run** — decided now, revisited only with
    evidence. Hot reload treats a suspended async frame as §8.2
    treats a suspended coroutine: stale, trapping on resume.
  - **Failure (R4.5, C6 stands).** `await` delivers the completed
    value as-is; fallibility lives in the value domain (`T | null`
    or result records — the API layer's policy). A trap during a
    step trap-stops the entry per C6; a trapped Context refuses
    further stepping until the host clears or releases it.
  - **Exports (Q12 kept).** `export async function f(): Promise<void>`
    lowers to the same zero-argument void C symbol; invocation runs
    to the first suspension, completion is observable as the pending
    count reaching zero.

  Contract: `compiler.md` §26. Accept: `a93` (nested chain), `a94`
  (two interleaved roots), `a95` (foreign-poll await — absorbs the
  earlier Q1 corpus request). Reject: `r96` (`new Promise`), `r97`
  (`.then` call), `r98` (`Promise.all`), `r99` (`await` outside
  `async`), `r100` (a handle dropped without an await; `tsc`-clean;
  rewritten by §70 from the floating-call form). §70 adds accept
  `a154`–`a155` (a held handle, a handle array) and reject `r157`.

- **Q35 (Workers — standard-library threads)** — *(Owner,
  2026-08-02. Two rulings recorded the same day: the "stdlib grows
  in computation only" line is a revisable convention, not a
  principle; and Workers land as standard library, not as a host
  pattern. CLAUDE.md's platform-capability list drops "threads"
  with a dated note.)*

  - **Model.** A worker is a runtime-owned OS thread running a
    dedicated Context of the same program image; module state is
    per-Context in both tiers (`compiler.md` §38, landed first as
    the isolation prerequisite). Messages are byte copies of
    transferable reference-class payloads, materialized as fresh
    receiver-Context instances; nothing is shared, nothing blocks
    on the spawning side, and the blocking receive exists only on
    the worker's own thread. Surface: `stdlib.md` §16; runtime
    layer: `compiler.md` §39; checker/lowering: §40.
  - **Not carried from TS Workers**, each a consequence of a
    standing invariant, not a gap: `new Worker(url)` dynamic
    loading (AOT — the entry is a named module function),
    `onmessage` push delivery (§14.6 — each worker pumps its own
    Context; receive is `wait`/`poll`), `SharedArrayBuffer`-style
    shared mutable state (copy-only messaging is the contracted
    surface; the C ABI physically permits sharing and
    synchronization is then the host's responsibility — scripts
    are trusted), and structured clone of arbitrary graphs
    (transferable fields only, stdlib §16.2).
  - **Failure.** A worker trap surfaces at the parent's `join` as
    trap kind 22 — loud at the join point, never silent (C6
    precedent).

  Contract: `compiler.md` §38–§40, `stdlib.md` §16. Accept:
  `a112`–`a113`, example `e11`. Reject: `r106`–`r110`.

## 3. Open items carried forward

- Value-class fields of reference/string/nullable types (C2): undecided
  until a corpus program needs them; the field-type whitelist stands.
- Generic constraints/variance beyond monomorphized `a12` shapes: revisit
  with corpus evidence.

## 4. Prelude and gate

- `prelude/lang.d.ts` — ambient declarations for §1/§2: sized-numeric
  aliases, `print`, `Context.collect`, `Context.free`, `CStruct` decorator
  (typed against TS 5 standard `ClassDecoratorContext`), `FixedArray`.
- `tsconfig.json` (repo root) — `strict`, `noEmit`, ES2022 target/lib,
  `types: []`; includes `prelude/**/*.d.ts` and `corpus/accept/**/*.ts`
  only. The reject corpus is excluded (corpus.md §2).
- Gate: `tsc -p tsconfig.json` — zero errors, standing.
