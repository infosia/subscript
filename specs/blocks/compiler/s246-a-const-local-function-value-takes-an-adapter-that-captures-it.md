<!-- §246 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 246. A const local function value takes an adapter that captures it

*(Added 2026-11-08.)* Origin: §240.4 item 1 and §243.4 item 1. The
owner selected this form on 2026-11-08: the adapter captures the value
as a lambda written in the source does, and no new ownership rule is
added.

Problem, measured at `ab3b0aca` with the debug CLI. `CB1` is
`(x: i32) => void`, `CB2` is `(x: i32, y: i32) => void`, `run(cb: CB2)`
calls `cb(1, 2)`, and `fv` is the const local
`const fv = (x: i32): void => { print(`fv ${x + k}`); }` with a const
local `k`. `tsc` accepts each form of the table.

| Form | subscript |
|---|---|
| `const h: CB2 = fv;` | S100 "type mismatch: the initializer expects `(i32, i32) => void`, got `(i32) => void`" |
| `run(fv);` | S100, the argument kind |
| `const h: CB1 = fr;` with `const fr = (x: i32): i32 => …` | S100 "the initializer expects `(i32) => void`, got `(i32) => i32`" |
| `m.forEach(fv);` on a `Map<string, i32>` | S014 "`Map.forEach` callback `fv` is a function value with fewer parameters than the place; an adapter for it needs a new environment, which no rule gives an owner; use a lambda that calls it" |
| `run(G);` with a module const `const G: CB1 = (x: i32): void => …` | S100, the argument kind |

The lambda that calls the value is accepted today in each of these
places: `const h: CB2 = (x: i32, y: i32): void => fv(x);`, also across
an `await`. It captures `fv` by value (S009, C5). Its environment has
the owner that each capturing lambda has (§68): it stays in the
defining function. A function value has no identity comparison
(`===` on a function type is S100), so a program cannot tell the
adapter from the value.

### 246.1 Rules

1. In each place of §240 rule 2, §243 rules 1 and 2, and §244 rule 1,
   a reference to a const local of a function type takes the adapter of
   that place when the value with that adapter fits the place. The
   checker records in the HIR, in place of the reference, the lambda
   that a program can write there: the parameters of the place, a body
   that calls the local with its own number of arguments, and the
   result rule of §240 rule 2 and §242 rule 5. The lambda captures the
   local by value, as S009 allows.
2. A reference to a module-level `const` of a function type takes the
   same adapter. That lambda reads the constant and captures nothing,
   as the lambda of §240 rule 2 does.
3. Each rule that applies to a capturing lambda applies to the lambda
   of rule 1: S009 at each place where the value can leave its defining
   function (a return, a field, an array literal, a clean argument), and
   §175. A form that S009 rejects for the lambda written in the source
   is rejected for the adapter. The adapter adds no form that a capture
   does not have.
4. These keep a rejection: a `let` local, a parameter, a field read, an
   element read, a call result, and any other expression that is not a
   name of rules 1 and 2. An async function value, a generic function,
   and a function with a default parameter keep their present
   diagnostic, as in §243 rule 4.
5. A value of rule 4 that fits the place with the adapter gets a
   diagnostic that names the cause: only a const local or a module
   `const` takes the adapter, so copy the value into a const local, or
   write the lambda that calls it. This holds in each place of rule 1,
   built-in callbacks included (§244 rule 4a). A value that does not fit
   with the adapter keeps the type mismatch diagnostic of the place.
6. A conditional follows §242 rule 1a and §244 rule 2: a leaf that is a
   name of rules 1 and 2 takes the place of that leaf as a function
   declaration does.
7. No accepted program changes its meaning. The adapter has the cost of
   the lambda that the program can write in its place. A built-in
   callback keeps its inline lowering (§235 rule 5c).
8. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 246.2 Acceptance

1. Red first, at the pin: the accept entry
   `a432-a-const-local-function-value-takes-an-adapter-that-captures-it`
   is rejected. The tracking note records the output.
2. `a432` passes `tsc`. Its output under `node` equals its golden. It
   has each form of the table, a const local that captures nothing, an
   adapter in an `async` function across an `await`, and an adapter in
   a conditional leaf. Each call prints, so the number of calls and the
   captured value are visible. If `main` is async, the LIR text
   snapshot grows by the `a432` block only.
3. The reject entry `r495-a-parameter-function-value-takes-no-adapter`
   has a parameter in a place of rule 1 and gives the diagnostic of
   rule 5. Its header states what `tsc` does, measured.
4. A unit test pins rules 1 to 6. It has each form of rule 4 and the
   escape forms of rule 3 (rejected) as controls.
5. The witness `s244-environment` uses a const local today. It moves to
   a form of rule 4, and its text is the diagnostic of rule 5.
6. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves, except the LIR text snapshot by the
   `a432` block if `main` is async.
