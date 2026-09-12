<!-- §3 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 3. Pre-registered criteria

- **P0.5 mobile link spike — kill criterion**: the spike has no
  dependency on the language (it emits a fixed program), so it runs
  immediately after P0, before any compiler investment. A minimal
  program (arithmetic + a runtime call + printed result) compiled
  through `cranelift-object` must produce a valid object and link cleanly
  with the platform linker for both device triples:
  `aarch64-linux-android` (NDK clang) and `aarch64-apple-ios` (Xcode
  clang). Compile+link is the whole criterion; simulators and emulators
  are not used, and run-level verification on mobile hardware is not
  required — execution semantics are carried by the host-side
  differential gate. Failure at the object or link level for either
  platform → **ship tier reverts to C emission**; dev tier stays
  Cranelift JIT. Environment note: the iOS half requires a Mac; the
  Android half runs on any host via NDK.
- **P4 performance gate**: the baseline is a hand-written C
  implementation of the a22 workload (same shape as corpus.md §4),
  compiled with the platform C compiler at `-O2`, measured on the same
  machine in the same session as the language runs and recorded in the
  tracking file when P4 opens. Criteria: ship tier within **1.5×** of
  the C baseline (eval median). Failing it reopens the backend decision
  with the measurement as the named criterion.

  *(Revised 2026-08-27, owner. This read "ship-AOT within 1.5× ...;
  dev-JIT within **4×** of the same baseline". Two things were wrong
  with it.)*

  **"ship-AOT" named a tier that no longer exists.** It meant the
  Cranelift AOT, which §11 superseded and which is now deleted. The
  ship tier is C emission and measures 1.35×, inside the 1.5×.

  **The dev-tier criterion measured the wrong thing.** Invariant 3
  states why the dev tier exists: it is "a **fast-iteration**
  development tier", and dropping it "forfeits the main
  **iteration-speed** argument for the language". §9 says the same in
  its own words — the JIT compile time "is the iteration-speed
  argument" — and then gated execution anyway. Measured on `a22`:

      dev tier   check + lower + finalize          5.0 ms
      ship tier  check + emit C, then compile+link 119.3 ms
                                                   24x faster to iterate

      dev tier   execution                        30.8x of C
      ship tier  execution                         1.35x of C

  The dev tier is 24× faster to reach a running program and 23× slower
  to run it. That is the trade the tier exists to make, and a 4×
  execution limit asked it not to make it. The limit was never met, and
  nothing re-measured it for long enough that this session found it by
  accident.

  **The dev tier's criterion is now iteration time.** Time from a
  changed source to a running program, on `a22`, must stay within
  **20 ms**, which is four times the 5.0 ms measured here. A hot reload
  of one function must stay within the same budget.

  **Dev-tier execution has a ceiling, not a target.** *(Owner,
  2026-08-27.)* It must stay within **25×** of the C
  baseline. That is a ceiling against regression, not a performance
  goal: the tier exists to iterate, and 25× is chosen from the measured
  19.6× that a constant-trip unroller reaches, with room above it.

  Making this a ceiling rather than dropping the measurement is
  deliberate. The old 4× was never met and nothing re-measured it for
  long enough that this session found it by accident; a number nobody
  can reach is not a gate. A ceiling above the measurement fails only
  when something gets worse, which is what a gate is for.

  *(No iteration-time gate existed before this. `codegen/tests/
  reload.rs` has nineteen correctness tests and measures no time. The
  property invariant 3 calls the main argument for the language was
  never measured.)*

  **The ship-tier ratio is scoped by host ISA.** *(Owner, 2026-08-28.)*
  The 1.5× held one number over two instruction sets, and the same
  emitted C does not cost the same on both. §10a records the cause,
  measured on x86-64/Windows on 2026-07-23: out-of-line growable-array
  access and copy-heavy value-class parameter passing are "both of which
  clang optimizes on arm64 but not on x86". Fix A (§10a, inline
  growable-array access) landed and moved `a22` from 17.2 ms to 14.0 ms.
  **Fix B (value-class parameters by const-pointer) was investigated and
  dropped as unsound**, and its sound restriction — leaf functions only —
  disqualifies the one value-class-parameter function in `a22`. So the
  x86-64 residual is a named and open codegen cost, not noise and not a
  port defect (`specs/tracking/windows-portability.md`).

  - **aarch64: 1.5×, unchanged.** Measured 1.34× on the reference
    machine (`specs/tracking/s70-held-async-handle.md`). This is the
    number that chose C emission over Cranelift AOT, and it does not
    move.
  - **x86-64: 2.5×.** Chosen from measurement the way §3's 25× dev-tier
    execution ceiling was chosen from a measured 19.6×: above the
    observation with room, so it fails when something gets worse. Six
    `a22` ship-tier runs on `x86_64-pc-windows-msvc` measured 2.08×,
    2.03×, 1.93×, 2.18×, 1.93×, and 1.92×.

  This is a **ceiling against regression, not a target**. Raising a
  pre-registered criterion to cover a known deficit would remove the
  only mechanism that keeps that deficit visible, which is
  `specs/tracking/gate-scope.md`'s finding with its sign reversed. The
  ceiling therefore carries the reason it is not 1.5×, and closing the
  gap is Fix B under a sound formulation — an interprocedural escape and
  alias condition, not the leaf-only restriction — which is a codegen
  change, not the backend change a 1.5× miss names.

  **The 2.5× is provisional on noise.** The run that set it reported the
  `a22` C baseline at 18.8% to 43.5% spread, over §9's ±20% limit. §9
  reports that and does not gate it, and it also means one run cannot
  pin a ceiling. A run on a quiet machine, per §9, replaces this number
  with a measured one.

- **P4 allocation gate**, and the gate becomes automatic. *(Owner,
  2026-08-28.)* Two changes. `specs/tracking/gate-scope.md`
  holds the evidence and the cost.

  **The gate measured `a22` alone, and `a22` has no path to the memory
  model.** `a22` builds three growable arrays of a value type and holds
  them to the end. It frees no object and collects nothing. §68 held a
  root-set defect for 31 days, and no gate reported it. Invariant 2 is
  the memory model; a gate that measures only arithmetic cannot see it.

  **The gate now measures `collect` as well.**

      source     benchmarks/workloads/subscript/collect.ts
      baseline   benchmarks/workloads/c/collect.c, -O2, same machine,
                 same session
      subjects   C, ship tier, dev tier
      agreement  every subject computes the same i32 checksum

  Criteria:

      ship tier   within 7.5x of the C baseline
      dev tier    within 8.5x, a ceiling, as a22's is

  Derivation: the two tiers measured 6.45× and 7.04× at `1bb670d`. The
  §68 regression measured 8.07× and 10.17×. 7.5× leaves 16% over the
  measurement, and it trips 7% under the known regression. 8.5× leaves
  21%, and it trips 16% under it.

  **The gate runs from a test target.** `cargo test --release` fails
  when a threshold is missed. In a debug build the gate does not run,
  and it states the reason: §3's subject is optimized code, and a debug
  runtime is not it.

  Measured cost: `perf-gate` takes 3.96 s with the binary built, and
  `collect` adds about 7 s. `cargo test --offline --release --workspace`
  takes about 235 s. The gate is under 5% of the suite it joins.

  **A gate does not need §9's quiet machine.** §9's precision exists for
  the published comparison, where 5% is the claim. A gate reports a
  regression. `perf-gate` read 1.35× and 19.64× on a quiet machine and
  1.38× and 20.17× on a loaded one, so run-to-run variation is about 3%.
  Every limit here keeps at least 11% headroom. Noise does not trip a
  limit, and the regression this gate exists to catch was 32%.

  **`perf-gate` serves two runs, and they are not the same run.**
  *(Added 2026-08-28, after the first round removed §9's void from
  both.)*

      default    a §9 reporting run. A subject whose spread exceeds
                 ±20% has its timing withheld and the run is void,
                 exit 2. Every number this project publishes comes
                 from this run.
      --gate     a gate run. §3's thresholds decide the exit status,
                 and spread is reported and never voids. The test
                 target passes this flag.

  The two must not merge. A gate that voids on a loaded machine fails
  for a reason that is not a regression. A reporting run that prints an
  invalid timing breaks §9, which says the timing is withheld.

  If the gate run proves unstable in practice, the answer is more
  headroom or a serialized run. It is not a weaker check. Record the
  measurement that shows the instability first.
