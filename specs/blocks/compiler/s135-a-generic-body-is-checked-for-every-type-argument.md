<!-- §135 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 135. A generic body is checked for every type argument

*(Added 2026-09-30.)* Origin: found during §127 and §134 (Phase
Review probes). The checker accepts programs that `tsc` rejects, which
breaks CLAUDE.md invariant 5.

Problem: the checker checks a generic function, method, or class body
once per instantiation, with the concrete type arguments substituted
(monomorphization). It never checks the body with the type parameters
themselves. Measured at `282375e9` through `subscript check`, each
program with a `main` that does not instantiate the generic:

- `function gf<T>(x: T): i32 { return nope(); }`: no errors (`tsc`
  TS2304).
- `const k: i32 = 1; function gf<T>(x: T): i32 { k = 2; return 0; }`:
  no errors (`tsc` TS2588).
- `function gf<T>(x: T): i32 { const s: string = 5; return 0; }`: no
  errors (`tsc` TS2322).
- `class G<T> { v: i32 = 0; run(): void { this.v = "a"; } }`: no
  errors (`tsc` TS2322).
- `function gf<T>(x: T): i32 { return x.v; }`: no errors (`tsc`
  TS2339).

And with an instantiation: `class B { v: i32 = 5; }` with
`gf<B>(new B())` for the last `gf` runs and prints `5`; `tsc` rejects
the body with TS2339 ("Property 'v' does not exist on type 'T'"). A
body is accepted when every instance happens to fit, which is duck
typing across instances, not the generic contract `tsc` checks.

### 135.1 Rules

1. Every generic body (a function, a method, an async function, a
   generator, a class member, a generic method of a generic class) is
   checked once with each type parameter bound to an opaque type
   parameter type, whether or not the program instantiates it. The
   per-instance check and lowering do not change. The opaque check
   runs in every checker run that feeds a later pass, including the
   provisional run whose loop-narrowing effects the final run reads,
   so a narrowing inside a loop of an uninstantiated body ends as it
   does in any other body.
2. The opaque check never decides anything that depends on the typing
   of a type parameter. It keeps a diagnostic only from a closed list of
   kinds that do not depend on the type argument, and drops every other
   diagnostic it produces:
   - a name that does not bind (S016), whether `tsc` binds it or not
     (a language restriction such as an unsupported global function is
     kept);
   - an assignment to a `const` binding or an import binding;
   - a type mismatch, or a nullable use (S011, including the §124 kill
     of a shared location), where no type of the diagnostic involves a
     type parameter or a type derived from one;
   - a member read, a method call, or a call of the value on a value
     whose type is a type parameter with no constraint (`tsc` TS2339,
     TS2349).
   Every other decision about a body — an operator, a result type, a
   deferral, a constraint's members, a language restriction — is made
   by the per-instance check, as before §135. The list is total: a new
   diagnostic kind is dropped until this rule names it. *(Replaced by
   §143 rules 1, 2, and 2a.)* *(Rewritten
   2026-09-30, owner decision, after two Phase Reviews: a form that
   modelled `tsc`'s typing of a type parameter with an empty reference
   class and patched each consuming site rejected `tsc`-valid programs
   the pin accepted at sites nobody patched — a fresh `number` beside a
   fresh `number`, a `switch` on `T`, `for...of` over a constrained `T`,
   a constrained `T` as an index, narrowing of a constrained `T`, `??`
   with a `T | null` operand. The rewrite trades those rejections for
   a recorded gap: see 135.3.)*
2b. A type argument that does not satisfy its type parameter's
   constraint (`T extends C`) is an error at the type argument, as
   `tsc` gives TS2344. *(Added 2026-09-30, owner decision.)* "Satisfy"
   is the checker's assignability after every numeric type in both
   types (every numeric alias of the prelude, `f16` included, and
   `number`) is replaced with the one type `number`; two instances of
   one generic class template compare by their type arguments after
   the same replacement, so the check makes no instance (`W<u8>`
   satisfies `T extends W<i32>`),
   as `tsc` sees them: `keep<u8>` for `T extends i32` satisfies the
   constraint, and the per-instance check decides the numeric typing of
   the body. Each site of a type argument list is checked, including a
   repeated argument list whose instance already exists. *(Added
   2026-09-30 after the Phase Review: the checker's own assignability
   rejected `u8`, `f64`, and `i32` against `i32`, `i64`, and `f64`
   constraints that `tsc` and the pin accept, and a repeated argument
   list was checked once.)*
3. A diagnostic of the opaque check is reported once per site, not
   once per instance; the per-instance check does not repeat a
   diagnostic the opaque check reported at the same site. An instance
   diagnostic at that site with a different code stays. Two diagnostics
   with the same code, message, and position are one diagnostic,
   whichever check produced them: an opaque check that makes concrete
   instances (`g<i32>` and `g<f64>` inside `f<T>`) reports a site of
   `g` once. *(Added 2026-09-30 after the Phase Review: that shape
   reported one site twice.)*
4. Step 0 (measure first): the round builds the opaque check as a
   prototype, runs every corpus accept, warn, and trap source and every
   example, and records each one that becomes rejected, with the
   diagnostic. If any is rejected, the round stops and reports: a
   rejected source is either a `tsc`-invalid program the corpus missed
   (the header's `tsc: accepts` is wrong) or a language operation the
   opaque type must allow (rule 2 must name it). The owner decides.
   Measured on a prototype of the first rule 2 text: 379 sources, 2
   rejected (`a12`, `a178`, both by a restriction that the corrected
   rule 2 leaves to the per-instance check); the checker time on the
   corpus rose 0.7 %. The implementation round measures again under
   the corrected rule 2 and stops if any source is rejected.

### 135.2 Acceptance

1. Red first: reject entries for the measured shapes (an uninstantiated
   body with an unknown name, a const assignment, a type mismatch in a
   class method; an instantiated body with a member read on `T`), each
   accepted at `282375e9`, each header with the measured `tsc` code.
2. Unit tests: a diagnostic inside a body instantiated twice is
   reported once; every corpus accept source and example stays
   accepted (the round measures all of them again).
3. No existing `.expected` golden moves.
4. Tests pin each kept diagnostic kind of rule 2 in an uninstantiated
   body, with a control of the same shape through a type parameter that
   the opaque check drops (for example `x > 1`, `-x < -y`, `switch (x)`,
   `for (const e of x)` with `T extends i32[]`); rule 2b with a
   satisfied control; the loop-narrowing shape; and rule 3's
   different-code case.
5. Tests pin the Phase Review shapes: rule 2b accepts `u8`, `f64`, and
   `i32` against `i32`, `i64`, and `f64` constraints and reports each
   site of a repeated outside argument list; the `g<i32>`/`g<f64>`
   inside `f<T>` shape reports one diagnostic; each silent test fires
   a `nope();` control through the opaque check.

### 135.3 Open

*(§143 closes this gap. It retires under §143 rule 6.)*

The opaque check drops every diagnostic outside the kinds of rule 2.
So a form that `tsc` rejects for a type parameter stays accepted when
the program does not instantiate the body, or when every instance
accepts the form. The class is total: every dropped kind in such a
body. The measured forms include:

- a relational or arithmetic operator beside a literal (`x > 1`,
  `x + 1`, TS2365);
- `===` or `as` between two type parameters (TS2367, TS2352);
- a type mismatch that involves a type parameter: a logical or nullish
  result where `tsc`'s union type does not fit, unary `-` assigned to
  `T`, `const s: string = x`, `const t: T = b` for `T extends Box`
  (TS2322);
- a nullable use that involves a type parameter (`x.v` on `T | null`,
  TS18047);
- a call of a constrained `T` (`x()` for `T extends Box`, TS2349);
- a member write on an unconstrained `T` (`x.v = 3`, TS2339);
- a type argument that is itself a type parameter outside the
  constraint (`g<T>(x)` inside `h<T>` for `g<T extends Box>`, TS2344).

A language restriction outside the kinds of rule 2 (for example a
`string` field of a `@ValueType` class) in a body that the program does
not instantiate is not reported. An editor running `tsserver` shows
each `tsc` code above. `collisions.md` §3 records the gap.
