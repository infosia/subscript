# ES2022 stdlib batch 5b — `Array.of` at fixed arity, `new Map(otherMap)` — evidence

Contract: `specs/blocks/stdlib.md` §9.11 and §10.9 (Rev 20);
`compiler.md` §103.5 item 3 closed, §105.3 row amended.
Date: 2026-09-27. Host: arm64 macOS.

`tsc` 5.9.2 (with the repository prelude) accepts `Array.of<i32>()`,
`Array.of<i32>(7)`, `Array.of(7)`, `new Map(m)`, and
`new Map<string, i32>(m)` (exit 0).

`node` v24.18.0:

```
[] [7] [1,2]                 Array.of(), Array.of(7), Array.of(1, 2)
a,b,c b 3 false 1            n = new Map(m); n.set("c",3); m.delete("a"): n keys, m keys, n.size, n === m, n.get("a")
true                         a reference value is the same object in the copy
```

## Before the implementation

The checker at `26fabb8`: `Array.of<i32>()`, `Array.of(7)`,
`Array.of<i32>(7)`, `Array.of()`, `Array.of<i32>(1, 2)` S014;
`new Map(m)` S100 (explicit type arguments required);
`new Map<string, i32>(m)` and a pair-array source S014 (no tuple type).

## §9.11 rule 4 measurement

A test shows that `Array.of<i32>(7)` and `[7]` lower to identical LIR
text for the same function; the emitted C differs only in the source
position table. Checker wall time over every corpus accept, warn, trap,
and reject entry (with their declarations and multi-file imports),
release build, median of 5 runs: `26fabb8` 93.585 ms over 558 entries;
the working tree 90.046 ms over 559 entries. No cost is measurable.

## First round

`Array.of` rewrites to the literal it equals; `new Map(m)` is one
`MapFromSource` instruction lowered on all three engines to
`subscript_rt_map_from_assoc`, which inserts through the one path `set`
uses. `a269-array-of-and-map-copy` equals `node` byte for byte on all
three engines. `r200` now pins the pair-array form. A first quick gate
failed on the entry's `tsc` check (a `Map.get` result used without a
miss test, TS18048); a second found `Array.of<Worker<…>>()` accepted
where the literal is rejected, and `Array.of` now goes through the
literal's element-type check.
`gate full 26fabb8 dirty:27 debug 1761/0/3 release 1758/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.

## Review

MAJOR: a freed `Map` source gave `trap [internal]` on the dev JIT (and
a silent empty copy on ship, where use-after-delete is undefined,
§8.1a); `m.set` and `m.size` on the same freed map trap
`use-after-delete`. The dev-only lifetime site is derived for a method
receiver only, so a container operand of any other runtime operation
has none; `new Set(s)` on a freed `s` has the same hole. MAJOR: the
`new Map(...)` check replaced the source expression's own diagnostics
(an unknown name, an arity error, a nullable source) with a generic
rejection. MINOR: an `#[ignore]` without a reason, duplicate group
headings in the API reference, two codes for one type-argument-arity
mistake.

The first MAJOR is a defect of the form: a `DevOnlyLifetime` site
carries no operand, so a second site on one call cannot say what it
tests (the coding agent's stop report). It moves to `compiler.md` §120.

## Second round

The source diagnostics of `new Map(...)` stay: an unknown name S016, an
argument error of the source its own, a nullable source S011 (C7), a
`Set` source S014 (the pair-iterable rule). A type-argument-arity
mistake is S100 for `Array.of` and `new Map`.
`gate full 26fabb8 dirty:31 debug 1766/0/3 release 1763/0/3 skips 2/0 clippy 5/18/13 goldens-moved 0 exit 0`.
