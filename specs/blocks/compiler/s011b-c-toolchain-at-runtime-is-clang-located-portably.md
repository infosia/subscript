<!-- §11b of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 11b. C toolchain at runtime is clang, located portably

**Windows note (2026-07-28):** §11c supersedes this section's Windows
toolchain choice — the default Windows ship-C compiler is MSVC `cl`, not
clang. The rest of §11b (system import libraries, binary-mode stdout,
staticlib name, `.exe` suffix) still applies to the `cl` path.

Three paths invoke a C toolchain while the standing gate runs — the two
ship-C AOT runners (`codegen/src/aot.rs` `run_aot`/`run_c_aot`) and the
`offsetof` layout probe (`codegen/tests/offsetof_layout.rs`). They compile
and link the emitted ship C (or a layout probe) and must reproduce the
§11-pinned ship semantics exactly, so they invoke **clang** — the compiler
§11 pins, with its flags (`-std=c11 -O2 -fwrapv -ffp-contract=off`) — not a
target-default driver whose signed-overflow or fp-contraction behaviour
would diverge from the goldens for the wrong reason. This is why clang, not
the MSVC `cl` driver, is used on Windows even though `cl` is what §11a's
crate build selects for the plain-C-ABI synthetic callee. clang's GNU-style
driver flags are identical across Unix and Windows; on Windows clang targets
`*-pc-windows-msvc` and links through the installed MSVC linker, so it
consumes the MSVC-ABI runtime staticlib and object without translation.
Resolution: `$CC` if set, else `clang` on `PATH`, else — on Windows — the
standard LLVM install (`%ProgramFiles%\LLVM\bin\clang.exe`); a missing clang
fails the run, never skips it (§8.3). Two host-shape details are
target-aware: the linked executable carries the host executable extension
(`.exe` on Windows), and the on-demand runtime staticlib is named by target
convention — `libsubscript_runtime.a` on Unix, `subscript_runtime.lib` on
`*-pc-windows-msvc` (`SUBSCRIPT_RUNTIME_STATICLIB` overrides resolution
entirely).

Two more Windows-only link/output details are required for the byte-exact
gate.
(1) A manual clang link of the runtime staticlib must add the Windows system
import libraries `rustc` supplies automatically (`kernel32`, `ntdll`,
`userenv`, `ws2_32`, `dbghelp` for the current toolchain — matched to
`rustc --print native-static-libs`); `cargo` links them for `rustc`, a hand
clang link does not. (2) A committed host entry C that writes the sink to
stdout sets that stream to binary mode (`_setmode(_fileno(stdout),
_O_BINARY)`, `_WIN32`-guarded) so the MSVCRT text mode does not translate
`\n` to `\r\n` and corrupt the byte-compared output; a no-op on every other
platform.

**Linux runtime system libraries (2026-08-09).** A manual clang link of the
runtime staticlib on Linux must add the platform native system libraries
`rustc` supplies automatically, the same way the Windows path adds its
import libraries. macOS hides them in `libSystem`, so the gap is latent
there and appears first on Linux: without them the link fails with
`undefined reference to exp/log/pow/sin/…` from the runtime's `f64` math
(measured). The list is the **set** `rustc --print native-static-libs` reports for the
target (`gcc_s`, `util`, `rt`, `pthread`, `m`, `dl`, `c` on
`x86_64-unknown-linux-gnu`) — the whole set, never just `-lm`; link order is
immaterial for these libraries. `runtime_system_libraries` returns this set
on Linux, empty on macOS. Evidence: `specs/tracking/linux-portability.md`.

The benchmark harness (`benchmarks/src/bin/perf-gate.rs`, with its committed
`benchmarks/a22-baseline.c` and `benchmarks/aot-entry.c`) is a fourth clang site with
the same treatment — clang location, `.exe` suffix, the system libraries on
the staticlib links, and binary-mode stdout in both committed C entries so
each subject matches the frozen golden. *(2026-09-05.)* The library list
is `subscript_codegen::runtime_system_libraries`, the one §11b names. The
two harnesses held a local list that was Windows-only, so on Linux the
staticlib link resolved no `-lm` and the ship subject failed to link
(`tan`, `acos`, `log`). A C link site in this repository reads the
canonical list; it does not carry a copy. Its C entries also read the timed
span from `QueryPerformanceCounter` on Windows (the MSVC UCRT has no
`clock_gettime`/`CLOCK_MONOTONIC`), converted to nanoseconds by
overflow-safe integer arithmetic — the same monotonic span, and since every
subject is timed the same way the cross-subject ratio is timing-method
independent. *(2026-09-05.)* On Linux both harnesses compile the timing
entry with `-D_POSIX_C_SOURCE=199309L`. The entry is compiled with
`-std=c11` to match the ship path's dialect, and glibc's `<time.h>`
hides `clock_gettime` and `CLOCK_MONOTONIC` in a strict ISO dialect.
Measured on glibc 2.35: `-std=c11` fails, `-std=gnu11` passes, and
`-std=c11 -D_POSIX_C_SOURCE=199309L` passes. Darwin shows both without
the macro, so the arm64 reference machine never met it.

The macro must come from the command line. The entry translation unit is
`runtime/include/subscript_runtime.h` concatenated with
`benchmarks/aot-entry.c`, so the first libc header of the unit comes from
the runtime header. glibc latches its feature set at that header, and a
`#define` inside `aot-entry.c` is then too late. The macro is
Linux-scoped, because Darwin's strict-POSIX mode hides
`CLOCK_MONOTONIC_RAW`, which `benchmarks/boundary-noop.c` reads. Its gate run is a test target (§3), so `cargo test
--release` reports a missed threshold; the §3 performance thresholds it reports are
machine- and toolchain-dependent (the recorded ship-tier figures are the
reference setup's, §11).
