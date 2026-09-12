<!-- §67 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 67. Checker scoping, and state that must survive a suspension

Origin: the §66 arc recorded three constructs it could not fix
(measurements 6e, 6i, 6j), and its reviews found four more defects
outside its subject. Owner decision 2026-08-25: **all seven land in
one cycle, in two passes.** One contract, two implementation and
review passes, because one review does not cover that surface —
the lesson of §66's own seven rounds.

Pass A is checker semantics: what the language accepts. Pass B is
lowering: what an accepted program does. The two passes share no
code and no corpus entry.

Measurements at `a239de7`, on this host. Every one is pre-existing.

Pass A. Each program is one stock `tsc` rejects and this compiler
accepts, so each breaks invariant 5.

1. A `switch` case reads a name a earlier case declared. Measured:
   the dev tier prints `case1:1` and the ship tier prints
   `case1:99`, with no diagnostic. `tsc` reports TS2454. The checker
   gives each case its own scope; TypeScript gives the whole switch
   body one scope, and the emitter writes one flat C block.
2. A function parameter and a body local of one name. Measured: the
   dev tier prints `7`; the ship tier stops with "redefinition of
   'v_n'". `tsc` reports TS2300.
3. Two `const` of one name in one block. Measured: the dev tier
   prints `2`; the ship tier stops with "redefinition of 'v_n'".
   `tsc` reports TS2451.
4. A lambda body declares a name that a nested lambda reads earlier
   in the same body. Measured: `node` prints `4`; both tiers print
   `3`. `tsc` accepts, because the read sits inside a closure; a
   direct read reports TS2448. **The two tiers agree here**, so this
   one is not a tier divergence. It is a divergence from TypeScript
   semantics, and it is the only measurement in this section whose
   fix can change what an accepted program prints.

Pass B. Each program is `tsc`-clean, so each is a valid subscript
program that does not run correctly.

5. Two `await` expressions in one expression. Measured: the dev
   tier stops with "internal lowering error: define async resume:
   Compilation(Verifier(... uses value v27 from non-dominating
   inst38))"; the ship tier runs and prints a corrupt first value.
6. A capturing lambda created before a suspension and called after
   it. Measured: the dev tier prints `15` then `-1927167400`; the
   ship tier prints `15` then `5`. The correct output is `15` twice.
   Both tiers are wrong, and they disagree.
7. `await` inside a `for...of` body. Measured: the dev tier stops
   with the same verifier error as item 5; the ship tier prints one
   iteration and stops. `yield` inside a `for...of` body fails the
   same way.
8. Two generators of one yield type in one module. Measured: the
   dev tier prints `1` then `2`; the ship tier refuses to emit,
   with "ambiguous generator resume target".

Items 5, 6, and 7 are one root cause: **a value that is live across
a suspension does not live in the coroutine frame.** The Cranelift
verifier names it exactly — a value defined before the suspension
is used after it, in a block the definition does not dominate. Item
8 is a separate defect: `generator_of`
(`codegen/src/cemit.rs`) recovers the target by searching for the
one generator whose yield type matches, and its own comment records
that it cannot recover the creator. The dev tier has no such search:
it stores the resume address in the frame at creation
(`codegen/src/lower/func.rs`, `GEN_RESUME_OFF`) and calls through
it.

### 67.1 Pass A rules — narrow, never diverge

The guiding rule: where this compiler and TypeScript disagree, this
compiler **rejects**. It never accepts a program and gives it a
different value. Rejecting more is inside invariant 5; computing a
different answer is not.

*(Clarified 2026-08-26 after the pass A review. Invariant 5 runs one
way: an accepted program must type-check under stock `tsc`. It does
not say a `tsc`-clean program must be accepted. This language is a
subset, and 23 reject entries already carry a
`tsc-clean-standalone` header. A rule that rejects a `tsc`-clean
program is therefore ordinary, and its entry records the `tsc`
verdict in the header. The pass A handoff stated the converse by
mistake, and the round put the weaker `+=` spelling in the corpus to
satisfy it.)*

1. A `switch` body is one scope. A declaration in one case is in
   scope for the whole body, as TypeScript has it. Two declarations
   of one name in one switch body fail with S100 that names the
   switch, matching the direction of TS2451.
1a. **The scope owns the disposal.** *(Added 2026-08-26 after the
   second pass A review.)* A `using` declaration in a case belongs
   to the switch body, so §60's hook runs at each exit of the switch
   body, in reverse declaration order across every case. Measured:
   `using a` in `case 0` falling through into `using b` in `case 1`
   prints `case0 / case1 / dispose:b / dispose:a / end` under
   TypeScript (downlevelled to ES2022, `node` v24.18.0), while this
   compiler printed `case0 / dispose:a / case1 / dispose:b / end` on
   both tiers. Rule 1 moved the scope and left the disposal site
   behind, so the round created that inconsistency.
2. A read of a name that a **different** case declares fails with
   S100 at the read. TypeScript rejects the same program through a
   definite-assignment analysis (TS2454) that this compiler does not
   have; this rule is narrower and needs no such analysis.
3. Two declarations of one name in one scope fail with S100 at the
   second declaration. A function parameter and a body local of one
   name are two declarations in one scope, because the body opens no
   scope of its own.
3a. **A class body is one member namespace: one name, one member.**
   *(Owner, 2026-08-29, after the Fable phase review of
   §66–§67, M1.)* Rule 3 is stated for a scope; a class body is the
   scope of its instance members, and §65 applied the rule to
   accessors only. Measured at `857757a`, `tsc` under the corpus
   gate's options:

       field + method     checker accepts; c.x prints 1, c.x() prints 2     tsc TS2300 ×2
       method + method    checker accepts; the lowering fails internally    tsc TS2393 ×2
                          ("class `C` has duplicate checked method `x`")
       field + field      checker accepts; prints 2, the later wins         tsc TS2300
       field + accessor   S100 (§65)                                        tsc TS2300 ×2
       static + instance  S100, static fields are undecided                 tsc accepts

   The checker resolved a member by its use — a read found the field,
   a call found the method — and accepted programs `tsc` rejects, which
   invariant 5 forbids. The method pair reached the lowering, which
   reports an internal error where a diagnostic belongs.

   **Rule.** Two instance members of one name in one class body —
   a field, a method, or an accessor pair, in any combination — fail
   with S100 at the second declaration, naming both kinds. §65's
   accessor rule is one case of this rule and keeps its message. The
   static namespace is separate, as `tsc` has it; static fields stay
   undecided under their own S100.

   No accept entry has a clashing class (measured, 0 of 165). Corpus:
   `r161` (field and method), `r162` (two methods), `r163` (two
   fields), each with the measured `tsc` code in its header.
4. A block-scoped declaration owns its name for the whole block. A
   read of that name earlier in the same block fails with S100 at
   the read, whether the read is direct or inside a nested lambda.
4a. **Rules 2 and 4 reach every name, not only a local.** *(Added
   2026-08-26 after the second pass A review.)* A declaration owns
   its name against an ambient namespace and against a class name
   too. Measured, each accepted here and rejected by `tsc`:
   `Math.abs(-2.5)` before `const Math: i32 = 3` printed `2.5:3`
   (TS2448, TS2454); `new Foo()` before `const Foo: i32 = 9` printed
   `1:9` (TS2351, TS2448, TS2454); the same two shapes across two
   switch cases behaved the same. Every site that asks whether a
   name is shadowed must consult the pending declarations and the
   switch declarations, not the bound locals alone.
   TypeScript accepts the closure form and rejects the direct form
   (TS2448); this rule rejects both, so **no accepted program
   changes its value**. Measurement 4's program becomes a rejection.
4b. **A local that owns a class name hides the class from that
   point.** *(Added 2026-08-26 after the third pass A review, which
   found the rule already widened this far while 4a described only
   the read-before-declaration shape.)* `const Foo: i32 = 9` before
   `new Foo()` fails with S100. Stock `tsc` rejects the same program
   (TS2351), so the direction is right. The message names the
   shadow. It must not say the class is unknown, because the class
   is declared and a local owns its name.
4c. **Module scope: an initializer reads only what is already
   initialized.** *(Added 2026-08-28 after the Fable phase review of
   §66–§67, finding C1.)* Rules 3 and 4 stop at the function boundary
   and did not say so. `reserve_block_declarations` runs for every
   function, lambda, block, branch, and switch body, and never for the
   module. Measured at `2a65724`, `tsc --strict` accepts this program:

   ```ts
   class Box { value: i32 = 5; }
   const g: Box = f();
   function f(): Box { return h; }
   const h: Box = new Box();
   export function main(): void { print(`h=${h.value}`); print(`g=${g.value}`); }
   ```

       subscript check   no error
       dev tier          signal 11
       ship tier         h=5, then SIGSEGV
       node              ReferenceError: Cannot access 'h' before initialization

   With `i32` in place of `Box`, both tiers print `0:1` and no
   diagnostic. With `string`, the null string prints nothing. The
   direct form `const a: i32 = b; const b: i32 = 2;` is accepted here
   too and prints `0:2`; `tsc` rejects it (TS2448).

   **Rule.** Module-level bindings initialize in declaration order. A
   module-level initializer reads or writes, directly or through any
   function it calls, only a module-level data binding declared before
   it. A violation fails with S100 at the initializer. The message
   names the binding and the call path that reaches it.

   The check is a fixpoint over the module's functions. A function's
   global set is its direct global reads and writes, plus the global
   sets of the functions it calls. An indirect call through a function
   value reads every global. A `function` declaration is hoisted and
   is not a binding for this rule. A class name is not a binding. A
   `const`, `let`, or `using` at module level is.

   **Why reject, not trap.** Invariant 6 asks for a clear, early
   error. A trap needs a null check at every read of a global
   reference, and every program pays it for a defect only a module
   initializer can cause. `tsc` accepts the function-mediated form and
   `node` throws; this compiler narrows (invariant 5). **No accepted
   program changes its value**: measured at `2a65724`, zero accept
   entries hold a calling module-level initializer followed by a later
   data binding.

   Corpus: `r158` (the direct form; `tsc` rejects, TS2448), `r159`
   (the function-mediated form; `tsc` accepts, `node` throws), `a160`
   (the legal shapes: an initializer that reads an earlier binding
   directly and through a function; a function declared after the
   initializer that reads only earlier bindings; a later binding read
   only from `main`).
5. Nothing else moves. Every other accepted program keeps its
   diagnostics and its output.
6. §66 measurement 6e's note applies: once a switch body is one
   scope, the emitter's per-case scope restore must go, because a
   name a case declares is then in scope in a later case.

### 67.2 Pass B rules — the frame holds what outlives a suspension

1. **Every value that is live across a suspension lives in the
   coroutine frame.** This holds for a temporary inside a composite
   expression, for a lambda environment, and for the loop state of a
   `for...of`. Neither tier keeps such a value in a register, on the
   C stack, or in a frame that the resume abandons.
1a. *(Added 2026-08-26 after the pass B review, which found rule 1
   implemented at four sites and measured seven `tsc`-clean shapes
   that still lose a value.)* "Every value" is the whole rule, not a
   list. The sites the first round missed, each measured: an array
   literal whose element suspends; a `new` whose argument suspends;
   a method receiver evaluated before a suspending argument; an
   assignment target resolved before a suspending right side; and
   the pre-read of a compound assignment. `xs[1] += await a()` is
   the worst of them — the dev tier stops with the verifier error
   and the ship tier prints `xs=1,2` where `xs=1,5` is correct, with
   no diagnostic.
1b. **The frame holds a slot only for a value that is live across a
   suspension.** *(Added 2026-08-26.)* The first round reserved one
   slot for every expression node in the body: an async function of
   100 statements with one `await` emitted a 3636-byte arena, about
   36 bytes per statement, in a per-invocation Context allocation.
   The planner reserves where a later sibling can suspend, and it
   reuses a slot whose value is dead.
1d. **The plan and the two lowerings walk in one order, and a test
   proves it.** *(Added 2026-08-26 after the second pass B review.)*
   The ship tier needs the frame layout before it emits the body, so
   a pre-pass is structural. That pre-pass and each tier's lowering
   consume one event list through a strict cursor, so any
   disagreement about order or kind is a hard error at compile time
   and a wrong slot at run time. Measured: the planner visited a
   `for` as init, cond, step, body while both tiers lower init,
   cond, body, step, and it visited a `switch` as discriminant then
   test-and-body pairs while both tiers emit every test and then
   every body. Both mismatches refused ordinary programs. A unit
   test asserts that the plan's event kinds equal each tier's
   request sequence, for every statement form.
1g. **Each tier consumes the whole event list, and a short cursor is
   a compile error.** *(Added 2026-08-26 after the third pass B
   review, which is the mechanism this arc lacked.)* The cursor is
   strict on kind but nothing asserted that it reached the end of a
   coroutine body, so a site the planner reserved for and a tier
   never spilled was silent. The planner walks every callee kind, so
   it already reserves for a site no tier closed: the check turns
   that reservation into an error the compiler reports, and it turns
   the search for unclosed sites from a review's guesswork into a
   corpus run. Measured before the check existed: a frame declared
   `spill0`, `spill1`, and `spill2` while the emitted body wrote
   only `spill0`, and the two unwritten slots were a foreign call's
   marshalled arguments, which the ship tier then read as garbage —
   `probe=4347879728` where `probe=2` is correct, with no
   diagnostic. Each tier asserts at the end of a coroutine body that
   the spill cursor and the lambda cursor are both exhausted.
1e. **Correctness before size.** *(Added 2026-08-26 after the second
   pass B review.)* Rule 1b narrows what the frame holds. A
   narrowing is admissible only when the same traversal that
   assigns slots proves the value dead, and a narrowing that is
   wrong is a silent wrong answer, not a larger frame. Measured
   after the first narrowing landed: a lambda captured before a
   loop and called after a suspension inside it printed `15` then
   two garbage values on the dev tier and `15` then two zeroes on
   the ship tier, because the scan walked the loop body once and
   missed the back edge; and a lambda reached by assignment rather
   than by `let` got no frame environment at all. When liveness is
   in doubt, reserve.
1f. **A slot's live range is the value's, not the statement's.**
   *(Added 2026-08-26 after the second pass B review.)* Measured:
   two lambda environments of one capture shape, both live across
   suspensions, shared one frame member, so an inner environment
   overwrote an outer one and both tiers printed the inner value
   twice. *(Widened after the third review.)* The live range
   is the range of the **local that holds the value**, not of the
   statement list that contains the literal. Measured: a lambda
   assigned to an outer local from inside a nested block kept its
   environment in a C block-local the frame abandons, so both tiers
   printed a wrong number and disagreed with each other; and a
   second lambda of one capture shape reused the member of a first
   that a nested block had assigned to an outer local, so both tiers
   agreed on a wrong answer, which the differential gate cannot
   see.
1c. **A spill slot is a typed frame member.** *(Added 2026-08-26.)*
   The first round emitted one untyped byte array and read it as
   `(*((T*)(void*)(_f->_spill + N)))`. The ship tier compiles with
   `-std=c11 -O2` and no `-fno-strict-aliasing`, so reading an
   `unsigned char[]` object through an incompatible lvalue is
   undefined (C11 6.5p7). The frame already carries typed members
   for the `let` declarations; a spill slot takes the same form.
   That also removes the offset arithmetic and the alignment
   round-up the two tiers computed differently.
1h. **Liveness is a property of evaluation order, not of source
   order.** *(Added 2026-08-26 after the fourth pass B review.)* The
   scan that decides whether a value is live across a suspension
   walks the same traversal that the planner walks to emit events.
   A scan that walks HIR source order treats a read that precedes
   the suspension in the text but follows it in evaluation as dead.
   Measured, each a wrong answer with no diagnostic and a tier
   disagreement: a lambda called with a suspending argument, where
   the callee is read first and used at the call — the dev tier
   printed `s02=1929953583` and the ship tier printed `s02=3` where
   `s02=10` is correct; a capturing lambda passed as an argument
   beside a suspending argument — `x01=-1777237247` and `x01=1`
   where `x01=15` is correct; and a `default` arm written before a
   suspending `case` test, which the switch reaches only after every
   test has run — `P2=-519027216` and `P2=0` where `P2=15` is
   correct. Three shapes of one root cause. Rule 1e already says
   that a doubtful liveness reserves; a scan that cannot see the
   read is not in doubt, so the rule needs the traversal, not more
   cases.
1i. **One function computes a spill's kind, and the planner and both
   tiers call it.** *(Added 2026-08-26 after the fourth pass B
   review.)* The planner took the kind from the expression type; the
   dev tier took it from the declared type at four sites — a
   parameter type, a function-type parameter, and two `FixedArray`
   element types. Where the two differ, the strict cursor of rule 1d
   refuses a `tsc`-clean program that the ship tier compiles and
   runs correctly. Measured: `take(null, await av(3))` against
   `function take(b: Box | null, n: i32)` stopped the dev tier with
   "coroutine spill event mismatch: planned Value(Null), lowered
   Value(Nullable(Class(ClassId(0))))" while the ship tier printed
   `P1=3`, which is correct. A strict cursor is a check on agreement,
   not a source of it.
1j. **The receiver of a suspending async call is a spill site both
   tiers close.** *(Added 2026-08-26 after the fourth pass B
   review.)* The planner reserves for it and neither tier consumes
   it, so rule 1g refuses the program. Measured: `await
   m.step(await av(5))` stopped both tiers with "coroutine spill
   cursor stopped at 1/2" where `P8=15` is correct. This is the one
   unclosed site that remained after rule 1g landed, and rule 1g
   named it rather than a review finding it.
1k. **A capturing lambda created inside a coroutine always holds
   its environment in the frame. No liveness test decides it.**
   *(Added 2026-08-26 after the fifth pass B review. This rule
   deletes machinery; it does not add any.)* Rounds 2 to 5 each
   narrowed the reservation and each narrowing was wrong in a new
   way. The fifth review measured four more holes in one scan, and
   two of them are not holes but boundaries: a lambda passed to a
   coroutine callee is used after the **callee's** suspension, which
   an intraprocedural scan cannot see; and liveness through a
   capture is transitive, because a lambda that captures a lambda
   keeps a pointer to the second environment. Measured, each a wrong
   answer with no diagnostic and a tier disagreement: a lambda
   passed to an async callee that suspends before it calls it — the
   dev tier printed `p6=399051668` and the ship tier printed `p6=0`
   where `21` is correct, and AddressSanitizer named it
   `stack-use-after-return`; a lambda whose destination local is
   declared in a `for` initializer, whose scope the trace closes
   before the body — `after=-665124504` and `after=0` where `21` is
   correct; a chained assignment `h1 = h2 = <lambda>`, where the
   environment takes the outermost target's binding — `h2only=
   1905131784` and `h2only=0` where `18` is correct; and a lambda
   reached only through another lambda's captures —
   `after=-761642027` and `after=1` where `21` is correct. Rule 1e
   says that a doubtful liveness reserves. Four rounds of evidence
   say this liveness is always doubtful, so the test goes. Rule 1b's
   size measurement was taken on expression spill slots, not on
   lambda environments, and it does not carry here. The liveness
   narrowing stays for expression spill slots, where the strict
   cursor of rule 1d proves the trace against both tiers.
1l. **A statement the lowering skips still consumes its planned
   events.** *(Added 2026-08-26 after the fifth pass B review.)* The
   dev tier stops at a terminator and skips the rest of a statement
   list. The trace walks the skipped statements, so rule 1g's
   end-of-body check reports events no tier consumed, and the dev
   tier refuses a program the ship tier compiles and runs.
   Measured: a `return;` followed by `xs.push(await av(2))` stopped
   the dev tier with "coroutine spill cursor stopped at 0/1" while
   the ship tier printed `start`, which is correct, and both tiers
   printed `start` at the pin. This is a regression that rule 1g
   introduced. Either the lowering advances every cursor across a
   skipped statement, or it does not skip. The check must not
   change which programs compile.
1m. **The operand tables changed a synchronous program, and that
   is rule 7 again.** *(Added 2026-08-26 after the sixth pass B
   review.)* The dev tier evaluates every argument into a table and
   then pushes them, so an aggregate operand is copied after a
   later operand has run. A later operand that overwrites the
   aggregate wins. Measured on a program that holds no `async` at
   all: `sink(h1.v, bump(h1))`, where `bump` overwrites `h1.v`,
   printed `call=199` on the dev tier and `call=31` on the ship
   tier, and `31` is correct and is what the pin printed on both
   tiers. The same shape reproduces on a `FixedArray` argument, on
   an indirect call, on a constructor, and on an array literal. The
   copy of an aggregate operand happens at the operand, not at the
   call. Rule 7 states the requirement; this rule names the second
   site that broke it.
1n. **A boundary struct's `new` is a spill site both tiers close.**
   *(Added 2026-08-26 after the sixth pass B review.)* The planner
   reserves for the receiver and for each argument that a later
   suspension outlives. The boundary branch of each tier stores
   arguments positionally and consumes nothing, so rule 1g refuses
   the program. Measured on `new SubRect(1, await av("rect-y", 2),
   3 as u32, 4 as u32)`: the dev tier stopped at "cursor stopped at
   1/2" and the ship tier at "cursor stopped at 0/2", where the pin
   compiled, linked, and printed `rect=1,2,3,4` on the ship tier.
   Rule 1l applies: the check must not change which programs
   compile.
2. Two `await` expressions in one expression are legal, and each
   operand evaluates once, left to right, with the earlier result
   held in the frame across the later suspension.
2a. **The ship tier allocates a suspension's label number after it
   emits the operands, not before.** *(Added 2026-08-26 after the
   fourth pass B review.)* `eval_async_call` read the yield counter
   before it emitted the argument list, so a nested `await` in that
   list took the number the outer call had already claimed. Measured
   on `await ai(await av(2))`: the dev tier printed `s01=3`, which
   is correct, and the ship tier stopped the C compiler with
   "redefinition of label '_gresume0'". The defect predates this
   section; the dev tier failed on the same program at the pin, so
   the two tiers agreed by both failing. Rule 2 states that the
   program is legal, so this section fixes it.
3. A capturing lambda created before a suspension and called after
   it reads the values it captured. Its environment lives in the
   frame.
4. `await` and `yield` inside a `for...of` body are legal. The loop
   subject, the index, and the bound live in the frame.
5. The ship tier dispatches a generator resume through the frame, as
   the dev tier does. `generator_of`'s search by yield type is
   deleted. Any number of generators of one yield type is legal, and
   a generator handle passed to a function resumes correctly.
6. Both tiers agree byte for byte on every program above.
7. **A program that does not suspend keeps its output.** *(Added
   2026-08-26 after the fourth pass B review.)* This section changes
   coroutines. It does not change the evaluation or the marshalling
   order of any other program. Measured: the round evaluated every
   operand of a foreign call before it marshalled any argument, for
   every call, because the pre-evaluation is keyed on the callee
   kind and not on the presence of a suspension. An array argument's
   data pointer and count are then read after a later argument has
   run. A later argument that grows the array moves its storage.
   Measured on a call whose third argument pushes to the array of
   the second: the pin printed `f2=2` and the round printed `f2=3`.
   The comment at the marshalling site states the old order and the
   reason for it, and the round left the comment in place. The order
   at the pin holds. If a suspension in a later argument makes the
   old order impossible, the round reports the conflict and changes
   nothing; the choice is not the round's.
7a. **The conflict of rule 7 is real, and it is recorded here rather
   than decided by a round.** *(Added 2026-08-26 after the fifth
   pass B review.)* The round did not report the conflict, and it
   changed the suspending case. Measured on two programs that differ
   only in whether the third argument suspends: the plain call gives
   `f2sync=2 len=3` and the suspending call gives `f2suspend=3
   len=3`, on both tiers. At the pin the suspending program did not
   run at all, so `3` is a new value, not a restored one. A
   marshalled array pointer and count cannot survive a suspension,
   because a collection moves the storage, so the pin's order is
   unavailable when a later argument suspends. The behaviour stands
   as measured and a corpus entry pins both twins. **Owner decision
   open:** whether a foreign call whose later argument suspends is
   legal at all, or is a compile error, or keeps this order. Until
   the owner decides, the compiler keeps this order and the entry
   records that the value is not settled.

   **Decided 2026-08-28: the call-time view** (§68.7.3, the Foreign
   row). The data pointer and count are read after every argument and
   immediately before the call, so `f2sync` and `f2suspend` both give
   `3`. The suspending call is legal. Rule 7's "before a later
   argument runs" is withdrawn for foreign array arguments.

### 67.3 Changes by site

Pass A, `compiler/src/check/`: the `switch` case scope becomes one
scope for the body (`stmt.rs`); `fx.declare` reports a duplicate in
one scope (`mod.rs`); a block records its declarations before it
checks its statements, so a read earlier in the block resolves to
the later declaration and reports; the lambda body check consults
that record across the closure boundary. `codegen/src/cemit.rs`
drops the per-case scope restore (§66 rule 6e).

Pass B, `codegen/src/lower/func.rs` and `codegen/src/cemit.rs`: the
frame gains a slot for every value live across a suspension, and
both tiers spill and reload it; the lambda environment of a
capturing lambda inside a coroutine becomes a frame field; the
`for...of` loop state becomes frame fields; the ship tier stores the
resume address in the frame at creation and calls through it, and
`generator_of` is deleted.

### 67.4 Corpus and gate (pre-registered exit criteria)

Red first, per pass, at the contract pin: the measurements above,
recorded with their outputs.

Pass A:

1. `corpus/reject/r148-switch-cross-case-read.ts`,
   `r149-switch-duplicate-declaration.ts`,
   `r150-parameter-and-local.ts`, `r151-duplicate-const.ts`, and
   `r152-read-before-declaration.ts` — the last one in the nested
   lambda form of measurement 4. Each pinned by code and line. None
   carries a `tsc-clean-standalone` line, because `tsc` rejects the
   first four; `r152` records that `tsc` accepts the closure form
   and that this rule is narrower.
2. `corpus/accept/a147-switch-body-scope.ts` + `.expected`: a
   `switch` whose cases declare and use distinct names, and one case
   that declares a name a later case does not read, so the one-scope
   rule is exercised without a rejection.
3. **No existing accept entry may move.** If rule 4 rejects one, the
   round stops and reports it: that is evidence the rule is too
   broad, not a golden to update.
4. `corpus/reject/r153-switch-cross-case-write.ts`: a plain
   assignment to a name a different case declares. *(Added
   2026-08-26 after the pass A review, which measured that the first
   round accepted the write and made the tiers disagree: the dev
   tier stopped with "internal lowering error: unbound local
   `counter`" and the ship tier ran. `node` refuses the same program
   with a temporal-dead-zone `ReferenceError`; stock `tsc` accepts
   it, so the entry carries a `tsc-clean-standalone` header.)*
5. *(Added 2026-08-26 after the second pass A review.)* One reject
   entry for an ambient-namespace name shadowed by a later
   declaration, and one for a class name, each with the `tsc` codes
   in its header. One accept entry pins rule 1a: a `using` in a case
   that falls through into a second `using`, printing `case0 /
   case1 / dispose:b / dispose:a / end` byte-exact on both tiers.
6. *(Added 2026-08-26 after the third pass A review.)* One reject
   entry for rule 4b: a local that owns a class name, declared
   before the `new`.
7. Counts: rejects 142 → 151; accept `.ts` 145 → 147; `.expected`
   146 → 148; accept source files 147 → 149.

Pass B:

5. `corpus/accept/a149-suspension-state.ts` + `.expected`: two
   `await` expressions in one expression, with prints that pin the
   evaluation order; a capturing lambda created before a suspension
   and called after it, including one that captures a managed value;
   `await` inside a `for...of` body and `yield` inside a `for...of`
   body; and two generators of one yield type, resumed in turn and
   also passed to a function. Byte-exact across dev JIT, ship C-AOT,
   and the golden.
6. Unit tests: the frame layout of a function with a value live
   across a suspension holds a slot for it; the ship tier emits no
   search over generators; a generator handle carries its resume
   address.
7. Counts, restated 2026-08-26 because pass A moved the base twice:
   accept `.ts` 147 → 148; `.expected` 148 → 149; accept source
   files 149 → 150; rejects unmoved at 151.
9. *(Added 2026-08-26 after the fourth pass B review.)*
   `a149-suspension-state` grows to pin every shape the review
   measured: the three rule 1h shapes, the rule 1i declared-type
   shapes on a parameter and on a `FixedArray` element, the rule 1j
   async-method receiver, and the rule 2a nested `await`. The
   counts of item 7 do not move, because the entry already exists.
   One interop test pins rule 7: a foreign call whose later argument
   grows the array of an earlier one, printing `f2=2`. The record
   quotes the dev-tier and the ship-tier output of each shape.
10. *(Added 2026-08-26 after the fifth pass B review.)*
   `a149-suspension-state` grows again: the four rule 1k shapes, the
   rule 1l unreachable statement after a `return` and after a
   `break`, and the two rule 7a foreign twins as an interop test
   pair. The counts of item 7 do not move. Rule 1k removes code, so
   the record states the frame size of a coroutine that holds one
   capturing lambda, before and after, and states that no golden
   moved.

Both passes:

8. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; clippy
   library counts at the 7 / 22 / 29 baseline. Every pre-existing
   golden and `.expected` byte-identical, the new entries excepted.
   The record quotes the test count and the wall time.
