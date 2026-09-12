<!-- §11 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 11. P4.3 ship tier — C emission (LLVM)

Owner decision 2026-07-23 (plan §8 Rev 2): the ship tier is
HIR→C→platform C compiler (`clang -std=c11 -O2 -fwrapv
-ffp-contract=off`, i.e. LLVM). The emitted C, the synthetic interop
callee, and the device-link builds all pin **`-std=c11`** (owner
2026-07-23) — the emitted dialect is verified C11 (compound literals,
`<stdint.h>`/`<stdbool.h>` types, C-ABI layout; strict
`-std=c11 -pedantic-errors` compiles it with no GNU extensions), so the
ship tier does not depend on the platform compiler's default `-std`.
`-fwrapv` makes signed overflow defined two's-complement wrap;
`-ffp-contract=off` matches the language's non-contracting f32.
Evidence: P4/P4.1/P4.2 (`specs/tracking/p4-performance.md`) — Cranelift
ship-AOT 23× a C baseline, ≈73% attributable to its scalar output;
emitted C carrying the same semantics measured 1.05×. The dev tier is
unchanged (Cranelift JIT, hot reload). The P4.2 emitter
(`codegen/src/cemit.rs`) is the a22-only spike; this phase makes it the
ship tier.

- **Coverage**: the C emitter handles the full run set a01–a24
  (reference classes, `Nullable`, lambdas/function pointers,
  generators/CPS, methods, `while`/`switch`/ternary, computed strings —
  everything the run set uses), not just a22's subset. Constructs
  outside the run set may return a clean `Err` until a corpus entry
  needs them.
- **Semantic faithfulness**: the emitted C carries the language's
  semantics exactly as the CLIF path does — C2 value copies, checked
  growable-array indexing and push growth, f32 kept in `float`, the
  P4.1 proof-based FixedArray bounds-check elimination and copy
  elision, Q14 formatting, and the trap model (a trap reports and
  returns without aborting the host, matching the runtime). It is not a
  hand-optimized rewrite; where semantics and CLIF differ the emitter
  is wrong.
- **Standing gate (replaces §8.3's Cranelift-AOT column)**: the default
  `cargo test` path becomes **dev-JIT ≡ ship-C-AOT ≡ golden**,
  byte-exact, all 24 entries. This is where dev/ship agreement is now
  established — by verification, since the two tiers are separate
  lowerings (plan §8 Rev 2). The `cranelift-object` AOT path is retained
  only as an optional extra cross-check column; its ship role has ended.
- **Ship targets**: the emitted C is compiled and linked per target,
  replacing the `cranelift-object` device link.
  - Two cross-compiled **mobile device triples** — `aarch64-apple-ios`
    (Xcode clang) and `aarch64-linux-android` (NDK clang) — compile+link
    only, as §3, no device execution.
  - **Desktop host targets** — each natively compiled, linked, **and
    executed** byte-exact by the standing gate when the gate runs on a
    host of that triple (`run_c_aot`; dev-JIT ≡ ship-C-AOT ≡ golden), so
    they are the most-verified ship targets — the mobile triples never
    execute:
    - `x86_64-unknown-linux-gnu` (added 2026-08-09). Measured:
      `subscript emit` → clang (host runtime staticlib + the §11b Linux
      system libraries) produces an x86-64 ELF PIE that runs and prints
      the golden output (`specs/tracking/linux-portability.md`).
    - `aarch64-apple-darwin` and `x86_64-pc-windows-msvc` (owner
      decision 2026-08-09). Standing evidence at declaration: the full
      gate executes the ship-C path on the arm64 macOS reference machine
      (every suite green, every golden byte-exact) and on windows-msvc
      (53 harnesses, 904 passed, 0 failed — commit `b3b670f`), through
      the §11b/§11c host toolchains. The declaration slice adds tooling
      parity only: both triples join the retained Cranelift-object
      cross-check (`SHIP_TARGET_TRIPLES`) with per-triple object-format
      assertions (Mach-O/ARM64, COFF/X86_64), and the macOS host gets a
      native `device-link.sh` section in the Linux section's shape. The
      Windows link smoke is the standing gate itself (`run_c_aot` under
      MSVC `cl`); the shell script does not run there and no separate
      script is added.

  The P0.5 kill criterion is unaffected: it already passed, and C emission
  was its pre-registered fallback architecture.
- **Reuse or replicate the runtime**: the emitted C may link the
  existing runtime staticlib or emit self-contained equivalents; either
  way behaviour must match the runtime (the standing gate enforces it).
- **Gate**: run set a01–a24 matches goldens under the C ship tier;
  dev-JIT ≡ ship-C-AOT ≡ golden is the default `cargo test`; device
  triples compile and link.
