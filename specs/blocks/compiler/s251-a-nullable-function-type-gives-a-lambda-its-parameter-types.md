<!-- §251 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 251. A nullable function type gives a lambda its parameter types

*(Added 2026-11-12.)* Origin: §249.4 item 3. Under the owner's standing
approval of 2026-11-06, selected by the owner on 2026-11-12.

Problem, measured at `25da2e75` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `CB` is `(x: i32) => void`, `one(x: i32)`
has the result `void`, `maybe(): CB | null` returns `one`, and
`three<T>(a: T, b: T, c: T): T` returns `c`. `tsc` accepts each form,
with no error.

| Form | subscript |
|---|---|
| `` const f: CB \| null = (x) => print(`f ${x}`); `` | S100 "parameters require a type annotation" |
| `` g = (x) => print(`g ${x}`); `` with `let g: CB \| null` | S100, the same |
| `` function retn(): CB \| null { return (x) => print(`r ${x}`); } `` | S100, the same |
| `` take((x) => print(`t ${x}`)); `` with `take(f: CB \| null)` | S100, the same |
| `` three(one, maybe(), (x) => print(`h ${x}`)); `` | S100, the same |
| `` three(maybe(), one, (x) => print(`k ${x}`)); `` | S100, the same |
| `` const c: CB \| null = b ? (x) => print(`c ${x}`) : null; `` | S100, the same |
| `` h.cb = (x) => print(`h ${x}`); `` with the field `cb: CB \| null` | S100, the same |
| `` const xs: (CB \| null)[] = [(x) => print(`xs ${x}`), null]; `` | S100, the same |
| `` m.set("k", (x) => print(`m ${x}`)); `` with `Map<string, CB \| null>` | S100, the same |

The same places with an annotated lambda are accepted, and the
adapters of §240 apply there: `const a: CB | null = (x: i32) => x + 1;`
and `const b: CB | null = nv;` with `nv(x: i32): i32` run. A lambda
takes its parameter types and its result context only from a context
that is a function type. `T | null` is not one, so the lambda gets no
context. `tsc` takes the contextual signature from the function member
of the union.

### 251.1 Rules

1. A contextual type `F | null`, where `F` is a function type, gives a
   lambda the parameter types and the result context that `F` gives.
   The lambda then has the type `F` in the place, as an annotated
   lambda has.
2. Rule 1 holds in each place that gives a lambda a contextual type
   (§212 rule 1a), and at each depth: an element of a list, a branch
   of a conditional, a parenthesized lambda, an argument after the
   inference of §249 rule 2a.
3. A hint of §249 rule 1 stays a hint. A nullable hint gives no
   parameter types.
4. An async lambda follows rule 1 with `F` the function type. Its
   result follows the present rule of a place with the type `F`
   (§238, §247).
5. No program that `tsc` rejects changes from rejected to accepted. No
   accepted program changes its meaning. No run-time cost.
6. The development tier, the ship tier, and the interpreter give the
   same output. The accept entry carries `js-comparable` and equals
   `node`.

### 251.2 Acceptance

1. Red first, at the pin: the accept entry
   `a436-a-nullable-function-type-gives-a-lambda-its-parameter-types`
   is rejected. The tracking note records the output.
2. `a436` passes `tsc`. Its output under `node` equals its golden. It
   has each form of the table, calls each lambda, and has one lambda
   with a result context: `const d: ((x: i32) => i32) | null = (x) => x * 2;`.
   Its `main` is not async.
3. A unit test pins rules 1 to 4, with the async form
   `const w: ((x: i32) => Promise<void>) | null = async (x) => { … };`,
   and with controls: a nullable hint in `[maybe(), (x) => …]` keeps
   the S100 of §249 rule 1 (`tsc` gives TS7006), and a lambda with no
   context keeps its S100.
4. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves.
