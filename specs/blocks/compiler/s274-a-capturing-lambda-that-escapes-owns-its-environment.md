<!-- §274 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 274. A capturing lambda that escapes owns its environment

*(Added 2026-11-13.)* Origin: downstream request R41 of
subscript-typegpu. The owner selected the form on 2026-11-13: option 2
of the request, a lambda that leaves its function owns its environment.

Problem, measured at `c9038174` with the debug CLI and `tsc` 5.9.2.
`Maker` is an interface that the classes `A` and `B` implement.
`P` is a class with a field `f: () => i32` that its constructor sets.
`tsc` accepts each form.

| Form | Result |
|---|---|
| `function build<D extends Maker>(d: D): P { const dd = d; return new P((): i32 => dd.make(1)); }` | S009 "value of type `P` may capture at return" |
| `function f(): () => i32 { const k = 3; return (): i32 => k; }` | S009 (C5, `r10-escaping-capture`) |
| the first form with an async lambda `async (): Promise<i32> => dd.make(1)` | runs, prints `2` |

The request needs a class that keeps a value of a type parameter with
no type parameter of its own. A field of an interface type stays
rejected (C24). C5 keeps the environment of a capturing lambda on the
stack, so the lambda cannot leave the function. An async lambda (§181
rule 2) and a synchronous callback of `then`, `catch`, or `finally`
(§186 rule 4) already own their environment: each evaluation allocates
an environment object that owns the captures, and the function value
keeps it. So a lambda that owns its environment can leave its function
today, and the three tiers lower it.

### 274.1 Rules

1. A synchronous lambda with captures owns its environment (§181 rule
   2) if its value can leave the function that makes it: a `return`, a
   store in a field, an array, a `Map`, or a `Set`, an argument of a
   parameter that the checker infers as escaping (§118), or a capture by
   a lambda that owns its environment.
2. A capturing lambda whose value cannot leave its function keeps the
   stack environment of C5, with no allocation. A program that the
   checker accepted before this section does not change its lowering.
3. The captures stay as C5 states: `const` locals, by value. An owned
   environment retains each counted capture and releases it when the
   last function value that holds the environment is released, so the
   block rule of §175 does not apply to such a lambda. A capture of
   `this` stays as §157 states.
4. The cost is one allocation for each evaluation of such a lambda,
   which the lowering of §181 already gives an async lambda. The escape
   is in the source of the function, or in the declaration of the
   parameter that receives the value (§118).
5. A capturing lambda never reaches a C callback (C5), by any path: a
   direct argument, a parameter, a field, an array element, a `Map`
   value, a call result, or an indirect call. The checker decides it
   for each value at the C callback site, with a fact that holds for
   each storage that the value can come from, not for the direct
   argument only. Before this section no storage could hold a capturing
   lambda, so the check at a C callback site trusted a value that it
   read from storage; this section makes that check unsound. The first
   Phase Review measured five programs that pass a capturing lambda to
   a C callback through a field, an array, a call result, and an
   indirect call; the ship tier passes the owned environment to
   `subscript_rt_cb_bind`, which does not retain it. Rule 7 closes the
   class.
6. The escape fact of a parameter (§118) has a kind: no escape, an
   escape into managed storage, or an escape into a C callback. A
   parameter that reaches both has the kind of a C callback. The kind
   propagates through each parameter that passes the value on, as the
   flag does today. Rule 1 applies to an escape into managed storage
   only, and rule 5 to an escape into a C callback. The coding agent
   measured that the flag alone cannot tell the two: a prototype that
   read it accepted a capture that reaches a C callback through a
   parameter.
7. A C callback type of a mirror (a callback typedef of a boundary
   header, such as `SubLogCallback`) is a type of its own. A named
   function, or a lambda with no captures, converts to it at a place
   that expects it. A plain function value of the same signature does
   not convert to it, and a value of it does not convert to a plain
   function type. A capturing lambda never takes it. So each storage of
   a C callback type holds only values with no environment, and rule 5
   holds for each path. Each type operation keeps the identity of the
   callback type: an alias, a nullable form, a container, a type
   argument, a cast, and a reload signature. A diagnostic names the
   callback type. The coding agent measured three forms before this
   rule. A check of the source of each value at a C callback site
   rejects 16 of 434 accept entries, `a90` among them. The checker did
   not tell a callback type from a plain function type of the same
   signature. A prototype of this rule accepts each of the 434 accept
   entries and changes one reject pin (`r502`: S100 in place of S009),
   and `tsc` accepts the project. A binding that owns the environment
   of a capturing lambda was the third form; it widens C5 and makes the
   bindings of a host that registers a new lambda each frame grow.
8. No program that `tsc` rejects changes from rejected to accepted. The
   three tiers give the same output. The accept entry carries
   `js-comparable` and equals `node`.

### 274.2 Acceptance

1. Red first, at the pin: the accept entry
   `a446-a-capturing-lambda-that-escapes-owns-its-environment` with: the
   first form of the table and a call through each class; a returned
   capturing lambda; a lambda stored in a field, in an array, and in a
   `Map`; a lambda passed to a parameter that a constructor stores; a
   counted capture (a string, an array, a class reference) that the
   lambda reads after the function returns; a lambda called many times
   after its maker returns. At the pin the checker rejects it.
2. After the change, `a446` passes `tsc`, the three tiers, and `node`,
   and the memory accounting of the development tier finds no live
   allocation after the run that the run did not keep.
3. Each reject entry that this section accepts moves to `retired:` with
   a line in its collision text. The implementation lists each one. A
   reject entry for a capture into a C callback stays.
4. A unit test of the checker pins rule 1 for each escape kind, and rule
   2: a lambda passed down to a non-escaping parameter and called there
   keeps `owns_environment` false.
5. C5 in `specs/blocks/collisions.md` gets a revision paragraph.
6. If a golden other than the LIR text snapshot growth of an async
   `main` moves, report and stop.
7. The check that each HIR execution fact reaches the LIR
   (`codegen/tests/lir.rs`) keys an owning lambda by its function
   instance and its position. A lambda in a generic function has one
   instance for each type argument, and a key of the position alone
   compared the captures of one instance with the LIR of another; the
   coding agent measured it at `a446`.

### 274.3 Phase Review

Two fresh reviews of the implementation.

1. CRITICAL, realistic, fixed (first review): a capturing lambda reached
   a C callback through a field, an array, a call result, and an
   indirect call, because the check at a C callback site trusted a
   value that it read from storage. In the ship tier the owned
   environment reached `subscript_rt_cb_bind`, which does not retain
   it. Rule 7 closes the class by type. Five reject entries pin the
   paths (`r503`–`r507`).
2. MINOR, realistic, fixed (first review): the unit test of rule 1
   checked acceptance only, not the flag, and missed six escape kinds.
3. MINOR, realistic, fixed (first review): no committed test ran the
   shapes of the retired reject entries; `a447` holds them.
4. MINOR, realistic, fixed (second review): C5 kept the bullets that
   this section replaces.
5. MINOR, realistic, fixed (second review): the doc of
   `owns_environment` and the diagnostic of rule 7 did not name the
   cause.
6. MINOR, realistic, recorded (second review): §274.4 item 1.
7. MINOR, contrived, fixed (second review): the reject entries of rule
   7 did not pin their lines.

The second review tried 37 paths to put a capturing lambda into a C
callback place: generics, `Map`, nullable forms and narrowing, `??`,
conditional branches, static fields, globals, array methods, method
results, `yield`, aliases, casts, and an async lambda. The checker
rejects each one, and the diagnostic names the callback type. Each of
13 forms of rule 1 ran correctly after a function that overwrites the
stack.

The full gate after the first fix found three more: the check that each
HIR fact reaches the LIR keyed an owning lambda by its position only
(acceptance item 7), the LIR text printed an absent callback type in
each function type, and the rule 7 rejection took the class of a
rejection that `tsc` makes. Each is fixed.

### 274.4 Open

1. A conditional of two values that a C callback type takes
   (`flag ? f : g`) is rejected at a C callback place: the check
   converts only a lambda or a function name.
