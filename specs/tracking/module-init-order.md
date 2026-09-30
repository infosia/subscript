# Module initialization order: Step 0 measurement

Status: **measurement round, 2026-09-30.** The round built a prototype,
measured it, and reverted every production change. Nothing landed.

The prototype contradicts two rules:

- `compiler.md` §128 rule 8: module globals initialize in file order
  (discovery order).
- `collisions.md` C18, the "Initialization order" paragraph.

## Method

- Base: `main` at `2d86edb7`, macOS arm64, release profile.
- The prototype read an environment variable, so one binary ran three
  orders: `bfs` (the current file order), `post` (dependency
  post-order over every edge), and `post-value` (post-order over value
  edges; a module that only `import type` declarations reach does not
  run).
- Probe programs lived under `$TMPDIR`. A temporary test
  (`codegen/tests/proto_init.rs`, deleted) ran each probe on the
  interpreter, the dev JIT, and the ship C tier.
- `node` v24.18.0 ran each probe two ways: the corpus runner
  (`corpus/node/run-js-corpus.cjs`: `ts.transpileModule` to CommonJS),
  and native ESM with type stripping (`--input-type=module`, specifiers
  rewritten to `./x.ts`, `print` bound to `console.log`).
- The effect run was `cargo test --offline --workspace --release
  --no-fail-fast`, once under `post` and once under `post-value`.
  `tools/gate.sh` did not run.

## 1. Where the order comes from

| Engine | Source of the order |
|---|---|
| Checker | Pass C (`check_bodies`) runs per file in file order. It appends `top_level` statements and `globals`. `module_data_bindings` reads the file order for the initializer-order check. |
| LIR | `lir/lowering.rs` builds one function, `<module initializer>`, from `globals` and `top_level` ordered by `initializer_index`. |
| Interpreter | Runs `lir.initializer`. No own order. |
| Dev JIT | `lower/func.rs` `define_init` lowers the LIR initializer. No own order. |
| Ship C | `cemit/emitter.rs` emits `subscript_init` from the LIR initializer. No own order. |
| Workers | `subscript_rt_worker_init` calls `subscript_init`, the same function. |
| Hot reload | `subscript_init` runs once when the session starts. A swap never runs it again (`reload.rs`). |

The order is one fact, set in the checker and read by the LIR
lowering. Every engine consumes the one LIR function.

A separate order is possible. The prototype kept the file order, the
`globals` vector, the module identities, and the emitted declaration
order. It added one HIR field, `initializer_segments`: per module, a
range of `top_level` and a list of `globals` indices, in run order. The
LIR lowering walks the segments. An empty field gives the old order.

## 2. Prototype

- Edges: every `import` declaration, every `export { .. } from`, and
  every `export * from`, in source order. An edge to a mirror
  (`.d.ts`) is ignored.
- Order: node's ESM evaluation order. A depth-first search from the
  entry marks a module when it enters it. It appends the module after
  its last dependency returns. An edge to a marked module is skipped,
  so a module in a cycle runs when the search completes it. Files that
  the entry does not reach follow, as roots, in file order.
- Type-only edge, `post`: an `import type` edge is an ordinary edge. A
  module that only type-only imports reach still runs, at its
  post-order position.
- Type-only edge, `post-value`: the search follows value edges only. A
  module that the entry reaches through any edge, but not through a
  value edge, does not run. Only the declaration form `import type`
  is a type-only edge. `import { type A }` is a value edge, because
  native ESM type stripping keeps it as `import {} from` (measured,
  probe p4).
- The initializer-order check (`module_initializer_diagnostics`) reads
  the run order in place of the file order.
- Regular-expression literal globals (`__subscript_regex_literal_N`)
  run first, before every module segment. See finding 6b.

## 3. Difficulty

Lines of the prototype (`git diff --stat` and the new file):

| File | Lines |
|---|---|
| `compiler/src/check/init_order.rs` (new) | 121, of which about 25 are the environment switch and the unused `bfs` path |
| `compiler/src/check/pipeline.rs` | +49 (segment record, order call, segment build, 4 lines of timing) |
| `compiler/src/check/mod.rs` | +6 / -4 (the check reads the run order) |
| `compiler/src/hir.rs` | +12 (field and segment type) |
| `compiler/src/hir/tests.rs` | +2 (struct literals) |
| `codegen/src/lir/lowering.rs` | +24 / -6 (walk the segments) |

Per engine: interpreter 0, dev JIT 0, ship C 0, workers 0, hot reload
0, watch 0. The CLI loader and the corpus reader keep
`discover_module_sources` unchanged.

## 4. Cost

Compile time. A synthetic program of 401 modules (a chain where
module `i` imports modules `i+1` and `i+2`, 799 edges), `subscript
check`, release:

| Order computation | Time per call |
|---|---|
| First version, linear file search per edge | 370–381 µs |
| Stem map, O(modules + edges) | 118–161 µs |

The check runs `run_with_effects` twice, so the order runs twice. The
whole check takes 0.03 s wall time. A program of 3 or 4 files takes
2–3 µs per call. One computation, shared by the two runs, halves the
cost.

Run time: zero. The emitted C of the prototype differs from the base
only in the order of the statements inside `subscript_init` and the
order of the matching position-table rows. Measured with `subscript
emit`: `a288` 0 lines differ; probe p3 8 lines differ (the two string
literals and the two position rows swap). No instruction, call, or
runtime path is added.

## 5. Effect

### Workspace test run

| Run | Passed | Failed | Wall time |
|---|---|---|---|
| `post` | 2,015 | 3 | 6 min 59 s |
| `post-value` | 2,015 | 3 | 5 min 10 s |

One failure is the temporary probe test without its environment. The
two others move under both orders:

- `compiler/tests/module_global_names.rs`
  `initializer_routes_disambiguate_same_name_functions`
- `compiler/tests/module_global_names.rs`
  `initializer_routes_disambiguate_same_name_constructors`

Each expects S100 for `main.ts` that reads, through a call, a global of
`lib.ts` that it imports. Under post-order `lib.ts` initializes first,
so the program is valid and checks clean. The tests pin label
disambiguation. A shape that stays invalid under post-order (a read
across a cycle) keeps that purpose.

No other test moved: no golden (`jit_ship_c_aot_and_golden_agree_byte_for_byte`),
no LIR snapshot (`coroutine_and_measurement_lir_text_matches_goldens`),
no emitted C test, no reject entry, no warn or trap entry, and no `node`
comparison (`every_accept_entry_has_a_total_js_claim_and_comparable_output_matches`).
The CLI test `cli_and_corpus_keep_import_initialization_order` passed:
sibling imports keep their source order.

The 19 multi-module accept entries do not print from a module
initializer, so no entry output changed, and no entry moved into
agreement with `node`. The 17 `js-comparable` entries agree with `node`
under all three orders. `a143` and `a297` are not comparable for C2 and
C8, not for the order.

### Probes

Each probe prints from module initializers. The three engines agreed
with each other on every probe under every order.

| Probe | Shape | `bfs` | `post` | `post-value` | node ESM | node corpus runner |
|---|---|---|---|---|---|---|
| p1 | the C18 program: `import type { A } from "./a"`, `import { vb } from "./b"` | `a init / b init / 2` | `a init / b init / 2` | `b init / 2` | `b init / 2` | `b init / 2` |
| p2 | main → b → a, main → c; main and b read imported globals in initializers | S100 twice (`vb`, `va` accessed before declaration) | `a / b / c / main / 5` | same as `post` | same as `post` | same as `post` |
| p3 | cycle main → x ⇄ y; x never uses `fy` | `main / x / y / 1` | `y / x / main / 1` | same as `post` | same as `post` | `x / main / 1` |
| p4 | `import { type A }`, a class used only as a type, an unused value import | `a / b / c / true` | same | same | same | `true` |
| p5 | main's global initializer calls a function of b that uses a regex literal | `false` | `true` | `true` | `true` | `true` |
| p6 | one file: a global initializer calls a later function that uses a regex literal | `false` | `true` | `true` | `true` | `true` |

## 6. Findings

a. The file order rejects valid programs. The entry is always first in
   discovery order, so an entry global initializer that reads an
   imported global, directly or through a call, is S100 (p2, and the
   two tests above). `tsc` and `node` accept p2.

b. A regex literal global initializes at the position where the checker
   meets the literal, not before its first use. p5 and p6 print `false`
   on all three engines under the file order, and `node` prints `true`.
   The differential gate cannot see this defect, because the three
   engines share it. The prototype runs these globals first, which
   fixes p5 and p6. This defect is independent of the module order.

c. The corpus runner is not native ESM. `ts.transpileModule` elides an
   import whose names the file uses only as types or not at all (p3,
   p4). Post-order agrees with native ESM on all six probes, and
   `post-value` agrees with the corpus runner only where no import is
   elided. An entry that prints from a module initializer and imports
   a name that it does not use as a value still disagrees with the
   runner under either order.

d. Nothing makes post-order hard. The order is computed once in the
   checker and every engine reads one LIR function. The costs are the
   HIR field, the segment walk in the LIR lowering, the two moved
   tests, and the regex hoist of finding b.

e. `post-value` leaves some globals of the program uninitialized. The
   checker already rejects a value use of a type-only import (§134),
   so no value path reaches them in the measured programs.

## Files the prototype touched (all reverted)

- `compiler/src/check/init_order.rs` (new, deleted)
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/hir.rs`
- `compiler/src/hir/tests.rs`
- `codegen/src/lir/lowering.rs`
- `codegen/tests/proto_init.rs` (new, deleted)

## Implementation

Red at the contract pin: `cargo run --offline -q -p subscript-cli -- check corpus/accept/a299-module-post-order/main.ts` exited 1.
It reported S100 for `vb` in `main.ts:9:21` and `va` in `b.ts:8:24`.
The report ended with `error: 2 error(s)`.
The node corpus runner returned `610a620a630a6d61696e0a350a` for `a299` and `6220696e69740a320a` for `a300`.

The implementation stores module ranges in `hir::Module::initializer_segments`.
The checker derives value dependency post-order and uses it for the initializer diagnostic.
The LIR lowering reads the segments for the one initializer function.
The accept entries are `a299` and `a300`.
The rule tests cover a cycle, an early read, a type specifier import, and sibling order.
The route tests use a cycle and retain the route labels.

Files: `compiler/src/check/init_order.rs`, `compiler/src/check/pipeline.rs`,
`compiler/src/check/mod.rs`, `compiler/src/hir.rs`, `compiler/src/hir/tests.rs`,
`codegen/src/lir/lowering.rs`, `compiler/tests/module_global_names.rs`,
`codegen/tests/module_init_order.rs`, `corpus/accept/a299-module-post-order/`,
`corpus/accept/a299-module-post-order.expected`, `corpus/accept/a300-type-only-module/`,
`corpus/accept/a300-type-only-module.expected`, and `specs/blocks/collisions.md`.

The first full gate at `a906f990` exited 1 with two failures in both profiles.
The LIR fact test found an allocation and a call from `a300` in HIR but absent from LIR.
The generated reference test found that the corpus index did not include the new entries.
The reference generator updated `generated-docs/corpus-index.md`.

Rule 3a makes `hir::Global::init` optional. The checker still checks each
source initializer, then removes the initializers and top-level statements
of modules without a value path from the entry. It keeps global storage.
The HIR visitor, capture check, layout check, and raise-site check skip
an absent initializer. The LIR lowering skips it too.
The unit test checks an allocating global and a top-level print in a
type-only module. The interpreter, dev JIT, and ship C tier print no bytes.
The two prior failing tests pass individually after these changes.

Additional compiler files: `compiler/src/check/bodies.rs`,
`compiler/src/check/expr/literal.rs`, `compiler/src/hir/definitions.rs`,
`compiler/src/check/capture.rs`, `compiler/src/check/layout.rs`,
`compiler/src/raise_sites.rs`, and `compiler/src/tests/language.rs`.
Additional codegen consumer: `codegen/tests/support/lir_facts.rs`.
The full gate at `8292614c` first exited 1 in the build step because
`compiler/tests/corpus_accept.rs` read `Global::init` as a required expression.
That compiler test now reads the optional initializer.

## Rule 3c: missing module identity fact

Command: `cargo test --offline -p subscript-codegen --test reload module_run_ -- --nocapture`.
The test ran against the current implementation before any production fix.
The build emitted no warnings. The command exited 101.

```text
running 2 tests
a changed empty-module run order must be refused: ()
test empty_module_run_order_change_is_refused ... FAILED
a changed module run set must be refused: ()
test module_run_set_change_is_refused ... FAILED
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 34 filtered out
```

Both accepted control swaps passed before the refusal assertions failed.
The run-set test uses the class, string global, and array global from the handoff.
The run-order test swaps two side-effect imports of empty modules.
The two checked forms have equal `initializer_segments` despite their different module run orders.

`InitializerSegment` carries a statement range and global indices, but no source module identity.
`hir::Module` carries no separate ordered list of source modules that run.
Rule 3c requires this list even when each module has no initializer work.
The checker must carry stable module identities in run order, including empty modules.
The reload consumer must compare this fact before it accepts a swap.

Work stops under `CLAUDE.md`, Core principles, rule 8: the consumer needs a fact that the form does not carry.
No production fix, rule 5a corpus measurement, documentation generation, hygiene run, or full gate ran after this finding.
The new regression tests remain Red.

## Contract form and fixes

Rule 4 at `1879737c` supplies the missing form.
`Module::initializer_modules` carries source module identities in run order, including empty modules.
Each identity has one initializer segment at the same index.
The declaration hash compares this list and names both lists when a swap changes it.
The checker computes the order once for both check runs.

`Module::regex_literal_globals` carries the indices of all regex literal globals.
These globals have initializer index zero and do not belong to any module segment.
The checker retains them even when their first instantiation occurs in a module that does not run.
The LIR lowering runs their initializers before the module segments.
The lowering reads the checker segments without an empty-list fallback.

The initializer-order check walks global initializers and top-level statements in execution order.
Each operation uses the same function-route summaries and the set of initialized bindings.
A module that does not run still checks its own source order.
The cycle fixture has no invalid entry export and asserts exactly one diagnostic.

### Red evidence before the fixes

Command: `cargo test --offline -p subscript-codegen --test module_init_order -- --nocapture`.
Exit: 101. Result: 6 passed, 6 failed. Time: 0.98 seconds.

| Rule | Test | Red output |
|---|---|---|
| 3b | `regex_from_type_only_generic_instantiation_runs_first` | Interpreter, dev JIT, and ship C: `false\n`; expected `true\n`. |
| 3b | `regex_from_later_generic_instantiation_runs_first` | Interpreter, dev JIT, and ship C: `false\n`; expected `true\n`. |
| 3b | `regex_in_later_function_runs_before_global_initializer` | Interpreter, dev JIT, and ship C: `false\n`; expected `true\n`. |
| 5 | `type_only_module_checks_initializer_order` | `type-only module must check source order`: the checker returned `Ok`. |
| 5a | `top_level_call_before_global_initializer_is_s100` | `top-level call reads an uninitialized global`: the checker returned `Ok`. |
| 5a | `cycle_top_level_call_before_global_initializer_is_s100` | `cycle call reads an uninitialized global`: the checker returned `Ok`. |

The after-initializer control printed `1\n` on all three engines before and after the fix.
The rule 3c Red tests and accepted controls are recorded above.
The empty-module test also asserts the different ordered identity lists after the fix.

Command: `cargo test --offline -p subscript-cli --test watch module_run_set_refusal -- --nocapture`.
Exit: 101. Result: 0 passed, 1 failed.

```text
Error: "expected run-set refusal, got Swapped(WatchCall { output: [114, 101, 102, 117, 115, 101, 100, 10], trap: None })"
test module_run_set_refusal_keeps_watch_session_live ... FAILED
```

The watch test now refuses the run-set change and then accepts a body-only control swap.
The codegen initializer tests pass: 12 passed, 0 failed.
The complete reload target passes: 36 passed, 0 failed.

### Corpus and example measurement

Command: `cargo test --offline -p subscript-codegen --test module_init_order_measurement -- --nocapture`.
The test checks all accept programs, warning sources, trap sources, and non-ambient example sources.
Directory accept programs include every reachable source and their ambient mirrors.
The test checks every program before it reports all rejected programs together.
It executes no script code.

```text
rule 5a: accept=291, warn=5, trap=71, examples=17, rejected=0
```

Result: 1 passed, 0 failed. Time: 1.75 seconds.
No source triggers the contract stop condition.
The test cost is one checker run per program; it proves rule 5a preserves corpus and example acceptance.

The new accept entry is `a301-regex-before-module`, with its `true\n` golden.
The three engines reproduce this golden.
The node corpus runner reports `0\tok\t747275650a` with node v24.18.0 and TypeScript 5.9.2.
The corpus test tables derive their entry set from files; no fixed accept table needs an update.
The C18 Accept line includes `a301`.
The reference generator updates `generated-docs/corpus-index.md`.

The complete measurement includes 415 non-ambient source files in 384 programs.
The repeat after the final regex form change reports zero rejected programs in 1.65 seconds.
The pinned `cargo fmt --check` passes.
`cargo build --offline --locked --workspace --all-targets` passes with zero build warnings.
The final preflight Clippy run emits no warnings for the new initializer and reload tests.
The `needless_range_loop` warning in the pipeline is absent.
No contract gap remains in the named findings.

### Changed files

The list includes the initial working-tree implementation and this fix.

```text
cli/tests/watch.rs
codegen/src/lir/lowering.rs
codegen/src/reload.rs
codegen/tests/module_init_order.rs
codegen/tests/module_init_order_measurement.rs
codegen/tests/module_global_names.rs
codegen/tests/reload.rs
codegen/tests/support/lir_facts.rs
compiler/src/check/bodies.rs
compiler/src/check/capture.rs
compiler/src/check/expr/literal.rs
compiler/src/check/init_order.rs
compiler/src/check/layout.rs
compiler/src/check/mod.rs
compiler/src/check/pipeline.rs
compiler/src/hir.rs
compiler/src/hir/definitions.rs
compiler/src/hir/tests.rs
compiler/src/raise_sites.rs
compiler/src/tests/language.rs
compiler/tests/corpus_accept.rs
compiler/tests/module_global_names.rs
corpus/accept/a299-module-post-order.expected
corpus/accept/a299-module-post-order/a.ts
corpus/accept/a299-module-post-order/b.ts
corpus/accept/a299-module-post-order/c.ts
corpus/accept/a299-module-post-order/main.ts
corpus/accept/a300-type-only-module.expected
corpus/accept/a300-type-only-module/a.ts
corpus/accept/a300-type-only-module/b.ts
corpus/accept/a300-type-only-module/main.ts
corpus/accept/a301-regex-before-module.expected
corpus/accept/a301-regex-before-module.ts
generated-docs/corpus-index.md
specs/blocks/collisions.md
specs/tracking/module-init-order.md
```


## Gate failure fixes

The preceding full gate ran once and reported:

```text
gate full 1879737c42d3023b87ab3dd7bf809a7f2fc2a8ba dirty:30 debug 2032/4/3 release 2028/5/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 1
```

Three reload unit tests expected declaration lists without the module run order item.
Their expected lists now include `module run order/set ["live.ts"]`.
The assertions still compare the complete declaration lists.

The nominal-type fixture changed the run set from `first.ts` to `second.ts` when it changed the class import.
The fixture now imports both modules for side effects before the class import.
Every revision asserts the same ordered run identities: `first.ts`, `second.ts`, `main.ts`.
The assertions still name `function read`, `variable value`, and `class Holder` for their respective type changes.
The body-only control still preserves the hash.

### Temporary directory collision

The release gate failed in `the_annotated_bare_map_forms_measure_their_recorded_tsc_class` with `File exists (os error 17)`.
The failure site is `TempProjectDirectory::create` in `compiler/tests/tsc_corpus.rs`.
The helper exists at HEAD and this task does not change it.
It names a directory with the process ID and `SystemTime::now().as_nanos()`.
Multiple tests in the same binary call the helper; the name has no thread identity or atomic sequence.
The repository search finds this prefix only in `compiler/tests/tsc_corpus.rs`.
The new initializer test code defines no temporary-directory name or directory-creation helper.
The watch helper uses a separate prefix and an atomic sequence.
The new watch test does not call that helper.

A probe under `$TMPDIR` uses the same name construction and `fs::create_dir` under its own parent directory.
The pinned Rust compiler builds the probe with `-O`.
The probe runs eight synchronized threads, with 1,000 attempts per thread.

```text
temp-name probe: threads=8, attempts=8000, already-exists=1346
```

The probe reproduces a collision within the existing helper's naming method.
No new test shares its directory names.
The failed gate does not record which other test created the same directory.
`compiler/tests/tsc_corpus.rs` is outside the handoff file set, so its naming method stays unchanged.
The next full gate retains the risk of this existing collision.

The fix changes `codegen/src/reload.rs`, `codegen/tests/module_global_names.rs`, and this tracking note.

Targeted checks pass: 25 reload unit tests and the nominal-type diagnostic test.
The pinned `cargo fmt --check` passes.
The all-target workspace build passes with zero warnings.
`tools/hygiene.sh` exits 0.


The additional full gate ran once and reported:

```text
gate full 1879737c42d3023b87ab3dd7bf809a7f2fc2a8ba dirty:31 debug 2036/0/3 release 2033/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

Record: `target/gate/20260930T133831Z-full.md`.
Both test profiles pass the three declaration-list tests and the nominal-type diagnostic test.
Both profiles also pass the rule 5a corpus and example measurement.
The temporary-directory collision does not recur in this gate.
The pinned format check, zero-warning workspace build, tsc check, and hygiene check pass.
The Clippy counts stay within the standing baselines. No existing golden changes.
The existing temporary-directory naming method remains outside the authorized file set.


## Second Phase Review fix round at 9abb178a

The round reads the amended contract and the second fix handoff.
The tests precede the production changes. The round stops at the rule 5a
measurement condition below. It does not change corpus sources to evade
that condition and does not run the full gate.

### Red outputs before the fixes

`module_init_order` reports 11 passed and 4 failed:

```text
every_loaded_module_has_an_initializer_segment:
  left: ["main.ts"]
 right: ["main.ts", "empty.ts", "other.ts"]
global_outside_initializer_segments_is_a_lowering_error:
  an unowned global must fail lowering: Module { ... }
type_only_module_runs_initializers:
  left: ["", "", ""]
 right: ["hidden\n", "hidden\n", "hidden\n"]
json_type_only_class_methods_read_initialized_globals:
  interpreter: InvalidLir { message: "expected container, found Null", pos: None }
  dev JIT: AbnormalTermination { status: "dev-JIT child signal 11", stdout: [49, 10], stderr: [] }
  ship: AbnormalTermination { status: "linked C program exited with signal: 11 (SIGSEGV)", stdout: [49, 10], stderr: [] }
test result: FAILED. 11 passed; 4 failed; 0 ignored
```

The ownership Red adds a checked global from a separate program to the
base HIR. It leaves the base segment record unchanged. The omitted LIR
Debug text in the Red output contains the accepted module, not an error.
Each JSON engine prints `1` before its failure. Child processes isolate
all three engine failures so the test reports all of them.

`module_init_routes` reports 3 passed and 3 failed:

```text
using_dispose_route_rejects_early_read_and_runs_after_global:
  expected S100: `m` is accessed before its declaration, through `scope` -> `R.[[Symbol.dispose]]`; checker accepted the program
foreign_callback_rejects_early_read_and_runs_after_global:
  expected S100: `late` is accessed before its declaration, through an indirect call; checker accepted the program
async_indirect_call_rejects_later_global_and_runs_after_it:
  expected S100: `m` is accessed before its declaration, through an indirect call; checker accepted the program
test result: FAILED. 3 passed; 3 failed; 0 ignored
```

The other rule 5c shapes already reject before this round's fix:

```text
main.ts:1:77: `later` is accessed before its declaration, through an indirect call [S100]
main.ts:1:24: `later` is accessed before its declaration, through an indirect call [S100]
main.ts:1:59: `later` is accessed before its declaration, through `apply` -> an indirect call [S100]
```

These are the forEach, stored function, and apply-lambda shapes,
respectively. Their global-first controls already run on all three
engines. The new assertions measure existing behavior; these tests have
no failing Red at the start of this round.

The two codegen reload refusal tests and the watch refusal test also
pass before the production fixes. The existing diagnostic already names
both lists. This round replaces substring checks with full text checks.
Under rule 3, a type-to-value import change alone does not change the run
set. The run-set fixture instead removes the imported empty module.

```text
reload refused: declaration `module run order/set ["main.ts"] (was module run order/set ["a.ts", "main.ts"])` changed; only function bodies can be hot-swapped
reload refused: declaration `module run order/set ["b.ts", "a.ts", "main.ts"] (was module run order/set ["a.ts", "b.ts", "main.ts"])` changed; only function bodies can be hot-swapped
```

### Changes and targeted validation before the stop

All imports, including `import type`, contribute dependency edges.
After the entry traversal, the traversal visits every other loaded
non-mirror module. The HIR keeps every module identity and segment.
`Global.init` is again an `Expr`. No module work is dropped.

The following rule 3a consumer files return byte-for-byte to HEAD:

- `compiler/src/check/bodies.rs`
- `compiler/src/hir/definitions.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/layout.rs`
- `compiler/src/raise_sites.rs`
- `compiler/src/tests/language.rs`
- `codegen/tests/support/lir_facts.rs`
- `compiler/tests/corpus_accept.rs`

`compiler/src/check/expr/literal.rs` keeps only the regex-before-module
change. `codegen/src/lir/lowering.rs` uses required initializers and
checks that each global has exactly one owner across module segments
and regex literal globals. The orphan error is:

```text
global `orphan` must have exactly one initializer owner; found 0
```

The route scan moves to `compiler/src/check/init_effects.rs`. Statement,
callee, and expression matches enumerate their HIR variants. Dispose
hooks run through their checked call routes. Foreign, async-start, and
generator calls add the indirect-call effect. Calls of function values
and built-in callbacks retain that effect; the apply-lambda route
retains its named direct step before the indirect call.

After these changes, the 15 module-order tests and 6 route tests pass.
The type-only JSON test prints `a init / 1 / hello / 3` on all three
engines. The six rejected route shapes assert the complete S100 message
and have a global-first firing control. The foreign control runs on the
JIT and C tier with the native fixture; the other controls run on all
three engines.

Pinned `cargo +1.95.0 fmt --all` completes. The targeted tests and the
compiler/codegen library build complete with zero build warnings.
These do not substitute for the unrun final checks.

The permanent `codegen/tests/module_init_order_measurement.rs` file is
deleted. The round compiles a temporary harness against the built
libraries, runs it once, and deletes the harness and its executable.

### Measurement stop

```text
rule 5a: accept=291, warn=5, trap=71, examples=17, source-files=415, rejected=2
test result: FAILED. 0 passed; 1 failed; 0 ignored
```

The 384 programs produce these diagnostics:

```text
a161-counted-handle-stores.ts:11:34: `globalHandle` is accessed before its declaration, through an indirect call [S100]
t24-stale-coroutine-reload.ts:16:28: `live` is accessed before its declaration, through an indirect call [S100]
```

The first source declares `let globalHandle: Promise<i32> = storedWork(90)`.
The second declares `let live: Generator<i32> = counting()`.
Rule 5b gives both calls an effect that reads every global, including the
binding whose initializer has not finished. Neither callee actually
reads that binding. The rejection is the precision gap of 137.3; it now
has two corpus examples, so the contract's earlier zero-rejection
measurement does not hold for the total scan.

The handoff and acceptance 5 require the round to stop and report any
rejected corpus or example source. No source or test is changed after
this measurement. Only this tracking record is updated.

Pending at the stop: the a300 three-engine golden update, header
canonicalization, generated-docs regeneration, pinned `cargo fmt
--check`, the all-target zero-warning build, the changed-file Clippy
check, hygiene, and the full gate. The a300 headers were changed before
the measurement to mark the C18 divergence; generated-docs is still the
previous round's output, and its golden has not been regenerated.
There is no gate verdict line for this round because no gate runs.

### Files at the stop

The working tree includes changes from the previous round as well as
this round. Its changed files are:

- `cli/tests/watch.rs`
- `codegen/src/lir/lowering.rs`
- `codegen/src/reload.rs`
- `codegen/tests/module_global_names.rs`
- `codegen/tests/reload.rs`
- `codegen/tests/module_init_order.rs` (new)
- `codegen/tests/module_init_routes.rs` (new)
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/init_order.rs` (new)
- `compiler/src/check/init_effects.rs` (new)
- `compiler/src/hir.rs`
- `compiler/src/hir/tests.rs`
- `compiler/tests/module_global_names.rs`
- `corpus/accept/a299-module-post-order/{main,a,b,c}.ts` and its `.expected` (new)
- `corpus/accept/a300-type-only-module/{main,a,b}.ts` and its `.expected` (new)
- `corpus/accept/a301-regex-before-module.ts` and its `.expected` (new)
- `generated-docs/corpus-index.md` (previous generated output)
- `specs/blocks/collisions.md` (owner paragraph and previous Accept id line)
- `specs/tracking/module-init-order.md`

The restored consumer files above have no diff from HEAD. The deleted
measurement file was untracked, so it no longer appears in git status.
The owner-written C18 paragraph is unchanged in this round. No git write
or commit is made.


## Contract correction and completion round at df311d40

The owner corrects rule 5b after the measurement stop and authorizes
this round. The round reads `git show HEAD` and follows the corrected
async and generator rules. The earlier stop record remains historical.

### Tests before the production correction

The route target has 12 tests. Before the correction, 7 pass and 5 fail:

```text
declared_async_call_does_not_read_unrelated_globals:
  `later` is accessed before its declaration, through an indirect call [S100]
declared_generator_creation_does_not_run_its_body:
  `live` is accessed before its declaration, through an indirect call [S100]
traced_generator_step_reads_only_its_body_globals:
  `live` is accessed before its declaration, through an indirect call [S100]
  `later` is accessed before its declaration, through an indirect call [S100]
traced_generator_next_rejects_body_read_and_runs_after_global:
  left: 2 diagnostics
 right: 1 diagnostic
  `live` is accessed before its declaration, through an indirect call [S100]
  `later` is accessed before its declaration, through an indirect call [S100]
untraced_generator_step_keeps_indirect_call_effect:
  left: "`later` is accessed before its declaration, through an indirect call"
 right: "`later` is accessed before its declaration, through `step` -> an indirect call"
test result: FAILED. 7 passed; 5 failed; 0 ignored
```

The async rule 5c test now awaits `Context.suspend()` before it reads
`m`. The global-first control prints `1` on all three engines. Its
complete diagnostic is already present before the correction:

```text
main.ts:1:80: `m` is accessed before its declaration, through `go` [S100]
```

The for-of rejection test also already passes before the correction:

```text
main.ts:1:64: `later` is accessed before its declaration, through `values` [S100]
```

The old scan reads the generator body at creation, so that rejection
alone does not distinguish creation from a step. The creation control
and the step-without-global-read control above fail before the fix and
pin the distinction. The unknown-generator test passes a generator to
an ordinary function parameter; that parameter has no traced origin.
All new rejection tests have a global-first firing control in the same
shape on all three engines.

### Corrected scan

Declared async functions and methods have direct call routes through
their entire bodies, including statements after an await. Creating a
held async frame follows that same body; awaiting it does not add a
second unknown-call effect. The async route names `go`, as required by
the corrected direct-call rule 5b. The general indirect-call wording
of rule 5c applies to the function-value and built-in callback cases.

Declared generator calls do not add body effects. Their arguments
still scan as expressions. A generator step traces its receiver through
global or local initializers and aliases to a declared generator
function or method and adds that body's route. The HIR for a generator
for-of contains the checked subject binding and a `next` call, so it
uses the same rule. A missing origin uses the indirect-call effect.
An assigned binding, a repeated local name, or an origin cycle loses
its origin. Function parameters and storage through fields are not
traced and retain the conservative indirect effect of rule 5b.

The 15 module-order tests and 12 route tests pass after the correction.
Codegen reload and module-name tests, compiler module-name tests, and
all 12 watch tests pass. The nominal-type reload test retains its exact
function-read assertion and its equal run-order fixture.

### Repeated measurement and golden

A temporary harness measures all sources once in this round:

```text
corpus/accept/a161-counted-handle-stores: accepted
corpus/trap/t24-stale-coroutine-reload: accepted
rule 5a/5b: accept=291, warn=5, trap=71, examples=17, source-files=415, rejected=0
```

There are 384 programs. No measurement stop condition fires. The
harness is deleted after execution. The permanent measurement test
remains deleted.

The first temporary a300 run cannot find the runtime static library
beside the temporary executable. This is a probe configuration error,
not a source rejection. A separate temporary golden probe sets
`SUBSCRIPT_RUNTIME_STATICLIB` to the existing built library. It does
not repeat the corpus measurement. It verifies equal bytes from the
interpreter, JIT, and C tier before writing the new a300 golden:

```text
a300 interpreter/JIT/C: "a init\nb init\n2\n"
```

The a300 headers use `js-comparable: no C18: Type-only imports run
module initializers.` The generated-docs generator runs and writes
all three derived documents; only `corpus-index.md` has a diff.
No existing tracked golden changes. The C18 paragraph written by the
owner is unchanged. The previous Accept id additions remain.

### Pre-gate checks

Pinned `cargo +1.95.0 fmt --check` passes. The locked, offline all-target
workspace build passes with zero warnings. The all-target workspace
Clippy run has zero warnings in changed files. Its library warning
counts are compiler/runtime/codegen = 3/18/13, the same as the previous
successful gate. No new Clippy warning is suppressed.
The route source has fewer than 2,000 lines. The rule 3a consumers
listed in the preceding round remain at their HEAD text.

This round changes `compiler/src/check/init_effects.rs`,
`codegen/tests/module_init_routes.rs`, the three a300 source headers,
the a300 golden, `generated-docs/corpus-index.md`, and this note.
The complete working-tree file list in the preceding round still
applies. No commit or git write is made. The indirect-call precision
gap of 137.3 remains open; resolving lambda literals or general stored
function values is not part of this correction.


### Full gate verdict

The pre-gate hygiene check exits 0. The full gate runs exactly once in
this completion round and reports:

```text
gate full df311d40279b4e1cd1ea9a686536d263eb3703fc dirty:24 debug 2050/0/3 release 2047/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

Record: `target/gate/20260930T151238Z-full.md`.
The debug and release suites pass the module-order and route tests,
the nominal-type reload diagnostic test, and the existing bare-map
measurement test. The temporary-directory collision does not recur.
The gate's pinned format check, zero-warning build, Clippy baseline,
tsc check, and hygiene check pass. No existing golden moves.
The round updates only this record after the gate; it makes no further
source change and does not repeat the gate.


## Third Phase Review fix round at 2a582028

The round reads the third handoff and `git show HEAD`. Tests precede
production changes. The generator route target reports these Reds:

```text
generator_creation_scans_body_before_any_step:
  expected S100: `later` is accessed before its declaration, through `values`; checker accepted the program
shadowed_generator_parameter_cannot_hide_body_read:
  expected S100: `m` is accessed before its declaration, through `bad`; checker accepted the program
generator_parameter_for_of_reads_nothing_beyond_creation:
  `s` is accessed before its declaration, through `sum` -> an indirect call [S100]
generator_parameter_next_reads_nothing_beyond_creation:
  `early` is accessed before its declaration, through `step` -> an indirect call [S100]
test result: FAILED. 7 passed; 4 failed; 0 ignored
```

The module-order target has 17 passing tests before the production fix.
The new two-owner and missing-index tests already return their complete
lowering errors. Full-message S100 assertions also pass before the fix.
The JSON test now calls `all_engines` without an environment switch or
three extra invocations of its executable. The rejected-source helper
no longer prints diagnostics in passing tests.


The fallback unit tests also have Reds before their fix:

```text
unresolved_method_routes_read_every_global:
  left: None
 right: Some(["[indirect call]"])
recorded_call_without_summary_reads_every_global:
  left: Some(["existing", "route"])
 right: Some(["[indirect call]"])
test result: FAILED. 0 passed; 2 failed; 0 ignored
known_builtin_methods_add_no_callback_effect:
  push: {"later": ["[indirect call]"]}
test result: FAILED. 0 passed; 1 failed; 0 ignored
```

The last Red distinguishes a resolved built-in method from an unknown
receiver. Array push/pop and String slice are checked receiver methods;
the scanner uses the existing HIR operation target and matches every
`BuiltinMethod` variant explicitly. Generator next reads nothing more.
Every nested intrinsic kind also has an exhaustive callback match.

The new entry print makes a300 Red at the contract pin. A temporary
archive of `2d86edb7` runs the updated source on its interpreter:

```text
a300 pin 2d86edb7: "entry\na init\nb init\n2\n"
```

The expected post-order output starts with `a init`, then `b init`,
then `entry`, then `2`. The archive and probe are under `$TMPDIR`.
No checkout or git write is used.


### Third-review changes and validation

The scanner no longer has a generator origin map or local-name trace.
It reads declared generator bodies at creation, as it reads declared
async bodies. Resolved generator next adds no body effect. The shadowed
parameter shape is S100 through `bad`; its global-first control prints
`7`. The parameter-for-of shape prints `6` on all three engines.

The callback classification names every intrinsic kind and every
resolved receiver built-in kind. Unknown receivers, missing class ids,
and absent methods use an indirect call. A recorded call without a
summary reads every checked global through the indirect route.

A source program cannot produce an unresolved method in executable HIR:
`check/expr/call.rs` resolves a reference-class method through its class
signature, emits S018 and an error expression when it has no signature,
and emits S018 for other unsupported receiver types. Class ids come
from checker-owned class declarations. Resolved Array push/pop,
String slice, and Generator next have HIR operation targets. Unit tests
build unresolved receiver, missing-id, and missing-method forms directly
and test all three fallbacks. Another unit test builds a recorded call
without a summary and checks the effect on every supplied global.

The binding list now derives from `checker.globals` alone, excluding
regex literal globals and sorting by module run order and initializer
position. It does not reconstruct declarations from the AST. Synthetic
or checked globals cannot disappear because of an AST pattern filter.

The lowering ownership tests build a second owner in an empty loaded
module and an owner index outside an empty globals vector. Both checks
already existed and pass before this round's production changes:

```text
global `shared` must have exactly one initializer owner; found 2
initializer owner names missing global index 0
```

After the fixes, the compiler fallback target passes 3 tests; the
module-order target passes 17 tests and the route target passes 11.
All S100 assertions in the two new integration files compare the full
message. Passing reject tests do not print their diagnostic.

The two integration files state their measured warm debug execution
cost at the top: module-order 0.40 s with 5 ship-C program compiles;
routes 0.77 s with 11 ship-C program compiles. Those measurements exclude
the Cargo build. The order target removes two duplicate ship-C program
compiles: the all-loaded-module HIR ownership test uses the interpreter,
and the single-file a301 regex test uses the interpreter because the
standing corpus comparison already runs its JIT and C program. The
JSON test uses the common `all_engines` helper with no extra test-process
invocation. The remaining C controls check accepted runtime routes.

### Third-review measurement

A temporary harness measures all sources exactly once in this round:

```text
rule 5a/5b: accept=291, warn=5, trap=71, examples=17, source-files=415, rejected=0
a300 interpreter/JIT/C: "a init\nb init\nentry\n2\n"
```

The 384 programs have no rejection; the stop condition does not fire.
The harness, executable, and temporary pin archive are deleted. The
three-engine equality check writes the a300 golden. Its entry now
prints at top level; its header keeps the C18 non-comparable claim.
The generated-docs generator writes all three derived documents.
Only the corpus index retains a diff from HEAD. No existing tracked
golden changes.

The round changes `compiler/src/check/init_effects.rs`,
`codegen/tests/module_init_order.rs`, `codegen/tests/module_init_routes.rs`,
`corpus/accept/a300-type-only-module/main.ts`, its `.expected`, and this
note. It regenerates generated-docs; the complete working-tree file
list in the second-review record still applies. The owner C18 paragraph
is unchanged and no git write or commit is made.

The precision gap for function values and built-in callbacks remains.
The corrected contract also records the top-level block using defect
as a separate section: that block lacks `Stmt::Using` and does not run
its dispose hook. This round does not change that form or claim to fix
it. The generator creation rule conservatively reads the whole body
even when no step runs before a later global, as rule 5c states.


Pinned `cargo +1.95.0 fmt --check` passes. The final all-target locked,
offline workspace build passes with zero warnings. The final all-target
Clippy run reports no warnings in changed files; the library counts
remain compiler/runtime/codegen = 3/18/13. One new unit-test `useless_vec`
warning is corrected by using an array, without a lint suppression.
`git diff --check` passes. The new scanner source stays below 2,000 lines.


### Third-review full gate verdict

The pre-gate hygiene check exits 0. `tools/gate.sh full` runs exactly
once in this round and reports:

```text
gate full 2a582028758ff25ab134e982c1fc280af7282e4c dirty:24 debug 2054/0/3 release 2051/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

Record: `target/gate/20260930T160912Z-full.md`.
All gate steps exit 0. Both profiles pass the shadowed-generator,
parameter-for-of, unknown-call fallback, and two new ownership tests.
The existing bare-map measurement test passes in both profiles, and
its temporary-directory collision does not recur. Existing goldens
stay unchanged. Clippy remains within its standing library baselines.
Only this tracking record is updated after the gate; no further source
change or gate run is made.


## Fourth Phase Review form audit at 993b211f

The round reads the fourth fix handoff and `git show HEAD`, including
rules 5b and 5c, acceptance 8, and 137.3. The handoff has this stop
condition:

> If the HIR lacks a fact this form needs (for example a lambda identity), stop and report what the form must carry.

The current HIR lacks a lambda unit identity. `ExprKind::Lambda` in
`compiler/src/hir.rs` carries `params`, `ret`, `body`, `captures`, and
`can_raise`. The surrounding expression carries its type and source
position. It has no declaration symbol, lambda id, or index of a unit
in a HIR lambda table. `check/expr/lambda.rs` constructs those fields
without an identity. `Module.functions` contains free functions;
lambda bodies remain inline in expressions.

The LIR lambda function id is allocated later in
`codegen/src/lir/lambda.rs`. It is not a fact the initializer checker
can read. A declared function value has `FuncRef(Symbol)`, but a lambda
value has no corresponding unit reference.

Rule 5b now requires summaries of individual lambda bodies and a set
of the function values made along the walked route. The fixed point
must refer from a made lambda value to its specific summarized body.
A source position is a diagnostic location, not that identity: a
lambda in a generic body can have the same position in multiple
monomorphized bodies. Structural equality of a body also does not
identify a lambda unit. This round does not replace the missing HIR
fact with a name, a source position, a body hash, a memory address, or
an unrecorded traversal number.

The form must carry a lambda unit identity that is unique within the
checked program, remains associated with the unit when HIR expressions
are cloned, and distinguishes separate monomorphized units. A lambda
creation expression must carry that identity, with a lookup from it to
the body whose existing parameter, return, capture, and position facts
are read by the summary scan. The body can remain inline or be indexed
in a HIR unit table; the missing fact is the unit identity and its body
association. Declared function and method values can use their existing
checked declaration identities. The identity need not be stable across
hot-reload generations; this request concerns one checked program.

The round stops at the handoff's form condition. It does not change the
HIR or production code under an inferred identity rule, and it does
not proceed with the separate assertion and lowering-range fixes.
No new test or Red run is made. No corpus measurement is run. The
pinned format check, all-target build, Clippy, generated-docs, hygiene,
and full gate are not run in this stopped round. There is no new gate
verdict line. The previous successful verdict belongs to the third
round and is not a verdict for this contract.

The only file this round changes is this tracking note. All earlier
working-tree source changes remain in place. No git write or commit
is made. The missing lambda fact leaves acceptance 8 and the fourth
handoff unfinished; the existing fourth-review false rejections and
minor defects have not been fixed in this round.


## Fourth Phase Review continuation at 0101b2bb

The owner supplies the missing lambda identity in rule 5b. This round
resumes the fourth handoff. It reads `git show HEAD` and rules 5b, 5c,
acceptance 8, and 137.3. It makes no git write or commit.

### Red before the fixes

The tests run against the prior scanner and lowering. Each diagnostic
below quotes the complete message. The lowering tests also print the
accepted LIR in their panic output; the record omits that long dump.

```text
module_init_order: 17 passed; 3 failed
  a_global_position_outside_its_own_segment_is_a_lowering_error:
    position belongs to another segment: Module { ... }
  a_segment_past_the_module_body_is_a_lowering_error:
    segment has a missing statement: Module { ... }
  a_reversed_segment_is_a_lowering_error:
    reversed range: Module { ... }

module_init_values: 0 passed; 4 failed
  a_host_call_can_initialize_a_global:
    S100: `device` is accessed before its declaration, through an indirect call
  a_direct_body_can_call_a_host_to_initialize_a_global:
    S100: `first` is accessed before its declaration, through `poll` -> an indirect call
  a_comparator_in_a_dependency_does_not_read_the_importer:
    S100 at a.ts: `count` is accessed before its declaration, through an indirect call
  a_map_callback_can_initialize_its_global:
    S100: `ys` is accessed before its declaration, through an indirect call

module_init_routes: 10 passed; 6 failed
  a_later_lambda_is_not_followed_by_an_earlier_call:
    S100: `cb` is accessed before its declaration, through `apply` -> an indirect call
  for_each_indirect_call_rejects_later_global_and_runs_after_it:
    S100: `later` is accessed before its declaration, through an indirect call
  lambda_argument_indirect_call_rejects_later_global_and_runs_after_it:
    S100: `later` is accessed before its declaration, through `apply` -> an indirect call
  stored_function_indirect_call_rejects_later_global_and_runs_after_it:
    S100: `later` is accessed before its declaration, through an indirect call
  values_made_in_followed_bodies_reach_later_indirect_calls:
    S100: `later` is accessed before its declaration, through `apply` -> an indirect call
  a_registered_callback_remains_available_at_a_later_host_call:
    left count 3; right count 2
    S100: `device` is accessed before its declaration, through an indirect call
    S100: `late` is accessed before its declaration, through an indirect call
    S100: `late` is accessed before its declaration, through an indirect call

check::init_effects::tests::a_missing_descriptor_class_is_an_indirect_call:
  assertion left None; right Some(["[indirect call]"])
  0 passed; 1 failed

lambda_ids_are_unique_across_generic_instances:
  E0425: cannot find type `LambdaId` in module `subscript_compiler::hir` (two sites)
  E0026: variant `subscript_compiler::hir::ExprKind::Lambda` does not have a field named `id`
  could not compile the test due to 3 previous errors
```

The fixed-point refusal and the lambda-creation control pass before the
fix. The new accepted routes supply their firing controls. The host
registration control asserts both diagnostics: registration can call
the callback, and the later host call can call it again.

The function and constructor route tests pass with their complete
messages before the scanner change. The type-only invalid fixture
reports these two complete messages:

```text
type mismatch: the initializer expects `i32`, got `string`
type mismatch: the argument expects `string`, got `i32`
```

A first assertion probe used incorrect expected text and failed with
those messages. The fixture then uses that exact text. The first lambda
identity fixture captures a parameter and gives S009. A `const` copy
makes the fixture valid; its three IDs differ, and two positions match.
These fixture errors do not represent production defects.

### Implementation and focused checks

`hir::LambdaId` names a checked lambda unit. The checker allocates each
ID from a monotonic counter. Each generic instance has a separate ID.
The two constructor sites are `check/expr/lambda.rs` and `hir/tests.rs`.
All codegen lambda matches already ignore other fields; no codegen
lambda constructor exists. Only the initializer scan reads the ID in
production. The identity test reads it to assert the contract.

Each summary records ordered reads, direct calls, indirect calls, and
value creation. Lambda bodies have separate summaries keyed by ID.
Arguments and receivers enter the scan before their call. The scan
keeps declared function and lambda values across initializer segments.
An indirect call follows all those bodies until the set stops changing.
Recursive routes resume when their set grows. Async and generator calls
retain the whole-body rule. Built-in and statement matches stay total.
Dispose hooks retain their calls.

A missing descriptor class records an indirect call. Lowering rejects
an initializer outside its own segment, a missing statement, and a
reversed range. The scanner asserts these internal HIR bounds before
it walks a segment. No malformed record can silently skip its effect.

Focused checks pass: 20 initializer-order tests, 16 route tests, four
function-value tests, and four scanner unit tests. The lambda identity
test passes. The same-name function and constructor tests retain their
purposes and assert the error count and full message.

Warm debug costs exclude builds: order 0.58 s with five ship-C compiles;
routes 1.28 s with 14 compiles; values 0.71 s with four compiles. Each
native compile checks behavior that requires its engine or fixture.
The scanner, HIR, and checker files stay below 2,000 lines.

The method-value form is unreachable in successful HIR: the checker
rejects a method read as a value. Methods still have declaration-keyed
summaries for direct routes. Rule 137.3 retains its precision gap for
unrelated earlier function values. Top-level block disposal remains a
separate defect; this round does not claim to fix its missing form.


### Measurement stop

The temporary harness runs once. It reports:

```text
rule 5a/5b: accept=291, warn=5, trap=71, examples=17, source-files=415, rejected=1
REJECTED trap/t72-narrowing-boundary-getter:
  S016: unknown type name `SGPUProbeBlendState` (line 8, column 14)
  S016: unknown class `SGPUProbeBlendState` (line 8, column 47)
```

This is a harness input defect. The harness uses the accept loader for
all three categories. Its mirror detector omits `SGPUProbeBlendState`.
The standing `codegen/tests/support/trap_corpus.rs` loader supplies the
interop mirror when that type occurs. The type is declared in that
mirror. The measurement does not establish a compiler regression or
prove acceptance of the complete corpus input.

The round follows the handoff stop condition after the rejection. It
does not change the harness and repeat the measurement. It deletes the
harness, executable, and temporary sort sources. It does not run node
for the sort comparison, generated-docs, pinned `fmt --check`, the
all-target build, Clippy, hygiene, or the full gate. The pinned formatter
ran for the focused tests, and the compiler library build passed with
no warnings. These checks do not replace the pending pre-gate checks.

There is no gate verdict for this continuation. The earlier third-round
verdict is not a verdict for HEAD 0101b2bb. The production and test fixes
remain in the working tree. The corpus measurement with the trap
loader's complete ambient input and the final checks remain unfinished.

### Files this continuation changes

```text
compiler/src/hir.rs
compiler/src/check/mod.rs
compiler/src/check/pipeline.rs
compiler/src/check/expr/lambda.rs
compiler/src/hir/tests.rs
compiler/src/check/init_effects.rs
compiler/tests/module_global_names.rs
codegen/src/lir/lowering.rs
codegen/tests/module_init_order.rs
codegen/tests/module_init_routes.rs
codegen/tests/module_init_values.rs
specs/tracking/module-init-order.md
```

The earlier working-tree changes remain. This continuation does not
change an emitted output, a golden, or a LIR snapshot. It does not edit
the owner C18 paragraph, `tools/gate.sh`, or `CLAUDE.md`.


## Fourth Phase Review verification after the harness input correction

The owner authorizes a new measurement with the corpus loaders' mirror
inputs. The stop condition excludes a harness input defect. This round
runs the corrected measurement once; it makes no git write or commit.

The temporary harness reuses the accept directory loader. It copies
`trap_sources` from the trap loader without a change. It also copies
the warning and example source loaders with their mirror order.
The trap inputs now include the interop mirror for `SGPUProbeBlendState`.
No production source changes to bypass a rejection.

```text
rule 5a/5b: accept=291, warn=5, trap=71, examples=17, source-files=415, rejected=0
```

All 384 programs check. No source triggers the amended stop condition.
The harness runs once and is deleted with its executable. The sort
probe uses the existing node runner and reports `0\tok\t310a`.
Node prints `1\n`, equal to the interpreter, JIT, and ship-C tests.
The temporary sort sources are deleted.

The generated-docs generator writes all three derived documents. Only
the existing corpus-index diff remains. Pinned
`cargo +1.95.0 fmt --check` passes. The final locked, offline, all-target
workspace build passes with zero warnings.

The first Clippy check reports one new `needless_range_loop` warning in
`codegen/src/lir/lowering.rs`. The loop now walks the validated segment
slice, followed by its final global-only position. The related tests
pass: order 20/0, routes 16/0, values 4/0. The final all-target Clippy
check reports zero warnings in changed files. The standing library
counts remain compiler/runtime/codegen = 3/18/13. No lint is suppressed.
The final format and all-target build checks pass after that change.
All changed Rust files stay below 2,000 lines. `git diff --check` passes.

This verification changes `codegen/src/lir/lowering.rs` and this note.
The earlier working-tree changes remain. It changes no golden or LIR
snapshot. The prior section records every Red and the full file list.
The separate top-level block disposal defect and the precision limits
of rule 137.3 remain outside this fix.


### Fourth-review full gate verdict

The pre-gate hygiene check exits 0. `tools/gate.sh full` runs exactly
once in this verification and reports:

```text
gate full 0101b2bb2a9d97c14ac05d416313a6d3051f1d88 dirty:26 debug 2067/1/3 release 2064/1/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 1
```

Record: `target/gate/20260930T175722Z-full.md`.
Fmt, build, Clippy, tsc, and hygiene exit 0. Debug and release each fail
one test:

```text
compiler/tests/nullish.rs:
module_initializer_lambda_call_keeps_the_indirect_call_order_error ... FAILED
thread panicked at compiler/tests/nullish.rs:70:58:
the nullish program must fail: Module { ... }
```

The fixture is:

```typescript
const n: i32 = ((): i32 => 3)();
export function main(): void { print(`${n}`); }
```

The old test expects S100 on `n` through an indirect call. Rule 5b
accepts this fixture: its lambda returns 3 and reads no global. The
checker accepts it in both profiles. The test predates the new rule;
its S100 expectation is no longer valid. The test file is outside the
fourth handoff's file set and this round does not edit it.

The gate passes the new lambda identity, range, host, sort, map, and
route tests in both profiles. The LIR golden checks pass. No tracked
golden moves. The corpus measurement still reports zero rejections.
The bare-map temporary-directory failure does not recur.

The round stops after this failed gate. It makes no source change and
no second gate run. Only this tracking record changes after the gate.
The old nullish test needs an accepted-and-runs case and an S100 control
whose lambda reads a later global. That test update remains unfinished.


## Nullish expectation correction under rule 5b

The owner adds `compiler/tests/nullish.rs` to the file set and names
the obsolete expectation. This round changes that test and this note.
It makes no git write or commit. The prior gate records the Red in
both profiles; the compiler accepted a lambda that reads no global.

The test is now
`module_initializer_lambda_call_reads_only_its_followed_body`.
The unchanged program with `((): i32 => 3)()` checks clean.
A control with `((): i32 => later)()` before the declaration of `later`
asserts exactly one S100 and its complete message:

```text
`later` is accessed before its declaration, through an indirect call
```

The complete nullish test target passes: 13 passed, 0 failed, 0 ignored.
Its warm execution costs 0.01 s and compiles no ship-C program.
Pinned `cargo +1.95.0 fmt --check` passes after one line-wrap correction.
The locked, offline, all-target workspace build passes with zero warnings.
No production code or golden changes. The earlier corpus measurement
remains valid; this round does not repeat it.


### Nullish correction full gate verdict

The pre-gate hygiene check exits 0. `tools/gate.sh full` runs exactly
once in this correction round and reports:

```text
gate full 0101b2bb2a9d97c14ac05d416313a6d3051f1d88 dirty:27 debug 2068/0/3 release 2065/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

Record: `target/gate/20260930T181846Z-full.md`.
Every gate step exits 0. The renamed test passes in debug and release,
with the clean program and the full-message S100 control. Build warnings
remain zero. Clippy remains at its standing library counts, 3/18/13.
No tracked golden moves. Only this note changes after the gate;
no source change or additional gate run is made.

The named fourth-review fixes and final checks are complete. The rule
137.3 precision limits and top-level block disposal defect remain the
separate open items of the contract.


## Fifth Phase Review at 36b28b2d

This round reads the fifth handoff and `git show HEAD`. Rule 5b now
uses unit summaries and an unordered closure for each initializer item.
The existing HIR carries the required unit identities and call arguments.
No HIR form change, git write, or commit is needed.

### Red outputs

The new tests run before the form change. Long accepted HIR and LIR
dumps from `expect_err` are omitted below; their accepted verdict is retained.

```text
module_init_closure: 0 passed; 4 failed; finished in 5.01s
  a_loop_follows_values_made_in_a_later_iteration:
    the item reads a later global: Module { ... } (checker accepted)
  a_loop_closes_values_made_by_its_direct_body:
    the item reads a later global: Module { ... } (checker accepted)
  a_generator_step_follows_values_made_after_creation:
    the item reads a later global: Module { ... } (checker accepted)
  twelve_callbacks_check_without_path_enumeration:
    twelve callbacks exceeded the five-second hang guard: Timeout

module_init_callbacks: 0 passed; 4 failed
  a_for_each_literal_does_not_run_an_earlier_greeter:
    S100: `greeting` is accessed before its declaration, through an indirect call
  a_sort_literal_does_not_run_an_earlier_ready_callback:
    S100: `registry` is accessed before its declaration, through an indirect call
  a_for_each_literal_does_not_run_an_earlier_record_function:
    S100: `stats` is accessed before its declaration, through an indirect call
  a_sort_literal_does_not_run_an_earlier_logger:
    S100: `prefix` is accessed before its declaration, through an indirect call

module_init_order: 20 passed; 3 failed
  a_top_level_statement_without_a_segment_is_a_lowering_error:
    statement is unowned: Module { ... } (lowering accepted)
  a_top_level_statement_in_two_segments_is_a_lowering_error:
    statement has two owners: Module { ... } (lowering accepted)
  segment_and_module_counts_must_agree:
    segment is missing: Module { ... } (lowering accepted)

a_made_unit_without_a_summary_returns_an_error: 0 passed; 1 failed
  made function value has a summary (library panic)
  a malformed unit must return an error, not panic (test assertion)
```

The first callback test run reaches the refusal assertion first. It
reports `through an indirect call` instead of `through a lambda`.
A second Red run checks the accepted fixture first and reports the
four false rejections quoted above. No script with an early read runs.

After the form change, three older route expectations fail:

```text
for_each_indirect_call_rejects_later_global_and_runs_after_it:
  left: `later` is accessed before its declaration, through a lambda
  right: `later` is accessed before its declaration, through an indirect call
generator_creation_scans_body_before_any_step:
  expected S100 through `values`; checker accepted
shadowed_generator_parameter_cannot_hide_body_read:
  left: `m` is accessed before its declaration, through `step` -> an indirect call
  right: `m` is accessed before its declaration, through `bad`
```

Those tests now pin the fifth-review rule. Generator creation makes a
unit; a step reads its body. The creation case has an early-step refusal
control. The shadowed-parameter case retains its early-read refusal.
The for-of and next parameter cases also assert early-read controls.
The three named silent tests receive their missing counterparts.
These counterpart assertions pass; they do not add production fixes.

### Form and cost

A summary records globals, direct calls, made units, and an indirect bit.
A built-in callback literal or declared function records a direct edge.
Other callback values record an indirect call. Generator creation makes
its declaration-keyed body unit. `next` records an indirect call;
generator for-of uses the checked next-call form. Async calls retain
the direct whole-body rule. Statement and built-in matches stay total.

Each item uses a queue and a memo set. A unit enters the queue once.
The item's direct closure adds made values; its first indirect call
adds every made unit, and new made values join that same queue.
The fixed point has no order inside the item and no path enumeration.
Reads retain one route per visited unit. Earlier items retain their
made set, and the global under initialization stays uninitialized.

The scanner returns an error for a made unit with no summary and for
invalid segment bounds or indices. Library code has no explicit panic,
assert, expect, or unreachable call. Lowering also checks statement
ownership and equality of segment and module counts.

```text
twelve callback checker cost: 5.045917ms
module_init_closure: 4 passed; 0 failed
scanner units: 5 passed; 0 failed
module_init_callbacks: 4 passed; 0 failed; 0.39 s; four ship-C compiles
module_init_order: 23 passed; 0 failed
module_init_routes: 16 passed; 0 failed; 15 ship-C compiles
module_init_values: 4 passed; 0 failed
module_global_names: 21 passed; 0 failed
nullish: 13 passed; 0 failed
```

The cost test asserts less than one second and has a five-second hang
guard. The callback tests compare interpreter, JIT, and ship-C bytes.
Each has a full-message S100 control for its literal callback.
The source files remain below 2,000 lines.

The changed files in this round are:

```text
compiler/src/check/init_effects.rs
compiler/tests/module_init_closure.rs
codegen/src/lir/lowering.rs
codegen/tests/module_init_order.rs
codegen/tests/module_init_routes.rs
codegen/tests/module_init_callbacks.rs
specs/tracking/module-init-order.md
```

The earlier working-tree changes remain. The separate top-level block
disposal defect and the precision gap of 137.3 remain open.

### Corpus and pre-gate checks

The temporary harness uses the corpus loaders' ambient mirrors for
accept, warn, trap, and examples. It runs once in this round.

```text
rule 5a/5b: accept=291, warn=5, trap=71, examples=17, source-files=415, rejected=0
```

All 384 programs check clean. The four accepted callback fixtures also
match node output bytes:

```text
g1: 620a610a68692c20620a
g2: 310a310a
g3: 360a380a
f3: 6170703a20310a
```

The temporary harness, executable, and comparison sources are deleted.
API, language, and corpus documentation generation completes.
Pinned `cargo +1.95.0 fmt --check` passes. The locked, offline workspace
all-target build passes with zero warnings. All-target Clippy passes;
no warning points to a changed file. Every changed Rust source is below
2,000 lines.

`git diff --check` and `tools/hygiene.sh` pass before the gate.
The full gate runs once for this round.

### Full gate result

The full gate completes once with exit 0.

```text
gate full 36b28b2decd5bed1752aef34f188ab1acdfe9833 dirty:29 debug 2080/0/3 release 2077/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

The gate record is `target/gate/20260930T194303Z-full.md`.
Debug has 2,080 passing tests and release has 2,077; neither has a
failure. The three ignored tests and two debug skips remain recorded.
Clippy counts are the standing compiler/runtime/codegen warnings;
none points to a changed file. No golden moves.

## Sixth Phase Review at 5a84d77b

This round reads the sixth handoff and `git show HEAD`. Rule 5b now
requires one parent link per unit and a route only for a reported read.
Acceptance 10 requires the regex case and a visit-count test.

### Red and controls

Two new regex tests run before the production fix:

```text
regex_in_an_imported_module_checks_after_an_entry_statement: FAILED
  S100: internal error: global initializer is outside its owner range
  b.ts:1:1
regex_in_a_later_run_module_checks_after_a_dependency_statement: FAILED
  S100: internal error: global initializer is outside its owner range
  b.ts:1:1
regex_in_later_function_runs_before_global_initializer: ok
result: 1 passed; 2 failed
```

The first later-run fixture also has a surface error at b.ts:1:86:

```text
loose equality is not in the language; use `===` / `!==`
```
The fixture changes to `===` and runs Red again before the fix. Both
new tests then fail only with the internal error quoted above.

The new visit test is written before the form change. It already passes:
25 visits, for twelve outer lambdas, twelve filter lambdas, and leaf.
The prior queue already memoizes unit visits; the defect is its route
copies. The wall-clock assertion is removed from the integration test.
Its same-shape firing control, with callback 11 reading a later global,
also passes before the form change and asserts this full S100 message:

```text
`later` is accessed before its declaration, through an indirect call
```

A constructor call in the lambda scanner still has its removed argument
in the first build. The build reports E0061 at init_effects.rs:509.
That call is corrected before the Green runs.

### Form and Green

The checker identifies regex globals once after body checks. It removes
those indices from the per-file segments before the route scan. The HIR
uses the same filtered segments; it has no second ownership filter.
The scan obtains module bindings from those owners and validates them
before reading an index. Regex literal initializer positions stay zero.

Each visited unit has one parent link. A global access records a link
index. Only a reported read builds a route. The scan returns its reads,
not an unused visit count. The visit count exists only in unit-test
builds. The twelve-callback test asserts 25 visits and at most one link
per unit plus the root and indirect link. Parent indices precede children.
The scanner drops its unused bindings parameter and its callers.
`InitializerSegment` is non-exhaustive and has a constructor; codegen
uses it for regex owners and tests use it for malformed owners.

```text
scanner units: 6 passed; 0 failed
module_init_closure: 4 passed; 0 failed
module_init_order: 25 passed; 0 failed; 0.60 s; seven ship-C compiles
module_init_routes: 16 passed; 0 failed
module_init_callbacks: 4 passed; 0 failed
module_init_values: 4 passed; 0 failed
module_global_names: 21 passed; 0 failed
nullish: 13 passed; 0 failed
```

The imported regex prints `m / true`; the later-run regex prints
`a / m / true`. Each matches on the interpreter, dev JIT, and ship C.

### Debug cost probes

The review supplies these before times: 1,600 lambdas 8.1 s, 1,600
filter callbacks 14.5 s, depth 800 with 800 items 20.4 s. It does not
supply their source files. This round records the explicit probe shapes
and compares the same source before and after the parent-link change.
Each `check_program` call includes both checker passes.

For the first two probes, the program makes 1,600 stored callback
lambdas, then calls cb0 in one global initializer. Each lambda calls a
stored leaf function. The filter variant also calls `values.filter`
with a literal callback. For the third probe, 800 declared functions
form a direct chain; 800 global initializers each call its root.

| Probe | Before | After |
|---|---:|---:|
| 1,600 lambdas, one call item | 1.886430542 s | 1.890515417 s |
| 1,600 filter callbacks, one call item | 2.019862708 s | 2.030096541 s |
| Depth 800, 800 call items | 18.818401958 s | 2.817167333 s |

An additional after probe calls each callback in its own item as it is
made: 1,600 lambdas 10.368711792 s, 1,600 filter callbacks 16.034705584 s,
and the same depth-800 chain 2.822675375 s. It exercises the growing made
set across items. All probe programs check clean. These times are
measurements, not test assertions.

### Corpus measurement

The temporary harness uses the accept loader and the same warn, trap,
and example mirror inputs as their corpus loaders. It runs once.

```text
rule 5a/5b: accept=291, warn=5, trap=71, examples=17, source-files=415, rejected=0
```

All 384 programs check clean. No corpus source is changed in this round.

### Pre-gate checks and files

The temporary measurement harness, cost probes, and executables are
deleted. API, language, and corpus documentation generation completes.
Pinned `cargo +1.95.0 fmt --check` passes. The locked, offline workspace
all-target build passes with zero warnings. All-target Clippy passes;
no warning points to a changed file. Each changed Rust source has fewer
than 2,000 lines. `git diff --check` passes.

This round changes these files:

```text
compiler/src/check/init_effects.rs
compiler/src/check/pipeline.rs
compiler/src/hir.rs
compiler/tests/module_init_closure.rs
codegen/src/lir/lowering.rs
codegen/tests/module_init_order.rs
specs/tracking/module-init-order.md
```

The earlier working-tree changes remain. No emitted-output golden or
LIR snapshot changes. The separate top-level block disposal defect and
the precision gap in 137.3 remain open.

`tools/hygiene.sh` passes before the gate.
The full gate runs once for the sixth round.

### Full gate result

The full gate completes once with exit 0.

```text
gate full 5a84d77bf8e290c4f735dc704286ef9297d93c21 dirty:29 debug 2083/0/3 release 2080/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

The gate record is `target/gate/20260930T204005Z-full.md`.
Debug has 2,083 passing tests and release has 2,080; neither has a
failure. The three ignored tests and two debug skips remain recorded.
Clippy counts are the standing compiler/runtime/codegen warnings;
none points to a changed file. No golden moves.

## Verification Review: count queue work

The visit-count test previously read `memo.len()`, the record that the
queue deduplication writes. It did not measure the queue work.
The test-only counter now increments on every successful queue pop in
`resolve`. It counts item roots as well as body units. The twelve-callback
test asserts 40 pops: fifteen initializer item roots, twelve outer lambda
units, twelve filter lambda units, and leaf. The parent-link bound uses
this actual pop count. No production counter or public API is added.

### Mutation Red

The memo guard is removed once while its set insertion is retained.
The queue then pushes duplicate units. The same test fails at its count
assertion, not at a check of the memo record:

```text
twelve_callbacks_visit_each_unit_once_per_item: FAILED
assertion `left == right` failed:
  fifteen item roots, twelve outer units, twelve filter units, and leaf
  left: 52
 right: 40
result: 0 passed; 1 failed
```

The guard is restored in a finally block before verification continues.
The guarded test passes with 40 pops before the mutation.

This round changes `compiler/src/check/init_effects.rs` and this tracking
note. The earlier working-tree changes remain. No golden or emitted
output change is needed. The full gate will run once for this round.

### Restored checks

After restoring the memo guard, all six scan unit tests pass. The
queue-count test passes with 40 pops. Pinned `cargo +1.95.0 fmt --check`,
`git diff --check`, and `tools/hygiene.sh` pass. The locked, offline
workspace all-target build passes with zero warnings. init_effects.rs
has 1,145 lines. The full gate now runs once.

### Full gate result

The full gate completes once with exit 0.

```text
gate full 5a84d77bf8e290c4f735dc704286ef9297d93c21 dirty:29 debug 2083/0/3 release 2080/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
```

The gate record is `target/gate/20260930T211722Z-full.md`.
Debug has 2,083 passing tests and release has 2,080; neither has a
failure. No golden moves. The memo guard remains restored.
