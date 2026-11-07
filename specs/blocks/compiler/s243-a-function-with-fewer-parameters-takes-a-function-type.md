<!-- §243 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 243. A function with fewer parameters takes a function type

*(Added 2026-11-07.)* Origin: §242.4 item 1. Under the owner's standing
approval of 2026-11-06 for `tsc` divergences with a known fix.

Problem, measured at `93daadf4` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI.
`eachi(xs: i32[], f: (x: i32, i: i32) => void): void` is a function,
`one(x: i32): void` and `twice(x: i32): i32` are functions,
`K.one(x: i32): void` is a static method, `CB2 = (x: i32, i: i32) => void`,
`F2 = (x: i32, i: i32) => i32`, and `n` is a local.

| Form | subscript | `tsc` |
|---|---|---|
| `eachi([1], (x: i32): void => print(`l${x}`));` | S100 "type mismatch: the argument expects `(i32, i32) => void`, got `(i32) => void`" | accepts |
| `eachi([2], (x) => print(`u${x}`));` | S100, the same | accepts |
| `eachi([3], () => print("z"));` | S100 "… got `() => void`" | accepts |
| `eachi([4], one);` | S100, the argument kind | accepts |
| `eachi([5], K.one);` | S100, the argument kind | accepts |
| `eachi([1], twice);` | S100 "… got `(i32) => i32`" | accepts |
| `const f: CB2 = one;` | S100 "type mismatch: the initializer expects `(i32, i32) => void`, got `(i32) => void`" | accepts |
| `const t: F2 = twice;` | S100, the initializer kind | accepts |
| `const r: F2 = (x: i32): i32 => x + 1;` | S100, the initializer kind | accepts |
| `const g: CB2 = (x: i32): void => print(`${x + n}`);` | S100, the initializer kind | accepts |
| `const h: CB2 = fv;` with a local `fv: (x: i32) => void` | S100, the initializer kind | accepts |

§235 rule 3 requires the same number of parameters. A caller through
the type `(x: i32, i: i32) => void` passes two arguments, and a function
of one parameter reads one. The representation of a function value has
no adapter for that, so the rule held for every function value.

The reason does not hold for two kinds of value:

- A lambda literal is made at its place. It can take the parameters of
  the place: the extra trailing parameters have the types of the place,
  and the body does not read them. Its environment does not change.
- A function declaration or a static method has no environment. A
  lambda with no environment and the parameters of the place calls it
  with its own number of arguments. That is the form of §240 rule 2.

A function value with an environment, such as the local `fv`, needs a
new environment that holds the original value, and no rule of the
language gives that environment an owner (§240.4 item 1). It stays
rejected.

### 243.1 Rules

1. A lambda literal with `N` parameters, in a place whose type is a
   function type `G` with `M > N` parameters, is accepted when the
   lambda with the `M - N` trailing parameters of `G` added fits `G`
   (§235 rule 2). The lambda then has those `M` parameters. The added
   parameters have no name in the source, and the body does not read
   them. A parameter with no type annotation takes the type of `G` at
   its position, as it does today when `N = M`.
2. A reference to a function declaration or to a static method with
   `N` parameters, in a place whose type is a function type `G` with
   `M > N` parameters, is accepted when the function is not generic, not
   async, and not a generator, and when the function with the `M - N`
   trailing parameters of `G` added fits `G`. The checker records in
   the HIR, in place of the reference, a lambda with no environment and
   the `M` parameters of `G`. It calls the function with its first `N`
   parameters. If the result of `G` is `void` and the function has a
   value result, the lambda discards the result, as in §240 rule 2, and
   §242 rule 5 applies to that result.
3. The places of rules 1 and 2 are the places of §240 rule 2, except
   the callback of a built-in method. A built-in callback keeps its
   arity rules (Q22, Q24, Q27).
4. A function value with an environment, a generic function, an async
   function, and a function with a default parameter keep their present
   diagnostic in such a place (§164.4 item 1).
5. Each value check of rules 1 and 2 is the value-taking check of §240
   rule 2a.
6. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 243.2 Acceptance

1. Red first, at the pin: the accept entry
   `a429-function-with-fewer-parameters-takes-a-function-type` is
   rejected. The tracking note records the output.
2. `a429` passes `tsc`. Its output under `node` equals its golden. It
   has each form of the table except `const h: CB2 = fv;`. Each callback
   prints, so the order of calls and the argument values are visible.
3. A unit test pins rules 1, 2, and 4. It has the HIR lambda of rule 2,
   the parameters of a lambda of rule 1, and each form of rule 4
   (rejected) as a control.
4. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves.
