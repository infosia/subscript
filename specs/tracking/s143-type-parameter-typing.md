# §143 — Type parameter typing

Contract: `specs/blocks/compiler/s143-a-type-parameter-is-typed-as-tsc-types-it.md`.

## Form and retained measurements

`TypeParameter(Box<TypeParameterType>)` carries identity, source name, and constraint.
`apparent_type` resolves direct constraints by identity, with a cycle guard.
Assignable values preserve parameter identity and follow each source constraint.
`GenericNumber` preserves the unsized number result. `GenericUnion(Box<[Type]>)` preserves union results.
The opaque check reports all diagnostics. The §135 rule 3 merge removes repeated reports by site.
No opaque instance reaches HIR. Its conservative match arms cite §143 and return their error-type result.

The pin and final `Type` sizes are 24 bytes. The inline parameter prototype measured 40 bytes.
The boxed parameter payload is 8 bytes; the boxed union slice is 16 bytes.
The type-layout unit test protects the interpreter completion layout.

The first rule 5 prototype checked 387 programs and 511 sources, with zero rejected programs.
Pin checker time was 1.405280042 seconds; the final first implementation took 1.436261750 seconds, 2.20 percent more.
The first 320-cell matrix had 80 failures. The release CLI build passed in 18.55 seconds.
The ship copy of a12 printed `42:2.5`, equal to its golden.

Red entries r290–r293 accept at `1b31a9a9` and reject after §143.
Their measured TypeScript codes are TS2365, TS2322, TS2564, and TS2339.
Concrete r294 pins the pre-existing S100/C20 rule; TypeScript accepts it.
r295 is Red at `36684546`; r296 is Red at `cf7cce94`.
a305 is Red at `2e5a8dfe`, with three S014 errors.

## Earlier stops

Round 1 stopped on six E0004 errors at the type-form compile probe; two files were outside the file set.
Round 2 stopped on the per-instance field-diagnostic test outside that file set.
Rounds 3 and 4 stopped on missing comparison and do-while restriction records.
The owner supplied C20 and compiler.md §124. Round 5 stopped on two gate failures.
Round 6 boxed the type payload and deferred Map/Set key kinds; both gate failures closed.
The API expansion stopped on a gate failure with 65 cells, then on 24 FixedArray length cells without a record.
The owner supplied the Q3 literal-length record. The final round 3 landing gate passed.
No production change was reverted after the handoff prohibited reverts.

### Restriction admission tests

Every test below requires a type that involves a parameter or a generic number result.
The apparent-type test applies before the project restriction defers.
Call arguments, callback signatures, assignment destinations, and cast overlap keep their separate type checks.

| Restriction | Apparent-type admission test | Project restriction |
|---|---|---|
| `TemplateInterpolation` | A resolved value type | Runtime formatter and concatenation kind |
| `ValueField` | A resolved field type | Value-class field layout |
| `CompositeAssignability` | Matching composite constructors; recursive component typing and function variance | Exact concrete component types at an instance |
| `SizedNumeric` | A numeric type, numeric enum, or the distinct `number` result | Numeric width and sized result assignment |
| `UnaryNumeric` | A value other than null, nullable, or void | Unary coercion and integer representation |
| `BooleanContext` | A type other than a required function | Concrete boolean operand or condition |
| `AssociativeKey` | A resolved type argument | Concrete hash and equality kind |
| `ArrayElementKind` | A resolved element or callback result type | Runtime callback and equality-search representation |
| `PartialValueLayout` | A resolved result type | Nullable-pointer result representation |
| `ContainerArgument` | A resolved type argument | Context affinity |
| `NullableShape` | A resolved nullable base type | Nullable reference representation |
| `AggregateLayout` | A fixed-array type | Concrete aggregate byte layout |
| `SwitchKind` | A resolved discriminant type | Concrete dispatch kind |
| `CastKind` | A resolved type, with overlap checked at the cast | Runtime conversion kind |
| `RelationalKind` | A non-nullable value other than void or a function; operand overlap is required | Concrete relational operand kind |
| `JsonSerializability` | A resolved type with no error | Concrete JSON graph and serialization support |
| `ByteAccessTarget` | A resolved type with no error | Concrete value layout and byte access support |

## Fixes and tests

| Fix | Test |
|---|---|
| A parameter carries its constraint and its identity; every opaque diagnostic remains. | `opaque_generics`; r290–r293; type parameter unit tests |
| Boxed parameter and union payloads preserve the interpreter layout. | Type layout unit test; `async_completion_layout` |
| Map/Set key, container, callback, nullable, and aggregate restrictions defer by the named admission tests. | `container_kinds_and_layouts_are_checked_per_instance`; growing-instance test; matrix container forms |
| Instance columns require admitted concrete controls; omission counts are assertions. | `generic_forms_follow_tsc`; exported omission lists |
| Restriction records resolve through their own boundaries and must contain their restriction token. | Record unit tests, including absent tokens and adjacent sections |
| Compound assignments apply TypeScript operand typing before project restrictions. | `compound_operands_and_callback_conditions_keep_typescript_typing`; `an_accessor_compound_result_must_fit_the_setter`; r295 |
| Fresh numeric results keep `number` at every consumer and in diagnostics. | `fresh_number_has_no_sized_operand_kind`; matrix equality, index, and bitwise sites |
| Constraints resolve by identity in both orders, and assignability walks every intermediate parameter. | `constraints_resolve_by_identity_in_both_declaration_orders`; `a_parameter_is_assignable_through_each_intermediate_constraint` |
| Case labels compare through the apparent type when either operand involves a parameter. | `a_parameter_case_label_compares_with_a_concrete_discriminant` |
| Cyclic constraints report S100 at their declaration. | `direct_constraint_cycles_report_the_declaration`; r296 |
| Compound diagnostic text names the operator and both operand types. | `compound_diagnostics_name_the_operator_and_both_operands`; checker diagnostic unit test |
| Binding patterns read constrained source shapes; ternaries and arrays preserve generic unions. | Derived binding, conditional, and array matrix cells |
| Every apparent union member must supply a shared field or method. | `every_union_member_must_supply_the_member`; derived shared-member cells |
| Error-constrained throws use the apparent type; other throws retain the Error-family restriction. | Derived throw cells and their rejected control |
| Arrow defaults check assignability against the declared parameter type. | `lambda_defaults_use_the_declared_parameter_type` |
| Type arguments follow their constraint before numeric normalization; callable fields use the apparent type. | Derived forwarded-constraint and callable-field cells |
| The shape scan derives methods and free functions from types.rs and checks typed comparisons and collection searches. | Eight `apparent_type_shapes` tests |
| JSON and byte-access targets defer by the named rule 2a tests; opaque JSON creates no helper. | `json_and_bytes_restrictions_wait_for_admitted_instances`; a305 |
| JSON graph insertion and lookup preserve declared keys; diagnostics use type names. | Corpus and matrix diagnostic checks for Rust Debug text |
| Callback parameter variance uses parameter constraints; concrete signatures keep exact project kinds. | Callback accept/reject controls in `opaque_generics` |
| Numeric byte-element widths defer after container shape and target identity checks. | `byte_arguments_read_constraints_and_defer_numeric_element_widths` |
| Context.free, byte-value equality, fixed callback arity, and FixedArray length name existing restriction records. | API concrete controls; Q3 bullet record tests |

### Changed expectations

| Test | Old expectation | New expectation | Rule / measured TypeScript result |
|---|---|---|---|
| field_values::a_generic_class_is_checked_per_instance (renamed to a_generic_class_reports_each_site_once) | Two missing-field reports for two instances | One report at the template field | §143 rules 2 and 3; TS2564 |
| opaque_generics::each_kept_kind_is_reported_in_an_uninstantiated_body, arithmetic control | Accepts `x + 1` | One S100 | §143 rule 1; TS2365 |
| Same test, string initializer control | Accepts string from T | One S100 | §143 rule 1b; TS2322 |
| Same test, class field assignment control | Accepts i32 from T | One S100 | §143 rule 1b; TS2322 |
| Same test, nullable parameter control | Accepts an unnarrowed member read | One S011 | §143 rule 1a; TS18047 |
| Same test, constrained callable control | Accepts a call of class-constrained T | One S100 | §143 rule 1a; TS2349 |
| a_form_that_tsc_rejects_for_a_type_parameter_is_left_to_the_instance (renamed) | No report without an instance; concrete operator report with one | One opaque S100 in either case | §143 rules 1 and 2; TS2365 |
| Same test, unknown-name firing control | One S016 | S016 and S100 | §143 rule 2; TS2304 and TS2365 |
| a_language_restriction_in_an_uninstantiated_body_is_left_to_the_instance (renamed) | No report without an instance | One S100 for a concrete string value field | §143 rule 2; tsc accepts, C2 |
| Same test, unknown-name firing control | One S016 | S016 and S100 | §143 rule 2; TS2304 and C2 |
| a_shared_location_kill_is_kept_in_an_uninstantiated_body, parameter control | Accepts an unnarrowed T-or-null read | One S011 | §143 rule 1a; TS18047 |

The type-parameter control sources in the kept-kind table stay intact and have exact diagnostic assertions.
The accepted-form table retains all forms and each unknown-name firing control.
`each_dropped_form_is_accepted` is renamed to `each_tsc_valid_form_is_accepted`; its accepted verdicts stay unchanged.
The operator and value-field tests include accepted controls.


The compound diagnostic unit test keeps S100 and its diagnostic count.
Its text changes from `compound assignment is not defined for boolean` to an operator and both operand types.
No existing verdict changes in the later fixes.

## Final matrix and source scan

The matrix derives value roles, consumer sites, and ambient API positions.
The API scope is `prelude/lang.d.ts`. ES-lib methods have product cells only where a consumer site names them.
The compound product keeps `+=`, `-=`, and `&=`, one per checker arm.
The other operators had equal verdicts across 633 measured groups. A separate test keeps all eleven operators.
TypeScript starts before the checker loop; up to eight scoped workers check separate slices.

The kind scan reads 37 variants from `compiler/src/types.rs`.
It gives 31 variants legal constraints and lists six internal or unavailable variants with reasons.
Class constraints include reference, value, and generic classes. Numeric constraints include every sized width.
The matrix has 36 kinds: nine existing kinds and 27 added kinds.
The added kinds retain every site with `x: T` and `T | null`; nine numeric added kinds also use `x + 1`.
This cut bounds debug cost below 25 seconds. Existing kinds retain all 20 roles.

| Axis or measurement | Final result |
|---|---:|
| Consumer sites | 57 |
| Current kind × role pairs | 180 |
| Added kind × role pairs | 63 |
| Product candidates | 27,702 |
| Product omitted instances | 7,532 |
| Product retained cells | 20,170 |
| Ambient callables / positions | 42 / 96 |
| API candidates / omitted instances | 20,736 / 8,775 |
| API retained cells | 11,961 |
| Additional form cells | 1,278 |
| Destination candidates / omitted instances | 3,360 / 712 |
| Destination retained cells | 2,648 |
| Total cells | 36,057 |
| No-instance / instance columns | 26,738 / 9,319 |
| Final failures | 0 |
| Debug matrix cost | 22.113 seconds |
| Debug cost with cleanup | 24.86 seconds |
| Final matrix test target, with cleanup | 26.17 seconds |
| Separate compound test | 396 cells, zero failures, 0.381 seconds |
| Shape scan | 520 sites, 19 groups, 49 justified raw-test functions, 0.25 seconds |

Regenerate inventories through the matrix's optional exports:

```sh
SUBSCRIPT_API_SITES=target/s143-api-sites.txt \
SUBSCRIPT_API_OMISSIONS=target/s143-api-omissions.txt \
SUBSCRIPT_API_CONTROLS=target/s143-api-controls \
SUBSCRIPT_MATRIX_OMISSIONS=target/s143-product-omissions.txt \
SUBSCRIPT_MATRIX_FAILURES=target/s143-matrix-failures.txt \
cargo test --offline --locked -p subscript-compiler --test generic_tsc_matrix -- --nocapture
```

`SUBSCRIPT_SHAPE_INVENTORY` exports the shape scan inventory. No per-cell table is kept here.

## Final review round 4 fix round

The C21 fix adds `Divergence::VoidValue`, its table entry, and the divergence on both void-value diagnostics. The reject table adds r297 (S100, line 11) and r298 (S100, line 10). Direct tests cover the entry and both diagnostics. Both requested tests and one complete `cargo test --offline --locked -p subscript-compiler` pass. `cargo fmt --check` and `git diff --check` pass. No gate runs, no existing golden changes, and no commit occurs.

Pin: `c5bad823`. All changes remain uncommitted.

`generic_overlap` admits numeric enum/number pairs and string-alias/string pairs in both orders.
The new kind axis exposed enum numeric-operation and f16-constraint deferral defects.
`SizedNumeric` admits enum constraints; numeric operations on them produce `GenericNumber`.
Numeric literals in a parameter context keep `GenericNumber`, so no apparent constraint fixes their storage width.
The f16 arithmetic test defers only a parameter operand, and still rejects a concrete f16 operand beside a parameter.
The two added `opaque_generics` tests cover casts, class receivers, both instance columns, numeric results, and rejected controls.
The full site axis includes casts to i32, u8, and string.

`a306-generic-enum-index` is Red at the pin: three S100 cast errors, with and without instances.
TypeScript 5.9.2 accepts both versions with the corpus options.
The final dev and ship CLIs print `b`, `b`, `b`, byte-equal to the new golden.
The ship probe uses a temporary source copy. No existing golden changes.
The new u8 result uses an explicit i32 cast before array indexing, as the concrete index rule requires.
The corpus index is regenerated through its generator.

The API syntax walker emits named omissions for constructors, construct/call signatures, properties, accessors, indices, and variables.
It fails on an unknown ambient member kind.
`new_callables_and_unexpressed_signatures_are_reported` adds a public constructor and a function-valued property.
The prelude now reports eight named omitted declarations; it retains all 42 callables and 96 positions.
The shape-scan header states its current measured counts.

Additional kind/site records cover non-array patterns (§107), alias case labels (§41), affine fields/captures (§40), and dropped async handles (C8).
Plain-string calls on alias constraints cite Q32. Null inference cites §97.
The multi-record check permits separate codes only when each code has a checked restriction record.
The API builder caches equal concrete controls and does not repeat its admission check in the checker loop.
This removes duplicate work without a cell cut. Cost drops from 25.48 to 22.27 seconds with cleanup.
The owner supplied C21 for the six void-related cells. Each cell names S100 and a token from that record.

### Rule 5 corpus measurement

The temporary harness uses `codegen/tests/corpus/mod.rs` entry discovery and source loading.
It copies the current trap loader functions directly, including all ambient mirrors.
It discovers examples recursively and adds the engine mirror. No source set is hand-listed.
Times exclude discovery and compilation; they include reject-corpus checks for diagnostic Debug text.

| Measurement | Pin | Final, same corpus | Final, with a306 |
|---|---:|---:|---:|
| Programs | 388 | 388 | 389 |
| Sources, including mirrors | 512 | 512 | 513 |
| Rejected programs | 0 | 0 | 0 |
| Debug checker seconds | 1.624195792 | 1.656287250 | 1.632653417 |

The same-corpus time increases 1.98 percent. Each number is one pass, not a median.
The temporary harness is removed after measurement.
No existing corpus or example becomes rejected. No public library API is added.

### Void record completion

C21 states the void-value restriction. Five product cells cite its binding token; the FixedArray.map cell cites its callback token.
`r297-void-binding` and `r298-void-map-callback` name C21 and carry measured `tsc: accepts` headers.
TypeScript 5.9.2 accepts both with the corpus options.
Both entries pin pre-existing rules, not Red regressions. The checker rejects r294, r297, and r298 identically at `51bf01a5`.
The CLI built from HEAD `c5bad823` reports these exact results:

```text
r297: S100 at 11:9: cannot bind a `void` value
r298: S100 at 10:10: the `map` callback must return a value
```

The generator regenerates the corpus index. No existing golden changes.
The matrix retains 33,401 cells and both columns, with zero failures in 20.288 seconds.
Rule 5 checks 389 programs and 513 sources, with zero rejected programs in 1.665499667 seconds.
The temporary harness uses current corpus loaders and is removed after measurement.
No public library API is added. No file-scope stop occurs. No commit occurs.
`cargo fmt --check` and `git diff --check` pass.
The quick gate builds all targets in 14.23 seconds, then stalls in the debug exception suite.
`a_worker_exception_stays_the_worker_trap_under_an_await` gives no result while the other 34 exception tests pass.
The preceding saved quick gate completes all 35 exception tests in 4.05 seconds.
This run remains at that test more than ten minutes after gate start.
The coding agent interrupts the stalled gate; it exits 1 without a verdict line.
The gate interrupt handler removes its incomplete record. No verdict line is fabricated.
This is the gate stop. No gate retry or out-of-scope fix occurs.

## Final review round 5 fix round

Constraint assignability recurses through nullable, array, fixed-array, Map, Set, generic-class, function, worker, endpoint, and coroutine components.
Function parameters use contravariance; returns use covariance. Concrete assignments keep their exact component rules.
`CompositeAssignability` names that rule 2a restriction. Component typing applies before the restriction defers.
`composite_constraints` covers linked identities, unrelated parameters, variance, constructor identity, lengths, and concrete-instance rejection.
Four derived matrix forms cover nullable links, nullable type arguments, function variance, and nested containers.
Equality diagnostics print declared operand types; the direct test requires `T` and `U`.

The destination axis reads composite variants from `Type`, with reasons for three internal forms.
It adds 16 destinations, including identity, at initializer, return, and argument sites for all 35 constrained kinds.
It retains 2,648 cells: 1,680 without instances and 968 with instances. An assertion pins 712 omitted concrete controls.
Existing records cover nullable representation, key kinds, message classes, and affine storage.
The API test pins all eight omitted declaration names. A new unlisted omission fails its direct test.
C21 tokens now match the narrowed inferred-binding and callback text. No collision record is edited by the coding agent.
The shape scan checks 520 sites in 0.250 seconds; its assignability fingerprint includes the new structural branches.

Rule 5 uses current corpus loaders, including the copied trap loaders and all ambient mirrors.
It checks 389 programs and 513 sources: zero rejected programs, 1.578234875 seconds in debug.
The temporary measurement harness is removed. No existing golden changes. No public library API is added.
The source tree of the measured pin CLI matches `51bf01a5` for every compiler source file.
That CLI gives S100 at r294 line 11, r297 line 11, and r298 line 10, with the same messages as HEAD.
TypeScript 5.9.2 accepts all three. These entries pin pre-existing restrictions, not Red regressions.

The expanded matrix retains 36,057 cells: 26,738 without instances and 9,319 with instances.
One decision remains: `function g<T extends void>(x: T): void { return x; }`.
TypeScript accepts it. The checker reports S100, "a `void` function cannot return a value".
Its concrete control, `function g(x: void): void { return x; }`, gives the same S100 at `51bf01a5` and HEAD.
No existing record states this return-expression restriction. C21 states inferred bindings and map callbacks only.
The cell remains failing under rule 4b. This is the missing-record stop; no new record or compiler verdict is invented.
The final matrix takes 22.113 seconds, or 24.86 seconds with cleanup. No cell cut is needed.
`cargo test --offline --locked -p subscript-compiler --no-fail-fast` runs all 52 targets.
It reports 866 passed, one failed, and one ignored test. Only the missing-record matrix cell fails.
`cargo fmt --check` and `git diff --check` pass. No `tools/gate.sh` run occurs, so no gate verdict line exists.
All authorized changes remain uncommitted. No file-scope or corpus-rejection stop occurs.

The void-return cell now names S100 and C21, with the token "`void` function cannot return a value".
`r299-void-return-value` and its reject-table row pin this pre-existing rule, not a Red regression.
The CLI from `51bf01a5` reports ``error[S100]: a `void` function cannot return a value`` at 9:3.
TypeScript 5.9.2 exits 0 with the corpus options. The current diagnostic names `Divergence::VoidValue`; its direct test passes.
The generator adds r299 to the corpus index. The 36,057-cell matrix passes; its complete test target takes 26.17 seconds.
`cargo test --offline --locked -p subscript-compiler` passes all 52 targets: 867 passed, zero failed, one ignored.
`cargo fmt --check` and `git diff --check` pass. No collision record or existing golden changes in this repair.
No gate runs, no gate verdict line exists, and no commit occurs. No stop condition remains.

## Final Rust file sizes

| File | Lines |
|---|---:|
| `compiler/src/check/bindings.rs` | 250 |
| `compiler/src/check/bodies.rs` | 845 |
| `compiler/src/check/class_shape.rs` | 979 |
| `compiler/src/check/container_argument.rs` | 93 |
| `compiler/src/check/exception.rs` | 591 |
| `compiler/src/check/expr.rs` | 553 |
| `compiler/src/check/expr/aggregate.rs` | 400 |
| `compiler/src/check/expr/array_of_and_map_copy.rs` | 134 |
| `compiler/src/check/expr/assign.rs` | 624 |
| `compiler/src/check/expr/call.rs` | 1565 |
| `compiler/src/check/expr/entry.rs` | 679 |
| `compiler/src/check/expr/lambda.rs` | 248 |
| `compiler/src/check/expr/literal.rs` | 555 |
| `compiler/src/check/expr/member.rs` | 590 |
| `compiler/src/check/expr/method.rs` | 1591 |
| `compiler/src/check/expr/namespace.rs` | 1282 |
| `compiler/src/check/expr/operator.rs` | 1516 |
| `compiler/src/check/generics.rs` | 651 |
| `compiler/src/check/instance_chain.rs` | 269 |
| `compiler/src/check/json.rs` | 1628 |
| `compiler/src/check/lookup.rs` | 212 |
| `compiler/src/check/mod.rs` | 1389 |
| `compiler/src/check/narrowing.rs` | 333 |
| `compiler/src/check/opaque.rs` | 678 |
| `compiler/src/check/pipeline.rs` | 410 |
| `compiler/src/check/signatures.rs` | 552 |
| `compiler/src/check/stmt.rs` | 1544 |
| `compiler/src/check/type_rules.rs` | 333 |
| `compiler/src/check/tyres.rs` | 680 |
| `compiler/src/divergence.rs` | 1325 |
| `compiler/src/hir/shared.rs` | 259 |
| `compiler/src/hir/sites.rs` | 583 |
| `compiler/src/types.rs` | 1307 |
| `compiler/tests/apparent_type_shapes.rs` | 791 |
| `compiler/tests/apparent_type_shapes/comparisons.rs` | 200 |
| `compiler/tests/composite_constraints.rs` | 89 |
| `compiler/tests/corpus_reject.rs` | 1366 |
| `compiler/tests/corpus_warn.rs` | 216 |
| `compiler/tests/field_values.rs` | 646 |
| `compiler/tests/generic_tsc_matrix.rs` | 1774 |
| `compiler/tests/generic_tsc_matrix/api.rs` | 536 |
| `compiler/tests/generic_tsc_matrix/destinations.rs` | 253 |
| `compiler/tests/generic_tsc_matrix/findings.rs` | 108 |
| `compiler/tests/generic_tsc_matrix/kinds.rs` | 339 |
| `compiler/tests/generic_tsc_matrix/product.rs` | 560 |
| `compiler/tests/opaque_generics.rs` | 1091 |

Every listed Rust file has fewer than 2,000 lines.

## Retained gate lines

The two failed quick gates caused stops. Later owner-authorized repair rounds passed their gates.
The owner ran the full landing gates.

```text
gate quick a9230193d22bebd979f635aaba67afc223bb9fa0 dirty:36 debug 2134/2/3 skips 2 goldens-moved 0 exit 1
gate quick a9230193d22bebd979f635aaba67afc223bb9fa0 dirty:37 debug 2138/0/3 skips 2 goldens-moved 0 exit 0
gate quick a9230193d22bebd979f635aaba67afc223bb9fa0 dirty:37 debug 2140/0/3 skips 2 goldens-moved 0 exit 0
gate full a9230193d22bebd979f635aaba67afc223bb9fa0 dirty:37 debug 2140/0/3 release 2137/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
gate quick 616eccd34085f4b5739814a16a9a79ecbdc26251 dirty:12 debug 2145/0/3 skips 2 goldens-moved 0 exit 0
gate full 616eccd34085f4b5739814a16a9a79ecbdc26251 dirty:12 debug 2145/0/3 release 2142/0/3 skips 2/0 clippy 4/18/13 goldens-moved 0 exit 0
gate quick cf7cce9480bb85b8e47ed574d32e27e29c784abe dirty:13 debug 2149/0/3 skips 2 goldens-moved 0 exit 0
gate full cf7cce9480bb85b8e47ed574d32e27e29c784abe dirty:13 debug 2149/0/3 release 2146/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
gate quick 1e901ca96f6856195cfafa856a92eb8b002fe35d dirty:29 debug 2157/0/3 skips 2 goldens-moved 0 exit 0
gate full 1e901ca96f6856195cfafa856a92eb8b002fe35d dirty:29 debug 2157/0/3 release 2154/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
gate quick d901355d8e34bf47aec1f238a097b0dd1fd39f09 dirty:15 debug 2159/0/3 skips 2 goldens-moved 0 exit 0
gate full d901355d8e34bf47aec1f238a097b0dd1fd39f09 dirty:15 debug 2159/0/3 release 2156/0/3 skips 2/0 clippy 3/18/13 goldens-moved 0 exit 0
gate quick 2e5a8dfef8dbf831b3dde682ebbb7e120560473c dirty:17 debug 2162/1/3 skips 2 goldens-moved 0 exit 1
gate quick 2e5a8dfef8dbf831b3dde682ebbb7e120560473c dirty:19 debug 2166/0/3 skips 2 goldens-moved 0 exit 0
gate full 2e5a8dfef8dbf831b3dde682ebbb7e120560473c dirty:19 debug 2166/0/3 release 2163/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

## Final review round 4 landing gate

The orchestrator added C21 to `collisions.md`: the checker held the
rule, and no record stated it. The first full gate on the round 16 tree
failed on two omissions (no C21 divergence variant, no reject-table rows
for `r297` and `r298`); round 17 added both.

```text
gate full c5bad823a1826a58bf7945b6dc872a6d4bf57417 dirty:22 debug 2171/0/3 release 2168/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

## Final review round 5 landing gate

The orchestrator narrowed C21 to inferred bindings and added the `void`
return form to it, measured at `5f707367`.

```text
gate full 5f707367d052e9e7e3a6f9e8c53920d958d1b9c6 dirty:17 debug 2177/0/3 release 2174/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```
