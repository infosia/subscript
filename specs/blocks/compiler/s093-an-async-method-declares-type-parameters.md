<!-- §93 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 93. An async method declares type parameters

*(2026-09-08.)* Origin: the review of `REPORT.md` on 2026-09-08 named
the form as the one hole between two accepted features. §82.4 accepts
`recv.m<A>(...)`. §64 accepts `await f<A>(...)` and `await
recv.m(...)`. Only the combination is rejected. Stock `tsc` accepts
it. §82.4 rule 5 gives the reason as "the await grammar gains no
form", which is a statement about the checker, not about the
language.

Measurements at pin `61c64c3`, this host (aarch64 macOS, rustc
1.95.0, Apple clang 21.0.0):

1. An async method with type parameters on a non-generic reference
   class (r181): S100 "async generic methods are not in the decided
   surface" at the declaration, with a divergence block
   (`Divergence::AsyncGenericMethod`).
2. The rejection is at collection. A call without type arguments, a
   floating call, and a read of the method as a value all report that
   same declaration diagnostic. No call site reports.
3. The same method on a generic class: S100 "generic classes cannot
   declare generic methods" (§82.4 rule 5, first half). That check
   runs first.
4. `static async m<T>()`: S100 "async static methods are not in the
   decided surface" (§37.1, C8). That check runs first.
5. The same method on a `@CStruct` value class: S100 "async generic
   methods are not in the decided surface". The value-class rejection
   sits after the generic-method branch in `collect_class`, so the
   branch masks it.
6. A bodiless async method with type parameters in a `declare class`
   written in a `.ts` source: the same masked result. §82.4 rule 1a
   states "function bodies are required" for that construct.
7. The sync call path instantiates a generic method at the call
   (`instantiate_generic_method_call`). The await path has no such
   step: it rejects a type-argument list with "method `m` is not
   generic".

Items 5 and 6 are defects of the message, not of the verdict. Each
program is rejected today, and each one names the wrong rule.

### 93.1 Rule

1. An `async` instance method on a non-generic reference class
   declares type parameters, as §82.4 rule 1 has it for a sync
   method. The parameters are in scope in the signature and the body.
2. `await recv.m<A>(...)` instantiates the method and awaits the
   instance. The await grammar gains this one form. §64 rule 3 now
   reads: the accepted forms are `await f(...)`, `await f<A>(...)`,
   `await recv.m(...)`, and `await recv.m<A>(...)`. Async functions
   and methods stay non-first-class.
3. Each distinct type-argument list yields one instance (§82.4 rule
   3). The instance is an ordinary async method of the class,
   checked and lowered as a §37.2 async method. Every consumer sees
   the instance name `m<A>`.
4. A call without type arguments fails with S100 "generic method `m`
   requires explicit type arguments" and a divergence block, as §82.4
   rule 2 has it. The site is the call.
5. A call outside await position drops the handle. The instance
   exists first, so the diagnostic is S013 at the statement, as r100
   has it.
6. A handle held for a later await is legal, as §70 has it. The
   instance is an ordinary async method, so §70 needs no change.
7. A read of the method as a value fails with S100 "async method `m`
   is not a first-class value; call it directly in await position".
   The template carries `is_async`, so the read reports the async
   text, not the §82.4 rule 6 text.
8. These rejections do not change, and each one reports at its own
   site: an async generic method on a generic class (§82.4 rule 5,
   first half); an async generic **static** method (§37.1); an async
   generic method on a `@CStruct` value class (§37.1); an async
   generic **generator** method (§37.1); a `@Descriptor` class and a
   mirror class.
9. Items 5 and 6 of the measurements report the rule that names the
   construct. The value-class rejection runs before the
   generic-method branch. The bodiless rejection stays inside the
   branch and now reaches an async template.
10. `Divergence::AsyncGenericMethod` is deleted. No site rejects with
   it, and §79.1 forbids a variant that no diagnostic produces.
11. *(Amended 2026-09-08, after the round measured `tsc`.)* A
   bodiless async method with type parameters, in a `declare class`
   written in a `.ts` source, reports S100 "function bodies are
   required" with **no** divergence block. Measured with tsc 5.9.2:
   `declare class Box { async load<T>(value: T): Promise<T>; }` gives
   TS1040 "'async' modifier cannot be used in an ambient context",
   exit 2. The same shape without `async` exits 0. §79 rule 2 gives
   the divergence block to a rejection that `tsc` accepts, so §82.4
   rule 1a's `BodilessDeclareGenericMethod` variant stays with the
   sync form alone.
12. *(Added 2026-09-08, after the round measured a cascade.)* A
   collection rule that rejects a method with type parameters records
   the method as a rejected template, in the namespace that `static`
   selects. §82.4 rule 1a states the consequence: the method reports
   nothing more at a call that names it. Rule 9 moved the value-class
   rejection above the template branch, and the round measured the
   cascade that the move creates: r187 reports its S100 at the
   declaration and then S018 "`Vec2` has no method `load`" at the
   call. One construct reports one diagnostic. This rule holds for
   every collection rule that rejects a method with type parameters,
   not for the value-class rule alone.
12a. *(Added 2026-09-08, after the round measured the reach of rule
   12.)* One construct reports one diagnostic at its declaration too.
   A static method with type parameters on a generic class reports
   S100 "generic classes cannot declare static members" alone. It
   reported that rule and "generic classes cannot declare generic
   methods" together before this section. The first rule that names
   the construct reports; a later rule that the same declaration also
   breaks stays silent. A program that removes `static` then reports
   the generic-method rule.

### 93.2 Checker and lowering

- `compiler/src/check/mod.rs` `collect_class`: the `is_async`
  rejection inside the generic-method branch is removed. The
  `@CStruct` value-class async rejection moves above that branch
  (rule 9).
- `compiler/src/check/expr.rs`, the await path, member callee: when
  the receiver class declares a generic method of that name,
  `instantiate_generic_method_call` runs first, and the arm continues
  with the instance name. The "is not generic" rejection applies only
  when the class declares no template of that name.
- `compiler/src/check/expr.rs`, the member read path: the async text
  when the template is async (rule 7).
- `compiler/src/divergence.rs`: the variant and its table row are
  deleted.
- `compiler/src/language_reference.rs`: the Q34 text names the fourth
  await form; the corpus list replaces r181 with a187 and r186;
  `generated-docs/` regenerates.
- `codegen/tests/cemit.rs`: the async two-instance assertion sits
  beside the sync one,
  `generic_method_instances_hold_distinct_hir_names_and_lir_ids`.
  `compiler/tests/` does not depend on `subscript-codegen`, so the
  LIR half of that assertion cannot live there. *(Added 2026-09-08,
  after the round reported the crate boundary.)*
- No change in `codegen/src/`, in the tiers, or in the runtime. The
  instance is an ordinary async method in HIR.

### 93.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: measurement items 1 and 2, recorded
on this host with exit 1.

1. `corpus/accept/a187-async-generic-method.ts` + `.expected`: a
   non-generic reference class with `async load<T>(value: T):
   Promise<T>` that awaits `Context.suspend()` and returns the value.
   Two instances: `i32` and a `@CStruct` value class `Vec2` of two
   `f32` fields. One await is direct; one handle is held in a local
   and awaited two statements later (rule 6). `main` prints each
   result. The header carries `js-comparable: no C8`. The golden
   comes from the dev JIT; the ship tier and the interpreter match it
   byte for byte.
2. `corpus/reject/r186-async-generic-method-without-type-args.ts`:
   S100 at the call, with the divergence block (rule 4).
3. `corpus/reject/r187-async-generic-method-on-value-class.ts`: S100
   "async methods on `@CStruct` value classes are not in the decided
   surface" at the declaration (rule 9).
4. `corpus/reject/r181-async-generic-method.ts` is deleted, and its
   harness row is removed, as r104 was in §64 rule 7.
5a. *(Added 2026-09-08.)* One rejected declaration gives one
   diagnostic. For each collection rule that rejects a method with
   type parameters, a program that also calls the method reports
   exactly that rule, and no S018 and no "is not generic" at the call
   (rule 12). The rules to cover: the `@CStruct` value-class async
   rejection, the async static rejection, and the async generator
   rejection. The control is a program whose method no rule rejects,
   where the call reports nothing.
5. Unit tests in the same commit: a floating `box.load<i32>(1)` is
   S013 at the statement; a read of `box.load` is the async
   first-class S100; two type-argument lists give two HIR method
   names and two LIR function ids; an async generic method on a
   generic class keeps its S100; an async generic static method keeps
   its S100; a bodiless async generic method in a `declare class`
   reports "function bodies are required" with no divergence block
   (rule 11), beside a sync control that keeps the
   `BodilessDeclareGenericMethod` block.
6. Counts: accept `.ts` 184 → 185, `.expected` 185 → 186; reject
   `.ts` 174 → 175. The §88 index and `generated-docs/` regenerate
   and agree.
7. Gates: `tools/gate.sh full` green in both profiles; clippy at the
   7/18/13 baseline; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden and `.expected` byte-identical, with the one
   exception item 8 names.
8. *(Added 2026-09-08, after the round measured it.)* The aggregate
   LIR text snapshot `codegen/tests/lir-goldens/corpus.txt` gains
   a187's block. `codegen/tests/lir.rs`
   `coroutine_and_measurement_lir_text_matches_goldens` collects
   every async corpus entry, so a new async entry adds a block by
   construction. Capture the snapshot with
   `SUBSCRIPT_CAPTURE_LIR_GOLDENS=1`, and record the change under the
   §2 procedure. The evidence that no pre-existing block moves: with
   a187's block removed, the captured text is byte-identical to the
   committed snapshot. The §2 record names this one file, and the
   gate reports its move.
