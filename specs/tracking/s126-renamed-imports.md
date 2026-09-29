# Renamed imports — compiler.md §126

## Change

`resolve_imports` finds the imported name in the target exports and scope.
It registers the declaration under the local name.
S016 names and points at the imported name. S017 points at the local name.
The missing-module poison path keeps both names without a change.

No Rust file needs a split: both edited Rust files remain below 2,000 lines.
No existing `.expected` file changes. The new golden states the contract values.

## Red at d9886bb

Built HEAD before production edits with `cargo build --offline --locked -p subscript-cli`.
Ran both new corpus entries through `target/debug/subscript run`.

The accept entry exits 1 with seven S016 diagnostics:

```text
main.ts:7:25: `Crate` is not exported by `./lib`
main.ts:7:41: `total` is not exported by `./lib`
main.ts:7:56: `increment` is not exported by `./lib`
main.ts:8:11: `Choice` is not exported by `./lib`
main.ts:8:28: `Status` is not exported by `./lib`
main.ts:8:44: `select` is not exported by `./lib`
main.ts:8:62: `Container` is not exported by `./lib`
```

A separate temporary probe imports only `{ g as f }` and prints `f()`.
It uses the accept entry's library. It exits 0 and prints `1`; the contract requires `2`.

The reject entry exits 0 and prints `1`; the contract requires S016 at `missing`.

## TypeScript measurement

TypeScript 5.9.2, with `prelude/lang.d.ts`, measured both entries.
Options: `--noEmit --strict --target ES2022 --module ESNext --moduleResolution Bundler --lib ES2022,ESNext.Disposable`.
The accept entry exits 0 without diagnostics.
The reject entry exits 2:

```text
main.ts(8,10): error TS2305: Module '"./lib"' has no exported member 'missing'.
```

## Tests

`compiler/tests/renamed_imports.rs` has one test each for rules 1, 2, and 4.
Each test includes a firing control.
They check declaration selection, local visibility, S016 names and positions, and S017 local collisions.
The existing discovery tests check both names in a poisoned import.
The targeted run passes: 3 renamed-import tests, 8 discovery tests, and 38 reject-corpus tests.
The tests finish in 0.01, 0.01, and 0.19 seconds respectively.

The accept corpus covers all seven declaration kinds, including a live global after an exporter write.
The full gate compares its output across the interpreter, dev JIT, ship C, and Node.

## Before the full gate

`cargo fmt` uses the pinned Rust 1.95.0 toolchain.
`cargo build --offline --locked --workspace --all-targets` passes without warnings in 12.62 seconds.
`tools/hygiene.sh` passes. The diff review finds no out-of-scope file or existing golden change.
The CLI accept run matches all eight golden lines.
The CLI reject check reports one S016 at `main.ts:8:10`, with the imported name `missing`.
## Gate

The first full gate failed: `generated_ai_references_are_byte_identical`
(debug and release), because `generated-docs/corpus-index.md` was not
regenerated. After `generate-api-reference`, one re-run:

```text
gate full d9886bbab24f5d62f639e327a814fd43c3de9f3a dirty:8 debug 1887/0/3 release 1884/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

## Phase Review

No CRITICAL, no MAJOR in the §126 diff. Two MINOR, fixed: the contract
cited §107 for poisoned imports (it is §63 rule 5), and acceptance 1
now states the Red shape measured above.

Open, outside §126: an assignment to an imported module global is
accepted (`import { count } from "./lib"; count = 10;` prints `11`
after `bump()`); `tsc` gives TS2632 and `node` throws. It is its own
section.
