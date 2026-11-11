<!-- §240 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 240. A named function takes a `void` result place

*(Added 2026-11-07.)* Origin: §237.4 item 1 and §238.4 item 3. Under the
owner's standing approval of 2026-11-06 for `tsc` divergences with a
known fix.

Problem, measured at `7d4178ac` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `named(x: i32): i32` and
`two(x: i32, i: i32): boolean` are functions, `K.twice(x: i32): i32` is
a static method, `each(xs: i32[], f: (x: i32) => void): void` is a
function, and `an(x: i32): Promise<void>` is async.

| Form | subscript | `tsc` |
|---|---|---|
| `xs.forEach(named)` | S100 "type mismatch: the `forEach` callback expects `(i32) or (i32, i32)` => void, got `(i32) => i32`" | accepts |
| `xs.forEach(two)` | S100, the same kind | accepts |
| `each(xs, named)` | S100 "type mismatch: the argument expects `(i32) => void`, got `(i32) => i32`" | accepts |
| `each(xs, K.twice)` | S100, the same | accepts |
| `const cb: (x: i32) => void = named;` | S100 "type mismatch: the initializer expects `(i32) => void`, got `(i32) => i32`" | accepts |
| `const cb2: (x: i32) => void = fv;` with a local `fv: (x: i32) => i32` | S100, the same | accepts |
| `each(xs, an)` and `each(xs, async (x: i32) => {})` | S100 "the argument expects `(i32) => void`, got `(i32) => Promise<void>`" | accepts |

A caller of a function type with the result `void` does not read the
result. A named function or a static method has no environment, so a
function that calls it and discards the result has no environment too:
`(x: i32) => { named(x); }`. That lambda is a form the language has, so
the lowering is known and has the cost of the call. No tier changes.

A function value with an environment, such as the local `fv`, needs a
new environment that holds the original function value. That new
environment needs an owner, and no rule of the language gives one. This
section does not add it (§240.4).

An async function or an async lambda in a place with the result `void`
drops its handle. No scheduler runs a handle that no holder awaits
(S013, `compiler.md` §70). That reason holds today, so these forms stay
rejected.

### 240.1 Rules

1. A reference to a function declaration or to a static method, in a
   place whose type is a function type with the result `void`, is
   accepted when the function is not generic, not async, and has a
   result that is not `void` and not a handle, and when the function
   with its result read as `void` fits the place (§235 rule 2). A
   function with a result `T | undefined` keeps its C7 diagnostic
   (§240.4).
2. The meaning of rule 1 is a lambda with no environment, with the
   parameters of the function: it calls the function with its
   arguments and discards the result. The checker records that lambda
   in the HIR in place of the reference. Each place of §235 rule 5a is
   such a place: an argument, an initializer, an assignment, a return
   value, a field, an element of an array literal (also beside a
   spread), and the callback of a built-in method, `Promise.finally`
   included.
2a. A site that checks a value against a place decides with the check
   that takes the value (§235 rule 5d). It does not decide with a
   type-only check, such as `assignable`, before it: that check does
   not see the lambda of rule 2. `??`, `??=`, `||=`, and `&&=` are such
   sites. The total check of §235 rule 5d lists each remaining
   type-only check of the type of a value, `assignable` and
   `require_assignable`, and the type-only checks `type_fits` and
   `function_fits`, with the reason that no conversion applies there,
   and fails on a new one.
3. The function runs once per call of the place, as the reference does.
   The program has no other run-time cost: a static callback lowering
   of a built-in method calls the function inline, as it calls a lambda.
4. A function value with an environment, an async function, and a
   generic function keep their present diagnostic in such a place.
5. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 240.2 Acceptance

1. Red first, at the pin: the accept entry
   `a427-named-function-takes-a-void-result-place` is rejected. The
   tracking note records the output.
2. `a427` passes `tsc`. Its output under `node` equals its golden. It
   has the first five rows of the table, `Map.forEach` and `Set.forEach`
   with a named function, a named function that prints, so the order of
   calls is visible, a field of a function type with the result `void`
   that holds a named function, and a return of a named function from a
   function with the result `(x: i32) => void`.
3. A unit test pins rules 1, 2, and 4. It has the HIR lambda of rule 2,
   and each form of rule 4 (rejected) as a control.
4. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves except the LIR text snapshot, and that
   snapshot moves only if `a427` has an async or generator entry.

### 240.3 Phase Review

*(Added 2026-11-07.)* Three fresh reviews of the implementation. The
contract gained rule 2a, the places of rule 2 beside a spread and in
`Promise.finally`, and the exclusion of a `T | undefined` result.

1. CRITICAL, realistic, fixed: `handler ??= named` checked clean and did
   not assign: dev and ship printed `no handler`, and `node` printed
   `n1`. The site decided with a type-only `assignable` check, then
   assumed that the value check reported a diagnostic. Rule 2a.
2. MAJOR, realistic, fixed: `n ?? named` checked clean, then gave an
   internal lowering error. The same cause.
3. MAJOR, realistic, fixed: an element beside a spread,
   `[...base, named]`, was checked by type only, and the total check
   gave a false reason for that call. The element now takes the value
   check; the §235 form `[...base, nb]` is accepted too.
4. MINOR, fixed: `Promise.finally(tick)` with a value result; the total
   check did not scan `type_fits` and `function_fits`.
5. MINOR, recorded: §240.4 items 4 to 9; the module initialization
   diagnostic names "a lambda" on the path of a referenced function.

### 240.4 Open

1. A function value with an environment in a place with the result
   `void` stays rejected: `const cb2: (x: i32) => void = fv;`. `tsc`
   accepts it. The new environment needs an owner.
   *(Closed 2026-11-08 by §246.)*
2. An async function in a place with the result `void` gives the type
   mismatch diagnostic, not S013. The rejection is correct; the
   diagnostic does not name the dropped handle.
   *(Closed 2026-11-11 by §247.)*
3. A function with a result `T | undefined` keeps the C7 diagnostic
   "a function whose result is `T | undefined` may only be called" in
   a place with the result `void`. `tsc` accepts it. The lambda of rule
   2 does not read the result, so the C7 reason does not apply to it.
4. A conditional in the callback place of a built-in method is
   rejected: `[1].forEach(flag ? named : K.twice)`. `tsc` accepts it.
   The context of the place does not reach the branches there. As an
   argument of a user function, an initializer, or a return value, the
   same conditional is accepted.
   *(Closed 2026-11-07 by §242.)*
5. An array literal or a conditional with no context takes the type of
   its first function element, so `const fs = [named, nothing]` and
   `const f = flag ? nothing : named` are rejected. `tsc` accepts both.
   The pin gives the same result.
   *(Closed 2026-11-11 by §248.)*
6. `Worker.spawn(work)` with a value result keeps the exact-shape
   diagnostic of Q35. `tsc` accepts it.
7. A generic call with no type argument infers its type argument from
   the reference before any place check: `const viaId: CB = id(named);`
   and `const b: Box<CB> = new Box(named);` are rejected. `tsc` accepts
   both. With `id<CB>(named)` and `new Box<CB>(named)`, both are
   accepted.
8. An array literal as a spread source gets no element context:
   `const all: CB[] = [...[named]];` is rejected. `tsc` accepts it.
9. A static method referenced before its class declaration is accepted:
   `const early: CB = B.m;`. `tsc` gives TS2449. A static method with a
   `void` result gives the same result, so this is older than §240. It
   is a gap against invariant 5.
   *(Closed 2026-11-07 by §241.)*
