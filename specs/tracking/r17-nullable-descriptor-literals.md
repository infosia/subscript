# §25.3a — descriptor literals through `Descriptor | null`

Status: **landed and verified 2026-08-02** against `compiler.md`
§25.3a. Origin: downstream bug report R17 (blocking its P5 slice E3,
bind groups). The downstream's control table isolated the trigger to
one property — nullable contextual type — before the report; the
reviewer reproduced the Red independently at the pin and confirmed
the diagnosed site (`check/expr.rs` object-literal contextual match
stopping at `Type::Class`, never unwrapping `Nullable`).

## Evidence (reviewer-run)

1. Red: at `4c35d27` the reviewer's minimal probe and the a117
   entry both rejected S100 ("object literals are not in the
   decided surface"; implementer recorded six diagnostics on a117,
   first at its line 31).
2. Fix: the contextual match unwraps `Nullable(Class)`; descriptor
   classes route to the existing descriptor-literal checker; plain
   classes keep S005 nullable or not; the literal's result type
   stays `Class(D)` and existing assignability widens into
   `D | null` — **no lowering change was needed**, matching the
   typed-temporary evidence in the report.
3. `a117-descriptor-literal-nullable-member` byte-identical under
   both tiers: defaulted- and required-nullable members, `{m:{}}`,
   `{m:null}`, omission, and the array-element nesting control.
   `r116-object-literal-nullable-class` pins S005 through the
   nullable position; standalone `tsc` exit 0 recorded (stock TS
   accepts `{}` structurally for an empty plain class through
   `| null` — strictly-narrower pin).
4. Reviewer live probes at the landing: the original Red probe runs
   (`1` then `0` — literal takes the arm, omission keeps the null
   default); the downstream's bind-group layout shape — an array of
   entries with exactly-one-of nullable descriptor members set
   inline — prints `0:true/false` / `1:false/true` under
   `subscript run`.
5. Gate 48 harnesses, 845 passed, 0 failed, exit 0 read directly;
   `tsc` exit 0; no existing golden moved; zero-warning (118 accept
   sources) and generated-docs gates green.

## Recorded for future corpus authors

Observation code for `?`-declared nullable members must be
`tsc`-strict-shaped: presence checks in template position
(`${o.m !== null}`); full narrowed reads belong on required-nullable
members (`m!: D | null`), whose `tsc` type carries no `undefined`.
Loose `!= null` is not available (the language rejects loose
equality).
