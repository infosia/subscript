# R39.9 — overloads by parameter type: deferred, with the narrowed shape

Owner decision 2026-09-02: R39.9 and R39.1 are deferred. This note
records the shape that a future decision starts from. Nothing here is
a contract; `compiler.md` §82.7 holds the cost facts.

## Why the request's rule 4 fails

The request states that `tsc` and this compiler select the same
signature on every accepted program. Measured against
`prelude/lang.d.ts`: every sized numeric is `number` to `tsc`, so
`tsc` selects the first numeric signature in declaration order, and
this compiler selects by the nominal sized type. `abs(x)` with
`x: i32` resolves to `abs(f32)` under `tsc` and to `abs(i32)` here.
The same holds for two classes where one is structurally assignable
to the other: `tsc` accepts a `Vec3f` argument for a `Vec2f`
parameter, this compiler does not.

## The narrowed shape

1. **Set.** Two or more signatures of one name and one arity, and one
   implementation signature, in the TypeScript form.
2. **Disjointness.** Every two signatures differ at one or more
   parameter positions by a kind that `tsc` cannot assign across. The
   kinds: the numeric family (every sized numeric is one kind),
   `string`, `boolean`, a class type. Two class types at one position
   are disjoint only when each class declares a public instance
   member name (field, method, or accessor) that no other class at
   that position declares. This is a sufficient condition for
   structural non-assignability in both directions, and this compiler
   checks it by name alone. `Vec2f {x, y}` beside `Vec3f {x, y, z}`
   fails the condition; `Mat4x4f` beside `Vec4f` passes it. A
   nullable type, an enum (`number` to `tsc` in both directions), an
   array, `FixedArray`, and a function type cannot serve as a
   distinguishing kind. A position that distinguishes no pair can hold
   any type.
3. **Resolution.** Under rule 2 at most one signature accepts a given
   argument list, so declaration order is irrelevant. A call selects
   the unique signature whose parameter types accept the argument
   types after C4 literal typing. No match rejects and names every
   signature. This compiler can reject a call that `tsc` accepts (a
   nominal mismatch at a non-distinguishing position); it never
   accepts a call that `tsc` resolves to a different signature.
4. **Body.** The one body checks once per signature, with every
   parameter at that signature's type. The instance name is the
   signature text (`mul(f32)`, `mul(V2)`, `f(i32,V2)`): a third
   reserved name family after `name=` (§65) and `m<A>` (§82.4). The
   union in the implementation signature is legal in that position
   only, parameters and return type; it reaches no HIR, LIR, or tier.
5. **Folding.** Inside an instance, `o instanceof C`,
   `!(o instanceof C)`, `typeof o === "number" | "string" |
   "boolean"`, and `!==`, on a parameter `o`, fold to a constant when
   they are the whole condition of an `if` or of a conditional
   expression. The dead arm is not checked. When a constant-`true`
   `if` has a `then` arm that ends in `return`, the rest of the block
   is unreachable and is not checked (the request's probe has this
   shape; `tsc` accepts it through the same control-flow narrowing).
   Alternative with a one-line rule: require the full `if`/`else`
   form and skip no statement. Outside an instance both operators keep
   S100.
6. **Scope.** Free functions, instance methods, and static methods of
   reference classes and `@CStruct` classes. Excluded, each S100 with
   a divergence block where `tsc` accepts: a constructor, an accessor,
   a generic function or method, an `async` function or method, a
   mirror or `@Descriptor` class, an arity overload (needs an
   `undefined` guard, S012), an exported overload (a host entry is one
   symbol; §64 rule 5 precedent), and an overloaded function as a
   value. A signature incompatible with the implementation rejects as
   `tsc` does (TS2394; no block).
7. **Lowering.** Each instance is an ordinary function or method.
   `sanitize` and the §65 rule 10 name table keep the C identifiers
   distinct. No tier changes.
8. **Corpus.** One accept entry: a value class with `mul(f32)` and
   `mul(V2)`; two classes with disjoint member names at one position;
   a free function over `string`, `boolean`, and a class; a `typeof`
   fold; signatures with different return types; one instance called
   twice. About six reject entries: two numeric signatures (`tsc`
   accepts), two structurally nested classes (`tsc` accepts), an
   incompatible implementation (TS2394), an arity overload (`tsc`
   accepts), an exported overload (`tsc` accepts), a call with no
   match. `collisions.md` gains a heading for the class.

## Cost and side effects

- The largest checker item in R39: bodiless signature collection, an
  overload-set entry in `ScopeItem` and `ClassSig`, one instantiation
  per signature (parameter annotations replaced, not type parameters
  substituted, so `instantiate_fn` is a partial model), the fold and
  the unreachable skip, and call resolution on three paths (free
  function, instance method, static method). Estimate: two to three
  coding-agent rounds and one review.
- Downstream: the HIR JSON carries `mul(V2)` as the method name. The
  downstream kernel generator keys on the method name and the argument
  count and moves to the instance name.
- The unreachable skip is a new checker rule and the likely review
  finding. The `if`/`else` alternative in item 5 removes it.

## R39.1

Deferred by the same decision. Zero downstream sites; §82.7 holds the
cost facts. A future request needs a downstream site that a
field-chain write cannot express.
