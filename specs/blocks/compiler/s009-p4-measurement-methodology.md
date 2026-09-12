<!-- §9 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 9. P4 measurement methodology

The thresholds are pre-registered in §3 and do not move. This section
pins *how* the numbers are produced, before any number exists.

- **The baseline is verified, not asserted.** The hand-written C
  program must print the same bytes as `corpus/accept/
  a22-matrix-propagation.expected`. A baseline that does not reproduce
  the frozen golden is not the same computation and the measurement is
  void. Same N, same iteration count, same LCG seed and sequence, same
  f32 arithmetic — the C source declares the correspondence in a
  comment naming the corpus entry.
- **What is timed**: the execution of the workload only — the loop the
  entry performs, measured inside the process, excluding process
  start-up, compilation, linking, JIT warm-up, and I/O. All three
  subjects (C, ship-AOT, dev-JIT) time the same span by the same
  clock class (monotonic).
- **Procedure**: warm up for **at least 200 ms of measured execution
  and at least 3 iterations**, discard it, then at least 11 timed runs;
  the reported figure is the **median**. Report the median, the min/max
  spread and **every sample** for each subject; a spread wider than
  ±20% of the median invalidates that subject's timing, which is
  withheld. **A subject must report its measured warm-up time and is
  rejected below the floor.**

  *(Defect recorded 2026-08-27: the harness's default does not meet
  this rule.* `DEFAULT_WARMUP` is 3, and `a22` runs about 4 ms, so a
  default run warms up for about 12 ms against a 200 ms floor. Both
  this session and the round that deleted the Cranelift AOT tier had
  their first run declared void by the harness's own noise check, and
  both had to pass `--warmup` by hand. **A gate whose default is void
  is a gate nobody can run correctly by accident.** The floor is a
  time, so the default must reach it by measuring, not by a count.)*

  *(Revised 2026-07-27; this said "at least 3 warm-up runs discarded"
  and min/max-only reporting. `benchmarks.md` Rev 3 has the evidence:
  `clang -O2` was deleting the warm-up loop outright in three of ten C
  workloads, so a count-based rule was satisfied while zero warm-up
  ran, and min/max alone could not distinguish a cold first sample from
  scattered noise. A count also cannot express "reach steady state"
  across per-iteration costs spanning 3.7 ms to 125 ms, and this
  machine's DVFS ramp is ~70 ms.)*
- **One session, one machine**: all three subjects are measured in the
  same session on the same machine, with the machine's state described
  in the tracking entry (host, CPU, whether on AC power). Numbers from
  different sessions are never compared.
- **Compile-time is gated, and dev-tier execution has a ceiling**:
  *(amended 2026-08-28.)* This read "compile-time is reported, not
  gated ... §3's 4× criterion is about execution". §3's revision of
  2026-08-27 deleted the 4× criterion and made iteration time the
  dev tier's criterion. Iteration time is now gated at 20 ms, and
  dev-tier execution has a 25× ceiling.
- **Both outcomes are recorded.** If a threshold fails, the tracking
  entry records the measurement, the failure, and the named criterion
  reopening the backend decision (§3) — the gate is not retried with a
  different methodology.
- **This section governs a reported measurement, not a gate run.**
  *(Added 2026-08-28.)* The quiet machine and the ±20% spread rule
  exist because a published comparison claims a 5% difference. A gate
  run under `cargo test` shares a machine with compiles and cannot
  meet them. It does not have to: every §3 limit keeps at least 11%
  headroom over its measurement, and run-to-run variation is about 3%.
  A gate reports a regression. A number this project publishes comes
  from a §9 run, and a gate result is never published as one.
