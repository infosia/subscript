<!-- §68 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 68. One ordered IR between the checker and the two tiers

Origin: the owner asked on 2026-08-26 why recent fixes need many
review rounds. An audit of the §66 and §67 arcs answers it. This is
not a downstream request. **No language surface moves.** The
accepted TypeScript subset, the C ABI, the host API, the CLI, and
every committed `.expected` stay as they are.

This section opens after §67 lands. Pass A landed at `1c578f9`.
Pass B landed at `9bde577` and is **not COMPLETE**: one CRITICAL
stays open, and it is item 2's `a152` below. §68 closes it. §67 closes instances of the
defect classes below; this section closes the classes.

Measurements re-taken at `9bde577`, where §67 pass B is landed and
not COMPLETE. `codegen/src/` holds 30352 lines. Every measurement is structural. Each one is read
from the committed tree, or quoted from the §66 and §67 records.

1. **The round count separates by area, not by difficulty.** A
   request that changes the checker or the standard library lands in
   one implementation commit: R33 `49bdd1d`, R34 `ca5cb4e`, R36
   `1438b76`, R37 `f29c4c5`. A request that changes codegen
   internals does not. §66 needed seven review rounds. §67 pass A
   needed four rounds and three reviews. §67 pass B needed five
   rounds and five reviews, and it leaves three MINOR and two
   adjacent defects open.
2. **Three traversals of one HIR tree each re-derive the evaluation
   order.** `hir::ExprKind` holds `Box<Expr>` operands
   (`compiler/src/hir.rs`), so no evaluation order exists in the
   data. `codegen/src/lower/func.rs` (9184 lines) walks the tree
   for the dev tier. `codegen/src/cemit.rs` (9763 lines) walks it
   for the ship tier. `codegen/src/suspension.rs` (871 lines) walks
   it for the spill plan. Each walk fixes the order by its own
   convention, and the three conventions must agree.
3. **The same walk exists twice.** `codegen/src/lower/func.rs` and
   `codegen/src/cemit.rs` each declare their own `walk_lets`, their
   own `count_yields`, and their own `count_async_calls`. The two
   `count_yields` must return one number, or the resume label tables
   of the two tiers disagree.
4. **Four §67 pass B CRITICAL findings are one class: traversal N
   disagrees with traversal M.** The round 2 review measured the
   `for` statement. The planner visits it as init, cond, step, body.
   Both tiers lower it as init, cond, body, step. Rule 1h records
   that the liveness scan read HIR source order as evaluation
   order. Rule 1i
   records that the planner took a spill kind from the expression
   type and the dev tier took it from the declared type. Rule 1j
   records a site the planner reserved for and neither tier closed.
5. **The strict cursor reports the disagreement at the user's
   program, not at the build.** The §67 record states it: the cursor
   "did its job — but it caught the defect at the user's program,
   not at the build." Rules 1d, 1e, 1f, 1g, 1h, 1i, 1j, 1k, and 1l
   exist to hold three traversals in step.
6. **Both tiers already want a control-flow graph.** The dev tier
   builds Cranelift blocks. The ship tier emits no structured C:
   every loop and every branch is a label and a `goto`. Measured
   over the whole of `codegen/src/cemit.rs`: **no emitted string
   holds a C `for`, `while`, or `do` keyword.** `emit_while`,
   `emit_for`, `emit_for_of`, and `emit_switch` emit labels and
   `goto` only.
   Both tiers build a graph from a tree, separately, for every
   function, and neither keeps the graph.
7. **The differential gate is blind to a shared wrong answer.**
   Three recorded instances: the `using` disposal order in a
   `switch` case (pass A second review); a second lambda that reuses
   a first lambda's frame member (round 3 MAJOR); the rule 7a
   foreign call that marshals an array count after a later argument
   grows the array (fifth review). The §67 record states the
   consequence each time: "Both tiers agree, so the differential
   gate does not see it."
8. **Two defects of that class stay open, and neither program
   suspends.** The §67 record names them as adjacent defects. A
   `@CStruct` receiver address, taken before an argument grows the
   same array, prints `sync=5` on both tiers, where `sync=12` is
   correct. A second program assigns a lambda inside a loop body,
   and calls it after the loop. The dev tier prints `v=22`. The
   ship tier reads an abandoned C block scope. The §67 answers are
   frame-scoped, and neither program holds a frame.
9. **The ship tier derives C identifiers from source names; the dev
   tier does not.** §66 measurement 5 records it:
   `codegen/src/lower/mod.rs` names a method by index, so every
   divergence of that class is one-sided. §66 closed the class with
   a `v_` prefix, a keyword list, and `_N` collision logic. A prefix
   is a convention that every future emitter site must obey.

Consequence: the two tiers hold the same semantic decisions twice,
and a third copy holds the evaluation order. A review finds one
instance for each round, because the class has no single site.

Rule 1g proved the alternative. When the check became total, the
round named all three remaining sites with their counts — the
number that three reviews did not produce. This section makes
that mechanism the default: **a defect class closes with a total
check at the build, not with a corpus entry for each instance.**

### 68.0 What does not move

The interface is not the subject. This section moves no part of it.

- Every program in `corpus/accept/` compiles and prints the same
  bytes, with the exceptions §68.6 item 2 names and pins as Red.
- The C ABI, the emitted header, the `subscript_*` symbol
  convention, and the host API do not move.
  - *(One addition, 2026-08-26, recorded rather than discovered.)*
    Step 2 added `subscript_rt_trap_index_out_of_bounds`. The dev
    tier is Cranelift and cannot format the bounds message the way
    emitted C does with `snprintf`, so it needs a runtime entry that
    takes the index and the length. Nothing existing changed and no
    host loses what it depends on. **Both tiers call it**, so the
    message has one source instead of two — one instance of the
    duplication this section exists to remove, closed early.
- `specs/blocks/collisions.md` does not move. No collision is
  decided or re-decided here.
- Invariant 3 holds: two execution forms, dev JIT and ship C AOT.
  They keep separate final lowerings. This section moves the shared
  part **above** them, not between them.

### 68.1 The form

1. **One IR sits between the checker and both tiers.** The name is
   LIR. One lowering builds it from typed HIR. Both tiers consume
   it. **No tier reads HIR.**
2. **The evaluation order is data.** Every operand of a LIR
   instruction is a value name or a constant. No operand holds a
   nested expression. The HIR → LIR lowering fixes the order once,
   and writes it as a sequence.
3. **The control flow is a graph.** A LIR function is a list of
   basic blocks. Each block ends with one terminator: branch,
   conditional branch, switch, return, trap, or suspend. No
   structured statement form survives into LIR.
4. **Every value has a name, a type, and exactly one definition.** A
   temporary is a value like any other value.
5. **Every entity carries an id.** A class, a method, a function, a
   local, a block, and a value each carry one id. **A target
   identifier derives from an id.** No target identifier derives
   from a source string. A source name rides beside the id as an
   attribute, for diagnostics and for the reload key. No consumer
   parses that attribute. *(R37 verified the same property for
   method names before its contract: no consumer of a method name
   parses it.)*

6. **A source binding is a value, not storage.** *(Added
   2026-08-26 after the step 1 review, which measured that item 4
   alone does not give this.)* `Local` storage exists only for a
   binding whose address the program takes, and the lowering states
   which those are. Item 4 says that every value has one definition.
   A lowering satisfies item 4 and still routes every binding
   through a function-scope slot, because the loads and the stores
   are then the only values. Measured on the step 1 lowering: 6742
   of 16715 instructions were local traffic, 2399 locals existed
   against 92 block parameters, and 24 of 48 coroutine functions
   read a local after their resume block. That is the storage model
   of §67, which items 6 to 8 of §68.2 exist to retire, so the form
   must forbid it and not only the rules.
7. **Loop traversal state is values.** *(Added 2026-08-26 after the
   step 1 review.)* A cursor, an index, and a bound cross a back
   edge as block parameters. An instruction that advances a
   traversal produces the advanced state as a result. Measured on
   the step 1 lowering: `for...of` created a cursor, stored it in a
   local, and read it in the body and in the condition, and no
   instruction ever wrote it back. Under item 4 the cursor cannot
   change, so the loop yields element 0 for ever, and a tier can
   only run it by holding an index that LIR does not carry.

8. **LIR carries every trap site, once, on the instruction or the
   terminator that owns it, and each carries its position.**
   *(Widened 2026-08-26 after step 2. The rule said "instruction",
   and a terminator owns sites too. `Suspend` lacked the position a
   reload's `StaleCoroutine` reports, and `Return` lacked the one a
   boundary-pointer scratch allocation reports. Two rounds, one
   class, so CLAUDE.md's two-round rule applies and the rule widens
   rather than a third terminator being fixed.)* *(Added 2026-08-26 after the second step 1 review.)*
   A trap site belongs to the operation whose operands the check
   reads. Every HIR node the lowering evaluates contributes its own
   sites, a node reached as the base of a place included. A
   function-level site is carried too: both tiers read the
   coroutine creator's allocation site and refuse the program when
   it is absent. Measured on the second step 1 attempt: `a[idx()].x
   = 9` traps today with "index 5 out of bounds for array length 2",
   and its whole LIR module carried no index trap; 13 entries lost
   the coroutine `Allocation` site, which no part of LIR named; and
   one `DivisionByZero` of `a76` became three, two of them on an
   address computation that has no divisor. A `checked` flag with no
   site and no position is not a trap site — a tier that reads it
   decides semantics, which §68.2 item 10 forbids.
9. **No instruction restates a fact the values table carries.**
   *(Added 2026-08-26 after the second step 1 review.)* An operand's
   type is a property of the value. An instruction that records its
   operand types again gives the verifier two copies of one fact,
   and a check that compares them cannot fire on anything the
   lowering built. Measured: after item 11 was amended, one check
   consulted a declared signature and about ten kept the
   self-comparing shape, including the exact line the first review
   named. Delete the restatement. Derive what an operation requires
   from the operation, and compare that against the values table.

10. **The module names its entry and its async roots by id.**
   *(Added 2026-08-26 after step 2 stopped.)* A consumer runs a
   program, so it needs to know which function starts it, and §26.3's
   standard runner needs every exported zero-parameter async
   function. Neither is derivable from the form today, and the
   interpreter compensated by matching a `source_name` against
   `"main"` — which item 5 forbids, and which this session did not
   catch when it accepted the interpreter. The module carries the
   entry `FunctionId` and the ordered list of async root
   `FunctionId`s.

### 68.2 The rules the form makes true

6. **Liveness is one fixed-point over the graph.** The analysis
   reaches a fixed point across back edges. No other consumer
   computes liveness. This retires §67.2 rules 1b, 1d, 1e, 1f, and
   1h, whose subject is the agreement of two liveness walks.
7. **A suspension is a terminator.** The values live across a
   suspension are the live-in set of its successor block. The frame
   holds that set, and holds nothing else. **No event list exists,
   and no cursor exists.** This retires §67.2 rules 1, 1c, 1g, 1i,
   1j, 1k, and 1l. It also retires rule 2a: a suspension carries a
   block id, so no emitter allocates a label number by hand.
7a. **A `Local` that is live across a suspension lives in the frame,
   and LIR says so.** *(Added 2026-08-28 after the Fable phase review
   of §68 consumers, C1.)* Item 7 says the frame holds the successor's
   live-in set and nothing else. A `Local` is storage, not a value, so
   it was never in that set, and both transcribers gave it a C local
   or a stack slot of the resume function, re-created at every resume.
   Measured at `2a65724` and at `e598994`:

       function* g(): Generator<i32> {
         const fixed: FixedArray<i32, 2> = [1, 2];
         yield fixed[0]; yield fixed[1]; yield fixed[0] + fixed[1];
       }
       dev 1,0,0   ship 1,0,0   interpreter 1,2,3

   `fixed[0] + (await val(3)) + fixed[1]` prints `4` on both tiers and
   `6` on the interpreter. Both tiers agree, so the differential gate
   cannot see it; the interpreter is right.

   The form carries the fact. Every `Local` declares its storage
   class: **activation** (dies with the activation) or **frame**
   (lives in the coroutine frame from its first definition to the
   function's end). The lowering marks a `Local` `frame` when any
   suspension lies between a definition and a use of it. The verifier
   fails a function in which an activation `Local` is read after a
   suspension that a definition of it dominates. Both transcribers
   read the class and decide nothing. Item 7's "nothing else" now
   reads: the frame holds the live-in set and the frame-class locals.

   Corpus: `a164` (a generator and an async function, each with a
   `FixedArray` local and a `FixedArray<CStruct, N>` local read after
   a suspension, in a loop and outside one). Red at `2a65724`.
8. **Storage scope is the live range, never the source block.** If a
   value outlives its source block, the value lives in
   function-scope storage. This closes measurement 8's second
   defect, which holds no frame, by the same rule that serves a
   coroutine.

8a. **A capturing lambda's environment is one instance per execution
   of the literal, in a function-scoped arena.** *(Owner decision
   2026-08-27. Rule 8 alone does not give this, and §68.6 item 2
   named rule 8 as the fix for `a151` and `a152`. That was wrong.)*

   Rule 8 is about **scope**. The defect is about **instance
   count**. One shared function-scope environment closes `a151` by
   accident — the loop's last iteration wins and the one local holds
   the last closure — and leaves `a152` wrong, because `a152` keeps
   iteration 0's closure in a second local while the literal runs
   again. Both tiers print `async-keep=30`, and `10` is correct.

   Three facts fix the shape of the answer:

   - The checker rejects a capturing lambda that escapes its
     defining function: "A capturing lambda may not escape its
     defining function." **The lifetime is bounded by the function's
     activation**, so no collectable allocation is needed.
   - S009 rejects a capturing lambda stored in an array, a field, or
     a global, so a local is its only home.
   - A `SubFn` copy copies the environment **pointer**, so two
     locals alias one environment. A slot per destination is
     therefore not enough either.

   **The reasoning above concluded a bump arena, and that was
   wrong.** *(Corrected 2026-08-27, the same day.)* It assumed the
   `SubFn` copy keeps sharing one environment, so instances must be
   per execution and their count is a loop's trip count. A fourth
   fact removes that assumption:

   - S009: "capturing lambdas may capture only const locals **by
     value**". A capture is immutable and copied, so **sharing an
     environment and copying it are not distinguishable**. No
     program can observe the difference, because no lambda can write
     a captured variable.

   So the environment travels **with the value**: every function-
   typed LIR value owns its environment storage, and a `Copy`, a
   block parameter, an edge, a parameter, and a resume all copy the
   environment rather than alias it. `keep = f` takes iteration 0's
   contents, and a later iteration overwrites `f`'s storage and not
   `keep`'s. The count is then the number of LIR values, which is
   static, and no arena is needed.

   Ordinary functions hold that storage in a shadow frame; a
   coroutine holds it in its own frame, which replaces §67 rule 1k's
   one member per literal — the member `a152` overwrites.

   A closure that does not outlive its block keeps a stack slot. The
   liveness of §68.2 item 6 already answers which is which, and no
   second analysis decides it.

   **What this does not do.** It is not a `Context` allocation. A
   `Context` allocation would hold the environment until an explicit
   collect, against a stack slot that costs nothing today, and it
   would give the user an allocation they cannot `delete`. Invariant
   2 is satisfied by the arena being explicit, scoped, and
   deterministic, not by a collector.

   **Measure, do not assert.** The record states the cost of the
   per-value environment storage. `a22` measured 1.34× with it, the
   same as without, so the cost does not reach the performance gate.

8b. **A value whose address is taken stays rooted for the rest of
   the activation.** *(Added 2026-08-28 after the Fable phase review
   of the post-§70 arc, finding C1.)* `26403be` gave
   `root_storage.rs` a fixed point that follows an address through
   SSA edges, locals, and `StoreAddress` into owners it knows, and
   keeps the base rooted while the address is reachable that way. It
   has no arm for `StoreGlobal`, and a `Call` transfers the
   dependency only into the call's result, so a callee that stores
   the operand ends the chain. Measured at `2a65724`, identical bytes
   on both tiers, expected `<sum>:<len>:1:1:31:47` on every line:

       cond-in-setup=0:0:0:0:8015:8016
       pushcond-in-function=0:0:0:0:8015:8016
       fieldcond-after=0:0:0:0:8015:8016

   One plan feeds both transcribers, so the differential gate cannot
   see it. This is the second instance of the address-base class
   (§33.4 records the first), and the two-round rule applies: the
   form changes, and no arm is added.

   **The rule.** If any instruction takes a value's address, that
   value's root slot is held from the address-taking instruction to
   the activation's end. The plan does not follow the address. It
   does not need to know where the address goes, so there is no arm
   to miss. The fixed point that followed addresses is deleted.

   Cost: one slot per address-taken value, held to function exit. An
   address is taken by a value-class-to-nullable conversion and by
   nothing else a script can write, so the count is small and the
   cost is bounded by the number of such conversions in a function.

   With S015 (§33.4) rejecting every store of such a value into a
   location that outlives the activation, the legal residue is the
   value that lives and dies in one activation, and this rule makes
   that residue sound without a chain.

   Corpus: `a163` — the in-activation shapes the review measured
   that S015 leaves legal, each read back through the foreign
   checker. Red at `2a65724`.
8c. **The form's own invariants are verified, not stated.** *(Added
   2026-08-28 after the Fable phase review of §68 form, C2, M3, M4,
   M5.)* Four checks the section claims did not exist, and each was
   built as a hand-written LIR module and accepted:

   - **Rule 7 / §68.7.4.** A value read after a resume that is not a
     successor parameter was accepted (`b1: %1 = Copy(%0)` after a
     `Suspend` whose successor has no parameters). The verifier treats
     a suspend edge as an ordinary edge. It must not: after a
     `Suspend`, the successor's live-in set is exactly its parameters,
     and a read of any other value fails verification.
   - **`array_base`.** An `Address` whose `array_base` names an
     undeclared value was accepted. The base must be a declared value
     that dominates the address, or verification fails. Every check
     keyed on the base — invalidation, the interpreter's poison
     registry — is silent for a wrong base, so this is the check that
     guards the others.
   - **Item 11 for intrinsics and built-in methods.** The verifier
     compared a call's operands against the `parameter_types` on the
     same instruction, so a `Math.Abs` called with three strings and a
     self-agreeing record verified clean. That is the shape core
     principle 9 forbids. **The module carries one signature table for
     intrinsics and built-in methods, derived from the checker's
     definitions, and the verifier compares every such call against
     it.** `CallTarget.parameter_types` for those kinds is then the
     restatement item 9 forbids, and it goes.
   - **Item 12's exhaustiveness.** The fact check enumerates from
     HIR's types without a wildcard for `TrapSite` only, because
     `ExprKind`, `Stmt`, `Callee`, and `AsyncCallee` are
     `#[non_exhaustive]` and force a wildcard in another crate. A new
     suspending or calling kind is then silently unchecked. **Those
     four enums lose `#[non_exhaustive]`**, as `TrapSite` did. CLAUDE.md's
     convention is for public extensible enums; HIR is this project's
     internal form, and a consumer that must be total over it is the
     point. The check then names every new kind at compile time.

   Item 12's "fails the build" is corrected to "fails the suite": the
   fact check runs in the test suite over every corpus entry, and the
   CLI does not run it. Totality over facts is the property; when it
   runs is not.

   The interpreter's exclusion list is part of the form's record. An
   exclusion whose reason no longer holds is removed: `a153` runs and
   matches its golden at `2a65724`, and the list said it could not.
9. **An address is a value, and it carries an invalidation point.**
   Every LIR instruction that can move an array's storage names the
   arrays that it invalidates. The lowering re-computes an address
   that crosses an invalidation of its base. This closes §67 rule 7,
   the rule 7a conflict, and measurement 8's first defect, as one
   rule instead of three sites.
10. **Neither tier decides semantics.** Each tier is a total
    function from LIR to its target. If a tier needs a fact that LIR
    does not carry, LIR is wrong. The round reports it and stops.
    That report is the intended outcome, not a failure of the round.
11. **A verifier runs on every LIR function, in every build.** It
    checks that every use is dominated by its definition; that every
    value has one definition; that every block ends with one
    terminator; that no address crosses an invalidation of its base;
    and that every operand type matches its instruction. **Every
    check compares two things the lowering derived separately.** A
    check that compares a record against the expression that built
    it cannot fire. Measured on the step 1 verifier: the call check
    read `instruction.operand_types != target.parameter_types`, and
    both sides came from one `map` over one operand list, so a call
    that passed three wrong operands to a one-parameter function
    verified clean. A call compares against the **callee's declared
    signature**. An intrinsic operation compares against a table
    that LIR carries, not against a positional index into a Rust
    array. The verifier's own tests must build the violating form,
    not mutate the record that the check reads. The
    verifier runs in the debug profile and in the release profile.
    This is rule 1g, generalized from spill slots to the whole form.

12. **A total check reports every fact that LIR drops.** *(Added
    2026-08-26. Two reviews raised one class, so CLAUDE.md's
    two-round rule applies: the form changes, and no third instance
    is fixed by hand.)* The first step 1 review found that a
    `for...of` needed an index and a bound that LIR did not carry.
    The second found that no part of LIR named the coroutine
    creator's allocation trap site, which both tiers read and
    require, and that 13 entries lost it. Each was found by reading.
    The build now compares, for every corpus entry, each fact a tier
    reads out of HIR against what LIR carries: a trap site per
    expression and per function, the entity ids, and the operand
    counts. A dropped fact fails the build and names the entry and
    the position. This is rule 1g of §67 in its general form: a
    total check turns a review's search into a build's list.

    **The check is only as complete as what we know a consumer
    needs, and writing a consumer is what tests it.** *(Added
    2026-08-26.)* The first run reported 153 dropped facts — every
    entry id and every async root — in one list. Step 2 then found a
    fact the check did not know to look for: a `Suspend`'s position,
    which a resume after a reload reports with `StaleCoroutine`.
    Adding that item made the check report 49 sites at once. So the
    relationship is the same one §68.7.5 states for the section: the
    interpreter tests §68.7, and each new consumer tests this check.
    A consumer that finds a dropped fact reports a defect in the
    check as well as in LIR. The check verifies a position on every
    terminator that owns a trap site, not on a named list of them.

    **The check enumerates its fact kinds from HIR's own types,
    exhaustively.** *(Added 2026-08-27 after step 3. Two rounds found
    a kind the check did not enumerate: a module with no entry, where
    the check compared presence and not absence; and a host entry's
    parameter validation, an attachment point beside the expression
    and the function that HIR carries ad hoc. CLAUDE.md's two-round
    rule applies, so the rule changes rather than a third kind being
    added by hand.)* A fact kind that HIR gains and the check does not
    learn is a compile error, not a silent omission. The check
    compares both directions: a fact HIR has and LIR drops, and a
    fact LIR has and HIR does not.

### 68.3 What retires

The deletions are part of the contract. §68.6 item 5 measures
them.

- `codegen/src/suspension.rs`, in whole: `SpillPlan`, `SpillEvent`,
  `EvalEvent`, the trace builder, the strict cursor, and the
  exhaustion check.
- §67.2 rules 1, 1b through 1l, 2a, 7, and 7a, as hand-written
  sites. Each rule keeps its corpus entry. No rule keeps a site.
  §67.1 is checker semantics and does not move.
- The duplicate walks of measurement 3. One lowering replaces both
  copies of each walk.
- §66's `v_` prefix, its keyword list, and its `_N` collision logic.
  A C identifier derives from a LIR id, so no source name reaches
  the C namespace.
- The per-tier dominance and ordering assertions that each tier
  holds today.

### 68.4 The order of the work

The differential gate guards this migration, if one tier moves at a
time. Each step ends with the full gate of §68.6 item 7. If a step
moves a committed golden, the step stops and reports it.

1. Define LIR. Write the HIR → LIR lowering and the §68.2 item 11
   verifier. Neither tier changes yet. The verifier runs over every
   corpus entry.
   **Nothing consumes LIR at this step, so no gate tests it.** The
   verifier is the only check, and one round writes both. The step
   therefore ends with a review that builds violating LIR by hand
   for each check of item 11, and that reads the LIR of named corpus
   entries against their known behaviour. *(Added 2026-08-26. The
   first attempt at this step passed every gate with a lowering that
   could not terminate a `for...of` and a call check that could not
   fire.)*
1b. **Write a reference interpreter for LIR.** *(Owner,
   2026-08-26. Inserted before step 2.)* It runs every corpus entry
   and its output joins the standing gate: interpreter ≡ dev ≡ ship
   ≡ golden. Neither tier changes yet.

   The reason: step 1 has no gate that tests LIR, because nothing
   consumes LIR. Both step 1 reviews found every CRITICAL by
   reading, and each cost an hour. Each one shows as a wrong output
   the moment LIR runs — a `for...of` cursor that never advances
   yields element 0 for ever; a dropped trap site does not trap; a
   binding read out of storage after a resume reads what the resume
   abandoned.

   It also gives step 2 a tiebreaker. When a dev tier on LIR
   disagrees with a ship tier on HIR, the interpreter says which
   side moved.

   **The tiebreaker has a blind spot, measured at step 2.** The
   interpreter's declared exclusions are almost all interop entries,
   because they need a native library it does not load. Step 2's
   dev-and-ship disagreements were `a97`, `a124`, and `a125` — all
   interop entries. So the third witness was unavailable exactly
   where the disagreement was, and the round reported "cannot
   adjudicate" rather than name a side. That is the right report,
   and it bounds what this step's tiebreaker is worth.

   **Boundaries.** The interpreter is not a third execution form,
   and invariant 3 does not move: the two shipped forms stay dev JIT
   and ship C AOT. The interpreter is a test oracle and is never
   shipped. It does not replace the golden; it agrees with it, as
   the tiers do. It links `runtime/`, so it shares the runtime and
   finds no runtime defect — it finds lowering and LIR defects,
   which is where every defect of §66, §67, and §68 step 1 lived.
   An entry the interpreter cannot run is named in a declared list
   with its reason. A silent skip is an escape hatch.

   It answers principle 12 as well: written from this section rather
   than from either tier, it does not share a tier's assumption. The
   two open defects that both tiers agree on are exactly that shape.
2. Move the dev tier to LIR. The ship tier stays on HIR. The
   differential gate now compares one LIR consumer against one HIR
   consumer, so it guards this step directly.
3. Move the ship tier to LIR. `cemit.rs` becomes a transcriber of
   blocks and instructions.
4. Delete `codegen/src/suspension.rs` and both cursors. Rules 6 and
   7 of §68.2 now carry what the cursors carried.
5. Move C identifiers to id form. Delete the `v_` prefix.

Steps 2 and 3 are the two steps that carry risk. Step 2 keeps a
working ship tier as the reference. Step 3 keeps a working dev tier
as the reference.

### 68.5 Changes by site

`compiler/`: `hir` is unchanged. The checker is unchanged. A new
module holds the LIR types.

`codegen/`: a new module holds the HIR → LIR lowering and the
verifier. `lower/func.rs` becomes a LIR → Cranelift transcriber.
`cemit.rs` becomes a LIR → C transcriber. `suspension.rs` is
deleted. `lower/mod.rs` keeps the symbol tables, and takes the C
name construction that `cemit.rs` holds today.

*(Recorded 2026-08-27 after step 3.)* `lower/mod.rs` **decides no
semantics**, which is item 10 applied to it. After step 2 it still
held 64 `hir::` references, and one of them derived a host entry's
wire-alias validation from HIR. That is why `t50` passed on the dev
tier and failed on the ship tier: one consumer had a fact the form
did not carry. The symbol tables and the C names are its whole role.
Step 2's commit says "the dev tier reads LIR"; `lower/func.rs` does,
and `lower/mod.rs` did not.

`runtime/`: unchanged. No runtime entry point moves.

### 68.6 Corpus and gate (pre-registered exit criteria)

1. **No committed golden or `.expected` moves**, except the entries
   item 2 names. A golden that moves is evidence of a defect in the
   step, not a golden to update. The round stops and reports it.
2. **The two open defects of measurement 8 close, and neither
   closes with a site-specific fix.** This is the sharpest test of
   the form. If either defect needs a hand-written site in a tier,
   LIR is wrong, and the round reports that instead of the fix.
   - `corpus/accept/a150-receiver-address-invalidation`: a
     `@CStruct` value class in an array, called as a method
     receiver, with an argument that grows the same array. Red at
     the contract pin: both tiers print `sync=5`, and `sync=12` is
     correct. The control line `ctl=12` stays correct at the pin.
   - `corpus/accept/a151-lambda-env-outlives-block`: a lambda
     assigned inside a loop body and called after the loop, with no
     coroutine. Red at the contract pin: the dev tier prints
     `v=22`, which is correct, and the ship tier printed `v=-1`.
     The ship tier reads an abandoned C block scope, so its value
     varies between runs.
   - `corpus/accept/a152-lambda-env-per-iteration`: the coroutine
     twin of the entry above. A lambda literal inside a loop body
     in an async function, held past the loop, with a suspension in
     the body. Red at the contract pin: both tiers print
     `async-keep=30`, where `async-keep=10` is correct. *(Added
     2026-08-26. The §67 pass B sixth review found it. Before §67
     round 6 the dev tier printed `async-keep=-2083027712` and the
     ship tier printed `async-keep=0`, so the tiers disagreed and
     the differential gate saw it. Round 6 made them agree on the
     wrong answer, which the gate cannot see. One frame member serves one lambda literal, and a literal
     inside a loop runs many times. §68.2 rule 8 is the fix: the
     storage scope is the live range, never the source block. A
     narrowing patch in §67 would be the seventh of its kind, so
     the defect moves here whole.)*
   - Every entry must be **Red at the contract pin, verified
     against a binary built from that pin.** *(The §67 lesson, in
     one line: a corpus entry that never failed before the fix
     proves nothing.)*
3. **LIR text goldens** for a named subset: every entry that holds a
   coroutine, plus the §66 and §67 measurement entries (`a145`,
   `a147`, `a148`, `a149`, `a150`, `a151`). The text form makes a
   LIR change reviewable as a diff. The rest of the corpus is
   covered by the verifier and by the existing goldens.
4. **Performance.** §3 fixes ship-AOT at 1.5× of the C baseline and
   dev-JIT at 4×. §11's bisection records 1.53× post-P19, with the
   trap checks that C6 requires. LIR names many temporaries, and the
   emitted C depends on the C compiler to coalesce them, so this is
   the named risk of the whole section. The round measures
   `a22-matrix-propagation` by the §9 methodology, before and after,
   on one machine in one session.
   - **`a22` alone was not enough, measured 2026-08-27.** This item
     gates one entry, and `a22` is matrix propagation: it allocates
     almost nothing. The `collect` workload of the cross-language
     suite — 20000 nodes over 6 rounds, each owning strings, 15000
     kept and the rest freed — regressed through §68 and no gate
     saw it. Bisected on one machine in one session, with the C
     baseline steady at 32.9 to 33.9 ms:

         pin            ship             dev-JIT
         9bde577      211.7 ms 6.43x   229.3 ms 6.97x   before §68
         628a491      273.5 ms 8.07x   344.5 ms 10.17x  after §68
         662a9ec      274.0 ms 8.11x   342.7 ms 10.14x  now

     §70 is not the cause: `628a491` is the commit before it and
     already carries the regression. LuaJIT and V8 measured the same
     across the pins, so the machine is not the cause either.

     **The allocation and free path has no standing gate.** The
     cross-language suite holds the workloads that exercise it, it
     runs by hand, and nothing ran it between 2026-07-27 and
     2026-08-27. The owner asking for an updated benchmark table is
     what found this.

     **The cause, measured: a dead LIR temporary stays a GC root for
     the whole activation.** Runtime call counts are identical at
     every pin, so it is not extra calls. The time is inside
     `Context.collect()`: 123 ms before §68, 297 ms at `8084c45`,
     189 ms at `628a491`. Live allocations say why — the collector
     reached 75005 per round and 5 at the end before §68, and leaves
     100006 to 175006 per round and 100006 at the end after it. The
     surplus is exact: 25000 allocations is 5000 dropped nodes times
     a node and its four strings, and 4720000 bytes is 5000 times
     944 bytes of that group's ship-tier capacity. A stale root holds
     the head of the chain the program deliberately dropped.

     **This violates §68.2 rule 8**, which says storage scope is the
     live range and never the source block. Rooting a value for the
     whole activation is precisely what that rule forbids, so this is
     a defect and not a trade this section made.

     **Closed 2026-08-28.** A shared root-storage plan derives slot
     interference from the liveness §68.2 item 6 already computes —
     no second fixed point — reuses a slot only across non-
     overlapping live ranges, and clears a slot when its value dies.
     Both transcribers consume that one plan.

         collect          before §68    regressed     fixed
         ship             211.7 ms      274.0 ms      207.5 ms
         dev-JIT          229.3 ms      342.7 ms      227.5 ms
         live per round   75005         100006+       75005
         live at the end  5             100006        5

     The live counts are the mechanism closed, not the timing
     improved: the 5000-node chain the program dropped is freed
     again, and both tiers agree exactly.

     **The cost, recorded.** `tree` — thirty depth-16 trees built,
     traversed, and freed with explicit `Context.free` — moved on the
     ship tier from 1.51× to 1.67×, measured twice on a quiet
     machine. Clearing a slot when its value dies costs a write, and
     `tree` frees densely. Its dev tier improved from 7.81× to 6.22×.
     **The trade is accepted**: a program that calls `collect()` and
     does not reclaim is worse than one that reclaims and runs 17 per
     cent slower on one allocation-dense shape. Invariant 2 says a
     program that never collects is correct and merely larger; it
     does not say a program that collects may keep the garbage.

     **One defect was hiding another.** The dead temporaries also
     masked a missing dev-JIT managed-global root registration, which
     surfaced and was fixed only once they were cleared.

   - **Kill criterion: a ship-AOT ratio above 1.75× stops the phase
     and reopens the form of the emitted C.**

     **Measured 2026-08-27, and the criterion is tripped.** Before
     and after, one machine, one session, `--warmup 60 --timed 15`,
     every subject's spread inside ±20 per cent, and the C baseline
     the same on both sides, which is what makes the pair valid.

         subject      before `9bde577`   after §68 step 3
         C              3.975 ms 1.00x    3.972 ms 1.00x
         emitted-C      6.099 ms 1.53x   15.928 ms 4.01x
         dev-JIT      114.151 ms 28.72x 154.661 ms 38.98x

     The ship tier is `emitted-C`; the harness's `ship-AOT` row is
     the Cranelift AOT that §11 superseded and is a cross-check.
     `1.53x` before matches §11's post-P19 record exactly, so the
     pin met this criterion and §68 is the cause.

     **The named risk is what happened.** This item predicted that
     LIR names many temporaries and that the emitted C would depend
     on the C compiler to coalesce them. The dominant cost is
     narrower than that: the innermost loop of `multiply` now takes
     the address of the locals that hold its two matrix parameters.

         after    v29 = &l0; v30 = &((v29)->d0);
                  v31 = &(((v30)->a)[v28]); v32 = *(v31);
         before   ((v_left).elements).a[((v_row * 4) + v_inner)]

     Taking `&` of a 64-byte local forces it to memory for the whole
     function, so the before shape could stay in registers and the
     after shape cannot. §68.2 item 9 makes an address a value, and
     the transcriber spells that value literally.

     `multiply` also declares 62 function-scope locals, one per SSA
     value, and copies block parameters at every edge. Both are
     secondary: the label-and-`goto` shape is unchanged from the pin,
     which measured 1.53x with it.

     **What reopens is the emitted C's form, not LIR.** An address
     chain consumed only by a load or a store is a member expression
     in C: `&x`, then `->f`, then `[i]`, then `*` is `x.f.a[i]`. The
     transcriber chooses the spelling; LIR keeps saying "address
     value".

     **Cleared 2026-08-27 at 1.34×, which is better than the pin.**
     The ship tier now measures 5.329 ms against 3.987 ms of hand C,
     with every subject's spread inside ±20 per cent. The pin was
     1.53×, so the migration ends faster than it started.

     **The cause was none of the three this session diagnosed
     first.** Address folding took 4.01× to 4.04×. Coalescing block
     parameters out of SSA took it to 4.00×. Four prologue fixes
     took it to 3.98×. Each was a real improvement to the emitted
     code and none of them mattered.

     The owner asked whether array element access called a foreign
     function more than once at a site, and whether `array_len` was
     called repeatedly. Both were true. Counted in the emitted C for
     `a22`:

         helper                     pin   before the fix
         subscript_rt_array_len       4      22
         subscript_rt_array_ptr      11       0
         subscript_rt_array_data      1      10

     The pin read `((SsArrayHeader*)h)->len` and `->data` inline and
     called the runtime **only inside a failed bounds branch**. The
     transcriber called `subscript_rt_array_len` for the test, again
     to build the trap message, and `subscript_rt_array_data` for
     the pointer — two or three opaque calls per element access, and
     one more per loop iteration for the loop condition.

     **An opaque call in a loop body is why the other three fixes
     did nothing.** The C compiler must assume such a call writes
     memory, so it cannot hoist, cannot vectorise, and spills every
     cached value across it. The aggregate copies were the symptom.
     Reading the header inline removed every one of those calls from
     `a22`'s emitted C and took 3.98× to 1.34×. The dev tier had the
     same shape and the same fix: 38.98× to 30.57×.

     **The lesson, once.** Three diagnoses failed because each read
     the emitted C for what looked wrong rather than for what the
     optimizer could not see through. Measuring one change at a time
     is what proved each of them worthless.
   - A ratio between 1.53× and 1.75× is reported to the owner with
     the emitted C of the inner loop, and the owner decides.
   - A dev-JIT ratio above 4× stops the phase.
5. **Line count.** The round reports the line count of
   `codegen/src/` before and after. This section predicted a
   decrease. An increase is evidence that the split is wrong, and
   the round reports it with the measurement.

   **Measured after step 3, and the prediction did not hold.**
   `codegen/src/` went from 30352 lines to 36195, an increase of
   5843.

       the two consumers   18951 -> 13345   (-5606, 30 per cent)
         lower/func.rs      9184 ->  7085
         cemit.rs           9767 ->  6260
       deleted             suspension.rs 871, trap_sites.rs 102
       added               lir.rs 7095, interpreter.rs 5129

   The consumers shrank as predicted. The lowering, the verifier,
   and the fact check cost about what the consumers saved, so the
   migration is flat, not down. The interpreter is 5129 of the
   increase; it is a test oracle the owner added at step 1b, after
   this item was written, and it is not part of the migration.

   **The prediction's premise was wrong, and the split is not.**
   The premise was that removing duplicate walks dominates. What
   dominates is making implicit knowledge explicit. The three walks
   were short because each re-derived, ad hoc, only what it needed;
   one lowering must state all of it once, and the verifier and the
   fact check had no counterpart at all before this section.

   So this item stops being a pass-or-fail gate and becomes a
   measurement with a reading. Judge the split on the consumers'
   size, which fell 30 per cent, on the duplicate walks being gone —
   `count_yields`, `walk_lets`, and `count_async_calls` existed once
   per tier — and on the count of facts that were implicit and are
   now checked, which is ten.
6. **Build and suite time.** The verifier runs in every build. The
   round records the debug and release suite wall time before and
   after. An increase above 20% goes to the owner with the
   measurement.
7. **Gates**: `cargo test --offline --workspace` in both profiles;
   a zero-warning build; `cargo fmt --check`; the `tsc` gate; clippy
   library counts at the 7 / 22 / 29 baseline. The record quotes the
   test count and the wall time for each step of §68.4.
8. **Tracking**: `specs/tracking/s68-one-ordered-ir.md`. Each step
   of §68.4 records its own gate run.

### 68.7 What a LIR instruction means

*(Added 2026-08-26. Step 1b stopped and reported that §68 defines the
form of LIR and not the meaning of its instructions, so no
interpreter can be written from this section. CLAUDE.md principle 8
makes that report the wanted outcome.)*

The finding matters more than the gap. §68.2 item 10 says that
neither tier decides semantics. **While the meaning of an instruction
is undefined, each tier decides it.** The two tiers agree today
because one lowering built both conventions out of HIR, not because
LIR pins a meaning. A differential gate between two tiers cannot see
that difference, which is CLAUDE.md principle 12 one level up.

**The interpreter is the completeness test for this section.** If a
reader writes it from this document alone, the document is complete.
If the reader must consult a tier, the document is not.

#### 68.7.1 How this section defines a meaning

1. **By reference where the source language already decides.** An
   instruction that carries a construct of the language means what
   the section that decides that construct says. This section names
   the section. It does not restate it.
2. **By definition where LIR has no source counterpart.** The
   iteration protocol, the address and provenance model, the
   suspension protocol, and edge argument transfer exist only in
   LIR. §68.7.4 defines them.
3. **Operands are positional.** Each row states the operand roles in
   order. The type of an operand comes from the values table, never
   from a record on the instruction (§68.1 item 9).
4. **A trap fires before the operation's effect**, in the order the
   instruction lists its sites (§68.1 item 8). A trap ends the
   program through the observer of §18.
5. **A gap is reported, never guessed.** If a reader cannot act on a
   row, the row is wrong. Report it and stop.

*(2026-08-28, review of §68 form, C1 and M1.)* Item 4 is the contract,
and the interpreter did not implement it. It read `instruction.traps`
at one place, integer `Div`/`Rem`, and raised every other trap it
raised from the runtime library or a `Trap` terminator. Measured: a
fixed-array read at index 4 of length 2 trapped on both tiers and
printed `1805878962` on the interpreter, then `4` on a second run — a
read past the payload in the oracle. `t01` (`JsonResultValue`) trapped
on both tiers and printed `0` on the interpreter. A trap position it
did raise was the instruction's, not the site's (`t27`: tiers `25:3`,
interpreter `25:19`), and no gate compared columns.

**The interpreter raises every site-owned trap kind from one dispatch
over `instruction.traps`, before the operation's effect, at the site's
position.** It has no per-kind arm and no runtime fallback for a kind
LIR owns. The trap gate compares line and column on every tier and on
the interpreter.

*(2026-08-28, review of §68 consumers, C4 and M1. Two conversions
this section said "as C does" are undefined in C, and each tier
decided one.)*

**A float to integer `as` conversion saturates, and `NaN` converts to
zero.** The value is truncated toward zero; a result below the
target's minimum is the minimum, above its maximum is the maximum;
`NaN` is `0`. Measured at `2a65724`: both tiers already do this
(`1e10 as i32` is `2147483647`, `(-1.0) as u32` is `0`, `300.0 as i8`
is `127`), and the interpreter wrapped (`1410065408`, `4294967295`,
`44`). The interpreter changes. C leaves the out-of-range case
undefined; the emitted C calls a helper that saturates, and the dev
tier uses the saturating convert. JavaScript has no such conversion,
and `as` is a no-op there, so a program that prints such a value is
not comparable: collisions.md C3 names it.

**Float `%` is the C `fmod`**, and it is in the language. The checker
accepted it, the dev tier refused it ("floating remainder is not
supported"), the ship tier emitted C `%` on a `double` and did not
compile, and the interpreter ran `fmod`. `fmod` agrees with
JavaScript's `%` for every IEEE case: the sign of the dividend, `x % 0`
is `NaN`, `Infinity % y` is `NaN`, `x % Infinity` is `x`, `NaN`
propagates. Both tiers call the runtime's `fmod`; Cranelift has no
float remainder. A program that prints such a value is comparable.

Corpus: `a165` (the three saturating conversions above, `NaN as i32`,
and float `%` over the seven cases). Its float `%` half is
`js-comparable: yes`; its conversion half cites C3.

#### 68.7.2 The instruction table

Numerics, string, and array behaviour come from the list at §2 and
from §16 for the narrow widths: two's-complement wrap on
`i32`/`u32`/`i64`/`u64`, `as` conversions that truncate and wrap as C
does, `f32` arithmetic at `f32` precision, true 64-bit bitwise
operations, and §2's Q14 formatting for interpolation.

| instruction | operands | means |
|---|---|---|
| `Copy` | value | the same value, of the same type. A value class copies by value (§62); a reference class copies the handle. |
| `StringLiteral` | none | a string of the module's literal table. §2 string rules. |
| `Zero` | none | the zero of the result type: `0`, `0.0`, `false`, or the null handle. |
| `LoadLocal` | none | the current value of the local. §68.1 item 6 limits a local to a binding whose address the program takes. |
| `StoreLocal` | value | the local takes the value. No result. |
| `AddressOfLocal` | none | the address of the local. §68.7.4 gives the address model. |
| `LoadGlobal`, `StoreGlobal`, `AddressOfGlobal` | as the local forms | the same, on a module global. |
| `FunctionRef` | none | the callable of a declared function, for an indirect call. |
| `MakeClosure` | one per capture, in declaration order | a callable with an environment that holds the captured values. §68.2 item 8 governs where the environment lives. |
| `Unary` | operand | §2 numerics, for the named operator. |
| `Binary` | left, right | §2 numerics, string, and comparison rules, for the named operator. Division by zero traps. |
| `Cast` | value | an explicit `as` conversion. §2: truncate and wrap as C does. §16 for the narrow widths. |
| `Coerce` | value | an implicit widening the checker inserted. It never loses a value. A conversion that loses a value is a `Cast`. |
| `AllocateClass` | none | a new instance, fields at their zero. The `Allocation` trap fires on failure. |
| `AddressOfValue` | value | the address of a temporary that holds the value. The temporary lives as long as the address (§68.2 item 8). |
| `AddressOfField` | base address or handle | the address of the named field. A field of an aggregate **value** has no address; `LoadField` reads it. An address exists only where the base has one. |
| `AddressOfIndex` | base, index | the address of the element. `checked` states that a bounds trap site is present; the site, not the flag, raises it (§68.1 item 8). |
| `LoadAddress` | address | the value at the address. |
| `StoreAddress` | address, value | the value is written at the address. No result. |
| `LoadField` | base handle, or an aggregate value | the field's value. The base is a reference-class handle, or a value that holds an aggregate: a value class, a `FixedArray`, or a built-in aggregate such as the generator's iteration result. *(Corrected 2026-08-26 after step 1b. The row named only the reference-class path, and 23 of the interpreter's 30 findings were the value path.)* |
| `Length` | container | the element count of an array, and the **byte** length of a string (Q5, `stdlib.md` §14). |
| `ArrayLiteral` | one per element, in order | a new array of the elements. |
| `ArraySpreadLiteral` | one per part, in order | a new array; a spread part contributes its elements in order (`stdlib.md` §14). |
| `Template` | one per interpolation, in order | the concatenation, formatted by §2's Q14 rules. |
| `Template` (no parts) | — | *(Added 2026-08-28, review of §68 consumers, C3.)* the empty string, and no trap. The dev tier emitted `""` and the ship tier reported `template consumed 0 of 1 traps`; the two transcribers decided an unstated case. It is stated here, and the lowering attaches no trap site to an empty template. |
| `Call` | see §68.7.3 | a call of the named target. |
| `IteratorCreate`, `IteratorHasNext`, `IteratorValue`, `IteratorBound`, `IteratorAdvance` | see §68.7.4 | the iteration protocol. |
| `ForeignArrayData` | array | *(Added 2026-08-28, review of §68 form, M2.)* the array's current data pointer, as an `Address` whose provenance is the array. A foreign call takes it as an operand for each array argument, so the call carries the snapshot §68.7.3 names. It is invalidated by the same instructions that invalidate any address into that array. |
| `Coerce` (an `Address` to a boundary value class, into that class's `Nullable`) | address | *(Same review, M2.)* the address as a non-null pointer value of the nullable type. This is the one `Coerce` that is not a widening: it converts an address to the C-visible pointer a boundary struct-pointer member holds (§33.1). It is legal only for a boundary value class, and the verifier admits no other address-to-data `Coerce` (§68.2 item 11). Rule 8b keeps the base rooted for the activation, and S015 keeps the value from leaving it. |
| `AllocateClass` (a value class) | — | *(Same review, M2.)* an `Address` of fresh zero-initialized storage for the class, in the activation, not "a new instance". A reference class yields a handle; a value class yields the address its constructor writes through. |
| `AsyncHandleCreate(target)` | the call's operands | *(Added for §70; the rows were missing.)* a new coroutine frame for the target, not polled, with its owner count at one, held by the result. |
| `AsyncHandleRetain` | handle | the same handle; the frame's owner count is one higher. |
| `AsyncHandleRelease` | handle | no value; the frame's owner count is one lower, and at zero the frame is freed at this instruction (§70.3 rule 3). |
| `AsyncHandleArrayRetain` | array of handles | no value; every element's frame is retained once. |
| `AsyncHandleArrayRelease` | array of handles | no value; every element's frame is released once. |

#### 68.7.3 What a call means

The target kind decides the operand roles.

| kind | operands | means |
|---|---|---|
| `Function` | the declared parameters, in order | a call of the module function. |
| `Method` | the receiver first, then the parameters | a call of the class method. A value-class receiver is an address; a reference-class receiver is a handle. |
| `Foreign` | the marshalled arguments, in order | a call across the C ABI. **The call-time view.** *(Owner, 2026-08-28. This replaces the sentence "an array argument's data pointer and count are read before a later argument runs ... taken at the argument's evaluation point", added 2026-08-26.)* An array argument is the array, as JavaScript passes a reference. The call carries the array's data pointer and count as operands, **read after every argument is evaluated and immediately before the call**. A later argument that grows the array is visible to the callee, whether or not it suspends. No snapshot exists that a later argument can invalidate, so rule 9 has nothing to recompute here and no stale pointer can reach the C side. §68.2 item 12's check verifies that the operands are read after the last argument. |
| `Indirect` | the callable first, then the parameters | a call through a value of `Type::Func`. |
| `Intrinsic` | the family's operands, in order | the operation the module's intrinsic table names. The table, not a positional index into a Rust array, defines it (§68.2 item 11). |
| `BuiltinMethod` | the receiver first, then the parameters | the standard-library method. `stdlib.md` decides each one. |

*(2026-08-28, review of §68 consumers, M6 — **open, the owner's**.)*
The sentence above and the tree disagree, and the tree disagrees with
itself. Measured at `2a65724`, a foreign call whose array argument is
grown by a later argument:

    grow in a later argument, no suspension            2   (snapshot before the later argument)
    grow after an await earlier in the function        2
    grow in a later argument that contains an await    3   (snapshot in the resume block, after grow)

`codegen/tests/interop.rs` pins the third as `f2suspend=3`. The
sentence above gives `2` for all three. Rule 9 forces the third: the
snapshot is an address into the array, `grow` invalidates it, and an
address that crosses an invalidation is recomputed. The first case
carries the hazard rule 9 exists for: a snapshot taken before a later
argument that reallocates the array is a stale pointer at the call.

Two consistent answers exist. **Call-time view**: the snapshot is
taken after every argument, so all three print `3`, the array is the
reference JavaScript passes, and no stale pointer is possible.
**Evaluation-point view, made safe**: the snapshot is taken at the
argument's evaluation point and the later argument's growth is a
trap, so the first and third cases stop. The first keeps §67.2 rule 7's
intent for sync code and changes one pinned value; the second changes
which programs run. **Decided: the call-time view** *(Owner, 2026-08-28)*. All three
cases print `3`. Every test pin that recorded the evaluation-point
value moves to the call-time value: in `codegen/tests/interop.rs`,
the `f2sync=2` twin and
`foreign_call_without_suspension_preserves_marshalling_order`'s
`f2=2` both become `3`; the `f2suspend=3` twin is already the
call-time value. No corpus `.expected` prints the sync case (`a149`
does not), so none moves.

#### 68.7.4 The three protocols that LIR alone defines

**Iteration.** `stdlib.md` §14 decides what `for...of` accepts and in
what order. §14.3 states that the loop is an index loop over the
container's own storage and that no iterator object exists. LIR
carries that as five instructions over three values — a cursor, an
index, and a bound — which §68.1 item 7 threads across the back edge.

- `IteratorCreate(kind)` takes the subject and produces the cursor.
- `IteratorBound` takes the cursor and produces the bound. *(Revised
  2026-08-29, owner, C13 retired.)* **Which bound depends on the
  spelling, and LIR carries which.** `Array.prototype.forEach` fixes
  its range before the first call, as ECMA does, so its bound is the
  element count at creation, read once. `for...of` over any kind, and
  `Map`/`Set` `forEach`, observe the live container: their bound is the
  container's current element count, read at every step, so an append
  during the traversal is visited. The lowering states the choice on
  the cursor's kind, both tiers and the interpreter read it, and the
  verifier checks that a fixed-bound cursor is created only by an
  `Array.forEach` lowering. Under the live bound the "position is
  within the container's current element count" clause of
  `IteratorHasNext` is the bound.
- **The cursor names a position in the container's own storage.** The
  bound is a position too, captured at creation. A kind whose storage
  holds no hole — an array, a `FixedArray`, a string — has the
  position equal to the index.
- `IteratorHasNext` takes cursor, index, and bound. It is true while
  the cursor names a live position, that position is below the bound,
  and the position is within the container's current element count.
- `IteratorValue` takes cursor, index, and bound, and produces the
  entry at the cursor's position, as `stdlib.md` §14 names it for the
  kind.
- `IteratorAdvance` takes cursor, index, and bound, and produces **one
  result: the next cursor**, at the next live position after the
  current one. The index advances by one, separately, and no rule
  reads the index for liveness.

*(Corrected 2026-08-26, twice, after step 1b. The first text ended a
traversal on the bound alone and trapped on `a80`. The second gave
`IteratorAdvance` two results, which no LIR instruction has. The
cursor carries the position, so one result is enough.)*

`corpus/accept/a80-for-of-foreach-mutation` decides the rule, in its
own header: "appends do not extend and removals shorten". The bound
is captured at a position, so an entry appended past it is never
reached. The current count is read, so a removal ends the traversal
early. An inactive position is never a body iteration, and the
protocol needs no edge that skips the body.

Worked against a80's golden. A `Map` of keys 1, 2, 3 deletes key 2
and appends key 4 on the first step, and the bound is 3:

    cursor at position 0, live, 0 < 3       visits key 1
    advance skips dead position 1           cursor at position 2
    cursor at position 2, live, 2 < 3       visits key 3
    advance                                 cursor at position 3
    position 3 is not below the bound       stops

An array of `1, 2, 3, 4` that pops twice on the first step, bound 4:

    position 0, 0 < 4, 0 < count 4          visits 1
    advance                                 position 1
    position 1, 1 < 4, 1 < count 2          visits 2
    advance                                 position 2
    2 < 4 holds, 2 < count 2 fails          stops

**Addresses and provenance.** An address is a value. An address into
a dynamic array carries the array value it came from, as provenance.
§68.2 item 9 requires every instruction that can move an array's
storage to name the arrays it invalidates. **An address whose base is
invalidated is not used again.** The lowering re-computes it. An
interpreter poisons it, and a use of a poisoned address is an error
that names the instruction and the invalidation. Neither tier
performs that check, so the interpreter is the only place it exists.

**Async driver.** *(Added 2026-09-08 by §94.)* The suspension
kind determines registration: `Async` parks for the next checkpoint;
`AsyncCall` and `AsyncHandle` register on the awaited frame.
A completed await also suspends. Completion queues continuations;
only the host checkpoint drains them. The resume edge reads the
cached result and never drives the child. §94 defines this protocol
for both tiers and the independent interpreter. Generator suspension
keeps its existing protocol.

**Suspension and resume.** `Suspend` is a terminator with a successor
block id (§68.2 item 7). **The successor block's parameters are the
live-in set of the suspension.** When the suspension produces a
value, that value is the successor's first parameter. The frame holds
exactly the successor's parameters and nothing else.

**`Suspend` carries its source position.** *(Added 2026-08-26 after
step 2.)* A resume after a hot reload raises `StaleCoroutine`, and
the position it reports is the suspension's. `Trap` carries a
position for the same reason (§68.7.5). The total check of §68.2 item
12 reported 49 sites once it knew to look.

**`Suspend` carries an argument list, as every other edge does.**
*(Added 2026-08-26. Step 1b reported that the section decided what
the frame holds and not how a value reaches it. Reusing a value's id
as a successor parameter breaks §68.1 item 4, and the edge-transfer
paragraph named only the three branching terminators.)* The arguments
bind to the successor's parameters by position, at the moment the
coroutine resumes. A resume value, where the suspension produces one,
is the first parameter and has no argument: the resume supplies it. *(This decides
the case the second step 1 review raised: a resume block had no place
to carry state other than the resume value.)*

**Edge argument transfer.** `Branch`, `ConditionalBranch`, and
`Switch` each carry an argument list per edge. The arguments bind to
the destination block's parameters, by position, at the moment the
edge is taken. The arguments are read before any binding happens, so
a swap across an edge is well defined.

*(2026-08-28, review of §68 form, M6; the bound sentence revised
2026-08-29 when C13 retired.)* The interpreter implemented a different
machine: it read the bound when `IteratorBound` executed, moved the
cursor in `IteratorAdvance` and skipped dead positions inside
`IteratorHasNext`, which wrote to the cursor. **The text above is the
contract.** `IteratorHasNext` is pure: a cursor is an SSA value (§68.1
item 4) and no instruction mutates it. The skip over dead positions is
`IteratorAdvance`'s. The bound is the spelling's, as the `IteratorBound`
row states.

#### 68.7.5 The terminators

| terminator | means |
|---|---|
| `Branch` | the single edge is taken. |
| `ConditionalBranch` | the condition is a `boolean` value. True takes the first edge. |
| `Switch` | the discriminant is compared for equality against each arm's constant, in order. The first equal arm is taken, and the default arm otherwise. |
| `Return` | the function ends. A value is returned when the signature declares one. A coroutine's return completes it. |
| `Trap` | the program ends through the observer of §18, with the named kind and position. |
| `Suspend` | see §68.7.4. |
| `Unreachable` | *(Added 2026-08-28, review of §68 form, M2.)* a successor that checked semantics prove unreachable. It is structural: it carries no trap site and no language meaning. Reaching it is an internal error of the lowering, and the interpreter reports it as invalid LIR, never as a program trap. The text golden prints it as itself. |

#### 68.7.5a What step 1b measured

*(Added 2026-08-26.)* The interpreter ran 98 corpus entries, matched
68 against the golden, and reported 30 disagreements. 51 entries are
declared exclusions, almost all of them interop entries that need the
synthetic native library.

Every disagreement fell into four groups, and each group is one
cause:

1. **23 entries: a field of an aggregate value.** §68.7.2's field
   rows named only the reference-class path. Corrected above.
2. **5 entries: a suspension's successor did not declare the values
   used after it.** §68.7.4 decides that the successor's block
   parameters are the live-in set and that the frame holds those and
   nothing else. The lowering does not build the successor that way,
   so the interpreter, following this section, discarded the values.
   **This blocks step 2.** Both tiers work today because both read
   HIR; a dev tier that reads LIR loses the state. The second step 1
   review predicted this as its M7, from reading. The interpreter
   measured it, on `a110`, `a139`, `a143`, `a145`, and `a149`.
3. **1 entry: the iteration contract contradicted `a80`.** Corrected
   above.
4. **1 entry: the standard runner.** §26.3 requires the runner to
   invoke every exported zero-parameter async function; the
   interpreter invoked `main` only. That is an incompleteness of the
   interpreter, not of LIR.

Groups 1 and 3 are defects in this section. Group 2 is a defect in
the lowering, and it is the one no gate could see, because nothing
consumed LIR.

#### 68.7.6 Language gaps this exercise found

These are gaps in the **language**, not in LIR. Each belongs to the
section that owns the construct, and each needs an owner decision.
LIR carries whatever that section decides.

1. **A container that changes while a `for...of` runs.** *(Corrected
   2026-08-26 after step 1b. This entry said the language had not
   decided it. The language had:
   `corpus/accept/a80-for-of-foreach-mutation` states "appends do not
   extend and removals shorten" and pins both. The corpus is the
   executable definition, so this is a **decided divergence with no
   collision entry**, not an open decision. It belongs to §69, not to
   the owner's queue.)* Measured 2026-08-26 on this host, on an array
   of `1, 2, 3` that pushes `4` on the first step:

       node        1 2 3 4   len=4
       subscript   1 2 3     len=4

   The array grows on both sides. `node` re-reads the length each
   step, and this compiler reads the bound once. **a80 decides this,
   so no decision is open.** `stdlib.md` §14.3 decided the fused
   index loop, and a80 decided the mutation rule that follows from
   it. The work is to give `collisions.md` the entry, under §69. No
   behaviour moves.

   The cost is worth recording, because it is why the two answers are
   not interchangeable: a live bound re-reads the base and the length
   every step, and §68.2 item 9 then re-materializes the base address
   every step, on the loop that `a22-matrix-propagation` measures.
2. **The temporal-dead-zone resolution order** (§66 measurement 6i).
   `node` prints `4` and this compiler prints `3`. No entry of
   `collisions.md` names it. **Owner decision open.**
3. **The order of module initialization.** HIR splits global
   initializers from top-level statements, so source order is not
   recoverable. Neither tier emits top-level statements today.
   **Owner decision open.**

*(§66 measurement 6j, the missing duplicate-declaration diagnostic,
was on this list and is closed. §67 pass A rejects a duplicate
declaration, and `corpus/reject/r149`, `r150`, and `r151` pin it at
S100. Three gaps stay, not four.)*
