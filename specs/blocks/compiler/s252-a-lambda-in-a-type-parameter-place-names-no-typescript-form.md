<!-- §252 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 252. A lambda in a type parameter place names no TypeScript form

*(Added 2026-11-12.)* Origin: §249.4 item 5. Under the owner's standing
approval of 2026-11-06, selected by the owner on 2026-11-12.

Problem, measured at `cf1b2d5f` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `wrap<T>(f: T): T`, `class Box<T>` with
`constructor(f: T)`, and `nn<T>(f: T | null): void` are generic. Each
lambda gets S100 "parameters require a type annotation" with the
divergence block "TypeScript accepts: `each([1, 2], (x) => {})`" of
the site `GenericCallbackParameterAnnotationMissing`.

| Form | `tsc` |
|---|---|
| `` wrap((x) => print(`w ${x}`)); `` | TS7006 "Parameter 'x' implicitly has an 'any' type" |
| `` new Box((x) => print(`b ${x}`)); `` | TS7006 |
| `` nn((x) => print(`n ${x}`)); `` | TS7006 |
| `` [mk, () => wrap((x) => …)] `` (`r497`) | TS7006 |
| `` lone((x) => …) `` with `lone<T>(f: (x: T) => void)` | accepts |
| `each([1, 2], (x) => {})` with `each<T>(xs: T[], f: (x: T) => void)` | accepts |
| `` fn((x) => …) `` with `fn<T>(f: ((x: T) => void) \| null)` | accepts |

The checker selects the site from the call alone: each lambda argument
of a generic callee takes it. `tsc` gives a lambda parameter a type only
from a function type in the place. A bare type parameter `T`, or
`T | null`, is not a function type. So the lambda has no context, and
`strict` rejects its parameter. The block then names a form that `tsc`
accepts, as a cause that does not apply.

### 252.1 Rules

1. A lambda argument of a generic callee takes the site
   `GenericCallbackParameterAnnotationMissing` only if the type of its
   parameter is a function type `F`, or `F | null` (§251), and the
   lambda has no more parameters than `F`. In a call that infers its
   type arguments, the type is the declared type, before the
   substitution. In a call with explicit type arguments, the type is the
   type after the substitution. `tsc` rejects a lambda with more
   parameters than `F` (TS2345, TS7006).
2. Another lambda argument with a parameter that has no type annotation
   takes the site of a lambda with no context: a `tsc` rejection, with
   no divergence block.
3. The message, the position, and the acceptance of each program do not
   change. Only the site and its divergence block change.

### 252.2 Acceptance

1. Red first, at the pin: the reject entry
   `r498-a-lambda-in-a-type-parameter-place-names-no-typescript-form`
   with `tsc: rejects TS7006` fails
   `divergence_blocks_match_every_reject_entry_tsc_header`, because its
   first diagnostic carries the block. The tracking note records the
   output.
2. After the change `r498` passes. It has the `wrap`, `Box`, and `nn`
   forms.
3. A unit test pins rules 1 and 2: each form of the table, and the
   controls `lone`, `each`, and `fn` that keep the block.
4. The witnesses and divergence entries move with the site. If a corpus
   entry or golden changes, report it and stop. No golden moves.
