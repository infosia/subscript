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
