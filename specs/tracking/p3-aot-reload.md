# P3 — AOT, hot reload, standing gate: evidence

Status: in progress, 2026-07-22. Contract: `specs/blocks/compiler.md` §8.

## Gate evidence (orchestrator-verified, independent run)

- `cargo build --offline --all-targets`: zero warnings.
- `cargo test --offline`: 177 passing (145 from P2 plus the AOT/reload
  work), zero failures. Library code has no panic sites; reference
  sweep clean.
- **Standing differential gate: 24/24** — `jit_aot_and_golden_agree_
  byte_for_byte` runs every run-set entry through the dev JIT and the
  AOT tier and compares both to the committed golden, byte-exact, in
  the default `cargo test`. The entry set is derived from
  `corpus/accept/`; a missing host `cc`, missing staticlib, or link
  failure fails the test rather than skipping.
- **No JIT/AOT disagreement on any entry.**
- Reload demos (§8.2), all green: accepted body edit (new behaviour
  observed, globals and live allocations survive), rejected layout edit
  (`ReloadError::DeclarationChanged { declaration: "class Point" }`,
  live program keeps running, a later body-only edit is accepted),
  stale-coroutine trap (`TrapKind::StaleCoroutine`, "stale coroutine
  after reload", at the `.next()` position). A seventh test runs all 24
  entries through reload-mode lowering against the same goldens,
  showing the indirection/globals/epoch changes are semantically
  neutral.
- **Device-triple link with the real lowering** (`sh
  codegen/device-link.sh`, entry a22, orchestrator re-run):
  `Mach-O 64-bit executable arm64` and `ELF 64-bit LSB pie executable,
  ARM aarch64 … interpreter /system/bin/linker64`. No binary executed;
  not part of `cargo test` (needs the NDK).

## Architecture as built

- **One lowering, no tier parameter**: AOT and dev JIT both call
  `lower_module_with(..., LowerOptions::default())`. `LowerOptions::
  reload` is a dev-tier *mode* (indirect calls, Context-reached
  globals, epoch-stamped coroutine frames), never set by the ship
  tier. Two unconditional, tier-neutral changes were needed for AOT:
  exported script functions get `Linkage::Export` as
  `ss_export_<name>`, and `ss_init` is exported.
- **Flags**: dev JIT `is_pic=false`; AOT `is_pic=true`, same
  `cranelift_native` ISA on the host so only PIC differs on the
  differential path; device triples use `isa::lookup(Triple)` plus the
  P0.5 `MachOBuildVersion` workaround for iOS.
- **Reload**: FNV-1a-64 per declaration (classes with indexed field
  name/type pairs, ctor/method signatures, enum member values, module
  variables, function signatures incl. export/generator flags); the
  per-declaration list is what names the first differing declaration.
  Slots are reserved from declarations only, so an accepted swap keeps
  numbering. `Context.fn_table` (offset 8) is repointed on swap; old
  modules are retained so pre-swap code addresses stay valid;
  `Context.reload_epoch` (offset 4) is stamped into coroutine frames
  and checked by `.next()` before the frame is touched.
  `Context::script_depth` guards the frame-boundary rule
  (`ReloadError::ScriptOnStack`).
- **AOT driver**: `run_aot` emits an object, builds the runtime
  staticlib on demand, links with a generated C entry that creates a
  Context, calls the export, and writes the sink bytes to stdout;
  traps are reported on stderr and mapped back through the position
  table.

## Implementation decisions (recorded; binding until revised)

- An accepted swap recompiles the whole module, so the reload epoch is
  per-module: any coroutine suspended across any accepted swap is
  stale.
- "Resume position" for the stale trap is the `.next()` call site (the
  suspended `yield` has no static site at resume time).
- Hash granularity: export-ness and generator-ness are part of the
  function entry; parameter *names*, default *values*, and `let` vs
  `const` on a global are not.
- FNV-1a-64 is written out rather than `DefaultHasher` (which does not
  promise cross-version stability).
- Lambdas are not declarations: a lambda value captured in Context
  state before a swap keeps its pre-swap body; named-function values
  forward through the table and pick up new bodies.
- Module globals move to a host-owned block under reload (fresh module
  data would silently reset state); `ss_init` runs once per session and
  the registered GC root ranges point into a block whose address never
  moves.
- `ReloadSession::call_export` accepts zero-argument `void` exports;
  the general Q12 host-entry surface is P5 work.

## Pending for P3 exit

- Phase Review; findings fixed in severity order.
- Golden freeze for a22–a24 (§2) once the review closes.
