<!-- §305 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 305. A conditional or spread element takes part in the list type

*(Added 2026-11-15.)* Origin: §248.4 items 1 and 2. On 2026-11-15 the
owner told Claude to continue with the open items. The standing approval
of 2026-11-06 covers a `tsc` divergence with a known fix.

Problem, measured at `43297a94` with `tsc` 5.9.2 and the debug CLI.
`none(): void`, `one(x: i32): void`, and `nv(x: i32): i32` are
functions, and `b` is a `boolean`. `tsc` accepts each form with no
error. The checker rejects each one.

| Form | Checker |
|---|---|
| `const hs = [none, b ? one : nv];` | S100 "the then branch expects `() => void`, got `(i32) => void`", and the same for the else branch |
| `const hs = [none, b ? one : one];` | the same two S100 |
| `const base = [one]; const hs = [none, ...base];` | S100 "the array element expects `() => void`, got `(i32) => void`" |
| `const base = [none]; const hs = [...base, one];` | the same S100 |

§248 rule 1 selects the type of a context-free list of functions from
its elements. A conditional element takes the first element as its
context (§212 rule 1a), so its branches fail before the selection. A
spread element does not take part in the selection.

### 305.1 Rules

1. In a context-free array literal under §248 rule 1, a conditional
   element whose branches are each a value of a function type is a
   candidate. Its type is the type that §248 rule 2 selects for it
   with no context. If §248 rule 2 selects no type, the element is not
   a candidate, and it keeps its present diagnostics.
2. A spread element whose source is an array of a function type is a
   candidate. Its type is the element type of the source.
3. The list takes the type of the first candidate that each other
   element fits (§248 rule 1). A conditional element then takes the
   selected type as its place, as in §242 rule 1a: each branch takes
   its adapter. A spread element with another element type copies each
   element through its adapter, as an element in a place of that type.
4. If an element is not a candidate, the list keeps the rule at the pin.
5. No accepted program changes its meaning or its type.
6. Before this change, `check_cond` and its helpers move from
   `compiler/src/check/expr/operator.rs` into a module of their own
   (§248.3 item 4).

### 305.2 Acceptance

1. Red first, at the pin: the accept entry `a461`, `js-comparable: yes`,
   with each form of the table in its own function, is rejected. It
   calls each element so that the adapters are visible. Its golden is
   the `node` output.
2. Unit tests for rules 1 to 4, each with a control in the same shape
   that stays rejected: a conditional element whose branches have no
   common type (`b ? one : word` with `word(s: string): void`), and a
   spread source whose element type no other element fits.
3. No `.expected` golden moves. No accepted program becomes rejected.
4. The development tier, the ship tier, and the interpreter give the
   same output.

### 305.3 Phase Review

### 305.4 Open
