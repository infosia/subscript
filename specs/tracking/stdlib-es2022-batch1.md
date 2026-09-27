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

## Landing

Every accepted form rewrites to an existing intrinsic before lowering:
`trimLeft`/`trimRight` to `TrimStart`/`TrimEnd`, `toString` to `Join`
with `","`, `splice(start)` to `Splice` with a supplied count; the search
positions and the `split` limit are one more `i32` operand on the
existing runtime entries (computed in `i64`, so `length + fromIndex`
does not overflow). `a264-string-array-search-positions` matches `node`
byte for byte (175 bytes) on the dev JIT, the ship C, and the
interpreter. The LIR text snapshot moved by one line: `a149`'s
`indexOf` call gains the default position `0`.

A fresh review ran every rule against `node` and both tiers, including
non-ASCII byte offsets, `i32::MIN` and `i32::MAX` positions, a RegExp
separator with captures and empty matches, and a nested `toString`: no
CRITICAL or MAJOR. MINOR, open: a diagnostic names the resolved
intrinsic (`trimStart`, `join`) where the source spelled `trimLeft` or
`toString`.

`gate full f55bcde dirty:21 debug 1707/0/2 release 1704/0/2 skips 2/0 clippy 5/18/13 goldens-moved 1 exit 0`.
