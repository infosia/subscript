# Q34 — async/await, poll-driven: evidence

Status: **landed and verified 2026-07-31** against `compiler.md` §26
and `collisions.md` Q34 (C8 revision). Origin: downstream request R4
(HANDOFF/REPORT exchange). Contract committed first; the
`tsc`-acceptance of the surface (ambient
`Context.suspend(): Promise<void>`, async chains, `await`
unwrapping) was probed against stock `tsc` before contracting.

**Design reference.** The HANDOFF appendix supplied a source-read of
Boa v0.21.1 (https://github.com/boa-dev/boa, `bc36c3fa`) *(docs —
source-read, not executed)*: its no-scheduler `JobExecutor` boundary
and evaluation-never-pumps separation validated R4.2 and were kept;
its reaction-driven promise machinery (job queues, microtask
ordering) was deliberately not copied — Q34's poll-driven awaitables
need none of it. Boa's undecided teardown semantics became Q34's
explicit "drop without continuations, no cleanup guarantee".

## Contract corrections during landing

The first draft left `a94` (multi-root concurrency) undrivable — the
two-tier gate invokes only `main`. The implementer stopped at the
mandated blocker instead of inventing a mechanism; §26.3 then gained
the standard-runner convention (`a658a0a`): runners invoke `main`,
then every other exported async function in declaration order, then
pump to quiescence — hosts kick whatever they choose.

## §26.5 evidence (reviewer-run)

1. `a93` (nested chain), `a94` (two roots, kick-order interleave),
   `a95` (foreign-poll await — absorbs the old Q1 request)
   byte-identical under dev-JIT and ship-C-AOT, plus the Cranelift
   object cross-check; goldens via the standard capture path. `a93`
   golden re-read locally and matched.
2. `r96`–`r100` all pin S013 (now "the unsupported Promise object
   surface"); `r100` (floating async call) is `tsc`-clean — the
   strictly-narrower pin. `r99` uses top-level `await` because SWC
   rejects `await` inside a non-async function at parse time, before
   any checker code runs (recorded so nobody "fixes" the entry).
   `r14-async` deleted — Q34 makes its construct legal.
3. Kick order pinned by
   `async_step_resumes_pending_roots_in_kick_order`; trapped-step
   no-op by `async_step_on_trapped_context_is_no_op`; reload
   staleness by `suspended_async_frame_is_stale_after_reload`;
   teardown drops suspended roots without running continuations
   (unit-tested), and a collecting root does not free later pending
   roots.
4. Prelude declares `Context.suspend`; the interop fixture gained
   deterministic `subDevicePoll` and its mirror was regenerated via
   `subscript bind`; `tsc` gate exit 0.
5. No previously tracked `.expected` changed; gate 48 harnesses,
   743 passed, exit 0, read directly; the CLI pump verified live by
   the reviewer (`subscript run` on an async program: three
   `Context.suspend()` rounds, then completion).

## Representation choices recorded (implementer, adopted)

Async rides the coroutine machinery unchanged in kind: HIR stores
the fulfilled type and erases `Promise<T>` after checking; an
awaiting frame stores its child-frame pointer and roots resume at
the innermost suspended child; pending roots live in a kick-order
queue in the runtime; exported async functions keep the Q12
zero-argument void ABI, hence `(): Promise<void>`.
