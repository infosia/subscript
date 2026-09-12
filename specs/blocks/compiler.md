# Compiler and runtime — contract

Status: the current contract. The revision log (Rev 0–25, 2026-07-22 to
2026-07-27) and every resolved or superseded section are in
`compiler-history.md`, under their original section numbers; a stub with
the same number stays here. Since Rev 25 a change is a dated section
or a dated note inside one (`*(Owner decision …)*`, `*(Amended …)*`),
and the tracking note of the arc records the commits. A section
lists the plan phase or the request it decides (`specs/subscript-project-plan.md`
§6; `HANDOFF-Rnn`). Evidence lands in `specs/tracking/<topic>.md`.

Layout *(2026-09-12)*: this file holds the index only. Each section
is one file under `specs/blocks/compiler/`, named `s<NNN>-<title>.md`
(`s068-one-ordered-ir-between-the-checker-and-the-two-tiers.md`). A
citation stays `compiler.md §N`; the table below resolves it to the
file. A new section is a new file and a new row here. The rule for
the file layout is §5.y.

## 0. Section index

The stages of the current form and the sections that hold their rules:

| Stage | Sections |
|---|---|
| parse and check (SWC, the checker, HIR) | §6, §67, §69, §72, §74, §76, §79, §82, §83, §87 |
| HIR→LIR lowering, the LIR form, the verifier | §20, §68, §73, §75 |
| dev tier (Cranelift JIT, hot reload) | §7, §8, §12.3a, §70 |
| ship tier (C emission, the platform C compiler) | §11, §11b, §11c, §66, §86, §89 |
| reference interpreter | §68.7 |
| runtime Context, allocation, traps, collection | §18, §19 (history), §21, §22, §80, §84 |
| interop and bindgen (C headers, boundary structs, handles) | §12, §13, §14, §23, §33, §44, §52, §56–§65 |
| workers | §38–§40, §84 |
| the gate, the corpus, the inventory | §2, §3, §8.3, §85, §88, §100 |

Every section, with its status:

| § | Title | Status | File |
|---|---|---|---|
| §1 | Architecture | active | [`s001-architecture.md`](compiler/s001-architecture.md) |
| §2 | Oracle: golden corpus outputs | active | [`s002-oracle-golden-corpus-outputs.md`](compiler/s002-oracle-golden-corpus-outputs.md) |
| §3 | Pre-registered criteria | active | [`s003-pre-registered-criteria.md`](compiler/s003-pre-registered-criteria.md) |
| §4 | Milestones and gates | active | [`s004-milestones-and-gates.md`](compiler/s004-milestones-and-gates.md) |
| §5 | Conventions | active | [`s005-conventions.md`](compiler/s005-conventions.md) |
| §6 | P1 checker contract | active | [`s006-p1-checker-contract.md`](compiler/s006-p1-checker-contract.md) |
| §7 | P2 runtime and JIT contract | active | [`s007-p2-runtime-and-jit-contract.md`](compiler/s007-p2-runtime-and-jit-contract.md) |
| §8 | P3 AOT, hot reload, and the standing gate | active | [`s008-p3-aot-hot-reload-and-the-standing-gate.md`](compiler/s008-p3-aot-hot-reload-and-the-standing-gate.md) |
| §9 | P4 measurement methodology | active | [`s009-p4-measurement-methodology.md`](compiler/s009-p4-measurement-methodology.md) |
| §10 | P4.1 lowering optimization and re-measurement | active | [`s010-p4-1-lowering-optimization-and-re-measurement.md`](compiler/s010-p4-1-lowering-optimization-and-re-measurement.md) |
| §10a | Emitted-C growable-array element access is inlined | active | [`s010a-emitted-c-growable-array-element-access-is-inlined.md`](compiler/s010a-emitted-c-growable-array-element-access-is-inlined.md) |
| §11 | P4.3 ship tier — C emission (LLVM) | active | [`s011-p4-3-ship-tier-c-emission-llvm.md`](compiler/s011-p4-3-ship-tier-c-emission-llvm.md) |
| §11a | C toolchain selection is target-portable (crate build) | active | [`s011a-c-toolchain-selection-is-target-portable-crate-build.md`](compiler/s011a-c-toolchain-selection-is-target-portable-crate-build.md) |
| §11b | C toolchain at runtime is clang, located portably | active; its Windows clang choice is superseded by §11c | [`s011b-c-toolchain-at-runtime-is-clang-located-portably.md`](compiler/s011b-c-toolchain-at-runtime-is-clang-located-portably.md) |
| §11c | C toolchain on Windows is MSVC `cl` (supersedes §11b's Windows clang) | active | [`s011c-c-toolchain-on-windows-is-msvc-cl-supersedes-11b-s-windows-c.md`](compiler/s011c-c-toolchain-on-windows-is-msvc-cl-supersedes-11b-s-windows-c.md) |
| §12 | P5 C-header binding vertical slice | active | [`s012-p5-c-header-binding-vertical-slice.md`](compiler/s012-p5-c-header-binding-vertical-slice.md) |
| §11d | No emitted type has an empty member list | active | [`s011d-no-emitted-type-has-an-empty-member-list.md`](compiler/s011d-no-emitted-type-has-an-empty-member-list.md) |
| §11e | A label is followed by a statement | active | [`s011e-a-label-is-followed-by-a-statement.md`](compiler/s011e-a-label-is-followed-by-a-statement.md) |
| §13 | P6 — production-C-header interop (host-agnostic) | active | [`s013-p6-production-c-header-interop-host-agnostic.md`](compiler/s013-p6-production-c-header-interop-host-agnostic.md) |
| §14 | P7 — async/Future model and remaining production shapes | active | [`s014-p7-async-future-model-and-remaining-production-shapes.md`](compiler/s014-p7-async-future-model-and-remaining-production-shapes.md) |
| §15 | P9 — standard library (`Math`, `Date`) | active | [`s015-p9-standard-library-math-date.md`](compiler/s015-p9-standard-library-math-date.md) |
| §16 | P14 — narrow numerics (`i8`/`u8`/`i16`/`u16`/`f16`) | active | [`s016-p14-narrow-numerics-i8-u8-i16-u16-f16.md`](compiler/s016-p14-narrow-numerics-i8-u8-i16-u16-f16.md) |
| §17 | P16 — generated API reference | active | [`s017-p16-generated-api-reference.md`](compiler/s017-p16-generated-api-reference.md) |
| §18 | The host Context C API, and the trap observer | active; 18.2e is history (superseded by §21.2) | [`s018-the-host-context-c-api-and-the-trap-observer.md`](compiler/s018-the-host-context-c-api-and-the-trap-observer.md) |
| §19 | P19 — trap unwind parity (CRITICAL) — RESOLVED 2026-07-26 | history — `compiler-history.md` §19 (resolved 2026-07-26) | [`s019-p19-trap-unwind-parity-critical-resolved-2026-07-26.md`](compiler/s019-p19-trap-unwind-parity-critical-resolved-2026-07-26.md) |
| §20 | P20 — the trap-site IR | active | [`s020-p20-the-trap-site-ir.md`](compiler/s020-p20-the-trap-site-ir.md) |
| §21 | P21 — the allocation path: fault injection and per-allocation attribution | active | [`s021-p21-the-allocation-path-fault-injection-and-per-allocation-a.md`](compiler/s021-p21-the-allocation-path-fault-injection-and-per-allocation-a.md) |
| §22 | P24 — two monotonic costs under invariant 2 | active | [`s022-p24-two-monotonic-costs-under-invariant-2.md`](compiler/s022-p24-two-monotonic-costs-under-invariant-2.md) |
| §23 | P25 — no header is privileged | active | [`s023-p25-no-header-is-privileged.md`](compiler/s023-p25-no-header-is-privileged.md) |
| §24 | Q32 — string-literal union aliases | active | [`s024-q32-string-literal-union-aliases.md`](compiler/s024-q32-string-literal-union-aliases.md) |
| §25 | Q33 — literal-constructible descriptor classes | active | [`s025-q33-literal-constructible-descriptor-classes.md`](compiler/s025-q33-literal-constructible-descriptor-classes.md) |
| §26 | Q34 — async/await, poll-driven | active | [`s026-q34-async-await-poll-driven.md`](compiler/s026-q34-async-await-poll-driven.md) |
| §27 | R5 — scalar array-pairs at parameter position | active | [`s027-r5-scalar-array-pairs-at-parameter-position.md`](compiler/s027-r5-scalar-array-pairs-at-parameter-position.md) |
| §28 | R6 — string-view fields in boundary structs | active | [`s028-r6-string-view-fields-in-boundary-structs.md`](compiler/s028-r6-string-view-fields-in-boundary-structs.md) |
| §29 | AI-facing generated reference | active | [`s029-ai-facing-generated-reference.md`](compiler/s029-ai-facing-generated-reference.md) |
| §30 | R7 — nested aggregates beside string fields; struct-level scalar/enum pairs | active | [`s030-r7-nested-aggregates-beside-string-fields-struct-level-scala.md`](compiler/s030-r7-nested-aggregates-beside-string-fields-struct-level-scala.md) |
| §31 | R8 — opaque handles in aggregate positions | active | [`s031-r8-opaque-handles-in-aggregate-positions.md`](compiler/s031-r8-opaque-handles-in-aggregate-positions.md) |
| §32 | R9 — recursive lowering at embedded positions | active | [`s032-r9-recursive-lowering-at-embedded-positions.md`](compiler/s032-r9-recursive-lowering-at-embedded-positions.md) |
| §33 | R10 — lowering through struct-pointer members | active | [`s033-r10-lowering-through-struct-pointer-members.md`](compiler/s033-r10-lowering-through-struct-pointer-members.md) |
| §34 | R11 — parameter-position handle-element pairs | active | [`s034-r11-parameter-position-handle-element-pairs.md`](compiler/s034-r11-parameter-position-handle-element-pairs.md) |
| §35 | R12 — `_Nullable` handle parameters | active | [`s035-r12-nullable-handle-parameters.md`](compiler/s035-r12-nullable-handle-parameters.md) |
| §36 | OBS-1 — emitted C names every referenced boundary typedef | active | [`s036-obs-1-emitted-c-names-every-referenced-boundary-typedef.md`](compiler/s036-obs-1-emitted-c-names-every-referenced-boundary-typedef.md) |
| §37 | R13 — async instance methods on reference classes | active | [`s037-r13-async-instance-methods-on-reference-classes.md`](compiler/s037-r13-async-instance-methods-on-reference-classes.md) |
| §38 | Workers round 1 — module state is Context state in both tiers | active | [`s038-workers-round-1-module-state-is-context-state-in-both-tiers.md`](compiler/s038-workers-round-1-module-state-is-context-state-in-both-tiers.md) |
| §39 | Workers round 2 — runtime threads, channels, worker lifecycle | active | [`s039-workers-round-2-runtime-threads-channels-worker-lifecycle.md`](compiler/s039-workers-round-2-runtime-threads-channels-worker-lifecycle.md) |
| §40 | Workers round 3 — the language surface (Q35) | active | [`s040-workers-round-3-the-language-surface-q35.md`](compiler/s040-workers-round-3-the-language-surface-q35.md) |
| §41 | R14 — `switch` over Q32 literal-union aliases | active | [`s041-r14-switch-over-q32-literal-union-aliases.md`](compiler/s041-r14-switch-over-q32-literal-union-aliases.md) |
| §42 | R15 — divergence flow: exhaustive switches and `unreachable()` | active | [`s042-r15-divergence-flow-exhaustive-switches-and-unreachable.md`](compiler/s042-r15-divergence-flow-exhaustive-switches-and-unreachable.md) |
| §43 | R16 — absence-capable Q32-alias descriptor members | active | [`s043-r16-absence-capable-q32-alias-descriptor-members.md`](compiler/s043-r16-absence-capable-q32-alias-descriptor-members.md) |
| §44 | OBS-3 — scalar handle fields beside arrays in scratch-lowered structs | active | [`s044-obs-3-scalar-handle-fields-beside-arrays-in-scratch-lowered.md`](compiler/s044-obs-3-scalar-handle-fields-beside-arrays-in-scratch-lowered.md) |
| §45 | R18 — contextual typing for conditional expressions | active | [`s045-r18-contextual-typing-for-conditional-expressions.md`](compiler/s045-r18-contextual-typing-for-conditional-expressions.md) |
| §46 | R19 — narrowing flows into conditional arms | active | [`s046-r19-narrowing-flows-into-conditional-arms.md`](compiler/s046-r19-narrowing-flows-into-conditional-arms.md) |
| §47 | OBS-4 — AAPCS64 packs eightbytes, not fields (CRITICAL) | active | [`s047-obs-4-aapcs64-packs-eightbytes-not-fields-critical.md`](compiler/s047-obs-4-aapcs64-packs-eightbytes-not-fields-critical.md) |
| §48 | R20 — external types in a generated mirror | active | [`s048-r20-external-types-in-a-generated-mirror.md`](compiler/s048-r20-external-types-in-a-generated-mirror.md) |
| §49 | R21 — a host-driven ship-tier form | active | [`s049-r21-a-host-driven-ship-tier-form.md`](compiler/s049-r21-a-host-driven-ship-tier-form.md) |
| §50 | R23 — wire-mapped literal-union aliases (`CEnum`) | active | [`s050-r23-wire-mapped-literal-union-aliases-cenum.md`](compiler/s050-r23-wire-mapped-literal-union-aliases-cenum.md) |
| §51 | R24 — `subscript bind` emits CEnum references (`@subscript-cenum`) | active | [`s051-r24-subscript-bind-emits-cenum-references-subscript-cenum.md`](compiler/s051-r24-subscript-bind-emits-cenum-references-subscript-cenum.md) |
| §52 | Wire-mapped aliases in boundary structs | active | [`s052-wire-mapped-aliases-in-boundary-structs.md`](compiler/s052-wire-mapped-aliases-in-boundary-structs.md) |
| §53 | R25 — entry-less dev sessions | active | [`s053-r25-entry-less-dev-sessions.md`](compiler/s053-r25-entry-less-dev-sessions.md) |
| §54 | Caller link inputs follow the translation units | active | [`s054-caller-link-inputs-follow-the-translation-units.md`](compiler/s054-caller-link-inputs-follow-the-translation-units.md) |
| §55 | A Cranelift frame probes the stack | active | [`s055-a-cranelift-frame-probes-the-stack.md`](compiler/s055-a-cranelift-frame-probes-the-stack.md) |
| §56 | R26 — integer literals read at the target's width | active | [`s056-r26-integer-literals-read-at-the-target-s-width.md`](compiler/s056-r26-integer-literals-read-at-the-target-s-width.md) |
| §57 | R27 — field initializers run on every construction | active | [`s057-r27-field-initializers-run-on-every-construction.md`](compiler/s057-r27-field-initializers-run-on-every-construction.md) |
| §58 | R29 — a class index signature is accessor sugar | active | [`s058-r29-a-class-index-signature-is-accessor-sugar.md`](compiler/s058-r29-a-class-index-signature-is-accessor-sugar.md) |
| §59 | R30 — host-called entries take handle and scalar parameters | active | [`s059-r30-host-called-entries-take-handle-and-scalar-parameters.md`](compiler/s059-r30-host-called-entries-take-handle-and-scalar-parameters.md) |
| §60 | R31 — `using` declarations: deterministic scope-exit dispose | active | [`s060-r31-using-declarations-deterministic-scope-exit-dispose.md`](compiler/s060-r31-using-declarations-deterministic-scope-exit-dispose.md) |
| §61 | R32 — a wire-mapped alias in an entry signature | active | [`s061-r32-a-wire-mapped-alias-in-an-entry-signature.md`](compiler/s061-r32-a-wire-mapped-alias-in-an-entry-signature.md) |
| §62 | R33 — an alignment override on `@CStruct` value classes | active | [`s062-r33-an-alignment-override-on-cstruct-value-classes.md`](compiler/s062-r33-an-alignment-override-on-cstruct-value-classes.md) |
| §63 | R35 — a discovery check for one unresolved import | active | [`s063-r35-a-discovery-check-for-one-unresolved-import.md`](compiler/s063-r35-a-discovery-check-for-one-unresolved-import.md) |
| §64 | R36 — async methods on generic classes, generic async functions | active | [`s064-r36-async-methods-on-generic-classes-generic-async-functions.md`](compiler/s064-r36-async-methods-on-generic-classes-generic-async-functions.md) |
| §65 | R37 — a named accessor is method sugar | active | [`s065-r37-a-named-accessor-is-method-sugar.md`](compiler/s065-r37-a-named-accessor-is-method-sugar.md) |
| §66 | Emitted C identifiers — two spaces, never one | active | [`s066-emitted-c-identifiers-two-spaces-never-one.md`](compiler/s066-emitted-c-identifiers-two-spaces-never-one.md) |
| §67 | Checker scoping, and state that must survive a suspension | active | [`s067-checker-scoping-and-state-that-must-survive-a-suspension.md`](compiler/s067-checker-scoping-and-state-that-must-survive-a-suspension.md) |
| §68 | One ordered IR between the checker and the two tiers | active | [`s068-one-ordered-ir-between-the-checker-and-the-two-tiers.md`](compiler/s068-one-ordered-ir-between-the-checker-and-the-two-tiers.md) |
| §69 | The language definition, checked instead of asserted | active | [`s069-the-language-definition-checked-instead-of-asserted.md`](compiler/s069-the-language-definition-checked-instead-of-asserted.md) |
| §70 | A held async handle, by reference count | active | [`s070-a-held-async-handle-by-reference-count.md`](compiler/s070-a-held-async-handle-by-reference-count.md) |
| §71 | Static members | active | [`s071-static-members.md`](compiler/s071-static-members.md) |
| §72 | One integer-literal reader, and one HIR walk | active | [`s072-one-integer-literal-reader-and-one-hir-walk.md`](compiler/s072-one-integer-literal-reader-and-one-hir-walk.md) |
| §73 | The LIR terminator walks itself | active | [`s073-the-lir-terminator-walks-itself.md`](compiler/s073-the-lir-terminator-walks-itself.md) |
| §74 | One handle-kind table | active | [`s074-one-handle-kind-table.md`](compiler/s074-one-handle-kind-table.md) |
| §75 | Four LIR facts the form carries once | active | [`s075-four-lir-facts-the-form-carries-once.md`](compiler/s075-four-lir-facts-the-form-carries-once.md) |
| §76 | Three checker facts the HIR carries once | active | [`s076-three-checker-facts-the-hir-carries-once.md`](compiler/s076-three-checker-facts-the-hir-carries-once.md) |
| §77 | Two runtime facts written once | active | [`s077-two-runtime-facts-written-once.md`](compiler/s077-two-runtime-facts-written-once.md) |
| §78 | The MINOR consolidation pass | active | [`s078-the-minor-consolidation-pass.md`](compiler/s078-the-minor-consolidation-pass.md) |
| §79 | A divergence diagnostic shows the TypeScript form and the subscript form | active | [`s079-a-divergence-diagnostic-shows-the-typescript-form-and-the-su.md`](compiler/s079-a-divergence-diagnostic-shows-the-typescript-form-and-the-su.md) |
| §80 | Array data past `len` is zero | active | [`s080-array-data-past-len-is-zero.md`](compiler/s080-array-data-past-len-is-zero.md) |
| §81 | R38 — a write through a `@CStruct` copy that nothing reads | active | [`s081-r38-a-write-through-a-cstruct-copy-that-nothing-reads.md`](compiler/s081-r38-a-write-through-a-cstruct-copy-that-nothing-reads.md) |
| §82 | R39 — six requests decided at `e1c2be1` | active | [`s082-r39-six-requests-decided-at-e1c2be1.md`](compiler/s082-r39-six-requests-decided-at-e1c2be1.md) |
| §83 | The operation-signature table is a total function of the HIR | active | [`s083-the-operation-signature-table-is-a-total-function-of-the-hir.md`](compiler/s083-the-operation-signature-table-is-a-total-function-of-the-hir.md) |
| §84 | Worker messages carry `string` fields by copy | active | [`s084-worker-messages-carry-string-fields-by-copy.md`](compiler/s084-worker-messages-carry-string-fields-by-copy.md) |
| §85 | One gate command, two shapes | active | [`s085-one-gate-command-two-shapes.md`](compiler/s085-one-gate-command-two-shapes.md) |
| §86 | C emission is linear in the function it emits | active | [`s086-c-emission-is-linear-in-the-function-it-emits.md`](compiler/s086-c-emission-is-linear-in-the-function-it-emits.md) |
| §87 | A synthetic owner is one scoped operation | active | [`s087-a-synthetic-owner-is-one-scoped-operation.md`](compiler/s087-a-synthetic-owner-is-one-scoped-operation.md) |
| §88 | The corpus index is the inventory | active | [`s088-the-corpus-index-is-the-inventory.md`](compiler/s088-the-corpus-index-is-the-inventory.md) |
| §89 | R40 — a long string constant is adjacent C literals | active; rule 3 superseded by §99 | [`s089-r40-a-long-string-constant-is-adjacent-c-literals.md`](compiler/s089-r40-a-long-string-constant-is-adjacent-c-literals.md) |
| §90 | No public entry point panics or faults on any input | active | [`s090-no-public-entry-point-panics-or-faults-on-any-input.md`](compiler/s090-no-public-entry-point-panics-or-faults-on-any-input.md) |
| §91 | The tutorials' programs run in the gate | active | [`s091-the-tutorials-programs-run-in-the-gate.md`](compiler/s091-the-tutorials-programs-run-in-the-gate.md) |
| §92 | An async call starts its body at the call | active | [`s092-an-async-call-starts-its-body-at-the-call.md`](compiler/s092-an-async-call-starts-its-body-at-the-call.md) |
| §93 | An async method declares type parameters | active | [`s093-an-async-method-declares-type-parameters.md`](compiler/s093-an-async-method-declares-type-parameters.md) |
| §94 | Host-driven async continuation queue | active | [`s094-host-driven-async-continuation-queue.md`](compiler/s094-host-driven-async-continuation-queue.md) |
| §95 | Three unearned divergences from TypeScript | active | [`s095-three-unearned-divergences-from-typescript.md`](compiler/s095-three-unearned-divergences-from-typescript.md) |
| §96 | A surrogate escape denotes its code point | active | [`s096-a-surrogate-escape-denotes-its-code-point.md`](compiler/s096-a-surrogate-escape-denotes-its-code-point.md) |
| §97 | A `using` binding can be null | active | [`s097-a-using-binding-can-be-null.md`](compiler/s097-a-using-binding-can-be-null.md) |
| §98 | One shift-count rule, whatever the spelling | active | [`s098-one-shift-count-rule-whatever-the-spelling.md`](compiler/s098-one-shift-count-rule-whatever-the-spelling.md) |
| §99 | A long string constant is static C byte data | active | [`s099-a-long-string-constant-is-static-c-byte-data.md`](compiler/s099-a-long-string-constant-is-static-c-byte-data.md) |
| §100 | The Windows host runs the standing gate | active | [`s100-the-windows-host-runs-the-standing-gate.md`](compiler/s100-the-windows-host-runs-the-standing-gate.md) |
| §101 | A disposal is not placed where control cannot arrive | active | [`s101-a-disposal-is-not-placed-where-control-cannot-arrive.md`](compiler/s101-a-disposal-is-not-placed-where-control-cannot-arrive.md) |
| §102 | A test waits on a fact, not on a clock | active | [`s102-a-test-waits-on-a-fact-not-on-a-clock.md`](compiler/s102-a-test-waits-on-a-fact-not-on-a-clock.md) |
| §103 | A rejection reason must fit the form it rejects | active | [`s103-a-rejection-reason-must-fit-the-form-it-rejects.md`](compiler/s103-a-rejection-reason-must-fit-the-form-it-rejects.md) |
| §104 | A bare `Map` is not an iteration source | active | [`s104-a-bare-map-is-not-an-iteration-source.md`](compiler/s104-a-bare-map-is-not-an-iteration-source.md) |
| §105 | The `Array` namespace, and `Array.from` | active | [`s105-the-array-namespace-and-array-from.md`](compiler/s105-the-array-namespace-and-array-from.md) |
| §106 | The reference interpreter stores a generator | active | [`s106-the-reference-interpreter-stores-a-generator.md`](compiler/s106-the-reference-interpreter-stores-a-generator.md) |
| §107 | Binding patterns, by source type and position | active | [`s107-binding-patterns-by-source-type-and-position.md`](compiler/s107-binding-patterns-by-source-type-and-position.md) |
| §108 | A field carries a value before a constructor returns | active | [`s108-a-field-carries-a-value-before-a-constructor-returns.md`](compiler/s108-a-field-carries-a-value-before-a-constructor-returns.md) |
