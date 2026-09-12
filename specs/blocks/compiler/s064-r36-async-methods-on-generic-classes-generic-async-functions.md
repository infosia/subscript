<!-- §64 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 64. R36 — async methods on generic classes, generic async functions

Origin: downstream request R36, 2026-08-23, at pin `bb9dadc`. The
downstream wraps a `GPUBuffer` in a generic class `Buffer<T>`. The
typed read-back belongs on that class and awaits a map, so the
method is `async`. §37.1 rejects an async method on a generic class
template (r104), and §26.1 accepts `await f(...)` only for a
directly declared, non-generic function.

Measurements at the pin, on this host:

1. A generic class with `async read(): Promise<T>`: S100 "async
   methods on generic class templates are not in the decided
   surface" (`collect_class`, `compiler/src/check/mod.rs`).
2. `await first<u32>(items)` for a generic `async function
   first<T>`: S100 "`first` is not a directly declared async
   function" (the await path in `compiler/src/check/expr.rs`
   accepts `ScopeItem::Func` only).
3. A non-generic class with an async method: accepted, runs.
4. `instantiate_fn` checks each instance as a function named
   `first<u32>` and marks it `exported` when the template is
   exported. The ship tier emits a sync instance of an exported
   generic `f<T>(): void` as `subscript_export_f_u32_`, a host entry
   that exists only while the program instantiates it. The runner
   kicks every exported async function with no parameters as a
   root (`subscript_kick_async_exports`, both tiers); an exported
   async instance would run twice.
5. Stock `tsc` 5.9.2 accepts an async method on a generic class, a
   generic async function with explicit type arguments, and an
   async arrow function. It rejects `async constructor()` (TS1089);
   the parser rejects it too ("Constructor can't be an async
   function").

### 64.1 Rule

1. An `async` method on a generic reference class is accepted. The
   class type parameters are in scope in the body and in the
   `Promise<T>` annotation. The body obeys §26.1 and §37.1
   unchanged. The §37.1 rejections that remain (async static, async
   generator, async on a `@CStruct` value class, `@Descriptor`,
   floating call) apply to the instance at instantiation, where
   every generic body check runs.
2. A generic `async function` is accepted. A call requires explicit
   type arguments, as every generic call does (S100 "generic
   function `f` requires explicit type arguments" when absent). The
   call is legal only in await position; a floating call is S013,
   as r100.
3. `await` accepts the two new forms wherever §26.1 and §37.1 accept
   the non-generic forms: `await f<A>(...)` and `await recv.m(...)`
   with `recv` of an instantiated generic class type. The await
   grammar gains no other form; async functions and methods stay
   non-first-class. *(§93, 2026-09-08: `await recv.m<A>(...)`
   is a fourth form. Async methods stay non-first-class.)*
4. Each distinct type-argument list yields one instance, checked
   and lowered as a §26.2 async function or a §37.2 async method.
   The instance name is the monomorphized name (`first<u32>`); the
   ship tier sanitizes it as it does for sync instances.
5. An instance of a generic function is not a host entry and not an
   async root. `hir::Function.exported` is `false` on every instance,
   sync or async. The `export` keyword on a generic declaration
   affects module imports only. Both tiers emit no
   `subscript_export_*` symbol for an instance, and
   `subscript_kick_async_exports` kicks none.
6. Unchanged diagnostics: an async arrow function keeps S100 "async
   arrow functions are not in the decided surface; use an async
   function declaration"; `async constructor()` keeps the parse
   error.
7. r104 retires: the file and the harness row are removed, as
   `r14-async` in §26.1. §37.1's generic-class rejection and §26.1's
   "directly declared" wording are superseded by this section.

### 64.2 Checker and lowering

- `compiler/src/check/mod.rs` `collect_class`: the async-method
  rejection on generic templates is removed.
- `compiler/src/check/expr.rs`, the await path, identifier callee:
  `ScopeItem::GenericFunc(key)` requires type arguments, resolves
  them, calls `instantiate_fn`, and continues as the instance
  (`is_async` check, arguments, `AsyncCallee::Function(instance)`).
  The member arm needs no change: an instantiated generic class is
  a `Type::Class`.
- `compiler/src/check/mod.rs` `instantiate_fn`: passes `exported =
  false` to `check_function` (rule 5). The module export set for
  imports is unchanged.
- `compiler/src/language_reference.rs`: the Q34 prose drops
  "non-generic" and names the two new forms; the corpus list
  replaces r104 with a143 and r140; `generated-docs/` regenerates.
- No change in `codegen/`: the instance is an ordinary async
  function or method in HIR. Rule 5 removes the instance from the
  export and root sets through the `exported` flag.

### 64.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: the two S100 in 64 items 1 and 2,
recorded (this host, exit 1).

1. `corpus/accept/a143-async-generic.ts` + `.expected`: `class
   Box<T>` with a field and `async read(): Promise<T>` that awaits
   `Context.suspend()` and returns the field; `async function
   first<T>(items: T[]): Promise<T>`; `export async function
   tick<T>(): Promise<void>` that prints once; both `Box` and
   `first` instantiated with `u32` and with a `@CStruct` value class
   (`Vec2`, two `f32` fields); `main` awaits each and prints the
   results through field reads. The golden shows the `tick` print
   once (rule 5). Golden from the dev JIT; ship byte-identical.
2. `corpus/reject/r140-async-lambda.ts`: an async arrow function in
   an async function body; S100 at the arrow; `tsc-clean-standalone`
   recorded in the header, as r100.
3. `corpus/reject/r104-async-generic-class-method.ts` is deleted;
   the harness row is removed.
4. Unit tests in the same commit: a floating `first<u32>(items)`
   call is S013; `await first(items)` without type arguments is
   S100; an async method on a generic `@CStruct` value class is the
   r103 S100 at instantiation; the HIR of a program with an
   exported generic async function has `exported == false` on the
   instance and the emitted C defines no `subscript_export_` symbol
   for it.
5. Counts: accept `.ts` 141 → 142; `.expected` 142 → 143; rejects
   135 → 135 (one removed, one added). The generated docs
   regenerate.
6. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden and `.expected` byte-identical.
