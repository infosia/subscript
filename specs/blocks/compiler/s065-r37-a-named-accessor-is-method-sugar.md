<!-- §65 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 65. R37 — a named accessor is method sugar

Origin: downstream request R37, 2026-08-25, at pin `f99d4cb`. The
downstream compiles typed HIR to WGSL. Its surface reinterprets
TypeGPU, which reads a binding, an address-space variable, and a
vector swizzle through a property. subscript has no accessor, so
the downstream writes a call at 49 authored sites and in 3 library
classes. §58 already decided this shape for one sugar: the checker
rewrites the spelling to a call of a declared method, no new HIR
form exists, and no tier changes. R37 asks for the same treatment
of a named accessor.

Measurements at the pin, on this host:

1. The four probes reproduce. A `get`/`set` pair reports S100
   "static methods and accessors are not decided" once per
   accessor, then S004 and S100 at each use. A read accessor on a
   `@CStruct` value class reports the same S100. A static method
   reports the same S100. A method type parameter reports S100
   "unknown type name `T`". One message covers static members and
   accessors together.
2. Mirror ingestion rejects an accessor through that same shared
   message (`resolve_class_shape` runs for boundary classes). A
   split is therefore necessary, not optional.
3. The `$` collision is a live tier divergence, not a latent one.
   A class that declares methods `$` and `_` runs on the dev tier
   and prints `1,2`. The ship tier emits two definitions of
   `subscript_m0__` and the C compiler stops: "redefinition of
   'subscript_m0__'" (Apple clang). No corpus entry covers it. No
   identifier in `corpus/`, `examples/`, or `prelude/` holds a `$`
   today, so a new escape moves no emitted C.
4. `hir::DISPOSE_METHOD_NAME` is `"[[Symbol.dispose]]"`. A reserved
   HIR method name that no source identifier can spell is already
   the practice.
5. Stock `tsc` 5.9.2 accepts the whole asked accept surface: a
   `get`/`set` pair, a second accessor on the same class, a read
   accessor on a `@CStruct` class, an accessor on a generic class,
   and `$` and `_` members together. It also accepts `x.v += 1`,
   `x.v++`, the write used as a value, a static accessor, and a
   write accessor on a value class; each subscript rejection below
   is therefore a narrower pin. It rejects a write through a
   read-only accessor (TS2540) and a field that shares an accessor
   name (TS2300).
6. `private` fields check today. A synchronous method on a
   `@CStruct` value class checks and runs.
7. The reject harness checks one script file and cannot carry a
   mirror. The mirror rejection is a unit test, as §58.2 did.

### 65.1 Rule

1. A class declares a read accessor `get name(): T { ... }` and a
   write accessor `set name(value: T) { ... }`. Both carry a body.
   A read accessor declares no parameter and an explicit return
   type. A write accessor declares one parameter with an explicit
   type, no default on that parameter, and no return type. A return
   type on a write accessor fails with S100; stock `tsc` rejects it
   too (TS1095). A default on the parameter fails with S100; `tsc`
   rejects it too (TS1052). *(Amended 2026-08-25 after the phase
   review: the first text left both spellings unstated, and the
   implementation accepted a return type, which broke the
   `tsc`-subset invariant.)*
1a. The pair shares one type. The read accessor's return type and
   the write accessor's parameter type must be the same type. A
   mismatch fails with S100 at the write accessor. Stock `tsc`
   accepts unrelated types, so this is a narrower pin. *(Added
   2026-08-25 after the phase review: rule 1 wrote one `T` but
   nothing enforced it, so the written value took its context from
   the read accessor's type and a valid write reported S008.)*
2. A read accessor is legal on a reference class and on a
   `@CStruct` value class. A write accessor is legal on a reference
   class only. A write accessor on a value class fails with S100
   that names the value class. A value class copies on assignment,
   so the write reaches a copy.
3. An accessor adds no member kind and no HIR form. The class
   member namespace holds `name` once: a field, a method, or an
   accessor pair owns it. A second declaration of the name fails
   with S100 that names both member kinds. A class declares at most
   one read accessor and at most one write accessor of one name; a
   second one of either kind fails with S100 that names two
   accessors. *(Amended 2026-08-25 after the phase review: a second
   write accessor passed the first text's clash test, overwrote the
   first signature, and reached an internal lowering error.)*
4. The pair records as two ordinary methods. The read accessor
   records as the method `name` with no parameters. The write
   accessor records as the method `name=` with one parameter and
   the return type `void`. An identifier holds no `=`, so neither
   name collides with a declared method. *(This is the one
   divergence from the request, which asks for one method. A class
   method table holds one signature per name, and both tiers key a
   method by its name; two signatures under one name collide. The
   read call HIR is exactly the HIR of `x.name()`, and the write
   call HIR is exactly the HIR of `x.name=(v)`.)*
5. A read `x.name` checks to the same HIR as a call of the read
   accessor. A write `x.name = v` in statement position checks to
   the same HIR as a call of the write accessor. The value checks
   against the write accessor's parameter type by ordinary
   assignability (S007 on a mismatch).
6. A read accessor without a write accessor is legal. The write
   spelling then fails with S100 at the assignment. A write
   accessor without a read accessor fails with S100 at the
   declaration, because the read spelling has no target.
7. The write used as a value is rejected with S100 and a message
   that names the spelling. §58.1 rule 7 rejects the same for an
   index signature. *(Amended 2026-09-02, §82.1: compound
   assignment, increment, and decrement on `x.name` now rewrite to
   the read-then-write form.)*
8. An accessor in a mirror class keeps its rejection with its own
   message. *(Amended 2026-08-29 by §71 and 2026-09-02 by §82.5: a
   static accessor is legal, a static read accessor with no write
   accessor included.)*
9. An accessor on a generic class is legal. The checker checks each
   accessor body at instantiation, as it checks every other member
   body (§64.1 rule 1).
10. **Two distinct HIR names that share a C namespace must have
    distinct C identifiers.** `sanitize` in `codegen/src/cemit.rs`
    gains two escapes: `$` becomes `_dollar_`, and `=` becomes
    `_set_`. Every other character keeps today's mapping. An escape
    is not enough on its own, because a C identifier holds only
    `[A-Za-z0-9_]`, so every escape text is itself a legal source
    identifier. The emitter therefore holds one table for each C
    namespace it names into: the methods of one class, the fields
    of one class, the module's functions, the module's globals, and
    the parameters of one function. The first name keeps
    `sanitize`'s output. A later name whose output is already taken
    gains the smallest free `_N` suffix, with `N` from 2. The order
    is the HIR declaration order, so the assignment is
    deterministic, and a name that does not collide keeps its
    current C spelling. *(Amended 2026-08-25 after the phase review.
    The first text defined the escapes alone and accepted the
    residual. Measured: `get v` / `set v` beside an ordinary method
    `v_set_` runs on the dev tier and stops the C compiler with
    "redefinition of 'subscript_m0_v_set_'" — the divergence of 65
    item 3, reachable with no `$` at all.)* *(§66, 2026-08-25: a table over
    HIR names does not see an identifier the emitter mints or
    derives. §66 closes those two cases.)*
11. Nothing else moves. Fields, methods, and index signatures keep
    their rules. Mirror ingestion reads no accessor. *(Static
    methods: §71. Method type parameters: §82.4.)*

`collisions.md` gains C12: JS runs an accessor on property access;
subscript calls the declared method.

### 65.2 Changes by site

- `compiler/src/check/mod.rs` (`resolve_class_shape`): the method
  arm splits the shared message. A static member reports "static
  methods and accessors are not decided" for a static accessor and
  keeps its other texts. An accessor in a boundary class reports
  its own S100. Every other accessor collects: the read accessor
  under `name`, the write accessor under `name=`. The arm validates
  the parameter count, the annotations, rule 2, rule 3, and rule 6.
- `compiler/src/check/mod.rs` (`ClassSig`): one new checker-side
  field records the accessor names of the class. `hir::ClassDef`
  gains nothing.
- `compiler/src/check/mod.rs` (`check_class_body`): the accessor
  bodies check as method bodies, under the names of rule 4.
- `compiler/src/check/expr.rs` (`member_on`): a `Type::Class`
  receiver whose class declares a read accessor `name` rewrites the
  read to the call, for a read and for a write target alike. The
  rewrite runs after the field lookup and before the
  method-as-value error.
- `compiler/src/check/expr.rs` (`check_assign`): a target that is a
  no-argument call of an accessor name rewrites the plain write to
  the write-accessor call. The rule 6 and rule 7 rejections report
  here.
- `compiler/src/check/expr.rs` (`check_update`): `x.name++` and
  `x.name--` report the rule 7 message.
- `codegen/src/cemit.rs` (`sanitize`): the rule 10 escapes.
- `compiler/src/language_reference.rs`: one feature entry for the
  accessor; `generated-docs/` regenerates.
- No runtime, prelude, or lowering change.

### 65.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the S100 in 65 item 1 and the
ship-tier C error in 65 item 3, both recorded (this host).

1. `corpus/accept/a144-accessor.ts` + `.expected` (golden from the
   dev JIT; ship byte-identical): a reference class with a
   `get`/`set` pair over a private field, read, written, and read
   inside a template string, plus a second accessor on the same
   class; a `@CStruct` value class with a read accessor; a generic
   class with an accessor over its type parameter, used at two
   types; one class that holds both an accessor named `$` and a
   member named `_`; and, added 2026-08-25 after the phase review,
   one class that holds a `get`/`set` pair for `v` beside an
   ordinary method named `v_set_`, which pins rule 10 on the ship
   tier.
2. `corpus/reject/r141-value-class-write-accessor.ts`: S100 at the
   write accessor. `tsc`-clean, recorded in the header.
3. `corpus/reject/r142-readonly-accessor-write.ts`: S100 at the
   assignment.
4. `corpus/reject/r143-accessor-compound-assign.ts`: S100 at the
   compound write. `tsc`-clean, recorded in the header.
5. `corpus/reject/r144-accessor-increment.ts`: S100 at the
   increment. `tsc`-clean, recorded in the header.
6. `corpus/reject/r145-accessor-write-as-value.ts`: S100 at the
   write. `tsc`-clean, recorded in the header.
7. `corpus/reject/r146-accessor-field-name-clash.ts`: S100 at the
   second declaration.
8. `corpus/reject/r147-static-accessor.ts`: S100 at the accessor.
   `tsc`-clean, recorded in the header.
9. Unit tests in the same commit: an accessor in a mirror class
   fails with its own S100; a write accessor with no read accessor
   fails; a wrong-typed written value fails with S007; the checker
   produces identical HIR for `x.name` and for the spelled call of
   the read accessor; `sanitize` maps `$` and `=` to the rule 10
   escapes; the emitted C for a class with `$` and `_` members
   holds two distinct symbols and compiles. Added 2026-08-25 after
   the phase review: a return type on a write accessor fails; a
   default on the write accessor parameter fails; a read and a
   write accessor of different types fail; a second read accessor
   and a second write accessor each fail; the rule 10 table gives
   distinct C identifiers in each of the five namespaces.
10. Counts: accept `.ts` 142 → 143; `.expected` 143 → 144; rejects
    135 → 142; accept source files 144 → 145. The generated docs
    regenerate.
11. Gates: `cargo test --offline --workspace` in both profiles;
    zero-warning build; `cargo fmt --check`; the `tsc` gate; every
    pre-existing golden and `.expected` byte-identical; clippy
    library counts at the 7 / 22 / 29 baseline. The record quotes
    the test count and the wall time.
