# Shared-location narrowing

Contract: `compiler.md §124`.

## Measurement and owner decision

The initial checker comparison used the saved `7bc395d` checker and the rule 2 and 3 prototype.
It checked every source under `corpus/accept`, `corpus/warn`, `corpus/trap`, and `examples`.
Ambient declarations were inputs, not entry points. Both `a19-modules` sources counted as entries.
Each check supplied the four corpus interop mirrors and the generated engine mirror.

| Group | Sources | Baseline rejects | Prototype rejects |
| --- | ---: | ---: | ---: |
| Accept | 264 | 0 | 1 |
| Warn | 5 | 0 | 0 |
| Trap | 64 | 0 | 0 |
| Examples | 17 | 0 | 0 |
| Total | 350 | 0 | 1 |

The initial measurement is **1 of 350** newly rejected sources.
`corpus/accept/a137-handle-entry-param.ts:32:32` reports S011 for `AdoptedState | null`.
The first `adopted.advance()` call ends the module global's narrowing before the second call.

The owner kept rules 2 and 3 and authorized the `a137` source change.
`main` copies `adopted` to a `const` local before the null check.
Both calls use that local. Its existing golden is unchanged.

The checker accepts all **355** corpus and example sources, including `a274` and `t68` through `t71`.
No additional corpus or example source requires a change.

## Test sources changed by the rule

- `codegen/tests/function_value_reference.rs`: `static_nullable_function_path_calls_and_copies` checks `S.opt` again after its path call.
  Its next statement copies the narrowed field to a local. The expected output remains `10\n12\nnull\n`.
- `codegen/tests/reload.rs`: `registration_reload_source` copies `ping` to the `const` local `current` before the check.
  The foreign call `subRequestPump` can run a script callback. `fire` uses the local across that call.
  Its expected output is unchanged.

## Red and TypeScript results

Every listed reject entry is accepted by the saved `7bc395d` checker.
The contract also records acceptance of the first four shapes at `3d140e2`.
TypeScript measurements use `node_modules/.bin/tsc`, strict mode, ES2022, `ESNext.Disposable`, and `prelude/lang.d.ts`.

| Entry | Checker diagnostic | Line | TypeScript |
| --- | --- | ---: | --- |
| `r252-narrowing-across-call` | S011 | 13 | accepts |
| `r253-narrowing-alias-store` | S100 | 13 | accepts |
| `r254-global-narrowing-across-call` | S011 | 13 | accepts |
| `r255-narrowing-across-await` | S011 | 13 | accepts |
| `r256-narrowing-loop-call` | S011 | 15 | accepts |
| `r257-narrowing-loop-alias-store` | S011 | 14 | accepts |
| `r258-narrowing-loop-global` | S011 | 15 | accepts |
| `r259-narrowing-loop-await` | S011 | 15 | accepts |
| `r260-narrowing-try-join` | S011 | 14 | accepts |
| `r261-narrowing-field-initializer` | S011 | 15 | accepts |
| `r262-narrowing-generator-resumption` | S011 | 15 | accepts |
| `r263-narrowing-using-exit` | S011 | 15 | accepts |

At `7bc395d`, each entry from `r256` through `r263` terminates with signal 11 on the dev JIT and ship C.
The JIT CLI reports the child signal and exits 2. Each directly invoked ship executable returns signal 11.
The eight entries cite C17. `specs/blocks/collisions.md` retains its supplied contents.

TypeScript accepts `a274-narrow-again-after-a-call`.
Its interpreter, dev JIT, ship C, and Node output matches its golden: `again=7\ncleared\ncopy=7\n`.

## HIR effects and flow joins

The HIR script-execution fact matches every expression and statement kind without a wildcard arm.
It includes script, method, function-value, and foreign calls; callback built-ins; constructors and calling field initializers.
It includes generator resumption, `await`, `yield`, and disposal at a `using` scope exit.
Non-callback built-ins such as `slice`, `push`, and `new Map()` preserve a narrowing.
A lambda's creation does not execute its body.

Each subtree summary carries script execution, field stores, global stores, and local stores.
Construction includes the effects of evaluated field defaults.
A field store ends shared facts with that field segment, independent of the receiver.
A global store ends facts for that binding and its extensions.
Local and global path roots have distinct identities, including a local that shadows a global name.

A HIR resolution pass supplies complete class definitions and loop summaries to the flow check.
The flow check applies summaries before loop heads, at catch entry, and after `try`.
The loop summary includes the body, condition, and step. It also includes generator advancement.
The syntactic assignment-root prescan is removed.
`do…while` is outside the language and reports S100.
A `switch` applies no summary at dispatch. It intersects dispatch and fall-through facts and records break-edge facts.
Its exit intersects the case exits and applies their summaries. Case paths contribute ended-narrowing notes to the exit.

Declaration, assignment, and join boundaries clear obsolete C17 diagnostic notes.
A note remains only for a reported narrowing that a shared-location effect ended.

## Runtime read guards

`Expr::trap_sites` compares a shared read type with its declared field or global storage type.
A read of `T` from `T | null` storage carries `NullNarrowing`; a local read carries no such site.
LIR attaches the site to the read's type conversion after the storage load.
The conversion returns the original box address for a boundary value used as a place.
A value use loads the payload after that conversion. A place retains its read type and read traps separately from address traps.
The module verifier checks each nullable conversion from its LIR origin, operand type, result type, and required trap.
The LIR verifier rejects a shared-read site on any other operation or on incompatible conversion types.
A violating-form test replaces the guarded conversion operation with a global load without changing the trap record.

The runtime uses `SharedNullNarrowing = 31` with message `a narrowed shared location is null`.
Checked `as` narrowing keeps `NullNarrowing = 4` and message `` `as` narrowing applied to null ``.
Both have the diagnostic rule name `null-narrowing`. The generated host header exposes their distinct constants.
The language-reference generator describes both kinds.
The interpreter tests the loaded value. The JIT and C tier test the pointer, including a function value's code pointer.
Checker-synthesized helpers carry no script-execution effect.
A direct store clears its own narrowing without a C17 diagnostic note.
The `a137` header cites C17.

## Additional corpus measurements

The four trap entries pass TypeScript with strict mode, ES2022, ESNext.Disposable, and the ambient prelude.
At `7bc395d`, all five sources below pass the checker and terminate with signal 11 on the dev JIT.
The saved pin sources match the `7bc395d` archive for both compiler and codegen.

| Entry | Current result | Position | TypeScript |
| --- | --- | --- | --- |
| `t68-narrowing-accessor-compound` | null-narrowing | 13:34 | accepts |
| `t69-narrowing-static-accessor-compound` | null-narrowing | 13:34 | accepts |
| `t70-narrowing-optional-getter` | null-narrowing | 17:16 | accepts |
| `t71-narrowing-destructuring-getter` | null-narrowing | 16:16 | accepts |
| `r264-narrowing-switch-join` | S011 | 11:22 | rejects TS18047 at 11:22 |

All five trap entries produce no stdout before the trap. Their `.expected` files are empty standard-output goldens.
The trap harness pins the new shared-read kind and message for `t68` through `t72`; no standard-output golden changes.
The trap harness passes on both execution tiers. The interpreter corpus sweep also passes.

## Files and tests

The initial `check/mod.rs` split is committed at `7bc395d`.
In this working tree, `compiler/src/check/pipeline.rs` moves the checker pipeline and operation-signature functions from `check/mod.rs`.
In this working tree, `codegen/tests/cemit/operations.rs` moves test functions from `codegen/tests/cemit.rs`.
Both splits are pure moves. The conversion and switch corrections require no additional split.
Each Rust source that this correction adds lines to remains below 2,000 lines.

- Shared-narrowing checker tests: 22 passed, including helper, switch, direct-store-note, and HIR-site controls.
- Shared-narrowing engine and verifier tests: 6 passed; the function/global read test includes local-copy controls on all three engines.
- Reject-corpus tests: 38 passed.
- Compiler library tests: 332 passed.
- Trap-corpus harness: passed on the dev JIT and ship C.
- LIR tests: 52 passed, including interpreter trap coverage for `t68` through `t72`.
- Runtime trap tests: 10 passed. Host-header tests: 11 passed.
- Compiler library clippy: 3 warnings, with no additional warning.

The shared-read guard adds a `SharedNullNarrowing` site at snapshot line 2028, `a128-host-owned-state.ts:20:55`.
The captured snapshot was compared before replacement. No existing `.expected` golden changes.

## Boundary-place and dispatch measurements

The saved pin's compiler and codegen sources match the `7bc395d` archive byte for byte.
The pin accepts the boundary probes with the generated interop mirror.

| Shape | Pin interpreter | Pin dev JIT | Pin ship C | Corrected engines | TypeScript |
| --- | --- | --- | --- | --- | --- |
| bb3; `t72-narrowing-boundary-getter` | InvalidLir on null | signal 11 | signal 11 | null-narrowing at 13:44 | accepts |
| bb4; boundary field store | `9\n` | `9\n` | `9\n` | `9\n` on all three | accepts |
| bb5; boundary global store | `9\n` | `9\n` | `9\n` | `9\n` on all three | accepts |

The new corpus paths are `corpus/trap/t72-narrowing-boundary-getter.ts` and its `.expected` file.
The engine test pins bb4 and bb5. The trap entry uses bb3's destructuring getter and field store.
`t68` through `t72` pass the trap harness on both execution tiers and the interpreter trap sweep.

The x3 switch probe passes TypeScript and prints `7\n` on the pinned JIT.
The checker accepts its dispatch shape. A store before that read fires S011 in the same test.
The switch exit test removes a redundant null comparison from an already narrowed case entry.
Its exit read still reports S011 with C17 after a call in another case ends the shared fact.
`r264` reports TS18047 at 11:22 under TypeScript; its header cites no C17.

## Checker cost

Release measurements use three discarded warm-ups and eleven timed passes on each benchmark source.
The parser runs outside the timed spans. Values below are medians in milliseconds.

| Source | HIR resolution | Effect summary | Second checker pass |
| --- | ---: | ---: | ---: |
| a22 | 0.346750 | 0.061917 | 0.499541 |
| collect | 0.186500 | 0.052125 | 0.262500 |

The second pass remains: rule 3a requires complete HIR effects and class definitions before the checker reaches loop heads.
Runtime guards prevent null reads but do not produce those required checker rejections.
The second-pass span includes capture checks, operation signatures, index checks, and raise-site derivation.

## Paired runtime benchmark

Both release binaries ran `perf-gate --gate --warmup 3 --timed 11`, sequentially, without another command or edit during execution.
The baseline source is the `7bc395d` archive. Both runs return exit 0 and meet every performance threshold.
The harness applies its 200 ms warm-up floor and takes eleven timed samples per subject.
The host is aarch64 macOS with Apple clang 21.0.0, `-O2`, and `-ffp-contract=off`.
The a22 output and collect checksum match their expected values in both runs.

| Workload | Engine | Pin median ms | Current median ms | Change |
| --- | --- | ---: | ---: | ---: |
| a22 | ship C | 5.314 | 5.264 | -0.94% |
| a22 | dev JIT | 78.621 | 78.530 | -0.12% |
| collect | ship C | 34.895 | 34.826 | -0.20% |
| collect | dev JIT | 114.191 | 114.756 | +0.49% |

Dev iteration changes from 3.932 ms to 4.134 ms; hot reload changes from 0.503 ms to 0.518 ms.
All subject spreads stay below 20%. These are paired benchmark measurements, not an isolated per-guard cost.
The pinned formatter check and offline locked workspace all-target build pass. The build reports no warnings.

## Origin-form verifier

`NarrowNonNull(NarrowOrigin)` carries `SharedRead` or `Local`, without a default origin.
The origin and HIR trap derivation use the shared HIR path predicate. Trap derivation separately compares storage and read types.
The module verifier runs on every build. It requires `SharedNullNarrowing` for `SharedRead`.
An exhaustive instruction-kind match identifies conversions: `Copy`, `Cast`, `Coerce`, and `NarrowNonNull`.
Only `NarrowNonNull` can convert `Data(Nullable(T))` to `Data(T)` or the boundary address of `T`.
A call can take `T | null` and return `T`; it is not a conversion.
Both verifier checks use the instruction-kind classification. The `Coerce` signature no longer accepts a nullable boundary-address conversion.
The shared path predicate matches every expression kind and receiver type without a wildcard arm.
The site storage-type lookup still has a wildcard receiver arm; it returns no declared field type outside `Class`.
Verifier rule 3 applies only when the target matches the nullable inner type.
Type-changing `as` conversions retain `Cast`, `NullNarrowing`, and `ClassMismatch`.
No dominance analysis or lowering-time shared-read check remains.

The verifier sweep checks the corpus entries below and reports zero verifier rejections.
The earlier corpus omitted calls that take `T | null` and return `T`.
That sweep did not establish acceptance of those legitimate calls.

| Group | Verified | Checker rejects | Verifier rejects |
| --- | ---: | ---: | ---: |
| Accept | 265 | 0 | 0 |
| Warn | 5 | 0 | 0 |
| Trap | 69 | 0 | 0 |
| Reject | 0 | 247 | 0 |

The JIT, C emitter, and interpreter handle `NarrowNonNull` through their conversion implementations.
The JIT instruction dispatcher replaces its trap list with the list returned by `consume_lifetimes` before conversion dispatch.
Both conversion branches therefore test the consumed list.
The interpreter exposes the runtime trap identity separately from the rule string.
Shared-read and `as` traps report their distinct kinds and messages.

The six shared-narrowing tests pass on the new form.
The verifier tests strip the required site from `SharedRead`.
For both origins, they replace `NarrowNonNull` with `Copy`, `Cast`, and `Coerce`; the verifier rejects each replacement.
A `const` local copy passes as `Local` without a null trap.
The type-changing cast test retains both sites and reports the `as` null kind on all three engines.
Shared function and global reads report the shared kind; local copies retain their output.
Boundary field and global stores retain their box and print `9` on all three engines.

The captured LIR snapshot differs by four instruction renames only:

| Origin | Renamed instructions | Snapshot lines |
| --- | ---: | --- |
| SharedRead | 1 | 2028 |
| Local | 3 | 15383, 15392, 19752 |

Each renamed instruction was `Coerce`. All other snapshot bytes remain unchanged from the supplied working tree.
The LIR suite passes all 52 tests. No additional Rust split is required.

Compiler library tests pass: 332. Reject-corpus tests pass: 38. The trap harness passes on both tiers.
The pinned formatter check and offline locked workspace all-target build pass without warnings.

## Nullable-argument calls

`a275-nullable-argument-call` pins a function, a method, and a function value with nullable arguments and non-null results.
Each call runs with null and non-null inputs.
The fifth review measured `orNew(c).v` before this correction and reported:
`internal lowering error: ... nullable-to-value conversion requires NarrowNonNull: Call(...)`.
This Red result is the review measurement of the working tree, not a measurement of committed HEAD.

The interpreter, dev JIT, ship C, and Node match the new golden: `function=7\nmethod=7\nvalue=7\nfunction=9\nmethod=9\nvalue=9\n`.
TypeScript accepts the entry with strict mode, ES2022, ESNext.Disposable, and the ambient prelude.
The entry is js-comparable. The corpus index is regenerated through its generator.
The shared path predicate supplies the HIR shared-read classification.
The unused `AsCast` origin and the test that relabeled a local origin are removed.
Type-changing casts retain their existing sites. No existing golden or LIR snapshot changes in this correction.
No additional Rust split is required.

## Generator-result and shared-path measurement

`a276-generator-result-narrowing` null-checks and reads `r.value` from a generator's `next()` result.
The Red measurement uses the supplied working tree at HEAD `d548737`, before the shared-path correction.
Interpreter lowering, dev JIT, and ship C report `NarrowNonNull(SharedRead) requires SharedNullNarrowing`.
The pin measurement uses compiler and codegen libraries built from the `c001aeb` archive.
At that pin and after the correction, all three engines match `value=7\nnull2\n`.
TypeScript accepts the entry with strict mode, ES2022, ESNext.Disposable, and `prelude/lang.d.ts`.
Node matches the same golden. The entry declares `js-comparable: yes`.

`Expr::is_shared_location` is the shared predicate for checker kills, LIR origins, and read sites.
Its expression-kind and receiver-type matches have no wildcard arm.
Globals, static fields, reference-instance fields, and fields reached through `this` are shared.
Fields of local `IterResult` values and local value-class copies are local.
A value field reached through shared storage remains shared.
The checker records classifications in the binding's lexical scope.
Local field-store summaries retain their exact path; they do not end unrelated local field facts.
`NarrowOrigin` is a closed enum. The verifier matches both origins explicitly.
Terminator diagnostics distinguish `SharedNullNarrowing` from the checked-`as` `NullNarrowing` kind.

The verifier sweep reports zero rejections: 266 accept, 5 warn, and 69 trap entries verify.
All 247 reject entries fail in the checker.
The predicate test covers reference receivers, value receivers, iterator results, globals, and `this`.
The checker test pairs preserved generator-result facts with shared-field and local-assignment controls.
The terminator test attaches each narrowing kind to a trap terminator and checks its distinct diagnostic.
The checker tests pass: 23. The engine and verifier tests pass: 7.
The LIR tests pass: 53. The compiler library tests pass: 333.
The new generator entry has a dedicated LIR origin assertion with a shared-read control.
It stays outside the existing coroutine text snapshot. All existing snapshot bytes remain unchanged.
The corpus-index generator includes a276.
The pinned formatter check and offline locked workspace all-target build pass without warnings.
No Rust file needs a split. No existing output golden changes.

## Boundary boxes held in locals

HIR `Local` carries the local name and its declared storage type.
The checker preserves that type when the expression narrows.
Synthetic locals and disposal receivers also carry their storage type.
The shared predicate reads storage types for locals and fields.
A nullable boundary value denotes a shared box, including a box reached through a local value copy's field.
The copy's own field slot stays local.
The receiver-type match includes `Nullable(Class)` and lists every type explicitly.
The site derivation uses the same exhaustive field-declaration lookup.
`path_kills` factors the shared-path condition without a new clippy warning.

| Entry | Shape | Red at `7bc395d` | Corrected result |
|---|---|---|---|
| `r265-boxed-local-narrowing-call` | x5: a call clears a nullable field in a local boundary box | Accepted; JIT and ship C signal 11 | S011 at line 15, with the C17 note |
| `r266-boxed-local-narrowing-alias-store` | x6: an alias clears the boxed field | Accepted; JIT and ship C signal 11 | S011 at line 14, with the C17 note |
| `t73-boxed-local-narrowing-call` | A destructuring getter calls the clearing function | Accepted; JIT and ship C signal 11 | `SharedNullNarrowing` at 17:18 on all three engines |
| `t74-boxed-local-narrowing-alias-store` | A destructuring getter clears the field through an alias | Accepted; JIT and ship C signal 11 | `SharedNullNarrowing` at 21:18 on all three engines |

The pin compiler and codegen source files match the `7bc395d` tree by Git blob hashes.
The JIT reports its child signal 11. The standalone ship executables terminate with signal 11 (shell status 139).
Both trap entries have empty pre-trap stdout and the message `a narrowed shared location is null`.
`a277-boundary-value-copy-narrowing` retains the b1 and b4 by-value controls.
All three engines match `copy true\nb1 5\nb4 5\n`; the pin ship executable matches too.
TypeScript accepts all five entries with strict mode, ES2022, ESNext.Disposable, the prelude, and the interop mirror.
The accept entry cites C2 because boundary value copies differ from JavaScript references.

The verifier sweep has zero rejections: 267 accept, 5 warn, and 71 trap entries verify.
All 249 reject entries fail in the checker. No existing accepted entry needs a source change.
The checker suites pass: 13 accept tests, 38 reject tests, and 23 shared-narrowing tests.
The LIR suite passes 53 tests; the engine and verifier suite passes 7 tests.
The trap harness passes. The compiler library suite passes 334 tests.
The new predicate test pairs boxed and by-value locals, with narrowed and nullable receiver types.
It also checks shared access through a box stored in a local value's field.
Compiler-library clippy reports 3 existing warnings; `path_kills` reports none.
No Rust file needs a split. Existing output goldens and the LIR snapshot remain unchanged.
The corpus-index generator includes the five new entries.
The pinned formatter check and offline locked workspace all-target build pass without warnings.

### Files changed for the boundary-box correction

- `codegen/src/lir.rs`
- `codegen/src/lir/address_taken.rs`
- `codegen/src/lir/expr.rs`
- `codegen/src/lir/place.rs`
- `codegen/tests/cemit.rs`
- `codegen/tests/corpus/mod.rs`
- `codegen/tests/support/lir_facts.rs`
- `codegen/tests/support/lir_facts/boundary.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/expr.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/using_scope.rs`
- `compiler/src/hir.rs`
- `compiler/src/hir/effects.rs`
- `compiler/src/hir/expression.rs`
- `compiler/src/hir/shared.rs`
- `compiler/src/hir/sites.rs`
- `compiler/src/hir/tests.rs`
- `compiler/src/hir/using.rs`
- `compiler/src/lib.rs`
- `compiler/src/trap_sites.rs`
- `compiler/src/warn.rs`
- `compiler/tests/corpus/mod.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/nullable_using.rs`
- `compiler/tests/nullish.rs`
- `compiler/tests/shared_narrowing.rs`
- `compiler/tests/support/lifetime_sites.rs`
- `corpus/accept/a277-boundary-value-copy-narrowing.expected`
- `corpus/accept/a277-boundary-value-copy-narrowing.ts`
- `corpus/reject/r265-boxed-local-narrowing-call.ts`
- `corpus/reject/r266-boxed-local-narrowing-alias-store.ts`
- `corpus/trap/t73-boxed-local-narrowing-call.expected`
- `corpus/trap/t73-boxed-local-narrowing-call.ts`
- `corpus/trap/t74-boxed-local-narrowing-alias-store.expected`
- `corpus/trap/t74-boxed-local-narrowing-alias-store.ts`
- `generated-docs/corpus-index.md`
- `specs/tracking/s124-narrowing-kill.md`

## Full gate

The a277 header `js-comparable: no C2` lacked the required `: <reason>` suffix, which failed
`js_corpus::every_accept_entry_has_a_total_js_claim_and_comparable_output_matches`. With the reason added:

`gate full 1a681250e5c13bc7eb7216049a25cbff2628e4c3 dirty:116 debug 1854/0/3 release 1851/0/3 skips 2/0 clippy 3/18/13 goldens-moved 1 exit 0`
