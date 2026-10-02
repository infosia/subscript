# §148 — static namespace imports

The checker binds a namespace to its source module. A scope-aware AST pass resolves each member before signatures and bodies.
Each resolved member is an ordinary import binding through the export graph. Generic templates use the resolved declarations.
The binding keeps its declaration identity and read-only status. No namespace value reaches HIR.
Missing modules keep qualified uses poisoned. Discovery records the namespace local in its own field. Type-only namespace imports remain rejected.

## Red

A CLI built from `4886fea2` rejects `a318-namespace-import` with exit status 1 and 14 diagnostics:

```text
S100 main.ts:6:8 only named imports are in the decided surface
S100 main.ts:7:8 only named imports are in the decided surface
S016 main.ts:10:12 unknown name `ns`
S016 main.ts:11:3 unknown name `ns`
S016 main.ts:12:12 unknown name `ns`
S016 main.ts:14:12 unknown name `ns`
S100 main.ts:15:12 qualified type names are not decided
S100 main.ts:15:19 `new` requires a class name
S100 main.ts:18:12 qualified type names are not decided
S100 main.ts:18:24 `new` requires a class name
S016 main.ts:21:12 unknown name `ns`
S100 main.ts:22:12 qualified type names are not decided
S016 main.ts:25:12 unknown name `ns`
S016 main.ts:26:12 unknown name `chain`
error: 14 error(s)
```

## Corpus evidence and C18 IDs

`a318-namespace-import` compares every export kind with named imports. It includes live reads, generic references, and a re-export chain.
The interpreter, dev JIT, ship C AOT, and Node 24.18.0 match its golden.
TypeScript 5.9.2 measures these headers with strict ES2022 checks and the repository prelude:

| Reject ID | Measured `tsc` result |
| --- | --- |
| `r316-namespace-argument` | TS2345 |
| `r317-namespace-stored` | accepts |
| `r318-namespace-indexed` | accepts |
| `r319-namespace-write` | TS2540 |
| `r320-namespace-missing` | TS2339 |
| `r321-namespace-export` | accepts |
| `r322-namespace-type-only` | accepts |
| `r323-namespace-returned` | TS2322 |
| `r324-namespace-comparison` | accepts |
| `r325-namespace-template` | accepts |
| `r326-namespace-typeof` | accepts |
| `r327-namespace-increment` | TS2540 |

## Validation

- `cargo test --offline --locked -p subscript-compiler` passes, including the §143 matrix, `tsc` corpus, and JS corpus.
- `cargo test --offline --locked -p subscript-codegen` passes, including documentation blocks and the LIR text snapshot.
- Namespace, local-shadow, and module-cycle corpus entries pass on all supported engines.
- Unit tests cover host exports, import writes, missing members, missing modules, and discovery poison.
- The default-import diagnostic and its existing test stay unchanged.
- The document generator updates `generated-docs/corpus-index.md`. Other generated documents stay unchanged.
- The LIR snapshot adds only the new async entry `a319`; existing snapshot sections stay unchanged.
- No existing `.expected` golden changes. The tutorial edit remains orchestrator-owned.
- `cargo fmt --check`, `git diff --check`, and `tools/hygiene.sh` pass.

## Round 3 evidence

The AST pass replaces the resolution at individual expression, call, constructor, assignment, and type-reference sites.
Bare namespace identifiers retain their value rejection. Lexical declarations and parameters shadow the qualifier in blocks and lambdas.
`export { ns }` gives one S100 with C18. A downstream re-export adds no diagnostic.

C18 entry IDs: `a319-namespace-async-worker`, `a320-namespace-shadow`, `a321-namespace-cycle`, and `r328-namespace-local-export`.
`a319` compares namespace calls and Worker entries with their named-import forms.

A CLI built from `4886fea2` rejects `a319`, `a320`, and `a321` with 4, 2, and 5 diagnostics, respectively.
`a319` reports S100 for the namespace import and Worker entry, and S016 for both qualified await calls.
`a320` reports S100 for the import and S016 for the final namespace read.
`a321` reports three S100 import failures and two S016 qualified-use failures.
The same CLI reports S016 for `r328`'s local export and S100 for its namespace import.

TypeScript 5.9.2 accepts all four entries under strict ES2022 checks with the repository prelude.
Node 24.18.0 matches the `a320` and `a321` goldens.
The `a319` await subset prints `3 3` and `5 5` on Node and the interpreter.
The full `a319` uses Worker adapters, which Node and the interpreter lack; its header states both exclusions.
The dev JIT and ship C AOT match the full `a319` golden, including `echo=37 echo=37`.

The remaining namespace codegen test checks six input orders and a changed-source control through the interpreter only.
Its measured debug cost is 25.824 ms. The native corpus tests cover the moved programs.
The generator adds the four entries to the corpus index. The LIR snapshot adds 23,015 bytes for `a319` only.
No existing `.expected` golden changes. No Phase Review result is recorded.

## Landing gate

The orchestrator amended C18 and the tutorial's module paragraph.

```text
gate full b0a8e55bd7b984ad4216b0bb628d3300fb2e3aac dirty:33 debug 2205/0/3 release 2202/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

## Phase Review fix round landing gate

`goldens-moved 1` is the LIR text snapshot: `a319` is an async entry.

```text
gate full b635e203414bdcac506b84713f7146bcc52bca82 dirty:25 debug 2206/0/3 release 2203/0/3 skips 2/0 clippy 2/18/13 goldens-moved 1 exit 0
```
