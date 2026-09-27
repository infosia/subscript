<!-- §116 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 116. An `await` delivers the exception of its handle

*(Added 2026-09-26.)* Origin: the §115 landing. The owner decided on
2026-09-26 that an exception that leaves an async body completes its
handle, and that an `await` of the handle raises it.

Problem: under §115.4 item 2, an exception that leaves an async body
traps. The form that TypeScript programs use most,
`try { await f(); } catch (e) { … }`, cannot catch a failure of `f`.
§115.3 rule 3 rejects a `try` block that holds `await`, so the program
cannot be written, but a program that needs the recovery has no form.
§115 round 3 measured that a handler region across a suspension is
correct on each engine; the rejection was a scope choice, not a
mechanism limit.

Measured on `node` v24.18.0 (exit 0 unless stated):

1. `const h = fails(); print("after call"); await h` where `fails`
   throws before its first `await`: `body start`, `after call`, then
   the `catch` of the `await` runs. The exception does not reach the
   caller at the call.
2. A throw after an `await` in the callee reaches the `catch` around
   `await later()`.
3. Two `await`s of one handle each raise, and the two caught values
   are the same object (`===` is `true`).
4. A rejection that no `await` observes ends the process with exit 1
   and the error text.
5. `try { yield i; … throw … } catch` in a generator catches the
   throw after the resume.

Measured on this language at `154cc3d`: two `await`s of one handle
return the same value (`42 42`); §70.3 rule 4 says an `await` consumes
the completion, not the ownership.

### 116.1 The rules

1. **An exception that leaves an async body completes its handle.**
   The handle holds the Error object, its report text, and the
   position of its last `throw`. The body does not trap. This holds
   for the part of the body that runs at the call (§92): the call
   returns the handle, and the exception does not reach the caller at
   the call.
2. **An `await` of a handle that completed with an exception raises
   it.** The raise has the same object, the same report text, and the
   same position. An `await` is a raise site of its function.
3. **Each `await` of the handle raises the same object.** No copy is
   made.
4. **A handle that holds an unobserved exception traps when its count
   reaches zero.** "Unobserved" means that no `await` raised it. The
   trap is `TrapKind::UncaughtException` (29), with the report text and
   the position of the last `throw`. This is the counterpart of item 4
   of the measurements. A handle that a leak keeps alive never reaches
   zero, and its exception is not reported (§70.3 rule 6, invariant 2).
   A handle release can therefore trap, so every release carries a
   word check on each engine.
4a. **A waiting `await` holds one count on the handle it awaits,**
   from its registration until its resume reads the completion. An
   `await` that is queued on a handle will observe it, so the release
   of the last script holder does not bring the count to zero, and
   rule 4 does not fire. §94.2 gives a registration a count on its
   waiter; this rule adds the awaited handle. A direct `await` of a call
   takes no extra count: the count that the call created moves to the
   registration, and the completion read releases it once. *(Added 2026-09-26 by the
   Phase Review. Measured on the dev tier: a handle held by a field,
   awaited in a helper inside a `try`, with the holder freed before
   the resume, trapped 29 as unobserved; `node` printed `caught late`.
   The value form trapped `internal: async resume without completion`,
   a §94 defect that this rule also closes.)*
4b. **An exception exit runs every scope-exit action that a normal
   exit runs.** A scope that owns async handles releases them on its
   exception edge, as a `return` releases them (§70.3 rule 2). The
   exception edge parks the pending exception, runs the dispose hooks
   (§115.5) and the releases in the order a normal exit runs them,
   and resumes the exception. One lowering function places the
   actions of every exit edge, so no exit kind can miss one.
   If a release on that edge brings a handle that holds an unobserved
   exception to zero, the Context traps with 29 for that handle at
   once, and the parked exception is dropped. `node` lets a `catch`
   take the propagating exception first and ends the process after;
   this language does not keep a deferred report. C8 records the
   divergence. *(Added 2026-09-26 by the Phase Review. Measured: a
   handle to a failed call, held in a scope that a `throw` left,
   was never released on each engine, so its exception was never
   reported; the program printed `caught TypeError skipped`, `end`,
   and exited 0, where `node` printed the same two lines, then
   `Error: dropped`, and exited 1.)*
4c. **A lambda borrows a captured handle.** C5 states that a lambda
   does not escape its defining function and captures only `const`
   locals, so the defining function's binding outlives every call of
   the lambda. *(§118 enforces C5 through a callee that stores or returns its
   function-typed parameter.)* A
   capture is not an owner: the closure takes no count, and no exit of
   the lambda, normal or exceptional, releases it. A copy inside the
   lambda (`const local = h`) acquires and releases as any copy does
   (§70.3 rule 2). *(Added 2026-09-27 by the Phase Review. Measured on
   both tiers: two calls of a lambda that copied a captured handle
   released it twice, and a later `await` of the handle in the
   defining function trapped `internal: async resume without
   completion` on the dev tier and died with signal 11 on the ship
   tier; expected `got 7`.)*
5. **An async body with no handle holder traps.** A host-callable
   async export has no script holder: an exception that leaves it
   becomes trap 29 at its completion, as under §115.4 item 2.
6. **The Error object stays an ordinary Context allocation.** The
   handle holds it as a collection root. The release of the handle
   does not free it, because a catch binding can hold it after the
   handle is gone.
7. **A `try` block can hold `await` and `yield`.** §115.3 rule 3
   retires. A generator body keeps §115.4 item 3: an exception that
   leaves it traps. This language has no `gen.throw()`, so no
   exception enters a generator at a `yield`.
8. **Workers and host callbacks do not change** (§115.4 items 4
   and 5).

### 116.2 The form

1. The handle's completion has two states: a value, or an exception.
   The runtime stores the exception in the frame's completion, not in
   a new header field. The exception payload (object, report text,
   position, observed flag) is stored out of line, so a value
   completion keeps the size and the work it had before this section. The host ABI does not change: the host sees
   only `subscript_rt_ctx_async_pending`, `async_step`, and
   `async_unfinished`.
2. The unwind exit of an async resume function completes the handle
   with the pending exception instead of settling it into trap 29.
   The settle stays for a frame with no holder (rule 5) and for a
   generator.
3. An `await` resumes the awaiting frame and then reads the
   completion. For an exception, it makes the exception pending and
   takes the raise edge of the `await` site. The success path keeps
   the flag check it has today.
4. The "can raise" fact (§115.6 rule 3) counts an `await` as a raise
   site. An async function can raise. A call to an async function is
   not a raise site; an `await` is.
5. The interpreter carries the same completion states. An exception
   completion that reaches a resume whose successor does not start
   with the raise of the `await` is `TrapKind::Internal`, as §94.1
   treats every other protocol defect.
6. **The count's fast path is in the generated code.** Where the
   static type is an async handle, the dev JIT and the ship C retain
   inline: a null test, then a saturating increment of the `u32` at
   frame offset 4 (§70.2). A release decrements inline when the count
   is above one, and calls the runtime otherwise; the runtime keeps
   the free, the rule 4 trap, and every check at one and at zero. A
   generator frame, whose offset 4 is its reload epoch, never takes
   the inline path. *(Added 2026-09-27. Measured: after the two forms
   above, `held-handles` stayed 12.0% slower than `154cc3d`, because
   rule 4a adds a runtime retain and release per `await` of a held
   handle, and each call searches the Context's frame table before it
   touches the count.)*

### 116.3 Sections this one amends

- §115.3 rule 3 retires: a `try` block can hold `await` and `yield`.
- §115.4 item 2 is replaced by §116.1 rules 1–5.
- Collision C6 and C8: an exception that leaves an async body is
  delivered at the `await`; a `try` block can hold a suspension.
- §70.3 rule 4 gains the exception completion.
- §70.3 rule 2: a lambda's capture of a handle is a borrow, not a copy
  (§116.1 rule 4c).

### 116.4 Corpus (pre-registered)

- `a258-await-delivers-exception`: a throw before the first `await`
  (the part run at the call), a throw after an `await`, a three-level
  async chain caught at the top, two `await`s of one handle with `===`
  on the caught objects, a `try` that spans several `await`s, and
  execution after the handler with later `await`s. `js-comparable`
  against `node`.
- `a259-try-holds-yield`: a generator whose `try` spans a `yield` and
  catches a throw after the resume. `js-comparable` against `node`.
- `a260-awaited-handle-outlives-its-holder`: a handle held by a field,
  awaited in a helper inside a `try`, with the holder freed before the
  resume; the exception form catches it, and the value form prints the
  value. `js-comparable: no Q6` (`Context.free` has no JavaScript
  shim). The Phase Review ran a plain-JS translation under `node`:
  `freed holder`, `later:resumed`, `caught late`, `end`.
- `a261-lambda-borrows-a-captured-handle`: a lambda that copies a
  captured handle, called twice, then an `await` of the handle in the
  defining function; and the same lambda with a `throw` caught in the
  defining function, with a failed captured handle that the defining
  function awaits after. `js-comparable: yes` if `node` agrees.
- `t67-exception-exit-releases-its-handle`: a handle to a failed call,
  held in a scope that a `throw` leaves. Trap 29 for the handle's
  exception at the exception exit. `js-comparable: no C8`.
- `t66-unobserved-async-exception`: a handle held only by a field of an
  object that `Context.free` releases, after its body threw. Trap 29 at
  the release, with the position of the `throw`.
- `t63-exception-leaves-async` is rewritten to rule 5: the only holder
  is the host. Its trap stays kind 29.
- `r234-try-holds-await` and `r235-try-holds-yield` retire.
- A three-engine test for each rule of §116.1, and a runtime test that
  an `await` of a handle completed with an exception leaves no pending
  frame in the continuation queue (§94).

### 116.5 Goldens that move

`t63` changes its source. The LIR text snapshot moves: `await` sites
gain raise edges, and async functions gain `can-raise`. The golden of
every existing async entry keeps its stdout; a change there is a stop.

### 116.6 Exit criteria

1. Every new entry of §116.4 fails at `154cc3d` (S010 for a `try`
   that holds a suspension, or a trap at the wrong site). The
   rewritten `t63` is a control: rule 5 keeps its behaviour, so it
   gives the same trap before and after. Each entry passes
   after the landing on the dev JIT, the ship C, the interpreter, and
   the golden, byte-exact.
2. `tsc` exits 0 over the accept entries.
3. Success path, measured on this host against the §115 landing:
   `fib-recursive`, `callbacks`, and `tree` on both tiers, and the
   async-cost workloads (`benchmarks/src/bin/async-cost.rs`, ship tier
   only). A ship-tier median more than 5% slower kills the landing. The
   ship `tree` median is bimodal on one binary (101.6 ms and 132.7 ms
   in two consecutive runs at `154cc3d`), so it is compared within
   its mode, and it runs until both modes appear or four runs pass.
   *(Owner decision 2026-09-27: `held-handles` is accepted at +8.4%
   over `154cc3d`, measured after §116.2 rule 6. The workload awaits
   40,000 handles that an array holds; rule 4a gives each such `await`
   a count, which is the cost of a safe `await` while its holder can
   be released. Every other workload stays within 5%.)*
4. `tools/gate.sh full` is green.
5. A fresh Phase Review has no open CRITICAL or MAJOR, and
   `tools/hygiene.sh` is clean.
