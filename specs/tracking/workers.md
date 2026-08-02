# Workers — evidence

Status: in progress, 2026-08-02. Owner-initiated: a TS-Worker-like
concurrency model. Contract rounds: §38 (this round), then runtime
channels, then the pattern example + Q35 register entry.

## The model (agreed 2026-08-02, chat)

Worker ≈ host-owned OS thread + one dedicated Context; messages are
C-ABI POD copies through runtime-owned thread-safe channels;
delivery is poll-driven (Q34 shape). Not carried from TS, by
standing invariants: `new Worker(url)` dynamic loading (AOT — a
worker entry is a named export), language-spawned threads (the host
owns platform capabilities), push `onmessage` (§14.6 — each worker
thread pumps its own Context), and shared-mutable-state APIs (the
contracted surface is copy-only messaging; the C ABI physically
permits sharing, and synchronization is then the host's problem —
scripts are trusted).

Owner ruling (2026-08-02, chat): the "stdlib grows in computation
only" line is a revisable convention, not a principle, and must not
be used to force a host-facade-only design. The channel layer
therefore lands in the runtime as `subscript_rt_*` C API with a
shipped header that `subscript bind` mirrors — one implementation,
host-portable scripts, threads still host-owned. No CLAUDE.md
change is needed for that shape.

## Round 1 grounding (§38)

- Runtime has no mutable global state: the only non-test `static`
  in `runtime/src` is a `const` BMP lookup table; no
  `thread_local`, no `Mutex`, no `unsafe impl Send/Sync`. One
  Context per thread is structurally sound.
- Dev tier reaches module globals through the Context-owned block
  (`lower/mod.rs`, `Context::globals_offset()` = offset 16;
  `reload.rs` calls `set_globals`). Ship tier emits process-wide
  C `static g_*;` (`cemit.rs`). The divergence is invisible to the
  differential gate (one Context) and to `context-per-scene`
  (sequential; `subscript_init` reinitializes). Found by reading
  both lowerings while contracting §38 — corrected the reviewer's
  earlier claim that module state was already Context-owned in both
  tiers.

## Round 1 result (2026-08-02, landed)

Landed per §38 in four files: `cemit.rs` (a `SubscriptModuleGlobals`
typedef, every access through an inline accessor reading the
Context slot at `Context::globals_offset()`, `static g_*` emission
removed, emitter no-static test), `context.rs` (Context owns the
block; re-init zeroes and reuses it; freed with the Context),
`ffi.rs` (`subscript_rt_globals_init`, an internal generated-code
ABI kept out of the public host header), `aot.rs` (the two-thread ×
two-Context harness, ship tier interleaved deterministically by a
pthread condvar coordinator in the C host).

Red first: at the pre-fix pin the ship harness failed with
Context 0 printing `1\n3\n` against the single-Context reference
`1\n2\n` — the shared-static interleave §38 predicted; the dev half
already passed.

Reviewer verification: gate 48 harnesses, 809 passed, 0 failed,
exit 0 read directly; `tsc -p tsconfig.json` exit 0; no golden and
no generated header moved.

Benchmark (§38.2-4), reviewer-run, serialized, `--warmup 8`, all
three runs passing the §9 noise check: pre-§38 HEAD 1.52× of the
hand-C baseline (median 6.270 ms); §38 tree 1.48× and 1.56×
(medians 6.18–6.23 ms). Absolute medians agree within 1.5% —
**no regression attributable to the indirection** on this workload.
Two observations recorded, not findings against §38: (1) today's
machine state measures ~1.5× where the committed §11 record says
1.05× — an environment-level discrepancy that predates §38 (both
sides of the A/B show it equally); (2) `a22`'s only mutable module
global is the LCG seed, so this benchmark has limited sensitivity
to global-access cost; a global-heavy microcase is worth adding
only if evidence demands it.

## Round 2 result

(pending)
