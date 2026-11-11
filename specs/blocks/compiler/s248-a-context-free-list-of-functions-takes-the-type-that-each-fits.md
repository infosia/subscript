<!-- §248 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 248. A context-free list of functions takes the type that each fits

*(Added 2026-11-11.)* Origin: §240.4 item 5. Under the owner's standing
approval of 2026-11-06, selected by the owner on 2026-11-11.

Problem, measured at `d25eb42e` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `none(): void`, `one(x: i32): void`, and
`nv(x: i32): i32` are functions, and `b` is a `boolean`. `tsc` accepts
each form, with no error.

| Form | subscript |
|---|---|
| `const fs = [none, one];` | S100 "type mismatch: the array element expects `() => void`, got `(i32) => void`" |
| `const f = b ? none : one;` | S100 "conditional branches have no common type: `() => void` and `(i32) => void`" |
| `const g = b ? one : nv;` | S100 "conditional branches have no common type: `(i32) => void` and `(i32) => i32`" |
| `const fs = [one, none];` | accepted |

With no context, an array literal takes the type of its first element,
and the other elements are checked against it. So the order of the
elements decides acceptance. A conditional with no context needs the
same type in both branches. `tsc` takes the type that each other
candidate fits, so the order does not matter.

### 248.1 Rules

1. An array literal with no context, whose elements are each a value of
   a function type, takes the type of the first element that each
   other element fits. "Fits" is the value-taking check of §240 rule 2a
   with the adapters of §240, §243, and §246. If no element is such a
   type, each element that does not fit gets its present diagnostic,
   as at the pin.
2. A conditional with no context, whose branches are each a value of a
   function type, takes the type of the first branch that the other
   branch fits, by the check of rule 1. The other branch then takes that
   type as its place, as in §242 rule 1a.
3. Each element or branch that does not have the selected type takes
   its adapter, as in a place with that type.
4. A list of values that are not of a function type keeps its present
   rule. An element with a handle result follows §247.
5. No accepted program changes its meaning. An accepted list keeps its
   type: the first element that each other element fits was already
   the type of each accepted list.
6. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 248.2 Acceptance

1. Red first, at the pin: the accept entry
   `a433-a-context-free-list-of-functions-takes-the-type-that-each-fits`
   is rejected. The tracking note records the output.
2. `a433` passes `tsc`. Its output under `node` equals its golden. It
   has each form of the table, and calls each element and branch so
   that the adapters are visible.
3. A unit test pins rules 1 to 4, with a control where no element fits
   each other element: `[one, word]` with `word(s: string): void` stays
   rejected. `tsc` accepts it with a union element type; a union of
   function types has no form here (`collisions.md` C7).
4. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves.
