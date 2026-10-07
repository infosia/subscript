<!-- §173 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 173. Each `TscRejects` site is split on its fact

*(Added 2026-10-07.)* Origin: `compiler.md` §154.3, the realistic
`TscRejects` instances. On 2026-10-06 the owner selected them after
§172. The measurement round at `ae165007`
(`specs/tracking/s173-tsc-rejects-instances.md`) found items 1, 2, and 8
closed (§159, §164, §155), item 3 (`xs.length = 0`) a lowering change
(§174), and the items below open. Each open item is a program that
stock `tsc` 5.9.2 accepts, that this checker rejects at a site whose
§154 class says that `tsc` rejects it, so the diagnostic carries no
block and no collision. No item needs a lowering.

| Item | Program | `tsc` | HEAD |
|---|---|---|---|
| 4 | `e.stack`, `e.cause` on an `Error` | accepts | S018 `ClassUndeclaredMemberRead`, `TscRejects`, no block |
| 5 | `new Map<string, i32>([["a", 1]])` | accepts | the array element mismatch, before `FormNewMapIterable` |
| 6 | `const ps: P[] = qs;` with `Q` structurally equal to `P`; the same in a function type | accepts | `AssignmentTypeMismatch`, `TscRejects`, no C1 block |
| 7 | `abstract class A { abstract x: i32; }` | accepts | `FieldAssignmentMissingUnassignedExit` (a missing initializer) |
| 9 | bare `this` in an arrow in a static method | accepts | `Diverges(ThisStaticMethodMember)` with a block (correct); the message says that `this` is available in methods |
| 10 | `e.message` on an untyped catch binding; `const x: i32 = f()` with `f(): void`; a read before its `const` in one block | rejects (TS18046, TS2322, TS2448) | `Diverges` with a C6, C21, C14 block |

Item 10 is the reverse of the others: `tsc` rejects these three
programs, so they carry no block, but the site also covers programs
that `tsc` accepts (a template use of the catch binding, an unannotated
void binding, a read in a lambda), which keep their block.

### 173.1 Rules

1. **Item 4.** `Error.stack` and `Error.cause` are rejected at one new
   `Diverges` site, `ErrorMemberOutsideSurface`, split from
   `ClassUndeclaredMemberRead` and from `ClassUndeclaredPropertyWrite`
   (a read and a write) on the facts "the receiver is the builtin
   `Error` class" and "the member is `stack` or `cause`". `stdlib.md`
   §19.5 states the reason for each: `stack` has no
   call-stack representation in the three tiers (node prints frames and
   source positions); `cause` needs an arbitrary payload and an
   absent-value model, and C7 has no `undefined`. A member that neither
   the Error lib nor the source class declares stays `TscRejects`.
2. **Item 5.** A non-spread array literal of two-element array literals
   as the argument of `new Map(...)` reaches the existing
   `FormNewMapIterable` site (`Diverges`, `stdlib.md` §10.4: no tuple
   type) before the homogeneous array element check, if each pair's key
   and value are assignable to the `K` and `V` of the Map. A pair that
   is not assignable (`new Map<string, i32>([["a", "b"]])`, TS2769) is
   `TscRejects`. `new Map(otherMap)`
   keeps its branch. An ordinary `["a", 1]` elsewhere keeps the array
   mismatch site.
3. **Item 6.** A compound assignment (an array element, a function
   parameter, a function result, at any depth) whose only mismatch is a
   pair of classes that `tsc` relates structurally reaches a new
   `Diverges` site, `NestedNominalClass`, with a C1 block; its example
   shows an array and a function type. The check uses the structural
   comparison that `ts_nominal_assignable` already applies, before the
   generic mismatch. A compound mismatch that the structural comparison
   also rejects stays `TscRejects`.
4. **Item 7.** An `abstract` property is rejected at a new `Diverges`
   site, `AbstractMember`, with C24 row 11, and row 11 states the
   reason: no subclass can implement the member, because the checker
   rejects a class that `extends` an abstract class (S100). Measured at
   `451edc8b`: `tsc` 5.9.2 accepts a subclass that implements an
   abstract property and one that implements an abstract method; the
   CLI rejects both at the `extends`. The message names the abstract
   member, not an initializer. The field stays declared in the class
   shape, so a read of it (`this.x`) gives no second diagnostic.
5. **Item 9.** The bare-`this` message in an arrow in a static method
   states that a static method names its class (`ClassName.member`),
   as the member form of the same site does. The class and the block
   do not change (`Diverges(ThisStaticMethodMember)`, C24 row 15).
6. **Item 10.** Each of the three sites splits on a fact that the
   consumer has:
   - C6: a property access on an unnarrowed catch binding is
     `TscRejects` (TS18046); another use of the binding keeps C6. An
     optional property access (`e?.message`) is a property access.
   - C21: a void result stored in an explicitly annotated non-void
     destination is `TscRejects` (TS2322); an unannotated void binding
     keeps C21.
   - C14: an immediate read of a pending `const` in its own block is
     `TscRejects` (TS2448); a read in a lambda keeps C14.
7. Each new or split site is in the §154 site table with a measured
   witness for each class, and the §154 total test passes.

### 173.2 Acceptance

1. Red first, at the contract pin: a reject entry for each open item
   (`r389` to `r394`), each with its measured `tsc` header and the HEAD
   diagnostic.
2. A unit test for each split with its same-shape control in the
   other class.
3. No existing `.expected` golden moves.

### 173.3 Open

1. A compound mismatch with a class pair and another mismatch
   (`const p: (P | null)[] = q;` with `q: Q[]`) is `Diverges` with the
   C1 block only; the other mismatch has its own reason (contrived).
