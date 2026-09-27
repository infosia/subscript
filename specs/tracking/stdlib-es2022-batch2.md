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

## Before the implementation

The checker at `38aa4ed` rejected each accessor with S100 "no accepted
member" and `toString()` with S100 "no accepted method".

## Landing

Eight `RegexFn` intrinsics read the stored flag set and the stored
`source`/`flags` handles; `toString` makes one allocation and reuses the
one escaping routine. The boolean accessors allocate nothing: a runtime
test checks all 96 accepted flag sets with an allocation-failure
injection as the firing control. `a265-regexp-flag-accessors` equals
`node` byte for byte (426 bytes) on the dev JIT, the ship C, and the
interpreter.

The LIR text snapshot moved by 384 added lines and no removed line: the
eight new intrinsic declarations appear in each of 48 snapshots, because
the LIR text lists every declared intrinsic.

In the same round, a diagnostic names the method the source spelled:
`"abc".trimLeft(1)` says `trimLeft` (it said `trimStart`), and an
element type that `join` rejects says `toString` for `toString()`.

A fresh review: no CRITICAL or MAJOR. MINOR, open: the eight new
`subscript_rt_regex_*` exports sit after the test module of
`runtime/src/ffi.rs`, not beside the other regex exports; a write to an
accessor (`/a/.global = true`) says "no accepted member", where `tsc`
says TS2540 read-only (the shape already held for `source` and
`flags`); three files past 2,000 lines grew (`runtime/src/ffi.rs`
7,523, `codegen/src/interpreter.rs` 7,246, `compiler/src/hir.rs` 5,240).

Found by the review, outside this batch: the literal `/a/v` is accepted
by the checker, but `tsc` at the ES2022 lib rejects it with TS1501
(only when targeting ES2024). `new RegExp("a", "v")` is `tsc`-clean.
Invariant 5 does not hold for the literal.

`gate full 38aa4ed dirty:21 debug 1713/0/2 release 1710/0/2 skips 2/0 clippy 5/18/13 goldens-moved 1 exit 0`.
