# Benchmarks Rev 6 — the noise gate reads the interquartile range — evidence

Contract: `specs/blocks/benchmarks.md`, Timing methodology, Rev 6.
Date: 2026-09-27.

## Problem

The x86_64 Windows host (Intel64 Family 6 Model 198, 20 logical cores)
did not produce a clean `cross-language` run in five full runs at
0a066c9 and 514ab82. Each run withheld 2 to 7 cells. Each run exited 1.

The old rule withheld a subject if one sample was more than 20% from
the median. The table reports the median.

## Data

Sources: four full Windows runs (three at 0a066c9, one at 514ab82),
and the committed `benchmarks/results.json` at the 12 commits that
changed it (arm64 macOS). The quartiles are linear interpolations
(Rev 6 definition).

Interquartile range as a fraction of the median, over all cells:

| Host | Cells | p50 | p90 | p95 | p99 | max | old rule withholds | IQR > 15% |
|---|---|---|---|---|---|---|---|---|
| arm64 macOS | 546 | 0.6% | 3.0% | 5.2% | 9.5% | 12.9% | 1 | 0 |
| x86_64 Windows | 160 | 3.9% | 8.0% | 10.3% | 17.4% | 21.1% | 20 | 4 |

Every cell that the old rule withheld on Windows, with the count of
samples more than 20% from the median:

| Run | Cell | IQR | Samples beyond 20% | Rev 6 |
|---|---|---|---|---|
| 1 | fib-recursive/C | 1.9% | 1 | valid |
| 1 | fib-recursive/V8 | 2.1% | 1 | valid |
| 1 | tree/C | 8.3% | 1 | valid |
| 1 | callbacks/subscript-ship | 21.1% | 1 | withheld |
| 1 | collect/subscript-ship | 2.2% | 1 | valid |
| 1 | collect/subscript-jit | 2.4% | 1 | valid |
| 2 | fib-recursive/subscript-jit | 5.4% | 2 | valid |
| 2 | collect/subscript-jit | 8.9% | 2 | valid |
| 3 | fib-recursive/C | 0.6% | 1 | valid |
| 3 | fib-recursive/subscript-jit | 4.0% | 1 | valid |
| 3 | fib-recursive/V8 | 18.3% | 1 | withheld |
| 3 | sort/C | 2.5% | 1 | valid |
| 3 | tree/C | 4.2% | 1 | valid |
| 3 | tree/V8 | 9.0% | 2 | valid |
| 3 | collect/subscript-jit | 5.0% | 1 | valid |
| 4 | fib-recursive/subscript-ship | 2.6% | 2 | valid |
| 4 | fib-recursive/V8 | 2.0% | 1 | valid |
| 4 | tree/V8 | 16.7% | 2 | withheld |
| 4 | callbacks/subscript-ship | 15.0% | 1 | valid (14.98%) |
| 4 | collect/subscript-jit | 5.2% | 1 | valid |

Rev 6 also withholds run 2 callbacks/subscript-ship (IQR 15.3%, no
sample beyond 20%).

Medians of withheld cells against the same cell in other runs, ms
(`*` = the old rule withheld it):

| Cell | run 1 | run 2 | run 3 | run 4 |
|---|---|---|---|---|
| collect/subscript-jit | 123.412* | 128.907* | 122.146* | 126.037* |
| fib-recursive/C | 2.299* | 2.303 | 2.386* | 2.380 |
| tree/C | 158.073* | 148.768 | 146.683* | 152.830 |
| collect/subscript-ship | 51.125* | 52.329 | 50.282 | 52.161 |
| callbacks/subscript-ship | 51.100* | 48.070 | 53.364 | 45.912* |

`collect/subscript-jit` failed the old rule in all four runs. Its
median stays within 5.4% across them.

## Consequence

The old rule withheld stable medians for one or two outlying samples.
Rev 6 tests the central half of the samples, which the median depends
on. On the macOS records, Rev 6 withholds no cell.

## First run under Rev 6

Windows, df1a887 plus the Rev 6 runner. Rev 6 withheld one cell:
`tree`/V8, with an interquartile range of 45.7% of the median, and 4
of 11 samples more than 20% from the median. The old rule applied to
the same samples withholds six cells: fib-recursive/C,
fib-recursive/subscript-jit, tree/V8, particles/V8,
callbacks/subscript-ship, and collect/subscript-jit. Five of the six
have one sample beyond 20%.

## Limit of the gate

The gate reads one run. `callbacks`/V8 on Windows moves 25.7% across
four runs (507 ms in run 1, about 410 ms in runs 2 to 4) with an
interquartile range of 3.3% or less. No per-run rule sees this.
