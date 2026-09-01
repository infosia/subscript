# warnings — a second severity for accepted programs

Status: contracted 2026-07-30 (owner decision); implementation follows.
Evidence lands in `specs/tracking/cli.md` (the surfacing) and this
file's exit criteria.

## 1. What a warning is here

A warning marks a program that is **legal but probably unintended
given this language's memory model** — the class invariant 2 creates,
where a program is "correct, merely larger". Two lines bound the
space permanently:

- **Nothing stock `tsc` already reports.** Unused locals, unreachable
  code, and their kin are the editor toolchain's job (invariant 5);
  duplicating them is debt. Warnings live only in the semantic layer
  `tsc` cannot see: sized integers, the memory model, the two tiers.
- **Nothing soundness.** Unsound is an error (S-code) and stays one.
  Warnings never gate acceptance and never appear for a rejected
  program: they are computed on the checked HIR, after acceptance.

Not a subcommand. Warnings surface wherever checking runs — `check`,
`emit`, `build`, `run` — exactly as errors do (cli.md §8). A separate
lint command becomes justified only if a future analysis is too
expensive to run on every check; none of the codes below is.

## 2. The W-code table

W-codes are stable and never renumbered, like S-codes
(compiler.md §6). Each has a one-line explanation string, like
`RuleCode::explanation()`.

### W001 — per-iteration allocation never released

Fires on a `new` of a reference class **inside a loop body** when, on
every path in the iteration, the result (a) does not escape the
iteration and (b) is not released.

- *Escape* (conservative — any of these mutes): returned; assigned to
  module state, an outer-scope binding, a field, an array element, or
  a map/set; captured by a lambda; passed as an argument to any call
  other than `Context.free` on it. Field/element *reads* are uses,
  not escapes.
- *Release*: a `Context.free` whose argument is the binding. Any
  `Context.collect()` call in the enclosing function mutes every
  W001 in that function (v1 conservatism, recorded: collect anywhere
  in the function is taken as "this function manages its garbage").
- Why the loop restriction: `examples/e03-memory.ts` teaches the
  one-shot unreleased allocation as *correct* (invariant 2 —
  reclaimed at Context release). What the owner rejected
  (`specs/tracking/dev-retention.md`) is growth **per unit of work**.
  A loop-carried unreleased allocation is that shape; a one-shot one
  is not.

### W002 — use after `Context.free`

Fires on a use of a local binding after a `Context.free(binding)`
statement in the same block, with no reassignment of the binding in
between. Straight-line and same-block only in v1 — no cross-branch
join reasoning; a use only reachable through a different branch does
not fire. The dev tier's freed-handle diagnostics (compiler.md
§8.1a-1) catch the dynamic remainder; W002 is invariant 6's "clear,
early error" for the statically obvious case.

### W003 — fresh userdata registered in a loop — Rev 2026-07-30

Fires on a callback-info aggregate **constructed inside a loop body**
with a userdata slot holding an allocation made in the same
iteration (a `new` expression as the argument, or a local whose
initializer is a `new` in the same loop body — W001's tracking
rules). Rationale: binding identity is `(code, userdata1, userdata2)`
(compiler.md §14.4a), so a fresh userdata address per iteration
interns a new record per iteration, and §14.4b roots its userdata —
growth per iteration with no release verb, hence no mute.

Recorded limits, both deliberate: a bounded setup loop registering
one sink per item is a **known false-positive class** (legal,
data-bounded; indistinguishable statically; the suppression story
stays §7's deferral) — and mutating an existing aggregate's userdata
field per iteration is a **known miss** (v1 anchors on
construction). The dynamic half — the host-driven per-frame shape no
static analysis can see — is compiler.md §14.4b (B2).

### W004 — write to a value copy that nothing reads — 2026-09-01

Fires on an assignment (plain or compound) whose target chain roots in
a **copy binding** of a value type — a `@CStruct` class or a
`FixedArray` — when the binding is
**write-only** in its function: every occurrence of the binding after
its declaration is the root of an assignment target. A copy binding is
one of:

- a value-typed parameter of a function, method, constructor, or
  lambda (copy-on-pass, C2);
- a `let`/`const` local of value type whose initializer is a place
  — a local, a global, an index expression, or a field chain rooted in
  one of those or in `this` (copy-on-assign and copy-on-index, C2);
- a `for...of` loop binding of value type (a copy per visit, C13); its
  origin is the subject rendered as source text.

`this` inside a value-class method is an address (compiler.md §68 LIR
`Method` row), not a copy, and never fires. A local whose initializer
is `new` or a call holds a fresh value, not a copy, and never fires.

*Read* (any one mutes every W004 on the binding): a field or index
read, a method call on the binding, passing it as an argument,
returning it, using it as an assignment value, or capturing it. The
root of an assignment target in statement position is not a read.
Statement position is an expression statement or a `for` step, where
the value is discarded. An assignment in value position (`const v =
p.x++`) reads the binding.
The rule is order-free on purpose: a binding with one read anywhere in
the function stays silent, so a loop that reads before it writes stays
silent.

*Shadowing.* HIR `Local` carries a name, not a binding id, so the pass
cannot tell two same-name bindings apart. A name bound more than once
in one function body (a parameter shadowed by a `let`, or two `let`s
in sibling blocks) is never a W004 candidate. Recorded miss, and a
form fact (compiler.md §68 does not give locals an identity).

*Rendering.* An index expression that is not itself a place renders as
`…` inside the copied place (`arr[…]`). A `for...of` subject that is a
call renders as its callee with `(…)` for the arguments
(`scores.values(…)`); any other non-place subject renders as `…`.
A checker-synthesized local (a name that starts with `[[`) is never a
copy binding and never a shadowing site.

Why: `tsc` sees a shared object and cannot report this; C2 makes the
write land in a copy and the effect vanishes. Downstream request R38
shipped a drag interaction with this shape (compiler.md §81). Recorded
misses, deliberate: a second write after a read (`b.x = 1; print(b.x);
b.y = 2;`) stays silent; a write to a captured copy inside a lambda
stays silent, because the capture is the read.

Position: the assignment. Message names the binding and its origin —
the parameter, or the copied place.

## 3. Surfacing (CLI)

- Rendered by the §8 (cli.md) renderer shape with `warning[W001]:`
  headers, `= rule:` from the W-code explanation, and a final
  `warning: N warning(s)` summary. No ANSI color (same rule).
- Warnings go to stderr. Exit codes unchanged: warnings alone exit 0
  and artifacts are produced. `check`'s
  `check: <file>: no errors` line appears only when there are
  neither errors nor warnings.
- `--deny-warnings`, accepted by all four subcommands: after
  printing, warnings become exit 1 and `emit`/`build` produce no
  artifacts.
- Errors and warnings never mix: a rejected program reports errors
  only (§1).

## 4. Corpus arm: `corpus/warn/`

Corpus-first applies: a warning without a corpus entry is not
decided. Each W-code has at least one firing entry in `corpus/warn/`
with its (code, line) pinned by a harness, exactly as
`corpus/reject/` pins S-codes. Every entry is an **accepted**,
`tsc`-clean program (it must be — warnings only exist for accepted
programs), and joins the `tsc` gate.

The non-firing net is broad, not per-entry: `corpus/accept/` and
`examples/*.ts` produce **zero warnings** under check. That is the
precision requirement — `e03-memory.ts` in particular must stay
silent, and `a16`-style explicit-collect loops must stay silent via
the collect mute.

## 5. API constraints

`check_program`'s signature, behaviour, and every existing caller are
untouched (the reject corpus and both lowerings depend on it). A new
compiler entry returns the checked module's warnings; only the CLI
consumes it in v1. `Diagnostic` and its `Display` are untouched;
warnings are their own type carrying (W-code, message, `Pos`).

## 6. Exit criteria (pre-registered)

1. `corpus/warn/` harness: every entry fires its pinned (code, line);
   entries are accepted programs and `tsc`-clean.
2. Zero warnings over all of `corpus/accept/` and `examples/*.ts` —
   including `e03-memory.ts` (one-shot) and the explicit-collect
   loop entries (mute).
3. CLI: exact-output test for a W001 rendering; warnings-only runs
   exit 0 with artifacts; `--deny-warnings` exits 1 and `emit` leaves
   no artifact behind.
4. All four subcommands print byte-identical warning text for the
   same program.
5. Every W-code explanation is non-empty (full-enum test).
6. `check_program` callers unchanged; reject corpus untouched; full
   gate green.

## 7. Out of scope (this contract)

Suppression directives (`// subscript-allow(...)` needs its own
design — it must stay `tsc`-clean); cross-branch or interprocedural
reasoning for W002; escape-analysis refinements beyond §2's
conservative list; additional codes (large value-class copy in a
loop was considered and deferred — it needs a size threshold
decision); a standalone lint command.
