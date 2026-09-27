# Lifetime operand measurements

## Baseline

This command passed without warnings before any file changed:

```text
cargo build --offline --locked --workspace --all-targets
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s
```

The probe sources and executable were in `$TMPDIR`.
The CLI measurements used `target/debug/subscript run <probe.ts>`.
They did not use the JIT entry point that enables freed-handle diagnostics.

The interpreter probe imported `codegen/tests/corpus/mod.rs` and called its `entry_sources` function.
It then called `check_program`, `lower_module`, and `interpret`, as `codegen/tests/lir.rs` does.
The probe linked `target/debug/libsubscript_compiler.rlib` and `target/debug/libsubscript_codegen.rlib` from the baseline build.

## Red measurements

Each probe used this source structure. The live control left line 4 empty.

```typescript
export function main(): void {
  let n: i32 = 1; print(`${n}`);
  // The setup from the table goes here.
  Context.free(x);
  // The operation from the table goes here.
  print("done");
}
```

Each setup occupied line 3. Each operation occupied line 5, with two spaces before the first token.

| Probe | Setup | Operation |
| --- | --- | --- |
| `map_copy` | `const x = new Map<i32, i32>(); x.set(1, 2);` | `const y = new Map(x);` |
| `set_copy` | `const x = new Set<i32>(); x.add(2);` | `const y = new Set<i32>(x);` |
| `array_from` | `const x = new Set<i32>(); x.add(1);` | `const y = Array.from(x);` |
| `set_union` | `const a = new Set<i32>(); const x = new Set<i32>(); x.add(2);` | `const y = a.union(x);` |
| `map_value` | `const a = new Map<i32, Set<i32>>(); a.has(0); const x = new Set<i32>(); x.add(2);` | `a.set(1, x);` |

All five CLI live controls exited 0 with stdout `"1\ndone\n"` and empty stderr.
All five freed-operand variants emitted W002 for `x` on line 5.
The table gives the execution result after that warning.

| Probe | CLI exit | CLI stdout | CLI trap |
| --- | --- | --- | --- |
| `map_copy` | 1 | `"1\n"` | `5:13: trap [internal]: Map/Set key kind Bits with width 0 is invalid` |
| `set_copy` | 0 | `"1\ndone\n"` | None |
| `array_from` | 0 | `"1\ndone\n"` | None |
| `set_union` | 1 | `"1\n"` | `5:13: trap [use-after-delete]: use of a deleted allocation` |
| `map_value` | 0 | `"1\ndone\n"` | None |

The source operand positions are `5:21`, `5:26`, `5:24`, `5:21`, and `5:12`, respectively.
Rule 3a excludes the stored argument in `map_value` from lifetime sites.

The corpus interpreter path returned these errors for both the live and freed variants:

| Probe | Pre-error stdout | Error |
| --- | --- | --- |
| `map_copy` | `"1\n"` | `invalid LIR: Map key has no layout` |
| `set_copy` | `"1\n"` | `invalid LIR: Set key has no layout` |
| `array_from` | `"1\n"` | `invalid LIR: Set key has no layout` |
| `set_union` | `"1\n"` | `invalid LIR: Set key has no layout` |
| `map_value` | `"1\n"` | `invalid LIR: Map key has no layout` |

All ten interpreter results were `InterpretError::Execution` with an `InvalidLir` source and no position.
The interpreter did not reach the operand use in any probe.

## Pin measurements

An archive of `e61e012` supplied a separate CLI and interpreter probe.
The offline locked build passed.
Both entry points repeated all ten variants above with the same results.
The archive and probe sources were in `$TMPDIR`.

The additional probes use `print("before")` on line 2.

| Operation | CLI, live | CLI, freed | Interpreter, live | Interpreter, freed |
| --- | --- | --- | --- | --- |
| `[0, ...x]`, where `x` is a Set | `before\ndone\n` | `before\ndone\n`, no trap | `Set key has no layout` | `Set key has no layout` |
| `a.reduce((acc: Set<i32>, v: i32): Set<i32> => acc, x)` | `before\ndone\n` | `before\ndone\n`, no trap | `before\ndone\n` | `before\ndone\n`, no trap |
| `a.reduce(keep, x)`, with the same callback in a local | `before\ndone\n` | `before\ndone\n`, no trap | `before\ndone\n` | `before\ndone\n`, no trap |
| `worker.post(x)`, where `x` is a message class | `done\n` | `done\n`, no trap | `Worker.Spawn requires a runtime worker adapter` | Same unsupported operation |

The spread and reduction operands are at `5:20`, `5:64`, and `5:28`.
The worker message operand is at `6:15`.
The worker probe follows the existing worker corpus profile, which excludes the interpreter.

`a270-numeric-container-copy` copies live numeric Map and Set sources.
At `e61e012`, the interpreter returned `InvalidLir: Map key has no layout` before output.
Node printed `false false\n`.
The new `.expected` contains these program bytes.
`node_modules/.bin/tsc -p tsconfig.json` passed with the new entry.

## Form and coverage

HIR lifetime sites carry `LifetimeOperand::Receiver` or `LifetimeOperand::Argument(index)`.
LIR lifetime traps carry the evaluated operand index.
Call lowering accounts for the receiver prefix.
The LIR verifier rejects an index outside the operand list.

`Expr::trap_sites` derives the sites in operand order.
Runtime parameters that copy an element, key, or value have no lifetime site.
Script, indirect, and foreign call arguments have no lifetime site.
`Context.free` has a distinct release site and retains its call position and pending delete diagnostic.

The dev JIT consumes named lifetime operands before the instruction effect.
The interpreter tests the named operand only.
The C emitter consumes lifetime sites without a check.
The loop forms for static array callbacks and Map/Set `forEach` retain their source sites.

New coverage includes constructor sources, Set algebra arguments, array spread sources, and runtime receivers.
It also includes reduction accumulators, worker messages, and synthesized-helper reference operands.
Array concatenation, regex, and Context byte-array sites exist, but their operand types reject `Context.free`.
The signature test covers runtime-operation tables across the corpus and examples.
The CLI tests cover Map and Set copies, Array.from, all seven Set algebra methods, spread, receivers, reductions, worker messages, and JSON helpers.

`Type::contained_types` matches every type variant without a default arm.
The interpreter collects these children recursively before execution.
The type test covers every leaf, every unary container, Map, Worker, and function parameter/result types.

## Green measurements

The CLI and interpreter returned `use-after-delete` at `5:21`, `5:26`, `5:24`, and `5:21` for the four original read probes.
All live controls returned `1\ndone\n`.
The freed Map value store still returned `1\ndone\n` with no trap.
A separate test stores the freed value, then reads it and traps at the later read position.

## Tests

- `subscript-cli --test lifetime_operands`: 10 tests passed; 1.85 seconds.
- `subscript-codegen --test lifetime_operands`: 9 tests passed; 0.03 seconds.
- `subscript-codegen --test lir`: 50 tests passed; 8.26 seconds.
- `subscript-compiler --test operation_signatures`: 9 tests passed; 0.88 seconds.
- `contained_types_cover_the_complete_grammar`: passed.

The signature check compares operation parameter contracts with sites from constructed call expressions.
Its firing control changes an argument type in the expression, without a change to the signature record.
The LIR firing control removes an operand, without a change to its lifetime site.
The CLI tests use the executable supplied by Cargo to the CLI test target.
Probe sources stay in `$TMPDIR`.

## LIR snapshot

The required capture command passed.
The snapshot has 22,859 lines before and after the change.
380 existing read sites gained an operand index.
One existing release site became `DevOnlyRelease(0)`.
61 lifetime sites are new.
441 lines changed; one changed line contains both kinds of change.
Removal of read and release lifetime traps from both snapshots gives identical text.
The final capture differs from the inherited snapshot only at the one release site.
The captured file replaced `codegen/tests/lir-goldens/corpus.txt`.

## Benchmark measurements

The release `perf-gate --gate` run passed every threshold.
All subjects stayed within the stated median spread limit of 20 percent.

| Subject | Measurement | Limit |
| --- | --- | --- |
| a22 dev JIT / C | 20.03; 79.828 ms median | 25.00 |
| collect dev JIT / C | 3.60; 118.023 ms median | 8.50 |
| a22 ship C / C | 1.36 | 1.50 |
| collect ship C / C | 1.13 | 7.50 |
| dev iteration | 3.741 ms | 20.000 ms |
| hot reload | 0.482 ms | 20.000 ms |

These are post-change measurements, not a paired estimate of the added checks alone.

## Lifetime trap position assertion

`map_and_set_use_after_delete_trap` expects `("test.ts", 4, 12)` for both Map and Set.
Column 12 names the operand `value`; column 18 names the member `size`.
No other member-position expectation required a change.

## Synthesized helpers and release sites

Every synthesized-helper call derives read sites at its argument positions, under compiler.md §120.1 rule 4.
The rule uses the function's `synthesized_helper` fact, without a helper-name test.
The CLI JSON test uses one `i32` field. Diagnostics mode is off (§8.1a-1).
Its live control prints `done\n`; its freed operand traps at `5:18`, the position of `x`.

HIR `DevOnlyRelease` and LIR `DevOnlyRelease(index)` distinguish release checks from read checks.
The interpreter and JIT select release diagnostics from this site, without an operation-name test.
Release diagnostics retain the call position. The C emitter consumes the site without a check.
The JIT consumes read lifetime sites once, before the instruction effect.
The operation-specific lifetime emission paths are removed.
The narrowing check retains its position before lifetime sites, under §20.2 and §120.1 rule 2.

CLI tests reside in `cli/tests/lifetime_operands.rs` and use `env!("CARGO_BIN_EXE_subscript")`.
Interpreter and LIR-verifier tests remain in `codegen/tests/lifetime_operands.rs`.
The expected stored-parameter indices come from generic API signature strings through the API reference projection.
The signature test parses `T`, `K`, and `V` parameters and includes `BuiltinMethod::ArrayPush`.
It contains no duplicate stored-operation list.

## Rejected freed operand classes

Each source was measured through `subscript run`. Each command exited 1 with one S100 diagnostic and no stdout.
The probe sources stayed in `$TMPDIR`.

| Class | Setup and use | Diagnostic at `Context.free(x)` |
| --- | --- | --- |
| Array concatenation | `const x: i32[] = [2]; a.concat(x);` | `type mismatch: the argument expects \`object\`, got \`i32[]\`` |
| Regex operand | `const x = /a/; "a".search(x);` | `type mismatch: the argument expects \`object\`, got \`RegExp\`` |
| Context byte-array operand | `const x: u8[] = [0, 0, 0, 0]; Context.fromBytes<Word>(x, 0);` | `type mismatch: the argument expects \`object\`, got \`u8[]\`` |

`Word` is `@CStruct class Word { value: i32 = 0; }`.
These operand types cannot reach their operations after a program frees them.
These diagnostics replace freed-operand tests under §120.2 item 2.

## Validation

The pinned-toolchain `cargo fmt --all --check` passed.
The offline locked workspace all-target build passed without warnings in 15.10 seconds.
The LIR runtime-kind mapping tests passed: 2 tests.

The full gate recorded three failed golden tests in each profile.
The reload test executable also exited with signal 11 in each profile.
The debug suite took 430 seconds. The release suite took 548 seconds.
The full interpreter corpus comparison passed in both profiles.
The release performance gate passed its thresholds.
TypeScript and hygiene passed.
The added warning is `single_match` at `codegen/src/lower/func/builtin.rs:211`.

The golden sweep reported missing lifetime-site consumption for these entries:
`a110`, `a111`, `a143`, `a145`, `a149`, `a187`, and `a258`.
For `a110`, LIR carries 16 sites, but the JIT consumes 15.
The missing site is `DevOnlyLifetime(0)` at `27:37`.
`create_async_child` calls `create_async_child_from_values` from the suspension path without `emit_instruction`.
The coroutine lifetime-emission path is reachable from suspension.
One shared consumer must also cover suspension sites.

`a69-json-stringify` and `a70-json-roundtrip` exited with signal 11 in the JIT.
Both entries serialize null reference-class values.
Source inspection shows that the new helper site reaches `live_check`, which reads the header without a null guard.
The signal's exact instruction was not measured.
The reload executable's signal was not isolated to one test.

| Finding | Result |
| --- | --- |
| 1. Synthesized helper arguments | The CLI helper test passes at the operand position. Existing JSON entries fail with signal 11. |
| 2. Unreachable freed operand classes | Array concatenation, regex, and Context byte-array S100 diagnostics are recorded above. |
| 3. Distinct release form | HIR and LIR carry `DevOnlyRelease`; engine diagnostic selection uses the form. |
| 4. One JIT lifetime consumer | Duplicate instruction paths are removed. Suspension sites remain unconsumed. |
| 5. CLI executable path | CLI tests use Cargo's executable path in the CLI package. |
| 6. Independent expected parameters | The signature test derives stored parameters from API strings, including ArrayPush. |
| 7. Null-check ordering | A comment cites the required site order and the lifetime header read. |
| 8. Tracking content | The record contains changes, measurements, validation results, and the gate verdict. |

## Files changed

- `cli/tests/lifetime_operands.rs`
- `codegen/src/cemit/access.rs`
- `codegen/src/cemit/arith.rs`
- `codegen/src/cemit/call.rs`
- `codegen/src/cemit/graph.rs`
- `codegen/src/cemit/intrinsic.rs`
- `codegen/src/cemit/suspend.rs`
- `codegen/src/interpreter.rs`
- `codegen/src/lib.rs`
- `codegen/src/lir.rs`
- `codegen/src/lir/builder.rs`
- `codegen/src/lir/call.rs`
- `codegen/src/lir/exception.rs`
- `codegen/src/lir/lowering.rs`
- `codegen/src/lir/place.rs`
- `codegen/src/lir/stmt.rs`
- `codegen/src/lir/verify_instruction.rs`
- `codegen/src/lir/verify_lifetime.rs`
- `codegen/src/lir/verify_raise.rs`
- `codegen/src/lir_types.rs`
- `codegen/src/lower/func/builtin.rs`
- `codegen/src/lower/func/coroutine.rs`
- `codegen/src/lower/func/expr.rs`
- `codegen/src/lower/func/instruction.rs`
- `codegen/src/lower/func/intrinsic.rs`
- `codegen/src/lower/func/place.rs`
- `codegen/src/lower/func/value.rs`
- `codegen/tests/lifetime_operands.rs`
- `codegen/tests/lir-goldens/corpus.txt`
- `codegen/tests/lir.rs`
- `codegen/tests/support/lir_facts.rs`
- `codegen/tests/support/lir_facts_lifetime.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/hir.rs`
- `compiler/src/lib.rs`
- `compiler/src/lifetime.rs`
- `compiler/src/lir.rs`
- `compiler/src/types.rs`
- `compiler/tests/operation_signatures.rs`
- `compiler/tests/support/lifetime_sites.rs`
- `corpus/accept/a270-numeric-container-copy.expected`
- `corpus/accept/a270-numeric-container-copy.ts`
- `generated-docs/corpus-index.md`
- `runtime/src/context.rs`
- `runtime/src/trap.rs`
- `specs/tracking/s120-lifetime-operand.md`

## Isolated failure measurements

The CLI build passed before these measurements. No production code changed for these measurements.
Each JSON entry ran alone through `subscript run`.
Both `a69-json-stringify` and `a70-json-roundtrip` exited 2 with `dev-JIT child signal 11`.
The reload test ran alone with `--test-threads=1`.
It exited 101 after SIGSEGV in `reload_mode_reproduces_every_committed_golden`.
Separate `ReloadSession` probes for a69 and a70 both exited 139.

The LIR dump identifies these outer helper argument sites:

| Entry | Position | Site | Operand type |
| --- | --- | --- | --- |
| a69 | `78:24` | `DevOnlyLifetime(0)` | `Nullable(Class(ClassId(2)))`, `Person | null` |
| a70 | `110:63` | `DevOnlyLifetime(0)` | `Nullable(Class(ClassId(2)))`, `Person | null` |

Both operands hold null at these calls.
The corresponding root helper also has `DevOnlyLifetime(1)` on its nullable value argument.
Index 0 in the root helper is its builder argument; index 1 is correct for the value.
The outer calls have no receiver prefix.

A reduced probe declares `class Item { value: i32 = 1; }`.
Its main function declares `const x: Item | null = null;` and calls `print(JSON.stringify(x));`.
Its outer site is `DevOnlyLifetime(0)` on `Nullable(Class(ClassId(1)))` at `4:24`.
A standalone reload probe exits 139.
With `new Item()` in place of null, the same nullable type prints `{"value":1}\n` and exits 0.
Probe sources, LIR dumps, and logs stay in the temporary directory.

`Type::handle_kind` maps this nullable type to `ReferenceClass`.
`HandleKind::needs_lifetime_trap` returns true for that kind.
Thus, rule 3 requires the site, and both the argument type and index are correct.
The JIT `live_check` reads `STATE_OFFSET` from the null pointer without a null guard.
The interpreter's lifetime check already excludes null handles.
A null guard in the common JIT lifetime consumer addresses this cause without a JSON-specific exception.
Deletion of nullable sites omits the required check when the same typed operand holds a freed allocation.

The amended §120.1 rule 2 requires null operands to pass without a header read.
The common JIT lifetime check branches past the header read when its operand is null.

## Test count accounting

The debug pass count changed from 1780 to 1757 as follows:

- Eight CLI cases became separate tests while their interpreter cases remained.
- Three tests were added: JSON helper arguments, helper form facts, and distinct release sites.
- One duplicate stored-role test was removed.
- Three golden tests changed from pass to failure.
- SIGSEGV prevented the reload binary from emitting its 30-test result summary.

The gate sums `test result:` summaries, not individual test output lines.
Thus, `1780 + 8 + 3 - 1 - 3 - 30 = 1757`.
The reload binary still lists 30 tests.
The release count follows the same change: `1777 - 23 = 1754`.

## Null operands and suspension consumption

`consume_lifetimes` accepts the trap sequence and evaluated operands.
Both `emit_instruction` and the suspension path in `create_async_child` call this consumer.
Each caller passes the remaining sites to its operation emitter.
The suspension path does not duplicate the lifetime rule.

`verify_trap_consumption_for` compares every expected LIR site with one consumed site.
Missing or extra sites return `trap-consumption mismatch`, including the function, position, and unmatched sites.
This check covers suspension sites and rejects both omissions and duplicate consumption.

`live_check` branches on null before the header load.
Null passes; non-null operands retain the live-state check and the source position.
The interpreter already excludes null. The C emitter still emits no lifetime check.

`nullable_json_helper_argument` runs through Cargo's CLI executable.
Its null `Person | null` operand prints `null\n` and exits successfully.
Its freed non-null control traps at `5:24`, the operand position, with no stdout.
The class has one `i32` field. The CLI uses diagnostics mode off (§8.1a-1).
All 11 CLI lifetime tests pass in 1.93 seconds.

`builtin.rs` uses an `if` for the stale-coroutine site instead of a single-arm `match`.

The isolated golden test target passes: 35 tests, 120.53 seconds.
This includes the seven async entries and the two JSON entries from the failure measurements.
The pinned formatter check passes. The workspace all-target build passes without warnings in 10.48 seconds.
The `single_match` warning is absent.
The isolated reload target passes: 30 tests, 2.91 seconds.
The LIR golden comparison passes: one test, 1.40 seconds.
The null guard and shared suspension consumer make no additional LIR snapshot change.

## Final validation

The full gate passes at `2acb2bf`.
Debug reports 1,791 passed, zero failed, and three ignored tests in 417 seconds.
Release reports 1,788 passed, zero failed, and three ignored tests in 522 seconds.
The added nullable test raises each pass count by one.
The three golden tests and the 30-test reload summary restore 33 passes in each profile.
Thus, the debug count changes from 1,757 to 1,791; release changes from 1,754 to 1,788.

The release performance test passes every threshold.
That test captures benchmark output and prints it only on failure; the gate records no new individual benchmark values.
The benchmark table above retains its earlier measurements.
The full interpreter comparison, TypeScript, and hygiene pass.
The one modified golden is the inherited LIR snapshot.

These files contain the null and suspension changes:

- `codegen/src/lower/func/value.rs`
- `codegen/src/lower/func/instruction.rs`
- `codegen/src/lower/func/coroutine.rs`
- `codegen/src/lower/func/builtin.rs`
- `cli/tests/lifetime_operands.rs`
- `specs/tracking/s120-lifetime-operand.md`

## Total read-through verification

The verifier matches every `InstructionKind` without a wildcard arm.
It derives operand requirements from instruction semantics and operand types, independently of attached sites.
A missing read site reports `read-through operand 0 has no lifetime site`.
The violating-form test constructs `IteratorCreate` without traps; the verifier rejects it.

`for…of` obtains its subject site from the HIR statement-read rule, before cursor creation.
The verifier also found these missing read sites:

- Array length expressions and foreign-call array data/length snapshots.
- Unchecked array indexing and repeated array-address resolution after an earlier read.
- Error `name` and `message` loads for `throw`.
- Async-owner field loads before object release.
- Async-handle and async-handle-array retain/release operations.
- The Map `forEach` key cursor and indexed `reduceRight` reverse cursor.
- Array pushes generated for static callback results.

Field-address computation itself does not read the payload.
Script constructors pass their receiver; their bodies carry the field-access sites.
These operations do not require a new read site solely for address calculation or argument transfer.

The HIR/LIR fact test requires equal site counts.
It derives additional statement and expanded-operation sites from HIR semantics.
The lifetime verifier checks counted retain and release instructions independently.
The async release verifier still requires exactly one non-lifetime `Call` site.

The storage exemption follows §120.1 rule 3a: only handle stores omit argument sites.
Array searches and Map fallback arguments now carry sites.
The signature test uses API parameter types and API storage-effect summaries as its independent facts.
Its outer match covers every signature group; Worker message arguments remain read-through.

The C emitter uses one lifetime-site consumer for instruction and suspension paths.
It emits no lifetime guard.
`CallbackUserdataFreed` maps to `Call`, not `DevOnlyRelease`.
`LifetimeOperand` and its implementation reside in `compiler/src/lifetime.rs`, with the HIR re-export preserved.

## Statement CLI measurements

The CLI lifetime target passes 16 tests in 1.25 seconds.
Map keys, Map values, Set values, Set keys, and explicit Set values trap at `5:19` after release.
Every corresponding live control prints `before\ndone\n`.
Both JSON helper tests use one-field classes; their freed controls trap at `5:18` and `5:24`.
The null control prints `null\n`.
Array search arguments and Map fallback arguments trap at their argument positions after release.

Array values, array keys, fixed-array values, and string code-point traversal pass with live subjects.
Their released-subject programs fail at `Context.free(x)` with S100:

| Subject type | Diagnostic |
| --- | --- |
| `i32[]` | `the argument expects \`object\`, got \`i32[]\`` |
| `FixedArray<i32, 2>` | `the argument expects \`object\`, got \`FixedArray<i32, 2>\`` |
| `string` | `the argument expects \`object\`, got \`string\`` |

These diagnostics replace unreachable freed-subject tests under §120.2 item 2.
Reverse cursors are internal LIR kinds, not additional source `ForOfKind` variants.

## Additional LIR snapshot sites

The inherited and captured snapshots each contain 22,859 lines.
The capture adds 171 read sites on 171 lines; no existing site changes or disappears.
Read-site counts change from 441 to 612.
Removal of read/release sites leaves identical snapshot text.

| Instruction | Added sites |
| --- | ---: |
| AsyncHandleRelease | 113 |
| AsyncHandleRetain | 5 |
| AsyncHandleArrayRelease | 2 |
| LoadField | 23 |
| Length | 6 |
| AddressOfField | 5 |
| AddressOfIndex | 3 |
| IteratorCreate | 9 |
| Call | 4 |
| ForeignArrayData | 1 |

The nine cursor sites comprise four ArrayValues, two ArrayKeysReverse, two MapValues, and one MapKeys.
The signature target passes nine tests in 0.89 seconds.

## Gate prerequisites

The isolated golden target passes 35 tests in 118.06 seconds.
The isolated reload target passes 30 tests in 3.79 seconds.
The LIR target passes all 50 tests in 8.39 seconds, including snapshot comparison and the interpreter corpus.
The interpreter lifetime target passes 10 tests in 0.03 seconds, including the missing-site violating form.
The workspace all-target build passes without warnings in 20.46 seconds.
The pinned formatter check passes.

## Full gate result at d6d1c8a

The full gate reports 1,794 passes and three failures in debug, and 1,791 passes and three failures in release.
Each profile retains three ignored tests.
The same three library tests fail in both profiles:

- `lir::verifier_tests::entryless_module_is_valid`: its hand-built array address instruction has no lifetime site.
- `lir::verifier_tests::valid_address_graph_passes`: the same instruction violates the new read-through contract.
- `lir::verify_raise::tests::a_handle_release_without_its_check_is_rejected`: its initial assertion requires exactly `[Call]`, without the new lifetime site.

The golden comparisons, interpreter corpus, reload target, and release performance thresholds pass.
TypeScript and hygiene pass.
The final prerequisite workspace build passes without warnings in 7.31 seconds.

Against HEAD, the snapshot changes 612 lines.
380 existing read sites gain operand indices; one release site becomes `DevOnlyRelease(0)`; 232 read sites are new.
The 232 new sites comprise 61 inherited additions and 171 additional sites.

## Release-field checks and reverse verification

An async-owner field load before `Context.free` carries `DevOnlyRelease(0)` at the call position.
The dev JIT and interpreter test the site before the load.
Both use the named operand and report `double-delete`.
The runtime and emitted check use the canonical message `Context.free of an already-deleted allocation`.
The C emitter consumes the site and emits no check.

The CLI test uses a `Holder` with one `Promise<i32>` field and a scalar-field control.
Both second releases report `double-delete` at `5:3`.
Both single-release controls print `done\n`.
The interpreter test confirms the same positions and results.

The verifier rejects a site whose operand is neither read-through nor guarded.
`AddressOfField` permits a guard without requiring a read site.
A `Copy` handle alias permits the argument guard before a static reduction loop.
A missing intrinsic operation record is an error.
The violating forms add a site to a stored array element and remove an intrinsic record.
Both forms fail verification.

The HIR/LIR fact comparison uses equality.
The expected side counts statement reads, repeated addresses, foreign-array snapshots, owner-field release reads, and expanded callback operations.
It derives these counts from HIR, independently of the emitted sites.
Counted retains and releases use their separate store and lifetime verifiers.
A duplicate expression lifetime site fails the fact comparison.

Callback lowering computes the expression trap list once.
The checked module records synthesized-helper names once.
Lowering and verification each compute their class handle-kind table once per module.

The LIR snapshot changes one additional line: `a260` changes its owner-field site from read to release.
The operand remains 0. The position changes from `26:16` to `26:3`.
Both snapshots contain 22,859 lines.

## Address and release fixtures

`entryless_module_is_valid` and `valid_address_graph_passes` carry the array-address lifetime site that lowering requires.
`a_handle_release_without_its_check_is_rejected` asserts the `Call` site and the operand-0 lifetime site separately.
It still rejects the release after removal of its runtime check.

## Validation measurements

The isolated golden target passes 35 tests in 116.77 seconds.
The isolated reload target passes 30 tests in 3.01 seconds.
The isolated codegen library target passes 231 tests in 9.63 seconds.
The CLI lifetime target passes 17 tests in 1.26 seconds.
The interpreter lifetime target passes 13 tests in 0.03 seconds.
The duplicate-site fact test passes in 0.01 seconds.

## Paired release benchmarks

The baseline is HEAD `8d86156b9c2f8b8db25e4a574b40692917a99ef1`.
Both builds ran `perf-gate --gate --warmup 3 --timed 11` on aarch64 macOS with Apple clang 21.0.0.
Each subject used at least three warm-up runs and 200 ms of measured warm-up, then eleven timed runs.
The numbers below are medians. Both runs passed every gate criterion and the 20 percent spread check.

| Subject | HEAD median (ms) | Tree median (ms) | HEAD / C | Tree / C |
| --- | ---: | ---: | ---: | ---: |
| a22 C | 3.984 | 3.961 | 1.00 | 1.00 |
| a22 ship C | 5.259 | 5.261 | 1.32 | 1.33 |
| a22 dev JIT | 77.737 | 78.675 | 19.51 | 19.86 |
| collect C | 32.475 | 32.530 | 1.00 | 1.00 |
| collect ship C | 35.009 | 35.277 | 1.08 | 1.08 |
| collect dev JIT | 114.315 | 115.107 | 3.52 | 3.54 |
| dev iteration | 3.738 | 3.930 | — | — |
| hot reload | 0.474 | 0.500 | — | — |

The dev-JIT median increases are 1.21 percent for a22 and 0.69 percent for collect.
The dev-iteration and hot-reload increases are 5.14 percent and 5.49 percent.
The iteration and reload limits are each 20 ms.

The LIR target passes 51 tests in 8.41 seconds, including the snapshot and exact execution-fact comparisons.
The signature target uses a checker-created helper and a script-function control.
The workspace all-target build and pinned formatter check pass without warnings.

## Full gate result at 8d86156

The debug and release suites pass. TypeScript and hygiene pass.
The modified golden is the LIR snapshot.


## Site-only JIT checks and operand types

The JIT removes five direct checks from Map.groupBy, Map methods, Set methods, and Set algebra arguments.
The `live_check` and `lifetime_check` methods no longer exist.
The header check is inline in `emit_trap`, which records each consumed site.
No emitter can call the header check by name.
The lifetime operand dispatcher is private to `value.rs`.

The verifier requires each read or release site to name a type that needs a lifetime trap.
It uses the same type predicate when it finds missing read sites.
A guarded `Copy` must preserve its operand's lifetime-trap type in its result.
This type-preserving handle alias is the form fact for the static-reduce argument guard.
Scalar copies and copies with different result types do not permit a guard.
The scalar violating-form test checks both read and release sites and rejects the unconditional Copy guard.
The existing reduction test covers live and freed handle accumulators on static and dynamic callbacks.

The paired benchmark table retains the measured HEAD and tree runs.
The LIR snapshot and existing `.expected` files have no additional changes.

## Site and type validation

Each test target ran alone before the full gate.

| Target | Passed | Test time (seconds) |
| --- | ---: | ---: |
| Golden | 35 | 121.94 |
| Reload | 30 | 2.99 |
| Codegen library | 231 | 8.12 |
| Interpreter lifetime | 14 | 0.02 |
| CLI lifetime | 17 | 1.25 |
| LIR snapshot | 1 | 1.41 |

The pinned formatter check passes.
The workspace all-target build passes without warnings in 11.01 seconds.
Runtime-library Clippy reports 18 warnings.

The full gate passes debug, release, Clippy, TypeScript, and hygiene.

```text
gate full 8d86156b9c2f8b8db25e4a574b40692917a99ef1 dirty:46 debug 1803/0/3 release 1800/0/3 skips 2/0 clippy 5/18/13 goldens-moved 1 exit 0
```
