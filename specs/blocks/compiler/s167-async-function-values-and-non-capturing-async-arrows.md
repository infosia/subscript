<!-- §167 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 167. Async function values and non-capturing async arrows

*(Added 2026-10-06.)* Origin: the owner's async usability proposals of
2026-10-06, item 5, first delivery. The owner selected it on
2026-10-06. Retained capture environments (item 5, second delivery) are
not in this section.

Problem: an async function is not a value, and an async arrow is
rejected. A program that passes an async operation to another function
must write a named function and a synchronous starter. `tsc` accepts
each form below. Measured at `5366af19` (`subscript check`):

| Form | This language |
|---|---|
| `const f: (x: i32) => Promise<i32> = twice;` (`twice` is async) | S100 `AsyncFunctionValue` |
| `const w = async (v: i32): Promise<i32> => { … };` | S100 `AsyncArrowFunction` (`r140`) |
| `await f(v)` with `f: (v: i32) => Promise<i32>` | S100 `AwaitLocalCall` |

The machinery exists. A synchronous function that returns a handle is a
value today, and a call through that value returns a handle that a
program stores and awaits (measured: `function starter(v: i32):
Promise<i32> { return named(v); }`, `const h: Promise<i32> = s(1);
await h;` prints `101`). A function value is a code and environment pair;
a non-capturing lambda has a null environment.

`node` v24 (`tsc`-emitted CommonJS, `print` bound to `console.log`) runs
an async arrow body to its first `await` at the call, like a named async
function (§92).

### 167.1 Rules

1. A named async function is a value. Its type is the function type of
   its signature, with the result `Promise<T>`. A call through the value
   has the semantics of a direct call: it runs the callee to its first
   `await` or its return (§92), and returns a new handle (§70).
2. An async arrow that captures nothing is accepted. Its value has the
   type `(P…) => Promise<T>`. A call through it follows rule 1. Its body
   follows every rule of an async function body: `await`, `try` across
   `await` (§116.1 rule 7), and exception delivery (§116).
3. The result type of an async arrow comes from its annotation, or from
   the contextual function type. An expression-body async arrow with
   neither takes `Promise<T>`, where `T` is the type of the body. A
   block-body async arrow with neither is rejected, as a block-body
   lambda is (`collisions.md` C24 row 4). An expression body is the
   value of a `return`, and follows the return rule of a named async
   function: a body whose type is a handle is rejected, as `return h`
   with `h: Promise<T>` is in a named async function (no promise
   adoption; measured at `5366af19`: S100 "the return value expects
   `i32`, got `Promise<i32>`"). The checker does not convert that body
   to an `await`. Round 2 measured that the conversion completes one
   turn earlier than `node` adoption in all three tiers.
4. An async arrow result annotation that is not `Promise<T>` is rejected
   with S100. `tsc` rejects it (TS1064). This site is `TscRejects`.
5. An async arrow that captures a local, a parameter, or `this` is
   rejected with S009. The message names the captured binding and
   states that an async arrow captures nothing. A module global, a named
   function, and a class are not captures. The site is `Diverges`
   (`collisions.md` C24 row 36).
6. The operand of `await` can be a call through a function value whose
   result type is `Promise<T>`: a local, a parameter, a global, a field,
   or an array element. The `await` has the semantics of a store of the
   handle in a temporary and an `await` of that temporary. The await
   takes the fresh handle count (§166 transfer fact). An `await` of a
   call whose result is not a handle keeps its current rejection.
7. A call through a function value that returns `Promise<T>` creates a
   handle. Every created handle has an awaited completion (`r100`). A
   dropped result is rejected with S013, as a dropped direct call is.
8. An async function value holds no count. It is a code and environment
   pair with a null environment, so arrays, fields, globals, and
   parameters hold it as they hold a synchronous function value. The
   §118 capture escape rule sees it as clean.
9. An async method read as a value stays rejected (`AsyncMethodValue`,
   C24 row 12): the value captures its receiver. A generic async arrow
   and an async generator arrow stay rejected.
10. Hot reload: a named async function value calls through the function
    table, as a synchronous function value does. An async arrow value
    keeps the body of the module that created it, as a lambda does. A
    frame that a call through either value suspends follows the existing
    stale-frame rule (`StaleCoroutine`).
11. The three tiers (dev JIT, C AOT, interpreter) give the same output
    and the same order as `node`.
12. A host-callable export stays a named function declaration.
13. The HIR lambda carries an async-body fact, through deferred
    initializers and every lambda consumer. The result type does not
    give that fact: a synchronous arrow that returns a handle from
    another call (`(): Promise<i32> => value(7)`) keeps its current form
    and gets no async frame. The lambda carries its callable result
    (`Promise<T>`) and its body result (`T`) as two facts. The callable
    code of an async function value, named or arrow, is a producer: it
    creates the frame, runs it to its first `await` or its return, and
    returns the handle with one owner. An ordinary indirect call then
    returns that handle, and needs no new call kind. Each tier (dev JIT,
    C AOT, interpreter) has this producer. Round 1 measured that the
    HIR lambda has no async fact, and that no tier wraps an async
    function as a callable.

### 167.2 Acceptance

1. Red first. At the contract pin, record each result:
   - Accept entry `a336`, `js-comparable: yes`, golden from `node`. It
     covers: a direct `await` of an async arrow; an async arrow passed
     to an async function that calls it after a suspension; a named
     async function as a value; a handle from a call through a value,
     stored and awaited later; an array, a field, and a global of async
     function values; an exception that leaves an async arrow and that
     a `try` around the `await` catches; an expression-body async arrow
     with no annotation; the order of an async arrow body against
     another root that prints each turn.
   - Reject entries, each with its measured `tsc` header: `r379` (an
     async arrow that captures a local), `r380` (a dropped handle from a
     call through a function value), `r381` (an async arrow result
     annotation that is not `Promise<T>`).
   - `r140` retires: the construct it pins is legal.
2. Unit tests, each with a same-shape control: the handle count after a
   call through a value is zero after success and after failure; an
   async arrow value in a field survives explicit collection; a frame
   suspended through an async arrow value traps `StaleCoroutine` after a
   reload, and a named async function value reaches the new body.
3. The §154 total test passes. The sites `AsyncFunctionValue` and
   `AsyncArrowFunction` leave the table, or name only the forms that stay
   rejected. Each new site has a measured `tsc` witness.
4. The async-cost benchmark (`benchmarks/src/bin/async-cost.rs`) on the
   existing workloads: the median is at most 1.05 times the pin median
   (release, best of three).
5. No existing `.expected` golden moves.

### 167.3 Open

The Phase Review found these. None is CRITICAL or MAJOR.

1. A handle-typed expression body with no annotation gives a second,
   cascading S100: the callable type becomes `Promise<Promise<T>>`
   (`const f = async () => value(7);` then a template of `await f()`).
2. A direct `await s(1)` of a synchronous function that returns a handle
   is rejected ("`s` is synchronous and cannot be awaited"), but
   `const f = s; await f(1)` is accepted. `tsc` accepts both. Rule 6
   names calls through values only.
3. The `AsyncArrowCapture` diagnostic shows the S009 rule line about an
   escape; an async arrow capture is rejected without an escape. The
   `AwaitIndirectCall` why-text says "The call returns an integer"; the
   site covers every non-handle result.
4. `compiler/tests/js_corpus.rs` reads the tracking note as a retirement
   source for `r140`, and its comment states that the C8 citation stays
   historical. C8 now marks `retired:r140-async-lambda`, so the hook is
   redundant.
5. `codegen/src/lower/func/async_callable.rs` indexes the incoming
   parameter slice directly (`incoming[0]`, `incoming[2..]`). The
   signature always has the context and environment parameters, so the
   index cannot fail (contrived).
