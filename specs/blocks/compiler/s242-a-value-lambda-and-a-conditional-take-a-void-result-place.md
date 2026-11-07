<!-- §242 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 242. A value lambda and a conditional take a `void` result place

*(Added 2026-11-07.)* Origin: §240.4 item 4, and a form measured beside
it. Under the owner's standing approval of 2026-11-06 for `tsc`
divergences with a known fix.

Problem, measured at `806b0196` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `named(x: i32): i32` and `other(x: i32): void`
are functions, `K.twice(x: i32): i32` is a static method,
`each(xs: i32[], f: (x: i32) => void): void` is a function,
`CB = (x: i32) => void`, and `m` is a `Map<string, i32>`.

| Form | subscript | `tsc` |
|---|---|---|
| `[1].forEach(flag ? named : K.twice);` | S100 "type mismatch: the `forEach` callback expects `(i32) or (i32, i32)` => void, got `(i32) => i32`" | accepts |
| `[2].forEach(flag ? other : named);` | S100 "conditional branches have no common type: `(i32) => void` and `(i32) => i32`" | accepts |
| `[3].forEach(!flag ? (x: i32): i32 => x : named);` | S100, the `forEach` callback kind | accepts |
| `m.forEach(flag ? (v: i32, k: string): void => {} : (v: i32, k: string): i32 => v);` | S100 "type mismatch: the else branch expects `(i32, string) => void`, got `(i32, string) => i32`" | accepts |
| `each([6], (x: i32): i32 => x);` | S100 "type mismatch: the argument expects `(i32) => void`, got `(i32) => i32`" | accepts |
| `[7].forEach((x: i32): i32 => { print(`${x}`); return x; });` | S100, the `forEach` callback kind | accepts |
| `const cb: CB = (x: i32): i32 => x;` | S100, the initializer kind | accepts |
| `const r: CB = flag ? (x: i32): i32 => x : other;` | S100 "type mismatch: the then branch expects `(i32) => void`, got `(i32) => i32`" | accepts |

These forms are accepted today:

- `each([4], flag ? named : K.twice);` and
  `const cc: CB = flag ? other : named;`: as the argument of a user
  function or as an initializer, the place reaches each branch, and
  §240 applies to each reference.
- `each([8], (x: i32) => x + 1);` and `[9].forEach((x: i32) => x + 1);`:
  a lambda with no result annotation in a place with the result `void`
  discards its operand (§237 rule 4).

So two holes remain. The callback place of a built-in method does not
reach the branches of a conditional. A lambda with a result annotation
that is a value does not take a place with the result `void`, in any
place.

A lambda literal is made at its place. No other code holds it, so no
reader of the value sees its result type. The lambda can have the
result `void` with no change to its environment: each `return e;` of
its body evaluates `e` and discards it. That is the form of §237 rule
4, which the language has, so the lowering is known and has no extra
cost.

### 242.1 Rules

1. The callback place of a built-in method reaches each branch of a
   conditional expression, as the place of an argument of a user
   function does. A branch that is a reference of §240 rule 1 takes the
   lambda of §240 rule 2. A branch that is a lambda takes rule 2 of this
   section.
1a. Some callback places take an optional trailing index, such as the
   callback of `forEach` or `filter` on an array. A conditional there
   is a tree of leaves: a leaf is a branch that is not a conditional
   and not in parentheses. Each leaf decides the arity of its own place.
   A lambda literal leaf takes the place with its own number of
   parameters as its context. Each other leaf is checked with no
   context, as before this section. Then it takes the place whose
   arity equals the arity of its type. No other sub-expression gets a
   place from the callback. The common type of the conditional is
   decided after each leaf takes its place. So rule 1a rejects no
   program that was accepted before this section.
2. A lambda literal with a result annotation that is a value type, in a
   place whose type is a function type with the result `void`, is
   accepted when the lambda with the result `void` fits the place
   (§235 rule 2). The lambda is not async and not a generator.
3. The body of a lambda of rule 2 is checked against its annotation, as
   today: a `return` operand that does not fit the annotation keeps its
   diagnostic. The lambda then has the result `void`, and each
   `return e;` evaluates `e` once and discards it.
4. The places of rule 2 are the places of §240 rule 2: an argument, an
   initializer, an assignment, a return value, a field, an element of an
   array literal, a branch of a conditional in one of these, and the
   callback of a built-in method.
5. An async lambda, a generator lambda, and a lambda with a result
   annotation `T | undefined` keep their present diagnostic. A lambda
   whose result is or carries a handle, such as `Promise<i32>[]`, keeps
   its present diagnostic too: a discarded handle has no holder that
   awaits it (S013). A result carries a handle when the S013 drop check
   finds one in it, nested arrays and `FixedArray` included. The same
   holds for a reference of §240 rule 1: a function whose result
   carries a handle is not converted. At the pin, §240 converted such a
   reference and dropped the handles: `each([1], jobs)` with
   `jobs(x: i32): Promise<i32>[]` was accepted. That program is now
   rejected with the diagnostic of its place.
6. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 242.2 Acceptance

1. Red first, at the pin: the accept entry
   `a428-value-lambda-and-conditional-take-a-void-result-place` is
   rejected. The tracking note records the output.
2. `a428` passes `tsc`. Its output under `node` equals its golden. It
   has each form of the first table. Each callback prints, so the order
   of calls is visible, and a lambda of rule 2 has a `return` operand
   with a visible effect, so rule 3 is visible.
3. A unit test pins rules 1, 2, 3, and 5. It has each form of rule 5
   (rejected) as a control, and a lambda of rule 2 whose `return`
   operand does not fit the annotation (rejected).
4. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves.
