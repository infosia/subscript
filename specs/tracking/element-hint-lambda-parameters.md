# An element hint gives no parameter types to a lambda — `compiler.md` §249

Status: IN PROGRESS.

## Red at the pin

The debug CLI built from `git archive 6b99f9f5` in `$TMPDIR`, run on a
`$TMPDIR` copy:

`subscript check r497-an-element-hint-gives-no-parameter-types-to-a-lambda.ts`
gave "no errors". `subscript run` printed `a1 1`, `a2 2`, and `ns 3`,
one per line. The CLI of the code of `6d2dd8a5`, the same code, gave
"no errors" for the `r497` that also holds the forms of rule 1a in
`more()`.

`subscript check a434-a-generic-argument-takes-the-type-that-other-arguments-fix.ts`
gave 3 S100 "parameters require a type annotation", at 42:32
(`firstOr(handlers, (x) => x + 1)`), 44:43
(`pair((x: i32): i32 => x * 2, (x) => x + 100)`), and 46:19
(`pair((x) => x + 1000, (x: i32): i32 => x * 3)`). The `two` and `twoR`
forms of `a434` were accepted at the pin.

## After the change

`a434` checks with no errors. `tsc` 5.9.2 accepts it. The development
tier, the ship tier, the interpreter, and `node` give the same bytes:
`one 1 two 2 one 3 twoR 4 two 5 firstOr 7 pair 107 pair 24`, one per
line. `main` is not async, so the LIR text snapshot does not change.

`r497` gives eight S100 "parameters require a type annotation", at
13:21, 14:38, 15:24, 43:31, 44:40, 46:28, 47:35, and 48:32, one at each
lambda. `tsc` 5.9.2 gives TS7006 "Parameter 'x' implicitly has an
'any' type" at the same eight positions. The site `NamedParameterAnnotationMissing` is a `tsc`
rejection, so the diagnostic carries no divergence block.

Each form that changes from accepted to S100, measured with the CLI of
the pin and the CLI after the change. `tsc` 5.9.2 gives TS7006 on each:

- `[one, (x) => …]`
- `[(x: i32): void => {}, (x) => …]`
- `[[one], [(x) => …]]`
- `[one, b ? one : (x) => …]`
- `[(): CB => one, () => (x) => …]`
- `[() => one, () => (x) => …]`
- `[() => one, () => { return (x) => …; }]`
- `[(y: i32) => one, (y: i32) => (x) => …]`
- `[mkd(), { f: (x) => … }]` and `[[mkd()], [{ f: (x) => … }]]`, with
  `@Descriptor class D { f!: CB; }` and `mkd(): D`
- rule 1a, with `mk(): CB`, `wrap<T>(f: T): T`, `class Box<T>` with a
  field `f: T` and a constructor `(f: T)`, and `mkb(): Box<CB>`:
  `[mk, () => wrap((x) => …)]`,
  `[mk, () => { return wrap((x) => …); }]`,
  `[b1, new Box((x) => …)]` with `b1 = new Box<CB>(one)`,
  `[mkb, () => new Box((x) => …)]`, and `[[one], Array.of((x) => …)]`

Each form that changes from rejected to accepted. `tsc` 5.9.2 accepts
each:

- `firstOr(handlers, (x) => x + 1)` with
  `firstOr<T>(xs: T[], fallback: T): T` and a list of `(x: i32) => i32`
- `pair((x: i32): i32 => x, (x) => x + 1)` and
  `pair((x) => x + 1, (x: i32): i32 => x)` with `pair<T>(a: T, b: T): T`

The unit test `initializer_inference.rs`
`generic_function_value_candidates_keep_the_parameter_divergence` had
the first two of these as rejected forms with the divergence
`GenericCallbackParameterAnnotationNeeded`. It now accepts them, and
keeps the divergence for `pair((x) => x + 1, (x) => x + 1)`, where no
argument fixes `T`. `tsc` rejects that form with TS7006. The why of that
divergence (`compiler/src/divergence/surface_forms.rs`) now states the
cause that remains: a type parameter that no non-literal argument fixes
gives a callback no parameter types.

Unchanged, measured the same way:

| Form | subscript | `tsc` |
|---|---|---|
| `[one, ...[(x) => …]]` | S100 | TS7006 |
| `b ? one : (x) => …` | S100 | TS7006 |
| `const fs: CB[] = [one, (x) => …]` | accepted | accepts |
| `const gs: CB[][] = [[one], [(x) => …]]` | accepted | accepts |
| `const ds: D[] = [mkd(), { f: (x) => … }]` | accepted | accepts |
| `const gs: (() => CB)[] = [() => one, () => (x) => …]` | accepted | accepts |
| `[() => one, (): CB => { return (x) => …; }]` | accepted | accepts |
| `hs = [one, (x) => …]` with `let hs: CB[]` | accepted | accepts |
| `take((x) => …)`, a `CB` result | accepted | accepts |
| `pick() ?? ((x) => …)`, alone and as an element | accepted | accepts |
| `two(one, [one, (x) => …])`, `twoR([one, (x) => …], one)` | accepted, run | accepts |
| `two<CB>(…)`, `twoR<CB>(…)` | accepted | accepts |
| `app(3, (x) => …)` with `app<T>(v: T, f: (x: T) => void)` | S100 | accepts |
| `[1.5 as f32, 2]`, `[1.5 as f32, -2]` | `f32[]` | accepts |
| `const w3: CB[] = [one, wrap((x) => …)]` | accepted | accepts |
| `[mk, (): CB => { return wrap((x) => …); }]` | accepted | accepts |
| `three(one, one, (x) => …)` | accepted | accepts |
| `three(one, maybe(), (x) => …)`, either order | S100 | accepts |

`two(one, [one, (x) => print(…)])` prints `two 1` for `r[1](1)`, and
`twoR([one, (x) => print(…)], one)` prints `twoR 2`, at the pin and
after the change. A literal argument such as `3` does not fix `T`
(§197 rule 3), so `app(3, (x) => …)` keeps the divergence of the pin.

## Form

- `compiler/src/check/expr/entry.rs`. `ContextKind` is the kind of an
  expected type: `Contextual` or `Hint`. `check_expr_as` checks an
  expression with an expected type of a kind. `check_expr` gives
  `Contextual`. The kind goes down through a parenthesis, and through
  each form that gives a context of its own to an inner expression:
  - an array literal gives its element context with its own kind. With
    no context, a later element takes `Hint` (§212 rule 1a), in
    `aggregate.rs`, with and without a spread;
  - a descriptor literal gives each member its own kind
    (`aggregate.rs`);
  - a conditional gives each branch its own kind. The sibling literal
    context is `Hint` (`conditional.rs`);
  - a lambda gives its result its own kind, unless an annotation or a
    rule of the body gives the result (`lambda.rs`). The frame keeps
    the kind (`Frame::result_kind`), and `return` reads it (`stmt.rs`).
- `check_lambda` takes parameter types from a `Contextual` type only.
- Rule 1a: `reads_hint` (`entry.rs`) names the forms that read a hint:
  a literal; a binary or a unary operator, which gives its context to
  its operands as it does at the pin; a descriptor literal; an array
  literal; a conditional; a parenthesis; and a lambda, for its result
  only. `check_expr_with_policy` gives each other form no context when
  the kind is `Hint`. So a call, a `new`, and `Array.of` take no
  contextual candidate from a hint. An operator marks its operand calls
  (`mark_operand_calls`, §212 rule 1a), so an operand call takes no
  contextual candidate either. The operand-call mark at the two hint
  sites of `aggregate.rs` is gone: the kind covers it.

## Rule 4: forms that read a context at the pin

The CLI of the code of `6d2dd8a5` (the code of the pin) and the CLI
after the change, on one probe per form. Each probe has
`const a: f32 = 1.5; const u: u8 = 1;` and the other locals that the
form names, and the form is the second element of a list with no
context. Each form gives the same result with both CLIs:

| Form | Result, both CLIs |
|---|---|
| `[a, 1 + 2]`, `[a, (1 + 2) * 3]`, `[a, 7 % 2]` | accepted, `f32[]` |
| `[a, 1 > 0 ? 2 + 1 : 3]`, `[a, !true ? 1 : 2]` | accepted, `f32[]` |
| `[u, ~1]`, `[1 as u8, 2 * 3]` | accepted, `u8[]` |
| `[u, 1 << 2]`, `[u, 1 & 3]` | S007, `i32` to `u8` |
| `[a, Math.abs(1)]` | S007, `f64` to `f32` |
| `[a, [1, 2].length]` | S007, `i32` to `f32` |
| template: ``[d, `x`]``, ``[a, `${1 + 2}`]`` with `d: Dir` | S100 type mismatch, got `string` |
| `as`: `[a, 2 as f32]`, `[u, (3 as u8) + 1]`, `[a, 1 as f32 + 2]` | accepted |
| assignment: `[a, w = 2]`, `[u, k = 3]`, `[a, w += 1]` with `let w: f32; let k: u8` | accepted |
| `??`: `[pp, mp() ?? new P(1)]`, `[one, mcb() ?? ((x) => …)]` | accepted |
| index: `[a, xs[0]]`, `[a, xs[1 - 1]]` with `xs: f32[]` | accepted |
| index: `[u, [1, 2][0]]` | S007, `i32` to `u8` |
| `new` with a class that is not generic: `[pp, new P(3)]`, `[pp, new P(1 + 2)]` | accepted |
| member read: `[a, pp.x]`, `[u, q.v]`, `[a, -pp.x]` | accepted |
| `[E.A, E.B]`, `[d, "down"]`, `[u, cnt()]`, `[u, k++]`, `[one, one]` | accepted |
| `[one, true && ((x) => …)]` | S100 "logical operators require booleans" |

The unit test pins the operator rows as rule 4 controls.
- `compiler/src/check/inference.rs`. `infer_arguments` checks each
  argument that `context_sensitive` finds after the other arguments.
  Such an argument holds a lambda with a parameter that has no
  annotation. Before the first of them, each type parameter that a
  non-literal candidate fixes becomes a contextual candidate. So the
  argument takes its parameter type with that type, as an explicit type
  argument gives it (rule 2a). The type is the inferred type of the
  candidates so far (`join_candidates`), the same join that gives the
  final type argument, so the order of the other arguments does not
  matter. With `three(one, maybe(), …)` the inferred type is `CB | null`,
  which gives no parameter types (§249.4 item 3); the CLI of
  `6d2dd8a5` rejected that form too.
- `compiler/src/check/expr/conditional.rs` (new). `check_cond` and its
  helpers moved out of `operator.rs` (§248.3 item 4), which has 1,772
  lines now.

## Open

- §249.4 items 3 to 5: a nullable function context, the order of flow
  state across arguments, and the divergence block of a hinted generic
  call.

- §249.4 item 1: `[one, (x) => …] as CB[]` gives an S100 beside the
  rejection of `as`.
- §249.4 item 2: `((x) => …) as CB` gives the S100 alone, and `tsc`
  accepts it.

## Files

- `compiler/src/check/expr/entry.rs`, `compiler/src/check/expr.rs`,
  `compiler/src/check/expr/aggregate.rs`,
  `compiler/src/check/expr/array_of_and_map_copy.rs`,
  `compiler/src/check/expr/conditional.rs` (new),
  `compiler/src/check/expr/operator.rs`,
  `compiler/src/check/expr/lambda.rs`, `compiler/src/check/expr/method.rs`,
  `compiler/src/check/expr/promise_reaction.rs`,
  `compiler/src/check/inference.rs`, `compiler/src/check/initializer.rs`,
  `compiler/src/check/mod.rs`, `compiler/src/check/stmt.rs`
- `compiler/tests/element_hint_lambda_parameters.rs` (new),
  `compiler/tests/corpus_reject.rs`,
  `compiler/tests/initializer_inference.rs`,
  `compiler/tests/apparent_type_shapes.rs`
- `compiler/src/divergence/surface_forms.rs`
- `corpus/reject/r497-an-element-hint-gives-no-parameter-types-to-a-lambda.ts`
- `corpus/accept/a434-a-generic-argument-takes-the-type-that-other-arguments-fix.ts`
  and `.expected`
- `generated-docs/corpus-index.md`
- `specs/blocks/compiler/s248-a-context-free-list-of-functions-takes-the-type-that-each-fits.md`
  (§248.4 item 3 closed),
  `specs/blocks/compiler/s249-an-element-hint-gives-no-parameter-types-to-a-lambda.md`
  (§249.3 and §249.4)
