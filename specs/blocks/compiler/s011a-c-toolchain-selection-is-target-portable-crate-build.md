<!-- §11a of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 11a. C toolchain selection is target-portable (crate build)

`codegen/build.rs` compiles the synthetic interop callee
(`corpus/interop/interop.c`) and archives it into every binary linking
`subscript-codegen`; being the crate's build step, it decides whether
`cargo build`/`cargo check` succeed at all. It selects the
target-appropriate C toolchain instead of hardcoding the Unix `cc`/`ar`
drivers: selection is by Rust target triple through the `cc` crate (already
resolved in `Cargo.lock` and present in the local registry cache —
offline-clean, no fetch), which drives the GCC/Clang driver
(`-std=c11 -O2 -fPIC`) plus `ar` on Unix targets and the MSVC toolchain
(`cl`/`lib`) on `*-pc-windows-msvc`. The `-std=c11` dialect pin (§11) is
carried across; the exact per-toolchain flag set is the implementation's
and is validated by execution, not asserted here. `CC`/`AR` overrides
remain honored where the driver accepts them.

**Unix clang selection (2026-08-09).** The synthetic interop fixture
(`corpus/interop/interop.h`) is clang-only by construction: it spells
`_Nullable` and `_Float16` to exercise the libclang binder. GCC rejects
`_Nullable`; clang before 15 rejects `_Float16` on x86-64 (measured on
Ubuntu 22.04: gcc 11, gcc 12, clang 14 each fail; clang 15 compiles the
fixture). The Unix default driver is therefore wrong for this fixture on a
GCC host. The fixture build scripts (`codegen/build.rs`,
`codegen/tests/native-fixture/build.rs`) must select a clang that compiles
x86 `_Float16`: resolve `$CC` first, then `clang`, then a `clang-NN` on
`PATH` newest-first; the first driver that compiles `_Float16` wins. A host
with no capable clang fails loud (§8.3), never a silent GCC fallback. This
matches §11b's runtime clang resolution and the fixture's "gate compiler is
clang" design. Evidence: `specs/tracking/linux-portability.md`.

Consequence: the workspace compiles on `x86_64-pc-windows-msvc` — already a
stated dev-tier host (§1). This is the *compilation* contract only; the
C-invocation sites that run while tests execute are §11b, and the dev-JIT
struct-by-value ABI is §12.3a. The bench harness (`benchmarks/src/bin/perf-gate.rs`)
compiles C only when the benchmark is run (no test drives it), so it is out
of the standing test gate; it takes the same clang path (§11b) and is
verified by running it, not by the suite.
