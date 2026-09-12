<!-- §11c of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 11c. C toolchain on Windows is MSVC `cl` (supersedes §11b's Windows clang)

Owner decision 2026-07-28: on `*-pc-windows-msvc` the ship-C toolchain is
the native MSVC compiler `cl`, not clang. subscript must build on Windows
with the platform toolchain alone — no LLVM install as a prerequisite.
This supersedes §11b's choice of clang on Windows; §11b still governs the
Unix host, and clang/clang-cl remains an optional cross-check. Every
other Windows detail §11b lists — the system import libraries, binary-mode
stdout, the `subscript_runtime.lib` staticlib name, the `.exe` suffix — is
unchanged and applies to the `cl` path.

Flags: `/nologo /std:c11 /O2 /utf-8 /fp:strict`. `/utf-8` makes `cl` read
the UTF-8 sources without the CP932/ACP-dependent C4819 warning. `/fp:strict`
is the `-ffp-contract=off` equivalent — it forbids contraction and
reassociation — but the stricter mode is *required*, not merely chosen: the
emitter writes `double inf = 1.0 / 0.0;` for `Infinity`, which `cl`
constant-folds and rejects (`C2124`) under the default `/fp:precise`;
`/fp:strict` defers it to a runtime infinity (clang only warns). Output and
link syntax is MSVC's: `/Fo:<dir>\` for objects, `/Fe:<exe> -link` for the
executable, `.obj` object files, and the §11b system import libraries as
bare `.lib` names (not `-l`). The compiler and its `INCLUDE`/`LIB`/`PATH`
environment are discovered with `cc::windows_registry::find_tool`, so no
prior `vcvars` shell is needed — `codegen/src/bin/msvc-cl` is a thin shim
that applies that lookup for the `sh`-driven capstone build, which cannot
run `vcvars` itself.

**Signed-overflow soundness.** §11b pinned clang because `-fwrapv` makes
signed overflow defined two's-complement wrap, the language's semantics;
`cl` has no `-fwrapv` equivalent. MSVC does not optimize on the
signed-overflow-is-UB assumption and wraps two's-complement *(docs)*. The
guarantee is re-established the project's standing way — by verification,
not by a compile flag: the Windows standing gate runs `cl` and stays
byte-exact (dev-JIT ≡ ship-C-AOT ≡ golden, §11), so any `cl` divergence on
overflow breaks the gate. Evidence (measured 2026-07-28, MSVC 19.44):
emitted ship C for the language examples e01–e08 compiles under `cl` and
is byte-identical to the goldens, the wrapping and `as`-conversion cases
(e01) included; `engine.c` compiles under `cl` (C4819 only, silenced by
`/utf-8`).

Four constraints the `cl` path adds, all measured:

1. **The emitter must not output an empty struct.** MSVC C mode rejects a
   zero-member struct (`error C2016`); clang accepts it. The opaque-handle
   pointee is emitted today as `struct Sub_N_<Handle> {}`. It must carry
   at least one member (a single `char`), or be an incomplete type used
   only behind a pointer — the pointee is never instantiated by value in
   emitted C, so either is ABI-safe. Measured: adding a `char` member
   keeps e09/e10 byte-identical across both tiers under `cl`.

2. **A host header that spells a boundary type `_Float16`/`__fp16` fails
   loud on the `cl` path.** Emitted C never spells `_Float16` (`f16` is
   `uint16_t` storage, §16.2), so `f16` *programs* build under `cl`. But a
   bound host facade whose C source spells the type directly — the
   `corpus/interop` fixture does — cannot be compiled by `cl`: MSVC 19.44
   has no half-width float in any `/std` or language mode (measured). This
   is the §16.2 fail-loud stance, never an integer substitution. On the
   MSVC-Windows configuration the interop fixture and the two-header gate
   that binds it are therefore excluded from the gate; the clang build
   still covers them.

3. **Constraint 2's exclusion is structural, not per-test.** A test that
   names a corpus entry must obtain its native libraries from one shared
   helper whose return type expresses "this entry does not run in this
   configuration"; a call site that ignores that case does not compile.
   Rationale is measured, not stylistic: constraint 2 was first
   implemented as a `#[cfg(all(windows, target_env = "msvc"))] if
   references_interop { continue; }` guard repeated at each call site, so
   an added test that omitted the guard ran an interop entry against no
   fixture and failed. Every per-feature golden test added after the
   exclusion landed (`8c43270` onward) omitted it; measured on
   `x86_64-pc-windows-msvc` 2026-08-02, `cargo test -p subscript-codegen
   --test golden` was 7 passed / 11 failed, every failure the same
   `unresolved foreign symbol ...: no supplied native library registers
   it`. A guard that must be copied is a guard that is forgotten; the
   type system carries it instead.

   The exclusion never weakens the gate off windows-msvc: on every other
   configuration the helper supplies the fixture for every entry that
   references it, and the standing gate (§2, §11) compares the full run
   set. On windows-msvc an excluded entry is compiled and run by neither
   tier, and the run set the test reports counts only what it compared.

4. **A toolchain failure report must carry both streams.** `cl` and
   `link.exe` write their diagnostics to stdout. Unix compilers write
   them to stderr. A report of one stream only is empty on one host
   family, so the caller sees the failure and no cause. Measured
   2026-08-05 on windows-msvc: `run_c_aot_with_native_libraries` failed
   for every program of a host facade that binds a C header, and printed
   `compiling/linking the emitted C failed:` with nothing after it. The
   real cause was 60 `cl` errors on stdout, and a wrapper compiler was
   needed to read them. `tool_output_report` now renders both streams
   with a label for each, drops an empty stream, and names a silent
   command.

   Every call site that runs a C compiler or a linker reports both
   streams. Twelve call sites use `tool_output_report`: the Cranelift-AOT
   link, the C-AOT compile, the `aot` and `cemit` test hosts, the
   `offsetof` probe, the examples host gate, and the perf, size, and
   cross-language benchmarks. The CLI writes both streams to its own
   stderr, then returns the exit status. Two exceptions stay on stderr
   alone, because cargo writes its diagnostics to stderr on every
   platform: the runtime static library build in `codegen`, and the same
   build in the CLI. A report of a program run is not in scope here. It
   reports the run, not the toolchain.
