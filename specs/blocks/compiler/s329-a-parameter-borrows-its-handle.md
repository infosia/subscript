<!-- §329 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 329. A parameter borrows its handle

*(Added 2026-11-18.)* Origin: §188.3 items 1, 5, and 6, rank 8 of
`specs/tracking/backlog.md`. A `tsc` divergence with a fix in sight
(standing approval of 2026-11-06). On 2026-11-18 the owner selected
the form of rule 1: a parameter borrows its value, with no summary of
the callee.

Problem, measured at `1ef45e1f`. `tsc` accepts each program below, and
each program awaits every handle. subscript rejects each one with S013
(§188 rule 4):

| Form | S013 at |
|---|---|
| `function count(xs: Promise<i32>[]): i32 { return xs.length; }`; the caller passes `xs`, then awaits `Promise.all(xs)` | the parameter `xs` |
| the same with a constructor parameter that reads only `jobs.length` | the parameter `jobs` |
| `function* gen(): Generator<Promise<i32>> { yield work(1); yield work(2); }`; the consumer awaits each value in a `for…of` | each `yield` operand |
| `const ps = (await many()).slice(0); await Promise.all(ps);` | the `await many()` |

A parameter is an origin (§188 rule 2), so the callee must discharge
it. But the caller still holds the value: it can await it after the
call. A `yield` gives the value to the consumer, as a `return` gives it
to the caller, but it is not a discharge. A built-in method result does
not carry the origins of its receiver, so the receiver is dropped.

The control `async function waitAll(xs: Promise<i32>[]) { await
Promise.all(xs); }` with `await waitAll([work(1), work(2)])` is
accepted at the pin.

### 329.1 Rules

1. **A parameter borrows its value.** A parameter of a function, a
   method, a constructor, or a lambda is not an origin. The caller
   holds the value. An argument discharges the caller's origins, as
   §188 rule 3 states. A handle that the caller then drops is a
   runtime case of §188 rule 5.
2. **A lambda written as a `then`, `catch`, or `finally` callback
   keeps its parameter as an origin.** The runtime gives the callback
   its value, and no caller holds it (§188 rule 2, `r416`). A function
   value at that place (`then(count)`, a parameter `f`) is a borrow by
   rule 1: a function type carries no fact about what the callee
   discharges. A handle that it drops is a runtime case of §188 rule 5.
   *(Amended 2026-11-18: round 1 stopped because the first text
   applied to each callback, and a function value carries no discharge
   fact.)*
3. **A default makes its own origins.** If a parameter has a default,
   each origin that the default makes (a call, an `await` result) is an
   origin of the body. The parameter carries it, so a discharge through
   the parameter discharges it (§257 rule 1). A default that reads an
   earlier parameter makes no origin (rule 1).
4. **A `yield` discharges.** The operand of a `yield` whose type holds
   a handle is discharged, as the operand of a `return` is (§188
   rule 3).
5. **A built-in method result carries its receiver.** A call of a
   built-in method on a receiver that carries origins gives a result
   that carries the same origins if the result type holds a handle
   (`slice`, `concat`). If the result type holds no handle,
   the receiver is not discharged. A discharge through the result
   discharges each origin, as for any value (§188 rule 3). A built-in
   method result is still not an origin of its own (§188.3 item 3).

### 329.2 Acceptance

1. Red first: an accept entry with each form of the problem table,
   the control, and the programs of `r417`, `r533`, and `r535` with a
   caller that awaits each handle. It also covers a method parameter,
   a lambda parameter called through a function value, and a default
   that makes a handle the body awaits. Each case prints a value. The
   entry is `js-comparable: yes` if `node` prints the same bytes. At
   the pin, `check` gives S013.
2. Reject entries: a `slice` result that holds handles and is dropped;
   a default that makes a handle that the body does not await; each is
   S013 with its position. Each header states what `tsc` does,
   measured. `r414`, `r416`, and `r534` keep their output.
3. `r417`, `r533`, and `r535` are deleted. Collision C8 writes each as
   `retired:<name>`, and its form is in the entry of acceptance 1.
4. Unit tests in `compiler/tests/async_handle_origins.rs`: rules 1 to
   5, each with a control in the same shape that stays S013 (core
   principle 9). Rule 2 has a test that `many().then(count)` and a
   function parameter `f` at `then(f)` are accepted, and the control
   that a lambda at `then` that reads only `length` stays S013.
5. The development tier and the ship tier print the same output. The
   CLI `check` output of each other corpus entry is the same at the pin
   and after.

### 329.3 Sections this one amends

- §188 rule 2: a function, method, constructor, or lambda parameter
  is not an origin, except the parameter of a lambda written as a
  `then`, `catch`, or `finally` callback. §188.3 items 1, 5, and 6
  close.
- §188 rule 3: a `yield` discharges, and a built-in method result
  carries its receiver's origins.
- §257 rule 2: the parameter is not an origin. A default that makes a
  handle makes an origin of the body.
- §317 rule 1: a parameter is not a selected origin. `r533` and `r535`
  are accepted.

### 329.4 Phase Review

Form. A parameter is not registered as an origin, except the parameter
of a lambda written at a `then`, `catch`, or `finally` argument. A
default registers the origins that it makes, and the parameter carries
them into the body. A `yield` discharges as a `return` does. A
built-in method result whose type holds a handle carries the origins
of its receiver. LIR, codegen, and the runtime do not change.

Measured with a480 (pin: 10 S013), development tier, ship tier, and
`node` the same:

| Form | Pin | After |
|---|---|---|
| A function parameter that is only read; the caller awaits | S013 | `function 2 2` |
| A constructor parameter (`r417`) | S013 | `constructor 2 2` |
| A method parameter | S013 | `method 1 1` |
| A lambda called through a function value | S013 | `lambda 1 1` |
| `yield work(1)` | S013 | `yield 3` |
| `(await many()).slice(0)` | S013 | `slice 2` |
| A default that reads an earlier parameter (`r533`) | S013 | `earlier 9 8 9` |
| A conditional of two parameters (`r535`) | S013 | `conditional 10 10 11` |
| A default that makes a handle that the body awaits | accepted | `default 7` |

`r542` (a dropped `slice` result) and `r543` (a default handle that
the body does not await) give S013; `tsc` accepts each. The CLI
`check` output of 1,077 corpus entries is the same at the pin and
after.

A generic `T extends Promise<i32>` in a boolean condition was rejected
only by the S013 of its parameter. It now gets the S100 of a
non-generic `Promise<i32>` there; `tsc` gives TS2801.

One fresh review found no CRITICAL or MAJOR finding. Its realistic
MINOR finding is fixed: a lambda default that reads a local of the
enclosing function got the §317 "earlier parameter" text. a480 now also
runs the `r533` default.

### 329.5 Open

1. Contrived: `f(p: Promise<i32> = v(), q: Promise<i32> = p)` with
   `await q` is S013, although `q` is omitted whenever `p` is (§317
   rule 2).
2. By rule 1, `start(work(1))` with a parameter that the callee does
   not await is accepted; the handle is a runtime case of §188 rule 5.
   By rule 5, `Promise.all(xs.slice(0, 1))` discharges the whole of
   `xs`, as `xs[0]` does (§188 rule 3).
