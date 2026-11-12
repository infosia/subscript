# String-literal alias conversion

Contract: compiler.md section 259.

## Red evidence

The unchanged debug CLI rejected a443 before the code change.
The first diagnostic was S100 at line 11: `type mismatch: the return value expects string, got Mode`.
The diagnostic uses backticks around the type names.

The handoff reports these measurements at the pin:

- init: S100, the initializer expects string, got Mode.
- assign: S100, the assignment expects string, got Mode.
- arg/print/push/ctor: S100, the argument expects string, got Mode.
- ret: S100, the return value expects string, got Mode.
- lam: S100, the lambda body expects string, got Mode.
- arr: S100, the array element expects string, got Mode.
- field: S100, the assignment expects string, got Mode.
- nul: S100, the initializer expects string or null, got Mode.
- wire: S100, the initializer expects string, got Wire.
- template control: `t b`.

## Implementation

The shared value-place helper inserts an explicit HIR Cast from the alias to string.
Structural assignability stays unchanged, so containers and function types keep their rejection.
The common LIR uses one AliasString operation with the alias id and discriminant.
The JIT and C tiers intern the selected alias table entry with subscript_rt_str_lit.
The interpreter reads the member table and uses its string-literal allocation path.
This form gives all three tiers the same conversion and string-literal cost.

The HIR Cast declares the Allocation trap, as a string literal does.
The LIR carries one Allocation trap on the AliasString operation.

The audit of compiler/src/raise_sites.rs requires no change.
Its can_raise fact tracks script exceptions, not allocation traps.
A string literal does not raise a script exception. The conversion follows the same rule.
A unit test checks the Allocation trap and the false expression_can_raise result.

The r501 diagnostic has a dedicated divergence block.
It cites compiler.md section 259 and explains the distinct array layouts.
The entry comment cites Q32 and section 259 rule 5.
The corpus row follows r500.

## Checks

- `cargo build --offline --locked`: passed.
- `cargo test --offline --locked -p subscript-compiler --test string_alias_to_string`: 2 passed, 0 failed; 0.02 seconds.
- `cargo test --offline --locked -p subscript-codegen --test string_alias_to_string_cemit`: 2 passed, 0 failed; 0.65 seconds.
- The golden harness checks a443 on dev JIT, ship C, and interpreter against the expected file, byte for byte.
- The C comparison control has no string comparison call.
- `cargo test --offline --locked -p subscript-compiler --test corpus_reject`: 50 passed, 0 failed; 1.02 seconds.
- `cargo test --offline --locked -p subscript-compiler --lib divergence::tests`: 10 passed, 0 failed; 0.01 seconds.
- TypeScript 5.9.2 accepted a443 and r501 with the project prelude and strict options: zero diagnostics.
- Node output for a443 equaled its expected file, byte for byte.
- `cargo fmt --check`: passed.
- `cargo clippy --offline --locked --workspace --all-targets`: passed. No warning remains in a changed file.

The unit test times exclude compilation. The three-tier test includes C compilation because it checks the ship tier.

## Registry changes

- compiler/tests/corpus_reject.rs: S100, line 10 pin for r501.
- compiler/src/divergence.rs: the new variant and its exhaustive list entry.
- compiler/src/divergence/entries.rs: the fragment dispatch.
- compiler/src/divergence/type_flow.rs: the array-layout divergence fragment.
- compiler/src/check/rejection_sites.rs: the named rejection site.
- compiler/src/check/rejection.rs: the rejection class and divergence mapping.
- compiler/src/check/rejection_witness_sites.rs: the direct witness site.
- compiler/src/check/rejection_witness_index.rs: the independent expected divergence.
- compiler/src/check/rejection_programs.txt: the alias-array witness source.
- compiler/src/check/rejection_targets.txt: the witness diagnostic pin.

No existing corpus output or golden changed. No commit or push ran.

## Registry retirement and matrix admission

LiteralAliasToString no longer names a rejection site or divergence.
The rejection registries remove its target, program, witness, and class.
The remaining references are in specs/tracking/s154-rejection-sites.md at lines 2032, 2147, and 2240.
No corpus entry cites this name.

The product omission pin stays at 7815.
The omitted set includes product-string-alias-parameter-case-string-instance.
Its case label gives S100: “type mismatch: the case label expects `string`, got `Label`”.
A case label compares values. It takes no alias-to-string conversion.
No removed rejection target or witness pinned this case-label message.
The removed LiteralAliasToString witness checks a return value.
The API omission set loses alias arguments to print, JSON.parse, graphemeLength, and sliceGraphemes.
The API pin changes from 9541 to 9537.
These four admitted instances pass an alias as argument zero:

- `api-print-0-value-string-alias-instance`: `print(message: string)`.
- `api-JSON-parse-0-value-string-alias-instance`: `JSON.parse(text: string)`.
- `api-subscript:text-graphemeLength-0-value-string-alias-instance`: `graphemeLength(s: string)`.
- `api-subscript:text-sliceGraphemes-0-value-string-alias-instance`: `sliceGraphemes(s: string, start: i32, end?: i32)`.

Each is a call argument place under section 259 rule 1.
The apparent-type allowlist records the nullable inner match in place_value.
The target comes from apparent_type, which resolves the nested nullable type.

The reference generator adds a443 and r501 at generated-docs/corpus-index.md lines 430 and 941.
The other generated reference files stay byte-identical.

## Validation

All commands use cargo offline. Times include compilation and process startup.

| Command arguments after cargo test --offline | Result | Seconds |
|---|---|---:|
| -p subscript-codegen --test lir every_hir_execution_fact_is_carried_by_lir | 1 passed, debug | 8.516 |
| -p subscript-compiler --lib generated_ai_references_are_byte_identical | 1 passed | 2.057 |
| -p subscript-compiler --lib check::rejection_total | 19 passed | 2.073 |
| -p subscript-compiler --test apparent_type_shapes | 8 passed | 1.139 |
| -p subscript-compiler --test generic_tsc_matrix generic_forms_follow_tsc -- --nocapture | 1 passed; 40251 cells, zero failures | 36.768 |
| -p subscript-compiler --test string_alias_to_string | 2 passed | 1.452 |
| -p subscript-codegen --test string_alias_to_string_cemit | 2 passed | 1.973 |
| -p subscript-compiler --lib divergence | 14 passed | 2.097 |

The generic matrix omits the product switch instance and checks the four admitted API cells with tsc.
The golden harness owns the three-tier check. The separate duplicate test is removed.
The fact comparison stays unchanged. Its arity table declares the new instruction's single operand.

## Case-label and verifier validation

All commands below passed. Times include compilation and process startup.
Each test command uses `cargo test --offline --locked`.

| Command arguments | Result | Seconds |
|---|---|---:|
| -p subscript-compiler --test generic_tsc_matrix -- --nocapture | 14 passed; main matrix: 40250 cells, zero failures | 36.161 |
| -p subscript-compiler --test string_alias_to_string | 2 passed | 1.405 |
| -p subscript-codegen --test string_alias_to_string_cemit | 2 passed | 4.703 |
| -p subscript-codegen --test string_alias_to_string_cemit verifier_rejects_alias_string_with_a_string_operand | 1 passed | 0.152 |
| cargo fmt --check | passed | 2.011 |

The product omission list has 7815 entries and includes the string-switch alias case.
The API omission list has 9537 entries and excludes the four argument instances listed above.
The HIR test counts each conversion and checks its string result and Mode operand types.
Its execution time is 0.03 seconds, excluding compilation and process startup.
The verifier test supplies a string parameter to AliasString and checks the signature diagnostic.
The same lowered module passes verification before the operand replacement.
The C comparison test retains its string-comparison control.
No registry file changes in this correction.

## Gate and Phase Review

- Full gate, first run: `debug 3333/5/5 release 3330/5/5`, exit 1. The five
  failures followed from the conversion: the LIR facts of the per-member
  string literals, the generated corpus index, the retired
  `LiteralAliasToString` target, the apparent-type allowlist, and the
  generic matrix pin. Round 5 fixed each one.
- Full gate after round 5: `debug 3339/0/5 release 3336/0/5`, exit 0.
- Phase Review: compiler.md §259.3. Round 6 fixed the MAJOR finding and
  four MINOR findings.
- Full gate after round 6: `debug 3337/2/5`, exit 1. The `case` label
  check was not in the list of type-only checks
  (`compiler/tests/type_only_assignability.rs`). Round 7 added it.
- Full gate after round 7: `debug 3339/0/5 release 3336/0/5 skips 2/0
  goldens-moved 0`, exit 0.
