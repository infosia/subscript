# Desktop ship targets — evidence

Status: **landed 2026-08-09** against `specs/blocks/compiler.md` §11
"Ship targets". Owner decision 2026-08-09: `aarch64-apple-darwin` and
`x86_64-pc-windows-msvc` join `x86_64-unknown-linux-gnu` (de52397) as
desktop host ship targets. Contract `fba2012`, implementation
`39623e7`, README `0a0cd37`.

## Why the declaration was cheap

The standing gate executes the ship-C path (`run_c_aot`: emit C →
host C toolchain → runtime staticlib link → run → byte-compare) on
every dev host on every test run. At declaration, that evidence
stood on both hosts: the arm64 macOS reference machine (full suite
green) and windows-msvc (53 harnesses, 904 passed, 0 failed —
commit `b3b670f`). The slice added tooling parity only.

## What landed

- `SHIP_TARGET_TRIPLES` is five; the ship-target object test asserts
  Mach-O/AArch64 for `aarch64-apple-darwin` and COFF/X86_64 for
  `x86_64-pc-windows-msvc`.
- `device-link.sh` gains a native macOS section (host clang, run,
  byte-compare against the committed golden; libSystem supplies the
  system libraries per §11b). Verified here: iOS + macOS + Android
  artifacts, macOS smoke byte-equal to the golden, exit 0.
- Two defects found by running the pulled work on this host, both
  fixed before this slice landed:
  1. `de52397`'s x86-64 triple required the Cranelift x86 backend on
     every host; the crate enabled only `arm64` explicitly, so the
     ship-target test failed on arm64 macOS
     (`ISA for x86_64-unknown-linux-gnu: Support for this target is
     disabled`). Fixed in `37561d0` (both backends explicit).
  2. The Android link had no system libraries and failed with
     undefined libm references (`tanh`) on any NDK host; NDK-less
     hosts skip that half, so the Linux measurement could not see
     it. Fixed in `39623e7` with the measured set from
     `rustc --print native-static-libs --target
     aarch64-linux-android`: `-ldl -llog -lunwind -lm -lc`.

Lesson, one line: a target added to a shared list is verified on
every host that runs the list, not only the host that added it.

## Gates

Arm64 reference machine: full workspace release gate exit 0;
ship-target test (5 triples) exit 0; `cargo fmt --check` exit 0;
`device-link.sh` exit 0. The toolchain pin carries the device-triple
std targets (`rust-toolchain.toml`, `37561d0`).

## Windows confirmation — closed 2026-08-09

The owed run is done. Measured on `x86_64-pc-windows-msvc` at
`085ce32`, with the pinned toolchain 1.95.0:

    $ cargo test -p subscript-codegen --lib ship_target_triples
    test aot::tests::ship_target_triples_emit_objects_for_the_real_lowering ... ok
    test result: ok. 1 passed; 0 failed                     exit 0

All five triples emit from the Windows host. The test states the
format and the architecture per triple and reads neither back out of
the object under test. The `emit-object` binary writes the same five
objects on this host, exit 0:

| Triple | Format | Bytes |
|---|---|---|
| `aarch64-apple-ios` | Mach-O | 10008 |
| `aarch64-linux-android` | ELF | 11968 |
| `x86_64-unknown-linux-gnu` | ELF | 11896 |
| `aarch64-apple-darwin` | Mach-O | 9984 |
| `x86_64-pc-windows-msvc` | COFF | 10246 |

The prediction held: the explicit arm64 and x86 Cranelift backends
(`37561d0`) let the darwin and ios triples emit from an x86-64 host.

The toolchain pin also holds on this host. `rust-toolchain.toml`
declares the two device-triple std targets, and rustup installed both
on Windows. `rustup show` lists `aarch64-apple-ios`,
`aarch64-linux-android`, and `x86_64-pc-windows-msvc`. The pin
strands no host.

Full workspace gate at the same commit: 53 harnesses, 904 passed, 0
failed, 1 ignored; `cargo build --workspace --all-targets` 0 warnings
in the dev profile and the release profile; `cargo fmt --check` exit
0; `npx tsc` exit 0. See `windows-portability.md` for that run.

Not covered here: `device-link.sh` needs an NDK and an Apple
toolchain, so the Windows host does not link the device artifacts. The
Windows evidence is object emission, not device link.
