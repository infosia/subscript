# A gate run compares the fastest runs — `compiler.md` §213

Status: COMPLETE.

## Measurement at the pin `1ffbb18d`

Host: 8 CPUs, aarch64, macOS. Each row is five `perf-gate --gate` runs
under N busy shell loops, in one session.

| N | `a22` ship median ratio | `a22` ship min ratio | `a22` dev median | `a22` dev min | Exit status |
|---|---|---|---|---|---|
| 0 | 1.30–1.34 | 1.30–1.34 | 19.2–19.9 | 19.4–19.9 | 0 × 5 |
| 4 | 1.32–1.33 | 1.32–1.33 | 21.4–22.2 | 20.2–21.4 | 0 × 5 |
| 8 | 1.33–1.37 | 1.33–1.36 | 28.1–31.0 | 19.7–20.1 | 1 × 5 |
| 16 | 1.16–5.03 | 1.33–1.35 | 16.4–56.0 | 47.4–52.9 | 1 × 3, 0 × 2 |

The miss at N = 8 is `a22` dev-JIT against 25x. At N = 16 the median
ratio also falls below the quiet value, so a median can hide a
regression under load.

The two gate misses in the record:

| Run | Median ratio | Min ratio |
|---|---|---|
| §200 | 1.53x | 6.182 / 4.760 = 1.30x |
| §212 | 1.87x | 6.635 / 5.018 = 1.32x |

## Rerun with the minimum

The harness of §213.1 rule 1, the same five runs per load. Each exit
status is the one that the minimum predicted at the pin.

| N | `a22` ship min ratio | `a22` dev min | Exit status |
|---|---|---|---|
| 0 | 1.33–1.34 | 19.8–19.9 | 0 × 5 |
| 4 | 1.33–1.36 | 21.0–21.9 | 0 × 5 |
| 8 | 1.33–1.38 | 19.7–22.1 | 0 × 5 |
| 16 | 1.34–1.38 | 50.1–55.1 | 1 × 5 |

The miss at N = 16 is `a22` dev-JIT against 25x (§213.1 rule 7).

Red: with the gate figure set to the median, the unit test of the §212
samples fails with 1.87x against the 1.32x it expects.

## Phase Review measurement

The review ran the wall-minimum harness at load steps that the first
table does not have. The miss starts between N = 8 and N = 10.

| N | Runs | Exit status | `a22` dev wall min ratio |
|---|---|---|---|
| 8 | 3 | 0 × 3 | 19.9–22.1 |
| 10 | 5 | 1 × 5 | 26.8–32.1 |
| 12 | 5 | 1 × 5 | 34.4–38.8 |

The two recorded gate misses have dev wall min ratios of 17.65x (§200)
and 22.68x (§212), so the busy loops are harsher than the real load.

Load that changes during a run, 16 gate runs at N = 12 and 16, no
regression in the program: load that starts between C and ship gives a
ship ratio of 1.42–1.43, and load that stops between them gives
1.21–1.26. No false ship miss appeared.

## Thread CPU time

Five `--gate` runs per load with §213.1 rule 1a. One clock read costs
106–123 ns.

| N | Exit status | `a22` dev wall min | `a22` dev CPU figure | `collect` dev CPU figure |
|---|---|---|---|---|
| 0 | 0 × 5 | 19.80–19.89 | 19.80–19.89 | 3.60–3.64 |
| 8 | 0 × 5 | 19.67–20.05 | 19.67–20.05 | 3.64–3.84 |
| 10 | 0 × 5 | 26.52–31.53 | 22.57–24.51 | 4.61–4.77 |
| 12 | 0 × 5 | 33.80–38.77 | 23.36–24.23 | 2.33–3.05 |
| 16 | 0 × 5 | 48.04–51.80 | 23.02–24.48 | 1.67–2.40 |

The `a22` CPU minimum grows from 78.6 ms to 104–115 ms at N ≥ 10, and
the C minimum grows 1.19x. The cause of that growth is not measured.
The `collect` C wall minimum grows to 105 ms at N = 16, so the
`collect` figure falls below its quiet value (§213.1 rule 3).

`gate full d6d8e604 dirty:9 debug 3053/0/5 release 3050/0/5 skips 2/0
clippy 5/18/11 goldens-moved 0 exit 0`. `tools/hygiene.sh` exit 0. The
first quick gate of the fix stopped at the build step on `xcrun` cache
warnings; the owner ran the `xcrun` lookups outside the sandbox.
