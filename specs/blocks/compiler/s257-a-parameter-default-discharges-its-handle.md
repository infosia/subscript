<!-- §257 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 257. A parameter default discharges its handle

*(Added 2026-11-12.)* Origin: §256.4 item 2. Under the owner's standing
approval of 2026-11-06, selected by the owner on 2026-11-12.

Problem, measured at `ce24838e` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `v(): Promise<i32>` is async. `tsc` accepts
each form, with no error. The checker gives S013 "an async handle is
dropped without any await of its completion" at the default.

| Form | Called as |
|---|---|
| `one(p: Promise<i32> = v()): Promise<i32> { return await p; }` | `await one()` |
| `one(p: Promise<i32> = v(), k: i32 = 2)`, `p` awaited | `await one()` |
| `one(p: Promise<void> = u()): Promise<void> { await p; }`, `u(): Promise<void>` async | `await one()` |
| the method `C.m(p: Promise<i32> = v())`, `p` awaited | `await c.m()` |
| `all(xs: Promise<i32>[] = [v(), v()])`, each element awaited | `await all()` |

A default is checked as an expression of its own, so its call of `v()`
is an origin (§188 rule 2) that nothing discharges (§188 rule 3). The
value of the default becomes the parameter. The parameter is an origin
of its own, so the body must discharge it: with an explicit argument,
`one(p: Promise<i32>): Promise<i32> { return 1; }` gives S013 at `p`.

A lambda default is checked with the same rule, and no call runs it: a
call through a function value passes every parameter (S100), so
`await f()` is rejected for `const f = async (p: Promise<i32> = v())`.
The accept entry has no lambda form (§257.4 item 1).

### 257.1 Rules

1. The default of a parameter whose type holds a handle discharges each
   origin of its value, as an argument of that parameter type does
   (§188 rule 3).
2. The parameter stays an origin (§188 rule 2). If the body does not
   discharge it, S013 is at the parameter, as with an explicit
   argument.
3. No program that `tsc` rejects changes from rejected to accepted.
   The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 257.2 Acceptance

1. Red first, at the pin: the accept entry
   `a441-a-parameter-default-discharges-its-handle` with each form of
   the table is rejected with S013. The tracking note records the
   output.
2. After the change, `a441` passes `tsc`, the three tiers, and `node`.
3. A unit test pins rules 1 and 2: each form of the table is accepted,
   and `one(p: Promise<i32> = v()): Promise<i32> { return 1; }` gives
   S013 at `p`, not at `v()`. A control: a call of `v()` as a statement
   in the body keeps its S013.
4. `main` of `a441` is async, so the LIR text snapshot grows by the
   `a441` block. No other golden moves. If another corpus entry or
   golden changes, report it and stop.

### 257.3 Phase Review

One fresh review of the implementation. No CRITICAL finding, and no
MAJOR finding. The discharge uses the predicate and the origin set of
an argument (§188 rule 3). The review measured rule 2 at a function, a
method, a static method, a constructor, a lambda, and a generator.

1. MINOR, contrived, recorded: §257.4 item 2.
2. MINOR, realistic, fixed: no test reached the discharge of a
   constructor default. The unit test now has a constructor form.
3. MINOR, fixed: the text of the tracking note on an unannotated
   default.

### 257.4 Open

1. A call through a function value passes every parameter, so a lambda
   default never runs. `const f = async (p: Promise<i32> = v()) => …;`
   called as `await f()` gives S100 "`f` expects 1 argument(s), got 0:
   a call through a function value passes every parameter". `tsc`
   accepts it. An immediately called lambda gives the same S100.
2. A default that reads an earlier parameter discharges it, as an
   argument does.
   `async function f(p: Promise<i32>, q: Promise<i32> = p)` with
   `return await q;` is accepted, and with `q: Promise<i32>` and
   no default it gives S013 at `p`. If a call passes `q` and `p`
   rejects, nothing awaits `p`, and the program traps
   (`uncaught-exception`). The check of §188 does not follow control
   flow, and it accepts the same drop through
   `const r = c ? p : q; await r;`.
