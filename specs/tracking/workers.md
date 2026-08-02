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

## Round 1 result

(pending)
