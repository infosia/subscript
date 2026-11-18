<!-- §335 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 335. A call through a constraint has the constraint type

*(Added 2026-11-18.)* Origin: §334.5 item 1, rank 13 of
`specs/tracking/backlog.md`. A `tsc` divergence with a fix in sight
(standing approval of 2026-11-06).

Problem, measured at `9e412632`. A class can implement an interface
method with a narrower result (§334 rule 1). A generic body is checked
in each instance with its type parameters replaced by the instance
types, so a call through a constraint gets the class method type:

```ts
class C { v: i32 = 7; }
interface R { m(): C | null; f(): (() => i32) | null; }
class K implements R { m(): C { … } f(): () => i32 { … } }
function g<U extends R>(x: U): i32 { const o = x.m(); if (o !== null) { return o.v; } return -1; }
function h<U extends R>(x: U): i32 { let fn = x.f(); if (fn === null) { return -1; } fn = null; return 0; }
g(new K()); h(new K());
```

Each gives S100 in the instance with `K` ("operator `!==` not defined
for `C` and `null`", "the assignment expects `() => i32`, got `null`").
The instance with a class `K0` whose methods return `C | null` is
accepted. `tsc` checks the body once, with `o: C | null`, and accepts
it. The instance check does not keep the constraint of a type
parameter (`compiler/src/check/generics.rs` replaces `U` with `K`), so
it cannot give the call the constraint type.

### 335.1 Rules

1. **The instance type carries the constraint.** In an instance of a
   generic body or a generic class, the type that replaces a type
   parameter `U` with an interface constraint `I` carries `U` and `I`
   as part of the type. So every value of that type carries them: a
   local, a parameter, a field (`this.t` with `t: T`), an array element
   (`xs[0]`, a `for…of` binding over `U[]`), a narrowed `U | null`, and
   a call result of type `U` (`id(x)` with `id<U extends R>(x: U): U`).
   Two type parameters with one instance type carry their own
   constraints. For assignment, layout, and lowering, the type is its
   instance type. A record on each binding site is not this form: a
   site that does not set it gives the class type again. *(Amended
   2026-11-18 after the Phase Review: round 1 kept the constraint on
   three local binding sites only, so a field, a `for…of` binding, an
   element, a narrowed union, and a call result of type `U` kept the
   class method type, and one instance of a body rejected it while
   another accepted it.)*
1a. **Outside the generic class, a member has the plain type.** A read
   of a member of a generic class instance from outside the class body
   (`b.t` with `b = new B(new K())`) has the instance type with no
   constraint (`K`), as `tsc` types `b.t` by the type argument of `b`.
   So `let o = b.t.m(); o = null;` stays rejected, as `tsc` rejects it
   (TS2322). A class instance type does not keep the type arguments of
   each use site, so inside another generic body a read of `b.t` with
   `b: B<U>` also has the plain type, and a null test of its result
   through `R` stays S100 there (recorded in the Phase Review). This
   errs toward rejection. The rule covers the whole type of each member
   seen from outside the class body, at every depth: a field, a method
   result, and each parameter type of a method, so a lambda passed to
   `b.each((t) => …)` with `each(f: (t: T) => i32)` takes `t: K`. One
   function removes the constraints from the member type; no member
   kind has its own rule. *(Amended 2026-11-19 after the fourth Phase
   Review: a method parameter type kept the constraint, so
   `b.each((t) => { let o = t.m(); o = null; return 1; })` was
   accepted; `tsc` gives TS2322.)*
   *(Added 2026-11-18: round 2 stopped because the constraint type of
   rule 1 leaked to a read from outside the class, which accepted a
   program that `tsc` rejects.)*
1b. **An instance does not depend on its caller.** One instance of a
   generic body serves every caller with the same instance types. So
   the instance is checked with the plain instance type of each type
   argument, and with the constraint of its own type parameters only
   (rule 1). A constraint that a caller's argument carries is not part
   of the instance. A call result whose type names a type parameter of
   the callee gives back each caller's own argument type for it,
   matched by the parameter identity, with or without a constraint on
   the callee parameter. So `first<T>(xs: T[]): T` gives
   `ConstrainedInstance` in `g<U extends R>` and `K` in `main`, in
   either order of the calls. *(Added 2026-11-18 after the second Phase
   Review: an unconstrained instance kept the constraint of its first
   caller, so the order of calls decided a false S011 or S100, and
   `let o = pick(k).m(); o = null;` was accepted where `tsc` gives
   TS2322.)*
1c. **Equality of types sees the instance type.** In an instance, each
   comparison of two types (a conditional join, an assignment, an
   operator operand, a return, a cache key) compares instance types: a
   constrained instance type of rule 1 equals its plain instance type,
   and equals any other constrained instance type with the same plain
   instance type. Only a method call reads the constraint (rule 2). The
   identity of type parameters (`U` is not `V`) is the check of the
   generic definition, not of an instance. The rule is one definition
   of type equality, not a list of comparison sites. *(Added 2026-11-19
   after the third Phase Review: `c ? x : y` with `x: U`, `y: V` and one
   instance type `K` gave "conditional branches have no common type:
   `K` and `K`", a regression from the pin, because raw equality saw two
   different types.)*
2. **A method call has the constraint type.** A method call on such a
   value has the type of the method of `I`, with the instance types of
   the body in place of its type parameters. The call runs the method
   of the instance class. If the two method types differ, §235 rule 5a
   records the conversion of the call result to the type of `I`.
3. **One type for every instance.** So each instance checks the body
   with the same types at each call through a constraint, as `tsc`
   checks the body once. A body that one instance accepts, every
   instance accepts, for these calls.
4. **Other uses stay.** A value of type `U` passed to a parameter, a
   return, and a store keep the instance type. A constraint that is a
   class (not an interface) has the same method types in each instance
   (the subset has no `extends` for classes), so rule 2 does not change
   it.

### 335.2 Acceptance

1. Red first: an accept entry with `g` and `h` of the problem, and a
   function with two type parameters `U extends R, V extends R` that
   tests both results, each called with `K` and with `K0`. Results
   `C`, a function type, an array, and a `Map`. It also calls through
   each value form of rule 1: a field of a generic class `B<T extends
   R>`, a `for…of` binding over `U[]`, `xs[0]`, a narrowed `U | null`,
   and `id(x)`; the rule 1c forms `c ? x : y` (`x: U`, `y: V`) and
   `c ? x : new K()`, each then compared with `===`; and the rule 1b forms `first<T>` and `pick<T>`, each
   called first from a constrained body and first from `main`, and a
   generic method `id<T>`. A reject entry holds the rule 1a form
   (`let o = b.t.m(); o = null;` outside the class); its header states
   TS2322, measured. A second reject entry holds
   `let o = pick(k).m(); o = null;` after a constrained call of
   `pick` (TS2322). A third reject entry holds the `b.each` form of
   rule 1a (TS2322). It prints each result.
   It is `js-comparable: yes` if `node` prints the same bytes. At the
   pin, `check` gives S100.
2. A unit test for rules 1 and 2, with the control that a call on a
   value of the class type (not through a constraint) keeps the class
   method type (core principle 9).
3. The development tier and the ship tier print the same output. The
   CLI `check` output of each other corpus entry is the same at the pin
   and after. The LIR golden changes only for a484 (its calls through a
   constraint record the conversion of rule 2) and the new entry.

### 335.3 Sections this one amends

- §334.5 item 1 closes.
