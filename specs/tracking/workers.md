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

## Round 3 surface probes (run 2026-08-02, before §39)

Stock `tsc` (`node_modules/.bin/tsc --strict --target es2022 --lib
es2022`, with `prelude/lang.d.ts`) accepts the planned ambient —
`Inbox<T>`/`Outbox<T>`/`Worker<In, Out>` with private constructors
(the `JsonResult` precedent) and `static spawn` on its own type
parameters — exit 0 on both probe programs: the annotated
echo round-trip, and the inference form (`const w =
Worker.spawn(entry)` with no type arguments). `lib` is ES2022 only,
so the DOM `Worker` name is not in scope and the name is free.

## Round 2 result (2026-08-02, landed)

Landed per §39: `runtime/src/worker.rs` (new module — threads,
queues, endpoints, lifecycle; all synchronization confined here,
`Mutex`+`Condvar`, no spinning, and **no `unsafe impl Send/Sync`
needed** — entry/init cross as `extern "C" fn` pointers and
payloads as owned byte buffers), `context.rs` (parent ownership;
release-time close→join→free), `ffi.rs` (eight public
`subscript_rt_worker_*` functions), `trap.rs`
(`WorkerTrapped = 22`), `host_header.rs` + regenerated
`subscript_runtime.h` (whole worker API is public-header, with
opaque worker/inbox/outbox types and the two function-pointer
typedefs).

Implementer decisions recorded: worker Contexts inherit the
parent's allocation tier; join is repeatable and repeats the
recorded outcome; `close` drains already-queued input before
end-of-input; parent teardown closes all workers before joining;
join-trap carries the worker's trap detail in the joining Context's
message.

Reviewer verification: gate 48 harnesses, 817 passed, 0 failed,
exit 0 read directly; `tsc` exit 0; corpus/goldens untouched;
header regenerated through its generator with the byte-compare
gate green. Eight new tests including the C-ABI echo round-trip,
trap-kind-22 propagation on join, two concurrent workers with
per-worker set assertions only, and parent release with a live
blocked worker.

## Round 3 result (2026-08-02, landed — arc complete)

Landed per §40 across 24 files: built-in `Worker`/`Inbox`/`Outbox`
generics monomorphized per message-class pair; entry, transferable,
affinity (all four escape positions unit-tested), and `new`
rejections; both tiers lower onto the §39 C API (payload sizes from
C layout, materialized nullable results); `ReloadSession::reload`
refuses with `LiveWorkers` while workers are live; prelude gains
the §16.1 ambient; generated docs regenerated with a Q35 block.

Corpus: `a112`/`a113` byte-identical under both tiers;
`r106`–`r110` pin code and line. `tsc` findings recorded in the
headers: r106 **and** r107 are `tsc`-clean strictly-narrower pins
(stock TS permits a Promise-returning function in a
void-callback position), r110 is `tsc`-rejected too (TS2673,
private constructor).

Example `e11-parallel-workers`: four workers, ranges posted first,
joins after, per-worker counts printed in worker order. The golden
is externally checkable: 1229/1033/983/958, total 4203 = π(40000),
matching the known prime-counting values.

Reviewer verification: gate 48 harnesses, 823 passed, 0 failed,
exit 0 read directly; `tsc` exit 0; golden sweep 113 entries, 0
skipped; zero-warning sweep green; no existing golden moved.
**Parallelism verified physically**, not structurally: a scaled
probe (ranges ×100, π(4 000 000) = 283 146, matches the known
value) measured user CPU 0.63 s against real 0.22 s — 2.9× wall
utilization across the four workers under the dev tier via
`subscript run`.

The arc is complete: §38 `1314d9d`, §39 `afd5b82`, §40 in this
commit. A no-context review of the cumulative arc diff follows as
the closing step.

## Clean Review Then Fix (2026-08-02, arc closed)

A fresh no-context reviewer read `1314d9d^..0788f4e` against
§38–§40 and stdlib §16 and ran both tiers. Findings: 1 CRITICAL /
1 MAJOR / 3 MINOR, all fixed same-day (contract `9a604dc`, fix
commit below); the reviewer separately verified the worker
synchronization, lifecycle interleavings, trap propagation, payload
ownership, the four escape positions, and entry-shape bypasses as
clean.

- **C-1 (CRITICAL, fixed).** The ship tier emitted every capturing
  lambda's environment as a function-local mutable `static` —
  §38.1-forbidden process-wide state the name-based §38.2-2 test
  missed. Wrong before Workers in plain programs: create a
  capturing lambda, recurse, call it — dev 3 / ship 0, reviewer-
  and implementer-reproduced (implementer Red run: dev `3`,
  ship `0`, exit 101). C5 non-escape makes automatic storage
  sound; `a114-lambda-env-recursion` pins the pattern under both
  tiers and the emitter test is now an any-mutable-static audit
  (immutable const tables whitelisted). Lesson, once: **a
  name-based absence test pins an instance; only a class-wide
  audit pins the class** — the same lesson as the §11c.3 copied
  guard, in test form.
- **M-1 (MAJOR, fixed).** Context-affinity missed container type
  arguments: a module-global `Map<i32, Worker<…>>` stored a live
  worker (reviewer ran it end-to-end; no memory unsafety, checker
  hole only). Rule restated as any container type argument (Map
  key/value, Set element); `r111-worker-in-map-value` pins
  (`tsc`-clean), unit tests cover the rest including local
  containers.
- **m-2 (fixed).** `subscript_rt_globals_init` conversion-failure
  arms now trap before returning null (both arms unit-tested;
  reachable only off 64-bit hosts).
- **m-3 (fixed).** Reload-mode worker echo round-trip added; the
  reload-only worker-init branch now runs end-to-end.
- **m-1 (fixed).** The fn-table `usize` laundering across the
  spawn boundary carries `// SAFETY:` comments at both crossings,
  and the load-bearing `ReloadSession` field order (Context before
  JIT modules, so workers join before code drops) is mechanically
  guarded. Correction to Round 2's record: "no `unsafe impl
  Send/Sync`" was true but incomplete — a raw pointer did cross
  threads, as a `usize`; the SAFETY rule now explicitly covers
  laundered crossings (§40.4-6).

Post-fix state: gate 48 harnesses, 828 passed, 0 failed, exit 0
(reviewer-run, direct); `tsc` exit 0; differential gate 114
entries, 0 skipped; zero-warning sweep green; no pre-existing
golden moved. No open findings; the arc is closed.
