# Sandbox tier — the proposal and the measurement round (2026-09-16)

Status: measurement round. No owner decision exists. Nothing here
changes a rule.

## The proposal

A host that runs user-authored content (mods, shared levels, plugins)
runs scripts it did not write. Design invariant 6 ("Scripts are
trusted") excludes that host. The proposal adds a third execution
form for untrusted scripts and changes nothing in the two trusted
tiers.

The candidate is the reference interpreter in
`codegen/src/interpreter.rs`. It consumes ordered LIR, uses the
shared runtime Context, and reports traps through the shared
`TrapKind`. Its module comment names it "a test oracle for the
shared lowering, not a shipped execution tier". The proposal makes
it the **sandbox tier**, with a **sandbox profile** that the checker
enforces at compile time and the interpreter enforces at run time:

| Part | Rule |
|---|---|
| compile-time | reject `Context.free`, `Context.fromBytes`, `Context.bytesOf` on a layout with a handle or a string, `Worker.spawn`, `Inbox`, `Outbox`; reject every mirror declaration outside the capability mirror; source-size and nesting limits |
| run-time | allocation quota per Context; interrupt flag polled at back-edges and calls; call-depth limit; string and array size limits |

The full proposal text is the handoff of 2026-09-16. Its claims
about Luau are *(docs)*.

## Review findings before measurement

Read at `aebef92`.

1. **Foreign calls are not designed.** The interpreter returns
   `Unsupported` for every `CallTargetKind::Foreign`. Of the 56
   accept entries with an `interpreter:` exclusion header, about 50
   name the synthetic native interop library. A capability model
   that filters mirror symbols needs a way to call the symbols it
   keeps. Two candidates: libffi, or a C trampoline table that
   `subscript bind` generates per mirror. The second keeps invariant
   4 and adds no dependency.
2. **The interpreter does not mediate memory.** `AddressTarget::Pointer(*mut u8)`
   reads and writes Context memory directly. Safety rests on
   verified LIR and the runtime's bounds checks, the same as the JIT.
   The profile rules carry the safety argument. The interpreter is
   the candidate for three reasons: it runs where a JIT is
   forbidden, the interrupt poll lands in one backend, and its
   trusted computing base is one module plus the runtime.
3. **The runtime holds no compiler.** A user-content host loads
   source at run time, so it ships the checker, the lowering, and
   the interpreter. `codegen` depends on Cranelift with no feature
   gate.
4. **`interpreter.rs` is 6,930 lines.** §5.y allows 2,000. A shipped
   tier must split it.
5. **No speed number exists.** The interpreter uses `HashMap` and
   `Rc` per instruction step.

Open questions from the proposal, with the recommended answer:

| Question | Recommendation |
|---|---|
| Is `Context.collect()` callable from a sandboxed script? | Yes. A rejection adds no safety. The interrupt flag bounds CPU. |
| Does the profile accept `async` and coroutines? | Yes. Both suspend at the host checkpoint that the poll covers. |
| Is the speed acceptable? | Decided after M1. |
| Does the shipped host load source at run time? | This is a requirement, not a question. Without it the tier has no use. |

## Pre-registered measurements

Each measurement is a number the decision needs. The round records
every number with the pin, the profile, and the command. The round
reverts every production change (CLAUDE.md, "Step 0").

| Id | Measurement | Decides |
|---|---|---|
| M1 | Interpreter time on the 10 benchmark workloads, beside the dev-JIT time, same method as `benchmarks/src/bin/cross-language.rs` | whether the tier is a product or a stepping stone |
| M2 | M1 again with an atomic interrupt flag polled at every back-edge and every call | the cost of the interrupt rule |
| M3 | Release binary size of one executable that links checker + lowering + interpreter, with and without Cranelift in the link | the portability claim, criterion 7 |
| M4 | Count of accept entries the interpreter runs, and each excluded entry's reason, from a full release sweep | the size of the foreign-call gap; the §85 row says 125 entries, the headers say 176 |
| M5 | Interpreter time per entry over the full sweep, the ten slowest | where the interpreter's cost sits |

Rule the prototype contradicts: the module comment of
`codegen/src/interpreter.rs` ("not a shipped execution tier"). The
prototype does not land.

## Results

Measured at `16b63c6`, `--release`, macOS arm64, `rustc 1.95.0`. The
coding agent's report is the round's record; the logs and the two
reverted patches are under `target/sandbox-probe/` on the host that
ran the round. Every production change was reverted; `git status`
on the reverted tree is empty.

### M1 — interpreter time on the benchmark matrix

One child process per workload. The dev-JIT and the interpreter use
the `cross-language.rs` method (warm-up 3, floor 200 ms, 11 timed
runs). A workload whose first interpreter run exceeds 60 s uses 1
warm-up and 3 timed runs; the sample spread of that group is 1.03%
to 2.53%.

| Workload | JIT median | Interpreter median | Ratio |
|---|---|---|---|
| callbacks | 0.461 s | 101.282 s | 219.5x |
| collect | 0.119 s | 1.792 s | 15.0x |
| fib-loop | 0.073 s | 77.390 s | 1054.3x |
| fib-recursive | 0.008 s | 2.498 s | 300.3x |
| mandelbrot | 0.133 s | 87.328 s | 654.4x |
| particles | 0.477 s | 486.104 s | 1018.9x |
| primes | 0.033 s | 21.960 s | 664.4x |
| queen | 0.036 s | 14.789 s | 413.9x |
| sort | 0.035 s | 17.675 s | 509.6x |
| tree | 0.417 s | 27.402 s | 65.7x |

The median ratio is 437x. 10 of 10 stdouts match the dev-JIT byte
for byte. `check_program` plus `lower_module` is under 1 ms per
workload. The two lowest ratios, `collect` and `tree`, are the two
workloads whose work is runtime allocation and string building,
which runs in `subscript-runtime` in both tiers.

### M2 — the cost of an interrupt poll

One `AtomicBool` on the interpreter, one relaxed load at the top of
`call_function` and at every back-edge in `take_edge`. Measured on
the six workloads whose M1 run is under 30 s; the other four were
skipped because a repeat adds nothing at their ratios.

| Workload | No poll | Poll | Change |
|---|---|---|---|
| collect | 1.792 s | 1.788 s | -0.25% |
| fib-recursive | 2.498 s | 2.529 s | +1.26% |
| primes | 21.960 s | 22.053 s | +0.42% |
| queen | 14.789 s | 14.823 s | +0.23% |
| sort | 17.675 s | 17.749 s | +0.42% |
| tree | 27.402 s | 27.000 s | -1.46% |

The change is inside the M1 sample spread (0.13% to 5.66%) on every
workload. The poll has no cost above the noise.

### M3 — binary size without Cranelift

A `jit` cargo feature, default on. The feature cannot gate `jit.rs`
alone: `layout.rs` names `cranelift_codegen::ir::types` in
`Repr::Scalar`, and `cemit.rs`, `root_storage.rs`, and `lower/` use
`layout`. The prototype gates `jit`, `lower`, `reload`, `native`,
`layout`, `cemit`, `emit_files`, `root_storage`, and `ship`. The
feature-off crate keeps `lir`, `lir_types`, and `interpreter`.

| Executable | Bytes | Cranelift symbols |
|---|---|---|
| probe, default features | 10,221,504 | 1,398 |
| probe, `default-features = false` | 7,137,008 | 0 |
| `target/release/subscript` at the pin | 11,135,024 | not measured |

The delta is 3,084,496 bytes (30.2%). It covers Cranelift and the C
emitter together. A Cranelift-only figure needs `Repr::Scalar` to
stop naming a Cranelift type.

### M4 — the entries the interpreter runs

Full release sweep on the reverted tree: 233 entries, 177 runnable,
177 matched, 56 excluded. Verdict `test result: ok. 1 passed; 0
failed`, 183.5 s.

| Cause of exclusion | Count |
|---|---|
| foreign call into the synthetic native interop library | 50 |
| runtime worker adapter | 3 |
| host-supplied entry argument | 2 |
| host pre-entry and post-run hooks | 1 |

50 of 56 exclusions are the foreign-call gap: 21% of the accept
corpus. The §85 row's 125 was correct at `3677d1f` (181 entries, a
hard-coded exclusion table of 56). The corpus grew by 52 entries
since, and the test now derives the count from the `interpreter:`
headers. 176 counts only single-file entries; `a19-modules` is a
directory entry with no exclusion header.

### M5 — the ten slowest entries

One `interpret()` per runnable entry, timed alone.

| Entry | `interpret()` |
|---|---|
| a22-matrix-propagation | 183.744 s |
| a24-particle-system | 0.887 s |
| a204-static-long-string | 0.067 s |
| a23-game-loop | 0.036 s |
| every other entry | under 1 ms |

Total 184.741 s over 177 entries; median 26 µs. One entry is 99.46%
of the sweep, and the sweep's worker pool cannot go below its
longest entry, so `a22-matrix-propagation` sets the release sweep's
wall time.

## Assessment

1. **The reference interpreter is not a product tier.** A median
   437x against the dev-JIT puts it about two orders of magnitude
   behind the position Luau occupies *(docs)*. The proposal's main
   argument, "it already exists", does not survive M1. A sandbox
   tier on a no-JIT platform needs a new bytecode VM. That is a
   phase the size of the JIT tier, and no host has asked for it.
2. **The safety rules are cheap and tier-independent.** M2 shows an
   interrupt poll at zero measurable cost. The review finding that
   safety rests on the profile rules, not on the interpreter, means
   the same rules lower into the JIT tier for the same price.
3. **The foreign-call gap does not exist on the JIT.** The JIT tier
   already calls the mirror. On the interpreter it is 50 entries of
   new work.
4. **The interpreter stays the third witness.** M4 shows it agrees
   with the goldens on every entry it runs. Nothing here changes
   that role.

Recommendation: do not revise invariant 6 on the interpreter's
basis. If sandboxing is wanted, the evidence supports a **sandbox
profile on the dev-JIT tier** (the compile-time rules, the interrupt
flag, the quota, the depth limit, and the capability mirror), on
platforms where the JIT runs. A no-JIT sandbox waits for a host that
needs it, with M1 as the number it must beat.

**Owner decision, 2026-09-16.** The sandbox is a compile profile on
the existing tiers (CLAUDE.md invariant 6, plan Rev 3, contract §109).
The reference interpreter is not promoted.

Two findings outside the proposal:

- `Repr::Scalar` in `codegen/src/layout.rs` names a Cranelift type,
  so the layout module cannot build without Cranelift. Layout is a
  C-ABI fact (invariant 1), not a Cranelift fact.
- `a22-matrix-propagation` is 99.46% of the release interpreter
  sweep's time. If the sweep's wall time matters, that entry is the
  whole cost.

## P26 round 1 — compile-time rules (landed)

Contract pin `d5b9519`. Implementation commit: the one after it.

| Item | Result |
|---|---|
| Red | at the pin, `subscript check --profile sandbox` on each reject entry exits 2, "unknown option `--profile`"; `check` exits 0 |
| Green | S023 `Context.free`, S024 `Context.fromBytes`, S025 `Inbox`/`Outbox`/`Worker.spawn`, S026 depth 257; each with a default-profile firing control |
| Corpus | `r233` to `r236`; twins `a235` to `a237`, goldens by inspection, confirmed on both tiers; `tsc: accepts` measured on all seven |
| Gate | `gate quick d5b9519 dirty:28 debug 1474/0/2 skips 2 goldens-moved 0 exit 0` |

Two contract corrections came out of the round, both in §109.2's
amendment: S019 was a retired code (§99.3), and the `bytesOf` clause
had no program because the default profile rejects the layout.

The checker thread: the parser and the checker recurse once per
nesting level. A 2 MiB debug test thread overflows at depth 66. The
64 MiB checker thread carries depth 2,000 and overflows at 2,500. The
default profile keeps no limit, so a trusted program past that depth
aborts the process instead of reporting. Recorded; no change
proposed.

S026 reports at the bracket that takes the whole-file depth over 256,
so `r236`'s 256th parenthesis fires because the function body's `{`
is open. That is the contracted over-approximation.

The round 1 gate under the coding agent's environment failed at the
build step on an `xcrun` cache write denial; the coordinator's
environment did not reproduce it. A dead-code warning in a shared
test module that the coding agent's clippy run did not cover was the
one real build failure; fixed by moving the helper to its caller.

## P26 round 2 — run-time rules (landed)

Contract pin `382de51`. Implementation commit: the one after
`c6251df`.

| Item | Result |
|---|---|
| Red | at the pin, `t57` printed `start` then `300` and exited 0; `t58` died on signal 4 with no stdout; `a238` printed its golden (no checkpoint existed) |
| Runtime | `Interrupted` 25, `AllocationQuota` 26, `StackBudget` 27; `subscript_rt_ctx_interrupt`, `_set_alloc_quota`, `_set_stack_budget`, `subscript_rt_sandbox_enter`, `subscript_rt_sandbox_poll`; header regenerated |
| LIR | `IntrinsicFamily::Sandbox` with `Enter` (every function body and every resume block) and `Poll` (every `while`, `for`, `for-of` header); nothing under the default profile, asserted by a two-profile diff |
| Executors | dev JIT, ship C, interpreter each lower both as the runtime call; the interpreter now brackets its entries with `enter_script`/`exit_script` |
| Runners | `RunConfig.profile`; every runner reads `hir.profile` and applies the defaults; the ship entry emits the two calls; `interpret_configured` |
| Corpus | `a238` `2997,8` on three tiers; `t57` `allocation-quota` at 16:17; `t58` `stack-budget` at 8:10 (the callee's entry); goldens-moved 1 (the LIR intrinsic table, two rows) |
| Interrupt latency | dev-JIT 22–43 µs; ship-C-AOT 32–36 µs; flag store to runner return, debug, arm64 macOS |
| Gate | `gate quick 382de51 dirty:41 debug 1501/0/2 skips 2 goldens-moved 1 exit 0` |

Two contract corrections after the round (`c6251df`): the
stack-budget trap site is the callee's entry, because `Enter` is the
first instruction of the body; and `live_bytes` must be a maintained
counter, because the first quota check folded over the live set and
measured quadratic (10,000 live: 1.09 s against 0.03 s; 20,000 live:
4.25 s against 0.02 s, debug `subscript run`). The counter is round
2b.

The limits are the host's facts, not the program's (§109.4 rule 5),
so LIR carries no limit and `interpret_configured` takes them from
the harness. `subscript emit` takes no `--profile`; a host that emits
C and links it sets the limits through the C API.

Exit criterion 5 has no entry point yet: `jit_bench` takes no option
record. Round 3.

## P26 round 2b — `live_bytes` is a maintained counter (landed)

Contract pin `c6251df`; §18 amended to agree (`0cd904a`).

| Item | Result |
|---|---|
| Sites | 12 moves across the exact-size and arena modes; eviction from the retained-dead queue moves nothing because retention already left the live set |
| Check | `debug_assert_eq!(live_bytes_by_walk(), live_bytes())` at the end of `collect()`; five tests compare the two after each kind of change; one firing control offsets the counter and asserts the unwind |
| Red | removing one maintenance site turned 8 and 11 tests Red |
| Measurement | debug `subscript run`, live allocations kept: 10,000 → default 0.01 s, sandbox 0.01 s (was 1.09 s); 20,000 → 0.01 s, 0.02 s (was 4.25 s); 100,000 → 0.06 s both; 200,000 → 0.11 s both |
| Gate | see the verdict line below |

`live_allocations` and `reserved_bytes` still walk. `context.rs` is
6,567 lines, on the §5.y split list.
Gate: `gate quick 28c8b538557895ea4a80da71e6e2ffb3e79daa28 dirty:1 debug 1507/0/2 skips 2 goldens-moved 0 exit 0`
