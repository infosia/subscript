# §163: An indexed read is not narrowed

## Measurements

The initial CLI measurement uses contract pin `873d084a`.
Each CLI input is a copy of the corpus entry in `$TMPDIR`.
Each `tsc` project extends the repository `tsconfig.json` and includes the prelude and one copied entry.
The measured TypeScript version is 5.9.2.

| Entry | `tsc` result | CLI result | Site class |
|---|---|---|---|
| `r373-indexed-read-after-null-check` | accepts, exit 0 | S011 at 11:34, exit 1 | `NullableMember`, `TscRejects`; no divergence block |
| `r374-indexed-read-without-null-check` | TS2531 at 11:12, exit 2 | S011 at 11:12, exit 1 | `NullableMember`, `TscRejects`; no divergence block |

Both CLI diagnostics give this message:

```text
`A | null` may be null here; narrow with a null check first
```

Both CLI diagnostics give this rule text:

```text
Unions are limited to nullable handles, including boundary boxes; nullable values must be narrowed before member access.
```

Both entries require the copy message of §163 rule 2.
Entry r373 also requires the C24 row 32 divergence block.
Entry r374 keeps the `TscRejects` class.

## Implementation

`NarrowingFact` carries a `FactKind`: `Narrowing` or `ElementCheck`.
An element-check fact carries the receiver path, the key identity, and its const-binding dependencies.
Only narrowing facts change a type or authorize an absence-capable read.
The readers are `expr/literal.rs`, `expr/member.rs`, `exception.rs`, the lambda entry filter, and the final loop-store check.

An indexed path requires a receiver path and an integer literal or a const local key.
A direct integer-literal const initializer, with or without an annotation, gives its key the integer value identity.
A const alias of such a local keeps that integer value identity.
Other const keys use the declaration position and binding name.
Computed initializers and separate destructured bindings keep declaration identities.
Integer value identities survive key-scope exits; declaration identities retain their scope dependencies.
A mutable key, a computed key, or a receiver without a path supplies no element fact.

`hir/effects.rs` records exact receiver stores and indexed stores, including unknown keys.
A receiver store, a same-element store, or a join with an unchecked incoming path ends an element-check fact.
Calls, different-key stores, and unknown-key stores keep the fact.
The loop summaries carry these store identities.
Scope exits discard facts whose const keys leave scope.
Loop and switch exits discard inner receiver facts and restore hidden outer facts.
These operations read the live facts and named bindings, not all scope bindings.
The existing §124 and §162 ends for narrowing facts remain unchanged.

The §161 work visits the current expression and the live facts.
A const-key lookup reads the named binding in its lexical scope.
No point scans all function paths or locals.

The nullable member, call, and assignment sites each split into checked and unchecked indexed reads.
Nullable function arguments and stores use the S100 indexed-call site pair with the copy message.
The checked sites carry `IndexedReadNullCheck`, with the C24 row 32 fragment.
The unchecked sites carry no divergence block.
Each diagnostic gives the copy message and the note of §163 rule 2.
`codegen/src/lir/place.rs` adds only the forced `..` pattern.

## Corpus

The CLI comparison against code pin `4286e511` covers 611 TypeScript entries: 294 accept entries and 317 reject entries.
The per-file record contains both exit statuses and both sorted rule-code lists.
234 files exit 0 in both binaries; 377 files exit 1 in both binaries.
Every file keeps its rule-code list.
Every exit status remains unchanged between the pin binary and the new binary.
Only the diagnostics of r373 and r374 change.
The standalone CLI comparison supplies no ambient dependency files.
The compiler corpus tests supply each entry's required inputs.

No existing reject header names the old S011 message for an indexed read.
The message-migration list is empty.
Entry r195 names a nullable local, so its header remains unchanged.
The new entries r373 and r374 keep their copy-message headers.
The reject test table adds both entries.

The reference generator changes only `generated-docs/corpus-index.md`.
The API reference and the language reference remain byte-identical.
No existing `.expected` golden changes.

## Tests

`compiler/tests/indexed_read_facts.rs` supplies thirteen tests with same-shape controls.
The tests cover literal, const, mutable, computed, shadowed, and destructured keys.
They cover calls, exact receiver stores, same-element stores, different-key stores, and unknown-key stores.
They cover loop back edges, negation, assignment checks, joins, arguments, stores, and nullable function calls.
The c1, c3, and m10 store shapes have different-key store controls.
Annotated constants, equal-valued constants, integer aliases, and computed const initializers have controls.
The const-copy control accepts every tested key form.
The thirteen tests take 0.02 seconds.
An internal scope-exit test covers a direct const key and a const key in an indexed receiver.
Each shape keeps an outer-binding fact as its control.

The §154 table adds eight witnesses for six sites and their independent divergence expectations.
Two witnesses measure the switch exit with an inner receiver and its outer-receiver control.
TypeScript accepts the control and gives TS2531 for the inner-receiver case.
It measures TS2531 for an unchecked member, TS2721 for an unchecked call, and TS2345 for an unchecked argument.
TypeScript 5.9.2 accepts the three checked witnesses.
The table reader decodes escaped newlines in the expected diagnostic note.

## Checks

The compiler suite passes all tests, including the §154 total test.
The codegen suite passes all tests.
The format check passes.
The repository hygiene check passes.
The workspace all-target Clippy check exits 0.
Its warning messages and source locations match code pin `4286e511`; no new warning appears.
Every changed Rust file stays below 2,000 lines.

## Phase Review measurements

TypeScript 5.9.2 and both release CLIs cover all 124 review probes.
The table states each diagnostic's class; an additional rejection can precede the indexed-read diagnostic.
The c1, c2, c3, c4, e2, and m10 same-element stores now select `TscRejects`.
The c8 equal-valued const read selects `Diverges`.
The m6c argument and m6d store keep the pin's S100 code and give the copy message.

| Probe | Tree diagnostic class | `tsc` result |
|---|---|---|
| `p/a1-arg.ts` | S005: Diverges | accepts |
| `p/a2-store-local.ts` | S005: Diverges | accepts |
| `p/a3-store-field.ts` | S005: Diverges | accepts |
| `p/a4-return.ts` | S005: Diverges | accepts |
| `p/as1-assign-cond.ts` | S011: Diverges | accepts |
| `p/as2-call-between.ts` | S011: Diverges | accepts |
| `p/as3-other-key-store.ts` | S011: Diverges | accepts |
| `p/as4-store-nonnull.ts` | S011: TscRejects | accepts |
| `p/as5-alias-store.ts` | S011: Diverges | accepts |
| `p/as6-arg-assign.ts` | S011: TscRejects | TS2531 |
| `p/c1-const-vs-literal-store.ts` | S011: TscRejects | TS2531 |
| `p/c10-let-later.ts` | S011: TscRejects | TS2531 |
| `p/c2-two-consts-store.ts` | S011: TscRejects | TS2531 |
| `p/c3-literal-check-const-store.ts` | S011: TscRejects | TS2531 |
| `p/c4-annot-const-store.ts` | S011: TscRejects | TS2531 |
| `p/c5-const-param-key.ts` | S011: Diverges | accepts |
| `p/c6-const-call-key.ts` | S011: Diverges | accepts |
| `p/c7-const-shadow.ts` | S011: TscRejects | TS2531 |
| `p/c8-two-consts-read.ts` | S011: Diverges | accepts |
| `p/c9-for-of-const.ts` | S011: Diverges; S100: TscRejects | accepts |
| `p/f1-fn-call.ts` | S100: Diverges | accepts |
| `p/f2-fn-call-nocheck.ts` | S100: TscRejects | TS2721 |
| `p/g1-global-recv-store.ts` | S011: TscRejects | TS2531 |
| `p/g2-global-elem-store.ts` | S011: TscRejects | TS2531 |
| `p/g3-global-loop-elem-store.ts` | S011: TscRejects; S100: TscRejects | TS2531 |
| `p/g4-global-loop-recv-store.ts` | S011: TscRejects; S100: TscRejects | TS2531 |
| `p/g5-global-try.ts` | S011: TscRejects | TS2531 |
| `p/j1-join.ts` | S011: TscRejects | TS2531 |
| `p/j2-join-ok.ts` | S011: Diverges | accepts |
| `p/j3-early-return.ts` | S011: Diverges | accepts |
| `p/j4-or.ts` | S011: Diverges | accepts |
| `p/l3-local-loop-elem-store.ts` | S011: TscRejects; S100: TscRejects | TS2531 |
| `p/l5-local-try.ts` | S011: TscRejects | TS2531 |
| `p/lm1-lambda.ts` | S009: Diverges; S011: TscRejects | TS2531 |
| `p/lm2-lambda-store.ts` | S009: Diverges; S011: Diverges | accepts |
| `p/lp1-for-i.ts` | S011: TscRejects; S100: TscRejects | TS2531 |
| `p/lp2-loop-continue.ts` | S011: TscRejects; S100: TscRejects | TS2531 |
| `p/lp3-loop-const-body.ts` | S011: TscRejects; S100: TscRejects | TS2531 |
| `p/n1-nested-recv.ts` | S011: Diverges | accepts |
| `p/n2-nested-recv-store.ts` | S011: TscRejects | TS2531 |
| `p/n3-nested-root-store.ts` | S011: TscRejects | TS2531 |
| `p/n4-2d.ts` | S011: TscRejects | TS2531 |
| `p/n5-2d-other.ts` | S011: Diverges | accepts |
| `p/n6-this.ts` | S011: TscRejects | TS2531 |
| `p/o1-optchain.ts` | S100: Diverges | accepts |
| `p/o2-nullish.ts` | S100: Diverges | accepts |
| `p/s1-switch.ts` | S100: Diverges | accepts |
| `p/s2-switch-case-const.ts` | S011: Diverges | accepts |
| `p/sh1-recv-shadow.ts` | S011: TscRejects | TS2531 |
| `p/sh2-recv-shadow-restore.ts` | S011: Diverges | accepts |
| `q/d1-destructure.ts` | S100: Diverges; S011: Diverges | TS2531 |
| `q/d2-nullish-assign.ts` | S011: TscRejects | TS2531 |
| `q/e1-enum-store.ts` | S100: Diverges; S011: Diverges | TS2531 |
| `q/e2-const-join-store.ts` | S011: TscRejects | TS2531 |
| `q/g0-global-control.ts` | S011: Diverges | accepts |
| `q/g3b-global-loop-control.ts` | S011: Diverges | accepts |
| `q/g3c-global-loop-store.ts` | S011: TscRejects | TS2531 |
| `q/g4c-global-loop-recv.ts` | S011: TscRejects | TS2531 |
| `q/g6-global-join.ts` | S011: TscRejects | TS2531 |
| `q/g7-global-join-recv.ts` | S011: TscRejects | TS2531 |
| `q/g8-global-try-recv.ts` | S011: TscRejects | TS2531 |
| `q/g9-global-field-recv.ts` | S011: TscRejects | TS2531 |
| `q/l3c-local-loop-store.ts` | S011: TscRejects | TS2531 |
| `q/l3d-local-loop-control.ts` | S011: Diverges | accepts |
| `q/lm3-lambda-body-check.ts` | S011: Diverges | accepts |
| `q/lm4-lambda-outer-fact.ts` | S011: TscRejects | TS2531 |
| `q/n7-recv-elem.ts` | S011: Diverges | accepts |
| `q/o1-optchain.ts` | accepts | accepts |
| `q/o2-nullish.ts` | accepts | accepts |
| `q/r1-let-key-param.ts` | S011: TscRejects | accepts |
| `q/r2-lit-vs-const-read.ts` | S011: Diverges | accepts |
| `q/s1-switch.ts` | accepts | accepts |
| `q/s3-switch-fact.ts` | S011: TscRejects | TS2531 |
| `q/s4-switch-ok.ts` | S011: Diverges | accepts |
| `q/s5-switch-case-const-shadow.ts` | S011: TscRejects | TS2531 |
| `q/t1-try-ok.ts` | S011: Diverges | accepts |
| `q/t2-try-after.ts` | S011: TscRejects | TS2531 |
| `m/m1-subclass-store.ts` | S100: Diverges; S005: TscRejects | accepts |
| `m/m10-const-alias.ts` | S100: Diverges; S011: TscRejects | TS2531 |
| `m/m11-callres.ts` | S100: Diverges; S011: TscRejects | TS2531 |
| `m/m12-field-old.ts` | S100: Diverges; S011: Diverges | accepts |
| `m/m2-method.ts` | S100: Diverges; S011: Diverges | accepts |
| `m/m3-field-write.ts` | S100: Diverges; S011: Diverges | accepts |
| `m/m4-local-old.ts` | S100: Diverges; S011: TscRejects | TS18047 |
| `m/m5-paren.ts` | S100: Diverges; S011: Diverges | accepts |
| `m/m6-fnvalue-arg.ts` | S100: Diverges; S100: Diverges | accepts |
| `m/m7-binop.ts` | S100: Diverges; S011: Diverges | accepts |
| `m/m8-nonnull-assert.ts` | S100: Diverges; S100: Diverges | accepts |
| `m/m9-key-binop.ts` | S100: Diverges; S011: Diverges | accepts |
| `m2/m10-const-alias.ts` | S011: TscRejects | TS2531 |
| `m2/m2-method.ts` | S011: Diverges | accepts |
| `m2/m6b-fnvalue-arg-local.ts` | S100: TscRejects | TS2345 |
| `m2/m6c-fnvalue-arg-idx-nocheck.ts` | S100: TscRejects | TS2345 |
| `m2/m6d-fnvalue-store.ts` | S100: Diverges | accepts |
| `m3/L1-for-step.ts` | S011: TscRejects | TS2531 |
| `m3/L10-cond-expr-store.ts` | S100: Diverges; S011: TscRejects | TS2531 |
| `m3/L11-and-store.ts` | S011: TscRejects | TS2531 |
| `m3/L12-tmpl-store.ts` | S011: TscRejects | TS2531 |
| `m3/L2-while-cond-store.ts` | S011: TscRejects | TS2531 |
| `m3/L3-nested-loop.ts` | S011: TscRejects | TS2531 |
| `m3/L4-switch-in-loop.ts` | S011: TscRejects | TS2531 |
| `m3/L5-for-of.ts` | S011: TscRejects | TS2531 |
| `m3/L6-loop-recv-field.ts` | S011: TscRejects | TS2531 |
| `m3/L7-loop-const-key-recheck.ts` | S011: TscRejects | TS2531 |
| `m3/L8-try-in-loop.ts` | S011: TscRejects | TS2531 |
| `m3/L9-loop-break-join.ts` | S011: TscRejects | TS2531 |
| `m4/L10b-cond-store.ts` | S011: TscRejects | TS2531 |
| `m4/L10c-cond-store-direct.ts` | S011: TscRejects | TS2531 |
| `m4/L10d-or-store.ts` | S011: TscRejects | TS2531 |
| `m4/L10e-nullish-store.ts` | S100: TscRejects; S011: TscRejects | TS2531 |
| `m4/L10f-cond-recv-store.ts` | S011: TscRejects | TS2531 |
| `m5/D1.ts` | S011: Diverges | accepts |
| `m5/D2.ts` | S011: TscRejects | TS2531 |
| `m5/D3.ts` | S011: Diverges | accepts |
| `m5/D4-hex.ts` | S011: TscRejects | TS2531 |
| `m6/R1-method-arg.ts` | S005: TscRejects | TS2345 |
| `m6/R2-ctor-arg.ts` | S005: TscRejects | TS2345 |
| `m6/R3-array-lit.ts` | S005: TscRejects | TS2322 |
| `m6/R4-incr.ts` | S011: TscRejects | TS2531 |
| `m6/R5-compound.ts` | S011: TscRejects | TS2531 |
| `m6/R6-elem-store.ts` | S005: TscRejects | TS2322 |
| `m6/R7-lambda-return.ts` | S009: Diverges; S005: TscRejects | TS2322 |
| `m6/R8-chain.ts` | S011: TscRejects | TS2531 |
| `m6/R9-cond-old.ts` | S011: TscRejects | TS2531 |

## Residual cases

These §163.2 cases give an indexed S011 with `TscRejects`, while TypeScript accepts:

| Form | Probe | Tree | `tsc` |
|---|---|---|---|
| An unchanged parameter key | `q/r1-let-key-param.ts` | S011, `TscRejects` | accepts |
| A non-null element store | `p/as4-store-nonnull.ts` | S011, `TscRejects` | accepts |

These §163.4 cases first reject the store with S100.
The follow-on indexed S011 keeps `Diverges`, while TypeScript gives TS2531.
The contract lists both as contrived because the program already fails the first rejection.

| Rejected store | Probe | Follow-on tree class | `tsc` |
|---|---|---|---|
| A destructuring assignment | `q/d1-destructure.ts` | S011, `Diverges` | TS2531 |
| An enum-member key | `q/e1-enum-store.ts` | S011, `Diverges` | TS2531 |

## Gate and full corpus

Every `corpus/**/*.ts` file except `.d.ts` (817 files: accept, reject, warn, trap, and interop) gives the same exit status and the same rule codes with the code pin `4286e511` and the tree.
`tools/gate.sh full`: `debug 2362/0/3 release 2359/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.
