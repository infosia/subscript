<!-- §57 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 57. R27 — field initializers run on every construction

*(A field with **no** initializer and no unconditional constructor
assignment is rejected since 2026-09-12: `compiler.md` §108.)*

A downstream report (R27, 2026-08-15): a `@CStruct` class with no
constructor and a field initializer `value: i32 = 37` prints
`field:37` on the dev tier and `field:0` on the ship tier. Both
tiers compile the program clean, and no trap fires. The downstream
avoids the shape with a generator rule and escalates the silent
divergence.

Measured here at `b1a5dab`, with `run_jit` / `run_c_aot`:

1. Value class, no constructor, `value: i32 = 37`:
   dev `field:37`, ship `field:0`. The downstream shape.
2. Reference class, no constructor, `value: i32 = 41`:
   dev `field:41`, ship `field:0`.
3. Constructor present, an initializer that calls a print helper,
   an argument that calls another: dev prints `init runs` before
   `arg runs`; ship prints `arg runs` before `init runs`.
4. `this` in a field initializer: the checker accepts it; the dev
   tier fails with `internal lowering error: `this` outside a
   method`, with or without a constructor; the ship tier runs it.

Cause, by site:

- The C emitter runs field initializers only inside the emitted
  constructor (`emit_constructor`, `codegen/src/cemit.rs`). For a
  class with no constructor, value `new` lowers to a zero literal
  and reference `new` to a bare allocation. Findings (1), (2).
- The Cranelift lowering (`eval_new`,
  `codegen/src/lower/func.rs`) runs the initializers at the
  construction site, before it evaluates the constructor
  arguments. The C emitter evaluates the arguments first.
  Finding (3).
- The checker (`check_class_body`, `compiler/src/check/mod.rs`)
  checks a field initializer in a context with a `this` binding.
  Neither tier defines that lowering. Finding (4).

Under `node`, the same source prints `arg runs` before
`init runs`, and a constructor-less class carries the initialized
value (measured, exit 0). The ship tier's with-constructor order
is the TS order; the dev tier's no-constructor result is the TS
result. Each tier is correct where the other is wrong.

### 57.1 Rule

For `new C(...)` of a non-boundary, non-descriptor class, both
tiers observe this order:

1. The argument expressions evaluate left to right.
2. The construction zero-initializes the instance.
3. The declared field initializers run in declaration order, once
   per construction, with or without a declared constructor.
4. The parameter defaults of the absent arguments evaluate left to
   right, with `this` bound to the instance. *(Added 2026-09-12 by
   the §108.4 Phase Review. Both tiers evaluated a default before
   step 3, so `constructor(n: i32 = this.count)` with `count: i32 =
   5` read `0`; `node` prints `5`: ECMA runs the field initializers
   before the parameter defaults.)*
5. The declared constructor body runs after the defaults.

Steps 2–5 are ordered against step 1 only where observable: no
argument side effect interleaves with an initializer, a parameter
default, or the constructor body.

A field initializer must not read `this`. The checker checks each
field initializer in a context with no `this` binding, so `this`
there gets the standing S100 diagnostic "`this` is only available
in constructors and methods". No program that uses it runs on the
dev tier today (finding 4), so nothing regresses.

### 57.2 Changes by site

- C emitter, class with no constructor and one or more
  initializers: value `new` materializes a zeroed temporary, runs
  the initializers into it, and yields it; reference `new` stores
  each initializer after the allocation. The with-constructor path
  stands — it already runs arguments, then initializers, then the
  body.
- Cranelift `eval_new`: the constructor arguments evaluate before
  the field initializers run. The no-constructor path stands.
- Checker `check_class_body`: the field-initializer context
  carries no `this` type.

Out of scope, unchanged: boundary-struct positional `new` (an
ambient mirror declares no field initializer) and Q33 descriptor
defaults (§25, §43).

### 57.3 What does not change

- Every pre-existing golden stays byte-identical. The green
  differential gate at `b1a5dab` proves no committed entry
  observes the missing initializers or the order.
- A with-constructor program whose initializers have no side
  effects keeps its bytes on both tiers.

### 57.4 Exit criteria (pre-registered)

1. Record the red first, at `b1a5dab`: the two new accept entries
   each produce different bytes on the two tiers; the new reject
   entry passes the checker.
2. New accept entry `a133-field-init-no-ctor`: a `@CStruct` value
   class and a reference class, each with no constructor and a
   non-zero field initializer; `main` prints the fields. The entry
   passes the checker gate, the `tsc` gate, and the
   tier-differential gate with a committed golden.
3. New accept entry `a134-field-init-order`: a class with a
   constructor, an initializer that calls a print helper, and a
   constructor argument that calls another. The golden pins
   `arg runs` before `init runs`.
4. New reject entry `r126-this-in-field-init`: `this` in a field
   initializer, S100, registered in `corpus_reject.rs`.
5. Unit tests: the emitted C for a constructor-less initialized
   value class contains the initializer store, and the same for a
   reference class; the checker rejects `this` in a field
   initializer with S100.
6. The workspace gate passes in both profiles; every pre-existing
   golden stays byte-identical; `cargo fmt --check` and the `tsc`
   gate exit 0.
