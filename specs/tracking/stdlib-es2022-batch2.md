# ES2022 stdlib batch 2 — `RegExp` flag accessors and `toString` — evidence

Contract: `specs/blocks/stdlib.md` §15.3a (Rev 16).
Date: 2026-09-27. Host: arm64 macOS.

`tsc` 5.9.2 (lib `ES2022` + `ESNext.Disposable`, strict) accepts the
seven accessors and `toString()` on a `RegExp` (exit 0).

`node` v24.18.0, `String(re)` and the accessors `global ignoreCase
multiline dotAll unicode sticky hasIndices`, then `toString()`,
`source`, `flags`:

```
/a/ false false false false false false false /a/ "a"
/a/gimsu true true true true true false false /a/gimsu "a" gimsu
/a/d false false false false false false true /a/d "a" d
/x/y false false false false false true false /x/y "x" y
/a\/b/ false false false false false false false /a\/b/ "a\\/b"
/a\/b/ false false false false false false false /a\/b/ "a\\/b"
/(?:)/ false false false false false false false /(?:)/ "(?:)"
/[/]/ false false false false false false false /[/]/ "[/]"
```

The `y` row is `node` only: this language rejects `y` (§15.3).
