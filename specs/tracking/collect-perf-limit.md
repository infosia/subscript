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

## Measurement on x86-64 with `cl` (§225.2)

Host: 20 logical CPUs, x86-64, Windows, MSVC 19.44.35222, at `75ef5164`.
Each row is five `perf-gate --gate` runs under N busy processes. The
figures are those of §213.1. This host has no thread CPU clock in the
harness, so the dev-JIT figure is a wall minimum (§213.1 rule 1a).

| N | `collect` ship | `collect` dev | `a22` ship | `a22` dev |
|---|---|---|---|---|
| 0 | 1.14–1.42 | 2.61–3.52 | 1.73–1.75 | 5.96–6.90 |
| 5 | 1.28–1.34 | 3.18–3.60 | 1.72–1.77 | 5.89–6.58 |
| 10 | 1.28–1.30 | 3.08–3.29 | 1.70–1.89 | 5.67–6.73 |
| 20 | 1.38–1.44 | 3.15–3.50 | 1.73–1.74 | 6.22–8.51 |
| 30 | 1.45–1.56 | 2.50–5.25 | 1.67–1.97 | 8.90–14.43 |
| 40 | 1.60–1.68 | 5.96–6.37 | 1.72–2.03 | 19.20–26.60 |

The 25 runs at N ≤ 30 met each limit. At N = 40, two busy processes per
CPU, each of the five runs missed: four runs read `collect` dev-JIT
6.04x to 6.37x against 6.00x, and one run read `a22` dev-JIT 26.60x
against 25.00x.

The `collect` ship figure stays far under the x86-64 limit of 7.50x:
the highest figure is 1.68x.

## Landing of §226

Three quiet `perf-gate --gate` runs at the landing, exit 0 each:

| Run | `collect` ship | `collect` dev | `a22` ship | `a22` dev |
|---|---|---|---|---|
| 1 | 0.90x | 1.34x | 1.65x | 7.22x |
| 2 | 1.34x | 3.51x | 1.72x | 5.97x |
| 3 | 1.15x | 3.10x | 1.79x | 6.37x |

One run during other builds on the same host read `collect` ship 4.75x
and `collect` dev 7.81x, and missed both. That run is not a quiet run
(§213.1 rule 7).

`tools/gate.sh full`: exit 0, debug 3103 passed, release 3100 passed, 0
failed. No golden moved.
