<!-- §202 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 202. A type alias names its aliased type

*(Added 2026-10-28.)* Origin: on 2026-10-27 the owner asked for the
divergences from TypeScript that are cheap to remove. On 2026-10-28
the owner selected this one second of the second group. The
measurement is in `specs/tracking/transparent-type-alias.md`.

Problem, measured at the pin `f36e06d3`: `type V = Vec;`, `type Id = i32;`,
`type Names = string[];`, `type Pick = (v: i32) => i32;`,
`type Opt = Vec | null;`, and `type Lookup = Map<string, Id>;` each
give S100 "type aliases are limited to a union of two or more string
literals". `tsc` 5.9.2 accepts them, and `node` runs them.
`collisions.md` C24 row 7 gives "No lowering is decided" as the
reason. A mirror alias is already transparent (§12.2), so the
lowering exists.

### 202.1 Rules

1. A module-level `type N = T;` with no type parameters, where `T` is
   not a union of string literals, declares `N` as a second name of
   `T`. Each use of `N` in a type position is the type `T`. The
   alias has no layout, no C name, and no run-time form of its own.
2. `T` is checked as a type annotation in that position is checked.
   A `T` that an annotation rejects fails with the text of that
   annotation, at the alias declaration. A use of an alias that
   failed gives no second diagnostic.
3. The declaration order does not matter. An alias can name a class,
   an enum, or an alias that a later declaration in the module
   declares.
4. An alias that reaches itself fails with S100 "type alias `N`
   refers to itself". This includes `type A = B; type B = A;`, which
   `tsc` rejects with TS2456, and `type L = L[];`, which `tsc`
   accepts. A nested array of unbounded depth has no finite type in
   this language.
5. `export type N = T;` exports the alias. `import { N }` and
   `import type { N }` import it, as for a string-literal alias
   (§128). A re-export follows §128.
6. A diagnostic that prints the type prints `T`, not `N`.
7. These stay rejected with their existing texts:
   - a generic alias (`type P<T> = …`);
   - an alias with a `T | undefined` member (§195.1 rule 5);
   - an alias that holds a `TaskGroup`;
   - an async function result that names an alias of `Promise<T>`.
     §26 requires the written `Promise<T>`.
   - a type alias in a function body (C24 row 5).
8. A string-literal union alias keeps its Q32 meaning. An alias of
   that alias (`type L2 = Label;`) is the same closed set.
9. The form has no run-time cost.

### 202.2 Acceptance

1. Red first, at the pin `f36e06d3`: the accept entry
   `a386-transparent-type-alias` is rejected. The reject entries
   `r446-type-alias-refers-to-itself` and
   `r447-type-alias-rejected-annotation` fail their expected errors at
   the pin. The tracking note records each result.
2. `a386` passes `tsc`, and its output under `node` equals its golden.
   It uses an alias of a class, a sized numeric, an array, a function
   type, a nullable class, a `Map` instantiation, an alias declared
   before its target, an alias of an alias, and an alias exported from
   one module and imported in another.
3. The `SourceAliasNotLiteralUnion` rejection site and its divergence
   entry are removed or changed to the rule 4 site. The rejection-site
   total check passes.
4. A unit test shows that a function signature with `N` and the same
   signature with `T` give the same HIR, with a control on a different
   `T`.
5. `collisions.md` C24 row 7 drops the transparent alias.
6. No golden moves.
