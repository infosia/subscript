# P3 — AOT, hot reload, standing gate: evidence

Status: COMPLETE, 2026-07-23. Contract: `specs/blocks/compiler.md` §8.

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
  `subscript_export_<name>`, and `subscript_init` is exported.
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
  data would silently reset state); `subscript_init` runs once per session and
  the registered GC root ranges point into a block whose address never
  moves.
- `ReloadSession::call_export` accepts zero-argument `void` exports;
  the general Q12 host-entry surface is P5 work.

## Phase Review (2026-07-22/23)

Fresh no-context review with adversarial probes run in a scratch export:
1 CRITICAL, 2 MAJOR, 11 MINOR. Fixed (suite 177 → 183):

- CRITICAL: a debug probe file was committed at the repository root
  carrying an absolute developer path — deleted. It was also the only
  panic site outside a test module. Sweep procedure corrected: the
  reference sweep now runs over all tracked files (`git grep`) with no
  path or extension filter; the earlier filtered sweep is what let it
  through.
- MAJOR: a trap was permanent, so after a stale-coroutine trap an
  unrelated export returned the stale record with no output and even an
  accepted reload did not recover — the contract's own stale rule
  bricked the session reload exists to keep alive. Contract amended
  (§8.2, Rev 5: a trap does not end the dev session) and implemented:
  `Context::clear_trap()` runs at the top of every host→script call,
  before `enter_script()`, so a record always belongs to its own call.
  The offset-0 flag invariant is intact — layout unchanged, the clear
  happens at `script_depth == 0` where no emitted check can observe the
  transition, and it is reporting-only: allocations, poisoned headers,
  globals, roots, sink and `reload_epoch` are untouched, so a stale
  coroutine stays stale.
- MINOR fixed: golden floors raised 21 → 24 (a deleted golden can no
  longer pass silently); a self-comparing object-shape assertion
  replaced with explicit host format/arch; nested `cargo build` per
  `run_aot` (26 per suite) reduced to a freshness-checked `OnceLock`
  (cold, warm and stale-source paths all verified); `device-link.sh`
  derives the NDK prebuilt tag from `uname` so the Android half runs on
  any host (§3); failed reload generations free their code pages;
  initializer-outside-the-hash behaviour documented where a caller
  looks; three stale docs corrected.

Verified by the review, unchanged: the standing gate is not theatre
(perturbing a golden fails both tiers independently; adding output to
the AOT entry fails JIT≡AOT on every entry; a missing staticlib fails
rather than skips), one lowering with no tier branch (`opts.reload` is
a dev-tier mode the ship path cannot reach), and the declaration hash
caught all 16 adversarial declaration pairs, comparing per-declaration
entries rather than a single 64-bit value.

## Golden freeze (§2)

a22–a24, captured from the dev JIT at P2, are **frozen** as of
2026-07-23: the AOT tier reproduces all 24 goldens byte-exactly through
the shared lowering, and `git diff` over `corpus/` across the P3 commits
is empty — the goldens were confirmed, never adjusted. Later changes
follow the golden-change procedure (§2). The test floors now pin 24
entries.

## Known limitations of "a trap does not end the dev session" (§8.2)

Recorded 2026-07-23 when the rule landed; none blocks P4, all are
candidates for a small phase after it.

1. **Post-trap state is a valid execution prefix, not a valid state.**
   A trap mid-statement leaves globals partially updated (`a = 1;`
   applied, `b = 2;` not), so the script's own invariants can be broken
   while the language's are intact. Subsequent calls then fail in ways
   that look unrelated to the original trap.
2. **A stale coroutine driven every frame traps every frame, forever.**
   Staleness is permanent by design and the language cannot recreate a
   coroutine, so after any accepted swap a module-level generator that
   the host drives per frame produces an endless trap stream with no
   in-language recovery. For this case the rule is worse for the
   developer than stopping was, unless the host recreates the
   coroutine. Fix candidate: report the invalidated coroutines from
   `reload()` so the host can recreate them instead of waiting for the
   trap.
3. **Tier divergence after a trap, invisible to the standing gate.**
   The dev tier clears and continues; the AOT entry writes the sink,
   reports on stderr and exits non-zero (`codegen/src/aot.rs`). No
   run-set entry traps, so the differential gate cannot see the
   difference: a trapping program can behave differently in dev and
   ship. Fix candidate: an accept entry that traps deliberately, with
   its expected report as the golden, compared on both tiers.
4. **Not every trap kind should be recoverable.** `AllocationFailure`
   (Context memory exhausted) is cleared like any other, so a systemic
   condition is re-reported as a series of sporadic errors; dev-tier
   retained bytes make exhaustion reachable in a long session. Fix
   candidate: mark allocation failure and internal errors
   non-recoverable in §8.2.
5. **Recovery is tested for two trap kinds only** (stale coroutine,
   out-of-bounds). Use-after-delete, UTF-8 boundary, class-mismatch
   narrowing and division-by-zero have no post-trap session test.

## P3 exit

Gate (§8.4) met: run set matches goldens under AOT; JIT≡AOT≡golden is
the default `cargo test` (24/24); reload demonstrated on run-set
programs (accepted body edit, rejected layout edit, stale-coroutine
trap, plus trap-recovery cases); device-triple link green with the real
lowering. Zero open CRITICAL/MAJOR. Next: P4 — performance gate against
a hand-written C baseline (§3).
