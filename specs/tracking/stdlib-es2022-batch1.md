# ES2022 stdlib batch 1 — search positions, `split` limit, aliases — evidence

Contract: `specs/blocks/stdlib.md` §8.9 and §9.9 (Rev 15).
Date: 2026-09-27. Host: arm64 macOS.

## Origin

An inventory of the ES2022 lib against `generated-docs/api-reference.md`
listed the missing members by cost. The owner chose five batches and
four design answers on 2026-09-27: an `at` miss traps; a negative
`split` limit follows JS; a new Error class does not turn an existing
trap into an exception; a `@deprecated` alias is accepted as `substr`
was. This note covers batch 1.

## Measurements before the contract

`tsc` 5.9.2, lib `ES2022` + `ESNext.Disposable`, strict: every form of
§8.9 and §9.9 checks clean (exit 0).

`node` v24.18.0:

| Expression | Result |
|---|---|
| `"abcabc".lastIndexOf("c", 3)`, `(…, 100)`, `(…, -5)` | 2, 5, −1 |
| `"abcabc".lastIndexOf("a", 0)`, `lastIndexOf("", 2)`, `lastIndexOf("", 99)` | 0, 2, 6 |
| `[1,2,3,1,2].indexOf(1, 1)`, `(1, -2)`, `(1, -100)`, `indexOf(2, 99)` | 3, 3, 0, −1 |
| `[NaN,1].includes(NaN, 0)`, `(NaN, 1)` | true, false |
| `[1,2,3,1,2].lastIndexOf(1, 2)`, `(1, -3)`, `(1, -100)`, `lastIndexOf(2, 99)` | 0, 0, −1, 4 |
| `[1,2,3,4,5].splice(2)` / receiver after | `[3,4,5]` / `[1,2]` |
| `[1,2,3].splice(-1)`, `[1,2].splice(9)` | `[3]`, `[]` |
| `"a,b,c".split(",", 2)`, `(",", 0)`, `(",", -1)` | `["a","b"]`, `[]`, `["a","b","c"]` |
| `"a1b2c".split(/\d/, 1)`, `"abc".split("", 2)` | `["a"]`, `["a","b"]` |
| `[1,2].toString()`, `[[1,2],[3]].toString()` | `"1,2"`, `"1,2,3"` (the nested form stays rejected with `join`, S014) |
| `"  x  ".trimLeft()`, `.trimRight()` | `"x  "`, `"  x"` |

## Before the implementation

The checker at `48c802c` rejected every new form with S100: string
`lastIndexOf(needle, position)`, `split(sep, limit)` for a string and a
RegExp separator, `trimLeft()`, `trimRight()`, array `indexOf`,
`includes`, and `lastIndexOf` with a position, `splice(start)`, and
`toString()` on a scalar and a nested array.
