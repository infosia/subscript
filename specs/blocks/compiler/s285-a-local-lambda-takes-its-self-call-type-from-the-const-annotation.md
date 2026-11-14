<!-- §285 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 285. A local lambda takes its self-call type from the `const` annotation

*(Added 2026-11-14.)* Origin: §284.4 item 4. Under the owner's standing
approval of 2026-11-06 for `tsc` divergences with a known fix.

Problem, measured at `5fba0d52` with the debug CLI. `tsc` accepts each
form.

| Form | subscript |
|---|---|
| `type F = (k: i32) => i32;` and, in a function body, `const fact: F = (k) => { if (k <= 1) return 1; return k * fact(k - 1); };` | S100 "`fact` is read before its declaration in this block" |
| the same at module level | accepts, prints `120` |
| in a function body, `const fact: (k: i32) => i32 = (k) => k <= 1 ? 1 : k * fact(k - 1);` | S100, the same |

§284 rule 2 takes a self-call only when the type of the lambda is
decided before its body is checked by a result annotation of the lambda
or by §238 rule 1. A function type annotation on the `const` also
decides each parameter type and the result type before the body is
checked, but rule 2 does not count it.

### 285.1 Rules

1. §284 rule 2 also applies when the `const` that holds the lambda has
   a function type annotation, and that annotation gives the lambda its
   parameter types and its result type. The body form does not matter:
   a block body and an expression body both take rule 1 of §284.
2. The type of the lambda is the type that the annotation gives it, as
   for the same form with no self-call. A body whose result does not
   convert to the annotated result type is rejected as it is today.
3. Each other rule of §284 holds unchanged.
4. A self-call, by §284 rule 1 or by rule 1 here, is taken only when
   the `const` has no type annotation, or its declared type is a
   function type, not a nullable or union type that holds one, and the
   lambda has the same parameter count as that function type. In each
   other case the self-call stays rejected with the S100 of the table.
   (`tsc` rejects an unguarded call of a `const` typed `F | null`; a
   lambda with fewer parameters than its annotation gives a self-call
   whose argument count does not match the function that the `const`
   holds.)

### 285.2 Acceptance

1. Red first, at the contract commit: the accept entry
   `a450-a-local-lambda-takes-its-self-call-type-from-the-const-annotation`
   is rejected with the S100 of the table. The tracking note records
   the output.
2. `a450` passes `tsc`, carries `js-comparable: yes`, and its output
   under `node` equals its golden. It holds both rows of the table in a
   function body, an `async` lambda typed by a `const` annotation that
   awaits its self-call, and a lambda that returns a string.
3. A unit test shows that a lambda whose body returns a value that does
   not convert to the annotated result keeps its present diagnostic
   when it also calls itself.
4. Unit tests pin rule 4: a `const` typed `((k: i32) => i32) | null`
   and a `const` typed `(k: i32, d: i32) => i32` with a one-parameter
   lambda each give the S100 of the table for a self-call. Each has a
   control with no self-call that the checker accepts.
5. The full gate passes. If `a450` has an `async` entry, the LIR text
   snapshot grows by its functions. No other golden moves.

### 285.3 Phase Review

Pending.

### 285.4 Open

None.
