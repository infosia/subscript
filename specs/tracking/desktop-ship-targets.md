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

## Open confirmation

Run the 5-triple ship-target object test once on windows-msvc (the
next routine gate run there covers it; the Cranelift arm64 backend
is now explicit in the crate, so the darwin/ios triples emit there
by construction).
