<!-- §26 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 26. Q34 — async/await, poll-driven

Owner decision 2026-07-31 (collisions.md Q34, C8 revision; downstream
request R4). The decision text lives in Q34; this section is the
implementation contract. The `tsc`-acceptance of the whole surface —
ambient `Context.suspend(): Promise<void>`, `async function` chains,
`await` unwrapping — was probed against stock `tsc` before
contracting.

### 26.1 Checker

`async function` declarations (module-level and exported) with
explicit `Promise<T>` return annotations; `await` legal only inside
them, applied to exactly the two Q34 awaitable forms. Rejected, with
corpus pins: `new Promise` (r96), any `.then`/`.catch`/`.finally`
call (r97), `Promise` statics (r98), `await` outside async (r99), a
floating async call statement (r100 — must be awaited). Async calls
are direct calls in await position; async function values are not
first-class (no storing/passing references to them as promises).
S013's meaning narrows accordingly: it now rejects the Promise
object surface, not the keywords; `r14-async` retires (file and
harness row removed — Q34 makes its pinned construct legal). *(§64,
2026-08-23: a generic async function with explicit type arguments
is also awaitable.)*

### 26.2 Lowering (both tiers)

*(Revised 2026-09-08 by §94, landed 2026-09-09.)*
Async frames use the continuation protocol in §68.7.4 and §94.
Every await suspends. A call starts its callee, but an await never
resumes that callee. Completion queues the caller's continuation.
`Context.suspend()` waits for the next host checkpoint.

### 26.3 Runtime C API and drivers

```c
uint64_t subscript_rt_ctx_async_pending(const subscript_rt_context*);
uint64_t subscript_rt_ctx_async_unfinished(const subscript_rt_context*);
uint64_t subscript_rt_ctx_async_step(subscript_rt_context*);
```

§94 defines the counts, checkpoint, traps, and teardown.
The standard runners invoke `main`, then other exported async
functions in declaration order, then step while `async_pending` is
nonzero. Embedding hosts choose their exports and checkpoint boundaries.
The runner stops on a trap. Quiescence does not imply that every
invocation completed; `async_unfinished` exposes that distinction.
No runner adds a deadlock trap or resumes blocked work at quiescence.

### 26.4 Corpus

`a93-async-chain` (accept): leaf async polling a counter via
`Context.suspend()`, middle async awaiting the leaf, async `main`
awaiting the middle; prints pin the resume ordering and final
values. `a94-async-two-roots`: two async exports kicked then pumped
by the harness, interleaving deterministically in kick order.
`a95-interop-async-await`: an async function awaiting a foreign
poll (the interop fixture gains a deterministic poll —
`subDevicePoll`-style, completing after a fixed call count — only if
no existing function fits); absorbs the Q1 request. Rejects
`r96`–`r100` as listed in Q34, `r100` `tsc`-clean (verified
standalone, recorded in its header). `r14-async` deleted.

### 26.5 Exit criteria (pre-registered)

1. `a93`–`a95` byte-identical under both tiers, driven by the same
   pump-to-quiescence entries the gate already uses.
2. `r96`–`r100` pin (code, line); `r100` type-checks under stock
   `tsc`; `r14` removed from tree and harness.
3. `async_step` determinism: a unit test pins the kick-order
   interleave; a trapped-Context step is a no-op (unit test).
4. Prelude declares `Context.suspend`; `tsc` gate green with the new
   accept entries included.
5. No existing golden moves (sync programs are unaffected by the
   pump); full gate green; zero-warning sweep unaffected.
6. Hot reload: a suspended async frame resumes stale per §8.2's
   existing trap (unit test at the reload layer).
