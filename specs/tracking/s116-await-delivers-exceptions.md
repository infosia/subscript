# §116 An `await` delivers the exception of its handle — evidence

Contract: `specs/blocks/compiler/s116-an-await-delivers-the-exception-of-its-handle.md`.
Date: 2026-09-26. Host: arm64 macOS. Pin: `154cc3d` (the §115 landing).

## Owner decision (2026-09-26)

An exception that leaves an async body completes its handle, and an
`await` raises it. The owner chose this over two alternatives: keep the
`try` rejection, or lift it and keep the trap.

## Measurements before the contract

`node` v24.18.0, a plain-JS program:

| Shape | Output |
|---|---|
| `const h = fails(); print; await h` (throw before the first `await`) | `body start`, `after call`, `caught early` |
| second `await h` | `caught again early` |
| `===` on the two caught values | `same object true` |
| `await later()`, throw after an `await` | `caught late` |
| generator, `try` spans `yield`, throw after the resume | `gen caught gen` |
| unobserved rejection | the process ends, exit 1, with the error text |

This language at `154cc3d`: two `await`s of one handle return the same
value (`42 42`).

## Success-path baseline at `154cc3d` (§116.6 criterion 3)

`cross-language --only <id>`, release build; milliseconds.

| Workload | ship | jit |
|---|---|---|
| fib-recursive | 3.717 | 8.066 |
| callbacks | 37.473 | 459.675 |
| tree | 102.259, 101.599, 132.717 | 419.600, 417.295, 418.887 |

The ship `tree` median is bimodal on one binary: two consecutive runs
gave 101.599 and 132.717 ms. The §115 runs (131.778 to 132.283 ms) were
all in the upper mode.

`async-cost` (ship tier), two runs, median ns:

| Workload | run 1 | run 2 |
|---|---|---|
| settled-awaits | 28,549,000 | 28,057,000 |
| held-handles | 9,132,000 | 9,038,000 |
| deep-chains | 16,134,000 | 16,194,000 |

## Criterion 3 failed; the measurement round

After the implementation and three review fixes, `async-cost` (ship)
medians: settled-awaits 33,435,000 / 33,478,000 ns (+17% to +19%),
held-handles 9,961,000 / 9,943,000 (+9% to +10%), deep-chains
18,156,000 / 18,098,000 (+12%). `fib-recursive` +2.7%, `callbacks`
-1.3%, `tree` (upper mode) unchanged. The kill criterion fired; the
landing stopped (owner decision 2026-09-27: measure, then optimize).

A measurement round (codex, prototypes reverted, tree checksum equal
before and after) removed one candidate at a time, with `154cc3d`
built from an export in the same session. Median ms, run 1 / run 2:

| Variant | settled-awaits | held-handles | deep-chains |
|---|---|---|---|
| `154cc3d` | 28.043 / 28.404 | 9.005 / 9.088 | 15.910 / 15.688 |
| unchanged | 32.187 / 32.912 | 9.613 / 9.626 | 18.086 / 18.281 |
| A removed (rule 4a count) | 31.432 / 29.582 | 9.240 / 9.360 | 16.582 / 16.837 |
| B removed (release word check) | 34.048 / 35.021 | 10.139 / 9.834 | 18.171 / 17.977 |
| C removed (`AwaitRaise` check) | 35.193 / 34.685 | 9.766 / 9.909 | 18.355 / 18.185 |
| D removed (exception completion state) | 31.915 / 31.456 | 9.637 / 9.619 | 17.775 / 17.905 |

A reversed-order control on the saved binaries attributed 69%, 70%,
and 79% of the gap to A and 30%, 33%, and 41% to D. B and C were
within the noise (opposite signs between the runs). In the emitted C
of a direct `await`, the rule 4a release sits next to the existing
creator release. Two forms follow: a direct `await` moves the call's
count to the registration (rule 4a), and the exception payload is
stored out of line (§116.2 rule 1).

## The cost fix, and a second miss

The direct-await count moved to the registration, and the exception
payload moved out of line (`AsyncFrameMeta` 88 B to 64 B, equal to
`154cc3d`; the interpreter completion 72 B to 56 B). Quick gate:
`gate quick 153c4f9 dirty:52 debug 1656/0/2 skips 2 goldens-moved 2 exit 0`.
`async-cost` medians of two runs against `154cc3d` in the same session:
settled-awaits 29.365 ms (+1.90%), held-handles 10.126 ms (+11.97%),
deep-chains 16.639 ms (+4.95%). The kill criterion fired for
held-handles; the round stopped (owner decision 2026-09-27: move the
count's fast path into generated code, §116.2 rule 6).

## The inline count, and the owner's acceptance

§116.2 rule 6 moved the count's fast path into the generated code
(emitted C: a null test and a saturating increment for a retain; an
inline decrement above one, else the runtime release and its word
check). Quick gate:
`gate quick ca379c1 dirty:56 debug 1657/0/2 skips 2 goldens-moved 2 exit 0`.
`async-cost`, tree/pin alternated, medians of two runs: settled-awaits
29.758 vs 28.456 ms (+4.57%), held-handles 9.841 vs 9.080 ms (+8.39%),
deep-chains 16.533 vs 16.532 ms (+0.00%). `fib-recursive` ship
3.714 ms. The owner accepted `held-handles` at +8.4% on 2026-09-27
(§116.6 criterion 3).
