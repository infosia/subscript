# §47 — AAPCS64 packs eightbytes, not fields

Status: **fixed and verified 2026-08-03** against `compiler.md`
§47; §12.3a's AAPCS64 wording corrected in the same landing.
Origin: downstream observation OBS-4, accepted as a miscompile —
the dev tier silently delivered wrong values on
`aarch64-apple-darwin` for a by-value boundary struct with two or
more sub-eightbyte integer fields, while the ship tier was correct.

## Cause

§12.3a recorded the AAPCS64 rule as passing a ≤16-byte struct as
"its components as arguments". AAPCS64 B.4 passes a small non-HFA
composite as **eightbyte images** instead. Component-wise passing
put the first field in `x0` and the second in `x1`; the callee read
`x0` as the whole first eightbyte and saw the second field as 0,
and at three fields the original second field arrived in the third
position. `{i64,i64}` was unaffected (each field is already an
eightbyte) and HFAs were unaffected (component-wise in float
registers is correct for them) — which is why the error survived.

The defect was in this project's own contract text, not only its
code.

## Pre-fix matrix (reviewer-reproduced by stashing the fix)

    {i32}            a=3                        correct
    {i32,i32}        x=3 y=0                    wrong (y lost)
    {i32,i32,i32}    a=3 b=0 c=7                wrong (b lost, b's value in c)
    {i16,i16,i32}    a=-3 b=0 c=0               wrong
    {u8,u8,u8,u8}    a=3 b=0 c=0 d=0            wrong
    {i64,i64}        a=3 b=7                    correct
    {f32,f32}        a=3.25 b=7.5               correct (HFA)
    {f32×4}          a=1.25 b=2.5 c=3.75 d=4.5  correct (HFA)
    {i32,f32}        a=3 b=0                    wrong
    {i32,pad,i64}    a=3 b=7                    correct
    {i64,i64,i64}    a=3 b=7 c=11               correct (by reference)

Five of eleven shapes were mis-delivered. The committed `a126`
golden carries the correct values for all eleven.

## Why neither side could find it

Verified at the pin: **no fixture function took a by-value struct
with two or more sub-eightbyte integer fields.** Every by-value
aggregate the corpus passed was a `(pointer, count)` descriptor, a
string view, or a two-`i64`/HFA shape — none of which need
packing. The downstream's facade passes every struct by pointer, so
its own programs could not reach it either.

This is the third instance of one pattern in this exchange: OBS-3's
cause was a *returned* descriptor where every entry built one
inline; R19's was a nullable *local* where every entry constructed
inline; this one is a field composition neither side's code
produced. The corpus pins the constructs its authors already knew.

## §47.4 evidence (reviewer-run)

1. Pre-fix matrix above, reproduced by the reviewer by stashing the
   lowering change and re-running `a126` through the capture
   harness — not taken from the implementer's report.
2. `a126-interop-by-value-packing` byte-identical under both tiers
   across all eleven shapes.
3. Lowering unit tests assert the register-image plan per shape
   (two `i32`s packed into one eightbyte at offsets 0 and 4; a
   12-byte struct as two eightbytes; HFAs component-wise; >16 bytes
   by reference), plus an `offsetof` layout proof — the class, not
   only the instances.
4. Gate 50 harnesses, 878 passed, 0 failed, exit 0 read directly;
   `tsc` exit 0; no existing golden moved; Win64's packed rule
   unchanged and unit-tested; SysV still a loud error.
