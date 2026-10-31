<!-- §202 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 202. A type alias names its aliased type

*(Added 2026-10-28.)* Origin: on 2026-10-27 the owner asked for the
divergences from TypeScript that are cheap to remove. On 2026-10-28
the owner selected it as the second section of the second group. The
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
2. The checker checks `T` with the rules of a type annotation.
   A `T` that an annotation rejects fails with the text of that
   annotation, at the alias declaration. A use of an alias that
   failed gives no second diagnostic.
3. The declaration order does not matter. An alias can name a class,
   an enum, or an alias that a later declaration in the module
   declares. Every alias resolves before the checker resolves the
   member types of a generic-class instance. So a member annotation
   that names an alias sees the aliased type, also when an alias names
   that instance (`type List = Link<i32>;` with a field
   `next: List | null` in `Link<T>`). A lookup never reads an alias
   that is not resolved: if it does, the checker reports an internal
   error, not a silent failed alias. *(Amended 2026-10-28 after the
   Phase Review, M1.)*
3a. A type argument on an alias (`Id<i32>`) fails with S100 "type
   alias `N` is not generic", as `tsc` TS2315 does. This applies to a
   mirror alias too. A value use of an alias (`new N()`, `N.m()`)
   fails with the existing text "type alias `N` used as a value".
4. An alias that reaches itself fails with S100 "type alias `N`
   refers to itself". The text is given once, at the alias that the
   cycle reaches again. Two cases have different `tsc` results:
   - A cycle whose every edge is a direct reference, through a union
     or parentheses only (`type A = B; type B = A;`,
     `type U = U | null;`): `tsc` rejects it with TS2456.
   - Any other cycle, through an array, a type argument, or a function
     type (`type L = L[];`, `type M = Map<string, M>;`): `tsc` accepts
     it. A type of unbounded depth has no finite type in this
     language, so this is a divergence.
   *(Amended 2026-10-28: the coding agent measured the second case.)*
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

### 202.3 Phase Review

*(Added 2026-10-28.)* A fresh review of `f36e06d3..cbdcc307` gave one
MAJOR and six MINOR findings.

1. MAJOR, realistic, fixed: an alias of a generic-class instance
   instantiated the members of that class before later aliases
   resolved. A member annotation that named a pending alias got the
   failed type with no diagnostic. `check` accepted the program, and
   `run` stopped with an internal lowering error. The cause was a
   pending alias that a lookup read as a failed alias. Rule 3 now
   states the order. `a387` pins it.
2. MINOR, realistic, fixed: a type argument on a non-generic alias was
   ignored. `tsc` gives TS2315. Rule 3a.
3. MINOR, realistic, fixed: `new P()` on an alias of a class gave
   S016 "unknown class `P`".
4. MINOR, realistic, fixed: the divergence text of a recursive alias
   named a nested array for every recursive shape.
5. MINOR, fixed: witness programs that no target row used.
6. MINOR, fixed: rule 2 and the origin line were not correct English.

Open:

1. `Opt | null`, where `type Opt = C | null`, is rejected with S011
   for `C | null | null`. The written form `C | null | null` is
   rejected too. `tsc` accepts both. Contrived.
2. `type V = void;` is rejected. `tsc` accepts it.
3. *(Closed 2026-10-31.)* `generated-docs/language-reference.md` had
   no text for a transparent alias.
