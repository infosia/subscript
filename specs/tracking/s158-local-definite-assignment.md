# §158: local definite assignment

## Contract pin and Red

Pin: `215bf8b9` (amended from `5cc2afd1`; the amendment changes only spec text). The CLI build used `cargo build --offline --locked -p subscript-cli` before any implementation change.

Every remaining table row passed the HEAD CLI with an initializer.
The checks included all three exit branches: `throw`, `return`, and `unreachable()`.

The initial HEAD check rejected `a331-local-definite-assignment.ts` with 43 S100 diagnostics.
Every diagnostic said `local declarations require an initializer` and carried `LocalInitializerMissingForm`.
All columns were 7. The declaration lines were:

```text
11, 16, 21, 26, 31, 36, 41, 46,
51, 56, 61, 66, 71, 76, 81, 86,
91, 96, 101, 106, 111, 116, 121, 126,
131, 136, 141, 146, 151, 156, 161, 166,
171, 176, 181, 186, 191, 196, 201, 206,
211, 217, 218
```

The final entry adds chained assignment for all five types.
A CLI rebuilt from the unchanged pin rejects it with 53 instances of the same S100 diagnostic.
All columns remain 7. The final declaration lines are:

```text
11, 16, 21, 26, 31, 36, 41, 46, 51
52, 56, 61, 66, 71, 76, 81, 86, 91
96, 97, 101, 106, 111, 116, 121, 126, 131
136, 141, 142, 146, 151, 156, 161, 166, 171
176, 181, 186, 187, 191, 196, 201, 206, 211
216, 221, 226, 231, 232, 236, 242, 243
```

## Form and consumers

`ExprKind::Unassigned` identifies zero storage without a source initializer.
This fact separates an unassigned local from a generated, initialized `Zero` value.
The annotation supplies the storage type. The marker creates no non-null flow fact.
A rejected annotation poisons the local name before the assignment analysis.

`assignment_flow` replaces the two HIR field-flow walkers in `rejection_facts`.
The same walker computes normal-exit assignment and assignment before a read for fields and locals.
The strict §108 constructor prefix rule remains in its existing consumer.
The earlier AST Descriptor shape check remains before HIR construction.

The walker tracks assignment order, conditional paths, switch fallthrough, scope shadowing, loop exits, and catch entry.
A compound assignment or update reads its target. It cannot establish the first definite assignment.
A lambda owns a separate analysis. C5 still rejects a mutable capture.

The lowering maps `Unassigned` to the existing typed LIR `Zero` instruction.
The interpreter, JIT, and C emitter consume that instruction.
HIR visitors and aggregate frame accounting include the marker.

The local declaration rejection site retires.
`LocalReadUnassigned` carries `TscRejects` and reports the read position, name, and contracted remedy.
`LocalTypeWithoutInitializer` carries a separate annotation restriction.
The parser rejects a const declaration without an initializer with TS1155.
The unreachable checker const site is removed.
The module-variable restriction remains at its existing C24 site.

## Corpus and measured TypeScript results

TypeScript: 5.9.2. Node: v24.18.0. All checks use the repository compiler options and prelude.

| Table row | TypeScript result | Coverage |
|---|---|---|
| Both `if` branches assign | accepts | a331 and unit control pair |
| One `if` branch assigns | TS2454 | r359 and unit control pair |
| Other branch throws, returns, or calls `unreachable()` | accepts | a331 and three unit control pairs |
| Every switch case and default assigns | accepts | a331 and unit control pair |
| Try and catch both assign | accepts | a331 and unit control pair |
| `while (true)` assigns before its break | accepts | a331 and unit control pair |
| Only the for body assigns | TS2454 | r360 and unit control pair |
| Only the for-of body assigns | TS2454 | r361 and unit control pair |
| Conditional assignment, then compound assignment | accepts | a331 and unit control pair |
| Chained assignment | accepts | a331 and unit control pair |

`a331` covers each accepted control-flow row for `i32`, `f64`, `string`, a reference class, and a nullable reference.
Node generated its golden. No existing `.expected` file changed.
`r362` pins the module-variable rejection. An uncalled function contains its assignment, and main reads the variable. TypeScript accepts this shape.

The path test checks 116 local and field subject/control forms in one TypeScript process.
Measured wall cost: 0.320 s. The other five checker/HIR tests cost less than 0.01 s.
The tests cover read-modify-write operations, zero storage types, function owners, C5, rejected annotations, and scope shadowing.
No new test executes a corpus entry on the three tiers.

## Self-check

- `cargo test --offline --locked -p subscript-compiler`: all tests pass.
- `cargo test --offline --locked -p subscript-codegen`: all tests pass.
- `cargo fmt --check`: passes.
- `cargo clippy --offline --locked --workspace --all-targets`: passes; compiler/runtime/codegen warnings are 5/18/13.
- The `tools/gate.sh` warning limits are 7/18/13. No new warning appears.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference`: passes.
- The §154 total check passes with 1,591 witnesses and 468 variants.
- The TypeScript corpus check passes with 651 measured entries, including the four new reject headers.
- The final chained-assignment expansion passes the existing corpus checks.
- The final golden and interpreter sweeps include the expanded entry.
- No existing golden changes. All changed Rust files contain at most 2,000 lines.

The retired local-initializer site loses five accepted witnesses.
Four TS2454 witnesses now target the read site.
The annotation guard gains a measured accepted-TypeScript witness; the unreachable const guard is removed.
Existing tests retain their purpose with the new declaration and read rules.
The corpus index changes through its generator.
`tools/gate.sh` was not run. No commit was made.

## Round 3 changes

Switch dispatch visits the discriminant and each case test in source order.
Case bodies start with the dispatch fact at their matching test.
Default dispatch follows every failed test, including tests after the default in source order.
The shared walker checks case-test reads for locals and fields.
The table includes unassigned, assigned, and earlier-case-body controls for both bindings.
TypeScript rejects a read after an assignment in an earlier case test: TS2454 for locals, TS2565 for fields.
The walker checks test expressions in order but keeps the entry assignment fact along dispatch.
Tests include a case-test write and a default before a case-test write.

An omitted or literal-true for condition has no zero-iteration exit.
The table includes both forms and a break-before-assignment control for each.
An exhaustive string-alias switch has no unmatched dispatch exit.
The check uses the existing alias members and the HIR member indices.
The table includes complete assignment and a case that breaks before assignment.
No new HIR fact is needed.

The local type restriction cites C24 row 3.
Its reason states that the type comes from the annotation or initializer, not later assignments.
The four read-witness keys name print, initializer, assignment RHS, and return forms.
Constructor consumers call the shared walker directly; the empty field-flow forwarders are removed.

The rejection class table splits by topic into two files: 1,669 and 344 lines.
A macro keeps one exhaustive match over the site enum.
The total source check recognizes both parts of this match.

## Recorded open items (§158.4)

1. HIR folds parentheses; parenthesized true loops differ from TypeScript flow narrowing.
2. The walker does not follow assignments through conditions that TypeScript narrows.
3. Descriptor members follow declaration order instead of source order.
4. Shared field flow classifies 11 of 26 constructor probes as TypeScript-accepted restrictions under the existing §108 sites.

These four items remain open. This note records no Phase Review result.

## Round 4 changes

The local analysis runs before the opaque check restores its snapshot.
It stores whole diagnostics in the existing opaque diagnostic form (§135).
The concrete analysis runs before the opaque merge and records its diagnostic range.
The merge reports each generic read once, including reads in bodies with multiple concrete instances.
No new HIR fact is needed.

The function-owner test adds uninstantiated generic functions, generic-class methods, and generic methods of ordinary classes.
A separate checker test checks each owner with two instances and an initialized control.
TypeScript 5.9.2 reports TS2454 for all three uninstantiated subjects and accepts all three controls.
Before the fix, the function-owner test fails because the generic function is accepted.
Before the fix, the literal-false while subject fails at its unreachable read.
After the fix, all six local assignment tests pass.
The path test checks 116 forms in one TypeScript batch: 0.317 s.
The other five tests complete within that run's 0.32 s total.

The switch walker makes one forward pass through the case bodies.
Each case merges its dispatch entry with the distinct fallthrough states from the preceding case.
The literal-false while condition now skips its body after condition evaluation.
The path table includes an unreachable read and an initialized control for this loop.
TypeScript accepts both.

The performance probe uses 2,000 fallthrough case labels, a default assignment, and a read after the switch.
Both versions pass the CLI check. Three CLI processes measure each version before and after the change.

| Storage | Before, seconds | After, seconds | Median before / after |
|---|---|---|---|
| Unassigned | 2.005, 1.154, 1.142 | 1.955, 0.054, 0.054 | 1.154 / 0.054 |
| Initialized | 0.060, 0.060, 0.059 | 0.060, 0.059, 0.059 | 0.060 / 0.059 |

The unassigned median falls by approximately 21 times.
The four §158.4 items stay open; this round changes none of them.
This note records no Phase Review result.

Round 4 self-check:

- `cargo test --offline --locked -p subscript-compiler`: passes, 103.923 s.
- `cargo test --offline --locked -p subscript-codegen`: passes, 353.405 s.
- `cargo fmt --check`: passes, 1.602 s.
- `cargo clippy --offline --locked --workspace --all-targets`: passes, 9.287 s.
- Library warning counts remain 5/18/13 against the gate limits of 7/18/13.
- No warning names a round 4 implementation or test file.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference`: passes, 0.481 s.
- `tools/hygiene.sh`: passes, 2.343 s.
- The changed Rust files remain within 2,000 lines. `git diff --check` passes.
- The generator changes only the existing corpus-index diff. No existing golden changes.
- `tools/gate.sh` was not run. No commit was made.

## Phase Review

| Pass | CRITICAL | MAJOR | MINOR | Result |
|---|---:|---:|---:|---|
| 1 | 0 | 2 | 8 | Fixed in round 3: `case` tests as reads, the `let x;` record (rule 1, C24 row 3); `for (;;)`, exhaustive string-literal switches, the `rejection.rs` split; the rest recorded in §158.4 |
| 2 | 0 | 1 | 5 | Fixed in round 4: generic bodies with no instance (rule 4), the one-pass `switch` walk, `while (false)`; parenthesized literals added to §158.4 item 1 |

After round 4 the pass 2 MAJOR probe gives one diagnostic per read, with
and without an instance.

Final gate: `gate full 12b54ab7 dirty:41 debug 2322/0/3 release 2319/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.

Status: COMPLETE.
