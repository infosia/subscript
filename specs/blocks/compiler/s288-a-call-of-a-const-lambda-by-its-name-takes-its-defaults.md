<!-- §288 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 288. A call of a `const` lambda by its name takes its defaults

*(Added 2026-11-14.)* Origin: §284.4 item 3. The owner selected the item
on 2026-11-14.

Problem, measured at `091e158d` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `tsc` accepts each form.

| Form | subscript |
|---|---|
| `function named(k: i32, s: string = "n"): void { … }` and `named(1);` | accepts |
| module scope `const top = (k: i32, s: string = "t"): void => { … };` and `top(2);` | S100 "`top` expects 2 argument(s), got 1: a call through a function value passes every parameter" |
| in a function body, `const loc = (k: i32, s: string = "d"): void => { …; if (k > 0) loc(k - 1); };` and `loc(3);` | S100, the same, at the self-call and at `loc(3)` |

The divergence `FunctionValueOptionalArgumentCount` states its reason: "A
function type has no optional parameter (C7)." A call through a value
of a function type cannot know the defaults. A call of a `const` by its
name, where the `const` holds a lambda literal, knows the lambda and its
defaults, as a call of a function declaration does. The reason does not
hold for that call.

### 288.1 Rules

1. A *named lambda call* is a call whose callee is the name of a
   `const`, at module scope or in a function body, whose initializer is
   a lambda literal, and the `const` has no type annotation. A
   self-call of §284 is a named lambda call.
2. A named lambda call can omit each argument from the first parameter
   with a default to the end. Each omitted argument takes its default,
   as for a call of a function declaration (§57.1): left to right, and a
   default reads the parameters before it.
3. A default of a lambda that a named lambda call can omit reads only
   the parameters before it and the names that a default of a function
   declaration at the same place can read. A default that reads a local
   that the lambda captures gives S100 at the default, with a message
   that names the captured local, when a named lambda call omits it.
   The caller evaluates the default outside the lambda environment, so
   the captured local is not in scope there. *(`tsc` accepts this
   form. It is recorded in 288.4.)*
3a. A default that a named lambda call evaluates is part of the call
   for each check that reads the evaluation order of the module, as a
   default of a function declaration is (the module initialization
   order). So a default that reads a module `const` before its
   declaration gives the S100 that the same default of a function
   declaration gives. *(Amended 2026-11-14 after the Phase Review,
   finding 1.)*
4. Each other call of a function value keeps
   `FunctionValueOptionalArgumentCount`: a call through a parameter,
   through a `let`, through a `const` with a function type annotation,
   through an element or a field, and through a `const` that holds
   another `const`. The divergence example of the site changes to a call
   through a `let`, which the rule still rejects.
5. A call that passes every argument keeps its present form and output.
6. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 288.2 Acceptance

1. Red first, at the contract commit: the accept entry
   `a453-a-call-of-a-const-lambda-by-its-name-takes-its-defaults` is
   rejected with the S100 of the table. The tracking note records the
   output.
2. `a453` passes `tsc`, carries `js-comparable: yes`, and its output
   under `node` equals its golden. It holds the module-scope and local
   rows of the table, a self-call that omits an argument, a default
   that reads an earlier parameter, a default with a side effect that
   runs once per omitted argument and not for a passed argument, and
   an async lambda with a default that awaits its self-call.
3. Unit tests pin rule 3 (one S100 that names the captured local, and a
   control where the call passes that argument) and rule 4 (a call
   through a parameter and through an annotated `const` each keep the
   S100 of the table).
4. The rejection-site total check passes. Each site keeps a witness of
   a form that it still rejects.
5. The full gate passes. `a453` has an async lambda, so the LIR text
   snapshot can grow by its functions. No other golden moves.

### 288.3 Phase Review

Pending.

### 288.4 Open

1. Rule 3: a default of a lambda that reads a captured local is
   rejected when a named lambda call omits it. `tsc` accepts it.
