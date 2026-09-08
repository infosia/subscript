# §94 — host-driven async continuations

Status: landed 2026-09-09. Contract dated 2026-09-08.
Orchestrator: Codex, assigned by the owner. Coding agent: Opus.
Contract: `specs/blocks/compiler.md` §94, §68.7.4, and C8/C16.
Baseline pin: `d8cc19c34ac5e2db27bdc0411d5210924528029b`.

## Evidence

Opus measured B1 and B2 on aarch64 macOS with rustc 1.95.0,
Apple clang 21.0.0, Node v24.18.0, and TypeScript 5.9.2.
Both rounds restored their production edits. Neither round committed code.
The local REPORT holds the full sources, commands, traces, and artifact inventory.
This record preserves the contract's evidence without machine-specific paths.

B1's prototype patch SHA-256:
`2ba089563adaf422c8355f529a6565db96836906c2669d1e7f6d04f54dd11229`.
B2 restored that exact patch. Codex verified the hashes and source restoration.

Eight Node comparison rows contain seven distinct shapes; p02 duplicates a185.
Five baseline rows differ, representing four distinct shapes. Both prototype
tiers match Node in all eight rows. This measures the supported subset only.

The prototype uses FIFO ready jobs, next-checkpoint parked frames, and
per-handle continuation lists. Every await suspends. Await never drives
the child. Multiple awaits reuse the completion cache.

The 1,000-iteration no-print ownership loop ends with zero live Context
payload bytes on both tiers. This does not measure all Rust allocations.
Explicit collection during a drain preserves the measured frames and values.
Traps preserve kind and source position; their execution order can move
earlier or later when the continuation moves to a checkpoint.

A1 registered held frames but retained two drivers. B1 replaces those
drivers with continuation registration. Its measured order differs from A1.

## B2 closes the review findings

- Ready async root: reload, step, repeated trapped step, clear, and step
  again preserve the stale trap and produce no post-await effects.
- Parked root: the existing async reload test passes. A counted control
  reports the trap at the `Context.suspend()` position.
- Blocked parent: its parked child traps first after reload. The parent
  remains blocked. This does not execute the parent's own stale check.
- Mixed queues: both creation orders start with ready=1 and parked=1.
  Ready effects precede promoted parked effects on both tiers.
- A deliberate prepend mutation reverses those effects and fails both
  focused tests. Restoring FIFO append makes both tests pass again.

Codex reproduced the saved q01/q02 C-host traces and verified nine restored
source files. Reload evidence also includes the test bodies and command logs.
The independent interpreter did not participate in either prototype.

Rule: measure the queue state a claim names before attributing an order to that state.

## Contract choices

§94 retains the measured FIFO checkpoint and permits multiple continuations
per frame within one drain. There is no job budget. A settled infinite
chain can prevent a checkpoint from returning; hosts control when it starts.
`Context.suspend()` waits for the next checkpoint.

`async_pending` counts ready plus parked work. `async_unfinished` separately
counts invocations without completion. A deadlocked wait graph can therefore
have pending zero and unfinished positive. No new trap is added.

C16 retires by contract. C8 keeps the host boundary and restricted Promise
surface. Traps remain Context traps, not rejections. Call-time prefix behavior
and the required-await checker restrictions remain.

## Cost

Ship C, release runtime, same host, three warmups and eleven timed iterations:

| Workload | Baseline medians | Prototype medians | Ratio |
|---|---|---|---|
| 200,000 completed-handle awaits | 10.283, 10.300 ms | 27.098, 27.968 ms | 2.64–2.72x |
| 20 rounds of 2,000 held handles | 3.780, 3.852 ms | 8.900, 8.937 ms | 2.31–2.36x |
| 400 chains of depth 200 | 5.749, 5.702 ms | 15.880, 16.291 ms | 2.76–2.86x |

The prototype allocates result storage per nonvoid scheduler resume.
No alternative implementation was measured. These ratios are implementation
costs, not lower bounds for JS-compatible order.

§94 sets a 3.0x per-workload implementation acceptance cap on this platform.
It is set after these measurements and before permanent implementation.
The existing non-async performance gate also passed under B1.

## Authorized existing golden changes (§2)

Three accept goldens move. No trap golden moves. No `.expected` outside this list differs.

`corpus/accept/a154-held-async-handle.expected`

```text
baseline : "main:start\nwork1:start=0\nmain:between\nwork2:start=1\nmain:held\nwork1:resume=3\nmain:first=13\nwork2:resume=3\nmain:second=23\n"
prototype: "main:start\nwork1:start=0\nmain:between\nwork2:start=1\nmain:held\nwork1:resume=3\nwork2:resume=3\nmain:first=13\nmain:second=23\n"
```

Both children resume before the parent reads either result. Every value is unchanged.

`corpus/accept/a155-async-handle-array.expected`

```text
baseline : "start=1\nstart=2\nstart=3\nheld=3\nresume=1\nvalue=10\nresume=2\nvalue=20\nresume=3\nvalue=30\ntotal=60\n"
prototype: "start=1\nstart=2\nstart=3\nheld=3\nresume=1\nresume=2\nresume=3\nvalue=10\nvalue=20\nvalue=30\ntotal=60\n"
```

`corpus/accept/a185-async-settled-await-order.expected`

```text
baseline : "start1\nleaf\nend1\nstart2\nleaf\nend2\nouter:start\ninner\nouter:end\nmain:mid\n"
prototype: "start1\nleaf\nstart2\nleaf\nend1\nend2\nouter:start\ninner\nmain:mid\nouter:end\n"
```

**The prototype bytes are the two `node-order` lines already written in `a185`'s own header.**
The entry that exists to record the divergence now matches the order it records.


No existing trap expected file moves. The LIR snapshot changes only for
async instruction streams and added cases; unrelated blocks remain identical.
New corpus outputs derive from §94 and must fail against the baseline pin.
Node remains a divergence detector. The permanent three-witness gate is required.

## Implementation and review

Not yet implemented. No final gate or implementation review result exists.
The next round ports the form, both tiers, and independent interpreter,
adds permanent corpus and host tests, and returns a reviewable diff.

## Landing, 2026-09-09

The implementation landed on top of pin `d8cc19c`. Two review rounds
ran before it: round 1 raised two MAJOR findings and one MINOR, and
round 2 fixed and measured every one.

Round 1's MAJOR findings, and their fixes:

- **The interpreter leaked every frame in a blocked wait ring.**
  `register_continuation` held strong references in both directions,
  so two frames that hold each other's handle keep each other alive.
  The class is wider than the two scheduler edges: a suspended
  frame's own saved state holds the handles the program gave it.
  `Interpreter::release_scheduler_storage`, called from a new `Drop`,
  releases scheduler storage transitively, with no continuation run
  and no collector. Five permanent tests: three cycle shapes, a
  completed-program control, and a queued-work control.
- **The cost measurements were invalid.** The cause was the warm-up,
  not allocation noise: three iterations is 12 ms against §9's 200 ms
  floor. With a measured warm-up floor every spread falls below 20%.
  Two sequential pairs give 2.73x / 2.35x / 2.56x and 2.71x / 2.31x /
  2.55x, inside §94.4's 3.0x cap.

Counts after the landing: accept `.ts` 191, `.expected` 192, reject
`.ts` 175, trap `.ts` 55. New entries: a188 to a193 and t55.

Gate verdict at the landing state:

```
gate full d8cc19c34ac5e2db27bdc0411d5210924528029b dirty:45 debug 1339/0/2 release 1337/0/2 skips 2/0 clippy 7/18/13 goldens-moved 4 exit 0
```

Record: `target/gate/20260908T192841Z-full.md`.

The four moved goldens are the three §94.3 authorizes plus the
aggregate LIR snapshot:

| File | Change |
|---|---|
| `a154-held-async-handle.expected` | `work2:resume=3` moves before `main:first=13` |
| `a155-async-handle-array.expected` | the three `resume=` lines precede the three `value=` lines |
| `a185-async-settled-await-order.expected` | `start2 leaf` precede `end1`; `main:mid` precedes `outer:end` |
| `codegen/tests/lir-goldens/corpus.txt` | async instruction streams and the new entries |

a185 now matches `node v24.18.0` and carries `js-comparable: yes`.
Its former C16 explanation and `node-order` header lines are gone.

## Phase Review, 2026-09-09

Two fresh no-context reviewers reviewed `d8cc19c..f86bcd1`, one on the
runtime and the two compiled tiers, one on the interpreter, the tests,
the corpus, the benchmarks and the docs. They raised two CRITICAL and
three MAJOR findings.

CRITICAL: a cleared trap replays the continuation, because the frame's
saved state names the earlier suspension (§94.2 now states the rule);
and the interpreter aliases a running frame, because `execute_coroutine`
keeps a `*mut Frame` into a `RefCell` that `AsyncHandleRetain` and
`register_continuation` borrow while the frame runs.

MAJOR: two comments state the opposite of §94.1 rule 11; the
`ReloadSession` teardown test observes nothing after its drop, so it
cannot fail for the property it names; and §94.3's self-await control
has no host test with the `unfinished` observer.

### Confirmed by the review, not only claimed

- Every §94.3 required case is covered except the self-await control.
- All ten changed or new corpus entries are Red at pin `d8cc19c`.
- `a185`, `a188`, `a189` and `a193` match `node v24.18.0` byte for byte.
- The interpreter calls no `Context::async_*`; its scheduler is its own.
- Removing the interpreter's `Drop` makes three teardown tests fail, so
  the lifetime fix has a firing control.
- The LIR snapshot went 1,139,176 → 1,260,829 bytes, 27 → 33 blocks.
  `a185` is the only changed pre-existing block, and its change is line
  and column shifts from the two deleted `node-order:` header lines. No
  instruction differs. Every other block is byte-identical.
- Async cost reproduced from the committed driver, ship C, release,
  two sequential pairs:

| Workload | Baseline median | New median | Ratio |
|---|---|---|---|
| settled-awaits | 10.640 / 10.590 ms | 29.486 / 28.304 ms | 2.77 / 2.67 |
| held-handles | 4.014 / 4.025 ms | 8.976 / 9.078 ms | 2.24 / 2.26 |
| deep-chains | 6.368 / 6.203 ms | 16.595 / 16.070 ms | 2.61 / 2.59 |

  Spreads 1.7% to 5.7%, every ratio under §94.4's 3.0x cap. Context
  payload at release is identical on both revisions, so §94 adds no
  retention.

### MINOR findings, open

Recorded here, not fixed in the correction round.

1. Seven comments cite a measurement round instead of the contract, and
   their rule numbers are the round's: `cemit.rs` 3887, 4031, 4097;
   `lower/func.rs` 6720, 6732, 6858, 6924, and 6882's "before this
   experiment".
2. Stale doc text on `Context::async_kick` and `async_step`, and the
   `poll-driven async roots` section header in `runtime/src/context.rs`.
3. A dead second staleness check at `lower/func.rs:7026-7030`.
4. `emit_async_handle_stale_check` emits nothing; the name says it does.
5. Three dead parameters kept alive by a discard in
   `await_async_child` and `await_async_handle`.
6. The LIR form carries the stale-trap site for `AsyncHandle` only,
   while the consumer derives it for `Async` and `AsyncCall`.
7. `subscript_rt_ctx_async_unfinished` has no behavioural test; nothing
   gated calls it.
8. The host-header generator gained a ninth hand-listed function name
   beside a generic loop that already covers them.
9. `a189` shares a reference value, not an aggregate one, where §94.3
   names both.
10. `docs/tutorial-typescript.md:429` still says "A pending `await`
    suspends"; every await suspends now. Line 518 describes
    `async_unfinished` more narrowly than §94.2 defines it.
11. The interpreter's `run()` loop has no progress guarantee on the
    branch where `async_step` returns `Ok(())` while trapped.
12. `kick`'s `exports` parameter in `codegen/tests/async_checkpoint.rs`
    is dead at all six call sites, so `call_export` is unexercised there.

## Correction round, 2026-09-09

Both CRITICAL findings and all three MAJOR findings are fixed at
`4dcbd98`. The twelve MINOR findings stay open.

The round stopped once before implementing, on a contract
contradiction it was right to raise: §94.2 counted a trapping job in
`async_pending` while the Context stayed trapped, and the new rule
excluded a stopped frame from that count. The same frame met both
descriptions, and `async_checkpoint.rs` asserts `pending == 1` right
after the trap. `610fd75` names the transition point: the stop happens
at `clear_trap`, not at the trap.

**The cleared-trap replay.** `Context` records the trapping frame and
its trap kind at the ready head. `clear_trap` moves that frame into
`async_stopped` unless the kind is `StaleCoroutine`. Measured on both
tiers, five clear-and-step rounds, three shapes:

| Shape | Output | Live allocations | Unfinished | Trap position |
|---|---|---:|---:|---|
| Resumed body | `m1 m2` | 4 | 1 | 6:14 |
| Started callee | `m1 m2 boom:start` | 6 | 2 | 4:14 |
| Resumed completed direct await | `m1 m2` | 4 | 1 | 7:14 |

No further output, no allocation growth, no unfinished growth, and no
stale-child `Internal` report. The successful control prints a third
line, so a later checkpoint can still run a continuation in that
shape.

**The interpreter aliasing.** Execution storage is now a separate
`Rc<RefCell<Frame>>`. `execute_coroutine` holds no raw pointer and no
`unsafe`, and a reentrant borrow reports an internal `InvalidLir`
rather than panicking, which core principle 5 requires.

**Verified here, not only reported.** The reload staleness tests pass
unchanged, so the exception holds. The two interpreter tests for the
reaching paths pass. Reverting the clearance transfer makes
`ship_c_cleared_continuations_never_replay_or_leak` fail on its
pending assertion, so the test fires.

Gate verdict for the correction round:

```
gate full 610fd75d8aeb8b9d3dc755b13092ff051b383e05 dirty:8 debug 1347/0/2 release 1345/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0
```

Record: `target/gate/20260908T214015Z-full.md`. No golden moved.
