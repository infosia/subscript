# §66 — emitted C identifiers, two spaces

Status: **landed 2026-08-25** against `specs/blocks/compiler.md`
§66. Origin: the R37 phase review, then an audit of the defect class
it exposed. Owner decision 2026-08-25. Contract `d9f7bbc`, amended `6ca8201`, `73bcab0`, `b86db0a`,
`a296b05`, `213d1f6`, `0fea497`, `105fb7a`,
implementation `d93f8eb`. This is not a downstream request, and no
language surface moved.

## Why

The R37 review found one collision between a declared member and a
symbol the emitter derives by suffix: an async method `x` beside a
method `x_resume` produced two definitions of
`subscript_m0_x_resume`. That one is loud. The audit that followed
found the same defect class at function scope, where it is silent.

## Findings at `a2228d9`

All pre-existing. R37 introduced none of them.

1. A parameter named `_t0` made the two tiers disagree with no
   diagnostic: the dev tier printed `306` and the ship tier printed
   `12`. A parameter named `_t1` printed `306` and `8`. `fresh_tmp`
   mints `_t0` inside a nested C block, so it shadows the parameter
   and the body reads the temporary where it means the parameter.
   Two declarations in one C scope are an error; two in nested
   scopes are silent.
2. An async method `x` beside a method `x_resume` stopped the C
   compiler. It stays loud when the two signatures are identical:
   two definitions are always a redefinition.
3. A method parameter named `_this` stopped the C compiler.
4. `ctx` was already safe through `is_c_keyword`, which held the C
   keywords and one further entry. The mechanism existed; the list
   did not keep step with the emitter.
5. The dev tier is immune: it names a method by index. Every
   divergence in this class is one-sided.
6. `_resume` is the only symbol built from a source name by suffix
   (audit of every symbol constructor in `codegen/src/cemit.rs`).

## What landed

Source space and emitter space are now separate. A function-scope
source identifier takes the prefix `v_`, and the emitter mints no
function-scope identifier that starts with `v_`. One table per
function covers the parameter names and every local name in the
body, reusing the `walk_lets` walk that already builds the coroutine
frame, and resolving a collision with the §65 rule 10 `_N` logic. A
coroutine frame's parameter members take the same prefix, so they
cannot collide with `_state`, `_this`, or `g{i}`. The derived
`{name}_resume` symbol enters the method or function table as a
synthetic entry beside `{name}`, so the same `_N` logic resolves it.

The C name of a local stays a function of its source name alone, so
C block scoping still reproduces the shadowing the language permits.

Corpus: `a145-emitted-identifiers`, which names parameters and
locals `_t0`, `_t1`, `_this`, `_frame`, `_out`, `_f`, `_state`,
`g0`, `_L0`, and `ctx`, reads them in nested blocks and in a loop so
the emitter's temporaries interleave, and holds `x`/`x_resume`,
`f`/`f_resume`, and an async function with the parameters `_state`
and `g0`. Counts: accept `.ts` 143 -> 144; `.expected` 144 -> 145;
accept source files 145 -> 146.

## Gates (this host, at `d93f8eb`)

- `cargo test --offline --workspace`: 59 suites, 1022 passed, 0
  failed, 1 ignored, in both profiles. Wall time 270 s (debug) and
  215 s (release).
- `cargo build --offline --workspace --all-targets`: 0 warnings.
- `cargo fmt --check`: exit 0. `tsc` 5.9.2 gate: exit 0.
- Clippy library counts at the baseline: 7 / 22 / 29.
- Every committed golden and `.expected` byte-identical, `a145`
  excepted as a new entry. The seven changed assertions in
  `codegen/tests/cemit.rs` are prefix changes and nothing else.
- The `a145` golden was checked by arithmetic, not by rerunning the
  compiler that produced it: 55 * 3 + 300 = 465 and 155 * 3 + 300 =
  765 for the two probes.
- The three findings above re-run clean: `_t0` and `_t1` now print
  `306` on both tiers, and both loud cases compile.

## Review (fresh no-context subagent)

Seven implementation rounds and seven no-context reviews. Reviews
one to six each found a defect the round before had missed; review
seven returned PASS with no CRITICAL and no MAJOR. The pattern that
made each round incomplete is worth one line: **a fix that is
locally correct can leave the same defect at a site the review's
test set never reached.** The search became exhaustive only at
review five, which paired every scope opener in the checker against
every block site in the emitter; after that pairing the remaining
work was finite and the rounds converged.

The first round closed the three findings above and
passed every gate. The phase review then found one CRITICAL and one
MAJOR that the first round did not close, and both were reproduced
here before the fix.

- CRITICAL-1, contract defect. Measurement 8 was taken on an `i32`
  local and generalized to every local. A managed local — a string,
  a reference class, or an aggregate that holds a handle — has no C
  identifier, so C block scoping cannot apply to it, and
  `emit_block` restored no scope state. Measured: `const s: string =
  "outer"` with an inner `const s: string = "inner"` printed `inner`
  then `outer` on the dev tier and `inner` then `inner` on the ship
  tier, with no diagnostic; a reference-class local printed `2`,`1`
  and `2`,`2`. Measurement 8a records the correction and rule 3a
  requires the scope restore, which `emit_for_of` already did.
- MAJOR-1. The lambda environment struct is a C namespace with no
  table: a lambda that captures `a$b` and `a_dollar_b` emitted two
  members of one name and the C compiler stopped. Loud. Rule 3b
  gives the struct a table at the declaration, the store, and the
  read.
- MINOR-1. The first round moved a free function's resume symbol
  from the prefix form `subscript_resume_{name}` to a suffix form.
  The prefix form was already collision-free, so rule 6 forbade the
  move; rule 4 now states that only the method resume is
  suffix-formed. Four further MINOR: no `///` docs on the new
  machinery, duplication between two builders, a unit test that
  asserted a proxy for the stated property, and one unreachable
  condition.

Verified with no finding across 39 programs run on both tiers: every
emitter-owned name as a parameter and as a local, in nested blocks,
loops, lambdas, generators, async functions and methods,
constructors, `using`, worker entries, `for...of`, `switch`, and
generics; `v_t0` beside `_t0`; `v_x` beside `x`; `$` beside
`_dollar_` beside `_dollar__2`; module globals named `_t0`, `g0`,
and `v_x`; and the resume symbol for an async method, an async free
function, and a generator free function, in both declaration orders.

## Adjacent defects, not fixed here

Both are dev-tier and ship-tier disagreements on ordinary programs.
Neither is an identifier problem, and this section does not touch
either. Both were reproduced here.

1. Two `await` expressions in one template literal. The dev tier
   stops with "internal lowering error: define async resume:
   Compilation(Verifier(... uses value v27 from non-dominating
   inst38))". The ship tier compiles and prints a wrong first value.
2. A capturing lambda created before a suspension and called after
   it. The emitted C puts the environment on the resume function's
   stack, so the stored pointer dangles once the resume returns. The
   dev tier printed `-1299693456` and the ship tier printed `1`,
   where `7` is correct. Both tiers are wrong, and they disagree.

3. `switch` case scoping. The checker gives each case its own scope;
   TypeScript gives the whole body one scope. A program that
   declares a name in one case and reads it in another is accepted
   here and rejected by stock `tsc` (TS2454), which breaks
   invariant 5, and the flat C block then makes the tiers disagree
   with no diagnostic: `case1:1` on the dev tier, `case1:99` on the
   ship tier. **Owner decision 2026-08-25: the checker moves to the
   TypeScript rule in the next cycle** (`compiler.md` §66
   measurement 6e).

4. Two `await` expressions in one expression, a capturing lambda
   held across a suspension, `await` or `yield` inside a `for...of`
   body, and two generators of one yield type in one module. All
   four are dev-tier or ship-tier failures on `tsc`-clean programs,
   and none is an identifier or scope problem.

**Owner decision 2026-08-25: items 1 to 4 and the three deferred
measurements land in one cycle**, in two passes — the checker
semantics (`switch` scope, TDZ name resolution, duplicate
declaration) and the async and generator lowering. One contract,
two implementation and review passes, because a single review does
not cover that surface: the lesson of this section's own seven
rounds.
