# Sandbox tier — the proposal and the measurement round (2026-09-16)

Status: the record of P26. It holds the proposal, the measurement
round, the owner decision of 2026-09-16, and every landed round.

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
| Runtime | `Interrupted` 25, `AllocationQuota` 26, `StackBudget` 27; `subscript_rt_ctx_interrupt` (replaced in the review-fix round by `subscript_rt_ctx_interrupt_handle` and `subscript_rt_interrupt_set`), `_set_alloc_quota`, `_set_stack_budget`, `subscript_rt_sandbox_enter`, `subscript_rt_sandbox_poll`; header regenerated |
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

## P26 round 3 — the cost runner, the adversarial sweep, the docs (landed)

Contract pin `af47450`. Gate:
`gate quick af47450 dirty:12 debug 1515/0/2 skips 2 goldens-moved 0 exit 0`.

### Criterion 5 — the cost of the profile

`sandbox-cost`, release, arm64 macOS, median of 11 after a 200 ms
warm-up floor, one child per cell. Ratio is sandbox over default.

| Workload | dev-JIT | ship-C-AOT |
|---|---|---|
| fib-recursive | 1.43x | 3.12x |
| fib-loop | 3.02x | 4.34x |
| mandelbrot | 1.85x | 1.03x |
| primes | 1.97x | 1.82x |
| sort | 1.30x | 1.56x |
| queen | 1.16x | 1.40x |
| particles | 1.19x | 2.11x |
| collect | 1.00x | 1.01x |
| tree | rejected S023 | rejected S023 |
| callbacks | not measured, quota | not measured, quota |

The cost tracks call and loop-edge frequency: both intrinsics are a
runtime call with a pending-trap check. `tree` frees with
`Context.free`, so it has no profile form; a variant without frees
is a different program. `callbacks` keeps 7 MB per round live and
passes the 64 MiB default; no runner took a host quota (round 3b).

Candidate for later evidence, not proposed here: an inline relaxed
load of the flag through a pointer the runtime hands out at
`enter_script`, in place of the call, with the stack check kept at
`Enter` only.

### Criterion 4 — the adversarial list

Five shapes, each with a control: 1,048,577 bytes → S026; depth 257
→ S026; recursion with no base case → `stack-budget` on both tiers;
an allocation loop with every block live → `allocation-quota` on
both tiers; `fromBytes` of forged bytes → S024.
`codegen/tests/sandbox_adversarial.rs`.

### Criterion 6 — the docs

`examples/sandbox/` (a C host that sets the limits, interrupts from a
second thread after 20 ms, and reads the trap back; checked by the
examples gate), tutorial Step 11, README, `llms.txt`,
`docs/tutorial-rust.md`. Every pasted output is from a run in the
round.

## P26 round 3b — a host quota on `RunConfig` (landed)

Contract pin `d5f84df`. `RunConfig.alloc_quota` and
`RunConfig.stack_budget`; a set value replaces the profile default
and applies under the default profile too. Five tests, one per
runner plus the default-profile clause; dropping the limit argument
turns all five Red. Gate:
`gate quick b1c48d778fa0a305f4cce158cc461ebf90fa5eda dirty:7 debug 1520/0/2 skips 2 goldens-moved 0 exit 0`

The criterion 5 table, complete (release, arm64 macOS, sandbox over
default; `callbacks` under a 256 MiB host quota, every other row
under the §109.5 defaults):

| Workload | dev-JIT | ship-C-AOT |
|---|---|---|
| fib-recursive | 1.47x | 3.18x |
| fib-loop | 3.02x | 4.41x |
| mandelbrot | 1.85x | 1.03x |
| primes | 1.97x | 1.82x |
| sort | 1.30x | 1.59x |
| queen | 1.16x | 1.42x |
| particles | 1.18x | 2.11x |
| callbacks | 1.15x | 2.95x |
| collect | 1.01x | 1.02x |
| tree | rejected S023 | rejected S023 |

Open at the end of the rounds: the CLI takes no host limit (§109.5
names none); the `benchmarks/Cargo.toml` bin comment names four of
six bins. File growth over the phase (`aebef92` to `594b267`):
`jit.rs` 1,985 → 2,179, which crosses §5.y and is split in the
review-fix round; `ship.rs` 2,184 → 2,481 and `interpreter.rs`
6,930 → 6,987, both already over the limit at the pin.

## P26 Phase Review (2026-09-17)

A fresh reviewer read the cumulative diff `aebef92..594b267` against
seven questions: the boundary's soundness, the `live_bytes` counter,
the stack floor, the interrupt flag, tests, contract-code agreement,
and conventions. Findings: CRITICAL 0, MAJOR 2, MINOR 9. Every
finding is fixed in `e2c6b13`; the contract amendments are `49271f6`
and `bb3fb42`.

| Severity | Finding | Fix |
|---|---|---|
| MAJOR | `jit.rs` was 1,985 lines at the pin and 2,179 at review; §5.y says split first | seven child modules under `codegen/src/jit/`, largest 441 lines, parent 687; a pure move, 62 functions before and after |
| MAJOR | the module initializer got no `Sandbox.Enter`; the LIR test's source had no global, so it could not see it | `Enter` on the initializer; the test source gained a global and a top-level statement and was Red at the pin |
| MINOR | `do` named in §109.3; HIR has none | contract |
| MINOR | S021 cited after the renumber | contract |
| MINOR | the tracking note's status line contradicted its content | record |
| MINOR | `expect` on thread spawn in library code | fallback to the caller's thread, one line; no injection point for a test |
| MINOR | the second thread stored into the flag inside the owner's `&mut Context` | the flag is an `Arc<Interrupt>` cell outside the Context's bytes; `subscript_rt_ctx_interrupt_handle` on the owner thread, `subscript_rt_interrupt_set` from any thread; `subscript_rt_ctx_interrupt` removed |
| MINOR | the emitted ship interrupt thread was detached; a short program freed the Context under it | joined before release, both branches |
| MINOR | the dev-JIT firing control left a runaway thread | the control sets the flag through the handle and joins |
| MINOR | `Profile::as_str` had no caller and no test | removed |
| MINOR | fused callback loops carried no `Poll`; the checkpoint interval was the array length | `Poll` on `array-callback.cond` and `for-each.cond`; two tests, Red at the pin |

Checked sound by the reviewer: every `live_bytes` site in both modes;
the mode switch refused after the first allocation; the stack floor
recorded at depth 0 on the script thread in every entry path
including `async_step`; the regex budget default; `bytesInto`'s range
check; no history token in any added comment; hygiene and fmt clean.

Interrupt latency with the control fixed, four serialized runs:
dev-JIT 20.1–28.3 µs, ship-C-AOT 37–41 µs.

Gate after the fixes: `gate quick 49271f6 dirty:18 debug 1528/0/2
skips 2 goldens-moved 0 exit 0`. `tools/hygiene.sh` exit 0 on the
committed tree.

### §109.8 exit criteria

| # | Criterion | Status |
|---|---|---|
| 1 | every §109.7 entry Red at the pin, Green after | met (rounds 1, 2) |
| 2 | `tools/gate.sh full` green | met: `gate full e2c6b13 dirty:1 debug 1528/0/2 release 1525/0/2 skips 2/0 clippy 7/18/13 goldens-moved 0 exit 0` |
| 3 | interrupt latency per tier recorded | met (round 2, review-fix) |
| 4 | the adversarial list rejects or traps | met (round 3) |
| 5 | the benchmark matrix under the profile | met (rounds 3, 3b) |
| 6 | README and tutorial state the profile | met (round 3) |
| 7 | hygiene clean at the Phase Review | met |

**P26 is COMPLETE** (2026-09-17): no open CRITICAL or MAJOR, the full
gate green, hygiene clean.

Open after the phase: the CLI takes no host limit and no interrupt;
twelve Rust files remain past §5.y, all past it at the pin; the
`benchmarks/Cargo.toml` bin comment names four of six bins.

## P26 follow-up — memory under the profile (landed 2026-09-17)

Owner request: document how a profile program reclaims memory. Contract
§109.8a (`99c9985`) and §18.2d's `subscript_rt_ctx_collect` (`608e6c5`,
after the round's part 1 stopped on the missing declaration: the host
header declares the `subscript_rt_ctx_` family only, and the script
intrinsic `subscript_rt_collect` is outside it). Implementation
`c1db294`; README section `cd003ec`.

`examples/sandbox/`, measured on the ship tier, batch 1,280 nodes with
one string each, a three-batch window, quota 1,048,576, threshold
786,432:

| Phase | Host collects | Peak live | Final live |
|---|---|---|---|
| A, the host paces | frames 10, 17, 24 | 868,480 | 199,216 |
| B, the script collects each frame | 0 | — | 199,216 |

Gate: `gate quick 608e6c5 dirty:7 debug 1530/0/2 skips 2 goldens-moved 0 exit 0`.

## Security review follow-ups (2026-09-17)

Two reviews after P26 COMPLETE: an external reproduction report (two
P1: `(/*)*/` cancels the S026 byte scan; `String.repeat` builds its
result outside the quota) and this session's adversarial review (two
MAJOR, one root cause: nesting through `<>`, `!`, and `? :` escapes
S026 and aborts the process; a chain of `!` is exponential in the
checker). A policy review then asked the contract to state outcomes.

Contract: `bc1d2ad` (S026 over lexer tokens; no script-sized buffer
outside the quota), `0acd1b6` (the checker bounds the tree and visits
each node once; the pipeline on the compile thread), and §109.0 (the
guarantees, the host's facts, the exclusions, S027, the work and
output budgets, the per-program source limit).

### Pre-registered measurements

| Id | Measurement | Decides |
|---|---|---|
| M6 | resident bytes of a Context holding 64 MiB of 8-byte objects, and of 4 KiB objects, in both memory modes, against the quota | the reserved-bytes charge of §109.0 "Memory, precisely" and its multipliers |
| M7 | the SWC parser alone on the deepest 1 MiB source per nesting construct, on the compile thread: returns or overflows, and the stack size that suffices | the token-level proxies §109.2 rule 2 needs, if any |
| M8 | wall time of the checker on a 200-deep `!` chain before and after the one-visit fix | rule 1 |
| M9 | whether a 1,073,741,824-byte compile-thread stack reserves and runs a 131,072-token type-argument nest on arm64 macOS, x86_64 Linux, and windows-msvc | §109.2a's two numbers |
| M11 | every M7 construct on a 131,072-byte file, parser alone, optimized at 2 GiB and unoptimized at 4 GiB: returns or overflows, and the margin | §109.2a's byte limit and two stacks |
| M12 | an 8,589,934,592-byte compile-thread reservation, unoptimized: spawns, commits nothing untouched, and parses a 131,072-byte parenthesis file | §109.2a's unoptimized stack |
| M10 | the cost attribution of three quadratic shapes inside every limit (the second Phase Review): `<i32>` type assertions × n (8,000: 13 s; 40,000: over 300 s), decorators × n on one line (32,000: 1.4 s; 60,000: 563 s) against one per line (28.7 s), labels × n (60,000 duplicate: killed at 30 s); which part is this compiler's (position mapping, label checks) and which is the SWC parser's; the fix for ours; a bound with a number for SWC's | whether guarantee 4 needs a per-line or per-statement token rule |

### Security round 1 — S026 over lexer tokens; the quota covers temporaries (landed)

Contract `bc1d2ad`, `8a258a1`. Gate:
`gate quick 32287d1 dirty:12 debug 1544/0/2 skips 2 goldens-moved 0 exit 0`.

| Item | Result |
|---|---|
| S026 Red | `(/*)*/` × 257 checked clean at the pin; × 2,000 aborted the process |
| S026 fix | the scan walks the SWC lexer's tokens (`parse.rs::with_tokens`), `${` is one opener; 12,000 comment levels lex in 0.7 ms release, 1 MiB of source in 26 ms; `r237` Red at the pin, `tsc: accepts` measured |
| Quota Red | `"x".repeat(268435456)` peaked at 270,123,008 resident bytes before the trap; 7,831,552 after |
| Quota fix | `Context::check_quota` and `quota_headroom`; `QuotaBuf` bounded by the headroom; `alloc_str_with` for `repeat` |
| Total check | `runtime/tests/quota_peak.rs`: a counting global allocator over ten entries under a 64 KiB quota with 256 MiB requests; at the pin eight of ten peaked at 268 MB to 805 MB and two recorded no trap; the second test derives the 99 exports from `ffi.rs` and fails on one that is neither covered nor exempt with a reason |

Bounded multiples kept, charged in the M6 round: `toUpperCase` and
`toLowerCase` (3× the receiver), `JSON.parse` (about 40 bytes per
node of a quota-held input).

### Security rounds 2 and 3 (landed)

One gate on the union tree:
`gate quick 595332f dirty:60 debug 1568/0/2 skips 2 goldens-moved 0 exit 0`.

**Round 2, nesting and budgets.** The exponential `!` chain was a
double visit in `warn.rs` (30 operators: 25.75 s → 0.02 s). The
main-thread abort was `program_loader` parsing imports on the caller's
thread. Nesting guard at three sites (measured boundaries in §109.2),
S027, per-program 8 MiB, checker work and LIR output budgets (both
unreachable under 1 MiB: max 187,498 of 16,777,216 units), the whole
pipeline on the compile thread (256 MiB), and the token limit of
16,384 per file from the parser measurement (§109.2a; the target is
131,072 at a 1 GiB stack, M9). Corpus `r238`–`r241`, twins
`a239`–`a243`. A 40,000-level `? :` source now checks clean under the
default profile in 0.13 s instead of aborting.

**Round 3, M6.** The quota charges reserved bytes (`charged_bytes`,
`EXACT_RECORD_BYTES` = 64): 64 MiB of 8-byte objects held 1,023 MB
(15.25x) before and 69.7 MB (1.04x) after. `subscript_rt_ctx_charged_bytes`
is the host's pacing figure; `examples/sandbox` paces on it (frame 6,
then every fourth frame; peak 915,472). `toUpperCase`/`toLowerCase`
charge 3x first; `JSON.parse` charges 40 bytes per node against the
headroom. `sandbox-cost`: no cell moved. The `callbacks` ratios of the
criterion 5 table are now 1.62x/3.72x at the pin (the move predates
M6).

### Security round 4 (landed)

Contract `465d74b`. Gate:
`gate quick 465d74b8a0b9d082758463d96fa94c6a2088cc2c dirty:21 debug 1571/0/2 skips 2 goldens-moved 0 exit 0`

M9 on arm64 macOS: a 1 GiB compile-thread reservation commits 16 KiB
untouched; 131,072 type-argument levels parse in 139 ms (smallest
stack 696,254,464 bytes, 5,312 per level); the unoptimized build
costs 16,018 per level, so its stack is 4 GiB (commits 128 KiB
untouched; the deepest admitted nest, 56,160 levels, parses in
236 ms and 910 MB). The token limit is 131,072 in every build,
derived by a `const fn` from the stack and the cost of the build; one
test pins the four numbers and the derivation. A refused spawn under
the profile is S026. `LowerError` carries a rule code, so the LIR
budget renders as `error[S026]`. Docs: the five rules and four S026
limits in README and Step 11; "What the host supplies" and "What is
excluded" from §109.0. Linux and windows-msvc M9 is the owner's gate
(§100): the 1 GiB and 4 GiB reservations spawn and commit nothing
untouched, the 131,072-level nest parses, the unoptimized workspace
suite passes.

### Second Phase Review and its fix round (landed)

A fresh reviewer read `f5e16ab..efef4db`: CRITICAL 1, MAJOR 6,
MINOR 6. The CRITICAL was the class "a parse before the S026 scan"
for the second time (`program_loader` parsed every file with the full
parser before the scan; a 600 KB type-argument nest inside the byte
limit aborted the compile thread), so the fix is the form: the
parser's entry owns the scan (§109.2 rule 5), with a total check that
every `Lexer`/`Parser` construction is the one constructor in
`parse.rs`. Also: a lexer error no longer discards the scan;
`check_program_with` spawns the compile thread itself; the quota's
total check reads every `subscript_rt_` export (99 → 229; 130 had
been neither covered nor exempt); `sort` (two receiver copies, 125 MB
outside a 64 MiB quota at the pin) and `boundary_scratch_alloc` (a
system allocation sized by the marshalled array, never seen before)
charge the quota first; instantiations spend work; type-parameter
constraints and defaults are guarded.

Gate: `gate quick a67c7b4 dirty:18 debug 1578/0/2 skips 2 goldens-moved 0 exit 0`.

Two measured facts about the scan's lexer: it reports a `Token::Error`
for a valid regular-expression escape, and it reads `/` as division,
so a bracket inside a regex literal counts (§109.2 amended both).

M10, attributed: `<i32>` × n is the SWC parser, O(n²), 180.8 s at the
most the token limit admits (43,690 levels); the numeric literal is
the SWC lexer, O(n²), about 1 s at the byte limit; decorators and
labels were this compiler's diagnostic renderer, O(n²) time and, on
one line, O(n²) output (7.69 GB for 32,000 items on a 160 KB line).
Decision: the parser's constant stays and the host's build timeout
bounds it; the renderer gains a line index, a 240-byte window, and a
200-item cap (round 6).

### Security round 6 — the renderer (landed)

Gate: `gate quick 1bb891c dirty:3 debug 1583/0/2 skips 2 goldens-moved 0 exit 0`.
At the pin, 32,000 items on one 160 KB line rendered 7.68 GB in 1.1 s
and 40,000 one-per-line items took 16.7 s (quadratic). After: a line
index per file, a 240-byte window at UTF-8 boundaries, 200 items;
640,000 items on one line render 59,986 bytes in 19 ms, 800,000
label items in 31 ms (linear). No pinned rendered form changed. Step
11 names the host's compile timeout.

### Third Phase Review and security round 7 (landed)

A fresh reviewer read `efef4db..b1e2bfe`: CRITICAL 1, MAJOR 3, MINOR
4. The CRITICAL was the third instance of "the parser runs on a
source the scan did not bound": a lexer with no parser reads `/` as
division, so a quote inside a regex literal desyncs the token and
bracket counts for the rest of the file. Under CLAUDE.md the class
is a defect of the form. Form: S026 is a byte limit only (131,072
per file, 8,388,608 per program); the token limit and the pre-parse
bracket depth are retired; the parser's stack is sized to the
deepest nesting a file of the byte limit can spell (parenthesis: one
byte per level); the checker's nesting guard is the one depth bound.
M11 measured every M7 construct at 131,072 bytes: all return; the
unoptimized parenthesis costs 31,151 per level (4.66x optimized),
which sets the unoptimized stack at 8 GiB (margin 2.10; optimized
2 GiB, 2.43). The `print` sink (268 MB outside a 64 MiB quota at the
pin) and callback bindings now charge the quota;
`parse_import_specifiers` spawns the compile thread; a regex literal
holding a quote before a deep nest and 257 lines of
`s.replace(/"/g, …)` are adversarial cases (the second checks clean
and runs on both tiers). Corpus: `r241` and `a243` are the byte
shapes; `r236`/`r237` reject through the nesting guard.

Gate: `gate quick 2410a08 dirty:29 debug 1578/0/2 skips 2 goldens-moved 1 exit 0`
(the moved golden is the retired token-count entry's). One flake
fixed on the way: the counting-allocator control read `PEAK − before`
and a concurrent free by the test harness, outside any test body,
shortened it by 24 to 118 bytes; the window now tracks its own floor.

Open: the in-repo runners' print observer still holds unbounded host
memory under the profile (round 8, §109.7a); `subscript_rt_print`
carries no `pos_id`, so its quota trap sits at position 0.

### Security round 8 (landed)

Gate: see the verdict line below. M12 in a debug build: an 8 GiB
compile-thread reservation spawns in 85 µs and commits 81,920 bytes
untouched; a 131,072-byte file of open parentheses (131,071 levels)
parses in 550 ms and 4,086,595,584 resident bytes, 31,150 per level,
0.48 of the stack. Under the profile the dev-JIT runner, the emitted
ship entry, and the reload session install no print observer and
read the charged Context sink: `sink.ts` (65,536 bytes × 4,096 lines)
now traps `AllocationQuota` with 1,022 whole lines (66,978,814 bytes,
0.998 of the quota) before the trap, and the dev-JIT peak is 101 MB
against 805 MB at the pin. Every profile golden holds.

Open after round 8: `examples/sandbox/main.c` installs a print
observer (a host program; its output is three lines); a profile run
that ends abnormally returns no output, because the sink reaches the
retained file once at the end; `subscript_rt_print` carries no
`pos_id`; `ship.rs` is 2,605 lines, one of thirteen files over §5.y.
`gate quick 75c963e39c209824648e4d0841700bc5e2db9866 dirty:11 debug 1582/0/2 skips 2 goldens-moved 0 exit 0`
