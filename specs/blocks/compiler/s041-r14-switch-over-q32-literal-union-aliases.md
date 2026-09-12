<!-- §41 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 41. R14 — `switch` over Q32 literal-union aliases

Owner decision 2026-08-02 (downstream request R14; not
hard-blocking — the downstream held its slice on the same-day
cadence rather than shipping an `if/else` fallback that a
102-member alias would make both slow and silently
non-exhaustive). Probed before contracting, `--lib es2022`
standalone: an exhaustive alias switch, a `default` subset, a
missing-member switch, and a duplicate-member switch are all
stock-`tsc`-clean; a non-member `case` label is `tsc`-rejected
(TS2678).

### 41.1 Checker

A Q32 string-literal union alias (§24) joins the legal `switch`
discriminant types. On an alias-typed discriminant every `case`
label must be a string literal naming a member (§24 contextual
typing; a non-member rejects — r113, `tsc`-rejected too,
recorded). Closed-set exhaustiveness: **without `default`, the
cases name every member exactly once** — a missing member rejects
(r112, `tsc`-clean, strictly narrower) and a duplicate member
rejects (r114, `tsc`-clean); with `default`, any subset of
distinct members is legal. Nothing else about `switch` changes
(break, fallthrough, scoping, other discriminant types).

### 41.2 Lowering (both tiers)

Case labels lower to their §24 `i32` discriminants; dispatch is
integer compare only — never a string comparison. The ship tier
emits a C `switch` over the discriminant (jump-table eligible);
the dev tier lowers as the existing integer switch. A cemit test
pins the a115 switch's emitted form: integer case labels, no
string-comparison call (the §24 test precedent).

### 41.3 Corpus

`a115-switch-literal-union` (accept): an exhaustive switch over a
three-member alias and a `default`-subset variant; the golden pins
the dispatch result for every member through both paths. Rejects:
`r112-switch-alias-missing-member` (no `default`, one member
absent; `tsc`-clean recorded), `r113-switch-alias-non-member` (a
label outside the set; `tsc` status recorded), `r114-switch-alias-duplicate-member`
(`tsc`-clean recorded).

### 41.4 Exit criteria (pre-registered)

1. `a115` byte-identical under both tiers.
2. `r112`–`r114` pin (code, line) with `tsc` statuses recorded in
   their headers.
3. The cemit pin of §41.2 (integer labels, no string compare).
4. Checker unit tests: missing member, duplicate member,
   non-member, `default` subset, and an exhaustive switch
   accepted.
5. Full gate green; `tsc` gate green with `a115` included; no
   existing golden moves; zero-warning sweep green;
   generated-docs byte-compare gates green.
