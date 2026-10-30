# A gate run compares the fastest runs — `compiler.md` §213

Status: implemented; Phase Review open.

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
| §200 | 1.53x | not recorded |
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
