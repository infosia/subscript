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
