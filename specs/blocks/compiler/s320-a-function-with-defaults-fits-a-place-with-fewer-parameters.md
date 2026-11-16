<!-- §320 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 320. A function with defaults fits a place with fewer parameters

*(Added 2026-11-16.)* Origin: §164.4 item 1. The owner told the work to
continue down the open-item triage on 2026-11-16.

Problem, measured at `7c6fb2fc`. With

```ts
function f(a: i32, b: i32 = 10): i32 { return a + b; }
function app(k: (a: i32) => i32): i32 { return k(1); }
const g = (a: i32, b: i32 = 20): i32 => a * b;
```

`app(f)`, `app(g)`, and `const h: (a: i32) => i32 = f;` each give S100
"type mismatch: … expects `(i32) => i32`, got `(i32, i32) => i32`".
`tsc` accepts each; `node` calls `f(1)`, so `b` takes its default. The
site claims `TscRejects`, which is false for these forms.

### 320.1 Rules

1. A function value fits a place whose type is a function type with
   fewer parameters when each parameter of the value after the
   parameters of the place has a default, and the value with only the
   parameters of the place fits the place (§235 rule 2).
2. The meaning is the lambda that a program can write there: the
   parameters of the place, a body that calls the value with those
   arguments only, so each omitted parameter takes its default (§164,
   §256). The checker records that lambda in the HIR in place of the
   reference, as §240 rule 2 and §246 rule 1 do.
3. The values of rule 1 are the values whose function is known at
   check time, so that its defaults are known: a function declaration,
   a static method, and a module `const` or a `const` local whose
   initializer is a lambda with defaults or the name of such a value.
   A function type carries no default (§303), so a `const` local whose
   initializer is another value (`const saved = l;` with
   `let l = flag ? f : g;`) is not a value of rule 1 and keeps the
   diagnostic of rule 4. *(Amended 2026-11-16: round 1 stopped because
   the first text included each `const` local, and the defaults of
   `saved` are not known at check time.)* Each place of §235 rule 5a is a place of rule 1.
   The rules of §246 rule 3 (S009, §175) apply to the lambda.
4. A value that rule 3 excludes (a `let` local, a `const` local with
   another initializer, a parameter, a field read, a call result) and
   that fits by rule 1 gets a diagnostic that names the cause: the
   defaults of the value are not known at check time, so write a
   lambda that calls the function, and the site is
   `Diverges` with a block of §79 rule 3 and an example of §311. An
   async function and a generic function keep their present
   diagnostic, as §246 rule 4 states.
4a. A built-in callback place keeps §244. Its parameters are each
   argument that the method passes (`map` passes the element, the
   index, and the array; `Map.groupBy` passes the element and the
   index; `Set.forEach` passes the value twice and the set). A
   parameter of the value at a position that the method fills takes
   that argument, as in `node`, never its default. Where the pin
   rejects the value at that position (the repeated value of
   `Set.forEach`, the map of `Map.forEach`, the array of `map`), the
   value stays rejected with the diagnostic of the pin. Rule 1 applies
   only to a parameter after the last argument that the method passes.
   *(Amended 2026-11-16 after the Phase Review: the round gave a
   parameter at a filled position its default, so
   `Map.groupBy([5, 6], grp)` with `grp(a: i32, i: i32 = 99)` gave the
   keys `5:99` and `6:99`, and `node` gives `5:0` and `6:1`; the three
   tiers agreed, so the differential gate did not see it.)*
5. A default runs once per call of the place, as the call of rule 2
   runs it. The program has no other run-time cost.
6. No accepted program changes its meaning. No program that `tsc`
   rejects becomes accepted. The development tier, the ship tier, and
   the interpreter give the same output. The accept entry carries
   `js-comparable` and equals `node`.

### 320.2 Acceptance

1. Red first: an accept entry with `app(f)`, `app(g)`, and `const h`
   with `h(2)`, with output equal to `node`. It is rejected at the pin.
2. A default that has a side effect (a call that prints) runs once per
   call of the place, in each tier.
3. A test of rule 4: `let l = f; app(l);` gives the §246 rule 5
   diagnostic with a block. A test that a place with a parameter whose
   type does not fit stays a type mismatch.
4. The CLI `check` output of each corpus entry is the same at the pin
   and after the change, except the new entries.

### 320.3 Phase Review

*(Filled in after the review.)*

### 320.4 Open

None.
