# Linux portability — evidence

Status: **complete 2026-08-09** (both host non-regressions
discharged). Contract: `specs/blocks/compiler.md`
§11a (crate C toolchain), §11b (runtime C toolchain), §12.3a (dev-tier
boundary-struct marshaling), §1 (dev tier hosts).

The Windows-MSVC port (`specs/tracking/windows-portability.md`) is the
precedent. The Linux port follows the same three root-cause shape and the
same two-task plan.

## Finding (2026-08-09)

`cargo build --workspace` on `x86_64-unknown-linux-gnu` fails in the
`subscript-interop-fixture` build script. The `cc` crate selects the Unix
default driver `cc`, which is GCC on this host. GCC cannot parse the
clang-only synthetic host facade `corpus/interop/interop.h`.

Host toolchain, measured:

| driver | version | `_Nullable` | `_Float16` on x86-64 |
|---|---|---|---|
| `cc` / `gcc` | 11.4.0 | fail | fail |
| `gcc-12` | 12 | fail | ok |
| `clang` (default) | 14.0.7 | ok | fail (x86 support needs clang ≥ 15) |
| `clang-15` | 15.0.7 | ok | ok |

`corpus/interop/interop.h` uses two clang-only constructs by design (it is
the libclang binder probe): `typedef _Float16 SubFloat16;` (`interop.h:48`)
and 19 `_Nullable` qualifiers. GCC rejects `_Nullable`; clang < 15 rejects
`_Float16` on x86-64. No single installed driver accepts both. `clang-15`
accepts both.

Measured: `clang-15` compiles all three fixture sources
(`corpus/interop/interop.c`, `external-device.c`, `wire-enum.c`) with 0
errors. `CC=clang-15 cargo build --workspace` is green.

## Root causes (measured, `CC=clang-15` plus a Linux system-library shim)

`cargo test -p subscript-codegen --no-fail-fast`:

| cause | symptom | tests |
|---|---|---|
| A. crate C toolchain is the Unix default (GCC) | fixture fails to compile; workspace build blocked | build blocker |
| B. runtime staticlib link misses Linux system libs | `undefined reference to exp/log/pow/sin/…` when clang links `libsubscript_runtime.a` | interop (link stage), golden, cemit, reload |
| C. §12.3a SysV struct-by-value not implemented | `foreign call passing/returning a boundary struct by value is only supported on aarch64 (AAPCS64) and x86-64 Windows (Win64) … target x86_64-unknown-linux-gnu is unsupported` (loud error, by design) | interop ×9, golden ×3, cemit ×1, reload ×1 |
| D. `node` binary absent (not a toolchain cause) | `api_reference`: `run node: No such file or directory` | api_reference ×1 |

Cause B masks cause C: the link fails before the by-value marshaler runs.
With B fixed (a `clang-15 … -lm -lpthread -ldl` shim), 4 interop tests
pass and the remaining 9 fail on cause C alone. Every non-interop failure
(golden ×3, cemit ×1, reload ×1) is the same cause C on the interop corpus
entries `a25-interop-chain`, `a26-interop-array-pair`,
`a27-interop-string-view`, `a126-interop-by-value-packing`,
`a131-interop-wire-enum-struct`.

Cause B minimal set: `-lm` alone clears all undefined references today.
The canonical set is `rustc --print native-static-libs` for the target:
`-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc`. §11b already matches
`rustc --print native-static-libs` on Windows; the Linux path must do the
same, not hardcode `-lm`.

Cause D is a test prerequisite, not a compilation cause. `api_reference`
needs a `node` binary to produce the divergence witness. It is unrelated
to the C toolchain and out of scope for the Linux compilation work.

## Decision (owner, 2026-08-09)

Full parity: build the fixture with a capable clang on Unix, and implement
§12.3a SysV dev-JIT boundary-struct-by-value marshaling. x86-64 Linux
becomes a verified dev-JIT host (§1). The clang resolution probes for a
clang that supports x86 `_Float16` (no system default change): `$CC`, then
`clang`, then `clang-NN` newest-first, first capable driver wins.

## Task plan (sequential — Task 2's gate depends on Task 1)

1. **Toolchain + link libs** (`codegen/build.rs`,
   `codegen/tests/native-fixture/build.rs`, `codegen/src/aot.rs`,
   `codegen/tests/offsetof_layout.rs`) — §11a/§11b: on Unix select a clang
   that compiles x86 `_Float16` (probe `$CC` → `clang` → `clang-NN`);
   add the Linux runtime system libraries from
   `rustc --print native-static-libs`. Expected: causes A + B clear;
   remainder is cause C only.
2. **SysV marshaling** (`codegen/src/lower/mod.rs`,
   `codegen/src/lower/func.rs`) — §12.3a: extend the ABI branch with the
   System V AMD64 eightbyte classification (INTEGER → GP register, SSE →
   XMM register; ≤ 16 bytes → up to two eightbytes; MEMORY class → stack
   for arguments, hidden pointer for returns). Expected: cause C clears
   (interop ×9, golden ×3, cemit ×1, reload ×1).

Out of scope: `api_reference` (cause D, needs `node`);
`benchmarks/src/bin/perf-gate.rs` (no test drives it).

## Verification method

- Task 1: `cargo build --workspace` green; `cargo test -p subscript-codegen`
  down to cause-C failures only.
- Task 2: `cargo test --workspace --no-fail-fast` green on
  `x86_64-unknown-linux-gnu` (except cause D). Every interop corpus entry
  agrees byte-for-byte across dev-JIT ≡ ship-C-AOT ≡ golden. No golden is
  edited.
- arm64 non-regression: re-run the full suite on the arm64 reference
  machine after Task 2 — the SysV branch must not change the AAPCS64 path.

## Status log

- 2026-08-09: finding recorded; three root causes classified from a
  measured `CC=clang-15` run plus a Linux system-library shim; two-task
  plan set.
- 2026-08-09: Task 1 (clang resolver + Linux system libs) implemented by
  the coding agent, reverted once for a workspace-wide `cargo fmt` that
  reformatted ~65 out-of-scope files (this repo was not stable-rustfmt
  canonical at the time), then re-done clean. *(Superseded 2026-08-09:
  the tree is now rustfmt-canonical under the `rust-toolchain.toml` pin
  and `cargo fmt --check` is a standing gate — CLAUDE.md, Code
  conventions. The rule against formatter runs from any other toolchain
  version stands.)* Orchestrator
  re-verified with `$CC` unset: `cargo build --workspace` green,
  offsetof 1/0, interop 4 pass / 9 §12.3a (no `undefined reference`),
  codegen lib 141/0. Causes A + B clear.
- 2026-08-09: Task 2 in progress. Measured correction to §12.3a: the corpus
  **does** exercise the SysV MEMORY argument path — `SubCallbackInfo`
  (24 bytes, `a25`–`a90`) and a `{i64,i64,i64}` triple (`a126`) — so that
  path is implemented (stack-by-value via Cranelift `StructArgument`, not
  `Indirect`), not staged. A struct return in SSE-class registers stays a
  loud error on SysV, the same as the AAPCS64/Win64 HFA-return limitation
  (shared follow-up); it keeps the `hfa_float_struct_return_fails_loud`
  test green on every ABI.
- 2026-08-09: Task 2 landed and the manual-link gap in
  `codegen/tests/cemit.rs` (`date_now_reads_the_pinned_context_clock…`)
  fixed to use `host_c_compiler()` plus `runtime_system_libraries`.
  Orchestrator re-verified with `$CC` unset: `cargo test --workspace
  --no-fail-fast` — every suite green except `api_reference` (cause D,
  `node` absent). Interop 13/13, golden 27/27 byte-exact, cemit 79/0,
  reload 11/11, codegen lib 142/0. Causes A + B + C clear.
- 2026-08-09: Phase Review (fresh no-context reviewer on Fable) — 0
  CRITICAL, 3 MAJOR, 8 MINOR. MAJOR 1 (SysV argument register-pressure
  revert unmodeled → silent mis-marshal) and MAJOR 2 (`f16` register-class
  eightbyte classifies INTEGER, psABI says SSE) each resolved to a **loud
  error** (fail-loud staging; the stack-revert and full-SSE-`f16` paths are
  follow-ups), with unit tests that fire the guards; no guard fires on any
  current corpus entry. MAJOR 3 (spec future-tense; missing Task 2 evidence
  and arm64 non-regression) resolved: §1/§12.3a/§11b brought to the
  post-landing state, evidence logged here. MINORs 4/5/6/8/9 fixed
  (incapable `$CC` loud, `OnceLock` compiler cache, windows-gnu LLVM
  fallback, `cemit.rs` uses `host_c_compiler()`, SysV-return unit test).
  Codegen lib 145/0 after the fixes; gate stays 13/13 and 27/27.

## Remaining gate before the phase is COMPLETE

- **arm64 (AAPCS64) non-regression** — **discharged 2026-08-09.**
  `cargo test --offline --workspace --release` on the arm64 reference
  machine (macOS, Apple M2 class), at `cfb583d`, exit 0: every suite
  green, every golden byte-exact, `tsc` gate exit 0. The shared
  marshaler refactor does not change the AAPCS64 path's output.
- **windows-msvc (Win64) non-regression** — **discharged 2026-08-09**
  (owner, recorded in commit `b3b670f`): on `x86_64-pc-windows-msvc`
  with the pinned toolchain, `cargo build --workspace --all-targets`
  0 warnings; `cargo test --workspace --no-fail-fast` 53 harnesses,
  904 passed, 0 failed, 1 ignored; `cargo fmt --check` exit 0; no
  golden byte changed. One unused-imports warning found there and
  fixed in the same commit (cfg-gated imports in
  `codegen/tests/cemit.rs`).

Both non-regressions are discharged; no gate remains open in this
phase.

## Ship target — x86_64 Linux (2026-08-09)

The dev-tier port above also makes `x86_64-unknown-linux-gnu` a **ship
target** (compiler.md §11 "Ship targets", §1). The ship tier is
HIR→C→clang, so the platform C compiler marshals the ABI and the dev-tier
SysV work is not needed here; the manual link needs the Task 1 Linux system
libraries (§11b).

Measured 2026-08-09 (`$CC` unset): `target/release/subscript emit
a01-hello.ts -o out` produced `program.c` + `entry.c`; native clang
(`-std=c11 -O2 -fwrapv -ffp-contract=off`) linked them with the host
`libsubscript_runtime.a` and `-lm -ldl -lpthread -lrt -lutil -lgcc_s -lc`
into an ELF 64-bit x86-64 PIE; running it printed the golden `hello`, exit
0. The standing gate already runs the same ship-C path byte-exact
(`run_c_aot`, golden 27/27), so x86-64 Linux is the one ship target that
executes in the gate — the mobile device triples only compile+link.

Tooling parity landed with this change: `x86_64-unknown-linux-gnu` is added
to the device-triple object emitter (`codegen/src/bin/emit-object.rs`, its
`aot.rs` shape test) and to `codegen/device-link.sh` (a native-clang
compile+link section), beside the arm64 iOS/Android triples.

## §55 stack probes — both profiles on this host (2026-08-10)

`3c3e7d7` turns on `enable_probestack` with the inline strategy in
both Cranelift flag sets (compiler.md §55). The rule is
target-independent, so the Linux gate re-ran at `cca69b0` in both
profiles — the §55 lesson is that a profile that only builds is
not a gate.

Measured on this host, toolchain 1.95.0, release and dev profiles:
926 passed, 0 failed, 1 ignored, 1 filtered, exit 0 in each.
`cargo fmt --check` exit 0. Golden ledger unchanged. The filtered
test is the `api_reference` witness; this host resolves no JS
engine (`s54-link-input-order.md`), and macOS ran it green at
`a94eeb0`.

## The JS toolchain gates run on this host (2026-08-10)

Node v24.19.0 (the witness baseline is the v24 line,
`js-api-sweep.md`) and the pinned typescript 5.9.2 are installed.

Measured at `346f2bd`, release profile:

- `api_reference` witness suite: 1 passed, 0 failed, exit 0.
- `npx tsc -p tsconfig.json`: exit 0.
- Full workspace gate, no test filtered: 927 passed, 0 failed, 1
  ignored, 0 filtered, exit 0.

Every gate now runs on this host. No count is borrowed from the
reference machine.

## Three x86-64 defects the pull of 2026-09-05 exposed

This host last ran the gate at `346f2bd` (2026-08-10), green. The next
run was at `90aa5cb`, about 250 commits later. Measured Red at `90aa5cb`,
both profiles: 66 suites, 1,252 passed, 2 failed, 1 ignored in debug, and
1,249 passed, 3 failed, 1 ignored in release.

Each defect is x86-64-only, and each shows on a SysV host alone. The
arm64 reference machine and the Windows host do not reach any of them.

1. **A SysV MEMORY-class argument was not rounded to whole eightbytes.**
   `AggregateArgPlan::Memory` passed the raw struct size to Cranelift
   `StructArgument`, whose x64 backend asserts a multiple of 8.
   `EngineTransform` measures size 20 and align 4, so
   `e09-c-structs-and-slices` and `gate/two-header-binding` panicked in
   the dev-JIT thread. Cause: `ec1d8be` (2026-08-28). Both corpus MEMORY
   shapes are 24 bytes, so the corpus never reached a rounded size. Fix:
   the plan carries the rounded stack size, and the emission zero-fills
   the slot before it copies. Contract: `compiler.md` §12.3a.
2. **The benchmark timing entry lost the POSIX clock declarations.**
   `perf-gate` compiles the entry at `-std=c11`, and glibc hides
   `clock_gettime` and `CLOCK_MONOTONIC` in a strict ISO dialect.
   Measured on glibc 2.35: `-std=c11` fails, `-std=gnu11` passes, and
   `-std=c11 -D_POSIX_C_SOURCE=199309L` passes. The macro must come from
   the command line. The entry unit is `subscript_runtime.h` concatenated
   with `aot-entry.c`, so glibc latches its feature set at the runtime
   header and a `#define` in `aot-entry.c` is too late. Contract:
   `compiler.md` §11.
3. **Two C link sites held a local system-library list.** Each harness
   had its own `runtime_system_libs`, Windows-only, so the Linux
   staticlib link resolved no `-lm` and the ship subject failed on
   `tan`, `acos`, and `log`. This is cause B above, third and fourth
   site. Both now read
   `subscript_codegen::runtime_system_libraries(CCompilerStyle::Unix)`.

Defects 2 and 3 were invisible until `bbced38` made `perf-gate` a test
target. The gate that reports a defect must run where the defect lives:
defect 1 waited 8 days for a SysV host, and defects 2 and 3 waited for a
hand-run binary to join `cargo test`.

### Gates after the fix, this host, both profiles

- debug: 66 suites, 1,255 passed, 0 failed, 1 ignored, 1,603 s.
- release: 66 suites, 1,253 passed, 0 failed, 1 ignored, 393 s.
- Zero-warning build in both profiles; `cargo fmt --check`, `tsc`, and
  `tools/hygiene.sh` exit 0; clippy 7 / 18 / 13, the recorded baseline.
- No corpus entry, golden, `.expected`, or `benchmarks/results.json`
  moved.

### One site of defect 3's class stayed

`codegen/tests/cemit.rs:2543` held the fifth copy of the list. It is
inside `#[cfg(all(windows, target_env = "msvc"))]`, so this host cannot
build it or test it. An unverified edit is worse than the record. The
Windows host must make that site read the canonical list.

**Closed 2026-09-05 on the `x86_64-pc-windows-msvc` host.** The MSVC arm
now reads `runtime_system_libraries(CCompilerStyle::Msvc)`. That call
returns the same five arguments in the same order, so the link line does
not change. No corpus entry, golden, or `.expected` moved.

The class is now unreachable. Every C link site in this repository reads
the canonical list. Two spellings of the five names stay, and neither is
a link site: `WINDOWS_SYSTEM_LIBRARIES` (`codegen/src/ship.rs:538`) is
the list itself, and the unit test at `codegen/src/ship.rs:1158` derives
its expected arguments separately from the function it checks (core
principle 9).

Gates on that host, both profiles, after the fix:

- debug: 66 suites, 1,232 passed, 0 failed, 1 ignored, 575 s.
- release: 66 suites, 1,230 passed, 0 failed, 1 ignored, 136 s.
- Zero-warning build in both profiles; `cargo fmt --check` and
  `tools/hygiene.sh` exit 0; clippy 7 / 18 / 13, the recorded baseline.
- `generated-docs/` regenerates with no diff.

`cargo test -p subscript-codegen --test cemit` links the emitted C with
MSVC `cl`, so the run proves the canonical list links on this host.

## Follow-ups (tracked, beyond this phase)

- SysV argument **register-pressure stack revert** (psABI §3.2.3 step 5) —
  replace the loud error with the demote-to-`SysVMemory` path; needs a
  high-arity by-value corpus entry to verify byte-exact.
- SysV **`f16` SSE classification** across both tiers (JIT + `cemit`), with
  a corpus entry — replaces the loud error.
- AAPCS64 has the same two unmodeled cases (register pressure; `f16` HFA);
  fold into the fixes above.

## Three x86-64 Linux defects the 2026-09-18 run exposed

This host last ran the gate at `5ad77fb` (2026-09-05), green. The next
run is this one, at `0a04232`, 588 commits later. The Windows host and
the arm64 reference machine report none of the three defects.

Host: Intel Core i5-4308U, 4 threads, 15 GiB; Ubuntu 22.04, glibc 2.35;
clang 14.0.0, gcc 11.4.0; rustc and cargo 1.95.0; node v24.20.0;
typescript 5.9.2.

`tools/gate.sh quick` at `0a04232`, record
`target/gate/20260917T232616Z-quick.md`:

| Step | Result |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo build --offline --locked --workspace --all-targets` | 0 warnings |
| `cargo test --offline --locked --workspace --no-fail-fast` | 1,601 passed, 3 failed, 2 ignored |

Three steps ran outside the quick shape, each green: clippy 7 / 18 / 13,
the recorded baseline; `node_modules/.bin/tsc -p tsconfig.json` exit 0;
`tools/hygiene.sh` exit 0. The workspace compiles with no warning. Each
of the three failures is a run-time failure of a test.

### Defect 1 — the ship-tier interrupt harness loses the POSIX declarations

`codegen/tests/sandbox_interrupt.rs:147`,
`the_ship_tier_returns_the_interrupt_trap`. The ship build of the
interrupt program fails to compile its C:

    /tmp/.../entry.c:513:19: error: use of undeclared identifier 'CLOCK_MONOTONIC'
    /tmp/.../entry.c:528:13: error: use of undeclared identifier 'useconds_t'
    3 warnings and 3 errors generated.

The harness is `interrupt_thread_c` (`codegen/src/ship.rs:1229`). Its
non-Windows arm calls `clock_gettime`, reads `CLOCK_MONOTONIC`, and
calls `usleep` with a `useconds_t` cast. The ship tier compiles with
`-std=c11` (§11 dialect pin), and glibc hides every POSIX name in a
strict ISO dialect. Darwin shows them without a macro, so the arm64
reference machine never met it, and the Windows arm uses
`QueryPerformanceCounter` and `Sleep`.

Measured on this host, clang 14.0.0 and glibc 2.35, on a probe that
holds the same four names:

| Command line | Result |
|---|---|
| `-std=c11` | `CLOCK_MONOTONIC` and `useconds_t` undeclared |
| `-std=gnu11` | clean |
| `-std=c11 -D_POSIX_C_SOURCE=200809L` | `usleep` still implicit |
| `-std=c11 -D_DEFAULT_SOURCE` | clean |
| `nanosleep` for `usleep`, `-std=c11 -D_POSIX_C_SOURCE=199309L` | clean |

`_POSIX_C_SOURCE=199309L` is the macro the four `benchmarks/` sites
already pass. It covers `clock_gettime`, `CLOCK_MONOTONIC`, and
`nanosleep`. It does not cover `usleep`, which glibc gives to
`_DEFAULT_SOURCE` and to XOPEN 500 only.

The macro must come from the command line. The harness is inserted at
`INTERRUPT_THREAD_ANCHOR` inside the host entry, after
`runtime/include/subscript_runtime.h`, and glibc latches its feature set
at the first libc header of the unit. §11b records that rule already.

**This is the fifth site of one class.** §11b named the class on
2026-09-05 and fixed four sites in `benchmarks/`. The fix at each site
is a copy of one flag. A fifth site appeared in `codegen/src/ship.rs`
eleven days later, with a fifth name (`usleep`) that the copied flag
does not even cover. CLAUDE.md: a fix that closes named sites does not
converge; make the class unreachable, or make a total check report every
remaining site at once.

### Defect 2 — the process-group test's own control passes the 300 s budget

`cli/tests/commands.rs:1709`,
`a_budget_kill_reaches_the_c_compiler_the_child_started`. The firing
control is a `--profile sandbox` build with no budget variable set, so
the compile child gets the contract's 300 s default (§109.2 rule 6). The
control does not reach its end here: it stops with
`error[S026]: the compiler passed its time budget`, and `assert_code`
reads exit 1 where the control requires 0. The test spends 300.02 s to
report it.

The control's cost is the C compile of `slow_host_c`, whose 65,536
unrolled steps are a constant in the test. Measured on this host, the
same body alone, outside the test:

    $ clang -std=c11 -O2 -fwrapv -ffp-contract=off -c slow.c -o slow.o
    686.39 s

§109.6a records the time-budget test's control as "67.6 s unoptimized".
That is one host's number. The test's doc comment states that the budget
is derived and not a constant, and the budget is; the fixture that sets
the control's cost is not.

### Defect 3 — a dev-JIT relocation passes the 2 GiB PC-relative range

`codegen/tests/boundary_scratch_breadth.rs:395`,
`lowered_positions_are_disjoint_and_sibling_content_independent_from_one_through_n`.
The first of the test's two JIT runs panics inside the JIT:

    panicked at cranelift-jit-0.125.4/src/compiled_blob.rs:61:80:
    called `Result::unwrap()` on an `Err` value: TryFromIntError(())

Line 61 is the `Reloc::X86PCRel4 | Reloc::X86CallPCRel4` arm of
`CompiledBlob::perform_relocations`. It narrows the target-minus-site
displacement to `i32`. The narrowing fails, so the two addresses are
more than 2 GiB apart. `JITBuilder::with_isa` is not PIC, and a
non-PIC x86-64 direct call or `lea` carries a 32-bit displacement only.
The arm64 reference machine and the Windows host do not reach it.

Measured on this host, debug profile:

| Condition | Result |
|---|---|
| `cargo test -p subscript-codegen --test boundary_scratch_breadth`, 5 runs | 4 failed, 1 passed |
| the same binary under `setarch -R`, 3 runs | 3 failed |

ASLR is therefore not the variable; it only moves a distance that is
already outside the range.

**It is a regression.** Measured in a worktree, the same host, the same
toolchain:

| Pin | Result |
|---|---|
| `5ad77fb` (2026-09-05, this host's last green run) | 1 passed, 1,184.59 s |
| `0a04232` (2026-09-18) | 1 failed, 34.81 s |

The two pins bracket 588 commits. The fall in wall time is §86's C
emission (`s86-c-emission.md` records 668 s to 34.5 s), not a variable
of this defect.

#### The cause of defect 3

`git bisect run`, 7 steps, over `5ac6b01..0a04232`. Each step ran the
test three times, because the failure is not deterministic. The first
bad commit is `1a621da` (2026-09-17), "the per-build compile-thread
stack and the 131,072-token limit (M9)".

That commit raised `COMPILE_THREAD_STACK_BYTES`
(`compiler/src/lib.rs:239`) from 268,435,456 to 4,294,967,296 in an
unoptimized build. §109.2a raised it again to 8,589,934,592 on the same
day. The dev JIT lowers on that thread:
`on_the_compile_thread(|| lower_module_with(&mut module, …))`
(`codegen/src/jit/compile.rs:46`). The JIT allocates its code and its
data while the thread holds a stack reservation larger than the 2 GiB
a non-PIC PC-relative displacement reaches, so a pair of allocations on
the two sides of that reservation is out of range.

The control, measured at `0a04232` in a worktree, one variable, the
same host and toolchain:

| `COMPILE_THREAD_STACK_BYTES`, unoptimized | Runs | Result |
|---|---|---|
| 8,589,934,592, as committed | 5 | 4 failed, 1 passed |
| 268,435,456, the value before `1a621da` | 3 | 3 passed |

The stack size is therefore the variable. The defect is not in the
stack size: §109.2a derives it from the parser's measured cost per
nesting level, and the sandbox profile's bounds rest on it. The defect
is that the dev JIT's own requirement — every JIT allocation within
2 GiB of every other, which `JITBuilder::with_isa` without PIC assumes
— is a fact that no form carries. §109.2 rule 3 puts the check and the
lowering on one thread. The stack that the check needs and the address
range that the JIT needs are in conflict, and nothing states it.

The optimized build reserves 2,147,483,648 bytes, at the range's own
boundary, and the test passes there: 5 runs of
`cargo test --offline --locked --release -p subscript-codegen --test
boundary_scratch_breadth` at `0a04232`, 5 passed, about 5.07 s each.
The release profile of the gate therefore reports nothing. A reservation
at the boundary is not a margin; the optimized build passes because
2 GiB is the largest reservation the range still holds.

### Defects 1 and 2 fixed (2026-09-18)

The coding agent ran two rounds. The orchestrator measured every gate.

**Defect 1.** `interrupt_thread_c` calls `nanosleep` in place of
`usleep`, so one feature level covers every POSIX name the emitted C
holds. `posix_feature_arguments` is the one definition of the macro
(`codegen/src/ship.rs`), and the non-MSVC arm of
`add_c11_optimized_flags` adds it. The four `benchmarks/` sites read
the function and hold no copy. The class is closed for every caller of
the pinned C11 flags: the first round left the flag opt-in per site,
and `codegen/tests/host_entry.rs` and
`codegen/tests/async_cleared_trap.rs` did not pass it. A unit test pins
the Unix arm and the MSVC arm apart, with the arguments written by
hand. Contract: §11b, the two paragraphs dated 2026-09-18.

**Defect 2.** The firing control sets the budget variable to the
ceiling, and the whole test is heavy. Owner decision 2026-09-18: the
test stays heavy; the 65,536-step fixture is not made smaller. The
alternative measured and refused was a smaller fixture that keeps the
test in the quick shape. Contract: §109.6a, the rule and the amendment
dated 2026-09-18.

Measured on this host after both rounds:

| Gate | Result |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo build --offline --locked --workspace --all-targets` | 0 warnings |
| `cargo clippy --offline --locked --workspace --all-targets` | 7 / 18 / 13 |
| `cargo test -p subscript-codegen --test sandbox_interrupt` | 4 passed |
| `cargo test -p subscript-codegen --test host_entry` | 5 passed |
| `cargo test -p subscript-codegen --test async_cleared_trap` | 4 passed |
| `cargo test -p subscript-codegen --lib` | 219 passed |
| `cargo test -p subscript-cli --test commands` | 32 passed, 3 gate-skip lines |
| the same, `SUBSCRIPT_HEAVY_TESTS=1`, the process-group test alone | 1 passed, 1,581.86 s |
| `tools/hygiene.sh` | exit 0 |

The heavy run's own figures: the control 632.92 s, the killed build
316.01 s on its derived 316-second budget, and no executable after
632.92 s more.

Defect 3 is open. It is a measurement round, and it lands nothing.

### Defect 3, the measurement round (2026-09-18)

The round built prototypes, recorded the numbers, and reverted every
change. The tree stayed at `38ef1f9`.

#### A. The two items are the JIT's own code and the JIT's own data

Method: a patched copy of `cranelift-jit` 0.125.4 in a scratch
directory prints the relocation the library cannot apply, and the
fixture prints the address of the `observe` function it registers.
Three runs, two failed.

| End | Region | Item |
|---|---|---|
| site | the JIT's generated code | `subscript_export_main`, blob size 1,562,668 |
| target | the JIT's generated data | `subscript_str0`, size 8 |

The kind is `X86CallPCRel4`. The displacement is −8,567,550,719 in one
run and −8,573,338,367 in the other. Each is just under 8,589,934,592,
the unoptimized `COMPILE_THREAD_STACK_BYTES`. The module holds 4
functions and 1,616 data objects. The registered `observe` function is
in the test executable's image and is neither end.

The JIT's code mapping and its data mapping fall on the two sides of
the compile thread's stack reservation. No host symbol takes part.

#### B. `is_pic` is refused by the dependency

`JITModule::new` asserts `!builder.isa.flags().is_pic()`
(`cranelift-jit-0.125.4/src/backend.rs:355`, "cranelift-jit needs
is_pic=false"). With the flag set, every dev-JIT construction panics
before it generates code.

| Profile | Baseline | `is_pic=true` |
|---|---|---|
| debug | 504 passed, 0 failed, 1 ignored | 235 passed, 270 failed, 1 ignored |
| release | 504 passed, 0 failed, 1 ignored | 234 passed, 270 failed, 1 ignored |

The indirection has no measured cost, because no module is built. The
candidate is closed at this version of the dependency.

#### C. A thread sized from the lowering is still over the range

Method: the same shape as §109.2a. A probe records the stack pointer at
the entry of `lower_module_with` and the minimum inside
`lir::expr::lower_expr` and `lir::address_taken::expr`. Levels 64, 512,
1,024, and 2,048; the slope is constant over every step.

| Construct | Source bytes per level | Debug bytes per level | Release bytes per level |
|---|---|---|---|
| `!` logical not | 1 | 32,848 | 2,144 |
| `~` bitwise complement | 1 | 32,848 | 2,144 |
| `-` negation | 2 | 32,848 | 2,144 |
| `+0` left-nested addition | 2 | 32,848 | 2,144 |
| `(` parenthesis | 1 | 0 | 0 |

The parenthesis costs the lowering nothing, because the HIR carries no
parenthesis node. The parenthesis is the parser's worst construct
(31,151 unoptimized). The two stages therefore have different worst
constructs, and the lowering's own worst is `!` at 1 source byte.

The derived requirement, the byte limit times the cost times the
contract margin of 1.5:

| Build | Worst file | With margin 1.5 | Present stack |
|---|---|---|---|
| debug | 131,072 × 32,848 = 4,305,453,056 | 6,458,179,584 | 8,589,934,592 |
| release | 131,072 × 2,144 = 281,018,368 | 421,527,552 | 2,147,483,648 |

Unoptimized, the lowering costs 1.05x the parser per level. A thread
sized from the lowering is 6,458,179,584 bytes, still over the 2 GiB a
non-PIC displacement reaches. Optimized it is 421,527,552, under it.

The prototype builds the JIT module on a thread of those two sizes and
leaves the check on the compile thread. Measured:

| Measurement | Result |
|---|---|
| `boundary_scratch_breadth`, 3 runs, debug | 3 passed, 56.88–58.91 s each |
| `-p subscript-codegen --no-fail-fast`, debug | 505 passed, 0 failed, 1 ignored |
| the same, release | 504 passed, 0 failed, 1 ignored |

The three passes are on a 6,458,179,584-byte reservation, which is over
the window. They measure this host's mapping placement, not range. The
candidate does not close the defect in an unoptimized build.

#### The conclusion of the round

Neither candidate resolves the conflict. `is_pic` is refused by the
dependency, and a thread sized from the lowering's own cost is still
1.05x the parser's in the build the gate runs. Any reservation over
2 GiB that lives between two of the JIT's allocations reproduces the
defect, and the unoptimized requirement of either stage is over 2 GiB.

The form must change, and the round names four ways. Each needs its own
measurement.

1. **Fork `cranelift-jit` and pin the fork.** CLAUDE.md states forking
   as this project's way to change a dependency. Two shapes: remove the
   `is_pic` refusal, or carve the code and the data from one
   reservation, which bounds the displacement by the module's own size.
   The second is the smaller change and makes the class unreachable.
2. **Make the recursive stages iterative.** No thread then needs a
   stack over 2 GiB. It touches the parser, which §109.2a sizes, and
   the lowering.
3. **Lower `SOURCE_BYTE_LIMIT`.** 131,072 × 31,151 × 1.5 is over 6 GB;
   the limit must fall to about 42,800 bytes to bring the parser's
   reservation under 2 GiB. That is a language-facing change.
4. **Size the compile thread by profile.** §109.2a's derivation is a
   requirement of the sandbox profile. A default-profile compile takes
   a stack under the window. It does not close the defect for a
   sandbox-profile dev-JIT module, so it is a narrowing, not a fix.

### Defect 3, the arena round (2026-09-18)

Owner decision 2026-09-18: take way 1 of the previous round. The round
then found that no fork is needed. `cranelift-jit` 0.125.4 re-exports
`ArenaMemoryProvider`, `JITMemoryProvider`, and `SystemMemoryProvider`
at its crate root, and `JITBuilder::memory_provider` installs one. The
arena reserves one contiguous region up front and carves its code,
read-write, and read-only segments from it, so every item of one module
is within the arena of every other item.

The prototype installs the arena at `codegen/src/jit/compile.rs:41` and
`codegen/src/reload.rs:606`. It reverted; the tree stayed at `4ec2593`.

#### The defect closes

| Tree | Runs of `boundary_scratch_breadth`, debug | Result |
|---|---|---|
| `4ec2593` | 6 | 0 passed, 6 failed |
| with the arena | 5 | 5 passed, 0 failed |

The displacement of the pair that failed:

| Condition | Displacement |
|---|---|
| no arena | −8,567,550,719 and −8,573,338,367 |
| the arena, 10 modules over 5 runs | −20,480, every run |

Address randomization moves the arena base and nothing else. The
read-only data sits at the arena base and the code starts five pages
later. The displacement is bounded by the module's own span, not by the
address space.

| Suite | Baseline `4ec2593` | With the arena |
|---|---|---|
| `-p subscript-codegen`, debug | 504 passed, 1 failed, 1 ignored | 505 passed, 0 failed, 1 ignored |
| the same, release | 504 passed, 0 failed, 1 ignored | 504 passed, 0 failed, 1 ignored |
| `--workspace`, debug | 1,605 passed, 1 failed, 2 ignored | 1,606 passed, 0 failed, 2 ignored |
| `-p subscript-codegen --test reload` | 20 passed | 20 passed |

No committed golden, `.expected` file, corpus output, or
`benchmarks/results.json` moved.

#### The memory one module takes

584 dev-JIT modules over four subjects, debug. `span` is the distance
from the lowest to the highest byte one module allocates.

| Subject | Modules | Largest code blob | Read-only total | Span |
|---|---|---|---|---|
| `boundary_scratch_breadth` | 2 | 1,562,668 | 17,308 | 1,583,180 |
| `long_string_constants` | 22 | 609 | 1,048,578 | 1,053,561 |
| `reload` | 273 | 111,840 | 1,844 | 136,342 |
| `golden` | 287 | 105,056 | 1,844 | 132,053 |

The measured maximum span is 1,583,180 bytes. The worst density is 3.21
arena bytes per source byte (`boundary_scratch_breadth`, 1,583,180 over
493,178).

#### The cost is under this host's noise

`/usr/bin/time` on the built debug binaries, run alone.

| Binary | Baseline | With the arena |
|---|---|---|
| `golden` | 169.12 s, 161.32 s | 188.23 s, 170.91 s, 155.23 s |
| `long_string_constants` | 78.49 s, 75.32 s | 87.58 s, 73.33 s |

One configuration's own spread is wider than the difference between
configurations, and the two `golden` ranges overlap. A control with the
arena cut to 4 MiB gave `golden` 159.39 s, inside the baseline range, so
the reservation itself costs nothing measurable. Peak resident memory of
`boundary_scratch_breadth` is 390,344 kB with the arena against
390,152–395,232 kB without, unchanged. The arena is `mmap` PROT_NONE, so
the reserve costs address space and not resident memory.

#### Two things the candidate breaks

**1. The arena panics where the present provider returns an error.**
`Segment::set_rw` calls `.expect(...)`
(`cranelift-jit-0.125.4/src/memory/arena.rs:44`), so an `mprotect`
failure aborts the process:

    thread 'subscript-compile' panicked at .../memory/arena.rs:44:18:
    unable to change memory protection for jit memory segment:
    SystemCall(Os { code: 12, kind: OutOfMemory, ... })

`SystemMemoryProvider` returns `Internal("… unable to make memory
readonly")` at the same point. Core principle 5 forbids the panic, and
the panic is in the dependency.

The threshold moves the other way. Each reload generation builds its own
`JITModule` and its own arena (`codegen/src/reload.rs:606`), and the
session holds every generation until `Drop`. Measured on one trivial
program reloaded in a loop, against `vm.max_map_count` 65,530:

| | The arena | `SystemMemoryProvider` |
|---|---|---|
| VMA maps per generation | 3 | 4 |
| Resident bytes per generation | ≈140 kB | ≈140 kB |
| First failing generation | 21,808 | 16,361 |

The arena reaches the limit later, not sooner, because it uses one fewer
mapping. Only the failure mode is worse.

**2. The reserve size has no derivation.** 1 GiB against a measured
worst of 1,583,180 bytes is a margin, not a derivation.
`SOURCE_BYTE_LIMIT` (131,072) and `PROGRAM_BYTE_LIMIT` (8,388,608) are
sandbox-profile limits, so under the default profile no language fact
bounds a module. Against the sandbox program limit the worst density
gives 8,388,608 × 3.21 × 1.5 = 40,390,905 bytes. Under the default
profile the reserve is a new cap where none exists today. Exhaustion,
measured with the arena set to 1 MiB, is not a diagnostic:

    internal lowering error: define LIR function 0: Allocation
    { message: "unable to alloc function", err: … "pre-allocated jit
    memory region exhausted" }

It reaches the caller as `RunError::Internal` and names no source
position.

Both are contract questions. The form must carry the JIT module's memory
bound, the way §109.2a carries the parser's stack bound.

#### Two corrections to the round's handoff

The handoff said `cranelift_jit::memory` is a public module. It is not;
`src/lib.rs` declares `mod memory;` and re-exports the items at the
crate root. The handoff also gave the debug baseline as 504 passed, 0
failed. The debug suite holds 506 tests, one more than release
(`lir_interpreter_debug_subset_traps_at_declared_sites`), so a green
debug run is 505 passed, 1 ignored.
