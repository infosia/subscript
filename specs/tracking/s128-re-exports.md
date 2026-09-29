# Named re-exports — compiler.md §128

## Contract and files

The contract pin is `162cfc9`. No git write command ran. No existing golden changed.

- Export definitions retain dependency edges; resolved exports map names to declaration identities.
- A source re-export creates no local binding. A local re-export can resolve a declaration or a named import.
- Resolution follows export dependencies, including imports behind local re-exports. Cycles report the source module and export name.
- Imports read resolved exports and retain the read-only binding flag.
- Host entry checks retain the declaration's export predicate.
- The parser returns re-export sources. The CLI loader follows those sources.
- `NamedModuleSurface` supplies the C18 divergence block for wildcard, namespace, and default exports.

Changed Rust files:

- `cli/src/program_loader.rs`
- `cli/tests/commands.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/exports.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/signatures.rs`
- `compiler/src/divergence.rs`
- `compiler/src/parse.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/re_exports.rs`
- `codegen/tests/re_exports.rs`

Corpus additions: `a288-re-export-kinds`, `a289-local-re-exports`, `a290-re-export-module-cycle`, and their new goldens.
Reject additions: `r270-re-export-missing`, `r271-re-export-alias-cycle`, `r272-re-export-duplicate`, and `r273-export-star`.
Each reject has a row in `compiler/tests/corpus_reject.rs`.
The generator updated `generated-docs/corpus-index.md`.

No pure-move split was required. The oversized `compiler/src/lib.rs` changes one existing re-export line and adds no lines.

## Red and TypeScript measurements

Built the unchanged pin with `cargo build --offline --locked -p subscript-cli`.
Saved that binary before implementation. Each entry ran through its `main.ts` with `subscript check`.
Every entry exited 1. The accept entries reached S100, with missing-export diagnostics after it.
The reject entries reached S100 instead of S016 or S017.
For `r273`, the Red S100 had no divergence block; the contracted C18 block was absent.

TypeScript version: 5.9.2. Each invocation included every `prelude/*.d.ts` and each entry's source files.
Options: `--strict --noEmit --target es2022 --module esnext --moduleResolution bundler --lib ES2022,ESNext.Disposable`.
Accept entries and `r273` exited 0. `r270`, `r271`, and `r272` exited 2 with TS2305, TS2303, and TS2300.
Each header states that measurement.

```text
## a290-re-export-module-cycle
Red exit: 1
error[S100]: only `export` declarations and named imports are in the decided surface
 --> left.ts:8:1
  |
8 | export { right } from "./right";
  | ^
  = rule: Constructs outside the decided language surface are rejected.
error[S016]: `right` is not exported by `./left`
 --> main.ts:7:16
  |
7 | import { left, right } from "./left";
  |                ^
  = rule: Every type, value, and imported name must bind to a declaration.
error: 2 error(s)

tsc exit: 0

## a288-re-export-kinds
Red exit: 1
error[S100]: only `export` declarations and named imports are in the decided surface
 --> surface.ts:7:1
  |
7 | export { call, Crate, total, increment, Choice, Status, select, Container } from "./bridge";
  | ^
  = rule: Constructs outside the decided language surface are rejected.
error[S016]: `call` is not exported by `./surface`
 --> main.ts:7:10
  |
7 | import { call, Crate, total, increment, Choice, Status, select, Container } from "./surface";
  |          ^
  = rule: Every type, value, and imported name must bind to a declaration.
error[S016]: `Crate` is not exported by `./surface`
 --> main.ts:7:16
  |
7 | import { call, Crate, total, increment, Choice, Status, select, Container } from "./surface";
  |                ^
  = rule: Every type, value, and imported name must bind to a declaration.
error[S016]: `total` is not exported by `./surface`
 --> main.ts:7:23
  |
7 | import { call, Crate, total, increment, Choice, Status, select, Container } from "./surface";
  |                       ^
  = rule: Every type, value, and imported name must bind to a declaration.
error[S016]: `increment` is not exported by `./surface`
 --> main.ts:7:30
  |
7 | import { call, Crate, total, increment, Choice, Status, select, Container } from "./surface";
  |                              ^
  = rule: Every type, value, and imported name must bind to a declaration.
error[S016]: `Choice` is not exported by `./surface`
 --> main.ts:7:41
  |
7 | import { call, Crate, total, increment, Choice, Status, select, Container } from "./surface";
  |                                         ^
  = rule: Every type, value, and imported name must bind to a declaration.
error[S016]: `Status` is not exported by `./surface`
 --> main.ts:7:49
  |
7 | import { call, Crate, total, increment, Choice, Status, select, Container } from "./surface";
  |                                                 ^
  = rule: Every type, value, and imported name must bind to a declaration.
error[S016]: `select` is not exported by `./surface`
 --> main.ts:7:57
  |
7 | import { call, Crate, total, increment, Choice, Status, select, Container } from "./surface";
  |                                                         ^
  = rule: Every type, value, and imported name must bind to a declaration.
error[S016]: `Container` is not exported by `./surface`
 --> main.ts:7:65
  |
7 | import { call, Crate, total, increment, Choice, Status, select, Container } from "./surface";
  |                                                                 ^
  = rule: Every type, value, and imported name must bind to a declaration.
error: 9 error(s)

tsc exit: 0

## a289-local-re-exports
Red exit: 1
error[S100]: only `export` declarations and named imports are in the decided surface
 --> surface.ts:9:1
  |
9 | export { local as other, imported as again };
  | ^
  = rule: Constructs outside the decided language surface are rejected.
error[S016]: `other` is not exported by `./surface`
 --> main.ts:7:10
  |
7 | import { other, again } from "./surface";
  |          ^
  = rule: Every type, value, and imported name must bind to a declaration.
error[S016]: `again` is not exported by `./surface`
 --> main.ts:7:17
  |
7 | import { other, again } from "./surface";
  |                 ^
  = rule: Every type, value, and imported name must bind to a declaration.
error: 3 error(s)

tsc exit: 0

## r273-export-star
Red exit: 1
error[S100]: only `export` declarations and named imports are in the decided surface
 --> corpus/reject/r273-export-star/main.ts:8:1
  |
8 | export * from "./lib";
  | ^
  = rule: Constructs outside the decided language surface are rejected.
error: 1 error(s)

tsc exit: 0

## r272-re-export-duplicate
Red exit: 1
error[S100]: only `export` declarations and named imports are in the decided surface
 --> corpus/reject/r272-re-export-duplicate/main.ts:8:1
  |
8 | export { a as x, b as x } from "./lib";
  | ^
  = rule: Constructs outside the decided language surface are rejected.
error: 1 error(s)

tsc exit: 2
corpus/reject/r272-re-export-duplicate/main.ts(8,15): error TS2300: Duplicate identifier 'x'.
corpus/reject/r272-re-export-duplicate/main.ts(8,23): error TS2300: Duplicate identifier 'x'.

## r271-re-export-alias-cycle
Red exit: 1
error[S100]: only `export` declarations and named imports are in the decided surface
 --> corpus/reject/r271-re-export-alias-cycle/main.ts:8:1
  |
8 | export { x } from "./lib";
  | ^
  = rule: Constructs outside the decided language surface are rejected.
error: 1 error(s)

tsc exit: 2
corpus/reject/r271-re-export-alias-cycle/lib.ts(6,10): error TS2303: Circular definition of import alias 'x'.

## r270-re-export-missing
Red exit: 1
error[S100]: only `export` declarations and named imports are in the decided surface
 --> corpus/reject/r270-re-export-missing/main.ts:8:1
  |
8 | export { nope } from "./lib";
  | ^
  = rule: Constructs outside the decided language surface are rejected.
error: 1 error(s)

tsc exit: 2
corpus/reject/r270-re-export-missing/main.ts(8,10): error TS2305: Module '"./lib"' has no exported member 'nope'.
```

## Checks before the full gate

- Compiler re-export tests: 8 passed.
- Reject corpus tests: 38 passed.
- Renamed import tests: 3 passed. Read-only import tests: 19 passed.
- Public dependency parser tests: 2 passed.
- Input order test: all 24 permutations passed under the interpreter; the changed declaration control produced the changed output.
- The input order test took 85.455125 ms. It avoids repeated native compilation across permutations.
- CLI re-export chain test: passed, including the missing-export control; 0.95 seconds.
- Node v24.18.0 with TypeScript 5.9.2 ran all three new accept entries. Their bytes match the new goldens.
- `cargo build --offline --locked --workspace --all-targets`: passed without warnings.
- Compiler Clippy: 3 library warnings, all at existing sites. No new warning remains.
- Generated docs: regenerated through `cargo run --offline -p subscript-compiler --bin generate-api-reference`.

## Gate boundary

Run `tools/gate.sh full` once after the final format check, with no concurrent work or edits.
The final response records its verdict. The gate checks both build profiles, the engines, Node, TypeScript, and hygiene.
The C18 heading remains the owner's change, as the handoff requires. This round does not change `specs/blocks/collisions.md`.

## Gate failure fixes

The first full gate failed on two tests in both profiles.

1. `collision_ids_and_headings_are_total` reported the absent C18 heading.
   The owner added C18 to `specs/blocks/collisions.md`.
   The agent made no change to `specs/blocks/` in this follow-up.
   The divergence test now passes.
2. `directory_emit_matches_the_shared_directory_reader` found different C text for `a289-local-re-exports`.
   The CLI discovered `main, surface, lib`; the corpus reader supplied `main, lib, surface`.
   Function numbers and definition order differed.
   The first correction sorted dependency source paths after loading. That correction was wrong: it changed module initialization order.
   The review correction below removes that sort and gives both readers the same discovery function.

The CLI test `import_order_does_not_change_emitted_c_with_a_firing_control` supplies the same program with two import orders.
Before the loader change, it failed with `import order changes program.c` (exit 101).
After the change, both orders produce identical `program.c`, `program.alloc.h`, and `entry.c` bytes.
The firing control changes a called function's return value. That change produces different `program.c` bytes.
The test took 41.484458 ms. It emits C without native compilation.

Follow-up checks before the gate:

- All 21 CLI command tests pass, including the directory reader comparison and existing diagnostic tests.
- The C18 divergence totality test passes.
- Generated docs were regenerated through `generate-api-reference`.
- `cargo fmt --check` passes.
- `cargo build --offline --locked --workspace --all-targets` passes without warnings.
- No existing `.expected` golden or `codegen/tests/lir-goldens/` snapshot changed.

The follow-up runs `tools/gate.sh full` exactly once. Its final verdict appears in the final response.


## Phase Review corrections

The amended contract is at `d8ac0e0`. These changes correct the implementation of §128; they add no initialization-order behavior.
No change in this round touches `specs/blocks/`, an existing golden, or an import-type rule. No git write command ran.

### Loader order

The review measured `main.ts` with imports `./b`, then `./a`.
The path sort printed `init a / init b`; the pin and Node printed `init b / init a`.
The sort also rejected `a.ts`'s `y = b + 1` with S100, although the pin printed `y=6 b=5`.
Those before-sort-correction measurements come from the review handoff.

Removed the sort. `discover_module_sources` now owns the breadth-first traversal in `compiler/src/parse.rs`.
The CLI and corpus reader call it. Each import or re-export source takes its position in source order.
The entry stays first; ambient mirrors retain their input order. The function deduplicates stable file identities.
The public API has a direct unit test for dependency order, re-exports, cycles, missing sources, and parser diagnostics.

Replaced the C-byte import-order-invariance test with `cli_and_corpus_keep_import_initialization_order`.
It compares CLI C output with corpus-reader C output, and compares CLI execution with the interpreter.
The reverse import order is its firing control. Measured outputs:

| Imports | CLI and interpreter output |
| --- | --- |
| `b`, `a` | `init b\ninit a\n1 2\n` |
| `a`, `b` | `init a\ninit b\n1 2\n` |
| `b`, `a`, with `a.y = b + 1` | `y=6 b=5\n` |

The test took 1.047 seconds. Separate probes printed both initialization orders under Node and the corrected CLI.
TypeScript 5.9.2 accepted both probes and the dependency probe. Node and the corrected CLI printed `y=6 b=5`.
All existing directory-entry outputs match their goldens. Both readers emit identical C for every directory entry.

### Named forms and poisoned exports

- Rule 7b: type-only export lists and type-only specifiers report S100 with C18. Ordinary named type exports remain accepted.
- Rule 7a: `default` in either name position reports S100 with C18, including string-name spellings.
- Rule 5: failed export names resolve to poison. Imports and subsequent re-exports emit no additional missing-name diagnostics.
- Alias cycles retain their failure cause. Each alias reports once, including prefixes, regardless of input order.
- Export diagnostics retain module order, even when dependency resolution visits another module first.
- Rule 5a: absent discovery re-export sources resolve to poison and retain a `poisoned_imports` record for the codegen guard.
- Rule 7c: every mirror export list, including an empty list, reports S100.
- Each new corpus source now has a purpose that describes its role. The generator updates the corpus index.

Red measurements used a saved binary before these checker corrections, after the loader correction.
Both `r274-type-only-export` and `r275-default-export-name` returned exit 0 and `check: ...: no errors`.
Both now report S100 with C18. Their reject-table rows pin line 8.
TypeScript 5.9.2 accepts both entries with all prelude declarations and the recorded strict corpus options.
The type-only entry contains no value use, so its measured header is `tsc: accepts`.

Additional probes, with the same saved binary:

| Probe | Before | Corrected | TypeScript / Node |
| --- | --- | --- | --- |
| Type-only re-export of `Box`, then `new Box()` | prints `4`, exit 0 | one S100 with C18 | TS1362, exit 2 |
| `value as default`, imported as `d` | prints `4`, exit 0 | one S100 with C18 | accepts; Node prints `4` |
| Missing `nope as x`, then `x as y`, then import `y` | three S016 diagnostics | one S016 at `bridge.ts:1:10` | one TS2305 at `bridge.ts:1:10` |

`type_only_re_export_value_use_reports_ts1362_and_c18` runs TypeScript and pins TS1362 plus the compiler's C18 variant.
Its ordinary-export control passes both compilers. The two TypeScript processes took 0.378 seconds together with checker calls.

### Tests and final checks

- Compiler re-export tests: 12 passed. Missing-name chains cover all 24 file orders; alias cycles cover all six file orders.
- The poisoned-local-export test compares the complete diagnostic list and preserves the imported/local name record.
- Discovery and mirror tests each include accepted and rejected controls.
- Parser tests: 12 passed, including the new public discovery API test.
- Reject corpus tests: 38 passed, including table coverage and divergence blocks.
- CLI tests: 21 passed, including directory goldens and initialization order.
- Export identity test: all 24 input orders passed; the changed-declaration control fired. Cost: 84.735 ms.
- Compiler Clippy: three existing library warnings, plus two existing test warnings. No new warning.
- Workspace all-target build: passed without warnings in 17.03 seconds.
- `cargo fmt --check` and `git diff --check`: passed.
- Generated docs: regenerated through `generate-api-reference`.

Additional changed files in this correction: `compiler/src/lib.rs`, `compiler/tests/tsc_corpus.rs`, and `codegen/tests/corpus/mod.rs`.
New reject directories: `r274-type-only-export` and `r275-default-export-name`.
The final full gate runs once, with no concurrent work or edits. The final response quotes its final verdict line.

## Second Phase Review: failure origins

The amended contract is at `430f14f`. The cycle rule is one S016 per cycle.
The updated handoff permits count-header edits in existing resolution rejects. Only r268 needs that permission.
No existing golden changes. No git write command runs.

### Form

The export graph maps each name to a declaration, a local dependency, a remote dependency, or poison.
`ExportResolution.items` carries a declaration or poison; it cannot carry a diagnostic to a downstream edge.
The origin table owns name and cycle diagnostics, ordered by program file and source position.
A back edge identifies exactly the cycle suffix. Its earliest member owns the diagnostic; prefixes receive poison.
The cycle message starts at that member and includes each cycle member, without the prefix.
A missing module statement reports S100 at its source specifier and gives every exported name poison.
Imports from missing modules also bind their named imports poisoned, so local uses and local re-exports stay silent.
Export bindings retain whether they come from a re-export. The scope alone reports two declaration names.
Export collisions that involve a re-export retain S017 at the second export.

The earlier Phase Review record described one diagnostic per alias, including prefixes. That implementation violated the amended rule.
The current tests require one diagnostic per cycle and compare complete diagnostic lists.

### Red and measurements

Before the implementation change, the updated checker tests failed in four places (10 passed, 4 failed, 0.03 seconds).

| Shape | Red result | Corrected result |
| --- | --- | --- |
| Prefix into a two-member cycle | Three S016 diagnostics; each message included the prefix | One S016 at the first cycle member |
| Two names in each of two missing-module statements | Four S016 diagnostics at names | Two S100 diagnostics at source specifiers |
| Exported function and global named `f` | Two S017 diagnostics | One scope S017 |
| Discovery re-export without the option | S016 at the name | S100 at the source specifier |

Stock TypeScript 5.9.2 measured the following probes with all prelude declarations and the strict corpus options.
Each probe exited 2. Temporary files stayed outside the corpus.

| Shape | TypeScript result | Time |
| --- | --- | --- |
| `main` imports a prefix into a two-member cycle | One TS2303 | 0.176 seconds |
| `export { a, b }` from an absent module | One TS2307 | 0.187 seconds |
| Exported function and global named `f` | Two TS2300 | 0.172 seconds |
| Exported class and enum named `K` | Two TS2567 | 0.173 seconds |

Declaration duplicates follow the scope rule; acceptance 4b compares resolution failures, not duplicate declarations.

### Total count check

Added `// tsc-error-count: <integer>` as a separate header line. Existing `tsc:` claims keep their format.
The count replaces a blank header line in r268, r270, and r271; diagnostic line numbers do not change.
The TypeScript output parser retains repeated codes instead of collapsing them into a set before counting.
The existing batch supplies both code sets and counts. No additional TypeScript process runs for the total check.
The check examines every reject's checker diagnostics and measured TS2303, TS2305, and TS2307 errors.
Resolution failures require a count header; header, TypeScript count, and complete checker diagnostic count must agree.
The firing control adds another missing name to the program. The count check rejects it against the original measurement.
Other controls reject a missing header, a wrong header, and a changed TypeScript count.
A parser test supplies two occurrences of one TypeScript code and retains both.

Measured: r268, r270, and r271 each have header 1, TypeScript 1, and checker 1.
The batch measured 538 corpus entries in 1.040 seconds. The additional checker pass took 78.413 milliseconds.
The count firing control took 5.558 milliseconds. All nine TypeScript harness tests passed in 1.18 seconds.

### Loader and corpus reader

The CLI no longer parses the entry to select its display name before discovery.
Discovery parses each source once and sends each specifier to the loader; the loader selects supported relative paths.
The CLI keeps its existing display names, including diagnostics for unsupported parent and nested paths.
All 21 command tests pass (2.91 seconds), including directory goldens and both initialization orders.
The corpus reader rejects every unreachable directory source and names the file.
Its firing control creates `lost.ts`, asserts the failure text, then adds a re-export that reaches it.
That test took 6.882 milliseconds. The 24-order export identity test took 84.964 milliseconds.
Both codegen tests passed in 0.09 seconds.
The `NamedModuleSurface` documentation now includes type-only forms and the `default` export name.

### Checks

The 16 checker tests cover all six cycle file orders, a downstream import, same-file source order, and separate cycles.
They also cover both declaration-duplicate shapes, missing module statements, and poisoned local imports.
All 38 reject corpus tests pass, including rows for r270 through r275 and divergence blocks (0.19 seconds).
Compiler Clippy retains its three library warnings and two existing test warnings. Changed sites add no warning.
No Rust file requires a size split in this round.
The generated documentation is regenerated through `generate-api-reference` before the final gate.
The final full gate runs once, after review, formatting, and the warning-free workspace build, with no concurrent work or edits.

The independent no-context Phase Review found no CRITICAL, MAJOR, or MINOR issue.
The 12 parser tests pass, including shared discovery order. `cargo fmt --check`, `git diff --check`, and hygiene pass.
The offline locked workspace all-target build passes without warnings.

## Third Phase Review: rejected declarations remain graph nodes

The amended contract is at `f3c601e`. This round changes no file in `specs/blocks` and no existing golden.

### Declaration names and failure origins

Declaration collection now retains every declared name, even when the declaration produces no scope item.
An exhaustive match covers every AST declaration kind. Binding patterns use the existing recursive name collector.
A rejected name enters the local scope as poison. Export collection copies that identity into the export graph.
Thus direct imports, remote re-exports, and local re-exports reach the original failure without another diagnostic.

Two new checker tests failed before the implementation change. The rejected numeric alias produced one S100 and two downstream S016 diagnostics.
A type-only export from an absent module produced its C18 S100 and a missing-module S100.
The corrected tests compare complete diagnostic lists, including positions, divergence variants, and the resolution flag.

The declaration test covers a numeric alias, a generic string alias, an interface, and an array binding pattern.
Each shape uses direct exports and local export lists, direct imports, and an import through a remote re-export.
Eight supported-declaration controls check that the graph still resolves real declarations.
The test performs sixteen small checker calls, with no code generation. Measured cost: 6.952 milliseconds.

New reject entry `r276-rejected-declaration-export` covers both pattern names through direct imports and a re-export chain.
The pre-change CLI returned exit 1 with five diagnostics: one pattern S100, two re-export S016, and two import S016.
The corrected checker returns only the pattern S100 at `lib.ts:8:14`.
The reject table includes that position. Every corpus file has its own purpose line.

TypeScript 5.9.2 accepts all eight declaration probes with the prelude and strict corpus options.
Each probe took 0.177–0.319 seconds. The batched corpus check also accepts r276.
The aliases and interface have no divergence variant at their existing rejection sites; unit tests pin those sites without adding incompatible corpus headers.

### Resolution diagnostic facts

`Diagnostic.resolution` records whether import or export resolution owns a diagnostic. The constructor defaults it to false.
The resolver sets it for missing names, absent module statements, and alias cycles.
The corpus count check reads this field. It no longer classifies diagnostics by message substrings or TypeScript code text.
Every reject passes through the count check. Only resolution failures or explicit count headers require count equality.

A new firing control writes a temporary reject entry with TS2305 and no count header.
The normal corpus reader finds it. The checker reports a resolution diagnostic, and the total check rejects the missing count.
Adding the header to the source makes the same check pass. No additional TypeScript process is needed.
Measured cost: 6.596 milliseconds. Existing mismatch controls remain.
The default field value has a direct diagnostic unit assertion; resolver tests assert the true value in exact diagnostic lists.

The batch measured 539 entries in 1.032 seconds. Its checker pass took 80.466 milliseconds.
Resolution entries r268, r270, and r271 each retain header 1, TypeScript 1, and checker 1.
No existing reject header needs another edit in this round.

### Unsupported export sources

Named export classification occurs before source resolution. An unsupported statement carries poison without resolution of its source.
Discovery records skip those statements too. A shared predicate classifies type-only, default-name, and namespace specifiers.
Five exact-list cases cover whole-list type-only, per-name type-only, default-name, wildcard, and namespace exports from an absent module.
Each reports only its C18 S100. The test uses five checker calls without code generation or file loading.
Measured cost: approximately 1.2 milliseconds.
TypeScript reports one TS2307 for each absent-source probe, exit 2; each probe took 0.174–0.177 seconds.
These cases use unit tests because their fixed C18 divergence block precludes a `tsc: rejects` corpus header (§79 rule 6).

### Checks and costs

The CLI chain test now states its cost: one native run checks source loading; the failure control only checks types.
It prints its elapsed time. Measured cost: 1.012 seconds.
Compiler export tests: 18 passed. TypeScript harness tests: 10 passed. Reject corpus tests: 38 passed.
The existing exact-list tests retain coverage for cycle prefixes, two-name absent-module statements, and declaration duplicates.
No changed Rust file needs a size split. No git write command runs.
Generated documentation, warning-free workspace build, formatting, and hygiene precede the single final full gate.

The offline locked workspace all-target build passed without warnings in 11.81 seconds.
Compiler Clippy retains three library warnings and two existing test warnings; no changed site adds a warning.
The documentation generator completed. `cargo fmt --check`, `git diff --check`, and hygiene passed.
No existing `.expected` file changed. The full gate now runs once, without concurrent work or edits.
