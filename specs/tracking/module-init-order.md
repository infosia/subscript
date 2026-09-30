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
