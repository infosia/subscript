<!-- §332 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 332. An indirect call runs the values that reach it

*(Added 2026-11-18.)* Origin: §137.3, rank 11 of
`specs/tracking/backlog.md`. A `tsc` divergence with a fix in sight
(standing approval of 2026-11-06).

Problem, measured at `2b7a4105`. The initializer-order check (§137
rules 5 to 5c) lets an indirect call run any value that an earlier item
or its own item made. So a call of a helper with a callback is rejected
if an unrelated lambda reads a later global:

```ts
function run(f: () => void): void { f(); }
const later = (): void => { print(`${LATER}`); };
run((): void => { print("first"); });
const LATER: i32 = 5;
```

S100 "`LATER` is accessed before its declaration, through `run` -> an
indirect call". The same with `xs.map(f)` in a helper and an unrelated
`const scale = (x: i32): i32 => x * FACTOR;`. `tsc` accepts both. The
call of `run` can run only the lambda that it receives.

### 332.1 Rules

1. **The values of an expression.** The callee expression of each
   indirect call of §137 rule 5b can hold these values:
   1. a made value written in place (a lambda literal, a declared
      function used as a value): that value, as in
      `((): i32 => LATER)()`;
   2. a parameter of its unit that the unit never assigns: the values
      of rule 3. A parameter that the unit assigns is item 5, and the
      assigned value escapes by rule 2. *(Amended 2026-11-18: round 3
      stopped because `f = later; f();` in the body ran `later` with no
      route.)*
   3. a `const` binding, module or local: the values that its
      initializer can hold;
   4. a conditional (`c ? f : g`, `a ?? b`, `a || b`) or a
      parenthesized expression: the union of the values of its
      operands;
   5. anything else (a `let`, a field, an array element, a call result,
      a generator step, a method that the scan cannot resolve, a
      foreign call): every escaped value (rule 2).

   *(Amended 2026-11-18: round 1 stopped because the first text gave
   an immediately called lambda no value, so its reads were not
   followed.)*
2. **Escape.** A made value escapes if a unit uses it, or a `const`
   binding that holds it, at a place other than these: the initializer
   of a `const` binding; the callee of a call; a built-in callback
   argument (§137 rule 5b: a direct call); an argument of a *known
   call*, for a parameter that does not escape. A known call is a
   direct call of a declared function, method, or constructor, or an
   indirect call whose callee expression has values by rule 1 items 1
   to 4; its argument goes to the parameter of each callee value. A
   parameter escapes if its unit uses it at a place other than these.
   If a parameter escapes, each value that it can hold escapes. A value
   that is passed to an indirect call of rule 1 item 5, to a foreign
   call, or to a method that the scan cannot resolve escapes.
3. **The values of a parameter.** The values that a parameter can hold
   are: the values (rule 1) of each argument that a known call passes
   for it; and its default value. These are all its values only if
   every call of its unit is a known call. If anything else can call
   the unit (a built-in that calls it as a callback, an indirect call
   of rule 1 item 5, a foreign callback, an escape of the unit), the
   parameter also holds every escaped value. *(Amended 2026-11-18:
   round 2 stopped because the second text took arguments from direct
   calls only, so `const run = (f) => { f(); }; run(later);` did not
   follow `later`. The Phase Review found that a built-in callback
   unit got no values: `m.forEach((h) => { h(); })` over a `Map` of
   function values ran nothing, and the program read a global before
   its initializer ran. The rule now names when the list is complete,
   in place of a list of the other callers.)*
4. **What an indirect call runs.** An indirect call runs the values
   that its callee expression can hold by rule 1. A built-in callback
   argument that is not a lambda literal or a declared function runs
   the values that the argument can hold by rule 1. Each set is limited
   to M(k) of §137 rule 5b. The reads of
   an item follow these values in place of every value in M(k).
5. **Totality.** The use match of rule 2 is exhaustive over the HIR
   with no catch-all arm, as §137 rule 5b requires. A use that the
   match cannot classify escapes.
6. **Residual.** The rule is an over-approximation: a parameter holds
   each value that any direct call in M(k) passes, not only the value of
   one call. An async function body is followed whole (§137 rule 5c).
   The error keeps the global and the route.

### 332.2 Acceptance

1. Red first: an accept entry with the two forms of the problem, a
   parameter passed on through two helpers, and an indirect call of a
   `const` binding that holds a lambda that reads no later global. It
   prints. It is `js-comparable: yes` if `node` prints the same bytes.
   At the pin, `check` gives S100.
2. Reject entries that stay S100, each with the route: the lambda that
   reads the later global passed to `run` itself; the same through two
   helpers; the same stored in a module `let` and called through it;
   an immediately called lambda that reads the later global; a `Map`
   of function values that `forEach` calls, with a lambda and with a
   declared function as the callback.
   Each header states what `tsc` does, measured.
3. Unit tests for rules 1 to 4, each with a control in the same shape
   (core principle 9), and a test that a new HIR use kind does not
   build until rule 2 names it.
4. The CLI `check` output of each other corpus entry is the same at the
   pin and after. If an entry changes, the report names it. The
   development tier and the ship tier print the same output.
5. Cost: the checker time over the corpus, at the pin and after
   (debug, best of three). A ratio above 1.05 is reported with its
   cause.

### 332.3 Sections this one amends

- §137 rule 5b: an indirect call runs the values that reach it
  (rules 1 to 4), not every value in M(k). §137.3 closes.
- §137 rule 5c: the residual is rule 6 of this section.

### 332.4 Phase Review

Form. `compiler/src/check/init_effects/values.rs` computes, once per
program, the escaped values, the values of each parameter, and the
count of calls of each unit that are not known calls. The route scan of
§137 follows, at each indirect call, the values of its callee
expression in place of every value in M(k). Each match of the use scan
names every HIR kind; `compiler/tests/initializer_use_totality.rs`
fails to build when a kind is added.

Measured, the pin `5dd2718f` and after, development tier, ship tier,
and `node`:

| Form | Pin | After |
|---|---|---|
| `run((): void => …)` with an unrelated lambda that reads a later global | S100 | accepted, `first` |
| a helper with `xs.map(f)` and an unrelated `const scale` | S100 | accepted, `2,3` |
| `outer` passes its parameter to `run` | S100 | accepted, `two` |
| `early()`, a `const` lambda | S100 | accepted, `early` |
| `r548` to `r551`: the lambda passed, passed through two helpers, stored in a `let`, called in place | S100 | S100, the same route |
| `r552`, `r553`: `Map.forEach` over function values, a lambda and a declared callback | S100 | S100 |

`node` throws `ReferenceError` for each reject entry, and `tsc` accepts
each. The CLI `check` output of 1,085 other corpus entries is the same
at the pin and after. Checker time over the corpus, debug, best of
three: 9.90 s to 10.22 s (1.032).

The contract was corrected three times before the review, each when the
round stopped on a missing value source: a lambda called in place, an
argument of a known indirect call, and an assigned parameter. Three
negative tests of `codegen/tests/module_init_routes.rs` encoded the
former over-approximation; each now passes or stores the value that
reads the later global.

The first review found one MAJOR finding, realistic: a unit that a
built-in calls as a callback had no parameter values, so
`m.forEach((h) => { h(); })` over a `Map` of function values read a
global before its initializer ran (signal 11 in the development tier).
Rule 3 now states when a parameter's values are complete, and the code
counts the calls of each unit that are not known calls. Its MINOR
findings are fixed: a catch-all arm in the callback match, and changed
indentation in test strings. A second review probed about 50 forms
and found no CRITICAL or MAJOR finding.

### 332.5 Open

1. Contrived: a unary or binary expression other than `||` and `??`
   gives a callee no values. No such expression gives a function value
   in the subset now.
2. Rule 6: a parameter holds the values of every known call in M(k),
   not of one call.
