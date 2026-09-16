<!-- §109 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 109. The sandbox profile

*(Owner decision 2026-09-16.)* Origin: plan Rev 3. Evidence:
`specs/tracking/sandbox-tier-2026-09-16.md`. Phase: P26.

Problem: a host that runs content it did not write (mods, shared
levels, plugins) had no execution form. Invariant 6 now reads
"Scripts are trusted, except under the sandbox profile". This section
is the profile.

The profile is a **compile profile**, not a tier. The checker narrows
the accepted language. The lowering emits two intrinsics that every
tier executes. The runtime holds three host-set limits. A program
that does not select the profile gets no new instruction and no new
check.

### 109.1 Selection

1. The CLI accepts `--profile sandbox` on `check`, `build`, and
   `run`. The default profile has no name and no flag.
2. `CheckOptions` carries the profile, and the checker reads it for
   109.2. The checked HIR module carries the profile it was checked
   under, so every consumer of HIR reads the same fact: the LIR
   lowering for 109.3, and each tier runner for the 109.5 defaults.
   No runner or lowering takes the profile as a second parameter.
   *(Amended 2026-09-16, before round 2: the first text named
   `LowerOptions` as a second carrier. One form carries the fact,
   core principle 8.)*
3. A corpus entry selects the profile with the header line
   `// profile: sandbox`. Every harness that compiles an entry reads
   the header and passes the profile to both options.
4. A program under the profile is a program of the accepted
   language. Every rule below removes a form or adds a check.
   Nothing is added to the surface, so the `tsc` gate is unchanged.

### 109.2 Compile-time rules

Each rule is one `RuleCode`. Each has one reject entry (109.7). Each
diagnostic names the profile in its message.

| Code | Rejects |
|---|---|
| S023 | `Context.free`. Memory is allocate-only. `Context.collect()` stays callable: a rejection adds no safety, and the interrupt flag bounds its cost. |
| S024 | `Context.fromBytes`, always. `Context.bytesOf` and `Context.bytesInto` stay accepted: the default profile already rejects a layout with a handle, a reference, or a string (S100, the value-class whitelist), so no profile rule is needed. |
| S025 | `Worker.spawn`, `Inbox`, and `Outbox`. |
| S026 | Source over a limit, before the parser runs: more than 1,048,576 bytes in one file, or a bracket depth over 256. The depth is a count over the bytes of `(`, `[`, `{` against `)`, `]`, `}`, with no lexing; a closer below zero resets to zero. The count over-approximates the syntactic depth, so it rejects more, never less. |

*(Amended 2026-09-16, after round 1.)* The first text assigned S019
to S022. §99.3 retires S019 and forbids its reuse, so the codes are
S023 to S026. The first text gave S024 a second clause over
`bytesOf` and `bytesInto` layouts with a handle. Round 1 measured
that the default profile rejects every such layout at the
declaration and at the call, so the clause had no program and is
removed (core principle 9).

**The checker runs on its own thread.** Round 1 measured that the
parser and the checker recurse once per nesting level: a debug test
thread of 2 MiB overflows at depth 66, and the release CLI on the
main thread runs depth 257. `check_program_with` runs the parse and
the check on a thread it spawns with a 64 MiB stack, and joins it.
The depth capacity is then a compiler fact, not the caller's thread.
S026's limit of 256 is under that capacity on every harness thread,
so the depth entry is constructible and the §90 mutation sweep
returns on every mutation of it. The default profile keeps no depth
limit.

The capability mirror is not a new rule. A script binds only the
`--mirror` it is given (§7, Step 7 of the host tutorial), and the
JIT refuses a foreign symbol the lowering did not import. A host
builds one mirror per trust level with `subscript bind` and passes
the narrow one to untrusted content. 109.7 adds the example.

Deferred, each for its own evidence: a checker work limit; a Worker
count limit as an alternative to S021.

### 109.3 What the lowering emits

A new `IntrinsicFamily::Sandbox` with two operations. The lowering
emits them only under the profile.

| Intrinsic | Placed | Effect |
|---|---|---|
| `Sandbox.Enter` | first instruction of every function body, after the parameter binds | calls `subscript_rt_sandbox_enter(ctx)`; then the pending-trap check |
| `Sandbox.Poll` | on the iteration edge of every HIR loop (`while`, `do`, `for`, `for-of`), before the condition | calls `subscript_rt_sandbox_poll(ctx)`; then the pending-trap check |

1. Both are calls into the shared runtime followed by the pending
   trap check that every runtime call already gets. No tier decides
   what they mean; the runtime does (§68.7).
2. The dev JIT, the C emitter, and the reference interpreter each
   lower the two intrinsics as that call. The gate stays total: a
   profile entry runs on every tier and matches its golden.
3. A host callback that re-enters a script function runs `Enter` at
   that function's entry, so the limits cover it.
4. `async` functions and coroutines are state machines on one native
   stack (§26). `Enter` runs at every resume that enters a function
   body, and `Poll` runs on every loop edge inside one. No second
   stack exists, so 109.4 rule 3 covers them.

### 109.4 What the runtime holds

Three limits on the Context. Each defaults to "none", so a trusted
program is unchanged. Each is one C API call.

| Limit | C API | Trap |
|---|---|---|
| interrupt | `subscript_rt_ctx_interrupt(ctx)` sets an atomic flag | `Interrupted` = 25 |
| allocation quota | `subscript_rt_ctx_set_alloc_quota(ctx, bytes)`; 0 is none | `AllocationQuota` = 26 |
| stack budget | `subscript_rt_ctx_set_stack_budget(ctx, bytes)`; 0 is none | `StackBudget` = 27 |

1. **Interrupt.** `subscript_rt_ctx_interrupt` is callable from any
   thread. It is the one Context call outside the exclusive contract,
   and it touches one atomic. `Enter` and `Poll` read the flag with a
   relaxed load. If the flag is set, the runtime records the
   `Interrupted` trap at the intrinsic's position and the trap-stop
   path returns to the host. `subscript_rt_ctx_clear_trap` clears the
   flag with the trap.
2. **Allocation quota.** The §21 allocation path compares
   `live_bytes` plus the request against the quota before it
   allocates. Over the quota, it takes the §21 fault path with
   `AllocationQuota` in place of `AllocationFailure`. Strings, arrays,
   maps, sets, and objects all pass through that path, so no separate
   size limit exists.
3. **Stack budget.** `enter_script` at depth 0 records the address of
   a local as the floor. `subscript_rt_sandbox_enter` compares the
   address of its own local against the floor minus the budget. Below
   the floor minus the budget, it records `StackBudget`. The host sets
   a budget below the thread's stack size. That is the host's fact,
   stated in the host tutorial.
4. A trap under the profile is an ordinary trap: first trap wins, the
   observer fires, the Context survives, the host reads it (§18.2).

### 109.5 Defaults under the CLI and the corpus harness

`subscript --profile sandbox` and a corpus entry with `profile:
sandbox` set the quota to 67,108,864 bytes and the stack budget to
524,288 bytes before the entry runs. A host embedding the runtime
sets its own. The interrupt has no default; nothing sets it unless
the host does.

### 109.6 Deferred: run-time source loading

"What you embed is the ship tier" (host tutorial). The runtime has no
API that compiles source at run time. A host that loads user content
at run time needs the dev tier behind a C ABI. That is a new surface
with its own dependency-size cost (measured at 3.08 MB for Cranelift
plus the C emitter together). It waits for a host that needs it, and
for its own section. Until then a host compiles user content with
`subscript build --profile sandbox` and links it as it links its own
scripts.

### 109.7 Corpus

| Entry | Kind | Proves |
|---|---|---|
| `r-sandbox-free` | reject | S023 |
| `r-sandbox-from-bytes` | reject | S024 |
| `r-sandbox-worker` | reject | S025 |
| `r-sandbox-source-depth` | reject | S026, depth 257 |
| `a-sandbox-clean` | accept | a profile program with a loop, a call, `bytesOf` on a scalar struct, and `Context.collect()` runs on every tier and matches its golden |
| `t-sandbox-alloc-quota` | trap | `AllocationQuota` at the allocation site, output before the trap intact |
| `t-sandbox-stack-budget` | trap | `StackBudget` at the recursive call; the entry prints nothing that depends on the depth reached |

Each reject entry's header states what `tsc` does, measured. Each
reject entry also carries a twin without the header line, and that
twin is an accept entry: the same source under the default profile
runs. That twin is the firing control. The worker entry has no twin;
the existing worker accept entries are its control.

A profile rejection is not a TypeScript divergence: the surface is
unchanged and the default profile accepts the same source. §79 rule
1's divergence block does not apply to a reject entry whose header
selects a profile. The harness that pairs blocks with entries skips
such an entry and asserts that it skipped at least one.

The interrupt has no corpus entry, because the corpus has no second
thread. It has one test in `codegen/tests/` per tier: a profile
program with an infinite loop, a second thread that sets the flag
after 50 ms, and an assertion that the entry returns with
`Interrupted`. The same test without the flag set is the firing
control, bounded by a 2 s abort. The recorded number is the time from
the flag to the return.

### 109.8 Exit criteria

1. Every 109.7 entry is Red at this contract's pin and Green after.
2. `tools/gate.sh full` is green.
3. The interrupt latency, per tier, is recorded in the tracking note.
4. The adversarial list runs and each item rejects or traps: a
   1,048,577-byte source; a 257-deep bracket source; recursion with no
   base case; an allocation loop; `fromBytes` of forged bytes. The
   outcome of each is recorded.
5. The benchmark matrix runs under the profile on the dev JIT and the
   ship tier. The ratio to the default profile is recorded per
   workload. No threshold; the number is the baseline.
6. `README.md` and `docs/tutorial-c-cpp.md` state the profile where
   they state "not a sandbox" today.
7. `tools/hygiene.sh` is clean at the Phase Review.
