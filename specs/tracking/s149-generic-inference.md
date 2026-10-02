# §149 — generic function inference

Date: 2026-10-02. Contract pin: `e726a16b`.

## Implementation

Named generic function calls infer arguments before instantiation. Imports and namespace calls use the same path, including direct async awaits.
Structural candidates cover every rule 2 shape. Non-literal candidates give literals their context; literal-only candidates use C4 defaults.
Conflicts and missing candidates report S100 with the parameter name and an explicit-type-argument fix.
The divergence block cites §149. Explicit type arguments bypass inference.
The checker reuses checked argument HIR. Each runtime argument evaluates once, in source order.

The opaque check records symbolic instance edges before concrete body checks. Diagnostic merging still occurs after both checks.
Inferred growing chains reject without a stack overflow. Plain recursion and finite constant-type chains pass.

## Red and external measurements

The CLI built offline from `e726a16b` rejected `a324-generic-inference/main.ts` with 17 S100 diagnostics and exit 1.
Each diagnostic had this message, with the function name from the table:

```text
error[S100]: generic function `id` requires explicit type arguments
error: 17 error(s)
```

All positions refer to `main.ts`:

| Function | Diagnostic positions |
|---|---|
| `id` | 19:12, 20:12, 40:12, 41:12 |
| `pair` | 21:12, 22:12, 26:12, 43:12, 44:12 |
| `first` | 28:12, 42:12 |
| `orNull` | 30:12 |
| `ns.id` | 31:15 |
| `importedId` | 32:12 |
| `second` | 33:12 |
| `nested` | 36:12 |
| `asyncFirst` | 37:23 |

Node 24.18.0 output equals the new golden. TypeScript 5.9.2 accepts a324 and r330–r332.
The conflicting Problem-table call belongs to r330. The accept entry compares the converted call with its explicit form.

## Expectations and verification

`async_generic.rs` now accepts `await first(items)` with `items: u32[]`. The same test rejects an async return-only type parameter.
`async_generic_method.rs` and `array_of_and_map_copy.rs` retain their rejections. Corpus r179 and r186 still pass.

The pin comparison covered 41,831 matrix and control verdicts. Changed cells: none.
The final §143 matrix passed 38,910 cells in 33.991 seconds. The compound matrix passed 396 cells in 0.566 seconds.
The final 38,910 verdicts also match the pin.

Both package suites passed with `cargo test --offline --locked`: compiler and codegen.
The new corpus passed interpreter, JIT, and C AOT. The JS corpus and measured tsc headers passed.
`cargo fmt --check` and `git diff --check` passed.

The document generator regenerated `generated-docs/`. The LIR generator added only the a324 section: 528 lines, 46,917 bytes.
Every existing LIR section remains byte-identical. No existing `.expected` file changed. No commit was made.

## Landing gate

`goldens-moved 1` is the LIR text snapshot: `a324` holds an async entry.

```text
gate full 9e18bb3e75a99103ff131e6c1d8ab7fddf448104 dirty:23 debug 2219/0/3 release 2216/0/3 skips 2/0 clippy 2/18/13 goldens-moved 1 exit 0
```

## Round 2 fixes and measurements

The literal-default check now stops inference after a diagnostic. Four call sites each report one S008: direct, second-parameter, namespace, and awaited calls.
The checker records null separately from candidate types. Null joins a candidate through the shared nullable acceptance rule.
The new a325 entry covers both argument orders beside explicit calls. Every existing `.expected` file stays unchanged.
The retained contract-pin CLI rejects a325 with two S100 diagnostics at 9:25 and 12:25.
Node 24.18.0 prints `true true` and `7 7`, which equal the new golden.
TypeScript 5.9.2 accepts a325 and r333 with the corpus options. The r333 entry pins conflicting literal defaults.
Missing candidates use a separate divergence entry with `empty()` and `empty<i32>()`.
The duplicate codegen inference test is deleted. The host-body source scan now excludes deleted tracked files.
The source-shape test fingerprints match the new null identity checks; their existing justification stays unchanged.
The §143 matrix passed 38,910 cells in 52.160 seconds. Every verdict matches the previous §149 verdict file; no cell changes.
The document generator adds only the a325 and r333 corpus-index rows. No new async entry changes the LIR snapshot.

A generic nullable candidate retains its existing nullable type when another argument is null. A unit test covers this inside a generic body.
Final package suites passed with `cargo test --offline --locked`: compiler and codegen.
The corpus sweeps passed interpreter, JIT, and C AOT, including a324 and a325. The JS corpus and measured tsc headers passed.
`cargo fmt --check`, `git diff --check`, and `tools/hygiene.sh` passed. No commit was made.

## Phase Review fix round landing gate

```text
gate full 6c8d3cdb936537d82b16fd9768de460a43d5d9be dirty:12 debug 2221/0/3 release 2218/0/3 skips 2/0 clippy 2/18/13 goldens-moved 0 exit 0
```

## Phase Review result

One review pass found no CRITICAL or MAJOR in §149 and MINOR 7. Five
are fixed (a duplicate diagnostic, a `null` argument joins to
`C | null`, the literal-only conflict pinned by `r333`, the
no-candidate example, a duplicate test); two are in §149.3. Outside
§149 the review found that a postfix update yields the new value; §150
takes it. §149 is COMPLETE.
