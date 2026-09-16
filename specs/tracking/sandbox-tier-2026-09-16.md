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

Pending.
