# Function value reference uses (compiler.md §123)

## File size

No split was required. Existing Rust files with added lines were below 2,000 lines:
`compiler/src/check/expr/call.rs` (1,428), `compiler/src/check/expr/method.rs` (1,520),
`compiler/src/ambient.rs` (1,669), `compiler/tests/corpus_reject.rs` (1,197),
and `codegen/src/lir/call.rs` (857).

## Red at f8cb649

`cargo build --offline --locked -p subscript-cli` succeeded before implementation.
A temporary Rust probe called the checker, dev JIT, ship C, and interpreter pipelines with the corpus sources.

| Entry | Checker | Dev JIT | Ship C | Interpreter |
|---|---|---|---|---|
| a272 | S014 at lines 23 and 24; S018 at lines 11, 17, 30, 32, 33 | Same checker rejection | Same checker rejection | Same checker rejection |
| r249 | S018 at line 10, column 14 | Same checker rejection | Same checker rejection | Same checker rejection |

S014 called the function value a scalar. S018 reported that `Holder` had no method `cb` or `optional`.

TypeScript 5.9.2 accepted a272 through `tsc -p tsconfig.json`.
TypeScript rejected r249 with TS2721 at line 10, column 12.
Node v24.18.0 printed `6 false`, then reported `TypeError: functions.getOr is not a function`.
The a272 header declares `js-comparable: no Q24`.

## Implementation

`map_get_value_ok` removes one nullable wrapper, then queries the value type's own `handle_kind`.
It accepts `supports_nullable()` or `HandleKind::Array`. It does not construct a nullable type for this query.
A boundary value has no handle kind; its nullable box does not make the value eligible.
S011-rejected handle kinds remain outside the set.

The scalar S014 reason remains `A scalar value type has no null miss value`.
The other S014 reason is `The value type has no | null form of the map's value representation`.
`string` receives this representation reason.
Both messages name `getOr` as the total accessor.

The checker reads a function field through `member_on`, applies path facts, and calls `check_indirect_call`.
The HIR carries `Callee::Value`; LIR lowers it to an indirect call.
The receiver and field value are evaluated once before the arguments.
The same indirect-call check rejects an unnarrowed field and an unnarrowed local with S100.
The same capture check rejects a capture passed through a function field.

The nullable Map test exposed a missing argument conversion in LIR call signature selection.
The selector now accepts null and the inner type for a nullable parameter.
The existing argument conversion emits the nullable function representation for all three engines.

## Targeted tests

- `compiler/tests/function_value_reference.rs`: three tests passed in 0.01 seconds.
  These cover both S014 reasons, `getOr` controls, field-path flow, local diagnostic parity, and capture boundaries.
- `codegen/tests/function_value_reference.rs`: two tests passed in 0.89 seconds.
  These compare exact output on the dev JIT, ship C, and interpreter.
  The tests cover a272, nullable Map values, hit-to-miss loops, and argument-side field replacement.
- The accept checker suite passed 13 tests; the reject checker suite passed 38 tests.
- `coroutine_and_measurement_lir_text_matches_goldens` passed in 1.41 seconds.
  The a272 entry has no coroutine and does not enter that snapshot. Snapshot movement: zero lines.
- `node_modules/.bin/tsc -p tsconfig.json` passed.
- The API reference and corpus index were updated through `generate-api-reference`.

The new a272 golden contains five output lines. No existing `.expected` file changed.

## Initial full gate

The pinned `cargo fmt --all --check` passed.
The offline, locked workspace build for all targets passed without warnings.
`tools/gate.sh full` ran once and failed. No retry or corrective code change followed.

The debug and release profiles each failed
`api_reference::tests::every_generated_rejection_is_rejected_by_the_checker`.
The new `Map<K, non-nullable V>` rejection row has no registered checker witness in `compiler/src/api_reference.rs`.
The dedicated S014 test passes but does not register that witness.

The gate's format, build, clippy, tsc, and hygiene steps passed.
The tier and interpreter corpus comparisons passed, including a66 and a272.
The gate reported zero modified existing goldens.
The implementation remains incomplete at the failed gate.

```text
gate full f8cb649065d7e78dd9e31aafff4632ee6198178c dirty:13 debug 1814/1/3 release 1811/1/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 1
```

## Fourth addendum at fa3b519

The working tree from the preceding addenda remained in place. No git write command ran.
No Rust file needed a split. Files with added lines remain below 2,000 lines:
`call.rs` (1,446), `method.rs` (1,529), `ambient.rs` (1,670), `divergence.rs` (1,262),
`corpus_reject.rs` (1,211), and `function_value_reference.rs` (139).

1. The predicate queries the original value handle, as described above.
   `corpus/reject/r251-map-boundary-struct-get.ts` uses `SubByValueI64Pair` from the interop mirror.
   It lives beside `r169`, the existing mirror-using reject entry.
   The reject harness loads the mirror and pins S014 at line 8.
2. The reason test covers i32, boolean, enum, string-literal alias, string, Date, RegExp, Generator, value class, FixedArray, and boundary struct.
   Each rejected `get` has an accepted `getOr` control.
3. `corpus/reject/r252-boundary-function-field-call.ts` uses `SubRequestInfo` from the same mirror.
   The reject harness pins S100 at line 8. The message names the C function pointer and the script function value.
   A dedicated test pins that reason and accepts the matching script-class call.
4. The duplicate a272 three-engine test was removed.
   The separate nullable Map and field-read evaluation-order test remains.
5. `a273-function-field-calls` contains no `get` or `getOr`.
   It covers direct, `this`, optional, static, side-effecting receiver, and narrowed nullable field calls.
   Its optional-call control skips the argument on null and evaluates it on a present receiver.
   Node v24.18.0 produced the exact 27-byte golden. TypeScript 5.9.2 accepted the entry.
6. The field-path invalidation defect remains open under compiler.md §124, as §123.1 rule 7 requires.
   This change does not modify call- or alias-store invalidation.

### Measurements before the correction

These measurements used the inherited working tree, before the fourth-addendum production changes.
The r251 checker witness failed because the checker accepted the boundary-struct map read.
The CLI checker also accepted r252. TypeScript accepted both entries with the interop mirror.
The reason test failed because `string` received the scalar reason.
The unsafe boundary-struct programs were not executed on an engine in this measurement.

### Validation before the fourth-addendum gate

- Compiler library: 333 passed in 0.07 seconds, after document generation.
- Reject harness: 38 passed in 0.10 seconds.
- Function reference checker tests: four passed in 0.22 seconds.
- Accept checker harness: 13 passed in 0.85 seconds.
- `tsc -p tsconfig.json`: passed. Node output equals the a273 golden, byte for byte.
- `cargo fmt --all --check`: passed with the pinned toolchain.
- `cargo build --offline --locked --workspace --all-targets`: passed without warnings in 15.35 seconds.
- API and corpus documents were regenerated with `generate-api-reference`.

Existing `.expected` files and the LIR snapshot have no changes.

### Fourth-addendum full gate

`tools/gate.sh full` ran once and passed. Debug had 1,815 passed tests; release had 1,812.
Both profiles had zero failures. The three-engine corpus comparisons include a66, a272, and a273.
The LIR snapshot test passed. Snapshot movement and existing golden movement: zero lines.
The gate's format, build, clippy baseline, TypeScript, and hygiene checks passed.
Release emitted one dead-code warning for the unchanged `Context::test_offset_live_bytes_counter` test helper.
Clippy counts remained compiler 5, runtime 18, and codegen 13.

```text
gate full fa3b519d9c80e28f9a120ccc171f156ccfc993d5 dirty:21 debug 1815/0/3 release 1812/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0
```

### Working-tree file inventory

The cumulative working tree contains these 21 changed or new files:

- `compiler/src/ambient.rs`
- `compiler/src/api_reference.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/divergence.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/function_value_reference.rs`
- `codegen/src/lir/call.rs`
- `codegen/tests/function_value_reference.rs`
- `corpus/accept/a272-function-value-reference-uses.ts`
- `corpus/accept/a272-function-value-reference-uses.expected`
- `corpus/accept/a273-function-field-calls.ts`
- `corpus/accept/a273-function-field-calls.expected`
- `corpus/reject/r249-unnarrowed-nullable-field-call.ts`
- `corpus/reject/r250-map-generator-get.ts`
- `corpus/reject/r251-map-boundary-struct-get.ts`
- `corpus/reject/r252-boundary-function-field-call.ts`
- `generated-docs/api-reference.md`
- `generated-docs/corpus-index.md`
- `specs/tracking/s123-function-value-reference.md`


## Fifth addendum at d1934bf: stopped at item 4

No git write command ran. The inherited working tree remains in place.
No split was required. The added-line files `namespace.rs` and `hir/sites.rs` had 1,255 and 481 lines.

1. The boundary function-field rejection, divergence, r252, and reject registration were removed.
   The new test compares a local copy with a direct field call through the interop mirror.
   The checker, interpreter, and dev JIT passed. Ship C could not find `interop.h` in the test configuration.
   This test still needs the native fixture configuration.
2. Static field reads now apply the existing path narrowing, except during a write.
   The checker test passed, including unnarrowed, reassigned, and out-of-path rejection controls.
   The interpreter, dev JIT, and ship C produced `10`, `12`, and `null` on separate lines.
3. HIR call sites now use nullable parameter types for null-argument lifetime sites.
   Operation signature matching admits null and inner values for nullable parameters.
   The reference-class map test passed on all three engines: `7 true true`.
   The boundary map test passed the interpreter and dev JIT: `true true true`.
   Its ship C run also needs the native fixture include directory.
4. The requested Inbox acceptance test failed with S100 before `get` could use its value type.
   The diagnostic is `Worker, Inbox, and Outbox values may not be container type arguments`.
   `compiler/src/check/tyres.rs` rejects the value argument and replaces it with `Type::Error`.
   The resulting `get` also reports S014. The same declaration fails at `8bc6ded`.
   The handle predicate alone cannot make this program accepted.
   Work stopped under the handoff rule for a contract that cannot be met as written.
5. The r250 description, r251 exercise tag, and API group name were edited.
   Exact r250 message validation and document regeneration remain incomplete.
6. The new tests remain in the working tree, including the failing Inbox acceptance witness.
   The existing field invalidation defect remains open under §124, now referenced by §123.1 rule 8.

### Pin measurements

A fresh compiler build from an archive of `8bc6ded` used a separate target directory.
The static field checker test failed with S100. The Inbox test failed with S100 and S014.
A shared-target codegen measurement produced mixed dependency results and is not valid pin evidence.
A separate-target codegen build subsequently confirmed the pin failures:
- Boundary field call: S018, `SubRequestInfo` has no method `callback`.
- Static call and local copy: S100, the nullable function is not callable.
- Both null-default map tests: invalid LIR, a null argument disagrees with the operation signature table.

A separate-target checker probe confirmed that r250 and r251 were already rejected at `8bc6ded`.
Both received S014 with the scalar reason, at lines 9 and 8 respectively.
They are regression guards against a widened predicate, rather than new rejection decisions.

Compiler contract §40 explicitly prohibits affine types as Map values and other container arguments.
The fifth addendum's Inbox acceptance requirement conflicts with this rule.

### Current targeted results

The checker target had four passes and one failure (Inbox container type argument).
The codegen target had three passes and two failures (missing interop header for ship C).
No existing expected golden or LIR snapshot was edited.


### Fifth-addendum full gate

The pinned format check passed. The offline, locked all-target workspace build passed without warnings in 14.55 seconds.
The full gate ran once and failed. No retry or corrective code change followed.
Each profile had six failures:
- Two boundary tests could not find `interop.h` during ship C compilation.
- The Inbox acceptance test failed the existing Context-affinity rule.
- Two generated-document identity tests failed because regeneration remains incomplete.
- The LIR snapshot differed first at line 3718. Both texts contain 1,848,060 bytes.

No expected file or LIR snapshot was edited. The gate reported zero moved goldens.
Release emitted the existing dead-code warning for `Context::test_offset_live_bytes_counter`.

```text
gate full d1934bfb8ecd4ebfa05f32b90dd0b172e84aa9a5 dirty:22 debug 1814/6/3 release 1811/6/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 1
```

### Fifth-addendum file changes

- `compiler/src/check/expr/call.rs`: removed the boundary-field rejection.
- `compiler/src/check/expr/namespace.rs`: applies static field narrowing on reads.
- `compiler/src/hir/sites.rs`: derives null-argument sites from the nullable parameter type.
- `compiler/src/ambient.rs`, `compiler/src/api_reference.rs`, `compiler/src/check/expr/method.rs`: renamed the rejection group.
- `compiler/src/divergence.rs`: removed the boundary-call divergence and registration.
- `compiler/tests/corpus_reject.rs`: removed the r252 witness and mirror registration.
- `compiler/tests/function_value_reference.rs`: added static-field flow and Inbox/Outbox acceptance witnesses.
- `codegen/tests/function_value_reference.rs`: added boundary calls, static calls, and null-default map tests.
- `corpus/reject/r250-map-generator-get.ts`, `corpus/reject/r251-map-boundary-struct-get.ts`: edited header text.
- `corpus/reject/r252-boundary-function-field-call.ts`: removed the inherited untracked file.
- `specs/tracking/s123-function-value-reference.md`: recorded the measurements and the stop.

All other inherited changes remain. Generated documents still reflect the preceding addendum.

## Sixth addendum at 00d39e7

No git write command ran. The inherited working-tree changes remain.
The sixth addendum permits the item 3 snapshot changes described below.

The mailbox acceptance test was deleted. No compiler rule for Worker, Inbox, or Outbox changed in this addendum.
The §40 disagreement remains open under §123.1 rule 8. Field-path invalidation remains open under rule 9 and §124.

No split was required. Only `codegen/tests/function_value_reference.rs` gains Rust lines in this addendum; its initial length was 130 lines.

1. Both boundary tests now use `support/native_fixture.rs`, as the existing interop tests do.
   The fixture supplies the header directory, host C sources, and native symbols.
   The fixture module, mirror helper, and both boundary tests share the existing non-MSVC condition.
   The function-field test prints `local` and `field` on separate lines on all three engines.
   The nullable boundary Map test prints `true true true` on all three engines.
2. `generate-api-reference` regenerated the API reference, language reference, and corpus index.
   The language reference remained byte-identical. Both generated-document identity tests passed.
   The API group names the values with no shared nullable-pointer form. The corpus index no longer lists r252.
3. The LIR capture contains 33 changed lines, each replaced once: 33 removed and 33 added lines.
   No entry entered or left the snapshot. Both files contain 1,848,060 bytes.
   A line-by-line comparison confirmed identical instructions, operands, positions, and trap multisets.
   Every difference moves argument lifetime sites after call and raise sites.
   This order follows the item 3 change in `compiler/src/hir/sites.rs`, which derives sites after parameter selection.

   | Previous trap order | New trap order | Changed lines |
   |---|---|---:|
   | DevOnlyLifetime(0), Call | Call, DevOnlyLifetime(0) | 23 |
   | DevOnlyLifetime(1), Call | Call, DevOnlyLifetime(1) | 1 |
   | DevOnlyLifetime(0), Call, Raise(Propagate) | Call, Raise(Propagate), DevOnlyLifetime(0) | 9 |

   The entries contribute: a146, 1 line; a149, 15 lines; a171, 15 lines; a224, 2 lines.
   The operation families contribute: Map, 5 lines; Array, 23 lines; Set, 2 lines; ContextBytes, 3 lines.
   The inspected capture replaced `codegen/tests/lir-goldens/corpus.txt`. No existing `.expected` file changed.
4. Each required test target ran alone and passed before the full gate:

   | Target | Passed | Seconds |
   |---|---:|---:|
   | Compiler function_value_reference | 4 | 0.22 |
   | Codegen function_value_reference | 5 | 2.37 |
   | Compiler lib | 333 | 0.07 |
   | Codegen lir | 52 | 8.43 |
   | Compiler corpus_reject | 38 | 0.09 |

The new codegen tests assert exact output from the interpreter, dev JIT, and ship C.
The checker tests retain rejection controls for nullable field flow and capture boundaries.
The r250 header contains the representation reason; the reject target and generated checker witness pass.

Files changed in this addendum:
- `compiler/tests/function_value_reference.rs`: deleted the mailbox acceptance test.
- `codegen/tests/function_value_reference.rs`: configured the native fixture for both boundary tests.
- `generated-docs/api-reference.md` and `generated-docs/corpus-index.md`: regenerated the references.
- `codegen/tests/lir-goldens/corpus.txt`: copied the inspected capture.
- `specs/tracking/s123-function-value-reference.md`: recorded these measurements.

The pinned `cargo fmt --all --check` passed.
The offline, locked workspace build for all targets passed without warnings in 7.88 seconds.

### Sixth-addendum full gate

`tools/gate.sh full` ran once and passed. No retry ran.
Debug passed 1,819 tests; release passed 1,816 tests. Both profiles had zero failures and three ignored tests.
The format, build, TypeScript, and hygiene steps passed. Clippy baseline counts remain 5/18/13.
The moved-golden count is one: `codegen/tests/lir-goldens/corpus.txt`, with the 33 replacements recorded above.
No existing `.expected` file moved.

```text
gate full 00d39e78529d6c1b02270744b0b00fa1408a1943 dirty:23 debug 1819/0/3 release 1816/0/3 skips 2/0 clippy 5/18/13 goldens-moved 1 exit 0
```

## Seventh addendum at 00d39e7

No git write command ran. The inherited working-tree changes remain, except for the required LIR snapshot restoration.
No split was required. The Rust files that gain lines started below 2,000 lines:
`compiler/src/check/expr/method.rs` (1,529), `compiler/tests/function_value_reference.rs` (145), and `codegen/tests/function_value_reference.rs` (152).
`compiler/src/hir/sites.rs` keeps its 487-line length.

1. `map_get_value_ok` matches only the ReferenceClass, Map, Set, Func, and Array handle kinds after nullable unwrapping.
   Worker, Inbox, and Outbox fall outside this set.
   The new Worker rejection control failed before the fix: the inherited checker accepted the local map's `get(1)`.
   After the fix, it reports one S014 with the representation reason.
   The existing reason test retains the scalar reason and representation reason checks, with `getOr` acceptance controls.
2. Parameter types are computed before argument lifetime sites, Call sites, and Raise sites.
   Null arguments retain their nullable-parameter lifetime sites.
   The generated LIR capture equals HEAD byte-for-byte: 1,848,060 bytes.
   It replaced the inherited snapshot; all 33 inherited line replacements disappeared.
   `git diff HEAD -- codegen/tests/lir-goldens/corpus.txt` is empty. No existing `.expected` file changed.
3. The boundary nullable-map test prints both fields of the hit, plus the null tests.
   All three engines produce `true 7 8 true true`.
   The first test edit required explicit i64 casts for the conditional fallback values; the corrected test passes.
4. The boundary field-call test assigns a second callback after construction, before the local copy and field call.
   All three engines produce `updated local` and `updated field` on separate lines.
   The original callback has no `updated` prefix, so a stale read changes the expected output.

The checker target passed 5 tests in 0.22 seconds.
The codegen function-value target passed 5 tests in 1.50 seconds.
The codegen LIR target ran alone and passed 52 tests in 14.70 seconds.
The pinned `cargo fmt --all --check` passed.
The offline, locked workspace build for all targets passed without warnings in 10.55 seconds.

Files changed in this addendum:
- `compiler/src/check/expr/method.rs`: closed handle-kind predicate.
- `compiler/tests/function_value_reference.rs`: Worker rejection control beside the reason test.
- `compiler/src/hir/sites.rs`: restored trap-site order.
- `codegen/tests/function_value_reference.rs`: field-value and callback-replacement assertions.
- `codegen/tests/lir-goldens/corpus.txt`: restored generated bytes to HEAD.
- `specs/tracking/s123-function-value-reference.md`: recorded measurements.

### Seventh-addendum full gate

`tools/gate.sh full` ran once and passed. No retry ran.
Debug passed 1,820 tests; release passed 1,817 tests. Both profiles had zero failures and three ignored tests.
The format, build, TypeScript, and hygiene steps passed. Clippy baseline counts remain 5/18/13.
The release test build emitted the existing dead-code warning for `Context::test_offset_live_bytes_counter`.
The moved-golden count is zero. The LIR snapshot diff against HEAD remains empty.

```text
gate full 00d39e78529d6c1b02270744b0b00fa1408a1943 dirty:22 debug 1820/0/3 release 1817/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0
```
