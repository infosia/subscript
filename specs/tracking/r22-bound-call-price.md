# R22 — the price of one bound call

Status: **measured and recorded 2026-08-08** against
`specs/blocks/benchmarks.md`, "Boundary price (`bound-call`)".
Origin: downstream request R22. Contract `544aa61` + `e0375ac`,
implementation `fc51bf1`.

## The question

R22 measured our ship-tier boundary from outside: 27–36 ns for one
bound call above raw C, on two backends, across five runs. R22 asked
for (1) an inside measurement that separates the candidates and (2) a
statement on whether the cost can shrink.

## The instrument

Five executables, one compiler (`/usr/bin/clang`), one flag set
(`-std=c11 -O2 -fwrapv -ffp-contract=off`), each in a fresh process.
The region is 1000 pairs: `bnSetBindGroup(u32, handle, u32[1])` then
`bnDraw(u32 ×4)`, against a no-op backend in a separate translation
unit. Policy per variant: warm-up to the 200 ms floor, then 15 timed
samples, median reported. Run it with:

    cargo run --offline --release -p subscript-benchmarks --bin bound-call

## Finding 1 — the first run measured the clock, not the boundary

The first run used `CLOCK_MONOTONIC`. Its quantum on the arm64 dev
machine is 1000 ns (smallest positive delta over 100 000 pairs), a
region spans 5–7 µs, and every layer delta equaled the quantum. The
contract now carries a quantum gate: the backend measures the quantum
and the runner rejects a variant whose quantum exceeds 1% of its
median region span. `CLOCK_MONOTONIC_RAW` measures 41 ns on the same
machine and passes.

## Finding 2 — the boundary costs 1.0–1.2 ns per bound call

Reviewer run (arm64 dev machine, macOS, Apple M2 class, idle;
representative of three runs):

| variant | ns/pair | ns/call | IQR% |
|---|---|---|---|
| `script` (real emitted C) | 6.958 | 3.479 | 0.01 |
| `mimic` (hand-copied body) | 6.958 | 3.479 | 0.59 |
| `no-trap` | 4.833 | 2.417 | 0.87 |
| `hoisted` | 6.250 | 3.125 | 1.33 |
| `floor` (direct calls) | 4.875 | 2.438 | 0.86 |

All five checksums identical (300000); every spread and quantum gate
passed; `script` and `mimic` differ by 0.00–4.54% across runs, so the
decomposition is valid.

- **Total boundary** (`script − floor`): 2.1–2.5 ns per pair =
  **1.0–1.2 ns per bound call**.
- **Trap checks** (`mimic − no-trap`): 2.1–2.4 ns per pair for two
  checks, ~1.1 ns per check. The largest layer.
- **Array accessor calls** (`mimic − hoisted`): 0.7–1.1 ns per pair
  for the two runtime calls (`subscript_rt_array_data`/`_len`), each
  a single header load behind an extern call.
- The layers overlap: their sum (2.8–3.5) exceeds the total
  (2.1–2.5) because the out-of-order core hides part of each layer
  under the calls. The layers bound each other; they do not add.

**R22's 27–36 ns per bound call does not reproduce.** Our measured
boundary is 25× smaller than R22's outside measurement of the same
construct on the same CPU class. The difference lives in the
downstream's path, not in the emitted C's per-call work: the emitted
C for this shape copies nothing, allocates nothing, and contains no
trampoline (verified by reading the emitted region; the `mimic`
equality pins that the read was faithful).

## Finding 3 — the child process is quiet here (R22 secondary)

R22 reported a 16–30% interquartile range inside the ship-tier child
against 1–4% in the parent. Here every child-process variant measures
IQR 0.0–1.7% with piped stdio, and the inherited-stdio control shows
the same. Fact, from our code: the runner starts children with
`std::process::Command`, sets no QoS class, no priority, and no
affinity; the child's stdio is three pipes. The runner's spawn does
not produce the reported variance.

## Answer to R22 request 2

The boundary can shrink — the trap check (~1.1 ns) and the accessor
pair (~0.4–0.5 ns each) are removable in principle — but there is no
27–36 ns to remove. The call itself (~2.4 ns round trip into a
separate translation unit) is the floor, and our boundary adds
~1 ns to it. A shrink slice is not scheduled: the measured cost does
not justify one, and the contract forbids one in this slice.

## Discriminators handed to the downstream

Recorded in the R22 response: check the emitted region C for
`subscript_rt_boundary_scratch_mark` (a struct parameter with
string-view or nested-pointer members adds a per-call scratch pair);
count the runtime calls at one `setBindGroup` site; confirm the
backend C and the emitted C both compile `-O2`; run this subject on
the R22 machine.
