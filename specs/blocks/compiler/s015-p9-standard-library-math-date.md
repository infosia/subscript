<!-- §15 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 15. P9 — standard library (`Math`, `Date`)

Contract in `specs/blocks/stdlib.md`; collision resolutions Q19/Q20 in
`collisions.md` §2. Compiler surface: ambient-namespace intrinsic calls
(`Math.<fn>`), checker-folded constant member reads, and an ambient
nominal value type erasing to `i64` (`Date`); every runtime-backed
operation lowers to an opaque `subscript_rt_math_*`/`subscript_rt_date_*` call on
both tiers (never a direct libm emission — clang constant-folds libm at
`-O2`, a cross-tier divergence hazard, `stdlib.md` §0.2). Gate:
`stdlib.md` §5.
