# ES2022 stdlib batch 5a — `at`, `find`/`findLast`, `findLastIndex`, `flatMap` — evidence

Contract: `specs/blocks/stdlib.md` §8.10 and §9.10 (Rev 19).
Date: 2026-09-27. Host: arm64 macOS.

Owner decision (2026-09-27): an `at` miss traps.

`tsc` 5.9.2 with the repository prelude plus the augmentation

```ts
declare interface Array<T> {
  at(index: i32): T;
  findLast(predicate: (value: T, index: i32) => boolean): T | undefined;
  findLastIndex(predicate: (value: T, index: i32) => boolean): i32;
}
declare interface String { at(index: i32): string; }
```

accepts `const x: i32 = a.at(-1)`, `const s: string = "abc".at(0)`,
`cs.find(...) === null`, `cs.findLast(...)`, `cs.findLastIndex(...)`,
`a.flatMap((v: i32): i32[] => [v, v])`, and `cs.find(...) ?? new C()`
(exit 0).

`node` v24.18.0:

```
1 3 undefined undefined c undefined é        (at: 0, -1, 3, -4; "abc".at(-1), at(5); "héllo".at(1))
0 2 -1 {"v":1} undefined                     (findIndex, findLastIndex, miss, findLast, find miss)
[1,10,2,20,3,30] [] [1,2] [0,1]              (flatMap)
```
