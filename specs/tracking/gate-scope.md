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

Cost is unmeasured. Measure the `perf-gate` wall time on a quiet
machine before the owner decides item 2. §9 requires a quiet machine,
and a gate that runs beside a compile measures the compile.

A cheaper form of item 2 exists: keep the gate hand-run, and add its
run to the phase-end checklist. That converts an unmeasured criterion
into a remembered one, which is what failed here.
