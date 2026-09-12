<!-- §71 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 71. Static members

Owner decision 2026-08-29, after the
member-namespace measurement under §67.1 rule 3a. Static fields,
methods, and accessors were rejected with S100 as "not decided". `tsc`
accepts them, and a consumer that generates classes meets the
rejection. This section decides them.

### 71.1 The rules

1. **A static member lives in the class's static namespace.** The
   static namespace and the instance namespace are separate, as `tsc`
   has them: `static x` beside `x` is legal. Two static members of one
   name fail with S100 at the second declaration (§67.1 rule 3a, per
   namespace).
2. **A static field is one storage per class, for the module's
   lifetime.** It is a module data binding for §67.1 rule 4c: its
   initializer runs at the class declaration's position among the
   module's statements, in declaration order within the class, and
   reads only bindings declared before it. It survives a hot reload
   as a module global does (§reload). `static readonly` is a `const`
   binding; a write to it fails as a write to a `const` does.
3. **A static method is a function with no receiver.** `this` inside
   a static method or static accessor fails with S100. Stock `tsc`
   binds `this` to the constructor there; this language has no class
   object, and the narrowing is recorded, not measured against `node`.
4. **A static accessor follows §65** in the static namespace: one
   name, `get` and `set` of one type, read as `C.x`, written as
   `C.x = v`. A static read accessor with no write accessor is
   legal, and `C.x = v` then fails with S100 (§65 rule 6; amended
   2026-09-02, §82.5).
5. **Access is through the class name only.** `C.x`, `C.m()`, `C.x =
   v`. Access through an instance (`c.x` where `x` is static) fails
   with S100; `tsc` reports TS2576 for it.
6. **Where static members are legal.** A reference class and a
   `@CStruct` value class (a static field does not change the
   instance layout). A generic class fails with S100 at the `static`
   keyword: `tsc` gives one storage per class, not per instantiation,
   and this language has no class object to hang it on; recorded as a
   narrowing. A `declare class` (mirror) and a `@Descriptor` class
   keep their existing rejections.
7. **Lowering.** A static field lowers to a module global; a static
   method to a free function; a static accessor to two free
   functions. Both tiers read the same LIR forms they already read
   for globals and free functions; no tier gains a new form.

### 71.2 Corpus

- `a168` (accept): a static field read and written through the class
  name, a static method, a static accessor pair, a static and an
  instance member of one name, a static initializer that reads an
  earlier module binding and an earlier static field, and
  `static readonly`. `js-comparable: yes` if the output matches
  `node`; measure it.
- `r164` (reject): two static members of one name (`tsc` TS2300).
- `r165` (reject): `this` in a static method (`tsc` accepts; recorded
  as a narrowing).
- `r166` (reject): a static member read through an instance (`tsc`
  TS2576).
- `r167` (reject): a static member on a generic class (`tsc` accepts a
  static field that names no type parameter; narrowing).

### 71.3 Exit criteria

1. `a168` byte-identical across dev, ship, interpreter, and golden.
2. Each reject entry's header carries the measured `tsc` code.
3. `tsc` accepts `a168` under the corpus gate's options.
4. No existing corpus entry, golden, or `.expected` moves.
5. Rule 4c's fixpoint treats a static field as a module data binding:
   a static initializer that reads a later module binding through a
   function is rejected (one more shape in `r159`'s family; fold it
   into `r165`'s file only if the diagnostic is the same — else a
   fifth reject entry, `r168`).
