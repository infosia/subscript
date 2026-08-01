# Windows portability — evidence

Status: in progress, 2026-07-23. Contract: `specs/blocks/compiler.md`
§11a; architecture §1 (dev tier: cranelift-jit, Windows/Mac).

## Finding (2026-07-23)

`cargo check --workspace --all-targets` on `x86_64-pc-windows-msvc`
(rustc 1.97.0) fails in the `subscript-codegen` build script:

```
running the C compiler ("cc") failed: program not found
  at codegen\build.rs:47
```

Everything checked before it succeeded (`subscript-runtime`,
`subscript-bindgen`, `subscript-runtime-stub`). The failure is isolated to
`codegen/build.rs`, which compiles the synthetic interop callee and links
it into every binary that links `subscript-codegen`; because it is the
crate build step, its failure blocks the whole workspace build.

Root cause: `build.rs` hardcodes the Unix GCC/Clang toolchain, which does
not exist under MSVC (the Rust host is `x86_64-pc-windows-msvc`):

| step | current (Unix-only) | MSVC equivalent |
|---|---|---|
| compiler | `cc` default (`build.rs:31`) | `cl.exe` (no `cc`) |
| archiver | `ar crs` (`build.rs:52-54`) | `lib.exe /OUT:` |
| flags | `-std=c11 -O2 -fPIC -c -o` (`build.rs:37-45`) | `/std:c11 /O2 /c /Fo:` (no `-fPIC`) |

Environment: Visual Studio 2022 Community is installed
(`cl.exe` present under `VC/Tools/MSVC/14.44.35207`) but not on the shell
`PATH` (needs a Developer prompt / vcvars). The `cc` crate performs MSVC
`vcvars` discovery; direct `cc`/`ar` invocation does not.

## Unix-only C-invocation sites (full inventory)

1. `codegen/build.rs` — synthetic callee compile + archive. **Build
   blocker.** (compile fix scope)
2. `codegen/src/aot.rs:407,479` — ship-C compile + link. Runtime path
   (AOT / standing gate). Follow-up.
3. `codegen/tests/offsetof_layout.rs:198` — layout probe. Test path.
   Follow-up.
4. `benchmarks/src/bin/perf-gate.rs:643` — bench harness. Bench path. Follow-up.

Sites 2–4 run outside a build script, where the `cc` crate's `Build`
(which reads the build-script env vars `TARGET`/`OPT_LEVEL`/`HOST`) does
not apply directly; porting them needs explicit target detection and is
materially larger than site 1.

## Decision (owner, 2026-07-23)

Option 1: make `codegen/build.rs` target-portable via the `cc` crate. `cc`
v1.3.0 is already in `Cargo.lock` and the local registry cache, so adding
it as a `[build-dependencies]` entry is offline-clean (no fetch). This
alone makes `cargo build`/`cargo check` succeed on Windows-MSVC.

## Scope boundary

- **This task (compile-green):** `codegen/build.rs` only. Verified by a
  clean `cargo check --workspace --all-targets` on `x86_64-pc-windows-msvc`.
- **Follow-up (test-green on x86-64/Windows):** port sites 2–4 above, and
  land the §12.3a Win64 dev-JIT struct-by-value marshaling. Until that
  lands, dev-JIT interop entries that pass a boundary struct **by value**
  raise the §12.3a loud codegen error on x86-64; scalar/pointer/(ptr,len)
  interop is target-neutral and unaffected.

## Test-green follow-up (2026-07-23)

With the workspace building, `cargo test --workspace --no-fail-fast` on
`x86_64-pc-windows-msvc` fails ~24 tests across three root causes:

| cause | symptom | tests |
|---|---|---|
| A. runtime staticlib name Unix-only | `libsubscript_runtime.a not found` (MSVC produces `subscript_runtime.lib`) | cemit ×11, golden ×2, aot unit ×3, interop scalar entries, all_patterns |
| B. §12.3a Win64 struct-by-value not implemented | `foreign call passing a boundary struct by value ... target x86_64-pc-windows-msvc is unsupported` (loud error, by design) | interop ×6 (buffer_view, string_label, handle_create_retain_release, chain_slot ×2, all_patterns_composed) |
| C. runtime C toolchain Unix `cc` | `cc: program not found`; GCC flags `-std=c11 -fwrapv -ffp-contract=off` | offsetof ×1; surfaces in aot.rs after A is fixed |

Cause A masks cause C for the run_c_aot tests (staticlib lookup
short-circuits before the compiler runs); both must land together.

Toolchain decision: on Windows the runtime C paths use **clang**, not MSVC
`cl` — §11 pins the ship tier to clang (`-fwrapv -ffp-contract=off`), and
the AOT run's purpose is byte-exact golden reproduction, which `cl` (no
`-fwrapv`) would not guarantee. clang is installed at
`%ProgramFiles%\LLVM\bin\clang.exe` on the gate machine. Contract: §11b.

### Task plan (sequential — Task 2's gate depends on Task 1)

1. **Toolchain + staticlib** (`codegen/src/aot.rs`,
   `codegen/tests/offsetof_layout.rs`) — §11b: locate clang portably
   (`$CC` → `clang` → Windows LLVM path); target-aware staticlib name;
   host `.exe` extension. Expected: causes A + C clear (~18 tests).
2. **Win64 marshaling** (`codegen/src/lower/func.rs`) — §12.3a: branch the
   marshaler on target ABI; implement Win64 (size ∈ {1,2,4,8} → one packed
   integer register, else by reference). Expected: cause B clears
   (interop ×6).

Out of scope for the test gate: `benchmarks/src/bin/perf-gate.rs` (no test drives it).

## Result (2026-07-23)

`cargo test --workspace --no-fail-fast` on `x86_64-pc-windows-msvc`:
**236 passed, 0 failed** (independently re-run by the orchestrator). Every
interop corpus entry agrees byte-for-byte across dev-JIT ≡ ship-C-AOT ≡
golden — the proof the Win64 marshaling is correct. No golden was edited.

Two implementation deviations surfaced during the tasks, both flagged and
kept minimal:

1. **Str/Array `(ptr,len)` are by-value 16-byte aggregates, not
   target-neutral.** Before this fix the dev JIT pushed them as two
   registers; correct on AAPCS64/SysV, an access violation on Win64 (where
   16 bytes go by reference). Routed both through the shared aggregate
   marshaler. §12.3a corrected accordingly.
2. **`-Wno-error=incompatible-pointer-types` added to the `run_c_aot`
   clang compile.** Emitted ship-C passes `Sub_0_SubChainHeader*` where the
   foreign header expects `SubChainHeader*` — layout-identical (invariant
   1), ABI-safe, but nominally distinct; clang 20+ made the mismatch a
   default error (host clang is 22.1.6). The flag restores the pre-20
   warning behavior.

## Follow-ups

- **Generator fix — DONE (2026-07-23).** The C emitter
  (`codegen/src/cemit.rs`) now casts a boundary struct pointer to the
  foreign header pointer type (`(SubChainHeader*)(&x)`) at both boundary
  sites (`marshal_foreign_c_arg`, `boundary_field_init`), via
  `boundary_ptr_cast`. The `-Wno-error=incompatible-pointer-types` flag
  (Deviation 2) was removed from `run_c_aot`; the emitted C compiles clean
  on any clang. Workspace stays 236/0, goldens byte-for-byte unchanged (the
  cast is compile-time). Closes CLAUDE.md principle 6 for this case.

- **Bench port — DONE (2026-07-23).** `benchmarks/src/bin/perf-gate.rs` uses the clang
  locator, `.exe` suffix, and Windows system libs; `benchmarks/a22-baseline.c`
  and `benchmarks/aot-entry.c` set binary-mode stdout and, on Windows, read the
  timed span from `QueryPerformanceCounter` (overflow-safe ns conversion)
  since the MSVC UCRT has no `clock_gettime`. Verified by running
  `cargo run -p subscript-benchmarks --bin perf-gate --release -- --warmup 5 --timed 15`: all
  four subjects (C, ship-AOT, dev-JIT, emitted-C) compile, run, and match
  the frozen golden; noise check passes. The §3 perf thresholds are missed
  as always for the Cranelift ship-AOT/dev-JIT tiers (the reason §11 Rev 8
  moved the ship tier to C emission); emitted-C measured **2.60x** of hand
  C on this Windows/clang-22 host vs the **1.05x** recorded on the reference
  setup (§11) — a machine/toolchain difference, not a port defect: C and
  emitted-C are timed by the same method, so the ratio is timing-independent.

- **Emitted-C x86 perf investigation (§10a) — A landed (2026-07-23).**
  The bench's emitted-C = 2.6x hand C on x86/Windows (vs 1.05x on the arm64
  ship target, §11) was root-caused by measurement, ruling out the Windows
  port, `-fwrapv` (≈0 cost), QPC timing (ratio is method-independent), and
  opt level (ratio ~2.5x flat at -O2 and -O3). It is structural: the emitted
  C's growable-array access was an opaque `subscript_rt_array_ptr` call and its
  value-class math is copy-heavy, both of which clang optimizes on arm64 but
  not on x86. These are the two well-understood AOT-codegen costs for a
  C-ABI value-type language: out-of-line array access, and passing large
  aggregates by value. Fix A (inline growable-array access, §10a) landed:
  emitted-C 17.2→14.0 ms at -O2 (measured), workspace 237/0, goldens
  byte-identical.
- **Fix B (value-class params by const-pointer) — investigated and DROPPED
  (2026-07-23).** A prototype passed large read-only value-class parameters
  of plain functions by `const T*` (measured A+B → ~12.1 ms / ~1.9x). An
  adversarial soundness review found it **unsound**: eligibility scanned only
  the callee's own body, so `f(m, arr)` reading `m` but calling `g(arr)`
  (which does `arr[0] = …`) elided `m` to a pointer aliasing `arr[0]` — the
  by-value tier returned the pre-call value, the by-pointer tier the mutated
  one (`1` vs `99`); the corpus lacked that shape, so the gate missed it. The
  sound restriction (leaf functions only — no call/`new`) is correct but
  disqualifies the one value-class-parameter function in a22 (`multiply` ends
  `return new Matrix4(result)`), so it yields ~0 benefit. A correct win needs
  interprocedural heap-write-freedom (transitive purity) — soundness-critical
  (its errors are silent miscompiles the test suite does not catch) for a
  benefit that is **x86-dev-host only** (the arm64 ship target has no
  value-copy problem, already 1.05x). Owner decision: not worth the risk;
  B dropped, no B code committed. Closing fully to 1.05x on x86 would need a
  SIMD vector-type representation — a larger separate effort, not scheduled.
- **Root cause of B's difficulty is a language-design choice (C2), not a
  codegen gap.** A scripting language that defines struct/aggregate
  parameters as **passed by reference** (no value-snapshot guarantee — the
  parameter binds to a pointer into caller storage) reaches this speed for
  free: no parameter copy is ever inserted. subscript's C2 deliberately
  mandates value semantics (snapshot copies; `a04` is the witness), which is
  precisely what turns by-reference parameter passing into an unsound
  optimization requiring interprocedural aliasing proof. The `1`-vs-`99`
  program above returns `99` under by-reference semantics and `1` under
  value semantics — they are observably different languages, not two
  implementations of one. Getting the same performance on this pattern
  without weakening C2 requires (1) sound interprocedural elision, (2) an
  opt-in by-reference parameter mode, or (3) SIMD value types — none free.
  This is a tradeoff subscript chose (value semantics over the copy),
  recorded so future perf work does not re-litigate it as a bug.

### Open
- **`mod.rs` gate doc comment — RESOLVED.** `mod.rs:199-210`
  (`boundary_struct_by_value_supported`) now states `(ptr,len)` /
  string-view is a 16-byte by-value aggregate handled on the by-value
  struct path, not target-neutral; consistent with §12.3a and the
  func.rs Str/Array comments. (This Open item predated the fix.)
- **arm64 re-verification — DONE (2026-07-23, orchestrator, arm64 Mac /
  M2).** The Str/Array routing change (deviation 1) was verified
  behavior-preserving on AAPCS64 by execution, not just inspection:
  `cargo test --offline` **236 passed, 0 failed, zero warnings**; the
  30-entry standing gate (dev-JIT ≡ ship-C-AOT ≡ golden) and the 6
  interop differential tests green; `sh codegen/device-link.sh` produced
  valid arm64 Mach-O (iOS) and ELF PIE (Android) with the changed
  `aot.rs`. No golden changed. The Windows-portability commits are green
  on both host architectures.
- **x86-64 SysV dev marshaling** remains unimplemented (loud error), the
  open ABI case in §12.3a.
- **`benchmarks/src/bin/perf-gate.rs`** C invocation is still Unix-only; no test drives
  it, so it is out of the standing gate.

## Status log

- 2026-07-23: finding recorded; contract §11a written; handoff for site 1
  (build.rs) emitted, implemented, verified, committed (38b7b8c).
- 2026-07-23: full Windows test run captured; three root causes classified;
  contracts §11b (runtime clang toolchain) and §12.3a (Win64 marshaling)
  written; two-task plan set.
- 2026-07-23: Task 1 (toolchain+staticlib, `aot.rs`/`offsetof_layout.rs`)
  implemented and verified — 24 failures → 9, remainder all §12.3a.
- 2026-07-23: Task 2 (Win64 marshaling, `lower/mod.rs`/`lower/func.rs`;
  deviations 1–2 above) implemented and verified — workspace green (236/0).
  §12.3a corrected; follow-ups recorded.
- 2026-07-23: Phase Review (fresh independent reviewer) — 0 CRITICAL,
  0 MAJOR, 3 MINOR (stale `(ptr,len)` "target-neutral" comments; global
  `-Wno-error` flag; hardcoded system-lib list). Win64 ABI, AAPCS64
  non-regression, `_setmode`, clang location, no-panic all verified correct.
  Gate satisfied. Doc-comment MINOR fixed; test-green work committed
  (05aa5da).
- 2026-07-23: Follow-up 1 (generator fix) done — `cemit.rs` casts boundary
  struct pointers to the header type; `-Wno-error` flag removed. Verified
  236/0 flag-free, goldens byte-identical.
- 2026-07-23: Bench port done — clang locator, `.exe` suffix, system libs,
  binary-mode stdout, and a `QueryPerformanceCounter` timing shim in both
  committed C entries. Benchmark runs on Windows; all four subjects match
  the golden. §11b extended.
- 2026-07-28: **P25 owes this file one unrun measurement.**
  `compiler.md` §23.8 criterion 5 is pre-registered kill-or-pass on both a
  Unix and a Windows host: a program calling a foreign function whose
  symbol no supplied native library registers must fail naming the symbol,
  rather than resolving by accident. It matters here specifically because
  `cranelift-jit` 0.125.4 cannot disable its default lookup — `dlsym`
  on Unix, `GetProcAddress` over loaded modules on Windows — so the two
  hosts can differ. The check runs on the demand side (every symbol the
  sole `Linkage::Import` path declares, verified before finalize/link),
  which is host-independent by construction, but that is an argument, not
  a measurement.

  Unix: passing, both tiers (`specs/tracking/p25-header-deprivileging.md`
  §5). Windows: **RUN 2026-07-28 — PASS**.

      $ cargo test --offline -p subscript-codegen --test native_library \
            unregistered_foreign_symbol_is_named_before_platform_lookup
      test unregistered_foreign_symbol_is_named_before_platform_lookup ... ok
      test result: ok. 1 passed; 0 failed

  Measured on `x86_64-pc-windows-msvc` with clang **not** on `PATH` and
  `$CC` unset — the MSVC-`cl` ship tier this file's work landed, no LLVM
  present. Both runners name the unresolved symbol before the Windows
  `GetProcAddress` default lookup can resolve it. The second item settled
  with it: the examples gate now compiles `engine.c` (with its
  `__declspec(thread)` frame record) under `cl` and passes, so that path is
  exercised. P25 criterion 5 is now PASS on both hosts; P25 is COMPLETE.

## Examples gate + MSVC ship tier (2026-07-28)

Contract: `compiler.md` §11c (new). Owner decision 2026-07-28: on
`*-pc-windows-msvc` the ship-C toolchain is MSVC `cl`, not clang — no LLVM
install as a prerequisite. Supersedes §11b's Windows clang choice on the
evidence below.

### Trigger

The `examples/` gate crate (added after the 2026-07-23 port) fails to
build on `x86_64-pc-windows-msvc`. Its dev-dependency
`subscript-interop-fixture` compiles `corpus/interop/interop.c` via the
`cc` crate, which selects MSVC `cl`; `interop.h:39` `typedef _Float16
SubFloat16` does not compile under `cl` (`error C2061`/`C2059` cascade,
cc-rs exit 2). The fixture is also a dependency of `subscript-codegen`, so
`cargo test -p subscript-codegen` hits the same wall.

### Feasibility spike (measured, MSVC 19.44, this host)

The `emit-c` binary emitted each example's ship C; each was compiled with
`cl -nologo -std:c11 -O2 -utf-8`, linked against `subscript_runtime.lib`
plus the §11b system libs, run, and byte-compared to the committed golden.

| subject | result |
|---|---|
| e01–e08 (language only) | compile under `cl`, **8/8 byte-identical** to goldens (integer wrapping, `as` conversions, f32, deterministic formatting included) |
| e09/e10 (opaque-handle C bind) | `error C2016` — emitter outputs `typedef struct Sub_N_EngWorld {}`; MSVC C mode rejects empty structs. After a one-`char` member is added, both **byte-identical** across dev-JIT ≡ ship-C-AOT ≡ golden |
| `engine.c` under `cl` | compiles; `__declspec(thread)` path exercised; only C4819 (silenced by `/utf-8`) — closes the 2026-07-28 open item above |
| `_Float16`/`__fp16`/`std::float16_t`/`<stdfloat>` under `cl` | none compile in any `/std` or language mode — MSVC has no half-width float |
| emitted ship C | never spells `_Float16` (`f16` = `uint16_t` storage, §16.2), so `f16` programs are `cl`-clean |

Conclusion: the MSVC ship tier is feasible. The two blockers are the empty
struct (fixable in the emitter) and the host-header `_Float16` spelling
(fail-loud on Windows, accepted by the owner).

### Task plan (handoff — coding agent)

Sequential; each task's gate is a clean `cargo test -p subscript-examples`
(and `-p subscript-codegen`) on `x86_64-pc-windows-msvc`, plus arm64/Unix
non-regression.

1. **Emitter: no empty structs** (`codegen/src/cemit.rs`). Give an
   otherwise-zero-member emitted struct a single `char` member, or emit it
   as an incomplete type used only behind a pointer (opaque-handle
   pointee; never instantiated by value). Add an accept-corpus entry that
   binds an opaque handle so the standing gate covers it. Gate: standing
   gate byte-exact both tiers; goldens unchanged (the member is never
   read). §11c constraint 1.

2. **Ship-C toolchain on Windows = `cl`** (`codegen/src/aot.rs`;
   `codegen/tests/offsetof_layout.rs`; `benchmarks/src/bin/perf-gate.rs`
   follow the same locator). `host_c_compiler()` selects `cl` on
   `*-pc-windows-msvc`; translate the GNU flags to MSVC (`/std:c11 /O2
   /utf-8`; drop `-fwrapv`/`-ffp-contract=off` — MSVC wraps and does not
   contract by default); compile+link through `cl`. Keep the §11b system
   libs, binary-mode stdout, staticlib name, `.exe` suffix. Gate: standing
   gate byte-exact under `cl`; the signed-overflow guarantee is the gate
   itself (§11c). `$CC` override still honored.

3. **`cc`-crate build scripts stay `cl`; f16 fixture excluded on Windows**
   (`codegen/tests/native-fixture/build.rs`, `examples/build.rs`, and the
   `subscript-interop-fixture` dependency in `codegen`/`examples`). Add
   `/utf-8` to the `cc` build (silences C4819). Gate the interop fixture
   compile and the `gate/two-header-binding.ts` example out of the
   `*-pc-windows-msvc` configuration (fail-loud on `_Float16`, never
   substituted; §11c constraint 2 / §16.2). `engine.c` stays `cl`-built.
   The examples gate's example discovery must skip the two-header gate on
   Windows without weakening it elsewhere.

4. **`examples/host/build.sh`**: the Windows branch links with `cl`
   (translated flags, `/utf-8`), not bare `clang`. It currently assumes
   `clang` on `PATH`; on an MSVC-only host that fails.

5. **Spec**: §11c landed (this change). Confirm §16.3 / examples.md need no
   further edit once the two-header gate is Windows-excluded.

Kept out of scope until the owner asks: making `cl` compile the `_Float16`
fixture (impossible without an integer substitution §16.2 forbids), and a
clang-cl configuration (still needs LLVM, which this decision removes as a
requirement).

### Result (2026-07-28)

Landed in eight commits on `spec/windows-msvc-ship-tier`. Final
verification on `x86_64-pc-windows-msvc` with **clang not on `PATH` and
`$CC` unset** (i.e. MSVC only, no LLVM): a bare `cargo build` reaches
`Finished`; `cargo test -p subscript-codegen` **211 passed, 0 failed**;
`cargo test -p subscript-examples` **5 passed, 0 failed** (capstone
included). The byte-exact differential (dev-JIT ≡ ship-C-AOT ≡ golden)
holds under `cl`; no golden byte changed.

Phase review (fresh independent reviewer, cumulative diff): all focus
areas clean — the float↔narrow-int equivalence re-derived by hand, no
golden changed, `/fp:strict` correct, exclusions Unix-safe, no panics, no
hardcoded paths. One MAJOR: the fixture crate is a workspace member whose
build script compiled `_Float16` unconditionally, so a bare (unscoped)
`cargo build` failed on the MSVC-only host though the dependency edges were
gated. Fixed by gating the fixture `build.rs` on the windows-msvc target
(`CARGO_CFG_TARGET_OS`/`_ENV`). Three MINORs left as accepted: the msvc
phase gate compares zero programs (its sole program is interop-gated; the
example set still exercises both tiers); `$CC` on windows-msvc is assumed
MSVC-style (`clang-cl` works, GNU `clang` would not); the shim collapses
exit codes >255 (`cl` never uses them). No open CRITICAL/MAJOR.

What each piece required, beyond the plan:

1. **Emitter — no empty struct** (`cemit.rs`): a zero-field opaque handle
   now carries `char subscript_opaque;`. Byte-exact (member unread).
2. **Fixture exclusion** was needed in **both** `examples/` and `codegen/`
   (the plan's premise that codegen was already gated was read off a
   contaminated tree — it was not). In codegen the fixture is used by four
   integration targets via `tests/support/native_fixture.rs`; each, plus
   every `corpus::references_interop` entry, is gated off windows-msvc.
3. **Ship-C toolchain = `cl`** (`aot.rs`): resolved with
   `cc::windows_registry::find_tool` (path + `INCLUDE`/`LIB`/`PATH` env);
   `$CC` honored; missing toolchain is a fail-loud `RunError`. Flags
   `/nologo /std:c11 /O2 /utf-8 /fp:strict`. **`/fp:strict` is required,
   not chosen**: the emitted `double inf = 1.0 / 0.0;` is constant-folded
   and rejected (`C2124`) by `cl` under the default `/fp:precise`;
   `/fp:strict` defers it to a runtime infinity and still forbids
   contraction/reassociation, so the differential stays byte-exact
   (§11c). The two CRLF host-observer tests were fixed by injecting a
   `_WIN32`-guarded `_setmode(_O_BINARY)` into the test `host_entry`.
   `offsetof_layout` is gated off windows-msvc (its probe includes
   `interop.h`'s `_Float16`; clang covers it on Unix).
4. **Capstone `build.sh`** uses `cl` through `codegen/src/bin/msvc-cl`, a
   shim that applies the same registry lookup so the `sh`-driven build
   needs no `vcvars`; objects are directed to `target/examples-host` so
   `cl` leaves nothing in the repo root.

**Separate pre-existing bug found and fixed — dev-JIT float↔narrow-int on
x86-64** (`lower/func.rs`). Not a ship-tier or `cl` issue: cranelift's x64
backend cannot emit a float↔integer conversion with a sub-32-bit (i8/i16)
operand or result — it hits `unreachable!()` (`isa/x64/inst/emit.rs:1054`)
— while its arm64 backend can, so the arm64 reference machine that
generated the goldens never exposed it; it had been latent on every x64
host since P14 added narrow numerics. Fix (in-tree, no cranelift fork):
widen a narrow int to i32 before `fcvt_from_*`; for float→narrow-int,
`fcvt_to_*_sat` into i32 then clamp to the narrow range (smax/smin signed,
umin unsigned) then `ireduce` — numerically identical to
`fcvt_to_*_sat(<narrow>)` for every input (in-range, both overflow
directions, NaN→0), so the CLIF stays arch-independent and the arm64
goldens keep agreeing. Regression test added for the saturating
overflow/NaN cases on both tiers.

## CLI on native MSVC — two reference-platform blind spots, 2026-07-30

Found and fixed by the owner on an x86_64-pc-windows-msvc host
(`9177740`): `build` forwarded the C compiler's output to stderr
unconditionally — a no-op on Unix where `cc` is silent on success,
but `cl` echoes source basenames and a localized progress line,
breaking build/check stderr byte-identity — forwarding now happens
only on failure, and cli.md §2.4 was revised to match; and the
`link-flags` test's expected stdout omitted the host system
libraries, which are empty off Windows and five entries on it — the
golden now appends `runtime_system_libraries`. Gate 0 failed on
MSVC (owner) and 714 passed / exit 0 on the macOS reference (this
sweep). The pattern matches this file's earlier entries: a contract
written against the quiet reference platform, falsified by the
noisier one.

**Process note.** The first coding-agent run (a Codex MCP session) was
told to do all four tasks at once with full autonomy; it rewrote 63 files
across unrelated subsystems, went silent, and — after its harness abort —
its server process kept running and rewrote the tree several times after
it was cleaned. Recovery required killing the OS processes. The work was
redone as single-task, file-scoped subagent handoffs, each independently
re-verified against the gate before commit. Lesson (one line): scope a
coding agent to one task and an explicit file set, and confirm the agent
process is dead before trusting a clean tree.

## The copied exclusion guard was forgotten ten times, 2026-08-02

Measured on `x86_64-pc-windows-msvc` at `cb5cd04` (clean tree):

    $ cargo test --workspace
    ... all other harnesses green ...
    -p subscript-codegen --test golden: 7 passed; 11 failed
    a111-interop-async-method-poll: dev-JIT run failed:
      unresolved foreign symbol `subDevicePoll`:
      no supplied native library registers it

All 11 failures are that one error on a different symbol. `tsc -p
tsconfig.json` exits 0; the tree is clean; no golden differs. The
non-interop entry `a110-async-method-receiver` added by the same commit
agrees dev-JIT ≡ ship-C-AOT ≡ golden, as does the whole-corpus sweep.

**Root cause.** §11c constraint 2 excludes the `_Float16` interop fixture
on windows-msvc. `a83a2aa` implemented that exclusion two ways: the
dependency edge and the `native_fixture` module are `#[cfg]`-gated (so
`native_libraries()` returns an empty `Vec` there), while *skipping the
entry* is a separate `#[cfg(all(windows, target_env = "msvc"))] if
references_interop { continue; }` guard copied at four call sites. An
empty library list is not an error at any type — it is exactly what a
non-interop entry gets — so a test that omits the skip guard compiles,
runs the entry, and fails at symbol resolution.

Every per-feature golden test added after `a83a2aa` omitted the guard.
Verified by reading `codegen/tests/golden.rs` at each commit:

| test (entry) | commit |
|---|---|
| `q34_async_…` (a95) | `8c43270` |
| `scalar_parameter_pair_…` (a96) | `d1b79e7` |
| `string_field_pointer_write/read_…` (a97/a98) | `8f797ce` |
| `texture_descriptor_write/read_…` (a99/a100) | `57a572f` |
| `recursive_boundary_pipeline_…` (a103–a105) | `aeaffcf` |
| `struct_pointer_recursive_…` (a106) | `21054e7` |
| `handle_parameter_pair_…` (a107) | `46bf03f` |
| `nullable_handle_parameter_…` (a108) | `f046db1` |
| `r13_async_method_…` (a111) | `cb5cd04` |

So the windows-msvc golden harness has been red since `8c43270`, not
since the latest commit. The "211 passed, 0 failed" recorded above is
`a83a2aa`'s measurement and was accurate then. The reference platform is
Unix/arm64, where the fixture builds and all 11 pass, so the arm64 gate
never showed it.

**Contract.** §11c constraint 3 (written 2026-08-02): the exclusion is
structural — one shared helper whose return type expresses "does not run
in this configuration", so omitting the case is a compile error rather
than a red test on one host. Same shape as P20's TrapSite IR, where a
missing arm became a build error.

### Task plan (handoff — coding agent)

One task, one file: `codegen/tests/golden.rs`. No other file may change;
no corpus entry, golden, or production source is touched.

1. Replace both `native_libraries` definitions with one pair returning
   `Option<Vec<NativeLibrary>>`: `None` when the entry cannot run in this
   configuration, `Some(libs)` otherwise. Off windows-msvc it is always
   `Some` (the fixture when `corpus::references_interop`, else empty). On
   windows-msvc it is `None` when any source references interop, else
   `Some(Vec::new())`. The `references_interop` predicate stays in
   `corpus`; this helper is its only consumer in this file.
2. Every call site handles `None` by skipping that entry — `return` in a
   single-entry test, `continue` in a loop — after printing the skipped
   id so `--nocapture` records what was not compared.
3. Delete the four copied `#[cfg(all(windows, target_env = "msvc"))] if
   … { continue; }` guards (the narrow-entry loop, the sweep's run-set
   filter, the cranelift cross-check); the helper is now the only place
   the predicate is applied. The sweep's whole-corpus golden **count**
   guard stays unconditional — goldens are never deleted, so that check
   must run on every host.
4. The sweep test reports the number of entries compared and the number
   skipped; the skipped count is 0 off windows-msvc.

Gate: `cargo test --workspace` green on this host (0 failed, no new
warnings), and green on the arm64/Unix reference with the skipped count
0 there — i.e. all 11 entries still compared, unchanged. No golden byte
changes; `git diff --stat` touches one file.

### Result (2026-08-02, `bb78eb6`)

Landed as specified, one file, 94 insertions / 60 deletions. Measured on
`x86_64-pc-windows-msvc` by the orchestrator, not taken from the
implementer's report:

    $ cargo test -p subscript-codegen --test golden
    test result: ok. 18 passed; 0 failed          (was 7 passed; 11 failed)
    $ cargo test -p subscript-codegen --test golden -- --nocapture
    golden sweep: compared 77 entries, skipped 34 entries    (77 + 34 = 111)
    $ cargo test --workspace
    48 harnesses, every one `ok`, 0 failed, no new warnings

`git status` lists one modified file; no golden byte changed. The 34
skipped entries are the same set the deleted `#[cfg(msvc)]` run-set
filter removed — the count is now printed and asserted rather than
silent.

Two review findings, both MINOR, both fixed before the commit: the new
`compared + skipped == golden_ids.len()` assertion holds for any number
of skips, so the reference configuration is now pinned by a
`#[cfg(not(windows-msvc))] assert_eq!(skipped, 0)` at that one site; and
the two helper signatures were hand-wrapped where rustfmt puts them on
one line. Whole-file `rustfmt` was forbidden in the handoff and not run:
this host's rustfmt disagrees with the committed formatting in 8 places
in this file and ~873 across the repo, so formatting is not a gate here
and a reformat would have buried the change in unrelated churn.

Off windows-msvc nothing changes: the helper returns `Some` for every
entry there, the deleted guards were all `#[cfg(msvc)]`-only, and the
new zero-skip assertion fails loudly if that ever stops being true. The
arm64/Unix re-run is still owed.

