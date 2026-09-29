# §125 — per-module top-level names

Contract: `specs/blocks/compiler/s125-each-module-has-its-own-top-level-names.md`.

## Implementation

Steps 1 through 5 passed their quick gates.
The final full gate ran once and passed. No edits or other builds ran during it.

1. JIT global and function slots use LIR ids. HIR-to-LIR maps use declaration symbols.
   The direct storage test gives two globals and two functions the same source names.
   Normal and reload storage each produce `7 101`.
2. The checker assigns module-qualified symbols to globals, functions, classes, and generic templates.
   HIR retains source names beside symbols. Generic instances use template and nominal type identities.
   Static members, workers, initializer analysis, captures, and raise-site analysis use those identities.
   A module declaration hides a mirror declaration only in its own scope. Foreign functions keep their C names.
3. Only host entries have program-wide name uniqueness. A second entry produces S017 and names both modules.
   Tests cover synchronous, scalar, handle, and async entries in both module orders.
   Distinct-name controls and same-name non-host exports are accepted.
4. Eight directory accept entries cover the required shapes. r267 pins duplicate host entries and cites C14.
   The old r267 and r268 directory entries are removed.
   Compiler, CLI, and Node harnesses enumerate directory programs through general rules.
   Node loads sibling TypeScript modules and clears their cache between programs.
   The a19 header records its measured Node agreement. Fixture constants come from the existing mirror.
5. One test declares every rule 2 kind twice, then runs each program on the interpreter, dev JIT, and ship C.
   Its firing control changes only library values. Separate tests cover reload and source-name diagnostics.

## File split

Before additions, the test module in `compiler/src/warn.rs` moved unchanged into `compiler/src/warn/tests.rs`.
This pure move reduces the 2265-line parent below 2000 lines. No signature or visibility changed for the move.
`compiler/src/lib.rs` edits the worker assertion and replaces the old class rejection test; its total line count decreases.

## Red measurements

Baseline: `ef03101`. The production diff from that pin to `51ea474` is empty for compiler, codegen, runtime, and CLI.
The baseline executable used restored tracked sources; the working changes were saved and restored through temporary files.
No baseline probe lives beside the corpus.

| Entry | Baseline result | Required output |
| --- | --- | --- |
| a278 module global | exit 0, `main=101 lib=101` | `main=7 lib=101` |
| a279 generic function | exit 0, `main=1 lib=1` | `main=2 lib=1` |
| a280 generic class | exit 0, `main=100 lib=100` | `main=7 lib=100` |
| a281 function | exit 1, S017: duplicate function name `read` in the program | `main=7 lib=100` |
| a282 class | exit 1, S100: duplicate class name `Box` in the program; three diagnostics | `main=7 lib=100` |
| a283 static member | exit 1, S100: duplicate class name `Box` in the program; five diagnostics | `main=7 lib=100` |
| a284 enum generic instance | exit 1, argument and return mismatches: expects `E`, got `E` | `main=9 lib=1` |
| a285 mirror constant | exit 0, `main=1 lib=1` | `main=7 lib=1` |
| r267 host entry | exit 1, S017: duplicate function name `update` in the program, at lib.ts:8 | S017, both modules named |

The a285 baseline used `build --mirror ... --run`, with the existing interop mirror and runtime library.
The other accept baselines used `run`; r267 used `check`.
TypeScript 5.9.2 accepts all eight accept programs and r267, each with exit 0.
The runs use the repository prelude, strict ES2022, ESNext modules, Bundler resolution, and ESNext.Disposable.
a285 also uses the corpus interop mirror.
Node v24.18.0 and TypeScript 5.9.2 produce all eight goldens and a19's `55` with the directory loader.

## Validation

No existing `.expected` golden changed. No LIR snapshot line changed.
The pinned formatter and warning-free all-target build precede each quick gate.

| Step | Debug passed / failed / ignored | Result |
| --- | --- | --- |
| 1 | 1864 / 0 / 3 | quick passed |
| 2 | 1864 / 0 / 3 | quick passed |
| 3 | 1867 / 0 / 3 | quick passed after two obsolete test expectations were corrected |
| 4 | 1867 / 0 / 3 | quick passed |
| 5 | 1872 / 0 / 3 | quick passed |

The first step 3 quick gate failed two obsolete expectations: cross-module class rejection and duplicate r267 TypeScript headers.
The corrected tests and the repeated step 3 quick gate pass.

## Test cost and coverage

- The direct same-name storage test takes 0.05 seconds for normal and reload storage.
- The compiler identity target has 12 tests and takes 0.01 seconds.
- The codegen identity target has four tests and takes 1.61 seconds.
- The total declaration-kind test takes 0.86 seconds alone, including two C builds.
  Both builds are required: the firing control changes only the library's values.
- The total test prints `main=7,8,9,10,11 lib=100,101,102,103,104`.
  The firing control prints `main=7,8,9,10,11 lib=200,201,202,203,204`.
- The reload test retains library global 100, then observes class and function body values 202, 203, and 204.
- The hash test changes parameter, return, global, and field types between two classes named C.
  Each change alters the fingerprint; a body-only edit does not.
- The CLI command target takes 2.08 seconds, including all nine directory entries.
- The Node comparison target takes 0.38 seconds. The TypeScript target takes 1.03 seconds.
- Repeated Node execution of a278 prints `main=7 lib=101` twice; module state does not leak between runs.

Clippy preflight library counts are compiler 3, runtime 18, and codegen 13, within the gate baselines 7, 18, and 13.

## Changed files

- `compiler/src/check/`: declaration symbols, scope tables, generic keys, host entry validation, and diagnostic labels.
- `compiler/src/hir.rs`, `compiler/src/hir/`: declaration symbols and identity-based lookups.
- `compiler/src/raise_sites.rs`, `compiler/src/warn.rs`, `compiler/src/warn/tests.rs`, `compiler/src/lib.rs`: consumers and tests.
- `codegen/src/lir/`, `codegen/src/lower/`, `codegen/src/reload.rs`: identity-based maps, JIT slots, fingerprints, and labels.
- `compiler/tests/module_global_names.rs`, `codegen/tests/module_global_names.rs`: identity tests and firing controls.
- `compiler/tests/corpus/`, `compiler/tests/corpus_accept.rs`, `compiler/tests/corpus_warn.rs`, `compiler/tests/corpus_reject.rs`: directory corpus support.
- `compiler/tests/operation_signatures.rs`, `compiler/tests/js_corpus.rs`, `compiler/tests/tsc_corpus.rs`: general directory enumeration.
- `compiler/tests/generic_method.rs`, `compiler/tests/retired_reasons.rs`, `compiler/tests/robustness.rs`: identity and reject harness updates.
- `codegen/tests/corpus/mod.rs`, `codegen/tests/support/lir_facts/iteration.rs`, `cli/tests/commands.rs`: shared readers and independent LIR checks.
- `corpus/node/run-js-corpus.cjs`, `corpus/accept/a19-modules/main.ts`: sibling loading and the measured comparison header.
- `corpus/accept/a278-*` through `a285-*`, their new goldens, and `corpus/reject/r267-duplicate-host-entry/`: required executable cases.
- `generated-docs/corpus-index.md`: regenerated corpus index.
- `specs/tracking/s125-global-names.md`: implementation and measurement record.

The final all-target build has zero warnings. Hygiene and `git diff --check` pass.

## Final full gate

Debug: 1872 passed, zero failed, three ignored. Release: 1869 passed, zero failed, three ignored.
TypeScript, hygiene, and the Clippy baseline check pass. No existing golden changed.
Record: `target/gate/20260929T000313Z-full.md`.

```text
gate full 51ea47437b2eb9dc2dfcb914411803b2d108435c dirty:73 debug 1872/0/3 release 1869/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

## Additional baseline diagnostic detail

- a282: S100 duplicates class `Box`; S017 duplicates field `value`; S018 reports no member `value`.
- a283: S100 duplicates class `Box`; two S017 diagnostics duplicate static field `value` and static method `read`.
  Two S018 diagnostics report no static members `value` and `read`.
- a284: two S100 diagnostics reject the argument and return types; each prints `E` for both nominal types.

## Review-fix stop: worker execution on every engine

The review handoff requires same-name worker entries in the total test on every engine.
A temporary two-module probe declares `Message` and `echo` in each module.
Each module spawns its worker, posts one message, closes and joins it, then prints the reply.
The checker produces two worker entries. The dev JIT and ship C both print `main=7` followed by `lib=100`.
The interpreter returns `Unsupported`: `Worker.Spawn requires a runtime worker adapter` before any output.

`codegen/src/interpreter.rs` rejects every Worker intrinsic through that branch.
`corpus/accept/a112-worker-echo.ts` explicitly excludes the interpreter because it needs another interpreter Context and a runtime worker adapter.
An uncalled worker declaration cannot prove worker dispatch identity on that engine.
The all-engine execution requirement needs interpreter worker support, or a contract decision about that engine's coverage.

The handoff requires a stop when its contract cannot be met as written.
The attempted production edits were restored to the incoming working tree; none of the six fixes is complete.
Only this measurement record remains from this round. Existing owner edits remain intact.
No existing golden or LIR snapshot changed. The final full gate did not start.

## Review fixes with the corrected worker coverage

The corrected handoff selects dev JIT and ship C for worker execution.
The earlier interpreter measurement remains valid; it does not block these fixes.

1. `scope_item` resolves the module scope before the ambient scope and mirror type aliases.
   `ScopeItem::TypeAlias` carries the resolved type to type and value consumers.
   Tests cover type aliases, literal constants, typed constants, foreign functions, handles, boundary classes, enums, and string aliases.
   Each test checks sibling visibility and a control that hides the mirror in that sibling.
2. The built-in Error retains its source name. Renderers label its synthetic origin as the prelude.
   Module classes use declaration symbols, so they need no rename of the built-in class.
   A mismatch now reports `Error (prelude/lang.d.ts)` when a module also declares Error.
3. `error` and `error_diverging` store the message unchanged.
   Generic arity, call arity, generator-order, static-constant, and capture messages use source names at their construction sites.
   The reject sweep checks diagnostic messages for declaration symbols and the hidden Error name.
   Its control constructs each invalid diagnostic directly; an ordinary source-name diagnostic passes.
4. The total declaration test retains the five kinds on all three engines.
   It also executes two same-name `echo` workers with distinct same-name `Message` classes on dev JIT and ship C.
   The workers add their module's value to the input. Both engines print `main=7` and `lib=100`.
   The control changes only the library value and prints `main=7` and `lib=200`.
5. The allocation class table adds the module when two class source names match.
   The control uses distinct class names and retains the unqualified name.
6. The reject sweep measures each checker pass separately.
   For 250 entries, the selective pass costs 0.120 seconds; the complete-mirror pass costs 3.211 seconds.
   The whole five-test target costs 3.52 seconds in debug.
   The second pass compares the selective mirror reader with an independent complete fixture set.
   This detects a missing mirror that changes the intended rejection; the r169 and r251 controls demonstrate that failure.

### Ambient-read audit

| Site | Finding and resolution |
| --- | --- |
| `tyres.rs::resolve_type_ref` | Read `type_aliases` before module scope. It now receives the alias through `scope_item`. |
| `expr/literal.rs::check_ident` | The earlier bare-name `ambient_int_consts` read is already replaced by a scope-selected declaration symbol. |
| `lookup.rs::scope_item` | Reads `file_scopes`, then `ambient_scope`, then `type_aliases`; all name consumers share this entry. |
| `expr/call.rs::check_foreign_call` | Reads `foreign_sigs` only after `ScopeItem::Foreign` selects the C symbol. |
| `expr/literal.rs`, `expr/assign.rs` | Read `global_sigs` through the scope-selected symbol. |
| `signatures.rs`, `bodies.rs`, `generics.rs` | Class, function, and template tables use declaration or instance symbols, not ambient name fallback. |
| `expr/entry.rs`, `expr/call.rs`, `expr/namespace.rs` | Built-in value fallbacks follow local and module scope checks. |

### Corpus and Red evidence

`a286-mirror-type-alias-shadow` declares class `SubLogCallback` in main and uses the mirror alias in lib.
The golden is `main=7 lib=100`. The directory reader now detects the callback alias as an interop reference.
The saved `ef03101` executable from the baseline build rejects this entry with S100 and S018.
S100 expects `(string, object | null, object | null) => void` but receives `SubLogCallback`.
S018 reports that the callback type has no member `value`.
The pre-fix working-tree executable produces the same two diagnostics.
The Error regression is Red before the fix: the initializer message contains `got [[Error]]`.

TypeScript 5.9.2 accepts a286. Node agrees with its golden through the corpus directory loader.
The compiler identity target passes 16 tests; the codegen identity target adds allocation metadata checks.
The codegen target, including worker controls and C builds, costs 2.68 seconds in the initial run.
The generated corpus index was regenerated. No existing golden or LIR snapshot changed.
All edited Rust files remain below 2,000 lines; this fix needs no additional split.
The pinned formatter and warning-free all-target build pass before the gates.

The first review-fix quick gate exposed an allocation-position regression from a file name on the synthetic Error position.
The fix retains its empty synthetic position and adds the prelude label only when a diagnostic or metadata name needs it.
This preserves position ids and LIR text. No golden was updated to accept the regression.
Capture diagnostics also construct source labels before message assembly; they apply no final message rewrite.
The Error-layout test expected the hidden name; its assertion now requires the source name while retaining all id and layout checks.

The review-fix quick gate reported four failures: two allocation-position tests, the LIR snapshot, and the obsolete Error-name assertion.
All four pass in focused runs after the corrections. The exception target passes all 13 tests.
The final codegen identity target passes six tests in 1.72 seconds, including both worker controls.
The final all-target build is warning-free. The full gate follows with no other job or edit during its run.

### Review-fix changed files

- `compiler/src/check/{mod,lookup,tyres,bindings,declarations,type_rules,generics,capture}.rs`: alias lookup, source names, and diagnostic construction.
- `compiler/src/check/expr/{literal,call,namespace,assign}.rs`: alias consumers and source-name message arguments.
- `compiler/tests/{module_global_names,exceptions,retired_reasons}.rs`: regressions, controls, diagnostic sweep, and pass timings.
- `compiler/tests/corpus/mod.rs`, `codegen/tests/corpus/mod.rs`: callback-alias mirror detection.
- `codegen/src/cemit.rs`, `codegen/tests/module_global_names.rs`: allocation labels, worker execution, and metadata controls.
- `corpus/accept/a286-mirror-type-alias-shadow/` and its new golden: module class and sibling mirror alias.
- `generated-docs/corpus-index.md`: generator output for a286.
- `specs/tracking/s125-global-names.md`: audit, costs, Red results, and validation.

### Review-fix full gate

The full gate ran exactly once. No edit or other build ran during it.
The C14 text and S017 owner edits were not changed. No commit or git write command ran.
No existing golden or LIR snapshot changed.
Record: `target/gate/20260929T005757Z-full.md`.

```text
gate full 76f5225bc029b4d9fab578322773483a0e9b28ad dirty:79 debug 1879/0/3 release 1876/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

## Second Phase Review fixes

1. Duplicate host entries use `HostEntryCollision`, with two module examples and the C symbol namespace reason (C14).
   The r267 test pins the complete rendered diagnostic, including its source position and divergence block.
2. Calls and static member access report a non-string mirror alias as `type alias X used as a value`.
   Separate tests cover `SubAccess()` and `SubLogCallback.foo`.
3. HIR documentation distinguishes source names from declaration symbols for calls, globals, function values, async calls, and workers.
   Class source names need not be unique. A method carries its declaration symbol within its class.
4. Global lookup failures in expression and place lowering print the source name.
   The typed mirror constant test checks the error without adding storage support.
5. The total test removes the base=200 repeat on every engine, including workers.
   It adds static field and method keys and a shared generic template instantiated over two enums named E.
   It prints `main=7,8,9,10,11,13,14,12 lib=100,101,102,103,104,106,107,105` on all three engines.
   Workers retain the measured dev JIT and ship C coverage and print `main=7` and `lib=100`.
   The distinct module values detect shared storage directly. The reload test retains its necessary body-edit pass.
6. The reject sweep runs the complete-mirror comparison only when selective diagnostics contain S016.
   The comment states the missing-mirror reason and removes the unrelated section citation.
7. `hir::declaration_label` supplies the shared-name module suffix and the prelude origin at every label site.
   Checker type and declaration messages, reload labels, and C allocation metadata call it.
   Its unit test covers unique names, shared names, and an empty prelude origin.
   `hir::source_name` is shared by checker messages and lowering errors, with a direct unit test.
8. The write-only `Checker::enum_ids` table, its initialization, and its insertion are removed.

### Measurements and files

- Compiler identity tests: 19 passed, 0.02 seconds.
- Reject sweep: five tests passed, 0.34 seconds.
  The 250 selective checks cost 135.787 ms; the conditional complete-mirror checks cost 27.145 ms.
  The previous complete-mirror cost was 3.211 seconds.
- Codegen identity tests: seven passed, 1.38 seconds with one test thread.
  The total declaration test costs 843.940 ms, including the two required C builds for ordinary code and workers.
- No existing golden or LIR snapshot line changed. No Rust file needs a size split.
- Changed compiler files: `src/divergence.rs`, `src/check/{identity,mod,pipeline,declarations,type_rules}.rs`,
  `src/check/expr/{call,namespace}.rs`, `src/hir.rs`, and the new `src/hir/names.rs`.
- Changed codegen files: `src/lir/{expr,place}.rs`, `src/reload.rs`, and `src/cemit.rs`.
- Changed tests: `compiler/tests/{module_global_names,retired_reasons}.rs` and `codegen/tests/module_global_names.rs`.
- The owner edits to C14 and the S017 row remain untouched. No commit or git write command ran.

### Open

A typed mirror constant still has no lowering storage. This defect is outside §125 and also exists at the handoff pin.
With `declare const K: i32;`, ``export function main(): void { print(`${K}`); }`` passes the checker.
Lowering reports ``main.ts:1:41: unknown global `K` ``. The error no longer exposes the internal declaration symbol.
The regression test pins ``unknown global `K` ``; it does not implement mirror storage.

The pinned formatter and the warning-free all-target build pass. The final full gate follows once, without concurrent work.

### Second Phase Review full gate

The full gate ran exactly once and passed. No edit or other build ran during it.
The gate includes hygiene and the Clippy baseline check. No existing golden or LIR snapshot changed.
Record: `target/gate/20260929T013602Z-full.md`.

```text
gate full 76f5225bc029b4d9fab578322773483a0e9b28ad dirty:82 debug 1884/0/3 release 1881/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```
