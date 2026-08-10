# R25 — entry-less dev sessions

Status: **landed 2026-08-10** against `specs/blocks/compiler.md`
§53. Origin: downstream request R25. Contract `78b34a3`,
implementation `432b3b3`.

## The request

The downstream's windowed example is host-driven: the host calls
the script once per redraw. A rename of the entry to `frame()`
failed at session creation with
`no exported `main(): void` entry point`, and the downstream
reverted it. The ship tier supports the shape
(`subscript emit --no-entry`); the dev session did not.

## Findings on this host, before the contract

- The blocker was one point: the lowering resolved `main`
  unconditionally. The session driver does not use `Lowered.main`;
  it calls exports through the slot table.
- The C emitter already splits on `require_main`
  (`emit_c` / `emit_c_without_main`).
- `subscript check` accepts the entry-less program (measured:
  exit 0).
- No existing test pinned session-creation failure for an
  entry-less module.

## What landed

The lowering adopts the C emitter's split: `LowerOptions` gains
`require_main` with a strict default, and `Lowered.main` becomes
optional. Only reload-mode lowering selects the permissive form.
Every run path keeps the strict form through one fallible accessor
that carries the unchanged diagnostic. `call_main` on an
entry-less session returns the existing `call_export` diagnostic
and the session continues.

No corpus entry: the accept and reject sets do not move, so the
evidence is the four §53.4 unit tests, which cover all three
constructors, the loud `call_main`, a body swap of `frame` on the
entry-less session, and the strict `run_jit`.

## Gates (this host, at `432b3b3`)

- Golden ledger: 183 files, all SHA-256 unchanged.
- `cargo test --offline --workspace --release`: 925 passed, 0
  failed, 1 ignored, exit 0.
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check` exit 0; `tsc` gate exit 0.
