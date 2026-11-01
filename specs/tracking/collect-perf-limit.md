# The `collect` limits follow the measurement — `compiler.md` §222

Status: COMPLETE.

## Measurement at `1f55d944`

Host: 8 CPUs, aarch64, macOS. Each row is five `perf-gate --gate` runs
under N busy shell loops. The figures are those of §213.1.

| N | `collect` ship | `collect` dev |
|---|---|---|
| 0 | 1.12–1.15 | 3.64–3.73 |
| 4 | 1.12–1.26 | 3.91–4.07 |
| 8 | 1.11–1.14 | 3.62–4.15 |
| 10 | 1.05–1.15 | 4.12–5.05 |
| 12 | 0.90–1.07 | 2.03–2.49 |
| 16 | 0.93–1.29 | 1.58–2.08 |

`collect` met each limit of §222 in each of the 30 runs. One run missed
for another reason: at N = 12, `a22` ship-tier read 1.86x against 1.50x
(§213.1 rule 7).

## Selection

| Subject | Limit | Over the highest figure | Over the quiet figure |
|---|---|---|---|
| ship, aarch64 | 1.50x | 16% (1.29x) | 30% (1.15x) |
| dev | 6.00x | 19% (5.05x) | 61% (3.73x) |

The dev limit is wider because load moves the dev figure more than the
ship figure.

The dev figures at N ≥ 12 and the ship figures at N = 12 fall under
the quiet figures because the C baseline rises under load (§213.1
rule 3). The ship figure at N = 16 reaches 1.29x, and the other four
runs at that load read 0.93x to 0.99x.
