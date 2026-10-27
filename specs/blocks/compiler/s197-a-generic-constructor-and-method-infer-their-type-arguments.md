<!-- §197 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 197. A generic constructor and a generic method infer their type arguments

*(Added 2026-10-27.)* Origin: on 2026-10-27 the owner asked for the
divergences from TypeScript that are cheap to remove, and selected
this one first. The measurement is in
`specs/tracking/generic-construction-inference.md`.

Problem, measured at the pin `14a8fde5` (`tsc` 5.9.2 accepts each row,
and `node` runs it):

| Form | `subscript check` |
|---|---|
| `new Box(n)`, `class Box<T> { constructor(v: T) }` | S100 "generic class `Box` requires explicit type arguments" |
| `const b: Box<i64> = new Box(7)` | S100, as above |
| `const xs: Box<i32>[] = [new Box(1)]` | S100, as above |
| `return new Pair(this.b, this.a)` in `Pair<A, B>` | S100, as above |
| `m: Map<string, Item> = new Map()` (a field) | S100 "`new Map` requires explicit type arguments (Q24)" |
| `const s: Set<string> = new Set()` | S100, as above for `Set` |
| `r.wrap(x)`, `wrap<T>(value: T): Box<T>` | S100 "generic method `wrap` requires explicit type arguments" |

§149 infers the type arguments of a generic function call. Its rule 7
leaves a constructor and a method out, and gives no reason.

### 197.1 Rules

1. `new G(args)` with no type arguments, where `G` is a generic class,
   `Map`, or `Set`, infers the type arguments of `G`. A call
   `recv.m(args)` of a generic method with no type arguments infers the
   type arguments of `m`. The class type arguments of `recv` are known
   from the type of `recv`. The result is the same expression with the
   type arguments written explicitly. Each later rule applies to it
   unchanged.
2. The candidates come from the arguments, by §149.1 rules 2 to 4. For
   a constructor, the parameters are the parameters of its
   `constructor`. A class with no `constructor` has no argument
   candidate.
3. **The contextual type.** If the expression has a contextual type
   that is `G<A1, …, An>` or `G<A1, …, An> | null`, the contextual type
   gives a candidate `Ai` for each type parameter of a constructor.
   A non-literal argument candidate comes first. The contextual
   candidate comes second. The C4 default of a literal comes last. A
   literal argument is then checked with the inferred type as its
   context (§149.1 rule 3). The contextual type of a method result
   gives no candidate (§149.1 rule 7 keeps this out).
4. A contextual type is the type that the checker gives the expression
   as its context: the annotation of a local, the type of a field with
   an initializer, the parameter type of an argument, the result type
   of a `return`, and the element type of an array literal with a
   context.
5. A type parameter with no candidate is rejected (S100) with the
   §149.1 rule 5 text. Two conflicting non-literal candidates are
   rejected (S100) with the §149.1 rule 4 text. The name in the text is
   the class name for a constructor, and `Class.method` for a method.
   `new Map()` and `new Set()` with no contextual type keep their Q24
   text.
6. Inference is a check-time step. Each argument is evaluated once, in
   order. The program has no run-time cost.
7. A generic method of a generic class stays rejected, as at the pin.

### 197.2 Acceptance

1. Red first, at the pin `14a8fde5`: the accept entry
   `a377-generic-construction-inference` is rejected. Each reject entry
   `r440`, `r441`, `r179`, and `r186` fails its expected error at the
   pin. The tracking note records each result.
2. `a377` passes `tsc`, and its output under `node` equals its golden.
3. `r179` and `r186` change to a method type parameter with no
   candidate. At the pin they pinned the rule 7 rejection, which this
   section removes.
4. The divergence entry `GENERIC_CONSTRUCTOR_TYPE_ARGUMENTS` and the
   rejection sites that only rule 7 reached are removed or changed to
   the §149 texts. The rejection-site total check passes.
5. A unit test pins the candidate order of rule 3, with controls: a
   non-literal argument against a different contextual type is a type
   error, not a contextual inference.
6. `stdlib.md` §10.1 cites this section for `new Map()` and `new Set()`.
7. No golden moves.
