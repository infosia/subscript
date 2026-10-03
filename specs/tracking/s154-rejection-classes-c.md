# S154: non-S014 rejection classes, group c

Measurement commit: `ee67931d2c365d61b28aac3b38efd8342900b853`.
This Step 0 measurement tests §79 rule 2 and the open scope in §153.3.
No production, test, or corpus file changed. No commit or gate run occurred.

The scope contains every production file under `compiler/src`, except groups a and b.
Group a contains `check/expr.rs` and `check/expr/`.
Group b contains `class_shape.rs`, `declarations.rs`, `stmt.rs`, `signatures.rs`, `exports.rs`, `bodies.rs`, and `exception.rs` under `check/`.

Method: inspect rejection calls and direct diagnostic record builds, then inspect each source guard.
`target/s154/c/inventory.py` finds calls to `error`, `resolution_error`, `error_diverging`, and `Diagnostic::new`.
The scan excludes test items by their `cfg(test)` scope. The scan excludes the two other file groups.
`diag.rs:198` supplies the sole direct diagnostic record build.
The inventory does not count `RuleCode` references, enum arms, HIR Error values, or warnings.
The ambient rejection rows all carry S014 at this commit. Thus, group c has no non-S014 ambient row.
`reject_subset` contributes only its S100 `MirrorParameter` arm. Its other named sites carry S014.

Inventory: 116 construction sites, including four common builders. The other 112 sites decide a rejection.
The four common builders are `diag.rs:198` and `check/bindings.rs:226,236,249`.
Their target diagnostics reach them through the selected callers. They have no independent rejection reason.
The final diagnostic determines the block result, after callers set the variant.
A missing block at a common builder needs a variant at its rejecting caller.
Do not add an unconditional variant to `Diagnostic::new` or `error`.

Each witness program exports `main(): void`.
The runner witness exports `main` from its library; its designated entry exports `run`.
The batch also calls `runner_main` after successful checking. This extra call reaches the runner diagnostic.
The empty-input API probe reaches `lib.rs:147`, but no source program reaches its empty-input guard.

The batch uses the built compiler library through `check_program`, without production instrumentation.
Source guards, messages, positions, and caller routes identify each target.
`checker-results.tsv` records full messages, codes, positions, and variants.
Each `<witness>.checker.txt` contains the complete render. The runner uses `<witness>.runner.checker.txt`.
The report script verifies each selected diagnostic's rendered block.
The first table selects one target diagnostic per site, even when another diagnostic comes first.
There are 25 such primary observations. The secondary table keeps alternate forms and rejected witnesses.

Probes include lib overloads, generics, unions, readonly arrays, interfaces, optional parameters, Iterable, truthiness, implicit conversions, literal types, and `prelude/lang.d.ts`.
Generic defaults reach all three instance arity sites with TypeScript-accepted programs.
Local aliases reach several ambient arity guards. Local class and function declarations reach import-use guards.
Those local declarations themselves receive earlier diagnostics. The table records the later target, as the handoff requires.
`pattern-iterator-extended.ts` adds an optional property to a TypeScript generator result. It reaches the no-block pattern arm.
`nullable-initialized.ts` reaches the no-block nullable-use arm, because TypeScript narrows the initialized local.

One final TypeScript project uses exactly the `tsconfig()` options in `compiler/tests/support/tsc.rs`.
Options: strict, noEmit, ES2022, ESNext, Bundler, ES2022 plus ESNext.Disposable, empty types, and forceConsistentCasingInFileNames.
The project includes all witness programs, their import dependencies, and `prelude/lang.d.ts`.
Each mirror and its program use one module-scoped TypeScript view. This prevents global mirror names from affecting other witnesses.
The checker receives the original program and ambient mirror as separate SourceFile inputs.
The global-augmentation experiment is excluded because it changes other programs' TypeScript names.

TypeScript version: `5.9.2`.
A stock CLI run reports only syntax errors when one project file has a syntax error.
Therefore, the final measurement uses stock TypeScript's `createProgram` and `getPreEmitDiagnostics` in one Node process.
This obtains both syntax and semantic diagnostics from the same project. No diagnostic suppression or option change occurs.
Command: `node target/s154/c/tsc-batch.cjs`.
`tsc-results.json` records per-program TS codes; `tsc-output.txt` contains the full diagnostics.

Summary counts include common builders. Counts describe distinct physical sites, not source-file counts.
A reached site uses an actual source witness, except the post-check runner call stated above.
A wrong site has at least one retained TypeScript-rejected witness with a block.
A wrong observation can coexist with an accepting witness, as §79 rule 6 permits.

| Scope | Sites | Reached | tsc accepts | Accepted with block | Needs variant | Wrong | Unreachable |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| All construction sites | 116 | 110 | 106 | 36 | 70 | 7 | 6 |
| Rejection decisions only | 112 | 106 | 102 | 35 | 67 | 5 | 6 |

Final checker cost: 0.121136125 seconds for 456 programs in one process through `check_program`.
This timer includes parsing and checking. Input reads precede the timer. Diagnostic renders follow the timer.
Full checker process wall cost: 0.154481625 seconds, including input reads and evidence-file writes.
Final TypeScript process wall cost: 0.390487708 seconds for 468 source files in one project.
TypeScript API cost inside that process: 0.272117459 seconds. It reports 143 diagnostics.
Seed corpus witnesses include other groups' diagnostics; their cost is included, but those diagnostics do not enter the site counts.

All witness paths below start at `target/s154/c/`. All site paths start at `compiler/src/`.
Messages contain their first 60 characters. “—” means that no target diagnostic came from a source program.
An unreachable row cites a positive probe and its source guard. The probe does not reach the target.
An existing variant proposal states the reason for the selected accepting witness. A broader site can need another reason for another form.

| Site, function, guard | Code | Message, first 60 characters | Witness | tsc | Block | Verdict | Proposed variant |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `lib.rs:147`; `check_program_with`; files.is_empty() | `S100` | `no source files given` | `runner-empty.ts (probe)` | — | — | unreachable; files.is_empty(); no source program reaches this input guard. | — |
| `parse.rs:237`; `parser_diagnostic`; SyntaxError::LoneSurrogateEscape | `S100` | `a lone surrogate escape has no UTF-8 encoding; write the pai` | `parse-surrogate.ts` | accepts | yes | ok | — |
| `parse.rs:245`; `parser_diagnostic`; any other SyntaxError | `S100` | `parse error: Expression expected` | `parse-invalid.ts` | TS1109 | no | ok | — |
| `provenance.rs:268`; `malformed`; malformed provenance directive | `S100` | `` mirror `target/s154/c/prov-malformed.d.ts` has malformed pro `` | `prov-malformed.ts` | accepts | no | needs variant | new |
| `provenance.rs:279`; `duplicate`; duplicate provenance directive | `S100` | `` mirror `target/s154/c/prov-duplicate.d.ts` has duplicate pro `` | `prov-duplicate.ts` | accepts | no | needs variant | new |
| `hir/host_entry.rs:36`; `runner_main`; main is absent or has parameters | `S100` | `` entry module exports no host entry `main` `` | `runner-library-main.ts` | accepts | no | needs variant | new |
| `check/layout.rs:264`; `class_layout`; requested alignment < natural alignment | `S100` | `` requested alignment 2 is below the natural alignment 4 for ` `` | `corpus-r135-valuetype-align-below-natural.ts` | accepts | yes | ok | — |
| `check/layout.rs:284`; `class_layout`; final class padding exceeds limit | `S100` | `` `C` layout exceeds the supported aggregate limit of 21474836 `` | `class-final-align.ts` | accepts | no | needs variant | AggregateLayoutLimit |
| `check/layout.rs:304`; `class_too_large`; field placement exceeds limit | `S100` | `` `Accumulated` layout exceeds the supported aggregate limit o `` | `corpus-r65-valuetype-field-offset-layout-too-large.ts` | accepts | yes | ok | — |
| `check/layout.rs:369`; `add_frame_slot`; accumulated frame slot exceeds limit | `S100` | `local aggregate storage makes the accumulated Cranelift stac` | `corpus-r71-accumulated-frame-locals-too-large.ts` | accepts | yes | ok | — |
| `check/layout.rs:413`; `validate_generator_layout`; coroutine frame member exceeds limit | `S100` | `generator frame layout exceeds the supported aggregate limit` | `corpus-r70-generator-frame-layout-too-large.ts` | accepts | yes | ok | — |
| `check/layout.rs:501`; `validate_generator_layout`; async call count multiplication overflows | `S100` | `async frame layout exceeds the supported aggregate limit` | `async-children-frame.ts (probe)` | — | — | unreachable; count_async_calls(...).checked_mul(8) returns None; a representable HIR cannot contain more than u64::MAX / 8 calls. | — |
| `check/layout.rs:517`; `validate_generator_layout`; final coroutine frame padding exceeds limit | `S100` | `generator frame layout exceeds the supported aggregate limit` | `generator-final-align.ts` | accepts | yes | ok | — |
| `check/layout.rs:536`; `closure_layout`; closure environment exceeds limit | `S100` | `closure environment layout exceeds the supported aggregate l` | `corpus-r69-closure-environment-layout-too-large.ts` | accepts | yes | ok | — |
| `check/layout.rs:865`; `validate`; pending type layout is TooLarge | `S100` | `` `FixedArray` byte size exceeds the supported aggregate limit `` | `layout-pending.ts` | accepts | yes | ok | — |
| `check/lookup.rs:19`; `scope_item`; a type-only import is used as a value | `S100` | `` `C` cannot be used as a value because it was imported with ` `` | `type-only-local-class.ts` | accepts | no | needs variant | new |
| `check/lookup.rs:37`; `scope_item`; scope item is a namespace import | `S100` | `` namespace import `ns` is a static qualifier and cannot be us `` | `namespace-local-function.ts` | accepts | no | needs variant | new |
| `check/lookup.rs:125`; `lookup_local_access`; read from another switch case | `S100` | `` `x` is read from a different switch case `` | `lookup-switch-let-closure.ts` | accepts | no | needs variant | DeclarationScope |
| `check/lookup.rs:127`; `lookup_local_access`; write in another switch case | `S100` | `` `x` is assigned in a case that does not declare it `` | `lookup-switch-write.ts` | accepts | yes | ok | — |
| `check/lookup.rs:154`; `lookup_local_access`; pending read shadows a class, function, or ambient namespace | `S100` | `` `C` is read before its declaration in this block `` | `lookup-shadow-class.ts` | accepts | no | needs variant | DeclarationScope |
| `check/lookup.rs:156`; `lookup_local_access`; other pending local read | `S100` | `` `x` is read before its declaration in this block `` | `lookup-pending-closure.ts` | accepts | yes | ok | — |
| `check/lookup.rs:175`; `lookup_local_access`; write to a pending local | `S100` | `` `x` is assigned before its declaration in this block `` | `lookup-write-closure.ts` | accepts | no | needs variant | new |
| `check/lookup.rs:194`; `lookup_local_access`; capture crosses a lambda boundary with a Context-affine type | `S100` | `` lambda captures Context-affine `box`; Worker, Inbox, and Out `` | `lookup-context-capture.ts` | accepts | no | needs variant | WorkerContextAffinity |
| `check/lookup.rs:203`; `lookup_local_access`; capture crosses a lambda boundary with a mutable local | `S009` | `` lambda captures `x`, which is not a `const` local; capturing `` | `capture-mutable.ts` | accepts | no | needs variant | new |
| `check/narrowing.rs:67`; `nullable_use_error`; path is in ended_shared_narrowing | `S011` | `` `Cell \| null` may be null here; narrow with a null check fir `` | `corpus-r256-narrowing-loop-call.ts` | accepts | yes | ok | — |
| `check/narrowing.rs:69`; `nullable_use_error`; path is absent from ended_shared_narrowing | `S011` | `` `C \| null` may be null here; narrow with a null check first `` | `nullable-initialized.ts` | accepts | no | needs variant | new |
| `check/rejection.rs:202`; `reject_subset`; site == RejectionSite::MirrorParameter; only S100 arm | `S100` | `parameter pattern outside the decided surface` | `mirror-pattern.ts` | accepts | yes | ok | — |
| `check/host_entries.rs:26`; `entry_file`; entry-file selection fails | `S100` | `a program with multiple source files must name its entry mod` | `entry-ambiguous.ts` | accepts | no | needs variant | new |
| `check/host_entries.rs:126`; `populate`; host export lacks a supported function signature | `S100` | `` entry export `f`: generic function `f` in `target/s154/c/hos `` | `host-generic.ts` | accepts | yes | ok | — |
| `check/generics.rs:65`; `apply_type_parameter_constraints`; type argument does not satisfy its constraint | `S100` | `` type argument `D` does not satisfy the constraint `C` of `T` `` | `constraint-structural.ts` | accepts | no | needs variant | new |
| `check/generics.rs:78`; `apply_type_parameter_constraints`; root constraint_cycle is true | `S100` | `` type parameter `T` has a circular constraint `` | `constraint-circular.ts` | TS2313 | no | ok | — |
| `check/generics.rs:283`; `instantiate_fn`; function type-argument count differs | `S100` | `` `f` expects 2 type argument(s), got 1 `` | `generic-function-default-explicit.ts` | accepts | no | needs variant | new |
| `check/generics.rs:375`; `instantiate_method`; method type-argument count differs | `S100` | `` `f` expects 2 type argument(s), got 1 `` | `generic-method-default-explicit.ts` | accepts | no | needs variant | new |
| `check/generics.rs:492`; `instantiate_class`; class type-argument count differs | `S100` | `` `C` expects 2 type argument(s), got 1 `` | `generic-class-default-explicit.ts` | accepts | no | needs variant | new |
| `check/generics.rs:580`; `check_pending_instance_bodies`; pending class lacks instance_arguments | `S100` | `internal error: deferred instance has no type arguments` | `generics-probe.ts (probe)` | — | — | unreachable; A pending ClassId lacks instance_arguments; instantiate_class inserts that record before it queues the body. | — |
| `check/generics.rs:588`; `check_pending_instance_bodies`; pending class lacks generic_classes template | `S100` | `internal error: deferred instance has no generic template` | `generics-probe.ts (probe)` | — | — | unreachable; A pending key lacks generic_classes; instantiate_class obtains the template from that same map and never deletes it. | — |
| `check/mirror_provenance.rs:33`; `collect_mirror_provenance`; foreign functions lack header provenance | `S100` | `` mirror `target/s154/c/prov-header.d.ts` declares foreign fun `` | `prov-header.ts` | accepts | no | needs variant | new |
| `check/mirror_provenance.rs:61`; `collect_mirror_provenance`; provenance names an absent parameter | `S100` | `` mirror `target/s154/c/prov-target-parameter.d.ts` has proven `` | `prov-target-parameter.ts` | accepts | no | needs variant | new |
| `check/mirror_provenance.rs:75`; `collect_mirror_provenance`; provenance names an absent callback alias | `S100` | `` mirror `target/s154/c/prov-target-callback.d.ts` has provena `` | `prov-target-callback.ts` | accepts | no | needs variant | new |
| `check/mirror_provenance.rs:120`; `collect_mirror_provenance`; callback lifetime names no callback-bearing class | `S100` | `` mirror `target/s154/c/prov-target-lifetime.d.ts` has provena `` | `prov-target-lifetime.ts` | accepts | no | needs variant | new |
| `check/mirror_provenance.rs:166`; `foreign_parameter_provenance`; array parameter lacks descriptor or scalar-pair provenance | `S100` | `` mirror `target/s154/c/prov-array.d.ts` parameter `foreign.xs `` | `prov-array.ts` | accepts | no | needs variant | new |
| `check/mirror_provenance.rs:180`; `foreign_parameter_provenance`; string parameter lacks string-view provenance | `S100` | `` mirror `target/s154/c/prov-string.d.ts` parameter `foreign.x `` | `prov-string.ts` | accepts | no | needs variant | new |
| `check/mirror_provenance.rs:193`; `foreign_parameter_provenance`; parameter type and provenance differ | `S100` | `` mirror `target/s154/c/prov-incompatible.d.ts` has provenance `` | `prov-incompatible.ts` | accepts | no | needs variant | new |
| `check/mirror_provenance.rs:217`; `callback_provenance`; callback field has no named type reference | `S100` | `` mirror `target/s154/c/callback-anonymous-field.d.ts` has an  `` | `callback-anonymous-field.ts` | accepts | no | needs variant | new |
| `check/mirror_provenance.rs:233`; `callback_provenance`; named callback field lacks callback provenance | `S100` | `` mirror `target/s154/c/callback-alias-field.d.ts` callback ty `` | `callback-alias-field.ts` | accepts | no | needs variant | new |
| `check/mod.rs:923`; `with_synthetic_owner`; initializer or switch-case owner rejects a synthetic prefix | `S100` | `` a non-place receiver of `??` or `?.` cannot be used in an in `` | `synthetic-global.ts` | accepts | yes | ok | — |
| `check/instance_chain.rs:236`; `enter_instance`; active instance chain contains an expanding cycle | `S100` | `` generic template `nest`: the chain of instances grows withou `` | `corpus-r288-growing-instance-chain.ts` | accepts | yes | ok | — |
| `check/init_effects.rs:782`; `module_initializer_diagnostics`; module segment validation fails | `S100` | `internal error: initializer segment is outside the module bo` | `root-initializer-recursive.ts (probe)` | — | — | unreachable; validate_module_segment fails; the checker creates every segment from its own top-level and global ranges. | — |
| `check/init_effects.rs:872`; `module_initializer_diagnostics`; initializer route lacks a function summary | `S100` | `internal error: made function value has no summary` | `root-initializer-recursive.ts (probe)` | — | — | unreachable; A made function value lacks a summary; the scanner supplies each function, constructor, method, and lambda summary. | — |
| `check/init_effects.rs:902`; `module_initializer_diagnostics`; initializer route reads a later binding | `S100` | `` `value` is accessed before its declaration, through `f` `` | `root-initializer-recursive.ts` | accepts | yes | ok | — |
| `check/container_argument.rs:49`; `container_argument`; Context-affine ArrayElement | `S100` | `Worker, Inbox, and Outbox values may not be array elements` | `worker-array.ts` | accepts | no | needs variant | WorkerContextAffinity |
| `check/container_argument.rs:55`; `container_argument`; Context-affine MapKey, MapValue, or SetElement | `S100` | `Worker, Inbox, and Outbox values may not be container type a` | `worker-map.ts` | accepts | yes | ok | — |
| `check/text.rs:49`; `check_uri_call`; URI call has explicit type arguments | `S100` | `` `encodeURI` is not generic `` | `uri-local-generic.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:22`; `resolve_async_return`; async return annotation is not TsTypeRef | `S100` | `` async functions must return an explicitly annotated `Promise `` | `async-parenthesized.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:31`; `resolve_async_return`; async return reference is qualified | `S100` | `` async functions must return an explicitly annotated `Promise `` | `async-qualified.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:40`; `resolve_async_return`; async return reference name is not Promise | `S100` | `` async functions must return an explicitly annotated `Promise `` | `async-alias.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:49`; `resolve_async_return`; async Promise reference has no type arguments | `S100` | `` `Promise` requires exactly one fulfilled-value type argument `` | `async-default-alias.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:58`; `resolve_async_return`; async Promise reference has other than one argument | `S100` | `` `Promise` requires exactly one fulfilled-value type argument `` | `async-arity-alias.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:97`; `resolve_type_position`; other TsType annotation form | `S100` | `type annotation form outside the decided surface` | `annotation-tuple.ts` | accepts | no | needs variant | NoTupleType |
| `check/tyres.rs:106`; `resolve_type_position`; void outside a result position | `S100` | `` `void` is only allowed as a return, generator element, or Pr `` | `corpus-r335-annotated-void-binding.ts` | accepts | yes | ok | — |
| `check/tyres.rs:123`; `resolve_keyword`; TsNumberKeyword | `S007` | `` bare `number` is rejected; there is no default numeric type  `` | `corpus-r08-bare-number.ts` | accepts | yes | ok | — |
| `check/tyres.rs:134`; `resolve_keyword`; TsAnyKeyword | `S001` | `` `any` is not part of the language `` | `corpus-r01-any.ts` | accepts | yes | ok | — |
| `check/tyres.rs:143`; `resolve_keyword`; TsUndefinedKeyword | `S012` | `` `undefined` is banned; the single null story is `null` `` | `undefined-type.ts` | accepts | yes | ok | — |
| `check/tyres.rs:169`; `resolve_keyword`; TsObjectKeyword outside admitted contexts | `S011` | `` `object` is a boundary-only type; it is not available to gen `` | `object-type.ts` | accepts | yes | ok | — |
| `check/tyres.rs:180`; `resolve_keyword`; other keyword type | `S100` | `keyword type outside the decided surface` | `keyword-unknown.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:193`; `resolve_type_ref`; qualified non-async type reference | `S100` | `qualified type names are not decided` | `qualified.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:214`; `resolve_type_ref`; ambient Worker, Inbox, or Outbox lacks arguments | `S100` | `` generic reference class `Worker` requires explicit type argu `` | `worker-local-default.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:224`; `resolve_type_ref`; ambient worker-type argument count differs | `S100` | `` `Worker` takes exactly 2 type argument(s) `` | `worker-local-one.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:246`; `resolve_type_ref`; worker message argument is not a plain reference class | `S100` | `` worker message type `i32` must be a plain reference class `` | `worker-local-scalar.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:267`; `resolve_type_ref`; ambient RegExp has type arguments | `S100` | `` `RegExp` is not generic `` | `regexp-local.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:283`; `resolve_type_ref`; general Promise reference lacks arguments | `S100` | `` `Promise` requires exactly one fulfilled-value type argument `` | `promise-default-alias.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:291`; `resolve_type_ref`; general Promise reference has other than one argument | `S100` | `` `Promise` requires exactly one fulfilled-value type argument `` | `promise-arity-alias.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:303`; `resolve_type_ref`; FixedArray reference lacks arguments | `S100` | `` `FixedArray` requires element type and length arguments `` | `fixed-default-alias.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:311`; `resolve_type_ref`; FixedArray reference has other than two arguments | `S100` | `` `FixedArray` takes exactly two type arguments `` | `fixed-one-alias.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:328`; `resolve_type_ref`; FixedArray length exceeds u32::MAX | `S008` | `FixedArray length 4294967296 out of range (maximum 429496729` | `fixed-length-overflow.ts` | accepts | no | needs variant | IntegerLiteralRange |
| `check/tyres.rs:343`; `resolve_type_ref`; FixedArray length is not a non-negative integer literal | `S100` | `` `FixedArray` length must be a non-negative integer literal `` | `fixed-length-negative.ts` | accepts | no | needs variant | IntegerLiteralRange |
| `check/tyres.rs:367`; `resolve_type_ref`; class-independent FixedArray is too large; variant context exists | `S100` | `` `FixedArray` byte size exceeds the supported aggregate limit `` | `corpus-r63-local-fixed-array-layout-too-large.ts` | accepts | yes | ok | — |
| `check/tyres.rs:369`; `resolve_type_ref`; class-independent FixedArray is too large; variant context is absent | `S100` | `` `FixedArray` byte size exceeds the supported aggregate limit `` | `fixed-too-large.ts` | accepts | no | needs variant | AggregateLayoutLimit |
| `check/tyres.rs:390`; `resolve_type_ref`; Array reference lacks exactly one type argument | `S100` | `` `Array` takes one type argument `` | `array-two-alias.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:403`; `resolve_type_ref`; Generator reference lacks a yield type argument | `S100` | `` `Generator` requires at least a yield type argument `` | `generator-no-argument.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:419`; `resolve_type_ref`; ambient Error-family type has type arguments | `S100` | `` `Error` is not generic `` | `error-local.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:430`; `resolve_type_ref`; ambient Map or Set reference lacks arguments | `S100` | `` generic reference class `Map` requires explicit type argumen `` | `map-local-default.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:439`; `resolve_type_ref`; ambient Map or Set argument count differs | `S100` | `` `Map` takes exactly 2 type argument(s) `` | `map-local-one.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:495`; `resolve_type_ref`; ambient Date reference has type arguments | `S100` | `` `Date` is not generic `` | `date-local.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:512`; `resolve_type_ref`; non-generic class reference has type arguments | `S100` | `` `C` is not generic `` | `class-local.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:518`; `resolve_type_ref`; generic class reference lacks arguments | `S100` | `` generic class `C` requires explicit type arguments `` | `class-default.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:541`; `resolve_type_ref`; boundary literal alias lacks an admitted wire form | `S100` | `` string-literal union alias `Choice` cannot appear in a bound `` | `boundary-alias.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:551`; `resolve_type_ref`; literal alias reference has type arguments | `S100` | `` string-literal union alias `Choice` is not generic `` | `alias-local.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:562`; `resolve_type_ref`; type reference has no known declaration | `S016` | `` unknown type name `Iterable` `` | `annotation-iterable.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:581`; `resolve_union`; TsIntersectionType | `S100` | `intersection types are not in the decided surface` | `intersection.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:593`; `resolve_union`; union contains TsUndefinedKeyword | `S012` | `` `undefined` is banned; the single null story is `null` `` | `undefined-union.ts` | accepts | yes | ok | — |
| `check/tyres.rs:633`; `resolve_union`; nullable inner type fails allows_nullable | `S011` | `` unions are limited to `Ref \| null`; `i32 \| null` is not a re `` | `nullable-scalar.ts` | accepts | no | needs variant | GeneralUnionAndUndefined |
| `check/tyres.rs:659`; `resolve_union`; other union shape | `S011` | `` unions are limited to `Ref \| null` `` | `general-union.ts` | accepts | yes | ok | — |
| `check/tyres.rs:673`; `resolve_fn_type`; TsConstructorType | `S100` | `constructor types are not in the decided surface` | `constructor-type.ts` | accepts | no | needs variant | new |
| `check/tyres.rs:688`; `resolve_fn_type`; function type parameter Ident lacks type_ann | `S100` | `function type parameters require annotations` | `function-unannotated.ts` | TS7006 | no | ok | — |
| `check/tyres.rs:698`; `resolve_fn_type`; function type parameter is not Ident | `S100` | `function type parameter form outside the decided surface` | `function-rest.ts` | accepts | no | needs variant | new |
| `check/inference.rs:130`; `infer_call_arguments`; type parameter has no candidate | `S100` | `` cannot infer type parameter `T` of `empty`: no candidate; us `` | `corpus-r331-generic-inference-return-only.ts` | accepts | yes | ok | — |
| `check/inference.rs:149`; `infer_call_arguments`; type parameter candidates conflict | `S100` | `` cannot infer type parameter `T` of `pair`: conflicting candi `` | `pair-conditional.ts` | accepts | yes | ok | — |
| `check/bindings.rs:45`; `declare_local`; FnCtx::declare reports a duplicate local | `S017` | `` duplicate declaration of `value` in one scope `` | `corpus-r151-duplicate-const.ts` | TS2451 | no | ok | — |
| `check/bindings.rs:58`; `reject_pattern`; Pattern::Rejected supplies a reason | `S100` | `a default value in a binding pattern needs a rule for a miss` | `corpus-r216-pattern-default-value.ts` | accepts | yes | ok | — |
| `check/bindings.rs:95`; `pattern_source_fits`; IterResult pattern reads only done or value | `S100` | `` an iterator result cannot supply a binding pattern; use `con `` | `pattern-iterator-ok.ts` | accepts | yes | ok | — |
| `check/bindings.rs:97`; `pattern_source_fits`; other IterResult pattern | `S100` | `` an iterator result cannot supply a binding pattern; use `con `` | `pattern-iterator-extended.ts` | accepts | no | needs variant | new |
| `check/bindings.rs:118`; `pattern_source_fits`; array or field pattern has an unsupported source shape | `S100` | `` an array binding pattern reads a `T[]` or a `FixedArray<T, N `` | `pattern-string.ts` | accepts | yes | ok | — |
| `check/bindings.rs:226`; `error`; error caller supplies the rejection; forwarding builder | `S100` | `type annotation form outside the decided surface` | `annotation-tuple.ts` | accepts | no | needs variant | new |
| `check/bindings.rs:236`; `resolution_error`; resolution_error caller supplies the rejection; forwarding builder | `S016` | `` `default` is not exported by `./namespace-default-export-lib `` | `namespace-default-export.ts` | accepts | no | needs variant | new |
| `check/bindings.rs:249`; `error_diverging`; error_diverging caller supplies the rejection; forwarding builder | `S100` | `` cannot infer type parameter `T` of `pair`: conflicting candi `` | `pair-conditional.ts` | accepts | yes | ok | — |
| `check/namespace_import.rs:21`; `namespace_member_ident`; namespace member is absent from checker exports | `S016` | `` `default` is not exported by `./namespace-default-export-lib `` | `namespace-default-export.ts` | accepts | no | needs variant | new |
| `check/type_rules.rs:292`; `report_not_assignable`; different non-nullable class types | `S005` | `nominal types are not interchangeable: the argument expects ` | `corpus-r06-structural-substitution.ts` | accepts | yes | ok | — |
| `check/type_rules.rs:299`; `report_not_assignable`; other incompatible class-like types | `S005` | `nominal types are not interchangeable: the initializer expec` | `assign-nullable-classes.ts` | accepts | no | needs variant | NominalClassIdentity |
| `check/type_rules.rs:302`; `report_not_assignable`; different numeric types | `S007` | `` implicit numeric conversion from `i32` to `i64`; spell it `a `` | `mixed-width.ts` | accepts | yes | ok | — |
| `check/type_rules.rs:312`; `report_not_assignable`; value class targets a non-class-like Nullable type | `S011` | `` value class `C` cannot be nullable `` | `nullable-function-class.ts` | accepts | no | needs variant | GeneralUnionAndUndefined |
| `check/type_rules.rs:327`; `report_not_assignable`; other mismatch has a supplied or literal-alias variant | `S100` | `` type mismatch: the initializer expects `B`, got `A` `` | `assign-string-enum.ts` | accepts | yes | ok | — |
| `check/type_rules.rs:329`; `report_not_assignable`; other mismatch lacks a variant | `S100` | `` type mismatch: the initializer expects `i64[]`, got `i32[]` `` | `assign-array-width.ts` | accepts | no | needs variant | SizedOperandWidths |
| `check/capture.rs:437`; `finish`; capturing value reaches an escape position | `S009` | `` lambda capturing `local` may capture at return `` | `corpus-r10-escaping-capture.ts` | accepts | yes | ok | — |
| `check/capture.rs:448`; `finish`; a clean call parameter receives a capturing value | `S009` | `` call `forward` parameter `cb` requires a clean argument; lam `` | `corpus-r241-parameter-returned.ts` | accepts | yes | ok | — |
| `diag.rs:198`; `new`; Diagnostic::new caller supplies the rejection; forwarding builder | `S100` | `type annotation form outside the decided surface` | `annotation-tuple.ts` | accepts | no | needs variant | new |

Secondary observations retain the same physical site. They do not increase the inventory count.
The proposed variant column above uses the primary accepting witness.
The readonly and literal annotation probes need reasons beyond `NoTupleType` at the shared annotation site.
The map-width probe needs a width reason beyond the nullable-class reason at the shared class-like assignment site.

| Site | Code | Message, first 60 characters | Witness | tsc | Block | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| `parse.rs:245` | `S100` | `parse error: Invalid character in identifier` | `parse-identifier-surrogate.ts` | TS1127,TS7005 | no | ok |
| `check/lookup.rs:19` | `S100` | `` `C` cannot be used as a value because it was imported with ` `` | `type-only-value.ts` | TS1361 | no | ok |
| `check/lookup.rs:37` | `S100` | `` namespace import `ns` is a static qualifier and cannot be us `` | `namespace-call.ts` | TS2349 | no | ok |
| `check/lookup.rs:125` | `S100` | `` `x` is read from a different switch case `` | `lookup-switch-read.ts` | TS2454 | no | ok |
| `check/lookup.rs:154` | `S100` | `` `Foo` is read before its declaration in this block `` | `corpus-r155-class-read-before-declaration.ts` | TS2351,TS2448,TS2454 | no | ok |
| `check/lookup.rs:175` | `S100` | `` `x` is assigned before its declaration in this block `` | `lookup-pending-write.ts` | TS2448 | no | ok |
| `check/narrowing.rs:69` | `S011` | `` `Resource \| null` may be null here; narrow with a null check `` | `corpus-r195-using-nullable-member.ts` | TS18047 | no | ok |
| `check/narrowing.rs:69` | `S100` | `` type `((i32) => i32) \| null` is not callable `` | `nullable-function-initialized.ts` | accepts | no | needs variant |
| `check/generics.rs:65` | `S100` | `` type argument `i32` does not satisfy the constraint `string` `` | `constraint-rejected.ts` | TS2344 | no | ok |
| `check/generics.rs:283` | `S100` | `` `f` expects 1 type argument(s), got 2 `` | `generic-function-arity.ts` | TS2558 | no | ok |
| `check/generics.rs:375` | `S100` | `` `f` expects 1 type argument(s), got 2 `` | `generic-method-arity.ts` | TS2558 | no | ok |
| `check/generics.rs:492` | `S100` | `` `C` expects 1 type argument(s), got 2 `` | `generic-class-arity.ts` | TS2314 | no | ok |
| `check/text.rs:49` | `S100` | `` `encodeURI` is not generic `` | `uri-generic.ts` | TS2558 | no | ok |
| `check/tyres.rs:22` | `S100` | `` async functions must return an explicitly annotated `Promise `` | `async-keyword.ts` | TS1064 | no | ok |
| `check/tyres.rs:49` | `S100` | `` `Promise` requires exactly one fulfilled-value type argument `` | `async-promise-missing.ts` | TS2314 | no | ok |
| `check/tyres.rs:58` | `S100` | `` `Promise` requires exactly one fulfilled-value type argument `` | `async-promise-arity.ts` | TS2314 | no | ok |
| `check/tyres.rs:97` | `S100` | `type annotation form outside the decided surface` | `annotation-readonly.ts` | accepts | no | needs variant |
| `check/tyres.rs:97` | `S100` | `type annotation form outside the decided surface` | `annotation-literal.ts` | accepts | no | needs variant |
| `check/tyres.rs:214` | `S100` | `` generic reference class `Worker` requires explicit type argu `` | `worker-missing.ts` | TS2314 | no | ok |
| `check/tyres.rs:224` | `S100` | `` `Worker` takes exactly 2 type argument(s) `` | `worker-arity.ts` | TS2314 | no | ok |
| `check/tyres.rs:246` | `S100` | `` worker message type `i32` must be a plain reference class `` | `worker-scalar.ts` | TS2344 | no | ok |
| `check/tyres.rs:267` | `S100` | `` `RegExp` is not generic `` | `regexp-generic.ts` | TS2315 | no | ok |
| `check/tyres.rs:283` | `S100` | `` `Promise` requires exactly one fulfilled-value type argument `` | `promise-missing.ts` | TS2314 | no | ok |
| `check/tyres.rs:291` | `S100` | `` `Promise` requires exactly one fulfilled-value type argument `` | `promise-arity.ts` | TS2314 | no | ok |
| `check/tyres.rs:303` | `S100` | `` `FixedArray` requires element type and length arguments `` | `fixed-missing.ts` | TS2314 | no | ok |
| `check/tyres.rs:311` | `S100` | `` `FixedArray` takes exactly two type arguments `` | `fixed-arity.ts` | TS2314 | no | ok |
| `check/tyres.rs:343` | `S100` | `` `FixedArray` length must be a non-negative integer literal `` | `fixed-length-fraction.ts` | accepts | no | needs variant |
| `check/tyres.rs:419` | `S100` | `` `Error` is not generic `` | `error-generic.ts` | TS2315 | no | ok |
| `check/tyres.rs:430` | `S100` | `` generic reference class `Map` requires explicit type argumen `` | `map-missing.ts` | TS2314 | no | ok |
| `check/tyres.rs:439` | `S100` | `` `Map` takes exactly 2 type argument(s) `` | `map-arity.ts` | TS2314 | no | ok |
| `check/tyres.rs:495` | `S100` | `` `Date` is not generic `` | `date-generic.ts` | TS2315 | no | ok |
| `check/tyres.rs:512` | `S100` | `` `C` is not generic `` | `class-generic.ts` | TS2315 | no | ok |
| `check/tyres.rs:518` | `S100` | `` generic class `C` requires explicit type arguments `` | `class-missing.ts` | TS2314 | no | ok |
| `check/tyres.rs:551` | `S100` | `` string-literal union alias `Choice` is not generic `` | `alias-generic.ts` | TS2315 | no | ok |
| `check/tyres.rs:562` | `S016` | `` unknown type name `Missing` `` | `unknown-type.ts` | TS2304 | no | ok |
| `check/tyres.rs:562` | `S016` | `` unknown type name `ReadonlyArray` `` | `lib-interface.ts` | accepts | no | needs variant |
| `check/tyres.rs:633` | `S011` | `` unions are limited to `Ref \| null`; `i32[] \| null` is not a  `` | `value-nullable-array.ts` | accepts | no | needs variant |
| `check/tyres.rs:659` | `S011` | `` unions are limited to `Ref \| null` `` | `union-unknown-rejected.ts` | TS2304 | yes | wrong |
| `check/inference.rs:130` | `S100` | `` cannot infer type parameter `T` of `id`: no candidate; use e `` | `inference-missing-rejected.ts` | TS2554 | yes | wrong |
| `check/inference.rs:149` | `S100` | `` cannot infer type parameter `T` of `pair`: conflicting candi `` | `inference-conflict-rejected.ts` | TS2345 | yes | wrong |
| `check/bindings.rs:97` | `S100` | `` an iterator result cannot supply a binding pattern; use `con `` | `pattern-iterator-bad.ts` | TS2339 | no | ok |
| `check/bindings.rs:118` | `S100` | `` an array binding pattern reads a `T[]` or a `FixedArray<T, N `` | `pattern-invalid.ts` | TS2488 | yes | wrong |
| `check/bindings.rs:236` | `S016` | `` `missing` is not exported by `./namespace-missing-lib` `` | `namespace-missing.ts` | TS2339 | no | ok |
| `check/bindings.rs:249` | `S100` | `` cannot infer type parameter `T` of `pair`: conflicting candi `` | `inference-conflict-rejected.ts` | TS2345 | yes | wrong |
| `check/namespace_import.rs:21` | `S016` | `` `missing` is not exported by `./namespace-missing-lib` `` | `namespace-missing.ts` | TS2339 | no | ok |
| `check/type_rules.rs:299` | `S005` | `nominal types are not interchangeable: the initializer expec` | `nominal-null-rejected.ts` | TS2322 | no | ok |
| `check/type_rules.rs:299` | `S005` | `nominal types are not interchangeable: the initializer expec` | `assign-map-width.ts` | accepts | no | needs variant |
| `check/type_rules.rs:312` | `S011` | `` value class `C` cannot be nullable `` | `nullable-function-class-reject.ts` | TS2322 | no | ok |
| `check/type_rules.rs:327` | `S100` | `` type mismatch: the initializer expects `B`, got `A` `` | `alias-assignment-rejected.ts` | TS2322 | yes | wrong |
| `check/type_rules.rs:329` | `S100` | `` type mismatch: the initializer expects `string`, got `i32` `` | `assign-rejected.ts` | TS2322 | no | ok |
| `check/type_rules.rs:329` | `S100` | `` type mismatch: the initializer expects `(i64) => i64`, got ` `` | `assign-function-width.ts` | accepts | no | needs variant |
| `check/init_effects.rs:902` | `S100` | `` `second` is accessed before its declaration, directly from t `` | `corpus-r158-module-initializer-direct-read.ts` | TS2448,TS2454 | no | ok |
| `diag.rs:198` | `S100` | `` cannot infer type parameter `T` of `pair`: conflicting candi `` | `inference-conflict-rejected.ts` | TS2345 | yes | wrong |

The remaining rejected-class primary sites are `parse.rs:245`, `check/generics.rs:78`, `check/tyres.rs:688`, and `check/bindings.rs:45`.
The probes found no accepting witness at those guards. This is a measured result, not a proof over all TypeScript programs.
Their witnesses report TS1109, TS2313, TS7006, and TS2451, respectively.
The local overload and JSDoc probes do not produce an accepting witness at the last two guards.

The six unreachable guards require an absent source input, invalid internal records, or an unrepresentable HIR call count.
`instantiate_class` writes the instance arguments before it queues the deferred body.
Its template key comes from `generic_classes`, which no checker pass deletes.
The pipeline derives initializer segments from its top-level and global ranges.
The initializer scanner supplies summaries for each made function, constructor, method, and lambda.
The recursive generic and initializer probes reach no missing-record diagnostic.
The bodyless function, bodyless method, overload, and unknown-function initializer probes also reach no missing-record diagnostic.
`async-children-frame.ts` reaches the frame-member diagnostic at `layout.rs:413`; it does not overflow the call-count multiplication.

Boundary observations for the three required cases:

| Witness | Physical site | Code | tsc | Block |
| --- | --- | --- | --- | --- |
| `condition.ts` | `check/stmt.rs:535`, group b | S100 | accepts | no |
| `pair-conditional.ts` | `check/inference.rs:149`, group c | S100 | accepts | yes |
| `apply-id.ts` | `check/expr/literal.rs:394`, generic function value arm, group a | S100 | accepts | no |

Only the group c row enters this group's counts.

The missing-block sites below are grouped by the proposed reason.
Common builders carry an asterisk. Their rejecting callers need the variant; the builders need no unconditional variant.

`AggregateLayoutLimit`:

- `check/layout.rs:284`; `class_layout`; `class-final-align.ts`.
- `check/tyres.rs:369`; `resolve_type_ref`; `fixed-too-large.ts`.

`DeclarationScope`:

- `check/lookup.rs:125`; `lookup_local_access`; `lookup-switch-let-closure.ts`.
- `check/lookup.rs:154`; `lookup_local_access`; `lookup-shadow-class.ts`.

`GeneralUnionAndUndefined`:

- `check/tyres.rs:633`; `resolve_union`; `nullable-scalar.ts`.
- `check/type_rules.rs:312`; `report_not_assignable`; `nullable-function-class.ts`.

`IntegerLiteralRange`:

- `check/tyres.rs:328`; `resolve_type_ref`; `fixed-length-overflow.ts`.
- `check/tyres.rs:343`; `resolve_type_ref`; `fixed-length-negative.ts`.

`NoTupleType`:

- `check/tyres.rs:97`; `resolve_type_position`; `annotation-tuple.ts`.

`NominalClassIdentity`:

- `check/type_rules.rs:299`; `report_not_assignable`; `assign-nullable-classes.ts`.

`SizedOperandWidths`:

- `check/type_rules.rs:329`; `report_not_assignable`; `assign-array-width.ts`.

`WorkerContextAffinity`:

- `check/lookup.rs:194`; `lookup_local_access`; `lookup-context-capture.ts`.
- `check/container_argument.rs:49`; `container_argument`; `worker-array.ts`.

`new`:

- `provenance.rs:268`; `malformed`; `prov-malformed.ts`.
- `provenance.rs:279`; `duplicate`; `prov-duplicate.ts`.
- `hir/host_entry.rs:36`; `runner_main`; `runner-library-main.ts`.
- `check/lookup.rs:19`; `scope_item`; `type-only-local-class.ts`.
- `check/lookup.rs:37`; `scope_item`; `namespace-local-function.ts`.
- `check/lookup.rs:175`; `lookup_local_access`; `lookup-write-closure.ts`.
- `check/lookup.rs:203`; `lookup_local_access`; `capture-mutable.ts`.
- `check/narrowing.rs:69`; `nullable_use_error`; `nullable-initialized.ts`.
- `check/host_entries.rs:26`; `entry_file`; `entry-ambiguous.ts`.
- `check/generics.rs:65`; `apply_type_parameter_constraints`; `constraint-structural.ts`.
- `check/generics.rs:283`; `instantiate_fn`; `generic-function-default-explicit.ts`.
- `check/generics.rs:375`; `instantiate_method`; `generic-method-default-explicit.ts`.
- `check/generics.rs:492`; `instantiate_class`; `generic-class-default-explicit.ts`.
- `check/mirror_provenance.rs:33`; `collect_mirror_provenance`; `prov-header.ts`.
- `check/mirror_provenance.rs:61`; `collect_mirror_provenance`; `prov-target-parameter.ts`.
- `check/mirror_provenance.rs:75`; `collect_mirror_provenance`; `prov-target-callback.ts`.
- `check/mirror_provenance.rs:120`; `collect_mirror_provenance`; `prov-target-lifetime.ts`.
- `check/mirror_provenance.rs:166`; `foreign_parameter_provenance`; `prov-array.ts`.
- `check/mirror_provenance.rs:180`; `foreign_parameter_provenance`; `prov-string.ts`.
- `check/mirror_provenance.rs:193`; `foreign_parameter_provenance`; `prov-incompatible.ts`.
- `check/mirror_provenance.rs:217`; `callback_provenance`; `callback-anonymous-field.ts`.
- `check/mirror_provenance.rs:233`; `callback_provenance`; `callback-alias-field.ts`.
- `check/text.rs:49`; `check_uri_call`; `uri-local-generic.ts`.
- `check/tyres.rs:22`; `resolve_async_return`; `async-parenthesized.ts`.
- `check/tyres.rs:31`; `resolve_async_return`; `async-qualified.ts`.
- `check/tyres.rs:40`; `resolve_async_return`; `async-alias.ts`.
- `check/tyres.rs:49`; `resolve_async_return`; `async-default-alias.ts`.
- `check/tyres.rs:58`; `resolve_async_return`; `async-arity-alias.ts`.
- `check/tyres.rs:180`; `resolve_keyword`; `keyword-unknown.ts`.
- `check/tyres.rs:193`; `resolve_type_ref`; `qualified.ts`.
- `check/tyres.rs:214`; `resolve_type_ref`; `worker-local-default.ts`.
- `check/tyres.rs:224`; `resolve_type_ref`; `worker-local-one.ts`.
- `check/tyres.rs:246`; `resolve_type_ref`; `worker-local-scalar.ts`.
- `check/tyres.rs:267`; `resolve_type_ref`; `regexp-local.ts`.
- `check/tyres.rs:283`; `resolve_type_ref`; `promise-default-alias.ts`.
- `check/tyres.rs:291`; `resolve_type_ref`; `promise-arity-alias.ts`.
- `check/tyres.rs:303`; `resolve_type_ref`; `fixed-default-alias.ts`.
- `check/tyres.rs:311`; `resolve_type_ref`; `fixed-one-alias.ts`.
- `check/tyres.rs:390`; `resolve_type_ref`; `array-two-alias.ts`.
- `check/tyres.rs:403`; `resolve_type_ref`; `generator-no-argument.ts`.
- `check/tyres.rs:419`; `resolve_type_ref`; `error-local.ts`.
- `check/tyres.rs:430`; `resolve_type_ref`; `map-local-default.ts`.
- `check/tyres.rs:439`; `resolve_type_ref`; `map-local-one.ts`.
- `check/tyres.rs:495`; `resolve_type_ref`; `date-local.ts`.
- `check/tyres.rs:512`; `resolve_type_ref`; `class-local.ts`.
- `check/tyres.rs:518`; `resolve_type_ref`; `class-default.ts`.
- `check/tyres.rs:541`; `resolve_type_ref`; `boundary-alias.ts`.
- `check/tyres.rs:551`; `resolve_type_ref`; `alias-local.ts`.
- `check/tyres.rs:562`; `resolve_type_ref`; `annotation-iterable.ts`.
- `check/tyres.rs:581`; `resolve_union`; `intersection.ts`.
- `check/tyres.rs:673`; `resolve_fn_type`; `constructor-type.ts`.
- `check/tyres.rs:698`; `resolve_fn_type`; `function-rest.ts`.
- `check/bindings.rs:97`; `pattern_source_fits`; `pattern-iterator-extended.ts`.
- `check/bindings.rs:226` *; `error`; `annotation-tuple.ts`.
- `check/bindings.rs:236` *; `resolution_error`; `namespace-default-export.ts`.
- `check/namespace_import.rs:21`; `namespace_member_ident`; `namespace-default-export.ts`.
- `diag.rs:198` *; `new`; `annotation-tuple.ts`.
