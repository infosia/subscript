# Divergence diagnostics — 2026-08-30

Owner decision, 2026-08-30: a reject diagnostic at a construct that
`tsc` accepts shows the TypeScript form and the subscript form in code
(`compiler.md` §79). The table content was authored by a Claude Opus
agent; the wiring by the coding agent (owner decision, 2026-08-30).

## Landed `9124208`

- `compiler/src/divergence.rs`: 50 variants; 126 `tsc: accepts` reject
  entries placed once each; every `subscript` fragment checked with
  `subscript check`, every `ts` fragment with the repository `tsc`.
- Render: the block follows `= rule:`; messages and positions before
  it are unchanged; two CLI byte-exact pins updated to include it.
- Gate: `corpus_reject.rs` — 126 entries render a block, 40 render
  none, reported all at once.
- Release suite 1,159 passed, 0 failed. Clippy 7/18/13.

## Topics with no `collisions.md` heading (23)

`collisions.md` states the divergences in prose, and nothing checked
that the list was complete. These variants cite the section that
decided the rule instead. A heading for each is open work on the
record.

| `AnyType` | `compiler.md §6` | S001: `any` is banned |
| `DynamicObjectModel` | `compiler.md §6` | S002, S003: no dynamic evaluation, no prototype mutation |
| `StorageOnlyFloat16` | `compiler.md §16` | Q23: `f16` is storage only |
| `WireEnumValues` | `compiler.md §50` | R23: `CEnum` wire values |
| `ConditionalWithoutContext` | `compiler.md §45` | R18: contextual typing for conditionals |
| `DroppedAsyncHandle` | `compiler.md §70` | A held async handle, by reference count |
| `StaticMemberSurface` | `compiler.md §71` | R38: static members |
| `WorkerEntryShape` | `compiler.md §40` | Q35: the Worker language surface |
| `WorkerContextAffinity` | `compiler.md §40` | Q35: Context affinity and message transfer |
| `SwitchOverAlias` | `compiler.md §41` | R14: `switch` over a Q32 alias |
| `UnreachableInValuePosition` | `compiler.md §42` | R15: divergence flow |
| `DescriptorConstruction` | `compiler.md §25` | Q33: literal-constructible descriptor classes |
| `EntryParameterType` | `compiler.md §61` | R32: a wire alias in an entry signature |
| `EmbeddedHeaderCopy` | `compiler.md §33.5` | R10 rule 10: an embedded chain header |
| `MathSubset` | `stdlib.md §1` | Q19: the `Math` subset |
| `DateSubset` | `stdlib.md §3` | Q20: the UTC-deterministic `Date` subset |
| `LocaleSensitiveString` | `stdlib.md §8` | Q21: the `String` subset |
| `ArrayMethodDefaults` | `stdlib.md §9` | Q22: the `Array` subset |
| `MapKeyKind`, `MapScalarGet` | `stdlib.md §10` | Q24: `Map` / `Set` |
| `NumberCoercionAndArguments` | `stdlib.md §11` | Q25, Q26: `Number`, parsing, formatting |
| `VariadicArguments` | `stdlib.md §12` | Q27: the sweep groups; no variadic parameter |
| `JsonSubset` | `stdlib.md §13` | Q28: `JSON` |
| `NoTupleType` | `stdlib.md §14` | Q30: `for…of` and container iteration |
| `RegExpSubset`, `ReplaceAllGlobalFlag` | `stdlib.md §15` | Q31: regular expressions |
| `ByteAccessTarget` | `stdlib.md §18` | R34: `Context.bytesOf` |
| `AggregateLayoutLimit` | `collisions.md Q29` | Q29: the two size limits |
