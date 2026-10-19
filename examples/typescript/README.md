# subscript for TypeScript beginners: examples

Each script shows one area where subscript differs from TypeScript. The
comments next to the code say what you write in subscript, why, and what
`node` prints where the output differs. Each script has its output in
`<name>.expected` beside it, and a test compares the two on both
compilers of subscript. Every script type-checks under `tsc --strict`.

| Script | Shows |
|---|---|
| [`numbers.ts`](numbers.ts) | Sized types in place of `number`, integer division, `as` conversions, wrap at the largest value |
| [`absence.ts`](absence.ts) | `null` only, `Pet \| null`, a test before a member read, `??` and `?.` |
| [`classes.ts`](classes.ts) | Classes compare by name, a reference class, a `@ValueType` class that copies |
| [`memory.ts`](memory.ts) | No collector runs by itself, no release code, `Context.free`, `using` |
| [`strings.ts`](strings.ts) | UTF-8 strings: `length` counts bytes, `for...of` counts characters |
| [`arrays.ts`](arrays.ts) | `push`, `map`, `filter`, `reduce` with typed arrows, an arrow that cannot leave its function |
| [`unions.ts`](unions.ts) | A closed union of string literals in place of a string enum, a `switch` over it |
| [`errors.ts`](errors.ts) | `throw` of error objects, `catch` with `instanceof`, `finally`, faults that no `catch` sees |
| [`generators.ts`](generators.ts) | A generator that does its work over several frames |
| [`rejected.ts`](rejected.ts) | A program that `tsc` accepts and subscript rejects, with its diagnostic |

Do the setup step of the
[TypeScript tutorial](../../docs/tutorial-typescript.md#setup). Then run
one script from the repository root:

```sh
subscript run examples/typescript/<name>.ts
```

The [TypeScript tutorial](../../docs/tutorial-typescript.md) describes
each difference in more detail.
[`specs/blocks/collisions.md`](../../specs/blocks/collisions.md) gives
the rule for each difference, with the reason.
