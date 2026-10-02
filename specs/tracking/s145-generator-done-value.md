# §145: finished generator reference values

The contract is `compiler.md` §145 at `4c6329d4`. Collision C23 names the runtime divergence.

## Red

A CLI built from `4c6329d4` measured both new trap entries before the change.
`t75` returned exit 2 with `program terminated abnormally (dev-JIT child signal 11)` and empty stdout.
`t76` returned exit 0 with `1\n`. Neither entry raised `generator-done-value`.
Stock `tsc` accepted both entries. Node raised `TypeError: Cannot read properties of undefined` at `b.v` for t75.
Node printed `1\n` for t76.

## Change

`Type::traps_on_generator_done_value` requires a non-nullable handle type other than `Str`.
Value classes, scalar types, strings, dates, and enums do not satisfy the predicate.
The HIR field-read site uses this predicate. The LIR verifier independently derives the required guard from the operand type.
Both tiers and the interpreter consume the same LIR site before the value load.
The runtime code is 32, with rule name `generator-done-value`.
Iterator-result field patterns use the same field-read path and report the bound name's position.
The C emitter declares a shared pointer iterator-result type once when nullable and non-nullable generators share one module.

## Measurements

| Entry | Node stdout | dev stdout | ship stdout | interpreter stdout |
|---|---|---|---|---|
| t75 | TypeError at later field use | empty; trap at 16:20 | empty; trap at 16:20 | empty; trap at 16:20 |
| t76 | `1\n` | empty; trap at 16:20 | empty; trap at 16:20 | empty; trap at 16:20 |
| a310 | `true\ntrue\n7\n7\n7\n7\n` | same | same | same |
| a311 | `undefined\nundefined\n` | `0\n0\n` | `0\n0\n` | `0\n0\n` |

A separate field-pattern probe trapped at 7:18 in all three execution forms.
The scalar entry cites C8 and declares itself not JS-comparable.
Ship CLI probes used source copies, a runtime archive copy, and an include copy under the temporary directory.
The capture with `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1` added only the a310 and a311 sections.
No existing `.expected` file changed. The header and reference documents came from their generators.

## Verification

`cargo test --offline --locked` passed for the compiler, codegen, and runtime crates.
The dev/ship trap sweep and accept sweep passed, including all new entries.
The interpreter trap sweep and the new three-form control tests passed.
The JS corpus target passed all 11 tests. Stock `tsc -p tsconfig.json` passed.
`cargo fmt --check` and `git diff --check` passed.
A second generator run produced byte-identical runtime headers and reference documents.
The Phase Review and its collision-index follow-up reported no findings. `tools/hygiene.sh` passed.
No Rust file that received new lines exceeded 2,000 lines. No commit was made.

## Landing gate

`goldens-moved 1` is the LIR text snapshot: `a310`, `a311`, `t75`, and
`t76` are generator entries.

```text
gate full 09de36d6915f5ab4457fc0ab7acfd351dd7a706d dirty:39 debug 2190/0/3 release 2187/0/3 skips 2/0 clippy 2/18/13 goldens-moved 1 exit 0
```

## Phase Review fix round 3

The contract is `compiler.md` §145 at `18d1b825`.
`Type::zero_is_value` covers every `Type` variant without a default arm.
It checks fixed-array elements and value-class fields recursively. A null string handle fails the check.
HIR sites and the LIR verifier use the same function with their own complete class fields.
Both member loads and field addresses require the guard. Both tiers and the interpreter consume it before the read.
The dev lowering now accepts an iterator-result address; the ship guard also resolves folded addresses.

A CLI and a three-form probe built from `9a28f076` measured the Red entries:

| Entry | Interpreter | dev | ship |
|---|---|---|---|
| t77 | `1\n` | internal lowering error: `IterResult.value has invalid base type` | `1\n` |
| t78 | `InvalidLir`: `null used as a string` | `false\n0\nend\n` | `false\n0\nend\n` |
| t79 | `1\n` | `1\n` | `1\n` |

`t77` reads through an inline field address. `t79` binds a fixed array through the iterator-result field pattern.
All three forms now raise `generator-done-value` with empty stdout at `15:15`, `14:23`, and `14:18`, respectively.
The new scalar value-class control `a312` prints `0:false\n0:false\n` on all three forms.
Only its new section enters the LIR snapshot. No existing section or `.expected` file changes.

`r300` cites `compiler.md §107.1` and §145 rule 1a.
Stock `tsc` reports TS2339: `Property 'other' does not exist on type 'IteratorResult<number, any>'.`
The checker reports S100 at `11:11`, with no follow-up diagnostic.
The `PatternSourceShape` text now names the `IterResult<T>` source.
The duplicate execution tests are removed. The retained test removes guards from valid LIR and tests verifier rejection.
Its measured debug test-suite cost is 0.01 s. It compiles no native program and runs no execution form.

The orchestrator must add `t77`–`t79` to C23, `a312` to C8, and `r300` to the §107.1 record.

`cargo test --offline --locked` passed for the compiler, codegen, and runtime crates after the fixes.
The three-form corpus checks, all 11 JS corpus tests, and the measured `tsc` corpus-header test passed.
`tsc -p tsconfig.json`, `cargo fmt --check`, and `git diff --check` passed.
The document generator produced byte-identical files on its second run.
Every changed Rust file remains below 2,000 lines. No commit was made. `tools/gate.sh` was not run.
`tools/hygiene.sh` passed.

## Phase Review fix round landing gate

`goldens-moved 1` is the LIR text snapshot (new generator entries).

```text
gate full 18d1b8254ef42e31511a4582b1906ae3bdea3aba dirty:26 debug 2189/0/3 release 2186/0/3 skips 2/0 clippy 2/18/13 goldens-moved 1 exit 0
```

## Phase Review fix round 4

The contract is `compiler.md` §145 at `ee9e8fe6`. Collision C23 names the runtime divergence.
A CLI and a three-form probe built from that pin measured the new entries before the change.

| Entry | Interpreter | dev | ship |
|---|---|---|---|
| t80 | `InvalidLir`: `reached a structurally unreachable LIR block` at 11:3 | `done=true|tag=0|v=w3|end\n` | same |
| t81 | `InvalidLir`: `string alias value has no member` | `v=w3|false false\n` | same |

`Type::zero_is_value` now receives alias wire values from each consumer's form.
A wire alias excludes zero when its wire table has no zero member.
Plain aliases and wire aliases with a zero member pass the predicate.
The recursive class-field, fixed-array, and iterator-result checks carry the same alias source.
HIR sites and the LIR verifier read their own alias tables.
The direct unit test covers both wire cases, plain aliases, fixed arrays, and nullable aliases.
The verifier test removes required guards from the two new entries and checks rejection.
The HIR, LIR, and runtime trap descriptions now state the rule 1 zero-value condition.
The runtime header generator produces the same header bytes; the header does not include these descriptions.

`t80` and `t81` now trap with empty stdout at 21:18 and 15:15 on all three execution forms.
`a313` reads a wire zero member, a fixed array of that alias, and a plain alias after generator completion.
It also reads a wire member before completion. All three forms match `w3\nw0\nw0|w0\nfirst\n`.
Only the new `a313` section enters the LIR snapshot. Every existing section remains byte-identical.
No existing `.expected` file changes.

The orchestrator must add `t80` and `t81` to C23 and `a313` to C8.

`cargo test --offline --locked` passed for the compiler, codegen, and runtime crates.
The new entries passed all three execution forms. The JS corpus target passed all 11 tests.
`cargo fmt --check`, `git diff --check`, and `tools/hygiene.sh` passed.
A second generator run produced byte-identical runtime headers and reference documents.
The corpus index adds only the three new entries. Every changed Rust file remains below 2,000 lines.
No commit was made. The existing C23 edit was preserved. `tools/gate.sh` was not run.

## Verification review fix round landing gate

`goldens-moved 1` is the LIR text snapshot (new generator entries).

```text
gate full da7aec5abbc36d5cca8c18792dc37d12284bb366 dirty:19 debug 2189/0/3 release 2186/0/3 skips 2/0 clippy 2/18/13 goldens-moved 1 exit 0
```

## Phase Review fix round 5

The contract is `compiler.md` §145 at `20e86b2a`.
The checker rejects every iterator-result binding pattern with S100.
The diagnostic names `const r = it.next(); if (r.done) ...`.
`r300` retains its measured TS2339 header and receives the new diagnostic.
`r301` records the review's loop with `tsc: accepts`.
Before the fix, the reject test accepted `r301` at the contract pin and failed.
The batched stock TypeScript test accepts `r301` and retains the recorded result for `r300`.

`t79` is retired. The LIR trap-site table, ship trap table, and malformed-LIR verifier test remove its row.
The checker uses the common field-read path for patterns and members. No separate pattern guard path exists.
The member-read guard remains unchanged.
The `PatternSourceShape` entry and coroutine reference generator reject iterator-result patterns.
The coroutine reference replaces the retired `t79` link with `r301`.
No generic matrix cell accepts iterator-result patterns; its type-kind table records that the source type has no name.

The HEAD LIR snapshot contains no `t79` section.
The existing accept entries `a310`–`a313` contain five iterator-result field patterns.
The new checker rejects these entries. The snapshot capture stops at `a310`.
The handoff limits corpus edits to its named files, so the accept-entry edits require scope approval.

The reject corpus target passes all 40 tests.
The compiler suite stops at `corpus_accept` because `a310`–`a313` now fail the source-shape check.
The combined compiler/codegen suite stops at the codegen golden test for the same four entries.
The generated documents come from `generate-api-reference`.
`cargo fmt --check` and `git diff --check` pass.
The existing collision edit remains unchanged. No commit was made. `tools/gate.sh` was not run.

## Phase Review fix round 6

The approved extension replaces the two iterator-result patterns in `a310` with member reads.
The interpreter, dev, and ship forms match its unchanged golden bytes.
The JS corpus passes all 11 tests. The reject corpus passes all 40 tests.
The stock TypeScript corpus target passes all 11 tests, including the headers for `r300` and `r301`.
The malformed-LIR member-read guard test passes.
The document generator produces byte-identical files on its second run.
`cargo fmt --check` and `git diff --check` pass.

The LIR capture now stops at the iterator-result pattern in `a311` at 13:9.
`a312` at 14:9 and `a313` at 17:9 also retain iterator-result patterns.
The compiler accept test and two LIR corpus tests fail on these sources.
These three entries remain outside the approved extension. They need member reads before the snapshot capture can finish.
The snapshot remains unchanged; its HEAD version contains no `t79` section.
The existing staged `t79` deletions and collision edit remain unchanged.
No commit was made. `tools/gate.sh` was not run.

## Phase Review fix round 7

The accept entries `a310`–`a313` replace five iterator-result field patterns with member reads.
Their `.expected` files remain unchanged. The dev, ship, and interpreter corpus sweeps match these outputs.
The regenerated LIR snapshot changes only the `a310`–`a313` sections.
No snapshot section is added or removed; HEAD contains no `t79` section.
The generated documents come from `generate-api-reference`. A second run produces byte-identical files.
The existing collision edit and staged `t79` deletions remain unchanged.
No commit was made. `tools/gate.sh` was not run.

`cargo test --offline --locked -p subscript-compiler -p subscript-codegen` passes.
The compiler accept corpus and all 40 reject tests pass.
The dev/ship and interpreter corpus sweeps pass, including `a310`–`a313`.
The batched stock TypeScript header test and all 11 JS corpus tests pass.
`cargo fmt --check`, `git diff --check`, and `tools/hygiene.sh` pass.
Every changed Rust file remains below 2,000 lines.

## Pattern-source revert landing gate

An `IterResult` is not a pattern source (§145 rule 1a, owner decision);
`t79` retires, `r301` pins the idiom, and `a310`–`a313` read `r.value`.
`goldens-moved 2` is the LIR text snapshot and the deleted `t79` golden.

```text
gate full 20e86b2a778bb950be8e4d01a69ff544c546e520 dirty:20 debug 2189/0/3 release 2186/0/3 skips 2/0 clippy 2/18/13 goldens-moved 2 exit 0
```
