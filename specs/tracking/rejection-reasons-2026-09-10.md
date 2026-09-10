# Five rejection reasons that did not fit their form

Status: **contract landed 2026-09-10** as `specs/blocks/compiler.md`
§103. Implementation is open.

Origin: `js-api-sweep.md` presented a standing answer for later
sessions, and its missing-prerequisite table still listed a regex
engine and an iterator protocol. P22 and P23 shipped both on
2026-07-27. The rows stayed for six weeks.

## The measurement

At `be15ee3`, macOS 26.6.2 arm64, with the release `subscript`
binary, TypeScript 5.9.2, and node v24.18.0. 35 probes over every
regex and iteration form, on the checker, the dev tier, the ship
tier, and stock `tsc`. Both tiers agreed byte-exact on every
supported probe.

The per-form result is `js-api-sweep.md`. Five recorded reasons did
not fit the form they reject:

| recorded reason | form | measurement |
|---|---|---|
| "`new Map([[k, v]])` requires a pair element" | `new Set<i32>([1, 2, 3])` | the form has no pair element |
| "subject-only fused view … receiver is `Bag`" | a user method named `values()` | the same class runs when it is named `each()` |
| "`match` **fails stock `tsc`**" | `"s".match(re)` | the call is `tsc`-clean; only the `index` read is TS2322 |
| "`match`/`matchAll`/`search` (a regex engine)" | `s.search(re)` | `search` shipped in P23 |
| "mutable state on a **value**" | `re.lastIndex` | §15.5a makes a `RegExp` a Context allocation |

Two current capabilities were unrecorded. A `Generator<T>` is passed,
returned, and stored in a class field on both tiers. A user class is
iterated through a method that returns a `Generator<T>`.

## Owner decisions, 2026-09-10

| item | decision |
|---|---|
| `new Set(source)` | implement (§103.1) |
| the `exec`/`match`/`matchAll` result type | **deferred, not refused** (§103.5 item 1) |
| receiver-typed view checks | implement (§103.2) |
| four normative reasons | retire (§103.3) |
| a `RegExp` is a handle | adopt (§103.4) |
| the missing corpus entries | add (§103.7) |

## Found while writing the contract, and open

`const ks: i32[] = [...m]` over a `Map<i32, string>` checks clean
here and prints the keys on both tiers. Stock `tsc` answers TS2322,
because it reads the spread as `[i32, string][]`. Invariant 5 says an
accepted program type-checks under stock `tsc`, and this one does
not.

`a81` is `tsc`-clean because its `[...map]` binding carries no type
annotation, so `tsc` infers `[i32, string][]`, the language infers
`i32[]`, and neither reports. The entry hides the disagreement rather
than pinning it.

No decision is taken. The fix changes what `[...map]` means, or
rejects it. Recorded as `compiler.md` §103.5, the `[...map]` item.

## Phase Review of the documentation round

A fresh no-context reviewer reproduced every probed row on both
tiers and confirmed every citation resolves. 0 CRITICAL, 6 MAJOR, 8
MINOR. Every MAJOR concerned a date, a citation, or a stated reason,
and none concerned a measured behaviour.

The two that changed a claim: the escaping-generator row cited `a20`
and `a180`, and neither contains a passed, returned, or stored
generator, so §103.7 requires the entry; and "the `matchAll` fusion
question is answered" over-read §14.3, which fuses an index loop over
a container's own storage, so the decision stays open.

MINOR 10 was not accepted. The reviewer called the user-class
`for…of` diagnostic's "stock `tsc` rejects this subject too"
measurably false. Its counterexample carries `[Symbol.iterator]`,
which S100 rejects first, so that diagnostic never fires on it. On
`r72`, the subject that reaches it, `tsc` answers TS2488.

`tools/hygiene.sh` exit 0.

## Not in this round

`corpus-inventory.md` reads "Status: finding, 2026-08-28. Open." and
says an entry addition edits five count sites. Four of the five hold
no count today. The note states a repaired condition as current,
which is this round's own defect class, in a file this round was not
asked to touch.
