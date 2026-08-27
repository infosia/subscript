# The performance criteria are not gates

Status: **finding, 2026-08-28. Open.** `specs/blocks/compiler.md` §3 is
the owner's to revise, so this file states the finding and a proposal.
It changes no criterion.

## Fact

§3 calls its performance criteria "pre-registered". Nothing runs them.
`perf-gate` is a hand-run binary. `cross-language` is a hand-run binary.

## Evidence

1. The repository has no CI configuration. `.github/workflows` does not
   exist.
2. `benchmarks/` has no `tests/` directory.
3. A grep for `perf-gate` across the tree finds its own source, its
   `Cargo.toml` description, and prose in `specs/`. No test target and
   no script invokes it.
4. `benchmarks/results.json` was regenerated on 2026-07-27 (`5f6a06c`)
   and next on 2026-08-28 (`1bb670d`). The one commit between them
   (`c39739b`, 2026-07-28) renamed the memory operations and measured
   nothing.

## Consequence, measured

§67 and §68 landed inside that 31-day window. §68 made every managed
LIR value a shadow-root slot for the whole activation, against §68.2
rule 8. The `collect` workload then measured 8.07× ship and 10.17× dev,
against 6.12× and 6.19× before. No gate reported it. The owner's
question about the benchmark tables found it, 31 days later.

`74a091c` fixed it. `1bb670d` records the measurement.

## Why `a22` could not find it

`perf-gate` measures `a22` alone. `a22` builds three growable arrays
once and holds them to the end. `Matrix4` carries `@CStruct`, so those
arrays hold packed values, not heap objects. `a22` frees no object and
collects nothing.

The regression cost scaled with the live managed-object count and with
collection. Three managed values cannot show a defect in root-set
management. The gate did not miss the regression by bad luck; the
workload has no path to it.

## Proposal

Two changes. Both are the owner's to accept.

1. **A workload with allocation traffic joins the gate.** `collect` is
   the workload that found the defect, and its subscript half already
   exists at `benchmarks/workloads/subscript/collect.ts`. A gate that
   measures only pure arithmetic cannot see the memory model, which is
   invariant 2's subject.
2. **The gate runs from a test target**, so an ordinary
   `cargo test --release` fails when a threshold is missed.

## Cost, measured 2026-08-28

`perf-gate` takes **3.96 s** wall, with the binary already built.
`cargo test --offline --release --workspace` takes about 235 s. The
gate is 1.7% of the suite it would join.

Adding `collect` costs about 7 s more, derived from the 1bb670d
medians and §9's floors: three subjects, each 200 ms of warm-up and
11 timed runs at 32 ms (C), 209 ms (ship), and 228 ms (dev).

**The whole proposal costs about 11 s.** Cost is not the obstacle.

## The real obstacle, and why it is smaller than it looks

§9 requires a quiet machine and voids a subject at ±20% spread. A gate
inside `cargo test` runs beside compiles, so it cannot meet §9.

That is the wrong requirement for this job. §9's precision exists for
the **published comparison**, where a 5% difference is the claim. A
gate has one job: report a regression. The regression it missed was
6.12× to 8.07×, which is 32%.

Two measurements of the same run support the point. `perf-gate` read
1.35× and 19.64× at 2026-08-27, and 1.38× and 20.17× today on a
loaded machine. Run-to-run variation is about 3%. §3's limits are
1.50× against 1.35× and 25× against 19.6×, so the headroom is 11% and
27%. Noise of 3% does not trip either, and a 32% regression trips both.

**A gate needs a threshold that noise cannot trip, not a quiet
machine.** Conflating the gate with the published comparison is what
made the gate look expensive.

## What is still open

The `collect` threshold does not exist. §3 pins no allocation-path
number, and picking one is the owner's. The measurement to pick it
from is `1bb670d`: ship 6.45× and dev 7.04×.

A cheaper form of item 2 also exists: keep the gate hand-run, and add
its run to the phase-end checklist. That converts an unmeasured
criterion into a remembered one, which is what failed here.
