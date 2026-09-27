# §118 A capture never outlives its frame — evidence

Contract: `specs/blocks/compiler/s118-a-capture-never-outlives-its-frame.md`.
Date: 2026-09-27. Host: arm64 macOS.

## The path-following form and its three reviews

The first form tainted every function-typed parameter and followed the
taint along values, then closed each further path a review found:

| Review | Open paths found |
|---|---|
| 1 | a generator or async argument (generator: `5` then `57394604` on the dev JIT; async: `1874466712` on the dev JIT, `15885840` on the ship C), `yield`, the `??` synthetic local |
| 2 | a held async handle with a caller that throws before `await`: the callee read `0` (dev JIT) and `123456789` (ship C) from a dead environment |
| 3 | a `Generator`-typed parameter, a for-of element, `.next().value`, a loop back-edge |

The first rule 6 (reject every capturing coroutine argument) also
rejected `a149-suspension-state`, which pins §67 rule 1k. The class
reached a third review, so the form changed (CLAUDE.md, two review
rounds).

## Measurement rounds (codex, prototypes reverted, tree checksum equal)

New rejections over 360 existing entries (corpus, examples, mirrors,
benchmark workloads), 32 README and tutorial blocks, and the inline
programs of one `cargo test --workspace --no-fail-fast`:

| Form | New rejections | Actually capturing |
|---|---|---|
| F1 (type-directed, flow-insensitive locals) | 54 (46 C callback aggregates, 6 field stores, 1 push, 1 global) | 0 |
| F1 + N1 (clean `new C(args)`) | 7 (5 corpus generator-in-a-field constructors, 2 interpreter tests) | 0 |
| F1 + N1 + N2 (escape inference) | 0 | 0 |

Every open review probe is rejected under F1 + N1 + N2, and a capture
passed through an indirect call is rejected. N2 found one unsound path
in hot reload: an old lambda survives a reload and calls a callee whose
parameter now escapes (`codegen/src/reload.rs` rechecks the new source,
retains old modules, and preserves globals). §118.1 rule 10 puts the
escape facts in the declaration hash.

## Landing

The rule is `compiler/src/check/capture.rs`; the parameter escape fact
is on the HIR parameter (`compiler/src/hir/parameter.rs`) and in the
reload declaration hash, listed only when the module has a lambda
parameter of a carrier type. The earlier taint and its special cases
are removed.

| Entry | at `274ccf4` | after |
|---|---|---|
| `r240-parameter-stored-in-field` | accepted | S009 at 16:17 (the caller's argument) |
| `r241-parameter-returned` | accepted | S009 at 12:28 |
| `r242-expression-body-returns-parameter` | accepted | S009 at 10:28 |
| `r243-capture-passed-to-generator` | accepted | S009 at 13:10 |
| `r244-capture-passed-to-async` | accepted | S009 at 13:22 |
| `r245-yield-returns-capture` | accepted | S009 at 9:9 |
| `r246-capture-through-loop-back-edge` | accepted | S009 at 12:27 |
| `r247-generator-parameter-stored` | accepted | S009 at 14:17 |
| `r248-capture-through-indirect-call` | accepted | S009 at 11:21 |
| `a262-parameter-called-and-passed-down` | accepted | accepted, `15`, `21`; equal to `node` |

No existing program is rejected. Quick gate:
`gate quick 6eabef9 dirty:30 debug 1693/0/2 skips 2 goldens-moved 0 exit 0`.

## Review of the landed form

A fresh review ran 56 probes and found no escape. It found three
over-rejections and one defect outside §118, each fixed:

- `this` (a builder method `return this`, `registry.push(this)`) and the
  result of an `await` of a held handle were may-capture; §118.1 rule 2
  now lists them as clean.
- A parameter default `b = a` was checked as a storage initializer with
  an empty environment; it is an assignment (rule 4a). The same fix
  exposed a store through the default, which is now rejected at the
  caller's argument.
- `a.push(named)` on an array of functions passed `check` and failed on
  the dev JIT: "a raise site with no raise edge". The raise-site
  predicate read "an operand has a function type"; it reads "the
  built-in calls a script callback" (§115.6 rule 3, amended). Entry
  `a263-array-of-functions-push` runs on the dev JIT and the ship C and
  equals `node`; the reference interpreter cannot pack a function value
  into an array, and the entry declares `interpreter: no`.

## Cost (§118.5 criterion 0)

Checker wall time over 554 corpus entries (accept, warn, trap, reject),
release build, median of 5 runs: `274ccf4` 81.750 ms, working tree
86.920 ms (+6.32%, about 9 µs per entry).

Quick gate after the review fixes:
`gate quick 8e2850c dirty:36 debug 1700/0/2 skips 2 goldens-moved 0 exit 0`.

## Soundness check of the relaxations

A focused review probed `this`, the `await` of a held handle, parameter
defaults, rule 4 by type, and the §115.6 raise-site change on the dev
JIT and the ship C. Each holds. Every callback-taking built-in keeps its
raise edge: a callback that throws inside each of the twelve operations
was caught by the enclosing `try` on both tiers.

Landing gate:
`gate full 8e2850c dirty:36 debug 1700/0/2 release 1697/0/2 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.

## Open items

- `reduce`/`reduceRight`: the initial value is bound to the callback's
  accumulator with no caller check, and the result fact ignores it
  (`compiler/src/check/capture.rs`, the `Reduce` arm). No leak is
  reachable: Q22 rejects a function or `Generator` accumulator, and a
  class accumulator holds only clean fields.
- Outside §118: `??` on a nullable function type passes `check` and fails
  in both engines (dev JIT: "mismatched argument count for jump"; ship C:
  "used type 'SubFn' where arithmetic or pointer type"). No corpus entry
  uses the form.
