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

## Status log

- 2026-07-23: finding recorded; contract §11a written; handoff to coding
  agent emitted for site 1.
