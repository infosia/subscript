# ES2022 stdlib batch 3 — `Date`, `toFixed()`, `Infinity` — evidence

Contract: `specs/blocks/stdlib.md` §3.1 and §11.4a (Rev 17).
Date: 2026-09-27. Host: arm64 macOS.

`tsc` 5.9.2 (lib `ES2022` + `ESNext.Disposable`, strict) accepts
`Infinity`, `(1.5).toFixed()`, `d.toJSON()`, `d.toUTCString()`,
`d.valueOf()`, `new Date(d)`, and `Date.UTC(2020)` (exit 0).

`node` v24.18.0:

```
Infinity Infinity -Infinity true 0
toFixed() 2 3 -2 1235 0 0 1e+21 NaN 1 1.4
toJSON 2023-11-14T22:13:20.123Z 2023-11-14T22:13:20.123Z
toUTCString Tue, 14 Nov 2023 22:13:20 GMT Thu, 01 Jan 1970 00:00:00 GMT Wed, 31 Dec 1969 23:59:59 GMT Fri, 01 Jan 1999 00:00:00 GMT
far Sat, 01 Jan 10000 00:00:00 GMT Fri, 01 Jan -0001 00:00:00 GMT Fri, 01 Jan -0001 00:00:00 GMT
valueOf 1700000000123 number
copy 1700000000123 false
UTC(year) 1577836800000 1577836800000 915148800000 0
```

This language at `36665da` (dev JIT): `Date.UTC(99, 0)`,
`Date.UTC(2020, 0)`, `Date.UTC(0, 0)`, `Date.UTC(-1, 0)` give
`915148800000 1577836800000 -2208988800000 -62198755200000`, equal to
`node`; `toFixed(0)` of 1.5, 2.5, −1.5, 0.5, 1e21, −0 gives
`2 3 -2 1 1e+21 0`, equal to `node`'s `toFixed()`.

## Before the implementation

The checker at `8cd9592`: `new Date(d)` S100 (expects `i64`),
`Date.UTC(year)` S100 (two arguments required), `valueOf()`, `toJSON()`,
`toUTCString()` S014 (the removed rows), `toFixed()` S014 (one argument
required), `Infinity` S016. No reject entry pinned the three removed
S014 rows.

## Landing

`new Date(d)` reuses `Date.New`; `Date.UTC(year)` gets month 0 from the
checker; `valueOf()` is `getTime()`; `toJSON()` is `ToIso` with its range
trap; `toUTCString()` is one new intrinsic over the shared civil-date
decomposition (`runtime/src/date.rs`); `toFixed()` is `ToFixed(0)`;
`Infinity` is an `f64` constant. `a266-date-es2022-additions` equals
`node` byte for byte (673 bytes) on the dev JIT, the ship C, and the
interpreter. The LIR text snapshot gained 48 lines, one
`ToUtcString` intrinsic declaration per snapshot.

A fresh review checked `toUTCString` against `node` over years −1, 0,
99, 100, 999, 1000, 9999, 10000, both TimeClip ends, and every weekday;
`toFixed()` over halfway cases, −0, 1e21, `NaN`, ±`Infinity`, and `f32`
receivers; `Infinity` in integer contexts (S007) and shadowed by a local
(the local wins, `tsc` accepts it). No CRITICAL or MAJOR. MINOR: the
`toISOString`/`toJSON` year-range trap is a divergence from `node` that
Q20 did not name (now named in `collisions.md` Q20); open: the `toJSON()`
range trap message names `toISOString`; a rejection test asserts
`is_err()` only for ten rows; three files past 2,000 lines grew
(`runtime/src/ffi.rs` 7,540, `compiler/src/hir.rs` 5,257,
`codegen/src/interpreter.rs` 7,250).

`gate full 8cd9592 dirty:21 debug 1722/0/2 release 1719/0/2 skips 2/0 clippy 5/18/13 goldens-moved 1 exit 0`.
