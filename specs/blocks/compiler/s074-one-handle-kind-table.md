<!-- §74 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 74. One handle-kind table

*(Owner decision, 2026-08-30; review findings.)*

**Rule 1.** `compiler/src/types.rs` defines `HandleKind` (one variant
per managed shape: `Str`, `RegExp`, `Object`, `Array`, `Map`, `Set`,
`Generator`, `AsyncHandle`, `Worker`, `Inbox`, `Outbox`, `Func`,
`ReferenceClass`, `FuncBox` for `Func | null`, `BoundaryBox` for
`T | null` with `T` a value boundary class, §33.5) and one function
`Type::handle_kind(&self, classes) -> Option<HandleKind>`. Every
predicate over "is this a handle" — collector-managed, reference shape
for nullability, reference equality, trap-site dereference — is a filter
over `HandleKind`, written next to the enum with a comment that names the
question it answers. No other table over `Type` variants decides the
question.

**Rule 1a.** A filter has one of two natures, and the comment names
it. A *fact* filter (collector-managed; dereference reaches a Context
allocation) takes the answer the runtime gives. An *acceptance* filter
(which `T | null` unions the checker accepts, S011; which operands
`===` accepts, S100; which `as` conversions and `Map`/`Set` keys are
accepted, S014) is a language decision: it keeps the answer the corpus
records. A change to an acceptance filter's answer is a corpus decision
(core principle 2), never a consolidation. The round that consolidates
records every candidate widening in the report and changes none.

**Rule 2.** `codegen/src/layout.rs` (`is_managed`,
`type_contains_managed`, `has_managed_interior`, `managed_words`) is the
one place in `codegen/` that decides which words the collector scans, and
it reads `HandleKind`. `cemit.rs` calls it; the C emitter keeps no copy.

**Rule 3.** Whether `Worker`, `Inbox`, `Outbox`, and a bare `Func` are
collector-managed is a fact of the runtime class table
(`runtime/src/context.rs`, `worker.rs`): a kind is managed if the runtime
allocates its payload in the Context and the marker can reach it.

Measured 2026-08-30 (`85242e9`): `Worker` is a `Box` owned by
`WorkerSet::workers` (`runtime/src/worker.rs`), no class id, not
reachable by the marker — not managed. `Inbox` and `Outbox` are stack
locals of the worker entry — not managed. A bare `Func` is a code
pointer and an environment pointer; the environment lives in activation
or coroutine storage and the shared root plan roots it — the `Func`
itself is not managed. `AsyncHandle` and `RegExp` are Context
allocations — managed.

Measured before the rules: four tables in `compiler/` (`check/layout.rs`
`is_managed`, `hir.rs` `reference_value`, `types.rs`
`is_reference_shape`, `check/mod.rs` `is_reference_class`) and three in
`codegen/` (`layout.rs` twice, `cemit.rs` twice). `cemit.rs` counted
`Worker`, `Inbox`, `Outbox`, and `Func` as managed; `layout.rs` did not.
`RegExp` reached one of the four compiler tables. The two tiers rooted
different sets.

**Check.** A unit test in `compiler/` lists every `Type` variant and
asserts `handle_kind` returns the recorded answer for each. A unit test
in `codegen/` asserts the expected managed word count for a local of
every handle kind and every `ForOfKind` iterator (a bare `Func` has 0
managed words; every iterator has 4). A test that compares the C
emitter's decision with the shared plan compares one function with
itself after this section and is not a check (core principle 9).
