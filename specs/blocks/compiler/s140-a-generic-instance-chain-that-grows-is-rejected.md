<!-- §140 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 140. A generic instance chain that grows without bound is rejected

*(Added 2026-10-01.)* Origin: found during §135 (a Phase Review
probe), recorded as an open defect.

Problem: the checker makes one instance per distinct list of type
arguments (monomorphization). A generic function that calls itself
with a type argument that contains its own type parameter asks for an
infinite chain of instances, and the checker overflows its stack.
Measured at `a9c994f9`:

```ts
class W<T> { v: T; constructor(v: T) { this.v = v; } }
function nest<T>(x: T, n: i32): i32 {
  if (n <= 0) { return 0; }
  return 1 + nest<W<T>>(new W<T>(x), n - 1);
}
export function main(): void { print(`${nest<i32>(1, 3)}`); }
```

`subscript check` ends with "thread 'main' has overflowed its stack /
fatal runtime error: stack overflow, aborting". `tsc` accepts the
program (polymorphic recursion is ordinary in TypeScript, which erases
types), and `node` prints `3`. A compiler crash is never a diagnostic
(core principle 5).

### 140.1 Rules

1. Every instance request comes from a type expression that the
   checker resolves inside a requesting instance: explicit type
   arguments of a call, of `new`, or of a method call, and a type
   annotation of a signature, a field, a local, or a return type. While
   it resolves the expression, the checker records, for each argument
   position j of the request, the argument positions k of the
   requesting instance whose type parameter the argument mentions, and
   whether the argument is that parameter itself (a plain edge) or
   contains it (an expanding edge: `W<p>`, `p[]`, `p | null`,
   `Map<K, p>`, `(x: p) => i32`).
2. The checker keeps the chain of instances it is making, with these
   edges. A request for a template that is already on the chain, at
   instance X, is a growing request when the edges compose, along the
   chain from X to the request, to a path from a position j of X back
   to the same position j of the request that holds an expanding edge.
   A growing request is S100 at the request site; no instance past it
   is made. The message names the template, the two instances, and the
   growing argument, and says that the instances grow without bound.
   Every other request is accepted: plain recursion, a parameter passed
   unchanged or swapped, a constant argument (`f<T>` calls
   `f<W<i32>>`), and a position that grows from another position that
   is constant (`f<A, B>` calls `f<i32, P<A, i32>>`). *(Rewritten
   2026-10-01, the third text. The first compared concrete arguments;
   the second marked a whole request as derived from a parameter;
   both rejected finite chains that the pin runs. A static graph over
   the templates (the ECMA-335 expansive-cycle rule *(docs)*) needs the
   target template of a method request before the checker types the
   receiver, which the checker does not have: the target depends on
   the receiver's type. The chain carries that fact, because each
   request on it is already resolved.)* This rule always terminates: a
   chain that grows without bound passes an expanding edge from a
   template position back to itself, and the template positions are
   finite.
3. This is a divergence from `tsc`, which accepts every such program.
   The implementation commit adds a collision heading (C19) with its
   divergence variant and a reject entry, and the rendered diagnostic
   carries the divergence block.
4. The edges are recorded wherever the checker resolves a type
   expression that requests an instance, in a function, a method, and
   a class template, and in the signature pass. No form of
   instantiation reaches the stack overflow. The recorder matches
   every type form with no catch-all arm.
4a. The §135 opaque check of a template does not check the bodies of
   the instances that its opaque arguments reach (its cost stays
   linear in the templates). It reports a growing request whose chain
   closes inside the template's own body and signatures. A chain that
   grows only through the body of another template is reported where
   a program instance makes it; a template that no program instance
   reaches and that grows only through another template's body is not
   reported, and it makes no instance, so it cannot overflow.
   *(Added 2026-10-01 after the verification review: checking those
   bodies made `subscript check` cost grow about 6x for each doubling
   of the generic helpers, 0.09 s to 2.71 s for 120 helpers in a debug
   build.)*

### 140.2 Acceptance

1. Red first: a reject entry with the measured program; at the
   contract pin `subscript check` ends with the stack overflow; the
   header states `tsc: accepts` and the divergence (C19). The round
   records the Red output.
2. Unit tests: the function shape, a method shape, a class shape
   (`class N<T> { next(): N<W<T>> | null { return null; } }` if the
   surface reaches it, or the closest shape), a mutual recursion
   through two templates (`f<T>` calls `g<W<T>>`, `g<U>` calls
   `f<U>`); controls: plain recursion `f<T>` calls `f<T>`, and a
   finite chain `f<i32>` calls `f<string>`, accepted and run; the
   constant-argument shapes of rule 2 (`f<W<i32>>` from `f<T>`, the
   same through mutual recursion, `Box<Box<i32>>` in a method of
   `Box<T>`) accepted and run, and an accept entry carries one of them
   (C19 lists it). Each rejection asserts the full message and exactly
   one diagnostic. An instance name in a message carries no file label
   unless two distinct declarations share the name. The second
   review's shapes are accepted and run: `f<A, W<i32>>` from
   `f<A, B>`, `f<B, W<i32>>`, `f<i32, P<A, i32>>`, and a class field
   `N<A, W<i32>> | null` in `N<A, B>`. A growing argument with an
   S011 error in it gives the S011 only.
3. No existing `.expected` golden moves.
4. A cost test: a program with many generic helpers that call each
   other with their own parameter checks with a number of body checks
   linear in the templates (count the checks, not the time), with a
   firing control.
