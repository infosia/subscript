<!-- §207 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 207. A static method and a local function are function values

*(Added 2026-10-28.)* Origin: on 2026-10-27 the owner asked for the
divergences from TypeScript that are cheap to remove. On 2026-10-28
the owner selected it as the last section of the second group. The
measurement is in `specs/tracking/static-and-local-function-values.md`.

Problem, measured at the pin `20534f71` (`tsc` 5.9.2 accepts each row, and
`node` runs it):

| Form | `subscript check` |
|---|---|
| `apply(M.twice, 4)`, `const g = M.twice;` | S100 "static method `M.twice` may only be called" |
| `function sq(x: i32): i32 { … }` in a function body | S100 "nested declarations are not in the decided surface" |

`collisions.md` C24 row 13 gives "No first-class value is decided for
a direct call target" for a static method, and row 5 gives "No
lowering is decided for a local declaration". A module function is
already a function value: a function pointer (C5). A static method has no `this` and
lowers to a plain C function, as a module function does. A local
function that reads no local of its enclosing function is a module
function with a name that only its block can see.

### 207.1 Rules

1. **A static method as a value.** `C.m`, where `m` is a static
   method of `C`, is a function value, as a module function is (C5).
   Its type is the method signature. The rules of a module function
   value apply: a form that is accepted or rejected for a module
   function value is accepted or rejected for a static method value. A static
   method of a generic class, a generic static method, and a static
   generator method stay rejected with the existing text.
2. **A local function.** A `function` declaration in a function body,
   a method body, a lambda body, or a block is a local function. If it
   reads no local, parameter, or `this` of an enclosing function, it is
   a module function with a hidden name. Its name is visible in the
   block that declares it, in the whole block, as in JavaScript: a call
   before the declaration is legal. It can call itself and another
   local function of the same block.
2a. A lifted local function keeps every fact of the place that
   declares it that a check of its body reads: the class body that
   encloses it, for `private` and `#x` member access, and the scope of
   its block, for the duplicate-declaration rule. Two local functions
   with one name in one block, or a local function and a local with one
   name, fail with S017 "duplicate declaration of `f` in one scope", as
   `tsc` TS2393 and TS2300 do. *(Added 2026-10-28 after the Phase
   Review, M1 and M2.)*
3. A local function is a function value, as a module function is. Its C name
   cannot equal the C name of any source declaration, and two local
   functions with one name in different blocks have different C names.
4. A local function that reads a local, a parameter, or `this` of an
   enclosing function fails with S100 "local function `f` reads the
   local `k` of its enclosing function". The text names the first such
   read. A type parameter of an enclosing function or class, and a call
   of a local function that stays in its block, count as reads. A write
   to a local counts too, and the text says "writes" for it. In a
   module-level block, the text says "of its enclosing block". A name
   that a rejected local declaration (rule 5) binds gives no second
   diagnostic.
   *(Amended 2026-10-28: the coding agent measured both cases.)* The diagnostic carries a "TypeScript accepts" block. The
   replacement is a `const` lambda.
5. A local class, enum, type alias, interface, and namespace stay
   rejected with the existing text (C24 row 5).
6. The forms have no run-time cost: a value of rule 1 or rule 3 is
   the function pointer of a module function (C5).

### 207.2 Acceptance

1. Red first, at the pin `20534f71`: the accept entry
   `a392-static-method-and-local-function-values` is rejected. The
   reject entry `r453-local-function-reads-a-local` fails its expected
   error at the pin. The tracking note records each result.
2. `a392` passes `tsc`, and its output under `node` equals its golden.
   It uses a static method as an argument, a local, and an array
   element; a static method that reads a static field after a write;
   a call before the declaration; self recursion and mutual recursion;
   a local function returned as a value; and a local function in a
   generator.
3. A unit test pins rule 3: two local functions with one name in two
   functions give two C names, and neither equals a module function
   with that name.
4. The rejection sites and divergence entries that only these forms
   reached are removed or changed. The rejection-site total check
   passes.
5. `collisions.md` C24 rows 5 and 13 cite this section.
6. No golden moves. The LIR text snapshot grows by the `a392` section
   only, because `a392` has a generator.

### 207.3 Phase Review

*(Added 2026-10-28.)* A fresh review of `20534f71..88718091` gave two
MAJOR and four MINOR findings.

1. MAJOR, realistic, fixed: a lifted local function lost the class
   body that encloses it. In a static method, `function countOf(c:
   Counter) { return c.count; }` gave "member `count` is private in
   class `Counter`"; `tsc` accepts it, and `node` runs it. The lift
   removed each fact of the declaring place. Rule 2a now states what
   the lifted function keeps. `a393` pins it.
2. MAJOR, realistic, fixed: the lift moved a local function out of its
   block before the duplicate-declaration check. Two `function f` in
   one block were accepted; `tsc` gives TS2393. Rule 2a. `r454` pins
   it.
3. MINOR, fixed: a write to a local was reported as a read.
4. MINOR, fixed: a rejected local type alias or class gave a second
   diagnostic at the local function that names it.
5. MINOR, contrived, fixed: in a module-level block, the text named an
   enclosing function.
6. MINOR, fixed: a test name still described the old rejection.

Open:

1. `Worker.spawn` with a local function as its entry passes `check` and
   runs. The diagnostic for another entry form still says "must be a
   directly named module-level function". Measured 2026-10-31.
