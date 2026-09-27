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

## Before the implementation

The checker at `7be3ff3`: array `at` S100 (no accepted member); string
`at` S014 (no scalar miss value); `find`/`findLast` S014 even for a
reference element (naming `findIndex`); `findLastIndex` S100;
`flatMap` S014 (the depth type undecided).

## Landing

The runtime reuses the existing `search` loop (a found flag and a
reverse direction), `map_source` (a flatten flag), and one
`append_array` shared with `concat`. `a268-array-string-at-find-flatmap`
equals `node` byte for byte on the dev JIT, the ship C, and the
interpreter. The `at` traps (out of range, off a UTF-8 boundary, the
`i32` extremes) have the kind and position of their siblings on all
three engines. The LIR text snapshot gained 288 declaration lines only.

A fresh review found no CRITICAL or MAJOR. It verified the traps, the
same object on a `find` hit and `null` on a miss, exceptions from every
callback reaching the caller's `try`, and that `flatMap` copies each
callback result (no aliasing). MINOR, fixed: §9.10 now states that
`FixedArray` gains none of the members. MINOR, open: the string `at`
trap message names `codePointAt`/`charAt` and the normalized index,
and the runtime decodes the index twice; the `FixedArray` rejection
text says the closure-taking family is accepted while it rejects
`find`; four files past 2,000 lines grew (`runtime/src/arrops.rs`
2,920, `runtime/src/ffi.rs` 7,870, `compiler/src/hir.rs` 5,332,
`codegen/src/interpreter.rs` 7,318).

The first full gate failed its clippy step:
`gate full 7be3ff3 dirty:23 debug 1752/0/2 release 1749/0/2 skips 2/0 clippy 5/20/13 goldens-moved 1 exit 1`.
Two new `clippy::too_many_arguments` warnings in `runtime/src/arrops.rs`
(`flat_map`, and `map_source`, which had gained a flatten flag). The
callback pair is one argument and the result shape is one `MapResult`
value now, so each takes seven; the runtime lib count is 18 again.
The one re-run after that cause:
`gate full 3ea3b34 dirty:3 debug 1752/0/2 release 1749/0/2 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.
