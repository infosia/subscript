<!-- §241 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 241. A class read before its declaration is rejected

*(Added 2026-11-07.)* Origin: §240.4 item 9. Under the owner's standing
approval of 2026-11-06 for `tsc` divergences with a known fix.

Problem, measured at `01be4d7e` with `tsc` 5.9.2, the libraries of the
project, and the debug CLI. `B` is a class with `static k(x: i32): i32`
and `static m(x: i32): void`, declared at the top level of the module
after the line in the table. `E` is an enum, declared the same way.
`Box<T>` is a generic class, declared the same way.

| Form, before the declaration | subscript | `tsc` |
|---|---|---|
| `const early: CB = B.m;` | accepts | TS2449 "Class 'B' used before its declaration." |
| `const v: i32 = B.k(1);` | accepts | TS2449 |
| `const b: B = new B();` | accepts | TS2449 |
| `const gb: Box<i32> = new Box<i32>(1);` | accepts | TS2449 |
| `const e: E = E.A;` | accepts | TS2450 "Enum 'E' used before its declaration." |
| `class A { static x: i32 = B.k(3); }` | accepts | TS2449 |
| `if (true) { const u: i32 = B.k(6); }` | accepts | TS2449 |
| `for (let i: i32 = 0; i < B.k(0); i++) {}` | accepts | TS2449 |
| `const s: i32 = ((): i32 => B.k(7))();` | accepts | TS2449 |

`tsc` accepts each of these forms before the declaration:

| Form | `tsc` |
|---|---|
| `const f1 = (): i32 => new B().n;` | accepts |
| `function f2(): i32 { return B.k(1); }` | accepts |
| `class A { y: i32 = B.k(4); }` | accepts |
| `class A { m(): i32 { return B.k(5); } }` | accepts |
| `class S { static a: i32 = 1; static b: i32 = S.a + 1; }` | accepts |

Each accepted program must pass `tsc` (invariant 5). The first table is
a gap against it. The program runs today because a class has no
initialization at run time, but `tsc` and the editor report an error on
each such program.

A read through a call, such as `const w: i32 = g();` where `g` reads a
static field declared later, is the C14 initialization order check
(`InitializerRouteRead`). This section does not change it.

### 241.1 Rules

1. A value reference to a class, a generic class, or an enum of the
   module is rejected with S100 "`B` is used before its declaration",
   when the reference is before the start of the declaration in the
   source and the reference is evaluated at once. A `const enum` is
   not such an enum: `tsc` with the options of the project accepts
   `const c: C = C.X;` before `const enum C { X, Y }`.
2. A reference is evaluated at once unless it is in one of these:
   - the body of a function declaration, a method, a getter, a setter,
     or a constructor,
   - the body of a lambda that is not called at once,
   - the initializer of an instance field,
   - the default of a parameter of a function, a method, a
     constructor, or a lambda that is not called at once, also when
     the parameter has no type annotation.
   A top-level initializer or statement, a nested block, a loop
   header, the initializer of a static field, and the body of a lambda
   that is called at once (`(() => ...)()`) are evaluated at once. This
   is the rule of `tsc` for TS2449 and TS2450.
3. A reference inside the declaration of the class or enum is not
   before its start, so `static b: i32 = S.a + 1;` in `S` stays
   accepted.
4. A type reference is not a value reference. `const f: (b: B) => i32`
   before `B` stays accepted.
5. No accepted program changes its output. Each accept entry passes
   `tsc` today, so none of them has the form of rule 1.

### 241.2 Acceptance

1. Red first, at the pin: the reject entry
   `r493-class-read-before-its-declaration` has the form
   `const early: CB = B.m;` and checks with no errors. The tracking
   note records the output.
2. The reject entry `r494-enum-read-before-its-declaration` has the
   form `const e: E = E.A;`. Its header states the measured TS2450.
3. A unit test pins rules 1 to 4. It has each form of the first table
   as a rejected case, and each form of the second table as an
   accepted control in the same program shape.
4. If a corpus entry or golden changes its type or output, report it
   and stop. No golden moves.

### 241.3 Phase Review

*(Added 2026-11-07.)* Two fresh reviews of the implementation. The
contract gained parameter defaults in rule 2 and the `const enum`
exception of rule 1.

1. MAJOR, realistic, fixed: the default of a lambda parameter with no
   type annotation, `const f = (x = B.k(1)): i32 => x;` before `B`, was
   rejected. `tsc` accepts it. The parameter types were decided in a
   frame cloned from the outer frame, so the default took the outer
   evaluation. Each frame context now states its evaluation where it
   is built (`FnCtx::new` takes an `Evaluation`).
2. MINOR, fixed: a `const enum` read before its declaration was
   rejected. `tsc` with the options of the project accepts it.
3. MINOR, fixed: the unit test had no firing control for rule 3, and
   no case for the deferred member forms of rule 2.
4. MINOR, recorded: §241.4 item 1.

### 241.4 Open

1. A read or a write of a static field before its class gives two S100
   at one site: the diagnostic of rule 1, and the C14 diagnostic
   "`B.z` is accessed before its declaration". `tsc` gives one TS2449.
   The program is rejected in both.
