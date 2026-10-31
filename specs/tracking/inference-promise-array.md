# Inference reads `Promise<T>` and `Array<T>` — `compiler.md` §215

Status: contract.

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
