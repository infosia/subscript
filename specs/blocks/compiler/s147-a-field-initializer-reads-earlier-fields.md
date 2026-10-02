<!-- §147 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 147. A field initializer reads earlier fields

*(Added 2026-10-02.)* Origin: a downstream proposal (`HANDOFF.md`,
2026-10-02, item 2). Owner decision of the same day: take it second.

Problem: `collisions.md` C9 rejects every `this` in a field initializer.
`tsc` accepts a read of a field that an earlier initializer set.
Measured at `14410e3d` through `subscript check` and `tsc` 5.9.2:

| Class body | `subscript check` | `tsc` |
|---|---|---|
| `x: i32 = 3; y: i32 = this.x + 1;` | S100 "`this` is only available in constructors and methods" | accepts |
| the same with a constructor that assigns `this.x` | S100 | accepts |
| `@ValueType`, `class G<T>`, and `y = this.x + A.s` with a static `s` | S100 | accepts |
| `b: B = new B(); y: i32 = this.b.v;` | S100 | accepts |
| `y: i32 = this.x + 1; x: i32 = 3;` (a later field) | S100 | TS2729 |
| `x: i32 = this.x;` (itself) | S100 | TS2729 |
| `x: i32; y: i32 = this.x;` with `x` set only in the constructor | S100 | TS2729 |
| `y: i32 = this.f();` (a method) | S100 | accepts |
| `y: A \| null = this;` | S100 | accepts |
| `y: i32 = this.g;` (a getter) | S100 | accepts |
| `y: () => i32 = () => this.x;` | S100 | accepts |

### 147.1 Rules

1. In an instance field initializer, `this.f` is a read of field `f`
   when `f` is an instance field of the same class, declared earlier,
   and has an initializer. It reads the value that `f`'s initializer
   stored (§57: initializers run in declaration order before the
   constructor body).
2. Every other `this` in a field initializer stays rejected: a read of
   the field itself or of a later field, a read of a field with no
   initializer, a method call, a getter or setter, a write through
   `this`, `this` as a value (passed, stored, returned, compared), and
   `this` inside a lambda. Each one can observe a field that holds no
   value yet, or exposes the partial instance. The diagnostic names the
   rule that rejects the form.
1a. A call `this.f(...)` where `f` is an earlier initialized field of a
   function type is a read of `f` followed by a call; it is accepted. A
   lambda cannot capture `this`, so the call cannot reach the partial
   instance. *(Added 2026-10-02 after the Phase Review.)*
3. Rule 1 applies to reference classes, `@ValueType` classes, and
   generic classes, with or without a declared constructor.
3a. A `@Descriptor` member default does not read `this`: every `this`
   there is rejected with S100. A descriptor is built from a literal, so
   a default runs beside the literal's supplied members, and `tsc` types
   an optional member as possibly `undefined` (TS2532 for
   `this.w * 3`). *(Added 2026-10-02 after the Phase Review: the checker
   accepted every `this` in a default, so `a?: i32 = this.b` before
   `b?: i32 = 7` read 0, and `self?: D | null = this` exposed the
   partial instance.)*
4. The evaluation order of §57 does not change: constructor arguments,
   then field initializers in declaration order, then the constructor
   body.
5. C9 is amended: it rejects the forms of rule 2, not every `this`.
   `tsc` accepts the method, getter, `this`-value, and lambda forms;
   C9 records that divergence.

### 147.2 Acceptance

1. Red first: an accept entry with each accepted row of the Problem
   table (earlier field, with and without a constructor, value class,
   generic class, static read beside it, a member of an earlier
   reference field), printing the values. It is rejected at the
   contract pin; `node` output equals the golden.
2. Reject entries for rule 2: a later field and the field itself
   (`tsc` TS2729), a constructor-only field (TS2729), a method call, a
   getter, `this` as a value, and `this` in a lambda (each `tsc`
   accepts; each names C9). The existing `r126-this-in-field-init` pins
   `value: i32 = this.tag + 1` after `tag: i32 = 2`, a rule 1 form; it
   retires (`retired:r126-this-in-field-init`).
3. A side-effect witness shows the order of rule 4.
4. No existing `.expected` golden moves.
