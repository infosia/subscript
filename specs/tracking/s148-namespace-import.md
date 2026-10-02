# §148 — static namespace imports

The checker binds a namespace to its source module. Each member resolves through the export graph to an ordinary import binding.
The binding keeps its declaration identity and read-only status. No namespace value reaches HIR.
Missing modules keep qualified uses poisoned. Discovery records the namespace spelling. Type-only namespace imports remain rejected.

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
- Namespace corpus, local shadow, and module-cycle tests pass on all three engines.
- Unit tests cover host exports, import writes, missing members, missing modules, and discovery poison.
- The default-import diagnostic and its existing test stay unchanged.
- The document generator updates `generated-docs/corpus-index.md`. Other generated documents and the LIR snapshot stay unchanged.
- No existing `.expected` golden changes. The tutorial edit remains orchestrator-owned.
- A fresh Phase Review reports no CRITICAL, MAJOR, or MINOR findings.
- `cargo fmt --check`, `git diff --check`, and `tools/hygiene.sh` pass.

## Landing gate

The orchestrator amended C18 and the tutorial's module paragraph.

```text
gate full b0a8e55bd7b984ad4216b0bb628d3300fb2e3aac dirty:33 debug 2205/0/3 release 2202/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```
