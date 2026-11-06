<!-- §225 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 225. A benchmark compiles with the ship C compiler

*(Added 2026-11-06.)* Origin: on 2026-11-06 the owner asked for the
x86-64 figure of §222.4 item 1 on `cl`. The owner selected this section
on the same day.

Problem, measured at `859b3a1a` on a 20-CPU x86-64 Windows host with
MSVC 19.44.35222 and clang 22.1.6 installed: the header of
`perf-gate --gate` states `C compiler: clang version 22.1.6`. §11c makes
`cl` the ship compiler on Windows, so the gate does not measure the C
that this project ships there. `perf-gate` and `cross-language` select
`$CC`, then `clang` on `PATH`, then the LLVM install (`host_cc` in
`benchmarks/src/bin/perf-gate.rs`, `resolve_clang` in
`benchmarks/src/bin/cross-language.rs`). Their comments state that this
matches the ship path. It matched §11b, and §11c changed the ship path
on 2026-07-28.

Consequences: a Windows figure of either harness is a clang figure. A
cost that only `cl` shows, such as the form of §210, is not visible to
the perf gate.

### 225.1 Rules

1. Each harness compiles the ship-tier C with the compiler and flags of
   the ship tier: `host_c_compiler` and `add_c11_optimized_flags` in
   `codegen/src/ship.rs`, and the link arguments of the ship tier.
2. Each harness compiles its C baseline with the same compiler as the
   ship tier, with the optimization flags of rule 1 for that compiler.
   A ratio then compares two outputs of one compiler.
3. On a Unix host, the compiler and the flags of each subject do not
   change. A test pins the Unix arguments.
4. The report header states the compiler, its version, and the flags of
   each subject.
5. The limits of §3 and §222 do not change in this section. If
   `perf-gate --gate` misses a limit on a quiet machine with `cl`, the
   landing stops and reports the figures.

### 225.2 Measurement for §222

After the landing, on this host, with `cl`: five `perf-gate --gate`
runs under each load of 0, 5, 10, 20, 30, and 40 busy processes. The
tracking note of §222 records the figures of `collect` and `a22`, the
host, and the compiler. A change to the x86-64 limits is a later owner
decision.

### 225.3 Acceptance

1. Red: at `859b3a1a` on this host, the header names clang.
2. After the landing, the header names `cl` on this host, and every
   subject agrees on the `a22` golden and the `collect` checksum.
3. A unit test pins the Unix arguments of rule 3, and a unit test
   finds the MSVC style for each subject on an MSVC host.
4. `tools/gate.sh full` is green.
5. The Phase Review has no open CRITICAL or MAJOR, and
   `tools/hygiene.sh` is clean.

### 225.4 Open

1. The snapshot in `benchmarks/README.windows-x86_64.md` is a clang
   snapshot. This section does not capture it again.
