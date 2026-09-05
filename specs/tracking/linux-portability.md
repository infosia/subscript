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

### One site of defect 3's class stays

`codegen/tests/cemit.rs:2543` holds the fifth copy of the list. It is
inside `#[cfg(all(windows, target_env = "msvc"))]`, so this host cannot
build it or test it. An unverified edit is worse than the record. The
Windows host must make that site read the canonical list.

## Follow-ups (tracked, beyond this phase)

- SysV argument **register-pressure stack revert** (psABI §3.2.3 step 5) —
  replace the loud error with the demote-to-`SysVMemory` path; needs a
  high-arity by-value corpus entry to verify byte-exact.
- SysV **`f16` SSE classification** across both tiers (JIT + `cemit`), with
  a corpus entry — replaces the loud error.
- AAPCS64 has the same two unmodeled cases (register pressure; `f16` HFA);
  fold into the fixes above.
