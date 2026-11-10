<!-- §247 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 247. A `void` result place names the handle that it drops

*(Added 2026-11-11.)* Origin: §240.4 item 2. Under the owner's standing
approval of 2026-11-06, selected by the owner on 2026-11-11.

Problem, measured at `2049e05b` with the debug CLI. `CB` is
`(x: i32) => void`, `each(xs: i32[], f: CB)` calls `f` on each element,
`work(x: i32): Promise<void>` and `val(x: i32): Promise<i32>` are
async. Each form is rejected, correctly: the place drops the handle, and
no holder awaits it (§70). The diagnostic differs by the form of the
value.

| Form | subscript |
|---|---|
| `each([1], work);` | S100 "type mismatch: the argument expects `(i32) => void`, got `(i32) => Promise<void>`", why C24 |
| `const f: CB = work;` | S100, the initializer kind |
| `each([1], val);` | S100, got `(i32) => Promise<i32>` |
| `each([1], async (x: i32): Promise<void> => { … });` | S100, the argument kind |
| `const w = work; each([1], w);` | S100, the argument kind |
| `each([1], (x: i32) => work(x));` | S013 "an async handle is dropped without any await of its completion", why §70 |
| `[1].forEach(work);` | S014 "`forEach` cannot carry a counted callback result (§171)" |
| `[1].forEach(async (x: i32): Promise<void> => {});` | S014, the same |

The why of C24 says that types need the same shape. That is not the
cause: §240 and §242 take a value result in a `void` place, and they do
not take a handle result, because the handle is then dropped. S013 names
that cause, and the lambda form already gets it.

### 247.1 Rules

1. In a place whose type is a function type with the result `void`, a
   value is rejected with S013 when the value fits the place with the
   adapter of §240 rule 2, §243, or §246, except that its result holds
   an async handle (the predicate of §242 rule 5). The value is a
   function name, a static method, a lambda literal (async or not), a
   name of §246 rules 1 and 2, or a value of §246 rule 4. The places are
   those of §240 rule 2, built-in callbacks included.
2. The message names the value, the handle type, and the place type. It
   states that no holder awaits the handle, and that a place with the
   handle result can await it. The why is that of the present S013
   (§70).
3. A conditional follows §242 rule 1a: the leaf that holds the handle
   gets the diagnostic of rule 1.
4. A value whose parameters do not fit the place keeps the type
   mismatch. A counted result that is not an async handle keeps its
   present rule: §242 in a place with the result `void`, and §171 in a
   callback place that reads the result (`map`, `filter`, `reduce`,
   `Map.groupBy`).
5. No program changes from rejected to accepted, or the reverse. Only
   the diagnostic changes.

### 247.2 Acceptance

1. Red first, at the pin: the reject entry
   `r496-a-void-result-place-names-the-handle-it-drops` gives the S100
   of the table. The tracking note records the output. Its header
   states what `tsc` does, measured.
2. After the change `r496` gives the S013 of rule 2.
3. A unit test pins each form of the table, a conditional leaf, and the
   controls of rule 4.
4. The witnesses and divergence entries move with the site. If a corpus
   entry or golden changes, report it and stop. No golden moves.
