<!-- §156 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 156. An initializer types an unannotated declaration

*(Added 2026-10-04.)* Origin: `compiler.md` §154.3 item 2 (`collisions.md`
C24 row 4). The owner selected it on 2026-10-04.

Problem: the checker infers the type of a local from its initializer, and
the result of an expression lambda from its body. It does not infer at
other positions with an initializer. `tsc` accepts each form below.
Measured at `40b0cbff` with `subscript check` and `tsc` 5.9.2:

| Form | `subscript check` |
|---|---|
| `const MAX = 10;`, `let count = 0;`, `const RATE = 0.5;`, `const NAME = "pool";`, `const ids = [1, 2, 3];` at module level | S100 "module-level variables require a type annotation" |
| `const twice = (x: i32): i32 => { … };` at module level | the same |
| `size = 4;`, `label = "c";`, `ratio = 1.5;` as fields | S100 "fields require a type annotation" |
| `static limit = 8;` | S100 "static fields require a type annotation" |
| `function grow(n: i32, by = 2): i32` | S100 "parameters require a type annotation" |

A defect in the same code path, present at the pin: two functions whose
annotated defaults call each other (`function first(n: i32 = second(1)):
i32` and `function second(n: i32 = first(1)): i32`) make `subscript
check` abort with a stack overflow. `tsc` accepts the program, and
`node` prints `1`.

These are the forms that a TypeScript author writes first. A module
constant and a field with a default are in most programs.

### 156.1 Rules

1. If a module variable, a field, a static field, or a parameter with a
   default has no type annotation, its type is the type that a local
   with the same initializer and the same `const` or `let` takes. C4
   applies: an integer literal with no context is `i32`, and a
   fractional literal is `f64`.
2. If the initializer has no type that a local takes, the declaration
   is rejected as a local is: for example, `[]` and `null`.
3. The type of an unannotated declaration is decided when the checker
   first needs it: at the declaration, or at an earlier read, also a
   read in a lambda body (`const f = () => later; const later = 1;`,
   which `tsc` accepts). The checker checks the initializer once and
   keeps the checked form for lowering. The type and the checked
   initializer are separate facts. The type state is one of: undecided,
   in progress, decided with its type, or rejected. An annotated
   declaration is decided by its annotation before any initializer is
   checked, so a lambda in its own initializer can read it (`const
   countdown: (n: i32) => i32 = (n: i32): i32 => … countdown(n - 1)`,
   which this checker and `tsc` accept at the pin).

   To decide a type, the checker checks only the parts of the
   initializer that the type needs. The decision produces a complete
   checked form. A part that the type does not need is a deferred unit
   in its own slot of that form, and the checker checks it once, later,
   in that slot. A deferred unit is one of two things: the body of a
   lambda with an annotated result, or the arguments of a direct call
   that is the whole initializer, if the callee has one signature, the
   signature is not generic, and its result is annotated. The type of
   that call is the callee's result. Every other call checks its
   arguments in the decision, and an argument decides its parameter on
   demand. If a supplied argument targets a parameter that is in
   progress, that is the cycle below. `tsc` follows the same rule,
   measured:
   - `function f(n: i32, k = f(n - 1, 0)): i32` is accepted (a direct
     call).
   - `k = n > 0 ? f(n - 1, 0) : 0`, `k = [f(n - 1, 0)]`, and `k = f(n -
     1, 0) + 1` are rejected (TS7022).
   - `k = n > 0 ? f(n - 1) : 0` is accepted (`k` is not supplied).
   - `function first(n = second(1) + 0)` with `function second(n =
     first(1))` is accepted (the inner initializer is a direct call).

   A body or an expression that the decision checks is checked under the
   same rules and the same context as at its declaration: the same
   `this`, the same scope, the same §147 and C5 rules. A decision adds
   facts. It never removes or replaces a fact that a check in the
   decision reads: a parameter, a constructor signature, or a scope
   entry stays readable with its decided type. A lambda whose result is annotated
   has the type of its signature; its body is not needed and is checked
   later, as the body of an annotated declaration is. A lambda with no
   result annotation needs its body. The rule applies at every depth: an
   array of lambdas, a lambda in a lambda. Measured with `tsc`, which
   follows the same rule:
   - `const g = (n: i32): i32 => … g(n - 1);` is accepted.
   - `const hs = [(n: i32): i32 => hs.length];` is accepted.
   - `const left = (): i32 => right(); const right = (): i32 => left();`
     is accepted.
   - `const f = (n: i32) => … f(n - 1);` is rejected (TS7023).
   - A field `g = (n: i32) => this.g(n);` is rejected (TS7024).

   A parameter with a default is a declaration under this rule. A
   function signature carries a type state for each parameter, and its
   annotated result is decided before any parameter. A call in a
   decision needs only the callee's result type; the arguments are
   checked with the body, later. Measured with `tsc`: `function first(n
   = second(1)): i32` with `function second(n = first(1)): i32` is
   accepted.

   A read of a rejected declaration gives no diagnostic (§154 rule 9).
   If a decision needs a type that is in progress (a cycle), the
   declaration is rejected (S100). `tsc` rejects the cycle (TS7022,
   TS7023, TS7024), so the site is `TscRejects`.
4. A read that runs before the initializer is a question of order, not
   of type. C14 and §147 rule 2 decide it as they do for an annotated
   declaration (measured: `const x: i32 = y + 1; const y: i32 = 2;` and
   `a: i32 = this.b + 1; b: i32 = 2;` are rejected; `tsc` gives TS2448
   and TS2729). An unannotated declaration gets the same diagnostic.
5. An inferred type is a declared type for every consumer: an import of
   the name, the host API surface (§129: a defaulted parameter of an
   exported entry function), the class layout (invariant 1), and the
   three tiers. No consumer reads the initializer to find the type.
6. A block lambda with no result annotation stays rejected (S100,
   `Diverges`, C24 row 4), as at the pin. `tsc` infers a block result by
   rules that depend on the order and the form of the `return`
   expressions; four Phase Reviews of a prototype found programs that
   it accepted and `tsc` rejects. The owner removed block-result
   inference from this section on 2026-10-04. The remedy is a result
   annotation; an expression lambda infers its result, as at the pin.

   A lambda parameter with no annotation takes its type from the
   context, as it does in a local. If the context is a parameter of a
   generic callee, a rejection there is `Diverges` (C24 row 4). `tsc`
   accepts the lambda when the type parameter is fixed by another
   argument (`firstOr(handlers, (x) => x + 1)` with `firstOr<T>(xs: T[],
   fallback: T)`), and rejects it when nothing fixes it (`g<T>(v: T)`
   with `g((x) => 1)`, TS7006). The checker does not infer the type
   argument from the other arguments before it checks the lambda, so
   the block shows in both cases; the second is a recorded reverse
   cost (§156.3).

   A defaulted parameter of a method of a generic class, whose inferred
   type contains a type parameter of the class (`pair(other = this.v)`
   with `v: T`), is rejected (S100, `Diverges`, C24 row 4), with the
   remedy "annotate the parameter". `tsc` types such a parameter with
   the uninstantiated type parameter, so a call that supplies a
   concrete argument is rejected (TS2345, TS2322); this checker would
   instantiate it.
7. A function declaration, a method, and a parameter with no default
   still require an annotation. C24 row 4 states them.
8. C24 row 4 lists only the positions of rules 6 and 7, with their
   reasons: a signature is the contract of a call, and the host binds it
   by its declared types; a block result and a generic-class default
   have no inference that agrees with `tsc` for every program.

9. An omitted argument is evaluated by one lowered function per
   defaulted parameter. The call site calls it with the receiver and
   the earlier arguments. No consumer inlines a default expression at a
   call site. A default that calls a function whose default calls back
   is ordinary recursion at run time, and it ends as the program ends
   it (measured: `depthA(n, acc = n > 0 ? depthB(n - 1) : 0)` with the
   mirror `depthB` prints `22` under `node`). A default that reads
   `this` reads the receiver that the call site passes.
10. Every consumer that reads a fact of a default (the exception scan,
    the C14 initialization-effect scan) reads it only for the omitted
    arguments of the call.
11. A generic instance that any phase requests is checked before
    lowering. No queue is filled by a phase that runs after the queue
    drains.
12. The checker reports diagnostics in the source order of their
    top-level declarations: by module in the order of §137, then by the
    start of the module-level declaration that holds the diagnostic.
    Inside one declaration, diagnostics keep the order in which the
    checker emits them, so the diagnostic that decides a rejection stays
    first (§154 rule 13). A deferred unit emits in the order of the
    declaration that holds its slot. Measured: a sort by line and column
    put the follow-on "type `Resource` is not indexable" (6:3) before
    the deciding S016 at 6:12 in `resource[Symbol.dispose]()`.

### 156.2 Acceptance

1. Red first: one accept entry, `js-comparable: yes`, with every form of
   the Problem table, an imported inferred module constant, a field
   whose inferred type sets the class layout (an `i32`, an `f64`, and a
   `string` field), and a `@ValueType` class with an inferred field.
   Its golden is the `node` output. At the contract pin, the checker
   rejects it; record the diagnostics.
2. One reject entry for the rule 3 cycle (`tsc: rejects`) and one for
   the generic-class default of rule 6 (`tsc: accepts`). The accept entry holds the rule 3 lambda
   read of a later declaration.
3. Unit tests for each rule. A test shows that an inferred field and an
   annotated field of the same type give the same layout (`offsetof`).
4. The §154 total test passes. The sites that rule 1 removes leave the
   site table, and their witnesses move to the remaining sites.
5. The mutual-default program of the Problem section and the `depthA`
   and `depthB` program of rule 9 run with no abort on every tier,
   annotated and unannotated, and match `node`. Both are in the accept
   entry.
6. The accept entry calls builtin methods with arguments in each
   inferred position (module variable, field, static field, default): `slice`, `join`, `indexOf`, `includes`,
   `toFixed`, and a string `slice`. It also requests a generic instance
   from an inferred field and from a default inside a function body.
7. No existing `.expected` golden moves.

### 156.3 Open

The cycle site of rule 3 is a measured claim, as the `TscRejects` class
of §154.3 is: `tsc` decides some initializers by a fast path that this
checker approximates by rule 3. A new instance is fixed when it is
found, by a rule on a fact the checker has, and it does not block a
phase. Instances found by the third Phase Review (contrived): `k = f(n -
1, 0) as f64`, a call through an annotated function value `h(f2(n - 1,
0))`, and `g(f(n - 1, 0))` with `g` an inferred lambda reach the cycle
site with no block; `tsc` accepts each.

`tsc` gives a `const` module variable and a `readonly` field with a
literal initializer its literal type: `const MODE = 1;` then `MODE === 2`
is rejected (TS2367), and `readonly size = 4;` then `this.size = n` in
the constructor is rejected (TS2322). This checker types them as a local
with the same initializer, and a local `const` has the same difference
at the pin. The owner recorded it on 2026-10-04 as a class of its own,
for a later section on literal types.

A chain of about 130 or more unannotated forward references (`const f0 =
() => f1(); …`) overflows the stack in a debug build (contrived; `tsc`
overflows at 1,000).

Instances found by the fifth Phase Review (contrived): `const handlers =
[() => handlers.length];`, `const f = () => f;`, and the pair `const
idle = () => running; const running = () => idle;` reach the cycle site
with no block; `tsc` types an expression lambda without its body there.

A lambda passed to a bare type parameter that no other argument fixes
(`g((x) => 1)`, TS7006) carries the rule 6 block.

Present before §156 in the annotated form: a defaulted parameter before a
required one (`function f(a = 1, b: i32)` with `f(2)`) passes the checker
and fails at lowering ("missing argument `b` with no default"); `tsc`
gives TS2554.

Check time is 30–50 % above `5fbada8c` on generated programs (4,500
module declarations: 11.05 s against 7.42 s, debug build). The scaling is
quadratic at the pin; §156 adds a constant factor. §160 measures the cost again and closes it.

Present before §156 in the annotated form; the inferred forms now reach
them. Each is realistic.

1. `(x: i32): i32 => { while (true) { return x; } }` passes the checker
   and fails at lowering: "non-void function has a reachable
   fallthrough".
2. A function value with a default, called with fewer arguments, gives
   "(1 required)", which is false.
