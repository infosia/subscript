# Five rejection reasons that did not fit their form

Status: **landed 2026-09-10** as `specs/blocks/compiler.md` §103.
Contract `a05642d`, implementation the commit that follows it.

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

## The implementation

Three implementer rounds and two Phase Reviews. §103.1 lowers to one
LIR instruction, `SetFromSource`, on all three tiers; the runtime's
four array-literal spread walkers took a sink so the two features
share one traversal. §103.2 splits `check_for_of_subject` into a
wrapper that holds the subject environment and a body.

**Red at the contract pin**, measured against a binary built from it:
`a214` gave 6 × S014 "`new Set(iterable)` is rejected: `new Map([[k,
v]])` requires a pair element"; `a215` gave 3 × S014. `a216` and
`a217` were not Red, and §103.7 asks the round to record that rather
than to predict it.

**Counts**, contract pin → working tree: accept `.ts` 213 → 217,
accept `.expected` 212 → 216, reject `.ts` 182 → 188.

**What moved:** `codegen/tests/lir-goldens/corpus.txt` +1050 lines,
none removed — the test collects every accept entry that declares a
generator, and `a215`, `a216` and `a217` each do.
`generated-docs/corpus-index.md` +10 rows;
`generated-docs/api-reference.md` one accepted row and the rejected
rows. Debug test count 1392 → 1393, the one new unit test. No
`.expected` of an existing entry moved, and no other golden moved.

**Gate**, on the final tree:

    gate full a05642d dirty:34 debug 1393/0/2 release 1391/0/2
              skips 2/0 clippy 7/18/13 goldens-moved 1 exit 0

## The two Phase Reviews

**Round 1: 0 CRITICAL, 2 MAJOR, 9 MINOR.** The reviewer ran 14 probes
on both tiers over shapes the corpus does not cover and found no
defect in the lowering. MAJOR-1: the `for…of` subject environment
still read the member name, because the receiver-typed path returned
before the flag was set — two programs differing only in a method
name, `each<object>()` clean and `values<object>()` S011. MAJOR-2:
§103.7 pre-registered two reject entries that §79 rule 4 forbids.

**Round 2: 0 CRITICAL, 2 MAJOR, 4 MINOR**, and both MAJORs were the
second raise of a class round 1 had raised. §103.8 holds the three
form rules that replaced the named-site fixes.

One finding was not accepted, in the documentation round's review:
the user-class `for…of` diagnostic says "stock `tsc` rejects this
subject too". The counterexample carried `[Symbol.iterator]`, which
S100 rejects first, so that diagnostic never fires on it. On `r72`,
the subject that reaches it, `tsc` answers TS2488.

## The arc after §103

§103 asked what a rejection reason must fit. The sections that follow
came from applying that question to the forms §103 measured.

| section | what it decided | landed |
|---|---|---|
| §104 | a bare `Map` is not an iteration source | 2026-09-10 |
| §105 | the `Array` namespace, and `Array.from` | 2026-09-10 |
| §106 | the reference interpreter stores a generator | 2026-09-11 |
| §107 | binding patterns, by source type and position | contract only |
| §108 | a field carries a value before a constructor returns | contract only |

Three invariant-5 holes came out of it, none of them looked for:
`for…of` and `[...]` over a bare `Map` (§104), and a field with no
initializer (§108). Each accepted a program stock `tsc` rejects, and
the second dereferences a null through a non-nullable reference field.

**§106 paid for itself on its first round.** It restored the
independent witness core principle 12 asks for, and the witness
immediately found that a re-entrant `.next()` on a stored generator
kills both production tiers with a signal — a shape no witness could
see while the interpreter refused the program earlier. §106.5 holds
the measurement.

Two form gaps are recorded open, each because naming sites does not
converge: §79 rule 6a, where the divergence gate reads the reject
corpus and cannot see an `ambient` row that no entry pins; and §85's
clippy count, which reads three `(lib)` lines of a `--all-targets`
run.

## §107, landed 2026-09-11

Binding patterns: array patterns over `T[]`/`FixedArray` and
named-field patterns over a class, in six accepted positions. One
Phase Review (0 CRITICAL, 1 MAJOR, 7 MINOR) and one fix round. The
MAJOR: W004 printed checker-internal storage names; the fix gives the
renderer one origin map for every synthesized local, and
`warnings.md` §2 now records the rendering.

A planner's claim measured false: the fix-round handoff said
`function take([a, b]: string)` is `tsc`-rejected. `tsc` accepts it —
a `string` iterates. The sentence was copied from a review finding
without running `tsc`, against the rule that a claim about another
system's behaviour requires running that system.

    gate full 79a39d7 dirty:32 debug 1429/0/2 release 1427/0/2
              skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0

## §108's implementation round, and the perf gate

Two coding-agent rounds: one stopped by the owner mid-gate, one that
took its tree over. The takeover changed one inherited file
(`codegen/src/lib.rs`, a test that passed on two diagnostics through
`.any`) and verified the rest. Narrowing reached four reject entries
(`r65`, `r68`, `r73`, `r92`), all repaired by an initializer or a
constructor assignment at their own pins; 532 `.ts` files, 76 doc
fences, and 4,166 Rust string literals swept. All 89 mirrors compile.

**The full gate ran six times, 02:37 to 05:44, on one criterion.**
That was a wrong process, and the owner said so: a failed full gate is
a stop-and-report, never a retry; a benchmark miss is never re-run.
The orchestrator's 20-minute watch did not run either. Both are now
standing rules.

**The criterion: `a22 dev-JIT`, limit 25.00x of C.** Measured under
`cargo test` against the repository `target/`: 27.64x, 31.84x,
31.88x, 27.52x, 31.90x — deterministic, 0.2–0.5% spread within a run.
The same binary standalone: 19.37x–20.09x, six runs. The pin binary
19.58x against the current 19.60x. The complete §108 diff on a clean
`9cff0d1` checkout passes; the same checkout without the diff passes.
`target/debug` moved aside: still red. `target/release` deleted and
rebuilt: still red. Under `CARGO_TARGET_DIR` outside the repository:
green, 2/2. **The ratio tracks the target-directory path, not the
code.** What in the launch context does it is not identified; a
2026-09 note records 707,606 stray `.rcgu.o` as an earlier cause of
slow first launches, and 2,648 remain today with `target/debug` at
35 GB. No location change is proposed.

    gate full 9cff0d1 dirty:28 debug 1442/0/2 release 1439/1/2
              skips 2/0 clippy 7/18/13 goldens-moved 0 exit 1   (target/)
    gate full 9cff0d1 dirty:28 debug 1442/0/2 release 1440/0/2
              skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0   (CARGO_TARGET_DIR outside)

Red at the pin: `r224` prints `0`; `r225` and `r226` die with signal
11; `r227` prints `2`. The pin accepts all four.

## §108.4 — `new` on a program-file `declare class`, and `this` before the assignment prefix ends

*(2026-09-12.)* Closes §108.1a. Contract `1814c61`, implementation
`3c78983`, note `f9fb0e6`, Phase Review contract `8934aff`, fix
`6004d20`.

**Measured at `4148eab`.** Eight forms checked clean and died with
signal 11 on both tiers: `new Ext()` on a program-file `declare
class`, a read of the field before its assignment (four spellings,
`tsc` TS2565), a method call on `this` and `this` as an argument
before the assignment (`tsc` clean). `new P(5)` on a declared
bodiless constructor printed `0`. A regex pass over 140 constructors
in the corpus, examples, and benchmarks found no `this` inside a
prefix beyond the two permitted forms; the build agreed.

**Phase Review: CRITICAL 1, MAJOR 2, MINOR 7.** The CRITICAL was in
the contract: form (b) said an initialized field holds a value inside
a parameter default, and both tiers evaluated the default before the
initializers. `count: i32 = 5`, `constructor(n: i32 = this.count)`
printed `0 5` on both tiers; `node` prints `5 5`. Both tiers agreed,
so no golden saw it. §57.1 step 4 now orders the defaults after the
initializers, `lower_new` lowers explicit arguments, initializers,
then defaults, and `a234` pins `5 5`. The two MAJORs were the prefix
end (positional, now "every rule-1 field holds a value") and the
site A `tsc`-accepted class (one form named, four pinned). Six
MINORs closed; one open: a generic class reports rule 6 once per
instance at the template position, as rule 1 does.

Red at the pins: `r229`–`r232` are accepted at `1814c61`; `a234`
prints `0 5` at `8934aff`; `p06` (a site B use after every field
holds a value) is rejected at `8934aff` and accepted after.

    gate full 1814c61 dirty:14 debug 1453/0/2 release 1451/0/2
              skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
    gate full 8934aff dirty:8 debug 1457/0/2 release 1455/0/2
              skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0

`tools/hygiene.sh` exit 0 at `6004d20`.
