<!-- §58 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 58. R29 — a class index signature is accessor sugar

Origin: downstream request R29, 2026-08-15, at pin `dfef090`. The
downstream authors GPU kernels in subscript and compiles the typed
HIR to WGSL. A kernel body is index-dense: `out[i] = f(a[i], b[i])`
is the common case. The checker rejects `a[i]` on the downstream's
generic wrapper classes, so their authors must write `get`/`set`
calls.

Measurements at the pin, on this host:

1. The report reproduces: `a[i]` on a generic class fails with
   S100 "type `...` is not indexable". `check_index`
   (`compiler/src/check/expr.rs`) indexes `Type::Array` and
   `Type::FixedArray` alone.
2. The handoff sketches ambient wrappers
   (`declare class StorageArray<T> { ... }`). That form does not
   check at the pin through any route. Mirror ingestion rejects a
   generic `declare class` at the declaration ("mirror declaration
   form outside the decided surface", `collect_mirror_decl`,
   `compiler/src/check/mod.rs`). A non-mirror `declare class`
   rejects each body-less method ("function bodies are required").
   An arrow-typed field is not callable as a method.
3. The form that checks at the pin is the generic script class
   with method bodies. Its methods check, and an `unreachable()`
   body satisfies a generic return (measured, exit 0). The
   handoff's own error message comes from this form.
4. Stock `tsc` accepts an index signature on a class with a
   sized-alias index type — `readonly [index: u32]: T` and the
   mutable form — and accepts reads and writes through it beside
   named members (measured, exit 0).

Decision: the language reads index signatures on class
declarations and defines them as accessor sugar. The rewrite is
complete in the checker, so the tiers agree by construction and
the downstream's HIR consumer sees the method calls it already
handles.

### 58.1 Rule

1. A class declaration can declare at most one index signature:
   `[index: I]: T` or `readonly [index: I]: T`. `I` is `i32` or
   `u32`.
2. Reference classes only. An index signature on a value class
   (`@CStruct`) or a descriptor class fails with S100.
3. The class must declare a method `get(index: I): T`. If the
   signature is not `readonly`, the class must also declare
   `set(index: I, value: T): void`. The types must match the
   signature exactly. A missing or mismatched accessor fails with
   S100 at the signature.
4. A read `a[i]` checks to the same HIR as `a.get(i)`. A write
   `a[i] = v` in statement position checks to the same HIR as
   `a.set(i, v)`. No new HIR form exists, and no tier changes.
5. The index expression checks against `I` by ordinary
   assignability (S007 on a numeric mismatch).
6. If the signature is `readonly`, the write spelling fails with
   S100 at the assignment.
7. The write used as a value is rejected with S100 and a message
   that names the spelling. *(Amended 2026-09-02, §82.1: compound
   assignment, increment, and decrement on `a[i]` now rewrite to
   the read-then-write form.)*
8. The language adds no bounds rule. The accessor body owns the
   behavior at every index value.
9. Nothing else moves. Array and `FixedArray` indexing keep the
   `i32` index rule. Mirror ingestion keeps rejecting generic
   classes and does not read index signatures.

`collisions.md` gains C10: JS reads a numeric property through an
index signature; subscript calls the declared accessor.

### 58.2 Changes by site

- `compiler/src/check/expr.rs` (`check_index`): a `Type::Class`
  receiver whose class declares an index signature rewrites the
  read to the `get` call. The blanket `i32` index rule stays for
  arrays.
- `compiler/src/check/expr.rs` (`check_assign` and the update
  path): an index target on a signature class rewrites the plain
  write to the `set` call, and rejects the `readonly`, compound,
  increment, and value-position spellings.
- Class collection (`compiler/src/check/mod.rs`, class HIR): parse
  the index-signature member, store index type, element type, and
  the `readonly` flag, and validate the accessors at the
  declaration.
- Mirror ingestion: unchanged. A unit test pins that an
  index-signature member in a mirror class keeps failing.
- No codegen, runtime, or prelude change.

### 58.3 Corpus and gate (pre-registered exit criteria)

Red first, at the contract pin: `a136` fails at the signature
member and at the index spellings; `r128`–`r130` fail for a
different reason than their registered code. Record the
diagnostics before the implementation.

1. `corpus/accept/a136-index-signature.ts` + `.expected` (golden
   from the dev JIT; ship byte-identical): a generic read-only
   wrapper and a generic mutable wrapper over a `data: T[]` field;
   writes through `m[i] = v`; reads through `m[i]`; one line pins
   `m[0] === m.get(0)` as `true`; two element types pin
   monomorphization.
2. `corpus/reject/r128-readonly-index-write.ts`: S100 at the
   write.
3. `corpus/reject/r129-index-signature-no-get.ts`: S100 at the
   declaration.
4. `corpus/reject/r130-index-compound-assign.ts`: S100 at the
   compound write.
5. Unit tests in the same commit: a wrong-typed index argument
   fails with S007; a value-class signature fails; a
   value-position write fails; a mirror index-signature member
   fails; the checker produces identical HIR for the sugar and the
   spelled calls.
6. Counts: accept single files 134 → 135; accept source files
   136 → 137; rejects 123 → 126; goldens 135 → 136. The generated
   docs regenerate byte-exact.
7. Gates: `cargo test --offline --workspace` in both profiles;
   zero-warning build; `cargo fmt --check`; the `tsc` gate; every
   pre-existing golden and `.expected` byte-identical.
