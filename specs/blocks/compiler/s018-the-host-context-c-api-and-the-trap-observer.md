<!-- §18 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 18. The host Context C API, and the trap observer

Owner decision 2026-07-26. **This section exists partly to close a
gap**: the `subscript_rt_ctx_*` surface is what an embedding host actually
calls, and it had no contract anywhere under `specs/` — it existed only
in `runtime/src/ffi.rs`. A host-facing ABI with no written contract is
the one surface where drift is least acceptable, since the host is
outside this repository and cannot be fixed by a commit here.

### 18.1 The existing surface, contracted retroactively

```c
void            subscript_rt_ctx_release(subscript_rt_context*);
const uint8_t*  subscript_rt_ctx_stdout(const subscript_rt_context*, uint64_t* len);
void            subscript_rt_ctx_seed_random(subscript_rt_context*, uint64_t seed);
void            subscript_rt_ctx_set_now(subscript_rt_context*, int64_t ms);
uint32_t        subscript_rt_ctx_trap_kind(const subscript_rt_context*);
uint32_t        subscript_rt_ctx_trap_pos_id(const subscript_rt_context*);
const uint8_t*  subscript_rt_ctx_trap_message(const subscript_rt_context*, uint64_t* len);
```

`seed_random` (`stdlib.md` §2) and `set_now` (§3) pin the two
nondeterministic inputs so tests and replays reproduce. The three
`trap_*` accessors are **post-hoc**: after a run returns, the host
reads the fault that stopped it. `Context::trap` records the **first**
trap and ignores later ones, so what the host reads is the
originating fault, not whatever was last seen while unwinding.

`pos_id` is an index into the compiler's position table; resolving it
to a TypeScript position needs that table, which is a separate
artifact from the Context.

### 18.2 The trap observer — observation only

```c
typedef void (*subscript_rt_trap_observer)(
    void* userdata, uint32_t kind, uint32_t pos_id,
    const uint8_t* message, uint64_t message_len);

void subscript_rt_ctx_set_trap_observer(
    subscript_rt_context*, subscript_rt_trap_observer observer, void* userdata);
```

Called at the moment a trap is recorded, **before** the unwind, on the
trapping thread. Passing a null observer clears it.

**What it adds over §18.1**, honestly and only this: the host learns
while *its own* state is still current — which frame, tick, or entity
it was processing. Once the run has returned, that context is gone and
the post-hoc accessors cannot recover it. Secondarily, the host stops
having to poll the trap flag after every call into script.

**Observation-only is enforced by shape, not by documentation:**

- The observer returns `void`. There is no encoding for "continue", so
  C6's rule that trapping is not catchable survives by construction
  rather than by promise. This is not a recovery mechanism and no
  later revision may make it one without revisiting C6.
- It is handed **no Context pointer**. *(An earlier revision of this
  section said this made re-entry "structurally impossible". That was
  an overclaim, corrected 2026-07-26 after the implementer pointed it
  out: `userdata` can carry a Context pointer, so the signature
  withholds the means without preventing the act. Calling through a
  smuggled pointer is undefined behaviour, not a rule violation — see
  §18.2a — and the honest statement is that the shape removes the
  obvious path, not every path.)*
- It fires **at most once per run** — `Context::trap` is first-wins.
- It must not call back into script. The Context is trapped; re-entry
  is undefined and the host is responsible for not attempting it.
- It cannot change script-visible output, so §0.3 determinism and the
  golden corpus are unaffected. The observer receives copies and no
  mutable Context handle.

### 18.2a Exactly when it fires, and what is true at that moment

The sequence, from fault to the host getting control back:

1. A runtime function, or an emitted check calling `subscript_rt_trap`,
   detects the fault.
2. `Context::trap(kind, message, pos_id)` stores a `TrapRecord` **if
   none is stored yet**, and sets `trap_flag = 1` **unconditionally**.
3. **The observer fires here.**
4. The runtime function returns a placeholder to generated code.
5. Generated code checks the trap flag after every **fault-capable**
   call and, seeing it set, pops its shadow frame and returns early —
   zero for a non-`void` return, `1` for a generator. *(Corrected
   2026-07-26: this said "after every script call", contradicting
   §18.2b two sections later. The dev tier implements fault-capable;
   the ship tier implements neither, which is P19 — §19.)*
6. Every caller repeats step 5, up every live frame.
7. The tier entry reads the record and reports `RunError::Trap`.

At step 3, therefore:

- **Every script frame is still live.** Nothing has unwound; the host
  call site that entered script is still below on the stack.
- **The record is already stored and the flag already set.** The
  observer sees exactly what the §18.1 accessors will report later,
  not a provisional state.
- **The shadow stack still holds every frame**, since `shadow_pop`
  happens during the unwind at step 5.

**The call must sit inside the `trap.is_none()` branch, not beside
it**, and the difference is observable. Runtime functions stay callable
on the unwind path and may detect further faults; those later
`trap()` calls are ignored for the record but still set the flag.
Firing outside the branch would report them too, contradicting
"at most once per run" and handing the host arguments that do not
match the record.

**Message lifetime.** The observer receives a pointer into the stored
record, which lives on the Context until it is released or reset — not
a borrow valid only for the callback. The host is not obliged to copy.

**Re-entrancy is a memory-safety requirement, not a convention.** The
observer runs inside `Context::trap`, which holds `&mut self`. An
observer that calls any `subscript_rt_*` function taking the Context creates
an aliasing violation across the FFI boundary — undefined behaviour,
not merely a semantic error. The C header must say so.

**Cost when no observer is registered:** one null check *per trap*,
not per call. Traps are rare; this is not a hot path.

**Implementation.** All trap sites — 70 across the runtime at the time
of writing — funnel through the single `Context::trap`, so the
observer has exactly one call site. Nothing in generated code changes,
and neither tier's lowering is touched. This is also why §18.4's
both-tier test is not checking that the hook mechanism differs between
tiers — it cannot — but that the two tiers **agree on which fault is
the originating one**.

### 18.2b `subscript_rt_ctx_clear_trap` — making a trapped Context callable again

```c
int subscript_rt_ctx_clear_trap(subscript_rt_context*);   /* 1 = cleared, 0 = refused */
```

C6 says the host decides what happens after a trap, and until now it
could not decide "continue": `Context::clear_trap` existed with the
right semantics and a proving unit test, but was **never exposed over
the C ABI** — its only production caller was the reload session. A
host could therefore only release a trapped Context and rebuild.

**Precondition, checked by the function, not left to the caller.**
Clearing is legal only at a host↔script boundary — no generated code
on the stack. Clearing while a script frame is live would resume a run
that has already given up. The function **returns 0 and does nothing**
if the precondition fails; it must not be a documented obligation,
because the one place a host would most naturally try it — inside the
trap observer — is exactly the illegal case (§18.2a: every script
frame is still live when the observer fires).

**Two guards are required, because `script_depth` alone is inert.**
*(Corrected 2026-07-26. This section originally named `script_depth ==
0` as the whole check. The implementer found that nothing maintains
`script_depth` outside the reload session — ordinary `run_jit` and AOT
host entries never touch it — so for the deployment shape that matters
it reads 0 always, the guard always passes, and it passes **from
inside the observer**, which is the exact case it exists to refuse. A
guard that is inert in production is worse than none, because it reads
as protection.)*

1. **An observer-active flag on the Context**, set for the duration of
   the observer call. `clear_trap` refuses while it is set. This is
   trivially correct and addresses the actual hazard directly.

   It is a **backstop, not a supported call**. Reaching
   `subscript_rt_ctx_clear_trap` from inside an observer already requires a
   Context pointer, and §18.2a makes calling any `subscript_rt_*` through
   one from there undefined behaviour. The flag turns the most likely
   such attempt into a defined refusal instead of a resumed run; it
   does not make observer re-entry a defined API, and nothing else
   about §18.2a's prohibition is relaxed.
2. **`script_depth` maintained for real**, which needs the host
   enter/exit API of §18.1a. Until that exists the depth check is
   retained but is not load-bearing, and this section says so rather
   than implying coverage it does not have.

### 18.1a Host enter/exit — making `script_depth` real

```c
void subscript_rt_ctx_enter_script(subscript_rt_context*);
void subscript_rt_ctx_exit_script(subscript_rt_context*);
```

A host brackets each call into an exported function with these. They
maintain `Context::script_depth`, which is what makes "no generated
code on the stack" a checkable condition rather than a comment. The
reload session already maintains the depth internally; this exposes
the same discipline to an embedding host.

They are **not** optional for a host that calls `clear_trap`: without
them the depth is always 0 and §18.2b's first guard is the only real
one. A host that never clears may ignore them.

### 18.1b The host C header

**`runtime/include/subscript_runtime.h`**, generated. *(Until
2026-07-26 there was none: the runtime's declarations were embedded ad
hoc in `AOT_ENTRY_C` and the emitted-C preamble, so a host outside this
repository had nothing to include — in a language whose fourth
invariant is that host interop crosses a C ABI only. Found while
implementing §18.2 and closed in the same phase; the AOT entry, the
emitted-C preamble and the benchmark entry all consume it now instead
of repeating declarations.)*

It is a single generated header covering the `subscript_rt_ctx_*`
surface (§18.1, §18.1a, §18.2, §18.2b, §18.2d) and the exported-entry
convention, **generated from the Rust declarations rather than
hand-written**, on the `bindgen` mirror's principle (§12.2) and
CLAUDE.md core principle 6 — generated code is never hand-edited, fix
the generator. A hand-kept header would drift from the ABI it claims
to describe, which is the failure this section was written to close.

**It clears reporting state and nothing else.** Live allocations stay
live, deleted ones stay deleted, the reload epoch survives, the stdout
sink survives. So:

> **Clearing makes the Context callable again. It does not roll
> anything back.** There is no transaction. A run that trapped
> mid-`update` leaves script data exactly as it was at the fault —
> an entity may be half-written.

The host's three coherent choices, in increasing cost: accept the
damaged state and continue; continue but detach the failing subsystem
(which is what the observer's frame-current context is *for*); or
release the Context and rebuild from `subscript_init`, which is the only one
that restores consistency.

**Not clearing is a silent failure mode worth naming.** The trap flag
stays set, and generated code checks it after every fault-capable
call — so the *next* exported call unwinds immediately, executing
nothing. The host keeps running while the script is quietly dead.

### 18.2c How a host detects a trap at all

Exported functions return `void` or their declared type, **never a
status**, and a trapped call returns the **zeroed** value for a
non-`void` return (`emit_trap_return`). An `update(): i32` that
trapped hands the host `0`, indistinguishable from a legitimate `0`.

**The return value therefore cannot be used to detect a trap.** The
host tests `subscript_rt_ctx_trap_kind(ctx) != 0` — the accessor returns `0`
when no trap is pending and `TrapKind` starts at 1 — or registers an
observer (§18.2) and tests its own flag. This is stated because
getting it wrong is silent: the host reads a plausible zero and
carries on.

### 18.2d Memory accounting — `subscript_rt_ctx_live_*` / `subscript_rt_ctx_reserved_bytes`

```c
uint64_t subscript_rt_ctx_live_allocations(const subscript_rt_context*);
uint64_t subscript_rt_ctx_live_bytes(const subscript_rt_context*);
uint64_t subscript_rt_ctx_reserved_bytes(const subscript_rt_context*);
```

Owner decision 2026-07-26, and this closes a larger gap than §18.2's.
**Invariant 2 — no implicit GC — makes explicit lifetime management
the memory model's centre, and the host had no way to measure whether
it was working.** A script that forgets `Context.free` and leaks a
little every frame is invisible from outside: `Context::live_count`
and `is_live` existed but were Rust-side only. The P15 review found a
container retaining 8.4 MB after churn; a benchmark caught it, and a
production host embedding the same build could not have.

- `live_allocations` — count of live allocations. **Tier-independent**:
  the same program has the same number of live objects on both tiers,
  so a host may compare dev and ship figures and a difference is a
  defect.
- `live_bytes` — payload bytes of those allocations.
- `reserved_bytes` — what the Context holds from the system: chunk
  capacity plus large allocations. This is the figure a memory budget
  is written against, and the one that moves when memory is freed to a
  free list but not returned.

**`live_bytes` and `reserved_bytes` are tier-dependent, and that is
not a defect.** The two tiers have different allocators — the ship
tier (§8.1b) bump-allocates fixed-size blocks in size-class chunks and
therefore rounds a payload up **when it fits a size class** — an
allocation above `LARGEST_BLOCK` is an individual system allocation of
its exact size — while the dev tier allocates exact payloads
throughout. A host comparing byte figures across tiers will see them
differ; only the **count** is comparable. Stating this is the point of
the entry, because a host that assumed otherwise would chase a
non-bug.

Measured on one program (3 allocations, 1 delete), reported as
`(live_allocations, live_bytes, reserved_bytes)`:

```
dev  = (2, 8, 60)   # measured before §8.1a-1; with freed-handle
                    # diagnostics off, the deleted allocation's layout is
                    # no longer reserved
ship = (2, 32, 65536)
```

The count agrees; neither byte figure does.

**Cost.** `reserved_bytes` is O(chunks + live large allocations) on the
ship tier and walks the retained allocation records on the dev tier when
freed-handle diagnostics are on (§8.1a-1) —
cheap, but not O(1). `live_allocations` and `live_bytes` walk live
blocks on the ship tier and are **O(live blocks)** — they are diagnostics, not per-frame counters. The contract
deliberately does **not** add running counters maintained in
`alloc`/`delete`: that would make the figures O(1) at the price of an
invariant that must stay correct across delete, chunk reuse and
`Context.collect()`, and a memory statistic that can itself drift is worse
than one that is slow.

Read-only: none of the three can change script-visible output, so
§0.3 determinism and the golden corpus are unaffected. A host that
makes decisions from them introduces its own nondeterminism, exactly
as reading a clock does; that is the host's to own.

**Gate.** Across both tiers, `live_allocations` agrees for the same
program at the same point. A program that allocates N objects and
deletes M reports N−M. After a trapped run followed by
`subscript_rt_ctx_clear_trap`, the figures are unchanged by the clear —
which is what makes §18.2b's "clearing rolls nothing back" claim
**host-verifiable** rather than only provable inside the runtime's own
tests. `reserved_bytes` never decreases across a `delete` alone
(memory returns to a free list, not to the system), which is the
property that would have surfaced the P15 retention from outside.

### 18.2e Per-allocation attribution — superseded by §21.2

Moved to `compiler-history.md` §18.2e on 2026-09-06; §21.2 holds the
current rule.

### 18.2f The print observer — streaming output without retention

**The stdout sink is cumulative and a C host cannot drain it.** `print`
appends to the Context sink; `subscript_rt_ctx_stdout` is a `const` read
returning a pointer into it; the draining accessor exists only on the
Rust surface, where the in-repo runners call it once at run end. That is
correct for the gate — the golden comparison wants the whole run's bytes —
and unacceptable for a long-running host, which retains every byte its
script ever printed (`specs/tracking/long-run-audit.md`, finding 1).

**The fix is the trap observer's shape applied to print** (§18.2 is the
precedent, deliberately):

    typedef void (*subscript_rt_print_observer)(void* userdata,
                                          const uint8_t* line,
                                          uint64_t line_len);
    void subscript_rt_ctx_set_print_observer(subscript_rt_context* ctx,
                                       subscript_rt_print_observer observer,
                                       void* userdata);

- **When set, each `print` delivers the line to the observer and retains
  nothing.** The sink does not grow. The bytes passed are exactly the
  bytes the sink would have stored, minus nothing — determinism is a
  property of the bytes, not of where they land.
- **When unset — the default — behaviour is exactly today's**: the sink
  accumulates and `subscript_rt_ctx_stdout` reads it. The gate never sets an
  observer, so every golden stands.
- The observer is called during `print`, inside script execution, under
  the same constraint as the trap observer: it must not call back into
  any `subscript_rt_*` API taking this Context (§18.2's aliasing rule, stated
  once there and referenced here).
- `line`/`line_len` are the line **without** the trailing newline the
  sink stores; the observer decides its own framing. Valid only for the
  duration of the call, like the trap observer's message.
- Switching the observer mid-run is allowed (unlike the freed-handle
  diagnostics setting there is no allocation-order invariant); bytes
  printed while unset stay in the sink, bytes printed while set are not
  added to it. A host that mixes the modes owns the seam.

Exit criteria: (1) a runtime unit test proves set-mode delivers each line
and leaves the sink empty, unset-mode accumulates, and a set→unset switch
leaves earlier lines observed and later lines sunk; (2) an FFI test
exercises the C surface both ways; (3) the capstone or a host example
gains the observer so the pattern is taught, with its golden unchanged in
meaning; (4) standing gate green, no golden moved; (5) the generated host
header documents the aliasing constraint and the no-retention property.

### 18.3 Why there is no in-language observer

A script cannot observe its own traps, and this is deliberate rather
than unimplemented. C6 makes trapping uncatchable; **a script-visible
trap observer is the first half of an exception mechanism** — once a
program can see its own fault, the pressure to let it continue arrives
immediately, and the property that "a trap stops the run" stops being
something the rest of the contract can rely on. Q20 (no Invalid-Date),
Q25 (`toFixed` range), Q27 (`shift` on empty) and Q28 (`NaN` in
`stringify`, reading `value` when `ok` is false) all lean on it.
Script-visible state that depends on a fault would also be a
determinism hazard.

### 18.4 Gate

A host-side test registers an observer, runs a program that traps, and
asserts: the observer fired exactly once; `kind`, `pos_id` and
`message` equal what the §18.1 accessors report afterwards; the run
still unwound and the Context is still trapped; and a second trap in
the same run does not fire it again. The same test runs on **both
tiers** — the observer is runtime-side, so both must agree, and a
divergence here would mean the two tiers disagree about which fault is
the originating one. Clearing the observer with null is covered.

Determinism: a corpus entry's output is byte-identical with and
without an observer registered. This is the check that the shape
really is observation-only.

For `subscript_rt_ctx_clear_trap`: a host-side test traps, clears, and calls
script again, asserting the second call **executes** rather than
unwinding on a stale flag; asserts a clear attempted with a live script
frame returns 0 and leaves the trap pending; and asserts that live
allocations, the reload epoch and the stdout sink are unchanged across
the clear — the property that makes "no rollback" true rather than
merely claimed. Both tiers.
