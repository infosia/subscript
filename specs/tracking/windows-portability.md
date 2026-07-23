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
4. `bench/src/main.rs:643` — bench harness. Bench path. Follow-up.

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

Out of scope for the test gate: `bench/src/main.rs` (no test drives it).

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

## Follow-ups (open)

- **Generator fix (MINOR), CLAUDE.md principle 6.** Deviation 2 masks a
  generator issue with a compiler flag rather than fixing the generator.
  The C emitter (`codegen/src/cemit.rs`) should emit an ABI-compatible
  pointer type or an explicit cast at the foreign-call boundary so the
  `-Wno-error` flag is unnecessary. Tracked, not blocking (ABI-safe, green).
- **Reconcile the `mod.rs` gate doc comment**, which still says `(ptr,len)`
  is target-neutral — now contradicted by §12.3a and the func.rs Str/Array
  comments.
- **arm64 re-verification.** The Str/Array routing change also runs on the
  AAPCS64 path (16 bytes → ≤16 → two registers, behavior-preserving by
  inspection) but was not executed on an arm64 host this session; the
  standing arm64 gate covers it when next run there.
- **x86-64 SysV dev marshaling** remains unimplemented (loud error), the
  open ABI case in §12.3a.
- **`bench/src/main.rs`** C invocation is still Unix-only; no test drives
  it, so it is out of the standing gate.

## Status log

- 2026-07-23: finding recorded; contract §11a written; handoff for site 1
  (build.rs) emitted, implemented, verified, committed (9a11b94).
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
  Gate satisfied. The doc-comment MINOR is being fixed before commit; the
  other two remain tracked follow-ups above.
