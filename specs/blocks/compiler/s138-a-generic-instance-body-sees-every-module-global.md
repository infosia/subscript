<!-- §138 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 138. A generic instance body sees every module global

*(Added 2026-10-01.)* Origin: found during §137 (the fourth, fifth,
and sixth Phase Reviews).

Problem: a generic class instance that a global initializer or a
top-level statement makes has its method bodies checked at that point,
and a module global declared later in the file does not resolve there.
The checker reports nothing, and the HIR carries an error-typed value
that the LIR verifier rejects. Measured at `17f8548e`:

- `class Box<T> { x: T; constructor(x: T) { this.x = x; } peek(): i32
  { return m.v; } }`, `const b: Box<i32> = new Box<i32>(1);`,
  `const m: Foo = new Foo();`, and a `main` that prints `b.peek()`:
  `subscript check` gives "no errors"; `subscript run` exits 2 with
  "internal lowering error: … invalid constant/type pairing:
  Constant { ty: Error, kind: Null }". `tsc` 5.9.2 accepts the
  program, and `node` prints `7`.
- The same program with `const early: i32 = b.peek();` before `m`
  (an early read): "no errors", then the same internal error. `node`
  throws a ReferenceError; §137 rule 5a makes it S100.
- The same class instantiated only inside `main`, or with `m` declared
  before the instantiation, runs and prints `7`.

### 138.1 Rules

1. A generic instance body (a method, a constructor, a field
   initializer, an accessor) resolves every module-level name of its
   module and every import, wherever the program first makes the
   instance. The position of the instantiation does not change what
   a name in the instance body binds to.
2. A read of a module global from an instance body that runs before
   the global's initializer is the §137 rule 5a S100, with the route
   through the instance member (for example "through
   `Box<i32>.peek`").
3. The checker never passes an error-typed value to lowering with no
   diagnostic. A name that does not resolve in an instance body is the
   S016 of any other body.

### 138.2 Acceptance

1. Red first: an accept entry (`js-comparable`) with the first measured
   shape; it fails at the contract pin (internal lowering error); the
   round records the Red output. Goldens on all three engines and
   `node`.
2. Unit tests: the early-read shape is S100 with the route through the
   instance member; the same with a constructor, a field initializer,
   and an accessor of the generic class; a generic function instance
   called from a later function (control, accepted); each asserts the
   full message.
3. No existing `.expected` golden moves.
