<!-- §1 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 1. Architecture

```
SWC parse (TS-subset front end, Rust)
  → semantic checker (C1–C14 + Q rules; rule-specific diagnostics, TS positions)
  → typed HIR
  → LIR: one ordered IR (evaluation order, control flow, liveness, trap
    sites as data), lowered once and verified before any consumer reads it
      ├─ dev tier: LIR → cranelift-jit (Windows, macOS, Linux x86-64), hot reload
      ├─ ship tier: LIR → C → platform C compiler (clang; MSVC `cl` on Windows)
      └─ reference interpreter over LIR (the third witness of the gate)
  all three call one runtime crate across a C-ABI-stable boundary:
  Context memory (manual delete, explicit collect), values, strings,
  arrays, traps, coroutine state, Q14 numeric formatting
```

- **Dev-tier hosts**: Windows, macOS, and `x86_64-unknown-linux-gnu`. The
  Linux host runs the full differential gate green (§12.3a SysV dev-JIT
  struct-by-value marshaling landed 2026-08-09;
  `specs/tracking/linux-portability.md`). AAPCS64 (arm64) and Win64
  non-regression after that shared refactor: both re-verified on their
  own hosts and discharged 2026-08-09 (tracking, "Remaining gate"). The
  ship tier ships the arm64 mobile device triples (iOS, Android) and the
  desktop host targets `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`,
  and `x86_64-pc-windows-msvc` (§11; desktops added 2026-08-09).
- One HIR→LIR lowering serves three consumers (§68). Their agreement is
  established by verification — the standing gate, dev-JIT ≡ ship-C ≡
  interpreter ≡ golden, byte-exact (§8.3, §85) — not by construction.
- **Coroutines**: CPS transform in codegen (iOS-safe; no fibers, no stack
  switching). Suspended state lives in the runtime as Context data.
- **Traps** (OOB, null narrowing, checked `as`, literal-range, failed
  narrowing): explicitly emitted checks calling runtime trap functions —
  no signal/SEH harvesting on any platform. Trap reports carry TS
  positions (the compiler embeds a position table).
- **Hot reload** (dev tier): per-function indirection table; module
  recompiled and swapped at frame boundaries. Reload eligibility is
  conservative: a swap is accepted only when the module's **declaration
  hash** is unchanged — the hash covers every type declaration (value
  and reference class field names/types/order, enum member values,
  `FixedArray` shapes), every module-level variable's name and type, and
  every function signature; only function-body edits reload, anything
  else requires a restart. Suspended coroutines whose function was
  replaced are invalidated: resuming one traps with a
  "stale coroutine after reload" diagnostic. (Both rules are this
  contract's; P3's reload demo must exercise an accepted body edit, a
  rejected layout edit, and a stale-coroutine trap.)
- Cranelift is pinned (exact `wasmtime`-family crate versions in
  `Cargo.lock`); the pin moves deliberately, never as a side effect.
