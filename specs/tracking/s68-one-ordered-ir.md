# §68 — one ordered IR between the checker and the two tiers

Contract `37b871f`. Origin: the owner asked why recent fixes need
many review rounds. §68's measurements answer it, and this file
records the work.

The section moves no language surface. Every gate run below compares
every committed golden and `.expected` byte for byte.

## Step 1 — define LIR, lower HIR to it, verify it

Two attempts. The first passed every gate and was wrong.

### First attempt

149 corpus entries lowered, 878 LIR functions verified, and the
round reported no inexpressible construct. Every gate was green.

The review found three CRITICAL, and two of them were holes in the
contract rather than defects in the round.

1. Every source binding became function-scope `Local` storage. 6742
   of 16715 instructions were local traffic, against 92 block
   parameters, and 24 of 48 coroutine functions read a local after
   their resume block. §68.2 item 7 says the frame holds the live-in
   set of a suspension's successor and nothing else; that set was
   empty, so a tier implementing item 7 builds an empty frame and
   reads uninitialized storage. §68.1 item 4 did not forbid this,
   because a lowering satisfies "every value has one definition"
   while making the loads and the stores the only values.
2. `for...of` created a cursor, stored it, and never advanced it.
   Under item 4 the cursor cannot change, so the loop yields element
   0 for ever. The ship tier needs an index and a bound per
   `for...of`; LIR carried neither.
3. The verifier's call check read `instruction.operand_types !=
   target.parameter_types`, and both sides came from one `map` over
   one operand list. A hand-built call passing three wrong operands
   to a one-parameter function verified clean.

The contract gained §68.1 items 6 and 7 and an amended §68.2 item
11. §68.4 step 1 now states why it needs a review of this kind: no
gate tests LIR while nothing consumes it.

### Second attempt

A source binding is now a value. Storage exists only where an
address analysis proves the address is taken. `for...of` threads the
cursor, the index, and the bound across the back edge as block
parameters, and `IteratorAdvance` produces the advanced cursor. The
call check consults the callee's declared signature, and an
intrinsic operation consults a table the LIR module carries.

Measured by the round, over the 149 corpus entries:

| | first | second |
|---|---:|---:|
| instructions | 16715 | 10189 |
| local traffic | 6742 | 120 |
| locals | 2399 | 31 |
| block parameters | 92 | 1270 |
| coroutine functions | 48 | 48 |
| ...reading a local after resume | 24 | 0 |

The instruction count fell 39 per cent. The local traffic carried no
information.

Gates, verified by this session: zero warnings; 1054 passed, 0
failed, 1 ignored across 60 suites in debug, 276 s; 1054 passed, 0
failed in release, 216 s; `cargo fmt --check` clean; the `tsc` gate
clean; clippy library counts 7 / 22 / 29; no committed golden or
`.expected` moved; 149 entries and 878 LIR functions verified.

`codegen/src/` moved 30352 → 35132 lines. The old HIR consumers stay
until §68.4 steps 2 to 4, so §68.6 item 5 has no meaning yet.

## The cost of one round, measured 2026-08-26

The owner asked how to make the loop faster. The measurement:

    debug suite wall 276 s, of which test execution is 260 s
      codegen/tests/golden.rs   131.8 s   51 %
      codegen/tests/cemit.rs     68.6 s   26 %
      the other 58 suites        59.7 s

`golden.rs` holds 33 tests, so the harness runs tests in parallel,
but one test walks all 149 entries in a serial `for` loop. Each
iteration runs the dev tier and then emits C, compiles it at `-O2`,
links it, and runs it. The suite time is that one test.

**Parallelizing that loop needs care.** `run_dev_corpus_entry` calls
process-global host fixtures for the host-owned-state entries. A
naive parallel loop trades 100 s for a flaky suite.

### Owner decisions, 2026-08-26

1. The coding agent runs targeted tests only. This session runs the
   full gate once. Two full gate runs per round measured the same
   tree twice, and the second attempt's report quoted a test count
   that did not match the tree.
2. This session's gate and the review run at the same time, with
   separate `CARGO_TARGET_DIR`.
3. A round runs the debug profile. §68.6 item 7 requires both
   profiles for a step, not for a round, so release runs at the end
   of a step.
4. The LIR text form of §68.6 item 3 comes forward into step 1. The
   first step 1 review wrote its own printer to read 4779 new lines.
   Every later review pays that cost again.
5. One binary built from each contract pin is kept. A review needs
   "does this reproduce at the pin?" for almost every finding.
6. MAJOR findings are fixed in one pass at the end of a step, not in
   a round of their own.

### Owner decision 2026-08-26 — the interpreter's cost stands

The owner accepts the interpreter's test time and asks that it not
run when it is not needed.

Measured, test execution only, on this host:

    debug   260.05 s -> 261.08 s   (+0.4 %)   lir sweep 4.5 s
    release 215.46 s -> 326.17 s   (+51 %)    lir sweep 116.4 s

Release is above §68.6 item 6's 20 per cent, and it stands. The cost
buys the only witness that does not share a tier's assumption
(CLAUDE.md principle 12), and cutting it means running fewer than
every entry, which is the thing the sweep is for.

**The measure: the full 97-entry sweep is opt-in.** The debug subset
runs always, at 4.5 s, and it is what a round iterates against. The
full sweep runs when this session runs the gate. A skipped sweep
prints the count it did not run, so a silent skip is not possible —
the gate reads that line.

The repetition to remove is a round running the whole `lir` suite
many times while it develops, not the one run inside the gate.

### The reason round count dominates

§67 pass B needed seven rounds. A round that is 20 per cent faster
does not answer that.

Step 1 has no gate that tests LIR, because nothing consumes LIR. A
defect is found only by reading. Step 2 puts the dev tier on LIR,
and the differential gate then tests LIR against the ship tier on
every corpus entry, automatically. **Time spent in step 1 is
therefore the expensive kind.** Step 1 fixes what stops step 2 from
starting; step 2's gate finds the rest.

## Triage rule, owner decision 2026-08-26

The owner asked that rare edge cases not take time before important
work. The order:

1. A program shape that the corpus or the downstream uses gives a
   wrong answer, or loses a trap.
2. A valid program does not compile.
3. A gate or a verifier cannot see a class of defect, **and** that
   blocks progress.
4. Everything else waits for the end of the step, and only what a
   real program reaches is fixed.

**LIR is machine-generated.** A defect that only a hand-built LIR
module reaches is not a program-level risk. The verifier exists to
catch a lowering defect, and from step 2 the differential gate
catches a lowering defect on every corpus entry, automatically and
for no reading time. Verifier completeness is worth less than this
record treated it before this decision.

### Before step 2

- `lower_module` rejects `while (true) { return 1; }` and
  `for (;;) { return 1; }`. Both tiers compile and run them. An
  infinite loop that returns from inside is an ordinary shape.
- `codegen/tests/lir.rs` asserts that no local is named
  `<for-of cursor>`. `declare_hidden_binding` sets `storage: None`
  unconditionally, so no such local can exist. The regression test
  for a CRITICAL cannot fail.

### With step 2, because they feed the performance kill criterion

§68.6 item 4 stops the phase at a ship-AOT ratio above 1.75×. Block
parameters went from 92 to 1270. Each item below adds to that count
or to the emitted code.

- Every parameter and every `for...of` loop variable is declared
  mutable, so a read-only binding threads through every state block.
- `invalidates` names every array value created so far. It carries
  no information a consumer can use, and it grows O(n²).
- A dead placeholder address is emitted at every value-class method
  call.
- A field initializer is inlined at every `new` site.

### End of step

Panic sites; `#[non_exhaustive]` and `#[must_use]`; the dead
`skip_resume` parameter; verifier coverage for comparison operands,
duplicate switch arms, duplicate field ids, and `IteratorCreate`;
replicated trap sites on a compound assignment; `invalidates` naming
values that do not dominate; unverified suspension-successor
parameters; module-initializer order; the `worker_operation`
sentinel; tests that parse a diagnostic source name.

### Closed without a fix

`Suspend::Yield` declares no invalidation. `yield` is void-typed, so
it cannot sit mid-expression where an address is live. The shape is
unreachable from the language surface. Reopen this only if that
surface changes.

### The next review is targeted, not exhaustive

Step 1 has no gate that tests LIR. Every defect costs reading time.
The next check confirms only that the step 2 blockers are closed.
Step 2's differential gate then finds semantic defects on 149
entries without a reader.

## Step 1b — the reference interpreter

Owner decision 2026-08-26: an interpreter for LIR, inserted before
step 2. Contract `3761f1f`, then §68.7 at `af5697d`.

The reason it exists: step 1 has no gate that tests LIR, because
nothing consumes LIR. Both step 1 reviews found every CRITICAL by
reading, at about an hour each.

### Attempt 1 — stopped, and the stop was the finding

No file changed. The round reported that §68 defined the **form** of
LIR and not the **meaning** of its instructions, so no interpreter
could be written from the section. It did not read either tier to
guess. CLAUDE.md principle 8 makes that report the wanted outcome.

The finding is larger than the gap. §68.2 item 10 says that neither
tier decides semantics. **While an instruction's meaning is
undefined, each tier decides it.** The two tiers agree today because
one lowering built both conventions out of HIR, not because LIR pins
a meaning. A differential gate between two tiers cannot see that.

§68.7 answers it: by reference where the language already decides, by
definition for the three protocols LIR alone has.

### Attempt 2 — 98 run, 68 matched, 30 disagreements

51 entries are declared exclusions, almost all interop entries that
need the synthetic native library.

Every disagreement fell into four causes, and **three were defects in
§68.7, which this session wrote**:

- 23 entries: a field of an aggregate value. The field rows named
  only the reference-class path.
- 1 entry: the iteration contract contradicted `a80`. The rule said
  the captured bound alone ends a traversal, and the interpreter
  trapped rather than pass the entry.
- 5 entries: a suspension's successor did not declare the values used
  after it. **This is a defect in the lowering, and it blocks step
  2.** Both tiers work today because both read HIR; a dev tier that
  reads LIR loses the state. The second step 1 review predicted it as
  M7, from reading; the interpreter measured it on `a110`, `a139`,
  `a143`, `a145`, and `a149`.
- 1 entry: the standard runner invoked `main` only, where §26.3
  requires every exported zero-parameter async function.

**A correction this session owes the record.** It reported to the
owner that the language had not decided what happens when a container
changes during a `for...of`. The language had:
`corpus/accept/a80-for-of-foreach-mutation` states in its own header
that "appends do not extend and removals shorten", and pins both. The
corpus is the executable definition. That is a decided divergence
with no `collisions.md` entry — §69's work, not an owner decision.

### Attempt 3 — stopped again, on three more gaps, and all three were ours

- `Suspend` carried a successor id and no argument list, so no value
  could reach the successor's parameters. Reusing a value's id breaks
  §68.1 item 4, and the edge-transfer paragraph named only the three
  branching terminators. The section decided what the frame holds and
  never said how a value reaches it.
- The iteration protocol counted elements, so a `Map` position that a
  removal makes inactive had no expression. `a80`'s `Map` half needs
  it.
- §68.7.6 item 1 still called the mutation decision open, three
  paragraphs after saying `a80` decides it.

**A process defect, ours.** The round stopped on the first gap and
changed no file, so the aggregate-`LoadField` work — 23 of the 30
findings, fully specified and blocked by nothing — did not happen. A
gap that blocks one instruction blocks that instruction, not the
round. Attempt 2 had it right: it ran 98 entries and reported four
causes. The handoff now says so.

### The two-round rule, applied to this arc

CLAUDE.md limits a defect class to two review rounds. "The
specification is incomplete" was raised three times, so the rule asks
whether the form is wrong.

It is not, and the numbers say so: 32 instructions undefined, then 30
disagreements, then 3 gaps. The rule's own remedy — "make a total
check at the build report every remaining site at once" — **already
exists here.** §68.7.5 makes the interpreter the completeness test
for §68.7. The mechanism is working, and each pass removes more of
the prose this session wrote than the last.

### Attempts 4 to 6 — the interpreter runs, and what it cost

**Attempt 4 closed the work.** 97 runnable entries run, 97 match the
golden, 52 declared exclusions with a reason each. `Suspend` carries
positional arguments; the lowering computes graph liveness and
repairs SSA, so a resume successor defines fresh parameter ids, a
resume result is parameter zero with no argument, and the remaining
parameters take explicit arguments. A join that a suspended path and
a plain path reach with different definitions gains a parameter too.
The frame holds the successor live-in set and nothing else.

**The step 2 blocker is closed**, and it was found before step 2
rather than during it.

**Attempt 5 was the gate this session ran, not the round.** The
round's targeted tests passed. The workspace gate did not:

- `coroutine_and_measurement_lir_text_matches_goldens` failed. The
  suspension work changed the LIR and the text goldens were stale.
- clippy codegen was 38 against the 29 baseline.
- **The debug suite went from 276 s to 1674 s.**

The round regenerated the goldens after reading the 402 KB diff, and
its reading is the evidence the suspension work is right: `Suspend`
prints explicit arguments, resume blocks gain parameters for live
values, post-resume uses name the new SSA values, and an array
address's provenance follows the resumed array value.

**A clippy suggestion was refused, correctly.** `Vec<Box<usize>>`
became `Vec<Rc<Cell<usize>>>`, not the suggested `Vec<usize>`: the
runtime holds raw addresses to root slots, and a `Vec<usize>`
reallocation invalidates them. A lint that breaks a soundness
requirement is not taken. The reason is now a comment at the site.

**Attempt 6 — the requirement was wrong, not the round.**

This session's profiling hypothesis was wrong, and the round measured
instead of accepting it: the `invalidates` clone cost 227 ms per
million instructions, and ordinary instruction execution was the
rest. `a22-matrix-propagation` is about a million matrix
multiplications.

`a22` is a **performance benchmark**. §68.6 item 4 uses it against
the 1.75× kill criterion. Its value is its cost, and **the
interpreter tests semantics, not cost.** Requiring all 97 entries in
both profiles was this session's requirement, not the contract's, and
it was wrong.

    debug sweep    1157 s  ->  4.4 s   (96 entries, plus 3 trap entries)
    release sweep   225 s  ->  118.9 s (97 entries, all of them)

Removing one entry from the debug run removed 1150 s, which confirms
the diagnosis completely.

**Keeping a debug run earned its keep.** Debug Rust checks integer
overflow and release does not, and this language's arithmetic wraps
by contract (§2). Three interpreter defects were found and fixed
there: unary negation now uses `wrapping_neg`, add, subtract,
multiply and signed division and remainder use wrapping operations,
and a shift masks its amount to the type width. A release-only sweep
would have reported 97 of 97 while computing the wrong arithmetic.

**The exclusion rule, stated once.** "The interpreter cannot run
this" is an escape hatch and is refused. "This entry's purpose is
cost, and cost is not what the interpreter tests" is a scope
statement and is allowed. Every entry outside the debug subset says
which applies. All 52 release exclusions are interop, worker, or
host-hook entries that need a facility the interpreter does not have,
and each names it.

## Step 2 — the dev tier reads LIR

Landed `5807d7b`. `codegen/src/lower/func.rs` holds no `hir::`
reference and went from 9184 lines to 6688. The transcriber does not
re-derive evaluation order, control flow, or liveness from a tree,
because LIR carries them as data. `cemit.rs` stayed on HIR and was
the reference, per CLAUDE.md principle 11.

### Nine facts LIR did not carry

Each was found by writing a consumer, and each was reported rather
than guessed. The round stopped eight times and every stop was right.

The entry id and the async roots; a `Suspend`'s arguments and its
position; a `Return`'s position; a foreign call's array snapshot; an
embedded field's address provenance; the `Map` iteration protocol;
and the absence of an entry. Each is now a fact §68.2 item 12's check
verifies, so a later consumer meets a build failure rather than a
silent wrong answer.

**The check taught itself.** Its first run reported 153 dropped facts
in one list — every entry id and every async root — instead of one
per round. Then step 2 found three facts it did not know to look for.
§68.2 item 12 now states that limit: the check is as complete as what
we know a consumer needs, and writing a consumer is what tests it.

### One class bought a check instead of a third fix

Three defects had one shape: LIR carried a trap and the transcriber
ignored it. The host trap observer stopped firing; the bounds message
lost its index and length; `t49`'s wire-enum trap was dropped.

CLAUDE.md limits a class to two rounds, so the third bought the
mechanism. Every LIR function's carried traps are compared against
what the transcriber consumed, at build time, naming the function,
the site, the counts, and what is missing or extra. §67 rule 1g is
the same mechanism, and **step 3 starts with it already in place.**

### What the differential gate could not see

Two defects were not program output, so no golden could hold them.

- The host trap observer stopped firing. The program still trapped
  and printed the same bytes; the host callback did not run. §18
  defines that API and a host depends on it. A `jit` unit test caught
  it.
- A boundary read of a C struct array read adjacent temporary bytes,
  because a header was copied to a temporary before it was assigned
  to a nullable C pointer. One cause, three symptoms: one entity read
  instead of two, a zeroed second entity, and wrong flags.

**The corpus did not carry the second shape.** All 18 interop entries
passed. `examples/e09-c-structs-and-slices` and the two-header phase
gate caught it. The corpus is the executable definition, so a shape
that reaches a real defect and is absent from it is a gap in the
definition. The missing shape is a multi-element array of nested
C-layout structs, with padding and a wide flags field, passed through
a const descriptor and written back through a mutable one, with the
second element observed. `a39` covers a simpler single-scalar case.
An entry is added with step 3.

### The minimum set, widened twice

This session told the round to run targeted tests only and to leave
the gate to this session. That saved time and left the round blind to
whatever it did not think to run. Three waves of failures each sat
one step outside its range: the `codegen` unit tests, then the
`cemit` trap differential, then the `examples` differential.

The set is now `cargo test -p subscript-codegen` and
`cargo test -p subscript-examples`, run by the round before it
reports. The rest — other crates, the release profile, `tsc`, clippy
— stays with this session.

### Gates at `5807d7b`

Zero warnings. 1067 passed in debug, 1066 in release, no failures.
The full 97-entry interpreter sweep ran. No committed golden,
`.expected`, or example output moved. clippy compiler 7, runtime 22,
codegen **19** — down from 29 because the old HIR consumer went, so
19 is the new ceiling.

## Steps 3 to 5, the performance gate, and the sharpest test

Landed `8084c45`, `01c99a4`, and `ea45068`.

`cemit.rs` reads no HIR and went from 9767 lines to 6260.
`lower/mod.rs` went from 71 `hir::` references to 0. `suspension.rs`
is deleted — 871 lines that §67 pass B needed seven rounds and six
reviews to build, and that existed only to hold three tree walks in
step. C identifiers are id-derived, so §66's `v_` prefix, keyword
list, and `_N` collision logic are gone; step 5 finished inside step
3's rewrite.

### The performance gate, and three wrong diagnoses

§68.6 item 4 stops the phase above 1.75×. Measured before and after
on one machine in one session, with the C baseline the same on both
sides:

    ship tier   1.53× at the pin  ->  4.01× when LIR landed

The criterion was tripped, and the item had named this risk in
advance. This session then diagnosed the cause three times and was
wrong three times:

    address folding      4.01x -> 4.04x
    SSA coalescing               4.00x
    four prologue fixes          3.98x

Each was a real improvement to the emitted code and none of them
mattered. Each was chosen by reading the emitted C for what looked
wrong.

**The owner asked whether array element access called a foreign
function more than once at a site, and whether `array_len` was called
repeatedly.** Both were true.

    helper                    pin   before the fix
    subscript_rt_array_len      4      22
    subscript_rt_array_ptr     11       0
    subscript_rt_array_data     1      10

The pin read `((SsArrayHeader*)h)->len` and `->data` inline and
called the runtime **only inside a failed bounds branch**. The
transcriber called `array_len` for the test, again to build the trap
message, and `array_data` for the pointer — two or three opaque calls
per element access, and one more per loop iteration.

**An opaque call in a loop body is why the other three fixes did
nothing.** The C compiler must assume such a call writes memory, so
it cannot hoist, cannot vectorise, and spills every cached value
across it. The aggregate copies this session kept removing were the
symptom. Reading the header inline took 3.98× to **1.34×**, which
beats the pin. The dev tier had the same shape: 38.98× to 30.57×.

**The lesson, once: read the emitted code for what the optimizer
cannot see through, not for what looks wrong.** The counts above take
one command and this session ran it only after being asked.

### The sharpest test

§68.6 item 2 named three entries and the rule that matters more: if
either defect needs a hand-written site in a tier, LIR is wrong.

All three close with no site-specific fix, and each is Red at the pin
against a binary built from `9bde577`.

- `a150` — a `@CStruct` receiver in an array with an argument that
  grows it. §68.2 item 9's provenance model closes it. Its ship-tier
  build failure was a checked index whose recomputed address carries
  no bounds trap; the verifier now rejects that disagreement.
- `a151`, `a152` — a lambda environment that outlives its block, and
  the same inside a coroutine. Closed by rule 8 plus per-value
  environment storage.

§66 recorded `a150` and `a151` as adjacent defects it would not fix.
§67 pass B moved `a152` here as the seventh of its narrowing class.
**Three arcs deferred them and the form closed all three.**

### A correction this session owes the record

Rule 8a first concluded a bump arena, reasoning that a `SubFn` copy
keeps sharing one environment, so instances are per execution and the
count is a loop's trip count. S009 removes the assumption: a capture
is a `const` local **by value**, so a copied environment and a shared
one are indistinguishable. The reasoning listed three facts and did
not read the fourth.

### Open, and not §68's

`dev-JIT` measured 28.72× at the pin against §3's 4× limit, and
30.57× now. The limit has been missed for a long time and nothing
re-measured it. That is a separate finding.
