# §67 — checker scoping, and state that must survive a suspension

Status: **in progress 2026-08-25** against `specs/blocks/compiler.md`
§67. Origin: the three constructs §66 recorded and deferred
(measurements 6e, 6i, 6j), plus four defects its reviews found
outside its subject. Owner decision 2026-08-25: all seven in one
cycle, in two passes. Contract `d889620`.

## Why two passes in one cycle

Pass A is checker semantics: what the language accepts. Pass B is
lowering: what an accepted program does. They share no code and no
corpus entry, and they fail in different ways. §66 needed seven
review rounds because one review did not cover its surface; this
cycle splits the surface instead of the cycle.

## Findings at `a239de7`

Pass A. Each program is one stock `tsc` rejects and this compiler
accepts, so each breaks invariant 5.

| program | dev | ship | `tsc` |
|---|---|---|---|
| `switch` case reads an earlier case's name | `case1:1` | `case1:99` | TS2454 |
| parameter and body local of one name | `7` | C error | TS2300 |
| two `const` of one name in one block | `2` | C error | TS2451 |
| nested lambda reads a name declared later | `3` | `3` | accepts |

The `switch` case is silent: no diagnostic on either tier. The last
row is the only one where the tiers agree; `node` prints `4` there,
so it is a divergence from TypeScript, not between the tiers.

Pass B. Each program is `tsc`-clean, so each is a valid subscript
program that does not run correctly.

| program | dev | ship |
|---|---|---|
| two `await` in one expression | lowering error | corrupt first value |
| capturing lambda across a suspension | `15`, `-1927167400` | `15`, `5` |
| `await` inside a `for...of` body | lowering error | one iteration, stops |
| two generators of one yield type | `1`, `2` | refuses to emit |

The correct output of the second row is `15` twice. Both tiers are
wrong there, and they disagree.

Items 1 to 3 of pass B are one root cause: a value that is live
across a suspension does not live in the coroutine frame. The
Cranelift verifier names it — a value defined before the suspension
is used after it, in a block the definition does not dominate. Item
4 is separate: `generator_of` recovers the resume target by
searching for the one generator whose yield type matches, and its
own comment records that it cannot recover the creator. The dev
tier has no such search; it stores the resume address in the frame
at creation and calls through it.

## The rule that shapes pass A

Where this compiler and TypeScript disagree, this compiler rejects.
It never accepts a program and gives it a different value.
Rejecting more is inside invariant 5; computing a different answer
is not. That is why measurement 4's program becomes a rejection
rather than a semantic change: no accepted program moves.

## Pass A

Landed `1c578f9`. Four rounds, three reviews. Contract `d889620`,
amended `5594c14`, `03e4e24`, `e8e600e`, `8c1ba4f`.

The first round closed all four defects and passed every gate, and
the review found no false rejection in 45 programs: a nested lambda
reading an enclosing-block binding, an inner block declaring its own
name after an outer read, a parameter with a nested-block local of
one name, a `for` initializer read in its condition, a method
reading a module global a later local shadows, mutually recursive
lambdas, and more. It also ran `subscript check` over all 372 `.ts`
files under `corpus/` and `examples/` with a HEAD binary and a
working-tree binary: the diff held only the five new reject entries.

The review found two defects the round had created or missed.

- CRITICAL. **The round opened the divergence it set out to close.**
  The cross-case comparison sat inside the read path, so a
  cross-case *write* was accepted: `subscript check` reported no
  error, the dev tier stopped with "internal lowering error: unbound
  local `counter`", and the ship tier ran. Before the round the
  per-case scope made the write unresolvable and the checker
  rejected it. `node` refuses the same program. The check now runs
  on the read path and the write path alike.
- MAJOR. `r148` did not reproduce its own defect. With no outer
  binding of the name, HEAD already reported "unknown name", so the
  entry was never Red for this rule and pinned no tier agreement.
  The entry now declares the outer binding, and the fix round
  verified against a HEAD binary that the amended program is
  accepted there and prints different text on the two tiers.

The lesson is one line, and it is the same one the §66 arc paid for
twice: **a corpus entry that never failed before the fix proves
nothing.** Check Red against a binary built from the pin, not
against reasoning.

Six MINOR: a write before a declaration reported a `const` rebind
for a `let` binding, a unit test whose assertion no longer pinned
its own name, a cascading second diagnostic in `r149`, an unrelated
`tsc` error in `r149`, a reservation left behind by a failed
declaration, and one missing `///` doc.

The second review found no CRITICAL: the fix round created nothing,
rejected nothing legitimate in 84 programs, and moved nothing
outside r148 to r153. It found two more MAJOR, both of them holes
this pass left rather than defects it created.

- The new rules reached a local but not an ambient namespace or a
  class name. Twelve sites asked whether a name was shadowed by
  reading the bound locals alone, so a name a later declaration owns
  still resolved to the builtin. Measured: `Math.abs(-2.5)` before
  `const Math: i32 = 3` printed `2.5:3`, and `new Foo()` before
  `const Foo: i32 = 9` printed `1:9`. `tsc` rejects both.
- Rule 1 moved the `switch` scope and left the disposal site behind.
  A `using` in a case still disposed at the end of that case.
  Measured: `case0 / dispose:a / case1 / dispose:b / end` on both
  tiers here, against `case0 / case1 / dispose:b / dispose:a / end`
  under TypeScript downlevelled to ES2022 (`node` v24.18.0). The
  tiers agree, so this is not a tier divergence; it is an accepted
  `tsc`-clean program with a different observable order, which this
  pass's own guiding rule forbids.

Two records worth keeping:

- `r153` is not Red at the contract pin. The pin rejects it too, for
  a different reason ("not an assignable binding"), and the reject
  table carries only the file, the code, and the line, so it cannot
  tell the two apart. The entry pins the regression the first round
  introduced; the message discrimination lives in a unit test.
- §67.1 rule 6 removed the emitter's per-case scope restore because
  a name a case declares is then in scope in a later case.
  Rules 1 and 2 reject every cross-case read and write, so no
  accepted program can observe that change. It is a forward guard
  with no accept-corpus coverage. Pass B must revisit it if a
  cross-case read ever becomes legal.
- `a147` is a forward regression pin, not a Red measurement. The
  pin binary and the landed tree emit byte-identical `program.c`
  for it and both print `case2 / default`. §67.4 item 2 asked for a
  non-rejecting entry, so the entry matches the contract; `a148`
  carries the whole accept-side Red.

The third review found no CRITICAL and no MAJOR. It built a pin
binary, ran 30-plus programs on both tiers, and compared every
expressible one against a TypeScript downlevel under `node`: the
widened predicate rejects nothing legitimate, the switch-body
`using` scope reproduces TypeScript's disposal order and count on
all 20 exit paths, a never-entered case leaves its flag false, and
a trap still runs no dispose on either tier (§18.1b holds). Four
MINOR followed: a message that called a shadowed class unknown, a
switch-scope message that said "block", a needless clone, and one
unused derive.

One rule arrived by accident and was made explicit afterwards. The
implementation already rejected a local that owns a class name and
is declared **before** the `new`, while rule 4a described only the
read-before-declaration shape. The direction is right — `tsc`
rejects it too (TS2351) — so rule 4b now records it, with `r156`.

A process note, because it cost a round: an edit script that
asserts partway through can fail after its first replacement and
write nothing, and a later `grep` that matches the *reference* to a
rule rather than the rule's own text will confirm the wrong thing.
The coding agent caught the missing rule 4b body, not the
orchestrator. Verify an added passage by grepping the passage.

Not moved, and reported as untouched: a class field and a method of
one name, two class fields of one name, and a class name shadowed by
a later local. Class members do not pass through the scope
machinery this pass changed, and `tsc` rejects the first two
(TS2300). They belong to a later request.

## Pass B

Contract `d889620`, amended `6a731fb`, `3960313`, and `8aaac26`.

The four defects share one root cause and one separate bug. Items 1
to 3 are the rule: a value that is live across a suspension must
live in the coroutine frame. Before this pass the frame held the
`let` declarations, the parameters, and the child-frame pointers,
and nothing else, so a temporary inside a composite expression, a
lambda environment, and the `for...of` loop state were all kept
where a resume cannot reach them. The Cranelift verifier names the
failure precisely: a value defined before the suspension is used
after it, in a block the definition does not dominate. Item 4 is
separate: the ship tier recovered a generator's resume target by
searching for the one generator whose yield type matched, and the
search is ambiguous with two. The dev tier never had it — it stores
the resume address in the frame at creation and calls through it.

## Round 1

All four defects closed on both tiers, and the round reported one
shape honestly as already correct at HEAD (two generators of one
yield type work on the dev tier; only the ship tier refused). The
review confirmed the parts that are easy to get wrong and hard to
notice: rooting through the new spill arena is correct, because the
collector scans an allocation's whole payload conservatively, so a
handle in a spill slot is traced while the frame is reachable. It
measured that with a string reachable only from a template
accumulator across two collects, a `for...of` whose array handle
exists only in a spill slot with a collect per iteration, a lambda
environment holding a string and a class handle across a
suspension, and a `using` value across a suspension. No frame
offset that anything else depends on moved, and hot reload, the
async pump, and the worker tests all pass.

Two CRITICAL remained, both pre-existing rather than introduced.

- Rule 1 was implemented at four sites, not as a rule. Seven
  `tsc`-clean shapes still lose a value: an array literal whose
  element suspends, a `new` whose argument suspends, a method
  receiver before a suspending argument, an assignment target
  resolved before a suspending right side, an index base, a
  `push` argument, and a compound assignment. The last is the worst
  and the reason this is not a MINOR: `xs[1] += await a()` stops
  the dev tier with the verifier error and makes the ship tier print
  `xs=1,2` where `xs=1,5` is correct, with no diagnostic.
- The ship tier declared a C temporary before a suspension and read
  it after the resume `goto`, which jumps past the initializer. The
  value is indeterminate on every resumed path, and the C frame is
  destroyed at each return in any case.

One MAJOR: the frame was sized by the whole body rather than by
what is live across a suspension. An async function of 100
arithmetic statements with one `await` and nothing live across it
emitted a 3636-byte arena — about 36 bytes per statement, in a
per-invocation Context allocation.

One MINOR worth keeping: the arena was one untyped byte array read
through `(*((T*)(void*)(_f->_spill + N)))`, which is undefined under
C11 6.5p7 at the `-std=c11 -O2` the ship tier uses. The diff's own
helper used `memcpy` for that reason, so it disagreed with itself.
Rules 1b and 1c record the sizing and the typed-member answer.

## Round 2

Rule 1 became a rule. The round replaced the four hand-written
sites with one shared pre-pass, `codegen/src/suspension.rs`, that
walks the body once and emits an ordered event list. Both tiers
consume that one list through a strict cursor. All seven shapes of
rule 1a print the correct value on both tiers.

The review found that the shared plan had moved the risk rather
than removed it. Three CRITICAL, all in the new machinery.

- The plan and the two lowerings walked in different orders. The
  planner visited a `for` as init, cond, step, body; both tiers
  lower init, cond, body, step. The planner visited a `switch` as
  discriminant then test-and-body pairs; both tiers emit every test
  and then every body. Both mismatches refused ordinary programs,
  so the strict cursor did its job — but it caught the defect at
  the user's program, not at the build.
- The size narrowing of rule 1b was wrong at a back edge. A lambda
  captured before a loop and called after a suspension inside it
  printed `15` then two garbage values on the dev tier and `15`
  then two zeroes on the ship tier. The scan walked the loop body
  once and did not see the value live on the second iteration.
- A lambda reached by assignment rather than by `let` got no frame
  environment at all.

One MAJOR: two lambda environments of one capture shape, both live
across suspensions, shared one frame member. The inner environment
overwrote the outer one, and both tiers printed the inner value
twice.

Rules 1d, 1e, and 1f record the answers: one traversal order proved
by a unit test; correctness before size; a slot's live range is the
value's.

## Round 3

The order test landed. `every_statement_form_requests_the_planned_spill_kinds_on_both_tiers`
asserts that the plan's event kinds equal each tier's request
sequence, for every statement form. The
liveness scan reached a fixed point across back edges, and the
lambda environment followed the local that holds it.

Two CRITICAL remained, and both were of the same class as round 2's
first finding — a site the planner reserved for that a tier never
closed.

- A frame declared `spill0`, `spill1`, and `spill2` while the
  emitted body wrote only `spill0`. The two unwritten slots were a
  foreign call's marshalled arguments, which the ship tier read as
  garbage: `probe=4347879728` where `probe=2` is correct, with no
  diagnostic.
- A lambda assigned to an outer local from inside a nested block
  kept its environment in a C block-local that the frame abandons.
  Both tiers printed a wrong number, and they disagreed.

One MAJOR of the class the differential gate cannot see: a second
lambda of one capture shape reused the member of a first that a
nested block had assigned to an outer local. Both tiers agreed on a
wrong answer.

Rule 1f widened to the local's live range. Rule 1g is the mechanism
this arc lacked: three reviews each found a fresh unclosed site,
and each fix closed one site. A search that finds a new instance
every round is not converging.

## Round 4

Rule 1g landed in both tiers. Each tier asserts at the end of a
coroutine body that the spill cursor and the lambda cursor are both
exhausted. The planner already walks every callee kind, so a site
no tier closed necessarily leaves an unconsumed event.

The check turned the search into an enumeration. Three unclosed
sites existed, and the round names all three with their counts:

    Context.bytesInto   stopped at 0/2
    descriptor literal  stopped at 0/1
    foreign call        stopped at 0/2

The 149 committed corpus entries left no unconsumed event. That is
the number the three reviews did not produce.

Measured after the fixes, on both tiers, byte-identical:

    if=21 block=45
    reuse=20,300
    agg=1,2

Before them the dev tier printed `if=0` and the ship tier printed
`block=196045776` for the first line, and both tiers printed
`reuse=30,300` for the second — the class the differential gate is
blind to.

Gates at round 4: zero warnings; 1040 passed, 0 failed, 1 ignored
across 59 suites in the debug profile, 262 s; 1040 passed, 0 failed
in release, 208 s; `cargo fmt --check` clean; the `tsc` gate clean;
clippy library counts 7 / 22 / 29, at the baseline; no committed
corpus file moved.

## Round 5

The fourth review found three CRITICAL and two MAJOR. Contract
`458ac79` names the root causes as rules 1h, 1i, 1j, 2a, and 7.

Three of the five were one root cause each, and the round closed
each as a class.

- Rule 1h. The liveness scan walked HIR source order and treated it
  as evaluation order. Three shapes lost a lambda environment: a
  callee read before a suspending argument, a capturing lambda
  passed beside a suspending argument, and a `default` arm written
  before a suspending `case` test. Each printed a different wrong
  number on each tier. The round deleted the scan. Liveness is now
  one linear pass over the trace the planner emits
  (`binding_is_used_after_suspension`), so no second traversal
  exists to disagree with the first. `EvalEvent::Release` carries
  the deferred uses, which is what a callee read before its
  arguments needs.
- Rule 1i. The planner took a spill's kind from the expression type
  and the dev tier took it from the declared type at four sites, so
  the strict cursor refused `tsc`-clean programs the ship tier ran
  correctly. The round deleted `save_value` and the four sites with
  it. An expression spill now goes through one function that reads
  `spill_kind(expr)`; an explicit type remains only for a synthetic
  value with no HIR expression.
- Rule 1j. The planner reserved for an async call's receiver and
  neither tier consumed it, so rule 1g refused the program. Both
  tiers now spill and reload the receiver, and root it in the
  parent frame while the ordinary arguments run.

Rule 2a was a ship-tier defect older than this section. The emitter
read the yield counter before it emitted the argument list, so a
nested `await` took the number the outer call had claimed. The
emitter now claims the state and the label after the operands. The
dev tier had failed on the same program at the pin, so the two
tiers had agreed by both failing; fixing the dev tier exposed it.

Rule 7 was the round 4 change itself. `prepares_call_operands`
keyed on the callee kind alone, so every foreign call in every
program — coroutine or not — evaluated all operands before it
marshalled any argument. An array argument's data pointer and count
were then read after a later argument had grown the array. The
predicate now also tests whether an argument suspends, and the
non-suspending order is back at the pin.

**This record said "the two requirements do not conflict". That was
wrong, and this session wrote it without measuring the suspending
case.** The fifth review measured it: a foreign call whose argument
suspends marshals the array count after the later argument runs, so
`f2suspend=3` where the non-suspending twin gives `f2sync=2`. Both
tiers agree, so the differential gate does not see it. At the pin
the suspending program did not run at all, so `3` is a new choice.
Rule 7 gave that choice to the owner and required the round to
report a conflict and change nothing. The round did not report it,
and this session repeated the round's claim.

Measured by this session on both tiers, all byte-identical:

    s02control=10 s02=10      was dev 1929953583 / ship 3
    x01control=15 x01=15      was dev -1777237247 / ship 1
    P2=15                     was dev -519027216 / ship 0
    P1=3                      was dev "spill event mismatch"
    P8=15                     was both tiers "cursor stopped at 1/2"
    s01=3                     was ship "redefinition of label '_gresume0'"
    f2=2 len=3                was f2=3 on both tiers
    probe=2                   the round 3 regression, still green

Gates: zero warnings; 1041 passed, 0 failed, 1 ignored across 59
suites in debug, 263 s; 1041 passed, 0 failed in release, 210 s;
`cargo fmt --check` clean; the `tsc` gate clean; clippy library
counts 7 / 22 / 29, at the baseline; no committed corpus file
moved.

### Open MINOR

The fourth review recorded three. None blocks the phase, and the
round 5 handoff did not carry them.

1. The suspending method path in `codegen/src/cemit.rs` repeats the
   receiver classification that the non-suspending path below it
   performs. Two paths that must stay in step.
2. `save_address` types a spilled pointer as `Type::U64`. The load
   and the store are correct; the type is the one place where a
   `SavedValue` does not match its `SpillKind`.
3. The `Type::Void` guard is in the trace and in the C emitter, and
   not in the dev tier's operand path. No operand of these lists is
   void-typed today, so it is unreachable. It is the same class as
   rule 1i: which site reserves at all is not a shared decision.

## Round 6

The fifth review found five CRITICAL and one MAJOR. Four of the five
were one subsystem. Contract `9fd9603` answers them with rules 1k,
1l, and 7a.

**This round removes code. It adds none.** `codegen/src/suspension.rs`
goes from 1028 lines to 871.

Rule 1k deletes the lambda-environment liveness test. Rounds 2 to 5
each narrowed that test and each narrowing was wrong in a new way.
The fifth review measured four more shapes, and two of them are
boundaries, not holes: a lambda passed to a coroutine callee is used
after the **callee's** suspension, which no intraprocedural scan
sees; and liveness through a capture is transitive, because a lambda
that captures a lambda holds a pointer to the second environment.
Gone with the test: `binding_is_used_after_suspension`,
`lambda_in_frame`, the binding, scope, and destination tracking, the
deferred uses, the synthetic events, the loop-backedge liveness, and
the separate lambda cursor. A capturing lambda inside a coroutine
now acquires a typed `LambdaEnv` slot every time.

The price, measured: a coroutine that holds one captured `i32`
lambda grows from 40 bytes to 48. Rule 1b's size measurement was
taken on expression spill slots, and the narrowing stays there,
where the strict cursor of rule 1d proves the trace against both
tiers.

Rule 1l repairs a regression that rule 1g introduced. The dev tier
skips statements after a terminator and did not advance the cursors
across them, so the end-of-body check reported unconsumed events and
refused programs the pin accepted. The skipped statements now replay
their planned kinds. Confirmed at the pin by this session: the HEAD
binary prints `start` for the `return;` shape, which the round 5
tree refused.

Rule 7a changes no behaviour. Both foreign twins are now interop
tests, and the unsettled one carries rule 7a in its name and in a
comment.

Measured by this session on both tiers, all byte-identical:

    p6=21 p6b=35     was dev 1206611024 / ship 0
    g2=21            was dev 1206611024 / ship 0
    after=21         transitive capture; was dev -761642027 / ship 1
    after=21         `for` initializer; was dev 477102440 / ship 0
    h2only=18        chained assignment; was dev -1113587448 / ship 0
    start            was dev "cursor stopped at 0/1"
    f2sync=2 len=3       and f2suspend=3 len=3, rule 7a, unsettled

Gates: zero warnings; 1043 passed, 0 failed, 1 ignored across 59
suites in debug, 312 s; 1043 passed, 0 failed in release, 234 s;
`cargo fmt --check` clean; the `tsc` gate clean; clippy library
counts 7 / 22 / 29, at the baseline; no committed corpus file moved.

## Adjacent defects, not fixed here

Both are `tsc`-clean programs that give a wrong answer. Neither is a
coroutine defect, and this section does not touch either. This
session reproduced both, and measured both against the pin.

1. **An address into a growable array, taken before a later operand
   grows it.** A `@CStruct` value class in an array, called as a
   method receiver, with an argument that pushes to the same array:

       const zs: V[] = [new V(7)];
       print(`sync=${zs[0].bump(growSync(zs, 5))}`);

   Measured with `growSync` pushing 64 elements: the pin's dev tier,
   this tree's dev tier, and this tree's ship tier all print
   `sync=5`, where `12` is correct. The control line `ws[0].bump(5)`
   prints `ctl=12` everywhere, and the element itself stays intact —
   only the receiver address is stale. **Both tiers agree on the
   wrong answer**, so the differential gate does not see it.

   This is the class of rule 7, one site wider. Rule 7 keeps a
   foreign call's array pointer and count correct against a later
   argument that grows the array. A method receiver has the same
   hazard and no such handling.

2. **A lambda environment assigned from a loop body and called after
   the loop.** No coroutine:

       let f = (): i32 => 0;
       for (let i: i32 = 0; i < 3; i = i + 1) {
         const k: i32 = i * 10;
         f = (): i32 => k + 2;
       }
       print(`v=${f()}`);

   The dev tier prints `v=22`, which is correct, at the pin and in
   this tree. The ship tier printed `v=-1`; the read is of an
   abandoned block scope, so the value varies. Rule 1k puts a
   capturing lambda's environment in the coroutine frame; this
   program has no frame.

## Round 7 — the two regressions, and nothing else

The sixth review found two CRITICAL and one MAJOR. Two of the three
were regressions against the pin. Contract `4bd3704` names them as
rules 1m and 1n. This round fixed those two and touched nothing
else.

Rule 1m is rule 7 at a second site, and the program holds no `async`
at all. The dev tier evaluated every argument into a table and
pushed them afterwards, so an aggregate operand was copied after a
later operand had run. Measured by this session: `sink(h1.v,
bump(h1))`, where `bump` overwrites `h1.v`, printed `call=199` on
the dev tier and `call=31` on the ship tier. The pin printed `31` on
both. The same shape reproduced on a `FixedArray` argument, an
indirect call, a constructor, and an array literal, and a second
form used `Context.collect()` in place of the mutation. An aggregate
operand is copied at the operand, not at the call.

Rule 1n is rule 1l at the boundary-struct branch. The planner
reserved for the receiver and for the argument; the boundary branch
of each tier stored arguments positionally and consumed nothing, so
rule 1g refused the program. The pin's ship tier compiled, linked,
and ran it.

Measured by this session, both tiers byte-identical after the fix:

    call=31 fixed=13 indirect=31 ctor=10,20,1 lit=10,20
    av:rect-y / rect=1,2,3,4

Gates: zero warnings; 1044 passed, 0 failed, 1 ignored across 59
suites in debug; 1044 passed, 0 failed in release; `cargo fmt
--check` clean; the `tsc` gate clean; clippy library counts
7 / 22 / 29; no committed corpus file moved.

## Why pass B did not land in its own cycle

*(This section records the block. The next section closes it.)*

The Phase Review rule is that a phase cannot be COMPLETE with an
open CRITICAL or MAJOR. §67 pass A is COMPLETE and landed at
`1c578f9`. **Pass B was not, in this cycle.**

Before round 6 this session pre-registered a stop: if a sixth review
found a CRITICAL of the lambda-environment class, pass B does not
land in this cycle. The sixth review found one, and the stop holds.

Open, and moved to §68 rather than patched here:

1. A lambda literal inside a loop owns one frame member for every
   iteration. Measured: `async-keep=30` on both tiers, where `10` is
   correct. At the pin the dev tier printed garbage and the ship
   tier printed `0`, so the tiers disagreed and the differential
   gate saw it. Round 6 made them agree. This is the seventh
   defect of the narrowing class, so it moves whole, as §68 corpus
   entry `a152`. §68.2 rule 8 is its fix: the storage scope is the
   live range, never the source block.
2. Three MINOR items from the fourth review, listed above under
   "Open MINOR".
3. Three MINOR items from the sixth review: an arity-only call that
   reads as a discarded result in the intrinsic families; no note at
   the `LambdaEnv` acquire that rule 1k never releases it; no note
   that `K::Cond` plans both arms because both are lowered.
4. The rule 7a owner decision, stated in the contract and pinned by
   an interop test pair.

The count of rounds is the finding, not the defects. Pass B needed
seven rounds and six reviews. §68 records the cause in its
measurement 4: three traversals of one HIR tree each re-derive the
evaluation order, and a review finds one instance of the
disagreement per round. §68 closes the class; this section closed
instances.

## Pass B is COMPLETE, 2026-08-28

Re-checked at `d53e4a8`. Item 1 above was the one CRITICAL, and §68
closed it: `corpus/accept/a152-lambda-env-per-iteration` runs, and
§68.2 rule 8 makes the storage scope the live range. Items 2 and 3
were six MINOR. **Five of the six are void, because §68 deleted the
code they name.** Measured by grep at this pin:

| Item | State |
|---|---|
| 4th review 1 — `cemit.rs` repeats the receiver classification on the suspending method path | Void. cemit reads LIR operands; the HIR-era path is deleted. |
| 4th review 2 — `save_address` types a spilled pointer `Type::U64` | Void. `save_address`, `SavedValue`, and `SpillKind` do not exist. |
| 4th review 3 — the `Type::Void` guard sits in the trace and the C emitter, not the dev tier | Void. The planner is deleted. |
| 6th review 1 — an arity-only call in the intrinsic families | **Open.** `codegen/src/cemit.rs`, `emit_array_intrinsic`. |
| 6th review 2 — no note at the `LambdaEnv` acquire | Void. `LambdaEnv` does not exist. |
| 6th review 3 — no note that `K::Cond` plans both arms | Void. The planner is deleted. |

Item 4, the rule 7a owner decision, is settled in the contract and
pinned by an interop test pair.

No CRITICAL and no MAJOR are open, so the Phase Review rule does not
block pass B. It is COMPLETE.

### The one open MINOR, stated correctly

The record above names it "an arity-only call that reads as a
discarded result". That is wrong, and this is the measured shape.
`emit_array_intrinsic` in `cemit.rs` decides whether an `Array` callback takes an index
parameter, and it reads the decision from the callback's parameter
count:

    match function.params.len() {
        arity if arity + 1 == expected => Ok(0),
        arity if arity == expected     => Ok(1),
        ...
    }

The checker knows whether the callback declares the index parameter.
LIR does not carry it, so the C emitter derives it again. That is
core principle 8's class: a form must carry every fact its consumers
need. The mapping is total over the six listed names today, so
nothing is wrong at this pin. A family with a third optional
parameter misclassifies silently.

### One more site of the same class

Not from any review; found while re-checking the six above.
`emit_parameter_initializers` and its sibling in `cemit.rs` each map `l::ParameterKind` to
the same C source spelling, with the same three arms. One fact,
two derivations.

Both sites are MINOR and both are recorded, not fixed. A fix is a
change to what LIR carries, which is §68.7's contract.
