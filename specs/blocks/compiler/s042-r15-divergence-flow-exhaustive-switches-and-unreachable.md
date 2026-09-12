<!-- §42 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 42. R15 — divergence flow: exhaustive switches and `unreachable()`

Owner decision 2026-08-02 (downstream R15; 15.1 blocking, 15.2
answered as a design decision). Probed before contracting
(`--lib es2022` standalone, exit 0): the R15.1 function shape and a
`declare function unreachable(): never` ambient are both
stock-`tsc`-clean — `tsc`'s own flow analysis accepts an exhaustive
alias switch with all-returning arms and treats the `never` call as
diverging, so the two rules below only align this compiler with
what `tsc` already concludes.

### 42.1 R15.1 — exhaustive alias switches in return-flow analysis

A `default`-less `switch` over a Q32 alias (§41 guarantees the
cases cover every member) counts as **diverging** in the
all-paths-return analysis when every case arm diverges (returns, or
ends in a diverging statement per this section). No runtime or
type-system change; arms that fall through to a `break` keep the
existing behavior (the switch then does not diverge). Enum,
integer, and string switches are unchanged — they have `default`-
free coverage no analysis can prove.

### 42.2 R15.2 — `unreachable()`

The prelude gains `declare function unreachable(): never;`. The
call is a statement (value position rejects — r115); flow analysis
treats it as diverging; at runtime it traps with the new trap kind
`unreachable-reached` (23) under C6's trap-stop semantics,
identically in both tiers. This is the intended spelling for
provably-dead paths in generated code — the bounds-check idiom the
downstream considered is not the designed answer. Dead-code
detection after a diverging statement is not added (existing
behavior unchanged).

### 42.3 Corpus

`a116-exhaustive-switch-returns` (accept): the R15.1 function shape
verbatim (every arm returns, no trailing return) plus a function
whose tail is `unreachable()` after early returns — the a115
assign-and-break concession removed. `t47-unreachable-reached`
(trap): a reachable `unreachable()` call traps with kind 23,
golden-pinned under both tiers. `r115-unreachable-as-value`
(reject): `unreachable()` in value position (`tsc` status probed
and recorded either way).

### 42.4 Exit criteria (pre-registered)

1. `a116` byte-identical under both tiers; `t47` traps with kind 23
   under both tiers, golden-pinned.
2. `r115` pins (code, line) with its `tsc` status recorded.
3. Flow unit tests: all-arms-return exhaustive switch accepted; one
   arm ending in `break` still rejects all-paths-return; a
   `default`-bearing switch unchanged; `unreachable()` satisfies
   return flow at a function tail.
4. Full gate green; `tsc` gate green with the prelude addition and
   `a116` included; no existing golden moves; zero-warning sweep
   green; generated-docs byte-compare gates green.
