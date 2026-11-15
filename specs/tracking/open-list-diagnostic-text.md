# Tracking: diagnostic text fixes from open lists (§303)

Contract: `specs/blocks/compiler/s303-diagnostic-text-fixes-from-open-lists.md`.
Origin: the open lists of §164, §186, §241, §244, and §284. On
2026-11-15 the owner told Claude to close the open items that a small
change closes first.

## Red and fix, by rule

Each rule has a unit test in `compiler/src/check/diagnostic_text_tests.rs`
or `compiler/src/warn/tests.rs` that failed at the contract commit
`3fc07dc1`.

| Rule | Before | After |
|---|---|---|
| 1 | note "A function type has no optional parameter (C7); pass every argument, …" | note "Pass every argument, or call a declaration with a default directly."; the `why` keeps C7 |
| 2 | "`.then(...)` takes no `null` callback; `.catch(r)` takes a rejection callback alone" | "`.then(...)` takes no `null` rejection callback; use `.then(f)`" |
| 3 | the §14.4 spread example | a reaction with spread callbacks, and its explicit form |
| 5 | one container reason for each extra parameter | the container reason at the container position; a `Set.forEach` reason for two parameters |
| 5a | a divergence block over forms that `tsc` rejects | site `CallbackExcessParameters` (S014), "`map` callback type declares 3 parameter(s); got 4" |
| 6 | `one` named `(i32, i32) => void` | `(i32) => void` |
| 7 | "copied from `….point`" | "copied from `rec(…).point`" |
| 8 | two S100 at one site | one S100, "`B` is used before its declaration" |

`tsc` 5.9.2 with the prelude of the project, measured for rule 5a: TS2345
for `sort` with 3 parameters, `map` with 4, `Map.forEach` with 4, and
`Map.groupBy` with 2; TS2769 for `reduce` with 5. `node` passes the
index to the `Map.groupBy` callback.

## Phase Review

One fresh review: two MAJOR findings, both in rule 5, and seven MINOR
findings; see §303.3. The MAJOR findings changed the contract: rule 5a
gives the extra parameters a site of the class that `tsc` rejects.

The first full gate failed in two integration tests that pinned the
texts before rules 1 and 5a; see §303.3.
