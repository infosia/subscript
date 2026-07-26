# P19 — trap unwind parity. COMPLETE 2026-07-26

Contract: `specs/blocks/compiler.md` §19. Opened as its own phase
because the defect predated P13 and P18 and was larger than either.

## What was wrong

The two tiers executed different amounts of code between a fault and
the stop. Measured: **14 of 19 trapping programs differed in stdout**
between dev-JIT and ship-C-AOT. Trap tuples agreed everywhere; only
what happened before the stop differed.

The bound was the end of the enclosing function, not one statement — a
loop containing a fault ran to completion, execution entered another
script function and finished its body, and an array was pushed to
twice after the fault, so **live Context state diverged too**.

The continuation path also corrupted memory: an out-of-range write to a
320-byte `@CStruct` element went through `ss_arr_at`, which recorded
the trap and returned a 256-byte static buffer, and the caller wrote
320 bytes into it. AddressSanitizer: `global-buffer-overflow`.

## Why nothing caught it

Three independent reasons, each sufficient — the founding invariant
had a hole shaped exactly like the class of program that could
demonstrate it:

1. an accept entry must terminate with deterministic output, so a
   trapping program could not be one;
2. `corpus.md`'s trap-category rule, **written the same morning**,
   said the observable result was the trap tuple "not stdout";
3. both `run_jit` and `run_c_aot` discarded the sink on the trap path.

## Staging, and why gate-first

**Stage B** widened the gate and committed **intentionally red** —
`codegen/tests/cemit.rs` failing 11 of 67, everything else green. That
is the Red half of the workflow, pre-registered in §19.5. **Stage A**
made it green.

Doing A first would have left no way to demonstrate the fix, and the
red commit is now the record of what the defect looked like.

## What landed

`Callee::can_trap()` in `compiler/src/hir.rs` is the shared policy for
calls, consumed by both lowerings. `ss_arr_at`, `ss_fa_at` and
`ss_scratch` are gone, their checks expanded at the call sites — the
only shape that closes the corruption, since a C function cannot make
its caller return. One inline Context-layout site, now machine-checked
against `Context::trap_flag_offset()`.

No loop back-edge polling: the implementer **checked the dev tier
rather than assuming**, found it has none either, and measured that
ship-only polling cost time for no parity gain.

## Performance — the phase made emitted C faster

| tree | out-of-line checks | inline checks | ×C |
|---|---:|---:|---:|
| pre-P13 (`4486b8d`) | **0** | 0 | 1.87× |
| post-P13 (`a7a4ea8`) | 15 | 0 | 3.74× |
| pre-P19 (`a757939`) | 15 | 0 | 3.65× |
| post-P19 | 1 | 24 | **1.53×** |

Bisected by the orchestrator on one machine, C baselines agreeing to
1.7%, checks counted directly in `a22`'s emitted C.

**The emitted C had no trap check at all before P13.** P13 added the
checking C6 requires and paid for it out-of-line; P19 fixed the form
and widened coverage — 25 checks against 15, and 2.4× faster — ending
faster than the tree that had none.

So the **1.05× figure CLAUDE.md cites is not a target to return to**:
it was measured on an emitter that did not do the checking the language
requires. Comparing 1.53× against it compares two correctness levels.

## Phase Review — 2 CRITICAL, 1 MAJOR, 4 MINOR. All closed.

**Both CRITICALs were introduced by P19 itself**, which is the reason
the review exists.

- **CRITICAL 1**: the div/rem guard interpolated the divisor twice, so
  a call-valued divisor was called twice. `100 / ((Math.random() * 3 +
  1) as i32)` advanced the PRNG twice on the ship tier — the founding
  invariant failing on an **accepted, non-trapping** program. No golden
  had that shape. `a73` now does.
- **CRITICAL 2**: compound assignment to an array element evaluated the
  RHS before the bounds check, diverging in stdout *and in the trap
  tuple* when both sides faulted. The comment above it described the
  dev tier's order correctly while the code below contradicted it.
  `t07` pins it.
- **MAJOR**: §19.7 claimed trap-capable operations are checked in both
  tiers "by construction". True of `Callee` variants, false of the
  ~10 non-call sites. Claim corrected; see the open item below.
- MINORs: a broken C6 parenthetical, a wrong performance mechanism
  (below), an unchecked layout dependency, and a `corpus.md` note
  written as a story rather than as fact-evidence-consequence.

The review confirmed under adversarial probing — 34 hand-written
programs, ASan, emitted assembly — that the scratch-buffer removal,
the single layout site, the absence of back-edge polling and
non-trapping stability all hold.

## Corrected: the performance mechanism

The contract first recorded `ss_arr_at` as "an out-of-line call per
array element access". **Wrong** — it was `static` and clang inlined
it. The reviewer rebuilt the removed helper and compiled both shapes
with the ship tier's own flags.

The cost was what the **fallback pointer** forced: a null compare and
`csel` per access choosing between the returned pointer and
`ss_scratch`, that global's address held live, a reachable cold-arm
call, and therefore an 80-byte frame with callee-saved spills. 82
instructions against 39.

**The helper cost leafness, register pressure and alias freedom — not
a call.** Worth keeping, because the wrong version was plausible and
would have misdirected the next optimisation.

## Open, carried forward

**About ten non-call trap sites remain two-place policy** — integer
div/rem, index read and write, `JsonResult.value`, narrowing casts,
use-after-delete, stale-coroutine, allocation-bearing literals — each
hard-coded separately in both lowerings. **Both of this phase's
CRITICALs were instances of that duplication failing**, so this is a
demonstrated hazard rather than a theoretical one.

A boolean predicate cannot finish it: these sites need the proof-based
elision of a proven index, the two resolutions a compound assignment
requires, the several trap points inside a template or array literal,
and the position each guard reports. Estimated: a shared classification
plus a both-tier coverage test, ~0.5–1 day; an explicit
checked-operation IR, ~2–4 days. **Owner decision pending.**

Two pre-existing defects the review found, both reproducing before
P19 and recorded rather than bundled in: `xs[i] += "s"` on a `string[]`
emits invalid C (`void*` addition, no `Type::Str` arm), and
`(xs[i] += v)` in expression position fails the ship tier with an
internal lowering error where the dev tier compiles it.

## Gate

`cargo build --offline --all-targets` zero warnings;
`cargo test --offline` **560 passed, 0 failed**; `tsc` exit 0;
`git diff --check` clean; no accept-corpus `.expected` moved by the
fix; ASan clean on the 320-byte case that previously overflowed.
