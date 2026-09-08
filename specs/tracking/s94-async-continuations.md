# §94 — host-driven async continuations

Status: contracted, implementation pending. Date: 2026-09-08.
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
