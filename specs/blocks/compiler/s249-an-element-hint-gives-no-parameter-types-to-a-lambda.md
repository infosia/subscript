<!-- §249 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 249. An element hint gives no parameter types to a lambda

*(Added 2026-11-11.)* Origin: §248.4 item 3, found by the Phase Review
of §248. The form breaks invariant 5: subscript accepts a program that
`tsc` rejects.

Problem, measured at `6d2dd8a5` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `one(x: i32): void` is a function.

| Form | subscript | `tsc` |
|---|---|---|
| `` const a1 = [one, (x) => print(`a1 ${x}`)]; `` | accepted, `a1[1](1)` prints `a1 1` | TS7006 "Parameter 'x' implicitly has an 'any' type" |
| `` const a2 = [(x: i32): void => {}, (x) => print(`a2 ${x}`)]; `` | accepted | TS7006 |
| `` const ns = [[one], [(x) => print(`ns ${x}`)]]; `` | accepted | TS7006 |
| `` const c1 = b ? one : (x) => print(`c1 ${x}`); `` | S100 "parameters require a type annotation" | TS7006 |
| `` const n1 = pick() ?? ((x) => print(`n1 ${x}`)); `` | accepted | accepts |

§212 rule 1a states that the type of an earlier element of an array
literal is a hint, not a contextual type. The checker still uses that
hint as the expected type of a later lambda element, so the lambda
takes its parameter types from a sibling. `tsc` gives an element of an
array literal with no contextual type no contextual type, so each
parameter with no annotation has the implicit type `any`, which
`strict` rejects.

### 249.1 Rules

1. A hint of §212 rule 1a is a kind of context of its own, not a
   contextual type. The checker carries the kind with the expected
   type. A lambda parameter never takes its type from a hint, at any
   depth: not as an element, a conditional branch, or an element of a
   nested array literal, and not through the result of a lambda or a
   field of a descriptor literal that the hint reaches. A context that
   a hint gives to an inner expression is a hint too. A lambda with a
   parameter that has no type annotation and only a hint gets the
   diagnostic that a lambda with no context gets: S100 "parameters
   require a type annotation".
1a. Only these forms read a hint: a literal; an operator that gives
   its context to its operands at the pin, such as an arithmetic or a
   unary operator, for its operands; a descriptor literal; an array
   literal; a conditional; a parenthesis; and a lambda, for its result
   only. So `[a, (1 + 2) * 3]` with `a: f32` keeps the type `f32[]`. Each other form reads a hint as
   no context. So a call, a `new`, and `Array.of` take no contextual
   candidate from a hint, at any depth. The checker decides this at
   one place, so a form that it does not list reads no hint.
2. A contextual type from outside the list keeps its rule: an
   annotation, a parameter type, a return type, an assignment target,
   and the right operand of `??` (§212 rule 1a). So
   `const fs: CB[] = [one, (x) => …]` stays accepted.
2a. An argument of a generic call takes as its contextual type the
   type of its parameter, with each type parameter that the other
   arguments fix replaced by its inferred type, as an explicit type
   argument gives it. A lambda that needs that context is checked after
   the arguments that fix the type parameter. So
   `two(one, [one, (x) => …])` with `two<T>(a: T, xs: T[]): T[]` is
   accepted, as `two<CB>(one, [one, (x) => …])` is. `tsc` accepts both.
3. A hint keeps its other uses: a literal element and the selection of
   §248 do not change.
4. No program that `tsc` accepts changes from accepted to rejected.
   Each program that changes is rejected by `tsc`.

### 249.2 Acceptance

1. Red first, at the pin: the reject entry
   `r497-an-element-hint-gives-no-parameter-types-to-a-lambda` is
   accepted. The tracking note records the output. Its header states
   what `tsc` does, measured.
2. After the change `r497` gives the S100 of rule 1 at each lambda.
3. A unit test pins rules 1 to 3: each table form, a nested list, and
   the controls of rule 2 (an annotated list, a parameter place, `??`).
4. The accept entry
   `a434-a-generic-argument-takes-the-type-that-other-arguments-fix`
   holds the forms of rule 2a. It passes `tsc`, carries `js-comparable`,
   and equals `node`.
5. If a corpus entry or golden changes, report it and stop. No golden
   moves.

### 249.3 Phase Review

One fresh review of the implementation. No CRITICAL finding.

1. MAJOR, realistic, fixed: the hint still gave parameter types through
   the result of a lambda element and through a member of a descriptor
   literal. A mark on the syntax of the element could not close the
   class. The expected type now carries its kind, a contextual type or
   a hint, and a lambda takes parameter types from a contextual type
   only.
2. MAJOR, realistic, fixed: a generic call where another argument fixes
   the type parameter, such as `two(one, [one, (x) => …])`, was rejected
   after the change, and `tsc` accepts it. Rule 2a: an argument that
   holds such a lambda is checked after the other arguments, with the
   type parameters that they fix.
3. MINOR, contrived: a mark stayed when a lambda had an error context.
   The marks are gone.
4. MINOR, recorded: §249.4 items 1 and 2.

A second review of the implementation, after the change of the form. No
CRITICAL finding.

1. MAJOR, realistic, fixed: a hint still reached lambda parameters
   through the inference of a generic call, a `new`, and `Array.of`
   from their contextual result, such as
   `[mk, () => wrap((x) => …)]`. Rule 1a: one place decides which forms
   read a hint, and each other form reads a hint as no context. The
   mark of a call operand at an element is gone.
2. MINOR, fixed: rule 2a fixed a type parameter from its first
   candidate, so the order of the other arguments decided acceptance.
   It now uses the inferred type of the candidates. §249.4 item 3.
3. MINOR, fixed: a test name stated that a divergence stays, and the
   test pins three accepted forms.
4. MINOR, contrived, recorded: §249.4 item 4.
5. MINOR, older, recorded: §249.4 item 5.

After the second review, a measurement against the pin CLI found a
rule 4 break: an operator with literal operands in a context-free list
after a typed element, such as `[a, 1 + 2]` with `a: f32`, gave S007.
Rule 1a now names the operators. The tracking note compares each form
that reads a context at the pin.

### 249.4 Open

1. `[one, (x) => …] as CB[]` gives the S100 of rule 1 beside the
   rejection of `as` that it had at the pin. The program is rejected in
   both.
2. `((x) => …) as CB` gives the S100 of rule 1 alone. `tsc` accepts it,
   because `as` gives a context to its operand there. `as` gives no
   context here.
3. A nullable function type gives a lambda no parameter types.
   `const f: CB | null = (x) => …` gives the S100 of rule 1, and `tsc`
   accepts it. So `three(one, maybe(), (x) => …)` with
   `maybe(): CB | null` and `three<T>(a: T, b: T, c: T): T` gives the
   S100 in each order of the first two arguments. `tsc` accepts both.
   *(Closed 2026-11-12 by §251.)*
4. Rule 2a checks an argument that holds a lambda after the other
   arguments. So a state that flows from one argument to the next, such
   as narrowing, sees the other order. No realistic program was found
   where this changes acceptance.
5. The S100 at a lambda in a generic call that a hint reaches carries
   the divergence block "TypeScript accepts: each([1, 2], (x) => {})".
   `tsc` rejects those forms with TS7006, so the block is wrong there.
