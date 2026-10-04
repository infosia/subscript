<!-- §155 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 155. Member modifiers are enforced as tsc enforces them

*(Added 2026-10-04.)* Origin: the §154 Phase Reviews (§154.3, "Outside
this section"). A defect, not a surface change.

Problem: the checker accepts four forms that `tsc` rejects. This breaks
CLAUDE.md invariant 5. Each form is realistic. The checker does not read
the `private`, `protected`, `abstract`, or `readonly` modifier of an
instance member or a class, and it drops the `?` of a parameter in a
function type. Measured at `6fa57ff5` with `subscript check` and `tsc`
5.9.2:

| Program | `subscript check` | `tsc` |
|---|---|---|
| `class A { private x: i32 = 1; }` then `new A().x` in a module function | no errors | TS2341 |
| `protected b: i32` read outside the class | no errors | TS2445 |
| `private f(): i32` called outside the class; `private static n` written outside the class | no errors | TS2341 |
| `private constructor() {}` then `new A()` outside the class | no errors | TS2673 |
| `protected constructor() {}` then `new Q()` outside the class | no errors | TS2674 |
| `abstract class S { v: i32 = 1; }` then `new S()` | no errors | TS2511 |
| `@ValueType class V { private constructor() {} … }` (also `protected`) | no errors | TS1238, TS1270 at the decorator |
| `readonly x: i32` then `a.x = 2` in a module function | no errors | TS2540 |
| `readonly x: i32` then `this.x = 3` in a method; `q.x += 1`; `q.x++` | no errors | TS2540 |
| `f: (a: i32, b?: i32) => void` called as `f(1, 2)`; the argument is `(a: i32, b: i32): void => …` | no errors | TS2345 |
| `f: (a: i32, b?: i32) => void` called as `f(1)` | S100 "expects 2 argument(s)", no block | accepts |

`tsc` accepts these forms, measured in one program:

- A `private` or `protected` member read through another receiver of the
  same class, inside an arrow in a method, and inside a static method of
  the class.
- `new P()` with a `private` constructor inside a static method of `P`.
- `this.x = 1`, `this.x += 1`, and `this.x++` on a `readonly` field in
  the constructor body, also when the field has an initializer.

`tsc` rejects these forms with TS2540: a `readonly` write inside an arrow
or a nested function in the constructor, a write through another
receiver (`o.x = 3`) in the constructor, and a write in a static method.
`tsc` rejects `new S()` of an `abstract` class inside a static method of
`S` (TS2511).

`tsc` rejects the decorator because the prelude constraint of
`ValueType` is `abstract new (...args: never[]) => object`, and a class
with a non-public constructor does not satisfy it. A `@Descriptor` class
already cannot declare a constructor.

A static `readonly` field is already a `const` binding: a write is
rejected. This section does not change it.

### 155.1 Rules

1. A `private` or `protected` member of a class is accessible only from
   code inside the body of that class. This includes nested functions and
   arrows, static members, and every receiver of the class type. A
   member is a field, a method, an accessor, or a static member. An
   access from other code is rejected (S100). Classes do not inherit, so
   `protected` has the same reach as `private`.
2. If a constructor is `private` or `protected`, a `new` of that class
   outside the body of the class is rejected (S100).
3. A `new` of an `abstract` class is rejected (S100) in every location,
   also inside the body of the class. The class is usable as a type and
   through its static members.
4. A write to a `readonly` instance field is accepted only as `this.f`
   directly in the constructor body of the declaring class. A write in a
   nested function or an arrow is not in the constructor body. Every
   write form is a write: `=`, a compound assignment, `++`, and `--`. A
   write in other code is rejected (S100).
5. An optional parameter `p?: T` in a function type is rejected at its
   declaration (S012). This is the C7 rule for an optional parameter of
   a function declaration. The rejection poisons the function type
   (§154 rule 9), so a call through it gives no second diagnostic. This
   includes a call of a field or a local whose type is the function type.
6. A `@ValueType` class with a `private` or `protected` constructor is
   rejected at the decorator (S100).
7. Each site of rules 1–6 is a `RejectionSite` (§154). The sites of rules
   1–4 and 6 are `TscRejects`. The site of rule 5 is `Diverges` and cites C7.
   Each message names the member, the modifier, and the class.

### 155.2 Acceptance

1. Red first: one reject entry for each of rules 1–6, from `r345`.
   The rules 1–4 and 6 entries are tsc-rejected and have a `tsc: rejects`
   header with the measured TS code. The rule 5 entry is `tsc: accepts`.
   At the contract pin, the rules 1–4 and 6 entries give no diagnostic, and
   the rule 5 entry gives its first diagnostic at the call, not at the
   declaration. Record each pin result.
2. One accept entry, `js-comparable: yes`, holds each form that `tsc`
   accepts in the Problem section. Its golden is the `node` output.
3. Unit tests for each rule. Each rejected form has a control in the
   same shape that the checker accepts.
4. The §154 total test passes with the new sites. Each site has the
   witnesses that §154 requires; the rules 1–4 and 6 witnesses include the
   measured `tsc`-rejected forms of the Problem section.
5. No existing `.expected` golden moves.

### 155.3 Open

1. A compound assignment through a `private` getter and a public setter
   is accepted; `tsc` rejects the declaration (TS2808). Contrived.
2. A `private` or `protected` target in a write form outside the subset
   (`**=`, `||=`, `??=`, a destructuring assignment) stops at the
   write-form site, which carries a block; `tsc` rejects the access
   (TS2341). A `readonly` target in `for (c.x of …)` stops at the
   `for…of` form site; `tsc` gives TS2540. Contrived.
