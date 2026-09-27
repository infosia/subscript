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
