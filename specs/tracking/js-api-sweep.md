# JS API sweep — what is implemented, what is deferred, and why

Companion to `js-alignment-audit.md`. The audit asked whether each
*divergence* had a solution we had not looked for; this file asks the
same of each *rejection*, and records the standing answer so a later
session does not re-derive it.

## The rule (owner, 2026-07-25)

**A JS API that exists and is implementable at realistic cost is
implemented, regardless of expected demand.** Two clarifications the
owner gave when the rule was applied:

- **Low demand is not a reason to reject.** "Not in v1" is a scope
  statement, not a cost. Where cost was the real content, say the cost.
- **Being a second spelling of an existing operation is not a reason to
  reject.** An API that duplicates `slice` or `a * b` is still
  implemented if JS has it.

Only two reasons survive the rule, and they are recorded per API below:
implementing it would **introduce a defect**, or the project **lacks a
prerequisite** it cannot cheaply acquire.

## Accepted — implemented

| API | contract | status |
|---|---|---|
| `Number.prototype.toString(radix)` | Q26, `stdlib.md` §11.5 | `ddd4852`, corpus `a62` |
| `Number.prototype.toExponential` | Q26, §11.5 | `ddd4852`, `a62` |
| `Number.prototype.toPrecision` | Q26, §11.5 | `ddd4852`, `a62` |
| `Math.clz32` | Q26, `stdlib.md` §1 | `ddd4852`, `a62` |

Both normative traps held. Radix 10 **delegates to the existing Q14
formatter** rather than reimplementing it, so the two agree by
construction and `a62` prints them side by side. `clz32` goes through
`subscript_rt_math_clz32` over `u32::leading_zeros`, and a `cemit` test
asserts the emitted C does **not** contain `__builtin_clz`, which is
undefined at zero where ECMA defines `clz32(0)` as `32`. Exponents are
unpadded (`(0).toExponential(2)` is `0.00e+0`, not C's `0.00e+00`).
`a62`'s golden was regenerated from node v24.18.0 and `cmp`'d
independently of the implementer. `r48`/`r49` were repurposed from the
old contract's `toPrecision`/`toString(16)` rejections to the
required-argument S014s.

## Accepted by the rule — contracted as Q27, fully implemented

The sweep of 2026-07-25 found these fail no surviving reason: each
exists in JS, introduces no defect, and needs no prerequisite the
project lacks. Contracted as **Q27** (`cdae592`) and **fully implemented
2026-07-26** across six stages (`stdlib.md` §12), corpus `a63`–`a68`.
The sixth stage — the `every` family on `FixedArray` — was added after
the P18 Phase Review found that group contracted but never staged.

**Writing the contract corrected the table in three places, each found
by measuring rather than reasoning** — the entries below are the
corrected form:

- **`shift` returns `undefined` on an empty array in JS**, which looked
  like the miss-value problem that keeps `find` and `at` out. It is
  not: `pop` already **traps** when empty (Q4/Q15), so the same rule
  covers `shift` and no sentinel is needed.
- **`splice` and `unshift` are variadic in JS** (`splice(1, 2, 9, 9,
  9)`, `unshift(a, b, c)`). The language has no variadic parameters —
  the same missing prerequisite that keeps `Math.max` at two
  arguments — so the accepted forms are delete-only `splice` and
  single-element `unshift`. **A recorded subset, not parity**, and the
  contract requires a reject entry naming the reason so a reader can
  tell the two apart.
- **The callback `array` parameter is not in the same class as the
  index parameter.** `f(v, i)` passes a value and an integer; `f(v, i,
  arr)` passes a reference to the container being iterated, which is
  the defect the P15 review found in aggregate `Map.forEach` and
  contradicts C5. Index accepted, array rejected.

| area | API | note |
|---|---|---|
| `Math` | `imul`, `fround` | duplicate spellings of `a * b` on `i32` and `x as f32`; the rule reaches them regardless |
| `String` | `substring`, `substr` | **not** duplicates of `slice`: measured on node v24.18.0, `"hello".substring(-2,3)` is `"hel"` (negatives clamp to 0, arguments swap when reversed) where `slice(-2,3)` is `""` |
| `String` | `charAt` | total — out of range is `""`, not `undefined`, so no miss value is needed |
| `String` | `concat` | duplicate of `+` |
| `String` | `codePointAt` | out of range should trap, as `charCodeAt` already does |
| `String` | the position argument of `startsWith`/`endsWith` | was rejected as "optional arguments not accepted"; implemented. Measured 2026-09-10: `"abc".startsWith("b", 1)` and `"abc".endsWith("b", 2)` are both `true`, as on node |
| `String` | `$$`/`$&` substitution in `replace`/`replaceAll` | needs no regex engine; implemented under Q27, and §15.3 extends it to `$1`–`$99` and `$<name>` for a regex pattern. *(Q21 recorded it as a divergence until 2026-09-09; the runtime always substituted.)* |
| `Array` | `reduceRight` with a required `init` | passes the same arity rule that `reduce` passes |
| `Array` | `splice` (delete-only), `shift` (traps when empty, as `pop` does), `unshift` (one element, as `push`), `copyWithin` | JS makes `splice`/`unshift` variadic; the language has no variadic parameters, so these are a recorded subset |
| `Array` | **the index parameter on callbacks** | `map((v, i) => …)`; the largest item here by real-world use, and the one that touches the checker's arity machinery. The third `array` parameter stays **rejected** — see below |
| `Array` | the `every` family on `FixedArray` | was a "v1 is `T[]` only" scope rejection. Implemented as §12 stage 6; `map` returns `U[]` and `filter` returns `T[]`, since a fixed-length receiver cannot give a fixed-length result when the element type or the length changes |
| `Map`/`Set` | `groupBy`, ES2024 set algebra (`union`, `intersection`, `difference`, `symmetricDifference`, `isSubsetOf`, `isSupersetOf`, `isDisjointFrom`) | |
| `Number` | `Number.parseInt`, `Number.parseFloat` | verified `=== parseInt` / `=== parseFloat` on node — pure aliases |

## Rejected — implementing it introduces a defect

The strongest class. Cost is not the objection and the rule does not
reach these.

- **Coercion.** `Number(x)`, the global `isNaN`/`isFinite`, `String` as
  a value or constructor. Coercion is the unsoundness the language
  exists to reject; adding these imports it.
- **Arity that changes meaning.** `sort` with no comparator
  (string-coerces the elements), `reduce` with no `init`. Q22's rule,
  and the reason Q25 requires `parseInt`'s radix and Q26 requires
  `toString`'s.
- **No miss value.** `find`/`findLast`, `at` (both `String` and
  `Array`), and `Map.get` where `V` is scalar. JS returns `undefined`;
  this language has no `undefined`, and `T | null` does not cover
  scalars — `string | null` is itself rejected (S011: unions are
  limited to `Ref | null`, and `string` is not a reference shape).
  `findIndex`, `charAt` and `getOr` are the total spellings.
  **`at` was misclassified as a duplicate spelling during the sweep and
  corrected**: it is this class, not a redundancy.
- **Mutation of an immutable value.** `Date` setters. A `Date` erases
  to `i64` and is a value; a setter would break that.

## Rejected — a prerequisite the project does not have

Not a cost question. Each would be decided by acquiring the
prerequisite, which is a separate decision from this rule.

| missing | APIs |
|---|---|
| locale data | every `toLocale*`, `localeCompare` |
| timezone database | `Date` local-time accessors, `getTimezoneOffset`, `Date.parse`, the `toString` family, the multi-argument `Date` constructor, `Date` in a template literal |
| Unicode normalization tables | `normalize` (Boa needs `icu_normalizer` for the same reason) |
| **tagged template machinery** | `String.raw` |
| **variadic parameters** | `Math.max`/`min`/`hypot` beyond two arguments, `f(...xs)`, `new C(...xs)`, insertion through `splice`, multi-element `unshift` |

**Two rows left this table on 2026-09-10. Do not restore them.** The
sweep of 2026-07-25 also listed **a regular-expression engine**
(`match`, `matchAll`, `search`, regex `split`, regex `replace`) and
**an iterator protocol** (`keys`/`values`/`entries`, `for…of`,
spread, construction from an iterable). The project acquired both on
2026-07-27: P23 shipped the engine (`stdlib.md` §15, Q31) and P22
shipped `for…of` with array-literal spread (`stdlib.md` §14, Q30).

**The rows stayed here for six weeks after that.** The delay is the
defect this section records. Parts of both rows run today. Each part
that stays rejected carries a different reason, and the next section
names that reason per form.

The two bold rows that remain are **language features, not library
gaps**. Each one is a phase of its own and is outside this rule;
recorded here so that "why is `f(...xs)` rejected" has one answer.

## Regex and iteration — the status of every form

*(Measured 2026-09-10 at `be15ee3` on macOS 26.6.2 arm64, with the
release `subscript` binary, TypeScript 5.9.2 and node v24.18.0. This
section replaces the two retired prerequisite rows. The history
section below it states the position of 2026-07-25 and is not
current guidance.)*

Read this section before you quote a regex or an iteration rejection
as current. A supported subset is not parity with JavaScript: each
row states the context, the receiver, and the result shape that the
language accepts. "Probe" names a temporary program from the round
that produced this section; the corpus entries are the permanent
witnesses.

### Regex, per form

| form | behaviour | witness | remaining obstacle |
|---|---|---|---|
| `/pat/flags`, `new RegExp(p, f)` | supported | §15.3; `a82`, `a83` | none |
| `re.test`, `re.source`, `re.flags` | supported | §15.3; `a82`, `a83` | none |
| `re.matchStart(g)`, `re.matchEnd(g)` | supported; an ambient name, not a JS one | `prelude/lang.d.ts`; `a82` | none |
| `s.search(re)` | supported; the result is a **byte** offset | §15.3; `a82`; probe on both tiers | none. `"añb".search(/b/)` is `3` here and `2` on node, which is the Q5 divergence and not a gap |
| `s.split(re)` | supported, with capture reinjection | §15.3; `a82`; probe on both tiers, node agrees | none |
| `s.replace(re, r)`, `s.replaceAll(re, r)` | supported; `$$`, `$&`, `` $` ``, `$'`, `$1`–`$99`, `$<name>` | §15.3; `a82`; probe on both tiers, node agrees | none |
| `s.match(re)` | rejected, S014 | `r27`; probe | **no array-with-extra-fields result type.** Stock `tsc` accepts the bare call. Only `const i: i32 = m.index` fails, with TS2322, because `RegExpMatchArray.index` is `index?: number`. This blocks parity. A reduced form without `index` and `input` is undecided; line 85's `splice` subset is the precedent |
| `/x/.exec(s)` | rejected, S014 | `r80`; probe | the same missing result type. `RegExpExecArray` extends `Array<string>` with a required `index` and `input`. It is **not** a tuple, and `tsc` accepts the whole probe, the `index` read included |
| `s.matchAll(re)` | rejected, S014 | `r81`; probe | the same missing result type. `RegExpExecArray.index` is required, so the optional-index argument that reaches `match` does not reach `matchAll`. The fusion decision §15.3 names stays **open**: §14.3 fuses an index loop over a container's own storage, and `matchAll` has none |
| `re.lastIndex`, the sticky flag `y` | rejected, S014 | `r82`, `r84`; probe | `lastIndex` drives `exec`, and the `exec` result type is the missing one. The sticky flag steers matching through `lastIndex`. **Not** a value-type restriction: §15.5a makes a `RegExp` an ordinary Context allocation, and `matchStart`/`matchEnd` already expose the handle's match state |
| `m.groups` | rejected, S014 | `r83` | no object with dynamic string keys |

### Iteration, per form

| form | behaviour | witness | remaining obstacle |
|---|---|---|---|
| `for…of` over `T[]`, `FixedArray<T, N>`, `Map`, `Set`, `string`, `Generator<T>` | supported | §14.1; `a77`, `a79`, `a84`–`a87`, `a180`; probe on both tiers | none |
| `map.keys()`, `map.values()`, `set.values()`, array `keys()`/`values()` as the direct `for…of` subject | supported | §14.1; `a78` | none |
| a `Generator<T>` held as a value — passed, returned, stored in a class field, driven by `.next()` | **supported** | C8; probe on both tiers. **No corpus entry pins these forms**: `a20` binds a generator to a local and drives `.next()`; `a180` uses one as a `for…of` subject only | none. The missing accept entry is an open proposal |
| a user class iterated through a method that returns a `Generator<T>` | **supported**, and `tsc`-clean | probe on both tiers | none. The method name must not be `keys`, `values`, or `entries`; the user-receiver row below states why |
| array-literal spread over `T[]`, `FixedArray<T, N>`, `Map`, `Set`, `string` | supported | §14.4; `a81` | none |
| `for (const x of userClass)` | rejected | `r72`; probe | `Symbol.iterator` is the one binding that stock `tsc` accepts, and `Symbol` is a permanent stdlib non-goal (§7). The language also rejects a computed method name (S100). **Invariant 5 does not force this row**: `class Bag { *[Symbol.iterator]() {…} }` is `tsc`-clean. Invariant 5 forbids the substitutes, because an `iterator()` method or a decorator leaves the class not iterable under `tsc` |
| `keys()`/`values()` assigned, returned, or passed | rejected, S014 | `r42`, `r76`, `r77`; probe | **two concrete requirements.** (1) A language type for the view. `tsc` names it `IterableIterator<T>`, and that spelling is `tsc`-clean, but the language answers S016 for the name. (2) An owner for the escaping value. `compiler.md` §70 landed a reference-counted handle on 2026-08-27, and §70.1 decision 1 keeps it on coroutine frame handles and defers the general form until evidence arrives |
| `entries()` anywhere, on any receiver | rejected, S014 | `r75`, `r79`; probe | no tuple type |
| `keys()`, `values()`, `entries()` on a **user** receiver | **accepted** by `compiler.md` §103.2 (owner, 2026-09-10); rejected at `be15ee3` | probe | none. The rules now read the receiver type, and the three names are ordinary members on a user class |
| array-literal spread of a `Generator<T>` | rejected, S014 | probe | a generator is single-use, and §14.4 keeps that mutation out of a value expression |
| `f(...xs)`, `new C(...xs)` | rejected, S014 | `r78` pins `f(...xs)`; the `new` form has no entry, and a probe covers it | variadic parameters. `tsc` rejects both forms too, with TS2556 |
| `{...a}` | rejected, S100 | probe | object literals are not in the decided surface. This is not an iteration gap |
| `new Map(iterable)` | rejected, S014 | `r43`; probe | no tuple type |
| `new Set(source)` | **accepted** by `compiler.md` §103.1 (owner, 2026-09-10); rejected at `be15ee3` with `Map`'s tuple reason | probe | none, for `K[]`, `FixedArray`, `Set<K>`, and `string`. A `Map` source stays rejected under invariant 5: `tsc` answers TS2769. A `Generator` source stays rejected by §14.4 |
| `for (const [k, v] of map)` | rejected, S100 | probe | destructuring is not in the decided surface. This is not an `entries()` gap |
| `Array.from(xs)` | rejected, S016 | probe | the name `Array` binds to no declaration. This is not an iteration gap |

### The reasons that did not survive the measurement

Each line below states a reason this file, a contract, or a
diagnostic gave, and what replaced it.

- **"The language has no iterator protocol."** It has one for
  `Generator<T>`. That generator is a stateful value, and it outlives
  the call that made it: the probes return one from a function and
  store one in a class field, on both tiers. No corpus entry pins
  those forms yet.
- **"An iterator held as a value is the first escaping temporary in
  the language, and a memory-model change."** The mechanism exists.
  `compiler.md` §70 landed a reference-counted handle, and §70.1
  decision 1 scopes it to coroutine frame handles. Name that scope,
  and name the missing view type; do not name the memory model.
- **"`match` fails stock `tsc`."** The call type-checks. Only the
  read of the optional `index` fails, with TS2322. The same argument
  does not reach `matchAll` or `exec`, whose `index` is required.
- **"`new Map/Set(iterable)` needs a tuple type."** `Map` needs one.
  `Set` does not.

**One reason survived a challenge to it.** `stdlib.md` §15.3 says
`matchAll` needs a fusion decision under Q30/§14.3. That decision is
open. §14.3 fuses an index loop over a container's own storage, and
`matchAll` has none, so §14.3 decides nothing for it.

### Where the retired reasons stood, and where they went

Four normative lines held a reason this measurement retired. A
contract outranks a tracking file, so each one was corrected in place
by `compiler.md` §103.3 and §103.4 (owner, 2026-09-10).

| site | what it held | what replaced it |
|---|---|---|
| `stdlib.md` §14.3 | "the first escaping temporary in the language, and a memory-model change (invariant 2)" | a missing view type, and §70.1 decision 1's scope for the reference-counted handle |
| `stdlib.md` §15.3, the `match` bullet | "**fails stock `tsc` under `strict`** … Invariant 5 excludes it, not a design choice" | the call type-checks; invariant 5 narrows to the `index` read |
| `collisions.md` Q31 | the same `match` claim, in the register | the same |
| `stdlib.md` §8 | "`match`/`matchAll`/`search` (a regex engine) — each a missing prerequisite" | `search` shipped in P23; `match` and `matchAll` cite the result type |

`stdlib.md` §15.3 also called `lastIndex` "mutable state on a
**value**" while §15.5a called a `RegExp` handle "an ordinary Context
allocation". §103.4 settled it: a `RegExp` is a handle, and
`lastIndex` is rejected because it drives `exec`.

## History — the "Wanted" investigation of 2026-07-25 (superseded)

**This section states the position of 2026-07-25 and 2026-07-27. The
section above holds the current status.** The investigation is kept
because it records why the project chose `regress` and why the fork
adds a budget, which are current tradeoffs.

**Owner, 2026-07-25: both were wanted, at high priority, to be
designed later.** Recorded with what the sweep already found, so the
later design did not start from zero.

**Iterator protocol — most of the machinery was already built.** The
standing note that "the language has no iterator protocol" (Q24) was
imprecise. The language had generator functions and the iterator
*result shape*: `corpus/accept/a20-coroutine-generator.ts` declares
`function* sequence(limit: i32)` with `yield`, and drives it with
`generator.next()`, reading `step.done` and `step.value`. The C tier
lowers generators through CPS (`compiler.md` §11 coverage list). So
suspendable functions, the `{done, value}` step shape, and both
tiers' lowering of them existed. What was missing was the
**binding**:

- a way to say "this type is iterable" (JS uses `Symbol.iterator`;
  symbols are not in the language, so this needs its own spelling)
- `for…of` desugaring onto that binding
- `keys`/`values`/`entries` on `Array`/`Map`/`Set` returning iterators
- spread, and construction from an iterable (`new Map([[k, v]])`)

Two constraints the design had to answer, both already decided
elsewhere: iteration order for `Map`/`Set` is **normative** insertion
order (Q24), so the protocol inherits a fixed order rather than
choosing one; and callbacks are **non-escaping by construction**
(C5), whereas an iterator is a stateful object that outlives the call
that made it — which is a memory-model question (invariant 2, no
implicit GC), not a syntax question. *(Superseded. §70's
reference-counted handle answered the memory-model half on
2026-08-27, and `Generator<T>` already escapes its creating call. The
current obstacle is §70.1 decision 1's scope and the missing view
type — see the status section above.)*

**Regex — Boa's engine is a reusable Rust crate.** Boa does not
hand-roll one: `core/engine/Cargo.toml` depends on **`regress`**
(pinned `0.10.4` with the `utf16` feature in Boa's workspace root), a
regex engine written for JS semantics rather than Rust's `regex`
crate — which matters, because JS regexes have backreferences and
lookbehind that `regex` deliberately excludes. This is the same shape
as the `ryu-js` finding: the expensive part is an existing crate, so
the cost question is "does it fit the constraints", not "can we write
one".

Two constraints to check before adopting it, neither checked at the
time: `regress`'s `utf16` feature is aimed at JS's UTF-16 strings
while this language stores UTF-8 (Q5), so the index domain had to be
settled; and §0.2 requires one implementation behind an opaque
`subscript_rt_*` symbol on both tiers, which a crate satisfies as
long as the ship tier links it rather than emitting anything.

**Both checked and resolved in favour of adoption; shipped as P23
(`stdlib.md` §15, Q31, `specs/tracking/p23-regex.md`, 2026-07-27).**
The index domain needed no conversion — `regress` matches UTF-8
natively and returns byte offsets, which is Q5's domain, and the
`utf16` feature stays **off**. A third constraint this section did not
anticipate turned out to be the real one: `regress` has no execution
budget at any version, and an unbounded match is a hang the host cannot
interrupt, so the engine is a fork that adds one.

## Undecided — the two that are not simple

- **`flat`/`flatMap`.** The depth appears in the result type, so
  `flat(depth)` with a runtime depth cannot be typed. A depth-1-only
  form is implementable; whether a partial API is better than none is
  not decided.
- **`String.fromCharCode`.** Takes UTF-16 code units, and a lone
  surrogate has no UTF-8 representation. Accepting only the
  non-surrogate range would be a silent divergence from JS rather than
  a subset.

## How to keep this current

Every rejection carries one of the reasons above. A rejection recorded
with any other reason — "not in v1", "redundant", "JS-number op" — has
not been checked against the rule and should be.

A prerequisite row that the project later acquired moves to the
status section, with the obstacle that stays named per form. The two
rows that P22 and P23 made obsolete on 2026-07-27, and that this file
carried until 2026-09-10, are the worked example.
