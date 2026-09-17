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

### 109.0 What the profile guarantees

*(Added 2026-09-17, after a policy review that asked for outcomes,
not only prohibitions.)* The rules in 109.1 to 109.5 exist to hold
the four statements below. A rule that does not serve one of them is
not a profile rule.

1. **Reach.** An accepted profile program reaches only the functions
   its mirror declares and the memory its own Context allocated. It
   holds no address it did not receive from the runtime or the host.
2. **Stop.** The program stops with an ordinary trap before any
   host-set limit is exceeded: the allocation quota before the bytes
   exist, the stack budget before the next frame, the interrupt at
   the next checkpoint. The Context survives and the host reads the
   trap.
3. **Return.** After the host sets the interrupt, control returns to
   the host within bounded work: the straight-line code between two
   checkpoints, plus one runtime operation in flight. A runtime
   operation has no interior checkpoint; its work is a polynomial in
   its input and output sizes, and both are under the quota. Regular
   expressions carry their own work budget (§23).
4. **Compile.** On every source within S026's limits, the compiler
   returns a diagnostic or an accepted program, in work bounded by a
   polynomial in the source size, on a thread whose stack it sizes.

**What the host supplies.** Each guarantee rests on a fact only the
host holds. The host tutorial states each one where the host acts.

- The thread that runs script has more stack than the budget plus
  the headroom 109.4 rule 3 names.
- Every function in the mirror is part of the trusted boundary. It
  validates the arguments a script controls (pointer and count pairs,
  handles, indices, lengths), bounds its own work or is excluded from
  guarantee 3 by the host's own statement, and does not block.
- The profile arms no interrupt. A host that needs guarantee 3 arms
  one, or accepts that a program under the profile runs until it
  returns or traps.
- Same-process execution trusts the compiler, the generated code, and
  the runtime to be memory-safe. A host that must contain a defect in
  those components adds an isolation boundary of its own, such as a
  separate process. The profile does not provide one.

**What is excluded, by name.**

- `Context.collect()` has no interior checkpoint. Its work is
  proportional to the live set plus the dead set, which the quota
  bounds; it is not interruptible. A host that needs a latency bound
  calls collect at its own boundary (109.8a) and sizes the quota to
  the collect it can afford.
- A host function's work is the host's (above).
- The quota bounds the bytes the Context reserves for the program.
  It does not bound the host's own allocations, or the compiler's.

**Memory, precisely.** The quota charges the bytes the allocator
reserves for an allocation: the payload rounded to its size class
plus the block header in the arena mode, and the payload plus the
header plus the per-allocation record in the exact-size mode. Process
memory attributable to the Context is then the quota plus a fixed
overhead, whatever the allocation sizes. The host reads the charged
bytes through `subscript_rt_ctx_charged_bytes` (§18.2d) and paces on
that figure (109.8a). Reserved bytes are a tier fact, as `live_bytes`
is (§18.2d): a 64 MiB quota holds 6,100,800 payload bytes of 8-byte
objects in the exact-size mode and 33,554,432 in the arena mode, and
about 66 MB of 4 KiB objects in both. *(Measured in the M6 round at
`52373a9`, before the charge: with 8-byte objects a 64 MiB quota held
1,023,410,200 bytes in the exact-size mode, 15.25x, and 134,348,816
in the arena mode, 2.00x; after the charge the worst case is 1.04x.
The estimate this paragraph first carried, 5x and 11x, was wrong.)*

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
| S023 | `Context.free`. Memory is allocate-only. `Context.collect()` stays callable: a rejection adds no safety, and its work is bounded by the quota (109.0, excluded from guarantee 3). |
| S027 | A function whose frame is over 65,536 bytes, under the profile. The checker already sizes every frame (`MAX_FRAME_BYTES`); the profile lowers the limit so that the stack check at `Enter` sees at most one bounded frame past the budget. |
| S024 | `Context.fromBytes`, always. `Context.bytesOf` and `Context.bytesInto` stay accepted: the default profile already rejects a layout with a handle, a reference, or a string (S100, the value-class whitelist), so no profile rule is needed. |
| S025 | `Worker.spawn`, `Inbox`, and `Outbox`. |
| S026 | Source over a limit, before the parser runs: more than 1,048,576 bytes in one file, more than 8,388,608 bytes in one program (every file the entry imports, mirrors excluded), or a bracket depth over 256. The depth is a count over the **lexer's tokens** of `(`, `[`, `{` against `)`, `]`, `}`: the SWC lexer runs as a plain iterator with no parser, so a bracket inside a comment, a string, a template, or a regular-expression literal is not a bracket. A closer below zero resets to zero. The lexer is a flat loop over the bytes, so its cost does not grow with the depth. |

*(Amended 2026-09-17, after an external review.)* The first text
counted bytes with no lexing and claimed the count over-approximates
the syntactic depth. It does not: a closer inside a comment
decrements the count, so `(/*)*/` repeated cancels the scan. Measured
at `5d288f5` with the release CLI: 257 such levels check clean under
the profile; 2,000 abort the process with a main-thread stack
overflow, in the warning walk that runs after the checker thread
returns. The count is now the lexer's. Every stage after the checker
that recurses over the tree (`check_warnings`, the LIR lowering, the
emitters) runs on the caller's thread and is bounded only by S026,
so the exactness of S026 is the profile's whole defence there. The
default profile keeps no limit and is unchanged.

*(Amended 2026-09-16, after round 1.)* The first text assigned S019
to S022. §99.3 retires S019 and forbids its reuse, so the codes are
S023 to S026. The first text gave S024 a second clause over
`bytesOf` and `bytesInto` layouts with a handle. Round 1 measured
that the default profile rejects every such layout at the
declaration and at the call, so the clause had no program and is
removed (core principle 9).

**The compiler runs on its own thread, and the checker bounds the
tree.** Round 1 measured that the parser and the checker recurse
once per nesting level: a debug test thread of 2 MiB overflows at
depth 66, and the release CLI on the main thread runs depth 257.
`check_program_with` runs the parse and the check on a thread it
spawns with a 64 MiB stack, and joins it. The depth capacity is then
a compiler fact, not the caller's thread. S026's limit of 256 is
under that capacity on every harness thread, so the depth entry is
constructible and the §90 mutation sweep returns on every mutation
of it. The default profile keeps no depth limit.

*(Amended 2026-09-17, after the adversarial review.)* Brackets are
not the only nesting. Measured at `5d288f5` with the release CLI
under the profile: `Array<Array<…>>` at depth 2,000 (14 KB), a chain
of 60,000 `!`, and a chain of 40,000 `? :` (480 KB) each abort the
process with a main-thread stack overflow, in the stages that run
after the checker thread returns. A chain of 30 `!` takes over 25 s
in the checker, doubling per operator. Three rules close the class:

1. **The checker visits each syntax node once.** A chain of n
   operators costs O(n). The unary-chain cost is a checker defect,
   fixed at its site, with a test that bounds the time of a 200-deep
   chain.
2. **S026's third limit is a nesting limit on the tree the checker
   walks.** One depth guard type, entered at every recursive
   descent in the checker (expressions, types, statements, patterns);
   a depth over 256 is S026 at that node, under the profile only.
   The HIR the checker emits is therefore bounded, and every later
   stage (`check_warnings`, the LIR lowering, the emitters) inherits
   the bound without a guard of its own. The bracket count over the
   lexer's tokens stays, because it is the one limit that runs before
   the parser.
3. **The whole pipeline runs on the compile thread.** The 64 MiB
   thread of `check_program_with` becomes the thread of the whole
   compile: parse, check, warnings, lowering, and emission, in the
   CLI and in every codegen runner. Nothing that recurses over the
   tree runs on the caller's thread.
4. **Work and output are budgeted.** Under the profile the checker
   counts the nodes it visits and the type instances it creates;
   over 16,777,216 it reports S026 and stops. The LIR lowering counts
   the instructions it emits; over 4,194,304 it reports S026 and
   stops. Nesting bounds depth; these two bound the width that
   instantiation and expansion can add.

The parser runs before rule 2 can reject, so its own tolerance is
measured, not assumed: for each nesting construct the grammar has
(type arguments, prefix operators, conditional expressions,
conditional types, assignment and exponent chains, member and call
chains, binary chains, template substitutions, unions, array-type
suffixes), the deepest source under 1,048,576 bytes runs through the
parser alone on the compile thread. A construct that overflows there
gets a token-level proxy limit before the parser, recorded here with
its number. The stack size of the compile thread is set from that
measurement.

The capability mirror is not a new rule. A script binds only the
`--mirror` it is given (§7, Step 7 of the host tutorial), and the
JIT refuses a foreign symbol the lowering did not import. A host
builds one mirror per trust level with `subscript bind` and passes
the narrow one to untrusted content. 109.7 adds the example.

Deferred, each for its own evidence: a checker work limit; a Worker
count limit as an alternative to S025.

### 109.3 What the lowering emits

A new `IntrinsicFamily::Sandbox` with two operations. The lowering
emits them only under the profile.

| Intrinsic | Placed | Effect |
|---|---|---|
| `Sandbox.Enter` | first instruction of every function body, after the parameter binds; the module initializer is a function body | calls `subscript_rt_sandbox_enter(ctx)`; then the pending-trap check |
| `Sandbox.Poll` | on the iteration edge of every loop the lowering emits, before the condition: the HIR loops (`while`, `for`, `for-of`) and the fused loops the lowering builds for a static callback, a static array callback, and `forEach` | calls `subscript_rt_sandbox_poll(ctx)`; then the pending-trap check |

*(Amended 2026-09-17, after the Phase Review.)* The first text named
`do`, which HIR does not have, and named the HIR loops only. The
module initializer got no `Enter`, and a fused callback loop got no
`Poll`, so the interval between checkpoints on such a loop was the
array length. Both are closed.

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
| interrupt | `subscript_rt_ctx_interrupt_handle(ctx)` on the owner thread; `subscript_rt_interrupt_set(handle)` from any thread | `Interrupted` = 25 |
| allocation quota | `subscript_rt_ctx_set_alloc_quota(ctx, bytes)`; 0 is none | `AllocationQuota` = 26 |
| stack budget | `subscript_rt_ctx_set_stack_budget(ctx, bytes)`; 0 is none | `StackBudget` = 27 |

1. **Interrupt.** The flag lives in its own heap cell, outside the
   Context's bytes, so a store from another thread never touches
   memory the owner thread holds exclusively. The host obtains the
   cell on the owner thread, before or between runs:
   `subscript_rt_ctx_interrupt_handle(ctx)` returns
   `const subscript_rt_interrupt*`, valid until the Context is
   released. `subscript_rt_interrupt_set(handle)` is callable from any
   thread and stores `true` with a relaxed store. `Enter` and `Poll`
   read the cell with a relaxed load. If the flag is set, the runtime
   records the `Interrupted` trap at the intrinsic's position and the
   trap-stop path returns to the host. `subscript_rt_ctx_clear_trap`
   clears the flag with the trap. *(Amended 2026-09-17, after the
   Phase Review: the first text stored through the Context pointer
   from the second thread, inside the owner's exclusive borrow.)*
2. **Allocation quota.** The §21 allocation path compares
   `live_bytes` plus the request against the quota before it
   allocates. Over the quota, it takes the §21 fault path with
   `AllocationQuota` in place of `AllocationFailure`. Strings, arrays,
   maps, sets, and objects all pass through that path, so no separate
   size limit exists. The runtime keeps `live_bytes` as a counter it
   maintains at every allocation, release, retention, and collection,
   in both memory modes. The quota check reads the counter and costs
   the same at every live count. A debug assertion compares the
   counter against the fold over the live set at every collection,
   and a test compares the two after each kind of change. *(Amended
   2026-09-16, after round 2: the first implementation folded over
   the live set at every allocation, measured quadratic at 10,000 and
   20,000 live allocations.)*

   **No buffer sized by script input exists outside the quota.** A
   runtime operation whose result size follows from its inputs
   computes that size first and allocates it through the Context
   (`alloc_str_with`, or the array and map paths), which checks the
   quota before any byte exists. Where the size is not known before
   the bytes are built, the buffer grows inside the quota's headroom
   (`Context::quota_headroom`, the bytes one more allocation can
   take): a temporary bounded by that headroom holds nothing past it,
   and the operation then reports the total it wanted to
   `check_quota`, which records the same trap the final allocation
   would have. The temporary is not itself a Context allocation, so
   `live_bytes` and the trap position do not change. The headroom is
   in reserved bytes: the largest payload that fits is the headroom
   less the header and the record in the exact-size mode, and the
   largest size class at or under the headroom in the arena mode. A
   growing temporary holds at most twice its charge while its vector
   doubles. Two entries keep a bounded
   multiple instead: `toUpperCase` and `toLowerCase` build at most
   three bytes per receiver byte through the standard library's
   locale-free case mapping, and `JSON.parse` builds a transient
   document of about 40 bytes per node from an input the quota holds.
   Each charges its multiple against the headroom before it builds,
   so the bound of §109.0 holds for them too. *(The charge for these
   two lands with the M6 round.)* A total check holds this: a test binary with a
   counting global allocator drives every `subscript_rt_*` entry whose
   result size a script controls, under a small quota and a huge
   request, and asserts the peak allocation stays under the quota plus
   a fixed slack; the same test derives the entry list from `ffi.rs`
   and fails on an entry that is neither covered nor listed with a
   reason. *(Amended 2026-09-17, after an external review:
   `String.repeat` built the result in a `Vec` before `alloc_str`,
   measured at 259 MiB resident for a 256 MiB request under a 64 MiB
   quota, before the trap.)*
3. **Stack budget.** `enter_script` at depth 0 records the address of
   a local as the floor. `subscript_rt_sandbox_enter` compares the
   address of its own local against the floor minus the budget. Below
   the floor minus the budget, it records `StackBudget`. The check
   runs after the frame of the entered function exists, so the
   overshoot past the budget is at most that one frame, which S027
   bounds at 65,536 bytes, plus the runtime's own call depth, which
   is under 65,536 bytes. The host sets a budget at least 131,072
   bytes below the thread's stack size. That is the host's fact,
   stated in the host tutorial. *(Amended 2026-09-17: the first text
   named no headroom.)*
4. A trap under the profile is an ordinary trap: first trap wins, the
   observer fires, the Context survives, the host reads it (§18.2).
5. The limits are the host's facts, not the program's. LIR carries
   the two intrinsics and no limit. Every runner, the CLI, and the
   interpreter harness set the limits the way a host does, through
   the C API or the runtime's own setters, before the entry runs.

### 109.5 Defaults under the CLI and the corpus harness

`subscript --profile sandbox` and a corpus entry with `profile:
sandbox` set the quota to 67,108,864 bytes and the stack budget to
524,288 bytes before the entry runs. A host embedding the runtime
sets its own. A Rust host, and every in-repo runner and benchmark,
sets its own through `RunConfig`: `alloc_quota` and `stack_budget`,
each `Option<u64>`, and a set value replaces the default. The
interrupt has no default; nothing sets it unless the host does. A
Rust caller of the in-process dev runner reaches the run's interrupt
cell through `RunConfig.interrupt_handle`, a sink the runner fills
before the first script call; the forked runner and the ship runner
refuse it, because their Context is in another process.
*(Amended 2026-09-17: round 3 could not measure `callbacks` because
no runner took a host quota.)*

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
| `t-sandbox-alloc-quota` | trap | `AllocationQuota` at the allocation site, output before the trap intact. The entry reaches the quota with allocations of 4 KiB or more, so the two tiers' reserved sizes agree within 3% and the trap lands on the same allocation in both. A trap entry that reaches the quota with small objects is not constructible: the tiers charge them differently (§109.0). |
| `t-sandbox-stack-budget` | trap | `StackBudget` at the entry of the function the run failed to enter; the entry prints nothing that depends on the depth reached |

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

### 109.8a Memory under the profile is reclaimed at a boundary

*(Added 2026-09-17.)* The profile rejects `Context.free`, so
collection is the one way memory returns. Nothing collects unbidden
(invariant 2), and the quota is a stop, not a pacer. Two patterns
keep a long-running profile program under its quota. Both are host
documentation; neither adds a rule to the compiler or the runtime.

1. **The host paces.** `subscript_rt_ctx_charged_bytes` reads the
   counter the quota compares against (§18.2d, §109.0), so a host
   reads it at every frame boundary at no cost. If the value is above
   the fraction of the quota the host chose, the host calls
   `subscript_rt_ctx_collect` there (§18.2d), outside any script
   call. The host, not the script, decides when. `live_bytes` is the
   payload figure and does not predict the trap. *(Amended
   2026-09-17, M6: the first text paced on `live_bytes`.)*
2. **The script collects at its own boundary.** `Context.collect()`
   stays callable under the profile. A script that calls it at the
   end of its frame function keeps its own live set bounded, and the
   host's pacer is the backstop.

The host tutorial shows both with a measured run, and
`examples/sandbox/` carries the program. A collect is stop-the-world
mark-sweep, proportional to the live set plus the dead set; a host
that needs a bound on its length has no rule here yet, and asks for
one with a measurement.

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
