# S154 round 1: rejection sites

Current pin: `ddc2e5f8c2deb534a08498b5d32fae71b6dc1e17`.

Status: stopped at the reject-corpus condition in the handoff.
The implementation is incomplete. The working tree contains the partial migration.
The corpus files and the `Divergence` variants did not change. No commit occurred.

## Stop evidence

The full test command reported only the rule 4 failure in the library tests.
Those tests reported 386 passes and one failure.
The command with the rule 4 test excluded then reached the reject-corpus test.
`divergence_blocks_match_every_reject_entry_tsc_header` failed for `r158-module-initializer-direct-read.ts`.
Its report states: `tsc rejects but the divergence block is present`.

The migration assigned `ModuleInitializerOrder` to `CheckInitEffects902` without the original route condition.
The original branch assigns the variant only when `!path.is_empty()`.
The direct-read program now receives a block. The original program receives none.
This changes the pinned rejection behavior. The handoff requires a stop if a reject entry moves.
No further production change or test followed this finding. Read-only analysis and this note followed it.
The initializer route still needs two named decision sites. The current count therefore remains provisional.

## Inventory method

The audit read the complete handoff, both addenda, `CLAUDE.md`, both contracts, and all three measurement tables.
It inspected rejection constructors, code-valued builders, stored diagnostics, error forwarders, and later divergence assignments.
The migration replaced 502 ordinary fixed-code constructors first.
It then named the remaining fixed-code calls and the branch sites in the five authorized text-error forms.
`RejectionFailure` carries the producer site and its message.
Export resolution forwards the whole site-built diagnostic. It preserves the resolution marker.
API rows obtain their code from the site map.

One closed enum currently declares 668 direct sites and the API-row case.
The API witness table contains 79 row identities. The current map therefore names 747 sites.
One exhaustive match supplies every site's code and class.
The enum file is separate from the match file. Every Rust file remains below 2,000 lines after formatting.
The count describes the current tree. It does not establish completion of rule 1.

The test records each site, message, and position at the site function.
The witness check compares that trace with the diagnostic and an independent target-message table.
The committed source tables copy the measurement programs and their companion files.
New probes separate alignment, provenance, pattern, nullable-call, and frame-copy branches.
The table states source guards for unreachable branches.
The table keeps witnesses for each observed TypeScript code set.

## Current site count

| Code | Sites |
| --- | ---: |
| S001 | 1 |
| S002 | 3 |
| S003 | 4 |
| S004 | 2 |
| S005 | 3 |
| S006 | 1 |
| S007 | 3 |
| S008 | 5 |
| S009 | 3 |
| S010 | 5 |
| S011 | 8 |
| S012 | 13 |
| S013 | 6 |
| S014 | 167 |
| S016 | 9 |
| S017 | 8 |
| S018 | 20 |
| S100 | 486 |
| Total | 747 |

## Red result and cost

The last isolated rule 4 run reports 366 distinct failure targets.
331 named `TscRejects` sites have a TypeScript-accepted witness. Each matching arm has one `// §154 Red` comment.
The other targets comprise one witness-class failure and 34 fragment-check targets for 23 distinct variants.
The fragment checks report 39 individual failures. Some variants fail more than one check.
No variant changed to close those failures.

The test runs 986 source witnesses and 106 carried variants.
A warm run measures TypeScript at 0.483 seconds, the checker at 0.243 seconds, and the total at 0.842 seconds.
One Node process uses stock TypeScript's `createProgram` and `getPreEmitDiagnostics` for one project.
This includes syntax and semantic errors when one witness has a syntax error.
Every retained witness label matches that run. No target message or named site remains absent.

The firing controls construct an accepted source with the `TscRejects` class, a diagnostic with a forbidden block, and an outside code literal.
They do not edit the record that the check reads.
The library run passes all firing controls and the source scan.

## Difference from 296

The typed traces map all 296 measured `needs variant` observations to 291 current decision sites.
Five observations duplicate a decision that another measured row already names.
40 additional current decision sites have accepted-class witnesses. Thus, `296 - 5 + 40 = 331`.

The map uses each measured witness, its target message, and its code. It does not rely only on old line numbers.
The runner measurement uses `runner:S100`; its mapped diagnostic code is `S100`.
No measured accepted-class observation lacks a current Red target.

The following groups account for the five duplicate observations.

| Current site | Measured observations | Count reduction | Reason |
| --- | --- | ---: | --- |
| `NullableMember` | a `namespace.rs:22`; c `check/narrowing.rs:69` | 1 | The caller and its code-valued forwarder describe the same rejection. |
| `CheckBodies225` | b `declarations:361`; b `bodies:225` | 1 | Both retained witnesses reach the body guard. The first witness does not reach the current generic declaration guard. |
| `CheckTyres97` | c `check/tyres.rs:97`; c `check/bindings.rs:226`; c `diag.rs:198` | 2 | The two common builders have no rejection decision. Their witness reaches the type-position guard. |
| `CheckNamespaceImport21` | c `check/bindings.rs:236`; c `check/namespace_import.rs:21` | 1 | The resolution builder forwards the namespace rejection. |

The following table lists all 40 additional Red sites.
An alignment row replaces one combined measured consumer with a separate producer branch.
A provenance row replaces one combined measured consumer with a separate record or cursor branch.
The five other rows contain newly retained accepted-class observations.

| Additional site | Accepted witness | Reason |
| --- | --- | --- |
| `CheckDeclarationsFailure121` | `new-align-count` | A separate alignment producer decides this rejection. |
| `CheckDeclarationsFailure124` | `new-align-object` | A separate alignment producer decides this rejection. |
| `CheckDeclarationsFailure127` | `new-align-empty` | A separate alignment producer decides this rejection. |
| `CheckDeclarationsFailure130` | `new-align-spread` | A separate alignment producer decides this rejection. |
| `CheckDeclarationsFailure133` | `new-align-method` | A separate alignment producer decides this rejection. |
| `CheckDeclarationsFailure141` | `new-align-key` | A separate alignment producer decides this rejection. |
| `CheckDeclarationsFailure144` | `new-align-string` | A separate alignment producer decides this rejection. |
| `CheckExprAggregate242` | `a-s004` | Another measured source reaches the missing-member branch without a TypeScript error. |
| `CheckExprCall174` | `b-probe-conversions` | TypeScript accepts the measured String call. This checker reports an unknown function. |
| `CheckExprLiteral554` | `c-lookup-switch-var-read` | TypeScript accepts the measured switch var binding. This checker reports an unknown name. |
| `CheckExprOperator1480` | `b-signatures-409` | The measured generator program reaches the checker yield-context guard without a TypeScript error. |
| `CheckLayout369` | `new-frame-argument-copy` | The new frame-copy witness reaches the frame branch outside local aggregate storage. |
| `ProvenanceDuplicate107` | `new-prov-descriptor-duplicate` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceDuplicate137` | `new-prov-string-view-duplicate` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceDuplicate164` | `new-prov-scalar-pair-duplicate` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceDuplicate188` | `new-prov-callback-duplicate` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceDuplicate209` | `new-prov-callback-lifetime-duplicate` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceDuplicate235` | `new-prov-external-duplicate` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceDuplicate251` | `new-prov-cenum-duplicate` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure339` | `new-prov-kind` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure366` | `new-prov-empty` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure383` | `new-prov-no-separator` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure396` | `new-prov-key` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure403` | `new-prov-unquoted` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure409` | `new-prov-unterminated` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure419` | `new-prov-escape-end` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure430` | `new-prov-escape-unknown` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure438` | `new-prov-control` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure459` | `new-prov-unicode-bad` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure477` | `new-prov-boolean` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceFailure493` | `new-prov-trailing` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceInvalidScalar` | `new-prov-unicode-scalar` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceMalformed128` | `new-prov-string-view-empty` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceMalformed155` | `new-prov-scalar-pair-empty` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceMalformed180` | `new-prov-callback-empty` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceMalformed201` | `new-prov-callback-lifetime-empty` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceMalformed227` | `new-prov-external-empty` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceMalformed243` | `new-prov-cenum-empty` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceMalformed98` | `new-prov-descriptor-empty` | A separate provenance record or cursor branch decides this rejection. |
| `ProvenanceShortUnicode` | `new-prov-unicode-short` | A separate provenance record or cursor branch decides this rejection. |

## Other rule 4 failures

`CheckDeclarations627` retains `WireEnumValues`, as the handoff requires.
Its guard requires a CEnum property without a type annotation.
Its retained witnesses receive `TS7008`. It has no measured TypeScript-accepted witness.
The total check reports this rule 3 failure separately from the 331 Red sites.

The following variants fail the expanded fragment check. The variant files remain unchanged.

| Variant | Failed checks |
| --- | --- |
| `AggregateLayoutLimit` | The checker lacks the fragment variant |
| `ArrayRestPattern` | The checker lacks the fragment variant |
| `BoundaryOnlyObject` | The checker lacks the fragment variant |
| `ByteAccessTarget` | TypeScript rejects the TS fragment; The checker rejects the subscript fragment |
| `ClassIndexSignature` | The checker rejects the subscript fragment |
| `DroppedAsyncHandle` | TypeScript rejects the TS fragment; The checker lacks the fragment variant; The checker rejects the subscript fragment |
| `EmbeddedHeaderCopy` | TypeScript rejects the TS fragment; The checker lacks the fragment variant; The checker rejects the subscript fragment |
| `GenericInferenceMissing` | The checker rejects the subscript fragment |
| `ModuleInitializerOrder` | TypeScript rejects the TS fragment; The checker rejects the subscript fragment |
| `NamedModuleSurface` | TypeScript rejects the TS fragment; The checker rejects the subscript fragment |
| `NestedPattern` | The checker lacks the fragment variant |
| `ObjectRestPattern` | The checker lacks the fragment variant |
| `OptionalChainIndex` | The checker lacks the fragment variant |
| `OptionalDescriptorMember` | The checker rejects the subscript fragment |
| `PatternDefaultValue` | The checker lacks the fragment variant |
| `PatternFieldName` | The checker lacks the fragment variant |
| `PatternSourceShape` | The checker lacks the fragment variant |
| `PromiseObject` | TypeScript rejects the TS fragment; The checker rejects the subscript fragment |
| `SharedLocationNarrowing` | TypeScript rejects the TS fragment; The checker lacks the fragment variant; The checker rejects the subscript fragment |
| `SwitchOverAlias` | TypeScript rejects the TS fragment; The checker lacks the fragment variant; The checker rejects the subscript fragment |
| `ThisBeforeFieldValues` | TypeScript rejects the TS fragment; The checker lacks the fragment variant; The checker rejects the subscript fragment |
| `UsingDeclaration` | TypeScript rejects the TS fragment; The checker rejects the subscript fragment |
| `WorkerContextAffinity` | TypeScript rejects the TS fragment; The checker rejects the subscript fragment |

## Verification status

- `cargo build --offline --locked -p subscript-compiler`: passes before the final test-table edits.
- `cargo test --offline --locked -p subscript-compiler`: 386 library tests pass; the rule 4 test fails.
- The same command with `-- --skip every_subset_rejection_carries_its_divergence` reaches the corpus stop above.
- `cargo fmt`: ran under the pinned toolchain before the stop.
- `cargo fmt --check`: did not run before the stop.
- Clippy: did not run before the stop. No claim against its seven-warning compiler baseline exists.
- `tools/gate.sh`: did not run.

The corpus stop prevents completion of the remaining test binaries and required final checks.

## Changed files

- `compiler/src/ambient.rs`
- `compiler/src/api_reference.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/container_argument.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/exports.rs`
- `compiler/src/check/expr/aggregate.rs`
- `compiler/src/check/expr/array_of_and_map_copy.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/expr/namespace.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/host_entries.rs`
- `compiler/src/check/inference.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/instance_chain.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/layout.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mirror_provenance.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/namespace_import.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/opaque.rs`
- `compiler/src/check/pattern.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/hir/host_entry.rs`
- `compiler/src/lib.rs`
- `compiler/src/parse.rs`
- `compiler/src/provenance.rs`
- `compiler/src/regex.rs`
- `compiler/src/check/rejection_programs.rs`
- `compiler/src/check/rejection_programs_s154.txt`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets_s154.txt`
- `compiler/src/check/rejection_tsc.cjs`
- `compiler/src/check/rejection_witness_sites.rs`
- `specs/tracking/s154-rejection-sites.md`

## Prior stop-report status

The following section preserves the prior report. Addendum 2 supersedes its form-information stop condition.

## Prior form-information stop report

Contract pin: `e9ed2528e39fb9bdb3b73302e677886b69f70d48`.

Status: stopped at the Addendum's form-information condition.
No production code, witness table, message, variant, or corpus entry changed.

### Evidence

The audit read `CLAUDE.md`, both contracts, and the handoff with its Addendum.
The audit searched production Rust for diagnostic constructors, rejection builders, stored diagnostics, error results, and divergence fields.
It then read each text-error producer and its diagnostic consumer.
Warnings, test controls, and successful string results do not enter this list.

The Addendum permits replacement of stored diagnostics with stored sites.
The following other forms carry error text without a typed rejection origin.
Their consumers select the code or class after the text crosses the function boundary.
The missing fact is the rejection-origin discriminant that the exhaustive site map needs.

| Form | Producers and forwarders | Diagnostic consumer |
| --- | --- | --- |
| Regular-expression failure: `Result<_, String>` | `regex.rs`: `validate_literal`, `validate_flags` | `check/expr/literal.rs:70`: `check_literal` |
| Alignment failure: `Result<u32, &'static str>` | `check/declarations.rs:140`: `value_type_alignment` | `check/declarations.rs:195`: `class_decorators` |
| Initializer failure: `Result<_, String>` | `check/init_effects.rs`: `ModuleItemScan::enqueue`, `ModuleRouteScan::resolve`, `validate_module_segment` | `check/init_effects.rs:779,871`: `module_initializer_diagnostics` |
| JSON helper failure: `Result<_, String>` | `check/json.rs`: functions listed below | `check/json.rs:122,252`: `check_json_call`, `check_json_parse` |
| Provenance syntax failure: `Result<_, String>` | `provenance.rs`: `parse_line`, `Cursor::token`, `separator`, `key`, `string`, `unicode_escape`, `boolean`, `finish` | `provenance.rs:60`: `parse` calls `malformed` |

The JSON functions are:

- `synthesize_json_serializer`
- `json_helper_body`
- `json_array_body`
- `json_object_body`
- `synthesize_json_parser`
- `json_validation_body`
- `json_array_validation_body`
- `json_construction_body`
- `json_array_construction_body`
- `json_type_index`

The alignment form combines argument shape, option shape, and alignment-value errors.
The initializer form combines three segment errors at one diagnostic consumer.
The JSON form forwards type-graph, unsupported-type, array-shape, and missing-error-class failures.
The provenance form forwards record-kind, field, string, escape, boolean, and trailing-data failures.
The regex form combines flag failures and the dependency's pattern error text.
None of these forms stores the origin site.

This stop interprets a rejection origin as the branch that returns the failure, rather than only its later diagnostic consumer.
The current measurement counts physical diagnostic constructions instead.
That distinction changes the site inventory and witness obligations.
The next contract decision must state the site boundary for these text-error forms.

### Authorized stored forms

`ExportResolution.failures` stores built diagnostics, as the Addendum states.
`DiagnosticSink`, the layout validator, and the opaque checker also store built diagnostics.
The opaque merge uses diagnostic codes, messages, positions, and ordered ranges.
`PatternRejection` stores its span, message, divergence, and bound names.
The frame and checker store optional divergence values for missing `this` and aggregate layout failures.
These forms also need explicit sites during the migration.
This report does not treat `ExportResolution.failures` as a new stop condition.

### Measurement status

The existing measurement counts 530 non-S014 construction sites, including four common builders.
Its groups contain 237, 177, and 116 sites.
Its `needs variant` count is 296.
This audit does not claim a new site count per code.
The site boundary above must resolve before a new count is complete.

The rule 4 test did not run.
No Red count, difference list against 296, or warm checker and TypeScript cost exists for this round.
The build, test suite, formatter, and Clippy did not run after the explicit stop.
`tools/gate.sh` did not run.

### Changed files

- `specs/tracking/s154-rejection-sites.md`

No commit occurred.

## Round 2

Status: the authorized group c pass is complete, with the acceptance 3 list below.
The migration remains incomplete. No commit occurred.
The round preserves the round 1 base. It closes 64 Red arms and corrects one existing container variant.
The current table has 671 direct sites and 79 API row identities: 750 sites.
The Red count falls from 331 to 267. All remaining rule 4 failures are Red class failures.
The undecided list has 33 sites, including the two nullable-use forwarding sites.
The other 234 Red sites belong to groups a and b.

### Conditional class audit

`CheckInitEffects902` becomes two guards: `InitializerRouteRead` and `InitializerDirectRead`.
The route guard uses `!path.is_empty()` and retains `ModuleInitializerOrder`.
The direct guard uses `path.is_empty()` and remains `TscRejects`.
`r158-module-initializer-direct-read.ts` keeps its program and header. It receives no divergence block.
`new-initializer-direct` measures `TS2448, TS2454` and reaches the direct guard.

The audit reads every removed divergence assignment and conditional variant choice against HEAD.
No other conditional variant choice lost its guard. The following splits already exist in the round 1 base.

| Condition | Separate sites |
| --- | --- |
| Local aggregate storage versus another frame allocation | `LocalAggregateFrameLimit`, `CheckLayout369` (now `AggregateArgumentFrameLimit`) |
| Recognized iterator fields versus another binding pattern | `CheckBindings95`, `CheckBindings97` |
| Ended shared narrowing versus another nullable call | `NullableCallShared`, `NullableCall` |
| Ended shared narrowing versus another nullable member use | `NullableMemberShared`, `NullableMember` |
| Different literal aliases versus another assignment mismatch | `AssignmentLiteralAlias`, `CheckTypeRules329` |
| Numeric conditional join versus another failed join | `ConditionalJoinSizedOperandWidths`, `ConditionalJoinGeneralUnionAndUndefined` |
| Static method or field without this versus another missing this context | `ThisStaticField`, `CheckExprEntry243` |
| Lone surrogate parse error versus another parse error | `Parse237`, `Parse245` |

This round adds two guard splits to keep each new variant specific.

| Original site | Diverges guard | TscRejects guard |
| --- | --- | --- |
| `CheckGenerics65` | `GenericConstraintIdentity`: two class types fail nominal compatibility | `GenericConstraintMismatch`: another constraint mismatch |
| `CheckNamespaceImport21` | `NamespaceUnexportedMember`: the missing name is default | `NamespaceMemberMissing`: another missing export |

### Closed sites and renames

This table names every closed site, its old name, its variant, and the section that decides its restriction.
Each new variant has a separate TypeScript fragment. The total test measures the fragment and its same-variant diagnostic.

| Old site | Guard name | Variant | Use | Collision |
| --- | --- | --- | --- | --- |
| `CheckLayout284` | `ClassFinalAlignmentLimit` | `ClassFinalAlignmentLimit` | new | `collisions.md Q29` |
| `CheckLayout369` | `AggregateArgumentFrameLimit` | `AggregateArgumentFrameLimit` | new | `collisions.md Q29` |
| `CheckTyres369` | `FixedArrayByteLimit` | `AggregateLayoutLimit` | reused | `collisions.md Q29` |
| `CheckLookup37` | `NamespaceAsValue` | `NamespaceAsValue` | new | `C18` |
| `CheckLookup125` | `SwitchCaseRead` | `SwitchCaseRead` | new | `compiler.md §67.1` |
| `CheckLookup154` | `BlockNameReadBeforeDeclaration` | `BlockNameReadBeforeDeclaration` | new | `C14` |
| `CheckLookup175` | `BlockNameWriteBeforeDeclaration` | `BlockNameWriteBeforeDeclaration` | new | `C14` |
| `CheckLookup194` | `ContextAffineCapture` | `ContextAffineCapture` | new | `compiler.md §40` |
| `CheckLookup203` | `MutableLocalCapture` | `MutableLocalCapture` | new | `C5` |
| `CheckContainerArgument49` | `ContextAffineArrayElement` | `ContextAffineArrayElement` | new | `compiler.md §40` |
| `CheckContainerArgument50` | `ContextAffineContainerArgument` | `ContextAffineContainerArgument` | new | `compiler.md §40` |
| `CheckTyres246` | `WorkerMessagePlainClass` | `WorkerMessagePlainClass` | new | `stdlib.md §16.2` |
| `CheckTyres541` | `BoundaryLiteralAlias` | `BoundaryLiteralAlias` | new | `compiler.md §24.1` |
| `CheckTyres633` | `NullableNonReference` | `NullableNonReference` | new | `C7` |
| `CheckTypeRules312` | `NullableValueClassAssignment` | `NullableValueClassAssignment` | new | `C7` |
| `CheckMirrorProvenance33` | `MirrorHeaderMissing` | `MirrorHeaderMissing` | new | `compiler.md §23.3` |
| `CheckMirrorProvenance61` | `MirrorParameterTargetMissing` | `MirrorParameterTargetMissing` | new | `compiler.md §23.3` |
| `CheckMirrorProvenance75` | `MirrorCallbackTargetMissing` | `MirrorCallbackTargetMissing` | new | `compiler.md §23.3` |
| `CheckMirrorProvenance120` | `MirrorLifetimeTargetMissing` | `MirrorLifetimeTargetMissing` | new | `compiler.md §111` |
| `CheckMirrorProvenance166` | `MirrorArrayProvenanceMissing` | `MirrorArrayProvenanceMissing` | new | `compiler.md §23.3` |
| `CheckMirrorProvenance180` | `MirrorStringProvenanceMissing` | `MirrorStringProvenanceMissing` | new | `compiler.md §23.3` |
| `CheckMirrorProvenance193` | `MirrorParameterProvenanceMismatch` | `MirrorParameterProvenanceMismatch` | new | `compiler.md §23.3` |
| `CheckMirrorProvenance217` | `MirrorAnonymousCallback` | `MirrorAnonymousCallback` | new | `compiler.md §23.3` |
| `CheckMirrorProvenance233` | `MirrorCallbackProvenanceMissing` | `MirrorCallbackProvenanceMissing` | new | `compiler.md §23.3` |
| `ProvenanceFailure339` | `ProvenanceUnknownKind` | `ProvenanceUnknownKind` | new | `compiler.md §23.3` |
| `ProvenanceFailure366` | `ProvenanceMissingKind` | `ProvenanceMissingKind` | new | `compiler.md §23.3` |
| `ProvenanceFailure383` | `ProvenanceFieldSeparator` | `ProvenanceFieldSeparator` | new | `compiler.md §23.3` |
| `ProvenanceFailure396` | `ProvenanceUnexpectedKey` | `ProvenanceUnexpectedKey` | new | `compiler.md §23.3` |
| `ProvenanceFailure403` | `ProvenanceUnquotedString` | `ProvenanceUnquotedString` | new | `compiler.md §23.3` |
| `ProvenanceFailure409` | `ProvenanceUnterminatedString` | `ProvenanceUnterminatedString` | new | `compiler.md §23.3` |
| `ProvenanceFailure419` | `ProvenanceUnterminatedEscape` | `ProvenanceUnterminatedEscape` | new | `compiler.md §23.3` |
| `ProvenanceFailure430` | `ProvenanceUnsupportedEscape` | `ProvenanceUnsupportedEscape` | new | `compiler.md §23.3` |
| `ProvenanceFailure438` | `ProvenanceControlCharacter` | `ProvenanceControlCharacter` | new | `compiler.md §23.3` |
| `ProvenanceFailure459` | `ProvenanceInvalidUnicodeDigits` | `ProvenanceInvalidUnicodeDigits` | new | `compiler.md §23.3` |
| `ProvenanceFailure477` | `ProvenanceInvalidBoolean` | `ProvenanceInvalidBoolean` | new | `compiler.md §23.3` |
| `ProvenanceFailure493` | `ProvenanceTrailingData` | `ProvenanceTrailingData` | new | `compiler.md §23.3` |
| `ProvenanceShortUnicode` | `ProvenanceShortUnicodeEscape` | `ProvenanceShortUnicodeEscape` | new | `compiler.md §23.3` |
| `ProvenanceInvalidScalar` | `ProvenanceInvalidUnicodeScalar` | `ProvenanceInvalidUnicodeScalar` | new | `compiler.md §23.3` |
| `ProvenanceMalformed67` | `ProvenanceHeaderBasename` | `ProvenanceHeaderBasename` | new | `compiler.md §23.3` |
| `ProvenanceDuplicate75` | `ProvenanceDuplicateHeader` | `ProvenanceDuplicateHeader` | new | `compiler.md §23.3` |
| `ProvenanceMalformed98` | `ProvenanceEmptyDescriptor` | `ProvenanceEmptyDescriptor` | new | `compiler.md §23.3` |
| `ProvenanceDuplicate107` | `ProvenanceDuplicateDescriptor` | `ProvenanceDuplicateDescriptor` | new | `compiler.md §23.3` |
| `ProvenanceMalformed128` | `ProvenanceEmptyStringView` | `ProvenanceEmptyStringView` | new | `compiler.md §23.3` |
| `ProvenanceDuplicate137` | `ProvenanceDuplicateStringView` | `ProvenanceDuplicateStringView` | new | `compiler.md §23.3` |
| `ProvenanceMalformed155` | `ProvenanceEmptyScalarPair` | `ProvenanceEmptyScalarPair` | new | `compiler.md §23.3` |
| `ProvenanceDuplicate164` | `ProvenanceDuplicateScalarPair` | `ProvenanceDuplicateScalarPair` | new | `compiler.md §23.3` |
| `ProvenanceMalformed180` | `ProvenanceEmptyCallback` | `ProvenanceEmptyCallback` | new | `compiler.md §23.3` |
| `ProvenanceDuplicate188` | `ProvenanceDuplicateCallback` | `ProvenanceDuplicateCallback` | new | `compiler.md §23.3` |
| `ProvenanceMalformed201` | `ProvenanceEmptyCallbackLifetime` | `ProvenanceEmptyCallbackLifetime` | new | `compiler.md §23.3` |
| `ProvenanceDuplicate209` | `ProvenanceDuplicateCallbackLifetime` | `ProvenanceDuplicateCallbackLifetime` | new | `compiler.md §23.3` |
| `ProvenanceMalformed227` | `ProvenanceEmptyExternalType` | `ProvenanceEmptyExternalType` | new | `compiler.md §23.3` |
| `ProvenanceDuplicate235` | `ProvenanceDuplicateExternalType` | `ProvenanceDuplicateExternalType` | new | `compiler.md §23.3` |
| `ProvenanceMalformed243` | `ProvenanceEmptyCEnum` | `ProvenanceEmptyCEnum` | new | `compiler.md §23.3` |
| `ProvenanceDuplicate251` | `ProvenanceDuplicateCEnum` | `ProvenanceDuplicateCEnum` | new | `compiler.md §23.3` |
| `CheckTyres22` | `AsyncReturnNonReference` | `AsyncReturnNonReference` | new | `compiler.md §26.1` |
| `CheckTyres31` | `AsyncReturnQualifiedName` | `AsyncReturnQualifiedName` | new | `compiler.md §26.1` |
| `CheckTyres40` | `AsyncReturnAlias` | `AsyncReturnAlias` | new | `compiler.md §26.1` |
| `CheckTyres49` | `AsyncReturnMissingArgument` | `AsyncReturnMissingArgument` | new | `compiler.md §26.1` |
| `CheckTyres58` | `AsyncReturnArgumentCount` | `AsyncReturnArgumentCount` | new | `compiler.md §26.1` |
| `CheckTyres328` | `FixedArrayLengthRange` | `FixedArrayLengthRange` | new | `collisions.md Q29` |
| `CheckTyres343` | `FixedArrayLengthLiteral` | `FixedArrayLengthLiteral` | new | `collisions.md Q3` |
| `CheckGenerics65` | `GenericConstraintIdentity` | `GenericConstraintIdentity` | new | `C1` |
| `CheckNamespaceImport21` | `NamespaceUnexportedMember` | `NamespaceUnexportedMember` | new | `C18` |
| `HirHostEntry36` | `RunnerMainMissing` | `RunnerMainMissing` | new | `collisions.md Q12` |
| `CheckHostEntries26` | `InvalidProgramEntry` | `InvalidProgramEntry` | new | `compiler.md §129.1` |

`InitializerRouteRead` and `InitializerDirectRead` replace the initializer line-number name.
`WireEnumUntypedMember` replaces `CheckDeclarations627`.
The extra guard names `GenericConstraintMismatch` and `NamespaceMemberMissing` carry no variant.

### Undecided sites: acceptance 3

No new collision or variant covers these sites. Their Red arms and programs remain.
The table gives one accepted witness per site. The source table retains every other witness.

| Site | Accepted witness | Target message | Unresolved restriction |
| --- | --- | --- | --- |
| `NullableCall` | `a-s236` | `type `((i32) => i32) \| null` is not callable` | No section decides the loss of an initializer proof for an annotated nullable callable. |
| `NullableMember` | `c-nullable-initialized` | ``C \| null` may be null here; narrow with a null check first` | No section decides the loss of an initializer proof for an annotated nullable class. |
| `CheckBindings97` | `c-pattern-iterator-extended` | `an iterator result cannot supply a binding pattern; use `const r = it.next(); if (r.done) ...`` | No section decides the builtin Generator name over a source alias that extends the result fields. |
| `CheckLookup19` | `c-type-only-local-class` | ``C` cannot be used as a value because it was imported with `import type`` | Section 134 decides type-only imports. It does not decide a type-only flag on a local shadow declaration. |
| `CheckGenerics283` | `c-generic-function-default-explicit` | ``f` expects 2 type argument(s), got 1` | No section decides omitted default type arguments for a generic function. |
| `CheckGenerics375` | `c-generic-method-default-explicit` | ``f` expects 2 type argument(s), got 1` | No section decides omitted default type arguments for a generic method. |
| `CheckGenerics492` | `c-generic-class-default-explicit` | ``C` expects 2 type argument(s), got 1` | No section decides omitted default type arguments for a generic class. |
| `CheckText49` | `c-uri-local-generic` | ``encodeURI` is not generic` | No section decides the builtin URI function name over a source generic function of the same name. |
| `CheckTyres97` | `c-annotation-literal` | `type annotation form outside the decided surface` | No section decides every annotation form that this catch-all rejects, including numeric literal annotations. |
| `CheckTyres180` | `c-annotation-never` | `keyword type outside the decided surface` | No section decides the rejected keyword type never. |
| `CheckTyres193` | `c-qualified` | `qualified type names are not decided` | No section decides every qualified type name, including globalThis.String. |
| `CheckTyres214` | `c-worker-local-default` | `generic reference class `Worker` requires explicit type arguments` | No section decides omitted defaults on a source type alias named Worker. |
| `CheckTyres224` | `c-worker-local-one` | ``Worker` takes exactly 2 type argument(s)` | No section decides the builtin Worker name over a source alias with a different parameter count. |
| `CheckTyres267` | `c-regexp-local` | ``RegExp` is not generic` | No section decides the builtin RegExp name over a source generic class. |
| `CheckTyres283` | `c-promise-default-alias` | ``Promise` requires exactly one fulfilled-value type argument` | No section decides omitted defaults on a source type alias named Promise in a general annotation. |
| `CheckTyres291` | `c-promise-arity-alias` | ``Promise` requires exactly one fulfilled-value type argument` | No section decides the builtin Promise name over a source alias with a different parameter count. |
| `CheckTyres303` | `c-fixed-default-alias` | ``FixedArray` requires element type and length arguments` | No section decides omitted defaults on a source type alias named FixedArray. |
| `CheckTyres311` | `c-fixed-one-alias` | ``FixedArray` takes exactly two type arguments` | No section decides the builtin FixedArray name over a source alias with a different parameter count. |
| `CheckTyres390` | `c-array-two-alias` | ``Array` takes one type argument` | No section decides the builtin Array name over a source alias with a different parameter count. |
| `CheckTyres403` | `c-generator-no-argument` | ``Generator` requires at least a yield type argument` | No section decides omission of the default Generator yield argument in a general annotation. |
| `CheckTyres419` | `c-error-local` | ``Error` is not generic` | No section decides the builtin Error name over a source generic class. |
| `CheckTyres430` | `c-map-local-default` | `generic reference class `Map` requires explicit type arguments` | No section decides omitted defaults on a source type alias named Map. |
| `CheckTyres439` | `c-map-local-one` | ``Map` takes exactly 2 type argument(s)` | No section decides the builtin Map name over a source alias with a different parameter count. |
| `CheckTyres495` | `c-date-local` | ``Date` is not generic` | No section decides the builtin Date name over a source generic class. |
| `CheckTyres512` | `c-class-local` | ``C` is not generic` | No section decides the builtin sized alias over a source generic class of the same name. |
| `CheckTyres518` | `c-class-default` | `generic class `C` requires explicit type arguments` | No section decides omitted default type arguments in a class annotation. |
| `CheckTyres551` | `c-alias-local` | `string-literal union alias `Choice` is not generic` | No section decides a literal alias name over a local generic class of the same name. |
| `CheckTyres562` | `c-lib-interface` | `unknown type name `ReadonlyArray`` | No section decides every unknown annotation name, including the standard ReadonlyArray interface. |
| `CheckTyres581` | `c-intersection` | `intersection types are not in the decided surface` | No section decides the blanket rejection of intersection annotations. |
| `CheckTyres673` | `c-constructor-type` | `constructor types are not in the decided surface` | No section decides the blanket rejection of constructor-type annotations. |
| `CheckTyres698` | `c-function-rest` | `function type parameter form outside the decided surface` | No section decides every rejected function-type parameter form, including a rest parameter. |
| `CheckTypeRules299` | `c-assign-map-width` | `nominal types are not interchangeable: the initializer expects `Map<i64, i64>`, got `Map<i32, i32>`` | No section decides invariant Map argument compatibility. C3 decides scalar numeric conversion. |
| `CheckTypeRules329` | `c-class-local` | `type mismatch: the initializer expects `C`, got `i32`` | No section decides builtin numeric type lookup over a local class of the same name. |

### WireEnumUntypedMember

No accepted witness exists for the untyped CEnum property guard.
The guard requires a TsPropertySignature without a type annotation. Strict TypeScript reports TS7008 for that form.
`new-cenum-untyped` measures TS7008. The site becomes TscRejects, with no WireEnumValues block.
Typed fractional, repeated, and oversized wire values retain the existing WireEnumValues sites.

### Fragment repairs

All 23 variants in the round 1 failure table pass the expanded fragment check.
Each TypeScript fragment is accepted by stock TypeScript and reaches a diagnostic with its own variant.
Each subscript fragment is accepted here or gives a concrete no-equivalent instruction.
AggregateLayoutLimit keeps both fragments. FixedArrayByteLimit now carries its existing variant, so its TypeScript fragment reaches that variant.
File markers identify real companion files. The checker receives ambient files and the entry role separately.
The TypeScript batch groups all companion errors under the fragment identity. It still uses one Node process.
RunnerMainMissing also checks runner_main after a successful program check.
InvalidProgramEntry checks two source files without an entry role, as section 129.1 requires.

The following blocks show every changed fragment, with its old and new text.

#### ArrayRestPattern

Old ts:

```ts
const xs: i32[] = [1, 2, 3];
const [head, ...rest] = xs;
```

New ts:

```ts
function probe(): void {
const xs: i32[] = [1, 2, 3];
const [head, ...rest] = xs;
}
```


#### BoundaryOnlyObject

Old ts:

```ts
class Box { value: i32 = 1; }
JSON.stringify(new Box() as object);
```

New ts:

```ts
function read(value: object): void {}
```


#### ByteAccessTarget

Old ts:

```ts
class Node { value: i32 = 0; }
Context.bytesOf<Node>(node);
```

New ts:

```ts
class Node { value: i32 = 0; }
const node: Node = new Node();
Context.bytesOf<Node>(node);
```

Old subscript:

```ts
@ValueType class Point { x: i32 = 0; }
Context.bytesOf<Point>(point);
```

New subscript:

```ts
@ValueType class Point { x: i32 = 0; }
const point: Point = new Point();
Context.bytesOf<Point>(point);
```


#### ClassIndexSignature

Old subscript:

```ts
class Values { [i: u32]: i32; get(i: u32): i32 { return 0; } set(i: u32, v: i32): void {} }
values[0] = 2;
const changed: i32 = values[0];
```

New subscript:

```ts
class Values { [i: u32]: i32; get(i: u32): i32 { return 0; } set(i: u32, v: i32): void {} }
const values: Values = new Values();
values[0] = 2;
const changed: i32 = values[0];
```


#### DroppedAsyncHandle

Old ts:

```ts
work();
```

New ts:

```ts
async function work(): Promise<void> { await Context.suspend(); }
function probe(): void { work(); }
```

Old subscript:

```ts
await work();
```

New subscript:

```ts
async function work(): Promise<void> { await Context.suspend(); }
async function probe(): Promise<void> { await work(); }
```


#### EmbeddedHeaderCopy

Old ts:

```ts
const copied: SubChainHeader = extension.header;
print(`${copied.sType}`);
```

New ts:

```ts
// file: main.ts
export function main(): void { const e: Ext = new Ext(new Header(null), 1); const h = e.header; }
// file: mirror.d.ts
// @subscript-c-header include="probe.h"
declare class Header { next: Header|null; constructor(next: Header|null); }
declare class Ext { header: Header; x: i32; constructor(header: Header,x: i32); }
```

Old subscript:

```ts
print(`${extension.header.sType}`);
```

New subscript:

```ts
// file: mirror.d.ts
// @subscript-c-header include="probe.h"
declare class Header { next: Header|null; constructor(next: Header|null); }
declare class Ext { header: Header; x: i32; constructor(header: Header,x: i32); }
// file: main.ts
export function main(): void { const e: Ext = new Ext(new Header(null), 1); const h: Header | null = e.header; }
```


#### GenericInferenceMissing

Old subscript:

```ts
function empty<T>(): T | null { return null; }
empty<i32>();
```

New subscript:

```ts
class Item {}
function empty<T>(): T | null { return null; }
empty<Item>();
```


#### ModuleInitializerOrder

Old ts:

```ts
const g: Box = f();
function f(): Box { return h; }
const h: Box = new Box();
```

New ts:

```ts
class Box {}
const g: Box = f();
function f(): Box { return h; }
const h: Box = new Box();
```

Old subscript:

```ts
const h: Box = new Box();
function f(): Box { return h; }
const g: Box = f();
```

New subscript:

```ts
class Box {}
const h: Box = new Box();
function f(): Box { return h; }
const g: Box = f();
```


#### NamedModuleSurface

Old ts:

```ts
// lib.ts
export const value: i32 = 1;
// main.ts
export * from "./lib";
```

New ts:

```ts
export default function read(): void {}
```

Old subscript:

```ts
// lib.ts
export const value: i32 = 1;
// main.ts
export { value } from "./lib";
```

New subscript:

```ts
export function main(): void {}
```


#### NestedPattern

Old ts:

```ts
const xss: i32[][] = [[1, 2]];
const [[first, second]] = xss;
```

New ts:

```ts
function probe(): void {
const xss: i32[][] = [[1, 2]];
const [[first, second]] = xss;
}
```


#### ObjectRestPattern

Old ts:

```ts
class Point { x: i32 = 1; y: i32 = 2; }
const { x, ...rest } = new Point();
```

New ts:

```ts
function probe(): void {
class Point { x: i32 = 1; y: i32 = 2; }
const { x, ...rest } = new Point();
}
```


#### OptionalChainIndex

Old ts:

```ts
const values: i32[] | null = [];
const value = values?.[0];
```

New ts:

```ts
class Values { [i: u32]: i32; data: i32[] = [1]; get(i: u32): i32 { return this.data[i as i32]; } set(i: u32, v: i32): void { this.data[i as i32] = v; } }
function probe(values: Values | null): void { const value: i32 = values?.[0] ?? 0; }
```


#### OptionalDescriptorMember

Old subscript:

```ts
type Mode = "fast" | "safe";
@Descriptor class D { value?: i32 = 1; mode?: Mode; }
if (d.mode !== undefined) { print(`${d.mode}`); }
```

New subscript:

```ts
type Mode = "fast" | "safe";
@Descriptor class D { value?: i32 = 1; mode?: Mode; }
const d: D = {};
if (d.mode !== undefined) { print(`${d.mode}`); }
```


#### PatternDefaultValue

Old ts:

```ts
const xs: i32[] = [];
const [first = 1] = xs;
```

New ts:

```ts
function probe(): void {
const xs: i32[] = [];
const [first = 1] = xs;
}
```


#### PatternFieldName

Old ts:

```ts
class Point { x: i32 = 1; }
const key = "x" as const;
const { [key]: value } = new Point();
```

New ts:

```ts
function probe(): void {
class Point { x: i32 = 1; }
const key = "x" as const;
const { [key]: value } = new Point();
}
```


#### PatternSourceShape

Old ts:

```ts
const text = "ab";
const [first, second] = text;
```

New ts:

```ts
function probe(): void {
const text = "ab";
const [first, second] = text;
}
```


#### PromiseObject

Old ts:

```ts
const pending = Promise.resolve(1);
leaf().then((v) => print(`${v}`));
```

New ts:

```ts
const pending = Promise.resolve(1);
```

Old subscript:

```ts
const value: i32 = await leaf();
```

New subscript:

```ts
async function leaf(): Promise<i32> { return 1; }
async function probe(): Promise<void> { const value: i32 = await leaf(); }
```


#### SharedLocationNarrowing

Old ts:

```ts
if (h.c !== null) { clear(h); print(`${h.c.v}`); }
```

New ts:

```ts
class Child { v: i32 = 1; }
class Holder { c: Child | null = new Child(); }
function clear(h: Holder): void { h.c = null; }
function probe(h: Holder): void { if (h.c !== null) { clear(h); print(`${h.c.v}`); } }
```

Old subscript:

```ts
const c = h.c; if (c !== null) { clear(h); print(`${c.v}`); }
```

New subscript:

```ts
class Child { v: i32 = 1; }
class Holder { c: Child | null = new Child(); }
function clear(h: Holder): void { h.c = null; }
function probe(h: Holder): void { const c: Child | null = h.c; if (c !== null) { clear(h); print(`${c.v}`); } }
```


#### SwitchOverAlias

Old ts:

```ts
switch (phase) { case "queued": break; }
```

New ts:

```ts
type Phase = "queued" | "done";
function probe(phase: Phase): void { switch (phase) { case "queued": break; } }
```

Old subscript:

```ts
switch (phase) { case "queued": break; default: break; }
```

New subscript:

```ts
type Phase = "queued" | "done";
function probe(phase: Phase): void { switch (phase) { case "queued": break; default: break; } }
```


#### ThisBeforeFieldValues

Old ts:

```ts
class Holder {
  inner: Inner;
  constructor() {
    this.show();
    this.inner = new Inner();
  }
}
```

New ts:

```ts
class Inner {}
class Holder { inner: Inner; constructor() { this.show(); this.inner = new Inner(); } show(): void {} }
```

Old subscript:

```ts
class Holder {
  inner: Inner;
  constructor() {
    this.inner = new Inner();
    this.show();
  }
}
```

New subscript:

```ts
class Inner {}
class Holder { inner: Inner; constructor() { this.inner = new Inner(); this.show(); } show(): void {} }
```


#### UsingDeclaration

Old ts:

```ts
await using resource = new Resource();
const f = (): i32 => { using r = new Resource(); return 1; };
```

New ts:

```ts
class Resource { [Symbol.dispose](): void {} }
async function probe(): Promise<void> { await using resource = new Resource(); }
```

Old subscript:

```ts
using resource = new Resource();
```

New subscript:

```ts
class Resource { [Symbol.dispose](): void {} }
function probe(): void { using resource = new Resource(); }
```


#### WorkerContextAffinity

Old ts:

```ts
class RefMessage { value: object = {}; }
const w: Worker<RefMessage, RefMessage> = Worker.spawn(echo);
```

New ts:

```ts
class Child { value: i32 = 1; }
class RefMessage { value: Child = new Child(); }
function entry(inbox: Inbox<RefMessage>, outbox: Outbox<RefMessage>): void {}
const worker: Worker<RefMessage, RefMessage> = Worker.spawn(entry);
```

Old subscript:

```ts
class CountMessage { count: i32 = 0; }
function run(): void { const w: Worker<CountMessage, CountMessage> = Worker.spawn(echo); }
```

New subscript:

```ts
class Message { value: i32 = 0; }
function entry(inbox: Inbox<Message>, outbox: Outbox<Message>): void {}
function run(): void { const worker: Worker<Message, Message> = Worker.spawn(entry); }
```

### Corpus changes

The four old programs remain in the round 1 source witness table, with their measured rejected labels.
Four new round 2 witnesses measure the current programs as TypeScript-accepted. No reject entry retires.
No accept program or golden changes.

| Entry | Change | Measured first diagnostic |
| --- | --- | --- |
| `r62-valuetype-fixed-array-layout-too-large.ts` | Initialize data with an empty array; TS2564 becomes acceptance. | S100, 9:9: `FixedArray` byte size exceeds the supported aggregate limit of 2147483647 bytes; AggregateLayoutLimit |
| `r148-switch-cross-case-read.ts` | Put the cross-case read inside a closure; TS2454 becomes acceptance. | S100, 14:61: `caseValue` is read from a different switch case; SwitchCaseRead |
| `r154-namespace-read-before-declaration.ts` | Put the shadowed Math read inside a closure; TS2339, TS2448, TS2454 become acceptance. | S100, 9:40: `Math` is read before its declaration in this block; BlockNameReadBeforeDeclaration |
| `r155-class-read-before-declaration.ts` | Put the shadowed Foo read inside a closure; TS2351, TS2448, TS2454 become acceptance. | S100, 17:38: `Foo` is read before its declaration in this block; BlockNameReadBeforeDeclaration |

The constraint split keeps r287-type-argument-outside-constraint.ts unchanged, without a block.
The namespace split keeps r320-namespace-missing/main.ts unchanged, without a block.

### Messages

The scan covers every touched production site and each carried failure message.
No Diverges message says that TypeScript rejects its form. No diagnostic text changes: the old/new text list is empty.
The corpus programs keep each target diagnostic message; only their positions inside the same line change.

### Test and representation changes

RejectionSite::Api keeps the row identity and its variant, rather than the complete API row.
This removes the large-error warnings from RejectionFailure without dropping the producer site.
The production-variant mismatch control still constructs an incorrect variant, independently of the witness table.
The existing const-container test expects ContextAffineContainerArgument, whose reason names its storage restriction.
The generic matrix names stdlib section 16.2 for the plain-message-class restriction at every Worker, Inbox, and Outbox API.
The matrix treats Q3 and collisions.md Q3 as the same section identity. It keeps its independent restriction and code checks.
Its admitted and omitted instance counts remain unchanged.
Two rejected group b witnesses measure TS2552 instead of TS2304 in the expanded TypeScript batch.
Their labels change: b-class_shape-791 and b-class_shape-800-pattern. Their checker targets do not change.
The TypeScript rejection class remains unchanged.
The divergence entry match moves into compiler/src/divergence/entries.rs. The match remains the single fragment table.
Every Rust file remains below 2,000 lines. New test-only helpers stay inside the cfg(test) module.

### Verification and cost

- `cargo test --offline --locked -p subscript-compiler`: 386 library tests pass; only the rule 4 test fails, on 267 Red sites.
- `cargo test --offline --locked -p subscript-compiler -- --skip every_subset_rejection_carries_its_divergence`: all remaining test binaries and doc tests pass.
- `cargo test --offline --locked -p subscript-compiler --test corpus_reject`: 45 tests pass.
- `cargo fmt --check`: passes.
- `cargo clippy --offline --locked -p subscript-compiler --all-targets`: passes; two library warnings, below the seven-warning tools/gate.sh baseline.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference`: passes.
- `tools/gate.sh`: does not run.

The run without rule 4 passes 923 tests across 54 executable results, including doc tests.
Their reported executable durations total 53.19 seconds. This excludes compilation and process startup.

The final rule 4 run measures 991 witnesses and 170 carried variants.
TypeScript costs 0.483 seconds, the checker costs 0.258 seconds, and the total costs 0.927 seconds.
The test doc comment states the rounded cost. One TypeScript process covers the full batch.

### All changed files

This list includes the preserved round 1 changes and every round 2 change.
- `compiler/src/ambient.rs`
- `compiler/src/api_reference.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/container_argument.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/exports.rs`
- `compiler/src/check/expr/aggregate.rs`
- `compiler/src/check/expr/array_of_and_map_copy.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/expr/namespace.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/host_entries.rs`
- `compiler/src/check/inference.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/instance_chain.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/layout.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mirror_provenance.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/namespace_import.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/opaque.rs`
- `compiler/src/check/pattern.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/divergence.rs`
- `compiler/src/hir/host_entry.rs`
- `compiler/src/lib.rs`
- `compiler/src/parse.rs`
- `compiler/src/provenance.rs`
- `compiler/src/regex.rs`
- `compiler/src/tests/collections.rs`
- `compiler/tests/generic_tsc_matrix.rs`
- `compiler/tests/generic_tsc_matrix/api.rs`
- `compiler/tests/generic_tsc_matrix/kinds.rs`
- `corpus/reject/r148-switch-cross-case-read.ts`
- `corpus/reject/r154-namespace-read-before-declaration.ts`
- `corpus/reject/r155-class-read-before-declaration.ts`
- `corpus/reject/r62-valuetype-fixed-array-layout-too-large.ts`
- `generated-docs/language-reference.md`
- `compiler/src/check/rejection_programs.rs`
- `compiler/src/check/rejection_programs_s154.txt`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets_s154.txt`
- `compiler/src/check/rejection_tsc.cjs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/divergence/entries.rs`
- `specs/tracking/s154-rejection-sites.md`

## Round 3

Status: the authorized group b pass is complete, with the acceptance 3 list below.
The migration remains incomplete. No commit occurred.
The round preserves the round 1 and round 2 base. It closes 74 Red arms with 74 new variants.
The table still has 671 direct sites and 79 API row identities: 750 sites.
The Red count falls from 267 to 193. Group b retains 35 undecided sites.
Group a retains 125 Red sites. The round 2 undecided list retains its 33 sites.

### Conditional class audit

The audit compares the seven group b files with HEAD and reads each Red guard.
No group b conditional variant choice lost a condition. This round adds no guard split.
The static method and static field paths still carry ThisStaticField through the frame.
The generic method guard still separates the declared synchronous template from every other bodiless generic method.
The descriptor, value-class, and reference-class branches remain separate.
The initializer route and direct-read split from round 2 remains unchanged.

### Closed sites and renames

Each row names the old site, its new guard and variant, the decision section, and its accepted witness.
Every variant is new. No two sites share a new variant.
The total test measures each new fragment and a diagnostic with its variant.

| Old site | Guard and variant | Collision | Accepted witness |
| --- | --- | --- | --- |
| `CheckBodies225` | `FunctionBodyMissing` | `compiler.md §108.1` | `b-declarations-361` |
| `CheckBodies387` | `StaticFieldInitializerMissing` | `compiler.md §108.1` | `b-bodies-387` |
| `CheckBodies713` | `FieldAssignmentAfterReturn` | `compiler.md §108.1` | `b-bodies-713` |
| `CheckBodies734` | `FieldAssignmentMissing` | `compiler.md §108.1` | `b-bodies-734-throw` |
| `CheckBodies824` | `ConstructorFieldReadBeforeAssignment` | `compiler.md §108.4` | `b-bodies-824-accept` |
| `CheckStmt1007` | `ForOfAwaitUsing` | `compiler.md §60.1` | `b-stmt-1007` |
| `CheckStmt1260` | `AliasCaseNonLiteral` | `compiler.md §41` | `b-stmt-1260` |
| `CheckClassShape83` | `ClassMemberNameClash` | `compiler.md §65.1` | `b-class_shape-83-accept` |
| `CheckClassShape150` | `DescriptorMethod` | `compiler.md §25.1` | `b-class_shape-150` |
| `CheckClassShape162` | `MirrorStaticMethod` | `compiler.md §71.1` | `b-class_shape-162` |
| `CheckClassShape170` | `DisposeStatic` | `compiler.md §60.1` | `b-class_shape-170` |
| `CheckClassShape197` | `MirrorAccessor` | `compiler.md §65.1` | `b-class_shape-197` |
| `CheckClassShape223` | `ReadAccessorReturnMissing` | `compiler.md §65.1` | `b-class_shape-223` |
| `CheckClassShape296` | `WriteAccessorPattern` | `compiler.md §65.1` | `b-class_shape-296` |
| `CheckClassShape305` | `WriteAccessorTypeMissing` | `compiler.md §65.1` | `b-class_shape-305` |
| `CheckClassShape386` | `GenericMethodBodyMissing` | `compiler.md §82.4` | `b-class_shape-386-accept` |
| `CheckClassShape414` | `DisposeAsync` | `compiler.md §60.1` | `b-class_shape-414` |
| `CheckClassShape423` | `DisposeSignature` | `compiler.md §60.1` | `b-class_shape-423` |
| `CheckClassShape461` | `DescriptorInheritance` | `compiler.md §25.1` | `b-class_shape-461` |
| `CheckClassShape463` | `ReferenceClassInheritance` | `compiler.md §115.1` | `b-class_shape-463` |
| `CheckClassShape485` | `DescriptorStaticField` | `compiler.md §71.1` | `b-class_shape-485` |
| `CheckClassShape493` | `MirrorStaticField` | `compiler.md §71.1` | `b-class_shape-493` |
| `CheckClassShape511` | `StaticFieldOptional` | `C7` | `b-class_shape-511` |
| `CheckClassShape529` | `ContextAffineStaticField` | `compiler.md §40` | `b-class_shape-529-accept` |
| `CheckClassShape578` | `DescriptorInitializerWithoutOptional` | `compiler.md §25.1` | `b-class_shape-578` |
| `CheckClassShape586` | `DescriptorRequiredWithoutDefinite` | `compiler.md §25.1` | `b-class_shape-586-accept` |
| `CheckClassShape595` | `InstanceFieldOptional` | `C7` | `b-class_shape-595` |
| `CheckClassShape624` | `WireAliasNestedField` | `compiler.md §52.2` | `b-class_shape-624` |
| `CheckClassShape653` | `ContextAffineInstanceField` | `compiler.md §40` | `b-class_shape-653-accept` |
| `CheckClassShape679` | `ValueFieldOutsideWhitelist` | `C2` | `b-class_shape-679` |
| `CheckClassShape701` | `DescriptorConstructor` | `compiler.md §25.1` | `b-class_shape-701` |
| `CheckClassShape719` | `WireAliasNestedConstructorParameter` | `compiler.md §52.2` | `b-class_shape-719` |
| `CheckClassShape765` | `ClassIndexSignatureCount` | `compiler.md §58.1` | `b-class_shape-765-accept` |
| `CheckClassShape774` | `ClassIndexSignatureNonReference` | `compiler.md §58.1` | `b-class_shape-774` |
| `CheckClassShape781` | `ClassIndexSignatureStatic` | `compiler.md §58.1` | `b-class_shape-781` |
| `CheckClassShape813` | `ClassIndexSignatureIndexType` | `compiler.md §58.1` | `b-class_shape-813` |
| `CheckClassShape859` | `WriteAccessorWithoutRead` | `compiler.md §65.1` | `a-s029` |
| `CheckClassShape881` | `AccessorTypeMismatch` | `compiler.md §65.1` | `b-class_shape-881` |
| `CheckClassShape936` | `ClassIndexSetSignature` | `compiler.md §58.1` | `b-class_shape-909` |
| `CheckDeclarations119` | `ModuleUsing` | `compiler.md §60.1` | `b-declarations-119` |
| `CheckDeclarations215` | `DescriptorOptions` | `compiler.md §62.1` | `b-declarations-215-accept` |
| `CheckDeclarations223` | `UnsupportedClassDecorator` | `compiler.md §130.1` | `b-declarations-223` |
| `CheckDeclarations232` | `DescriptorValueType` | `compiler.md §25.1` | `b-declarations-232` |
| `CheckDeclarations474` | `EnumImplicitValueOverflow` | `compiler.md §72` | `b-declarations-474` |
| `CheckDeclarations577` | `WireEnumEmpty` | `compiler.md §50.1` | `b-declarations-577` |
| `CheckDeclarations599` | `WireEnumMemberForm` | `compiler.md §50.1` | `b-declarations-599-index` |
| `CheckDeclarations610` | `WireEnumMemberKey` | `compiler.md §50.1` | `b-declarations-610` |
| `CheckDeclarations818` | `MirrorVariableForm` | `compiler.md §136.1` | `b-declarations-818` |
| `CheckSignatures47` | `WireAliasNestedForeignParameter` | `compiler.md §52.2` | `b-signatures-47` |
| `CheckSignatures60` | `WireAliasNestedForeignReturn` | `compiler.md §50.2` | `b-signatures-60` |
| `CheckSignatures79` | `ForeignDirectCallback` | `compiler.md §23.3` | `b-signatures-79` |
| `CheckSignatures106` | `ForeignReturnProvenance` | `compiler.md §23.3` | `b-signatures-106` |
| `CheckSignatures409` | `AsyncGeneratorFunction` | `compiler.md §26.1` | `b-signatures-409` |
| `CheckSignatures425` | `AsyncReturnAnnotationMissing` | `compiler.md §26.1` | `b-signatures-425` |
| `CheckSignatures509` | `OptionalParameter` | `C7` | `b-signatures-509` |
| `CheckException235` | `ErrorMessageType` | `compiler.md §115.1` | `b-exception-235-alias` |
| `CheckException244` | `ErrorConstructorArguments` | `compiler.md §115.1` | `b-exception-244` |
| `CheckException282` | `ErrorCallWithoutNew` | `compiler.md §115.1` | `b-exception-282` |
| `CheckDeclarationsFailure121` | `ValueTypeArgumentCount` | `compiler.md §62.1` | `new-align-count` |
| `CheckDeclarationsFailure124` | `ValueTypeOptionsNonLiteral` | `compiler.md §62.1` | `new-align-object` |
| `CheckDeclarationsFailure127` | `ValueTypeOptionCount` | `compiler.md §62.1` | `new-align-empty` |
| `CheckDeclarationsFailure130` | `ValueTypeOptionSpread` | `compiler.md §62.1` | `new-align-spread` |
| `CheckDeclarationsFailure133` | `ValueTypeOptionPropertyForm` | `compiler.md §62.1` | `new-align-method` |
| `CheckDeclarationsFailure141` | `ValueTypeOptionKey` | `compiler.md §62.1` | `new-align-key` |
| `CheckDeclarationsFailure144` | `ValueTypeAlignmentNonLiteral` | `compiler.md §62.1` | `new-align-string` |
| `CheckDeclarationsFailure148` | `ValueTypeAlignmentOutsideSet` | `compiler.md §62.1` | `new-align-value` |
| `CheckClassShape136` | `ComputedMethodName` | `compiler.md §60.1` | `b-class_shape-136` |
| `CheckExports73` | `MirrorExportList` | `compiler.md §128.1` | `b-exports-73` |
| `CheckDeclarations40` | `TopLevelNameClash` | `compiler.md §125.1` | `b-declarations-40` |
| `CheckDeclarations78` | `UnsupportedModuleDeclaration` | `C18` | `b-declarations-78-namespace` |
| `CheckSignatures234` | `PoisonedDefaultImport` | `C18` | `b-signatures-234-lib` |
| `CheckSignatures300` | `DefaultImport` | `C18` | `b-signatures-300` |
| `CheckStmt391` | `NullInitializerInference` | `compiler.md §97` | `b-stmt-391` |
| `CheckStmt425` | `UsingBindingResourceType` | `compiler.md §97.1` | `b-stmt-425-null` |

### Undecided sites: acceptance 3

No collision or variant covers these sites. Their Red arms and programs remain.
The table gives one accepted witness per site and its target message.
The round 2 undecided table remains unchanged.

| Site | Accepted witness | Target message | Unresolved restriction |
| --- | --- | --- | --- |
| `CheckBodies123` | `b-bodies-123` | `var` is not in the language; use `let` or `const` | No section decides the blanket rejection of var declarations. |
| `CheckBodies158` | `b-bodies-158` | module-level variables require an initializer | No section decides the initializer requirement for every module variable. |
| `CheckBodies261` | `b-bodies-261-enum` | not all paths return a value | No section decides the loss of return completeness after an exhaustive enum switch. |
| `CheckStmt210` | `b-stmt-210` | nested declarations are not in the decided surface | No section decides every rejected nested declaration, including a local class. |
| `CheckStmt295` | `b-stmt-295` | statement form outside the decided surface | No section decides every rejected statement form, including debugger. |
| `CheckStmt311` | `b-stmt-311` | `var` is not in the language; use `let` or `const` | No section decides the blanket rejection of local var declarations. |
| `CheckStmt359` | `b-stmt-359` | local declarations require an initializer | No section decides the initializer requirement for every local declaration. |
| `CheckStmt475` | `b-stmt-475` | generator return values are not in the decided surface | No section decides the rejection of a generator return value. |
| `CheckStmt535` | `c-condition` | condition must be boolean, got `i32` | Section 143 names the absence of a source rule for boolean-only conditions. |
| `CheckStmt788` | `b-stmt-788` | `for await…of` requires the Promise object/iterator surface, which is not in the language | No section decides the rejection of for await over a synchronous array. |
| `CheckStmt994` | `b-stmt-994` | `var` is not in the language; use `let` or `const` | No section decides the blanket rejection of var loop bindings. |
| `CheckStmt1019` | `b-stmt-1019` | `for…of` requires a `const` or `let` identifier binding | No section decides the rejection of an assignment target in a for-of head. |
| `CheckStmt1101` | `b-stmt-1101-accept` | `values()` expects no arguments | No section decides the count of an empty tuple spread as one iterator-method argument. |
| `CheckStmt1298` | `b-stmt-1298` | switch discriminants are integers, enums, strings, or string-literal union aliases; got `boolean` | No section decides the rejection of a boolean switch discriminant. |
| `CheckClassShape354` | `b-class_shape-354` | generator methods are not in the decided surface | No section decides the blanket rejection of synchronous generator methods. |
| `CheckClassShape475` | `b-class_shape-475` | computed or non-identifier field names are not decided | No section decides every non-identifier field key, including a constant string key. |
| `CheckClassShape520` | `b-class_shape-520` | static fields require a type annotation | No section decides the explicit type annotation requirement for every static field. |
| `CheckClassShape612` | `b-class_shape-612` | fields require a type annotation | No section decides the explicit type annotation requirement for every instance field. |
| `CheckClassShape732` | `b-class_shape-732` | constructor parameter properties are not decided | No section decides the blanket rejection of constructor parameter properties. |
| `CheckClassShape841` | `b-class_shape-841` | class member form outside the decided surface | No section decides every rejected class member form, including an ECMAScript private field. |
| `CheckExports101` | `b-exports-101-lib` | export source module `s154_external_101` is not among the program's files | No section decides the exclusion of an external module that an ambient declaration supplies. |
| `CheckExports370` | `b-exports-370-augment` | `missing_exports_370_augment` is not exported by `./other` | No section decides the exclusion of a named export that module augmentation supplies. |
| `CheckDeclarations131` | `b-declarations-131` | declaration form outside the decided surface | No section decides every rejected source declaration form, including an interface. |
| `CheckDeclarations461` | `b-declarations-461` | string enum member names are not decided | No section decides the rejection of a string-literal enum member name. |
| `CheckDeclarations518` | `b-declarations-518` | string-literal union aliases cannot be generic | No section decides the rejection of type parameters on a literal-union alias. |
| `CheckDeclarations530` | `b-declarations-530` | type aliases are limited to a union of two or more string literals | No section decides every rejected alias form, including a sized numeric alias. |
| `CheckDeclarations550` | `b-declarations-550` | duplicate string-literal union member `a` | No section decides the rejection of repeated members in a literal-union alias. |
| `CheckDeclarations749` | `b-declarations-749` | mirror declaration form outside the decided surface | No section decides every rejected mirror declaration form, including an ambient namespace. |
| `CheckSignatures213` | `b-signatures-213` | imported module `./missing` is not among the program's files | No section decides a diagnostic for an absent side-effect import in the discovery path. |
| `CheckSignatures263` | `b-signatures-263-lib` | imported module `s154_external_263` is not among the program's files | No section decides the exclusion of an external module that an ambient declaration supplies. |
| `CheckSignatures317` | `b-signatures-317-augment` | `missing_signatures_317_augment` is not exported by `./other` | No section decides the exclusion of an imported name that module augmentation supplies. |
| `CheckSignatures373` | `b-signatures-373` | module-level variables require a type annotation | No section decides the explicit type annotation requirement for every module variable. |
| `CheckSignatures478` | `b-signatures-478` | function return types must be annotated | No section decides the explicit return annotation requirement for every synchronous function. |
| `CheckSignatures519` | `b-signatures-519` | parameters require a type annotation | No section decides the explicit type annotation requirement for an identifier parameter with an inferred default. |
| `CheckSignatures548` | `b-signatures-548` | parameters require a type annotation | No section decides the explicit type annotation requirement for a pattern parameter with an inferred default. |

### Fragment additions

No existing variant fragment changes. Each new variant starts with the accepted source that its closed-site row names.
Each subscript fragment states "no equivalent" and gives a concrete alternative.
PoisonedDefaultImport uses the same discovery option that its source witness supplies.
Its fragment names a separate external module to prevent a duplicate ambient declaration in the TypeScript batch.
The checker test supplies that module in poison_missing_modules. The source alone cannot reach this configuration guard.
AliasCaseNonLiteral cites compiler.md section 41, as the independent generic matrix restriction does.
The section 41.1 source rule and the matrix counts remain unchanged.

### Corpus changes

Twelve entries now use accepted TypeScript programs. Three entries retire.
Each old program remains as a named round3-old witness with its measured TypeScript rejection codes.
Each rewritten program also becomes a named round3-corpus witness. No accept entry or golden changes.
The table gives the first checker diagnostic after each rewrite, or before retirement.

| Entry | Change | Measured first diagnostic |
| --- | --- | --- |
| `r94-descriptor-method.ts` | Add a nullish fallback to the rejected method body. | S100, 10:3: descriptor classes cannot declare methods; DescriptorMethod |
| `r133-using-without-dispose.ts` | Use an explicitly null-typed binding at the same resource-type guard. | S100, 10:3: a `using` binding must be a reference class with a disposal hook, or that class or null; UsingBindingResourceType |
| `r192-using-nullable-without-dispose.ts` | Use an explicitly null-typed binding at the same resource-type guard. | S100, 10:3: a `using` binding must be a reference class with a disposal hook, or that class or null; UsingBindingResourceType |
| `r136-valuetype-align-not-in-set.ts` | Use a source decorator with an accepted call signature; retain the invalid alignment. | S100, 7:1: `@ValueType` alignment must be an integer literal in {2, 4, 8, 16}; ValueTypeAlignmentOutsideSet |
| `r137-descriptor-align.ts` | Use a source decorator with an accepted call signature; retain the rejected options. | S100, 7:1: `@Descriptor` does not accept options; DescriptorOptions |
| `r146-accessor-field-name-clash.ts` | Replace the field/accessor clash with a method overload at the same member-namespace guard. | S017, 10:3: a method cannot share the member name `current` with a method; ClassMemberNameClash |
| `r161-field-method-member-name-clash.ts` | Retire; the old field/method clash remains a rejected witness. | S017, 9:3: a method cannot share the member name `x` with a field; ClassMemberNameClash |
| `r162-duplicate-method-member-name.ts` | Replace the first implementation with an overload signature. | S017, 12:3: a method cannot share the member name `x` with a method; ClassMemberNameClash |
| `r163-duplicate-field-member-name.ts` | Retire; the old duplicate field remains a rejected witness. | S017, 9:3: a field cannot share the member name `x` with a field; ClassMemberNameClash |
| `r164-duplicate-static-member-name.ts` | Retire; the old static field/method clash remains a rejected witness. | S017, 9:10: a static method cannot share the member name `value` with a field; ClassMemberNameClash |
| `r224-field-without-initializer.ts` | Add a constructor that always throws and assigns no field. | S100, 8:3: field `count` of `Counter` has no initializer, and no constructor statement assigns it; write `count: i32 = …`, or assign `this.count = …` at the top level of the constructor; FieldAssignmentMissing |
| `r225-reference-field-without-initializer.ts` | Add a constructor that always throws and assigns no field. | S100, 12:3: field `inner` of `Holder` has no initializer, and no constructor statement assigns it; write `inner: Inner = …`, or assign `this.inner = …` at the top level of the constructor; FieldAssignmentMissing |
| `r228-field-assigned-after-early-return.ts` | Use a constant false condition on the early return. | S100, 12:3: field `inner` of `Holder` is assigned at the constructor's top level after a statement that holds a `return`, so the constructor can return before the assignment; write `inner: Inner = …`, or assign `this.inner = …` at the top level of the constructor, before every statement that holds a `return`; FieldAssignmentAfterReturn |
| `r230-field-read-before-its-assignment.ts` | Use the definite-assignment spelling on the read field. | S100, 15:14: `this.inner` reads field `inner` of `Holder` before the constructor assigns it at its top level; move the read after `this.inner = …`, or give `inner` an initializer; ConstructorFieldReadBeforeAssignment |
| `r292-generic-uninitialized-field.ts` | Add a constructor that always throws and assigns no field. | S100, 10:3: field `v` of `G<T>` has no initializer, and no constructor statement assigns it; write `v: i32 = …`, or assign `this.v = …` at the top level of the constructor; FieldAssignmentMissing |

The harness removes the three retired entries. The language-reference generator removes the former field/accessor example.
The generated corpus index removes the three retired entries. The remaining collision-index reference retains its rewritten r146 entry.

### Messages

The scan reads every touched production diagnostic and carried failure message.
Two texts drop a false TypeScript outcome. Every other touched message stays unchanged.

FieldAssignmentAfterReturn, old:

```text
field `{name}` of `{class_name}` is assigned at the constructor's top level after a statement that holds a `return`, so the constructor can return before the assignment (stock `tsc` answers TS2564); {spellings}, before every statement that holds a `return`
```

FieldAssignmentAfterReturn, new:

```text
field `{name}` of `{class_name}` is assigned at the constructor's top level after a statement that holds a `return`, so the constructor can return before the assignment; {spellings}, before every statement that holds a `return`
```

FieldAssignmentMissing, old:

```text
field `{name}` of `{class_name}` has no initializer, and no constructor statement assigns it (stock `tsc` answers TS2564); {spellings}
```

FieldAssignmentMissing, new:

```text
field `{name}` of `{class_name}` has no initializer, and no constructor statement assigns it; {spellings}
```

### Test and representation changes

The existing field, accessor-template, and declaration-duplicate tests expect the variants that their migrated guards now carry.
The recorded TypeScript forms keep their measured TypeScript classes. Their checker variant lists change to the migrated variants.
The generic matrix keeps its independent source restriction records and its admitted and omitted instance sets.
Divergence::ALL includes each new variant. The independent enum scan still reports a missing or repeated variant.
The constructor-prefix comment names the first complete assignment prefix and the new site A variant.
Every Rust file remains below 2,000 lines. The new fragment-option helper stays in the existing test-only module.

### Verification and cost

- `cargo test --offline --locked -p subscript-compiler`: 386 library tests pass; only rule 4 fails, on 193 Red sites.
- The same suite with `-- --skip every_subset_rejection_carries_its_divergence`: all remaining test binaries and doc tests pass.
- `cargo test --offline --locked -p subscript-compiler --test corpus_reject`: 45 tests pass.
- `cargo fmt --check`: passes.
- `cargo clippy --offline --locked -p subscript-compiler --all-targets`: passes, with two library warnings against the seven-warning baseline.
- Clippy reports 13 existing test-only warnings. None comes from a round 3 change.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference`: passes.
- `tools/gate.sh`: does not run.

The final rule 4 run measures 1,018 witnesses and 244 carried variants.
TypeScript costs 0.508 seconds, the checker costs 0.280 seconds, and the total costs 0.969 seconds.
One TypeScript process covers the full batch. The test doc comment states the rounded cost.

The run without rule 4 passes 923 tests across 54 executable results, including doc tests.
Their reported executable durations total 53.88 seconds. This excludes compilation and process startup.

### All changed files

This list includes the preserved round 1 and round 2 changes and every round 3 change.
- `compiler/src/ambient.rs`
- `compiler/src/api_reference.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/container_argument.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/exports.rs`
- `compiler/src/check/expr/aggregate.rs`
- `compiler/src/check/expr/array_of_and_map_copy.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/expr/namespace.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/host_entries.rs`
- `compiler/src/check/inference.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/instance_chain.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/layout.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mirror_provenance.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/namespace_import.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/opaque.rs`
- `compiler/src/check/pattern.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/divergence.rs`
- `compiler/src/hir/host_entry.rs`
- `compiler/src/language_reference.rs`
- `compiler/src/lib.rs`
- `compiler/src/parse.rs`
- `compiler/src/provenance.rs`
- `compiler/src/regex.rs`
- `compiler/src/tests/collections.rs`
- `compiler/tests/async_generic_method.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/field_values.rs`
- `compiler/tests/generic_method.rs`
- `compiler/tests/generic_tsc_matrix.rs`
- `compiler/tests/generic_tsc_matrix/api.rs`
- `compiler/tests/generic_tsc_matrix/kinds.rs`
- `compiler/tests/re_exports.rs`
- `compiler/tests/tsc_corpus.rs`
- `corpus/reject/r133-using-without-dispose.ts`
- `corpus/reject/r136-valuetype-align-not-in-set.ts`
- `corpus/reject/r137-descriptor-align.ts`
- `corpus/reject/r146-accessor-field-name-clash.ts`
- `corpus/reject/r148-switch-cross-case-read.ts`
- `corpus/reject/r154-namespace-read-before-declaration.ts`
- `corpus/reject/r155-class-read-before-declaration.ts`
- `corpus/reject/r161-field-method-member-name-clash.ts`
- `corpus/reject/r162-duplicate-method-member-name.ts`
- `corpus/reject/r163-duplicate-field-member-name.ts`
- `corpus/reject/r164-duplicate-static-member-name.ts`
- `corpus/reject/r192-using-nullable-without-dispose.ts`
- `corpus/reject/r224-field-without-initializer.ts`
- `corpus/reject/r225-reference-field-without-initializer.ts`
- `corpus/reject/r228-field-assigned-after-early-return.ts`
- `corpus/reject/r230-field-read-before-its-assignment.ts`
- `corpus/reject/r292-generic-uninitialized-field.ts`
- `corpus/reject/r62-valuetype-fixed-array-layout-too-large.ts`
- `corpus/reject/r94-descriptor-method.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `compiler/src/check/rejection_programs.rs`
- `compiler/src/check/rejection_programs_s154.txt`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets_s154.txt`
- `compiler/src/check/rejection_tsc.cjs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/divergence/entries.rs`
- `specs/tracking/s154-rejection-sites.md`

## Round 4

Scope: group a, `check/expr.rs` and all `check/expr/` files. The rounds 1–3 working tree remains the base. No commit, accept corpus move, golden update, or spec edit outside this note. The rounds 2 and 3 undecided lists above remain unchanged.

Red falls from 193 to **125**: 68 prior undecided sites (33 round 2, 35 round 3) plus **57** group a sites. This round closes 71 original Red arms, adds 55 variants, and adds 40 distinct guard sites. The direct inventory is 711 sites; with 79 API identities it is 790. This is a partial migration, not phase completion.

### Conditional class audit

The HEAD audit finds two conditional variant choices in group a. Conditional joins retain `ConditionalJoinSizedOperandWidths` only when both apparent operands are numeric; other joins retain `ConditionalJoinGeneralUnionAndUndefined`. Missing `this` retains the frame-provided field-initializer site through `missing_this_site`; without that frame fact, `CheckExprEntry243` remains undecided. Neither condition was dropped. Ambient API dispatch retains each declared API identity and variant.

The following 40 splits use existing type, signature, frame, source member-name, or parsed declaration facts. Ordinary rejected cases use `TscRejects`. The three synchronous/generator value sites and the quoted descriptor-key site remain Red rather than inventing a source decision. A representative witness targets each reachable new guard in the independent source/target tables.

| Accepted-side guard | Other guard | Existing distinguishing fact | Accepted-side result |
| --- | --- | --- | --- |
| `FixedArrayObjectMember` | `FixedArrayUnknownMember` | super::is_object_member(name) | Diverges |
| `MapObjectMember` | `MapUnknownMember` | super::is_object_member(name) | Diverges |
| `SetObjectMember` | `SetUnknownMember` | super::is_object_member(name) | Diverges |
| `GeneratorResultObjectMember` | `GeneratorResultUnknownMember` | super::is_object_member(name) | Diverges |
| `NumericObjectMember` | `NumericUnknownMember` | super::is_object_member(name) | Diverges |
| `BoundaryObjectMember` | `BoundaryUnknownMember` | super::is_object_member(name) | Diverges |
| `WorkerStaticObjectMethod` | `WorkerStaticUnknownMethod` | super::is_object_member(&name) | Diverges |
| `WorkerObjectMethod` | `WorkerUnknownMethod` | super::is_object_member(&name) | Diverges |
| `InboxObjectMethod` | `InboxUnknownMethod` | super::is_object_member(&name) | Diverges |
| `OutboxObjectMethod` | `OutboxUnknownMethod` | super::is_object_member(&name) | Diverges |
| `FixedArrayObjectMethod` | `FixedArrayUnknownMethod` | super::is_object_member(&name) | Diverges |
| `NumericObjectMethod` | `NumericUnknownMethod` | super::is_object_member(name) | Diverges |
| `StringObjectMember` | `StringUnknownMember` | super::is_object_member(name) | Diverges |
| `MapObjectMethod` | `MapUnknownMethod` | super::is_object_member(name) | Diverges |
| `SetObjectMethod` | `SetUnknownMethod` | super::is_object_member(name) | Diverges |
| `ArrayObjectMember` | `ArrayUnknownMember` | super::is_object_member(name) | Diverges |
| `RegexCompile` | `RegexCompileUnknownMethod` | name == "compile" | Diverges |
| `AsyncMethodValue` | `SynchronousMethodValue` | sig.is_async | Diverges; other guard remains Red |
| `GenericAsyncMethodValue` | `GenericSynchronousMethodValue` | template.function.is_async | Diverges; other guard remains Red |
| `AsyncFunctionValue` | `GeneratorFunctionValue` | sig.is_async | Diverges; other guard remains Red |
| `AsyncGeneratorYield` | `YieldOutsideGenerator` | the current frame is async | Diverges |
| `ByteArgumentIdentity` | `ByteArgumentTypeMismatch` | matches!((&checked.ty, expected), (Type::FixedArray(..), Type::FixedArray(..)) \| (Type::Class(_), Type::Class(_))) | Diverges |
| `WorkerSpawnSpread` | `WorkerSpawnArgumentCount` | c.args.len() == 1 && c.args[0].spread.is_some() | Diverges |
| `WorkerEntryShape` | `WorkerEntryParameterCount` | sig.params.len() <= 2 | Diverges |
| `WorkerEntryStructuralEndpoints` | `WorkerEntryEndpointMismatch` | matches!((&self.apparent_type(&sig.params[0].ty), &self.apparent_type(&sig.params[1].ty)), (Type::Class(_), Type::Class(_))) | Diverges |
| `ArrayCallbackThisArgument` | `ArrayCallbackArgumentCount` | c.args.len() == 2 | Diverges |
| `MapCallbackThisArgument` | `MapCallbackArgumentCount` | c.args.len() == 2 | Diverges |
| `SetCallbackThisArgument` | `SetCallbackArgumentCount` | c.args.len() == 2 | Diverges |
| `DescriptorRequiredMemberQuotedKey` | `DescriptorRequiredMemberMissing` | a quoted literal source key names the required member | Red: no decided restriction |
| `FieldInitializerUninitializedRead` | `FieldInitializerDeclaredRead` | the parsed field has definite assignment and no initializer | Diverges |
| `DescriptorDefaultThisArithmetic` | `DescriptorDefaultOptionalNumericOperand` | the referenced descriptor field is a string | Diverges |
| `BareYieldNonVoid` | `BareYieldDeclaredNonVoid` | the generator yield type is inferred rather than annotated | Diverges |
| `NamespaceContextValue` | `NamespaceIncompatibleContext` | the apparent expected type is Object, Class, Error, or Nullable (Error preserves a rejected object annotation context) | Diverges |
| `ClassObjectMemberWrite` | `ClassUndeclaredPropertyWrite` | super::is_object_member(name) | Diverges |
| `ClassObjectMemberRead` | `ClassUndeclaredMemberRead` | super::is_object_member(name) | Diverges |
| `ClassObjectMethodCall` | `ClassUndeclaredMethodCall` | super::is_object_member(&name) | Diverges |
| `AwaitClassObjectMethod` | `AwaitClassUndeclaredMethod` | super::is_object_member(&name) | Diverges |
| `ClassRuntimeMember` | `ClassStaticMemberMissing` | super::is_function_member(prop) | Diverges |
| `GenericClassRuntimeMember` | `GenericClassStaticMemberMissing` | super::is_function_member(prop) | Diverges |
| `MapCopyNullableSource` | `MapCopyNullableNonMap` | the nullable source contains a builtin Map | Diverges |

`FieldInitializer.definite_uninitialized` carries the already parsed definite-assignment flag and absent initializer into the read guard. `Frame.yield_annotated` carries the existing signature yield-known-at-entry fact into the bare-yield guard. These facts choose diagnostic classes; they do not change acceptance or HIR. Object and Function member tests distinguish inherited TypeScript names from arbitrary missing names. No TypeScript probe, witness key, target code, or message chooses a production class.

`YieldOutsideGenerator` has no checker-reachable witness: the parser rejects yield outside generator syntax; synchronous generator functions have generator frames, and rejected generator methods have no signature so their bodies are skipped. The independent index records that reason. `AsyncGeneratorYield` is reached and uses the existing AsyncGeneratorFunction fragment.

### Closed sites and renames

Each row gives the old site, guard name, variant, source decision, and accepted witness. The independent source table retains the full programs.

| Old site | Guard name | Variant (new/reused) | Collision | Accepted witness |
| --- | --- | --- | --- | --- |
| `CheckExprOperator53` | `UnaryNumericCoercion` | `UnaryNumericCoercion` (new) | C3 | `a-s204` |
| `CheckExprOperator106` | `BitwiseIntegerOperand` | `BitwiseIntegerOperand` (new) | collisions.md Q18 | `a-s206` |
| `CheckExprOperator134` | `DeleteProperty` | `DeleteProperty` (new) | collisions.md Q6 | `a-s207` |
| `CheckExprOperator642` | `OptionalMethodCall` | `OptionalMethodCall` (new) | C7 | `a-s220` |
| `CheckExprOperator679` | `OptionalFunctionCall` | `OptionalFunctionCall` (new) | C7 | `a-s221` |
| `CheckExprOperator968` | `UndefinedEqualityPair` | `UndefinedEqualityPair` (new) | C7 | `a-s224` |
| `CheckExprOperator986` | `UndefinedEqualityNonMember` | `UndefinedEqualityNonMember` (new) | C7 | `a-s225` |
| `CheckExprOperator1514` | `BareYieldNonVoid` | `BareYieldNonVoid` (new) | compiler.md §151.1 | `a-s233` |
| `CheckExprMember35` | `DescriptorDefaultThisArithmetic` | `ThisInFieldInitializer` (reused) | C9 | `a-s127` |
| `CheckExprMember73` | `FieldInitializerUninitializedRead` | `ThisInFieldInitializer` (reused) | C9 | `a-s128` |
| `CheckExprMember305` | `ReadSetterOnlyAccessor` | `WriteAccessorWithoutRead` (reused) | compiler.md §65.1 | `a-s138` |
| `CheckExprNamespace186` | `ReadStaticSetterOnlyAccessor` | `WriteAccessorWithoutRead` (reused) | compiler.md §65.1 | `a-s180` |
| `CheckExprAssign571` | `WriteSetterOnlyAccessor` | `WriteAccessorWithoutRead` (reused) | compiler.md §65.1 | `a-s029` |
| `CheckExprAssign646` | `WriteStaticSetterOnlyAccessor` | `WriteAccessorWithoutRead` (reused) | compiler.md §65.1 | `a-s032` |
| `CheckExprMember348` | `AsyncMethodValue` | `AsyncMethodValue` (new) | compiler.md §37.1 | `a-s141` |
| `CheckExprMember360` | `GenericAsyncMethodValue` | `GenericAsyncMethodValue` (new) | compiler.md §93.1 | `a-s142` |
| `CheckExprLiteral375` | `AsyncFunctionValue` | `AsyncFunctionValue` (new) | compiler.md §26.1 | `a-s117` |
| `CheckExprMember421` | `FixedArrayObjectMember` | `FixedArrayObjectMember` (new) | collisions.md Q3 | `a-s146` |
| `CheckExprMember453` | `MapObjectMember` | `MapObjectMember` (new) | stdlib.md §10.4 | `a-s148` |
| `CheckExprMember481` | `SetObjectMember` | `SetObjectMember` (new) | stdlib.md §10.4 | `a-s150` |
| `CheckExprMember581` | `GeneratorResultObjectMember` | `GeneratorResultObjectMember` (new) | C8 | `a-s154` |
| `CheckExprMember634` | `NumericObjectMember` | `NumericObjectMember` (new) | stdlib.md §11 | `a-s155` |
| `CheckExprMember643` | `BoundaryObjectMember` | `BoundaryObjectMember` (new) | C7 | `a-s156` |
| `CheckExprCall683` | `WorkerStaticObjectMethod` | `WorkerStaticObjectMethod` (new) | stdlib.md §16.1 | `a-s049` |
| `CheckExprCall883` | `WorkerObjectMethod` | `WorkerObjectMethod` (new) | stdlib.md §16.1 | `a-s053` |
| `CheckExprCall908` | `InboxObjectMethod` | `InboxObjectMethod` (new) | stdlib.md §16.1 | `a-s054` |
| `CheckExprCall930` | `OutboxObjectMethod` | `OutboxObjectMethod` (new) | stdlib.md §16.1 | `a-s055` |
| `CheckExprCall1014` | `FixedArrayObjectMethod` | `FixedArrayObjectMethod` (new) | collisions.md Q3 | `a-s057` |
| `CheckExprMethod36` | `NumericObjectMethod` | `NumericObjectMethod` (new) | stdlib.md §11 | `a-s158` |
| `CheckExprMethod242` | `StringObjectMember` | `StringObjectMember` (new) | stdlib.md §8 | `a-s159` |
| `CheckExprMethod1070` | `MapObjectMethod` | `MapObjectMethod` (new) | stdlib.md §10.4 | `a-s171` |
| `CheckExprMethod1190` | `SetObjectMethod` | `SetObjectMethod` (new) | stdlib.md §10.4 | `a-s173` |
| `CheckExprMethod1570` | `ArrayObjectMember` | `ArrayObjectMember` (new) | stdlib.md §9 | `a-s177` |
| `CheckExprNamespace884` | `RegexCompile` | `RegexCompile` (new) | stdlib.md §15.3 | `a-s201` |
| `CheckExprLiteral185` | `Float16LiteralRange` | `IntegerLiteralRange` (reused) | C4 | `a-s110` |
| `CheckExprLiteral200` | `FractionalIntegerLiteral` | `FractionalIntegerLiteral` (new) | C4 | `a-s111` |
| `CheckExprLiteral340` | `NamespaceContextValue` | `NamespaceAsValue` (reused) | C18 | `a-s116` |
| `CheckExprAggregate34` | `ArrayLiteralHole` | `ArrayHoleConstruction` (reused) | compiler.md §105.3 | `a-s000` |
| `CheckExprAggregate302` | `ArraySpreadLiteralHole` | `ArrayHoleConstruction` (reused) | compiler.md §105.3 | `a-s009` |
| `CheckExprAggregate72` | `FixedArrayLiteralLength` | `FixedArrayLiteralLength` (new) | collisions.md Q3 | `a-s001` |
| `CheckExprCall476` | `ByteArgumentIdentity` | `ByteArgumentIdentity` (new) | stdlib.md §18.1 | `a-s045` |
| `CheckExprCall1519` | `GenericConstructorTypeArguments` | `GenericConstructorTypeArguments` (new) | compiler.md §149.1 | `a-s073` |
| `CheckExprNamespace659` | `WorkerSpawnSpread` | `WorkerSpawnSpread` (new) | compiler.md §40.1 | `a-s191` |
| `CheckExprNamespace691` | `WorkerEntryLocalValue` | `WorkerEntryLocalValue` (new) | compiler.md §40.1 | `a-s193` |
| `CheckExprNamespace705` | `WorkerEntryGeneric` | `WorkerEntryGeneric` (new) | compiler.md §40.1 | `a-s194` |
| `CheckExprNamespace729` | `WorkerEntryShape` | `WorkerEntrySignature` (new) | compiler.md §40.1 | `a-s196` |
| `CheckExprNamespace742` | `WorkerEntryStructuralEndpoints` | `WorkerEntryStructuralEndpoints` (new) | compiler.md §40.1 | `a-s197` |
| `CheckExprNamespace768` | `WorkerExplicitMessageIdentity` | `WorkerExplicitMessageIdentity` (new) | C1 | `a-s199` |
| `CheckExprEntry461` | `AwaitNonHandle` | `AwaitNonHandle` (new) | compiler.md §26.1 | `a-s090` |
| `CheckExprEntry521` | `AwaitLocalCall` | `AwaitLocalCall` (new) | compiler.md §26.1 | `a-s093` |
| `CheckExprEntry559` | `AwaitUndeclaredAsyncFunction` | `AwaitUndeclaredAsyncFunction` (new) | compiler.md §26.1 | `a-s094` |
| `CheckExprEntry571` | `AwaitSynchronousFunction` | `AwaitSynchronousFunction` (new) | compiler.md §26.1 | `a-s095` |
| `CheckExprEntry604` | `AwaitComputedMethod` | `AwaitComputedMethod` (new) | compiler.md §37.1 | `a-s097` |
| `CheckExprEntry620` | `AwaitNonClassMethod` | `AwaitNonClassMethod` (new) | compiler.md §37.1 | `a-s098` |
| `CheckExprEntry653` | `AwaitSynchronousMethod` | `AwaitSynchronousMethod` (new) | compiler.md §37.1 | `a-s100` |
| `CheckExprEntry682` | `AwaitIndirectCall` | `AwaitIndirectCall` (new) | compiler.md §26.1 | `a-s102` |
| `CheckExprMethod505` | `ArrayUnshiftEmpty` | `ArrayUnshiftEmpty` (new) | stdlib.md §9 | `a-s161` |
| `CheckExprMethod681` | `ArrayCallbackThisArgument` | `ArrayCallbackThisArgument` (new) | stdlib.md §9 | `a-s165` |
| `CheckExprMethod1153` | `MapCallbackThisArgument` | `MapCallbackThisArgument` (new) | stdlib.md §10.4 | `a-s172` |
| `CheckExprMethod1228` | `SetCallbackThisArgument` | `SetCallbackThisArgument` (new) | stdlib.md §10.4 | `a-s174` |
| `CheckExprMethod825` | `MapGroupByArraySource` | `MapGroupByArraySource` (new) | stdlib.md §10.4 | `a-s168` |
| `CheckExprMethod843` | `MapGroupByVoidKey` | `MapGroupByVoidKey` (new) | C21 | `a-s169` |
| `CheckExprMember339` | `ClassObjectMemberWrite` | `ClassInheritedObjectMember` (new) | compiler.md §6 | `a-s140` |
| `CheckExprMember372` | `ClassObjectMemberRead` | `ClassInheritedObjectMember` (reused) | compiler.md §6 | `a-s143` |
| `CheckExprCall1119` | `ClassObjectMethodCall` | `ClassInheritedObjectMember` (reused) | compiler.md §6 | `a-s061` |
| `CheckExprEntry645` | `AwaitClassObjectMethod` | `ClassInheritedObjectMember` (reused) | compiler.md §6 | `a-s099` |
| `CheckExprLiteral405` | `ClassRuntimeValue` | `ClassRuntimeObject` (new) | compiler.md §71.1 | `a-s119` |
| `CheckExprNamespace212` | `ClassRuntimeMember` | `ClassRuntimeObject` (reused) | compiler.md §71.1 | `a-s182` |
| `CheckExprNamespace228` | `GenericClassRuntimeMember` | `ClassRuntimeObject` (reused) | compiler.md §71.1 | `a-s184` |
| `CheckExprArrayOfAndMapCopy107` | `MapCopyNullableSource` | `MapCopyNullableSource` (new) | stdlib.md §10.9 | `a-s012` |
| `CheckExprOperator1480` | `AsyncGeneratorYield` | `AsyncGeneratorFunction` (reused) | compiler.md §26.1 | `b-signatures-409` |

`CheckExprAggregate242` becomes `DescriptorRequiredMemberQuotedKey` for the ignored quoted-key witness, and `DescriptorRequiredMemberMissing` for a genuinely absent key. This is an undecided accepted form plus an ordinary TypeScript rejection, not a new divergence.

### Undecided sites: acceptance 3

These 57 sites have accepted TypeScript witnesses but no source section deciding the entire rejected form. They remain Red. Every listed witness and exact target message is stored independently in `rejection_programs_s154.txt` and `rejection_targets_s154.txt`; other witnesses for each guard remain there too.

| Site | Accepted witness | Target message | Unresolved restriction |
| --- | --- | --- | --- |
| `SynchronousMethodValue` | `round4-SynchronousMethodValue` | method `f` may only be called, not read as a value | No section decides a blanket ban on synchronous instance method values. |
| `GenericSynchronousMethodValue` | `round4-GenericSynchronousMethodValue` | method `f` may only be called, not read as a value | No section decides a blanket ban on generic synchronous method values. |
| `GeneratorFunctionValue` | `round4-GeneratorFunctionValue` | generators may only be called, not passed as values | No section decides a blanket ban on first-class synchronous generator function values. |
| `UnknownNamespaceConstructor` | `a-s074` | unknown class `Object` | No section decides every ambient constructor name handled by this catch-all, including Object. |
| `CheckExprOperator82` | `a-s205` | `!` requires a boolean operand, got `i32` | No section decides boolean-only truth testing for numeric values. |
| `CheckExprOperator142` | `a-s208` | unary operator outside the decided surface | No section decides the entire unary catch-all, including typeof. |
| `CheckExprOperator456` | `a-s215` | logical operators require booleans, got `i32` | No section decides boolean-only logical operands. |
| `CheckExprOperator495` | `a-s216` | operator outside the decided surface | No section decides the entire binary catch-all, including in and exponentiation. |
| `CheckExprOperator1236` | `a-s226` | operator `+=` is not defined for `E` and `i32` | No section decides every enum compound arithmetic form. |
| `CheckExprOperator1296` | `a-s228` | operator not defined for `E` and `E` | No section decides enum arithmetic or generic numeric promotion in this catch-all. |
| `CheckExprOperator1382` | `a-s229` | condition must be boolean, got `i32` | No section decides boolean-only conditional tests. |
| `CheckExprOperator1488` | `a-s232` | `yield*` delegation is not in the decided surface | No section decides a blanket ban on yield delegation. |
| `CheckExprOperator1576` | `a-s234` | `as` converts between sized numerics, enum to integer, or narrows `object \| null` to a class; cannot convert `C` to `C` | No section decides rejection of a class identity assertion. |
| `CheckExprMember135` | `a-s131` | private names are not in the decided surface | No section decides a blanket ban on ECMAScript private names. |
| `CheckExprMember190` | `a-s132` | array indices are `i32`, got `i64` | No section decides i32-only indexing instead of other sized integers. |
| `CheckExprMember201` | `a-s133` | array indices are `i32`, got `i64` | No section decides i32-only FixedArray indexing. |
| `CheckExprMember209` | `a-s134` | index 2 out of bounds for FixedArray length 1 | No section decides compile-time rejection instead of runtime bounds handling for this literal index. |
| `CheckExprMember221` | `a-s135` | type `string` is not indexable | No section decides every non-array index receiver, including string. |
| `CheckExprMember397` | `a-s144` | method `push` may only be called, not read as a value | The Array API listing does not decide a blanket ban on reading a method value. |
| `CheckExprMember408` | `a-s145` | method `map` may only be called, not read as a value | The Array callback API listing does not decide reading a method value. |
| `CheckExprMember445` | `a-s147` | method `get` may only be called, not read as a value | The Map API listing does not decide reading a method value. |
| `CheckExprMember473` | `a-s149` | method `add` may only be called, not read as a value | The Set API listing does not decide reading a method value. |
| `CheckExprMember501` | `a-s151` | method `slice` may only be called, not read as a value | The string API listing does not decide reading a method value. |
| `CheckExprMember550` | `a-s152` | method `test` may only be called, not read as a value | The RegExp API listing does not decide reading a method value. |
| `CheckExprMember556` | `a-s153` | coroutine step results are read-only | No section decides readonly step-result fields for every extended result shape. |
| `CheckExprMember652` | `a-s157` | `boolean` has no member `valueOf` | No section decides the missing boolean valueOf member. |
| `CheckExprLambda193` | `a-s105` | a lambda with a block body requires a return type annotation | No section decides a mandatory return annotation on every block lambda. |
| `CheckExprLambda211` | `a-s106` | not all paths return a value | No section decides this enum-exhaustive return completeness restriction. |
| `CheckExprLiteral130` | `a-s109` | literal form outside the decided surface | No section decides every literal catch-all form, including bigint. |
| `CheckExprLiteral270` | `a-s113` | type `C` cannot be interpolated into a template | No section decides every unsupported template interpolation receiver, including a class. |
| `CheckExprLiteral394` | `c-apply-id` | generic function `id` requires explicit type arguments | No section decides omission of explicit generic function arguments in every value context. |
| `CheckExprLiteral413` | `a-s120` | enum `E` used as a value; use a member | No section decides a blanket ban on enum objects as values. |
| `CheckExprLiteral437` | `a-s123` | foreign function `host` may only be called | No section decides a blanket ban on foreign function values. |
| `CheckExprLiteral531` | `a-s125` | ambient function `print` may only be called | No section decides a blanket ban on ambient function values. |
| `CheckExprLiteral554` | `a-s126` | unknown name `missing` | The same guard includes a switch-scoped var that the checker did not bind; no section decides that ignored declaration. |
| `CheckExprAggregate102` | `a-s002` | cannot infer the type of an empty array literal without context | No section decides the absence of empty-array inference in every uncontextualized position. |
| `CheckExprAggregate155` | `a-s003` | spread properties are not supported in descriptor literals | No section decides a blanket ban on descriptor literal spread properties. |
| `CheckExprAggregate165` | `a-s004` | descriptor literal member names must be identifiers | No section decides identifier-only descriptor literal source keys. |
| `CheckExprAggregate184` | `a-s005` | descriptor literals contain data properties only | No section decides data-properties-only descriptor literals in every source form. |
| `DescriptorRequiredMemberQuotedKey` | `a-s004` | descriptor literal for `D` is missing required member `x` | No section decides ignoring a quoted descriptor literal key that names the required member. |
| `CheckExprCall174` | `a-s041` | unknown function `missing` | The catch-all also rejects the TypeScript ambient String function; no section decides this whole name surface. |
| `CheckExprCall220` | `a-s042` | generator `gen` is called before its yield type is known; declare it earlier in the program | No section decides this ordering restriction for an inferred generator yield type. |
| `CheckExprCall1067` | `a-s059` | `return` is outside the coroutine surface (next) | No section explicitly decides a blanket ban on generator return calls. |
| `CheckExprCall1130` | `a-s062` | `boolean` has no method `valueOf` | No section decides the missing boolean valueOf method. |
| `CheckExprCall1157` | `a-s063` | `push` expects 1 argument(s) (1 required), got 2 | This shared argument-count builder lacks a builtin origin fact; its Array.push witness alone cannot classify all callers. |
| `CheckExprCall1300` | `a-s064` | `new` requires a class name | No section decides every construction target rejected by the class-name catch-all. |
| `CheckExprCall1385` | `a-s069` | `new Map` requires explicit type arguments (Q24) | Q24 does not decide all omitted builtin Map arguments and defaults covered by this guard. |
| `CheckExprNamespace205` | `a-s181` | static method `C.f` may only be called | No section decides a blanket ban on synchronous static method values. |
| `CheckExprNamespace249` | `a-s185` | enum `E` has no member `toString` | No section decides every rejected enum object reflection member, including toString. |
| `CheckExprEntry243` | `a-s082` | `this` is only available in constructors and methods | Section 147 governs field initializers, not the general lexical this capture rejected by this guard. |
| `CheckExprEntry296` | `a-s085` | the `!` assertion is not in the decided surface; narrow with a null check | No section decides a blanket ban on non-null assertions. |
| `CheckExprEntry304` | `a-s104` | function expressions are not in the decided surface; use an arrow | No section decides a blanket ban on function expressions, including generator expressions. |
| `CheckExprEntry313` | `a-s087` | expression form outside the decided surface | No section decides every expression catch-all form, including tagged templates. |
| `CheckExprAssign41` | `a-s014` | assignment operator outside the decided surface | No section decides all assignment operators rejected by this catch-all, including logical assignment. |
| `CheckExprAssign500` | `a-s028` | assignment target outside the decided surface | No section decides every rejected assignment target, including an asserted identifier. |
| `CheckExprAssign598` | `a-s030` | private names are not in the decided surface | No section decides a blanket ban on ECMAScript private names. |
| `CheckExprMethod1544` | `a-s170` | type mismatch: the `Map.groupBy` callback expects `(i32)` => …, got `i32` | C3 decides scalar conversion; it does not decide callback parameter variance or every callback shape in this shared guard. |

### Fragment additions and repairs

The 55 new variant records live in `compiler/src/divergence/group_a.rs`, referenced by the single exhaustive dispatch in `entries.rs`. Each fragment is accepted by stock tsc and produces the same carried variant here. Each subscript record states `no equivalent` and an actionable alternative. All 299 carried variant fragments pass rule 7.

The shared new ClassInheritedObjectMember fragment reaches instance read, write, call, and awaited-call guards. ClassRuntimeObject reaches a bare class value and ordinary/generic class static reflection guards. A temporary measurement checked all 8 shared fragments against their 18 touched shared guards; all passed. The measurement helper was removed afterward.

Five existing fragments expand so that each newly sharing guard is shown. Their original rejection forms remain covered:

#### WriteAccessorWithoutRead

Old ts:

```ts
class C { set x(v: i32) {} }
export function main(): void { new C().x = 1; }
```

New ts:

```ts
class C { set x(v: i32) {} static set y(v: i32) {} }
export function main(): void { const c = new C(); c.x; c.x = 1; C.y; C.y = 1; }
```

#### ThisInFieldInitializer

Old ts:

```ts
class C { value: i32 = this.read(); read(): i32 { return 3; } }
```

New ts:

```ts
class C { a!: i32; b: i32 = this.a; value: i32 = this.read(); constructor() { this.a = 1; } read(): i32 { return 3; } }
@Descriptor class D { a?: string = "x"; b?: string = this.a + "x"; }
```

#### IntegerLiteralRange

Old ts:

```ts
const big: i32 = 3000000000;
```

New ts:

```ts
const big: i32 = 3000000000;
const half: f16 = 70000;
```

#### NamespaceAsValue

Old ts:

```ts
// file: main.ts
import * as ns from "./namespace-local-function-lib"; export function main():void { function ns():void {} ns(); }
// file: namespace-local-function-lib.ts
export function f():void {}
```

New ts:

```ts
// file: main.ts
import * as ns from "./namespace-local-function-lib"; export function main(): void { const typed: object = ns; } export function probe(): void { function ns(): void {} ns(); }
// file: namespace-local-function-lib.ts
export function f(): void {}
```

#### ArrayHoleConstruction

Old ts:

```ts
const xs: i32[] = new Array<i32>(3);
```

New ts:

```ts
export function main(): void { const xs: i32[] = new Array<i32>(3); const hole = [,1]; const spread = [,...[1]]; }
```

AsyncGeneratorFunction already reaches both the declaration and the async-generator yield guards; its fragment is unchanged. Existing why records for ThisInFieldInitializer, WriteAccessorWithoutRead, IntegerLiteralRange, NamespaceAsValue, and ArrayHoleConstruction describe the rejection reasons at their newly sharing sites.

### Corpus changes

No round 4 corpus file changes or retirements are needed. Common TypeScript-rejected forms stay on distinct TscRejects guards. The rejected descriptor numeric arithmetic, declared nonvoid bare yield, missing required descriptor member, ordinary class unknown member/method, incompatible namespace context, and invalid nullable Map source stay rejected without an invented divergence. The prior rounds’ corpus rewrites and retirements remain. The latest corpus rejection test passes all 45 tests, including the all-entry header and first-diagnostic checks.

### Messages

Old: §147 rule 3a: `this` is forbidden in a descriptor member default; arithmetic on an optional member also fails with TS2532

New: §147 rule 3a: `this` is forbidden in a descriptor member default

The old TypeScript-rejection claim is removed because the string-field witness is accepted. Numeric optional-member arithmetic stays on its separate TscRejects guard. The touched Diverges messages contain no assertion that TypeScript rejects the accepted form.

### Test and representation changes

Site enum, map, witness inventory, independent target rows, and source programs are updated together. Existing accepted witnesses are retained; common rejected controls target the new guards. The rule 4 batch contains 1,052 witnesses and 299 distinct carried variants. The Worker.spawn matrix record uses compiler.md §40.1 instead of §40, matching the precise WorkerEntrySignature decision; product dimensions, source restriction, expected codes, and matrix counts remain unchanged.

Rust files remain below 2,000 lines. New fragment constants are split into group_a.rs to keep entries.rs below the cap. New expression helpers precede the test module, avoiding an items-after-test-module warning. Test-only helpers remain scoped to tests.

### Verification and cost

- `cargo test --offline --locked -p subscript-compiler`: 386 library tests pass; only `every_subset_rejection_carries_its_divergence` fails, with exactly 125 Red targets. No missing target, coverage, class-label, or fragment failure remains.
- The same command with `-- --skip every_subset_rejection_carries_its_divergence`: all remaining unit, integration, and doc tests pass (923 passed, 1 ignored).
- `cargo test --offline --locked -p subscript-compiler --test corpus_reject`: 45 passed.
- `cargo fmt --check`: passed.
- `cargo clippy --offline --locked -p subscript-compiler --all-targets`: passed with the existing 2 library and 13 test-only warnings; no new warning.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference`: passed; generated files refreshed only through this command.
- `tools/gate.sh` was not run.

The final post-cleanup rule 4 run measured 1.043 seconds total: tsc 0.521 seconds, checker 0.298 seconds, with the same 386 passing tests and 125 Red failures. The source cost comment records 1.04 / 0.52 / 0.30 seconds. The test cost does not include the separately run integration matrix.

### All changed files

The following list reports the complete working-tree change set relative to HEAD, including the preserved rounds 1–3 base. Round 4 changes expression guards, diagnostic inventories/fragments, the two carried source facts, the Worker matrix restriction record, generated language documentation, and this appended note. Other listed changes are inherited.
- `compiler/src/ambient.rs`
- `compiler/src/api_reference.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/container_argument.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/exports.rs`
- `compiler/src/check/expr.rs`
- `compiler/src/check/expr/aggregate.rs`
- `compiler/src/check/expr/array_of_and_map_copy.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/expr/namespace.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/field_initializer.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/host_entries.rs`
- `compiler/src/check/inference.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/instance_chain.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/layout.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mirror_provenance.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/namespace_import.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/opaque.rs`
- `compiler/src/check/pattern.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/divergence.rs`
- `compiler/src/hir/host_entry.rs`
- `compiler/src/language_reference.rs`
- `compiler/src/lib.rs`
- `compiler/src/parse.rs`
- `compiler/src/provenance.rs`
- `compiler/src/regex.rs`
- `compiler/src/tests/collections.rs`
- `compiler/tests/async_generic_method.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/field_values.rs`
- `compiler/tests/generic_method.rs`
- `compiler/tests/generic_tsc_matrix.rs`
- `compiler/tests/generic_tsc_matrix/api.rs`
- `compiler/tests/generic_tsc_matrix/kinds.rs`
- `compiler/tests/re_exports.rs`
- `compiler/tests/tsc_corpus.rs`
- `corpus/reject/r133-using-without-dispose.ts`
- `corpus/reject/r136-valuetype-align-not-in-set.ts`
- `corpus/reject/r137-descriptor-align.ts`
- `corpus/reject/r146-accessor-field-name-clash.ts`
- `corpus/reject/r148-switch-cross-case-read.ts`
- `corpus/reject/r154-namespace-read-before-declaration.ts`
- `corpus/reject/r155-class-read-before-declaration.ts`
- `corpus/reject/r161-field-method-member-name-clash.ts` (deleted)
- `corpus/reject/r162-duplicate-method-member-name.ts`
- `corpus/reject/r163-duplicate-field-member-name.ts` (deleted)
- `corpus/reject/r164-duplicate-static-member-name.ts` (deleted)
- `corpus/reject/r192-using-nullable-without-dispose.ts`
- `corpus/reject/r224-field-without-initializer.ts`
- `corpus/reject/r225-reference-field-without-initializer.ts`
- `corpus/reject/r228-field-assigned-after-early-return.ts`
- `corpus/reject/r230-field-read-before-its-assignment.ts`
- `corpus/reject/r292-generic-uninitialized-field.ts`
- `corpus/reject/r62-valuetype-fixed-array-layout-too-large.ts`
- `corpus/reject/r94-descriptor-method.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `compiler/src/check/rejection_programs.rs`
- `compiler/src/check/rejection_programs_s154.txt`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets_s154.txt`
- `compiler/src/check/rejection_tsc.cjs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/divergence/group_a.rs`
- `specs/tracking/s154-rejection-sites.md`

## Round 5

The rounds 1–4 working tree remains the base. No commit is made. The 125 prior Red sites are closed or split. One new split site remains Red: `ConstAssertionExpression`. The phase is not complete. Acceptance 3 requires this open list because no allowed contract decides `as const`.

### Open list: acceptance 3

| Site | Witness | Measured result | Missing decision |
|---|---|---|---|
| `ConstAssertionExpression` | `round5-ConstAssertionExpression`: `function f(): void { 1 as const; }` | Stock TypeScript accepts; checker emits S100, `expression form outside the decided surface`, with no variant. | C24 row 18 does not list const assertions. No other section decides this restriction. |

The site keeps `// §154 Red` and a provisional `TscRejects` class. The rule 4 test reports it as an accepted witness at a TscRejects site. This failure is required evidence, not a passing result. The unsupported C24 variant was removed. Specs outside this note are not changed.

### Corpus-index contract blocker

The unchanged C24 heading has no Accept, Reject, or Trap paragraph. The same text is present at HEAD. `collision_table_is_a_consistent_corpus_index` therefore reports `C24 pins no corpus entry through its Accept:/Reject:/Trap: paragraph`. The handoff requires reporting new C24 entry ids and prohibits editing collisions.md. r344 is the new reject pin to report. The test is preserved; no exception hides this failure.

### Prior 125 sites

Each row states the current class and variant, or the replacing split. The named witness is measured by the single stock TypeScript process. The static target table pins the emitted code and message. Programs rejected earlier do not count as reaching a later site.

| Prior site | Current code / class | Variant | Witness or replacement |
|---|---|---|---|
| `NullableCall` | S100 / Diverges | `NullableCallForm` | `round5-nullable-call-initialized` (accepts) |
| `NullableMember` | S011 / Diverges | `NullableMemberForm` | `c-nullable-initialized` (accepts) |
| `CheckBindings97` | S100 / TscRejects | `—` | `c-pattern-iterator-bad` (TS2339) |
| `CheckLookup19` | S100 / TscRejects | `—` | `c-type-only-value` (TS1361) |
| `CheckGenerics283` | S100 / TscRejects | `—` | `c-generic-function-arity` (TS2558) |
| `CheckGenerics375` | S100 / TscRejects | `—` | `c-generic-method-arity` (TS2558) |
| `CheckGenerics492` | S100 / TscRejects | `—` | `c-generic-class-arity` (TS2314) |
| `CheckText49` | S100 / TscRejects | `—` | `c-uri-generic` (TS2558) |
| `CheckTyres97` | split | — | `TupleAnnotation`, `ThisAnnotation`, `QueryAnnotation`, `StructuralAnnotation`, `OptionalAnnotation`, `RestAnnotation`, `ConditionalAnnotation`, `InferAnnotation`, `OperatorAnnotation`, `IndexedAnnotation`, `MappedAnnotation`, `PredicateAnnotation`, `ImportAnnotation`, `StringLiteralAnnotation`, `NumberLiteralAnnotation`, `BooleanLiteralAnnotation`, `BigIntLiteralAnnotation`, `TemplateLiteralAnnotation`, `UnsupportedAnnotationKind` |
| `CheckTyres180` | split | — | `NeverAnnotation`, `UnknownAnnotation`, `SymbolAnnotation`, `BigIntAnnotation`, `UnsupportedKeywordKind` |
| `CheckTyres193` | S100 / Diverges | `Tyres193Form` | `c-qualified` (accepts) |
| `CheckTyres214` | S100 / TscRejects | `—` | `c-worker-missing` (TS2314) |
| `CheckTyres224` | S100 / TscRejects | `—` | `b-class_shape-529` (TS2314) |
| `CheckTyres267` | S100 / TscRejects | `—` | `c-regexp-generic` (TS2315) |
| `CheckTyres283` | S100 / TscRejects | `—` | `c-promise-missing` (TS2314) |
| `CheckTyres291` | S100 / TscRejects | `—` | `c-promise-arity` (TS2314) |
| `CheckTyres303` | S100 / TscRejects | `—` | `c-fixed-missing` (TS2314) |
| `CheckTyres311` | S100 / TscRejects | `—` | `c-fixed-arity` (TS2314) |
| `CheckTyres390` | S100 / Diverges | `ArrayTypeArgument` | `c-array-two-alias` (accepts) |
| `CheckTyres403` | S100 / Diverges | `Tyres403Form` | `c-generator-no-argument` (accepts) |
| `CheckTyres419` | S100 / TscRejects | `—` | `c-error-generic` (TS2315) |
| `CheckTyres430` | S100 / TscRejects | `—` | `c-map-missing` (TS2314) |
| `CheckTyres439` | S100 / TscRejects | `—` | `c-map-arity` (TS2314) |
| `CheckTyres495` | S100 / TscRejects | `—` | `c-date-generic` (TS2315) |
| `CheckTyres512` | S100 / TscRejects | `—` | `c-class-generic` (TS2315) |
| `CheckTyres518` | S100 / TscRejects | `—` | `c-class-missing` (TS2314) |
| `CheckTyres551` | S100 / TscRejects | `—` | `c-alias-generic` (TS2315) |
| `CheckTyres562` | S016 / Diverges | `LibTypeName` | `a-s087` (accepts) |
| `CheckTyres581` | S100 / Diverges | `Tyres581Form` | `c-intersection` (accepts) |
| `CheckTyres673` | S100 / Diverges | `Tyres673Form` | `c-constructor-type` (accepts) |
| `CheckTyres698` | split | — | `FunctionTypeRestParameter`, `FunctionTypeArrayPattern`, `FunctionTypeObjectPattern` |
| `CheckTypeRules299` | S005 / Diverges | `TypeRules299Form` | `c-assign-map-width` (accepts) |
| `CheckTypeRules329` | split | — | `LiteralAliasToString`, `EnumToInteger`, `ArrayToFixedArray`, `FunctionParameterIdentity`, `AssignmentTypeMismatch` |
| `CheckBodies123` | S100 / Diverges | `Bodies123Form` | `b-bodies-123` (accepts) |
| `CheckBodies158` | S100 / Diverges | `Bodies158Form` | `b-bodies-158` (accepts) |
| `CheckBodies261` | S100 / Diverges | `ReturnFlowCoverage` | `b-bodies-261-enum` (accepts) |
| `CheckStmt210` | split | — | `LocalClassDeclaration`, `LocalFunctionDeclaration`, `LocalEnumDeclaration`, `LocalAliasDeclaration`, `LocalInterfaceDeclaration`, `LocalNamespaceDeclaration`, `LocalRejectedDeclaration` |
| `CheckStmt295` | split | — | `DoWhileStatement`, `ForInStatement`, `LabeledStatement`, `DebuggerStatement`, `WithStatement`, `UnsupportedStatementKind` |
| `CheckStmt311` | S100 / Diverges | `Stmt311Form` | `b-stmt-311` (accepts) |
| `CheckStmt359` | S100 / Diverges | `Stmt359Form` | `b-stmt-359` (accepts) |
| `CheckStmt475` | S100 / Diverges | `GeneratorReturnValue` | `b-stmt-475` (accepts) |
| `CheckStmt535` | S100 / Diverges | `Stmt535Form` | `a-s236` (accepts) |
| `CheckStmt788` | S013 / Diverges | `Stmt788Form` | `b-stmt-788` (accepts) |
| `CheckStmt994` | S100 / Diverges | `Stmt994Form` | `b-stmt-994` (accepts) |
| `CheckStmt1019` | S100 / Diverges | `Stmt1019Form` | `b-stmt-1019` (accepts) |
| `CheckStmt1101` | S100 / Diverges | `ForOfSpreadCall` | `b-stmt-1101-accept` (accepts) |
| `CheckStmt1298` | S100 / Diverges | `Stmt1298Form` | `b-stmt-1298` (accepts) |
| `CheckClassShape354` | S100 / Diverges | `ClassShape354Form` | `b-class_shape-354` (accepts) |
| `CheckClassShape475` | S100 / Diverges | `IdentifierFieldName` | `b-class_shape-475` (accepts) |
| `CheckClassShape520` | S100 / Diverges | `ClassShape520Form` | `b-class_shape-520` (accepts) |
| `CheckClassShape612` | S100 / Diverges | `ClassShape612Form` | `b-class_shape-612` (accepts) |
| `CheckClassShape732` | S100 / Diverges | `ClassShape732Form` | `b-class_shape-732` (accepts) |
| `CheckClassShape841` | split | — | `PrivateFieldDeclaration`, `PrivateMethodDeclaration`, `StaticBlockDeclaration`, `AutoAccessorDeclaration`, `UnsupportedClassMemberKind` |
| `CheckExports101` | S100 / TscRejects | `—` | `b-exports-101` (TS2307) |
| `CheckExports370` | S016 / TscRejects | `—` | `b-exports-370` (TS2305) |
| `CheckDeclarations131` | split | — | `SourceInterfaceDeclaration`, `SourceNamespaceDeclaration` |
| `CheckDeclarations461` | S100 / Diverges | `Declarations461Form` | `b-declarations-461` (accepts) |
| `CheckDeclarations518` | S100 / Diverges | `Declarations518Form` | `b-declarations-518` (accepts) |
| `CheckDeclarations530` | S100 / Diverges | `Declarations530Form` | `b-declarations-530` (accepts) |
| `CheckDeclarations550` | S100 / Diverges | `Declarations550Form` | `b-declarations-550` (accepts) |
| `CheckDeclarations749` | S100 / Diverges | `Declarations749Form` | `b-declarations-749` (accepts) |
| `CheckSignatures213` | S100 / Diverges | `Signatures213Form` | `b-signatures-213` (accepts) |
| `CheckSignatures263` | S100 / Diverges | `Signatures263Form` | `round5-missing-side-effect` (accepts) |
| `CheckSignatures317` | S016 / TscRejects | `—` | `b-signatures-317` (TS2305) |
| `CheckSignatures373` | S100 / Diverges | `Signatures373Form` | `a-s089` (accepts) |
| `CheckSignatures478` | S100 / Diverges | `Signatures478Form` | `b-signatures-478` (accepts) |
| `CheckSignatures519` | S100 / Diverges | `Signatures519Form` | `b-signatures-519` (accepts) |
| `CheckSignatures548` | S100 / Diverges | `Signatures548Form` | `b-signatures-548` (accepts) |
| `SynchronousMethodValue` | S100 / Diverges | `SynchronousMethodValueForm` | `round4-SynchronousMethodValue` (accepts) |
| `GenericSynchronousMethodValue` | S100 / Diverges | `GenericSynchronousMethodValueForm` | `round4-GenericSynchronousMethodValue` (accepts) |
| `GeneratorFunctionValue` | S100 / Diverges | `GeneratorFunctionValueForm` | `round4-GeneratorFunctionValue` (accepts) |
| `UnknownNamespaceConstructor` | S100 / Diverges | `LibConstructorName` | `a-s074` (accepts) |
| `CheckExprOperator82` | S100 / Diverges | `ExprOperator82Form` | `a-s205` (accepts) |
| `CheckExprOperator142` | split | — | `TypeofOperator`, `VoidOperator`, `UnaryPlusOperator` |
| `CheckExprOperator456` | S100 / Diverges | `ExprOperator456Form` | `a-s215` (accepts) |
| `CheckExprOperator495` | split | — | `InOperator`, `ExponentOperator` |
| `CheckExprOperator1236` | split | — | `CompoundEnumOperand`, `CompoundStringOperand`, `CompoundInvalidOperand` |
| `CheckExprOperator1296` | split | — | `BinaryEnumOperand`, `StringRelationalOperand`, `BinaryStringOperand`, `BooleanRelationalOperand`, `BinaryInvalidOperand` |
| `CheckExprOperator1382` | S100 / Diverges | `ExprOperator1382Form` | `a-s229` (accepts) |
| `CheckExprOperator1488` | S100 / Diverges | `ExprOperator1488Form` | `a-s232` (accepts) |
| `CheckExprOperator1576` | split | — | `IdentityAssertion`, `IntegerEnumAssertion`, `NullableClassAssertion`, `StringAliasAssertion`, `InvalidAssertion` |
| `CheckExprMember135` | S100 / Diverges | `ExprMember135Form` | `a-s131` (accepts) |
| `CheckExprMember190` | S100 / Diverges | `ExprMember190Form` | `a-s132` (accepts) |
| `CheckExprMember201` | S100 / Diverges | `ExprMember201Form` | `a-s133` (accepts) |
| `CheckExprMember209` | S100 / Diverges | `ExprMember209Form` | `a-s134` (accepts) |
| `CheckExprMember221` | S100 / Diverges | `ExprMember221Form` | `a-s135` (accepts) |
| `CheckExprMember397` | S100 / Diverges | `ExprMember397Form` | `a-s144` (accepts) |
| `CheckExprMember408` | S100 / Diverges | `ExprMember408Form` | `a-s145` (accepts) |
| `CheckExprMember445` | S100 / Diverges | `ExprMember445Form` | `a-s147` (accepts) |
| `CheckExprMember473` | S100 / Diverges | `ExprMember473Form` | `a-s149` (accepts) |
| `CheckExprMember501` | S100 / Diverges | `ExprMember501Form` | `a-s151` (accepts) |
| `CheckExprMember550` | S100 / Diverges | `ExprMember550Form` | `a-s152` (accepts) |
| `CheckExprMember556` | S100 / Diverges | `ExprMember556Form` | `a-s153` (accepts) |
| `CheckExprMember652` | split | — | `BooleanMember`, `FunctionMember`, `GeneratorMember`, `EnumMember`, `LiteralAliasMember`, `InvalidReceiverMember` |
| `CheckExprLambda193` | S100 / Diverges | `ExprLambda193Form` | `a-s105` (accepts) |
| `CheckExprLambda211` | S100 / Diverges | `LambdaReturnFlowCoverage` | `a-s106` (accepts) |
| `CheckExprLiteral130` | S100 / Diverges | `BigIntLiteral` | `a-s109` (accepts) |
| `CheckExprLiteral270` | S100 / Diverges | `ExprLiteral270Form` | `a-s113` (accepts) |
| `CheckExprLiteral394` | S100 / Diverges | `GenericFunctionValue` | `c-apply-id` (accepts) |
| `CheckExprLiteral413` | S100 / Diverges | `ExprLiteral413Form` | `a-s120` (accepts) |
| `CheckExprLiteral437` | S100 / Diverges | `ExprLiteral437Form` | `a-s123` (accepts) |
| `CheckExprLiteral531` | S100 / Diverges | `ExprLiteral531Form` | `a-s125` (accepts) |
| `CheckExprLiteral554` | S016 / Diverges | `LibGlobalValue` | `round5-lib-global-value` (accepts) |
| `CheckExprAggregate102` | S100 / Diverges | `ExprAggregate102Form` | `a-s002` (accepts) |
| `CheckExprAggregate155` | S100 / Diverges | `ExprAggregate155Form` | `a-s003` (accepts) |
| `CheckExprAggregate165` | S100 / Diverges | `ExprAggregate165Form` | `a-s004` (accepts) |
| `CheckExprAggregate184` | S100 / Diverges | `ExprAggregate184Form` | `a-s005` (accepts) |
| `DescriptorRequiredMemberQuotedKey` | S100 / TscRejects | `—` | unreachable — A rejected property increments the diagnostic count; descriptor_literal returns Error before required-field checks. |
| `CheckExprCall174` | S016 / Diverges | `LibGlobalCall` | `b-probe-conversions` (accepts) |
| `CheckExprCall220` | S100 / Diverges | `ExprCall220Form` | `a-s042` (accepts) |
| `CheckExprCall1067` | S100 / Diverges | `ExprCall1067Form` | `a-s059` (accepts) |
| `CheckExprCall1130` | split | — | `BooleanMethod`, `FunctionMethod`, `GeneratorMethod`, `EnumMethod`, `LiteralAliasMethod`, `InvalidReceiverMethod` |
| `CheckExprCall1157` | split | — | `ArgumentCountExprCall1605`, `ArgumentCountExprCall1529`, `ArgumentCountExprCall1129`, `ArgumentCountExprCall1113`, `ArgumentCountExprCall1066`, `ArgumentCountExprCall983`, `ArgumentCountExprCall979`, `ArgumentCountExprCall966`, `ArgumentCountExprCall936`, `ArgumentCountExprCall907`, `ArgumentCountExprCall846`, `ArgumentCountExprCall539`, `ArgumentCountExprCall282`, `ArgumentCountExprCall255`, `ArgumentCountExprCall229`, `ArgumentCountExprCall205`, `ArgumentCountExprEntry652`, `ArgumentCountExprEntry566`, `ArgumentCountExprMethod1410`, `ArgumentCountExprMethod1299`, `ArgumentCountExprMethod1244`, `ArgumentCountExprMethod1235`, `ArgumentCountExprMethod1161`, `ArgumentCountExprMethod1147`, `ArgumentCountExprMethod1139`, `ArgumentCountExprMethod1134`, `ArgumentCountExprMethod1120`, `ArgumentCountExprMethod547`, `ArgumentCountExprMethod521`, `ArgumentCountExprMethod499`, `ArgumentCountExprMethod491`, `ArgumentCountExprMethod458`, `ArgumentCountExprMethod450`, `ArgumentCountExprMethod438`, `ArgumentCountExprMethod413`, `ArgumentCountExprMethod388`, `ArgumentCountExprMethod347`, `ArgumentCountExprMethod328`, `ArgumentCountExprMethod183`, `ArgumentCountExprMethod113`, `ArgumentCountExprNamespace1233`, `ArgumentCountExprNamespace1225`, `ArgumentCountExprNamespace1134`, `ArgumentCountExprNamespace1113`, `ArgumentCountExprNamespace878`, `ArgumentCountExprNamespace840`, `ArgumentCountExprNamespace812`, `ArgumentCountExprNamespace541`, `ArgumentCountExprNamespace480`, `ArgumentCountExprNamespace404`, `ArgumentCountText56` |
| `CheckExprCall1300` | S100 / Diverges | `ExprCall1300Form` | `a-s064` (accepts) |
| `CheckExprCall1385` | S100 / Diverges | `GenericConstructorTypeArguments` | `a-s069` (accepts) |
| `CheckExprNamespace205` | S100 / Diverges | `ExprNamespace205Form` | `a-s181` (accepts) |
| `CheckExprNamespace249` | S018 / Diverges | `ExprNamespace249Form` | `a-s185` (accepts) |
| `CheckExprEntry243` | S100 / Diverges | `ExprEntry243Form` | `a-s082` (accepts) |
| `CheckExprEntry296` | S100 / Diverges | `ExprEntry296Form` | `a-s085` (accepts) |
| `CheckExprEntry304` | S100 / Diverges | `ExprEntry304Form` | `a-s104` (accepts) |
| `CheckExprEntry313` | split | — | `AngleAssertionExpression`, `SatisfiesExpression`, `InstantiationExpression`, `CommaExpression`, `TaggedTemplateExpression`, `ClassExpression`, `MetaPropertyExpression`, `PrivateNameExpression`, `ConstAssertionExpression`, `UnsupportedExpressionKind` |
| `CheckExprAssign41` | S100 / Diverges | `ExprAssign41Form` | `a-s014` (accepts) |
| `CheckExprAssign500` | S100 / Diverges | `ExprAssign500Form` | `a-s028` (accepts) |
| `CheckExprAssign598` | S100 / Diverges | `ExprAssign598Form` | `a-s030` (accepts) |
| `CheckExprMethod1544` | S100 / Diverges | `ExprMethod1544Form` | `a-s176` (accepts) |

### Syntax, operator, receiver, type pair, and callee splits

The 16 syntax/operator/receiver/type-pair catch-alls split as follows. The seventeenth catch-all is the shared argument-count builder, listed below. Invalid and parser-excluded forms keep distinct TscRejects sites.

| Prior site | Replacements |
|---|---|
| `CheckStmt210` | `LocalClassDeclaration`, `LocalFunctionDeclaration`, `LocalEnumDeclaration`, `LocalAliasDeclaration`, `LocalInterfaceDeclaration`, `LocalNamespaceDeclaration`, `LocalRejectedDeclaration` |
| `CheckDeclarations131` | `SourceInterfaceDeclaration`, `SourceNamespaceDeclaration` |
| `CheckStmt295` | `DoWhileStatement`, `ForInStatement`, `LabeledStatement`, `DebuggerStatement`, `WithStatement`, `UnsupportedStatementKind` |
| `CheckClassShape841` | `PrivateFieldDeclaration`, `PrivateMethodDeclaration`, `StaticBlockDeclaration`, `AutoAccessorDeclaration`, `UnsupportedClassMemberKind` |
| `CheckTyres97` | `TupleAnnotation`, `ThisAnnotation`, `QueryAnnotation`, `StructuralAnnotation`, `OptionalAnnotation`, `RestAnnotation`, `ConditionalAnnotation`, `InferAnnotation`, `OperatorAnnotation`, `IndexedAnnotation`, `MappedAnnotation`, `PredicateAnnotation`, `ImportAnnotation`, `StringLiteralAnnotation`, `NumberLiteralAnnotation`, `BooleanLiteralAnnotation`, `BigIntLiteralAnnotation`, `TemplateLiteralAnnotation`, `UnsupportedAnnotationKind` |
| `CheckTyres180` | `NeverAnnotation`, `UnknownAnnotation`, `SymbolAnnotation`, `BigIntAnnotation`, `UnsupportedKeywordKind` |
| `CheckTyres698` | `FunctionTypeRestParameter`, `FunctionTypeArrayPattern`, `FunctionTypeObjectPattern` |
| `CheckExprOperator142` | `TypeofOperator`, `VoidOperator`, `UnaryPlusOperator` |
| `CheckExprOperator495` | `InOperator`, `ExponentOperator` |
| `CheckExprOperator1236` | `CompoundEnumOperand`, `CompoundStringOperand`, `CompoundInvalidOperand` |
| `CheckExprOperator1296` | `BinaryEnumOperand`, `StringRelationalOperand`, `BinaryStringOperand`, `BooleanRelationalOperand`, `BinaryInvalidOperand` |
| `CheckExprOperator1576` | `IdentityAssertion`, `IntegerEnumAssertion`, `NullableClassAssertion`, `StringAliasAssertion`, `InvalidAssertion` |
| `CheckExprEntry313` | `AngleAssertionExpression`, `SatisfiesExpression`, `InstantiationExpression`, `CommaExpression`, `TaggedTemplateExpression`, `ClassExpression`, `MetaPropertyExpression`, `PrivateNameExpression`, `ConstAssertionExpression`, `UnsupportedExpressionKind` |
| `CheckExprMember652` | `BooleanMember`, `FunctionMember`, `GeneratorMember`, `EnumMember`, `LiteralAliasMember`, `InvalidReceiverMember` |
| `CheckExprCall1130` | `BooleanMethod`, `FunctionMethod`, `GeneratorMethod`, `EnumMethod`, `LiteralAliasMethod`, `InvalidReceiverMethod` |
| `CheckTypeRules329` | `LiteralAliasToString`, `EnumToInteger`, `ArrayToFixedArray`, `FunctionParameterIdentity`, `AssignmentTypeMismatch` |

The unbound value, function, and type fallbacks also split by origin. The pinned stock ES2022 and ESNext.Disposable lib name lists distinguish excluded lib names from unbound source names. Excluded lib constructors share the lib constructor site. Source declarations take precedence. Nullable nominal assignment separates assignment narrowing (C24 row 26) from generic argument identity (row 25). String concat has its own variadic count reason.

| New site | Code / class | Variant | Witness or guard |
|---|---|---|---|
| `LocalClassDeclaration` | S100 / Diverges | `LocalClassDeclaration` | `b-stmt-210` (accepts) |
| `LocalFunctionDeclaration` | S100 / Diverges | `LocalFunctionDeclaration` | `c-namespace-local-function` (accepts) |
| `LocalEnumDeclaration` | S100 / Diverges | `LocalEnumDeclaration` | `round5-LocalEnumDeclaration` (accepts) |
| `LocalAliasDeclaration` | S100 / Diverges | `LocalAliasDeclaration` | `c-alias-local` (accepts) |
| `LocalInterfaceDeclaration` | S100 / Diverges | `LocalInterfaceDeclaration` | `round5-LocalInterfaceDeclaration` (accepts) |
| `LocalNamespaceDeclaration` | S100 / TscRejects | `—` | `round5-LocalNamespaceDeclaration` (TS1235) |
| `LocalRejectedDeclaration` | S100 / TscRejects | `—` | unreachable — Var and Using declarations are handled before this branch; the inner match names every other Decl variant. |
| `SourceInterfaceDeclaration` | S100 / Diverges | `SourceInterfaceDeclaration` | `b-declarations-131` (accepts) |
| `SourceNamespaceDeclaration` | S100 / Diverges | `SourceNamespaceDeclaration` | `b-declarations-78-namespace` (accepts) |
| `DoWhileStatement` | S100 / Diverges | `DoWhileStatement` | `round5-DoWhileStatement` (accepts) |
| `ForInStatement` | S100 / Diverges | `ForInStatement` | `round5-ForInStatement` (accepts) |
| `LabeledStatement` | S100 / Diverges | `LabeledStatement` | `round5-LabeledStatement` (accepts) |
| `DebuggerStatement` | S100 / Diverges | `DebuggerStatement` | `b-stmt-295` (accepts) |
| `WithStatement` | S100 / TscRejects | `—` | unreachable — parse uses strict TypeScript mode, which rejects a with statement before checking. |
| `UnsupportedStatementKind` | S100 / TscRejects | `—` | unreachable — The outer statement match handles all remaining Stmt variants; the inner match names each rejected variant. |
| `PrivateFieldDeclaration` | S100 / Diverges | `PrivateFieldDeclaration` | `a-s030` (accepts) |
| `PrivateMethodDeclaration` | S100 / Diverges | `PrivateMethodDeclaration` | `round5-PrivateMethodDeclaration` (accepts) |
| `StaticBlockDeclaration` | S100 / Diverges | `StaticBlockDeclaration` | `round5-StaticBlockDeclaration` (accepts) |
| `AutoAccessorDeclaration` | S100 / Diverges | `AutoAccessorDeclaration` | `round5-AutoAccessorDeclaration` (accepts) |
| `UnsupportedClassMemberKind` | S100 / TscRejects | `—` | unreachable — Constructor, Method, ClassProp, TsIndexSignature, and Empty are handled earlier; the inner match names all remaining variants. |
| `TupleAnnotation` | S100 / Diverges | `TupleAnnotation` | `a-s191` (accepts) |
| `ThisAnnotation` | S100 / Diverges | `ThisAnnotation` | `round5-ThisAnnotation` (accepts) |
| `QueryAnnotation` | S100 / Diverges | `QueryAnnotation` | `round5-QueryAnnotation` (accepts) |
| `StructuralAnnotation` | S100 / Diverges | `StructuralAnnotation` | `round5-StructuralAnnotation` (accepts) |
| `OptionalAnnotation` | S100 / TscRejects | `—` | unreachable — Optional types occur inside tuples only; TupleAnnotation rejects the enclosing tuple without visiting its elements. |
| `RestAnnotation` | S100 / TscRejects | `—` | unreachable — Rest types occur inside tuples only; TupleAnnotation rejects the enclosing tuple without visiting its elements. |
| `ConditionalAnnotation` | S100 / Diverges | `ConditionalAnnotation` | `round5-ConditionalAnnotation` (accepts) |
| `InferAnnotation` | S100 / TscRejects | `—` | `round5-InferAnnotation` (TS1338) |
| `OperatorAnnotation` | S100 / Diverges | `OperatorAnnotation` | `round5-OperatorAnnotation` (accepts) |
| `IndexedAnnotation` | S100 / Diverges | `IndexedAnnotation` | `round5-IndexedAnnotation` (accepts) |
| `MappedAnnotation` | S100 / Diverges | `MappedAnnotation` | `round5-MappedAnnotation` (accepts) |
| `PredicateAnnotation` | S100 / Diverges | `PredicateAnnotation` | `round5-PredicateAnnotation` (accepts) |
| `ImportAnnotation` | S100 / Diverges | `ImportAnnotation` | `round5-import-type` (accepts) |
| `StringLiteralAnnotation` | S100 / Diverges | `StringLiteralAnnotation` | `round5-StringLiteralAnnotation` (accepts) |
| `NumberLiteralAnnotation` | S100 / Diverges | `NumberLiteralAnnotation` | `c-annotation-literal` (accepts) |
| `BooleanLiteralAnnotation` | S100 / Diverges | `BooleanLiteralAnnotation` | `round5-BooleanLiteralAnnotation` (accepts) |
| `BigIntLiteralAnnotation` | S100 / Diverges | `BigIntLiteralAnnotation` | `round5-BigIntLiteralAnnotation` (accepts) |
| `TemplateLiteralAnnotation` | S100 / Diverges | `TemplateLiteralAnnotation` | `round5-TemplateLiteralAnnotation` (accepts) |
| `UnsupportedAnnotationKind` | S100 / TscRejects | `—` | unreachable — The outer annotation match and the inner rejected-form match cover every TsType variant. |
| `NeverAnnotation` | S100 / Diverges | `NeverAnnotation` | `c-annotation-never` (accepts) |
| `UnknownAnnotation` | S100 / Diverges | `UnknownAnnotation` | `round5-UnknownAnnotation` (accepts) |
| `SymbolAnnotation` | S100 / Diverges | `SymbolAnnotation` | `round5-SymbolAnnotation` (accepts) |
| `BigIntAnnotation` | S100 / Diverges | `BigIntAnnotation` | `round5-BigIntAnnotation` (accepts) |
| `UnsupportedKeywordKind` | S100 / TscRejects | `—` | unreachable — The outer keyword match and the inner Never, Unknown, Symbol, BigInt match cover every keyword kind. |
| `FunctionTypeRestParameter` | S100 / Diverges | `FunctionTypeRestParameter` | `c-function-rest` (accepts) |
| `FunctionTypeArrayPattern` | S100 / Diverges | `FunctionTypeArrayPattern` | `round5-FunctionTypeArrayPattern` (accepts) |
| `FunctionTypeObjectPattern` | S100 / Diverges | `FunctionTypeObjectPattern` | `round5-FunctionTypeObjectPattern` (accepts) |
| `TypeofOperator` | S100 / Diverges | `TypeofOperator` | `a-s208` (accepts) |
| `VoidOperator` | S100 / Diverges | `VoidOperator` | `round5-VoidOperator` (accepts) |
| `UnaryPlusOperator` | S100 / Diverges | `UnaryPlusOperator` | `round5-UnaryPlusOperator` (accepts) |
| `InOperator` | S100 / Diverges | `InOperator` | `round5-InOperator` (accepts) |
| `ExponentOperator` | S100 / Diverges | `ExponentOperator` | `a-s216` (accepts) |
| `CompoundEnumOperand` | S100 / Diverges | `CompoundEnumOperand` | `a-s226` (accepts) |
| `CompoundStringOperand` | S100 / Diverges | `CompoundStringOperand` | `round5-CompoundStringOperand` (accepts) |
| `CompoundInvalidOperand` | S100 / TscRejects | `—` | `round5-CompoundInvalidOperand` (TS2365) |
| `BinaryEnumOperand` | S100 / Diverges | `BinaryEnumOperand` | `a-s228` (accepts) |
| `StringRelationalOperand` | S100 / Diverges | `StringRelationalOperand` | `round5-StringRelationalOperand` (accepts) |
| `BinaryStringOperand` | S100 / Diverges | `BinaryStringOperand` | `round5-BinaryStringOperand` (accepts) |
| `BooleanRelationalOperand` | S100 / Diverges | `BooleanRelationalOperand` | `round5-BooleanRelationalOperand` (accepts) |
| `BinaryInvalidOperand` | S100 / TscRejects | `—` | `c-corpus-r290-generic-relational-operand` (TS2365) |
| `IdentityAssertion` | S100 / Diverges | `IdentityAssertion` | `a-s234` (accepts) |
| `IntegerEnumAssertion` | S100 / Diverges | `IntegerEnumAssertion` | `round5-IntegerEnumAssertion` (accepts) |
| `NullableClassAssertion` | S100 / Diverges | `NullableClassAssertion` | `round5-NullableClassAssertion` (accepts) |
| `StringAliasAssertion` | S100 / Diverges | `StringAliasAssertion` | `round5-StringAliasAssertion` (accepts) |
| `InvalidAssertion` | S100 / TscRejects | `—` | `round5-InvalidAssertion` (TS2352) |
| `AngleAssertionExpression` | S100 / Diverges | `AngleAssertionExpression` | `round5-AngleAssertionExpression` (accepts) |
| `SatisfiesExpression` | S100 / Diverges | `SatisfiesExpression` | `round5-SatisfiesExpression` (accepts) |
| `InstantiationExpression` | S100 / Diverges | `InstantiationExpression` | `round5-InstantiationExpression` (accepts) |
| `CommaExpression` | S100 / Diverges | `CommaExpression` | `round5-CommaExpression` (accepts) |
| `TaggedTemplateExpression` | S100 / Diverges | `TaggedTemplateExpression` | `a-s087` (accepts) |
| `ClassExpression` | S100 / Diverges | `ClassExpression` | `round5-ClassExpression` (accepts) |
| `MetaPropertyExpression` | S100 / Diverges | `MetaPropertyExpression` | `round5-MetaPropertyExpression` (accepts) |
| `PrivateNameExpression` | S100 / TscRejects | `—` | unreachable — Standalone private names fail parsing; a private name in an in expression is not visited because InOperator rejects first. |
| `ConstAssertionExpression` | S100 / TscRejects | `—` | `round5-ConstAssertionExpression` (accepts) |
| `UnsupportedExpressionKind` | S100 / TscRejects | `—` | unreachable — The remaining Invalid and JSX variants are rejected by the TypeScript parser with JSX disabled. |
| `BooleanMember` | S100 / Diverges | `BooleanMember` | `a-s157` (accepts) |
| `FunctionMember` | S100 / Diverges | `FunctionMember` | `round5-FunctionMember` (accepts) |
| `GeneratorMember` | S100 / Diverges | `GeneratorMember` | `round5-GeneratorMember` (accepts) |
| `EnumMember` | S100 / Diverges | `EnumMember` | `round5-EnumMember` (accepts) |
| `LiteralAliasMember` | S100 / Diverges | `LiteralAliasMember` | `round5-LiteralAliasMember` (accepts) |
| `InvalidReceiverMember` | S100 / TscRejects | `—` | `round5-InvalidReceiverMember` (TS18050) |
| `BooleanMethod` | S100 / Diverges | `BooleanMethod` | `a-s062` (accepts) |
| `FunctionMethod` | S100 / Diverges | `FunctionMethod` | `round5-FunctionMethod` (accepts) |
| `GeneratorMethod` | S100 / TscRejects | `—` | unreachable — The generator receiver branch returns before the final method-receiver match. |
| `EnumMethod` | S100 / Diverges | `EnumMethod` | `round5-EnumMethod` (accepts) |
| `LiteralAliasMethod` | S100 / Diverges | `LiteralAliasMethod` | `round5-LiteralAliasMethod` (accepts) |
| `InvalidReceiverMethod` | S100 / TscRejects | `—` | `round5-InvalidReceiverMethod` (TS18050) |
| `LiteralAliasToString` | S100 / Diverges | `LiteralAliasToString` | `round5-LiteralAliasToString` (accepts) |
| `EnumToInteger` | S100 / Diverges | `EnumToInteger` | `round5-EnumToInteger` (accepts) |
| `ArrayToFixedArray` | S100 / Diverges | `ArrayToFixedArray` | `round5-ArrayToFixedArray` (accepts) |
| `FunctionParameterIdentity` | S100 / Diverges | `FunctionParameterIdentity` | `round5-FunctionParameterIdentity` (accepts) |
| `AssignmentTypeMismatch` | S100 / TscRejects | `—` | `c-assign-rejected` (TS2322) |
| `TypeParameterDefault` | S100 / Diverges | `TypeParameterDefault` | `c-async-default-alias` (accepts) |
| `UnboundValueName` | S016 / TscRejects | `—` | `a-s126` (TS2304) |
| `UnboundFunctionName` | S016 / TscRejects | `—` | `a-s041` (TS2304) |
| `UnboundTypeName` | S016 / TscRejects | `—` | `c-corpus-r169-embedded-header-copy` (TS2304) |
| `UnknownNamespaceConstructor` | S100 / Diverges | `LibConstructorName` | `a-s074` (accepts) |
| `NullableNominalAssignment` | S005 / Diverges | `NullableNominalAssignment` | `round5-nullable-assignment` (accepts) |
| `ArrayPushArgumentCount` | split | — |  |
| `ArrayConcatArgumentCount` | split | — |  |
| `GeneratorNextArgumentCount` | split | — |  |
| `StringConcatArgumentCount` | S100 / Diverges | `StringConcatArgumentCount` | `round5-string-concat-count` (accepts) |

Every `check_argument_count` caller now passes its own RejectionSite. Spread calls and calls with an Error parameter suppress count follow-ons. Earlier arity guards remain independent sites.

| Argument-count caller | Code / class | Variant | Witness or guard |
|---|---|---|---|
| `ArgumentCountExprCall1605` | S100 / TscRejects | `—` | `round5-ArgumentCountExprCall1529` (TS2554) |
| `ArgumentCountExprCall1529` | S100 / TscRejects | `—` | `round5-constructor-error-type-count` (TS2304,TS2554) |
| `ArgumentCountExprCall1129` | S100 / TscRejects | `—` | `round5-ArgumentCountExprCall1129` (TS2554) |
| `ArgumentCountExprCall1113` | S100 / TscRejects | `—` | `round5-ArgumentCountExprCall1113` (TS2554) |
| `ArgumentCountExprCall1066` | S100 / Diverges | `GeneratorNextArgumentCount` | `round5-ArgumentCountExprCall1066` (accepts) |
| `ArgumentCountExprCall983` | S100 / TscRejects | `—` | `round5-ArgumentCountExprCall983` (TS2554) |
| `ArgumentCountExprCall979` | S100 / Diverges | `ArrayPushArgumentCount` | `a-s063` (accepts) |
| `ArgumentCountExprCall966` | S100 / TscRejects | `—` | `round5-ArgumentCountExprCall966` (TS2554) |
| `ArgumentCountExprCall936` | S100 / TscRejects | `—` | `round5-ArgumentCountExprCall936` (TS2554) |
| `ArgumentCountExprCall907` | S100 / TscRejects | `—` | `round5-ArgumentCountExprCall907` (TS2554) |
| `ArgumentCountExprCall846` | S100 / TscRejects | `—` | `round5-ArgumentCountExprCall846` (TS2554) |
| `ArgumentCountExprCall539` | S100 / TscRejects | `—` | `round5-ArgumentCountExprCall539` (TS2554) |
| `ArgumentCountExprCall282` | S100 / TscRejects | `—` | `round5-ArgumentCountExprCall282` (TS2554) |
| `ArgumentCountExprCall255` | S100 / TscRejects | `—` | `round5-foreign-count` (TS2554) |
| `ArgumentCountExprCall229` | S100 / TscRejects | `—` | `round5-ArgumentCountExprCall205` (TS2554) |
| `ArgumentCountExprCall205` | S100 / TscRejects | `—` | `round5-async-call-count` (TS2554) |
| `ArgumentCountExprEntry652` | S100 / TscRejects | `—` | `round5-ArgumentCountExprEntry652` (TS2554) |
| `ArgumentCountExprEntry566` | S100 / TscRejects | `—` | `round5-ArgumentCountExprEntry566` (TS2554) |
| `ArgumentCountExprMethod1410` | S100 / TscRejects | `—` | unreachable — This caller requires arg.spread.is_some(); check_argument_count suppresses counts for spread arguments. |
| `ArgumentCountExprMethod1299` | S100 / TscRejects | `—` | unreachable — This caller handles a single spread after the Set arity check; spread or an Error parameter suppresses counts. |
| `ArgumentCountExprMethod1244` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod1244` (TS2554) |
| `ArgumentCountExprMethod1235` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod1235` (TS2554) |
| `ArgumentCountExprMethod1161` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod1161` (TS2554) |
| `ArgumentCountExprMethod1147` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod1147` (TS2554) |
| `ArgumentCountExprMethod1139` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod1139` (TS2554) |
| `ArgumentCountExprMethod1134` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod1134` (TS2554) |
| `ArgumentCountExprMethod1120` | S100 / TscRejects | `—` | `round5-map-reference-get-count` (TS2554) |
| `ArgumentCountExprMethod547` | S100 / TscRejects | `—` | unreachable — copyWithin returns before this call unless the argument count is 2 or 3, matching its required and total counts. |
| `ArgumentCountExprMethod521` | S100 / TscRejects | `—` | unreachable — unshift returns before this call unless the argument count is 1, matching its single required parameter. |
| `ArgumentCountExprMethod499` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod499` (TS2554) |
| `ArgumentCountExprMethod491` | S100 / TscRejects | `—` | unreachable — splice returns before this call unless the argument count is 1 or 2, matching its required and total counts. |
| `ArgumentCountExprMethod458` | S100 / Diverges | `ArrayConcatArgumentCount` | `round5-ArgumentCountExprMethod458` (accepts) |
| `ArgumentCountExprMethod450` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod450` (TS2554) |
| `ArgumentCountExprMethod438` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod438` (TS2554) |
| `ArgumentCountExprMethod413` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod413` (TS2554) |
| `ArgumentCountExprMethod388` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod388` (TS2554) |
| `ArgumentCountExprMethod347` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod347` (TS2554) |
| `ArgumentCountExprMethod328` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod328` (TS2554) |
| `ArgumentCountExprMethod183` | S100 / TscRejects | `—` | `round5-ArgumentCountExprMethod183` (TS2554) |
| `ArgumentCountExprMethod113` | S100 / TscRejects | `—` | unreachable — The number-method arity_ok guard matches the single parameter and its optional flag before this call. |
| `ArgumentCountExprNamespace1233` | S100 / TscRejects | `—` | `round5-ArgumentCountExprNamespace1233` (TS2554) |
| `ArgumentCountExprNamespace1225` | S100 / TscRejects | `—` | `round5-ArgumentCountExprNamespace1225` (TS2554) |
| `ArgumentCountExprNamespace1134` | S100 / TscRejects | `—` | `round5-ArgumentCountExprNamespace1134` (TS2554) |
| `ArgumentCountExprNamespace1113` | S100 / TscRejects | `—` | `round5-ArgumentCountExprNamespace1113` (TS2554) |
| `ArgumentCountExprNamespace878` | S100 / TscRejects | `—` | `round5-ArgumentCountExprNamespace878` (TS2554) |
| `ArgumentCountExprNamespace840` | S100 / TscRejects | `—` | `round5-ArgumentCountExprNamespace840` (TS2554) |
| `ArgumentCountExprNamespace812` | S100 / TscRejects | `—` | `round5-ArgumentCountExprNamespace812` (TS2554) |
| `ArgumentCountExprNamespace541` | S100 / TscRejects | `—` | unreachable — The Number global call returns unless c.args.len() equals params.len(); all parameters are required. |
| `ArgumentCountExprNamespace480` | S100 / TscRejects | `—` | unreachable — The Number predicate returns unless c.args.len() is 1, matching its single required parameter. |
| `ArgumentCountExprNamespace404` | S100 / TscRejects | `—` | unreachable — The Math call returns unless c.args.len() equals arity; params contains arity required parameters. |
| `ArgumentCountText56` | S100 / TscRejects | `—` | `round5-ArgumentCountText56` (TS2554) |

### Rules 9–11 and remeasured D sites

Rejected local declarations bind an Error local. Type and value lookup return poison before builtin or capture handling. Statement entry derives poison names from the active lexical scopes and restores the outer set on exit. Rejected local var bindings also bind poison. Rejected mirror `declare module` names and their named exports are collected before import resolution; re-exports and imports resolve them as poisoned. Rejected descriptor properties poison the whole literal before required-field checks.

| D1 site | Code / class | Variant | Remaining witness or guard |
|---|---|---|---|
| `CheckTyres214` | S100 / TscRejects | `—` | `c-worker-missing` (TS2314) |
| `CheckTyres419` | S100 / TscRejects | `—` | `c-error-generic` (TS2315) |
| `CheckTyres439` | S100 / TscRejects | `—` | `c-map-arity` (TS2314) |
| `CheckText49` | S100 / TscRejects | `—` | `c-uri-generic` (TS2558) |
| `DescriptorRequiredMemberQuotedKey` | S100 / TscRejects | `—` | unreachable — A rejected property increments the diagnostic count; descriptor_literal returns Error before required-field checks. |
| `CheckTyres495` | S100 / TscRejects | `—` | `c-date-generic` (TS2315) |
| `CheckTyres224` | S100 / TscRejects | `—` | `b-class_shape-529` (TS2314) |
| `CheckSignatures317` | S016 / TscRejects | `—` | `b-signatures-317` (TS2305) |
| `CheckTyres267` | S100 / TscRejects | `—` | `c-regexp-generic` (TS2315) |
| `CheckLookup19` | S100 / TscRejects | `—` | `c-type-only-value` (TS1361) |
| `CheckTyres512` | S100 / TscRejects | `—` | `c-class-generic` (TS2315) |
| `CheckTyres551` | S100 / TscRejects | `—` | `c-alias-generic` (TS2315) |
| `CheckExports370` | S016 / TscRejects | `—` | `b-exports-370` (TS2305) |
| `CheckTyres430` | S100 / TscRejects | `—` | `c-map-missing` (TS2314) |
| `CheckBindings97` | S100 / TscRejects | `—` | `c-pattern-iterator-bad` (TS2339) |
| `CheckExports101` | S100 / TscRejects | `—` | `b-exports-101` (TS2307) |

All 16 D1 sites are now TscRejects. Fifteen retain non-cascade rejected witnesses. `DescriptorRequiredMemberQuotedKey` is unreachable after descriptor property poisoning. The original cascade programs remain in the source fixture file. Independent regression tests require exactly one declaration diagnostic for a local class, function, alias, var, and a mirror module import.

Promise, FixedArray, Generator, sized aliases, and mirror aliases resolve an in-scope source declaration first. Worker, Inbox, Outbox, Map, Set, Date, RegExp, and Error already use scope guards. Array stays a builtin under stdlib §9.0. Seven acceptance controls assert checker success independently of the rejection index.

| D2 site | Code / class | Variant | Witness |
|---|---|---|---|
| `CheckTyres283` | S100 / TscRejects | `—` | `c-promise-missing` (TS2314) |
| `CheckTyres291` | S100 / TscRejects | `—` | `c-promise-arity` (TS2314) |
| `CheckTyres303` | S100 / TscRejects | `—` | `c-fixed-missing` (TS2314) |
| `CheckTyres311` | S100 / TscRejects | `—` | `c-fixed-arity` (TS2314) |
| `CheckTyres390` | S100 / Diverges | `ArrayTypeArgument` | `c-array-two-alias` (accepts) |
| `CheckTyres403` | S100 / Diverges | `Tyres403Form` | `c-generator-no-argument` (accepts) |

`round5-shadow-generator-next` pins S018 at `ClassUndeclaredMethodCall`. The exact requested `FixedArray<i32,3>` control now rejects at `NumberLiteralAnnotation` (S100, C24 row 17): its numeric type argument is unsupported for a source class, so instance creation stops before member checking. The compatible control keeps `class FixedArray<T,N> { tag: i32 = 7; }` and uses `FixedArray<i32,i32>`; it pins S018 at `ClassUndeclaredMemberRead`. Independent tests require both member controls to emit one S018 diagnostic from the named class site. This distinction preserves the literal-type restriction and detects restoration of the builtin branch.

Type-parameter defaults reject at function, method, class, and source alias declarations. Function/class declarations bind poison; generic methods retain a rejected template. Three independent controls require one TypeParameterDefault diagnostic and no use-site cascade. The variant cites C24 row 28. Explicitly supplied arguments do not hide a declaration default.

| D3 use site | Code / class | Variant | Remaining witness |
|---|---|---|---|
| `CheckGenerics283` | S100 / TscRejects | `—` | `c-generic-function-arity` (TS2558) |
| `CheckGenerics375` | S100 / TscRejects | `—` | `c-generic-method-arity` (TS2558) |
| `CheckGenerics492` | S100 / TscRejects | `—` | `c-generic-class-arity` (TS2314) |
| `CheckTyres518` | S100 / TscRejects | `—` | `c-class-missing` (TS2314) |

All four D3 use sites now have non-default TS-rejected witnesses. Defaults are pinned at TypeParameterDefault instead. D4 generic alias and generic function value are true C24 divergences with corrected messages. D5 type qualifier and instance/type-pair forms are split and carry their actual restriction.

The §42.1 pair keeps ReturnFlowCoverage and LambdaReturnFlowCoverage. Source-enum tests still require default coverage. `round5-source-enum-bytes` is accepted by both checkers: a value-class enum field read through Context.fromBytes can carry 42 without naming an enum member. Source enums therefore are not closed literal sets for return coverage. No C24 replacement is needed.

### Messages

| Site | Old message | New message |
|---|---|---|
| CheckDeclarations518 | `string-literal union aliases cannot be generic` | `source type aliases cannot be generic` |
| CheckExprLiteral394 | `generic function id requires explicit type arguments` | `generic function id has no first-class value; call it directly or use a lambda` (names are quoted in the diagnostic) |
| AsyncReturnNonReference, AsyncReturnQualifiedName, AsyncReturnAlias | `async functions must return Promise<T>` | `async functions must return an explicitly annotated builtin Promise<T>` (Promise is quoted in the diagnostic) |
| TypeParameterDefault (new) | — | `type-parameter defaults are not in the decided surface; pass an explicit type argument` |

The async alias fragment now uses a genuine accepted mirror typedef alias. Missing builtin Promise arguments and wrong builtin arity remain TscRejects after source alias poisoning removes their earlier cascades.

### Corpus and collision ids

Four nullable-narrowing reject entries retire. A rewrite would either change their purpose or contrive an unrelated TypeScript-accepted rejection. Their complete original sources and measured TS code sets remain under round5-old keys in the witness programs. No accept entry or golden changes.

| Entry | Action | Measured first checker diagnostic |
|---|---|---|
| r120-narrowing-escapes-conditional | retire | S005 at line 22, nominal nullable assignment; NullableNominalAssignment |
| r195-using-nullable-member | retire | S011 at line 14, nullable member access; NullableMember |
| r249-unnarrowed-nullable-field-call | retire | S100 at line 10, nullable method call; NullableCall |
| r264-narrowing-switch-join | retire | S011 at line 11, nullable member access; NullableMember |
| r344-type-parameter-default | add; TypeScript accepts | S100 at line 8, type-parameter defaults are not in the decided surface; pass an explicit type argument; TypeParameterDefault |

New C24 reject pin: **r344** (row 28). Retired C24-site entries: r120 (row 26), r195 (row 26), r249 (row 26), r264 (row 26). No existing entry is rewritten in round 5. The prior rounds’ rewrites and retirements remain. collisions.md is not edited.

### Verification and review

The prescribed compiler command reports 391 passed lib tests and one failed rule 4 test: ConstAssertionExpression. Since cargo stops at that lib failure, a supplementary run executes all remaining tests with exactly two explicit skips: the rule 4 test and the unchanged C24 corpus-index test. All 927 other compiler tests pass, including the full generic matrix, reject corpus, source-shadow controls, logical/shared narrowing, and re-export checks. The C24 index failure was measured separately during the supplementary run before that skip was added.

The generic matrix records now name C24 rows 1, 26, and 29 for truth, nullable assignment, and do-while, and Q3 for array-to-fixed conversion. C24 record matching also requires its stated diagnostic message, so a shared C24 heading cannot excuse another row. The control omission count changes from 7735 to 7697 as the documented controls become admissible. This expands the tested instance set; it does not skip failures.

Logical and shared narrowing tests now require NullableMemberForm on ordinary nullable-member sites while preserving SharedLocationNarrowing on ended shared facts. Re-export tests require the new source alias/interface and missing-import variants and the corrected generic alias message. Their codes, positions, and other text stay pinned. NullableCallForm now uses an initialized non-null callback with a nullable declared type, and round5-nullable-call-initialized pins it without an earlier truth-test failure.

r344 is accepted by the pre-round CLI binary (`check: no errors`) and rejected at its declaration by the new checker. The reject-corpus suite verifies its S100 at line 8.

The prescribed codegen command passes all 723 tests. No golden or pinned reject message moves. `cargo fmt --check` and `git diff --check` pass. Workspace all-target clippy passes with 3 compiler, 18 runtime, and 13 codegen lib warnings, within the 7/18/13 baseline; no new warning remains. The generator succeeds, and generated-document byte checks pass in the compiler suite. tools/gate.sh is not run. The mandatory review hygiene scan passes. Every changed path is within the authorized scope, and the list below covers all 108 cumulative changed files. The mandatory fresh Phase Review found the unsupported const-assertion variant and two source-shadow test gaps. The variant was removed and the test gaps were fixed. Re-review found no new findings. The acceptance-3 contract blocker remains open; the phase is not COMPLETE. The review hygiene scan passes.

The rule 4 batch measures 1216 witnesses and 458 carried variants in one TypeScript process. Warm cost: 1.102 seconds total, with 0.538 seconds for TypeScript and 0.366 seconds for checker/fragment checks. This final warm measurement runs after codegen finishes. The doc comment states 1.10 seconds (0.54 + 0.37). Rust source files remain at most 2000 lines.

### Complete cumulative changed-file list

The list includes the preserved rounds 1–4 base and every round 5 change. Generated documentation changes come from generate-api-reference. No file outside the authorized scope changes.

- `compiler/src/ambient.rs`
- `compiler/src/api_reference.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/container_argument.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/exports.rs`
- `compiler/src/check/expr.rs`
- `compiler/src/check/expr/aggregate.rs`
- `compiler/src/check/expr/array_of_and_map_copy.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/expr/namespace.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/field_initializer.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/host_entries.rs`
- `compiler/src/check/inference.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/instance_chain.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/layout.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mirror_provenance.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/namespace_import.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/opaque.rs`
- `compiler/src/check/pattern.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/divergence.rs`
- `compiler/src/hir/host_entry.rs`
- `compiler/src/language_reference.rs`
- `compiler/src/lib.rs`
- `compiler/src/parse.rs`
- `compiler/src/provenance.rs`
- `compiler/src/regex.rs`
- `compiler/src/tests/collections.rs`
- `compiler/tests/async_generic_method.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/field_values.rs`
- `compiler/tests/generic_method.rs`
- `compiler/tests/generic_tsc_matrix.rs`
- `compiler/tests/generic_tsc_matrix/api.rs`
- `compiler/tests/generic_tsc_matrix/kinds.rs`
- `compiler/tests/generic_tsc_matrix/product.rs`
- `compiler/tests/logical_narrowing.rs`
- `compiler/tests/re_exports.rs`
- `compiler/tests/shared_narrowing.rs`
- `compiler/tests/tsc_corpus.rs`
- `corpus/reject/r120-narrowing-escapes-conditional.ts` (deleted)
- `corpus/reject/r133-using-without-dispose.ts`
- `corpus/reject/r136-valuetype-align-not-in-set.ts`
- `corpus/reject/r137-descriptor-align.ts`
- `corpus/reject/r146-accessor-field-name-clash.ts`
- `corpus/reject/r148-switch-cross-case-read.ts`
- `corpus/reject/r154-namespace-read-before-declaration.ts`
- `corpus/reject/r155-class-read-before-declaration.ts`
- `corpus/reject/r161-field-method-member-name-clash.ts` (deleted)
- `corpus/reject/r162-duplicate-method-member-name.ts`
- `corpus/reject/r163-duplicate-field-member-name.ts` (deleted)
- `corpus/reject/r164-duplicate-static-member-name.ts` (deleted)
- `corpus/reject/r192-using-nullable-without-dispose.ts`
- `corpus/reject/r195-using-nullable-member.ts` (deleted)
- `corpus/reject/r224-field-without-initializer.ts`
- `corpus/reject/r225-reference-field-without-initializer.ts`
- `corpus/reject/r228-field-assigned-after-early-return.ts`
- `corpus/reject/r230-field-read-before-its-assignment.ts`
- `corpus/reject/r249-unnarrowed-nullable-field-call.ts` (deleted)
- `corpus/reject/r264-narrowing-switch-join.ts` (deleted)
- `corpus/reject/r292-generic-uninitialized-field.ts`
- `corpus/reject/r62-valuetype-fixed-array-layout-too-large.ts`
- `corpus/reject/r94-descriptor-method.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `compiler/src/ambient/lib_names.rs`
- `compiler/src/check/rejection_programs.rs`
- `compiler/src/check/rejection_programs_s154.txt`
- `compiler/src/check/rejection_regressions.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets_s154.txt`
- `compiler/src/check/rejection_tsc.cjs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/divergence/established.rs`
- `compiler/src/divergence/established_tail.rs`
- `compiler/src/divergence/group_a.rs`
- `compiler/src/divergence/round_five.rs`
- `compiler/src/divergence/round_five_splits.rs`
- `corpus/reject/r344-type-parameter-default.ts`
- `specs/tracking/s154-rejection-sites.md`


## Round 6 — checker facts, const assertions, and guard names

C24 row 18 now classifies `ConstAssertionExpression` as `Diverges`. Its independent TypeScript-accepted `1 as const` witness and carried fragment pass. **Red count: 0.** The current working tree from rounds 1–5 was preserved; no commit was made. The caller's existing `specs/blocks/collisions.md` edit was preserved without modification, including `Reject: r344`.

### Splits and facts

The ordinary-error arm in each row is `TscRejects` and carries no block. The nullable rows use the first name for the ordinary arm and the second name for the accepted initializer arm. Other rows likewise name the ordinary arm first. Existing diagnostic codes and messages are preserved.

| Ordinary-error site | Divergence site | Checker fact |
| --- | --- | --- |
| `NullableMember` | `NullableMemberNonNullInitializer` | Nearest declared binding has a non-null initializer; any preceding local write clears the fact. |
| `NullableCall` | `NullableCallNonNullInitializer` | Same binding initializer/write fact, applied to a local callee. |
| `NullableNominalAssignment` | `NullableNominalAssignmentNonNullInitializer` | The assigned expression identifies that binding; type-only assignments cannot claim the fact. |
| `FieldAssignmentMissingUnassignedExit` | `FieldAssignmentMissingNoNormalExit` | Checked constructor HIR has an unassigned normal exit versus assignment on every normal exit or no normal exit. |
| `FieldAssignmentAfterReturnUnassignedExit` | `FieldAssignmentAfterUnreachableReturn` | Reachability of the return, including a constant false branch. |
| `FieldAssignmentNestedUnassignedExit` | `FieldAssignmentNestedEveryNormalExit` | Checked branch assignments cover every reachable normal exit. |
| `ConstructorFieldReadUnassigned` | `ConstructorDefiniteFieldReadBeforeAssignment` | Source definite-assignment (!) field spelling; nested assignment flow uses a separate accepted site. |
| `ImmediateShadowedNameRead` | `BlockNameReadBeforeDeclaration` | Lookup crossed a function boundary; an immediate read does not. |
| `ImmediateNameWriteBeforeDeclaration` | `BlockNameWriteBeforeDeclaration` | Same lookup function-boundary count for writes. |
| `CatchBindingInvalidAnnotation` | `CatchBindingAnnotation` | The source catch annotation is the any keyword. |
| `CatchBindingPatternWithoutAny` | `CatchBindingPattern` | The destructured catch binding carries that any annotation. |
| `InstanceofRightNotClass` | `InstanceofRightNotErrorFamily` | The checked right operand names a class, generic class or function. |
| `InstanceofLeftPrimitive` | `InstanceofLeftNotErrorFamily` | The apparent left type is an object or callable instead of a primitive. |
| `ErrorMessageNonString` | `ErrorMessageType` | The apparent argument is a string literal alias instead of a non-string. |
| `GenericInferenceRequiredArgumentMissing` | `GenericInferenceNoCandidate` | Actual argument count is below the template required count. |
| `GenericInferenceIncompatibleKinds` | `GenericInferenceConflictingCandidates` | Both inferred candidates are apparent numeric kinds; other incompatible kinds remain ordinary errors. |
| `NamedImportUnresolvedModule` | `NamedImportModuleMissing` | A missing-module import has named specifiers instead of being a side-effect import. |
| `BindingPatternNonIterableSource` | `BindingPatternSourceKind` | The array-pattern source has apparent string type, which is iterable in TypeScript. |
| `WorkerMessageNonClass` | `WorkerMessagePlainClass` | The checked message is a value-class object instead of a primitive. |
| `ExplicitVoidReturnValue` | `VoidFunctionReturnValue` | A lambda has no explicit result annotation and inherits a contextual void result. |
| `DisjointLiteralAliasAssignment` | `AssignmentLiteralAlias` | Source literal-alias members are contained in the destination alias members. |
| `FunctionReturnPathMissing` | `FunctionReturnCoverage` | Checked HIR return coverage, constant branches and complete source-enum switches. |
| `LambdaReturnPathMissing` | `LambdaReturnCoverage` | Same checked return-coverage fact for a lambda. |
| `BuiltinArrayTypeArgumentCount` | `ArrayTypeArgumentCount` | Source Array binding exists; language Array remains builtin while TypeScript resolves the source declaration. |
| `FunctionImplementationMissing` | `FunctionBodyMissing` | Source overload group has an implementation or is ambient. |
| `GenericMethodImplementationMissing` | `GenericMethodBodyMissing` | Source method overload group has an implementation or belongs to an ambient class. |
| `ClassMemberDuplicateImplementation` | `ClassMemberNameClash` | Both repeated methods have bodies; a bodyless signature can be an overload. |
| `UsingBindingNotDisposable` | `UsingBindingResourceType` | The checked resource is null, which TypeScript permits in using. |
| `IteratorMethodValueArgumentCount` | `IteratorMethodArgumentCount` | All arguments are spread arguments rather than actual value arguments. |
| `DuplicateTopLevelClass` | `TopLevelNameClash` | Both scope items are class declarations. |
| `TopLevelOverloadImplementationMissing` | `TopLevelNameClash` | All repeated nonambient function signatures lack an implementation. |
| `AsyncReturnNotPromise` | `AsyncReturnNonReference` | Parenthesized annotation resolves through apparent type to AsyncHandle. |
| `AsyncReturnSourceNotPromise` | `AsyncReturnAlias` | Source type alias has apparent AsyncHandle type instead of a source class. |
| `PoisonedRelativeDefaultImport` | `PoisonedDefaultImport` | Missing relative-module path versus an external poisoned module. |
| `InstancePrototypeMissing` | `InstancePrototypeMember` | Source class actually declares a prototype field. |
| `CollectionCallbackNotFunction` | `CollectionCallbackTypeMismatch` | Checked callback apparent type is callable versus a scalar. |
| `BuiltinValueTypeAlignmentOutsideSet` | `ValueTypeAlignmentOutsideSet` | A source function named ValueType shadows the ambient decorator, even if declared later. |
| `BuiltinDescriptorOptions` | `DescriptorOptions` | Same full-module source-function lookup for Descriptor. |
| `UnionUnknownTypeMember` | `UnionGeneralUnionAndUndefined` | Named union member is absent from substitutions, source scope and admitted library type names. |
| `WireEnumMethodMember` | `WireEnumMemberForm` | Source method signature is not a numeric Record member. |
| `DuplicateNumericIndexSignature` | `ClassIndexSignatureCount` | Existing and incoming index signatures both use sized numeric key types. |
| `ComputedFieldUnboundName` | `ComputedFieldDeclaration` | Computed bare identifier has neither a scope binding nor a source/import declaration anywhere in the module. |
| `ExternalImportEqualsDeclaration` | `UnsupportedModuleDeclaration` | TsImportEquals module_ref is an external module reference rather than an entity name. |
| `ExportAssignmentEsModule` | `UnsupportedModuleDeclaration` | TsExportAssignment is rejected by the pinned ESNext module configuration, with or without additional exports. |
| `DescriptorRequiredFieldUnassigned` | `DescriptorRequiredWithoutDefinite` | Nonambient field has a checked source type needing a value (including reference/descriptor classes, arrays and Q32 aliases) and represented source AST flow proves an unassigned normal exit. Absent/empty/one-arm constructors are covered; unsupported switch/try/loop flow stays shared instead of claiming a rejection fact. |
| `MirrorArrayParameterNonIterable` | `MirrorParameter` | Boundary array binding pattern has a concrete noniterable scalar annotation; source aliases are excluded. |
| `ConstructorFieldReadUnassigned` | `ConstructorFieldReadAfterNestedAssignment` | Checked HIR proves the field assigned before the read, including both branches, same-arm/loop-body ordering, and chained assignments. |

The nullable initializer fact is scoped to the nearest declaration, never inherited by a shadowed binding, and cleared by a preceding assignment. Parameter, reassigned-null, and shadowed-binding controls check that boundary. Type-only assignability cannot claim an expression initializer fact. SharedLocation narrowing remains unchanged. Descriptor source AST and constructor HIR normal-exit facts classify diagnostics without loosening the language's strict constructor assignment rule.

### Restored and retired corpus entries

These are measured first diagnostics from the all-pass corpus harness. No entries were retired in round 6. Earlier r148/r62/r94 rewrites remain because their rejection witnesses have the independent or unrepresented TypeScript conditions documented below.

| Entry | Measured first diagnostic | Disposition |
| --- | --- | --- |
| `r120-narrowing-escapes-conditional.ts` | `S005` at line 22 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r195-using-nullable-member.ts` | `S011` at line 14 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r249-unnarrowed-nullable-field-call.ts` | `S100` at line 10 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r264-narrowing-switch-join.ts` | `S011` at line 11 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r224-field-without-initializer.ts` | `S100` at line 8 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r225-reference-field-without-initializer.ts` | `S100` at line 12 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r228-field-assigned-after-early-return.ts` | `S100` at line 12 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r292-generic-uninitialized-field.ts` | `S100` at line 10 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r133-using-without-dispose.ts` | `S100` at line 10 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r192-using-nullable-without-dispose.ts` | `S100` at line 10 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r146-accessor-field-name-clash.ts` | `S017` at line 10 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r162-duplicate-method-member-name.ts` | `S017` at line 12 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r161-field-method-member-name-clash.ts` | `S017` at line 9 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r163-duplicate-field-member-name.ts` | `S017` at line 9 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r164-duplicate-static-member-name.ts` | `S017` at line 9 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r154-namespace-read-before-declaration.ts` | `S100` at line 9 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r155-class-read-before-declaration.ts` | `S100` at line 17 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r230-field-read-before-its-assignment.ts` | `S100` at line 15 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r136-valuetype-align-not-in-set.ts` | `S100` at line 7 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |
| `r137-descriptor-align.ts` | `S100` at line 7 | Restored byte-for-byte from HEAD; first-diagnostic harness passes; no divergence block at the ordinary-error site. |

### Remaining shared sites

The exhaustive witness index has 169 sites with both accepted and rejected TypeScript witnesses. Each row names one measured witness of each class; the index retains all witnesses. A later/unrelated error is not a fact about the local rejection guard. S014 is an explicit contract exception: its required block cannot be suppressed even when raw syntax makes the TypeScript error obvious.

| Site (API rows use their ApiRejectionId) | Accepted witness | Rejected witness | Why it remains shared |
| --- | --- | --- | --- |
| `DescriptorRequiredWithoutDefinite` | `b-class_shape-586-accept` | `round6-descriptor-switch-unassigned` | Descriptor shape checking has source AST but deliberately does not lower constructor HIR. Its represented assignment fact covers straight-line/block/if/return/throw paths; switch/try/loop evaluation requires additional definite-assignment analysis. The unrepresented forms keep the shared restriction; the switch-assigned control proves they cannot default to TscRejects. |
| `SwitchCaseRead` | `c-lookup-switch-let-closure` | `c-lookup-switch-const-closure` | TypeScript case reachability and binding availability are not represented by the pending-binding lookup. const/let and function-boundary counts alone do not separate the measured pair; the attempted split was reverted. |
| `AsyncHandleUnawaited` | `b-bodies-236` | `round5-AsyncMethodArgumentCount` | The rejected witness has an independent method argument-count error. Dropped-handle ownership itself is accepted by TypeScript in both witnesses. |
| `StaticFieldInitializerMissing` | `b-bodies-387` | `b-class_shape-529` | The rejected witness independently uses a Worker type argument outside its TypeScript constraint. The missing static initializer is accepted in both; no initializer fact separates them. |
| `LiteralAliasDuplicateCase` | `b-stmt-1238-accept` | `b-stmt-1238` | TypeScript retains the literal value of the initialized discriminant; the checker retains the declared alias and non-nullness, not that value. The parameter control has the same declared alias and duplicate cases. |
| `DescriptorMethod` | `b-class_shape-150` | `c-corpus-r94-descriptor-method` | The rejected corpus witness contains an independent optional-member read/type error. The data-only method restriction is TypeScript-accepted regardless of that body error. |
| `PrivateFieldDeclaration` | `a-s030` | `a-s222` | The rejected witness independently contains a forbidden optional-chain/private-field access. Both private declarations themselves are TypeScript-accepted. |
| `IndexSignatureGetterMismatch` | `b-class_shape-765-accept` | `b-class_shape-765` | The rejected witness independently repeats numeric index signatures (now split at DuplicateNumericIndexSignature). TypeScript requires no matching language getter for either witness. |
| `ClassIndexSetSignature` | `b-class_shape-765-accept` | `b-class_shape-765` | Same independent duplicate numeric index-signature error; the language setter-signature condition has no TypeScript counterpart. |
| `GenericClassStaticMember` | `b-declarations-255` | `round5-poisoned-constructor-count` | The rejected witness independently calls a constructor with excess arguments. The generic class static member is TypeScript-accepted in both. |
| `InitializerRouteRead` | `c-root-initializer-recursive` | `c-root-initializer-callback` | The effect-summary route retains labels and transitive reads, but [lambda] also labels library callbacks; it loses the direct IIFE versus callback origin and TypeScript definite-assignment context. A lambda label alone cannot establish rejection. |
| `NumberLiteralAnnotation` | `c-annotation-literal` | `round5-shadow-fixed-length` | The rejected witness independently instantiates a source-shadowed FixedArray generic with an argument that violates its source constraint. Numeric literal annotations themselves are accepted; literal resolution carries no enclosing generic constraint. |
| `VoidTypeOutsideResult` | `c-corpus-r337-void-array` | `b-bodies-734-void` | The rejected witness has an independently uninitialized required field. Void type syntax is TypeScript-accepted in both; type-position resolution has no constructor-assignment fact. |
| `FixedArrayByteLimit` | `c-fixed-layout-pattern` | `c-corpus-r62-valuetype-fixed-array-layout-too-large` | The rejected corpus witness has an independently uninitialized required field. The language aggregate-byte limit has no TypeScript counterpart. |
| `TypeNameUnknown` | `a-s087` | `a-probe-iterable` | The rejected witness independently passes an incompatible Iterable element type to Map.groupBy. Both named library types exist in TypeScript; library-name resolution cannot separate a later call error. |
| `IntersectionTypeAnnotation` | `c-intersection` | `b-stmt-425-accept` | The rejected witness independently initializes a using resource that does not implement disposal. Intersection syntax itself is accepted in both. |
| `NullableNonReference` | `c-nullable-scalar` | `b-stmt-516-nullable` | The rejected witness independently returns no value from a value-returning function. Scalar nullable annotation syntax itself is accepted in both. |
| `NullableValueClassAssignment` | `c-nullable-function-class` | `c-nullable-function-class-reject` | The accepted witness merges a callable interface into the class in TypeScript. The checker poisons/rejects source interfaces instead of retaining their merged call signatures; the apparent value-class type does not carry that fact. |
| `EmptyArrayInference` | `a-s002` | `b-stmt-425-accept` | The rejected witness independently uses a nondisposable resource. An uncontextualized empty array itself is accepted in both; array inference has no disposal condition. |
| `FieldInitializerThisUse` | `a-s080` | `a-s222` | The rejected witness independently contains optional-chain/private-field syntax. The field initializer using this itself is TypeScript-accepted in both. |
| `StringLocaleCompare` | `a001.ts` | `a001-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `StringToLocaleUpperCase` | `a002.ts` | `a002-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `StringToLocaleLowerCase` | `a003.ts` | `a003-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `StringNormalize` | `a004.ts` | `a004-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `RegexStringMatch` | `a005.ts` | `a005-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `RegexStringMatchAll` | `a006.ts` | `a006-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayFind` | `a010.ts` | `a010-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayFindLast` | `a011.ts` | `a011-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayFlat` | `a012.ts` | `a012-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayFlatMap` | `a013.ts` | `a013-c.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayEntries` | `a014.ts` | `a014-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayKeys` | `a015.ts` | `a015-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayValues` | `a016.ts` | `a016-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateLocalGetFullYear` | `a017.ts` | `a017-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateLocalGetMonth` | `a018.ts` | `a018-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateLocalGetDate` | `a019.ts` | `a019-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateLocalGetDay` | `a020.ts` | `a020-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateLocalGetHours` | `a021.ts` | `a021-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateLocalGetMinutes` | `a022.ts` | `a022-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateLocalGetSeconds` | `a023.ts` | `a023-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateLocalGetMilliseconds` | `a024.ts` | `a024-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateLocalGetTimezoneOffset` | `a025.ts` | `a025-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateStringToString` | `a027.ts` | `a027-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateStringToDateString` | `a028.ts` | `a028-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateStringToTimeString` | `a029.ts` | `a029-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateStringToLocaleString` | `a030.ts` | `a030-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateStringToLocaleDateString` | `a031.ts` | `a031-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateStringToLocaleTimeString` | `a032.ts` | `a032-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MapKeys` | `a033.ts` | `a033-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MapValues` | `a034.ts` | `a034-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MapEntries` | `a035.ts` | `a035-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `SetKeys` | `a036.ts` | `a036-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `SetValues` | `a037.ts` | `a037-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `SetEntries` | `a038.ts` | `a038-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonStringifyMapKV` | `a039.ts` | `a039-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonStringifySetK` | `a040.ts` | `a040-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonStringifyObject` | `a041.ts` | `a041-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonStringifyFunction` | `a042.ts` | `a042-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonStringifyF16` | `a043.ts` | `a043-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonParseDateText` | `a045.ts` | `a045-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormIsNaNValue` | `a046.ts` | `a046-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormIsFiniteValue` | `a047.ts` | `a047-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormParseIntValue` | `a048.ts` | `a048-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormNumberValue` | `a049.ts` | `a049-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormNewNumberValue` | `a050.ts` | `a050-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormToLocaleString` | `a051.ts` | `a051-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormToString` | `a052.ts` | `a052-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormToPrecision` | `a053.ts` | `a053-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormMaxMinHypotWithMoreThanTwoArguments` | `a055.ts` | `a055-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormMathUsedAsAValue` | `a056.ts` | `a056-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormDateParse` | `a057.ts` | `a057-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormNewDate` | `a058.ts` | `a058-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormNewDateYearMonth` | `a059.ts` | `a059-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormTemplateInterpolation` | `a060.ts` | `a060-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormDirectComparison` | `a061.ts` | `a061-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormSet` | `a062.ts` | `a062-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormSort` | `a063.ts` | `a063-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormReduceCallback` | `a064.ts` | `a064-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormReduceRightCallback` | `a065.ts` | `a065-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormCallbackValueIndexArray` | `a066.ts` | `a066-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormSpliceStartDeleteCountItems` | `a067.ts` | `a067-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormUnshiftValueValues` | `a068.ts` | `a068-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormNonCallbackTMethods` | `a069-accepted.ts` | `a069.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MapScalarGet` | `a070.ts` | `a070-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MapNonNullableGet` | `a071.ts` | `a071-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormNewMapIterable` | `a072.ts` | `a072-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormNewSetMap` | `a073-b.ts` | `a073.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormNewSetGeneratorT` | `a074.ts` | `a074-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormArrayUsedAsAValue` | `a075.ts` | `a075-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormArrayFromSourceMapFn` | `a076.ts` | `a076-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormArrayFromMap` | `a077.ts` | `a077-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormArrayFromGeneratorT` | `a078.ts` | `a078-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormIsArrayValue` | `a079.ts` | `a079-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormOfValue` | `a080.ts` | `a080-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormNewArrayLength` | `a081.ts` | `a081-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `FormAlgebraNonSet` | `a083.ts` | `a083-old.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArraySpreadFixedArray` | `s001.ts` | `s001-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArraySpreadMap` | `s002.ts` | `s002-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArraySpreadGenerator` | `s003.ts` | `s003-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArraySpreadSource` | `s004.ts` | `s004-old.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MapCopyKey` | `s005-c.ts` | `s005.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ContextBytesMissingType` | `s006.ts` | `s006-c.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ContextBytesSpread` | `s009-b.ts` | `s009.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `CallSpread` | `s010-b.ts` | `s010.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `SetSourceSpread` | `s011-b.ts` | `s011.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `SetSourceDomain` | `s012-c.ts` | `s012.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `NewMapSetKey` | `s013.ts` | `s013-c.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `RegexSticky` | `s014.ts` | `s014-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ContextValue` | `s015.ts` | `s015-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `NumberValue` | `s016.ts` | `s016-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonValue` | `s017.ts` | `s017-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateValue` | `s018.ts` | `s018-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MapSetValue` | `s019.ts` | `s019-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `NumberGlobalValue` | `s020.ts` | `s020-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `CoercingGlobalValue` | `s021.ts` | `s021-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `RegexMember` | `s022.ts` | `s022-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateMemberWrite` | `s023.ts` | `s023-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateMethodValue` | `s024.ts` | `s024-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `NumberMethodValue` | `s025.ts` | `s025-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayElementDomain` | `s027.ts` | `s027-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayJoinDomain` | `s028.ts` | `s028-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayCallbackSpread` | `s029-b.ts` | `s029.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayAccumulatorDomain` | `s030.ts` | `s030-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayMapResult` | `s031.ts` | `s031-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MapGroupByKey` | `s032.ts` | `s032-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayStaticMember` | `s033-c.ts` | `s033.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayFromTypeCount` | `s034-d.ts` | `s034.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayFromSpread` | `s036-b.ts` | `s036.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayFromSource` | `s037-b.ts` | `s037.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `CallbackParameterCount` | `s038-d.ts` | `s038.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ContextMember` | `s039.ts` | `s039-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonMember` | `s040.ts` | `s040-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayFromValue` | `s041.ts` | `s041-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ArrayMember` | `s042.ts` | `s042-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MapGroupByValue` | `s043.ts` | `s043-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MapSetMember` | `s044.ts` | `s044-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MathMemberWrite` | `s045-b.ts` | `s045.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MathMethodValue` | `s046.ts` | `s046-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MathMember` | `s047-c.ts` | `s047.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MathCallCount` | `s048-b.ts` | `s048.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `NumberMemberWrite` | `s049-b.ts` | `s049.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `NumberStaticValue` | `s050.ts` | `s050-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `NumberMember` | `s051.ts` | `s051-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `NumberGlobalCount` | `s053.ts` | `s053-old.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `RegexExec` | `s054.ts` | `s054-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `StringPatternSpread` | `s055-b.ts` | `s055.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `StringSearchPattern` | `s056.ts` | `s056-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateStaticWrite` | `s057.ts` | `s057-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateStaticValue` | `s058.ts` | `s058-c.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateNewSpread` | `s059-b.ts` | `s059.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `DateMember` | `s060-b.ts` | `s060.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `HalfFloatUnary` | `s061.ts` | `s061-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonStaticMember` | `s062-c.ts` | `s062.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonStringifyCount` | `s063.ts` | `s063-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonStringifySpread` | `s064-b.ts` | `s064.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonStringifyDomain` | `s065.ts` | `s065-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonParseCount` | `s067.ts` | `s067-old.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonParseSpread` | `s068-b.ts` | `s068.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonParseTarget` | `s070.ts` | `s070-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonParseDomain` | `s071.ts` | `s071-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `JsonError` | `s073.ts` | `s073-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ForOfEntries` | `s074.ts` | `s074-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ForOfMap` | `s076.ts` | `s076-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ForOfUserClass` | `s077.ts` | `s077-old.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `ForOfSubject` | `s078-accepted.ts` | `s078.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `RegexMatchType` | `s079.ts` | `s079-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `MapSetTypeKey` | `s080.ts` | `s080-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `HalfFloatUpdate` | `s081.ts` | `s081-c.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |
| `HalfFloatBinary` | `s082.ts` | `s082-b.ts` | S014: §153 requires Diverges even for a known TypeScript error. Raw arity/kind facts may exist, but a TscRejects partition would violate that explicit invariant; rejected probes must still carry the block. |

Additional review controls cover standalone `export =`, empty/one-arm/both-arm/throwing descriptor constructors, reference/nested-descriptor/array/Q32 alias fields and assigned/unassigned switch controls, and a read after both-arm assignment. Constructor-read variants use `ConstructorFieldReadWithAssignmentFact`; ordinary reads use no block. The generic matrix measured 7,694 omitted instances (three fewer than the previous count), and its full TypeScript comparison passes with that measured set.

### Rename map

Site and divergence names now state guards rather than source line numbers. The enum scan has zero `[A-Za-z][0-9]{2,}` matches; the regression additionally rejects single digits in these names. API row payloads such as F16 remain semantic API identifiers, not source-line names. The map below composes intermediate renames to their final names.

| Old name | Final name |
| --- | --- |
| `ArgumentCountExprCall1066` | `GeneratorNextArgumentCount` |
| `ArgumentCountExprCall1113` | `AsyncMethodArgumentCount` |
| `ArgumentCountExprCall1129` | `InstanceMethodArgumentCount` |
| `ArgumentCountExprCall1529` | `ValueConstructorArgumentCount` |
| `ArgumentCountExprCall1605` | `ReferenceConstructorArgumentCount` |
| `ArgumentCountExprCall205` | `SourceFunctionArgumentCount` |
| `ArgumentCountExprCall229` | `ForeignFunctionArgumentCount` |
| `ArgumentCountExprCall255` | `ContextMethodArgumentCount` |
| `ArgumentCountExprCall282` | `AmbientFunctionArgumentCount` |
| `ArgumentCountExprCall539` | `FunctionValueArgumentCount` |
| `ArgumentCountExprCall846` | `ScalarToStringArgumentCount` |
| `ArgumentCountExprCall907` | `InboxWaitArgumentCount` |
| `ArgumentCountExprCall936` | `OutboxPostArgumentCount` |
| `ArgumentCountExprCall966` | `FixedArrayPushArgumentCount` |
| `ArgumentCountExprCall979` | `ArrayPushArgumentCount` |
| `ArgumentCountExprCall983` | `ArrayPopArgumentCount` |
| `ArgumentCountExprEntry566` | `AwaitMethodArgumentCount` |
| `ArgumentCountExprEntry652` | `AwaitFunctionArgumentCount` |
| `ArgumentCountExprMethod1120` | `MapGetArgumentCount` |
| `ArgumentCountExprMethod113` | `NumericMethodCheckedArgumentCount` |
| `ArgumentCountExprMethod1134` | `MapGetOrArgumentCount` |
| `ArgumentCountExprMethod1139` | `MapSetArgumentCount` |
| `ArgumentCountExprMethod1147` | `MapHasArgumentCount` |
| `ArgumentCountExprMethod1161` | `MapClearArgumentCount` |
| `ArgumentCountExprMethod1235` | `SetAddOrHasArgumentCount` |
| `ArgumentCountExprMethod1244` | `SetClearArgumentCount` |
| `ArgumentCountExprMethod1299` | `SetDeleteArgumentCount` |
| `ArgumentCountExprMethod1410` | `SetBinaryArgumentCount` |
| `ArgumentCountExprMethod183` | `FixedArrayAtArgumentCount` |
| `ArgumentCountExprMethod328` | `ArrayAtArgumentCount` |
| `ArgumentCountExprMethod347` | `ArraySearchArgumentCount` |
| `ArgumentCountExprMethod388` | `ArrayJoinArgumentCount` |
| `ArgumentCountExprMethod413` | `ArraySliceArgumentCount` |
| `ArgumentCountExprMethod438` | `ArrayFillArgumentCount` |
| `ArgumentCountExprMethod450` | `ArrayReverseArgumentCount` |
| `ArgumentCountExprMethod458` | `ArrayConcatCallArgumentCount` |
| `ArgumentCountExprMethod491` | `ArraySpliceArgumentCount` |
| `ArgumentCountExprMethod499` | `ArrayShiftArgumentCount` |
| `ArgumentCountExprMethod521` | `ArrayUnshiftArgumentCount` |
| `ArgumentCountExprMethod547` | `ArrayCopyWithinArgumentCount` |
| `ArgumentCountExprNamespace1113` | `DateUtcArgumentCount` |
| `ArgumentCountExprNamespace1134` | `DateNowArgumentCount` |
| `ArgumentCountExprNamespace1225` | `NumberConversionArgumentCount` |
| `ArgumentCountExprNamespace1233` | `BigIntConversionArgumentCount` |
| `ArgumentCountExprNamespace404` | `MathMethodArgumentCount` |
| `ArgumentCountExprNamespace480` | `NumberPredicateArgumentCount` |
| `ArgumentCountExprNamespace541` | `GlobalNumberArgumentCount` |
| `ArgumentCountExprNamespace812` | `RegexConstructorArgumentCount` |
| `ArgumentCountExprNamespace840` | `RegexTestArgumentCount` |
| `ArgumentCountExprNamespace878` | `RegexExecArgumentCount` |
| `ArgumentCountText56` | `UriCodecArgumentCount` |
| `Bodies123Form` | `ModuleVarDeclarationForm` |
| `Bodies158Form` | `ModuleInitializerMissingForm` |
| `CheckBindings118` | `BindingPatternSourceKind` |
| `CheckBindings45` | `DuplicateLocalDeclaration` |
| `CheckBindings95` | `IteratorBindingUnsupportedMembers` |
| `CheckBindings97` | `IteratorBindingUnknownMembers` |
| `CheckBodies123` | `ModuleVarDeclaration` |
| `CheckBodies158` | `ModuleInitializerMissing` |
| `CheckBodies236` | `AsyncHandleUnawaited` |
| `CheckBodies261` | `FunctionReturnCoverage` |
| `CheckBodies703` | `FieldDefiniteAssertionUnassigned` |
| `CheckBodies724` | `FieldAssignmentNestedEveryNormalExit` |
| `CheckBodies839` | `ConstructorThisBeforeFieldValues` |
| `CheckCapture437` | `CaptureEffectEscapes` |
| `CheckCapture448` | `CaptureEffectArgument` |
| `CheckClassShape142` | `DescriptorDisposeMethod` |
| `CheckClassShape178` | `AsyncStaticMethod` |
| `CheckClassShape187` | `ValueClassDisposeMethod` |
| `CheckClassShape215` | `ReadAccessorParameters` |
| `CheckClassShape252` | `ValueClassWriteAccessor` |
| `CheckClassShape270` | `WriteAccessorReturnAnnotation` |
| `CheckClassShape278` | `WriteAccessorParameterCount` |
| `CheckClassShape288` | `WriteAccessorDefaultParameter` |
| `CheckClassShape33` | `DuplicateReadAccessor` |
| `CheckClassShape347` | `AsyncGeneratorMethod` |
| `CheckClassShape354` | `GeneratorMethodDeclaration` |
| `CheckClassShape364` | `ValueClassAsyncMethod` |
| `CheckClassShape379` | `MethodBodyMissing` |
| `CheckClassShape44` | `DuplicateWriteAccessor` |
| `CheckClassShape454` | `ValueClassInheritance` |
| `CheckClassShape475` | `ComputedFieldDeclaration` |
| `CheckClassShape520` | `StaticFieldAnnotationMissing` |
| `CheckClassShape561` | `DescriptorOptionalDefaultMissing` |
| `CheckClassShape570` | `DescriptorRequiredInitializer` |
| `CheckClassShape612` | `InstanceFieldAnnotationMissing` |
| `CheckClassShape644` | `DescriptorOptionalInitializerMissing` |
| `CheckClassShape732` | `ConstructorParameterProperty` |
| `CheckClassShape791` | `IndexSignatureParameterAnnotationMissing` |
| `CheckClassShape800` | `IndexSignatureParameterKind` |
| `CheckClassShape824` | `IndexSignatureElementAnnotationMissing` |
| `CheckClassShape909` | `IndexSignatureGetterMismatch` |
| `CheckDeclarations255` | `GenericClassStaticMember` |
| `CheckDeclarations284` | `GenericClassGenericMethod` |
| `CheckDeclarations361` | `SourceFunctionBodyMissing` |
| `CheckDeclarations403` | `DuplicateTypeParameter` |
| `CheckDeclarations436` | `ModuleBindingPattern` |
| `CheckDeclarations461` | `EnumStringMemberName` |
| `CheckDeclarations489` | `EnumNonIntegerMember` |
| `CheckDeclarations518` | `GenericSourceAlias` |
| `CheckDeclarations530` | `SourceAliasNotLiteralUnion` |
| `CheckDeclarations538` | `LiteralAliasDiscriminantLimit` |
| `CheckDeclarations550` | `DuplicateLiteralAliasMember` |
| `CheckDeclarations585` | `WireAliasDiscriminantLimit` |
| `CheckDeclarations619` | `DuplicateWireAliasMember` |
| `CheckDeclarations640` | `WireEnumMemberNonIntegerSyntax` |
| `CheckDeclarations649` | `WireEnumMemberNonIntegerValue` |
| `CheckDeclarations662` | `WireEnumValueRange` |
| `CheckDeclarations674` | `DuplicateWireEnumValue` |
| `CheckDeclarations68` | `DefaultExportDeclaration` |
| `CheckDeclarations749` | `MirrorModuleDeclaration` |
| `CheckException218` | `ErrorConstructorTypeArguments` |
| `CheckException328` | `CatchBindingUnnarrowedUse` |
| `CheckException356` | `ThrowOperandNotErrorFamily` |
| `CheckException382` | `FinallyClause` |
| `CheckException448` | `CatchBindingPattern` |
| `CheckException464` | `CatchBindingAnnotation` |
| `CheckException511` | `InstanceofRightNotErrorFamily` |
| `CheckException526` | `InstanceofLeftNotErrorFamily` |
| `CheckExports101` | `ExportSourceModuleMissing` |
| `CheckExports114` | `ExportSpecifierKind` |
| `CheckExports126` | `TypeOnlyOrDefaultExportSpecifier` |
| `CheckExports139` | `TypeOnlyImportReexport` |
| `CheckExports162` | `DuplicateAliasedExport` |
| `CheckExports292` | `ExportAliasCycle` |
| `CheckExports313` | `ReexportSpecifierKind` |
| `CheckExports327` | `ExportLocalMissing` |
| `CheckExports370` | `ReexportMemberMissing` |
| `CheckExports44` | `DuplicateDirectExport` |
| `CheckExports81` | `TypeOnlyExportDeclaration` |
| `CheckExprAggregate102` | `EmptyArrayInference` |
| `CheckExprAggregate155` | `DescriptorLiteralSpread` |
| `CheckExprAggregate165` | `DescriptorLiteralQuotedKey` |
| `CheckExprAggregate184` | `DescriptorLiteralAccessor` |
| `CheckExprAggregate194` | `DescriptorLiteralDuplicateMember` |
| `CheckExprAggregate202` | `DescriptorLiteralUnknownMember` |
| `CheckExprArrayOfAndMapCopy31` | `ArrayOfTypeArgumentCount` |
| `CheckExprArrayOfAndMapCopy87` | `MapCopyTypeArgumentCount` |
| `CheckExprAssign107` | `ReadonlyIndexAssignment` |
| `CheckExprAssign176` | `StaticAccessorAssignmentExpressionValue` |
| `CheckExprAssign192` | `ReadonlyStaticAccessorAssignment` |
| `CheckExprAssign200` | `StaticSetterParameterMissing` |
| `CheckExprAssign260` | `AccessorAssignmentExpressionValue` |
| `CheckExprAssign272` | `ReadonlyAccessorAssignment` |
| `CheckExprAssign280` | `SetterParameterMissing` |
| `CheckExprAssign34` | `UnsignedShiftAssignment` |
| `CheckExprAssign41` | `LogicalOrPowerAssignment` |
| `CheckExprAssign445` | `ConstLocalAssignment` |
| `CheckExprAssign466` | `ImportedBindingAssignment` |
| `CheckExprAssign478` | `ConstGlobalAssignment` |
| `CheckExprAssign491` | `NonVariableAssignmentTarget` |
| `CheckExprAssign500` | `NonPlaceAssignmentTarget` |
| `CheckExprAssign52` | `DestructuringAssignment` |
| `CheckExprAssign598` | `PrivateMemberAssignment` |
| `CheckExprAssign632` | `ConstStaticFieldAssignment` |
| `CheckExprAssign98` | `IndexAssignmentExpressionValue` |
| `CheckExprCall101` | `ClassCalledWithoutNew` |
| `CheckExprCall1044` | `CoroutineStepLayoutLimit` |
| `CheckExprCall1067` | `CoroutineReturnOrThrowCall` |
| `CheckExprCall109` | `EnumCalled` |
| `CheckExprCall1110` | `InstanceStaticMethodCall` |
| `CheckExprCall117` | `MirrorTypeAliasCalled` |
| `CheckExprCall125` | `LiteralAliasCalled` |
| `CheckExprCall1300` | `ConstructorNotNamedClass` |
| `CheckExprCall1310` | `LocalValueConstructed` |
| `CheckExprCall1319` | `DynamicFunctionConstructed` |
| `CheckExprCall1328` | `PromiseConstructed` |
| `CheckExprCall1339` | `WorkerEndpointConstructed` |
| `CheckExprCall137` | `DynamicEvaluatorCalled` |
| `CheckExprCall1385` | `ContainerConstructorTypeArgumentsMissing` |
| `CheckExprCall1394` | `ContainerConstructorTypeArgumentCount` |
| `CheckExprCall1454` | `SetConstructorSourceCount` |
| `CheckExprCall1480` | `NonGenericConstructorTypeArguments` |
| `CheckExprCall1541` | `OpaqueHandleConstructed` |
| `CheckExprCall1558` | `AmbientClassConstructed` |
| `CheckExprCall1570` | `DescriptorClassConstructed` |
| `CheckExprCall163` | `UnreachableExpressionValue` |
| `CheckExprCall174` | `UnknownFunctionName` |
| `CheckExprCall220` | `GeneratorYieldTypeNotKnown` |
| `CheckExprCall25` | `SpreadCallArgument` |
| `CheckExprCall394` | `ContextByteTargetKind` |
| `CheckExprCall425` | `ContextByteTargetLayout` |
| `CheckExprCall629` | `NonGenericStaticMethodTypeArguments` |
| `CheckExprCall660` | `PromiseCombinatorCall` |
| `CheckExprCall669` | `PromiseStaticCall` |
| `CheckExprCall67` | `NonGenericFunctionTypeArguments` |
| `CheckExprCall775` | `GenericMethodTypeArgumentsMissing` |
| `CheckExprCall818` | `ValueClassMethodMissing` |
| `CheckExprCall841` | `ToStringTypeArguments` |
| `CheckExprCall966` | `ToStringArgumentCount` |
| `CheckExprEntry167` | `VoidExpressionValue` |
| `CheckExprEntry199` | `DescriptorDefaultThisUse` |
| `CheckExprEntry215` | `FieldInitializerThisUse` |
| `CheckExprEntry243` | `ThisOutsideMethod` |
| `CheckExprEntry270` | `NominalObjectLiteral` |
| `CheckExprEntry281` | `ObjectLiteralWithoutDescriptorContext` |
| `CheckExprEntry296` | `NonNullAssertionExpression` |
| `CheckExprEntry304` | `FunctionExpression` |
| `CheckExprEntry373` | `EmbeddedHeaderCopied` |
| `CheckExprEntry444` | `AwaitOutsideAsync` |
| `CheckExprEntry478` | `AwaitNotDirectCall` |
| `CheckExprEntry495` | `ContextSuspendArguments` |
| `CheckExprEntry579` | `AwaitFunctionTypeArguments` |
| `CheckExprEntry661` | `AwaitMethodTypeArguments` |
| `CheckExprLambda193` | `BlockLambdaReturnAnnotationMissing` |
| `CheckExprLambda211` | `LambdaReturnCoverage` |
| `CheckExprLambda47` | `AsyncArrowFunction` |
| `CheckExprLambda56` | `GeneratorArrowFunction` |
| `CheckExprLiteral130` | `BigIntLiteral` |
| `CheckExprLiteral223` | `IntegerLiteralRange` |
| `CheckExprLiteral270` | `TemplateInterpolationKind` |
| `CheckExprLiteral333` | `UnknownValueName` |
| `CheckExprLiteral394` | `GenericFunctionValue` |
| `CheckExprLiteral413` | `EnumObjectValue` |
| `CheckExprLiteral421` | `MirrorTypeAliasValue` |
| `CheckExprLiteral429` | `LiteralAliasValue` |
| `CheckExprLiteral437` | `ForeignFunctionValue` |
| `CheckExprLiteral457` | `DynamicEvaluatorValue` |
| `CheckExprLiteral531` | `AmbientFunctionValue` |
| `CheckExprLiteral55` | `RegexLiteralUnicodeSetsFlag` |
| `CheckExprLiteral554` | `UnknownAmbientValueName` |
| `CheckExprMember124` | `DescriptorAbsentMemberRead` |
| `CheckExprMember135` | `PrivateMemberRead` |
| `CheckExprMember190` | `ArrayIndexNotInt` |
| `CheckExprMember201` | `FixedArrayIndexNotInt` |
| `CheckExprMember209` | `FixedArrayConstantIndexBounds` |
| `CheckExprMember221` | `NonIndexableReceiver` |
| `CheckExprMember253` | `ValueClassMemberMissing` |
| `CheckExprMember261` | `InstancePrototypeMember` |
| `CheckExprMember329` | `InstanceStaticMemberRead` |
| `CheckExprMember397` | `ArrayMethodValue` |
| `CheckExprMember408` | `FixedArrayMethodValue` |
| `CheckExprMember445` | `MapMethodValue` |
| `CheckExprMember473` | `SetMethodValue` |
| `CheckExprMember501` | `StringMethodValue` |
| `CheckExprMember550` | `GeneratorResultDoneWrite` |
| `CheckExprMember556` | `GeneratorResultValueWrite` |
| `CheckExprMember75` | `FieldInitializerSelfRead` |
| `CheckExprMethod1255` | `SetBinaryRequiredArgumentCount` |
| `CheckExprMethod1544` | `CollectionCallbackTypeMismatch` |
| `CheckExprMethod464` | `ArraySpliceRequiredArgument` |
| `CheckExprMethod520` | `ArrayCopyWithinRequiredArgumentCount` |
| `CheckExprMethod553` | `ArraySortArgumentCount` |
| `CheckExprMethod583` | `ArrayReduceArgumentCount` |
| `CheckExprMethod736` | `ArrayMapVoidCallback` |
| `CheckExprMethod809` | `MapGroupByArgumentCount` |
| `CheckExprMethod853` | `MapGroupByNonFunctionCallback` |
| `CheckExprNamespace155` | `NamespaceClassPrototypeRead` |
| `CheckExprNamespace167` | `NamespaceConstFieldWrite` |
| `CheckExprNamespace205` | `StaticMethodValue` |
| `CheckExprNamespace221` | `NamespacePrototypeRead` |
| `CheckExprNamespace249` | `EnumStaticMemberMissing` |
| `CheckExprNamespace259` | `MirrorAliasStaticValue` |
| `CheckExprNamespace267` | `LiteralAliasStaticValue` |
| `CheckExprNamespace281` | `GlobalPrototypeRead` |
| `CheckExprNamespace516` | `NumberParserIdentityMismatch` |
| `CheckExprNamespace633` | `WorkerMessageNotTransferable` |
| `CheckExprNamespace675` | `WorkerEntryNotNamedFunction` |
| `CheckExprNamespace716` | `WorkerEntryAsync` |
| `CheckExprNamespace758` | `WorkerSpawnTypeArgumentCount` |
| `CheckExprNamespace815` | `RegexConstructorTypeArguments` |
| `CheckExprNamespace929` | `ReplaceAllRegexNotGlobal` |
| `CheckExprNamespace944` | `StringStaticMethodArgumentCount` |
| `CheckExprOperator1286` | `BinaryMixedNumericTypes` |
| `CheckExprOperator1382` | `ConditionalNonBooleanCondition` |
| `CheckExprOperator1488` | `YieldDelegation` |
| `CheckExprOperator201` | `IndexUpdateExpressionValue` |
| `CheckExprOperator220` | `AccessorUpdateExpressionValue` |
| `CheckExprOperator239` | `UpdateNonNumericTarget` |
| `CheckExprOperator276` | `ReadonlyIndexUpdate` |
| `CheckExprOperator346` | `ReadonlyAccessorUpdate` |
| `CheckExprOperator378` | `UpdateSetterParameterMissing` |
| `CheckExprOperator456` | `LogicalNonBooleanOperand` |
| `CheckExprOperator537` | `OptionalCallValueWithoutFallback` |
| `CheckExprOperator562` | `OptionalMemberValueWithoutFallback` |
| `CheckExprOperator607` | `OptionalComputedMember` |
| `CheckExprOperator733` | `OptionalPrivateMember` |
| `CheckExprOperator82` | `LogicalNotNonBoolean` |
| `CheckGenerics283` | `GenericFunctionTypeArgumentCount` |
| `CheckGenerics375` | `GenericMethodTypeArgumentCount` |
| `CheckGenerics492` | `GenericClassTypeArgumentCount` |
| `CheckGenerics580` | `DeferredInstanceArgumentsMissing` |
| `CheckGenerics588` | `DeferredInstanceTemplateMissing` |
| `CheckGenerics78` | `GenericConstraintCycle` |
| `CheckHostEntries126` | `HostEntrySignatureMismatch` |
| `CheckInference130` | `GenericInferenceNoCandidate` |
| `CheckInference149` | `GenericInferenceConflictingCandidates` |
| `CheckInitEffectsFailure757` | `InitializerSegmentRange` |
| `CheckInitEffectsFailure764` | `GlobalInitializerOwnerRange` |
| `CheckInstanceChain236` | `GenericInstanceExpansionLimit` |
| `CheckJsonFailure1028` | `JsonValidatorTypeKind` |
| `CheckJsonFailure1267` | `JsonConstructorTypeKind` |
| `CheckJsonFailure1305` | `JsonArrayConstructorTypeKind` |
| `CheckJsonFailure1376` | `JsonArrayStoreTypeKind` |
| `CheckJsonFailure636` | `JsonSerializerTypeKind` |
| `CheckLayout264` | `ClassAlignmentBelowNatural` |
| `CheckLayout304` | `ClassFieldLayoutLimit` |
| `CheckLayout413` | `SuspendFrameMemberLayoutLimit` |
| `CheckLayout501` | `AsyncChildFrameLayoutLimit` |
| `CheckLayout517` | `GeneratorFrameFinalAlignmentLimit` |
| `CheckLayout536` | `ClosureEnvironmentLayoutLimit` |
| `CheckLayout865` | `StoredAggregateLayoutLimit` |
| `CheckLookup127` | `SwitchCaseWriteOutsideDeclaration` |
| `CheckLookup156` | `BlockPendingReadWithoutProgramShadow` |
| `CheckLookup19` | `TypeOnlyImportValueUse` |
| `CheckMod923` | `InitializerNonPlaceOptionalReceiver` |
| `CheckSignatures119` | `ForeignFunctionHeaderMissing` |
| `CheckSignatures187` | `TypeOnlyNamespaceImport` |
| `CheckSignatures213` | `NamespaceImportTargetMissing` |
| `CheckSignatures263` | `NamedImportModuleMissing` |
| `CheckSignatures317` | `NamedImportMemberMissing` |
| `CheckSignatures373` | `ModuleVariableAnnotationMissing` |
| `CheckSignatures382` | `WorkerEndpointModuleGlobal` |
| `CheckSignatures478` | `FunctionReturnAnnotationMissing` |
| `CheckSignatures519` | `NamedParameterAnnotationMissing` |
| `CheckSignatures548` | `PatternParameterAnnotationMissing` |
| `CheckStmt1019` | `ForOfBindingKind` |
| `CheckStmt1028` | `ForOfBindingCount` |
| `CheckStmt1037` | `ForOfBindingInitializer` |
| `CheckStmt1101` | `IteratorMethodArgumentCount` |
| `CheckStmt1238` | `LiteralAliasDuplicateCase` |
| `CheckStmt1249` | `LiteralAliasUnknownCase` |
| `CheckStmt1298` | `SwitchDiscriminantKind` |
| `CheckStmt1428` | `LiteralAliasSwitchCoverage` |
| `CheckStmt197` | `LambdaUsingDeclaration` |
| `CheckStmt241` | `LabeledBreak` |
| `CheckStmt244` | `BreakOutsideLoopOrSwitch` |
| `CheckStmt261` | `LabeledContinue` |
| `CheckStmt268` | `ContinueOutsideLoop` |
| `CheckStmt311` | `LocalVarDeclaration` |
| `CheckStmt324` | `AwaitUsingDeclaration` |
| `CheckStmt359` | `LocalInitializerMissing` |
| `CheckStmt475` | `GeneratorReturnValue` |
| `CheckStmt484` | `VoidFunctionReturnValue` |
| `CheckStmt516` | `ReturnValueMissing` |
| `CheckStmt535` | `StatementNonBooleanCondition` |
| `CheckStmt788` | `AsyncForOf` |
| `CheckStmt994` | `ForOfVarBinding` |
| `CheckText49` | `UriCodecTypeArguments` |
| `CheckTypeRules292` | `DistinctNominalClassAssignment` |
| `CheckTypeRules299` | `DistinctNominalContainerAssignment` |
| `CheckTypeRules302` | `ImplicitNumericAssignment` |
| `CheckTyres106` | `VoidTypeOutsideResult` |
| `CheckTyres123` | `AnyTypeAnnotation` |
| `CheckTyres134` | `UnknownTypeAnnotation` |
| `CheckTyres143` | `UndefinedKeywordAnnotation` |
| `CheckTyres169` | `ObjectTypeOutsideBoundary` |
| `CheckTyres193` | `QualifiedSourceTypeName` |
| `CheckTyres214` | `WorkerTypeArgumentsMissing` |
| `CheckTyres224` | `WorkerTypeArgumentCount` |
| `CheckTyres267` | `RegExpTypeArguments` |
| `CheckTyres283` | `PromiseTypeArgumentMissing` |
| `CheckTyres291` | `PromiseTypeArgumentCount` |
| `CheckTyres303` | `FixedArrayTypeArgumentsMissing` |
| `CheckTyres311` | `FixedArrayTypeArgumentCount` |
| `CheckTyres390` | `ArrayTypeArgumentCount` |
| `CheckTyres403` | `GeneratorYieldTypeMissing` |
| `CheckTyres419` | `ErrorTypeArguments` |
| `CheckTyres430` | `MapTypeArgumentsMissing` |
| `CheckTyres439` | `MapTypeArgumentCount` |
| `CheckTyres495` | `DateTypeArguments` |
| `CheckTyres512` | `NonGenericClassTypeArguments` |
| `CheckTyres518` | `GenericClassTypeArgumentsMissing` |
| `CheckTyres551` | `LiteralAliasTypeArguments` |
| `CheckTyres562` | `TypeNameUnknown` |
| `CheckTyres581` | `IntersectionTypeAnnotation` |
| `CheckTyres593` | `UndefinedUnionMember` |
| `CheckTyres673` | `ConstructorTypeAnnotation` |
| `CheckTyres688` | `FunctionTypeParameterAnnotationMissing` |
| `ClassShape354Form` | `GeneratorMethodForm` |
| `ClassShape520Form` | `StaticFieldAnnotationMissingForm` |
| `ClassShape612Form` | `InstanceFieldAnnotationMissingForm` |
| `ClassShape732Form` | `ConstructorParameterPropertyForm` |
| `ConstructorDefiniteFieldReadBeforeAssignment` (variant) | `ConstructorFieldReadWithAssignmentFact` |
| `ConstructorFieldReadBeforeAssignment` (site) | `ConstructorDefiniteFieldReadBeforeAssignment` |
| `ConstructorFieldReadBeforeAssignment` (variant) | `ConstructorFieldReadWithAssignmentFact` |
| `Declarations461Form` | `EnumStringMemberNameForm` |
| `Declarations518Form` | `GenericSourceAliasForm` |
| `Declarations530Form` | `SourceAliasNotLiteralUnionForm` |
| `Declarations550Form` | `DuplicateLiteralAliasMemberForm` |
| `Declarations749Form` | `MirrorModuleDeclarationForm` |
| `DescriptorRequiredFieldWithoutConstructor` | `DescriptorRequiredFieldUnassigned` |
| `ExportAssignmentWithOtherExports` | `ExportAssignmentEsModule` |
| `ExprAggregate102Form` | `EmptyArrayInferenceForm` |
| `ExprAggregate155Form` | `DescriptorLiteralSpreadForm` |
| `ExprAggregate165Form` | `DescriptorLiteralQuotedKeyForm` |
| `ExprAggregate184Form` | `DescriptorLiteralAccessorForm` |
| `ExprAssign41Form` | `LogicalOrPowerAssignmentForm` |
| `ExprAssign500Form` | `NonPlaceAssignmentTargetForm` |
| `ExprAssign598Form` | `PrivateMemberAssignmentForm` |
| `ExprCall1067Form` | `CoroutineReturnOrThrowCallForm` |
| `ExprCall1300Form` | `ConstructorNotNamedClassForm` |
| `ExprCall220Form` | `GeneratorYieldTypeNotKnownForm` |
| `ExprEntry243Form` | `ThisOutsideMethodForm` |
| `ExprEntry296Form` | `NonNullAssertionExpressionForm` |
| `ExprEntry304Form` | `FunctionExpressionForm` |
| `ExprLambda193Form` | `BlockLambdaReturnAnnotationMissingForm` |
| `ExprLiteral270Form` | `TemplateInterpolationKindForm` |
| `ExprLiteral413Form` | `EnumObjectValueForm` |
| `ExprLiteral437Form` | `ForeignFunctionValueForm` |
| `ExprLiteral531Form` | `AmbientFunctionValueForm` |
| `ExprMember135Form` | `PrivateMemberReadForm` |
| `ExprMember190Form` | `ArrayIndexNotIntForm` |
| `ExprMember201Form` | `FixedArrayIndexNotIntForm` |
| `ExprMember209Form` | `FixedArrayConstantIndexBoundsForm` |
| `ExprMember221Form` | `NonIndexableReceiverForm` |
| `ExprMember397Form` | `ArrayMethodValueForm` |
| `ExprMember408Form` | `FixedArrayMethodValueForm` |
| `ExprMember445Form` | `MapMethodValueForm` |
| `ExprMember473Form` | `SetMethodValueForm` |
| `ExprMember501Form` | `StringMethodValueForm` |
| `ExprMember550Form` | `GeneratorResultDoneWriteForm` |
| `ExprMember556Form` | `GeneratorResultValueWriteForm` |
| `ExprMethod1544Form` | `CollectionCallbackTypeMismatchForm` |
| `ExprNamespace205Form` | `StaticMethodValueForm` |
| `ExprNamespace249Form` | `EnumStaticMemberMissingForm` |
| `ExprOperator1382Form` | `ConditionalNonBooleanConditionForm` |
| `ExprOperator1488Form` | `YieldDelegationForm` |
| `ExprOperator456Form` | `LogicalNonBooleanOperandForm` |
| `ExprOperator82Form` | `LogicalNotNonBooleanForm` |
| `FieldAssignmentAfterReturn` | `FieldAssignmentAfterUnreachableReturn` |
| `FieldAssignmentMissing` | `FieldAssignmentMissingNoNormalExit` |
| `FieldAssignmentNested` | `FieldAssignmentNestedEveryNormalExit` |
| `Float16Binary` | `HalfFloatBinary` |
| `Float16LiteralRange` | `HalfFloatLiteralRange` |
| `Float16Unary` | `HalfFloatUnary` |
| `Float16Update` | `HalfFloatUpdate` |
| `Lib147` | `SourceFilesEmpty` |
| `NestedFieldAssignment` | `NestedFieldAssignmentEveryNormalExit` |
| `NullableCallForm` | `NullableCallNonNullInitializer` |
| `NullableMemberForm` | `NullableMemberNonNullInitializer` |
| `NullableNominalAssignment` (variant) | `NullableNominalAssignmentNonNullInitializer` |
| `Parse237` | `ParserLoneSurrogateEscape` |
| `Parse245` | `ParserSyntaxError` |
| `PatternArrayRestPattern4` | `ArrayBindingRestElement` |
| `PatternNestedPattern1` | `BindingPatternUnsupportedRoot` |
| `PatternNestedPattern2` | `ArrayBindingNestedPattern` |
| `PatternNestedPattern3` | `ObjectBindingNestedPattern` |
| `PatternObjectRestPattern9` | `ObjectBindingRestProperty` |
| `PatternPatternDefaultValue5` | `ArrayBindingDefaultValue` |
| `PatternPatternDefaultValue6` | `ObjectBindingShorthandDefault` |
| `PatternPatternDefaultValue7` | `ObjectBindingFieldDefault` |
| `PatternPatternFieldName8` | `ObjectBindingNonLiteralFieldName` |
| `ProvenanceInvalidUtf8` | `ProvenanceInvalidUnicodeEncoding` |
| `RegexFailure18` | `RegexUnsupportedFlag` |
| `RegexFailure25` | `RegexDuplicateFlag` |
| `RegexFailure33` | `RegexUnicodeFlagsConflict` |
| `Signatures213Form` | `NamespaceImportTargetMissingForm` |
| `Signatures263Form` | `NamedImportModuleMissingForm` |
| `Signatures373Form` | `ModuleVariableAnnotationMissingForm` |
| `Signatures478Form` | `FunctionReturnAnnotationMissingForm` |
| `Signatures519Form` | `NamedParameterAnnotationMissingForm` |
| `Signatures548Form` | `PatternParameterAnnotationMissingForm` |
| `Stmt1019Form` | `ForOfBindingKindForm` |
| `Stmt1298Form` | `SwitchDiscriminantKindForm` |
| `Stmt311Form` | `LocalVarDeclarationForm` |
| `Stmt359Form` | `LocalInitializerMissingForm` |
| `Stmt535Form` | `StatementNonBooleanConditionForm` |
| `Stmt788Form` | `AsyncForOfForm` |
| `Stmt994Form` | `ForOfVarBindingForm` |
| `StorageOnlyFloat16` | `StorageOnlyHalfFloat` |
| `SwitchCaseReadBeforeDeclaration` | `SwitchCaseWriteOutsideDeclaration` |
| `SwitchCaseWriteBeforeDeclaration` | `BlockPendingReadWithoutProgramShadow` |
| `TypeRules299Form` | `DistinctNominalContainerAssignmentForm` |
| `Tyres193Form` | `QualifiedSourceTypeNameForm` |
| `Tyres403Form` | `GeneratorYieldTypeMissingForm` |
| `Tyres581Form` | `IntersectionTypeAnnotationForm` |
| `Tyres673Form` | `ConstructorTypeAnnotationForm` |

### Verification and cost

- `cargo test --offline --locked -p subscript-compiler`: 931 passed, 0 failed, 1 existing ignored; 54 suites including doc tests. Collision corpus index and the restored first-diagnostic harness pass.
- `cargo test --offline --locked -p subscript-codegen`: 723 passed, 0 failed, 1 existing ignored; 50 suites including doc tests. No golden was changed.
- `cargo fmt --check`: pass.
- `cargo clippy --offline --locked --workspace --all-targets`: pass; compiler/runtime/codegen library warnings are **5/18/13**, below or equal to `tools/gate.sh` baselines **7/18/13**. No new helper warning remains. `tools/gate.sh` was not run.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference`: pass; generated docs regenerated through the generator.
- Final independent Phase Review: CRITICAL 0, MAJOR 0, MINOR 0. `tools/hygiene.sh` and `git diff --check`: pass.
- Every changed Rust file stays at or below 2,000 lines; maximum is `compiler/src/check/rejection.rs` at 1,987. Test-only modules remain scoped by `cfg(test)`.
- Rule 4: **1,234 witnesses, 459 carried variants, Red 0**. Warm batch: **1.154 s** (tsc **0.547 s**, checker **0.375 s**). A second warm run while codegen engine tests were executing took **2.016 s** (tsc **0.931 s**, checker **0.709 s**); both passed. The source comment records the approximate normal warm cost of 1.15 s.
- Site and variant enum grep: **0** matches of `[A-Za-z][0-9]{2,}`; the single-digit regression also passes.

### Every cumulative changed file

The following list includes preserved round 1–5 files, round 6 edits and generated files. `specs/blocks/collisions.md` is the caller's preserved input, not an agent edit. Restored entries above are identical to HEAD and therefore no longer appear in the cumulative diff.

- `compiler/src/ambient.rs`
- `compiler/src/ambient/lib_names.rs`
- `compiler/src/api_reference.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/container_argument.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/exports.rs`
- `compiler/src/check/expr.rs`
- `compiler/src/check/expr/aggregate.rs`
- `compiler/src/check/expr/array_of_and_map_copy.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/expr/namespace.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/field_initializer.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/host_entries.rs`
- `compiler/src/check/inference.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/instance_chain.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/layout.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mirror_provenance.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/namespace_import.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/opaque.rs`
- `compiler/src/check/pattern.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_facts.rs`
- `compiler/src/check/rejection_programs.rs`
- `compiler/src/check/rejection_programs_s154.txt`
- `compiler/src/check/rejection_regressions.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets_s154.txt`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_tsc.cjs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/divergence/established.rs`
- `compiler/src/divergence/established_tail.rs`
- `compiler/src/divergence/group_a.rs`
- `compiler/src/divergence/round_five.rs`
- `compiler/src/divergence/round_five_splits.rs`
- `compiler/src/hir/host_entry.rs`
- `compiler/src/language_reference.rs`
- `compiler/src/lib.rs`
- `compiler/src/parse.rs`
- `compiler/src/provenance.rs`
- `compiler/src/regex.rs`
- `compiler/src/tests/collections.rs`
- `compiler/tests/async_generic_method.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/field_values.rs`
- `compiler/tests/generic_method.rs`
- `compiler/tests/generic_tsc_matrix.rs`
- `compiler/tests/generic_tsc_matrix/api.rs`
- `compiler/tests/generic_tsc_matrix/kinds.rs`
- `compiler/tests/generic_tsc_matrix/product.rs`
- `compiler/tests/re_exports.rs`
- `compiler/tests/shared_narrowing.rs`
- `compiler/tests/tsc_corpus.rs`
- `corpus/reject/r148-switch-cross-case-read.ts`
- `corpus/reject/r344-type-parameter-default.ts`
- `corpus/reject/r62-valuetype-fixed-array-layout-too-large.ts`
- `corpus/reject/r94-descriptor-method.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `specs/blocks/collisions.md` (caller edit, preserved)
- `specs/tracking/s154-rejection-sites.md`

## Round 7 — rejection facts and witness classes

This section records code changes and measurements. It records no Phase Review result.

### Fixes

| Finding | Fact that separates the sites | Witness keys |
|---|---|---|
| M1 | A non-null initializer, a preceding non-null write, or a null branch with a terminal call proves the path non-null. | `flow-member-*`, `flow-nominal-*`, `flow-call-*`, `flow-cache-return`, `probe-nullable-try-both` |
| M1 controls | Branch joins intersect live exits. Loops include the zero-iteration exit. A lambda starts without its parent's facts. | `probe-nullable-conditional-write-*`; regression tests cover shadowed writes, captures, return guards, and throw guards. |
| M2 | Every switch case and default, or both try/catch exits, hold the field. A source field retains its `declare` marker. | `field-all-exits-*`, `field-unassigned-exit-*`, `declare-field-*` |
| M3 | Sized numeric types erase to number. Arrays, function signatures, and instances compare their erased component types. | `erased-assignment-*`, `erased-equality-array`, `probe-equality-erased-*`, `nonnull-null-equality-*` |
| M3 controls | A concrete source cannot satisfy an unknown destination type parameter merely because its constraint matches. | `r291-generic-constraint-to-parameter`; `distinct-assignment-*` |
| M4 | A local function declaration owns its name for the entire body. The reserved name carries the error type. | `hoisted-local-call`, `probe-hoisted-local-*`, `probe-hoisted-invalid-argument-*` |
| M5 strings | Only addition admits a string operand with a different scalar type. Other operators retain no block. | `string-add-*`, `string-invalid-*` |
| M5 enums | Distinct enum equality and nonnumeric operands differ from enum/number operations. Addition also admits enum/string concatenation. | `enum-disjoint-*`, `enum-number-*`, `probe-enum-invalid-*` |
| M5 enum members | Object member names differ from missing enum member names. | `enum-object-*`, `enum-missing-*` |
| M5 function types | Parameter and result types compare after erasure. An incompatible signature retains no block. | `function-erasure-*`, `function-distinct-*`, `probe-function-result-*`, `probe-function-result-distinct-*` |
| M5 this | An arrow has an enclosing method receiver. A module-level function has none. | `this-arrow-*`, `this-module-*` |
| M5 switch reads | A lookup crosses a lambda boundary, or reads directly from another case. | `switch-closure-*`, `switch-direct-*`, `c-lookup-switch-read` |
| M6 | Only a nullable function type receives a nullable-call flow block. A nonfunction callee retains no block. | `noncallable-*`, `flow-call-*`, `flow-call-null-*` |
| M7 | A cross-case closure can run before the binding initializer. C14 covers declaration order and the temporal dead zone. | `switch-closure-*`; `SwitchCaseClosureRead` cites C14. |
| MINOR 2 | A rejected alias supplies an error context. A contextual lambda then gives no second diagnostic. | `poisoned-alias-lambda`, `probe-poisoned-alias-context-*`, `probe-poisoned-invalid-alias-*` |

The arrow message now states that a lambda cannot capture `this`. Its remedy names a const local.
The switch fragment no longer claims that the two tiers use different storage scopes.
The normal-exit field walk includes switch and try/catch. Break ends the current case walk.
The diagnostic flow facts retain lexical ownership. A write to a shadowed local does not kill its outer binding's fact.

### Restored entries

`r148-switch-cross-case-read.ts` matches HEAD byte for byte. Its direct read gives S100 with no block; stock tsc gives TS2454.
The harness still pins its diagnostic at line 14. The closure program remains a separate accepted TypeScript witness.
The round 6 restorations `r120`, `r195`, `r249`, and `r264` remain active. The corpus header check covers them.
No entry retires in this round. No accept golden changes.

### Names

| Previous name | Topic or guard name |
|---|---|
| `divergence/round_five.rs` | `divergence/surface_forms.rs` |
| `divergence/round_five_splits.rs` | `divergence/expression_forms.rs` |
| `divergence/group_a.rs` | `divergence/builtin_calls.rs` |
| `rejection_programs_s154.txt` | `rejection_programs.txt` |
| `rejection_targets_s154.txt` | `rejection_targets.txt` |
| `round5-<form>` / `round6-<form>` witness keys | `<form>` |
| `NullableMemberNonNullInitializer` | `NullableMemberNonNullFlow` |
| `NullableCallNonNullInitializer` | `NullableCallNonNullFlow` |
| `NullableNominalAssignmentNonNullInitializer` | `NullableNominalAssignmentNonNullFlow` |
| `ThisOutsideMethodForm` variant | `ThisInMethodArrow` |
| `EnumStaticMemberMissingForm` variant | `EnumObjectMember` |
| `SwitchCaseRead` variant | `SwitchCaseClosureRead` |

The three nullable names apply to sites and variants. Fragment constant names use the same flow guard.
The `rejection_total.rs` module comment cites §153 and §154.
The failure text helper now resides in `rejection_failure.rs`. The exhaustive site map remains below 2,000 lines.

### Recorded MINORs — no code fix

| Program | Compiler measurement | Stock tsc measurement |
|---|---|---|
| Source `class Promise {}` followed by `new Promise()` | S013 with `PromiseObject` | accepts |
| Source `class Array<T> { x: i32 = 1; }` with an array-literal initializer of `Array<i32>` | accepts with the builtin Array meaning | TS2741 |
| A local `a: i32` followed by `(a, 2)` | S100 with `CommaExpression` | TS2695 |

These shapes are contrived. The Array rule remains the builtin rule of stdlib §9.0.

### Probe coverage

The new table programs contain 133 witnesses: 69 TypeScript accepts and 64 TypeScript rejects.
Each principal split has four source forms per TypeScript class. Additional forms cover function results and declaration poison contexts.
The independent regressions also check lexical ownership, lambda boundaries, zero-iteration loops, and return/throw guards.
The total test checks exact target text, reached site, block class, stock tsc labels, and fragment truth.

### Validation

- Compiler suite: 934 tests pass; one test remains ignored.
- Codegen suite: 723 tests pass; one test remains ignored. No golden changes.
- Repository hygiene check: pass.
- Total witness check: 1,367 witnesses and 463 carried variants; zero failed targets.
- Warm witness cost: 2.133 seconds; stock tsc costs 1.013 seconds and the checker costs 0.644 seconds. That run shared the machine with other work. Measured alone after the full gate, three runs: 1.152–1.187 seconds (tsc 0.55 s, checker 0.41 s). The doc comment states 1.16 seconds.
- Generic matrix: 38,868 cells and zero failures. Its final product omits 7,698 concrete instance forms.
- Workspace all-target clippy: pass. Compiler/runtime/codegen library warnings remain 5/18/13, within the 7/18/13 baseline.
- API-reference generator: pass. Generated documents come from the generator.
- Formatting and diff whitespace checks: pass.
- Site and variant names with `[A-Za-z][0-9]{2,}`: zero. Rust files above 2,000 lines: zero.

No commit occurs. The caller's `Reject: r344` collision-table edit remains unchanged.
The validation commands do not include `tools/gate.sh`.

### Cumulative changed files

The current working tree contains changes in 90 files. This list includes the inherited round 1–6 changes.
The collision-table change belongs to the caller. The restored r148 file equals HEAD and therefore has no cumulative diff.

- `compiler/src/ambient.rs`
- `compiler/src/ambient/lib_names.rs`
- `compiler/src/api_reference.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/container_argument.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/exports.rs`
- `compiler/src/check/expr.rs`
- `compiler/src/check/expr/aggregate.rs`
- `compiler/src/check/expr/array_of_and_map_copy.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/expr/namespace.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/field_initializer.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/host_entries.rs`
- `compiler/src/check/inference.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/instance_chain.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/layout.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mirror_provenance.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/namespace_import.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/opaque.rs`
- `compiler/src/check/pattern.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_facts.rs`
- `compiler/src/check/rejection_failure.rs`
- `compiler/src/check/rejection_programs.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_regressions.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_tsc.cjs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/builtin_calls.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/divergence/established.rs`
- `compiler/src/divergence/established_tail.rs`
- `compiler/src/divergence/expression_forms.rs`
- `compiler/src/divergence/surface_forms.rs`
- `compiler/src/divergence/type_flow.rs`
- `compiler/src/hir/host_entry.rs`
- `compiler/src/language_reference.rs`
- `compiler/src/lib.rs`
- `compiler/src/parse.rs`
- `compiler/src/provenance.rs`
- `compiler/src/regex.rs`
- `compiler/src/tests/collections.rs`
- `compiler/tests/async_generic_method.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/field_values.rs`
- `compiler/tests/generic_method.rs`
- `compiler/tests/generic_tsc_matrix.rs`
- `compiler/tests/generic_tsc_matrix/api.rs`
- `compiler/tests/generic_tsc_matrix/kinds.rs`
- `compiler/tests/generic_tsc_matrix/product.rs`
- `compiler/tests/re_exports.rs`
- `compiler/tests/shared_narrowing.rs`
- `compiler/tests/tsc_corpus.rs`
- `corpus/reject/r344-type-parameter-default.ts`
- `corpus/reject/r62-valuetype-fixed-array-layout-too-large.ts`
- `corpus/reject/r94-descriptor-method.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `specs/blocks/collisions.md`
- `specs/tracking/s154-rejection-sites.md`

## Round 8: first diagnostics and flow facts

This section records implementation and test evidence. It records no Phase Review result.
The round 1–7 working tree stays the base. The collision-table edit stays unchanged. No commit exists for this round.

### Nominal assignment

S005 now separates the classes through erased assignability and collected structural class signatures.
A structurally compatible class pair carries C1. A nullable target retains the same class-identity restriction.
Different instances of one generic declaration carry C24 row 25, through `ErasedNominalTypeArguments`.
An incompatible class, Map, Set, or generic-instance pair reaches `IncompatibleNominalAssignment`, without a block.
The existing nullable-source flow split takes precedence over these branches.
The structural comparison uses a recursion stack. A failed pair cannot become a successful pair on another member comparison.
Only nominal assignment requests the structural class comparison. Other erased-type consumers retain their previous comparison.
This scope preserves the generic-body matrix records for Outbox.post and Worker.post.

Witnesses: `round8-{class,nullable-class,map,set,box}-{accept,reject}-{0,1,2,3}`.
Class witnesses use initializers, arguments, returns, and assignments.
Map, Set, and Box witnesses use those four positions too.
Rejected labels include TS2322, TS2345, and TS2741. Accepted witnesses have no tsc diagnostic.
`erased-assignment-3` moves from the C1 site to the generic-argument site.

### Rejected names

A local without an initializer binds every pattern name with a mutable error type.
A later read or write adds no diagnostic. The regression beside the local-declaration poisoning test uses an assignment and a template read.
Witnesses: `round8-uninitialized-{accept,reject}-{0,1,2,3}`.
Rejected programs read the variable before assignment (TS2454). They still get one declaration diagnostic.
A rejected string enum binds its declaration name as poisoned. Its later member and print uses add no diagnostic.
A rejected parameter property binds its parameter name as poisoned inside the constructor.
Normal constructor parameters retain their signature positions after a parameter property.
Three direct regression programs cover parameter reads, a later normal parameter, and a defaulted property parameter.

### Rule 13

The total check selects the first diagnostic by input-file order, then line and column.
It checks that every tsc-accepted witness has a block at that position.
A TscRejects witness must place its target at that position.
The negative control reverses diagnostic emission order. A follow-on target still fails.

`SpreadCallArgument` splits into `SuperConstructorCall`, `DynamicImportCall`, and `NonExpressionCalleeUnavailable`.
A frame carries whether its constructor has a base class.
An unavailable super receiver or a missing literal import target reaches `NonExpressionCalleeUnavailable`, without a block.
Witnesses: `round8-{super,import-call}-{accept,reject}-{0,1,2,3}`.
Five more witnesses cover base-free constructors, instance methods, static methods, empty imports, and numeric imports.
Their labels are TS2335, TS2337, TS1450, and TS7036.
A derived-class program first reaches `ReferenceClassInheritance`, with a block. Its inherited-member diagnostic is a permitted follow-on.
A super call carries the existing class-inheritance block, whose fragment now includes the call.
A resolved dynamic import first reaches `DynamicImportCall`, with its C18 block.

String enum values now reach `EnumStringValue`, with `StringEnumMemberValue` and compiler.md §72.1.
The numeric literal range reason does not describe a string enum value.
Witnesses: `round8-enum-accept-{0,1,2,3}`.
Parameter properties first reach `ConstructorParameterProperty`.
Optional parameters first reach `OptionalParameter`.
Mixed default and named imports first reach `DefaultImport` in source order.
Export-star declarations first reach `DefaultExportDeclaration`.
Each first diagnostic carries its existing block.
Witnesses: `round8-{property,optional,mixed-import,export-all}-accept-{0,1,2,3}`.
The argument-count follow-ons after rejected parameter forms remain permitted.

The rule-13 check removes follow-on witnesses from these TscRejects index rows:

- ValueClassMemberMissing: removes `a-probe-unconstrained-object`; retains `a-s136`.
- ValueClassMethodMissing: removes `a-probe-unconstrained-object`; retains `a-s051`.
- OptionalPrivateMember: replaces `a-s222` with `round8-optional-private-reject`.
- WorkerEndpointConstructed: removes `b-signatures-382`; retains `a-s068`.
- WorkerTypeArgumentCount: removes `b-class_shape-529`; retains two direct witnesses.
- UnknownClassConstructor: replaces the incomplete-mirror program with `round8-unknown-constructor-reject`.
- FunctionImplementationMissing: removes the overload-group follow-on; retains `b-bodies-225` and four new bodyless methods.
- DuplicateNumericIndexSignature: uses `round8-duplicate-index-reject`, with valid get and set methods.
- UnboundTypeName: removes `constructor-error-type-count`; retains three first-site witnesses.

`TopLevelOverloadImplementationMissing` has no first-site witness.
Its guard requires an earlier bodyless overload, whose FunctionImplementationMissing position precedes the duplicate declaration.
Its index row records that guard. The old program remains in the program table.
`ValueConstructorArgumentCount` retains `constructor-error-type-count`.
Its new-expression position precedes the unresolved type argument in source order, although emission order differs.
All old programs remain in the program table.

### Abstract methods

The bodyless-function branch reads the source abstract modifier separately from overload and ambient-declaration facts.
An abstract method reaches `AbstractMethodBodyMissing`, with C24 row 11.
A concrete bodyless method retains `FunctionImplementationMissing`, without a block.
Witnesses: `round8-abstract-{accept,reject}-{0,1,2,3}`. The rejected label is TS2391.

### Switch facts

The nullable-flow checker intersects dispatch facts with the previous case's reachable fall-through facts.
A nullable write on a fall-through path removes the earlier non-null fact.
A non-null write cannot establish a fact absent from the direct dispatch edge.
A terminated case contributes no fall-through edge.
Witnesses: `round8-null-switch-{accept,reject}-{0,1,2,3}`. The rejected label is TS18047.

The field-exit checker distinguishes normal edges, break edges, return edges, and thrown exits.
A normal case edge continues to the next case. A switch consumes its break edges.
A break inside an if retains its field state until the switch consumes it.
Grouped labels reach the assignment before they exit.
Witnesses: `round8-field-switch-{accept,reject}-{0,1,2,3}`. The rejected label is TS2564.
The accepted forms reach `FieldAssignmentNestedEveryNormalExit`; the rejected forms reach `FieldAssignmentNestedUnassignedExit`.

### Static this

A function frame carries its collected static class identity separately from its instance receiver type.
A static this member read, write, or call checks that class's static member namespace.
An existing static member reaches `ThisStaticMethodMember`, with a true message and C24 row 15.
An instance member reaches `InstanceMemberInStaticMethod`, without a block.
Generic static methods retain the same class fact.
Witnesses: `round8-static-{accept,reject}-{0,1,2,3}`. The rejected label is TS2339.
Four accepted and four rejected witnesses add ordinary methods, generic methods, accessors, and quoted field keys.
They use `round8-static-{method,generic-method,accessor,quoted-field}-{accept,reject}`.
The rejected labels are TS2339 and TS7053.

### Minor fixes and recorded gaps

Do-while, for-in, labeled, and debugger fragments now state their separate restrictions and applicable alternatives.
The total check measures all four TypeScript fragments in its single tsc process.
The erased-assignment reason now describes structural assignability, including arrays with nullable elements.
Witnesses: `round8-array-null-accept-{0,1,2,3}` and `round8-array-incompatible-reject-{0,1,2,3}`.
Incompatible array elements give TS2322 or TS2345 and no block.
The parser's test-only RuleCode and Divergence imports move inside its test module.
The static-frame shape check reads the apparent type. The existing assignability scan fingerprint includes the new nominal guards.
Diagnostic emission moves to `rejection_diagnostic.rs`, so `rejection.rs` remains below 2,000 lines.

The handoff records these contrived gaps; this round does not fix them:

- Stale nullable-flow facts after rejected try/finally, labeled break, do-while, nullish assignment, or a destructuring swap.
- Field facts for `while(true)` with break, an infinite for loop, or a conditional-expression assignment.

### Measurements

140 new source programs extend the witness table. The total check covers 1,507 witnesses and 467 carried variants.
The rule-4 check reports zero failing targets.
The enum-name test reports zero site or variant names with source line numbers.
The round-7 handoff reports cargo wall time of 1.33–1.36 seconds.
Its split is tsc 0.55 seconds, checker 0.41 seconds, and other work 0.37–0.40 seconds.
The final rejection-test run reports 1.62 seconds: tsc 0.574 seconds, checker 0.453 seconds, and other work 0.593 seconds.
All 28 rejection tests pass.
The cost comment states the cargo wall time and its split. The tsc batch remains one process.


### Validation commands

- `cargo test --offline --locked -p subscript-compiler`: 937 tests pass; one existing test stays ignored.
- `cargo test --offline --locked -p subscript-codegen`: 723 tests pass; one existing test stays ignored.
- `cargo fmt --check`: passes.
- `cargo clippy --offline --locked --workspace --all-targets`: passes. Compiler/runtime/codegen library warnings remain 5/18/13, within the 7/18/13 baseline.
- `cargo run --offline --locked -p subscript-compiler --bin generate-api-reference`: passes.
- `git diff --check`: passes.
- `tools/hygiene.sh`: passes. The scan runs once at the end of this round.

No new warning names a round-8 helper. No Rust file exceeds 2,000 lines; `rejection.rs` has 1,972 lines.
The test-only parser imports stay inside the test module. The enum-name test passes.
No golden or pinned corpus message changes in this round. `tools/gate.sh` does not run.

### Round 8 changed files

This round changes 28 files.

- `compiler/src/check/bodies.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_diagnostic.rs`
- `compiler/src/check/rejection_facts.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_regressions.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/divergence/established.rs`
- `compiler/src/divergence/expression_forms.rs`
- `compiler/src/divergence/type_flow.rs`
- `compiler/src/parse.rs`
- `compiler/tests/apparent_type_shapes.rs`
- `specs/tracking/s154-rejection-sites.md`

### Cumulative changed files

The preserved working tree contains 92 changed files.
The collision-table path below is the preserved owner edit.

- `compiler/src/ambient.rs`
- `compiler/src/ambient/lib_names.rs`
- `compiler/src/api_reference.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/container_argument.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/exports.rs`
- `compiler/src/check/expr.rs`
- `compiler/src/check/expr/aggregate.rs`
- `compiler/src/check/expr/array_of_and_map_copy.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/expr/namespace.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/field_initializer.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/host_entries.rs`
- `compiler/src/check/inference.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/instance_chain.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/layout.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mirror_provenance.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/namespace_import.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/opaque.rs`
- `compiler/src/check/pattern.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_diagnostic.rs`
- `compiler/src/check/rejection_facts.rs`
- `compiler/src/check/rejection_failure.rs`
- `compiler/src/check/rejection_programs.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_regressions.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_tsc.cjs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/builtin_calls.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/divergence/established.rs`
- `compiler/src/divergence/established_tail.rs`
- `compiler/src/divergence/expression_forms.rs`
- `compiler/src/divergence/surface_forms.rs`
- `compiler/src/divergence/type_flow.rs`
- `compiler/src/hir/host_entry.rs`
- `compiler/src/language_reference.rs`
- `compiler/src/lib.rs`
- `compiler/src/parse.rs`
- `compiler/src/provenance.rs`
- `compiler/src/regex.rs`
- `compiler/src/tests/collections.rs`
- `compiler/tests/apparent_type_shapes.rs`
- `compiler/tests/async_generic_method.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/field_values.rs`
- `compiler/tests/generic_method.rs`
- `compiler/tests/generic_tsc_matrix.rs`
- `compiler/tests/generic_tsc_matrix/api.rs`
- `compiler/tests/generic_tsc_matrix/kinds.rs`
- `compiler/tests/generic_tsc_matrix/product.rs`
- `compiler/tests/re_exports.rs`
- `compiler/tests/shared_narrowing.rs`
- `compiler/tests/tsc_corpus.rs`
- `corpus/reject/r344-type-parameter-default.ts`
- `corpus/reject/r62-valuetype-fixed-array-layout-too-large.ts`
- `corpus/reject/r94-descriptor-method.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `specs/blocks/collisions.md`
- `specs/tracking/s154-rejection-sites.md`


## Round 9: regressions, truth, and names

### Changes

- The field-exit fact closes a source-enum switch when its labels cover every declared member value.
  An incomplete switch retains the unassigned exit. This change does not alter nullable flow or return coverage.
- Relational string sites require `<`, `>`, `<=`, or `>=` on two strings.
  String subtraction, multiplication, and bitwise operations reach `BinaryInvalidOperand`, without a block.
  String and boolean relational variants cite C24 row 31.
  Their reasons state that no comparison lowering is decided. The string remedy compares `charCodeAt` values.
- Statement and conditional truth tests split on `AsyncHandle` and `Func`.
  `StatementAlwaysTruthyCondition` and `ConditionalAlwaysTruthyCondition` are `TscRejects`.
  An uncalled class method in a root truth test reaches `MethodValueTruthTest`, also `TscRejects`.
  Parentheses preserve the truth-test context. Other member reads retain their existing sites.
- Rest syntax reaches `RestParameter`, with its own fragment and the no-variadic-parameter reason from `stdlib.md` §14.4.
  The mirror binding-pattern site is now `MirrorBindingPatternParameter`.
- The §71 reason separates generic static storage from the absence of a static instance receiver.
  The reason no longer says that a non-generic class has no single slot.
- Witness keys and paths use site or form names. Round, review, and source-line key names are absent.
  The test is now `first_diagnostics_and_switch_facts`.
- Seventeen obsolete target names occupied 21 rows. Those rows are deleted.
  The total check compares every target row with the closed site inventory, including API sites.
  `target_row_without_site_fires` supplies its failure control.
- The rule 13 check uses the first diagnostic in source order for both its decision and failure text.
  `accepted_first_diagnostic_without_block_fires_in_source_order` supplies its failure control.
  The control puts the later diagnostic first in emission order.
- The apparent-type allowlist follows `member_on_context`, with the same fingerprint and reason.

### Measured regression witnesses

| Form | Stock tsc diagnostic set | Checker class |
|---|---|---|
| Exhaustive source-enum constructor switch without default | accepts | `FieldAssignmentNestedEveryNormalExit`, block |
| Incomplete source-enum constructor switch | TS2564 | `FieldAssignmentNestedUnassignedExit`, no block |
| String subtraction, multiplication, or bitwise AND | TS2362, TS2363 | `BinaryInvalidOperand`, no block |
| Async call as statement or conditional truth test | TS2801 | always-truthy condition site, no block |
| Function value as statement or conditional truth test | TS2774 | always-truthy condition site, no block |
| Synchronous, async, or generic method value as truth test | TS2774 | `MethodValueTruthTest`, no block |
| Script rest parameter | accepts | `RestParameter`, §14.4 block |

One stock tsc batch measures the regression witnesses, the existing witnesses, and every carried TypeScript fragment.
The total check covers 1,521 witnesses and 468 carried variants. Red count: 0.

### Residual records from the handoff

These records remain outside this repair. No residual implementation changes occur.

- Same-private-field class pairs carry the C1 block.
- `static abstract`, `f(a?: i32, b: i32)`, and `c[0]` on a class carry a block.
- `const x: u32 = -1` says "literal 1".
- Mixed-type `<` says "mixed-type arithmetic".

The §154.3 residual instances remain unchanged. This section records no Phase Review result.

### Validation

- `cargo test --offline --locked -p subscript-compiler`: passes all 939 tests, including doc tests.
- `cargo test --offline --locked -p subscript-codegen`: passes all 723 tests. No golden moves.
- `cargo fmt --check` and `git diff --check`: pass.
- Workspace all-target clippy passes. Compiler/runtime/codegen library warnings remain 5/18/13, within the 7/18/13 baseline.
- The API-reference generator succeeds. The compiler suite passes generated-document checks and the collision corpus-index check.
- The warm rule 4 test reports `finished in 1.54s`: tsc 0.573s, checker 0.459s, total internal time 1.312s.
- All 1,199 general-program keys are unique. Round, review, and source-line key checks report zero matches.
- Every Rust file remains below 2,000 lines. The maximum is `check/rejection.rs`, at 1,978 lines.
- Test-only helpers stay inside the existing `cfg(test)` modules. Production facts remain production-scoped.
- No accept golden or corpus reject message changes. The generator owns generated-document edits.
- `tools/gate.sh` is not run. No commit occurs.

### Round 9 edited files

- `compiler/src/check/bodies.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_facts.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_regressions.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/divergence/established_tail.rs`
- `compiler/src/divergence/expression_forms.rs`
- `compiler/src/divergence/type_flow.rs`
- `compiler/tests/apparent_type_shapes.rs`
- `specs/tracking/s154-rejection-sites.md`

### Cumulative changed files

The files below include the preserved base. The collision-table change is the preserved owner edit.
- `compiler/src/ambient.rs`
- `compiler/src/ambient/lib_names.rs`
- `compiler/src/api_reference.rs`
- `compiler/src/check/bindings.rs`
- `compiler/src/check/bodies.rs`
- `compiler/src/check/capture.rs`
- `compiler/src/check/class_shape.rs`
- `compiler/src/check/container_argument.rs`
- `compiler/src/check/declarations.rs`
- `compiler/src/check/exception.rs`
- `compiler/src/check/exports.rs`
- `compiler/src/check/expr.rs`
- `compiler/src/check/expr/aggregate.rs`
- `compiler/src/check/expr/array_of_and_map_copy.rs`
- `compiler/src/check/expr/assign.rs`
- `compiler/src/check/expr/call.rs`
- `compiler/src/check/expr/entry.rs`
- `compiler/src/check/expr/lambda.rs`
- `compiler/src/check/expr/literal.rs`
- `compiler/src/check/expr/member.rs`
- `compiler/src/check/expr/method.rs`
- `compiler/src/check/expr/namespace.rs`
- `compiler/src/check/expr/operator.rs`
- `compiler/src/check/field_initializer.rs`
- `compiler/src/check/generics.rs`
- `compiler/src/check/host_entries.rs`
- `compiler/src/check/inference.rs`
- `compiler/src/check/init_effects.rs`
- `compiler/src/check/instance_chain.rs`
- `compiler/src/check/json.rs`
- `compiler/src/check/layout.rs`
- `compiler/src/check/lookup.rs`
- `compiler/src/check/mirror_provenance.rs`
- `compiler/src/check/mod.rs`
- `compiler/src/check/namespace_import.rs`
- `compiler/src/check/narrowing.rs`
- `compiler/src/check/opaque.rs`
- `compiler/src/check/pattern.rs`
- `compiler/src/check/pipeline.rs`
- `compiler/src/check/rejection.rs`
- `compiler/src/check/rejection_diagnostic.rs`
- `compiler/src/check/rejection_facts.rs`
- `compiler/src/check/rejection_failure.rs`
- `compiler/src/check/rejection_programs.rs`
- `compiler/src/check/rejection_programs.txt`
- `compiler/src/check/rejection_regressions.rs`
- `compiler/src/check/rejection_sites.rs`
- `compiler/src/check/rejection_targets.txt`
- `compiler/src/check/rejection_total.rs`
- `compiler/src/check/rejection_tsc.cjs`
- `compiler/src/check/rejection_witness_index.rs`
- `compiler/src/check/rejection_witness_sites.rs`
- `compiler/src/check/rejection_witnesses.txt`
- `compiler/src/check/signatures.rs`
- `compiler/src/check/stmt.rs`
- `compiler/src/check/text.rs`
- `compiler/src/check/type_rules.rs`
- `compiler/src/check/tyres.rs`
- `compiler/src/divergence.rs`
- `compiler/src/divergence/builtin_calls.rs`
- `compiler/src/divergence/entries.rs`
- `compiler/src/divergence/established.rs`
- `compiler/src/divergence/established_tail.rs`
- `compiler/src/divergence/expression_forms.rs`
- `compiler/src/divergence/surface_forms.rs`
- `compiler/src/divergence/type_flow.rs`
- `compiler/src/hir/host_entry.rs`
- `compiler/src/language_reference.rs`
- `compiler/src/lib.rs`
- `compiler/src/parse.rs`
- `compiler/src/provenance.rs`
- `compiler/src/regex.rs`
- `compiler/src/tests/collections.rs`
- `compiler/tests/apparent_type_shapes.rs`
- `compiler/tests/async_generic_method.rs`
- `compiler/tests/corpus_reject.rs`
- `compiler/tests/field_values.rs`
- `compiler/tests/generic_method.rs`
- `compiler/tests/generic_tsc_matrix.rs`
- `compiler/tests/generic_tsc_matrix/api.rs`
- `compiler/tests/generic_tsc_matrix/kinds.rs`
- `compiler/tests/generic_tsc_matrix/product.rs`
- `compiler/tests/re_exports.rs`
- `compiler/tests/shared_narrowing.rs`
- `compiler/tests/tsc_corpus.rs`
- `corpus/reject/r344-type-parameter-default.ts`
- `corpus/reject/r62-valuetype-fixed-array-layout-too-large.ts`
- `corpus/reject/r94-descriptor-method.ts`
- `generated-docs/corpus-index.md`
- `generated-docs/language-reference.md`
- `specs/blocks/collisions.md`
- `specs/tracking/s154-rejection-sites.md`

## Phase Review

Three fresh reviews ran on the cumulative working tree.

| Pass | Base | CRITICAL | MAJOR | MINOR | Fixed in |
|---|---|---:|---:|---:|---|
| 1 | `e72419b9` | 0 | 7 | 6 | round 7 |
| 2 | `1ba9ef14` | 0 | 6 | 7 | round 8 |
| 3 | `aee0e020` | 0 | 15 | 9 | round 9 (regressions and false text) |

Every MAJOR is an instance of one class: the checker approximates the
`tsc` acceptance of a program, so a `TscRejects` site can be reached by
a `tsc`-accepted program, or a `Diverges` site can block a
`tsc`-rejected mistake. The counts did not fall, so the owner decided
on 2026-10-04 to close the phase with the residual instances recorded:
§154.3 lists the ten pass-3 instances that were present before §154
and four acceptance gaps outside the section. Pass 3 found that the new
facts change no acceptance: every corpus program and 1,185 witness
programs give the HEAD exit status, except the intended rule 10 and
rule 11 programs.

Round 9 fixed the pass-3 regressions against HEAD (the exhaustive enum
`switch` field fact, string operators other than relational, truth
tests on an async handle or a function value), the two false `why`
texts (a rest parameter at the mirror site, `<` on strings), and the
MINOR names, dead target rows, rule 13 firing control, and cost text.
Measured after round 9 with the CLI: each of the five fixed forms gives
the expected block or no block.

Final gate:
`gate full e35643e9 dirty:86 debug 2259/0/3 release 2256/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.
`tools/hygiene.sh`: exit 0. COMPLETE.
