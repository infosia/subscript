<!-- §66 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 66. Emitted C identifiers — two spaces, never one

Origin: the R37 phase review found one collision between a declared
member and a symbol the emitter derives by suffix (§65 review note).
The audit that followed found the same defect class at function
scope, where it is **silent**. Owner decision 2026-08-25 to close the
class, not the instance. This is not a downstream request, and no
language surface moves.

Measurements at `a2228d9`, on this host. Every one is pre-existing;
§65 introduced none of them.

1. A parameter named `_t0` makes the two tiers disagree with no
   diagnostic. The program adds `g(i) + _t0` in a loop. The dev tier
   prints `306`. The ship tier prints `12`. A parameter named `_t1`
   prints `306` and `8`. The emitted body is:

   ```c
   static int32_t subscript_fn_f(void* ctx, int32_t _t0) {
       int32_t total = 0;
       {   int32_t i = 0;
           ...
           {   int32_t _t1 = total;
               int32_t _t0 = subscript_fn_g(ctx, i);
               total = ((_t1 + _t0) + _t0);
   ```

   `fresh_tmp` mints `_t0` inside a nested block. C block scoping
   makes it shadow the parameter, so the second `_t0` reads the
   temporary. Two declarations in one scope are a C error; two in
   nested scopes are silent.
2. An async method `x` beside a method `x_resume` stops the C
   compiler: "redefinition of 'subscript_m0_x_resume'". Both are
   file-scope definitions, so this one is loud. It stays loud when
   the two signatures are identical.
3. A method parameter named `_this` stops the C compiler:
   "redefinition of parameter '_this'".
4. `ctx` is already safe: `is_c_keyword` holds the C keywords and
   `ctx`, and a colliding name takes a trailing `_` (`ctx_`,
   measured). The mechanism exists. The list holds one entry that is
   not a C keyword.
5. The dev tier is immune. `codegen/src/lower/mod.rs` names a method
   `subscript_m{ci}_{mi}` by index. Only the C emitter mangles by
   name, so every divergence here is one-sided.
6. Audit of every symbol constructor in `codegen/src/cemit.rs`:
   `_resume` is the only symbol built from a source name by suffix.
   The lambda, bridge, worker-entry, constructor, and string-alias
   symbols are index-formed. `subscript_opaque` is emitted only for
   a class that declares no field.
6a. *(Corrected 2026-08-25 after the second phase review. The audit
   of item 6 read the function symbols and missed the frame **type**
   names.)* A coroutine frame type is built from a source name by
   **prefix**: `Async_m{class}_{method}` and `Async_{function}`, and
   the same two for `Gen_`. They share one C namespace. Measured: a
   free async function named `m0_x` beside an async method `x` on
   class index 0 both give `Async_m0_x`, and the C compiler stops
   with "redefinition of 'Async_m0_x'". Loud.
6b. A local inside a coroutine body resolves through `gen_locals`,
   which `local_ref` scans before every other stack. Such a local
   has no C identifier, so it is a rule 3a local. `emit_for_of`
   restores `gen_locals`; `emit_block` and `emit_for` do not.
   Measured, silent: an `async` function with `const s: string =
   "outer"` and an inner `const s: string = "inner"` prints `inner`
   then `outer` on the dev tier and `inner` then `inner` on the ship
   tier.
6c. An **unmanaged** local that shadows an outer **managed** local
   of one name does not mask it. `emit_let`'s unmanaged arm records
   no entry in `managed_scope`, while `emit_for_of_binding` records
   one for exactly this reason, so the outer entry wins the reverse
   scan for the rest of the block. Measured: `const s: string =
   "outer-string"` with an inner `const s: i32 = 5` prints
   `inner=5` then `outer-string` on the dev tier, and the ship tier
   stops with "incompatible pointer to integer conversion". The
   silent form of the same defect needs two types that are both
   `void*` in C.
6d. *(Third review, 2026-08-25.)* Two emitter defects survive that a
   scope restore cannot reach, because the emitter opens no C block
   where the language opens a scope. Both are on `tsc`-clean
   programs, so both are legal subscript programs that the dev tier
   runs and the C compiler rejects.
   - `emit_for_of` emits the loop binding and the body into one C
     block. A body local that shadows the binding is a redefinition.
     Measured: `for (const v of xs) { const v: i32 = 100; print(...)
     }` prints `100` three times on the dev tier, and the C compiler
     stops with "redefinition of 'v_v'".
   - `emit_lambda_fn` saves and restores the per-function stacks but
     not `assoc_iters`, which only `begin_fn` clears and a lambda
     never calls. A lambda created inside a `Map` or `Set` `for...of`
     emits the enclosing function's iterator handle in its own
     `return`. Measured: the dev tier prints `a10` and the C
     compiler stops with "use of undeclared identifier".
6f. *(Fourth review, 2026-08-25.)* A `for...of` **over a generator**
   reproduces the first defect of 6d, in the one `for...of` lowering
   `emit_for_of` never sees. `check_for_of` desugars that form to a
   `While` whose body holds the step, the break test, the binding,
   and the loop body in one flat HIR block, so both tiers emit one C
   block for all of it, while the checker gives the binding one
   scope and the body another. Measured, `tsc`-clean: `for (const v
   of numbers()) { const v: i32 = 100; print(...) }` prints
   `gen-for-of-body:100` twice on the dev tier, and the C compiler
   stops with "redefinition of 'v_v'". The seven fused `for...of`
   kinds — array values, array keys, `FixedArray` values, `Map`
   keys, `Map` values, `Set` values, and string code points — all
   agree across the tiers; the generator-driven form is the eighth
   and the only one that fails.
6g. *(Fifth review, 2026-08-25. The pairing is now exhaustive.)* The
   review paired every scope opener in the checker with every block
   site in the emitter. Checker: `ast::Stmt::Block`, `check_branch`
   (if-then, if-else, while body, `for` body, `for...of` body), the
   `for` head, the `for...of` binding, the `switch` case, and the
   lambda body. Emitter: the function bodies, `if`, `Block`,
   `while`, the `for` body, the `for...of` body, the `switch` case,
   the lambda body, and the coroutine resumes. Every pair opens a C
   block except two: the `switch` case, which measurement 6e records
   as an owner decision, and the lambda body.
6h. `emit_lambda_fn` materializes the capture copies and the body's
   own top-level locals in one C block, while the checker gives the
   lambda body its own scope and resolves a capture across that
   boundary. A body local whose name equals a capture is therefore a
   C redefinition. Measured, `tsc`-clean: a lambda that captures `n`
   from an enclosing `const n: i32 = 1`, holds an inner lambda
   reading the capture, and then declares `const n: i32 = 2`, prints
   `3` on the dev tier, and the C compiler stops with "redefinition
   of 'v_n'". This is the last unbraced pair of 6g.
6i. *(Sixth review, 2026-08-25. Recorded, not fixed here.)* The
   program of 6h reaches that collision only because name resolution
   diverges from TypeScript. The checker declares a name when it
   checks the declaration statement, so a lambda nested inside the
   body resolves the name outward to the enclosing binding.
   TypeScript and JavaScript block-scope the body, so the body's own
   `const` owns every reference in that body. Measured: `node`
   prints `4` and both subscript tiers print `3`. Stock `tsc`
   accepts the program, because the read sits inside a nested
   closure; a direct read reports TS2448. Under a TypeScript-faithful
   resolver the body binding owns the name, no capture of it is
   recorded, and the 6h collision cannot arise. The emitter fix
   stands on its own and is still correct. **Consequence for the
   corpus: no accept entry pins this shape.** The corpus is the
   language's executable definition, and a golden here settles
   a semantic divergence from TypeScript that no owner decision
   covers. The emitter unit test pins the C block without pinning a
   value. Name resolution needs its own request and its own owner
   decision.

   **Closed 2026-08-29 (owner decision).** §67.1 rule 4 rejects
   the read whether it is direct or inside a nested lambda, so the
   silent value is gone: measured at `c426ee7`, the program of 6h
   fails with S100 at the read. What remains is a narrowing: `tsc`
   accepts a lambda that reads a `const` declared later in its block,
   and `node` prints the later value when the call follows the
   declaration. Accepting it needs capture by reference, because C5's
   by-value capture at the literal has no value to take before the
   declaration runs. That is a revision of C5 and of §68.2 rule 8a,
   recorded here as the alternative and not taken. The rejection
   stands, and the diagnostic names the read.
6j. *(Sixth review, 2026-08-25. Recorded, not fixed here.)* The
   checker reports no duplicate declaration in one scope. Measured:
   `function f(n: i32): i32 { const n: i32 = 7; return n; }` prints
   `7` on the dev tier and stops the C compiler with "redefinition
   of 'v_n'"; two `const n` in one block, in a constructor, and in a
   method behave the same; the `async` form reaches an internal
   lowering error on the dev tier. Stock `tsc` rejects every one
   (TS2300, TS2451), so none is a valid subscript program under
   invariant 5. This is the bucket of measurement 6e — a
   checker-acceptance gap whose fix can move diagnostics — and it
   belongs to the same follow-up cycle.
6e. *(Third review, 2026-08-25. Recorded, not fixed here.)* The
   checker gives each `switch` case its own scope; TypeScript gives
   the whole switch body one scope. A program that declares a name
   in one case and reads it in another is accepted here and rejected
   by stock `tsc` (TS2454), which breaks invariant 5, and the flat C
   block then makes the two tiers disagree with no diagnostic.
   Measured: `case 1` prints `case1:1` on the dev tier and
   `case1:99` on the ship tier. The emitter is not the defect.
   Bracing each case in C is correct under only one of the two
   scope rules, so this section changes nothing here.

   **Owner decision 2026-08-25: the checker moves to the TypeScript
   rule — one scope for the whole `switch` body — in its own cycle,
   after §66 lands.** Two questions that cycle must settle from
   measurement, not from this note: `tsc` rejects the cross-case
   read with TS2454, which is a definite-assignment analysis this
   compiler may not have, so the cycle decides between implementing
   that analysis and taking a narrower rule that rejects a
   cross-case read outright; and once the body is one scope, the
   per-case scope restore in the emitter must go, because a name
   declared in one case is then legally in scope in a later one.
7. The emitter's own function-scope identifiers are `ctx`, `_this`,
   `_frame`, `_out`, `_f`, `_t{n}` (`fresh_tmp`), and `_L{n}`
   (`fresh_label`). A coroutine frame struct holds `_state`,
   `_this`, and `g{i}`.
8. The language permits shadowing in a nested block (measured: the
   inner `const x` prints `2`, the outer prints `1`). For a local
   that becomes a C variable, the emitter reproduces it with C block
   scoping: `local_ref` maps a name to `sanitize(name)` and tracks
   no scope. A fix must keep the C name a function of the source
   name alone.
8a. *(Corrected 2026-08-25 after the phase review. Measurement 8 was
   taken on an `i32` local and generalized to every local, which is
   wrong.)* A **managed** local — a string, a reference class, or an
   aggregate that holds a handle — has no C identifier. It lives in
   a shadow-frame slot, and `local_ref` resolves it by a reverse
   scan of `managed_scope`. `emit_block` saves and restores nothing,
   so an inner binding masks the outer one for the rest of the
   function and C block scoping never applies. Measured: `const s:
   string = "outer"` with an inner `const s: string = "inner"`
   prints `inner` then `outer` on the dev tier, and `inner` then
   `inner` on the ship tier, with no diagnostic. A reference-class
   local prints `2` then `1`, and `2` then `2`. `emit_for_of`
   already truncates both stacks on exit; `emit_block` does not.
8b. The lambda environment struct is a C namespace with no table.
   Its member is `sanitize(capture)` at the declaration, the store,
   and the read. Measured: a lambda that captures `a$b` and
   `a_dollar_b` emits `typedef struct { int32_t a_dollar_b; int32_t
   a_dollar_b; } EnvL0;` and the C compiler stops with "duplicate
   member". This one is loud.

### 66.1 Rule

1. **Every identifier the C emitter writes belongs to exactly one of
   two spaces.** Source space holds an identifier derived from an
   HIR name. Emitter space holds an identifier the emitter mints. No
   identifier belongs to both.
2. A function-scope source identifier — a parameter or a local —
   takes the prefix `v_`. The emitter mints no function-scope
   identifier that starts with `v_`. This separates the two spaces
   by construction where a collision is silent.
3. Within one function, two distinct source names take distinct C
   identifiers. The emitter holds one table per function over the
   parameter names and every local name in the body, in declaration
   order, and appends the smallest free `_N` on a collision, as §65
   rule 10. Two bindings of one source name keep one C identifier;
   C block scoping then reproduces the shadowing the language
   permits (measurement 8).
3a. **A local that has no C identifier obeys the same shadowing.**
   *(Added 2026-08-25 after the phase review; measurement 8a.
   Widened after the second review; measurements 6b and 6c.)* A
   managed local lives in a shadow-frame slot and a coroutine local
   lives in a frame field, so C block scoping cannot apply to
   either. The emitter's own scope bookkeeping supplies it. Every
   site that opens a lexical scope — `emit_block`, `emit_for`, and
   `emit_for_of` — records the lengths of **both** name stacks on
   entry, `managed_scope` and `gen_locals`, and truncates to them on
   exit. *(`local_types` was a third stack until the second review
   found it write-only; it is deleted.)* `shadow_cursor` and the frame let
   cursor stay monotonic, so no restored scope reads a stale slot.
3a-i. **Every binding masks a same-named binding from an outer
   block, whatever its storage.** An unmanaged local records an
   entry in `managed_scope` beside its C name, as
   `emit_for_of_binding` already does, so a `string` outside and an
   `i32` inside resolve to the inner one. Rule 3a's restore is what
   makes the mask end with its block.
3b. The lambda environment struct is one namespace and takes one
   table. *(Added 2026-08-25 after the phase review; measurement
   8b.)* The member declaration, the store in the creating frame,
   and the read in the lambda body all read it.
4. A symbol the emitter derives from a source name — by suffix or
   by prefix — takes its identifier from the table of the name it
   derives from, or drops the source name entirely for an index.
   *(Widened 2026-08-25 after the second review; measurement 6a. The
   coroutine frame types `Async_...` and `Gen_...` are
   prefix-derived and reached the same collision. A frame type takes
   the index form `Async_m{class}_{method index}`, which matches the
   dev tier's own convention and removes the source name from the
   derivation.)* The
   async **method** resume symbol is the only one:
   `{name}_resume` enters the class method table as a synthetic
   entry beside `{name}`, and the §65 rule 10 `_N` logic resolves a
   collision with a declared member. *(Clarified 2026-08-25 after
   the phase review.)* A free function's resume symbol is
   prefix-formed, `subscript_resume_{name}`, and no source name can
   reach that spelling, so it keeps it and takes no synthetic entry.
5. A coroutine frame struct is one namespace. `_state`, `_this`, and
   `g{i}` are emitter space. A parameter member is source space and
   takes rule 2's prefix.
6. Nothing else moves. A file-scope symbol keeps its `subscript_`
   prefix and its §65 rule 10 table. A class field member keeps its
   spelling and its §65 table. A module global keeps `g_` and its
   table. `is_c_keyword` keeps its list and its trailing `_`.
7. The rule is about emitted C only. No language surface and no
   diagnostic changes, and a source identifier keeps every spelling
   the language accepts today. *(Amended 2026-08-25 after the fourth
   review; measurement 6f.)* One HIR shape changes: `check_for_of`
   wraps a generator-driven loop body in `hir::Stmt::Block`, so the
   body is one scope in the HIR as it already is in the checker's
   own scopes. Both tiers and `rewrite_using_scope` already handle
   that statement, so `break`, `continue`, `return`, and scope-exit
   disposal keep their behaviour. No diagnostic and no accepted or
   rejected program moves.

§65 rule 10 gave each C namespace a table over HIR names. This
section adds the two cases a table over HIR names cannot see: an
identifier the emitter mints, and an identifier it derives.

### 66.2 Changes by site

- `codegen/src/cemit.rs` `emit_let` and `local_ref`: a local's C
  name comes from the per-function table of rule 3, with rule 2's
  prefix. `local_ref` keeps its shadow-frame and generator-frame
  arms, which name no C identifier of their own.
- `codegen/src/cemit.rs` `Emitter::new` and the per-function setup:
  the parameter table of §65 grows to cover the locals. `walk_lets`
  already collects the body's `let` names for the coroutine frame;
  the same walk builds the table.
- `codegen/src/cemit.rs` the coroutine frame emission: a parameter
  member takes rule 2's prefix, so it cannot collide with `_state`
  or `g{i}`.
- `codegen/src/cemit.rs` the method and function tables: an async
  member adds the synthetic `{name}_resume` entry of rule 4, and the
  resume signature and every use read the table.
- `compiler/src/check/stmt.rs` (`check_for_of`, the generator
  branch): the loop body becomes one `hir::Stmt::Block` instead of
  a flat extend (rule 7, measurement 6f).
- No runtime, prelude, or dev-tier change, and no other checker
  change.

### 66.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: measurements 1, 2, and 3 recorded
with their outputs (this host).

1. `corpus/accept/a145-emitted-identifiers.ts` + `.expected`: a
   program whose parameters and locals are named `_t0`, `_t1`,
   `_this`, `_frame`, `_out`, `_f`, `_state`, `g0`, `_L0`, and
   `ctx`, read inside nested blocks and inside a loop so the
   emitter's temporaries interleave with them; an async method `x`
   beside a method `x_resume`; an async function `f` beside a
   function `f_resume`; and an async function whose parameters are
   named `_state` and `g0`, so the frame struct exercises rule 5.
   Byte-exact across dev JIT, ship C-AOT, and the golden. The entry
   is `tsc`-clean, like every accept entry.
2. Unit tests in the same commit: a parameter and a local carry the
   rule 2 prefix in emitted C; the emitted C for measurement 1's
   program declares no name that equals a parameter name; the
   synthetic resume entry resolves against a declared `x_resume`;
   the per-function table gives distinct names to `a$b` and
   `a_dollar_b` as two locals; the frame struct of an async function
   with a parameter named `_state` holds two distinct members. Added
   2026-08-25 after the phase review: an environment struct for a
   lambda that captures `a$b` and `a_dollar_b` holds two distinct
   members; `emit_block` restores both scope stacks, so a managed
   local declared in a nested block does not mask the outer one.
2a. Corpus entries added 2026-08-25 after the phase review, in
   `corpus/accept/a146-scoped-locals.ts` + `.expected`: a `string`
   local and a reference-class local, each shadowed in a nested
   block and read again after it; a shadowed managed local inside a
   loop body and inside an `if` branch; and a lambda that captures
   `a$b` beside `a_dollar_b`. Byte-exact across dev JIT, ship
   C-AOT, and the golden. This entry is the gate for rules 3a and
   3b; without it the corpus does not see them.
2b. Widened 2026-08-25 after the second review, because the item 2a
   list is what missed measurements 6b and 6c. `a146` also holds:
   the same shadowing inside a generator body and inside an `async`
   body across a suspension; a shadowed local in a `switch` case
   and in a `for...of` body; an unmanaged local that shadows an
   outer managed local of one name, and the reverse; and a free
   async function named `m0_x` beside an async method `x` on the
   first class, which pins measurement 6a. Every construct that
   opens a lexical scope appears at least once.
2c. *(Third review, 2026-08-25.)* Item 2b's shapes must appear
   **outside** a coroutine as well. In the entry as first written,
   the `switch` case and the `for...of` body sat inside a generator,
   where every local is a frame field — the one storage class that
   shows neither defect of measurement 6d. `a146` must hold, outside
   any coroutine: a `for...of` body that declares a local shadowing
   the loop binding; a `for` **body** declaration, not only a
   shadowed initializer; a lambda **body** with its own locals,
   including a lambda created inside a `Map` or `Set` `for...of`; a
   constructor body, a method body, and an accessor body; and a
   `using` scope. A construct that no corpus entry can express,
   because `tsc` rejects it, is named in measurement 6e instead.
2d. *(Fourth review, 2026-08-25.)* `a146` also holds a `for...of`
   **over a generator**, outside any coroutine, whose body declares
   a local that shadows the loop binding. The seven fused `for...of`
   kinds were all measured to agree; the generator-driven form is a
   separate lowering and needs its own line in the entry.
2e. *(Fifth review, 2026-08-25; corrected by the sixth,
   2026-08-25.)* `a146` holds, outside any coroutine, a shadowed
   local inside one `switch` case, which item 2b named and item 2c's
   list left out. It does **not** hold a lambda body local that
   shadows one of the lambda's own captures: measurement 6i shows
   that program's value depends on a name resolution that diverges
   from TypeScript, so a golden would settle that divergence without
   a decision. The emitter unit test
   `lambda_body_uses_a_nested_c_scope_below_capture_copies` pins the
   C block of measurement 6h instead, with no value. A shadow inside one case is expressible and was
   measured to agree on both tiers; only the cross-case read of
   measurement 6e is not expressible.
3. Counts: accept `.ts` 143 → 145; `.expected` 144 → 146; accept
   source files 145 → 147. Rejects do not move. The generated docs
   regenerate.
4. **Every committed golden and `.expected` stays byte-identical**,
   `a145` excepted as a new entry. Rule 2 moves emitted C text, not
   program output. The `codegen/tests/cemit.rs` assertions that pin
   body text move with it, in the same commit; that is expected, and
   each move must be a prefix change and nothing else.
5. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; clippy
   library counts at the 7 / 22 / 29 baseline. The record quotes the
   test count and the wall time.

### 66.1 The emitted spelling is not an interface

*(2026-08-28. A consumer asked whether `SubC{id}` and `d{id}` are
contract, because it compiles a C probe that names both.)*

**They are not, and no consumer must name them.**

§68 requires that a C identifier derives from a LIR id, so that no
source name reaches the C namespace. That rule is what closes §66's
collision class. The spelling that satisfies it is a consequence, not
a promise.

`cemit.rs` builds a class name as `format!("SubC{}", id.0)`, and
`ClassId` is `pub struct ClassId(pub usize)` — an index into the
module's class table. Adding, removing, or reordering a class moves
every later id, so the same source class takes a different C name in a
different program. The spelling is stable for one compilation, and for
nothing wider.

**This project names no emitted identifier to prove anything.** §12.3's
`offsetof` proof compiles its probe against the C header's own type
names, and compares the result against the layout the compiler
computed. It never reads emitted C.

**A consumer proves the same property the same way.**
`subscript_codegen::layout::value_class_layouts` is public. It returns
one `StructLayout` per `@CStruct` class — `name`, `size`, `align`, and
one `FieldLayout { name, offset }` per field — keyed by the **source**
class and field names. The caller compares those against
`sizeof`/`_Alignof`/`offsetof` taken from its own header. Both sides of
that comparison are names the consumer controls.
