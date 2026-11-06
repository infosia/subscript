<!-- §222 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 222. The `collect` limits follow the measurement

*(Added 2026-11-01.)* Origin: §213.4 item 1. The owner selected this
section on 2026-11-01. The measurement is in
`specs/tracking/collect-perf-limit.md`.

Problem, measured at `1f55d944` on an 8-CPU aarch64 host: `collect` reads
1.12x to 1.15x ship and 3.64x to 3.73x dev on a quiet machine. The limits
are 7.50x and 8.50x. The gate does not report a ship slowdown under
about 6.5x. §3 derives the limits from 6.45x and 7.04x at `1bb670d`.

### 222.1 Rules

1. On aarch64, the `collect` ship-tier limit is 1.50x.
2. On x86-64, the `collect` ship-tier limit stays 7.50x. This host has
   no x86-64 measurement.
3. The `collect` dev-JIT limit is 6.00x on each ISA.
4. The figure stays the figure of §213.1.

### 222.2 Acceptance

1. `perf-gate --gate` meets each `collect` limit on a quiet machine.
2. The tracking note records the figure at each load.
3. §3 and §213.4 cite this section.
4. No golden moves.

### 222.3 Phase Review

*(Added 2026-11-01.)* A fresh review gave four MINOR findings and no
CRITICAL or MAJOR finding.

1. MINOR, realistic, fixed: the tracking note said that each figure at
   N ≥ 12 falls under the quiet figure. Ship at N = 16 reaches 1.29x.
2. MINOR, realistic, fixed: §8 stated the 7.5× ceiling in the present
   tense.
3. MINOR, realistic, open: no test reads `ship_scope`.
4. MINOR, contrived, open: rule 3 tightens the x86-64 dev limit with no
   measurement. Item 1 of §222.4 records it.

### 222.4 Open

1. The x86-64 ship limit and the x86-64 dev limit have no measurement. A
   miss on x86-64 gives the first figure. §225.2 measures them with `cl`,
   and §226 sets the x86-64 ship limit and the Windows dev limits.
2. No test reads `ship_scope`, so a swap of the two reasons in
   `perf-gate.rs` passes.
