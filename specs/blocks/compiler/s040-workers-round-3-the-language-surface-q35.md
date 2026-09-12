<!-- §40 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 40. Workers round 3 — the language surface (Q35)

Owner decision 2026-08-02. The script-facing surface is
`stdlib.md` §16 (ambient `tsc`-probed, tracking); this section is
the checker/lowering contract. The example requirement is the
owner's, verbatim intent: a real parallel computation across
worker threads, not a messaging demo.

### 40.1 Checker

`Worker<In, Out>`, `Inbox<T>`, `Outbox<T>` are built-in generic
reference types, monomorphized per message-class pair like other
generics. Enforced, each with a corpus pin:

- `Worker.spawn(entry)`: `entry` is a directly named module-level
  synchronous non-capturing function of the exact entry shape. A
  capturing lambda rejects (r106 — stock `tsc` accepts it, so the
  pin is `tsc`-clean, recorded in its header); an `async` entry
  rejects (r107 — also expected `tsc`-clean via `void`-return
  assignability, verify and record).
- Message classes are transferable per stdlib §16.2; a string
  field rejects (r108, innermost-field diagnostics per the §32
  precedent).
- Context-affinity: a `Worker`/`Inbox`/`Outbox` module global,
  class field, array element, or lambda capture rejects (r109 pins
  the module-global case; one pin for the class, the checker
  covers all four escape positions with unit tests).
  *(Amended 2026-08-02, arc Clean Review: "array element" was
  implemented for the three array positions but not for container
  type arguments — a module-global `Map<i32, Worker<…>>` stored
  and retrieved a live worker, verified end-to-end. The rule as
  now written: an affine type is illegal as ANY container type
  argument — `Map` key and value, `Set` element, alongside the
  array positions. r111 pins the Map-value case; unit tests cover
  the rest.)*
- `new Worker(...)` rejects with the checker's own diagnostic
  (r110; `tsc` also rejects via the private constructor — not a
  `tsc`-clean pin, recorded as such).

### 40.2 Lowering (both tiers)

`Worker.spawn` lowers to `subscript_rt_worker_spawn` with the
program's module initializer, the monomorphized entry (adapted to
the C entry ABI), and the two payload sizes from the message
classes' C layout. Methods lower to the §39 C API;
`wait`/`poll` results are the runtime's materialized
Context-owned instances, typed as the message class; `null` maps
from the API's empty/closed results. Byte-identical across tiers
under the standing gate.

Hot reload: a swap is **Refused** while the session's Context has
live workers (reload-layer unit test; §8.2's staleness rationale —
worker threads hold the old code).

### 40.3 Corpus, example, docs

Accept: `a112-worker-echo` — one worker echo round-trip, all
printing from `main` (worker prints nothing), deterministic golden
under both tiers. `a113-worker-parallel` — two workers computing
disjoint chunks; `main` collects and prints per-worker results in
worker order after joins (interleaving never observable in the
golden). Reject: `r106`–`r110` per §40.1.

Example (owner requirement 2026-08-02): `e11-parallel-workers.ts` —
four workers each counting primes in a disjoint range of one
problem; `main` posts one range per worker, collects, prints
per-worker counts in worker order plus the total. The golden
carries no timing; the examples README row tells the reader how to
observe the parallelism (`time` on the CLI run, worker count
visible in the source). The computation is deliberately simple;
the parallelism must be real (concurrent computing workers, not
sequential round-trips).

Prelude gains the §16.1 ambient verbatim; generated-docs
regenerate (the language-reference gains a Q35 block; corpus-index
picks up the new entries).

### 40.4 Exit criteria (pre-registered)

1. `a112`/`a113` byte-identical under both tiers; `e11` runs under
   the examples harness in both tiers with its committed golden.
2. `r106`–`r110` pin (code, line); `r106` verified `tsc`-clean and
   recorded; `r107`'s `tsc` status verified and recorded either
   way.
3. Checker unit tests cover all four escape positions and the
   non-entry spawn arguments.
4. Reload refusal with live workers (unit test). *(Added
   2026-08-02, arc Clean Review m-3: plus a reload-mode worker
   echo round-trip — the reload-only init branch in the worker
   entry path runs end-to-end, not only the Refused pin.)*
5. `tsc` gate green with prelude + new corpus + `e11` included; no
   existing golden moves; full gate green; zero-warning sweep
   green; generated-docs byte-compare gates green.
6. *(Added 2026-08-02, arc Clean Review m-1)*: the reload-session
   fn-table pointer crosses to worker threads laundered as
   `usize`; the invariant that makes it sound (LiveWorkers refusal
   plus ReloadSession field order joining workers before code is
   dropped) is written as a `// SAFETY:` comment at the crossing
   and pinned by a comment-presence assertion or field-order
   test — the §39.3 SAFETY rule applies to laundered crossings,
   not only to literal `unsafe impl`.
