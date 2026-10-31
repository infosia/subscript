# Inference reads `Promise<T>` and `Array<T>` — `compiler.md` §215

Status: implemented; Phase Review open.

## Measurement at the pin `fbf3509a`

Tool: `tsc` 5.9.2, strict, target es2022.

| Program | `tsc` |
|---|---|
| `const b: Buf<P> = await af();` | no error |
| `const p2: Promise<Buf<P>> = af();` | no error |
| `return af();` with result `Promise<Buf<P>>` | no error |
| `const r = await fr(q);` | no error |
| `const x: P[] = mk();`, `mk<T>(): Array<T>` | no error |
| `const b = await az();`, `b.push(1);` | no error: `T` is `unknown` |
| `const b: number = (await az())[0];` | TS2322: `unknown` to `number` |

subscript gives S100 "the return value expects `Buf<P>`, got
`Promise<Buf<P>>`" for `return af<P>();`, so the third row is §167.1 rule
3 and not inference. `const p: Promise<Buf<P>> = af<P>();` is accepted.

## Implementation

Red, on a first draft of `a398`, with a binary built from `3769dcc8`: 11
errors, 10 × S100 "cannot infer type parameter" and 1 × S013 at an async
arrow in the draft. The final entry replaces that arrow, and it has no
separate Red run.

`a398` passes `tsc`, `node`, and the three tiers. No `.expected` file
moves. `codegen/tests/lir-goldens/corpus.txt` grows by the `a398`
section only.

A form that stays rejected and now gives another text: `await sf(q)`
with a synchronous `sf` gives "`sf<P>` is synchronous and cannot be
awaited", and `const r: i32 = await fr(1);` gives "the argument expects
`Promise<i32>`, got `i32`".
