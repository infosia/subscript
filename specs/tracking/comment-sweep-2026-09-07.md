# Comments in Rust code — the sweep of 2026-09-07

Status: **landed** at `59b68e2`. Contract: `specs/blocks/compiler.md`
§5.x (`7e79353`). Origin: the owner's request of 2026-09-07 — no
phase or fix history in comments; no redundant comments; the
rewriting done by Opus.

## Measured before

127 Rust files, 11,671 comment lines. A pattern over phase (`P6`),
request (`R39`), observation (`OBS-3`), review round, date, and
former-behaviour words flagged 223 lines: compiler 77 (14 files),
codegen 57 (13), runtime 37 (9), bindgen 33 (10), benchmarks 4 (3),
cli 0, examples 0. Marker counts: `P<n>` 109, `R<n>` 50, `OBS-<n>`
19, "retired" 12, "round <n>" 11.

## The sweep

Four Opus subagents, one per crate group, two at a time (compiler
and runtime; then codegen; then bindgen, benchmarks, cli, examples),
each with the flagged list and the four rules. A stripper that
removes comments and string literals compared every changed file's
token sequence with `HEAD`: 48 files, all identical. Result: 48
files, 368 insertions, 378 deletions; every flagged line rewritten
or deleted except 26 false positives (the runtime's own word
"retired" for an allocation leaving service; "used to <verb>" as
purpose; "no longer" as a runtime state). Rule 3 removed or
shortened 22 more comments (a sentence repeated at four trap sites
that `TrapKind::Internal`'s doc carries; `// First strip comments.`
beside the call; `// Define bodies.`).

Two stale facts corrected on the way: a foreign C-ABI call was
described as having "no lowering path yet" (it lowers to an
imported symbol at `codegen/src/lir.rs` and `lower/mod.rs`), and
`allow_wire_alias_boundary` was said to be set only in a foreign
signature (it is set at three positions, §52.2). One comment said
"legacy module orchestration only" for a key that three live sites
use.

Residual after the sweep: 8 lines, all "previously validated" or
"previously created" as a runtime fact. The `MIRROR_SOURCE` raw
string in `codegen/tests/offsetof_layout.rs` had its TypeScript
comments rewritten too (five lines inside a string literal, outside
the stripper's guarantee; the parser discards them and the line
count is unchanged).

Fresh review: MAJOR 1 — the `S015` note lost its prohibition
("not in use" for "retired, never reused") — restored as "No rule
takes `S015`, and none will (§33.4)". MINOR 3: one wrong section
(§14 for §13.3), one modal, one 35-word sentence; all fixed. The
reviewer checked 150 hunks and every cited section against the §0
index.

Kept by decision: `Q<n>` ids (collision-register citations, rule 1);
"regression tests" as a banner naming a test category; a
`specs/tracking/p4-performance.md` path in `perf-gate.rs` (a file
path, not a phase tag — the owner's call whether tracking paths stay
in comments).

## Gate

```text
gate full 7e793537edaa057fc7c61b2877b89bc4b133ae5e dirty:48 debug 1287/0/2 release 1285/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```
