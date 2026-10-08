<!-- §115 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 115. Recoverable exceptions, and `JSON.parse` returns `T`

*(Added 2026-09-26.)* Origin: a downstream proposal (`HANDOFF.md`,
2026-09-26) to replace `JsonResult<T>` with a direct result and a
catchable failure. The owner accepted the reversal of collision C6
(Q9) on 2026-09-26, with three decisions: a hook that throws while an
exception propagates traps; an exception that leaves an async body, a
generator body, or a Worker entry traps; a `try` block that holds a
suspension is rejected.

Problem, measured at the pin `cb1d469` with `tsc` 5.9.2 and the
repository `tsconfig.json`:

1. `throw`, `try`/`catch`, and a `JSON.parse` that returns `T` are
   `tsc`-clean. This language rejects the first two with S010 and
   wraps the third in `JsonResult<T>`. That is a divergence from the
   system the language is shaped after.
2. Each parse allocates one `JsonResult<T>`, and the caller must
   release it with `Context.free` (stdlib §13.4 states the cost).
3. A read of `.value` on a failed result is a trap
   (`TrapKind::JsonResultValue`). A program error of that class stops
   the Context.

Measured facts that fix the surface:

- Under `strict`, `catch (e)` types `e` as `unknown`. `const n: i32 = e`
  fails with TS2322.
- `catch (e: Error)` fails with TS1196. Only `unknown` and `any` are
  legal annotations.
- `e instanceof SyntaxError`, `e.message`, `e.name`, `throw e`,
  `catch { }`, and `try { } finally { }` are `tsc`-clean. `Error`,
  `SyntaxError`, and `TypeError` come from `lib.es5.d.ts`.
- `throw 42` and `class MyErr extends Error {}` are `tsc`-clean.
- With `parse<T>(text: string): T` in the prelude,
  `const c: Config = JSON.parse(text)` infers `T = Config`.
  `JSON.parse(text)` with no context gives `unknown`.

Code facts at the pin: a trap is a nonzero word at `Context` offset 0
plus a `TrapRecord`. After each call that can fault, each engine reads
the word and leaves the function through one unwind exit (§18, §20).
LIR carries no unwind edge. The `using` disposal is a HIR-to-HIR
rewrite in the checker that copies the hook calls onto each syntactic
exit (§60). Runtime loops that call a script callback stop when the
word is nonzero.

### 115.1 The Error classes

1. The **Error family** is seven ambient reference classes: `Error`,
   `SyntaxError`, `TypeError`, `RangeError`, `ReferenceError`,
   `EvalError`, and `URIError` (the last four since 2026-09-27, stdlib
   §19.1). `tsc` reads them from `lib.es5.d.ts`; the prelude declares
   nothing for them.
2. Each class has two fields, `name: string` and `message: string`,
   and a constructor `(message: string)`. `new Error()` is accepted
   and sets `message` to `""`. The constructor sets `name` to the class
   name. A call without `new` fails with S100. Any other member
   (`stack`, `cause`) fails with S018.
3. The classes of the family share one layout. A hidden `u32` kind tag
   precedes the two fields. The tag selects the class for
   `instanceof`. The `name` field does not, because a program can
   assign it. The names denote one static type, so an assignment
   between them is accepted, as `tsc` accepts it for classes of one
   shape.
4. An Error is an ordinary Context-allocated reference object. The
   rules of every reference class apply: Context scope, `Context.free`,
   and collection only when it is invoked.
5. The language has no inheritance. `class E extends Error` fails as
   every `extends` on a class fails today.
6. The Error classes are not JSON types. `JSON.stringify` of an Error,
   or of a value that holds an Error field, fails with S014, as a `Map`
   does (stdlib §13.5). `JSON.parse` with an Error target fails with
   S014. *(Added 2026-09-26: round 1 measured that `stringify` printed
   the hidden tag, where `node` prints `{}`.)*

### 115.2 `throw`

1. `throw expr` requires the static type of an Error-family class
   (§115.1 rule 1). Any other operand fails with S010 (`throw 42` and
   `throw "x"` are `tsc`-clean).
2. `throw` ends the flow of its block, as `return` does.
3. At a `throw`, the runtime records the object and the `pos_id` of
   the `throw` site as the pending exception. A rethrow (`throw e`)
   records the same object and the position of the rethrow. No copy
   is made.
4. The pending exception is a collection root while it is pending.

### 115.3 `try` and `catch`

1. `try { … } catch (e) { … }` and `try { … } catch { … }` are
   accepted. `catch (e: unknown)` is accepted and means the same as
   `catch (e)`. `catch (e: any)` is `tsc`-clean and fails with S010,
   because the language has no `any`. Every other annotation is
   TS1196.
2. `finally` fails with S010 and a message that names `finally`.
   *(Removed 2026-10-08 by §180: `finally` is accepted.)*
3. A `try` block that holds `await` or `yield` fails with S010 and a
   message that names the suspension. The rule reads the `try` block
   only. A `catch` block can hold a suspension.
   *(Retired 2026-09-26 by §116.1 rule 7.)*
4. The catch binding has a checker-internal type, "caught". It has
   two legal uses: the left operand of `instanceof`, and the operand
   of `throw`. Any other use fails with S010. `catch (e) { }` with no
   use is accepted.
5. `e instanceof C` requires `C` to be an Error-family class. It narrows `e` to `C` in the true branch, by the same
   flow rules as a null test (C7). `x instanceof C` on a value whose
   static type is an Error-family class is accepted with the same
   narrowing. Every other `instanceof` fails with S100.
6. The nearest enclosing `try` whose block holds the raise site
   catches the exception. Entry to its `catch` clause clears the
   pending state, binds the object, and continues. Later script
   operations and later host entries run as usual.
7. A handler is lexical. An exception in a callee propagates to the
   caller's handler through direct calls, method calls, recursion,
   indirect calls through a function value, and lambdas. No form of
   call is a special case, and `JSON.parse` is not.
8. `break`, `continue`, and `return` inside a `try` block or a `catch`
   block leave the statement as they leave any block.

### 115.4 Where an exception stops

An exception that reaches one of these boundaries becomes a trap:
the Context records `TrapKind::UncaughtException` with the Error's
`name`, its `message`, and the `pos_id` of the last `throw`.

1. The outermost script frame of a host entry. The host API does not
   change. The Context state after the return is the state after a
   trap today.
2. The body of an `async` function, at its completion or at the exit
   that the exception takes. The awaiting caller observes a trap, not
   an exception. This includes the part of the body that runs at the
   call (§92). *(Replaced 2026-09-26 by §116.1 rules 1–5: the
   exception completes the handle, and an `await` raises it. Only an
   async body with no holder traps.)*
3. The body of a generator.
4. A Worker entry. The trap is the Worker Context's trap, reported as
   a Worker trap is reported today.
5. A script callback that a host C function calls through a callback
   trampoline. No exception crosses a C frame.

A script callback that a runtime loop calls (`forEach`, `map`,
`sort`, and every other built-in that takes a callback) propagates the
exception to the caller of the built-in. The loop stops as it stops
on a trap today, and the built-in call is a raise site of its caller.

### 115.5 `using` on an exception exit

1. An exception exit of a scope runs each applicable dispose hook
   once, in reverse declaration order, innermost scope first. Then
   the exception continues to the next handler. This is the order of
   §60.1 rule 4 for the other exits.
2. If a hook raises while another exception is pending, the Context
   traps with `TrapKind::DisposeRaisedDuringExit`. JavaScript builds a
   `SuppressedError`; this language records the divergence in C11.
3. If a hook raises on a normal exit (the end of a scope, `return`,
   `break`, `continue`), the exit becomes an exception exit. The
   remaining hooks of that exit run under rule 1 and rule 2.
4. A trap runs no dispose hook. C11 and §60 do not change for a trap.
5. **A `using` scope is one HIR node, and the shared lowering places
   every hook.** The checker emits `Stmt::Using` for the statements
   that follow a `using` declaration up to the end of its block. The
   node carries its bindings in declaration order and its body. A
   binding carries its type, from which the nullable guard of §97
   follows. A `switch` whose arms declare `using` bindings is one node
   whose body is the `switch`; each of its bindings carries its
   storage and its active flag (§97.1 rule 7). Each hook binds to the
   binding that the node resolved, never to a name looked up at the
   exit. The checker copies no hook call onto
   any exit. The HIR-to-LIR lowering, which the three engines share,
   emits the hooks on each edge that leaves the node: the fall-through
   end, `return` (after the value evaluates into a local), `break`,
   `continue`, and the exception edge. It emits them on no other edge
   (§101). The engines receive ordinary control flow.
6. **A hook runs outside every handler of the node's body.** A raise
   in a hook on a normal exit resolves to the handlers that enclose
   the node, never to a `try` inside its body. This is the rule that
   §115.3 rule 6 gives for a `return` inside a `try` block.
7. **The exception edge parks and resumes.** The handler of the node
   parks the pending exception, runs the hooks, and resumes the same
   exception with its object, its report text, and the position of
   its last `throw`. A catch plus `throw e` is not the form: it moves
   the position to the binding (§115.2 rule 3). A raise inside the
   hooks of a parked exit is trap 30.
8. The park stack is a collection root. A trap empties it.

*(Amended 2026-09-26 after the Phase Review. The first form copied
the hooks onto each syntactic exit in the checker and rebuilt the
exception exit from statement shapes in a later pass. Measured: a
hook copied onto a `return` inside a `try` block became a raise site
of that `try`, so a raising hook ran twice and the inner `catch`
caught it — `d`, `caught inside`, `after`, `d` — where `node` prints
`d`, `caught outside`. The same defect class had reached round 3 as a
position defect, so the form changes, not the site.)*

### 115.6 The form

1. **One word, two states.** The word at `Context` offset 0 is `0` for
   none, `1` for a trap, and `2` for a pending exception. Every
   existing test of "nonzero" stays correct for "stop". A trap
   overwrites a pending exception with `1` and drops the object.
2. **LIR carries the handler edge.** A raise site names its handler
   block, or the propagate exit when no handler encloses it. The
   verifier requires that each named handler exists and starts with a
   catch entry. A site with no edge is a verifier error, so an engine
   cannot drop one (§20.1).
3. **"Can raise" is a callee fact, derived one time in HIR.** A
   function can raise if its body holds a `throw`, a `JSON.parse`, a
   built-in call that calls a script callback (§118.2: not a built-in
   call whose operand only has a function type), an indirect call, or a
   call to a function that can raise. Each engine reads the fact. None
   derives it again. The C emitter checks the word after each call to
   a callee that can raise.
4. **The success path reads what it reads today.** The check after a
   call that can raise is the existing one-word load and compare. The
   test for `2` and the jump to a handler run only when the word is
   nonzero.
5. **The interpreter** propagates an exception as a distinct error
   value, not as a trap error. It converts at the same boundaries as
   §115.4.
6. **Positions.** A raise site carries its `pos_id`, as a trap site
   does (§20.2). The uncaught report cites the last `throw`.

### 115.7 `JSON.parse<T>`

1. `JSON.parse<T>(text: string): T`. The prelude declaration changes
   to that signature. `T` comes from the explicit type argument or
   from the contextual type. A call with neither fails with S014, as
   it does today.
2. The accepted targets and the validation rules of stdlib §13.3 and
   §13.4 do not change. Only the failure channel changes.
3. Malformed text raises `SyntaxError`. Its message is
   `JSON.parse: invalid syntax at byte <N>`, where `<N>` is the UTF-8
   byte offset of the first byte the parser cannot accept (Q5 indexes
   strings by byte).
4. Input deeper than `MAX_JSON_DEPTH = 128` raises `SyntaxError` with
   the message `JSON.parse: nesting deeper than 128 at byte <N>`.
5. Well-formed text that does not match `T` raises `TypeError`. Its
   message is `JSON.parse: document does not match <T>`, where `<T>`
   is the target type as the source spells it. This covers a missing
   field, a wrong kind, an integer out of range, a non-integer for an
   integer target, a finite `f64` that overflows `f32`, and a lone
   surrogate for a `string` target.
6. The messages are this project's. They do not copy any JavaScript
   engine, and an entry that prints one is not `js-comparable` (C6).
7. The parse document is released before the raise. No partial value
   is visible. Allocation failure and an internal fault stay traps.
8. `JsonResult<T>` is removed. `TrapKind::JsonResultValue` (18) and its
   trap site retire; the number 18 stays unassigned, as 25–27 do.

### 115.8 Numbers

- `TrapKind::UncaughtException = 29`.
- `TrapKind::DisposeRaisedDuringExit = 30`.
- S010 keeps its code. Its text becomes "An exception form outside the
  decided exception surface is rejected." Each rejection carries a
  message that names its form (§103).

### 115.9 Sections this one amends

- Collision C6 is revised: exceptions are in, with the surface of
  §115.1–§115.5. Traps stay uncatchable.
- Collision C11: the item "JS runs disposal during throw-unwind" is
  revised to §115.5. A trap still runs no hook.
- Collision C8 gains §115.3 rule 3 and §115.4 items 2 and 3.
- Stdlib §13.4 is superseded by §115.7 for the failure channel. Its
  target rules, its depth limit, and its numeric rules stay.
- §20.3: the `JsonResult.value` site retires.
- §60.1 rule 8 and §97.1 rule 9 ("the rewrite is checker-complete;
  no new HIR node, no codegen change") retire. A `using` scope is the
  HIR node of §115.5 rule 5, and the shared HIR-to-LIR lowering
  places the hooks. The order rules of §60.1 rule 4, the guard of
  §97, and §101 hold unchanged.
- `corpus.md`: the Q9 row reads "`throw` / `try` / `catch` (§115)".
- `examples.md` §6: `e05-no-exceptions` becomes `e05-errors`. Its
  golden moves with its source.

### 115.10 Corpus (pre-registered)

The collision table cites an entry only when the entry exists
(`compiler/tests/js_corpus.rs`), so C6 and C8 gain the new ids in the
implementation commit, not in this one.

Accept, each with a hand-checked golden:

- `a250-throw-catch`: `throw new Error`, `catch (e)`, `catch { }`,
  `catch (e: unknown)`, the three `instanceof` tests, `message` and
  `name`, and the execution after the handler.
- `a251-exception-propagation`: a raise in a helper, in a nested call,
  in a recursion, through an indirect call, through a lambda, and
  through a `forEach` callback. The caller catches each.
- `a252-nested-handlers-rethrow`: nested handlers, a rethrow to the
  outer handler, identity of the rethrown object (`===`), and `break`,
  `continue`, and `return` inside `try` and `catch`.
- `a253-using-exception-exit`: hooks run once, in reverse order, on an
  exception exit through two scopes. The normal exits keep their
  order.
- `a254-using-hook-raises`: a hook raises on a normal exit. The
  remaining hooks run, and an outer handler catches the exception.
- `a255-json-parse-direct`: a successful parse of each target family,
  `SyntaxError` with its offset, the depth limit, `TypeError` on each
  mismatch class of §115.7 rule 5, and a later successful parse.
- `a256-try-in-async`: a `try` with no suspension inside an async
  function, and a `catch` block that awaits.
- `a257-using-exit-inside-try`: inside a `using` scope, a `return`, a
  `break`, and a `continue` in a `try` block whose `catch` must not
  catch a raising hook; each hook runs once, and the outer handler
  catches. Its golden matches `node`.
- `a70`, `a71`, `a72`, and `t33` migrate to the direct result.

Reject:

- `r11-throw` stays. Its operand is a string, so its message changes
  to the non-Error rule of §115.2 rule 1.
- `r233-finally`, `r234-try-holds-await`, `r235-try-holds-yield`,
  `r236-caught-binding-use`, `r237-catch-annotation` (`catch (e: any)`),
  `r238-instanceof-non-error`, `r239-stringify-error`. Each is
  `tsc`-clean, and each header states it.

Trap, with the trap tuple identical on each engine:

- `t61-uncaught-exception`: kind 29, the message, the `throw` position.
- `t62-hook-raises-during-exit`: kind 30.
- `t63-exception-leaves-async`: kind 29 at the async body.
- `t64-exception-leaves-generator`: kind 29 at the generator body.
- `t65-exception-leaves-host-callback`: kind 29 at the trampoline,
  with the host fixture of §111 (`interpreter: no`).
- `t01-json-result-value` retires. `t02`–`t05` keep their faults and
  change only a `JsonResult` spelling, if they hold one.

Tests outside the corpus: a Worker test for §115.4 item 4; a runtime
test that 10,000 caught parse failures leave the JSON parser and
builder tables empty; a verifier test that builds a raise site with no
edge.

### 115.11 Goldens that move

`a70`, `a71`, `a72`, `t33`, and the example `e05` change their source. Their stdout
stays the same where the program prints the same values. The LIR text
snapshot moves: raise sites gain handler edges. `t01` retires. Any
other moved golden is a stop.

### 115.12 Exit criteria

1. Every entry of §115.10 fails at `cb1d469` (S010, S016, or S100), and
   green after the landing on the dev JIT, the ship C, the
   interpreter, and the golden, byte-exact.
2. `tsc` exits 0 over the accept entries with the new prelude.
3. Outside `specs/` and the retired entries, `git grep JsonResult`
   answers nothing.
4. Success-path cost, measured before and after on this host with the
   cross-language runner: `fib-recursive`, `callbacks`, and `tree` on
   both tiers. A median more than 5% slower on the ship tier kills
   the landing; the round stops and reports. A loop of 200,000
   successful `JSON.parse<Config>` calls on the ship tier is measured
   at the pin (with `JsonResult`) and after, and both numbers are
   recorded.
5. `tools/gate.sh full` is green.
6. The Phase Review has no open CRITICAL or MAJOR, and
   `tools/hygiene.sh` is clean.
