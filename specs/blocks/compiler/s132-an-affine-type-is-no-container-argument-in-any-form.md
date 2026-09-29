<!-- §132 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 132. An affine type is no container argument in any form

*(Added 2026-09-30.)* Origin: a disagreement between §40.1 and the
checker, found during §124 and ordered by the owner on 2026-09-28.

Problem: §40.1 states that a Context-affine type (`Worker`, `Inbox`,
`Outbox`) is illegal as any container type argument: a `Map` key and
value, a `Set` element, and the array positions. The checker applies
the rule where it resolves a type annotation, and inside a generic
body after substitution, but not to the type arguments of a
construction. Measured at `5c5ec98` through `subscript check`:

- `const m = new Map<i32, Worker<Message, Message>>();` and
  `new Map<i32, Worker<Message, Message>>();`: no errors.
- `const m = new Map<i32, Inbox<Message>>();`: no errors.
- `const s = new Set<Worker<Message, Message>>();`: only S014 (not a
  permitted key kind), not the §40.1 diagnostic.
- With a type annotation, `const m: Map<i32, Worker<Message, Message>>
  = new Map<...>();` gives the §40.1 S100 and then a second diagnostic
  for the same failure (S005 "the initializer expects `Map<i32,
  <error>>`"); a `Set` or a `Map` key gives S100 and then S014.

`tsc` accepts each of these programs.

### 132.1 Rules

1. The §40.1 affine rule is one predicate at the one place the checker
   forms a container type (`Map<K, V>`, `Set<T>`, an array element),
   whatever the source: a type annotation, the type arguments of `new`,
   an inferred type, a generic substitution, a field, a parameter, or a
   return type. No form of a container type bypasses it.
2. A rejected container type reports the §40.1 S100 once, at the
   offending type argument, and is poisoned: no other diagnostic
   follows from it (no S005 for the mismatched `<error>` argument, no
   S014 for the key kind).
2a. *(Added 2026-09-30 after the Phase Review: a `Type::Error` nested in
   a container was not poisoned, so each consumer of `<error>[]` or
   `Map<K, <error>>` reported again — an array method gave S014, an
   interpolation, an operator, an assignment, `JSON.parse`, message
   transferability, and `Map.forEach`/`Set.forEach` each gave S100 or
   S005; the class predates §132, as `a: Nope[]` shows.)* A type that
   contains the error type is the error type: the checker's type
   constructors (an array, `Map`, `Set`, `FixedArray`, a nullable form,
   a function type, a generic instance) return the error type when any
   component is the error type, so every consumer sees one poisoned
   value and reports nothing more. A test builds each consumer above
   on a poisoned container and asserts one diagnostic.
3. §40.1 does not change otherwise.

### 132.2 Acceptance

1. Red first: a reject entry with `const m = new Map<i32,
   Worker<Message, Message>>();` in a function body, accepted at
   `5c5ec98`; the round records the Red output. Its header states the
   measured `tsc` result and cites the collision or register entry
   that r111 cites.
2. Unit tests: `new Map` with the affine type as key and as value,
   `new Set`, each of `Worker`, `Inbox`, `Outbox`, and a construction
   through a type alias if the surface has one; each gives exactly one
   diagnostic. Controls: the same constructions with a message class
   are accepted.
3. The annotated forms of r111 and the unit tests of §40.1 give
   exactly one diagnostic each.
4. No `.expected` golden moves.
